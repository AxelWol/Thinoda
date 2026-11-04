//! Markdown source tools over the existing lossless, recovery-backed notes field.
pub(crate) const EXPORT_SOURCE_WARNING: &str = "This format exports formatted notes as Markdown source, not rich text. The complete source is retained, but formatting will not be displayed. The source workbook is unchanged. Save as Thinoda to preserve note formats.";
pub(crate) const EXPORT_SOURCE_REQUIRED: &str = "This format cannot preserve note formatting. Use Export with confirmation, or save as Thinoda.";

use egui::{Id, Response, Ui};
use pulldown_cmark::{Event, Parser, Tag};

use crate::{i18n::t, legal_markdown::Markdown};

pub(crate) const WORD_LIMIT: &str =
    "Word note export limit reached. The source is unchanged; shorten the note or save as Thinoda.";
pub(crate) const WORD_UNSUPPORTED: &str = "Word cannot preserve this note's formatting. Links, images, HTML and advanced Markdown are not supported. The source is unchanged; save as Thinoda.";
pub(crate) const FREEMIND_LIMIT: &str = "FreeMind note export limit reached. The source is unchanged; shorten the note or save as Thinoda.";
pub(crate) const FREEMIND_UNSUPPORTED: &str = "FreeMind cannot preserve this note's formatting. Links, images, HTML, advanced Markdown, tabs and repeated spaces are not supported. The source is unchanged; save as Thinoda.";
pub(crate) const EXCEL_LIMIT: &str = "Excel note export limit reached. The source is unchanged; shorten the note or save as Thinoda.";
pub(crate) const EXCEL_UNSUPPORTED: &str = "Excel cannot preserve this note's formatting. Links, images, HTML, tabs and advanced Markdown are not supported. The source is unchanged; save as Thinoda.";
const STRIKE_UNAVAILABLE: &str = "Cannot apply strikethrough here. Select text within one paragraph outside code, or shorten the note.";

#[derive(Clone, Copy)]
enum Format {
    Bold,
    Italic,
    Strike,
    Bullet,
    Numbered,
}

fn remove_emphasis(
    source: &str,
    start: usize,
    end: usize,
    delimiter: &str,
) -> Option<(String, std::ops::Range<usize>)> {
    let selected = &source[start..end];
    if let Some(inner) = selected
        .strip_prefix(delimiter)
        .and_then(|text| text.strip_suffix(delimiter))
    {
        let first = source[..start].chars().count();
        return Some((
            format!("{}{}{}", &source[..start], inner, &source[end..]),
            first..first + inner.chars().count(),
        ));
    }
    if source[..start].ends_with(delimiter) && source[end..].starts_with(delimiter) {
        let prefix = &source[..start - delimiter.len()];
        let suffix = &source[end + delimiter.len()..];
        let first = prefix.chars().count();
        return Some((
            format!("{prefix}{selected}{suffix}"),
            first..first + selected.chars().count(),
        ));
    }
    None
}

fn strikethrough_is_rendered(source: &str, selection: &std::ops::Range<usize>) -> bool {
    if selection.is_empty() {
        return true;
    }
    if !preview_allowed(source) {
        return false;
    }
    let Some(first) = selection.start.checked_sub(2) else {
        return false;
    };
    let Some(last) = selection.end.checked_add(2) else {
        return false;
    };
    if last > source.chars().count() {
        return false;
    }
    let byte_index = |index| {
        source
            .char_indices()
            .nth(index)
            .map_or(source.len(), |(byte, _)| byte)
    };
    let expected = byte_index(first)..byte_index(last);
    Parser::new_ext(source, crate::legal_markdown::OPTIONS)
        .into_offset_iter()
        .any(|(event, range)| {
            matches!(event, Event::Start(Tag::Strikethrough)) && range == expected
        })
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
    let delimiter = match format {
        Format::Bold => Some("**"),
        Format::Italic => Some("*"),
        Format::Strike => Some("~~"),
        _ => None,
    };
    let (start, end) = if let Some(delimiter) = delimiter {
        if let Some(result) = remove_emphasis(source, start, end, delimiter) {
            return result;
        }
        let selected = &source[start..end];
        let after_leading = selected.trim_start_matches(char::is_whitespace);
        let trimmed = after_leading.trim_end_matches(char::is_whitespace);
        let (first, last) = if trimmed.is_empty() {
            (end, end)
        } else {
            let first = start + selected.len() - after_leading.len();
            (first, first + trimmed.len())
        };
        if (first, last) != (start, end)
            && let Some(result) = remove_emphasis(source, first, last, delimiter)
        {
            return result;
        }
        (first, last)
    } else {
        (start, end)
    };
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
        Format::Strike => format!("~~{selected}~~"),
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
        Format::Bold | Format::Italic | Format::Strike => {
            let delimiter = if matches!(format, Format::Italic) {
                1
            } else {
                2
            };
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

#[derive(Clone)]
struct EnlargedNote {
    owner: Id,
    frame: u64,
    text: String,
    markdown: bool,
}

fn enlarged_id() -> Id {
    Id::unique("enlarged-notes-draft")
}

pub(crate) fn enlarged_active(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp::<EnlargedNote>(enlarged_id())
            .is_some_and(|draft| ctx.cumulative_frame_nr() <= draft.frame.saturating_add(1))
    })
}

