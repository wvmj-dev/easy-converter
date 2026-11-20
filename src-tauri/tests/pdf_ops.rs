//! PDF operations end to end, with the real qpdf, PDFium worker, Tesseract and
//! Ghostscript. A test is skipped (with a message) when a tool it needs is missing.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::*;
use easy_converter_lib::error::ErrorCode;
use easy_converter_lib::tools::{qpdf, ToolId};
use image::{Rgba, RgbaImage};
use serde_json::json;

async fn env(name: &str) -> Env {
    Env::new(name).with_tools().await
}

/// Text of a PDF, extracted by the PDFium worker.
async fn text_of(env: &Env, pdf: &Path) -> String {
    let job = env
        .run_json(json!({ "type": "pdfExtractText", "input": pdf }))
        .await;
    fs::read_to_string(assert_done(&job)).unwrap()
}

/// Renders one page with the PDFium worker and decodes it.
async fn render_page(env: &Env, pdf: &Path, page: u32, dpi: u32) -> image::RgbImage {
    let job = env
        .run_json(json!({
            "type": "pdfToImages", "input": pdf, "format": "png", "dpi": dpi,
            "pages": [{ "start": page, "end": page }]
        }))
        .await;
    let out = assert_done(&job);
    let image = decode(&out).to_rgb8();
    fs::remove_file(out).unwrap();
    image
}

fn is_red(p: &image::Rgb<u8>) -> bool {
    p[0] > 180 && p[1] < 90 && p[2] < 90
}

fn is_blue(p: &image::Rgb<u8>) -> bool {
    p[2] > 180 && p[0] < 90 && p[1] < 90
}

/// Landscape picture: left half red, right half blue.
fn red_blue(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, _| {
        if x < width / 2 {
            Rgba([230, 20, 20, 255])
        } else {
            Rgba([20, 20, 230, 255])
        }
    })
}

#[tokio::test]
async fn images_become_pages_with_jpeg_kept_and_orientation_applied() {
    let env = env("img2pdf").await;
    let photo = env.path("photo.jpg");
    // Stored landscape, EXIF says "rotate 90° clockwise": shown portrait, red on top.
    write_jpeg_with_exif(&photo, &red_blue(64, 32), 6);
    let png = env.path("alpha.png");
    write_png(&png, &rgba_pattern(40, 30), None);

    let job = env
        .run_json(json!({
            "type": "imagesToPdf", "inputs": [photo, png],
            "pageSize": "image", "marginMm": 0
        }))
        .await;
    let pdf = assert_done(&job);
    assert_eq!(pdf, env.path("photo-convertido.pdf"));
    let bytes = fs::read(&pdf).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.7"));

    // The JPEG image data is embedded byte for byte, without re-encoding...
    let jpeg = fs::read(&photo).unwrap();
    let sos = jpeg.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    let eoi = jpeg.windows(2).rposition(|w| w == [0xFF, 0xD9]).unwrap();
    assert!(contains(&bytes, &jpeg[sos..eoi + 2]));
    // ...but without its metadata (location, camera, comments).
    assert!(!contains(&bytes, SECRET));

    if !(env.has(ToolId::Qpdf) && env.has(ToolId::Pdfium)) {
        return;
    }
    assert_eq!(qpdf_pages(&pdf), 2);
    // Page 1 is portrait (32 × 64 at 96 dpi) with the red half on top.
    let page = render_page(&env, &pdf, 1, 96).await;
    assert_eq!((page.width(), page.height()), (32, 64));
    assert!(is_red(page.get_pixel(16, 8)), "{:?}", page.get_pixel(16, 8));
    assert!(
        is_blue(page.get_pixel(16, 56)),
        "{:?}",
        page.get_pixel(16, 56)
    );
}

