import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import AboutView from './AboutView.vue'
import { setupI18n } from '../i18n'
import en from '../i18n/en.json'
import ptBR from '../i18n/pt-BR.json'
import { version } from '../../package.json'

describe('AboutView', () => {
  it('shows the version, license, AI notice and credits', () => {
    const wrapper = mount(AboutView, { global: { plugins: [setupI18n('pt-BR')] } })
    const text = wrapper.text()

    expect(text).toContain(`Versão ${version}`)
    expect(text).toContain(ptBR.about.license.text)
    expect(text).toContain(ptBR.about.ai.text)
    for (const name of ['Tauri', 'FFmpeg', 'Tesseract OCR', 'PDFium', 'Ghostscript']) {
      expect(text).toContain(name)
    }
  })

  it('is translated to English', () => {
    const wrapper = mount(AboutView, { global: { plugins: [setupI18n('en')] } })
    expect(wrapper.text()).toContain(en.about.ai.title)
    expect(wrapper.text()).toContain(`Version ${version}`)
  })
})
