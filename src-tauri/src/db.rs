use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, Transaction};

use crate::error::CommandError;
use crate::models::{AppSettings, TimerMode, TimerSnapshot};

/// Bump this whenever a new migration is appended to `MIGRATIONS`.
const LATEST_SCHEMA_VERSION: u32 = 4;

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

    // Phase 0 — v1.1.2 E1: refuse a database written by a NEWER version of
    // the app BEFORE any pragma or write (including the WAL journal switch in
    // `configure`) touches the file. The probe connection only reads
    // `user_version`; `Connection::open` itself never writes. A probe failure
    // (garbage/corrupt file) falls through to the normal corruption handling
    // in Phase 1/2.
    let probe = Connection::open(path).ok().and_then(|conn| schema_version(&conn).ok());
    if let Some(version) = probe {
        if version > LATEST_SCHEMA_VERSION {
            return Err(CommandError::database_too_new(version, LATEST_SCHEMA_VERSION));
        }
    }

    // Phase 1 — open + configure. An unreadable/unopenable file is treated as
    // corrupt media and goes through the recovery path.
    let mut conn = match open_and_configure(path) {
        Ok(conn) => conn,
        Err(_) => {
            recover_corrupt(path)?;
            return fresh_database(path);
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
fn open_and_configure(path: &Path) -> Result<Connection, CommandError> {
    let conn = Connection::open(path)?;
    configure(&conn, true)?;
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
    let mut conn = open_and_configure(path)?;
    migrate_and_seed(&mut conn)?;
    Ok(conn)
}

/// Copies the database file aside as `<name>.pre-v<target>-<timestamp>.bak`
/// before an in-place schema upgrade, so a failed/corrupted upgrade always has
/// a restorable snapshot. The WAL is checkpointed first so the copy includes
/// every committed transaction.
fn backup_before_migration(conn: &Connection, path: &Path) -> Result<(), CommandError> {
    let _ = conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", []);
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
    let backup = path.with_file_name(format!("{base}.pre-v{LATEST_SCHEMA_VERSION}-{ts}.bak"));
    std::fs::copy(path, &backup).map_err(|err| {
        CommandError::internal(format!(
            "failed to create pre-migration backup {}: {err}",
            backup.display()
        ))
    })?;
    Ok(())
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
fn configure(conn: &Connection, persistent: bool) -> Result<(), CommandError> {
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
    // under the fallback category. `通用` and blank stay standalone.
    tx.execute(
        "INSERT INTO projects (id, profile_id, category_id, name, normalized_name, sort_order, created_at, updated_at)
         SELECT 'prj-' || hex(randomblob(8)), 'local',
                (SELECT id FROM categories WHERE profile_id = 'local' AND name = '其他'),
                t.pname, t.pname,
                ROW_NUMBER() OVER (ORDER BY t.pname) - 1, ?1, ?1
         FROM (SELECT DISTINCT TRIM(project) AS pname FROM tasks) t
         WHERE t.pname <> '' AND t.pname <> '通用'",
        params![now],
    )?;
    tx.execute(
        "UPDATE tasks SET project_id = (
            SELECT p.id FROM projects p
            WHERE p.profile_id = 'local' AND p.normalized_name = TRIM(tasks.project)
         )
         WHERE TRIM(project) <> '' AND TRIM(project) <> '通用'",
        [],
    )?;

    // Status mirrors the legacy done flag.
    tx.execute(
        "UPDATE tasks SET status = CASE WHEN done = 1 THEN 'done' ELSE 'todo' END",
        [],
    )?;

    // Budget snapshot estimated from the user's current focus duration.
    tx.execute(
        "UPDATE tasks
         SET target_seconds = pomodoro_target *
             COALESCE((SELECT focus_duration_minutes FROM settings WHERE id = 1), 25) * 60",
        [],
    )?;
    tx.execute(
        "INSERT INTO budget_history (id, task_id, old_target_seconds, new_target_seconds, source, created_at)
         SELECT 'bh-' || hex(randomblob(8)), id, NULL, target_seconds, 'migration', ?1 FROM tasks",
        params![now],
    )?;

    // One confirmed segment per eligible historical session (effective time
    // = focused_seconds; project/category snapshots best-effort from the
    // migrated task).
    tx.execute(
        "INSERT INTO focus_segments (id, profile_id, session_id, task_id,
                                     project_id_snapshot, project_name_snapshot, category_id_snapshot,
                                     effective_start_ms, effective_end_ms, effective_ms, state, created_at)
         SELECT 'seg-' || hex(randomblob(8)), 'local', s.id, s.task_id,
                t.project_id, s.project_snapshot,
                (SELECT p.category_id FROM projects p WHERE p.id = t.project_id),
                s.started_at, s.started_at + s.focused_seconds * 1000, s.focused_seconds * 1000,
                'confirmed', s.ended_at
         FROM sessions s LEFT JOIN tasks t ON t.id = s.task_id
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
        assert_eq!(schema_version(&conn).unwrap(), 4);

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
}
