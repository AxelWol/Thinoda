//! draw.io / diagrams.net documents and embedded SVG, PNG and HTML diagrams.
//!
//! Import converts every page into a Freeform board sheet: vertices become shapes,
//! notes become cards, swimlanes and containers become Zones, groups become board
//! groups and edges become free connectors. Content without a native equivalent is
//! approximated and reported through [`ImportWarning`] categories.
//!
//! Export writes one uncompressed page per sheet. Board content maps back to the
//! same draw.io cells; ordinary mind-map topics, branches and annotations are
//! written as drawn shapes and reported by the export preflight. Thinoda-only
//! details use `thinoda*` style keys or model attributes, which other editors
//! may discard when saving.

use crate::{Format, ImportReport, ImportWarning, IoError};
use base64::Engine;
use cssparser::{ParseError, Parser, ParserInput};
use cssparser_color::{Color as CssColor, RgbaLegacy, hsl_to_rgb, hwb_to_rgb};
use quick_xml::events::{BytesStart, Event};
use std::collections::{BTreeSet, HashMap, HashSet};
use thinoda_core::{
    ApproxMeasure, Color, MAX_PRESENTATION_NAME_BYTES, MAX_ZONES, MIN_ZONE_SIZE, Rect, Sheet,
    TextAlign, TopicId, TopicShape, Vec2, Workbook, ZoneId,
    board::{
        Band, BoardMember, CANVAS_LABEL_FONT_SIZE, CanvasConnector, CanvasGroupId, CanvasObject,
        CanvasObjectId, CanvasObjectKind, Card, CardKind, ConnectorMarker, ConnectorMarkerKind,
        ConnectorPort, ConnectorRoute, DEFAULT_CONNECTOR_MARKER_SIZE, DrawioRectangleStencil,
        Endpoint, GridKind, InkStroke, InkTool, MAX_BOARD_COORDINATE, MAX_BOARD_TEXT_CHARS,
        MAX_GRID_SIZE, MAX_STROKE_WIDTH, MIN_CARD_SIZE, MIN_GRID_SIZE, MIN_STROKE_WIDTH,
        ObjectStyle, RectangleSides, ShapeKind, ShapeRounding, ShapeRoundingMode, SheetRootMode,
        normalize_rotation, rotate_around, valid_board_position, valid_connector_marker_size,
    },
    images::{self, MAX_DISPLAY_SIZE},
    layout_sheet,
    resources::{MAX_RESOURCE_BYTES, Resource},
};

mod html;
mod label_html;

use label_html::strip_html;

/// Largest accepted input and inflated diagram text.
const MAX_BYTES: usize = 64 * 1024 * 1024;
/// Largest published-link input accepted, including its URL and fragment.
pub const MAX_LINK_BYTES: usize = MAX_BYTES;
const MAX_XML_DEPTH: usize = 64;
const MAX_XML_ELEMENTS: usize = 400_000;
const MAX_PAGES: usize = 1_000;
/// Nesting depth of cells (parent references), independent of XML depth.
const MAX_CELL_DEPTH: usize = 256;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn invalid(message: &str) -> IoError {
    IoError::Invalid(message.into())
}

fn xml_error(error: impl std::fmt::Display) -> IoError {
    IoError::Xml(error.to_string())
}

// ---------------------------------------------------------------------------------------
// Minimal bounded DOM

#[derive(Debug, Default)]
pub(crate) struct Element {
    pub(crate) name: String,
    pub(crate) attrs: Vec<(String, String)>,
    pub(crate) children: Vec<Element>,
    pub(crate) text: String,
}

impl Element {
    pub(crate) fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub(crate) fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }
}

fn element_from(start: &BytesStart) -> Result<Element, IoError> {
    let mut element = Element {
        name: start.local_name().as_ref().to_owned(),
        ..Element::default()
    };
    for attr in start.attributes() {
        let attr = attr.map_err(xml_error)?;
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(xml_error)?;
        element.attrs.push((
            attr.key.local_name().as_ref().to_owned(),
            value.into_owned(),
        ));
    }
    Ok(element)
}

fn attach(
    stack: &mut [Element],
    root: &mut Option<Element>,
    element: Element,
) -> Result<(), IoError> {
    match stack.last_mut() {
        Some(parent) => parent.children.push(element),
        None if root.is_none() => *root = Some(element),
        None => return Err(invalid("draw.io XML has more than one root element")),
    }
    Ok(())
}

/// Parses XML without DTDs or external entities, with depth and element limits.
pub(crate) fn parse_dom(text: &str) -> Result<Element, IoError> {
    if text.len() > MAX_BYTES {
        return Err(invalid("draw.io document exceeds size limit"));
    }
    let mut reader = quick_xml::Reader::from_str(text);
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut count = 0usize;
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(start) => {
                count += 1;
                if count > MAX_XML_ELEMENTS || stack.len() >= MAX_XML_DEPTH {
                    return Err(invalid("draw.io XML exceeds structure limits"));
                }
                if stack.is_empty() && root.is_some() {
                    return Err(invalid("draw.io XML has more than one root element"));
                }
                stack.push(element_from(&start)?);
            }
            Event::Empty(start) => {
                count += 1;
                if count > MAX_XML_ELEMENTS || stack.len() >= MAX_XML_DEPTH {
                    return Err(invalid("draw.io XML exceeds structure limits"));
                }
                let element = element_from(&start)?;
                attach(&mut stack, &mut root, element)?;
            }
            Event::End(_) => {
                let element = stack
                    .pop()
                    .ok_or_else(|| invalid("draw.io XML is not well formed"))?;
                attach(&mut stack, &mut root, element)?;
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.xml10_content());
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.into_inner());
                }
            }
            Event::GeneralRef(r) => {
                let ch = match r.resolve_char_ref().map_err(xml_error)? {
                    Some(ch) => ch,
                    None => match &*r {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "apos" => '\'',
                        "quot" => '"',
                        other => {
                            return Err(IoError::Xml(format!("unknown entity &{other};")));
                        }
                    },
                };
                if let Some(top) = stack.last_mut() {
                    top.text.push(ch);
                }
            }
            Event::DocType(_) => {
                return Err(invalid("draw.io documents must not declare a DTD"));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(invalid("draw.io XML is not well formed"));
    }
    root.ok_or_else(|| invalid("draw.io document is empty"))
}

/// The root start tag only; tolerates a DOCTYPE (SVG files usually have one).
fn root_start(text: &str) -> Option<Element> {
    let mut reader = quick_xml::Reader::from_str(text);
    loop {
        match reader.read_event() {
            Ok(Event::Start(start) | Event::Empty(start)) => return element_from(&start).ok(),
            Ok(
                Event::Decl(_)
                | Event::Comment(_)
                | Event::PI(_)
                | Event::DocType(_)
                | Event::Text(_),
            ) => {}
            _ => return None,
        }
    }
}

fn is_drawio_xml(text: &str) -> bool {
    root_start(text).is_some_and(|root| matches!(root.name.as_str(), "mxfile" | "mxGraphModel"))
}

// ---------------------------------------------------------------------------------------
// Containers and compression

fn inflate(data: &[u8]) -> Option<Vec<u8>> {
    use miniz_oxide::inflate::{decompress_to_vec_with_limit, decompress_to_vec_zlib_with_limit};
    let zlib_header = data.len() >= 2
        && data[0] & 0x0F == 8
        && (u16::from(data[0]) << 8 | u16::from(data[1])) % 31 == 0;
    if zlib_header && let Ok(out) = decompress_to_vec_zlib_with_limit(data, MAX_BYTES) {
        return Some(out);
    }
    decompress_to_vec_with_limit(data, MAX_BYTES).ok()
}

pub(crate) fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// draw.io URI-encodes XML before compressing it or storing it in PNG text.
fn uri_decode(text: String) -> String {
    if text.trim_start().starts_with('<') {
        text
    } else {
        percent_decode(&text).unwrap_or(text)
    }
}

/// Decodes `base64(deflateRaw(encodeURIComponent(xml)))` diagram text.
pub(crate) fn decode_compressed(text: &str) -> Result<String, IoError> {
    let cleaned: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let data = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .map_err(|_| invalid("draw.io page is neither XML nor compressed diagram data"))?;
    decode_compressed_bytes(&data)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BoundedDecodeError {
    Invalid,
    Limit,
}

/// Decodes a library fragment while charging both successful and failed
/// inflations against the caller's remaining expansion budget.
pub(crate) fn decode_compressed_bounded(
    text: &str,
    max_decoded: usize,
    work_remaining: &mut usize,
) -> Result<String, BoundedDecodeError> {
    use miniz_oxide::inflate::{
        TINFLStatus, decompress_to_vec_with_limit, decompress_to_vec_zlib_with_limit,
    };

    let cleaned: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let data = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .map_err(|_| BoundedDecodeError::Invalid)?;
    let max_inflated = max_decoded.saturating_mul(3);
    let mut attempt = |zlib: bool| {
        let limit = max_inflated.min(*work_remaining);
        if limit == 0 {
            return Err(BoundedDecodeError::Limit);
        }
        let result = if zlib {
            decompress_to_vec_zlib_with_limit(&data, limit)
        } else {
            decompress_to_vec_with_limit(&data, limit)
        };
        match result {
            Ok(output) => {
                *work_remaining = work_remaining.saturating_sub(output.len());
                Ok(output)
            }
            Err(error) => {
                // miniz may retain a partially filled buffer on failure. Charging
                // the whole buffer is conservative and bounds repeated failures.
                *work_remaining = work_remaining.saturating_sub(error.output.len());
                if error.status == TINFLStatus::HasMoreOutput {
                    Err(BoundedDecodeError::Limit)
                } else {
                    Err(BoundedDecodeError::Invalid)
                }
            }
        }
    };
    let zlib_header = data.len() >= 2
        && data[0] & 0x0F == 8
        && (u16::from(data[0]) << 8 | u16::from(data[1])) % 31 == 0;
    let raw = if zlib_header {
        match attempt(true) {
            Ok(output) => output,
            Err(first) => match attempt(false) {
                Ok(output) => output,
                Err(second)
                    if first == BoundedDecodeError::Limit
                        || second == BoundedDecodeError::Limit =>
                {
                    return Err(BoundedDecodeError::Limit);
                }
                Err(_) => return Err(BoundedDecodeError::Invalid),
            },
        }
    } else {
        attempt(false)?
    };
    let text = String::from_utf8(raw).map_err(|_| BoundedDecodeError::Invalid)?;
    let decoded = uri_decode(text);
    if decoded.len() > max_decoded {
        return Err(BoundedDecodeError::Limit);
    }
    Ok(decoded)
}

fn decode_compressed_bytes(data: &[u8]) -> Result<String, IoError> {
    let raw = inflate(data).ok_or_else(|| invalid("draw.io page data cannot be decompressed"))?;
    let text = String::from_utf8(raw).map_err(|_| invalid("draw.io page data is not UTF-8"))?;
    Ok(uri_decode(text))
}

/// Imports only self-contained `#R` fragments, without loading their URL.
pub(crate) fn import_link(link: &str) -> Result<ImportReport, IoError> {
    if link.len() > MAX_LINK_BYTES {
        return Err(invalid("draw.io link exceeds size limit"));
    }
    let link = link.trim();
    let parsed = if link.starts_with('#') {
        None
    } else {
        let url = url::Url::parse(link).map_err(|_| invalid("invalid draw.io link"))?;
        if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
            return Err(invalid("draw.io link must use HTTP or HTTPS"));
        }
        Some(url)
    };
    let fragment = match &parsed {
        Some(url) => url.fragment(),
        None => link.strip_prefix('#'),
    };
    let encoded = fragment
        .and_then(|fragment| fragment.strip_prefix('R'))
        .filter(|payload| !payload.is_empty())
        .ok_or_else(|| invalid("draw.io link has no embedded #R diagram"))?;
    let decoded = percent_decode(encoded)
        .ok_or_else(|| invalid("draw.io link has invalid percent encoding or UTF-8"))?;
    let xml = if decoded
        .trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('<')
    {
        decoded
    } else {
        decode_compressed(&decoded)?
    };
    import(xml.as_bytes())
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Text of a draw.io PNG metadata chunk (tEXt, zTXt or iTXt).
fn png_mxfile(bytes: &[u8]) -> Option<String> {
    if !bytes.starts_with(PNG_SIGNATURE) {
        return None;
    }
    let mut pos = PNG_SIGNATURE.len();
    while pos.checked_add(8)? <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        let kind = &bytes[pos + 4..pos + 8];
        let start = pos + 8;
        let end = start.checked_add(len)?;
        if end.checked_add(4)? > bytes.len() {
            return None;
        }
        let data = &bytes[start..end];
        if let Some(rest) = data
            .strip_prefix(b"mxfile\0")
            .or_else(|| data.strip_prefix(b"mxGraphModel\0"))
        {
            match kind {
                b"tEXt" => return Some(latin1(rest)),
                b"zTXt" => {
                    let (&method, compressed) = rest.split_first()?;
                    if method != 0 {
                        return None;
                    }
                    return Some(latin1(&inflate(compressed)?));
                }
                b"iTXt" => {
                    let (&flag, rest) = rest.split_first()?;
                    let (&method, rest) = rest.split_first()?;
                    if flag > 1 || method != 0 {
                        return None;
                    }
                    let lang_end = rest.iter().position(|&b| b == 0)?;
                    let rest = &rest[lang_end + 1..];
                    let key_end = rest.iter().position(|&b| b == 0)?;
                    let text = &rest[key_end + 1..];
                    let text = if flag == 1 {
                        inflate(text)?
                    } else {
                        text.to_vec()
                    };
                    return String::from_utf8(text).ok();
                }
                _ => {}
            }
        }
        if kind == b"IEND" {
            return None;
        }
        pos = end + 4;
    }
    None
}

fn svg_content(content: &str) -> Result<String, IoError> {
    let trimmed = content
        .trim_start()
        .trim_start_matches('\u{feff}')
        .trim_start();
    if trimmed.starts_with('<') {
        return Ok(trimmed.to_owned());
    }
    if trimmed.starts_with('%') {
        return percent_decode(trimmed)
            .ok_or_else(|| invalid("SVG diagram has invalid URI encoding or UTF-8"));
    }
    let cleaned: String = trimmed
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let data = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .map_err(|_| invalid("SVG diagram is neither XML nor valid base64"))?;
    if let Ok(text) = std::str::from_utf8(&data) {
        let text = text
            .trim_start()
            .trim_start_matches('\u{feff}')
            .trim_start();
        let xml = if text.starts_with('<') {
            Some(text.to_owned())
        } else if text.starts_with('%') {
            percent_decode(text)
        } else {
            None
        };
        if let Some(xml) = xml.filter(|xml| is_drawio_xml(xml)) {
            return Ok(xml);
        }
    }
    decode_compressed_bytes(&data)
}

/// `true` when `bytes` hold a draw.io diagram, including SVG and PNG exports
/// with an embedded editable copy.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    if bytes.starts_with(PNG_SIGNATURE) {
        return png_mxfile(bytes).is_some();
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    match root_start(text) {
        Some(root) => match root.name.as_str() {
            "mxfile" | "mxGraphModel" => true,
            "svg" => root
                .attr("content")
                .is_some_and(|content| svg_content(content).is_ok_and(|xml| is_drawio_xml(&xml))),
            _ => false,
        },
        None => false,
    }
}

pub(crate) fn sniff_html(bytes: &[u8]) -> bool {
    const ATTRIBUTE: &[u8] = b"class";
    if bytes.len() > MAX_BYTES
        || !bytes
            .windows(ATTRIBUTE.len())
            .any(|window| window.eq_ignore_ascii_case(ATTRIBUTE))
    {
        return false;
    }
    html::extract(bytes).is_ok_and(|xml| is_drawio_xml(&xml))
}

pub(crate) fn import_html(bytes: &[u8]) -> Result<ImportReport, IoError> {
    import(html::extract(bytes)?.as_bytes())
}

struct Page {
    name: String,
    model: Element,
}

fn read_pages(bytes: &[u8]) -> Result<Vec<Page>, IoError> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid("draw.io document exceeds size limit"));
    }
    let text = if bytes.starts_with(PNG_SIGNATURE) {
        uri_decode(png_mxfile(bytes).ok_or_else(|| invalid("PNG has no embedded draw.io diagram"))?)
    } else {
        let text = std::str::from_utf8(bytes).map_err(|_| invalid("draw.io file is not UTF-8"))?;
        text.strip_prefix('\u{feff}').unwrap_or(text).to_owned()
    };
    let text = match root_start(&text) {
        Some(root) if root.name == "svg" => svg_content(
            root.attr("content")
                .ok_or_else(|| invalid("SVG has no embedded draw.io diagram"))?,
        )?,
        _ => text,
    };
    let root = parse_dom(&text)?;
    match root.name.as_str() {
        "mxGraphModel" => Ok(vec![Page {
            name: String::new(),
            model: root,
        }]),
        "mxfile" => {
            let mut pages = Vec::new();
            for mut diagram in root.children.into_iter().filter(|c| c.name == "diagram") {
                if pages.len() >= MAX_PAGES {
                    return Err(invalid("draw.io document has too many pages"));
                }
                let name = diagram.attr("name").unwrap_or_default().to_owned();
                let model = match diagram
                    .children
                    .iter()
                    .position(|c| c.name == "mxGraphModel")
                {
                    Some(index) => diagram.children.swap_remove(index),
                    None if diagram.text.trim().is_empty() => Element {
                        name: "mxGraphModel".into(),
                        ..Element::default()
                    },
                    None => {
                        let model = parse_dom(&decode_compressed(diagram.text.trim())?)?;
                        if model.name != "mxGraphModel" {
                            return Err(invalid("draw.io page has no graph model"));
                        }
                        model
                    }
                };
                pages.push(Page { name, model });
            }
            if pages.is_empty() {
                return Err(invalid("draw.io document contains no pages"));
            }
            Ok(pages)
        }
        _ => Err(invalid("not a draw.io document")),
    }
}

// ---------------------------------------------------------------------------------------
// Styles and labels

const MAX_COLOR_NESTING: usize = 16;

#[derive(Clone, Copy)]
struct ParsedColor {
    color: Option<Color>,
    approximate: bool,
}

fn color_delimiters_balanced(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                if i + 1 == bytes.len() {
                    return false;
                }
                i += 2;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" {
                    i += 1;
                }
                if i + 1 == bytes.len() || i == bytes.len() {
                    return false;
                }
                i += 2;
            }
            b'\'' | b'"' => return false,
            b'(' => {
                depth += 1;
                if depth > MAX_COLOR_NESTING {
                    return false;
                }
                i += 1;
            }
            b')' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
                i += 1;
            }
            _ => i += 1,
        }
    }
    depth == 0
}

fn css_rgba(color: RgbaLegacy) -> Option<Color> {
    color.alpha.is_finite().then(|| {
        Color::rgba(
            color.red,
            color.green,
            color.blue,
            (color.alpha * 255.0).round() as u8,
        )
    })
}

fn static_css_color(color: CssColor) -> Option<Color> {
    match color {
        CssColor::Rgba(color) => css_rgba(color),
        CssColor::Hsl(color) => {
            let (hue, saturation, lightness, alpha) = (
                color.hue?,
                color.saturation?,
                color.lightness?,
                color.alpha?,
            );
            if !hue.is_finite()
                || !(0.0..=360.0).contains(&hue)
                || !saturation.is_finite()
                || !lightness.is_finite()
                || !alpha.is_finite()
            {
                return None;
            }
            let (red, green, blue) = hsl_to_rgb(hue / 360.0, saturation, lightness);
            css_rgba(RgbaLegacy::from_floats(red, green, blue, alpha))
        }
        CssColor::Hwb(color) => {
            let (hue, whiteness, blackness, alpha) =
                (color.hue?, color.whiteness?, color.blackness?, color.alpha?);
            if !hue.is_finite()
                || !(0.0..=360.0).contains(&hue)
                || !whiteness.is_finite()
                || !blackness.is_finite()
                || !alpha.is_finite()
            {
                return None;
            }
            let (red, green, blue) = hwb_to_rgb(hue / 360.0, whiteness, blackness);
            css_rgba(RgbaLegacy::from_floats(red, green, blue, alpha))
        }
        _ => None,
    }
}

