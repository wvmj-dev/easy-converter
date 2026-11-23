//! Archive operations (phase 5).
//!
//! - Creating ZIP, TAR, TAR.GZ and 7z from any files: Rust.
//! - Extracting ZIP, TAR, TAR.GZ, GZ and 7z: Rust, into a new folder (see `extract` for
//!   the safety rules). RAR, xz, bzip2, encrypted archives and rare methods go to the
//!   system 7-Zip (optional).

mod create;
mod extract;
mod names;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use ts_rs::TS;

use super::{blocking_job, process_job, OutputPlanner};
use crate::error::{AppError, AppResult};
use crate::fs_utils;
use crate::jobs::{JobCategory, JobContext, JobSpec, PlannedOutput};
use crate::tools::{sevenzip, ToolBox, ToolId};

use extract::{Failure, Sink, MAX_EXTRACT_BYTES};

/// Most files in one new archive.
pub const MAX_ARCHIVE_INPUTS: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
    Tar,
    SevenZ,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ArchiveCreateParams {
    pub inputs: Vec<PathBuf>,
    pub format: ArchiveFormat,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ArchiveExtractParams {
    pub input: PathBuf,
}

/// How an archive is read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    Zip,
    Tar,
    TarGz,
    /// A single compressed file, with the name it gets once extracted.
    Gz(String),
    SevenZ,
    /// Only 7-Zip reads it (RAR, xz, bzip2).
    SevenZipOnly,
}

/// The reader for a file name and the name of the folder it is extracted to.
fn source_of(file_name: &str) -> Option<(Source, String)> {
    let lower = file_name.to_lowercase();
    let cut = |suffix: &str| file_name[..file_name.len() - suffix.len()].to_owned();
    let pairs: [(&str, Source); 10] = [
        (".tar.gz", Source::TarGz),
        (".tgz", Source::TarGz),
        (".tar.xz", Source::SevenZipOnly),
        (".tar.bz2", Source::SevenZipOnly),
        (".tar", Source::Tar),
        (".zip", Source::Zip),
        (".7z", Source::SevenZ),
        (".rar", Source::SevenZipOnly),
        (".xz", Source::SevenZipOnly),
        (".bz2", Source::SevenZipOnly),
    ];
    for (suffix, source) in pairs {
        if lower.ends_with(suffix) && lower.len() > suffix.len() {
            return Some((source, cut(suffix)));
        }
    }
    if lower.ends_with(".gz") && lower.len() > 3 {
        let inner = cut(".gz");
        let folder = Path::new(&inner)
            .file_stem()
            .map_or_else(|| inner.clone(), |s| s.to_string_lossy().into_owned());
        return Some((Source::Gz(inner), folder));
    }
    None
}

/// Whether extracting this file needs 7-Zip whatever its content.
pub fn needs_seven_zip(file_name: &str) -> bool {
    matches!(source_of(file_name), Some((Source::SevenZipOnly, _)))
}

pub fn create(params: ArchiveCreateParams, planner: &OutputPlanner) -> AppResult<JobSpec> {
    if !(1..=MAX_ARCHIVE_INPUTS).contains(&params.inputs.len()) {
        return Err(AppError::InvalidParameter(format!(
            "between 1 and {MAX_ARCHIVE_INPUTS} files per archive"
        )));
    }
    let inputs = params
        .inputs
        .iter()
        .map(|p| fs_utils::validate_input_file(p))
        .collect::<AppResult<Vec<_>>>()?;
    let format = params.format;
    let destination = match format {
        ArchiveFormat::Zip => planner.destination(&inputs[0], "zip")?,
        ArchiveFormat::Tar => planner.destination(&inputs[0], "tar")?,
        ArchiveFormat::SevenZ => planner.destination(&inputs[0], "7z")?,
        ArchiveFormat::TarGz => {
            let tar = planner.destination(&inputs[0], "tar")?;
            let name = tar
                .file_name()
                .map(|n| format!("{}.gz", n.to_string_lossy()))
                .ok_or_else(|| AppError::InvalidPath(tar.display().to_string()))?;
            tar.with_file_name(name)
        }
    };
    let work_inputs = inputs.clone();
    Ok(blocking_job(
        "archiveCreate",
        JobCategory::Archive,
        inputs,
        move |ctx| {
            let items = create::items(&work_inputs)?;
            let temp = ctx.temp_dir.join("output.archive");
            create::create(&items, format, &temp, ctx)?;
            Ok(vec![PlannedOutput { temp, destination }])
        },
    ))
}

fn run_rust_extraction(
    source: &Source,
    input: &Path,
    out_dir: &Path,
    ctx: &JobContext,
) -> Result<(), Failure> {
    let mut sink = Sink::new(out_dir, ctx, MAX_EXTRACT_BYTES);
    match source {
        Source::Zip => extract::zip(input, &mut sink, ctx),
        Source::Tar => extract::tar(input, &mut sink, ctx),
        Source::TarGz => extract::tar_gz(input, &mut sink, ctx),
        Source::Gz(name) => extract::gz(input, name, &mut sink, ctx),
        Source::SevenZ => extract::seven_z(input, &mut sink, ctx),
        Source::SevenZipOnly => Err(Failure::NeedsSevenZip("this format".into())),
    }
}

