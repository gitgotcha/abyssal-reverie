import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { TaskDetailDialog } from '../TaskDetailDialog'
import type { Task } from '../../../domain/models'

afterEach(() => {
  cleanup()
  try { window.localStorage?.clear() } catch { /* jsdom may disable storage */ }
  try { window.sessionStorage?.clear() } catch { /* jsdom may disable storage */ }
  vi.restoreAllMocks()
})

const taskWithoutPriority: Task = {
  id: 'task-null-priority',
  title: '未设置优先级',
  done: false,
  pomodoroTarget: 1,
  priority: null,
  project: '通用',
  projectId: null,
  targetSeconds: 1500,
  budgetSource: 'creation',
  status: 'todo',
  deadline: null,
  notes: '',
  tagId: null,
  sortOrder: 0,
  createdAt: 0,
  updatedAt: 0,
  completedAt: null,
  relationshipRevision: 0,
}

describe('TaskDetailDialog optional priority', () => {
  it('keeps a null priority unset when another field is edited and saved', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn(async () => undefined)
    render(
      <TaskDetailDialog
        task={taskWithoutPriority}
        categories={[]}
        projects={[]}
        tags={[]}
        onClose={vi.fn()}
        onSave={onSave}
        onApplyRelationship={vi.fn(async () => undefined)}
        onToggleDone={vi.fn()}
        onArchive={vi.fn()}
        onComplete={vi.fn(async () => undefined)}
      />,
    )

    await user.clear(screen.getByRole('textbox', { name: '任务标题' }))
    await user.type(screen.getByRole('textbox', { name: '任务标题' }), '标题已编辑')
    await user.click(screen.getByRole('button', { name: '保存' }))

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1))
    const saveCalls = onSave.mock.calls as unknown as Array<[Record<string, unknown>]>
    expect(saveCalls[0][0].priority).toBeUndefined()
  })

  it('offers an explicit unset priority option', () => {
    render(
      <TaskDetailDialog
        task={taskWithoutPriority}
        categories={[]}
        projects={[]}
        tags={[]}
        onClose={vi.fn()}
        onSave={vi.fn(async () => undefined)}
        onApplyRelationship={vi.fn(async () => undefined)}
        onToggleDone={vi.fn()}
        onArchive={vi.fn()}
        onComplete={vi.fn(async () => undefined)}
      />,
    )

    expect(screen.getAllByRole('option', { name: '未设置' }).length).toBeGreaterThanOrEqual(1)
    expect((screen.getByRole('combobox', { name: '优先级' }) as HTMLSelectElement).value).toBe('')
  })

  it('uses the relationship patch to clear selected optional metadata', async () => {
    const user = userEvent.setup()
    const onApplyRelationship = vi.fn(async () => undefined)
    const task: Task = {
      ...taskWithoutPriority,
      priority: 'med',
      deadline: '2026-12-31',
      notes: '待清空',
    }
    render(
      <TaskDetailDialog
        task={task}
        categories={[]}
        projects={[]}
        tags={[]}
        onClose={vi.fn()}
        onSave={vi.fn(async () => undefined)}
        onApplyRelationship={onApplyRelationship}
        onToggleDone={vi.fn()}
        onArchive={vi.fn()}
        onComplete={vi.fn(async () => undefined)}
      />,
    )

    await user.selectOptions(screen.getByRole('combobox', { name: '优先级' }), '')
    await user.clear(screen.getByLabelText('截止日期'))
    await user.clear(screen.getByRole('textbox', { name: '备注' }))
    await user.click(screen.getByRole('button', { name: '保存' }))

    await waitFor(() => expect(onApplyRelationship).toHaveBeenCalledTimes(1))
    const relationshipCalls = onApplyRelationship.mock.calls as unknown as Array<[Record<string, any>]>
    const patch = relationshipCalls[0][0]
    expect(patch.priority).toEqual({ action: 'clear' })
    expect(patch.deadline).toEqual({ action: 'clear' })
    expect(patch.notes).toEqual({ action: 'clear' })
  })

  it('saves an explicit budget edit as target seconds without changing relationships', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn(async () => undefined)
    const onApplyRelationship = vi.fn(async () => undefined)
    render(
      <TaskDetailDialog
        task={{ ...taskWithoutPriority, targetSeconds: 25 * 60 }}
        categories={[]}
        projects={[]}
        tags={[]}
        onClose={vi.fn()}
        onSave={onSave}
        onApplyRelationship={onApplyRelationship}
        onToggleDone={vi.fn()}
        onArchive={vi.fn()}
        onComplete={vi.fn(async () => undefined)}
      />,
    )

    const budget = screen.getByRole('spinbutton', { name: '预算分钟' })
    await user.clear(budget)
    await user.type(budget, '40')
    await user.click(screen.getByRole('button', { name: '保存' }))

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1))
    const saveCalls = onSave.mock.calls as unknown as Array<[Record<string, unknown>]>
    expect(saveCalls[0][0].targetSeconds).toBe(40 * 60)
    expect(onApplyRelationship).not.toHaveBeenCalled()
  })

  it('restores an unfinished draft after leaving and returning to a task', async () => {
    const user = userEvent.setup()
    const first = render(
      <TaskDetailDialog
        task={taskWithoutPriority}
        categories={[]}
        projects={[]}
        tags={[]}
        onClose={vi.fn()}
        onSave={vi.fn(async () => undefined)}
        onApplyRelationship={vi.fn(async () => undefined)}
        onToggleDone={vi.fn()}
        onArchive={vi.fn()}
        onComplete={vi.fn(async () => undefined)}
      />,
    )
    await user.clear(screen.getByRole('textbox', { name: '任务标题' }))
    await user.type(screen.getByRole('textbox', { name: '任务标题' }), '离开后仍在')
    first.unmount()

    render(
      <TaskDetailDialog
        task={taskWithoutPriority}
        categories={[]}
        projects={[]}
        tags={[]}
        onClose={vi.fn()}
        onSave={vi.fn(async () => undefined)}
        onApplyRelationship={vi.fn(async () => undefined)}
        onToggleDone={vi.fn()}
        onArchive={vi.fn()}
        onComplete={vi.fn(async () => undefined)}
      />,
    )
    expect(screen.getByRole('textbox', { name: '任务标题' })).toHaveValue('离开后仍在')
    expect(screen.getByText('已恢复未保存草稿')).toBeInTheDocument()
  })

  it('asks before discarding an unfinished draft', async () => {
    const user = userEvent.setup()
    const onClose = vi.fn()
    vi.spyOn(window, 'confirm').mockReturnValue(false)
    render(
      <TaskDetailDialog
        task={taskWithoutPriority}
        categories={[]}
        projects={[]}
        tags={[]}
        onClose={onClose}
        onSave={vi.fn(async () => undefined)}
        onApplyRelationship={vi.fn(async () => undefined)}
        onToggleDone={vi.fn()}
        onArchive={vi.fn()}
        onComplete={vi.fn(async () => undefined)}
      />,
    )
    await user.type(screen.getByRole('textbox', { name: '任务标题' }), '修改')
    await user.click(screen.getByRole('button', { name: '关闭任务详情' }))
    expect(window.confirm).toHaveBeenCalled()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('shows the current archived project without offering it as a new choice', () => {
    render(
      <TaskDetailDialog
        task={{ ...taskWithoutPriority, projectId: 'p-archived', project: '旧项目' }}
        categories={[{ id: 'cat', profileId: 'local', name: '工作', status: 'active', sortOrder: 0, createdAt: 0, updatedAt: 0 }]}
        projects={[{ id: 'p-archived', profileId: 'local', categoryId: 'cat', categoryName: '工作', name: '旧项目', description: '', status: 'archived', planStartDate: null, dueDate: null, sortOrder: 0, taskCount: 1, doneTaskCount: 0, createdAt: 0, updatedAt: 0 }]}
        tags={[]}
        onClose={vi.fn()}
        onSave={vi.fn(async () => undefined)}
        onApplyRelationship={vi.fn(async () => undefined)}
        onToggleDone={vi.fn()}
        onArchive={vi.fn()}
        onComplete={vi.fn(async () => undefined)}
      />,
    )
    expect(screen.getByRole('combobox', { name: '所属项目' })).toHaveValue('已归档 · 旧项目')
  })
})
