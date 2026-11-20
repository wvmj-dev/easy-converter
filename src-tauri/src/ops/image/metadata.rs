//! Lossless metadata removal for JPEG, PNG and WebP.
//!
//! The compressed image data is copied byte for byte; only metadata containers are dropped
//! (EXIF, XMP, IPTC, comments, text chunks, thumbnails, trailing data). Colour information
//! (ICC profile, gamma, colour space) is kept so the picture looks the same, and when the
//! original had a non-default EXIF orientation, a minimal EXIF block holding only that tag is
//! written back, so the picture is not shown rotated.
//!
//! Every read is bounds-checked: these parsers receive untrusted files.

use crate::error::{AppError, AppResult};

/// Containers whose metadata can be removed without re-encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Jpeg,
    Png,
    WebP,
}

/// Returns a copy of `data` without metadata. `orientation` is the EXIF orientation of the
/// original (1 to 8); 1 means no rotation and no EXIF block is written.
pub fn strip(container: Container, data: &[u8], orientation: u8) -> AppResult<Vec<u8>> {
    let exif = (2..=8)
        .contains(&orientation)
        .then(|| minimal_exif(orientation));
    match container {
        Container::Jpeg => strip_jpeg(data, exif.as_deref()),
        Container::Png => strip_png(data, exif.as_deref()),
        Container::WebP => strip_webp(data, exif.as_deref()),
    }
}

fn invalid(what: &str) -> AppError {
    AppError::InvalidFile(what.to_owned())
}

/// TIFF structure (little endian) with a single IFD entry: Orientation (0x0112), SHORT.
fn minimal_exif(orientation: u8) -> Vec<u8> {
    let mut tiff = Vec::with_capacity(26);
    tiff.extend_from_slice(b"II*\0");
    tiff.extend_from_slice(&8u32.to_le_bytes()); // offset of the first IFD
    tiff.extend_from_slice(&1u16.to_le_bytes()); // number of entries
    tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // tag: Orientation
    tiff.extend_from_slice(&3u16.to_le_bytes()); // type: SHORT
    tiff.extend_from_slice(&1u32.to_le_bytes()); // count
    tiff.extend_from_slice(&u16::from(orientation).to_le_bytes());
    tiff.extend_from_slice(&[0, 0]); // value padding to 4 bytes
    tiff.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    tiff
}

// ---------------------------------------------------------------------------------------
// JPEG
// ---------------------------------------------------------------------------------------

const SOI: u8 = 0xD8;
const EOI: u8 = 0xD9;
const SOS: u8 = 0xDA;
const APP0: u8 = 0xE0;
const APP1: u8 = 0xE1;
const APP2: u8 = 0xE2;
const APP14: u8 = 0xEE;
const COM: u8 = 0xFE;

/// Whether a JPEG marker segment carries image or colour data that must be kept.
fn keep_jpeg_segment(marker: u8, payload: &[u8]) -> bool {
    match marker {
        // JFIF header (density and version). JFXX extensions only hold thumbnails.
        APP0 => payload.starts_with(b"JFIF\0"),
        // ICC profile chunks. Other APP2 users (MPF, FlashPix) are metadata.
        APP2 => payload.starts_with(b"ICC_PROFILE\0"),
        // Adobe segment: tells decoders how to convert CMYK/YCCK colours.
        APP14 => payload.starts_with(b"Adobe"),
        // EXIF, XMP, IPTC, comments and every other application segment.
        APP1 | 0xE3..=0xED | 0xEF | COM => false,
        // Tables, frame and scan headers, restart intervals...
        _ => true,
    }
}

