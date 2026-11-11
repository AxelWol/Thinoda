//! draw.io (diagrams.net) shape libraries.
//!
//! draw.io stores a custom library as `<mxlibrary>` wrapping a JSON array. Each
//! entry holds a diagram fragment (`xml`, plain or
//! `base64(deflateRaw(encodeURIComponent(xml)))`) or an inline image (`data`),
//! with its default size and title. Entries are kept as plain `mxGraphModel`
//! XML so inserting one reuses the validated draw.io clipboard paste.

use crate::{IoError, drawio, drawio_clipboard};
use std::sync::OnceLock;

/// Largest library file accepted.
pub const MAX_LIBRARY_BYTES: usize = 8 * 1024 * 1024;
/// Most entries kept from one library.
pub const MAX_LIBRARY_ENTRIES: usize = 1_000;
/// Total decoded entry XML retained across imported libraries and scratchpad.
pub const MAX_DECODED_BYTES: usize = 32 * 1024 * 1024;
/// Input and expansion work allowed for one import batch or saved-state load.
pub const MAX_DECODE_WORK_BYTES: usize = 128 * 1024 * 1024;
/// Error reported when decoded or expansion limits are exhausted.
pub const DECODE_LIMIT_ERROR: &str = "draw.io library exceeds the decoded size or processing limit";
/// Longest entry or library title kept, in characters.
pub const MAX_TITLE_CHARS: usize = 200;
/// Largest entry width or height accepted.
const MAX_ENTRY_SIZE: f32 = 10_000.0;

/// One insertable library item.
#[derive(Debug, Clone, PartialEq)]
pub struct StencilEntry {
    pub title: String,
    /// Plain `mxGraphModel` XML of the fragment.
    pub xml: String,
    pub width: f32,
    pub height: f32,
}

/// A named collection of insertable items.
#[derive(Debug, Clone, PartialEq)]
pub struct StencilLibrary {
    /// Stable identifier for built-in libraries; empty for imported ones.
    pub id: String,
    pub title: String,
    pub entries: Vec<StencilEntry>,
}

/// A parsed library and the number of entries that could not be used.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLibrary {
    pub library: StencilLibrary,
    pub skipped: usize,
}

fn invalid(message: &str) -> IoError {
    IoError::Invalid(message.into())
}

/// `true` when `text` looks like a draw.io library file.
pub fn looks_like_library(text: &str) -> bool {
    library_body(text).is_some()
}

fn library_body(text: &str) -> Option<&str> {
    let text = text.trim_start_matches('\u{feff}').trim();
    let start = text.find("<mxlibrary")?;
    if !text[..start].trim().is_empty() && !text[..start].trim_start().starts_with("<?xml") {
        return None;
    }
    let after = &text[start + "<mxlibrary".len()..];
    let open_end = after.find('>')?;
    if after[..open_end].ends_with('/') {
        return Some("");
    }
    let body = &after[open_end + 1..];
    let end = body.rfind("</mxlibrary>")?;
    body[end + "</mxlibrary>".len()..]
        .trim()
        .is_empty()
        .then_some(&body[..end])
}

/// Parses a draw.io `<mxlibrary>` file. `title` names the library, usually
/// after its file. Unusable entries (such as remote images or malformed
/// fragments) are skipped and counted. Resource-limit violations reject the
/// library rather than retaining a misleading partial result.
pub fn parse_library(title: &str, text: &str) -> Result<ParsedLibrary, IoError> {
    let mut work_remaining = MAX_DECODE_WORK_BYTES;
    parse_library_with_budget(title, text, MAX_DECODED_BYTES, &mut work_remaining)
}

