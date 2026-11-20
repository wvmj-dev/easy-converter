//! PDF operations (phase 2).
//!
//! - Structure (merge, split, select, rotate, protect, unlock, compress): qpdf.
//! - Rendering, text and image extraction: PDFium, in the isolated worker process.
//! - OCR: pages rendered by PDFium, recognised by Tesseract as text-only PDF pages and laid
//!   over the original pages with qpdf, so the original stays untouched and only an
//!   invisible, searchable text layer is added.
//! - Strong compression: Ghostscript, when installed.
//! - Images to PDF: written in Rust (see `from_images`).
//!
//! Every PDF the user gives is read only by those separate processes.

mod from_images;
pub mod pages;

use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use ts_rs::TS;

use super::image::{self as img, Rotation};
use super::{blocking_job, process_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::fs_utils;
use crate::jobs::{JobCategory, JobContext, JobSpec, PlannedOutput};
use crate::pdf_worker::{self, RenderFormat, WorkerTask};
use crate::tools::process::run_streaming;
use crate::tools::qpdf::{self, Restrictions};
use crate::tools::{ToolBox, ToolId};

pub use from_images::PdfPageSize;
use pages::PageRange;

/// Most files in one merge or images-to-PDF job.
pub const MAX_MERGE_INPUTS: usize = 200;
/// Most parts in a split by ranges.
pub const MAX_SPLIT_RANGES: usize = 500;
pub const MIN_DPI: u32 = 36;
pub const MAX_DPI: u32 = 600;
pub const DEFAULT_OCR_DPI: u32 = 300;
/// Most OCR languages combined in one job (each one slows recognition down).
pub const MAX_OCR_LANGUAGES: usize = 5;

// ---------------------------------------------------------------------------------------
// Request parameters
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ImagesToPdfParams {
    pub inputs: Vec<PathBuf>,
    pub page_size: PdfPageSize,
    /// Blank border around each image, in millimetres.
    pub margin_mm: u32,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfMergeParams {
    pub inputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "camelCase")]
#[ts(export)]
pub enum PdfSplitMode {
    /// One file per page.
    EveryPage,
    /// One file every `pages` pages.
    Every { pages: u32 },
    /// One file per range.
    Ranges { ranges: Vec<PageRange> },
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfSplitParams {
    pub input: PathBuf,
    pub split: PdfSplitMode,
}

/// Keeps only the selected pages, in the selected order (extracts and reorders).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfSelectPagesParams {
    pub input: PathBuf,
    pub pages: Vec<PageRange>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfRotateParams {
    pub input: PathBuf,
    pub rotation: Rotation,
    /// Pages to rotate; every page when absent.
    #[serde(default)]
    #[ts(optional)]
    pub pages: Option<Vec<PageRange>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PdfImageFormat {
    Png,
    Jpeg,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfToImagesParams {
    pub input: PathBuf,
    pub format: PdfImageFormat,
    pub dpi: u32,
    /// JPEG quality, 1 to 100.
    #[serde(default)]
    #[ts(optional)]
    pub quality: Option<u8>,
    /// Pages to convert; every page when absent.
    #[serde(default)]
    #[ts(optional)]
    pub pages: Option<Vec<PageRange>>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfExtractTextParams {
    pub input: PathBuf,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfExtractImagesParams {
    pub input: PathBuf,
}

/// Protects with a password (AES-256). The owner password, required when anything is
/// restricted, lifts the restrictions.
#[derive(Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfProtectParams {
    pub input: PathBuf,
    pub user_password: String,
    #[serde(default)]
    #[ts(optional)]
    pub owner_password: Option<String>,
    #[serde(default)]
    pub no_print: bool,
    #[serde(default)]
    pub no_copy: bool,
    #[serde(default)]
    pub no_edit: bool,
}

impl fmt::Debug for PdfProtectParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PdfProtectParams")
            .field("input", &self.input)
            .field("user_password", &"<redacted>")
            .field(
                "owner_password",
                &self.owner_password.as_ref().map(|_| "<redacted>"),
            )
            .field("no_print", &self.no_print)
            .field("no_copy", &self.no_copy)
            .field("no_edit", &self.no_edit)
            .finish()
    }
}

/// Removes the protection. The password may be absent for files that open without one
/// (only the editing restrictions are removed then).
#[derive(Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfUnlockParams {
    pub input: PathBuf,
    #[serde(default)]
    #[ts(optional)]
    pub password: Option<String>,
}

impl fmt::Debug for PdfUnlockParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PdfUnlockParams")
            .field("input", &self.input)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PdfCompressLevel {
    /// Lossless: object streams and recompressed data (qpdf).
    Standard,
    /// Images downsampled to about 150 dpi (Ghostscript, when installed).
    Strong,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfCompressParams {
    pub input: PathBuf,
    pub level: PdfCompressLevel,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PdfOcrParams {
    pub input: PathBuf,
    /// Tesseract language codes, such as `por` and `eng`.
    pub languages: Vec<String>,
}

// ---------------------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------------------

/// Validates a PDF input: existing regular file, `.pdf` extension and PDF header.
pub fn validate_pdf(path: &Path) -> AppResult<PathBuf> {
    let path = fs_utils::validate_input_file(path)?;
    if fs_utils::extension_of(&path).as_deref() != Some("pdf") {
        return Err(AppError::InvalidParameter(format!(
            "not a PDF file: {}",
            path.display()
        )));
    }
    // The header may follow some junk, but must be within the first kilobyte.
    let mut head = Vec::with_capacity(1024);
    File::open(&path)?.take(1024).read_to_end(&mut head)?;
    if !head.windows(5).any(|w| w == b"%PDF-") {
        return Err(AppError::InvalidFile("not a PDF file".into()));
    }
    Ok(path)
}

fn validate_dpi(dpi: u32) -> AppResult<u32> {
    if (MIN_DPI..=MAX_DPI).contains(&dpi) {
        Ok(dpi)
    } else {
        Err(AppError::InvalidParameter(format!(
            "resolution must be between {MIN_DPI} and {MAX_DPI} dpi"
        )))
    }
}

fn validate_optional_pages(pages: Option<&[PageRange]>) -> AppResult<()> {
    pages.map_or(Ok(()), pages::validate)
}

/// Checks that qpdf can be used, so a missing tool is reported before queueing.
fn require(tools: &ToolBox, id: ToolId) -> AppResult<()> {
    if tools.is_available(id) {
        Ok(())
    } else {
        Err(AppError::ToolMissing(
            crate::tools::definition(id).name.to_owned(),
        ))
    }
}

fn require_pdfium(tools: &ToolBox) -> AppResult<()> {
    tools.pdfium_worker().map(|_| ())
}

/// A job with one PDF input and one PDF output, built by `work` into `temp`.
fn single_pdf_job<F, Fut>(kind: &str, input: PathBuf, destination: PathBuf, work: F) -> JobSpec
where
    F: FnOnce(JobContext, PathBuf, PathBuf) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    let work_input = input.clone();
    process_job(kind, JobCategory::Pdf, vec![input], move |ctx| async move {
        ctx.set_indeterminate();
        let temp = ctx.temp_dir.join("output.pdf");
        work(ctx, work_input, temp.clone()).await?;
        Ok(vec![PlannedOutput { temp, destination }])
    })
}

// ---------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------

pub fn images_to_pdf(params: ImagesToPdfParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    if !(1..=MAX_MERGE_INPUTS).contains(&params.inputs.len()) {
        return Err(AppError::InvalidParameter(format!(
            "between 1 and {MAX_MERGE_INPUTS} images"
        )));
    }
    if params.margin_mm > from_images::MAX_MARGIN_MM {
        return Err(AppError::InvalidParameter(format!(
            "margin must be at most {} mm",
            from_images::MAX_MARGIN_MM
        )));
    }
    let inputs = params
        .inputs
        .iter()
        .map(|p| img::validate_input(p))
        .collect::<AppResult<Vec<_>>>()?;
    for input in &inputs {
        img::detect_format(input)?;
    }
    let destination = planner.destination(&inputs[0], "pdf")?;
    let (page_size, margin) = (params.page_size, params.margin_mm);
    let work_inputs = inputs.clone();
    Ok(blocking_job(
        "imagesToPdf",
        JobCategory::Pdf,
        inputs,
        move |ctx| {
            let temp = ctx.temp_dir.join("output.pdf");
            from_images::build(ctx, &work_inputs, page_size, margin, &temp)?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn merge(
    params: PdfMergeParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    if !(2..=MAX_MERGE_INPUTS).contains(&params.inputs.len()) {
        return Err(AppError::InvalidParameter(format!(
            "between 2 and {MAX_MERGE_INPUTS} PDF files can be joined"
        )));
    }
    require(tools, ToolId::Qpdf)?;
    let inputs = params
        .inputs
        .iter()
        .map(|p| validate_pdf(p))
        .collect::<AppResult<Vec<_>>>()?;
    let destination = planner.destination(&inputs[0], "pdf")?;
    let tools = tools.clone();
    let work_inputs = inputs.clone();
    Ok(process_job(
        "pdfMerge",
        JobCategory::Pdf,
        inputs,
        move |ctx| async move {
            ctx.set_indeterminate();
            let temp = ctx.temp_dir.join("output.pdf");
            let mut cmd = qpdf::command(&tools)?;
            cmd.arg("--empty")
                .arg("--pages")
                .args(&work_inputs)
                .arg("--")
                .arg(&temp);
            qpdf::run(cmd).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn split(
    params: PdfSplitParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require(tools, ToolId::Qpdf)?;
    let input = validate_pdf(&params.input)?;
    match &params.split {
        PdfSplitMode::EveryPage => {}
        PdfSplitMode::Every { pages } => {
            if !(1..=pages::MAX_PAGE).contains(pages) {
                return Err(AppError::InvalidParameter("invalid number of pages".into()));
            }
        }
        PdfSplitMode::Ranges { ranges } => {
            if ranges.is_empty() || ranges.len() > MAX_SPLIT_RANGES {
                return Err(AppError::InvalidParameter(format!(
                    "between 1 and {MAX_SPLIT_RANGES} parts"
                )));
            }
            pages::validate(ranges)?;
        }
    }
    let planner = planner.clone();
    let tools = tools.clone();
    let work_input = input.clone();
    let mode = params.split;
    Ok(process_job(
        "pdfSplit",
        JobCategory::Pdf,
        vec![input],
        move |ctx| async move {
            let mut outputs = Vec::new();
            match mode {
                PdfSplitMode::EveryPage | PdfSplitMode::Every { .. } => {
                    ctx.set_indeterminate();
                    let size = match mode {
                        PdfSplitMode::Every { pages } => pages,
                        _ => 1,
                    };
                    let mut cmd = qpdf::command(&tools)?;
                    cmd.arg(&work_input)
                        .arg(format!("--split-pages={size}"))
                        .arg(ctx.temp_dir.join("part.pdf"));
                    qpdf::run(cmd).await?;
                    for file in qpdf::split_outputs(&ctx.temp_dir, "part-")? {
                        // qpdf names parts `part-01.pdf` or `part-01-03.pdf`.
                        let label = file
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .and_then(|s| s.strip_prefix("part-"))
                            .unwrap_or("part")
                            .to_owned();
                        let destination =
                            planner.destination_part(&work_input, &format!("p{label}"), "pdf")?;
                        outputs.push(PlannedOutput {
                            temp: file,
                            destination,
                        });
                    }
                }
                PdfSplitMode::Ranges { ranges } => {
                    for (index, range) in ranges.iter().enumerate() {
                        ctx.check_cancelled()?;
                        let temp = ctx.temp_dir.join(format!("range-{:04}.pdf", index + 1));
                        let mut cmd = qpdf::command(&tools)?;
                        cmd.arg("--empty")
                            .arg("--pages")
                            .arg(&work_input)
                            .arg(pages::to_qpdf(std::slice::from_ref(range)))
                            .arg("--")
                            .arg(&temp);
                        qpdf::run(cmd).await?;
                        let label = pages::to_qpdf(std::slice::from_ref(range));
                        let destination =
                            planner.destination_part(&work_input, &format!("p{label}"), "pdf")?;
                        outputs.push(PlannedOutput { temp, destination });
                        ctx.set_steps(index + 1, ranges.len());
                    }
                }
            }
            if outputs.is_empty() {
                return Err(AppError::InvalidFile("the PDF has no pages".into()));
            }
            Ok(outputs)
        },
    ))
}

pub fn select_pages(
    params: PdfSelectPagesParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require(tools, ToolId::Qpdf)?;
    let input = validate_pdf(&params.input)?;
    pages::validate(&params.pages)?;
    let destination = planner.destination(&input, "pdf")?;
    let selection = pages::to_qpdf(&params.pages);
    let tools = tools.clone();
    Ok(single_pdf_job(
        "pdfSelectPages",
        input,
        destination,
        move |_, input, temp| async move {
            let mut cmd = qpdf::command(&tools)?;
            // `.` keeps the document-level data (outline, metadata) of the input.
            cmd.arg(&input)
                .arg("--pages")
                .arg(".")
                .arg(selection)
                .arg("--")
                .arg(&temp);
            qpdf::run(cmd).await
        },
    ))
}

pub fn rotate(
    params: PdfRotateParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require(tools, ToolId::Qpdf)?;
    let input = validate_pdf(&params.input)?;
    validate_optional_pages(params.pages.as_deref())?;
    let destination = planner.destination(&input, "pdf")?;
    let angle = match params.rotation {
        Rotation::Cw90 => "+90",
        Rotation::Cw180 => "+180",
        Rotation::Cw270 => "+270",
    };
    let rotate_arg = match &params.pages {
        Some(ranges) => format!("--rotate={angle}:{}", pages::to_qpdf(ranges)),
        None => format!("--rotate={angle}"),
    };
    let tools = tools.clone();
    Ok(single_pdf_job(
        "pdfRotate",
        input,
        destination,
        move |_, input, temp| async move {
            let mut cmd = qpdf::command(&tools)?;
            cmd.arg(&input).arg(rotate_arg).arg(&temp);
            qpdf::run(cmd).await
        },
    ))
}

pub fn to_images(
    params: PdfToImagesParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_pdfium(tools)?;
    let input = validate_pdf(&params.input)?;
    let dpi = validate_dpi(params.dpi)?;
    validate_optional_pages(params.pages.as_deref())?;
    let quality = img::validate_quality(params.quality.unwrap_or(img::DEFAULT_JPEG_QUALITY))?;
    let (format, ext) = match params.format {
        PdfImageFormat::Png => (RenderFormat::Png, "png"),
        PdfImageFormat::Jpeg => (RenderFormat::Jpeg, "jpg"),
    };
    // Checked now so a bad name fails before the job runs.
    planner.destination_part(&input, "p1", ext)?;
    let planner = planner.clone();
    let tools = tools.clone();
    let work_input = input.clone();
    let selection = params.pages;
    Ok(process_job(
        "pdfToImages",
        JobCategory::Pdf,
        vec![input],
        move |ctx| async move {
            let out_dir = ctx.temp_dir.join("pages");
            std::fs::create_dir(&out_dir)?;
            let task = WorkerTask::Render {
                input: work_input.clone(),
                pages: selection,
                dpi,
                format,
                quality,
                out_dir: out_dir.clone(),
            };
            pdf_worker::run(&tools, &ctx, task, (0.0, 100.0)).await?;
            numbered_outputs(&out_dir, &planner, &work_input, ext, |name| {
                // page-00012 → p012
                let page: u32 = name.strip_prefix("page-")?.parse().ok()?;
                Some(format!("p{page:03}"))
            })
        },
    ))
}

pub fn extract_text(
    params: PdfExtractTextParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_pdfium(tools)?;
    let input = validate_pdf(&params.input)?;
    let destination = planner.destination(&input, "txt")?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "pdfExtractText",
        JobCategory::Pdf,
        vec![input],
        move |ctx| async move {
            let temp = ctx.temp_dir.join("output.txt");
            let task = WorkerTask::ExtractText {
                input: work_input,
                out_file: temp.clone(),
            };
            pdf_worker::run(&tools, &ctx, task, (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn extract_images(
    params: PdfExtractImagesParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_pdfium(tools)?;
    let input = validate_pdf(&params.input)?;
    planner.destination_part(&input, "p001-img01", "png")?;
    let planner = planner.clone();
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "pdfExtractImages",
        JobCategory::Pdf,
        vec![input],
        move |ctx| async move {
            let out_dir = ctx.temp_dir.join("images");
            std::fs::create_dir(&out_dir)?;
            let task = WorkerTask::ExtractImages {
                input: work_input.clone(),
                out_dir: out_dir.clone(),
            };
            pdf_worker::run(&tools, &ctx, task, (0.0, 100.0)).await?;
            let outputs = numbered_outputs(&out_dir, &planner, &work_input, "png", |name| {
                // page-00012-image-003 → p012-img03
                let rest = name.strip_prefix("page-")?;
                let (page, image) = rest.split_once("-image-")?;
                let (page, image): (u32, u32) = (page.parse().ok()?, image.parse().ok()?);
                Some(format!("p{page:03}-img{image:02}"))
            })?;
            if outputs.is_empty() {
                return Err(AppError::InvalidFile("the PDF has no images".into()));
            }
            Ok(outputs)
        },
    ))
}

/// Maps the files the worker wrote to their destinations, in name order.
fn numbered_outputs(
    dir: &Path,
    planner: &OutputPlanner,
    input: &Path,
    ext: &str,
    label: impl Fn(&str) -> Option<String>,
) -> AppResult<Vec<PlannedOutput>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|temp| {
            let name = temp
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(&label)
                .ok_or_else(|| AppError::Internal(format!("unexpected file {}", temp.display())))?;
            let destination = planner.destination_part(input, &name, ext)?;
            Ok(PlannedOutput { temp, destination })
        })
        .collect()
}

pub fn protect(
    params: PdfProtectParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require(tools, ToolId::Qpdf)?;
    let input = validate_pdf(&params.input)?;
    qpdf::validate_password(&params.user_password, "password")?;
    let restrictions = Restrictions {
        no_print: params.no_print,
        no_copy: params.no_copy,
        no_edit: params.no_edit,
    };
    let owner = match params.owner_password.as_deref().filter(|p| !p.is_empty()) {
        Some(owner) => {
            qpdf::validate_password(owner, "owner password")?;
            if owner == params.user_password {
                return Err(AppError::InvalidParameter(
                    "the owner password must differ from the password".into(),
                ));
            }
            owner.to_owned()
        }
        None if restrictions.any() => {
            return Err(AppError::InvalidParameter(
                "restrictions need an owner password".into(),
            ));
        }
        // Nothing restricted: the password alone gives full access either way.
        None => params.user_password.clone(),
    };
    let destination = planner.destination(&input, "pdf")?;
    let user = params.user_password;
    let tools = tools.clone();
    Ok(single_pdf_job(
        "pdfProtect",
        input,
        destination,
        move |ctx, input, temp| async move {
            let args = qpdf::encrypt_args(&input, &temp, &user, &owner, restrictions)?;
            let args_file = ctx.temp_dir.join("qpdf-args");
            qpdf::write_private(&args_file, &args)?;
            let mut cmd = tools.command(ToolId::Qpdf)?;
            cmd.arg(format!("@{}", args_file.display()));
            let result = qpdf::run(cmd).await;
            let _ = std::fs::remove_file(&args_file);
            result
        },
    ))
}

pub fn unlock(
    params: PdfUnlockParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require(tools, ToolId::Qpdf)?;
    let input = validate_pdf(&params.input)?;
    let password = params.password.filter(|p| !p.is_empty());
    if let Some(password) = &password {
        qpdf::validate_password(password, "password")?;
    }
    let destination = planner.destination(&input, "pdf")?;
    let tools = tools.clone();
    Ok(single_pdf_job(
        "pdfUnlock",
        input,
        destination,
        move |ctx, input, temp| async move {
            let mut cmd = qpdf::command(&tools)?;
            let password_file = ctx.temp_dir.join("qpdf-password");
            if let Some(password) = &password {
                qpdf::write_private(&password_file, password)?;
                cmd.arg(format!("--password-file={}", password_file.display()));
            }
            cmd.arg("--decrypt").arg(&input).arg(&temp);
            let result = qpdf::run(cmd).await;
            let _ = std::fs::remove_file(&password_file);
            result
        },
    ))
}

pub fn compress(
    params: PdfCompressParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    let input = validate_pdf(&params.input)?;
    let level = params.level;
    match level {
        PdfCompressLevel::Standard => require(tools, ToolId::Qpdf)?,
        PdfCompressLevel::Strong => require(tools, ToolId::Ghostscript)?,
    }
    let destination = planner.destination(&input, "pdf")?;
    let tools = tools.clone();
    Ok(single_pdf_job(
        "pdfCompress",
        input,
        destination,
        move |_, input, temp| async move {
            match level {
                PdfCompressLevel::Standard => {
                    let mut cmd = qpdf::command(&tools)?;
                    cmd.arg(&input)
                        .args([
                            "--object-streams=generate",
                            "--compress-streams=y",
                            "--recompress-flate",
                            "--compression-level=9",
                        ])
                        .arg(&temp);
                    qpdf::run(cmd).await?;
                }
                PdfCompressLevel::Strong => {
                    let mut cmd = tools.command(ToolId::Ghostscript)?;
                    cmd.args(ghostscript_args(&input, &temp)?);
                    run_streaming(cmd, |_| {}).await?;
                }
            }
            // A "compressed" file larger than the original helps nobody.
            if std::fs::metadata(&temp)?.len() >= std::fs::metadata(&input)?.len() {
                std::fs::copy(&input, &temp)?;
            }
            Ok(())
        },
    ))
}

/// Ghostscript arguments for strong compression. `-dSAFER` stops the PostScript
/// interpreter from touching other files; `%` in the output path is escaped because
/// Ghostscript reads it as a page number placeholder.
pub fn ghostscript_args(input: &Path, output: &Path) -> AppResult<Vec<String>> {
    let to_str = |p: &Path| {
        p.to_str()
            .map(str::to_owned)
            .ok_or_else(|| AppError::InvalidPath(format!("{} is not valid UTF-8", p.display())))
    };
    Ok(vec![
        "-q".into(),
        "-dSAFER".into(),
        "-dBATCH".into(),
        "-dNOPAUSE".into(),
        "-sDEVICE=pdfwrite".into(),
        "-dCompatibilityLevel=1.7".into(),
        "-dPDFSETTINGS=/ebook".into(),
        "-dDetectDuplicateImages=true".into(),
        format!("-sOutputFile={}", to_str(output)?.replace('%', "%%")),
        to_str(input)?,
    ])
}

/// Checks the requested OCR languages against the installed ones.
fn validate_languages(requested: &[String], installed: &[String]) -> AppResult<String> {
    if requested.is_empty() || requested.len() > MAX_OCR_LANGUAGES {
        return Err(AppError::InvalidParameter(format!(
            "choose between 1 and {MAX_OCR_LANGUAGES} languages"
        )));
    }
    for lang in requested {
        let well_formed = !lang.is_empty()
            && lang.len() <= 32
            && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && lang != "osd";
        if !well_formed || !installed.contains(lang) {
            return Err(AppError::InvalidParameter(format!(
                "OCR language not installed: {lang}"
            )));
        }
    }
    Ok(requested.join("+"))
}

pub fn ocr(params: PdfOcrParams, planner: &OutputPlanner, tools: &ToolBox) -> AppResult<JobSpec> {
    require(tools, ToolId::Qpdf)?;
    require(tools, ToolId::Tesseract)?;
    require_pdfium(tools)?;
    let input = validate_pdf(&params.input)?;
    let languages = validate_languages(&params.languages, &tools.tesseract_languages())?;
    let destination = planner.destination(&input, "pdf")?;
    let tools = tools.clone();
    Ok(single_pdf_job(
        "pdfOcr",
        input,
        destination,
        move |ctx, input, temp| async move {
            // 1. Render every page (0-30 %).
            let pages_dir = ctx.temp_dir.join("pages");
            std::fs::create_dir(&pages_dir)?;
            let task = WorkerTask::Render {
                input: input.clone(),
                pages: None,
                dpi: DEFAULT_OCR_DPI,
                format: RenderFormat::Png,
                quality: 100,
                out_dir: pages_dir.clone(),
            };
            pdf_worker::run(&tools, &ctx, task, (0.0, 30.0)).await?;
            let mut images: Vec<PathBuf> = std::fs::read_dir(&pages_dir)?
                .flatten()
                .map(|e| e.path())
                .collect();
            images.sort();

            // 2. Recognise each page into a text-only PDF page (30-95 %).
            let text_dir = ctx.temp_dir.join("text");
            std::fs::create_dir(&text_dir)?;
            let mut text_pages = Vec::with_capacity(images.len());
            for (index, image) in images.iter().enumerate() {
                ctx.check_cancelled()?;
                let base = text_dir.join(format!("page-{:05}", index + 1));
                let mut cmd = tools.command(ToolId::Tesseract)?;
                cmd.arg(image)
                    .arg(&base)
                    .args(["-l", &languages, "--dpi", &DEFAULT_OCR_DPI.to_string()])
                    .args(["-c", "textonly_pdf=1", "pdf"]);
                run_streaming(cmd, |_| {}).await?;
                text_pages.push(base.with_extension("pdf"));
                let done = (index + 1) as f32 / images.len().max(1) as f32;
                ctx.set_progress(30.0 + 65.0 * done);
            }
            if text_pages.is_empty() {
                return Err(AppError::InvalidFile("the PDF has no pages".into()));
            }

            // 3. Lay the text pages over the original ones.
            let text_pdf = ctx.temp_dir.join("text.pdf");
            let mut cmd = qpdf::command(&tools)?;
            cmd.arg("--empty")
                .arg("--pages")
                .args(&text_pages)
                .arg("--")
                .arg(&text_pdf);
            qpdf::run(cmd).await?;
            let mut cmd = qpdf::command(&tools)?;
            cmd.arg(&input)
                .arg("--overlay")
                .arg(&text_pdf)
                .arg("--")
                .arg(&temp);
            qpdf::run(cmd).await
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_never_show_in_debug_output() {
        let params: PdfProtectParams = serde_json::from_value(serde_json::json!({
            "input": "/a.pdf", "userPassword": "hunter2", "ownerPassword": "s3cret"
        }))
        .unwrap();
        let text = format!("{params:?}");
        assert!(
            !text.contains("hunter2") && !text.contains("s3cret"),
            "{text}"
        );
        let params: PdfUnlockParams = serde_json::from_value(serde_json::json!({
            "input": "/a.pdf", "password": "hunter2"
        }))
        .unwrap();
        assert!(!format!("{params:?}").contains("hunter2"));
    }

    #[test]
    fn languages_must_be_installed_and_well_formed() {
        let installed = vec!["eng".to_owned(), "por".to_owned(), "osd".to_owned()];
        assert_eq!(
            validate_languages(&["por".into(), "eng".into()], &installed).unwrap(),
            "por+eng"
        );
        assert!(validate_languages(&[], &installed).is_err());
        assert!(validate_languages(&["osd".into()], &installed).is_err());
        assert!(validate_languages(&["deu".into()], &installed).is_err());
        assert!(validate_languages(&["eng+por".into()], &installed).is_err());
        assert!(validate_languages(&vec!["eng".into(); 6], &installed).is_err());
    }

    #[test]
    fn ghostscript_output_escapes_percent_signs() {
        let args =
            ghostscript_args(Path::new("/in 100%.pdf"), Path::new("/tmp/out%d.pdf")).unwrap();
        assert!(args.contains(&"-dSAFER".to_owned()));
        assert!(args.contains(&"-sOutputFile=/tmp/out%%d.pdf".to_owned()));
        assert_eq!(args.last().unwrap(), "/in 100%.pdf");
    }

    #[test]
    fn pdf_inputs_need_extension_and_header() {
        let dir = std::env::temp_dir().join(format!("ec-pdf-validate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("a.pdf");
        std::fs::write(&good, b"junk%PDF-1.7\n").unwrap();
        assert!(validate_pdf(&good).is_ok());
        let fake = dir.join("b.pdf");
        std::fs::write(&fake, b"hello").unwrap();
        assert!(matches!(validate_pdf(&fake), Err(AppError::InvalidFile(_))));
        let wrong_ext = dir.join("c.txt");
        std::fs::write(&wrong_ext, b"%PDF-1.7").unwrap();
        assert!(matches!(
            validate_pdf(&wrong_ext),
            Err(AppError::InvalidParameter(_))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
