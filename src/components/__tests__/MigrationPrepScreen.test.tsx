import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { MigrationPrepScreen } from '../MigrationPrepScreen'
import type { AppGateway } from '../../services/appGateway'
import type { MigrationPreview } from '../../domain/models'

afterEach(() => cleanup())

function previewOf(kind: 'legacySemantic' | 'nullableMetadata'): MigrationPreview {
  return {
    migrationKind: kind,
    schemaVersion: kind === 'nullableMetadata' ? 4 : 3,
    taskCount: 3,
    sessionCount: 2,
    projectsToCreate: kind === 'legacySemantic' ? ['Java 面试'] : [],
    generalTaskCount: 0,
    suggestedFocusMinutes: 25,
  }
}

function gatewayWith(preview: MigrationPreview) {
  return {
    previewMigration: vi.fn(async () => preview),
    confirmMigration: vi.fn(async () => {
      throw new Error('confirm must not fire during a preview test')
    }),
    cancelUpgrade: vi.fn(),
  } as unknown as AppGateway
}

// 阶段 A 缺口 3（Codex 复审）：迁移界面按 migrationKind 呈现两种协议——
// v1–v3 显示预算基准与项目映射决策；v4 隐藏二者并明确不改写旧数据。
describe('MigrationPrepScreen — versioned preview (task 1.4)', () => {
  it('legacySemantic shows the budget basis and project mapping options', async () => {
    render(
      <MigrationPrepScreen
        gateway={gatewayWith(previewOf('legacySemantic'))}
        onMigrated={() => undefined}
      />,
    )
    expect(await screen.findByText(/预算基准/)).toBeTruthy()
    expect(screen.getByText(/Java 面试/)).toBeTruthy()
    expect(screen.getByText(/创建正式项目「通用」/)).toBeTruthy()
  })

  it('nullableMetadata hides budget/mapping inputs and states no rewrite of old data', async () => {
    render(
      <MigrationPrepScreen
        gateway={gatewayWith(previewOf('nullableMetadata'))}
        onMigrated={() => undefined}
      />,
    )
    await screen.findByText(/不会改写旧任务、预算或历史记录/)
    expect(screen.queryByText(/预算基准/)).toBeNull()
    expect(screen.queryByText(/创建正式项目「通用」/)).toBeNull()
    // 只读预览仍展示数据规模。
    expect(screen.getByText(/任务：/)).toBeTruthy()
  })

  it('preview stays read-only — confirm never fires while rendering', async () => {
    const gateway = gatewayWith(previewOf('legacySemantic'))
    render(
      <MigrationPrepScreen gateway={gateway} onMigrated={() => undefined} />,
    )
    await screen.findByText(/预算基准/)
    expect(gateway.previewMigration).toHaveBeenCalledTimes(1)
    expect(gateway.confirmMigration).not.toHaveBeenCalled()
  })
})
