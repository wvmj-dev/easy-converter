import type { ToolId } from '../types/ToolId'

export interface ToolInstallInfo {
  /** Command for Debian and Ubuntu, or null when the tool ships with the app. */
  debian: string | null
  fedora: string | null
  /** Official site for optional tools on Windows. */
  windowsSite: string | null
}

export const TOOL_INSTALL: Record<ToolId, ToolInstallInfo> = {
  ffmpeg: { debian: 'sudo apt install ffmpeg', fedora: 'sudo dnf install ffmpeg-free', windowsSite: null },
  ffprobe: { debian: 'sudo apt install ffmpeg', fedora: 'sudo dnf install ffmpeg-free', windowsSite: null },
  qpdf: { debian: 'sudo apt install qpdf', fedora: 'sudo dnf install qpdf', windowsSite: null },
  tesseract: {
    debian: 'sudo apt install tesseract-ocr tesseract-ocr-por tesseract-ocr-eng',
    fedora: 'sudo dnf install tesseract tesseract-langpack-por tesseract-langpack-eng',
    windowsSite: null,
  },
  pdfium: { debian: null, fedora: null, windowsSite: null },
  libreoffice: {
    debian: 'sudo apt install libreoffice-writer libreoffice-calc libreoffice-impress',
    fedora: 'sudo dnf install libreoffice-writer libreoffice-calc libreoffice-impress',
    windowsSite: 'libreoffice.org',
  },
  pandoc: { debian: 'sudo apt install pandoc', fedora: 'sudo dnf install pandoc', windowsSite: 'pandoc.org' },
  calibre: { debian: 'sudo apt install calibre', fedora: 'sudo dnf install calibre', windowsSite: 'calibre-ebook.com' },
  ghostscript: {
    debian: 'sudo apt install ghostscript',
    fedora: 'sudo dnf install ghostscript',
    windowsSite: 'ghostscript.com',
  },
  sevenZip: { debian: 'sudo apt install 7zip', fedora: 'sudo dnf install 7zip', windowsSite: '7-zip.org' },
}

export const FEDORA_FULL_FFMPEG = 'sudo dnf swap ffmpeg-free ffmpeg --allowerasing'
