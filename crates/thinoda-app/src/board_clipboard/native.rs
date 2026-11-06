//! Native clipboard transport. Keep the handle alive for X11 clipboard ownership.

use super::{PreparedCopy, Snapshot, read_html, verify_cut_snapshot};

#[derive(Default)]
pub(super) struct Transport {
    clipboard: Option<arboard::Clipboard>,
    fake: Option<std::rc::Rc<std::cell::RefCell<FakeClipboard>>>,
}

impl Transport {
    fn clipboard(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().map_err(|error| {
                let wayland =
                    cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
                clipboard_initialization_error(&error.to_string(), wayland)
            })?);
        }
        Ok(self.clipboard.as_mut().expect("clipboard just initialized"))
    }

    pub(super) fn write(&mut self, prepared: &PreparedCopy) -> Result<(), String> {
        if let Some(fake) = &self.fake {
            let mut fake = fake.borrow_mut();
            if fake.fail_write {
                return Err("clipboard write denied".into());
            }
            fake.text = prepared.text.clone();
            fake.html = Some(prepared.html.clone());
            #[cfg(test)]
            {
                if let Some(text) = &fake.written_text {
                    fake.text = text.clone();
                }
                if let Some(html) = &fake.written_html {
                    fake.html = html.clone();
                }
            }
        } else {
            let clipboard = self.clipboard()?;
            clipboard
                .set_html(&prepared.html, Some(&prepared.text))
                .map_err(|error| error.to_string())?;
            if prepared.cut.is_none() {
                let html = clipboard.get().html().map_err(|error| {
                    format!("rich clipboard write could not be verified: {error}")
                })?;
                if read_html(&html)? != Some(prepared.rich.clone()) {
                    return Err("rich clipboard write could not be verified".into());
                }
            }
        }
        if prepared.cut.is_some() {
            let snapshot = self.read(None)?;
            verify_cut_snapshot(prepared, &snapshot)?;
        }
        Ok(())
    }

    pub(super) fn read(&mut self, event_text: Option<&str>) -> Result<Snapshot, String> {
        if let Some(fake) = &self.fake {
            let fake = fake.borrow();
            if fake.fail_read {
                return Err("clipboard read denied".into());
            }
            if fake.prefer_event_text
                && let Some(text) = event_text
            {
                return Ok(Snapshot {
                    text: text.into(),
                    html: None,
                });
            }
            #[cfg(test)]
            let after = fake
                .read_html_after
                .clone()
                .unwrap_or_else(|| fake.html.clone());
            #[cfg(not(test))]
            let after = fake.html.clone();
            return stable_snapshot(fake.html.clone(), fake.text.clone(), after);
        }
        let clipboard = self.clipboard()?;
        // Separate native reads can race another application's clipboard
        // change. Require a stable rich representation across the text read;
        // plain-text equality is never used as evidence of ownership.
        let before = clipboard.get().html().ok();
        let text = clipboard.get_text().map_err(|error| error.to_string())?;
        let after = clipboard.get().html().ok();
        stable_snapshot(before, text, after)
    }

    pub(super) fn headless() -> Self {
        Self {
            clipboard: None,
            fake: Some(std::rc::Rc::new(std::cell::RefCell::new(FakeClipboard {
                prefer_event_text: true,
                ..FakeClipboard::default()
            }))),
        }
    }

    #[cfg(test)]
    pub(super) fn with_fake(fake: std::rc::Rc<std::cell::RefCell<FakeClipboard>>) -> Self {
        Self {
            clipboard: None,
            fake: Some(fake),
        }
    }
}

fn stable_snapshot(
    before: Option<String>,
    text: String,
    after: Option<String>,
) -> Result<Snapshot, String> {
    if before != after {
        return Err("clipboard changed while it was being read".into());
    }
    Ok(Snapshot { text, html: after })
}

fn clipboard_initialization_error(error: &str, wayland: bool) -> String {
    if wayland {
        format!(
            "{error}. Wayland clipboard access requires a compositor supporting \
             ext-data-control-v1 or wlr-data-control-unstable-v1"
        )
    } else {
        error.to_owned()
    }
}

#[derive(Default)]
pub(super) struct FakeClipboard {
    pub text: String,
    pub html: Option<String>,
    pub fail_write: bool,
    pub fail_read: bool,
    pub prefer_event_text: bool,
    #[cfg(test)]
    pub written_text: Option<String>,
    #[cfg(test)]
    pub written_html: Option<Option<String>>,
    #[cfg(test)]
    pub read_html_after: Option<Option<String>>,
}

#[cfg(test)]
mod tests {
    use super::clipboard_initialization_error;

    #[test]
    fn wayland_clipboard_initialization_error_explains_protocol_requirement() {
        let error = clipboard_initialization_error("unavailable", true);
        assert!(error.contains("unavailable"));
        assert!(error.contains("ext-data-control-v1"));
        assert!(error.contains("wlr-data-control-unstable-v1"));
        assert_eq!(
            clipboard_initialization_error("unavailable", false),
            "unavailable"
        );
    }
}
