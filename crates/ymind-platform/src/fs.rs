//! Durable same-directory replacement and external-change detection.

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Append `.bak` to the complete filename (for example `map.xmind.bak`).
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".bak");
    PathBuf::from(name)
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Durably write bytes before atomically replacing the destination.
/// The old destination survives any failed replacement. A requested backup is
/// durably replaced before the destination; callers must serialize concurrent saves.
pub fn atomic_write(path: impl AsRef<Path>, bytes: &[u8], keep_backup: bool) -> io::Result<()> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "destination must name a file")
    })?;
    let mut temp_name = OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let temporary = Temporary(parent.join(temp_name));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary.0)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if keep_backup {
        match fs::read(path) {
            Ok(previous) => atomic_write(backup_path(path), &previous, false)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    fs::rename(&temporary.0, path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "could not replace {} (the file may be read-only or in use): {e}",
                path.display()
            ),
        )
    })?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// Cheap file identity used to detect a possible external edit (not a content hash).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFingerprint {
    pub len: u64,
    pub modified: SystemTime,
    path: PathBuf,
}

/// Record a file's length and last-modified timestamp.
pub fn fingerprint(path: impl AsRef<Path>) -> io::Result<FileFingerprint> {
    let path = path.as_ref();
    let metadata = fs::metadata(path)?;
    Ok(FileFingerprint {
        len: metadata.len(),
        modified: metadata.modified()?,
        path: path.to_path_buf(),
    })
}

impl FileFingerprint {
    /// Check the recorded file; deletion counts as a change.
    pub fn changed_since(&self) -> io::Result<bool> {
        changed_since(self)
    }
}

/// Check the file recorded in a fingerprint, including deletion.
pub fn changed_since(previous: &FileFingerprint) -> io::Result<bool> {
    match fingerprint(&previous.path) {
        Ok(now) => Ok(now != *previous),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e),
    }
}

/// Find likely cloud-sync conflict siblings with the same extension.
/// This is a heuristic; numbered iCloud copies may also be intentional duplicates.
pub fn conflict_copies(path: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let computer = std::env::var("COMPUTERNAME").ok().map(|s| s.to_lowercase());
    let mut copies = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let candidate = entry.path();
        if candidate == path
            || !entry.file_type()?.is_file()
            || candidate
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                != extension
        {
            continue;
        }
        let candidate_stem = candidate
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        let Some(suffix) = candidate_stem.strip_prefix(&stem) else {
            continue;
        };
        let conflict =
            suffix.starts_with(" (") && suffix.ends_with(')') && suffix.contains("conflicted copy");
        let numbered = suffix
            .strip_prefix(' ')
            .is_some_and(|number| number.parse::<u32>().is_ok_and(|n| n >= 2));
        let machine = suffix.strip_prefix('-').is_some_and(|name| {
            (!name.is_empty() && (name.starts_with("desktop-") || name.starts_with("laptop-")))
                || computer
                    .as_deref()
                    .is_some_and(|c| !c.is_empty() && name == c)
        });
        if conflict || numbered || machine {
            copies.push(candidate);
        }
    }
    copies.sort();
    Ok(copies)
}
