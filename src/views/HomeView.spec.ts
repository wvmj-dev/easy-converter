import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import HomeView from './HomeView.vue'
import { setupI18n } from '../i18n'
import ptBR from '../i18n/pt-BR.json'

describe('HomeView', () => {
  it('renders texts from the i18n catalog', () => {
    const wrapper = mount(HomeView, {
      global: { plugins: [setupI18n()] },
    })

    expect(wrapper.text()).toContain(ptBR.app.name)
    expect(wrapper.text()).toContain(ptBR.home.title)
  })
})
