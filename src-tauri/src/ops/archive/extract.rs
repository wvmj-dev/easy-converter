//! Extracting ZIP, TAR, TAR.GZ, GZ and 7z in Rust.
//!
//! Every entry goes through [`Sink`], which:
//! - keeps paths inside the output folder (no `..`, absolute names made relative);
//! - never creates links or special files (only folders and regular files);
//! - never overwrites: a repeated name gets a number;
//! - counts the bytes really written and stops past a limit, whatever the archive claims
//!   (decompression bombs);
//! - checks for cancellation while copying.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use flate2::read::MultiGzDecoder;

use super::names::safe_relative;
use crate::error::{AppError, AppResult};
use crate::jobs::JobContext;
use crate::tools::sevenzip;

/// Most bytes extracted from one archive.
pub const MAX_EXTRACT_BYTES: u64 = 32 * 1024 * 1024 * 1024;
/// Most entries extracted from one archive.
pub const MAX_ENTRIES: usize = 200_000;
const CHUNK: usize = 64 * 1024;

/// Why a Rust reader gave up: the archive may still open with 7-Zip.
#[derive(Debug)]
pub enum Failure {
    App(AppError),
    /// Encrypted, or a compression method the crates do not include.
    NeedsSevenZip(String),
}

impl From<AppError> for Failure {
    fn from(err: AppError) -> Self {
        Failure::App(err)
    }
}

impl From<io::Error> for Failure {
    fn from(err: io::Error) -> Self {
        Failure::App(err.into())
    }
}

pub type ExtractResult = Result<(), Failure>;

/// Writes entries into the output folder, enforcing the rules above.
pub struct Sink<'a> {
    root: PathBuf,
    ctx: &'a JobContext,
    max_bytes: u64,
    written: u64,
    entries: usize,
}

impl<'a> Sink<'a> {
    pub fn new(root: &Path, ctx: &'a JobContext, max_bytes: u64) -> Self {
        Self {
            root: root.to_path_buf(),
            ctx,
            max_bytes,
            written: 0,
            entries: 0,
        }
    }

    fn count_entry(&mut self) -> AppResult<()> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(AppError::TooLarge(format!(
                "the archive has more than {MAX_ENTRIES} entries"
            )));
        }
        Ok(())
    }

    pub fn dir(&mut self, name: &str) -> AppResult<()> {
        self.count_entry()?;
        if let Some(rel) = safe_relative(name) {
            fs::create_dir_all(self.root.join(rel))?;
        }
        Ok(())
    }

    /// Copies `reader` into a new file named after `name`. Unsafe names are skipped.
    pub fn file(&mut self, name: &str, reader: &mut dyn Read) -> AppResult<()> {
        self.count_entry()?;
        self.ctx.check_cancelled()?;
        let Some(rel) = safe_relative(name) else {
            return Ok(());
        };
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = create_unique(&path)?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(read_error(err)),
            };
            self.written += n as u64;
            if self.written > self.max_bytes {
                return Err(sevenzip::too_large(self.max_bytes));
            }
            out.write_all(&buf[..n])?;
            self.ctx.check_cancelled()?;
        }
        Ok(())
    }
}

/// A damaged stream shows up as an I/O error while reading.
fn read_error(err: io::Error) -> AppError {
    match err.kind() {
        ErrorKind::InvalidData | ErrorKind::InvalidInput | ErrorKind::UnexpectedEof => {
            AppError::InvalidFile(format!("the archive is damaged: {err}"))
        }
        _ => err.into(),
    }
}

/// Creates `path`, or `name (2).ext`... when an entry with that name was already written.
fn create_unique(path: &Path) -> AppResult<File> {
    for n in 1..=10_000u32 {
        let candidate = if n == 1 {
            path.to_path_buf()
        } else {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = match path.extension() {
                Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
                None => format!("{stem} ({n})"),
            };
            path.with_file_name(name)
        };
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok(file),
            // A folder of that name (from another entry) also counts as taken.
            Err(err) if err.kind() == ErrorKind::AlreadyExists || candidate.is_dir() => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(AppError::Internal(format!(
        "too many files named {}",
        path.display()
    )))
}

/// Reports how much of the input file has been read, as the job progress.
struct Metered<'a, R> {
    inner: R,
    ctx: &'a JobContext,
    read: u64,
    total: u64,
}

impl<R: Read> Read for Metered<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        if self.total > 0 {
            self.ctx
                .set_progress(self.read as f32 * 100.0 / self.total as f32);
        }
        Ok(n)
    }
}

fn metered<'a>(input: &Path, ctx: &'a JobContext) -> AppResult<Metered<'a, BufReader<File>>> {
    let file = File::open(input)?;
    let total = file.metadata()?.len();
    Ok(Metered {
        inner: BufReader::with_capacity(CHUNK, file),
        ctx,
        read: 0,
        total,
    })
}

fn tar_entries(reader: impl Read, sink: &mut Sink) -> ExtractResult {
    let mut archive = tar::Archive::new(reader);
    let entries = archive
        .entries()
        .map_err(|err| AppError::InvalidFile(format!("not a TAR archive: {err}")))?;
    for entry in entries {
        let mut entry = entry.map_err(read_error)?;
        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            sink.dir(&name)?;
        } else if kind.is_file() || kind == tar::EntryType::Continuous || kind.is_gnu_sparse() {
            sink.file(&name, &mut entry)?;
        }
        // Links, devices and FIFOs are never created.
    }
    Ok(())
}

