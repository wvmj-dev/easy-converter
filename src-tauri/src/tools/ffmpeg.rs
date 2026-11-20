//! FFmpeg and ffprobe: media information, command building and progress.
//!
//! - Every file is given as a `file:` URL of an absolute, canonical path, so a name can
//!   never be read as an option or as another protocol.
//! - Inputs are opened with `-protocol_whitelist file`: a crafted file (a playlist disguised
//!   as a video, for example) can never make FFmpeg open a network address.
//! - Progress comes from `-progress pipe:1`, compared with the duration from ffprobe.

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use ts_rs::TS;

use super::process::{capture, run_streaming, ProcessOutput};
use super::{ToolBox, ToolId};
use crate::error::{AppError, AppResult};
use crate::jobs::JobContext;

/// Timeout for ffprobe.
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest media duration accepted (protects the progress math and the arguments).
pub const MAX_DURATION_MS: u64 = 100 * 3600 * 1000;

// ---------------------------------------------------------------------------------------
// Media information
// ---------------------------------------------------------------------------------------

/// What the app knows about a video or audio file, read with ffprobe.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MediaInfo {
    /// Duration in milliseconds, when known.
    pub duration_ms: Option<u64>,
    /// The main video track (cover pictures are ignored).
    pub video: Option<VideoStream>,
    pub audio: Vec<AudioStream>,
    pub subtitle_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VideoStream {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    /// Frames per second, for display.
    pub fps: Option<f64>,
    /// Exact frame rate as `num/den`, for filters.
    #[serde(skip)]
    #[ts(skip)]
    pub frame_rate: Option<String>,
    #[serde(skip)]
    #[ts(skip)]
    pub pix_fmt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AudioStream {
    pub codec: String,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
}

impl MediaInfo {
    pub fn duration(&self) -> Option<Duration> {
        self.duration_ms.map(Duration::from_millis)
    }
}

#[derive(Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    #[serde(default)]
    disposition: ProbeDisposition,
}

