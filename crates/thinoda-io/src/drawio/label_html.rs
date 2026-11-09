//! Convert draw.io HTML labels to inert, editable plain text.

use super::{ImportWarning, MAX_BYTES, MAX_XML_ELEMENTS};
use lol_html::{
    HtmlRewriter, MemorySettings, Settings, doc_text, element, end_tag, html_content::TextType,
};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

#[derive(Default)]
struct LabelText {
    output: String,
    text_node: String,
    pending_breaks: u8,
    hidden: usize,
    open_blocks: usize,
    formatting_loss: bool,
}

impl LabelText {
    fn block_boundary(&mut self, breaks: u8) {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.pending_breaks = self.pending_breaks.max(breaks);
        }
    }

    fn flush_boundary(&mut self) {
        for _ in 0..self.pending_breaks {
            self.output.push('\n');
        }
        self.pending_breaks = 0;
    }

    fn line_break(&mut self) {
        self.flush_boundary();
        self.output.push('\n');
    }

    fn text_chunk(&mut self, text: &str, last: bool, decode_references: bool) {
        self.text_node.push_str(text);
        if !last {
            return;
        }
        let text = std::mem::take(&mut self.text_node);
        if text.is_empty() {
            return;
        }
        self.flush_boundary();
        if decode_references {
            self.formatting_loss |= unresolved_entity(&text);
            self.output.push_str(&decode_entities(&text));
        } else {
            self.output.push_str(&text);
        }
    }
}

fn numeric_replacement(value: Option<u32>) -> Option<char> {
    Some(match value {
        None | Some(0 | 0xd800..=0xdfff | 0x110000..) => '\u{fffd}',
        Some(0x80) => '\u{20ac}',
        Some(0x82) => '\u{201a}',
        Some(0x83) => '\u{0192}',
        Some(0x84) => '\u{201e}',
        Some(0x85) => '\u{2026}',
        Some(0x86) => '\u{2020}',
        Some(0x87) => '\u{2021}',
        Some(0x88) => '\u{02c6}',
        Some(0x89) => '\u{2030}',
        Some(0x8a) => '\u{0160}',
        Some(0x8b) => '\u{2039}',
        Some(0x8c) => '\u{0152}',
        Some(0x8e) => '\u{017d}',
        Some(0x91) => '\u{2018}',
        Some(0x92) => '\u{2019}',
        Some(0x93) => '\u{201c}',
        Some(0x94) => '\u{201d}',
        Some(0x95) => '\u{2022}',
        Some(0x96) => '\u{2013}',
        Some(0x97) => '\u{2014}',
        Some(0x98) => '\u{02dc}',
        Some(0x99) => '\u{2122}',
        Some(0x9a) => '\u{0161}',
        Some(0x9b) => '\u{203a}',
        Some(0x9c) => '\u{0153}',
        Some(0x9e) => '\u{017e}',
        Some(0x9f) => '\u{0178}',
        _ => return None,
    })
}

fn decode_entities(text: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    for (start, _) in text.match_indices("&#") {
        let tail = &text[start + 2..];
        let (digits, radix, prefix_len) = match tail.strip_prefix(['x', 'X']) {
            Some(digits) => (digits, 16, 1),
            None => (tail, 10, 0),
        };
        let len = digits
            .bytes()
            .take_while(|byte| match radix {
                16 => byte.is_ascii_hexdigit(),
                _ => byte.is_ascii_digit(),
            })
            .count();
        if len == 0 || digits.as_bytes().get(len) != Some(&b';') {
            continue;
        }
        let value = u32::from_str_radix(&digits[..len], radix).ok();
        let Some(replacement) = numeric_replacement(value) else {
            continue;
        };
        output.push_str(&html_escape::decode_html_entities(&text[cursor..start]));
        output.push(replacement);
        cursor = start + 2 + prefix_len + len + 1;
    }
    output.push_str(&html_escape::decode_html_entities(&text[cursor..]));
    output
}

// `html-escape` intentionally only decodes references terminated by a semicolon.
// Report other references that a browser may interpret rather than claiming fidelity.
fn unresolved_entity(text: &str) -> bool {
    text.match_indices('&').any(|(start, _)| {
        let tail = &text[start + 1..];
        if tail.starts_with('#') {
            let end = tail
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'#')
                .count();
            if tail.as_bytes().get(end) == Some(&b';') {
                let reference = &text[start..start + end + 2];
                let decoded = html_escape::decode_html_entities(reference);
                return decoded == reference || decoded.contains(['\u{fffe}', '\u{ffff}']);
            }
            return true;
        }

        let len = tail
            .bytes()
            .take_while(u8::is_ascii_alphanumeric)
            .take(32)
            .count();
        if len == 0 {
            return false;
        }
        if tail.as_bytes().get(len) == Some(&b';') {
            let reference = &text[start..start + len + 2];
            if html_escape::decode_html_entities(reference) != reference {
                return false;
            }
        }
        for end in 1..=len {
            let prefix = &tail.as_bytes()[..end];
            let index = html_escape::NAMED_ENTITIES.partition_point(|(name, _)| *name < prefix);
            let Some((name, _)) = html_escape::NAMED_ENTITIES.get(index) else {
                break;
            };
            if *name == prefix {
                return true;
            }
            if !name.starts_with(prefix) {
                break;
            }
        }
        false
    })
}

