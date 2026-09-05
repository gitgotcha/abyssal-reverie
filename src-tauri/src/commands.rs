use std::fs;
use std::sync::MutexGuard;

use rusqlite::Connection;
use tauri::AppHandle;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::error::CommandError;
use crate::models::{
    AppSettings, BootstrapPayload, CompleteTimerInput, CompleteTimerResult, CreateTagInput,
    CreateTaskInput, DeleteTagResult, ExportBundle, ExportSummary, FinishTimerInput,
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

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    let conn = lock_db(&state)?;
    repository::delete_task(&conn, &id)
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, input: AppSettings) -> Result<SaveSettingsResult, CommandError> {
    let conn = lock_db(&state)?;
    repository::save_settings(&conn, &input)
}

// ─── Timer state machine ─────────────────────────────────────────────────────

#[tauri::command]
pub fn start_timer(state: State<'_, AppState>, input: StartTimerInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    repository::start_timer(&mut conn, &settings, &input)
}

#[tauri::command]
pub fn pause_timer(state: State<'_, AppState>, input: TimerRevisionInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    repository::pause_timer(&mut conn, &input)
}

#[tauri::command]
pub fn resume_timer(state: State<'_, AppState>, input: TimerRevisionInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    repository::resume_timer(&mut conn, &input)
}

#[tauri::command]
pub fn reset_timer(state: State<'_, AppState>, input: TimerRevisionInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    repository::reset_timer(&mut conn, &settings, &input)
}

#[tauri::command]
pub fn switch_timer_mode(state: State<'_, AppState>, input: SwitchTimerModeInput) -> Result<TimerSnapshot, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    repository::switch_timer_mode(&mut conn, &settings, &input)
}

/// v1.1.2: switch the active round's task — closes the current session with
/// its actual focused time and opens a new focus session for the chosen task,
/// atomically.
#[tauri::command]
pub fn switch_timer_task(
    state: State<'_, AppState>,
    input: crate::models::SwitchTimerTaskInput,
) -> Result<crate::models::SwitchTimerTaskResult, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    repository::switch_timer_task(&mut conn, &settings, &input)
}

#[tauri::command]
pub fn complete_timer(state: State<'_, AppState>, input: CompleteTimerInput) -> Result<CompleteTimerResult, CommandError> {
    let mut conn = lock_db(&state)?;
    let settings = repository::get_settings(&conn)?;
    repository::complete_timer(&mut conn, &settings, &input)
}

// ─── Sessions and statistics ─────────────────────────────────────────────────

#[tauri::command]
pub fn finish_timer(
    state: State<'_, AppState>,
    input: FinishTimerInput,
) -> Result<FinishTimerResult, CommandError> {
    let mut conn = lock_db(&state)?;
    repository::finish_timer(&mut conn, &input)
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
