import type {
  AppSettings,
  MiniWindowPrefs,
  Category,
  CreateCategoryInput,
  UpdateCategoryInput,
  Project,
  CreateProjectInput,
  UpdateProjectInput,
  TaskProgress,
  CompleteTaskInput,
  CompleteTaskResult,
  BootstrapPayload,
  CompleteTimerInput,
  CompleteTimerResult,
  CreateTagInput,
  CreateTaskInput,
  DeleteTagResult,
  ExportSummary,
  FinishTimerInput,
  FinishTimerResult,
  ImportPreview,
  ImportSummary,
  ReorderTagInput,
  SaveSettingsResult,
  SessionQuery,
  StartTimerInput,
  Statistics,
  StatisticsQuery,
  SwitchTimerModeInput,
  SwitchTimerTaskInput,
  SwitchTimerTaskResult,
  Tag,
  TagDeletePreview,
  Task,
  TimerRevisionInput,
  TimerSession,
  TimerSnapshot,
  UpdateTagInput,
  UpdateTaskInput,
} from '../domain/models'
import type { TrayAction, TrayIndicator, TimerExpiredPayload } from '../domain/tray'

export interface AppGateway {
  bootstrap(): Promise<BootstrapPayload>
  startTimer(input: StartTimerInput): Promise<TimerSnapshot>
  pauseTimer(input: TimerRevisionInput): Promise<TimerSnapshot>
  resumeTimer(input: TimerRevisionInput): Promise<TimerSnapshot>
  resetTimer(input: TimerRevisionInput): Promise<TimerSnapshot>
  switchTimerMode(input: SwitchTimerModeInput): Promise<TimerSnapshot>
  /** v1.1.2: atomic "close current session + open a new round for the task". */
  switchTimerTask(input: SwitchTimerTaskInput): Promise<SwitchTimerTaskResult>
  completeTimer(input: CompleteTimerInput): Promise<CompleteTimerResult>
  /** v1.1: manual "结束" — records actual focused time; timer returns to idle. */
  finishTimer(input: FinishTimerInput): Promise<FinishTimerResult>
  createTask(input: CreateTaskInput): Promise<Task>
  updateTask(input: UpdateTaskInput): Promise<Task>
  deleteTask(id: string): Promise<void>
  // ─── Categories, projects & task ledger (v1.2) ────────────────────────────
  listCategories(): Promise<Category[]>
  createCategory(input: CreateCategoryInput): Promise<Category>
  updateCategory(input: UpdateCategoryInput): Promise<Category>
  listProjects(): Promise<Project[]>
  createProject(input: CreateProjectInput): Promise<Project>
  updateProject(input: UpdateProjectInput): Promise<Project>
  /** Real-time progress for one task (confirmed + provisional segments). */
  getTaskProgress(taskId: string): Promise<TaskProgress>
  /** Batch variant — one call for list views. */
  getAllTaskProgress(): Promise<TaskProgress[]>
  /** v1.2 B4: atomic "pause + save segment + mark done + unbind". */
  completeTaskNow(input: CompleteTaskInput): Promise<CompleteTaskResult>
  // ─── Tags (v1.1) ──────────────────────────────────────────────────────────
  listTags(): Promise<Tag[]>
  createTag(input: CreateTagInput): Promise<Tag>
  updateTag(input: UpdateTagInput): Promise<Tag>
  reorderTag(input: ReorderTagInput): Promise<Tag[]>
  previewDeleteTag(id: string): Promise<TagDeletePreview>
  deleteTag(id: string): Promise<DeleteTagResult>
  saveSettings(input: AppSettings): Promise<SaveSettingsResult>
  listSessions(query: SessionQuery): Promise<TimerSession[]>
  getStatistics(query: StatisticsQuery): Promise<Statistics>
  /** Paints the live tooltip + menu labels into the system tray. */
  setTrayIndicator(input: TrayIndicator): Promise<void>
  /** Subscribes to the Rust completion backstop. Returns an unsubscribe. */
  subscribeTimerExpired(cb: (payload: TimerExpiredPayload) => void): () => void
  /** v1.3 A1: authoritative settlement result from the background ticker. */
  subscribeTimerSettled(cb: (payload: { timer: TimerSnapshot; session: TimerSession; newlyCompleted: boolean }) => void): () => void
  /** v1.3 A4: snapshot broadcast to every window after a timer change. */
  subscribeTimerChanged(cb: (snapshot: TimerSnapshot) => void): () => void
  // ─── Mini window (v1.3 B/C) ─────────────────────────────────────────────────
  /** Shows + focuses the main window (小窗「返回主窗」). */
  showMainWindow(): Promise<void>
  /** Toggles the mini window's visibility (小窗「关闭」= hide; 计时继续). */
  toggleMiniWindow(): Promise<void>
  loadMiniPrefs(): Promise<MiniWindowPrefs>
  saveMiniPrefs(prefs: MiniWindowPrefs): Promise<void>
  /** v1.3 C4: undo a task completion; real recorded segments stay. */
  undoCompleteTask(taskId: string): Promise<Task>
  /** Subscribes to tray menu actions (pause/resume, reset). Returns an unsubscribe. */
  subscribeTrayAction(cb: (action: TrayAction) => void): () => void
  /** Subscribes to the global-shortcut conflict warning (hotkey taken by another
   *  app). The callback receives the conflicting accelerator string. */
  subscribeGlobalShortcutConflict(cb: (shortcut: string) => void): () => void
  /** Whether the app launches at Windows login (autostart plugin state). */
  getAutostart(): Promise<boolean>
  /** Enables/disables launch-at-login; resolves to the resulting state. */
  setAutostart(enabled: boolean): Promise<boolean>
  /** Opens a native Save-As dialog; resolves to the chosen path or null. */
  pickExportPath(suggestedName: string): Promise<string | null>
  /** Opens a native Open dialog (JSON only); resolves to the chosen path or null. */
  pickImportPath(): Promise<string | null>
  /** Writes the full backup bundle as JSON to `path`. */
  exportBackup(path: string): Promise<ExportSummary>
  /** Writes all sessions as a CSV spreadsheet to `path`. */
  exportSessionsCsv(path: string): Promise<ExportSummary>
  /** Reads + validates a backup file without mutating the DB. */
  previewImport(path: string): Promise<ImportPreview>
  /** Replaces tasks/sessions/settings from the backup at `path`. */
  importBackup(path: string): Promise<ImportSummary>
}
