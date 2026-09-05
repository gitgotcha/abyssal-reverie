use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::CommandError;
use crate::models::{
    AppSettings, CompleteTimerInput, CompleteTimerResult, CreateTagInput, CreateTaskInput, DayStat,
    BackupHeader, DeleteTagResult, ExportBundle, ExportBundleV1, FinishTimerInput,
    FinishTimerResult, ImportPreview, ImportSummary, ProjectStat,
    SaveSettingsResult, SessionQuery, SessionStatus, SessionV1, StartTimerInput, Statistics,
    StatisticsQuery, SwitchTimerModeInput, SwitchTimerTaskInput, SwitchTimerTaskResult, Tag,
    TagDeletePreview, TagKind, Task, TaskPriority,
    TimerMode, TimerSession, TimerSnapshot, TimerState, UpdateTagInput, UpdateTaskInput,
};

// ─── Fixed snapshot labels (design spec §3) ──────────────────────────────────
pub const NO_TASK_TITLE: &str = "未指定任务";
pub const NO_TASK_PROJECT: &str = "通用";
pub const SHORT_BREAK_TITLE: &str = "短休";
pub const LONG_BREAK_TITLE: &str = "长休";
pub const BREAK_PROJECT: &str = "休息";

pub const MIN_POMODORO_TARGET: i64 = 1;
pub const MAX_POMODORO_TARGET: i64 = 99;
pub const MAX_TITLE_CHARS: usize = 200;
/// Tag display-name limit, counted in Unicode characters (not bytes — Chinese
/// must not be miscounted).
pub const MAX_TAG_NAME_CHARS: usize = 20;
/// Hard cap on the number of tags, including the system tags. Prevents
/// meaningless unbounded growth (spec §15).
pub const MAX_TAGS: usize = 100;
pub const DEFAULT_PROJECT: &str = "通用";

pub const MIN_DURATION_MINUTES: i64 = 1;
pub const MAX_DURATION_MINUTES: i64 = 180;
pub const MIN_DAILY_GOAL: i64 = 1;
pub const MAX_DAILY_GOAL: i64 = 50;

/// Backup bundle identity + version (Item 3: data export & backup).
pub const EXPORT_APP_NAME: &str = "abyssal-reverie";
pub const EXPORT_SCHEMA_VERSION: u32 = 2;

const TASK_COLUMNS: &str = "id, title, done, pomodoro_target, priority, project, tag_id, \
                            sort_order, created_at, updated_at, completed_at";
const SESSION_COLUMNS: &str = "id, task_id, task_title_snapshot, project_snapshot, tag_id, \
                               tag_name_snapshot, mode, status, planned_seconds, focused_seconds, \
                               started_at, ended_at, finish_reason, statistics_eligible, \
                               qualification_reason";

/// Focus sessions shorter than this never enter statistics (v1.1 rule: 29s
/// excluded, 30s counted). Rust is the only place this rule lives.
pub const MIN_QUALIFYING_FOCUS_SECONDS: i64 = 30;

/// Natural completion (`complete_timer`) is allowed while the countdown is
/// within this many milliseconds of its deadline — a small tolerance that
/// absorbs timer/scheduler jitter. Anything earlier must use `finish_timer`
/// (manual "结束") or `reset_timer` (v1.1 ruling, review round 3).
pub const COMPLETE_SCHEDULING_TOLERANCE_MS: i64 = 250;

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn clean_title(raw: &str) -> String {
    raw.trim().chars().take(MAX_TITLE_CHARS).collect()
}

fn clean_project(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        DEFAULT_PROJECT.to_owned()
    } else {
        trimmed.to_owned()
    }
}

pub fn validate_title(raw: &str) -> Result<(), CommandError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CommandError::validation("title must not be empty"));
    }
    if trimmed.chars().count() > MAX_TITLE_CHARS {
        return Err(CommandError::validation(format!(
            "title must be at most {MAX_TITLE_CHARS} characters"
        )));
    }
    Ok(())
}

pub fn validate_pomodoro_target(target: i64) -> Result<(), CommandError> {
    if (MIN_POMODORO_TARGET..=MAX_POMODORO_TARGET).contains(&target) {
        Ok(())
    } else {
        Err(CommandError::validation(format!(
            "pomodoroTarget must be between {MIN_POMODORO_TARGET} and {MAX_POMODORO_TARGET}"
        )))
    }
}

// ─── Row mapping ─────────────────────────────────────────────────────────────

fn task_from_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    let priority_text: String = row.get("priority")?;
    Ok(Task {
        id: row.get("id")?,
        title: row.get("title")?,
        done: row.get::<_, i64>("done")? != 0,
        pomodoro_target: row.get("pomodoro_target")?,
        priority: TaskPriority::parse_str(&priority_text).unwrap_or(TaskPriority::Med),
        project: row.get("project")?,
        tag_id: row.get("tag_id")?,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        completed_at: row.get("completed_at")?,
    })
}

fn session_from_row(row: &Row<'_>) -> rusqlite::Result<TimerSession> {
    let mode_text: String = row.get("mode")?;
    let status_text: String = row.get("status")?;
    let eligible: i64 = row.get("statistics_eligible")?;
    Ok(TimerSession {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        task_title_snapshot: row.get("task_title_snapshot")?,
        project_snapshot: row.get("project_snapshot")?,
        tag_id: row.get("tag_id")?,
        tag_name_snapshot: Some(row.get("tag_name_snapshot")?),
        mode: TimerMode::parse_str(&mode_text).unwrap_or(TimerMode::Focus),
        status: SessionStatus::parse_str(&status_text).unwrap_or(SessionStatus::Abandoned),
        planned_seconds: row.get("planned_seconds")?,
        focused_seconds: row.get("focused_seconds")?,
        started_at: row.get("started_at")?,
        ended_at: row.get("ended_at")?,
        finish_reason: Some(row.get("finish_reason")?),
        statistics_eligible: Some(eligible != 0),
        qualification_reason: Some(row.get("qualification_reason")?),
    })
}

/// Stable fallback tag (id, name). The fallback can be renamed but never
/// deleted, so its id is constant; only its name may change over time.
fn fallback_tag(conn: &Connection) -> Result<(String, String), CommandError> {
    conn.query_row(
        "SELECT id, name FROM tags WHERE is_fallback = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|err| CommandError::internal(format!("fallback tag missing: {err}")))
}

/// Computes the effective qualification fields for a session being written,
/// honoring explicit values from a v2 backup and backfilling v1-shaped data
/// per the v1.1 rules (spec §7.3).
fn effective_qualification(
    session: &TimerSession,
) -> (String, i64, String) {
    let focused = session.focused_seconds.max(0);
    let eligible = session.statistics_eligible.unwrap_or(
        session.mode == TimerMode::Focus
            && session.status == SessionStatus::Completed
            && focused >= MIN_QUALIFYING_FOCUS_SECONDS,
    );
    let qualification = session.qualification_reason.clone().unwrap_or_else(|| {
        if session.mode != TimerMode::Focus {
            "non_focus".to_owned()
        } else if eligible && session.status == SessionStatus::Completed {
            "qualified".to_owned()
        } else if session.status == SessionStatus::Abandoned {
            if focused < MIN_QUALIFYING_FOCUS_SECONDS {
                "too_short".to_owned()
            } else {
                "abandoned".to_owned()
            }
        } else {
            "too_short".to_owned()
        }
    });
    let finish = session
        .finish_reason
        .clone()
        .unwrap_or_else(|| "legacy".to_owned());
    (finish, if eligible { 1 } else { 0 }, qualification)
}

// ─── Tasks ───────────────────────────────────────────────────────────────────

pub fn list_tasks(conn: &Connection) -> Result<Vec<Task>, CommandError> {
    let mut stmt =
        conn.prepare(&format!("SELECT {TASK_COLUMNS} FROM tasks ORDER BY sort_order, created_at"))?;
    let rows = stmt.query_map([], task_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_task(conn: &Connection, id: &str) -> Result<Task, CommandError> {
    conn.query_row(
        &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
        params![id],
        task_from_row,
    )
    .optional()?
    .ok_or_else(|| CommandError::not_found(format!("task {id} not found")))
}

pub fn insert_task(conn: &Connection, input: &CreateTaskInput) -> Result<Task, CommandError> {
    validate_title(&input.title)?;
    validate_pomodoro_target(input.pomodoro_target)?;

    let now = now_millis();
    let sort_order: i64 = conn
        .query_row("SELECT COALESCE(MAX(sort_order) + 1, 0) FROM tasks", [], |row| row.get(0))
        .unwrap_or(0);
    // F4 lets the user pick the tag; empty/legacy callers land on the
    // fallback tag.
    let (fallback_id, _fallback_name) = fallback_tag(conn)?;
    let tag_id = if input.tag_id.is_empty() { fallback_id } else { input.tag_id.clone() };

    let task = Task {
        id: Uuid::new_v4().to_string(),
        title: clean_title(&input.title),
        done: false,
        pomodoro_target: input.pomodoro_target,
        priority: input.priority,
        project: clean_project(&input.project),
        tag_id,
        sort_order,
        created_at: now,
        updated_at: now,
        completed_at: None,
    };

    conn.execute(
        "INSERT INTO tasks (id, title, done, pomodoro_target, priority, project, tag_id,
                            sort_order, created_at, updated_at, completed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            task.id,
            task.title,
            task.done as i64,
            task.pomodoro_target,
            task.priority.as_str(),
            task.project,
            task.tag_id,
            task.sort_order,
            task.created_at,
            task.updated_at,
            task.completed_at,
        ],
    )?;

    Ok(task)
}

pub fn update_task(conn: &Connection, input: &UpdateTaskInput) -> Result<Task, CommandError> {
    let mut task = get_task(conn, &input.id)?;

    if let Some(title) = &input.title {
        validate_title(title)?;
        task.title = clean_title(title);
    }
    if let Some(target) = input.pomodoro_target {
        validate_pomodoro_target(target)?;
        task.pomodoro_target = target;
    }
    if let Some(priority) = input.priority {
        task.priority = priority;
    }
    if let Some(project) = &input.project {
        task.project = clean_project(project);
    }
    if let Some(done) = input.done {
        if done != task.done {
            task.done = done;
            task.completed_at = if done { Some(now_millis()) } else { None };
        }
    }

    task.updated_at = now_millis();

    conn.execute(
        "UPDATE tasks SET title = ?1, done = ?2, pomodoro_target = ?3, priority = ?4,
                          project = ?5, updated_at = ?6, completed_at = ?7
         WHERE id = ?8",
        params![
            task.title,
            task.done as i64,
            task.pomodoro_target,
            task.priority.as_str(),
            task.project,
            task.updated_at,
            task.completed_at,
            task.id,
        ],
    )?;

    Ok(task)
}

pub fn delete_task(conn: &Connection, id: &str) -> Result<(), CommandError> {
    let removed = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
    if removed == 0 {
        return Err(CommandError::not_found(format!("task {id} not found")));
    }
    Ok(())
}

// ─── Tags (v1.1, spec §9) ────────────────────────────────────────────────────

fn tag_from_row(row: &Row<'_>) -> rusqlite::Result<Tag> {
    let kind_text: String = row.get("kind")?;
    Ok(Tag {
        id: row.get("id")?,
        name: row.get("name")?,
        kind: TagKind::parse_str(&kind_text).unwrap_or(TagKind::Custom),
        is_fallback: row.get::<_, i64>("is_fallback")? != 0,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// Validates and trims a tag display name. Length is measured in Unicode
/// characters so Chinese names are not miscounted as bytes (spec §9.2).
fn validate_tag_name(raw: &str) -> Result<String, CommandError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CommandError::validation("标签名称不能为空"));
    }
    let char_count = trimmed.chars().count();
    if char_count > MAX_TAG_NAME_CHARS {
        return Err(CommandError::validation(format!(
            "标签名称不能超过 {MAX_TAG_NAME_CHARS} 个字符"
        )));
    }
    if trimmed.chars().any(|c| c.is_control() || c == '\n' || c == '\r') {
        return Err(CommandError::validation("标签名称不能包含控制字符或换行符"));
    }
    Ok(trimmed.to_owned())
}

/// Returns the tags in display order (spec §11.5: the UI renders them as-is).
pub fn list_tags(conn: &Connection) -> Result<Vec<Tag>, CommandError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, kind, is_fallback, sort_order, created_at, updated_at
         FROM tags ORDER BY sort_order, created_at",
    )?;
    let rows = stmt.query_map([], tag_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn create_tag(conn: &Connection, input: &CreateTagInput) -> Result<Tag, CommandError> {
    let name = validate_tag_name(&input.name)?;
    let normalized = name.to_lowercase();

    let count: i64 = conn.query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))?;
    if count as usize >= MAX_TAGS {
        return Err(CommandError::validation(format!(
            "标签数量已达上限（{MAX_TAGS} 个）"
        )));
    }

    let duplicate: Option<String> = conn
        .query_row(
            "SELECT id FROM tags WHERE normalized_name = ?1",
            params![normalized],
            |row| row.get(0),
        )
        .optional()?;
    if duplicate.is_some() {
        return Err(CommandError::validation(format!("标签“{name}”已存在")));
    }

    let now = now_millis();
    let sort_order: i64 = conn
        .query_row("SELECT COALESCE(MAX(sort_order) + 1, 0) FROM tags", [], |row| row.get(0))
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO tags (id, name, normalized_name, kind, is_fallback, sort_order,
                           created_at, updated_at)
         VALUES (?1, ?2, ?3, 'custom', 0, ?4, ?5, ?5)",
        params![Uuid::new_v4().to_string(), name, normalized, sort_order, now],
    )?;

    Ok(list_tags(conn)?.into_iter().find(|t| t.name == name).expect("tag just inserted"))
}

pub fn update_tag(conn: &Connection, input: &UpdateTagInput) -> Result<Tag, CommandError> {
    let mut tag = conn
        .query_row(
            "SELECT id, name, kind, is_fallback, sort_order, created_at, updated_at
             FROM tags WHERE id = ?1",
            params![input.id],
            tag_from_row,
        )
        .optional()?
        .ok_or_else(|| CommandError::not_found(format!("tag {} not found", input.id)))?;

    if let Some(name) = &input.name {
        let cleaned = validate_tag_name(name)?;
        let normalized = cleaned.to_lowercase();
        let duplicate: Option<String> = conn
            .query_row(
                "SELECT id FROM tags WHERE normalized_name = ?1 AND id != ?2",
                params![normalized, input.id],
                |row| row.get(0),
            )
            .optional()?;
        if duplicate.is_some() {
            return Err(CommandError::validation(format!("标签“{cleaned}”已存在")));
        }
        tag.name = cleaned;
        tag.updated_at = now_millis();
        conn.execute(
            "UPDATE tags SET name = ?1, normalized_name = ?2, updated_at = ?3 WHERE id = ?4",
            params![tag.name, normalized, tag.updated_at, tag.id],
        )?;
    }

    Ok(tag)
}

