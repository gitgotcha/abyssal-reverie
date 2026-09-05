import { DEFAULT_SETTINGS, durationSecondsForMode, idleTimerForMode } from '../domain/defaults'
import type { AppGateway } from '../services/appGateway'
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
  StatisticsDayBoundary,
  StatisticsQuery,
  SwitchTimerModeInput,
  SwitchTimerTaskInput,
  SwitchTimerTaskResult,
  Tag,
  TagDeletePreview,
  Task,
  TimerMode,
  TimerRevisionInput,
  TimerSession,
  TimerSnapshot,
  UpdateTagInput,
  UpdateTaskInput,
} from '../domain/models'
import type { TrayAction, TrayIndicator, TimerExpiredPayload } from '../domain/tray'

export interface InjectedError {
  code: string
  message: string
}

let sequence = 0
function nextId(prefix: string): string {
  sequence += 1
  return `${prefix}-${Date.now().toString(36)}-${sequence}`
}

/**
 * In-memory `AppGateway` for unit/component tests.
 *
 * Mirrors the action-style timer state machine closely enough to exercise the
 * React layer without a Tauri runtime or SQLite. Use `injectFailure` to make
 * the next call reject with a specific `CommandError`-shaped error, enabling
 * deterministic conflict / not-found / database-error paths.
 */
export class FakeAppGateway implements AppGateway {
  private tasks: Task[] = []
  private settings: AppSettings = { ...DEFAULT_SETTINGS, updatedAt: Date.now() }
  private timer: TimerSnapshot = idleTimerForMode('focus', this.settings)
  private sessions: TimerSession[] = []
  private tags: Tag[] = [
    { id: 'system-study', name: '学习', kind: 'system', isFallback: false, sortOrder: 0, createdAt: 0, updatedAt: 0 },
    { id: 'system-work', name: '工作', kind: 'system', isFallback: false, sortOrder: 1, createdAt: 0, updatedAt: 0 },
    { id: 'system-life', name: '生活', kind: 'system', isFallback: false, sortOrder: 2, createdAt: 0, updatedAt: 0 },
    { id: 'system-other', name: '其他', kind: 'system', isFallback: true, sortOrder: 3, createdAt: 0, updatedAt: 0 },
  ]
  private categories: Category[] = [
    { id: 'cat-study', profileId: 'local', name: '学习', status: 'active', sortOrder: 0, createdAt: 0, updatedAt: 0 },
    { id: 'cat-work', profileId: 'local', name: '工作', status: 'active', sortOrder: 1, createdAt: 0, updatedAt: 0 },
    { id: 'cat-life', profileId: 'local', name: '生活', status: 'active', sortOrder: 2, createdAt: 0, updatedAt: 0 },
    { id: 'cat-other', profileId: 'local', name: '其他', status: 'active', sortOrder: 3, createdAt: 0, updatedAt: 0 },
  ]
  private projects: Project[] = []
  private failures: InjectedError[] = []
  private timerExpiredHandler: ((payload: TimerExpiredPayload) => void) | null = null
  private settledHandler: ((payload: { timer: TimerSnapshot; session: TimerSession; newlyCompleted: boolean }) => void) | null = null
  private changedHandler: ((snapshot: TimerSnapshot) => void) | null = null

  /** Queue an error that the next gateway call will reject with. */
  injectFailure(code: string, message = 'injected failure'): void {
    this.failures.push({ code, message })
  }

  /** Drop all queued failures and reset persisted state. */
  reset(): void {
    this.failures = []
    this.tasks = []
    this.settings = { ...DEFAULT_SETTINGS, updatedAt: Date.now() }
    this.timer = idleTimerForMode('focus', this.settings)
    this.sessions = []
  }

  private takeFailure(): void {
    const failure = this.failures.shift()
    if (failure) {
      const error = new Error(failure.message) as Error & { code?: string }
      error.code = failure.code
      throw error
    }
  }