/// Retain text and line boundaries without executing or loading label content.
pub(super) fn strip_html(text: &str, warnings: &mut BTreeSet<ImportWarning>) -> String {
    strip_html_with_limit(text, warnings, MAX_BYTES)
}

fn strip_html_with_limit(
    text: &str,
    warnings: &mut BTreeSet<ImportWarning>,
    memory_limit: usize,
) -> String {
    if !text.contains('<') {
        if unresolved_entity(text) {
            warnings.insert(ImportWarning::Formatting);
        }
        return decode_entities(text);
    }

    let result = Rc::new(RefCell::new(LabelText::default()));
    let parsed = {
        let elements = Rc::clone(&result);
        let chunks = Rc::clone(&result);
        let mut element_count = 0usize;
        let settings = Settings::new()
            .with_memory_settings(MemorySettings::new().with_max_allowed_memory_usage(memory_limit))
            .append_element_content_handler(element!("*", move |el| {
                element_count += 1;
                if element_count > MAX_XML_ELEMENTS {
                    return Err(std::io::Error::other("HTML label element limit").into());
                }

                let name = el.tag_name();
                let hide = matches!(name.as_str(), "script" | "style" | "template")
                    || el.has_attribute("hidden");
                if hide {
                    let mut state = elements.borrow_mut();
                    state.formatting_loss = true;
                    if el.can_have_content() {
                        state.hidden += 1;
                        let state = Rc::clone(&elements);
                        el.on_end_tag(end_tag!(move |_| {
                            let mut state = state.borrow_mut();
                            state.hidden -= 1;
                            Ok(())
                        }))?;
                    }
                    return Ok(());
                }

                let mut state = elements.borrow_mut();
                if state.hidden > 0 {
                    return Ok(());
                }
                if !el.attributes().is_empty() || !matches!(name.as_str(), "br" | "div" | "span") {
                    state.formatting_loss = true;
                }
                if name == "br" {
                    state.line_break();
                    return Ok(());
                }

                let breaks = match name.as_str() {
                    "p" => 2,
                    "div" | "li" | "ul" | "ol" | "blockquote" | "pre" | "table" | "tr" | "h1"
                    | "h2" | "h3" | "h4" | "h5" | "h6" => 1,
                    _ => 0,
                };
                if breaks > 0 && el.can_have_content() {
                    state.block_boundary(breaks);
                    state.open_blocks += 1;
                    let state = Rc::clone(&elements);
                    el.on_end_tag(end_tag!(move |_| {
                        let mut state = state.borrow_mut();
                        state.block_boundary(breaks);
                        state.open_blocks -= 1;
                        Ok(())
                    }))?;
                }
                Ok(())
            }))
            .append_document_content_handler(doc_text!(move |chunk| {
                let mut state = chunks.borrow_mut();
                if state.hidden == 0 {
                    let decode = matches!(chunk.text_type(), TextType::Data | TextType::RCData);
                    state.text_chunk(chunk.as_str(), chunk.last_in_text_node(), decode);
                }
                Ok(())
            }));
        let mut rewriter = HtmlRewriter::new(settings, |_: &[u8]| {});
        (|| {
            for chunk in text.as_bytes().chunks(16 * 1024) {
                rewriter.write(chunk)?;
            }
            rewriter.end()
        })()
    };

    if parsed.is_err() {
        warnings.insert(ImportWarning::Formatting);
        warnings.insert(ImportWarning::Other);
        return text.to_owned();
    }
    let mut result = result.borrow_mut();
    if result.formatting_loss || result.open_blocks > 0 || result.hidden > 0 {
        warnings.insert(ImportWarning::Formatting);
    }
    std::mem::take(&mut result.output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_limit_keeps_inert_source_and_reports_the_loss() {
        let source = format!("<span title=\"{}\">visible</span>", "x".repeat(4096));
        let mut warnings = BTreeSet::new();
        assert_eq!(strip_html_with_limit(&source, &mut warnings, 1024), source);
        assert_eq!(
            warnings,
            BTreeSet::from([ImportWarning::Formatting, ImportWarning::Other])
        );
    }
}
