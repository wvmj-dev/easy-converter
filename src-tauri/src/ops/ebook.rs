//! Ebook operations (phase 5): EPUB, MOBI, AZW3, FB2 and PDF with Calibre's
//! `ebook-convert` (optional).

use std::path::PathBuf;

use serde::Deserialize;
use ts_rs::TS;

use super::office;
use super::{process_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::jobs::{JobCategory, JobSpec, PlannedOutput};
use crate::tools::{calibre, ToolBox, ToolId};

const EBOOK_EXTENSIONS: [&str; 5] = ["epub", "mobi", "azw3", "azw", "fb2"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EbookFormat {
    Epub,
    Mobi,
    Azw3,
    Pdf,
}

impl EbookFormat {
    fn extension(self) -> &'static str {
        match self {
            EbookFormat::Epub => "epub",
            EbookFormat::Mobi => "mobi",
            EbookFormat::Azw3 => "azw3",
            EbookFormat::Pdf => "pdf",
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct EbookConvertParams {
    pub input: PathBuf,
    pub format: EbookFormat,
}

pub fn convert(
    params: EbookConvertParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    office::require(tools, ToolId::Calibre)?;
    let (input, ext) = office::validate_input(&params.input, &EBOOK_EXTENSIONS, "ebook")?;
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
        "ebookConvert",
        JobCategory::Ebook,
        vec![input],
        move |ctx| async move {
            let temp = ctx.temp_dir.join(format!("output.{}", format.extension()));
            calibre::convert(&tools, &ctx, &work_input, &temp).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}
