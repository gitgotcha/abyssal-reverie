import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import type { AppGateway } from './appGateway'
import type {
  AppSettings,
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

export class TauriAppGateway implements AppGateway {
  bootstrap() { return invoke<BootstrapPayload>('bootstrap_app') }
  startTimer(input: StartTimerInput) { return invoke<TimerSnapshot>('start_timer', { input }) }
  pauseTimer(input: TimerRevisionInput) { return invoke<TimerSnapshot>('pause_timer', { input }) }
  resumeTimer(input: TimerRevisionInput) { return invoke<TimerSnapshot>('resume_timer', { input }) }
  resetTimer(input: TimerRevisionInput) { return invoke<TimerSnapshot>('reset_timer', { input }) }
  switchTimerMode(input: SwitchTimerModeInput) { return invoke<TimerSnapshot>('switch_timer_mode', { input }) }
  switchTimerTask(input: SwitchTimerTaskInput) { return invoke<SwitchTimerTaskResult>('switch_timer_task', { input }) }
  completeTimer(input: CompleteTimerInput) { return invoke<CompleteTimerResult>('complete_timer', { input }) }
  finishTimer(input: FinishTimerInput) { return invoke<FinishTimerResult>('finish_timer', { input }) }
  createTask(input: CreateTaskInput) { return invoke<Task>('create_task', { input }) }
  updateTask(input: UpdateTaskInput) { return invoke<Task>('update_task', { input }) }
  deleteTask(id: string) { return invoke<void>('delete_task', { id }) }

  // ─── Categories, projects & task ledger (v1.2) ────────────────────────────
  listCategories() { return invoke<Category[]>('list_categories') }
  createCategory(input: CreateCategoryInput) { return invoke<Category>('create_category', { input }) }
  updateCategory(input: UpdateCategoryInput) { return invoke<Category>('update_category', { input }) }
  listProjects() { return invoke<Project[]>('list_projects') }
  createProject(input: CreateProjectInput) { return invoke<Project>('create_project', { input }) }
  updateProject(input: UpdateProjectInput) { return invoke<Project>('update_project', { input }) }
  getTaskProgress(taskId: string) { return invoke<TaskProgress>('get_task_progress', { taskId }) }
  getAllTaskProgress() { return invoke<TaskProgress[]>('get_all_task_progress') }
  completeTaskNow(input: CompleteTaskInput) { return invoke<CompleteTaskResult>('complete_task_now', { input }) }

  // ─── Tags (v1.1) ──────────────────────────────────────────────────────────
  listTags() { return invoke<Tag[]>('list_tags') }
  createTag(input: CreateTagInput) { return invoke<Tag>('create_tag', { input }) }
  updateTag(input: UpdateTagInput) { return invoke<Tag>('update_tag', { input }) }
  reorderTag(input: ReorderTagInput) { return invoke<Tag[]>('reorder_tag', { input }) }
  previewDeleteTag(id: string) { return invoke<TagDeletePreview>('preview_delete_tag', { id }) }
  deleteTag(id: string) { return invoke<DeleteTagResult>('delete_tag', { id }) }

  saveSettings(input: AppSettings) { return invoke<SaveSettingsResult>('save_settings', { input }) }
  listSessions(query: SessionQuery) { return invoke<TimerSession[]>('list_sessions', { query }) }
  getStatistics(query: StatisticsQuery) { return invoke<Statistics>('get_statistics', { query }) }

  setTrayIndicator(input: TrayIndicator) { return invoke<void>('set_tray_indicator', { input }) }

  // `listen` is async (returns a promise for the unlisten handle); the React
  // effect that registers these expects a synchronous teardown, so we hand it
  // a closure that resolves the handle lazily and unlistens on cleanup.
  subscribeTimerExpired(cb: (payload: TimerExpiredPayload) => void): () => void {
    let unlisten: UnlistenFn | undefined
    let stopped = false
    void listen<TimerExpiredPayload>('timer-expired', e => cb(e.payload))
      .then(fn => { unlisten = stopped ? void fn() : fn })
      .catch(() => undefined)
    return () => { stopped = true; unlisten?.() }
  }

  subscribeTrayAction(cb: (action: TrayAction) => void): () => void {
    let unlisten: UnlistenFn | undefined
    let stopped = false
    void listen<TrayAction>('tray-action', e => cb(e.payload))
      .then(fn => { unlisten = stopped ? void fn() : fn })
      .catch(() => undefined)
    return () => { stopped = true; unlisten?.() }
  }

  subscribeGlobalShortcutConflict(cb: (shortcut: string) => void): () => void {
    let unlisten: UnlistenFn | undefined
    let stopped = false
    void listen<{ shortcut: string }>('global-shortcut-conflict', e => cb(e.payload.shortcut))
      .then(fn => { unlisten = stopped ? void fn() : fn })
      .catch(() => undefined)
    return () => { stopped = true; unlisten?.() }
  }

  getAutostart() { return invoke<boolean>('get_autostart') }
  setAutostart(enabled: boolean) { return invoke<boolean>('set_autostart', { enabled }) }

  // ─── Data export & backup (Item 3) ───────────────────────────────────────

  pickExportPath(suggestedName: string) {
    return invoke<string | null>('pick_export_path', { suggestedName })
  }
  pickImportPath() { return invoke<string | null>('pick_import_path') }
  exportBackup(path: string) { return invoke<ExportSummary>('export_backup_to', { path }) }
  exportSessionsCsv(path: string) { return invoke<ExportSummary>('export_sessions_csv_to', { path }) }
  previewImport(path: string) { return invoke<ImportPreview>('preview_import_from', { path }) }
  importBackup(path: string) { return invoke<ImportSummary>('import_backup_from', { path }) }
}
