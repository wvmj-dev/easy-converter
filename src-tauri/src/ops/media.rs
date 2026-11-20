//! Shared pieces of the video and audio operations: output formats, which codecs each
//! container can take as they are (so `-c copy` is used whenever possible), encoder
//! choice and quality presets, time ranges and input validation.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::fs_utils;
use crate::jobs::JobCategory;
use crate::tools::ffmpeg::{self, MediaInfo, VideoStream, MAX_DURATION_MS};
use crate::tools::{ToolBox, ToolId};

// ---------------------------------------------------------------------------------------
// Quality
// ---------------------------------------------------------------------------------------

/// Quality preset for encoding: higher quality means bigger files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MediaQuality {
    High,
    Medium,
    Low,
}

impl MediaQuality {
    fn pick<T>(self, high: T, medium: T, low: T) -> T {
        match self {
            MediaQuality::High => high,
            MediaQuality::Medium => medium,
            MediaQuality::Low => low,
        }
    }

    /// Audio bitrate used with this preset by lossy encoders, in kbit/s.
    pub fn audio_kbps(self) -> u32 {
        self.pick(192, 128, 96)
    }
}

pub const MIN_AUDIO_KBPS: u32 = 32;
pub const MAX_AUDIO_KBPS: u32 = 320;

pub fn validate_kbps(kbps: Option<u32>) -> AppResult<Option<u32>> {
    match kbps {
        Some(k) if !(MIN_AUDIO_KBPS..=MAX_AUDIO_KBPS).contains(&k) => {
            Err(AppError::InvalidParameter(format!(
                "audio bitrate must be between {MIN_AUDIO_KBPS} and {MAX_AUDIO_KBPS} kbit/s"
            )))
        }
        other => Ok(other),
    }
}

// ---------------------------------------------------------------------------------------
// Video containers
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum VideoContainer {
    Mp4,
    Mkv,
    Webm,
    Mov,
    Avi,
}

