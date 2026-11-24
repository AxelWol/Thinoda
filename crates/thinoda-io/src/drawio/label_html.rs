//! Convert draw.io HTML labels to inert, editable text and bounded inline styles.

use super::{ImportWarning, MAX_BYTES, MAX_XML_ELEMENTS};
use lol_html::{
    HtmlRewriter, MemorySettings, Settings, doc_text, element, end_tag, html_content::TextType,
};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};
use thinoda_core::{
    InlineBaseline, InlineParagraph, InlineText, InlineTextRun, InlineTextStyle, ListKind,
    rich_text::{
        MAX_INLINE_FONT_FAMILY_BYTES, MAX_INLINE_FONT_SIZE, MAX_INLINE_TEXT_CHARS,
        MAX_INLINE_TEXT_RUNS, MIN_INLINE_FONT_SIZE,
    },
};

#[derive(Default)]
struct RichLabel {
    text: InlineText,
    style: InlineTextStyle,
    lists: Vec<ListKind>,
    pending_item: Option<ListKind>,
    overflow: bool,
}

#[derive(Default)]
struct LabelText {
    output: String,
    text_node: String,
    scalar_count: usize,
    paragraph: u32,
    pending_breaks: u8,
    hidden: usize,
    open_blocks: usize,
    formatting_loss: bool,
    rich: Option<RichLabel>,
}

impl LabelText {
    fn block_boundary(&mut self, breaks: u8) {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.formatting_loss |= self.output.ends_with(' ');
            self.pending_breaks = self.pending_breaks.max(breaks);
        }
    }

    fn flush_boundary(&mut self) {
        for _ in 0..self.pending_breaks {
            self.push_newline();
        }
        self.pending_breaks = 0;
    }

    fn push_newline(&mut self) {
        self.output.push('\n');
        self.scalar_count += 1;
        self.paragraph = self.paragraph.saturating_add(1);
    }

    fn line_break(&mut self) {
        self.formatting_loss |= self.output.ends_with(' ');
        self.flush_boundary();
        self.record_list_item();
        self.record_run(1);
        self.push_newline();
    }

    fn record_list_item(&mut self) {
        if let Some(rich) = &mut self.rich
            && let Some(list) = rich.pending_item.take()
        {
            if rich
                .text
                .paragraphs
                .last()
                .is_some_and(|item| item.index == self.paragraph)
            {
                self.formatting_loss = true;
            } else {
                rich.text.paragraphs.push(InlineParagraph {
                    index: self.paragraph,
                    list,
                });
            }
        }
    }

    fn record_run(&mut self, length: usize) {
        if let Some(rich) = &mut self.rich {
            if self.scalar_count.saturating_add(length) > MAX_INLINE_TEXT_CHARS {
                rich.overflow = true;
            } else if !rich.style.is_empty() && length > 0 && !rich.overflow {
                let start = self.scalar_count as u32;
                let end = (self.scalar_count + length) as u32;
                if let Some(last) = rich.text.runs.last_mut()
                    && last.end == start
                    && last.style == rich.style
                {
                    last.end = end;
                } else if rich.text.runs.len() < MAX_INLINE_TEXT_RUNS {
                    rich.text.runs.push(InlineTextRun {
                        start,
                        end,
                        style: rich.style.clone(),
                    });
                } else {
                    rich.overflow = true;
                }
            }
        }
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
        let text = if decode_references {
            self.formatting_loss |= unresolved_entity(&text);
            decode_entities(&text)
        } else {
            text
        };
        let at_line_start =
            self.pending_breaks > 0 || self.output.is_empty() || self.output.ends_with('\n');
        let follows_space = self.pending_breaks == 0 && self.output.ends_with(' ');
        self.formatting_loss |= collapsible_whitespace(&text, at_line_start, follows_space);
        self.flush_boundary();
        let length = text.chars().count();
        self.record_list_item();
        self.record_run(length);
        self.scalar_count += length;
        self.paragraph = self
            .paragraph
            .saturating_add(text.chars().filter(|ch| *ch == '\n').count() as u32);
        self.output.push_str(&text);
    }
}