  private computeStatistics(query: StatisticsQuery): Statistics {
    const from = query.from
    const to = query.to
    const focus = this.sessions.filter(
      (s) => s.mode === 'focus' && s.status === 'completed' && s.startedAt >= from && s.startedAt <= to,
    )
    const focusSeconds = focus.reduce((acc, s) => acc + s.focusedSeconds, 0)

    const byDay = new Map<string, { sessions: number; focusSeconds: number }>()
    for (const boundary of query.days) byDay.set(boundary.date, { sessions: 0, focusSeconds: 0 })
    for (const s of focus) {
      const day = this.dayKeyFor(s.startedAt, query.days)
      if (!day) continue
      const entry = byDay.get(day) ?? { sessions: 0, focusSeconds: 0 }
      entry.sessions += 1
      entry.focusSeconds += s.focusedSeconds
      byDay.set(day, entry)
    }

    const byProjectMap = new Map<string, { sessions: number; focusSeconds: number }>()
    for (const s of focus) {
      const entry = byProjectMap.get(s.projectSnapshot) ?? { sessions: 0, focusSeconds: 0 }
      entry.sessions += 1
      entry.focusSeconds += s.focusedSeconds
      byProjectMap.set(s.projectSnapshot, entry)
    }

    const byTagMap = new Map<string, { sessions: number; focusSeconds: number }>()
    for (const s of focus) {
      const tag = s.tagNameSnapshot ?? '其他'
      const entry = byTagMap.get(tag) ?? { sessions: 0, focusSeconds: 0 }
      entry.sessions += 1
      entry.focusSeconds += s.focusedSeconds
      byTagMap.set(tag, entry)
    }

    const best = [...byDay.entries()].sort((a, b) => b[1].focusSeconds - a[1].focusSeconds)[0]

    return {
      from,
      to,
      focusSessionCount: focus.length,
      focusSeconds,
      dailyGoal: this.settings.dailyGoal,
      streakDays: 0,
      bestDay: best ? best[0] : null,
      byDay: [...byDay.entries()].map(([date, v]) => ({ date, ...v })),
      byProject: [...byProjectMap.entries()].map(([project, v]) => ({ project, ...v })),
      byTag: [...byTagMap.entries()].map(([project, v]) => ({ project, ...v })),
      byCategory: [],
    }
  }

  private dayKeyFor(ts: number, days: StatisticsDayBoundary[]): string | null {
    for (const d of days) {
      if (ts >= d.from && ts <= d.to) return d.date
    }
    return null
  }

  async bootstrap(): Promise<BootstrapPayload> {
    this.takeFailure()
    return {
      tasks: this.tasks,
      tags: this.tags,
      settings: this.settings,
      timer: this.timer,
      // Rust's bootstrap lists sessions newest-first (`started_at DESC`).
      sessions: [...this.sessions].sort((a, b) => b.startedAt - a.startedAt),
      statistics: this.computeStatistics({ from: 0, to: Date.now(), days: [] }),
    }
  }

  async startTimer(input: StartTimerInput): Promise<TimerSnapshot> {
    this.takeFailure()
    const durationSeconds = durationSecondsForMode(input.mode, this.settings)
    const now = Date.now()
    const task = this.tasks.find(t => t.id === input.selectedTaskId)
    this.timer = {
      ...this.timer,
      mode: input.mode,
      state: 'running',
      activeSessionId: nextId('session'),
      selectedTaskId: input.selectedTaskId,
      taskTitleSnapshot: input.mode === 'focus'
        ? (task?.title ?? '未指定任务')
        : (input.mode === 'short' ? '短休' : '长休'),
      projectSnapshot: input.mode === 'focus' ? (task?.project ?? '通用') : '休息',
      durationSeconds,
      remainingSeconds: durationSeconds,
      startedAt: now,
      targetEndAt: now + durationSeconds * 1000,
      pausedAt: null,
      revision: this.timer.revision + 1,
      updatedAt: now,
    }
    return this.timer
  }

  async pauseTimer(input: TimerRevisionInput): Promise<TimerSnapshot> {
    this.takeFailure()
    const now = Date.now()
    this.timer = {
      ...this.timer,
      state: 'paused',
      pausedAt: now,
      revision: this.expectedRevision(input),
      updatedAt: now,
    }
    return this.timer
  }

  async resumeTimer(input: TimerRevisionInput): Promise<TimerSnapshot> {
    this.takeFailure()
    const now = Date.now()
    this.timer = {
      ...this.timer,
      state: 'running',
      pausedAt: null,
      revision: this.expectedRevision(input),
      updatedAt: now,
    }
    return this.timer
  }

