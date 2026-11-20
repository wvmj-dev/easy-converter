//! Operations. Each `JobRequest` variant is turned into a [`JobSpec`] here, with its
//! parameters validated, and implemented in `ops/<category>.rs`.

pub mod image;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::fs_utils;
use crate::jobs::{JobCategory, JobContext, JobRequest, JobSpec, PlannedOutput};
use crate::settings::{OutputLocation, Settings};

/// Validates a request and builds the job that runs it.
pub fn build_job(request: JobRequest, settings: &Settings) -> AppResult<JobSpec> {
    let planner = OutputPlanner::from_settings(settings)?;
    match request {
        JobRequest::ImageConvert(params) => image::convert(params, &planner),
        JobRequest::ImageResize(params) => image::resize(params, &planner),
        JobRequest::ImageCrop(params) => image::crop(params, &planner),
        JobRequest::ImageRotate(params) => image::rotate(params, &planner),
        JobRequest::ImageFlip(params) => image::flip(params, &planner),
        JobRequest::ImageCompress(params) => image::compress(params, &planner),
        JobRequest::ImageStripMetadata(params) => image::strip_metadata(params, &planner),
        JobRequest::ImageMerge(params) => image::merge(params, &planner),
    }
}

/// Decides where output files go, from the settings: next to the input or in a chosen
/// folder, always named `<input stem>-<suffix>.<ext>`. The queue numbers the name when it
/// already exists, so nothing is overwritten.
#[derive(Debug, Clone)]
pub struct OutputPlanner {
    folder: Option<PathBuf>,
    suffix: String,
}

impl OutputPlanner {
    pub fn from_settings(settings: &Settings) -> AppResult<Self> {
        fs_utils::validate_name_fragment(&settings.output_suffix)?;
        let folder = match &settings.output {
            OutputLocation::SameFolder => None,
            OutputLocation::Folder { path } => Some(fs_utils::validate_directory(path)?),
        };
        Ok(Self {
            folder,
            suffix: settings.output_suffix.clone(),
        })
    }

    /// Destination for the result of `input` with the extension `ext`.
    /// `input` must already be validated (absolute and canonical).
    pub fn destination(&self, input: &Path, ext: &str) -> AppResult<PathBuf> {
        fs_utils::validate_extension(ext)?;
        let folder = match &self.folder {
            Some(folder) => folder.clone(),
            None => input
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| AppError::InvalidPath(input.display().to_string()))?,
        };
        Ok(folder.join(fs_utils::output_file_name(input, &self.suffix, ext)))
    }
}

/// Builds a job whose work runs on a blocking thread, so CPU-heavy code done in Rust never
/// stalls the async runtime. `work` must call `ctx.check_cancelled()` between steps: a
/// cancelled or timed-out job is reported right away, and the thread stops at the next check.
pub fn blocking_job<F>(kind: &str, category: JobCategory, inputs: Vec<PathBuf>, work: F) -> JobSpec
where
    F: FnOnce(&JobContext) -> AppResult<Vec<PlannedOutput>> + Send + 'static,
{
    JobSpec {
        kind: kind.to_owned(),
        category,
        inputs,
        runner: Box::new(move |ctx| {
            Box::pin(async move {
                let ctx = Arc::new(ctx);
                let worker = Arc::clone(&ctx);
                match tokio::task::spawn_blocking(move || work(&worker)).await {
                    Ok(result) => result,
                    Err(err) if err.is_panic() => Err(AppError::Internal(
                        "the operation stopped unexpectedly".into(),
                    )),
                    Err(err) => Err(AppError::Internal(err.to_string())),
                }
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_next_to_input_or_in_folder() {
        let dir = std::env::temp_dir().canonicalize().unwrap();
        let input = dir.join("photo.final.PNG");

        let same = OutputPlanner::from_settings(&Settings::default()).unwrap();
        assert_eq!(
            same.destination(&input, "jpg").unwrap(),
            dir.join("photo.final-convertido.jpg")
        );

        let settings = Settings {
            output: OutputLocation::Folder { path: dir.clone() },
            output_suffix: "small".into(),
            ..Settings::default()
        };
        let folder = OutputPlanner::from_settings(&settings).unwrap();
        assert_eq!(
            folder
                .destination(Path::new("/elsewhere/a.png"), "png")
                .unwrap(),
            dir.join("a-small.png")
        );
        assert!(folder.destination(&input, "../x").is_err());
    }

    #[test]
    fn rejects_missing_output_folder_and_bad_suffix() {
        let settings = Settings {
            output: OutputLocation::Folder {
                path: PathBuf::from("/definitely/not/here"),
            },
            ..Settings::default()
        };
        assert!(OutputPlanner::from_settings(&settings).is_err());

        let settings = Settings {
            output_suffix: "../escape".into(),
            ..Settings::default()
        };
        assert!(OutputPlanner::from_settings(&settings).is_err());
    }
}
