//! Structural checks for formats whose decoder accepts damaged files without an error.
//!
//! The JPEG decoder fills whatever is missing from a truncated file with grey and reports
//! success, which would turn a broken download into a "converted" picture that is mostly
//! grey. Walking the marker structure up to the end-of-image marker catches that before
//! decoding. The file is streamed, never read whole into memory.

use std::io::{self, BufRead};

use crate::error::{AppError, AppResult};

const SOI: u8 = 0xD8;
const EOI: u8 = 0xD9;
const SOS: u8 = 0xDA;

fn damaged(what: &str) -> AppError {
    AppError::InvalidFile(what.to_owned())
}

fn truncated() -> AppError {
    damaged("truncated JPEG file")
}

/// Fails unless the JPEG in `reader` has every segment complete, at least one scan and an
/// end-of-image marker. Anything after that marker is ignored.
pub fn check_jpeg(mut reader: impl BufRead) -> AppResult<()> {
    let mut stream = Stream {
        reader: &mut reader,
    };
    if stream.byte()? != Some(0xFF) || stream.byte()? != Some(SOI) {
        return Err(damaged("not a JPEG file"));
    }
    let mut seen_scan = false;
    // Set when the scan data ended at a marker that was already read.
    let mut pending = None;

    loop {
        let marker = match pending.take() {
            Some(marker) => marker,
            None => {
                if stream.byte()?.ok_or_else(truncated)? != 0xFF {
                    return Err(damaged("damaged JPEG structure"));
                }
                // Markers may be preceded by any number of 0xFF fill bytes.
                loop {
                    match stream.byte()?.ok_or_else(truncated)? {
                        0xFF => {}
                        marker => break marker,
                    }
                }
            }
        };

        match marker {
            EOI if seen_scan => return Ok(()),
            EOI => return Err(damaged("JPEG file without image data")),
            SOI | 0x00 => return Err(damaged("damaged JPEG structure")),
            0x01 | 0xD0..=0xD7 => {}
            _ => {
                let high = stream.byte()?.ok_or_else(truncated)?;
                let low = stream.byte()?.ok_or_else(truncated)?;
                let length = u64::from(u16::from_be_bytes([high, low]));
                if length < 2 {
                    return Err(damaged("damaged JPEG segment"));
                }
                stream.skip(length - 2)?;
                if marker == SOS {
                    seen_scan = true;
                    pending = Some(stream.scan_end()?);
                }
            }
        }
    }
}

struct Stream<'a, R: BufRead> {
    reader: &'a mut R,
}

impl<R: BufRead> Stream<'_, R> {
    fn byte(&mut self) -> io::Result<Option<u8>> {
        let buf = self.reader.fill_buf()?;
        let Some(&b) = buf.first() else {
            return Ok(None);
        };
        self.reader.consume(1);
        Ok(Some(b))
    }

    fn skip(&mut self, mut count: u64) -> AppResult<()> {
        while count > 0 {
            let available = self.reader.fill_buf()?.len();
            if available == 0 {
                return Err(truncated());
            }
            let step = available.min(usize::try_from(count).unwrap_or(usize::MAX));
            self.reader.consume(step);
            count -= step as u64;
        }
        Ok(())
    }

    /// Skips entropy-coded data and returns the marker that ends it. Inside that data,
    /// 0xFF is only followed by 0x00 (stuffing) or a restart marker.
    fn scan_end(&mut self) -> AppResult<u8> {
        loop {
            let buf = self.reader.fill_buf()?;
            if buf.is_empty() {
                return Err(truncated());
            }
            match buf.iter().position(|&b| b == 0xFF) {
                Some(at) => self.reader.consume(at + 1),
                None => {
                    let len = buf.len();
                    self.reader.consume(len);
                    continue;
                }
            }
            match self.byte()?.ok_or_else(truncated)? {
                0x00 | 0xD0..=0xD7 => {}
                0xFF => {
                    // Fill bytes before a marker.
                    loop {
                        match self.byte()?.ok_or_else(truncated)? {
                            0xFF => {}
                            0x00 => break,
                            marker => return Ok(marker),
                        }
                    }
                }
                marker => return Ok(marker),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    fn jpeg(width: u32, height: u32, progressive_like: bool) -> Vec<u8> {
        let img = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x * 7) as u8, (y * 5) as u8, (x ^ y) as u8])
        });
        let mut data = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, 90)
            .encode_image(&img)
            .unwrap();
        if progressive_like {
            // A second scan, as progressive files have, before the end marker.
            let eoi = data.len() - 2;
            let mut out = data[..eoi].to_vec();
            out.extend_from_slice(&[0xFF, SOS, 0x00, 0x03, 0x00, 0x12, 0xFF, 0x00, 0x34]);
            out.extend_from_slice(&[0xFF, 0xFF, EOI]);
            data = out;
        }
        data
    }

    fn check(data: &[u8]) -> AppResult<()> {
        // A tiny buffer exercises every refill path.
        check_jpeg(BufReader::with_capacity(3, Cursor::new(data)))
    }

    #[test]
    fn complete_files_pass_even_with_trailing_data() {
        let mut data = jpeg(40, 30, false);
        check(&data).unwrap();
        data.extend_from_slice(b"trailer after the end marker");
        check(&data).unwrap();
        check(&jpeg(16, 16, true)).unwrap();
    }

    #[test]
    fn every_truncation_is_rejected() {
        let data = jpeg(40, 30, false);
        for len in 0..data.len() {
            assert!(
                matches!(check(&data[..len]), Err(AppError::InvalidFile(_))),
                "truncated at {len} of {} was accepted",
                data.len()
            );
        }
    }

    #[test]
    fn structure_without_scan_or_with_bad_segment_is_rejected() {
        assert!(check(&[0xFF, SOI, 0xFF, EOI]).is_err());
        assert!(check(&[0xFF, SOI, 0xFF, 0xE0, 0x00, 0x01]).is_err());
        assert!(check(b"\x89PNG\r\n\x1a\n").is_err());
        assert!(check(&[0xFF, SOI, 0x12, 0x34]).is_err());
    }

    #[test]
    fn garbage_never_panics() {
        let data = jpeg(24, 24, true);
        for i in 0..data.len() {
            for value in [0x00, 0xFF, 0xD9, 0xDA, 0x01] {
                let mut corrupt = data.clone();
                corrupt[i] = value;
                let _ = check(&corrupt);
            }
        }
    }
}