  async resetTimer(input: TimerRevisionInput): Promise<TimerSnapshot> {
    this.takeFailure()
    const now = Date.now()
    this.timer = {
      ...idleTimerForMode(this.timer.mode, this.settings),
      revision: this.expectedRevision(input),
      updatedAt: now,
    }
    return this.timer
  }

  async switchTimerMode(input: SwitchTimerModeInput): Promise<TimerSnapshot> {    this.takeFailure()
    const now = Date.now()

    // Mirrors the Rust machine: switching submits a started session
    // (completed with actual elapsed time) instead of abandoning it.
    const started =
      this.timer.state !== 'idle' && this.timer.activeSessionId && this.timer.startedAt
    if (started) {
      const focusedSeconds = Math.max(
        0,
        this.timer.durationSeconds -
          (this.timer.state === 'running' && this.timer.targetEndAt
            ? Math.max(0, Math.ceil((this.timer.targetEndAt - now) / 1000))
            : this.timer.remainingSeconds),
      )
      this.sessions = [
        ...this.sessions,
        {
          id: this.timer.activeSessionId!,
          taskId: this.timer.selectedTaskId,
          taskTitleSnapshot: this.timer.taskTitleSnapshot ?? '未指定任务',
          projectSnapshot: this.timer.projectSnapshot ?? '通用',
          mode: this.timer.mode,
          status: 'completed',
          plannedSeconds: this.timer.durationSeconds,
          focusedSeconds,
          startedAt: this.timer.startedAt ?? now,
          endedAt: now,
        },
      ]
    }

    this.timer = {
      ...idleTimerForMode(input.mode, this.settings),
      revision: this.expectedRevision(input),
      updatedAt: now,
    }
    return this.timer
  }

  /** v1.2 B2: same-clock switch — the round keeps its clock and session; the
   *  ledger attributes time per task via segments. */
  async switchTimerTask(input: SwitchTimerTaskInput): Promise<SwitchTimerTaskResult> {
    this.takeFailure()
    const now = Date.now()
    if (this.timer.state !== 'running' && this.timer.state !== 'paused') {
      throw new Error('switch_timer_task requires a running or paused timer')
    }
    if (this.timer.activeSessionId !== input.activeSessionId) {
      throw new Error('activeSessionId does not match timer')
    }
    const newTask = this.tasks.find(t => t.id === input.newTaskId)
    if (!newTask) {
      const error = new Error(`task ${input.newTaskId} not found`) as Error & { code?: string }
      error.code = 'NOT_FOUND'
      throw error
    }
    if (newTask.done) throw new Error('cannot switch to a completed task')
    if (this.timer.mode !== 'focus') throw new Error('tasks can only be switched during a focus round')
    if (this.timer.selectedTaskId === input.newTaskId) {
      return { timer: this.timer, closedSession: null, newlyClosed: false }
    }

    this.timer = {
      ...this.timer,
      state: this.timer.state,
      activeSessionId: input.activeSessionId,
      selectedTaskId: newTask.id,
      taskTitleSnapshot: newTask.title,
      projectSnapshot: newTask.project,
      startedAt: this.timer.startedAt ?? now,
      targetEndAt: this.timer.targetEndAt,
      remainingSeconds: this.timer.remainingSeconds,
      pausedAt: this.timer.state === 'paused' ? now : null,
      revision: this.timer.revision + 1,
      updatedAt: now,
    }
    return { timer: this.timer, closedSession: null, newlyClosed: true }
  }