fn parse_css_color<'i>(
    input: &mut Parser<'i, '_>,
    depth: usize,
) -> Result<ParsedColor, ParseError<'i, ()>> {
    if depth >= MAX_COLOR_NESTING {
        return Err(input.new_custom_error(()));
    }
    if input
        .try_parse(|input| input.expect_function_matching("light-dark"))
        .is_ok()
    {
        return input.parse_nested_block(|input| {
            let light = parse_css_color(input, depth + 1)?;
            input.expect_comma()?;
            parse_css_color(input, depth + 1)?;
            Ok(ParsedColor {
                color: light.color,
                approximate: true,
            })
        });
    }
    let color = static_css_color(CssColor::parse(input)?);
    Ok(ParsedColor {
        color,
        approximate: color.is_none(),
    })
}

fn parse_style_color(value: &str) -> Option<ParsedColor> {
    if value == "none" {
        return Some(ParsedColor {
            color: Some(Color::TRANSPARENT),
            approximate: false,
        });
    }
    if color_delimiters_balanced(value) {
        let mut input = ParserInput::new(value);
        let mut parser = Parser::new(&mut input);
        if let Ok(parsed) = parse_css_color(&mut parser, 0)
            && parser.expect_exhausted().is_ok()
        {
            return Some(parsed);
        }
    }
    // Keep the historical bare/multiple-# hex fallback, but disclose it as non-CSS.
    Color::from_hex(value).map(|color| ParsedColor {
        color: Some(color),
        approximate: true,
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ConstraintPerimeter {
    Rectangle,
    Ellipse,
    Rhombus,
    #[default]
    Unknown,
}

#[derive(Debug, Default)]
struct Style {
    tokens: Vec<String>,
    map: HashMap<String, String>,
    suppress_defaults: bool,
    partial_sides: [Option<String>; 4],
    constraint_perimeter: ConstraintPerimeter,
}

impl Style {
    fn parse(text: &str) -> Self {
        let mut style = Style {
            suppress_defaults: text.starts_with(';'),
            constraint_perimeter: if text.starts_with(';') {
                ConstraintPerimeter::Unknown
            } else {
                ConstraintPerimeter::Rectangle
            },
            ..Self::default()
        };
        for raw in text.split(';') {
            if let Some((key, value)) = raw.split_once('=')
                && let Some(index) = PARTIAL_SIDES.iter().position(|side| *side == key)
            {
                style.partial_sides[index] = (value != "none").then(|| value.to_owned());
            }
            let part = raw.trim();
            if part.is_empty() {
                continue;
            }
            match part.split_once('=') {
                Some((key, value)) => {
                    if key.trim() == "perimeter" {
                        style.constraint_perimeter = match value.trim() {
                            "rectanglePerimeter" => ConstraintPerimeter::Rectangle,
                            "ellipsePerimeter" => ConstraintPerimeter::Ellipse,
                            "rhombusPerimeter" => ConstraintPerimeter::Rhombus,
                            _ => ConstraintPerimeter::Unknown,
                        };
                    }
                    style
                        .map
                        .insert(key.trim().to_owned(), value.trim().to_owned());
                }
                None => {
                    // Named styles are merged in order; defaultEdge restores its end marker.
                    if part == "defaultEdge" {
                        style.map.insert("endArrow".into(), "classic".into());
                    }
                    style.constraint_perimeter = match part {
                        "defaultVertex" => ConstraintPerimeter::Rectangle,
                        "ellipse" => ConstraintPerimeter::Ellipse,
                        "rhombus" => ConstraintPerimeter::Rhombus,
                        "text" | "label" | "edgeLabel" | "swimlane" => style.constraint_perimeter,
                        _ => ConstraintPerimeter::Unknown,
                    };
                    style.tokens.push(part.to_owned());
                }
            }
        }
        style
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    /// A Thinoda stencil reference stored in the style, if any.
    fn stencil(&self) -> Option<thinoda_core::stencil_object::StencilRef> {
        self.get(crate::drawio_stencil::STYLE_PACKAGE_KEY)?;
        crate::drawio_stencil::reference_from_style(
            self.map.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        )
    }

    /// Whether the style references a stencil this build can draw; its native
    /// draw.io shape is then only a fallback for draw.io itself.
    fn draws_stencil(&self) -> bool {
        self.stencil()
            .is_some_and(|stencil| stencil.resolve().is_some())
    }

    fn flag(&self, key: &str) -> bool {
        self.get(key) == Some("1")
    }

    fn num(&self, key: &str) -> Option<f32> {
        self.get(key)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
    }

    fn connector_port(&self, source: bool) -> Result<Option<ConnectorPort>, ()> {
        let keys = if source {
            ["exitX", "exitY", "exitDx", "exitDy", "exitPerimeter"]
        } else {
            ["entryX", "entryY", "entryDx", "entryDy", "entryPerimeter"]
        };
        if keys.iter().all(|key| self.get(key).is_none()) {
            return Ok(None);
        }
        let optional_number = |key| match self.get(key) {
            Some(_) => self.num(key).ok_or(()),
            None => Ok(0.0),
        };
        let perimeter = match self.get(keys[4]) {
            None => true,
            Some(_) => match self.num(keys[4]) {
                Some(0.0) => false,
                Some(1.0) => true,
                _ => return Err(()),
            },
        };
        let port = ConnectorPort {
            position: Vec2::new(self.num(keys[0]).ok_or(())?, self.num(keys[1]).ok_or(())?),
            offset: Vec2::new(optional_number(keys[2])?, optional_number(keys[3])?),
            perimeter,
        };
        port.is_valid().then_some(Some(port)).ok_or(())
    }

    fn rectangle_sides(&self) -> RectangleSides {
        // The published stylesheet converts decimal numbers before its renderer
        // compares each value with "1"; a literal "none" removes the key.
        let visible = |value: &Option<String>| {
            value.as_deref().is_none_or(|value| {
                let value = value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
                !value.to_ascii_lowercase().contains("0x")
                    && value
                        .parse::<f64>()
                        .is_ok_and(|number| number.is_finite() && number == 1.0)
            })
        };
        RectangleSides {
            top: visible(&self.partial_sides[0]),
            right: visible(&self.partial_sides[1]),
            bottom: visible(&self.partial_sides[2]),
            left: visible(&self.partial_sides[3]),
        }
    }

    fn text_align(&self) -> Option<TextAlign> {
        match self.get("align")? {
            "left" => Some(TextAlign::Left),
            "center" => Some(TextAlign::Center),
            "right" => Some(TextAlign::Right),
            _ => None,
        }
    }

    fn has(&self, token: &str) -> bool {
        self.tokens.iter().any(|t| t == token)
    }

    /// The `shape` key, otherwise the leading named style.
    fn shape(&self) -> &str {
        self.get("shape")
            .or_else(|| self.tokens.first().map(String::as_str))
            .unwrap_or("")
    }

    /// `None` for absent or theme-default colors; `none` is transparent.
    fn color(&self, key: &str, opacity_key: &str) -> Option<Color> {
        let color = parse_style_color(self.get(key)?)?.color?;
        Some(self.color_with_opacity(color, opacity_key))
    }

    fn text_color(&self) -> Option<Color> {
        if self.get("fontColor") == Some("none") {
            Some(self.color_with_opacity(Color::BLACK, "textOpacity"))
        } else {
            self.color("fontColor", "textOpacity").or_else(|| {
                self.num("textOpacity")
                    .is_some_and(|opacity| opacity <= 0.0)
                    .then_some(Color::TRANSPARENT)
            })
        }
    }

    fn color_with_opacity(&self, mut color: Color, opacity_key: &str) -> Color {
        if let Some(opacity) = self.num(opacity_key) {
            color.a = (f32::from(color.a) * (opacity / 100.0).clamp(0.0, 1.0)).round() as u8;
        }
        color
    }

    fn stroke_width(&self) -> f32 {
        self.num("strokeWidth")
            .unwrap_or(1.0)
            .clamp(MIN_STROKE_WIDTH, MAX_STROKE_WIDTH)
    }

    fn opacity(&self) -> f32 {
        (self.num("opacity").unwrap_or(100.0) / 100.0).clamp(0.0, 1.0)
    }

    fn label_opacity_loss(&self) -> bool {
        self.opacity() < 1.0 && self.num("textOpacity").is_none_or(|opacity| opacity > 0.0)
    }

    fn label_style_loss(&self, source_is_edge: bool) -> bool {
        // Vertices default to 12px; edges and removed defaults use mxGraph's 11px.
        // These built-in vertex styles leave the default label settings unchanged.
        if !self.tokens.is_empty()
            && (source_is_edge
                || self.tokens.len() > 1
                || self.get("shape").is_some()
                || !matches!(
                    self.tokens.first().map(String::as_str),
                    Some("ellipse" | "rhombus" | "triangle")
                ))
        {
            return true;
        }
        match self.get("fontSize") {
            None => source_is_edge || self.suppress_defaults,
            Some("none") => true,
            Some(_) => self
                .num("fontSize")
                .is_none_or(|size| size != CANVAS_LABEL_FONT_SIZE),
        }
    }

    fn double_ellipse_label_loss(&self, has_label: bool) -> bool {
        if !has_label {
            return false;
        }
        let nonzero_spacing = |key| {
            self.get(key)
                .is_some_and(|value| value != "none" && self.num(key).is_none_or(|v| v != 0.0))
        };
        self.get("whiteSpace") == Some("wrap")
            || self
                .get("overflow")
                .is_some_and(|value| !matches!(value, "" | "none" | "visible"))
            || self
                .get("labelPosition")
                .is_some_and(|value| !matches!(value, "" | "none" | "center"))
            || self
                .get("verticalLabelPosition")
                .is_some_and(|value| !matches!(value, "" | "none" | "middle"))
            || self
                .get("horizontal")
                .is_some_and(|value| !matches!(value, "" | "none" | "1"))
            || self.flag("noLabel")
            || self
                .get("textDirection")
                .is_some_and(|value| !matches!(value, "" | "none" | "default"))
            || self
                .get("labelWidth")
                .is_some_and(|value| !matches!(value, "" | "none"))
            || [
                "spacing",
                "spacingTop",
                "spacingBottom",
                "spacingLeft",
                "spacingRight",
            ]
            .into_iter()
            .any(nonzero_spacing)
            || self
                .get("fontStyle")
                .is_some_and(|value| value != "none" && self.num("fontStyle").is_none())
    }

    fn connector_label_appearance_loss(&self, source_is_edge: bool) -> bool {
        let text = self.text_color();
        let visible = text.is_none_or(|color| color.a > 0);
        let translucent = text.is_some_and(|color| color.a > 0 && color.a < 255);
        matches!(self.get("fontColor"), None | Some("" | "default"))
            || visible
                && (!source_is_edge
                    || self.get("labelBackgroundColor") == Some("none")
                    || translucent)
    }

    fn band(&self) -> Band {
        if self.get("thinodaBand") == Some("below") {
            Band::Below
        } else {
            Band::Above
        }
    }

    fn rotation(&self) -> f32 {
        self.num("rotation").unwrap_or(0.0)
    }

    fn start_arrow(&self) -> bool {
        self.get("startArrow")
            .is_some_and(|value| !matches!(value, "" | "none"))
    }

    fn end_arrow(&self) -> bool {
        match self.get("endArrow") {
            Some(value) => !matches!(value, "" | "none"),
            None => !self.suppress_defaults,
        }
    }

    fn marker(&self, key: &str) -> Option<ConnectorMarker> {
        let name = match self.get(key) {
            Some(name) => name,
            None if key == "endArrow" && !self.suppress_defaults => "classic",
            None => return None,
        };
        let (legacy_key, size_key, fill_key) = if key == "startArrow" {
            ("thinodaLegacyStartArrow", "startSize", "startFill")
        } else {
            ("thinodaLegacyEndArrow", "endSize", "endFill")
        };
        if name == "classic"
            && self.flag(legacy_key)
            && self
                .get(size_key)
                .is_none_or(|_| self.num(size_key) == Some(6.0))
            && self
                .get(fill_key)
                .is_none_or(|_| self.num(fill_key) == Some(1.0))
        {
            return None;
        }
        let kind = match name {
            "classic" => ConnectorMarkerKind::Classic,
            "classicThin" => ConnectorMarkerKind::ClassicThin,
            "open" => ConnectorMarkerKind::Open,
            "openThin" => ConnectorMarkerKind::OpenThin,
            "block" => ConnectorMarkerKind::Block,
            "blockThin" => ConnectorMarkerKind::BlockThin,
            "diamond" => ConnectorMarkerKind::Diamond,
            "diamondThin" => ConnectorMarkerKind::DiamondThin,
            "oval" => ConnectorMarkerKind::Oval,
            "halfCircle" => ConnectorMarkerKind::HalfCircle,
            "baseDash" => ConnectorMarkerKind::BaseDash,
            "dash" => ConnectorMarkerKind::Dash,
            _ => return None,
        };
        Some(ConnectorMarker { kind })
    }

    fn marker_size(&self, arrow_key: &str, size_key: &str) -> Option<f32> {
        self.marker(arrow_key)?;
        self.num(size_key)
            .filter(|value| valid_connector_marker_size(*value))
            .filter(|value| *value != DEFAULT_CONNECTOR_MARKER_SIZE)
    }

    fn marker_unfilled(&self, arrow_key: &str, fill_key: &str) -> bool {
        self.marker(arrow_key).is_some() && self.num(fill_key) == Some(0.0)
    }

    fn color_appearance_loss(&self, key: &str, opacity_key: &str) -> bool {
        let unresolved_color = self
            .get(key)
            .is_some_and(|value| parse_style_color(value).is_none_or(|parsed| parsed.approximate));
        let unresolved_opacity = self.get(opacity_key).is_some_and(|_| {
            self.num(opacity_key).is_none_or(|opacity| {
                !(0.0..=100.0).contains(&opacity)
                    || (opacity != 100.0 && self.color(key, opacity_key).is_none())
            })
        });
        unresolved_color || unresolved_opacity
    }

    /// Visual details without a Thinoda equivalent.
    fn formatting_loss(&self, edge: bool, card_text_alignment: bool) -> bool {
        const FLAGS: [&str; 7] = [
            "dashed", "shadow", "glass", "sketch", "flipH", "flipV", "comic",
        ];
        const PORT_KEYS: [&str; 4] = [
            "sourcePort",
            "targetPort",
            "sourcePortConstraint",
            "targetPortConstraint",
        ];
        let colored = |key: &str| {
            self.get(key)
                .is_some_and(|v| !matches!(v, "" | "none" | "default"))
        };
        let differs_from = |key: &str, default: f32| {
            self.get(key)
                .is_some_and(|_| self.num(key).is_none_or(|value| value != default))
        };
        (self.get("shape").is_some() && !self.tokens.is_empty())
            || FLAGS
                .iter()
                .any(|key| self.flag(key) && !(*key == "dashed" && self.flag("thinodaSankeyEdge")))
            || self.get("fontColor") == Some("none")
            || self.color_appearance_loss("fillColor", "fillOpacity")
            || self.color_appearance_loss("strokeColor", "strokeOpacity")
            || self.color_appearance_loss("fontColor", "textOpacity")
            || (self.shape() == "swimlane"
                && self.get("swimlaneFillColor").is_some()
                && self.color_appearance_loss("swimlaneFillColor", "fillOpacity"))
            || self.get("opacity").is_some_and(|_| {
                self.num("opacity")
                    .is_none_or(|opacity| !(0.0..=100.0).contains(&opacity))
            })
            || self.get("strokeWidth").is_some_and(|_| {
                self.num("strokeWidth").is_none_or(|width| {
                    !(if self.flag("thinodaSankeyEdge") {
                        0.0
                    } else {
                        MIN_STROKE_WIDTH
                    }..=MAX_STROKE_WIDTH)
                        .contains(&width)
                })
            })
            || colored("gradientColor")
            || colored("labelBackgroundColor")
            || colored("labelBorderColor")
            || self.get("fontFamily").is_some()
            || self.num("fontSize").is_some_and(|v| v != 11.0 && v != 12.0)
            || self.num("fontStyle").is_some_and(|v| v != 0.0)
            || self.get("align").is_some_and(|v| {
                if card_text_alignment {
                    self.text_align().is_none()
                } else {
                    v != "center"
                }
            })
            || self.get("verticalAlign").is_some_and(|v| {
                // Thinoda image cards caption beneath the picture.
                v != "middle" && !(v == "bottom" && self.get("thinodaCard") == Some("image"))
            })
            || self.get("portConstraint").is_some()
            || differs_from("portConstraintRotation", 0.0)
            || differs_from("perimeterSpacing", 0.0)
            || (edge
                && (self.get("jumpStyle").is_some_and(|v| v != "none")
                    || ["startArrow", "endArrow"].iter().any(|key| {
                        self.get(key).is_some_and(|v| {
                            !matches!(
                                v,
                                "" | "none"
                                    | "classic"
                                    | "classicThin"
                                    | "open"
                                    | "openThin"
                                    | "block"
                                    | "blockThin"
                                    | "diamond"
                                    | "diamondThin"
                                    | "oval"
                                    | "halfCircle"
                                    | "baseDash"
                                    | "dash"
                            )
                        })
                    })
                    || [("startArrow", "startFill"), ("endArrow", "endFill")]
                        .iter()
                        .any(|(arrow, key)| {
                            self.get(key).is_some_and(|_| {
                                self.num(key).is_none_or(|value| {
                                    (value != 0.0 && value != 1.0)
                                        || (value == 0.0 && self.marker(arrow).is_none())
                                })
                            })
                        })
                    || [("startArrow", "startSize"), ("endArrow", "endSize")]
                        .iter()
                        .any(|(arrow, key)| {
                            self.get(key).is_some_and(|_| {
                                self.num(key).is_none_or(|value| {
                                    !valid_connector_marker_size(value)
                                        || (value != DEFAULT_CONNECTOR_MARKER_SIZE
                                            && self.marker(arrow).is_none())
                                })
                            })
                        })
                    || ["sourcePerimeterSpacing", "targetPerimeterSpacing"]
                        .iter()
                        .any(|key| differs_from(key, 0.0))
                    || PORT_KEYS.iter().any(|key| self.get(key).is_some())))
    }
}

fn truncate_chars(text: String, max: usize, warnings: &mut BTreeSet<ImportWarning>) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => {
            warnings.insert(ImportWarning::Other);
            text[..end].to_owned()
        }
        None => text,
    }
}

// ---------------------------------------------------------------------------------------
// Cells

#[derive(Debug, Default)]
struct Geometry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    invalid_frame: bool,
    relative: bool,
    source: Option<Vec2>,
    target: Option<Vec2>,
    offset: Option<Vec2>,
    points: Vec<Vec2>,
}

#[derive(Debug)]
struct Cell {
    id: String,
    parent: Option<String>,
    value: String,
    style: Style,
    vertex: bool,
    edge: bool,
    source: Option<String>,
    target: Option<String>,
    visible: bool,
    geometry: Geometry,
    link: Option<String>,
    tooltip: Option<String>,
}

fn number(element: &Element, key: &str) -> Option<f32> {
    match element.attr(key) {
        None => Some(0.0),
        Some(value) => value
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite()),
    }
}

fn point(element: &Element, warnings: &mut BTreeSet<ImportWarning>) -> Option<Vec2> {
    match (number(element, "x"), number(element, "y")) {
        (Some(x), Some(y)) => Some(Vec2::new(x, y)),
        _ => {
            warnings.insert(ImportWarning::Other);
            None
        }
    }
}

fn read_geometry(element: Option<&Element>, warnings: &mut BTreeSet<ImportWarning>) -> Geometry {
    let Some(element) = element else {
        return Geometry::default();
    };
    let (x, y, width, height, invalid_frame) = match (
        number(element, "x"),
        number(element, "y"),
        number(element, "width"),
        number(element, "height"),
    ) {
        (Some(x), Some(y), Some(width), Some(height)) if width >= 0.0 && height >= 0.0 => {
            (x, y, width, height, false)
        }
        _ => {
            warnings.insert(ImportWarning::Other);
            (0.0, 0.0, 0.0, 0.0, true)
        }
    };
    let mut geometry = Geometry {
        x,
        y,
        width,
        height,
        invalid_frame,
        relative: element.attr("relative") == Some("1"),
        ..Geometry::default()
    };
    for child in &element.children {
        match (child.name.as_str(), child.attr("as")) {
            ("mxPoint", Some("sourcePoint")) => geometry.source = point(child, warnings),
            ("mxPoint", Some("targetPoint")) => geometry.target = point(child, warnings),
            ("mxPoint", Some("offset")) => geometry.offset = point(child, warnings),
            ("Array", Some("points")) => {
                geometry.points = child
                    .children
                    .iter()
                    .filter(|p| p.name == "mxPoint")
                    .filter_map(|p| point(p, warnings))
                    .collect();
            }
            _ => {}
        }
    }
    geometry
}

