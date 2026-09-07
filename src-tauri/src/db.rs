use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, Transaction};

use crate::error::CommandError;
use crate::models::{AppSettings, TimerMode, TimerSnapshot};

/// Bump this whenever a new migration is appended to `MIGRATIONS`.
pub const LATEST_SCHEMA_VERSION: u32 = 5;

fn unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
    id              TEXT PRIMARY KEY,
    title           TEXT    NOT NULL,
    done            INTEGER NOT NULL DEFAULT 0,
    pomodoro_target INTEGER NOT NULL DEFAULT 1,
    priority        TEXT    NOT NULL DEFAULT 'med',
    project         TEXT    NOT NULL DEFAULT '通用',
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    completed_at    INTEGER
);
CREATE INDEX IF NOT EXISTS idx_tasks_sort_order ON tasks(sort_order);

CREATE TABLE IF NOT EXISTS timer_state (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    mode                TEXT    NOT NULL,
    state               TEXT    NOT NULL,
    active_session_id   TEXT,
    selected_task_id    TEXT,
    task_title_snapshot TEXT,
    project_snapshot    TEXT,
    duration_seconds    INTEGER NOT NULL CHECK (duration_seconds > 0),
    remaining_seconds   INTEGER NOT NULL CHECK (remaining_seconds >= 0),
    started_at          INTEGER,
    target_end_at       INTEGER,
    paused_at           INTEGER,
    revision            INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    updated_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id                  TEXT PRIMARY KEY,
    task_id             TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    task_title_snapshot TEXT    NOT NULL,
    project_snapshot    TEXT    NOT NULL,
    mode                TEXT    NOT NULL,
    status              TEXT    NOT NULL,
    planned_seconds     INTEGER NOT NULL,
    focused_seconds     INTEGER NOT NULL,
    started_at          INTEGER NOT NULL,
    ended_at            INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_started_at ON sessions(started_at);
CREATE INDEX IF NOT EXISTS idx_sessions_task_id ON sessions(task_id);

CREATE TABLE IF NOT EXISTS settings (
    id                     INTEGER PRIMARY KEY CHECK (id = 1),
    focus_duration_minutes INTEGER NOT NULL,
    short_break_minutes    INTEGER NOT NULL,
    long_break_minutes     INTEGER NOT NULL,
    auto_start_break       INTEGER NOT NULL,
    sound_enabled          INTEGER NOT NULL,
    notification_enabled   INTEGER NOT NULL,
    daily_goal             INTEGER NOT NULL,
    updated_at             INTEGER NOT NULL
);
"#;

/// v2 (Item 4 Round 4, R1-03): in-app "reduce motion" switch. Added as a
/// column migration so existing v1 databases upgrade in place, preserving all
/// tasks/sessions/settings.
const MIGRATION_V2: &str =
    "ALTER TABLE settings ADD COLUMN reduce_motion INTEGER NOT NULL DEFAULT 0;";

/// Runs the v2 → v3 migration ("local experience prerequisites", v1.1):
///
/// 1. creates `tags` with a permanent fallback tag and seeds the four system
///    tags (学习/工作/生活/其他 — stable ids);
/// 2. rebuilds `tasks` and `sessions` with tag columns and explicit finish
///    reason / qualification fields (old tables are renamed FIRST so foreign
///    keys never dangle; the old shapes are dropped only after the copy is
///    verified row-for-row);
/// 3. extends `timer_state` with the frozen tag snapshot columns;
/// 4. backfills everything to the fallback tag — never guessing from project
///    strings;
/// 5. verifies `PRAGMA foreign_key_check` is clean before committing.
///
/// Runs entirely inside the caller's transaction; any failure rolls back and
/// the pre-migration file (already copied aside by `open_at`) stays intact.
fn run_v3_migration(tx: &Transaction) -> Result<(), CommandError> {
    let now = unix_millis();

    // 1) Tags: single primary tag per task, exactly one permanent fallback.
    tx.execute_batch(
        "CREATE TABLE tags (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            normalized_name TEXT NOT NULL UNIQUE,
            kind            TEXT NOT NULL CHECK (kind IN ('system','custom')),
            is_fallback     INTEGER NOT NULL CHECK (is_fallback IN (0,1)),
            sort_order      INTEGER NOT NULL CHECK (sort_order >= 0),
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX idx_tags_single_fallback ON tags(is_fallback) WHERE is_fallback = 1;",
    )?;
    for (id, name, sort, fallback) in [
        ("system-study", "学习", 0i64, 0i64),
        ("system-work", "工作", 1, 0),
        ("system-life", "生活", 2, 0),
        ("system-other", "其他", 3, 1),
    ] {
        tx.execute(
            "INSERT INTO tags (id, name, normalized_name, kind, is_fallback, sort_order,
                               created_at, updated_at)
             VALUES (?1, ?2, ?2, 'system', ?3, ?4, ?5, ?5)",
            params![id, name, fallback, sort, now],
        )?;
    }

    // 2) Rename old shapes first: the rename rewrites the old sessions' FK
    //    references to `tasks_v2_old`, so dropping the old tables later can
    //    never violate a constraint and foreign_keys stays ON throughout.
    tx.execute_batch(
        "ALTER TABLE tasks RENAME TO tasks_v2_old;
         ALTER TABLE sessions RENAME TO sessions_v2_old;",
    )?;

    // 3) v3 tasks: one non-null primary tag (RESTRICT), defaulted to fallback.
    tx.execute_batch(
        "CREATE TABLE tasks (
            id              TEXT PRIMARY KEY,
            title           TEXT NOT NULL,
            done            INTEGER NOT NULL DEFAULT 0,
            pomodoro_target INTEGER NOT NULL DEFAULT 1,
            priority        TEXT NOT NULL DEFAULT 'med',
            project         TEXT NOT NULL DEFAULT '通用',
            tag_id          TEXT NOT NULL REFERENCES tags(id) ON DELETE RESTRICT,
            sort_order      INTEGER NOT NULL DEFAULT 0,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            completed_at    INTEGER
        );",
    )?;
    tx.execute(
        "INSERT INTO tasks (id, title, done, pomodoro_target, priority, project, tag_id,
                            sort_order, created_at, updated_at, completed_at)
         SELECT id, title, done, pomodoro_target, priority, project,
                (SELECT id FROM tags WHERE is_fallback = 1),
                sort_order, created_at, updated_at, completed_at
         FROM tasks_v2_old",
        [],
    )?;

    // 4) v3 sessions: tag snapshots + explicit finish reason / qualification.
    tx.execute_batch(
        "CREATE TABLE sessions (
            id                  TEXT PRIMARY KEY,
            task_id             TEXT REFERENCES tasks(id) ON DELETE SET NULL,
            task_title_snapshot TEXT NOT NULL,
            project_snapshot    TEXT NOT NULL,
            tag_id              TEXT REFERENCES tags(id) ON DELETE SET NULL,
            tag_name_snapshot   TEXT NOT NULL,
            mode                TEXT NOT NULL,
            status              TEXT NOT NULL,
            planned_seconds     INTEGER NOT NULL,
            focused_seconds     INTEGER NOT NULL CHECK (focused_seconds >= 0),
            started_at          INTEGER NOT NULL,
            ended_at            INTEGER NOT NULL,
            finish_reason       TEXT NOT NULL CHECK (finish_reason IN
                                  ('elapsed','manual_finish','reset','mode_change','legacy')),
            statistics_eligible INTEGER NOT NULL CHECK (statistics_eligible IN (0,1)),
            qualification_reason TEXT NOT NULL CHECK (qualification_reason IN
                                  ('qualified','too_short','abandoned','non_focus','legacy'))
        );",
    )?;
    tx.execute(
        "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot,
                               tag_id, tag_name_snapshot, mode, status,
                               planned_seconds, focused_seconds, started_at, ended_at,
                               finish_reason, statistics_eligible, qualification_reason)
         SELECT id, task_id, task_title_snapshot, project_snapshot,
                (SELECT id FROM tags WHERE is_fallback = 1),
                (SELECT name FROM tags WHERE is_fallback = 1),
                mode, status, planned_seconds, focused_seconds, started_at, ended_at,
                'legacy',
                CASE WHEN mode = 'focus' AND status = 'completed' AND focused_seconds >= 30
                     THEN 1 ELSE 0 END,
                CASE
                    WHEN mode = 'focus' AND status = 'completed' AND focused_seconds >= 30
                        THEN 'qualified'
                    WHEN mode = 'focus' AND focused_seconds < 30 THEN 'too_short'
                    WHEN status = 'abandoned' THEN 'abandoned'
                    ELSE 'non_focus'
                END
         FROM sessions_v2_old",
        [],
    )?;

    // 5) Verify the copy preserved every row before dropping the originals.
    let old_tasks: i64 = tx.query_row("SELECT COUNT(*) FROM tasks_v2_old", [], |r| r.get(0))?;
    let new_tasks: i64 = tx.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))?;
    let old_sessions: i64 =
        tx.query_row("SELECT COUNT(*) FROM sessions_v2_old", [], |r| r.get(0))?;
    let new_sessions: i64 = tx.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
    if old_tasks != new_tasks || old_sessions != new_sessions {
        return Err(CommandError::database(format!(
            "v3 migration row count mismatch: tasks {old_tasks}->{new_tasks}, sessions {old_sessions}->{new_sessions}"
        )));
    }

    // 6) Drop old shapes (child first), then recreate indexes on final names.
    tx.execute_batch("DROP TABLE sessions_v2_old; DROP TABLE tasks_v2_old;")?;
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_tasks_sort_order ON tasks(sort_order);
         CREATE INDEX IF NOT EXISTS idx_tasks_tag_sort ON tasks(tag_id, sort_order);
         CREATE INDEX IF NOT EXISTS idx_sessions_started_at ON sessions(started_at);
         CREATE INDEX IF NOT EXISTS idx_sessions_task_id ON sessions(task_id);
         CREATE INDEX IF NOT EXISTS idx_sessions_tag_started ON sessions(tag_id, started_at);
         CREATE INDEX IF NOT EXISTS idx_sessions_qualification
             ON sessions(mode, statistics_eligible, started_at);",
    )?;

    // 7) timer_state gains the frozen tag snapshot columns (nullable ALTER).
    tx.execute_batch(
        "ALTER TABLE timer_state ADD COLUMN tag_id TEXT REFERENCES tags(id) ON DELETE SET NULL;
         ALTER TABLE timer_state ADD COLUMN tag_name_snapshot TEXT;",
    )?;

    // 8) No dangling references may survive the upgrade.
    let violations: usize = tx
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |row| row.get::<_, String>(0))?
        .count();
    if violations > 0 {
        return Err(CommandError::database(format!(
            "foreign_key_check reported {violations} violating rows after the v3 migration"
        )));
    }
    Ok(())
}

/// 1.3 conservation: per-child-table audit that the rebuild preserved every
/// task link — not just how many. Row counts, non-NULL counts and time sums
/// CANNOT catch a swap of task ids between two rows; a bidirectional EXCEPT
/// on (id, task_id) can.
fn assert_child_task_links_preserved(
    tx: &Transaction<'_>,
    old_table: &str,
    new_table: &str,
) -> Result<(), CommandError> {
    let scalar = |sql: &str| -> Result<i64, CommandError> {
        tx.query_row(sql, [], |row| row.get(0))
            .map_err(|err| CommandError::database(format!("v5 conservation query failed: {err}")))
    };
    // Row-level link conservation (Codex 复审阻断 3): COUNT-based checks only
    // prove the NUMBER of links survived — two rows could swap task_id and
    // still pass. A symmetric (id, task_id) EXCEPT proves every row points at
    // the SAME task, in both directions.
    let old_minus_new = scalar(&format!(
        "SELECT COUNT(*) FROM (SELECT id, task_id FROM {old_table} EXCEPT SELECT id, task_id FROM {new_table})"
    ))?;
    if old_minus_new != 0 {
        return Err(CommandError::database(format!(
            "v5 migration conservation mismatch ({old_table} links changed on rebuild): {old_minus_new} row(s) lost their original task_id"
        )));
    }
    let new_minus_old = scalar(&format!(
        "SELECT COUNT(*) FROM (SELECT id, task_id FROM {new_table} EXCEPT SELECT id, task_id FROM {old_table})"
    ))?;
    if new_minus_old != 0 {
        return Err(CommandError::database(format!(
            "v5 migration conservation mismatch ({old_table} links changed on rebuild): {new_minus_old} row(s) point at a different task_id"
        )));
    }
    Ok(())
}

