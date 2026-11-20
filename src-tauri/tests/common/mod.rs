//! Shared helpers for integration tests: a job queue that records events and fixtures
//! generated on the fly (no binary files in the repository).

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use easy_converter_lib::error::ErrorCode;
use easy_converter_lib::fs_utils::TempSpace;
use easy_converter_lib::jobs::{EventSink, Job, JobEvent, JobId, JobQueue, JobRequest, JobStatus};
use easy_converter_lib::ops::{self, OpsContext};
use easy_converter_lib::settings::Settings;
use easy_converter_lib::tools::resolver::ResolveContext;
use easy_converter_lib::tools::{ToolBox, ToolId, ToolRegistry};
use image::{ImageEncoder, Rgba, RgbaImage};

/// Text that must never survive metadata removal.
pub const SECRET: &[u8] = b"SECRET-GPS-AND-SERIAL";

#[derive(Default)]
pub struct RecordingSink {
    events: Mutex<Vec<(JobEvent, Job)>>,
}

impl EventSink for RecordingSink {
    fn emit(&self, event: JobEvent, job: &Job) {
        self.events.lock().unwrap().push((event, job.clone()));
    }
}

/// A scratch folder with an input area and a job queue writing to it.
pub struct Env {
    pub dir: PathBuf,
    pub settings: Settings,
    pub tools: ToolBox,
    pub queue: JobQueue,
    sink: Arc<RecordingSink>,
}

impl Env {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir()
            .join(format!("ec-it-{name}-{}", std::process::id()))
            .canonicalize_or_create();
        let settings = Settings::default();
        let sink = Arc::new(RecordingSink::default());
        let temp = TempSpace::create(&dir.join(".work")).unwrap();
        let config_settings = settings.clone();
        let queue = JobQueue::new(
            temp,
            sink.clone(),
            Arc::new(move || config_settings.clone()),
        );
        Self {
            dir,
            settings,
            tools: ToolBox::default(),
            queue,
            sink,
        }
    }

    /// Resolves the real tools: system qpdf, Tesseract and Ghostscript, the PDFium
    /// downloaded by `scripts/fetch-pdfium.sh`, and the app binary as PDFium worker.
    pub async fn with_tools(mut self) -> Self {
        let ctx = ResolveContext {
            resource_dir: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")),
            ..ResolveContext::default()
        };
        let worker = PathBuf::from(env!("CARGO_BIN_EXE_easy-converter"));
        self.tools = ToolRegistry::default().toolbox(&ctx, Some(worker)).await;
        self
    }

    /// Whether a tool is available; prints why a test is skipped when it is not.
    pub fn has(&self, id: ToolId) -> bool {
        let available = self.tools.is_available(id);
        if !available {
            eprintln!("skipping: {id:?} is not available");
        }
        available
    }

    pub fn ops_context(&self) -> OpsContext {
        OpsContext {
            settings: self.settings.clone(),
            tools: self.tools.clone(),
        }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Validates and enqueues a request, then waits for every job to finish.
    pub async fn run(&self, request: JobRequest) -> Job {
        let spec = ops::build_job(request, &self.ops_context()).expect("request should be valid");
        let id = self.queue.enqueue(spec).unwrap();
        self.queue.wait_idle().await;
        self.final_of(id)
    }

    pub async fn run_json(&self, json: serde_json::Value) -> Job {
        self.run(serde_json::from_value(json).expect("request JSON should parse"))
            .await
    }

    pub fn final_of(&self, id: JobId) -> Job {
        self.sink
            .events
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(e, j)| j.id == id && matches!(e, JobEvent::Finished | JobEvent::Failed))
            .map(|(_, j)| j.clone())
            .expect("job should have finished")
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        self.queue.temp().remove_all();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

trait CanonicalizeOrCreate {
    fn canonicalize_or_create(self) -> PathBuf;
}

impl CanonicalizeOrCreate for PathBuf {
    fn canonicalize_or_create(self) -> PathBuf {
        let _ = fs::remove_dir_all(&self);
        fs::create_dir_all(&self).unwrap();
        self.canonicalize().unwrap()
    }
}

pub fn assert_done(job: &Job) -> PathBuf {
    assert_eq!(job.status, JobStatus::Done, "job failed: {:?}", job.error);
    assert_eq!(job.outputs.len(), 1);
    job.outputs[0].clone()
}

/// Outputs of a job that must have succeeded.
pub fn assert_done_many(job: &Job) -> Vec<PathBuf> {
    assert_eq!(job.status, JobStatus::Done, "job failed: {:?}", job.error);
    job.outputs.clone()
}

pub fn assert_failed(job: &Job, code: ErrorCode) {
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(
        job.error.as_ref().map(|e| e.code),
        Some(code),
        "{:?}",
        job.error
    );
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

// ---------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------

/// Gradient with a transparent right half.
pub fn rgba_pattern(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        let alpha = if x >= width / 2 { 0 } else { 255 };
        Rgba([
            (x * 255 / width.max(1)) as u8,
            (y * 255 / height.max(1)) as u8,
            120,
            alpha,
        ])
    })
}

