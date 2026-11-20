// PDF operations offered by the interface: default options, availability, a light
// validation for immediate feedback and the requests sent to the backend. The backend
// validates everything again; the limits below only mirror `ops/pdf.rs`.
import type { FileInfo } from '../types/FileInfo'
import type { JobRequest } from '../types/JobRequest'
import type { PageRange } from '../types/PageRange'
import type { PdfCompressLevel } from '../types/PdfCompressLevel'
import type { PdfImageFormat } from '../types/PdfImageFormat'
import type { Rotation } from '../types/Rotation'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'
import { parsePages } from './pageRanges'

export const PDF_LIMITS = {
  maxMergeInputs: 200,
  maxSplitRanges: 500,
  minDpi: 36,
  maxDpi: 600,
  maxPasswordBytes: 127,
  maxOcrLanguages: 5,
} as const

export const PDF_OPERATIONS = [
  'merge',
  'split',
  'selectPages',
  'rotate',
  'compress',
  'toImages',
  'extractText',
  'extractImages',
  'ocr',
  'protect',
  'unlock',
] as const
export type PdfOperation = (typeof PDF_OPERATIONS)[number]

export type PageScope = 'all' | 'some'

export interface PdfOptions {
  split: { mode: 'everyPage' | 'every' | 'ranges'; every: number; ranges: string }
  selectPages: { pages: string }
  rotate: { rotation: Rotation; scope: PageScope; pages: string }
  toImages: { format: PdfImageFormat; dpi: number; quality: number; scope: PageScope; pages: string }
  protect: {
    password: string
    confirm: string
    ownerPassword: string
    noPrint: boolean
    noCopy: boolean
    noEdit: boolean
  }
  unlock: { password: string }
  compress: { level: PdfCompressLevel }
  ocr: { languages: string[] }
}

export function defaultPdfOptions(ocrLanguages: string[] = []): PdfOptions {
  return {
    split: { mode: 'everyPage', every: 2, ranges: '' },
    selectPages: { pages: '' },
    rotate: { rotation: 'cw90', scope: 'all', pages: '' },
    toImages: { format: 'png', dpi: 150, quality: 90, scope: 'all', pages: '' },
    protect: { password: '', confirm: '', ownerPassword: '', noPrint: false, noCopy: false, noEdit: false },
    unlock: { password: '' },
    compress: { level: 'standard' },
    ocr: { languages: defaultOcrLanguages(ocrLanguages) },
  }
}

/** Portuguese and English when installed, otherwise the first installed language. */
export function defaultOcrLanguages(installed: string[]): string[] {
  const usable = usableOcrLanguages(installed)
  const preferred = ['por', 'eng'].filter((lang) => usable.includes(lang))
  return preferred.length > 0 ? preferred : usable.slice(0, 1)
}

/** Installed languages that can be chosen (`osd` only detects orientation). */
export function usableOcrLanguages(installed: string[]): string[] {
  return installed.filter((lang) => lang !== 'osd')
}

/** Tools each operation needs. Strong compression needs Ghostscript (checked separately). */
const REQUIRED_TOOLS: Record<PdfOperation, ToolId[]> = {
  merge: ['qpdf'],
  split: ['qpdf'],
  selectPages: ['qpdf'],
  rotate: ['qpdf'],
  compress: [],
  toImages: ['pdfium'],
  extractText: ['pdfium'],
  extractImages: ['pdfium'],
  ocr: ['qpdf', 'pdfium', 'tesseract'],
  protect: ['qpdf'],
  unlock: ['qpdf'],
}

const TOOL_NAMES: Record<string, string> = {
  qpdf: 'qpdf',
  pdfium: 'PDFium',
  tesseract: 'Tesseract',
  ghostscript: 'Ghostscript',
}

function isIntIn(value: unknown, min: number, max: number): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= min && value <= max
}

/** Smallest known page count among the files, used to check page numbers. */
function minPages(files: FileInfo[]): number | null {
  const counts = files.map((f) => f.pdf?.pages).filter((n): n is number => typeof n === 'number')
  return counts.length > 0 ? Math.min(...counts) : null
}

/** Why an operation cannot be used with these files, or `null` when it can. */
export function unavailableReason(
  operation: PdfOperation,
  files: FileInfo[],
  hasTool: (id: ToolId) => boolean,
): Problem | null {
  if (files.length === 0) return { key: 'pdf.unavailable.noFiles' }
  const missing = REQUIRED_TOOLS[operation].find((id) => !hasTool(id))
  if (missing) return { key: 'pdf.unavailable.tool', params: { tool: TOOL_NAMES[missing] ?? missing } }
  if (operation === 'merge') {
    if (files.length < 2) return { key: 'pdf.unavailable.needsTwo' }
    if (files.length > PDF_LIMITS.maxMergeInputs) {
      return { key: 'pdf.unavailable.tooMany', params: { max: PDF_LIMITS.maxMergeInputs } }
    }
  }
  if (operation !== 'unlock') {
    const locked = files.find((f) => f.pdf?.needsPassword)
    if (locked) return { key: 'pdf.unavailable.locked', params: { name: locked.name } }
  }
  return null
}

function pagesProblem(text: string, files: FileInfo[]): Problem | null {
  const parsed = parsePages(text, minPages(files))
  return parsed.ok ? null : parsed.problem
}