/// Runs the v4 → v5 migration ("management glass", optional task metadata):
///
/// 1. rebuilds `tasks` so `tag_id` and `priority` become NULL-able (v4 had
///    them NOT NULL — `priority TEXT NOT NULL DEFAULT 'med'` and
///    `tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE RESTRICT` — and
///    SQLite cannot ALTER an existing column constraint away; the v2→v3
///    rename-first rebuild is the precedent);
/// 2. rebuilds `sessions`, `budget_history` and `focus_segments` in the same
///    transaction ONLY to re-point their `task_id` foreign keys at the fresh
///    table (values are copied verbatim — no rewrite, no recompute);
/// 3. preserves every historical value: a task tagged 「其他」 with priority
///    「中」 stays exactly that — NULL-ability only affects FUTURE writes
///    (title-only creation), never a mass clear of existing data;
/// 4. audits conservation BEFORE dropping the old shapes (1.3): per-table row
///    counts, total invested time (sessions focused_seconds, focus_segments
///    effective_ms), per-child task-link counts, a symmetric EXCEPT diff of
///    (id, tag_id, priority) on tasks, and the timer_state binding. Any
///    mismatch fails the whole transaction;
/// 5. verifies `PRAGMA foreign_key_check` is clean before committing.
///
/// `timer_state` needs no rebuild: `selected_task_id` is a bare TEXT column
/// with no FK (its binding survives because task ids are preserved verbatim)
/// and `tag_id` references `tags`, which this migration does not touch.
fn run_v5_migration(tx: &Transaction<'_>) -> Result<(), CommandError> {
    // 0) timer_state is never rebuilt, but its binding must survive the
    //    rebuild VERBATIM (Codex 复审阻断 3): snapshot the values up front and
    //    compare after the old shapes are gone — a non-tautological
    //    before/after proof instead of a self-join that always passes.
    let timer_binding_before: Vec<Option<String>> = {
        let mut stmt = tx.prepare("SELECT selected_task_id FROM timer_state ORDER BY id")?;
        let rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    // 1) Rename all four shapes first: the rename rewrites child FKs to the
    //    `_v4_old` names, so the fresh tables define their FKs against the
    //    final `tasks` name and the old rows can be dropped without ever
    //    violating a constraint. `foreign_keys` stays ON throughout (v3
    //    precedent, run_v3_migration step 2).
    tx.execute_batch(
        "ALTER TABLE tasks RENAME TO tasks_v4_old;
         ALTER TABLE sessions RENAME TO sessions_v4_old;
         ALTER TABLE budget_history RENAME TO budget_history_v4_old;
         ALTER TABLE focus_segments RENAME TO focus_segments_v4_old;",
    )?;

    // 2) v5 tasks: `tag_id` / `priority` become NULL-able (defaults dropped —
    //    "unspecified" must be NULL, not a silent 'med'). Everything else
    //    keeps the exact v4 shape.
    tx.execute_batch(
        "CREATE TABLE tasks (
            id              TEXT PRIMARY KEY,
            title           TEXT NOT NULL,
            done            INTEGER NOT NULL DEFAULT 0,
            pomodoro_target INTEGER NOT NULL DEFAULT 1,
            priority        TEXT,
            project         TEXT NOT NULL DEFAULT '通用',
            tag_id          TEXT REFERENCES tags(id) ON DELETE RESTRICT,
            sort_order      INTEGER NOT NULL DEFAULT 0,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            completed_at    INTEGER,
            profile_id      TEXT NOT NULL DEFAULT 'local',
            project_id      TEXT REFERENCES projects(id),
            target_seconds  INTEGER NOT NULL DEFAULT 0,
            budget_source   TEXT NOT NULL DEFAULT 'migration',
            status          TEXT NOT NULL DEFAULT 'todo',
            deadline        TEXT,
            notes           TEXT NOT NULL DEFAULT '',
            relationship_revision INTEGER NOT NULL DEFAULT 0
        );",
    )?;

    // 3) Child tables rebuilt with IDENTICAL shapes — only their FK targets
    //    move to the fresh tasks table. Column lists mirror the v3/v4
    //    definitions exactly (sessions: v3 shape + v4 profile_id ALTER).
    tx.execute_batch(
        "CREATE TABLE sessions (
            id                  TEXT PRIMARY KEY,
            task_id             TEXT REFERENCES tasks(id) ON DELETE SET NULL,
            task_title_snapshot TEXT NOT NULL,
            project_snapshot    TEXT NOT NULL,
            tag_id              TEXT REFERENCES tags(id) ON DELETE SET NULL,
            tag_name_snapshot   TEXT NOT NULL,
            mode                TEXT NOT NULL,
            status              TEXT NOT NULL,
            planned_seconds     INTEGER NOT NULL,
            focused_seconds     INTEGER NOT NULL CHECK (focused_seconds >= 0),
            started_at          INTEGER NOT NULL,
            ended_at            INTEGER NOT NULL,
            finish_reason       TEXT NOT NULL CHECK (finish_reason IN
                                  ('elapsed','manual_finish','reset','mode_change','legacy')),
            statistics_eligible INTEGER NOT NULL CHECK (statistics_eligible IN (0,1)),
            qualification_reason TEXT NOT NULL CHECK (qualification_reason IN
                                  ('qualified','too_short','abandoned','non_focus','legacy')),
            profile_id          TEXT NOT NULL DEFAULT 'local'
        );

        CREATE TABLE budget_history (
            id                 TEXT PRIMARY KEY,
            task_id            TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            old_target_seconds INTEGER,
            new_target_seconds INTEGER NOT NULL,
            source             TEXT NOT NULL CHECK (source IN ('creation','migration','recalc')),
            created_at         INTEGER NOT NULL
        );

        CREATE TABLE focus_segments (
            id                    TEXT PRIMARY KEY,
            profile_id            TEXT NOT NULL REFERENCES profiles(id),
            session_id            TEXT NOT NULL,
            task_id               TEXT REFERENCES tasks(id) ON DELETE SET NULL,
            project_id_snapshot   TEXT,
            project_name_snapshot TEXT NOT NULL,
            category_id_snapshot  TEXT,
            category_name_snapshot TEXT NOT NULL DEFAULT '',
            effective_start_ms    INTEGER NOT NULL,
            effective_end_ms      INTEGER NOT NULL,
            effective_ms          INTEGER NOT NULL CHECK (effective_ms >= 0),
            state                 TEXT NOT NULL DEFAULT 'pending'
                                  CHECK (state IN ('pending','confirmed','void')),
            temporal_precision    TEXT NOT NULL DEFAULT 'effective_interval'
                                  CHECK (temporal_precision IN ('effective_interval','legacy_total_only')),
            created_at            INTEGER NOT NULL
        );",
    )?;

    // 4) Copy every row verbatim — no value rewrites, no recomputation (R2).
    tx.execute_batch(
        "INSERT INTO tasks (id, title, done, pomodoro_target, priority, project, tag_id,
                            sort_order, created_at, updated_at, completed_at, profile_id,
                            project_id, target_seconds, budget_source, status, deadline, notes,
                            relationship_revision)
         SELECT id, title, done, pomodoro_target, priority, project, tag_id,
                sort_order, created_at, updated_at, completed_at, profile_id,
                project_id, target_seconds, budget_source, status, deadline, notes,
                0
         FROM tasks_v4_old;

        INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                              tag_name_snapshot, mode, status, planned_seconds,
                              focused_seconds, started_at, ended_at, finish_reason,
                              statistics_eligible, qualification_reason, profile_id)
         SELECT id, task_id, task_title_snapshot, project_snapshot, tag_id,
                tag_name_snapshot, mode, status, planned_seconds,
                focused_seconds, started_at, ended_at, finish_reason,
                statistics_eligible, qualification_reason, profile_id
         FROM sessions_v4_old;

        INSERT INTO budget_history (id, task_id, old_target_seconds, new_target_seconds,
                                    source, created_at)
         SELECT id, task_id, old_target_seconds, new_target_seconds, source, created_at
         FROM budget_history_v4_old;

        INSERT INTO focus_segments (id, profile_id, session_id, task_id, project_id_snapshot,
                                    project_name_snapshot, category_id_snapshot,
                                    category_name_snapshot, effective_start_ms,
                                    effective_end_ms, effective_ms, state,
                                    temporal_precision, created_at)
         SELECT id, profile_id, session_id, task_id, project_id_snapshot,
                project_name_snapshot, category_id_snapshot,
                category_name_snapshot, effective_start_ms,
                effective_end_ms, effective_ms, state,
                temporal_precision, created_at
         FROM focus_segments_v4_old;",
    )?;

    // 5) Conservation audit (1.3): `foreign_key_check` passing is NOT data
    //    preservation — a CASCADE could have silently deleted history. Every
    //    mismatch rolls the whole transaction back.
    let scalar = |sql: &str| -> Result<i64, CommandError> {
        tx.query_row(sql, [], |row| row.get(0))
            .map_err(|err| CommandError::database(format!("v5 conservation query failed: {err}")))
    };
    let conservation = |label: &str, old: i64, new: i64| -> Result<(), CommandError> {
        if old != new {
            return Err(CommandError::database(format!(
                "v5 migration conservation mismatch ({label}): {old} -> {new}"
            )));
        }
        Ok(())
    };

    conservation("tasks.row_count", scalar("SELECT COUNT(*) FROM tasks_v4_old")?, scalar("SELECT COUNT(*) FROM tasks")?)?;
    conservation("sessions.row_count", scalar("SELECT COUNT(*) FROM sessions_v4_old")?, scalar("SELECT COUNT(*) FROM sessions")?)?;
    conservation("budget_history.row_count", scalar("SELECT COUNT(*) FROM budget_history_v4_old")?, scalar("SELECT COUNT(*) FROM budget_history")?)?;
    conservation("focus_segments.row_count", scalar("SELECT COUNT(*) FROM focus_segments_v4_old")?, scalar("SELECT COUNT(*) FROM focus_segments")?)?;
    conservation(
        "sessions.focused_seconds_total",
        scalar("SELECT COALESCE(SUM(focused_seconds), 0) FROM sessions_v4_old")?,
        scalar("SELECT COALESCE(SUM(focused_seconds), 0) FROM sessions")?,
    )?;
    conservation(
        "focus_segments.effective_ms_total",
        scalar("SELECT COALESCE(SUM(effective_ms), 0) FROM focus_segments_v4_old")?,
        scalar("SELECT COALESCE(SUM(effective_ms), 0) FROM focus_segments")?,
    )?;
    // Historical metadata values must survive VERBATIM (「历史『其他』『中』不被
    // 批量清空」): a symmetric EXCEPT diff over (id, tag_id, priority) must be
    // empty in BOTH directions.
    conservation(
        "tasks.old_minus_new(id,tag_id,priority)",
        scalar("SELECT COUNT(*) FROM (SELECT id, tag_id, priority FROM tasks_v4_old EXCEPT SELECT id, tag_id, priority FROM tasks)")?,
        0,
    )?;
    conservation(
        "tasks.new_minus_old(id,tag_id,priority)",
        scalar("SELECT COUNT(*) FROM (SELECT id, tag_id, priority FROM tasks EXCEPT SELECT id, tag_id, priority FROM tasks_v4_old)")?,
        0,
    )?;
    // Per-child link conservation (Codex 复审阻断 3): COUNT-based checks only
    // prove the NUMBER of links — two rows could swap task_id and pass. The
    // (id, task_id) pairs themselves are audited symmetrically for all three
    // child tables; budget_history is NOT covered by its row count (its
    // task_id is NOT NULL, but a swap preserves counts all the same).
    assert_child_task_links_preserved(tx, "sessions_v4_old", "sessions")?;
    assert_child_task_links_preserved(tx, "budget_history_v4_old", "budget_history")?;
    assert_child_task_links_preserved(tx, "focus_segments_v4_old", "focus_segments")?;
    // timer_state binding (bare TEXT, no FK): the dangling-reference count is
    // checked against BOTH the old and the new tasks shapes — a non-trivial
    // proof that no selected task id was lost by the rebuild. (A plain link
    // count against itself would be tautological: timer_state is not rebuilt.)
    conservation(
        "timer_state.dangling_selected_task",
        scalar("SELECT COUNT(*) FROM timer_state WHERE selected_task_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM tasks_v4_old WHERE id = timer_state.selected_task_id)")?,
        scalar("SELECT COUNT(*) FROM timer_state WHERE selected_task_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM tasks WHERE id = timer_state.selected_task_id)")?,
    )?;

    // 6) Drop old shapes (children first), then recreate every index whose
    //    parent table was rebuilt (dropping a table drops its indexes).
    tx.execute_batch(
        "DROP TABLE focus_segments_v4_old;
         DROP TABLE budget_history_v4_old;
         DROP TABLE sessions_v4_old;
         DROP TABLE tasks_v4_old;
         CREATE INDEX IF NOT EXISTS idx_tasks_sort_order ON tasks(sort_order);
         CREATE INDEX IF NOT EXISTS idx_tasks_tag_sort ON tasks(tag_id, sort_order);
         CREATE INDEX IF NOT EXISTS idx_tasks_project ON tasks(project_id);
         CREATE INDEX IF NOT EXISTS idx_sessions_started_at ON sessions(started_at);
         CREATE INDEX IF NOT EXISTS idx_sessions_task_id ON sessions(task_id);
         CREATE INDEX IF NOT EXISTS idx_sessions_tag_started ON sessions(tag_id, started_at);
         CREATE INDEX IF NOT EXISTS idx_sessions_qualification
             ON sessions(mode, statistics_eligible, started_at);
         CREATE INDEX IF NOT EXISTS idx_budget_history_task ON budget_history(task_id);
         CREATE INDEX IF NOT EXISTS idx_segments_session ON focus_segments(session_id);
         CREATE INDEX IF NOT EXISTS idx_segments_task_state ON focus_segments(task_id, state);",
    )?;

    // 7) No dangling references may survive the upgrade.
    let violations: usize = tx
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |row| row.get::<_, String>(0))?
        .count();
    if violations > 0 {
        return Err(CommandError::database(format!(
            "foreign_key_check reported {violations} violating rows after the v5 migration"
        )));
    }

    // 8) timer_state binding survived VERBATIM (step 0 snapshot).
    let timer_binding_after: Vec<Option<String>> = {
        let mut stmt = tx.prepare("SELECT selected_task_id FROM timer_state ORDER BY id")?;
        let rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if timer_binding_before != timer_binding_after {
        return Err(CommandError::database(
            "v5 migration conservation mismatch (timer_state.selected_task_id): the binding changed across the rebuild",
        ));
    }
    Ok(())
}

