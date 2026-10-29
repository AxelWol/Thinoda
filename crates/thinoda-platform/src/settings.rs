//! Forward-compatible TOML configuration and a bounded recent-files list.

use crate::{
    Error, Paths, Result,
    credentials::{CredentialStore, NativeCredentialStore},
    fs::atomic_write,
    unix_seconds,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
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
    Spanish,
    French,
    Danish,
    Dutch,
    Portuguese,
    PortugueseBrazil,
    Italian,
    Polish,
}

impl Language {
    pub const ALL: [Self; 11] = [
        Self::Auto,
        Self::English,
        Self::German,
        Self::Spanish,
        Self::French,
        Self::Danish,
        Self::Dutch,
        Self::Portuguese,
        Self::PortugueseBrazil,
        Self::Italian,
        Self::Polish,
    ];

    /// Autonyms remain recognizable even when the current UI language is unfamiliar.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::English => "English",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::French => "Français",
            Self::Danish => "Dansk",
            Self::Dutch => "Nederlands",
            Self::Portuguese => "Português (Portugal)",
            Self::PortugueseBrazil => "Português (Brasil)",
            Self::Italian => "Italiano",
            Self::Polish => "Polski",
        }
    }
}

/// Last selected sidebar family, independent of whether the panel is visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidebarFamily {
    Symbols,
    #[default]
    #[serde(other)]
    Format,
}

/// Last selected formatting tab, retained while the Symbols family is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatTab {
    Map,
    Data,
    #[default]
    #[serde(other)]
    Style,
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

/// Response-depth guidance, independent of provider-specific reasoning APIs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiResponseMode {
    Quick,
    #[default]
    Balanced,
    Deep,
}

/// Generation harness. Existing settings migrate to the OpenAI-compatible API.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    #[default]
    OpenAiApi,
    CopilotCli,
    ClaudeCli,
    CodexCli,
}

impl AiProvider {
    pub const ALL: [Self; 4] = [
        Self::OpenAiApi,
        Self::CopilotCli,
        Self::ClaudeCli,
        Self::CodexCli,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::OpenAiApi => "OpenAI API",
            Self::CopilotCli => "GitHub Copilot CLI",
            Self::ClaudeCli => "Claude CLI",
            Self::CodexCli => "OpenAI Codex CLI",
        }
    }

    pub fn cli_index(self) -> Option<usize> {
        match self {
            Self::OpenAiApi => None,
            Self::CopilotCli => Some(0),
            Self::ClaudeCli => Some(1),
            Self::CodexCli => Some(2),
        }
    }
}

/// AI assistant preferences. Embeddings always use the OpenAI-compatible API.
/// API keys are stored in the OS credential store; the environment variable remains an override.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub enabled: bool,
    pub provider: AiProvider,
    /// Separate optional model overrides, preserving API and other CLI choices.
    pub cli_models: [String; 3],
    pub base_url: String,
    /// Empty = first chat model offered by the server.
    pub model: String,
    /// Kept in memory only; `settings.toml` must never contain this value.
    #[serde(skip)]
    pub api_key: String,
    /// Non-secret marker indicating that an API key should exist in the OS credential store.
    pub api_key_stored: bool,
    /// False when a legacy key could not be secured or the native store could not be read.
    #[serde(skip, default = "credentials_ready")]
    pub credentials_ready: bool,
    /// Requests a credential-store update on the next successful settings save.
    #[serde(skip)]
    pub api_key_dirty: bool,
    pub temperature: f32,
    pub response_mode: AiResponseMode,
    pub saved_prompts: Vec<crate::ai_prompts::SavedPrompt>,
    /// Explicit embedding model; never inherits the chat model.
    pub embedding_model: String,
}

impl fmt::Debug for AiSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiSettings")
            .field("enabled", &self.enabled)
            .field("provider", &self.provider)
            .field("cli_models", &self.cli_models)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"<redacted>")
            .field("api_key_stored", &self.api_key_stored)
            .field("credentials_ready", &self.credentials_ready)
            .field("temperature", &self.temperature)
            .field("response_mode", &self.response_mode)
            .field("embedding_model", &self.embedding_model)
            .finish()
    }
}

fn credentials_ready() -> bool {
    true
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: AiProvider::default(),
            cli_models: Default::default(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: String::new(),
            api_key: String::new(),
            api_key_stored: false,
            credentials_ready: true,
            api_key_dirty: false,
            temperature: 0.7,
            response_mode: AiResponseMode::Balanced,
            saved_prompts: Vec::new(),
            embedding_model: String::new(),
        }
    }
}

