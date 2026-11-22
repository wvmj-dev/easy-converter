//! Presentation operations (phase 4), with LibreOffice (optional).
//!
//! - PPT, PPTX and ODP between themselves and to PDF.
//! - To images: LibreOffice makes a PDF in the job's temporary folder, then PDFium renders
//!   it in the isolated worker, one image per slide.

use std::path::PathBuf;

use serde::Deserialize;
use ts_rs::TS;

use super::image as img;
use super::office;
use super::pdf::{numbered_outputs, validate_dpi, PdfImageFormat};
use super::{process_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::jobs::{JobCategory, JobSpec, PlannedOutput};
use crate::pdf_worker::{self, RenderFormat, WorkerTask};
use crate::tools::libreoffice::{self, Conversion};
use crate::tools::{ToolBox, ToolId};

const PRESENTATION_EXTENSIONS: [&str; 3] = ["ppt", "pptx", "odp"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PresentationFormat {
    Pdf,
    Pptx,
    Odp,
}

impl PresentationFormat {
    fn extension(self) -> &'static str {
        match self {
            PresentationFormat::Pdf => "pdf",
            PresentationFormat::Pptx => "pptx",
            PresentationFormat::Odp => "odp",
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PresentationConvertParams {
    pub input: PathBuf,
    pub format: PresentationFormat,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PresentationToImagesParams {
    pub input: PathBuf,
    pub format: PdfImageFormat,
    pub dpi: u32,
    /// JPEG quality, 1 to 100.
    #[serde(default)]
    #[ts(optional)]
    pub quality: Option<u8>,
}

pub fn convert(
    params: PresentationConvertParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    office::require(tools, ToolId::Libreoffice)?;
    let (input, ext) =
        office::validate_input(&params.input, &PRESENTATION_EXTENSIONS, "presentation")?;
    let format = params.format;
    if ext == format.extension() {
        return Err(AppError::InvalidParameter(format!(
            "the file is already {}",
            format.extension().to_uppercase()
        )));
    }
    let destination = planner.destination(&input, format.extension())?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "presentationConvert",
        JobCategory::Presentation,
        vec![input],
        move |ctx| async move {
            ctx.set_indeterminate();
            let conversion = Conversion {
                input: &work_input,
                convert_to: format.extension(),
                infilter: None,
            };
            let temp = libreoffice::convert(&tools, &ctx, conversion).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn to_images(
    params: PresentationToImagesParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    office::require(tools, ToolId::Libreoffice)?;
    tools.pdfium_worker()?;
    let (input, _) =
        office::validate_input(&params.input, &PRESENTATION_EXTENSIONS, "presentation")?;
    let dpi = validate_dpi(params.dpi)?;
    let quality = img::validate_quality(params.quality.unwrap_or(img::DEFAULT_JPEG_QUALITY))?;
    let (format, ext) = match params.format {
        PdfImageFormat::Png => (RenderFormat::Png, "png"),
        PdfImageFormat::Jpeg => (RenderFormat::Jpeg, "jpg"),
    };
    planner.destination_part(&input, "001", ext)?;
    let planner = planner.clone();
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "presentationToImages",
        JobCategory::Presentation,
        vec![input],
        move |ctx| async move {
            ctx.set_indeterminate();
            let conversion = Conversion {
                input: &work_input,
                convert_to: "pdf",
                infilter: None,
            };
            let pdf = libreoffice::convert(&tools, &ctx, conversion).await?;
            ctx.set_progress(30.0);
            let out_dir = libreoffice::fresh_dir(&ctx.temp_dir, "slides")?;
            let task = WorkerTask::Render {
                input: pdf,
                pages: None,
                dpi,
                format,
                quality,
                out_dir: out_dir.clone(),
            };
            pdf_worker::run(&tools, &ctx, task, (30.0, 100.0)).await?;
            numbered_outputs(&out_dir, &planner, &work_input, ext, |name| {
                // page-00012 → 012
                let slide: u32 = name.strip_prefix("page-")?.parse().ok()?;
                Some(format!("{slide:03}"))
            })
        },
    ))
}
