//! Forward-compatible TOML configuration and a bounded recent-files list.

use crate::{Paths, Result, fs::atomic_write, unix_seconds};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Requested application appearance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

/// Requested UI language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    Auto,
    English,
    German,
}

/// A recent document, optionally pinned above ordinary recents.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecentFile {
    pub path: PathBuf,
    pub opened_at: u64,
    pub pinned: bool,
}

/// Last window geometry, in logical pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    pub width: f32,
    pub height: f32,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub maximized: bool,
}
impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 800.0,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

/// Local cloud-folder preference; no credentials are stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudSettings {
    pub preferred_folder: Option<PathBuf>,
    pub show_cloud_badges: bool,
}
impl Default for CloudSettings {
    fn default() -> Self {
        Self {
            preferred_folder: None,
            show_cloud_badges: true,
        }
    }
}

/// AI assistant preferences (any OpenAI-compatible server such as LM Studio or Ollama).
/// The API key may instead be supplied via the `YMIND_AI_API_KEY` environment variable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub enabled: bool,
    pub base_url: String,
    /// Empty = first chat model offered by the server.
    pub model: String,
    pub api_key: String,
    pub temperature: f32,
}
impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: String::new(),
            api_key: String::new(),
            temperature: 0.7,
        }
    }
}

/// Persisted preferences. Unknown fields are ignored and missing fields default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub language: Language,
    /// Only autosave documents that already have an original file path.
    pub autosave: bool,
    pub autosave_interval_secs: u64,
    pub recovery_interval_secs: u64,
    pub backup_on_save: bool,
    pub recent_files: Vec<RecentFile>,
    pub default_structure: String,
    pub default_theme: String,
    pub ui_scale: f32,
    pub show_status_bar: bool,
    pub show_format_panel: bool,
    pub snap_to_grid: bool,
    pub smooth_zoom: bool,
    pub spell_check: bool,
    pub check_for_updates: bool,
    pub confirm_on_delete: bool,
    pub window: Option<WindowState>,
    pub last_open_dir: Option<PathBuf>,
    pub last_export_format: Option<String>,
    pub cloud: CloudSettings,
    pub ai: AiSettings,
    pub shortcuts_overrides: BTreeMap<String, String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            language: Language::Auto,
            autosave: true,
            autosave_interval_secs: 30,
            recovery_interval_secs: 1,
            backup_on_save: true,
            recent_files: Vec::new(),
            default_structure: "mind_map".into(),
            default_theme: "default".into(),
            ui_scale: 1.0,
            show_status_bar: true,
            show_format_panel: true,
            snap_to_grid: false,
            smooth_zoom: true,
            spell_check: false,
            check_for_updates: true,
            confirm_on_delete: true,
            window: None,
            last_open_dir: None,
            last_export_format: None,
            cloud: CloudSettings::default(),
            ai: AiSettings::default(),
            shortcuts_overrides: BTreeMap::new(),
        }
    }
}

/// Load result containing a user-visible warning when broken settings were preserved.
#[derive(Clone, Debug)]
pub struct SettingsLoad {
    pub settings: Settings,
    pub warning: Option<String>,
}

impl Settings {
    /// Load preferences, logging any broken-file warning. I/O errors remain errors.
    pub fn load(paths: &Paths) -> Result<Self> {
        Ok(Self::load_with_warning(paths)?.settings)
    }

