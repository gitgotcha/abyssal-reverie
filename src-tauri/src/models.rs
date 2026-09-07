use serde::{Deserialize, Serialize};

// ─── Enums ───────────────────────────────────────────────────────────────────
// Each enum round-trips through SQLite as TEXT and through IPC as lowercase.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    High,
    Med,
    Low,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Med
    }
}

impl TaskPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskPriority::High => "high",
            TaskPriority::Med => "med",
            TaskPriority::Low => "low",
        }
    }

    pub fn parse_str(value: &str) -> Option<Self> {
        match value {
            "high" => Some(TaskPriority::High),
            "med" => Some(TaskPriority::Med),
            "low" => Some(TaskPriority::Low),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimerMode {
    Focus,
    Short,
    Long,
}

impl TimerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TimerMode::Focus => "focus",
            TimerMode::Short => "short",
            TimerMode::Long => "long",
        }
    }

    pub fn parse_str(value: &str) -> Option<Self> {
        match value {
            "focus" => Some(TimerMode::Focus),
            "short" => Some(TimerMode::Short),
            "long" => Some(TimerMode::Long),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimerState {
    Idle,
    Running,
    Paused,
    Done,
}

impl TimerState {
    pub fn as_str(self) -> &'static str {
        match self {
            TimerState::Idle => "idle",
            TimerState::Running => "running",
            TimerState::Paused => "paused",
            TimerState::Done => "done",
        }
    }

    pub fn parse_str(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(TimerState::Idle),
            "running" => Some(TimerState::Running),
            "paused" => Some(TimerState::Paused),
            "done" => Some(TimerState::Done),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Completed,
    Abandoned,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Completed => "completed",
            SessionStatus::Abandoned => "abandoned",
        }
    }

    pub fn parse_str(value: &str) -> Option<Self> {
        match value {
            "completed" => Some(SessionStatus::Completed),
            "abandoned" => Some(SessionStatus::Abandoned),
            _ => None,
        }
    }
}

// ─── Tags (v1.1) ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagKind {
    System,
    Custom,
}

