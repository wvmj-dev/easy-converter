//! Image operations (phase 1), done in Rust with the `image` crate.
//!
//! - The format is always detected from the file content, never trusted from the extension.
//! - Decoding has size and memory limits, so a crafted file cannot exhaust memory.
//! - The EXIF orientation is applied to the pixels before any change, because re-encoded
//!   images do not carry EXIF. The ICC colour profile is kept when the output format
//!   supports it.
//! - Animated GIF, WebP and PNG files are reduced to their first frame (except for
//!   metadata removal, which keeps every frame).
//! - Truncated JPEG files are refused: the decoder would silently fill the missing part
//!   with grey and the job would look successful.

mod integrity;
mod metadata;

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};

use image::codecs::bmp::BmpEncoder;
use image::codecs::gif::GifEncoder;
use image::codecs::ico::IcoEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::codecs::tiff::TiffEncoder;
use image::codecs::webp::WebPEncoder;
use image::imageops::{self, FilterType};
use image::metadata::Orientation;
use image::{
    ColorType, DynamicImage, ImageBuffer, ImageDecoder, ImageEncoder, ImageError, ImageFormat,
    ImageReader, Limits, Rgba, RgbaImage,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{blocking_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::fs_utils;
use crate::jobs::{JobCategory, JobContext, JobSpec, PlannedOutput};

/// Largest width or height accepted, for inputs and outputs.
pub const MAX_DIMENSION: u32 = 65_535;
/// Largest number of pixels of an output image (16384 × 16384, 1 GiB as RGBA).
pub const MAX_OUTPUT_PIXELS: u64 = 16_384 * 16_384;
/// Memory a decoder may allocate for one image.
const MAX_DECODE_ALLOC: u64 = 2 * 1024 * 1024 * 1024;
/// Largest file read fully into memory (metadata removal).
const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;
/// Most images that can be joined in one job.
pub const MAX_MERGE_INPUTS: usize = 50;
/// Largest gap between joined images, in pixels.
pub const MAX_MERGE_SPACING: u32 = 2_000;
/// Largest scale for the percent resize mode.
pub const MAX_RESIZE_PERCENT: u32 = 1_000;
/// JPEG quality used when an operation does not ask for one.
pub const DEFAULT_JPEG_QUALITY: u8 = 90;
/// ICO images cannot be larger than this; bigger inputs are scaled down to fit.
const ICO_MAX_SIZE: u32 = 256;
/// GIF quantization speed (1 = slowest and best, 30 = fastest). 10 is a good balance.
const GIF_SPEED: i32 = 10;
/// Resampling filter: close to Lanczos in quality and noticeably faster.
const RESIZE_FILTER: FilterType = FilterType::CatmullRom;

// ---------------------------------------------------------------------------------------
// Request parameters
// ---------------------------------------------------------------------------------------

/// Formats the app can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ImageOutputFormat {
    Png,
    Jpeg,
    Webp,
    Bmp,
    Gif,
    Tiff,
    Ico,
}

impl ImageOutputFormat {
    /// Extension used for new files.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Bmp => "bmp",
            Self::Gif => "gif",
            Self::Tiff => "tiff",
            Self::Ico => "ico",
        }
    }

    /// Every extension that names this format.
    fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Png => &["png"],
            Self::Jpeg => &["jpg", "jpeg", "jpe", "jfif"],
            Self::Webp => &["webp"],
            Self::Bmp => &["bmp", "dib"],
            Self::Gif => &["gif"],
            Self::Tiff => &["tiff", "tif"],
            Self::Ico => &["ico"],
        }
    }

    fn from_detected(format: ImageFormat) -> Option<Self> {
        Some(match format {
            ImageFormat::Png => Self::Png,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::WebP => Self::Webp,
            ImageFormat::Bmp => Self::Bmp,
            ImageFormat::Gif => Self::Gif,
            ImageFormat::Tiff => Self::Tiff,
            ImageFormat::Ico => Self::Ico,
            _ => return None,
        })
    }
}

/// Converts to another format. `quality` (1 to 100) applies to JPEG only.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ImageConvertParams {
    pub input: PathBuf,
    pub format: ImageOutputFormat,
    #[serde(default)]
    #[ts(optional)]
    pub quality: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "camelCase")]
