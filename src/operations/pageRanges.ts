// Page selections typed by the user, such as "1-3, 5, 9-7". A range written backwards
// selects its pages in reverse order. The backend validates the result again
// (`ops/pdf/pages.rs`); the limits here mirror it.
import type { PageRange } from '../types/PageRange'
import type { Problem } from './image'

export const PAGE_LIMITS = { maxPage: 100_000, maxRanges: 1_000 } as const

export type ParsedPages = { ok: true; ranges: PageRange[] } | { ok: false; problem: Problem }

/**
 * Parses a page selection. `pageCount`, when known, rejects pages past the end.
 */
export function parsePages(text: string, pageCount?: number | null): ParsedPages {
  const parts = text
    .split(/[,;]/)
    .map((part) => part.trim())
    .filter((part) => part !== '')
  if (parts.length === 0) {
    return { ok: false, problem: { key: 'pdf.pages.empty' } }
  }
  if (parts.length > PAGE_LIMITS.maxRanges) {
    return { ok: false, problem: { key: 'pdf.pages.tooMany', params: { max: PAGE_LIMITS.maxRanges } } }
  }
  const max = pageCount ?? PAGE_LIMITS.maxPage
  const ranges: PageRange[] = []
  for (const part of parts) {
    const match = /^(\d{1,6})(?:\s*[-–]\s*(\d{1,6}))?$/.exec(part)
    if (!match) {
      return { ok: false, problem: { key: 'pdf.pages.invalid', params: { part } } }
    }
    const start = Number(match[1])
    const end = match[2] === undefined ? start : Number(match[2])
    for (const page of [start, end]) {
      if (page < 1 || page > max) {
        return pageCount
          ? { ok: false, problem: { key: 'pdf.pages.pastEnd', params: { page, count: pageCount } } }
          : { ok: false, problem: { key: 'pdf.pages.invalid', params: { part } } }
      }
    }
    ranges.push({ start, end })
  }
  return { ok: true, ranges }
}

/** Number of pages a selection produces. */
export function countPages(ranges: PageRange[]): number {
  return ranges.reduce((sum, r) => sum + Math.abs(r.end - r.start) + 1, 0)
}
