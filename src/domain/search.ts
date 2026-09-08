import { pinyin } from 'pinyin-pro'

type SearchIndex = { source: string; text: string; pinyin: string; initials: string }
const indexCache = new Map<string, SearchIndex>()

/** Full-width/case/spacing normalization shared by every picker and list. */
export function normalizeSearchText(value: string): string {
  return value.normalize('NFKC').toLocaleLowerCase().replace(/\s+/g, '')
}

function indexFor(id: string, fields: string[]): SearchIndex {
  const source = fields.join('\u0000')
  const cached = indexCache.get(id)
  if (cached?.source === source) return cached
  const text = normalizeSearchText(fields.join(' '))
  const syllables = pinyin(text, { toneType: 'none', type: 'array' })
  const entry: SearchIndex = {
    source,
    text,
    pinyin: normalizeSearchText(syllables.join('')),
    initials: normalizeSearchText(syllables.map(item => item[0] ?? '').join('')),
  }
  indexCache.set(id, entry)
  return entry
}

function score(index: SearchIndex, query: string): number | null {
  if (index.text === query) return 0
  if (index.pinyin === query || index.initials === query) return 1
  if (index.text.startsWith(query)) return 10
  if (index.pinyin.startsWith(query) || index.initials.startsWith(query)) return 11
  if (index.text.includes(query)) return 20
  if (index.pinyin.includes(query) || index.initials.includes(query)) return 21
  return null
}

/**
 * Stable fuzzy search: exact > prefix > contains, with Chinese/pinyin/
 * initials matching. The cached index is rebuilt automatically when an
 * entity's searchable source changes (for example after a rename).
 */
export function searchEntities<T>(
  items: readonly T[],
  rawQuery: string,
  getFields: (item: T) => readonly string[],
  getId: (item: T, index: number) => string = (_item, index) => String(index),
): T[] {
  const query = normalizeSearchText(rawQuery)
  if (!query) return [...items]
  return items
    .map((item, originalIndex) => {
      const fields = getFields(item).map(value => String(value))
      return { item, originalIndex, rank: score(indexFor(getId(item, originalIndex), fields), query) }
    })
    .filter((entry): entry is { item: T; originalIndex: number; rank: number } => entry.rank !== null)
    .sort((a, b) => a.rank - b.rank || a.originalIndex - b.originalIndex)
    .map(entry => entry.item)
}

export function invalidateSearchIndex(id: string): void {
  indexCache.delete(id)
}

export function clearSearchIndex(): void {
  indexCache.clear()
}
