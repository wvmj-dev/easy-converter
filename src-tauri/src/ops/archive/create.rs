//! Creating ZIP, TAR, TAR.GZ and 7z archives in Rust.
//!
//! Each input file goes in at the top of the archive under its own name (repeated names
//! are numbered), with its modification time. Owners, permissions and paths of the
//! user's folders are not recorded.

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::write::GzEncoder;
use flate2::Compression;

use super::names::UniqueNames;
use super::ArchiveFormat;
use crate::error::{AppError, AppResult};
use crate::jobs::JobContext;

const CHUNK: usize = 64 * 1024;

/// A file to add and its name inside the archive.
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

pub fn items(inputs: &[PathBuf]) -> AppResult<Vec<Item>> {
    let mut names = UniqueNames::default();
    inputs
        .iter()
        .map(|path| {
            let meta = fs::metadata(path)?;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_owned());
            Ok(Item {
                path: path.clone(),
                name: names.take(&name),
                size: meta.len(),
                modified: meta.modified().ok(),
            })
        })
        .collect()
}

/// Reads an input file, reporting progress over all inputs and stopping when cancelled.
struct Progress<'a> {
    ctx: &'a JobContext,
    done: u64,
    total: u64,
}

struct Tracked<'a, 'b> {
    file: File,
    progress: &'b mut Progress<'a>,
}

impl Read for Tracked<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.progress.ctx.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let n = self.file.read(buf)?;
        self.progress.done += n as u64;
        if self.progress.total > 0 {
            self.progress
                .ctx
                .set_progress(self.progress.done as f32 * 100.0 / self.progress.total as f32);
        }
        Ok(n)
    }
}

impl<'a> Progress<'a> {
    fn open<'b>(&'b mut self, path: &Path) -> AppResult<Tracked<'a, 'b>> {
        Ok(Tracked {
            file: File::open(path)?,
            progress: self,
        })
    }
}

/// An I/O error caused by cancelling becomes [`AppError::Cancelled`].
fn io_error(ctx: &JobContext, err: io::Error) -> AppError {
    if ctx.is_cancelled() {
        AppError::Cancelled
    } else {
        err.into()
    }
}

fn unix_seconds(time: Option<SystemTime>) -> u64 {
    time.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// UTC calendar date and time of a Unix timestamp (days-from-civil, Howard Hinnant).
fn civil(seconds: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        month,
        day,
        (rest / 3600) as u32,
        (rest / 60 % 60) as u32,
        (rest % 60) as u32,
    )
}

fn zip_time(time: Option<SystemTime>) -> zip::DateTime {
    let (y, mo, d, h, mi, s) = civil(unix_seconds(time));
    u16::try_from(y)
        .ok()
        .and_then(|y| {
            zip::DateTime::from_date_and_time(y, mo as u8, d as u8, h as u8, mi as u8, s as u8).ok()
        })
        .unwrap_or_default()
}

fn write_zip(items: &[Item], output: &Path, progress: &mut Progress) -> AppResult<()> {
    let ctx = progress.ctx;
    let mut writer = zip::ZipWriter::new(BufWriter::with_capacity(CHUNK, File::create(output)?));
    for item in items {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip_time(item.modified))
            .unix_permissions(0o644)
            .large_file(item.size >= u64::from(u32::MAX));
        writer
            .start_file(item.name.as_str(), options)
            .map_err(|err| AppError::Internal(format!("could not write ZIP: {err}")))?;
        let mut reader = progress.open(&item.path)?;
        io::copy(&mut reader, &mut writer).map_err(|err| io_error(ctx, err))?;
    }
    writer
        .finish()
        .map_err(|err| AppError::Internal(format!("could not write ZIP: {err}")))?
        .flush()?;
    Ok(())
}

fn write_tar<W: Write>(items: &[Item], writer: W, progress: &mut Progress) -> AppResult<W> {
    let ctx = progress.ctx;
    let mut builder = tar::Builder::new(writer);
    builder.mode(tar::HeaderMode::Deterministic);
    for item in items {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(item.size);
        header.set_mode(0o644);
        header.set_mtime(unix_seconds(item.modified));
        let mut reader = progress.open(&item.path)?;
        builder
            .append_data(&mut header, &item.name, &mut reader)
            .map_err(|err| io_error(ctx, err))?;
    }
    builder.into_inner().map_err(|err| io_error(ctx, err))
}

fn write_seven_z(items: &[Item], output: &Path, progress: &mut Progress) -> AppResult<()> {
    let ctx = progress.ctx;
    let seven_z_error = |err: sevenz_rust2::Error| {
        if ctx.is_cancelled() {
            AppError::Cancelled
        } else {
            AppError::Internal(format!("could not write 7z: {err}"))
        }
    };
    let mut writer = sevenz_rust2::ArchiveWriter::create(output).map_err(seven_z_error)?;
    for item in items {
        let entry = sevenz_rust2::ArchiveEntry::from_path(&item.path, item.name.clone());
        let reader = progress.open(&item.path)?;
        writer
            .push_archive_entry(entry, Some(reader))
            .map_err(seven_z_error)?;
    }
    writer.finish().map_err(|err| io_error(ctx, err))?;
    Ok(())
}

pub fn create(
    items: &[Item],
    format: ArchiveFormat,
    output: &Path,
    ctx: &JobContext,
) -> AppResult<()> {
    let mut progress = Progress {
        ctx,
        done: 0,
        total: items.iter().map(|i| i.size).sum(),
    };
    match format {
        ArchiveFormat::Zip => write_zip(items, output, &mut progress),
        ArchiveFormat::Tar => {
            let file = BufWriter::with_capacity(CHUNK, File::create(output)?);
            write_tar(items, file, &mut progress)?.flush()?;
            Ok(())
        }
        ArchiveFormat::TarGz => {
            let file = BufWriter::with_capacity(CHUNK, File::create(output)?);
            let gz = GzEncoder::new(file, Compression::default());
            write_tar(items, gz, &mut progress)?
                .finish()
                .map_err(|err| io_error(ctx, err))?
                .flush()?;
            Ok(())
        }
        ArchiveFormat::SevenZ => write_seven_z(items, output, &mut progress),
    }?;
    ctx.check_cancelled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_unix_time_to_calendar() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        // 2024-02-29 12:34:56 UTC
        assert_eq!(civil(1_709_210_096), (2024, 2, 29, 12, 34, 56));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
    }
}
