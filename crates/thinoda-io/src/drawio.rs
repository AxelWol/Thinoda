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
//! details use `thinoda*` style keys, which draw.io preserves.

use crate::{Format, ImportReport, ImportWarning, IoError};
use base64::Engine;
use quick_xml::events::{BytesStart, Event};
use std::collections::{BTreeSet, HashMap, HashSet};
use thinoda_core::{
    ApproxMeasure, Color, MAX_PRESENTATION_NAME_BYTES, MAX_ZONES, MIN_ZONE_SIZE, Rect, Sheet,
    TextAlign, TopicId, TopicShape, Vec2, Workbook, ZoneId,
    board::{
        Band, BoardMember, CanvasConnector, CanvasGroupId, CanvasObject, CanvasObjectId,
        CanvasObjectKind, Card, CardKind, ConnectorMarker, ConnectorMarkerKind, ConnectorRoute,
        DEFAULT_CONNECTOR_MARKER_SIZE, DrawioRectangleStencil, Endpoint, GridKind, InkStroke,
        InkTool, MAX_BOARD_COORDINATE, MAX_BOARD_TEXT_CHARS, MAX_GRID_SIZE, MAX_STROKE_WIDTH,
        MIN_CARD_SIZE, MIN_GRID_SIZE, MIN_STROKE_WIDTH, ObjectStyle, ShapeKind, ShapeRounding,
        SheetRootMode, normalize_rotation, rotate_around, valid_connector_marker_size,
    },
    images::{self, MAX_DISPLAY_SIZE},
    layout_sheet,
    resources::{MAX_RESOURCE_BYTES, Resource},
};

mod html;

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
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Element>,
    text: String,
}

