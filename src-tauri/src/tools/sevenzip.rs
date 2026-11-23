//! Extracting with the system 7-Zip (optional): RAR, xz, bzip2 and anything the Rust
//! readers do not support (encrypted ZIP and 7z, rare compression methods).
//!
//! 7-Zip already refuses paths that leave the output folder. After it runs, links it may
//! have created are removed, and the size of what it wrote is checked against the same
//! limit as the Rust readers.

use std::fs;
use std::path::Path;

use super::process::run_streaming;
use super::{ToolBox, ToolId};
use crate::error::{AppError, AppResult};
use crate::jobs::JobContext;

/// Given instead of a real password, so an encrypted archive fails instead of waiting for
/// one to be typed.
const NO_PASSWORD: &str = "-pEASY-CONVERTER-NO-PASSWORD";

pub async fn extract(
    tools: &ToolBox,
    ctx: &JobContext,
    input: &Path,
    out_dir: &Path,
    max_bytes: u64,
) -> AppResult<()> {
    let mut cmd = tools.command(ToolId::SevenZip)?;
    let mut target = std::ffi::OsString::from("-o");
    target.push(out_dir.as_os_str());
    cmd.current_dir(&ctx.temp_dir)
        .args(["x", "-y", "-bd", "-bso0", "-bsp0", NO_PASSWORD])
        .arg(target)
        .arg("--")
        .arg(input);
    ctx.set_indeterminate();
    run_streaming(cmd, |_| {}).await.map_err(|err| match err {
        AppError::ProcessFailed(message) => map_error(&message),
        other => other,
    })?;
    let written = clean_tree(out_dir)?;
    if written > max_bytes {
        return Err(too_large(max_bytes));
    }
    Ok(())
}

pub fn too_large(max_bytes: u64) -> AppError {
    AppError::TooLarge(format!(
        "the extracted files would take more than {} GB",
        max_bytes / (1024 * 1024 * 1024)
    ))
}

/// Removes links and special files and returns the total size of the regular files.
fn clean_tree(dir: &Path) -> AppResult<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = fs::symlink_metadata(entry.path())?.file_type();
        if kind.is_dir() {
            total += clean_tree(&entry.path())?;
        } else if kind.is_file() {
            total += entry.metadata()?.len();
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(total)
}

fn map_error(message: &str) -> AppError {
    let lower = message.to_lowercase();
    if lower.contains("wrong password") || lower.contains("can not open encrypted archive") {
        AppError::Password("the archive is protected by a password".into())
    } else if lower.contains("cannot open the file as archive")
        || lower.contains("is not archive")
        || lower.contains("unexpected end of archive")
        || lower.contains("data error")
        || lower.contains("crc failed")
    {
        AppError::InvalidFile("7-Zip could not read the archive (damaged or unsupported)".into())
    } else if lower.contains("unsupported method") {
        AppError::InvalidFile(
            "the archive uses a method this 7-Zip cannot read (RAR needs a 7-Zip with RAR support)"
                .into(),
        )
    } else {
        let tail: String = message
            .chars()
            .rev()
            .take(400)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        AppError::ProcessFailed(format!("7-Zip: {tail}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_7zip_errors() {
        assert!(matches!(
            map_error("ERROR: Wrong password : a.txt"),
            AppError::Password(_)
        ));
        assert!(matches!(
            map_error("ERROR: x.rar\nCannot open the file as archive"),
            AppError::InvalidFile(_)
        ));
        assert!(matches!(
            map_error("Unsupported Method : a.txt"),
            AppError::InvalidFile(_)
        ));
        assert!(matches!(map_error("odd"), AppError::ProcessFailed(_)));
    }

    #[cfg(unix)]
    #[test]
    fn removes_links_and_counts_bytes() {
        let dir = std::env::temp_dir().join(format!("ec-7z-clean-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/a.txt"), "12345").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", dir.join("link")).unwrap();
        assert_eq!(clean_tree(&dir).unwrap(), 5);
        assert!(fs::symlink_metadata(dir.join("link")).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
