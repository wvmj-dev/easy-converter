//! PDFium runs in a separate process: the app's own executable started with
//! [`WORKER_FLAG`]. A damaged or malicious PDF that crashes or hangs PDFium only takes
//! down that process, and cancelling or timing out a job kills it right away, like any
//! other external tool.
//!
//! Protocol:
//! - the app writes a [`WorkerRequest`] as JSON to a file in the job's temporary folder
//!   and passes its path as the only argument (nothing sensitive on the command line);
//! - the worker prints `progress <done> <total>` lines on stdout while it works;
//! - on failure it prints one `EC-WORKER-ERROR <code> <message>` line on stderr and exits
//!   with status 2. The app turns that line back into an [`AppError`].

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat};
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::jobs::JobContext;
use crate::ops::pdf::pages::{self, PageRange};
use crate::tools::process::run_streaming;
use crate::tools::ToolBox;

/// First argument that starts the executable as a PDFium worker instead of the app.
pub const WORKER_FLAG: &str = "--pdfium-worker";
const ERROR_MARKER: &str = "EC-WORKER-ERROR";
const PROGRESS_PREFIX: &str = "progress ";

/// Largest rendered page, in pixels (about 10 000 × 10 000). Larger pages are scaled down.
pub const MAX_RENDER_PIXELS: f32 = 100_000_000.0;
/// Most images extracted from one document.
pub const MAX_EXTRACTED_IMAGES: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderFormat {
    Png,
    Jpeg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "task", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkerTask {
    /// Renders pages to `out_dir/page-00001.<ext>` (original page numbers).
    #[serde(rename_all = "camelCase")]
    Render {
        input: PathBuf,
        pages: Option<Vec<PageRange>>,
        dpi: u32,
        format: RenderFormat,
        quality: u8,
        out_dir: PathBuf,
    },
    /// Writes the text of every page to `out_file`, pages separated by a blank line.
    #[serde(rename_all = "camelCase")]
    ExtractText { input: PathBuf, out_file: PathBuf },
    /// Saves embedded images as `out_dir/page-00001-image-001.png`.
    #[serde(rename_all = "camelCase")]
    ExtractImages { input: PathBuf, out_dir: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerRequest {
    pub library: PathBuf,
    pub task: WorkerTask,
}

// ---------------------------------------------------------------------------------------
// App side
// ---------------------------------------------------------------------------------------

/// Runs `task` in a worker process. Its progress fills `span` (from, to) of the job's
/// progress, so a job with several steps never goes backwards.
pub async fn run(
    tools: &ToolBox,
    ctx: &JobContext,
    task: WorkerTask,
    span: (f32, f32),
) -> AppResult<()> {
    let (mut cmd, library) = tools.pdfium_worker()?;
    let request_path = ctx.temp_dir.join("worker-request.json");
    let request = WorkerRequest { library, task };
    let json = serde_json::to_vec(&request).map_err(|e| AppError::Internal(e.to_string()))?;
    fs::write(&request_path, json)?;
    cmd.arg(WORKER_FLAG).arg(&request_path);

    let result = run_streaming(cmd, |line| {
        if let Some((done, total)) = parse_progress(line) {
            if total > 0 {
                let fraction = done.min(total) as f32 / total as f32;
                ctx.set_progress(span.0 + (span.1 - span.0) * fraction);
            }
        }
    })
    .await;
    let _ = fs::remove_file(&request_path);
    result.map(|_| ()).map_err(map_worker_error)
}

fn parse_progress(line: &str) -> Option<(usize, usize)> {
    let (done, total) = line.strip_prefix(PROGRESS_PREFIX)?.split_once(' ')?;
    Some((done.trim().parse().ok()?, total.trim().parse().ok()?))
}

/// Recovers the error reported by the worker from the process failure message.
fn map_worker_error(err: AppError) -> AppError {
    let AppError::ProcessFailed(message) = &err else {
        return err;
    };
    let Some(at) = message.find(ERROR_MARKER) else {
        return err;
    };
    let line = message[at + ERROR_MARKER.len()..]
        .lines()
        .next()
        .unwrap_or_default()
        .trim();
    let (code, detail) = line.split_once(' ').unwrap_or((line, ""));
    let detail = detail.to_owned();
    match code {
        "invalidFile" => AppError::InvalidFile(detail),
        "password" => AppError::Password(detail),
        "tooLarge" => AppError::TooLarge(detail),
        "invalidParameter" => AppError::InvalidParameter(detail),
        "toolMissing" => AppError::ToolMissing(detail),
        "io" => AppError::Io(std::io::Error::other(detail)),
        _ => AppError::ProcessFailed(detail),
    }
}

// ---------------------------------------------------------------------------------------
// Worker side
// ---------------------------------------------------------------------------------------

/// When the process was started as a worker, does the work and returns the exit code.
/// Called first thing in `main`, before anything of the app is initialised.
pub fn run_if_worker() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != WORKER_FLAG {
        return None;
    }
    let (Some(request), None) = (args.next(), args.next()) else {
        report(&AppError::InvalidParameter(
            "usage: --pdfium-worker <request.json>".into(),
        ));
        return Some(2);
    };
    match serve(Path::new(&request)) {
        Ok(()) => Some(0),
        Err(err) => {
            report(&err);
            Some(2)
        }
    }
}

fn report(err: &AppError) {
    let code = match err {
        AppError::InvalidFile(_) => "invalidFile",
        AppError::Password(_) => "password",
        AppError::TooLarge(_) => "tooLarge",
        AppError::InvalidParameter(_) => "invalidParameter",
        AppError::ToolMissing(_) => "toolMissing",
        AppError::Io(_) => "io",
        _ => "failed",
    };
    let message = err.to_string().replace(['\n', '\r'], " ");
    eprintln!("{ERROR_MARKER} {code} {message}");
}

fn serve(request_path: &Path) -> AppResult<()> {
    let data = fs::read(request_path)?;
    let request: WorkerRequest = serde_json::from_slice(&data)
        .map_err(|e| AppError::InvalidParameter(format!("bad worker request: {e}")))?;
    let bindings = Pdfium::bind_to_library(&request.library)
        .map_err(|e| AppError::ToolMissing(format!("PDFium could not be loaded: {e}")))?;
    let pdfium = Pdfium::new(bindings);

    match request.task {
        WorkerTask::Render {
            input,
            pages,
            dpi,
            format,
            quality,
            out_dir,
        } => render(
            &pdfium,
            &input,
            pages.as_deref(),
            dpi,
            format,
            quality,
            &out_dir,
        ),
        WorkerTask::ExtractText { input, out_file } => extract_text(&pdfium, &input, &out_file),
        WorkerTask::ExtractImages { input, out_dir } => extract_images(&pdfium, &input, &out_dir),
    }
}

fn open<'a>(pdfium: &'a Pdfium, input: &Path) -> AppResult<PdfDocument<'a>> {
    pdfium.load_pdf_from_file(input, None).map_err(pdfium_error)
}

