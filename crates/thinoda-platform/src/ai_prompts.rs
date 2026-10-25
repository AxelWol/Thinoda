//! Bounded prompt preferences, saved without reading or changing credentials.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{Error, Paths, Result, Settings, fs::atomic_write};

pub const MAX_PROMPTS: usize = 32;
pub const MAX_NAME_BYTES: usize = 80;
pub const MAX_TEXT_BYTES: usize = 8192;
pub const MAX_LIBRARY_BYTES: usize = 131_072;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPrompt {
    pub id: String,
    pub name: String,
    pub text: String,
}

pub fn validate(prompts: &[SavedPrompt]) -> std::result::Result<(), String> {
    if prompts.len() > MAX_PROMPTS {
        return Err("Save at most 32 prompts.".into());
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut bytes = 0;
    for prompt in prompts {
        if prompt.id.is_empty()
            || prompt.id.len() > 64
            || !prompt
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            || !ids.insert(&prompt.id)
        {
            return Err("Saved prompt IDs must be unique and valid.".into());
        }
        if prompt.name.trim().is_empty() || prompt.name.len() > MAX_NAME_BYTES {
            return Err("Enter a prompt name of at most 80 bytes.".into());
        }
        if !names.insert(prompt.name.trim().to_lowercase()) {
            return Err("Prompt names must be unique.".into());
        }
        if prompt.text.trim().is_empty() || prompt.text.len() > MAX_TEXT_BYTES {
            return Err("Enter a prompt of at most 8192 bytes.".into());
        }
        bytes += prompt.id.len() + prompt.name.len() + prompt.text.len();
        if bytes > MAX_LIBRARY_BYTES {
            return Err("Saved prompts exceed the 128 KiB library limit.".into());
        }
    }
    Ok(())
}

fn read(paths: &Paths) -> Result<(toml::Value, Vec<SavedPrompt>)> {
    let path = paths.config_dir.join("settings.toml");
    let value = match crate::fs::read(&path) {
        Ok(bytes) => {
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| Error::InvalidAiPrompts("Settings are not valid UTF-8.".into()))?;
            toml::from_str::<toml::Value>(text)
                .map_err(|_| Error::InvalidAiPrompts("Could not parse existing settings.".into()))?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            toml::Value::try_from(Settings::default())?
        }
        Err(error) => return Err(error.into()),
    };
    if value.get("ai").and_then(|ai| ai.get("api_key")).is_some() {
        return Err(Error::InvalidAiPrompts(
            "Secure legacy credentials in Settings before saving prompts.".into(),
        ));
    }
    let settings: Settings = value
        .clone()
        .try_into()
        .map_err(|_| Error::InvalidAiPrompts("Could not parse existing settings.".into()))?;
    validate(&settings.ai.saved_prompts).map_err(Error::InvalidAiPrompts)?;
    Ok((value, settings.ai.saved_prompts))
}

/// Read the current library without consulting the credential store.
pub fn load(paths: &Paths) -> Result<Vec<SavedPrompt>> {
    read(paths).map(|(_, prompts)| prompts)
}

/// Replace only this preference in fresh settings; reject an observed library conflict.
pub fn save(paths: &Paths, expected: &[SavedPrompt], prompts: &[SavedPrompt]) -> Result<()> {
    validate(prompts).map_err(Error::InvalidAiPrompts)?;
    let (mut value, current) = read(paths)?;
    if current != expected {
        return Err(Error::InvalidAiPrompts(
            "Saved prompts changed elsewhere. Cancel and reload prompts.".into(),
        ));
    }
    let table = value
        .as_table_mut()
        .ok_or_else(|| Error::InvalidAiPrompts("Could not parse existing settings.".into()))?;
    let ai = table
        .entry("ai")
        .or_insert_with(|| toml::Value::Table(Default::default()));
    let ai = ai
        .as_table_mut()
        .ok_or_else(|| Error::InvalidAiPrompts("Could not parse existing AI settings.".into()))?;
    ai.insert("saved_prompts".into(), toml::Value::try_from(prompts)?);
    atomic_write(
        paths.config_dir.join("settings.toml"),
        toml::to_string_pretty(&value)?.as_bytes(),
        false,
    )?;
    Ok(())
}