struct CssFontFamily {
    name: String,
    quoted: bool,
    valid: bool,
    has_fallback: bool,
}

fn first_css_font_family(value: &str) -> CssFontFamily {
    let mut input = cssparser::ParserInput::new(value);
    let mut parser = cssparser::Parser::new(&mut input);
    let mut family = CssFontFamily {
        name: String::new(),
        quoted: false,
        valid: true,
        has_fallback: false,
    };
    while !parser.is_exhausted() {
        match parser.next() {
            Ok(cssparser::Token::Ident(part)) if !family.quoted => {
                if !family.name.is_empty() {
                    family.name.push(' ');
                }
                family.name.push_str(part);
            }
            Ok(cssparser::Token::QuotedString(part))
                if family.name.is_empty() && !family.quoted =>
            {
                family.name.push_str(part);
                family.quoted = true;
            }
            Ok(cssparser::Token::Comma) if !family.name.is_empty() => {
                family.has_fallback = true;
                break;
            }
            _ => {
                family.valid = false;
                break;
            }
        }
    }
    family
}

fn inline_css(
    style: &mut InlineTextStyle,
    inherited: &InlineTextStyle,
    base_font_size: Option<f32>,
    value: &str,
) -> bool {
    let mut loss = false;
    let mut input = cssparser::ParserInput::new(value);
    let mut declarations = cssparser::Parser::new(&mut input);
    while !declarations.is_exhausted() {
        let declaration = declarations.parse_until_after(cssparser::Delimiter::Semicolon, |part| {
            let start = part.position();
            while part.next_including_whitespace_and_comments().is_ok() {}
            Ok::<_, cssparser::ParseError<'_, ()>>(part.slice_from(start))
        });
        let Ok(declaration) = declaration else {
            loss = true;
            continue;
        };
        if declaration.trim().is_empty() {
            continue;
        }
        let Some((name, value)) = declaration.split_once(':') else {
            loss = true;
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        let lower = value.to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "font-weight" | "font-style" | "font-family" | "font-size" | "color"
        ) && matches!(lower.as_str(), "inherit" | "unset")
        {
            match name.as_str() {
                "font-weight" => style.bold = inherited.bold,
                "font-style" => style.italic = inherited.italic,
                "font-family" => style.font_family.clone_from(&inherited.font_family),
                "font-size" => style.font_size = inherited.font_size,
                "color" => style.color = inherited.color,
                _ => unreachable!(),
            }
            continue;
        }
        match name.as_str() {
            "font-weight" => match lower.as_str() {
                "bold" => style.bold = Some(true),
                "normal" => style.bold = Some(false),
                "bolder" | "lighter" => {
                    style.bold = Some(lower == "bolder");
                    loss = true;
                }
                _ => {
                    let mut input = cssparser::ParserInput::new(&lower);
                    let mut parser = cssparser::Parser::new(&mut input);
                    let is_number = matches!(parser.next(), Ok(cssparser::Token::Number { .. }))
                        && parser.is_exhausted();
                    match lower.parse::<f64>() {
                        Ok(weight)
                            if is_number
                                && weight.is_finite()
                                && (1.0..=1000.0).contains(&weight) =>
                        {
                            style.bold = Some(weight >= 600.0);
                            loss |= weight != 400.0 && weight != 700.0;
                        }
                        _ => loss = true,
                    }
                }
            },
            "font-style" => match lower.as_str() {
                "italic" => style.italic = Some(true),
                "oblique" => {
                    style.italic = Some(true);
                    loss = true;
                }
                "normal" => style.italic = Some(false),
                _ => loss = true,
            },
            "text-decoration" | "text-decoration-line" => {
                if lower == "none" {
                    style.underline = Some(false);
                    style.strikethrough = Some(false);
                } else {
                    let mut underline = false;
                    let mut strike = false;
                    for part in lower.split_whitespace() {
                        match part {
                            "underline" => underline = true,
                            "line-through" => strike = true,
                            _ => loss = true,
                        }
                    }
                    style.underline = Some(underline);
                    style.strikethrough = Some(strike);
                }
            }
            "font-family" => {
                let parsed = first_css_font_family(value);
                loss |= parsed.has_fallback;
                let family = if parsed.quoted {
                    parsed.name.as_str()
                } else {
                    parsed.name.trim()
                };
                if parsed.valid
                    && !parsed.quoted
                    && !parsed.has_fallback
                    && matches!(family.to_ascii_lowercase().as_str(), "inherit" | "unset")
                {
                    style.font_family.clone_from(&inherited.font_family);
                    continue;
                }
                if parsed.valid
                    && !parsed.quoted
                    && matches!(
                        family.to_ascii_lowercase().as_str(),
                        "inherit" | "unset" | "initial" | "revert" | "revert-layer"
                    )
                {
                    loss = true;
                    continue;
                }
                if family.trim().is_empty()
                    || family.len() > MAX_INLINE_FONT_FAMILY_BYTES
                    || family.chars().any(char::is_control)
                    || !parsed.valid
                {
                    loss = true;
                } else {
                    style.font_family = Some(family.to_owned());
                }
            }
            "font-size" => {
                let relative = lower
                    .strip_suffix("em")
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .or_else(|| {
                        lower
                            .strip_suffix('%')
                            .and_then(|value| value.trim().parse::<f32>().ok())
                            .map(|value| value / 100.0)
                    });
                if relative == Some(1.0) {
                    style.font_size = inherited.font_size;
                    continue;
                }
                let size = lower
                    .strip_suffix("px")
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .or_else(|| {
                        lower
                            .strip_suffix("pt")
                            .and_then(|value| value.trim().parse::<f32>().ok())
                            .map(|value| value * 4.0 / 3.0)
                    })
                    .or_else(|| {
                        relative.and_then(|scale| {
                            inherited
                                .font_size
                                .or(base_font_size)
                                .map(|size| size * scale)
                        })
                    });
                match size {
                    Some(size)
                        if size.is_finite()
                            && (MIN_INLINE_FONT_SIZE..=MAX_INLINE_FONT_SIZE).contains(&size) =>
                    {
                        style.font_size = Some(size)
                    }
                    _ => loss = true,
                }
            }
            "color" => match super::parse_style_color(value) {
                Some(parsed) if parsed.color.is_some() => {
                    style.color = parsed.color;
                    loss |= parsed.approximate;
                }
                _ => loss = true,
            },
            "vertical-align" => match lower.as_str() {
                "sub" => style.baseline = Some(InlineBaseline::Subscript),
                "super" => style.baseline = Some(InlineBaseline::Superscript),
                "baseline" => style.baseline = Some(InlineBaseline::Normal),
                _ => loss = true,
            },
            "white-space" if lower == "normal" => {}
            _ => loss = true,
        }
    }
    loss
}

