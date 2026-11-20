//! Builds a PDF with one image per page, in Rust with lopdf (writing only: lopdf never
//! parses files the user gives the app).
//!
//! - JPEG photos (greyscale or colour) keep their compressed image data byte for byte,
//!   with no quality loss. Their metadata (EXIF with location and camera, XMP, comments)
//!   is removed first, like in every other operation, and the EXIF orientation is applied
//!   by the page's transformation matrix instead of by re-encoding the pixels.
//! - Other images (and CMYK JPEGs) are decoded, with the orientation applied, and stored
//!   losslessly with Flate compression; transparency becomes a soft mask.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use image::DynamicImage;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::jobs::JobContext;
use crate::ops::image::metadata::{self, Container};
use crate::ops::image::{self as img, integrity, ImageOutputFormat};

/// Points per millimetre.
const PT_PER_MM: f32 = 72.0 / 25.4;
/// Pixel size assumed for the "image size" page mode (CSS pixels).
const IMAGE_PAGE_DPI: f32 = 96.0;
pub const MAX_MARGIN_MM: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PdfPageSize {
    /// Each page has the size of its image.
    Image,
    A4,
    Letter,
}

impl PdfPageSize {
    /// Portrait size in points, `None` for [`PdfPageSize::Image`].
    fn points(self) -> Option<(f32, f32)> {
        match self {
            Self::Image => None,
            Self::A4 => Some((595.28, 841.89)),
            Self::Letter => Some((612.0, 792.0)),
        }
    }
}

/// An image ready to be placed on a page.
struct Placed {
    xobject: Stream,
    smask: Option<Stream>,
    /// Size as displayed (after orientation), in pixels.
    width: u32,
    height: u32,
    /// EXIF orientation still to apply (1 when the pixels are already upright).
    orientation: u8,
}

pub fn build(
    ctx: &JobContext,
    inputs: &[std::path::PathBuf],
    page_size: PdfPageSize,
    margin_mm: u32,
    output: &Path,
) -> AppResult<()> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::with_capacity(inputs.len());
    let margin = margin_mm as f32 * PT_PER_MM;

    for (index, input) in inputs.iter().enumerate() {
        let placed = prepare(input)?;
        ctx.check_cancelled()?;
        let page_id = add_page(&mut doc, pages_id, placed, page_size, margin)?;
        kids.push(Object::Reference(page_id));
        ctx.set_steps(index + 1, inputs.len() + 1);
    }

    let count = i64::try_from(kids.len()).unwrap_or(i64::MAX);
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut writer = BufWriter::new(File::create(output)?);
    doc.save_to(&mut writer)
        .map_err(|e| AppError::Internal(format!("could not write the PDF: {e}")))?;
    writer.flush()?;
    Ok(())
}

fn prepare(input: &Path) -> AppResult<Placed> {
    if img::detect_format(input)? == ImageOutputFormat::Jpeg {
        let data = img::read_limited(input)?;
        integrity::check_jpeg(&data[..])?;
        let frame = integrity::jpeg_frame(&data)
            .ok_or_else(|| AppError::InvalidFile("JPEG without a frame header".into()))?;
        if frame.bits == 8 && matches!(frame.components, 1 | 3) {
            let orientation = img::orientation_of(&data)?;
            // Orientation 1: no EXIF block at all, since the page applies the rotation.
            let data = metadata::strip(Container::Jpeg, &data, 1)?;
            let swap = (5..=8).contains(&orientation);
            let color_space = if frame.components == 1 {
                "DeviceGray"
            } else {
                "DeviceRGB"
            };
            let dict = dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(frame.width),
                "Height" => i64::from(frame.height),
                "ColorSpace" => color_space,
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            };
            let (width, height) = if swap {
                (frame.height, frame.width)
            } else {
                (frame.width, frame.height)
            };
            return Ok(Placed {
                xobject: Stream::new(dict, data),
                smask: None,
                width,
                height,
                orientation,
            });
        }
    }
    decoded(img::load(input)?.image)
}

/// Stores decoded pixels with Flate compression (lossless).
fn decoded(image: DynamicImage) -> AppResult<Placed> {
    let (width, height) = (image.width(), image.height());
    let gray = !image.color().has_color();
    let smask = if img::has_transparency(&image) {
        let alpha: Vec<u8> = image.to_luma_alpha8().pixels().map(|p| p[1]).collect();
        Some(flate_image(width, height, "DeviceGray", alpha)?)
    } else {
        None
    };
    let (color_space, pixels) = if gray {
        ("DeviceGray", image.to_luma8().into_raw())
    } else {
        ("DeviceRGB", image.to_rgb8().into_raw())
    };
    Ok(Placed {
        xobject: flate_image(width, height, color_space, pixels)?,
        smask,
        width,
        height,
        orientation: 1,
    })
}

fn flate_image(width: u32, height: u32, color_space: &str, pixels: Vec<u8>) -> AppResult<Stream> {
    let dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(width),
        "Height" => i64::from(height),
        "ColorSpace" => color_space,
        "BitsPerComponent" => 8,
    };
    let mut stream = Stream::new(dict, pixels);
    stream
        .compress()
        .map_err(|e| AppError::Internal(format!("compression failed: {e}")))?;
    Ok(stream)
}

