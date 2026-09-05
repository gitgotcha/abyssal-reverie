export type TaskPriority = 'high' | 'med' | 'low'
export type TimerMode = 'focus' | 'short' | 'long'
export type TimerState = 'idle' | 'running' | 'paused' | 'done'
export type SessionStatus = 'completed' | 'abandoned'
export type TagKind = 'system' | 'custom'

export interface Task {
  id: string
  title: string
  done: boolean
  pomodoroTarget: number
  priority: TaskPriority
  /** Resolved display project name (通用 for standalone tasks). */
  project: string
  /** v1.2: owning project row (null = standalone). */
  projectId: string | null
  /** v1.2: frozen budget in seconds (预计番茄数 × 创建时单次专注分钟 × 60). */
  targetSeconds: number
  /** v1.2: creation | migration | recalc */
  budgetSource: string
  /** v1.2: todo | done | archived */
  status: string
  /** v1.2: ISO date string or null. */
  deadline: string | null
  /** v1.2: free notes. */
  notes: string
  /** Owning primary tag. v1.1: always present on Rust payloads. */
  tagId: string
  sortOrder: number
  createdAt: number
  updatedAt: number
  completedAt: number | null
}

export interface AppSettings {
  focusDurationMinutes: number
  shortBreakMinutes: number
  longBreakMinutes: number
  autoStartBreak: boolean
  soundEnabled: boolean
  notificationEnabled: boolean
  dailyGoal: number
  /** R1-03: pauses the ocean background video to cut CPU/GPU/battery cost. */
  reduceMotion: boolean
  updatedAt: number
}

export interface TimerSnapshot {
  mode: TimerMode
  state: TimerState
  activeSessionId: string | null
  selectedTaskId: string | null
  taskTitleSnapshot: string | null
  projectSnapshot: string | null
  /** Tag frozen when this round started (v1.1). Null while idle. */
  tagId?: string | null
  tagNameSnapshot?: string | null
  durationSeconds: number
  remainingSeconds: number
  startedAt: number | null
  targetEndAt: number | null
  pausedAt: number | null
  revision: number
  updatedAt: number
}

export interface TimerSession {
  id: string
  taskId: string | null
  taskTitleSnapshot: string
  projectSnapshot: string
  /** v1.1 tag snapshot fields — absent on old persisted payloads/tests. */
  tagId?: string | null
  tagNameSnapshot?: string
  mode: TimerMode
  status: SessionStatus
  plannedSeconds: number
  focusedSeconds: number
  startedAt: number
  endedAt: number
  finishReason?: string
  statisticsEligible?: boolean
  qualificationReason?: string
}

// ─── Tags (v1.1) ─────────────────────────────────────────────────────────────

export interface Tag {
  id: string
  name: string
  kind: TagKind
  isFallback: boolean
  sortOrder: number
  createdAt: number
  updatedAt: number
}

export interface CreateTagInput {
  name: string
}

export interface UpdateTagInput {
  id: string
  name?: string
}

export interface ReorderTagInput {
  id: string
  /** -1 moves one slot up (earlier), +1 one slot down (later). */
  direction: number
}

export interface TagDeletePreview {
  tagId: string
  affectedTasks: number
}

export interface DeleteTagResult {
  deletedTagId: string
  fallbackTagId: string
  reassignedTasks: number
  tags: Tag[]
  tasks: Task[]
}

export interface StatisticsDayBoundary {
  date: string
  from: number
  to: number
}

export interface Statistics {
  from: number
  to: number
  focusSessionCount: number
  focusSeconds: number
  dailyGoal: number
  streakDays: number
  bestDay: string | null
  byDay: Array<{ date: string; sessions: number; focusSeconds: number }>
  byProject: Array<{ project: string; sessions: number; focusSeconds: number }>
  byTag: Array<{ project: string; sessions: number; focusSeconds: number }>
  /** v1.2: category distribution from the segment ledger's snapshots. */
  byCategory: Array<{ project: string; sessions: number; focusSeconds: number }>
}

export interface CommandError {
  code: 'VALIDATION_ERROR' | 'NOT_FOUND' | 'CONFLICT' | 'DATABASE_ERROR' | 'INTERNAL_ERROR' | 'DATABASE_TOO_NEW'
  message: string
}

/** v1.1 §10.4 — which sessions a query may return. */
export type SessionScope = 'activity' | 'all'

export interface FinishTimerInput {
  expectedRevision: number
  activeSessionId: string
}

export interface FinishTimerResult {
  timer: TimerSnapshot
  session: TimerSession
  statistics: Statistics
  newlyFinished: boolean
  statisticsEligible: boolean
  qualificationReason: string
}

