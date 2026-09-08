import { afterEach, describe, expect, it, vi } from 'vitest'
import { FakeAppGateway } from './FakeAppGateway'
import type { NullablePatch, RelationshipPatch } from '../domain/models'
import { DEFAULT_SETTINGS } from '../domain/defaults'

// 阶段 C（任务 1.9）契约行为组：FakeAppGateway 与 Rust 权威语义对齐——
// 标题-only 创建落 NULL、显式清空通道、修订号守卫、无标签轮次快照。

const keep = <T,>(): NullablePatch<T> => ({ action: 'keep' })
const clear = <T,>(): NullablePatch<T> => ({ action: 'clear' })
const set = <T,>(value: T): NullablePatch<T> => ({ action: 'set', value })

afterEach(() => vi.useRealTimers())

describe('FakeAppGateway nullable contract (task 1.9)', () => {
  it('title-only creation lands NULL tag and priority (红灯② 前端侧转绿)', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({ title: '只标题', pomodoroTarget: 1 })
    expect(task.tagId).toBeNull()
    expect(task.priority).toBeNull()
    expect(task.relationshipRevision).toBe(0)
  })

  it('explicit tag/priority choices are respected verbatim', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({
      title: '显式选择',
      pomodoroTarget: 1,
      tagId: 'system-work',
      priority: 'high',
    })
    expect(task.tagId).toBe('system-work')
    expect(task.priority).toBe('high')
  })

  it('applyTaskRelationship clear NULLs the field and bumps the revision once', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({
      title: '要清空',
      pomodoroTarget: 1,
      tagId: 'system-work',
      priority: 'med',
    })
    const patch: RelationshipPatch = {
      taskId: task.id,
      expectedRevision: task.relationshipRevision,
      project: keep(),
      tag: clear(),
      priority: clear(),
      deadline: keep(),
      notes: keep(),
    }
    const updated = await gw.applyTaskRelationship(patch)
    expect(updated.tagId).toBeNull()
    expect(updated.priority).toBeNull()
    expect(updated.relationshipRevision).toBe(task.relationshipRevision + 1)
  })

  it('keep-everything patch is a no-op (no revision bump)', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({
      title: '无操作',
      pomodoroTarget: 1,
      tagId: 'system-work',
      priority: 'med',
    })
    const updated = await gw.applyTaskRelationship({
      taskId: task.id,
      expectedRevision: task.relationshipRevision,
      project: keep(),
      tag: keep(),
      priority: keep(),
      deadline: keep(),
      notes: keep(),
    })
    expect(updated.relationshipRevision).toBe(task.relationshipRevision)
  })

  it('clearing an already-NULL field does not bump the revision', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({ title: '本来就无', pomodoroTarget: 1 })
    const updated = await gw.applyTaskRelationship({
      taskId: task.id,
      expectedRevision: task.relationshipRevision,
      project: keep(),
      tag: clear(),
      priority: clear(),
      deadline: keep(),
      notes: keep(),
    })
    expect(updated.relationshipRevision).toBe(task.relationshipRevision)
  })

  it('clears deadline and notes through the relationship patch', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({
      title: '清空日期备注',
      pomodoroTarget: 1,
      deadline: '2026-12-31',
      notes: '待清空',
    })
    const updated = await gw.applyTaskRelationship({
      taskId: task.id,
      expectedRevision: task.relationshipRevision,
      project: keep(),
      tag: keep(),
      priority: keep(),
      deadline: clear(),
      notes: clear(),
    })
    expect(updated.deadline).toBeNull()
    expect(updated.notes).toBe('')
    expect(updated.relationshipRevision).toBe(task.relationshipRevision + 1)
  })

  it('stale expectedRevision is rejected even when the patch is equivalent', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({
      title: '并发等价',
      pomodoroTarget: 1,
      tagId: 'system-work',
      priority: 'med',
    })
    await expect(
      gw.applyTaskRelationship({
        taskId: task.id,
        expectedRevision: task.relationshipRevision + 1,
        project: keep(),
        tag: keep(),
        priority: keep(),
        deadline: keep(),
        notes: keep(),
      }),
    ).rejects.toMatchObject({ code: 'CONFLICT' })
  })

  it('setting a missing tag is NOT_FOUND', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({ title: '标签校验', pomodoroTarget: 1 })
    await expect(
      gw.applyTaskRelationship({
        taskId: task.id,
        expectedRevision: task.relationshipRevision,
        project: keep(),
        tag: set('tag-missing'),
        priority: keep(),
        deadline: keep(),
        notes: keep(),
      }),
    ).rejects.toMatchObject({ code: 'NOT_FOUND' })
  })

  it('rejects attaching a task to an archived project', async () => {
    const gw = new FakeAppGateway()
    const project = await gw.createProject({ name: '已归档项目', categoryId: 'cat-study' })
    await gw.updateProject({ id: project.id, status: 'archived' })
    const task = await gw.createTask({ title: '项目校验', pomodoroTarget: 1 })

    await expect(
      gw.applyTaskRelationship({
        taskId: task.id,
        expectedRevision: task.relationshipRevision,
        project: set(project.id),
        tag: keep(),
        priority: keep(),
        deadline: keep(),
        notes: keep(),
      }),
    ).rejects.toMatchObject({ code: 'VALIDATION' })
  })

  it('tagless task timer round carries no fallback tag', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({ title: '无标签计时', pomodoroTarget: 1 })
    const timer = await gw.startTimer({
      expectedRevision: 0,
      mode: 'focus',
      selectedTaskId: task.id,
    })
    expect(timer.tagId).toBeNull()
    expect(timer.tagNameSnapshot).toBe('无标签')
  })

  it('manual finish preserves a tagless focus snapshot', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({ title: '无标签手动结束', pomodoroTarget: 1 })
    const timer = await gw.startTimer({ expectedRevision: 0, mode: 'focus', selectedTaskId: task.id })
    const result = await gw.finishTimer({ expectedRevision: timer.revision, activeSessionId: timer.activeSessionId! })
    expect(result.session.tagId).toBeNull()
    expect(result.session.tagNameSnapshot).toBe('无标签')
  })

  it('keeps old task budgets and active timer duration when the default changes', async () => {
    const gw = new FakeAppGateway()
    const task = await gw.createTask({ title: '固定预算', pomodoroTarget: 2 })
    const running = await gw.startTimer({
      expectedRevision: 0,
      mode: 'focus',
      selectedTaskId: task.id,
    })
    const changed = await gw.saveSettings({
      ...DEFAULT_SETTINGS,
      focusDurationMinutes: 40,
    })

    const bootstrap = await gw.bootstrap()
    expect(bootstrap.tasks.find((item) => item.id === task.id)?.targetSeconds).toBe(2 * 25 * 60)
    expect(changed.timer.durationSeconds).toBe(running.durationSeconds)
    expect(changed.timer.remainingSeconds).toBe(running.remainingSeconds)
  })

  it('break rounds keep the fallback tag snapshot', async () => {
    const gw = new FakeAppGateway()
    const timer = await gw.startTimer({
      expectedRevision: 0,
      mode: 'short',
      selectedTaskId: null,
    })
    expect(timer.tagId).toBe('system-other')
    expect(timer.tagNameSnapshot).toBe('其他')
  })

  it('clears task associations when a tag is deleted', async () => {
    const gw = new FakeAppGateway()
    const tag = await gw.createTag({ name: '待删除' })
    const task = await gw.createTask({
      title: '不应被自动改标签',
      pomodoroTarget: 1,
      tagId: tag.id,
    })
    await gw.deleteTag(tag.id)
    const restored = (await gw.bootstrap()).tasks.find((item) => item.id === task.id)
    expect(restored?.tagId).toBeNull()
  })

  it('treats the session query upper bound as exclusive', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(1000)
    const gw = new FakeAppGateway()
    const timer = await gw.startTimer({
      expectedRevision: 0,
      mode: 'focus',
      selectedTaskId: null,
    })
    await gw.finishTimer({ expectedRevision: timer.revision, activeSessionId: timer.activeSessionId! })

    const sessions = await gw.listSessions({ scope: 'all', from: 0, to: 1000 })
    expect(sessions).toHaveLength(0)
  })
})
