// Spreadsheet operations offered by the interface. The backend validates everything
// again (`ops/spreadsheet.rs`).
import type { CsvDelimiter } from '../types/CsvDelimiter'
import type { FileInfo } from '../types/FileInfo'
import type { JobRequest } from '../types/JobRequest'
import type { SpreadsheetFormat } from '../types/SpreadsheetFormat'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'
import { extensionOf, missingTool } from './office'

export const SPREADSHEET_OPERATIONS = ['convert', 'csvToJson', 'jsonToCsv'] as const
export type SpreadsheetOperation = (typeof SPREADSHEET_OPERATIONS)[number]

export const SPREADSHEET_FORMATS: SpreadsheetFormat[] = ['xlsx', 'ods', 'csv', 'pdf']
export const CSV_DELIMITERS: CsvDelimiter[] = ['comma', 'semicolon', 'tab', 'pipe']
const SHEET_EXTENSIONS = ['xls', 'xlsx', 'ods', 'csv']

export interface SpreadsheetOptions {
  convert: { format: SpreadsheetFormat }
  csvToJson: { delimiter: CsvDelimiter | 'auto'; header: boolean; typed: boolean }
  jsonToCsv: { delimiter: CsvDelimiter; bom: boolean; escapeFormulas: boolean }
}

export function defaultSpreadsheetOptions(): SpreadsheetOptions {
  return {
    convert: { format: 'xlsx' },
    csvToJson: { delimiter: 'auto', header: true, typed: false },
    jsonToCsv: { delimiter: 'comma', bom: false, escapeFormulas: true },
  }
}

/** Extensions each operation accepts. */
const INPUTS: Record<SpreadsheetOperation, string[]> = {
  convert: SHEET_EXTENSIONS,
  csvToJson: ['csv'],
  jsonToCsv: ['json'],
}

export function unavailableReason(
  operation: SpreadsheetOperation,
  files: FileInfo[],
  hasTool: (id: ToolId) => boolean,
): Problem | null {
  if (files.length === 0) return { key: 'spreadsheet.unavailable.noFiles' }
  const other = files.find((f) => !INPUTS[operation].includes(extensionOf(f)))
  if (other) return { key: `spreadsheet.unavailable.${operation}`, params: { name: other.name } }
  return operation === 'convert' ? missingTool(['libreoffice'], hasTool) : null
}

export function validateOptions(
  operation: SpreadsheetOperation,
  options: SpreadsheetOptions,
  files: FileInfo[],
): Problem | null {
  if (operation !== 'convert') return null
  const same = files.find((f) => extensionOf(f) === options.convert.format)
  return same
    ? { key: 'spreadsheet.convert.same', params: { name: same.name, format: options.convert.format.toUpperCase() } }
    : null
}

export function buildSpreadsheetRequests(
  operation: SpreadsheetOperation,
  files: FileInfo[],
  options: SpreadsheetOptions,
): JobRequest[] {
  return files.map((file): JobRequest => {
    const input = file.path
    switch (operation) {
      case 'convert':
        return { type: 'spreadsheetConvert', input, format: options.convert.format }
      case 'csvToJson': {
        const o = options.csvToJson
        return {
          type: 'csvToJson',
          input,
          header: o.header,
          typed: o.typed,
          ...(o.delimiter !== 'auto' ? { delimiter: o.delimiter } : {}),
        }
      }
      case 'jsonToCsv': {
        const o = options.jsonToCsv
        return { type: 'jsonToCsv', input, delimiter: o.delimiter, bom: o.bom, escapeFormulas: o.escapeFormulas }
      }
    }
  })
}