#[tokio::test]
async fn merge_split_select_and_rotate_with_qpdf() {
    let env = env("structure").await;
    if !(env.has(ToolId::Qpdf) && env.has(ToolId::Pdfium)) {
        return;
    }
    let a = env.path("a.pdf");
    let b = env.path("b.pdf");
    write_text_pdf(&a, 2);
    write_text_pdf(&b, 1);

    // Merge: 2 + 1 pages.
    let merged = assert_done(
        &env.run_json(json!({ "type": "pdfMerge", "inputs": [a, b] }))
            .await,
    );
    assert_eq!(qpdf_pages(&merged), 3);

    // Split every page: three files named by page.
    let parts = assert_done_many(
        &env.run_json(
            json!({ "type": "pdfSplit", "input": merged, "split": { "mode": "everyPage" } }),
        )
        .await,
    );
    assert_eq!(parts.len(), 3);
    for part in &parts {
        assert_eq!(qpdf_pages(part), 1);
    }
    let names: Vec<String> = parts
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names[0], "a-convertido-convertido-p1.pdf");

    // Split by ranges.
    let parts = assert_done_many(
        &env.run_json(json!({
            "type": "pdfSplit", "input": merged,
            "split": { "mode": "ranges", "ranges": [{ "start": 1, "end": 2 }, { "start": 3, "end": 3 }] }
        }))
        .await,
    );
    assert_eq!(
        parts.iter().map(|p| qpdf_pages(p)).collect::<Vec<_>>(),
        vec![2, 1]
    );

    // Select pages 2 then 1 (reorder): the text order follows.
    let selected = assert_done(
        &env.run_json(json!({
            "type": "pdfSelectPages", "input": a,
            "pages": [{ "start": 2, "end": 1 }]
        }))
        .await,
    );
    let text = text_of(&env, &selected).await;
    let (p1, p2) = (text.find("PAGE 1").unwrap(), text.find("PAGE 2").unwrap());
    assert!(p2 < p1, "{text}");

    // Rotate only page 1: it renders landscape, page 2 stays portrait.
    let rotated = assert_done(
        &env.run_json(json!({
            "type": "pdfRotate", "input": a, "rotation": "cw90",
            "pages": [{ "start": 1, "end": 1 }]
        }))
        .await,
    );
    let first = render_page(&env, &rotated, 1, 36).await;
    let second = render_page(&env, &rotated, 2, 36).await;
    assert!(first.width() > first.height());
    assert!(second.width() < second.height());

    // A page beyond the end is a parameter error, not a crash.
    let job = env
        .run_json(
            json!({ "type": "pdfSelectPages", "input": a, "pages": [{ "start": 9, "end": 9 }] }),
        )
        .await;
    assert_failed(&job, ErrorCode::InvalidParameter);
}

#[tokio::test]
async fn text_images_and_page_rendering_with_the_pdfium_worker() {
    let env = env("pdfium").await;
    if !env.has(ToolId::Pdfium) {
        return;
    }
    let doc = env.path("doc.pdf");
    write_text_pdf(&doc, 3);
    let text = text_of(&env, &doc).await;
    for n in 1..=3 {
        assert!(text.contains(&format!("PAGE {n} HELLO")), "{text}");
    }

    // Pages to JPEG at 72 dpi: A4 is 595 × 842 points.
    let pages = assert_done_many(
        &env.run_json(json!({
            "type": "pdfToImages", "input": doc, "format": "jpeg", "dpi": 72, "quality": 80,
            "pages": [{ "start": 3, "end": 2 }]
        }))
        .await,
    );
    let names: Vec<String> = pages
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["doc-convertido-p002.jpg", "doc-convertido-p003.jpg"]
    );
    let page = decode(&pages[0]);
    assert_eq!((page.width(), page.height()), (595, 842));
    assert_eq!(format_of(&pages[0]), image::ImageFormat::Jpeg);

    // Images embedded by images-to-PDF come back out.
    let pic = env.path("pic.png");
    write_png(&pic, &red_blue(50, 20), None);
    let pdf = assert_done(
        &env.run_json(json!({ "type": "imagesToPdf", "inputs": [pic, pic], "pageSize": "a4", "marginMm": 10 }))
            .await,
    );
    let images = assert_done_many(
        &env.run_json(json!({ "type": "pdfExtractImages", "input": pdf }))
            .await,
    );
    assert_eq!(images.len(), 2);
    let first = decode(&images[0]).to_rgb8();
    assert_eq!((first.width(), first.height()), (50, 20));
    assert!(is_red(first.get_pixel(5, 5)) && is_blue(first.get_pixel(45, 5)));
}

#[tokio::test]
async fn damaged_pdfs_fail_cleanly_in_the_worker() {
    let env = env("damaged").await;
    if !env.has(ToolId::Pdfium) {
        return;
    }
    let bad = env.path("bad.pdf");
    fs::write(&bad, b"%PDF-1.7\nthis is not really a PDF\n%%EOF").unwrap();
    let job = env
        .run_json(json!({ "type": "pdfExtractText", "input": bad }))
        .await;
    assert_failed(&job, ErrorCode::InvalidFile);

    let truncated = env.path("cut.pdf");
    write_text_pdf(&truncated, 2);
    let data = fs::read(&truncated).unwrap();
    fs::write(&truncated, &data[..data.len() / 3]).unwrap();
    let job = env
        .run_json(json!({ "type": "pdfToImages", "input": truncated, "format": "png", "dpi": 50 }))
        .await;
    // PDFium either recovers the readable pages or reports a damaged file; it never
    // takes the app down.
    assert!(matches!(
        job.status,
        easy_converter_lib::jobs::JobStatus::Done | easy_converter_lib::jobs::JobStatus::Failed
    ));
}

