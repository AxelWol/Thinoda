//! Versioned, opt-in keyboard profiles. Legacy shortcut strings remain separate.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const STANDARD: &str = "thinoda-standard";
pub const XMIND: &str = "xmind-compatible";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeymapPreset {
    #[default]
    Standard,
    Xmind,
}

/// `command` means Cmd on macOS and Ctrl elsewhere; `ctrl` is physical Control.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBinding {
    pub key: String,
    pub command: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeymapProfile {
    pub id: String,
    pub name: String,
    pub preset: KeymapPreset,
    /// Missing entries inherit the preset; an empty list explicitly unassigns.
    #[serde(default)]
    pub bindings: BTreeMap<String, Vec<KeyBinding>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keymaps {
    pub version: u32,
    pub active: String,
    pub profiles: Vec<KeymapProfile>,
}

impl Default for Keymaps {
    fn default() -> Self {
        Self {
            version: 1,
            active: STANDARD.into(),
            profiles: Vec::new(),
        }
    }
}

impl Keymaps {
    pub fn profile(&self) -> Option<&KeymapProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.id == self.active)
    }

    pub fn preset(&self) -> KeymapPreset {
        if self.active == XMIND {
            KeymapPreset::Xmind
        } else {
            self.profile().map_or(KeymapPreset::Standard, |p| p.preset)
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "Unsupported keyboard profile version: {}",
                self.version
            ));
        }
        if self.profiles.len() > 32 {
            return Err("At most 32 keyboard profiles are supported".into());
        }
        let mut ids = BTreeSet::from([STANDARD, XMIND]);
        let mut names =
            BTreeSet::from(["thinoda standard".to_owned(), "xmind-compatible".to_owned()]);
        for profile in &self.profiles {
            if profile.id.trim().is_empty() || profile.id.len() > 128 || !ids.insert(&profile.id) {
                return Err("Keyboard profile IDs must be unique and nonempty".into());
            }
            let name = profile.name.trim();
            if name.is_empty() || name.chars().count() > 80 || !names.insert(name.to_lowercase()) {
                return Err("Keyboard profile names must be unique and 1-80 characters".into());
            }
            if profile.bindings.len() > 512 {
                return Err("Too many keyboard bindings".into());
            }
            for (command, bindings) in &profile.bindings {
                if command.is_empty() || command.len() > 128 || bindings.len() > 8 {
                    return Err("Invalid keyboard command or too many aliases".into());
                }
                for binding in bindings {
                    if binding.key.is_empty() || binding.key.len() > 32 {
                        return Err("Invalid keyboard key name".into());
                    }
                }
            }
        }
        if !ids.contains(self.active.as_str()) {
            return Err("The active keyboard profile does not exist".into());
        }
        Ok(())
    }

    /// Duplicate even built-in profiles without editing their immutable defaults.
    pub fn duplicate_active(&mut self, name: &str) -> Result<(), String> {
        let profile = KeymapProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().into(),
            preset: self.preset(),
            bindings: self
                .profile()
                .map(|p| p.bindings.clone())
                .unwrap_or_default(),
        };
        let previous = std::mem::replace(&mut self.active, profile.id.clone());
        self.profiles.push(profile);
        if let Err(error) = self.validate() {
            self.profiles.pop();
            self.active = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn rename_active(&mut self, name: &str) -> Result<(), String> {
        let index = self
            .profiles
            .iter()
            .position(|p| p.id == self.active)
            .ok_or("Duplicate a built-in profile before editing it")?;
        let old = std::mem::replace(&mut self.profiles[index].name, name.trim().into());
        if let Err(error) = self.validate() {
            self.profiles[index].name = old;
            return Err(error);
        }
        Ok(())
    }

    pub fn delete_active(&mut self) -> Result<(), String> {
        let index = self
            .profiles
            .iter()
            .position(|p| p.id == self.active)
            .ok_or("Built-in keyboard profiles cannot be deleted")?;
        self.profiles.remove(index);
        self.active = STANDARD.into();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_roundtrip_and_explicit_unassignment_survives() {
        let mut maps = Keymaps::default();
        maps.duplicate_active("Writing").unwrap();
        maps.profiles[0]
            .bindings
            .insert("file.save".into(), Vec::new());
        let encoded = toml::to_string(&maps).unwrap();
        let decoded: Keymaps = toml::from_str(&encoded).unwrap();
        assert_eq!(maps, decoded);
        assert_eq!(decoded.profile().unwrap().bindings["file.save"], vec![]);
        maps.rename_active("Planning").unwrap();
        assert_eq!(maps.profile().unwrap().name, "Planning");
        maps.duplicate_active("Second").unwrap();
        assert!(maps.rename_active(" planning ").is_err());
        assert_eq!(maps.profile().unwrap().name, "Second");
        maps.delete_active().unwrap();
        assert_eq!(maps.active, STANDARD);
        assert!(maps.delete_active().is_err());
    }

    #[test]
    fn invalid_version_identity_and_failed_duplicates_do_not_activate() {
        let mut maps = Keymaps::default();
        assert!(maps.duplicate_active(" ").is_err());
        assert_eq!(maps, Keymaps::default());
        maps.active = "missing".into();
        assert!(maps.validate().is_err());
        maps.active = XMIND.into();
        assert_eq!(maps.preset(), KeymapPreset::Xmind);
        maps.version = 2;
        assert!(maps.validate().is_err());
    }

    #[test]
    fn old_settings_preserve_inactive_override_strings_without_migration() {
        let settings: crate::Settings = toml::from_str(
            "[shortcuts_overrides]\n\"file.save\" = \"Alt+S\"\n\"12\" = \"Ctrl+N\"\n",
        )
        .unwrap();
        assert_eq!(settings.keymaps, Keymaps::default());
        assert_eq!(settings.shortcuts_overrides["file.save"], "Alt+S");
        assert_eq!(settings.shortcuts_overrides["12"], "Ctrl+N");
        let encoded = toml::to_string(&settings).unwrap();
        assert_eq!(
            toml::from_str::<crate::Settings>(&encoded).unwrap(),
            settings
        );
    }
}
