//! Finds each external tool and probes its version and capabilities.
//!
//! Order of resolution for executables:
//! 1. bundled sidecar (Windows only; on Linux the required tools are package dependencies);
//! 2. the `PATH`;
//! 3. well-known install folders (Windows only, for optional tools that rarely add
//!    themselves to the `PATH`);
//! 4. the path configured by the user.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::RwLock;

use super::capabilities::{parse_ffmpeg_encoders, parse_tesseract_languages, parse_version};
use super::process::{capture, harden};
use super::{definition, ToolDefinition, ToolId, ToolKind, ToolSource, ToolStatus, TOOLS};
use crate::error::{AppError, AppResult};

/// LibreOffice can take a few seconds to print its version on a cold start.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// Where to look for tools, besides the `PATH`.
#[derive(Debug, Clone, Default)]
pub struct ResolveContext {
    /// Folder of the app executable, where Tauri places sidecars.
    pub sidecar_dir: Option<PathBuf>,
    /// Tauri resource folder (PDFium, Windows DLLs and data).
    pub resource_dir: Option<PathBuf>,
    /// Paths chosen by the user.
    pub custom_paths: BTreeMap<ToolId, PathBuf>,
}

/// Holds the resolved status of every tool.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    statuses: RwLock<Option<Vec<ToolStatus>>>,
}

impl ToolRegistry {
    /// Returns the cached statuses, resolving them on first use.
    pub async fn statuses(&self, ctx: &ResolveContext) -> Vec<ToolStatus> {
        if let Some(statuses) = self.statuses.read().await.as_ref() {
            return statuses.clone();
        }
        self.refresh(ctx).await
    }

    /// Resolves all tools again, for example after the user sets a custom path.
    pub async fn refresh(&self, ctx: &ResolveContext) -> Vec<ToolStatus> {
        let mut guard = self.statuses.write().await;
        let handles: Vec<_> = TOOLS
            .iter()
            .map(|def| {
                let def = *def;
                let ctx = ctx.clone();
                tokio::spawn(async move { resolve(&def, &ctx).await })
            })
            .collect();
        let mut statuses = Vec::with_capacity(handles.len());
        for (handle, def) in handles.into_iter().zip(TOOLS.iter()) {
            statuses.push(handle.await.unwrap_or_else(|err| {
                missing(def, Some(format!("resolution task failed: {err}")))
            }));
        }
        *guard = Some(statuses.clone());
        statuses
    }

    /// Status of one tool, if already resolved.
    pub async fn status(&self, id: ToolId) -> Option<ToolStatus> {
        self.statuses
            .read()
            .await
            .as_ref()
            .and_then(|list| list.iter().find(|s| s.id == id).cloned())
    }

    /// Builds a command for an available executable tool, with its environment prepared.
    pub async fn command(&self, id: ToolId) -> AppResult<Command> {
        let status = self
            .status(id)
            .await
            .filter(|s| s.available)
            .ok_or_else(|| AppError::ToolMissing(definition(id).name.to_owned()))?;
        if matches!(definition(id).kind, ToolKind::Library { .. }) {
            return Err(AppError::Internal(format!("{id:?} is not an executable")));
        }
        let path = status
            .path
            .ok_or_else(|| AppError::ToolMissing(definition(id).name.to_owned()))?;
        Ok(tool_command(id, &path, status.runtime_dir.as_deref()))
    }

    /// Path of an available library tool (PDFium).
    pub async fn library_path(&self, id: ToolId) -> AppResult<PathBuf> {
        self.status(id)
            .await
            .filter(|s| s.available)
            .and_then(|s| s.path)
            .ok_or_else(|| AppError::ToolMissing(definition(id).name.to_owned()))
    }
}

/// Creates a command for a tool binary. Bundled Windows tools run from their resource
/// folder, with it in the `PATH` so their DLLs are found.
fn tool_command(id: ToolId, path: &Path, runtime_dir: Option<&Path>) -> Command {
    let mut cmd = Command::new(path);
    harden(&mut cmd);
    if let Some(dir) = runtime_dir {
        cmd.current_dir(dir);
        let mut paths = vec![dir.to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("PATH", joined);
        }
        if id == ToolId::Tesseract {
            cmd.env("TESSDATA_PREFIX", dir.join("tessdata"));
        }
    }
    cmd
}

async fn resolve(def: &ToolDefinition, ctx: &ResolveContext) -> ToolStatus {
    match def.kind {
        ToolKind::Library {
            resource_dir,
            linux_file,
            windows_file,
        } => resolve_library(def, ctx, resource_dir, linux_file, windows_file),
        ToolKind::Executable {
            names,
            version_args,
            signature,
            windows_sidecar,
        } => resolve_executable(def, ctx, names, version_args, signature, windows_sidecar).await,
    }
}

