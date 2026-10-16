//! Best-effort discovery of local sync folders; no network or provider SDKs.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Recognized local synchronization providers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudProvider {
    OneDrive,
    ICloud,
    Dropbox,
    GoogleDrive,
    Box,
    Nextcloud,
    OwnCloud,
}

/// An existing provider-managed directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudFolder {
    pub provider: CloudProvider,
    pub name: String,
    pub path: PathBuf,
}

/// OS-specific folder conventions, independently selectable in tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudOs {
    Windows,
    MacOs,
    Linux,
}
impl CloudOs {
    /// The currently compiled operating system (other Unix systems use Linux conventions).
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// Injectable discovery inputs. The core detector never reads global environment
/// variables. `drive_roots` allows testing Windows mounted-drive discovery without
/// touching real drive letters; `new` leaves it empty, `current` fills D through Z.
#[derive(Clone, Debug)]
pub struct CloudEnv {
    pub home: PathBuf,
    pub vars: BTreeMap<String, OsString>,
    pub os: CloudOs,
    pub drive_roots: Vec<PathBuf>,
}
impl CloudEnv {
    /// Build isolated discovery inputs with no environment or mounted drives.
    pub fn new(home: impl Into<PathBuf>, os: CloudOs) -> Self {
        Self {
            home: home.into(),
            vars: BTreeMap::new(),
            os,
            drive_roots: Vec::new(),
        }
    }

    /// Capture actual home, environment, and platform drive roots.
    pub fn current() -> Option<Self> {
        let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
        let os = CloudOs::current();
        let vars = std::env::vars_os()
            .filter_map(|(k, v)| k.into_string().ok().map(|k| (k, v)))
            .collect();
        let drive_roots = if os == CloudOs::Windows {
            ('D'..='Z')
                .map(|c| PathBuf::from(format!("{c}:\\")))
                .collect()
        } else {
            Vec::new()
        };
        Some(Self {
            home,
            vars,
            os,
            drive_roots,
        })
    }

    fn var(&self, name: &str) -> Option<&OsString> {
        self.vars
            .iter()
            .find(|(k, v)| {
                !v.is_empty()
                    && (k.as_str() == name
                        || (self.os == CloudOs::Windows && k.eq_ignore_ascii_case(name)))
            })
            .map(|(_, v)| v)
    }
}

/// Discover currently installed providers, returning only existing directories.
pub fn detect_cloud_folders() -> Vec<CloudFolder> {
    CloudEnv::current()
        .map(|env| detect_cloud_folders_with(&env))
        .unwrap_or_default()
}

/// Discover providers using only injected inputs and local directory metadata.
pub fn detect_cloud_folders_with(env: &CloudEnv) -> Vec<CloudFolder> {
    let mut folders = Vec::new();
    let mut add = |provider, name: String, path: PathBuf| {
        if path.is_dir()
            && !folders
                .iter()
                .any(|f: &CloudFolder| normalized(&f.path, env.os) == normalized(&path, env.os))
        {
            folders.push(CloudFolder {
                provider,
                name,
                path,
            });
        }
    };
    match env.os {
        CloudOs::Windows => {
            for (var, name) in [
                ("OneDrive", "OneDrive"),
                ("OneDriveCommercial", "OneDrive (business)"),
                ("OneDriveConsumer", "OneDrive (personal)"),
            ] {
                if let Some(path) = env.var(var) {
                    add(CloudProvider::OneDrive, name.into(), PathBuf::from(path));
                }
            }
            add(
                CloudProvider::ICloud,
                "iCloud Drive".into(),
                env.home.join("iCloudDrive"),
            );
            for root in &env.drive_roots {
                for name in ["My Drive", "Meine Ablage"] {
                    add(
                        CloudProvider::GoogleDrive,
                        "Google Drive".into(),
                        root.join(name),
                    );
                }
            }
            add(
                CloudProvider::GoogleDrive,
                "Google Drive".into(),
                env.home.join("Google Drive"),
            );
        }
        CloudOs::MacOs => {
            let storage = env.home.join("Library").join("CloudStorage");
            if let Ok(entries) = std::fs::read_dir(&storage) {
                let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
                entries.sort();
                for path in entries {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    if name.starts_with("OneDrive-") {
                        add(CloudProvider::OneDrive, name.into_owned(), path);
                    } else if name.starts_with("GoogleDrive-") {
                        add(CloudProvider::GoogleDrive, name.into_owned(), path);
                    }
                }
            }
            add(
                CloudProvider::ICloud,
                "iCloud Drive".into(),
                env.home
                    .join("Library")
                    .join("Mobile Documents")
                    .join("com~apple~CloudDocs"),
            );
            add(CloudProvider::Box, "Box".into(), storage.join("Box-Box"));
        }
        CloudOs::Linux => {
            add(
                CloudProvider::OneDrive,
                "OneDrive".into(),
                env.home.join("OneDrive"),
            );
        }
    }
    let mut dropbox_configs = vec![env.home.join(".dropbox").join("info.json")];
    if env.os == CloudOs::Windows {
        for name in ["APPDATA", "LOCALAPPDATA"] {
            if let Some(root) = env.var(name) {
                dropbox_configs.push(PathBuf::from(root).join("Dropbox").join("info.json"));
            }
        }
    }
    for config in dropbox_configs {
        if let Ok(bytes) = std::fs::read(config)
            && let Ok(serde_json::Value::Object(accounts)) = serde_json::from_slice(&bytes)
        {
            for (account, details) in accounts {
                if let Some(path) = details.get("path").and_then(|v| v.as_str()) {
                    add(
                        CloudProvider::Dropbox,
                        format!("Dropbox ({account})"),
                        PathBuf::from(path),
                    );
                }
            }
        }
    }
    for (provider, name) in [
        (CloudProvider::Dropbox, "Dropbox"),
        (CloudProvider::Box, "Box"),
        (CloudProvider::Nextcloud, "Nextcloud"),
        (CloudProvider::OwnCloud, "ownCloud"),
    ] {
        add(provider, name.into(), env.home.join(name));
    }
    folders
}

/// Return the most specific containing sync folder using this OS's case rules.
pub fn provider_for_path<'a>(folders: &'a [CloudFolder], path: &Path) -> Option<&'a CloudFolder> {
    provider_for_path_on_os(folders, path, CloudOs::current())
}

/// Testable variant of `provider_for_path`, including case-insensitive Windows/macOS
/// comparisons and component boundaries (Dropbox never matches Dropbox-other).
pub fn provider_for_path_on_os<'a>(
    folders: &'a [CloudFolder],
    path: &Path,
    os: CloudOs,
) -> Option<&'a CloudFolder> {
    let path = normalized(path, os);
    folders
        .iter()
        .filter_map(|folder| {
            let root = normalized(&folder.path, os);
            (path == root
                || path
                    .strip_prefix(&root)
                    .is_some_and(|suffix| suffix.starts_with('/')))
            .then_some((root.len(), folder))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, folder)| folder)
}

fn normalized(path: &Path, os: CloudOs) -> String {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        }
    });
    let text = path.to_string_lossy();
    let text = text.strip_prefix("\\\\?\\").unwrap_or(&text);
    let text = if os == CloudOs::Windows || cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.to_owned()
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in text.split('/') {
        match part {
            "." => {}
            ".." if parts.last().is_some_and(|p| !p.is_empty()) => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let text = parts.join("/").trim_end_matches('/').to_owned();
    if os == CloudOs::Linux {
        text
    } else {
        text.to_lowercase()
    }
}
