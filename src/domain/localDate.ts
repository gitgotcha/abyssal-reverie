/** Calendar helpers for YYYY-MM-DD values. They intentionally use local
 * calendar fields and never parse an ISO date as a UTC instant. */

const DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/

function parts(value: string): [number, number, number] | null {
  const match = DATE_RE.exec(value)
  if (!match) return null
  return [Number(match[1]), Number(match[2]), Number(match[3])]
}

export function isValidLocalDate(value: string): boolean {
  const parsed = parts(value)
  if (!parsed) return false
  const [year, month, day] = parsed
  if (month < 1 || month > 12 || day < 1) return false
  const last = new Date(year, month, 0).getDate()
  return day <= last
}

export function addLocalDays(value: string, days: number): string {
  if (!isValidLocalDate(value) || !Number.isInteger(days)) {
    throw new Error('invalid local date')
  }
  const [year, month, day] = parts(value)!
  const result = new Date(year, month - 1, day)
  result.setDate(result.getDate() + days)
  return todayLocalDate(result)
}

export function todayLocalDate(now = new Date()): string {
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
}
