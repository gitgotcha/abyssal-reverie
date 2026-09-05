import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'

import App from '../../../App'
import { GatewayProvider } from '../../../services/gatewayContext'
import { FakeAppGateway } from '../../../test/FakeAppGateway'
import type { TimerSession } from '../../../domain/models'

afterEach(() => cleanup())

function renderWithGateway(gateway: FakeAppGateway) {
  return render(
    <GatewayProvider gateway={gateway}>
      <App />
    </GatewayProvider>,
  )
}

async function createTask(gateway: FakeAppGateway, user: ReturnType<typeof userEvent.setup>, title: string) {
  await user.click(screen.getByRole('button', { name: '任务' }))
  const input = await screen.findByPlaceholderText('添加任务…')
  await user.type(input, title)
  await user.keyboard('{Enter}')
  await screen.findByText(title)
  await user.click(screen.getByRole('button', { name: '专注' }))
}

describe('v1.1.2 stage B: task switching', () => {
  it('switching tasks while running closes A with its time and starts B', async () => {
    const gateway = new FakeAppGateway()
    const user = userEvent.setup()
    renderWithGateway(gateway)

    await createTask(gateway, user, '任务 A')
    await createTask(gateway, user, '任务 B')

    // Select A (idle → pending selection), start the round.
    await user.click(screen.getByRole('button', { name: /任务 A/ }))
    await user.click(screen.getByRole('button', { name: '开始专注' }))
    await screen.findByText('专注中')

    // Click B while running → confirm dialog → switch.
    await user.click(screen.getByRole('button', { name: /任务 B/ }))
    const dialog = await screen.findByRole('alertdialog')
    expect(dialog.textContent).toContain('切换到「任务 B」')
    await user.click(screen.getByRole('button', { name: '切换任务' }))

    // The backend snapshot now owns task B; A's session was closed.
    await waitFor(() => {
      expect(gateway.currentTimer.selectedTaskId).toBe(gateway.currentTasks.find(t => t.title === '任务 B')?.id)
    })
    const all = await gateway.listSessions({ scope: 'all' })
    const closedA = all.find((s: TimerSession) => s.taskTitleSnapshot === '任务 A')
    expect(closedA).toBeDefined()
    expect(closedA?.status).toBe('completed')
    expect(closedA?.finishReason).toBe('manual_finish')
  })

  it('a short switch-closed session stays out of the activity view', async () => {
    const gateway = new FakeAppGateway()
    const user = userEvent.setup()
    renderWithGateway(gateway)

    await createTask(gateway, user, '任务 A')
    await createTask(gateway, user, '任务 B')
    await user.click(screen.getByRole('button', { name: /任务 A/ }))
    await user.click(screen.getByRole('button', { name: '开始专注' }))
    await screen.findByText('专注中')
    await user.click(screen.getByRole('button', { name: /任务 B/ }))
    await user.click(await screen.findByRole('button', { name: '切换任务' }))

    // An instant switch focuses ~0s → below the 30s rule → hidden.
    const visible = await gateway.listSessions({ limit: 50 })
    expect(visible).toHaveLength(0)
    const activity = screen.getByText('专注航线').closest('aside')
    expect(activity).not.toBeNull()
    expect(activity!.textContent).not.toContain('任务 A')
  })
})

describe('v1.1.2 stage C: log consistency', () => {
  it('an expiry reaching the app twice still appends exactly one log row', async () => {
    const gateway = new FakeAppGateway()
    const user = userEvent.setup()
    renderWithGateway(gateway)

    await createTask(gateway, user, '双入口')
    await user.click(screen.getByRole('button', { name: /双入口/ }))
    await user.click(screen.getByRole('button', { name: '开始专注' }))
    await screen.findByText('专注中')

    // The UI tick and the Rust backstop both reach for the same session.
    gateway.emitTimerExpired()
    gateway.emitTimerExpired()
    const activity = () => screen.getByText('专注航线').closest('aside')!
    await waitFor(() => {
      expect(activity().textContent).toContain('双入口')
    })

    const sessions = await gateway.listSessions({ limit: 50 })
    expect(sessions).toHaveLength(1)
    // Exactly one row for that session inside the activity panel (the task
    // itself still shows in the timer's task selector).
    expect((activity().textContent.match(/双入口/g) ?? []).length).toBe(1)
  })

  it('reloaded logs keep the newest record at the same position as appended ones', async () => {
    const gateway = new FakeAppGateway()
    const now = Date.now()
    // Seed history: an older and a newer eligible focus session.
    const older: TimerSession = {
      id: 'seed-old', taskId: null, taskTitleSnapshot: '较早记录', projectSnapshot: '通用',
      mode: 'focus', status: 'completed', plannedSeconds: 1500, focusedSeconds: 1500,
      startedAt: now - 3_600_000, endedAt: now - 3_000_000, finishReason: 'elapsed',
      statisticsEligible: true, qualificationReason: 'qualified',
    }
    const newer: TimerSession = {
      id: 'seed-new', taskId: null, taskTitleSnapshot: '较新记录', projectSnapshot: '通用',
      mode: 'focus', status: 'completed', plannedSeconds: 1500, focusedSeconds: 1200,
      startedAt: now - 600_000, endedAt: now - 300_000, finishReason: 'elapsed',
      statisticsEligible: true, qualificationReason: 'qualified',
    }
    gateway.seedSession(older)
    gateway.seedSession(newer)

    renderWithGateway(gateway)
    // The right-hand activity panel ("专注航线") shows oldest-at-top,
    // newest-at-bottom — the same visual order an in-session append must
    // produce (head-insert into the newest-first array).
    const olderEl = await screen.findByText('较早记录')
    const newerEl = await screen.findByText('较新记录')
    expect(olderEl.compareDocumentPosition(newerEl) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })
})