#[derive(Deserialize, Default)]
struct ProbeDisposition {
    #[serde(default)]
    attached_pic: u8,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

/// Reads a file's tracks and duration with ffprobe.
pub async fn probe(tools: &ToolBox, path: &Path) -> AppResult<MediaInfo> {
    let mut cmd = tools.command(ToolId::Ffprobe)?;
    cmd.args(["-v", "error", "-protocol_whitelist", "file"])
        .args(["-print_format", "json", "-show_entries"])
        .arg(
            "format=duration:stream=codec_type,codec_name,width,height,pix_fmt,\
             avg_frame_rate,r_frame_rate,sample_rate,channels:stream_disposition=attached_pic",
        )
        .arg(file_url(path));
    let out = capture(cmd, PROBE_TIMEOUT).await?;
    if !out.status.success() {
        return Err(AppError::InvalidFile(last_line(&out.stderr)));
    }
    parse_probe(&out.stdout)
}

/// Turns ffprobe's JSON into [`MediaInfo`].
pub fn parse_probe(json: &str) -> AppResult<MediaInfo> {
    let parsed: ProbeOutput = serde_json::from_str(json)
        .map_err(|err| AppError::InvalidFile(format!("unreadable media information: {err}")))?;
    let mut info = MediaInfo {
        duration_ms: parsed
            .format
            .and_then(|f| f.duration)
            .and_then(|d| parse_seconds(&d)),
        video: None,
        audio: Vec::new(),
        subtitle_count: 0,
    };
    for stream in parsed.streams {
        let codec = stream.codec_name.clone().unwrap_or_default();
        match stream.codec_type.as_deref() {
            Some("video") if info.video.is_none() && stream.disposition.attached_pic == 0 => {
                let (Some(width), Some(height)) = (stream.width, stream.height) else {
                    continue;
                };
                let frame_rate = [&stream.avg_frame_rate, &stream.r_frame_rate]
                    .into_iter()
                    .flatten()
                    .find_map(|r| valid_rate(r));
                info.video = Some(VideoStream {
                    codec,
                    width,
                    height,
                    fps: frame_rate.as_deref().and_then(rate_value),
                    frame_rate,
                    pix_fmt: stream.pix_fmt,
                });
            }
            Some("audio") => info.audio.push(AudioStream {
                codec,
                sample_rate: stream.sample_rate.and_then(|r| r.parse().ok()),
                channels: stream.channels,
            }),
            Some("subtitle") => info.subtitle_count += 1,
            _ => {}
        }
    }
    Ok(info)
}

/// Seconds such as `12.345678` as milliseconds, within the accepted range.
fn parse_seconds(text: &str) -> Option<u64> {
    let seconds: f64 = text.trim().parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let ms = (seconds * 1000.0).round() as u64;
    (ms <= MAX_DURATION_MS).then_some(ms)
}

/// A frame rate `num/den` with both parts small positive integers; anything else is
/// refused, because the value goes into filter graphs.
fn valid_rate(rate: &str) -> Option<String> {
    let (num, den) = rate.split_once('/')?;
    let num: u32 = num.parse().ok()?;
    let den: u32 = den.parse().ok()?;
    if num == 0 || den == 0 || num > 1_000_000 || den > 1_000_000 {
        return None;
    }
    let fps = f64::from(num) / f64::from(den);
    (0.5..=1000.0)
        .contains(&fps)
        .then(|| format!("{num}/{den}"))
}

fn rate_value(rate: &str) -> Option<f64> {
    let (num, den) = rate.split_once('/')?;
    let fps = num.parse::<f64>().ok()? / den.parse::<f64>().ok()?;
    Some((fps * 100.0).round() / 100.0)
}

// ---------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------

/// `file:` URL of a path, so FFmpeg neither parses it as an option nor as a protocol.
pub fn file_url(path: &Path) -> OsString {
    let mut url = OsString::from("file:");
    url.push(path.as_os_str());
    url
}

/// An FFmpeg command with the options every operation uses: quiet, no stdin, progress on
/// stdout. Output files are always new files inside the job's temporary folder.
pub fn command(tools: &ToolBox) -> AppResult<Command> {
    command_with_log(tools, "error")
}

/// Like [`command`], with another log level (the loudness analysis prints at `info`).
pub fn command_with_log(tools: &ToolBox, level: &str) -> AppResult<Command> {
    let mut cmd = tools.command(ToolId::Ffmpeg)?;
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", level])
        .args(["-progress", "pipe:1", "-y"]);
    Ok(cmd)
}

/// Adds an input: options that apply to it go before, then the whitelisted `file:` URL.
pub fn add_input(cmd: &mut Command, path: &Path) {
    cmd.args(["-protocol_whitelist", "file", "-i"])
        .arg(file_url(path));
}

/// Runs FFmpeg, reporting progress in `span` (a slice of 0..=100) when `total` is known.
pub async fn run(
    cmd: Command,
    ctx: &JobContext,
    total: Option<Duration>,
    span: (f32, f32),
) -> AppResult<ProcessOutput> {
    if total.is_none() {
        ctx.set_indeterminate();
    }
    run_streaming(cmd, |line| {
        if let (Some(total), Some(ProgressUpdate::Time(done))) = (total, parse_progress_line(line))
        {
            if let Some(percent) = progress_percent(done, total) {
                ctx.set_progress(span.0 + percent / 100.0 * (span.1 - span.0));
            }
        }
    })
    .await
    .map_err(map_error)
}

/// Turns FFmpeg's messages into app errors.
fn map_error(err: AppError) -> AppError {
    let AppError::ProcessFailed(message) = &err else {
        return err;
    };
    let lower = message.to_lowercase();
    if lower.contains("invalid data found when processing input")
        || lower.contains("moov atom not found")
        || lower.contains("does not contain any stream")
        || lower.contains("matches no streams")
    {
        AppError::InvalidFile(last_line(message))
    } else if lower.contains("unknown encoder") || lower.contains("encoder not found") {
        AppError::ToolMissing(format!("FFmpeg encoder ({})", last_line(message)))
    } else {
        err
    }
}

fn last_line(message: &str) -> String {
    message
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(message)
        .trim()
        .to_owned()
}

/// The first of `candidates` that this FFmpeg has. With an unknown encoder list the first
/// candidate is used, and FFmpeg reports it if it is missing.
pub fn pick_encoder(tools: &ToolBox, candidates: &[&'static str]) -> Option<&'static str> {
    match tools.ffmpeg_encoders() {
        None => candidates.first().copied(),
        Some(available) => candidates
            .iter()
            .copied()
            .find(|c| available.iter().any(|a| a == c)),
    }
}

/// Time for FFmpeg's `-ss`, `-t` and `-to`: seconds with millisecond precision.
pub fn seconds_arg(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

// ---------------------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------------------

/// One meaningful line of `ffmpeg -progress pipe:1` output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProgressUpdate {
    /// Position already written to the output.
    Time(Duration),
    /// FFmpeg finished writing.
    End,
}

/// Parses a `key=value` line from `-progress pipe:1`.
///
/// Both `out_time_us` and `out_time_ms` are in microseconds (the latter is misnamed in FFmpeg).
pub fn parse_progress_line(line: &str) -> Option<ProgressUpdate> {
    let (key, value) = line.trim().split_once('=')?;
    match key {
        "out_time_us" | "out_time_ms" => value
            .trim()
            .parse::<u64>()
            .ok()
            .map(|us| ProgressUpdate::Time(Duration::from_micros(us))),
        "progress" if value.trim() == "end" => Some(ProgressUpdate::End),
        _ => None,
    }
}

/// Percentage of `total` covered by `elapsed`, clamped to 0..=100.
pub fn progress_percent(elapsed: Duration, total: Duration) -> Option<f32> {
    if total.is_zero() {
        return None;
    }
    let ratio = elapsed.as_secs_f64() / total.as_secs_f64();
    Some((ratio * 100.0).clamp(0.0, 100.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_lines() {
        assert_eq!(
            parse_progress_line("out_time_us=1500000"),
            Some(ProgressUpdate::Time(Duration::from_millis(1500)))
        );
        assert_eq!(
            parse_progress_line("out_time_ms=2000000\n"),
            Some(ProgressUpdate::Time(Duration::from_secs(2)))
        );
        assert_eq!(
            parse_progress_line("progress=end"),
            Some(ProgressUpdate::End)
        );
        assert_eq!(parse_progress_line("progress=continue"), None);
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
        assert_eq!(parse_progress_line("frame=42"), None);
        assert_eq!(parse_progress_line("garbage"), None);
    }

    #[test]
    fn computes_percent() {
        let total = Duration::from_secs(10);
        assert_eq!(progress_percent(Duration::from_secs(5), total), Some(50.0));
        assert_eq!(
            progress_percent(Duration::from_secs(20), total),
            Some(100.0)
        );
        assert_eq!(
            progress_percent(Duration::from_secs(1), Duration::ZERO),
            None
        );
    }

    #[test]
    fn parses_ffprobe_json() {
        let json = r#"{
            "streams": [
                {"codec_type": "video", "codec_name": "mjpeg", "width": 600, "height": 600,
                 "disposition": {"attached_pic": 1}},
                {"codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
                 "pix_fmt": "yuv420p", "avg_frame_rate": "30000/1001", "r_frame_rate": "30000/1001",
                 "disposition": {"attached_pic": 0}},
                {"codec_type": "audio", "codec_name": "aac", "sample_rate": "48000", "channels": 2},
                {"codec_type": "audio", "codec_name": "opus", "sample_rate": "x"},
                {"codec_type": "subtitle", "codec_name": "subrip"}
            ],
            "format": {"duration": "12.3456"}
        }"#;
        let info = parse_probe(json).unwrap();
        assert_eq!(info.duration_ms, Some(12_346));
        let video = info.video.unwrap();
        assert_eq!(
            (video.codec.as_str(), video.width, video.height),
            ("h264", 1920, 1080)
        );
        assert_eq!(video.frame_rate.as_deref(), Some("30000/1001"));
        assert_eq!(video.fps, Some(29.97));
        assert_eq!(info.audio.len(), 2);
        assert_eq!(info.audio[0].sample_rate, Some(48_000));
        assert_eq!(info.audio[1].sample_rate, None);
        assert_eq!(info.subtitle_count, 1);
    }

    #[test]
    fn refuses_odd_values_from_ffprobe() {
        let json = r#"{"streams": [{"codec_type": "video", "codec_name": "h264",
            "width": 10, "height": 10, "avg_frame_rate": "0/0",
            "r_frame_rate": "25/1;[x]"}], "format": {"duration": "-5"}}"#;
        let info = parse_probe(json).unwrap();
        assert_eq!(info.duration_ms, None);
        assert_eq!(info.video.unwrap().frame_rate, None);
        assert!(parse_probe("not json").is_err());
        assert_eq!(parse_seconds("1e300"), None);
        assert_eq!(parse_seconds("NaN"), None);
    }

    #[test]
    fn builds_safe_arguments() {
        assert_eq!(
            file_url(Path::new("/tmp/-i x.mp4")),
            OsString::from("file:/tmp/-i x.mp4")
        );
        assert_eq!(seconds_arg(0), "0.000");
        assert_eq!(seconds_arg(61_005), "61.005");
    }

    #[test]
    fn picks_the_first_available_encoder() {
        use crate::tools::{ToolId, ToolStatus};
        let status = |encoders: Option<Vec<String>>| ToolStatus {
            id: ToolId::Ffmpeg,
            name: "FFmpeg".into(),
            required: true,
            available: true,
            path: Some("/usr/bin/ffmpeg".into()),
            source: None,
            version: None,
            error: None,
            encoders,
            languages: None,
            runtime_dir: None,
        };
        let fedora = ToolBox::new(
            vec![status(Some(vec!["libopenh264".into(), "aac".into()]))],
            None,
        );
        assert_eq!(
            pick_encoder(&fedora, &["libx264", "libopenh264", "mpeg4"]),
            Some("libopenh264")
        );
        assert_eq!(pick_encoder(&fedora, &["libvpx-vp9"]), None);
        let unknown = ToolBox::new(vec![status(None)], None);
        assert_eq!(pick_encoder(&unknown, &["libx264"]), Some("libx264"));
    }

    #[test]
    fn maps_ffmpeg_errors() {
        let err = map_error(AppError::ProcessFailed(
            "exit 1: x.mp4: Invalid data found when processing input".into(),
        ));
        assert!(matches!(err, AppError::InvalidFile(_)));
        let err = map_error(AppError::ProcessFailed("Unknown encoder 'libx264'".into()));
        assert!(matches!(err, AppError::ToolMissing(_)));
    }
}
