//! draw.io / diagrams.net documents (`.drawio`, `.dio`, `.drawio.svg`, `.drawio.png`).
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
    TopicId, TopicShape, Vec2, Workbook, ZoneId,
    board::{
        Band, BoardMember, CanvasConnector, CanvasGroupId, CanvasObject, CanvasObjectId,
        CanvasObjectKind, Card, CardKind, ConnectorRoute, Endpoint, GridKind, InkStroke, InkTool,
        MAX_BOARD_TEXT_CHARS, MAX_GRID_SIZE, MAX_STROKE_WIDTH, MIN_CARD_SIZE, MIN_GRID_SIZE,
        MIN_STROKE_WIDTH, ObjectStyle, ShapeKind, SheetRootMode, normalize_rotation, rotate_around,
    },
    layout_sheet,
};

/// Largest accepted input and inflated diagram text.
const MAX_BYTES: usize = 64 * 1024 * 1024;
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

fn percent_decode(text: &str) -> Option<String> {
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
    let raw = inflate(&data).ok_or_else(|| invalid("draw.io page data cannot be decompressed"))?;
    let text = String::from_utf8(raw).map_err(|_| invalid("draw.io page data is not UTF-8"))?;
    Ok(uri_decode(text))
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Text of the `mxfile` PNG chunk written by draw.io (tEXt, zTXt or iTXt).
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
        if let Some(rest) = data.strip_prefix(b"mxfile\0") {
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
                    let (_, rest) = rest.split_first()?;
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
    let trimmed = content.trim_start();
    if trimmed.starts_with('<') {
        Ok(trimmed.to_owned())
    } else {
        decode_compressed(trimmed)
    }
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
            "svg" => root.attr("content").is_some_and(|content| {
                svg_content(content).is_ok_and(|xml| {
                    let xml = xml.trim_start();
                    xml.starts_with("<mxfile") || xml.starts_with("<mxGraphModel")
                })
            }),
            _ => false,
        },
        None => false,
    }
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
}

