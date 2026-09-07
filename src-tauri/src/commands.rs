use std::fs;
use std::sync::MutexGuard;

use rusqlite::Connection;
use tauri::Emitter;
use tauri::AppHandle;
use tauri::Manager;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::error::CommandError;
use crate::models::{
    AppSettings, BootstrapPayload, CompleteTimerInput, CompleteTimerResult, CreateTagInput,
    CreateTaskInput, DeleteTagResult, ExportSummary, FinishTimerInput,
    FinishTimerResult, ImportPreview, ImportSummary, SaveSettingsResult, SessionQuery,
    StartTimerInput, Statistics, StatisticsQuery, SwitchTimerModeInput, Tag, TagDeletePreview,
    Task, TimerRevisionInput, TimerSession, TimerSnapshot, UpdateTagInput, UpdateTaskInput,
};
use crate::repository;
use crate::AppState;

/// How much recent history `bootstrap_app` ships to the frontend.
const BOOTSTRAP_SESSION_LIMIT: i64 = 50;

fn lock_db<'a>(state: &'a State<'_, AppState>) -> Result<MutexGuard<'a, Connection>, CommandError> {
    state
        .db
        .lock()
        .map_err(|err| CommandError::internal(format!("database lock poisoned: {err}")))
}

#[tauri::command]
pub fn bootstrap_app(state: State<'_, AppState>) -> Result<BootstrapPayload, CommandError> {
    // R06: while an old-schema migration awaits user confirmation, no
    // business payload may be served.
    if state
        .migration_pending
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        let conn = lock_db(&state)?;
        let version = crate::db::schema_version(&conn)?;
        return Err(CommandError::migration_required(version, crate::db::LATEST_SCHEMA_VERSION));
    }
    let conn = lock_db(&state)?;

    Ok(BootstrapPayload {
        tasks: repository::list_tasks(&conn)?,
        tags: repository::list_tags(&conn)?,
        settings: repository::get_settings(&conn)?,
        timer: repository::get_timer(&conn)?,
        sessions: repository::list_sessions(&conn, BOOTSTRAP_SESSION_LIMIT)?,
        statistics: repository::all_time_statistics(&conn)?,
    })
}

// ─── Tags (v1.1) ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_tags(state: State<'_, AppState>) -> Result<Vec<Tag>, CommandError> {
    let conn = lock_db(&state)?;
    repository::list_tags(&conn)
}

#[tauri::command]
pub fn create_tag(state: State<'_, AppState>, input: CreateTagInput) -> Result<Tag, CommandError> {
    let conn = lock_db(&state)?;
    repository::create_tag(&conn, &input)
}

#[tauri::command]
pub fn update_tag(state: State<'_, AppState>, input: UpdateTagInput) -> Result<Tag, CommandError> {
    let conn = lock_db(&state)?;
    repository::update_tag(&conn, &input)
}

#[tauri::command]
pub fn reorder_tag(
    state: State<'_, AppState>,
    input: crate::models::ReorderTagInput,
) -> Result<Vec<Tag>, CommandError> {
    let conn = lock_db(&state)?;
    repository::reorder_tag(&conn, &input)
}

#[tauri::command]
pub fn preview_delete_tag(
    state: State<'_, AppState>,
    id: String,
) -> Result<TagDeletePreview, CommandError> {
    let conn = lock_db(&state)?;
    repository::preview_delete_tag(&conn, &id)
}

#[tauri::command]
pub fn delete_tag(
    state: State<'_, AppState>,
    id: String,
) -> Result<DeleteTagResult, CommandError> {
    let mut conn = lock_db(&state)?;
    repository::delete_tag(&mut conn, &id)
}

#[tauri::command]
pub fn create_task(state: State<'_, AppState>, input: CreateTaskInput) -> Result<Task, CommandError> {
    let conn = lock_db(&state)?;
    repository::insert_task(&conn, &input)
}

#[tauri::command]
pub fn update_task(state: State<'_, AppState>, input: UpdateTaskInput) -> Result<Task, CommandError> {
    let conn = lock_db(&state)?;
    repository::update_task(&conn, &input)
}

/// v1.4 (batch 1): atomic task-relationship change (project/tag clear|set)
/// guarded by the task's relationship revision.
#[tauri::command]
pub fn apply_task_relationship(
    state: State<'_, AppState>,
    patch: crate::models::RelationshipPatch,
) -> Result<Task, CommandError> {
    let mut conn = lock_db(&state)?;
    repository::apply_relationship_patch(&mut conn, &patch)
}

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    let conn = lock_db(&state)?;
    repository::delete_task(&conn, &id)
}

