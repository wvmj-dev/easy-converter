//! Document operations (phase 4), with LibreOffice and Pandoc (both optional).
//!
//! - Word processor files (DOC, DOCX, ODT, RTF) and plain text go through LibreOffice.
//! - Markdown and HTML go through Pandoc; PDF from them is made by Pandoc (DOCX) and then
//!   LibreOffice, since Pandoc alone would need a LaTeX installation.
//! - DOCX and ODT become Markdown or HTML with Pandoc; DOC and RTF first become DOCX with
//!   LibreOffice.
//! - PDF to DOCX or ODT uses LibreOffice's PDF import, which recreates the text in frames.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use ts_rs::TS;

use super::office::{self, detect_encoding, read_head};
use super::pdf::validate_pdf;
use super::{process_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::jobs::{JobCategory, JobContext, JobSpec, PlannedOutput};
use crate::tools::libreoffice::{self, Conversion};
use crate::tools::pandoc::{self, Reader, Writer};
use crate::tools::{qpdf, ToolBox, ToolId};

/// Largest Markdown or HTML file accepted. Pandoc keeps the whole document in memory.
pub const MAX_MARKUP_BYTES: u64 = 50 * 1024 * 1024;

const OFFICE_EXTENSIONS: [&str; 4] = ["doc", "docx", "odt", "rtf"];
pub const DOCUMENT_EXTENSIONS: [&str; 9] = [
    "doc", "docx", "odt", "rtf", "txt", "md", "markdown", "html", "htm",
];

// ---------------------------------------------------------------------------------------
// Request parameters
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DocumentFormat {
    Pdf,
    Docx,
    Odt,
    Rtf,
    Txt,
    Html,
    Md,
}

impl DocumentFormat {
    pub fn extension(self) -> &'static str {
        match self {
            DocumentFormat::Pdf => "pdf",
            DocumentFormat::Docx => "docx",
            DocumentFormat::Odt => "odt",
            DocumentFormat::Rtf => "rtf",
            DocumentFormat::Txt => "txt",
            DocumentFormat::Html => "html",
            DocumentFormat::Md => "md",
        }
    }

    /// LibreOffice `--convert-to` value, for the formats LibreOffice writes.
    fn office_filter(self) -> Option<&'static str> {
        match self {
            DocumentFormat::Pdf => Some("pdf"),
            DocumentFormat::Docx => Some("docx"),
            DocumentFormat::Odt => Some("odt"),
            DocumentFormat::Rtf => Some("rtf"),
            DocumentFormat::Txt => Some("txt:Text (encoded):UTF8"),
            DocumentFormat::Html | DocumentFormat::Md => None,
        }
    }

    fn pandoc_writer(self) -> Option<Writer> {
        match self {
            DocumentFormat::Docx => Some(Writer::Docx),
            DocumentFormat::Odt => Some(Writer::Odt),
            DocumentFormat::Rtf => Some(Writer::Rtf),
            DocumentFormat::Html => Some(Writer::Html),
            DocumentFormat::Md => Some(Writer::Markdown),
            DocumentFormat::Txt => Some(Writer::Plain),
            DocumentFormat::Pdf => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DocumentConvertParams {
    pub input: PathBuf,
    pub format: DocumentFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PdfDocumentFormat {
    Docx,
    Odt,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfToDocumentParams {
    pub input: PathBuf,
    pub format: PdfDocumentFormat,
}

// ---------------------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputKind {
    /// DOC, DOCX, ODT, RTF.
    Office,
    Text,
    Html,
    Markdown,
}

impl InputKind {
    fn of(ext: &str) -> Option<Self> {
        match ext {
            "txt" => Some(InputKind::Text),
            "html" | "htm" => Some(InputKind::Html),
            "md" | "markdown" => Some(InputKind::Markdown),
            _ if OFFICE_EXTENSIONS.contains(&ext) => Some(InputKind::Office),
            _ => None,
        }
    }
}

/// One step of a conversion. The output of each step is the input of the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Office {
        convert_to: &'static str,
        /// Plain text input: the import filter is chosen from the detected encoding.
        text_input: bool,
    },
    Pandoc {
        reader: Reader,
        writer: Writer,
    },
}

impl Step {
    fn tool(self) -> ToolId {
        match self {
            Step::Office { .. } => ToolId::Libreoffice,
            Step::Pandoc { .. } => ToolId::Pandoc,
        }
    }
}

fn office(format: DocumentFormat, text_input: bool) -> AppResult<Step> {
    format
        .office_filter()
        .map(|convert_to| Step::Office {
            convert_to,
            text_input,
        })
        .ok_or_else(|| AppError::Internal(format!("{format:?} is not written by LibreOffice")))
}

fn pandoc_to(reader: Reader, format: DocumentFormat) -> AppResult<Step> {
    format
        .pandoc_writer()
        .map(|writer| Step::Pandoc { reader, writer })
        .ok_or_else(|| AppError::Internal(format!("{format:?} is not written by Pandoc")))
}

/// The steps that turn a file with extension `ext` into `format`.
fn plan(ext: &str, format: DocumentFormat) -> AppResult<Vec<Step>> {
    use DocumentFormat as F;
    let kind = InputKind::of(ext)
        .ok_or_else(|| AppError::InvalidParameter(format!("not a supported document: .{ext}")))?;
    let same = match kind {
        InputKind::Html => format == F::Html,
        InputKind::Markdown => format == F::Md,
        _ => format.extension() == ext,
    };
    if same {
        return Err(AppError::InvalidParameter(format!(
            "the file is already {}",
            format.extension().to_uppercase()
        )));
    }
    let steps = match kind {
        InputKind::Office => match format {
            F::Pdf | F::Docx | F::Odt | F::Rtf | F::Txt => vec![office(format, false)?],
            F::Html | F::Md => match ext {
                "docx" => vec![pandoc_to(Reader::Docx, format)?],
                "odt" => vec![pandoc_to(Reader::Odt, format)?],
                _ => vec![office(F::Docx, false)?, pandoc_to(Reader::Docx, format)?],
            },
        },
        InputKind::Text => match format {
            F::Pdf | F::Docx | F::Odt | F::Rtf => vec![office(format, true)?],
            _ => {
                return Err(AppError::InvalidParameter(format!(
                    "plain text cannot become {}",
                    format.extension().to_uppercase()
                )))
            }
        },
        InputKind::Html | InputKind::Markdown => {
            let reader = if kind == InputKind::Html {
                Reader::Html
            } else {
                Reader::Markdown
            };
            match format {
                F::Pdf => vec![pandoc_to(reader, F::Docx)?, office(F::Pdf, false)?],
                _ => vec![pandoc_to(reader, format)?],
            }
        }
    };
    Ok(steps)
}

fn writer_extension(writer: Writer) -> &'static str {
    match writer {
        Writer::Docx => "docx",
        Writer::Odt => "odt",
        Writer::Rtf => "rtf",
        Writer::Html => "html",
        Writer::Markdown => "md",
        Writer::Plain => "txt",
    }
}

