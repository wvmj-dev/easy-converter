// Ebook conversion offered by the interface. The backend validates everything again
// (`ops/ebook.rs`).
import type { EbookFormat } from '../types/EbookFormat'
import type { FileInfo } from '../types/FileInfo'
import type { JobRequest } from '../types/JobRequest'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'
import { extensionOf, missingTool } from './office'

export const EBOOK_FORMATS: EbookFormat[] = ['epub', 'azw3', 'mobi', 'pdf']

export interface EbookOptions {
  format: EbookFormat
}

export function defaultEbookOptions(): EbookOptions {
  return { format: 'epub' }
}

export function unavailableReason(
  files: FileInfo[],
  options: EbookOptions,
  hasTool: (id: ToolId) => boolean,
): Problem | null {
  if (files.length === 0) return { key: 'ebook.unavailable.noFiles' }
  const same = files.find((f) => extensionOf(f) === options.format)
  if (same) return { key: 'ebook.unavailable.same', params: { name: same.name, format: options.format.toUpperCase() } }
  return missingTool(['calibre'], hasTool)
}

export function buildEbookRequests(files: FileInfo[], options: EbookOptions): JobRequest[] {
  return files.map((file) => ({ type: 'ebookConvert', input: file.path, format: options.format }))
}