// ─── Categories, projects & task ledger (v1.2) ──────────────────────────────

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> Result<Vec<crate::models::Category>, CommandError> {
    let conn = lock_db(&state)?;
    repository::list_categories(&conn)
}

#[tauri::command]
pub fn create_category(state: State<'_, AppState>, input: crate::models::CreateCategoryInput) -> Result<crate::models::Category, CommandError> {
    let conn = lock_db(&state)?;
    repository::create_category(&conn, &input)
}

#[tauri::command]
pub fn update_category(state: State<'_, AppState>, input: crate::models::UpdateCategoryInput) -> Result<crate::models::Category, CommandError> {
    let conn = lock_db(&state)?;
    repository::update_category(&conn, &input)
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<crate::models::Project>, CommandError> {
    let conn = lock_db(&state)?;
    repository::list_projects(&conn)
}

#[tauri::command]
pub fn create_project(state: State<'_, AppState>, input: crate::models::CreateProjectInput) -> Result<crate::models::Project, CommandError> {
    let conn = lock_db(&state)?;
    repository::create_project(&conn, &input)
}

#[tauri::command]
pub fn update_project(state: State<'_, AppState>, input: crate::models::UpdateProjectInput) -> Result<crate::models::Project, CommandError> {
    let conn = lock_db(&state)?;
    repository::update_project(&conn, &input)
}

#[tauri::command]
pub fn get_task_progress(state: State<'_, AppState>, task_id: String) -> Result<crate::models::TaskProgress, CommandError> {
    let conn = lock_db(&state)?;
    repository::get_task_progress(&conn, &task_id)
}

#[tauri::command]
pub fn get_all_task_progress(state: State<'_, AppState>) -> Result<Vec<crate::models::TaskProgress>, CommandError> {
    let conn = lock_db(&state)?;
    repository::get_all_task_progress(&conn)
}

#[tauri::command]
pub fn complete_task_now(app: tauri::AppHandle, state: State<'_, AppState>, input: crate::models::CompleteTaskInput) -> Result<crate::models::CompleteTaskResult, CommandError> {
    let mut conn = lock_db(&state)?;
    let result = repository::complete_task_now(&mut conn, &input)?;
    let _ = app.emit("timer-changed", &result.timer);
    Ok(result)
}

// ─── Migration preparation (R06) ─────────────────────────────────────────────

/// Read-only preview of the pending semantic migration. Changes NOTHING.
#[tauri::command]
pub fn preview_migration(state: State<'_, AppState>) -> Result<crate::models::MigrationPreview, CommandError> {
    let conn = lock_db(&state)?;
    crate::db::preview_v4_migration(&conn)
}

/// Applies the migration with the user's confirmed parameters, then returns
/// the full bootstrap payload so the UI can continue without a restart.
#[tauri::command]
pub fn confirm_migration(
    state: State<'_, AppState>,
    params: crate::models::MigrationParams,
) -> Result<BootstrapPayload, CommandError> {
    if !state
        .migration_pending
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        return Err(CommandError::validation("no migration is pending"));
    }
    if params.budget_focus_minutes < 1 || params.budget_focus_minutes > 180 {
        return Err(CommandError::validation("budget_focus_minutes must be 1..=180"));
    }
    if params.general_mapping != "standalone" && params.general_mapping != "project" {
        return Err(CommandError::validation("general_mapping must be standalone|project"));
    }
    let mut conn = lock_db(&state)?;
    crate::db::run_migrations_with(&mut conn, &params)?;
    crate::db::seed_defaults(&conn)?;
    state
        .migration_pending
        .store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(BootstrapPayload {
        tasks: repository::list_tasks(&conn)?,
        tags: repository::list_tags(&conn)?,
        settings: repository::get_settings(&conn)?,
        timer: repository::get_timer(&conn)?,
        sessions: repository::list_sessions(&conn, BOOTSTRAP_SESSION_LIMIT)?,
        statistics: repository::all_time_statistics(&conn)?,
    })
}

/// The user declined the upgrade — exit without touching any data.
#[tauri::command]
pub fn cancel_upgrade(app: tauri::AppHandle) {
    app.exit(0);
}