fn legacy_font_color(value: &str) -> Option<thinoda_core::Color> {
    if value.is_empty() {
        return None;
    }
    let value = value.trim_matches(|c| matches!(c, '\t' | '\n' | '\x0c' | '\r' | ' '));
    if value.eq_ignore_ascii_case("transparent") {
        return None;
    }

    let bytes = value.as_bytes();
    let is_hex = matches!(bytes.len(), 4 | 7)
        && bytes[0] == b'#'
        && bytes[1..].iter().all(u8::is_ascii_hexdigit);
    let is_name = !bytes.is_empty() && bytes.iter().all(u8::is_ascii_alphabetic);
    if let Some(color) = (is_hex || is_name)
        .then(|| super::parse_style_color(value))
        .flatten()
        .filter(|parsed| !parsed.approximate)
        .and_then(|parsed| parsed.color)
        .filter(|color| color.a == 255)
    {
        return Some(color);
    }

    // HTML legacy colours use a bounded, non-CSS hexadecimal fallback.
    let mut digits = Vec::with_capacity(129);
    for ch in value.chars() {
        if digits.len() == 128 {
            break;
        }
        if ch > '\u{ffff}' {
            digits.push(b'0');
            if digits.len() < 128 {
                digits.push(b'0');
            }
        } else {
            digits.push(if ch.is_ascii_hexdigit() {
                ch as u8
            } else {
                b'0'
            });
        }
    }
    if value.starts_with('#') {
        digits.remove(0);
    }
    if digits.is_empty() {
        digits.resize(3, b'0');
    }
    while digits.len() % 3 != 0 {
        digits.push(b'0');
    }
    let length = digits.len() / 3;
    let skip = length.saturating_sub(8);
    let mut red = &digits[skip..length];
    let mut green = &digits[length + skip..2 * length];
    let mut blue = &digits[2 * length + skip..];
    while red.len() > 2 && red[0] == b'0' && green[0] == b'0' && blue[0] == b'0' {
        red = &red[1..];
        green = &green[1..];
        blue = &blue[1..];
    }
    let component = |value: &[u8]| {
        let value = &value[..value.len().min(2)];
        u8::from_str_radix(std::str::from_utf8(value).ok()?, 16).ok()
    };
    Some(thinoda_core::Color::rgb(
        component(red)?,
        component(green)?,
        component(blue)?,
    ))
}