impl TagKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TagKind::System => "system",
            TagKind::Custom => "custom",
        }
    }

    pub fn parse_str(value: &str) -> Option<Self> {
        match value {
            "system" => Some(TagKind::System),
            "custom" => Some(TagKind::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub kind: TagKind,
    pub is_fallback: bool,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTagInput {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTagInput {
    pub id: String,
    /// Rename when present; empty/whitespace or >20 chars rejected.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderTagInput {
    pub id: String,
    /// -1 moves the tag one slot up (earlier), +1 one slot down (later).
    pub direction: i64,
}

/// Shown to the user before a tag deletion is confirmed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagDeletePreview {
    pub tag_id: String,
    pub affected_tasks: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTagResult {
    pub deleted_tag_id: String,
    pub fallback_tag_id: String,
    pub reassigned_tasks: i64,
    pub tags: Vec<Tag>,
    pub tasks: Vec<Task>,
}

// ─── Persisted records ───────────────────────────────────────────────────────

/// Stable id of the permanent fallback tag ("其他"), seeded by the v3 schema
/// migration. Used to backfill tasks/sessions written before v1.1.
pub const FALLBACK_TAG_ID: &str = "system-other";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub done: bool,
    pub pomodoro_target: i64,
    /// v5: optional — NULL means the user never picked a priority.
    #[serde(default)]
    pub priority: Option<TaskPriority>,
    /// Resolved display name of the owning project (or `通用` when the task
    /// is standalone). Kept in sync with `project_id`; used for session
    /// snapshots and preserved in v2 backups for compatibility.
    pub project: String,
    /// v1.2: owning project row (NULL = standalone task).
    #[serde(default)]
    pub project_id: Option<String>,
    /// v1.2: frozen budget `预计番茄数 × 创建时单次专注分钟 × 60`.
    #[serde(default)]
    pub target_seconds: i64,
    /// v1.2: how the budget was set (creation/migration/recalc).
    #[serde(default)]
    pub budget_source: String,
    /// v1.2: todo | done | archived (kept in sync with `done`).
    #[serde(default = "default_task_status")]
    pub status: String,
    /// v1.2: user-facing deadline (ISO date string).
    #[serde(default)]
    pub deadline: Option<String>,
    /// v1.2: free notes.
    #[serde(default)]
    pub notes: String,
    /// v5: optional owning tag — NULL means title-only creation or an
    /// explicit clear. Historical rows keep their pre-v5 tags verbatim.
    #[serde(default)]
    pub tag_id: Option<String>,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    /// v1.4 (batch 1): optimistic-concurrency guard for relationship changes.
    /// Incremented ONLY by `apply_relationship_patch`, never by other writes.
    #[serde(default)]
    pub relationship_revision: i64,
}

fn default_task_status() -> String {
    "todo".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub focus_duration_minutes: i64,
    pub short_break_minutes: i64,
    pub long_break_minutes: i64,
    pub auto_start_break: bool,
    pub sound_enabled: bool,
    pub notification_enabled: bool,
    pub daily_goal: i64,
    /// R1-03 (Item 4 Round 4): in-app "reduce motion" switch — pauses the ocean
    /// background video to cut CPU/GPU/battery cost. `serde(default)` keeps v1
    /// backups (which lack the field) importable.
    #[serde(default)]
    pub reduce_motion: bool,
    pub updated_at: i64,
}

impl Default for AppSettings {
    /// Mirrors `DEFAULT_SETTINGS` in `src/domain/defaults.ts`.
    fn default() -> Self {
        Self {
            focus_duration_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            auto_start_break: false,
            sound_enabled: true,
            notification_enabled: true,
            daily_goal: 8,
            reduce_motion: false,
            updated_at: 0,
        }
    }
}

impl AppSettings {
    pub fn duration_seconds_for_mode(&self, mode: TimerMode) -> i64 {
        let minutes = match mode {
            TimerMode::Focus => self.focus_duration_minutes,
            TimerMode::Short => self.short_break_minutes,
            TimerMode::Long => self.long_break_minutes,
        };
        minutes * 60
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerSnapshot {
    pub mode: TimerMode,
    pub state: TimerState,
    pub active_session_id: Option<String>,
    pub selected_task_id: Option<String>,
    pub task_title_snapshot: Option<String>,
    pub project_snapshot: Option<String>,
    /// Tag frozen when this round started (v1.1). Cleared when idle.
    pub tag_id: Option<String>,
    /// Tag NAME frozen when this round started — survives tag renames/deletes.
    pub tag_name_snapshot: Option<String>,
    pub duration_seconds: i64,
    pub remaining_seconds: i64,
    pub started_at: Option<i64>,
    pub target_end_at: Option<i64>,
    pub paused_at: Option<i64>,
    pub revision: i64,
    pub updated_at: i64,
}

impl TimerSnapshot {
    /// Mirrors `idleTimerForMode()` in `src/domain/defaults.ts`.
    pub fn idle(mode: TimerMode, duration_seconds: i64) -> Self {
        Self {
            mode,
            state: TimerState::Idle,
            active_session_id: None,
            selected_task_id: None,
            task_title_snapshot: None,
            project_snapshot: None,
            tag_id: None,
            tag_name_snapshot: None,
            duration_seconds,
            remaining_seconds: duration_seconds,
            started_at: None,
            target_end_at: None,
            paused_at: None,
            revision: 0,
            updated_at: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerSession {
    pub id: String,
    pub task_id: Option<String>,
    pub task_title_snapshot: String,
    pub project_snapshot: String,
    /// Owning tag at session time (stable id). `None` only for v1 backups —
    /// the importer maps it to the fallback tag.
    #[serde(default)]
    pub tag_id: Option<String>,
    /// Tag NAME frozen at session time — historical label, never re-tagged.
    /// `None` only for v1 backups (imported as the current fallback name).
    #[serde(default)]
    pub tag_name_snapshot: Option<String>,
    pub mode: TimerMode,
    pub status: SessionStatus,
    pub planned_seconds: i64,
    pub focused_seconds: i64,
    pub started_at: i64,
    pub ended_at: i64,
    /// Why the session ended. `None` only for v1 backups (imported as
    /// "legacy" — the original reason is unrecoverable).
    #[serde(default)]
    pub finish_reason: Option<String>,
    /// Whether the session counts toward focus statistics. `None` only for
    /// v1 backups (import backfills per the v1.1 qualification rules).
    #[serde(default)]
    pub statistics_eligible: Option<bool>,
    /// Why the session is (in)eligible. `None` only for v1 backups.
    #[serde(default)]
    pub qualification_reason: Option<String>,
}

// ─── Statistics payloads ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsDayBoundary {
    pub date: String,
    pub from: i64,
    pub to: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub date: String,
    pub sessions: i64,
    pub focus_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStat {
    pub project: String,
    pub sessions: i64,
    pub focus_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Statistics {
    pub from: i64,
    pub to: i64,
    pub focus_session_count: i64,
    pub focus_seconds: i64,
    pub daily_goal: i64,
    pub streak_days: i64,
    pub best_day: Option<String>,
    pub by_day: Vec<DayStat>,
    pub by_project: Vec<ProjectStat>,
    /// v1.1: tag distribution, aggregated by tag_name_snapshot (renaming a
    /// tag must not rewrite historical statistics).
    pub by_tag: Vec<ProjectStat>,
    /// v1.2: category distribution by the segments' category snapshot
    /// (moving a project between categories never rewrites history).
    pub by_category: Vec<ProjectStat>,
}

// ─── Command payloads ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub title: String,
    pub pomodoro_target: i64,
    /// v5: `None` (or JSON `null`) = 未选择 → stored as NULL. A JSON string
    /// keeps its explicit-selection meaning. Legacy callers that always sent
    /// the fallback id keep working — they now get a task explicitly tagged
    /// 「其他」 instead of one silently retagged.
    #[serde(default)]
    pub priority: Option<TaskPriority>,
    /// v1.2: owning project id (None/empty = standalone task). The legacy
    /// free-text `project` field is ignored when a project_id is present.
    #[serde(default)]
    pub project_id: Option<String>,
    /// Legacy free-text project field (pre-v1.2 callers).
    #[serde(default = "default_task_project")]
    pub project: String,
    /// v1.2: deadline (ISO date string).
    #[serde(default)]
    pub deadline: Option<String>,
    /// v1.2: free notes.
    #[serde(default)]
    pub notes: Option<String>,
    /// v5: `None`/empty = 未选择 → stored as NULL (title-only creation no
    /// longer lands on the fallback tag); a non-empty id is used verbatim.
    #[serde(default)]
    pub tag_id: Option<String>,
}

/// v1.4 (batch 1): explicit optional-metadata patch — distinguishes "not
/// mentioned" (`keep`) from "clear to NULL" (`clear`). Mirrors the frontend
/// `NullablePatch<T>` discriminated union; never conflate the two with a bare
/// `Option<T>`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum NullablePatch<T> {
    Keep,
    Clear,
    Set { value: T },
}

impl<T> NullablePatch<T> {
    /// Serde default so an absent field means "keep current value".
    pub fn keep() -> Self {
        NullablePatch::Keep
    }
}

/// v1.4 (batch 1): atomic task-relationship change guarded by an optimistic
/// revision. The revision increments ONLY here — normal task edits never
/// touch it, so a stale `expectedRevision` always means a genuine concurrent
/// relationship change. Mirrors the frontend `RelationshipPatch`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipPatch {
    pub task_id: String,
    pub expected_revision: i64,
    #[serde(default = "NullablePatch::keep")]
    pub project: NullablePatch<String>,
    #[serde(default = "NullablePatch::keep")]
    pub tag: NullablePatch<String>,
}

fn default_task_project() -> String {
    "通用".to_owned()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskInput {
    pub id: String,
    pub title: Option<String>,
    pub pomodoro_target: Option<i64>,
    pub priority: Option<TaskPriority>,
    /// v1.2: move to a project (None = unchanged; empty string detaches).
    #[serde(default)]
    pub project_id: Option<String>,
    pub project: Option<String>,
    /// v1.2: budget recalculation target seconds (writes a `recalc` history
    /// row; invested time is never touched).
    #[serde(default)]
    pub target_seconds: Option<i64>,
    /// v1.2: deadline (empty string clears).
    #[serde(default)]
    pub deadline: Option<String>,
    /// v1.2: free notes (empty string clears).
    #[serde(default)]
    pub notes: Option<String>,
    /// v1.2: archive/restore (soft delete).
    #[serde(default)]
    pub archived: Option<bool>,
    pub done: Option<bool>,
}

// ─── Categories & projects (v1.2) ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub profile_id: String,
    pub name: String,
    pub status: String,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCategoryInput {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCategoryInput {
    pub id: String,
    /// Rename (1–20 chars, unique within the profile).
    pub name: Option<String>,
    /// -1 moves one slot up, +1 one slot down.
    pub direction: Option<i64>,
    /// Archive (soft) / restore.
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub profile_id: String,
    pub category_id: String,
    /// Display name of the owning category (resolved for the UI).
    pub category_name: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub plan_start_date: Option<String>,
    pub due_date: Option<String>,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// Resolved counters for list/detail views.
    pub task_count: i64,
    pub done_task_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub category_id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub plan_start_date: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectInput {
    pub id: String,
    pub name: Option<String>,
    /// Move to another category (same profile only).
    pub category_id: Option<String>,
    pub description: Option<String>,
    /// active | completed | archived
    pub status: Option<String>,
    pub plan_start_date: Option<String>,
    pub due_date: Option<String>,
}

/// R06: read-only preview for a pending v3 → v4 semantic migration.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPreview {
    pub schema_version: u32,
    pub task_count: i64,
    pub session_count: i64,
    /// Distinct trimmed legacy project names (excluding 通用/blank) that will
    /// become real projects.
    pub projects_to_create: Vec<String>,
    /// Tasks in the legacy default project 通用 — their mapping is a choice.
    pub general_task_count: i64,
    /// The user's current focus duration — the SUGGESTED budget basis, never
    /// an authority the user cannot override.
    pub suggested_focus_minutes: i64,
}

/// R06: the user's confirmed migration decisions.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationParams {
    /// Minutes per pomodoro used to estimate legacy budgets (marked as
    /// 'migration' source — never presented as exact history).
    pub budget_focus_minutes: i64,
    /// "standalone" (default): 通用 tasks become independent.
    /// "project": 通用 becomes one real project in the fallback category.
    pub general_mapping: String,
}

/// v1.2: real-time task progress (confirmed ledger + provisional segment).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub task_id: String,
    pub confirmed_seconds: i64,
    pub provisional_seconds: i64,
    pub target_seconds: i64,
    /// min(1.0, (confirmed + provisional) / target) — 0 when target is 0.
    pub progress: f64,
    /// Seconds beyond the budget (only meaningful once progress reaches 1).
    pub over_seconds: i64,
}

/// v1.2 B4: complete a task now — freeze + confirm the current segment, mark
/// the task done and unbind it, keep the main clock paused with its remaining
/// time. One transaction; retries are idempotent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteTaskInput {
    pub task_id: String,
    pub expected_revision: i64,
    pub active_session_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteTaskResult {
    pub task: Task,
    pub timer: TimerSnapshot,
    /// Effective ms saved into the (now confirmed) segment, if a segment was
    /// open for this task in the active session.
    pub segment_saved_ms: i64,
    pub newly_completed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPayload {
    pub tasks: Vec<Task>,
    pub tags: Vec<Tag>,
    pub settings: AppSettings,
    pub timer: TimerSnapshot,
    pub sessions: Vec<TimerSession>,
    pub statistics: Statistics,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingsResult {
    pub settings: AppSettings,
    pub timer: TimerSnapshot,
}

// ─── Timer command payloads ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerRevisionInput {
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTimerInput {
    pub expected_revision: i64,
    pub mode: TimerMode,
    pub selected_task_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchTimerModeInput {
    pub expected_revision: i64,
    pub mode: TimerMode,
}

/// `switch_timer_task` input (v1.1.2): switch the round's task mid-session.
/// v1.1.2 semantics — close the current session (actual focused time, same
/// qualification rules as a manual finish) and open a new focus session for
/// the chosen task, in one transaction. v1.2's segment ledger replaces this
/// with same-clock splitting.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchTimerTaskInput {
    pub expected_revision: i64,
    pub active_session_id: String,
    pub new_task_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchTimerTaskResult {
    pub timer: TimerSnapshot,
    /// The session closed by this call; `None` when nothing was closed
    /// (idempotent replay or switch to the already-current task).
    pub closed_session: Option<TimerSession>,
    pub newly_closed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteTimerInput {
    pub expected_revision: i64,
    pub active_session_id: String,
    pub recovery: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteTimerResult {
    pub timer: TimerSnapshot,
    pub session: TimerSession,
    pub statistics: Statistics,
    pub newly_completed: bool,
}

/// Which sessions a query may return (v1.1 spec §10.4).
///
/// - `activity`: only statistics-eligible focus sessions — the activity bar
///   and every user-visible page. Reset/abandoned/too_short/break records
///   never leak through.
/// - `all`: everything, for exports, backups and tests only. Defaults to
///   `activity` so legacy callers cannot accidentally leak hidden records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionScope {
    Activity,
    All,
}

impl Default for SessionScope {
    fn default() -> Self {
        SessionScope::Activity
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionQuery {
    pub limit: Option<i64>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    #[serde(default)]
    pub scope: Option<SessionScope>,
}

/// `finish_timer` input (v1.1 §8.5): user clicks "结束".
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishTimerInput {
    pub expected_revision: i64,
    pub active_session_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishTimerResult {
    pub timer: TimerSnapshot,
    pub session: TimerSession,
    pub statistics: Statistics,
    /// False when the session already existed (idempotent replay).
    pub newly_finished: bool,
    pub statistics_eligible: bool,
    pub qualification_reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsQuery {
    pub from: i64,
    pub to: i64,
    pub days: Vec<StatisticsDayBoundary>,
}

// ─── Data export & backup (Item 3) ─────────────────────────────────────────

/// Full backup bundle. JSON-serialized for restore; never CSV (lossless).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBundle {
    pub app: String,
    pub schema_version: u32,
    pub exported_at: i64,
    pub settings: AppSettings,
    pub tags: Vec<Tag>,
    pub tasks: Vec<Task>,
    pub sessions: Vec<TimerSession>,
}

/// Row counts shown to the user before a destructive import is confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub schema_version: u32,
    pub tags: i64,
    pub tasks: i64,
    pub sessions: i64,
}

// ─── Backup parsing DTOs (v1.1 review: version-header-first dispatch) ────────

/// Minimal header read before the payload is deserialized: the version decides
/// which DTO parses the file, so a v1 backup is never fed to the v2 struct.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupHeader {
    pub app: String,
    pub schema_version: u32,
}

/// v1.0.0 backup shape: no tags, no tag ids, no qualification fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBundleV1 {
    pub app: String,
    pub schema_version: u32,
    #[serde(default)]
    pub exported_at: i64,
    pub settings: AppSettings,
    pub tasks: Vec<TaskV1>,
    pub sessions: Vec<SessionV1>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskV1 {
    pub id: String,
    pub title: String,
    pub done: bool,
    pub pomodoro_target: i64,
    pub priority: TaskPriority,
    pub project: String,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionV1 {
    pub id: String,
    pub task_id: Option<String>,
    pub task_title_snapshot: String,
    pub project_snapshot: String,
    pub mode: TimerMode,
    pub status: SessionStatus,
    pub planned_seconds: i64,
    pub focused_seconds: i64,
    pub started_at: i64,
    pub ended_at: i64,
}

/// Result of a successful export (bytes written to disk).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub path: String,
    pub bytes: u64,
    pub tasks: i64,
    pub sessions: i64,
}

/// Result of a successful import (rows replaced in the database).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub path: String,
    pub tasks: i64,
    pub sessions: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_serialize_to_the_frontend_strings() {
        assert_eq!(serde_json::to_value(TaskPriority::Med).unwrap(), "med");
        assert_eq!(serde_json::to_value(TaskPriority::High).unwrap(), "high");
        assert_eq!(serde_json::to_value(TimerMode::Short).unwrap(), "short");
        assert_eq!(serde_json::to_value(TimerMode::Long).unwrap(), "long");
        assert_eq!(serde_json::to_value(TimerState::Paused).unwrap(), "paused");
        assert_eq!(serde_json::to_value(SessionStatus::Abandoned).unwrap(), "abandoned");
    }

    #[test]
    fn enums_round_trip_through_their_text_representation() {
        for (text, priority) in [("high", TaskPriority::High), ("med", TaskPriority::Med), ("low", TaskPriority::Low)] {
            assert_eq!(priority.as_str(), text);
            assert_eq!(TaskPriority::parse_str(text), Some(priority));
        }
        assert_eq!(TimerMode::parse_str("focus"), Some(TimerMode::Focus));
        assert_eq!(TimerState::parse_str("done"), Some(TimerState::Done));
        assert_eq!(SessionStatus::parse_str("nope"), None);
    }

    #[test]
    fn timer_snapshot_uses_camel_case_keys() {
        let snapshot = TimerSnapshot::idle(TimerMode::Focus, 1500);
        let json = serde_json::to_value(&snapshot).expect("serializable");

        assert_eq!(json["mode"], "focus");
        assert_eq!(json["state"], "idle");
        assert_eq!(json["durationSeconds"], 1500);
        assert_eq!(json["remainingSeconds"], 1500);
        assert_eq!(json["revision"], 0);
        assert!(json.get("activeSessionId").is_some(), "expected activeSessionId key");
        assert!(json.get("targetEndAt").is_some(), "expected targetEndAt key");
        assert!(json["startedAt"].is_null());
    }

    #[test]
    fn default_settings_match_the_frontend_defaults() {
        let settings = AppSettings::default();

        assert_eq!(settings.focus_duration_minutes, 25);
        assert_eq!(settings.short_break_minutes, 5);
        assert_eq!(settings.long_break_minutes, 15);
        assert!(!settings.auto_start_break);
        assert!(settings.sound_enabled);
        assert!(settings.notification_enabled);
        assert_eq!(settings.daily_goal, 8);
        assert_eq!(settings.duration_seconds_for_mode(TimerMode::Focus), 1500);
        assert_eq!(settings.duration_seconds_for_mode(TimerMode::Short), 300);
        assert_eq!(settings.duration_seconds_for_mode(TimerMode::Long), 900);
    }

    #[test]
    fn task_deserializes_from_the_frontend_payload() {
        let json = serde_json::json!({
            "id": "task-1",
            "title": "Write the spec",
            "done": false,
            "pomodoroTarget": 4,
            "priority": "high",
            "project": "Abyssal Reverie",
            "sortOrder": 2,
            "createdAt": 1,
            "updatedAt": 2,
            "completedAt": null,
        });

        let task: Task = serde_json::from_value(json).expect("deserializable");

        assert_eq!(task.pomodoro_target, 4);
        assert_eq!(task.priority, Some(TaskPriority::High));
        assert_eq!(task.tag_id, None, "old payload without tagId means 未选择");
        assert_eq!(task.relationship_revision, 0, "absent revision defaults to 0");
        assert_eq!(task.sort_order, 2);
        assert_eq!(task.completed_at, None);
    }
}
