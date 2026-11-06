//! Optional, local-only context-menu usage counts.
//!
//! Collection is off by default. Counts use stable, non-localized command IDs
//! scoped by a bounded menu context, are stored only in the profile's config
//! folder, are never uploaded, and never change menu order. Each activation
//! re-reads and merges the file so concurrent windows do not overwrite counts.

use std::{collections::BTreeMap, io};

use serde_json::{Map, Value, json};
use thinoda_platform::Paths;

use crate::{
    app::Cmd,
    context_menus::{MenuContext, usage_command_id},
};

pub const FILE_NAME: &str = "menu_usage.json";
const VERSION: u32 = 1;
const MAX_KEYS: usize = 512;
const MAX_BYTES: usize = 64 * 1024;

/// `context.command-id`, or `None` for commands without a stable identity.
pub fn key(context: MenuContext, command: &Cmd) -> Option<String> {
    usage_command_id(command).map(|id| format!("{}.{id}", context.id()))
}

/// Key for the card-text toolbar entry, which is not a [`Cmd`].
pub fn card_text_key(context: MenuContext) -> String {
    format!("{}.style.card-text", context.id())
}

fn valid_key(key: &str) -> bool {
    let Some((context, id)) = key.split_once('.') else {
        return false;
    };
    MenuContext::ALL.iter().any(|known| known.id() == context)
        && !id.is_empty()
        && key.len() <= 96
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-".contains(&byte))
}

fn path(paths: &Paths) -> std::path::PathBuf {
    paths.config_dir.join(FILE_NAME)
}

/// Reads the stored counts. A missing file is empty; malformed entries are dropped.
pub fn load(paths: &Paths) -> io::Result<BTreeMap<String, u32>> {
    let bytes = match thinoda_platform::fs::read(path(paths)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error),
    };
    if bytes.len() > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "menu usage file is too large",
        ));
    }
    let invalid = |message: &str| io::Error::new(io::ErrorKind::InvalidData, message.to_owned());
    let stored: Value = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if stored.get("version").and_then(Value::as_u64) != Some(u64::from(VERSION)) {
        return Err(invalid("unsupported menu usage version"));
    }
    let counts = stored
        .get("counts")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("menu usage counts are missing"))?;
    Ok(counts
        .iter()
        .filter_map(|(key, count)| {
            let count = u32::try_from(count.as_u64()?).ok()?;
            (count > 0 && valid_key(key)).then(|| (key.clone(), count))
        })
        .take(MAX_KEYS)
        .collect())
}

fn save(paths: &Paths, counts: BTreeMap<String, u32>) -> io::Result<()> {
    let counts: Map<String, Value> = counts
        .into_iter()
        .map(|(key, count)| (key, json!(count)))
        .collect();
    let bytes = serde_json::to_vec_pretty(&json!({ "version": VERSION, "counts": counts }))
        .map_err(io::Error::other)?;
    thinoda_platform::fs::atomic_write(path(paths), &bytes, false)
}

/// Adds one activation for an allowlisted key, merging with the stored file.
pub fn record(paths: &Paths, key: &str) -> io::Result<()> {
    if !valid_key(key) {
        return Ok(());
    }
    let mut counts = load(paths)?;
    if counts.len() >= MAX_KEYS && !counts.contains_key(key) {
        return Ok(());
    }
    let count = counts.entry(key.to_owned()).or_default();
    *count = count.saturating_add(1);
    save(paths, counts)
}

/// Removes every stored count.
pub fn clear(paths: &Paths) -> io::Result<()> {
    save(paths, BTreeMap::new())
}

/// Counts sorted by frequency (then key) for display.
pub fn ranked(counts: &BTreeMap<String, u32>) -> Vec<(String, u32)> {
    let mut ranked: Vec<_> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_canvas::Action;
    use thinoda_core::Vec2;

    fn paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(dir.path());
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        (dir, paths)
    }

    #[test]
    fn keys_are_stable_bounded_and_position_free() {
        assert_eq!(
            key(
                MenuContext::Blank,
                &Cmd::Canvas(Action::InsertFloating(Vec2::new(3.0, 4.0)))
            ),
            Some("blank.topic.insert-floating".into())
        );
        assert_eq!(
            key(MenuContext::Topic, &Cmd::Canvas(Action::Copy)),
            Some("topic.edit.copy".into())
        );
        assert_eq!(key(MenuContext::Topic, &Cmd::OpenPath("x".into())), None);
        assert!(valid_key(&card_text_key(MenuContext::Board)));
        assert!(!valid_key("topic.Edit Copy"));
        assert!(!valid_key("unknown.edit.copy"));
    }

    #[test]
    fn records_merge_saturate_and_clear() {
        let (_dir, paths) = paths();
        assert!(load(&paths).unwrap().is_empty());
        record(&paths, "topic.edit.copy").unwrap();
        record(&paths, "topic.edit.copy").unwrap();
        record(&paths, "blank.edit.paste").unwrap();
        record(&paths, "not allowed").unwrap();
        let counts = load(&paths).unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(
            ranked(&counts),
            vec![
                ("topic.edit.copy".to_owned(), 2),
                ("blank.edit.paste".to_owned(), 1)
            ]
        );
        let mut saturated = BTreeMap::new();
        saturated.insert("topic.edit.copy".to_owned(), u32::MAX);
        save(&paths, saturated).unwrap();
        record(&paths, "topic.edit.copy").unwrap();
        assert_eq!(load(&paths).unwrap()["topic.edit.copy"], u32::MAX);
        clear(&paths).unwrap();
        assert!(load(&paths).unwrap().is_empty());
    }

    #[test]
    fn malformed_files_are_reported_not_overwritten() {
        let (_dir, paths) = paths();
        std::fs::write(path(&paths), b"not json").unwrap();
        assert!(load(&paths).is_err());
        assert!(record(&paths, "topic.edit.copy").is_err());
        assert_eq!(std::fs::read(path(&paths)).unwrap(), b"not json");
    }
}