fn read_cell(
    outer: &Element,
    inner: &Element,
    user: Option<&Element>,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Cell {
    let owned = |v: Option<&str>| v.filter(|v| !v.is_empty()).map(str::to_owned);
    Cell {
        id: outer.attr("id").unwrap_or_default().to_owned(),
        parent: owned(inner.attr("parent")),
        value: match user {
            Some(user) => user.attr("label"),
            None => inner.attr("value"),
        }
        .unwrap_or_default()
        .to_owned(),
        style: Style::parse(inner.attr("style").unwrap_or_default()),
        vertex: inner.attr("vertex") == Some("1"),
        edge: inner.attr("edge") == Some("1"),
        source: owned(inner.attr("source")),
        target: owned(inner.attr("target")),
        visible: inner.attr("visible") != Some("0"),
        geometry: read_geometry(inner.child("mxGeometry"), warnings),
        link: owned(user.and_then(|u| u.attr("link"))),
        tooltip: owned(user.and_then(|u| u.attr("tooltip"))),
    }
}

fn read_cells(model: &Element, warnings: &mut BTreeSet<ImportWarning>) -> Vec<Cell> {
    let Some(root) = model.child("root") else {
        return Vec::new();
    };
    let mut cells: Vec<Cell> = Vec::new();
    let mut ids = HashSet::new();
    for element in &root.children {
        let mut cell = match element.name.as_str() {
            "mxCell" => read_cell(element, element, None, warnings),
            "UserObject" | "object" => {
                let Some(inner) = element.child("mxCell") else {
                    warnings.insert(ImportWarning::Other);
                    continue;
                };
                match element.attr("placeholders") {
                    Some("1") => {
                        warnings.insert(ImportWarning::DrawioPlaceholdersUnsupported);
                    }
                    None | Some("0") => {}
                    Some(_) => {
                        warnings.insert(ImportWarning::Other);
                    }
                }
                if element.attr("placeholder").is_some() {
                    warnings.insert(ImportWarning::DrawioPlaceholdersUnsupported);
                }
                if element.attrs.iter().any(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "id" | "label" | "link" | "tooltip" | "placeholders"
                    )
                }) {
                    warnings.insert(ImportWarning::Other);
                }
                read_cell(element, inner, Some(element), warnings)
            }
            _ => {
                warnings.insert(ImportWarning::Other);
                continue;
            }
        };
        if cell.id.is_empty() {
            cell.id = format!("\u{0}{}", cells.len());
        }
        if !ids.insert(cell.id.clone()) {
            warnings.insert(ImportWarning::Other);
            continue;
        }
        cells.push(cell);
    }
    cells
}

// ---------------------------------------------------------------------------------------
// Import

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Group,
    Container,
    /// Shape, its base orientation in degrees relative to draw.io's east-facing
    /// default, and whether a quarter turn leaves the outline unchanged.
    Shape(ShapeKind, f32, bool),
    /// A built-in shape drawn as the nearest Thinoda shape (a formatting loss).
    Approx(ShapeKind, f32, bool),
    /// A connection point drawn as a small dot in the stroke color.
    Waypoint,
    /// A dummy stencil whose shape is invisible but whose label and bounds remain.
    Anchor,
    Text,
    Line,
    Image,
    Card(CardKind),
    Unknown,
}

/// Resolves draw.io's numeric style values before its mockup renderer calls `parseInt`.
fn mockup_radius_sign(value: &str) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;

    let value = value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if value == "none" {
        return Some(Ordering::Greater);
    }
    if let Ok(number) = value.parse::<f64>()
        && number.is_finite()
    {
        let magnitude = number.abs();
        // JavaScript stringifies smaller nonzero numbers in exponent notation.
        return Some(if number == 0.0 || (1e-6..1.0).contains(&magnitude) {
            Ordering::Equal
        } else if number < 0.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }

    let (negative, value) = if let Some(value) = value.strip_prefix('-') {
        (true, value)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let (hex, value) = if let Some(value) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        (true, value)
    } else {
        (false, value)
    };
    let mut digits = value
        .bytes()
        .take_while(|byte| byte.is_ascii_digit() || (hex && byte.is_ascii_hexdigit()))
        .peekable();
    digits.peek()?;
    let nonzero = digits.any(|byte| byte != b'0');
    Some(match (negative, nonzero) {
        (_, false) => Ordering::Equal,
        (false, true) => Ordering::Greater,
        (true, true) => Ordering::Less,
    })
}

/// Resolves the positive integer passed to the renderer, bounded by the largest
/// representable frame. Larger values have the same clamped visible corners.
fn mockup_radius(value: &str) -> f32 {
    let value = value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if value == "none" {
        return 10.0;
    }
    if mockup_radius_sign(value) != Some(std::cmp::Ordering::Greater) {
        return 0.0;
    }
    if let Ok(number) = value.parse::<f64>()
        && number.is_finite()
    {
        if !(1e-6..1e21).contains(&number) {
            return format!("{number:e}")
                .bytes()
                .next()
                .map_or(0.0, |digit| f32::from(digit - b'0'));
        }
        return number.trunc().min(f64::from(MAX_BOARD_COORDINATE)) as f32;
    }
    let value = value.strip_prefix('+').unwrap_or(value);
    let (radix, digits) = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        (16, hex)
    } else {
        (10, value)
    };
    digits
        .bytes()
        .map_while(|byte| (byte as char).to_digit(radix))
        .fold(0.0_f32, |result, digit| {
            (result * radix as f32 + digit as f32).min(MAX_BOARD_COORDINATE)
        })
}

fn mockup_rectangle_stencil(style: &Style) -> Option<DrawioRectangleStencil> {
    Some(match style.get("shape")? {
        "mxgraph.mockup.rrect" => DrawioRectangleStencil::Mockup,
        "mxgraph.mockup.forms.rrect" => DrawioRectangleStencil::Forms,
        "mxgraph.mockup.containers.rrect" => DrawioRectangleStencil::Containers,
        "mxgraph.mockup.misc.rrect" => DrawioRectangleStencil::Misc,
        "mxgraph.mockup.graphics.rrect" => DrawioRectangleStencil::Graphics,
        "mxgraph.mockup.text.rrect" => DrawioRectangleStencil::Text,
        "mxgraph.ios.rrect" => DrawioRectangleStencil::Ios,
        _ => return None,
    })
}

fn is_mockup_rectangle(style: &Style) -> bool {
    mockup_rectangle_stencil(style).is_some()
}

fn is_standard_rounded_rectangle(style: &Style) -> bool {
    matches!(style.shape(), "" | "rect" | "rectangle" | "label")
        && style.num("rounded") == Some(1.0)
}

fn native_rounded_rectangle_hint(style: &Style) -> bool {
    style.get("thinodaShape") == Some("nativeRoundedRectangle")
        && ["arcSize", "absoluteArcSize"]
            .into_iter()
            .all(|key| style.get(key).is_none_or(|value| value == "none"))
        && style
            .get("roundedPerimeter")
            .is_none_or(|value| matches!(value, "none" | "0"))
}

fn standard_rectangle_rounding(style: &Style) -> (ShapeRounding, bool) {
    let mut approximate = false;
    let mut flag = |key| match style.get(key) {
        None | Some("none") => false,
        Some(_) if style.num(key) == Some(1.0) => true,
        Some(_) if style.num(key) == Some(0.0) => false,
        Some(_) => {
            approximate = true;
            false
        }
    };
    let absolute = flag("absoluteArcSize");
    let rounded_perimeter = flag("roundedPerimeter");
    let default = if absolute { 20.0 } else { 15.0 };
    let radius = match style.get("arcSize") {
        None | Some("none") => default,
        Some(_) => match style.num("arcSize") {
            Some(value) if (0.0..=MAX_BOARD_COORDINATE).contains(&value) => value,
            Some(value) => {
                approximate = true;
                value.clamp(0.0, MAX_BOARD_COORDINATE)
            }
            None => {
                approximate = true;
                0.0
            }
        },
    };
    (
        ShapeRounding {
            radius,
            stencil: DrawioRectangleStencil::Standard,
            mode: if absolute {
                ShapeRoundingMode::AbsoluteArc
            } else {
                ShapeRoundingMode::Relative
            },
            rounded_perimeter,
        },
        approximate,
    )
}

const FLOWCHART_ANNOTATION_1: &str = "mxgraph.flowchart.annotation_1";
const FLOWCHART_PROCESS: &str = "mxgraph.flowchart.process";

fn mockup_rectangle(style: &Style) -> Kind {
    use std::cmp::Ordering;

    let radius = style
        .get("rSize")
        .map_or(Some(Ordering::Greater), mockup_radius_sign);
    let unresolved_color = [
        ("fillColor", "fillOpacity"),
        ("strokeColor", "strokeOpacity"),
        ("fontColor", "textOpacity"),
    ]
    .iter()
    .any(|(key, opacity)| style.get(key).is_none() || style.color_appearance_loss(key, opacity));
    let unresolved_opacity = [
        ("fillColor", "fillOpacity"),
        ("strokeColor", "strokeOpacity"),
        ("fontColor", "textOpacity"),
    ]
    .iter()
    .any(|(color, opacity)| {
        style.get(opacity).is_some_and(|_| {
            style
                .num(opacity)
                .is_none_or(|value| value != 100.0 && style.color(color, opacity).is_none())
        })
    });
    let unresolved_stylesheet = style.suppress_defaults || !style.tokens.is_empty();
    match radius {
        Some(Ordering::Equal)
            if !unresolved_color && !unresolved_opacity && !unresolved_stylesheet =>
        {
            Kind::Shape(ShapeKind::Rectangle, 0.0, true)
        }
        Some(Ordering::Greater) => Kind::Approx(ShapeKind::RoundedRectangle, 0.0, true),
        _ => Kind::Approx(ShapeKind::Rectangle, 0.0, true),
    }
}

fn classify(style: &Style) -> Kind {
    if style.has("group") {
        return Kind::Group;
    }
    match style.stencil() {
        Some(stencil) if stencil.resolve().is_some() => {
            Kind::Shape(ShapeKind::Rectangle, 0.0, true)
        }
        // An uninstalled stencil is still a shape instance: keep its native
        // fallback when that is a shape, so the reference survives re-export.
        Some(_) => match classify_native(style) {
            kind @ (Kind::Shape(..) | Kind::Approx(..)) => kind,
            _ => Kind::Shape(ShapeKind::Rectangle, 0.0, true),
        },
        None => classify_native(style),
    }
}

fn classify_native(style: &Style) -> Kind {
    if let Some(kind) = style.get("thinodaCard") {
        return Kind::Card(match kind {
            "text" => CardKind::Text,
            "image" => CardKind::Image,
            _ if style.has("text") => CardKind::Text,
            _ => CardKind::Sticky,
        });
    }
    if matches!(
        style.get("shape"),
        Some(
            "mxgraph.mockup.anchor"
                | "mxgraph.mockup.forms.anchor"
                | "mxgraph.mockup.containers.anchor"
                | "mxgraph.mockup.misc.anchor"
                | "mxgraph.mockup.graphics.anchor"
                | "mxgraph.mockup.navigation.anchor"
                | "mxgraph.ios.anchor"
                | "mxgraph.ios7ui.anchor"
                | "mxgraph.android.anchor"
                | "mxgraph.bootstrap.anchor"
                | "mxgraph.eip.anchor"
                | "mxgraph.er.anchor"
        )
    ) {
        return Kind::Anchor;
    }
    let name = style.shape();
    if name == "swimlane" || style.has("swimlane") || style.flag("container") {
        return Kind::Container;
    }
    if style.get("image").is_some_and(|v| !v.is_empty()) {
        return Kind::Image;
    }
    if is_mockup_rectangle(style) {
        return mockup_rectangle(style);
    }
    use ShapeKind as S;
    let shape = |kind, base| Kind::Shape(kind, base, false);
    let symmetric = |kind| Kind::Shape(kind, 0.0, true);
    match name {
        "" | "rect" | "rectangle" | "label" => {
            if style.num("rounded") == Some(1.0) {
                symmetric(S::RoundedRectangle)
            } else if style.get("shape") == Some("rectangle")
                && style.get("rounded") == Some("0")
                && style.get("thinodaShape") == Some("process")
            {
                symmetric(S::Process)
            } else {
                symmetric(S::Rectangle)
            }
        }
        "text" => Kind::Text,
        "note" | "note2" => Kind::Card(CardKind::Sticky),
        "line" => Kind::Line,
        "image" => Kind::Image,
        "ellipse"
        | "mxgraph.flowchart.start_1"
        | "mxgraph.flowchart.start_2"
        | "mxgraph.flowchart.on-page_reference" => symmetric(S::Ellipse),
        "doubleEllipse" => symmetric(S::DoubleEllipse),
        "endState" => symmetric(S::FinalState),
        FLOWCHART_ANNOTATION_1 if style.get("shape") == Some(FLOWCHART_ANNOTATION_1) => {
            Kind::Approx(S::Rectangle, 0.0, false)
        }
        "partialRectangle" => shape(S::Rectangle, 0.0),
        "mxgraph.basic.rect" | "mxgraph.bootstrap.rect" => symmetric(S::Rectangle),
        "mxgraph.bootstrap.rrect" => symmetric(S::RoundedRectangle),
        "waypoint" => Kind::Waypoint,
        "folder" | "cube" | "message" | "umlFrame" => Kind::Approx(S::Rectangle, 0.0, true),
        "mxgraph.basic.partConcEllipse" => Kind::Approx(S::Ellipse, 0.0, true),
        "mxgraph.arrows2.arrow" => Kind::Approx(S::Arrow, 0.0, false),
        "rhombus" => symmetric(S::Diamond),
        "triangle" => shape(S::Triangle, 90.0),
        "mxgraph.flowchart.extract_or_measurement" => shape(S::Triangle, 0.0),
        "mxgraph.flowchart.merge_or_storage" => shape(S::Triangle, 180.0),
        "hexagon" => shape(S::Hexagon, 0.0),
        "cylinder" | "cylinder3" | "mxgraph.flowchart.database" => shape(S::Cylinder, 0.0),
        "cloud" => shape(S::Cloud, 0.0),
        "document" | "mxgraph.flowchart.document" => shape(S::Document, 0.0),
        "process" | "mxgraph.flowchart.predefined_process" => shape(S::PredefinedProcess, 0.0),
        "parallelogram" => shape(S::Parallelogram, 0.0),
        "trapezoid" => shape(S::Trapezoid, 0.0),
        "step" => shape(S::Chevron, 0.0),
        "singleArrow" => shape(S::Arrow, 0.0),
        "doubleArrow" => shape(S::DoubleArrow, 0.0),
        "callout" => shape(S::Callout, 0.0),
        "cross" => symmetric(S::Plus),
        "mxgraph.basic.star" => shape(S::Star, 0.0),
        "mxgraph.basic.pentagon" => shape(S::Pentagon, 0.0),
        "mxgraph.basic.octagon2" => shape(S::Octagon, 0.0),
        "mxgraph.basic.orthogonal_triangle" => shape(S::RightTriangle, 0.0),
        FLOWCHART_PROCESS if style.get("shape") == Some(FLOWCHART_PROCESS) => {
            Kind::Approx(S::Process, 0.0, false)
        }
        "mxgraph.flowchart.decision" => shape(S::Decision, 0.0),
        "mxgraph.flowchart.terminator" => shape(S::Terminator, 0.0),
        "mxgraph.flowchart.data" => shape(S::Data, 0.0),
        "mxgraph.flowchart.manual_input" => shape(S::ManualInput, 0.0),
        "mxgraph.flowchart.preparation" => shape(S::Preparation, 0.0),
        "delay" | "mxgraph.flowchart.delay" => shape(S::Delay, 0.0),
        "mxgraph.flowchart.off-page_reference" | "offPageConnector" => {
            shape(S::OffPageConnector, 0.0)
        }
        _ => Kind::Unknown,
    }
}

/// draw.io `partialRectangle` side flags, in clockwise path order.
const PARTIAL_SIDES: [&str; 4] = ["top", "right", "bottom", "left"];

/// Default draw.io waypoint size before stroke-width adjustment.
const WAYPOINT_SIZE: f32 = 6.0;

fn waypoint_diameter(style: &Style) -> f32 {
    let size = style.num("size").unwrap_or(WAYPOINT_SIZE);
    let width = style
        .num("strokeWidth")
        .filter(|width| *width >= 0.0)
        .unwrap_or(1.0);
    let diameter = (size - 2.0).max(0.0) + 2.0 * width;
    if diameter.is_finite() && diameter <= MAX_BOARD_COORDINATE {
        diameter
    } else {
        WAYPOINT_SIZE
    }
}

fn waypoint_absolute_port(cell: &Cell, port: ConnectorPort) -> Option<ConnectorPort> {
    let style = &cell.style;
    let direction = match style.get("direction") {
        None | Some("" | "none" | "east") => "east",
        Some("north") => "north",
        Some("west") => "west",
        Some("south") => "south",
        Some(_) => return None,
    };
    let anchor_direction = match style.get("anchorPointDirection") {
        None | Some("" | "none") => true,
        Some(_) => style.num("anchorPointDirection")? == 1.0,
    };
    let spacing = match style.get("perimeterSpacing") {
        None | Some("" | "none") => 0.0,
        Some(_) => style.num("perimeterSpacing")?,
    };
    let sideways = matches!(direction, "north" | "south");
    let mut width = cell.geometry.width + 2.0 * spacing;
    let mut height = cell.geometry.height + 2.0 * spacing;
    if sideways && anchor_direction {
        std::mem::swap(&mut width, &mut height);
    }
    let mut offset = Vec2::new(
        (port.position.x - 0.5) * width + port.offset.x,
        (port.position.y - 0.5) * height + port.offset.y,
    );
    let mut flip_h = style.num("flipH") == Some(1.0);
    let mut flip_v = style.num("flipV") == Some(1.0);
    if sideways {
        std::mem::swap(&mut flip_h, &mut flip_v);
    }
    if flip_h {
        offset.x = -offset.x;
    }
    if flip_v {
        offset.y = -offset.y;
    }
    let direction_rotation = if anchor_direction {
        direction_degrees(style)
    } else {
        0.0
    };
    let rotation = (style.rotation() + direction_rotation).rem_euclid(360.0);
    let mapped = ConnectorPort {
        position: Vec2::new(0.5, 0.5),
        offset: rotate_around(offset, Vec2::ZERO, rotation),
        perimeter: false,
    };
    mapped.is_valid().then_some(mapped)
}

fn direction_degrees(style: &Style) -> f32 {
    match style.get("direction") {
        Some("south") => 90.0,
        Some("west") => 180.0,
        Some("north") => 270.0,
        _ => 0.0,
    }
}

fn object_style(style: &Style) -> ObjectStyle {
    ObjectStyle {
        fill: style.color("fillColor", "fillOpacity"),
        stroke: style.color("strokeColor", "strokeOpacity"),
        text_color: style.text_color(),
        stroke_width: style.stroke_width(),
        opacity: style.opacity(),
    }
}

/// Resolves visible light-theme fallbacks while reporting lost adaptive styling.
fn resolved_shape_colors(
    style: &Style,
    object: &mut CanvasObject,
    has_label: bool,
    adaptive_fixed_colors: bool,
    outline_visible: bool,
) -> bool {
    let mut approximate = !style.tokens.is_empty()
        || has_label && matches!(style.get("fontColor"), None | Some("" | "default"));
    for (key, opacity_key, fallback, target) in [
        (
            "fillColor",
            "fillOpacity",
            Color::WHITE,
            &mut object.style.fill,
        ),
        (
            "strokeColor",
            "strokeOpacity",
            Color::BLACK,
            &mut object.style.stroke,
        ),
    ] {
        let value = style.get(key);
        *target = match value {
            None if style.suppress_defaults => Some(Color::TRANSPARENT),
            None | Some("" | "default") => {
                approximate = true;
                Some(style.color_with_opacity(fallback, opacity_key))
            }
            Some("none") => Some(Color::TRANSPARENT),
            Some(_) => {
                approximate |= style.color_appearance_loss(key, opacity_key);
                style.color(key, opacity_key).or_else(|| {
                    approximate = true;
                    Some(style.color_with_opacity(fallback, opacity_key))
                })
            }
        };
        if style.get(opacity_key).is_some() && style.num(opacity_key).is_none() {
            approximate = true;
        }
    }
    if adaptive_fixed_colors && object.style.opacity > 0.0 {
        let visible_outline =
            outline_visible && object.style.stroke.is_some_and(|color| color.a > 0);
        approximate |= object.style.fill.is_some_and(|color| color.a > 0)
            || visible_outline
            || has_label && object.style.text_color.is_some_and(|color| color.a > 0);
    }
    approximate
}

