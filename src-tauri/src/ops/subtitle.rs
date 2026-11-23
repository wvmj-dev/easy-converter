//! Subtitle operations (phase 5).
//!
//! - Converting between SRT, WebVTT and ASS and shifting times: Rust (see `format`).
//!   Input may be UTF-8 or Windows-1252; output is UTF-8.
//! - Embedding a subtitle in a video: FFmpeg, as a selectable track (soft subtitle),
//!   copying the video and audio as they are.

mod format;

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use ts_rs::TS;

use super::media::{self, require_ffmpeg, VideoContainer};
use super::office::{self, decode_text, detect_encoding, read_head, TextEncoding};
use super::{blocking_job, process_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::jobs::{JobCategory, JobSpec, PlannedOutput};
use crate::tools::ffmpeg;
use crate::tools::ToolBox;

pub use format::Kind;

/// Largest subtitle file accepted. Real ones are a few hundred kilobytes.
pub const MAX_SUBTITLE_BYTES: u64 = 20 * 1024 * 1024;
/// Largest shift: one day either way.
pub const MAX_OFFSET_MS: i64 = 24 * 3_600_000;
const SUBTITLE_EXTENSIONS: [&str; 4] = ["srt", "vtt", "ass", "ssa"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SubtitleFormat {
    Srt,
    Vtt,
    Ass,
}

impl SubtitleFormat {
    fn kind(self) -> Kind {
        match self {
            SubtitleFormat::Srt => Kind::Srt,
            SubtitleFormat::Vtt => Kind::Vtt,
            SubtitleFormat::Ass => Kind::Ass,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            SubtitleFormat::Srt => "srt",
            SubtitleFormat::Vtt => "vtt",
            SubtitleFormat::Ass => "ass",
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SubtitleConvertParams {
    pub input: PathBuf,
    pub format: SubtitleFormat,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SubtitleShiftParams {
    pub input: PathBuf,
    /// Milliseconds to add to every time; negative makes subtitles appear earlier.
    #[ts(type = "number")]
    pub offset_ms: i64,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VideoEmbedSubtitleParams {
    /// The video.
    pub input: PathBuf,
    pub subtitle: PathBuf,
    /// ISO 639-2 code of the subtitle language (`por`, `eng`...), when known.
    #[serde(default)]
    #[ts(optional)]
    pub language: Option<String>,
}

/// Validates a subtitle input and returns it with its kind.
fn validate_subtitle(path: &Path) -> AppResult<(PathBuf, Kind)> {
    let (path, ext) = office::validate_input(path, &SUBTITLE_EXTENSIONS, "subtitle file")?;
    if fs::metadata(&path)?.len() > MAX_SUBTITLE_BYTES {
        return Err(AppError::TooLarge(format!(
            "subtitle files can have at most {} MB",
            MAX_SUBTITLE_BYTES / (1024 * 1024)
        )));
    }
    let kind = match ext.as_str() {
        "srt" => Kind::Srt,
        "vtt" => Kind::Vtt,
        _ => Kind::Ass,
    };
    Ok((path, kind))
}

fn read_subtitle(path: &Path) -> AppResult<String> {
    Ok(decode_text(fs::read(path)?))
}

/// The output keeps the input's extension (`.ssa` stays `.ssa`).
fn same_extension(path: &Path) -> String {
    crate::fs_utils::extension_of(path).unwrap_or_else(|| "srt".to_owned())
}

pub fn convert(params: SubtitleConvertParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let (input, from) = validate_subtitle(&params.input)?;
    let to = params.format.kind();
    if from == to {
        return Err(AppError::InvalidParameter(format!(
            "the file is already {}",
            params.format.extension().to_uppercase()
        )));
    }
    let ext = params.format.extension();
    let destination = planner.destination(&input, ext)?;
    let work_input = input.clone();
    Ok(blocking_job(
        "subtitleConvert",
        JobCategory::Subtitle,
        vec![input],
        move |ctx| {
            let text = read_subtitle(&work_input)?;
            ctx.check_cancelled()?;
            let converted = format::convert(from, to, &text)?;
            let temp = ctx.temp_dir.join(format!("output.{ext}"));
            fs::write(&temp, converted)?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn shift(params: SubtitleShiftParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    let (input, kind) = validate_subtitle(&params.input)?;
    let offset = params.offset_ms;
    if offset == 0 || offset.abs() > MAX_OFFSET_MS {
        return Err(AppError::InvalidParameter(
            "the shift must be between -24 and 24 hours, and not zero".into(),
        ));
    }
    let ext = same_extension(&input);
    let destination = planner.destination(&input, &ext)?;
    let work_input = input.clone();
    Ok(blocking_job(
        "subtitleShift",
        JobCategory::Subtitle,
        vec![input],
        move |ctx| {
            let text = read_subtitle(&work_input)?;
            ctx.check_cancelled()?;
            let moved = format::shift(kind, &text, offset)?;
            let temp = ctx.temp_dir.join(format!("output.{ext}"));
            fs::write(&temp, moved)?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

fn validate_language(language: Option<&str>) -> AppResult<Option<String>> {
    match language.map(str::trim).filter(|l| !l.is_empty()) {
        None => Ok(None),
        Some(code) if code.len() == 3 && code.chars().all(|c| c.is_ascii_lowercase()) => {
            Ok(Some(code.to_owned()))
        }
        Some(code) => Err(AppError::InvalidParameter(format!(
            "language must be a three-letter code such as \"por\": {code:?}"
        ))),
    }
}

/// Container of the result and the subtitle codec it stores. AVI and the formats the
/// app does not write cannot hold text subtitles well, so they become MKV.
fn embed_target(video: &Path, kind: Kind) -> (VideoContainer, &'static str) {
    match VideoContainer::for_input(video) {
        container @ (VideoContainer::Mp4 | VideoContainer::Mov)
            if VideoContainer::Mp4.is_native(video) || container == VideoContainer::Mov =>
        {
            (container, "mov_text")
        }
        VideoContainer::Webm => (VideoContainer::Webm, "webvtt"),
        _ => (
            VideoContainer::Mkv,
            match kind {
                Kind::Srt => "srt",
                Kind::Vtt => "webvtt",
                Kind::Ass => "ass",
            },
        ),
    }
}

pub fn embed(
    params: VideoEmbedSubtitleParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = media::validate_input(&params.input, JobCategory::Video)?;
    let (subtitle, kind) = validate_subtitle(&params.subtitle)?;
    let language = validate_language(params.language.as_deref())?;
    let (container, codec) = embed_target(&input, kind);
    let destination = planner.destination(&input, container.extension())?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "videoEmbedSubtitle",
        JobCategory::Video,
        vec![input, subtitle.clone()],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let existing = info.subtitle_count;
            let temp = ctx
                .temp_dir
                .join(format!("output.{}", container.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            ffmpeg::add_input(&mut cmd, &work_input);
            if detect_encoding(&read_head(&subtitle)?) == TextEncoding::Windows1252 {
                cmd.args(["-sub_charenc", "CP1252"]);
            }
            ffmpeg::add_input(&mut cmd, &subtitle);
            cmd.args(["-map", "0:v", "-map", "0:a?", "-map", "0:s?", "-map", "1:0"])
                .args(["-c", "copy"])
                .arg(format!("-c:s:{existing}"))
                .arg(codec);
            if let Some(language) = &language {
                cmd.arg(format!("-metadata:s:s:{existing}"))
                    .arg(format!("language={language}"));
            }
            container.output_args(&mut cmd);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, info.duration(), (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_are_three_lowercase_letters() {
        assert_eq!(validate_language(None).unwrap(), None);
        assert_eq!(validate_language(Some(" ")).unwrap(), None);
        assert_eq!(
            validate_language(Some("por")).unwrap().as_deref(),
            Some("por")
        );
        for bad in ["pt", "POR", "p;r", "port"] {
            assert!(validate_language(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn picks_a_container_that_holds_text_subtitles() {
        assert_eq!(
            embed_target(Path::new("/v/a.mp4"), Kind::Ass),
            (VideoContainer::Mp4, "mov_text")
        );
        assert_eq!(
            embed_target(Path::new("/v/a.m4v"), Kind::Srt),
            (VideoContainer::Mp4, "mov_text")
        );
        assert_eq!(
            embed_target(Path::new("/v/a.mov"), Kind::Srt),
            (VideoContainer::Mov, "mov_text")
        );
        assert_eq!(
            embed_target(Path::new("/v/a.webm"), Kind::Srt),
            (VideoContainer::Webm, "webvtt")
        );
        assert_eq!(
            embed_target(Path::new("/v/a.mkv"), Kind::Ass),
            (VideoContainer::Mkv, "ass")
        );
        assert_eq!(
            embed_target(Path::new("/v/a.avi"), Kind::Srt),
            (VideoContainer::Mkv, "srt")
        );
        // WMV, FLV... are not written by the app; MKV takes any stream.
        assert_eq!(
            embed_target(Path::new("/v/a.wmv"), Kind::Vtt),
            (VideoContainer::Mkv, "webvtt")
        );
    }
}
