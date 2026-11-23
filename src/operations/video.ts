// Video operations offered by the interface: default options, availability, a light
// validation for immediate feedback and the requests sent to the backend. The backend
// validates everything again; the limits below only mirror `ops/video.rs`.
import type { AudioFormat } from '../types/AudioFormat'
import type { FileInfo } from '../types/FileInfo'
import type { JobRequest } from '../types/JobRequest'
import type { MediaQuality } from '../types/MediaQuality'
import type { VideoContainer } from '../types/VideoContainer'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'
import { isIntIn, minDuration, missingMediaTool, parseRange, rangeLength, type RangeText } from './media'

export const VIDEO_LIMITS = {
  maxMergeInputs: 50,
  minHeight: 16,
  maxHeight: 4320,
  maxGifFps: 30,
  minGifWidth: 32,
  maxGifWidth: 1280,
  maxGifMs: 120_000,
} as const

export const VIDEO_OPERATIONS = [
  'convert',
  'compress',
  'resize',
  'trim',
  'extractAudio',
  'toGif',
  'merge',
  'audioTrack',
  'embedSubtitle',
] as const
export type VideoOperation = (typeof VIDEO_OPERATIONS)[number]

export const VIDEO_CONTAINERS: VideoContainer[] = ['mp4', 'mkv', 'webm', 'mov', 'avi']
export const MEDIA_QUALITIES: MediaQuality[] = ['high', 'medium', 'low']
export const VIDEO_HEIGHTS = [2160, 1440, 1080, 720, 480, 360, 240] as const
export const EXTRACT_FORMATS: ('original' | AudioFormat)[] = ['original', 'mp3', 'm4a', 'opus', 'flac', 'wav']

/** Video encoders the backend can use for each container (`ops/media.rs`). */
const CONTAINER_ENCODERS: Record<VideoContainer, string[]> = {
  mp4: ['libx264', 'libopenh264', 'mpeg4'],
  mov: ['libx264', 'libopenh264', 'mpeg4'],
  mkv: ['libx264', 'libopenh264', 'libvpx-vp9', 'mpeg4'],
  webm: ['libvpx-vp9', 'libvpx'],
  avi: ['mpeg4', 'libx264', 'libopenh264'],
}

export interface VideoOptions {
  convert: { container: VideoContainer }
  compress: { quality: MediaQuality }
  resize: { height: number }
  trim: RangeText & { accurate: boolean }
  extractAudio: { format: 'original' | AudioFormat }
  toGif: RangeText & { fps: number; width: number }
  audioTrack: { mode: 'remove' | 'replace'; audio: string }
  embedSubtitle: { subtitle: string; language: string }
}

export function defaultVideoOptions(): VideoOptions {
  return {
    convert: { container: 'mp4' },
    compress: { quality: 'medium' },
    resize: { height: 720 },
    trim: { start: '', end: '', accurate: false },
    extractAudio: { format: 'original' },
    toGif: { start: '', end: '', fps: 10, width: 480 },
    audioTrack: { mode: 'remove', audio: '' },
    embedSubtitle: { subtitle: '', language: '' },
  }
}

/** Why an operation cannot be used with these files, or `null` when it can. */
export function unavailableReason(
  operation: VideoOperation,
  files: FileInfo[],
  hasTool: (id: ToolId) => boolean,
): Problem | null {
  if (files.length === 0) return { key: 'video.unavailable.noFiles' }
  const missing = missingMediaTool(hasTool)
  if (missing) return missing
  if (operation === 'merge') {
    if (files.length < 2) return { key: 'video.unavailable.needsTwo' }
    if (files.length > VIDEO_LIMITS.maxMergeInputs) {
      return { key: 'video.unavailable.tooMany', params: { max: VIDEO_LIMITS.maxMergeInputs } }
    }
  }
  if (operation === 'extractAudio') {
    const silent = files.find((f) => f.media && f.media.audio.length === 0)
    if (silent) return { key: 'video.unavailable.noAudio', params: { name: silent.name } }
  }
  return null
}