impl VideoContainer {
    pub fn extension(self) -> &'static str {
        match self {
            VideoContainer::Mp4 => "mp4",
            VideoContainer::Mkv => "mkv",
            VideoContainer::Webm => "webm",
            VideoContainer::Mov => "mov",
            VideoContainer::Avi => "avi",
        }
    }

    /// Container used to keep "the same format" as an input. Formats the app does not
    /// write (WMV, FLV, MPEG, TS, 3GP) become MP4.
    pub fn for_input(path: &Path) -> Self {
        match fs_utils::extension_of(path).as_deref() {
            Some("mkv") => VideoContainer::Mkv,
            Some("webm") => VideoContainer::Webm,
            Some("mov") => VideoContainer::Mov,
            Some("avi") => VideoContainer::Avi,
            _ => VideoContainer::Mp4,
        }
    }

    /// Whether the input already is in this container (its extra tracks can be kept).
    pub fn is_native(self, path: &Path) -> bool {
        let ext = fs_utils::extension_of(path);
        match self {
            VideoContainer::Mp4 => matches!(ext.as_deref(), Some("mp4" | "m4v")),
            other => ext.as_deref() == Some(other.extension()),
        }
    }

    /// Video encoders, in order of preference. Fedora's `ffmpeg-free` has no x264, so
    /// there are always fallbacks.
    fn video_encoders(self) -> &'static [&'static str] {
        match self {
            VideoContainer::Mp4 | VideoContainer::Mov => &["libx264", "libopenh264", "mpeg4"],
            VideoContainer::Mkv => &["libx264", "libopenh264", "libvpx-vp9", "mpeg4"],
            VideoContainer::Webm => &["libvpx-vp9", "libvpx"],
            VideoContainer::Avi => &["mpeg4", "libx264", "libopenh264"],
        }
    }

    fn audio_encoders(self) -> &'static [&'static str] {
        match self {
            VideoContainer::Mp4 | VideoContainer::Mov => &["aac"],
            VideoContainer::Mkv => &["aac", "libopus", "libvorbis"],
            VideoContainer::Webm => &["libopus", "libvorbis"],
            VideoContainer::Avi => &["libmp3lame", "ac3", "pcm_s16le"],
        }
    }

    /// Video codecs this container takes without re-encoding.
    pub fn copies_video(self, codec: &str) -> bool {
        let list: &[&str] = match self {
            VideoContainer::Mp4 => &["h264", "hevc", "mpeg4", "av1"],
            VideoContainer::Mov => &["h264", "hevc", "mpeg4", "av1", "prores", "mjpeg"],
            VideoContainer::Mkv => &[
                "h264",
                "hevc",
                "mpeg4",
                "vp8",
                "vp9",
                "av1",
                "mpeg1video",
                "mpeg2video",
                "theora",
                "mjpeg",
                "prores",
            ],
            VideoContainer::Webm => &["vp8", "vp9", "av1"],
            VideoContainer::Avi => &["mpeg4", "h264", "mjpeg", "msmpeg4v2", "msmpeg4v3"],
        };
        list.contains(&codec)
    }

    /// Audio codecs this container takes without re-encoding.
    pub fn copies_audio(self, codec: &str) -> bool {
        let list: &[&str] = match self {
            VideoContainer::Mp4 => &["aac", "mp3", "ac3", "eac3", "alac"],
            VideoContainer::Mov => &["aac", "mp3", "ac3", "eac3", "alac", "pcm_s16le"],
            VideoContainer::Mkv => &[
                "aac",
                "mp3",
                "mp2",
                "opus",
                "vorbis",
                "flac",
                "ac3",
                "eac3",
                "dts",
                "truehd",
                "alac",
                "pcm_s16le",
                "pcm_s24le",
            ],
            VideoContainer::Webm => &["opus", "vorbis"],
            VideoContainer::Avi => &["mp3", "mp2", "ac3", "pcm_s16le"],
        };
        list.contains(&codec)
    }

    /// Container-specific output options.
    pub fn output_args(self, cmd: &mut Command) {
        if matches!(self, VideoContainer::Mp4 | VideoContainer::Mov) {
            // Index at the start, so the file plays while it is still being read.
            cmd.args(["-movflags", "+faststart"]);
        }
    }

    /// Video encoding options for this container at `quality`.
    pub fn video_codec_args(
        self,
        tools: &ToolBox,
        quality: MediaQuality,
        video: &VideoStream,
        out_size: Option<(u32, u32)>,
    ) -> AppResult<Vec<String>> {
        let encoder = ffmpeg::pick_encoder(tools, self.video_encoders())
            .ok_or_else(|| missing_encoder(self.video_encoders()))?;
        let mut args: Vec<String> = vec!["-c:v".into(), encoder.into()];
        let q = |h: u32, m: u32, l: u32| quality.pick(h, m, l).to_string();
        match encoder {
            "libx264" => args.extend([
                "-preset".into(),
                "medium".into(),
                "-crf".into(),
                q(21, 26, 31),
            ]),
            "libvpx-vp9" => args.extend([
                "-crf".into(),
                q(32, 37, 42),
                "-b:v".into(),
                "0".into(),
                "-row-mt".into(),
                "1".into(),
                "-deadline".into(),
                "good".into(),
                "-cpu-used".into(),
                "4".into(),
            ]),
            "mpeg4" => args.extend(["-q:v".into(), q(3, 6, 10)]),
            // Encoders without a constant-quality mode get a bitrate from the frame size.
            _ => {
                let (w, h) = out_size.unwrap_or((video.width, video.height));
                let fps = video.fps.unwrap_or(30.0).min(120.0);
                let bits_per_pixel = quality.pick(0.1, 0.06, 0.035);
                let kbps = (f64::from(w) * f64::from(h) * fps * bits_per_pixel / 1000.0)
                    .clamp(150.0, 20_000.0) as u32;
                args.extend(["-b:v".into(), format!("{kbps}k")]);
            }
        }
        // 4:2:0 plays everywhere; some inputs (screen captures, PNG sequences) are 4:4:4.
        if encoder != "mjpeg" {
            args.extend(["-pix_fmt".into(), "yuv420p".into()]);
        }
        Ok(args)
    }

    /// Audio encoding options for this container at `quality`.
    pub fn audio_codec_args(
        self,
        tools: &ToolBox,
        quality: MediaQuality,
    ) -> AppResult<Vec<String>> {
        let encoder = ffmpeg::pick_encoder(tools, self.audio_encoders())
            .ok_or_else(|| missing_encoder(self.audio_encoders()))?;
        let mut args: Vec<String> = vec!["-c:a".into(), encoder.into()];
        if !encoder.starts_with("pcm_") {
            args.extend(["-b:a".into(), format!("{}k", quality.audio_kbps())]);
        }
        Ok(args)
    }

    /// Copies the video track when this container takes it, otherwise encodes it.
    pub fn video_copy_or_encode(
        self,
        tools: &ToolBox,
        quality: MediaQuality,
        video: &VideoStream,
    ) -> AppResult<Vec<String>> {
        if self.copies_video(&video.codec) {
            Ok(vec!["-c:v".into(), "copy".into()])
        } else {
            self.video_codec_args(tools, quality, video, None)
        }
    }

    /// Copies the audio tracks when this container takes all of them, otherwise encodes.
    pub fn audio_copy_or_encode(
        self,
        tools: &ToolBox,
        quality: MediaQuality,
        info: &MediaInfo,
    ) -> AppResult<Vec<String>> {
        if info.audio.is_empty() {
            return Ok(Vec::new());
        }
        if info.audio.iter().all(|a| self.copies_audio(&a.codec)) {
            Ok(vec!["-c:a".into(), "copy".into()])
        } else {
            self.audio_codec_args(tools, quality)
        }
    }
}

