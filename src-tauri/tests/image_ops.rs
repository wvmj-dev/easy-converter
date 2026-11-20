//! End-to-end tests of the image operations: request JSON → validation → queue → files.

mod common;

use std::fs;

use common::*;
use easy_converter_lib::error::{AppError, ErrorCode};
use easy_converter_lib::jobs::JobRequest;
use easy_converter_lib::ops;
use image::{GenericImageView, ImageFormat};
use serde_json::json;

#[tokio::test]
async fn converts_png_with_transparency_to_jpeg_on_white() {
    let env = Env::new("convert");
    let input = env.path("logo.png");
    write_png(&input, &rgba_pattern(64, 32), None);

    let job = env
        .run_json(
            json!({ "type": "imageConvert", "input": input, "format": "jpeg", "quality": 80 }),
        )
        .await;

    let output = assert_done(&job);
    assert_eq!(output, env.path("logo-convertido.jpg"));
    assert_eq!(format_of(&output), ImageFormat::Jpeg);
    let img = decode(&output).to_rgb8();
    assert_eq!(img.dimensions(), (64, 32));
    // The transparent half became white instead of black.
    let px = img.get_pixel(60, 16);
    assert!(px.0.iter().all(|&c| c > 240), "{px:?}");
    assert!(input.exists(), "the original is kept");
}

#[tokio::test]
async fn converting_to_every_format_works() {
    let env = Env::new("formats");
    let input = env.path("pic.png");
    write_png(&input, &rgba_pattern(40, 20), None);

    for (format, expected) in [
        ("png", ImageFormat::Png),
        ("jpeg", ImageFormat::Jpeg),
        ("webp", ImageFormat::WebP),
        ("bmp", ImageFormat::Bmp),
        ("gif", ImageFormat::Gif),
        ("tiff", ImageFormat::Tiff),
        ("ico", ImageFormat::Ico),
    ] {
        let job = env
            .run_json(json!({ "type": "imageConvert", "input": input, "format": format }))
            .await;
        let output = assert_done(&job);
        assert_eq!(format_of(&output), expected, "{format}");
        assert_eq!(decode(&output).dimensions(), (40, 20), "{format}");
    }
}

#[tokio::test]
async fn exif_orientation_is_applied_before_converting() {
    let env = Env::new("orientation");
    let input = env.path("phone.jpg");
    // Stored landscape (60x30) with "rotate 90° clockwise" in EXIF: upright is 30x60.
    write_jpeg_with_exif(&input, &photo_pattern(60, 30), 6);

    let job = env
        .run_json(json!({ "type": "imageConvert", "input": input, "format": "png" }))
        .await;

    let output = assert_done(&job);
    assert_eq!(decode(&output).dimensions(), (30, 60));
    assert!(!contains(&fs::read(&output).unwrap(), SECRET));
}

#[tokio::test]
async fn icc_profile_is_kept() {
    let env = Env::new("icc");
    let input = env.path("wide-gamut.png");
    let profile = b"fake-icc-profile-for-testing".repeat(4);
    write_png(&input, &photo_pattern(16, 16), Some(&profile));

    let job = env
        .run_json(json!({ "type": "imageConvert", "input": input, "format": "jpeg" }))
        .await;

    let output = fs::read(assert_done(&job)).unwrap();
    assert!(contains(&output, b"ICC_PROFILE\0"));
    assert!(contains(&output, &profile));
}

#[tokio::test]
async fn resizes_in_every_mode_and_keeps_the_format_and_extension() {
    let env = Env::new("resize");
    let input = env.path("photo.jpeg");
    fs::write(&input, jpeg_bytes(&photo_pattern(200, 100), 90)).unwrap();

    let cases = [
        (
            json!({ "mode": "exact", "width": 50, "height": 70 }),
            (50, 70),
        ),
        (
            json!({ "mode": "fit", "width": 80, "height": 80 }),
            (80, 40),
        ),
        (json!({ "mode": "percent", "percent": 150 }), (300, 150)),
    ];
    for (resize, expected) in cases {
        let job = env
            .run_json(json!({ "type": "imageResize", "input": input, "resize": resize }))
            .await;
        let output = assert_done(&job);
        assert_eq!(output.extension().unwrap(), "jpeg");
        assert_eq!(format_of(&output), ImageFormat::Jpeg);
        assert_eq!(decode(&output).dimensions(), expected);
    }
}

#[tokio::test]
async fn crops_and_rejects_areas_outside_the_image() {
    let env = Env::new("crop");
    let input = env.path("a.png");
    let source = rgba_pattern(50, 40);
    write_png(&input, &source, None);

    let job = env
        .run_json(json!({ "type": "imageCrop", "input": input, "x": 10, "y": 5, "width": 20, "height": 30 }))
        .await;
    let output = decode(&assert_done(&job)).to_rgba8();
    assert_eq!(output.dimensions(), (20, 30));
    assert_eq!(output.get_pixel(0, 0), source.get_pixel(10, 5));

    let job = env
        .run_json(json!({ "type": "imageCrop", "input": input, "x": 40, "y": 0, "width": 20, "height": 10 }))
        .await;
    assert_failed(&job, ErrorCode::InvalidParameter);
}

