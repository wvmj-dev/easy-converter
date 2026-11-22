//! Spreadsheet operations (phase 4).
//!
//! - XLS, XLSX, ODS and CSV between themselves and to PDF: LibreOffice (optional). CSV
//!   input is read with the detected delimiter and encoding; CSV output is UTF-8 and holds
//!   the first sheet only.
//! - CSV ↔ JSON: Rust (see `csv_json`).

mod csv_json;

use std::path::PathBuf;

use serde::Deserialize;
use ts_rs::TS;

use super::office::{self, detect_encoding, read_head};
use super::{blocking_job, process_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::jobs::{JobCategory, JobSpec, PlannedOutput};
use crate::tools::libreoffice::{self, Conversion};
use crate::tools::{ToolBox, ToolId};

use csv_json::{CsvToJsonOptions, JsonToCsvOptions};

const SHEET_EXTENSIONS: [&str; 4] = ["xls", "xlsx", "ods", "csv"];

// ---------------------------------------------------------------------------------------
// Request parameters
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SpreadsheetFormat {
    Xlsx,
    Ods,
    Csv,
    Pdf,
}

impl SpreadsheetFormat {
    fn extension(self) -> &'static str {
        match self {
            SpreadsheetFormat::Xlsx => "xlsx",
            SpreadsheetFormat::Ods => "ods",
            SpreadsheetFormat::Csv => "csv",
            SpreadsheetFormat::Pdf => "pdf",
        }
    }

    fn office_filter(self) -> &'static str {
        match self {
            SpreadsheetFormat::Xlsx => "xlsx",
            SpreadsheetFormat::Ods => "ods",
            // Comma, double quotes, UTF-8 (76), from the first line.
            SpreadsheetFormat::Csv => "csv:Text - txt - csv (StarCalc):44,34,76,1",
            SpreadsheetFormat::Pdf => "pdf",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CsvDelimiter {
    Comma,
    Semicolon,
    Tab,
    Pipe,
}

impl CsvDelimiter {
    fn byte(self) -> u8 {
        match self {
            CsvDelimiter::Comma => b',',
            CsvDelimiter::Semicolon => b';',
            CsvDelimiter::Tab => b'\t',
            CsvDelimiter::Pipe => b'|',
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SpreadsheetConvertParams {
    pub input: PathBuf,
    pub format: SpreadsheetFormat,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CsvToJsonParams {
    pub input: PathBuf,
    /// Detected from the file when absent.
    #[serde(default)]
    #[ts(optional)]
    pub delimiter: Option<CsvDelimiter>,
    /// The first row names the columns (objects); otherwise rows become arrays.
    pub header: bool,
    /// Numbers and `true`/`false` become JSON values, empty cells `null`.
    pub typed: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct JsonToCsvParams {
    pub input: PathBuf,
    pub delimiter: CsvDelimiter,
    /// UTF-8 byte order mark, so Excel shows accents correctly.
    pub bom: bool,
    /// Prefix text that looks like a formula with `'`, so it never runs when opened.
    pub escape_formulas: bool,
}

// ---------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------

pub fn convert(
    params: SpreadsheetConvertParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    office::require(tools, ToolId::Libreoffice)?;
    let (input, ext) = office::validate_input(&params.input, &SHEET_EXTENSIONS, "spreadsheet")?;
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
        "spreadsheetConvert",
        JobCategory::Spreadsheet,
        vec![input],
        move |ctx| async move {
            ctx.set_indeterminate();
            let infilter = if ext == "csv" {
                let head = read_head(&work_input)?;
                let encoding = detect_encoding(&head);
                let text = csv_json::decode(head);
                let delimiter = csv_json::sniff_delimiter(text.as_bytes());
                Some(format!(
                    "CSV:{},34,{},1",
                    delimiter,
                    encoding.libreoffice_csv_code()
                ))
            } else {
                None
            };
            let conversion = Conversion {
                input: &work_input,
                convert_to: format.office_filter(),
                infilter: infilter.as_deref(),
            };
            let temp = libreoffice::convert(&tools, &ctx, conversion).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn csv_to_json(params: CsvToJsonParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let (input, _) = office::validate_input(&params.input, &["csv"], "CSV file")?;
    let destination = planner.destination(&input, "json")?;
    let options = CsvToJsonOptions {
        delimiter: params.delimiter.map(CsvDelimiter::byte),
        header: params.header,
        typed: params.typed,
    };
    let work_input = input.clone();
    Ok(blocking_job(
        "csvToJson",
        JobCategory::Spreadsheet,
        vec![input],
        move |ctx| {
            let temp = ctx.temp_dir.join("output.json");
            csv_json::csv_to_json(&work_input, &temp, options, &|| ctx.check_cancelled())?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn json_to_csv(params: JsonToCsvParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let (input, _) = office::validate_input(&params.input, &["json"], "JSON file")?;
    let destination = planner.destination(&input, "csv")?;
    let options = JsonToCsvOptions {
        delimiter: params.delimiter.byte(),
        bom: params.bom,
        escape_formulas: params.escape_formulas,
    };
    let work_input = input.clone();
    Ok(blocking_job(
        "jsonToCsv",
        JobCategory::Spreadsheet,
        vec![input],
        move |ctx| {
            let temp = ctx.temp_dir.join("output.csv");
            csv_json::json_to_csv(&work_input, &temp, options, &|| ctx.check_cancelled())?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}
