//! Glyph icons available in egui's default fonts (Ubuntu, Noto Emoji, emoji-icon-font).
//! Using glyphs keeps the binary small and makes icons follow the text color.

pub const MENU: &str = "☰";
pub const UNDO: &str = "⟲";
pub const REDO: &str = "⟳";
pub const SEARCH: &str = "🔍";
pub const ADD: &str = "➕";
pub const CLOSE: &str = "✖";
pub const CHECK: &str = "✔";
pub const SETTINGS: &str = "⚙";
pub const NEW_FILE: &str = "🗋";
pub const OPEN: &str = "🗁";
pub const SAVE: &str = "💾";
pub const EXPORT: &str = "📤";
pub const PRINT: &str = "🖶";
pub const TOPIC: &str = "◻";
pub const SUBTOPIC: &str = "↪";
pub const RELATIONSHIP: &str = "⤴";
pub const BOUNDARY: &str = "☐";
pub const SUMMARY: &str = "}";
pub const NOTE: &str = "📝";
pub const LINK: &str = "🔗";
pub const LABEL: &str = "🏷";
pub const IMAGE: &str = "🖼";
pub const MARKER: &str = "☺";
pub const TODO: &str = "☑";
pub const OUTLINER: &str = "📃";
pub const MAP: &str = "⛶";
pub const ZEN: &str = "◑";
pub const PITCH: &str = "▶";
pub const FORMAT: &str = "🎨";
pub const ZOOM_IN: &str = "🔍+";
pub const ZOOM_OUT: &str = "−";
pub const FIT: &str = "⛶";
pub const SHARE: &str = "↗";
pub const CLOUD: &str = "☁";
pub const INFO: &str = "ℹ";
pub const WARNING: &str = "⚠";
pub const ERROR: &str = "⛔";
pub const CHEVRON_RIGHT: &str = "⏵";
pub const CHEVRON_DOWN: &str = "⏷";
pub const COLLAPSE: &str = "⊟";
pub const EXPAND: &str = "⊞";
pub const FILTER: &str = "🔻";
pub const SHEET: &str = "🗐";
pub const KEYBOARD: &str = "⌨";
pub const HELP: &str = "❓";
pub const RECENT: &str = "🕓";
pub const PIN: &str = "📌";
pub const TRASH: &str = "🗑";
pub const STAR: &str = "★";
pub const COPY: &str = "🗐";
pub const BOLD: &str = "B";
pub const ITALIC: &str = "I";
pub const STRIKE: &str = "S";
pub const UNDERLINE: &str = "U";

/// All icons with their names (used to verify that every glyph is renderable).
pub const ALL: &[(&str, &str)] = &[
    ("MENU", MENU),
    ("UNDO", UNDO),
    ("REDO", REDO),
    ("SEARCH", SEARCH),
    ("ADD", ADD),
    ("CLOSE", CLOSE),
    ("CHECK", CHECK),
    ("SETTINGS", SETTINGS),
    ("NEW_FILE", NEW_FILE),
    ("OPEN", OPEN),
    ("SAVE", SAVE),
    ("EXPORT", EXPORT),
    ("PRINT", PRINT),
    ("TOPIC", TOPIC),
    ("SUBTOPIC", SUBTOPIC),
    ("RELATIONSHIP", RELATIONSHIP),
    ("BOUNDARY", BOUNDARY),
    ("SUMMARY", SUMMARY),
    ("NOTE", NOTE),
    ("LINK", LINK),
    ("LABEL", LABEL),
    ("IMAGE", IMAGE),
    ("MARKER", MARKER),
    ("TODO", TODO),
    ("OUTLINER", OUTLINER),
    ("MAP", MAP),
    ("ZEN", ZEN),
    ("PITCH", PITCH),
    ("FORMAT", FORMAT),
    ("ZOOM_IN", ZOOM_IN),
    ("ZOOM_OUT", ZOOM_OUT),
    ("FIT", FIT),
    ("SHARE", SHARE),
    ("CLOUD", CLOUD),
    ("INFO", INFO),
    ("WARNING", WARNING),
    ("ERROR", ERROR),
    ("CHEVRON_RIGHT", CHEVRON_RIGHT),
    ("CHEVRON_DOWN", CHEVRON_DOWN),
    ("COLLAPSE", COLLAPSE),
    ("EXPAND", EXPAND),
    ("FILTER", FILTER),
    ("SHEET", SHEET),
    ("KEYBOARD", KEYBOARD),
    ("HELP", HELP),
    ("RECENT", RECENT),
    ("PIN", PIN),
    ("TRASH", TRASH),
    ("STAR", STAR),
    ("COPY", COPY),
    ("BOLD", BOLD),
    ("ITALIC", ITALIC),
    ("STRIKE", STRIKE),
    ("UNDERLINE", UNDERLINE),
];

#[cfg(test)]
mod tests {
    #[test]
    fn all_glyphs_available() {
        let harness = egui_kittest::Harness::new_ui_state(
            |ui, missing: &mut Vec<&'static str>| {
                let font = egui::FontId::proportional(16.0);
                *missing = super::ALL
                    .iter()
                    .filter(|(_, g)| !ui.fonts_mut(|f| f.has_glyphs(&font, g)))
                    .map(|(n, _)| *n)
                    .collect();
            },
            Vec::new(),
        );
        assert!(
            harness.state().is_empty(),
            "missing glyphs: {:?}",
            harness.state()
        );
    }
}
