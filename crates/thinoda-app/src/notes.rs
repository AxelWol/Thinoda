//! Markdown source tools over the existing lossless, recovery-backed notes field.
pub(crate) const EXPORT_SOURCE_WARNING: &str = "This format exports formatted notes as Markdown source, not rich text. The complete source is retained, but formatting will not be displayed. The source workbook is unchanged. Save as Thinoda to preserve note formats.";
pub(crate) const EXPORT_SOURCE_REQUIRED: &str = "This format cannot preserve note formatting. Use Export with confirmation, or save as Thinoda.";

use egui::{Id, Response, Ui};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::{i18n::t, legal_markdown::Markdown};

pub(crate) const WORD_LIMIT: &str =
    "Word note export limit reached. The source is unchanged; shorten the note or save as Thinoda.";
pub(crate) const WORD_UNSUPPORTED: &str = "Word cannot preserve this note's formatting. Links, images, HTML and advanced Markdown are not supported. The source is unchanged; save as Thinoda.";
pub(crate) const FREEMIND_LIMIT: &str = "FreeMind note export limit reached. The source is unchanged; shorten the note or save as Thinoda.";
pub(crate) const FREEMIND_UNSUPPORTED: &str = "FreeMind cannot preserve this note's formatting. Links, images, HTML, advanced Markdown, tabs and repeated spaces are not supported. The source is unchanged; save as Thinoda.";
pub(crate) const EXCEL_LIMIT: &str = "Excel note export limit reached. The source is unchanged; shorten the note or save as Thinoda.";
pub(crate) const EXCEL_UNSUPPORTED: &str = "Excel cannot preserve this note's formatting. Links, images, HTML, tabs and advanced Markdown are not supported. The source is unchanged; save as Thinoda.";
const EMPHASIS_UNAVAILABLE: &str =
    "Cannot apply formatting to this selection. The formatting action was canceled.";

#[derive(Clone, Copy, Debug)]
enum Format {
    Bold,
    Italic,
    Strike,
    Bullet,
    Numbered,
}

fn marker_is_escaped(source: &str, position: usize) -> bool {
    source[..position]
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count()
        % 2
        == 1
}

fn unescaped_asterisk_run(source: &str, position: usize) -> usize {
    let before = source[..position]
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'*')
        .count();
    let after = source[position..]
        .bytes()
        .take_while(|byte| *byte == b'*')
        .count();
    (before + after).saturating_sub(usize::from(marker_is_escaped(source, position - before)))
}

fn italic_marker_runs_are_odd(opening: usize, closing: usize) -> bool {
    opening % 2 == 1 && closing % 2 == 1
}