#[derive(Debug, Clone, Copy)]
enum Item {
    Object(CanvasObjectId),
    Card(TopicId),
    /// Groups and Zones: edges attached to them end at the centre.
    Area(Rect),
}

#[derive(Debug, Clone, Copy, Default)]
struct Context {
    /// Absolute top-left of the parent vertex; children are relative to it.
    origin: Vec2,
    parent_size: Vec2,
    parent_is_vertex: bool,
    zone: Option<usize>,
    bucket: Option<usize>,
}

enum End {
    Ready { endpoint: Endpoint, detached: bool },
    Pending,
    Missing,
}

struct PageImport<'a> {
    cells: &'a [Cell],
    index: HashMap<&'a str, usize>,
    children: Vec<Vec<usize>>,
    sheet: Sheet,
    warnings: &'a mut BTreeSet<ImportWarning>,
    items: HashMap<usize, Item>,
    shapes: HashMap<CanvasObjectId, ShapeKind>,
    visited: Vec<bool>,
    paint_order: Vec<usize>,
    zones: Vec<(ZoneId, Vec<TopicId>, Vec<CanvasObjectId>)>,
    buckets: Vec<(Vec<BoardMember>, bool)>,
    locked: Vec<BoardMember>,
    deferred: Vec<(usize, Context)>,
    adaptive_fixed_colors: bool,
}

impl<'a> PageImport<'a> {
    fn warn(&mut self, warning: ImportWarning) {
        self.warnings.insert(warning);
    }

    fn label(&mut self, cell: &Cell) -> String {
        let text = if cell.style.flag("html") {
            strip_html(&cell.value, self.warnings)
        } else {
            cell.value.clone()
        };
        truncate_chars(text, MAX_BOARD_TEXT_CHARS, self.warnings)
    }

    fn translated_point(&mut self, origin: Vec2, point: Vec2) -> Option<Vec2> {
        let point = origin + point;
        if valid_board_position(point) {
            Some(point)
        } else {
            self.warn(ImportWarning::Other);
            None
        }
    }

    fn translated_points(&mut self, origin: Vec2, points: impl Iterator<Item = Vec2>) -> Vec<Vec2> {
        points
            .filter_map(|point| self.translated_point(origin, point))
            .collect()
    }

    fn register(&mut self, cell: usize, item: Item, context: Context, locked: bool) {
        self.items.insert(cell, item);
        let member = match item {
            Item::Object(id) => BoardMember::Object(id),
            Item::Card(id) => BoardMember::Card(id),
            Item::Area(_) => return,
        };
        if let Some(zone) = context.zone {
            match member {
                BoardMember::Object(id) => self.zones[zone].2.push(id),
                BoardMember::Card(id) => self.zones[zone].1.push(id),
            }
        }
        if let Some(bucket) = context.bucket {
            self.buckets[bucket].0.push(member);
        }
        if locked {
            self.locked.push(member);
        }
    }

    fn add_object(
        &mut self,
        cell: usize,
        object: CanvasObject,
        context: Context,
    ) -> Option<CanvasObjectId> {
        let shape = match &object.kind {
            CanvasObjectKind::Shape { shape, .. } => Some(*shape),
            _ => None,
        };
        match self.sheet.add_canvas_object(object) {
            Ok(id) => {
                if let Some(shape) = shape {
                    self.shapes.insert(id, shape);
                }
                let locked = self.cells[cell].style.flag("locked");
                self.register(cell, Item::Object(id), context, locked);
                Some(id)
            }
            Err(error) => {
                log::warn!("draw.io cell {} skipped: {error}", self.cells[cell].id);
                self.warn(ImportWarning::Other);
                None
            }
        }
    }

    fn vertex_rect(&self, cell: &Cell, context: Context) -> Rect {
        let g = &cell.geometry;
        let mut min = if g.relative && context.parent_is_vertex {
            Vec2::new(
                context.origin.x + g.x * context.parent_size.x,
                context.origin.y + g.y * context.parent_size.y,
            )
        } else {
            Vec2::new(context.origin.x + g.x, context.origin.y + g.y)
        };
        if g.relative
            && let Some(offset) = g.offset
        {
            min = min + offset;
        }
        Rect::from_min_size(min, Vec2::new(g.width, g.height))
    }

    fn run(&mut self) {
        let roots: Vec<usize> = (0..self.cells.len())
            .filter(|&i| self.cells[i].parent.is_none())
            .collect();
        for root in roots {
            self.visited[root] = true;
            for layer in self.children[root].clone() {
                self.visited[layer] = true;
                let cell = &self.cells[layer];
                if !cell.visible {
                    self.warn(ImportWarning::Other);
                    continue;
                }
                if cell.vertex || cell.edge {
                    self.visited[layer] = false;
                    self.visit(layer, Context::default(), 0);
                } else {
                    for child in self.children[layer].clone() {
                        self.visit(child, Context::default(), 0);
                    }
                }
            }
        }
        let orphans: Vec<usize> = (0..self.cells.len())
            .filter(|&i| {
                self.cells[i]
                    .parent
                    .as_deref()
                    .is_some_and(|p| !self.index.contains_key(p))
            })
            .collect();
        if !orphans.is_empty() {
            self.warn(ImportWarning::Other);
        }
        for orphan in orphans {
            self.visit(orphan, Context::default(), 0);
        }
        for (cell, context) in std::mem::take(&mut self.deferred) {
            self.edge(cell, context, true);
        }
        let hidden = self.visited.iter().filter(|v| !**v).count();
        if hidden > 0 {
            self.warn(ImportWarning::Other);
        }
    }

    fn visit(&mut self, index: usize, context: Context, depth: usize) {
        if self.visited[index] {
            return;
        }
        self.visited[index] = true;
        let cells = self.cells;
        let cell = &cells[index];
        if !cell.visible || depth > MAX_CELL_DEPTH {
            self.warn(ImportWarning::Other);
            return;
        }
        self.paint_order.push(index);
        if cell.edge {
            for &child in &self.children[index] {
                // Edge labels are child vertices; they are folded into the edge.
                if cells[child].vertex {
                    self.visited[child] = true;
                }
            }
            self.edge(index, context, false);
            return;
        }
        if !cell.vertex {
            for child in self.children[index].clone() {
                self.visit(child, context, depth + 1);
            }
            return;
        }
        if cell.geometry.invalid_frame {
            log::warn!("draw.io cell {} skipped: malformed geometry", cell.id);
            self.warn(ImportWarning::Other);
            return;
        }
        let rect = self.vertex_rect(cell, context);
        let mut child_context = Context {
            origin: rect.min,
            parent_size: rect.size,
            parent_is_vertex: true,
            ..context
        };
        let has_children = self.children[index]
            .iter()
            .any(|&c| cells[c].visible && (cells[c].vertex || cells[c].edge));
        let kind = classify(&cell.style);
        let card_text_alignment = matches!(kind, Kind::Card(CardKind::Sticky | CardKind::Text));
        if cell.style.formatting_loss(false, card_text_alignment) {
            self.warn(ImportWarning::Formatting);
        }
        if matches!(cell.style.shape(), "note" | "note2") && !cell.style.draws_stencil() {
            self.warn(ImportWarning::DrawioFoldedNotesApproximated);
        }
        match kind {
            Kind::Group => {
                let bucket = context.bucket.unwrap_or_else(|| {
                    self.buckets.push((Vec::new(), false));
                    self.buckets.len() - 1
                });
                if cell.style.flag("locked") {
                    self.buckets[bucket].1 = true;
                }
                child_context.bucket = Some(bucket);
                self.items.insert(index, Item::Area(rect));
            }
            Kind::Container if let Some(zone) = self.zone(cell, rect) => {
                child_context.zone = Some(zone);
                child_context.bucket = None;
                self.items.insert(index, Item::Area(rect));
            }
            _ => {
                let drawn = self.draw(index, kind, rect, context);
                if has_children {
                    let bucket = match context.bucket {
                        Some(bucket) => bucket,
                        None => {
                            let members = drawn.into_iter().collect();
                            self.buckets.push((members, false));
                            self.buckets.len() - 1
                        }
                    };
                    child_context.bucket = Some(bucket);
                }
            }
        }
        for child in self.children[index].clone() {
            self.visit(child, child_context, depth + 1);
        }
    }

    /// A container frame, unless it cannot be a valid Zone.
    fn zone(&mut self, cell: &Cell, rect: Rect) -> Option<usize> {
        if self.sheet.zones.len() >= MAX_ZONES
            || rect.size.x < MIN_ZONE_SIZE
            || rect.size.y < MIN_ZONE_SIZE
            || cell.style.rotation() != 0.0
        {
            return None;
        }
        let label = self.label(cell);
        let mut name: String = label
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("Container")
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        while name.len() > MAX_PRESENTATION_NAME_BYTES {
            name.pop();
        }
        if label.trim() != name {
            self.warn(ImportWarning::Formatting);
        }
        let id = self.sheet.create_board_frame(&name, rect).ok()?;
        let swimlane = cell.style.shape() == "swimlane";
        if !swimlane {
            self.warn(ImportWarning::Formatting);
        }
        let stroke = cell.style.color("strokeColor", "strokeOpacity");
        let background = if swimlane {
            cell.style.color("swimlaneFillColor", "fillOpacity")
        } else {
            cell.style.color("fillColor", "fillOpacity")
        };
        if stroke.is_some_and(|color| color.a == 0)
            || swimlane && background.is_some_and(|color| color.a > 0)
            || swimlane && cell.style.get("fillColor").is_some()
            || !label.is_empty() && cell.style.get("fontColor").is_some()
        {
            self.warn(ImportWarning::Formatting);
        }
        if let Some(zone) = self.sheet.zones.iter_mut().find(|z| z.id == id) {
            zone.color = stroke.filter(|c| c.a > 0);
            zone.background_color = background;
            zone.clip = cell.style.flag("thinodaClip");
        }
        self.zones.push((id, Vec::new(), Vec::new()));
        Some(self.zones.len() - 1)
    }

    /// Draws a vertex and returns its board member, if one was created.
    fn draw(
        &mut self,
        index: usize,
        kind: Kind,
        rect: Rect,
        context: Context,
    ) -> Option<BoardMember> {
        let cells = self.cells;
        let cell = &cells[index];
        let style = &cell.style;
        let label = self.label(cell);
        if style.opacity() < 1.0 && matches!(kind, Kind::Card(_) | Kind::Image)
            || !label.is_empty()
                && (style.label_opacity_loss()
                    || kind == Kind::Line && style.connector_label_appearance_loss(false))
        {
            self.warn(ImportWarning::Formatting);
        }
        if cell.link.is_some() && !matches!(kind, Kind::Card(_)) {
            self.warn(ImportWarning::Links);
        }
        let shape_object = |shape: ShapeKind, frame: Rect, rotation: f32, label: String| {
            let mut object = CanvasObject::shape(shape, frame);
            if let CanvasObjectKind::Shape { label: text, .. } = &mut object.kind {
                *text = label;
            }
            object.rotation = rotation;
            object.style = object_style(style);
            object.band = style.band();
            object.alt_text = cell.tooltip.clone().unwrap_or_default();
            object
        };
        let object = match kind {
            Kind::Shape(..) if style.draws_stencil() => {
                // Stencils are drawn in their own frame; draw.io's `direction`
                // turns that frame like a rotation with swapped sides.
                let direction = direction_degrees(style).rem_euclid(360.0);
                let frame = if direction % 180.0 == 90.0 {
                    Rect::from_center_size(rect.center(), Vec2::new(rect.size.y, rect.size.x))
                } else {
                    rect
                };
                if style.flag("flipH") || style.flag("flipV") {
                    self.warn(ImportWarning::Formatting);
                }
                let rotation = direction + style.rotation();
                let mut object = shape_object(ShapeKind::Rectangle, frame, rotation, label);
                object.stencil = style.stencil();
                object
            }
            Kind::Shape(shape, base, symmetric) | Kind::Approx(shape, base, symmetric) => {
                if matches!(kind, Kind::Approx(..)) {
                    self.warn(ImportWarning::Formatting);
                }
                let total = if symmetric {
                    0.0
                } else {
                    (base + direction_degrees(style)).rem_euclid(360.0)
                };
                let frame = if total % 180.0 == 90.0 {
                    Rect::from_center_size(rect.center(), Vec2::new(rect.size.y, rect.size.x))
                } else {
                    rect
                };
                let rotation = total + style.rotation();
                let mut object = shape_object(shape, frame, rotation, label);
                if let Some(stencil) = mockup_rectangle_stencil(style) {
                    object.rounding = Some(ShapeRounding {
                        radius: style.get("rSize").map_or(10.0, mockup_radius),
                        stencil,
                        ..Default::default()
                    });
                } else if is_standard_rounded_rectangle(style)
                    && !native_rounded_rectangle_hint(style)
                {
                    let (rounding, approximate) = standard_rectangle_rounding(style);
                    object.rounding = Some(rounding);
                    if approximate
                        || style.constraint_perimeter != ConstraintPerimeter::Rectangle
                        || style
                            .get("direction")
                            .is_some_and(|value| !matches!(value, "" | "east" | "none"))
                    {
                        self.warn(ImportWarning::Formatting);
                    }
                }
                if style.get("shape") == Some(FLOWCHART_ANNOTATION_1) {
                    let has_area = frame.size.x > 0.0 && frame.size.y > 0.0;
                    object.rectangle_sides = Some(RectangleSides {
                        top: has_area,
                        right: false,
                        bottom: has_area,
                        left: has_area,
                    });
                    object.style.fill = Some(Color::TRANSPARENT);
                    let default_stroke =
                        if style.suppress_defaults && style.get("strokeColor").is_none() {
                            Color::TRANSPARENT
                        } else {
                            Color::BLACK
                        };
                    object.style.stroke = Some(
                        style
                            .color("strokeColor", "strokeOpacity")
                            .unwrap_or_else(|| {
                                style.color_with_opacity(default_stroke, "strokeOpacity")
                            }),
                    );
                }
                if style.shape() == "partialRectangle" {
                    object.rectangle_sides = Some(style.rectangle_sides());
                    let has_label = !object.label().is_empty();
                    let outline_visible = object.rectangle_sides.is_some_and(|sides| {
                        sides.top || sides.right || sides.bottom || sides.left
                    });
                    if resolved_shape_colors(
                        style,
                        &mut object,
                        has_label,
                        self.adaptive_fixed_colors,
                        outline_visible,
                    ) {
                        self.warn(ImportWarning::Formatting);
                    }
                }
                if shape == ShapeKind::DoubleEllipse {
                    if let Some(raw) = style.get("margin")
                        && raw != "none"
                    {
                        match style
                            .num("margin")
                            .filter(|value| (0.0..=MAX_BOARD_COORDINATE).contains(value))
                        {
                            Some(margin) => {
                                object.double_ellipse_margin = Some(margin);
                                if margin * 2.0 > frame.size.x.min(frame.size.y) {
                                    self.warn(ImportWarning::Formatting);
                                }
                            }
                            None => self.warn(ImportWarning::Formatting),
                        }
                    }
                    let has_label = !object.label().is_empty();
                    if style.double_ellipse_label_loss(has_label) {
                        self.warn(ImportWarning::Formatting);
                    }
                    if resolved_shape_colors(
                        style,
                        &mut object,
                        has_label,
                        self.adaptive_fixed_colors,
                        true,
                    ) {
                        self.warn(ImportWarning::Formatting);
                    }
                }
                if shape == ShapeKind::FinalState {
                    let has_label = !object.label().is_empty();
                    if style.double_ellipse_label_loss(has_label)
                        || style
                            .get("margin")
                            .is_some_and(|value| !matches!(value, "" | "none"))
                    {
                        self.warn(ImportWarning::Formatting);
                    }
                    if resolved_shape_colors(
                        style,
                        &mut object,
                        has_label,
                        self.adaptive_fixed_colors,
                        true,
                    ) {
                        self.warn(ImportWarning::Formatting);
                    }
                    if style.get("fillColor") == Some("strokeColor") {
                        let fill = match style.get("strokeColor") {
                            None if style.suppress_defaults => Color::TRANSPARENT,
                            Some("none") => Color::TRANSPARENT,
                            _ => style
                                .color("strokeColor", "fillOpacity")
                                .unwrap_or_else(|| {
                                    style.color_with_opacity(Color::BLACK, "fillOpacity")
                                }),
                        };
                        object.style.fill = Some(fill);
                        // The live link between fill and stroke is not retained.
                        self.warn(ImportWarning::Formatting);
                    }
                }
                if let Some(stencil) = style.stencil() {
                    // An uninstalled stencil keeps its reference; the classified
                    // legacy shape (with its normalized frame) is its fallback,
                    // without shape-specific options a stencil may not carry.
                    object.rounding = None;
                    object.rectangle_sides = None;
                    object.double_ellipse_margin = None;
                    object.stencil = Some(stencil);
                }
                object
            }
            Kind::Waypoint => {
                self.warn(ImportWarning::Formatting);
                let diameter = waypoint_diameter(style);
                let frame = Rect::from_center_size(rect.center(), Vec2::new(diameter, diameter));
                let mut object = shape_object(ShapeKind::Ellipse, frame, 0.0, label);
                let color = style
                    .color("strokeColor", "fillOpacity")
                    .unwrap_or_else(|| {
                        let fallback =
                            if style.suppress_defaults && style.get("strokeColor").is_none() {
                                Color::TRANSPARENT
                            } else {
                                Color::BLACK
                            };
                        style.color_with_opacity(fallback, "fillOpacity")
                    });
                object.style.fill = Some(color);
                object.style.stroke = Some(Color::TRANSPARENT);
                object
            }
            Kind::Anchor => {
                self.warn(ImportWarning::Formatting);
                let mut object = shape_object(ShapeKind::Rectangle, rect, style.rotation(), label);
                object.style.fill = Some(Color::TRANSPARENT);
                object.style.stroke = Some(Color::TRANSPARENT);
                object
            }
            Kind::Text => {
                let mut object = shape_object(ShapeKind::Rectangle, rect, style.rotation(), label);
                object.style.fill = Some(object.style.fill.unwrap_or(Color::TRANSPARENT));
                object.style.stroke = Some(object.style.stroke.unwrap_or(Color::TRANSPARENT));
                object
            }
            Kind::Image => {
                let source = style.get("image").unwrap_or_default();
                if let Some((resource, pixels)) = inline_image(source, rect.size) {
                    let keep_aspect = style.get("imageAspect") != Some("0");
                    let (card_rect, size) = image_card_rect(rect, pixels, &label, keep_aspect);
                    let image = CardImage {
                        resource,
                        size,
                        plain: true,
                    };
                    return self.card(
                        index,
                        CardKind::Image,
                        card_rect,
                        label,
                        context,
                        Some(image),
                    );
                }
                // Remote and library images are never fetched; keep their source as alt text.
                self.warn(ImportWarning::Resources);
                let mut object = shape_object(ShapeKind::Rectangle, rect, style.rotation(), label);
                if object.alt_text.is_empty() && !source.starts_with("data:") {
                    object.alt_text = source.chars().take(MAX_IMAGE_ALT_CHARS).collect();
                }
                object
            }
            Kind::Unknown | Kind::Container | Kind::Group => {
                if kind == Kind::Unknown {
                    self.warn(ImportWarning::UnsupportedDrawioShapes);
                    self.warn(ImportWarning::Other);
                } else {
                    self.warn(ImportWarning::Formatting);
                }
                shape_object(ShapeKind::Rectangle, rect, style.rotation(), label)
            }
            Kind::Line => {
                let center = rect.center();
                let vertical = matches!(style.get("direction"), Some("south" | "north"));
                let (a, b) = if vertical {
                    (
                        Vec2::new(center.x, rect.min.y),
                        Vec2::new(center.x, rect.max().y),
                    )
                } else {
                    (
                        Vec2::new(rect.min.x, center.y),
                        Vec2::new(rect.max().x, center.y),
                    )
                };
                let rotation = style.rotation();
                let mut connector = CanvasConnector::new(
                    Endpoint::Point(rotate_around(a, center, rotation)),
                    Endpoint::Point(rotate_around(b, center, rotation)),
                );
                connector.arrow_end = false;
                connector.label = label;
                connector.color = style.color("strokeColor", "strokeOpacity");
                connector.width = style.stroke_width();
                let mut object = CanvasObject::connector(connector);
                object.style.text_color = style.text_color();
                object.style.opacity = style.opacity();
                object.band = style.band();
                object
            }
            Kind::Card(card_kind) => {
                let image = (card_kind == CardKind::Image)
                    .then(|| style.get("image"))
                    .flatten()
                    .and_then(|source| inline_image(source, rect.size))
                    .map(|(resource, pixels)| {
                        let stored = Vec2::new(
                            style.num("thinodaImageWidth").unwrap_or(0.0),
                            style.num("thinodaImageHeight").unwrap_or(0.0),
                        );
                        let size = if images::valid_size(stored) {
                            stored
                        } else {
                            image_card_rect(rect, pixels, &label, true).1
                        };
                        CardImage {
                            resource,
                            size,
                            plain: false,
                        }
                    });
                return self.card(index, card_kind, rect, label, context, image);
            }
        };
        if !object.label().is_empty() && style.label_style_loss(false) {
            self.warn(ImportWarning::Formatting);
        }
        self.add_object(index, object, context)
            .map(BoardMember::Object)
    }

