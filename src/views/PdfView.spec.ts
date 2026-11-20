import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import PdfView from './PdfView.vue'
import { setupI18n } from '../i18n'
import ptBR from '../i18n/pt-BR.json'
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

function tool(id: string, languages: string[] | null = null): ToolStatus {
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
    languages,
  } as ToolStatus
}

function pdf(name: string): FileInfo {
  return {
    path: `/p/${name}`,
    name,
    extension: 'pdf',
    size: 10,
    category: 'pdf',
    image: null,
    pdf: { pages: 3, encrypted: false, needsPassword: false },
    media: null,
  }
}

async function mountView(files: FileInfo[]) {
  const pinia = createPinia()
  setActivePinia(pinia)
  useFilesStore().files = files
  const wrapper = mount(PdfView, { global: { plugins: [pinia, setupI18n('pt-BR')] } })
  await flushPromises()
  return wrapper
}

describe('PdfView', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockImplementation(async (cmd: string) =>
      cmd === 'get_tools_status'
        ? [tool('qpdf'), tool('pdfium'), tool('tesseract', ['eng', 'osd', 'por'])]
        : 1,
    )
  })

  it('merges the files in list order with one job', async () => {
    const wrapper = await mountView([pdf('b.pdf'), pdf('a.pdf')])
    expect(wrapper.text()).toContain('3 páginas')
    await wrapper.find('.actions .btn--primary').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: { type: 'pdfMerge', inputs: ['/p/b.pdf', '/p/a.pdf'] },
    })
  })

  it('offers the installed OCR languages, Portuguese and English first', async () => {
    const wrapper = await mountView([pdf('a.pdf')])
    await wrapper.find('input[name="pdf-operation"][value="ocr"]').setValue(true)
    const boxes = wrapper.findAll('input[type="checkbox"]')
    expect(boxes.map((b) => (b.element as HTMLInputElement).value)).toEqual(['eng', 'por'])
    expect(boxes.every((b) => (b.element as HTMLInputElement).checked)).toBe(true)
    expect(wrapper.text()).toContain(ptBR.pdf.ocr.names.por)
  })

  it('blocks Ghostscript compression when it is missing and clears passwords after use', async () => {
    const wrapper = await mountView([pdf('a.pdf')])
    await wrapper.find('input[name="pdf-operation"][value="compress"]').setValue(true)
    await wrapper.find('input[name="pdf-compress-level"][value="strong"]').setValue(true)
    expect(wrapper.find('.actions__problem').text()).toBe(ptBR.pdf.compress.noGhostscript)

    await wrapper.find('input[name="pdf-operation"][value="protect"]').setValue(true)
    await wrapper.find('#protect-password').setValue('segredo')
    await wrapper.find('#protect-confirm').setValue('segredo')
    await wrapper.find('.actions .btn--primary').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: expect.objectContaining({ type: 'pdfProtect', userPassword: 'segredo' }),
    })
    expect((wrapper.find('#protect-password').element as HTMLInputElement).value).toBe('')
    expect((wrapper.find('#protect-confirm').element as HTMLInputElement).value).toBe('')
  })
})
