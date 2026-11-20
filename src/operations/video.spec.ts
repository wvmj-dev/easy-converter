import { describe, expect, it } from 'vitest'
import type { FileInfo } from '../types/FileInfo'
import { buildAudioRequests, defaultAudioOptions } from './audio'
import { buildVideoRequests, defaultVideoOptions, unavailableReason, validateOptions } from './video'

function clip(name: string, durationMs: number | null = 10_000, audio = true): FileInfo {
  return {
    path: `/v/${name}`,
    name,
    extension: 'mp4',
    size: 10,
    category: 'video',
    image: null,
    pdf: null,
    media: {
      durationMs,
      video: { codec: 'h264', width: 320, height: 240, fps: 25 },
      audio: audio ? [{ codec: 'aac', sampleRate: 48_000, channels: 2 }] : [],
      subtitleCount: 0,
    },
  }
}

const all = () => true

describe('video availability', () => {
  it('needs FFmpeg, two files to join and audio to extract', () => {
    expect(unavailableReason('convert', [clip('a.mp4')], (id) => id !== 'ffmpeg')?.key).toBe('media.missingTool')
    expect(unavailableReason('merge', [clip('a.mp4')], all)?.key).toBe('video.unavailable.needsTwo')
    expect(unavailableReason('extractAudio', [clip('a.mp4', 1000, false)], all)?.key).toBe('video.unavailable.noAudio')
    expect(unavailableReason('convert', [clip('a.mp4')], all)).toBeNull()
  })
})

describe('video options', () => {
  it('checks heights, times, GIF limits and encoders', () => {
    const files = [clip('a.mp4')]
    const o = defaultVideoOptions()
    o.resize.height = 721
    expect(validateOptions('resize', o, files, null)?.key).toBe('video.resize.invalid')
    expect(validateOptions('trim', o, files, null)?.key).toBe('video.trim.empty')
    o.trim.start = '0:20'
    expect(validateOptions('trim', o, files, null)?.key).toBe('media.time.pastEnd')
    o.toGif.fps = 60
    expect(validateOptions('toGif', o, files, null)?.key).toBe('video.toGif.invalidFps')
    o.toGif.fps = 10
    expect(validateOptions('toGif', o, [clip('long.mp4', 600_000)], null)?.key).toBe('video.toGif.tooLong')
    o.toGif.end = '30'
    expect(validateOptions('toGif', o, [clip('long.mp4', 600_000)], null)).toBeNull()
    o.convert.container = 'webm'
    expect(validateOptions('convert', o, files, ['libx264', 'aac'])?.key).toBe('video.convert.noEncoder')
    expect(validateOptions('convert', o, files, null)).toBeNull()
    o.audioTrack.mode = 'replace'
    expect(validateOptions('audioTrack', o, files, null)?.key).toBe('video.audioTrack.chooseAudio')
  })
})

describe('video requests', () => {
  it('builds one job per file, or one to join', () => {
    const files = [clip('a.mp4'), clip('b.mp4')]
    const o = defaultVideoOptions()
    expect(buildVideoRequests('merge', files, o)).toEqual([{ type: 'videoMerge', inputs: ['/v/a.mp4', '/v/b.mp4'] }])
    o.trim = { start: '1', end: '1:00', accurate: true }
    expect(buildVideoRequests('trim', files, o)[1]).toEqual({
      type: 'videoTrim',
      input: '/v/b.mp4',
      range: { startMs: 1000, endMs: 60_000 },
      accurate: true,
    })
    expect(buildVideoRequests('extractAudio', files, o)[0]).toEqual({ type: 'videoExtractAudio', input: '/v/a.mp4' })
    o.extractAudio.format = 'mp3'
    expect(buildVideoRequests('extractAudio', files, o)[0]).toEqual({
      type: 'videoExtractAudio',
      input: '/v/a.mp4',
      format: 'mp3',
    })
    expect(buildVideoRequests('toGif', files, o)[0]).toEqual({ type: 'videoToGif', input: '/v/a.mp4', fps: 10, width: 480 })
    o.audioTrack = { mode: 'replace', audio: '/m/song.mp3' }
    expect(buildVideoRequests('audioTrack', files, o)[0]).toEqual({
      type: 'videoAudioTrack',
      input: '/v/a.mp4',
      action: { mode: 'replace', audio: '/m/song.mp3' },
    })
  })
})

describe('audio requests', () => {
  it('sends a bitrate only for lossy formats', () => {
    const files = [{ ...clip('a.wav'), category: 'audio' as const }]
    const o = defaultAudioOptions()
    expect(buildAudioRequests('convert', files, o)[0]).toEqual({ type: 'audioConvert', input: '/v/a.wav', format: 'mp3' })
    o.convert.bitrate = 128
    expect(buildAudioRequests('convert', files, o)[0]).toEqual({
      type: 'audioConvert',
      input: '/v/a.wav',
      format: 'mp3',
      bitrateKbps: 128,
    })
    o.convert.format = 'flac'
    expect(buildAudioRequests('convert', files, o)[0]).toEqual({ type: 'audioConvert', input: '/v/a.wav', format: 'flac' })
    expect(buildAudioRequests('mono16k', files, o)[0]).toEqual({ type: 'audioMono16k', input: '/v/a.wav' })
  })
})