/// Opens a migrated, seeded in-memory database. Used by tests.
pub fn open_in_memory() -> Result<Connection, CommandError> {
    let mut conn = Connection::open_in_memory()?;
    configure(&conn, false)?;
    run_migrations(&mut conn)?;
    seed_defaults(&conn)?;
    Ok(conn)
}

/// Opens (or creates) the on-disk database, creating parent directories as needed.
///
/// Defensive against a corrupt on-disk database (P0: the app must still start):
/// if the file cannot be opened or fails `PRAGMA integrity_check`, it is renamed
/// aside with a `.corrupt-<timestamp>` suffix (preserved for manual recovery)
/// and a fresh database is created in its place. The corrupt file is never
/// silently discarded.
///
/// **Migration failure is NOT corruption** (v1.1 review #2): if opening and the
/// integrity check succeed but a schema migration fails, the transaction rolls
/// back, the original file is preserved untouched, and startup aborts with a
/// diagnosable error — no rename, no fresh empty database. Before any pending
/// migration runs, a timestamped backup of the database is created.
pub fn open_at(path: &Path) -> Result<Connection, CommandError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|err| {
                CommandError::internal(format!(
                    "failed to create database directory {}: {err}",
                    parent.display()
                ))
            })?;
        }
    }

    // Phase 0 — v1.1.2 E1 + P112-06.6: refuse a database written by a NEWER
    // version of the app BEFORE anything touches the source. The check runs
    // against a THROWAWAY COPY of the {db, -wal, -shm} set, so even a WAL-mode
    // database is evaluated with its committed pages (never just the main
    // file header), and the source is not opened for writing at all. A probe
    // failure (garbage/corrupt file) falls through to the normal corruption
    // handling in Phase 1/2.
    if path.exists() {
        match probe_schema_version(path) {
            Ok(Some(version)) if version > LATEST_SCHEMA_VERSION => {
                return Err(CommandError::database_too_new(version, LATEST_SCHEMA_VERSION));
            }
            Ok(_) => {}
            Err(_) => {} // unreadable copy: Phase 1/2 produce the real diagnosis
        }
    }

    // Phase 1 — open + configure. P112-06.3: permission problems, lock
    // contention and I/O errors must NOT be treated as corruption and must
    // never rename the user's file aside. Only a file that OPENS but fails
    // the explicit corruption diagnosis goes through recovery.
    // Phase 1 - open + configure. P112-06.3: permission problems, lock
    // contention and I/O errors must NOT be treated as corruption and must
    // never rename the user's file aside. Only an explicit SQLite corruption
    // diagnosis (NOTADB / CORRUPT) opens the recovery path.
    let mut conn = match open_and_configure(path) {
        Ok(conn) => conn,
        Err(err) if is_corruption_diagnosis(&err) => {
            recover_corrupt(path)?;
            return fresh_database(path);
        }
        Err(err) => {
            return Err(CommandError::database(format!(
                "cannot open database at {} (locked, permission denied, or I/O error); the file was left untouched: {err}",
                path.display()
            )));
        }
    };

    // Phase 2 — integrity. A corrupt file → isolate + rebuild.
    if !is_healthy(&conn) {
        drop(conn);
        recover_corrupt(path)?;
        return fresh_database(path);
    }

    // Phase 3 — a pending upgrade → timestamped safety copy first.
    let version = schema_version(&conn)?;
    if version > 0 && version < LATEST_SCHEMA_VERSION {
        backup_before_migration(&conn, path)?;
    }

    // Phase 4 — migrate + seed. Failure here is NOT corruption: the
    // transaction has already rolled back, the original file stays untouched,
    // and startup must abort with a diagnosable error.
    if let Err(err) = migrate_and_seed(&mut conn) {
        return Err(CommandError::database(format!(
            "schema migration failed; the original database was preserved at {} \
             and no changes were applied: {err}",
            path.display()
        )));
    }

    Ok(conn)
}

/// Opens and configures a connection without migrating or seeding.
fn open_and_configure(path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;
    configure(&conn, true)?;
    Ok(conn)
}

/// R06: opens an OLD-schema database WITHOUT migrating it — preparation mode.
/// The connection may only serve preview/confirm/cancel until the user
/// decides; all business commands are gated on `AppState.migration_pending`.
pub fn open_prepared(path: &Path) -> Result<Connection, CommandError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|err| {
                CommandError::internal(format!(
                    "failed to create database directory {}: {err}",
                    parent.display()
                ))
            })?;
        }
    }
    let conn = open_and_configure(path).map_err(CommandError::from)?;
    Ok(conn)
}

/// Runs migrations and seeds defaults on an already-configured connection.
fn migrate_and_seed(conn: &mut Connection) -> Result<(), CommandError> {
    run_migrations(conn)?;
    seed_defaults(conn)?;
    Ok(())
}

/// Recovery path for corrupt media: create a fresh, fully migrated and seeded
/// database at `path` (the corrupt file has already been renamed aside).
fn fresh_database(path: &Path) -> Result<Connection, CommandError> {
    let mut conn = open_and_configure(path).map_err(CommandError::from)?;
    migrate_and_seed(&mut conn)?;
    Ok(conn)
}

/// Creates a VERIFIED pre-migration backup (P112-06.5/7):
/// 1. checkpoint(TRUNCATE) — its busy result is CHECKED; a busy checkpoint
///    means readers/writers are active and the copy would be inconsistent.
/// 2. copy the (now complete) main file to a fresh `.pre-v<N>-<ts>.bak` path.
/// 3. verify the copy by opening it read-only: integrity ok + schema version
///    matches the source. Any failure aborts the migration — the original
///    database stays untouched.
fn backup_before_migration(conn: &Connection, path: &Path) -> Result<(), CommandError> {
    let checkpoint: (i64, i64, i64) = conn.query_row(
        "PRAGMA wal_checkpoint(TRUNCATE)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if checkpoint.0 != 0 {
        return Err(CommandError::database(format!(
            "pre-migration checkpoint is busy ({checkpoint:?}); close other readers/writers              and retry — refusing to back up a partially-flushed database"
        )));
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if base.is_empty() {
        return Err(CommandError::internal("database path has no file name"));
    }
    let backup = path.with_file_name(format!("{base}.pre-v{LATEST_SCHEMA_VERSION}-{ts}.bak"));
    std::fs::copy(path, &backup).map_err(|err| {
        CommandError::internal(format!(
            "failed to create pre-migration backup {}: {err}",
            backup.display()
        ))
    })?;
    // Verify the copy is a consistent, complete snapshot of the source. The
    // copy inherits the source's WAL header, so the verifier opens it read-
    // write ONLY to switch it to journal_mode=DELETE - making the snapshot
    // self-contained (no .bak-wal/.bak-shm sidecars) - then checks integrity
    // and the schema version. Any failure aborts the migration.
    let source_version = schema_version(conn)?;
    let verify = (|| {
        let vconn = Connection::open(&backup)?;
        let healthy: String = vconn.pragma_query_value(None, "integrity_check", |r| r.get(0))?;
        if !healthy.eq_ignore_ascii_case("ok") {
            return Err(CommandError::database(format!(
                "pre-migration backup failed integrity check: {healthy}"
            )));
        }
        vconn.pragma_update(None, "journal_mode", "DELETE")?;
        let version: u32 = vconn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version != source_version {
            return Err(CommandError::database(format!(
                "pre-migration backup version {version} != source {source_version}"
            )));
        }
        Ok(())
    })();
    match verify {
        Ok(()) => Ok(()),
        Err(err) => Err(CommandError::database(format!(
            "{err}; migration aborted, original database untouched"
        ))),
    }
}

/// True only for explicit SQLite corruption diagnoses — a locked, permission-
/// denied or I/O-failing database must never be renamed aside (P112-06.3).
fn is_corruption_diagnosis(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(ffi, _) => {
            ffi.extended_code == rusqlite::ffi::SQLITE_NOTADB
                || ffi.extended_code == rusqlite::ffi::SQLITE_CORRUPT
                || ffi.code == rusqlite::ErrorCode::DatabaseCorrupt
        }
        _ => false,
    }
}

/// A healthy database reports exactly "ok" from `PRAGMA integrity_check`.
fn is_healthy(conn: &Connection) -> bool {
    conn.pragma_query_value(None, "integrity_check", |row| row.get::<_, String>(0))
        .map(|v| v.eq_ignore_ascii_case("ok"))
        .unwrap_or(false)
}

/// Renames a possibly-corrupt database (and its WAL/SHM siblings) aside with a
/// `.corrupt-<timestamp>` suffix so the user can recover it manually later,
/// then lets a subsequent `try_open_at` create a fresh file at `path`. Rename
/// errors are ignored — if the file is locked we still attempt a fresh open.
fn recover_corrupt(path: &Path) -> Result<(), CommandError> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if base.is_empty() {
        return Ok(());
    }
    let suffix = format!(".corrupt-{ts}");
    for name in [base.clone(), format!("{base}-wal"), format!("{base}-shm")] {
        let candidate = path.with_file_name(&name);
        if candidate.exists() {
            let backup = path.with_file_name(format!("{name}{suffix}"));
            let _ = std::fs::rename(&candidate, &backup);
        }
    }
    Ok(())
}

/// Applies the startup pragmas from the design spec: `foreign_keys = ON`,
/// `busy_timeout = 5000`, plus `journal_mode = WAL` for file-backed databases
/// (in-memory databases cannot use WAL).
fn configure(conn: &Connection, persistent: bool) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    if persistent {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
    }
    Ok(())
}

pub fn schema_version(conn: &Connection) -> Result<u32, CommandError> {
    let version = conn.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?;
    Ok(version)
}

/// Applies any outstanding migrations. Safe to call on every launch.
pub fn run_migrations(conn: &mut Connection) -> Result<(), CommandError> {
    let current = schema_version(conn)?;
    if current >= LATEST_SCHEMA_VERSION {
        return Ok(());
    }

    if current < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V1)?;
        tx.pragma_update(None, "user_version", 1u32)?;
        tx.commit()?;
    }

    if current < 2 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V2)?;
        tx.pragma_update(None, "user_version", 2u32)?;
        tx.commit()?;
    }

    if current < 3 {
        let tx = conn.transaction()?;
        run_v3_migration(&tx)?;
        tx.pragma_update(None, "user_version", 3u32)?;
        tx.commit()?;
    }

    if current < 4 {
        let tx = conn.transaction()?;
        run_v4_migration(&tx)?;
        tx.pragma_update(None, "user_version", 4u32)?;
        tx.commit()?;
    }

    if current < 5 {
        // v5 is pure metadata NULL-ability: no user decisions, no recomputes
        // — the automatic path needs no preview parameters (R06 preview
        // dispatch is handled separately in task 1.4).
        let tx = conn.transaction()?;
        run_v5_migration(&tx)?;
        tx.pragma_update(None, "user_version", 5u32)?;
        tx.commit()?;
    }

    Ok(())
}