/// Parses one library against the caller's remaining retained and work budgets.
/// The caller deducts accepted entries from `decoded_available` between
/// libraries; failed attempts still consume `work_remaining`.
pub fn parse_library_with_budget(
    title: &str,
    text: &str,
    decoded_available: usize,
    work_remaining: &mut usize,
) -> Result<ParsedLibrary, IoError> {
    *work_remaining = (*work_remaining).min(MAX_DECODE_WORK_BYTES);
    charge_work(work_remaining, text.len())?;
    if text.len() > MAX_LIBRARY_BYTES {
        return Err(invalid("draw.io library exceeds the size limit"));
    }
    let body = library_body(text).ok_or_else(|| invalid("not a draw.io library"))?;
    let body = body.trim();
    let values: Vec<serde_json::Value> = if body.is_empty() {
        Vec::new()
    } else {
        match serde_json::from_str(body) {
            Ok(values) => values,
            Err(_) => {
                let json = quick_xml::escape::unescape(body)
                    .map_err(|_| invalid("draw.io library is not valid"))?;
                serde_json::from_str(&json)
                    .map_err(|_| invalid("draw.io library is not valid JSON"))?
            }
        }
    };
    let mut skipped = values.len().saturating_sub(MAX_LIBRARY_ENTRIES);
    let mut entries = Vec::new();
    let mut decoded_remaining = decoded_available.min(MAX_DECODED_BYTES);
    for value in values.into_iter().take(MAX_LIBRARY_ENTRIES) {
        match parse_entry(&value, decoded_remaining, work_remaining)? {
            Some(entry) => {
                decoded_remaining -= entry.xml.len();
                entries.push(entry);
            }
            None => skipped += 1,
        }
    }
    Ok(ParsedLibrary {
        library: StencilLibrary {
            id: String::new(),
            title: clamp_title(title),
            entries,
        },
        skipped,
    })
}

fn expansion_limit() -> IoError {
    invalid(DECODE_LIMIT_ERROR)
}

fn charge_work(work_remaining: &mut usize, bytes: usize) -> Result<(), IoError> {
    if bytes > *work_remaining {
        *work_remaining = 0;
        return Err(expansion_limit());
    }
    *work_remaining -= bytes;
    Ok(())
}

fn clamp_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TITLE_CHARS)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn size_field(value: &serde_json::Value, key: &str) -> Option<f32> {
    let size = match value.get(key)? {
        serde_json::Value::Number(number) => number.as_f64()?,
        serde_json::Value::String(text) => text.trim().parse().ok()?,
        _ => return None,
    } as f32;
    (size.is_finite() && size > 0.0).then_some(size.min(MAX_ENTRY_SIZE))
}

fn parse_entry(
    value: &serde_json::Value,
    decoded_remaining: usize,
    work_remaining: &mut usize,
) -> Result<Option<StencilEntry>, IoError> {
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(clamp_title)
        .unwrap_or_default();
    let width = size_field(value, "w").unwrap_or(80.0);
    let height = size_field(value, "h").unwrap_or(80.0);
    let max_decoded = decoded_remaining.min(drawio_clipboard::MAX_TEXT_BYTES);
    let xml = if let Some(xml) = value.get("xml").and_then(serde_json::Value::as_str) {
        let xml = xml.trim();
        let xml = if xml.starts_with('<') {
            charge_work(work_remaining, xml.len())?;
            if xml.len() > max_decoded {
                return Err(expansion_limit());
            }
            xml.to_owned()
        } else {
            match drawio::decode_compressed_bounded(xml, max_decoded, work_remaining) {
                Ok(xml) => xml,
                Err(drawio::BoundedDecodeError::Invalid) => return Ok(None),
                Err(drawio::BoundedDecodeError::Limit) => return Err(expansion_limit()),
            }
        };
        xml.trim_start_matches('\u{feff}').trim().to_owned()
    } else {
        let Some(data) = value.get("data").and_then(serde_json::Value::as_str) else {
            return Ok(None);
        };
        let escaped_len = data.bytes().fold(data.len(), |len, byte| {
            len.saturating_add(match byte {
                b'&' => 4,
                b'<' | b'>' => 3,
                b'\'' | b'"' => 5,
                _ => 0,
            })
        });
        if escaped_len > max_decoded {
            return Err(expansion_limit());
        }
        let Some(xml) = image_model(data, width, height) else {
            return Ok(None);
        };
        charge_work(work_remaining, xml.len())?;
        xml
    };
    if xml.len() > max_decoded {
        return Err(expansion_limit());
    }
    if !xml.starts_with("<mxGraphModel") {
        return Ok(None);
    }
    Ok(Some(StencilEntry {
        title,
        xml,
        width,
        height,
    }))
}

