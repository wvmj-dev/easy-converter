import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import ArchiveView from './ArchiveView.vue'
import EbookView from './EbookView.vue'
import SubtitleView from './SubtitleView.vue'
import { setupI18n } from '../i18n'
import { useFilesStore } from '../stores/files'
import type { FileInfo } from '../types/FileInfo'
import type { ToolStatus } from '../types/ToolStatus'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }),
}))

function tool(id: string): ToolStatus {
  return { id, name: id, required: false, available: true, path: `/usr/bin/${id}`, source: 'system', version: '1', error: null, encoders: null, languages: null } as ToolStatus
}

function file(name: string, category: FileInfo['category']): FileInfo {
  return { path: `/d/${name}`, name, extension: name.split('.').pop() ?? null, size: 10, category, image: null, pdf: null, media: null }
}

async function mountView(view: unknown, files: FileInfo[], tools: ToolStatus[] = []) {
  invoke.mockImplementation(async (cmd: string) => (cmd === 'get_tools_status' ? tools : 1))
  const pinia = createPinia()
  setActivePinia(pinia)
  useFilesStore().files = files
  const wrapper = mount(view as typeof SubtitleView, { global: { plugins: [pinia, setupI18n('pt-BR')] } })
  await flushPromises()
  return wrapper
}

const start = (wrapper: Awaited<ReturnType<typeof mountView>>) => wrapper.find('.actions .btn--primary')

describe('phase 5 screens', () => {
  beforeEach(() => invoke.mockReset())

  it('shifts subtitles once a valid time is typed', async () => {
    const wrapper = await mountView(SubtitleView, [file('a.srt', 'subtitle')])
    await wrapper.find('input[value="shift"]').setValue(true)
    expect(start(wrapper).attributes('disabled')).toBeDefined()
    await wrapper.find('#subtitle-offset').setValue('2,5')
    await wrapper.find('input[value="earlier"]').setValue(true)
    await start(wrapper).trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'subtitleShift', input: '/d/a.srt', offsetMs: -2500 },
    })
  })

  it('converts ebooks with Calibre', async () => {
    const wrapper = await mountView(EbookView, [file('a.mobi', 'ebook')], [tool('calibre')])
    await start(wrapper).trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'ebookConvert', input: '/d/a.mobi', format: 'epub' },
    })
  })

  it('compresses every file in the list into one archive', async () => {
    const wrapper = await mountView(ArchiveView, [file('a.pdf', 'pdf'), file('b.zip', 'archive')])
    expect(wrapper.text()).toContain('a.pdf')
    await wrapper.find('input[value="create"]').setValue(true)
    expect(start(wrapper).text()).toContain('1 tarefa')
    await start(wrapper).trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'archiveCreate', inputs: ['/d/a.pdf', '/d/b.zip'], format: 'zip' },
    })
  })
})
