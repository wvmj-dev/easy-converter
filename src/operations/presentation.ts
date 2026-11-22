// Presentation operations offered by the interface. The backend validates everything
// again (`ops/presentation.rs`).
import type { FileInfo } from '../types/FileInfo'
import type { JobRequest } from '../types/JobRequest'
import type { PdfImageFormat } from '../types/PdfImageFormat'
import type { PresentationFormat } from '../types/PresentationFormat'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'
import { isIntIn } from './media'
import { extensionOf, missingTool } from './office'
import { PDF_LIMITS } from './pdf'

export const PRESENTATION_OPERATIONS = ['convert', 'toImages'] as const
export type PresentationOperation = (typeof PRESENTATION_OPERATIONS)[number]

export const PRESENTATION_FORMATS: PresentationFormat[] = ['pdf', 'pptx', 'odp']

export interface PresentationOptions {
  convert: { format: PresentationFormat }
  toImages: { format: PdfImageFormat; dpi: number; quality: number }
}

export function defaultPresentationOptions(): PresentationOptions {
  return {
    convert: { format: 'pdf' },
    toImages: { format: 'png', dpi: 150, quality: 90 },
  }
}

const REQUIRED_TOOLS: Record<PresentationOperation, ToolId[]> = {
  convert: ['libreoffice'],
  toImages: ['libreoffice', 'pdfium'],
}

export function unavailableReason(
  operation: PresentationOperation,
  files: FileInfo[],
  hasTool: (id: ToolId) => boolean,
): Problem | null {
  if (files.length === 0) return { key: 'presentation.unavailable.noFiles' }
  return missingTool(REQUIRED_TOOLS[operation], hasTool)
}

export function validateOptions(
  operation: PresentationOperation,
  options: PresentationOptions,
  files: FileInfo[],
): Problem | null {
  if (operation === 'convert') {
    const same = files.find((f) => extensionOf(f) === options.convert.format)
    return same
      ? { key: 'presentation.convert.same', params: { name: same.name, format: options.convert.format.toUpperCase() } }
      : null
  }
  const o = options.toImages
  if (!isIntIn(o.dpi, PDF_LIMITS.minDpi, PDF_LIMITS.maxDpi)) {
    return { key: 'pdf.toImages.invalidDpi', params: { min: PDF_LIMITS.minDpi, max: PDF_LIMITS.maxDpi } }
  }
  return o.format === 'jpeg' && !isIntIn(o.quality, 1, 100) ? { key: 'image.invalid.quality' } : null
}

export function buildPresentationRequests(
  operation: PresentationOperation,
  files: FileInfo[],
  options: PresentationOptions,
): JobRequest[] {
  return files.map((file): JobRequest => {
    if (operation === 'convert') {
      return { type: 'presentationConvert', input: file.path, format: options.convert.format }
    }
    const o = options.toImages
    return {
      type: 'presentationToImages',
      input: file.path,
      format: o.format,
      dpi: o.dpi,
      ...(o.format === 'jpeg' ? { quality: o.quality } : {}),
    }
  })
}
