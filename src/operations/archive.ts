// Archive operations offered by the interface. The backend validates everything again
// (`ops/archive.rs`).
import type { ArchiveFormat } from '../types/ArchiveFormat'
import type { FileInfo } from '../types/FileInfo'
import type { JobRequest } from '../types/JobRequest'
import type { ToolId } from '../types/ToolId'
import type { Problem } from './image'
import { missingTool } from './office'

export const ARCHIVE_OPERATIONS = ['extract', 'create'] as const
export type ArchiveOperation = (typeof ARCHIVE_OPERATIONS)[number]

export const ARCHIVE_FORMATS: ArchiveFormat[] = ['zip', 'sevenZ', 'tarGz', 'tar']
export const MAX_ARCHIVE_INPUTS = 1000

export interface ArchiveOptions {
  create: { format: ArchiveFormat }
}

export function defaultArchiveOptions(): ArchiveOptions {
  return { create: { format: 'zip' } }
}

/** Formats only the system 7-Zip extracts (mirrors `needs_seven_zip` in the backend). */
export function needsSevenZip(name: string): boolean {
  return /\.(rar|xz|bz2)$/i.test(name)
}

export function unavailableReason(
  operation: ArchiveOperation,
  files: FileInfo[],
  hasTool: (id: ToolId) => boolean,
): Problem | null {
  if (files.length === 0) return { key: 'archive.unavailable.noFiles' }
  if (operation === 'create') {
    return files.length > MAX_ARCHIVE_INPUTS
      ? { key: 'archive.unavailable.tooMany', params: { max: MAX_ARCHIVE_INPUTS } }
      : null
  }
  const other = files.find((f) => f.category !== 'archive')
  if (other) return { key: 'archive.unavailable.notArchive', params: { name: other.name } }
  return files.some((f) => needsSevenZip(f.name)) ? missingTool(['sevenZip'], hasTool) : null
}

export function buildArchiveRequests(
  operation: ArchiveOperation,
  files: FileInfo[],
  options: ArchiveOptions,
): JobRequest[] {
  if (operation === 'create') {
    return [{ type: 'archiveCreate', inputs: files.map((f) => f.path), format: options.create.format }]
  }
  return files.map((file) => ({ type: 'archiveExtract', input: file.path }))
}
