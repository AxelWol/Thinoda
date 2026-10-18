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
