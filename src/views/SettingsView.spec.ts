import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import SettingsView from './SettingsView.vue'
import { setupI18n } from '../i18n'
import ptBR from '../i18n/pt-BR.json'
import type { Settings } from '../types/Settings'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))

const saved: Settings = {
  language: 'pt-BR',
  concurrency: 4,
  videoConcurrency: 1,
  output: { mode: 'sameFolder' },
  outputSuffix: 'convertido',
  timeouts: { image: 10, video: 720 },
  toolPaths: {},
}

function mountSettings() {
  return mount(SettingsView, { global: { plugins: [setupI18n('pt-BR')] } })
}

describe('SettingsView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    invoke.mockReset()
  })

  it('loads the saved settings into the form', async () => {
    invoke.mockResolvedValue(saved)
    const wrapper = mountSettings()
    await flushPromises()

    expect(wrapper.find('form').exists()).toBe(true)
    expect((wrapper.find('#suffix').element as HTMLInputElement).value).toBe('convertido')
    expect((wrapper.find('#concurrency').element as HTMLInputElement).value).toBe('4')
  })

  it('sends the edited settings and shows backend errors', async () => {
    invoke.mockResolvedValueOnce(saved)
    const wrapper = mountSettings()
    await flushPromises()

    await wrapper.find('#suffix').setValue('final')
    invoke.mockRejectedValueOnce({ code: 'invalidParameter', message: 'invalid file name fragment' })
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(invoke).toHaveBeenLastCalledWith('update_settings', {
      newSettings: expect.objectContaining({ outputSuffix: 'final' }),
    })
    expect(wrapper.find('[role="alert"]').text()).toContain('Valor inválido.')
  })

  it('asks for a folder before saving when "chosen folder" is selected', async () => {
    invoke.mockResolvedValueOnce(saved)
    const wrapper = mountSettings()
    await flushPromises()

    await wrapper.find('input[type="radio"][value="folder"]').setValue(true)
    expect(wrapper.find('button[type="submit"]').attributes('disabled')).toBeDefined()
    expect(wrapper.text()).toContain(ptBR.settings.output.folderRequired)

    await wrapper.find('form').trigger('submit')
    await flushPromises()
    expect(invoke).not.toHaveBeenCalledWith('update_settings', expect.anything())
  })
})
