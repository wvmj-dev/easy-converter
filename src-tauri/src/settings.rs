//! User settings, persisted as JSON with `tauri-plugin-store` and always validated in Rust.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::fs_utils;
use crate::jobs::JobCategory;
use crate::tools::ToolId;

const STORE_FILE: &str = "settings.json";
const STORE_KEY: &str = "settings";

/// Longest timeout accepted for any category: 24 hours.
pub const MAX_TIMEOUT_MINUTES: u32 = 24 * 60;
/// Upper bound for concurrent jobs, regardless of the CPU count.
pub const MAX_CONCURRENCY: u32 = 64;
pub const MAX_VIDEO_CONCURRENCY: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Language {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "pt-BR")]
    PtBr,
    #[serde(rename = "en")]
    En,
}

/// Where converted files are written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "camelCase")]
#[ts(export)]
pub enum OutputLocation {
    /// Next to the original file, with the output suffix.
    SameFolder,
    /// In a folder chosen by the user.
    Folder { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    pub language: Language,
    /// Maximum number of jobs running at the same time.
    pub concurrency: u32,
    /// Maximum number of video jobs running at the same time.
    pub video_concurrency: u32,
    pub output: OutputLocation,
    /// Appended to output file names: `photo-<suffix>.png`.
    pub output_suffix: String,
    /// Timeout per category, in minutes.
    pub timeouts: BTreeMap<JobCategory, u32>,
    /// Paths chosen by the user for tools not found automatically.
    pub tool_paths: BTreeMap<ToolId, PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            concurrency: default_concurrency(),
            video_concurrency: 1,
            output: OutputLocation::SameFolder,
            output_suffix: "convertido".to_owned(),
            timeouts: JobCategory::ALL
                .into_iter()
                .map(|c| (c, default_timeout_minutes(c)))
                .collect(),
            tool_paths: BTreeMap::new(),
        }
    }
}

/// Half of the logical CPUs, at least one.
pub fn default_concurrency() -> u32 {
    let cpus = std::thread::available_parallelism().map_or(2, |n| n.get());
    u32::try_from(cpus / 2)
        .unwrap_or(1)
        .clamp(1, MAX_CONCURRENCY)
}

fn default_timeout_minutes(category: JobCategory) -> u32 {
    match category {
        JobCategory::Image | JobCategory::Subtitle => 10,
        JobCategory::Document | JobCategory::Spreadsheet | JobCategory::Presentation => 15,
        JobCategory::Ebook => 30,
        JobCategory::Pdf => 60,
        JobCategory::Audio | JobCategory::Archive => 120,
        JobCategory::Video => 12 * 60,
    }
}

impl Settings {
    /// Fills categories missing from older settings files with their defaults.
    fn normalized(mut self) -> Self {
        for category in JobCategory::ALL {
            self.timeouts
                .entry(category)
                .or_insert_with(|| default_timeout_minutes(category));
        }
        self
    }

    /// Drops saved paths that no longer exist (for example, a removed USB drive), so the
    /// rest of the saved settings is kept when the app starts.
    fn without_missing_paths(mut self) -> Self {
        if let OutputLocation::Folder { path } = &self.output {
            if !path.is_dir() {
                self.output = OutputLocation::SameFolder;
            }
        }
        self.tool_paths.retain(|_, path| path.is_file());
        self
    }

    /// Validates every field and returns the settings with canonical paths.
    pub fn validated(self) -> AppResult<Self> {
        let mut s = self.normalized();

        if !(1..=MAX_CONCURRENCY).contains(&s.concurrency) {
            return Err(AppError::InvalidParameter(format!(
                "concurrency must be between 1 and {MAX_CONCURRENCY}"
            )));
        }
        if !(1..=MAX_VIDEO_CONCURRENCY).contains(&s.video_concurrency) {
            return Err(AppError::InvalidParameter(format!(
                "video concurrency must be between 1 and {MAX_VIDEO_CONCURRENCY}"
            )));
        }
        fs_utils::validate_name_fragment(&s.output_suffix)?;

        if let OutputLocation::Folder { path } = &s.output {
            let canonical = fs_utils::validate_directory(path)?;
            s.output = OutputLocation::Folder { path: canonical };
        }

        for (category, minutes) in &s.timeouts {
            if !(1..=MAX_TIMEOUT_MINUTES).contains(minutes) {
                return Err(AppError::InvalidParameter(format!(
                    "timeout for {category:?} must be between 1 and {MAX_TIMEOUT_MINUTES} minutes"
                )));
            }
        }

        let mut tool_paths = BTreeMap::new();
        for (tool, path) in s.tool_paths {
            tool_paths.insert(tool, fs_utils::validate_input_file(&path)?);
        }
        s.tool_paths = tool_paths;

        Ok(s)
    }