#[tokio::test]
async fn rotates_and_flips() {
    let env = Env::new("rotate");
    let input = env.path("a.png");
    let source = rgba_pattern(30, 10);
    write_png(&input, &source, None);

    let job = env
        .run_json(json!({ "type": "imageRotate", "input": input, "rotation": "cw90" }))
        .await;
    let rotated = decode(&assert_done(&job)).to_rgba8();
    assert_eq!(rotated.dimensions(), (10, 30));
    // Clockwise: the top-left pixel moves to the top-right corner.
    assert_eq!(rotated.get_pixel(9, 0), source.get_pixel(0, 0));

    let job = env
        .run_json(json!({ "type": "imageFlip", "input": input, "axis": "horizontal" }))
        .await;
    let flipped = decode(&assert_done(&job)).to_rgba8();
    assert_eq!(flipped.get_pixel(29, 3), source.get_pixel(0, 3));
}

#[tokio::test]
async fn compression_reduces_jpeg_and_never_grows_a_file() {
    let env = Env::new("compress");
    let jpeg = env.path("big.jpg");
    fs::write(&jpeg, jpeg_bytes(&photo_pattern(300, 300), 98)).unwrap();
    let original_size = fs::metadata(&jpeg).unwrap().len();

    let job = env
        .run_json(json!({ "type": "imageCompress", "input": jpeg, "quality": 40 }))
        .await;
    let output = assert_done(&job);
    assert!(fs::metadata(&output).unwrap().len() < original_size / 2);

    // A tiny, already optimal file: the result is never bigger than the original.
    let tiny = env.path("tiny.gif");
    fs::write(
        &tiny,
        b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;",
    )
    .unwrap();
    let job = env
        .run_json(json!({ "type": "imageCompress", "input": tiny, "quality": 80 }))
        .await;
    let output = assert_done(&job);
    assert!(fs::metadata(&output).unwrap().len() <= fs::metadata(&tiny).unwrap().len());
}

#[tokio::test]
async fn strips_metadata_without_touching_pixels() {
    let env = Env::new("strip");
    let input = env.path("phone.jpg");
    write_jpeg_with_exif(&input, &photo_pattern(48, 24), 6);
    let original = fs::read(&input).unwrap();

    let job = env
        .run_json(json!({ "type": "imageStripMetadata", "input": input }))
        .await;

    let output = assert_done(&job);
    let stripped = fs::read(&output).unwrap();
    assert!(!contains(&stripped, SECRET));
    assert!(stripped.len() < original.len());
    assert_eq!(
        fs::read(&input).unwrap(),
        original,
        "the original is untouched"
    );
    // Same pixels (no re-encoding) and still shown upright.
    assert_eq!(decode(&output).to_rgb8(), decode(&input).to_rgb8());
    let info = ops::image::probe(&output).unwrap();
    assert_eq!((info.width, info.height), (24, 48));
}

#[tokio::test]
async fn strip_metadata_rejects_formats_without_support() {
    let env = Env::new("strip-bmp");
    let input = env.path("a.png");
    write_png(&input, &rgba_pattern(4, 4), None);
    let bmp = env
        .run_json(json!({ "type": "imageConvert", "input": input, "format": "bmp" }))
        .await;
    let bmp = assert_done(&bmp);

    let request: JobRequest =
        serde_json::from_value(json!({ "type": "imageStripMetadata", "input": bmp })).unwrap();
    assert!(matches!(
        ops::build_job(request, &env.ops_context()),
        Err(AppError::InvalidParameter(_))
    ));
}

#[tokio::test]
async fn merges_side_by_side_and_stacked() {
    let env = Env::new("merge");
    let a = env.path("a.png");
    let b = env.path("b.png");
    let c = env.path("c.jpg");
    write_png(&a, &rgba_pattern(40, 20), None);
    write_png(&b, &rgba_pattern(10, 40), None);
    fs::write(&c, jpeg_bytes(&photo_pattern(30, 30), 90)).unwrap();

    let job = env
        .run_json(json!({
            "type": "imageMerge", "inputs": [a, b, c], "direction": "horizontal",
            "format": "png", "matchSize": false, "spacing": 5
        }))
        .await;
    let output = assert_done(&job);
    assert_eq!(output, env.path("a-convertido.png"));
    assert_eq!(decode(&output).dimensions(), (40 + 10 + 30 + 2 * 5, 40));

    let job = env
        .run_json(json!({
            "type": "imageMerge", "inputs": [a, b, c], "direction": "vertical",
            "format": "jpeg", "matchSize": true, "spacing": 0
        }))
        .await;
    // Every image scaled to the narrowest width (10): heights 5 + 40 + 10.
    assert_eq!(decode(&assert_done(&job)).dimensions(), (10, 55));
}