fn pdfium_error(err: PdfiumError) -> AppError {
    match err {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            AppError::Password("the PDF is protected by a password".into())
        }
        PdfiumError::PdfiumLibraryInternalError(
            PdfiumInternalError::FormatError | PdfiumInternalError::FileError,
        ) => AppError::InvalidFile("not a valid PDF file".into()),
        PdfiumError::IoError(e) => AppError::Io(e),
        other => AppError::InvalidFile(other.to_string()),
    }
}

fn progress(done: usize, total: usize) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{PROGRESS_PREFIX}{done} {total}");
    let _ = out.flush();
}

fn page_count(document: &PdfDocument) -> u32 {
    u32::try_from(document.pages().len()).unwrap_or(0)
}

/// Size in pixels of a page rendered at `dpi`, scaled down to stay under the limit.
pub fn render_size(width_pt: f32, height_pt: f32, dpi: u32) -> (i32, i32) {
    let scale = dpi as f32 / 72.0;
    let (mut w, mut h) = (width_pt.max(1.0) * scale, height_pt.max(1.0) * scale);
    let pixels = w * h;
    if pixels > MAX_RENDER_PIXELS {
        let shrink = (MAX_RENDER_PIXELS / pixels).sqrt();
        w *= shrink;
        h *= shrink;
    }
    (w.round().max(1.0) as i32, h.round().max(1.0) as i32)
}

#[allow(clippy::too_many_arguments)]
fn render(
    pdfium: &Pdfium,
    input: &Path,
    selection: Option<&[PageRange]>,
    dpi: u32,
    format: RenderFormat,
    quality: u8,
    out_dir: &Path,
) -> AppResult<()> {
    let document = open(pdfium, input)?;
    let indices = pages::indices(selection, page_count(&document))?;
    let total = indices.len();
    for (done, index) in indices.into_iter().enumerate() {
        let page_index = PdfPageIndex::try_from(index)
            .map_err(|_| AppError::InvalidParameter("page out of range".into()))?;
        let page = document.pages().get(page_index).map_err(pdfium_error)?;
        let (width, height) = render_size(page.width().value, page.height().value, dpi);
        let config = PdfRenderConfig::new()
            .set_target_width(width)
            .set_target_height(height)
            .set_clear_color(PdfColor::WHITE)
            .render_form_data(true);
        let bitmap = page.render_with_config(&config).map_err(pdfium_error)?;
        let image = bitmap.as_image().map_err(pdfium_error)?;
        let name = format!("page-{:05}", index + 1);
        match format {
            RenderFormat::Png => save_png(&image, &out_dir.join(format!("{name}.png")))?,
            RenderFormat::Jpeg => save_jpeg(&image, quality, &out_dir.join(format!("{name}.jpg")))?,
        }
        progress(done + 1, total);
    }
    Ok(())
}