    pub fn timeout_for(&self, category: JobCategory) -> Duration {
        let minutes = self
            .timeouts
            .get(&category)
            .copied()
            .unwrap_or_else(|| default_timeout_minutes(category));
        Duration::from_secs(u64::from(minutes) * 60)
    }
}

/// Settings shared between commands, the tool registry and the job queue.
#[derive(Debug, Default)]
pub struct SettingsState {
    current: RwLock<Settings>,
}

impl SettingsState {
    /// Loads settings from the store. Invalid or missing data falls back to defaults.
    pub fn load<R: Runtime>(app: &AppHandle<R>) -> AppResult<Self> {
        let store = app.store(STORE_FILE)?;
        let settings = store
            .get(STORE_KEY)
            .and_then(|value| serde_json::from_value::<Settings>(value).ok())
            .map(|s| {
                s.without_missing_paths().validated().unwrap_or_else(|err| {
                    eprintln!("ignoring invalid saved settings: {err}");
                    Settings::default()
                })
            })
            .unwrap_or_default();
        Ok(Self::new(settings))
    }

    pub fn new(settings: Settings) -> Self {
        Self {
            current: RwLock::new(settings),
        }
    }

    pub fn get(&self) -> Settings {
        match self.current.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Validates, persists and applies new settings. Returns the stored version.
    pub fn update<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        settings: Settings,
    ) -> AppResult<Settings> {
        let settings = settings.validated()?;
        let value =
            serde_json::to_value(&settings).map_err(|err| AppError::Settings(err.to_string()))?;
        let store = app.store(STORE_FILE)?;
        store.set(STORE_KEY, value);
        store.save()?;
        match self.current.write() {
            Ok(mut guard) => *guard = settings.clone(),
            Err(poisoned) => *poisoned.into_inner() = settings.clone(),
        }
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let settings = Settings::default().validated().unwrap();
        assert!(settings.concurrency >= 1);
        assert_eq!(settings.video_concurrency, 1);
        assert_eq!(settings.timeouts.len(), JobCategory::ALL.len());
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let settings: Settings =
            serde_json::from_str(r#"{ "outputSuffix": "converted" }"#).unwrap();
        let settings = settings.validated().unwrap();
        assert_eq!(settings.output_suffix, "converted");
        assert_eq!(settings.output, OutputLocation::SameFolder);
        assert_eq!(
            settings.timeout_for(JobCategory::Image),
            Duration::from_secs(600)
        );
    }

    #[test]
    fn rejects_out_of_range_values() {
        let cases = [
            Settings {
                concurrency: 0,
                ..Settings::default()
            },
            Settings {
                concurrency: MAX_CONCURRENCY + 1,
                ..Settings::default()
            },
            Settings {
                video_concurrency: 0,
                ..Settings::default()
            },
            Settings {
                output_suffix: "../x".into(),
                ..Settings::default()
            },
            Settings {
                output: OutputLocation::Folder {
                    path: PathBuf::from("relative"),
                },
                ..Settings::default()
            },
            Settings {
                timeouts: BTreeMap::from([(JobCategory::Image, 0)]),
                ..Settings::default()
            },
            Settings {
                tool_paths: BTreeMap::from([(ToolId::Pandoc, PathBuf::from("/does/not/exist"))]),
                ..Settings::default()
            },
        ];
        for case in cases {
            assert!(
                case.clone().validated().is_err(),
                "{case:?} should be rejected"
            );
        }
    }

    #[test]
    fn output_location_uses_mode_tag() {
        let json = serde_json::to_value(OutputLocation::Folder {
            path: "/tmp".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "mode": "folder", "path": "/tmp" })
        );
    }
}