  async completeTimer(input: CompleteTimerInput): Promise<CompleteTimerResult> {
    this.takeFailure()
    const now = Date.now()
    // Idempotency first (mirrors Rust): an existing completed session for
    // this id returns `newlyCompleted: false` and never appends twice.
    const existing = this.sessions.find(s => s.id === input.activeSessionId)
    if (existing && input.activeSessionId) {
      return {
        timer: this.timer,
        session: existing,
        statistics: this.computeStatistics({ from: 0, to: now, days: [] }),
        newlyCompleted: false,
      }
    }
    // Natural completion means the countdown reached zero: the focused time
    // is the full round (mirrors Rust, where the deadline guard rejects any
    // early call).
    const focusedSeconds = this.timer.durationSeconds
    const session: TimerSession = {
      id: input.activeSessionId || nextId('session'),
      taskId: this.timer.selectedTaskId,
      taskTitleSnapshot: this.timer.taskTitleSnapshot ?? '未指定任务',
      projectSnapshot: this.timer.projectSnapshot ?? '通用',
      mode: this.timer.mode,
      status: 'completed',
      plannedSeconds: this.timer.durationSeconds,
      focusedSeconds: focusedSeconds > 0 ? focusedSeconds : this.timer.durationSeconds,
      startedAt: this.timer.startedAt ?? now,
      endedAt: now,
      finishReason: 'elapsed',
      statisticsEligible: this.timer.mode === 'focus' && focusedSeconds >= 30,
      qualificationReason: this.timer.mode !== 'focus'
        ? 'non_focus'
        : focusedSeconds >= 30 ? 'qualified' : 'too_short',
    }
    this.sessions = [...this.sessions, session]
    this.timer = {
      ...idleTimerForMode(this.timer.mode, this.settings),
      revision: this.timer.revision + 1,
      updatedAt: now,
    }
    return {
      timer: this.timer,
      session,
      statistics: this.computeStatistics({ from: 0, to: now, days: [] }),
      newlyCompleted: true,
    }
  }

  // ─── Tags (v1.1, in-memory) ───────────────────────────────────────────────

  async listTags(): Promise<Tag[]> {
    this.takeFailure()
    return [...this.tags].sort((a, b) => a.sortOrder - b.sortOrder)
  }

  async createTag(input: CreateTagInput): Promise<Tag> {
    this.takeFailure()
    const name = input.name.trim()
    if (!name) throw new Error('标签名称不能为空')
    if (this.tags.some(t => t.name.toLowerCase() === name.toLowerCase())) {
      throw new Error(`标签“${name}”已存在`)
    }
    if (this.tags.length >= 100) throw new Error('标签数量已达上限（100 个）')
    const now = Date.now()
    const tag: Tag = {
      id: nextId('tag'),
      name,
      kind: 'custom',
      isFallback: false,
      sortOrder: Math.max(...this.tags.map(t => t.sortOrder), -1) + 1,
      createdAt: now,
      updatedAt: now,
    }
    this.tags.push(tag)
    return tag
  }

  async updateTag(input: UpdateTagInput): Promise<Tag> {
    this.takeFailure()
    const tag = this.tags.find(t => t.id === input.id)
    if (!tag) throw new Error(`tag ${input.id} not found`)
    if (input.name !== undefined) {
      const name = input.name.trim()
      if (this.tags.some(t => t.id !== input.id && t.name.toLowerCase() === name.toLowerCase())) {
        throw new Error(`标签“${name}”已存在`)
      }
      tag.name = name
      tag.updatedAt = Date.now()
    }
    return tag
  }

  async reorderTag(input: ReorderTagInput): Promise<Tag[]> {
    this.takeFailure()
    const sorted = [...this.tags].sort((a, b) => a.sortOrder - b.sortOrder)
    const index = sorted.findIndex(t => t.id === input.id)
    const target = index + (input.direction < 0 ? -1 : 1)
    if (index >= 0 && target >= 0 && target < sorted.length) {
      const a = sorted[index]
      const b = sorted[target]
      const tmp = a.sortOrder
      a.sortOrder = b.sortOrder
      b.sortOrder = tmp
    }
    return [...this.tags].sort((a, b) => a.sortOrder - b.sortOrder)
  }

  async previewDeleteTag(id: string): Promise<TagDeletePreview> {
    this.takeFailure()
    const tag = this.tags.find(t => t.id === id)
    if (!tag) throw new Error(`tag ${id} not found`)
    if (tag.isFallback) throw new Error('保底标签不能删除')
    return { tagId: id, affectedTasks: this.tasks.filter(t => t.tagId === id).length }
  }