    fn card(
        &mut self,
        index: usize,
        kind: CardKind,
        rect: Rect,
        label: String,
        context: Context,
        image: Option<CardImage>,
    ) -> Option<BoardMember> {
        let cell = &self.cells[index];
        let style = &cell.style;
        let mut card = Card::new(
            kind,
            Vec2::new(
                rect.size.x.max(MIN_CARD_SIZE),
                rect.size.y.max(MIN_CARD_SIZE),
            ),
        );
        card.rotation = normalize_rotation(style.rotation());
        card.color = style.color("fillColor", "fillOpacity");
        if style.get("strokeColor").is_some()
            || kind != CardKind::Image && card.color.is_some_and(|color| color.a == 0)
        {
            self.warn(ImportWarning::Formatting);
        }
        // draw.io image cells have no background, so keep the card see-through.
        if image.as_ref().is_some_and(|image| image.plain) {
            card.color = Some(card.color.unwrap_or(Color::TRANSPARENT));
        }
        if cell.tooltip.is_some() {
            self.warn(ImportWarning::Other);
        }
        match self.sheet.add_card(&label, rect.center(), card) {
            Ok(id) => {
                if let Some(topic) = self.sheet.topic_mut(id) {
                    topic.style.fill = card.color;
                    topic.style.text_color = style.text_color();
                    if matches!(kind, CardKind::Sticky | CardKind::Text) {
                        topic.style.align = style.text_align();
                    }
                    topic.hyperlink = cell.link.clone();
                }
                if let Some(image) = image
                    && let Err(error) =
                        images::set_image(&mut self.sheet, id, Some((image.resource, image.size)))
                {
                    log::warn!("draw.io image {} skipped: {error}", cell.id);
                    self.warn(ImportWarning::Resources);
                }
                self.register(index, Item::Card(id), context, style.flag("locked"));
                Some(BoardMember::Card(id))
            }
            Err(error) => {
                log::warn!("draw.io card {} skipped: {error}", cell.id);
                self.warn(ImportWarning::Other);
                None
            }
        }
    }

    fn end(&self, terminal: Option<&str>, point: Option<Vec2>, last_pass: bool) -> End {
        if let Some(&index) = terminal.and_then(|id| self.index.get(id)) {
            match self.items.get(&index) {
                Some(Item::Object(id)) => {
                    return End::Ready {
                        endpoint: Endpoint::Object(*id),
                        detached: false,
                    };
                }
                Some(Item::Card(id)) => {
                    return End::Ready {
                        endpoint: Endpoint::Topic(*id),
                        detached: false,
                    };
                }
                Some(Item::Area(rect)) => {
                    let center = rect.center();
                    return match valid_board_position(center).then_some(center).or(point) {
                        Some(point) => End::Ready {
                            endpoint: Endpoint::Point(point),
                            detached: true,
                        },
                        None => End::Missing,
                    };
                }
                None if !last_pass && !self.visited[index] => return End::Pending,
                None => {}
            }
        }
        match point {
            Some(p) => End::Ready {
                endpoint: Endpoint::Point(p),
                detached: terminal.is_some(),
            },
            None => End::Missing,
        }
    }

    fn connector_port(
        &mut self,
        style: &Style,
        source: bool,
        endpoint: Endpoint,
        terminal: Option<&str>,
    ) -> Option<ConnectorPort> {
        let supported_target = match endpoint {
            Endpoint::Topic(_) => true,
            Endpoint::Object(id) => self.shapes.contains_key(&id),
            Endpoint::Point(_) => false,
        };
        let target_cell = terminal
            .and_then(|id| self.index.get(id))
            .and_then(|index| self.cells.get(*index));
        let waypoint = matches!(endpoint, Endpoint::Object(_))
            && target_cell.is_some_and(|cell| classify(&cell.style) == Kind::Waypoint);
        let waypoint_center = waypoint
            && target_cell
                .is_some_and(|cell| cell.style.get("perimeter") == Some("centerPerimeter"));
        let center_port = ConnectorPort {
            position: Vec2::new(0.5, 0.5),
            offset: Vec2::ZERO,
            perimeter: false,
        };
        if target_cell.is_some_and(|cell| {
            is_standard_rounded_rectangle(&cell.style)
                && cell.style.num("roundedPerimeter") == Some(1.0)
        }) && (style
            .get("edgeStyle")
            .is_some_and(|value| !matches!(value, "" | "none"))
            || style.flag("curved"))
        {
            self.warn(ImportWarning::Formatting);
        }
        match style.connector_port(source) {
            Ok(None) if waypoint_center => Some(center_port),
            Ok(None) => {
                if matches!(endpoint, Endpoint::Object(id)
                    if matches!(
                        self.shapes.get(&id),
                        Some(ShapeKind::DoubleEllipse | ShapeKind::FinalState)
                    )
                        || target_cell.is_some_and(|cell| is_standard_rounded_rectangle(&cell.style)))
                    && !target_cell.is_some_and(|cell| {
                        self.connector_geometry_matches_source(cell, endpoint, true)
                    })
                {
                    self.warn(ImportWarning::Formatting);
                }
                None
            }
            Ok(Some(port)) if supported_target && waypoint_center && port.perimeter => {
                Some(center_port)
            }
            Ok(Some(port)) if supported_target && waypoint && !port.perimeter => {
                if let Some(mapped) =
                    target_cell.and_then(|cell| waypoint_absolute_port(cell, port))
                {
                    Some(mapped)
                } else {
                    self.warn(ImportWarning::Formatting);
                    Some(port)
                }
            }
            Ok(Some(port)) if supported_target => {
                let compatible = target_cell.is_some_and(|cell| {
                    self.connector_geometry_matches_source(cell, endpoint, port.perimeter)
                });
                if !compatible {
                    self.warn(ImportWarning::Formatting);
                }
                Some(port)
            }
            Ok(Some(_)) | Err(()) => {
                self.warn(ImportWarning::Formatting);
                None
            }
        }
    }

    fn connector_geometry_matches_source(
        &self,
        cell: &Cell,
        endpoint: Endpoint,
        perimeter: bool,
    ) -> bool {
        let style = &cell.style;
        if style
            .get("direction")
            .is_some_and(|value| !matches!(value, "" | "east"))
            || ["flipH", "flipV", "stencilFlipH", "stencilFlipV"]
                .iter()
                .any(|key| {
                    style
                        .get(key)
                        .is_some_and(|value| !matches!(value, "" | "0" | "none"))
                })
            || style.tokens.iter().any(|token| {
                !matches!(
                    token.as_str(),
                    "defaultVertex"
                        | "ellipse"
                        | "rhombus"
                        | "triangle"
                        | "text"
                        | "label"
                        | "edgeLabel"
                        | "swimlane"
                )
            })
        {
            return false;
        }
        match classify(style) {
            Kind::Shape(_, base, symmetric) | Kind::Approx(_, base, symmetric)
                if !symmetric && base.rem_euclid(360.0) != 0.0 =>
            {
                return false;
            }
            Kind::Waypoint | Kind::Image => return false,
            Kind::Card(_)
                if cell.geometry.width < MIN_CARD_SIZE || cell.geometry.height < MIN_CARD_SIZE =>
            {
                return false;
            }
            _ => {}
        }
        if !perimeter {
            return true;
        }
        if cell.geometry.width <= 0.0 || cell.geometry.height <= 0.0 {
            return false;
        }
        let expected = match endpoint {
            Endpoint::Topic(_) => ConstraintPerimeter::Rectangle,
            Endpoint::Object(id) => match self.shapes.get(&id) {
                Some(
                    ShapeKind::Rectangle
                    | ShapeKind::RoundedRectangle
                    | ShapeKind::Process
                    | ShapeKind::PredefinedProcess,
                ) => ConstraintPerimeter::Rectangle,
                Some(ShapeKind::Ellipse | ShapeKind::DoubleEllipse | ShapeKind::FinalState) => {
                    ConstraintPerimeter::Ellipse
                }
                Some(ShapeKind::Diamond) => ConstraintPerimeter::Rhombus,
                _ => return false,
            },
            Endpoint::Point(_) => return false,
        };
        style.constraint_perimeter == expected
    }

    fn edge(&mut self, index: usize, context: Context, last_pass: bool) {
        let cells = self.cells;
        let cell = &cells[index];
        let style = &cell.style;
        let origin = context.origin;
        if let Some(tool) = style.get("thinodaInk") {
            if style.formatting_loss(true, false)
                || style.start_arrow()
                || style.end_arrow()
                || !matches!(style.connector_port(true), Ok(None))
                || !matches!(style.connector_port(false), Ok(None))
            {
                self.warn(ImportWarning::Formatting);
            }
            let g = &cell.geometry;
            let points = self.translated_points(
                origin,
                g.source.iter().chain(&g.points).chain(&g.target).copied(),
            );
            if points.is_empty() {
                self.warn(ImportWarning::Other);
                return;
            }
            let tool = if tool == "highlighter" {
                InkTool::Highlighter
            } else {
                InkTool::Pen
            };
            let color = style
                .color("strokeColor", "strokeOpacity")
                .unwrap_or(Color::BLACK);
            let mut object =
                CanvasObject::ink(InkStroke::new(tool, color, style.stroke_width(), points));
            object.band = style.band();
            object.style.opacity = style.opacity();
            self.add_object(index, object, context);
            return;
        }
        let source = cell
            .geometry
            .source
            .and_then(|point| self.translated_point(origin, point));
        let target = cell
            .geometry
            .target
            .and_then(|point| self.translated_point(origin, point));
        let from = self.end(cell.source.as_deref(), source, last_pass);
        let to = self.end(cell.target.as_deref(), target, last_pass);
        let (from, to, detached) = match (from, to) {
            (
                End::Ready {
                    endpoint: from,
                    detached: from_detached,
                },
                End::Ready {
                    endpoint: to,
                    detached: to_detached,
                },
            ) => (from, to, from_detached || to_detached),
            (End::Pending, _) | (_, End::Pending) => {
                self.deferred.push((index, context));
                return;
            }
            _ => {
                self.warn(ImportWarning::Other);
                return;
            }
        };
        let routed_hints = !cell.geometry.points.is_empty()
            && style
                .get("edgeStyle")
                .is_some_and(|value| !value.is_empty() && value != "none");
        if style.formatting_loss(true, false) || routed_hints {
            self.warn(ImportWarning::Formatting);
        }
        if cell.link.is_some() {
            self.warn(ImportWarning::Links);
        }
        let mut label = self.label(cell);
        let label_style_loss = !label.is_empty() && style.label_style_loss(true);
        for child in self.children[index].clone() {
            let child = &cells[child];
            if child.vertex && child.visible {
                let text = self.label(child);
                if !text.is_empty() {
                    // Folding child labels loses their independent positions and styles.
                    self.warn(ImportWarning::Formatting);
                    if !label.is_empty() {
                        label.push('\n');
                    }
                    label.push_str(&text);
                }
            }
        }
        if !label.is_empty()
            && (label_style_loss
                || style.label_opacity_loss()
                || style.connector_label_appearance_loss(true))
        {
            self.warn(ImportWarning::Formatting);
        }
        let mut connector = CanvasConnector::new(from, to);
        connector.from_port = self.connector_port(style, true, from, cell.source.as_deref());
        connector.to_port = self.connector_port(style, false, to, cell.target.as_deref());
        connector.route = if style.flag("curved") {
            ConnectorRoute::Curve
        } else if style
            .get("edgeStyle")
            .is_some_and(|v| !v.is_empty() && v != "none")
        {
            ConnectorRoute::Elbow
        } else {
            ConnectorRoute::Straight
        };
        connector.waypoints = self
            .translated_points(origin, cell.geometry.points.iter().copied())
            .into();
        connector.arrow_end = style.end_arrow();
        connector.arrow_start = style.start_arrow();
        connector.start_marker = style.marker("startArrow");
        connector.end_marker = style.marker("endArrow");
        connector.start_marker_size = style.marker_size("startArrow", "startSize");
        connector.end_marker_size = style.marker_size("endArrow", "endSize");
        connector.start_marker_unfilled = style.marker_unfilled("startArrow", "startFill");
        connector.end_marker_unfilled = style.marker_unfilled("endArrow", "endFill");
        connector.label = truncate_chars(label, MAX_BOARD_TEXT_CHARS, self.warnings);
        connector.color = style.color("strokeColor", "strokeOpacity");
        connector.width = style.stroke_width();
        let mut object = CanvasObject::connector(connector);
        object.style.text_color = style.text_color();
        object.style.opacity = style.opacity();
        object.band = style.band();
        object.alt_text = cell.tooltip.clone().unwrap_or_default();
        if self.add_object(index, object, context).is_some() && detached {
            self.warn(ImportWarning::DetachedConnectors);
        }
    }

    fn finish(mut self) -> Sheet {
        for (zone, roots, objects) in std::mem::take(&mut self.zones) {
            if (!roots.is_empty() || !objects.is_empty())
                && let Err(error) = self.sheet.set_zone_members(zone, roots, objects)
            {
                log::warn!("draw.io container membership skipped: {error}");
                self.warn(ImportWarning::Other);
            }
        }
        for (members, locked) in std::mem::take(&mut self.buckets) {
            if members.len() < 2 {
                continue;
            }
            match self.sheet.group_board_members(&members) {
                Ok(group) => {
                    if locked
                        && let Some(group) =
                            self.sheet.canvas.groups.iter_mut().find(|g| g.id == group)
                    {
                        group.locked = true;
                    }
                }
                Err(error) => {
                    log::warn!("draw.io group skipped: {error}");
                    self.warn(ImportWarning::Other);
                }
            }
        }
        for member in std::mem::take(&mut self.locked) {
            if self
                .sheet
                .set_board_members_locked(&[member], true)
                .is_err()
            {
                self.warn(ImportWarning::Other);
            }
        }
        // Deferred edges must retain their place in the source cell traversal.
        let order: HashMap<_, _> = self
            .paint_order
            .iter()
            .enumerate()
            .filter_map(|(rank, cell)| match self.items.get(cell) {
                Some(Item::Object(id)) => Some((*id, rank)),
                _ => None,
            })
            .collect();
        self.sheet
            .canvas
            .objects
            .sort_by_key(|object| order.get(&object.id).copied().unwrap_or(usize::MAX));
        self.sheet
    }
}

/// Longest image source kept as alt text on a placeholder.
const MAX_IMAGE_ALT_CHARS: usize = 500;
/// Longest side, in pixels, of a rasterised inline SVG image.
const MAX_SVG_RASTER: f32 = 1024.0;

/// An embedded image for an imported image card.
struct CardImage {
    resource: Resource,
    size: Vec2,
    /// A native draw.io image cell (transparent card), not a Thinoda card.
    plain: bool,
}

/// Decodes a draw.io inline image (`data:<type>,<base64>`) into a PNG/JPEG resource and
/// its pixel size. SVG is rasterised; other types and every non-`data:` source (remote
/// URLs, draw.io library paths) are rejected, so nothing is ever fetched.
fn inline_image(source: &str, display: Vec2) -> Option<(Resource, Vec2)> {
    let (media, payload) = source.strip_prefix("data:")?.split_once(',')?;
    let media = media.strip_suffix(";base64").unwrap_or(media);
    if payload.len() > MAX_RESOURCE_BYTES / 3 * 4 + 4 {
        return None;
    }
    let base64 = || {
        base64::engine::general_purpose::STANDARD
            .decode(payload.trim())
            .ok()
    };
    match media.to_ascii_lowercase().as_str() {
        "image/png" | "image/jpeg" | "image/jpg" => {
            let bytes = base64()?;
            let decoded = images::decode(&bytes).ok()?;
            let name = match decoded.media_type {
                "image/png" => "draw.io image.png",
                _ => "draw.io image.jpg",
            };
            let pixels = Vec2::new(decoded.width as f32, decoded.height as f32);
            Some((Resource::new(name, decoded.media_type, bytes).ok()?, pixels))
        }
        "image/svg+xml" => {
            let svg = base64().or_else(|| percent_decode(payload).map(String::into_bytes))?;
            let (png, pixels) = rasterize_svg(&svg, display)?;
            Some((
                Resource::new("draw.io image.png", "image/png", png).ok()?,
                pixels,
            ))
        }
        _ => None,
    }
}

/// Renders untrusted SVG to PNG at about twice its display size. External and file
/// references are refused; compressed SVGZ is rejected to rule out inflation bombs.
fn rasterize_svg(svg: &[u8], display: Vec2) -> Option<(Vec<u8>, Vec2)> {
    use image::ImageEncoder;
    use resvg::{tiny_skia, usvg};
    if svg.starts_with(&[0x1f, 0x8b]) {
        return None;
    }
    let mut options = usvg::Options {
        fontdb: crate::fonts::database(),
        ..Default::default()
    };
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = usvg::Tree::from_data(svg, &options).ok()?;
    let size = tree.size();
    let longest = size.width().max(size.height());
    let target = (display.x.max(display.y) * 2.0).clamp(64.0, MAX_SVG_RASTER);
    if !longest.is_finite() || longest <= 0.0 || !target.is_finite() {
        return None;
    }
    let scale = target / longest;
    let width = (size.width() * scale).round().clamp(1.0, MAX_SVG_RASTER) as u32;
    let height = (size.height() * scale).round().clamp(1.0, MAX_SVG_RASTER) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(
            width as f32 / size.width(),
            height as f32 / size.height(),
        ),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied alpha; PNG expects straight alpha.
    let mut rgba = pixmap.take();
    crate::render::unpremultiply_rgba(&mut rgba);
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some((png, Vec2::new(width as f32, height as f32)))
}

/// Image card geometry for a draw.io image cell: the image keeps the cell size (fitted to
/// its aspect unless `imageAspect=0`) and the card adds room for the caption beneath.
fn image_card_rect(rect: Rect, pixels: Vec2, label: &str, keep_aspect: bool) -> (Rect, Vec2) {
    let mut size = if keep_aspect && pixels.x > 0.0 && pixels.y > 0.0 {
        pixels * (rect.size.x / pixels.x).min(rect.size.y / pixels.y)
    } else {
        rect.size
    };
    let longest = size.x.max(size.y);
    if longest > MAX_DISPLAY_SIZE {
        size = size * (MAX_DISPLAY_SIZE / longest);
    }
    let size = Vec2::new(size.x.max(1.0), size.y.max(1.0));
    let lines = label.lines().count().max(1) as f32;
    let card = Vec2::new(
        (size.x + 28.0).max(MIN_CARD_SIZE),
        size.y + 6.0 + lines * 27.0 + 34.0,
    );
    // Card content is centred vertically; align the image with the original cell.
    let content = size.y + 6.0 + lines * 18.0;
    let top = rect.center().y - size.y / 2.0 - (card.y - content) / 2.0;
    let min = Vec2::new(rect.center().x - card.x / 2.0, top);
    (Rect::from_min_size(min, card), size)
}