fn collapsible_whitespace(text: &str, mut at_line_start: bool, mut follows_space: bool) -> bool {
    for byte in text.bytes() {
        match byte {
            b'\t' | b'\n' | b'\r' | b'\x0c' => return true,
            b' ' => {
                if at_line_start || follows_space {
                    return true;
                }
                follows_space = true;
            }
            _ => {
                at_line_start = false;
                follows_space = false;
            }
        }
    }
    false
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

pub(super) fn parse_rich_html(
    text: &str,
    warnings: &mut BTreeSet<ImportWarning>,
    base_font_size: Option<f32>,
) -> (String, Option<InlineText>) {
    parse_html_with_limit(text, warnings, MAX_BYTES, true, base_font_size)
}

fn strip_html_with_limit(
    text: &str,
    warnings: &mut BTreeSet<ImportWarning>,
    memory_limit: usize,
) -> String {
    parse_html_with_limit(text, warnings, memory_limit, false, None).0
}

fn parse_html_with_limit(
    text: &str,
    warnings: &mut BTreeSet<ImportWarning>,
    memory_limit: usize,
    rich: bool,
    base_font_size: Option<f32>,
) -> (String, Option<InlineText>) {
    if !text.contains('<') {
        let decoded = decode_entities(text);
        if unresolved_entity(text)
            || collapsible_whitespace(&decoded, true, false)
            || decoded.ends_with(' ')
        {
            warnings.insert(ImportWarning::Formatting);
        }
        return (decoded, None);
    }

    let result = Rc::new(RefCell::new(LabelText {
        rich: rich.then(RichLabel::default),
        ..Default::default()
    }));
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
                if name == "br" {
                    if !el.attributes().is_empty() {
                        state.formatting_loss = true;
                    }
                    state.line_break();
                    return Ok(());
                }

                let mut previous_style = None;
                let mut previous_lists = 0usize;
                let marker = name == "span"
                    && el.get_attribute("data-thinoda-break").as_deref() == Some("1")
                    && state.rich.is_some();
                if let Some(rich) = &mut state.rich {
                    let inherited_style = rich.style.clone();
                    previous_style = Some(inherited_style.clone());
                    previous_lists = rich.lists.len();
                    let inherited_underline = rich.style.underline;
                    let inherited_strike = rich.style.strikethrough;
                    let inherited_color = rich.style.color;
                    let mut loss = false;
                    match name.as_str() {
                        "b" | "strong" => rich.style.bold = Some(true),
                        "i" | "em" => rich.style.italic = Some(true),
                        "u" => rich.style.underline = Some(true),
                        "s" | "strike" | "del" => rich.style.strikethrough = Some(true),
                        "sub" => rich.style.baseline = Some(InlineBaseline::Subscript),
                        "sup" => rich.style.baseline = Some(InlineBaseline::Superscript),
                        "ul" | "ol" => {
                            loss |= !rich.lists.is_empty();
                            rich.lists.push(if name == "ul" {
                                ListKind::Bulleted
                            } else {
                                ListKind::Numbered
                            });
                        }
                        "li" => {
                            rich.pending_item = rich.lists.last().copied();
                            loss |= rich.pending_item.is_none();
                        }
                        "div" | "span" | "p" | "font" => {}
                        _ => loss = true,
                    }
                    if name == "font"
                        && let Some(color) = el.get_attribute("color")
                    {
                        loss |= unresolved_entity(&color);
                        if let Some(color) = legacy_font_color(&decode_entities(&color)) {
                            rich.style.color = Some(color);
                        }
                    }
                    if name == "font"
                        && let Some(face) = el.get_attribute("face")
                    {
                        loss |= unresolved_entity(&face);
                        let parsed = first_css_font_family(&decode_entities(&face));
                        let family = if parsed.quoted {
                            parsed.name.as_str()
                        } else {
                            parsed.name.trim()
                        };
                        if parsed.valid
                            && !parsed.has_fallback
                            && !family.trim().is_empty()
                            && family.len() <= MAX_INLINE_FONT_FAMILY_BYTES
                            && !family.chars().any(char::is_control)
                            && (parsed.quoted
                                || !matches!(
                                    family.to_ascii_lowercase().as_str(),
                                    "inherit"
                                        | "unset"
                                        | "initial"
                                        | "revert"
                                        | "revert-layer"
                                        | "default"
                                ))
                        {
                            rich.style.font_family = Some(family.to_owned());
                        } else {
                            loss = true;
                        }
                    }
                    let mut inherit_underline = false;
                    let mut inherit_strike = false;
                    for attribute in el.attributes() {
                        match attribute.name().to_ascii_lowercase().as_str() {
                            "style" => {
                                loss |= inline_css(
                                    &mut rich.style,
                                    &inherited_style,
                                    base_font_size,
                                    &attribute.value(),
                                );
                            }
                            "data-thinoda-underline" if attribute.value() == "inherit" => {
                                inherit_underline = true;
                            }
                            "data-thinoda-strike" if attribute.value() == "inherit" => {
                                inherit_strike = true;
                            }
                            "data-thinoda-break" if marker => {}
                            "color" | "face" if name == "font" => {}
                            _ => loss = true,
                        }
                    }
                    if inherit_underline {
                        rich.style.underline = inherited_underline;
                    }
                    if inherit_strike {
                        rich.style.strikethrough = inherited_strike;
                    }
                    loss |= (inherited_underline == Some(true) || inherited_strike == Some(true))
                        && rich.style.color != inherited_color;
                    if inherited_underline == Some(true) {
                        rich.style.underline = Some(true);
                    }
                    if inherited_strike == Some(true) {
                        rich.style.strikethrough = Some(true);
                    }
                    state.formatting_loss |= loss;
                } else if !el.attributes().is_empty() || !matches!(name.as_str(), "div" | "span") {
                    state.formatting_loss = true;
                }
                let marker_end = if marker {
                    state.line_break();
                    Some(state.scalar_count)
                } else {
                    None
                };
                let breaks = match name.as_str() {
                    "p" => 2,
                    "div" | "li" | "ul" | "ol" | "blockquote" | "pre" | "table" | "tr" | "h1"
                    | "h2" | "h3" | "h4" | "h5" | "h6" => 1,
                    _ => 0,
                };
                if el.can_have_content() {
                    if breaks > 0 {
                        state.block_boundary(breaks);
                        state.open_blocks += 1;
                    }
                    let state = Rc::clone(&elements);
                    el.on_end_tag(end_tag!(move |_| {
                        let mut state = state.borrow_mut();
                        if breaks > 0 {
                            state.block_boundary(breaks);
                            state.open_blocks -= 1;
                        }
                        if marker_end.is_some_and(|end| state.scalar_count != end) {
                            state.formatting_loss = true;
                        }
                        if name == "li"
                            && state
                                .rich
                                .as_ref()
                                .is_some_and(|rich| rich.pending_item.is_some())
                        {
                            state.flush_boundary();
                            state.record_list_item();
                        }
                        if let Some(previous_style) = previous_style.take()
                            && let Some(rich) = &mut state.rich
                        {
                            rich.style = previous_style;
                            rich.lists.truncate(previous_lists);
                        }
                        Ok(())
                    }))?;
                } else if let Some(previous_style) = previous_style
                    && let Some(rich) = &mut state.rich
                {
                    rich.style = previous_style;
                    rich.lists.truncate(previous_lists);
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
        return (text.to_owned(), None);
    }
    let mut result = result.borrow_mut();
    if result.formatting_loss
        || result.open_blocks > 0
        || result.hidden > 0
        || result.output.ends_with(' ')
    {
        warnings.insert(ImportWarning::Formatting);
    }
    let output = std::mem::take(&mut result.output);
    let rich = result.rich.take().and_then(|rich| {
        if rich.overflow || rich.text.validate(&output).is_err() {
            warnings.insert(ImportWarning::Formatting);
            None
        } else {
            (!rich.text.is_empty()).then_some(rich.text)
        }
    });
    (output, rich)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_font_color_truncates_after_supplementary_character_replacement() {
        assert_eq!(
            legacy_font_color(&"f".repeat(129)),
            Some(thinoda_core::Color::rgb(255, 255, 255))
        );
        assert_eq!(
            legacy_font_color(&format!("{}🌍a", "0".repeat(127))),
            Some(thinoda_core::Color::rgb(0, 0, 0))
        );
    }

    #[test]
    fn rich_html_preserves_nested_styles_and_unicode_scalar_ranges() {
        let mut warnings = BTreeSet::new();
        let (text, rich) = parse_rich_html(
            r#"<b>A<span style="font-style:italic;color:#ff0000">🙂</span>B</b>"#,
            &mut warnings,
            None,
        );
        assert_eq!(text, "A🙂B");
        assert!(warnings.is_empty(), "{warnings:?}");
        let runs = rich.unwrap().runs;
        assert_eq!((runs[0].start, runs[0].end), (0, 1));
        assert_eq!(runs[1].style.bold, Some(true));
        assert_eq!(runs[1].style.italic, Some(true));
        assert_eq!(
            runs[1].style.color,
            Some(thinoda_core::Color::rgb(255, 0, 0))
        );
        assert_eq!((runs[2].start, runs[2].end), (2, 3));
    }

    #[test]
    fn relative_size_with_unknown_parent_warns_without_guessing() {
        let mut warnings = BTreeSet::new();
        let (_, rich) = parse_rich_html(
            r#"<span style="font-size:150%">Text</span>"#,
            &mut warnings,
            None,
        );
        assert!(rich.is_none());
        assert!(warnings.contains(&ImportWarning::Formatting));
    }

    #[test]
    fn rich_html_keeps_lists_and_warns_for_inert_unsupported_content() {
        let mut warnings = BTreeSet::new();
        let (text, rich) = parse_rich_html(
            "<ul><li>one</li><li><sup>two</sup></li></ul>",
            &mut warnings,
            None,
        );
        assert_eq!(text, "one\ntwo");
        assert!(warnings.is_empty(), "{warnings:?}");
        let rich = rich.unwrap();
        assert_eq!(
            rich.paragraphs,
            vec![
                InlineParagraph {
                    index: 0,
                    list: ListKind::Bulleted,
                },
                InlineParagraph {
                    index: 1,
                    list: ListKind::Bulleted,
                },
            ]
        );
        assert_eq!(
            rich.runs[0].style.baseline,
            Some(InlineBaseline::Superscript)
        );

        let mut warnings = BTreeSet::new();
        let (text, rich) = parse_rich_html(
            r#"<span style="font-weight:bold;background-image:url(https://example.invalid/x)">ok</span><script>hidden</script>"#,
            &mut warnings,
            None,
        );
        assert_eq!(text, "ok");
        assert_eq!(rich.unwrap().runs[0].style.bold, Some(true));
        assert!(warnings.contains(&ImportWarning::Formatting));
    }

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