impl AiSettings {
    /// Replace the API key and mark it for secure persistence.
    pub fn set_api_key(&mut self, api_key: impl Into<String>) {
        self.api_key = api_key.into();
        self.api_key_dirty = true;
    }
}

/// Visible high-frequency commands above the desktop ribbon.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickAccessSettings {
    pub save: bool,
    pub undo: bool,
    pub redo: bool,
    pub print: bool,
}

impl Default for QuickAccessSettings {
    fn default() -> Self {
        Self {
            save: true,
            undo: true,
            redo: true,
            print: true,
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
    pub recent_templates: Vec<String>,
    pub quick_styles: Vec<thinoda_core::quick_styles::QuickStyle>,
    pub custom_themes: Vec<thinoda_core::custom_theme::CustomTheme>,
    pub default_structure: String,
    pub default_theme: String,
    pub default_custom_theme: Option<thinoda_core::custom_theme::CustomTheme>,
    pub ui_scale: f32,
    pub show_status_bar: bool,
    pub ribbon_collapsed: bool,
    /// Compact ribbon sizing (default). Persisted under a new key so earlier
    /// saved `compact_ribbon = false` values start compact once; Large is opt-in.
    #[serde(rename = "ribbon_compact")]
    pub compact_ribbon: bool,
    pub quick_access: QuickAccessSettings,
    pub show_format_panel: bool,
    pub sidebar_family: SidebarFamily,
    pub format_tab: FormatTab,
    pub show_topic_toolbar: bool,
    pub show_topic_add_buttons: bool,
    /// Visual free-placement alignment feedback; does not snap or change positions.
    pub show_alignment_guides: bool,
    /// Optional installed fallback face; never replaces a document's configured font.
    pub default_cjk_font: Option<String>,
    /// Sort label drafts by Unicode code-point order; disabled to preserve insertion order.
    pub label_auto_sort: bool,
    /// Initial canvas zoom for newly created/opened documents, independent of UI scaling.
    pub initial_canvas_zoom: f32,
    pub reuse_last_structure: bool,
    pub reuse_last_theme: bool,
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
    /// Preserved but inactive. Stable app shortcut IDs use names such as
    /// `file.save`; legacy/numeric/unknown keys are not guessed or migrated.
    pub shortcuts_overrides: BTreeMap<String, String>,
    pub keymaps: crate::keymaps::Keymaps,
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
            recent_templates: Vec::new(),
            quick_styles: Vec::new(),
            custom_themes: Vec::new(),
            default_structure: "mind_map".into(),
            default_theme: "default".into(),
            default_custom_theme: None,
            ui_scale: 1.0,
            show_status_bar: true,
            ribbon_collapsed: false,
            compact_ribbon: true,
            quick_access: QuickAccessSettings::default(),
            show_format_panel: true,
            sidebar_family: SidebarFamily::default(),
            format_tab: FormatTab::default(),
            show_topic_toolbar: true,
            show_topic_add_buttons: true,
            show_alignment_guides: false,
            default_cjk_font: None,
            label_auto_sort: false,
            initial_canvas_zoom: 1.0,
            reuse_last_structure: false,
            reuse_last_theme: false,
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
            keymaps: crate::keymaps::Keymaps::default(),
        }
    }
}

/// Load result containing a user-visible warning when broken settings were preserved.
#[derive(Clone, Debug)]
pub struct SettingsLoad {
    pub settings: Settings,
    pub warning: Option<String>,
    /// Errors that make a simple `Settings::load` call unsafe to treat as complete.
    pub credential_error: Option<String>,
}

impl Settings {
    pub fn validate_custom_themes(
        themes: &[thinoda_core::custom_theme::CustomTheme],
    ) -> std::result::Result<(), String> {
        if themes.len() > 32 {
            return Err("at most 32 saved custom themes are supported".into());
        }
        let mut names = std::collections::HashSet::new();
        for theme in themes {
            theme.validate()?;
            if !names.insert(theme.name.trim().to_lowercase()) {
                return Err("custom theme names must be unique".into());
            }
        }
        Ok(())
    }

    pub fn validate_quick_styles(
        styles: &[thinoda_core::quick_styles::QuickStyle],
    ) -> std::result::Result<(), String> {
        if styles.len() > 32 {
            return Err("at most 32 saved quick styles are supported".into());
        }
        let mut names = std::collections::HashSet::new();
        for style in styles {
            style.validate()?;
            if !names.insert(style.name.trim().to_lowercase()) {
                return Err("quick style names must be unique".into());
            }
        }
        Ok(())
    }