/// draw.io reads grid sizes with JavaScript's `parseFloat`, which accepts a
/// numeric prefix even when non-numeric characters follow it.
fn grid_number(value: &str) -> Option<(f64, bool)> {
    // ECMAScript whitespace includes BOM but excludes Unicode's next-line character.
    let js_whitespace = |c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}';
    let value = value.trim_start_matches(js_whitespace);
    let bytes = value.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mut digits = 0;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
        digits += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent_end = end + 1;
        if matches!(bytes.get(exponent_end), Some(b'+' | b'-')) {
            exponent_end += 1;
        }
        let exponent_start = exponent_end;
        while bytes.get(exponent_end).is_some_and(u8::is_ascii_digit) {
            exponent_end += 1;
        }
        if exponent_end > exponent_start {
            end = exponent_end;
        }
    }
    let number = value[..end].parse::<f64>().ok()?;
    let trailing = !value[end..].trim_matches(js_whitespace).is_empty();
    Some((number, trailing))
}

fn warn_grid_adjustment(warnings: &mut BTreeSet<ImportWarning>) {
    warnings.insert(ImportWarning::DrawioGridAdjusted);
}

fn import_grid(model: &Element, sheet: &mut Sheet, warnings: &mut BTreeSet<ImportWarning>) {
    const DEFAULT_DRAWIO_GRID_SIZE: f32 = 10.0;
    let enabled = model.attr("grid") != Some("0");
    let mut grid = if enabled {
        GridKind::Lines
    } else {
        GridKind::None
    };
    let mut snap = enabled;
    match (model.attr("thinodaGrid"), model.attr("thinodaSnap")) {
        (None, None) => {}
        (Some(grid_value), Some(snap_value)) => {
            let saved_grid = match grid_value {
                "none" => Some(GridKind::None),
                "dots" => Some(GridKind::Dots),
                "lines" => Some(GridKind::Lines),
                _ => None,
            };
            let saved_snap = match snap_value {
                "0" => Some(false),
                "1" => Some(true),
                _ => None,
            };
            if let (Some(saved_grid), Some(saved_snap)) = (saved_grid, saved_snap)
                && (saved_grid != GridKind::None) == enabled
            {
                grid = saved_grid;
                snap = saved_snap;
            } else {
                warn_grid_adjustment(warnings);
            }
        }
        _ => {
            warn_grid_adjustment(warnings);
        }
    }
    let size = match model.attr("gridSize") {
        None => DEFAULT_DRAWIO_GRID_SIZE,
        Some(value) => match grid_number(value) {
            Some((size, trailing)) if size.is_finite() && size > 0.0 => {
                let min = f64::from(MIN_GRID_SIZE);
                let max = f64::from(MAX_GRID_SIZE);
                if trailing || !(min..=max).contains(&size) {
                    warn_grid_adjustment(warnings);
                }
                size.clamp(min, max) as f32
            }
            _ => {
                warn_grid_adjustment(warnings);
                DEFAULT_DRAWIO_GRID_SIZE
            }
        },
    };
    sheet.board.grid = grid;
    sheet.board.grid_size = size;
    sheet.board.snap = snap;
}

fn drawio_background_atom(value: &str, in_pair: bool) -> bool {
    if value.len() > 200 {
        return false;
    }
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|c| c.is_ascii_hexdigit());
    }
    if value.bytes().all(|c| c.is_ascii_alphabetic()) {
        return (3..=30).contains(&value.len())
            && !["none", "inherit", "initial", "unset", "revert", "default"]
                .iter()
                .any(|excluded| value.eq_ignore_ascii_case(excluded));
    }
    let Some((name, arguments)) = value.split_once('(') else {
        return false;
    };
    let Some(arguments) = arguments.strip_suffix(')') else {
        return false;
    };
    let lower_name = name.to_ascii_lowercase();
    if !matches!(lower_name.as_str(), "rgb" | "rgba" | "hsl" | "hsla")
        || (in_pair && (name != lower_name || !matches!(name, "rgb" | "rgba")))
    {
        return false;
    }
    let arguments = arguments.to_ascii_lowercase().replacen("deg", "", 1);
    arguments.chars().all(|c| {
        c.is_ascii_digit()
            || matches!(c, '.' | ',' | '%' | '/' | '+' | '-')
            || ((c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
    })
}

fn drawio_fixed_background_pair(value: &str) -> bool {
    if value.len() > 200 {
        return false;
    }
    let value = value.trim();
    // The editor's adaptive-color parser accepts fewer forms than CSS itself.
    let Some(inner) = value
        .strip_prefix("light-dark(")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return false;
    };
    let mut depth = 0usize;
    for (index, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            ',' if depth == 0 => {
                let (light, rest) = inner.split_at(index);
                let dark = &rest[1..];
                if !drawio_background_atom(light, true) || !drawio_background_atom(dark, true) {
                    return false;
                }
                let Some(light) = parse_style_color(light) else {
                    return false;
                };
                let Some(dark) = parse_style_color(dark) else {
                    return false;
                };
                return !light.approximate
                    && !dark.approximate
                    && light.color.is_some()
                    && light.color == dark.color;
            }
            _ => {}
        }
    }
    false
}

fn import_background(model: &Element, sheet: &mut Sheet, warnings: &mut BTreeSet<ImportWarning>) {
    if let Some(value) = model.attr("background")
        && !matches!(value, "" | "none")
    {
        match parse_style_color(value) {
            Some(ParsedColor {
                color: Some(color),
                approximate,
            }) => {
                sheet.theme.background = Some(color);
                let fixed_pair = drawio_fixed_background_pair(value);
                let fixed_atom = !approximate
                    && drawio_background_atom(value, false)
                    && matches!(model.attr("adaptiveColors"), Some("simple" | "none"));
                if !(fixed_pair || fixed_atom) || color.a < 255 || model.attr("page") != Some("0") {
                    warnings.insert(ImportWarning::DrawioBackgroundAdjusted);
                }
            }
            _ => {
                warnings.insert(ImportWarning::DrawioBackgroundAdjusted);
            }
        }
    }
    if model
        .attr("backgroundGradient")
        .is_some_and(|value| !matches!(value, "" | "none"))
        || model
            .attr("backgroundImage")
            .is_some_and(|value| !value.is_empty())
    {
        warnings.insert(ImportWarning::DrawioBackgroundAdjusted);
    }
}

fn import_page(
    page: &Page,
    number: usize,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Result<Sheet, IoError> {
    let title = if page.name.trim().is_empty() {
        format!("Page-{number}")
    } else {
        page.name.clone()
    };
    let mut sheet = Sheet::new(title, "");
    sheet.root_mode = SheetRootMode::Hidden;
    let model = &page.model;
    import_grid(model, &mut sheet, warnings);
    import_background(model, &mut sheet, warnings);
    let cells = read_cells(model, warnings);
    let index: HashMap<&str, usize> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let mut children = vec![Vec::new(); cells.len()];
    for (i, cell) in cells.iter().enumerate() {
        if let Some(&parent) = cell.parent.as_deref().and_then(|p| index.get(p))
            && parent != i
        {
            children[parent].push(i);
        }
    }
    let mut import = PageImport {
        cells: &cells,
        index,
        children,
        sheet,
        warnings,
        items: HashMap::new(),
        shapes: HashMap::new(),
        visited: vec![false; cells.len()],
        paint_order: Vec::with_capacity(cells.len()),
        zones: Vec::new(),
        buckets: Vec::new(),
        locked: Vec::new(),
        deferred: Vec::new(),
        adaptive_fixed_colors: !matches!(model.attr("adaptiveColors"), Some("simple" | "none")),
    };
    import.run();
    if let Some(extension) = model.attr("thinodaSankey") {
        import.apply_sankey(extension)?;
    }
    Ok(import.finish())
}

/// Imports every page of a draw.io document as a Freeform board sheet.
pub(crate) fn import(bytes: &[u8]) -> Result<ImportReport, IoError> {
    import_with_first_page_warnings(bytes).map(|(report, _)| report)
}

/// Imports all pages, retaining the first page's warnings separately for clipboard paste.
pub(crate) fn import_with_first_page_warnings(
    bytes: &[u8],
) -> Result<(ImportReport, Vec<ImportWarning>), IoError> {
    let pages = read_pages(bytes)?;
    let mut warnings = BTreeSet::new();
    let (first, rest) = pages
        .split_first()
        .ok_or_else(|| invalid("draw.io document contains no pages"))?;
    let mut workbook = Workbook::new(import_page(first, 1, &mut warnings)?);
    let first_page_warnings = warnings.iter().copied().collect();
    for (i, page) in rest.iter().enumerate() {
        workbook
            .sheets
            .push(import_page(page, i + 2, &mut warnings)?);
    }
    Ok((
        ImportReport {
            format: Format::DrawIo,
            workbook,
            warnings: warnings.into_iter().collect(),
        },
        first_page_warnings,
    ))
}

// ---------------------------------------------------------------------------------------
// Export

/// Versioned semantic extension, separate from ordinary draw.io stroke widths.
/// Cell IDs, not generated geometry, are used to reattach the native graph.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SankeyExtension {
    version: u32,
    settings: thinoda_core::sankey::SankeySettings,
    nodes: std::collections::BTreeMap<TopicId, Option<thinoda_core::notes::NoteContent>>,
    links: std::collections::BTreeMap<CanvasObjectId, SankeyLinkExtension>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SankeyLinkExtension {
    flow: thinoda_core::sankey::Flow,
    /// Invisible zero flows must not stay hidden after their weight is edited.
    opacity: f32,
    from: TopicId,
    to: TopicId,
    /// Baseline of the visual approximation: detect edits made in other tools.
    width: f32,
    label: String,
    display_label: String,
}

impl SankeyExtension {
    fn from_sheet(sheet: &Sheet) -> Result<Self, IoError> {
        let values = thinoda_core::sankey::evaluate(sheet).map_err(IoError::Invalid)?;
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        let exported = exported_topics(sheet, &layout);
        if values.nodes.iter().any(|id| !exported.contains(id)) {
            return Err(invalid(
                "Expand all Sankey endpoint topics before exporting to draw.io.",
            ));
        }
        Ok(Self {
            version: 2,
            settings: sheet.sankey.clone().unwrap_or_default(),
            nodes: sheet
                .topics
                .iter()
                .filter(|(id, _)| values.nodes.contains(id))
                .map(|(id, topic)| (*id, topic.notes.clone()))
                .collect(),
            links: sheet
                .canvas
                .objects
                .iter()
                .filter_map(|o| match &o.kind {
                    CanvasObjectKind::Connector(c) => c.sankey.map(|flow| {
                        (
                            o.id,
                            SankeyLinkExtension {
                                flow,
                                opacity: o.style.opacity,
                                from: match c.from {
                                    Endpoint::Topic(id) => id,
                                    _ => sheet.root,
                                },
                                to: match c.to {
                                    Endpoint::Topic(id) => id,
                                    _ => sheet.root,
                                },
                                width: exported_flow_width(sheet, &layout, &values, o.id, c),
                                label: c.label.clone(),
                                display_label: thinoda_core::sankey::flow_label(
                                    c,
                                    values.mass.get(&o.id).copied().unwrap_or(0.0),
                                ),
                            },
                        )
                    }),
                    _ => None,
                })
                .collect(),
        })
    }
}

impl PageImport<'_> {
    fn apply_sankey(&mut self, json: &str) -> Result<(), IoError> {
        if json.len() > MAX_BYTES {
            return Err(invalid("Sankey extension exceeds import size limit"));
        }
        let sheet = self.sheet.clone();
        let items = self.items.clone();
        if self.apply_sankey_candidate(json).is_err() {
            self.sheet = sheet;
            self.items = items;
            self.warn(ImportWarning::DrawioSankeyStale);
        }
        Ok(())
    }

    fn apply_sankey_candidate(&mut self, json: &str) -> Result<(), IoError> {
        let extension: SankeyExtension = serde_json::from_str(json)?;
        if extension.version != 2 {
            return Err(IoError::NewerVersion(format!(
                "draw.io Sankey extension {}",
                extension.version
            )));
        }
        let mut topics = HashMap::new();
        let mut objects = HashMap::new();
        for (old, notes) in extension.nodes {
            let key = format!("t{}", old.0);
            let index = *self.index.get(key.as_str()).ok_or_else(|| {
                invalid("Sankey node cell was removed; native extension is stale")
            })?;
            if self.cells[index].style.rotation() != 0.0 {
                return Err(invalid(
                    "Sankey node was rotated; retain ordinary rotated cells",
                ));
            }
            let topic = match self.items.get(&index) {
                Some(Item::Card(id)) => *id,
                Some(Item::Object(id)) => {
                    let id = *id;
                    let o = self
                        .sheet
                        .canvas
                        .object(id)
                        .ok_or_else(|| invalid("Missing Sankey node"))?
                        .clone();
                    let CanvasObjectKind::Shape { label, .. } = &o.kind else {
                        return Err(invalid("Sankey node must be a topic-compatible vertex"));
                    };
                    let topic = self
                        .sheet
                        .add_card(
                            label,
                            o.rect.center(),
                            Card::new(CardKind::Text, o.rect.size),
                        )
                        .map_err(IoError::Invalid)?;
                    self.sheet.canvas.objects.retain(|o| o.id != id);
                    self.items.insert(index, Item::Card(topic));
                    objects.insert(id, topic);
                    topic
                }
                _ => return Err(invalid("Sankey node is not an editable vertex")),
            };
            if let Some(t) = self.sheet.topics.get_mut(&topic) {
                t.notes = notes;
            }
            topics.insert(old, topic);
        }
        for o in &mut self.sheet.canvas.objects {
            if let CanvasObjectKind::Connector(c) = &mut o.kind {
                for end in [&mut c.from, &mut c.to] {
                    if let Endpoint::Object(id) = end
                        && let Some(topic) = objects.get(id)
                    {
                        *end = Endpoint::Topic(*topic);
                    }
                }
            }
        }
        for (old, link) in extension.links {
            let key = format!("o{}", old.0);
            let index = *self.index.get(key.as_str()).ok_or_else(|| {
                invalid("Sankey link cell was removed; native extension is stale")
            })?;
            let Some(Item::Object(id)) = self.items.get(&index) else {
                return Err(invalid("Sankey link cell is not an edge"));
            };
            let Some(o) = self.sheet.canvas.object_mut(*id) else {
                return Err(invalid("Missing Sankey edge"));
            };
            let CanvasObjectKind::Connector(c) = &mut o.kind else {
                return Err(invalid("Sankey link must remain an edge"));
            };
            let from = topics
                .get(&link.from)
                .ok_or_else(|| invalid("Missing Sankey source"))?;
            let to = topics
                .get(&link.to)
                .ok_or_else(|| invalid("Missing Sankey target"))?;
            let cell = &self.cells[index];
            let expected: f32 = num(link.width)
                .parse()
                .map_err(|_| invalid("Invalid Sankey stroke baseline"))?;
            if c.from != Endpoint::Topic(*from)
                || c.to != Endpoint::Topic(*to)
                || !link.width.is_finite()
                || cell
                    .style
                    .num("strokeWidth")
                    .is_none_or(|width| (width - expected).abs() > 0.00001)
                || c.label != link.display_label
                || !cell.edge
            {
                return Err(invalid("Sankey edge was reconnected or visually edited"));
            }
            c.sankey = Some(link.flow);
            c.label = link.label;
            o.style.opacity = link.opacity;
        }
        let remap = |map: std::collections::BTreeMap<TopicId,u32>| -> Result<std::collections::BTreeMap<TopicId,u32>,IoError> {
            map.into_iter().map(|(id,value)|
                topics.get(&id).copied().map(|new| (new,value)).ok_or_else(|| invalid("Missing Sankey settings node"))
            ).collect()
        };
        let mut settings = extension.settings;
        settings.sources = remap(settings.sources)?;
        settings.stages = remap(settings.stages)?;
        self.sheet.sankey = Some(settings);
        self.sheet.validate().map_err(IoError::Invalid)?;
        Ok(())
    }
}

fn exported_flow_width(
    sheet: &Sheet,
    layout: &thinoda_core::Layout,
    values: &thinoda_core::sankey::FlowValues,
    id: CanvasObjectId,
    c: &CanvasConnector,
) -> f32 {
    if c.sankey.is_some_and(|f| f.weight.is_some()) {
        (values.mass.get(&id).copied().unwrap_or(0.0) as f32
            * thinoda_core::sankey::effective_scale(sheet, layout, values))
        .min(MAX_STROKE_WIDTH)
    } else {
        c.width
    }
}

/// XML attribute text; characters invalid in XML 1.0 are dropped.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c if c < ' ' || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

/// HTML label (`html=1`) text.
fn html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out
}

fn num(value: f32) -> String {
    if value == 0.0 || !value.is_finite() {
        "0".into()
    } else {
        value.to_string()
    }
}

fn color_keys(out: &mut String, key: &str, opacity_key: &str, color: Option<Color>) {
    let Some(color) = color else {
        return;
    };
    // Removing a font color restores draw.io's default instead of hiding text.
    if color == Color::TRANSPARENT && key != "fontColor" {
        out.push_str(&format!("{key}=none;"));
        return;
    }
    out.push_str(&format!(
        "{key}=#{:02X}{:02X}{:02X};",
        color.r, color.g, color.b
    ));
    if color.a < 255 {
        out.push_str(&format!(
            "{opacity_key}={};",
            num(f32::from(color.a) / 255.0 * 100.0)
        ));
    }
}

fn shape_style(shape: ShapeKind) -> &'static str {
    use ShapeKind as S;
    match shape {
        S::Rectangle => "rounded=0;",
        S::RoundedRectangle => "rounded=1;thinodaShape=nativeRoundedRectangle;",
        S::Ellipse => "ellipse;",
        S::DoubleEllipse => "shape=doubleEllipse;perimeter=ellipsePerimeter;",
        S::FinalState => "shape=endState;perimeter=ellipsePerimeter;",
        S::Diamond => "rhombus;",
        S::Triangle => "triangle;direction=north;",
        S::RightTriangle => "shape=mxgraph.basic.orthogonal_triangle;",
        S::Parallelogram => "shape=parallelogram;perimeter=parallelogramPerimeter;fixedSize=1;",
        S::Trapezoid => "shape=trapezoid;perimeter=trapezoidPerimeter;fixedSize=1;",
        S::Pentagon => "shape=mxgraph.basic.pentagon;",
        S::Hexagon => "shape=hexagon;perimeter=hexagonPerimeter2;fixedSize=1;",
        S::Octagon => "shape=mxgraph.basic.octagon2;dx=15;",
        S::Star => "shape=mxgraph.basic.star;",
        S::Plus => "shape=cross;",
        S::Arrow => "shape=singleArrow;",
        S::DoubleArrow => "shape=doubleArrow;",
        S::Chevron => "shape=step;perimeter=stepPerimeter;fixedSize=1;",
        S::Cloud => "ellipse;shape=cloud;",
        S::Callout => "shape=callout;",
        S::Cylinder => "shape=cylinder3;boundedLbl=1;backgroundOutline=1;size=15;",
        S::Document => "shape=document;boundedLbl=1;",
        S::Process => "shape=rectangle;rounded=0;thinodaShape=process;",
        S::PredefinedProcess => "shape=process;backgroundOutline=1;",
        S::Decision => "shape=mxgraph.flowchart.decision;",
        S::Terminator => "shape=mxgraph.flowchart.terminator;",
        S::Data => "shape=mxgraph.flowchart.data;",
        S::ManualInput => "shape=mxgraph.flowchart.manual_input;",
        S::Preparation => "shape=mxgraph.flowchart.preparation;",
        S::Delay => "shape=mxgraph.flowchart.delay;",
        S::OffPageConnector => "shape=offPageConnector;",
    }
}

fn rounded_stencil_name(stencil: DrawioRectangleStencil) -> &'static str {
    match stencil {
        DrawioRectangleStencil::Mockup => "mxgraph.mockup.rrect",
        DrawioRectangleStencil::Forms => "mxgraph.mockup.forms.rrect",
        DrawioRectangleStencil::Containers => "mxgraph.mockup.containers.rrect",
        DrawioRectangleStencil::Misc => "mxgraph.mockup.misc.rrect",
        DrawioRectangleStencil::Graphics => "mxgraph.mockup.graphics.rrect",
        DrawioRectangleStencil::Text => "mxgraph.mockup.text.rrect",
        DrawioRectangleStencil::Ios => "mxgraph.ios.rrect",
        DrawioRectangleStencil::Standard => "rectangle",
    }
}