fn strip_jpeg(data: &[u8], exif: Option<&[u8]>) -> AppResult<Vec<u8>> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != SOI {
        return Err(invalid("not a JPEG file"));
    }
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&[0xFF, SOI]);

    // The orientation block goes right after the JFIF header (or SOI when there is none).
    let mut pending_exif = exif.map(|tiff| {
        let mut segment = Vec::with_capacity(tiff.len() + 10);
        let length = u16::try_from(tiff.len() + 8).unwrap_or(u16::MAX);
        segment.extend_from_slice(&[0xFF, APP1]);
        segment.extend_from_slice(&length.to_be_bytes());
        segment.extend_from_slice(b"Exif\0\0");
        segment.extend_from_slice(tiff);
        segment
    });
    let mut seen_scan = false;
    let mut pos = 2;

    loop {
        if pos >= data.len() || data[pos] != 0xFF {
            return Err(invalid("damaged JPEG structure"));
        }
        // Markers may be preceded by any number of 0xFF fill bytes.
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        let Some(&marker) = data.get(pos) else {
            return Err(invalid("truncated JPEG file"));
        };
        pos += 1;

        match marker {
            EOI => {
                if !seen_scan {
                    return Err(invalid("JPEG file without image data"));
                }
                // Anything after EOI (thumbnails, maker trailers) is dropped.
                out.extend_from_slice(&[0xFF, EOI]);
                return Ok(out);
            }
            SOI | 0x00 => return Err(invalid("damaged JPEG structure")),
            // Standalone markers without a length.
            0x01 | 0xD0..=0xD7 => out.extend_from_slice(&[0xFF, marker]),
            _ => {
                let length_bytes = data
                    .get(pos..pos + 2)
                    .ok_or_else(|| invalid("truncated JPEG file"))?;
                let length = usize::from(u16::from_be_bytes([length_bytes[0], length_bytes[1]]));
                if length < 2 {
                    return Err(invalid("damaged JPEG segment"));
                }
                let segment = data
                    .get(pos..pos + length)
                    .ok_or_else(|| invalid("truncated JPEG file"))?;
                if marker != APP0 {
                    if let Some(exif_segment) = pending_exif.take() {
                        out.extend_from_slice(&exif_segment);
                    }
                }
                if keep_jpeg_segment(marker, &segment[2..]) {
                    out.extend_from_slice(&[0xFF, marker]);
                    out.extend_from_slice(segment);
                }
                pos += length;

                if marker == SOS {
                    seen_scan = true;
                    let end = entropy_data_end(data, pos)?;
                    out.extend_from_slice(&data[pos..end]);
                    pos = end;
                }
            }
        }
    }
}

/// Index of the marker that ends the entropy-coded data starting at `start`.
/// Inside that data, 0xFF is followed by 0x00 (stuffing) or a restart marker.
fn entropy_data_end(data: &[u8], start: usize) -> AppResult<usize> {
    let mut i = start;
    while i + 1 < data.len() {
        if data[i] == 0xFF {
            let next = data[i + 1];
            if next == 0x00 || (0xD0..=0xD7).contains(&next) {
                i += 2;
                continue;
            }
            return Ok(i);
        }
        i += 1;
    }
    Err(invalid("truncated JPEG file"))
}

// ---------------------------------------------------------------------------------------
// PNG
// ---------------------------------------------------------------------------------------

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Ancillary chunks that affect how the image is displayed (or animated) and are kept.
/// Critical chunks (uppercase first letter) are always kept.
const PNG_KEPT_ANCILLARY: &[&[u8; 4]] = &[
    b"tRNS", b"gAMA", b"cHRM", b"sRGB", b"iCCP", b"cICP", b"mDCV", b"cLLI", b"sBIT", b"bKGD",
    b"pHYs", b"acTL", b"fcTL", b"fdAT",
];

fn strip_png(data: &[u8], exif: Option<&[u8]>) -> AppResult<Vec<u8>> {
    if !data.starts_with(PNG_SIGNATURE) {
        return Err(invalid("not a PNG file"));
    }
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(PNG_SIGNATURE);
    let mut pending_exif = exif;
    let mut pos = PNG_SIGNATURE.len();
    let mut first = true;

    loop {
        let header = data
            .get(pos..pos + 8)
            .ok_or_else(|| invalid("truncated PNG file"))?;
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if length > i32::MAX as u32 {
            return Err(invalid("damaged PNG chunk"));
        }
        let kind: [u8; 4] = [header[4], header[5], header[6], header[7]];
        let total = 12 + length as usize;
        let chunk = data
            .get(pos..pos + total)
            .ok_or_else(|| invalid("truncated PNG file"))?;
        if first && &kind != b"IHDR" {
            return Err(invalid("PNG file does not start with IHDR"));
        }
        first = false;

        if &kind == b"IDAT" || &kind == b"IEND" {
            if let Some(tiff) = pending_exif.take() {
                write_png_chunk(&mut out, b"eXIf", tiff);
            }
        }
        let critical = kind[0].is_ascii_uppercase();
        if critical || PNG_KEPT_ANCILLARY.contains(&&kind) {
            out.extend_from_slice(chunk);
        }
        pos += total;
        if &kind == b"IEND" {
            // Anything after IEND is dropped.
            return Ok(out);
        }
    }
}