impl Style {
    fn parse(text: &str) -> Self {
        let mut style = Style::default();
        for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            match part.split_once('=') {
                Some((key, value)) => {
                    style
                        .map
                        .insert(key.trim().to_owned(), value.trim().to_owned());
                }
                None => style.tokens.push(part.to_owned()),
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
        let mut color = match value {
            "" | "default" | "inherit" => return None,
            "none" => return Some(Color::TRANSPARENT),
            hex => Color::from_hex(hex)?,
        };
        if let Some(opacity) = self.num(opacity_key) {
            color.a = (f32::from(color.a) * (opacity / 100.0).clamp(0.0, 1.0)).round() as u8;
        }
        Some(color)
    }

    fn stroke_width(&self) -> f32 {
        self.num("strokeWidth")
            .unwrap_or(1.0)
            .clamp(MIN_STROKE_WIDTH, MAX_STROKE_WIDTH)
    }

    fn opacity(&self) -> f32 {
        (self.num("opacity").unwrap_or(100.0) / 100.0).clamp(0.0, 1.0)
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

    /// Visual details without a Thinoda equivalent.
    fn formatting_loss(&self, edge: bool) -> bool {
        const FLAGS: [&str; 7] = [
            "dashed", "shadow", "glass", "sketch", "flipH", "flipV", "comic",
        ];
        const ARROWS: [&str; 8] = [
            "",
            "none",
            "classic",
            "classicThin",
            "block",
            "blockThin",
            "open",
            "openThin",
        ];
        let colored = |key: &str| {
            self.get(key)
                .is_some_and(|v| !matches!(v, "" | "none" | "default"))
        };
        FLAGS.iter().any(|key| self.flag(key))
            || colored("gradientColor")
            || colored("labelBackgroundColor")
            || colored("labelBorderColor")
            || self.get("fontFamily").is_some()
            || self.num("fontSize").is_some_and(|v| v != 11.0 && v != 12.0)
            || self.num("fontStyle").is_some_and(|v| v != 0.0)
            || self.get("align").is_some_and(|v| v != "center")
            || self.get("verticalAlign").is_some_and(|v| v != "middle")
            || (edge
                && (self.get("jumpStyle").is_some_and(|v| v != "none")
                    || ["startArrow", "endArrow"]
                        .iter()
                        .any(|key| self.get(key).is_some_and(|v| !ARROWS.contains(&v)))))
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

fn read_geometry(element: Option<&Element>) -> Geometry {
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
                    .map(point)
                    .collect();
            }
            _ => {}
        }
    }
    geometry
}

fn read_cell(outer: &Element, inner: &Element, user: Option<&Element>) -> Cell {
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
        geometry: read_geometry(inner.child("mxGeometry")),
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
            "mxCell" => read_cell(element, element, None),
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
                read_cell(element, inner, Some(element))
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
    Text,
    Line,
    Image,
    Card(CardKind),
    Unknown,
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
    let name = style.shape();
    if name == "swimlane" || style.has("swimlane") || style.flag("container") {
        return Kind::Container;
    }
    if style.get("image").is_some_and(|v| !v.is_empty()) {
        return Kind::Image;
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
        "mxgraph.flowchart.delay" => shape(S::Delay, 0.0),
        "mxgraph.flowchart.off-page_reference" | "offPageConnector" => {
            shape(S::OffPageConnector, 0.0)
        }
        _ => Kind::Unknown,
    }
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
        text_color: style.color("fontColor", "textOpacity"),
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
        if cell.style.formatting_loss(false) {
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
            Kind::Shape(shape, base, symmetric) => {
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
                shape_object(shape, frame, total + style.rotation(), label)
            }
            Kind::Text => {
                let mut object = shape_object(ShapeKind::Rectangle, rect, style.rotation(), label);
                object.style.fill = Some(object.style.fill.unwrap_or(Color::TRANSPARENT));
                object.style.stroke = Some(object.style.stroke.unwrap_or(Color::TRANSPARENT));
                object
            }
            Kind::Image => {
                self.warn(ImportWarning::Resources);
                shape_object(ShapeKind::Rectangle, rect, style.rotation(), label)
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
                object.style.opacity = style.opacity();
                object.band = style.band();
                object
            }
            Kind::Card(card_kind) => return self.card(index, card_kind, rect, label, context),
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
        card.color = style.color("fillColor", "fillOpacity").filter(|c| c.a > 0);
        if cell.tooltip.is_some() {
            self.warn(ImportWarning::Other);
        }
        match self.sheet.add_card(&label, rect.center(), card) {
            Ok(id) => {
                if let Some(topic) = self.sheet.topic_mut(id) {
                    topic.style.fill = card.color;
                    topic.style.text_color = style.color("fontColor", "textOpacity");
                    topic.hyperlink = cell.link.clone();
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
        if style.formatting_loss(true) || !cell.geometry.points.is_empty() {
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
        connector.arrow_end = style.get("endArrow") != Some("none");
        connector.arrow_start = style
            .get("startArrow")
            .is_some_and(|v| !v.is_empty() && v != "none");
        connector.label = truncate_chars(label, MAX_BOARD_TEXT_CHARS, self.warnings);
        connector.color = style.color("strokeColor", "strokeOpacity");
        connector.width = style.stroke_width();
        let mut object = CanvasObject::connector(connector);
        object.style.text_color = style.color("fontColor", "textOpacity");
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
    if color.a == 0 {
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
        let mut writer = Self {
            sheet,
            out: String::new(),
            layout,
            topics,
            object_zone: HashMap::new(),
            root_zone: HashMap::new(),
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
                if *shape == ShapeKind::Rectangle
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
                    ConnectorRoute::Straight => {}
                    ConnectorRoute::Elbow => style.push_str(
                        "edgeStyle=orthogonalEdgeStyle;rounded=0;orthogonalLoop=1;jettySize=auto;",
                    ),
                    ConnectorRoute::Curve => style.push_str("curved=1;"),
                }
                style.push_str(if connector.arrow_end {
                    "endArrow=classic;"
                } else {
                    "endArrow=none;"
                });
                if connector.arrow_start {
                    style.push_str("startArrow=classic;");
                }
                color_keys(&mut style, "strokeColor", "strokeOpacity", connector.color);
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
                    &[],
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
        let mut style = String::from(match card.kind {
            CardKind::Sticky => "shape=note;thinodaCard=sticky;backgroundOutline=1;size=14;",
            CardKind::Text => "text;thinodaCard=text;",
            CardKind::Image => "shape=note;thinodaCard=image;backgroundOutline=1;size=14;",
        });
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
    let mut out = String::from("<mxfile host=\"Thinoda\" type=\"device\">\n");
    for (index, sheet) in book.sheets.iter().enumerate() {
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
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests;
