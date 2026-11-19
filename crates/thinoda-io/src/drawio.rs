//! draw.io / diagrams.net documents and embedded SVG, PNG and HTML diagrams.
//!
//! Import converts every page into a Freeform board sheet: vertices become shapes,
//! notes become cards, flat swimlanes and containers become Zones, nested
//! containers become scene frames, groups become board groups and edges become
//! free connectors. Content without a native equivalent is approximated and
//! reported through [`ImportWarning`] categories.
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
use std::sync::Arc;
use thinoda_core::drawio_passthrough::{
    DrawioCellPassthrough as Passthrough, DrawioDocumentPassthrough, DrawioPagePassthrough,
    RESERVED_DOCUMENT_ATTRIBUTES, RESERVED_MODEL_ATTRIBUTES,
};
use thinoda_core::object_data::ObjectData;
use thinoda_core::{
    ApproxMeasure, Color, MAX_PRESENTATION_NAME_BYTES, MAX_ZONES, MIN_ZONE_SIZE, Rect, Sheet,
    TextAlign, TopicId, TopicShape, Vec2, Workbook, ZoneId,
    board::{
        Band, BoardMember, CANVAS_LABEL_FONT_SIZE, CanvasConnector, CanvasGroupId, CanvasObject,
        CanvasObjectId, CanvasObjectKind, Card, CardKind, ConnectorLabel, ConnectorLabelPlacement,
        ConnectorMarker, ConnectorMarkerKind, ConnectorPort, ConnectorRoute,
        DEFAULT_CONNECTOR_MARKER_SIZE, DEFAULT_LINE_JUMP_SIZE, DrawioRectangleStencil, EdgeShape,
        EdgeShapeKind, Endpoint, GridKind, InkStroke, InkTool, LineJumpStyle, LineJumps,
        MAX_BOARD_COORDINATE, MAX_BOARD_TEXT_CHARS, MAX_CONNECTOR_LABEL_OFFSET,
        MAX_CONNECTOR_LABELS, MAX_GRID_SIZE, MAX_STROKE_WIDTH, MIN_CARD_SIZE, MIN_GRID_SIZE,
        MIN_STROKE_WIDTH, ObjectStyle, RectangleSides, ShapeKind, ShapeRounding, ShapeRoundingMode,
        SheetRootMode, normalize_rotation, rotate_around, valid_board_position,
        valid_connector_marker_size, valid_edge_shape_width, valid_line_jump_size,
    },
    images::{self, MAX_DISPLAY_SIZE},
    label_style::{LabelStyle, LabelVAlign},
    layout_sheet,
    resources::{MAX_RESOURCE_BYTES, Resource},
};

mod html;
mod label_html;

use label_html::{parse_rich_html, strip_html};

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

/// Ownership table, revision 1 (architecture §3.1.1): style keys that first-class
/// fields represent or that import consumes. They are never kept as passthrough,
/// so a cleared field cannot be resurrected by its source value. Every `thinoda*`
/// key is owned as well.
const OWNED_STYLE_KEYS: &[&str] = &[
    "absoluteArcSize",
    "allowGaps",
    "anchorPointDirection",
    "arcSize",
    "bottom",
    "childLayout",
    "collapsible",
    "colspan",
    "container",
    "curved",
    "direction",
    "edgeStyle",
    "elbow",
    "endArrow",
    "endFill",
    "endSize",
    "entryDx",
    "entryDy",
    "entryPerimeter",
    "entryX",
    "entryY",
    "evenColumnColor",
    "evenRowColor",
    "exitDx",
    "exitDy",
    "exitPerimeter",
    "exitX",
    "exitY",
    "fill",
    "fillColor",
    "fillOpacity",
    "fontColor",
    "horizontal",
    "horizontalStack",
    "html",
    "image",
    "imageAspect",
    "jumpSize",
    "jumpStyle",
    "left",
    "locked",
    "margin",
    "marginBottom",
    "marginLeft",
    "marginRight",
    "marginTop",
    "oddColumnColor",
    "oddRowColor",
    "opacity",
    "perimeter",
    "perimeterSpacing",
    "resizeLast",
    "resizeParent",
    "resizeParentMax",
    "right",
    "rotation",
    "rounded",
    "roundedPerimeter",
    "rowspan",
    "rSize",
    "shape",
    "size",
    "stackBorder",
    "stackSpacing",
    "startArrow",
    "startFill",
    "startSize",
    "strokeColor",
    "strokeOpacity",
    "strokeWidth",
    "swimlaneFillColor",
    "textOpacity",
    "top",
    "whiteSpace",
];

/// Edge-shape parameters Thinoda owns on connectors; vertices keep them as
/// passthrough because stencils may bind them.
const EDGE_SHAPE_STYLE_KEYS: &[&str] = &["width", "startWidth", "endWidth"];

/// Shape style effects Thinoda owns on imported built-in and stencil shapes;
/// other cells keep them as passthrough.
const SHAPE_EFFECT_STYLE_KEYS: &[&str] = &[
    "dashed",
    "dashPattern",
    "fixDash",
    "gradientColor",
    "gradientDirection",
    "shadow",
    "glass",
    "fillStyle",
    "flipH",
    "flipV",
];

/// Passthrough keys that only mean something next to another key; dropped on
/// export when that key is absent, so an edit cannot leave an orphan behind.
const DEPENDENT_STYLE_KEYS: &[(&str, &str)] = &[
    ("dashPattern", "dashed"),
    ("fixDash", "dashed"),
    ("jettySize", "edgeStyle"),
    ("orthogonalLoop", "edgeStyle"),
    ("gradientDirection", "gradientColor"),
];

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
            .find(|(k, _)| k == key || k.rsplit(':').next() == Some(key))
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
        element
            .attrs
            .push((attr.key.as_ref().to_owned(), value.into_owned()));
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
    id: Option<String>,
    dropped_id: bool,
    dropped_content: bool,
    model: Element,
}

struct DrawioDocument {
    pages: Vec<Page>,
    passthrough: Option<Arc<DrawioDocumentPassthrough>>,
    dropped_attributes: bool,
    dropped_content: bool,
}