/// Wraps an inline image entry as a draw.io image cell. Only `data:` images
/// are accepted, so inserting an entry never fetches anything.
fn image_model(data: &str, width: f32, height: f32) -> Option<String> {
    let (media, payload) = data.trim().strip_prefix("data:")?.split_once(',')?;
    let media = media.strip_suffix(";base64").unwrap_or(media);
    if !media.to_ascii_lowercase().starts_with("image/")
        || payload.contains([';', '"', '<', '&'])
        || media.contains([';', '"', '<', '&'])
    {
        return None;
    }
    let style = format!(
        "shape=image;verticalLabelPosition=bottom;verticalAlign=top;imageAspect=0;aspect=fixed;image=data:{media},{payload}"
    );
    Some(model(&[Cell::Vertex {
        value: "",
        style: &style,
        width,
        height,
    }]))
}

/// Serialises a library in draw.io's format with plain-XML entries, so
/// draw.io and Thinoda can both open it.
pub fn encode_library(library: &StencilLibrary) -> String {
    let entries: Vec<_> = library
        .entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "xml": entry.xml,
                "w": entry.width,
                "h": entry.height,
                "title": entry.title,
            })
        })
        .collect();
    format!(
        "<mxlibrary>{}</mxlibrary>",
        serde_json::Value::Array(entries)
    )
}

/// Turns draw.io clipboard text (plain or URI-encoded `mxGraphModel`) into a
/// library entry, for adding copied or selected content to a library.
pub fn entry_from_clipboard(
    title: &str,
    text: &str,
    width: f32,
    height: f32,
) -> Result<StencilEntry, IoError> {
    let text = text.trim();
    if text.len() > drawio_clipboard::MAX_TEXT_BYTES.saturating_mul(3) {
        return Err(invalid("draw.io data exceeds the size limit"));
    }
    let xml = if text.starts_with('%') {
        drawio::percent_decode(text).ok_or_else(|| invalid("draw.io data is not valid"))?
    } else {
        text.to_owned()
    };
    if xml.len() > drawio_clipboard::MAX_TEXT_BYTES || !xml.starts_with("<mxGraphModel") {
        return Err(invalid("draw.io data is not valid"));
    }
    let size = |value: f32| {
        if value.is_finite() && value > 0.0 {
            value.min(MAX_ENTRY_SIZE)
        } else {
            80.0
        }
    };
    Ok(StencilEntry {
        title: clamp_title(title),
        xml,
        width: size(width),
        height: size(height),
    })
}

enum Cell<'a> {
    Vertex {
        value: &'a str,
        style: &'a str,
        width: f32,
        height: f32,
    },
    Edge {
        style: &'a str,
        width: f32,
        height: f32,
    },
}