// ---------------------------------------------------------------------------------------
// Audio formats
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AudioFormat {
    Mp3,
    M4a,
    Aac,
    Ogg,
    Opus,
    Flac,
    Wav,
    Aiff,
    Wma,
    /// Matroska audio: takes any codec, used to extract a track as it is.
    Mka,
}

impl AudioFormat {
    pub fn extension(self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::M4a => "m4a",
            AudioFormat::Aac => "aac",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Opus => "opus",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
            AudioFormat::Aiff => "aiff",
            AudioFormat::Wma => "wma",
            AudioFormat::Mka => "mka",
        }
    }

    /// Format of an audio input, to keep it.
    pub fn for_input(path: &Path) -> Self {
        match fs_utils::extension_of(path).as_deref() {
            Some("mp3") => AudioFormat::Mp3,
            Some("aac") => AudioFormat::Aac,
            Some("ogg" | "oga") => AudioFormat::Ogg,
            Some("opus") => AudioFormat::Opus,
            Some("flac") => AudioFormat::Flac,
            Some("wav") => AudioFormat::Wav,
            Some("aiff" | "aif") => AudioFormat::Aiff,
            Some("wma") => AudioFormat::Wma,
            Some("mka") => AudioFormat::Mka,
            _ => AudioFormat::M4a,
        }
    }

    /// Format that holds a codec as it is, to extract a track without re-encoding.
    pub fn for_codec(codec: &str) -> Self {
        [
            AudioFormat::Mp3,
            AudioFormat::M4a,
            AudioFormat::Opus,
            AudioFormat::Ogg,
            AudioFormat::Flac,
            AudioFormat::Wav,
        ]
        .into_iter()
        .find(|f| f.copies(codec))
        .unwrap_or(AudioFormat::Mka)
    }

    pub fn is_lossless(self) -> bool {
        matches!(
            self,
            AudioFormat::Flac | AudioFormat::Wav | AudioFormat::Aiff
        )
    }

    fn encoders(self) -> &'static [&'static str] {
        match self {
            AudioFormat::Mp3 => &["libmp3lame"],
            AudioFormat::M4a | AudioFormat::Aac => &["aac"],
            AudioFormat::Ogg => &["libvorbis", "libopus"],
            AudioFormat::Opus => &["libopus"],
            AudioFormat::Flac => &["flac"],
            AudioFormat::Wav => &["pcm_s16le"],
            AudioFormat::Aiff => &["pcm_s16be"],
            AudioFormat::Wma => &["wmav2"],
            AudioFormat::Mka => &["flac"],
        }
    }

    /// Codecs this format takes without re-encoding.
    pub fn copies(self, codec: &str) -> bool {
        let list: &[&str] = match self {
            AudioFormat::Mp3 => &["mp3"],
            AudioFormat::M4a => &["aac", "alac"],
            AudioFormat::Aac => &["aac"],
            AudioFormat::Ogg => &["vorbis", "opus", "flac"],
            AudioFormat::Opus => &["opus"],
            AudioFormat::Flac => &["flac"],
            AudioFormat::Wav => &["pcm_s16le", "pcm_s24le", "pcm_s32le", "pcm_f32le", "pcm_u8"],
            AudioFormat::Aiff => &["pcm_s16be", "pcm_s24be"],
            AudioFormat::Wma => &["wmav1", "wmav2"],
            AudioFormat::Mka => return !codec.is_empty(),
        };
        list.contains(&codec)
    }

    /// Encoding options for this format; `kbps` only applies to lossy formats.
    pub fn codec_args(self, tools: &ToolBox, kbps: u32) -> AppResult<Vec<String>> {
        let encoder = ffmpeg::pick_encoder(tools, self.encoders())
            .ok_or_else(|| missing_encoder(self.encoders()))?;
        let mut args: Vec<String> = vec!["-c:a".into(), encoder.into()];
        if !self.is_lossless() && self != AudioFormat::Mka {
            args.extend(["-b:a".into(), format!("{kbps}k")]);
        }
        Ok(args)
    }

    /// Copies `codec` when this format takes it and no bitrate was asked for.
    pub fn copy_or_encode(
        self,
        tools: &ToolBox,
        codec: &str,
        kbps: Option<u32>,
    ) -> AppResult<Vec<String>> {
        if kbps.is_none() && self.copies(codec) {
            Ok(vec!["-c:a".into(), "copy".into()])
        } else {
            self.codec_args(tools, kbps.unwrap_or(MediaQuality::High.audio_kbps()))
        }
    }
}

