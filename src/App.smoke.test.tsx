import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'

import App from './App'
import { GatewayProvider } from './services/gatewayContext'
import { FakeAppGateway } from './test/FakeAppGateway'

afterEach(() => cleanup())

function renderWithGateway(gateway: FakeAppGateway) {
  return render(
    <GatewayProvider gateway={gateway}>
      <App />
    </GatewayProvider>,
  )
}

describe('Abyssal Reverie', () => {
  it('renders the timer and main navigation', async () => {
    const gateway = new FakeAppGateway()
    renderWithGateway(gateway)

    // bootstrap resolves asynchronously; the timer chrome is present immediately.
    expect(await screen.findByText((_, el) => el?.textContent === '25:00')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '开始专注' })).toBeInTheDocument()
    expect(screen.getByText('今日概览')).toBeInTheDocument()
  })

  it('persists a created task through the gateway', async () => {
    const gateway = new FakeAppGateway()
    const user = userEvent.setup()
    renderWithGateway(gateway)

    // Navigate to the Tasks panel (exact match avoids hitting "删除任务").
    await user.click(screen.getByRole('button', { name: '任务' }))

    const input = await screen.findByPlaceholderText('添加任务…')
    await user.type(input, '编写集成测试')
    await user.keyboard('{Enter}')

    // The new task card appears in the active list and is seeded into the fake gateway.
    await waitFor(() => {
      expect(screen.getByText('编写集成测试')).toBeInTheDocument()
    })
    const payload = await gateway.bootstrap()
    expect(payload.tasks.some((t) => t.title === '编写集成测试')).toBe(true)
  })

  it('archives a task through the gateway (soft delete, v1.2 D-7)', async () => {
    const gateway = new FakeAppGateway()
    const user = userEvent.setup()
    renderWithGateway(gateway)

    // Create a task, then archive it via its row archive button.
    await user.click(screen.getByRole('button', { name: '任务' }))
    const input = await screen.findByPlaceholderText('添加任务…')
    await user.type(input, '临时任务')
    await user.keyboard('{Enter}')

    await screen.findByText('临时任务')
    const archiveButton = screen.getByRole('button', { name: '归档任务' })
    await user.click(archiveButton)

    // The default 待办 filter hides the archived task.
    await waitFor(() => {
      expect(screen.queryByText('临时任务')).not.toBeInTheDocument()
    })
    // Soft delete: the record survives with status archived.
    const payload = await gateway.bootstrap()
    const archived = payload.tasks.find((t) => t.title === '临时任务')
    expect(archived).toBeDefined()
    expect(archived?.status).toBe('archived')
  })
})