/// Wait for the ordinary Notes blur commit before capturing an editor revision.
pub(crate) fn editor_open_requested(ui: &Ui, request_id: Id, source_owner: Id, frame: u64) -> bool {
    ui.data_mut(|data| {
        let request = data.get_temp::<(Id, u64)>(request_id);
        data.remove::<(Id, u64)>(request_id);
        match request {
            Some((expected, requested_frame)) if expected == source_owner => {
                if frame == requested_frame {
                    data.insert_temp(request_id, (expected, requested_frame));
                }
                frame == requested_frame.saturating_add(1)
            }
            _ => false,
        }
    })
}

pub(crate) fn tools(
    ui: &mut Ui,
    response: &Response,
    current: &str,
    done: &mut Option<String>,
    markdown: bool,
    revision: u64,
) -> bool {
    let source = done
        .clone()
        .or_else(|| ui.data(|data| data.get_temp::<String>(response.id)))
        .unwrap_or_else(|| current.to_owned());
    let id = enlarged_id();
    let frame = ui.ctx().cumulative_frame_nr();
    let owner = response.id.with((&source, markdown, revision));
    let source_owner = response.id.with((&source, markdown));
    let request_id = id.with("open-request");
    let requested = editor_open_requested(ui, request_id, source_owner, frame);
    let mut draft = ui.data_mut(|data| {
        let draft = data
            .get_temp::<EnlargedNote>(id)
            .filter(|draft| draft.owner == owner && frame <= draft.frame.saturating_add(1));
        data.remove::<EnlargedNote>(id);
        draft
    });
    let allowed = ui.is_enabled() && thinoda_ui::widgets::touch_activation_allowed(ui.ctx());
    if requested && allowed {
        draft = Some(EnlargedNote {
            owner,
            frame,
            text: source,
            markdown,
        });
        egui::text_edit::TextEditState::default().store(ui.ctx(), id.with(owner));
    }
    if let Some(mut pending) = draft {
        let mut apply = false;
        let mut cancel = false;
        let labels = crate::i18n::dialog_labels();
        let result = thinoda_ui::Dialog::new(id, t("Notes editor"))
            .labels(&labels)
            .enabled(ui.is_enabled())
            .default_width(720.0)
            .default_height(520.0)
            .min_size(egui::vec2(280.0, 160.0))
            .show(ui.ctx(), |ui| {
                ui.small(t("Apply to keep edits. Cancel discards them. Unapplied dialog edits are not saved or recovered."));
                ui.small(t("Changing the selection, tab or document revision closes enlarged editors without applying their edits. Apply before editing elsewhere."));
                let label = ui.label(t("Notes"));
                let edit = ui.add_enabled(
                    !crate::equations::enlarged_active(ui.ctx()),
                    egui::TextEdit::multiline(&mut pending.text)
                        .id(id.with(owner))
                        .desired_rows(12)
                        .desired_width(f32::INFINITY),
                );
                let edit = edit.labelled_by(label.id);
                // Reuse the same formatting, equation and inert-preview tools.
                // Their results only replace this staged text until Apply.
                ui.data_mut(|data| data.insert_temp(edit.id, pending.text.clone()));
                let mut replacement = None;
                let mut formatted = tools_inline(
                    ui,
                    &edit,
                    &pending.text,
                    &mut replacement,
                );
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                formatted |= thinoda_ui::widgets::page_section(ui, t("Equation"), |ui| {
                    equation_tools(ui, &edit, &pending.text, &mut replacement, pending.markdown || formatted, revision)
                });
                if let Some(replacement) = replacement {
                    pending.text = replacement;
                }
                pending.markdown |= formatted;
                ui.horizontal_wrapped(|ui| {
                    apply = ui.add_enabled(
                        !crate::equations::editing_active(ui.ctx()),
                        egui::Button::new(t("Apply notes")),
                    ).clicked();
                    cancel = ui.button(t("Cancel notes edit")).clicked();
                });
            });
        if apply && allowed {
            ui.data_mut(|data| data.insert_temp(response.id, pending.text.clone()));
            *done = Some(pending.text);
            return pending.markdown;
        }
        if !result.close_requested && !cancel {
            pending.frame = frame;
            ui.data_mut(|data| data.insert_temp(id, pending));
        }
        return false;
    }
    if ui
        .add_enabled(
            !crate::equations::editing_active(ui.ctx()),
            egui::Button::new(t("Enlarge notes editor")),
        )
        .clicked()
        && allowed
    {
        // Let ordinary Notes blur/commit before capturing its new revision.
        ui.data_mut(|data| data.insert_temp(request_id, (source_owner, frame)));
        ui.ctx().request_repaint();
    }
    tools_inline(ui, response, current, done)
}

