import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import AudioView from './AudioView.vue'
import VideoView from './VideoView.vue'
import { setupI18n } from '../i18n'
import ptBR from '../i18n/pt-BR.json'
import { useFilesStore } from '../stores/files'
import type { FileInfo } from '../types/FileInfo'
import type { ToolStatus } from '../types/ToolStatus'

const invoke = vi.fn()
const open = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: (...args: unknown[]) => open(...args) }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }),
}))

function tool(id: string): ToolStatus {
  return {
    id,
    name: id,
    required: true,
    available: true,
    path: `/usr/bin/${id}`,
    source: 'system',
    version: '1',
    error: null,
    encoders: null,
    languages: null,
  } as ToolStatus
}

function media(name: string, category: 'video' | 'audio'): FileInfo {
  return {
    path: `/m/${name}`,
    name,
    extension: name.split('.').pop() ?? null,
    size: 10,
    category,
    image: null,
    pdf: null,
    media: {
      durationMs: 65_000,
      video: category === 'video' ? { codec: 'h264', width: 1920, height: 1080, fps: 30 } : null,
      audio: [{ codec: 'aac', sampleRate: 48_000, channels: 2 }],
      subtitleCount: 0,
    },
  }
}

async function mountView(view: typeof VideoView | typeof AudioView, files: FileInfo[]) {
  const pinia = createPinia()
  setActivePinia(pinia)
  useFilesStore().files = files
  const wrapper = mount(view, { global: { plugins: [pinia, setupI18n('pt-BR')] } })
  await flushPromises()
  return wrapper
}

describe('VideoView', () => {
  beforeEach(() => {
    invoke.mockReset()
    open.mockReset()
    invoke.mockImplementation(async (cmd: string) => (cmd === 'get_tools_status' ? [tool('ffmpeg'), tool('ffprobe')] : 1))
  })

  it('shows duration and size, and converts each video', async () => {
    const wrapper = await mountView(VideoView, [media('a.mkv', 'video')])
    expect(wrapper.text()).toContain('1:05')
    expect(wrapper.text()).toContain('1920 × 1080')
    await wrapper.find('.actions .btn--primary').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'videoConvert', input: '/m/a.mkv', container: 'mp4' },
    })
  })

  it('validates cut times before starting', async () => {
    const wrapper = await mountView(VideoView, [media('a.mp4', 'video')])
    await wrapper.find('input[name="video-operation"][value="trim"]').setValue(true)
    await wrapper.find('#video-trim-start').setValue('2:00')
    expect(wrapper.find('.actions__problem').text()).toBe(ptBR.media.time.pastEnd.replace('{duration}', '1:05'))
    await wrapper.find('#video-trim-start').setValue('0:10')
    await wrapper.find('#video-trim-end').setValue('0:20')
    await wrapper.find('.actions .btn--primary').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'videoTrim', input: '/m/a.mp4', range: { startMs: 10_000, endMs: 20_000 }, accurate: false },
    })
  })

  it('replaces the audio with a file chosen in the dialog', async () => {
    open.mockResolvedValue('/m/song.mp3')
    const wrapper = await mountView(VideoView, [media('a.mp4', 'video')])
    await wrapper.find('input[name="video-operation"][value="audioTrack"]').setValue(true)
    await wrapper.find('input[name="video-audio-mode"][value="replace"]').setValue(true)
    expect(wrapper.find('.actions__problem').text()).toBe(ptBR.video.audioTrack.chooseAudio)
    await wrapper.find('.options .btn').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('song.mp3')
    await wrapper.find('.actions .btn--primary').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'videoAudioTrack', input: '/m/a.mp4', action: { mode: 'replace', audio: '/m/song.mp3' } },
    })
  })

  it('disables everything without FFmpeg', async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === 'get_tools_status' ? [] : 1))
    const wrapper = await mountView(VideoView, [media('a.mp4', 'video')])
    expect(wrapper.find('.actions .btn--primary').attributes('disabled')).toBeDefined()
  })
})

describe('AudioView', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockImplementation(async (cmd: string) => (cmd === 'get_tools_status' ? [tool('ffmpeg'), tool('ffprobe')] : 1))
  })

  it('converts with the chosen bitrate', async () => {
    const wrapper = await mountView(AudioView, [media('a.wav', 'audio'), media('v.mp4', 'video')])
    expect(wrapper.text()).toContain(ptBR.audio.otherFiles.split(' | ')[0].replace('{n}', '1'))
    await wrapper.find('#audio-bitrate').setValue('128')
    await wrapper.find('.actions .btn--primary').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'audioConvert', input: '/m/a.wav', format: 'mp3', bitrateKbps: 128 },
    })
  })
})