fn resolve_library(
    def: &ToolDefinition,
    ctx: &ResolveContext,
    resource_dir: &str,
    linux_file: &str,
    windows_file: &str,
) -> ToolStatus {
    let file = if cfg!(windows) {
        windows_file
    } else {
        linux_file
    };
    let bundled_dir = ctx.resource_dir.as_ref().map(|r| r.join(resource_dir));
    let bundled = bundled_dir
        .as_ref()
        .map(|dir| dir.join(file))
        .filter(|p| p.is_file());

    let (path, source) = match (bundled, ctx.custom_paths.get(&def.id)) {
        (Some(path), _) => (path, ToolSource::Bundled),
        (None, Some(custom)) if custom.is_file() => (custom.clone(), ToolSource::Custom),
        _ => return missing(def, None),
    };
    let version = path
        .parent()
        .and_then(|dir| std::fs::read_to_string(dir.join("VERSION")).ok())
        .and_then(|text| parse_pdfium_version(&text));

    ToolStatus {
        available: true,
        path: Some(path),
        source: Some(source),
        version,
        ..missing(def, None)
    }
}

/// pdfium-binaries ships a `VERSION` file with lines like `BUILD=6721`.
fn parse_pdfium_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("BUILD="))
        .map(|build| format!("chromium/{}", build.trim()))
}

async fn resolve_executable(
    def: &ToolDefinition,
    ctx: &ResolveContext,
    names: &[&str],
    version_args: &[&str],
    signature: &str,
    windows_sidecar: bool,
) -> ToolStatus {
    let Some((path, source, runtime_dir)) = locate_executable(def, ctx, names, windows_sidecar)
    else {
        return missing(def, None);
    };

    let mut status = ToolStatus {
        path: Some(path.clone()),
        source: Some(source),
        runtime_dir: runtime_dir.clone(),
        ..missing(def, None)
    };

    let mut cmd = tool_command(def.id, &path, runtime_dir.as_deref());
    cmd.args(version_args);
    match capture(cmd, PROBE_TIMEOUT).await {
        Ok(out) => {
            let version = parse_version(&out.stdout).or_else(|| parse_version(&out.stderr));
            if version.is_none() && !out.status.success() {
                status.error = Some(format!("version check exited with {}", out.status));
                return status;
            }
            if !is_expected_tool(&out.stdout, &out.stderr, signature) {
                status.error = Some(format!(
                    "{} does not look like {}",
                    path.display(),
                    def.name
                ));
                return status;
            }
            status.version = version;
            status.available = true;
        }
        Err(err) => {
            status.error = Some(err.to_string());
            return status;
        }
    }

    match def.id {
        ToolId::Ffmpeg => {
            let mut cmd = tool_command(def.id, &path, runtime_dir.as_deref());
            cmd.args(["-hide_banner", "-encoders"]);
            status.encoders = capture(cmd, PROBE_TIMEOUT)
                .await
                .ok()
                .map(|out| parse_ffmpeg_encoders(&out.stdout));
        }
        ToolId::Tesseract => {
            let mut cmd = tool_command(def.id, &path, runtime_dir.as_deref());
            cmd.arg("--list-langs");
            // Older versions print the list to stderr.
            status.languages = capture(cmd, PROBE_TIMEOUT).await.ok().map(|out| {
                let langs = parse_tesseract_languages(&out.stdout);
                if langs.is_empty() {
                    parse_tesseract_languages(&out.stderr)
                } else {
                    langs
                }
            });
        }
        _ => {}
    }
    status
}

/// Whether the version output names the expected program.
fn is_expected_tool(stdout: &str, stderr: &str, signature: &str) -> bool {
    [stdout, stderr]
        .iter()
        .any(|text| text.to_lowercase().contains(signature))
}

type Located = (PathBuf, ToolSource, Option<PathBuf>);

fn locate_executable(
    def: &ToolDefinition,
    ctx: &ResolveContext,
    names: &[&str],
    windows_sidecar: bool,
) -> Option<Located> {
    if cfg!(windows) && windows_sidecar {
        if let Some(dir) = &ctx.sidecar_dir {
            if let Some(path) = find_in_dirs(names, std::slice::from_ref(dir)) {
                let runtime_dir = ctx
                    .resource_dir
                    .as_ref()
                    .map(|r| r.join(windows_runtime_folder(def.id)))
                    .filter(|d| d.is_dir());
                return Some((path, ToolSource::Bundled, runtime_dir));
            }
        }
    }

    if let Some(path_var) = std::env::var_os("PATH") {
        let dirs: Vec<PathBuf> = std::env::split_paths(&path_var).collect();
        if let Some(path) = find_in_dirs(names, &dirs) {
            return Some((path, ToolSource::System, None));
        }
    }

    if cfg!(windows) {
        if let Some(path) = find_in_dirs(names, &windows_known_dirs(def.id)) {
            return Some((path, ToolSource::KnownLocation, None));
        }
    }

    ctx.custom_paths
        .get(&def.id)
        .filter(|p| is_executable(p))
        .map(|p| (p.clone(), ToolSource::Custom, None))
}

