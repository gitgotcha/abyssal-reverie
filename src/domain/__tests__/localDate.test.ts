import { describe, expect, it } from 'vitest'
import { addLocalDays, isValidLocalDate, todayLocalDate } from '../localDate'

describe('local date helpers', () => {
  it('validates calendar dates without UTC parsing', () => {
    expect(isValidLocalDate('2028-02-29')).toBe(true)
    expect(isValidLocalDate('2027-02-29')).toBe(false)
    expect(isValidLocalDate('2026-2-03')).toBe(false)
    expect(isValidLocalDate('2026-04-31')).toBe(false)
  })

  it('adds days across month and year boundaries', () => {
    expect(addLocalDays('2026-12-28', 7)).toBe('2027-01-04')
    expect(addLocalDays('2028-02-29', 1)).toBe('2028-03-01')
  })

  it('formats today from local calendar fields', () => {
    const value = todayLocalDate(new Date(2026, 8, 8, 23, 59, 0))
    expect(value).toBe('2026-09-08')
  })
})