  async deleteTag(id: string): Promise<DeleteTagResult> {
    this.takeFailure()
    const tag = this.tags.find(t => t.id === id)
    if (!tag) throw new Error(`tag ${id} not found`)
    if (tag.isFallback) throw new Error('保底标签不能删除')
    let reassigned = 0
    for (const task of this.tasks) {
      if (task.tagId === id) {
        task.tagId = 'system-other'
        reassigned += 1
      }
    }
    for (const session of this.sessions) {
      if (session.tagId === id) session.tagId = null
    }
    this.tags = this.tags.filter(t => t.id !== id)
    return {
      deletedTagId: id,
      fallbackTagId: 'system-other',
      reassignedTasks: reassigned,
      tags: [...this.tags],
      tasks: [...this.tasks],
    }
  }

  async createTask(input: CreateTaskInput): Promise<Task> {
    this.takeFailure()
    const now = Date.now()
    const projectId = input.projectId?.trim() || null
    const project = projectId ? this.projects.find((p) => p.id === projectId) : undefined
    if (projectId && !project) {
      const error = new Error('project not found') as Error & { code?: string }
      error.code = 'VALIDATION_ERROR'
      throw error
    }
    const task: Task = {
      id: nextId('task'),
      title: input.title,
      done: false,
      pomodoroTarget: input.pomodoroTarget,
      priority: input.priority,
      project: project?.name ?? '通用',
      projectId,
      targetSeconds: input.pomodoroTarget * this.settings.focusDurationMinutes * 60,
      budgetSource: 'creation',
      status: 'todo',
      deadline: input.deadline ?? null,
      notes: input.notes ?? '',
      tagId: 'system-other',
      sortOrder: this.tasks.length,
      createdAt: now,
      updatedAt: now,
      completedAt: null,
    }
    this.tasks = [...this.tasks, task]
    return task
  }

  async updateTask(input: UpdateTaskInput): Promise<Task> {
    this.takeFailure()
    const now = Date.now()
    const target = this.tasks.find((t) => t.id === input.id)
    if (!target) {
      const error = new Error('task not found') as Error & { code?: string }
      error.code = 'NOT_FOUND'
      throw error
    }
    const updated: Task = {
      ...target,
      ...(input.title !== undefined ? { title: input.title } : {}),
      ...(input.pomodoroTarget !== undefined ? { pomodoroTarget: input.pomodoroTarget } : {}),
      ...(input.priority !== undefined ? { priority: input.priority } : {}),
      ...(input.projectId !== undefined
        ? (() => {
            const project = this.projects.find((p) => p.id === input.projectId)
            return { projectId: input.projectId || null, project: project?.name ?? '通用' }
          })()
        : {}),
      ...(input.project !== undefined ? { project: input.project } : {}),
      ...(input.targetSeconds !== undefined
        ? { targetSeconds: input.targetSeconds, budgetSource: 'recalc' }
        : {}),
      ...(input.deadline !== undefined ? { deadline: input.deadline || null } : {}),
      ...(input.notes !== undefined ? { notes: input.notes } : {}),
      ...(input.archived !== undefined
        ? { status: input.archived ? 'archived' : target.done ? 'done' : 'todo' }
        : {}),
      ...(input.done !== undefined
        ? { done: input.done, completedAt: input.done ? now : null, status: input.done ? 'done' : 'todo' }
        : {}),
      updatedAt: now,
    } as Task
    this.tasks = this.tasks.map((t) => (t.id === input.id ? updated : t))
    return updated
  }

  async deleteTask(id: string): Promise<void> {
    this.takeFailure()
    this.tasks = this.tasks.filter((t) => t.id !== id)
  }

  async saveSettings(input: AppSettings): Promise<SaveSettingsResult> {
    this.takeFailure()
    const now = Date.now()
    this.settings = { ...input, updatedAt: now }
    const timer: TimerSnapshot =
      this.timer.state === 'idle'
        ? { ...idleTimerForMode(this.timer.mode as TimerMode, this.settings), revision: this.timer.revision, updatedAt: now }
        : this.timer
    return { settings: this.settings, timer }
  }

  async listSessions(query: SessionQuery): Promise<TimerSession[]> {
    this.takeFailure()
    const scope = query.scope ?? 'activity'
    let result = this.sessions
    if (scope !== 'all') result = result.filter((s) => s.statisticsEligible === true)
    if (query.from !== undefined) result = result.filter((s) => s.startedAt >= query.from!)
    if (query.to !== undefined) result = result.filter((s) => s.startedAt <= query.to!)
    result = result.slice().sort((a, b) => b.startedAt - a.startedAt)
    if (query.limit !== undefined) result = result.slice(0, query.limit)
    return result
  }

