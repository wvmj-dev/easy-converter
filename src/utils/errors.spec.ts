import { describe, expect, it } from 'vitest'
import type { Composer } from 'vue-i18n'
import { setupI18n } from '../i18n'
import { errorMessage, errorParts } from './errors'

const t = setupI18n('pt-BR').global.t as Composer['t']

describe('error messages', () => {
  it('translate the code and keep the detail apart', () => {
    expect(errorParts(t, { code: 'invalidFile', message: 'invalid or unsupported file: bad header', detail: 'bad header' })).toEqual({
      message: 'O arquivo está danificado ou em um formato não suportado.',
      detail: 'bad header',
    })
    expect(errorMessage(t, { code: 'notFound', message: 'file not found: /a', detail: '/a' })).toBe(
      'Arquivo ou pasta não encontrado. Ele pode ter sido movido ou apagado. (/a)',
    )
  })

  it('name the missing tool and drop details that repeat the message', () => {
    expect(errorMessage(t, { code: 'toolMissing', message: 'tool not available: Calibre', detail: 'Calibre' })).toBe(
      'Calibre não foi encontrado. Veja na tela Ferramentas como instalar.',
    )
    expect(errorParts(t, { code: 'timeout', message: 'operation timed out' }).detail).toBeNull()
    expect(errorMessage(t, new Error('boom'))).toBe('Erro inesperado. (boom)')
    expect(errorMessage(t, 42)).toBe('Erro inesperado.')
  })
})
