import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import HomeView from './HomeView.vue'
import { setupI18n } from '../i18n'
import ptBR from '../i18n/pt-BR.json'

const invoke = vi.fn()
let dropHandler: ((event: { payload: unknown }) => void) | null = null

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async (handler: (event: { payload: unknown }) => void) => {
      dropHandler = handler
      return () => {}
    },
  }),
}))

function mountHome() {
  return mount(HomeView, { global: { plugins: [setupI18n('pt-BR')] } })
}

describe('HomeView', () => {
  beforeEach(() => {
    invoke.mockReset()
    dropHandler = null
  })

  it('renders texts from the i18n catalog', () => {
    const wrapper = mountHome()
    expect(wrapper.text()).toContain(ptBR.home.title)
    expect(wrapper.text()).toContain(ptBR.dropZone.title)
  })

  it('probes dropped files and lists them once', async () => {
    invoke.mockImplementation(async (_cmd: string, args: { path: string }) => ({
      path: args.path,
      name: args.path.split('/').pop(),
      extension: 'png',
      size: 2048,
      category: 'image',
    }))
    const wrapper = mountHome()
    await flushPromises()

    dropHandler?.({ payload: { type: 'drop', paths: ['/home/u/a.png', '/home/u/a.png'] } })
    await flushPromises()

    expect(invoke).toHaveBeenCalledWith('probe_file', { path: '/home/u/a.png' })
    expect(wrapper.findAll('.file-list__item')).toHaveLength(1)
    expect(wrapper.text()).toContain('a.png')
    expect(wrapper.text()).toContain('Imagem')
  })

  it('shows translated errors for files the backend rejects', async () => {
    invoke.mockRejectedValue({ code: 'notFound', message: 'file not found: /x/gone.txt' })
    const wrapper = mountHome()
    await flushPromises()

    dropHandler?.({ payload: { type: 'drop', paths: ['/x/gone.txt'] } })
    await flushPromises()

    expect(wrapper.find('[role="alert"]').text()).toContain(ptBR.errors.notFound)
    expect(wrapper.findAll('.file-list__item')).toHaveLength(0)
  })
})