  /** v1.1 manual finish — records actual focused time; timer returns to idle. */
  async finishTimer(input: FinishTimerInput): Promise<FinishTimerResult> {
    this.takeFailure()
    const now = Date.now()
    const existing = this.sessions.find(s => s.id === input.activeSessionId)
    if (existing) {
      // Idempotent replay — no second session, no second effect.
      return {
        timer: this.timer,
        session: existing,
        statistics: this.computeStatistics({ from: 0, to: now, days: [] }),
        newlyFinished: false,
        statisticsEligible: existing.statisticsEligible ?? false,
        qualificationReason: existing.qualificationReason ?? 'legacy',
      }
    }
    if (this.timer.state !== 'running' && this.timer.state !== 'paused') {
      throw new Error('CONFLICT: finish_timer requires a running or paused timer')
    }
    const focused = Math.max(
      0,
      Math.round((now - (this.timer.startedAt ?? now)) / 1000),
    )
    const eligible = this.timer.mode === 'focus' && focused >= 30
    const session: TimerSession = {
      id: input.activeSessionId,
      taskId: this.timer.selectedTaskId,
      taskTitleSnapshot: this.timer.taskTitleSnapshot ?? '未指定任务',
      projectSnapshot: this.timer.projectSnapshot ?? '通用',
      tagId: this.timer.tagId ?? 'system-other',
      tagNameSnapshot: this.timer.tagNameSnapshot ?? '其他',
      mode: this.timer.mode,
      status: 'completed',
      plannedSeconds: this.timer.durationSeconds,
      focusedSeconds: focused,
      startedAt: this.timer.startedAt ?? now,
      endedAt: now,
      finishReason: 'manual_finish',
      statisticsEligible: eligible,
      qualificationReason: this.timer.mode !== 'focus'
        ? 'non_focus'
        : eligible ? 'qualified' : 'too_short',
    }
    this.sessions = [...this.sessions, session]
    this.timer = {
      ...idleTimerForMode(this.timer.mode, this.settings),
      revision: this.timer.revision + 1,
      updatedAt: now,
    }
    return {
      timer: this.timer,
      session,
      statistics: this.computeStatistics({ from: 0, to: now, days: [] }),
      newlyFinished: true,
      statisticsEligible: eligible,
      qualificationReason: session.qualificationReason ?? 'legacy',
    }
  }

  async getStatistics(query: StatisticsQuery): Promise<Statistics> {
    this.takeFailure()
    return this.computeStatistics(query)
  }

  // ─── Categories, projects & task ledger (v1.2, in-memory) ─────────────────

  async listCategories(): Promise<Category[]> {
    this.takeFailure()
    return [...this.categories].sort((a, b) => a.sortOrder - b.sortOrder)
  }

  async createCategory(input: CreateCategoryInput): Promise<Category> {
    this.takeFailure()
    const name = input.name.trim()
    if (!name) throw new Error('类别名称不能为空')
    if (this.categories.some((c) => c.name === name)) throw new Error(`类别"${name}"已存在`)
    const now = Date.now()
    const category: Category = {
      id: nextId('cat'),
      profileId: 'local',
      name,
      status: 'active',
      sortOrder: Math.max(...this.categories.map((c) => c.sortOrder), -1) + 1,
      createdAt: now,
      updatedAt: now,
    }
    this.categories.push(category)
    return category
  }

  async updateCategory(input: UpdateCategoryInput): Promise<Category> {
    this.takeFailure()
    const category = this.categories.find((c) => c.id === input.id)
    if (!category) throw new Error(`category ${input.id} not found`)
    if (input.name !== undefined) category.name = input.name.trim()
    if (input.archived !== undefined) category.status = input.archived ? 'archived' : 'active'
    category.updatedAt = Date.now()
    return category
  }

  async listProjects(): Promise<Project[]> {
    this.takeFailure()
    return [...this.projects]
  }

