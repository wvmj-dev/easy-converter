// Helpers shared by the document, spreadsheet and presentation screens.
import type { FileInfo } from '../types/FileInfo'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'

const TOOL_NAMES: Partial<Record<ToolId, string>> = {
  libreoffice: 'LibreOffice',
  pandoc: 'Pandoc',
  pdfium: 'PDFium',
}

/** The first of `tools` that is missing, as a problem to show. */
export function missingTool(tools: ToolId[], hasTool: (id: ToolId) => boolean): Problem | null {
  const missing = tools.find((id) => !hasTool(id))
  return missing ? { key: 'office.missingTool', params: { tool: TOOL_NAMES[missing] ?? missing } } : null
}

/** Lowercase extension of a file, `''` when it has none. */
export function extensionOf(file: FileInfo): string {
  return (file.extension ?? '').toLowerCase()
}
