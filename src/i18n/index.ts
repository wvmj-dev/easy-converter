import { createI18n } from 'vue-i18n'
import ptBR from './pt-BR.json'

export type MessageSchema = typeof ptBR

export function setupI18n() {
  return createI18n<[MessageSchema], 'pt-BR'>({
    legacy: false,
    locale: 'pt-BR',
    fallbackLocale: 'pt-BR',
    messages: {
      'pt-BR': ptBR,
    },
  })
}