  async createProject(input: CreateProjectInput): Promise<Project> {
    this.takeFailure()
    const name = input.name.trim()
    if (!name) throw new Error('项目名称不能为空')
    if (this.projects.some((p) => p.name === name)) throw new Error(`项目"${name}"已存在`)
    const now = Date.now()
    const category = this.categories.find((c) => c.id === input.categoryId)
    if (!category) throw new Error('category not found')
    const project: Project = {
      id: nextId('prj'),
      profileId: 'local',
      categoryId: input.categoryId,
      categoryName: category.name,
      name,
      description: input.description ?? '',
      status: 'active',
      planStartDate: input.planStartDate ?? null,
      dueDate: input.dueDate ?? null,
      sortOrder: this.projects.length,
      createdAt: now,
      updatedAt: now,
      taskCount: 0,
      doneTaskCount: 0,
    }
    this.projects.push(project)
    return project
  }

  async updateProject(input: UpdateProjectInput): Promise<Project> {
    this.takeFailure()
    const project = this.projects.find((p) => p.id === input.id)
    if (!project) throw new Error(`project ${input.id} not found`)
    if (input.name !== undefined) project.name = input.name.trim()
    if (input.categoryId !== undefined) {
      const category = this.categories.find((c) => c.id === input.categoryId)
      if (!category) throw new Error('category not found')
      project.categoryId = input.categoryId
      project.categoryName = category.name
    }
    if (input.description !== undefined) project.description = input.description
    if (input.status !== undefined) project.status = input.status
    if (input.planStartDate !== undefined) project.planStartDate = input.planStartDate || null
    if (input.dueDate !== undefined) project.dueDate = input.dueDate || null
    project.updatedAt = Date.now()
    return project
  }

  async getAllTaskProgress(): Promise<TaskProgress[]> {
    this.takeFailure()
    const map = new Map(this.tasks.map((t) => [t.id, {
      taskId: t.id,
      confirmedSeconds: 0,
      provisionalSeconds: 0,
      targetSeconds: t.targetSeconds,
      progress: 0,
      overSeconds: 0,
    }]))
    return [...map.values()]
  }

  async getTaskProgress(taskId: string): Promise<TaskProgress> {
    this.takeFailure()
    const task = this.tasks.find((t) => t.id === taskId)
    if (!task) throw new Error(`task ${taskId} not found`)
    return {
      taskId,
      confirmedSeconds: 0,
      provisionalSeconds: 0,
      targetSeconds: task.targetSeconds,
      progress: 0,
      overSeconds: 0,
    }
  }

  async completeTaskNow(input: CompleteTaskInput): Promise<CompleteTaskResult> {
    this.takeFailure()
    const now = Date.now()
    const task = this.tasks.find((t) => t.id === input.taskId)
    if (!task) throw new Error('task not found')
    if (task.done) {
      return { task, timer: this.timer, segmentSavedMs: 0, newlyCompleted: false }
    }
    task.done = true
    task.status = 'done'
    task.completedAt = now
    const focused = Math.max(0, this.timer.durationSeconds - this.timer.remainingSeconds)
    this.timer = {
      ...this.timer,
      state: 'paused',
      selectedTaskId: null,
      taskTitleSnapshot: '未指定任务',
      projectSnapshot: '通用',
      targetEndAt: null,
      pausedAt: now,
      revision: this.timer.revision + 1,
      updatedAt: now,
    }
    return { task: { ...task }, timer: this.timer, segmentSavedMs: focused * 1000, newlyCompleted: true }
  }
  // ─── Mini window (v1.3 B/C, in-memory stubs) ────────────────────────────────

  miniPrefs: MiniWindowPrefs = { x: null, y: null, alwaysOnTop: true, collapsed: false }

  async showMainWindow(): Promise<void> { this.takeFailure() }
  async toggleMiniWindow(): Promise<void> { this.takeFailure() }
  async loadMiniPrefs(): Promise<MiniWindowPrefs> {
    this.takeFailure()
    return this.miniPrefs
  }
  async saveMiniPrefs(prefs: MiniWindowPrefs): Promise<void> {
    this.takeFailure()
    this.miniPrefs = prefs
  }
  async undoCompleteTask(taskId: string): Promise<Task> {
    this.takeFailure()
    const task = this.tasks.find((t) => t.id === taskId)
    if (!task) throw new Error(`task ${taskId} not found`)
    if (!task.done) throw new Error('task is not completed')
    task.done = false
    task.status = 'todo'
    task.completedAt = null
    return { ...task }
  }