/// Local midnight of migration day is not needed — epoch millis only.
fn v4_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// v1.2 schema: 类别 → 项目 → 任务 + 时间片段账本 + 档案字段 + 任务预算快照.
///
/// - `profiles` starts with a single `local` guest profile (v1.4 activates
///   account profiles; every business row already carries `profile_id`).
/// - Default categories 学习/工作/生活/其他 are seeded; `其他` is the fallback.
/// - Legacy free-text `tasks.project` values are consolidated into real
///   projects (exact match after trimming, never fuzzy), all landing in the
///   `其他` category; `通用` tasks become standalone (project_id NULL).
/// - Task budgets are estimated as `pomodoro_target × 当前专注时长 × 60` with
///   `budget_source = 'migration'` and a `budget_history` row — an estimate,
///   never presented as exact history (roadmap §3.1).
/// - Every eligible historical session gets one `confirmed` focus segment
///   (effective time = focused_seconds; boundaries approximated from
///   started_at).
pub fn run_v4_migration(tx: &Transaction<'_>) -> Result<(), CommandError> {
    // Default parameters: current settings as the budget basis, 通用 stays
    // standalone. Used by tests and by open_at's auto path; the interactive
    // startup path (R06) passes explicit user-confirmed parameters.
    let params = crate::models::MigrationParams {
        budget_focus_minutes: 0, // 0 = read from settings
        general_mapping: "standalone".to_owned(),
    };
    run_v4_migration_with(tx, &params)
}

/// R06: the semantic migration honours the user's confirmed decisions —
/// budget basis minutes and the 通用 mapping. Nothing here runs before the
/// frontend has shown the preview and received an explicit confirmation.
pub fn run_v4_migration_with(tx: &Transaction<'_>, params: &crate::models::MigrationParams) -> Result<(), CommandError> {
    let now = v4_now();

    tx.execute_batch(
        r#"
        CREATE TABLE profiles (
            id         TEXT PRIMARY KEY,
            kind       TEXT NOT NULL CHECK (kind IN ('guest','account')),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE categories (
            id              TEXT PRIMARY KEY,
            profile_id      TEXT NOT NULL REFERENCES profiles(id),
            name            TEXT NOT NULL,
            normalized_name TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','archived')),
            sort_order      INTEGER NOT NULL DEFAULT 0,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            UNIQUE (profile_id, normalized_name)
        );

        CREATE TABLE projects (
            id              TEXT PRIMARY KEY,
            profile_id      TEXT NOT NULL REFERENCES profiles(id),
            category_id     TEXT NOT NULL REFERENCES categories(id),
            name            TEXT NOT NULL,
            normalized_name TEXT NOT NULL,
            description     TEXT NOT NULL DEFAULT '',
            status          TEXT NOT NULL DEFAULT 'active'
                            CHECK (status IN ('active','completed','archived')),
            plan_start_date TEXT,
            due_date        TEXT,
            sort_order      INTEGER NOT NULL DEFAULT 0,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            UNIQUE (profile_id, normalized_name)
        );
        CREATE INDEX idx_projects_category ON projects(category_id);

        CREATE TABLE budget_history (
            id                 TEXT PRIMARY KEY,
            task_id            TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            old_target_seconds INTEGER,
            new_target_seconds INTEGER NOT NULL,
            source             TEXT NOT NULL CHECK (source IN ('creation','migration','recalc')),
            created_at         INTEGER NOT NULL
        );
        CREATE INDEX idx_budget_history_task ON budget_history(task_id);

        CREATE TABLE focus_segments (
            id                    TEXT PRIMARY KEY,
            profile_id            TEXT NOT NULL REFERENCES profiles(id),
            session_id            TEXT NOT NULL,
            task_id               TEXT REFERENCES tasks(id) ON DELETE SET NULL,
            project_id_snapshot   TEXT,
            project_name_snapshot TEXT NOT NULL,
            category_id_snapshot  TEXT,
            category_name_snapshot TEXT NOT NULL DEFAULT '',
            effective_start_ms    INTEGER NOT NULL,
            effective_end_ms      INTEGER NOT NULL,
            effective_ms          INTEGER NOT NULL CHECK (effective_ms >= 0),
            state                 TEXT NOT NULL DEFAULT 'pending'
                                  CHECK (state IN ('pending','confirmed','void')),
            -- R08: 'effective_interval' = real run interval; 'legacy_total_only' =
            -- only the total is accurate (old sessions have no pause ledger), the
            -- start/end pair is metadata and must never be treated as a precise
            -- interval (no union maths, no cross-midnight splitting).
            temporal_precision    TEXT NOT NULL DEFAULT 'effective_interval'
                                  CHECK (temporal_precision IN ('effective_interval','legacy_total_only')),
            created_at            INTEGER NOT NULL
        );
        CREATE INDEX idx_segments_session ON focus_segments(session_id);
        CREATE INDEX idx_segments_task_state ON focus_segments(task_id, state);
        "#,
    )?;

    // Local guest profile (single-profile until v1.4).
    tx.execute(
        "INSERT INTO profiles (id, kind, created_at, updated_at) VALUES ('local', 'guest', ?1, ?1)",
        params![now],
    )?;

    // Default categories; 其他 doubles as the fallback category.
    for (i, name) in ["学习", "工作", "生活", "其他"].iter().enumerate() {
        tx.execute(
            "INSERT INTO categories (id, profile_id, name, normalized_name, status, sort_order, created_at, updated_at)
             VALUES (?1, 'local', ?2, ?2, 'active', ?3, ?4, ?4)",
            params![format!("cat-default-{i}"), name, i as i64, now],
        )?;
    }

    // Tasks gain the v1.2 columns. SQLite ALTERs only ADD columns.
    tx.execute_batch(
        r#"
        ALTER TABLE tasks ADD COLUMN profile_id TEXT NOT NULL DEFAULT 'local';
        ALTER TABLE tasks ADD COLUMN project_id TEXT REFERENCES projects(id);
        ALTER TABLE tasks ADD COLUMN target_seconds INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE tasks ADD COLUMN budget_source TEXT NOT NULL DEFAULT 'migration';
        ALTER TABLE tasks ADD COLUMN status TEXT NOT NULL DEFAULT 'todo';
        ALTER TABLE tasks ADD COLUMN deadline TEXT;
        ALTER TABLE tasks ADD COLUMN notes TEXT NOT NULL DEFAULT '';
        ALTER TABLE sessions ADD COLUMN profile_id TEXT NOT NULL DEFAULT 'local';
        CREATE INDEX idx_tasks_project ON tasks(project_id);
        "#,
    )?;

    // Consolidate identical trimmed legacy project names into real projects
    // under the fallback category. Blank names stay standalone; 通用 follows
    // the user's R06 decision (standalone default, or one real project).
    tx.execute(
        "INSERT INTO projects (id, profile_id, category_id, name, normalized_name, sort_order, created_at, updated_at)
         SELECT 'prj-' || hex(randomblob(8)), 'local',
                (SELECT id FROM categories WHERE profile_id = 'local' AND name = '其他'),
                t.pname, t.pname,
                ROW_NUMBER() OVER (ORDER BY t.pname) - 1, ?1, ?1
         FROM (SELECT DISTINCT TRIM(project) AS pname FROM tasks) t
         WHERE t.pname <> '' AND (t.pname <> '通用' OR ?2 = 'project')",
        params![now, params.general_mapping],
    )?;
    tx.execute(
        "UPDATE tasks SET project_id = (
            SELECT p.id FROM projects p
            WHERE p.profile_id = 'local' AND p.normalized_name = TRIM(tasks.project)
         )
         WHERE TRIM(project) <> '' AND (TRIM(project) <> '通用' OR ?1 = 'project')",
        params![params.general_mapping],
    )?;

    // Status mirrors the legacy done flag.
    tx.execute(
        "UPDATE tasks SET status = CASE WHEN done = 1 THEN 'done' ELSE 'todo' END",
        [],
    )?;

    // R06: budget snapshot estimated from the USER-CONFIRMED basis minutes
    // (falling back to the current settings when the startup path took the
    // default). Marked budget_source='migration' — an estimate, never exact
    // history.
    let basis_minutes = if params.budget_focus_minutes > 0 {
        params.budget_focus_minutes
    } else {
        tx.query_row(
            "SELECT COALESCE((SELECT focus_duration_minutes FROM settings WHERE id = 1), 25)",
            [],
            |row| row.get::<_, i64>(0),
        )?
    };
    tx.execute(
        "UPDATE tasks
         SET target_seconds = pomodoro_target * ?1 * 60",
        params![basis_minutes],
    )?;
    tx.execute(
        "INSERT INTO budget_history (id, task_id, old_target_seconds, new_target_seconds, source, created_at)
         SELECT 'bh-' || hex(randomblob(8)), id, NULL, target_seconds, 'migration', ?1 FROM tasks",
        params![now],
    )?;

    // R07/R08: one confirmed legacy record per eligible historical session.
    // - Attribution comes from the session's OCCURRENCE-TIME project snapshot
    //   (matched against the projects consolidated in this migration); the
    //   task's CURRENT project must never rewrite history. Unmatched names
    //   stay unmapped (project_id NULL, name preserved).
    // - Old sessions have no pause ledger: the accurate fact is the TOTAL
    //   (focused_seconds × 1000). started/ended are kept as metadata only and
    //   the row is marked 'legacy_total_only' — it must never be treated as a
    //   continuous effective interval.
    tx.execute(
        "INSERT INTO focus_segments (id, profile_id, session_id, task_id,
                                     project_id_snapshot, project_name_snapshot, category_id_snapshot,
                                     effective_start_ms, effective_end_ms, effective_ms, state,
                                     temporal_precision, created_at)
         SELECT 'seg-' || hex(randomblob(8)), 'local', s.id, s.task_id,
                mp.project_id, s.project_snapshot, mp.category_id,
                s.started_at, s.started_at + s.focused_seconds * 1000, s.focused_seconds * 1000,
                'confirmed', 'legacy_total_only', s.ended_at
         FROM sessions s
         LEFT JOIN (SELECT p.id AS project_id, p.normalized_name, p.category_id
                    FROM projects p WHERE p.profile_id = 'local') mp
                ON mp.normalized_name = TRIM(s.project_snapshot)
         WHERE s.statistics_eligible = 1 AND s.focused_seconds > 0",
        [],
    )?;

    // No dangling references may survive the upgrade.
    let violations: usize = tx
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |row| row.get::<_, String>(0))?
        .count();
    if violations > 0 {
        return Err(CommandError::database(format!(
            "foreign_key_check reported {violations} violating rows after the v4 migration"
        )));
    }
    Ok(())
}

/// R06: migration entry that carries the user-confirmed parameters down to
/// the v4 semantic step.
pub fn run_migrations_with(conn: &mut Connection, params: &crate::models::MigrationParams) -> Result<(), CommandError> {
    let current = schema_version(conn)?;
    if current > LATEST_SCHEMA_VERSION {
        return Err(CommandError::database_too_new(current, LATEST_SCHEMA_VERSION));
    }
    if current >= LATEST_SCHEMA_VERSION {
        return Ok(());
    }
    // Re-run older steps exactly like run_migrations (shared code path).
    if current < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V1)?;
        tx.pragma_update(None, "user_version", 1u32)?;
        tx.commit()?;
    }
    if current < 2 {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_V2)?;
        tx.pragma_update(None, "user_version", 2u32)?;
        tx.commit()?;
    }
    if current < 3 {
        let tx = conn.transaction()?;
        run_v3_migration(&tx)?;
        tx.pragma_update(None, "user_version", 3u32)?;
        tx.commit()?;
    }
    if current < 4 {
        let tx = conn.transaction()?;
        run_v4_migration_with(&tx, params)?;
        tx.pragma_update(None, "user_version", 4u32)?;
        tx.commit()?
    }
    if current < 5 {
        // v5 is pure metadata NULL-ability: no user decisions involved, so
        // the parameterised R06 entry takes the same automatic path.
        let tx = conn.transaction()?;
        run_v5_migration(&tx)?;
        tx.pragma_update(None, "user_version", 5u32)?;
        tx.commit()?
    }
    Ok(())
}