    /// Load preferences, logging any broken-file warning. I/O errors remain errors.
    pub fn load(paths: &Paths) -> Result<Self> {
        let loaded = Self::load_with_warning(paths)?;
        if let Some(error) = loaded.credential_error {
            return Err(Error::Credential(error));
        }
        Ok(loaded.settings)
    }

    /// Load preferences and return a warning suitable for a notification banner.
    pub fn load_with_warning(paths: &Paths) -> Result<SettingsLoad> {
        Self::load_with_credentials(paths, &NativeCredentialStore)
    }

    /// Load preferences using an injected credential store (also used for deterministic tests).
    pub fn load_with_credentials(
        paths: &Paths,
        credentials: &dyn CredentialStore,
    ) -> Result<SettingsLoad> {
        let path = paths.config_dir.join("settings.toml");
        let bytes = match crate::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(SettingsLoad {
                    settings: Self::default(),
                    warning: None,
                    credential_error: None,
                });
            }
            Err(e) => return Err(e.into()),
        };
        let parsed = std::str::from_utf8(&bytes)
            .map_err(|e| e.to_string())
            .and_then(|text| toml::from_str::<toml::Value>(text).map_err(|e| e.to_string()))
            .and_then(|mut value| {
                let legacy_api_key = value
                    .get_mut("ai")
                    .and_then(toml::Value::as_table_mut)
                    .and_then(|ai| ai.remove("api_key"));
                let legacy_api_key = match legacy_api_key {
                    Some(value) => Some(
                        value
                            .as_str()
                            .ok_or_else(|| "AI API key must be a string".to_string())?
                            .to_owned(),
                    ),
                    None => None,
                };
                let settings = value
                    .try_into::<Self>()
                    .map_err(|e: toml::de::Error| e.to_string())?;
                Self::validate_quick_styles(&settings.quick_styles)?;
                crate::ai_prompts::validate(&settings.ai.saved_prompts)?;
                Self::validate_custom_themes(&settings.custom_themes)?;
                settings.keymaps.validate()?;
                if let Some(theme) = &settings.default_custom_theme {
                    theme.validate()?;
                }
                Ok((settings, legacy_api_key))
            });
        match parsed {
            Ok((mut settings, legacy_api_key)) => {
                settings.normalize();
                let mut warning = None;
                let mut credential_error = None;
                if let Some(legacy_api_key) = legacy_api_key {
                    if legacy_api_key.trim().is_empty() {
                        if let Err(error) = Self::write_settings(paths, &settings) {
                            warning = Some(format!(
                                "Could not remove the empty legacy AI API key from settings: {error}"
                            ));
                        }
                    } else {
                        settings.ai.api_key = legacy_api_key;
                        match credentials.set_password(settings.ai.api_key.trim()) {
                            Ok(()) => {
                                settings.ai.api_key = settings.ai.api_key.trim().to_owned();
                                settings.ai.api_key_stored = true;
                                settings.ai.credentials_ready = true;
                                if let Err(error) = Self::write_settings(paths, &settings) {
                                    let message = format!(
                                        "The AI API key is secured, but settings.toml could not be updated to remove its legacy copy: {error}"
                                    );
                                    warning = Some(message.clone());
                                    credential_error = Some(message);
                                }
                            }
                            Err(error) => {
                                settings.ai.credentials_ready = false;
                                let message = format!(
                                    "Could not move the legacy AI API key to secure storage: {error}. The key remains in settings.toml and AI requests are disabled until it can be secured."
                                );
                                warning = Some(message.clone());
                                credential_error = Some(message);
                            }
                        }
                    }
                } else if settings.ai.api_key_stored {
                    match credentials.get_password() {
                        Ok(Some(api_key)) => {
                            settings.ai.api_key = api_key;
                            settings.ai.credentials_ready = true;
                        }
                        // Browser keys live only for one session (see credentials.rs).
                        Ok(None) if cfg!(target_arch = "wasm32") => {
                            settings.ai.api_key_stored = false;
                            settings.ai.credentials_ready = true;
                        }
                        Ok(None) => {
                            settings.ai.credentials_ready = false;
                            let message = "The saved AI API key is missing from secure storage. Save a replacement key or clear the saved key in Settings › AI.".to_owned();
                            warning = Some(message.clone());
                            credential_error = Some(message);
                        }
                        Err(error) => {
                            settings.ai.credentials_ready = false;
                            let message = format!(
                                "Could not access secure AI credential storage: {error}. AI requests are disabled until storage is available."
                            );
                            warning = Some(message.clone());
                            credential_error = Some(message);
                        }
                    }
                }
                Ok(SettingsLoad {
                    settings,
                    warning,
                    credential_error,
                })
            }
            Err(error) => {
                let mut broken = paths
                    .config_dir
                    .join(format!("settings.toml.broken-{}", unix_seconds()));
                if crate::fs::exists(&broken) {
                    broken = paths.config_dir.join(format!(
                        "settings.toml.broken-{}-{}",
                        unix_seconds(),
                        uuid::Uuid::new_v4()
                    ));
                }
                crate::fs::rename(&path, &broken)?;
                let warning = format!(
                    "Could not read settings; defaults restored. Original preserved at {}: {error}",
                    broken.display()
                );
                log::warn!("{warning}");
                Ok(SettingsLoad {
                    settings: Self::default(),
                    warning: Some(warning),
                    credential_error: None,
                })
            }
        }
    }

    /// Atomically persist preferences, normalizing recents and invalid numeric values.
    pub fn save(&self, paths: &Paths) -> Result<()> {
        let mut settings = self.clone();
        settings.save_with_credentials(paths, &NativeCredentialStore)
    }

    /// Store or clear the API key before atomically writing settings without secret values.
    pub fn save_with_credentials(
        &mut self,
        paths: &Paths,
        credentials: &dyn CredentialStore,
    ) -> Result<()> {
        let mut saved = self.clone();
        crate::ai_prompts::validate(&saved.ai.saved_prompts).map_err(Error::InvalidAiPrompts)?;
        saved
            .keymaps
            .validate()
            .map_err(crate::Error::InvalidKeymaps)?;
        Self::validate_custom_themes(&saved.custom_themes)
            .map_err(crate::Error::InvalidCustomThemes)?;
        if let Some(theme) = &saved.default_custom_theme {
            theme
                .validate()
                .map_err(crate::Error::InvalidCustomThemes)?;
        }
        Self::validate_quick_styles(&saved.quick_styles)
            .map_err(crate::Error::InvalidQuickStyles)?;
        let api_key = saved.ai.api_key.trim();
        if !api_key.is_empty() {
            if saved.ai.api_key_dirty || !saved.ai.api_key_stored || !saved.ai.credentials_ready {
                credentials
                    .set_password(api_key)
                    .map_err(Error::Credential)?;
            }
            saved.ai.api_key = api_key.to_owned();
            saved.ai.api_key_stored = true;
            saved.ai.credentials_ready = true;
            saved.ai.api_key_dirty = false;
        } else if saved.ai.api_key_dirty || (saved.ai.api_key_stored && saved.ai.credentials_ready)
        {
            credentials.delete_password().map_err(Error::Credential)?;
            saved.ai.api_key_stored = false;
            saved.ai.credentials_ready = true;
            saved.ai.api_key_dirty = false;
        }
        Self::write_settings(paths, &saved)?;
        *self = saved;
        Ok(())
    }

    fn write_settings(paths: &Paths, settings: &Self) -> Result<()> {
        crate::ai_prompts::validate(&settings.ai.saved_prompts).map_err(Error::InvalidAiPrompts)?;
        settings
            .keymaps
            .validate()
            .map_err(crate::Error::InvalidKeymaps)?;
        Self::validate_custom_themes(&settings.custom_themes)
            .map_err(crate::Error::InvalidCustomThemes)?;
        if let Some(theme) = &settings.default_custom_theme {
            theme
                .validate()
                .map_err(crate::Error::InvalidCustomThemes)?;
        }
        Self::validate_quick_styles(&settings.quick_styles)
            .map_err(crate::Error::InvalidQuickStyles)?;
        let mut normalized = settings.clone();
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
        let mut seen = std::collections::HashSet::new();
        self.recent_templates.retain(|id| {
            let known = thinoda_core::templates::TEMPLATES
                .iter()
                .any(|template| template.id == id);
            if !known {
                log::warn!("Ignoring unknown recent template ID: {id}");
            }
            known && seen.insert(id.clone())
        });
        self.recent_templates.truncate(8);
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
        self.initial_canvas_zoom = if self.initial_canvas_zoom.is_finite() {
            self.initial_canvas_zoom.clamp(0.1, 5.0)
        } else {
            1.0
        };
    }

    /// Remember the most recently used built-in templates, independently of files.
    pub fn add_recent_template(&mut self, id: &str) {
        self.recent_templates.retain(|previous| previous != id);
        self.recent_templates.insert(0, id.to_string());
        self.normalize();
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
            .filter(|r| crate::fs::is_file(&r.path))
            .collect()
    }
}

fn path_key(path: &Path) -> String {
    let normalized = crate::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let value = normalized.to_string_lossy().into_owned();
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        value.to_lowercase()
    } else {
        value
    }
}