fn tools_inline(
    ui: &mut Ui,
    response: &Response,
    current: &str,
    done: &mut Option<String>,
) -> bool {
    if crate::equations::enlarged_active(ui.ctx()) {
        return false;
    }
    let mut formatted = false;
    let touch_allowed = thinoda_ui::widgets::touch_activation_allowed(ui.ctx());
    let mut source = done
        .clone()
        .or_else(|| ui.data(|data| data.get_temp::<String>(response.id)))
        .unwrap_or_else(|| current.to_owned());
    let selection_id = response.id.with("format-selection");
    let selection_owner = response.id.with(&source);
    let feedback_id = response.id.with("strike-feedback");
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
            ("Strikethrough notes", Format::Strike),
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
                let previous_selection = selection.clone();
                let (replacement, selection) = format_selection(&source, selection, format);
                let invalid_strike = matches!(format, Format::Strike)
                    && replacement.len() > source.len()
                    && !strikethrough_is_rendered(&replacement, &selection);
                let selection = if invalid_strike {
                    previous_selection
                } else {
                    selection
                };
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
                if invalid_strike {
                    ui.data_mut(|data| data.insert_temp(feedback_id, selection_owner));
                    continue;
                }
                ui.data_mut(|data| data.remove::<Id>(feedback_id));
                source = replacement;
                // Commit through the same conflict-checked inspector path as typing.
                ui.data_mut(|data| data.insert_temp(response.id, source.clone()));
                *done = Some(source.clone());
                formatted = true;
            }
        }
    });
    if ui.data(|data| data.get_temp::<Id>(feedback_id)) == Some(selection_owner) {
        ui.colored_label(ui.visuals().warn_fg_color, t(STRIKE_UNAVAILABLE));
    }
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