#[tokio::test]
async fn protect_and_unlock_keep_passwords_off_the_command_line() {
    let env = env("password").await;
    if !(env.has(ToolId::Qpdf) && env.has(ToolId::Pdfium)) {
        return;
    }
    let doc = env.path("doc.pdf");
    write_text_pdf(&doc, 1);
    let protected = assert_done(
        &env.run_json(json!({
            "type": "pdfProtect", "input": doc, "userPassword": "abre te sésamo",
            "ownerPassword": "dono", "noPrint": true, "noCopy": true
        }))
        .await,
    );
    assert_eq!(qpdf_requires_password(&protected), 0);
    // Encrypted content: the text is not readable in the file.
    assert!(!contains(&fs::read(&protected).unwrap(), b"PAGE 1 HELLO"));
    // No temporary password file is left behind.
    assert!(fs::read_dir(env.queue.temp().root())
        .unwrap()
        .next()
        .is_none());

    let info = qpdf::info(&env.tools, &protected).await.unwrap();
    assert!(info.encrypted && info.needs_password && info.pages.is_none());

    // Without the password, PDFium and qpdf report it as a password problem.
    let job = env
        .run_json(json!({ "type": "pdfExtractText", "input": protected }))
        .await;
    assert_failed(&job, ErrorCode::Password);
    let job = env
        .run_json(json!({ "type": "pdfUnlock", "input": protected, "password": "errada" }))
        .await;
    assert_failed(&job, ErrorCode::Password);

    let unlocked = assert_done(
        &env.run_json(
            json!({ "type": "pdfUnlock", "input": protected, "password": "abre te sésamo" }),
        )
        .await,
    );
    assert_eq!(qpdf_requires_password(&unlocked), 2);
    assert!(text_of(&env, &unlocked).await.contains("PAGE 1 HELLO"));
    let info = qpdf::info(&env.tools, &unlocked).await.unwrap();
    assert_eq!(info.pages, Some(1));
    assert!(!info.encrypted);
}

#[tokio::test]
async fn compression_never_returns_a_bigger_file() {
    let env = env("compress").await;
    if !env.has(ToolId::Qpdf) {
        return;
    }
    let pic = env.path("pic.png");
    write_png(&pic, &photo_pattern(600, 400), None);
    let pdf = assert_done(
        &env.run_json(
            json!({ "type": "imagesToPdf", "inputs": [pic], "pageSize": "a4", "marginMm": 0 }),
        )
        .await,
    );
    let original = fs::metadata(&pdf).unwrap().len();
    let standard = assert_done(
        &env.run_json(json!({ "type": "pdfCompress", "input": pdf, "level": "standard" }))
            .await,
    );
    assert!(fs::metadata(&standard).unwrap().len() <= original);
    assert_eq!(qpdf_pages(&standard), 1);

    if !env.has(ToolId::Ghostscript) {
        return;
    }
    let strong = assert_done(
        &env.run_json(json!({ "type": "pdfCompress", "input": pdf, "level": "strong" }))
            .await,
    );
    assert!(fs::metadata(&strong).unwrap().len() <= original);
    assert_eq!(qpdf_pages(&strong), 1);
}

#[tokio::test]
async fn ocr_adds_searchable_text_to_scanned_pages() {
    let env = env("ocr").await;
    if !(env.has(ToolId::Qpdf) && env.has(ToolId::Pdfium) && env.has(ToolId::Tesseract)) {
        return;
    }
    // A "scanned" PDF: a text page rendered to an image and put back as an image page.
    let source = env.path("source.pdf");
    write_text_pdf(&source, 1);
    let scan = assert_done(
        &env.run_json(json!({
            "type": "pdfToImages", "input": source, "format": "png", "dpi": 150
        }))
        .await,
    );
    let scanned = assert_done(
        &env.run_json(
            json!({ "type": "imagesToPdf", "inputs": [scan], "pageSize": "a4", "marginMm": 0 }),
        )
        .await,
    );
    assert!(!text_of(&env, &scanned).await.contains("HELLO"));

    let ocr = assert_done(
        &env.run_json(json!({ "type": "pdfOcr", "input": scanned, "languages": ["eng"] }))
            .await,
    );
    assert_eq!(qpdf_pages(&ocr), 1);
    let text = text_of(&env, &ocr).await;
    assert!(text.contains("HELLO"), "OCR text: {text:?}");

    // Languages that are not installed are refused before queueing.
    let request = serde_json::from_value(json!({
        "type": "pdfOcr", "input": scanned, "languages": ["xyz"]
    }))
    .unwrap();
    assert!(easy_converter_lib::ops::build_job(request, &env.ops_context()).is_err());
}

#[tokio::test]
async fn missing_tools_are_reported_before_queueing() {
    let env = Env::new("no-tools");
    let doc = env.path("doc.pdf");
    write_text_pdf(&doc, 1);
    for request in [
        json!({ "type": "pdfMerge", "inputs": [doc, doc] }),
        json!({ "type": "pdfExtractText", "input": doc }),
        json!({ "type": "pdfCompress", "input": doc, "level": "strong" }),
    ] {
        let request = serde_json::from_value(request).unwrap();
        let Err(err) = easy_converter_lib::ops::build_job(request, &env.ops_context()) else {
            panic!("a job was built without its tool");
        };
        assert_eq!(err.code(), ErrorCode::ToolMissing);
    }
    // Images to PDF needs no external tool.
    let pic: PathBuf = env.path("p.png");
    write_png(&pic, &rgba_pattern(4, 4), None);
    let request = serde_json::from_value(
        json!({ "type": "imagesToPdf", "inputs": [pic], "pageSize": "image", "marginMm": 0 }),
    )
    .unwrap();
    assert!(easy_converter_lib::ops::build_job(request, &env.ops_context()).is_ok());
}