// ─── Mini window (v1.3 B/C) ──────────────────────────────────────────────────

/// Shows + focuses the main window (used by the mini window's 返回主窗 button).
#[tauri::command]
pub fn show_main_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Toggles the mini window's visibility (tray menu + mini close button).
#[tauri::command]
pub fn toggle_mini_window(app: tauri::AppHandle) {
    if let Some(mini) = app.get_webview_window("mini") {
        if mini.is_visible().unwrap_or(false) {
            let _ = mini.hide();
        } else {
            let _ = mini.show();
        }
    }
}

/// Loads the mini window's device-local prefs.
#[tauri::command]
pub fn load_mini_prefs(app: tauri::AppHandle) -> crate::prefs::MiniWindowPrefs {
    crate::prefs::load_device_settings(&app).mini
}

/// Persists the mini window's device-local prefs and applies them live.
#[tauri::command]
pub fn save_mini_prefs(
    app: tauri::AppHandle,
    prefs: crate::prefs::MiniWindowPrefs,
) -> Result<(), CommandError> {
    let mut settings = crate::prefs::load_device_settings(&app);
    settings.mini = prefs.clone();
    crate::prefs::save_device_settings(&app, &settings)
        .map_err(|e| CommandError::internal(format!("保存窗口偏好失败: {e}")))?;
    crate::prefs::apply_mini_prefs(&app, &prefs);
    Ok(())
}

