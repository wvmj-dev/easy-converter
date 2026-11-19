import { createI18n } from 'vue-i18n'
import type { Language } from '../types/Language'
import ptBR from './pt-BR.json'
import en from './en.json'

export type MessageSchema = typeof ptBR
export type Locale = 'pt-BR' | 'en'

export const LOCALES: Locale[] = ['pt-BR', 'en']

/** Picks the interface locale from the setting, falling back to the system language. */
export function resolveLocale(language: Language, systemLanguage = navigator.language): Locale {
  if (LOCALES.includes(language as Locale)) {
    return language as Locale
  }
  return systemLanguage.toLowerCase().startsWith('pt') ? 'pt-BR' : 'en'
}

export function setupI18n(locale: Locale = resolveLocale('auto')) {
  return createI18n<[MessageSchema], Locale, false>({
    legacy: false,
    locale,
    fallbackLocale: 'pt-BR',
    messages: {
      'pt-BR': ptBR,
      en,
    },
  })
}

export const i18n = setupI18n()

export function setLocale(locale: Locale) {
  i18n.global.locale.value = locale
  document.documentElement.lang = locale
}