/** First problem with the options, or `null` when they look valid. */
export function validateOptions(
  operation: PdfOperation,
  options: PdfOptions,
  files: FileInfo[],
  hasTool: (id: ToolId) => boolean,
): Problem | null {
  switch (operation) {
    case 'split': {
      const split = options.split
      if (split.mode === 'every') {
        return isIntIn(split.every, 1, 100_000) ? null : { key: 'pdf.split.invalidEvery' }
      }
      if (split.mode === 'ranges') {
        const parsed = parsePages(split.ranges, minPages(files))
        if (!parsed.ok) return parsed.problem
        return parsed.ranges.length <= PDF_LIMITS.maxSplitRanges
          ? null
          : { key: 'pdf.pages.tooMany', params: { max: PDF_LIMITS.maxSplitRanges } }
      }
      return null
    }
    case 'selectPages':
      return pagesProblem(options.selectPages.pages, files)
    case 'rotate':
      return options.rotate.scope === 'some' ? pagesProblem(options.rotate.pages, files) : null
    case 'toImages': {
      const o = options.toImages
      if (!isIntIn(o.dpi, PDF_LIMITS.minDpi, PDF_LIMITS.maxDpi)) {
        return { key: 'pdf.toImages.invalidDpi', params: { min: PDF_LIMITS.minDpi, max: PDF_LIMITS.maxDpi } }
      }
      if (o.format === 'jpeg' && !isIntIn(o.quality, 1, 100)) return { key: 'image.invalid.quality' }
      return o.scope === 'some' ? pagesProblem(o.pages, files) : null
    }
    case 'protect': {
      const o = options.protect
      const tooLong = (p: string) => new TextEncoder().encode(p).length > PDF_LIMITS.maxPasswordBytes
      if (o.password === '') return { key: 'pdf.protect.passwordRequired' }
      if (tooLong(o.password) || tooLong(o.ownerPassword)) {
        return { key: 'pdf.protect.tooLong', params: { max: PDF_LIMITS.maxPasswordBytes } }
      }
      if (/[\u0000-\u001f\u007f]/.test(o.password + o.ownerPassword)) {
        return { key: 'pdf.protect.controlChars' }
      }
      if (o.password !== o.confirm) return { key: 'pdf.protect.mismatch' }
      const restricted = o.noPrint || o.noCopy || o.noEdit
      if (restricted && o.ownerPassword === '') return { key: 'pdf.protect.ownerRequired' }
      if (o.ownerPassword !== '' && o.ownerPassword === o.password) {
        return { key: 'pdf.protect.ownerSame' }
      }
      return null
    }
    case 'compress':
      return options.compress.level === 'strong' && !hasTool('ghostscript')
        ? { key: 'pdf.compress.noGhostscript' }
        : null
    case 'ocr': {
      const count = options.ocr.languages.length
      return count >= 1 && count <= PDF_LIMITS.maxOcrLanguages
        ? null
        : { key: 'pdf.ocr.chooseLanguages', params: { max: PDF_LIMITS.maxOcrLanguages } }
    }
    case 'merge':
    case 'extractText':
    case 'extractImages':
    case 'unlock':
      return null
  }
}

function pages(text: string): PageRange[] {
  const parsed = parsePages(text)
  return parsed.ok ? parsed.ranges : []
}

/** Requests for the backend: one job per file, or a single job when joining. */
export function buildPdfRequests(
  operation: PdfOperation,
  files: FileInfo[],
  options: PdfOptions,
): JobRequest[] {
  const each = (make: (input: string) => JobRequest) => files.map((file) => make(file.path))
  switch (operation) {
    case 'merge':
      return [{ type: 'pdfMerge', inputs: files.map((f) => f.path) }]
    case 'split': {
      const s = options.split
      const split =
        s.mode === 'every'
          ? { mode: 'every' as const, pages: s.every }
          : s.mode === 'ranges'
            ? { mode: 'ranges' as const, ranges: pages(s.ranges) }
            : { mode: 'everyPage' as const }
      return each((input) => ({ type: 'pdfSplit', input, split }))
    }
    case 'selectPages':
      return each((input) => ({ type: 'pdfSelectPages', input, pages: pages(options.selectPages.pages) }))
    case 'rotate': {
      const o = options.rotate
      return each((input) => ({
        type: 'pdfRotate',
        input,
        rotation: o.rotation,
        ...(o.scope === 'some' ? { pages: pages(o.pages) } : {}),
      }))
    }
    case 'toImages': {
      const o = options.toImages
      return each((input) => ({
        type: 'pdfToImages',
        input,
        format: o.format,
        dpi: o.dpi,
        ...(o.format === 'jpeg' ? { quality: o.quality } : {}),
        ...(o.scope === 'some' ? { pages: pages(o.pages) } : {}),
      }))
    }
    case 'extractText':
      return each((input) => ({ type: 'pdfExtractText', input }))
    case 'extractImages':
      return each((input) => ({ type: 'pdfExtractImages', input }))
    case 'protect': {
      const o = options.protect
      return each((input) => ({
        type: 'pdfProtect',
        input,
        userPassword: o.password,
        ...(o.ownerPassword !== '' ? { ownerPassword: o.ownerPassword } : {}),
        noPrint: o.noPrint,
        noCopy: o.noCopy,
        noEdit: o.noEdit,
      }))
    }
    case 'unlock': {
      const password = options.unlock.password
      return each((input) => ({ type: 'pdfUnlock', input, ...(password !== '' ? { password } : {}) }))
    }
    case 'compress':
      return each((input) => ({ type: 'pdfCompress', input, level: options.compress.level }))
    case 'ocr':
      return each((input) => ({ type: 'pdfOcr', input, languages: [...options.ocr.languages] }))
  }
}

/** Forgets typed passwords, so they do not stay in memory or on screen after use. */
export function clearPasswords(options: PdfOptions): void {
  Object.assign(options.protect, { password: '', confirm: '', ownerPassword: '' })
  options.unlock.password = ''
}
