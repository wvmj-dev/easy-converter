//! Tauri commands called by the frontend. Every argument is validated here or below.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, AppResult};
use crate::fs_utils::{self, FileInfo};
use crate::jobs::{JobCategory, JobId, JobQueue, JobRequest};
use crate::ops;
use crate::settings::{Settings, SettingsState};
use crate::tools::resolver::ResolveContext;
use crate::tools::{ffmpeg, qpdf, ToolBox, ToolId, ToolRegistry, ToolStatus};

pub fn resolve_context<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> ResolveContext {
    ResolveContext {
        sidecar_dir: std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf)),
        resource_dir: app.path().resource_dir().ok(),
        custom_paths: settings.tool_paths.clone(),
    }
}

/// Tools as they are now, for building a job.
async fn toolbox<R: Runtime>(
    app: &AppHandle<R>,
    registry: &ToolRegistry,
    settings: &Settings,
) -> ToolBox {
    let ctx = resolve_context(app, settings);
    registry.toolbox(&ctx, std::env::current_exe().ok()).await
}

#[tauri::command]
pub async fn enqueue_job<R: Runtime>(
    app: AppHandle<R>,
    request: JobRequest,
    queue: State<'_, JobQueue>,
    registry: State<'_, ToolRegistry>,
    settings: State<'_, Arc<SettingsState>>,
) -> AppResult<JobId> {
    let settings = settings.get();
    let tools = toolbox(&app, &registry, &settings).await;
    let ctx = ops::OpsContext { settings, tools };
    // Validation touches the file system (canonical paths, format sniffing).
    let spec = tokio::task::spawn_blocking(move || ops::build_job(request, &ctx))
        .await
        .map_err(|err| AppError::Internal(err.to_string()))??;
    queue.enqueue(spec)
}

#[tauri::command]
pub async fn cancel_job(id: JobId, queue: State<'_, JobQueue>) -> AppResult<()> {
    queue.cancel(id)
}

#[tauri::command]
pub async fn get_tools_status<R: Runtime>(
    app: AppHandle<R>,
    registry: State<'_, ToolRegistry>,
    settings: State<'_, Arc<SettingsState>>,
) -> AppResult<Vec<ToolStatus>> {
    let ctx = resolve_context(&app, &settings.get());
    Ok(registry.statuses(&ctx).await)
}

#[tauri::command]
pub async fn refresh_tools<R: Runtime>(
    app: AppHandle<R>,
    registry: State<'_, ToolRegistry>,
    settings: State<'_, Arc<SettingsState>>,
) -> AppResult<Vec<ToolStatus>> {
    let ctx = resolve_context(&app, &settings.get());
    Ok(registry.refresh(&ctx).await)
}

#[tauri::command]
pub async fn probe_file<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    registry: State<'_, ToolRegistry>,
    settings: State<'_, Arc<SettingsState>>,
) -> AppResult<FileInfo> {
    let mut info = tokio::task::spawn_blocking(move || fs_utils::probe(&path))
        .await
        .map_err(|err| AppError::Internal(err.to_string()))??;
    match info.category {
        Some(JobCategory::Pdf) => {
            let tools = toolbox(&app, &registry, &settings.get()).await;
            info.pdf = qpdf::info(&tools, &info.path).await;
        }
        Some(JobCategory::Video | JobCategory::Audio) => {
            let tools = toolbox(&app, &registry, &settings.get()).await;
            // A file ffprobe cannot read is refused now instead of failing every job later.
            if tools.is_available(ToolId::Ffprobe) {
                info.media = Some(ffmpeg::probe(&tools, &info.path).await?);
            }
        }
        _ => {}
    }
    Ok(info)
}

#[tauri::command]
pub async fn get_settings(settings: State<'_, Arc<SettingsState>>) -> AppResult<Settings> {
    Ok(settings.get())
}

#[tauri::command]
pub async fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    new_settings: Settings,
    settings: State<'_, Arc<SettingsState>>,
    registry: State<'_, ToolRegistry>,
) -> AppResult<Settings> {
    let previous = settings.get();
    let saved = settings.update(&app, new_settings)?;
    if saved.tool_paths != previous.tool_paths {
        registry.refresh(&resolve_context(&app, &saved)).await;
    }
    Ok(saved)
}

/// Opens a file produced by a job with the default app. Only job outputs are allowed.
#[tauri::command]
pub async fn open_output<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    queue: State<'_, JobQueue>,
) -> AppResult<()> {
    ensure_known_output(&queue, &path)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|err| AppError::Internal(err.to_string()))
}

/// Shows a file produced by a job in the system file manager.
#[tauri::command]
pub async fn reveal_output<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    queue: State<'_, JobQueue>,
) -> AppResult<()> {
    ensure_known_output(&queue, &path)?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|err| AppError::Internal(err.to_string()))
}

fn ensure_known_output(queue: &JobQueue, path: &Path) -> AppResult<()> {
    if !queue.is_known_output(path) {
        return Err(AppError::NotAllowed(format!(
            "not a file produced by this app: {}",
            path.display()
        )));
    }
    if !path.exists() {
        return Err(AppError::NotFound(path.display().to_string()));
    }
    Ok(())
}