/// Opaque photo-like pattern that JPEG compresses well but not trivially.
pub fn photo_pattern(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        let v = ((x * 7 + y * 13) ^ (x * y)) as u8;
        Rgba([v, v.wrapping_mul(3), (x + y) as u8, 255])
    })
}

pub fn write_png(path: &Path, image: &RgbaImage, icc: Option<&[u8]>) {
    let mut data = Vec::new();
    let mut encoder = image::codecs::png::PngEncoder::new(&mut data);
    if let Some(profile) = icc {
        encoder.set_icc_profile(profile.to_vec()).unwrap();
    }
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    fs::write(path, data).unwrap();
}

pub fn jpeg_bytes(image: &RgbaImage, quality: u8) -> Vec<u8> {
    let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
    let mut data = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, quality)
        .encode_image(&rgb)
        .unwrap();
    data
}

/// JPEG with an EXIF block (orientation plus secret text), an XMP block, a comment and
/// trailing data after the end marker — like photos straight from a phone.
pub fn write_jpeg_with_exif(path: &Path, image: &RgbaImage, orientation: u16) {
    let plain = jpeg_bytes(image, 92);
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II*\0");
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(SECRET);

    let segment = |marker: u8, payload: &[u8]| {
        let mut s = vec![0xFF, marker];
        s.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        s.extend_from_slice(payload);
        s
    };
    let mut exif = b"Exif\0\0".to_vec();
    exif.extend(tiff);
    let mut xmp = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    xmp.extend_from_slice(SECRET);

    let app0_len = usize::from(u16::from_be_bytes([plain[4], plain[5]]));
    let split = 4 + app0_len;
    let mut out = plain[..split].to_vec();
    out.extend(segment(0xE1, &exif));
    out.extend(segment(0xE1, &xmp));
    out.extend(segment(0xFE, SECRET));
    out.extend_from_slice(&plain[split..]);
    out.extend_from_slice(SECRET);
    fs::write(path, out).unwrap();
}

pub fn decode(path: &Path) -> image::DynamicImage {
    image::ImageReader::open(path)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
}

pub fn format_of(path: &Path) -> image::ImageFormat {
    image::ImageReader::open(path)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .format()
        .unwrap()
}

/// PDF with one line of large text per page: "PAGE <n> HELLO".
pub fn write_text_pdf(path: &Path, pages: u32) {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let mut kids = Vec::new();
    for n in 1..=pages {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 40.into()]),
                Operation::new("Td", vec![60.into(), 700.into()]),
                Operation::new(
                    "Tj",
                    vec![Object::string_literal(format!("PAGE {n} HELLO"))],
                ),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => i64::from(pages),
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).unwrap();
}

/// Page count as reported by qpdf.
pub fn qpdf_pages(path: &Path) -> u32 {
    let out = std::process::Command::new("qpdf")
        .arg("--show-npages")
        .arg(path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

/// Exit code of `qpdf --requires-password` (0 = needs a password, 2 = not encrypted).
pub fn qpdf_requires_password(path: &Path) -> i32 {
    std::process::Command::new("qpdf")
        .arg("--requires-password")
        .arg(path)
        .status()
        .unwrap()
        .code()
        .unwrap()
}