fn empty_emphasis_in_unsupported_context(source: &str, selection: &std::ops::Range<usize>) -> bool {
    let only_whitespace = source
        .chars()
        .skip(selection.start)
        .take(selection.end.saturating_sub(selection.start))
        .all(char::is_whitespace);
    if !only_whitespace {
        return false;
    }
    let position = source
        .char_indices()
        .nth(selection.end)
        .map_or(source.len(), |(byte, _)| byte);
    let bytes = source.as_bytes();
    let mut adjacent_runs = Vec::new();
    for candidate in [position.checked_sub(1), Some(position)]
        .into_iter()
        .flatten()
    {
        let Some(&marker) = bytes.get(candidate) else {
            continue;
        };
        if !matches!(marker, b'*' | b'_' | b'~') {
            continue;
        }
        let mut start = candidate;
        let mut end = candidate + 1;
        while start > 0 && bytes[start - 1] == marker {
            start -= 1;
        }
        while end < bytes.len() && bytes[end] == marker {
            end += 1;
        }
        if !adjacent_runs.contains(&(start..end)) {
            adjacent_runs.push(start..end);
        }
    }
    if !preview_allowed(source) {
        return true;
    }
    Parser::new_ext(source, crate::legal_markdown::OPTIONS)
        .into_offset_iter()
        .any(|(event, range)| {
            let inside = range.start < position && position < range.end;
            match event {
                Event::Code(_) | Event::Html(_) | Event::InlineHtml(_) => inside,
                Event::Start(Tag::CodeBlock(_)) => inside,
                Event::Start(Tag::Strong | Tag::Emphasis | Tag::Strikethrough) => {
                    adjacent_runs.iter().any(|run| {
                        run.contains(&range.start)
                            || range
                                .end
                                .checked_sub(1)
                                .is_some_and(|last| run.contains(&last))
                    })
                }
                _ => false,
            }
        })
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
        let opening = unescaped_asterisk_run(source, start);
        let closing = unescaped_asterisk_run(source, end - delimiter.len());
        if !marker_is_escaped(source, start)
            && !marker_is_escaped(source, end - delimiter.len())
            && (delimiter != "*" || italic_marker_runs_are_odd(opening, closing))
        {
            let first = source[..start].chars().count();
            return Some((
                format!("{}{}{}", &source[..start], inner, &source[end..]),
                first..first + inner.chars().count(),
            ));
        }
    }
    if source[..start].ends_with(delimiter) && source[end..].starts_with(delimiter) {
        let opening = if start == end {
            source[..start]
                .bytes()
                .rev()
                .take_while(|byte| *byte == b'*')
                .count()
        } else {
            unescaped_asterisk_run(source, start - delimiter.len())
        };
        let closing = if start == end {
            source[end..]
                .bytes()
                .take_while(|byte| *byte == b'*')
                .count()
        } else {
            unescaped_asterisk_run(source, end)
        };
        if !marker_is_escaped(source, start - delimiter.len())
            && !marker_is_escaped(source, end)
            && (delimiter != "*" || italic_marker_runs_are_odd(opening, closing))
        {
            let prefix = &source[..start - delimiter.len()];
            let suffix = &source[end + delimiter.len()..];
            let first = prefix.chars().count();
            return Some((
                format!("{prefix}{selected}{suffix}"),
                first..first + selected.chars().count(),
            ));
        }
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

fn projection_is_preserved(source: &str, replacement: &str) -> bool {
    let Some(before) = thinoda_core::notes::plain_text_projection(source) else {
        return false;
    };
    thinoda_core::notes::plain_text_projection(replacement).as_deref() == Some(before.as_str())
}

fn emphasis_is_rendered(
    source: &str,
    replacement: &str,
    selection: &std::ops::Range<usize>,
    format: Format,
) -> bool {
    if selection.start > selection.end {
        return false;
    }
    if selection.end > replacement.chars().count() {
        return false;
    }
    if selection.is_empty() {
        return true;
    }
    let Some(before) = thinoda_core::notes::plain_text_projection(source) else {
        return false;
    };
    if thinoda_core::notes::plain_text_projection(replacement).as_deref() != Some(before.as_str()) {
        return false;
    }
    let byte_index = |index| {
        replacement
            .char_indices()
            .nth(index)
            .map_or(replacement.len(), |(byte, _)| byte)
    };
    let selected = byte_index(selection.start)..byte_index(selection.end);
    let events: Vec<_> = Parser::new_ext(replacement, crate::legal_markdown::OPTIONS)
        .into_offset_iter()
        .collect();
    events.iter().any(|(event, outer)| {
        let is_requested_style = matches!(
            (format, event),
            (Format::Bold, Event::Start(Tag::Strong))
                | (Format::Italic, Event::Start(Tag::Emphasis))
        );
        if !is_requested_style || outer.start > selected.start || outer.end < selected.end {
            return false;
        }
        let mut visible = false;
        for (event, range) in &events {
            let is_visible = match event {
                Event::Text(text) | Event::Code(text) => !text.is_empty(),
                Event::Html(text) | Event::InlineHtml(text) => {
                    !text.is_empty() && !text.trim_start().starts_with("<!--")
                }
                Event::SoftBreak | Event::HardBreak | Event::FootnoteReference(_) => true,
                _ => false,
            };
            if !is_visible || range.start < outer.start || range.end > outer.end {
                continue;
            }
            visible = true;
            if range.start < selected.start || range.end > selected.end {
                return false;
            }
        }
        visible
    })
}

const BOLD_STYLE: u8 = 1;
const ITALIC_STYLE: u8 = 2;
const STRIKE_STYLE: u8 = 4;
const CODE_STYLE: u8 = 8;

#[derive(Clone, Copy)]
struct StyledCharacter {
    character: char,
    style: u8,
    selected: bool,
}

#[derive(Default)]
struct StyledDocument {
    characters: Vec<StyledCharacter>,
    structure: Vec<(usize, String)>,
}

fn character_byte_range(
    source: &str,
    range: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    if range.start > range.end || range.end > source.chars().count() {
        return None;
    }
    let byte_index = |index| {
        source
            .char_indices()
            .nth(index)
            .map_or(source.len(), |(byte, _)| byte)
    };
    Some(byte_index(range.start)..byte_index(range.end))
}

fn styled_document(
    source: &str,
    selection: Option<&std::ops::Range<usize>>,
) -> Option<StyledDocument> {
    fn append(
        result: &mut Vec<StyledCharacter>,
        source: &str,
        text: &str,
        range: &std::ops::Range<usize>,
        selection: Option<&std::ops::Range<usize>>,
        style: u8,
    ) -> Option<()> {
        let exact_offsets = source.get(range.clone())? == text;
        let contained = selection.is_some_and(|s| range.start >= s.start && range.end <= s.end);
        let disjoint = selection.is_none_or(|s| range.end <= s.start || range.start >= s.end);
        if !exact_offsets && !contained && !disjoint {
            return None;
        }
        for (offset, character) in text.char_indices() {
            let selected = if exact_offsets {
                selection.is_some_and(|s| {
                    let start = range.start + offset;
                    start >= s.start && start + character.len_utf8() <= s.end
                })
            } else {
                contained
            };
            result.push(StyledCharacter {
                character,
                style,
                selected,
            });
        }
        Some(())
    }

    let mut result = StyledDocument::default();
    let mut bold = 0usize;
    let mut italic = 0usize;
    let mut strike = 0usize;
    let mut code = 0usize;
    for (event, range) in Parser::new_ext(source, crate::legal_markdown::OPTIONS).into_offset_iter()
    {
        match event {
            Event::Start(tag) => {
                match &tag {
                    Tag::Strong => bold += 1,
                    Tag::Emphasis => italic += 1,
                    Tag::Strikethrough => strike += 1,
                    Tag::CodeBlock(_) => code += 1,
                    _ => {}
                }
                if !matches!(tag, Tag::Strong | Tag::Emphasis | Tag::Strikethrough) {
                    result
                        .structure
                        .push((result.characters.len(), format!("+{tag:?}")));
                }
            }
            Event::End(tag) => {
                match tag {
                    TagEnd::Strong => bold -= 1,
                    TagEnd::Emphasis => italic -= 1,
                    TagEnd::Strikethrough => strike -= 1,
                    TagEnd::CodeBlock => code -= 1,
                    _ => {}
                }
                if !matches!(
                    tag,
                    TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough
                ) {
                    result
                        .structure
                        .push((result.characters.len(), format!("-{tag:?}")));
                }
            }
            event => {
                let style = (u8::from(bold > 0) * BOLD_STYLE)
                    | (u8::from(italic > 0) * ITALIC_STYLE)
                    | (u8::from(strike > 0) * STRIKE_STYLE)
                    | (u8::from(code > 0) * CODE_STYLE);
                match event {
                    Event::Text(text) => {
                        append(
                            &mut result.characters,
                            source,
                            &text,
                            &range,
                            selection,
                            style,
                        )?;
                    }
                    Event::Code(text) => {
                        append(
                            &mut result.characters,
                            source,
                            &text,
                            &range,
                            selection,
                            style | CODE_STYLE,
                        )?;
                    }
                    Event::Html(text) | Event::InlineHtml(text)
                        if !text.trim_start().starts_with("<!--") =>
                    {
                        append(
                            &mut result.characters,
                            source,
                            &text,
                            &range,
                            selection,
                            style,
                        )?;
                    }
                    Event::SoftBreak | Event::HardBreak => {
                        let text = if matches!(event, Event::SoftBreak) {
                            " "
                        } else {
                            "\n"
                        };
                        append(
                            &mut result.characters,
                            source,
                            text,
                            &range,
                            selection,
                            style,
                        )?;
                    }
                    Event::FootnoteReference(name) => {
                        append(
                            &mut result.characters,
                            source,
                            &format!("[{name}]"),
                            &range,
                            selection,
                            style,
                        )?;
                    }
                    Event::Rule | Event::TaskListMarker(_) => {
                        result
                            .structure
                            .push((result.characters.len(), format!("{event:?}")));
                    }
                    _ => {}
                }
            }
        }
    }
    Some(result)
}

fn unchanged_without_selected_characters(before: &StyledDocument, after: &StyledDocument) -> bool {
    let visible = |document: &StyledDocument| {
        document
            .characters
            .iter()
            .filter(|character| !character.selected)
            .map(|character| (character.character, character.style))
            .collect::<Vec<_>>()
    };
    if visible(before) != visible(after) || before.structure.len() != after.structure.len() {
        return false;
    }
    let offsets = |document: &StyledDocument| {
        let mut count = 0usize;
        std::iter::once(0)
            .chain(document.characters.iter().map(|character| {
                count += usize::from(!character.selected);
                count
            }))
            .collect::<Vec<_>>()
    };
    let before_offsets = offsets(before);
    let after_offsets = offsets(after);
    before.structure.iter().zip(&after.structure).all(
        |((before_index, before_tag), (after_index, after_tag))| {
            before_offsets[*before_index] == after_offsets[*after_index] && before_tag == after_tag
        },
    )
}

fn empty_emphasis_preserves_existing_content(
    source: &str,
    replacement: &str,
    previous_selection: &std::ops::Range<usize>,
    selection: &std::ops::Range<usize>,
    format: Format,
    adding: bool,
) -> bool {
    if !preview_allowed(source) || !preview_allowed(replacement) {
        return false;
    }
    let delimiter = match format {
        Format::Bold => "**",
        Format::Italic => "*",
        Format::Strike => "~~",
        Format::Bullet | Format::Numbered => return false,
    };
    let (marked, caret) = if adding {
        (replacement, selection.start)
    } else {
        (source, previous_selection.start)
    };
    let Some(start) = caret.checked_sub(delimiter.len()) else {
        return false;
    };
    let Some(end) = caret.checked_add(delimiter.len()) else {
        return false;
    };
    let Some(markers) = character_byte_range(marked, &(start..end)) else {
        return false;
    };
    if marked.get(markers.clone()) != Some(format!("{delimiter}{delimiter}").as_str()) {
        return false;
    }
    let before = styled_document(source, (!adding).then_some(&markers));
    let after = styled_document(replacement, adding.then_some(&markers));
    match (before, after) {
        (Some(before), Some(after)) => unchanged_without_selected_characters(&before, &after),
        _ => false,
    }
}

fn emphasis_styles_are_preserved(
    source: &str,
    replacement: &str,
    selection: &std::ops::Range<usize>,
    format: Format,
    adding: bool,
) -> bool {
    let target = match format {
        Format::Bold => BOLD_STYLE,
        Format::Italic => ITALIC_STYLE,
        Format::Strike => STRIKE_STYLE,
        Format::Bullet | Format::Numbered => return false,
    };
    let Some(selection) = character_byte_range(replacement, selection) else {
        return false;
    };
    let Some(before) = styled_document(source, None) else {
        return false;
    };
    let Some(after) = styled_document(replacement, Some(&selection)) else {
        return false;
    };
    if before.characters.len() != after.characters.len() || before.structure != after.structure {
        return false;
    }
    let mut changed = false;
    for (before, after) in before.characters.iter().zip(&after.characters) {
        if before.character != after.character || (before.style ^ after.style) & !target != 0 {
            return false;
        }
        let was_styled = before.style & target != 0;
        let is_styled = after.style & target != 0;
        if after.selected {
            if adding && !is_styled || !adding && !was_styled && is_styled {
                return false;
            }
            changed |= was_styled != is_styled;
        } else if was_styled != is_styled {
            return false;
        }
    }
    changed
}

fn emphasis_change_is_valid(
    source: &str,
    replacement: &str,
    previous_selection: &std::ops::Range<usize>,
    selection: &std::ops::Range<usize>,
    format: Format,
) -> bool {
    if !matches!(format, Format::Bold | Format::Italic | Format::Strike) {
        return true;
    }
    if empty_emphasis_in_unsupported_context(source, previous_selection) {
        return false;
    }
    if replacement.len() > source.len() {
        if selection.is_empty() {
            return empty_emphasis_preserves_existing_content(
                source,
                replacement,
                previous_selection,
                selection,
                format,
                true,
            );
        }
        let rendered = match format {
            Format::Bold | Format::Italic => {
                emphasis_is_rendered(source, replacement, selection, format)
            }
            Format::Strike => {
                strikethrough_is_rendered(replacement, selection)
                    && projection_is_preserved(source, replacement)
            }
            Format::Bullet | Format::Numbered => unreachable!(),
        };
        rendered && emphasis_styles_are_preserved(source, replacement, selection, format, true)
    } else if replacement.len() < source.len() {
        if previous_selection.is_empty() {
            empty_emphasis_preserves_existing_content(
                source,
                replacement,
                previous_selection,
                selection,
                format,
                false,
            )
        } else {
            projection_is_preserved(source, replacement)
                && emphasis_styles_are_preserved(source, replacement, selection, format, false)
        }
    } else {
        true
    }
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
    let feedback_id = response.id.with("format-feedback");
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
                let invalid = !emphasis_change_is_valid(
                    &source,
                    &replacement,
                    &previous_selection,
                    &selection,
                    format,
                );
                let selection = if invalid {
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
                if invalid {
                    ui.data_mut(|data| {
                        data.insert_temp(response.id, source.clone());
                        data.insert_temp(feedback_id, (selection_owner, EMPHASIS_UNAVAILABLE));
                    });
                    *done = None;
                    continue;
                }
                ui.data_mut(|data| data.remove::<(Id, &'static str)>(feedback_id));
                source = replacement;
                // Commit through the same conflict-checked inspector path as typing.
                ui.data_mut(|data| data.insert_temp(response.id, source.clone()));
                *done = Some(source.clone());
                formatted = true;
            }
        }
    });
    if let Some((owner, message)) = ui.data(|data| data.get_temp::<(Id, &'static str)>(feedback_id))
        && owner == selection_owner
    {
        ui.colored_label(ui.visuals().warn_fg_color, t(message));
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
    fn formatting_feedback_exists_in_every_catalog() {
        for language in crate::i18n::Lang::ALL {
            for key in ["Strikethrough notes", EMPHASIS_UNAVAILABLE] {
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
    fn italic_does_not_remove_bold_or_escaped_markers() {
        assert_eq!(
            format_selection("**café**", 2..6, Format::Italic),
            ("***café***".into(), 3..7)
        );
        assert_eq!(
            format_selection("**café**", 0..8, Format::Italic),
            ("***café***".into(), 1..9)
        );
        assert_eq!(
            format_selection("**café**", 1..7, Format::Italic),
            ("***café***".into(), 2..8)
        );
        assert_eq!(
            format_selection("***café***", 3..7, Format::Italic),
            ("**café**".into(), 2..6)
        );
        assert_eq!(
            format_selection("***café***", 3..7, Format::Bold),
            ("*café*".into(), 1..5)
        );
        let source = r"\*café*";
        let (formatted, inner) = format_selection(source, 2..6, Format::Italic);
        assert_eq!(formatted, r"\**café**");
        assert!(emphasis_is_rendered(
            source,
            &formatted,
            &inner,
            Format::Italic
        ));
    }

    #[test]
    fn asterisk_runs_respect_escaped_markers_at_each_boundary() {
        for (source, positions, expected) in [
            (r"\***x", 1..=4, 2),
            (r"z\\***x", 3..=6, 3),
            ("é\\***x", 3..=6, 2),
        ] {
            for position in positions {
                assert_eq!(
                    unescaped_asterisk_run(source, position),
                    expected,
                    "{source:?} at byte {position}"
                );
            }
        }
    }

    #[test]
    fn empty_emphasis_avoids_code_and_existing_marker_runs() {
        for (source, selection) in [
            ("**café**", 1..1),
            ("**café**", 2..2),
            ("**café**", 6..6),
            ("***café***", 1..1),
            ("***café***", 2..2),
            ("~~café~~", 1..1),
            ("`code`", 3..3),
            ("~~~\n  \n~~~", 4..6),
            ("    code", 6..6),
            ("<span>text</span>", 3..3),
        ] {
            assert!(
                empty_emphasis_in_unsupported_context(source, &selection),
                "{source:?} {selection:?}"
            );
        }
        for (source, selection) in [
            ("a**bc", 2..2),
            ("a  b", 1..3),
            ("café", 0..0),
            ("café", 4..4),
            ("**café**", 4..4),
        ] {
            assert!(
                !empty_emphasis_in_unsupported_context(source, &selection),
                "{source:?} {selection:?}"
            );
        }
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
    fn emphasis_validation_accepts_rendered_nested_styles_without_spill() {
        for (format, source, selection) in [
            (Format::Bold, "café 🌱", 0..6),
            (Format::Italic, "café 🌱", 0..6),
            (Format::Bold, "*café*", 0..6),
            (Format::Italic, "**café**", 2..6),
            (Format::Italic, "**one two**", 6..9),
            (Format::Bold, "one\ntwo", 0..7),
            (Format::Bold, "[café](https://example.invalid)", 0..31),
        ] {
            let (formatted, inner) = format_selection(source, selection, format);
            assert!(
                emphasis_is_rendered(source, &formatted, &inner, format),
                "{format:?}: {source:?} -> {formatted:?}, {inner:?}"
            );
        }
    }

    #[test]
    fn emphasis_validation_rejects_unrenderable_or_bounded_out_selections() {
        for format in [Format::Bold, Format::Italic] {
            for (source, selection) in [
                ("first\n\nsecond", 0..13),
                ("`code`", 1..5),
                ("~~~\ncode\n~~~", 4..8),
                ("*", 0..1),
            ] {
                let (formatted, inner) = format_selection(source, selection, format);
                assert!(
                    !emphasis_is_rendered(source, &formatted, &inner, format),
                    "{format:?}: {source:?} -> {formatted:?}, {inner:?}"
                );
            }
            let large = "x".repeat(thinoda_core::notes::MAX_PROJECTION_BYTES + 1);
            let (formatted, inner) = format_selection(&large, 0..1, format);
            assert!(!emphasis_is_rendered(&large, &formatted, &inner, format));
            let many_events = format!("{}target", "**x** ".repeat(700));
            let last = many_events.chars().count();
            let (formatted, inner) = format_selection(&many_events, last - 6..last, format);
            assert!(!emphasis_is_rendered(
                &many_events,
                &formatted,
                &inner,
                format
            ));
            let deep = format!(
                "{}target",
                "> ".repeat(thinoda_core::notes::MAX_PROJECTION_DEPTH)
            );
            let last = deep.chars().count();
            let (formatted, inner) = format_selection(&deep, last - 6..last, format);
            assert!(!emphasis_is_rendered(&deep, &formatted, &inner, format));
            let (formatted, inner) = format_selection("a  b", 1..3, format);
            assert!(emphasis_is_rendered("a  b", &formatted, &inner, format));
            assert!(!emphasis_is_rendered("a", "a", &(2..2), format));
        }
    }

    #[test]
    fn emphasis_validation_preserves_other_styles_selection_and_structure() {
        let source = "__a__ b";
        let original = 1..4;
        let (replacement, selection) = format_selection(source, original.clone(), Format::Bold);
        assert_eq!(replacement, "_**_a_**_ b");
        assert!(emphasis_is_rendered(
            source,
            &replacement,
            &selection,
            Format::Bold
        ));
        assert!(!emphasis_change_is_valid(
            source,
            &replacement,
            &original,
            &selection,
            Format::Bold
        ));

        assert!(!emphasis_styles_are_preserved(
            "ab",
            "**ab**",
            &(2..3),
            Format::Bold,
            true
        ));
        assert!(emphasis_styles_are_preserved(
            "**ab**",
            "a**b**",
            &(0..1),
            Format::Bold,
            false
        ));
        assert!(!emphasis_styles_are_preserved(
            "**ab**",
            "ab",
            &(0..1),
            Format::Bold,
            false
        ));
        assert!(!emphasis_styles_are_preserved(
            "*a*",
            "~~a~~",
            &(2..3),
            Format::Strike,
            true
        ));
        assert!(!emphasis_styles_are_preserved(
            "[a](x)",
            "**[a](y)**",
            &(2..8),
            Format::Bold,
            true
        ));
        assert!(!emphasis_styles_are_preserved(
            "# a",
            "**a**",
            &(2..3),
            Format::Bold,
            true
        ));

        for (format, source, original) in [
            (Format::Bold, "[café](https://example.invalid)", 0..31),
            (Format::Bold, "x &amp; y", 2..7),
            (Format::Italic, "**one two**", 6..9),
            (Format::Strike, "before **café** after", 7..15),
        ] {
            let (replacement, selection) = format_selection(source, original.clone(), format);
            assert!(
                emphasis_change_is_valid(source, &replacement, &original, &selection, format),
                "{format:?} {source:?} {original:?} -> {replacement:?} {selection:?}"
            );
        }
    }

    #[test]
    fn empty_emphasis_is_rejected_when_preview_context_exceeds_bounds() {
        let plain = "x".repeat(thinoda_core::notes::MAX_PROJECTION_BYTES + 1);
        let code = format!("`{plain}`");
        for source in [&plain, &code] {
            for format in [Format::Bold, Format::Italic, Format::Strike] {
                let original = 2..2;
                let (replacement, selection) = format_selection(source, original.clone(), format);
                assert!(!emphasis_change_is_valid(
                    source,
                    &replacement,
                    &original,
                    &selection,
                    format
                ));
            }
        }
    }

    #[test]
    fn emphasis_checks_both_source_and_result_at_the_exact_byte_limit() {
        let limit = thinoda_core::notes::MAX_PROJECTION_BYTES;
        for (length, expected) in [(limit - 4, true), (limit - 3, false)] {
            let source = "x".repeat(length);
            for original in [1..1, 1..2] {
                let (replacement, selection) =
                    format_selection(&source, original.clone(), Format::Bold);
                assert_eq!(
                    emphasis_change_is_valid(
                        &source,
                        &replacement,
                        &original,
                        &selection,
                        Format::Bold
                    ),
                    expected,
                    "source bytes: {length}, selection: {original:?}"
                );
            }
        }
        for (length, expected) in [(limit, true), (limit + 1, false)] {
            let source = format!("**x**{}", "x".repeat(length - 5));
            let original = 2..3;
            let (replacement, selection) =
                format_selection(&source, original.clone(), Format::Bold);
            assert_eq!(
                emphasis_change_is_valid(
                    &source,
                    &replacement,
                    &original,
                    &selection,
                    Format::Bold
                ),
                expected,
                "source bytes: {length}"
            );
        }
    }

    #[test]
    fn empty_emphasis_does_not_change_existing_rendered_content() {
        for format in [Format::Bold, Format::Italic, Format::Strike] {
            for (source, original) in [("abcd", 2..2), ("a  b", 1..3)] {
                let (replacement, selection) = format_selection(source, original.clone(), format);
                assert!(
                    emphasis_change_is_valid(source, &replacement, &original, &selection, format),
                    "{format:?} {source:?} -> {replacement:?}"
                );
                let (restored, restored_selection) =
                    format_selection(&replacement, selection.clone(), format);
                assert_eq!(restored, source);
                assert!(
                    emphasis_change_is_valid(
                        &replacement,
                        &restored,
                        &selection,
                        &restored_selection,
                        format
                    ),
                    "{format:?} {replacement:?} -> {restored:?}"
                );
            }
        }
        let source = "**café**";
        let original = 4..4;
        let (replacement, selection) = format_selection(source, original.clone(), Format::Italic);
        assert_eq!(replacement, "**ca**fé**");
        assert!(!emphasis_change_is_valid(
            source,
            &replacement,
            &original,
            &selection,
            Format::Italic
        ));
    }

    #[test]
    fn accepted_emphasis_does_not_change_other_rendered_styles() {
        let fixtures = [
            "abc",
            "*abc*",
            "**abc**",
            "***abc***",
            "a *bc* d",
            "**ab** cd",
            "*a* **b** c",
            "a `bc` d",
            "a ~~bc~~ d",
            "a \\* b",
            "__a__ b",
            "a **b *c* d** e",
            "*a **b** c* d",
        ];
        let mut accepted = [0usize; 3];
        let rendered = |source: &str| {
            thinoda_core::notes::formatted_document(source)
                .ok()
                .map(|doc| {
                    doc.paragraphs
                        .into_iter()
                        .flat_map(|paragraph| paragraph.runs)
                        .flat_map(|run| {
                            run.text
                                .chars()
                                .map(|character| (character, run.style))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
        };
        for source in fixtures {
            let Some(before) = rendered(source) else {
                continue;
            };
            for start in 0..source.chars().count() {
                for end in start + 1..=source.chars().count() {
                    for (index, format) in [Format::Bold, Format::Italic, Format::Strike]
                        .into_iter()
                        .enumerate()
                    {
                        let previous_selection = start..end;
                        let (replacement, selection) =
                            format_selection(source, previous_selection.clone(), format);
                        if selection.is_empty() {
                            continue;
                        }
                        if !emphasis_change_is_valid(
                            source,
                            &replacement,
                            &previous_selection,
                            &selection,
                            format,
                        ) {
                            continue;
                        }
                        let Some(after) = rendered(&replacement) else {
                            continue;
                        };
                        accepted[index] += 1;
                        assert_eq!(
                            before.iter().map(|(ch, _)| ch).collect::<String>(),
                            after.iter().map(|(ch, _)| ch).collect::<String>(),
                            "{format:?} {source:?} {start}..{end} -> {replacement:?}"
                        );
                        for ((_, old), (_, new)) in before.iter().zip(&after) {
                            assert_eq!(
                                old.code, new.code,
                                "{format:?} {source:?} {start}..{end} -> {replacement:?}"
                            );
                            match format {
                                Format::Bold => assert_eq!(
                                    (old.italic, old.strike),
                                    (new.italic, new.strike),
                                    "{format:?} {source:?} {start}..{end} -> {replacement:?}"
                                ),
                                Format::Italic => assert_eq!(
                                    (old.bold, old.strike),
                                    (new.bold, new.strike),
                                    "{format:?} {source:?} {start}..{end} -> {replacement:?}"
                                ),
                                Format::Strike => assert_eq!(
                                    (old.bold, old.italic),
                                    (new.bold, new.italic),
                                    "{format:?} {source:?} {start}..{end} -> {replacement:?}"
                                ),
                                _ => unreachable!(),
                            }
                        }
                    }
                    assert!(accepted.into_iter().all(|count| count > 0));
                }
            }
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