fn missing_encoder(candidates: &[&str]) -> AppError {
    AppError::ToolMissing(format!("FFmpeg encoder ({})", candidates.join(" / ")))
}

// ---------------------------------------------------------------------------------------
// Time ranges
// ---------------------------------------------------------------------------------------

/// Part of a media file, in milliseconds. Without `end_ms` it goes to the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct TimeRange {
    pub start_ms: u64,
    #[serde(default)]
    #[ts(optional)]
    pub end_ms: Option<u64>,
}

impl TimeRange {
    pub const ALL: TimeRange = TimeRange {
        start_ms: 0,
        end_ms: None,
    };

    pub fn validate(self) -> AppResult<Self> {
        let end_ok = self
            .end_ms
            .is_none_or(|end| end > self.start_ms && end <= MAX_DURATION_MS);
        if self.start_ms > MAX_DURATION_MS || !end_ok {
            return Err(AppError::InvalidParameter(
                "the end must come after the start".into(),
            ));
        }
        Ok(self)
    }

    /// Checks the range against the real duration and returns how long the part is, when
    /// the duration is known. The end is clamped to the duration.
    pub fn resolve(self, duration_ms: Option<u64>) -> AppResult<Option<u64>> {
        let Some(total) = duration_ms else {
            return Ok(self.end_ms.map(|end| end - self.start_ms));
        };
        if self.start_ms >= total {
            return Err(AppError::InvalidParameter(format!(
                "the start ({}) is past the end of the file ({})",
                ffmpeg::seconds_arg(self.start_ms),
                ffmpeg::seconds_arg(total)
            )));
        }
        let end = self.end_ms.map_or(total, |end| end.min(total));
        Ok(Some(end - self.start_ms))
    }

    pub fn is_all(self) -> bool {
        self == Self::ALL
    }

    /// Input options that seek to the start (before `-i`, so seeking is fast).
    pub fn seek_args(self, cmd: &mut Command) {
        if self.start_ms > 0 {
            cmd.arg("-ss").arg(ffmpeg::seconds_arg(self.start_ms));
        }
    }

    /// Output option that limits the length.
    pub fn length_args(cmd: &mut Command, length_ms: Option<u64>, has_end: bool) {
        if let (true, Some(length)) = (has_end, length_ms) {
            cmd.arg("-t").arg(ffmpeg::seconds_arg(length));
        }
    }
}

/// Converts a length in milliseconds to a duration for progress.
pub fn ms(length: Option<u64>) -> Option<Duration> {
    length.map(Duration::from_millis)
}

// ---------------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------------

/// Validates an input of the given category: existing regular file with a known
/// extension. Its content is checked by ffprobe when the job runs.
pub fn validate_input(path: &Path, category: JobCategory) -> AppResult<PathBuf> {
    let path = fs_utils::validate_input_file(path)?;
    let found = fs_utils::extension_of(&path)
        .as_deref()
        .and_then(fs_utils::detect_category);
    if found != Some(category) {
        let what = match category {
            JobCategory::Video => "video",
            _ => "audio",
        };
        return Err(AppError::InvalidParameter(format!(
            "not a supported {what} file: {}",
            path.display()
        )));
    }
    Ok(path)
}

