//! Markdown source tools over the existing lossless, recovery-backed notes field.

use egui::{Response, Ui};

use crate::{i18n::t, legal_markdown::Markdown};

pub(crate) const WORD_LIMIT: &str =
    "Word note export limit reached. The source is unchanged; shorten the note or save as Thinoda.";
pub(crate) const WORD_UNSUPPORTED: &str = "Word cannot preserve this note's formatting. Links, images, HTML and advanced Markdown are not supported. The source is unchanged; save as Thinoda.";
pub(crate) const FREEMIND_LIMIT: &str = "FreeMind note export limit reached. The source is unchanged; shorten the note or save as Thinoda.";
pub(crate) const FREEMIND_UNSUPPORTED: &str = "FreeMind cannot preserve this note's formatting. Links, images, HTML, advanced Markdown, tabs and repeated spaces are not supported. The source is unchanged; save as Thinoda.";

#[derive(Clone, Copy)]
enum Format {
    Bold,
    Italic,
    Bullet,
    Numbered,
}

fn format_selection(
    source: &str,
    range: std::ops::Range<usize>,
    format: Format,
) -> (String, std::ops::Range<usize>) {
    let start = source
        .char_indices()
        .nth(range.start)
        .map_or(source.len(), |(byte, _)| byte);
    let end = source
        .char_indices()
        .nth(range.end.max(range.start))
        .map_or(source.len(), |(byte, _)| byte);
    if let Some(delimiter) = match format {
        Format::Bold => Some("**"),
        Format::Italic => Some("*"),
        _ => None,
    } {
        let selected = &source[start..end];
        let inner = selected
            .strip_prefix(delimiter)
            .and_then(|text| text.strip_suffix(delimiter));
        if let Some(inner) = inner {
            let first = source[..start].chars().count();
            return (
                format!("{}{}{}", &source[..start], inner, &source[end..]),
                first..first + inner.chars().count(),
            );
        }
        if source[..start].ends_with(delimiter) && source[end..].starts_with(delimiter) {
            let prefix = &source[..start - delimiter.len()];
            let suffix = &source[end + delimiter.len()..];
            let first = prefix.chars().count();
            return (
                format!("{prefix}{selected}{suffix}"),
                first..first + selected.chars().count(),
            );
        }
    }
    let (start, end) = match format {
        Format::Bullet | Format::Numbered => {
            let start = source[..start].rfind('\n').map_or(0, |byte| byte + 1);
            // A selection ending at the start of a line excludes that line.
            let end = if end > start && source[..end].ends_with('\n') {
                end - 1
            } else {
                source[end..]
                    .find('\n')
                    .map_or(source.len(), |byte| end + byte)
            };
            (start, end)
        }
        _ => (start, end),
    };
    let selected = &source[start..end];
    let replacement = match format {
        Format::Bold => format!("**{selected}**"),
        Format::Italic => format!("*{selected}*"),
        Format::Bullet | Format::Numbered => {
            let lines: Vec<_> = selected.split('\n').collect();
            let remove = lines
                .iter()
                .all(|line| list_content(line, format).is_some());
            lines
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
                    let (indent, content) = line.split_at(indent);
                    if remove {
                        // All selected lines were checked before removing any markers.
                        format!("{indent}{}", list_content(content, format).unwrap())
                    } else {
                        let content = list_content(content, Format::Bullet)
                            .or_else(|| list_content(content, Format::Numbered))
                            .unwrap_or(content);
                        match format {
                            Format::Numbered => format!("{indent}{}. {content}", index + 1),
                            _ => format!("{indent}- {content}"),
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    };
    let start_chars = source[..start].chars().count();
    let selection = match format {
        Format::Bold | Format::Italic => {
            let delimiter = if matches!(format, Format::Bold) { 2 } else { 1 };
            let start = start_chars + delimiter;
            start..start + selected.chars().count()
        }
        Format::Bullet | Format::Numbered => start_chars..start_chars + replacement.chars().count(),
    };
    (
        format!("{}{}{}", &source[..start], replacement, &source[end..]),
        selection,
    )
}

fn list_content(line: &str, format: Format) -> Option<&str> {
    let line = line.trim_start_matches([' ', '\t']);
    match format {
        Format::Bullet => line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("+ ")),
        Format::Numbered => {
            let digits = line.bytes().take_while(u8::is_ascii_digit).count();
            if !(1..=9).contains(&digits) {
                return None;
            }
            line[digits..]
                .strip_prefix(". ")
                .or_else(|| line[digits..].strip_prefix(") "))
        }
        _ => None,
    }
}

fn preview_allowed(source: &str) -> bool {
    thinoda_core::notes::plain_text_projection(source).is_some()
}

pub(crate) fn tools(
    ui: &mut Ui,
    response: &Response,
    current: &str,
    done: &mut Option<String>,
) -> bool {
    let mut formatted = false;
    let touch_allowed = thinoda_ui::widgets::touch_activation_allowed(ui.ctx());
    let mut source = done
        .clone()
        .or_else(|| ui.data(|data| data.get_temp::<String>(response.id)))
        .unwrap_or_else(|| current.to_owned());
    let selection_id = response.id.with("format-selection");
    let selection_owner = response.id.with(&source);
    if response.has_focus()
        && let Some(range) = egui::TextEdit::load_state(ui.ctx(), response.id)
            .and_then(|state| state.cursor.char_range())
    {
        let range = range.as_sorted_char_range();
        ui.data_mut(|data| {
            data.insert_temp(selection_id, (selection_owner, range.start.0..range.end.0))
        });
    }
    ui.horizontal_wrapped(|ui| {
        for (label, format) in [
            ("Bold notes", Format::Bold),
            ("Italic notes", Format::Italic),
            ("Bulleted list", Format::Bullet),
            ("Numbered list", Format::Numbered),
        ] {
            if ui.button(t(label)).clicked() && touch_allowed {
                // TextEdit may collapse its selection on blur before the button runs.
                let selection = ui
                    .data(|data| data.get_temp::<(egui::Id, std::ops::Range<usize>)>(selection_id))
                    .filter(|(owner, _)| *owner == selection_owner)
                    .map(|(_, selection)| selection)
                    .unwrap_or_else(|| {
                        let end = source.chars().count();
                        end..end
                    });
                let (replacement, selection) = format_selection(&source, selection, format);
                source = replacement;
                let mut state =
                    egui::TextEdit::load_state(ui.ctx(), response.id).unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(selection.start),
                        egui::text::CCursor::new(selection.end),
                    )));
                state.store(ui.ctx(), response.id);
                response.request_focus();
                // Commit through the same conflict-checked inspector path as typing.
                ui.data_mut(|data| data.insert_temp(response.id, source.clone()));
                *done = Some(source.clone());
                formatted = true;
            }
        }
    });
    ui.small(t(
        "Formatting is stored as Markdown text; other views and exports may show markup.",
    ));
    let preview_id = response.id.with("markdown-preview");
    let mut preview = ui.data(|data| data.get_temp::<bool>(preview_id).unwrap_or(false));
    let previous = preview;
    if ui.checkbox(&mut preview, t("Markdown preview")).changed() && !touch_allowed {
        preview = previous;
    }
    ui.data_mut(|data| data.insert_temp(preview_id, preview));
    if !preview {
        return formatted;
    }
    ui.small(t("Preview does not open links or load images."));
    if !preview_allowed(&source) {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            t("Note preview limit reached. The full source is preserved and editable."),
        );
        return formatted;
    }
    if ui.button(t("Copy note as plain text")).clicked() && touch_allowed {
        // The size/complexity check above uses the same parser options as projection.
        if let Some(text) = thinoda_core::notes::plain_text_projection(&source) {
            ui.ctx().copy_text(text);
        }
    }
    ui.small(t("Copies Markdown text without formatting or link/image destinations. The saved note is unchanged."));
    let cache_id = preview_id.with("renderer");
    let mut markdown = ui
        .data(|data| data.get_temp::<Markdown>(cache_id))
        .unwrap_or_default();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(preview_id)
            .max_height(240.0)
            .show(ui, |ui| {
                let _ = markdown.show(ui, &source);
            });
    });
    ui.data_mut(|data| data.insert_temp(cache_id, markdown));
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_uses_character_offsets_and_preserves_surrounding_source() {
        assert_eq!(
            format_selection("a café 🌱 end", 2..8, Format::Bold),
            ("a **café 🌱** end".into(), 4..10)
        );
        assert_eq!(
            format_selection("abc", 1..1, Format::Italic),
            ("a**bc".into(), 2..2)
        );
        assert_eq!(
            format_selection("", 0..0, Format::Bold),
            ("****".into(), 2..2)
        );
    }

    #[test]
    fn lists_format_complete_selected_lines_without_consuming_the_next_line() {
        assert_eq!(
            format_selection("one\ntwo\nthree", 1..8, Format::Numbered).0,
            "1. one\n2. two\nthree"
        );
        assert_eq!(
            format_selection("one\ntwo\nthree", 5..5, Format::Bullet).0,
            "one\n- two\nthree"
        );
        assert_eq!(format_selection("one\n", 4..4, Format::Bullet).0, "one\n- ");
        assert_eq!(
            format_selection("one\ntwo", 0..4, Format::Bullet).0,
            "- one\ntwo"
        );
        assert_eq!(
            format_selection("café\n二", 99..99, Format::Numbered).0,
            "café\n1. 二"
        );
    }

    #[test]
    fn emphasis_is_reversible_for_selected_content_wrappers_and_empty_insertions() {
        for (format, source, selection) in [
            (Format::Bold, "a café 🌱 end", 2..8),
            (Format::Italic, "日本語 note", 0..3),
            (Format::Bold, "empty", 2..2),
        ] {
            let (formatted, inner) = format_selection(source, selection.clone(), format);
            assert_eq!(
                format_selection(&formatted, inner, format),
                (source.to_owned(), selection)
            );
        }

        assert_eq!(
            format_selection("a **café** end", 2..10, Format::Bold),
            ("a café end".into(), 2..6)
        );
        assert_eq!(
            format_selection("a *note* end", 2..8, Format::Italic),
            ("a note end".into(), 2..6)
        );
    }

    #[test]
    fn list_actions_toggle_selected_lines_and_preserve_unicode_indentation_and_suffix() {
        for format in [Format::Bullet, Format::Numbered] {
            let source = "intro\n  café\n\t日本語\nend";
            let (listed, selection) = format_selection(source, 7..17, format);
            assert_eq!(format_selection(&listed, selection, format).0, source);
        }
        assert_eq!(
            format_selection("- one\n- two", 0..11, Format::Numbered).0,
            "1. one\n2. two"
        );
        assert_eq!(
            format_selection("3) one\n9. two", 0..13, Format::Bullet).0,
            "- one\n- two"
        );
        assert_eq!(
            format_selection("- existing\nplain", 0..16, Format::Bullet).0,
            "- existing\n- plain"
        );
    }

    #[test]
    fn list_detection_does_not_consume_plain_text_or_unbounded_number_prefixes() {
        for source in [
            "2026 report",
            "-dash",
            "*emphasis*",
            "1234567890. text",
            "二. text",
        ] {
            assert!(list_content(source, Format::Numbered).is_none());
            assert!(list_content(source, Format::Bullet).is_none());
        }
        let (listed, range) = format_selection("", 0..0, Format::Bullet);
        assert_eq!(listed, "- ");
        assert_eq!(format_selection(&listed, range, Format::Bullet).0, "");
    }
    #[test]
    fn preview_bounds_do_not_truncate_or_mutate_source() {
        assert!(preview_allowed("**Bold**\n\n- item\n- *emphasis*"));
        assert!(!preview_allowed(
            &"x".repeat(thinoda_core::notes::MAX_PROJECTION_BYTES + 1)
        ));
        assert!(!preview_allowed(
            &"> ".repeat(thinoda_core::notes::MAX_PROJECTION_DEPTH + 1)
        ));
    }
}
