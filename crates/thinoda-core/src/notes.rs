//! Lossless versioned note sources and bounded, non-mutating Markdown projections.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

/// Versioned source syntax. Unknown versions are rejected rather than reinterpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteFormat {
    #[serde(rename = "markdown-v1")]
    MarkdownV1,
}

/// Legacy strings remain literal; explicit formatting uses a versioned envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum NoteContent {
    Plain(String),
    Formatted { format: NoteFormat, source: String },
}

impl NoteContent {
    pub fn markdown(source: String) -> Self {
        Self::Formatted {
            format: NoteFormat::MarkdownV1,
            source,
        }
    }

    pub fn is_markdown(&self) -> bool {
        matches!(self, Self::Formatted { .. })
    }

    /// Editing the source never silently changes its declared syntax.
    pub fn with_source(&self, source: String) -> Self {
        if self.is_markdown() {
            Self::markdown(source)
        } else {
            Self::Plain(source)
        }
    }
}

impl From<String> for NoteContent {
    fn from(source: String) -> Self {
        Self::Plain(source)
    }
}

impl From<&str> for NoteContent {
    fn from(source: &str) -> Self {
        Self::Plain(source.to_owned())
    }
}

impl std::ops::Deref for NoteContent {
    type Target = str;

    fn deref(&self) -> &str {
        match self {
            Self::Plain(source) | Self::Formatted { source, .. } => source,
        }
    }
}

impl std::fmt::Display for NoteContent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self)
    }
}

pub fn has_formatted_notes(book: &crate::Workbook) -> bool {
    book.sheets.iter().any(|sheet| {
        sheet
            .topics
            .values()
            .any(|topic| topic.notes.as_ref().is_some_and(NoteContent::is_markdown))
    })
}

pub const MAX_PROJECTION_BYTES: usize = 16 * 1024;
pub const MAX_PROJECTION_EVENTS: usize = 2048;
pub const MAX_PROJECTION_DEPTH: usize = 32;

const OPTIONS: pulldown_cmark::Options = pulldown_cmark::Options::ENABLE_TABLES
    .union(pulldown_cmark::Options::ENABLE_STRIKETHROUGH)
    .union(pulldown_cmark::Options::ENABLE_TASKLISTS)
    .union(pulldown_cmark::Options::ENABLE_FOOTNOTES);