fn model(cells: &[Cell<'_>]) -> String {
    use quick_xml::escape::escape;
    let mut xml = String::from(
        "<mxGraphModel adaptiveColors=\"none\"><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>",
    );
    for (index, cell) in cells.iter().enumerate() {
        let id = index + 2;
        match cell {
            Cell::Vertex {
                value,
                style,
                width,
                height,
            } => xml.push_str(&format!(
                "<mxCell id=\"{id}\" value=\"{}\" style=\"{}\" vertex=\"1\" parent=\"1\"><mxGeometry width=\"{width}\" height=\"{height}\" as=\"geometry\"/></mxCell>",
                escape(*value),
                escape(*style),
            )),
            Cell::Edge {
                style,
                width,
                height,
            } => xml.push_str(&format!(
                "<mxCell id=\"{id}\" value=\"\" style=\"{}\" edge=\"1\" parent=\"1\"><mxGeometry width=\"{width}\" height=\"{height}\" relative=\"1\" as=\"geometry\"><mxPoint y=\"{height}\" as=\"sourcePoint\"/><mxPoint x=\"{width}\" as=\"targetPoint\"/></mxGeometry></mxCell>",
                escape(*style),
            )),
        }
    }
    xml.push_str("</root></mxGraphModel>");
    xml
}

/// A built-in entry: English title, draw.io style, size and optional label.
type Spec = (&'static str, &'static str, f32, f32, &'static str);

const GENERAL: &[Spec] = &[
    (
        "Rectangle",
        "rounded=0;whiteSpace=wrap;html=1;",
        120.0,
        60.0,
        "",
    ),
    (
        "Rounded rectangle",
        "rounded=1;whiteSpace=wrap;html=1;",
        120.0,
        60.0,
        "",
    ),
    (
        "Text",
        "text;thinodaCard=text;html=1;whiteSpace=wrap;",
        60.0,
        30.0,
        "Text",
    ),
    (
        "Ellipse",
        "ellipse;whiteSpace=wrap;html=1;",
        120.0,
        80.0,
        "",
    ),
    (
        "Square",
        "whiteSpace=wrap;html=1;aspect=fixed;",
        80.0,
        80.0,
        "",
    ),
    (
        "Circle",
        "ellipse;whiteSpace=wrap;html=1;aspect=fixed;",
        80.0,
        80.0,
        "",
    ),
    (
        "Double ellipse",
        "shape=doubleEllipse;perimeter=ellipsePerimeter;html=1;fillColor=#ffffff;strokeColor=#000000;fontColor=#000000;",
        100.0,
        60.0,
        "",
    ),
    ("Diamond", "rhombus;whiteSpace=wrap;html=1;", 80.0, 80.0, ""),
    (
        "Parallelogram",
        "shape=parallelogram;perimeter=parallelogramPerimeter;whiteSpace=wrap;html=1;fixedSize=1;",
        120.0,
        60.0,
        "",
    ),
    (
        "Hexagon",
        "shape=hexagon;perimeter=hexagonPerimeter2;whiteSpace=wrap;html=1;fixedSize=1;",
        120.0,
        80.0,
        "",
    ),
    (
        "Triangle",
        "triangle;whiteSpace=wrap;html=1;",
        60.0,
        80.0,
        "",
    ),
    (
        "Cylinder",
        "shape=cylinder3;whiteSpace=wrap;html=1;boundedLbl=1;backgroundOutline=1;size=15;",
        60.0,
        80.0,
        "",
    ),
    (
        "Cloud",
        "shape=cloud;whiteSpace=wrap;html=1;",
        120.0,
        80.0,
        "",
    ),
    (
        "Document",
        "shape=document;whiteSpace=wrap;html=1;boundedLbl=1;",
        120.0,
        80.0,
        "",
    ),
    (
        "Chevron",
        "shape=step;perimeter=stepPerimeter;whiteSpace=wrap;html=1;fixedSize=1;",
        120.0,
        80.0,
        "",
    ),
    (
        "Trapezoid",
        "shape=trapezoid;perimeter=trapezoidPerimeter;whiteSpace=wrap;html=1;fixedSize=1;",
        120.0,
        60.0,
        "",
    ),
    (
        "Callout",
        "shape=callout;whiteSpace=wrap;html=1;perimeter=calloutPerimeter;",
        120.0,
        80.0,
        "",
    ),
    (
        "Plus",
        "shape=cross;whiteSpace=wrap;html=1;",
        80.0,
        80.0,
        "",
    ),
];

const GENERAL_EDGES: &[(&str, &str)] = &[
    ("Line", "endArrow=none;html=1;rounded=0;"),
    ("Arrow", "endArrow=classic;html=1;rounded=0;"),
    (
        "Bidirectional arrow",
        "endArrow=classic;startArrow=classic;html=1;rounded=0;",
    ),
];

const BASIC: &[Spec] = &[
    (
        "Star",
        "shape=mxgraph.basic.star;whiteSpace=wrap;html=1;",
        90.0,
        85.0,
        "",
    ),
    (
        "Pentagon",
        "shape=mxgraph.basic.pentagon;whiteSpace=wrap;html=1;",
        90.0,
        85.0,
        "",
    ),
    (
        "Octagon",
        "shape=mxgraph.basic.octagon2;whiteSpace=wrap;html=1;",
        90.0,
        90.0,
        "",
    ),
    (
        "Right triangle",
        "shape=mxgraph.basic.orthogonal_triangle;whiteSpace=wrap;html=1;",
        80.0,
        80.0,
        "",
    ),
    (
        "Hexagon",
        "shape=hexagon;perimeter=hexagonPerimeter2;whiteSpace=wrap;html=1;",
        90.0,
        80.0,
        "",
    ),
    (
        "Plus",
        "shape=cross;whiteSpace=wrap;html=1;",
        80.0,
        80.0,
        "",
    ),
    (
        "Rounded rectangle",
        "rounded=1;whiteSpace=wrap;html=1;arcSize=30;",
        100.0,
        60.0,
        "",
    ),
];

const FLOWCHART: &[Spec] = &[
    (
        "Process",
        "shape=rectangle;rounded=0;thinodaShape=process;whiteSpace=wrap;html=1;",
        100.0,
        60.0,
        "",
    ),
    (
        "Decision",
        "shape=mxgraph.flowchart.decision;whiteSpace=wrap;html=1;",
        100.0,
        80.0,
        "",
    ),
    (
        "Terminator",
        "shape=mxgraph.flowchart.terminator;whiteSpace=wrap;html=1;",
        100.0,
        50.0,
        "",
    ),
    (
        "Data",
        "shape=mxgraph.flowchart.data;whiteSpace=wrap;html=1;",
        100.0,
        60.0,
        "",
    ),
    (
        "Document",
        "shape=mxgraph.flowchart.document;whiteSpace=wrap;html=1;",
        100.0,
        70.0,
        "",
    ),
    (
        "Predefined process",
        "shape=mxgraph.flowchart.predefined_process;whiteSpace=wrap;html=1;",
        100.0,
        60.0,
        "",
    ),
    (
        "Manual input",
        "shape=mxgraph.flowchart.manual_input;whiteSpace=wrap;html=1;",
        100.0,
        60.0,
        "",
    ),
    (
        "Preparation",
        "shape=mxgraph.flowchart.preparation;whiteSpace=wrap;html=1;",
        100.0,
        60.0,
        "",
    ),
    (
        "Database",
        "shape=mxgraph.flowchart.database;whiteSpace=wrap;html=1;",
        60.0,
        80.0,
        "",
    ),
    (
        "Delay",
        "shape=mxgraph.flowchart.delay;whiteSpace=wrap;html=1;",
        100.0,
        60.0,
        "",
    ),
    (
        "Off-page connector",
        "shape=mxgraph.flowchart.off-page_reference;whiteSpace=wrap;html=1;",
        60.0,
        60.0,
        "",
    ),
    (
        "On-page reference",
        "shape=mxgraph.flowchart.on-page_reference;whiteSpace=wrap;html=1;",
        60.0,
        60.0,
        "",
    ),
    (
        "Extract",
        "shape=mxgraph.flowchart.extract_or_measurement;whiteSpace=wrap;html=1;",
        80.0,
        60.0,
        "",
    ),
    (
        "Merge",
        "shape=mxgraph.flowchart.merge_or_storage;whiteSpace=wrap;html=1;",
        80.0,
        60.0,
        "",
    ),
];

const ARROWS: &[Spec] = &[
    (
        "Arrow",
        "shape=singleArrow;whiteSpace=wrap;html=1;arrowWidth=0.4;arrowSize=0.4;",
        100.0,
        60.0,
        "",
    ),
    (
        "Double arrow",
        "shape=doubleArrow;whiteSpace=wrap;html=1;arrowWidth=0.4;arrowSize=0.3;",
        100.0,
        60.0,
        "",
    ),
    (
        "Chevron",
        "shape=step;perimeter=stepPerimeter;whiteSpace=wrap;html=1;fixedSize=1;",
        100.0,
        60.0,
        "",
    ),
    (
        "Triangle",
        "triangle;whiteSpace=wrap;html=1;direction=south;",
        80.0,
        60.0,
        "",
    ),
];

/// Built-in library identifiers, in display order.
pub const BUILTIN_IDS: [&str; 4] = ["general", "basic", "flowchart", "arrows"];

fn builtin(id: &str, title: &str, specs: &[Spec], edges: &[(&str, &str)]) -> StencilLibrary {
    let mut entries: Vec<_> = specs
        .iter()
        .map(|&(title, style, width, height, value)| StencilEntry {
            title: title.into(),
            xml: model(&[Cell::Vertex {
                value,
                style,
                width,
                height,
            }]),
            width,
            height,
        })
        .collect();
    entries.extend(edges.iter().map(|&(title, style)| StencilEntry {
        title: title.into(),
        xml: model(&[Cell::Edge {
            style,
            width: 80.0,
            height: 80.0,
        }]),
        width: 80.0,
        height: 80.0,
    }));
    StencilLibrary {
        id: id.into(),
        title: title.into(),
        entries,
    }
}

/// Thinoda's built-in libraries, modelled on draw.io's General, Basic,
/// Flowchart and Arrows sections. Every entry maps to a native board item.
/// Titles are English and meant to be localized by the host.
pub fn builtin_libraries() -> &'static [StencilLibrary] {
    static LIBRARIES: OnceLock<Vec<StencilLibrary>> = OnceLock::new();
    LIBRARIES.get_or_init(|| {
        vec![
            builtin(BUILTIN_IDS[0], "General", GENERAL, GENERAL_EDGES),
            builtin(BUILTIN_IDS[1], "Basic", BASIC, &[]),
            builtin(BUILTIN_IDS[2], "Flowchart", FLOWCHART, &[]),
            builtin(BUILTIN_IDS[3], "Arrows", ARROWS, &[]),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    fn compressed(xml: &str) -> String {
        let encoded = drawio_clipboard::encode_uri_component(xml);
        let deflated = miniz_oxide::deflate::compress_to_vec(encoded.as_bytes(), 6);
        base64::engine::general_purpose::STANDARD.encode(deflated)
    }

    fn library(entries: serde_json::Value) -> String {
        format!("<mxlibrary>{entries}</mxlibrary>")
    }

    const RECT: &str = "<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/><mxCell id=\"2\" value=\"A\" style=\"rounded=0;whiteSpace=wrap;html=1;\" vertex=\"1\" parent=\"1\"><mxGeometry width=\"120\" height=\"60\" as=\"geometry\"/></mxCell></root></mxGraphModel>";

    #[test]
    fn builtin_entries_insert_as_native_items_without_warnings() {
        let mut failures = Vec::new();
        for library in builtin_libraries() {
            assert!(!library.entries.is_empty(), "{}", library.id);
            for entry in &library.entries {
                let paste = drawio_clipboard::decode(&entry.xml)
                    .unwrap_or_else(|error| panic!("{}/{}: {error}", library.id, entry.title));
                if !paste.warnings.is_empty() || !paste.losses.is_empty() || paste.bounds.is_none()
                {
                    failures.push(format!(
                        "{}/{}: {:?} {:?}",
                        library.id, entry.title, paste.warnings, paste.losses
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn parses_plain_compressed_and_image_entries() {
        let png = base64::engine::general_purpose::STANDARD.encode([137, 80, 78, 71]);
        let zlib = base64::engine::general_purpose::STANDARD.encode(
            miniz_oxide::deflate::compress_to_vec_zlib(
                drawio_clipboard::encode_uri_component(RECT).as_bytes(),
                6,
            ),
        );
        let text = format!(
            "<mxlibrary>{}</mxlibrary>",
            serde_json::json!([
                {"xml": RECT, "w": 120, "h": 60, "title": "Plain"},
                {"xml": compressed(RECT), "w": "120", "h": "60", "title": "Packed", "aspect": "fixed"},
                {"data": format!("data:image/png;base64,{png}"), "w": 32, "h": 32, "title": "Icon"},
                {"xml": zlib, "title": "Zlib"},
                {"data": "https://example.com/remote.png", "w": 32, "h": 32},
                {"xml": "not a diagram", "w": 10, "h": 10},
            ])
        );
        let parsed = parse_library("Mine", &text).unwrap();
        assert_eq!(parsed.skipped, 2);
        let entries = &parsed.library.entries;
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].xml, RECT);
        assert_eq!(entries[1].xml, RECT);
        assert_eq!((entries[1].width, entries[1].height), (120.0, 60.0));
        assert!(entries[2].xml.contains("image=data:image/png,"));
        assert_eq!(entries[3].xml, RECT);
        assert_eq!(parsed.library.title, "Mine");
    }

    #[test]
    fn escaped_and_empty_libraries_parse() {
        let escaped = "<?xml version=\"1.0\"?>\n<mxlibrary>[{&quot;xml&quot;:&quot;&lt;mxGraphModel&gt;&lt;root/&gt;&lt;/mxGraphModel&gt;&quot;,&quot;w&quot;:10,&quot;h&quot;:10}]</mxlibrary>";
        let parsed = parse_library("Escaped", escaped).unwrap();
        assert_eq!(parsed.library.entries.len(), 1);
        assert!(
            parse_library("Empty", "<mxlibrary/>")
                .unwrap()
                .library
                .entries
                .is_empty()
        );
        assert!(
            parse_library("Empty", "<mxlibrary>[]</mxlibrary>")
                .unwrap()
                .library
                .entries
                .is_empty()
        );
    }

    #[test]
    fn rejects_non_libraries_and_oversized_files() {
        assert!(parse_library("x", "<mxfile/>").is_err());
        assert!(parse_library("x", "<mxlibrary>{}</mxlibrary>").is_err());
        assert!(parse_library("x", "<mxlibrary>[]</mxlibrary><extra/>").is_err());
        let huge = format!("<mxlibrary>[{}]</mxlibrary>", " ".repeat(MAX_LIBRARY_BYTES));
        assert!(parse_library("x", &huge).is_err());
        assert!(!looks_like_library("<mxGraphModel/>"));
    }

    #[test]
    fn limits_entries_titles_and_sizes() {
        let entry = serde_json::json!({"xml": RECT, "w": 1e9, "h": -5, "title": "x".repeat(500)});
        let values: Vec<_> = std::iter::repeat_n(entry, MAX_LIBRARY_ENTRIES + 3).collect();
        let text = format!(
            "<mxlibrary>{}</mxlibrary>",
            serde_json::Value::Array(values)
        );
        let parsed = parse_library("t\u{7}itle", &text).unwrap();
        assert_eq!(parsed.library.entries.len(), MAX_LIBRARY_ENTRIES);
        assert_eq!(parsed.skipped, 3);
        let first = &parsed.library.entries[0];
        assert_eq!(first.title.chars().count(), MAX_TITLE_CHARS);
        assert_eq!((first.width, first.height), (MAX_ENTRY_SIZE, 80.0));
        assert_eq!(parsed.library.title, "title");
    }

    #[test]
    fn decoded_and_work_budgets_are_cumulative() {
        let packed = compressed(RECT);
        let text = library(serde_json::json!([
            {"xml": RECT},
            {"xml": packed},
        ]));
        let inflated = drawio_clipboard::encode_uri_component(RECT).len();
        let required_work = text.len() + RECT.len() + inflated;

        let mut work = required_work;
        let parsed = parse_library_with_budget("two", &text, RECT.len() * 2, &mut work).unwrap();
        assert_eq!(parsed.library.entries.len(), 2);
        assert_eq!(work, 0);

        let mut work = required_work;
        assert!(parse_library_with_budget("two", &text, RECT.len() * 2 - 1, &mut work).is_err());
        let mut work = required_work - 1;
        assert!(parse_library_with_budget("two", &text, RECT.len() * 2, &mut work).is_err());
        assert_eq!(work, 0);
    }

    #[test]
    fn oversized_and_malformed_expansions_spend_the_work_budget() {
        let invalid = "x".repeat(64 * 1024);
        let packed = compressed(&invalid);
        let text = library(serde_json::json!([{"xml": packed}, {"xml": packed}]));
        let mut work = text.len() + invalid.len() * 2 - 1;
        assert!(parse_library_with_budget("invalid", &text, MAX_DECODED_BYTES, &mut work).is_err());
        assert_eq!(work, 0);

        let mut bytes = base64::engine::general_purpose::STANDARD
            .decode(compressed(&invalid))
            .unwrap();
        bytes.truncate(bytes.len() - 1);
        let truncated = base64::engine::general_purpose::STANDARD.encode(bytes);
        let mut available = MAX_DECODE_WORK_BYTES;
        assert_eq!(
            drawio::decode_compressed_bounded(&truncated, MAX_DECODED_BYTES, &mut available),
            Err(drawio::BoundedDecodeError::Invalid)
        );
        let charged = MAX_DECODE_WORK_BYTES - available;
        assert!(charged >= invalid.len());

        let text = library(serde_json::json!([{"xml": truncated}, {"xml": RECT}]));
        let mut work = text.len() + charged - 1;
        let result = parse_library_with_budget("truncated", &text, MAX_DECODED_BYTES, &mut work);
        assert!(result.is_err(), "charged {charged}, remaining {work}");
        assert_eq!(work, 0);

        let text = library(serde_json::json!([{"xml": compressed(&invalid)}]));
        let mut work = MAX_DECODE_WORK_BYTES;
        assert!(parse_library_with_budget("large", &text, 128, &mut work).is_err());
        assert!(MAX_DECODE_WORK_BYTES - work <= text.len() + 128 * 3);
    }

    #[test]
    fn decoded_limit_accepts_its_boundary_and_rejects_more() {
        let size = MAX_DECODED_BYTES / 4;
        let prefix = "<mxGraphModel>";
        let suffix = "</mxGraphModel>";
        let xml = format!(
            "{prefix}{}{suffix}",
            "x".repeat(size - prefix.len() - suffix.len())
        );
        assert_eq!(xml.len(), size);
        let packed = compressed(&xml);
        let entries = vec![serde_json::json!({"xml": packed}); 4];
        let parsed = parse_library("at limit", &library(serde_json::json!(entries))).unwrap();
        assert_eq!(
            parsed
                .library
                .entries
                .iter()
                .map(|entry| entry.xml.len())
                .sum::<usize>(),
            MAX_DECODED_BYTES
        );
        let entries = vec![serde_json::json!({"xml": packed}); 5];
        assert!(parse_library("over limit", &library(serde_json::json!(entries))).is_err());
    }

    #[test]
    fn an_entry_cannot_expand_past_the_clipboard_limit() {
        let prefix = "<mxGraphModel>";
        let suffix = "</mxGraphModel>";
        let xml = format!(
            "{prefix}{}{suffix}",
            "x".repeat(drawio_clipboard::MAX_TEXT_BYTES + 1 - prefix.len() - suffix.len())
        );
        let text = library(serde_json::json!([{"xml": compressed(&xml)}]));
        assert!(parse_library("too large", &text).is_err());
    }

    #[test]
    fn encoded_libraries_round_trip() {
        let library = StencilLibrary {
            id: String::new(),
            title: "Round trip".into(),
            entries: builtin_libraries()[0].entries.clone(),
        };
        let parsed = parse_library("Round trip", &encode_library(&library)).unwrap();
        assert_eq!(parsed.skipped, 0);
        assert_eq!(parsed.library, library);
    }

    #[test]
    fn clipboard_text_becomes_an_entry() {
        let encoded = drawio_clipboard::encode_uri_component(RECT);
        let entry = entry_from_clipboard("Copied", &encoded, 120.0, f32::NAN).unwrap();
        assert_eq!(entry.xml, RECT);
        assert_eq!((entry.width, entry.height), (120.0, 80.0));
        assert!(entry_from_clipboard("Bad", "<mxfile/>", 1.0, 1.0).is_err());
    }
}
