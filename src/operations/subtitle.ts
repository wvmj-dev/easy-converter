// Subtitle operations offered by the interface. The backend validates everything again
// (`ops/subtitle.rs`).
import type { FileInfo } from '../types/FileInfo'
import type { JobRequest } from '../types/JobRequest'
import type { SubtitleFormat } from '../types/SubtitleFormat'
import type { Problem } from './image'
import { parseTime } from './media'
import { extensionOf } from './office'

export const SUBTITLE_OPERATIONS = ['convert', 'shift'] as const
export type SubtitleOperation = (typeof SUBTITLE_OPERATIONS)[number]

export const SUBTITLE_FORMATS: SubtitleFormat[] = ['srt', 'vtt', 'ass']
/** Extensions offered when choosing a subtitle file for a video. */
export const SUBTITLE_EXTENSIONS = ['srt', 'vtt', 'ass', 'ssa']
/** ISO 639-2 codes offered for an embedded subtitle. */
export const SUBTITLE_LANGUAGES = ['por', 'eng', 'spa', 'fra', 'deu', 'ita', 'jpn'] as const
export const MAX_OFFSET_MS = 24 * 3600 * 1000

export interface SubtitleOptions {
  convert: { format: SubtitleFormat }
  shift: { offset: string; direction: 'later' | 'earlier' }
}

export function defaultSubtitleOptions(): SubtitleOptions {
  return { convert: { format: 'srt' }, shift: { offset: '', direction: 'later' } }
}

/** Shift in milliseconds (negative: earlier), or `null` when the text is not a time. */
export function parseOffset(options: SubtitleOptions['shift']): number | null {
  const ms = parseTime(options.offset)
  if (ms === null || ms === 0 || ms > MAX_OFFSET_MS) return null
  return options.direction === 'earlier' ? -ms : ms
}

function formatOf(file: FileInfo): SubtitleFormat | null {
  const ext = extensionOf(file)
  if (ext === 'ssa') return 'ass'
  return SUBTITLE_FORMATS.find((f) => f === ext) ?? null
}

export function unavailableReason(files: FileInfo[]): Problem | null {
  return files.length === 0 ? { key: 'subtitle.unavailable.noFiles' } : null
}

export function validateOptions(operation: SubtitleOperation, options: SubtitleOptions, files: FileInfo[]): Problem | null {
  if (operation === 'shift') {
    return parseOffset(options.shift) === null ? { key: 'subtitle.shift.invalid' } : null
  }
  const same = files.find((f) => formatOf(f) === options.convert.format)
  return same
    ? { key: 'subtitle.convert.same', params: { name: same.name, format: options.convert.format.toUpperCase() } }
    : null
}

export function buildSubtitleRequests(
  operation: SubtitleOperation,
  files: FileInfo[],
  options: SubtitleOptions,
): JobRequest[] {
  return files.map((file): JobRequest =>
    operation === 'convert'
      ? { type: 'subtitleConvert', input: file.path, format: options.convert.format }
      : { type: 'subtitleShift', input: file.path, offsetMs: parseOffset(options.shift) ?? 0 },
  )
}
