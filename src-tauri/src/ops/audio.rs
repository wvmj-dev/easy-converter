//! Audio operations (phase 3), all with FFmpeg. Only the first audio track is used.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use ts_rs::TS;

use super::media::{self, require_audio, require_ffmpeg, AudioFormat, MediaQuality, TimeRange};
use super::{process_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::jobs::{JobCategory, JobSpec, PlannedOutput};
use crate::tools::ffmpeg;
use crate::tools::ToolBox;

/// Loudness target of the normalization (EBU R128 style, as used by streaming services).
const LOUDNORM: &str = "loudnorm=I=-16:TP=-1.5:LRA=11";
/// Only the first audio track, without cover pictures, subtitles or data.
const MAP_FIRST_AUDIO: [&str; 6] = ["-map", "0:a:0", "-vn", "-sn", "-dn", "-map_metadata"];

// ---------------------------------------------------------------------------------------
// Request parameters
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AudioConvertParams {
    pub input: PathBuf,
    pub format: AudioFormat,
    /// Bitrate for lossy formats, in kbit/s. When absent the track is copied if the format
    /// holds it, otherwise encoded at 192 kbit/s.
    #[serde(default)]
    #[ts(optional)]
    pub bitrate_kbps: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AudioTrimParams {
    pub input: PathBuf,
    pub range: TimeRange,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AudioNormalizeParams {
    pub input: PathBuf,
}

/// Mono, 16 kHz, 16-bit WAV: the input speech recognition tools expect.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AudioMonoParams {
    pub input: PathBuf,
}

fn validate_audio(path: &Path) -> AppResult<PathBuf> {
    media::validate_input(path, JobCategory::Audio)
}

// ---------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------

pub fn convert(
    params: AudioConvertParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_audio(&params.input)?;
    let kbps = media::validate_kbps(params.bitrate_kbps)?;
    let format = params.format;
    let destination = planner.destination(&input, format.extension())?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "audioConvert",
        JobCategory::Audio,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let track = require_audio(&info)?;
            let temp = ctx.temp_dir.join(format!("output.{}", format.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            ffmpeg::add_input(&mut cmd, &work_input);
            cmd.args(MAP_FIRST_AUDIO).arg("0");
            cmd.args(format.copy_or_encode(&tools, &track.codec, kbps)?);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, info.duration(), (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn trim(
    params: AudioTrimParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_audio(&params.input)?;
    let range = params.range.validate()?;
    let format = AudioFormat::for_input(&input);
    let destination = planner.destination(&input, format.extension())?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "audioTrim",
        JobCategory::Audio,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let track = require_audio(&info)?;
            let length = range.resolve(info.duration_ms)?;
            let temp = ctx.temp_dir.join(format!("output.{}", format.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            range.seek_args(&mut cmd);
            ffmpeg::add_input(&mut cmd, &work_input);
            TimeRange::length_args(&mut cmd, length, range.end_ms.is_some());
            cmd.args(MAP_FIRST_AUDIO).arg("0");
            // Audio frames are a few milliseconds long, so copying cuts precisely enough
            // and loses nothing.
            cmd.args(format.copy_or_encode(&tools, &track.codec, None)?);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, media::ms(length), (0.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

pub fn normalize(
    params: AudioNormalizeParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = validate_audio(&params.input)?;
    let format = AudioFormat::for_input(&input);
    let destination = planner.destination(&input, format.extension())?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "audioNormalize",
        JobCategory::Audio,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            let track = require_audio(&info)?;
            // Pass 1 measures the loudness; pass 2 applies one linear gain, which keeps
            // the dynamics instead of pumping the volume up and down.
            let mut cmd = ffmpeg::command_with_log(&tools, "info")?;
            ffmpeg::add_input(&mut cmd, &work_input);
            cmd.args(["-map", "0:a:0", "-vn", "-sn", "-dn", "-af"])
                .arg(format!("{LOUDNORM}:print_format=json"))
                .args(["-f", "null", "-"]);
            let analysis = ffmpeg::run(cmd, &ctx, info.duration(), (0.0, 50.0)).await?;
            let measured = parse_loudnorm(&analysis.stderr)?;

            let temp = ctx.temp_dir.join(format!("output.{}", format.extension()));
            let mut cmd = ffmpeg::command(&tools)?;
            ffmpeg::add_input(&mut cmd, &work_input);
            cmd.args(MAP_FIRST_AUDIO).arg("0");
            cmd.arg("-af")
                .arg(format!("{LOUDNORM}:{measured}:linear=true"));
            // loudnorm works at 192 kHz; go back to the original rate.
            let rate = track
                .sample_rate
                .filter(|r| (8_000..=192_000).contains(r))
                .unwrap_or(48_000);
            cmd.arg("-ar").arg(rate.to_string());
            cmd.args(format.codec_args(&tools, MediaQuality::High.audio_kbps())?);
            cmd.arg(ffmpeg::file_url(&temp));
            ffmpeg::run(cmd, &ctx, info.duration(), (50.0, 100.0)).await?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

/// Reads the measurements printed by the first loudnorm pass and returns them as filter
/// options. Every value is checked to be a finite number before going into the filter.
fn parse_loudnorm(stderr: &str) -> AppResult<String> {
    let start = stderr
        .rfind("\"input_i\"")
        .and_then(|i| stderr[..i].rfind('{'))
        .ok_or_else(|| AppError::ProcessFailed("loudness analysis gave no result".into()))?;
    let end = stderr[start..]
        .find('}')
        .map(|i| start + i + 1)
        .ok_or_else(|| AppError::ProcessFailed("loudness analysis gave no result".into()))?;
    let values: serde_json::Value = serde_json::from_str(&stderr[start..end])
        .map_err(|err| AppError::ProcessFailed(format!("unreadable loudness analysis: {err}")))?;
    let number = |key: &str| -> AppResult<f64> {
        values
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                AppError::InvalidFile("the audio is silent or too short to normalize".into())
            })
    };
    Ok(format!(
        "measured_I={:.2}:measured_TP={:.2}:measured_LRA={:.2}:measured_thresh={:.2}:offset={:.2}",
        number("input_i")?,
        number("input_tp")?,
        number("input_lra")?,
        number("input_thresh")?,
        number("target_offset")?,
    ))
}

pub fn mono_16k(
    params: AudioMonoParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    require_ffmpeg(tools)?;
    let input = media::validate_video_or_audio(&params.input)?;
    let destination = planner.destination(&input, "wav")?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "audioMono16k",
        JobCategory::Audio,
        vec![input],
        move |ctx| async move {
            let info = ffmpeg::probe(&tools, &work_input).await?;
            require_audio(&info)?;
            let temp = ctx.temp_dir.join("output.wav");
            let mut cmd = ffmpeg::command(&tools)?;
            ffmpeg::add_input(&mut cmd, &work_input);
            cmd.args(MAP_FIRST_AUDIO).arg("-1");
            cmd.args(["-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"]);
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
    fn reads_loudnorm_measurements() {
        let stderr = "Stream mapping: ...\n[Parsed_loudnorm_0 @ 0x1] \n{\n\t\"input_i\" : \"-27.61\",\n\t\"input_tp\" : \"-4.47\",\n\t\"input_lra\" : \"18.06\",\n\t\"input_thresh\" : \"-39.20\",\n\t\"output_i\" : \"-16.58\",\n\t\"target_offset\" : \"0.58\"\n}\n";
        assert_eq!(
            parse_loudnorm(stderr).unwrap(),
            "measured_I=-27.61:measured_TP=-4.47:measured_LRA=18.06:measured_thresh=-39.20:offset=0.58"
        );
    }

    #[test]
    fn refuses_silent_or_odd_measurements() {
        let silent = "{\"input_i\" : \"-inf\", \"input_tp\" : \"-inf\", \"input_lra\" : \"0.00\", \"input_thresh\" : \"-70.00\", \"target_offset\" : \"inf\"}";
        assert!(matches!(
            parse_loudnorm(silent),
            Err(AppError::InvalidFile(_))
        ));
        let injected = "{\"input_i\" : \"-20:af=evil\", \"input_tp\" : \"0\", \"input_lra\" : \"0\", \"input_thresh\" : \"0\", \"target_offset\" : \"0\"}";
        assert!(parse_loudnorm(injected).is_err());
        assert!(parse_loudnorm("no json here").is_err());
    }
}
