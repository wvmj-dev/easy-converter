import { describe, expect, it } from 'vitest'
import { baseName, formatBytes } from './format'

describe('formatBytes', () => {
  it('formats sizes with the locale', () => {
    expect(formatBytes(512, 'en')).toBe('512 B')
    expect(formatBytes(1536, 'en')).toBe('1.5 KB')
    expect(formatBytes(1536, 'pt-BR')).toBe('1,5 KB')
    expect(formatBytes(250 * 1024 * 1024, 'en')).toBe('250 MB')
  })
})

describe('baseName', () => {
  it('handles both path separators', () => {
    expect(baseName('/home/u/photo.png')).toBe('photo.png')
    expect(baseName('C:\\Users\\u\\photo.png')).toBe('photo.png')
  })
})
