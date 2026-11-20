import { describe, expect, it } from 'vitest'
import { formatDuration, parseRange, parseTime, rangeLength } from './media'

describe('parseTime', () => {
  it('reads seconds, m:ss and h:mm:ss with decimals', () => {
    expect(parseTime('90')).toBe(90_000)
    expect(parseTime('1:30')).toBe(90_000)
    expect(parseTime(' 1:02:03.5 ')).toBe(3_723_500)
    expect(parseTime('0:00,25')).toBe(250)
  })

  it('refuses invalid times', () => {
    for (const text of ['', 'abc', '1:60', '1:60:00', '-5', '1.2.3', '1::2', '999999999']) {
      expect(parseTime(text), text).toBeNull()
    }
  })
})

describe('formatDuration', () => {
  it('formats short and long durations', () => {
    expect(formatDuration(5_000)).toBe('0:05')
    expect(formatDuration(65_400)).toBe('1:05.4')
    expect(formatDuration(3_723_000)).toBe('1:02:03')
  })
})

describe('parseRange', () => {
  it('accepts an empty start and end', () => {
    expect(parseRange({ start: '', end: '' }, null)).toEqual({ ok: true, range: { startMs: 0 } })
    expect(parseRange({ start: '1', end: '2.5' }, 10_000)).toEqual({ ok: true, range: { startMs: 1000, endMs: 2500 } })
  })

  it('reports order, invalid times and a start past the end', () => {
    const problem = (start: string, end: string, duration: number | null) => {
      const parsed = parseRange({ start, end }, duration)
      return parsed.ok ? null : parsed.problem.key
    }
    expect(problem('5', '5', null)).toBe('media.time.order')
    expect(problem('x', '', null)).toBe('media.time.invalid')
    expect(problem('', 'y', null)).toBe('media.time.invalid')
    expect(problem('20', '', 10_000)).toBe('media.time.pastEnd')
  })

  it('measures the length of a range', () => {
    expect(rangeLength({ startMs: 1000 }, 5000)).toBe(4000)
    expect(rangeLength({ startMs: 1000, endMs: 9000 }, 5000)).toBe(4000)
    expect(rangeLength({ startMs: 0 }, null)).toBeNull()
  })
})
