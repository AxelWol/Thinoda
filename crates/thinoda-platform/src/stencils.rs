//! Stencil panel preferences and imported draw.io libraries.
//!
//! Kept in `stencils.json` beside the settings so large libraries never bloat
//! `settings.toml`. Library contents are opaque `<mxlibrary>` text here; the
//! app validates them with its draw.io parser before use.

use std::{collections::BTreeSet, io::Write};

use serde::{Deserialize, Serialize};

use crate::{Error, Paths, Result, fs};

pub const FILE_NAME: &str = "stencils.json";
/// Total serialized size limit of the stencil file.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
pub const SIZE_LIMIT_ERROR: &str = "Stencil libraries exceed the 32 MiB limit.";
pub const MAX_LIBRARIES: usize = 64;
pub const MAX_ID_BYTES: usize = 64;
pub const MAX_TITLE_BYTES: usize = 800;
/// Built-in, collapsed or hidden library IDs remembered at most.
pub const MAX_IDS: usize = 256;

/// An imported or user-built library, stored as draw.io `<mxlibrary>` text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedLibrary {
    pub id: String,
    pub title: String,
    pub xml: String,
}

/// Persistent state of the Stencils panel.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StencilState {
    /// Built-in libraries the user hid with "More shapes…".
    pub hidden_builtin: Vec<String>,
    /// Library sections the user collapsed.
    pub collapsed: Vec<String>,
    /// Imported libraries and the scratchpad, in display order.
    pub libraries: Vec<SavedLibrary>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

struct SizeLimit {
    written: usize,
    exceeded: bool,
}

impl Write for SizeLimit {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.written) {
            self.exceeded = true;
            return Err(std::io::Error::other(SIZE_LIMIT_ERROR));
        }
        self.written += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Checks the structural and serialized-size limits; the app validates contents.
pub fn validate(state: &StencilState) -> std::result::Result<(), String> {
    if state.libraries.len() > MAX_LIBRARIES {
        return Err(format!("Keep at most {MAX_LIBRARIES} stencil libraries."));
    }
    if state.hidden_builtin.len() > MAX_IDS || state.collapsed.len() > MAX_IDS {
        return Err("Too many remembered stencil sections.".into());
    }
    if !state
        .hidden_builtin
        .iter()
        .chain(&state.collapsed)
        .all(|id| valid_id(id))
    {
        return Err("Stencil section IDs must be valid.".into());
    }
    let mut ids = BTreeSet::new();
    for library in &state.libraries {
        if !valid_id(&library.id) || !ids.insert(library.id.as_str()) {
            return Err("Stencil library IDs must be unique and valid.".into());
        }
        if library.title.len() > MAX_TITLE_BYTES {
            return Err("Stencil library titles are too long.".into());
        }
    }
    let mut counter = SizeLimit {
        written: 0,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut counter, state);
    if counter.exceeded {
        return Err(SIZE_LIMIT_ERROR.into());
    }
    result.map_err(|error| error.to_string())?;
    Ok(())
}

fn path(paths: &Paths) -> std::path::PathBuf {
    paths.config_dir.join(FILE_NAME)
}

/// Loads the panel state; a missing file is the default state.
pub fn load(paths: &Paths) -> Result<StencilState> {
    let bytes = match fs::read_bounded(path(paths), MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StencilState::default());
        }
        Err(error) => return Err(error.into()),
    };
    let state: StencilState = serde_json::from_slice(&bytes)?;
    validate(&state).map_err(Error::InvalidStencils)?;
    Ok(state)
}

/// Atomically replaces the stencil file.
pub fn save(paths: &Paths, state: &StencilState) -> Result<()> {
    validate(state).map_err(Error::InvalidStencils)?;
    let bytes = serde_json::to_vec(state)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::InvalidStencils(SIZE_LIMIT_ERROR.into()));
    }
    fs::atomic_write(path(paths), &bytes, false)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(root: &std::path::Path) -> Paths {
        let paths = Paths::from_root(root);
        paths.ensure().unwrap();
        paths
    }

    fn library(id: &str) -> SavedLibrary {
        SavedLibrary {
            id: id.into(),
            title: "Mine".into(),
            xml: "<mxlibrary>[]</mxlibrary>".into(),
        }
    }

    #[test]
    fn missing_file_loads_defaults_and_saved_state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths(dir.path());
        assert_eq!(load(&paths).unwrap(), StencilState::default());
        let state = StencilState {
            hidden_builtin: vec!["arrows".into()],
            collapsed: vec!["basic".into()],
            libraries: vec![library("scratchpad"), library("lib-1")],
        };
        save(&paths, &state).unwrap();
        assert_eq!(load(&paths).unwrap(), state);
    }

    #[test]
    fn invalid_state_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths(dir.path());
        let duplicate = StencilState {
            libraries: vec![library("a"), library("a")],
            ..Default::default()
        };
        assert!(save(&paths, &duplicate).is_err());
        let bad_id = StencilState {
            collapsed: vec!["../x".into()],
            ..Default::default()
        };
        assert!(save(&paths, &bad_id).is_err());
        let many = StencilState {
            libraries: (0..=MAX_LIBRARIES)
                .map(|i| library(&format!("l{i}")))
                .collect(),
            ..Default::default()
        };
        assert!(save(&paths, &many).is_err());
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(
            paths.config_dir.join(FILE_NAME),
            br#"{"libraries":[{"id":"","title":"","xml":""}]}"#,
        )
        .unwrap();
        assert!(load(&paths).is_err());
        std::fs::write(paths.config_dir.join(FILE_NAME), b"not json").unwrap();
        assert!(load(&paths).is_err());
    }

    #[test]
    fn validation_checks_the_exact_serialized_limit_before_saving() {
        let mut state = StencilState {
            libraries: vec![library("a")],
            ..Default::default()
        };
        state.libraries[0].xml.clear();
        let overhead = serde_json::to_vec(&state).unwrap().len();
        state.libraries[0].xml = "x".repeat(MAX_BYTES - overhead);
        assert!(validate(&state).is_ok());
        state.libraries[0].xml.push('x');
        assert!(validate(&state).is_err());

        state.libraries[0].xml = "\"".repeat((MAX_BYTES - overhead) / 2 + 1);
        assert!(state.libraries[0].xml.len() < MAX_BYTES);
        assert!(validate(&state).is_err());
    }
}