#[ts(export)]
pub enum ResizeMode {
    /// Exactly this size; the proportions may change.
    Exact { width: u32, height: u32 },
    /// As large as possible inside this box, keeping the proportions.
    Fit { width: u32, height: u32 },
    /// Scales both sides by a percentage (1 to 1000).
    Percent { percent: u32 },
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ImageResizeParams {
    pub input: PathBuf,
    pub resize: ResizeMode,
}

/// Keeps the rectangle starting at (`x`, `y`), in pixels of the upright image.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ImageCropParams {
    pub input: PathBuf,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Clockwise rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Rotation {
    Cw90,
    Cw180,
    Cw270,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ImageRotateParams {
    pub input: PathBuf,
    pub rotation: Rotation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FlipAxis {
    /// Mirror left to right.
    Horizontal,
    /// Mirror top to bottom.
    Vertical,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ImageFlipParams {
    pub input: PathBuf,
    pub axis: FlipAxis,
}

/// Re-encodes in the same format to reduce the size. JPEG uses `quality` (1 to 100); PNG
/// uses the strongest lossless compression; other formats are re-encoded. When the result
/// is not smaller, the original content is kept.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ImageCompressParams {
    pub input: PathBuf,
    pub quality: u8,
}

/// Removes EXIF, XMP, IPTC, comments and similar metadata. Lossless for JPEG, PNG and
/// WebP; TIFF is re-encoded without loss.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ImageStripMetadataParams {
    pub input: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MergeDirection {
    /// Side by side, left to right.
    Horizontal,
    /// Stacked, top to bottom.
    Vertical,
}

/// Joins images in the given order into a single one.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ImageMergeParams {
    pub inputs: Vec<PathBuf>,
    pub direction: MergeDirection,
    pub format: ImageOutputFormat,
    /// Scales the images to the same height (side by side) or width (stacked), using the
    /// smallest one so nothing is enlarged.
    pub match_size: bool,
    /// Gap between images, in pixels (transparent, or white in formats without alpha).
    pub spacing: u32,
}

/// Image details returned by `probe_file`. The size is the upright size, after the EXIF
/// orientation is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: ImageOutputFormat,
}

// ---------------------------------------------------------------------------------------
// Job builders: validate the request, then return the job that runs it
// ---------------------------------------------------------------------------------------

pub fn convert(params: ImageConvertParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let input = validate_input(&params.input)?;
    detect_format(&input)?;
    let quality = validate_quality(params.quality.unwrap_or(DEFAULT_JPEG_QUALITY))?;
    let format = params.format;
    let destination = planner.destination(&input, format.extension())?;
    let options = EncodeOptions::with_quality(quality);
    Ok(single_input_job(
        "imageConvert",
        input,
        move |ctx, input| transform_one(ctx, input, destination, format, options, Ok),
    ))
}

pub fn resize(params: ImageResizeParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let input = validate_input(&params.input)?;
    let mode = params.resize;
    match mode {
        ResizeMode::Exact { width, height } | ResizeMode::Fit { width, height } => {
            validate_dimension("width", width)?;
            validate_dimension("height", height)?;
        }
        ResizeMode::Percent { percent } => {
            if !(1..=MAX_RESIZE_PERCENT).contains(&percent) {
                return Err(AppError::InvalidParameter(format!(
                    "percent must be between 1 and {MAX_RESIZE_PERCENT}"
                )));
            }
        }
    }
    let (format, destination) = same_format_destination(&input, planner)?;
    Ok(single_input_job("imageResize", input, move |ctx, input| {
        transform_one(
            ctx,
            input,
            destination,
            format,
            EncodeOptions::default(),
            |img| {
                let (width, height) = target_size(img.width(), img.height(), mode)?;
                Ok(resize_image(&img, width, height))
            },
        )
    }))
}

pub fn crop(params: ImageCropParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let input = validate_input(&params.input)?;
    validate_dimension("width", params.width)?;
    validate_dimension("height", params.height)?;
    let (x, y, width, height) = (params.x, params.y, params.width, params.height);
    let (format, destination) = same_format_destination(&input, planner)?;
    Ok(single_input_job("imageCrop", input, move |ctx, input| {
        transform_one(
            ctx,
            input,
            destination,
            format,
            EncodeOptions::default(),
            |img| {
                let fits_x = x.checked_add(width).is_some_and(|end| end <= img.width());
                let fits_y = y.checked_add(height).is_some_and(|end| end <= img.height());
                if !fits_x || !fits_y {
                    return Err(AppError::InvalidParameter(format!(
                        "crop area {width}x{height} at ({x}, {y}) is outside the {}x{} image",
                        img.width(),
                        img.height()
                    )));
                }
                Ok(img.crop_imm(x, y, width, height))
            },
        )
    }))
}

pub fn rotate(params: ImageRotateParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let input = validate_input(&params.input)?;
    let rotation = params.rotation;
    let (format, destination) = same_format_destination(&input, planner)?;
    Ok(single_input_job("imageRotate", input, move |ctx, input| {
        transform_one(
            ctx,
            input,
            destination,
            format,
            EncodeOptions::default(),
            |img| {
                Ok(match rotation {
                    Rotation::Cw90 => img.rotate90(),
                    Rotation::Cw180 => img.rotate180(),
                    Rotation::Cw270 => img.rotate270(),
                })
            },
        )
    }))
}

pub fn flip(params: ImageFlipParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let input = validate_input(&params.input)?;
    let axis = params.axis;
    let (format, destination) = same_format_destination(&input, planner)?;
    Ok(single_input_job("imageFlip", input, move |ctx, input| {
        transform_one(
            ctx,
            input,
            destination,
            format,
            EncodeOptions::default(),
            |img| {
                Ok(match axis {
                    FlipAxis::Horizontal => img.fliph(),
                    FlipAxis::Vertical => img.flipv(),
                })
            },
        )
    }))
}

pub fn compress(params: ImageCompressParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let input = validate_input(&params.input)?;
    let quality = validate_quality(params.quality)?;
    let (format, destination) = same_format_destination(&input, planner)?;
    let options = EncodeOptions {
        jpeg_quality: quality,
        png_compression: CompressionType::Best,
    };
    Ok(single_input_job(
        "imageCompress",
        input,
        move |ctx, input| {
            let planned = transform_one(ctx, input, destination, format, options, Ok)?;
            // Never hand back a "compressed" file that is bigger than the original.
            for output in &planned {
                if fs::metadata(&output.temp)?.len() >= fs::metadata(input)?.len() {
                    fs::copy(input, &output.temp)?;
                }
            }
            Ok(planned)
        },
    ))
}

pub fn strip_metadata(
    params: ImageStripMetadataParams,
    planner: &OutputPlanner,
) -> AppResult<JobSpec> {
    let input = validate_input(&params.input)?;
    let (format, destination) = same_format_destination(&input, planner)?;
    let container = match format {
        ImageOutputFormat::Jpeg => Some(metadata::Container::Jpeg),
        ImageOutputFormat::Png => Some(metadata::Container::Png),
        ImageOutputFormat::Webp => Some(metadata::Container::WebP),
        ImageOutputFormat::Tiff => None,
        _ => {
            return Err(AppError::InvalidParameter(
                "metadata removal supports JPEG, PNG, WebP and TIFF".into(),
            ))
        }
    };
    Ok(single_input_job(
        "imageStripMetadata",
        input,
        move |ctx, input| {
            let Some(container) = container else {
                // TIFF keeps its metadata in the same structure as the pixels: re-encode.
                return transform_one(
                    ctx,
                    input,
                    destination,
                    format,
                    EncodeOptions::default(),
                    Ok,
                );
            };
            ctx.set_progress(10.0);
            let data = read_limited(input)?;
            let orientation = orientation_of(&data)?;
            ctx.check_cancelled()?;
            let stripped = metadata::strip(container, &data, orientation)?;
            ctx.set_progress(80.0);
            let temp = temp_output(ctx, format);
            fs::write(&temp, stripped)?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn merge(params: ImageMergeParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    if !(2..=MAX_MERGE_INPUTS).contains(&params.inputs.len()) {
        return Err(AppError::InvalidParameter(format!(
            "between 2 and {MAX_MERGE_INPUTS} images can be joined"
        )));
    }
    if params.spacing > MAX_MERGE_SPACING {
        return Err(AppError::InvalidParameter(format!(
            "spacing must be at most {MAX_MERGE_SPACING} pixels"
        )));
    }
    let inputs = params
        .inputs
        .iter()
        .map(|path| validate_input(path))
        .collect::<AppResult<Vec<_>>>()?;
    for input in &inputs {
        detect_format(input)?;
    }
    let format = params.format;
    let destination = planner.destination(&inputs[0], format.extension())?;
    let layout = MergeLayout {
        direction: params.direction,
        match_size: params.match_size,
        spacing: params.spacing,
    };
    let work_inputs = inputs.clone();
    Ok(blocking_job(
        "imageMerge",
        JobCategory::Image,
        inputs,
        move |ctx| {
            let (image, icc) = merge_images(ctx, &work_inputs, layout)?;
            ctx.check_cancelled()?;
            let temp = temp_output(ctx, format);
            encode(image, format, EncodeOptions::default(), icc, &temp)?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

// ---------------------------------------------------------------------------------------
// Probing
// ---------------------------------------------------------------------------------------

/// Reads the size and format from the file header without decoding the pixels.
/// Returns `None` when the file is not a readable image.
pub fn probe(path: &Path) -> Option<ImageInfo> {
    let (reader, format) = open_reader(path).ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let (width, height) = if swaps_sides(orientation) {
        (height, width)
    } else {
        (width, height)
    };
    Some(ImageInfo {
        width,
        height,
        format,
    })
}

fn swaps_sides(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

// ---------------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------------

/// Checks that the path is an existing file with an image extension.
fn validate_input(path: &Path) -> AppResult<PathBuf> {
    let path = fs_utils::validate_input_file(path)?;
    let is_image = fs_utils::extension_of(&path)
        .as_deref()
        .and_then(fs_utils::detect_category)
        == Some(JobCategory::Image);
    if !is_image {
        return Err(AppError::InvalidParameter(format!(
            "not an image file: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn validate_quality(quality: u8) -> AppResult<u8> {
    if (1..=100).contains(&quality) {
        Ok(quality)
    } else {
        Err(AppError::InvalidParameter(
            "quality must be between 1 and 100".into(),
        ))
    }
}

fn validate_dimension(name: &str, value: u32) -> AppResult<()> {
    if (1..=MAX_DIMENSION).contains(&value) {
        Ok(())
    } else {
        Err(AppError::InvalidParameter(format!(
            "{name} must be between 1 and {MAX_DIMENSION}"
        )))
    }
}

fn check_output_size(width: u32, height: u32) -> AppResult<()> {
    if width == 0 || height == 0 {
        return Err(AppError::InvalidParameter(
            "the result would be empty".into(),
        ));
    }
    if width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_OUTPUT_PIXELS
    {
        return Err(AppError::TooLarge(format!(
            "the result would be {width}x{height} pixels"
        )));
    }
    Ok(())
}

/// Format of an input file, from its content.
fn detect_format(path: &Path) -> AppResult<ImageOutputFormat> {
    open_reader(path).map(|(_, format)| format)
}

/// Output format and destination for operations that keep the input format. The input
/// extension is kept when it matches the content; otherwise the right one is used.
fn same_format_destination(
    input: &Path,
    planner: &OutputPlanner,
) -> AppResult<(ImageOutputFormat, PathBuf)> {
    let format = detect_format(input)?;
    let extension = fs_utils::extension_of(input)
        .filter(|ext| format.extensions().contains(&ext.as_str()))
        .unwrap_or_else(|| format.extension().to_owned());
    Ok((format, planner.destination(input, &extension)?))
}

/// Target size for a resize. Never returns zero.
fn target_size(width: u32, height: u32, mode: ResizeMode) -> AppResult<(u32, u32)> {
    let scaled = |value: u32, factor: f64| ((f64::from(value) * factor).round() as u32).max(1);
    let (w, h) = match mode {
        ResizeMode::Exact { width, height } => (width, height),
        ResizeMode::Fit {
            width: box_w,
            height: box_h,
        } => {
            let factor =
                (f64::from(box_w) / f64::from(width)).min(f64::from(box_h) / f64::from(height));
            (
                scaled(width, factor).min(box_w),
                scaled(height, factor).min(box_h),
            )
        }
        ResizeMode::Percent { percent } => {
            let factor = f64::from(percent) / 100.0;
            (scaled(width, factor), scaled(height, factor))
        }
    };
    check_output_size(w, h)?;
    Ok((w, h))
}

// ---------------------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------------------

/// Job with one input, run on a blocking thread.
fn single_input_job<F>(kind: &str, input: PathBuf, work: F) -> JobSpec
where
    F: FnOnce(&JobContext, &Path) -> AppResult<Vec<PlannedOutput>> + Send + 'static,
{
    let work_input = input.clone();
    blocking_job(kind, JobCategory::Image, vec![input], move |ctx| {
        work(ctx, &work_input)
    })
}

/// Decodes, applies `transform` and encodes, checking for cancellation between steps.
fn transform_one(
    ctx: &JobContext,
    input: &Path,
    destination: PathBuf,
    format: ImageOutputFormat,
    options: EncodeOptions,
    transform: impl FnOnce(DynamicImage) -> AppResult<DynamicImage>,
) -> AppResult<Vec<PlannedOutput>> {
    ctx.set_progress(5.0);
    let loaded = load(input)?;
    ctx.check_cancelled()?;
    ctx.set_progress(40.0);
    let image = transform(loaded.image)?;
    ctx.check_cancelled()?;
    ctx.set_progress(60.0);
    let temp = temp_output(ctx, format);
    encode(image, format, options, loaded.icc, &temp)?;
    Ok(vec![PlannedOutput { temp, destination }])
}

fn temp_output(ctx: &JobContext, format: ImageOutputFormat) -> PathBuf {
    ctx.temp_dir.join(format!("output.{}", format.extension()))
}

#[derive(Debug, Clone, Copy)]
struct MergeLayout {
    direction: MergeDirection,
    match_size: bool,
    spacing: u32,
}

/// Loads every input and draws them on one canvas. Returns the image and the ICC profile
/// to keep (only when every input has the same one).
fn merge_images(
    ctx: &JobContext,
    inputs: &[PathBuf],
    layout: MergeLayout,
) -> AppResult<(DynamicImage, Option<Vec<u8>>)> {
    let total_steps = inputs.len() + 1;
    let mut images = Vec::with_capacity(inputs.len());
    let mut icc: Option<Option<Vec<u8>>> = None;
    for (index, input) in inputs.iter().enumerate() {
        let loaded = load(input)?;
        icc = match icc {
            None => Some(loaded.icc),
            Some(previous) if previous == loaded.icc => Some(previous),
            Some(_) => Some(None),
        };
        images.push(loaded.image);
        ctx.check_cancelled()?;
        ctx.set_steps(index + 1, total_steps);
    }

    let horizontal = layout.direction == MergeDirection::Horizontal;
    if layout.match_size {
        // Cross-axis size shared by every image: the smallest one, so nothing is enlarged.
        let target = images
            .iter()
            .map(|img| {
                if horizontal {
                    img.height()
                } else {
                    img.width()
                }
            })
            .min()
            .unwrap_or(1);
        for img in &mut images {
            let (w, h) = (img.width(), img.height());
            let (new_w, new_h) = if horizontal {
                let new_w = (u64::from(w) * u64::from(target) / u64::from(h)).max(1);
                (u32::try_from(new_w).unwrap_or(u32::MAX), target)
            } else {
                let new_h = (u64::from(h) * u64::from(target) / u64::from(w)).max(1);
                (target, u32::try_from(new_h).unwrap_or(u32::MAX))
            };
            if (new_w, new_h) != (w, h) {
                check_output_size(new_w, new_h)?;
                *img = resize_image(img, new_w, new_h);
            }
            ctx.check_cancelled()?;
        }
    }

    let gaps = u64::from(layout.spacing) * (images.len() as u64 - 1);
    let main: u64 = images
        .iter()
        .map(|img| {
            u64::from(if horizontal {
                img.width()
            } else {
                img.height()
            })
        })
        .sum::<u64>()
        + gaps;
    let cross = images
        .iter()
        .map(|img| {
            if horizontal {
                img.height()
            } else {
                img.width()
            }
        })
        .max()
        .unwrap_or(1);
    let main = u32::try_from(main).unwrap_or(u32::MAX);
    let (canvas_w, canvas_h) = if horizontal {
        (main, cross)
    } else {
        (cross, main)
    };
    check_output_size(canvas_w, canvas_h)?;

    let mut canvas = RgbaImage::from_pixel(canvas_w, canvas_h, Rgba([0, 0, 0, 0]));
    let mut offset: i64 = 0;
    for img in images {
        let rgba = img.to_rgba8();
        let (x, y) = if horizontal {
            (offset, i64::from((cross - rgba.height()) / 2))
        } else {
            (i64::from((cross - rgba.width()) / 2), offset)
        };
        imageops::replace(&mut canvas, &rgba, x, y);
        let size = if horizontal {
            rgba.width()
        } else {
            rgba.height()
        };
        offset += i64::from(size) + i64::from(layout.spacing);
        ctx.check_cancelled()?;
    }
    ctx.set_steps(inputs.len(), total_steps);
    Ok((DynamicImage::ImageRgba8(canvas), icc.flatten()))
}

// ---------------------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------------------

struct Loaded {
    /// Upright image (EXIF orientation already applied).
    image: DynamicImage,
    icc: Option<Vec<u8>>,
}

fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

/// Opens a reader with the format detected from the content and the decoding limits set.
fn open_reader(path: &Path) -> AppResult<(ImageReader<BufReader<File>>, ImageOutputFormat)> {
    let file = File::open(path)?;
    prepare_reader(BufReader::new(file))
}

fn prepare_reader<R: std::io::BufRead + Seek>(
    reader: R,
) -> AppResult<(ImageReader<R>, ImageOutputFormat)> {
    let mut reader = ImageReader::new(reader).with_guessed_format()?;
    let format = reader
        .format()
        .and_then(ImageOutputFormat::from_detected)
        .ok_or_else(|| AppError::InvalidFile("not a supported image format".into()))?;
    reader.limits(decode_limits());
    Ok((reader, format))
}

fn load(path: &Path) -> AppResult<Loaded> {
    let (reader, format) = open_reader(path)?;
    if format == ImageOutputFormat::Jpeg {
        integrity::check_jpeg(BufReader::new(File::open(path)?))?;
    }
    let mut decoder = reader.into_decoder().map_err(image_error)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    // A damaged profile only costs colour accuracy; the image is still usable.
    let icc = decoder.icc_profile().ok().flatten();
    let mut image = DynamicImage::from_decoder(decoder).map_err(image_error)?;
    image.apply_orientation(orientation);
    Ok(Loaded { image, icc })
}

/// EXIF orientation (1 to 8) of an image held in memory.
fn orientation_of(data: &[u8]) -> AppResult<u8> {
    let (reader, _) = prepare_reader(Cursor::new(data))?;
    let mut decoder = reader.into_decoder().map_err(image_error)?;
    Ok(decoder
        .orientation()
        .unwrap_or(Orientation::NoTransforms)
        .to_exif())
}

fn read_limited(path: &Path) -> AppResult<Vec<u8>> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_INPUT_BYTES {
        return Err(AppError::TooLarge(format!(
            "file has {size} bytes; the limit is {MAX_INPUT_BYTES}"
        )));
    }
    let mut data = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_INPUT_BYTES {
        return Err(AppError::TooLarge("file grew while being read".into()));
    }
    Ok(data)
}

fn image_error(err: ImageError) -> AppError {
    match err {
        ImageError::Limits(e) => AppError::TooLarge(e.to_string()),
        // Running out of data while decoding means the file is cut short, not a disk problem.
        ImageError::IoError(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            AppError::InvalidFile(format!("truncated file: {e}"))
        }
        ImageError::IoError(e) => AppError::Io(e),
        ImageError::Decoding(e) => AppError::InvalidFile(e.to_string()),
        ImageError::Unsupported(e) => AppError::InvalidFile(e.to_string()),
        ImageError::Parameter(e) => AppError::InvalidParameter(e.to_string()),
        ImageError::Encoding(e) => AppError::Internal(format!("encoding failed: {e}")),
    }
}

// ---------------------------------------------------------------------------------------
// Pixel operations
// ---------------------------------------------------------------------------------------

/// Resizes with premultiplied alpha when the image has transparency, so transparent
/// pixels do not bleed dark fringes into the edges.
fn resize_image(image: &DynamicImage, width: u32, height: u32) -> DynamicImage {
    if !has_transparency(image) {
        return image.resize_exact(width, height, RESIZE_FILTER);
    }
    let mut buffer = image.to_rgba16();
    for px in buffer.pixels_mut() {
        let alpha = u32::from(px[3]);
        for c in 0..3 {
            px[c] = ((u32::from(px[c]) * alpha + 32_767) / 65_535) as u16;
        }
    }
    let mut resized: ImageBuffer<Rgba<u16>, Vec<u16>> =
        imageops::resize(&buffer, width, height, RESIZE_FILTER);
    for px in resized.pixels_mut() {
        let alpha = u32::from(px[3]);
        for c in 0..3 {
            // Fully transparent pixels have no colour left to recover.
            px[c] = (u32::from(px[c]) * 65_535 + alpha / 2)
                .checked_div(alpha)
                .map_or(0, |value| value.min(65_535) as u16);
        }
    }
    let resized = DynamicImage::ImageRgba16(resized);
    match image.color() {
        ColorType::La8 => DynamicImage::ImageLumaA8(resized.to_luma_alpha8()),
        ColorType::La16 => DynamicImage::ImageLumaA16(resized.to_luma_alpha16()),
        ColorType::Rgba8 => DynamicImage::ImageRgba8(resized.to_rgba8()),
        ColorType::Rgba32F => DynamicImage::ImageRgba32F(resized.to_rgba32f()),
        _ => resized,
    }
}

/// Whether any pixel is not fully opaque.
fn has_transparency(image: &DynamicImage) -> bool {
    match image {
        DynamicImage::ImageLumaA8(buf) => buf.pixels().any(|p| p[1] < u8::MAX),
        DynamicImage::ImageRgba8(buf) => buf.pixels().any(|p| p[3] < u8::MAX),
        DynamicImage::ImageLumaA16(buf) => buf.pixels().any(|p| p[1] < u16::MAX),
        DynamicImage::ImageRgba16(buf) => buf.pixels().any(|p| p[3] < u16::MAX),
        DynamicImage::ImageRgba32F(buf) => buf.pixels().any(|p| p[3] < 1.0),
        _ => false,
    }
}

/// Composites over white and drops the alpha channel (for JPEG).
fn flatten_on_white(image: DynamicImage) -> DynamicImage {
    let gray = matches!(
        image.color(),
        ColorType::L8 | ColorType::L16 | ColorType::La8 | ColorType::La16
    );
    if !image.color().has_alpha() || !has_transparency(&image) {
        return if gray {
            DynamicImage::ImageLuma8(image.to_luma8())
        } else {
            DynamicImage::ImageRgb8(image.to_rgb8())
        };
    }
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = Vec::with_capacity(rgba.as_raw().len() / 4 * 3);
    let (pixels, _) = rgba.as_raw().as_chunks::<4>();
    for px in pixels {
        let alpha = u32::from(px[3]);
        for &channel in &px[..3] {
            let blended = (u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255;
            rgb.push(blended as u8);
        }
    }
    // The buffer has exactly width × height × 3 bytes, so this never falls back.
    let flattened = image::RgbImage::from_raw(width, height, rgb).map_or_else(
        || DynamicImage::ImageRgb8(image.to_rgb8()),
        DynamicImage::ImageRgb8,
    );
    if gray {
        DynamicImage::ImageLuma8(flattened.to_luma8())
    } else {
        flattened
    }
}

/// Same channels, 8 bits per channel.
fn to_8bit(image: DynamicImage) -> DynamicImage {
    match image {
        DynamicImage::ImageLuma16(_) => DynamicImage::ImageLuma8(image.to_luma8()),
        DynamicImage::ImageLumaA16(_) => DynamicImage::ImageLumaA8(image.to_luma_alpha8()),
        DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgb32F(_) => {
            DynamicImage::ImageRgb8(image.to_rgb8())
        }
        DynamicImage::ImageRgba16(_) | DynamicImage::ImageRgba32F(_) => {
            DynamicImage::ImageRgba8(image.to_rgba8())
        }
        other => other,
    }
}

/// Converts the pixel layout to one the encoder writes without further loss.
fn prepare_for(image: DynamicImage, format: ImageOutputFormat) -> DynamicImage {
    match format {
        ImageOutputFormat::Jpeg => flatten_on_white(image),
        ImageOutputFormat::Png => match image {
            DynamicImage::ImageRgb32F(_) => DynamicImage::ImageRgb16(image.to_rgb16()),
            DynamicImage::ImageRgba32F(_) => DynamicImage::ImageRgba16(image.to_rgba16()),
            other => other,
        },
        ImageOutputFormat::Tiff => match image {
            DynamicImage::ImageLumaA8(_) => DynamicImage::ImageRgba8(image.to_rgba8()),
            DynamicImage::ImageLumaA16(_) | DynamicImage::ImageRgba32F(_) => {
                DynamicImage::ImageRgba16(image.to_rgba16())
            }
            other => other,
        },
        ImageOutputFormat::Webp | ImageOutputFormat::Bmp => to_8bit(image),
        ImageOutputFormat::Gif | ImageOutputFormat::Ico => {
            DynamicImage::ImageRgba8(image.to_rgba8())
        }
    }
}

// ---------------------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct EncodeOptions {
    jpeg_quality: u8,
    png_compression: CompressionType,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self::with_quality(DEFAULT_JPEG_QUALITY)
    }
}

impl EncodeOptions {
    fn with_quality(jpeg_quality: u8) -> Self {
        Self {
            jpeg_quality,
            png_compression: CompressionType::Default,
        }
    }
}

/// Encodes `image` into `path`, keeping the ICC profile when the format supports it.
fn encode(
    image: DynamicImage,
    format: ImageOutputFormat,
    options: EncodeOptions,
    icc: Option<Vec<u8>>,
    path: &Path,
) -> AppResult<()> {
    let image = if format == ImageOutputFormat::Ico
        && (image.width() > ICO_MAX_SIZE || image.height() > ICO_MAX_SIZE)
    {
        let (w, h) = target_size(
            image.width(),
            image.height(),
            ResizeMode::Fit {
                width: ICO_MAX_SIZE,
                height: ICO_MAX_SIZE,
            },
        )?;
        resize_image(&image, w, h)
    } else {
        image
    };
    let image = prepare_for(image, format);

    let mut writer = BufWriter::new(File::create(path)?);
    match format {
        ImageOutputFormat::Png => {
            let encoder = PngEncoder::new_with_quality(
                &mut writer,
                options.png_compression,
                PngFilter::Adaptive,
            );
            write_with_icc(&image, encoder, icc)
        }
        ImageOutputFormat::Jpeg => {
            let encoder = JpegEncoder::new_with_quality(&mut writer, options.jpeg_quality);
            write_with_icc(&image, encoder, icc)
        }
        ImageOutputFormat::Webp => {
            write_with_icc(&image, WebPEncoder::new_lossless(&mut writer), icc)
        }
        ImageOutputFormat::Tiff => write_with_icc(&image, TiffEncoder::new(&mut writer), icc),
        ImageOutputFormat::Bmp => write_with_icc(&image, BmpEncoder::new(&mut writer), None),
        ImageOutputFormat::Gif => write_with_icc(
            &image,
            GifEncoder::new_with_speed(&mut writer, GIF_SPEED),
            None,
        ),
        ImageOutputFormat::Ico => write_with_icc(&image, IcoEncoder::new(&mut writer), None),
    }?;
    writer.flush()?;
    Ok(())
}

fn write_with_icc(
    image: &DynamicImage,
    mut encoder: impl ImageEncoder,
    icc: Option<Vec<u8>>,
) -> AppResult<()> {
    if let Some(profile) = icc {
        // Formats without ICC support simply drop it.
        let _ = encoder.set_icc_profile(profile);
    }
    image.write_with_encoder(encoder).map_err(image_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(width, height, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 77, 255])
        }))
    }

    #[test]
    fn target_sizes() {
        let fit = ResizeMode::Fit {
            width: 100,
            height: 100,
        };
        assert_eq!(target_size(400, 200, fit).unwrap(), (100, 50));
        assert_eq!(target_size(200, 400, fit).unwrap(), (50, 100));
        assert_eq!(target_size(10, 5, fit).unwrap(), (100, 50));
        assert_eq!(
            target_size(3, 1000, fit).unwrap(),
            (1, 100),
            "never zero pixels"
        );
        let exact = ResizeMode::Exact {
            width: 7,
            height: 9,
        };
        assert_eq!(target_size(400, 200, exact).unwrap(), (7, 9));
        let half = ResizeMode::Percent { percent: 50 };
        assert_eq!(target_size(401, 3, half).unwrap(), (201, 2));
        let huge = ResizeMode::Percent { percent: 1000 };
        assert!(matches!(
            target_size(20_000, 20_000, huge),
            Err(AppError::TooLarge(_))
        ));
    }

    #[test]
    fn flatten_composites_over_white() {
        let img = DynamicImage::ImageRgba8(
            RgbaImage::from_vec(2, 1, vec![0, 0, 0, 0, 255, 0, 0, 128]).unwrap(),
        );
        let flat = flatten_on_white(img).to_rgb8();
        assert_eq!(flat.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(flat.get_pixel(1, 0).0, [255, 127, 127]);
    }

    #[test]
    fn resize_with_alpha_has_no_dark_fringe() {
        // Left half opaque red, right half fully transparent black.
        let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(8, 2, |x, _| {
            if x < 4 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        }));
        let resized = resize_image(&img, 4, 1).to_rgba8();
        for px in resized.pixels().filter(|p| p[3] > 0) {
            assert!(px[0] >= 250, "colour bled from transparent pixels: {px:?}");
        }
        assert_eq!(resized.dimensions(), (4, 1));
    }

    #[test]
    fn every_format_round_trips() {
        let dir = std::env::temp_dir().join(format!("ec-img-roundtrip-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for format in [
            ImageOutputFormat::Png,
            ImageOutputFormat::Jpeg,
            ImageOutputFormat::Webp,
            ImageOutputFormat::Bmp,
            ImageOutputFormat::Gif,
            ImageOutputFormat::Tiff,
            ImageOutputFormat::Ico,
        ] {
            let path = dir.join(format!("out.{}", format.extension()));
            encode(
                rgba(300, 120),
                format,
                EncodeOptions::default(),
                None,
                &path,
            )
            .unwrap();
            let info = probe(&path).unwrap();
            assert_eq!(info.format, format);
            let expected = if format == ImageOutputFormat::Ico {
                (256, 102)
            } else {
                (300, 120)
            };
            assert_eq!((info.width, info.height), expected, "{format:?}");
            load(&path).unwrap();
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn decoding_limits_reject_huge_declared_sizes() {
        // A valid PNG header declaring 100000 x 100000 pixels, without the pixel data.
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&100_000u32.to_be_bytes());
        ihdr.extend_from_slice(&100_000u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&ihdr);
        let crc = metadata_crc(&[b"IHDR", &ihdr]);
        png.extend_from_slice(&crc.to_be_bytes());

        let dir = std::env::temp_dir().join(format!("ec-img-bomb-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bomb.png");
        fs::write(&path, png).unwrap();
        let result = load(&path);
        assert!(
            matches!(result, Err(AppError::TooLarge(_))),
            "got {:?}",
            result.err()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    fn metadata_crc(parts: &[&[u8]]) -> u32 {
        // Small local CRC so the test does not depend on the metadata module internals.
        let mut crc = 0xFFFF_FFFFu32;
        for part in parts {
            for &byte in *part {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        0xEDB8_8320 ^ (crc >> 1)
                    } else {
                        crc >> 1
                    };
                }
            }
        }
        crc ^ 0xFFFF_FFFF
    }
}