impl Element {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn child(&self, name: &str) -> Option<&Element> {
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
fn parse_dom(text: &str) -> Result<Element, IoError> {
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
fn decode_compressed(text: &str) -> Result<String, IoError> {
    let cleaned: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let data = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .map_err(|_| invalid("draw.io page is neither XML nor compressed diagram data"))?;
    decode_compressed_bytes(&data)
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

#[derive(Debug, Default)]
struct Style {
    tokens: Vec<String>,
    map: HashMap<String, String>,
    suppress_defaults: bool,
}

impl Style {
    fn parse(text: &str) -> Self {
        let mut style = Style {
            suppress_defaults: text.starts_with(';'),
            ..Self::default()
        };
        for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            match part.split_once('=') {
                Some((key, value)) => {
                    style
                        .map
                        .insert(key.trim().to_owned(), value.trim().to_owned());
                }
                None => {
                    // Named styles are merged in order; defaultEdge restores its end marker.
                    if part == "defaultEdge" {
                        style.map.insert("endArrow".into(), "classic".into());
                    }
                    style.tokens.push(part.to_owned());
                }
            }
        }
        style
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    fn flag(&self, key: &str) -> bool {
        self.get(key) == Some("1")
    }

    fn num(&self, key: &str) -> Option<f32> {
        self.get(key)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
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
        let value = self.get(key)?;
        let value = value
            .strip_prefix("light-dark(")
            .and_then(|v| v.split(',').next())
            .unwrap_or(value)
            .trim();
        let color = match value {
            "" | "default" | "inherit" => return None,
            "none" => return Some(Color::TRANSPARENT),
            hex => Color::from_hex(hex)?,
        };
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

    /// Visual details without a Thinoda equivalent.
    fn formatting_loss(&self, edge: bool, card_text_alignment: bool) -> bool {
        const FLAGS: [&str; 7] = [
            "dashed", "shadow", "glass", "sketch", "flipH", "flipV", "comic",
        ];
        const PORT_KEYS: [&str; 14] = [
            "exitX",
            "exitY",
            "exitDx",
            "exitDy",
            "exitPerimeter",
            "entryX",
            "entryY",
            "entryDx",
            "entryDy",
            "entryPerimeter",
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
        let unresolved_text_opacity = self.get("textOpacity").is_some_and(|_| {
            self.num("textOpacity").is_none_or(|opacity| {
                opacity != 100.0 && self.color("fontColor", "textOpacity").is_none()
            })
        });
        let unresolved_font_color = self.get("fontColor").is_some_and(|color| {
            !matches!(color, "" | "default" | "none")
                && self.color("fontColor", "textOpacity").is_none()
        });
        FLAGS.iter().any(|key| self.flag(key))
            || self.get("fontColor") == Some("none")
            || unresolved_text_opacity
            || unresolved_font_color
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

fn html_entity(name: &str) -> Option<char> {
    if let Some(number) = name.strip_prefix('#') {
        let value = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(value).filter(|c| *c != '\0');
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        _ => return None,
    })
}

/// Plain text of an HTML label; markup beyond line structure is reported.
fn strip_html(text: &str, warnings: &mut BTreeSet<ImportWarning>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(['<', '&']) {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if let Some(after) = tail.strip_prefix('<') {
            let is_tag =
                after.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!');
            if is_tag && let Some(end) = tail.find('>') {
                let tag = &tail[1..end];
                let closing = tag.starts_with('/');
                let body = tag.trim_start_matches('/');
                let name = body
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let has_attributes = body
                    .trim_end_matches('/')
                    .trim()
                    .contains(char::is_whitespace);
                match name.as_str() {
                    "br" => out.push('\n'),
                    "div" | "p" | "li" | "span" => {
                        if has_attributes {
                            warnings.insert(ImportWarning::Formatting);
                        }
                        if !closing && name != "span" && !out.is_empty() && !out.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                    _ if name.starts_with("!--") => {}
                    _ => {
                        warnings.insert(ImportWarning::Formatting);
                    }
                }
                rest = &tail[end + 1..];
                continue;
            }
            out.push('<');
            rest = &tail[1..];
        } else {
            let entity = tail
                .as_bytes()
                .iter()
                .take(12)
                .position(|&b| b == b';')
                .and_then(|end| html_entity(&tail[1..end]).map(|ch| (ch, end)));
            match entity {
                Some((ch, end)) => {
                    out.push(ch);
                    rest = &tail[end + 1..];
                }
                None => {
                    out.push('&');
                    rest = &tail[1..];
                }
            }
        }
    }
    out.push_str(rest);
    out
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

fn number(element: &Element, key: &str) -> f32 {
    element
        .attr(key)
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

fn point(element: &Element) -> Vec2 {
    Vec2::new(number(element, "x"), number(element, "y"))
}

fn waypoint(element: &Element, warnings: &mut BTreeSet<ImportWarning>) -> Option<Vec2> {
    let coordinate = |key| match element.attr(key) {
        None => Some(0.0),
        Some(value) => value
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite()),
    };
    match (coordinate("x"), coordinate("y")) {
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
    let mut geometry = Geometry {
        x: number(element, "x"),
        y: number(element, "y"),
        width: number(element, "width").max(0.0),
        height: number(element, "height").max(0.0),
        relative: element.attr("relative") == Some("1"),
        ..Geometry::default()
    };
    for child in &element.children {
        match (child.name.as_str(), child.attr("as")) {
            ("mxPoint", Some("sourcePoint")) => geometry.source = Some(point(child)),
            ("mxPoint", Some("targetPoint")) => geometry.target = Some(point(child)),
            ("mxPoint", Some("offset")) => geometry.offset = Some(point(child)),
            ("Array", Some("points")) => {
                geometry.points = child
                    .children
                    .iter()
                    .filter(|p| p.name == "mxPoint")
                    .filter_map(|p| waypoint(p, warnings))
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

fn mockup_rectangle(style: &Style) -> Kind {
    use std::cmp::Ordering;

    let radius = style
        .get("rSize")
        .map_or(Some(Ordering::Greater), mockup_radius_sign);
    let unresolved_color = ["fillColor", "strokeColor", "fontColor"]
        .iter()
        .any(|key| match style.get(key) {
            Some("none") => false,
            Some(value) => !value
                .strip_prefix('#')
                .is_some_and(|hex| !hex.starts_with('#') && Color::from_hex(hex).is_some()),
            _ => true,
        });
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
            if style.flag("rounded") {
                symmetric(S::RoundedRectangle)
            } else {
                symmetric(S::Rectangle)
            }
        }
        "text" => Kind::Text,
        "note" => Kind::Card(CardKind::Sticky),
        "line" => Kind::Line,
        "image" => Kind::Image,
        "ellipse"
        | "doubleEllipse"
        | "mxgraph.flowchart.start_1"
        | "mxgraph.flowchart.start_2"
        | "mxgraph.flowchart.on-page_reference" => symmetric(S::Ellipse),
        "partialRectangle"
            if PARTIAL_SIDES
                .iter()
                .any(|side| style.get(side) == Some("0")) =>
        {
            Kind::Approx(S::Rectangle, 0.0, true)
        }
        "partialRectangle" | "mxgraph.basic.rect" | "mxgraph.bootstrap.rect" => {
            symmetric(S::Rectangle)
        }
        "mxgraph.bootstrap.rrect" => symmetric(S::RoundedRectangle),
        "waypoint" => Kind::Waypoint,
        "folder" | "cube" | "message" | "umlFrame" => Kind::Approx(S::Rectangle, 0.0, true),
        "endState" | "mxgraph.basic.partConcEllipse" => Kind::Approx(S::Ellipse, 0.0, true),
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
        "mxgraph.flowchart.process" => shape(S::Process, 0.0),
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

/// draw.io `partialRectangle` side flags; any `0` hides that side's outline.
const PARTIAL_SIDES: [&str; 4] = ["top", "right", "bottom", "left"];

/// draw.io draws waypoints as fixed-size dots regardless of the cell geometry.
const WAYPOINT_SIZE: f32 = 6.0;

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
    Ready(Endpoint),
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
    visited: Vec<bool>,
    zones: Vec<(ZoneId, Vec<TopicId>, Vec<CanvasObjectId>)>,
    buckets: Vec<(Vec<BoardMember>, bool)>,
    locked: Vec<BoardMember>,
    deferred: Vec<(usize, Context)>,
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
        match self.sheet.add_canvas_object(object) {
            Ok(id) => {
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
        let swimlane = cell.style.shape() == "swimlane" || cell.style.has("swimlane");
        if let Some(zone) = self.sheet.zones.iter_mut().find(|z| z.id == id) {
            zone.color = cell
                .style
                .color("strokeColor", "strokeOpacity")
                .filter(|c| c.a > 0);
            zone.background_color = if swimlane {
                cell.style.color("swimlaneFillColor", "fillOpacity")
            } else {
                cell.style.color("fillColor", "fillOpacity")
            };
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
                let mut object = shape_object(shape, frame, total + style.rotation(), label);
                if let Some(stencil) = mockup_rectangle_stencil(style) {
                    object.rounding = Some(ShapeRounding {
                        radius: style.get("rSize").map_or(10.0, mockup_radius),
                        stencil,
                    });
                }
                if style.shape() == "partialRectangle" && matches!(kind, Kind::Approx(..)) {
                    object.style.stroke = Some(Color::TRANSPARENT);
                }
                object
            }
            Kind::Waypoint => {
                let frame =
                    Rect::from_center_size(rect.center(), Vec2::new(WAYPOINT_SIZE, WAYPOINT_SIZE));
                let mut object = shape_object(ShapeKind::Ellipse, frame, 0.0, label);
                let color = object.style.stroke.unwrap_or(Color::BLACK);
                object.style.fill = Some(color);
                object.style.stroke = Some(color);
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
        card.color = style
            .color("fillColor", "fillOpacity")
            .filter(|c| c.a > 0 || kind == CardKind::Image);
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

    fn end(
        &self,
        terminal: Option<&str>,
        point: Option<Vec2>,
        origin: Vec2,
        last_pass: bool,
    ) -> End {
        if let Some(&index) = terminal.and_then(|id| self.index.get(id)) {
            match self.items.get(&index) {
                Some(Item::Object(id)) => return End::Ready(Endpoint::Object(*id)),
                Some(Item::Card(id)) => return End::Ready(Endpoint::Topic(*id)),
                Some(Item::Area(rect)) => return End::Ready(Endpoint::Point(rect.center())),
                None if !last_pass && !self.visited[index] => return End::Pending,
                None => {}
            }
        }
        match point {
            Some(p) => End::Ready(Endpoint::Point(origin + p)),
            None => End::Missing,
        }
    }

    fn edge(&mut self, index: usize, context: Context, last_pass: bool) {
        let cells = self.cells;
        let cell = &cells[index];
        let style = &cell.style;
        let origin = context.origin;
        if let Some(tool) = style.get("thinodaInk") {
            if style.formatting_loss(true, false) || style.start_arrow() || style.end_arrow() {
                self.warn(ImportWarning::Formatting);
            }
            let g = &cell.geometry;
            let points: Vec<Vec2> = g
                .source
                .iter()
                .chain(&g.points)
                .chain(&g.target)
                .map(|p| origin + *p)
                .collect();
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
        let from = self.end(
            cell.source.as_deref(),
            cell.geometry.source,
            origin,
            last_pass,
        );
        let to = self.end(
            cell.target.as_deref(),
            cell.geometry.target,
            origin,
            last_pass,
        );
        let (from, to) = match (from, to) {
            (End::Ready(from), End::Ready(to)) => (from, to),
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
        for child in self.children[index].clone() {
            let child = &cells[child];
            if child.vertex && child.visible {
                let text = self.label(child);
                if !text.is_empty() {
                    if !label.is_empty() {
                        label.push('\n');
                    }
                    label.push_str(&text);
                }
            }
        }
        if !label.is_empty()
            && (style.label_opacity_loss() || style.connector_label_appearance_loss(true))
        {
            self.warn(ImportWarning::Formatting);
        }
        let mut connector = CanvasConnector::new(from, to);
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
        connector.waypoints = cell
            .geometry
            .points
            .iter()
            .map(|point| origin + *point)
            .collect();
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
        self.add_object(index, object, context);
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

fn import_page(page: &Page, number: usize, warnings: &mut BTreeSet<ImportWarning>) -> Sheet {
    let title = if page.name.trim().is_empty() {
        format!("Page-{number}")
    } else {
        page.name.clone()
    };
    let mut sheet = Sheet::new(title, "");
    sheet.root_mode = SheetRootMode::Hidden;
    let model = &page.model;
    sheet.board.grid = if model.attr("grid") == Some("1") {
        GridKind::Lines
    } else {
        GridKind::None
    };
    if let Some(size) = model
        .attr("gridSize")
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite())
    {
        sheet.board.grid_size = size.clamp(MIN_GRID_SIZE, MAX_GRID_SIZE);
    }
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
        visited: vec![false; cells.len()],
        zones: Vec::new(),
        buckets: Vec::new(),
        locked: Vec::new(),
        deferred: Vec::new(),
    };
    import.run();
    import.finish()
}

/// Imports every page of a draw.io document as a Freeform board sheet.
pub(crate) fn import(bytes: &[u8]) -> Result<ImportReport, IoError> {
    let pages = read_pages(bytes)?;
    let mut warnings = BTreeSet::new();
    let mut sheets = pages
        .iter()
        .enumerate()
        .map(|(i, page)| import_page(page, i + 1, &mut warnings));
    let mut workbook = Workbook::new(
        sheets
            .next()
            .ok_or_else(|| invalid("draw.io document contains no pages"))?,
    );
    workbook.sheets.extend(sheets);
    Ok(ImportReport {
        format: Format::DrawIo,
        workbook,
        warnings: warnings.into_iter().collect(),
    })
}

// ---------------------------------------------------------------------------------------
// Export

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
    if color.a == 0 && key != "fontColor" {
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
        S::RoundedRectangle => "rounded=1;",
        S::Ellipse => "ellipse;",
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
        S::Process => "shape=mxgraph.flowchart.process;",
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

struct PageWriter<'a> {
    sheet: &'a Sheet,
    out: String,
    layout: thinoda_core::Layout,
    topics: HashSet<TopicId>,
    object_zone: HashMap<CanvasObjectId, ZoneId>,
    root_zone: HashMap<TopicId, ZoneId>,
    connector_midpoints: HashMap<CanvasObjectId, Vec2>,
    groups: HashMap<CanvasGroupId, (String, Rect)>,
    written_groups: HashSet<CanvasGroupId>,
    written: HashSet<BoardMember>,
}

impl<'a> PageWriter<'a> {
    fn new(sheet: &'a Sheet) -> Self {
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        let topics = layout
            .order
            .iter()
            .copied()
            .filter(|id| !sheet.is_anchor(*id))
            .collect();
        let edge_targets = crate::board_export::connector_attachment_counts(sheet);
        let connector_midpoints = if edge_targets.is_empty() {
            HashMap::new()
        } else {
            thinoda_core::build_scene(sheet, &layout)
                .items
                .into_iter()
                .filter_map(|item| match (item.id, item.geometry) {
                    (
                        thinoda_core::SceneId::Object(id),
                        thinoda_core::scene::SceneGeometry::Polyline { points, .. },
                    ) if edge_targets.contains_key(&id) => {
                        thinoda_core::board::polyline_midpoint(&points).map(|point| (id, point))
                    }
                    _ => None,
                })
                .collect()
        };
        let mut writer = Self {
            sheet,
            out: String::new(),
            layout,
            topics,
            object_zone: HashMap::new(),
            root_zone: HashMap::new(),
            connector_midpoints,
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
        writer
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
                if let Some(rounding) = object.rounding {
                    style.push_str(&format!(
                        "shape={};rSize={};",
                        rounded_stencil_name(rounding.stencil),
                        num(rounding.radius)
                    ));
                } else if *shape == ShapeKind::Rectangle
                    && style_ref.fill.is_some_and(|c| c.a == 0)
                    && style_ref.stroke.is_some_and(|c| c.a == 0)
                {
                    style.push_str("text;");
                } else {
                    style.push_str(shape_style(*shape));
                }
                style.push_str("whiteSpace=wrap;html=1;");
                color_keys(&mut style, "fillColor", "fillOpacity", style_ref.fill);
                color_keys(&mut style, "strokeColor", "strokeOpacity", style_ref.stroke);
                color_keys(&mut style, "fontColor", "textOpacity", style_ref.text_color);
                style.push_str(&format!("strokeWidth={};", num(style_ref.stroke_width)));
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
                    connector.color.or(object.style.stroke),
                );
                color_keys(
                    &mut style,
                    "fontColor",
                    "textOpacity",
                    object.style.text_color,
                );
                style.push_str(&format!("strokeWidth={};", num(connector.width)));
                common_keys(&mut style, &object.style, 0.0, object.locked, object.band);
                let ends = [self.terminal(connector.from), self.terminal(connector.to)];
                self.edge(
                    &cell_id,
                    &connector.label,
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
        let image = topic
            .image
            .as_deref()
            .and_then(|reference| self.sheet.resources.get(reference).ok())
            .filter(|resource| matches!(resource.media_type.as_str(), "image/png" | "image/jpeg"));
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
    Ok(export_sheets(&book.sheets).into_bytes())
}

fn export_sheets<'a>(sheets: impl IntoIterator<Item = &'a Sheet>) -> String {
    let mut out = String::from("<mxfile host=\"Thinoda\" type=\"device\">\n");
    for (index, sheet) in sheets.into_iter().enumerate() {
        let grid = if sheet.board.grid == GridKind::None {
            0
        } else {
            1
        };
        out.push_str(&format!(
            "<diagram id=\"thinoda-page-{}\" name=\"{}\">\n<mxGraphModel dx=\"0\" dy=\"0\" grid=\"{grid}\" gridSize=\"{}\" guides=\"1\" tooltips=\"1\" connect=\"1\" arrows=\"1\" fold=\"1\" page=\"0\" pageScale=\"1\" pageWidth=\"850\" pageHeight=\"1100\" math=\"0\" shadow=\"0\">\n<root>\n<mxCell id=\"0\"/>\n<mxCell id=\"1\" parent=\"0\"/>\n",
            index + 1,
            esc(&sheet.title),
            num(sheet.board.grid_size),
        ));
        out.push_str(&PageWriter::new(sheet).write());
        out.push_str("</root>\n</mxGraphModel>\n</diagram>\n");
    }
    out.push_str("</mxfile>\n");
    out
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
    let xml = export_sheets(std::iter::once(sheet));
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
    let xml = export_sheets(std::iter::once(sheet));
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
