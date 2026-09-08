import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ManagementPanel, type ManagementPanelProps } from '../ManagementPanel'

afterEach(() => cleanup())

const props: ManagementPanelProps = {
  tasks: [],
  tags: [],
  categories: [],
  projects: [],
  progress: {},
  focusDurationMinutes: 25,
  onCreateTask: vi.fn(async () => undefined),
  onToggleTask: vi.fn(async () => undefined),
  onArchiveTask: vi.fn(async () => undefined),
  onStartFocus: vi.fn(),
  onCompleteTask: vi.fn(async () => undefined),
  onCyclePriority: vi.fn(async () => undefined),
  onUpdateTask: vi.fn(async () => undefined),
  onApplyTaskRelationship: vi.fn(async () => undefined),
  tagOps: {
    createTag: vi.fn(async () => undefined),
    renameTag: vi.fn(async () => undefined),
    reorderTag: vi.fn(async () => undefined),
    previewDeleteTag: vi.fn(async () => ({ tagId: 'x', affectedTasks: 0 })),
    deleteTag: vi.fn(async () => undefined),
  },
  projectOps: {
    createProject: vi.fn(async () => undefined),
    renameProject: vi.fn(async () => undefined),
    archiveProject: vi.fn(async () => undefined),
    createCategory: vi.fn(async () => undefined),
    renameCategory: vi.fn(async () => undefined),
    archiveCategory: vi.fn(async () => undefined),
    moveProject: vi.fn(async () => undefined),
  },
}

describe('ManagementPanel', () => {
  it('keeps task, project and tag management as sibling views', async () => {
    const user = userEvent.setup()
    render(<ManagementPanel {...props} />)

    expect(screen.getByRole('button', { name: '任务' })).toHaveAttribute('aria-current', 'page')
    await user.click(screen.getByRole('button', { name: '项目' }))
    expect(screen.getByRole('dialog', { name: '管理项目与类别' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: '标签' }))
    expect(screen.getByRole('dialog', { name: '管理标签' })).toBeInTheDocument()
  })
})
