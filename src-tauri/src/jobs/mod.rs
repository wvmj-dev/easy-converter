//! Job queue: scheduling, progress, cancellation and timeouts.

pub mod events;
pub mod job;
pub mod queue;

use serde::Deserialize;
use ts_rs::TS;

use crate::ops::archive::{ArchiveCreateParams, ArchiveExtractParams};
use crate::ops::audio::{
    AudioConvertParams, AudioMonoParams, AudioNormalizeParams, AudioTrimParams,
};
use crate::ops::document::{DocumentConvertParams, PdfToDocumentParams};
use crate::ops::ebook::EbookConvertParams;
use crate::ops::image::{
    ImageCompressParams, ImageConvertParams, ImageCropParams, ImageFlipParams, ImageMergeParams,
    ImageResizeParams, ImageRotateParams, ImageStripMetadataParams,
};
use crate::ops::pdf::{
    ImagesToPdfParams, PdfCompressParams, PdfExtractImagesParams, PdfExtractTextParams,
    PdfMergeParams, PdfOcrParams, PdfProtectParams, PdfRotateParams, PdfSelectPagesParams,
    PdfSplitParams, PdfToImagesParams, PdfUnlockParams,
};
use crate::ops::presentation::{PresentationConvertParams, PresentationToImagesParams};
use crate::ops::spreadsheet::{CsvToJsonParams, JsonToCsvParams, SpreadsheetConvertParams};
use crate::ops::subtitle::{SubtitleConvertParams, SubtitleShiftParams, VideoEmbedSubtitleParams};
use crate::ops::video::{
    VideoAudioTrackParams, VideoCompressParams, VideoConvertParams, VideoExtractAudioParams,
    VideoMergeParams, VideoResizeParams, VideoToGifParams, VideoTrimParams,
};

pub use events::{EventSink, JobEvent, TauriEventSink};
pub use job::{Job, JobCategory, JobId, JobStatus};
pub use queue::{JobContext, JobQueue, JobSpec, PlannedOutput};

/// An operation requested by the frontend. Each operation is one variant, tagged by `type`.
///
/// Operations are added phase by phase (images, PDF, video...), each with its function in
/// `ops/<category>.rs`.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum JobRequest {
    ImageConvert(ImageConvertParams),
    ImageResize(ImageResizeParams),
    ImageCrop(ImageCropParams),
    ImageRotate(ImageRotateParams),
    ImageFlip(ImageFlipParams),
    ImageCompress(ImageCompressParams),
    ImageStripMetadata(ImageStripMetadataParams),
    ImageMerge(ImageMergeParams),
    ImagesToPdf(ImagesToPdfParams),
    PdfMerge(PdfMergeParams),
    PdfSplit(PdfSplitParams),
    PdfSelectPages(PdfSelectPagesParams),
    PdfRotate(PdfRotateParams),
    PdfToImages(PdfToImagesParams),
    PdfExtractText(PdfExtractTextParams),
    PdfExtractImages(PdfExtractImagesParams),
    PdfProtect(PdfProtectParams),
    PdfUnlock(PdfUnlockParams),
    PdfCompress(PdfCompressParams),
    PdfOcr(PdfOcrParams),
    VideoConvert(VideoConvertParams),
    VideoTrim(VideoTrimParams),
    VideoCompress(VideoCompressParams),
    VideoResize(VideoResizeParams),
    VideoExtractAudio(VideoExtractAudioParams),
    VideoToGif(VideoToGifParams),
    VideoMerge(VideoMergeParams),
    VideoAudioTrack(VideoAudioTrackParams),
    AudioConvert(AudioConvertParams),
    AudioTrim(AudioTrimParams),
    AudioNormalize(AudioNormalizeParams),
    AudioMono16k(AudioMonoParams),
    DocumentConvert(DocumentConvertParams),
    PdfToDocument(PdfToDocumentParams),
    SpreadsheetConvert(SpreadsheetConvertParams),
    CsvToJson(CsvToJsonParams),
    JsonToCsv(JsonToCsvParams),
    PresentationConvert(PresentationConvertParams),
    PresentationToImages(PresentationToImagesParams),
    SubtitleConvert(SubtitleConvertParams),
    SubtitleShift(SubtitleShiftParams),
    VideoEmbedSubtitle(VideoEmbedSubtitleParams),
    EbookConvert(EbookConvertParams),
    ArchiveCreate(ArchiveCreateParams),
    ArchiveExtract(ArchiveExtractParams),
}