async fn run_steps(
    tools: &ToolBox,
    ctx: &JobContext,
    input: &Path,
    steps: &[Step],
) -> AppResult<PathBuf> {
    let mut current = input.to_path_buf();
    for (index, step) in steps.iter().enumerate() {
        ctx.check_cancelled()?;
        current = match *step {
            Step::Office {
                convert_to,
                text_input,
            } => {
                let infilter = if text_input {
                    let encoding = detect_encoding(&read_head(&current)?);
                    Some(format!("Text (encoded):{}", encoding.libreoffice_name()))
                } else {
                    None
                };
                let conversion = Conversion {
                    input: &current,
                    convert_to,
                    infilter: infilter.as_deref(),
                };
                libreoffice::convert(tools, ctx, conversion).await?
            }
            Step::Pandoc { reader, writer } => {
                let out =
                    ctx.temp_dir
                        .join(format!("step-{}.{}", index + 1, writer_extension(writer)));
                pandoc::convert(tools, ctx, &current, reader, writer, &out).await?;
                out
            }
        };
    }
    Ok(current)
}

// ---------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------

pub fn convert(
    params: DocumentConvertParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    let (input, ext) = office::validate_input(&params.input, &DOCUMENT_EXTENSIONS, "document")?;
    let steps = plan(&ext, params.format)?;
    for step in &steps {
        office::require(tools, step.tool())?;
    }
    if matches!(
        InputKind::of(&ext),
        Some(InputKind::Html | InputKind::Markdown)
    ) && std::fs::metadata(&input)?.len() > MAX_MARKUP_BYTES
    {
        return Err(AppError::TooLarge(format!(
            "Markdown and HTML files can have at most {} MB",
            MAX_MARKUP_BYTES / (1024 * 1024)
        )));
    }
    let destination = planner.destination(&input, params.format.extension())?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "documentConvert",
        JobCategory::Document,
        vec![input],
        move |ctx| async move {
            ctx.set_indeterminate();
            let temp = run_steps(&tools, &ctx, &work_input, &steps).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn pdf_to_document(
    params: PdfToDocumentParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    office::require(tools, ToolId::Libreoffice)?;
    let input = validate_pdf(&params.input)?;
    let ext = match params.format {
        PdfDocumentFormat::Docx => "docx",
        PdfDocumentFormat::Odt => "odt",
    };
    let destination = planner.destination(&input, ext)?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "pdfToDocument",
        JobCategory::Document,
        vec![input],
        move |ctx| async move {
            ctx.set_indeterminate();
            // LibreOffice cannot ask for a password in headless mode; say why it fails.
            if qpdf::info(&tools, &work_input)
                .await
                .is_some_and(|info| info.needs_password)
            {
                return Err(AppError::Password(
                    "remove the PDF password before converting it".into(),
                ));
            }
            let conversion = Conversion {
                input: &work_input,
                convert_to: ext,
                infilter: Some("writer_pdf_import"),
            };
            let temp = libreoffice::convert(&tools, &ctx, conversion).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use DocumentFormat as F;

    fn tools_of(ext: &str, format: DocumentFormat) -> Vec<ToolId> {
        plan(ext, format)
            .unwrap()
            .iter()
            .map(|s| s.tool())
            .collect()
    }

    #[test]
    fn office_files_go_through_libreoffice() {
        for ext in ["doc", "docx", "odt", "rtf"] {
            for format in [F::Pdf, F::Txt] {
                assert_eq!(tools_of(ext, format), [ToolId::Libreoffice]);
            }
        }
        assert_eq!(
            plan("docx", F::Txt).unwrap(),
            [Step::Office {
                convert_to: "txt:Text (encoded):UTF8",
                text_input: false
            }]
        );
        assert_eq!(tools_of("doc", F::Docx), [ToolId::Libreoffice]);
    }

    #[test]
    fn markup_goes_through_pandoc() {
        assert_eq!(
            plan("md", F::Docx).unwrap(),
            [Step::Pandoc {
                reader: Reader::Markdown,
                writer: Writer::Docx
            }]
        );
        assert_eq!(
            plan("htm", F::Md).unwrap(),
            [Step::Pandoc {
                reader: Reader::Html,
                writer: Writer::Markdown
            }]
        );
        assert_eq!(
            tools_of("markdown", F::Pdf),
            [ToolId::Pandoc, ToolId::Libreoffice]
        );
        assert_eq!(tools_of("docx", F::Md), [ToolId::Pandoc]);
        assert_eq!(tools_of("odt", F::Html), [ToolId::Pandoc]);
        assert_eq!(
            tools_of("rtf", F::Md),
            [ToolId::Libreoffice, ToolId::Pandoc]
        );
    }

    #[test]
    fn text_input_uses_the_detected_encoding() {
        assert_eq!(
            plan("txt", F::Odt).unwrap(),
            [Step::Office {
                convert_to: "odt",
                text_input: true
            }]
        );
        assert!(plan("txt", F::Md).is_err());
        assert!(plan("txt", F::Html).is_err());
    }

    #[test]
    fn refuses_same_format_and_unknown_input() {
        for (ext, format) in [
            ("docx", F::Docx),
            ("htm", F::Html),
            ("markdown", F::Md),
            ("txt", F::Txt),
            ("pdf", F::Pdf),
            ("exe", F::Docx),
        ] {
            assert!(
                matches!(plan(ext, format), Err(AppError::InvalidParameter(_))),
                "{ext} -> {format:?}"
            );
        }
    }
}
