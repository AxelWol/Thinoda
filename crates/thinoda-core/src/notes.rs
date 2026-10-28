//! Bounded, non-mutating projections for Markdown-authored topic notes.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

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
