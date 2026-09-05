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
  project: string
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
  project: string
  /** Defaults to the fallback tag when omitted. */
  tagId?: string
}

export interface UpdateTaskInput extends Partial<CreateTaskInput> {
  id: string
  done?: boolean
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
