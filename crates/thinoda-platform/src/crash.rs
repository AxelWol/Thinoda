//! Panic diagnostics and best-effort synchronous recovery flushing.

use crate::fs::{EntryKind, list_dir};
use crate::{Paths, recovery::RecoveryManager, unix_seconds};
use std::{backtrace::Backtrace, io, path::PathBuf, sync::Arc};

/// Install a process-global hook, chaining the previous hook after saving state.
/// The hook owns an `Arc`; call `RecoveryManager::finish` on clean exit even if
/// the hook still holds it. Install once, before starting worker threads.
pub fn install_panic_hook(paths: Paths, recovery: Option<Arc<RecoveryManager>>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let flushed = recovery
            .as_ref()
            .is_none_or(|manager| manager.try_flush_for_crash());
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        let location = info
            .location()
            .map(|p| format!("{}:{}:{}", p.file(), p.line(), p.column()))
            .unwrap_or_else(|| "unknown".into());
        let text = format!(
            "Thinoda {} crash\nTimestamp: {}\nOS: {} / {}\nMessage: {}\nLocation: {}\nRecovery flushed: {}\n\n{}\n",
            crate::system::app_version(),
            unix_seconds(),
            crate::system::os_name(),
            crate::system::arch(),
            message,
            location,
            flushed,
            Backtrace::force_capture()
        );
        let dir = paths.data_dir.join("crash-logs");
        let path = dir.join(format!(
            "crash-{}-{}.log",
            unix_seconds(),
            uuid::Uuid::new_v4()
        ));
        let _ = crate::fs::atomic_write(path, text.as_bytes(), false);
        previous(info);
    }));
}

/// Crash log identity and creation time recorded in its filename (Unix seconds).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashLog {
    pub path: PathBuf,
    pub timestamp: u64,
}

/// Enumerate crash logs newest first; missing directories are an empty list.
pub fn list_crash_logs(paths: &Paths) -> io::Result<Vec<CrashLog>> {
    let entries = match list_dir(paths.data_dir.join("crash-logs")) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut logs = Vec::new();
    for (path, kind) in entries {
        if kind != EntryKind::File {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(rest) = name
            .strip_prefix("crash-")
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };
        let timestamp = rest.split('-').next().and_then(|s| s.parse().ok());
        if let Some(timestamp) = timestamp {
            logs.push(CrashLog {
                path: path.clone(),
                timestamp,
            });
        }
    }
    logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then(b.path.cmp(&a.path)));
    Ok(logs)
}

/// Most recent crash at or after `timestamp` (inclusive to avoid same-second misses).
pub fn last_crash_since(paths: &Paths, timestamp: u64) -> io::Result<Option<CrashLog>> {
    Ok(list_crash_logs(paths)?
        .into_iter()
        .find(|log| log.timestamp >= timestamp))
}
