import { afterEach, describe, expect, it } from 'vitest'

const REAL_TZ = process.env.TZ

afterEach(() => {
  process.env.TZ = REAL_TZ
})

/**
 * v1.1.2 D2: the day boundary must be the NEXT local midnight computed by
 * the calendar, not `from + 86_400_000`. On DST days that fixed offset
 * slides the boundary by an hour (spring: 23h days end at 01:00 next day;
 * autumn: 25h days end at 23:00 the same day).
 */
describe('dayBoundary with DST transitions (America/New_York)', () => {
  it('spring-forward day 2026-03-08 covers exactly 23 hours', async () => {
    process.env.TZ = 'America/New_York'
    const { dayBoundary } = await import('../statistics')
    const b = dayBoundary(new Date(2026, 2, 8, 12, 0, 0))
    expect(b.date).toBe('2026-03-08')
    expect(b.to - b.from).toBe(23 * 3_600_000)
    // The boundary is midnight of the next calendar day.
    expect(new Date(b.to).getHours()).toBe(0)
    expect(new Date(b.to).getDate()).toBe(9)
  })

  it('fall-back day 2026-11-01 covers exactly 25 hours', async () => {
    process.env.TZ = 'America/New_York'
    const { dayBoundary } = await import('../statistics')
    const b = dayBoundary(new Date(2026, 10, 1, 12, 0, 0))
    expect(b.date).toBe('2026-11-01')
    expect(b.to - b.from).toBe(25 * 3_600_000)
    expect(new Date(b.to).getDate()).toBe(2)
  })

  it('a normal day in a non-DST zone stays 24 hours', async () => {
    process.env.TZ = 'Asia/Shanghai'
    const { dayBoundary } = await import('../statistics')
    const b = dayBoundary(new Date(2026, 8, 6, 12, 0, 0))
    expect(b.date).toBe('2026-09-06')
    expect(b.to - b.from).toBe(24 * 3_600_000)
  })

  it('weekBoundaries produce seven contiguous half-open days', async () => {
    process.env.TZ = 'America/New_York'
    const { weekBoundaries } = await import('../statistics')
    // Wed Oct 28, 2026 — its week (Mon Oct 26 .. Sun Nov 1) contains the
    // fall-back transition on Sunday Nov 1, so the week spans 169 hours.
    const days = weekBoundaries(new Date(2026, 9, 28, 15, 0, 0))
    expect(days).toHaveLength(7)
    expect(days[0].date).toBe('2026-10-26')
    for (let i = 1; i < days.length; i += 1) {
      expect(days[i].from).toBe(days[i - 1].to)
    }
    expect(days[6].to - days[0].from).toBe(7 * 24 * 3_600_000 + 3_600_000)
  })
})