/** First problem with the options, or `null` when they look valid. */
export function validateOptions(
  operation: VideoOperation,
  options: VideoOptions,
  files: FileInfo[],
  encoders: string[] | null,
): Problem | null {
  switch (operation) {
    case 'convert': {
      const container = options.convert.container
      if (encoders && !CONTAINER_ENCODERS[container].some((e) => encoders.includes(e))) {
        return { key: 'video.convert.noEncoder', params: { format: container.toUpperCase() } }
      }
      return null
    }
    case 'resize':
      return isIntIn(options.resize.height, VIDEO_LIMITS.minHeight, VIDEO_LIMITS.maxHeight) &&
        options.resize.height % 2 === 0
        ? null
        : { key: 'video.resize.invalid', params: { min: VIDEO_LIMITS.minHeight, max: VIDEO_LIMITS.maxHeight } }
    case 'trim': {
      const parsed = parseRange(options.trim, minDuration(files))
      if (!parsed.ok) return parsed.problem
      return options.trim.start.trim() === '' && options.trim.end.trim() === ''
        ? { key: 'video.trim.empty' }
        : null
    }
    case 'toGif': {
      const o = options.toGif
      if (!isIntIn(o.fps, 1, VIDEO_LIMITS.maxGifFps)) {
        return { key: 'video.toGif.invalidFps', params: { max: VIDEO_LIMITS.maxGifFps } }
      }
      if (!isIntIn(o.width, VIDEO_LIMITS.minGifWidth, VIDEO_LIMITS.maxGifWidth)) {
        return {
          key: 'video.toGif.invalidWidth',
          params: { min: VIDEO_LIMITS.minGifWidth, max: VIDEO_LIMITS.maxGifWidth },
        }
      }
      const parsed = parseRange(o, minDuration(files))
      if (!parsed.ok) return parsed.problem
      const durations = files.map((f) => f.media?.durationMs ?? null)
      const longest = Math.max(...durations.map((d) => rangeLength(parsed.range, d) ?? 0))
      return longest > VIDEO_LIMITS.maxGifMs
        ? { key: 'video.toGif.tooLong', params: { max: VIDEO_LIMITS.maxGifMs / 1000 } }
        : null
    }
    case 'audioTrack':
      return options.audioTrack.mode === 'replace' && options.audioTrack.audio === ''
        ? { key: 'video.audioTrack.chooseAudio' }
        : null
    case 'embedSubtitle':
      return options.embedSubtitle.subtitle === '' ? { key: 'video.embedSubtitle.chooseSubtitle' } : null
    case 'compress':
    case 'extractAudio':
    case 'merge':
      return null
  }
}

function range(text: RangeText) {
  const parsed = parseRange(text, null)
  return parsed.ok ? parsed.range : { startMs: 0 }
}

/** Requests for the backend: one job per file, or a single job when joining. */
export function buildVideoRequests(
  operation: VideoOperation,
  files: FileInfo[],
  options: VideoOptions,
): JobRequest[] {
  const each = (make: (input: string) => JobRequest) => files.map((file) => make(file.path))
  switch (operation) {
    case 'convert':
      return each((input) => ({ type: 'videoConvert', input, container: options.convert.container }))
    case 'compress':
      return each((input) => ({ type: 'videoCompress', input, quality: options.compress.quality }))
    case 'resize':
      return each((input) => ({ type: 'videoResize', input, height: options.resize.height }))
    case 'trim':
      return each((input) => ({
        type: 'videoTrim',
        input,
        range: range(options.trim),
        accurate: options.trim.accurate,
      }))
    case 'extractAudio': {
      const format = options.extractAudio.format
      return each((input) => ({ type: 'videoExtractAudio', input, ...(format !== 'original' ? { format } : {}) }))
    }
    case 'toGif': {
      const o = options.toGif
      const whole = o.start.trim() === '' && o.end.trim() === ''
      return each((input) => ({
        type: 'videoToGif',
        input,
        fps: o.fps,
        width: o.width,
        ...(whole ? {} : { range: range(o) }),
      }))
    }
    case 'merge':
      return [{ type: 'videoMerge', inputs: files.map((f) => f.path) }]
    case 'audioTrack': {
      const o = options.audioTrack
      const action = o.mode === 'replace' ? { mode: 'replace' as const, audio: o.audio } : { mode: 'remove' as const }
      return each((input) => ({ type: 'videoAudioTrack', input, action }))
    }
    case 'embedSubtitle': {
      const { subtitle, language } = options.embedSubtitle
      return each((input) => ({
        type: 'videoEmbedSubtitle',
        input,
        subtitle,
        ...(language !== '' ? { language } : {}),
      }))
    }
  }
}
