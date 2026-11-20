import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import ImageView from './ImageView.vue'
import DropZone from '../components/DropZone.vue'
import { setupI18n } from '../i18n'
import ptBR from '../i18n/pt-BR.json'
import { useFilesStore } from '../stores/files'
import type { FileInfo } from '../types/FileInfo'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }),
}))

function image(name: string, format: 'png' | 'gif' = 'png'): FileInfo {
  return {
    path: `/p/${name}`,
    name,
    extension: format,
    size: 10,
    category: 'image',
    image: { width: 200, height: 100, format },
    pdf: null,
    media: null,
  }
}

function mountView(files: FileInfo[]) {
  const pinia = createPinia()
  setActivePinia(pinia)
  useFilesStore().files = files
  return mount(ImageView, { global: { plugins: [pinia, setupI18n('pt-BR')] } })
}

async function choose(wrapper: ReturnType<typeof mountView>, operation: string) {
  await wrapper.find(`input[name="image-operation"][value="${operation}"]`).setValue(true)
}

describe('ImageView', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockResolvedValue(1)
  })

  it('asks for images when the list is empty', () => {
    const wrapper = mountView([])
    expect(wrapper.text()).toContain(ptBR.image.title)
    expect(wrapper.find('.btn--primary[disabled]').exists()).toBe(false)
    expect(wrapper.text()).not.toContain(ptBR.operations.title)
  })

  it('tells, without an error, when nothing new was added', async () => {
    const wrapper = mountView([image('a.png')])
    invoke.mockImplementation(async (cmd: string) => (cmd === 'probe_file' ? image('a.png') : 1))
    wrapper.findComponent(DropZone).vm.$emit('files', ['/p/a.png'])
    await flushPromises()

    expect(wrapper.find('.alert--info').text()).toBe(ptBR.image.noNewImages)
    expect(wrapper.find('.alert--danger').exists()).toBe(false)
  })

  it('enqueues one conversion per image', async () => {
    const wrapper = mountView([image('a.png'), image('b.png')])
    await wrapper.find('#convert-format').setValue('webp')
    await wrapper.find('.actions .btn--primary').trigger('click')
    await flushPromises()

    const calls = invoke.mock.calls.filter(([cmd]) => cmd === 'enqueue_job')
    expect(calls.map(([, args]) => args)).toEqual([
      { request: { type: 'imageConvert', input: '/p/a.png', format: 'webp' } },
      { request: { type: 'imageConvert', input: '/p/b.png', format: 'webp' } },
    ])
    expect(wrapper.find('.alert--success').text()).toContain('2 tarefas')
  })

  it('starts the crop area from the whole first image and blocks invalid areas', async () => {
    const wrapper = mountView([image('a.png')])
    await choose(wrapper, 'crop')
    expect((wrapper.find('#crop-width').element as HTMLInputElement).value).toBe('200')

    await wrapper.find('#crop-x').setValue(150)
    const button = wrapper.find('.actions .btn--primary')
    expect(button.attributes('disabled')).toBeDefined()
    expect(wrapper.find('.actions__problem').text()).toContain('a.png')

    await button.trigger('click')
    expect(invoke).not.toHaveBeenCalledWith('enqueue_job', expect.anything())
  })

  it('joins images in list order with one job', async () => {
    const wrapper = mountView([image('a.png'), image('b.png')])
    await choose(wrapper, 'merge')
    // Reordering is offered while joining.
    await wrapper.findAll('.file-list__actions button')[0]?.trigger('click') // a ↑ is disabled
    await wrapper.findAll('.file-list__actions button')[3]?.trigger('click') // b ↑
    await wrapper.find('.actions .btn--primary').trigger('click')
    await flushPromises()

    expect(invoke).toHaveBeenCalledWith('enqueue_job', {
      request: expect.objectContaining({ type: 'imageMerge', inputs: ['/p/b.png', '/p/a.png'] }),
    })
  })

  it('explains why metadata removal is not available for GIF', async () => {
    const wrapper = mountView([image('anim.gif', 'gif')])
    await choose(wrapper, 'stripMetadata')
    expect(wrapper.find('.actions .btn--primary').attributes('disabled')).toBeDefined()
    expect(wrapper.find('.actions__problem').text()).toContain('anim.gif')
  })

  it('shows backend errors', async () => {
    invoke.mockRejectedValue({ code: 'invalidFile', message: 'not a supported image format' })
    const wrapper = mountView([image('a.png')])
    await wrapper.find('.actions .btn--primary').trigger('click')
    await flushPromises()
    expect(wrapper.find('.alert--danger').text()).toContain(ptBR.errors.invalidFile)
  })
})