export interface CreateTaskInput {
  title: string
  pomodoroTarget: number
  priority: TaskPriority
  /** v1.2: owning project id (omit/empty = standalone task). */
  projectId?: string
  /** Legacy free-text project (pre-v1.2 callers). */
  project?: string
  /** v1.2: ISO date string. */
  deadline?: string
  /** v1.2: free notes. */
  notes?: string
  /** Defaults to the fallback tag when omitted. */
  tagId?: string
}

export interface UpdateTaskInput extends Partial<CreateTaskInput> {
  id: string
  done?: boolean
  /** v1.2: budget recalculation target seconds (writes a recalc history row). */
  targetSeconds?: number
  /** v1.2: archive (soft delete) / restore. */
  archived?: boolean
}

// ─── Categories & projects (v1.2) ────────────────────────────────────────────

export interface Category {
  id: string
  profileId: string
  name: string
  status: string
  sortOrder: number
  createdAt: number
  updatedAt: number
}

export interface CreateCategoryInput {
  name: string
}

export interface UpdateCategoryInput {
  id: string
  name?: string
  /** -1 up, +1 down. */
  direction?: number
  archived?: boolean
}

export interface Project {
  id: string
  profileId: string
  categoryId: string
  categoryName: string
  name: string
  description: string
  status: string
  planStartDate: string | null
  dueDate: string | null
  sortOrder: number
  createdAt: number
  updatedAt: number
  taskCount: number
  doneTaskCount: number
}

export interface CreateProjectInput {
  name: string
  categoryId: string
  description?: string
  planStartDate?: string
  dueDate?: string
}

export interface UpdateProjectInput {
  id: string
  name?: string
  categoryId?: string
  description?: string
  /** active | completed | archived */
  status?: string
  planStartDate?: string
  dueDate?: string
}

/** v1.2: real-time task progress. */
export interface TaskProgress {
  taskId: string
  confirmedSeconds: number
  provisionalSeconds: number
  targetSeconds: number
  /** min(1, (confirmed + provisional) / target). */
  progress: number
  /** Seconds beyond the budget. */
  overSeconds: number
}

export interface CompleteTaskInput {
  taskId: string
  expectedRevision: number
  activeSessionId: string
}

export interface CompleteTaskResult {
  task: Task
  timer: TimerSnapshot
  segmentSavedMs: number
  newlyCompleted: boolean
}

export interface TimerRevisionInput {
  expectedRevision: number
}

export interface StartTimerInput extends TimerRevisionInput {
  mode: TimerMode
  selectedTaskId: string | null
}

export interface SwitchTimerModeInput extends TimerRevisionInput {
  mode: TimerMode
}

/** v1.1.2: switch the active round's task. Closes the current session with
 *  its actual focused time and opens a new focus session for the chosen
 *  task, atomically (v1.2 replaces the clock reset with segment splitting). */
export interface SwitchTimerTaskInput extends TimerRevisionInput {
  activeSessionId: string
  newTaskId: string
}

export interface SwitchTimerTaskResult {
  timer: TimerSnapshot
  /** The session closed by this call; null when nothing was closed. */
  closedSession: TimerSession | null
  newlyClosed: boolean
}

export interface CompleteTimerInput extends TimerRevisionInput {
  activeSessionId: string
  recovery?: boolean
}

export interface CompleteTimerResult {
  timer: TimerSnapshot
  session: TimerSession
  statistics: Statistics
  newlyCompleted: boolean
}

export interface SessionQuery {
  limit?: number
  from?: number
  to?: number
  /** Defaults to 'activity' — hidden records need an explicit 'all'. */
  scope?: SessionScope
}

export interface StatisticsQuery {
  from: number
  to: number
  days: StatisticsDayBoundary[]
}

export interface BootstrapPayload {
  tasks: Task[]
  tags: Tag[]
  settings: AppSettings
  timer: TimerSnapshot
  sessions: TimerSession[]
  statistics: Statistics
}

export interface SaveSettingsResult {
  settings: AppSettings
  timer: TimerSnapshot
}

// ─── Data export & backup (Item 3) ─────────────────────────────────────────

/** Row counts returned by `previewImport` before a destructive import. */
export interface ImportPreview {
  schemaVersion: number
  tags: number
  tasks: number
  sessions: number
}

/** Result of a successful export (bytes written to disk). */
export interface ExportSummary {
  path: string
  bytes: number
  tasks: number
  sessions: number
}

/** Result of a successful import (rows replaced in the database). */
export interface ImportSummary {
  path: string
  tasks: number
  sessions: number
}