#[tokio::test]
async fn never_overwrites_existing_files() {
    let env = Env::new("numbering");
    let input = env.path("x.png");
    write_png(&input, &rgba_pattern(8, 8), None);
    fs::write(env.path("x-convertido.webp"), b"keep me").unwrap();

    let request = json!({ "type": "imageConvert", "input": input, "format": "webp" });
    let first = assert_done(&env.run_json(request.clone()).await);
    let second = assert_done(&env.run_json(request).await);

    assert_eq!(first, env.path("x-convertido (2).webp"));
    assert_eq!(second, env.path("x-convertido (3).webp"));
    assert_eq!(fs::read(env.path("x-convertido.webp")).unwrap(), b"keep me");
}

#[tokio::test]
async fn rejects_invalid_requests_before_queueing() {
    let env = Env::new("invalid");
    let fake = env.path("fake.png");
    fs::write(&fake, "this is text, not a picture").unwrap();
    let text = env.path("notes.txt");
    fs::write(&text, "hello").unwrap();
    let real = env.path("real.png");
    write_png(&real, &rgba_pattern(8, 8), None);

    let build = |value: serde_json::Value| {
        let request: JobRequest = serde_json::from_value(value)?;
        Ok::<_, serde_json::Error>(ops::build_job(request, &env.ops_context()))
    };

    let cases = [
        (
            json!({ "type": "imageConvert", "input": fake, "format": "png" }),
            ErrorCode::InvalidFile,
        ),
        (
            json!({ "type": "imageConvert", "input": text, "format": "png" }),
            ErrorCode::InvalidParameter,
        ),
        (
            json!({ "type": "imageConvert", "input": "relative.png", "format": "png" }),
            ErrorCode::InvalidPath,
        ),
        (
            json!({ "type": "imageConvert", "input": env.path("gone.png"), "format": "png" }),
            ErrorCode::NotFound,
        ),
        (
            json!({ "type": "imageConvert", "input": real, "format": "png", "quality": 0 }),
            ErrorCode::InvalidParameter,
        ),
        (
            json!({ "type": "imageResize", "input": real, "resize": { "mode": "exact", "width": 0, "height": 5 } }),
            ErrorCode::InvalidParameter,
        ),
        (
            json!({ "type": "imageResize", "input": real, "resize": { "mode": "percent", "percent": 5000 } }),
            ErrorCode::InvalidParameter,
        ),
        (
            json!({ "type": "imageMerge", "inputs": [real], "direction": "vertical", "format": "png", "matchSize": false, "spacing": 0 }),
            ErrorCode::InvalidParameter,
        ),
        (
            json!({ "type": "imageMerge", "inputs": [real, real], "direction": "vertical", "format": "png", "matchSize": false, "spacing": 99999 }),
            ErrorCode::InvalidParameter,
        ),
    ];
    for (value, code) in cases {
        let result = build(value.clone()).expect("JSON should parse");
        assert_eq!(result.err().map(|e| e.code()), Some(code), "{value}");
    }

    // Unknown fields and unknown operations are rejected by the deserializer.
    assert!(build(
        json!({ "type": "imageConvert", "input": real, "format": "png", "shell": "rm -rf /" })
    )
    .is_err());
    assert!(build(json!({ "type": "runCommand", "input": real })).is_err());
    assert!(build(json!({ "type": "imageConvert", "input": real, "format": "exe" })).is_err());
}

#[tokio::test]
async fn probe_reports_upright_size_and_real_format() {
    let env = Env::new("probe");
    let disguised = env.path("actually-jpeg.png");
    write_jpeg_with_exif(&disguised, &photo_pattern(20, 10), 8);

    let info = easy_converter_lib::fs_utils::probe(&disguised).unwrap();
    let image = info.image.unwrap();
    assert_eq!((image.width, image.height), (10, 20));
    assert_eq!(image.format, ops::image::ImageOutputFormat::Jpeg);

    // Resizing keeps the real format and fixes the extension.
    let job = env
        .run_json(json!({ "type": "imageResize", "input": disguised, "resize": { "mode": "percent", "percent": 50 } }))
        .await;
    assert_eq!(assert_done(&job), env.path("actually-jpeg-convertido.jpg"));
}

#[tokio::test]
async fn truncated_jpeg_fails_instead_of_turning_grey() {
    let env = Env::new("truncated");
    let input = env.path("cut.jpg");
    let full = jpeg_bytes(&photo_pattern(64, 48), 90);
    fs::write(&input, &full[..full.len() * 2 / 3]).unwrap();

    let job = env
        .run_json(json!({ "type": "imageConvert", "input": input, "format": "png" }))
        .await;
    assert_failed(&job, ErrorCode::InvalidFile);
    assert!(!env.path("cut-convertido.png").exists());

    // Other formats cut short are reported as damaged files too, not as disk errors.
    let png = env.path("cut.png");
    write_png(&png, &photo_pattern(64, 48), None);
    let data = fs::read(&png).unwrap();
    fs::write(&png, &data[..data.len() / 2]).unwrap();
    let job = env
        .run_json(json!({ "type": "imageConvert", "input": png, "format": "jpeg" }))
        .await;
    assert_failed(&job, ErrorCode::InvalidFile);
}