/// Resource subfolder holding the DLLs and data of a bundled Windows tool.
fn windows_runtime_folder(id: ToolId) -> &'static str {
    match id {
        ToolId::Qpdf => "qpdf",
        ToolId::Tesseract => "tesseract",
        _ => "",
    }
}

/// Usual install folders of optional tools on Windows.
fn windows_known_dirs(id: ToolId) -> Vec<PathBuf> {
    let program_files: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    let mut dirs = Vec::new();
    for base in &program_files {
        match id {
            ToolId::Libreoffice => dirs.push(base.join("LibreOffice").join("program")),
            ToolId::Calibre => dirs.push(base.join("Calibre2")),
            ToolId::SevenZip => dirs.push(base.join("7-Zip")),
            ToolId::Pandoc => dirs.push(base.join("Pandoc")),
            ToolId::Ghostscript => {
                // Ghostscript installs into versioned folders: gs\gs10.04.0\bin.
                if let Ok(entries) = std::fs::read_dir(base.join("gs")) {
                    let mut versions: Vec<PathBuf> =
                        entries.flatten().map(|e| e.path().join("bin")).collect();
                    versions.sort();
                    versions.reverse();
                    dirs.extend(versions);
                }
            }
            _ => {}
        }
    }
    if id == ToolId::Pandoc {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Pandoc"));
        }
    }
    dirs
}

fn find_in_dirs(names: &[&str], dirs: &[PathBuf]) -> Option<PathBuf> {
    names.iter().find_map(|name| {
        let file = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            (*name).to_owned()
        };
        dirs.iter()
            .map(|dir| dir.join(&file))
            .find(|candidate| is_executable(candidate))
    })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn missing(def: &ToolDefinition, error: Option<String>) -> ToolStatus {
    ToolStatus {
        id: def.id,
        name: def.name.to_owned(),
        required: def.required,
        available: false,
        path: None,
        source: None,
        version: None,
        error,
        encoders: None,
        languages: None,
        runtime_dir: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pdfium_version_file() {
        let text = "MAJOR=133\nMINOR=0\nBUILD=6921\nPATCH=0\n";
        assert_eq!(parse_pdfium_version(text).as_deref(), Some("chromium/6921"));
        assert_eq!(parse_pdfium_version("garbage"), None);
    }

    #[cfg(unix)]
    #[test]
    fn finds_executables_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("easy-converter-find-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("tool");
        let data = dir.join("data");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        std::fs::write(&data, "").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert_eq!(
            find_in_dirs(&["tool"], std::slice::from_ref(&dir)),
            Some(exe)
        );
        assert_eq!(find_in_dirs(&["data"], std::slice::from_ref(&dir)), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn custom_path_is_used_when_not_in_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("easy-converter-custom-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake-pandoc");
        std::fs::write(&fake, "#!/bin/sh\necho 'pandoc 9.9.9'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let def = ToolDefinition {
            kind: super::super::exe(
                &["easy-converter-no-such-tool"],
                &["--version"],
                "pandoc",
                false,
            ),
            ..*definition(ToolId::Pandoc)
        };
        let ctx = ResolveContext {
            custom_paths: BTreeMap::from([(ToolId::Pandoc, fake.clone())]),
            ..ResolveContext::default()
        };
        let status = resolve(&def, &ctx).await;

        assert!(status.available, "{status:?}");
        assert_eq!(status.source, Some(ToolSource::Custom));
        assert_eq!(status.version.as_deref(), Some("pandoc 9.9.9"));

        // A configured path that points to another program is refused.
        let other = dir.join("not-pandoc");
        std::fs::write(&other, "#!/bin/sh\necho 'Python 3.13.5'\n").unwrap();
        std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ctx = ResolveContext {
            custom_paths: BTreeMap::from([(ToolId::Pandoc, other.clone())]),
            ..ResolveContext::default()
        };
        let status = resolve(&def, &ctx).await;
        assert!(!status.available, "{status:?}");
        assert!(status
            .error
            .as_deref()
            .is_some_and(|e| e.contains("does not look like Pandoc")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn missing_tool_is_reported_unavailable() {
        let def = ToolDefinition {
            kind: super::super::exe(
                &["easy-converter-no-such-tool"],
                &["--version"],
                "pandoc",
                false,
            ),
            ..*definition(ToolId::Pandoc)
        };
        let status = resolve(&def, &ResolveContext::default()).await;
        assert!(!status.available);
        assert!(status.path.is_none());
    }

    /// Requires FFmpeg installed; skipped otherwise.
    #[tokio::test]
    async fn resolves_system_ffmpeg_with_encoders() {
        let status = resolve(definition(ToolId::Ffmpeg), &ResolveContext::default()).await;
        if status.path.is_none() {
            eprintln!("ffmpeg not installed, skipping");
            return;
        }
        assert!(status.available, "{status:?}");
        assert!(status.version.unwrap_or_default().contains("ffmpeg"));
        assert!(!status.encoders.unwrap_or_default().is_empty());
    }
}
