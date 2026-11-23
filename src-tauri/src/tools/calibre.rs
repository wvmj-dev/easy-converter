//! Running Calibre's `ebook-convert`.
//!
//! Every run uses the job's temporary folder for Calibre's settings and temporary files,
//! so the user's own Calibre library and preferences are never read or changed, and has
//! its HTTP clients (Python and the Chromium engine used for PDF) pointed to a closed
//! local port, so a book that references remote resources cannot reach the network.

use std::path::Path;

use super::libreoffice::{block_network, fresh_dir, DEAD_PROXY};
use super::process::run_streaming;
use super::{ToolBox, ToolId};
use crate::error::{AppError, AppResult};
use crate::jobs::JobContext;

/// Converts `input` into `output`; the output format comes from its extension.
pub async fn convert(
    tools: &ToolBox,
    ctx: &JobContext,
    input: &Path,
    output: &Path,
) -> AppResult<()> {
    let config = fresh_dir(&ctx.temp_dir, "calibre-config")?;
    let temp = fresh_dir(&ctx.temp_dir, "calibre-temp")?;
    let mut cmd = tools.command(ToolId::Calibre)?;
    block_network(&mut cmd);
    cmd.current_dir(&ctx.temp_dir)
        .env("CALIBRE_CONFIG_DIRECTORY", &config)
        .env("CALIBRE_TEMP_DIR", &temp)
        .env("QT_QPA_PLATFORM", "offscreen")
        .env(
            "QTWEBENGINE_CHROMIUM_FLAGS",
            format!("--proxy-server={DEAD_PROXY}"),
        )
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .arg(input)
        .arg(output);
    ctx.set_progress(0.0);
    run_streaming(cmd, |line| {
        if let Some(percent) = parse_progress(line) {
            ctx.set_progress(percent);
        }
    })
    .await
    .map_err(|err| match err {
        AppError::ProcessFailed(message) => map_error(&message),
        other => other,
    })?;
    if output.is_file() {
        Ok(())
    } else {
        Err(AppError::ProcessFailed("Calibre produced no file".into()))
    }
}

/// Calibre prints its progress as lines like `34% Running transforms on e-book...`.
fn parse_progress(line: &str) -> Option<f32> {
    let (number, _) = line.trim_start().split_once('%')?;
    let value: f32 = number.parse().ok()?;
    (0.0..=100.0).contains(&value).then_some(value)
}

fn map_error(message: &str) -> AppError {
    let lower = message.to_lowercase();
    if lower.contains("drm") {
        AppError::InvalidFile("the book is protected by DRM and cannot be converted".into())
    } else if lower.contains("not a valid")
        || lower.contains("badzipfile")
        || lower.contains("unknown book type")
    {
        AppError::InvalidFile("Calibre could not read the book (damaged or unsupported)".into())
    } else {
        let tail: String = message
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or(message)
            .chars()
            .take(400)
            .collect();
        AppError::ProcessFailed(format!("Calibre: {tail}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_progress_lines() {
        assert_eq!(
            parse_progress("34% Running transforms on e-book..."),
            Some(34.0)
        );
        assert_eq!(
            parse_progress("  1% Converting input to HTML..."),
            Some(1.0)
        );
        assert_eq!(parse_progress("Output saved to x"), None);
        assert_eq!(parse_progress("500% nonsense"), None);
    }

    #[test]
    fn explains_drm_and_damaged_books() {
        assert!(matches!(
            map_error("calibre.ebooks.DRMError: This file is locked with DRM"),
            AppError::InvalidFile(_)
        ));
        assert!(matches!(
            map_error("zipfile.BadZipFile: File is not a zip file"),
            AppError::InvalidFile(_)
        ));
        assert!(
            matches!(map_error("Traceback...\nValueError: odd"), AppError::ProcessFailed(m) if m.ends_with("ValueError: odd"))
        );
    }
}
