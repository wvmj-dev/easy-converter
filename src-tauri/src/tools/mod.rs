//! External tools: definitions, resolution and process execution.

pub mod capabilities;
pub mod ffmpeg;
pub mod process;
pub mod resolver;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use resolver::ToolRegistry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ToolId {
    Ffmpeg,
    Ffprobe,
    Qpdf,
    Tesseract,
    Pdfium,
    Libreoffice,
    Pandoc,
    Calibre,
    Ghostscript,
    SevenZip,
}

/// How a tool was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ToolSource {
    /// Shipped with the app (Windows sidecar or bundled resource).
    Bundled,
    /// Found in the `PATH`.
    System,
    /// Found in a well-known install folder (Windows).
    KnownLocation,
    /// Path chosen by the user in the settings.
    Custom,
}

/// Result of resolving a tool, exposed to the frontend by `get_tools_status`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ToolStatus {
    pub id: ToolId,
    pub name: String,
    /// Required tools ship with the app; optional ones only enable extra features.
    pub required: bool,
    pub available: bool,
    pub path: Option<PathBuf>,
    pub source: Option<ToolSource>,
    pub version: Option<String>,
    /// Why the tool is unavailable, when it was found but could not be used.
    pub error: Option<String>,
    /// FFmpeg only: names of the available encoders.
    pub encoders: Option<Vec<String>>,
    /// Tesseract only: installed OCR languages.
    pub languages: Option<Vec<String>>,
    /// Folder with the tool's DLLs and data, set for bundled Windows tools.
    #[serde(skip)]
    #[ts(skip)]
    pub runtime_dir: Option<PathBuf>,
}

/// What kind of artifact a tool is.
#[derive(Debug, Clone, Copy)]
pub enum ToolKind {
    /// A program run as a separate process.
    Executable {
        /// Candidate file names, in order of preference (without `.exe`).
        names: &'static [&'static str],
        version_args: &'static [&'static str],
        /// Shipped as a sidecar next to the app executable on Windows.
        windows_sidecar: bool,
    },
    /// A shared library bundled as a resource (PDFium).
    Library {
        resource_dir: &'static str,
        linux_file: &'static str,
        windows_file: &'static str,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ToolDefinition {
    pub id: ToolId,
    pub name: &'static str,
    pub required: bool,
    pub kind: ToolKind,
}

const fn exe(
    names: &'static [&'static str],
    version_args: &'static [&'static str],
    windows_sidecar: bool,
) -> ToolKind {
    ToolKind::Executable {
        names,
        version_args,
        windows_sidecar,
    }
}

pub const TOOLS: [ToolDefinition; 10] = [
    ToolDefinition {
        id: ToolId::Ffmpeg,
        name: "FFmpeg",
        required: true,
        kind: exe(&["ffmpeg"], &["-version"], true),
    },
    ToolDefinition {
        id: ToolId::Ffprobe,
        name: "ffprobe",
        required: true,
        kind: exe(&["ffprobe"], &["-version"], true),
    },
    ToolDefinition {
        id: ToolId::Qpdf,
        name: "qpdf",
        required: true,
        kind: exe(&["qpdf"], &["--version"], true),
    },
    ToolDefinition {
        id: ToolId::Tesseract,
        name: "Tesseract",
        required: true,
        kind: exe(&["tesseract"], &["--version"], true),
    },
    ToolDefinition {
        id: ToolId::Pdfium,
        name: "PDFium",
        required: true,
        kind: ToolKind::Library {
            resource_dir: "pdfium",
            linux_file: "libpdfium.so",
            windows_file: "pdfium.dll",
        },
    },
    ToolDefinition {
        id: ToolId::Libreoffice,
        name: "LibreOffice",
        required: false,
        kind: exe(&["soffice", "libreoffice"], &["--version"], false),
    },
    ToolDefinition {
        id: ToolId::Pandoc,
        name: "Pandoc",
        required: false,
        kind: exe(&["pandoc"], &["--version"], false),
    },
    ToolDefinition {
        id: ToolId::Calibre,
        name: "Calibre",
        required: false,
        kind: exe(&["ebook-convert"], &["--version"], false),
    },
    ToolDefinition {
        id: ToolId::Ghostscript,
        name: "Ghostscript",
        required: false,
        kind: exe(&["gs", "gswin64c"], &["--version"], false),
    },
    ToolDefinition {
        id: ToolId::SevenZip,
        name: "7-Zip",
        required: false,
        kind: exe(&["7zz", "7z"], &["i"], false),
    },
];

pub fn definition(id: ToolId) -> &'static ToolDefinition {
    // Every ToolId has exactly one entry in TOOLS (checked by a test).
    TOOLS.iter().find(|d| d.id == id).unwrap_or(&TOOLS[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_one_definition() {
        let ids = [
            ToolId::Ffmpeg,
            ToolId::Ffprobe,
            ToolId::Qpdf,
            ToolId::Tesseract,
            ToolId::Pdfium,
            ToolId::Libreoffice,
            ToolId::Pandoc,
            ToolId::Calibre,
            ToolId::Ghostscript,
            ToolId::SevenZip,
        ];
        for id in ids {
            assert_eq!(TOOLS.iter().filter(|d| d.id == id).count(), 1, "{id:?}");
            assert_eq!(definition(id).id, id);
        }
    }
}
