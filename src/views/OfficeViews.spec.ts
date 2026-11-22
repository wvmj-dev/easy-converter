import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import DocumentView from './DocumentView.vue'
import PresentationView from './PresentationView.vue'
import SpreadsheetView from './SpreadsheetView.vue'
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

function tool(id: string, available = true): ToolStatus {
  return {
    id,
    name: id,
    required: false,
    available,
    path: available ? `/usr/bin/${id}` : null,
    source: available ? 'system' : null,
    version: '1',
    error: null,
    encoders: null,
    languages: null,
  } as ToolStatus
}

function file(name: string, category: FileInfo['category']): FileInfo {
  return { path: `/d/${name}`, name, extension: name.split('.').pop() ?? null, size: 10, category, image: null, pdf: null, media: null }
}

async function mountView(view: unknown, files: FileInfo[], tools: ToolStatus[]) {
  invoke.mockImplementation(async (cmd: string) => (cmd === 'get_tools_status' ? tools : 1))
  const pinia = createPinia()
  setActivePinia(pinia)
  useFilesStore().files = files
  const wrapper = mount(view as typeof DocumentView, { global: { plugins: [pinia, setupI18n('pt-BR')] } })
  await flushPromises()
  return wrapper
}

const start = (wrapper: Awaited<ReturnType<typeof mountView>>) => wrapper.find('.actions .btn--primary')

describe('office screens', () => {
  beforeEach(() => invoke.mockReset())

  it('converts documents and explains a missing tool', async () => {
    const wrapper = await mountView(DocumentView, [file('a.md', 'document')], [tool('pandoc'), tool('libreoffice', false)])
    // PDF from Markdown needs LibreOffice too.
    expect(wrapper.text()).toContain('LibreOffice')
    expect(start(wrapper).attributes('disabled')).toBeDefined()
    await wrapper.find('input[value="docx"]').setValue(true)
    expect(start(wrapper).attributes('disabled')).toBeUndefined()
    await start(wrapper).trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'documentConvert', input: '/d/a.md', format: 'docx' },
    })
  })

  it('turns CSV into JSON without any tool', async () => {
    const wrapper = await mountView(SpreadsheetView, [file('a.csv', 'spreadsheet')], [])
    await wrapper.find('input[value="csvToJson"]').setValue(true)
    await start(wrapper).trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'csvToJson', input: '/d/a.csv', header: true, typed: false },
    })
  })

  it('renders slides as images', async () => {
    const wrapper = await mountView(PresentationView, [file('a.pptx', 'presentation')], [tool('libreoffice'), tool('pdfium')])
    await wrapper.find('input[value="toImages"]').setValue(true)
    await start(wrapper).trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'presentationToImages', input: '/d/a.pptx', format: 'png', dpi: 150 },
    })
  })
})
