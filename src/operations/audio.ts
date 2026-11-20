// Audio operations offered by the interface. The backend validates everything again;
// the limits below only mirror `ops/audio.rs` and `ops/media.rs`.
import type { AudioFormat } from '../types/AudioFormat'
import type { FileInfo } from '../types/FileInfo'
import type { JobRequest } from '../types/JobRequest'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'
import { minDuration, missingMediaTool, parseRange, type RangeText } from './media'

export const AUDIO_OPERATIONS = ['convert', 'trim', 'normalize', 'mono16k'] as const
export type AudioOperation = (typeof AUDIO_OPERATIONS)[number]

export const AUDIO_FORMATS: AudioFormat[] = ['mp3', 'm4a', 'ogg', 'opus', 'flac', 'wav']
export const LOSSLESS_FORMATS: AudioFormat[] = ['flac', 'wav', 'aiff']
/** `auto` keeps the track as it is when the format holds it, otherwise uses 192 kbit/s. */
export const AUDIO_BITRATES = ['auto', 96, 128, 192, 256, 320] as const
export type AudioBitrate = (typeof AUDIO_BITRATES)[number]

export interface AudioOptions {
  convert: { format: AudioFormat; bitrate: AudioBitrate }
  trim: RangeText
}

export function defaultAudioOptions(): AudioOptions {
  return {
    convert: { format: 'mp3', bitrate: 'auto' },
    trim: { start: '', end: '' },
  }
}

export function unavailableReason(
  files: FileInfo[],
  hasTool: (id: ToolId) => boolean,
): Problem | null {
  if (files.length === 0) return { key: 'audio.unavailable.noFiles' }
  return missingMediaTool(hasTool)
}

export function validateOptions(operation: AudioOperation, options: AudioOptions, files: FileInfo[]): Problem | null {
  if (operation !== 'trim') return null
  const parsed = parseRange(options.trim, minDuration(files))
  if (!parsed.ok) return parsed.problem
  return options.trim.start.trim() === '' && options.trim.end.trim() === '' ? { key: 'video.trim.empty' } : null
}

export function buildAudioRequests(operation: AudioOperation, files: FileInfo[], options: AudioOptions): JobRequest[] {
  return files.map((file): JobRequest => {
    const input = file.path
    switch (operation) {
      case 'convert': {
        const { format, bitrate } = options.convert
        const lossy = !LOSSLESS_FORMATS.includes(format)
        return {
          type: 'audioConvert',
          input,
          format,
          ...(lossy && bitrate !== 'auto' ? { bitrateKbps: bitrate } : {}),
        }
      }
      case 'trim': {
        const parsed = parseRange(options.trim, null)
        return { type: 'audioTrim', input, range: parsed.ok ? parsed.range : { startMs: 0 } }
      }
      case 'normalize':
        return { type: 'audioNormalize', input }
      case 'mono16k':
        return { type: 'audioMono16k', input }
    }
  })
}
