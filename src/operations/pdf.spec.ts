import { describe, expect, it } from 'vitest'
import type { FileInfo } from '../types/FileInfo'
import type { ToolId } from '../types/ToolId'
import {
  buildPdfRequests,
  clearPasswords,
  defaultOcrLanguages,
  defaultPdfOptions,
  unavailableReason,
  validateOptions,
} from './pdf'

function pdf(name: string, pages: number | null = 10, needsPassword = false): FileInfo {
  return {
    path: `/p/${name}`,
    name,
    extension: 'pdf',
    size: 100,
    category: 'pdf',
    image: null,
    pdf: { pages, encrypted: needsPassword, needsPassword },
  }
}

const all = () => true
const without =
  (...missing: ToolId[]) =>
  (id: ToolId) =>
    !missing.includes(id)

describe('PDF operations', () => {
  it('needs the right files and tools', () => {
    expect(unavailableReason('merge', [pdf('a.pdf')], all)?.key).toBe('pdf.unavailable.needsTwo')
    expect(unavailableReason('toImages', [pdf('a.pdf')], without('pdfium'))).toEqual({
      key: 'pdf.unavailable.tool',
      params: { tool: 'PDFium' },
    })
    const locked = pdf('l.pdf', null, true)
    expect(unavailableReason('rotate', [locked], all)?.key).toBe('pdf.unavailable.locked')
    expect(unavailableReason('unlock', [locked], all)).toBeNull()
  })

  it('checks pages against the shortest file', () => {
    const options = defaultPdfOptions()
    options.selectPages.pages = '1-12'
    expect(validateOptions('selectPages', options, [pdf('a.pdf', 20), pdf('b.pdf', 10)], all)).toEqual({
      key: 'pdf.pages.pastEnd',
      params: { page: 12, count: 10 },
    })
    options.selectPages.pages = '10-1'
    expect(validateOptions('selectPages', options, [pdf('a.pdf', 20), pdf('b.pdf', 10)], all)).toBeNull()
  })

  it('validates passwords before sending them', () => {
    const options = defaultPdfOptions()
    const check = () => validateOptions('protect', options, [pdf('a.pdf')], all)?.key ?? null
    expect(check()).toBe('pdf.protect.passwordRequired')
    Object.assign(options.protect, { password: 'abc', confirm: 'abd' })
    expect(check()).toBe('pdf.protect.mismatch')
    options.protect.confirm = 'abc'
    expect(check()).toBeNull()
    options.protect.noPrint = true
    expect(check()).toBe('pdf.protect.ownerRequired')
    options.protect.ownerPassword = 'abc'
    expect(check()).toBe('pdf.protect.ownerSame')
    options.protect.ownerPassword = 'owner'
    expect(check()).toBeNull()
    options.protect.password = options.protect.confirm = 'á'.repeat(64)
    expect(check()).toBe('pdf.protect.tooLong')
  })

  it('needs Ghostscript only for strong compression', () => {
    const options = defaultPdfOptions()
    expect(validateOptions('compress', options, [pdf('a.pdf')], without('ghostscript'))).toBeNull()
    options.compress.level = 'strong'
    expect(validateOptions('compress', options, [pdf('a.pdf')], without('ghostscript'))?.key).toBe(
      'pdf.compress.noGhostscript',
    )
  })

  it('builds the backend requests', () => {
    const files = [pdf('a.pdf'), pdf('b.pdf')]
    const options = defaultPdfOptions(['eng', 'osd', 'por'])
    expect(buildPdfRequests('merge', files, options)).toEqual([
      { type: 'pdfMerge', inputs: ['/p/a.pdf', '/p/b.pdf'] },
    ])
    Object.assign(options.toImages, { format: 'jpeg', dpi: 200, quality: 70, scope: 'some', pages: '2, 4-3' })
    expect(buildPdfRequests('toImages', [files[0]!], options)).toEqual([
      {
        type: 'pdfToImages',
        input: '/p/a.pdf',
        format: 'jpeg',
        dpi: 200,
        quality: 70,
        pages: [
          { start: 2, end: 2 },
          { start: 4, end: 3 },
        ],
      },
    ])
    expect(buildPdfRequests('ocr', [files[0]!], options)).toEqual([
      { type: 'pdfOcr', input: '/p/a.pdf', languages: ['por', 'eng'] },
    ])
    expect(buildPdfRequests('unlock', [files[0]!], options)).toEqual([
      { type: 'pdfUnlock', input: '/p/a.pdf' },
    ])
  })

  it('prefers Portuguese and English for OCR and forgets passwords', () => {
    expect(defaultOcrLanguages(['deu', 'osd'])).toEqual(['deu'])
    expect(defaultOcrLanguages(['osd'])).toEqual([])
    const options = defaultPdfOptions()
    Object.assign(options.protect, { password: 'a', confirm: 'a', ownerPassword: 'b' })
    options.unlock.password = 'c'
    clearPasswords(options)
    expect(options.protect).toMatchObject({ password: '', confirm: '', ownerPassword: '' })
    expect(options.unlock.password).toBe('')
  })
})
