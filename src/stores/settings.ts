import { ref } from 'vue'
import { defineStore } from 'pinia'
import { api } from '../services/api'
import { resolveLocale, setLocale } from '../i18n'
import type { Settings } from '../types/Settings'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings | null>(null)

  function apply(value: Settings) {
    settings.value = value
    setLocale(resolveLocale(value.language))
  }

  async function load() {
    apply(await api.getSettings())
  }

  /** Saves through the backend, which validates and may normalize the values. */
  async function save(value: Settings) {
    apply(await api.updateSettings(value))
  }

  return { settings, load, save }
})