fn save_png(image: &DynamicImage, path: &Path) -> AppResult<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    image
        .write_to(&mut writer, ImageFormat::Png)
        .map_err(|e| AppError::Internal(format!("encoding failed: {e}")))?;
    writer.flush()?;
    Ok(())
}

fn save_jpeg(image: &DynamicImage, quality: u8, path: &Path) -> AppResult<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    JpegEncoder::new_with_quality(&mut writer, quality.clamp(1, 100))
        .encode_image(&image.to_rgb8())
        .map_err(|e| AppError::Internal(format!("encoding failed: {e}")))?;
    writer.flush()?;
    Ok(())
}

fn extract_text(pdfium: &Pdfium, input: &Path, out_file: &Path) -> AppResult<()> {
    let document = open(pdfium, input)?;
    let total = page_count(&document) as usize;
    let mut writer = BufWriter::new(File::create(out_file)?);
    for (index, page) in document.pages().iter().enumerate() {
        if index > 0 {
            writer.write_all(b"\n\n")?;
        }
        let text = page.text().map_err(pdfium_error)?.all();
        writer.write_all(text.trim_end().as_bytes())?;
        progress(index + 1, total);
    }
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn extract_images(pdfium: &Pdfium, input: &Path, out_dir: &Path) -> AppResult<()> {
    let document = open(pdfium, input)?;
    let total = page_count(&document) as usize;
    let mut saved = 0usize;
    for (index, page) in document.pages().iter().enumerate() {
        let mut on_page = 0usize;
        for object in page.objects().iter() {
            let Some(image_object) = object.as_image_object() else {
                continue;
            };
            // Images PDFium cannot decode (rare filters) are skipped, not fatal.
            let Ok(image) = image_object.get_raw_image() else {
                continue;
            };
            if saved >= MAX_EXTRACTED_IMAGES {
                return Err(AppError::TooLarge(format!(
                    "the document has more than {MAX_EXTRACTED_IMAGES} images"
                )));
            }
            on_page += 1;
            saved += 1;
            let name = format!("page-{:05}-image-{:03}.png", index + 1, on_page);
            save_png(&image, &out_dir.join(name))?;
        }
        progress(index + 1, total);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_lines() {
        assert_eq!(parse_progress("progress 3 10"), Some((3, 10)));
        assert_eq!(parse_progress("progress x 10"), None);
        assert_eq!(parse_progress("other"), None);
    }

    #[test]
    fn maps_worker_errors_back() {
        let failed = |line: &str| {
            map_worker_error(AppError::ProcessFailed(format!(
                "process exited with exit status: 2: {line}"
            )))
        };
        assert!(matches!(
            failed("EC-WORKER-ERROR password the PDF is protected"),
            AppError::Password(m) if m == "the PDF is protected"
        ));
        assert!(matches!(
            failed("EC-WORKER-ERROR invalidFile bad"),
            AppError::InvalidFile(_)
        ));
        assert!(matches!(
            failed("EC-WORKER-ERROR toolMissing PDFium could not be loaded"),
            AppError::ToolMissing(_)
        ));
        assert!(matches!(
            failed("segmentation fault"),
            AppError::ProcessFailed(_)
        ));
        assert!(matches!(
            map_worker_error(AppError::Timeout),
            AppError::Timeout
        ));
    }

    #[test]
    fn render_size_follows_dpi_and_caps_huge_pages() {
        // A4 at 150 dpi.
        assert_eq!(render_size(595.0, 842.0, 150), (1240, 1754));
        // A 5 m × 5 m poster at 600 dpi is scaled down under the pixel limit.
        let (w, h) = render_size(14_173.0, 14_173.0, 600);
        assert!((w as f32) * (h as f32) <= MAX_RENDER_PIXELS * 1.001);
        assert_eq!(w, h);
    }

    #[test]
    fn requests_round_trip_and_reject_unknown_fields() {
        let request = WorkerRequest {
            library: "/lib/libpdfium.so".into(),
            task: WorkerTask::ExtractText {
                input: "/a.pdf".into(),
                out_file: "/a.txt".into(),
            },
        };
        let json = serde_json::to_string(&request).unwrap();
        let back: WorkerRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.task, WorkerTask::ExtractText { .. }));
        let evil = json.replace("\"outFile\"", "\"extra\":1,\"outFile\"");
        assert!(serde_json::from_str::<WorkerRequest>(&evil).is_err());
    }
}