/// R06: read-only preview of the pending semantic migration. Runs against a
/// v3 database and changes NOTHING — the numbers shown are the numbers the
/// user confirms before any conversion happens.
pub fn preview_v4_migration(conn: &Connection) -> Result<crate::models::MigrationPreview, CommandError> {
    use crate::models::MigrationPreview;
    let version = schema_version(conn)?;
    if version >= LATEST_SCHEMA_VERSION {
        return Err(CommandError::validation("database does not need migration"));
    }
    let task_count: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))?;
    let session_count: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
    let general_task_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE TRIM(project) = '通用'", [], |r| r.get(0))?
    ;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT TRIM(project) FROM tasks WHERE TRIM(TRIM(project)) <> '' AND TRIM(project) <> '通用' ORDER BY 1"
    )?;
    let projects_to_create = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let suggested_focus_minutes: i64 = conn.query_row(
        "SELECT COALESCE((SELECT focus_duration_minutes FROM settings WHERE id = 1), 25)",
        [],
        |r| r.get(0),
    )?;
    Ok(MigrationPreview {
        schema_version: version,
        task_count,
        session_count,
        projects_to_create,
        general_task_count,
        suggested_focus_minutes,
    })
}
/// Inserts the single settings row and idle timer row if they are missing.
pub fn seed_defaults(conn: &Connection) -> Result<(), CommandError> {
    let settings = AppSettings::default();
    conn.execute(
        "INSERT INTO settings (
            id, focus_duration_minutes, short_break_minutes, long_break_minutes,
            auto_start_break, sound_enabled, notification_enabled, daily_goal,
            reduce_motion, updated_at
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO NOTHING",
        params![
            settings.focus_duration_minutes,
            settings.short_break_minutes,
            settings.long_break_minutes,
            settings.auto_start_break as i64,
            settings.sound_enabled as i64,
            settings.notification_enabled as i64,
            settings.daily_goal,
            settings.reduce_motion as i64,
            settings.updated_at,
        ],
    )?;

    let timer = TimerSnapshot::idle(
        TimerMode::Focus,
        settings.duration_seconds_for_mode(TimerMode::Focus),
    );
    conn.execute(
        "INSERT INTO timer_state (
            id, mode, state, active_session_id, selected_task_id,
            task_title_snapshot, project_snapshot, duration_seconds, remaining_seconds,
            started_at, target_end_at, paused_at, revision, updated_at
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO NOTHING",
        params![
            timer.mode.as_str(),
            timer.state.as_str(),
            timer.active_session_id,
            timer.selected_task_id,
            timer.task_title_snapshot,
            timer.project_snapshot,
            timer.duration_seconds,
            timer.remaining_seconds,
            timer.started_at,
            timer.target_end_at,
            timer.paused_at,
            timer.revision,
            timer.updated_at,
        ],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .expect("query should prepare");
        stmt.query_map([], |row| row.get::<_, String>(0))
            .expect("query should run")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows should decode")
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
            .expect("count should run")
    }

    #[test]
    fn migrations_create_every_expected_table() {
        let conn = open_in_memory().expect("database should open");

        let tables = table_names(&conn);

        assert!(tables.contains(&"tasks".to_owned()), "missing tasks: {tables:?}");
        assert!(tables.contains(&"timer_state".to_owned()), "missing timer_state: {tables:?}");
        assert!(tables.contains(&"sessions".to_owned()), "missing sessions: {tables:?}");
        assert!(tables.contains(&"settings".to_owned()), "missing settings: {tables:?}");
        assert!(tables.contains(&"tags".to_owned()), "missing tags: {tables:?}");
        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = open_in_memory().expect("database should open");

        run_migrations(&mut conn).expect("re-running migrations should be a no-op");
        run_migrations(&mut conn).expect("re-running migrations should be a no-op");

        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);
        assert_eq!(table_names(&conn).len(), 10);
    }

    #[test]
    fn seed_defaults_inserts_a_single_settings_and_timer_row() {
        let conn = open_in_memory().expect("database should open");

        assert_eq!(count(&conn, "settings"), 1);
        assert_eq!(count(&conn, "timer_state"), 1);

        let (focus, short, long, goal, sound, notify) = conn
            .query_row(
                "SELECT focus_duration_minutes, short_break_minutes, long_break_minutes,
                        daily_goal, sound_enabled, notification_enabled
                 FROM settings WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .expect("settings row should exist");
        assert_eq!((focus, short, long, goal, sound, notify), (25, 5, 15, 8, 1, 1));

        let (mode, state, duration, remaining, revision) = conn
            .query_row(
                "SELECT mode, state, duration_seconds, remaining_seconds, revision
                 FROM timer_state WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .expect("timer row should exist");
        assert_eq!(mode, "focus");
        assert_eq!(state, "idle");
        assert_eq!(duration, 1500);
        assert_eq!(remaining, 1500);
        assert_eq!(revision, 0);
    }

    #[test]
    fn seed_defaults_is_idempotent() {
        let conn = open_in_memory().expect("database should open");

        seed_defaults(&conn).expect("re-seeding should be a no-op");
        seed_defaults(&conn).expect("re-seeding should be a no-op");

        assert_eq!(count(&conn, "settings"), 1);
        assert_eq!(count(&conn, "timer_state"), 1);
    }

    #[test]
    fn single_row_tables_reject_a_second_row() {
        let conn = open_in_memory().expect("database should open");

        let result = conn.execute(
            "INSERT INTO settings (id, focus_duration_minutes, short_break_minutes,
                long_break_minutes, auto_start_break, sound_enabled,
                notification_enabled, daily_goal, updated_at)
             VALUES (2, 25, 5, 15, 0, 1, 1, 8, 0)",
            [],
        );

        assert!(result.is_err(), "timer_state/settings must stay single-row");
    }

    #[test]
    fn timer_state_rejects_a_negative_revision() {
        let conn = open_in_memory().expect("database should open");

        let result = conn.execute("UPDATE timer_state SET revision = -1 WHERE id = 1", []);

        assert!(result.is_err(), "revision must stay >= 0");
    }

    #[test]
    fn timer_state_rejects_a_non_positive_duration() {
        let conn = open_in_memory().expect("database should open");

        assert!(
            conn.execute("UPDATE timer_state SET duration_seconds = 0 WHERE id = 1", [])
                .is_err(),
            "duration_seconds must stay > 0"
        );
        assert!(
            conn.execute("UPDATE timer_state SET duration_seconds = -5 WHERE id = 1", [])
                .is_err(),
            "duration_seconds must stay > 0"
        );
    }

    #[test]
    fn timer_state_rejects_negative_remaining_seconds() {
        let conn = open_in_memory().expect("database should open");

        let result = conn.execute("UPDATE timer_state SET remaining_seconds = -1 WHERE id = 1", []);

        assert!(result.is_err(), "remaining_seconds must stay >= 0");
    }

    #[test]
    fn deleting_a_task_preserves_its_session_with_a_null_task_id() {
        let conn = open_in_memory().expect("database should open");

        conn.execute(
            "INSERT INTO tasks (id, title, done, pomodoro_target, priority, project, tag_id,
                                sort_order, created_at, updated_at, completed_at)
             VALUES ('task-1', 'Deep work', 0, 4, 'high', 'Abyssal', 'system-other', 0, 1, 1, NULL)",
            [],
        )
        .expect("task should insert");
        conn.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                                   tag_name_snapshot, mode, status, planned_seconds,
                                   focused_seconds, started_at, ended_at, finish_reason,
                                   statistics_eligible, qualification_reason)
             VALUES ('session-1', 'task-1', 'Deep work', 'Abyssal', 'system-other', '其他',
                     'focus', 'completed', 1500, 1500, 1, 1501, 'legacy', 1, 'qualified')",
            [],
        )
        .expect("session should insert");

        conn.execute("DELETE FROM tasks WHERE id = 'task-1'", [])
            .expect("task should delete");

        let (task_id, title) = conn
            .query_row(
                "SELECT task_id, task_title_snapshot FROM sessions WHERE id = 'session-1'",
                [],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
            )
            .expect("session should survive");

        assert_eq!(task_id, None, "foreign key should null out on delete");
        assert_eq!(title, "Deep work", "snapshots must survive task deletion");
    }

    #[test]
    fn corrupt_database_is_recovered_by_reseeding() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-recover-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");
        // Garbage bytes that are not a valid SQLite database.
        std::fs::write(&db_path, b"not a sqlite database at all\x00\x01\x02")
            .expect("write garbage");

        let conn = open_at(&db_path).expect("open_at should recover from corruption");

        // Fresh, healthy database: default settings + idle timer present.
        let (focus, goal) = conn
            .query_row(
                "SELECT focus_duration_minutes, daily_goal FROM settings WHERE id = 1",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .expect("settings should exist");
        assert_eq!(focus, 25);
        assert_eq!(goal, 8);
        assert!(is_healthy(&conn), "recovered database must be healthy");

        // The corrupt file was preserved with a `.corrupt-` suffix for manual recovery.
        let preserved = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".corrupt-"));
        assert!(preserved, "corrupt file must be preserved for manual recovery");

        // Cleanup.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_database_is_created_and_seeded_on_first_open() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-empty-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");

        let conn = open_at(&db_path).expect("open_at should create a fresh database");
        assert!(is_healthy(&conn));
        assert_eq!(count(&conn, "settings"), 1);
        assert_eq!(count(&conn, "timer_state"), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v3_database_migrates_to_v4_with_projects_budgets_and_segments() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-v4-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");

        // Build a genuine v3 database: a task in the free-text project
        // "Java 面试" and one eligible focus session for it.
        {
            let mut conn = Connection::open(&db_path).expect("open");
            configure(&conn, false).expect("configure");
            {
                let tx = conn.transaction().expect("tx");
                tx.execute_batch(MIGRATION_V1).expect("v1");
                tx.pragma_update(None, "user_version", 1u32).expect("v1 marker");
                tx.commit().expect("v1 commit");
            }
            {
                let tx = conn.transaction().expect("tx");
                tx.execute_batch(MIGRATION_V2).expect("v2");
                tx.pragma_update(None, "user_version", 2u32).expect("v2 marker");
                tx.commit().expect("v2 commit");
            }
            {
                let tx = conn.transaction().expect("tx");
                run_v3_migration(&tx).expect("v3");
                tx.pragma_update(None, "user_version", 3u32).expect("v3 marker");
                tx.commit().expect("v3 commit");
            }
            seed_defaults(&conn).expect("seed");
            let fallback: String = conn
                .query_row("SELECT id FROM tags WHERE is_fallback = 1", [], |r| r.get(0))
                .expect("fallback tag");
            conn.execute(
                "INSERT INTO tasks (id, title, done, pomodoro_target, priority, project, tag_id, sort_order, created_at, updated_at, completed_at)
                 VALUES ('task-legacy', '数据库迁移', 0, 2, 'med', 'Java 面试', ?1, 0, 1, 1, NULL)",
                params![fallback],
            ).expect("task");
            conn.execute(
                "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id, tag_name_snapshot, mode, status, planned_seconds, focused_seconds, started_at, ended_at, finish_reason, statistics_eligible, qualification_reason)
                 VALUES ('sess-legacy', 'task-legacy', '数据库迁移', 'Java 面试', ?1, '其他', 'focus', 'completed', 1500, 1200, 1000, 2200, 'elapsed', 1, 'qualified')",
                params![fallback],
            ).expect("session");
        }

        let conn = open_at(&db_path).expect("open_at should migrate v3 → v4");
        // open_at runs the FULL chain (now through v5); the assertions below
        // pin the v4 semantic outcomes, which v5 preserves verbatim.
        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);

        // The free-text project became a real project under 其他.
        let project: (String, String, String) = conn
            .query_row(
                "SELECT p.id, p.name, c.name FROM projects p JOIN categories c ON c.id = p.category_id
                 WHERE p.normalized_name = 'Java 面试'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("project consolidated");
        assert_eq!(project.1, "Java 面试");
        assert_eq!(project.2, "其他");

        // The task links to it and got a budget estimate (2 × 25min × 60).
        let task: (String, i64, String) = conn
            .query_row(
                "SELECT project_id, target_seconds, budget_source FROM tasks WHERE id = 'task-legacy'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("task row");
        assert_eq!(task.0, project.0);
        assert_eq!(task.1, 2 * 25 * 60);
        assert_eq!(task.2, "migration");

        // The historical session got one confirmed segment with 20 minutes.
        let seg: i64 = conn
            .query_row(
                "SELECT effective_ms FROM focus_segments WHERE session_id = 'sess-legacy' AND state = 'confirmed'",
                [],
                |r| r.get(0),
            )
            .expect("segment");
        assert_eq!(seg, 1_200_000);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Builds a v3-schema in-memory database with seeded defaults.
    fn v3_database() -> Connection {
        let mut conn = Connection::open_in_memory().expect("mem db");
        configure(&conn, false).expect("configure");
        {
            let tx = conn.transaction().expect("tx");
            tx.execute_batch(MIGRATION_V1).expect("v1");
            tx.pragma_update(None, "user_version", 1u32).expect("v1");
            tx.commit().expect("v1");
        }
        {
            let tx = conn.transaction().expect("tx");
            tx.execute_batch(MIGRATION_V2).expect("v2");
            tx.pragma_update(None, "user_version", 2u32).expect("v2");
            tx.commit().expect("v2");
        }
        {
            let tx = conn.transaction().expect("tx");
            run_v3_migration(&tx).expect("v3");
            tx.pragma_update(None, "user_version", 3u32).expect("v3");
            tx.commit().expect("v3");
        }
        seed_defaults(&conn).expect("seed");
        conn
    }

    #[test]
    fn migration_preview_reads_v3_without_converting() {
        let conn = v3_database();
        let fallback: String = conn
            .query_row("SELECT id FROM tags WHERE is_fallback = 1", [], |r| r.get(0))
            .expect("fallback tag");
        conn.execute(
            "INSERT INTO tasks (id, title, done, pomodoro_target, priority, project, tag_id, sort_order, created_at, updated_at, completed_at)
             VALUES ('t1', '数据库迁移', 0, 2, 'med', 'Java 面试', ?1, 0, 1, 1, NULL),
                    ('t2', '看书', 0, 1, 'low', '通用', ?1, 1, 1, 1, NULL)",
            params![fallback],
        ).expect("tasks");

        let preview = preview_v4_migration(&conn).expect("preview");
        assert_eq!(preview.schema_version, 3);
        assert_eq!(preview.task_count, 2);
        assert_eq!(preview.projects_to_create, vec!["Java 面试".to_owned()]);
        assert_eq!(preview.general_task_count, 1);
        // The preview must not have touched the data.
        let version = schema_version(&conn).expect("version");
        assert_eq!(version, 3, "preview is read-only");
    }

    #[test]
    fn confirmed_migration_honours_user_parameters() {
        let mut conn = v3_database();
        let fallback: String = conn
            .query_row("SELECT id FROM tags WHERE is_fallback = 1", [], |r| r.get(0))
            .expect("fallback tag");
        conn.execute(
            "INSERT INTO tasks (id, title, done, pomodoro_target, priority, project, tag_id, sort_order, created_at, updated_at, completed_at)
             VALUES ('t1', '数据库迁移', 0, 2, 'med', 'Java 面试', ?1, 0, 1, 1, NULL),
                    ('t2', '看书', 0, 1, 'low', '通用', ?1, 1, 1, 1, NULL)",
            params![fallback],
        ).expect("tasks");

        // The user confirmed a 40-minute budget basis and a real 通用 project.
        let params = crate::models::MigrationParams {
            budget_focus_minutes: 40,
            general_mapping: "project".to_owned(),
        };
        run_migrations_with(&mut conn, &params).expect("migrate");
        assert_eq!(schema_version(&conn).expect("version"), LATEST_SCHEMA_VERSION);

        // Budget honours the confirmed basis: 2 × 40min × 60 = 4800s.
        let target: i64 = conn
            .query_row("SELECT target_seconds FROM tasks WHERE id = 't1'", [], |r| r.get(0))
            .expect("target");
        assert_eq!(target, 4800);

        // 通用 became a real project per the user's choice.
        let general: i64 = conn
            .query_row("SELECT COUNT(*) FROM projects WHERE normalized_name = '通用'", [], |r| r.get(0))
            .expect("general project");
        assert_eq!(general, 1);
        let t2_project: Option<String> = conn
            .query_row("SELECT project_id FROM tasks WHERE id = 't2'", [], |r| r.get(0))
            .expect("t2 link");
        assert!(t2_project.is_some());
    }

    #[test]
    fn open_failure_is_not_treated_as_corruption() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-openfail-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        // A DIRECTORY at the database path fails to open with an I/O error -
        // not a corruption diagnosis. The path must be left completely alone.
        let db_path = dir.join("abyssal-reverie.sqlite");
        std::fs::create_dir(&db_path).expect("create blocking directory");

        let err = open_at(&db_path).expect_err("an unopenable path must fail");
        assert_ne!(err.code, crate::error::ErrorCode::DatabaseTooNew);
        assert!(db_path.is_dir(), "the blocking path must not be renamed aside");
        let sidecars = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
            .count();
        assert_eq!(sidecars, 0, "no corruption rename may happen on open errors");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn newer_database_is_refused_and_untouched() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-toonew-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");

        // Simulate a database written by a future version: a plain (default
        // journal mode) SQLite file stamped with a too-new user_version.
        {
            let conn = Connection::open(&db_path).expect("create future db");
            conn.pragma_update(None, "user_version", 99u32).expect("stamp v99");
        }
        let before = std::fs::read(&db_path).expect("read before");
        assert!(!before.is_empty());

        let err = open_at(&db_path).expect_err("a too-new database must be refused");
        assert_eq!(err.code, crate::error::ErrorCode::DatabaseTooNew);
        assert!(err.message.contains("99"), "message should carry the on-disk version");

        // Not a single byte may have changed — no WAL sidecar, no journal
        // switch, no migration attempt.
        let after = std::fs::read(&db_path).expect("read after");
        assert_eq!(before, after, "the refused database file must be byte-identical");
        let sidecars = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.ends_with("-wal") || name.ends_with("-shm") || name.contains(".corrupt-") || name.contains(".bak")
            })
            .count();
        assert_eq!(sidecars, 0, "no sidecar files may be created by the refused open");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v1_database_migrates_to_v2_preserving_data() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-migrate-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");

        // Build a genuine v1 database by hand (no reduce_motion column), with
        // user data that must survive the in-place upgrade.
        {
            let mut conn = Connection::open(&db_path).expect("open");
            conn.execute_batch(MIGRATION_V1).expect("apply v1");
            conn.pragma_update(None, "user_version", 1u32).expect("v1 marker");
            conn.execute(
                "INSERT INTO tasks (id, title, created_at, updated_at) VALUES ('t1', 'Kept', 1, 1)",
                [],
            )
            .expect("task");
            conn.execute(
                "INSERT INTO settings (id, focus_duration_minutes, short_break_minutes,
                                       long_break_minutes, auto_start_break, sound_enabled,
                                       notification_enabled, daily_goal, updated_at)
                 VALUES (1, 17, 5, 15, 0, 1, 1, 8, 0)",
                [],
            )
            .expect("settings");
        }

        // Reopen through the normal path → migrates to v2 without data loss.
        let conn = open_at(&db_path).expect("open_at should migrate v1 → v2");
        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);

        let settings = crate::repository::get_settings(&conn).expect("settings readable");
        assert_eq!(settings.focus_duration_minutes, 17, "user setting preserved");
        assert!(!settings.reduce_motion, "new column defaults to false");
        assert_eq!(count(&conn, "tasks"), 1, "task data preserved");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── A0: migration failure ≠ corruption (v1.1 review #2) ──────────────────

    #[test]
    fn migration_failure_preserves_database_and_blocks_startup() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-migfail-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");

        // A genuine v1 database with a hostile pre-existing `reduce_motion`
        // column, so the v2 migration (ALTER TABLE ... ADD reduce_motion)
        // deterministically fails with "duplicate column name".
        {
            let mut conn = Connection::open(&db_path).expect("open");
            conn.execute_batch(MIGRATION_V1).expect("apply v1");
            conn.pragma_update(None, "user_version", 1u32).expect("v1 marker");
            conn.execute_batch("ALTER TABLE settings ADD COLUMN reduce_motion TEXT;")
                .expect("hostile column");
            conn.execute(
                "INSERT INTO tasks (id, title, created_at, updated_at) VALUES ('t1', 'Kept', 1, 1)",
                [],
            )
            .expect("task");
        }

        let result = open_at(&db_path);

        // Startup must be blocked with a diagnosable migration error.
        let err = result.expect_err("migration failure must block startup");
        assert!(
            err.message.contains("migration"),
            "error must mention migration, got: {}",
            err.message
        );

        // The original database must be preserved: same version, same data.
        let raw = Connection::open(&db_path).expect("original file must still open");
        let version: u32 = raw
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version readable");
        assert_eq!(version, 1, "user_version must be unchanged");
        let tasks: i64 = raw
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .expect("count readable");
        assert_eq!(tasks, 1, "original data must be preserved");

        // Migration failure must NOT be treated as corruption: no rename, and
        // no fresh empty database created in place of the original.
        let renamed = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"));
        assert!(!renamed, "migration failure must not trigger corrupt-recovery");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pre_migration_backup_is_created_before_upgrading() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-prebackup-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");

        {
            let mut conn = Connection::open(&db_path).expect("open");
            conn.execute_batch(MIGRATION_V1).expect("apply v1");
            conn.pragma_update(None, "user_version", 1u32).expect("v1 marker");
            conn.execute(
                "INSERT INTO tasks (id, title, created_at, updated_at) VALUES ('t1', 'Kept', 1, 1)",
                [],
            )
            .expect("task");
        }

        let conn = open_at(&db_path).expect("upgrade should succeed");
        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);

        let mut backups: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains(".pre-v"))
            .collect();
        backups.sort();
        assert_eq!(
            backups.len(),
            1,
            "exactly one pre-migration backup expected, got: {backups:?}"
        );

        // The backup must preserve the pre-migration (v1) state.
        let backup_path = dir.join(&backups[0]);
        let raw = Connection::open(&backup_path).expect("backup must open");
        let version: u32 = raw
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("backup version readable");
        assert_eq!(version, 1, "backup must preserve pre-migration version");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── A1–A3: SQLite v3 migration (v1.1 local prerequisites) ────────────────

    /// Builds a genuine v2 on-disk database with representative user data.
    fn build_v2_database(db_path: &Path) {
        let mut conn = Connection::open(db_path).expect("open");
        conn.execute_batch(MIGRATION_V1).expect("apply v1");
        conn.execute_batch(MIGRATION_V2).expect("apply v2");
        conn.pragma_update(None, "user_version", 2u32).expect("v2 marker");
        crate::db::seed_defaults(&conn).expect("seed settings/timer");
        conn.execute(
            "INSERT INTO tasks (id, title, created_at, updated_at) VALUES ('t1', 'Kept', 1, 1)",
            [],
        )
        .expect("task");
        // focus completed 600s → eligible / qualified
        conn.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, mode,
                                   status, planned_seconds, focused_seconds, started_at, ended_at)
             VALUES ('s-eligible', NULL, 'task', 'P', 'focus', 'completed', 1500, 600, 1, 2)",
            [],
        )
        .expect("session eligible");
        // focus completed 10s → too_short (hidden everywhere)
        conn.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, mode,
                                   status, planned_seconds, focused_seconds, started_at, ended_at)
             VALUES ('s-short', NULL, 'task', 'P', 'focus', 'completed', 1500, 10, 3, 4)",
            [],
        )
        .expect("session short");
        // focus abandoned 600s → abandoned (never counted)
        conn.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, mode,
                                   status, planned_seconds, focused_seconds, started_at, ended_at)
             VALUES ('s-abandoned', NULL, 'task', 'P', 'focus', 'abandoned', 1500, 600, 5, 6)",
            [],
        )
        .expect("session abandoned");
        // short break completed → non_focus
        conn.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, mode,
                                   status, planned_seconds, focused_seconds, started_at, ended_at)
             VALUES ('s-break', NULL, '短休', '休息', 'short', 'completed', 300, 300, 7, 8)",
            [],
        )
        .expect("session break");
    }

    /// Builds a genuine v4 on-disk database by applying the REAL migration
    /// chain (V1+V2 batches, then run_v3_migration / run_v4_migration) and
    /// seeding representative user data — never a hand-copied schema.
    ///
    /// Fixture totals (asserted verbatim by the v5 conservation tests):
    /// 3 tasks / 2 sessions / 3 budget rows / 2 segments; invested time =
    /// 900 focused seconds = 900,000 effective ms; task links: sessions 1,
    /// segments 1, budget 3; timer bound to 't-keep1'.
    fn build_v4_database(db_path: &Path) {
        let mut conn = Connection::open(db_path).expect("open");
        conn.execute_batch(MIGRATION_V1).expect("apply v1");
        conn.execute_batch(MIGRATION_V2).expect("apply v2");
        conn.pragma_update(None, "user_version", 2u32).expect("v2 marker");
        {
            let tx = conn.transaction().expect("tx");
            run_v3_migration(&tx).expect("v3 step");
            tx.pragma_update(None, "user_version", 3u32).expect("v3 marker");
            tx.commit().expect("commit v3");
        }
        {
            let tx = conn.transaction().expect("tx");
            run_v4_migration(&tx).expect("v4 step");
            tx.pragma_update(None, "user_version", 4u32).expect("v4 marker");
            tx.commit().expect("commit v4");
        }
        seed_defaults(&conn).expect("seed settings/timer");

        // Three tasks carrying exactly the historical values v5 must preserve
        // verbatim (历史『其他』『中』不被批量清空): other/med, work/high, study/low.
        conn.execute_batch(
            "INSERT INTO tasks (id, title, done, pomodoro_target, priority, project, tag_id,
                                sort_order, created_at, updated_at, completed_at, profile_id,
                                project_id, target_seconds, budget_source, status, deadline, notes)
             VALUES ('t-keep1', '保留任务一', 0, 1, 'med', '通用', 'system-other',
                     0, 1, 1, NULL, 'local', NULL, 1500, 'migration', 'todo', NULL, ''),
                    ('t-keep2', '保留任务二', 0, 2, 'high', '通用', 'system-work',
                     1, 1, 1, NULL, 'local', NULL, 3000, 'migration', 'todo', NULL, ''),
                    ('t-keep3', '保留任务三', 1, 1, 'low', '通用', 'system-study',
                     2, 1, 1, 2, 'local', NULL, 1500, 'migration', 'done', NULL, '');
            INSERT INTO budget_history (id, task_id, old_target_seconds, new_target_seconds, source, created_at)
             VALUES ('bh-1', 't-keep1', NULL, 1500, 'migration', 1),
                    ('bh-2', 't-keep2', NULL, 3000, 'migration', 1),
                    ('bh-3', 't-keep3', NULL, 1500, 'migration', 1);
            INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                                  tag_name_snapshot, mode, status, planned_seconds,
                                  focused_seconds, started_at, ended_at, finish_reason,
                                  statistics_eligible, qualification_reason, profile_id)
             VALUES ('s-1', 't-keep1', '保留任务一', '通用', 'system-other', '其他', 'focus', 'completed',
                     1500, 600, 1, 2, 'elapsed', 1, 'qualified', 'local'),
                    ('s-2', NULL, '自由轮次', '通用', NULL, '', 'short', 'completed',
                     300, 300, 3, 4, 'elapsed', 0, 'non_focus', 'local');
            INSERT INTO focus_segments (id, profile_id, session_id, task_id, project_id_snapshot,
                                        project_name_snapshot, category_id_snapshot,
                                        category_name_snapshot, effective_start_ms,
                                        effective_end_ms, effective_ms, state,
                                        temporal_precision, created_at)
             VALUES ('seg-1', 'local', 's-1', 't-keep1', NULL, '通用', NULL, '',
                     1, 601000, 600000, 'confirmed', 'effective_interval', 2),
                    ('seg-2', 'local', 's-2', NULL, NULL, '通用', NULL, '',
                     3, 300003, 300000, 'confirmed', 'effective_interval', 4);
            UPDATE timer_state SET selected_task_id = 't-keep1' WHERE id = 1;",
        )
        .expect("seed v4 fixture data");
    }

    // ─── 批次 1：v4 → v5（可空元数据；rename-first 重建 + 守恒对账）────────────


    #[test]
    fn v5_migration_conserves_exact_task_links_per_child_row() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-v5links-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");
        build_v4_database(&db_path);

        let conn = open_at(&db_path).expect("v5 upgrade should succeed");

        // Row-level link conservation: every child row must still point at
        // the SAME task id (fixture-known values). A swap that preserves row
        // counts, NOT-NULL counts and time sums must FAIL here and in the
        // in-migration EXCEPT audit.
        let links = |table: &str| -> Vec<(String, Option<String>)> {
            conn.prepare(&format!("SELECT id, task_id FROM {table} ORDER BY id"))
                .expect("prepare")
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .expect("query")
                .collect::<Result<Vec<_>, _>>()
                .expect("rows")
        };
        assert_eq!(
            links("sessions"),
            vec![("s-1".to_owned(), Some("t-keep1".to_owned())), ("s-2".to_owned(), None)],
            "sessions.task_id must survive verbatim"
        );
        assert_eq!(
            links("budget_history"),
            vec![
                ("bh-1".to_owned(), Some("t-keep1".to_owned())),
                ("bh-2".to_owned(), Some("t-keep2".to_owned())),
                ("bh-3".to_owned(), Some("t-keep3".to_owned())),
            ],
            "budget_history.task_id must survive verbatim"
        );
        assert_eq!(
            links("focus_segments"),
            vec![
                ("seg-1".to_owned(), Some("t-keep1".to_owned())),
                ("seg-2".to_owned(), None),
            ],
            "focus_segments.task_id must survive verbatim"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v5_migration_timer_binding_survives_verbatim() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-v5timer-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");
        build_v4_database(&db_path);

        // Read the pre-migration binding with a throwaway connection.
        let before: Option<String> = {
            let raw = Connection::open(&db_path).expect("reopen fixture");
            raw.query_row(
                "SELECT selected_task_id FROM timer_state WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .expect("pre-migration binding")
        };

        let conn = open_at(&db_path).expect("v5 upgrade should succeed");
        let after: Option<String> = conn
            .query_row("SELECT selected_task_id FROM timer_state WHERE id = 1", [], |r| r.get(0))
            .expect("post-migration binding");
        assert_eq!(after, before, "timer_state.selected_task_id must survive verbatim");

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn migrates_v4_to_v5_preserving_all_metadata() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-v5-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");
        build_v4_database(&db_path);

        let conn = open_at(&db_path).expect("v5 upgrade should succeed");
        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);

        // Historical values survive VERBATIM — no batch clear of 「其他」/「中」.
        let rows: Vec<(String, Option<String>, Option<String>)> = conn
            .prepare("SELECT id, tag_id, priority FROM tasks ORDER BY id")
            .expect("prepare")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");
        assert_eq!(
            rows,
            vec![
                ("t-keep1".to_owned(), Some("system-other".to_owned()), Some("med".to_owned())),
                ("t-keep2".to_owned(), Some("system-work".to_owned()), Some("high".to_owned())),
                ("t-keep3".to_owned(), Some("system-study".to_owned()), Some("low".to_owned())),
            ]
        );

        // The whole point of v5: the two metadata columns accept NULL.
        conn.execute("UPDATE tasks SET tag_id = NULL WHERE id = 't-keep3'", [])
            .expect("tag_id must be nullable after v5");
        conn.execute("UPDATE tasks SET priority = NULL WHERE id = 't-keep3'", [])
            .expect("priority must be nullable after v5");

        // Every index whose parent table was rebuilt is present again.
        let indexes: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'index'
                 AND tbl_name IN ('tasks','sessions','budget_history','focus_segments')",
            )
            .expect("prepare")
            .query_map([], |row| row.get(0))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");
        for expected in [
            "idx_budget_history_task",
            "idx_segments_session",
            "idx_segments_task_state",
            "idx_sessions_qualification",
            "idx_sessions_started_at",
            "idx_sessions_task_id",
            "idx_sessions_tag_started",
            "idx_tasks_project",
            "idx_tasks_sort_order",
            "idx_tasks_tag_sort",
        ] {
            assert!(indexes.iter().any(|i| i == expected), "missing index {expected}: {indexes:?}");
        }

        // timer_state binding survives the rebuild (bare TEXT, ids preserved).
        let selected: Option<String> = conn
            .query_row("SELECT selected_task_id FROM timer_state WHERE id = 1", [], |r| r.get(0))
            .expect("timer row");
        assert_eq!(selected.as_deref(), Some("t-keep1"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v5_migration_conserves_rows_and_invested_time() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-v5con-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");
        build_v4_database(&db_path);

        let conn = open_at(&db_path).expect("v5 upgrade should succeed");

        // Row counts, per table (fixture: 3 / 2 / 3 / 2).
        assert_eq!(count(&conn, "tasks"), 3);
        assert_eq!(count(&conn, "sessions"), 2);
        assert_eq!(count(&conn, "budget_history"), 3);
        assert_eq!(count(&conn, "focus_segments"), 2);

        // Total invested time — the 1.3 conservation headline.
        let focused: i64 = conn
            .query_row("SELECT COALESCE(SUM(focused_seconds), 0) FROM sessions", [], |r| r.get(0))
            .expect("focused sum");
        assert_eq!(focused, 900, "total invested seconds must survive the rebuild");
        let effective: i64 = conn
            .query_row("SELECT COALESCE(SUM(effective_ms), 0) FROM focus_segments", [], |r| r.get(0))
            .expect("effective sum");
        assert_eq!(effective, 900_000, "total invested ms must survive the rebuild");

        // Task-link distributions (CASCADE must not have eaten anything).
        let session_links: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions WHERE task_id IS NOT NULL", [], |r| r.get(0))
            .expect("session links");
        assert_eq!(session_links, 1);
        let segment_links: i64 = conn
            .query_row("SELECT COUNT(*) FROM focus_segments WHERE task_id IS NOT NULL", [], |r| r.get(0))
            .expect("segment links");
        assert_eq!(segment_links, 1);
        let budget_links: i64 = conn
            .query_row("SELECT COUNT(*) FROM budget_history WHERE task_id IS NOT NULL", [], |r| r.get(0))
            .expect("budget links");
        assert_eq!(budget_links, 3);

        // The rebuilt foreign keys still pass the check.
        let violations: usize = conn
            .prepare("PRAGMA foreign_key_check")
            .expect("prepare")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .count();
        assert_eq!(violations, 0, "foreign_key_check must stay clean after v5");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v5_migration_handles_an_empty_database() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-v5empty-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");
        build_v4_database(&db_path);

        // Empty every business table (conservation must pass with all zeros).
        {
            let raw = Connection::open(&db_path).expect("reopen fixture");
            raw.execute_batch(
                "DELETE FROM focus_segments;
                 DELETE FROM budget_history;
                 DELETE FROM sessions;
                 DELETE FROM tasks;
                 UPDATE timer_state SET selected_task_id = NULL;",
            )
            .expect("empty the fixture");
        }

        let conn = open_at(&db_path).expect("v5 upgrade should succeed on empty data");
        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);
        assert_eq!(count(&conn, "tasks"), 0);
        assert_eq!(count(&conn, "sessions"), 0);
        assert_eq!(count(&conn, "budget_history"), 0);
        assert_eq!(count(&conn, "focus_segments"), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── Codex 复审阻断 3：守恒对账必须捕捉 task_id 换绑 ────────────────────────
    // 行数、非空关联数与时间总和完全一致的两份数据，仍可能发生子表行之间的
    // task_id 互换——守恒审计必须对 (id, task_id) 做双向 EXCEPT 才能拦截。

    #[test]
    fn child_link_conservation_catches_a_swapped_task_id_in_sessions() {
        let mut conn = Connection::open_in_memory().expect("db");
        let tx = conn.transaction().expect("tx");
        tx.execute_batch(
            "CREATE TABLE sessions_old (id TEXT PRIMARY KEY, task_id TEXT);
             CREATE TABLE sessions_new (id TEXT PRIMARY KEY, task_id TEXT);
             INSERT INTO sessions_old VALUES ('s1','tA'), ('s2','tB');
             INSERT INTO sessions_new VALUES ('s1','tB'), ('s2','tA');",
        )
        .expect("fixture");
        let result = assert_child_task_links_preserved(&tx, "sessions_old", "sessions_new");
        assert!(
            result.is_err(),
            "a swapped task_id (counts identical) must fail the conservation audit"
        );
    }

    #[test]
    fn child_link_conservation_catches_a_swapped_task_id_in_budget_history() {
        let mut conn = Connection::open_in_memory().expect("db");
        let tx = conn.transaction().expect("tx");
        tx.execute_batch(
            "CREATE TABLE budget_history_old (id TEXT PRIMARY KEY, task_id TEXT);
             CREATE TABLE budget_history_new (id TEXT PRIMARY KEY, task_id TEXT);
             INSERT INTO budget_history_old VALUES ('bh1','tA'), ('bh2','tB');
             INSERT INTO budget_history_new VALUES ('bh1','tB'), ('bh2','tA');",
        )
        .expect("fixture");
        let result = assert_child_task_links_preserved(&tx, "budget_history_old", "budget_history_new");
        assert!(result.is_err(), "a swapped task_id must fail the conservation audit");
    }

    #[test]
    fn child_link_conservation_catches_a_swapped_task_id_in_focus_segments() {
        let mut conn = Connection::open_in_memory().expect("db");
        let tx = conn.transaction().expect("tx");
        tx.execute_batch(
            "CREATE TABLE focus_segments_old (id TEXT PRIMARY KEY, task_id TEXT);
             CREATE TABLE focus_segments_new (id TEXT PRIMARY KEY, task_id TEXT);
             INSERT INTO focus_segments_old VALUES ('seg1','tA'), ('seg2','tB');
             INSERT INTO focus_segments_new VALUES ('seg1','tB'), ('seg2','tA');",
        )
        .expect("fixture");
        let result = assert_child_task_links_preserved(&tx, "focus_segments_old", "focus_segments_new");
        assert!(result.is_err(), "a swapped task_id must fail the conservation audit");
    }

    #[test]
    fn migrates_v2_to_v3_with_default_tags_and_backfill() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-v3-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");
        build_v2_database(&db_path);

        let conn = open_at(&db_path).expect("v3 upgrade should succeed");
        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);

        // Four system tags seeded once, with exactly one fallback ("其他").
        let tags: Vec<(String, String, i64)> = conn
            .prepare("SELECT id, name, is_fallback FROM tags ORDER BY sort_order")
            .expect("prepare")
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");
        assert_eq!(
            tags,
            vec![
                ("system-study".to_owned(), "学习".to_owned(), 0),
                ("system-work".to_owned(), "工作".to_owned(), 0),
                ("system-life".to_owned(), "生活".to_owned(), 0),
                ("system-other".to_owned(), "其他".to_owned(), 1),
            ]
        );

        // Existing task backfilled to the fallback tag (never guessed).
        let task_tag: String = conn
            .query_row("SELECT tag_id FROM tasks WHERE id = 't1'", [], |r| r.get(0))
            .expect("task tag");
        assert_eq!(task_tag, "system-other");

        // Session backfill per the v1.1 qualification rules.
        let row = |id: &str| -> (String, i64, String, String) {
            conn.query_row(
                "SELECT finish_reason, statistics_eligible, qualification_reason, tag_name_snapshot
                 FROM sessions WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("session row")
        };
        assert_eq!(row("s-eligible"), ("legacy".into(), 1, "qualified".into(), "其他".into()));
        assert_eq!(row("s-short"), ("legacy".into(), 0, "too_short".into(), "其他".into()));
        assert_eq!(row("s-abandoned"), ("legacy".into(), 0, "abandoned".into(), "其他".into()));
        assert_eq!(row("s-break"), ("legacy".into(), 0, "non_focus".into(), "其他".into()));

        // timer_state gained the snapshot columns (empty while idle).
        let timer_tag: Option<String> = conn
            .query_row("SELECT tag_id FROM timer_state WHERE id = 1", [], |r| r.get(0))
            .expect("timer tag column exists");
        assert_eq!(timer_tag, None);

        // All pre-migration data survived.
        assert_eq!(count(&conn, "tasks"), 1);
        assert_eq!(count(&conn, "sessions"), 4);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v3_migration_is_idempotent_on_reopen() {
        let dir = std::env::temp_dir().join(format!(
            "abyssal-v3re-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let db_path = dir.join("abyssal-reverie.sqlite");
        build_v2_database(&db_path);

        let first = open_at(&db_path).expect("first open");
        let tags_first: i64 = count(&first, "tags");
        let sessions_first: i64 = count(&first, "sessions");
        drop(first);

        let second = open_at(&db_path).expect("reopen must be a no-op");
        assert_eq!(schema_version(&second).unwrap(), LATEST_SCHEMA_VERSION);
        assert_eq!(count(&second, "tags"), tags_first, "tags must not duplicate");
        assert_eq!(count(&second, "sessions"), sessions_first, "sessions must not duplicate");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sessions_v3_rejects_invalid_qualification_fields() {
        let conn = open_in_memory().expect("db");
        let base = "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot,
                    tag_id, tag_name_snapshot, mode, status, planned_seconds, focused_seconds,
                    started_at, ended_at, finish_reason, statistics_eligible, qualification_reason)
                    VALUES ('x', NULL, 't', 'P', 'system-other', '其他', 'focus', 'completed',
                    1500, 600, 1, 2, ?1, ?2, ?3)";

        assert!(
            conn.execute(base, params!["elapsed", 2i64, "qualified"]).is_err(),
            "statistics_eligible must stay 0/1"
        );
        assert!(
            conn.execute(base, params!["bogus", 1i64, "qualified"]).is_err(),
            "finish_reason must be from the allowed set"
        );
        assert!(
            conn.execute(base, params!["elapsed", 1i64, "bogus"]).is_err(),
            "qualification_reason must be from the allowed set"
        );
        assert!(
            conn.execute(
                "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot,
                    tag_id, tag_name_snapshot, mode, status, planned_seconds, focused_seconds,
                    started_at, ended_at, finish_reason, statistics_eligible, qualification_reason)
                 VALUES ('y', NULL, 't', 'P', 'system-other', '其他', 'focus', 'completed',
                    1500, -1, 1, 2, 'elapsed', 1, 'qualified')",
                [],
            )
            .is_err(),
            "focused_seconds must stay >= 0"
        );
    }

    // ─── A4: real-database upgrade drill (review #2 control) ──────────────────

    #[test]
    #[ignore = "A4 drill: upgrades a COPY of the real v1.0.0 database; run with cargo test drill_real -- --ignored --nocapture"]
    fn drill_real_v2_database_upgrade_to_v3() {
        let appdata = std::env::var("APPDATA").expect("APPDATA must be set");
        let source = std::path::PathBuf::from(appdata)
            .join("com.abyssalreverie.focus")
            .join("abyssal-reverie.sqlite");
        assert!(source.exists(), "real database not found at {}", source.display());

        let dir = std::env::temp_dir().join(format!(
            "abyssal-drill-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");

        // Copy the whole WAL set so the copy is a faithful snapshot.
        for ext in ["", "-wal", "-shm"] {
            let from = std::path::PathBuf::from(format!("{}{ext}", source.display()));
            if from.exists() {
                std::fs::copy(&from, dir.join(format!("abyssal-reverie.sqlite{ext}")))
                    .expect("copy must succeed");
            }
        }

        let copy_path = dir.join("abyssal-reverie.sqlite");
        let conn = open_at(&copy_path).expect("real database upgrade must succeed");
        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);

        eprintln!(
            "[drill] upgraded OK — tags={} tasks={} sessions={}",
            count(&conn, "tags"),
            count(&conn, "tasks"),
            count(&conn, "sessions")
        );
        eprintln!("[drill] upgraded copy preserved at {} for inspection", dir.display());
    }

    /// v1.3.0 acceptance drill: a COPY of the real v1.1.1 database (schema v3,
    /// preserved in release/v1.3.0/rehearsal/live-backup/) goes through the
    /// R06 flow — read-only preview, user-parameter migration, reconciliation,
    /// and the cancel path. The LIVE database is never touched: the user
    /// confirmed the real v3→v4 migration on 2026-09-06, so the live file is
    /// already v4 and this drill must stay reproducible regardless of it.
    #[test]
    #[ignore = "v1.3.0 drill: upgrades a COPY of the real v1.1.1 snapshot; run with cargo test drill_real_v3 -- --ignored --nocapture"]
    fn drill_real_v3_to_v4_with_preview_and_reconcile() {
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../release/v1.3.0/rehearsal/live-backup")
            .join("abyssal-reverie.sqlite");
        assert!(
            source.exists(),
            "rehearsal v3 snapshot not found at {} — restore release/v1.3.0/rehearsal/live-backup/ (sqlite + -wal + -shm) first",
            source.display()
        );

        let dir = std::env::temp_dir().join(format!(
            "abyssal-drill-v4-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        for ext in ["", "-wal", "-shm"] {
            let from = std::path::PathBuf::from(format!("{}{ext}", source.display()));
            if from.exists() {
                std::fs::copy(&from, dir.join(format!("abyssal-reverie.sqlite{ext}")))
                    .expect("copy must succeed");
            }
        }
        let copy_path = dir.join("abyssal-reverie.sqlite");

        // 0) Pre-state on the copy.
        {
            let pre = Connection::open(&copy_path).expect("pre-open");
            let v = schema_version(&pre).expect("version");
            eprintln!("[drill] source schema_version = {v}");
            assert_eq!(v, 3, "the rehearsal snapshot must be schema v3");
        }

        // 1) CANCEL path: open_prepared does NOT migrate; data stays v3.
        {
            let mut prep = open_prepared(&copy_path).expect("prep open");
            assert_eq!(schema_version(&prep).unwrap(), 3, "prep mode must not migrate");
            let preview = preview_v4_migration(&prep).expect("preview");
            eprintln!(
                "[drill] preview: tasks={} sessions={} projects_to_create={:?} general_tasks={} suggested_basis={}min",
                preview.task_count, preview.session_count,
                preview.projects_to_create, preview.general_task_count,
                preview.suggested_focus_minutes,
            );
            assert_eq!(schema_version(&prep).unwrap(), 3, "preview is read-only");
            let pre_tasks: i64 = prep.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0)).unwrap();
            let pre_sessions: i64 = prep.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0)).unwrap();
            prep.pragma_update(None, "journal_mode", "DELETE").ok(); // flush copy state
            drop(prep);
            eprintln!("[drill] cancel path OK — schema still v3, data intact (tasks={pre_tasks}, sessions={pre_sessions})");
        }

        // 2) CONFIRM path with the preview's suggested basis, standalone 通用.
        let params = crate::models::MigrationParams {
            budget_focus_minutes: 0, // 0 = read the basis from the snapshot's settings (real user data)
            general_mapping: "standalone".to_owned(),
        };
        {
            let mut conn = open_prepared(&copy_path).expect("reopen for migration");
            run_migrations_with(&mut conn, &params).expect("confirmed migration");
            seed_defaults(&conn).expect("seed");
            assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA_VERSION);

            // 3) Reconciliation.
            let tasks: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0)).unwrap();
            let sessions: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0)).unwrap();
            let eligible_sessions: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sessions WHERE statistics_eligible = 1 AND focused_seconds > 0",
                [], |r| r.get(0)).unwrap();
            let segments: i64 = conn.query_row(
                "SELECT COUNT(*) FROM focus_segments WHERE state = 'confirmed' AND temporal_precision = 'legacy_total_only'",
                [], |r| r.get(0)).unwrap();
            let session_seconds: i64 = conn.query_row(
                "SELECT COALESCE(SUM(focused_seconds),0) FROM sessions WHERE statistics_eligible = 1 AND focused_seconds > 0",
                [], |r| r.get(0)).unwrap();
            let segment_ms: i64 = conn.query_row(
                "SELECT COALESCE(SUM(effective_ms),0) FROM focus_segments WHERE state = 'confirmed' AND temporal_precision = 'legacy_total_only'",
                [], |r| r.get(0)).unwrap();
            let projects: i64 = conn.query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0)).unwrap();
            let budgets: i64 = conn.query_row(
                "SELECT COUNT(*) FROM budget_history WHERE source = 'migration'", [], |r| r.get(0)).unwrap();
            let unmapped: i64 = conn.query_row(
                "SELECT COUNT(*) FROM focus_segments WHERE project_id_snapshot IS NULL", [], |r| r.get(0)).unwrap();
            let fk_violations: usize = conn
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .count();

            eprintln!("[drill] post-migration: tasks={tasks} sessions={sessions} projects={projects} legacy_segments={segments} unmapped={unmapped} budget_rows={budgets} fk_violations={fk_violations}");
            eprintln!("[drill] reconcile: session_seconds={session_seconds}s segment_ms={segment_ms}ms");
            assert_eq!(segments, eligible_sessions, "every eligible session gets exactly one legacy record");
            assert_eq!(segment_ms, session_seconds * 1000, "×1000 unit conversion must be exact");
            assert_eq!(fk_violations, 0, "foreign_key_check must be clean");
            assert_eq!(budgets, tasks, "each migrated task gets exactly one 'migration' budget_history row");
        }

        eprintln!("[drill] upgraded copy preserved at {} for inspection", dir.display());
    }
}