/// Page size in points and the rectangle (x, y, width, height) where the image goes.
pub fn layout(
    image_width: u32,
    image_height: u32,
    page_size: PdfPageSize,
    margin: f32,
) -> ((f32, f32), (f32, f32, f32, f32)) {
    let (iw, ih) = (image_width.max(1) as f32, image_height.max(1) as f32);
    match page_size.points() {
        None => {
            let (w, h) = (iw * 72.0 / IMAGE_PAGE_DPI, ih * 72.0 / IMAGE_PAGE_DPI);
            ((w + 2.0 * margin, h + 2.0 * margin), (margin, margin, w, h))
        }
        Some((pw, ph)) => {
            // Landscape images get landscape pages.
            let (pw, ph) = if iw > ih { (ph, pw) } else { (pw, ph) };
            let (aw, ah) = ((pw - 2.0 * margin).max(1.0), (ph - 2.0 * margin).max(1.0));
            let scale = (aw / iw).min(ah / ih);
            let (w, h) = (iw * scale, ih * scale);
            ((pw, ph), ((pw - w) / 2.0, (ph - h) / 2.0, w, h))
        }
    }
}

/// Matrix that maps the unit square of the stored image to its upright position in the
/// unit square, for an EXIF orientation (1 to 8).
pub fn orientation_matrix(orientation: u8) -> [f32; 6] {
    match orientation {
        2 => [-1.0, 0.0, 0.0, 1.0, 1.0, 0.0],
        3 => [-1.0, 0.0, 0.0, -1.0, 1.0, 1.0],
        4 => [1.0, 0.0, 0.0, -1.0, 0.0, 1.0],
        5 => [0.0, -1.0, -1.0, 0.0, 1.0, 1.0],
        6 => [0.0, -1.0, 1.0, 0.0, 0.0, 1.0],
        7 => [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
        8 => [0.0, 1.0, -1.0, 0.0, 1.0, 0.0],
        _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    }
}

fn add_page(
    doc: &mut Document,
    pages_id: ObjectId,
    placed: Placed,
    page_size: PdfPageSize,
    margin: f32,
) -> AppResult<ObjectId> {
    let ((pw, ph), (x, y, w, h)) = layout(placed.width, placed.height, page_size, margin);
    let [a, b, c, d, e, f] = orientation_matrix(placed.orientation);
    let matrix = [a * w, b * h, c * w, d * h, e * w + x, f * h + y];

    let mut xobject = placed.xobject;
    if let Some(mask) = placed.smask {
        let mask_id = doc.add_object(mask);
        xobject.dict.set("SMask", mask_id);
    }
    let image_id = doc.add_object(xobject);

    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new("cm", matrix.iter().map(|&v| Object::Real(v)).collect()),
            Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
            Operation::new("Q", vec![]),
        ],
    };
    let encoded = content
        .encode()
        .map_err(|e| AppError::Internal(format!("could not write the page: {e}")))?;
    let content_id = doc.add_object(Stream::new(dictionary! {}, encoded));

    Ok(doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(pw), Object::Real(ph)],
        "Resources" => dictionary! {
            "XObject" => dictionary! { "Im0" => image_id },
        },
        "Contents" => content_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: [f32; 6], u: f32, v: f32) -> (f32, f32) {
        (m[0] * u + m[2] * v + m[4], m[1] * u + m[3] * v + m[5])
    }

    /// Where the stored image's top-left pixel (u = 0, v = 1) ends up, for each EXIF
    /// orientation, compared with what the orientation means.
    #[test]
    fn orientation_matrices_put_the_first_pixel_where_exif_says() {
        let expected = [
            (1, (0.0, 1.0)), // top-left stays top-left
            (2, (1.0, 1.0)), // mirrored: top-right
            (3, (1.0, 0.0)), // rotated 180: bottom-right
            (4, (0.0, 0.0)), // flipped: bottom-left
            (5, (0.0, 1.0)), // transposed: top-left
            (6, (1.0, 1.0)), // rotated 90 clockwise: top-right
            (7, (1.0, 0.0)), // transverse: bottom-right
            (8, (0.0, 0.0)), // rotated 90 counter-clockwise: bottom-left
        ];
        for (orientation, corner) in expected {
            assert_eq!(
                apply(orientation_matrix(orientation), 0.0, 1.0),
                corner,
                "orientation {orientation}"
            );
        }
    }

    #[test]
    fn layout_fits_and_centres_on_paper() {
        // Portrait image on A4 without margin: full width, centred vertically.
        let ((pw, ph), (x, y, w, h)) = layout(1000, 1000, PdfPageSize::A4, 0.0);
        assert_eq!((pw, ph), (595.28, 841.89));
        assert!((w - 595.28).abs() < 0.01 && (h - 595.28).abs() < 0.01);
        assert!(x.abs() < 0.01 && (y - (841.89 - 595.28) / 2.0).abs() < 0.01);
        // Landscape images get a landscape page.
        let ((pw, ph), _) = layout(2000, 1000, PdfPageSize::Letter, 10.0);
        assert_eq!((pw, ph), (792.0, 612.0));
        // Image-sized page at 96 dpi plus margins.
        let ((pw, ph), (x, y, _, _)) = layout(96, 192, PdfPageSize::Image, 5.0);
        assert_eq!((pw, ph, x, y), (82.0, 154.0, 5.0, 5.0));
    }
}