pub fn extract(
    params: ArchiveExtractParams,
    planner: &OutputPlanner,
    tools: &ToolBox,
) -> AppResult<JobSpec> {
    let input = fs_utils::validate_input_file(&params.input)?;
    let file_name = input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (source, stem) = source_of(&file_name).ok_or_else(|| {
        AppError::InvalidParameter(format!("not a supported archive: {}", input.display()))
    })?;
    if source == Source::SevenZipOnly && !tools.is_available(ToolId::SevenZip) {
        return Err(AppError::ToolMissing("7-Zip".into()));
    }
    let destination = planner.destination_folder(&input, &names::clean_component(&stem))?;
    let tools = tools.clone();
    let work_input = input.clone();
    Ok(process_job(
        "archiveExtract",
        JobCategory::Archive,
        vec![input],
        move |ctx| async move {
            let ctx = Arc::new(ctx);
            let out_dir = ctx.temp_dir.join("extracted");
            fs::create_dir(&out_dir)?;
            let result = {
                let (ctx, input, out) = (Arc::clone(&ctx), work_input.clone(), out_dir.clone());
                tokio::task::spawn_blocking(move || {
                    run_rust_extraction(&source, &input, &out, &ctx)
                })
                .await
                .map_err(|err| AppError::Internal(format!("extraction stopped: {err}")))?
            };
            match result {
                Ok(()) => {}
                Err(Failure::App(err)) => return Err(err),
                Err(Failure::NeedsSevenZip(reason)) => {
                    if !tools.is_available(ToolId::SevenZip) {
                        return Err(if reason.contains("encrypted") {
                            AppError::Password(format!("{reason}; install 7-Zip to extract it"))
                        } else {
                            AppError::ToolMissing(format!("7-Zip ({reason})"))
                        });
                    }
                    // Start over with 7-Zip in a clean folder.
                    fs::remove_dir_all(&out_dir)?;
                    fs::create_dir(&out_dir)?;
                    sevenzip::extract(&tools, &ctx, &work_input, &out_dir, MAX_EXTRACT_BYTES)
                        .await?;
                    unpack_inner_tar(&ctx, &out_dir).await?;
                }
            }
            if fs::read_dir(&out_dir)?.next().is_none() {
                return Err(AppError::InvalidFile("the archive has no files".into()));
            }
            Ok(vec![PlannedOutput {
                temp: out_dir,
                destination,
            }])
        },
    ))
}

/// 7-Zip takes `.tar.xz` and `.tar.bz2` apart one layer at a time: what comes out is the
/// `.tar`. When that is all there is, its files are extracted too, with the Rust reader.
async fn unpack_inner_tar(ctx: &Arc<JobContext>, out_dir: &Path) -> AppResult<()> {
    let entries: Vec<PathBuf> = fs::read_dir(out_dir)?.flatten().map(|e| e.path()).collect();
    let [tar] = entries.as_slice() else {
        return Ok(());
    };
    let is_tar = tar.is_file() && fs_utils::extension_of(tar).as_deref() == Some("tar");
    if !is_tar {
        return Ok(());
    }
    let inner = ctx.temp_dir.join("inner-tar");
    fs::rename(tar, &inner)?;
    let (ctx2, out) = (Arc::clone(ctx), out_dir.to_path_buf());
    let result =
        tokio::task::spawn_blocking(move || run_rust_extraction(&Source::Tar, &inner, &out, &ctx2))
            .await
            .map_err(|err| AppError::Internal(format!("extraction stopped: {err}")))?;
    match result {
        Ok(()) => Ok(()),
        Err(Failure::App(err)) => Err(err),
        Err(Failure::NeedsSevenZip(reason)) => Err(AppError::InvalidFile(reason)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_reader_and_folder_name() {
        let cases = [
            ("Fotos.tar.gz", Source::TarGz, "Fotos"),
            ("a.TGZ", Source::TarGz, "a"),
            ("a.b.zip", Source::Zip, "a.b"),
            ("x.7z", Source::SevenZ, "x"),
            ("x.tar", Source::Tar, "x"),
            ("x.tar.xz", Source::SevenZipOnly, "x"),
            ("x.rar", Source::SevenZipOnly, "x"),
            ("notas.txt.gz", Source::Gz("notas.txt".into()), "notas"),
        ];
        for (name, source, stem) in cases {
            assert_eq!(source_of(name), Some((source, stem.to_owned())), "{name}");
        }
        assert_eq!(source_of(".zip"), None);
        assert_eq!(source_of("a.pdf"), None);
        assert!(needs_seven_zip("a.rar") && !needs_seven_zip("a.zip"));
    }
}
