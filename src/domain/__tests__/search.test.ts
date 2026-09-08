import { describe, expect, it } from 'vitest'
import { searchEntities } from '../search'

const entries = [
  { id: 'exact', name: '论文' },
  { id: 'prefix', name: '论文写作' },
  { id: 'contains', name: '阅读论文方法' },
  { id: 'other', name: '日常整理' },
]

describe('fuzzy entity search', () => {
  it('matches Chinese, pinyin and initials', () => {
    expect(searchEntities(entries, '论文', item => [item.name]).map(item => item.id)).toEqual([
      'exact', 'prefix', 'contains',
    ])
    expect(searchEntities(entries, 'lunwen', item => [item.name]).map(item => item.id)).toEqual([
      'exact', 'prefix', 'contains',
    ])
    expect(searchEntities(entries, 'lw', item => [item.name]).map(item => item.id)).toEqual([
      'exact', 'prefix', 'contains',
    ])
  })

  it('normalizes case/full-width input and keeps stable ties', () => {
    expect(searchEntities(entries, 'ＬＵＮＷＥＮ', item => [item.name]).map(item => item.id)).toEqual([
      'exact', 'prefix', 'contains',
    ])
    const tied = [{ id: 'a', name: '甲' }, { id: 'b', name: '架' }]
    expect(searchEntities(tied, 'j', item => [item.name]).map(item => item.id)).toEqual(['a', 'b'])
  })
})
