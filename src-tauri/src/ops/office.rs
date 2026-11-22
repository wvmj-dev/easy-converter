//! Helpers shared by the office operations (documents, spreadsheets and presentations).

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::fs_utils;
use crate::tools::{definition, ToolBox, ToolId};

/// How much of a text file is read to guess its encoding and delimiter.
const SNIFF_BYTES: u64 = 1024 * 1024;

/// Validates an input whose extension must be one of `allowed` (lowercase). Returns the
/// canonical path and its extension.
pub fn validate_input(path: &Path, allowed: &[&str], what: &str) -> AppResult<(PathBuf, String)> {
    let path = fs_utils::validate_input_file(path)?;
    let ext = match fs_utils::extension_of(&path) {
        Some(ext) if allowed.contains(&ext.as_str()) => ext,
        _ => {
            return Err(AppError::InvalidParameter(format!(
                "not a supported {what}: {}",
                path.display()
            )))
        }
    };
    let mut head = [0u8; 8];
    let read = File::open(&path)?.read(&mut head)?;
    if !signature_matches(&ext, &head[..read]) {
        return Err(AppError::InvalidFile(format!(
            "the content is not {}",
            ext.to_uppercase()
        )));
    }
    Ok((path, ext))
}

/// Whether the first bytes fit the extension. LibreOffice detects formats by content, so
/// without this check a file could be opened as something else than it claims to be
/// (an HTML page named `.docx`, for example). Legacy binary formats (DOC, XLS, PPT) are
/// not checked: programs often save HTML or RTF under those names.
fn signature_matches(ext: &str, head: &[u8]) -> bool {
    match ext {
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" => head.starts_with(b"PK\x03\x04"),
        "rtf" => head.starts_with(b"{\\rtf"),
        _ => true,
    }
}

/// Reports a missing tool before the job is queued.
pub fn require(tools: &ToolBox, id: ToolId) -> AppResult<()> {
    if tools.is_available(id) {
        Ok(())
    } else {
        Err(AppError::ToolMissing(definition(id).name.to_owned()))
    }
}

/// Text encodings told apart for plain text and CSV input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    /// What spreadsheet programs on Windows write in Western languages.
    Windows1252,
}

impl TextEncoding {
    /// Name used by LibreOffice's "Text (encoded)" import filter.
    pub fn libreoffice_name(self) -> &'static str {
        match self {
            TextEncoding::Utf8 => "UTF8",
            TextEncoding::Windows1252 => "MS_1252",
        }
    }

    /// Number used by LibreOffice's CSV import filter options.
    pub fn libreoffice_csv_code(self) -> u32 {
        match self {
            TextEncoding::Utf8 => 76,
            TextEncoding::Windows1252 => 1,
        }
    }
}

/// The first megabyte of a file, for sniffing.
pub fn read_head(path: &Path) -> AppResult<Vec<u8>> {
    let mut head = Vec::new();
    File::open(path)?.take(SNIFF_BYTES).read_to_end(&mut head)?;
    Ok(head)
}

/// UTF-8 when the bytes are valid UTF-8 (a character cut at the end does not count).
pub fn detect_encoding(head: &[u8]) -> TextEncoding {
    match std::str::from_utf8(head) {
        Ok(_) => TextEncoding::Utf8,
        Err(err) if err.error_len().is_none() => TextEncoding::Utf8,
        Err(_) => TextEncoding::Windows1252,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_encodings() {
        assert_eq!(detect_encoding("ação".as_bytes()), TextEncoding::Utf8);
        // "ç" cut in the middle by the sniffing limit.
        assert_eq!(detect_encoding(&"aç".as_bytes()[..2]), TextEncoding::Utf8);
        assert_eq!(detect_encoding(b"a\xe7\xe3o"), TextEncoding::Windows1252);
    }

    #[test]
    fn checks_the_content_of_zip_and_rtf_formats() {
        assert!(signature_matches("docx", b"PK\x03\x04rest"));
        assert!(!signature_matches("ods", b"<html>"));
        assert!(signature_matches("rtf", b"{\\rtf1"));
        assert!(!signature_matches("rtf", b"plain"));
        assert!(signature_matches("doc", b"<html>"));
        assert!(signature_matches("csv", b""));
    }

    #[test]
    fn refuses_other_extensions() {
        let file = std::env::temp_dir().join(format!("ec-office-{}.txt", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        assert!(validate_input(&file, &["txt"], "document").is_ok());
        assert!(matches!(
            validate_input(&file, &["docx"], "document"),
            Err(AppError::InvalidParameter(_))
        ));
        std::fs::remove_file(file).unwrap();
    }
}