/// v1.3 C4: undo a task completion — the task returns to todo while its real
/// recorded segments stay; the main clock remains paused.
#[tauri::command]
pub fn undo_complete_task(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<crate::models::Task, CommandError> {
    let conn = lock_db(&state)?;
    repository::undo_complete_task(&conn, &task_id)
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, input: AppSettings) -> Result<SaveSettingsResult, CommandError> {
    let conn = lock_db(&state)?;
    repository::save_settings(&conn, &input)
}

// ─── Timer state machine ─────────────────────────────────────────────────────

#[tauri::command]
pub fn start_timer(app: tauri::AppHandle, state: State<'_, AppState>, input: StartTimerInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    let snapshot = repository::start_timer(&mut conn, &settings, &input)?;
    let _ = app.emit("timer-changed", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn pause_timer(app: tauri::AppHandle, state: State<'_, AppState>, input: TimerRevisionInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    let snapshot = repository::pause_timer(&mut conn, &input)?;
    let _ = app.emit("timer-changed", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn resume_timer(app: tauri::AppHandle, state: State<'_, AppState>, input: TimerRevisionInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    let snapshot = repository::resume_timer(&mut conn, &input)?;
    let _ = app.emit("timer-changed", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn reset_timer(app: tauri::AppHandle, state: State<'_, AppState>, input: TimerRevisionInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    let snapshot = repository::reset_timer(&mut conn, &settings, &input)?;
    let _ = app.emit("timer-changed", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn switch_timer_mode(app: tauri::AppHandle, state: State<'_, AppState>, input: SwitchTimerModeInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    let snapshot = repository::switch_timer_mode(&mut conn, &settings, &input)?;
    let _ = app.emit("timer-changed", &snapshot);
    Ok(snapshot)
}

/// v1.1.2: switch the active round's task — closes the current session with
/// its actual focused time and opens a new focus session for the chosen task,
/// atomically.
#[tauri::command]
pub fn switch_timer_task(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    input: crate::models::SwitchTimerTaskInput,
) -> Result<crate::models::SwitchTimerTaskResult, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    let result = repository::switch_timer_task(&mut conn, &settings, &input)?;
    let _ = app.emit("timer-changed", &result.timer);
    Ok(result)
}

#[tauri::command]
pub fn complete_timer(app: tauri::AppHandle, state: State<'_, AppState>, input: CompleteTimerInput) -> Result<CompleteTimerResult, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    let result = repository::complete_timer(&mut conn, &settings, &input)?;
    let _ = app.emit("timer-changed", &result.timer);
    Ok(result)
}

// ─── Sessions and statistics ─────────────────────────────────────────────────

#[tauri::command]
pub fn finish_timer(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    input: FinishTimerInput,
) -> Result<FinishTimerResult, CommandError> {
    let mut conn = lock_db(&state)?;
    let result = repository::finish_timer(&mut conn, &input)?;
    let _ = app.emit("timer-changed", &result.timer);
    Ok(result)
}

#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>, query: SessionQuery) -> Result<Vec<TimerSession>, CommandError> {
    let conn = lock_db(&state)?;
    repository::list_sessions_query(&conn, &query)
}

#[tauri::command]
pub fn get_statistics(state: State<'_, AppState>, query: StatisticsQuery) -> Result<Statistics, CommandError> {
    let conn = lock_db(&state)?;
    repository::get_statistics(&conn, &query)
}

// ─── Data export & backup (Item 3) ─────────────────────────────────────────

/// Opens a native "Save As" dialog and returns the chosen path (or `null` if
/// the user cancelled). The dialog is shown from Rust so the file I/O stays
/// behind the single `AppGateway` IPC boundary — no JS dialog plugin needed.
#[tauri::command]
pub fn pick_export_path(app: AppHandle, suggested_name: String) -> Result<Option<String>, CommandError> {
    let chosen = app
        .dialog()
        .file()
        .set_title("导出 Abyssal Reverie 备份")
        .set_file_name(&suggested_name)
        .add_filter("JSON 备份", &["json"])
        .blocking_save_file();
    match chosen {
        Some(p) => {
            let pb = p
                .into_path()
                .map_err(|e| CommandError::internal(format!("无法解析文件路径: {e}")))?;
            Ok(Some(pb.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

/// Opens a native "Open" dialog restricted to JSON backups; returns the chosen
/// path (or `null` if cancelled).
#[tauri::command]
pub fn pick_import_path(app: AppHandle) -> Result<Option<String>, CommandError> {
    let chosen = app
        .dialog()
        .file()
        .set_title("导入 Abyssal Reverie 备份")
        .add_filter("JSON 备份", &["json"])
        .blocking_pick_file();
    match chosen {
        Some(p) => {
            let pb = p
                .into_path()
                .map_err(|e| CommandError::internal(format!("无法解析文件路径: {e}")))?;
            Ok(Some(pb.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

/// Serializes the full backup bundle and writes it to `path`.
#[tauri::command]
pub fn export_backup_to(state: State<'_, AppState>, path: String) -> Result<ExportSummary, CommandError> {
    let conn = lock_db(&state)?;
    let bundle = repository::export_data(&conn)?;
    let json = serde_json::to_string_pretty(&bundle)
        .map_err(|e| CommandError::internal(format!("序列化备份失败: {e}")))?;
    let bytes = json.len() as u64;
    fs::write(&path, json).map_err(|e| CommandError::internal(format!("写入文件失败: {e}")))?;
    Ok(ExportSummary {
        path,
        bytes,
        tasks: bundle.tasks.len() as i64,
        sessions: bundle.sessions.len() as i64,
    })
}

/// Writes a spreadsheet-friendly CSV of all sessions to `path`.
#[tauri::command]
pub fn export_sessions_csv_to(state: State<'_, AppState>, path: String) -> Result<ExportSummary, CommandError> {
    let conn = lock_db(&state)?;
    let csv = repository::export_sessions_csv(&conn)?;
    let bytes = csv.len() as u64;
    fs::write(&path, csv).map_err(|e| CommandError::internal(format!("写入文件失败: {e}")))?;
    let sessions = repository::list_sessions(&conn, i64::MAX)?.len() as i64;
    Ok(ExportSummary {
        path,
        bytes,
        tasks: 0,
        sessions,
    })
}

/// Reads and validates a backup file, returning row counts for the confirm
/// step. Does NOT mutate the database.
#[tauri::command]
pub fn preview_import_from(path: String) -> Result<ImportPreview, CommandError> {
    let raw = fs::read_to_string(&path)
        .map_err(|e| CommandError::validation(format!("无法读取文件: {e}")))?;
    let bundle = repository::parse_backup_text(&raw)?;
    repository::validate_import(&bundle)?;
    Ok(repository::preview_from_bundle(&bundle))
}

/// Replaces tasks, sessions and settings from the chosen backup file.
#[tauri::command]
pub fn import_backup_from(state: State<'_, AppState>, path: String) -> Result<ImportSummary, CommandError> {
    let raw = fs::read_to_string(&path)
        .map_err(|e| CommandError::validation(format!("无法读取文件: {e}")))?;
    let bundle = repository::parse_backup_text(&raw)?;
    let mut conn = lock_db(&state)?;
    let mut summary = repository::import_data(&mut conn, &bundle)?;
    summary.path = path;
    Ok(summary)
}