pub fn tar(input: &Path, sink: &mut Sink, ctx: &JobContext) -> ExtractResult {
    tar_entries(metered(input, ctx)?, sink)
}

pub fn tar_gz(input: &Path, sink: &mut Sink, ctx: &JobContext) -> ExtractResult {
    tar_entries(MultiGzDecoder::new(metered(input, ctx)?), sink)
}

/// A single compressed file: its name without `.gz`.
pub fn gz(input: &Path, name: &str, sink: &mut Sink, ctx: &JobContext) -> ExtractResult {
    let mut decoder = MultiGzDecoder::new(metered(input, ctx)?);
    sink.file(name, &mut decoder)?;
    Ok(())
}

pub fn zip(input: &Path, sink: &mut Sink, ctx: &JobContext) -> ExtractResult {
    let mut archive = zip::ZipArchive::new(BufReader::new(File::open(input)?))
        .map_err(|err| AppError::InvalidFile(format!("not a ZIP archive: {err}")))?;
    let total = archive.len();
    // Check every entry first, so an encrypted or unsupported one sends the whole
    // archive to 7-Zip before anything is written.
    for index in 0..total {
        let entry = archive.by_index_raw(index).map_err(zip_failure)?;
        if entry.encrypted() {
            return Err(Failure::NeedsSevenZip(
                "the ZIP archive is encrypted".into(),
            ));
        }
        if !matches!(
            entry.compression(),
            zip::CompressionMethod::Stored
                | zip::CompressionMethod::Deflated
                | zip::CompressionMethod::Deflate64
        ) {
            return Err(Failure::NeedsSevenZip(format!(
                "the ZIP archive uses {:?} compression",
                entry.compression()
            )));
        }
    }
    for index in 0..total {
        let mut entry = archive.by_index(index).map_err(zip_failure)?;
        let name = entry.name().map_err(zip_failure)?.into_owned();
        if entry.is_dir() {
            sink.dir(&name)?;
        } else if entry.is_symlink() {
            sink.count_entry()?;
        } else {
            sink.file(&name, &mut entry)?;
        }
        ctx.set_steps(index + 1, total);
    }
    Ok(())
}

fn zip_failure(err: zip::result::ZipError) -> Failure {
    use zip::result::ZipError;
    match err {
        ZipError::UnsupportedArchive(what) => Failure::NeedsSevenZip(what.to_owned()),
        ZipError::CompressionMethodNotSupported(method) => {
            Failure::NeedsSevenZip(format!("compression method {method}"))
        }
        ZipError::InvalidPassword => Failure::NeedsSevenZip("the ZIP archive is encrypted".into()),
        ZipError::Io(err) => Failure::App(read_error(err)),
        other => Failure::App(AppError::InvalidFile(format!(
            "the ZIP archive is damaged: {other}"
        ))),
    }
}

pub fn seven_z(input: &Path, sink: &mut Sink, ctx: &JobContext) -> ExtractResult {
    use sevenz_rust2::{ArchiveReader, Error, Password};
    let open = ArchiveReader::new(BufReader::new(File::open(input)?), Password::empty());
    let mut reader = match open {
        Ok(reader) => reader,
        Err(err) => return Err(seven_z_failure(err, None)),
    };
    let total: u64 = reader.archive().files.iter().map(|f| f.size).sum();
    let mut done = 0u64;
    // Errors of the app are kept here and the reader is stopped with a placeholder.
    let mut app_error: Option<AppError> = None;
    let result = reader.for_each_entries(|entry, data| {
        let outcome = if entry.is_anti_item() {
            Ok(())
        } else if entry.is_directory() {
            sink.dir(entry.name())
        } else {
            sink.file(entry.name(), data)
        };
        match outcome {
            Ok(()) => {
                done += entry.size();
                if total > 0 {
                    ctx.set_progress(done as f32 * 100.0 / total as f32);
                }
                Ok(true)
            }
            Err(err) => {
                app_error = Some(err);
                Err(Error::Other("stopped".into()))
            }
        }
    });
    match result {
        Ok(()) => Ok(()),
        Err(err) => Err(seven_z_failure(err, app_error)),
    }
}

fn seven_z_failure(err: sevenz_rust2::Error, app_error: Option<AppError>) -> Failure {
    use sevenz_rust2::Error;
    if let Some(app) = app_error {
        return Failure::App(app);
    }
    match err {
        Error::PasswordRequired | Error::MaybeBadPassword(_) => {
            Failure::NeedsSevenZip("the 7z archive is encrypted".into())
        }
        Error::UnsupportedCompressionMethod(method) => {
            Failure::NeedsSevenZip(format!("compression method {method}"))
        }
        Error::ExternalUnsupported | Error::Unsupported(_) => {
            Failure::NeedsSevenZip("an unsupported 7z feature".into())
        }
        Error::Io(err, _) | Error::FileOpen(err, _) if err.kind() == ErrorKind::NotFound => {
            Failure::App(err.into())
        }
        other => Failure::App(AppError::InvalidFile(format!(
            "the 7z archive is damaged: {other}"
        ))),
    }
}