/// Validates an input that may be a video or an audio file.
pub fn validate_video_or_audio(path: &Path) -> AppResult<PathBuf> {
    validate_input(path, JobCategory::Audio).or_else(|_| validate_input(path, JobCategory::Video))
}

/// FFmpeg and ffprobe are both needed by every media operation.
pub fn require_ffmpeg(tools: &ToolBox) -> AppResult<()> {
    for id in [ToolId::Ffmpeg, ToolId::Ffprobe] {
        if !tools.is_available(id) {
            return Err(AppError::ToolMissing(
                crate::tools::definition(id).name.to_owned(),
            ));
        }
    }
    Ok(())
}

pub fn require_video(info: &MediaInfo) -> AppResult<&VideoStream> {
    info.video
        .as_ref()
        .ok_or_else(|| AppError::InvalidFile("the file has no video track".into()))
}

pub fn require_audio(info: &MediaInfo) -> AppResult<&crate::tools::ffmpeg::AudioStream> {
    info.audio
        .first()
        .ok_or_else(|| AppError::InvalidFile("the file has no audio track".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containers_copy_only_what_they_hold() {
        assert!(VideoContainer::Mp4.copies_video("h264"));
        assert!(!VideoContainer::Mp4.copies_video("vp9"));
        assert!(VideoContainer::Webm.copies_audio("opus"));
        assert!(!VideoContainer::Webm.copies_audio("aac"));
        assert!(VideoContainer::Mkv.copies_audio("flac"));
        assert_eq!(
            VideoContainer::for_input(Path::new("/a/b.WMV")),
            VideoContainer::Mp4
        );
        assert_eq!(
            VideoContainer::for_input(Path::new("/a/b.mkv")),
            VideoContainer::Mkv
        );
        assert!(VideoContainer::Mp4.is_native(Path::new("/a/b.m4v")));
        assert!(!VideoContainer::Mp4.is_native(Path::new("/a/b.ts")));
    }

    #[test]
    fn audio_formats_for_inputs_and_codecs() {
        assert_eq!(
            AudioFormat::for_input(Path::new("/x.oga")),
            AudioFormat::Ogg
        );
        assert_eq!(
            AudioFormat::for_input(Path::new("/x.aif")),
            AudioFormat::Aiff
        );
        assert_eq!(AudioFormat::for_codec("aac"), AudioFormat::M4a);
        assert_eq!(AudioFormat::for_codec("vorbis"), AudioFormat::Ogg);
        assert_eq!(AudioFormat::for_codec("dts"), AudioFormat::Mka);
        assert!(!AudioFormat::Mka.copies(""));
    }

    #[test]
    fn copy_unless_a_bitrate_is_asked() {
        let tools = ToolBox::default();
        // Unknown encoder list: the preferred encoder is used.
        assert_eq!(
            AudioFormat::Mp3
                .copy_or_encode(&tools, "mp3", None)
                .unwrap(),
            vec!["-c:a", "copy"]
        );
        assert_eq!(
            AudioFormat::Mp3
                .copy_or_encode(&tools, "mp3", Some(128))
                .unwrap(),
            vec!["-c:a", "libmp3lame", "-b:a", "128k"]
        );
        assert_eq!(
            AudioFormat::Flac
                .copy_or_encode(&tools, "mp3", None)
                .unwrap(),
            vec!["-c:a", "flac"]
        );
    }

    #[test]
    fn validates_time_ranges() {
        assert!(TimeRange {
            start_ms: 5,
            end_ms: Some(5)
        }
        .validate()
        .is_err());
        assert!(TimeRange {
            start_ms: MAX_DURATION_MS + 1,
            end_ms: None
        }
        .validate()
        .is_err());
        let range = TimeRange {
            start_ms: 1000,
            end_ms: Some(9000),
        }
        .validate()
        .unwrap();
        assert_eq!(range.resolve(Some(5000)).unwrap(), Some(4000));
        assert_eq!(range.resolve(None).unwrap(), Some(8000));
        assert!(range.resolve(Some(1000)).is_err());
        assert_eq!(TimeRange::ALL.resolve(Some(7000)).unwrap(), Some(7000));
        assert!(validate_kbps(Some(16)).is_err());
        assert_eq!(validate_kbps(Some(128)).unwrap(), Some(128));
    }
}