  // --- Tray surface (no-op in tests, but recorded for assertions) ---

  /** Last indicator pushed by the App, useful for component-test assertions. */
  lastTrayIndicator: TrayIndicator | null = null

  async setTrayIndicator(input: TrayIndicator): Promise<void> {
    this.takeFailure()
    this.lastTrayIndicator = input
  }

  subscribeTimerExpired(cb: (payload: TimerExpiredPayload) => void): () => void {
    // Store the handler so tests can simulate the Rust backstop event.
    this.timerExpiredHandler = cb
    return () => { this.timerExpiredHandler = null }
  }

  subscribeTimerSettled(cb: (payload: { timer: TimerSnapshot; session: TimerSession; newlyCompleted: boolean }) => void): () => void {
    this.settledHandler = cb
    return () => { this.settledHandler = null }
  }

  subscribeTimerChanged(cb: (snapshot: TimerSnapshot) => void): () => void {
    this.changedHandler = cb
    return () => { this.changedHandler = null }
  }

  /** Test hook: fire the background `timer-expired` event like Rust's ticker. */
  emitTimerExpired(): void {
    this.timerExpiredHandler?.({ activeSessionId: this.timer.activeSessionId ?? '', expectedRevision: this.timer.revision })
  }

  /** Test hook: fire the v1.3 authoritative settlement event. */
  emitTimerSettled(sessionId: string): void {
    const session = this.sessions.find(s => s.id === sessionId)
    if (!session) return
    this.settledHandler?.({ timer: this.timer, session, newlyCompleted: true })
  }

  /** Test hook: fire the v1.3 snapshot broadcast. */
  emitTimerChanged(): void {
    this.changedHandler?.(this.timer)
  }

  /** Test hook: drop a persisted session directly into the (fake) database. */
  seedSession(session: TimerSession): void {
    this.sessions = [...this.sessions, session]
  }

  /** Test accessors for asserting persisted state without private-field pokes. */
  get currentTimer(): TimerSnapshot { return this.timer }
  get currentTasks(): Task[] { return this.tasks }

  subscribeTrayAction(_cb: (action: TrayAction) => void): () => void {
    return () => undefined
  }

  subscribeGlobalShortcutConflict(_cb: (shortcut: string) => void): () => void {
    return () => undefined
  }

  // --- Autostart (in-memory stub for tests) ---

  /** Mirrors the OS registry state for component tests. */
  launchAtLogin = false

  async getAutostart(): Promise<boolean> {
    this.takeFailure()
    return this.launchAtLogin
  }

  async setAutostart(enabled: boolean): Promise<boolean> {
    this.takeFailure()
    this.launchAtLogin = enabled
    return enabled
  }

  // --- Data export & backup (no-op stubs for tests) ---

  async pickExportPath(suggestedName: string): Promise<string | null> {
    this.takeFailure()
    return `${suggestedName}.json`
  }

  async pickImportPath(): Promise<string | null> {
    this.takeFailure()
    return 'import.json'
  }

  async exportBackup(_path: string): Promise<ExportSummary> {
    this.takeFailure()
    return { path: _path, bytes: 0, tasks: this.tasks.length, sessions: this.sessions.length }
  }

  async exportSessionsCsv(_path: string): Promise<ExportSummary> {
    this.takeFailure()
    return { path: _path, bytes: 0, tasks: 0, sessions: this.sessions.length }
  }

  async previewImport(_path: string): Promise<ImportPreview> {
    this.takeFailure()
    return {
      schemaVersion: 2,
      tags: this.tags.length,
      tasks: this.tasks.length,
      sessions: this.sessions.length,
    }
  }

  async importBackup(_path: string): Promise<ImportSummary> {
    this.takeFailure()
    // Mirror the Rust replace: wipe and re-seed from the (fake) current data.
    return { path: _path, tasks: this.tasks.length, sessions: this.sessions.length }
  }

  private expectedRevision(input: TimerRevisionInput): number {
    // Optimistic concurrency: reject stale writes the same way Rust would.
    if (input.expectedRevision < this.timer.revision) {
      const error = new Error('timer revision conflict') as Error & { code?: string }
      error.code = 'CONFLICT'
      throw error
    }
    return input.expectedRevision
  }
}
