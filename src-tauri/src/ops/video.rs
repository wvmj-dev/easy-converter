//! Video operations (phase 3), all with FFmpeg.
//!
//! Each job reads the file with ffprobe first, then copies every track the output
//! container can hold as it is (`-c copy`) and encodes only the rest. Encoders are chosen
//! from the ones this FFmpeg has, so builds without x264 (Fedora) still work.

use std::path::PathBuf;

use serde::Deserialize;
use ts_rs::TS;

use super::media::{
    self, require_audio, require_ffmpeg, require_video, AudioFormat, MediaQuality, TimeRange,
    VideoContainer,
};
use super::{process_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::jobs::{JobCategory, JobSpec, PlannedOutput};
use crate::tools::ffmpeg::{self, MediaInfo};
use crate::tools::ToolBox;

/// Most videos in one merge.
pub const MAX_MERGE_INPUTS: usize = 50;
pub const MIN_HEIGHT: u32 = 16;
pub const MAX_HEIGHT: u32 = 4320;
pub const MAX_GIF_FPS: u32 = 30;
pub const MIN_GIF_WIDTH: u32 = 32;
pub const MAX_GIF_WIDTH: u32 = 1280;
/// Longest part turned into a GIF: GIFs grow very fast with length.
pub const MAX_GIF_MS: u64 = 120_000;

// ---------------------------------------------------------------------------------------
// Request parameters
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VideoConvertParams {
    pub input: PathBuf,
    pub container: VideoContainer,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VideoTrimParams {
    pub input: PathBuf,
    pub range: TimeRange,
    /// Re-encode so the cut is exact to the frame. Otherwise tracks are copied and the cut
    /// starts at the nearest key frame (fast, no quality loss).
    #[serde(default)]
    pub accurate: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VideoCompressParams {
    pub input: PathBuf,
    pub quality: MediaQuality,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VideoResizeParams {
    pub input: PathBuf,
    /// New height in pixels; the width follows the aspect ratio.
    pub height: u32,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VideoExtractAudioParams {
    pub input: PathBuf,
    /// Output format; when absent the track is saved as it is, in a fitting format.
    #[serde(default)]
    #[ts(optional)]
    pub format: Option<AudioFormat>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VideoToGifParams {
    pub input: PathBuf,
    pub fps: u32,
    /// Maximum width in pixels (never enlarged).
    pub width: u32,
    /// Part of the video; the whole video when absent.
    #[serde(default)]
    #[ts(optional)]
    pub range: Option<TimeRange>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VideoMergeParams {
    pub inputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "camelCase")]
#[ts(export)]
pub enum AudioTrackAction {
    /// Removes every audio track.
    Remove,
    /// Replaces the audio with the first audio track of another file.
    Replace { audio: PathBuf },
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VideoAudioTrackParams {
    pub input: PathBuf,
    pub action: AudioTrackAction,
}

// ---------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------

fn validate_video(path: &std::path::Path) -> AppResult<PathBuf> {
    media::validate_input(path, JobCategory::Video)
}

/// Subtitle tracks are kept when the output container is the input's own (copying them
/// works then); otherwise they are dropped.
fn subtitle_args(
    cmd: &mut tokio::process::Command,
    container: VideoContainer,
    input: &std::path::Path,
    info: &MediaInfo,
) {
    if info.subtitle_count > 0 && container.is_native(input) {
        cmd.args(["-map", "0:s?", "-c:s", "copy"]);
    } else {
        cmd.arg("-sn");
    }
}

/// The main video track and every audio track of input 0.
const MAP_VIDEO_AND_AUDIO: [&str; 4] = ["-map", "0:V:0", "-map", "0:a?"];

fn even(n: u32) -> u32 {
    n.max(2) & !1
}

// ---------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------

pub fn convert(
    params: VideoConvertParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_video(&params.input)?;
    let container = params.container;
    let destination = planner.destination(&input, container.extension())?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "videoConvert",
        JobCategory::Video,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let video = require_video(&info)?;
            let temp = ctx
                .temp_dir
                .join(format!("output.{}", container.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            ffmpeg::add_input(&mut cmd, &work_input);
            cmd.args(MAP_VIDEO_AND_AUDIO)
                .args(["-map_metadata", "0", "-dn"]);
            subtitle_args(&mut cmd, container, &work_input, &info);
            cmd.args(container.video_copy_or_encode(&tools, MediaQuality::High, video)?);
            cmd.args(container.audio_copy_or_encode(&tools, MediaQuality::High, &info)?);
            container.output_args(&mut cmd);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, info.duration(), (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn trim(
    params: VideoTrimParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_video(&params.input)?;
    let range = params.range.validate()?;
    let container = VideoContainer::for_input(&input);
    let destination = planner.destination(&input, container.extension())?;
    let accurate = params.accurate;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "videoTrim",
        JobCategory::Video,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let video = require_video(&info)?;
            let length = range.resolve(info.duration_ms)?;
            let temp = ctx
                .temp_dir
                .join(format!("output.{}", container.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            range.seek_args(&mut cmd);
            ffmpeg::add_input(&mut cmd, &work_input);
            TimeRange::length_args(&mut cmd, length, range.end_ms.is_some());
            cmd.args(MAP_VIDEO_AND_AUDIO)
                .args(["-map_metadata", "0", "-dn"]);
            subtitle_args(&mut cmd, container, &work_input, &info);
            if accurate {
                cmd.args(container.video_codec_args(&tools, MediaQuality::High, video, None)?);
                if !info.audio.is_empty() {
                    cmd.args(container.audio_codec_args(&tools, MediaQuality::High)?);
                }
            } else {
                cmd.args(container.video_copy_or_encode(&tools, MediaQuality::High, video)?);
                cmd.args(container.audio_copy_or_encode(&tools, MediaQuality::High, &info)?);
                cmd.args(["-avoid_negative_ts", "make_zero"]);
            }
            container.output_args(&mut cmd);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, media::ms(length), (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn compress(
    params: VideoCompressParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_video(&params.input)?;
    let container = VideoContainer::for_input(&input);
    let destination = planner.destination(&input, container.extension())?;
    let quality = params.quality;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "videoCompress",
        JobCategory::Video,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let video = require_video(&info)?;
            let temp = ctx
                .temp_dir
                .join(format!("output.{}", container.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            ffmpeg::add_input(&mut cmd, &work_input);
            cmd.args(MAP_VIDEO_AND_AUDIO)
                .args(["-map_metadata", "0", "-dn"]);
            subtitle_args(&mut cmd, container, &work_input, &info);
            cmd.args(container.video_codec_args(&tools, quality, video, None)?);
            if !info.audio.is_empty() {
                cmd.args(container.audio_codec_args(&tools, quality)?);
            }
            container.output_args(&mut cmd);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, info.duration(), (0.0, 100.0)).await?;
            // A "compressed" file larger than the original helps nobody.
            if container.is_native(&work_input)
                && std::fs::metadata(&temp)?.len() >= std::fs::metadata(&work_input)?.len()
            {
                std::fs::copy(&work_input, &temp)?;
            }
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn resize(
    params: VideoResizeParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_video(&params.input)?;
    if !(MIN_HEIGHT..=MAX_HEIGHT).contains(&params.height) || !params.height.is_multiple_of(2) {
        return Err(AppError::InvalidParameter(format!(
            "height must be an even number between {MIN_HEIGHT} and {MAX_HEIGHT}"
        )));
    }
    let height = params.height;
    let container = VideoContainer::for_input(&input);
    let destination = planner.destination(&input, container.extension())?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "videoResize",
        JobCategory::Video,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let video = require_video(&info)?;
            let width = even(
                (u64::from(video.width) * u64::from(height) / u64::from(video.height.max(1)))
                    .min(u64::from(u32::MAX)) as u32,
            );
            let temp = ctx
                .temp_dir
                .join(format!("output.{}", container.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            ffmpeg::add_input(&mut cmd, &work_input);
            cmd.args(MAP_VIDEO_AND_AUDIO)
                .args(["-map_metadata", "0", "-dn"]);
            subtitle_args(&mut cmd, container, &work_input, &info);
            // -2 keeps the aspect ratio with an even width, as 4:2:0 video needs.
            cmd.arg("-vf")
                .arg(format!("scale=-2:{height}:flags=lanczos"));
            cmd.args(container.video_codec_args(
                &tools,
                MediaQuality::High,
                video,
                Some((width, height)),
            )?);
            cmd.args(container.audio_copy_or_encode(&tools, MediaQuality::High, &info)?);
            container.output_args(&mut cmd);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, info.duration(), (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn extract_audio(
    params: VideoExtractAudioParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_video(&params.input)?;
    // The name is checked now with a placeholder extension, so a bad suffix fails early.
    planner.destination(&input, "m4a")?;
    let requested = params.format;
    let planner = planner.clone();
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "videoExtractAudio",
        JobCategory::Video,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let track = require_audio(&info)?;
            let format = requested.unwrap_or_else(|| AudioFormat::for_codec(&track.codec));
            let destination = planner.destination(&work_input, format.extension())?;
            let temp = ctx.temp_dir.join(format!("output.{}", format.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            ffmpeg::add_input(&mut cmd, &work_input);
            cmd.args(["-map", "0:a:0", "-vn", "-sn", "-dn", "-map_metadata", "0"]);
            cmd.args(format.copy_or_encode(&tools, &track.codec, None)?);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, info.duration(), (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn to_gif(
    params: VideoToGifParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_video(&params.input)?;
    if !(1..=MAX_GIF_FPS).contains(&params.fps) {
        return Err(AppError::InvalidParameter(format!(
            "frame rate must be between 1 and {MAX_GIF_FPS}"
        )));
    }
    if !(MIN_GIF_WIDTH..=MAX_GIF_WIDTH).contains(&params.width) {
        return Err(AppError::InvalidParameter(format!(
            "width must be between {MIN_GIF_WIDTH} and {MAX_GIF_WIDTH} pixels"
        )));
    }
    let range = params.range.unwrap_or(TimeRange::ALL).validate()?;
    let destination = planner.destination(&input, "gif")?;
    let (fps, max_width) = (params.fps, params.width);
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "videoToGif",
        JobCategory::Video,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let video = require_video(&info)?;
            let length = range.resolve(info.duration_ms)?.ok_or_else(|| {
                AppError::InvalidParameter(
                    "the length of the video is unknown; choose an end time".into(),
                )
            })?;
            if length > MAX_GIF_MS {
                return Err(AppError::TooLarge(format!(
                    "a GIF can be at most {} seconds long",
                    MAX_GIF_MS / 1000
                )));
            }
            let width = max_width.min(video.width);
            let temp = ctx.temp_dir.join("output.gif");
            let mut cmd = ffmpeg::command(&tools)?;
            range.seek_args(&mut cmd);
            ffmpeg::add_input(&mut cmd, &work_input);
            cmd.arg("-t").arg(ffmpeg::seconds_arg(length));
            cmd.args(["-map", "0:V:0", "-an", "-sn", "-dn"]);
            // One palette made for this clip gives far better colours than the default one.
            cmd.arg("-vf").arg(format!(
                "fps={fps},scale={width}:-1:flags=lanczos,split[a][b];\
                 [a]palettegen=stats_mode=diff[p];\
                 [b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle"
            ));
            cmd.args(["-c:v", "gif", "-loop", "0"]);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, media::ms(Some(length)), (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn merge(
    params: VideoMergeParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    if !(2..=MAX_MERGE_INPUTS).contains(&params.inputs.len()) {
        return Err(AppError::InvalidParameter(format!(
            "between 2 and {MAX_MERGE_INPUTS} videos can be joined"
        )));
    }
    let inputs = params
        .inputs
        .iter()
        .map(|p| validate_video(p))
        .collect::<AppResult<Vec<_>>>()?;
    let container = VideoContainer::for_input(&inputs[0]);
    let destination = planner.destination(&inputs[0], container.extension())?;
    let tools = tools.clone();
    let work_inputs = inputs.clone();
    Ok(process_job(
        "videoMerge",
        JobCategory::Video,
        inputs,
        move |ctx| async move {
            ctx.set_indeterminate();
            let mut infos = Vec::with_capacity(work_inputs.len());
            for input in &work_inputs {
                ctx.check_cancelled()?;
                let info = ffmpeg::probe(&tools, input).await?;
                require_video(&info)?;
                infos.push(info);
            }
            let total: Option<u64> = infos.iter().map(|i| i.duration_ms).sum();
            let temp = ctx
                .temp_dir
                .join(format!("output.{}", container.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            if can_concat_copy(container, &infos) {
                let list = ctx.temp_dir.join("list.ffconcat");
                std::fs::write(&list, concat_list(&work_inputs)?)?;
                cmd.args(["-f", "concat", "-safe", "0"]);
                ffmpeg::add_input(&mut cmd, &list);
                cmd.args([
                    "-map", "0:V:0", "-map", "0:a:0?", "-c", "copy", "-sn", "-dn",
                ]);
            } else {
                for input in &work_inputs {
                    ffmpeg::add_input(&mut cmd, input);
                }
                let first = require_video(&infos[0])?;
                let size = (even(first.width), even(first.height));
                let rate = first.frame_rate.clone().unwrap_or_else(|| "30".into());
                let with_audio = infos.iter().any(|i| !i.audio.is_empty());
                cmd.arg("-filter_complex")
                    .arg(concat_filter(&infos, size, &rate, with_audio)?);
                cmd.args(["-map", "[v]"]);
                cmd.args(container.video_codec_args(
                    &tools,
                    MediaQuality::High,
                    first,
                    Some(size),
                )?);
                if with_audio {
                    cmd.args(["-map", "[a]"]);
                    cmd.args(container.audio_codec_args(&tools, MediaQuality::High)?);
                }
            }
            container.output_args(&mut cmd);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, media::ms(total), (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

/// Videos can be joined without re-encoding when their main tracks match exactly and the
/// container holds them.
fn can_concat_copy(container: VideoContainer, infos: &[MediaInfo]) -> bool {
    let Some(first) = infos.first() else {
        return false;
    };
    let Some(v0) = &first.video else {
        return false;
    };
    let a0 = first.audio.first();
    container.copies_video(&v0.codec)
        && a0.is_none_or(|a| container.copies_audio(&a.codec))
        && infos.iter().all(|info| {
            let video_same = info.video.as_ref().is_some_and(|v| {
                v.codec == v0.codec
                    && v.width == v0.width
                    && v.height == v0.height
                    && v.pix_fmt == v0.pix_fmt
                    && v.frame_rate == v0.frame_rate
            });
            let audio_same = match (info.audio.first(), a0) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.codec == b.codec && a.sample_rate == b.sample_rate && a.channels == b.channels
                }
                _ => false,
            };
            video_same && audio_same
        })
}

/// An `ffconcat` list. Paths are quoted, with `'` escaped; a line break in a name would
/// add a directive to the list, so such names are refused.
fn concat_list(inputs: &[PathBuf]) -> AppResult<String> {
    let mut list = String::from("ffconcat version 1.0\n");
    for input in inputs {
        let path = input.to_str().ok_or_else(|| {
            AppError::InvalidPath(format!("{} is not valid UTF-8", input.display()))
        })?;
        if path.contains(['\n', '\r']) {
            return Err(AppError::InvalidPath(format!(
                "file names with line breaks cannot be joined: {}",
                input.display()
            )));
        }
        list.push_str("file 'file:");
        list.push_str(&path.replace('\'', "'\\''"));
        list.push_str("'\n");
    }
    Ok(list)
}

/// Filter graph that brings every video to the first one's size and frame rate (with
/// black bars when the shape differs) and the audio to 48 kHz stereo, then joins them.
/// Videos without audio get silence of their length.
fn concat_filter(
    infos: &[MediaInfo],
    (w, h): (u32, u32),
    rate: &str,
    with_audio: bool,
) -> AppResult<String> {
    let mut graph = String::new();
    let mut pads = String::new();
    for (i, info) in infos.iter().enumerate() {
        graph.push_str(&format!(
            "[{i}:V:0]scale={w}:{h}:force_original_aspect_ratio=decrease,\
             pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={rate},format=yuv420p[v{i}];"
        ));
        pads.push_str(&format!("[v{i}]"));
        if with_audio {
            if info.audio.is_empty() {
                let length = info.duration_ms.ok_or_else(|| {
                    AppError::InvalidFile("a video without audio has an unknown length".into())
                })?;
                graph.push_str(&format!(
                    "anullsrc=r=48000:cl=stereo,atrim=duration={}[a{i}];",
                    ffmpeg::seconds_arg(length)
                ));
            } else {
                graph.push_str(&format!(
                    "[{i}:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}];"
                ));
            }
            pads.push_str(&format!("[a{i}]"));
        }
    }
    let n = infos.len();
    let a = u8::from(with_audio);
    graph.push_str(&format!("{pads}concat=n={n}:v=1:a={a}[v]"));
    if with_audio {
        graph.push_str("[a]");
    }
    Ok(graph)
}

pub fn audio_track(
    params: VideoAudioTrackParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_video(&params.input)?;
    let replacement = match &params.action {
        AudioTrackAction::Remove => None,
        AudioTrackAction::Replace { audio } => Some(media::validate_video_or_audio(audio)?),
    };
    let container = VideoContainer::for_input(&input);
    let destination = planner.destination(&input, container.extension())?;
    let tools = tools.clone();
    let work_input = input.clone();
    let mut inputs = vec![input];
    inputs.extend(replacement.clone());
    Ok(process_job(
        "videoAudioTrack",
        JobCategory::Video,
        inputs,
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let video = require_video(&info)?;
            let temp = ctx
                .temp_dir
                .join(format!("output.{}", container.extension()));
            // Every input goes before the output options.
            let replacement_codec = match &replacement {
                Some(audio_file) => {
                    let audio_info = ffmpeg::probe(&tools, audio_file).await?;
                    Some(require_audio(&audio_info)?.codec.clone())
                }
                None => None,
            };
            let mut cmd = ffmpeg::command(&tools)?;
            ffmpeg::add_input(&mut cmd, &work_input);
            if let Some(audio_file) = &replacement {
                ffmpeg::add_input(&mut cmd, audio_file);
            }
            cmd.args(["-map", "0:V:0", "-map_metadata", "0", "-dn"]);
            subtitle_args(&mut cmd, container, &work_input, &info);
            cmd.args(container.video_copy_or_encode(&tools, MediaQuality::High, video)?);
            match replacement_codec {
                None => {
                    cmd.arg("-an");
                }
                Some(codec) => {
                    cmd.args(["-map", "1:a:0"]);
                    if container.copies_audio(&codec) {
                        cmd.args(["-c:a", "copy"]);
                    } else {
                        cmd.args(container.audio_codec_args(&tools, MediaQuality::High)?);
                    }
                    // The video keeps its length: longer audio is cut, shorter ends early.
                    if let Some(length) = info.duration_ms {
                        cmd.arg("-t").arg(ffmpeg::seconds_arg(length));
                    }
                }
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
    use crate::tools::ffmpeg::{AudioStream, VideoStream};

    fn info(codec: &str, w: u32, audio: Option<&str>, ms: Option<u64>) -> MediaInfo {
        MediaInfo {
            duration_ms: ms,
            video: Some(VideoStream {
                codec: codec.into(),
                width: w,
                height: 240,
                fps: Some(25.0),
                frame_rate: Some("25/1".into()),
                pix_fmt: Some("yuv420p".into()),
            }),
            audio: audio
                .map(|c| AudioStream {
                    codec: c.into(),
                    sample_rate: Some(48_000),
                    channels: Some(2),
                })
                .into_iter()
                .collect(),
            subtitle_count: 0,
        }
    }

    #[test]
    fn copies_only_matching_videos() {
        let a = info("h264", 320, Some("aac"), Some(1000));
        assert!(can_concat_copy(
            VideoContainer::Mp4,
            &[a.clone(), a.clone()]
        ));
        assert!(!can_concat_copy(
            VideoContainer::Webm,
            &[a.clone(), a.clone()]
        ));
        let wider = info("h264", 640, Some("aac"), Some(1000));
        assert!(!can_concat_copy(VideoContainer::Mp4, &[a.clone(), wider]));
        let silent = info("h264", 320, None, Some(1000));
        assert!(!can_concat_copy(VideoContainer::Mp4, &[a, silent]));
    }

    #[test]
    fn concat_list_quotes_paths() {
        let list = concat_list(&[PathBuf::from("/v/it's.mp4")]).unwrap();
        assert_eq!(list, "ffconcat version 1.0\nfile 'file:/v/it'\\''s.mp4'\n");
        assert!(concat_list(&[PathBuf::from("/v/a\nfile x.mp4")]).is_err());
    }

    #[test]
    fn concat_filter_adds_silence_where_needed() {
        let graph = concat_filter(
            &[
                info("h264", 320, Some("aac"), Some(1000)),
                info("vp9", 640, None, Some(2500)),
            ],
            (320, 240),
            "25/1",
            true,
        )
        .unwrap();
        assert!(graph.contains("anullsrc=r=48000:cl=stereo,atrim=duration=2.500[a1]"));
        assert!(graph.ends_with("[v0][a0][v1][a1]concat=n=2:v=1:a=1[v][a]"));
        let silent_unknown = info("h264", 320, None, None);
        assert!(concat_filter(&[silent_unknown], (320, 240), "25/1", true).is_err());
    }

    #[test]
    fn even_sizes() {
        assert_eq!(even(427), 426);
        assert_eq!(even(1), 2);
    }
}