    /// Load preferences and return a warning suitable for a notification banner.
    pub fn load_with_warning(paths: &Paths) -> Result<SettingsLoad> {
        let path = paths.config_dir.join("settings.toml");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(SettingsLoad {
                    settings: Self::default(),
                    warning: None,
                });
            }
            Err(e) => return Err(e.into()),
        };
        let parsed = std::str::from_utf8(&bytes)
            .map_err(|e| e.to_string())
            .and_then(|text| toml::from_str::<Self>(text).map_err(|e| e.to_string()));
        match parsed {
            Ok(mut settings) => {
                settings.normalize();
                Ok(SettingsLoad {
                    settings,
                    warning: None,
                })
            }
            Err(error) => {
                let mut broken = paths
                    .config_dir
                    .join(format!("settings.toml.broken-{}", unix_seconds()));
                if broken.exists() {
                    broken = paths.config_dir.join(format!(
                        "settings.toml.broken-{}-{}",
                        unix_seconds(),
                        uuid::Uuid::new_v4()
                    ));
                }
                std::fs::rename(&path, &broken)?;
                let warning = format!(
                    "Could not read settings; defaults restored. Original preserved at {}: {error}",
                    broken.display()
                );
                log::warn!("{warning}");
                Ok(SettingsLoad {
                    settings: Self::default(),
                    warning: Some(warning),
                })
            }
        }
    }

    /// Atomically persist preferences, normalizing recents and invalid numeric values.
    pub fn save(&self, paths: &Paths) -> Result<()> {
        let mut normalized = self.clone();
        normalized.normalize();
        let text = toml::to_string_pretty(&normalized)?;
        atomic_write(
            paths.config_dir.join("settings.toml"),
            text.as_bytes(),
            false,
        )?;
        Ok(())
    }

    fn normalize(&mut self) {
        let mut merged: Vec<RecentFile> = Vec::with_capacity(self.recent_files.len());
        let mut indices = std::collections::HashMap::new();
        for recent in self.recent_files.drain(..) {
            let key = path_key(&recent.path);
            if let Some(&index) = indices.get(&key) {
                let previous: &mut RecentFile = &mut merged[index];
                previous.pinned |= recent.pinned;
                if recent.opened_at > previous.opened_at {
                    previous.path = recent.path;
                    previous.opened_at = recent.opened_at;
                }
            } else {
                indices.insert(key, merged.len());
                merged.push(recent);
            }
        }
        merged.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.opened_at.cmp(&a.opened_at)));
        merged.truncate(20);
        self.recent_files = merged;
        self.recovery_interval_secs = self.recovery_interval_secs.max(1);
        self.autosave_interval_secs = self.autosave_interval_secs.max(1);
        if !self.ui_scale.is_finite() || self.ui_scale <= 0.0 {
            self.ui_scale = 1.0;
        }
    }

    /// Add or bump a path without losing its pinned state.
    pub fn add_recent(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        let key = path_key(&path);
        let pinned = self
            .recent_files
            .iter()
            .any(|r| path_key(&r.path) == key && r.pinned);
        self.recent_files.retain(|r| path_key(&r.path) != key);
        self.recent_files.insert(
            0,
            RecentFile {
                path,
                opened_at: unix_seconds(),
                pinned,
            },
        );
        self.normalize();
    }

    /// Pin/unpin a recent path and restore the pinned-first ordering.
    pub fn pin_recent(&mut self, path: impl AsRef<Path>, pinned: bool) {
        let key = path_key(path.as_ref());
        for recent in &mut self.recent_files {
            if path_key(&recent.path) == key {
                recent.pinned = pinned;
            }
        }
        self.normalize();
    }

    /// Remove every matching entry.
    pub fn remove_recent(&mut self, path: impl AsRef<Path>) {
        let key = path_key(path.as_ref());
        self.recent_files.retain(|r| path_key(&r.path) != key);
    }

    /// Clear the list, including pinned entries.
    pub fn clear_recent(&mut self) {
        self.recent_files.clear();
    }

    /// Existing files in current display order; does not silently prune pinned entries.
    pub fn existing_recent(&self) -> Vec<&RecentFile> {
        self.recent_files
            .iter()
            .filter(|r| r.path.is_file())
            .collect()
    }
}

fn path_key(path: &Path) -> String {
    let normalized = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let value = normalized.to_string_lossy().into_owned();
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        value.to_lowercase()
    } else {
        value
    }
}