/// Returns visible note text for search/copy, or `None` when a safe bounded
/// projection is unavailable. The original source is never altered.
pub fn plain_text_projection(source: &str) -> Option<String> {
    if source.len() > MAX_PROJECTION_BYTES {
        return None;
    }
    let mut depth = 0usize;
    for (count, event) in Parser::new_ext(source, OPTIONS).enumerate() {
        if count >= MAX_PROJECTION_EVENTS {
            return None;
        }
        match event {
            Event::Start(_) => {
                depth += 1;
                if depth > MAX_PROJECTION_DEPTH {
                    return None;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    let mut output = String::new();
    let newline = |output: &mut String| {
        while output.ends_with('\t') {
            output.pop();
        }
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
    };
    for event in Parser::new_ext(source, OPTIONS) {
        match event {
            Event::Text(text)
            | Event::Code(text)
            | Event::Html(text)
            | Event::InlineHtml(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text) => output.push_str(&text),
            Event::SoftBreak | Event::HardBreak => output.push('\n'),
            Event::Start(
                Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::Item
                | Tag::CodeBlock(_)
                | Tag::BlockQuote(_),
            ) => newline(&mut output),
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::BlockQuote(_)
                | TagEnd::TableRow
                | TagEnd::TableHead
                | TagEnd::Table,
            )
            | Event::Rule => newline(&mut output),
            Event::End(TagEnd::TableCell) => output.push('\t'),
            Event::TaskListMarker(checked) => {
                output.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::FootnoteReference(name) => {
                output.push('[');
                output.push_str(&name);
                output.push(']');
            }
            Event::Start(Tag::FootnoteDefinition(name)) => {
                newline(&mut output);
                output.push('[');
                output.push_str(&name);
                output.push_str("] ");
            }
            _ => {}
        }
    }
    Some(output.trim_end_matches(['\n', '\t']).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_notes_stay_strings_and_formatted_sources_are_versioned() {
        let source = "**literal**\r\n\n日本語 <tag> [label](file:///private)";
        let legacy: NoteContent =
            serde_json::from_str(&serde_json::to_string(source).unwrap()).unwrap();
        assert_eq!(legacy, NoteContent::Plain(source.into()));
        assert_eq!(serde_json::to_value(&legacy).unwrap(), source);
        let formatted = NoteContent::markdown(source.into());
        assert_eq!(
            serde_json::to_value(&formatted).unwrap(),
            serde_json::json!({"format": "markdown-v1", "source": source})
        );
        assert_eq!(
            serde_json::from_value::<NoteContent>(serde_json::to_value(&formatted).unwrap())
                .unwrap(),
            formatted
        );
        for invalid in [
            serde_json::json!({"format": "markdown-v2", "source": source}),
            serde_json::json!({"format": "markdown-v1"}),
            serde_json::json!({"format": "markdown-v1", "source": source, "unknown": true}),
        ] {
            assert!(serde_json::from_value::<NoteContent>(invalid).is_err());
        }
    }

    #[test]
    fn editing_deleting_undo_and_clipboard_preserve_explicit_format() {
        let mut doc = crate::Document::new(crate::Workbook::default());
        let root = doc.sheet().root;
        doc.edit_sheet("Plain note", |sheet| {
            sheet.set_notes(root, Some("**literal**".into()))
        })
        .unwrap();
        assert!(
            !doc.sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap()
                .is_markdown()
        );
        doc.edit_sheet("Format note", |sheet| {
            sheet.set_markdown_notes(root, Some("**café**".into()))
        })
        .unwrap();
        doc.edit_sheet("Edit source", |sheet| {
            sheet.set_notes(root, Some("*日本語*".into()))
        })
        .unwrap();
        let sheet = doc.sheet().clone();
        let encoded = crate::clipboard::encode_sheet(&sheet).unwrap();
        assert!(encoded.starts_with("THINODA-SHEET-3\n"));
        assert_eq!(crate::clipboard::decode_sheet(&encoded).unwrap(), sheet);
        assert_eq!(
            crate::clipboard::decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        assert!(
            crate::clipboard::decode_sheet(&encoded.replacen(
                "THINODA-SHEET-3",
                "THINODA-SHEET-2",
                1
            ))
            .is_err()
        );
        let outline = crate::outline::to_outline(&sheet, root);
        assert_eq!(
            crate::outline::sheet_from_outline("Copy", &outline)
                .root_topic()
                .notes,
            sheet.root_topic().notes
        );
        let payload = crate::clipboard::copy_topics(&sheet, &[root]).unwrap();
        assert_eq!(
            payload.subtrees[0]
                .topics
                .iter()
                .find(|topic| topic.id == root)
                .unwrap()
                .notes,
            sheet.root_topic().notes
        );
        let mut destination = crate::Sheet::new("Destination", "Root");
        let pasted = destination
            .paste(destination.root, &payload.subtrees)
            .unwrap();
        assert_eq!(
            destination.topic(pasted[0]).unwrap().notes,
            sheet.root_topic().notes
        );
        doc.edit_sheet("Delete note", |sheet| sheet.set_notes(root, None))
            .unwrap();
        assert!(doc.sheet().root_topic().notes.is_none());
        doc.undo();
        assert_eq!(doc.sheet().root_topic().notes, sheet.root_topic().notes);
        doc.undo();
        assert_eq!(doc.sheet().root_topic().notes.as_deref(), Some("**café**"));
        doc.undo();
        assert_eq!(doc.sheet().root_topic().notes, Some("**literal**".into()));
        doc.redo();
        assert!(
            doc.sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap()
                .is_markdown()
        );
    }

    #[test]
    fn projection_keeps_visible_unicode_code_lists_and_table_boundaries() {
        assert_eq!(
            plain_text_projection("# Café\n\n**Bold** and *日本語* `a*b`\n\n- One\n- Two\n\n[Label](https://example.invalid/private) ![Alt](file:///private.png)\n\n```text\ncode\nline\n```").as_deref(),
            Some("Café\nBold and 日本語 a*b\nOne\nTwo\nLabel Alt\ncode\nline")
        );
        assert_eq!(
            plain_text_projection("- [x] Done\n- [ ] Pending\n\n| A | B |\n| - | - |\n| 1 | 2 |")
                .as_deref(),
            Some("[x] Done\n[ ] Pending\nA\tB\n1\t2")
        );
        assert_eq!(
            plain_text_projection("first  \nsecond\nthird").as_deref(),
            Some("first\nsecond\nthird")
        );
    }

    #[test]
    fn bounds_reject_projection_without_truncating_or_mutating_source() {
        assert!(plain_text_projection(&"x".repeat(MAX_PROJECTION_BYTES)).is_some());
        assert!(plain_text_projection(&"x".repeat(MAX_PROJECTION_BYTES + 1)).is_none());
        assert!(plain_text_projection(&"> ".repeat(MAX_PROJECTION_DEPTH + 1)).is_none());
        assert!(plain_text_projection(&"*a* ".repeat(MAX_PROJECTION_EVENTS)).is_none());
        let source =
            "<script>literal only</script>\n\n$not rendered math$\n\n<!-- literal comment -->";
        assert_eq!(
            plain_text_projection(source).as_deref(),
            Some("<script>literal only</script>\n$not rendered math$\n<!-- literal comment -->")
        );
        assert_eq!(plain_text_projection(""), Some(String::new()));
    }
}
