//! Thin non-GUI operating-system integrations. Commands never use a shell.

use std::{io, path::Path, process::Command};

/// Whether `url` may be handed to the OS: HTTP(S) or `mailto:` without control characters.
pub fn is_openable_url(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    let scheme_ok = lower.starts_with("https://")
        || lower.starts_with("http://")
        || (lower.starts_with("mailto:") && url.len() > "mailto:".len());
    scheme_ok && !url.chars().any(char::is_control)
}

/// Open an HTTP(S) URL in the system browser or a `mailto:` URL in the default mail
/// client; other schemes are rejected.
pub fn open_url(url: &str) -> io::Result<()> {
    if !is_openable_url(url) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only HTTP(S) and mailto URLs are supported",
        ));
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler").arg(url);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    spawn_reaped(&mut command)
}

/// Select a file in Explorer/Finder, or open its parent on Linux.
pub fn reveal_in_file_manager(path: impl AsRef<Path>) -> io::Result<()> {
    let path = std::fs::canonicalize(path.as_ref())?;
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("explorer.exe");
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(&path);
        c.arg(arg);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg("-R").arg(&path);
        c
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(if path.is_dir() {
            path.as_path()
        } else {
            path.parent().unwrap_or(&path)
        });
        c
    };
    spawn_reaped(&mut command)
}

/// Opens a generated local print document in the default HTML application.
/// This is only a handoff, never confirmation that a dialog opened or a page printed.
/// Caller keeps the file alive while the external application may still read it.
pub fn open_print_document(path: &Path) -> io::Result<()> {
    let path = std::fs::canonicalize(path)?;
    if !path.is_file() || path.extension().is_none_or(|e| e != "html") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected an HTML print document",
        ));
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler").arg(&path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(&path);
        c
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(&path);
        c
    };
    // Called on a worker: observe launcher failures without blocking egui.
    // Do not capture pipes: desktop launchers can pass them to a long-lived
    // browser, keeping Command::output blocked even after the launcher exits.
    let mut child = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "print document launcher exited with {status}"
                )))
            };
        }
        if std::time::Instant::now() >= deadline {
            // Some xdg-open helpers wait for the browser to close. Leave it
            // running, but do not leave Thinoda's output controls busy forever.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "print launcher has not finished; the document may already be open in your browser",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn spawn_reaped(command: &mut Command) -> io::Result<()> {
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Rust's stable platform OS identifier, such as `windows`, `macos`, or `linux`.
pub const fn os_name() -> &'static str {
    std::env::consts::OS
}
/// Target CPU architecture, such as `x86_64` or `aarch64`.
pub const fn arch() -> &'static str {
    std::env::consts::ARCH
}
/// Application workspace version compiled into this crate.
pub const fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
/// Whether an explicit portable root was selected with `THINODA_HOME`.
pub fn is_portable() -> bool {
    std::env::var_os("THINODA_HOME").is_some_and(|s| !s.is_empty())
}

#[cfg(test)]
mod print_tests {
    use super::*;

    #[test]
    fn print_launcher_rejects_missing_files_directories_and_other_extensions() {
        let directory = tempfile::tempdir().unwrap();
        assert!(open_print_document(&directory.path().join("missing.html")).is_err());
        assert!(open_print_document(directory.path()).is_err());
        let file = directory.path().join("document.txt");
        std::fs::write(&file, "not a print document").unwrap();
        assert_eq!(
            open_print_document(&file).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
