use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::ErrorPayload;

pub type JobId = u64;

/// File family a job works on. Used for scheduling limits and timeouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum JobCategory {
    Image,
    Pdf,
    Video,
    Audio,
    Document,
    Spreadsheet,
    Presentation,
    Subtitle,
    Ebook,
    Archive,
}

impl JobCategory {
    pub const ALL: [JobCategory; 10] = [
        JobCategory::Image,
        JobCategory::Pdf,
        JobCategory::Video,
        JobCategory::Audio,
        JobCategory::Document,
        JobCategory::Spreadsheet,
        JobCategory::Presentation,
        JobCategory::Subtitle,
        JobCategory::Ebook,
        JobCategory::Archive,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

/// Snapshot of a job, sent to the frontend with every job event.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Job {
    pub id: JobId,
    /// Operation name, matching the `type` tag of the `JobRequest` variant.
    pub kind: String,
    pub category: JobCategory,
    pub inputs: Vec<PathBuf>,
    /// Final paths of the files produced, filled when the job is done.
    pub outputs: Vec<PathBuf>,
    pub status: JobStatus,
    /// Progress from 0 to 100, or `null` when it cannot be measured.
    pub progress: Option<f32>,
    pub error: Option<ErrorPayload>,
}