fn write_png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    let length = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    let crc = crc32(&[kind.as_slice(), payload]);
    out.extend_from_slice(&crc.to_be_bytes());
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// CRC-32 as used by PNG, over the concatenation of `parts`.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for &byte in *part {
            crc = CRC_TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ (crc >> 8);
        }
    }
    crc ^ 0xFFFF_FFFF
}

// ---------------------------------------------------------------------------------------
// WebP
// ---------------------------------------------------------------------------------------

const VP8X_FLAG_EXIF: u8 = 0x08;
const VP8X_FLAG_XMP: u8 = 0x04;

/// Chunks of the extended format that hold image or display data.
const WEBP_KEPT_CHUNKS: &[&[u8; 4]] = &[
    b"VP8X", b"ICCP", b"ANIM", b"ANMF", b"ALPH", b"VP8 ", b"VP8L",
];

fn strip_webp(data: &[u8], exif: Option<&[u8]>) -> AppResult<Vec<u8>> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return Err(invalid("not a WebP file"));
    }
    let riff_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    // Ignore anything after the RIFF container.
    let end = data.len().min(8usize.saturating_add(riff_size));

    let mut chunks: Vec<([u8; 4], &[u8])> = Vec::new();
    let mut pos = 12;
    while pos + 8 <= end {
        let kind: [u8; 4] = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
        let size = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;
        let payload = data
            .get(pos + 8..pos + 8 + size)
            .filter(|_| pos + 8 + size <= end)
            .ok_or_else(|| invalid("truncated WebP file"))?;
        chunks.push((kind, payload));
        pos += 8 + size + (size & 1);
    }
    let Some((first_kind, _)) = chunks.first() else {
        return Err(invalid("WebP file without image data"));
    };

    // Simple formats (a single VP8/VP8L chunk) cannot hold metadata nor an orientation.
    if first_kind != b"VP8X" {
        let (kind, payload) = chunks[0];
        if &kind != b"VP8 " && &kind != b"VP8L" {
            return Err(invalid("WebP file without image data"));
        }
        return Ok(riff(&[(kind, payload.to_vec())]));
    }

    let mut kept: Vec<([u8; 4], Vec<u8>)> = Vec::with_capacity(chunks.len() + 1);
    for (kind, payload) in chunks {
        if !WEBP_KEPT_CHUNKS.contains(&&kind) {
            continue;
        }
        let mut payload = payload.to_vec();
        if &kind == b"VP8X" {
            let flags = payload
                .first_mut()
                .ok_or_else(|| invalid("damaged WebP header"))?;
            *flags &= !(VP8X_FLAG_EXIF | VP8X_FLAG_XMP);
            if exif.is_some() {
                *flags |= VP8X_FLAG_EXIF;
            }
        }
        kept.push((kind, payload));
    }
    if let Some(tiff) = exif {
        // EXIF goes after the image data in the extended format.
        kept.push((*b"EXIF", tiff.to_vec()));
    }
    Ok(riff(&kept))
}

