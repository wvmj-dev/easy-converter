import { describe, expect, it } from 'vitest'
import type { FileInfo } from '../types/FileInfo'
import type { ToolId } from '../types/ToolId'
import { buildDocumentRequests, defaultDocumentOptions, toolsFor, unavailableReason as documentReason } from './document'
import {
  buildPresentationRequests,
  defaultPresentationOptions,
  unavailableReason as presentationReason,
  validateOptions as validatePresentation,
} from './presentation'
import {
  buildSpreadsheetRequests,
  defaultSpreadsheetOptions,
  unavailableReason as spreadsheetReason,
  validateOptions as validateSpreadsheet,
} from './spreadsheet'

function file(name: string, category: FileInfo['category']): FileInfo {
  return {
    path: `/f/${name}`,
    name,
    extension: name.split('.').pop() ?? null,
    size: 10,
    category,
    image: null,
    pdf: null,
    media: null,
  }
}

const all = () => true
const without = (missing: ToolId) => (id: ToolId) => id !== missing

describe('document conversions', () => {
  it('mirror the backend plan', () => {
    expect(toolsFor('docx', 'pdf')).toEqual(['libreoffice'])
    expect(toolsFor('doc', 'md')).toEqual(['libreoffice', 'pandoc'])
    expect(toolsFor('odt', 'html')).toEqual(['pandoc'])
    expect(toolsFor('md', 'pdf')).toEqual(['pandoc', 'libreoffice'])
    expect(toolsFor('htm', 'docx')).toEqual(['pandoc'])
    expect(toolsFor('txt', 'odt')).toEqual(['libreoffice'])
    expect(toolsFor('txt', 'md')).toBeNull()
    expect(toolsFor('markdown', 'md')).toBeNull()
    expect(toolsFor('docx', 'docx')).toBeNull()
  })

  it('explain what blocks a conversion', () => {
    const o = defaultDocumentOptions()
    expect(documentReason([], o, all)?.key).toBe('document.unavailable.noFiles')
    expect(documentReason([file('a.pdf.docx', 'document')], o, without('libreoffice'))).toEqual({
      key: 'office.missingTool',
      params: { tool: 'LibreOffice' },
    })
    o.format = 'md'
    expect(documentReason([file('a.md', 'document')], o, all)?.key).toBe('document.unavailable.same')
    expect(documentReason([file('a.txt', 'document')], o, all)?.key).toBe('document.unavailable.notOffered')
    expect(documentReason([file('a.docx', 'document')], o, without('libreoffice'))).toBeNull()
    expect(documentReason([file('a.rtf', 'document')], o, without('libreoffice'))?.key).toBe('office.missingTool')
    expect(buildDocumentRequests([file('a.docx', 'document')], o)).toEqual([
      { type: 'documentConvert', input: '/f/a.docx', format: 'md' },
    ])
  })
})

describe('spreadsheet operations', () => {
  it('accept only the inputs each operation reads', () => {
    const csv = file('a.csv', 'spreadsheet')
    const json = file('b.json', 'spreadsheet')
    expect(spreadsheetReason('convert', [csv, json], all)?.key).toBe('spreadsheet.unavailable.convert')
    expect(spreadsheetReason('csvToJson', [json], all)?.key).toBe('spreadsheet.unavailable.csvToJson')
    expect(spreadsheetReason('jsonToCsv', [csv], all)?.key).toBe('spreadsheet.unavailable.jsonToCsv')
    expect(spreadsheetReason('convert', [csv], without('libreoffice'))?.key).toBe('office.missingTool')
    // CSV and JSON are converted in Rust: no tool needed.
    expect(spreadsheetReason('csvToJson', [csv], () => false)).toBeNull()
  })

  it('refuse the same format and build requests', () => {
    const o = defaultSpreadsheetOptions()
    const xlsx = file('a.xlsx', 'spreadsheet')
    expect(validateSpreadsheet('convert', o, [xlsx])?.key).toBe('spreadsheet.convert.same')
    o.convert.format = 'pdf'
    expect(validateSpreadsheet('convert', o, [xlsx])).toBeNull()
    expect(buildSpreadsheetRequests('csvToJson', [file('a.csv', 'spreadsheet')], o)).toEqual([
      { type: 'csvToJson', input: '/f/a.csv', header: true, typed: false },
    ])
    o.csvToJson.delimiter = 'semicolon'
    expect(buildSpreadsheetRequests('csvToJson', [file('a.csv', 'spreadsheet')], o)[0]).toMatchObject({
      delimiter: 'semicolon',
    })
    expect(buildSpreadsheetRequests('jsonToCsv', [file('b.json', 'spreadsheet')], o)).toEqual([
      { type: 'jsonToCsv', input: '/f/b.json', delimiter: 'comma', bom: false, escapeFormulas: true },
    ])
  })
})

describe('presentation operations', () => {
  it('need LibreOffice, and PDFium for images', () => {
    const deck = [file('a.pptx', 'presentation')]
    expect(presentationReason('convert', deck, without('pdfium'))).toBeNull()
    expect(presentationReason('toImages', deck, without('pdfium'))).toEqual({
      key: 'office.missingTool',
      params: { tool: 'PDFium' },
    })
  })

  it('validate options and build requests', () => {
    const o = defaultPresentationOptions()
    const deck = [file('a.pdf.pptx', 'presentation')]
    o.convert.format = 'pptx'
    expect(validatePresentation('convert', o, deck)?.key).toBe('presentation.convert.same')
    o.toImages.dpi = 5
    expect(validatePresentation('toImages', o, deck)?.key).toBe('pdf.toImages.invalidDpi')
    o.toImages = { format: 'jpeg', dpi: 96, quality: 80 }
    expect(validatePresentation('toImages', o, deck)).toBeNull()
    expect(buildPresentationRequests('toImages', deck, o)).toEqual([
      { type: 'presentationToImages', input: '/f/a.pdf.pptx', format: 'jpeg', dpi: 96, quality: 80 },
    ])
  })
})