fn read_pages(bytes: &[u8]) -> Result<DrawioDocument, IoError> {
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
        "mxGraphModel" => Ok(DrawioDocument {
            pages: vec![Page {
                name: String::new(),
                id: None,
                dropped_id: false,
                dropped_content: false,
                model: root,
            }],
            passthrough: None,
            dropped_attributes: false,
            dropped_content: false,
        }),
        "mxfile" => {
            let dropped_content = !root.text.trim().is_empty()
                || root.children.iter().any(|child| child.name != "diagram");
            let mut dropped_attributes = false;
            let mut passthrough = DrawioDocumentPassthrough::default();
            let mut seen = HashSet::new();
            let mut bytes = 0usize;
            for (name, value) in &root.attrs {
                if RESERVED_DOCUMENT_ATTRIBUTES.contains(&name.as_str()) {
                    continue;
                }
                let valid = thinoda_core::drawio_passthrough::valid_attribute_name(name)
                    && !value
                        .chars()
                        .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
                    && seen.insert(name.as_str());
                let next = bytes.saturating_add(name.len()).saturating_add(value.len());
                if !valid || next > thinoda_core::drawio_passthrough::MAX_DOCUMENT_PASSTHROUGH_BYTES
                {
                    dropped_attributes = true;
                    continue;
                }
                bytes = next;
                passthrough.attributes.push((name.clone(), value.clone()));
            }
            debug_assert!(passthrough.validate().is_ok());
            let passthrough = (!passthrough.is_empty()).then(|| Arc::new(passthrough));
            let mut pages = Vec::new();
            for mut diagram in root.children.into_iter().filter(|c| c.name == "diagram") {
                if pages.len() >= MAX_PAGES {
                    return Err(invalid("draw.io document has too many pages"));
                }
                let name = diagram.attr("name").unwrap_or_default().to_owned();
                let id = diagram
                    .attr("id")
                    .filter(|id| {
                        !id.is_empty()
                            && id.len() <= thinoda_core::drawio_passthrough::MAX_SOURCE_ID_BYTES
                            && !id.chars().any(char::is_control)
                    })
                    .map(str::to_owned);
                let dropped_id = diagram.attr("id").is_some() && id.is_none();
                let mut dropped_content = diagram
                    .attrs
                    .iter()
                    .any(|(name, _)| !matches!(name.as_str(), "id" | "name"));
                let model = match diagram
                    .children
                    .iter()
                    .position(|c| c.name == "mxGraphModel")
                {
                    Some(index) => {
                        dropped_content |=
                            diagram.children.len() != 1 || !diagram.text.trim().is_empty();
                        diagram.children.swap_remove(index)
                    }
                    None if diagram.text.trim().is_empty() => {
                        dropped_content |= !diagram.children.is_empty();
                        Element {
                            name: "mxGraphModel".into(),
                            ..Element::default()
                        }
                    }
                    None => {
                        dropped_content |= !diagram.children.is_empty();
                        let model = parse_dom(&decode_compressed(diagram.text.trim())?)?;
                        if model.name != "mxGraphModel" {
                            return Err(invalid("draw.io page has no graph model"));
                        }
                        model
                    }
                };
                pages.push(Page {
                    name,
                    id,
                    dropped_id,
                    dropped_content,
                    model,
                });
            }
            if pages.is_empty() {
                return Err(invalid("draw.io document contains no pages"));
            }
            Ok(DrawioDocument {
                pages,
                passthrough,
                dropped_attributes,
                dropped_content,
            })
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

#[derive(Debug, Default, Clone)]
struct Style {
    tokens: Vec<String>,
    map: HashMap<String, String>,
    /// Explicit `key=value` keys in source order, for passthrough.
    order: Vec<String>,
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
                    let key = key.trim();
                    if !style.map.contains_key(key) {
                        style.order.push(key.to_owned());
                    }
                    style.map.insert(key.to_owned(), value.trim().to_owned());
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

    /// A stencil reference: an explicit Thinoda reference, otherwise a plain
    /// draw.io shape that no native kind draws exactly but a built-in stencil
    /// package (or the cell's inline `stencil(...)`) defines as data. Bundled
    /// draw.io families only draw shapes without any native kind, so an
    /// approximate native kind stays editable as that kind.
    fn stencil(&self) -> Option<thinoda_core::stencil_object::StencilRef> {
        let explicit = self.get(crate::drawio_stencil::STYLE_PACKAGE_KEY).is_some();
        let implicit = !explicit
            && match classify_native(self) {
                Kind::Approx(..) => crate::drawio_stencil::maps_builtin_drawio_shape(self.shape()),
                // A bare token is a named-style lookup, never a bundled shape.
                Kind::Unknown if self.get("shape").is_some() => {
                    crate::drawio_stencil::maps_drawio_shape(self.shape())
                }
                Kind::Unknown => crate::drawio_stencil::maps_builtin_drawio_shape(self.shape()),
                _ => false,
            };
        if !explicit && !implicit {
            return None;
        }
        crate::drawio_stencil::style_reference(
            self.map.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            self.shape(),
            implicit,
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

    fn scene_layout_policy(
        &self,
        header: thinoda_core::scene_layout::HeaderPolicy,
    ) -> (
        Option<thinoda_core::scene_layout::ContainerLayoutPolicy>,
        bool,
    ) {
        use thinoda_core::scene_layout::{
            ContainerLayout, ContainerLayoutPolicy, StackAxis, StackCrossAxisPolicy,
            StackResizePolicy,
        };

        if self.get("childLayout") != Some("stackLayout") {
            return (None, self.get("childLayout").is_some());
        }

        let axis = match self.get("horizontalStack").unwrap_or("1") {
            "1" => StackAxis::Horizontal,
            "0" => StackAxis::Vertical,
            _ => return (None, true),
        };
        let number = |key: &str, default: f32| match self.get(key) {
            None => Some(default),
            Some(_) => self.num(key).filter(|value| *value >= 0.0),
        };
        let (Some(spacing), Some(border), Some(left), Some(right), Some(top), Some(bottom)) = (
            number("stackSpacing", 0.0),
            number("stackBorder", 0.0),
            number("marginLeft", 0.0),
            number("marginRight", 0.0),
            number("marginTop", 0.0),
            number("marginBottom", 0.0),
        ) else {
            return (None, true);
        };
        let flag = |key: &str, default: bool| match self.get(key) {
            None => Some(default),
            Some("1") => Some(true),
            Some("0") => Some(false),
            Some(_) => None,
        };
        let (
            Some(fill),
            Some(resize_parent),
            Some(resize_parent_max),
            Some(resize_last),
            Some(allow_gaps),
        ) = (
            flag("fill", true),
            flag("resizeParent", true),
            flag("resizeParentMax", true),
            flag("resizeLast", false),
            flag("allowGaps", false),
        )
        else {
            return (None, true);
        };

        let [left, right, top, bottom] =
            [border + left, border + right, border + top, border + bottom];
        if ![left, top, right, bottom]
            .iter()
            .all(|value| *value == left)
        {
            return (None, true);
        }

        let resize = if resize_parent {
            if resize_parent_max {
                StackResizePolicy::GrowParent
            } else {
                StackResizePolicy::FitParent
            }
        } else if resize_last {
            StackResizePolicy::FillLast
        } else {
            StackResizePolicy::Preserve
        };

        if allow_gaps {
            return (None, true);
        }

        (
            Some(ContainerLayoutPolicy {
                header,
                layout: ContainerLayout::Stack {
                    axis,
                    spacing,
                    padding: left,
                    cross_axis: if fill {
                        StackCrossAxisPolicy::Fill
                    } else {
                        StackCrossAxisPolicy::Preserve
                    },
                    resize,
                },
            }),
            false,
        )
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

    fn label_style_loss(&self, source_is_edge: bool, label_text: bool) -> bool {
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
            // Mapped onto the shape's label style; invalid sizes warn there.
            _ if label_text => false,
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
        self.get("overflow")
            .is_some_and(|value| !matches!(value, "" | "none" | "visible"))
            || self.flag("noLabel")
            || self
                .get("textDirection")
                .is_some_and(|value| !matches!(value, "" | "none" | "default"))
            || self
                .get("labelWidth")
                .is_some_and(|value| !matches!(value, "" | "none"))
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
        let kind = drawio_marker_kind(name)?;
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
    /// `shape_effects` marks cells whose dash, gradient, shadow, glass and
    /// flips map to native shape style (see [`shape_effects`]); `label_text`
    /// marks cells whose label text keys map to a label style (see
    /// [`label_text_style`]).
    fn formatting_loss(
        &self,
        edge: bool,
        card_text_alignment: bool,
        shape_effects: bool,
        label_text: bool,
    ) -> bool {
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
            || FLAGS.iter().any(|key| {
                self.flag(key)
                    && !(*key == "dashed" && self.flag("thinodaSankeyEdge"))
                    && !(shape_effects && SHAPE_EFFECT_STYLE_KEYS.contains(key))
            })
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
            || (!shape_effects && colored("gradientColor"))
            || (label_text
                && (self
                    .get("overflow")
                    .is_some_and(|value| !matches!(value, "" | "none" | "visible"))
                    || self
                        .get("textDirection")
                        .is_some_and(|value| !matches!(value, "" | "none" | "default"))))
            || (!label_text
                && (colored("labelBackgroundColor")
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
                        v != "middle"
                            && !(v == "bottom" && self.get("thinodaCard") == Some("image"))
                    })))
            || self.get("portConstraint").is_some()
            || differs_from("portConstraintRotation", 0.0)
            || differs_from("perimeterSpacing", 0.0)
            || (edge
                && (self.line_jump_loss()
                    || ["startArrow", "endArrow"].iter().any(|key| {
                        self.get(key).is_some_and(|v| {
                            !matches!(v, "" | "none") && drawio_marker_kind(v).is_none()
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

impl Style {
    /// The edge `shape` with its parameters, and whether any part of it is
    /// lost: an unknown shape, or a width Thinoda cannot represent.
    fn edge_shape(&self) -> (Option<EdgeShape>, bool) {
        let Some(name) = self.get("shape").filter(|name| !name.is_empty()) else {
            return (None, false);
        };
        if name == "connector" {
            return (None, false);
        }
        let Some(kind) = EdgeShapeKind::from_drawio(name) else {
            return (None, true);
        };
        let mut loss = false;
        let mut width = |key: &str, allowed: bool| {
            if !allowed || self.get(key).is_none() {
                return None;
            }
            let value = self.num(key).filter(|value| valid_edge_shape_width(*value));
            loss |= value.is_none();
            value
        };
        let mut shape = EdgeShape::new(kind);
        shape.width = width("width", kind.has_width());
        shape.start_width = width("startWidth", kind.has_end_widths());
        shape.end_width = width("endWidth", kind.has_end_widths());
        shape.fill = kind
            .has_fill()
            .then(|| self.color("fillColor", "fillOpacity"))
            .flatten()
            .filter(|fill| fill.a > 0);
        (Some(shape), loss)
    }

    fn line_jumps(&self) -> Option<LineJumps> {
        let style = match self.get("jumpStyle")? {
            "arc" => LineJumpStyle::Arc,
            "gap" => LineJumpStyle::Gap,
            "sharp" => LineJumpStyle::Sharp,
            "line" => LineJumpStyle::Line,
            _ => return None,
        };
        let mut jumps = LineJumps::new(style);
        if self.get("jumpSize").is_some() {
            // draw.io reads the size with parseInt.
            jumps.size = self.num("jumpSize")?.trunc();
            if !valid_line_jump_size(jumps.size) {
                return None;
            }
        }
        Some(jumps)
    }

    fn line_jump_loss(&self) -> bool {
        let styled = self.get("jumpStyle").is_some_and(|v| v != "none");
        (styled && self.line_jumps().is_none())
            || self.flag("noJump")
            || self.get("jumpLayers").is_some()
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
    alternate_bounds: Option<Rect>,
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
    collapsed: bool,
    source: Option<String>,
    target: Option<String>,
    visible: bool,
    has_geometry: bool,
    geometry: Geometry,
    link: Option<String>,
    tooltip: Option<String>,
    /// `UserObject` attributes without a first-class field, including `link`.
    attributes: Vec<(String, String)>,
}

/// Whether an edge-label child looks different from the connector's own label,
/// which is how Thinoda draws every connector label.
fn child_label_style_loss(child: &Style, edge: &Style) -> bool {
    child.suppress_defaults
        || child.tokens.iter().any(|token| token != "edgeLabel")
        || child.num("fontSize") != Some(CANVAS_LABEL_FONT_SIZE)
        || child.text_color() != edge.text_color()
        || child.get("labelBackgroundColor") == Some("none")
        || child.map.iter().any(|(key, value)| !match key.as_str() {
            "html"
            | "resizable"
            | "points"
            | "fontSize"
            | "fontColor"
            | "textOpacity"
            | "labelBackgroundColor" => true,
            "align" => matches!(value.as_str(), "" | "center"),
            "verticalAlign" => matches!(value.as_str(), "" | "middle"),
            "fontStyle" => matches!(value.as_str(), "" | "0"),
            _ => false,
        })
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

fn has_extra_xml(element: &Element, allowed_attributes: &[&str], allow_children: bool) -> bool {
    element
        .attrs
        .iter()
        .any(|(name, _)| !allowed_attributes.contains(&name.as_str()))
        || !element.text.trim().is_empty()
        || (!allow_children && !element.children.is_empty())
}

fn read_geometry(element: Option<&Element>, warnings: &mut BTreeSet<ImportWarning>) -> Geometry {
    let Some(element) = element else {
        return Geometry::default();
    };
    if has_extra_xml(
        element,
        &["x", "y", "width", "height", "relative", "as"],
        true,
    ) || element.attr("as").is_some_and(|value| value != "geometry")
        || element
            .attr("relative")
            .is_some_and(|value| !matches!(value, "0" | "1"))
    {
        warnings.insert(ImportWarning::Other);
    }
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
    let mut seen = HashSet::new();
    for child in &element.children {
        if let Some(role) = child.attr("as")
            && matches!(
                role,
                "sourcePoint" | "targetPoint" | "offset" | "alternateBounds" | "points"
            )
            && !seen.insert(role)
        {
            warnings.insert(ImportWarning::Other);
        }
        match (child.name.as_str(), child.attr("as")) {
            ("mxPoint", Some(role @ ("sourcePoint" | "targetPoint" | "offset"))) => {
                if has_extra_xml(child, &["as", "x", "y"], false) {
                    warnings.insert(ImportWarning::Other);
                }
                let point = point(child, warnings);
                match role {
                    "sourcePoint" => geometry.source = point,
                    "targetPoint" => geometry.target = point,
                    _ => geometry.offset = point,
                }
            }
            ("mxRectangle", Some("alternateBounds")) => {
                if has_extra_xml(child, &["as", "x", "y", "width", "height"], false) {
                    warnings.insert(ImportWarning::Other);
                }
                match (
                    number(child, "x"),
                    number(child, "y"),
                    number(child, "width"),
                    number(child, "height"),
                ) {
                    (Some(x), Some(y), Some(width), Some(height))
                        if width >= 0.0 && height >= 0.0 =>
                    {
                        geometry.alternate_bounds = Some(Rect::from_min_size(
                            Vec2::new(x, y),
                            Vec2::new(width, height),
                        ));
                    }
                    _ => {
                        warnings.insert(ImportWarning::Other);
                    }
                }
            }
            ("Array", Some("points")) => {
                if has_extra_xml(child, &["as"], true)
                    || child.children.iter().any(|point| {
                        point.name != "mxPoint" || has_extra_xml(point, &["x", "y"], false)
                    })
                {
                    warnings.insert(ImportWarning::Other);
                }
                geometry.points = child
                    .children
                    .iter()
                    .filter(|p| p.name == "mxPoint")
                    .filter_map(|p| point(p, warnings))
                    .collect();
            }
            _ => {
                warnings.insert(ImportWarning::Other);
            }
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
    let style = Style::parse(inner.attr("style").unwrap_or_default());
    if has_extra_xml(
        inner,
        &[
            "id",
            "parent",
            "value",
            "style",
            "vertex",
            "edge",
            "collapsed",
            "source",
            "target",
            "visible",
            "connectable",
        ],
        true,
    ) || inner
        .children
        .iter()
        .filter(|child| child.name == "mxGeometry")
        .count()
        > 1
        || inner
            .children
            .iter()
            .any(|child| child.name != "mxGeometry")
        || ["vertex", "edge", "visible"].iter().any(|key| {
            inner
                .attr(key)
                .is_some_and(|value| !matches!(value, "0" | "1"))
        })
        || inner.attr("connectable").is_some_and(|value| {
            value != "1" && !(value == "0" && style.tokens.iter().any(|token| token == "edgeLabel"))
        })
        || user.is_some_and(|user| {
            user.children.len() != 1
                || !user.text.trim().is_empty()
                || inner.attr("id").is_some()
                || inner.attr("value").is_some()
        })
    {
        warnings.insert(ImportWarning::Other);
    }
    let collapsed = match inner.attr("collapsed") {
        None | Some("0") => false,
        Some("1") => true,
        Some(_) => {
            warnings.insert(ImportWarning::Other);
            false
        }
    };
    Cell {
        id: outer.attr("id").unwrap_or_default().to_owned(),
        parent: owned(inner.attr("parent")),
        value: match user {
            Some(user) => user.attr("label"),
            None => inner.attr("value"),
        }
        .unwrap_or_default()
        .to_owned(),
        style,
        vertex: inner.attr("vertex") == Some("1"),
        edge: inner.attr("edge") == Some("1"),
        collapsed,
        source: owned(inner.attr("source")),
        target: owned(inner.attr("target")),
        visible: inner.attr("visible") != Some("0"),
        has_geometry: inner.child("mxGeometry").is_some(),
        geometry: read_geometry(inner.child("mxGeometry"), warnings),
        link: owned(user.and_then(|u| u.attr("link"))),
        tooltip: owned(user.and_then(|u| u.attr("tooltip"))),
        attributes: Vec::new(),
    }
}

fn structural_cell_has_unrepresented_data(cell: &Cell, layer: bool) -> bool {
    cell.has_geometry
        || cell.collapsed
        || cell.source.is_some()
        || cell.target.is_some()
        || cell.link.is_some()
        || cell.tooltip.is_some()
        || !cell.attributes.is_empty()
        || !cell.style.tokens.is_empty()
        || cell.style.suppress_defaults
        || cell
            .style
            .map
            .iter()
            .any(|(key, value)| !layer || key != "locked" || !matches!(value.as_str(), "0" | "1"))
}

/// Moves link, tags, placeholders and custom properties of a `UserObject` into
/// object data; attributes data cannot hold stay as passthrough, and so does
/// everything when the data exceeds its bounds.
fn split_object_data(
    attributes: &[(String, String)],
) -> (Option<Arc<ObjectData>>, Vec<(String, String)>) {
    let mut data = ObjectData::default();
    let mut rest = Vec::new();
    for (name, value) in attributes {
        match name.as_str() {
            "link" if !value.trim().is_empty() => data.link = Some(value.clone()),
            "link" => {}
            "tags" => data.tags = ObjectData::parse_tags(value),
            "placeholders" => data.placeholders = value == "1",
            _ if thinoda_core::object_data::valid_property_name(name) => {
                data.properties.push((name.clone(), value.clone()));
            }
            _ => rest.push((name.clone(), value.clone())),
        }
    }
    if data.is_empty() {
        return (None, rest);
    }
    if data.validate().is_err() {
        return (None, attributes.to_vec());
    }
    (Some(Arc::new(data)), rest)
}

/// `UserObject` attributes written from object data, in import order.
fn data_attributes(data: &ObjectData) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    if let Some(link) = &data.link {
        attributes.push(("link".to_owned(), link.clone()));
    }
    if !data.tags.is_empty() {
        attributes.push(("tags".to_owned(), data.tags_text()));
    }
    if data.placeholders {
        attributes.push(("placeholders".to_owned(), "1".to_owned()));
    }
    attributes.extend(data.properties.iter().cloned());
    attributes
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
                        if ["label", "tooltip"].iter().any(|name| {
                            element.attr(name).is_some_and(
                                thinoda_core::object_data::uses_unresolved_drawio_variables,
                            )
                        }) {
                            warnings.insert(ImportWarning::DrawioPlaceholdersUnsupported);
                        }
                    }
                    None | Some("0") => {}
                    Some(_) => {
                        warnings.insert(ImportWarning::Other);
                    }
                }
                if element.attr("placeholder").is_some() {
                    warnings.insert(ImportWarning::DrawioPlaceholdersUnsupported);
                }
                let attributes: Vec<(String, String)> = element
                    .attrs
                    .iter()
                    .filter(|(key, _)| {
                        !thinoda_core::drawio_passthrough::RESERVED_ATTRIBUTES
                            .contains(&key.as_str())
                    })
                    .cloned()
                    .collect();
                if attributes.iter().any(|(key, value)| {
                    !thinoda_core::drawio_passthrough::valid_attribute_name(key)
                        || value
                            .chars()
                            .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
                }) {
                    warnings.insert(ImportWarning::Other);
                }
                let mut cell = read_cell(element, inner, Some(element), warnings);
                cell.attributes = attributes
                    .into_iter()
                    .filter(|(key, value)| {
                        thinoda_core::drawio_passthrough::valid_attribute_name(key)
                            && !value
                                .chars()
                                .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
                    })
                    .collect();
                cell
            }
            _ => {
                warnings.insert(ImportWarning::Other);
                continue;
            }
        };
        if cell.id.is_empty() {
            warnings.insert(ImportWarning::Other);
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
        ..Default::default()
    }
}

/// Maps draw.io's shape effects onto `object`, returning the keys it owns and
/// whether anything was only approximated. `swap_flips` exchanges `flipH` and
/// `flipV` for frames turned a quarter by `direction` (as draw.io's `mxShape`).
fn shape_effects(
    style: &Style,
    object: &mut CanvasObject,
    swap_flips: bool,
) -> (Vec<&'static str>, bool) {
    use thinoda_core::shape_style::{DashPattern, GradientDirection};
    let mut owned = vec!["dashed", "shadow", "glass", "flipH", "flipV"];
    let mut approximate = false;
    if style.flag("dashed") {
        let pattern = match style.get("dashPattern") {
            None | Some("") => Some(DashPattern::default()),
            Some(text) => DashPattern::parse(text),
        };
        if let Some(pattern) = pattern {
            object.style.dash = Some(thinoda_core::board::ShapeDash {
                pattern,
                fixed: style.flag("fixDash"),
            });
            owned.extend(["dashPattern", "fixDash"]);
        } else {
            approximate = true;
            object.style.dash = Some(thinoda_core::board::ShapeDash {
                fixed: style.flag("fixDash"),
                ..Default::default()
            });
            owned.push("fixDash");
        }
    }
    match style.get("gradientColor") {
        None | Some("" | "none") => owned.extend(["gradientColor", "gradientDirection"]),
        Some(value) => {
            approximate |= parse_style_color(value).is_some_and(|parsed| parsed.approximate);
            let direction = match style.get("gradientDirection") {
                None | Some("") => Some(GradientDirection::South),
                Some(name) => GradientDirection::from_drawio(name),
            };
            match (style.color("gradientColor", "fillOpacity"), direction) {
                (Some(color), Some(direction)) => {
                    object.style.gradient =
                        Some(thinoda_core::board::ShapeGradient { color, direction });
                    owned.extend(["gradientColor", "gradientDirection"]);
                }
                _ => approximate = true,
            }
        }
    }
    match style.get("fillStyle") {
        None | Some("") => owned.push("fillStyle"),
        Some(name) => match thinoda_core::board::FillPattern::from_drawio(name) {
            Some((pattern, approximated)) => {
                object.style.fill_pattern = pattern;
                approximate |= approximated;
                if !approximated {
                    owned.push("fillStyle");
                }
            }
            None => approximate = true,
        },
    }
    object.style.shadow = style.flag("shadow");
    object.style.glass = style.flag("glass");
    let (flip_h, flip_v) = (style.flag("flipH"), style.flag("flipV"));
    (object.style.flip_h, object.style.flip_v) = if swap_flips {
        (flip_v, flip_h)
    } else {
        (flip_h, flip_v)
    };
    (owned, approximate)
}

/// Whether a vertex's label text keys map to its shape's label style.
fn maps_label_text(kind: Kind, style: &Style) -> bool {
    matches!(kind, Kind::Shape(..) | Kind::Approx(..) | Kind::Text)
        && style.get("childLayout") != Some("tableLayout")
}

/// Whether `horizontal` turns a header rather than the label of this cell.
fn header_orientation(style: &Style) -> bool {
    matches!(style.shape(), "swimlane" | "table")
        || style.get("childLayout").is_some()
        || style.flag("container")
        || style.get("startSize").is_some()
}

/// Maps draw.io's label text keys onto a shape label style, returning the
/// style (when anything differs from the default), the keys it owns and
/// whether a value was only approximated.
fn label_text_style(style: &Style) -> (Option<LabelStyle>, Vec<&'static str>, bool) {
    use thinoda_core::label_style::{
        MAX_LABEL_FONT_FAMILY_BYTES, MAX_LABEL_FONT_SIZE, MAX_LABEL_SPACING, MIN_LABEL_FONT_SIZE,
    };
    let mut label = LabelStyle::default();
    let mut owned = Vec::new();
    let mut approximate = false;
    let present = |key: &str| {
        style
            .get(key)
            .filter(|value| !matches!(*value, "" | "none"))
    };
    let mut accept = |key: &'static str, ok: bool| {
        if ok {
            owned.push(key);
        } else {
            approximate = true;
        }
    };
    match present("fontFamily") {
        None => accept("fontFamily", true),
        Some(family) => {
            let family = family.trim();
            let ok = !family.is_empty()
                && family.len() <= MAX_LABEL_FONT_FAMILY_BYTES
                && !family.chars().any(char::is_control);
            if ok {
                label.font_family = Some(family.to_owned());
            }
            accept("fontFamily", ok);
        }
    }
    match present("fontSize") {
        None => {
            // Without the default stylesheet draw.io falls back to mxGraph's 11px.
            if style.suppress_defaults && style.get("fontSize").is_none() {
                label.font_size = Some(11.0);
            }
            accept("fontSize", true);
        }
        Some(_) => {
            let size = style
                .num("fontSize")
                .filter(|size| (MIN_LABEL_FONT_SIZE..=MAX_LABEL_FONT_SIZE).contains(size));
            if let Some(size) = size.filter(|size| *size != CANVAS_LABEL_FONT_SIZE) {
                label.font_size = Some(size);
            }
            accept("fontSize", size.is_some());
        }
    }
    match present("fontStyle") {
        None => accept("fontStyle", true),
        Some(_) => {
            let bits = style
                .num("fontStyle")
                .filter(|bits| *bits >= 0.0 && bits.fract() == 0.0 && *bits <= 15.0);
            if let Some(bits) = bits {
                label.set_font_style_bits(bits as u32);
            }
            accept("fontStyle", bits.is_some());
        }
    }
    let horizontal = |value: Option<&str>| match value {
        None => Some(TextAlign::Center),
        Some("left") => Some(TextAlign::Left),
        Some("center") => Some(TextAlign::Center),
        Some("right") => Some(TextAlign::Right),
        Some(_) => None,
    };
    let vertical = |value: Option<&str>| match value {
        None => Some(LabelVAlign::Middle),
        Some("top") => Some(LabelVAlign::Top),
        Some("middle") => Some(LabelVAlign::Middle),
        Some("bottom") => Some(LabelVAlign::Bottom),
        Some(_) => None,
    };
    for (key, target) in [
        ("align", &mut label.align),
        ("labelPosition", &mut label.position),
    ] {
        let value = horizontal(present(key));
        if let Some(value) = value {
            *target = value;
        }
        accept(key, value.is_some());
    }
    for (key, target) in [
        ("verticalAlign", &mut label.vertical_align),
        ("verticalLabelPosition", &mut label.vertical_position),
    ] {
        let value = vertical(present(key));
        if let Some(value) = value {
            *target = value;
        }
        accept(key, value.is_some());
    }
    for (key, target) in [
        ("spacing", &mut label.spacing),
        ("spacingTop", &mut label.spacing_top),
        ("spacingLeft", &mut label.spacing_left),
        ("spacingBottom", &mut label.spacing_bottom),
        ("spacingRight", &mut label.spacing_right),
    ] {
        if present(key).is_none() {
            accept(key, true);
            continue;
        }
        let value = style
            .num(key)
            .filter(|value| value.abs() <= MAX_LABEL_SPACING);
        if let Some(value) = value {
            *target = value;
        }
        accept(key, value.is_some());
    }
    for (key, target) in [
        ("labelBackgroundColor", &mut label.background),
        ("labelBorderColor", &mut label.border),
    ] {
        match style.get(key) {
            None | Some("" | "none" | "default") => accept(key, true),
            Some(value) => {
                let parsed = parse_style_color(value);
                *target = parsed.and_then(|parsed| parsed.color);
                accept(
                    key,
                    parsed.is_some_and(|parsed| !parsed.approximate) && target.is_some(),
                );
            }
        }
    }
    label.vertical = style.get("horizontal") == Some("0") && !header_orientation(style);
    let white_space = style.get("whiteSpace");
    if !label.is_default() || white_space.is_some_and(|value| value != "wrap") {
        label.wrap = white_space == Some("wrap");
    }
    ((!label.is_default()).then_some(label), owned, approximate)
}

/// draw.io keys for a shape's label text style.
fn label_text_style_keys(out: &mut String, label: &LabelStyle) {
    if let Some(family) = &label.font_family {
        let family: String = family.chars().filter(|c| !matches!(c, ';' | '=')).collect();
        if !family.is_empty() {
            out.push_str(&format!("fontFamily={family};"));
        }
    }
    if let Some(size) = label.font_size {
        out.push_str(&format!("fontSize={};", num(size)));
    }
    let bits = label.font_style_bits();
    if bits != 0 {
        out.push_str(&format!("fontStyle={bits};"));
    }
    let horizontal = |align: TextAlign| match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    };
    let vertical = |align: LabelVAlign| match align {
        LabelVAlign::Top => "top",
        LabelVAlign::Middle => "middle",
        LabelVAlign::Bottom => "bottom",
    };
    if label.align != TextAlign::Center {
        out.push_str(&format!("align={};", horizontal(label.align)));
    }
    if label.vertical_align != LabelVAlign::Middle {
        out.push_str(&format!(
            "verticalAlign={};",
            vertical(label.vertical_align)
        ));
    }
    if label.position != TextAlign::Center {
        out.push_str(&format!("labelPosition={};", horizontal(label.position)));
    }
    if label.vertical_position != LabelVAlign::Middle {
        out.push_str(&format!(
            "verticalLabelPosition={};",
            vertical(label.vertical_position)
        ));
    }
    if label.spacing != thinoda_core::label_style::DEFAULT_LABEL_SPACING {
        out.push_str(&format!("spacing={};", num(label.spacing)));
    }
    for (key, value) in [
        ("spacingTop", label.spacing_top),
        ("spacingLeft", label.spacing_left),
        ("spacingBottom", label.spacing_bottom),
        ("spacingRight", label.spacing_right),
    ] {
        if value != 0.0 {
            out.push_str(&format!("{key}={};", num(value)));
        }
    }
    for (key, color) in [
        ("labelBackgroundColor", label.background),
        ("labelBorderColor", label.border),
    ] {
        color_keys(
            out,
            key,
            "",
            color
                .filter(|color| color.a > 0)
                .map(|color| Color { a: 255, ..color }),
        );
    }
    if label.vertical && !out.contains("horizontal=") {
        out.push_str("horizontal=0;");
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
    Container(CanvasObjectId),
}

#[derive(Debug, Clone, Copy, Default)]
struct Context {
    /// Absolute top-left of the parent vertex; children are relative to it.
    origin: Vec2,
    parent_size: Vec2,
    parent_is_vertex: bool,
    zone: Option<usize>,
    bucket: Option<usize>,
    scene_parent: Option<CanvasObjectId>,
    /// Named board layer of the cell (`None` = base layer).
    layer: Option<thinoda_core::BoardLayerId>,
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
    scene_container_cells: HashSet<usize>,
    sheet: Sheet,
    warnings: &'a mut BTreeSet<ImportWarning>,
    items: HashMap<usize, Item>,
    shapes: HashMap<CanvasObjectId, ShapeKind>,
    visited: Vec<bool>,
    paint_order: Vec<usize>,
    zones: Vec<(ZoneId, Vec<TopicId>, Vec<CanvasObjectId>)>,
    buckets: Vec<(Vec<BoardMember>, bool, Option<usize>)>,
    locked: Vec<BoardMember>,
    deferred: Vec<(usize, Context)>,
    adaptive_fixed_colors: bool,
    inline_stencils: Option<
        Option<(
            std::sync::Arc<thinoda_core::stencil::StencilPackage>,
            String,
        )>,
    >,
    bundled_stencils: Option<BundledStencils>,
    /// Cells whose passthrough was already given to an object.
    passthrough_cells: HashSet<usize>,
    /// Style keys mapped by [`shape_effects`] and [`label_text_style`], per cell.
    shape_effect_keys: HashMap<usize, Vec<&'static str>>,
    passthrough_bytes: usize,
    /// Layers locked only after import, so grouping and locking can still edit them.
    locked_layers: Vec<thinoda_core::BoardLayerId>,
}

/// Per bundled draw.io package id: the page's closure and, once an object
/// uses it, its resource.
type BundledStencils = HashMap<
    String,
    (
        std::sync::Arc<thinoda_core::stencil::StencilPackage>,
        Option<String>,
    ),
>;

impl<'a> PageImport<'a> {
    fn warn(&mut self, warning: ImportWarning) {
        self.warnings.insert(warning);
    }

    fn owner_source_id(&mut self, index: usize) -> Option<String> {
        let id = &self.cells[index].id;
        if !id.is_empty()
            && !id.starts_with('\u{0}')
            && !matches!(id.as_str(), "0" | "1")
            && id.len() <= thinoda_core::drawio_passthrough::MAX_SOURCE_ID_BYTES
            && !id.chars().any(char::is_control)
            && self.passthrough_bytes.saturating_add(id.len())
                <= thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2
        {
            self.passthrough_bytes += id.len();
            Some(id.clone())
        } else {
            self.warn(ImportWarning::Other);
            None
        }
    }

    /// Unknown style keys, attributes and the id of `cell` (architecture §3.1.1).
    fn cell_passthrough(
        &mut self,
        cell: usize,
        object: &CanvasObject,
        attributes: Vec<(String, String)>,
    ) -> Option<Arc<Passthrough>> {
        let source = &self.cells[cell];
        let bound = object
            .stencil
            .as_ref()
            .map(crate::drawio_stencil::bound_style_keys)
            .unwrap_or_default();
        let sankey = source.style.flag("thinodaSankeyEdge");
        let edge = matches!(object.kind, CanvasObjectKind::Connector(_));
        let effects = self.shape_effect_keys.get(&cell);
        let style = source
            .style
            .order
            .iter()
            .filter(|key| {
                !OWNED_STYLE_KEYS.contains(&key.as_str())
                    && !key.starts_with("thinoda")
                    && !bound.contains(key.as_str())
                    && !(sankey && key.as_str() == "dashed")
                    && !(edge && EDGE_SHAPE_STYLE_KEYS.contains(&key.as_str()))
                    && !effects.is_some_and(|owned| owned.contains(&key.as_str()))
            })
            .filter_map(|key| {
                let value = source.style.get(key)?;
                (thinoda_core::drawio_passthrough::valid_style_key(key)
                    && thinoda_core::drawio_passthrough::valid_style_value(value))
                .then(|| (key.clone(), value.to_owned()))
            })
            .collect();
        let passthrough = Passthrough {
            source_id: (!source.id.starts_with('\u{0}')
                && !matches!(source.id.as_str(), "0" | "1")
                && source.id.len() <= thinoda_core::drawio_passthrough::MAX_SOURCE_ID_BYTES
                && !source.id.chars().any(char::is_control))
            .then(|| source.id.clone()),
            style,
            attributes,
        };
        if passthrough.is_empty() {
            return None;
        }
        let bytes = passthrough.byte_len();
        if passthrough.validate().is_err()
            || self.passthrough_bytes + bytes
                > thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2
        {
            self.warn(ImportWarning::Other);
            return None;
        }
        self.passthrough_bytes += bytes;
        Some(Arc::new(passthrough))
    }

    fn label(&mut self, cell: &Cell) -> String {
        let text = if cell.style.flag("html") {
            strip_html(&cell.value, self.warnings)
        } else {
            cell.value.clone()
        };
        truncate_chars(text, MAX_BOARD_TEXT_CHARS, self.warnings)
    }

    fn rich_label(&mut self, cell: &Cell) -> (String, Option<thinoda_core::InlineText>) {
        if !cell.style.flag("html") {
            return (self.label(cell), None);
        }
        let (text, rich) = parse_rich_html(&cell.value, self.warnings);
        let text = truncate_chars(text, MAX_BOARD_TEXT_CHARS, self.warnings);
        let rich = rich.filter(|value| {
            if value.validate(&text).is_err() {
                self.warnings.insert(ImportWarning::Formatting);
                false
            } else {
                true
            }
        });
        (text, rich)
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

    fn register(&mut self, cell: usize, item: Item, mut context: Context, locked: bool) {
        self.items.insert(cell, item);
        if context.layer.is_some() && matches!(item, Item::Card(_)) {
            // Cards always stay on the base layer and cannot share its groups.
            self.warn(ImportWarning::DrawioLayerContentFlattened);
            context.bucket = None;
        }
        if context.scene_parent.is_some() && matches!(item, Item::Card(_)) {
            self.warn(ImportWarning::DrawioSceneCardContainmentNotPreserved);
        }
        let member = match item {
            Item::Object(id) => BoardMember::Object(id),
            Item::Card(id) => BoardMember::Card(id),
            Item::Area(_) | Item::Container(_) => return,
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

    /// The page's inline stencils, embedded once as a single closure resource.
    fn inline_stencils(
        &mut self,
    ) -> Option<(
        std::sync::Arc<thinoda_core::stencil::StencilPackage>,
        String,
    )> {
        if self.inline_stencils.is_none() {
            let cells = self.cells;
            let bundle =
                crate::drawio_stencil::inline_bundle(cells.iter().map(|cell| cell.style.shape()))
                    .and_then(|package| {
                        let bytes = serde_json::to_vec(package.as_ref()).ok()?;
                        let resource = thinoda_core::resources::Resource::new(
                            "stencil-package.json",
                            "application/json",
                            bytes,
                        )
                        .ok()?;
                        let reference = self.sheet.resources.insert(resource).ok()?;
                        Some((package, reference))
                    });
            self.inline_stencils = Some(bundle);
        }
        self.inline_stencils.clone().flatten()
    }

    /// The page's bundled draw.io stencils: one closure per bundled package
    /// holding every definition the page's visible cells draw from it, so a
    /// page never embeds one resource per shape.
    fn bundled_stencils(&mut self) -> &mut BundledStencils {
        if self.bundled_stencils.is_none() {
            let bundle = crate::drawio_bundle::bundle();
            let mut used: std::collections::BTreeMap<String, BTreeSet<String>> = Default::default();
            for (index, cell) in self.cells.iter().enumerate() {
                if let Some(stencil) = cell.style.stencil()
                    && stencil.package_resource.is_none()
                    && !stencil.is_installed_builtin()
                    && self.shown(index)
                    && bundle
                        .package_by_id(&stencil.package)
                        .is_some_and(|package| package.definition(&stencil.definition).is_some())
                {
                    used.entry(stencil.package)
                        .or_default()
                        .insert(stencil.definition);
                }
            }
            let mut stencils = HashMap::new();
            for (id, definitions) in used {
                let roots: Vec<&str> = definitions.iter().map(String::as_str).collect();
                if let Some(closure) = bundle
                    .package_by_id(&id)
                    .and_then(|package| package.definition_closure(&roots).ok())
                {
                    stencils.insert(id, (std::sync::Arc::new(closure), None));
                }
            }
            self.bundled_stencils = Some(stencils);
        }
        self.bundled_stencils.get_or_insert_default()
    }

    /// The page closure of a bundled package and its resource, embedding the
    /// resource when the first object draws from it.
    fn bundled_stencil(
        &mut self,
        package: &str,
    ) -> Option<(
        std::sync::Arc<thinoda_core::stencil::StencilPackage>,
        String,
    )> {
        let (closure, reference) = self.bundled_stencils().get(package)?.clone();
        if let Some(reference) = reference {
            return Some((closure, reference));
        }
        let bytes = serde_json::to_vec(closure.as_ref()).ok()?;
        let resource = thinoda_core::resources::Resource::new(
            "stencil-package.json",
            "application/json",
            bytes,
        )
        .ok()?;
        let reference = self.sheet.resources.insert(resource).ok()?;
        if let Some(entry) = self.bundled_stencils().get_mut(package) {
            entry.1 = Some(reference.clone());
        }
        Some((closure, reference))
    }

    /// Whether the cell and all its ancestors are visible, as `visit` requires.
    fn shown(&self, mut index: usize) -> bool {
        for _ in 0..=MAX_CELL_DEPTH {
            let cell = &self.cells[index];
            if !cell.visible {
                return false;
            }
            match cell
                .parent
                .as_deref()
                .and_then(|parent| self.index.get(parent))
            {
                Some(&parent) if parent != index => index = parent,
                _ => return true,
            }
        }
        false
    }

    /// draw.io's lookup of placeholder `name` on `cell` and its ancestors, with
    /// the index of the ancestor holding it.
    fn placeholder_value(&self, cell: &Cell, name: &str) -> Option<(String, Option<usize>)> {
        match name {
            "id" => return Some((cell.id.clone(), None)),
            "width" if cell.vertex => return Some((cell.geometry.width.to_string(), None)),
            "height" if cell.vertex => return Some((cell.geometry.height.to_string(), None)),
            _ => {}
        }
        let own = |cell: &Cell| {
            cell.attributes
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        if let Some(value) = own(cell) {
            return Some((value, None));
        }
        let mut parent = cell.parent.as_deref();
        for _ in 0..self.cells.len() {
            let &index = parent.and_then(|id| self.index.get(id))?;
            let ancestor = &self.cells[index];
            if let Some(value) = own(ancestor) {
                return Some((value, Some(index)));
            }
            parent = ancestor.parent.as_deref();
        }
        None
    }

    fn replaces_placeholders(cell: &Cell) -> bool {
        cell.attributes
            .iter()
            .any(|(name, value)| name == "placeholders" && value == "1")
    }

    /// `text` with `cell`'s placeholders resolved, for imports without a
    /// data model (Zones).
    fn resolved_cell_text<'t>(&self, cell: &Cell, text: &'t str) -> std::borrow::Cow<'t, str> {
        if !Self::replaces_placeholders(cell) {
            return std::borrow::Cow::Borrowed(text);
        }
        thinoda_core::object_data::replace_placeholders(text, |name| {
            self.placeholder_value(cell, name).map(|(value, _)| value)
        })
    }

    /// Values a placeholder of `cell` takes from a draw.io ancestor that does not
    /// become a Thinoda scene container, copied so the label still resolves.
    fn inherited_placeholder_values(&self, cell: usize) -> Vec<(String, String)> {
        let own = &self.cells[cell];
        if !Self::replaces_placeholders(own) {
            return Vec::new();
        }
        let mut inherited: Vec<(String, String)> = Vec::new();
        for text in [Some(own.value.as_str()), own.tooltip.as_deref()]
            .into_iter()
            .flatten()
        {
            let _ = thinoda_core::object_data::replace_placeholders(text, |name| {
                if !inherited.iter().any(|(known, _)| known == name)
                    && let Some((value, Some(holder))) = self.placeholder_value(own, name)
                    && !matches!(self.items.get(&holder), Some(Item::Container(_)))
                {
                    inherited.push((name.to_owned(), value));
                }
                None
            });
        }
        inherited
    }

    fn add_object(
        &mut self,
        cell: usize,
        mut object: CanvasObject,
        context: Context,
    ) -> Option<CanvasObjectId> {
        if self.passthrough_cells.insert(cell) {
            let mut attributes = self.cells[cell].attributes.clone();
            attributes.extend(self.inherited_placeholder_values(cell));
            let (data, rest) = split_object_data(&attributes);
            object.data = data;
            object.drawio = self.cell_passthrough(cell, &object, rest);
        }
        if let Some(stencil) = &object.stencil
            && stencil.package == crate::drawio_stencil::INLINE_PACKAGE_ID
            && stencil.package_resource.is_none()
            && let Some((package, reference)) = self.inline_stencils()
            && package.definition(&stencil.definition).is_some()
            && let Some(stencil) = &mut object.stencil
        {
            stencil.embed_package(package, reference);
        }
        if let Some(stencil) = &object.stencil
            && stencil.package_resource.is_none()
            && !stencil.is_installed_builtin()
            && let Some((package, reference)) = self.bundled_stencil(&stencil.package.clone())
            && package.definition(&stencil.definition).is_some()
            && let Some(stencil) = &mut object.stencil
        {
            stencil.embed_package(package, reference);
        }
        let shape = match &object.kind {
            CanvasObjectKind::Shape { shape, .. } => Some(*shape),
            _ => None,
        };
        object.layer = context.layer;
        match self.sheet.add_canvas_object(object) {
            Ok(id) => {
                if let Some(shape) = shape {
                    self.shapes.insert(id, shape);
                }
                if let Some(parent) = context.scene_parent {
                    self.sheet.canvas.scene.parents.insert(id, parent);
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

    fn vertex_rect_from_bounds(&self, cell: &Cell, context: Context, bounds: Rect) -> Rect {
        let geometry = &cell.geometry;
        let mut min = if geometry.relative && context.parent_is_vertex {
            Vec2::new(
                context.origin.x + bounds.min.x * context.parent_size.x,
                context.origin.y + bounds.min.y * context.parent_size.y,
            )
        } else {
            Vec2::new(
                context.origin.x + bounds.min.x,
                context.origin.y + bounds.min.y,
            )
        };
        if geometry.relative
            && let Some(offset) = geometry.offset
        {
            min = min + offset;
        }
        Rect::from_min_size(min, bounds.size)
    }

    fn vertex_rect(&self, cell: &Cell, context: Context) -> Rect {
        let geometry = &cell.geometry;
        self.vertex_rect_from_bounds(
            cell,
            context,
            Rect::from_min_size(
                Vec2::new(geometry.x, geometry.y),
                Vec2::new(geometry.width, geometry.height),
            ),
        )
    }

    fn run(&mut self) {
        let roots: Vec<usize> = (0..self.cells.len())
            .filter(|&i| self.cells[i].parent.is_none())
            .collect();
        for root in roots {
            if self.cells[root].vertex || self.cells[root].edge {
                self.warn(ImportWarning::Other);
                self.visit(root, Context::default(), 0);
                continue;
            }
            if self.cells[root].id != "0"
                || !self.cells[root].value.is_empty()
                || !self.cells[root].visible
                || structural_cell_has_unrepresented_data(&self.cells[root], false)
            {
                self.warn(ImportWarning::Other);
            }
            self.visited[root] = true;
            for (position, layer) in self.children[root].clone().into_iter().enumerate() {
                self.visited[layer] = true;
                let cell = &self.cells[layer];
                if cell.vertex || cell.edge {
                    self.visited[layer] = false;
                    self.visit(layer, Context::default(), 0);
                } else {
                    let context = Context {
                        layer: self.board_layer(layer, position),
                        ..Context::default()
                    };
                    for child in self.children[layer].clone() {
                        self.visit(child, context, 0);
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

    /// The board layer for a draw.io layer cell. An unnamed (or "Background"),
    /// visible and unlocked first layer is the base layer.
    fn board_layer(&mut self, index: usize, position: usize) -> Option<thinoda_core::BoardLayerId> {
        if structural_cell_has_unrepresented_data(&self.cells[index], true) {
            self.warn(ImportWarning::Other);
        }
        let cell = &self.cells[index];
        let locked = cell.style.flag("locked");
        let value: String = cell
            .value
            .trim()
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(thinoda_core::board_layers::MAX_BOARD_LAYER_NAME_CHARS)
            .collect();
        if position == 0 && cell.visible && !locked && (value.is_empty() || value == "Background") {
            if cell.id != "1" || !cell.value.is_empty() {
                self.warn(ImportWarning::Other);
            }
            return None;
        }
        if cell.value != value || value.trim() != value {
            self.warn(ImportWarning::Other);
        }
        let cell = &self.cells[index];
        if self.sheet.canvas.layers.len() >= thinoda_core::board_layers::MAX_BOARD_LAYERS {
            self.warn(ImportWarning::DrawioLayerContentFlattened);
            return None;
        }
        let name = match value.trim() {
            "" if position == 0 => "Background".to_owned(),
            "" => format!("Layer {position}"),
            name => name.to_owned(),
        };
        let id = self.sheet.add_board_layer(&name).ok()?;
        let source_id = (!cell.id.is_empty()
            && !cell.id.starts_with('\u{0}')
            && !matches!(cell.id.as_str(), "0" | "1")
            && cell.id.len() <= thinoda_core::drawio_passthrough::MAX_SOURCE_ID_BYTES
            && !cell.id.chars().any(char::is_control)
            && !self
                .sheet
                .canvas
                .layers
                .iter()
                .any(|layer| layer.source_id.as_deref() == Some(cell.id.as_str())))
        .then(|| cell.id.clone());
        let visible = cell.visible;
        if source_id.is_none() {
            self.warn(ImportWarning::Other);
        }
        let layer = self
            .sheet
            .canvas
            .layers
            .last_mut()
            .expect("layer was just added");
        layer.visible = visible;
        layer.source_id = source_id;
        if locked {
            self.locked_layers.push(id);
        }
        Some(id)
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
        let rect = if self.scene_container_cells.contains(&index) && cell.collapsed {
            cell.geometry
                .alternate_bounds
                .map(|bounds| self.vertex_rect_from_bounds(cell, context, bounds))
                .unwrap_or_else(|| self.vertex_rect(cell, context))
        } else {
            self.vertex_rect(cell, context)
        };
        let compact_rect = if self.scene_container_cells.contains(&index)
            && cell.collapsed
            && cell.geometry.alternate_bounds.is_some()
        {
            self.vertex_rect(cell, context)
        } else {
            rect
        };
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
        let effects = matches!(kind, Kind::Shape(..) | Kind::Approx(..))
            && cell.style.get("childLayout") != Some("tableLayout");
        let label_text = maps_label_text(kind, &cell.style);
        if cell
            .style
            .formatting_loss(false, card_text_alignment, effects, label_text)
        {
            self.warn(ImportWarning::Formatting);
        }
        if matches!(cell.style.shape(), "note" | "note2") && !cell.style.draws_stencil() {
            self.warn(ImportWarning::DrawioFoldedNotesApproximated);
        }
        if cell.style.get("childLayout") == Some("tableLayout") {
            if self.table(index, rect, context, depth) {
                return;
            }
            log::warn!("draw.io table {} imported without its grid", cell.id);
            self.warn(ImportWarning::DrawioSceneLayoutNotPreserved);
        }
        match kind {
            Kind::Group => {
                if !cell.value.is_empty() || cell.collapsed {
                    self.warn(ImportWarning::Other);
                }
                if cell.style.suppress_defaults
                    || cell.style.tokens.iter().any(|token| token != "group")
                    || cell
                        .style
                        .map
                        .iter()
                        .any(|(key, value)| key != "locked" || !matches!(value.as_str(), "0" | "1"))
                {
                    self.warn(ImportWarning::Formatting);
                }
                if context.scene_parent.is_some() {
                    self.warn(ImportWarning::Other);
                }
                let bucket = if let Some(bucket) = context.bucket {
                    self.warn(ImportWarning::Other);
                    bucket
                } else {
                    self.buckets.push((Vec::new(), false, Some(index)));
                    self.buckets.len() - 1
                };
                if cell.style.flag("locked") {
                    self.buckets[bucket].1 = true;
                }
                child_context.bucket = Some(bucket);
                self.items.insert(index, Item::Area(rect));
            }
            Kind::Container
                if self.scene_container_cells.contains(&index) && context.bucket.is_none() =>
            {
                if let Some(id) = self.scene_container(index, compact_rect, rect, context) {
                    self.sheet.canvas.scene.containers.insert(id);
                    child_context.zone = None;
                    child_context.bucket = None;
                    child_context.scene_parent = Some(id);
                    self.items.insert(index, Item::Container(id));
                } else {
                    self.warn(ImportWarning::Other);
                    let drawn = self.draw(
                        index,
                        Kind::Shape(ShapeKind::Rectangle, 0.0, true),
                        rect,
                        context,
                    );
                    if has_children {
                        let bucket = match context.bucket {
                            Some(bucket) => bucket,
                            None => {
                                let members = drawn.into_iter().collect();
                                self.buckets.push((members, false, None));
                                self.buckets.len() - 1
                            }
                        };
                        child_context.bucket = Some(bucket);
                    }
                }
            }
            Kind::Container
                if let Some(zone) = self.zone(
                    cell,
                    rect,
                    context.zone.is_some() || context.bucket.is_some(),
                ) =>
            {
                if context.layer.is_some() {
                    self.warn(ImportWarning::DrawioLayerContentFlattened);
                }
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
                            self.buckets.push((members, false, None));
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

    fn scene_container(
        &mut self,
        index: usize,
        compact_rect: Rect,
        expanded_rect: Rect,
        context: Context,
    ) -> Option<CanvasObjectId> {
        let cell = &self.cells[index];
        let style = &cell.style;
        let can_preserve_collapse = cell.collapsed
            && cell.geometry.alternate_bounds.is_some()
            && expanded_rect.size.x >= MIN_ZONE_SIZE
            && expanded_rect.size.y >= MIN_ZONE_SIZE
            && style.rotation() == 0.0;
        if cell.collapsed && !can_preserve_collapse {
            log::warn!(
                "draw.io collapsed scene state on cell {} has no usable alternate bounds",
                cell.id
            );
            self.warn(ImportWarning::DrawioCollapsedSceneExpanded);
        }
        if expanded_rect.size.x < MIN_ZONE_SIZE
            || expanded_rect.size.y < MIN_ZONE_SIZE
            || style.rotation() != 0.0
        {
            return None;
        }
        let mut header = thinoda_core::scene_layout::HeaderPolicy {
            side: thinoda_core::scene_layout::HeaderSide::Top,
            extent: 0.0,
        };

        let label = self.label(cell);
        let mut object = CanvasObject::shape(ShapeKind::Rectangle, compact_rect);
        if let CanvasObjectKind::Shape {
            label: object_label,
            ..
        } = &mut object.kind
        {
            *object_label = label;
        }
        object.style = object_style(style);
        if style.shape() == "swimlane" {
            if style.get("swimlaneFillColor").is_some() || style.get("fillColor").is_some() {
                self.warn(ImportWarning::Formatting);
            }
            if let Some(fill) = style.color("swimlaneFillColor", "fillOpacity") {
                object.style.fill = Some(fill);
            }
            let horizontal = match style.get("horizontal") {
                None | Some("1") => true,
                Some("0") => false,
                Some(_) => {
                    self.warn(ImportWarning::Formatting);
                    true
                }
            };
            let default_extent = 40.0;
            let source_extent = style.num("startSize").unwrap_or(default_extent);
            let limit = if horizontal {
                compact_rect.size.y
            } else {
                compact_rect.size.x
            };
            let extent = source_extent.clamp(0.0, limit);
            if (style.get("startSize").is_some() && style.num("startSize").is_none())
                || extent != source_extent
            {
                self.warn(ImportWarning::Formatting);
            }
            header = thinoda_core::scene_layout::HeaderPolicy {
                side: if horizontal {
                    thinoda_core::scene_layout::HeaderSide::Top
                } else {
                    thinoda_core::scene_layout::HeaderSide::Left
                },
                extent,
            };
            let mut stencil =
                thinoda_core::stencil_object::StencilRef::new("thinoda.general", "header-frame-v2");
            stencil
                .parameters
                .insert("headerSide".into(), if horizontal { 0.0 } else { 3.0 });
            stencil
                .parameters
                .insert("headerHeight".into(), f64::from(extent));
            object.stencil = Some(stencil);
        }
        object.band = style.band();
        object.alt_text = cell.tooltip.clone().unwrap_or_default();
        let (layout_policy, unsupported_layout) = style.scene_layout_policy(header);
        if unsupported_layout {
            log::warn!(
                "draw.io child layout on cell {} is only partially represented",
                cell.id
            );
            self.warn(ImportWarning::DrawioSceneLayoutNotPreserved);
        }
        let Some(id) = self.add_object(index, object, context) else {
            if can_preserve_collapse {
                self.warn(ImportWarning::DrawioCollapsedSceneExpanded);
            }
            return None;
        };
        if can_preserve_collapse {
            self.sheet.canvas.scene.collapsed.insert(id, expanded_rect);
        }
        if let Some(policy) = layout_policy {
            self.sheet.canvas.scene.layout_policies.insert(id, policy);
        }
        Some(id)
    }

    /// Imports a draw.io `tableLayout` container as a native table whose rows
    /// and cells become its grid. Returns false, before creating anything,
    /// when the source is not a rectangular grid of rows and cells.
    fn table(&mut self, index: usize, rect: Rect, context: Context, depth: usize) -> bool {
        use thinoda_core::scene_layout::{
            ContainerLayout, ContainerLayoutPolicy, HeaderPolicy, HeaderSide,
        };
        use thinoda_core::scene_table::{
            MAX_TABLE_SLOTS, MAX_TABLE_TRACKS, MIN_TABLE_TRACK, SceneTable, SceneTableCell,
            TableStripes, table_body, table_frame_size,
        };

        let cells = self.cells;
        let cell = &cells[index];
        let style = &cell.style;
        if context.bucket.is_some()
            || cell.collapsed
            || style.rotation() != 0.0
            || depth + 3 > MAX_CELL_DEPTH
        {
            return false;
        }
        let vertices = |parent: usize| -> Vec<usize> {
            self.children[parent]
                .iter()
                .copied()
                .filter(|&child| cells[child].vertex)
                .collect()
        };
        let rows = vertices(index);
        let grid: Vec<Vec<usize>> = rows.iter().map(|&row| vertices(row)).collect();
        let columns = grid.first().map_or(0, Vec::len);
        if rows.is_empty()
            || rows.len() > MAX_TABLE_TRACKS
            || columns == 0
            || columns > MAX_TABLE_TRACKS
            || rows.len() * columns > MAX_TABLE_SLOTS
            || grid.iter().any(|row| row.len() != columns)
            || rows
                .iter()
                .chain(grid.iter().flatten())
                .any(|&cell| cells[cell].geometry.invalid_frame)
        {
            return false;
        }

        // draw.io treats tables like swimlanes: an absent startSize is 40.
        let default_extent = if matches!(style.shape(), "table" | "swimlane") {
            40.0
        } else {
            0.0
        };
        let horizontal = style.get("horizontal") != Some("0");
        let limit = if horizontal { rect.size.y } else { rect.size.x };
        let source_extent = style.num("startSize").unwrap_or(default_extent);
        let extent = source_extent.clamp(0.0, (limit - MIN_TABLE_TRACK).max(0.0));
        if extent != source_extent {
            self.warn(ImportWarning::Formatting);
        }
        let header = (extent > 0.0).then_some(HeaderPolicy {
            side: if horizontal {
                HeaderSide::Top
            } else {
                HeaderSide::Left
            },
            extent,
        });
        let Ok(body) = table_body(rect, header) else {
            return false;
        };

        let span = |cell: usize, key: &str, room: usize| -> (usize, bool) {
            let style = &cells[cell].style;
            match style.get(key) {
                None => (1, false),
                Some(raw) => match raw.trim().parse::<usize>() {
                    Ok(span) if (1..=room).contains(&span) => (span, false),
                    _ => (1, true),
                },
            }
        };
        let fit = |sizes: Vec<f32>, total: f32| -> Vec<f32> {
            let sum: f32 = sizes.iter().sum();
            let count = sizes.len() as f32;
            sizes
                .into_iter()
                .map(|size| {
                    let size = if sum > 0.0 && sum.is_finite() && total > 0.0 {
                        size * total / sum
                    } else {
                        total / count
                    };
                    (size.round()).max(MIN_TABLE_TRACK)
                })
                .collect()
        };
        let row_sizes = rows
            .iter()
            .map(|&row| cells[row].geometry.height.max(0.0))
            .collect();
        // Prefer an unmerged cell of each column; merged ones keep their slot
        // size only as alternate bounds.
        let column_sizes = (0..columns)
            .map(|column| {
                let source = grid
                    .iter()
                    .map(|row| row[column])
                    .find(|&cell| span(cell, "colspan", columns).0 == 1)
                    .unwrap_or(grid[0][column]);
                let geometry = &cells[source].geometry;
                match geometry.alternate_bounds {
                    Some(bounds) => bounds.size.x.max(0.0),
                    None => geometry.width.max(0.0) / span(source, "colspan", columns).0 as f32,
                }
            })
            .collect();
        let mut table = SceneTable {
            rows: fit(row_sizes, body.size.y),
            columns: fit(column_sizes, body.size.x),
            cells: Vec::new(),
            stripes: TableStripes {
                even_rows: style.color("evenRowColor", ""),
                odd_rows: style.color("oddRowColor", ""),
                even_columns: style.color("evenColumnColor", ""),
                odd_columns: style.color("oddColumnColor", ""),
            },
        };

        // Each slot belongs to the first cell whose span reaches it; covered
        // source cells are hidden in draw.io and only their content is lost.
        let mut covered = vec![false; rows.len() * columns];
        let mut sources: Vec<usize> = Vec::new();
        let mut clamped = false;
        for (r, row) in grid.iter().enumerate() {
            for (c, &source) in row.iter().enumerate() {
                if covered[r * columns + c] {
                    if !self.label(&cells[source]).trim().is_empty()
                        || self.children[source]
                            .iter()
                            .any(|&child| cells[child].vertex || cells[child].edge)
                    {
                        self.warn(ImportWarning::Formatting);
                    }
                    continue;
                }
                let (mut row_span, bad_row) = span(source, "rowspan", rows.len() - r);
                let (mut column_span, bad_column) = span(source, "colspan", columns - c);
                clamped |= bad_row || bad_column;
                let free = |covered: &[bool], r: usize, c: usize| !covered[r * columns + c];
                let width = (c..c + column_span)
                    .take_while(|&column| free(&covered, r, column))
                    .count();
                if width < column_span {
                    column_span = width;
                    clamped = true;
                }
                let height = (r..r + row_span)
                    .take_while(|&row| {
                        (c..c + column_span).all(|column| free(&covered, row, column))
                    })
                    .count();
                if height < row_span {
                    row_span = height;
                    clamped = true;
                }
                for row in r..r + row_span {
                    for column in c..c + column_span {
                        covered[row * columns + column] = true;
                    }
                }
                let mut slot = SceneTableCell::new(CanvasObjectId(0), r, c);
                slot.row_span = row_span as u16;
                slot.column_span = column_span as u16;
                table.cells.push(slot);
                sources.push(source);
            }
        }
        if clamped {
            self.warn(ImportWarning::Formatting);
        }
        if table.validate().is_err() {
            return false;
        }
        let actual: HashSet<usize> = sources.iter().copied().collect();

        let frame = Rect::from_min_size(rect.min, table_frame_size(&table, header));
        if frame.size != rect.size {
            self.warn(ImportWarning::Formatting);
        }
        let Ok(body) = table_body(frame, header) else {
            return false;
        };
        let label = self.label(cell);
        let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
        if let CanvasObjectKind::Shape {
            label: object_label,
            ..
        } = &mut object.kind
        {
            *object_label = label;
        }
        object.style = object_style(style);
        object.band = style.band();
        object.alt_text = cell.tooltip.clone().unwrap_or_default();
        if let Some(header) = header {
            let mut stencil =
                thinoda_core::stencil_object::StencilRef::new("thinoda.general", "header-frame-v2");
            stencil.parameters.insert(
                "headerSide".into(),
                if header.side == HeaderSide::Top {
                    0.0
                } else {
                    3.0
                },
            );
            stencil
                .parameters
                .insert("headerHeight".into(), f64::from(header.extent));
            object.stencil = Some(stencil);
        }
        let Some(table_id) = self.add_object(index, object, context) else {
            return false;
        };
        self.sheet.canvas.scene.containers.insert(table_id);
        if let Some(header) = header {
            self.sheet.canvas.scene.layout_policies.insert(
                table_id,
                ContainerLayoutPolicy {
                    header,
                    layout: ContainerLayout::Free,
                },
            );
        }
        self.items.insert(index, Item::Container(table_id));

        let table_context = Context {
            origin: rect.min,
            parent_size: rect.size,
            parent_is_vertex: true,
            zone: None,
            bucket: None,
            scene_parent: Some(table_id),
            ..context
        };
        let mut complete = true;
        for (r, &row) in rows.iter().enumerate() {
            self.visited[row] = true;
            let row_cell = &cells[row];
            if !self.label(row_cell).trim().is_empty()
                || row_cell.style.num("startSize").unwrap_or(0.0) > 0.0
            {
                self.warn(ImportWarning::Formatting);
            }
            let row_fill = if row_cell.style.shape() == "swimlane" {
                row_cell.style.color("swimlaneFillColor", "fillOpacity")
            } else {
                row_cell.style.color("fillColor", "fillOpacity")
            }
            .filter(|fill| fill.a > 0);
            for &source in &grid[r] {
                if !actual.contains(&source) {
                    self.mark_visited(source);
                }
            }
            for (slot, &source) in sources.iter().enumerate() {
                if usize::from(table.cells[slot].row) != r {
                    continue;
                }
                let source_cell = &cells[source];
                let source_style = &source_cell.style;
                self.visited[source] = true;
                self.paint_order.push(source);
                let cell_rect = table.cell_rect(body, &table.cells[slot]);
                let label = self.label(source_cell);
                let mut object = CanvasObject::shape(ShapeKind::Rectangle, cell_rect);
                object.style = object_style(source_style);
                object.band = source_style.band();
                object.alt_text = source_cell.tooltip.clone().unwrap_or_default();
                let has_label = !label.trim().is_empty();
                if let CanvasObjectKind::Shape {
                    label: object_label,
                    ..
                } = &mut object.kind
                {
                    *object_label = label;
                }
                match source_style.shape() {
                    "partialRectangle" => {
                        // draw.io paints table grid lines on the table; Thinoda
                        // paints them as cell sides in the table's stroke.
                        let mut sides = source_style.rectangle_sides();
                        let row_lines = style.get("rowLines") != Some("0");
                        let column_lines = style.get("columnLines") != Some("0");
                        sides.top |= row_lines;
                        sides.bottom |= row_lines;
                        sides.left |= column_lines;
                        sides.right |= column_lines;
                        object.rectangle_sides = Some(sides);
                        let outline_visible =
                            sides.top || sides.right || sides.bottom || sides.left;
                        let mut resolved = source_style.clone();
                        if resolved.get("strokeColor") == Some("inherit") {
                            match style.get("strokeColor") {
                                Some(stroke) => {
                                    resolved.map.insert("strokeColor".into(), stroke.into());
                                }
                                None => {
                                    resolved.map.remove("strokeColor");
                                }
                            }
                        }
                        if resolved_shape_colors(
                            &resolved,
                            &mut object,
                            has_label,
                            self.adaptive_fixed_colors,
                            outline_visible,
                        ) {
                            self.warn(ImportWarning::Formatting);
                        }
                    }
                    "swimlane" => {
                        object.style.fill = source_style.color("swimlaneFillColor", "fillOpacity");
                        if source_style.num("startSize").unwrap_or(40.0) > 0.0 {
                            self.warn(ImportWarning::Formatting);
                        }
                    }
                    "" | "rect" | "rectangle" => {}
                    _ => self.warn(ImportWarning::Formatting),
                }
                if source_style.rotation() != 0.0 {
                    self.warn(ImportWarning::Formatting);
                }
                if object.style.fill.is_none_or(|fill| fill.a == 0)
                    && let Some(fill) = row_fill
                {
                    object.style.fill = Some(fill);
                }
                let Some(id) = self.add_object(source, object, table_context) else {
                    complete = false;
                    self.mark_visited(source);
                    continue;
                };
                self.sheet.canvas.scene.containers.insert(id);
                self.items.insert(source, Item::Container(id));
                table.cells[slot].id = id;
                let content_context = Context {
                    origin: cell_rect.min,
                    parent_size: cell_rect.size,
                    scene_parent: Some(id),
                    ..table_context
                };
                for child in self.children[source].clone() {
                    self.visit(child, content_context, depth + 3);
                }
            }
            let row_origin = self.vertex_rect(row_cell, table_context).min;
            for child in self.children[row].clone() {
                if cells[child].edge {
                    let context = Context {
                        origin: row_origin,
                        ..table_context
                    };
                    self.visit(child, context, depth + 2);
                } else if !cells[child].vertex {
                    self.mark_visited(child);
                }
            }
        }
        for child in self.children[index].clone() {
            if cells[child].edge {
                self.visit(child, table_context, depth + 1);
            } else if !cells[child].vertex {
                self.mark_visited(child);
            }
        }
        if complete {
            self.sheet.canvas.scene.tables.insert(table_id, table);
        } else {
            self.warn(ImportWarning::DrawioSceneLayoutNotPreserved);
        }
        true
    }

    fn mark_visited(&mut self, index: usize) {
        let mut stack = vec![index];
        while let Some(index) = stack.pop() {
            if !std::mem::replace(&mut self.visited[index], true) {
                stack.extend(self.children[index].iter().copied());
            }
        }
    }

    /// A container frame, unless it cannot be a valid Zone.
    fn zone(&mut self, cell: &Cell, rect: Rect, nested: bool) -> Option<usize> {
        if nested {
            self.warn(ImportWarning::Other);
        }
        if self.sheet.zones.len() >= MAX_ZONES
            || rect.size.x < MIN_ZONE_SIZE
            || rect.size.y < MIN_ZONE_SIZE
            || cell.style.rotation() != 0.0
        {
            return None;
        }
        let label = self.label(cell);
        let label = self.resolved_cell_text(cell, &label);
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
        let source_id = if !cell.id.is_empty()
            && !cell.id.starts_with('\u{0}')
            && !matches!(cell.id.as_str(), "0" | "1")
            && cell.id.len() <= thinoda_core::drawio_passthrough::MAX_SOURCE_ID_BYTES
            && !cell.id.chars().any(char::is_control)
            && self.passthrough_bytes.saturating_add(cell.id.len())
                <= thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2
        {
            self.passthrough_bytes += cell.id.len();
            Some(cell.id.clone())
        } else {
            self.warn(ImportWarning::Other);
            None
        };
        let swimlane = cell.style.shape() == "swimlane";
        if !swimlane {
            self.warn(ImportWarning::Formatting);
        }
        // draw.io defaults to 40; Thinoda exports its frame header at 24.
        if (swimlane && cell.style.get("startSize").is_none())
            || cell.style.suppress_defaults
            || cell.style.tokens.iter().any(|token| token != "swimlane")
            || cell
                .style
                .map
                .iter()
                .any(|(key, value)| !match key.as_str() {
                    "strokeColor" | "strokeOpacity" | "swimlaneFillColor" | "fillOpacity" => true,
                    "html" | "horizontal" => value == "1",
                    "whiteSpace" => value == "wrap",
                    "startSize" => value.parse::<f32>().is_ok_and(|size| size == 24.0),
                    "collapsible" => value == "0",
                    "thinodaClip" => matches!(value.as_str(), "0" | "1"),
                    _ => false,
                })
        {
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
            zone.drawio_source_id = source_id;
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
        let (label, rich_label) =
            if !matches!(kind, Kind::Line | Kind::Card(CardKind::Image) | Kind::Image) {
                self.rich_label(cell)
            } else {
                (self.label(cell), None)
            };
        if style.opacity() < 1.0 && matches!(kind, Kind::Card(_) | Kind::Image)
            || !label.is_empty()
                && (style.label_opacity_loss()
                    || kind == Kind::Line && style.connector_label_appearance_loss(false))
        {
            self.warn(ImportWarning::Formatting);
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
        let mut object = match kind {
            Kind::Shape(..) if style.draws_stencil() => {
                // Stencils are drawn in their own frame; draw.io's `direction`
                // turns that frame like a rotation with swapped sides.
                let direction = direction_degrees(style).rem_euclid(360.0);
                let frame = if direction % 180.0 == 90.0 {
                    Rect::from_center_size(rect.center(), Vec2::new(rect.size.y, rect.size.x))
                } else {
                    rect
                };
                let rotation = direction + style.rotation();
                let mut object = shape_object(ShapeKind::Rectangle, frame, rotation, label);
                object.stencil = style.stencil();
                let (owned, approximate) =
                    shape_effects(style, &mut object, direction % 180.0 == 90.0);
                self.shape_effect_keys.insert(index, owned);
                if approximate {
                    self.warn(ImportWarning::Formatting);
                }
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
                let (owned, approximate) = shape_effects(style, &mut object, total % 180.0 == 90.0);
                self.shape_effect_keys.insert(index, owned);
                if approximate {
                    self.warn(ImportWarning::Formatting);
                }
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
                        None,
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
                return self.card(index, card_kind, rect, label, context, image, rich_label);
            }
        };
        if matches!(object.kind, CanvasObjectKind::Shape { .. }) {
            object.label_rich_text = rich_label.map(Arc::new);
        }
        let label_text = maps_label_text(kind, style);
        if label_text && matches!(object.kind, CanvasObjectKind::Shape { .. }) {
            let (label_style, owned, approximate) = label_text_style(style);
            object.label_style = label_style.map(Arc::new);
            self.shape_effect_keys
                .entry(index)
                .or_default()
                .extend(owned);
            if approximate && !object.label().is_empty() {
                self.warn(ImportWarning::Formatting);
            }
        }
        if !object.label().is_empty() && style.label_style_loss(false, label_text) {
            self.warn(ImportWarning::Formatting);
        }
        self.add_object(index, object, context)
            .map(BoardMember::Object)
    }

    #[allow(clippy::too_many_arguments)]
    fn card(
        &mut self,
        index: usize,
        kind: CardKind,
        rect: Rect,
        label: String,
        context: Context,
        image: Option<CardImage>,
        rich: Option<thinoda_core::InlineText>,
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
                let source_id = self.owner_source_id(index);
                if let Some(topic) = self.sheet.topic_mut(id) {
                    topic.drawio_source_id = source_id;
                    topic.style.fill = card.color;
                    topic.style.text_color = style.text_color();
                    topic.inline_text = rich.unwrap_or_default();
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
                Some(Item::Container(id)) => {
                    return End::Ready {
                        endpoint: Endpoint::Object(*id),
                        detached: false,
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
                .filter(|key| {
                    // Native flips mirror ports exactly as draw.io does.
                    !(matches!(**key, "flipH" | "flipV")
                        && matches!(endpoint, Endpoint::Object(id)
                        if self.sheet.canvas.object(id).is_some_and(|object| {
                            object.style.flip_h == style.flag("flipH")
                                && object.style.flip_v == style.flag("flipV")
                        })))
                })
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
            if style.formatting_loss(true, false, false, false)
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
        let edge_style = style.get("edgeStyle").filter(|value| !value.is_empty());
        let route = if style.flag("curved") {
            ConnectorRoute::Curve
        } else {
            match edge_style {
                None | Some("none") => ConnectorRoute::Straight,
                Some("elbowEdgeStyle") if style.get("elbow") == Some("vertical") => {
                    ConnectorRoute::ElbowVertical
                }
                Some("elbowEdgeStyle") => ConnectorRoute::ElbowHorizontal,
                Some("sideToSideEdgeStyle") => ConnectorRoute::SideToSide,
                Some("topToBottomEdgeStyle") => ConnectorRoute::TopToBottom,
                Some("entityRelationEdgeStyle") => ConnectorRoute::EntityRelation,
                Some("segmentEdgeStyle") => ConnectorRoute::Segment,
                Some("isometricEdgeStyle") if style.get("elbow") == Some("vertical") => {
                    ConnectorRoute::IsometricVertical
                }
                Some("isometricEdgeStyle") => ConnectorRoute::IsometricHorizontal,
                Some(_) => ConnectorRoute::Elbow,
            }
        };
        // Routed styles Thinoda reproduces exactly keep their draw.io meaning,
        // including the first-waypoint hint; any other router is approximated
        // by Thinoda's orthogonal elbow.
        let approximated_router = edge_style.is_some_and(|value| {
            !matches!(
                value,
                "none"
                    | "orthogonalEdgeStyle"
                    | "elbowEdgeStyle"
                    | "sideToSideEdgeStyle"
                    | "topToBottomEdgeStyle"
                    | "entityRelationEdgeStyle"
                    | "segmentEdgeStyle"
                    | "isometricEdgeStyle"
            )
        }) && !style.flag("curved");
        let routed_hints = !cell.geometry.points.is_empty()
            && edge_style.is_some_and(|value| value != "none")
            && !route.is_drawio_edge_style();
        let custom_segment = route == ConnectorRoute::EntityRelation
            && style
                .get("segment")
                .is_some_and(|value| value.trim().parse::<f32>().ok() != Some(30.0));
        // Thinoda draws self-loops like draw.io's default `Loop` router; other
        // loop directions and sizes are not kept.
        let custom_loop = from == to
            && !matches!(from, Endpoint::Point(_))
            && (style
                .get("direction")
                .is_some_and(|value| !value.is_empty() && value != "west")
                || style
                    .get("segment")
                    .is_some_and(|value| value.trim().parse::<f32>().ok() != Some(10.0)));
        let (edge_shape, edge_shape_loss) = style.edge_shape();
        if style.formatting_loss(true, false, false, false)
            || routed_hints
            || approximated_router
            || custom_segment
            || custom_loop
            || edge_shape_loss
        {
            self.warn(ImportWarning::Formatting);
        }
        let (mut label, mut rich_label) = self.rich_label(cell);
        let label_style_loss = !label.is_empty() && style.label_style_loss(true, false);
        let label_placement = self.edge_label_placement(&cell.geometry);
        let mut extra_labels = Vec::new();
        for child in self.children[index].clone() {
            let child = &cells[child];
            if child.vertex && child.visible {
                let (text, rich_text) = self.rich_label(child);
                if text.is_empty()
                    && rich_text
                        .as_ref()
                        .is_none_or(|rich| rich.paragraphs.is_empty())
                {
                    continue;
                }
                if child.geometry.relative && !child.geometry.invalid_frame {
                    if extra_labels.len() >= MAX_CONNECTOR_LABELS {
                        self.warn(ImportWarning::Other);
                        continue;
                    }
                    if child_label_style_loss(&child.style, style) {
                        self.warn(ImportWarning::Formatting);
                    }
                    let g = &child.geometry;
                    let placement = self.label_placement(
                        g.x,
                        g.y,
                        g.offset.unwrap_or(Vec2::ZERO) + Vec2::new(g.width, g.height) * 0.5,
                    );
                    extra_labels.push(ConnectorLabel {
                        text: truncate_chars(text, MAX_BOARD_TEXT_CHARS, self.warnings),
                        placement,
                        rich_text: rich_text.map(Arc::new),
                    });
                } else {
                    // Absolute children have no edge-relative position; fold them.
                    self.warn(ImportWarning::Formatting);
                    if !label.is_empty()
                        || rich_label
                            .as_ref()
                            .is_some_and(|rich| !rich.paragraphs.is_empty())
                    {
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
        connector.route = route;
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
        connector.line_jumps = style.line_jumps();
        connector.edge_shape = edge_shape;
        connector.label = truncate_chars(label, MAX_BOARD_TEXT_CHARS, self.warnings);
        if rich_label
            .as_ref()
            .is_some_and(|rich| rich.validate(&connector.label).is_err())
        {
            self.warn(ImportWarning::Formatting);
            rich_label = None;
        }
        if !connector.label.is_empty()
            || rich_label
                .as_ref()
                .is_some_and(|rich| !rich.paragraphs.is_empty())
        {
            connector.label_placement = label_placement;
        }
        connector.extra_labels = extra_labels;
        connector.color = style.color("strokeColor", "strokeOpacity");
        connector.width = style.stroke_width();
        let mut object = CanvasObject::connector(connector);
        object.label_rich_text = rich_label.map(Arc::new);
        object.style.text_color = style.text_color();
        object.style.opacity = style.opacity();
        object.band = style.band();
        object.alt_text = cell.tooltip.clone().unwrap_or_default();
        if self.add_object(index, object, context).is_some() && detached {
            self.warn(ImportWarning::DetachedConnectors);
        }
    }

    /// The draw.io position of an edge's own label, unless it is the midpoint.
    fn edge_label_placement(&mut self, geometry: &Geometry) -> Option<ConnectorLabelPlacement> {
        let offset = geometry.offset.unwrap_or(Vec2::ZERO);
        if !geometry.relative {
            // mxGraph places absolute edge labels at the end-point midpoint.
            if offset != Vec2::ZERO {
                self.warn(ImportWarning::Formatting);
            }
            return None;
        }
        (geometry.x != 0.0 || geometry.y != 0.0 || offset != Vec2::ZERO)
            .then(|| self.label_placement(geometry.x, geometry.y, offset))
    }

    fn label_placement(&mut self, x: f32, y: f32, offset: Vec2) -> ConnectorLabelPlacement {
        let clamp = |value: f32, limit: f32| value.clamp(-limit, limit);
        let placement = ConnectorLabelPlacement {
            along: x.clamp(-1.0, 1.0),
            orthogonal: clamp(y, MAX_CONNECTOR_LABEL_OFFSET),
            offset: Vec2::new(
                clamp(offset.x, MAX_CONNECTOR_LABEL_OFFSET),
                clamp(offset.y, MAX_CONNECTOR_LABEL_OFFSET),
            ),
        };
        if placement.along != x || placement.orthogonal != y || placement.offset != offset {
            self.warn(ImportWarning::Formatting);
        }
        placement
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
        for (members, locked, source) in std::mem::take(&mut self.buckets) {
            if members.len() < 2 {
                if source.is_some() {
                    self.warn(ImportWarning::Other);
                }
                continue;
            }
            match self.sheet.group_board_members(&members) {
                Ok(group) => {
                    let source_id = source.and_then(|index| self.owner_source_id(index));
                    if let Some(group) = self.sheet.canvas.groups.iter_mut().find(|g| g.id == group)
                    {
                        group.locked = locked;
                        group.drawio_source_id = source_id;
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
                Some(Item::Object(id) | Item::Container(id)) => Some((*id, rank)),
                _ => None,
            })
            .collect();
        self.sheet
            .canvas
            .objects
            .sort_by_key(|object| order.get(&object.id).copied().unwrap_or(usize::MAX));
        self.sheet.canvas.normalize_layer_order();
        for id in std::mem::take(&mut self.locked_layers) {
            if self.sheet.set_board_layer_locked(id, true).is_err() {
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
    let roots: Vec<_> = model
        .children
        .iter()
        .filter(|child| child.name == "root")
        .collect();
    if page.dropped_content
        || !model.text.trim().is_empty()
        || roots.len() > 1
        || model.children.len() != roots.len()
        || roots
            .iter()
            .any(|root| !root.attrs.is_empty() || !root.text.trim().is_empty())
    {
        warnings.insert(ImportWarning::Other);
    }
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
    let container_cells: Vec<_> = cells
        .iter()
        .map(|cell| classify(&cell.style) == Kind::Container)
        .collect();
    let mut scene_container_cells = HashSet::new();
    for (child, cell) in cells.iter().enumerate() {
        if !container_cells[child] {
            continue;
        }
        if cell.style.flag("thinodaSceneContainer") {
            scene_container_cells.insert(child);
        }
        if let Some(parent) = cell.parent.as_deref().and_then(|id| index.get(id))
            && container_cells[*parent]
        {
            scene_container_cells.insert(child);
            scene_container_cells.insert(*parent);
        }
    }
    let mut import = PageImport {
        cells: &cells,
        index,
        children,
        scene_container_cells,
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
        inline_stencils: None,
        bundled_stencils: None,
        passthrough_cells: HashSet::new(),
        shape_effect_keys: HashMap::new(),
        passthrough_bytes: 0,
        locked_layers: Vec::new(),
    };
    import.run();
    if import.cells.iter().enumerate().any(|(i, cell)| {
        !import.passthrough_cells.contains(&i)
            && (cell.tooltip.is_some()
                || cell
                    .attributes
                    .iter()
                    .any(|(name, value)| match name.as_str() {
                        "link" => !matches!(import.items.get(&i), Some(Item::Card(_))),
                        "placeholders" => value != "0",
                        _ => true,
                    }))
    }) {
        // Cards keep links, but other non-object cells lose them and tooltips.
        import.warn(ImportWarning::Other);
    }
    if import.cells.iter().enumerate().any(|(i, cell)| {
        !import.passthrough_cells.contains(&i)
            && cell
                .attributes
                .iter()
                .any(|(name, value)| name == "placeholders" && value == "1")
            && [Some(cell.value.as_str()), cell.tooltip.as_deref()]
                .into_iter()
                .flatten()
                .any(|text| {
                    matches!(
                        thinoda_core::object_data::replace_placeholders(text, |_| Some(
                            String::new()
                        )),
                        std::borrow::Cow::Owned(_)
                    )
                })
    }) {
        // Topics and cards show their text as written.
        import.warn(ImportWarning::DrawioPlaceholdersUnsupported);
    }
    if let Some(extension) = model.attr("thinodaSankey") {
        import.apply_sankey(extension)?;
    }
    let mut represented_cells: HashSet<usize> = import
        .items
        .iter()
        .filter(|(_, item)| matches!(item, Item::Object(_) | Item::Container(_)))
        .map(|(index, _)| *index)
        .collect();
    represented_cells.extend(import.items.iter().filter_map(|(index, item)| match item {
        Item::Card(id) if import.sheet.topic(*id)?.drawio_source_id.is_some() => Some(*index),
        _ => None,
    }));
    represented_cells.extend(
        import
            .sheet
            .zones
            .iter()
            .filter_map(|zone| import.index.get(zone.drawio_source_id.as_deref()?).copied()),
    );
    let layer_cells: HashSet<usize> = import
        .sheet
        .canvas
        .layers
        .iter()
        .filter_map(|layer| {
            layer
                .source_id
                .as_deref()
                .and_then(|id| import.index.get(id).copied())
        })
        .collect();
    let mut sheet = import.finish();
    let group_sources: HashSet<_> = sheet
        .canvas
        .groups
        .iter()
        .filter_map(|group| group.drawio_source_id.as_deref())
        .collect();
    represented_cells.extend(
        cells
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| group_sources.contains(cell.id.as_str()).then_some(index)),
    );
    let (drawio, dropped) = page_passthrough(page, &cells, &represented_cells, &layer_cells);
    if dropped {
        warnings.insert(ImportWarning::Other);
    }
    sheet.drawio = drawio;
    Ok(sheet)
}

/// Record source IDs for represented owners so generated IDs cannot reuse
/// them after their original owner is deleted.
fn page_passthrough(
    page: &Page,
    cells: &[Cell],
    represented_cells: &HashSet<usize>,
    layer_cells: &HashSet<usize>,
) -> (Option<Arc<DrawioPagePassthrough>>, bool) {
    let budget = thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2;
    let mut drawio = DrawioPagePassthrough {
        diagram_id: page.id.clone(),
        model_attributes: Vec::new(),
        cell_ids: Vec::new(),
    };
    let mut seen = HashSet::new();
    let mut dropped = page.dropped_id;
    let mut bytes = drawio.byte_len();
    for (name, value) in &page.model.attrs {
        if RESERVED_MODEL_ATTRIBUTES.contains(&name.as_str()) {
            continue;
        }
        let valid = thinoda_core::drawio_passthrough::valid_attribute_name(name)
            && !value
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
            && seen.insert(name.as_str());
        let next = bytes.saturating_add(name.len()).saturating_add(value.len());
        if !valid || next > budget {
            dropped = true;
            continue;
        }
        bytes = next;
        drawio.model_attributes.push((name.clone(), value.clone()));
    }
    for (index, cell) in cells.iter().enumerate() {
        let id = &cell.id;
        if !(represented_cells.contains(&index) || layer_cells.contains(&index))
            || id.starts_with('\u{0}')
        {
            continue;
        }
        if matches!(id.as_str(), "0" | "1")
            || id.len() > thinoda_core::drawio_passthrough::MAX_SOURCE_ID_BYTES
            || id.chars().any(char::is_control)
            || drawio.cell_ids.len() >= thinoda_core::drawio_passthrough::MAX_PAGE_CELL_IDS
            || bytes.saturating_add(id.len()) > budget
        {
            dropped = true;
            continue;
        }
        bytes += id.len();
        drawio.cell_ids.push(id.clone());
    }
    drawio.cell_ids.sort_unstable();
    debug_assert!(drawio.validate().is_ok());
    ((!drawio.is_empty()).then(|| Arc::new(drawio)), dropped)
}

/// Imports every page of a draw.io document as a Freeform board sheet.
pub(crate) fn import(bytes: &[u8]) -> Result<ImportReport, IoError> {
    import_with_first_page_warnings(bytes).map(|(report, _)| report)
}

/// Imports all pages, retaining the first page's warnings separately for clipboard paste.
pub(crate) fn import_with_first_page_warnings(
    bytes: &[u8],
) -> Result<(ImportReport, Vec<ImportWarning>), IoError> {
    let document = read_pages(bytes)?;
    let dropped_document_attributes = document.dropped_attributes;
    let dropped_document_content = document.dropped_content;
    let pages = document.pages;
    let mut warnings = BTreeSet::new();
    let (first, rest) = pages
        .split_first()
        .ok_or_else(|| invalid("draw.io document contains no pages"))?;
    let mut workbook = Workbook::new(import_page(first, 1, &mut warnings)?);
    if dropped_document_attributes || dropped_document_content {
        warnings.insert(ImportWarning::Other);
    }
    let first_page_warnings = warnings.iter().copied().collect();
    for (i, page) in rest.iter().enumerate() {
        workbook
            .sheets
            .push(import_page(page, i + 2, &mut warnings)?);
    }
    workbook.drawio = document.passthrough;
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
    fn from_sheet(
        sheet: &Sheet,
        node_ids: &HashMap<TopicId, TopicId>,
        edge_ids: &HashMap<CanvasObjectId, CanvasObjectId>,
    ) -> Result<Self, IoError> {
        let values = thinoda_core::sankey::evaluate(sheet).map_err(IoError::Invalid)?;
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        let exported = exported_topics(sheet, &layout);
        if node_ids.keys().any(|id| !exported.contains(id)) {
            return Err(invalid(
                "Expand all Sankey endpoint and settings topics before exporting to draw.io.",
            ));
        }
        let mut extension = Self {
            version: 2,
            settings: sheet.sankey.clone().unwrap_or_default(),
            nodes: sheet
                .topics
                .iter()
                .filter(|(id, _)| node_ids.contains_key(id))
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
        };
        let node = |id: TopicId| {
            node_ids
                .get(&id)
                .copied()
                .ok_or_else(|| invalid("Missing exported Sankey node ID"))
        };
        extension.nodes = extension
            .nodes
            .into_iter()
            .map(|(id, notes)| Ok((node(id)?, notes)))
            .collect::<Result<_, IoError>>()?;
        extension.settings.sources = extension
            .settings
            .sources
            .into_iter()
            .map(|(id, value)| Ok((node(id)?, value)))
            .collect::<Result<_, IoError>>()?;
        extension.settings.stages = extension
            .settings
            .stages
            .into_iter()
            .map(|(id, value)| Ok((node(id)?, value)))
            .collect::<Result<_, IoError>>()?;
        extension.links = extension
            .links
            .into_iter()
            .map(|(id, mut link)| {
                link.from = node(link.from)?;
                link.to = node(link.to)?;
                let id = edge_ids
                    .get(&id)
                    .copied()
                    .ok_or_else(|| invalid("Missing exported Sankey edge ID"))?;
                Ok((id, link))
            })
            .collect::<Result<_, IoError>>()?;
        Ok(extension)
    }
}

impl PageImport<'_> {
    fn apply_sankey(&mut self, json: &str) -> Result<(), IoError> {
        if json.len() > MAX_BYTES {
            return Err(invalid("Sankey extension exceeds import size limit"));
        }
        let sheet = self.sheet.clone();
        let items = self.items.clone();
        match self.apply_sankey_candidate(json) {
            Ok(true) => {
                // Shape-to-card conversion cannot retain the shape.
                self.warn(ImportWarning::Formatting);
            }
            Ok(false) => {}
            Err(_) => {
                self.sheet = sheet;
                self.items = items;
                self.warn(ImportWarning::DrawioSankeyStale);
            }
        }
        Ok(())
    }

    fn apply_sankey_candidate(&mut self, json: &str) -> Result<bool, IoError> {
        let extension: SankeyExtension = serde_json::from_str(json)?;
        if extension.version != 2 {
            return Err(IoError::NewerVersion(format!(
                "draw.io Sankey extension {}",
                extension.version
            )));
        }
        let mut topics = HashMap::new();
        let mut objects = HashMap::new();
        let structured_objects: HashSet<_> = self
            .zones
            .iter()
            .flat_map(|(_, _, objects)| objects.iter().copied())
            .chain(
                self.buckets
                    .iter()
                    .flat_map(|(members, _, _)| members.iter())
                    .chain(self.locked.iter())
                    .filter_map(|member| match member {
                        BoardMember::Object(id) => Some(*id),
                        BoardMember::Card(_) => None,
                    }),
            )
            .collect();
        let mut converted = false;
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
                    if !o.alt_text.is_empty()
                        || o.data.as_ref().is_some_and(|data| {
                            !data.tags.is_empty()
                                || !data.properties.is_empty()
                                || data.placeholders
                        })
                        || o.layer.is_some()
                        || o.locked
                        || self.sheet.canvas.scene.parents.contains_key(&id)
                        || structured_objects.contains(&id)
                        || o.drawio.as_ref().is_some_and(|data| {
                            !data.style.is_empty() || !data.attributes.is_empty()
                        })
                    {
                        return Err(invalid(
                            "Sankey node has object data or structure that a card cannot retain",
                        ));
                    }
                    let mut card = Card::new(CardKind::Text, o.rect.size);
                    card.color = o.style.fill;
                    let topic = self
                        .sheet
                        .add_card(label, o.rect.center(), card)
                        .map_err(IoError::Invalid)?;
                    if let Some(t) = self.sheet.topic_mut(topic) {
                        t.style.fill = card.color;
                        t.style.text_color = o.style.text_color;
                        t.style.align = self.cells[index].style.text_align();
                        t.hyperlink = o.data.as_ref().and_then(|data| data.link.clone());
                        t.drawio_source_id =
                            o.drawio.as_ref().and_then(|data| data.source_id.clone());
                        t.inline_text = o.label_rich_text.as_deref().cloned().unwrap_or_default();
                    }
                    self.sheet.canvas.objects.retain(|o| o.id != id);
                    self.items.insert(index, Item::Card(topic));
                    objects.insert(id, topic);
                    converted = true;
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
        Ok(converted)
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

fn html_rich_label(
    text: &str,
    rich: &thinoda_core::InlineText,
    whole: Option<&thinoda_core::label_style::LabelStyle>,
) -> String {
    use thinoda_core::{InlineBaseline, InlineTextStyle, ListKind};

    fn span(
        text: &str,
        style: &InlineTextStyle,
        whole: Option<&LabelStyle>,
        boundary: bool,
    ) -> String {
        let mut css = String::new();
        if let Some(value) = style.bold {
            css.push_str(if value {
                "font-weight:bold;"
            } else {
                "font-weight:normal;"
            });
        }
        if let Some(value) = style.italic {
            css.push_str(if value {
                "font-style:italic;"
            } else {
                "font-style:normal;"
            });
        }
        let mut inherited = String::new();
        if style.underline.is_some() || style.strikethrough.is_some() {
            let underline = style
                .underline
                .unwrap_or_else(|| whole.is_some_and(|style| style.underline));
            let strike = style
                .strikethrough
                .unwrap_or_else(|| whole.is_some_and(|style| style.strikethrough));
            let values: Vec<_> = [
                underline.then_some("underline"),
                strike.then_some("line-through"),
            ]
            .into_iter()
            .flatten()
            .collect();
            css.push_str("text-decoration:");
            if values.is_empty() {
                css.push_str("none");
            } else {
                css.push_str(&values.join(" "));
            }
            css.push(';');
            if style.underline.is_none() {
                inherited.push_str(" data-thinoda-underline=\"inherit\"");
            }
            if style.strikethrough.is_none() {
                inherited.push_str(" data-thinoda-strike=\"inherit\"");
            }
        }
        if let Some(family) = &style.font_family {
            let mut quoted = String::new();
            for ch in family.chars() {
                if ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_' | ',' | '.') {
                    quoted.push(ch);
                } else {
                    quoted.push_str(&format!("\\{:x} ", u32::from(ch)));
                }
            }
            css.push_str(&format!("font-family:'{quoted}';"));
        }
        if let Some(size) = style.font_size {
            css.push_str(&format!("font-size:{}px;", num(size)));
        }
        if let Some(color) = style.color {
            css.push_str(&format!(
                "color:rgba({},{},{},{:.6});",
                color.r,
                color.g,
                color.b,
                f32::from(color.a) / 255.0
            ));
        }
        if let Some(baseline) = style.baseline {
            css.push_str(match baseline {
                InlineBaseline::Normal => "vertical-align:baseline;",
                InlineBaseline::Subscript => "vertical-align:sub;",
                InlineBaseline::Superscript => "vertical-align:super;",
            });
        }
        let boundary = if boundary {
            " data-thinoda-break=\"1\""
        } else {
            ""
        };
        format!(
            "<span style=\"{}\"{inherited}{boundary}>{}</span>",
            html(&css),
            html(text)
        )
    }

    let chars: Vec<_> = text.chars().collect();
    let mut out = String::new();
    let mut start = 0usize;
    let mut run_index = 0usize;
    let mut active_list = None;
    for (index, paragraph) in text.split('\n').enumerate() {
        let end = start + paragraph.chars().count();
        let list = rich
            .paragraphs
            .iter()
            .find(|item| item.index as usize == index)
            .map(|item| item.list);
        if active_list != list {
            match active_list {
                Some(ListKind::Bulleted) => out.push_str("</ul>"),
                Some(ListKind::Numbered) => out.push_str("</ol>"),
                None => {}
            }
            match list {
                Some(ListKind::Bulleted) => out.push_str("<ul>"),
                Some(ListKind::Numbered) => out.push_str("<ol>"),
                None => {}
            }
            active_list = list;
        }
        out.push_str(if list.is_some() { "<li>" } else { "<div>" });
        let mut cursor = start;
        while cursor < end {
            while run_index < rich.runs.len() && rich.runs[run_index].end as usize <= cursor {
                run_index += 1;
            }
            let run = rich.runs.get(run_index);
            let styled = run.is_some_and(|run| run.start as usize <= cursor);
            let next = if styled {
                end.min(run.map_or(end, |run| run.end as usize))
            } else {
                end.min(run.map_or(end, |run| run.start as usize))
            };
            let part: String = chars[cursor..next].iter().collect();
            if let Some(run) = run.filter(|_| styled) {
                out.push_str(&span(&part, &run.style, whole, false));
            } else {
                out.push_str(&html(&part));
            }
            cursor = next;
        }
        if end < chars.len() {
            while run_index < rich.runs.len() && rich.runs[run_index].end as usize <= end {
                run_index += 1;
            }
            if let Some(run) = rich
                .runs
                .get(run_index)
                .filter(|run| run.start as usize <= end)
            {
                out.push_str(&span("", &run.style, whole, true));
            } else {
                out.push_str("<span data-thinoda-break=\"1\"></span>");
            }
        }
        out.push_str(if list.is_some() { "</li>" } else { "</div>" });
        start = end + 1;
    }
    match active_list {
        Some(ListKind::Bulleted) => out.push_str("</ul>"),
        Some(ListKind::Numbered) => out.push_str("</ol>"),
        None => {}
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

/// draw.io keys for a shape's native effects. Thinoda frames are never turned
/// by `direction` on export, so the flips are written as stored.
fn shape_effect_style(out: &mut String, style: &ObjectStyle) {
    if let Some(dash) = style.dash {
        out.push_str("dashed=1;");
        if dash.pattern != thinoda_core::shape_style::DashPattern::default() {
            out.push_str(&format!("dashPattern={};", dash.pattern.to_drawio()));
        }
        if dash.fixed {
            out.push_str("fixDash=1;");
        }
    }
    if let Some(gradient) = style.gradient {
        color_keys(
            out,
            "gradientColor",
            "",
            Some(Color {
                a: 255,
                ..gradient.color
            }),
        );
        if gradient.direction != thinoda_core::shape_style::GradientDirection::South {
            out.push_str(&format!(
                "gradientDirection={};",
                gradient.direction.drawio_name()
            ));
        }
    }
    if !style.fill_pattern.is_solid() {
        out.push_str(&format!("fillStyle={};", style.fill_pattern.drawio_name()));
    }
    for (key, set) in [
        ("shadow", style.shadow),
        ("glass", style.glass),
        ("flipH", style.flip_h),
        ("flipV", style.flip_v),
    ] {
        if set {
            out.push_str(&format!("{key}=1;"));
        }
    }
}

/// Appends a cell's preserved draw.io style keys after the generated ones.
/// Generated keys always win, and dependent keys are dropped when the key they
/// refine is gone (e.g. `dashPattern` once the line is no longer dashed).
fn passthrough_style(out: &mut String, drawio: Option<&Passthrough>) {
    let Some(drawio) = drawio.filter(|drawio| !drawio.style.is_empty()) else {
        return;
    };
    let mut present: HashMap<String, String> = out
        .split(';')
        .filter_map(|entry| entry.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    let rest: Vec<_> = drawio
        .style
        .iter()
        .filter(|(key, _)| !present.contains_key(key))
        .collect();
    for (key, value) in &rest {
        present.insert(key.clone(), value.clone());
    }
    for (key, value) in rest {
        let orphan = DEPENDENT_STYLE_KEYS
            .iter()
            .find(|(dependent, _)| dependent == key)
            .is_some_and(|(_, owner)| {
                present
                    .get(*owner)
                    .is_none_or(|value| matches!(value.as_str(), "0" | "none" | ""))
            });
        if !orphan {
            out.push_str(&format!("{key}={value};"));
        }
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
    /// draw.io cell id of each canvas object.
    cell_ids: HashMap<CanvasObjectId, String>,
    /// Preserved UserObject attributes for the next cell written.
    attributes: Vec<(String, String)>,
    /// Position of the next edge's own label, unless it is the midpoint.
    label_placement: Option<ConnectorLabelPlacement>,
    /// Every cell id written or reserved, for generated label cell ids.
    used_ids: HashSet<String>,
    /// draw.io layer cell id of each named board layer.
    layer_cells: HashMap<thinoda_core::BoardLayerId, String>,
    /// Generated draw.io cell IDs for zones and groups.
    zone_cells: HashMap<ZoneId, String>,
    group_cells: HashMap<CanvasGroupId, String>,
    topic_cells: HashMap<TopicId, String>,
    sankey_topic_ids: HashMap<TopicId, TopicId>,
    sankey_edge_ids: HashMap<CanvasObjectId, CanvasObjectId>,
    /// Generated IDs for mind-map boundaries, links, relationships and summaries.
    map_cells: HashMap<String, String>,
    /// Row cell ids and absolute row origins of each written table.
    table_rows: HashMap<CanvasObjectId, Vec<(String, Vec2)>>,
}

/// Layer cell ids of named board layers: the imported draw.io id when it is
/// unique and not reserved, otherwise `l{id}`.
fn layer_cell_ids(
    sheet: &Sheet,
    reserved: &HashSet<String>,
) -> HashMap<thinoda_core::BoardLayerId, String> {
    let tombstones: HashSet<&str> = sheet
        .drawio
        .iter()
        .flat_map(|page| page.cell_ids.iter().map(String::as_str))
        .collect();
    let mut used = reserved.clone();
    let mut ids = HashMap::new();
    for layer in &sheet.canvas.layers {
        if let Some(source) = &layer.source_id
            && used.insert(source.clone())
        {
            ids.insert(layer.id, source.clone());
        }
    }
    for layer in &sheet.canvas.layers {
        if ids.contains_key(&layer.id) {
            continue;
        }
        let base = format!("l{}", layer.id.0);
        let mut id = base.clone();
        let mut n = 1;
        while used.contains(&id) || tombstones.contains(id.as_str()) {
            id = format!("{base}-{n}");
            n += 1;
        }
        used.insert(id.clone());
        ids.insert(layer.id, id);
    }
    ids
}

/// Cell ids of canvas objects: the imported draw.io id when it is still unique
/// and not one of the ids generated for other cells (`reserved`), otherwise
/// `o{id}`. Sankey edge IDs are allocated with their semantic extension.
fn object_cell_ids(
    sheet: &Sheet,
    reserved: &HashSet<String>,
    sankey_ids: &HashMap<CanvasObjectId, CanvasObjectId>,
) -> HashMap<CanvasObjectId, String> {
    let tombstones: HashSet<&str> = sheet
        .drawio
        .iter()
        .flat_map(|page| page.cell_ids.iter().map(String::as_str))
        .collect();
    let mut ids = HashMap::new();
    let mut used = reserved.clone();
    for (object, exported) in sankey_ids {
        let id = format!("o{}", exported.0);
        used.insert(id.clone());
        ids.insert(*object, id);
    }
    for object in &sheet.canvas.objects {
        let Some(source) = object
            .drawio
            .as_ref()
            .and_then(|drawio| drawio.source_id.as_ref())
        else {
            continue;
        };
        if !ids.contains_key(&object.id) && used.insert(source.clone()) {
            ids.insert(object.id, source.clone());
        }
    }
    for object in &sheet.canvas.objects {
        if ids.contains_key(&object.id) {
            continue;
        }
        let base = format!("o{}", object.id.0);
        let mut id = base.clone();
        let mut n = 1;
        while used.contains(&id) || tombstones.contains(id.as_str()) {
            id = format!("{base}-{n}");
            n += 1;
        }
        used.insert(id.clone());
        ids.insert(object.id, id);
    }
    ids
}

fn next_numeric_cell_id(
    prefix: &str,
    cursor: &mut u64,
    reserved: &mut HashSet<String>,
    unavailable: &HashSet<String>,
) -> Result<u64, IoError> {
    loop {
        let candidate = *cursor;
        let id = format!("{prefix}{candidate}");
        if !unavailable.contains(&id) && reserved.insert(id) {
            *cursor = candidate.saturating_add(1);
            return Ok(candidate);
        }
        *cursor = candidate
            .checked_add(1)
            .ok_or_else(|| invalid("draw.io cell ID space exhausted"))?;
    }
}

impl<'a> PageWriter<'a> {
    fn cell_id(&self, id: CanvasObjectId) -> String {
        self.cell_ids
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("o{}", id.0))
    }
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
            // Hidden layers are still exported, so measure them as if shown.
            let shown;
            let scene_sheet = if sheet.canvas.layers.iter().any(|layer| !layer.visible) {
                let mut copy = sheet.clone();
                for layer in &mut copy.canvas.layers {
                    layer.visible = true;
                }
                shown = copy;
                &shown
            } else {
                sheet
            };
            for item in thinoda_core::build_scene(scene_sheet, &layout).items {
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
            cell_ids: HashMap::new(),
            attributes: Vec::new(),
            label_placement: None,
            used_ids: HashSet::new(),
            layer_cells: HashMap::new(),
            zone_cells: HashMap::new(),
            group_cells: HashMap::new(),
            topic_cells: HashMap::new(),
            sankey_topic_ids: HashMap::new(),
            sankey_edge_ids: HashMap::new(),
            map_cells: HashMap::new(),
            table_rows: HashMap::new(),
        };
        let mut reserved: HashSet<String> = ["0", "1"].map(String::from).into();
        let mut unavailable: HashSet<String> = sheet
            .drawio
            .iter()
            .flat_map(|page| page.cell_ids.iter().cloned())
            .collect();
        unavailable.extend(
            sheet
                .canvas
                .objects
                .iter()
                .filter_map(|object| object.drawio.as_ref()?.source_id.clone()),
        );
        unavailable.extend(
            sheet
                .canvas
                .layers
                .iter()
                .filter_map(|layer| layer.source_id.clone()),
        );
        unavailable.extend(
            sheet
                .zones
                .iter()
                .filter_map(|zone| zone.drawio_source_id.clone()),
        );
        unavailable.extend(
            sheet
                .topics
                .values()
                .filter_map(|topic| topic.drawio_source_id.clone()),
        );
        unavailable.extend(
            sheet
                .canvas
                .groups
                .iter()
                .filter_map(|group| group.drawio_source_id.clone()),
        );
        let mut zone_sources: HashMap<&str, usize> = HashMap::new();
        for source in sheet
            .zones
            .iter()
            .filter_map(|zone| zone.drawio_source_id.as_deref())
        {
            *zone_sources.entry(source).or_default() += 1;
        }
        let other_sources: HashSet<&str> = sheet
            .canvas
            .objects
            .iter()
            .filter_map(|object| object.drawio.as_ref()?.source_id.as_deref())
            .chain(
                sheet
                    .canvas
                    .layers
                    .iter()
                    .filter_map(|layer| layer.source_id.as_deref()),
            )
            .chain(
                sheet
                    .topics
                    .values()
                    .filter_map(|topic| topic.drawio_source_id.as_deref()),
            )
            .chain(
                sheet
                    .canvas
                    .groups
                    .iter()
                    .filter_map(|group| group.drawio_source_id.as_deref()),
            )
            .collect();
        for zone in &sheet.zones {
            if let Some(source) = zone.drawio_source_id.as_deref()
                && zone_sources.get(source) == Some(&1)
                && !other_sources.contains(source)
                && reserved.insert(source.to_owned())
            {
                writer.zone_cells.insert(zone.id, source.to_owned());
            }
        }
        let mut sankey_nodes = writer.flows.nodes.clone();
        if let Some(settings) = &sheet.sankey {
            sankey_nodes.extend(settings.sources.keys().copied());
            sankey_nodes.extend(settings.stages.keys().copied());
        }
        let mut topic_sources: HashMap<&str, usize> = HashMap::new();
        for source in sheet
            .topics
            .values()
            .filter_map(|topic| topic.drawio_source_id.as_deref())
        {
            *topic_sources.entry(source).or_default() += 1;
        }
        let other_topic_sources: HashSet<&str> = sheet
            .canvas
            .objects
            .iter()
            .filter_map(|object| object.drawio.as_ref()?.source_id.as_deref())
            .chain(
                sheet
                    .canvas
                    .layers
                    .iter()
                    .filter_map(|layer| layer.source_id.as_deref()),
            )
            .chain(
                sheet
                    .zones
                    .iter()
                    .filter_map(|zone| zone.drawio_source_id.as_deref()),
            )
            .chain(
                sheet
                    .canvas
                    .groups
                    .iter()
                    .filter_map(|group| group.drawio_source_id.as_deref()),
            )
            .collect();
        for topic in sheet.topics.values() {
            let Some(source) = topic.drawio_source_id.as_deref() else {
                continue;
            };
            if topic_sources.get(source) != Some(&1) || other_topic_sources.contains(source) {
                continue;
            }
            if sankey_nodes.contains(&topic.id) {
                let Some(number) = source
                    .strip_prefix('t')
                    .and_then(|id| id.parse::<u64>().ok())
                    .filter(|number| format!("t{number}") == source)
                else {
                    continue;
                };
                if reserved.insert(source.to_owned()) {
                    writer.sankey_topic_ids.insert(topic.id, TopicId(number));
                    writer.topic_cells.insert(topic.id, source.to_owned());
                }
            } else if reserved.insert(source.to_owned()) {
                writer.topic_cells.insert(topic.id, source.to_owned());
            }
        }
        let mut group_sources: HashMap<&str, usize> = HashMap::new();
        for source in sheet
            .canvas
            .groups
            .iter()
            .filter_map(|group| group.drawio_source_id.as_deref())
        {
            *group_sources.entry(source).or_default() += 1;
        }
        let other_group_sources: HashSet<&str> = sheet
            .canvas
            .objects
            .iter()
            .filter_map(|object| object.drawio.as_ref()?.source_id.as_deref())
            .chain(
                sheet
                    .canvas
                    .layers
                    .iter()
                    .filter_map(|layer| layer.source_id.as_deref()),
            )
            .chain(
                sheet
                    .zones
                    .iter()
                    .filter_map(|zone| zone.drawio_source_id.as_deref()),
            )
            .chain(
                sheet
                    .topics
                    .values()
                    .filter_map(|topic| topic.drawio_source_id.as_deref()),
            )
            .collect();
        for group in &sheet.canvas.groups {
            if let Some(source) = group.drawio_source_id.as_deref()
                && group_sources.get(source) == Some(&1)
                && !other_group_sources.contains(source)
                && reserved.insert(source.to_owned())
            {
                writer.group_cells.insert(group.id, source.to_owned());
            }
        }
        for id in &sankey_nodes {
            let cell = format!("t{}", id.0);
            if !writer.sankey_topic_ids.contains_key(id)
                && !unavailable.contains(&cell)
                && reserved.insert(cell)
            {
                writer.sankey_topic_ids.insert(*id, *id);
            }
        }
        let mut topic_unavailable = unavailable.clone();
        topic_unavailable.extend(sheet.topics.keys().map(|id| format!("t{}", id.0)));
        let mut topic_cursor = 1;
        for id in &sankey_nodes {
            if !writer.sankey_topic_ids.contains_key(id) {
                let new = next_numeric_cell_id(
                    "t",
                    &mut topic_cursor,
                    &mut reserved,
                    &topic_unavailable,
                )?;
                writer.sankey_topic_ids.insert(*id, TopicId(new));
            }
        }
        let mut object_sources: HashMap<&str, usize> = HashMap::new();
        for source in sheet
            .canvas
            .objects
            .iter()
            .filter_map(|object| object.drawio.as_ref()?.source_id.as_deref())
        {
            *object_sources.entry(source).or_default() += 1;
        }
        let layer_sources: HashSet<&str> = sheet
            .canvas
            .layers
            .iter()
            .filter_map(|layer| layer.source_id.as_deref())
            .collect();
        let sankey_objects: Vec<_> = sheet
            .canvas
            .objects
            .iter()
            .filter(|object| {
                matches!(&object.kind, CanvasObjectKind::Connector(c) if c.sankey.is_some())
            })
            .collect();
        for object in &sankey_objects {
            let source = object
                .drawio
                .as_ref()
                .and_then(|passthrough| passthrough.source_id.as_deref());
            if let Some(source) = source
                && let Some(number) = source
                    .strip_prefix('o')
                    .and_then(|id| id.parse::<u64>().ok())
                && format!("o{number}") == source
                && object_sources.get(source) == Some(&1)
                && !layer_sources.contains(source)
                && reserved.insert(source.to_owned())
            {
                writer
                    .sankey_edge_ids
                    .insert(object.id, CanvasObjectId(number));
            }
        }
        for object in &sankey_objects {
            let cell = format!("o{}", object.id.0);
            if !writer.sankey_edge_ids.contains_key(&object.id)
                && !unavailable.contains(&cell)
                && reserved.insert(cell)
            {
                writer.sankey_edge_ids.insert(object.id, object.id);
            }
        }
        let mut edge_cursor = 1;
        for object in &sankey_objects {
            if let std::collections::hash_map::Entry::Vacant(entry) =
                writer.sankey_edge_ids.entry(object.id)
            {
                let new = next_numeric_cell_id("o", &mut edge_cursor, &mut reserved, &unavailable)?;
                entry.insert(CanvasObjectId(new));
            }
        }
        let mut generated_id = |base: String| {
            let mut id = base.clone();
            let mut suffix = 1;
            while reserved.contains(&id) || unavailable.contains(&id) {
                id = format!("{base}-{suffix}");
                suffix += 1;
            }
            reserved.insert(id.clone());
            id
        };
        for id in sheet.topics.keys() {
            if writer.topic_cells.contains_key(id) {
                continue;
            }
            let base = format!("t{}", id.0);
            let cell = if let Some(mapped) = writer.sankey_topic_ids.get(id) {
                format!("t{}", mapped.0)
            } else {
                generated_id(base)
            };
            writer.topic_cells.insert(*id, cell);
        }
        let map_ids = writer
            .layout
            .boundaries
            .iter()
            .map(|boundary| format!("b{}", boundary.id.0))
            .chain((0..writer.layout.connectors.len()).map(|n| format!("e{n}")))
            .chain(
                writer
                    .layout
                    .relationships
                    .iter()
                    .map(|relationship| format!("r{}", relationship.id.0)),
            )
            .chain(
                writer
                    .layout
                    .summaries
                    .iter()
                    .map(|summary| format!("s{}", summary.id.0)),
            );
        for id in map_ids {
            writer.map_cells.insert(id.clone(), generated_id(id));
        }
        for zone in &sheet.zones {
            writer
                .zone_cells
                .entry(zone.id)
                .or_insert_with(|| generated_id(format!("z{}", zone.id.0)));
        }
        for group in &sheet.canvas.groups {
            writer
                .group_cells
                .entry(group.id)
                .or_insert_with(|| generated_id(format!("g{}", group.id.0)));
        }
        writer.layer_cells = layer_cell_ids(sheet, &reserved);
        reserved.extend(writer.layer_cells.values().cloned());
        writer.cell_ids = object_cell_ids(sheet, &reserved, &writer.sankey_edge_ids);
        reserved.extend(writer.cell_ids.values().cloned());
        reserved.extend(
            sheet
                .drawio
                .iter()
                .flat_map(|page| page.cell_ids.iter().cloned()),
        );
        writer.used_ids = reserved;
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
            let parent = match (
                members.iter().find_map(|m| writer.member_layer_cell(*m)),
                &zones.into_iter().collect::<Vec<_>>()[..],
            ) {
                (Some(layer), _) => layer,
                (None, [Some(zone)]) => writer.zone_cells[zone].clone(),
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

    /// The layer cell of a member on a named board layer.
    fn member_layer_cell(&self, member: BoardMember) -> Option<String> {
        let BoardMember::Object(id) = member else {
            return None;
        };
        let layer = self.sheet.canvas.object(id)?.layer?;
        self.layer_cells.get(&layer).cloned()
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
        if let BoardMember::Object(id) = member
            && let Some(parent) = self.sheet.canvas.scene.parents.get(&id)
        {
            let origin = self
                .sheet
                .canvas
                .scene
                .collapsed
                .get(parent)
                .map(|bounds| bounds.min)
                .or_else(|| {
                    self.sheet
                        .canvas
                        .object(*parent)
                        .map(|object| object.rect.min)
                })
                .unwrap_or(Vec2::ZERO);
            return (self.cell_id(*parent), origin);
        }
        if let Some(group) = self.member_group(member) {
            return (self.group_cells[&group].clone(), self.groups[&group].1.min);
        }
        // draw.io layers hold top-level cells, so a layered object leaves its frame.
        if let Some(layer) = self.member_layer_cell(member) {
            return (layer, Vec2::ZERO);
        }
        match self.member_zone(member) {
            Some(zone) => (self.zone_cells[&zone].clone(), self.zone_origin(zone)),
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
        self.vertex_with_alternate_bounds(id, label, style, parent, rect, None, false, user, None);
    }

    #[allow(clippy::too_many_arguments)]
    fn vertex_with_alternate_bounds(
        &mut self,
        id: &str,
        label: &str,
        style: &str,
        parent: &str,
        rect: Rect,
        alternate_bounds: Option<Rect>,
        collapsed: bool,
        user: Option<(&str, &str)>,
        rich: Option<(&thinoda_core::InlineText, Option<&LabelStyle>)>,
    ) {
        let collapsed = if collapsed { " collapsed=\"1\"" } else { "" };
        let geometry = match alternate_bounds {
            Some(bounds) => format!(
                "<mxRectangle x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"alternateBounds\"/>",
                num(bounds.min.x),
                num(bounds.min.y),
                num(bounds.size.x),
                num(bounds.size.y)
            ),
            None => String::new(),
        };
        let geometry = if alternate_bounds.is_some() {
            format!(
                "<mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\">{geometry}</mxGeometry>",
                num(rect.min.x),
                num(rect.min.y),
                num(rect.size.x),
                num(rect.size.y)
            )
        } else {
            format!(
                "<mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\"/>",
                num(rect.min.x),
                num(rect.min.y),
                num(rect.size.x),
                num(rect.size.y)
            )
        };
        let label = esc(&rich.map_or_else(
            || html(label),
            |(rich, style)| html_rich_label(label, rich, style),
        ));
        let style = esc(style);
        let extra = self.take_attributes();
        let user = user.or_else(|| (!extra.is_empty()).then_some(("", "")));
        match user {
            Some((tooltip, link)) => {
                self.out.push_str(&format!("<UserObject label=\"{label}\" id=\"{id}\""));
                if !tooltip.is_empty() {
                    self.out.push_str(&format!(" tooltip=\"{}\"", esc(tooltip)));
                }
                if !link.is_empty() {
                    self.out.push_str(&format!(" link=\"{}\"", esc(link)));
                }
                self.out.push_str(&extra);
                self.out.push_str(&format!(
                    "><mxCell style=\"{style}\" vertex=\"1\" parent=\"{parent}\"{collapsed}>{geometry}</mxCell></UserObject>\n"
                ));
            }
            None => self.out.push_str(&format!(
                "<mxCell id=\"{id}\" value=\"{label}\" style=\"{style}\" vertex=\"1\" parent=\"{parent}\"{collapsed}>{geometry}</mxCell>\n"
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
        rich: Option<&thinoda_core::InlineText>,
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
        let mut position = String::new();
        if let Some(placement) = self.label_placement.take() {
            for (key, value) in [("x", placement.along), ("y", placement.orthogonal)] {
                if value != 0.0 {
                    position.push_str(&format!(" {key}=\"{}\"", num(value)));
                }
            }
            if placement.offset != Vec2::ZERO {
                geometry.push_str(&format!(
                    "<mxPoint x=\"{}\" y=\"{}\" as=\"offset\"/>",
                    num(placement.offset.x),
                    num(placement.offset.y)
                ));
            }
        }
        let cell = format!(
            "style=\"{}\" edge=\"1\" parent=\"{parent}\"{terminals}><mxGeometry{position} relative=\"1\" as=\"geometry\">{geometry}</mxGeometry></mxCell>",
            esc(style)
        );
        let label =
            esc(&rich.map_or_else(|| html(label), |rich| html_rich_label(label, rich, None)));
        let extra = self.take_attributes();
        if tooltip.is_empty() && extra.is_empty() {
            self.out
                .push_str(&format!("<mxCell id=\"{id}\" value=\"{label}\" {cell}\n"));
        } else {
            let tooltip = if tooltip.is_empty() {
                String::new()
            } else {
                format!(" tooltip=\"{}\"", esc(tooltip))
            };
            self.out.push_str(&format!(
                "<UserObject label=\"{label}\"{tooltip} id=\"{id}\"{extra}><mxCell {cell}</UserObject>\n"
            ));
        }
    }

    /// Writes further connector labels as draw.io edge-label children.
    fn edge_labels(&mut self, edge_id: &str, labels: &[ConnectorLabel], text_color: Option<Color>) {
        for (n, label) in labels.iter().enumerate() {
            let mut id = format!("{edge_id}-label-{}", n + 1);
            while self.used_ids.contains(&id) {
                id.push('x');
            }
            self.used_ids.insert(id.clone());
            let mut style = format!(
                "edgeLabel;html=1;align=center;verticalAlign=middle;resizable=0;points=[];fontSize={};",
                num(CANVAS_LABEL_FONT_SIZE)
            );
            color_keys(&mut style, "fontColor", "textOpacity", text_color);
            let placement = label.placement;
            let mut position = String::new();
            for (key, value) in [("x", placement.along), ("y", placement.orthogonal)] {
                if value != 0.0 {
                    position.push_str(&format!(" {key}=\"{}\"", num(value)));
                }
            }
            let offset = if placement.offset == Vec2::ZERO {
                String::new()
            } else {
                format!(
                    "<mxPoint x=\"{}\" y=\"{}\" as=\"offset\"/>",
                    num(placement.offset.x),
                    num(placement.offset.y)
                )
            };
            let value = label.rich_text.as_deref().map_or_else(
                || html(&label.text),
                |rich| html_rich_label(&label.text, rich, None),
            );
            self.out.push_str(&format!(
                "<mxCell id=\"{}\" value=\"{}\" style=\"{}\" vertex=\"1\" connectable=\"0\" parent=\"{}\"><mxGeometry{position} relative=\"1\" as=\"geometry\">{offset}</mxGeometry></mxCell>\n",
                esc(&id),
                esc(&value),
                esc(&style),
                esc(edge_id)
            ));
        }
    }

    /// Escaped ` name="value"` text of the pending preserved attributes.
    fn take_attributes(&mut self) -> String {
        std::mem::take(&mut self.attributes)
            .into_iter()
            .map(|(name, value)| format!(" {name}=\"{}\"", esc(&value)))
            .collect()
    }

    fn terminal(&self, end: Endpoint) -> Terminal {
        match end {
            Endpoint::Point(p) => Terminal::Point(p),
            Endpoint::Object(id) => match self.sheet.canvas.object(id) {
                Some(object) if matches!(object.kind, CanvasObjectKind::Shape { .. }) => {
                    Terminal::Cell(self.cell_id(id))
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
                Terminal::Cell(self.topic_cells[&id].clone())
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
            let id = self.zone_cells[&zone.id].clone();
            self.vertex(&id, &zone.name, &style, "1", zone.frame, None);
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
            let origin = self
                .sheet
                .zones
                .iter()
                .find(|zone| self.zone_cells[&zone.id] == parent)
                .map_or(Vec2::ZERO, |zone| zone.frame.min);
            let locked = self.sheet.canvas.group(group).is_some_and(|g| g.locked);
            let style = if locked { "group;locked=1;" } else { "group;" };
            let id = self.group_cells[&group].clone();
            self.vertex(
                &id,
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

    /// Writes the rows of a table, then each row's cells in column order with
    /// hidden placeholders for slots covered by a merged cell, as draw.io does.
    fn table_grid(&mut self, id: CanvasObjectId, frame: Rect) {
        const ROW_STYLE: &str = "shape=tableRow;horizontal=0;startSize=0;swimlaneHead=0;swimlaneBody=0;strokeColor=inherit;top=0;left=0;bottom=0;right=0;collapsible=0;dropTarget=0;fillColor=none;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;";
        const COVERED_STYLE: &str = "shape=partialRectangle;html=1;whiteSpace=wrap;connectable=0;strokeColor=inherit;overflow=hidden;fillColor=none;top=0;left=0;bottom=0;right=0;pointerEvents=1;";
        let Some(table) = self.sheet.canvas.scene.tables.get(&id) else {
            return;
        };
        let header = thinoda_core::scene_table::table_header(&self.sheet.canvas, id);
        let Ok(body) = thinoda_core::scene_table::table_body(frame, header) else {
            return;
        };
        let table_id = self.cell_id(id);
        let mut rows = Vec::with_capacity(table.rows.len());
        let mut y = 0.0;
        for (index, height) in table.rows.iter().enumerate() {
            let row_id = self.generated_id(format!("{table_id}-row{index}"));
            let offset = body.min - frame.min;
            self.vertex(
                &row_id,
                "",
                ROW_STYLE,
                &table_id,
                Rect::from_min_size(
                    Vec2::new(offset.x, offset.y + y),
                    Vec2::new(body.size.x, *height),
                ),
                None,
            );
            rows.push((row_id, Vec2::new(body.min.x, body.min.y + y)));
            y += height;
        }
        self.table_rows.insert(id, rows.clone());
        for (row, (row_id, _)) in rows.iter().enumerate() {
            let mut x = 0.0;
            for (column, width) in table.columns.iter().enumerate() {
                match table.cell_at(row, column) {
                    Some(cell)
                        if usize::from(cell.row) == row && usize::from(cell.column) == column =>
                    {
                        self.write_member(BoardMember::Object(cell.id));
                    }
                    _ => {
                        let covered = self.generated_id(format!("{row_id}-{column}"));
                        let slot = Rect::from_min_size(
                            Vec2::new(x, 0.0),
                            Vec2::new(*width, table.rows[row]),
                        );
                        let mut out = String::new();
                        std::mem::swap(&mut out, &mut self.out);
                        self.vertex_with_alternate_bounds(
                            &covered,
                            "",
                            COVERED_STYLE,
                            row_id,
                            slot,
                            Some(Rect::from_min_size(Vec2::ZERO, slot.size)),
                            false,
                            None,
                            None,
                        );
                        std::mem::swap(&mut out, &mut self.out);
                        self.out.push_str(&out.replacen(
                            " vertex=\"1\"",
                            " vertex=\"1\" visible=\"0\"",
                            1,
                        ));
                    }
                }
                x += width;
            }
        }
    }

    fn generated_id(&mut self, mut id: String) -> String {
        while self.used_ids.contains(&id) {
            id.push('x');
        }
        self.used_ids.insert(id.clone());
        id
    }

    fn write_member(&mut self, member: BoardMember) {
        // Table cells are written by their table, under the table's row cells.
        if let BoardMember::Object(id) = member
            && let Some((table, _)) =
                thinoda_core::scene_table::table_of_cell(&self.sheet.canvas, id)
            && !self.table_rows.contains_key(&table)
        {
            self.member(BoardMember::Object(table));
            return;
        }
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
        let table = self.sheet.canvas.scene.tables.get(&id);
        let table_cell = thinoda_core::scene_table::table_of_cell(&self.sheet.canvas, id)
            .and_then(|(table, grid)| Some((grid, *grid.cell(id)?, self.table_rows.get(&table)?)));
        let (parent, origin) = match table_cell {
            Some((_, cell, rows)) => rows[usize::from(cell.row)].clone(),
            None => self.parent(BoardMember::Object(id)),
        };
        let cell_id = self.cell_id(id);
        let drawio = object.drawio.as_deref();
        self.attributes = object
            .data
            .as_deref()
            .map(data_attributes)
            .unwrap_or_default();
        self.attributes
            .extend(drawio.map(|d| d.attributes.clone()).unwrap_or_default());
        match &object.kind {
            CanvasObjectKind::Shape { shape, label } => {
                let mut style = String::new();
                let style_ref = &object.style;
                let scene_container = self.sheet.canvas.scene.containers.contains(&id);
                let collapsed_bounds = self.sheet.canvas.scene.collapsed.get(&id).copied();
                let scene_header = object
                    .stencil
                    .as_ref()
                    .filter(|stencil| scene_container && stencil.definition == "header-frame-v2");
                let stencil_native = object
                    .stencil
                    .as_ref()
                    .and_then(crate::drawio_stencil::native_style);
                if let Some(table) = table {
                    style
                        .push_str("shape=table;childLayout=tableLayout;container=1;collapsible=0;");
                    let header = thinoda_core::scene_table::table_header(&self.sheet.canvas, id);
                    style.push_str(&format!(
                        "startSize={};",
                        num(header.map_or(0.0, |header| header.extent))
                    ));
                    if header.is_some_and(|header| {
                        header.side == thinoda_core::scene_layout::HeaderSide::Left
                    }) {
                        style.push_str("horizontal=0;");
                    }
                    let stripes = &table.stripes;
                    for (key, color) in [
                        ("evenRowColor", stripes.even_rows),
                        ("oddRowColor", stripes.odd_rows),
                        ("evenColumnColor", stripes.even_columns),
                        ("oddColumnColor", stripes.odd_columns),
                    ] {
                        color_keys(
                            &mut style,
                            key,
                            "",
                            color.map(|color| Color { a: 255, ..color }),
                        );
                    }
                } else if let Some(mut native) = stencil_native {
                    if scene_header.and_then(|stencil| stencil.parameters.get("headerSide"))
                        == Some(&3.0)
                    {
                        native = native.replace("horizontal=1;", "horizontal=0;");
                    }
                    style.push_str(&native);
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
                    && table_cell.is_none()
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
                if let Some(stencil) = object.stencil.as_ref().filter(|_| table.is_none()) {
                    if let Some(scene_header) = scene_header {
                        if let Some(extent) = scene_header.parameters.get("headerHeight") {
                            style.push_str(&format!("startSize={};", num(*extent as f32)));
                        }
                    } else {
                        style.push_str(&crate::drawio_stencil::reference_style(stencil));
                    }
                }
                if let Some((_, cell, _)) = table_cell {
                    if cell.row_span > 1 {
                        style.push_str(&format!("rowspan={};", cell.row_span));
                    }
                    if cell.column_span > 1 {
                        style.push_str(&format!("colspan={};", cell.column_span));
                    }
                }
                if scene_container && table.is_none() {
                    style.push_str("container=1;");
                    if is_isolated_scene_container(self.sheet, id) {
                        style.push_str("thinodaSceneContainer=1;");
                    }
                    if collapsed_bounds.is_some() {
                        style.push_str("collapsible=1;");
                    }
                    if let Some(stack_style) = self
                        .sheet
                        .canvas
                        .scene
                        .layout_policies
                        .get(&id)
                        .and_then(scene_stack_style)
                    {
                        style.push_str(&stack_style);
                    }
                }
                match object.label_style.as_deref() {
                    Some(label) => {
                        style.push_str(if label.wrap {
                            "whiteSpace=wrap;html=1;"
                        } else {
                            "whiteSpace=nowrap;html=1;"
                        });
                        label_text_style_keys(&mut style, label);
                    }
                    None => style.push_str("whiteSpace=wrap;html=1;"),
                }
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
                shape_effect_style(&mut style, style_ref);
                passthrough_style(&mut style, drawio);
                let user = (!object.alt_text.is_empty()).then_some((object.alt_text.as_str(), ""));
                // draw.io keeps a table cell's own slot size as its alternate bounds.
                let slot = table_cell.map(|(grid, cell, _)| {
                    Rect::from_min_size(
                        Vec2::ZERO,
                        Vec2::new(
                            grid.columns[usize::from(cell.column)],
                            grid.rows[usize::from(cell.row)],
                        ),
                    )
                });
                self.vertex_with_alternate_bounds(
                    &cell_id,
                    label,
                    &style,
                    &parent,
                    object.rect.translate(Vec2::ZERO - origin),
                    slot.or_else(|| {
                        collapsed_bounds.map(|bounds| bounds.translate(Vec2::ZERO - origin))
                    }),
                    collapsed_bounds.is_some(),
                    user,
                    object
                        .label_rich_text
                        .as_deref()
                        .map(|rich| (rich, object.label_style.as_deref())),
                );
                if table.is_some() {
                    self.table_grid(id, object.rect);
                }
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
                    ConnectorRoute::ElbowHorizontal => style.push_str("edgeStyle=elbowEdgeStyle;"),
                    ConnectorRoute::ElbowVertical => {
                        style.push_str("edgeStyle=elbowEdgeStyle;elbow=vertical;");
                    }
                    ConnectorRoute::SideToSide => style.push_str("edgeStyle=sideToSideEdgeStyle;"),
                    ConnectorRoute::TopToBottom => {
                        style.push_str("edgeStyle=topToBottomEdgeStyle;")
                    }
                    ConnectorRoute::EntityRelation => {
                        style.push_str("edgeStyle=entityRelationEdgeStyle;");
                    }
                    ConnectorRoute::Segment => style.push_str("edgeStyle=segmentEdgeStyle;"),
                    ConnectorRoute::IsometricHorizontal => {
                        style.push_str("edgeStyle=isometricEdgeStyle;");
                    }
                    ConnectorRoute::IsometricVertical => {
                        style.push_str("edgeStyle=isometricEdgeStyle;elbow=vertical;");
                    }
                }
                let marker_name = |marker: Option<ConnectorMarker>| {
                    marker.map_or("classic", |m| drawio_marker_name(m.kind))
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
                if let Some(shape) = connector.edge_shape {
                    style.push_str("shape=");
                    style.push_str(shape.kind.drawio_name());
                    style.push(';');
                    for (key, value) in [
                        ("width", shape.width),
                        ("startWidth", shape.start_width),
                        ("endWidth", shape.end_width),
                    ] {
                        if let Some(value) = value {
                            style.push_str(&format!("{key}={};", num(value)));
                        }
                    }
                    if shape.fill.is_some() {
                        color_keys(&mut style, "fillColor", "fillOpacity", shape.fill);
                    }
                }
                if let Some(jumps) = connector.line_jumps {
                    style.push_str("jumpStyle=");
                    style.push_str(match jumps.style {
                        LineJumpStyle::Arc => "arc",
                        LineJumpStyle::Gap => "gap",
                        LineJumpStyle::Sharp => "sharp",
                        LineJumpStyle::Line => "line",
                    });
                    style.push(';');
                    if jumps.size != DEFAULT_LINE_JUMP_SIZE {
                        style.push_str(&format!("jumpSize={};", num(jumps.size)));
                    }
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
                passthrough_style(&mut style, drawio);
                self.label_placement = connector.label_placement.filter(|_| {
                    connector.sankey.is_none()
                        && (!connector.label.is_empty() || object.has_label_list())
                });
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
                    object.label_rich_text.as_deref(),
                );
                self.edge_labels(&cell_id, &connector.extra_labels, object.style.text_color);
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
                passthrough_style(&mut style, drawio);
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
                let cell = format!(
                    "style=\"{}\" edge=\"1\" parent=\"{parent}\"><mxGeometry relative=\"1\" as=\"geometry\">{geometry}</mxGeometry></mxCell>",
                    esc(&style)
                );
                let extra = self.take_attributes();
                self.out.push_str(&if extra.is_empty() {
                    format!("<mxCell id=\"{cell_id}\" value=\"\" {cell}\n")
                } else {
                    format!("<UserObject label=\"\" id=\"{cell_id}\"{extra}><mxCell {cell}</UserObject>\n")
                });
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
        let cell = self.topic_cells[&id].clone();
        self.vertex_with_alternate_bounds(
            &cell,
            &topic.title,
            &style,
            &parent,
            rect.translate(Vec2::ZERO - origin),
            None,
            false,
            (!link.is_empty()).then_some(("", link)),
            (!topic.inline_text.is_empty()).then_some((&topic.inline_text, None)),
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
            let id = self.map_cells[&format!("b{}", boundary.id.0)].clone();
            self.vertex(&id, title, &style, "1", boundary.rect, None);
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
            let id = self.map_cells[&format!("e{n}")].clone();
            self.edge(&id, "", &style, "1", Vec2::ZERO, ends, &[], "", None);
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
            let cell = self.topic_cells[&id].clone();
            self.vertex(
                &cell,
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
            let id = self.map_cells[&format!("r{}", relationship.id.0)].clone();
            self.edge(
                &id,
                &model.title,
                &style,
                "1",
                Vec2::ZERO,
                ends,
                &[],
                "",
                None,
            );
        }
        for summary in self.layout.summaries.clone() {
            let id = self.map_cells[&format!("s{}", summary.id.0)].clone();
            self.edge(
                &id,
                "",
                "endArrow=none;html=1;",
                "1",
                Vec2::ZERO,
                [Terminal::Point(summary.start), Terminal::Point(summary.end)],
                &[summary.tip],
                "",
                None,
            );
        }
    }

    fn topic_terminal(&self, id: TopicId, fallback: Vec2) -> Terminal {
        if self.topics.contains(&id) {
            Terminal::Cell(self.topic_cells[&id].clone())
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
    Ok(export_sheets(&book.sheets, book.drawio.as_deref())?.into_bytes())
}

fn export_sheets<'a>(
    sheets: impl IntoIterator<Item = &'a Sheet>,
    document: Option<&DrawioDocumentPassthrough>,
) -> Result<String, IoError> {
    if let Some(document) = document {
        document.validate().map_err(|error| invalid(&error))?;
    }
    let sheets: Vec<_> = sheets.into_iter().collect();
    for sheet in &sheets {
        if let Some(page) = &sheet.drawio {
            page.validate().map_err(|error| invalid(&error))?;
        }
        validate_scene_export(sheet)?;
    }
    if sheets.len() > 1
        && sheets
            .iter()
            .any(|sheet| has_isolated_scene_containers(sheet))
    {
        return Err(IoError::Invalid(
            "draw.io export cannot preserve scene containers isolated across multiple pages yet"
                .into(),
        ));
    }
    let document_attributes: String = document
        .into_iter()
        .flat_map(|document| &document.attributes)
        .map(|(name, value)| format!(" {name}=\"{}\"", esc(value)))
        .collect();
    let mut out = format!("<mxfile host=\"Thinoda\" type=\"device\"{document_attributes}>\n");
    let mut diagram_ids = HashSet::new();
    let kept_ids: Vec<Option<String>> = sheets
        .iter()
        .map(|sheet| {
            sheet
                .drawio
                .as_ref()
                .and_then(|page| page.diagram_id.clone())
                .filter(|id| diagram_ids.insert(id.clone()))
        })
        .collect();
    for (index, sheet) in sheets.into_iter().enumerate() {
        let diagram_id = kept_ids[index].clone().unwrap_or_else(|| {
            let base = format!("thinoda-page-{}", index + 1);
            let mut id = base.clone();
            let mut n = 1;
            while !diagram_ids.insert(id.clone()) {
                id = format!("{base}-{n}");
                n += 1;
            }
            id
        });
        let model_attributes: String = sheet
            .drawio
            .iter()
            .flat_map(|page| &page.model_attributes)
            .map(|(name, value)| format!(" {name}=\"{}\"", esc(value)))
            .collect();
        let (grid, thinoda_grid) = match sheet.board.grid {
            GridKind::None => (0, "none"),
            GridKind::Dots => (1, "dots"),
            GridKind::Lines => (1, "lines"),
        };
        let thinoda_snap = u8::from(sheet.board.snap);
        let writer = PageWriter::new(sheet)?;
        let sankey = if sheet.has_sankey() {
            let extension = SankeyExtension::from_sheet(
                sheet,
                &writer.sankey_topic_ids,
                &writer.sankey_edge_ids,
            )?;
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
            "<diagram id=\"{}\" name=\"{}\">\n<mxGraphModel dx=\"0\" dy=\"0\" grid=\"{grid}\" gridSize=\"{}\" thinodaGrid=\"{thinoda_grid}\" thinodaSnap=\"{thinoda_snap}\"{background}{sankey} guides=\"1\" tooltips=\"1\" connect=\"1\" arrows=\"1\" fold=\"1\" page=\"0\" pageScale=\"1\" pageWidth=\"850\" pageHeight=\"1100\" math=\"0\" shadow=\"0\"{model_attributes}>\n<root>\n<mxCell id=\"0\"/>\n<mxCell id=\"1\" parent=\"0\"/>\n",
            esc(&diagram_id),
            esc(&sheet.title),
            num(sheet.board.grid_size),
        ));
        for layer in &sheet.canvas.layers {
            let id = writer
                .layer_cells
                .get(&layer.id)
                .cloned()
                .unwrap_or_else(|| format!("l{}", layer.id.0));
            out.push_str(&format!(
                "<mxCell id=\"{}\" value=\"{}\"{}{} parent=\"0\"/>\n",
                esc(&id),
                esc(&layer.name),
                if layer.locked {
                    " style=\"locked=1;\""
                } else {
                    ""
                },
                if layer.visible { "" } else { " visible=\"0\"" },
            ));
        }
        out.push_str(&writer.write());
        out.push_str("</root>\n</mxGraphModel>\n</diagram>\n");
    }
    out.push_str("</mxfile>\n");
    Ok(out)
}

fn has_isolated_scene_containers(sheet: &Sheet) -> bool {
    sheet
        .canvas
        .scene
        .containers
        .iter()
        .any(|id| is_isolated_scene_container(sheet, *id))
}

fn is_isolated_scene_container(sheet: &Sheet, id: CanvasObjectId) -> bool {
    let scene = &sheet.canvas.scene;
    !scene.parents.iter().any(|(&child, &parent)| {
        (child == id || parent == id)
            && scene.containers.contains(&child)
            && scene.containers.contains(&parent)
    })
}

fn scene_stack_style(policy: &thinoda_core::scene_layout::ContainerLayoutPolicy) -> Option<String> {
    use thinoda_core::scene_layout::{ContainerLayout, StackCrossAxisPolicy, StackResizePolicy};

    let ContainerLayout::Stack {
        axis,
        spacing,
        padding,
        cross_axis,
        resize,
    } = policy.layout
    else {
        return Some(String::new());
    };
    if resize == StackResizePolicy::Distribute {
        return None;
    }

    let mut style = format!(
        "childLayout=stackLayout;horizontalStack={};stackSpacing={};stackBorder={};fill={};",
        if axis == thinoda_core::scene_layout::StackAxis::Horizontal {
            1
        } else {
            0
        },
        num(spacing),
        num(padding),
        u8::from(cross_axis == StackCrossAxisPolicy::Fill),
    );
    match resize {
        StackResizePolicy::Preserve => style.push_str("resizeParent=0;resizeLast=0;"),
        StackResizePolicy::FillLast => style.push_str("resizeParent=0;resizeLast=1;"),
        StackResizePolicy::GrowParent => style.push_str("resizeParent=1;resizeParentMax=1;"),
        StackResizePolicy::FitParent => style.push_str("resizeParent=1;resizeParentMax=0;"),
        StackResizePolicy::Distribute => return None,
    }
    Some(style)
}

fn validate_scene_export(sheet: &Sheet) -> Result<(), IoError> {
    let scene = &sheet.canvas.scene;
    if scene.is_empty() {
        return Ok(());
    }
    for (&id, policy) in &scene.layout_policies {
        if !scene.containers.contains(&id) {
            return Err(IoError::Invalid(
                "draw.io export cannot preserve layout policies on non-container objects".into(),
            ));
        }
        if scene_stack_style(policy).is_none() {
            return Err(IoError::Invalid(
                "draw.io export cannot preserve distributed stack spacing yet".into(),
            ));
        }
    }
    let has_nested_scene_containers = scene.parents.iter().any(|(&child, &parent)| {
        scene.containers.contains(&child) && scene.containers.contains(&parent)
    });
    if has_nested_scene_containers && has_isolated_scene_containers(sheet) {
        return Err(IoError::Invalid(
            "draw.io export cannot preserve scene containers when an isolated container is combined with other scene containers yet"
                .into(),
        ));
    }
    if scene.containers.iter().any(|&container| {
        is_isolated_scene_container(sheet, container)
            && sheet.canvas.objects.iter().any(|object| {
                scene.parents.get(&object.id) == Some(&container)
                    && matches!(object.kind, CanvasObjectKind::Connector(_))
            })
    }) {
        return Err(IoError::Invalid(
            "draw.io export cannot preserve scene connectors inside isolated containers yet".into(),
        ));
    }

    let scene_objects: HashSet<_> = scene
        .containers
        .iter()
        .copied()
        .chain(scene.parents.keys().copied())
        .chain(scene.parents.values().copied())
        .collect();
    if sheet
        .canvas
        .objects
        .iter()
        .any(|object| scene_objects.contains(&object.id) && object.group.is_some())
    {
        return Err(IoError::Invalid(
            "draw.io export cannot preserve scene objects that also belong to a group".into(),
        ));
    }
    if sheet
        .zones
        .iter()
        .any(|zone| zone.objects.iter().any(|id| scene_objects.contains(id)))
    {
        return Err(IoError::Invalid(
            "draw.io export cannot preserve scene objects that also belong to a zone".into(),
        ));
    }
    for id in &scene.containers {
        let object = sheet
            .canvas
            .object(*id)
            .ok_or_else(|| IoError::Invalid("draw.io scene container was not found".into()))?;
        if !matches!(object.kind, CanvasObjectKind::Shape { .. }) {
            return Err(IoError::Invalid(
                "draw.io export cannot preserve non-shape scene containers".into(),
            ));
        }
        if let Some(stencil) = &object.stencil
            && stencil.definition == "header-frame-v2"
        {
            let side = stencil.parameters.get("headerSide").copied().unwrap_or(0.0);
            if side != 0.0 && side != 3.0 {
                return Err(IoError::Invalid(
                    "draw.io export cannot preserve right or bottom scene headers yet".into(),
                ));
            }
        } else if object.stencil.is_some() {
            return Err(IoError::Invalid(
                "draw.io export cannot preserve custom scene container stencils yet".into(),
            ));
        }
    }
    Ok(())
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

pub(crate) fn embed_svg(
    mut svg: String,
    sheet: &Sheet,
    document: Option<&DrawioDocumentPassthrough>,
) -> Result<Vec<u8>, IoError> {
    if svg.len() > MAX_BYTES {
        return Err(invalid("editable draw.io image exceeds size limit"));
    }
    if !valid_rendered_svg(&svg) {
        return Err(invalid("rendered SVG is not a valid SVG document"));
    }
    let xml = export_sheets(std::iter::once(sheet), document)?;
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

pub(crate) fn embed_png(
    mut png: Vec<u8>,
    sheet: &Sheet,
    document: Option<&DrawioDocumentPassthrough>,
) -> Result<Vec<u8>, IoError> {
    if png.len() > MAX_BYTES {
        return Err(invalid("editable draw.io image exceeds size limit"));
    }
    let insertion = rendered_png_insertion(&png)
        .ok_or_else(|| invalid("rendered PNG has no valid image and end chunk"))?;
    let xml = export_sheets(std::iter::once(sheet), document)?;
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

const DRAWIO_MARKER_NAMES: [(&str, ConnectorMarkerKind); 30] = [
    ("classic", ConnectorMarkerKind::Classic),
    ("classicThin", ConnectorMarkerKind::ClassicThin),
    ("open", ConnectorMarkerKind::Open),
    ("openThin", ConnectorMarkerKind::OpenThin),
    ("block", ConnectorMarkerKind::Block),
    ("blockThin", ConnectorMarkerKind::BlockThin),
    ("diamond", ConnectorMarkerKind::Diamond),
    ("diamondThin", ConnectorMarkerKind::DiamondThin),
    ("oval", ConnectorMarkerKind::Oval),
    ("halfCircle", ConnectorMarkerKind::HalfCircle),
    ("baseDash", ConnectorMarkerKind::BaseDash),
    ("dash", ConnectorMarkerKind::Dash),
    ("ERone", ConnectorMarkerKind::ErOne),
    ("ERmandOne", ConnectorMarkerKind::ErMandOne),
    ("ERmany", ConnectorMarkerKind::ErMany),
    ("ERoneToMany", ConnectorMarkerKind::ErOneToMany),
    ("ERzeroToOne", ConnectorMarkerKind::ErZeroToOne),
    ("ERzeroToMany", ConnectorMarkerKind::ErZeroToMany),
    ("manyOptional", ConnectorMarkerKind::ManyOptional),
    ("box", ConnectorMarkerKind::Box),
    ("cross", ConnectorMarkerKind::Cross),
    ("circle", ConnectorMarkerKind::Circle),
    ("circlePlus", ConnectorMarkerKind::CirclePlus),
    ("async", ConnectorMarkerKind::Async),
    ("openAsync", ConnectorMarkerKind::OpenAsync),
    ("doubleBlock", ConnectorMarkerKind::DoubleBlock),
    ("sysMLx", ConnectorMarkerKind::SysmlX),
    ("sysMLLost", ConnectorMarkerKind::SysmlLost),
    ("sysMLFound", ConnectorMarkerKind::SysmlFound),
    ("sysMLPackCont", ConnectorMarkerKind::SysmlPackCont),
];

fn drawio_marker_kind(name: &str) -> Option<ConnectorMarkerKind> {
    DRAWIO_MARKER_NAMES
        .iter()
        .find_map(|(candidate, kind)| (*candidate == name).then_some(*kind))
}

fn drawio_marker_name(kind: ConnectorMarkerKind) -> &'static str {
    DRAWIO_MARKER_NAMES
        .iter()
        .find_map(|(name, candidate)| (*candidate == kind).then_some(*name))
        .unwrap_or("classic")
}

#[cfg(test)]
mod tests;
