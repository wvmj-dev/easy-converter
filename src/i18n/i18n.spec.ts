import { describe, expect, it } from 'vitest'
import { resolveLocale } from '.'
import ptBR from './pt-BR.json'
import en from './en.json'

function keys(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) {
    return [prefix]
  }
  return Object.entries(value).flatMap(([key, child]) => keys(child, prefix ? `${prefix}.${key}` : key))
}

describe('i18n', () => {
  it('has the same keys in every language', () => {
    expect(keys(en).sort()).toEqual(keys(ptBR).sort())
  })

  it('resolves the interface locale', () => {
    expect(resolveLocale('pt-BR', 'en-US')).toBe('pt-BR')
    expect(resolveLocale('en', 'pt-BR')).toBe('en')
    expect(resolveLocale('auto', 'pt-PT')).toBe('pt-BR')
    expect(resolveLocale('auto', 'de-DE')).toBe('en')
  })
})
