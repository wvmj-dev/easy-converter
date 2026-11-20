import { describe, expect, it } from 'vitest'
import { countPages, parsePages } from './pageRanges'

describe('parsePages', () => {
  it('reads single pages, ranges and reversed ranges', () => {
    expect(parsePages(' 1-3, 5 ;9 – 7 ')).toEqual({
      ok: true,
      ranges: [
        { start: 1, end: 3 },
        { start: 5, end: 5 },
        { start: 9, end: 7 },
      ],
    })
  })

  it('explains what is wrong', () => {
    expect(parsePages('')).toEqual({ ok: false, problem: { key: 'pdf.pages.empty' } })
    expect(parsePages('1-a')).toMatchObject({ ok: false, problem: { key: 'pdf.pages.invalid' } })
    expect(parsePages('0')).toMatchObject({ ok: false, problem: { key: 'pdf.pages.invalid' } })
    expect(parsePages('2-12', 10)).toEqual({
      ok: false,
      problem: { key: 'pdf.pages.pastEnd', params: { page: 12, count: 10 } },
    })
  })

  it('counts the selected pages', () => {
    const parsed = parsePages('1-3, 9-7, 5')
    expect(parsed.ok && countPages(parsed.ranges)).toBe(7)
  })
})
