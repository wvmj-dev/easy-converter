// Document conversion offered by the interface. Which tools a conversion needs mirrors
// the plan in `ops/document.rs`, which validates everything again.
import type { DocumentFormat } from '../types/DocumentFormat'
import type { FileInfo } from '../types/FileInfo'
import type { JobRequest } from '../types/JobRequest'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'
import { extensionOf, missingTool } from './office'

export const DOCUMENT_FORMATS: DocumentFormat[] = ['pdf', 'docx', 'odt', 'rtf', 'txt', 'html', 'md']

/** Formats that leave images out (the image files would not travel with the output). */
export const TEXT_FORMATS: DocumentFormat[] = ['txt', 'html', 'md']

export interface DocumentOptions {
  format: DocumentFormat
}

export function defaultDocumentOptions(): DocumentOptions {
  return { format: 'pdf' }
}

type Kind = 'office' | 'text' | 'html' | 'markdown'

function kindOf(ext: string): Kind | null {
  if (['doc', 'docx', 'odt', 'rtf'].includes(ext)) return 'office'
  if (ext === 'txt') return 'text'
  if (ext === 'html' || ext === 'htm') return 'html'
  if (ext === 'md' || ext === 'markdown') return 'markdown'
  return null
}

function isSameFormat(ext: string, format: DocumentFormat): boolean {
  const kind = kindOf(ext)
  if (kind === 'html') return format === 'html'
  if (kind === 'markdown') return format === 'md'
  return ext === format
}

/**
 * Tools that turn a file with extension `ext` into `format`, in order, or `null` when
 * that conversion is not offered.
 */
export function toolsFor(ext: string, format: DocumentFormat): ToolId[] | null {
  const kind = kindOf(ext)
  if (!kind || isSameFormat(ext, format)) return null
  const textual = format === 'html' || format === 'md'
  switch (kind) {
    case 'office':
      if (!textual) return ['libreoffice']
      return ext === 'docx' || ext === 'odt' ? ['pandoc'] : ['libreoffice', 'pandoc']
    case 'text':
      return ['pdf', 'docx', 'odt', 'rtf'].includes(format) ? ['libreoffice'] : null
    case 'html':
    case 'markdown':
      return format === 'pdf' ? ['pandoc', 'libreoffice'] : ['pandoc']
  }
}

export function unavailableReason(
  files: FileInfo[],
  options: DocumentOptions,
  hasTool: (id: ToolId) => boolean,
): Problem | null {
  if (files.length === 0) return { key: 'document.unavailable.noFiles' }
  const needed = new Set<ToolId>()
  for (const file of files) {
    const ext = extensionOf(file)
    const tools = toolsFor(ext, options.format)
    if (!tools) {
      const key = isSameFormat(ext, options.format) ? 'document.unavailable.same' : 'document.unavailable.notOffered'
      return { key, params: { name: file.name, format: options.format.toUpperCase() } }
    }
    tools.forEach((id) => needed.add(id))
  }
  return missingTool([...needed], hasTool)
}

export function buildDocumentRequests(files: FileInfo[], options: DocumentOptions): JobRequest[] {
  return files.map((file) => ({ type: 'documentConvert', input: file.path, format: options.format }))
}