/// Reads `user_version` from a THROWAWAY COPY of the {db, -wal, -shm} set so
/// the check sees committed WAL pages without opening the source for writing.
/// Returns `Ok(None)` when the source does not exist (fresh install).
pub fn probe_schema_version(path: &Path) -> Result<Option<u32>, CommandError> {
    if !path.exists() {
        return Ok(None);
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("abyssal-probe-{ts}"));
    std::fs::create_dir_all(&tmp).map_err(|err| {
        CommandError::internal(format!("probe: cannot create temp dir: {err}"))
    })?;
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if base.is_empty() {
        return Err(CommandError::internal("probe: database path has no file name"));
    }
    let mut copied_main = false;
    for name in [base.clone(), format!("{base}-wal"), format!("{base}-shm")] {
        let src = path.with_file_name(&name);
        if src.exists() {
            std::fs::copy(&src, tmp.join(&name)).map_err(|err| {
                CommandError::internal(format!("probe: cannot copy {name}: {err}"))
            })?;
            if name == base {
                copied_main = true;
            }
        }
    }
    if !copied_main {
        return Err(CommandError::internal("probe: main database file missing"));
    }
    let result = Connection::open(tmp.join(&base))
        .map_err(CommandError::from)
        .and_then(|conn| schema_version(&conn))
        .map(Some);
    let _ = std::fs::remove_dir_all(&tmp);
    result.map_err(|err| {
        CommandError::database(format!("probe: source database unreadable on the copy: {err}"))
    })
}