fn topic_shape_style(shape: TopicShape) -> &'static str {
    match shape {
        TopicShape::RoundedRect => "rounded=1;",
        TopicShape::Rect => "rounded=0;",
        TopicShape::Ellipse => "ellipse;",
        TopicShape::Capsule => "rounded=1;arcSize=50;",
        TopicShape::Diamond => "rhombus;",
        TopicShape::Underline | TopicShape::None => "text;",
    }
}

pub(crate) fn fixed_port_position_approximate(
    sheet: &Sheet,
    object: Option<&CanvasObject>,
    layout: Option<&thinoda_core::Layout>,
    endpoint: Endpoint,
    port: ConnectorPort,
) -> bool {
    match endpoint {
        Endpoint::Object(_) => {
            let Some(object) = object else {
                return false;
            };
            let CanvasObjectKind::Shape { shape, .. } = object.kind else {
                return false;
            };
            if shape == ShapeKind::Triangle {
                return true;
            }
            port.perimeter
                && (object.rect.size.x <= 0.0
                    || object.rect.size.y <= 0.0
                    || !matches!(
                        shape,
                        ShapeKind::Rectangle
                            | ShapeKind::RoundedRectangle
                            | ShapeKind::Process
                            | ShapeKind::PredefinedProcess
                            | ShapeKind::Ellipse
                            | ShapeKind::DoubleEllipse
                            | ShapeKind::FinalState
                            | ShapeKind::Diamond
                    ))
        }
        Endpoint::Topic(id) if port.perimeter => {
            let Some(topic) = sheet.topic(id) else {
                return false;
            };
            let Some(node) = layout.and_then(|layout| layout.topics.get(&id)) else {
                return false;
            };
            node.rect.size.x <= 0.0
                || node.rect.size.y <= 0.0
                || topic.card.is_none()
                    && matches!(node.style.shape, TopicShape::Ellipse | TopicShape::Diamond)
        }
        Endpoint::Topic(_) | Endpoint::Point(_) => false,
    }
}

fn common_keys(out: &mut String, style: &ObjectStyle, rotation: f32, locked: bool, band: Band) {
    if style.opacity < 1.0 {
        out.push_str(&format!("opacity={};", num(style.opacity * 100.0)));
    }
    if rotation != 0.0 {
        out.push_str(&format!("rotation={};", num(rotation)));
    }
    if locked {
        out.push_str("locked=1;");
    }
    if band == Band::Below {
        out.push_str("thinodaBand=below;");
    }
}

enum Terminal {
    Cell(String),
    Point(Vec2),
}

pub(crate) fn exported_topics(sheet: &Sheet, layout: &thinoda_core::Layout) -> HashSet<TopicId> {
    layout
        .order
        .iter()
        .copied()
        .filter(|id| !sheet.is_anchor(*id))
        .collect()
}

pub(crate) fn card_image<'a>(
    sheet: &'a Sheet,
    topic: &thinoda_core::Topic,
) -> Option<&'a Resource> {
    topic
        .image
        .as_deref()
        .and_then(|reference| sheet.resources.get(reference).ok())
        .filter(|resource| matches!(resource.media_type.as_str(), "image/png" | "image/jpeg"))
}

struct PageWriter<'a> {
    sheet: &'a Sheet,
    flows: thinoda_core::sankey::FlowValues,
    out: String,
    layout: thinoda_core::Layout,
    topics: HashSet<TopicId>,
    object_zone: HashMap<CanvasObjectId, ZoneId>,
    root_zone: HashMap<TopicId, ZoneId>,
    connector_midpoints: HashMap<CanvasObjectId, Vec2>,
    detached_fixed_points: HashMap<CanvasObjectId, (Vec2, Vec2)>,
    groups: HashMap<CanvasGroupId, (String, Rect)>,
    written_groups: HashSet<CanvasGroupId>,
    written: HashSet<BoardMember>,
}

impl<'a> PageWriter<'a> {
    fn new(sheet: &'a Sheet) -> Result<Self, IoError> {
        let flows = thinoda_core::sankey::evaluate(sheet).map_err(IoError::Invalid)?;
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        let topics = exported_topics(sheet, &layout);
        let edge_targets = crate::board_export::connector_attachment_counts(sheet);
        let detached_fixed: HashSet<_> = sheet
            .canvas
            .objects
            .iter()
            .filter_map(|object| match &object.kind {
                CanvasObjectKind::Connector(connector)
                    if [
                        (connector.from, connector.from_port),
                        (connector.to, connector.to_port),
                    ]
                    .into_iter()
                    .any(|(end, port)| {
                        port.is_some()
                            && matches!(end, Endpoint::Topic(id) if !topics.contains(&id))
                    }) =>
                {
                    Some(object.id)
                }
                _ => None,
            })
            .collect();
        let mut connector_midpoints = HashMap::new();
        let mut detached_fixed_points = HashMap::new();
        if !edge_targets.is_empty() || !detached_fixed.is_empty() {
            for item in thinoda_core::build_scene(sheet, &layout).items {
                if let (
                    thinoda_core::SceneId::Object(id),
                    thinoda_core::scene::SceneGeometry::Polyline { points, .. },
                ) = (item.id, item.geometry)
                {
                    if edge_targets.contains_key(&id)
                        && let Some(point) = thinoda_core::board::polyline_midpoint(&points)
                    {
                        connector_midpoints.insert(id, point);
                    }
                    if detached_fixed.contains(&id)
                        && let (Some(first), Some(last)) = (points.first(), points.last())
                    {
                        detached_fixed_points.insert(id, (*first, *last));
                    }
                }
            }
        }
        let mut writer = Self {
            sheet,
            out: String::new(),
            layout,
            topics,
            object_zone: HashMap::new(),
            root_zone: HashMap::new(),
            connector_midpoints,
            flows,
            detached_fixed_points,
            groups: HashMap::new(),
            written_groups: HashSet::new(),
            written: HashSet::new(),
        };
        for zone in &sheet.zones {
            for object in &zone.objects {
                writer.object_zone.insert(*object, zone.id);
            }
            for root in &zone.roots {
                writer.root_zone.insert(*root, zone.id);
            }
        }
        for group in &sheet.canvas.groups {
            let members = sheet.board_group_members(group.id);
            let bounds = members
                .iter()
                .filter_map(|m| writer.member_bounds(*m))
                .reduce(|a, b| a.union(&b))
                .unwrap_or_default();
            let zones: HashSet<_> = members.iter().map(|m| writer.member_zone(*m)).collect();
            let parent = match zones.into_iter().collect::<Vec<_>>()[..] {
                [Some(zone)] => format!("z{}", zone.0),
                _ => "1".into(),
            };
            writer.groups.insert(group.id, (parent, bounds));
        }
        Ok(writer)
    }

    fn card_rect(&self, id: TopicId) -> Option<Rect> {
        let topic = self.sheet.topic(id)?;
        Some(Rect::from_center_size(topic.position?, topic.card?.size))
    }

    fn member_bounds(&self, member: BoardMember) -> Option<Rect> {
        match member {
            BoardMember::Object(id) => self
                .sheet
                .canvas
                .object(id)
                .map(CanvasObject::rotated_bounds),
            BoardMember::Card(id) => {
                let rotation = self.sheet.topic(id)?.card?.rotation;
                Some(thinoda_core::board::rotated_bounds(
                    self.card_rect(id)?,
                    rotation,
                ))
            }
        }
    }

    fn member_zone(&self, member: BoardMember) -> Option<ZoneId> {
        match member {
            BoardMember::Object(id) => self.object_zone.get(&id).copied(),
            BoardMember::Card(id) => self.root_zone.get(&id).copied(),
        }
    }

    fn member_group(&self, member: BoardMember) -> Option<CanvasGroupId> {
        match member {
            BoardMember::Object(id) => self.sheet.canvas.object(id)?.group,
            BoardMember::Card(id) => self.sheet.canvas.card_state(id)?.group,
        }
        .filter(|g| self.groups.contains_key(g))
    }

    fn zone_origin(&self, id: ZoneId) -> Vec2 {
        self.sheet
            .zones
            .iter()
            .find(|z| z.id == id)
            .map_or(Vec2::ZERO, |z| z.frame.min)
    }

    /// Parent cell id and its absolute origin.
    fn parent(&self, member: BoardMember) -> (String, Vec2) {
        if let Some(group) = self.member_group(member) {
            return (format!("g{}", group.0), self.groups[&group].1.min);
        }
        match self.member_zone(member) {
            Some(zone) => (format!("z{}", zone.0), self.zone_origin(zone)),
            None => ("1".into(), Vec2::ZERO),
        }
    }

