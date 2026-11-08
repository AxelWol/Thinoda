//! Native clipboard transport. Keep the handle alive for X11 clipboard ownership.

use super::{PreparedCopy, PreparedSheet, SheetWrite, Snapshot, read_html, verify_cut_snapshot};

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
            fake.svg = None;
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

    /// Writes plain text only, replacing any rich representation.
    pub(super) fn write_text(&mut self, text: &str) -> Result<(), String> {
        if let Some(fake) = &self.fake {
            let mut fake = fake.borrow_mut();
            if fake.fail_write {
                return Err("clipboard write denied".into());
            }
            fake.text = text.to_owned();
            fake.html = None;
            fake.svg = None;
            return Ok(());
        }
        self.clipboard()?
            .set_text(text)
            .map_err(|error| error.to_string())
    }

    pub(super) fn write_sheet(
        &mut self,
        prepared: &PreparedSheet,
        owner: Option<raw_window_handle::WindowHandle<'_>>,
    ) -> Result<SheetWrite, String> {
        if let Some(fake) = &self.fake {
            let mut fake = fake.borrow_mut();
            if fake.fail_write {
                return Err("clipboard write denied".into());
            }
            fake.text = prepared.text.clone();
            fake.html = None;
            fake.svg = None;
            return match &prepared.svg {
                Err(error) => Ok(SheetWrite::TextOnly(error.clone())),
                #[cfg(test)]
                Ok(_) if fake.fail_svg_write => {
                    Ok(SheetWrite::TextOnly("SVG clipboard write denied".into()))
                }
                Ok(svg) => {
                    fake.svg = Some(svg.clone());
                    Ok(SheetWrite::TextAndSvg)
                }
            };
        }

        let svg = match &prepared.svg {
            Ok(svg) => svg,
            Err(error) => {
                self.write_text(&prepared.text)?;
                return Ok(SheetWrite::TextOnly(error.clone()));
            }
        };
        #[cfg(target_os = "windows")]
        {
            let raw_window_handle::RawWindowHandle::Win32(handle) =
                owner.ok_or("application window is unavailable")?.as_raw()
            else {
                self.write_text(&prepared.text)?;
                return Ok(SheetWrite::TextOnly(
                    "the application window does not support Windows SVG clipboard data".into(),
                ));
            };
            let Some(format) = clipboard_win::raw::register_format("image/svg+xml") else {
                self.write_text(&prepared.text)?;
                return Ok(SheetWrite::TextOnly(
                    "could not register the SVG clipboard format".into(),
                ));
            };
            let _clipboard = clipboard_win::Clipboard::new_attempts_for(handle.hwnd.get() as _, 5)
                .map_err(|error| format!("could not open clipboard: {error}"))?;
            write_sheet_formats(
                &mut WindowsSheetClipboard(format.get()),
                &prepared.text,
                svg,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (owner, svg);
            self.write_text(&prepared.text)?;
            Ok(SheetWrite::TextOnly(
                "SVG clipboard data is not supported on this platform".into(),
            ))
        }
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

    #[cfg(target_os = "windows")]
    pub(super) fn uses_fake(&self) -> bool {
        self.fake.is_some()
    }

    #[cfg(test)]
    pub(super) fn with_fake(fake: std::rc::Rc<std::cell::RefCell<FakeClipboard>>) -> Self {
        Self {
            clipboard: None,
            fake: Some(fake),
        }
    }
}

#[cfg(any(target_os = "windows", test))]
trait SheetClipboardSink {
    fn clear(&mut self) -> Result<(), String>;
    fn write_text(&mut self, text: &str) -> Result<(), String>;
    fn write_svg(&mut self, bytes: &[u8]) -> Result<(), String>;
}

#[cfg(any(target_os = "windows", test))]
fn write_sheet_formats(
    sink: &mut impl SheetClipboardSink,
    text: &str,
    svg: &str,
) -> Result<SheetWrite, String> {
    sink.clear()?;
    let mut bytes = svg.as_bytes().to_vec();
    bytes.push(0);
    // Windows consumers that enumerate formats generally choose the first they recognize.
    let svg_result = sink.write_svg(&bytes);
    sink.write_text(text)?;
    match svg_result {
        Ok(()) => Ok(SheetWrite::TextAndSvg),
        Err(error) => Ok(SheetWrite::TextOnly(error)),
    }
}

#[cfg(target_os = "windows")]
struct WindowsSheetClipboard(u32);

#[cfg(target_os = "windows")]
impl SheetClipboardSink for WindowsSheetClipboard {
    fn clear(&mut self) -> Result<(), String> {
        clipboard_win::raw::empty().map_err(|error| format!("could not empty clipboard: {error}"))
    }

    fn write_text(&mut self, text: &str) -> Result<(), String> {
        clipboard_win::raw::set_string_with(text, clipboard_win::options::NoClear)
            .map_err(|error| format!("could not write sheet text: {error}"))
    }

    fn write_svg(&mut self, bytes: &[u8]) -> Result<(), String> {
        clipboard_win::raw::set_without_clear(self.0, bytes)
            .map_err(|error| format!("could not write SVG clipboard data: {error}"))
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
    pub svg: Option<String>,
    pub fail_write: bool,
    pub fail_read: bool,
    pub prefer_event_text: bool,
    #[cfg(test)]
    pub fail_svg_write: bool,
    #[cfg(test)]
    pub written_text: Option<String>,
    #[cfg(test)]
    pub written_html: Option<Option<String>>,
    #[cfg(test)]
    pub read_html_after: Option<Option<String>>,
}

#[cfg(test)]
mod tests {
    use super::{
        SheetClipboardSink, SheetWrite, clipboard_initialization_error, write_sheet_formats,
    };

    #[derive(Default)]
    struct RecordingSink {
        operations: Vec<String>,
        fail: Option<&'static str>,
    }

    impl SheetClipboardSink for RecordingSink {
        fn clear(&mut self) -> Result<(), String> {
            self.operations.push("clear".into());
            if self.fail == Some("clear") {
                Err("clear failed".into())
            } else {
                Ok(())
            }
        }

        fn write_text(&mut self, text: &str) -> Result<(), String> {
            self.operations.push(format!("text:{text}"));
            if self.fail == Some("text") {
                Err("text failed".into())
            } else {
                Ok(())
            }
        }

        fn write_svg(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.operations.push(format!("svg:{bytes:?}"));
            if self.fail == Some("svg") {
                Err("svg failed".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn sheet_formats_clear_once_and_keep_text_on_svg_failure() {
        let mut sink = RecordingSink::default();
        assert!(matches!(
            write_sheet_formats(&mut sink, "sheet", "<svg/>"),
            Ok(SheetWrite::TextAndSvg)
        ));
        assert_eq!(
            sink.operations,
            ["clear", "svg:[60, 115, 118, 103, 47, 62, 0]", "text:sheet"]
        );

        for stage in ["clear", "text", "svg"] {
            let mut sink = RecordingSink {
                fail: Some(stage),
                ..Default::default()
            };
            let result = write_sheet_formats(&mut sink, "sheet", "<svg/>");
            if stage == "svg" {
                assert!(
                    matches!(result, Ok(SheetWrite::TextOnly(reason)) if reason == "svg failed")
                );
                assert_eq!(sink.operations.len(), 3);
            } else {
                assert!(result.is_err());
                assert_eq!(sink.operations.len(), if stage == "clear" { 1 } else { 3 });
            }
        }
    }

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