fn riff(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let body: usize = chunks
        .iter()
        .map(|(_, p)| 8 + p.len() + (p.len() & 1))
        .sum();
    let mut out = Vec::with_capacity(12 + body);
    out.extend_from_slice(b"RIFF");
    let size = u32::try_from(4 + body).unwrap_or(u32::MAX);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    for (kind, payload) in chunks {
        out.extend_from_slice(kind);
        let len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(payload);
        if payload.len() & 1 == 1 {
            out.push(0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageDecoder, ImageReader, RgbImage, RgbaImage};
    use std::io::Cursor;

    const SECRET: &[u8] = b"SECRET-GPS-AND-CAMERA";

    fn exif_with_secret(orientation: u8) -> Vec<u8> {
        // Valid orientation IFD followed by bytes a metadata reader would expose.
        let mut tiff = minimal_exif(orientation);
        tiff.extend_from_slice(SECRET);
        tiff
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    fn orientation_of(data: &[u8]) -> u8 {
        let mut decoder = ImageReader::new(Cursor::new(data))
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        decoder.orientation().unwrap().to_exif()
    }

    fn decode(data: &[u8]) -> image::DynamicImage {
        ImageReader::new(Cursor::new(data))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap()
    }

    fn sample_rgb() -> RgbImage {
        RgbImage::from_fn(16, 8, |x, y| {
            image::Rgb([(x * 16) as u8, (y * 32) as u8, 128])
        })
    }

    /// JPEG with JFIF, EXIF (with orientation), XMP, a comment, an ICC chunk and trailing data.
    fn jpeg_with_metadata(orientation: u8) -> Vec<u8> {
        let mut plain = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut plain, 90)
            .encode_image(&sample_rgb())
            .unwrap();
        let segment = |marker: u8, payload: &[u8]| {
            let mut s = vec![0xFF, marker];
            s.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
            s.extend_from_slice(payload);
            s
        };
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend(exif_with_secret(orientation));
        let mut xmp = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
        xmp.extend_from_slice(SECRET);
        let mut icc = b"ICC_PROFILE\0\x01\x01".to_vec();
        icc.extend_from_slice(b"fake-profile");
        let mut com = b"comment ".to_vec();
        com.extend_from_slice(SECRET);

        // The encoder output starts with SOI + JFIF APP0; insert after them.
        let app0_len = usize::from(u16::from_be_bytes([plain[4], plain[5]]));
        let split = 4 + app0_len;
        let mut out = plain[..split].to_vec();
        out.extend(segment(APP1, &exif));
        out.extend(segment(APP1, &xmp));
        out.extend(segment(APP2, &icc));
        out.extend(segment(COM, &com));
        out.extend_from_slice(&plain[split..]);
        out.extend_from_slice(SECRET); // trailer after EOI
        out
    }

    #[test]
    fn jpeg_metadata_is_removed_and_pixels_kept() {
        let original = jpeg_with_metadata(6);
        let stripped = strip(Container::Jpeg, &original, 6).unwrap();

        assert!(!contains(&stripped, SECRET));
        assert!(!contains(&stripped, b"http://ns.adobe.com"));
        assert!(contains(&stripped, b"ICC_PROFILE\0"));
        assert!(contains(&stripped, b"JFIF\0"));
        assert_eq!(orientation_of(&stripped), 6);
        assert_eq!(decode(&stripped).to_rgb8(), decode(&original).to_rgb8());
    }

    #[test]
    fn jpeg_without_rotation_gets_no_exif() {
        let original = jpeg_with_metadata(1);
        let stripped = strip(Container::Jpeg, &original, 1).unwrap();
        assert!(!contains(&stripped, b"Exif\0\0"));
        assert_eq!(orientation_of(&stripped), 1);
    }

    #[test]
    fn progressive_like_multi_scan_structure_is_copied() {
        // Entropy data containing stuffed 0xFF00 and restart markers must be kept intact.
        let mut data = vec![0xFF, SOI];
        data.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x03, 0x01]); // DQT (dummy)
        data.extend_from_slice(&[0xFF, SOS, 0x00, 0x03, 0x00]);
        data.extend_from_slice(&[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56]);
        data.extend_from_slice(&[0xFF, SOS, 0x00, 0x03, 0x00, 0x78]);
        data.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x04, b'h', b'i']); // COM between scans
        data.extend_from_slice(&[0xFF, EOI]);

        let stripped = strip(Container::Jpeg, &data, 1).unwrap();
        let expected_len = data.len() - 6;
        assert_eq!(stripped.len(), expected_len);
        assert!(!contains(&stripped, b"hi"));
        assert!(contains(
            &stripped,
            &[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56]
        ));
    }

    fn png_with_metadata() -> Vec<u8> {
        let img = RgbaImage::from_fn(5, 4, |x, y| {
            image::Rgba([x as u8 * 40, y as u8 * 50, 7, 200])
        });
        let mut plain = Vec::new();
        image::codecs::png::PngEncoder::new(&mut plain)
            .write_image(img.as_raw(), 5, 4, image::ExtendedColorType::Rgba8)
            .unwrap();
        // Insert text, time and eXIf chunks right after IHDR (8 + 25 bytes).
        let split = 8 + 25;
        let mut out = plain[..split].to_vec();
        let mut text = b"Author\0".to_vec();
        text.extend_from_slice(SECRET);
        write_png_chunk(&mut out, b"tEXt", &text);
        write_png_chunk(&mut out, b"tIME", &[7, 233, 1, 2, 3, 4, 5]);
        write_png_chunk(&mut out, b"eXIf", &exif_with_secret(8));
        write_png_chunk(&mut out, b"gAMA", &45455u32.to_be_bytes());
        out.extend_from_slice(&plain[split..]);
        out.extend_from_slice(SECRET);
        out
    }

    use image::ImageEncoder;

    #[test]
    fn png_metadata_is_removed_and_pixels_kept() {
        let original = png_with_metadata();
        let stripped = strip(Container::Png, &original, 8).unwrap();

        assert!(!contains(&stripped, SECRET));
        assert!(!contains(&stripped, b"tEXt"));
        assert!(!contains(&stripped, b"tIME"));
        assert!(contains(&stripped, b"gAMA"));
        assert_eq!(orientation_of(&stripped), 8);
        assert_eq!(decode(&stripped).to_rgba8(), decode(&original).to_rgba8());
    }

    fn webp_with_metadata() -> Vec<u8> {
        let img = RgbaImage::from_fn(6, 3, |x, y| {
            image::Rgba([x as u8 * 30, y as u8 * 60, 9, 255])
        });
        let mut plain = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut plain)
            .write_image(img.as_raw(), 6, 3, image::ExtendedColorType::Rgba8)
            .unwrap();
        // Rebuild as the extended format with EXIF and XMP chunks.
        let vp8l_size = u32::from_le_bytes([plain[16], plain[17], plain[18], plain[19]]) as usize;
        let vp8l = plain[20..20 + vp8l_size].to_vec();
        let mut vp8x = vec![VP8X_FLAG_EXIF | VP8X_FLAG_XMP, 0, 0, 0];
        vp8x.extend_from_slice(&(6u32 - 1).to_le_bytes()[..3]);
        vp8x.extend_from_slice(&(3u32 - 1).to_le_bytes()[..3]);
        let mut xmp = b"<x:xmpmeta>".to_vec();
        xmp.extend_from_slice(SECRET);
        riff(&[
            (*b"VP8X", vp8x),
            (*b"VP8L", vp8l),
            (*b"EXIF", exif_with_secret(3)),
            (*b"XMP ", xmp),
        ])
    }

    #[test]
    fn webp_metadata_is_removed_and_pixels_kept() {
        let original = webp_with_metadata();
        let stripped = strip(Container::WebP, &original, 3).unwrap();

        assert!(!contains(&stripped, SECRET));
        assert!(!contains(&stripped, b"XMP "));
        assert_eq!(stripped[20] & VP8X_FLAG_XMP, 0);
        assert_eq!(orientation_of(&stripped), 3);
        assert_eq!(decode(&stripped).to_rgba8(), decode(&original).to_rgba8());

        let unrotated = strip(Container::WebP, &original, 1).unwrap();
        assert!(!contains(&unrotated, b"EXIF"));
        assert_eq!(unrotated[20] & VP8X_FLAG_EXIF, 0);
    }

    #[test]
    fn crc_matches_reference() {
        // CRC of "IEND" with no data, as found in every PNG file.
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
    }

    #[test]
    fn truncated_or_garbage_input_never_panics() {
        let samples = [
            (Container::Jpeg, jpeg_with_metadata(6)),
            (Container::Png, png_with_metadata()),
            (Container::WebP, webp_with_metadata()),
        ];
        for (container, data) in samples {
            for len in 0..data.len() {
                let _ = strip(container, &data[..len], 6);
            }
            let mut corrupted = data.clone();
            for i in (0..corrupted.len()).step_by(3) {
                corrupted[i] = corrupted[i].wrapping_mul(31).wrapping_add(7);
                let _ = strip(container, &corrupted, 6);
            }
        }
        assert!(strip(Container::Jpeg, b"not an image", 1).is_err());
        assert!(strip(Container::Png, b"", 1).is_err());
        assert!(strip(Container::WebP, b"RIFF\xff\xff\xff\xffWEBP", 1).is_err());
    }
}