pub(crate) fn equation_tools(
    ui: &mut Ui,
    response: &Response,
    current: &str,
    done: &mut Option<String>,
    markdown: bool,
    revision: u64,
) -> bool {
    let source = done
        .clone()
        .or_else(|| ui.data(|data| data.get_temp::<String>(response.id)))
        .unwrap_or_else(|| current.to_owned());
    if let Some(replacement) = crate::equations::tools(ui, response, &source, markdown, revision) {
        ui.data_mut(|data| data.insert_temp(response.id, replacement.clone()));
        *done = Some(replacement);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enlarged_editor_messages_exist_in_every_catalog() {
        for language in crate::i18n::Lang::ALL {
            for key in [
                "Enlarge notes editor",
                "Enlarge equation editor",
                "Notes editor",
                "Equation editor",
                "Apply notes",
                "Cancel notes edit",
                "Apply to keep edits. Cancel discards them. Unapplied dialog edits are not saved or recovered.",
                "Changing the selection, tab or document revision closes enlarged editors without applying their edits. Apply before editing elsewhere.",
                "Select a topic to edit its data.",
                "Equation",
            ] {
                if language != crate::i18n::Lang::En {
                    assert_ne!(crate::i18n::t_in(language, key), key, "{language:?}: {key}");
                }
            }
        }
    }

    #[test]
    fn strikethrough_messages_exist_in_every_catalog() {
        for language in crate::i18n::Lang::ALL {
            for key in ["Strikethrough notes", STRIKE_UNAVAILABLE] {
                if language != crate::i18n::Lang::En {
                    assert_ne!(crate::i18n::t_in(language, key), key, "{language:?}: {key}");
                }
            }
        }
    }

    #[test]
    fn formatting_uses_character_offsets_and_preserves_surrounding_source() {
        assert_eq!(
            format_selection("a café 🌱 end", 2..8, Format::Bold),
            ("a **café 🌱** end".into(), 4..10)
        );
        assert_eq!(
            format_selection("a café 🌱 end", 2..8, Format::Strike),
            ("a ~~café 🌱~~ end".into(), 4..10)
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
            (Format::Strike, "a café 🌱 end", 2..8),
            (Format::Strike, "empty", 2..2),
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
        assert_eq!(
            format_selection("a ~~café~~ end", 2..10, Format::Strike),
            ("a café end".into(), 2..6)
        );
    }

    #[test]
    fn emphasis_preserves_outer_whitespace_and_formats_selected_text() {
        let source = "a  café 🌱  end";
        for format in [Format::Bold, Format::Italic, Format::Strike] {
            let (formatted, selection) = format_selection(source, 2..10, format);
            let document = thinoda_core::notes::formatted_document(&formatted).unwrap();
            assert!(
                document.paragraphs.iter().flat_map(|p| &p.runs).any(|run| {
                    run.text == "café 🌱"
                        && match format {
                            Format::Bold => run.style.bold,
                            Format::Italic => run.style.italic,
                            Format::Strike => run.style.strike,
                            _ => false,
                        }
                }),
                "{formatted:?}: {document:?}"
            );
            assert_eq!(format_selection(&formatted, selection, format).0, source);
        }
    }

    #[test]
    fn emphasis_handles_whitespace_only_and_wrapped_whitespace_selections() {
        for (format, source, selection) in [
            (Format::Bold, "a  **café**  b", 2..12),
            (Format::Italic, "a  *café*  b", 2..10),
            (Format::Strike, "a  ~~café~~  b", 2..12),
        ] {
            assert_eq!(format_selection(source, selection, format).0, "a  café  b");
        }
        for format in [Format::Bold, Format::Italic, Format::Strike] {
            let (formatted, selection) = format_selection("a  b", 1..3, format);
            assert_eq!(format_selection(&formatted, selection, format).0, "a  b");
        }
        assert_eq!(
            format_selection("a ~~  ~~ b", 4..6, Format::Strike).0,
            "a    b"
        );
    }

    #[test]
    fn strike_validation_requires_a_bounded_rendered_span() {
        for (source, selection) in [
            ("café 🌱", 0..6),
            ("before **café** after", 7..15),
            ("line one\nline two", 0..17),
        ] {
            let (formatted, inner) = format_selection(source, selection, Format::Strike);
            assert!(
                strikethrough_is_rendered(&formatted, &inner),
                "{formatted:?}"
            );
        }
        for (source, selection) in [
            ("first\n\nsecond", 0..13),
            ("`code`", 1..5),
            ("~~~\ncode\n~~~", 4..8),
        ] {
            let (formatted, inner) = format_selection(source, selection, Format::Strike);
            assert!(
                !strikethrough_is_rendered(&formatted, &inner),
                "{formatted:?}"
            );
        }
        let large = "x".repeat(thinoda_core::notes::MAX_PROJECTION_BYTES + 1);
        let (formatted, inner) = format_selection(&large, 0..1, Format::Strike);
        assert!(!strikethrough_is_rendered(&formatted, &inner));
        let many_events = format!("{}target", "**x** ".repeat(700));
        let last = many_events.chars().count();
        let (formatted, inner) = format_selection(&many_events, last - 6..last, Format::Strike);
        assert!(!strikethrough_is_rendered(&formatted, &inner));
        let deep = format!(
            "{}target",
            "> ".repeat(thinoda_core::notes::MAX_PROJECTION_DEPTH)
        );
        let last = deep.chars().count();
        let (formatted, inner) = format_selection(&deep, last - 6..last, Format::Strike);
        assert!(!strikethrough_is_rendered(&formatted, &inner));
        assert!(strikethrough_is_rendered("~~~~", &(2..2)));
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
