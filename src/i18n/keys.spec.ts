import { describe, expect, it } from 'vitest'
import en from './en.json'
import ptBR from './pt-BR.json'

// Every source file except tests, as text.
const sources = import.meta.glob(['../**/*.{ts,vue}', '!../**/*.spec.ts', '!../types/**'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

type Messages = Record<string, unknown>

function has(messages: Messages, key: string): boolean {
  let node: unknown = messages
  for (const part of key.split('.')) {
    if (typeof node !== 'object' || node === null || !(part in node)) return false
    node = (node as Messages)[part]
  }
  return typeof node === 'string'
}

/** Keys written literally: t('a.b'), te('a.b'), key: 'a.b' (problems) and i18n-t keypath. */
function literalKeys(): Map<string, string> {
  const found = new Map<string, string>()
  const patterns = [/\bt[e]?\(\s*'([a-zA-Z0-9_.]+)'/g, /\bkey: '([a-zA-Z0-9_.]+)'/g]
  for (const [file, text] of Object.entries(sources)) {
    for (const pattern of patterns) {
      for (const match of text.matchAll(pattern)) {
        const key = match[1] ?? ''
        if (key.includes('.')) found.set(key, file)
      }
    }
  }
  return found
}

describe('translations', () => {
  it('have every literal key used in the code, in both languages', () => {
    const keys = literalKeys()
    expect(keys.size).toBeGreaterThan(100)
    const missing = [...keys].filter(([key]) => !has(ptBR, key) || !has(en, key)).map(([key, file]) => `${key} (${file})`)
    expect(missing).toEqual([])
  })
})
