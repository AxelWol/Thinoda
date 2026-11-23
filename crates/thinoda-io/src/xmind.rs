use crate::{ImportWarning, IoError, zip_entry};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use thinoda_core::{
    Boundary, ChildRange, Color, LineShape, Sheet, Structure, Summary, TextAlign, TopicId,
    TopicShape, Vec2, Workbook, images,
    numbering::{NumberFormat, Numbering},
    resources::{
        MAX_RESOURCE_BYTES, MAX_RESOURCES, MAX_SHEET_RESOURCE_BYTES, MAX_WORKBOOK_RESOURCE_BYTES,
        Resource,
    },
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAX_XMIND_ARCHIVE_ENTRIES: usize = 1024;
const WAYPOINT_SNAPSHOT_VERSION: &str = "THINODA-XMIND-WAYPOINTS-1";
const MARKER_SNAPSHOT_VERSION: &str = "THINODA-XMIND-MARKERS-1";
const EXTENDED_MARKER_SNAPSHOT_VERSION: &str = "THINODA-XMIND-MARKERS-2";
const SIZED_MARKER_SNAPSHOT_VERSION: &str = "THINODA-XMIND-MARKERS-3";
const FILLED_MARKER_SNAPSHOT_VERSION: &str = "THINODA-XMIND-MARKERS-4";
const DIAMOND_MARKER_SNAPSHOT_VERSION: &str = "THINODA-XMIND-MARKERS-5";
const OVAL_MARKER_SNAPSHOT_VERSION: &str = "THINODA-XMIND-MARKERS-6";
const HALF_CIRCLE_MARKER_SNAPSHOT_VERSION: &str = "THINODA-XMIND-MARKERS-7";
const BAR_MARKER_SNAPSHOT_VERSION: &str = "THINODA-XMIND-MARKERS-8";
const ROUNDING_SNAPSHOT_VERSION: &str = "THINODA-XMIND-ROUNDING-1";
const RECTANGLE_SIDES_SNAPSHOT_VERSION: &str = "THINODA-XMIND-RECTANGLE-SIDES-1";
const DOUBLE_ELLIPSE_SNAPSHOT_VERSION: &str = "THINODA-XMIND-DOUBLE-ELLIPSE-1";
const FIXED_PORT_SNAPSHOT_VERSION: &str = "THINODA-XMIND-FIXED-PORTS-1";
const DRAWIO_ROUNDING_SNAPSHOT_VERSION: &str = "THINODA-XMIND-DRAWIO-ROUNDING-1";
const FINAL_STATE_SNAPSHOT_VERSION: &str = "THINODA-XMIND-FINAL-STATE-1";
const SANKEY_SNAPSHOT_VERSION: &str = "THINODA-XMIND-SANKEY-1";
const STENCIL_SNAPSHOT_VERSION: &str = "THINODA-XMIND-STENCIL-1";
const SCENE_SNAPSHOT_VERSION: &str = "THINODA-XMIND-SCENE-1";
const PINNED_STENCIL_SNAPSHOT_VERSION: &str = "THINODA-XMIND-STENCIL-2";
const CLOSED_STENCIL_SNAPSHOT_VERSION: &str = "THINODA-XMIND-STENCIL-3";
const SCENE_STENCIL_SNAPSHOT_VERSION: &str = "THINODA-XMIND-SCENE-STENCIL-1";
const COLLAPSE_SNAPSHOT_VERSION: &str = "THINODA-XMIND-COLLAPSE-1";
const SCENE_LAYOUT_SNAPSHOT_VERSION: &str = "THINODA-XMIND-SCENE-LAYOUT-1";
const SCENE_STENCIL_LAYOUT_SNAPSHOT_VERSION: &str = "THINODA-XMIND-SCENE-STENCIL-LAYOUT-1";
const COLLAPSE_LAYOUT_SNAPSHOT_VERSION: &str = "THINODA-XMIND-COLLAPSE-LAYOUT-1";
const SEMANTIC_SNAPSHOT_VERSION: &str = "THINODA-XMIND-SEMANTIC-1";
const MARKER_LIBRARY_SNAPSHOT_VERSION: &str = "THINODA-XMIND-MARKER-LIBRARY-1";
const LINE_JUMPS_SNAPSHOT_VERSION: &str = "THINODA-XMIND-LINE-JUMPS-1";
const EDGE_ROUTES_SNAPSHOT_VERSION: &str = "THINODA-XMIND-EDGE-ROUTES-1";
const SEGMENT_ROUTES_SNAPSHOT_VERSION: &str = "THINODA-XMIND-SEGMENT-ROUTES-1";
const EDGE_SHAPES_SNAPSHOT_VERSION: &str = "THINODA-XMIND-EDGE-SHAPES-1";
const CONNECTOR_LABELS_SNAPSHOT_VERSION: &str = "THINODA-XMIND-CONNECTOR-LABELS-1";
const OBJECT_DATA_SNAPSHOT_VERSION: &str = "THINODA-XMIND-OBJECT-DATA-1";
const LAYERS_SNAPSHOT_VERSION: &str = "THINODA-XMIND-LAYERS-1";
const TABLES_SNAPSHOT_VERSION: &str = "THINODA-XMIND-TABLES-1";
const SHAPE_STYLES_SNAPSHOT_VERSION: &str = "THINODA-XMIND-SHAPE-STYLES-1";
const LABEL_STYLES_SNAPSHOT_VERSION: &str = "THINODA-XMIND-LABEL-STYLES-1";
const RICH_LABELS_SNAPSHOT_VERSION: &str = "THINODA-XMIND-RICH-LABELS-1";
const RICH_CONNECTOR_LABELS_SNAPSHOT_VERSION: &str = "THINODA-XMIND-RICH-CONNECTOR-LABELS-1";
const RICH_EXTRA_CONNECTOR_LABELS_SNAPSHOT_VERSION: &str =
    "THINODA-XMIND-RICH-EXTRA-CONNECTOR-LABELS-1";
const CARD_BASELINE_SNAPSHOT_VERSION: &str = "THINODA-XMIND-CARD-BASELINE-1";
const DRAWIO_DOCUMENT_SNAPSHOT_VERSION: &str = "THINODA-XMIND-DRAWIO-DOCUMENT-1";
const DRAWIO_ZONE_IDS_SNAPSHOT_VERSION: &str = "THINODA-XMIND-DRAWIO-ZONE-IDS-1";
const DRAWIO_TOPIC_IDS_SNAPSHOT_VERSION: &str = "THINODA-XMIND-DRAWIO-TOPIC-IDS-1";
const DRAWIO_GROUP_IDS_SNAPSHOT_VERSION: &str = "THINODA-XMIND-DRAWIO-GROUP-IDS-1";

#[derive(Default)]
struct PackageResources {
    original: HashMap<String, Resource>,
    converted_images: HashMap<String, Resource>,
}

fn image_expansion_fits(resolved_bytes: usize, extra: usize, sheet_count: usize) -> bool {
    let budget = MAX_SHEET_RESOURCE_BYTES.min(MAX_WORKBOOK_RESOURCE_BYTES / sheet_count.max(1));
    resolved_bytes <= budget && extra <= budget - resolved_bytes
}

fn supported_static_svg_css(css: &str) -> bool {
    if css.contains('\\') {
        return false;
    }
    let mut normalized = String::with_capacity(css.len());
    let mut offset = 0;
    let mut quote = None;
    let mut parentheses = 0usize;
    let mut previous = None;
    while offset < css.len() {
        let rest = &css[offset..];
        if quote.is_none() && rest.starts_with("/*") {
            let Some(end) = rest[2..].find("*/") else {
                return false;
            };
            let css_whitespace = |ch| matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{c}');
            let whitespace_delimited = normalized.chars().next_back().is_some_and(css_whitespace)
                && rest[end + 4..].chars().next().is_some_and(css_whitespace);
            let at_boundary = rest[end + 4..]
                .trim_start_matches(css_whitespace)
                .chars()
                .next()
                .is_none_or(|ch| matches!(ch, '{' | '}' | ';' | ':' | ','));
            if parentheses != 0
                || (previous.is_some_and(|ch| !matches!(ch, '{' | '}' | ';' | ':' | ','))
                    && !whitespace_delimited
                    && !at_boundary)
            {
                return false;
            }
            offset += end + 4;
            continue;
        }
        if quote.is_none() && rest.starts_with("*/") {
            return false;
        }
        let Some(ch) = rest.chars().next() else {
            return false;
        };
        normalized.push(ch);
        offset += ch.len_utf8();
        if !ch.is_whitespace() {
            previous = Some(ch);
        }
        if quote == Some(ch) {
            quote = None;
        } else if quote.is_none() && matches!(ch, '\'' | '"') {
            quote = Some(ch);
        } else if quote.is_none() && ch == '(' {
            parentheses += 1;
        } else if quote.is_none() && ch == ')' {
            let Some(depth) = parentheses.checked_sub(1) else {
                return false;
            };
            parentheses = depth;
        }
    }
    if quote.is_some() || parentheses != 0 {
        return false;
    }
    let css = normalized.to_ascii_lowercase();
    if css.contains('@')
        || css.contains("animation")
        || css.contains("transition")
        || css.contains("var(")
    {
        return false;
    }
    let identifier_byte =
        |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') || !byte.is_ascii();
    let mut search_from = 0;
    while let Some(relative) = css[search_from..].find("url") {
        let index = search_from + relative;
        if index
            .checked_sub(1)
            .is_some_and(|previous| identifier_byte(css.as_bytes()[previous]))
            || css
                .as_bytes()
                .get(index + 3)
                .is_some_and(|&next| identifier_byte(next))
        {
            search_from = index + 3;
            continue;
        }
        let Some(reference) = css[index + 3..].strip_prefix('(') else {
            return false;
        };
        let Some(end) = reference.find(')') else {
            return false;
        };
        let value = reference[..end].trim();
        let value = if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        let Some(fragment) = value.strip_prefix('#') else {
            return false;
        };
        if fragment.is_empty()
            || !fragment.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
        {
            return false;
        }
        search_from = index + 4 + end + 1;
    }
    true
}

fn embedded_svg_raster_pixels(href: &str) -> Option<u64> {
    use base64::Engine as _;
    use image::ImageDecoder as _;

    let (media_type, data) = if let Some(data) = href.strip_prefix("data:image/png;base64,") {
        ("image/png", data)
    } else if let Some(data) = href.strip_prefix("data:image/jpeg;base64,") {
        ("image/jpeg", data)
    } else if let Some(data) = href.strip_prefix("data:image/webp;base64,") {
        ("image/webp", data)
    } else {
        ("image/gif", href.strip_prefix("data:image/gif;base64,")?)
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    if media_type == "image/gif" {
        let png = images::transcode_static_gif(&bytes).ok()?;
        let image = images::decode(&png).ok()?;
        return Some(u64::from(image.width) * u64::from(image.height));
    }
    if media_type == "image/webp" {
        let mut decoder =
            image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).ok()?;
        if decoder.orientation().ok()? != image::metadata::Orientation::NoTransforms {
            return None;
        }
        let png = images::transcode_static_webp(&bytes).ok()?;
        let image = images::decode(&png).ok()?;
        return Some(u64::from(image.width) * u64::from(image.height));
    }
    if media_type == "image/png"
        && image::codecs::png::PngDecoder::new(std::io::Cursor::new(&bytes))
            .ok()?
            .is_apng()
            .ok()?
    {
        return None;
    }
    if images::requires_visual_normalization(&bytes).ok()? {
        return None;
    }
    let image = images::decode(&bytes).ok()?;
    (image.media_type == media_type).then_some(u64::from(image.width) * u64::from(image.height))
}

fn supported_static_svg(bytes: &[u8]) -> bool {
    use quick_xml::{NsReader, events::Event, name::ResolveResult};

    if bytes.len() > MAX_RESOURCE_BYTES {
        return false;
    }
    let mut reader = NsReader::from_reader(bytes);
    let mut stylesheet: Option<String> = None;
    let mut image_pixels = 0u64;
    let mut image_count = 0usize;
    let mut has_use = false;
    for _ in 0..100_000 {
        match reader.read_event() {
            Ok(event @ (Event::Start(_) | Event::Empty(_))) => {
                if stylesheet.is_some() {
                    return false;
                }
                let starts_element = matches!(&event, Event::Start(_));
                let (Event::Start(element) | Event::Empty(element)) = event else {
                    unreachable!();
                };
                let is_image = element.local_name().as_ref() == "image";
                has_use |= element.local_name().as_ref() == "use";
                if is_image {
                    if !matches!(
                        reader.resolver().resolve_element(element.name()).0,
                        ResolveResult::Bound(namespace)
                            if namespace.0 == "http://www.w3.org/2000/svg"
                    ) {
                        return false;
                    }
                    image_count += 1;
                    if image_count > 16 {
                        return false;
                    }
                }
                if matches!(
                    element.local_name().as_ref(),
                    "feImage"
                        | "foreignObject"
                        | "script"
                        | "animate"
                        | "animateColor"
                        | "animateMotion"
                        | "animateTransform"
                        | "set"
                        | "discard"
                ) {
                    return false;
                }
                let mut image_href = false;
                for attribute in element.attributes() {
                    let Ok(attribute) = attribute else {
                        return false;
                    };
                    let Ok(value) = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    else {
                        return false;
                    };
                    if is_image && attribute.key.local_name().as_ref() == "href" {
                        if image_href {
                            return false;
                        }
                        match reader.resolver().resolve_attribute(attribute.key).0 {
                            ResolveResult::Unbound => {}
                            ResolveResult::Bound(namespace)
                                if namespace.0 == "http://www.w3.org/1999/xlink" => {}
                            _ => return false,
                        }
                        image_href = true;
                        let Some(pixels) = embedded_svg_raster_pixels(&value) else {
                            return false;
                        };
                        image_pixels += pixels;
                        if image_pixels > images::MAX_PIXELS {
                            return false;
                        }
                        continue;
                    }
                    let value = value.to_ascii_lowercase();
                    let css_attribute = matches!(
                        attribute.key.local_name().as_ref(),
                        "style"
                            | "fill"
                            | "stroke"
                            | "filter"
                            | "clip-path"
                            | "mask"
                            | "marker"
                            | "marker-start"
                            | "marker-mid"
                            | "marker-end"
                            | "cursor"
                            | "color-profile"
                    );
                    if value.contains("animation")
                        || value.contains("transition")
                        || (css_attribute && !supported_static_svg_css(&value))
                        || (attribute.key.local_name().as_ref() == "href"
                            && !value.trim_start().starts_with('#'))
                    {
                        return false;
                    }
                }
                if is_image && !image_href {
                    return false;
                }
                if element.local_name().as_ref() == "style" && starts_element {
                    stylesheet = Some(String::new());
                }
            }
            Ok(Event::Text(text)) if stylesheet.is_some() => {
                if let Some(css) = &mut stylesheet {
                    css.push_str(&text.xml_content(quick_xml::XmlVersion::Implicit1_0));
                }
            }
            Ok(Event::CData(text)) if stylesheet.is_some() => {
                if let Some(css) = &mut stylesheet {
                    css.push_str(text.as_ref());
                }
            }
            Ok(Event::GeneralRef(_)) if stylesheet.is_some() => return false,
            Ok(Event::End(element)) if element.local_name().as_ref() == "style" => {
                let Some(css) = stylesheet.take() else {
                    return false;
                };
                if !supported_static_svg_css(&css) {
                    return false;
                }
            }
            Ok(Event::DocType(_) | Event::PI(_)) | Err(_) => return false,
            Ok(Event::Eof) => return stylesheet.is_none() && !(image_count > 0 && has_use),
            _ => {}
        }
    }
    false
}

fn transcode_package_picture(path: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    match image::guess_format(bytes) {
        Ok(image::ImageFormat::WebP) => images::transcode_static_webp(bytes),
        Ok(image::ImageFormat::Gif) => images::transcode_static_gif(bytes),
        _ if path.rsplit_once('.').is_some_and(|(_, ext)| {
            ext.eq_ignore_ascii_case("svg") || ext.eq_ignore_ascii_case("svgz")
        }) =>
        {
            let compressed = bytes.starts_with(&[0x1f, 0x8b]);
            let expanded;
            let svg = if compressed {
                if bytes.len() > MAX_RESOURCE_BYTES {
                    return Err("compressed SVG exceeds the resource limit".into());
                }
                let mut decoder = flate2::bufread::GzDecoder::new(bytes);
                expanded = {
                    let mut data = Vec::new();
                    (&mut decoder)
                        .take(MAX_RESOURCE_BYTES as u64 + 1)
                        .read_to_end(&mut data)
                        .map_err(|error| format!("invalid compressed SVG: {error}"))?;
                    data
                };
                if expanded.len() > MAX_RESOURCE_BYTES || !decoder.into_inner().is_empty() {
                    return Err("compressed SVG exceeds the limit or has trailing data".into());
                }
                expanded.as_slice()
            } else if path
                .rsplit_once('.')
                .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("svgz"))
            {
                return Err("invalid compressed SVG".into());
            } else {
                bytes
            };
            if !supported_static_svg(svg) {
                return Err("invalid or unsupported SVG image".into());
            }
            crate::render::rasterize_xmind_svg(svg)
                .map(|(png, _)| png)
                .ok_or_else(|| "invalid or unsupported SVG image".into())
        }
        _ => Err("unsupported image format".into()),
    }
}

const ENCRYPTED_PACKAGE_ERROR: &str = "This XMind file is password-protected. Remove the password in XMind, save the map again, and then import it.";

struct TopicImportContext<'a> {
    resources: &'a PackageResources,
    theme: &'a Value,
    warnings: std::cell::RefCell<BTreeSet<ImportWarning>>,
    /// Callouts and nested detached topics: (owner, floating topic, offset from the owner).
    floating_links: std::cell::RefCell<Vec<(TopicId, TopicId, Option<Vec2>)>>,
}

impl TopicImportContext<'_> {
    fn warn(&self, warning: ImportWarning) {
        self.warnings.borrow_mut().insert(warning);
    }
}

/// `Ok(None)` for picture sources outside the package (reported, never fetched).
/// Unsafe package references stay fatal.
fn image_reference(src: &str) -> Result<Option<String>, IoError> {
    if !src.starts_with("xap:resources/") {
        return Ok(None);
    }
    image_component(src).map(|(_, path)| Some(path))
}

fn image_component(src: &str) -> Result<(&str, String), IoError> {
    let component = src
        .strip_prefix("xap:resources/")
        .ok_or_else(|| IoError::Invalid("unsupported XMind image reference".into()))?;
    if component.is_empty()
        || component.len() > 255
        || component == "."
        || component == ".."
        || !component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(IoError::Invalid(
            "unsafe XMind image resource reference".into(),
        ));
    }
    Ok((component, format!("resources/{component}")))
}

struct ImageAttributes {
    src: String,
    width: Option<f32>,
    height: Option<f32>,
    below: bool,
}

/// Reads picture attributes. Invalid presentation values are reported and replaced
/// by defaults; a missing source is reported by the resource inventory.
fn image_attributes(image: &Value, context: &TopicImportContext<'_>) -> Option<ImageAttributes> {
    let src = image.get("src").and_then(Value::as_str)?;
    let dimension = |name| -> Option<f32> {
        let value = image.get(name).filter(|value| !value.is_null())?;
        let number = value
            .as_f64()
            .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
            .map(|number| number as f32)
            .filter(|number| number.is_finite() && *number > 0.0);
        if number.is_none() {
            context.warn(ImportWarning::Formatting);
        }
        number
    };
    let below = match image.get("align").and_then(Value::as_str) {
        None if image.get("align").is_none_or(Value::is_null) => false,
        Some("top") => false,
        Some("bottom") => true,
        _ => {
            context.warn(ImportWarning::Formatting);
            false
        }
    };
    Some(ImageAttributes {
        src: src.to_owned(),
        width: dimension("width"),
        height: dimension("height"),
        below,
    })
}

/// Proportionally fits a requested picture size into Thinoda's display bounds.
fn fit_display_size(size: Vec2) -> Option<Vec2> {
    if !(size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0) {
        return None;
    }
    let scale = (images::MAX_DISPLAY_SIZE / size.x)
        .min(images::MAX_DISPLAY_SIZE / size.y)
        .min(1.0);
    let fitted = Vec2::new(size.x * scale, size.y * scale);
    images::valid_size(fitted).then_some(fitted)
}

fn contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key(key) || object.values().any(|value| contains_key(value, key))
        }
        Value::Array(values) => values.iter().any(|value| contains_key(value, key)),
        _ => false,
    }
}

/// Child collections that Thinoda imports as topics.
const TOPIC_COLLECTIONS: [&str; 4] = ["attached", "summary", "detached", "callout"];

#[derive(Default)]
struct ResourceReferences {
    /// Topic pictures (`image.src`).
    images: BTreeSet<String>,
    /// Package files attached to topics (`href: xap:resources/...`).
    attachments: BTreeSet<String>,
    warnings: BTreeSet<ImportWarning>,
}

struct SheetResourceReservation {
    references: ResourceReferences,
    bytes: usize,
    count: usize,
}

struct PackageRecoveryBudget {
    sheets: Vec<SheetResourceReservation>,
    workbook_bytes: usize,
    workbook_count: usize,
}

impl PackageRecoveryBudget {
    fn new(references: Vec<ResourceReferences>, sizes: &HashMap<String, usize>) -> Self {
        let sheets: Vec<_> = references
            .into_iter()
            .map(|references| {
                let mut bytes = 0usize;
                let mut count = 0usize;
                for path in references.images.union(&references.attachments) {
                    if let Some(size) = sizes.get(path) {
                        bytes = bytes.saturating_add(*size);
                        count += 1;
                    }
                }
                SheetResourceReservation {
                    references,
                    bytes,
                    count,
                }
            })
            .collect();
        let workbook_bytes = sheets
            .iter()
            .fold(0usize, |total, sheet| total.saturating_add(sheet.bytes));
        let workbook_count = sheets
            .iter()
            .fold(0usize, |total, sheet| total.saturating_add(sheet.count));
        Self {
            sheets,
            workbook_bytes,
            workbook_count,
        }
    }

    fn reserve_picture(&mut self, path: &str, original: usize, converted: usize) -> bool {
        let mut extra_bytes = 0usize;
        let mut extra_count = 0usize;
        for sheet in &self.sheets {
            if !sheet.references.images.contains(path) {
                continue;
            }
            let shared = sheet.references.attachments.contains(path);
            let extra = if shared {
                converted
            } else {
                converted.saturating_sub(original)
            };
            let count = usize::from(shared);
            if sheet.bytes.saturating_add(extra) > MAX_SHEET_RESOURCE_BYTES
                || sheet.count.saturating_add(count) > MAX_RESOURCES
            {
                return false;
            }
            extra_bytes = extra_bytes.saturating_add(extra);
            extra_count = extra_count.saturating_add(count);
        }
        if self.workbook_bytes.saturating_add(extra_bytes) > MAX_WORKBOOK_RESOURCE_BYTES
            || self.workbook_count.saturating_add(extra_count) > MAX_RESOURCES
        {
            return false;
        }
        for sheet in &mut self.sheets {
            if sheet.references.images.contains(path) {
                let shared = sheet.references.attachments.contains(path);
                sheet.bytes += if shared {
                    converted
                } else {
                    converted.saturating_sub(original)
                };
                sheet.count += usize::from(shared);
            }
        }
        self.workbook_bytes += extra_bytes;
        self.workbook_count += extra_count;
        true
    }
}

/// One traversal over every imported topic collection. Unsafe package references
/// are fatal; everything else that cannot be imported is reported.
fn collect_topic_resource_paths(
    topic: &Value,
    references: &mut ResourceReferences,
    depth: usize,
) -> Result<(), IoError> {
    if depth > 2000 {
        return Err(IoError::Invalid("XMind topic nesting exceeds 2000".into()));
    }
    if topic.get("attachment").is_some() || topic.get("attachments").is_some() {
        references.warnings.insert(ImportWarning::Resources);
    }
    if topic
        .get("extensions")
        .is_some_and(|extensions| contains_key(extensions, "resourceRefs"))
    {
        references.warnings.insert(ImportWarning::Resources);
    }
    if topic
        .get("notes")
        .is_some_and(|notes| contains_key(notes, "image"))
    {
        references.warnings.insert(ImportWarning::Resources);
    }
    if let Some(image) = topic.get("image") {
        match image.get("src").and_then(Value::as_str) {
            Some(src) => match image_reference(src)? {
                Some(path) => {
                    references.images.insert(path);
                }
                None => {
                    references.warnings.insert(ImportWarning::Resources);
                }
            },
            None => {
                references.warnings.insert(ImportWarning::Resources);
            }
        }
    }
    if let Some(href) = topic.get("href").and_then(Value::as_str)
        && href.starts_with("xap:")
    {
        // XMind stores attached files in the package and links them from the topic.
        match href
            .starts_with("xap:resources/")
            .then(|| image_component(href).ok())
            .flatten()
        {
            Some((_, path)) => {
                references.attachments.insert(path);
            }
            None => {
                references.warnings.insert(ImportWarning::Links);
            }
        }
    }
    if let Some(children) = topic.get("children").and_then(Value::as_object) {
        for kind in TOPIC_COLLECTIONS {
            if let Some(values) = children.get(kind).and_then(Value::as_array) {
                for child in values {
                    collect_topic_resource_paths(child, references, depth + 1)?;
                }
            }
        }
    }
    Ok(())
}

fn safe_manifest_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('/')
        && !path
            .bytes()
            .any(|byte| byte == b'\\' || byte == b':' || byte.is_ascii_control())
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn archive_names<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<BTreeSet<String>, IoError> {
    if archive.len() > MAX_XMIND_ARCHIVE_ENTRIES {
        return Err(IoError::Invalid(
            "XMind archive contains too many entries".into(),
        ));
    }
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let name = file.name()?.into_owned();
        if !safe_manifest_path(name.strip_suffix('/').unwrap_or(&name)) {
            return Err(IoError::Invalid(format!(
                "unsafe XMind archive path {name}"
            )));
        }
        if !names.insert(name.clone()) {
            return Err(IoError::Invalid(format!(
                "duplicate XMind archive entry {name}"
            )));
        }
        if file.size() > crate::MAX_ENTRY_SIZE {
            return Err(IoError::Invalid(format!(
                "XMind archive entry {name} exceeds size limit"
            )));
        }
    }
    Ok(names)
}

/// Reads and validates `manifest.json` when present. The manifest is an inventory:
/// declared entries must exist and be safe, but ancillary payloads are never read.
fn manifest_entries<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    names: &BTreeSet<String>,
) -> Result<Option<serde_json::Map<String, Value>>, IoError> {
    if !names.contains("manifest.json") {
        return Ok(None);
    }
    let manifest: Value = serde_json::from_slice(&crate::zip_entry_with_limit(
        archive,
        "manifest.json",
        4 * 1024 * 1024,
    )?)?;
    let entries = manifest
        .get("file-entries")
        .and_then(Value::as_object)
        .ok_or_else(|| IoError::Invalid("XMind manifest is missing file-entries".into()))?;
    // Encrypted packages declare per-entry key derivation data; their content
    // cannot be parsed, so report the actionable cause instead of a JSON error.
    if entries
        .values()
        .any(|metadata| metadata.get("encryption-data").is_some())
    {
        return Err(IoError::Invalid(ENCRYPTED_PACKAGE_ERROR.into()));
    }
    if !entries.contains_key("content.json") {
        return Err(IoError::Invalid(
            "XMind manifest does not declare content.json".into(),
        ));
    }
    let mut declared_resources = 0usize;
    for (path, metadata) in entries {
        if !safe_manifest_path(path) || !metadata.is_object() {
            return Err(IoError::Invalid(format!(
                "unsafe or invalid XMind manifest path {path}"
            )));
        }
        if let Some(component) = path.strip_prefix("resources/") {
            let (_, expected) = image_component(&format!("xap:resources/{component}"))?;
            if expected != *path {
                return Err(IoError::Invalid(format!(
                    "unsafe XMind manifest resource path {path}"
                )));
            }
            declared_resources += 1;
        }
        // Ancillary entries (for example desktop thumbnails) are not map resources.
        // Validate their manifest envelope, but never read or decode their payloads.
        if !names.contains(path) {
            return Err(IoError::Invalid(format!(
                "XMind manifest entry is missing from archive: {path}"
            )));
        }
        if declared_resources > MAX_RESOURCES {
            return Err(IoError::Invalid(
                "XMind archive exceeds the 128 embedded image limit".into(),
            ));
        }
    }
    Ok(Some(entries.clone()))
}

/// Resolves topic pictures against the archive inventory. Budgets count every
/// package resource, including unused ones, which are size-checked but never read.
fn package_resources<R: std::io::Read + std::io::Seek>(
    content: &Value,
    archive: &mut ZipArchive<R>,
    names: &BTreeSet<String>,
) -> Result<(PackageResources, BTreeSet<ImportWarning>), IoError> {
    let mut references = ResourceReferences::default();
    let mut sheet_references = Vec::new();
    for sheet in content
        .as_array()
        .ok_or_else(|| IoError::Invalid("XMind content must be an array".into()))?
    {
        let mut sheet_resources = ResourceReferences::default();
        if let Some(root) = sheet.get("rootTopic") {
            collect_topic_resource_paths(root, &mut sheet_resources, 0)?;
        }
        references
            .images
            .extend(sheet_resources.images.iter().cloned());
        references
            .attachments
            .extend(sheet_resources.attachments.iter().cloned());
        references
            .warnings
            .extend(sheet_resources.warnings.iter().copied());
        sheet_references.push(sheet_resources);
    }
    let ResourceReferences {
        images: referenced,
        attachments,
        mut warnings,
    } = references;

    let mut available = BTreeSet::new();
    let mut sizes = HashMap::new();
    let mut package_bytes = 0u64;
    for path in names {
        let Some(component) = path.strip_prefix("resources/") else {
            continue;
        };
        if component.is_empty() {
            continue;
        }
        // Archive paths are already validated; nested or unusual names can
        // never be referenced by a flat `xap:resources/` reference.
        match image_component(&format!("xap:resources/{component}")) {
            Ok((_, expected)) if expected == *path => {
                available.insert(path.clone());
                sizes.insert(path.clone(), archive.by_name(path)?.size() as usize);
            }
            _ => {
                warnings.insert(ImportWarning::Resources);
            }
        }
        package_bytes = package_bytes.saturating_add(archive.by_name(path)?.size());
    }
    if available.len() > MAX_RESOURCES {
        return Err(IoError::Invalid(
            "XMind archive exceeds the 128 embedded image limit".into(),
        ));
    }
    if package_bytes > MAX_WORKBOOK_RESOURCE_BYTES as u64 {
        return Err(IoError::Invalid(
            "XMind embedded images exceed the 64 MiB workbook limit".into(),
        ));
    }
    if available
        .iter()
        .any(|path| !referenced.contains(path) && !attachments.contains(path))
    {
        warnings.insert(ImportWarning::Resources);
    }

    let mut resources = PackageResources::default();
    let mut budget = PackageRecoveryBudget::new(sheet_references, &sizes);
    for path in referenced {
        if !available.contains(&path) {
            warnings.insert(ImportWarning::Resources);
            continue;
        }
        let mut bytes = crate::zip_entry_with_limit(archive, &path, MAX_RESOURCE_BYTES as u64)?;
        let mut converted = false;
        // The decoder checks headers and dimensions before allocating pixels.
        let media_type = match images::decode(&bytes) {
            Ok(decoded) => decoded.media_type,
            Err(_) => {
                warnings.insert(ImportWarning::Resources);
                let Ok(png) = transcode_package_picture(&path, &bytes) else {
                    continue;
                };
                if !budget.reserve_picture(&path, bytes.len(), png.len()) {
                    continue;
                }
                bytes = png;
                converted = true;
                "image/png"
            }
        };
        let component = path.strip_prefix("resources/").unwrap_or_default();
        let resource = Resource::new(component, media_type, bytes).map_err(IoError::Invalid)?;
        if converted && attachments.contains(&path) {
            resources.converted_images.insert(path, resource);
        } else {
            resources.original.insert(path, resource);
        }
    }
    // Attachments are opaque: never decoded or sniffed. Paths that are also valid
    // pictures reuse the image resource, which has the same bytes.
    for path in attachments {
        if resources.original.contains_key(&path) {
            continue;
        }
        if !available.contains(&path) {
            warnings.insert(ImportWarning::Resources);
            continue;
        }
        let bytes = crate::zip_entry_with_limit(archive, &path, MAX_RESOURCE_BYTES as u64)?;
        let component = path.strip_prefix("resources/").unwrap_or_default();
        match Resource::new(component, "application/octet-stream", bytes) {
            Ok(resource) => {
                resources.original.insert(path, resource);
            }
            Err(_) => {
                warnings.insert(ImportWarning::Resources);
            }
        }
    }
    Ok((resources, warnings))
}

fn topic_image_model(
    sheet: &Sheet,
    id: TopicId,
    component_id: &str,
) -> Result<Option<Value>, IoError> {
    let Some(topic) = sheet.topics.get(&id) else {
        return Ok(None);
    };
    let Some(reference) = topic.image.as_deref() else {
        return Ok(None);
    };
    let resource = sheet.resources.get(reference).map_err(IoError::Invalid)?;
    let decoded = images::decode(resource.bytes()).map_err(IoError::Invalid)?;
    if decoded.media_type != resource.media_type {
        return Err(IoError::Invalid(
            "image media type does not match its content".into(),
        ));
    }
    let size = topic
        .image_size
        .unwrap_or_else(|| images::default_size(decoded.width, decoded.height));
    if !images::valid_size(size) {
        return Err(IoError::Invalid(
            "invalid XMind image display dimensions".into(),
        ));
    }
    Ok(Some(json!({
        "src": format!("xap:resources/{component_id}"),
        "width": size.x,
        "height": size.y,
        "align": if topic.image_below { "bottom" } else { "top" }
    })))
}

fn export_resource_files(book: &Workbook) -> Result<BTreeMap<String, Vec<u8>>, IoError> {
    let mut files = BTreeMap::new();
    let mut workbook_bytes = 0usize;
    for (sheet_index, sheet) in book.sheets.iter().enumerate() {
        // Attachments are opaque package files. Every other resource must be a
        // visible picture, so validate the sheet as if its attachments were removed.
        let attachment_refs: HashSet<&str> = sheet
            .topics
            .values()
            .filter_map(|topic| topic.attachment.as_ref())
            .map(|attachment| attachment.resource.as_str())
            .collect();
        let visual_only;
        let visual = if attachment_refs.is_empty() {
            sheet
        } else {
            let mut stripped = sheet.clone();
            for topic in stripped.topics.values_mut() {
                topic.attachment = None;
            }
            let image_refs: HashSet<String> = stripped
                .topics
                .values()
                .filter_map(|topic| topic.image.clone())
                .collect();
            stripped.resources.entries.retain(|id, _| {
                let reference = format!("{}{id}", thinoda_core::resources::RESOURCE_PREFIX);
                image_refs.contains(&reference) || !attachment_refs.contains(reference.as_str())
            });
            visual_only = stripped;
            &visual_only
        };
        images::validate_visual(visual).map_err(IoError::Invalid)?;
        let mut referenced = HashSet::new();
        let mut sheet_bytes = 0usize;
        let mut add_file = |path: String, bytes: Vec<u8>| -> Result<(), IoError> {
            if bytes.is_empty() || bytes.len() > MAX_RESOURCE_BYTES {
                return Err(IoError::Invalid(
                    "XMind resource must contain 1 byte to 8 MiB".into(),
                ));
            }
            sheet_bytes = sheet_bytes.saturating_add(bytes.len());
            workbook_bytes = workbook_bytes.saturating_add(bytes.len());
            if sheet_bytes > MAX_SHEET_RESOURCE_BYTES {
                return Err(IoError::Invalid(
                    "XMind sheet embedded resources exceed the 32 MiB limit".into(),
                ));
            }
            if workbook_bytes > MAX_WORKBOOK_RESOURCE_BYTES {
                return Err(IoError::Invalid(
                    "XMind embedded resources exceed the 64 MiB workbook limit".into(),
                ));
            }
            files.insert(path, bytes);
            if files.len() > MAX_RESOURCES {
                return Err(IoError::Invalid(
                    "XMind workbook exceeds the 128 embedded resource limit".into(),
                ));
            }
            Ok(())
        };
        for (id, topic) in &sheet.topics {
            if let Some(attachment) = &topic.attachment {
                attachment.validate(sheet).map_err(IoError::Invalid)?;
                referenced.insert(attachment.resource.as_str());
                let resource = sheet
                    .resources
                    .get(&attachment.resource)
                    .map_err(IoError::Invalid)?;
                let path = format!(
                    "resources/{}",
                    attachment_component(
                        &format!("topic-{sheet_index}-{}", id.0),
                        &attachment.name
                    )
                );
                add_file(path, resource.bytes().to_vec())?;
            }
            let Some(reference) = topic.image.as_deref() else {
                if topic.image_size.is_some() || topic.image_below {
                    return Err(IoError::Invalid(
                        "XMind topic has image display metadata without an image".into(),
                    ));
                }
                continue;
            };
            referenced.insert(reference);
            let resource = sheet.resources.get(reference).map_err(IoError::Invalid)?;
            let decoded = images::decode(resource.bytes()).map_err(IoError::Invalid)?;
            if decoded.media_type != resource.media_type {
                return Err(IoError::Invalid(
                    "image media type does not match its content".into(),
                ));
            }
            let size = topic
                .image_size
                .unwrap_or_else(|| images::default_size(decoded.width, decoded.height));
            if !images::valid_size(size) {
                return Err(IoError::Invalid(
                    "invalid XMind image display dimensions".into(),
                ));
            }
            let path = format!("resources/topic-{sheet_index}-{}", id.0);
            add_file(path, resource.bytes().to_vec())?;
        }
        if referenced.len() != sheet.resources.entries.len() {
            return Err(IoError::Invalid(
                "XMind export cannot preserve unreferenced embedded resources".into(),
            ));
        }
    }
    Ok(files)
}

fn export_topic_style(
    sheet: &Sheet,
    id: TopicId,
) -> std::borrow::Cow<'_, thinoda_core::TopicStyle> {
    if sheet.theme.custom_theme.is_some() {
        std::borrow::Cow::Owned(
            thinoda_core::quick_styles::QuickStyle::from_appearance(
                "Export".into(),
                &thinoda_core::theme::resolve_style(sheet, id, false),
            )
            .style,
        )
    } else {
        std::borrow::Cow::Borrowed(&sheet.topics[&id].style)
    }
}

fn shape_class(shape: TopicShape) -> &'static str {
    match shape {
        TopicShape::RoundedRect => "org.xmind.topicShape.roundedRect",
        TopicShape::Rect => "org.xmind.topicShape.rect",
        TopicShape::Ellipse => "org.xmind.topicShape.ellipse",
        TopicShape::Capsule => "org.xmind.topicShape.capsule",
        TopicShape::Diamond => "org.xmind.topicShape.diamond",
        TopicShape::Underline => "org.xmind.topicShape.underline",
        TopicShape::None => "org.xmind.topicShape.none",
    }
}

fn parse_shape_class(class: &str) -> Option<TopicShape> {
    match class_key(class).as_str() {
        "roundedrect" | "roundedrectangle" => Some(TopicShape::RoundedRect),
        "rect" | "rectangle" => Some(TopicShape::Rect),
        "ellipse" | "ellipserect" | "ellipticrectangle" => Some(TopicShape::Ellipse),
        "capsule" => Some(TopicShape::Capsule),
        "diamond" => Some(TopicShape::Diamond),
        "underline" => Some(TopicShape::Underline),
        "none" => Some(TopicShape::None),
        _ => None,
    }
}

/// Nearest Thinoda shape for XMind shapes it cannot draw; callers report the change.
fn approximate_shape_class(class: &str) -> Option<TopicShape> {
    let key = class_key(class);
    match key.as_str() {
        "circle" => Some(TopicShape::Ellipse),
        "doubleunderline" => Some(TopicShape::Underline),
        "parallelogram" | "hexagon" | "roundedhexagon" | "singlebreakangle" | "stack" | "cloud" => {
            Some(TopicShape::RoundedRect)
        }
        _ if key.starts_with("fishhead") => Some(TopicShape::RoundedRect),
        _ => None,
    }
}

fn line_class(shape: LineShape) -> &'static str {
    match shape {
        LineShape::Curve => "org.xmind.branchConnection.curve",
        LineShape::Straight => "org.xmind.branchConnection.straight",
        LineShape::Elbow => "org.xmind.branchConnection.elbow",
        LineShape::RoundedElbow => "org.xmind.branchConnection.roundedElbow",
    }
}

/// Includes the desktop names `bight`, `fold` and `roundedFold` for the same lines.
fn parse_line_class(class: &str) -> Option<LineShape> {
    match class_key(class).as_str() {
        "curve" | "bight" => Some(LineShape::Curve),
        "straight" => Some(LineShape::Straight),
        "elbow" | "fold" => Some(LineShape::Elbow),
        "roundedelbow" | "roundedfold" => Some(LineShape::RoundedElbow),
        // Earlier Thinoda builds wrote the UI label to this private property.
        _ => None,
    }
}

/// XMind has no hidden root: a Hidden-mode anchor is exported as a central topic named
/// after the sheet, and every floating root stays a detached topic.
fn export_title<'a>(sheet: &Sheet, id: TopicId, title: &'a str) -> std::borrow::Cow<'a, str> {
    if id == sheet.root && sheet.root_mode == thinoda_core::board::SheetRootMode::Hidden {
        crate::synthesized_root_title(sheet).into()
    } else {
        title.into()
    }
}

/// Version of the public `content.json` projection written by [`export`]. The private
/// snapshot is trusted only if regenerating the projection it was written with gives
/// the file's content, so earlier projections must keep producing identical output.
///
/// 1. Original projection (no `thinodaPublicProjection` metadata key).
/// 2. Adds topic width, child numbering, attachments, unbalanced split counts,
///    relationship control points and the sheet background/tapered-line theme.
const PUBLIC_PROJECTION: u64 = 2;

/// Package path of an exported attachment. The extension is kept (sanitized) so
/// XMind shows a matching file icon.
fn attachment_component(topic_component: &str, name: &str) -> String {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .filter(|extension| {
            (1..=16).contains(&extension.len())
                && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    format!(
        "{}{extension}",
        topic_component.replacen("topic-", "attachment-", 1)
    )
}

/// XMind's unbalanced-map extension: the first `right` children go to the right.
fn unbalanced_extension(right: usize) -> Value {
    json!([{
        "provider": UNBALANCED_PROVIDER,
        "content": [{"name": "right-number", "content": right.to_string()}]
    }])
}

fn public_content(book: &Workbook, projection: u64) -> Result<Value, IoError> {
    struct Projection<'a> {
        sheet: &'a Sheet,
        ids: &'a HashMap<TopicId, String>,
        version: u64,
    }
    fn topic(p: &Projection<'_>, id: TopicId) -> Result<Value, IoError> {
        let (sheet, ids) = (p.sheet, p.ids);
        let model = &sheet.topics[&id];
        let mut children = serde_json::Map::new();
        let attached: Vec<_> = model
            .children
            .iter()
            .map(|child| topic(p, *child))
            .collect::<Result<_, _>>()?;
        if !attached.is_empty() {
            children.insert("attached".into(), json!(attached));
        }
        if id == sheet.root {
            let detached: Vec<_> = sheet
                .floating
                .iter()
                .map(|floating_id| -> Result<Value, IoError> {
                    let floating = &sheet.topics[floating_id];
                    let position = floating.position.unwrap_or(Vec2::ZERO);
                    let mut value = topic(p, *floating_id)?;
                    value["position"] = json!({"x": position.x, "y": position.y});
                    Ok(value)
                })
                .collect::<Result<_, _>>()?;
            if !detached.is_empty() {
                children.insert("detached".into(), json!(detached));
            }
        }
        let style = export_topic_style(sheet, id);
        let mut properties = serde_json::Map::new();
        if let Some(color) = style.fill {
            properties.insert("svg:fill".into(), json!(color.to_hex()));
        }
        if let Some(color) = style.text_color {
            properties.insert("fo:color".into(), json!(color.to_hex()));
        }
        if let Some(size) = style.font_size {
            properties.insert("fo:font-size".into(), json!(format!("{size}pt")));
        }
        if let Some(bold) = style.bold {
            properties.insert(
                "fo:font-weight".into(),
                json!(if bold { "bold" } else { "normal" }),
            );
        }
        if let Some(italic) = style.italic {
            properties.insert(
                "fo:font-style".into(),
                json!(if italic { "italic" } else { "normal" }),
            );
        }
        if let Some(color) = style.border_color {
            properties.insert("border-line-color".into(), json!(color.to_hex()));
        }
        if let Some(width) = style.border_width {
            properties.insert("border-line-width".into(), json!(width.to_string()));
        }
        if let Some(color) = style.line_color {
            properties.insert("line-color".into(), json!(color.to_hex()));
        }
        if let Some(width) = style.line_width.or(sheet.theme.line_width) {
            properties.insert("line-width".into(), json!(width.to_string()));
        }
        if let Some(shape) = style.shape {
            properties.insert("shape-class".into(), json!(shape_class(shape)));
        }
        if let Some(line_shape) = style.line_shape {
            properties.insert("line-class".into(), json!(line_class(line_shape)));
        }
        if let Some(font_family) = style
            .font_family
            .as_ref()
            .or(sheet.theme.font_family.as_ref())
        {
            properties.insert("fo:font-family".into(), json!(font_family));
        }
        if let Some(align) = style.align {
            properties.insert(
                "fo:text-align".into(),
                json!(match align {
                    TextAlign::Left => "left",
                    TextAlign::Center => "center",
                    TextAlign::Right => "right",
                }),
            );
        }
        if style.underline == Some(true) || style.strikethrough == Some(true) {
            let mut decoration = Vec::new();
            if style.underline == Some(true) {
                decoration.push("underline");
            }
            if style.strikethrough == Some(true) {
                decoration.push("line-through");
            }
            properties.insert("fo:text-decoration".into(), json!(decoration.join(" ")));
        }
        if style.underline == Some(false) && style.strikethrough.is_none() {
            properties.insert("fo:text-decoration".into(), json!("none"));
        }
        let mut value = json!({
            "id": ids[&id], "class": "topic", "title": export_title(sheet, id, &model.title), "children": children,
            "markers": model.markers.iter().map(|marker| json!({"markerId": marker})).collect::<Vec<_>>(),
            "labels": model.labels, "branch": if model.collapsed { "folded" } else { "" },
            "style": {"properties": properties}
        });
        if let Some(image) = topic_image_model(sheet, id, &ids[&id])? {
            value["image"] = image;
        }
        if let Some(notes) = &model.notes {
            value["notes"] = json!({"plain":{"content": &**notes}});
        }
        if let Some(link) = &model.hyperlink {
            value["href"] = json!(link);
        }
        let structure = if id == sheet.root {
            Some(model.structure.unwrap_or(sheet.structure))
        } else {
            model.structure
        };
        if let Some(structure) = structure {
            value["structureClass"] = json!(structure.xmind_class());
        }
        if p.version >= 2 {
            if let Some(width) = model.style.width {
                value["customWidth"] = json!(width);
            }
            if let Some(numbering) = &model.child_numbering {
                value["numbering"] = json!({"numberFormat": xmind_number_format(numbering.format)});
                if !numbering.tiered {
                    value["numbering"]["prependingNumbers"] = json!("none");
                }
            }
            // XMind shows package files on the topic itself; the attachment replaces any
            // web link publicly (the private snapshot keeps both).
            if let Some(attachment) = &model.attachment {
                value["href"] = json!(format!(
                    "xap:resources/{}",
                    attachment_component(&ids[&id], &attachment.name)
                ));
            }
            if structure == Some(Structure::MapUnbalanced)
                && !model.children.is_empty()
                && model
                    .children
                    .iter()
                    .all(|child| sheet.topics[child].branch_side.is_some())
            {
                let right = model
                    .children
                    .iter()
                    .filter(|child| {
                        sheet.topics[*child].branch_side == Some(thinoda_core::BranchSide::Right)
                    })
                    .count();
                value["extensions"] = unbalanced_extension(right);
            }
        }
        let boundaries: Vec<_> = sheet.boundaries.iter().filter(|boundary| boundary.parent == id).map(|boundary| {
            json!({
                "id": format!("{}-boundary-{}", ids[&id], boundary.id.0),
                "class": "boundary",
                "range": boundary.range.map(|range| format!("({}, {})", range.start, range.end)).unwrap_or_else(|| "master".into()),
                "title": boundary.title,
                "style": {"properties": boundary.color.map(|color| json!({"svg:fill": color.to_hex()})).unwrap_or_else(|| json!({}))}
            })
        }).collect();
        if !boundaries.is_empty() {
            value["boundaries"] = json!(boundaries);
        }
        let sheet_summaries: Vec<_> = sheet
            .summaries
            .iter()
            .filter(|summary| summary.parent == id)
            .collect();
        if !sheet_summaries.is_empty() {
            value["summaries"] = json!(
                sheet_summaries
                    .iter()
                    .map(|summary| json!({
                        "id": format!("{}-summary-{}", ids[&id], summary.id.0),
                        "class": "summary",
                        "range": format!("({}, {})", summary.range.start, summary.range.end),
                        "topicId": ids.get(&summary.topic)
                    }))
                    .collect::<Vec<_>>()
            );
            value["children"]["summary"] = json!(
                sheet_summaries
                    .iter()
                    .map(|summary| topic(p, summary.topic))
                    .collect::<Result<Vec<_>, _>>()?
            );
        }
        Ok(value)
    }
    let sheets: Vec<Value> = book
        .sheets
        .iter()
        .enumerate()
        .map(|(sheet_index, sheet)| -> Result<Value, IoError> {
            let ids: HashMap<TopicId, String> = sheet
                .topics
                .keys()
                .map(|id| (*id, format!("topic-{sheet_index}-{}", id.0)))
                .collect();
            let p = Projection {
                sheet,
                ids: &ids,
                version: projection,
            };
            let mut value = json!({
                "id": format!("sheet-{sheet_index}"), "class": "sheet", "title": sheet.title,
                "rootTopic": topic(&p, sheet.root)?,
                "relationships": sheet.relationships.iter().map(|relationship| {
                    let mut value = json!({
                        "id": format!("rel-{sheet_index}-{}", relationship.id.0),
                        "class": "relationship",
                        "end1Id": ids.get(&relationship.from), "end2Id": ids.get(&relationship.to),
                        "title": relationship.title,
                        "style": {"properties": relationship.color.map(|color| json!({"line-color": color.to_hex()})).unwrap_or_else(|| json!({}))}
                    });
                    if projection >= 2
                        && let Some([a, b]) = relationship.control_points
                    {
                        value["controlPoints"] =
                            json!({"0": {"x": a.x, "y": a.y}, "1": {"x": b.x, "y": b.y}});
                    }
                    value
                }).collect::<Vec<_>>()
            });
            if projection >= 2 {
                let mut properties = serde_json::Map::new();
                if let Some(background) = sheet.theme.background {
                    properties.insert("svg:fill".into(), json!(background.to_hex()));
                }
                properties.insert(
                    "line-tapered".into(),
                    json!(if sheet.theme.tapered_lines { "tapered" } else { "none" }),
                );
                value["theme"] = json!({"map": {"properties": properties}});
            }
            Ok(value)
        })
        .collect::<Result<_, _>>()?;
    Ok(json!(sheets))
}

fn content_digest(content: &Value) -> Result<String, IoError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(content)?)
    ))
}

fn private_snapshot(book: &Workbook) -> Result<String, IoError> {
    if book.has_drawio_group_source_ids() {
        Ok(serde_json::to_string(&(
            DRAWIO_GROUP_IDS_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_drawio_topic_source_ids() {
        Ok(serde_json::to_string(&(
            DRAWIO_TOPIC_IDS_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_drawio_zone_source_ids() {
        Ok(serde_json::to_string(&(
            DRAWIO_ZONE_IDS_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.drawio.is_some() {
        Ok(serde_json::to_string(&(
            DRAWIO_DOCUMENT_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_card_baseline_overrides() {
        Ok(serde_json::to_string(&(
            CARD_BASELINE_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_rich_extra_connector_labels() {
        Ok(serde_json::to_string(&(
            RICH_EXTRA_CONNECTOR_LABELS_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_rich_connector_labels() {
        Ok(serde_json::to_string(&(
            RICH_CONNECTOR_LABELS_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_rich_labels() {
        Ok(serde_json::to_string(&(
            RICH_LABELS_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_label_styles() {
        Ok(serde_json::to_string(&(
            LABEL_STYLES_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_shape_styles() {
        Ok(serde_json::to_string(&(
            SHAPE_STYLES_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_tables() {
        Ok(serde_json::to_string(&(TABLES_SNAPSHOT_VERSION, book))?)
    } else if book.has_layers() {
        Ok(serde_json::to_string(&(LAYERS_SNAPSHOT_VERSION, book))?)
    } else if book.has_object_data() {
        Ok(serde_json::to_string(&(
            OBJECT_DATA_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_connector_labels() {
        Ok(serde_json::to_string(&(
            CONNECTOR_LABELS_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_connector_edge_shapes() {
        Ok(serde_json::to_string(&(
            EDGE_SHAPES_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_drawio_segment_routes() {
        Ok(serde_json::to_string(&(
            SEGMENT_ROUTES_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_drawio_edge_routes() {
        Ok(serde_json::to_string(&(
            EDGE_ROUTES_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_connector_line_jumps() {
        Ok(serde_json::to_string(&(LINE_JUMPS_SNAPSHOT_VERSION, book))?)
    } else if book.has_library_connector_markers() {
        Ok(serde_json::to_string(&(
            MARKER_LIBRARY_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_semantic_model() {
        Ok(serde_json::to_string(&(SEMANTIC_SNAPSHOT_VERSION, book))?)
    } else if book.has_scene_collapse() && book.has_scene_layout_policies() {
        Ok(serde_json::to_string(&(
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_scene_collapse() {
        Ok(serde_json::to_string(&(COLLAPSE_SNAPSHOT_VERSION, book))?)
    } else if book.has_scene_hierarchy()
        && book.has_scene_layout_policies()
        && (book.has_stencil_definition_hashes() || book.has_stencil_definition_closures())
    {
        Ok(serde_json::to_string(&(
            SCENE_STENCIL_LAYOUT_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_scene_hierarchy() && book.has_scene_layout_policies() {
        Ok(serde_json::to_string(&(
            SCENE_LAYOUT_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_scene_hierarchy()
        && (book.has_stencil_definition_hashes() || book.has_stencil_definition_closures())
    {
        Ok(serde_json::to_string(&(
            SCENE_STENCIL_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_scene_hierarchy() {
        Ok(serde_json::to_string(&(SCENE_SNAPSHOT_VERSION, book))?)
    } else if book.has_stencil_definition_closures() {
        Ok(serde_json::to_string(&(
            CLOSED_STENCIL_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_stencil_definition_hashes() {
        Ok(serde_json::to_string(&(
            PINNED_STENCIL_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_stencils() {
        Ok(serde_json::to_string(&(STENCIL_SNAPSHOT_VERSION, book))?)
    } else if book.has_sankey() {
        Ok(serde_json::to_string(&(SANKEY_SNAPSHOT_VERSION, book))?)
    } else if book.has_final_states() {
        Ok(serde_json::to_string(&(
            FINAL_STATE_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_drawio_rectangle_rounding() {
        Ok(serde_json::to_string(&(
            DRAWIO_ROUNDING_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_fixed_connector_ports() {
        Ok(serde_json::to_string(&(FIXED_PORT_SNAPSHOT_VERSION, book))?)
    } else if book.has_double_ellipses() {
        Ok(serde_json::to_string(&(
            DOUBLE_ELLIPSE_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_rectangle_sides() {
        Ok(serde_json::to_string(&(
            RECTANGLE_SIDES_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_shape_rounding() {
        Ok(serde_json::to_string(&(ROUNDING_SNAPSHOT_VERSION, book))?)
    } else if book.has_bar_connector_markers() {
        Ok(serde_json::to_string(&(BAR_MARKER_SNAPSHOT_VERSION, book))?)
    } else if book.has_half_circle_connector_markers() {
        Ok(serde_json::to_string(&(
            HALF_CIRCLE_MARKER_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_oval_connector_markers() {
        Ok(serde_json::to_string(&(
            OVAL_MARKER_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_diamond_connector_markers() {
        Ok(serde_json::to_string(&(
            DIAMOND_MARKER_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_connector_marker_fills() {
        Ok(serde_json::to_string(&(
            FILLED_MARKER_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_connector_marker_sizes() {
        Ok(serde_json::to_string(&(
            SIZED_MARKER_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_extended_connector_markers() {
        Ok(serde_json::to_string(&(
            EXTENDED_MARKER_SNAPSHOT_VERSION,
            book,
        ))?)
    } else if book.has_connector_markers() {
        Ok(serde_json::to_string(&(MARKER_SNAPSHOT_VERSION, book))?)
    } else if book.has_connector_waypoints() {
        Ok(serde_json::to_string(&(WAYPOINT_SNAPSHOT_VERSION, book))?)
    } else {
        Ok(serde_json::to_string(book)?)
    }
}

fn strict_private_snapshot(source: &str) -> bool {
    source
        .trim_start()
        .strip_prefix('[')
        .and_then(|contents| {
            String::deserialize(&mut serde_json::Deserializer::from_str(contents)).ok()
        })
        .is_some_and(|version| {
            version == MARKER_SNAPSHOT_VERSION
                || version == EXTENDED_MARKER_SNAPSHOT_VERSION
                || version == SIZED_MARKER_SNAPSHOT_VERSION
                || version == FILLED_MARKER_SNAPSHOT_VERSION
                || version == DIAMOND_MARKER_SNAPSHOT_VERSION
                || version == OVAL_MARKER_SNAPSHOT_VERSION
                || version == HALF_CIRCLE_MARKER_SNAPSHOT_VERSION
                || version == BAR_MARKER_SNAPSHOT_VERSION
                || version == ROUNDING_SNAPSHOT_VERSION
                || version == RECTANGLE_SIDES_SNAPSHOT_VERSION
                || version == DOUBLE_ELLIPSE_SNAPSHOT_VERSION
                || version == FIXED_PORT_SNAPSHOT_VERSION
                || version == DRAWIO_ROUNDING_SNAPSHOT_VERSION
                || version == FINAL_STATE_SNAPSHOT_VERSION
                || version == SANKEY_SNAPSHOT_VERSION
                || version == STENCIL_SNAPSHOT_VERSION
                || version == SCENE_SNAPSHOT_VERSION
                || version == PINNED_STENCIL_SNAPSHOT_VERSION
                || version == CLOSED_STENCIL_SNAPSHOT_VERSION
                || version == SCENE_STENCIL_SNAPSHOT_VERSION
                || version == COLLAPSE_SNAPSHOT_VERSION
                || version == SCENE_LAYOUT_SNAPSHOT_VERSION
                || version == SCENE_STENCIL_LAYOUT_SNAPSHOT_VERSION
                || version == COLLAPSE_LAYOUT_SNAPSHOT_VERSION
                || version == SEMANTIC_SNAPSHOT_VERSION
                || version == MARKER_LIBRARY_SNAPSHOT_VERSION
                || version == LINE_JUMPS_SNAPSHOT_VERSION
                || version == EDGE_ROUTES_SNAPSHOT_VERSION
                || version == SEGMENT_ROUTES_SNAPSHOT_VERSION
                || version == EDGE_SHAPES_SNAPSHOT_VERSION
                || version == CONNECTOR_LABELS_SNAPSHOT_VERSION
                || version == OBJECT_DATA_SNAPSHOT_VERSION
                || version == LAYERS_SNAPSHOT_VERSION
                || version == TABLES_SNAPSHOT_VERSION
                || version == SHAPE_STYLES_SNAPSHOT_VERSION
                || version == LABEL_STYLES_SNAPSHOT_VERSION
                || version == RICH_LABELS_SNAPSHOT_VERSION
                || version == RICH_CONNECTOR_LABELS_SNAPSHOT_VERSION
                || version == RICH_EXTRA_CONNECTOR_LABELS_SNAPSHOT_VERSION
                || version == CARD_BASELINE_SNAPSHOT_VERSION
                || version == DRAWIO_DOCUMENT_SNAPSHOT_VERSION
                || version == DRAWIO_ZONE_IDS_SNAPSHOT_VERSION
                || version == DRAWIO_TOPIC_IDS_SNAPSHOT_VERSION
                || version == DRAWIO_GROUP_IDS_SNAPSHOT_VERSION
        })
}

fn snapshot_contains_versioned_fields(source: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(source) else {
        return false;
    };
    let workbook = value
        .as_array()
        .and_then(|envelope| envelope.get(1))
        .unwrap_or(&value);
    if workbook.get("drawio").is_some_and(|value| !value.is_null()) {
        return true;
    }
    if workbook
        .get("sheets")
        .and_then(Value::as_array)
        .is_some_and(|sheets| {
            sheets.iter().any(|sheet| {
                sheet.get("sankey").is_some_and(|v| !v.is_null())
                    || sheet
                        .get("zones")
                        .and_then(Value::as_array)
                        .is_some_and(|zones| {
                            zones.iter().any(|zone| {
                                zone.get("drawio_source_id").is_some_and(|v| !v.is_null())
                            })
                        })
                    || sheet
                        .get("topics")
                        .and_then(Value::as_object)
                        .is_some_and(|topics| {
                            topics.values().any(|topic| {
                                topic.get("drawio_source_id").is_some_and(|v| !v.is_null())
                                    || topic
                                        .get("inline_text")
                                        .and_then(|text| text.get("runs"))
                                        .and_then(Value::as_array)
                                        .is_some_and(|runs| {
                                            runs.iter().any(|run| {
                                                run.get("style")
                                                    .and_then(|style| style.get("baseline"))
                                                    .is_some_and(|value| !value.is_null())
                                            })
                                        })
                            })
                        })
                    || sheet
                        .get("canvas")
                        .and_then(|canvas| canvas.get("scene"))
                        .is_some()
                    || sheet
                        .get("canvas")
                        .and_then(|canvas| canvas.get("layers"))
                        .is_some()
                    || sheet
                        .get("canvas")
                        .and_then(|canvas| canvas.get("groups"))
                        .and_then(Value::as_array)
                        .is_some_and(|groups| {
                            groups.iter().any(|group| {
                                group.get("drawio_source_id").is_some_and(|v| !v.is_null())
                            })
                        })
            })
        })
    {
        return true;
    }
    workbook
        .get("sheets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|sheet| sheet.get("canvas")?.get("objects")?.as_array())
        .flatten()
        .any(|object| {
            object.get("rounding").is_some()
                || object.get("data").is_some()
                || object.get("layer").is_some()
                || object.get("label_rich_text").is_some()
                || object.get("rectangle_sides").is_some()
                || object.get("double_ellipse_margin").is_some()
                || object.get("stencil").is_some()
                || matches!(
                    object
                        .get("kind")
                        .and_then(|kind| kind.get("shape"))
                        .and_then(|shape| shape.get("shape"))
                        .and_then(Value::as_str),
                    Some("double-ellipse" | "final-state")
                )
                || object
                    .get("kind")
                    .and_then(|kind| kind.get("connector"))
                    .and_then(Value::as_object)
                    .is_some_and(|connector| {
                        connector.contains_key("start_marker")
                            || connector.contains_key("sankey")
                            || connector.contains_key("end_marker")
                            || connector.contains_key("start_marker_size")
                            || connector.contains_key("end_marker_size")
                            || connector.contains_key("start_marker_unfilled")
                            || connector.contains_key("end_marker_unfilled")
                            || connector.contains_key("from_port")
                            || connector.contains_key("to_port")
                            || connector.contains_key("line_jumps")
                            || connector.contains_key("edge_shape")
                            || connector.contains_key("label_placement")
                            || connector.contains_key("extra_labels")
                            || matches!(
                                connector.get("route").and_then(Value::as_str),
                                Some(
                                    "elbow-horizontal"
                                        | "elbow-vertical"
                                        | "side-to-side"
                                        | "top-to-bottom"
                                        | "entity-relation"
                                        | "segment"
                                        | "isometric-horizontal"
                                        | "isometric-vertical"
                                )
                            )
                    })
        })
}

fn read_private_snapshot(source: &str) -> Result<Workbook, IoError> {
    if let Some(contents) = source.trim_start().strip_prefix('[') {
        let version = String::deserialize(&mut serde_json::Deserializer::from_str(contents))?;
        if version != WAYPOINT_SNAPSHOT_VERSION
            && version != MARKER_SNAPSHOT_VERSION
            && version != EXTENDED_MARKER_SNAPSHOT_VERSION
            && version != SIZED_MARKER_SNAPSHOT_VERSION
            && version != FILLED_MARKER_SNAPSHOT_VERSION
            && version != DIAMOND_MARKER_SNAPSHOT_VERSION
            && version != OVAL_MARKER_SNAPSHOT_VERSION
            && version != HALF_CIRCLE_MARKER_SNAPSHOT_VERSION
            && version != BAR_MARKER_SNAPSHOT_VERSION
            && version != ROUNDING_SNAPSHOT_VERSION
            && version != RECTANGLE_SIDES_SNAPSHOT_VERSION
            && version != DOUBLE_ELLIPSE_SNAPSHOT_VERSION
            && version != FIXED_PORT_SNAPSHOT_VERSION
            && version != DRAWIO_ROUNDING_SNAPSHOT_VERSION
            && version != FINAL_STATE_SNAPSHOT_VERSION
            && version != SANKEY_SNAPSHOT_VERSION
            && version != STENCIL_SNAPSHOT_VERSION
            && version != SCENE_SNAPSHOT_VERSION
            && version != PINNED_STENCIL_SNAPSHOT_VERSION
            && version != CLOSED_STENCIL_SNAPSHOT_VERSION
            && version != SCENE_STENCIL_SNAPSHOT_VERSION
            && version != COLLAPSE_SNAPSHOT_VERSION
            && version != SCENE_LAYOUT_SNAPSHOT_VERSION
            && version != SCENE_STENCIL_LAYOUT_SNAPSHOT_VERSION
            && version != COLLAPSE_LAYOUT_SNAPSHOT_VERSION
            && version != SEMANTIC_SNAPSHOT_VERSION
            && version != MARKER_LIBRARY_SNAPSHOT_VERSION
            && version != LINE_JUMPS_SNAPSHOT_VERSION
            && version != EDGE_ROUTES_SNAPSHOT_VERSION
            && version != SEGMENT_ROUTES_SNAPSHOT_VERSION
            && version != EDGE_SHAPES_SNAPSHOT_VERSION
            && version != CONNECTOR_LABELS_SNAPSHOT_VERSION
            && version != OBJECT_DATA_SNAPSHOT_VERSION
            && version != LAYERS_SNAPSHOT_VERSION
            && version != TABLES_SNAPSHOT_VERSION
            && version != SHAPE_STYLES_SNAPSHOT_VERSION
            && version != LABEL_STYLES_SNAPSHOT_VERSION
            && version != RICH_LABELS_SNAPSHOT_VERSION
            && version != RICH_CONNECTOR_LABELS_SNAPSHOT_VERSION
            && version != RICH_EXTRA_CONNECTOR_LABELS_SNAPSHOT_VERSION
            && version != CARD_BASELINE_SNAPSHOT_VERSION
            && version != DRAWIO_DOCUMENT_SNAPSHOT_VERSION
            && version != DRAWIO_ZONE_IDS_SNAPSHOT_VERSION
            && version != DRAWIO_TOPIC_IDS_SNAPSHOT_VERSION
            && version != DRAWIO_GROUP_IDS_SNAPSHOT_VERSION
        {
            return Err(IoError::NewerVersion(version));
        }
        let (_, mut workbook): (String, Workbook) = serde_json::from_str(source)?;
        let group_envelope = version == DRAWIO_GROUP_IDS_SNAPSHOT_VERSION;
        if workbook.has_drawio_group_source_ids() != group_envelope {
            return Err(IoError::NewerVersion(
                "draw.io group source IDs require a group-ID-aware private XMind snapshot".into(),
            ));
        }
        if group_envelope {
            workbook
                .hydrate_stencil_packages()
                .map_err(IoError::Invalid)?;
            return Ok(workbook);
        }
        let topic_envelope = version == DRAWIO_TOPIC_IDS_SNAPSHOT_VERSION;
        if workbook.has_drawio_topic_source_ids() != topic_envelope {
            return Err(IoError::NewerVersion(
                "draw.io topic source IDs require a topic-ID-aware private XMind snapshot".into(),
            ));
        }
        if topic_envelope {
            workbook
                .hydrate_stencil_packages()
                .map_err(IoError::Invalid)?;
            return Ok(workbook);
        }
        let zone_envelope = version == DRAWIO_ZONE_IDS_SNAPSHOT_VERSION;
        if workbook.has_drawio_zone_source_ids() != zone_envelope {
            return Err(IoError::NewerVersion(
                "draw.io zone source IDs require a zone-ID-aware private XMind snapshot".into(),
            ));
        }
        if zone_envelope {
            workbook
                .hydrate_stencil_packages()
                .map_err(IoError::Invalid)?;
            return Ok(workbook);
        }
        let document_envelope = version == DRAWIO_DOCUMENT_SNAPSHOT_VERSION;
        if workbook.drawio.is_some() != document_envelope {
            return Err(IoError::NewerVersion(
                "draw.io document metadata requires a document-aware private XMind snapshot".into(),
            ));
        }
        if document_envelope {
            workbook
                .hydrate_stencil_packages()
                .map_err(IoError::Invalid)?;
            return Ok(workbook);
        }
        let card_baseline_envelope = version == CARD_BASELINE_SNAPSHOT_VERSION;
        if workbook.has_card_baseline_overrides() != card_baseline_envelope {
            return Err(IoError::NewerVersion(
                "card baseline overrides require a card-baseline-aware private XMind snapshot"
                    .into(),
            ));
        }
        let rich_extra_connector_labels_envelope = version
            == RICH_EXTRA_CONNECTOR_LABELS_SNAPSHOT_VERSION
            || card_baseline_envelope && workbook.has_rich_extra_connector_labels();
        if workbook.has_rich_extra_connector_labels() != rich_extra_connector_labels_envelope {
            return Err(IoError::NewerVersion(
                "rich extra connector labels require a rich-extra-connector-label-aware private XMind snapshot"
                    .into(),
            ));
        }
        let rich_connector_labels_envelope = version == RICH_CONNECTOR_LABELS_SNAPSHOT_VERSION
            || (rich_extra_connector_labels_envelope || card_baseline_envelope)
                && workbook.has_rich_connector_labels();
        if workbook.has_rich_connector_labels() != rich_connector_labels_envelope {
            return Err(IoError::NewerVersion(
                "rich connector labels require a rich-connector-label-aware private XMind snapshot"
                    .into(),
            ));
        }
        let rich_labels_envelope = version == RICH_LABELS_SNAPSHOT_VERSION
            || (rich_connector_labels_envelope
                || rich_extra_connector_labels_envelope
                || card_baseline_envelope)
                && workbook.has_rich_labels();
        if workbook.has_rich_labels() != rich_labels_envelope {
            return Err(IoError::NewerVersion(
                "rich labels require a rich-label-aware private XMind snapshot".into(),
            ));
        }
        let label_styles_envelope = version == LABEL_STYLES_SNAPSHOT_VERSION
            || (rich_labels_envelope
                || rich_connector_labels_envelope
                || rich_extra_connector_labels_envelope
                || card_baseline_envelope)
                && workbook.has_label_styles();
        if workbook.has_label_styles() != label_styles_envelope {
            return Err(IoError::NewerVersion(
                "label styles require a label-style-aware private XMind snapshot".into(),
            ));
        }
        let version = if !label_styles_envelope
            && !rich_labels_envelope
            && !rich_connector_labels_envelope
            && !rich_extra_connector_labels_envelope
            && !card_baseline_envelope
        {
            version
        } else if workbook.has_shape_styles() {
            SHAPE_STYLES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_tables() {
            TABLES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_layers() {
            LAYERS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_object_data() {
            OBJECT_DATA_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_labels() {
            CONNECTOR_LABELS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_edge_shapes() {
            EDGE_SHAPES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_segment_routes() {
            SEGMENT_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_edge_routes() {
            EDGE_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_line_jumps() {
            LINE_JUMPS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let shape_styles_envelope = version == SHAPE_STYLES_SNAPSHOT_VERSION;
        if workbook.has_shape_styles() != shape_styles_envelope {
            return Err(IoError::NewerVersion(
                "shape styles require a shape-style-aware private XMind snapshot".into(),
            ));
        }
        let version = if !shape_styles_envelope {
            version
        } else if workbook.has_tables() {
            TABLES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_layers() {
            LAYERS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_object_data() {
            OBJECT_DATA_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_labels() {
            CONNECTOR_LABELS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_edge_shapes() {
            EDGE_SHAPES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_segment_routes() {
            SEGMENT_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_edge_routes() {
            EDGE_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_line_jumps() {
            LINE_JUMPS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let tables_envelope = version == TABLES_SNAPSHOT_VERSION;
        if workbook.has_tables() != tables_envelope {
            return Err(IoError::NewerVersion(
                "tables require a table-aware private XMind snapshot".into(),
            ));
        }
        let version = if !tables_envelope {
            version
        } else if workbook.has_layers() {
            LAYERS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_object_data() {
            OBJECT_DATA_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_labels() {
            CONNECTOR_LABELS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_edge_shapes() {
            EDGE_SHAPES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_segment_routes() {
            SEGMENT_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_edge_routes() {
            EDGE_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_line_jumps() {
            LINE_JUMPS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let layers_envelope = version == LAYERS_SNAPSHOT_VERSION;
        if workbook.has_layers() != layers_envelope {
            return Err(IoError::NewerVersion(
                "layers require a layer-aware private XMind snapshot".into(),
            ));
        }
        let version = if !layers_envelope {
            version
        } else if workbook.has_object_data() {
            OBJECT_DATA_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_labels() {
            CONNECTOR_LABELS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_edge_shapes() {
            EDGE_SHAPES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_segment_routes() {
            SEGMENT_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_edge_routes() {
            EDGE_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_line_jumps() {
            LINE_JUMPS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let object_data_envelope = version == OBJECT_DATA_SNAPSHOT_VERSION;
        if workbook.has_object_data() != object_data_envelope {
            return Err(IoError::NewerVersion(
                "object data requires an object-data-aware private XMind snapshot".into(),
            ));
        }
        let version = if !object_data_envelope {
            version
        } else if workbook.has_connector_labels() {
            CONNECTOR_LABELS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_edge_shapes() {
            EDGE_SHAPES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_segment_routes() {
            SEGMENT_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_edge_routes() {
            EDGE_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_line_jumps() {
            LINE_JUMPS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let connector_labels_envelope = version == CONNECTOR_LABELS_SNAPSHOT_VERSION;
        if workbook.has_connector_labels() != connector_labels_envelope {
            return Err(IoError::NewerVersion(
                "connector labels require a connector-label-aware private XMind snapshot".into(),
            ));
        }
        let version = if !connector_labels_envelope {
            version
        } else if workbook.has_connector_edge_shapes() {
            EDGE_SHAPES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_segment_routes() {
            SEGMENT_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_edge_routes() {
            EDGE_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_line_jumps() {
            LINE_JUMPS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let edge_shapes_envelope = version == EDGE_SHAPES_SNAPSHOT_VERSION;
        if workbook.has_connector_edge_shapes() != edge_shapes_envelope {
            return Err(IoError::NewerVersion(
                "connector edge shapes require an edge-shape-aware private XMind snapshot".into(),
            ));
        }
        let version = if !edge_shapes_envelope {
            version
        } else if workbook.has_drawio_segment_routes() {
            SEGMENT_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_drawio_edge_routes() {
            EDGE_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_line_jumps() {
            LINE_JUMPS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let segment_routes_envelope = version == SEGMENT_ROUTES_SNAPSHOT_VERSION;
        if workbook.has_drawio_segment_routes() != segment_routes_envelope {
            return Err(IoError::NewerVersion(
                "draw.io segment routes require a segment-route-aware private XMind snapshot"
                    .into(),
            ));
        }
        let version = if !segment_routes_envelope {
            version
        } else if workbook.has_drawio_edge_routes() {
            EDGE_ROUTES_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_connector_line_jumps() {
            LINE_JUMPS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let edge_routes_envelope = version == EDGE_ROUTES_SNAPSHOT_VERSION;
        if workbook.has_drawio_edge_routes() != edge_routes_envelope {
            return Err(IoError::NewerVersion(
                "draw.io edge routes require an edge-route-aware private XMind snapshot".into(),
            ));
        }
        let version = if !edge_routes_envelope {
            version
        } else if workbook.has_connector_line_jumps() {
            LINE_JUMPS_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let line_jumps_envelope = version == LINE_JUMPS_SNAPSHOT_VERSION;
        if workbook.has_connector_line_jumps() != line_jumps_envelope {
            return Err(IoError::NewerVersion(
                "connector line jumps require a line-jump-aware private XMind snapshot".into(),
            ));
        }
        let version = if !line_jumps_envelope {
            version
        } else if workbook.has_library_connector_markers() {
            MARKER_LIBRARY_SNAPSHOT_VERSION.to_owned()
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let marker_library_envelope = version == MARKER_LIBRARY_SNAPSHOT_VERSION;
        if workbook.has_library_connector_markers() != marker_library_envelope {
            return Err(IoError::NewerVersion(
                "draw.io library connector markers require a marker-library-aware private XMind snapshot"
                    .into(),
            ));
        }
        let version = if !marker_library_envelope {
            version
        } else if workbook.has_semantic_model() {
            SEMANTIC_SNAPSHOT_VERSION.to_owned()
        } else {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        };
        let semantic_envelope = version == SEMANTIC_SNAPSHOT_VERSION;
        if workbook.has_semantic_model() != semantic_envelope {
            return Err(IoError::NewerVersion(
                "socio-technical data requires a semantic-aware private XMind snapshot".into(),
            ));
        }
        let version = if semantic_envelope {
            COLLAPSE_LAYOUT_SNAPSHOT_VERSION.to_owned()
        } else {
            version
        };
        if version != COLLAPSE_SNAPSHOT_VERSION
            && version != COLLAPSE_LAYOUT_SNAPSHOT_VERSION
            && workbook.has_scene_collapse()
        {
            return Err(IoError::NewerVersion(
                "collapsed containers require a collapse-aware private XMind snapshot".into(),
            ));
        }
        if version != SCENE_SNAPSHOT_VERSION
            && version != SCENE_STENCIL_SNAPSHOT_VERSION
            && version != COLLAPSE_SNAPSHOT_VERSION
            && version != SCENE_LAYOUT_SNAPSHOT_VERSION
            && version != SCENE_STENCIL_LAYOUT_SNAPSHOT_VERSION
            && version != COLLAPSE_LAYOUT_SNAPSHOT_VERSION
            && workbook.has_scene_hierarchy()
        {
            return Err(IoError::NewerVersion(
                "scene hierarchy requires a scene-aware private XMind snapshot".into(),
            ));
        }
        if version != SCENE_LAYOUT_SNAPSHOT_VERSION
            && version != SCENE_STENCIL_LAYOUT_SNAPSHOT_VERSION
            && version != COLLAPSE_LAYOUT_SNAPSHOT_VERSION
            && workbook.has_scene_layout_policies()
        {
            return Err(IoError::NewerVersion(
                "scene layout policies require a layout-aware private XMind snapshot".into(),
            ));
        }
        let version =
            if version == SCENE_SNAPSHOT_VERSION || version == SCENE_LAYOUT_SNAPSHOT_VERSION {
                STENCIL_SNAPSHOT_VERSION.to_owned()
            } else if version == SCENE_STENCIL_SNAPSHOT_VERSION
                || version == COLLAPSE_SNAPSHOT_VERSION
                || version == SCENE_STENCIL_LAYOUT_SNAPSHOT_VERSION
                || version == COLLAPSE_LAYOUT_SNAPSHOT_VERSION
            {
                CLOSED_STENCIL_SNAPSHOT_VERSION.to_owned()
            } else {
                version
            };
        if version != PINNED_STENCIL_SNAPSHOT_VERSION
            && version != CLOSED_STENCIL_SNAPSHOT_VERSION
            && workbook.has_stencil_definition_hashes()
        {
            return Err(IoError::NewerVersion(
                "pinned stencil definitions require a pinned-stencil-aware private XMind snapshot"
                    .into(),
            ));
        }
        if version != CLOSED_STENCIL_SNAPSHOT_VERSION && workbook.has_stencil_definition_closures()
        {
            return Err(IoError::NewerVersion(
                "stencil definition closures require a closure-aware private XMind snapshot".into(),
            ));
        }
        if version != STENCIL_SNAPSHOT_VERSION
            && version != PINNED_STENCIL_SNAPSHOT_VERSION
            && version != CLOSED_STENCIL_SNAPSHOT_VERSION
            && workbook.has_stencils()
        {
            return Err(IoError::NewerVersion(
                "stencil shapes require a stencil-aware private XMind snapshot".into(),
            ));
        }
        let version = if version == STENCIL_SNAPSHOT_VERSION
            || version == PINNED_STENCIL_SNAPSHOT_VERSION
            || version == CLOSED_STENCIL_SNAPSHOT_VERSION
        {
            SANKEY_SNAPSHOT_VERSION.to_owned()
        } else {
            version
        };
        if version != SANKEY_SNAPSHOT_VERSION && workbook.has_sankey() {
            return Err(IoError::NewerVersion(
                "Sankey flows require a Sankey-aware private XMind snapshot".into(),
            ));
        }
        let version = if version == SANKEY_SNAPSHOT_VERSION {
            FINAL_STATE_SNAPSHOT_VERSION.to_owned()
        } else {
            version
        };
        if version != FINAL_STATE_SNAPSHOT_VERSION && workbook.has_final_states() {
            return Err(IoError::NewerVersion(
                "final states require a final-state-aware private XMind snapshot".into(),
            ));
        }
        let version = if version == FINAL_STATE_SNAPSHOT_VERSION {
            DRAWIO_ROUNDING_SNAPSHOT_VERSION
        } else {
            version.as_str()
        };
        if version != DRAWIO_ROUNDING_SNAPSHOT_VERSION && workbook.has_drawio_rectangle_rounding() {
            return Err(IoError::NewerVersion(
                "standard draw.io rectangle rounding requires a rounding-aware private XMind snapshot"
                    .into(),
            ));
        }
        let version = if version == DRAWIO_ROUNDING_SNAPSHOT_VERSION {
            FIXED_PORT_SNAPSHOT_VERSION
        } else {
            version
        };
        if version != FIXED_PORT_SNAPSHOT_VERSION && workbook.has_fixed_connector_ports() {
            return Err(IoError::NewerVersion(
                "fixed connector ports require a port-aware private XMind snapshot".into(),
            ));
        }
        let version = if version == FIXED_PORT_SNAPSHOT_VERSION {
            DOUBLE_ELLIPSE_SNAPSHOT_VERSION
        } else {
            version
        };
        if version != DOUBLE_ELLIPSE_SNAPSHOT_VERSION && workbook.has_double_ellipses() {
            return Err(IoError::NewerVersion(
                "double ellipses require a double-ellipse-aware private XMind snapshot".into(),
            ));
        }
        let version = if version == DOUBLE_ELLIPSE_SNAPSHOT_VERSION {
            RECTANGLE_SIDES_SNAPSHOT_VERSION
        } else {
            version
        };
        if version != RECTANGLE_SIDES_SNAPSHOT_VERSION && workbook.has_rectangle_sides() {
            return Err(IoError::NewerVersion(
                "rectangle outline sides require a partial-rectangle-aware private XMind snapshot"
                    .into(),
            ));
        }
        // The rectangle-side envelope supports all earlier rounding-era features.
        let version = if version == RECTANGLE_SIDES_SNAPSHOT_VERSION {
            ROUNDING_SNAPSHOT_VERSION
        } else {
            version
        };
        if version != MARKER_SNAPSHOT_VERSION
            && version != EXTENDED_MARKER_SNAPSHOT_VERSION
            && version != SIZED_MARKER_SNAPSHOT_VERSION
            && version != FILLED_MARKER_SNAPSHOT_VERSION
            && version != DIAMOND_MARKER_SNAPSHOT_VERSION
            && version != OVAL_MARKER_SNAPSHOT_VERSION
            && version != HALF_CIRCLE_MARKER_SNAPSHOT_VERSION
            && version != BAR_MARKER_SNAPSHOT_VERSION
            && version != ROUNDING_SNAPSHOT_VERSION
            && workbook.has_connector_markers()
        {
            return Err(IoError::NewerVersion(
                "connector markers require a marker-aware private XMind snapshot".into(),
            ));
        }
        if version != EXTENDED_MARKER_SNAPSHOT_VERSION
            && version != SIZED_MARKER_SNAPSHOT_VERSION
            && version != FILLED_MARKER_SNAPSHOT_VERSION
            && version != DIAMOND_MARKER_SNAPSHOT_VERSION
            && version != OVAL_MARKER_SNAPSHOT_VERSION
            && version != HALF_CIRCLE_MARKER_SNAPSHOT_VERSION
            && version != BAR_MARKER_SNAPSHOT_VERSION
            && version != ROUNDING_SNAPSHOT_VERSION
            && workbook.has_extended_connector_markers()
        {
            return Err(IoError::NewerVersion(
                "classic and thin connector markers require an extended-marker private XMind snapshot"
                    .into(),
            ));
        }
        if version != SIZED_MARKER_SNAPSHOT_VERSION
            && version != FILLED_MARKER_SNAPSHOT_VERSION
            && version != DIAMOND_MARKER_SNAPSHOT_VERSION
            && version != OVAL_MARKER_SNAPSHOT_VERSION
            && version != HALF_CIRCLE_MARKER_SNAPSHOT_VERSION
            && version != BAR_MARKER_SNAPSHOT_VERSION
            && version != ROUNDING_SNAPSHOT_VERSION
            && workbook.has_connector_marker_sizes()
        {
            return Err(IoError::NewerVersion(
                "connector marker sizes require a size-aware private XMind snapshot".into(),
            ));
        }
        if version != FILLED_MARKER_SNAPSHOT_VERSION
            && version != DIAMOND_MARKER_SNAPSHOT_VERSION
            && version != OVAL_MARKER_SNAPSHOT_VERSION
            && version != HALF_CIRCLE_MARKER_SNAPSHOT_VERSION
            && version != BAR_MARKER_SNAPSHOT_VERSION
            && version != ROUNDING_SNAPSHOT_VERSION
            && workbook.has_connector_marker_fills()
        {
            return Err(IoError::NewerVersion(
                "connector marker fills require a fill-aware private XMind snapshot".into(),
            ));
        }
        if version != DIAMOND_MARKER_SNAPSHOT_VERSION
            && version != OVAL_MARKER_SNAPSHOT_VERSION
            && version != HALF_CIRCLE_MARKER_SNAPSHOT_VERSION
            && version != BAR_MARKER_SNAPSHOT_VERSION
            && version != ROUNDING_SNAPSHOT_VERSION
            && workbook.has_diamond_connector_markers()
        {
            return Err(IoError::NewerVersion(
                "diamond connector markers require a diamond-aware private XMind snapshot".into(),
            ));
        }
        if version != OVAL_MARKER_SNAPSHOT_VERSION
            && version != HALF_CIRCLE_MARKER_SNAPSHOT_VERSION
            && version != BAR_MARKER_SNAPSHOT_VERSION
            && version != ROUNDING_SNAPSHOT_VERSION
            && workbook.has_oval_connector_markers()
        {
            return Err(IoError::NewerVersion(
                "oval connector markers require an oval-aware private XMind snapshot".into(),
            ));
        }
        if version != HALF_CIRCLE_MARKER_SNAPSHOT_VERSION
            && version != BAR_MARKER_SNAPSHOT_VERSION
            && version != ROUNDING_SNAPSHOT_VERSION
            && workbook.has_half_circle_connector_markers()
        {
            return Err(IoError::NewerVersion(
                "half-circle connector markers require a half-circle-aware private XMind snapshot"
                    .into(),
            ));
        }
        if version != BAR_MARKER_SNAPSHOT_VERSION
            && version != ROUNDING_SNAPSHOT_VERSION
            && workbook.has_bar_connector_markers()
        {
            return Err(IoError::NewerVersion(
                "bar connector markers require a bar-aware private XMind snapshot".into(),
            ));
        }
        if version != ROUNDING_SNAPSHOT_VERSION && workbook.has_shape_rounding() {
            return Err(IoError::NewerVersion(
                "shape rounding requires a rounding-aware private XMind snapshot".into(),
            ));
        }
        workbook
            .hydrate_stencil_packages()
            .map_err(IoError::Invalid)?;
        Ok(workbook)
    } else {
        let workbook: Workbook = serde_json::from_str(source)?;
        if workbook.has_drawio_group_source_ids() {
            return Err(IoError::NewerVersion(
                "draw.io group source IDs require a group-ID-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_drawio_topic_source_ids() {
            return Err(IoError::NewerVersion(
                "draw.io topic source IDs require a topic-ID-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_drawio_zone_source_ids() {
            return Err(IoError::NewerVersion(
                "draw.io zone source IDs require a zone-ID-aware private XMind snapshot".into(),
            ));
        }
        if workbook.drawio.is_some() {
            return Err(IoError::NewerVersion(
                "draw.io document metadata requires a document-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_card_baseline_overrides() {
            return Err(IoError::NewerVersion(
                "card baseline overrides require a card-baseline-aware private XMind snapshot"
                    .into(),
            ));
        }
        if workbook.has_rich_labels() {
            return Err(IoError::NewerVersion(
                "rich labels require a rich-label-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_label_styles() {
            return Err(IoError::NewerVersion(
                "label styles require a label-style-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_shape_styles() {
            return Err(IoError::NewerVersion(
                "shape styles require a shape-style-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_tables() {
            return Err(IoError::NewerVersion(
                "tables require a table-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_layers() {
            return Err(IoError::NewerVersion(
                "layers require a layer-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_object_data() {
            return Err(IoError::NewerVersion(
                "object data requires an object-data-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_connector_labels() {
            return Err(IoError::NewerVersion(
                "connector labels require a connector-label-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_connector_edge_shapes() {
            return Err(IoError::NewerVersion(
                "connector edge shapes require an edge-shape-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_drawio_segment_routes() {
            return Err(IoError::NewerVersion(
                "draw.io segment routes require a segment-route-aware private XMind snapshot"
                    .into(),
            ));
        }
        if workbook.has_drawio_edge_routes() {
            return Err(IoError::NewerVersion(
                "draw.io edge routes require an edge-route-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_connector_line_jumps() {
            return Err(IoError::NewerVersion(
                "connector line jumps require a line-jump-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_library_connector_markers() {
            return Err(IoError::NewerVersion(
                "draw.io library connector markers require a marker-library-aware private XMind snapshot"
                    .into(),
            ));
        }
        if workbook.has_semantic_model() {
            return Err(IoError::NewerVersion(
                "socio-technical data requires a semantic-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_scene_hierarchy() {
            return Err(IoError::NewerVersion(
                "scene hierarchy requires a scene-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_stencils() {
            return Err(IoError::NewerVersion(
                "stencil shapes require a stencil-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_sankey() {
            return Err(IoError::NewerVersion(
                "Sankey flows require a Sankey-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_final_states() {
            return Err(IoError::NewerVersion(
                "final states require a final-state-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_fixed_connector_ports() {
            return Err(IoError::NewerVersion(
                "fixed connector ports require a port-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_double_ellipses() {
            return Err(IoError::NewerVersion(
                "double ellipses require a double-ellipse-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_rectangle_sides() {
            return Err(IoError::NewerVersion(
                "rectangle outline sides require a partial-rectangle-aware private XMind snapshot"
                    .into(),
            ));
        }
        if workbook.has_shape_rounding() {
            return Err(IoError::NewerVersion(
                "shape rounding requires a rounding-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_connector_markers() {
            return Err(IoError::NewerVersion(
                "connector markers require a marker-aware private XMind snapshot".into(),
            ));
        }
        if workbook.has_connector_waypoints() {
            return Err(IoError::Invalid(
                "connector waypoints require a versioned private XMind snapshot".into(),
            ));
        }
        Ok(workbook)
    }
}

pub(crate) fn export(book: &Workbook) -> Result<Vec<u8>, IoError> {
    let resource_files = export_resource_files(book)?;
    let stable = private_snapshot(book)?;
    let sheets = public_content(book, PUBLIC_PROJECTION)?;
    let content = serde_json::to_vec(&sheets)?;
    let metadata = serde_json::to_vec(&json!({
        "creator": {"name": "Thinoda", "version": env!("CARGO_PKG_VERSION")},
        "dataStructureVersion": "2",
        "thinodaWorkbook": stable,
        "thinodaPublicProjection": PUBLIC_PROJECTION,
        "thinodaContentSha256": content_digest(&sheets)?
    }))?;
    let mut file_entries = serde_json::Map::new();
    file_entries.insert("content.json".into(), json!({}));
    file_entries.insert("metadata.json".into(), json!({}));
    for path in resource_files.keys() {
        file_entries.insert(path.clone(), json!({}));
    }
    let manifest = serde_json::to_vec(&json!({"file-entries":file_entries}))?;
    let xml = legacy_compatibility(book).into_bytes();
    let mut out = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut out);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in [
        ("content.json", content.as_slice()),
        ("metadata.json", metadata.as_slice()),
        ("manifest.json", manifest.as_slice()),
        ("content.xml", xml.as_slice()),
    ] {
        zip.start_file(name, options)?;
        zip.write_all(data)?;
    }
    for (path, data) in resource_files {
        zip.start_file(path, options)?;
        zip.write_all(&data)?;
    }
    zip.finish()?;
    Ok(out.into_inner())
}
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn legacy_compatibility(book: &Workbook) -> String {
    fn topic(sheet: &Sheet, id: TopicId, out: &mut String, depth: usize) {
        if depth > 2000 {
            return;
        }
        let t = &sheet.topics[&id];
        let structure_attr = if id == sheet.root {
            Some(t.structure.unwrap_or(sheet.structure).xmind_class())
        } else {
            t.structure.map(Structure::xmind_class)
        };
        out.push_str(&format!(
            "<topic id=\"{}\"{}{}>",
            id.0,
            if t.collapsed {
                " branch=\"folded\""
            } else {
                ""
            },
            structure_attr
                .map(|structure| format!(" structure-class=\"{}\"", xml_escape(structure)))
                .unwrap_or_default()
        ));
        let style = export_topic_style(sheet, id);
        let mut properties = Vec::new();
        if let Some(color) = style.text_color {
            properties.push(format!("fo:color=\"{}\"", color.to_hex()));
        }
        if let Some(color) = style.fill {
            properties.push(format!("background-color=\"{}\"", color.to_hex()));
        }
        if let Some(color) = style.border_color {
            properties.push(format!("svg:stroke=\"{}\"", color.to_hex()));
        }
        if let Some(width) = style.border_width {
            properties.push(format!("svg:stroke-width=\"{width}\""));
        }
        if let Some(shape) = style.shape {
            properties.push(format!("shape-class=\"{}\"", shape_class(shape)));
        }
        if let Some(size) = style.font_size {
            properties.push(format!("fo:font-size=\"{size}pt\""));
        }
        if let Some(family) = style
            .font_family
            .as_ref()
            .or(sheet.theme.font_family.as_ref())
        {
            properties.push(format!("fo:font-family=\"{}\"", xml_escape(family)));
        }
        if let Some(bold) = style.bold {
            properties.push(format!(
                "fo:font-weight=\"{}\"",
                if bold { "bold" } else { "normal" }
            ));
        }
        if let Some(italic) = style.italic {
            properties.push(format!(
                "fo:font-style=\"{}\"",
                if italic { "italic" } else { "normal" }
            ));
        }
        let mut decoration = Vec::new();
        if style.underline == Some(true) {
            decoration.push("underline");
        }
        if style.strikethrough == Some(true) {
            decoration.push("line-through");
        }
        if !decoration.is_empty() {
            properties.push(format!("fo:text-decoration=\"{}\"", decoration.join(" ")));
        }
        if let Some(align) = style.align {
            properties.push(format!(
                "fo:text-align=\"{}\"",
                match align {
                    TextAlign::Left => "left",
                    TextAlign::Center => "center",
                    TextAlign::Right => "right",
                }
            ));
        }
        if let Some(color) = style.line_color {
            properties.push(format!("line-color=\"{}\"", color.to_hex()));
        }
        if let Some(width) = style.line_width.or(sheet.theme.line_width) {
            properties.push(format!("line-width=\"{width}\""));
        }
        if let Some(line_shape) = style.line_shape {
            properties.push(format!("line-class=\"{}\"", line_class(line_shape)));
        }
        if !properties.is_empty() {
            out.push_str("<style><topic-properties ");
            out.push_str(&properties.join(" "));
            out.push_str("/></style>");
        }
        out.push_str("<title>");
        out.push_str(&xml_escape(&export_title(sheet, id, &t.title)));
        out.push_str("</title>");
        if let Some(note) = &t.notes {
            out.push_str("<notes><plain>");
            out.push_str(&xml_escape(note));
            out.push_str("</plain></notes>");
        }
        if let Some(link) = &t.hyperlink {
            out.push_str(&format!("<hyperlink xlink:href=\"{}\"/>", xml_escape(link)));
        }
        for marker in &t.markers {
            out.push_str(&format!(
                "<marker-refs><marker-ref marker-id=\"{}\"/></marker-refs>",
                xml_escape(marker)
            ));
        }
        for label in &t.labels {
            out.push_str(&format!(
                "<labels><label>{}</label></labels>",
                xml_escape(label)
            ));
        }
        for child in &t.children {
            topic(sheet, *child, out, depth + 1);
        }
        out.push_str("</topic>");
    }
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><xmap-content xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:fo=\"http://www.w3.org/1999/XSL/Format\" xmlns:svg=\"http://www.w3.org/2000/svg\" version=\"2.0\">",
    );
    for sheet in &book.sheets {
        out.push_str(&format!(
            "<sheet title=\"{}\"><topic-positioning>free</topic-positioning>",
            xml_escape(&sheet.title)
        ));
        topic(sheet, sheet.root, &mut out, 0);
        out.push_str("</sheet>");
    }
    out.push_str("</xmap-content>");
    out
}
fn color(s: &str) -> Option<Color> {
    Color::from_hex(s)
}
fn parse_range(value: &Value) -> Option<ChildRange> {
    let text = value.as_str()?.trim();
    if text.eq_ignore_ascii_case("master") {
        return None;
    }
    let mut parts = text
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(',');
    let start = parts.next()?.trim().parse().ok()?;
    let end = parts.next()?.trim().parse().ok()?;
    Some(ChildRange { start, end })
}

fn parse_number(value: &Value) -> Option<f32> {
    value.as_f64().map(|x| x as f32).or_else(|| {
        value
            .as_str()
            .and_then(|x| x.trim().trim_end_matches("pt").parse().ok())
    })
}

/// Trimmed explicit style value. Blank values and XMind's `inherited` keyword defer
/// to the default, exactly as if the property were absent.
fn style_value<'a>(props: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    props
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("inherited"))
}

fn style_number(props: &serde_json::Map<String, Value>, key: &str) -> Option<f32> {
    match props.get(key)? {
        Value::String(_) => style_value(props, key).and_then(|text| {
            text.trim_end_matches("pt")
                .trim_end_matches("px")
                .trim()
                .parse()
                .ok()
        }),
        value => parse_number(value),
    }
    .filter(|number: &f32| number.is_finite())
}

/// CSS weights: keywords or numbers, where 600 and above render bold.
fn parse_font_weight(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "bold" | "bolder" => Some(true),
        "normal" | "lighter" => Some(false),
        number => number
            .parse::<f32>()
            .ok()
            .filter(|weight| weight.is_finite())
            .map(|weight| weight >= 600.0),
    }
}

/// Lower-cased class suffix without separators, so `roundedRect`, `rounded-rect`
/// and `org.xmind.topicShape.roundedRect` compare equal.
fn class_key(class: &str) -> String {
    class
        .rsplit('.')
        .next()
        .unwrap_or(class)
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-' && *ch != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Clamps an XMind child range to the parent's attached children. Reversed ranges
/// (as written by some generators) are swapped; out-of-range ends are reported.
fn normalize_range(
    range: ChildRange,
    children: usize,
    context: &TopicImportContext<'_>,
) -> Option<ChildRange> {
    if children == 0 {
        context.warn(ImportWarning::Other);
        return None;
    }
    let (start, end) = if range.start <= range.end {
        (range.start, range.end)
    } else {
        (range.end, range.start)
    };
    let last = children - 1;
    if end > last {
        context.warn(ImportWarning::Other);
    }
    let end = end.min(last);
    Some(ChildRange {
        start: start.min(end),
        end,
    })
}

const XMIND_NUMBER_FORMATS: [(&str, NumberFormat); 4] = [
    ("org.xmind.numbering.arabic", NumberFormat::Decimal),
    ("org.xmind.numbering.roman", NumberFormat::UpperRoman),
    ("org.xmind.numbering.lowercase", NumberFormat::LowerAlpha),
    ("org.xmind.numbering.uppercase", NumberFormat::UpperAlpha),
];

/// XMind has no lower-case Roman numbering; it is written as upper-case Roman.
fn xmind_number_format(format: NumberFormat) -> &'static str {
    match format {
        NumberFormat::Decimal => "org.xmind.numbering.arabic",
        NumberFormat::UpperRoman | NumberFormat::LowerRoman => "org.xmind.numbering.roman",
        NumberFormat::LowerAlpha => "org.xmind.numbering.lowercase",
        NumberFormat::UpperAlpha => "org.xmind.numbering.uppercase",
    }
}

/// Reads `numbering` (which numbers the topic's children). Prefixes, suffixes,
/// custom separators, depth settings and unknown formats cannot be represented
/// and are reported; an unknown format falls back to decimal numbers.
fn import_numbering(value: &Value, context: &TopicImportContext<'_>) -> Option<Numbering> {
    let numbering = value.as_object()?;
    for (key, field) in numbering {
        let default = match key.as_str() {
            "numberFormat" | "prependingNumbers" => continue,
            "numberSeparator" => field.as_str().is_some_and(|text| text.trim() == "."),
            _ => !non_empty(Some(field)),
        };
        if !default {
            context.warn(ImportWarning::Formatting);
        }
    }
    let format = numbering
        .get("numberFormat")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|format| !format.is_empty())?;
    if format == "org.xmind.numbering.none" {
        return None;
    }
    let format = XMIND_NUMBER_FORMATS
        .iter()
        .find(|(id, _)| *id == format)
        .map(|(_, format)| *format)
        .unwrap_or_else(|| {
            context.warn(ImportWarning::Formatting);
            NumberFormat::Decimal
        });
    let tiered = numbering
        .get("prependingNumbers")
        .and_then(Value::as_str)
        .is_none_or(|prepend| prepend.trim() != "none");
    Some(Numbering {
        format,
        tiered,
        start: 1,
    })
}

/// Thinoda's topic width bounds (the Style panel's "Topic Width" range).
const TOPIC_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 1.0..=10_000.0;

/// `customWidth` is a number or numeric string; 0 means automatic, as in XMind.
fn import_custom_width(value: &Value, context: &TopicImportContext<'_>) -> Option<f32> {
    let width = match value {
        Value::Null => return None,
        Value::String(text) if text.trim().is_empty() => return None,
        value => parse_number(value).filter(|width| width.is_finite()),
    };
    match width {
        Some(0.0) => None,
        Some(width) if TOPIC_WIDTH_RANGE.contains(&width) => Some(width),
        _ => {
            context.warn(ImportWarning::Formatting);
            None
        }
    }
}

const UNBALANCED_PROVIDER: &str = "org.xmind.ui.map.unbalanced";

/// Number of leading children placed on the right of an unbalanced map.
fn unbalanced_right_number(topic: &Value) -> Option<usize> {
    topic
        .get("extensions")?
        .as_array()?
        .iter()
        .filter(|extension| {
            extension.get("provider").and_then(Value::as_str) == Some(UNBALANCED_PROVIDER)
        })
        .filter_map(|extension| extension.get("content")?.as_array())
        .flatten()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some("right-number"))
        .and_then(|entry| match entry.get("content")? {
            Value::String(text) => text.trim().parse().ok(),
            value => value
                .as_u64()
                .and_then(|number| usize::try_from(number).ok()),
        })
}

/// Map-level theme properties: a non-white background and tapered branch lines.
/// White is XMind's default canvas and keeps Thinoda's light/dark-mode background.
fn import_sheet_theme(theme: &Value, sheet: &mut Sheet) {
    let Some(props) = theme
        .get("map")
        .and_then(|map| map.get("properties"))
        .and_then(Value::as_object)
    else {
        return;
    };
    if let Some(background) = style_value(props, "svg:fill").and_then(color)
        && !matches!(
            background
                .to_hex()
                .trim_start_matches('#')
                .to_ascii_uppercase()
                .as_str(),
            "FFFFFF" | "FFFFFFFF"
        )
    {
        sheet.theme.background = Some(background);
    }
    match style_value(props, "line-tapered")
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("none") => sheet.theme.tapered_lines = false,
        Some("tapered") => sheet.theme.tapered_lines = true,
        _ => {}
    }
}

/// XMind's Cartesian `controlPoints` (offsets from each end topic). The polar
/// `{angle, amount}` form and incomplete or invalid values use automatic routing.
fn import_control_points(
    relationship: &Value,
    context: &TopicImportContext<'_>,
) -> Option<[Vec2; 2]> {
    let points = relationship.get("controlPoints")?;
    if !non_empty(Some(points)) {
        return None;
    }
    let point = |key: &str| -> Option<Vec2> {
        let point = points.get(key)?;
        Some(Vec2::new(
            parse_number(point.get("x")?)?,
            parse_number(point.get("y")?)?,
        ))
    };
    let parsed = point("0")
        .zip(point("1"))
        .map(|(a, b)| [a, b])
        .filter(|points| thinoda_core::Relationship::valid_control_points(Some(*points)));
    if parsed.is_none() {
        context.warn(ImportWarning::Formatting);
    }
    parsed
}

/// Visible text of an XMind `realHTML` note. Tags are dropped (never interpreted)
/// and block boundaries become line breaks.
fn html_note_text(html: &str) -> String {
    let mut text = String::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        text.push_str(&decode_html_entities(&rest[..start]));
        let Some(end) = rest[start..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[start + 1..start + end]
            .trim_start_matches('/')
            .split(|c: char| c.is_ascii_whitespace() || c == '/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let closing = rest[start + 1..].starts_with('/');
        if tag == "br" || (closing && matches!(tag.as_str(), "p" | "div" | "li" | "tr")) {
            text.push('\n');
        }
        rest = &rest[start + end + 1..];
    }
    text.push_str(&decode_html_entities(rest));
    let lines: Vec<_> = text.lines().map(str::trim_end).collect();
    lines.join("\n").trim().to_owned()
}

fn decode_html_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        rest = &rest[start..];
        let decoded = rest[1..]
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| {
                let entity = &rest[1..=end];
                let character = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" | "#39" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => entity
                        .strip_prefix("#x")
                        .or_else(|| entity.strip_prefix("#X"))
                        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                        .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                        .and_then(char::from_u32)
                        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t')),
                }?;
                Some((character, end + 2))
            });
        match decoded {
            Some((character, length)) => {
                output.push(character);
                rest = &rest[length..];
            }
            None => {
                output.push('&');
                rest = &rest[1..];
            }
        }
    }
    output.push_str(rest);
    output
}

/// Whether HTML carries more than paragraphs and line breaks (styles, lists, links).
fn html_has_formatting(html: &str) -> bool {
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else {
            return false;
        };
        let inner = rest[start + 1..start + end]
            .trim_start_matches('/')
            .trim_end_matches('/');
        let mut parts = inner.split_ascii_whitespace();
        let name = parts.next().unwrap_or_default().to_ascii_lowercase();
        if parts.next().is_some() || !matches!(name.as_str(), "p" | "div" | "br" | "span") {
            return true;
        }
        rest = &rest[start + end + 1..];
    }
    false
}

/// Notes prefer `plain`; HTML or delta-only notes keep their visible text and are reported.
fn import_notes(notes: &Value, context: &TopicImportContext<'_>) -> Option<String> {
    let plain = notes
        .get("plain")
        .and_then(|x| x.get("content"))
        .and_then(Value::as_str);
    let html = notes
        .get("realHTML")
        .and_then(|x| x.get("content"))
        .and_then(Value::as_str);
    let ops = notes
        .get("ops")
        .and_then(|x| x.get("ops"))
        .and_then(Value::as_array);
    let ops_formatted = ops.is_some_and(|ops| {
        ops.iter().any(|op| {
            op.get("attributes")
                .and_then(Value::as_object)
                .is_some_and(|attributes| !attributes.is_empty())
                || op.get("insert").is_some_and(|insert| !insert.is_string())
        })
    });
    if let Some(plain) = plain {
        if html.is_some_and(html_has_formatting) || (html.is_none() && ops_formatted) {
            context.warn(ImportWarning::Notes);
        }
        return Some(plain.to_owned());
    }
    if let Some(html) = html {
        context.warn(ImportWarning::Notes);
        return Some(html_note_text(html)).filter(|text| !text.is_empty());
    }
    if let Some(ops) = ops {
        context.warn(ImportWarning::Notes);
        let text: String = ops
            .iter()
            .filter_map(|op| op.get("insert").and_then(Value::as_str))
            .collect();
        let text = text.trim_end_matches('\n').to_owned();
        return Some(text).filter(|text| !text.is_empty());
    }
    if notes.as_object().is_some_and(|notes| !notes.is_empty()) {
        context.warn(ImportWarning::Notes);
    }
    None
}

/// Reports source content that has no Thinoda equivalent yet.
fn canonical_marker_id(id: &str) -> &str {
    if id.eq_ignore_ascii_case("week-web") {
        "week-wed"
    } else {
        id
    }
}

fn report_unsupported_topic_data(v: &Value, context: &TopicImportContext<'_>) {
    if v.get("taskInfo").is_some_and(|task| !task.is_null()) {
        context.warn(ImportWarning::Markers);
    }
    if v.get("markers").is_some_and(|value| {
        value.as_array().map_or_else(
            || non_empty(Some(value)),
            |markers| {
                markers.iter().any(|marker| {
                    marker
                        .get("markerId")
                        .or_else(|| marker.get("id"))
                        .and_then(Value::as_str)
                        .is_none_or(|id| {
                            thinoda_core::markers::find(canonical_marker_id(id)).is_none()
                        })
                })
            },
        )
    }) {
        context.warn(ImportWarning::Markers);
    }
    if v.get("structureClass").is_some_and(|value| {
        non_empty(Some(value))
            && value.as_str().is_none_or(|class| {
                let class = class.trim();
                Structure::from_xmind_class(class).is_none_or(|structure| {
                    class != "org.xmind.ui.map" && class != structure.xmind_class()
                })
            })
    }) {
        context.warn(ImportWarning::Formatting);
    }
    if v.get("labels").is_some_and(|value| {
        value.as_array().map_or_else(
            || non_empty(Some(value)),
            |labels| labels.iter().any(|label| !label.is_string()),
        )
    }) {
        context.warn(ImportWarning::Other);
    }
    if v.get("href")
        .and_then(Value::as_str)
        .is_some_and(|href| href.starts_with("xmind:"))
    {
        context.warn(ImportWarning::Links);
    }
    if ["comments", "audioNotes"]
        .iter()
        .any(|key| non_empty(v.get(key)))
    {
        context.warn(ImportWarning::Other);
    }
    if v.get("children").is_some_and(|children| {
        children.as_object().map_or_else(
            || non_empty(Some(children)),
            |children| {
                children.iter().any(|(kind, values)| {
                    non_empty(Some(values))
                        && (!TOPIC_COLLECTIONS.contains(&kind.as_str())
                            || values
                                .as_array()
                                .is_none_or(|topics| topics.iter().any(|topic| !topic.is_object())))
                })
            },
        )
    }) {
        context.warn(ImportWarning::Other);
    }
}

fn imported_topic_title<'a>(v: &'a Value, context: &TopicImportContext<'_>) -> Cow<'a, str> {
    let plain = v.get("title").and_then(Value::as_str);
    let source = v.get("attributedTitle");
    let runs = source
        .and_then(Value::as_array)
        .filter(|runs| !runs.is_empty());
    let attributed = runs.and_then(|runs| {
        runs.iter()
            .map(|run| run.get("text").and_then(Value::as_str))
            .collect::<Option<String>>()
    });
    if v.get("title")
        .is_some_and(|title| !title.is_null() && !title.is_string())
        || (non_empty(source)
            && (runs.is_none_or(|runs| {
                runs.iter().any(|run| {
                    run.as_object()
                        .is_none_or(|run| run.keys().any(|key| key != "text"))
                })
            }) || attributed
                .as_deref()
                .is_none_or(|text| plain.is_some_and(|title| title != text))))
    {
        context.warn(ImportWarning::Formatting);
    }
    match (plain, attributed) {
        (Some(title), _) => Cow::Borrowed(title),
        (None, Some(title)) => Cow::Owned(title),
        (None, None) => Cow::Borrowed("Untitled"),
    }
}

fn non_empty(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Array(values)) => !values.is_empty(),
        Some(Value::Object(object)) => !object.is_empty(),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// Parses a callout or nested detached topic as a floating topic linked to its owner.
fn parse_linked_floating(
    v: &Value,
    s: &mut Sheet,
    owner: TopicId,
    depth: usize,
    idmap: &mut HashMap<String, TopicId>,
    context: &TopicImportContext<'_>,
    role: &'static str,
) -> Result<(), IoError> {
    let position = v.get("position").and_then(|p| {
        let x = p.get("x").and_then(Value::as_f64)? as f32;
        let y = p.get("y").and_then(Value::as_f64)? as f32;
        (x.is_finite() && y.is_finite()).then(|| Vec2::new(x, y))
    });
    let fid = parse_topic(v, s, Some(owner), depth + 1, idmap, context, role)?;
    s.topic_mut(owner)
        .unwrap()
        .children
        .retain(|child| *child != fid);
    let floating = s.topic_mut(fid).unwrap();
    floating.parent = None;
    floating.position = Some(Vec2::ZERO);
    s.floating.push(fid);
    context
        .floating_links
        .borrow_mut()
        .push((owner, fid, position));
    context.warn(ImportWarning::Other);
    Ok(())
}

fn parse_topic(
    v: &Value,
    s: &mut Sheet,
    parent: Option<TopicId>,
    depth: usize,
    idmap: &mut HashMap<String, TopicId>,
    context: &TopicImportContext<'_>,
    role: &'static str,
) -> Result<TopicId, IoError> {
    if depth > 2000 {
        return Err(IoError::Invalid("XMind topic nesting exceeds 2000".into()));
    }
    let title = imported_topic_title(v, context);
    let id = match parent {
        Some(parent_id) => s.add_child(parent_id, None, title.as_ref()),
        None => s.root,
    };
    if parent.is_none() {
        s.topics.get_mut(&id).unwrap().title = title.to_string();
    }
    let image = v
        .get("image")
        .and_then(|image| image_attributes(image, context));
    if let Some(key) = v.get("id").and_then(Value::as_str) {
        idmap.insert(key.to_string(), id);
    }
    report_unsupported_topic_data(v, context);
    let notes = v
        .get("notes")
        .and_then(|notes| import_notes(notes, context));
    // Package files linked from the topic become Thinoda attachments; when the
    // file was skipped by the inventory the link is kept as text.
    let attachment = v
        .get("href")
        .and_then(Value::as_str)
        .filter(|href| href.starts_with("xap:resources/"))
        .and_then(|href| image_component(href).ok())
        .and_then(|(_, path)| context.resources.original.get(&path))
        .map(|resource| -> Result<_, IoError> {
            let reference = s
                .resources
                .insert(resource.clone())
                .map_err(IoError::Invalid)?;
            // XMind titles attachment topics with the file name; Thinoda's own
            // exports prefix it with the attachment glyph.
            let name = title
                .strip_prefix('📎')
                .map(str::trim_start)
                .unwrap_or(&title);
            Ok(thinoda_core::attachments::Attachment {
                resource: reference,
                name: thinoda_core::attachments::safe_name(name),
            })
        })
        .transpose()?;
    let t = s.topic_mut(id).unwrap();
    t.collapsed = v.get("branch").and_then(Value::as_str) == Some("folded");
    t.hyperlink = v.get("href").and_then(Value::as_str).map(str::to_owned);
    if attachment.is_some() {
        t.hyperlink = None;
        t.attachment = attachment;
    }
    if let Some(n) = notes {
        t.notes = Some(n.into());
    }
    if let Some(a) = v.get("labels").and_then(Value::as_array) {
        t.labels = a
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
    }
    if let Some(a) = v.get("markers").and_then(Value::as_array) {
        t.markers = a
            .iter()
            .filter_map(|x| x.get("markerId").or_else(|| x.get("id")))
            .filter_map(Value::as_str)
            .map(canonical_marker_id)
            .map(str::to_owned)
            .collect();
    }
    if let Some(st) = v
        .get("structureClass")
        .and_then(Value::as_str)
        .and_then(Structure::from_xmind_class)
    {
        t.structure = Some(st);
    }
    let mut props = context
        .theme
        .get(role)
        .and_then(|style| style.get("properties"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(overrides) = v
        .get("style")
        .and_then(|x| x.get("properties"))
        .and_then(Value::as_object)
    {
        props.extend(overrides.clone());
    }
    if !props.is_empty() {
        let style = &mut s.topic_mut(id).unwrap().style;
        let color_value = |key: &str| style_value(&props, key).and_then(color);
        style.fill = color_value("svg:fill");
        style.text_color = color_value("fo:color");
        style.border_color = color_value("border-line-color").or_else(|| color_value("svg:stroke"));
        style.border_width = style_number(&props, "border-line-width")
            .or_else(|| style_number(&props, "svg:stroke-width"));
        style.bold = style_value(&props, "fo:font-weight").and_then(|weight| {
            let bold = parse_font_weight(weight);
            if bold.is_none() {
                context.warn(ImportWarning::Formatting);
            }
            bold
        });
        style.italic = style_value(&props, "fo:font-style")
            .map(|value| matches!(value.to_ascii_lowercase().as_str(), "italic" | "oblique"));
        style.font_size = style_number(&props, "fo:font-size");
        style.font_family = style_value(&props, "fo:font-family").map(str::to_owned);
        if let Some(decoration) = style_value(&props, "fo:text-decoration") {
            let decoration = decoration.to_ascii_lowercase();
            if decoration == "none" {
                style.underline = Some(false);
                style.strikethrough = None;
            } else {
                style.underline = decoration.contains("underline").then_some(true);
                style.strikethrough = decoration.contains("line-through").then_some(true);
            }
        }
        style.align = style_value(&props, "fo:text-align").and_then(|value| {
            match value.to_ascii_lowercase().as_str() {
                "left" => Some(TextAlign::Left),
                "center" => Some(TextAlign::Center),
                "right" => Some(TextAlign::Right),
                _ => None,
            }
        });
        style.line_color = color_value("line-color");
        style.line_width = style_number(&props, "line-width");
        style.shape = style_value(&props, "shape-class").and_then(|class| {
            parse_shape_class(class).or_else(|| {
                context.warn(ImportWarning::Formatting);
                approximate_shape_class(class)
            })
        });
        style.line_shape = style_value(&props, "line-class")
            .or_else(|| style_value(&props, "line-shape"))
            .and_then(|class| {
                let shape = parse_line_class(class);
                if shape.is_none() {
                    context.warn(ImportWarning::Formatting);
                }
                shape
            });
    }
    if let Some(width) = v.get("customWidth") {
        s.topic_mut(id).unwrap().style.width = import_custom_width(width, context);
    }
    if let Some(numbering) = v.get("numbering") {
        s.topic_mut(id).unwrap().child_numbering = import_numbering(numbering, context);
    }
    // Resource availability, decoding, and warnings were settled by the package
    // inventory; topics whose picture was skipped there import without it.
    if let Some((attributes, resource)) = image.and_then(|attributes| {
        let path = image_reference(&attributes.src).ok().flatten()?;
        // Opaque attachment payloads at the same path are not pictures.
        let resource = context
            .resources
            .converted_images
            .get(&path)
            .or_else(|| context.resources.original.get(&path))
            .filter(|resource| resource.media_type != "application/octet-stream")?;
        Some((attributes, resource))
    }) {
        let decoded = images::decode(resource.bytes()).map_err(IoError::Invalid)?;
        let source_size = Vec2::new(decoded.width as f32, decoded.height as f32);
        let requested = match (attributes.width, attributes.height) {
            (Some(width), Some(height)) => Vec2::new(width, height),
            (Some(width), None) => Vec2::new(width, width * source_size.y / source_size.x),
            (None, Some(height)) => Vec2::new(height * source_size.x / source_size.y, height),
            (None, None) => images::default_size(decoded.width, decoded.height),
        };
        let size = if images::valid_size(requested) {
            requested
        } else {
            context.warn(ImportWarning::Formatting);
            fit_display_size(requested)
                .unwrap_or_else(|| images::default_size(decoded.width, decoded.height))
        };
        let reference = s
            .resources
            .insert(resource.clone())
            .map_err(IoError::Invalid)?;
        // An attachment may already own these exact bytes as an opaque file; the
        // successful decode above establishes the picture type (as in `set_image`).
        if let Some(stored) = reference
            .strip_prefix(thinoda_core::resources::RESOURCE_PREFIX)
            .and_then(|stored| s.resources.entries.get_mut(stored))
            && stored.media_type == "application/octet-stream"
        {
            stored.media_type = resource.media_type.clone();
        }
        let topic = s.topic_mut(id).unwrap();
        topic.image = Some(reference);
        topic.image_size = Some(size);
        topic.image_below = attributes.below;
    }
    let attached = v
        .get("children")
        .and_then(|x| x.get("attached"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for child in attached {
        let child_role = if role == "centralTopic" {
            "mainTopic"
        } else {
            "subTopic"
        };
        parse_topic(&child, s, Some(id), depth + 1, idmap, context, child_role)?;
    }
    let children = s.topic(id).map_or(0, |topic| topic.children.len());
    let unbalanced = v
        .get("structureClass")
        .and_then(Value::as_str)
        .and_then(Structure::from_xmind_class)
        == Some(Structure::MapUnbalanced);
    if unbalanced
        && children > 0
        && let Some(right) = unbalanced_right_number(v)
    {
        let right = right.min(children);
        let kids = s.topic(id).unwrap().children.clone();
        for (index, child) in kids.into_iter().enumerate() {
            s.topic_mut(child).unwrap().branch_side = Some(if index < right {
                thinoda_core::BranchSide::Right
            } else {
                thinoda_core::BranchSide::Left
            });
        }
    }
    if let Some(boundaries) = v.get("boundaries").and_then(Value::as_array) {
        for boundary in boundaries {
            let range = match boundary.get("range") {
                None | Some(Value::Null) => None,
                Some(value)
                    if value
                        .as_str()
                        .is_some_and(|text| text.trim().eq_ignore_ascii_case("master")) =>
                {
                    None
                }
                Some(value) => match parse_range(value) {
                    Some(range) => match normalize_range(range, children, context) {
                        Some(range) => Some(range),
                        None => continue,
                    },
                    None => {
                        context.warn(ImportWarning::Other);
                        None
                    }
                },
            };
            let boundary_id = s.alloc_boundary_id();
            s.boundaries.push(Boundary {
                id: boundary_id,
                parent: id,
                range,
                title: boundary
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                color: boundary
                    .get("style")
                    .and_then(|x| x.get("properties"))
                    .and_then(|x| x.get("svg:fill").or_else(|| x.get("fill")))
                    .and_then(Value::as_str)
                    .and_then(color),
            });
        }
    }
    let mut summary_topics = Vec::new();
    if let Some(summaries) = v
        .get("children")
        .and_then(|x| x.get("summary"))
        .and_then(Value::as_array)
    {
        for summary_topic in summaries {
            let summary_id = parse_topic(
                summary_topic,
                s,
                Some(id),
                depth + 1,
                idmap,
                context,
                "summaryTopic",
            )?;
            s.topic_mut(id)
                .unwrap()
                .children
                .retain(|child| *child != summary_id);
            s.topic_mut(summary_id).unwrap().parent = None;
            summary_topics.push(summary_id);
        }
    }
    if let Some(summaries) = v.get("summaries").and_then(Value::as_array) {
        for summary in summaries {
            // Only this topic's unused summary topics can be bracket labels.
            let topic_id = summary
                .get("topicId")
                .and_then(Value::as_str)
                .and_then(|key| idmap.get(key))
                .copied()
                .filter(|topic| summary_topics.contains(topic));
            let Some(topic_id) = topic_id else {
                context.warn(ImportWarning::Other);
                continue;
            };
            let full = ChildRange {
                start: 0,
                end: children.saturating_sub(1),
            };
            let range = match summary.get("range") {
                None | Some(Value::Null) => Some(full),
                Some(value)
                    if value
                        .as_str()
                        .is_some_and(|text| text.trim().eq_ignore_ascii_case("master")) =>
                {
                    Some(full)
                }
                Some(value) => Some(parse_range(value).unwrap_or_else(|| {
                    context.warn(ImportWarning::Other);
                    full
                })),
            }
            .and_then(|range| normalize_range(range, children, context));
            let Some(range) = range else {
                continue;
            };
            summary_topics.retain(|topic| *topic != topic_id);
            let summary_id = s.alloc_summary_id();
            s.summaries.push(Summary {
                id: summary_id,
                parent: id,
                range,
                topic: topic_id,
            });
        }
    }
    // Summary topics without a usable bracket stay visible as linked floating topics.
    for topic in summary_topics {
        s.floating.push(topic);
        s.topic_mut(topic).unwrap().position = Some(Vec2::ZERO);
        context.floating_links.borrow_mut().push((id, topic, None));
        context.warn(ImportWarning::Other);
    }
    let collection = |kind: &str| {
        v.get("children")
            .and_then(|x| x.get(kind))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    if parent.is_none() {
        for d in collection("detached") {
            let p = d
                .get("position")
                .map(|p| {
                    Vec2::new(
                        p.get("x").and_then(Value::as_f64).unwrap_or(0.) as f32,
                        p.get("y").and_then(Value::as_f64).unwrap_or(0.) as f32,
                    )
                })
                .unwrap_or(Vec2::ZERO);
            let fid = parse_topic(&d, s, Some(id), depth + 1, idmap, context, "floatingTopic")?;
            s.topic_mut(id)
                .unwrap()
                .children
                .retain(|child| *child != fid);
            let floating = s.topic_mut(fid).unwrap();
            floating.parent = None;
            floating.position = Some(p);
            s.floating.push(fid);
        }
    } else {
        // Thinoda has no topic-anchored floating children; keep them visible and linked.
        for d in collection("detached") {
            parse_linked_floating(&d, s, id, depth, idmap, context, "floatingTopic")?;
        }
    }
    for callout in collection("callout") {
        parse_linked_floating(&callout, s, id, depth, idmap, context, "calloutTopic")?;
    }
    Ok(id)
}
/// Positions callouts and nested detached topics beside their owner and links them
/// with a relationship. Uses one layout pass: floating blocks are laid out at the
/// origin and translated by their position, so owner rects shift with their block.
fn place_linked_floating(s: &mut Sheet, links: &[(TopicId, TopicId, Option<Vec2>)]) {
    if links.is_empty() {
        return;
    }
    let layout = thinoda_core::layout::layout_sheet(s, &thinoda_core::layout::ApproxMeasure, false);
    let floating_roots: HashSet<TopicId> = s.floating.iter().copied().collect();
    let summary_parents: HashMap<TopicId, TopicId> = s
        .summaries
        .iter()
        .map(|summary| (summary.topic, summary.parent))
        .collect();
    let mut block_roots: HashMap<TopicId, Option<TopicId>> = HashMap::new();
    let mut block_root = |sheet: &Sheet, owner: TopicId| -> Option<TopicId> {
        *block_roots.entry(owner).or_insert_with(|| {
            let mut id = owner;
            // Nesting depth is bounded at import; the loop bound guards malformed cycles.
            for _ in 0..=sheet.topics.len() {
                if floating_roots.contains(&id) {
                    return Some(id);
                }
                id = match sheet.topic(id).and_then(|topic| topic.parent) {
                    Some(parent) => parent,
                    None => *summary_parents.get(&id)?,
                };
            }
            None
        })
    };
    // Owner rect without its block translation; hidden owners use the nearest visible ancestor.
    let owner_rect = |sheet: &Sheet, mut id: TopicId| {
        for _ in 0..=sheet.topics.len() {
            if let Some(rect) = layout.rect(id) {
                return Some(rect);
            }
            id = sheet.topic(id)?.parent?;
        }
        None
    };
    let linked: HashMap<TopicId, (TopicId, Option<Vec2>)> = links
        .iter()
        .map(|(owner, floating, offset)| (*floating, (*owner, *offset)))
        .collect();
    let mut placed: HashMap<TopicId, Vec2> = HashMap::new();
    let mut callout_index: HashMap<TopicId, usize> = HashMap::new();
    for (_, floating, _) in links {
        // Resolve owners first; chains are bounded by the number of links.
        let mut chain = vec![*floating];
        while let Some((owner, _)) = chain.last().and_then(|id| linked.get(id)) {
            match block_root(s, *owner) {
                Some(root) if linked.contains_key(&root) && !placed.contains_key(&root) => {
                    if chain.contains(&root) || chain.len() > links.len() {
                        break;
                    }
                    chain.push(root);
                }
                _ => break,
            }
        }
        for id in chain.into_iter().rev() {
            if placed.contains_key(&id) {
                continue;
            }
            let (owner, offset) = linked[&id];
            let shift = block_root(s, owner)
                .map(|root| {
                    placed
                        .get(&root)
                        .copied()
                        .or_else(|| s.topic(root).and_then(|topic| topic.position))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            let rect = owner_rect(s, owner);
            let position = match (offset, rect) {
                (Some(offset), Some(rect)) => rect.min + shift + offset,
                (Some(offset), None) => shift + offset,
                (None, Some(rect)) => {
                    let index = callout_index.entry(owner).or_default();
                    let position = Vec2::new(
                        rect.min.x + rect.size.x + 40.0,
                        rect.min.y - 40.0 - 50.0 * *index as f32,
                    ) + shift;
                    *index += 1;
                    position
                }
                (None, None) => shift,
            };
            placed.insert(id, position);
        }
    }
    for (owner, floating, _) in links {
        if let Some(topic) = s.topic_mut(*floating) {
            topic.position = placed.get(floating).copied();
        }
        let id = s.alloc_relationship_id();
        s.relationships.push(thinoda_core::Relationship {
            id,
            from: *owner,
            to: *floating,
            control_points: None,
            title: String::new(),
            color: None,
        });
    }
}

#[cfg(test)]
pub(crate) fn import(bytes: &[u8]) -> Result<Workbook, IoError> {
    let (workbook, warnings) = import_with_report(bytes)?;
    if warnings.is_empty() {
        Ok(workbook)
    } else {
        Err(IoError::ImportNeedsConfirmation(warnings))
    }
}

/// Imports an XMind workbook and reports source content that could not be kept.
pub(crate) fn import_with_report(bytes: &[u8]) -> Result<(Workbook, Vec<ImportWarning>), IoError> {
    if bytes.starts_with(b"PK\x03\x04") {
        let mut z = ZipArchive::new(Cursor::new(bytes))?;
        let names = archive_names(&mut z)?;
        if names.contains("content.json") {
            manifest_entries(&mut z, &names)?;
            let doc: Value = serde_json::from_slice(&zip_entry(&mut z, "content.json")?)?;
            let (resources, mut warnings) = package_resources(&doc, &mut z, &names)?;
            let meta = if z
                .file_names()
                .any(|n| n.is_ok_and(|v| v == "metadata.json"))
            {
                match serde_json::from_slice::<Value>(&zip_entry(&mut z, "metadata.json")?) {
                    Ok(metadata) => Some(metadata),
                    Err(e) => {
                        log::warn!("Ignoring invalid XMind metadata; reading public content: {e}");
                        None
                    }
                }
            } else {
                None
            };
            if let Some(w) = meta
                .as_ref()
                .and_then(|m| m.get("thinodaWorkbook"))
                .and_then(Value::as_str)
            {
                let protected_snapshot = strict_private_snapshot(w);
                match read_private_snapshot(w) {
                    Ok(workbook) => {
                        if let Err(e) = workbook.validate() {
                            if protected_snapshot {
                                return Err(IoError::Invalid(format!(
                                    "invalid versioned private XMind snapshot: {e}"
                                )));
                            }
                            log::warn!("Ignoring invalid private XMind snapshot: {e}");
                        } else {
                            // Files without the key predate projection 2.
                            let projection = match meta
                                .as_ref()
                                .and_then(|m| m.get("thinodaPublicProjection"))
                            {
                                None => Some(1),
                                Some(value) => value
                                    .as_u64()
                                    .filter(|version| (1..=PUBLIC_PROJECTION).contains(version)),
                            };
                            let public_matches = match projection
                                .map(|projection| public_content(&workbook, projection))
                            {
                                Some(Ok(content)) => content == doc,
                                Some(Err(e)) => {
                                    log::warn!(
                                        "Ignoring private XMind snapshot that cannot be serialized: {e}"
                                    );
                                    false
                                }
                                None => {
                                    log::warn!(
                                        "Ignoring private XMind snapshot with an unknown public projection"
                                    );
                                    false
                                }
                            };
                            let unchanged =
                                match meta.as_ref().and_then(|m| m.get("thinodaContentSha256")) {
                                    Some(hash) => {
                                        hash.as_str() == Some(content_digest(&doc)?.as_str())
                                            && public_matches
                                    }
                                    // Old Thinoda exports have no digest. Only trust them when their
                                    // generated public representation still matches the actual file.
                                    None => public_matches,
                                };
                            let resources_match = match export_resource_files(&workbook) {
                                Ok(expected) => {
                                    expected.len() == resources.original.len()
                                        && expected.iter().all(|(path, bytes)| {
                                            resources.original.get(path).is_some_and(|resource| {
                                                resource.bytes() == bytes.as_slice()
                                            })
                                        })
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Ignoring private XMind snapshot with invalid image resources: {e}"
                                    );
                                    false
                                }
                            };
                            if unchanged && resources_match && warnings.is_empty() {
                                return Ok((workbook, Vec::new()));
                            }
                            log::warn!(
                                "XMind public content or image resources changed; ignoring stale private snapshot"
                            );
                        }
                    }
                    Err(error @ IoError::NewerVersion(_)) => return Err(error),
                    Err(error) if protected_snapshot || snapshot_contains_versioned_fields(w) => {
                        return Err(error);
                    }
                    Err(e) => log::warn!("Ignoring unreadable private XMind snapshot: {e}"),
                }
            }
            let arr = doc
                .as_array()
                .ok_or_else(|| IoError::Invalid("XMind content must be an array".into()))?;
            let mut sheets = Vec::new();
            for v in arr {
                let title = v.get("title").and_then(Value::as_str).unwrap_or("Sheet");
                let root = v
                    .get("rootTopic")
                    .ok_or_else(|| IoError::Invalid("XMind sheet missing rootTopic".into()))?;
                let mut s = Sheet::new(title, "Untitled");
                let mut ids = HashMap::new();
                let context = TopicImportContext {
                    resources: &resources,
                    theme: &v["theme"],
                    warnings: Default::default(),
                    floating_links: Default::default(),
                };
                parse_topic(root, &mut s, None, 0, &mut ids, &context, "centralTopic")?;
                import_sheet_theme(&v["theme"], &mut s);
                if [
                    "zones",
                    "topicOverlapping",
                    "arrangeableLayerOrder",
                    "extensions",
                ]
                .iter()
                .any(|key| non_empty(v.get(key)))
                {
                    context.warn(ImportWarning::Other);
                }
                if let Some(rels) = v.get("relationships").and_then(Value::as_array) {
                    for r in rels {
                        let a = r
                            .get("end1Id")
                            .and_then(Value::as_str)
                            .and_then(|x| ids.get(x))
                            .copied();
                        let b = r
                            .get("end2Id")
                            .and_then(Value::as_str)
                            .and_then(|x| ids.get(x))
                            .copied();
                        if let (Some(a), Some(b)) = (a, b) {
                            let rid = s.alloc_relationship_id();
                            s.relationships.push(thinoda_core::Relationship {
                                id: rid,
                                from: a,
                                to: b,
                                control_points: import_control_points(r, &context),
                                title: r.get("title").and_then(Value::as_str).unwrap_or("").into(),
                                color: r
                                    .get("style")
                                    .and_then(|style| style.get("properties"))
                                    .and_then(|props| props.get("line-color"))
                                    .and_then(Value::as_str)
                                    .and_then(color),
                            });
                        } else {
                            context.warn(ImportWarning::Other);
                        }
                    }
                }
                place_linked_floating(&mut s, &context.floating_links.borrow());
                warnings.extend(context.warnings.into_inner());
                sheets.push(s);
            }
            if sheets.is_empty() {
                return Err(IoError::Invalid("XMind workbook has no sheets".into()));
            }
            return Ok((
                Workbook {
                    sheets,
                    active_sheet: 0,
                    metadata: Default::default(),
                    drawio: None,
                },
                warnings.into_iter().collect(),
            ));
        }
        return import_legacy_archive(&mut z, &names);
    }
    if bytes.starts_with(b"<") {
        return import_legacy_xml_with_resources(bytes, None, &HashMap::new());
    }
    Err(IoError::Invalid("not an XMind archive".into()))
}
#[derive(Default)]
struct LegacyXmlNode {
    name: String,
    attributes: BTreeMap<String, String>,
    content: Vec<LegacyXmlContent>,
}

enum LegacyXmlContent {
    Text(String),
    Element(LegacyXmlNode),
}

impl LegacyXmlNode {
    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    fn children(&self, name: &str) -> impl Iterator<Item = &Self> {
        self.content
            .iter()
            .filter_map(move |content| match content {
                LegacyXmlContent::Element(element) if element.local_name() == name => Some(element),
                _ => None,
            })
    }

    fn child(&self, name: &str) -> Option<&Self> {
        self.children(name).next()
    }

    fn local_name(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }

    fn text(&self) -> String {
        let mut output = String::new();
        self.append_text(&mut output);
        output
    }

    fn append_text(&self, output: &mut String) {
        for content in &self.content {
            match content {
                LegacyXmlContent::Text(text) => output.push_str(text),
                LegacyXmlContent::Element(element) => element.append_text(output),
            }
        }
    }
}

fn parse_legacy_xml(bytes: &[u8]) -> Result<LegacyXmlNode, IoError> {
    use quick_xml::{Reader, events::Event};
    if bytes.len() > crate::MAX_ENTRY_SIZE as usize {
        return Err(IoError::Invalid(
            "XMind XML entry exceeds the 200 MiB limit".into(),
        ));
    }
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut stack: Vec<LegacyXmlNode> = Vec::new();
    let mut root = None;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                if stack.len() >= 2000 {
                    return Err(IoError::Invalid("XMind nesting exceeds 2000 levels".into()));
                }
                stack.push(legacy_xml_node(&event)?);
            }
            Ok(Event::Empty(event)) => {
                let node = legacy_xml_node(&event)?;
                if let Some(parent) = stack.last_mut() {
                    parent.content.push(LegacyXmlContent::Element(node));
                } else if root.replace(node).is_some() {
                    return Err(IoError::Invalid("XMind XML has multiple roots".into()));
                }
            }
            Ok(Event::Text(event)) => {
                if let Some(parent) = stack.last_mut() {
                    let text = event.xml_content(quick_xml::XmlVersion::Implicit1_0);
                    parent
                        .content
                        .push(LegacyXmlContent::Text(text.into_owned()));
                }
            }
            Ok(Event::CData(event)) => {
                if let Some(parent) = stack.last_mut() {
                    parent
                        .content
                        .push(LegacyXmlContent::Text(event.as_ref().to_owned()));
                }
            }
            Ok(Event::End(_)) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| IoError::Xml("unexpected closing element".into()))?;
                if let Some(parent) = stack.last_mut() {
                    parent.content.push(LegacyXmlContent::Element(node));
                } else if root.replace(node).is_some() {
                    return Err(IoError::Invalid("XMind XML has multiple roots".into()));
                }
            }
            Ok(Event::DocType(_)) => {
                return Err(IoError::Invalid(
                    "XMind XML document types are not supported".into(),
                ));
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(IoError::Xml(error.to_string())),
            _ => {}
        }
        buffer.clear();
    }
    if !stack.is_empty() {
        return Err(IoError::Xml("unclosed XMind XML element".into()));
    }
    root.ok_or_else(|| IoError::Invalid("XMind XML has no document element".into()))
}

fn legacy_xml_node(event: &quick_xml::events::BytesStart<'_>) -> Result<LegacyXmlNode, IoError> {
    let name = event.name().as_ref().to_ascii_lowercase();
    let mut attributes = BTreeMap::new();
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| IoError::Xml(error.to_string()))?;
        let key = attribute.key.as_ref().to_ascii_lowercase();
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|error| IoError::Xml(error.to_string()))?
            .to_string();
        attributes.insert(key, value);
    }
    Ok(LegacyXmlNode {
        name,
        attributes,
        content: Vec::new(),
    })
}

type LegacyProperties = serde_json::Map<String, Value>;

#[derive(Default)]
struct LegacyStyles {
    definitions: HashMap<String, LegacyProperties>,
    roles: HashMap<String, LegacyProperties>,
}

fn legacy_descendants<'a>(
    node: &'a LegacyXmlNode,
    name: &str,
    output: &mut Vec<&'a LegacyXmlNode>,
) {
    for content in &node.content {
        if let LegacyXmlContent::Element(element) = content {
            if element.local_name() == name {
                output.push(element);
            }
            legacy_descendants(element, name, output);
        }
    }
}

fn legacy_property_map(node: &LegacyXmlNode) -> LegacyProperties {
    node.attributes
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect()
}

fn legacy_styles(styles_xml: Option<&LegacyXmlNode>, sheet: &LegacyXmlNode) -> LegacyStyles {
    let mut styles = LegacyStyles::default();
    let Some(root) = styles_xml else {
        return styles;
    };
    let mut definitions = Vec::new();
    legacy_descendants(root, "style", &mut definitions);
    for definition in definitions {
        let Some(id) = definition.attribute("id") else {
            continue;
        };
        let mut property_nodes = Vec::new();
        for name in [
            "topic-properties",
            "relationship-properties",
            "boundary-properties",
            "map-properties",
        ] {
            legacy_descendants(definition, name, &mut property_nodes);
        }
        if !property_nodes.is_empty() {
            let properties = styles.definitions.entry(id.to_owned()).or_default();
            for property_node in property_nodes {
                properties.extend(legacy_property_map(property_node));
            }
        }
    }
    let Some(theme_id) = sheet.attribute("theme") else {
        return styles;
    };
    let mut master_styles = Vec::new();
    legacy_descendants(root, "master-style", &mut master_styles);
    let Some(master) = master_styles
        .into_iter()
        .find(|style| style.attribute("id") == Some(theme_id))
    else {
        return styles;
    };
    let mut defaults = Vec::new();
    legacy_descendants(master, "default-style", &mut defaults);
    for default in defaults {
        let (Some(family), Some(style_id)) = (
            default.attribute("style-family"),
            default.attribute("style-id"),
        ) else {
            continue;
        };
        if let Some(properties) = styles.definitions.get(style_id) {
            styles.roles.insert(family.to_owned(), properties.clone());
        }
    }
    styles
}

fn legacy_apply_style(
    topic: &mut thinoda_core::TopicStyle,
    properties: &LegacyProperties,
    warnings: &mut BTreeSet<ImportWarning>,
) {
    let color_value = |key: &str| style_value(properties, key).and_then(color);
    topic.fill = color_value("svg:fill").or_else(|| color_value("background-color"));
    topic.text_color = color_value("fo:color");
    topic.border_color = color_value("border-line-color").or_else(|| color_value("svg:stroke"));
    topic.border_width = style_number(properties, "border-line-width")
        .or_else(|| style_number(properties, "svg:stroke-width"));
    topic.bold = style_value(properties, "fo:font-weight").and_then(|value| {
        let bold = parse_font_weight(value);
        if bold.is_none() {
            warnings.insert(ImportWarning::Formatting);
        }
        bold
    });
    topic.italic = style_value(properties, "fo:font-style")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "italic" | "oblique"));
    topic.font_size = style_number(properties, "fo:font-size");
    topic.font_family = style_value(properties, "fo:font-family").map(str::to_owned);
    if let Some(decoration) = style_value(properties, "fo:text-decoration") {
        let decoration = decoration.to_ascii_lowercase();
        if decoration == "none" {
            topic.underline = Some(false);
            topic.strikethrough = None;
        } else {
            topic.underline = decoration.contains("underline").then_some(true);
            topic.strikethrough = decoration.contains("line-through").then_some(true);
        }
    }
    topic.align = style_value(properties, "fo:text-align").and_then(|value| {
        match value.to_ascii_lowercase().as_str() {
            "left" => Some(TextAlign::Left),
            "center" => Some(TextAlign::Center),
            "right" => Some(TextAlign::Right),
            _ => None,
        }
    });
    topic.line_color = color_value("line-color");
    topic.line_width = style_number(properties, "line-width");
    if let Some(class) = style_value(properties, "shape-class") {
        topic.shape = parse_shape_class(class).or_else(|| {
            warnings.insert(ImportWarning::Formatting);
            approximate_shape_class(class)
        });
    }
    if let Some(class) =
        style_value(properties, "line-class").or_else(|| style_value(properties, "line-shape"))
    {
        topic.line_shape = parse_line_class(class);
        if topic.line_shape.is_none() {
            warnings.insert(ImportWarning::Formatting);
        }
    }
}

fn legacy_note_text(notes: &LegacyXmlNode) -> (Option<String>, bool, bool) {
    if let Some(plain) = notes.child("plain") {
        return (Some(plain.text().trim().to_owned()), false, false);
    }
    let mut html = notes.child("html");
    if html.is_none() {
        html = notes.child("realhtml");
    }
    let Some(html) = html else {
        let text = notes.text().trim().to_owned();
        return ((!text.is_empty()).then_some(text), true, false);
    };
    fn append(node: &LegacyXmlNode, text: &mut String, formatted: &mut bool, has_image: &mut bool) {
        for content in &node.content {
            match content {
                LegacyXmlContent::Text(value) => text.push_str(value),
                LegacyXmlContent::Element(child) => {
                    let name = child.local_name();
                    if name == "br" && !text.ends_with('\n') {
                        text.push('\n');
                    }
                    if name == "img" || name == "image" {
                        *has_image = true;
                        *formatted = true;
                    }
                    if !matches!(name, "html" | "body" | "p" | "div" | "span" | "br" | "head")
                        || child.attributes.keys().any(|key| key != "xmlns")
                    {
                        *formatted = true;
                    }
                    append(child, text, formatted, has_image);
                    if matches!(name, "p" | "div" | "li" | "tr") {
                        text.push('\n');
                    }
                }
            }
        }
    }
    let mut text = String::new();
    let mut formatted = true;
    let mut has_image = false;
    append(html, &mut text, &mut formatted, &mut has_image);
    (
        (!text.trim().is_empty()).then(|| text.trim().to_owned()),
        formatted,
        has_image,
    )
}

fn legacy_resource_path(href: &str) -> Option<String> {
    let path = href
        .strip_prefix("xap:")
        .unwrap_or(href)
        .trim_start_matches('/');
    let (root, _) = path.split_once('/')?;
    if !matches!(root, "attachments" | "resources") || !safe_manifest_path(path) {
        return None;
    }
    Some(path.to_owned())
}

struct LegacyImportContext<'a> {
    styles: &'a LegacyStyles,
    files: &'a HashMap<String, Vec<u8>>,
    pictures: &'a mut LegacyPictureRecovery,
    ids: HashMap<String, TopicId>,
    used_files: HashSet<String>,
    warnings: BTreeSet<ImportWarning>,
    floating_links: Vec<(TopicId, TopicId, Option<Vec2>)>,
    explicit_floating_positions: Vec<(TopicId, Vec2)>,
}

struct LegacyPictureRecovery {
    cache: HashMap<String, Option<Vec<u8>>>,
    reserved_bytes: usize,
    reserved_count: usize,
    sheet_count: usize,
}

impl LegacyPictureRecovery {
    fn picture(&mut self, path: &str, bytes: &[u8]) -> Option<&[u8]> {
        if !self.cache.contains_key(path) {
            let png = transcode_package_picture(path, bytes).ok().filter(|png| {
                self.reserved_count
                    .saturating_add(1)
                    .saturating_mul(self.sheet_count)
                    <= MAX_RESOURCES
                    && image_expansion_fits(self.reserved_bytes, png.len(), self.sheet_count)
            });
            if let Some(png) = &png {
                self.reserved_bytes += png.len();
                self.reserved_count += 1;
            }
            self.cache.insert(path.to_owned(), png);
        }
        self.cache.get(path)?.as_deref()
    }
}

fn legacy_normalize_range(
    range: ChildRange,
    children: usize,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Option<ChildRange> {
    if children == 0 {
        warnings.insert(ImportWarning::Other);
        return None;
    }
    let (start, end) = if range.start <= range.end {
        (range.start, range.end)
    } else {
        (range.end, range.start)
    };
    let end = end.min(children - 1);
    if range.start.max(range.end) >= children {
        warnings.insert(ImportWarning::Other);
    }
    Some(ChildRange {
        start: start.min(end),
        end,
    })
}

fn legacy_numbering(
    node: &LegacyXmlNode,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Option<Numbering> {
    let numbering = node.child("numbering")?;
    let properties = numbering.child("numbering-properties").unwrap_or(numbering);
    let format = properties
        .attribute("number-format")
        .or_else(|| properties.attribute("numberformat"))?;
    if format == "org.xmind.numbering.none" {
        return None;
    }
    let format = XMIND_NUMBER_FORMATS
        .iter()
        .find(|(id, _)| *id == format.trim())
        .map(|(_, format)| *format)
        .unwrap_or_else(|| {
            warnings.insert(ImportWarning::Formatting);
            NumberFormat::Decimal
        });
    if properties
        .attribute("number-separator")
        .is_some_and(|separator| separator.trim() != ".")
    {
        warnings.insert(ImportWarning::Formatting);
    }
    Some(Numbering {
        format,
        tiered: properties
            .attribute("prepending-numbers")
            .or_else(|| properties.attribute("prependingnumbers"))
            .is_none_or(|value| value.trim() != "none"),
        start: 1,
    })
}

fn legacy_style(node: &LegacyXmlNode, role: &str, styles: &LegacyStyles) -> LegacyProperties {
    let mut properties = styles.roles.get(role).cloned().unwrap_or_default();
    if let Some(id) = node.attribute("style-id")
        && let Some(overrides) = styles.definitions.get(id)
    {
        properties.extend(overrides.clone());
    }
    let mut inline = Vec::new();
    legacy_descendants(node, "topic-properties", &mut inline);
    if let Some(inline) = inline.first() {
        properties.extend(legacy_property_map(inline));
    }
    properties
}

fn legacy_topic_position(node: &LegacyXmlNode) -> Option<Vec2> {
    let position = node.child("position")?;
    let x = position
        .attribute("svg:x")
        .and_then(|value| parse_number(&Value::String(value.to_owned())))?;
    let y = position
        .attribute("svg:y")
        .and_then(|value| parse_number(&Value::String(value.to_owned())))?;
    (x.abs() <= 1_000_000.0 && y.abs() <= 1_000_000.0).then(|| Vec2::new(x, y))
}

fn legacy_import_topic(
    sheet: &mut Sheet,
    id: TopicId,
    node: &LegacyXmlNode,
    role: &str,
    context: &mut LegacyImportContext<'_>,
    depth: usize,
) -> Result<(), IoError> {
    if depth >= 2000 {
        return Err(IoError::Invalid("XMind nesting exceeds 2000 levels".into()));
    }
    let title = node.child("title");
    let title_text = title
        .map(LegacyXmlNode::text)
        .unwrap_or_default()
        .replace('\r', "");
    let width = title
        .and_then(|title| title.attribute("svg:width"))
        .and_then(|value| parse_number(&Value::String(value.to_owned())));
    if title.is_some_and(|title| title.attribute("svg:width").is_some()) {
        if let Some(width) = width.filter(|width| (1.0..=10_000.0).contains(width)) {
            sheet.topic_mut(id).unwrap().style.width = Some(width);
        } else {
            context.warnings.insert(ImportWarning::Formatting);
        }
    }
    if let Some(external_id) = node.attribute("id")
        && context.ids.insert(external_id.to_owned(), id).is_some()
    {
        return Err(IoError::Invalid(format!(
            "duplicate XMind topic id {external_id}"
        )));
    }
    let topic = sheet.topic_mut(id).unwrap();
    if !title_text.is_empty() || topic.title.is_empty() {
        topic.title = title_text;
    }
    topic.collapsed = node.attribute("branch") == Some("folded");
    topic.structure = node.attribute("structure-class").and_then(|class| {
        let structure = Structure::from_xmind_class(class);
        if structure.is_none() {
            context.warnings.insert(ImportWarning::Formatting);
        }
        structure
    });
    let link_href = node
        .attribute("xlink:href")
        .or_else(|| node.attribute("href"))
        .or_else(|| {
            node.child("hyperlink")
                .and_then(|link| link.attribute("xlink:href"))
        })
        .or_else(|| {
            node.child("hyperlink")
                .and_then(|link| link.attribute("href"))
        })
        .map(str::to_owned);
    if let Some(href) = &link_href {
        topic.hyperlink = Some(href.to_owned());
        if href.starts_with("xmind:#") {
            context.warnings.insert(ImportWarning::Links);
        }
    }
    if let Some(markers) = node.child("marker-refs") {
        for marker in markers.children("marker-ref") {
            if let Some(marker_id) = marker.attribute("marker-id") {
                let marker_id = canonical_marker_id(marker_id);
                if thinoda_core::markers::find(marker_id).is_none() {
                    context.warnings.insert(ImportWarning::Markers);
                }
                topic.markers.push(marker_id.to_owned());
            }
        }
    }
    if let Some(labels) = node.child("labels") {
        topic.labels = labels.children("label").map(LegacyXmlNode::text).collect();
    }
    if let Some(notes) = node.child("notes") {
        let (text, formatted, has_image) = legacy_note_text(notes);
        topic.notes = text.map(Into::into);
        if formatted {
            context.warnings.insert(ImportWarning::Notes);
        }
        if has_image {
            context.warnings.insert(ImportWarning::Resources);
        }
    }
    let properties = legacy_style(node, role, context.styles);
    if node
        .attribute("style-id")
        .is_some_and(|id| !context.styles.definitions.contains_key(id))
    {
        context.warnings.insert(ImportWarning::Formatting);
    }
    legacy_apply_style(
        &mut sheet.topic_mut(id).unwrap().style,
        &properties,
        &mut context.warnings,
    );
    sheet.topic_mut(id).unwrap().child_numbering = legacy_numbering(node, &mut context.warnings);

    if let Some(image) = node.child("image") {
        let href = image
            .attribute("xlink:href")
            .or_else(|| image.attribute("href"));
        let path = href.and_then(legacy_resource_path);
        let imported = path.as_ref().and_then(|path| {
            let bytes = context.files.get(path)?;
            let (bytes, decoded) = match images::decode(bytes) {
                Ok(decoded) => (bytes.as_slice(), decoded),
                Err(_) => {
                    context.warnings.insert(ImportWarning::Resources);
                    let png = context.pictures.picture(path, bytes)?;
                    (png, images::decode(png).ok()?)
                }
            };
            let component = path.rsplit('/').next().unwrap_or("image");
            let resource = Resource::new(component, decoded.media_type, bytes.to_vec()).ok()?;
            let reference = sheet.resources.insert(resource).ok()?;
            if let Some(stored) = reference
                .strip_prefix(thinoda_core::resources::RESOURCE_PREFIX)
                .and_then(|stored| sheet.resources.entries.get_mut(stored))
                && stored.media_type == "application/octet-stream"
            {
                stored.media_type = decoded.media_type.to_owned();
            }
            Some((path.clone(), reference, decoded))
        });
        if let Some((path, reference, decoded)) = imported {
            context.used_files.insert(path);
            let width = image
                .attribute("svg:width")
                .and_then(|value| parse_number(&Value::String(value.to_owned())));
            let height = image
                .attribute("svg:height")
                .and_then(|value| parse_number(&Value::String(value.to_owned())));
            let requested = match (width, height) {
                (Some(width), Some(height)) => Vec2::new(width, height),
                (Some(width), None) => {
                    Vec2::new(width, width * decoded.height as f32 / decoded.width as f32)
                }
                (None, Some(height)) => Vec2::new(
                    height * decoded.width as f32 / decoded.height as f32,
                    height,
                ),
                (None, None) => Vec2::new(decoded.width as f32, decoded.height as f32),
            };
            let size = if images::valid_size(requested) {
                requested
            } else {
                context.warnings.insert(ImportWarning::Formatting);
                images::default_size(decoded.width, decoded.height)
            };
            let topic = sheet.topic_mut(id).unwrap();
            topic.image = Some(reference);
            topic.image_size = Some(size);
            topic.image_below = image.attribute("align") == Some("bottom");
        } else {
            context.warnings.insert(ImportWarning::Resources);
        }
    }

    // Local file hyperlinks become opaque attachments only when the package contains
    // the referenced member; external links are never fetched.
    if let Some(href) = link_href.as_deref()
        && let Some(path) = legacy_resource_path(href)
    {
        if let Some(bytes) = context.files.get(&path) {
            let name = thinoda_core::attachments::safe_name(path.rsplit('/').next().unwrap_or(""));
            if let Ok(resource) = Resource::new(&name, "application/octet-stream", bytes.clone())
                && let Ok(reference) = sheet.resources.insert(resource)
            {
                let topic = sheet.topic_mut(id).unwrap();
                topic.hyperlink = None;
                topic.attachment = Some(thinoda_core::attachments::Attachment {
                    resource: reference,
                    name,
                });
                context.used_files.insert(path);
            } else {
                context.warnings.insert(ImportWarning::Resources);
            }
        } else {
            context.warnings.insert(ImportWarning::Resources);
        }
    }

    let children = node.child("children");
    let topic_collections: Vec<_> = children
        .into_iter()
        .flat_map(|children| children.children("topics"))
        .collect();
    let attached: Vec<_> = if topic_collections.is_empty() {
        let nested = children
            .into_iter()
            .flat_map(|children| children.children("topic"));
        nested.chain(node.children("topic")).collect()
    } else {
        topic_collections
            .iter()
            .filter(|topics| topics.attribute("type") == Some("attached"))
            .flat_map(|topics| topics.children("topic"))
            .collect()
    };
    for child in &attached {
        let child_id = sheet.add_child(
            id,
            None,
            child
                .child("title")
                .map(LegacyXmlNode::text)
                .unwrap_or_default(),
        );
        let role = if id == sheet.root {
            "mainTopic"
        } else {
            "subTopic"
        };
        legacy_import_topic(sheet, child_id, child, role, context, depth + 1)?;
    }
    if let Some(children) = children {
        for topics in children.children("topics") {
            let kind = topics.attribute("type").unwrap_or_default();
            if !matches!(kind, "attached" | "detached" | "summary")
                && topics.children("topic").next().is_some()
            {
                context.warnings.insert(ImportWarning::Other);
            }
            if kind == "detached" {
                for detached in topics.children("topic") {
                    let position = legacy_topic_position(detached);
                    let child_id = sheet.add_floating(
                        detached
                            .child("title")
                            .map(LegacyXmlNode::text)
                            .unwrap_or_default(),
                        position.unwrap_or_default(),
                    );
                    legacy_import_topic(
                        sheet,
                        child_id,
                        detached,
                        "floatingTopic",
                        context,
                        depth + 1,
                    )?;
                    context.floating_links.push((id, child_id, None));
                    if let Some(position) = position {
                        context
                            .explicit_floating_positions
                            .push((child_id, position));
                    }
                    context.warnings.insert(ImportWarning::Other);
                }
            } else if kind == "summary" {
                for summary in topics.children("topic") {
                    let summary_id = sheet.add_floating(
                        summary
                            .child("title")
                            .map(LegacyXmlNode::text)
                            .unwrap_or_default(),
                        Vec2::ZERO,
                    );
                    legacy_import_topic(
                        sheet,
                        summary_id,
                        summary,
                        "summaryTopic",
                        context,
                        depth + 1,
                    )?;
                    let range = summary
                        .attribute("range")
                        .and_then(|range| parse_range(&Value::String(range.to_owned())))
                        .and_then(|range| {
                            legacy_normalize_range(range, attached.len(), &mut context.warnings)
                        });
                    if let Some(range) = range {
                        sheet.floating.retain(|floating| *floating != summary_id);
                        sheet.topic_mut(summary_id).unwrap().position = None;
                        let summary_id_model = sheet.alloc_summary_id();
                        sheet.summaries.push(Summary {
                            id: summary_id_model,
                            parent: id,
                            range,
                            topic: summary_id,
                        });
                    } else {
                        context.warnings.insert(ImportWarning::Other);
                        context.floating_links.push((id, summary_id, None));
                    }
                }
            }
        }
    }
    if let Some(boundaries) = node.child("boundaries") {
        for boundary in boundaries.children("boundary") {
            let range = boundary
                .attribute("range")
                .filter(|value| !value.eq_ignore_ascii_case("master"))
                .and_then(|value| parse_range(&Value::String(value.to_owned())))
                .and_then(|range| {
                    legacy_normalize_range(range, attached.len(), &mut context.warnings)
                });
            let mut boundary_color = None;
            if let Some(style_id) = boundary.attribute("style-id")
                && let Some(properties) = context.styles.definitions.get(style_id)
            {
                boundary_color = style_value(properties, "svg:fill").and_then(color);
            } else if boundary.attribute("style-id").is_some() {
                context.warnings.insert(ImportWarning::Formatting);
            }
            let id_model = sheet.alloc_boundary_id();
            sheet.boundaries.push(Boundary {
                id: id_model,
                parent: id,
                range,
                title: boundary
                    .child("title")
                    .map(LegacyXmlNode::text)
                    .filter(|title| !title.is_empty()),
                color: boundary_color,
            });
        }
    }
    for ignored in ["extensions", "comments", "zones"] {
        if node.child(ignored).is_some() {
            context.warnings.insert(ImportWarning::Other);
        }
    }
    Ok(())
}

fn import_legacy_archive<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    names: &BTreeSet<String>,
) -> Result<(Workbook, Vec<ImportWarning>), IoError> {
    let content = crate::zip_entry_with_limit(archive, "content.xml", crate::MAX_ENTRY_SIZE)?;
    let styles = names
        .contains("styles.xml")
        .then(|| crate::zip_entry_with_limit(archive, "styles.xml", 4 * 1024 * 1024))
        .transpose()?;
    let resource_paths: Vec<_> = names
        .iter()
        .filter(|path| {
            !path.ends_with('/')
                && (path.starts_with("attachments/") || path.starts_with("resources/"))
        })
        .collect();
    if resource_paths.len() > MAX_RESOURCES {
        return Err(IoError::Invalid(
            "XMind archive exceeds the 128 embedded resource limit".into(),
        ));
    }
    let mut total_bytes = 0u64;
    for path in &resource_paths {
        let size = archive.by_name(path)?.size();
        if size > MAX_RESOURCE_BYTES as u64 {
            return Err(IoError::Invalid(format!(
                "XMind resource {path} exceeds the 8 MiB limit"
            )));
        }
        total_bytes = total_bytes.saturating_add(size);
    }
    if total_bytes > MAX_WORKBOOK_RESOURCE_BYTES as u64 {
        return Err(IoError::Invalid(
            "XMind resources exceed the 64 MiB workbook limit".into(),
        ));
    }
    let mut files = HashMap::new();
    for path in resource_paths {
        files.insert(
            path.clone(),
            crate::zip_entry_with_limit(archive, path, MAX_RESOURCE_BYTES as u64)?,
        );
    }
    import_legacy_xml_with_resources(&content, styles.as_deref(), &files)
}

fn import_legacy_xml_with_resources(
    bytes: &[u8],
    styles_bytes: Option<&[u8]>,
    files: &HashMap<String, Vec<u8>>,
) -> Result<(Workbook, Vec<ImportWarning>), IoError> {
    let document = parse_legacy_xml(bytes)?;
    let styles_document = styles_bytes.map(parse_legacy_xml).transpose()?;
    let sheets: Vec<_> = if document.local_name() == "sheet" {
        vec![&document]
    } else {
        document.children("sheet").collect()
    };
    let mut imported_sheets = Vec::new();
    let mut warnings = BTreeSet::new();
    let mut used_files = HashSet::new();
    let mut pictures = LegacyPictureRecovery {
        cache: HashMap::new(),
        reserved_bytes: files
            .values()
            .fold(0usize, |total, bytes| total.saturating_add(bytes.len())),
        reserved_count: files.len(),
        sheet_count: sheets.len(),
    };
    for sheet_xml in sheets {
        let roots: Vec<_> = sheet_xml.children("topic").collect();
        let root = roots
            .first()
            .copied()
            .or_else(|| {
                let mut descendants = Vec::new();
                legacy_descendants(sheet_xml, "topic", &mut descendants);
                descendants.first().copied()
            })
            .ok_or_else(|| IoError::Invalid("XMind XML sheet has no root topic".into()))?;
        let title = sheet_xml
            .child("title")
            .map(LegacyXmlNode::text)
            .or_else(|| sheet_xml.attribute("title").map(str::to_owned))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Sheet".into());
        let root_title = root
            .child("title")
            .map(LegacyXmlNode::text)
            .unwrap_or_default();
        let styles = legacy_styles(styles_document.as_ref(), sheet_xml);
        let mut sheet = Sheet::new(title, root_title);
        if let Some(properties) = styles.roles.get("map") {
            sheet.theme.background = style_value(properties, "svg:fill")
                .or_else(|| style_value(properties, "background-color"))
                .and_then(color);
            sheet.theme.line_width = style_number(properties, "line-width");
            sheet.theme.font_family = style_value(properties, "fo:font-family").map(str::to_owned);
        }
        let mut context = LegacyImportContext {
            styles: &styles,
            files,
            pictures: &mut pictures,
            ids: HashMap::new(),
            used_files: HashSet::new(),
            warnings: BTreeSet::new(),
            floating_links: Vec::new(),
            explicit_floating_positions: Vec::new(),
        };
        let root_id = sheet.root;
        legacy_import_topic(&mut sheet, root_id, root, "centralTopic", &mut context, 0)?;
        if let Some(structure) = sheet
            .topic_mut(root_id)
            .and_then(|topic| topic.structure.take())
        {
            sheet.structure = structure;
        }
        for extra in roots.into_iter().skip(1) {
            let id = sheet.add_floating(
                extra
                    .child("title")
                    .map(LegacyXmlNode::text)
                    .unwrap_or_default(),
                legacy_topic_position(extra).unwrap_or_default(),
            );
            legacy_import_topic(&mut sheet, id, extra, "floatingTopic", &mut context, 0)?;
            context.warnings.insert(ImportWarning::Other);
        }
        if let Some(relationships) = sheet_xml.child("relationships") {
            for relationship in relationships.children("relationship") {
                let end1 = relationship
                    .attribute("end1")
                    .and_then(|id| context.ids.get(id))
                    .copied();
                let end2 = relationship
                    .attribute("end2")
                    .and_then(|id| context.ids.get(id))
                    .copied();
                if let (Some(from), Some(to)) = (end1, end2) {
                    let mut relation_color = None;
                    if let Some(style_id) = relationship.attribute("style-id")
                        && let Some(properties) = styles.definitions.get(style_id)
                    {
                        relation_color = style_value(properties, "line-color").and_then(color);
                    } else if relationship.attribute("style-id").is_some() {
                        context.warnings.insert(ImportWarning::Formatting);
                    }
                    if relationship.child("control-points").is_some() {
                        context.warnings.insert(ImportWarning::Other);
                    }
                    let id = sheet.alloc_relationship_id();
                    sheet.relationships.push(thinoda_core::Relationship {
                        id,
                        from,
                        to,
                        title: relationship
                            .child("title")
                            .map(LegacyXmlNode::text)
                            .unwrap_or_default(),
                        color: relation_color,
                        control_points: None,
                    });
                } else {
                    context.warnings.insert(ImportWarning::Other);
                }
            }
        }
        place_linked_floating(&mut sheet, &context.floating_links);
        for (id, position) in &context.explicit_floating_positions {
            sheet.topic_mut(*id).unwrap().position = Some(*position);
        }
        sheet.validate().map_err(|error| {
            IoError::Invalid(format!("invalid imported XMind XML sheet: {error}"))
        })?;
        used_files.extend(context.used_files);
        warnings.extend(context.warnings);
        imported_sheets.push(sheet);
    }
    if imported_sheets.is_empty() {
        return Err(IoError::Invalid("XMind XML has no sheets".into()));
    }
    if files.keys().any(|path| !used_files.contains(path)) {
        warnings.insert(ImportWarning::Resources);
    }
    Ok((
        Workbook {
            sheets: imported_sheets,
            active_sheet: 0,
            metadata: Default::default(),
            drawio: None,
        },
        warnings.into_iter().collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    fn png(color: [u8; 3], width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            width,
            height,
            image::Rgb(color),
        ))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
        bytes.into_inner()
    }

    fn jpeg(color: [u8; 3], width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            width,
            height,
            image::Rgb(color),
        ))
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .unwrap();
        bytes.into_inner()
    }

    fn image_workbook() -> (Workbook, Vec<u8>) {
        let bytes = png([120, 20, 200], 32, 24);
        image_workbook_with(bytes, "image/png", "source.png")
    }

    fn image_workbook_with(bytes: Vec<u8>, media_type: &str, name: &str) -> (Workbook, Vec<u8>) {
        let mut sheet = Sheet::new("Evidence", "Root");
        let root = sheet.root;
        let child = sheet.add_child(root, None, "Child");
        let reference = sheet
            .resources
            .insert(Resource::new(name, media_type, bytes.clone()).unwrap())
            .unwrap();
        let root_topic = sheet.topic_mut(root).unwrap();
        root_topic.image = Some(reference.clone());
        root_topic.image_size = Some(Vec2::new(73.5, 44.25));
        root_topic.image_below = true;
        let child_topic = sheet.topic_mut(child).unwrap();
        child_topic.image = Some(reference);
        child_topic.image_below = false;
        (Workbook::new(sheet), bytes)
    }

    #[test]
    fn drawio_zone_ids_require_aware_private_snapshot() {
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        sheet
            .create_board_frame(
                "Lane",
                thinoda_core::Rect::from_min_size(
                    thinoda_core::Vec2::ZERO,
                    thinoda_core::Vec2::new(300.0, 200.0),
                ),
            )
            .unwrap();
        sheet.zones[0].drawio_source_id = Some("source-zone".into());
        book.drawio = Some(std::sync::Arc::new(
            thinoda_core::drawio_passthrough::DrawioDocumentPassthrough {
                attributes: vec![("owner".into(), "team".into())],
            },
        ));
        let snapshot = private_snapshot(&book).unwrap();
        assert!(snapshot.contains(DRAWIO_ZONE_IDS_SNAPSHOT_VERSION));
        assert!(strict_private_snapshot(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
        let raw = serde_json::to_string(&book).unwrap();
        assert!(snapshot_contains_versioned_fields(&raw));
        assert!(read_private_snapshot(&raw).is_err());
        let old = serde_json::to_string(&(DRAWIO_DOCUMENT_SNAPSHOT_VERSION, &book)).unwrap();
        assert!(read_private_snapshot(&old).is_err());
        let missing =
            serde_json::to_string(&(DRAWIO_ZONE_IDS_SNAPSHOT_VERSION, &Workbook::default()))
                .unwrap();
        assert!(read_private_snapshot(&missing).is_err());
    }

    #[test]
    fn drawio_topic_ids_require_aware_private_snapshot() {
        let mut book = Workbook::default();
        let root = book.active().root;
        book.active_mut().topic_mut(root).unwrap().drawio_source_id = Some("source-topic".into());
        book.active_mut()
            .create_board_frame(
                "Lane",
                thinoda_core::Rect::from_min_size(
                    thinoda_core::Vec2::ZERO,
                    thinoda_core::Vec2::new(300.0, 200.0),
                ),
            )
            .unwrap();
        book.active_mut().zones[0].drawio_source_id = Some("source-zone".into());
        let snapshot = private_snapshot(&book).unwrap();
        assert!(snapshot.contains(DRAWIO_TOPIC_IDS_SNAPSHOT_VERSION));
        assert!(strict_private_snapshot(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
        let raw = serde_json::to_string(&book).unwrap();
        assert!(snapshot_contains_versioned_fields(&raw));
        assert!(read_private_snapshot(&raw).is_err());
        let old = serde_json::to_string(&(DRAWIO_ZONE_IDS_SNAPSHOT_VERSION, &book)).unwrap();
        assert!(read_private_snapshot(&old).is_err());
        let missing =
            serde_json::to_string(&(DRAWIO_TOPIC_IDS_SNAPSHOT_VERSION, &Workbook::default()))
                .unwrap();
        assert!(read_private_snapshot(&missing).is_err());
    }

    #[test]
    fn drawio_group_ids_require_aware_private_snapshot() {
        use thinoda_core::{Card, CardKind};

        let mut book = Workbook::default();
        let sheet = book.active_mut();
        let first = sheet
            .add_card(
                "A",
                Vec2::ZERO,
                Card::new(CardKind::Text, Vec2::new(120.0, 60.0)),
            )
            .unwrap();
        let second = sheet
            .add_card(
                "B",
                Vec2::new(150.0, 0.0),
                Card::new(CardKind::Text, Vec2::new(120.0, 60.0)),
            )
            .unwrap();
        sheet
            .group_board_members(&[
                thinoda_core::board::BoardMember::Card(first),
                thinoda_core::board::BoardMember::Card(second),
            ])
            .unwrap();
        sheet.canvas.groups[0].drawio_source_id = Some("source-group".into());
        sheet.topic_mut(first).unwrap().drawio_source_id = Some("source-topic".into());
        let snapshot = private_snapshot(&book).unwrap();
        assert!(snapshot.contains(DRAWIO_GROUP_IDS_SNAPSHOT_VERSION));
        assert!(strict_private_snapshot(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
        let raw = serde_json::to_string(&book).unwrap();
        assert!(snapshot_contains_versioned_fields(&raw));
        assert!(read_private_snapshot(&raw).is_err());
        let old = serde_json::to_string(&(DRAWIO_TOPIC_IDS_SNAPSHOT_VERSION, &book)).unwrap();
        assert!(read_private_snapshot(&old).is_err());
        let missing =
            serde_json::to_string(&(DRAWIO_GROUP_IDS_SNAPSHOT_VERSION, &Workbook::default()))
                .unwrap();
        assert!(read_private_snapshot(&missing).is_err());
    }

    #[test]
    fn drawio_document_metadata_requires_aware_private_snapshot() {
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        let id = sheet
            .add_card(
                "Text",
                thinoda_core::Vec2::ZERO,
                thinoda_core::Card::new(
                    thinoda_core::CardKind::Sticky,
                    thinoda_core::Vec2::new(160.0, 100.0),
                ),
            )
            .unwrap();
        sheet
            .topic_mut(id)
            .unwrap()
            .inline_text
            .update_range("Text", 0..1, |style| {
                style.baseline = Some(thinoda_core::InlineBaseline::Superscript);
            })
            .unwrap();
        book.drawio = Some(std::sync::Arc::new(
            thinoda_core::drawio_passthrough::DrawioDocumentPassthrough {
                attributes: vec![("owner".into(), "team".into())],
            },
        ));
        let plain = Workbook {
            drawio: book.drawio.clone(),
            ..Workbook::default()
        };
        assert_eq!(import(&export(&plain).unwrap()).unwrap(), plain);
        let snapshot = private_snapshot(&book).unwrap();
        assert!(snapshot.contains(DRAWIO_DOCUMENT_SNAPSHOT_VERSION));
        assert!(strict_private_snapshot(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
        let raw = serde_json::to_string(&book).unwrap();
        assert!(snapshot_contains_versioned_fields(&raw));
        assert!(read_private_snapshot(&raw).is_err());
        let old = serde_json::to_string(&(CARD_BASELINE_SNAPSHOT_VERSION, &book)).unwrap();
        assert!(read_private_snapshot(&old).is_err());
        let missing =
            serde_json::to_string(&(DRAWIO_DOCUMENT_SNAPSHOT_VERSION, &Workbook::default()))
                .unwrap();
        assert!(read_private_snapshot(&missing).is_err());
    }

    #[test]
    fn card_baseline_requires_matching_private_snapshot_envelope() {
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        let id = sheet
            .add_card(
                "A🙂B",
                thinoda_core::Vec2::ZERO,
                thinoda_core::Card::new(
                    thinoda_core::CardKind::Sticky,
                    thinoda_core::Vec2::new(160.0, 100.0),
                ),
            )
            .unwrap();
        sheet
            .topic_mut(id)
            .unwrap()
            .inline_text
            .update_range("A🙂B", 1..2, |style| {
                style.baseline = Some(thinoda_core::InlineBaseline::Superscript);
            })
            .unwrap();
        let snapshot = private_snapshot(&book).unwrap();
        assert!(snapshot.contains(CARD_BASELINE_SNAPSHOT_VERSION));
        assert!(strict_private_snapshot(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        let raw = serde_json::to_string(&book).unwrap();
        assert!(snapshot_contains_versioned_fields(&raw));
        assert!(read_private_snapshot(&raw).is_err());
        assert!(
            read_private_snapshot(
                &serde_json::to_string(&(RICH_EXTRA_CONNECTOR_LABELS_SNAPSHOT_VERSION, &book))
                    .unwrap()
            )
            .is_err()
        );
        assert!(
            read_private_snapshot(
                &serde_json::to_string(&(CARD_BASELINE_SNAPSHOT_VERSION, &Workbook::default()))
                    .unwrap()
            )
            .is_err()
        );
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
        let mut connector = thinoda_core::board::CanvasConnector::new(
            thinoda_core::Endpoint::Point(thinoda_core::Vec2::ZERO),
            thinoda_core::Endpoint::Point(thinoda_core::Vec2::new(100.0, 0.0)),
        );
        let mut rich = thinoda_core::InlineText::default();
        rich.update_range("label", 0..5, |style| style.bold = Some(true))
            .unwrap();
        connector
            .extra_labels
            .push(thinoda_core::board::ConnectorLabel {
                text: "label".into(),
                rich_text: Some(std::sync::Arc::new(rich.clone())),
                ..Default::default()
            });
        let mut object = thinoda_core::board::CanvasObject::connector(connector);
        object.set_label("label").unwrap();
        object.label_rich_text = Some(std::sync::Arc::new(rich));
        book.active_mut().add_canvas_object(object).unwrap();
        assert_eq!(
            read_private_snapshot(&private_snapshot(&book).unwrap()).unwrap(),
            book
        );
    }

    #[test]
    fn stencil_definition_closures_and_hashes_use_distinct_xmind_snapshots() {
        let mut package = thinoda_core::stencil_object::find_package("thinoda.general")
            .unwrap()
            .clone();
        package.id = "vendor.xmind-offline".into();
        let definition = package.definition("folder").unwrap();
        let mut sheet = Sheet::new("Stencil", "Root");
        sheet
            .add_canvas_object(thinoda_core::stencil_object::new_object(
                &package,
                definition,
                Vec2::ZERO,
            ))
            .unwrap();
        let book = Workbook::new(sheet);

        let snapshot = private_snapshot(&book).unwrap();
        assert!(snapshot.starts_with(&format!("[\"{CLOSED_STENCIL_SNAPSHOT_VERSION}\"")));
        assert!(strict_private_snapshot(&snapshot));
        let restored = read_private_snapshot(&snapshot).unwrap();
        assert_eq!(restored, book);
        assert!(
            thinoda_core::stencil_object::render(&restored.sheets[0].canvas.objects[0]).is_some(),
            "the private XMind closure should render without an installed library"
        );

        let legacy = serde_json::to_string(&(STENCIL_SNAPSHOT_VERSION, &book)).unwrap();
        assert!(
            read_private_snapshot(&legacy)
                .unwrap_err()
                .to_string()
                .contains("pinned stencil definitions")
        );

        let mut hash_only = book.clone();
        hash_only.sheets[0].canvas.objects[0]
            .stencil
            .as_mut()
            .unwrap()
            .package_resource = None;
        hash_only.sheets[0].resources = Default::default();
        let pinned = private_snapshot(&hash_only).unwrap();
        assert!(pinned.starts_with(&format!("[\"{PINNED_STENCIL_SNAPSHOT_VERSION}\"")));
        assert_eq!(read_private_snapshot(&pinned).unwrap(), hash_only);

        let mut hashless = hash_only;
        hashless.sheets[0].canvas.objects[0]
            .stencil
            .as_mut()
            .unwrap()
            .definition_hash = None;
        let legacy = private_snapshot(&hashless).unwrap();
        assert!(legacy.starts_with(&format!("[\"{STENCIL_SNAPSHOT_VERSION}\"")));
        assert_eq!(read_private_snapshot(&legacy).unwrap(), hashless);
    }

    fn archive_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|index| {
                let mut file = archive.by_index(index).unwrap();
                let name = file.name().unwrap().into_owned();
                let mut data = Vec::new();
                file.read_to_end(&mut data).unwrap();
                (name, data)
            })
            .collect()
    }

    fn make_archive(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in entries {
            if name.ends_with('/') {
                writer
                    .add_directory(name, SimpleFileOptions::default())
                    .unwrap();
            } else {
                writer
                    .start_file(name, SimpleFileOptions::default())
                    .unwrap();
            }
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn update_content(entries: &mut [(String, Vec<u8>)], update: impl FnOnce(&mut Value)) {
        let (_, bytes) = entries
            .iter_mut()
            .find(|(name, _)| name == "content.json")
            .unwrap();
        let mut content: Value = serde_json::from_slice(bytes).unwrap();
        update(&mut content);
        *bytes = serde_json::to_vec(&content).unwrap();
    }

    fn update_manifest(entries: &mut [(String, Vec<u8>)], update: impl FnOnce(&mut Value)) {
        let (_, bytes) = entries
            .iter_mut()
            .find(|(name, _)| name == "manifest.json")
            .unwrap();
        let mut manifest: Value = serde_json::from_slice(bytes).unwrap();
        update(&mut manifest);
        *bytes = serde_json::to_vec(&manifest).unwrap();
    }

    fn image_resource_path(topic: TopicId) -> String {
        format!("resources/topic-0-{}", topic.0)
    }

    #[test]
    fn connector_markers_reject_forged_old_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint,
        };

        let mut book = Workbook::default();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let original = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&original)).unwrap(), book);
        for snapshot in [
            serde_json::to_string(&book).unwrap(),
            serde_json::to_string(&(WAYPOINT_SNAPSHOT_VERSION, &book)).unwrap(),
        ] {
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(snapshot);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(matches!(
                import(&make_archive(&entries)),
                Err(IoError::NewerVersion(_))
            ));
        }
        for snapshot in [
            private_snapshot(&book).unwrap(),
            serde_json::to_string(&book).unwrap(),
            serde_json::to_string(&(WAYPOINT_SNAPSHOT_VERSION, &book)).unwrap(),
        ] {
            for (from, to) in [
                ("\"kind\":\"open\"", "\"kind\":\"unknown-marker\""),
                ("\"arrow_end\":true", "\"arrow_end\":false"),
            ] {
                let malformed = snapshot.replace(from, to);
                assert_ne!(snapshot, malformed);
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(malformed);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(import(&make_archive(&entries)).is_err());
            }
        }
    }

    #[test]
    fn classic_and_thin_markers_reject_forged_old_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint,
        };

        for kind in [
            ConnectorMarkerKind::Classic,
            ConnectorMarkerKind::ClassicThin,
            ConnectorMarkerKind::OpenThin,
            ConnectorMarkerKind::BlockThin,
        ] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.end_marker = Some(ConnectorMarker { kind });
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);
            let mut entries = original;
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] =
                json!(serde_json::to_string(&(MARKER_SNAPSHOT_VERSION, &book)).unwrap());
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(matches!(
                import(&make_archive(&entries)),
                Err(IoError::NewerVersion(_))
            ));
        }
    }

    #[test]
    fn marker_sizes_reject_forged_old_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = Workbook::default();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        connector.end_marker_size = Some(12.5);
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let mut entries = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&entries)).unwrap(), book);
        let (_, bytes) = entries
            .iter_mut()
            .find(|(name, _)| name == "metadata.json")
            .unwrap();
        let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
        metadata["thinodaWorkbook"] =
            json!(serde_json::to_string(&(EXTENDED_MARKER_SNAPSHOT_VERSION, &book)).unwrap());
        *bytes = serde_json::to_vec(&metadata).unwrap();
        assert!(matches!(
            import(&make_archive(&entries)),
            Err(IoError::NewerVersion(_))
        ));
    }

    #[test]
    fn marker_fills_reject_forged_old_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = Workbook::default();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Classic,
        });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let mut entries = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&entries)).unwrap(), book);
        let (_, bytes) = entries
            .iter_mut()
            .find(|(name, _)| name == "metadata.json")
            .unwrap();
        let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
        metadata["thinodaWorkbook"] =
            json!(serde_json::to_string(&(SIZED_MARKER_SNAPSHOT_VERSION, &book)).unwrap());
        *bytes = serde_json::to_vec(&metadata).unwrap();
        assert!(matches!(
            import(&make_archive(&entries)),
            Err(IoError::NewerVersion(_))
        ));
    }

    #[test]
    fn diamond_markers_reject_forged_old_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        for kind in [
            ConnectorMarkerKind::Diamond,
            ConnectorMarkerKind::DiamondThin,
        ] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.end_marker = Some(ConnectorMarker { kind });
            connector.end_marker_size = Some(12.5);
            connector.end_marker_unfilled = true;
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let mut entries = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&entries)).unwrap(), book);
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] =
                json!(serde_json::to_string(&(FILLED_MARKER_SNAPSHOT_VERSION, &book)).unwrap());
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(matches!(
                import(&make_archive(&entries)),
                Err(IoError::NewerVersion(_))
            ));
        }
    }

    #[test]
    fn oval_markers_reject_forged_old_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = Workbook::default();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.arrow_start = true;
        connector.start_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Diamond,
        });
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Oval,
        });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let mut entries = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&entries)).unwrap(), book);
        let (_, bytes) = entries
            .iter_mut()
            .find(|(name, _)| name == "metadata.json")
            .unwrap();
        let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
        metadata["thinodaWorkbook"] =
            json!(serde_json::to_string(&(DIAMOND_MARKER_SNAPSHOT_VERSION, &book)).unwrap());
        *bytes = serde_json::to_vec(&metadata).unwrap();
        assert!(matches!(
            import(&make_archive(&entries)),
            Err(IoError::NewerVersion(_))
        ));
    }

    #[test]
    fn half_circle_markers_reject_forged_old_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = Workbook::default();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.arrow_start = true;
        connector.start_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Oval,
        });
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::HalfCircle,
        });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let mut entries = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&entries)).unwrap(), book);
        let (_, bytes) = entries
            .iter_mut()
            .find(|(name, _)| name == "metadata.json")
            .unwrap();
        let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
        metadata["thinodaWorkbook"] =
            json!(serde_json::to_string(&(OVAL_MARKER_SNAPSHOT_VERSION, &book)).unwrap());
        *bytes = serde_json::to_vec(&metadata).unwrap();
        assert!(matches!(
            import(&make_archive(&entries)),
            Err(IoError::NewerVersion(_))
        ));
    }

    #[test]
    fn bar_markers_reject_forged_old_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        for kind in [ConnectorMarkerKind::BaseDash, ConnectorMarkerKind::Dash] {
            for bar_at_start in [false, true] {
                let mut book = Workbook::default();
                let mut connector = CanvasConnector::new(
                    Endpoint::Point(Vec2::ZERO),
                    Endpoint::Point(Vec2::new(100.0, 0.0)),
                );
                connector.arrow_start = true;
                let bar = Some(ConnectorMarker { kind });
                let half_circle = Some(ConnectorMarker {
                    kind: ConnectorMarkerKind::HalfCircle,
                });
                (connector.start_marker, connector.end_marker) = if bar_at_start {
                    (bar, half_circle)
                } else {
                    (half_circle, bar)
                };
                connector.start_marker_size = Some(0.0);
                connector.end_marker_size = Some(12.5);
                connector.start_marker_unfilled = true;
                connector.end_marker_unfilled = true;
                book.active_mut()
                    .add_canvas_object(CanvasObject::connector(connector))
                    .unwrap();
                let mut entries = archive_entries(&export(&book).unwrap());
                assert_eq!(import(&make_archive(&entries)).unwrap(), book);
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                let snapshot: (String, Value) =
                    serde_json::from_str(metadata["thinodaWorkbook"].as_str().unwrap()).unwrap();
                assert_eq!(snapshot.0, BAR_MARKER_SNAPSHOT_VERSION);
                metadata["thinodaWorkbook"] = json!(
                    serde_json::to_string(&(HALF_CIRCLE_MARKER_SNAPSHOT_VERSION, &book)).unwrap()
                );
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(matches!(
                    import(&make_archive(&entries)),
                    Err(IoError::NewerVersion(_))
                ));
            }
        }
    }

    #[test]
    fn rich_labels_require_matching_private_snapshot_envelope() {
        use thinoda_core::board::{CanvasObject, CanvasObjectKind, ShapeKind};
        use thinoda_core::{Rect, Vec2};

        for label_style in [false, true] {
            let mut book = Workbook::default();
            let mut object = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 30.0)),
            );
            let CanvasObjectKind::Shape { label, .. } = &mut object.kind else {
                unreachable!()
            };
            *label = "A🙂B".into();
            let mut rich = thinoda_core::InlineText::default();
            rich.update_range(label, 1..2, |style| style.bold = Some(true))
                .unwrap();
            object.label_rich_text = Some(std::sync::Arc::new(rich));
            if label_style {
                object.label_style =
                    Some(std::sync::Arc::new(thinoda_core::label_style::LabelStyle {
                        bold: true,
                        ..Default::default()
                    }));
            }
            book.active_mut().add_canvas_object(object).unwrap();
            let snapshot = private_snapshot(&book).unwrap();
            assert!(snapshot.contains(RICH_LABELS_SNAPSHOT_VERSION));
            assert!(strict_private_snapshot(&snapshot));
            assert!(snapshot_contains_versioned_fields(&snapshot));
            assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
            for forged in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(LABEL_STYLES_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(RICH_LABELS_SNAPSHOT_VERSION, &Workbook::default()))
                    .unwrap(),
            ] {
                assert!(read_private_snapshot(&forged).is_err());
            }
        }
    }

    #[test]
    fn rich_connector_labels_require_matching_private_snapshot_envelope() {
        use thinoda_core::board::{CanvasConnector, CanvasObject};
        use thinoda_core::{Endpoint, Vec2};

        let mut book = Workbook::default();
        let mut object = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        ));
        object.set_label("A🙂B").unwrap();
        let mut rich = thinoda_core::InlineText::default();
        rich.update_range(object.label(), 1..2, |style| style.bold = Some(true))
            .unwrap();
        object.label_rich_text = Some(std::sync::Arc::new(rich));
        book.active_mut().add_canvas_object(object).unwrap();
        let snapshot = private_snapshot(&book).unwrap();
        assert!(snapshot.contains(RICH_CONNECTOR_LABELS_SNAPSHOT_VERSION));
        assert!(strict_private_snapshot(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        for forged in [
            serde_json::to_string(&book).unwrap(),
            serde_json::to_string(&(RICH_LABELS_SNAPSHOT_VERSION, &book)).unwrap(),
            serde_json::to_string(&(RICH_CONNECTOR_LABELS_SNAPSHOT_VERSION, &Workbook::default()))
                .unwrap(),
        ] {
            assert!(read_private_snapshot(&forged).is_err());
        }
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
    }

    #[test]
    fn rich_extra_connector_labels_require_matching_private_snapshot_envelope() {
        use std::sync::Arc;
        use thinoda_core::board::{CanvasConnector, CanvasObject, ConnectorLabel, ShapeKind};
        use thinoda_core::{Endpoint, Vec2};

        for primary in [false, true] {
            for (label_style, shape_rich) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                let mut book = Workbook::default();
                let mut connector = CanvasConnector::new(
                    Endpoint::Point(Vec2::ZERO),
                    Endpoint::Point(Vec2::new(100.0, 0.0)),
                );
                let mut rich = thinoda_core::InlineText::default();
                rich.update_range("A🙂B", 1..2, |style| style.bold = Some(true))
                    .unwrap();
                connector.extra_labels.push(ConnectorLabel {
                    text: "A🙂B".into(),
                    rich_text: Some(Arc::new(rich.clone())),
                    ..Default::default()
                });
                let mut object = CanvasObject::connector(connector);
                if primary {
                    object.set_label("A🙂B").unwrap();
                    object.label_rich_text = Some(Arc::new(rich));
                }
                book.active_mut().add_canvas_object(object).unwrap();
                if label_style || shape_rich {
                    let mut shape = CanvasObject::shape(
                        ShapeKind::Rectangle,
                        thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 30.0)),
                    );
                    if label_style {
                        shape.label_style = Some(Arc::new(thinoda_core::label_style::LabelStyle {
                            bold: true,
                            ..Default::default()
                        }));
                    }
                    if shape_rich {
                        shape.set_label("shape").unwrap();
                        let mut rich = thinoda_core::InlineText::default();
                        rich.update_range("shape", 0..5, |style| style.italic = Some(true))
                            .unwrap();
                        shape.label_rich_text = Some(Arc::new(rich));
                    }
                    book.active_mut().add_canvas_object(shape).unwrap();
                }
                let snapshot = private_snapshot(&book).unwrap();
                assert!(snapshot.contains(RICH_EXTRA_CONNECTOR_LABELS_SNAPSHOT_VERSION));
                assert!(strict_private_snapshot(&snapshot));
                assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
                for forged in [
                    serde_json::to_string(&book).unwrap(),
                    serde_json::to_string(&(RICH_CONNECTOR_LABELS_SNAPSHOT_VERSION, &book))
                        .unwrap(),
                    serde_json::to_string(&(
                        RICH_EXTRA_CONNECTOR_LABELS_SNAPSHOT_VERSION,
                        &Workbook::default(),
                    ))
                    .unwrap(),
                ] {
                    assert!(read_private_snapshot(&forged).is_err());
                }
                assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
            }
        }
    }

    #[test]
    fn label_styles_require_matching_private_snapshot_envelope() {
        use thinoda_core::Vec2;
        use thinoda_core::board::{CanvasObject, ShapeKind};
        use thinoda_core::label_style::LabelStyle;

        for shape_style in [false, true] {
            let mut book = Workbook::default();
            let sheet = book.active_mut();
            let mut object = CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 30.0)),
            );
            object.label_style = Some(std::sync::Arc::new(LabelStyle {
                bold: true,
                ..LabelStyle::default()
            }));
            object.style.shadow = shape_style;
            sheet.add_canvas_object(object).unwrap();
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);

            let plain = Workbook::default();
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(SHAPE_STYLES_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(COLLAPSE_LAYOUT_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(LABEL_STYLES_SNAPSHOT_VERSION, &plain)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(
                    matches!(
                        import(&make_archive(&entries)),
                        Err(IoError::NewerVersion(_))
                    ),
                    "{snapshot}"
                );
            }
        }
    }

    #[test]
    fn shape_styles_require_matching_private_snapshot_envelope() {
        use thinoda_core::Vec2;
        use thinoda_core::board::{CanvasObject, ShapeKind};

        for table in [false, true] {
            let mut book = Workbook::default();
            let sheet = book.active_mut();
            let mut object = CanvasObject::shape(
                ShapeKind::Triangle,
                thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 30.0)),
            );
            object.style.flip_h = true;
            sheet.add_canvas_object(object).unwrap();
            if table {
                sheet
                    .insert_canvas_table(Vec2::new(100.0, 0.0), 2, 2, None)
                    .unwrap();
            }
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);

            let plain = Workbook::default();
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(TABLES_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(COLLAPSE_LAYOUT_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(SHAPE_STYLES_SNAPSHOT_VERSION, &plain)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(
                    matches!(
                        import(&make_archive(&entries)),
                        Err(IoError::NewerVersion(_))
                    ),
                    "{snapshot}"
                );
            }
        }
    }

    #[test]
    fn tables_require_matching_private_snapshot_envelope() {
        use thinoda_core::Vec2;

        for layered in [false, true] {
            let mut book = Workbook::default();
            let sheet = book.active_mut();
            let table = sheet.insert_canvas_table(Vec2::ZERO, 2, 2, None).unwrap();
            if layered {
                let layer = sheet.add_board_layer("Notes").unwrap();
                sheet
                    .move_canvas_objects_to_layer(&[table], Some(layer))
                    .unwrap();
            }
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);

            let plain = Workbook::default();
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(LAYERS_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(COLLAPSE_LAYOUT_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(TABLES_SNAPSHOT_VERSION, &plain)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(
                    matches!(
                        import(&make_archive(&entries)),
                        Err(IoError::NewerVersion(_))
                    ),
                    "{snapshot}"
                );
            }
        }
    }

    #[test]
    fn layers_require_matching_private_snapshot_envelope() {
        use std::sync::Arc;
        use thinoda_core::object_data::ObjectData;
        use thinoda_core::{CanvasObject, Rect, ShapeKind, Vec2};

        for data in [false, true] {
            let mut book = Workbook::default();
            let sheet = book.active_mut();
            let mut object = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 20.0)),
            );
            if data {
                object.data = Some(Arc::new(ObjectData {
                    tags: vec!["flow".into()],
                    ..Default::default()
                }));
            }
            let id = sheet.add_canvas_object(object).unwrap();
            let layer = sheet.add_board_layer("Notes").unwrap();
            sheet
                .move_canvas_objects_to_layer(&[id], Some(layer))
                .unwrap();
            sheet.set_board_layer_locked(layer, true).unwrap();
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);

            let plain = Workbook::default();
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(OBJECT_DATA_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(LAYERS_SNAPSHOT_VERSION, &plain)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(
                    matches!(
                        import(&make_archive(&entries)),
                        Err(IoError::NewerVersion(_))
                    ),
                    "{snapshot}"
                );
            }
        }
    }

    #[test]
    fn object_data_requires_matching_private_snapshot_envelope() {
        use std::sync::Arc;
        use thinoda_core::object_data::ObjectData;
        use thinoda_core::{CanvasConnector, CanvasObject, ConnectorLabel, Endpoint, Vec2};

        for labels in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            if labels {
                connector.extra_labels.push(ConnectorLabel {
                    text: "n".into(),
                    placement: Default::default(),
                    rich_text: None,
                });
            }
            let mut object = CanvasObject::connector(connector);
            object.data = Some(Arc::new(ObjectData {
                tags: vec!["flow".into()],
                ..Default::default()
            }));
            book.active_mut().add_canvas_object(object).unwrap();
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);

            let plain = Workbook::default();
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(CONNECTOR_LABELS_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(OBJECT_DATA_SNAPSHOT_VERSION, &plain)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(
                    matches!(
                        import(&make_archive(&entries)),
                        Err(IoError::NewerVersion(_))
                    ),
                    "{snapshot}"
                );
            }
        }
    }

    #[test]
    fn connector_labels_require_matching_private_snapshot_envelope() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorLabel, ConnectorLabelPlacement, EdgeShape,
            EdgeShapeKind, Endpoint, Vec2,
        };

        for edge_shape in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            connector.extra_labels.push(ConnectorLabel {
                text: "0..1".into(),
                placement: ConnectorLabelPlacement {
                    along: 1.0,
                    orthogonal: 6.0,
                    offset: Vec2::ZERO,
                },
                rich_text: None,
            });
            if edge_shape {
                connector.edge_shape = Some(EdgeShape::new(EdgeShapeKind::Link));
            }
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);

            let plain = Workbook::default();
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(EDGE_SHAPES_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(CONNECTOR_LABELS_SNAPSHOT_VERSION, &plain)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(
                    matches!(
                        import(&make_archive(&entries)),
                        Err(IoError::NewerVersion(_))
                    ),
                    "{snapshot}"
                );
            }
        }
    }

    #[test]
    fn edge_shapes_require_matching_private_snapshot_envelope() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorRoute, EdgeShape, EdgeShapeKind, Endpoint, Vec2,
        };

        for segment_route in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            connector.edge_shape = Some(EdgeShape::new(EdgeShapeKind::Link));
            if segment_route {
                connector.route = ConnectorRoute::Segment;
            }
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);

            let plain = Workbook::default();
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(SEGMENT_ROUTES_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(EDGE_SHAPES_SNAPSHOT_VERSION, &plain)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(
                    matches!(
                        import(&make_archive(&entries)),
                        Err(IoError::NewerVersion(_))
                    ),
                    "{snapshot}"
                );
            }
        }
    }

    #[test]
    fn segment_routes_require_matching_private_snapshot_envelope() {
        use thinoda_core::{CanvasConnector, CanvasObject, ConnectorRoute, Endpoint, Vec2};

        for edge_route in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            connector.route = ConnectorRoute::IsometricHorizontal;
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            if edge_route {
                let mut other = CanvasConnector::new(
                    Endpoint::Point(Vec2::ZERO),
                    Endpoint::Point(Vec2::new(0.0, 80.0)),
                );
                other.route = ConnectorRoute::TopToBottom;
                book.active_mut()
                    .add_canvas_object(CanvasObject::connector(other))
                    .unwrap();
            }
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);

            let plain = Workbook::default();
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(EDGE_ROUTES_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(SEGMENT_ROUTES_SNAPSHOT_VERSION, &plain)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(
                    matches!(
                        import(&make_archive(&entries)),
                        Err(IoError::NewerVersion(_))
                    ),
                    "{snapshot}"
                );
            }
        }
    }

    #[test]
    fn edge_routes_require_matching_private_snapshot_envelope() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorRoute, Endpoint, LineJumpStyle, LineJumps, Vec2,
        };

        for jumps in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            connector.route = ConnectorRoute::SideToSide;
            if jumps {
                connector.line_jumps = Some(LineJumps::new(LineJumpStyle::Arc));
            }
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);

            let plain = Workbook::default();
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(LINE_JUMPS_SNAPSHOT_VERSION, &book)).unwrap(),
                serde_json::to_string(&(EDGE_ROUTES_SNAPSHOT_VERSION, &plain)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(
                    matches!(
                        import(&make_archive(&entries)),
                        Err(IoError::NewerVersion(_))
                    ),
                    "{snapshot}"
                );
            }
        }
    }

    #[test]
    fn line_jumps_require_matching_private_snapshot_envelope() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint,
            LineJumpStyle, LineJumps, Vec2,
        };

        let mut book = Workbook::default();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.line_jumps = Some(LineJumps::new(LineJumpStyle::Arc));
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::ErOne,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let original = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&original)).unwrap(), book);

        let plain = Workbook::default();
        for snapshot in [
            serde_json::to_string(&book).unwrap(),
            serde_json::to_string(&(MARKER_LIBRARY_SNAPSHOT_VERSION, &book)).unwrap(),
            serde_json::to_string(&(LINE_JUMPS_SNAPSHOT_VERSION, &plain)).unwrap(),
        ] {
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(snapshot);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(
                matches!(
                    import(&make_archive(&entries)),
                    Err(IoError::NewerVersion(_))
                ),
                "{snapshot}"
            );
        }
    }

    #[test]
    fn library_markers_require_matching_private_snapshot_envelope() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = Workbook::default();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::ErZeroToMany,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let original = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&original)).unwrap(), book);

        let plain = Workbook::default();
        for snapshot in [
            serde_json::to_string(&book).unwrap(),
            serde_json::to_string(&(SEMANTIC_SNAPSHOT_VERSION, &book)).unwrap(),
            serde_json::to_string(&(COLLAPSE_LAYOUT_SNAPSHOT_VERSION, &book)).unwrap(),
            serde_json::to_string(&(MARKER_LIBRARY_SNAPSHOT_VERSION, &plain)).unwrap(),
        ] {
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(snapshot);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(
                matches!(
                    import(&make_archive(&entries)),
                    Err(IoError::NewerVersion(_))
                ),
                "{snapshot}"
            );
        }
    }

    #[test]
    fn shape_rounding_rejects_old_and_malformed_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind,
            DrawioRectangleStencil, Endpoint, Rect, ShapeKind, ShapeRounding, Vec2,
        };

        let mut book = Workbook::default();
        let mut shape = CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        shape.rounding = Some(ShapeRounding {
            radius: 17.0,
            stencil: DrawioRectangleStencil::Forms,
            ..Default::default()
        });
        book.active_mut().add_canvas_object(shape).unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Dash,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let original = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&original)).unwrap(), book);

        for snapshot in [
            serde_json::to_string(&book).unwrap(),
            serde_json::to_string(&(BAR_MARKER_SNAPSHOT_VERSION, &book)).unwrap(),
        ] {
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(snapshot);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(matches!(
                import(&make_archive(&entries)),
                Err(IoError::NewerVersion(_))
            ));
        }
        for version in [ROUNDING_SNAPSHOT_VERSION, BAR_MARKER_SNAPSHOT_VERSION] {
            let mut malformed = serde_json::to_value(&book).unwrap();
            malformed["sheets"][0]["canvas"]["objects"][0]["rounding"]["radius"] = json!("invalid");
            let snapshot = serde_json::to_string(&(version, malformed)).unwrap();
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(snapshot);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(import(&make_archive(&entries)).is_err(), "{version}");
        }
    }

    #[test]
    fn final_states_reject_old_private_snapshots() {
        use thinoda_core::{
            CanvasObject, DrawioRectangleStencil, Rect, ShapeKind, ShapeRounding,
            ShapeRoundingMode, Vec2,
        };

        let mut book = Workbook::default();
        let mut rounded = CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        rounded.rounding = Some(ShapeRounding {
            radius: 17.5,
            stencil: DrawioRectangleStencil::Standard,
            mode: ShapeRoundingMode::AbsoluteArc,
            rounded_perimeter: true,
        });
        book.active_mut().add_canvas_object(rounded).unwrap();
        book.active_mut()
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::FinalState,
                Rect::from_min_size(Vec2::new(120.0, 0.0), Vec2::new(30.0, 30.0)),
            ))
            .unwrap();
        let snapshot = private_snapshot(&book).unwrap();
        assert!(strict_private_snapshot(&snapshot));
        assert!(snapshot_contains_versioned_fields(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        for version in [
            WAYPOINT_SNAPSHOT_VERSION,
            ROUNDING_SNAPSHOT_VERSION,
            RECTANGLE_SIDES_SNAPSHOT_VERSION,
            DOUBLE_ELLIPSE_SNAPSHOT_VERSION,
            FIXED_PORT_SNAPSHOT_VERSION,
            DRAWIO_ROUNDING_SNAPSHOT_VERSION,
        ] {
            let forged = serde_json::to_string(&(version, &book)).unwrap();
            assert!(read_private_snapshot(&forged).is_err(), "{version}");
        }
        assert!(read_private_snapshot(&serde_json::to_string(&book).unwrap()).is_err());
    }

    #[test]
    fn scene_hierarchy_rejects_old_or_unversioned_private_snapshots() {
        use thinoda_core::{CanvasObject, Rect, ShapeKind, Vec2};
        let mut book = Workbook::default();
        let pool = book
            .active_mut()
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 100.0)),
            ))
            .unwrap();
        book.active_mut().set_canvas_container(pool, true).unwrap();
        let snapshot = private_snapshot(&book).unwrap();
        assert!(strict_private_snapshot(&snapshot));
        assert!(snapshot_contains_versioned_fields(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        let original = archive_entries(&export(&book).unwrap());
        for source in [
            serde_json::to_string(&(STENCIL_SNAPSHOT_VERSION, &book)).unwrap(),
            serde_json::to_string(&book).unwrap(),
        ] {
            assert!(snapshot_contains_versioned_fields(&source));
            assert!(matches!(
                read_private_snapshot(&source),
                Err(IoError::NewerVersion(_))
            ));
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(source);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(matches!(
                import(&make_archive(&entries)),
                Err(IoError::NewerVersion(_))
            ));
        }
    }

    #[test]
    fn socio_technical_data_requires_aware_private_snapshot_envelopes() {
        let mut book = Workbook::default();
        let mut semantic = thinoda_core::semantic::SemanticModel::default();
        semantic.entities.insert(
            thinoda_core::semantic::SemanticEntityId(2),
            thinoda_core::semantic::SemanticEntity::Participant(
                thinoda_core::semantic::Participant {
                    kind: thinoda_core::semantic::ParticipantKind::Human,
                    name: "Case worker".into(),
                    description: String::new(),
                },
            ),
        );
        book.active_mut().semantic = Some(semantic);

        let snapshot = private_snapshot(&book).unwrap();
        assert!(strict_private_snapshot(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        let older = serde_json::to_string(&(COLLAPSE_LAYOUT_SNAPSHOT_VERSION, &book)).unwrap();
        assert!(read_private_snapshot(&older).is_err());
        assert!(read_private_snapshot(&serde_json::to_string(&book).unwrap()).is_err());
    }

    #[test]
    fn scene_layout_policies_require_layout_aware_private_snapshots() {
        use thinoda_core::{
            CanvasObject, Rect, ShapeKind, Vec2,
            scene_layout::{
                ContainerLayout, ContainerLayoutPolicy, HeaderPolicy, HeaderSide, StackAxis,
                StackCrossAxisPolicy, StackResizePolicy,
            },
            stencil_object::{find_package, new_object},
        };

        for (collapsed, with_stencil, expected_version, old_version) in [
            (
                false,
                false,
                SCENE_LAYOUT_SNAPSHOT_VERSION,
                SCENE_SNAPSHOT_VERSION,
            ),
            (
                false,
                true,
                SCENE_STENCIL_LAYOUT_SNAPSHOT_VERSION,
                SCENE_STENCIL_SNAPSHOT_VERSION,
            ),
            (
                true,
                true,
                COLLAPSE_LAYOUT_SNAPSHOT_VERSION,
                COLLAPSE_SNAPSHOT_VERSION,
            ),
        ] {
            let mut book = Workbook::default();
            let pool = book
                .active_mut()
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 100.0)),
                ))
                .unwrap();
            book.active_mut().set_canvas_container(pool, true).unwrap();
            book.active_mut()
                .set_canvas_container_layout_policy(
                    pool,
                    Some(ContainerLayoutPolicy {
                        header: HeaderPolicy {
                            side: HeaderSide::Top,
                            extent: 24.0,
                        },
                        layout: ContainerLayout::Stack {
                            axis: StackAxis::Vertical,
                            spacing: 8.0,
                            padding: 8.0,
                            cross_axis: StackCrossAxisPolicy::Fill,
                            resize: StackResizePolicy::Preserve,
                        },
                    }),
                )
                .unwrap();
            if with_stencil {
                let package = find_package("thinoda.general").unwrap();
                book.active_mut()
                    .add_canvas_object(new_object(
                        package,
                        package.definition("message").unwrap(),
                        Vec2::new(250.0, 0.0),
                    ))
                    .unwrap();
            }
            if collapsed {
                book.active_mut()
                    .set_canvas_container_collapsed(
                        pool,
                        Some(Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 30.0))),
                    )
                    .unwrap();
            }

            let snapshot = private_snapshot(&book).unwrap();
            assert!(snapshot.starts_with(&format!("[\"{expected_version}\"")));
            assert!(strict_private_snapshot(&snapshot));
            assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
            assert_eq!(import(&export(&book).unwrap()).unwrap(), book);

            let old = serde_json::to_string(&(old_version, &book)).unwrap();
            assert!(matches!(
                read_private_snapshot(&old),
                Err(IoError::NewerVersion(message)) if message.contains("layout policies")
            ));
        }
    }

    #[test]
    fn drawio_rounding_rejects_old_private_snapshots() {
        use thinoda_core::{
            CanvasObject, DrawioRectangleStencil, Rect, ShapeKind, ShapeRounding,
            ShapeRoundingMode, Vec2,
        };

        let mut book = Workbook::default();
        let mut shape = CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        shape.rounding = Some(ShapeRounding {
            radius: 17.5,
            stencil: DrawioRectangleStencil::Standard,
            mode: ShapeRoundingMode::AbsoluteArc,
            rounded_perimeter: true,
        });
        book.active_mut().add_canvas_object(shape).unwrap();
        let snapshot = private_snapshot(&book).unwrap();
        assert!(strict_private_snapshot(&snapshot));
        assert_eq!(read_private_snapshot(&snapshot).unwrap(), book);
        for version in [
            WAYPOINT_SNAPSHOT_VERSION,
            ROUNDING_SNAPSHOT_VERSION,
            RECTANGLE_SIDES_SNAPSHOT_VERSION,
            DOUBLE_ELLIPSE_SNAPSHOT_VERSION,
            FIXED_PORT_SNAPSHOT_VERSION,
        ] {
            let forged = serde_json::to_string(&(version, &book)).unwrap();
            assert!(read_private_snapshot(&forged).is_err(), "{version}");
        }
        assert!(read_private_snapshot(&serde_json::to_string(&book).unwrap()).is_err());
    }

    #[test]
    fn rectangle_sides_reject_old_and_malformed_private_snapshots() {
        use thinoda_core::{CanvasObject, Rect, RectangleSides, ShapeKind, Vec2};

        let mut book = Workbook::default();
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        shape.rectangle_sides = Some(RectangleSides {
            top: true,
            right: false,
            bottom: false,
            left: true,
        });
        book.active_mut().add_canvas_object(shape).unwrap();
        let original = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&original)).unwrap(), book);

        for snapshot in [
            serde_json::to_string(&book).unwrap(),
            serde_json::to_string(&(ROUNDING_SNAPSHOT_VERSION, &book)).unwrap(),
        ] {
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(snapshot);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(matches!(
                import(&make_archive(&entries)),
                Err(IoError::NewerVersion(_))
            ));
        }
        for version in [RECTANGLE_SIDES_SNAPSHOT_VERSION, ROUNDING_SNAPSHOT_VERSION] {
            let mut malformed = serde_json::to_value(&book).unwrap();
            malformed["sheets"][0]["canvas"]["objects"][0]["rectangle_sides"]["top"] =
                json!("invalid");
            let snapshot = serde_json::to_string(&(version, malformed)).unwrap();
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(snapshot);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(import(&make_archive(&entries)).is_err(), "{version}");
        }
    }

    #[test]
    fn fixed_connector_ports_reject_old_and_malformed_private_snapshots() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorPort, Endpoint, Rect, ShapeKind, Vec2,
        };

        let mut book = Workbook::default();
        let shape = book
            .active_mut()
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
            ))
            .unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Point(Vec2::new(200.0, 0.0)),
        );
        connector.from_port = Some(ConnectorPort {
            position: Vec2::new(1.0, 0.25),
            offset: Vec2::ZERO,
            perimeter: true,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let original = archive_entries(&export(&book).unwrap());
        assert_eq!(import(&make_archive(&original)).unwrap(), book);
        for snapshot in [
            serde_json::to_string(&book).unwrap(),
            serde_json::to_string(&(DOUBLE_ELLIPSE_SNAPSHOT_VERSION, &book)).unwrap(),
        ] {
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(snapshot);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(matches!(
                import(&make_archive(&entries)),
                Err(IoError::NewerVersion(_))
            ));
        }
        for version in [FIXED_PORT_SNAPSHOT_VERSION, DOUBLE_ELLIPSE_SNAPSHOT_VERSION] {
            let mut malformed = serde_json::to_value(&book).unwrap();
            malformed["sheets"][0]["canvas"]["objects"][1]["kind"]["connector"]["from_port"]["position"]
                ["x"] = json!("invalid");
            let snapshot = serde_json::to_string(&(version, malformed)).unwrap();
            let mut entries = original.clone();
            let (_, bytes) = entries
                .iter_mut()
                .find(|(name, _)| name == "metadata.json")
                .unwrap();
            let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
            metadata["thinodaWorkbook"] = json!(snapshot);
            *bytes = serde_json::to_vec(&metadata).unwrap();
            assert!(import(&make_archive(&entries)).is_err(), "{version}");
        }
    }

    #[test]
    fn double_ellipses_reject_old_and_malformed_private_snapshots() {
        use thinoda_core::{
            CanvasObject, DrawioRectangleStencil, Rect, RectangleSides, ShapeKind, ShapeRounding,
            Vec2,
        };

        for margin in [None, Some(8.5)] {
            let mut book = Workbook::default();
            let mut ellipse = CanvasObject::shape(
                ShapeKind::DoubleEllipse,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
            );
            ellipse.double_ellipse_margin = margin;
            book.active_mut().add_canvas_object(ellipse).unwrap();
            let mut rectangle = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(120.0, 0.0), Vec2::new(100.0, 60.0)),
            );
            rectangle.rectangle_sides = Some(RectangleSides::default());
            book.active_mut().add_canvas_object(rectangle).unwrap();
            let mut rounded = CanvasObject::shape(
                ShapeKind::RoundedRectangle,
                Rect::from_min_size(Vec2::new(240.0, 0.0), Vec2::new(100.0, 60.0)),
            );
            rounded.rounding = Some(ShapeRounding {
                radius: 15.0,
                stencil: DrawioRectangleStencil::Forms,
                ..Default::default()
            });
            book.active_mut().add_canvas_object(rounded).unwrap();
            let original = archive_entries(&export(&book).unwrap());
            assert_eq!(import(&make_archive(&original)).unwrap(), book);
            for snapshot in [
                serde_json::to_string(&book).unwrap(),
                serde_json::to_string(&(RECTANGLE_SIDES_SNAPSHOT_VERSION, &book)).unwrap(),
            ] {
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(matches!(
                    import(&make_archive(&entries)),
                    Err(IoError::NewerVersion(_))
                ));
            }
            for version in [
                DOUBLE_ELLIPSE_SNAPSHOT_VERSION,
                RECTANGLE_SIDES_SNAPSHOT_VERSION,
            ] {
                let mut malformed = serde_json::to_value(&book).unwrap();
                malformed["sheets"][0]["canvas"]["objects"][0]["double_ellipse_margin"] =
                    json!("invalid");
                let snapshot = serde_json::to_string(&(version, malformed)).unwrap();
                let mut entries = original.clone();
                let (_, bytes) = entries
                    .iter_mut()
                    .find(|(name, _)| name == "metadata.json")
                    .unwrap();
                let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
                metadata["thinodaWorkbook"] = json!(snapshot);
                *bytes = serde_json::to_vec(&metadata).unwrap();
                assert!(import(&make_archive(&entries)).is_err(), "{version}");
            }
        }
    }

    #[test]
    fn externally_edited_xmind_content_supersedes_stale_marker_snapshot() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint,
        };

        let mut book = Workbook::default();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Block,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let mut entries = archive_entries(&export(&book).unwrap());
        let (_, bytes) = entries
            .iter_mut()
            .find(|(name, _)| name == "content.json")
            .unwrap();
        let mut content: Value = serde_json::from_slice(bytes).unwrap();
        content[0]["rootTopic"]["title"] = json!("Edited externally");
        *bytes = serde_json::to_vec(&content).unwrap();
        let reopened = import(&make_archive(&entries)).unwrap();
        assert_eq!(reopened.active().root_topic().title, "Edited externally");
        assert!(reopened.active().canvas.is_empty());
    }

    #[test]
    fn stable_shape_class_mappings_are_explicit() {
        assert_eq!(
            shape_class(TopicShape::RoundedRect),
            "org.xmind.topicShape.roundedRect"
        );
        assert_eq!(
            shape_class(TopicShape::Underline),
            "org.xmind.topicShape.underline"
        );
        assert_eq!(
            parse_shape_class("org.xmind.topicShape.ellipseRect"),
            Some(TopicShape::Ellipse)
        );
        for shape in TopicShape::ALL {
            assert_eq!(parse_shape_class(shape_class(shape)), Some(shape));
        }
        assert_eq!(
            line_class(LineShape::RoundedElbow),
            "org.xmind.branchConnection.roundedElbow"
        );
        assert_eq!(
            parse_line_class("Rounded elbow"),
            Some(LineShape::RoundedElbow)
        );
        for shape in LineShape::ALL {
            assert_eq!(parse_line_class(line_class(shape)), Some(shape));
        }
    }

    #[test]
    fn legacy_xml_export_preserves_topic_style_and_content() {
        let mut w = Workbook::new(Sheet::new("S", "Root & title"));
        w.active_mut().theme.font_family = Some("monospace".into());
        w.active_mut().theme.line_width = Some(3.5);
        let root = w.active().root;
        let child = w.active_mut().add_child(root, None, "Child");
        w.active_mut().topic_mut(child).unwrap().style = thinoda_core::TopicStyle {
            fill: Some(Color::rgb(10, 20, 30)),
            shape: Some(TopicShape::Diamond),
            line_shape: Some(LineShape::RoundedElbow),
            font_family: Some("Noto Sans".into()),
            font_size: Some(14.0),
            line_width: Some(4.0),
            ..Default::default()
        };
        let bytes = export(&w).unwrap();
        let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let xml = crate::zip_entry(&mut z, "content.xml").unwrap();
        let xml = String::from_utf8(xml).unwrap();
        assert!(xml.contains("Root &amp; title"));
        assert!(xml.contains("Child"));
        assert!(xml.contains("shape-class=\"org.xmind.topicShape.diamond\""));
        let (imported, warnings) =
            import_legacy_xml_with_resources(xml.as_bytes(), None, &HashMap::new()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(imported.active().root_topic().style.line_width, Some(3.5));
        assert_eq!(
            imported.active().root_topic().style.font_family.as_deref(),
            Some("monospace")
        );
        let imported_child = imported
            .active()
            .topic(imported.active().root_topic().children[0])
            .unwrap();
        assert_eq!(imported_child.style.fill, Some(Color::rgb(10, 20, 30)));
        assert_eq!(imported_child.style.line_width, Some(4.0));
        assert_eq!(imported_child.style.shape, Some(TopicShape::Diamond));
        assert_eq!(
            imported_child.style.line_shape,
            Some(LineShape::RoundedElbow)
        );
        assert_eq!(
            imported_child.style.font_family.as_deref(),
            Some("Noto Sans")
        );
    }

    #[test]
    fn legacy_xmind_archive_imports_sheets_styles_annotations_and_resources() {
        let content = br#"<?xml version="1.0"?>
            <xmap-content xmlns:xlink="http://www.w3.org/1999/xlink"
                          xmlns:svg="http://www.w3.org/2000/svg">
              <sheet theme="theme">
                <title>First</title>
                <topic id="root" structure-class="org.xmind.ui.logic.right"
                       style-id="central">
                  <title svg:width="120pt">Root</title>
                  <numbering><numbering-properties
                    number-format="org.xmind.numbering.roman"/></numbering>
                  <children><topics type="attached">
                    <topic id="a" xlink:href="xap:attachments/evidence.bin">
                      <title>One</title>
                      <image xlink:href="xap:resources/picture.png" svg:width="40pt"/>
                      <notes><html><p>Line one</p><p>Line two</p></html></notes>
                    </topic>
                    <topic id="b"><title>Two</title></topic>
                  </topics>
                  <topics type="summary">
                    <topic id="summary" range="(0, 1)">
                      <title>Summary</title>
                    </topic>
                  </topics>
                  <topics type="detached"><topic id="floating">
                    <title>Floating</title>
                    <position svg:x="100pt" svg:y="-30pt"/>
                  </topic></topics>
                  </children>
                  <boundaries><boundary range="(0, 1)" style-id="boundary">
                    <title>Group</title>
                  </boundary></boundaries>
                </topic>
                <relationships><relationship end1="a" end2="b">
                  <title>Related</title>
                </relationship></relationships>
              </sheet>
              <sheet><title>Second</title><topic id="root-2"><title>Other map</title></topic></sheet>
            </xmap-content>"#;
        let styles = br##"<xmap-styles xmlns:svg="http://www.w3.org/2000/svg"><styles>
            <style id="central"><topic-properties svg:fill="#112233"/></style>
            <style id="boundary"><boundary-properties svg:fill="#445566"/></style>
            <style id="map"><map-properties svg:fill="#778899"/></style>
          </styles><master-styles><master-style id="theme">
            <default-style style-family="centralTopic" style-id="central"/>
            <default-style style-family="map" style-id="map"/>
          </master-style></master-styles></xmap-styles>"##;
        let picture = png([10, 20, 30], 2, 1);
        let entries = vec![
            ("content.xml".into(), content.to_vec()),
            ("styles.xml".into(), styles.to_vec()),
            ("resources/picture.png".into(), picture),
            ("attachments/evidence.bin".into(), vec![1, 2, 3]),
            ("resources/unused.bin".into(), vec![4, 5]),
        ];
        let (workbook, warnings) = import_with_report(&make_archive(&entries)).unwrap();

        assert_eq!(workbook.sheets.len(), 2);
        let sheet = workbook.active();
        assert_eq!(sheet.title, "First");
        assert_eq!(sheet.structure, Structure::LogicRight);
        assert_eq!(sheet.theme.background, Some(Color::rgb(119, 136, 153)));
        assert_eq!(
            sheet.topic(sheet.root).unwrap().style.fill,
            Some(Color::rgb(17, 34, 51))
        );
        assert_eq!(sheet.topic(sheet.root).unwrap().style.width, Some(120.0));
        assert_eq!(
            sheet.topic(sheet.root).unwrap().child_numbering,
            Some(Numbering {
                format: NumberFormat::UpperRoman,
                tiered: true,
                start: 1,
            })
        );
        assert_eq!(sheet.boundaries.len(), 1);
        assert_eq!(
            sheet.boundaries[0].range,
            Some(ChildRange { start: 0, end: 1 })
        );
        assert_eq!(sheet.boundaries[0].color, Some(Color::rgb(68, 85, 102)));
        assert_eq!(sheet.summaries.len(), 1);
        assert_eq!(
            sheet
                .relationships
                .iter()
                .filter(|relationship| relationship.title == "Related")
                .count(),
            1
        );
        assert_eq!(sheet.floating.len(), 1);
        assert_eq!(
            sheet.topic(sheet.floating[0]).unwrap().position,
            Some(Vec2::new(100.0, -30.0))
        );
        let image_topic = sheet.topic(sheet.root_topic().children[0]).unwrap();
        assert_eq!(image_topic.image_size, Some(Vec2::new(40.0, 20.0)));
        assert_eq!(image_topic.notes.as_deref(), Some("Line one\nLine two"));
        let attachment = image_topic.attachment.as_ref().unwrap();
        assert_eq!(attachment.name, "evidence.bin");
        assert_eq!(
            sheet.resources.get(&attachment.resource).unwrap().bytes(),
            [1, 2, 3]
        );
        assert_eq!(
            warnings,
            vec![
                ImportWarning::Notes,
                ImportWarning::Resources,
                ImportWarning::Other
            ]
        );
    }

    #[test]
    fn sdk_pattern_exports_manifest_public_images_and_private_exact_roundtrip() {
        let (book, bytes) = image_workbook();
        let exported = export(&book).unwrap();
        let entries = archive_entries(&exported);
        let manifest: Value = serde_json::from_slice(
            &entries
                .iter()
                .find(|(name, _)| name == "manifest.json")
                .unwrap()
                .1,
        )
        .unwrap();
        let root_path = image_resource_path(book.active().root);
        let child = book.active().root_topic().children[0];
        let child_path = image_resource_path(child);
        for path in ["content.json", "metadata.json", &root_path, &child_path] {
            assert!(manifest["file-entries"].get(path).is_some());
        }
        assert_eq!(
            entries
                .iter()
                .find(|(name, _)| name == &root_path)
                .unwrap()
                .1,
            bytes
        );
        assert_eq!(
            entries
                .iter()
                .find(|(name, _)| name == &child_path)
                .unwrap()
                .1,
            bytes
        );
        let content: Value = serde_json::from_slice(
            &entries
                .iter()
                .find(|(name, _)| name == "content.json")
                .unwrap()
                .1,
        )
        .unwrap();
        let public_root = &content[0]["rootTopic"];
        assert_eq!(
            public_root["image"],
            json!({
                "src": format!("xap:{root_path}"),
                "width": 73.5,
                "height": 44.25,
                "align": "bottom"
            })
        );
        assert_eq!(
            public_root["children"]["attached"][0]["image"]["align"],
            "top"
        );
        assert_eq!(import(&exported).unwrap(), book);

        // Without Thinoda's private metadata, the public SDK-shaped package still
        // imports the verified image data and placement.
        let mut public_entries = entries;
        let metadata = public_entries
            .iter_mut()
            .find(|(name, _)| name == "metadata.json")
            .unwrap();
        metadata.1 = b"{}".to_vec();
        public_entries.push(("resources/".into(), Vec::new()));
        let imported = import(&make_archive(&public_entries)).unwrap();
        let imported_root = imported.active().root_topic();
        let root_image = imported
            .active()
            .resources
            .get(imported_root.image.as_deref().unwrap())
            .unwrap();
        assert_eq!(root_image.bytes(), bytes);
        assert_eq!(imported_root.image_size, Some(Vec2::new(73.5, 44.25)));
        assert!(imported_root.image_below);
        let child_image = imported.active().topic(imported_root.children[0]).unwrap();
        assert_eq!(child_image.image_size, Some(Vec2::new(32.0, 24.0)));
        assert!(!child_image.image_below);

        let mut sdk_defaults = public_entries.clone();
        update_content(&mut sdk_defaults, |content| {
            let image = content[0]["rootTopic"]["image"].as_object_mut().unwrap();
            image.remove("width");
            image.remove("height");
            image.remove("align");
        });
        let imported_defaults = import(&make_archive(&sdk_defaults)).unwrap();
        let default_image = imported_defaults.active().root_topic();
        assert_eq!(default_image.image_size, Some(Vec2::new(32.0, 24.0)));
        assert!(!default_image.image_below);

        let mut width_only = public_entries.clone();
        update_content(&mut width_only, |content| {
            content[0]["rootTopic"]["image"]
                .as_object_mut()
                .unwrap()
                .remove("height");
        });
        let imported_width_only = import(&make_archive(&width_only)).unwrap();
        let sized_image = imported_width_only.active().root_topic();
        assert_eq!(sized_image.image_size, Some(Vec2::new(73.5, 55.125)));
        assert!(sized_image.image_below);

        let mut height_only = public_entries.clone();
        update_content(&mut height_only, |content| {
            content[0]["rootTopic"]["image"]
                .as_object_mut()
                .unwrap()
                .remove("width");
        });
        let imported_height_only = import(&make_archive(&height_only)).unwrap();
        let sized_image = imported_height_only.active().root_topic();
        assert_eq!(sized_image.image_size, Some(Vec2::new(59.0, 44.25)));
        assert!(sized_image.image_below);

        let jpeg_bytes = jpeg([20, 80, 190], 28, 16);
        let (jpeg_book, _) = image_workbook_with(jpeg_bytes, "image/jpeg", "source.jpg");
        assert_eq!(import(&export(&jpeg_book).unwrap()).unwrap(), jpeg_book);
    }

    #[test]
    fn image_resource_fuzz_seeds_import_and_round_trip() {
        let seeds = [
            (
                include_bytes!("../../../fuzz/corpus/xmind/package-image-default-size.xmind")
                    .as_slice(),
                Vec2::new(32.0, 24.0),
                false,
            ),
            (
                include_bytes!("../../../fuzz/corpus/xmind/package-image-width-only.xmind")
                    .as_slice(),
                Vec2::new(73.5, 55.125),
                true,
            ),
        ];

        for (seed, size, below) in seeds {
            let workbook = import(seed).unwrap();
            let topic = workbook.active().root_topic();
            assert_eq!(topic.image_size, Some(size));
            assert_eq!(topic.image_below, below);
            assert_eq!(import(&export(&workbook).unwrap()).unwrap(), workbook);
        }
    }

    #[test]
    fn desktop_sample_packages_import_the_same_map() {
        let without_thumbnail = include_bytes!("../tests/fixtures/sample-map-no-thumbnail.xmind");
        let with_thumbnail = include_bytes!("../tests/fixtures/sample-map-with-thumbnail.xmind");
        let (expected, warnings) = import_with_report(without_thumbnail).unwrap();
        // Bold title runs and sheet layout settings are not preserved.
        assert_eq!(
            warnings,
            vec![ImportWarning::Formatting, ImportWarning::Other]
        );
        assert_eq!(expected.sheets.len(), 1);
        assert_eq!(expected.active().root_topic().title, "Central Topic");
        assert_eq!(
            import_with_report(with_thumbnail).unwrap(),
            (expected.clone(), warnings)
        );
        assert!(matches!(
            import(without_thumbnail),
            Err(IoError::ImportNeedsConfirmation(_))
        ));
        assert_eq!(import(&export(&expected).unwrap()).unwrap(), expected);
    }

    #[test]
    fn ancillary_package_entries_do_not_change_imported_content() {
        let (book, _) = image_workbook();
        let mut entries = archive_entries(&export(&book).unwrap());
        // Ancillary payloads are not interpreted, even if they look like map data.
        let paths = [
            "Thumbnails/thumbnail.png",
            "Thumbnails/preview@2x.png",
            "META-INF/vendor/future.json",
        ];
        update_manifest(&mut entries, |manifest| {
            for path in paths {
                manifest["file-entries"][path] = json!({});
            }
        });
        for path in paths {
            entries.push((path.into(), b"not an image or JSON".to_vec()));
        }
        entries.push(("Thumbnails/".into(), Vec::new()));
        entries.push(("unlisted-vendor-data.bin".into(), vec![0xff]));
        entries
            .iter_mut()
            .find(|(name, _)| name == "content.xml")
            .unwrap()
            .1 = b"not XML".to_vec();
        assert_eq!(import(&make_archive(&entries)).unwrap(), book);
    }

    #[test]
    fn ancillary_entries_keep_package_validation_and_resource_limits() {
        let (book, _) = image_workbook();
        let original = archive_entries(&export(&book).unwrap());
        for path in [
            "../preview.png",
            "/preview.png",
            "Thumbnails/../preview.png",
            "Thumbnails\\preview.png",
            "C:preview.png",
            "Thumbnails//preview.png",
            "Thumbnails/\npreview.png",
        ] {
            let mut entries = original.clone();
            update_manifest(&mut entries, |manifest| {
                manifest["file-entries"][path] = json!({});
            });
            assert!(import(&make_archive(&entries)).is_err(), "{path}");

            let mut entries = original.clone();
            entries.push((path.into(), Vec::new()));
            assert!(import(&make_archive(&entries)).is_err(), "{path}");
        }
        let mut missing = original.clone();
        update_manifest(&mut missing, |manifest| {
            manifest["file-entries"]["Thumbnails/missing.png"] = json!({});
        });
        assert!(
            import(&make_archive(&missing))
                .unwrap_err()
                .to_string()
                .contains("missing from archive")
        );
        let mut invalid_metadata = original.clone();
        update_manifest(&mut invalid_metadata, |manifest| {
            manifest["file-entries"]["Thumbnails/thumbnail.png"] = json!(null);
        });
        invalid_metadata.push(("Thumbnails/thumbnail.png".into(), vec![0xff]));
        assert!(
            import(&make_archive(&invalid_metadata))
                .unwrap_err()
                .to_string()
                .contains("unsafe or invalid XMind manifest path")
        );

        for declared in [false, true] {
            let mut entries = original.clone();
            let path = "resources/unused.png";
            if declared {
                update_manifest(&mut entries, |manifest| {
                    manifest["file-entries"][path] = json!({});
                });
            }
            entries.push((path.into(), vec![0xff]));
            assert!(import(&make_archive(&entries)).is_err());
        }

        let mut at_limit = original;
        for index in at_limit.len()..MAX_XMIND_ARCHIVE_ENTRIES {
            at_limit.push((format!("vendor/entry-{index}"), Vec::new()));
        }
        assert_eq!(import(&make_archive(&at_limit)).unwrap(), book);
        at_limit.push(("vendor/one-too-many".into(), Vec::new()));
        assert!(
            import(&make_archive(&at_limit))
                .unwrap_err()
                .to_string()
                .contains("too many entries")
        );
    }

    #[test]
    fn desktop_ancillary_entries_do_not_reduce_image_allowance() {
        let mut file_entries = json!({
            "content.json": {},
            "Thumbnails/thumbnail.png": {},
        });
        let mut entries = vec![
            ("Thumbnails/".into(), Vec::new()),
            ("Thumbnails/thumbnail.png".into(), vec![0xff]),
            ("vendor/future.json".into(), Vec::new()),
            ("content.xml".into(), b"not XML".to_vec()),
        ];
        let image = png([120, 20, 200], 32, 24);
        let children: Vec<_> = (0..MAX_RESOURCES)
            .map(|index| {
                let path = format!("resources/image-{index}.png");
                file_entries[&path] = json!({});
                entries.push((path, image.clone()));
                json!({
                    "id": format!("child-{index}"),
                    "title": format!("Image {index}"),
                    "image": {"src": format!("xap:resources/image-{index}.png")},
                })
            })
            .collect();
        let content = json!([{
            "rootTopic": {
                "id": "root",
                "title": "Root",
                "children": {"attached": children},
            },
        }]);
        entries.push(("content.json".into(), serde_json::to_vec(&content).unwrap()));
        entries.push((
            "manifest.json".into(),
            serde_json::to_vec(&json!({"file-entries": file_entries})).unwrap(),
        ));
        let imported = import(&make_archive(&entries)).unwrap();
        assert_eq!(imported.active().root_topic().children.len(), MAX_RESOURCES);
        for id in &imported.active().root_topic().children {
            assert!(imported.active().topic(*id).unwrap().image.is_some());
        }
    }

    fn report(entries: &[(String, Vec<u8>)]) -> (Workbook, Vec<ImportWarning>) {
        import_with_report(&make_archive(entries)).unwrap()
    }

    #[test]
    fn package_resource_validation_rejects_unsafe_data_and_reports_unusable_pictures() {
        let (book, _) = image_workbook();
        let original = archive_entries(&export(&book).unwrap());

        // External or non-package pictures are never fetched; the topic keeps its text.
        for src in ["https://example.invalid/image.png", "resources/image.png"] {
            let mut entries = original.clone();
            update_content(&mut entries, |content| {
                content[0]["rootTopic"]["image"]["src"] = json!(src);
            });
            let (workbook, warnings) = report(&entries);
            assert_eq!(warnings, vec![ImportWarning::Resources], "{src}");
            assert_eq!(workbook.active().root_topic().image, None, "{src}");
        }
        for src in [
            "xap:resources/",
            "xap:resources/../escape",
            "xap:resources/a/b",
            "xap:resources/a\\b",
            "xap:resources/C:evil",
        ] {
            let mut entries = original.clone();
            update_content(&mut entries, |content| {
                content[0]["rootTopic"]["image"]["src"] = json!(src);
            });
            assert!(
                import_with_report(&make_archive(&entries)).is_err(),
                "{src}"
            );
        }

        for align in ["left", "center"] {
            let mut entries = original.clone();
            update_content(&mut entries, |content| {
                content[0]["rootTopic"]["image"]["align"] = json!(align);
            });
            let (workbook, warnings) = report(&entries);
            assert_eq!(warnings, vec![ImportWarning::Formatting], "{align}");
            assert!(!workbook.active().root_topic().image_below, "{align}");
        }

        // The manifest is optional: the archive itself is the inventory.
        let mut missing_manifest = original.clone();
        missing_manifest.retain(|(name, _)| name != "manifest.json");
        let (workbook, warnings) = report(&missing_manifest);
        assert!(warnings.is_empty());
        assert!(workbook.active().root_topic().image.is_some());

        let mut undeclared = original.clone();
        update_content(&mut undeclared, |content| {
            content[0]["rootTopic"]["image"]["src"] = json!("xap:resources/unknown");
        });
        let (workbook, warnings) = report(&undeclared);
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        assert_eq!(workbook.active().root_topic().image, None);

        let mut unsafe_manifest = original.clone();
        update_manifest(&mut unsafe_manifest, |manifest| {
            let entries = manifest["file-entries"].as_object_mut().unwrap();
            let old = image_resource_path(book.active().root);
            let value = entries.remove(&old).unwrap();
            entries.insert("resources/../escape".into(), value);
        });
        assert!(import_with_report(&make_archive(&unsafe_manifest)).is_err());

        let mut missing = original.clone();
        missing.retain(|(name, _)| name != &image_resource_path(book.active().root));
        assert!(
            import_with_report(&make_archive(&missing))
                .unwrap_err()
                .to_string()
                .contains("missing from archive")
        );

        let mut unsupported = original.clone();
        let path = image_resource_path(book.active().root);
        unsupported
            .iter_mut()
            .find(|(name, _)| name == &path)
            .unwrap()
            .1 = b"GIF89a".to_vec();
        let (workbook, warnings) = report(&unsupported);
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        assert_eq!(workbook.active().root_topic().image, None);
        assert_eq!(workbook.active().resources.entries.len(), 1);

        let mut mismatched_media_type = book.clone();
        mismatched_media_type
            .active_mut()
            .resources
            .entries
            .values_mut()
            .next()
            .unwrap()
            .media_type = "image/jpeg".into();
        assert!(
            export(&mismatched_media_type)
                .unwrap_err()
                .to_string()
                .contains("media type")
        );

        let image_bytes = original
            .iter()
            .find(|(name, _)| name.starts_with("resources/"))
            .unwrap()
            .1
            .clone();
        let mut too_many = original.clone();
        too_many.retain(|(name, _)| !name.starts_with("resources/"));
        let paths: Vec<_> = (0..=MAX_RESOURCES)
            .map(|index| format!("resources/limit-{index}"))
            .collect();
        update_manifest(&mut too_many, |manifest| {
            let entries = manifest["file-entries"].as_object_mut().unwrap();
            entries.retain(|name, _| !name.starts_with("resources/"));
            for path in &paths {
                entries.insert(path.clone(), json!({}));
            }
        });
        too_many.extend(paths.into_iter().map(|path| (path, image_bytes.clone())));
        let error = import(&make_archive(&too_many)).unwrap_err().to_string();
        assert!(error.contains("128 embedded image limit"), "{error}");
    }

    #[test]
    fn static_webp_picture_is_recovered_without_overwriting_its_source() {
        let pixels = image::RgbaImage::from_pixel(4, 3, image::Rgba([30, 80, 190, 128]));
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels.clone())
            .write_to(&mut encoded, image::ImageFormat::WebP)
            .unwrap();
        let webp = encoded.into_inner();
        let archive_for = |attachment: bool| {
            let mut root = json!({
                "title": "Root",
                "image": {"src": "xap:resources/picture.webp"}
            });
            if attachment {
                root["href"] = json!("xap:resources/picture.webp");
            }
            make_archive(&[
                (
                    "content.json".into(),
                    serde_json::to_vec(&json!([{"rootTopic": root}])).unwrap(),
                ),
                ("resources/picture.webp".into(), webp.clone()),
            ])
        };
        let archive = archive_for(false);
        let (book, warnings) = import_with_report(&archive).unwrap();
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        let sheet = book.active();
        let resource = sheet
            .resources
            .get(sheet.root_topic().image.as_ref().unwrap())
            .unwrap();
        assert_eq!(resource.media_type, "image/png");
        assert_eq!(
            image::load_from_memory(resource.bytes())
                .unwrap()
                .to_rgba8(),
            pixels
        );
        assert!(matches!(
            import(&archive),
            Err(IoError::ImportNeedsConfirmation(warnings))
                if warnings.contains(&ImportWarning::Resources)
        ));
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);

        let (book, warnings) = import_with_report(&archive_for(true)).unwrap();
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        let sheet = book.active();
        let image = sheet
            .resources
            .get(sheet.root_topic().image.as_ref().unwrap())
            .unwrap();
        assert_eq!(image.media_type, "image/png");
        assert_eq!(
            image::load_from_memory(image.bytes()).unwrap().to_rgba8(),
            pixels
        );
        let attachment = sheet.root_topic().attachment.as_ref().unwrap();
        assert_eq!(
            sheet.resources.get(&attachment.resource).unwrap().bytes(),
            webp
        );
        book.validate().unwrap();
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
    }

    #[test]
    fn shared_webp_preserves_attachment_when_an_extra_resource_would_exceed_the_limit() {
        let mut webp = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([1, 2, 3, 255]),
        ))
        .write_to(&mut webp, image::ImageFormat::WebP)
        .unwrap();
        let webp = webp.into_inner();
        let children: Vec<_> = (0..MAX_RESOURCES - 1)
            .map(|i| json!({"title": format!("File {i}"), "href": format!("xap:resources/file-{i}")}))
            .collect();
        let mut entries = vec![
            (
                "content.json".into(),
                serde_json::to_vec(&json!([{"rootTopic": {
                    "title": "Root",
                    "href": "xap:resources/picture.webp",
                    "image": {"src": "xap:resources/picture.webp"},
                    "children": {"attached": children}
                }}]))
                .unwrap(),
            ),
            ("resources/picture.webp".into(), webp.clone()),
        ];
        entries.extend((0..MAX_RESOURCES - 1).map(|i| {
            (
                format!("resources/file-{i}"),
                format!("file-{i}").into_bytes(),
            )
        }));
        let (book, warnings) = import_with_report(&make_archive(&entries)).unwrap();
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        let sheet = book.active();
        assert!(sheet.root_topic().image.is_none());
        let attachment = sheet.root_topic().attachment.as_ref().unwrap();
        assert_eq!(
            sheet.resources.get(&attachment.resource).unwrap().bytes(),
            webp
        );
        book.validate().unwrap();
    }

    #[test]
    fn legacy_webp_picture_and_attachment_are_recovered_separately() {
        let pixels = image::RgbaImage::from_pixel(4, 3, image::Rgba([30, 80, 190, 128]));
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels.clone())
            .write_to(&mut encoded, image::ImageFormat::WebP)
            .unwrap();
        let webp = encoded.into_inner();
        let content = br#"<xmap-content><sheet><title>Sheet</title>
            <topic id="root" xlink:href="xap:resources/picture.webp">
                <title>Root</title><image xlink:href="xap:resources/picture.webp"/>
            </topic></sheet></xmap-content>"#;
        let archive = make_archive(&[
            ("content.xml".into(), content.to_vec()),
            ("resources/picture.webp".into(), webp.clone()),
        ]);
        let (book, warnings) = import_with_report(&archive).unwrap();
        assert!(warnings.contains(&ImportWarning::Resources));
        let sheet = book.active();
        let image = sheet
            .resources
            .get(sheet.root_topic().image.as_ref().unwrap())
            .unwrap();
        assert_eq!(image.media_type, "image/png");
        assert_eq!(
            image::load_from_memory(image.bytes()).unwrap().to_rgba8(),
            pixels
        );
        let attachment = sheet.root_topic().attachment.as_ref().unwrap();
        assert_eq!(
            sheet.resources.get(&attachment.resource).unwrap().bytes(),
            webp
        );
        assert!(matches!(
            import(&archive),
            Err(IoError::ImportNeedsConfirmation(_))
        ));
        book.validate().unwrap();
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
    }

    #[test]
    fn legacy_webp_recovery_keeps_attachments_when_resource_limit_is_full() {
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([1, 2, 3, 255]),
        ))
        .write_to(&mut encoded, image::ImageFormat::WebP)
        .unwrap();
        let webp = encoded.into_inner();
        let content = br#"<xmap-content><sheet><topic
            xlink:href="xap:resources/picture.webp"><title>Root</title>
            <image xlink:href="xap:resources/picture.webp"/>
            </topic></sheet></xmap-content>"#;
        let mut entries = vec![
            ("content.xml".into(), content.to_vec()),
            ("resources/picture.webp".into(), webp.clone()),
        ];
        entries.extend((0..MAX_RESOURCES - 1).map(|i| (format!("resources/unused-{i}"), vec![1])));
        let (book, warnings) = import_with_report(&make_archive(&entries)).unwrap();
        assert!(warnings.contains(&ImportWarning::Resources));
        let sheet = book.active();
        assert!(sheet.root_topic().image.is_none());
        let attachment = sheet.root_topic().attachment.as_ref().unwrap();
        assert_eq!(
            sheet.resources.get(&attachment.resource).unwrap().bytes(),
            webp
        );
        book.validate().unwrap();
    }

    #[test]
    fn static_gif_pictures_are_recovered_without_flattening_animation() {
        let pixels = image::RgbaImage::from_pixel(4, 3, image::Rgba([30, 80, 190, 255]));
        for animated in [false, true] {
            let mut gif = Vec::new();
            {
                let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif);
                encoder
                    .encode_frame(image::Frame::new(pixels.clone()))
                    .unwrap();
                if animated {
                    encoder
                        .encode_frame(image::Frame::new(pixels.clone()))
                        .unwrap();
                }
            }
            for legacy in [false, true] {
                let content = if legacy {
                    (
                        "content.xml",
                        br#"<xmap-content><sheet><topic
                            xlink:href="xap:resources/picture.gif"><title>Root</title>
                            <image xlink:href="xap:resources/picture.gif"/>
                            </topic></sheet></xmap-content>"#
                            .to_vec(),
                    )
                } else {
                    (
                        "content.json",
                        serde_json::to_vec(&json!([{"rootTopic": {
                            "title": "Root",
                            "href": "xap:resources/picture.gif",
                            "image": {"src": "xap:resources/picture.gif"}
                        }}]))
                        .unwrap(),
                    )
                };
                let archive = make_archive(&[
                    (content.0.into(), content.1),
                    ("resources/picture.gif".into(), gif.clone()),
                ]);
                let (book, warnings) = import_with_report(&archive).unwrap();
                assert!(warnings.contains(&ImportWarning::Resources));
                let sheet = book.active();
                let attachment = sheet.root_topic().attachment.as_ref().unwrap();
                assert_eq!(
                    sheet.resources.get(&attachment.resource).unwrap().bytes(),
                    gif
                );
                if animated {
                    assert!(sheet.root_topic().image.is_none());
                } else {
                    let image = sheet
                        .resources
                        .get(sheet.root_topic().image.as_ref().unwrap())
                        .unwrap();
                    assert_eq!(image.media_type, "image/png");
                    assert_eq!(
                        image::load_from_memory(image.bytes()).unwrap().to_rgba8(),
                        pixels
                    );
                }
                assert!(matches!(
                    import(&archive),
                    Err(IoError::ImportNeedsConfirmation(_))
                ));
                book.validate().unwrap();
                assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
            }
        }
    }

    #[test]
    fn static_svg_picture_recovers_for_modern_and_legacy_packages() {
        use base64::Engine as _;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            10,
            4,
            image::Rgba([18, 85, 170, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let embedded = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
        let examples = [
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><rect width="10" height="4" fill="#1255aa"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><style><![CDATA[.box { fill: #1255aa; }]]></style><rect class="box" width="10" height="4"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><style>/* Generated */ .box { /* Fill */ fill: #1255aa; }</style><rect class="box" width="10" height="4"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><rect width="10" height="4" style="/* Fill */ fill:#1255aa"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><style>.box { fill: /* Fill */ #1255aa; }</style><rect class="box" width="10" height="4"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><rect width="10" height="4" style="fill: /* Fill */ #1255aa"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><rect width="10" height="4" style="fill:/**/#1255aa"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><style>.other,/**/.box { fill: #1255aa; }</style><rect class="box" width="10" height="4"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><style>.box/**/{fill/**/:#1255aa/**/;}</style><rect class="box" width="10" height="4"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><rect width="10" height="4" style="fill:#1255aa/**/"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><style>.box/**/ {fill/**/ :#1255aa/**/ ;}</style><rect class="box" width="10" height="4"/></svg>"##.to_vec(),
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><rect width="10" height="4" style="fill:#1255aa/**/  "/></svg>"##.to_vec(),
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><image width="10" height="4" href="data:image/png;base64,{embedded}"/></svg>"#).into_bytes(),
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="4"><image width="10" height="4" xlink:href="data:image/png;base64,{embedded}"/></svg>"#).into_bytes(),
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:ref="http://www.w3.org/1999/xlink" width="10" height="4"><image width="10" height="4" ref:href="data:image/png;base64,{embedded}"/></svg>"#).into_bytes(),
        ];
        let mut examples: Vec<_> = examples
            .into_iter()
            .map(|svg| (svg, [18, 85, 170, 255]))
            .collect();
        examples.push((
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><style>.burlywood { fill: burlywood; }</style><rect class="burlywood" width="10" height="4"/></svg>"#.to_vec(),
            [222, 184, 135, 255],
        ));
        examples.push((
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><rect width="10" height="4" fill="burlywood"/></svg>"#.to_vec(),
            [222, 184, 135, 255],
        ));
        let mut jpeg = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            10,
            4,
            image::Rgb([18, 85, 170]),
        ))
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
        let jpeg = jpeg.into_inner();
        let expected = image::load_from_memory(&jpeg)
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)
            .0;
        let jpeg = base64::engine::general_purpose::STANDARD.encode(jpeg);
        examples.push((format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><image width="10" height="4" href="data:image/jpeg;base64,{jpeg}"/></svg>"#).into_bytes(), expected));
        let mut webp = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            10,
            4,
            image::Rgba([18, 85, 170, 255]),
        ))
        .write_to(&mut webp, image::ImageFormat::WebP)
        .unwrap();
        let webp = base64::engine::general_purpose::STANDARD.encode(webp.into_inner());
        examples.push((format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4"><image width="10" height="4" href="data:image/webp;base64,{webp}"/></svg>"#).into_bytes(), [18, 85, 170, 255]));
        for (svg, expected) in &examples {
            for legacy in [false, true] {
                let content = if legacy {
                    (
                        "content.xml",
                        br#"<xmap-content><sheet><topic
                        xlink:href="xap:resources/picture.svg"><title>Root</title>
                        <image xlink:href="xap:resources/picture.svg"/>
                        </topic></sheet></xmap-content>"#
                            .to_vec(),
                    )
                } else {
                    (
                        "content.json",
                        serde_json::to_vec(&json!([{"rootTopic": {
                            "title": "Root",
                            "href": "xap:resources/picture.svg",
                            "image": {"src": "xap:resources/picture.svg"}
                        }}]))
                        .unwrap(),
                    )
                };
                let archive = make_archive(&[
                    (content.0.into(), content.1),
                    ("resources/picture.svg".into(), svg.to_vec()),
                ]);
                let (book, warnings) = import_with_report(&archive).unwrap();
                assert!(warnings.contains(&ImportWarning::Resources));
                let sheet = book.active();
                let image = sheet
                    .resources
                    .get(sheet.root_topic().image.as_ref().unwrap())
                    .unwrap();
                assert_eq!(image.media_type, "image/png");
                let pixels = image::load_from_memory(image.bytes()).unwrap().to_rgba8();
                assert_eq!(pixels.dimensions(), (10, 4));
                assert_eq!(pixels.get_pixel(0, 0).0, *expected);
                assert_eq!(sheet.root_topic().image_size, Some(Vec2::new(10.0, 4.0)));
                let attachment = sheet.root_topic().attachment.as_ref().unwrap();
                assert_eq!(
                    sheet.resources.get(&attachment.resource).unwrap().bytes(),
                    svg.as_slice()
                );
                assert!(matches!(
                    import(&archive),
                    Err(IoError::ImportNeedsConfirmation(_))
                ));
                book.validate().unwrap();
                assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
            }
        }
    }

    #[test]
    fn unsupported_svg_pictures_are_not_silently_flattened() {
        let examples: &[&[u8]] = &[
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><image href="https://example.com/a.png"/></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"><animate attributeName="x" to="4"/></rect></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><style>@keyframes move {}</style></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><style>.box { ani<!-- split -->mation: move 1s; }</style></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><style>.box { ani&#109;ation: move 1s; }</style></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><style>.box { anima/**/tion: move 1s; }</style></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><style>.box { fill: url(/**/https://example.com/a); }</style></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><style>/* unclosed .box { fill: red; }</style></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><style>.box { fill: url(https://example.com/a); }</style></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="url(https://example.com/a)"/></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" filter="u\72l(https://example.com/a)"/></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect style="ani&#109;ation: move 1s"/></svg>"#,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect style="anim\61tion: move 1s"/></svg>"#,
            b"not svg",
            &[0x1f, 0x8b, 8, 0],
        ];
        for svg in examples {
            for legacy in [false, true] {
                let content = if legacy {
                    (
                        "content.xml",
                        br#"<xmap-content><sheet><topic xlink:href="xap:resources/picture.svg">
                            <title>Root</title><image xlink:href="xap:resources/picture.svg"/>
                            </topic></sheet></xmap-content>"#
                            .to_vec(),
                    )
                } else {
                    (
                        "content.json",
                        serde_json::to_vec(&json!([{"rootTopic": {
                            "title": "Root", "href": "xap:resources/picture.svg",
                            "image": {"src": "xap:resources/picture.svg"}
                        }}]))
                        .unwrap(),
                    )
                };
                let archive = make_archive(&[
                    (content.0.into(), content.1),
                    ("resources/picture.svg".into(), svg.to_vec()),
                ]);
                let (book, warnings) = import_with_report(&archive).unwrap();
                assert!(warnings.contains(&ImportWarning::Resources));
                let sheet = book.active();
                assert!(sheet.root_topic().image.is_none());
                let attachment = sheet.root_topic().attachment.as_ref().unwrap();
                assert_eq!(
                    sheet.resources.get(&attachment.resource).unwrap().bytes(),
                    *svg
                );
                book.validate().unwrap();
            }
        }
    }

    #[test]
    fn svg_picture_rasterization_bounds_large_intrinsic_size() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="50000"><rect width="100000" height="50000" fill="red"/></svg>"#;
        let png = transcode_package_picture("resources/picture.SVG", svg).unwrap();
        let image = images::decode(&png).unwrap();
        assert_eq!((image.width, image.height), (1024, 512));
        assert!(png.len() <= MAX_RESOURCE_BYTES);
    }

    #[test]
    fn compressed_svg_pictures_are_bounded_and_keep_original_attachments() {
        fn gzip(data: &[u8]) -> Vec<u8> {
            let mut writer =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            writer.write_all(data).unwrap();
            writer.finish().unwrap()
        }
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="#1255aa"/></svg>"##;
        let compressed = gzip(svg);
        for path in ["picture.svgz", "picture.svg"] {
            let png = transcode_package_picture(path, &compressed).unwrap();
            let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
            assert_eq!(pixels.get_pixel(0, 0).0, [18, 85, 170, 255]);
        }
        for legacy in [false, true] {
            let content = if legacy {
                ("content.xml", br#"<xmap-content><sheet><topic xlink:href="xap:resources/picture.svgz"><title>Root</title><image xlink:href="xap:resources/picture.svgz"/></topic></sheet></xmap-content>"#.to_vec())
            } else {
                (
                    "content.json",
                    serde_json::to_vec(&json!([{"rootTopic": {
                        "title": "Root", "href": "xap:resources/picture.svgz",
                        "image": {"src": "xap:resources/picture.svgz"}
                    }}]))
                    .unwrap(),
                )
            };
            let archive = make_archive(&[
                (content.0.into(), content.1),
                ("resources/picture.svgz".into(), compressed.clone()),
            ]);
            let (book, warnings) = import_with_report(&archive).unwrap();
            assert!(warnings.contains(&ImportWarning::Resources));
            let sheet = book.active();
            assert!(sheet.root_topic().image.is_some());
            let attachment = sheet.root_topic().attachment.as_ref().unwrap();
            assert_eq!(
                sheet.resources.get(&attachment.resource).unwrap().bytes(),
                compressed
            );
            assert!(matches!(
                import(&archive),
                Err(IoError::ImportNeedsConfirmation(_))
            ));
        }
        let mut trailing = compressed.clone();
        trailing.extend_from_slice(&gzip(svg));
        assert!(transcode_package_picture("picture.svgz", &trailing).is_err());
        assert!(
            transcode_package_picture("picture.svgz", &compressed[..compressed.len() - 2]).is_err()
        );
        let unsafe_svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script/></svg>"#;
        assert!(transcode_package_picture("picture.svgz", &gzip(unsafe_svg)).is_err());
        let mut bounded = svg.to_vec();
        bounded.resize(MAX_RESOURCE_BYTES, b' ');
        assert!(transcode_package_picture("picture.svgz", &gzip(&bounded)).is_ok());
        bounded.push(b' ');
        assert!(transcode_package_picture("picture.svgz", &gzip(&bounded)).is_err());
    }

    #[test]
    fn embedded_svg_pngs_require_static_bounded_images_without_reuse() {
        use base64::Engine as _;

        fn png(width: u32, height: u32) -> Vec<u8> {
            let mut output = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image::RgbaImage::new(width, height))
                .write_to(&mut output, image::ImageFormat::Png)
                .unwrap();
            output.into_inner()
        }
        fn element(bytes: &[u8]) -> String {
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            format!(r#"<image width="1" height="1" href="data:image/png;base64,{data}"/>"#)
        }
        fn svg(contents: &str) -> Vec<u8> {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">{contents}</svg>"#
            )
            .into_bytes()
        }

        let small = element(&png(1, 1));
        assert!(supported_static_svg(&svg(&small.repeat(16))));
        assert!(!supported_static_svg(&svg(&small.repeat(17))));
        let prefixed = small.replace(" href=", " other:href=");
        let wrong_namespace = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:other="urn:other" width="1" height="1">{prefixed}</svg>"#
        );
        assert!(!supported_static_svg(wrong_namespace.as_bytes()));
        let xlink_namespace = wrong_namespace.replace("urn:other", "http://www.w3.org/1999/xlink");
        assert!(supported_static_svg(xlink_namespace.as_bytes()));
        let wrong_element = small.replace("<image", "<other:image");
        let wrong_element = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:other="urn:other" width="1" height="1">{wrong_element}</svg>"#
        );
        assert!(!supported_static_svg(wrong_element.as_bytes()));
        assert!(!supported_static_svg(&svg(&format!(
            "{small}<use href=\"#x\"/>"
        ))));
        assert!(!supported_static_svg(&svg(
            r#"<image href="data:image/png;base64,bm90LXBuZw=="/>"#
        )));
        assert!(!supported_static_svg(&svg(
            r#"<image href="data:image/svg+xml;base64,PHN2Zy8+"/>"#
        )));
        assert!(!supported_static_svg(&svg(r#"<image/>"#)));

        let big = element(&png(2048, 2048));
        assert!(supported_static_svg(&svg(&big)));
        assert!(!supported_static_svg(&svg(&format!("{big}{small}"))));

        let mut animated = png(1, 1);
        let mut chunk = Vec::from(&b"acTL\0\0\0\x02\0\0\0\0"[..]);
        let checksum = crc32fast::hash(&chunk);
        chunk.extend_from_slice(&checksum.to_be_bytes());
        animated.splice(33..33, [8u32.to_be_bytes().as_slice(), &chunk].concat());
        assert!(!supported_static_svg(&svg(&element(&animated))));
    }

    #[test]
    fn embedded_svg_jpegs_preserve_pixels_and_reject_orientation_changes() {
        use base64::Engine as _;
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            4,
            4,
            image::Rgb([18, 85, 170]),
        ))
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .unwrap();
        let jpeg = output.into_inner();
        let svg = |bytes: &[u8]| {
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><image width="4" height="4" href="data:image/jpeg;base64,{data}"/></svg>"#).into_bytes()
        };
        let png = transcode_package_picture("picture.svg", &svg(&jpeg)).unwrap();
        let source = image::load_from_memory(&jpeg).unwrap().to_rgba8();
        let result = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(source, result);

        let exif = [
            b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1,
            0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
        ];
        let mut rotated = jpeg.clone();
        let length = u16::try_from(exif.len() + 2).unwrap().to_be_bytes();
        let segment = [&[0xff, 0xe1][..], &length, &exif].concat();
        rotated.splice(2..2, segment);
        assert!(images::requires_visual_normalization(&rotated).unwrap());
        assert!(!supported_static_svg(&svg(&rotated)));
        assert!(!supported_static_svg(&svg(&[0x89, b'P', b'N', b'G'])));
    }

    #[test]
    fn embedded_svg_webp_preserves_transparency_and_rejects_animation() {
        use base64::Engine as _;
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            4,
            4,
            image::Rgba([20, 40, 80, 128]),
        ))
        .write_to(&mut output, image::ImageFormat::WebP)
        .unwrap();
        let still = output.into_inner();
        let svg = |bytes: &[u8]| {
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><image width="4" height="4" href="data:image/webp;base64,{data}"/></svg>"#).into_bytes()
        };
        let png = transcode_package_picture("picture.svg", &svg(&still)).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        let pixel = pixels.get_pixel(0, 0).0;
        assert_eq!(pixel[3], 128);
        for (actual, expected) in pixel[..3].iter().zip([20u8, 40, 80]) {
            assert!(actual.abs_diff(expected) <= 1);
        }

        let mut frame = vec![0; 16];
        frame[6] = 3;
        frame[9] = 3;
        frame[12] = 1;
        frame.extend_from_slice(&still[12..]);
        let mut payload = b"WEBPVP8X".to_vec();
        payload.extend_from_slice(&10u32.to_le_bytes());
        payload.extend_from_slice(&[2, 0, 0, 0, 3, 0, 0, 3, 0, 0]);
        payload.extend_from_slice(b"ANIM");
        payload.extend_from_slice(&6u32.to_le_bytes());
        payload.extend_from_slice(&[0; 6]);
        payload.extend_from_slice(b"ANMF");
        payload.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        payload.extend_from_slice(&frame);
        if !frame.len().is_multiple_of(2) {
            payload.push(0);
        }
        let mut animated = b"RIFF".to_vec();
        animated.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        animated.extend_from_slice(&payload);
        assert!(
            image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&animated))
                .unwrap()
                .has_animation()
        );
        let animated_svg = svg(&animated);
        assert!(!supported_static_svg(&animated_svg));
        for legacy in [false, true] {
            let content = if legacy {
                ("content.xml", br#"<xmap-content><sheet><topic xlink:href="xap:resources/picture.svg"><title>Root</title><image xlink:href="xap:resources/picture.svg"/></topic></sheet></xmap-content>"#.to_vec())
            } else {
                (
                    "content.json",
                    serde_json::to_vec(&json!([{"rootTopic": {
                        "title": "Root", "href": "xap:resources/picture.svg",
                        "image": {"src": "xap:resources/picture.svg"}
                    }}]))
                    .unwrap(),
                )
            };
            let archive = make_archive(&[
                (content.0.into(), content.1),
                ("resources/picture.svg".into(), animated_svg.clone()),
            ]);
            let (book, warnings) = import_with_report(&archive).unwrap();
            assert!(warnings.contains(&ImportWarning::Resources));
            let sheet = book.active();
            assert!(sheet.root_topic().image.is_none());
            let attachment = sheet.root_topic().attachment.as_ref().unwrap();
            assert_eq!(
                sheet.resources.get(&attachment.resource).unwrap().bytes(),
                animated_svg
            );
        }
    }

    #[test]
    fn embedded_svg_gif_preserves_frame_offsets_and_rejects_animation() {
        use base64::Engine as _;
        let frame = image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 30, 70, 255]));
        let mut still = Vec::new();
        image::codecs::gif::GifEncoder::new(&mut still)
            .encode_frame(image::Frame::new(frame.clone()))
            .unwrap();
        still[6..10].copy_from_slice(&[4, 0, 4, 0]);
        let packed = still[10];
        let mut offset = 13;
        if packed & 0x80 != 0 {
            offset += 3 * (1usize << (usize::from(packed & 7) + 1));
        }
        while still[offset] == 0x21 {
            offset += 2;
            while still[offset] != 0 {
                offset += 1 + usize::from(still[offset]);
            }
            offset += 1;
        }
        assert_eq!(still[offset], 0x2c);
        still[offset + 1..offset + 5].copy_from_slice(&[1, 0, 1, 0]);
        let svg = |bytes: &[u8]| {
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><image width="4" height="4" href="data:image/gif;base64,{data}"/></svg>"#).into_bytes()
        };
        let png = transcode_package_picture("picture.svg", &svg(&still)).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(pixels.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(pixels.get_pixel(1, 1).0, [200, 30, 70, 255]);
        assert_eq!(pixels.get_pixel(3, 3).0, [0, 0, 0, 0]);

        let mut animated = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut animated);
            encoder
                .encode_frame(image::Frame::new(frame.clone()))
                .unwrap();
            encoder.encode_frame(image::Frame::new(frame)).unwrap();
        }
        assert!(!supported_static_svg(&svg(&animated)));
        for (gif, recovered) in [(&still, true), (&animated, false)] {
            let picture = svg(gif);
            for legacy in [false, true] {
                let content = if legacy {
                    ("content.xml", br#"<xmap-content><sheet><topic xlink:href="xap:resources/picture.svg"><title>Root</title><image xlink:href="xap:resources/picture.svg"/></topic></sheet></xmap-content>"#.to_vec())
                } else {
                    (
                        "content.json",
                        serde_json::to_vec(&json!([{"rootTopic": {
                            "title": "Root", "href": "xap:resources/picture.svg",
                            "image": {"src": "xap:resources/picture.svg"}
                        }}]))
                        .unwrap(),
                    )
                };
                let archive = make_archive(&[
                    (content.0.into(), content.1),
                    ("resources/picture.svg".into(), picture.clone()),
                ]);
                let (book, warnings) = import_with_report(&archive).unwrap();
                assert!(warnings.contains(&ImportWarning::Resources));
                assert!(matches!(
                    import(&archive),
                    Err(IoError::ImportNeedsConfirmation(_))
                ));
                let sheet = book.active();
                assert_eq!(sheet.root_topic().image.is_some(), recovered);
                let attachment = sheet.root_topic().attachment.as_ref().unwrap();
                assert_eq!(
                    sheet.resources.get(&attachment.resource).unwrap().bytes(),
                    picture
                );
            }
        }
    }

    #[test]
    fn svg_css_allows_local_gradients_but_not_external_urls() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4">
            <defs><linearGradient id="gradient"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient></defs>
            <style>.box { fill: url("#gradient"); }</style>
            <rect class="box" width="10" height="4"/>
            </svg>"##;
        let png = transcode_package_picture("resources/picture.svg", svg).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        assert!(pixels.get_pixel(0, 0).0[0] > pixels.get_pixel(0, 0).0[2]);
        assert!(pixels.get_pixel(9, 0).0[2] > pixels.get_pixel(9, 0).0[0]);
        let presentation = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="4">
            <defs><linearGradient id="gradient"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient></defs>
            <rect fill="url(#gradient)" width="10" height="4"/>
            </svg>"##;
        let png = transcode_package_picture("resources/picture.svg", presentation).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        assert!(pixels.get_pixel(0, 0).0[0] > pixels.get_pixel(0, 0).0[2]);
        assert!(pixels.get_pixel(9, 0).0[2] > pixels.get_pixel(9, 0).0[0]);
        for css in [
            "fill:url(https://example.com/image)",
            "fill:url('https://example.com/image')",
            "fill:url(#local) url(https://example.com/image)",
            "fill:url(\")",
            "fill:url(#)",
            "fill:url(#local",
        ] {
            assert!(!supported_static_svg_css(css), "{css}");
        }
        for css in ["fill:url(#local)", "fill:URL( '#local-id' )"] {
            assert!(supported_static_svg_css(css), "{css}");
        }
    }

    #[test]
    fn svg_css_accepts_url_inside_identifiers_without_accepting_external_urls() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><style>.burlywood { fill: burlywood; }</style><rect class="burlywood" width="4" height="4"/></svg>"#;
        let png = transcode_package_picture("picture.svg", svg).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(pixels.get_pixel(1, 1).0, [222, 184, 135, 255]);
        assert!(supported_static_svg_css(".urlcolor { fill: burlywood }"));
        for css in [
            ".burlywood { fill: url(https://example.com/image) }",
            "fill:burlywood;stroke:url(https://example.com/image)",
            "fill:burlywood;stroke:u/**/rl(https://example.com/image)",
        ] {
            assert!(!supported_static_svg_css(css), "{css}");
        }
    }

    #[test]
    fn svg_css_allows_whitespace_delimited_comments_without_hiding_tokens() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><style>.box { fill: /* color */ #1255aa; }</style><rect class="box" width="4" height="4"/></svg>"##;
        let png = transcode_package_picture("picture.svg", svg).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(pixels.get_pixel(1, 1).0, [0x12, 0x55, 0xaa, 255]);
        for css in [
            "anima/* comment */tion: slide",
            "fill: u/* comment */rl(https://example.com/image)",
            "fill: url( /* comment */ https://example.com/image)",
            "fill: /* comment */ url(https://example.com/image)",
            "fill:/**/url(https://example.com/image)",
            "fill:/**/var(--color)",
            "animation:/**/slide",
            ".a,/**/.b { fill: url(https://example.com/image) }",
            ".a,/**/.b { anima/**/tion: slide }",
            ".a,/**/.b { fill: u/**/rl(https://example.com/image) }",
            "fill: rgb(1,/**/2,3)",
            "animation/**/:slide",
            "fill/**/:url(https://example.com/image)",
            "fill:url/**/(https://example.com/image)",
            "animation:slide/**/",
            "fill:url(https://example.com/image)/**/",
            "fill:var(--color)/**/",
            "animation/**/ :slide",
            "fill/**/ :url(https://example.com/image)",
            "fill:u/**/ rl(https://example.com/image)",
        ] {
            assert!(!supported_static_svg_css(css), "{css}");
        }
        assert!(supported_static_svg_css(
            ".other,/**/.box { fill: #1255aa; }"
        ));
        assert!(supported_static_svg_css(".box/**/{fill/**/:#1255aa/**/;}"));
        assert!(supported_static_svg_css("fill:#1255aa/**/"));
        assert!(supported_static_svg_css(
            ".box/**/ {fill/**/ :#1255aa/**/ ;}"
        ));
        assert!(supported_static_svg_css("fill:#1255aa/**/  "));
        assert!(supported_static_svg_css("fill:#1255aa/**/\t\n\r\u{c}"));
    }

    #[test]
    fn webp_expansion_respects_sheet_and_workbook_resource_limits() {
        let one_sheet = MAX_SHEET_RESOURCE_BYTES;
        assert!(image_expansion_fits(one_sheet - 1, 1, 1));
        assert!(!image_expansion_fits(one_sheet - 1, 2, 1));
        let three_sheets = MAX_WORKBOOK_RESOURCE_BYTES / 3;
        assert!(image_expansion_fits(three_sheets - 1, 1, 3));
        assert!(!image_expansion_fits(three_sheets - 1, 2, 3));
        assert!(!image_expansion_fits(one_sheet + 1, 0, 1));
        assert!(!image_expansion_fits(one_sheet + 1, 1, 1));
    }

    #[test]
    fn modern_picture_budget_enforces_sheet_and_workbook_byte_boundaries() {
        let mut shared = ResourceReferences::default();
        shared.images.insert("picture".into());
        shared.attachments.insert("picture".into());
        shared.attachments.insert("filler".into());
        let sizes = HashMap::from([
            ("picture".into(), 10),
            ("filler".into(), MAX_SHEET_RESOURCE_BYTES - 20),
        ]);
        let mut budget = PackageRecoveryBudget::new(vec![shared], &sizes);
        assert!(!budget.reserve_picture("picture", 10, 11));
        assert!(budget.reserve_picture("picture", 10, 10));

        let mut first = ResourceReferences::default();
        first.images.insert("picture".into());
        first.attachments.insert("first".into());
        let mut second = ResourceReferences::default();
        second.attachments.insert("second".into());
        let mut third = ResourceReferences::default();
        third.attachments.insert("third".into());
        let half = MAX_SHEET_RESOURCE_BYTES / 2;
        let sizes = HashMap::from([
            ("picture".into(), 10),
            ("first".into(), MAX_SHEET_RESOURCE_BYTES - 11),
            ("second".into(), half + 1),
            ("third".into(), half),
        ]);
        let mut budget = PackageRecoveryBudget::new(vec![first, second, third], &sizes);
        assert!(!budget.reserve_picture("picture", 10, 11));
        assert!(budget.reserve_picture("picture", 10, 10));
    }

    #[test]
    fn webp_recovery_does_not_exceed_workbook_resource_count() {
        let pixels = image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
        let picture = image::DynamicImage::ImageRgba8(pixels);
        let mut png = Cursor::new(Vec::new());
        let mut webp = Cursor::new(Vec::new());
        picture.write_to(&mut png, image::ImageFormat::Png).unwrap();
        picture
            .write_to(&mut webp, image::ImageFormat::WebP)
            .unwrap();
        let sheets: Vec<_> = (0..65)
            .map(|_| {
                json!({"rootTopic": {
                    "title": "Root",
                    "image": {"src": "xap:resources/picture.png"},
                    "children": {"attached": [{
                        "title": "WebP",
                        "image": {"src": "xap:resources/picture.webp"}
                    }]}
                }})
            })
            .collect();
        let archive = make_archive(&[
            ("content.json".into(), serde_json::to_vec(&sheets).unwrap()),
            ("resources/picture.png".into(), png.into_inner()),
            ("resources/picture.webp".into(), webp.into_inner()),
        ]);
        let (book, warnings) = import_with_report(&archive).unwrap();
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        assert_eq!(book.sheets.len(), 65);
        for sheet in &book.sheets {
            assert!(sheet.root_topic().image.is_some());
            assert!(
                sheet
                    .topic(sheet.root_topic().children[0])
                    .unwrap()
                    .image
                    .is_none()
            );
        }
        book.validate().unwrap();
    }

    #[test]
    fn webp_recovery_counts_only_resources_referenced_by_each_sheet() {
        let pixels = image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
        let picture = image::DynamicImage::ImageRgba8(pixels);
        let mut png = Cursor::new(Vec::new());
        let mut webp = Cursor::new(Vec::new());
        picture.write_to(&mut png, image::ImageFormat::Png).unwrap();
        picture
            .write_to(&mut webp, image::ImageFormat::WebP)
            .unwrap();
        let png = png.into_inner();
        let sheets: Vec<_> = (0..65)
            .map(|index| {
                let path = if index == 0 {
                    "picture.webp"
                } else {
                    "picture.png"
                };
                json!({"rootTopic": {
                    "title": "Root",
                    "image": {"src": format!("xap:resources/{path}")}
                }})
            })
            .collect();
        let archive = make_archive(&[
            ("content.json".into(), serde_json::to_vec(&sheets).unwrap()),
            ("resources/picture.png".into(), png.clone()),
            ("resources/picture.webp".into(), webp.into_inner()),
        ]);
        let (book, warnings) = import_with_report(&archive).unwrap();
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        assert_eq!(book.sheets.len(), 65);
        for (index, sheet) in book.sheets.iter().enumerate() {
            let resource = sheet
                .resources
                .get(sheet.root_topic().image.as_ref().unwrap())
                .unwrap();
            if index != 0 {
                assert_eq!(resource.bytes(), png);
            } else {
                assert_eq!(resource.media_type, "image/png");
            }
        }
        book.validate().unwrap();
    }

    #[test]
    fn malformed_webp_picture_is_skipped_with_a_warning() {
        let archive = make_archive(&[
            (
                "content.json".into(),
                serde_json::to_vec(&json!([{"rootTopic": {
                    "title": "Root",
                    "image": {"src": "xap:resources/broken.webp"}
                }}]))
                .unwrap(),
            ),
            (
                "resources/broken.webp".into(),
                b"RIFF\x04\0\0\0WEBP".to_vec(),
            ),
        ]);
        let (book, warnings) = import_with_report(&archive).unwrap();
        assert!(book.active().root_topic().image.is_none());
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        assert!(matches!(
            import(&archive),
            Err(IoError::ImportNeedsConfirmation(_))
        ));
    }

    #[test]
    fn package_images_enforce_display_pixel_and_encoded_size_bounds() {
        let (book, _) = image_workbook();
        let original = archive_entries(&export(&book).unwrap());

        for (width, expected) in [(0.0, None), (-1.0, None), (1500.0, Some(1024.0))] {
            let mut invalid_display = original.clone();
            update_content(&mut invalid_display, |content| {
                content[0]["rootTopic"]["image"]["width"] = json!(width);
            });
            let (workbook, warnings) = report(&invalid_display);
            assert_eq!(warnings, vec![ImportWarning::Formatting], "{width}");
            let size = workbook.active().root_topic().image_size.unwrap();
            assert!(images::valid_size(size), "{width}");
            if let Some(expected) = expected {
                assert_eq!(size.x, expected, "{width}");
            }
        }

        let mut oversized_pixels = original.clone();
        let path = image_resource_path(book.active().root);
        oversized_pixels
            .iter_mut()
            .find(|(name, _)| name == &path)
            .unwrap()
            .1 = png([0, 0, 0], images::MAX_DIMENSION + 1, 1);
        let (workbook, warnings) = report(&oversized_pixels);
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        assert_eq!(workbook.active().root_topic().image, None);

        let mut oversized_bytes = original;
        oversized_bytes
            .iter_mut()
            .find(|(name, _)| name == &path)
            .unwrap()
            .1 = vec![0; MAX_RESOURCE_BYTES + 1];
        assert!(
            import(&make_archive(&oversized_bytes))
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
    }

    #[test]
    fn changed_archive_image_does_not_restore_stale_private_bytes_and_attachments_are_reported() {
        let (book, _) = image_workbook();
        let mut changed = archive_entries(&export(&book).unwrap());
        let path = image_resource_path(book.active().root);
        changed
            .iter_mut()
            .find(|(name, _)| name == &path)
            .unwrap()
            .1 = png([10, 210, 40], 32, 24);
        let imported = import(&make_archive(&changed)).unwrap();
        assert_ne!(imported.active().resources, book.active().resources);
        assert_eq!(
            imported
                .active()
                .resources
                .get(imported.active().root_topic().image.as_deref().unwrap())
                .unwrap()
                .bytes(),
            png([10, 210, 40], 32, 24)
        );

        let mut with_attachment = Sheet::new("Files", "Root");
        let root = with_attachment.root;
        thinoda_core::attachments::insert(
            &mut with_attachment,
            root,
            "evidence.bin",
            vec![1, 2, 3],
        )
        .unwrap();
        let (imported, warnings) = public_report(&Workbook::new(with_attachment));
        assert!(warnings.is_empty(), "{warnings:?}");
        let topic = imported.active().root_topic().children[0];
        let attachment = imported.active().topics[&topic].attachment.clone().unwrap();
        assert_eq!(attachment.name, "evidence.bin");
        assert_eq!(
            imported
                .active()
                .resources
                .get(&attachment.resource)
                .unwrap()
                .bytes(),
            [1, 2, 3]
        );

        // The structured `attachment` object is not XMind's link model and stays unsupported.
        let mut attachment_content = archive_entries(&export(&book).unwrap());
        update_content(&mut attachment_content, |content| {
            content[0]["rootTopic"]["attachment"] = json!({"src": "xap:resources/anything"});
        });
        let (workbook, warnings) = report(&attachment_content);
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        assert_eq!(
            workbook.active().root_topic().title,
            book.active().root_topic().title
        );

        let (_, warnings) = import_legacy_xml_with_resources(
            b"<xmap-content><sheet><topic><image src=\"x\"/></topic></sheet></xmap-content>",
            None,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(warnings, vec![ImportWarning::Resources]);
    }

    fn content_report(sheet: Value) -> (Workbook, Vec<ImportWarning>) {
        let entries = vec![(
            "content.json".to_owned(),
            serde_json::to_vec(&json!([sheet])).unwrap(),
        )];
        report(&entries)
    }

    #[test]
    fn encrypted_packages_fail_with_an_actionable_message() {
        let entries = vec![
            ("content.json".to_owned(), b"\x00\x01binary".to_vec()),
            (
                "manifest.json".to_owned(),
                serde_json::to_vec(&json!({"file-entries": {
                    "content.json": {"encryption-data": {"algorithm-name": "AES/CBC/PKCS5Padding"}},
                }}))
                .unwrap(),
            ),
        ];
        let error = import_with_report(&make_archive(&entries)).unwrap_err();
        assert_eq!(
            error.to_string(),
            IoError::Invalid(ENCRYPTED_PACKAGE_ERROR.into()).to_string()
        );
    }

    #[test]
    fn callouts_and_nested_detached_topics_become_linked_floating_topics() {
        let (workbook, warnings) = content_report(json!({
            "rootTopic": {
                "id": "root",
                "title": "Root",
                "children": {
                    "attached": [{
                        "id": "a",
                        "title": "A",
                        "children": {
                            "callout": [{"id": "c1", "title": "Callout 1"}, {"id": "c2", "title": "Callout 2"}],
                            "detached": [{"id": "d", "title": "Nested", "position": {"x": 10, "y": 20}}],
                        },
                    }],
                    "callout": [{"id": "rc", "title": "Root callout",
                        "children": {"callout": [{"id": "cc", "title": "Callout of callout"}]}}],
                },
            },
        }));
        assert_eq!(warnings, vec![ImportWarning::Other]);
        let sheet = workbook.active();
        let by_title = |title: &str| {
            sheet
                .topics
                .values()
                .find(|topic| topic.title == title)
                .unwrap()
                .id
        };
        let a = by_title("A");
        assert_eq!(sheet.root_topic().children, vec![a]);
        assert!(sheet.topic(a).unwrap().children.is_empty());
        let layout =
            thinoda_core::layout::layout_sheet(sheet, &thinoda_core::layout::ApproxMeasure, false);
        let a_rect = layout.rect(a).unwrap();
        for (owner, title) in [
            (a, "Callout 1"),
            (a, "Callout 2"),
            (a, "Nested"),
            (sheet.root, "Root callout"),
            (by_title("Root callout"), "Callout of callout"),
        ] {
            let floating = by_title(title);
            assert!(sheet.floating.contains(&floating), "{title}");
            assert_eq!(sheet.topic(floating).unwrap().parent, None, "{title}");
            assert!(
                sheet
                    .relationships
                    .iter()
                    .any(|relationship| relationship.from == owner && relationship.to == floating),
                "{title}"
            );
        }
        // Callouts stack beside their owner; detached positions are owner-relative.
        let c1 = sheet
            .topic(by_title("Callout 1"))
            .unwrap()
            .position
            .unwrap();
        let c2 = sheet
            .topic(by_title("Callout 2"))
            .unwrap()
            .position
            .unwrap();
        assert_eq!(c1.x, a_rect.min.x + a_rect.size.x + 40.0);
        assert_eq!(c2.y, c1.y - 50.0);
        assert_eq!(
            sheet.topic(by_title("Nested")).unwrap().position,
            Some(a_rect.min + Vec2::new(10.0, 20.0))
        );
        // A callout of a callout is placed relative to its placed owner.
        let rc = sheet
            .topic(by_title("Root callout"))
            .unwrap()
            .position
            .unwrap();
        let cc = sheet
            .topic(by_title("Callout of callout"))
            .unwrap()
            .position
            .unwrap();
        assert!(cc.x > rc.x);
        workbook.validate().unwrap();
    }

    #[test]
    fn note_fallbacks_keep_visible_text_and_report_lost_formatting() {
        let note = |notes: Value| {
            let (workbook, warnings) =
                content_report(json!({"rootTopic": {"title": "Root", "notes": notes}}));
            let text = workbook
                .active()
                .root_topic()
                .notes
                .as_ref()
                .map(|notes| notes.to_string());
            (text, warnings)
        };
        assert_eq!(
            note(json!({"plain": {"content": "Plain"}, "realHTML": {"content": "<p>Plain</p>"}})),
            (Some("Plain".into()), vec![])
        );
        assert_eq!(
            note(
                json!({"plain": {"content": "Plain"}, "realHTML": {"content": "<p><b>Plain</b></p>"}})
            ),
            (Some("Plain".into()), vec![ImportWarning::Notes])
        );
        assert_eq!(
            note(
                json!({"realHTML": {"content": "<div>One &amp; two</div><div>Three<br/>Four &#x263A;</div>"}})
            ),
            (
                Some("One & two\nThree\nFour \u{263A}".into()),
                vec![ImportWarning::Notes]
            )
        );
        assert_eq!(
            note(
                json!({"ops": {"ops": [{"insert": "Bold", "attributes": {"bold": true}}, {"insert": " text\n"}]}})
            ),
            (Some("Bold text".into()), vec![ImportWarning::Notes])
        );
    }

    #[test]
    fn unsupported_topic_and_sheet_data_is_reported() {
        let cases = [
            (
                json!({"title": "Root", "markers": [{"markerId": "vendor-unknown"}]}),
                ImportWarning::Markers,
            ),
            (
                json!({"title": "Root", "markers": {"markerId": "priority-1"}}),
                ImportWarning::Markers,
            ),
            (
                json!({"title": "Root", "structureClass": "org.vendor.future"}),
                ImportWarning::Formatting,
            ),
            (
                json!({"title": "Root", "structureClass": "org.xmind.ui.timeline.vertical.btt"}),
                ImportWarning::Formatting,
            ),
            (
                json!({"title": "Root", "labels": ["kept", {"name": "lost"}]}),
                ImportWarning::Other,
            ),
            (
                json!({"title": "Root", "children": ["lost"]}),
                ImportWarning::Other,
            ),
            (
                json!({"title": "Root", "children": {"attached": {"title": "lost"}}}),
                ImportWarning::Other,
            ),
            (
                json!({"title": "Root", "taskInfo": {"progress": 0.5}}),
                ImportWarning::Markers,
            ),
            (
                json!({"title": "Root", "href": "xmind:#other-topic"}),
                ImportWarning::Links,
            ),
            (
                json!({"title": "Root", "comments": [{"content": "Review"}]}),
                ImportWarning::Other,
            ),
            (
                json!({"title": "Root", "children": {"future": [{"title": "X"}]}}),
                ImportWarning::Other,
            ),
            (
                json!({"title": "Root", "attributedTitle": [{"text": "Root", "fo:font-style": "italic"}]}),
                ImportWarning::Formatting,
            ),
            (
                json!({"title": "Root", "summaries": [{"topicId": "nowhere", "range": "(0,0)"}]}),
                ImportWarning::Other,
            ),
        ];
        for (root, expected) in cases {
            let (_, warnings) = content_report(json!({"rootTopic": root.clone()}));
            assert_eq!(warnings, vec![expected], "{root}");
            let archive = make_archive(&[(
                "content.json".to_owned(),
                serde_json::to_vec(&json!([{"rootTopic": root}])).unwrap(),
            )]);
            assert!(matches!(
                import(&archive),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings.contains(&expected)
            ));
        }
        let (book, warnings) = content_report(json!({
            "rootTopic": {"title": "Root", "labels": ["kept", {"name": "lost"}]}
        }));
        assert_eq!(book.active().root_topic().labels, ["kept"]);
        assert_eq!(warnings, vec![ImportWarning::Other]);
        let (_, warnings) = content_report(json!({
            "rootTopic": {
                "title": "Root", "markers": [], "labels": [],
                "children": {}, "structureClass": "org.xmind.ui.map"
            }
        }));
        assert!(warnings.is_empty());
        let (workbook, warnings) = content_report(json!({
            "rootTopic": {"id": "r", "title": "Root"},
            "relationships": [{"end1Id": "r", "end2Id": "missing"}],
        }));
        assert_eq!(warnings, vec![ImportWarning::Other]);
        assert!(workbook.active().relationships.is_empty());
        let (_, warnings) = content_report(json!({
            "rootTopic": {"title": "Root"},
            "zones": [{"id": "z"}],
        }));
        assert_eq!(warnings, vec![ImportWarning::Other]);
        let (_, warnings) = content_report(json!({"rootTopic": {"title": "Root"}, "zones": []}));
        assert!(warnings.is_empty());
        for field in [
            json!({"topicOverlapping": false}),
            json!({"topicOverlapping": true}),
            json!({"arrangeableLayerOrder": ["topic"]}),
            json!({"extensions": [{"provider": "vendor", "content": {"style": "value"}}]}),
        ] {
            let mut sheet = json!({"rootTopic": {"title": "Root"}});
            sheet
                .as_object_mut()
                .unwrap()
                .extend(field.as_object().unwrap().clone());
            let (_, warnings) = content_report(sheet.clone());
            assert_eq!(warnings, vec![ImportWarning::Other], "{sheet}");
            let archive = make_archive(&[(
                "content.json".to_owned(),
                serde_json::to_vec(&json!([sheet])).unwrap(),
            )]);
            assert!(matches!(
                import(&archive),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings.contains(&ImportWarning::Other)
            ));
        }
        let (_, warnings) = content_report(json!({
            "rootTopic": {"title": "Root"},
            "topicOverlapping": null,
            "arrangeableLayerOrder": [],
            "extensions": [],
        }));
        assert!(warnings.is_empty());
    }

    #[test]
    fn attributed_title_text_is_recovered_or_disclosed() {
        let (book, warnings) = content_report(json!({
            "rootTopic": {"attributedTitle": [{"text": "Hello "}, {"text": "world"}]}
        }));
        assert_eq!(book.active().root_topic().title, "Hello world");
        assert!(warnings.is_empty());

        let (_, warnings) = content_report(json!({
            "rootTopic": {"title": "Plain", "attributedTitle": [{"text": "Pl"}, {"text": "ain"}]}
        }));
        assert!(warnings.is_empty());

        let (book, warnings) = content_report(json!({
            "rootTopic": {"title": "Plain", "attributedTitle": [{"text": "Different"}]}
        }));
        assert_eq!(book.active().root_topic().title, "Plain");
        assert_eq!(warnings, vec![ImportWarning::Formatting]);
        let archive = make_archive(&[(
            "content.json".to_owned(),
            serde_json::to_vec(&json!([{
                "rootTopic": {"title": "Plain", "attributedTitle": [{"text": "Different"}]}
            }]))
            .unwrap(),
        )]);
        assert!(matches!(
            import(&archive),
            Err(IoError::ImportNeedsConfirmation(warnings))
                if warnings.contains(&ImportWarning::Formatting)
        ));

        let (_, warnings) = content_report(json!({
            "rootTopic": {"title": "Plain", "attributedTitle": [{"text": 42}]}
        }));
        assert_eq!(warnings, vec![ImportWarning::Formatting]);

        let (book, warnings) = content_report(json!({
            "rootTopic": {"title": "Root", "children": {"attached": [
                {"attributedTitle": [{"text": "Child"}]}
            ]}}
        }));
        let root = book.active().root_topic();
        assert_eq!(book.active().topics[&root.children[0]].title, "Child");
        assert!(warnings.is_empty());

        let (book, warnings) = content_report(json!({
            "rootTopic": {"attributedTitle": [{"text": "Styled", "fo:font-weight": "bold"}]}
        }));
        assert_eq!(book.active().root_topic().title, "Styled");
        assert_eq!(warnings, vec![ImportWarning::Formatting]);

        let (book, warnings) = content_report(json!({
            "rootTopic": {"title": "Plain", "attributedTitle": []}
        }));
        assert_eq!(book.active().root_topic().title, "Plain");
        assert!(warnings.is_empty());
    }

    #[test]
    fn unreferenced_package_resources_are_reported_but_never_read() {
        let entries = vec![
            (
                "content.json".to_owned(),
                serde_json::to_vec(&json!([{"rootTopic": {"title": "Root"}}])).unwrap(),
            ),
            // Not a decodable image: proves unused resources are not decoded.
            ("resources/unused.bin".to_owned(), b"not an image".to_vec()),
        ];
        let (workbook, warnings) = report(&entries);
        assert_eq!(warnings, vec![ImportWarning::Resources]);
        assert!(workbook.active().resources.entries.is_empty());
        assert!(matches!(
            import(&make_archive(&entries)),
            Err(IoError::ImportNeedsConfirmation(_))
        ));
    }

    /// The map stored in `tests/fixtures/thinoda-pre-x2-export.xmind`, written by the
    /// exporter before public projection 2. Its private-only fields (numbering, width,
    /// control points, branch sides) were absent from that file's public content.
    fn pre_x2_workbook() -> Workbook {
        let mut sheet = Sheet::new("Projection", "Root");
        let root = sheet.root;
        sheet.structure = Structure::MapUnbalanced;
        let a = sheet.add_child(root, None, "Alpha");
        let b = sheet.add_child(root, None, "Beta");
        let c = sheet.add_child(root, None, "Gamma");
        let a1 = sheet.add_child(a, None, "Alpha one");
        sheet.add_child(a, None, "Alpha two");
        let summary = sheet.add_child(a, None, "Alpha summary");
        sheet
            .topic_mut(a)
            .unwrap()
            .children
            .retain(|id| *id != summary);
        sheet.topic_mut(summary).unwrap().parent = None;
        let summary_id = sheet.alloc_summary_id();
        sheet.summaries.push(Summary {
            id: summary_id,
            parent: a,
            range: ChildRange { start: 0, end: 1 },
            topic: summary,
        });
        let boundary_id = sheet.alloc_boundary_id();
        sheet.boundaries.push(Boundary {
            id: boundary_id,
            parent: root,
            range: Some(ChildRange { start: 1, end: 2 }),
            title: Some("Group".into()),
            color: Color::from_hex("#336699"),
        });
        let relationship_id = sheet.alloc_relationship_id();
        sheet.relationships.push(thinoda_core::Relationship {
            id: relationship_id,
            from: a1,
            to: c,
            title: "relates".into(),
            color: Color::from_hex("#AA0000"),
            control_points: Some([Vec2::new(40.0, -30.0), Vec2::new(-20.0, 15.5)]),
        });
        let reference = sheet
            .resources
            .insert(Resource::new("dot.png", "image/png", png([10, 200, 90], 8, 6)).unwrap())
            .unwrap();
        let topic = sheet.topic_mut(a).unwrap();
        topic.child_numbering = Some(thinoda_core::numbering::Numbering {
            format: thinoda_core::numbering::NumberFormat::UpperRoman,
            tiered: true,
            start: 1,
        });
        topic.style.width = Some(180.0);
        topic.style.fill = Color::from_hex("#FFEEAA");
        topic.style.bold = Some(true);
        topic.notes = Some("Plain note".to_owned().into());
        topic.labels = vec!["label".into()];
        topic.markers = vec!["priority-1".into()];
        topic.hyperlink = Some("https://example.com/".into());
        let topic = sheet.topic_mut(b).unwrap();
        topic.branch_side = Some(thinoda_core::BranchSide::Left);
        topic.image = Some(reference);
        topic.image_size = Some(Vec2::new(16.0, 12.0));
        sheet.topic_mut(c).unwrap().collapsed = false;
        let floating = sheet.add_child(root, None, "Floating");
        sheet
            .topic_mut(root)
            .unwrap()
            .children
            .retain(|id| *id != floating);
        let topic = sheet.topic_mut(floating).unwrap();
        topic.parent = None;
        topic.position = Some(Vec2::new(300.0, -120.0));
        sheet.floating.push(floating);
        Workbook::new(sheet)
    }

    #[test]
    fn pre_projection_2_exports_still_restore_their_private_snapshot() {
        let fixture = include_bytes!("../tests/fixtures/thinoda-pre-x2-export.xmind");
        let entries = archive_entries(fixture);
        let metadata: Value = serde_json::from_slice(
            &entries
                .iter()
                .find(|(name, _)| name == "metadata.json")
                .unwrap()
                .1,
        )
        .unwrap();
        assert!(metadata.get("thinodaPublicProjection").is_none());
        assert_eq!(
            import_with_report(fixture).unwrap(),
            (pre_x2_workbook(), Vec::new())
        );
    }

    fn strip_snapshot(entries: &mut [(String, Vec<u8>)]) {
        let (_, bytes) = entries
            .iter_mut()
            .find(|(name, _)| name == "metadata.json")
            .unwrap();
        let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
        metadata.as_object_mut().unwrap().remove("thinodaWorkbook");
        *bytes = serde_json::to_vec(&metadata).unwrap();
    }

    /// Imports only the public XMind content another application would see.
    fn public_report(book: &Workbook) -> (Workbook, Vec<ImportWarning>) {
        let mut entries = archive_entries(&export(book).unwrap());
        strip_snapshot(&mut entries);
        report(&entries)
    }

    #[test]
    fn public_projection_contains_only_reviewed_xmind_fields() {
        fn assert_keys(value: &Value, allowed: &[&str]) {
            let object = value.as_object().unwrap();
            assert!(
                object.keys().all(|key| allowed.contains(&key.as_str())),
                "unexpected keys: {:?}",
                object
                    .keys()
                    .filter(|key| !allowed.contains(&key.as_str()))
                    .collect::<Vec<_>>()
            );
        }
        fn check_topic(topic: &Value) {
            assert_keys(
                topic,
                &[
                    "id",
                    "class",
                    "title",
                    "children",
                    "markers",
                    "labels",
                    "branch",
                    "style",
                    "image",
                    "notes",
                    "href",
                    "structureClass",
                    "customWidth",
                    "numbering",
                    "extensions",
                    "boundaries",
                    "summaries",
                    "position",
                ],
            );
            assert_keys(&topic["style"], &["properties"]);
            for property in topic["style"]["properties"]
                .as_object()
                .into_iter()
                .flat_map(|properties| properties.keys())
            {
                assert!(
                    [
                        "svg:fill",
                        "fo:color",
                        "fo:font-size",
                        "fo:font-weight",
                        "fo:font-style",
                        "border-line-color",
                        "border-line-width",
                        "line-color",
                        "line-width",
                        "shape-class",
                        "line-class",
                        "fo:font-family",
                        "fo:text-align",
                        "fo:text-decoration",
                    ]
                    .contains(&property.as_str()),
                    "unexpected style property {property}"
                );
            }
            if let Some(markers) = topic["markers"].as_array() {
                for marker in markers {
                    assert_keys(marker, &["markerId"]);
                }
            }
            if !topic["image"].is_null() {
                assert_keys(&topic["image"], &["src", "width", "height", "align"]);
            }
            if !topic["position"].is_null() {
                assert_keys(&topic["position"], &["x", "y"]);
            }
            for child_topics in topic["children"]
                .as_object()
                .into_iter()
                .flat_map(|children| children.iter())
            {
                assert!(["attached", "detached", "summary"].contains(&child_topics.0.as_str()));
                for child in child_topics.1.as_array().unwrap() {
                    check_topic(child);
                }
            }
            for boundary in topic["boundaries"].as_array().into_iter().flatten() {
                assert_keys(boundary, &["id", "class", "range", "title", "style"]);
                assert_keys(&boundary["style"], &["properties"]);
            }
            for summary in topic["summaries"].as_array().into_iter().flatten() {
                assert_keys(summary, &["id", "class", "range", "topicId"]);
            }
        }

        let entries = archive_entries(&export(&pre_x2_workbook()).unwrap());
        let content: Value = serde_json::from_slice(
            &entries
                .iter()
                .find(|(name, _)| name == "content.json")
                .unwrap()
                .1,
        )
        .unwrap();
        for sheet in content.as_array().unwrap() {
            assert_keys(
                sheet,
                &[
                    "id",
                    "class",
                    "title",
                    "rootTopic",
                    "relationships",
                    "theme",
                ],
            );
            check_topic(&sheet["rootTopic"]);
            for relationship in sheet["relationships"].as_array().unwrap() {
                assert_keys(
                    relationship,
                    &[
                        "id",
                        "class",
                        "end1Id",
                        "end2Id",
                        "title",
                        "style",
                        "controlPoints",
                    ],
                );
                assert_keys(&relationship["style"], &["properties"]);
            }
        }
    }

    #[test]
    fn projection_2_fields_survive_without_the_private_snapshot() {
        let book = pre_x2_workbook();
        let entries = archive_entries(&export(&book).unwrap());
        let metadata: Value = serde_json::from_slice(
            &entries
                .iter()
                .find(|(name, _)| name == "metadata.json")
                .unwrap()
                .1,
        )
        .unwrap();
        assert_eq!(
            metadata["thinodaPublicProjection"],
            json!(PUBLIC_PROJECTION)
        );
        assert_eq!(import(&make_archive(&entries)).unwrap(), book);

        let mut sheet = book.sheets[0].clone();
        sheet.theme.background = Color::from_hex("#102030");
        sheet.theme.tapered_lines = false;
        let alpha = sheet.root_topic().children[0];
        let file =
            thinoda_core::attachments::insert(&mut sheet, alpha, "plan.pdf", vec![7; 9]).unwrap();
        // All root children have explicit sides so the unbalanced split is representable.
        for (index, child) in sheet.root_topic().children.clone().into_iter().enumerate() {
            sheet.topic_mut(child).unwrap().branch_side = Some(if index < 2 {
                thinoda_core::BranchSide::Right
            } else {
                thinoda_core::BranchSide::Left
            });
        }
        let source = Workbook::new(sheet);
        let (imported, warnings) = public_report(&source);
        assert!(warnings.is_empty(), "{warnings:?}");
        let (before, after) = (source.active(), imported.active());
        assert_eq!(after.theme.background, Color::from_hex("#102030"));
        assert!(!after.theme.tapered_lines);
        assert_eq!(after.root_topic().structure, Some(Structure::MapUnbalanced));
        let sides: Vec<_> = after
            .root_topic()
            .children
            .iter()
            .map(|child| after.topics[child].branch_side)
            .collect();
        let expected: Vec<_> = before
            .root_topic()
            .children
            .iter()
            .map(|child| before.topics[child].branch_side)
            .collect();
        assert_eq!(sides, expected);
        let alpha_after = after.root_topic().children[0];
        let topic = &after.topics[&alpha_after];
        assert_eq!(topic.style.width, Some(180.0));
        assert_eq!(topic.child_numbering, before.topics[&alpha].child_numbering);
        let attached = *topic.children.last().unwrap();
        assert_eq!(after.topics[&attached].title, before.topics[&file].title);
        let attachment = after.topics[&attached].attachment.clone().unwrap();
        assert_eq!(attachment.name, "plan.pdf");
        assert_eq!(
            after.resources.get(&attachment.resource).unwrap().bytes(),
            [7; 9]
        );
        assert_eq!(
            after.relationships[0].control_points,
            before.relationships[0].control_points
        );
        let summary = &after.summaries[0];
        assert_eq!(summary.range, ChildRange { start: 0, end: 1 });
        assert_eq!(
            after.boundaries[0].range,
            Some(ChildRange { start: 1, end: 2 })
        );
    }

    #[test]
    fn attachments_sharing_image_bytes_keep_one_resource() {
        let (mut book, bytes) = image_workbook();
        let sheet = &mut book.sheets[0];
        let root = sheet.root;
        thinoda_core::attachments::insert(sheet, root, "copy.png", bytes.clone()).unwrap();
        assert_eq!(import(&export(&book).unwrap()).unwrap(), book);
        let (imported, warnings) = public_report(&book);
        assert!(warnings.is_empty(), "{warnings:?}");
        let sheet = imported.active();
        assert_eq!(sheet.resources.entries.len(), 1);
        let child = *sheet.root_topic().children.last().unwrap();
        let attachment = sheet.topics[&child].attachment.clone().unwrap();
        assert_eq!(
            Some(attachment.resource.clone()),
            sheet.root_topic().image.clone()
        );
        assert_eq!(
            sheet.resources.get(&attachment.resource).unwrap().bytes(),
            bytes
        );
    }

    #[test]
    fn unknown_public_projection_falls_back_to_public_content() {
        let book = pre_x2_workbook();
        let mut entries = archive_entries(&export(&book).unwrap());
        let (_, bytes) = entries
            .iter_mut()
            .find(|(name, _)| name == "metadata.json")
            .unwrap();
        let mut metadata: Value = serde_json::from_slice(bytes).unwrap();
        metadata["thinodaPublicProjection"] = json!(PUBLIC_PROJECTION + 1);
        *bytes = serde_json::to_vec(&metadata).unwrap();
        let (imported, _) = report(&entries);
        // Private-only data (the hyperlink beside notes, labels) is still public; the
        // snapshot is ignored so per-topic ids are freshly allocated.
        assert_eq!(imported.active().root_topic().title, "Root");
        let alpha = imported.active().root_topic().children[0];
        assert_eq!(imported.active().topics[&alpha].style.width, Some(180.0));
    }

    #[test]
    fn generator_style_ranges_are_normalized() {
        let children = json!({"attached": [
            {"id": "a", "title": "A"}, {"id": "b", "title": "B"}, {"id": "c", "title": "C"}
        ], "summary": [
            {"id": "s1", "title": "Reversed"},
            {"id": "s2", "title": "Master"},
            {"id": "s3", "title": "Past the end"},
            {"id": "s4", "title": "Orphan"}
        ]});
        let (workbook, warnings) = content_report(json!({"rootTopic": {
            "id": "root", "title": "Root", "children": children,
            "summaries": [
                {"id": "x1", "range": "(2,0)", "topicId": "s1"},
                {"id": "x2", "range": "master", "topicId": "s2"},
                {"id": "x3", "range": "(1,10)", "topicId": "s3"},
                {"id": "x4", "range": "(0,0)", "topicId": "missing"}
            ],
            "boundaries": [
                {"id": "b1", "range": "(10,2)"},
                {"id": "b2", "range": "master"}
            ]
        }}));
        assert_eq!(warnings, vec![ImportWarning::Other]);
        let sheet = workbook.active();
        let ranges: Vec<_> = sheet
            .summaries
            .iter()
            .map(|summary| summary.range)
            .collect();
        assert_eq!(
            ranges,
            vec![
                ChildRange { start: 0, end: 2 },
                ChildRange { start: 0, end: 2 },
                ChildRange { start: 1, end: 2 },
            ]
        );
        let boundaries: Vec<_> = sheet
            .boundaries
            .iter()
            .map(|boundary| boundary.range)
            .collect();
        assert_eq!(
            boundaries,
            vec![Some(ChildRange { start: 2, end: 2 }), None]
        );
        // The summary topic without a valid bracket stays visible as a floating topic.
        let orphan = sheet
            .floating
            .iter()
            .find(|id| sheet.topics[id].title == "Orphan");
        assert!(orphan.is_some());
        sheet.validate().unwrap();
    }

    #[test]
    fn summaries_on_leaf_topics_become_floating_topics() {
        let (workbook, warnings) = content_report(json!({"rootTopic": {
            "id": "root", "title": "Root",
            "children": {"summary": [{"id": "s", "title": "Lonely"}]},
            "summaries": [{"id": "x", "range": "(0,0)", "topicId": "s"}]
        }}));
        assert_eq!(warnings, vec![ImportWarning::Other]);
        let sheet = workbook.active();
        assert!(sheet.summaries.is_empty());
        assert_eq!(sheet.floating.len(), 1);
        sheet.validate().unwrap();
    }

    #[test]
    fn style_aliases_and_approximations_are_classified() {
        let topic = |properties: Value| {
            content_report(json!({"rootTopic": {
                "title": "Root",
                "style": {"properties": properties}
            }}))
        };
        for (shape, expected) in [
            (
                "org.xmind.topicShape.ellipticrectangle",
                TopicShape::Ellipse,
            ),
            ("org.xmind.topicShape.ELLIPSE-RECT", TopicShape::Ellipse),
        ] {
            let (workbook, warnings) = topic(json!({"shape-class": shape}));
            assert!(warnings.is_empty(), "{shape}: {warnings:?}");
            assert_eq!(
                workbook.active().root_topic().style.shape,
                Some(expected),
                "{shape}"
            );
        }
        for (shape, expected) in [
            ("org.xmind.topicShape.circle", TopicShape::Ellipse),
            ("org.xmind.topicShape.hexagon", TopicShape::RoundedRect),
            ("org.xmind.topicShape.cloud", TopicShape::RoundedRect),
        ] {
            let (workbook, warnings) = topic(json!({"shape-class": shape}));
            assert_eq!(warnings, vec![ImportWarning::Formatting], "{shape}");
            assert_eq!(
                workbook.active().root_topic().style.shape,
                Some(expected),
                "{shape}"
            );
        }
        for (line, expected) in [
            ("org.xmind.branchConnection.bight", LineShape::Curve),
            ("org.xmind.branchConnection.fold", LineShape::Elbow),
            (
                "org.xmind.branchConnection.roundedfold",
                LineShape::RoundedElbow,
            ),
        ] {
            let (workbook, warnings) = topic(json!({"line-class": line}));
            assert!(warnings.is_empty(), "{line}: {warnings:?}");
            assert_eq!(
                workbook.active().root_topic().style.line_shape,
                Some(expected),
                "{line}"
            );
        }
        for (weight, bold) in [
            ("700", true),
            ("bolder", true),
            ("400", false),
            ("normal", false),
        ] {
            let (workbook, warnings) = topic(json!({"fo:font-weight": weight}));
            assert!(warnings.is_empty(), "{weight}: {warnings:?}");
            assert_eq!(
                workbook.active().root_topic().style.bold,
                Some(bold),
                "{weight}"
            );
        }
        let (_, warnings) = topic(json!({"fo:font-weight": "heavy-ish"}));
        assert_eq!(warnings, vec![ImportWarning::Formatting]);
        let (workbook, warnings) = topic(json!({"fo:font-weight": "inherited"}));
        assert!(warnings.is_empty());
        assert_eq!(workbook.active().root_topic().style.bold, None);
    }

    #[test]
    fn numbering_widths_and_control_points_import_from_public_xmind() {
        let (workbook, warnings) = content_report(json!({
            "rootTopic": {
                "id": "root", "title": "Root", "customWidth": 240,
                "numbering": {"numberFormat": "org.xmind.numbering.lowercase", "prependingNumbers": "none"},
                "children": {"attached": [{"id": "a", "title": "A", "customWidth": 0}, {"id": "b", "title": "B"}]}
            },
            "relationships": [{
                "id": "r", "end1Id": "a", "end2Id": "b",
                "controlPoints": {"0": {"x": 10, "y": -5}, "1": {"x": -3, "y": 4.5}}
            }]
        }));
        assert!(warnings.is_empty(), "{warnings:?}");
        let sheet = workbook.active();
        assert_eq!(sheet.root_topic().style.width, Some(240.0));
        assert_eq!(
            sheet.root_topic().child_numbering,
            Some(Numbering {
                format: NumberFormat::LowerAlpha,
                tiered: false,
                start: 1
            })
        );
        assert_eq!(
            sheet.topics[&sheet.root_topic().children[0]].style.width,
            None
        );
        assert_eq!(
            sheet.relationships[0].control_points,
            Some([Vec2::new(10.0, -5.0), Vec2::new(-3.0, 4.5)])
        );

        let (workbook, warnings) = content_report(json!({
            "rootTopic": {
                "id": "root", "title": "Root", "customWidth": 99999,
                "numbering": {"numberFormat": "org.xmind.numbering.unknown", "suffix": ")"},
                "children": {"attached": [{"id": "a", "title": "A"}, {"id": "b", "title": "B"}]}
            },
            "relationships": [{
                "id": "r", "end1Id": "a", "end2Id": "b",
                "controlPoints": {"0": {"amount": 0.3, "angle": 1.2}}
            }]
        }));
        assert_eq!(warnings, vec![ImportWarning::Formatting]);
        let sheet = workbook.active();
        assert_eq!(sheet.root_topic().style.width, None);
        assert_eq!(
            sheet
                .root_topic()
                .child_numbering
                .map(|numbering| numbering.format),
            Some(NumberFormat::Decimal)
        );
        assert_eq!(sheet.relationships[0].control_points, None);
    }

    #[test]
    fn marker_catalog_and_week_web_alias_import_without_loss() {
        let ids = [
            "task-pause",
            "flag-dark-blue",
            "star-dark-blue",
            "smiley-embarrass",
            "people-red",
            "people-orange",
            "people-yellow",
            "people-green",
            "people-blue",
            "people-purple",
            "people-dark-blue",
            "arrow-left",
            "arrow-right",
            "arrow-up",
            "arrow-down",
            "arrow-left-right",
            "arrow-up-down",
            "arrow-refresh",
            "week-web",
        ];
        let markers: Vec<_> = ids.iter().map(|id| json!({"markerId": id})).collect();
        let (workbook, warnings) =
            content_report(json!({"rootTopic": {"title": "Root", "markers": markers}}));
        assert!(warnings.is_empty(), "{warnings:?}");
        let imported = &workbook.active().root_topic().markers;
        assert_eq!(imported.len(), ids.len());
        assert_eq!(imported.last().map(String::as_str), Some("week-wed"));
        for id in &ids[..ids.len() - 1] {
            assert!(imported.iter().any(|marker| marker == id), "{id}");
        }
    }

    #[test]
    fn white_xmind_canvas_keeps_the_thinoda_background() {
        let (workbook, warnings) = content_report(json!({
            "rootTopic": {"title": "Root"},
            "theme": {"map": {"properties": {"svg:fill": "#ffffff", "line-tapered": "tapered"}}}
        }));
        assert!(warnings.is_empty());
        assert_eq!(workbook.active().theme.background, None);
        assert!(workbook.active().theme.tapered_lines);
    }
}
