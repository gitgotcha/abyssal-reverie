import { describe, expect, it } from 'vitest'

import { formatFocusedDuration, sortLogsDesc, upsertLogNewestFirst } from '../format'
import type { SessionLog } from '../types'
import type { TimerMode } from '../../../domain/models'

function log(id: string, startedAt: number, task = id): SessionLog {
  return {
    id, startedAt, time: '10:00', duration: 1, task,
    mode: 'focus' as TimerMode, status: 'completed', tag: null,
  }
}

describe('v1.1.2 C1 canonical log order', () => {
  it('sortLogsDesc always puts the newest record at index 0', () => {
    const a = log('a', 1000)
    const b = log('b', 3000)
    const c = log('c', 2000)
    // Whatever ingestion order, the array is newest-first.
    expect(sortLogsDesc([a, b, c]).map(l => l.id)).toEqual(['b', 'c', 'a'])
    expect(sortLogsDesc([c, a, b]).map(l => l.id)).toEqual(['b', 'c', 'a'])
  })

  it('upsertLogNewestFirst dedupes by id and keeps the newest position', () => {
    const first = log('s1', 5000, '第一次')
    const replay = log('s1', 5000, '第一次')
    const newer = log('s2', 9000, '第二次')
    let logs = upsertLogNewestFirst([], first)
    logs = upsertLogNewestFirst(logs, newer)
    logs = upsertLogNewestFirst(logs, replay) // duplicate arrival — replace, not append
    expect(logs.map(l => l.id)).toEqual(['s2', 's1'])
  })
})

describe('v1.1.2 D3 focused-time copy', () => {
  it('spells sub-minute durations in seconds', () => {
    expect(formatFocusedDuration(29)).toBe('29 秒')
    expect(formatFocusedDuration(59)).toBe('59 秒')
  })

  it('spells minute-plus durations in minutes', () => {
    expect(formatFocusedDuration(60)).toBe('1 分钟')
    expect(formatFocusedDuration(125 * 60 + 40)).toBe('126 分钟')
  })
})
