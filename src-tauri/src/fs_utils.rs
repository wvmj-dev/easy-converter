//! Path validation, output naming and temporary workspace handling.
//!
//! Rules enforced here:
//! - the original file is never overwritten;
//! - work happens in a temporary folder and results are moved at the end;
//! - temporary folders are removed when a job ends and when the app exits.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::jobs::{JobCategory, JobId};
use crate::ops::image::ImageInfo;

/// Characters that are invalid in file names on Windows (and therefore avoided everywhere).
const FORBIDDEN_NAME_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Maximum length accepted for a user-provided file name fragment.
const MAX_NAME_FRAGMENT_LEN: usize = 40;

/// Temporary session folders older than this are considered leftovers of a crash.
const STALE_SESSION_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Canonical form of an existing path. On Windows the `\\?\` prefix that
/// `canonicalize` adds is removed when the path does not need it: such paths look odd in
/// the interface and several external tools (Ghostscript, Tesseract) cannot open them.
pub fn canonicalize(path: &Path) -> AppResult<PathBuf> {
    let canonical = path
        .canonicalize()
        .map_err(|_| AppError::NotFound(path.display().to_string()))?;
    if cfg!(windows) {
        Ok(strip_verbatim(&canonical))
    } else {
        Ok(canonical)
    }
}

/// `\\?\C:\dir` becomes `C:\dir` and `\\?\UNC\server\share` becomes
/// `\\server\share`; anything else, or a path too long for the classic form, is kept.
fn strip_verbatim(path: &Path) -> PathBuf {
    const MAX_CLASSIC_LEN: usize = 259;
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    let simplified = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        let bytes = rest.as_bytes();
        let is_drive = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\';
        if !is_drive {
            return path.to_path_buf();
        }
        rest.to_owned()
    } else {
        return path.to_path_buf();
    };
    if simplified.len() > MAX_CLASSIC_LEN {
        path.to_path_buf()
    } else {
        PathBuf::from(simplified)
    }
}

/// Validates a path received from the frontend that must point to an existing file.
/// Returns the canonical path.
pub fn validate_input_file(path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::InvalidPath(format!(
            "path must be absolute: {}",
            path.display()
        )));
    }
    let canonical = canonicalize(path)?;
    if !canonical.is_file() {
        return Err(AppError::InvalidPath(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

/// Validates a path that must point to an existing directory. Returns the canonical path.
pub fn validate_directory(path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::InvalidPath(format!(
            "path must be absolute: {}",
            path.display()
        )));
    }
    let canonical = canonicalize(path)?;
    if !canonical.is_dir() {
        return Err(AppError::InvalidPath(format!(
            "not a directory: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

/// Validates a file extension (without the dot): 1 to 10 ASCII letters or digits.
pub fn validate_extension(ext: &str) -> AppResult<()> {
    let valid =
        !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(AppError::InvalidParameter(format!(
            "invalid extension: {ext:?}"
        )))
    }
}

/// Validates a fragment that will be inserted in a file name, such as the output suffix.
pub fn validate_name_fragment(fragment: &str) -> AppResult<()> {
    let invalid = fragment.trim().is_empty()
        || fragment.chars().count() > MAX_NAME_FRAGMENT_LEN
        || fragment.starts_with('.')
        || fragment.ends_with('.')
        || fragment.ends_with(' ')
        || fragment
            .chars()
            .any(|c| c.is_control() || FORBIDDEN_NAME_CHARS.contains(&c));
    if invalid {
        Err(AppError::InvalidParameter(format!(
            "invalid file name fragment: {fragment:?}"
        )))
    } else {
        Ok(())
    }
}

/// Lowercase extension of a path, without the dot.
pub fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

/// Builds `<stem>-<suffix>.<ext>` for an input file.
pub fn output_file_name(input: &Path, suffix: &str, ext: &str) -> String {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_owned());
    format!("{stem}-{suffix}.{ext}")
}

/// Basic information about a file chosen by the user, returned by `probe_file`.
/// Details such as pages, duration and codecs are added by each phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub extension: Option<String>,
    pub size: u64,
    /// `null` when the format is not supported.
    pub category: Option<JobCategory>,
    /// Size and real format, for images whose header could be read.
    pub image: Option<ImageInfo>,
    /// Pages and protection, for PDFs (filled in by `probe_file` with qpdf).
    pub pdf: Option<crate::tools::qpdf::PdfInfo>,
}

/// Reads the metadata of a file received from the frontend.
pub fn probe(path: &Path) -> AppResult<FileInfo> {
    let path = validate_input_file(path)?;
    let size = fs::metadata(&path)?.len();
    let extension = extension_of(&path);
    let category = extension.as_deref().and_then(detect_category);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // An image that cannot be read is refused now instead of failing every job later.
    let image = match category {
        Some(JobCategory::Image) => Some(
            crate::ops::image::probe(&path)
                .ok_or_else(|| AppError::InvalidFile("not a supported image format".into()))?,
        ),
        _ => None,
    };
    Ok(FileInfo {
        path,
        name,
        extension,
        size,
        category,
        image,
        pdf: None,
    })
}

/// Category of a file from its (lowercase) extension.
pub fn detect_category(ext: &str) -> Option<JobCategory> {
    let category = match ext {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tif" | "tiff" | "ico" => {
            JobCategory::Image
        }
        "pdf" => JobCategory::Pdf,
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "m4v" | "wmv" | "flv" | "mpg" | "mpeg" | "ts"
        | "3gp" => JobCategory::Video,
        "mp3" | "wav" | "flac" | "ogg" | "oga" | "opus" | "m4a" | "aac" | "wma" | "aiff"
        | "aif" => JobCategory::Audio,
        "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "markdown" | "html" | "htm" => {
            JobCategory::Document
        }
        "xls" | "xlsx" | "ods" | "csv" | "json" => JobCategory::Spreadsheet,
        "ppt" | "pptx" | "odp" => JobCategory::Presentation,
        "srt" | "vtt" | "ass" | "ssa" => JobCategory::Subtitle,
        "epub" | "mobi" | "azw3" | "azw" | "fb2" => JobCategory::Ebook,
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" => JobCategory::Archive,
        _ => return None,
    };
    Some(category)
}

/// Path with ` (n)` appended to the file stem: `name (2).ext`.
fn numbered_path(path: &Path, n: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
        None => format!("{stem} ({n})"),
    };
    path.with_file_name(name)
}

/// Atomically reserves a destination path that does not exist yet, numbering it if needed
/// (`name.ext`, `name (2).ext`, `name (3).ext`...). An empty file is created as the
/// reservation, so concurrent jobs never pick the same name and nothing is overwritten.
pub fn reserve_unique_path(desired: &Path) -> AppResult<PathBuf> {
    for n in 1..=10_000u32 {
        let candidate = if n == 1 {
            desired.to_path_buf()
        } else {
            numbered_path(desired, n)
        };
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(AppError::Internal(format!(
        "could not find a free file name for {}",
        desired.display()
    )))
}

/// Moves a finished file from the temporary folder to a path reserved by
/// [`reserve_unique_path`]. Falls back to copying when the rename crosses file systems.
/// On failure, the reservation is removed.
pub fn move_into_reserved(temp: &Path, reserved: &Path) -> AppResult<()> {
    if fs::rename(temp, reserved).is_ok() {
        return Ok(());
    }
    match fs::copy(temp, reserved) {
        Ok(_) => {
            // The copy succeeded; a leftover temp file is removed with the job folder anyway.
            let _ = fs::remove_file(temp);
            Ok(())
        }
        Err(err) => {
            let _ = fs::remove_file(reserved);
            Err(err.into())
        }
    }
}

/// Temporary workspace for the current app session.
///
/// Layout: `<system temp>/<app identifier>/session-<pid>/job-<id>/`.
#[derive(Debug, Clone)]
pub struct TempSpace {
    root: PathBuf,
}

impl TempSpace {
    /// Creates the session folder under `base` (usually `<system temp>/<app identifier>`)
    /// and removes stale session folders left by previous runs.
    pub fn create(base: &Path) -> AppResult<Self> {
        fs::create_dir_all(base)?;
        remove_stale_sessions(base, STALE_SESSION_AGE);
        let root = base.join(format!("session-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Creates (or recreates empty) the folder for a job.
    pub fn create_job_dir(&self, id: JobId) -> AppResult<PathBuf> {
        let dir = self.job_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn job_dir(&self, id: JobId) -> PathBuf {
        self.root.join(format!("job-{id}"))
    }

    pub fn remove_job_dir(&self, id: JobId) {
        let dir = self.job_dir(id);
        if dir.exists() {
            if let Err(err) = fs::remove_dir_all(&dir) {
                eprintln!("failed to remove temp folder {}: {err}", dir.display());
            }
        }
    }

    /// Removes the whole session folder. Called when the app exits.
    pub fn remove_all(&self) {
        if self.root.exists() {
            if let Err(err) = fs::remove_dir_all(&self.root) {
                eprintln!(
                    "failed to remove temp folder {}: {err}",
                    self.root.display()
                );
            }
        }
    }
}

/// Removes `session-*` folders in `base` whose last modification is older than `max_age`.
fn remove_stale_sessions(base: &Path, max_age: Duration) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_session = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with("session-"));
        if !is_session || !path.is_dir() {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= max_age);
        if old_enough && !session_owner_is_alive(&entry.file_name().to_string_lossy()) {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Best-effort check that the process which created `session-<pid>` is still running.
#[cfg(target_os = "linux")]
fn session_owner_is_alive(folder_name: &str) -> bool {
    folder_name
        .strip_prefix("session-")
        .and_then(|pid| pid.parse::<u32>().ok())
        .is_some_and(|pid| pid != std::process::id() && Path::new(&format!("/proc/{pid}")).exists())
}

#[cfg(not(target_os = "linux"))]
fn session_owner_is_alive(_folder_name: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("easy-converter-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn output_name_appends_suffix_and_extension() {
        let name = output_file_name(Path::new("/tmp/photo.final.PNG"), "convertido", "jpg");
        assert_eq!(name, "photo.final-convertido.jpg");
    }

    #[test]
    fn reserve_numbers_existing_files() {
        let dir = scratch("reserve");
        let desired = dir.join("a-convertido.jpg");

        let first = reserve_unique_path(&desired).unwrap();
        let second = reserve_unique_path(&desired).unwrap();
        let third = reserve_unique_path(&desired).unwrap();

        assert_eq!(first, desired);
        assert_eq!(second, dir.join("a-convertido (2).jpg"));
        assert_eq!(third, dir.join("a-convertido (3).jpg"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reserve_never_touches_existing_content() {
        let dir = scratch("keep");
        let original = dir.join("file.txt");
        fs::write(&original, "original").unwrap();

        let reserved = reserve_unique_path(&original).unwrap();

        assert_ne!(reserved, original);
        assert_eq!(fs::read_to_string(&original).unwrap(), "original");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn move_into_reserved_replaces_only_the_reservation() {
        let dir = scratch("move");
        let temp = dir.join("temp.bin");
        fs::write(&temp, "result").unwrap();
        let reserved = reserve_unique_path(&dir.join("out.bin")).unwrap();

        move_into_reserved(&temp, &reserved).unwrap();

        assert_eq!(fs::read_to_string(&reserved).unwrap(), "result");
        assert!(!temp.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn validates_name_fragments() {
        assert!(validate_name_fragment("convertido").is_ok());
        assert!(validate_name_fragment("converted file").is_ok());
        for bad in [
            "", "  ", "../x", "a/b", "a\\b", "a:b", ".hidden", "dot.", "x\n", "a*",
        ] {
            assert!(
                validate_name_fragment(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
        assert!(validate_name_fragment(&"x".repeat(41)).is_err());
    }

    #[test]
    fn validates_extensions() {
        assert!(validate_extension("jpg").is_ok());
        assert!(validate_extension("mp4").is_ok());
        for bad in ["", ".jpg", "j/pg", "verylongextension", "tar.gz"] {
            assert!(
                validate_extension(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn validate_input_file_rejects_relative_missing_and_dirs() {
        let dir = scratch("input");
        let file = dir.join("in.txt");
        fs::write(&file, "x").unwrap();

        assert!(validate_input_file(&file).is_ok());
        assert!(matches!(
            validate_input_file(Path::new("relative.txt")),
            Err(AppError::InvalidPath(_))
        ));
        assert!(matches!(
            validate_input_file(&dir.join("missing.txt")),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            validate_input_file(&dir),
            Err(AppError::InvalidPath(_))
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn verbatim_prefix_is_removed_only_when_safe() {
        let strip = |p: &str| strip_verbatim(Path::new(p)).to_string_lossy().into_owned();
        assert_eq!(strip(r"\\?\C:\Users\a\b.pdf"), r"C:\Users\a\b.pdf");
        assert_eq!(
            strip(r"\\?\UNC\server\share\a.pdf"),
            r"\\server\share\a.pdf"
        );
        // Device paths and paths too long for the classic form keep the prefix.
        assert_eq!(strip(r"\\?\Volume{1234}\a.pdf"), r"\\?\Volume{1234}\a.pdf");
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(strip(&long), long);
        assert_eq!(strip("/home/a.pdf"), "/home/a.pdf");
    }

    #[test]
    fn probe_detects_category_and_size() {
        let dir = scratch("probe");
        let file = dir.join("Report.PDF");
        fs::write(&file, [0u8; 10]).unwrap();

        let info = probe(&file).unwrap();
        assert_eq!(info.name, "Report.PDF");
        assert_eq!(info.extension.as_deref(), Some("pdf"));
        assert_eq!(info.size, 10);
        assert_eq!(info.category, Some(JobCategory::Pdf));
        assert_eq!(info.image, None);

        // An image extension on something that is not an image is refused up front.
        let fake = dir.join("Photo.JPG");
        fs::write(&fake, b"not an image").unwrap();
        assert!(matches!(probe(&fake), Err(AppError::InvalidFile(_))));

        let other = dir.join("notes.xyz");
        fs::write(&other, "").unwrap();
        assert_eq!(probe(&other).unwrap().category, None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn temp_space_creates_and_removes_job_dirs() {
        let base = scratch("tempspace");
        let space = TempSpace::create(&base).unwrap();
        let job = space.create_job_dir(7).unwrap();
        fs::write(job.join("partial.tmp"), "x").unwrap();

        space.remove_job_dir(7);
        assert!(!job.exists());

        space.remove_all();
        assert!(!space.root().exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn stale_sessions_are_removed() {
        let base = scratch("stale");
        // A pid far above the kernel limit, so it never belongs to a live process.
        let stale = base.join("session-4294967");
        let unrelated = base.join("other");
        fs::create_dir_all(&stale).unwrap();
        fs::create_dir_all(&unrelated).unwrap();

        remove_stale_sessions(&base, Duration::ZERO);

        assert!(!stale.exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(base).unwrap();
    }
}