/// Swaps the tag's sort order with its neighbour (up = earlier, down = later).
/// At a boundary the call is a no-op. Returns the updated list.
pub fn reorder_tag(
    conn: &Connection,
    input: &crate::models::ReorderTagInput,
) -> Result<Vec<Tag>, CommandError> {
    let (id, sort_order): (String, i64) = conn
        .query_row(
            "SELECT id, sort_order FROM tags WHERE id = ?1",
            params![input.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CommandError::not_found(format!("tag {} not found", input.id)))?;

    let neighbour: Option<(String, i64)> = if input.direction < 0 {
        conn.query_row(
            "SELECT id, sort_order FROM tags WHERE sort_order < ?1
             ORDER BY sort_order DESC LIMIT 1",
            params![sort_order],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
    } else {
        conn.query_row(
            "SELECT id, sort_order FROM tags WHERE sort_order > ?1
             ORDER BY sort_order ASC LIMIT 1",
            params![sort_order],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
    };

    if let Some((neighbour_id, neighbour_sort)) = neighbour {
        let tx = conn.unchecked_transaction()?;
        tx.execute("UPDATE tags SET sort_order = ?1 WHERE id = ?2", params![neighbour_sort, id])?;
        tx.execute("UPDATE tags SET sort_order = ?1 WHERE id = ?2", params![sort_order, neighbour_id])?;
        tx.commit()?;
    }

    list_tags(conn)
}

/// Reports how many tasks reference the tag, for the delete confirmation
/// dialog. Deleting the fallback tag is rejected here as well so the UI can
/// surface it before the destructive call.
pub fn preview_delete_tag(conn: &Connection, id: &str) -> Result<TagDeletePreview, CommandError> {
    let tag = conn
        .query_row(
            "SELECT id, name, kind, is_fallback, sort_order, created_at, updated_at
             FROM tags WHERE id = ?1",
            params![id],
            tag_from_row,
        )
        .optional()?
        .ok_or_else(|| CommandError::not_found(format!("tag {id} not found")))?;
    if tag.is_fallback {
        return Err(CommandError::conflict("保底标签不能删除"));
    }
    let affected_tasks: i64 =
        conn.query_row("SELECT COUNT(*) FROM tasks WHERE tag_id = ?1", params![id], |row| {
            row.get(0)
        })?;
    Ok(TagDeletePreview { tag_id: tag.id, affected_tasks })
}

/// Two-phase delete per spec §9.3: the UI confirms with the affected count,
/// then this runs everything in ONE transaction — reassign the tag's tasks to
/// the fallback tag, let the FKs null out timer/session references (snapshots
/// are preserved), and delete the tag. The affected count is recomputed here;
/// the preview value is never trusted.
pub fn delete_tag(conn: &mut Connection, id: &str) -> Result<DeleteTagResult, CommandError> {
    let tx = conn.transaction()?;

    let tag = tx
        .query_row(
            "SELECT id, name, kind, is_fallback, sort_order, created_at, updated_at
             FROM tags WHERE id = ?1",
            params![id],
            tag_from_row,
        )
        .optional()?
        .ok_or_else(|| CommandError::not_found(format!("tag {id} not found")))?;
    if tag.is_fallback {
        return Err(CommandError::conflict("保底标签不能删除"));
    }

    let (fallback_id, _fallback_name) = fallback_tag(&tx)?;
    let reassigned = tx.execute(
        "UPDATE tasks SET tag_id = ?1, updated_at = ?2 WHERE tag_id = ?3",
        params![fallback_id, now_millis(), id],
    )?;

    // tasks.tag_id is RESTRICT, so the tag can only be deleted after the
    // reassignment above. timer_state.tag_id and sessions.tag_id are
    // SET NULL — snapshots (tag_name_snapshot) are never touched.
    tx.execute("DELETE FROM tags WHERE id = ?1", params![id])?;

    // The FK-based nulling happens on DELETE; assert it actually did.
    let dangling_timer: i64 = tx.query_row(
        "SELECT COUNT(*) FROM timer_state WHERE tag_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    if dangling_timer > 0 {
        return Err(CommandError::database(
            "timer_state still references the deleted tag after delete_tag"
        ));
    }

    tx.commit()?;

    Ok(DeleteTagResult {
        deleted_tag_id: id.to_owned(),
        fallback_tag_id: fallback_id,
        reassigned_tasks: reassigned as i64,
        tags: list_tags(conn)?,
        tasks: list_tasks(conn)?,
    })
}

// ─── Settings, timer and sessions ────────────────────────────────────────────

pub fn get_settings(conn: &Connection) -> Result<AppSettings, CommandError> {
    conn.query_row(
        "SELECT focus_duration_minutes, short_break_minutes, long_break_minutes, auto_start_break,
                sound_enabled, notification_enabled, daily_goal, reduce_motion, updated_at
         FROM settings WHERE id = 1",
        [],
        |row| {
            Ok(AppSettings {
                focus_duration_minutes: row.get(0)?,
                short_break_minutes: row.get(1)?,
                long_break_minutes: row.get(2)?,
                auto_start_break: row.get::<_, i64>(3)? != 0,
                sound_enabled: row.get::<_, i64>(4)? != 0,
                notification_enabled: row.get::<_, i64>(5)? != 0,
                daily_goal: row.get(6)?,
                reduce_motion: row.get::<_, i64>(7)? != 0,
                updated_at: row.get(8)?,
            })
        },
    )
    .map_err(Into::into)
}

pub fn validate_settings(settings: &AppSettings) -> Result<(), CommandError> {
    let check_range = |value: i64, min: i64, max: i64, name: &str| {
        if (min..=max).contains(&value) {
            Ok(())
        } else {
            Err(CommandError::validation(format!(
                "{name} must be between {min} and {max}"
            )))
        }
    };

    check_range(
        settings.focus_duration_minutes,
        MIN_DURATION_MINUTES,
        MAX_DURATION_MINUTES,
        "focusDurationMinutes",
    )?;
    check_range(
        settings.short_break_minutes,
        MIN_DURATION_MINUTES,
        MAX_DURATION_MINUTES,
        "shortBreakMinutes",
    )?;
    check_range(
        settings.long_break_minutes,
        MIN_DURATION_MINUTES,
        MAX_DURATION_MINUTES,
        "longBreakMinutes",
    )?;
    check_range(
        settings.daily_goal,
        MIN_DAILY_GOAL,
        MAX_DAILY_GOAL,
        "dailyGoal",
    )?;
    Ok(())
}

/// Persists settings and returns the updated settings plus a refreshed timer.
///
/// If the timer is idle, its `durationSeconds` / `remainingSeconds` are
/// recalculated from the new durations so the UI immediately reflects the
/// change. A running or paused timer is left untouched — the new durations
/// take effect on the next session.
pub fn save_settings(
    conn: &Connection,
    settings: &AppSettings,
) -> Result<SaveSettingsResult, CommandError> {
    validate_settings(settings)?;

    let now = now_millis();
    conn.execute(
        "UPDATE settings SET focus_duration_minutes = ?1, short_break_minutes = ?2,
                              long_break_minutes = ?3, auto_start_break = ?4,
                              sound_enabled = ?5, notification_enabled = ?6,
                              daily_goal = ?7, reduce_motion = ?8, updated_at = ?9
         WHERE id = 1",
        params![
            settings.focus_duration_minutes,
            settings.short_break_minutes,
            settings.long_break_minutes,
            settings.auto_start_break as i64,
            settings.sound_enabled as i64,
            settings.notification_enabled as i64,
            settings.daily_goal,
            settings.reduce_motion as i64,
            now,
        ],
    )?;

    let mut timer = get_timer(conn)?;
    if timer.state == TimerState::Idle {
        let new_duration = settings.duration_seconds_for_mode(timer.mode);
        timer.duration_seconds = new_duration;
        timer.remaining_seconds = new_duration;
        timer.updated_at = now;
        write_timer(conn, &timer)?;
    }

    let persisted = get_settings(conn)?;
    Ok(SaveSettingsResult {
        settings: persisted,
        timer,
    })
}

fn write_timer(conn: &Connection, timer: &TimerSnapshot) -> Result<(), CommandError> {
    conn.execute(
        "UPDATE timer_state SET mode = ?1, state = ?2, active_session_id = ?3,
                                selected_task_id = ?4, task_title_snapshot = ?5,
                                project_snapshot = ?6, tag_id = ?7, tag_name_snapshot = ?8,
                                duration_seconds = ?9, remaining_seconds = ?10,
                                started_at = ?11, target_end_at = ?12,
                                paused_at = ?13, revision = ?14, updated_at = ?15
         WHERE id = 1",
        params![
            timer.mode.as_str(),
            timer.state.as_str(),
            timer.active_session_id,
            timer.selected_task_id,
            timer.task_title_snapshot,
            timer.project_snapshot,
            timer.tag_id,
            timer.tag_name_snapshot,
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

pub fn get_timer(conn: &Connection) -> Result<TimerSnapshot, CommandError> {
    conn.query_row(
        "SELECT mode, state, active_session_id, selected_task_id, task_title_snapshot,
                project_snapshot, tag_id, tag_name_snapshot, duration_seconds, remaining_seconds,
                started_at, target_end_at, paused_at, revision, updated_at
         FROM timer_state WHERE id = 1",
        [],
        |row| {
            let mode_text: String = row.get(0)?;
            let state_text: String = row.get(1)?;
            Ok(TimerSnapshot {
                mode: TimerMode::parse_str(&mode_text).unwrap_or(TimerMode::Focus),
                state: TimerState::parse_str(&state_text).unwrap_or(TimerState::Idle),
                active_session_id: row.get(2)?,
                selected_task_id: row.get(3)?,
                task_title_snapshot: row.get(4)?,
                project_snapshot: row.get(5)?,
                tag_id: row.get(6)?,
                tag_name_snapshot: row.get(7)?,
                duration_seconds: row.get(8)?,
                remaining_seconds: row.get(9)?,
                started_at: row.get(10)?,
                target_end_at: row.get(11)?,
                paused_at: row.get(12)?,
                revision: row.get(13)?,
                updated_at: row.get(14)?,
            })
        },
    )
    .map_err(Into::into)
}

/// Activity-view listing: bootstrap and user-visible pages see only
/// statistics-eligible focus sessions (v1.1 ruling). Hidden records survive
/// in exports (`scope = all`).
pub fn list_sessions(conn: &Connection, limit: i64) -> Result<Vec<TimerSession>, CommandError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SESSION_COLUMNS} FROM sessions WHERE statistics_eligible = 1
         ORDER BY started_at DESC, rowid DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![limit], session_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

// ─── Timer state machine (design spec §4) ────────────────────────────────────

/// Computes the snapshot title/project for a timer start, per spec §3.
fn snapshot_for_mode(mode: TimerMode, task: Option<&Task>) -> (String, String) {
    match mode {
        TimerMode::Focus => {
            if let Some(t) = task {
                (t.title.clone(), t.project.clone())
            } else {
                (NO_TASK_TITLE.to_owned(), NO_TASK_PROJECT.to_owned())
            }
        }
        TimerMode::Short => (SHORT_BREAK_TITLE.to_owned(), BREAK_PROJECT.to_owned()),
        TimerMode::Long => (LONG_BREAK_TITLE.to_owned(), BREAK_PROJECT.to_owned()),
    }
}

/// Live remaining seconds for a running timer, derived from `target_end_at`.
/// Rounds UP (ceiling) per spec §8.1 so a 29.x-second focus can never be
/// misjudged as 30 seconds at settlement time.
fn live_remaining(timer: &TimerSnapshot, now: i64) -> i64 {
    match timer.target_end_at {
        Some(end) => (((end - now) + 999) / 1000).max(0),
        None => timer.remaining_seconds,
    }
}

/// Writes a finished (completed or abandoned) session for a timer that was
/// started. Uses the timer's `active_session_id` so the session is traceable
/// to its start. The tag snapshot comes from the timer (frozen at start) with
/// the fallback tag as a safety net.
fn write_finished_session(
    conn: &Connection,
    timer: &TimerSnapshot,
    now: i64,
    status: SessionStatus,
    focused_seconds: i64,
    finish_reason: &str,
) -> Result<(), CommandError> {
    let session_id = timer
        .active_session_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let focused = focused_seconds.max(0);
    // Reset-created sessions are internal by ruling: never eligible, never in
    // the activity view, only preserved in full exports.
    let eligible =
        timer.mode == TimerMode::Focus && status == SessionStatus::Completed && focused >= MIN_QUALIFYING_FOCUS_SECONDS;
    let qualification = if timer.mode != TimerMode::Focus {
        "non_focus"
    } else if status == SessionStatus::Abandoned {
        if focused < MIN_QUALIFYING_FOCUS_SECONDS {
            "too_short"
        } else {
            "abandoned"
        }
    } else if eligible {
        "qualified"
    } else {
        "too_short"
    };
    let (fallback_id, fallback_name) = fallback_tag(conn)?;
    let session = TimerSession {
        id: session_id,
        task_id: timer.selected_task_id.clone(),
        task_title_snapshot: timer.task_title_snapshot.clone().unwrap_or_default(),
        project_snapshot: timer.project_snapshot.clone().unwrap_or_default(),
        tag_id: Some(timer.tag_id.clone().unwrap_or_else(|| fallback_id.clone())),
        tag_name_snapshot: Some(
            timer
                .tag_name_snapshot
                .clone()
                .unwrap_or_else(|| fallback_name.clone()),
        ),
        mode: timer.mode,
        status,
        planned_seconds: timer.duration_seconds,
        focused_seconds: focused,
        started_at: timer.started_at.unwrap_or(now),
        ended_at: now,
        finish_reason: Some(finish_reason.to_owned()),
        statistics_eligible: Some(eligible),
        qualification_reason: Some(qualification.to_owned()),
    };
    conn.execute(
        "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                               tag_name_snapshot, mode, status, planned_seconds,
                               focused_seconds, started_at, ended_at, finish_reason,
                               statistics_eligible, qualification_reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            session.id,
            session.task_id,
            session.task_title_snapshot,
            session.project_snapshot,
            session.tag_id,
            session.tag_name_snapshot,
            session.mode.as_str(),
            session.status.as_str(),
            session.planned_seconds,
            session.focused_seconds,
            session.started_at,
            session.ended_at,
            finish_reason,
            eligible as i64,
            qualification,
        ],
    )?;
    Ok(())
}

/// Writes the internal record a user-initiated reset produces (v1.1 ruling):
/// abandoned, `finish_reason = reset`, never eligible, hidden from the
/// activity view, preserved only in full exports.
fn write_reset_session(
    conn: &Connection,
    timer: &TimerSnapshot,
    now: i64,
) -> Result<(), CommandError> {
    let focused = timer.duration_seconds - timer.remaining_seconds;
    write_finished_session(conn, timer, now, SessionStatus::Abandoned, focused, "reset")
}

/// Checks that the timer's revision matches `expected_revision`, else CONFLICT.
fn check_revision(timer: &TimerSnapshot, expected: i64) -> Result<(), CommandError> {
    if timer.revision != expected {
        return Err(CommandError::conflict(format!(
            "timer revision mismatch: expected {expected}, found {}",
            timer.revision
        )));
    }
    Ok(())
}

/// `start_timer`: idle/done → running. Copies task snapshots, generates a
/// session UUID, computes `target_end_at`, bumps revision.
pub fn start_timer(
    conn: &mut Connection,
    settings: &AppSettings,
    input: &StartTimerInput,
) -> Result<TimerSnapshot, CommandError> {
    let tx = conn.transaction()?;
    let mut timer = get_timer(&tx)?;
    check_revision(&timer, input.expected_revision)?;

    if timer.state != TimerState::Idle && timer.state != TimerState::Done {
        return Err(CommandError::validation(format!(
            "start_timer requires idle or done state, found {:?}",
            timer.state
        )));
    }

    let task = match (&input.selected_task_id, input.mode) {
        (Some(id), TimerMode::Focus) => Some(get_task(&tx, id)?),
        _ => None,
    };
    let (title_snap, project_snap) = snapshot_for_mode(input.mode, task.as_ref());
    // v1.1: freeze the tag alongside title/project. A selected task donates
    // its tag; breaks and no-task rounds use the fallback tag. Mid-run tag
    // changes never alter this snapshot.
    let (tag_id, tag_name) = match (&input.selected_task_id, input.mode) {
        (Some(_), TimerMode::Focus) => {
            let task = task.as_ref().expect("task resolved above");
            let name: String = tx.query_row(
                "SELECT name FROM tags WHERE id = ?1",
                params![task.tag_id],
                |row| row.get(0),
            )?;
            (task.tag_id.clone(), name)
        }
        _ => fallback_tag(&tx)?,
    };
    let duration = settings.duration_seconds_for_mode(input.mode);
    let now = now_millis();
    let session_id = Uuid::new_v4().to_string();

    timer.mode = input.mode;
    timer.state = TimerState::Running;
    timer.active_session_id = Some(session_id);
    timer.selected_task_id = input.selected_task_id.clone();
    timer.task_title_snapshot = Some(title_snap);
    timer.project_snapshot = Some(project_snap);
    timer.tag_id = Some(tag_id);
    timer.tag_name_snapshot = Some(tag_name);
    timer.duration_seconds = duration;
    timer.remaining_seconds = duration;
    timer.started_at = Some(now);
    timer.target_end_at = Some(now + duration * 1000);
    timer.paused_at = None;
    timer.revision += 1;
    timer.updated_at = now;

    write_timer(&tx, &timer)?;
    tx.commit()?;
    Ok(timer)
}

/// `pause_timer`: running → paused. Computes remaining from `target_end_at`,
/// clears `target_end_at`, bumps revision.
pub fn pause_timer(
    conn: &mut Connection,
    input: &crate::models::TimerRevisionInput,
) -> Result<TimerSnapshot, CommandError> {
    let tx = conn.transaction()?;
    let mut timer = get_timer(&tx)?;
    check_revision(&timer, input.expected_revision)?;

    if timer.state != TimerState::Running {
        return Err(CommandError::validation(format!(
            "pause_timer requires running state, found {:?}",
            timer.state
        )));
    }

    let now = now_millis();
    timer.remaining_seconds = live_remaining(&timer, now);
    timer.state = TimerState::Paused;
    timer.target_end_at = None;
    timer.paused_at = Some(now);
    timer.revision += 1;
    timer.updated_at = now;

    write_timer(&tx, &timer)?;
    tx.commit()?;
    Ok(timer)
}

/// `resume_timer`: paused → running. Generates a new `target_end_at` from
/// remaining, bumps revision.
pub fn resume_timer(
    conn: &mut Connection,
    input: &crate::models::TimerRevisionInput,
) -> Result<TimerSnapshot, CommandError> {
    let tx = conn.transaction()?;
    let mut timer = get_timer(&tx)?;
    check_revision(&timer, input.expected_revision)?;

    if timer.state != TimerState::Paused {
        return Err(CommandError::validation(format!(
            "resume_timer requires paused state, found {:?}",
            timer.state
        )));
    }

    let now = now_millis();
    timer.state = TimerState::Running;
    timer.target_end_at = Some(now + timer.remaining_seconds * 1000);
    timer.paused_at = None;
    timer.revision += 1;
    timer.updated_at = now;

    write_timer(&tx, &timer)?;
    tx.commit()?;
    Ok(timer)
}

/// `reset_timer`: any → idle (current mode). If a session was started, writes
/// an abandoned session first. Bumps revision.
pub fn reset_timer(
    conn: &mut Connection,
    settings: &AppSettings,
    input: &crate::models::TimerRevisionInput,
) -> Result<TimerSnapshot, CommandError> {
    let tx = conn.transaction()?;
    let mut timer = get_timer(&tx)?;
    check_revision(&timer, input.expected_revision)?;

    let now = now_millis();
    let started = timer.state != TimerState::Idle
        && timer.active_session_id.is_some()
        && timer.started_at.is_some();

    if started {
        timer.remaining_seconds = match timer.state {
            TimerState::Running => live_remaining(&timer, now),
            _ => timer.remaining_seconds,
        };
        write_reset_session(&tx, &timer, now)?;
    }

    let duration = settings.duration_seconds_for_mode(timer.mode);
    timer.state = TimerState::Idle;
    timer.active_session_id = None;
    timer.selected_task_id = None;
    timer.task_title_snapshot = None;
    timer.project_snapshot = None;
    timer.duration_seconds = duration;
    timer.remaining_seconds = duration;
    timer.started_at = None;
    timer.target_end_at = None;
    timer.paused_at = None;
    timer.revision += 1;
    timer.updated_at = now;

    write_timer(&tx, &timer)?;
    tx.commit()?;
    Ok(timer)
}

/// `switch_timer_mode`: idle/done → idle (new mode). Defensive against stale
/// frontends: an active (running/paused) timer must never be switched — the
/// v1.1 UI disables the buttons and this returns CONFLICT (decision D-2,
/// superseding the v1.0.0 "switch submits elapsed" behavior). No session is
/// created for a legal switch; the revision only advances on success.
pub fn switch_timer_mode(
    conn: &mut Connection,
    settings: &AppSettings,
    input: &SwitchTimerModeInput,
) -> Result<TimerSnapshot, CommandError> {
    let tx = conn.transaction()?;
    let mut timer = get_timer(&tx)?;
    check_revision(&timer, input.expected_revision)?;

    if timer.state != TimerState::Idle && timer.state != TimerState::Done {
        return Err(CommandError::conflict(format!(
            "cannot switch mode while a timer is {:?}; finish or reset it first",
            timer.state
        )));
    }

    let duration = settings.duration_seconds_for_mode(input.mode);
    timer.mode = input.mode;
    timer.state = TimerState::Idle;
    timer.active_session_id = None;
    timer.selected_task_id = None;
    timer.task_title_snapshot = None;
    timer.project_snapshot = None;
    timer.tag_id = None;
    timer.tag_name_snapshot = None;
    timer.duration_seconds = duration;
    timer.remaining_seconds = duration;
    timer.started_at = None;
    timer.target_end_at = None;
    timer.paused_at = None;
    timer.revision += 1;
    timer.updated_at = now_millis();

    write_timer(&tx, &timer)?;
    tx.commit()?;
    Ok(timer)
}

/// `switch_timer_task` (v1.1.2): switch the running/paused round's task.
///
/// One transaction: the current session is closed with its actual focused
/// time (`manual_finish` semantics — the `finish_reason` CHECK set does not
/// include a task-switch value, and a mid-round switch IS a manual end), then
/// a new focus session opens for the chosen task with a full fresh duration.
/// Task A's history stays with task A. v1.2's segment ledger replaces the
/// clock reset with same-clock segment splitting.
///
/// Idempotency comes before every other check (v1.1 invariant): a replayed
/// command whose session is already closed returns that session with
/// `newly_closed = false` instead of conflicting.
pub fn switch_timer_task(
    conn: &mut Connection,
    settings: &AppSettings,
    input: &SwitchTimerTaskInput,
) -> Result<SwitchTimerTaskResult, CommandError> {
    let tx = conn.transaction()?;

    // 1) Idempotency first: an already-closed session for this id wins over
    //    every other check (stale revision included).
    let existing = tx
        .query_row(
            &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
            params![input.active_session_id],
            session_from_row,
        )
        .optional()?;
    if let Some(session) = existing {
        let timer = get_timer(&tx)?;
        return Ok(SwitchTimerTaskResult {
            timer,
            closed_session: Some(session),
            newly_closed: false,
        });
    }

    // 2) Validate timer state, active session, revision.
    let mut timer = get_timer(&tx)?;
    check_revision(&timer, input.expected_revision)?;
    if timer.state != TimerState::Running && timer.state != TimerState::Paused {
        return Err(CommandError::validation(format!(
            "switch_timer_task requires a running or paused timer, found {:?}",
            timer.state
        )));
    }
    match &timer.active_session_id {
        Some(id) if id == &input.active_session_id => {}
        _ => return Err(CommandError::validation("activeSessionId does not match timer")),
    }

    // 3) The new task must exist, be open, and differ from the current one.
    if timer.mode != TimerMode::Focus {
        return Err(CommandError::validation(
            "tasks can only be switched during a focus round",
        ));
    }
    let new_task = get_task(&tx, &input.new_task_id)?;
    if new_task.done {
        return Err(CommandError::validation(
            "cannot switch to a completed task; reopen it first",
        ));
    }
    if timer.selected_task_id.as_deref() == Some(input.new_task_id.as_str()) {
        // Switching to the already-current task changes nothing.
        return Ok(SwitchTimerTaskResult { timer, closed_session: None, newly_closed: false });
    }

    let now = now_millis();

    // 4) Close the current session with its actual focused time. Drift-free:
    //    remaining is ceiling-rounded so 29.x seconds never count as 30.
    let effective_remaining = match timer.state {
        TimerState::Running => live_remaining(&timer, now),
        _ => timer.remaining_seconds,
    };
    let focused = (timer.duration_seconds - effective_remaining).max(0);
    write_finished_session(&tx, &timer, now, SessionStatus::Completed, focused, "manual_finish")?;
    let closed_session = tx
        .query_row(
            &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
            params![input.active_session_id],
            session_from_row,
        )
        .optional()?
        .ok_or_else(|| CommandError::internal("closed session disappeared mid-query"))?;

    // 5) Open the new round for the chosen task. A paused switch starts the
    //    new round paused with a full duration (the user explicitly paused).
    let duration = settings.duration_seconds_for_mode(TimerMode::Focus);
    let new_session_id = Uuid::new_v4().to_string();
    let tag_name: String = tx
        .query_row(
            "SELECT name FROM tags WHERE id = ?1",
            params![new_task.tag_id],
            |row| row.get(0),
        )?;

    timer.mode = TimerMode::Focus;
    timer.active_session_id = Some(new_session_id);
    timer.selected_task_id = Some(new_task.id.clone());
    timer.task_title_snapshot = Some(new_task.title.clone());
    timer.project_snapshot = Some(new_task.project.clone());
    timer.tag_id = Some(new_task.tag_id.clone());
    timer.tag_name_snapshot = Some(tag_name);
    timer.duration_seconds = duration;
    timer.remaining_seconds = duration;
    match timer.state {
        TimerState::Running => {
            timer.started_at = Some(now);
            timer.target_end_at = Some(now + duration * 1000);
            timer.paused_at = None;
        }
        _ => {
            // Paused switch → the new round stays paused with a full duration.
            timer.started_at = Some(now);
            timer.target_end_at = None;
            timer.paused_at = Some(now);
        }
    }
    timer.revision += 1;
    timer.updated_at = now;
    write_timer(&tx, &timer)?;

    tx.commit()?;

    Ok(SwitchTimerTaskResult {
        timer,
        closed_session: Some(closed_session),
        newly_closed: true,
    })
}

/// `complete_timer`: the NATURAL-completion path (running → done). Idempotent
/// — if a completed session with the same `activeSessionId` already exists,
/// returns it with `newlyCompleted = false`. If an abandoned session exists,
/// returns CONFLICT.
///
/// v1.1 ruling: this command may only fire when the countdown has actually
/// reached (or is within `COMPLETE_SCHEDULING_TOLERANCE_MS` of) its deadline.
/// An early call returns CONFLICT without writing a session, changing the
/// timer, or producing any notification — a rendering bug must never end a
/// running focus session. Ending early is `finish_timer`'s job.
pub fn complete_timer(
    conn: &mut Connection,
    _settings: &AppSettings,
    input: &CompleteTimerInput,
) -> Result<CompleteTimerResult, CommandError> {
    let tx = conn.transaction()?;

    // 1–2: Check if a session with this ID already exists.
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT status, mode FROM sessions WHERE id = ?1",
            params![input.active_session_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;

    if let Some((status, _mode)) = &existing {
        if status == "completed" {
            // Idempotent: return the existing session and done timer.
            let session = tx
                .query_row(
                    &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                    params![input.active_session_id],
                    session_from_row,
                )
                .optional()?
                .ok_or_else(|| CommandError::internal("completed session disappeared mid-query"))?;
            let timer = get_timer(&tx)?;
            let stats = all_time_statistics(&tx)?;
            return Ok(CompleteTimerResult {
                timer,
                session,
                statistics: stats,
                newly_completed: false,
            });
        }
        if status == "abandoned" {
            return Err(CommandError::conflict(
                "cannot complete a session that was already abandoned",
            ));
        }
    }

    // 3: Validate timer state, active session, revision.
    let mut timer = get_timer(&tx)?;
    check_revision(&timer, input.expected_revision)?;

    if timer.state != TimerState::Running {
        return Err(CommandError::validation(format!(
            "complete_timer requires running state, found {:?}",
            timer.state
        )));
    }
    match &timer.active_session_id {
        Some(id) if id == &input.active_session_id => {}
        _ => return Err(CommandError::validation("activeSessionId does not match timer")),
    }

    // 3b: Deadline guard (v1.1 ruling) — recompute the remaining time from the
    // wall clock and reject early calls. The idempotency branch above has
    // already handled replays, so this cannot block a legitimate completion.
    let now = now_millis();
    let remaining_ms = match timer.target_end_at {
        Some(end) => end - now,
        None => {
            return Err(CommandError::conflict(
                "running timer has no target_end_at; cannot complete naturally",
            ))
        }
    };
    if remaining_ms > COMPLETE_SCHEDULING_TOLERANCE_MS {
        return Err(CommandError::conflict(format!(
            "timer has not expired yet ({remaining_ms}ms remaining); \
             use finish_timer to end it early"
        )));
    }

    // 4: Compute focused time from the same `now` used by the deadline guard.
    let actual_remaining = live_remaining(&timer, now);
    let focused = (timer.duration_seconds - actual_remaining).max(0);
    // 30-second qualification lives here and nowhere else (v1.1 §8.2).
    let eligible = timer.mode == TimerMode::Focus && focused >= MIN_QUALIFYING_FOCUS_SECONDS;
    let qualification = if timer.mode != TimerMode::Focus {
        "non_focus"
    } else if eligible {
        "qualified"
    } else {
        "too_short"
    };
    let (fallback_id, fallback_name) = fallback_tag(&tx)?;

    let session = TimerSession {
        id: input.active_session_id.clone(),
        task_id: timer.selected_task_id.clone(),
        task_title_snapshot: timer.task_title_snapshot.clone().unwrap_or_else(|| NO_TASK_TITLE.to_owned()),
        project_snapshot: timer.project_snapshot.clone().unwrap_or_else(|| NO_TASK_PROJECT.to_owned()),
        tag_id: Some(timer.tag_id.clone().unwrap_or_else(|| fallback_id.clone())),
        tag_name_snapshot: Some(
            timer
                .tag_name_snapshot
                .clone()
                .unwrap_or_else(|| fallback_name.clone()),
        ),
        mode: timer.mode,
        status: SessionStatus::Completed,
        planned_seconds: timer.duration_seconds,
        focused_seconds: focused,
        started_at: timer.started_at.unwrap_or(now),
        ended_at: now,
        finish_reason: Some("elapsed".to_owned()),
        statistics_eligible: Some(eligible),
        qualification_reason: Some(qualification.to_owned()),
    };

    // 4: INSERT with ON CONFLICT DO NOTHING so a duplicate insert is a no-op.
    let inserted = tx.execute(
        "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                               tag_name_snapshot, mode, status, planned_seconds,
                               focused_seconds, started_at, ended_at, finish_reason,
                               statistics_eligible, qualification_reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO NOTHING",
        params![
            session.id,
            session.task_id,
            session.task_title_snapshot,
            session.project_snapshot,
            session.tag_id,
            session.tag_name_snapshot,
            session.mode.as_str(),
            session.status.as_str(),
            session.planned_seconds,
            session.focused_seconds,
            session.started_at,
            session.ended_at,
            "elapsed",
            eligible as i64,
            qualification,
        ],
    )?;

    // 5: Set timer to done, remaining to zero, bump revision.
    timer.state = TimerState::Done;
    timer.remaining_seconds = 0;
    timer.target_end_at = None;
    timer.revision += 1;
    timer.updated_at = now;
    write_timer(&tx, &timer)?;

    let stats = all_time_statistics(&tx)?;
    tx.commit()?;

    // 6: newlyCompleted = true only if we actually inserted the row.
    Ok(CompleteTimerResult {
        timer,
        session,
        statistics: stats,
        newly_completed: inserted > 0,
    })
}

/// Freezes a running timer as paused before the app quits via the tray.
///
/// Per the user's explicit rule: when the user chooses "彻底退出" (quit) from
/// the tray while a focus session is running, the remaining time must be saved
/// and the state restored as `paused` so focus time is not silently consumed on
/// the next launch — the user resumes manually. A non-running timer is left
/// untouched (this is a no-op for idle/paused/done).
///
/// The remaining time is derived from `target_end_at` (drift-free), `target_end_at`
/// is cleared, `paused_at` is stamped, and the revision is bumped. The WAL is
/// checkpointed so the change survives the imminent process exit even with
/// `synchronous=NORMAL`.
/// `finish_timer` (v1.1 §8.5): the user clicks "结束" — the session is saved
/// with its actual focused time and the timer returns to the current mode's
/// idle state (natural completion still ends in `done` with sound+notification).
///
/// Idempotency is checked BEFORE the revision/state validation (review #5):
/// a replayed command with a stale revision returns the recorded session with
/// `newly_finished = false` instead of a conflict. A lost race against
/// `complete_timer` is handled the same way — exactly one session, one effect.
pub fn finish_timer(
    conn: &mut Connection,
    input: &FinishTimerInput,
) -> Result<FinishTimerResult, CommandError> {
    let tx = conn.transaction()?;

    // 1) Idempotency first: an existing session for this id wins over every
    //    other check (stale revision included).
    let existing: Option<(String, i64, String)> = tx
        .query_row(
            "SELECT finish_reason, statistics_eligible, qualification_reason
             FROM sessions WHERE id = ?1",
            params![input.active_session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    if let Some((_, eligible, qualification)) = &existing {
        let session = tx
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![input.active_session_id],
                session_from_row,
            )
            .optional()?
            .ok_or_else(|| CommandError::internal("finished session disappeared mid-query"))?;
        let timer = get_timer(&tx)?;
        let stats = all_time_statistics(&tx)?;
        return Ok(FinishTimerResult {
            timer,
            session,
            statistics: stats,
            newly_finished: false,
            statistics_eligible: *eligible != 0,
            qualification_reason: qualification.clone(),
        });
    }

    // 2) No session yet — validate timer state, active session, revision.
    let mut timer = get_timer(&tx)?;
    check_revision(&timer, input.expected_revision)?;

    if timer.state != TimerState::Running && timer.state != TimerState::Paused {
        return Err(CommandError::conflict(format!(
            "finish_timer requires a running or paused timer, found {:?}",
            timer.state
        )));
    }
    match &timer.active_session_id {
        Some(id) if id == &input.active_session_id => {}
        _ => return Err(CommandError::conflict("activeSessionId does not match timer")),
    }

    // 3) Drift-free focused time; remaining is ceiling-rounded so 29.x seconds
    //    can never count as 30 (spec §8.1).
    let now = now_millis();
    let effective_remaining = match timer.state {
        TimerState::Running => live_remaining(&timer, now),
        _ => timer.remaining_seconds,
    };
    let focused = (timer.duration_seconds - effective_remaining).max(0);
    // The 30-second rule lives here and nowhere else.
    let eligible = timer.mode == TimerMode::Focus && focused >= MIN_QUALIFYING_FOCUS_SECONDS;
    let qualification = if timer.mode != TimerMode::Focus {
        "non_focus"
    } else if eligible {
        "qualified"
    } else {
        "too_short"
    };
    let (fallback_id, fallback_name) = fallback_tag(&tx)?;

    let session = TimerSession {
        id: input.active_session_id.clone(),
        task_id: timer.selected_task_id.clone(),
        task_title_snapshot: timer
            .task_title_snapshot
            .clone()
            .unwrap_or_else(|| NO_TASK_TITLE.to_owned()),
        project_snapshot: timer
            .project_snapshot
            .clone()
            .unwrap_or_else(|| NO_TASK_PROJECT.to_owned()),
        tag_id: Some(timer.tag_id.clone().unwrap_or_else(|| fallback_id.clone())),
        tag_name_snapshot: Some(
            timer
                .tag_name_snapshot
                .clone()
                .unwrap_or_else(|| fallback_name.clone()),
        ),
        mode: timer.mode,
        status: SessionStatus::Completed,
        planned_seconds: timer.duration_seconds,
        focused_seconds: focused,
        started_at: timer.started_at.unwrap_or(now),
        ended_at: now,
        finish_reason: Some("manual_finish".to_owned()),
        statistics_eligible: Some(eligible),
        qualification_reason: Some(qualification.to_owned()),
    };

    // 4) Same transaction: insert (ON CONFLICT guards the complete_timer race)
    //    + timer back to idle + statistics read.
    let inserted = tx.execute(
        "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                               tag_name_snapshot, mode, status, planned_seconds,
                               focused_seconds, started_at, ended_at, finish_reason,
                               statistics_eligible, qualification_reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO NOTHING",
        params![
            session.id,
            session.task_id,
            session.task_title_snapshot,
            session.project_snapshot,
            session.tag_id,
            session.tag_name_snapshot,
            session.mode.as_str(),
            session.status.as_str(),
            session.planned_seconds,
            session.focused_seconds,
            session.started_at,
            session.ended_at,
            "manual_finish",
            eligible as i64,
            qualification,
        ],
    )?;

    if inserted == 0 {
        // A concurrent completion won the race — defer to its result without
        // touching the timer.
        let session = tx
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![input.active_session_id],
                session_from_row,
            )
            .optional()?
            .ok_or_else(|| CommandError::internal("session disappeared mid-query"))?;
        let timer = get_timer(&tx)?;
        let stats = all_time_statistics(&tx)?;
        return Ok(FinishTimerResult {
            timer,
            session,
            statistics: stats,
            newly_finished: false,
            statistics_eligible: eligible,
            qualification_reason: qualification.to_owned(),
        });
    }

    // 5) Manual finish ends in the current mode's idle state with full duration.
    timer.state = TimerState::Idle;
    timer.active_session_id = None;
    timer.selected_task_id = None;
    timer.task_title_snapshot = None;
    timer.project_snapshot = None;
    timer.tag_id = None;
    timer.tag_name_snapshot = None;
    timer.remaining_seconds = timer.duration_seconds;
    timer.target_end_at = None;
    timer.paused_at = None;
    timer.revision += 1;
    timer.updated_at = now;
    write_timer(&tx, &timer)?;

    let stats = all_time_statistics(&tx)?;
    tx.commit()?;

    Ok(FinishTimerResult {
        timer,
        session,
        statistics: stats,
        newly_finished: inserted > 0,
        statistics_eligible: eligible,
        qualification_reason: qualification.to_owned(),
    })
}

pub fn persist_running_as_paused(conn: &Connection) -> Result<(), CommandError> {
    let mut timer = get_timer(conn)?;
    if timer.state != TimerState::Running {
        return Ok(());
    }
    let now = now_millis();
    timer.remaining_seconds = live_remaining(&timer, now);
    timer.state = TimerState::Paused;
    timer.target_end_at = None;
    timer.paused_at = Some(now);
    timer.revision += 1;
    timer.updated_at = now;
    write_timer(conn, &timer)?;
    // Force the WAL into the main db file before the process exits so the
    // paused state is durable across `app.exit(0)`.
    let _ = conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", []);
    Ok(())
}

/// Lists sessions matching the frontend's query (limit + optional time range).
pub fn list_sessions_query(
    conn: &Connection,
    query: &SessionQuery,
) -> Result<Vec<TimerSession>, CommandError> {
    let mut sql = String::from("SELECT ");
    sql.push_str(SESSION_COLUMNS);
    sql.push_str(" FROM sessions WHERE 1=1");
    // Default scope is `activity`: hidden records (too_short, abandoned,
    // breaks) can only be read explicitly with `all` (exports/tests).
    if query.scope.unwrap_or_default() != crate::models::SessionScope::All {
        sql.push_str(" AND statistics_eligible = 1");
    }
    let mut bindings: Vec<i64> = Vec::new();
    if let Some(from) = query.from {
        sql.push_str(" AND started_at >= ?");
        bindings.push(from);
    }
    if let Some(to) = query.to {
        sql.push_str(" AND started_at <= ?");
        bindings.push(to);
    }
    sql.push_str(" ORDER BY started_at DESC, rowid DESC");
    if let Some(limit) = query.limit {
        sql.push_str(" LIMIT ?");
        bindings.push(limit);
    }

    let mut stmt = conn.prepare(&sql)?;
    let params = rusqlite::params_from_iter(bindings.iter());
    let rows = stmt.query_map(params, session_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Validates the frontend's explicit day boundaries per the spec §6:
/// - days must be ordered by date
/// - no overlaps
/// - each segment from < to
/// - each segment must fall within the total [from, to] range
fn validate_day_boundaries(query: &StatisticsQuery) -> Result<(), CommandError> {
    let days = &query.days;
    for (i, d) in days.iter().enumerate() {
        if d.from >= d.to {
            return Err(CommandError::validation(format!(
                "day boundary '{}' has from >= to",
                d.date
            )));
        }
        if d.from < query.from || d.to > query.to {
            return Err(CommandError::validation(format!(
                "day boundary '{}' falls outside the total range",
                d.date
            )));
        }
        if i > 0 {
            let prev = &days[i - 1];
            if d.from < prev.to {
                return Err(CommandError::validation(format!(
                    "day boundaries overlap: '{}' and '{}'",
                    prev.date, d.date
                )));
            }
            if d.date <= prev.date {
                return Err(CommandError::validation(format!(
                    "day boundaries must be ordered: '{}' before '{}'",
                    d.date, prev.date
                )));
            }
        }
    }
    Ok(())
}

/// Computes statistics for an explicit time range with per-day buckets.
///
/// Per the spec §6: only `mode = 'focus' AND status = 'completed'` sessions
/// are counted. `streak_days` is the consecutive-day count ending today (or
/// yesterday if today has no sessions). `best_day` is the date with the most
/// focus seconds. Rust does NOT guess DST day buckets — it uses the explicit
/// `days` array provided by the frontend.
pub fn get_statistics(
    conn: &Connection,
    query: &StatisticsQuery,
) -> Result<Statistics, CommandError> {
    validate_day_boundaries(query)?;

    // Collect completed focus sessions in range. Only statistics-eligible
    // sessions count (v1.1: 30-second rule + abandoned never counted).
    let mut stmt = conn.prepare(
        &format!(
            "SELECT {SESSION_COLUMNS} FROM sessions
             WHERE mode = 'focus' AND status = 'completed' AND statistics_eligible = 1
               AND started_at >= ?1 AND started_at <= ?2
             ORDER BY started_at ASC"
        ),
    )?;
    let sessions: Vec<TimerSession> = stmt
        .query_map(params![query.from, query.to], session_from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    let focus_session_count = sessions.len() as i64;
    let focus_seconds: i64 = sessions.iter().map(|s| s.focused_seconds).sum();

    // by_day: bucket each session into the frontend-provided day boundaries.
    let mut by_day_map: std::collections::BTreeMap<String, (i64, i64)> =
        query.days.iter().map(|d| (d.date.clone(), (0, 0))).collect();
    for s in &sessions {
        for d in &query.days {
            // Half-open [from, to) so a session at a day boundary falls into
            // exactly one bucket, matching the spec's non-overlap invariant.
            if s.started_at >= d.from && s.started_at < d.to {
                if let Some(entry) = by_day_map.get_mut(&d.date) {
                    entry.0 += 1;
                    entry.1 += s.focused_seconds;
                }
                break;
            }
        }
    }
    let by_day: Vec<DayStat> = by_day_map
        .iter()
        .map(|(date, (sessions, seconds))| DayStat {
            date: date.clone(),
            sessions: *sessions,
            focus_seconds: *seconds,
        })
        .collect();

    // by_project: aggregate across all sessions in range.
    let mut by_project_map: std::collections::BTreeMap<String, (i64, i64)> = std::collections::BTreeMap::new();
    for s in &sessions {
        let entry = by_project_map.entry(s.project_snapshot.clone()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += s.focused_seconds;
    }
    let by_project: Vec<ProjectStat> = by_project_map
        .iter()
        .map(|(project, (sessions, seconds))| ProjectStat {
            project: project.clone(),
            sessions: *sessions,
            focus_seconds: *seconds,
        })
        .collect();

    // by_tag: aggregate by the FROZEN tag name snapshot — a tag rename must
    // never rewrite historical statistics (v1.1 §11.7).
    let mut by_tag_map: std::collections::BTreeMap<String, (i64, i64)> = std::collections::BTreeMap::new();
    for s in &sessions {
        let tag = s.tag_name_snapshot.clone().unwrap_or_else(|| "其他".to_owned());
        let entry = by_tag_map.entry(tag).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += s.focused_seconds;
    }
    let mut by_tag: Vec<ProjectStat> = by_tag_map
        .iter()
        .map(|(tag, (sessions, seconds))| ProjectStat {
            project: tag.clone(),
            sessions: *sessions,
            focus_seconds: *seconds,
        })
        .collect();
    by_tag.sort_by(|a, b| b.focus_seconds.cmp(&a.focus_seconds).then(a.project.cmp(&b.project)));

    // best_day: the date with the most focus seconds.
    let best_day = by_day
        .iter()
        .filter(|d| d.focus_seconds > 0)
        .max_by_key(|d| d.focus_seconds)
        .map(|d| d.date.clone());

    // streak_days: consecutive days ending today (or yesterday).
    let streak_days = compute_streak(&by_day);

    let settings = get_settings(conn)?;

    Ok(Statistics {
        from: query.from,
        to: query.to,
        focus_session_count,
        focus_seconds,
        daily_goal: settings.daily_goal,
        streak_days,
        best_day,
        by_day,
        by_project,
    by_tag,
    })
}

/// Counts consecutive days with at least one completed focus session,
/// ending at the last day in `by_day` (which the frontend ensures is today
/// or the most recent day with data).
fn compute_streak(by_day: &[DayStat]) -> i64 {
    if by_day.is_empty() {
        return 0;
    }
    // Walk backwards from the end while sessions > 0.
    let mut streak = 0i64;
    for day in by_day.iter().rev() {
        if day.sessions > 0 {
            streak += 1;
        } else {
            break;
        }
    }
    streak
}

/// All-time focus totals.
///
/// `by_day`, `streak_days` and `best_day` all depend on the caller's local
/// calendar boundaries, so they stay empty here and are filled by
/// `get_statistics` (T9), which receives explicit day buckets from the
/// frontend.
pub fn all_time_statistics(conn: &Connection) -> Result<Statistics, CommandError> {
    let (count, seconds) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(focused_seconds), 0)
         FROM sessions
         WHERE mode = 'focus' AND status = 'completed' AND statistics_eligible = 1",
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;

    let mut stmt = conn.prepare(
        "SELECT project_snapshot, COUNT(*) AS sessions, COALESCE(SUM(focused_seconds), 0) AS focus_seconds
         FROM sessions
         WHERE mode = 'focus' AND status = 'completed' AND statistics_eligible = 1
         GROUP BY project_snapshot
         ORDER BY focus_seconds DESC, project_snapshot ASC",
    )?;
    let by_project = stmt
        .query_map([], |row| {
            Ok(ProjectStat {
                project: row.get(0)?,
                sessions: row.get(1)?,
                focus_seconds: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut by_tag_stmt = conn.prepare(
        "SELECT COALESCE(tag_name_snapshot, '其他') AS tag, COUNT(*) AS sessions,
                COALESCE(SUM(focused_seconds), 0) AS focus_seconds
         FROM sessions
         WHERE mode = 'focus' AND status = 'completed' AND statistics_eligible = 1
         GROUP BY tag ORDER BY focus_seconds DESC, tag ASC",
    )?;
    let by_tag = by_tag_stmt
        .query_map([], |row| {
            Ok(ProjectStat {
                project: row.get(0)?,
                sessions: row.get(1)?,
                focus_seconds: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let settings = get_settings(conn)?;

    Ok(Statistics {
        from: 0,
        to: now_millis(),
        focus_session_count: count,
        focus_seconds: seconds,
        daily_goal: settings.daily_goal,
        streak_days: 0,
        best_day: None,
        by_day: Vec::new(),
        by_project,
        by_tag,
    })
}

// ─── Data export & backup (Item 3) ──────────────────────────────────────────

/// Builds a lossless JSON backup bundle: settings, all tasks, all sessions.
pub fn export_data(conn: &Connection) -> Result<ExportBundle, CommandError> {
    // Backups are complete by definition: they use scope=all so hidden
    // records (too_short / abandoned / breaks) survive the round trip.
    Ok(ExportBundle {
        app: EXPORT_APP_NAME.to_owned(),
        schema_version: EXPORT_SCHEMA_VERSION,
        exported_at: now_millis(),
        settings: get_settings(conn)?,
        tags: list_tags(conn)?,
        tasks: list_tasks(conn)?,
        sessions: list_all_sessions(conn)?,
    })
}

/// The four system tags as they would exist in the database. Used to upgrade
/// v1 backups (which have no tags at all).
fn default_tags(now: i64) -> Vec<Tag> {
    vec![
        Tag { id: "system-study".into(), name: "学习".into(), kind: TagKind::System, is_fallback: false, sort_order: 0, created_at: now, updated_at: now },
        Tag { id: "system-work".into(), name: "工作".into(), kind: TagKind::System, is_fallback: false, sort_order: 1, created_at: now, updated_at: now },
        Tag { id: "system-life".into(), name: "生活".into(), kind: TagKind::System, is_fallback: false, sort_order: 2, created_at: now, updated_at: now },
        Tag { id: "system-other".into(), name: "其他".into(), kind: TagKind::System, is_fallback: true, sort_order: 3, created_at: now, updated_at: now },
    ]
}

/// Upgrades a v1.0.0 backup to the v2 bundle shape: seeds the four default
/// tags, moves every task to the fallback tag, and backfills session
/// qualification per the §7.3 rules (finish_reason = "legacy").
fn normalize_v1_bundle(v1: ExportBundleV1) -> ExportBundle {
    let now = if v1.exported_at > 0 { v1.exported_at } else { now_millis() };
    let tags = default_tags(now);

    let tasks = v1
        .tasks
        .into_iter()
        .map(|t| Task {
            id: t.id,
            title: t.title,
            done: t.done,
            pomodoro_target: t.pomodoro_target,
            priority: t.priority,
            project: t.project,
            tag_id: crate::models::FALLBACK_TAG_ID.to_owned(),
            sort_order: t.sort_order,
            created_at: t.created_at,
            updated_at: t.updated_at,
            completed_at: t.completed_at,
        })
        .collect();

    let sessions = v1
        .sessions
        .into_iter()
        .map(|session| {
            let probe = TimerSession {
                id: session.id,
                task_id: session.task_id,
                task_title_snapshot: session.task_title_snapshot,
                project_snapshot: session.project_snapshot,
                tag_id: None,
                tag_name_snapshot: None,
                mode: session.mode,
                status: session.status,
                planned_seconds: session.planned_seconds,
                focused_seconds: session.focused_seconds,
                started_at: session.started_at,
                ended_at: session.ended_at,
                finish_reason: None,
                statistics_eligible: None,
                qualification_reason: None,
            };
            let (finish, eligible, qualification) = effective_qualification(&probe);
            TimerSession {
                tag_id: Some(crate::models::FALLBACK_TAG_ID.to_owned()),
                tag_name_snapshot: Some("其他".to_owned()),
                finish_reason: Some(finish),
                statistics_eligible: Some(eligible != 0),
                qualification_reason: Some(qualification),
                ..probe
            }
        })
        .collect();

    ExportBundle {
        app: v1.app,
        schema_version: EXPORT_SCHEMA_VERSION,
        exported_at: v1.exported_at,
        settings: v1.settings,
        tags,
        tasks,
        sessions,
    }
}

/// Version-header-first backup parsing (v1.1 review): read `app` +
/// `schema_version`, then hand the payload to the matching DTO. A v1 backup
/// is upgraded to the v2 bundle shape; anything else is rejected.
pub fn parse_backup_text(text: &str) -> Result<ExportBundle, CommandError> {
    let header: BackupHeader = serde_json::from_str(text)
        .map_err(|err| CommandError::validation(format!("备份文件格式无效: {err}")))?;

    if header.app != EXPORT_APP_NAME {
        return Err(CommandError::validation("文件不是 Abyssal Reverie 备份"));
    }

    match header.schema_version {
        1 => {
            let v1: ExportBundleV1 = serde_json::from_str(text)
                .map_err(|err| CommandError::validation(format!("v1 备份解析失败: {err}")))?;
            Ok(normalize_v1_bundle(v1))
        }
        2 => serde_json::from_str::<ExportBundle>(text)
            .map_err(|err| CommandError::validation(format!("v2 备份解析失败: {err}"))),
        other => Err(CommandError::validation(format!(
            "备份版本 {other} 不受支持（当前最高 {EXPORT_SCHEMA_VERSION}）"
        ))),
    }
}

/// Full listing without the activity-visibility filter (exports/backups).
fn list_all_sessions(conn: &Connection) -> Result<Vec<TimerSession>, CommandError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SESSION_COLUMNS} FROM sessions ORDER BY started_at ASC, rowid ASC"
    ))?;
    let rows = stmt.query_map([], session_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Validates a parsed backup bundle before it touches the database.
/// Row counts shown to the user before a destructive import is confirmed.
pub fn preview_from_bundle(bundle: &ExportBundle) -> ImportPreview {
    ImportPreview {
        schema_version: bundle.schema_version,
        tags: bundle.tags.len() as i64,
        tasks: bundle.tasks.len() as i64,
        sessions: bundle.sessions.len() as i64,
    }
}

pub fn validate_import(bundle: &ExportBundle) -> Result<(), CommandError> {
    if bundle.app != EXPORT_APP_NAME {
        return Err(CommandError::validation("文件不是 Abyssal Reverie 备份"));
    }
    if bundle.schema_version == 0 || bundle.schema_version > EXPORT_SCHEMA_VERSION {
        return Err(CommandError::validation(format!(
            "备份版本 {} 不受支持（当前最高 {}）",
            bundle.schema_version, EXPORT_SCHEMA_VERSION
        )));
    }
    // v2 bundles must carry their tags, with exactly one fallback, and every
    // task's tag must resolve inside the bundle.
    let fallback_count = bundle.tags.iter().filter(|t| t.is_fallback).count();
    if bundle.schema_version >= 2 {
        if bundle.tags.is_empty() {
            return Err(CommandError::validation("v2 备份缺少标签数据"));
        }
        if fallback_count != 1 {
            return Err(CommandError::validation("v2 备份必须有且只有一个保底标签"));
        }
        for task in &bundle.tasks {
            if !bundle.tags.iter().any(|t| t.id == task.tag_id) {
                return Err(CommandError::validation(format!(
                    "任务“{}”引用了备份中不存在的标签",
                    task.title
                )));
            }
        }
    }
    validate_settings(&bundle.settings)?;
    for task in &bundle.tasks {
        validate_title(&task.title)?;
        validate_pomodoro_target(task.pomodoro_target)?;
    }
    for session in &bundle.sessions {
        if session.id.trim().is_empty() {
            return Err(CommandError::validation("会话记录缺少 id 字段"));
        }
    }
    Ok(())
}

/// Replaces tasks, sessions and settings in a single transaction, then resets
/// the live timer to idle (preserving its mode) so a restored running/paused
/// session does not dangle. Destructive — the caller must have confirmed.
pub fn import_data(conn: &mut Connection, bundle: &ExportBundle) -> Result<ImportSummary, CommandError> {
    validate_import(bundle)?;

    let now = now_millis();
    let tx = conn.transaction()?;

    // Replace tags first (tasks reference them; timer_state.tag_id is SET NULL
    // by the FK and the timer is reset to idle below).
    tx.execute("DELETE FROM sessions", [])?;
    tx.execute("DELETE FROM tasks", [])?;
    tx.execute("DELETE FROM tags", [])?;
    for tag in &bundle.tags {
        tx.execute(
            "INSERT INTO tags (id, name, normalized_name, kind, is_fallback, sort_order,
                               created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                tag.id,
                tag.name,
                tag.name.to_lowercase(),
                tag.kind.as_str(),
                tag.is_fallback as i64,
                tag.sort_order,
                now,
            ],
        )?;
    }

    // Replace tasks (ids preserved from the backup; tag_id defaults to the
    // fallback tag for v1 backups via the model's serde default).
    for task in &bundle.tasks {
        tx.execute(
            "INSERT INTO tasks (id, title, done, pomodoro_target, priority, project, tag_id,
                                sort_order, created_at, updated_at, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                task.id,
                task.title,
                task.done as i64,
                task.pomodoro_target,
                task.priority.as_str(),
                task.project,
                task.tag_id,
                task.sort_order,
                task.created_at,
                task.updated_at,
                task.completed_at,
            ],
        )?;
    }

    // Replace sessions (ids preserved from the backup). v1-shaped sessions
    // (missing qualification fields) are backfilled per the v1.1 rules.
    tx.execute("DELETE FROM sessions", [])?;
    for session in &bundle.sessions {
        let (finish, eligible, qualification) = effective_qualification(session);
        let (fallback_id, fallback_name) = fallback_tag(&tx)?;
        tx.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                                   tag_name_snapshot, mode, status, planned_seconds,
                                   focused_seconds, started_at, ended_at, finish_reason,
                                   statistics_eligible, qualification_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                session.id,
                session.task_id,
                session.task_title_snapshot,
                session.project_snapshot,
                session.tag_id.clone().unwrap_or_else(|| fallback_id.clone()),
                session
                    .tag_name_snapshot
                    .clone()
                    .unwrap_or_else(|| fallback_name.clone()),
                session.mode.as_str(),
                session.status.as_str(),
                session.planned_seconds,
                session.focused_seconds,
                session.started_at,
                session.ended_at,
                finish,
                eligible,
                qualification,
            ],
        )?;
    }

    // Replace settings.
    tx.execute(
        "UPDATE settings SET focus_duration_minutes = ?1, short_break_minutes = ?2,
                          long_break_minutes = ?3, auto_start_break = ?4,
                          sound_enabled = ?5, notification_enabled = ?6,
                          daily_goal = ?7, reduce_motion = ?8, updated_at = ?9
         WHERE id = 1",
        params![
            bundle.settings.focus_duration_minutes,
            bundle.settings.short_break_minutes,
            bundle.settings.long_break_minutes,
            bundle.settings.auto_start_break as i64,
            bundle.settings.sound_enabled as i64,
            bundle.settings.notification_enabled as i64,
            bundle.settings.daily_goal,
            bundle.settings.reduce_motion as i64,
            now,
        ],
    )?;

    // Reset the live timer to idle for the restored current mode so no
    // restored session id is left dangling as the active session.
    let timer = get_timer(&tx)?;
    if timer.state != TimerState::Idle {
        let mut idle = TimerSnapshot::idle(timer.mode, bundle.settings.duration_seconds_for_mode(timer.mode));
        idle.revision = timer.revision + 1;
        idle.updated_at = now;
        write_timer(&tx, &idle)?;
    }

    tx.commit()?;

    Ok(ImportSummary {
        path: String::new(),
        tasks: bundle.tasks.len() as i64,
        sessions: bundle.sessions.len() as i64,
    })
}

/// Escapes a single CSV field per RFC 4180 (quote if it contains comma,
/// quote, CR or LF; double internal quotes).
fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Builds a spreadsheet-friendly CSV of every session. Timestamps are emitted
/// as epoch milliseconds (lossless, convertible in any spreadsheet) because the
/// app stores them that way and avoids a date-formatting dependency.
pub fn export_sessions_csv(conn: &Connection) -> Result<String, CommandError> {
    // CSV is a complete-record export: hidden (too_short / abandoned) rows are
    // included on purpose and flagged by their qualification columns.
    let sessions = list_all_sessions(conn)?;
    let mut out = String::from(
        "id,taskId,taskTitle,project,tagName,mode,status,plannedSeconds,focusedSeconds,\
         plannedMinutes,focusedMinutes,startedAt,endedAt,finishReason,statisticsEligible,\
         qualificationReason\n",
    );
    for session in &sessions {
        let row = [
            csv_field(&session.id),
            csv_field(session.task_id.as_deref().unwrap_or("")),
            csv_field(&session.task_title_snapshot),
            csv_field(&session.project_snapshot),
            csv_field(session.tag_name_snapshot.as_deref().unwrap_or("")),
            session.mode.as_str().to_owned(),
            session.status.as_str().to_owned(),
            session.planned_seconds.to_string(),
            session.focused_seconds.to_string(),
            (session.planned_seconds / 60).to_string(),
            (session.focused_seconds / 60).to_string(),
            session.started_at.to_string(),
            session.ended_at.to_string(),
            session.finish_reason.clone().unwrap_or_else(|| "legacy".to_owned()),
            session
                .statistics_eligible
                .map(|v| v.to_string())
                .unwrap_or_else(|| "false".to_owned()),
            session
                .qualification_reason
                .clone()
                .unwrap_or_else(|| "legacy".to_owned()),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::StatisticsDayBoundary;

    fn create_input(title: &str) -> CreateTaskInput {
        CreateTaskInput {
            title: title.to_owned(),
            pomodoro_target: 4,
            priority: TaskPriority::High,
            project: "Abyssal".to_owned(),
            tag_id: String::new(),
        }
    }

    /// Starts a focus round on `task_id` and returns the fresh timer snapshot.
    fn start_focus(conn: &mut Connection, settings: &AppSettings, task_id: Option<&str>) -> TimerSnapshot {
        let timer = get_timer(conn).expect("timer");
        start_timer(
            conn,
            settings,
            &StartTimerInput {
                expected_revision: timer.revision,
                mode: TimerMode::Focus,
                selected_task_id: task_id.map(str::to_owned),
            },
        )
        .expect("start_timer")
    }

    // ─── v1.1.2 stage B: switch_timer_task ───────────────────────────────────

    #[test]
    fn switch_timer_finishes_old_session_and_starts_new_round() {
        let mut conn = db::open_in_memory().expect("database should open");
        let settings = get_settings(&conn).expect("settings");
        let task_a = insert_task(&conn, &create_input("任务 A")).expect("task A");
        let task_b = insert_task(&conn, &create_input("任务 B")).expect("task B");

        let started = start_focus(&mut conn, &settings, Some(&task_a.id));
        let old_revision = started.revision;

        let result = switch_timer_task(
            &mut conn,
            &settings,
            &SwitchTimerTaskInput {
                expected_revision: started.revision,
                active_session_id: started.active_session_id.clone().expect("session id"),
                new_task_id: task_b.id.clone(),
            },
        )
        .expect("switch succeeds");

        // The new round belongs to task B with a full fresh duration.
        assert_eq!(result.newly_closed, true);
        assert_eq!(result.timer.selected_task_id.as_deref(), Some(task_b.id.as_str()));
        assert_eq!(result.timer.task_title_snapshot.as_deref(), Some("任务 B"));
        assert_eq!(result.timer.project_snapshot.as_deref(), Some("Abyssal"));
        assert_eq!(result.timer.state, TimerState::Running);
        assert_eq!(result.timer.duration_seconds, started.duration_seconds);
        assert_eq!(result.timer.remaining_seconds, started.duration_seconds);
        assert!(result.timer.revision > old_revision);
        assert_ne!(result.timer.active_session_id, started.active_session_id);

        // The old session is recorded for task A only — history is not moved.
        let closed = result.closed_session.expect("closed session");
        assert_eq!(closed.id, started.active_session_id.expect("started session id"));
        assert_eq!(closed.task_id.as_deref(), Some(task_a.id.as_str()));
        assert_eq!(closed.task_title_snapshot, "任务 A");
        assert_eq!(closed.status, SessionStatus::Completed);
        assert_eq!(closed.finish_reason.as_deref(), Some("manual_finish"));
        // An instant switch focuses ~0s → below the 30s rule → hidden from
        // statistics, exactly like a manual finish.
        assert_eq!(closed.statistics_eligible, Some(false));

        // Activity view must not show the short switch session.
        let visible = list_sessions(&conn, 50).expect("list");
        assert!(visible.iter().all(|s| s.id != closed.id));
    }

    #[test]
    fn switch_to_a_missing_task_rolls_back_everything() {
        let mut conn = db::open_in_memory().expect("database should open");
        let settings = get_settings(&conn).expect("settings");
        let task_a = insert_task(&conn, &create_input("任务 A")).expect("task A");
        let started = start_focus(&mut conn, &settings, Some(&task_a.id));

        let err = switch_timer_task(
            &mut conn,
            &settings,
            &SwitchTimerTaskInput {
                expected_revision: started.revision,
                active_session_id: started.active_session_id.clone().expect("session id"),
                new_task_id: "no-such-task".to_owned(),
            },
        )
        .expect_err("missing task must fail");

        assert_eq!(err.code, crate::error::ErrorCode::NotFound);
        // Nothing was written: timer still on task A, same revision, and the
        // old session was NOT closed.
        let timer = get_timer(&conn).expect("timer");
        assert_eq!(timer.selected_task_id.as_deref(), Some(task_a.id.as_str()));
        assert_eq!(timer.revision, started.revision);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 0, "no session may be written on a failed switch");
    }

    #[test]
    fn switch_while_paused_starts_the_new_round_paused() {
        let mut conn = db::open_in_memory().expect("database should open");
        let settings = get_settings(&conn).expect("settings");
        let task_a = insert_task(&conn, &create_input("任务 A")).expect("task A");
        let task_b = insert_task(&conn, &create_input("任务 B")).expect("task B");
        let started = start_focus(&mut conn, &settings, Some(&task_a.id));

        let paused = pause_timer(
            &mut conn,
            &crate::models::TimerRevisionInput { expected_revision: started.revision },
        )
        .expect("pause");

        let result = switch_timer_task(
            &mut conn,
            &settings,
            &SwitchTimerTaskInput {
                expected_revision: paused.revision,
                active_session_id: paused.active_session_id.clone().expect("session id"),
                new_task_id: task_b.id.clone(),
            },
        )
        .expect("switch while paused");

        assert_eq!(result.timer.state, TimerState::Paused);
        assert_eq!(result.timer.selected_task_id.as_deref(), Some(task_b.id.as_str()));
        assert_eq!(result.timer.remaining_seconds, result.timer.duration_seconds);
        assert!(result.timer.target_end_at.is_none());
    }

    #[test]
    fn switch_rejects_stale_revision_and_wrong_active_session() {
        let mut conn = db::open_in_memory().expect("database should open");
        let settings = get_settings(&conn).expect("settings");
        let task_a = insert_task(&conn, &create_input("任务 A")).expect("task A");
        let task_b = insert_task(&conn, &create_input("任务 B")).expect("task B");
        let started = start_focus(&mut conn, &settings, Some(&task_a.id));
        let session_id = started.active_session_id.clone().expect("session id");

        let stale = switch_timer_task(
            &mut conn,
            &settings,
            &SwitchTimerTaskInput {
                expected_revision: started.revision - 1,
                active_session_id: session_id.clone(),
                new_task_id: task_b.id.clone(),
            },
        )
        .expect_err("stale revision");
        assert_eq!(stale.code, crate::error::ErrorCode::Conflict);

        let wrong_session = switch_timer_task(
            &mut conn,
            &settings,
            &SwitchTimerTaskInput {
                expected_revision: started.revision,
                active_session_id: "some-other-session".to_owned(),
                new_task_id: task_b.id.clone(),
            },
        )
        .expect_err("wrong active session");
        assert_eq!(wrong_session.code, crate::error::ErrorCode::ValidationError);
    }

    #[test]
    fn switch_is_idempotent_on_a_replayed_command() {
        let mut conn = db::open_in_memory().expect("database should open");
        let settings = get_settings(&conn).expect("settings");
        let task_a = insert_task(&conn, &create_input("任务 A")).expect("task A");
        let task_b = insert_task(&conn, &create_input("任务 B")).expect("task B");
        let started = start_focus(&mut conn, &settings, Some(&task_a.id));
        let session_id = started.active_session_id.clone().expect("session id");
        let input = SwitchTimerTaskInput {
            expected_revision: started.revision,
            active_session_id: session_id.clone(),
            new_task_id: task_b.id.clone(),
        };

        let first = switch_timer_task(&mut conn, &settings, &input).expect("first switch");
        // Replay with the ORIGINAL revision: the closed session already
        // exists, so the command defers to it instead of conflicting.
        let replay = switch_timer_task(&mut conn, &settings, &input).expect("replay");

        assert!(!replay.newly_closed);
        assert_eq!(replay.timer.active_session_id, first.timer.active_session_id);
        assert_eq!(replay.timer.revision, first.timer.revision);
        // Only one closed session exists for the original round.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions WHERE id = ?1", params![session_id], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn switch_to_the_current_task_is_a_no_op() {
        let mut conn = db::open_in_memory().expect("database should open");
        let settings = get_settings(&conn).expect("settings");
        let task_a = insert_task(&conn, &create_input("任务 A")).expect("task A");
        let started = start_focus(&mut conn, &settings, Some(&task_a.id));
        let session_id = started.active_session_id.clone().expect("session id");

        let result = switch_timer_task(
            &mut conn,
            &settings,
            &SwitchTimerTaskInput {
                expected_revision: started.revision,
                active_session_id: session_id,
                new_task_id: task_a.id.clone(),
            },
        )
        .expect("no-op switch");

        assert!(!result.newly_closed);
        assert!(result.closed_session.is_none());
        assert_eq!(result.timer.revision, started.revision);
        assert_eq!(result.timer.state, TimerState::Running);
    }

    fn seed_session(
        conn: &Connection,
        id: &str,
        mode: TimerMode,
        status: SessionStatus,
        project: &str,
        focused_seconds: i64,
    ) {
        let (fallback_id, fallback_name) = fallback_tag(conn).expect("fallback tag");
        let eligible = mode == TimerMode::Focus
            && status == SessionStatus::Completed
            && focused_seconds >= MIN_QUALIFYING_FOCUS_SECONDS;
        conn.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                                   tag_name_snapshot, mode, status, planned_seconds,
                                   focused_seconds, started_at, ended_at, finish_reason,
                                   statistics_eligible, qualification_reason)
             VALUES (?1, NULL, 'snapshot', ?2, ?3, ?4, ?5, ?6, 1500, ?7, 1, 2, 'legacy', ?8, ?9)",
            params![
                id,
                project,
                fallback_id,
                fallback_name,
                mode.as_str(),
                status.as_str(),
                focused_seconds,
                eligible as i64,
                if mode != TimerMode::Focus {
                    "non_focus"
                } else if eligible {
                    "qualified"
                } else if status == SessionStatus::Abandoned {
                    "abandoned"
                } else {
                    "too_short"
                },
            ],
        )
        .expect("session should insert");
    }

    #[test]
    fn created_tasks_round_trip_through_the_database() {
        let conn = db::open_in_memory().expect("database should open");

        let task = insert_task(&conn, &create_input("Write the migration")).expect("task should insert");
        let stored = get_task(&conn, &task.id).expect("task should load");

        assert_eq!(stored.title, "Write the migration");
        assert_eq!(stored.pomodoro_target, 4);
        assert_eq!(stored.priority, TaskPriority::High);
        assert_eq!(stored.project, "Abyssal");
        assert!(!stored.done);
        assert_eq!(stored.completed_at, None);
    }

    #[test]
    fn tasks_are_returned_in_sort_order() {
        let conn = db::open_in_memory().expect("database should open");

        let first = insert_task(&conn, &create_input("First")).expect("insert");
        let second = insert_task(&conn, &create_input("Second")).expect("insert");

        let tasks = list_tasks(&conn).expect("list");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, first.id);
        assert_eq!(tasks[1].id, second.id);
        assert!(second.sort_order > first.sort_order);
    }

    #[test]
    fn blank_and_oversized_titles_are_rejected() {
        let conn = db::open_in_memory().expect("database should open");

        let blank = insert_task(&conn, &create_input("   "));
        assert!(matches!(blank, Err(err) if err.code == crate::error::ErrorCode::ValidationError));

        let too_long = insert_task(&conn, &create_input(&"x".repeat(MAX_TITLE_CHARS + 1)));
        assert!(matches!(too_long, Err(err) if err.code == crate::error::ErrorCode::ValidationError));
    }

    #[test]
    fn out_of_range_pomodoro_targets_are_rejected() {
        let conn = db::open_in_memory().expect("database should open");

        for target in [0, -1, MAX_POMODORO_TARGET + 1] {
            let mut input = create_input("Bad target");
            input.pomodoro_target = target;
            let result = insert_task(&conn, &input);
            assert!(result.is_err(), "target {target} should be rejected");
        }
    }

    #[test]
    fn blank_project_falls_back_to_the_default() {
        let conn = db::open_in_memory().expect("database should open");
        let mut input = create_input("No project");
        input.project = "  ".to_owned();

        let task = insert_task(&conn, &input).expect("insert");

        assert_eq!(task.project, DEFAULT_PROJECT);
    }

    #[test]
    fn update_applies_only_the_supplied_fields() {
        let conn = db::open_in_memory().expect("database should open");
        let task = insert_task(&conn, &create_input("Original")).expect("insert");

        let updated = update_task(
            &conn,
            &UpdateTaskInput {
                id: task.id.clone(),
                title: Some("Renamed".to_owned()),
                pomodoro_target: None,
                priority: Some(TaskPriority::Low),
                project: None,
                done: None,
            },
        )
        .expect("update");

        assert_eq!(updated.title, "Renamed");
        assert_eq!(updated.priority, TaskPriority::Low);
        assert_eq!(updated.pomodoro_target, 4, "untouched fields must persist");
        assert_eq!(updated.project, "Abyssal");
    }

    #[test]
    fn toggling_done_sets_and_clears_completed_at() {
        let conn = db::open_in_memory().expect("database should open");
        let task = insert_task(&conn, &create_input("Finish me")).expect("insert");

        let done = update_task(
            &conn,
            &UpdateTaskInput {
                id: task.id.clone(),
                title: None,
                pomodoro_target: None,
                priority: None,
                project: None,
                done: Some(true),
            },
        )
        .expect("update");
        assert!(done.done);
        assert!(done.completed_at.is_some());

        let reopened = update_task(
            &conn,
            &UpdateTaskInput {
                id: task.id.clone(),
                title: None,
                pomodoro_target: None,
                priority: None,
                project: None,
                done: Some(false),
            },
        )
        .expect("update");
        assert!(!reopened.done);
        assert_eq!(reopened.completed_at, None);
    }

    #[test]
    fn updating_or_deleting_a_missing_task_is_not_found() {
        let conn = db::open_in_memory().expect("database should open");

        let update = update_task(
            &conn,
            &UpdateTaskInput {
                id: "missing".to_owned(),
                title: Some("Nope".to_owned()),
                pomodoro_target: None,
                priority: None,
                project: None,
                done: None,
            },
        );
        assert!(matches!(update, Err(err) if err.code == crate::error::ErrorCode::NotFound));

        let delete = delete_task(&conn, "missing");
        assert!(matches!(delete, Err(err) if err.code == crate::error::ErrorCode::NotFound));
    }

    #[test]
    fn delete_removes_the_task() {
        let conn = db::open_in_memory().expect("database should open");
        let task = insert_task(&conn, &create_input("Temporary")).expect("insert");

        delete_task(&conn, &task.id).expect("delete");

        assert!(get_task(&conn, &task.id).is_err());
        assert!(list_tasks(&conn).expect("list").is_empty());
    }

    #[test]
    fn all_time_statistics_only_count_completed_focus_sessions() {
        let conn = db::open_in_memory().expect("database should open");

        seed_session(&conn, "s1", TimerMode::Focus, SessionStatus::Completed, "Abyssal", 1500);
        seed_session(&conn, "s2", TimerMode::Focus, SessionStatus::Completed, "Abyssal", 1200);
        seed_session(&conn, "s3", TimerMode::Focus, SessionStatus::Abandoned, "Abyssal", 900);
        seed_session(&conn, "s4", TimerMode::Short, SessionStatus::Completed, "休息", 300);

        let stats = all_time_statistics(&conn).expect("statistics");

        assert_eq!(stats.focus_session_count, 2);
        assert_eq!(stats.focus_seconds, 2700);
        assert_eq!(stats.daily_goal, 8);
        assert_eq!(stats.by_project.len(), 1);
        assert_eq!(stats.by_project[0].project, "Abyssal");
        assert_eq!(stats.by_project[0].sessions, 2);
        assert_eq!(stats.by_project[0].focus_seconds, 2700);
    }

    #[test]
    fn save_settings_persists_and_returns_the_updated_row() {
        let conn = db::open_in_memory().expect("database should open");

        let mut settings = get_settings(&conn).expect("default settings");
        settings.focus_duration_minutes = 30;
        settings.short_break_minutes = 10;
        settings.long_break_minutes = 20;
        settings.daily_goal = 6;
        settings.auto_start_break = true;

        let result = save_settings(&conn, &settings).expect("save");
        assert_eq!(result.settings.focus_duration_minutes, 30);
        assert_eq!(result.settings.short_break_minutes, 10);
        assert_eq!(result.settings.long_break_minutes, 20);
        assert_eq!(result.settings.daily_goal, 6);
        assert!(result.settings.auto_start_break);
        assert!(result.settings.updated_at > 0);

        // Re-read to confirm persistence.
        let reread = get_settings(&conn).expect("re-read");
        assert_eq!(reread.focus_duration_minutes, 30);
        assert_eq!(reread.daily_goal, 6);
    }

    #[test]
    fn save_settings_refreshes_idle_timer_durations() {
        let conn = db::open_in_memory().expect("database should open");

        let mut settings = get_settings(&conn).expect("default settings");
        settings.focus_duration_minutes = 45;

        let result = save_settings(&conn, &settings).expect("save");

        // Idle timer should reflect the new 45-minute focus duration.
        assert_eq!(result.timer.state, TimerState::Idle);
        assert_eq!(result.timer.mode, TimerMode::Focus);
        assert_eq!(result.timer.duration_seconds, 45 * 60);
        assert_eq!(result.timer.remaining_seconds, 45 * 60);
    }

    #[test]
    fn save_settings_rejects_out_of_range_durations_and_goals() {
        let conn = db::open_in_memory().expect("database should open");

        let mut settings = get_settings(&conn).expect("default settings");

        settings.focus_duration_minutes = 0;
        assert!(save_settings(&conn, &settings).is_err());

        settings.focus_duration_minutes = 25;
        settings.short_break_minutes = 200;
        assert!(save_settings(&conn, &settings).is_err());

        settings.short_break_minutes = 5;
        settings.daily_goal = 0;
        assert!(save_settings(&conn, &settings).is_err());

        settings.daily_goal = 100;
        assert!(save_settings(&conn, &settings).is_err());
    }

    // ─── Tag domain tests (v1.1, spec §9) ─────────────────────────────────────

    #[test]
    fn migrates_seeded_tags_are_listed_in_order() {
        let conn = db::open_in_memory().expect("db");

        let tags = list_tags(&conn).expect("list tags");

        assert_eq!(tags.len(), 4);
        assert_eq!(tags[0].name, "学习");
        assert_eq!(tags[1].name, "工作");
        assert_eq!(tags[2].name, "生活");
        assert_eq!(tags[3].name, "其他");
        assert!(tags.iter().all(|t| t.kind == TagKind::System));
        assert_eq!(tags[3].id, "system-other");
        assert!(tags[3].is_fallback);
        assert!(tags[..3].iter().all(|t| !t.is_fallback));
    }

    #[test]
    fn creates_a_custom_tag_and_lists_it_last() {
        let conn = db::open_in_memory().expect("db");

        let created = create_tag(&conn, &CreateTagInput { name: "  运动  ".to_owned() })
            .expect("create tag");

        assert_eq!(created.name, "运动");
        assert_eq!(created.kind, TagKind::Custom);
        assert!(!created.is_fallback);

        let tags = list_tags(&conn).expect("list");
        assert_eq!(tags.len(), 5);
        assert_eq!(tags[4].name, "运动", "new tag appends after the defaults");
    }

    #[test]
    fn rejects_duplicate_tag_names_case_insensitively() {
        let conn = db::open_in_memory().expect("db");

        create_tag(&conn, &CreateTagInput { name: "阅读".to_owned() }).expect("first");

        let duplicate = create_tag(&conn, &CreateTagInput { name: "  阅读 ".to_owned() });
        assert!(matches!(duplicate, Err(ref e) if e.code == crate::error::ErrorCode::ValidationError));

        // Also against the seeded system tags (normalized_name compare).
        let clash = create_tag(&conn, &CreateTagInput { name: "工作".to_owned() });
        assert!(matches!(clash, Err(ref e) if e.code == crate::error::ErrorCode::ValidationError));
    }

    #[test]
    fn rejects_blank_overlong_or_control_char_tag_names() {
        let conn = db::open_in_memory().expect("db");

        assert!(create_tag(&conn, &CreateTagInput { name: "   ".to_owned() }).is_err());
        assert!(create_tag(&conn, &CreateTagInput { name: "\n".to_owned() }).is_err());
        assert!(
            create_tag(&conn, &CreateTagInput {
                name: "标".repeat(MAX_TAG_NAME_CHARS + 1),
            })
            .is_err(),
            "names are counted in characters, so 21 Chinese chars must fail"
        );
        // Exactly 20 chars is fine.
        create_tag(&conn, &CreateTagInput { name: "标".repeat(MAX_TAG_NAME_CHARS) })
            .expect("20-char name is legal");
    }

    #[test]
    fn enforces_hard_limit_of_100_tags() {
        let conn = db::open_in_memory().expect("db");
        // 4 system tags exist; create up to the cap.
        for i in 4..MAX_TAGS {
            create_tag(&conn, &CreateTagInput { name: format!("标签{i}") }).expect("within cap");
        }
        let overflow = create_tag(&conn, &CreateTagInput { name: "超额".to_owned() });
        assert!(matches!(overflow, Err(ref e) if e.code == crate::error::ErrorCode::ValidationError));
    }

    #[test]
    fn renames_a_tag_and_rejects_conflicts() {
        let conn = db::open_in_memory().expect("db");
        let tag = create_tag(&conn, &CreateTagInput { name: "旧名".to_owned() }).expect("create");

        let renamed = update_tag(&conn, &UpdateTagInput {
            id: tag.id.clone(),
            name: Some("新名".to_owned()),
        })
        .expect("rename");
        assert_eq!(renamed.name, "新名");

        // Rename onto another tag's name is rejected.
        let clash = update_tag(&conn, &UpdateTagInput {
            id: tag.id.clone(),
            name: Some("工作".to_owned()),
        });
        assert!(matches!(clash, Err(ref e) if e.code == crate::error::ErrorCode::ValidationError));

        // The fallback tag can be renamed but never removed via update.
        let fallback = list_tags(&conn).expect("list").into_iter()
            .find(|t| t.is_fallback).expect("fallback");
        let renamed_fallback = update_tag(&conn, &UpdateTagInput {
            id: fallback.id.clone(),
            name: Some("其余".to_owned()),
        })
        .expect("fallback rename is allowed");
        assert_eq!(renamed_fallback.name, "其余");
    }

    #[test]
    fn reorders_tag_by_swapping_with_neighbour() {
        let conn = db::open_in_memory().expect("db");
        let custom = create_tag(&conn, &CreateTagInput { name: "自定义".to_owned() }).expect("create");

        // Move up: swaps with 其他 (the previous tag in display order).
        let moved_up = reorder_tag(&conn, &crate::models::ReorderTagInput {
            id: custom.id.clone(),
            direction: -1,
        })
        .expect("reorder");
        assert_eq!(moved_up[3].name, "自定义");
        assert_eq!(moved_up[4].name, "其他");

        // Moving the first tag up is a no-op.
        let unchanged = reorder_tag(&conn, &crate::models::ReorderTagInput {
            id: moved_up[0].id.clone(),
            direction: -1,
        })
        .expect("reorder at boundary");
        assert_eq!(unchanged[0].name, "学习");
    }

    #[test]
    fn preview_delete_reports_affected_tasks() {
        let conn = db::open_in_memory().expect("db");
        let tag = create_tag(&conn, &CreateTagInput { name: "项目".to_owned() }).expect("create");
        for i in 0..2 {
            insert_task(&conn, &CreateTaskInput {
                title: format!("任务{i}"),
            tag_id: String::new(),
                pomodoro_target: 1,
                priority: TaskPriority::Med,
                project: "通用".to_owned(),
            })
            .expect("task");
            conn.execute("UPDATE tasks SET tag_id = ?1 WHERE title = ?2", params![tag.id, format!("任务{i}")])
                .expect("attach tag");
        }

        let preview = preview_delete_tag(&conn, &tag.id).expect("preview");
        assert_eq!(preview.affected_tasks, 2);

        let fallback = list_tags(&conn).expect("list").into_iter().find(|t| t.is_fallback).unwrap();
        let conflict = preview_delete_tag(&conn, &fallback.id);
        assert!(matches!(conflict, Err(ref e) if e.code == crate::error::ErrorCode::Conflict));
    }

    #[test]
    fn delete_tag_reassigns_tasks_and_preserves_snapshots() {
        let mut conn = db::open_in_memory().expect("db");
        let tag = create_tag(&conn, &CreateTagInput { name: "将被删除".to_owned() }).expect("create");

        // A task on the tag + a session whose snapshot froze that tag name.
        insert_task(&conn, &CreateTaskInput {
            title: "会转移的任务".to_owned(),
            tag_id: String::new(),
            pomodoro_target: 1,
            priority: TaskPriority::Med,
            project: "通用".to_owned(),
        })
        .expect("task");
        conn.execute("UPDATE tasks SET tag_id = ?1", params![tag.id]).expect("attach");
        conn.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                                   tag_name_snapshot, mode, status, planned_seconds,
                                   focused_seconds, started_at, ended_at, finish_reason,
                                   statistics_eligible, qualification_reason)
             VALUES ('s1', NULL, 'task', 'P', ?1, ?2, 'focus', 'completed', 1500, 600, 1, 2,
                     'elapsed', 1, 'qualified')",
            params![tag.id, tag.name],
        )
        .expect("session");

        let result = delete_tag(&mut conn, &tag.id).expect("delete");

        assert_eq!(result.reassigned_tasks, 1);
        assert_eq!(result.deleted_tag_id, tag.id);

        // Task moved to the fallback tag.
        let tasks = list_tasks(&conn).expect("tasks");
        assert_eq!(tasks[0].tag_id, result.fallback_tag_id);

        // Historical session keeps its frozen snapshot; the stable id is nulled.
        let snapshot: Option<String> = conn
            .query_row("SELECT tag_name_snapshot FROM sessions WHERE id = 's1'", [], |r| r.get(0))
            .expect("snapshot preserved");
        assert_eq!(snapshot.as_deref(), Some(tag.name.as_str()));
        let history_tag: Option<String> = conn
            .query_row("SELECT tag_id FROM sessions WHERE id = 's1'", [], |r| r.get(0))
            .expect("history tag id nulled");
        assert_eq!(history_tag, None, "tag_id uses ON DELETE SET NULL");

        // The tag is gone and cannot be deleted twice.
        assert!(matches!(
            delete_tag(&mut conn, &tag.id),
            Err(ref e) if e.code == crate::error::ErrorCode::NotFound
        ));
    }

    #[test]
    fn delete_fallback_tag_is_rejected() {
        let mut conn = db::open_in_memory().expect("db");
        let fallback = list_tags(&conn).expect("list").into_iter().find(|t| t.is_fallback).unwrap();

        let rejected = delete_tag(&mut conn, &fallback.id);
        assert!(matches!(rejected, Err(ref e) if e.code == crate::error::ErrorCode::Conflict));
        assert_eq!(count_tags(&conn), 4);
    }

    fn count_tags(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0)).expect("count")
    }

    #[test]
    fn new_tasks_default_to_fallback_tag() {
        let conn = db::open_in_memory().expect("db");
        let task = insert_task(&conn, &create_input("默认标签任务")).expect("task");
        let fallback = list_tags(&conn).expect("list").into_iter().find(|t| t.is_fallback).unwrap();
        assert_eq!(task.tag_id, fallback.id);
    }

    #[test]
    fn start_timer_freezes_task_tag() {
        let mut conn = db::open_in_memory().expect("db");
        let tag = create_tag(&conn, &CreateTagInput { name: "工作".to_owned() });
        assert!(tag.is_err(), "工作 is a seeded system tag — use another name");
        let tag = create_tag(&conn, &CreateTagInput { name: "深度工作".to_owned() }).expect("create");

        insert_task(&conn, &CreateTaskInput {
            title: "带标签的任务".to_owned(),
            tag_id: String::new(),
            pomodoro_target: 1,
            priority: TaskPriority::Med,
            project: "通用".to_owned(),
        })
        .expect("task");
        conn.execute("UPDATE tasks SET tag_id = ?1 WHERE title = '带标签的任务'", params![tag.id])
            .expect("attach tag");
        let task_id: String = conn
            .query_row("SELECT id FROM tasks WHERE title = '带标签的任务'", [], |r| r.get(0))
            .expect("task id");

        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0,
            mode: TimerMode::Focus,
            selected_task_id: Some(task_id),
        })
        .expect("start");

        assert_eq!(timer.tag_id.as_deref(), Some(tag.id.as_str()));
        assert_eq!(timer.tag_name_snapshot.as_deref(), Some("深度工作"));
    }

    // ─── Statistics tests (T9) ───────────────────────────────────────────────

    fn seed_completed_focus(conn: &Connection, id: &str, started_at: i64, focused: i64, project: &str) {
        let (fallback_id, fallback_name) = fallback_tag(conn).expect("fallback tag");
        let eligible = focused >= MIN_QUALIFYING_FOCUS_SECONDS;
        conn.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                                   tag_name_snapshot, mode, status, planned_seconds,
                                   focused_seconds, started_at, ended_at, finish_reason,
                                   statistics_eligible, qualification_reason)
             VALUES (?1, NULL, 'task', ?2, ?3, ?4, 'focus', 'completed', 1500, ?5, ?6, ?7,
                     'legacy', ?8, ?9)",
            params![
                id,
                project,
                fallback_id,
                fallback_name,
                focused,
                started_at,
                started_at + focused * 1000,
                eligible as i64,
                if eligible { "qualified" } else { "too_short" },
            ],
        )
        .expect("seed session");
    }

    #[test]
    fn get_statistics_buckets_sessions_into_explicit_day_boundaries() {
        let conn = db::open_in_memory().expect("db");

        // Day 1: 0..86400000, Day 2: 86400000..172800000
        seed_completed_focus(&conn, "s1", 1000, 1500, "Abyssal");
        seed_completed_focus(&conn, "s2", 50000000, 1500, "Abyssal");
        seed_completed_focus(&conn, "s3", 80000000, 1200, "Design");
        seed_completed_focus(&conn, "s4", 100000000, 1500, "Abyssal");

        let stats = get_statistics(
            &conn,
            &StatisticsQuery {
                from: 0,
                to: 172800000,
                days: vec![
                    StatisticsDayBoundary { date: "2026-01-01".into(), from: 0, to: 86400000 },
                    StatisticsDayBoundary { date: "2026-01-02".into(), from: 86400000, to: 172800000 },
                ],
            },
        )
        .expect("stats");

        assert_eq!(stats.focus_session_count, 4);
        assert_eq!(stats.focus_seconds, 5700);
        assert_eq!(stats.by_day.len(), 2);
        assert_eq!(stats.by_day[0].date, "2026-01-01");
        assert_eq!(stats.by_day[0].sessions, 3);
        assert_eq!(stats.by_day[0].focus_seconds, 4200);
        assert_eq!(stats.by_day[1].date, "2026-01-02");
        assert_eq!(stats.by_day[1].sessions, 1);
        assert_eq!(stats.by_day[1].focus_seconds, 1500);
    }

    #[test]
    fn get_statistics_computes_by_project_aggregation() {
        let conn = db::open_in_memory().expect("db");

        seed_completed_focus(&conn, "s1", 1000, 1500, "Abyssal");
        seed_completed_focus(&conn, "s2", 2000, 1500, "Abyssal");
        seed_completed_focus(&conn, "s3", 3000, 900, "Design");

        let stats = get_statistics(
            &conn,
            &StatisticsQuery {
                from: 0,
                to: 172800000,
                days: vec![],
            },
        )
        .expect("stats");

        assert_eq!(stats.by_project.len(), 2);
        let abyssal = stats.by_project.iter().find(|p| p.project == "Abyssal").expect("Abyssal");
        assert_eq!(abyssal.sessions, 2);
        assert_eq!(abyssal.focus_seconds, 3000);
    }

    #[test]
    fn get_statistics_finds_best_day() {
        let conn = db::open_in_memory().expect("db");

        seed_completed_focus(&conn, "s1", 1000, 1500, "A");
        seed_completed_focus(&conn, "s2", 90000000, 3000, "B");

        let stats = get_statistics(
            &conn,
            &StatisticsQuery {
                from: 0,
                to: 172800000,
                days: vec![
                    StatisticsDayBoundary { date: "d1".into(), from: 0, to: 86400000 },
                    StatisticsDayBoundary { date: "d2".into(), from: 86400000, to: 172800000 },
                ],
            },
        )
        .expect("stats");

        assert_eq!(stats.best_day, Some("d2".to_owned()));
    }

    #[test]
    fn get_statistics_aggregates_by_tag_snapshot() {
        let conn = db::open_in_memory().expect("db");
        // Sessions with explicit tag snapshots (as they would be frozen at
        // start time). One shares the snapshot with another → grouped.
        let rows = [
            ("s1", "工作", 600i64, 100i64),
            ("s2", "工作", 900, 200),
            ("s3", "学习", 300, 300),
        ];
        for (id, tag, focused, started) in rows {
            let (fallback_id, _) = fallback_tag(&conn).expect("fallback");
            conn.execute(
                "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                       tag_name_snapshot, mode, status, planned_seconds, focused_seconds,
                       started_at, ended_at, finish_reason, statistics_eligible, qualification_reason)
                 VALUES (?1, NULL, 't', 'P', ?2, ?3, 'focus', 'completed', 1500, ?4,
                         ?5, ?6, 'manual_finish', 1, 'qualified')",
                params![id, fallback_id, tag, focused, started, started + focused * 1000],
            )
            .expect("seed");
        }

        let stats = get_statistics(
            &conn,
            &StatisticsQuery { from: 0, to: 1_000_000_000, days: vec![] },
        )
        .expect("stats");

        assert_eq!(stats.by_tag.len(), 2);
        let work = stats.by_tag.iter().find(|t| t.project == "工作").expect("工作");
        assert_eq!(work.sessions, 2);
        assert_eq!(work.focus_seconds, 1500);
        let study = stats.by_tag.iter().find(|t| t.project == "学习").expect("学习");
        assert_eq!(study.focus_seconds, 300);
    }

    #[test]
    fn get_statistics_computes_streak_from_consecutive_days() {
        let conn = db::open_in_memory().expect("db");

        // 3 consecutive days with sessions, then a gap.
        seed_completed_focus(&conn, "s1", 0, 1500, "A");
        seed_completed_focus(&conn, "s2", 86400000, 1500, "A");
        seed_completed_focus(&conn, "s3", 172800000, 1500, "A");
        // Day 4: no session.
        // Day 5: session (streak should be 2 from the end).
        seed_completed_focus(&conn, "s4", 345600000, 1500, "A");

        let stats = get_statistics(
            &conn,
            &StatisticsQuery {
                from: 0,
                to: 432000000,
                days: vec![
                    StatisticsDayBoundary { date: "d1".into(), from: 0, to: 86400000 },
                    StatisticsDayBoundary { date: "d2".into(), from: 86400000, to: 172800000 },
                    StatisticsDayBoundary { date: "d3".into(), from: 172800000, to: 259200000 },
                    StatisticsDayBoundary { date: "d4".into(), from: 259200000, to: 345600000 },
                    StatisticsDayBoundary { date: "d5".into(), from: 345600000, to: 432000000 },
                ],
            },
        )
        .expect("stats");

        // Walking backwards: d5 has 1 session, d4 has 0 → streak = 1.
        assert_eq!(stats.streak_days, 1);
    }

    #[test]
    fn get_statistics_rejects_overlapping_day_boundaries() {
        let conn = db::open_in_memory().expect("db");

        let result = get_statistics(
            &conn,
            &StatisticsQuery {
                from: 0,
                to: 172800000,
                days: vec![
                    StatisticsDayBoundary { date: "d1".into(), from: 0, to: 90000000 },
                    StatisticsDayBoundary { date: "d2".into(), from: 80000000, to: 172800000 },
                ],
            },
        );
        assert!(matches!(result, Err(ref e) if e.code == crate::error::ErrorCode::ValidationError));
    }

    #[test]
    fn get_statistics_rejects_unordered_dates() {
        let conn = db::open_in_memory().expect("db");

        let result = get_statistics(
            &conn,
            &StatisticsQuery {
                from: 0,
                to: 172800000,
                days: vec![
                    StatisticsDayBoundary { date: "2026-01-02".into(), from: 0, to: 86400000 },
                    StatisticsDayBoundary { date: "2026-01-01".into(), from: 86400000, to: 172800000 },
                ],
            },
        );
        assert!(matches!(result, Err(ref e) if e.code == crate::error::ErrorCode::ValidationError));
    }

    #[test]
    fn get_statistics_rejects_boundary_outside_total_range() {
        let conn = db::open_in_memory().expect("db");

        let result = get_statistics(
            &conn,
            &StatisticsQuery {
                from: 1000,
                to: 2000,
                days: vec![
                    StatisticsDayBoundary { date: "d1".into(), from: 0, to: 500 },
                ],
            },
        );
        assert!(matches!(result, Err(ref e) if e.code == crate::error::ErrorCode::ValidationError));
    }

    #[test]
    fn list_sessions_query_filters_by_time_range_and_limit() {
        let conn = db::open_in_memory().expect("db");

        seed_completed_focus(&conn, "s1", 100, 1500, "A");
        seed_completed_focus(&conn, "s2", 200, 1500, "A");
        seed_completed_focus(&conn, "s3", 300, 1500, "A");

        let all = list_sessions_query(&conn, &SessionQuery { limit: None, from: None, to: None, scope: None })
            .expect("query");
        assert_eq!(all.len(), 3);

        let limited = list_sessions_query(&conn, &SessionQuery { limit: Some(2), from: None, to: None, scope: None })
            .expect("query");
        assert_eq!(limited.len(), 2);
        // Most recent first.
        assert_eq!(limited[0].id, "s3");

        let ranged = list_sessions_query(&conn, &SessionQuery { limit: None, from: Some(150), to: Some(250), scope: None })
            .expect("query");
        assert_eq!(ranged.len(), 1);
        assert_eq!(ranged[0].id, "s2");
    }

    // ─── Timer state machine tests ───────────────────────────────────────────

    fn settings() -> AppSettings {
        AppSettings::default()
    }

    #[test]
    fn start_timer_creates_a_running_session_with_task_snapshot() {
        let mut conn = db::open_in_memory().expect("db");
        let task = insert_task(&conn, &CreateTaskInput {
            title: "Write tests".to_owned(),
            tag_id: String::new(),
            pomodoro_target: 3,
            priority: TaskPriority::High,
            project: "Backend".to_owned(),
        }).expect("task");

        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0,
            mode: TimerMode::Focus,
            selected_task_id: Some(task.id.clone()),
        }).expect("start");

        assert_eq!(timer.state, TimerState::Running);
        assert_eq!(timer.mode, TimerMode::Focus);
        assert!(timer.active_session_id.is_some());
        assert_eq!(timer.selected_task_id, Some(task.id));
        assert_eq!(timer.task_title_snapshot.as_deref(), Some("Write tests"));
        assert_eq!(timer.project_snapshot.as_deref(), Some("Backend"));
        assert!(timer.target_end_at.is_some());
        assert_eq!(timer.revision, 1);
        assert_eq!(timer.duration_seconds, 1500);
    }

    #[test]
    fn start_timer_without_task_uses_fixed_snapshot() {
        let mut conn = db::open_in_memory().expect("db");

        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0,
            mode: TimerMode::Focus,
            selected_task_id: None,
        }).expect("start");

        assert_eq!(timer.task_title_snapshot.as_deref(), Some("未指定任务"));
        assert_eq!(timer.project_snapshot.as_deref(), Some("通用"));
    }

    #[test]
    fn start_short_break_uses_break_snapshot() {
        let mut conn = db::open_in_memory().expect("db");

        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0,
            mode: TimerMode::Short,
            selected_task_id: None,
        }).expect("start");

        assert_eq!(timer.task_title_snapshot.as_deref(), Some("短休"));
        assert_eq!(timer.project_snapshot.as_deref(), Some("休息"));
        assert_eq!(timer.duration_seconds, 300);
    }

    #[test]
    fn start_rejects_wrong_revision() {
        let mut conn = db::open_in_memory().expect("db");

        let result = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 99,
            mode: TimerMode::Focus,
            selected_task_id: None,
        });

        assert!(matches!(result, Err(e) if e.code == crate::error::ErrorCode::Conflict));
    }

    #[test]
    fn start_rejects_already_running() {
        let mut conn = db::open_in_memory().expect("db");
        start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("first start");

        let result = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 1, mode: TimerMode::Focus, selected_task_id: None,
        });

        assert!(result.is_err());
    }

    #[test]
    fn pause_resume_round_trip_preserves_remaining() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");

        // Simulate 5 seconds elapsed.
        timer.target_end_at = Some(now_millis() + (1500 - 5) * 1000);
        write_timer(&conn, &timer).expect("write");

        let paused = pause_timer(&mut conn, &crate::models::TimerRevisionInput {
            expected_revision: 1,
        }).expect("pause");

        assert_eq!(paused.state, TimerState::Paused);
        assert!(paused.target_end_at.is_none());
        assert!(paused.paused_at.is_some());
        assert_eq!(paused.revision, 2);
        // Remaining should be ~1495 (allow small timing drift).
        assert!(paused.remaining_seconds >= 1485 && paused.remaining_seconds <= 1500);

        let resumed = resume_timer(&mut conn, &crate::models::TimerRevisionInput {
            expected_revision: 2,
        }).expect("resume");

        assert_eq!(resumed.state, TimerState::Running);
        assert!(resumed.target_end_at.is_some());
        assert!(resumed.paused_at.is_none());
        assert_eq!(resumed.revision, 3);
    }

    #[test]
    fn pause_rejects_non_running() {
        let mut conn = db::open_in_memory().expect("db");

        let result = pause_timer(&mut conn, &crate::models::TimerRevisionInput {
            expected_revision: 0,
        });

        assert!(result.is_err());
    }

    #[test]
    fn reset_from_running_writes_abandoned_and_returns_idle() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");

        // Simulate 10s elapsed.
        timer.target_end_at = Some(now_millis() + 1490 * 1000);
        write_timer(&conn, &timer).expect("write");

        let reset = reset_timer(&mut conn, &settings(), &crate::models::TimerRevisionInput {
            expected_revision: 1,
        }).expect("reset");

        assert_eq!(reset.state, TimerState::Idle);
        assert_eq!(reset.active_session_id, None);
        assert_eq!(reset.revision, 2);
        assert_eq!(reset.remaining_seconds, 1500);

        // v1.1 ruling: reset records are internal — hidden from the activity
        // view, preserved in full exports.
        let activity = list_sessions(&conn, 10).expect("activity");
        assert!(activity.is_empty(), "reset record must not appear in the activity view");

        let everything = list_sessions_query(&conn, &SessionQuery {
            limit: None, from: None, to: None, scope: Some(crate::models::SessionScope::All),
        }).expect("all");
        assert_eq!(everything.len(), 1);
        assert_eq!(everything[0].status, SessionStatus::Abandoned);
        assert_eq!(everything[0].finish_reason.as_deref(), Some("reset"));
        assert_eq!(everything[0].statistics_eligible, Some(false));
    }

    #[test]
    fn reset_from_idle_does_not_create_session() {
        let mut conn = db::open_in_memory().expect("db");

        let reset = reset_timer(&mut conn, &settings(), &crate::models::TimerRevisionInput {
            expected_revision: 0,
        }).expect("reset");

        assert_eq!(reset.state, TimerState::Idle);
        assert_eq!(reset.revision, 1);

        let sessions = list_sessions(&conn, 10).expect("sessions");
        assert!(sessions.is_empty());
    }

    #[test]
    fn switch_mode_rejects_active_timer() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");

        // Simulate 10 minutes focused (15 remaining).
        timer.target_end_at = Some(now_millis() + 900 * 1000);
        write_timer(&conn, &timer).expect("write");

        // Running: switching is rejected (decision D-2), nothing written.
        let conflict = switch_timer_mode(&mut conn, &settings(), &SwitchTimerModeInput {
            expected_revision: 1, mode: TimerMode::Short,
        });
        assert!(matches!(conflict, Err(ref e) if e.code == crate::error::ErrorCode::Conflict));

        // Paused: equally rejected.
        pause_timer(&mut conn, &crate::models::TimerRevisionInput {
            expected_revision: 1,
        }).expect("pause");
        let conflict = switch_timer_mode(&mut conn, &settings(), &SwitchTimerModeInput {
            expected_revision: 2, mode: TimerMode::Long,
        });
        assert!(matches!(conflict, Err(ref e) if e.code == crate::error::ErrorCode::Conflict));

        // No session may be created by a rejected switch.
        let sessions = list_sessions(&conn, 10).expect("sessions");
        assert!(sessions.is_empty());

        // idle/done switches stay legal and session-free.
        resume_timer(&mut conn, &crate::models::TimerRevisionInput {
            expected_revision: 2,
        }).expect("resume");
        reset_timer(&mut conn, &settings(), &crate::models::TimerRevisionInput {
            expected_revision: 3,
        }).expect("reset");
        let switched = switch_timer_mode(&mut conn, &settings(), &SwitchTimerModeInput {
            expected_revision: 4, mode: TimerMode::Short,
        }).expect("switch from idle");
        assert_eq!(switched.mode, TimerMode::Short);
        assert_eq!(switched.state, TimerState::Idle);
        assert_eq!(switched.duration_seconds, 300);
    }

    #[test]
    fn complete_timer_creates_completed_session_and_done_timer() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        // Natural expiry: push the deadline into the past.
        timer.target_end_at = Some(now_millis() - 500);
        write_timer(&conn, &timer).expect("write");

        let result = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 1,
            active_session_id: session_id.clone(),
            recovery: None,
        }).expect("complete");

        assert!(result.newly_completed);
        assert_eq!(result.timer.state, TimerState::Done);
        assert_eq!(result.timer.remaining_seconds, 0);
        assert_eq!(result.timer.revision, 2);
        assert_eq!(result.session.id, session_id);
        assert_eq!(result.session.status, SessionStatus::Completed);
        assert_eq!(result.session.mode, TimerMode::Focus);
    }

    #[test]
    fn complete_timer_is_idempotent() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        // Natural expiry: push the deadline into the past.
        timer.target_end_at = Some(now_millis() - 500);
        write_timer(&conn, &timer).expect("write");

        let first = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 1, active_session_id: session_id.clone(), recovery: None,
        }).expect("first complete");

        assert!(first.newly_completed);

        // Second call with the same session ID returns the same session.
        let second = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 2, active_session_id: session_id, recovery: None,
        }).expect("idempotent");

        assert!(!second.newly_completed);
        assert_eq!(second.session.id, first.session.id);
        assert_eq!(second.session.status, SessionStatus::Completed);

        // Only one session in the DB — a legitimate natural completion of the
        // full duration, eligible and visible in the activity view.
        let sessions = list_sessions_query(&conn, &SessionQuery {
            limit: None, from: None, to: None, scope: Some(crate::models::SessionScope::All),
        }).expect("sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].statistics_eligible, Some(true));
        assert_eq!(sessions[0].finish_reason.as_deref(), Some("elapsed"));
    }

    #[test]
    fn complete_timer_rejects_abandoned_session_id() {
        let mut conn = db::open_in_memory().expect("db");
        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        // Reset writes an abandoned session with that ID.
        reset_timer(&mut conn, &settings(), &crate::models::TimerRevisionInput {
            expected_revision: 1,
        }).expect("reset");

        let result = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 2, active_session_id: session_id, recovery: None,
        });

        assert!(matches!(result, Err(e) if e.code == crate::error::ErrorCode::Conflict));
    }

    #[test]
    fn complete_timer_rejects_mismatched_session_id() {
        let mut conn = db::open_in_memory().expect("db");
        start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");

        let result = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 1,
            active_session_id: "wrong-id".to_owned(),
            recovery: None,
        });

        assert!(result.is_err());
    }

    // ─── Quit-while-running persistence (Item 4, Round 1) ─────────────────────

    #[test]
    fn persist_running_as_paused_freezes_remaining_and_bumps_revision() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");

        // Simulate 20 seconds elapsed.
        timer.target_end_at = Some(now_millis() + 1480 * 1000);
        write_timer(&conn, &timer).expect("write");

        persist_running_as_paused(&conn).expect("persist");

        let stored = get_timer(&conn).expect("reload");
        assert_eq!(stored.state, TimerState::Paused);
        assert!(stored.target_end_at.is_none(), "target_end_at must be cleared");
        assert!(stored.paused_at.is_some(), "paused_at must be stamped");
        assert!(stored.remaining_seconds >= 1470 && stored.remaining_seconds <= 1500,
            "remaining should be ~1480, got {}", stored.remaining_seconds);
        assert_eq!(stored.revision, 2, "revision must advance");
        assert!(stored.active_session_id.is_some(), "session must be preserved for manual resume");
    }

    #[test]
    fn persist_running_as_paused_is_a_noop_when_not_running() {
        let conn = db::open_in_memory().expect("db");
        // Idle timer: nothing to freeze.
        persist_running_as_paused(&conn).expect("persist");

        let stored = get_timer(&conn).expect("reload");
        assert_eq!(stored.state, TimerState::Idle);
        assert_eq!(stored.revision, 0, "idle timer must not be mutated");

        // A paused timer must also be left untouched.
        let mut paused = get_timer(&conn).expect("reload");
        paused.state = TimerState::Paused;
        paused.remaining_seconds = 1234;
        paused.revision = 5;
        write_timer(&conn, &paused).expect("write");
        persist_running_as_paused(&conn).expect("persist again");

        let stored2 = get_timer(&conn).expect("reload");
        assert_eq!(stored2.state, TimerState::Paused);
        assert_eq!(stored2.remaining_seconds, 1234);
        assert_eq!(stored2.revision, 5, "paused timer must not be mutated");
    }

    #[test]
    fn quit_as_paused_is_resumable_on_next_launch() {
        let mut conn = db::open_in_memory().expect("db");
        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        // Quit while running → frozen as paused (revision advances 1 → 2).
        persist_running_as_paused(&conn).expect("persist");
        let paused = get_timer(&conn).expect("reload");
        assert_eq!(paused.revision, 2);

        // Next launch sees a paused timer; resume then complete it.
        let resumed = resume_timer(&mut conn, &crate::models::TimerRevisionInput {
            expected_revision: 2,
        }).expect("resume");
        assert_eq!(resumed.state, TimerState::Running);
        assert_eq!(resumed.revision, 3);

        // Natural expiry so the completion is legal (v1.1 deadline guard).
        let mut expired = resumed.clone();
        expired.target_end_at = Some(now_millis() - 500);
        write_timer(&conn, &expired).expect("write");

        let result = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 3, active_session_id: session_id, recovery: None,
        }).expect("complete");
        assert!(result.newly_completed);
        assert_eq!(result.session.status, SessionStatus::Completed);
        assert_eq!(result.session.mode, TimerMode::Focus);
    }

    // ─── finish_timer + qualification + scope (v1.1, review #4/#5/#6) ─────────

    fn count_rows(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
            .expect("count")
    }

    #[test]
    fn finish_under_30s_is_hidden_and_not_counted() {
        let mut conn = db::open_in_memory().expect("db");
        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        // Simulate 29 seconds focused (ceiling keeps 1471s remaining).
        let mut running = timer.clone();
        running.target_end_at = Some(now_millis() + 1471 * 1000);
        write_timer(&conn, &running).expect("write");

        let result = finish_timer(&mut conn, &FinishTimerInput {
            expected_revision: running.revision, active_session_id: session_id,
        }).expect("finish");

        assert!(result.newly_finished);
        assert!(!result.statistics_eligible, "29s focus must not be counted");
        assert_eq!(result.qualification_reason, "too_short");
        assert_eq!(result.timer.state, TimerState::Idle);
        assert_eq!(result.timer.remaining_seconds, 1500);

        // Hidden from the activity view...
        let activity = list_sessions(&conn, 50).expect("activity");
        assert!(activity.is_empty(), "too_short must not appear in the activity view");
        // ...but preserved in the complete export (scope = all).
        let everything = list_sessions_query(&conn, &SessionQuery {
            limit: None, from: None, to: None, scope: Some(crate::models::SessionScope::All),
        }).expect("all");
        assert_eq!(everything.len(), 1);
        assert_eq!(everything[0].finish_reason.as_deref(), Some("manual_finish"));

        let stats = all_time_statistics(&conn).expect("stats");
        assert_eq!(stats.focus_session_count, 0);
    }

    #[test]
    fn finish_at_30s_is_counted_and_visible() {
        let mut conn = db::open_in_memory().expect("db");
        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        let mut running = timer.clone();
        running.target_end_at = Some(now_millis() + 1470 * 1000);
        write_timer(&conn, &running).expect("write");

        let result = finish_timer(&mut conn, &FinishTimerInput {
            expected_revision: running.revision, active_session_id: session_id,
        }).expect("finish");

        assert!(result.statistics_eligible, "30s focus counts (29 excluded, 30 counted)");
        assert_eq!(result.qualification_reason, "qualified");

        let activity = list_sessions(&conn, 50).expect("activity");
        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0].finish_reason.as_deref(), Some("manual_finish"));

        let stats = all_time_statistics(&conn).expect("stats");
        assert_eq!(stats.focus_session_count, 1);
        assert!(stats.focus_seconds >= 30);
    }

    #[test]
    fn finish_from_paused_uses_persisted_remaining() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");

        // Paused with 1470s remaining (30s focused, held while paused).
        timer.state = TimerState::Paused;
        timer.remaining_seconds = 1470;
        timer.target_end_at = None;
        timer.paused_at = Some(now_millis());
        timer.revision = 1;
        write_timer(&conn, &timer).expect("write");

        let result = finish_timer(&mut conn, &FinishTimerInput {
            expected_revision: 1, active_session_id: timer.active_session_id.clone().unwrap(),
        }).expect("finish");

        assert!(result.statistics_eligible, "paused focus uses persisted remaining");
        let activity = list_sessions(&conn, 50).expect("activity");
        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0].focused_seconds, 30);
    }

    #[test]
    fn finish_timer_replay_with_stale_revision_returns_existing() {
        let mut conn = db::open_in_memory().expect("db");
        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        let first = finish_timer(&mut conn, &FinishTimerInput {
            expected_revision: 1, active_session_id: session_id.clone(),
        }).expect("first finish");
        assert!(first.newly_finished);

        // Replayed command with the now-stale revision: idempotency must win
        // over the revision check (review #5).
        let replay = finish_timer(&mut conn, &FinishTimerInput {
            expected_revision: 1, active_session_id: session_id,
        }).expect("replay");

        assert!(!replay.newly_finished);
        assert_eq!(replay.session.id, first.session.id);
        assert_eq!(count_rows(&conn, "sessions"), 1);
    }

    #[test]
    fn complete_then_finish_race_yields_single_session() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        // Natural expiry so complete_timer may legally fire first.
        timer.target_end_at = Some(now_millis() - 500);
        write_timer(&conn, &timer).expect("write");

        let completed = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 1, active_session_id: session_id.clone(), recovery: None,
        }).expect("complete wins");
        assert!(completed.newly_completed);

        // finish_timer arrives afterwards — idempotent, no second session.
        let finished = finish_timer(&mut conn, &FinishTimerInput {
            expected_revision: 1, active_session_id: session_id,
        }).expect("finish after complete");
        assert!(!finished.newly_finished);

        assert_eq!(count_rows(&conn, "sessions"), 1);
    }

    #[test]
    fn finish_then_complete_race_yields_single_session() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");

        // Focus for 30s, then finish wins the race.
        timer.target_end_at = Some(now_millis() + 1470 * 1000);
        write_timer(&conn, &timer).expect("write");
        let session_id = timer.active_session_id.clone().unwrap();

        let finished = finish_timer(&mut conn, &FinishTimerInput {
            expected_revision: 1, active_session_id: session_id.clone(),
        }).expect("finish wins");
        assert!(finished.newly_finished);

        // complete_timer arrives afterwards — idempotent, no second session,
        // and the timer stays idle (the manual-finish terminal state).
        let completed = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 2, active_session_id: session_id, recovery: None,
        }).expect("complete after finish");
        assert!(!completed.newly_completed);

        assert_eq!(count_rows(&conn, "sessions"), 1);
        assert_eq!(finished.timer.state, TimerState::Idle);
    }

    #[test]
    fn complete_timer_before_deadline_is_rejected() {
        let mut conn = db::open_in_memory().expect("db");
        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        // 20 minutes still on the clock — an early call must be rejected.
        let result = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 1, active_session_id: session_id.clone(), recovery: None,
        });

        assert!(matches!(result, Err(ref e) if e.code == crate::error::ErrorCode::Conflict));
        assert!(
            result.err().unwrap().message.contains("finish_timer"),
            "the error must point at the early-exit path"
        );

        // No session written, timer untouched.
        let everything = list_sessions_query(&conn, &SessionQuery {
            limit: None, from: None, to: None, scope: Some(crate::models::SessionScope::All),
        }).expect("all");
        assert!(everything.is_empty(), "rejected completion must not write a session");

        let unchanged = get_timer(&conn).expect("timer");
        assert_eq!(unchanged.state, TimerState::Running);
        assert_eq!(unchanged.revision, 1);
        assert!(unchanged.target_end_at.is_some());
    }

    #[test]
    fn complete_timer_within_scheduling_tolerance_is_allowed() {
        let mut conn = db::open_in_memory().expect("db");
        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: None,
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();

        // 200ms remaining — inside the 250ms scheduling tolerance.
        timer.target_end_at = Some(now_millis() + 200);
        timer.revision = 1;
        write_timer(&conn, &timer).expect("write");

        let result = complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 1, active_session_id: session_id, recovery: None,
        }).expect("within tolerance completes naturally");

        assert_eq!(result.timer.state, TimerState::Done);
        assert!(result.newly_completed);
    }

    #[test]
    fn list_sessions_query_hides_ineligible_by_default() {
        let conn = db::open_in_memory().expect("db");
        seed_completed_focus(&conn, "s-eligible", 100, 1500, "A");
        // Hidden rows written directly: too_short, abandoned focus, break.
        let (fallback_id, fallback_name) = fallback_tag(&conn).expect("fallback");
        for (id, mode, status, focused, qualification) in [
            ("s-short", "focus", "completed", 10i64, "too_short"),
            ("s-abandoned", "focus", "abandoned", 600i64, "abandoned"),
            ("s-break", "short", "completed", 300i64, "non_focus"),
        ] {
            conn.execute(
                "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                       tag_name_snapshot, mode, status, planned_seconds, focused_seconds,
                       started_at, ended_at, finish_reason, statistics_eligible, qualification_reason)
                 VALUES (?1, NULL, 't', 'P', ?2, ?3, ?4, ?5, 1500, ?6, 1, 2, 'legacy', 0, ?7)",
                params![id, fallback_id, fallback_name, mode, status, focused, qualification],
            )
            .expect("hidden session");
        }

        let default_view = list_sessions_query(&conn, &SessionQuery {
            limit: None, from: None, to: None, scope: None,
        }).expect("default");
        assert_eq!(default_view.len(), 1, "activity default hides ineligible rows");
        assert_eq!(default_view[0].id, "s-eligible");

        let everything = list_sessions_query(&conn, &SessionQuery {
            limit: None, from: None, to: None, scope: Some(crate::models::SessionScope::All),
        }).expect("all");
        assert_eq!(everything.len(), 4, "scope=all reads every record");
    }

    #[test]
    fn export_backup_and_csv_preserve_hidden_sessions() {
        let conn = db::open_in_memory().expect("db");
        seed_completed_focus(&conn, "s-eligible", 100, 1500, "A");
        let (fallback_id, fallback_name) = fallback_tag(&conn).expect("fallback");
        conn.execute(
            "INSERT INTO sessions (id, task_id, task_title_snapshot, project_snapshot, tag_id,
                   tag_name_snapshot, mode, status, planned_seconds, focused_seconds,
                   started_at, ended_at, finish_reason, statistics_eligible, qualification_reason)
             VALUES ('s-reset', NULL, 't', 'P', ?1, ?2, 'focus', 'abandoned', 1500, 25, 1, 2,
                     'reset', 0, 'too_short')",
            params![fallback_id, fallback_name],
        )
        .expect("hidden reset record");

        let bundle = export_data(&conn).expect("export");
        assert_eq!(bundle.sessions.len(), 2, "backup must include hidden records");

        let csv = export_sessions_csv(&conn).expect("csv");
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3, "header + 2 records (hidden record included)");
        assert!(csv.contains("reset"), "reset record must be in the CSV");
    }

    // ─── Backup v2: version-header-first parsing (v1.1, review round 2) ───────

    #[test]
    fn exports_v2_bundle_with_tags() {
        let conn = db::open_in_memory().expect("db");
        let bundle = export_data(&conn).expect("export");

        assert_eq!(bundle.schema_version, 2);
        assert_eq!(bundle.tags.len(), 4);
        assert_eq!(bundle.tags.iter().filter(|t| t.is_fallback).count(), 1);
    }

    #[test]
    fn parses_v1_backup_json_via_header_and_backfills() {
        let mut conn = db::open_in_memory().expect("db");

        // A literal v1.0.0 backup: no tags, no tagId, no qualification fields.
        let v1_json = r#"{
            "app": "abyssal-reverie",
            "schemaVersion": 1,
            "exportedAt": 1000,
            "settings": {
                "focusDurationMinutes": 25, "shortBreakMinutes": 5,
                "longBreakMinutes": 15, "autoStartBreak": false,
                "soundEnabled": true, "notificationEnabled": true,
                "dailyGoal": 8, "updatedAt": 0
            },
            "tasks": [
                { "id": "t1", "title": "Legacy task", "done": false,
                  "pomodoroTarget": 2, "priority": "high", "project": "P",
                  "sortOrder": 0, "createdAt": 1, "updatedAt": 1, "completedAt": null }
            ],
            "sessions": [
                { "id": "s1", "taskId": "t1", "taskTitleSnapshot": "Legacy task",
                  "projectSnapshot": "P", "mode": "focus", "status": "completed",
                  "plannedSeconds": 1500, "focusedSeconds": 600, "startedAt": 1, "endedAt": 601 }
            ]
        }"#;

        let bundle = parse_backup_text(v1_json).expect("v1 backup parses");
        assert_eq!(bundle.schema_version, 2, "normalized to the v2 shape");
        assert_eq!(bundle.tags.len(), 4);
        assert_eq!(bundle.tasks[0].tag_id, "system-other");
        assert_eq!(bundle.sessions[0].finish_reason.as_deref(), Some("legacy"));
        assert_eq!(bundle.sessions[0].statistics_eligible, Some(true));
        assert_eq!(bundle.sessions[0].qualification_reason.as_deref(), Some("qualified"));

        let summary = import_data(&mut conn, &bundle).expect("import");
        assert_eq!(summary.tasks, 1);
        assert_eq!(summary.sessions, 1);

        let task_tag: String = conn
            .query_row("SELECT tag_id FROM tasks WHERE id = 't1'", [], |r| r.get(0))
            .expect("task tag");
        assert_eq!(task_tag, "system-other");
        let tags: i64 = conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0)).expect("tags");
        assert_eq!(tags, 4);
    }

    #[test]
    fn rejects_unknown_backup_version() {
        let json = r#"{ "app": "abyssal-reverie", "schemaVersion": 99 }"#;
        let err = parse_backup_text(json).expect_err("must reject");
        assert!(err.message.contains("不受支持"), "got: {}", err.message);
    }

    #[test]
    fn v2_backup_round_trips_through_parse_and_import() {
        let source = db::open_in_memory().expect("db");
        insert_task(&source, &create_input("Round trip")).expect("task");
        seed_completed_focus(&source, "s1", 100, 1500, "A");

        let bundle = export_data(&source).expect("export");
        let text = serde_json::to_string(&bundle).expect("serialize");

        let parsed = parse_backup_text(&text).expect("parse");
        assert_eq!(parsed.tags.len(), bundle.tags.len());
        assert_eq!(parsed.sessions[0].statistics_eligible, Some(true));

        let mut target = db::open_in_memory().expect("db");
        let summary = import_data(&mut target, &parsed).expect("import");
        assert_eq!(summary.sessions, 1);

        let restored = list_sessions_query(&target, &SessionQuery {
            limit: None, from: None, to: None, scope: Some(crate::models::SessionScope::All),
        }).expect("sessions");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].statistics_eligible, Some(true));
        assert_eq!(restored[0].tag_name_snapshot.as_deref(), Some("其他"));
    }

    #[test]
    fn preview_from_bundle_counts_tags() {
        let conn = db::open_in_memory().expect("db");
        let bundle = export_data(&conn).expect("export");
        let preview = preview_from_bundle(&bundle);
        assert_eq!(preview.tags, 4);
        assert_eq!(preview.schema_version, 2);
    }


    // ─── Data export & backup tests (Item 3) ───────────────────────────────

    #[test]
    fn export_then_import_round_trips_all_data() {
        let mut conn = db::open_in_memory().expect("db");

        let task = insert_task(&conn, &CreateTaskInput {
            title: "Backup me".to_owned(),
            tag_id: String::new(),
            pomodoro_target: 6,
            priority: TaskPriority::Low,
            project: "Archive".to_owned(),
        }).expect("insert");

        let mut timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Focus, selected_task_id: Some(task.id.clone()),
        }).expect("start");
        let session_id = timer.active_session_id.clone().unwrap();
        // Natural expiry so the completion is legal (v1.1 deadline guard).
        timer.target_end_at = Some(now_millis() - 500);
        write_timer(&conn, &timer).expect("write");
        complete_timer(&mut conn, &settings(), &CompleteTimerInput {
            expected_revision: 1, active_session_id: session_id, recovery: None,
        }).expect("complete");

        let bundle = export_data(&conn).expect("export");
        assert_eq!(bundle.app, "abyssal-reverie");
        assert_eq!(bundle.tasks.len(), 1);
        assert_eq!(bundle.sessions.len(), 1);

        // Mutate the live DB, then import the bundle back.
        delete_task(&conn, &task.id).expect("delete");
        assert!(list_tasks(&conn).expect("list").is_empty());

        let summary = import_data(&mut conn, &bundle).expect("import");
        assert_eq!(summary.tasks, 1);
        assert_eq!(summary.sessions, 1);

        let restored = list_tasks(&conn).expect("list");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].title, "Backup me");
        assert_eq!(restored[0].project, "Archive");

        // The restored session is a legitimate natural completion (full
        // duration) and survives the round trip.
        let sessions = list_sessions_query(&conn, &SessionQuery {
            limit: None, from: None, to: None, scope: Some(crate::models::SessionScope::All),
        }).expect("sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].statistics_eligible, Some(true));
        assert_eq!(sessions[0].finish_reason.as_deref(), Some("elapsed"));
    }

    #[test]
    fn import_resets_a_running_timer_to_idle() {
        let mut conn = db::open_in_memory().expect("db");

        let timer = start_timer(&mut conn, &settings(), &StartTimerInput {
            expected_revision: 0, mode: TimerMode::Short, selected_task_id: None,
        }).expect("start");
        assert_eq!(timer.state, TimerState::Running);

        let bundle = export_data(&conn).expect("export");
        // Simulate a foreign backup whose settings differ; import must still
        // reset the live timer to idle for the restored mode.
        let summary = import_data(&mut conn, &bundle).expect("import");
        assert_eq!(summary.sessions, 0);

        let restored = get_timer(&conn).expect("timer");
        assert_eq!(restored.state, TimerState::Idle);
        assert_eq!(restored.mode, TimerMode::Short);
        assert_eq!(restored.active_session_id, None);
    }

    #[test]
    fn import_rejects_a_non_abyssal_backup() {
        let mut conn = db::open_in_memory().expect("db");
        let mut bundle = export_data(&conn).expect("export");
        bundle.app = "some-other-app".to_owned();

        let result = import_data(&mut conn, &bundle);
        assert!(matches!(result, Err(ref e) if e.code == crate::error::ErrorCode::ValidationError));
    }

    #[test]
    fn import_rejects_a_bad_backup_and_leaves_existing_data_intact() {
        let mut conn = db::open_in_memory().expect("db");

        // Seed real, in-use data: a task and a completed focus session.
        let task = insert_task(&conn, &CreateTaskInput {
            title: "Real task".to_owned(),
            tag_id: String::new(),
            pomodoro_target: 2,
            priority: TaskPriority::High,
            project: "Real".to_owned(),
        }).expect("seed task");
        seed_session(&conn, "real-session", TimerMode::Focus, SessionStatus::Completed, "Real", 1500);

        // Build a backup from current data, then corrupt it (oversized title).
        let mut bundle = export_data(&conn).expect("export");
        bundle.tasks[0].title = "x".repeat(MAX_TITLE_CHARS + 1);

        let result = import_data(&mut conn, &bundle);
        assert!(matches!(result, Err(ref e) if e.code == crate::error::ErrorCode::ValidationError));

        // Existing task + session must be untouched (no partial overwrite).
        let tasks = list_tasks(&conn).expect("list tasks");
        assert_eq!(tasks.len(), 1, "existing task count preserved");
        assert_eq!(tasks[0].id, task.id);
        assert_eq!(tasks[0].title, "Real task", "existing task not overwritten");

        let sessions = list_sessions(&conn, 10).expect("list sessions");
        assert_eq!(sessions.len(), 1, "existing session count preserved");
        assert_eq!(sessions[0].id, "real-session");
    }

    #[test]
    fn export_sessions_csv_escapes_special_characters() {
        let conn = db::open_in_memory().expect("db");
        seed_session(&conn, "s,1", TimerMode::Focus, SessionStatus::Completed, "Pro,ject", 1500);

        let csv = export_sessions_csv(&conn).expect("csv");
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2, "header + one row");
        assert!(lines[0].starts_with("id,taskId,taskTitle,project"), "header present");
        // Comma-bearing id and project must be quoted.
        assert!(lines[1].contains("\"s,1\""), "id with comma is quoted: {}", lines[1]);
        assert!(lines[1].contains("\"Pro,ject\""), "project with comma is quoted: {}", lines[1]);
    }
}
