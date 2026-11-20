// Shared pieces of the video and audio screens: times typed by the user ("1:23.5"),
// durations for display and time ranges. The backend validates everything again
// (`ops/media.rs`); the limits here mirror it.
import type { FileInfo } from '../types/FileInfo'
import type { TimeRange } from '../types/TimeRange'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'

/** Longest duration accepted, as in `tools/ffmpeg.rs` (100 hours). */
export const MAX_DURATION_MS = 100 * 3600 * 1000

export interface RangeText {
  start: string
  end: string
}

/**
 * Parses a time: seconds ("83", "83.5"), "m:ss" or "h:mm:ss", with optional decimals.
 * Returns milliseconds, or `null` when the text is not a valid time.
 */
export function parseTime(text: string): number | null {
  const trimmed = text.trim().replace(',', '.')
  const match = /^(?:(\d{1,3}):)?(?:(\d{1,3}):)?(\d{1,6})(?:\.(\d{1,3}))?$/.exec(trimmed)
  if (!match) return null
  const [, a, b, s, frac] = match
  // With two colons the first group is hours; with one it is minutes.
  const hours = b !== undefined ? Number(a) : 0
  const minutes = b !== undefined ? Number(b) : Number(a ?? 0)
  const seconds = Number(s)
  if ((a !== undefined && seconds >= 60) || (b !== undefined && minutes >= 60)) return null
  const ms = ((hours * 60 + minutes) * 60 + seconds) * 1000 + Number((frac ?? '').padEnd(3, '0'))
  return ms <= MAX_DURATION_MS ? ms : null
}

/** A duration as "m:ss" or "h:mm:ss", with tenths when they matter. */
export function formatDuration(ms: number): string {
  const totalTenths = Math.round(ms / 100)
  const tenths = totalTenths % 10
  const totalSeconds = Math.floor(totalTenths / 10)
  const s = totalSeconds % 60
  const m = Math.floor(totalSeconds / 60) % 60
  const h = Math.floor(totalSeconds / 3600)
  const pad = (n: number) => String(n).padStart(2, '0')
  const base = h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`
  return tenths > 0 ? `${base}.${tenths}` : base
}

/** Shortest known duration among the files, used to check times. */
export function minDuration(files: FileInfo[]): number | null {
  const known = files.map((f) => f.media?.durationMs).filter((n): n is number => typeof n === 'number')
  return known.length > 0 ? Math.min(...known) : null
}

export type ParsedRange = { ok: true; range: TimeRange } | { ok: false; problem: Problem }

/** Parses a start and an optional end. An empty start means the beginning. */
export function parseRange(text: RangeText, durationMs: number | null): ParsedRange {
  const startMs = text.start.trim() === '' ? 0 : parseTime(text.start)
  if (startMs === null) return { ok: false, problem: { key: 'media.time.invalid', params: { value: text.start } } }
  let endMs: number | undefined
  if (text.end.trim() !== '') {
    const end = parseTime(text.end)
    if (end === null) return { ok: false, problem: { key: 'media.time.invalid', params: { value: text.end } } }
    if (end <= startMs) return { ok: false, problem: { key: 'media.time.order' } }
    endMs = end
  }
  if (durationMs !== null && startMs >= durationMs) {
    return { ok: false, problem: { key: 'media.time.pastEnd', params: { duration: formatDuration(durationMs) } } }
  }
  return { ok: true, range: endMs === undefined ? { startMs } : { startMs, endMs } }
}

/** Length of a range in a file of `durationMs`, when it can be known. */
export function rangeLength(range: TimeRange, durationMs: number | null): number | null {
  const end = range.endMs ?? durationMs
  if (end === null || end === undefined) return null
  return Math.min(end, durationMs ?? end) - range.startMs
}

/** FFmpeg and ffprobe are needed by every video and audio operation. */
export function missingMediaTool(hasTool: (id: ToolId) => boolean): Problem | null {
  const missing = (['ffmpeg', 'ffprobe'] as ToolId[]).find((id) => !hasTool(id))
  return missing ? { key: 'media.missingTool', params: { tool: missing === 'ffmpeg' ? 'FFmpeg' : 'ffprobe' } } : null
}

export function isIntIn(value: unknown, min: number, max: number): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= min && value <= max
}