    fn vertex(
        &mut self,
        id: &str,
        label: &str,
        style: &str,
        parent: &str,
        rect: Rect,
        user: Option<(&str, &str)>,
    ) {
        let geometry = format!(
            "<mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\"/>",
            num(rect.min.x),
            num(rect.min.y),
            num(rect.size.x),
            num(rect.size.y)
        );
        let label = esc(&html(label));
        let style = esc(style);
        match user {
            Some((tooltip, link)) => {
                self.out.push_str(&format!("<UserObject label=\"{label}\" id=\"{id}\""));
                if !tooltip.is_empty() {
                    self.out.push_str(&format!(" tooltip=\"{}\"", esc(tooltip)));
                }
                if !link.is_empty() {
                    self.out.push_str(&format!(" link=\"{}\"", esc(link)));
                }
                self.out.push_str(&format!(
                    "><mxCell style=\"{style}\" vertex=\"1\" parent=\"{parent}\">{geometry}</mxCell></UserObject>\n"
                ));
            }
            None => self.out.push_str(&format!(
                "<mxCell id=\"{id}\" value=\"{label}\" style=\"{style}\" vertex=\"1\" parent=\"{parent}\">{geometry}</mxCell>\n"
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn edge(
        &mut self,
        id: &str,
        label: &str,
        style: &str,
        parent: &str,
        origin: Vec2,
        ends: [Terminal; 2],
        points: &[Vec2],
        tooltip: &str,
    ) {
        let mut terminals = String::new();
        let mut geometry = String::new();
        for (end, (attr, role)) in ends
            .iter()
            .zip([("source", "sourcePoint"), ("target", "targetPoint")])
        {
            match end {
                Terminal::Cell(cell) => terminals.push_str(&format!(" {attr}=\"{cell}\"")),
                Terminal::Point(p) => geometry.push_str(&format!(
                    "<mxPoint x=\"{}\" y=\"{}\" as=\"{role}\"/>",
                    num(p.x - origin.x),
                    num(p.y - origin.y)
                )),
            }
        }
        if !points.is_empty() {
            geometry.push_str("<Array as=\"points\">");
            for p in points {
                geometry.push_str(&format!(
                    "<mxPoint x=\"{}\" y=\"{}\"/>",
                    num(p.x - origin.x),
                    num(p.y - origin.y)
                ));
            }
            geometry.push_str("</Array>");
        }
        let cell = format!(
            "style=\"{}\" edge=\"1\" parent=\"{parent}\"{terminals}><mxGeometry relative=\"1\" as=\"geometry\">{geometry}</mxGeometry></mxCell>",
            esc(style)
        );
        let label = esc(&html(label));
        if tooltip.is_empty() {
            self.out
                .push_str(&format!("<mxCell id=\"{id}\" value=\"{label}\" {cell}\n"));
        } else {
            self.out.push_str(&format!(
                "<UserObject label=\"{label}\" tooltip=\"{}\" id=\"{id}\"><mxCell {cell}</UserObject>\n",
                esc(tooltip)
            ));
        }
    }

    fn terminal(&self, end: Endpoint) -> Terminal {
        match end {
            Endpoint::Point(p) => Terminal::Point(p),
            Endpoint::Object(id) => match self.sheet.canvas.object(id) {
                Some(object) if matches!(object.kind, CanvasObjectKind::Shape { .. }) => {
                    Terminal::Cell(format!("o{}", id.0))
                }
                Some(object) if object.is_connector() => {
                    Terminal::Point(self.connector_midpoints.get(&id).copied().unwrap_or_else(
                        || {
                            log::warn!("draw.io connector target {id} has no resolved path");
                            object.rotated_bounds().center()
                        },
                    ))
                }
                Some(object) => Terminal::Point(object.rotated_bounds().center()),
                None => Terminal::Point(Vec2::ZERO),
            },
            Endpoint::Topic(id) if self.topics.contains(&id) => {
                Terminal::Cell(format!("t{}", id.0))
            }
            Endpoint::Topic(id) => Terminal::Point(
                self.layout
                    .rect(id)
                    .map(|r| r.center())
                    .or_else(|| self.sheet.topic(id).and_then(|t| t.position))
                    .unwrap_or(Vec2::ZERO),
            ),
        }
    }

    fn zones(&mut self) {
        for zone in &self.sheet.zones {
            let mut style =
                String::from("swimlane;whiteSpace=wrap;html=1;startSize=24;collapsible=0;");
            color_keys(&mut style, "strokeColor", "strokeOpacity", zone.color);
            color_keys(
                &mut style,
                "swimlaneFillColor",
                "fillOpacity",
                zone.background_color,
            );
            if zone.clip {
                style.push_str("thinodaClip=1;");
            }
            self.vertex(
                &format!("z{}", zone.id.0),
                &zone.name,
                &style,
                "1",
                zone.frame,
                None,
            );
        }
    }

    /// Writes a board member, preceded by its group cell and followed by the
    /// group's other members, so draw.io sees each group as one container.
    fn member(&mut self, member: BoardMember) {
        if self.written.contains(&member) {
            return;
        }
        if let Some(group) = self.member_group(member)
            && self.written_groups.insert(group)
        {
            let (parent, bounds) = self.groups[&group].clone();
            let origin = if parent == "1" {
                Vec2::ZERO
            } else {
                self.zone_origin(ZoneId(parent[1..].parse().unwrap_or(0)))
            };
            let locked = self.sheet.canvas.group(group).is_some_and(|g| g.locked);
            let style = if locked { "group;locked=1;" } else { "group;" };
            self.vertex(
                &format!("g{}", group.0),
                "",
                style,
                &parent,
                bounds.translate(Vec2::ZERO - origin),
                None,
            );
            for other in self.sheet.board_group_members(group) {
                self.write_member(other);
            }
        }
        self.write_member(member);
    }

    fn write_member(&mut self, member: BoardMember) {
        if !self.written.insert(member) {
            return;
        }
        match member {
            BoardMember::Object(id) => self.object(id),
            BoardMember::Card(id) => self.card(id),
        }
    }

    fn object(&mut self, id: CanvasObjectId) {
        let Some(object) = self.sheet.canvas.object(id) else {
            return;
        };
        let (parent, origin) = self.parent(BoardMember::Object(id));
        let cell_id = format!("o{}", id.0);
        match &object.kind {
            CanvasObjectKind::Shape { shape, label } => {
                let mut style = String::new();
                let style_ref = &object.style;
                let stencil_native = object
                    .stencil
                    .as_ref()
                    .and_then(crate::drawio_stencil::native_style);
                if let Some(native) = &stencil_native {
                    style.push_str(native);
                } else if let Some(sides) = object.rectangle_sides {
                    style.push_str(&format!(
                        "shape=partialRectangle;top={};right={};bottom={};left={};",
                        u8::from(sides.top),
                        u8::from(sides.right),
                        u8::from(sides.bottom),
                        u8::from(sides.left),
                    ));
                } else if let Some(rounding) = object.rounding {
                    if rounding.is_standard() {
                        style.push_str(&format!(
                            "shape=rectangle;rounded=1;arcSize={};absoluteArcSize={};roundedPerimeter={};",
                            num(rounding.radius),
                            u8::from(rounding.mode == ShapeRoundingMode::AbsoluteArc),
                            u8::from(rounding.rounded_perimeter),
                        ));
                    } else {
                        style.push_str(&format!(
                            "shape={};rSize={};",
                            rounded_stencil_name(rounding.stencil),
                            num(rounding.radius)
                        ));
                    }
                } else if *shape == ShapeKind::Rectangle
                    && style_ref.fill.is_some_and(|c| c.a == 0)
                    && style_ref.stroke.is_some_and(|c| c.a == 0)
                {
                    style.push_str("text;");
                } else {
                    style.push_str(shape_style(*shape));
                }
                if let Some(margin) = object.double_ellipse_margin {
                    style.push_str(&format!("margin={};", num(margin)));
                }
                if let Some(stencil) = &object.stencil {
                    style.push_str(&crate::drawio_stencil::reference_style(stencil));
                }
                style.push_str("whiteSpace=wrap;html=1;");
                let special_outline = object.rectangle_sides.is_some()
                    || matches!(*shape, ShapeKind::DoubleEllipse | ShapeKind::FinalState);
                let fill = if special_outline {
                    Some(style_ref.fill.unwrap_or(Color::TRANSPARENT))
                } else {
                    style_ref.fill
                };
                let stroke =
                    if special_outline && style_ref.stroke.is_none() && style_ref.fill.is_some() {
                        Some(Color::TRANSPARENT)
                    } else {
                        style_ref.stroke
                    };
                color_keys(&mut style, "fillColor", "fillOpacity", fill);
                color_keys(&mut style, "strokeColor", "strokeOpacity", stroke);
                color_keys(&mut style, "fontColor", "textOpacity", style_ref.text_color);
                let width = if special_outline {
                    style_ref.effective_shape_stroke_width()
                } else {
                    style_ref.stroke_width
                };
                style.push_str(&format!("strokeWidth={};", num(width)));
                common_keys(
                    &mut style,
                    style_ref,
                    object.rotation,
                    object.locked,
                    object.band,
                );
                let user = (!object.alt_text.is_empty()).then_some((object.alt_text.as_str(), ""));
                self.vertex(
                    &cell_id,
                    label,
                    &style,
                    &parent,
                    object.rect.translate(Vec2::ZERO - origin),
                    user,
                );
            }
            CanvasObjectKind::Connector(connector) => {
                let mut style = String::from("html=1;");
                match connector.route {
                    ConnectorRoute::Straight => {
                        if !connector.waypoints.is_empty() {
                            style.push_str("edgeStyle=none;");
                        }
                    }
                    ConnectorRoute::Elbow => style.push_str(
                        "edgeStyle=orthogonalEdgeStyle;rounded=0;orthogonalLoop=1;jettySize=auto;",
                    ),
                    ConnectorRoute::Curve => {
                        style.push_str("curved=1;");
                        if !connector.waypoints.is_empty() {
                            style.push_str("edgeStyle=none;");
                        }
                    }
                }
                let marker_name = |marker: Option<ConnectorMarker>| match marker.map(|m| m.kind) {
                    Some(ConnectorMarkerKind::Classic) => "classic",
                    Some(ConnectorMarkerKind::ClassicThin) => "classicThin",
                    Some(ConnectorMarkerKind::Open) => "open",
                    Some(ConnectorMarkerKind::OpenThin) => "openThin",
                    Some(ConnectorMarkerKind::Block) => "block",
                    Some(ConnectorMarkerKind::BlockThin) => "blockThin",
                    Some(ConnectorMarkerKind::Diamond) => "diamond",
                    Some(ConnectorMarkerKind::DiamondThin) => "diamondThin",
                    Some(ConnectorMarkerKind::Oval) => "oval",
                    Some(ConnectorMarkerKind::HalfCircle) => "halfCircle",
                    Some(ConnectorMarkerKind::BaseDash) => "baseDash",
                    Some(ConnectorMarkerKind::Dash) => "dash",
                    None => "classic",
                };
                style.push_str("endArrow=");
                style.push_str(if connector.arrow_end {
                    marker_name(connector.end_marker)
                } else {
                    "none"
                });
                style.push(';');
                if connector.arrow_end && connector.end_marker.is_none() {
                    style.push_str("thinodaLegacyEndArrow=1;");
                }
                if connector.arrow_start {
                    style.push_str("startArrow=");
                    style.push_str(marker_name(connector.start_marker));
                    style.push(';');
                    if connector.start_marker.is_none() {
                        style.push_str("thinodaLegacyStartArrow=1;");
                    }
                }
                if let Some(size) = connector.start_marker_size {
                    style.push_str(&format!("startSize={};", num(size)));
                }
                if let Some(size) = connector.end_marker_size {
                    style.push_str(&format!("endSize={};", num(size)));
                }
                if connector.start_marker_unfilled {
                    style.push_str("startFill=0;");
                }
                if connector.end_marker_unfilled {
                    style.push_str("endFill=0;");
                }
                color_keys(
                    &mut style,
                    "strokeColor",
                    "strokeOpacity",
                    if connector.sankey.is_some()
                        && self.sheet.sankey.as_ref().is_some_and(|s| s.color_scale)
                    {
                        Some(thinoda_core::sankey::weight_color(
                            self.flows.mass.get(&id).copied().unwrap_or(0.0),
                        ))
                    } else {
                        connector.color.or(object.style.stroke)
                    },
                );
                color_keys(
                    &mut style,
                    "fontColor",
                    "textOpacity",
                    object
                        .style
                        .text_color
                        .or_else(|| connector.sankey.is_some().then_some(Color::BLACK)),
                );
                if !connector.label.is_empty() || connector.sankey.is_some() {
                    style.push_str(&format!(
                        "fontSize={};",
                        num(if connector.sankey.is_some() {
                            12.0
                        } else {
                            CANVAS_LABEL_FONT_SIZE
                        })
                    ));
                }
                if connector.sankey.is_some() {
                    style.push_str("thinodaSankeyEdge=1;");
                }
                let width =
                    exported_flow_width(self.sheet, &self.layout, &self.flows, id, connector);
                style.push_str(&format!("strokeWidth={};", num(width)));
                if connector.sankey.is_some_and(|f| f.weight.is_none()) {
                    style.push_str("dashed=1;");
                }
                common_keys(&mut style, &object.style, 0.0, object.locked, object.band);
                if connector.sankey.is_some_and(|f| f.weight.is_some())
                    && self.flows.mass.get(&id).is_some_and(|v| *v == 0.0)
                {
                    style.push_str("opacity=0;");
                }
                let mut ends = [self.terminal(connector.from), self.terminal(connector.to)];
                for (side, port) in [connector.from_port, connector.to_port]
                    .into_iter()
                    .enumerate()
                {
                    if port.is_some() && matches!(ends[side], Terminal::Point(_)) {
                        if let Some(points) = self.detached_fixed_points.get(&object.id) {
                            ends[side] =
                                Terminal::Point(if side == 0 { points.0 } else { points.1 });
                        } else {
                            log::warn!(
                                "draw.io fixed connector end {}:{side} has no resolved path",
                                object.id
                            );
                        }
                    }
                }
                for (port, terminal, prefix) in [
                    (connector.from_port, &ends[0], "exit"),
                    (connector.to_port, &ends[1], "entry"),
                ] {
                    if let Some(port) = port
                        && matches!(terminal, Terminal::Cell(_))
                    {
                        style.push_str(&format!(
                            "{prefix}X={};{prefix}Y={};{prefix}Dx={};{prefix}Dy={};{prefix}Perimeter={};",
                            num(port.position.x),
                            num(port.position.y),
                            num(port.offset.x),
                            num(port.offset.y),
                            u8::from(port.perimeter)
                        ));
                    }
                }
                self.edge(
                    &cell_id,
                    &if connector.sankey.is_some() {
                        thinoda_core::sankey::flow_label(
                            connector,
                            self.flows.mass.get(&id).copied().unwrap_or(0.0),
                        )
                    } else {
                        connector.label.clone()
                    },
                    &style,
                    &parent,
                    origin,
                    ends,
                    &connector.waypoints,
                    &object.alt_text,
                );
            }
            CanvasObjectKind::Ink(ink) => {
                let mut style = String::from(match ink.tool {
                    InkTool::Pen => "thinodaInk=1;",
                    InkTool::Highlighter => "thinodaInk=highlighter;",
                });
                style.push_str("endArrow=none;html=1;");
                color_keys(&mut style, "strokeColor", "strokeOpacity", Some(ink.color));
                style.push_str(&format!("strokeWidth={};", num(ink.width)));
                common_keys(&mut style, &object.style, 0.0, object.locked, object.band);
                let center = object.rect.center();
                let points: Vec<Vec2> = ink
                    .points
                    .iter()
                    .map(|p| rotate_around(*p, center, object.rotation))
                    .collect();
                let first = points[0];
                let (middle, last) = match points.len() {
                    1 => (&points[..0], None),
                    n => (&points[1..n - 1], Some(points[n - 1])),
                };
                let mut geometry = format!(
                    "<mxPoint x=\"{}\" y=\"{}\" as=\"sourcePoint\"/>",
                    num(first.x - origin.x),
                    num(first.y - origin.y)
                );
                if let Some(last) = last {
                    geometry.push_str(&format!(
                        "<mxPoint x=\"{}\" y=\"{}\" as=\"targetPoint\"/>",
                        num(last.x - origin.x),
                        num(last.y - origin.y)
                    ));
                }
                if !middle.is_empty() {
                    geometry.push_str("<Array as=\"points\">");
                    for p in middle {
                        geometry.push_str(&format!(
                            "<mxPoint x=\"{}\" y=\"{}\"/>",
                            num(p.x - origin.x),
                            num(p.y - origin.y)
                        ));
                    }
                    geometry.push_str("</Array>");
                }
                self.out.push_str(&format!(
                    "<mxCell id=\"{cell_id}\" value=\"\" style=\"{}\" edge=\"1\" parent=\"{parent}\"><mxGeometry relative=\"1\" as=\"geometry\">{geometry}</mxGeometry></mxCell>\n",
                    esc(&style)
                ));
            }
        }
    }

    fn card(&mut self, id: TopicId) {
        let (Some(topic), Some(rect)) = (self.sheet.topic(id), self.card_rect(id)) else {
            return;
        };
        let Some(card) = topic.card else {
            return;
        };
        let (parent, origin) = self.parent(BoardMember::Card(id));
        let image = card_image(self.sheet, topic);
        let mut style = String::from(match (card.kind, image) {
            (CardKind::Sticky, _) => "shape=note;thinodaCard=sticky;backgroundOutline=1;size=14;",
            (CardKind::Text, _) => "text;thinodaCard=text;",
            (CardKind::Image, None) => "shape=note;thinodaCard=image;backgroundOutline=1;size=14;",
            (CardKind::Image, Some(_)) => {
                "shape=image;thinodaCard=image;imageAspect=1;verticalAlign=bottom;"
            }
        });
        if matches!(card.kind, CardKind::Sticky | CardKind::Text) {
            let align = thinoda_core::theme::resolve_style(self.sheet, id, false).align;
            style.push_str(match align {
                TextAlign::Left => "align=left;",
                TextAlign::Center => "align=center;",
                TextAlign::Right => "align=right;",
            });
        }
        if let Some(resource) = image {
            // draw.io's style syntax reserves `;`, so inline images omit `;base64`.
            style.push_str(&format!(
                "image=data:{},{};",
                resource.media_type,
                base64::engine::general_purpose::STANDARD.encode(resource.bytes())
            ));
            if let Some(size) = topic.image_size {
                style.push_str(&format!(
                    "thinodaImageWidth={};thinodaImageHeight={};",
                    num(size.x),
                    num(size.y)
                ));
            }
        }
        style.push_str("whiteSpace=wrap;html=1;");
        color_keys(
            &mut style,
            "fillColor",
            "fillOpacity",
            card.color.or(topic.style.fill),
        );
        color_keys(
            &mut style,
            "fontColor",
            "textOpacity",
            topic.style.text_color,
        );
        let locked = self.sheet.canvas.card_state(id).is_some_and(|s| s.locked);
        common_keys(
            &mut style,
            &ObjectStyle::default(),
            card.rotation,
            locked,
            Band::Above,
        );
        let link = topic.hyperlink.as_deref().unwrap_or_default();
        self.vertex(
            &format!("t{}", id.0),
            &topic.title,
            &style,
            &parent,
            rect.translate(Vec2::ZERO - origin),
            (!link.is_empty()).then_some(("", link)),
        );
    }

    /// Ordinary topics and their annotations as plain diagram cells.
    fn map(&mut self) {
        let sheet = self.sheet;
        for boundary in &self.layout.boundaries.clone() {
            let model = sheet.boundaries.iter().find(|b| b.id == boundary.id);
            let mut style = String::from(
                "rounded=1;dashed=1;fillColor=none;whiteSpace=wrap;html=1;verticalAlign=top;",
            );
            color_keys(
                &mut style,
                "strokeColor",
                "strokeOpacity",
                model.and_then(|b| b.color),
            );
            let title = model.and_then(|b| b.title.as_deref()).unwrap_or_default();
            self.vertex(
                &format!("b{}", boundary.id.0),
                title,
                &style,
                "1",
                boundary.rect,
                None,
            );
        }
        for (n, connector) in self.layout.connectors.clone().iter().enumerate() {
            let mut style = String::from("endArrow=none;html=1;");
            color_keys(
                &mut style,
                "strokeColor",
                "strokeOpacity",
                Some(connector.color),
            );
            style.push_str(&format!("strokeWidth={};", num(connector.width)));
            let ends = [
                self.topic_terminal(connector.parent, connector.start),
                self.topic_terminal(connector.child, connector.end),
            ];
            self.edge(&format!("e{n}"), "", &style, "1", Vec2::ZERO, ends, &[], "");
        }
        for id in self.layout.order.clone() {
            let Some(topic) = sheet.topic(id) else {
                continue;
            };
            if sheet.is_anchor(id) || topic.card.is_some() {
                continue;
            }
            let (Some(layout), Some(rect)) = (self.layout.topics.get(&id), self.layout.rect(id))
            else {
                continue;
            };
            let resolved = &layout.style;
            let mut style = String::from(topic_shape_style(resolved.shape));
            style.push_str("whiteSpace=wrap;html=1;");
            color_keys(
                &mut style,
                "fillColor",
                "fillOpacity",
                Some(resolved.fill.unwrap_or(Color::TRANSPARENT)),
            );
            color_keys(
                &mut style,
                "strokeColor",
                "strokeOpacity",
                Some(resolved.border_color.unwrap_or(Color::TRANSPARENT)),
            );
            color_keys(
                &mut style,
                "fontColor",
                "textOpacity",
                Some(resolved.text_color),
            );
            style.push_str(&format!(
                "strokeWidth={};",
                num(resolved
                    .border_width
                    .clamp(MIN_STROKE_WIDTH, MAX_STROKE_WIDTH))
            ));
            let link = topic.hyperlink.clone().unwrap_or_default();
            self.vertex(
                &format!("t{}", id.0),
                &topic.title,
                &style,
                "1",
                rect,
                (!link.is_empty()).then_some(("", link.as_str())),
            );
        }
        for relationship in self.layout.relationships.clone() {
            let Some(model) = sheet.relationships.iter().find(|r| r.id == relationship.id) else {
                continue;
            };
            let mut style = String::from("curved=1;dashed=1;endArrow=classic;html=1;");
            color_keys(&mut style, "strokeColor", "strokeOpacity", model.color);
            if !model.title.is_empty() {
                style.push_str(&format!("fontSize={};", num(CANVAS_LABEL_FONT_SIZE)));
            }
            let ends = [
                self.topic_terminal(model.from, relationship.start),
                self.topic_terminal(model.to, relationship.end),
            ];
            self.edge(
                &format!("r{}", relationship.id.0),
                &model.title,
                &style,
                "1",
                Vec2::ZERO,
                ends,
                &[],
                "",
            );
        }
        for summary in self.layout.summaries.clone() {
            self.edge(
                &format!("s{}", summary.id.0),
                "",
                "endArrow=none;html=1;",
                "1",
                Vec2::ZERO,
                [Terminal::Point(summary.start), Terminal::Point(summary.end)],
                &[summary.tip],
                "",
            );
        }
    }

    fn topic_terminal(&self, id: TopicId, fallback: Vec2) -> Terminal {
        if self.topics.contains(&id) {
            Terminal::Cell(format!("t{}", id.0))
        } else {
            Terminal::Point(fallback)
        }
    }

    fn write(mut self) -> String {
        let sheet = self.sheet;
        self.zones();
        for object in sheet
            .canvas
            .objects
            .iter()
            .filter(|o| o.band == Band::Below)
        {
            self.member(BoardMember::Object(object.id));
        }
        self.map();
        for id in self.layout.order.clone() {
            if sheet.topic(id).is_some_and(|t| t.card.is_some()) {
                self.member(BoardMember::Card(id));
            }
        }
        for object in sheet
            .canvas
            .objects
            .iter()
            .filter(|o| o.band == Band::Above)
        {
            self.member(BoardMember::Object(object.id));
        }
        self.out
    }
}

/// Writes all sheets as uncompressed draw.io pages.
pub(crate) fn export(book: &Workbook) -> Result<Vec<u8>, IoError> {
    Ok(export_sheets(&book.sheets)?.into_bytes())
}

fn export_sheets<'a>(sheets: impl IntoIterator<Item = &'a Sheet>) -> Result<String, IoError> {
    let mut out = String::from("<mxfile host=\"Thinoda\" type=\"device\">\n");
    for (index, sheet) in sheets.into_iter().enumerate() {
        let (grid, thinoda_grid) = match sheet.board.grid {
            GridKind::None => (0, "none"),
            GridKind::Dots => (1, "dots"),
            GridKind::Lines => (1, "lines"),
        };
        let thinoda_snap = u8::from(sheet.board.snap);
        let sankey = if sheet.has_sankey() {
            let extension = SankeyExtension::from_sheet(sheet)?;
            format!(
                " thinodaSankey=\"{}\"",
                esc(&serde_json::to_string(&extension)?)
            )
        } else {
            String::new()
        };
        let background = sheet
            .theme
            .background
            .map(|color| {
                let color = color.to_hex();
                format!(" background=\"light-dark({color},{color})\"")
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "<diagram id=\"thinoda-page-{}\" name=\"{}\">\n<mxGraphModel dx=\"0\" dy=\"0\" grid=\"{grid}\" gridSize=\"{}\" thinodaGrid=\"{thinoda_grid}\" thinodaSnap=\"{thinoda_snap}\"{background}{sankey} guides=\"1\" tooltips=\"1\" connect=\"1\" arrows=\"1\" fold=\"1\" page=\"0\" pageScale=\"1\" pageWidth=\"850\" pageHeight=\"1100\" math=\"0\" shadow=\"0\">\n<root>\n<mxCell id=\"0\"/>\n<mxCell id=\"1\" parent=\"0\"/>\n",
            index + 1,
            esc(&sheet.title),
            num(sheet.board.grid_size),
        ));
        out.push_str(&PageWriter::new(sheet)?.write());
        out.push_str("</root>\n</mxGraphModel>\n</diagram>\n");
    }
    out.push_str("</mxfile>\n");
    Ok(out)
}

fn valid_rendered_svg(svg: &str) -> bool {
    if svg.len() > MAX_BYTES || !svg.starts_with("<svg ") {
        return false;
    }
    let mut reader = quick_xml::Reader::from_str(svg);
    let mut depth = 0usize;
    let mut seen_root = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                if depth == 0 {
                    if seen_root || start.name().as_ref() != "svg" {
                        return false;
                    }
                    for attr in start.attributes() {
                        let Ok(attr) = attr else { return false };
                        if attr.key.as_ref() == "content" {
                            return false;
                        }
                    }
                    seen_root = true;
                }
                depth += 1;
            }
            Ok(Event::Empty(_)) if depth == 0 => return false,
            Ok(Event::End(_)) => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            Ok(Event::Text(text)) if depth == 0 && !text.xml10_content().trim().is_empty() => {
                return false;
            }
            Ok(Event::CData(_) | Event::GeneralRef(_)) if depth == 0 => return false,
            Ok(Event::DocType(_)) | Err(_) => return false,
            Ok(Event::Eof) => return seen_root && depth == 0,
            _ => {}
        }
    }
}

fn embedded_image_fits(image: usize, payload: usize, overhead: usize) -> bool {
    image
        .checked_add(payload)
        .and_then(|length| length.checked_add(overhead))
        .is_some_and(|length| length <= MAX_BYTES)
}

pub(crate) fn embed_svg(mut svg: String, sheet: &Sheet) -> Result<Vec<u8>, IoError> {
    if svg.len() > MAX_BYTES {
        return Err(invalid("editable draw.io image exceeds size limit"));
    }
    if !valid_rendered_svg(&svg) {
        return Err(invalid("rendered SVG is not a valid SVG document"));
    }
    let xml = export_sheets(std::iter::once(sheet))?;
    let escaped_len = xml.chars().try_fold(0usize, |length, character| {
        let size = match character {
            '&' => 5,
            '<' | '>' => 4,
            '"' => 6,
            '\n' | '\r' => 5,
            '\t' => 4,
            c if c < ' ' || c == '\u{fffe}' || c == '\u{ffff}' => 0,
            c => c.len_utf8(),
        };
        length.checked_add(size)
    });
    if !escaped_len
        .is_some_and(|length| embedded_image_fits(svg.len(), length, " content=\"\"".len()))
    {
        return Err(invalid("editable draw.io image exceeds size limit"));
    }
    svg.insert_str(4, &format!(" content=\"{}\"", esc(&xml)));
    Ok(svg.into_bytes())
}

fn rendered_png_insertion(png: &[u8]) -> Option<usize> {
    if png.len() > MAX_BYTES || !png.starts_with(PNG_SIGNATURE) {
        return None;
    }
    let mut pos = PNG_SIGNATURE.len();
    let mut has_data = false;
    loop {
        let header = png.get(pos..pos.checked_add(8)?)?;
        let len = u32::from_be_bytes(header[..4].try_into().ok()?) as usize;
        let kind = &header[4..];
        let data_start = pos.checked_add(8)?;
        let data_end = data_start.checked_add(len)?;
        let end = data_end.checked_add(4)?;
        let data = png.get(data_start..data_end)?;
        let checksum = u32::from_be_bytes(png.get(data_end..end)?.try_into().ok()?);
        if crc32fast::hash(png.get(pos + 4..data_end)?) != checksum {
            return None;
        }
        if pos == PNG_SIGNATURE.len() {
            if kind != b"IHDR" || len != 13 || data[..4] == [0; 4] || data[4..8] == [0; 4] {
                return None;
            }
        } else if kind == b"IHDR" {
            return None;
        }
        if matches!(kind, b"tEXt" | b"zTXt" | b"iTXt") && data.starts_with(b"mxfile\0") {
            return None;
        }
        if kind == b"IDAT" && !data.is_empty() {
            has_data = true;
        }
        if kind == b"IEND" {
            // diagrams.net looks for editable metadata before the first IDAT.
            return (len == 0 && has_data && end == png.len())
                .then_some(PNG_SIGNATURE.len() + 12 + 13);
        }
        pos = end;
    }
}

pub(crate) fn embed_png(mut png: Vec<u8>, sheet: &Sheet) -> Result<Vec<u8>, IoError> {
    if png.len() > MAX_BYTES {
        return Err(invalid("editable draw.io image exceeds size limit"));
    }
    let insertion = rendered_png_insertion(&png)
        .ok_or_else(|| invalid("rendered PNG has no valid image and end chunk"))?;
    let xml = export_sheets(std::iter::once(sheet))?;
    let text_len = crate::drawio_clipboard::encoded_uri_component_len(&xml)
        .and_then(|length| length.checked_add(b"mxfile\0".len()))
        .ok_or_else(|| invalid("editable draw.io image exceeds size limit"))?;
    if !embedded_image_fits(png.len(), text_len, 12) {
        return Err(invalid("editable draw.io image exceeds size limit"));
    }
    let text_len = u32::try_from(text_len)
        .map_err(|_| invalid("editable draw.io image exceeds size limit"))?;
    let text = crate::drawio_clipboard::encode_uri_component(&xml);
    let chunk_len = text_len as usize + 12;
    let original_len = png.len();
    png.resize(original_len + chunk_len, 0);
    png.copy_within(insertion..original_len, insertion + chunk_len);
    let chunk = &mut png[insertion..insertion + chunk_len];
    chunk[..4].copy_from_slice(&text_len.to_be_bytes());
    chunk[4..8].copy_from_slice(b"tEXt");
    chunk[8..15].copy_from_slice(b"mxfile\0");
    chunk[15..chunk_len - 4].copy_from_slice(text.as_bytes());
    let checksum = crc32fast::hash(&chunk[4..chunk_len - 4]);
    chunk[chunk_len - 4..].copy_from_slice(&checksum.to_be_bytes());
    Ok(png)
}

#[cfg(test)]
mod tests;
