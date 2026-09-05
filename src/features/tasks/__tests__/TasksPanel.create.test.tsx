import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { TasksPanel } from '../TasksPanel'
import type { Tag, Task } from '../../../domain/models'

afterEach(() => cleanup())

const fallbackTag: Tag = {
  id: 'system-other',
  name: '其他',
  kind: 'system',
  isFallback: true,
  sortOrder: 0,
  createdAt: 0,
  updatedAt: 0,
}

function renderPanel(overrides: {
  onCreateTask?: ReturnType<typeof vi.fn>
  onNotify?: ReturnType<typeof vi.fn>
} = {}) {
  const onCreateTask = overrides.onCreateTask ?? vi.fn(async () => {
    const task: Task = {
      id: 'task-1',
      title: 'x',
      done: false,
      pomodoroTarget: 1,
      priority: 'med',
      project: '通用',
      tagId: 'system-other',
      sortOrder: 0,
      createdAt: 0,
      updatedAt: 0,
      completedAt: null,
    }
    return task
  })
  const onNotify = overrides.onNotify ?? vi.fn()
  render(
    <TasksPanel
      tasks={[]}
      tags={[fallbackTag]}
      onCreateTask={onCreateTask as unknown as (input: unknown) => Promise<unknown>}
      onToggleTask={vi.fn()}
      onDeleteTask={vi.fn()}
      onCyclePriority={vi.fn()}
      onNotify={onNotify as unknown as (message: string) => void}
      tagOps={{
        createTag: vi.fn(),
        renameTag: vi.fn(),
        reorderTag: vi.fn(),
        previewDeleteTag: vi.fn(async () => ({ tagId: 'x', affectedTasks: 0 })),
        deleteTag: vi.fn(),
      }}
    />,
  )
  return { onCreateTask, onNotify }
}

describe('TasksPanel create feedback (v1.1.2 stage A)', () => {
  it('clears the title only after the create succeeds', async () => {
    const user = userEvent.setup()
    let resolveCreate: (value: unknown) => void = () => undefined
    const { onCreateTask } = renderPanel({
      onCreateTask: vi.fn(
        () => new Promise((resolve) => { resolveCreate = resolve }),
      ) as unknown as ReturnType<typeof vi.fn>,
    })

    const input = screen.getByPlaceholderText('添加任务…')
    await user.type(input, '写周报')
    await user.click(screen.getByRole('button', { name: '添加任务' }))

    // While the create is still in flight the draft must remain untouched.
    expect(onCreateTask).toHaveBeenCalledTimes(1)
    expect((input as HTMLInputElement).value).toBe('写周报')

    resolveCreate(undefined)
    await waitFor(() => {
      expect((input as HTMLInputElement).value).toBe('')
    })
  })

  it('keeps the full draft and notifies when the create fails', async () => {
    const user = userEvent.setup()
    const { onCreateTask, onNotify } = renderPanel({
      onCreateTask: vi.fn(async () => {
        throw new Error('database is locked')
      }) as unknown as ReturnType<typeof vi.fn>,
    })

    const input = screen.getByPlaceholderText('添加任务…')
    await user.type(input, '失败也要保留草稿')
    await user.click(screen.getByRole('button', { name: '添加任务' }))

    await waitFor(() => {
      expect(onNotify).toHaveBeenCalledTimes(1)
    })
    expect(String(onNotify.mock.calls[0][0])).toContain('database is locked')
    expect(onCreateTask).toHaveBeenCalledTimes(1)
    // Draft survives so the user can retry in place.
    expect((input as HTMLInputElement).value).toBe('失败也要保留草稿')
    // The input is refocused for an immediate retry.
    expect(input).toHaveFocus()
  })

  it('ignores a second submit while one is already in flight', async () => {
    const user = userEvent.setup()
    const { onCreateTask } = renderPanel({
      onCreateTask: vi.fn(
        () => new Promise((resolve) => { setTimeout(() => resolve(undefined), 30) }),
      ) as unknown as ReturnType<typeof vi.fn>,
    })

    const input = screen.getByPlaceholderText('添加任务…')
    await user.type(input, '只创建一次')
    await user.click(screen.getByRole('button', { name: '添加任务' }))
    await user.click(screen.getByRole('button', { name: '添加任务' }))

    await waitFor(() => {
      expect((input as HTMLInputElement).value).toBe('')
    })
    expect(onCreateTask).toHaveBeenCalledTimes(1)
  })

  it('does not submit while an IME composition is active (Enter to confirm candidates)', async () => {
    const user = userEvent.setup()
    const { onCreateTask } = renderPanel()

    const input = screen.getByPlaceholderText('添加任务…')
    await user.type(input, '中文输入法')
    // Simulate the Enter keydown that confirms an IME candidate list: the
    // browser reports it as composing (keyCode 229). jsdom does not set
    // isComposing from the constructor — patch it the way real composition
    // events look before the handler runs.
    const composingEvent = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
    Object.defineProperty(composingEvent, 'isComposing', { value: true })
    Object.defineProperty(composingEvent, 'keyCode', { value: 229 })
    input.dispatchEvent(composingEvent)

    expect(onCreateTask).not.toHaveBeenCalled()

    // A plain (non-composing) Enter still submits.
    await user.keyboard('{Enter}')
    expect(onCreateTask).toHaveBeenCalledTimes(1)
  })
})
