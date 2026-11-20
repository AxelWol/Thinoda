use crate::{ExportOptions, ImportReport, ImportWarning, IoError, zip_entry};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{Cursor, Write},
};
use thinoda_core::{
    OutlineNode, Sheet, TopicId, Workbook,
    outline::{parse_indented_text, to_indented_text},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(crate) const MAX_XMINDMARK_BYTES: usize = 32 * 1024 * 1024;
const MAX_XMINDMARK_TOPICS: usize = 200_000;

pub(crate) fn archive(files: &[(&str, &[u8])]) -> Result<Vec<u8>, IoError> {
    let mut out = Cursor::new(Vec::new());
    let mut z = ZipWriter::new(&mut out);
    let opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in files {
        z.start_file(*name, opt)?;
        z.write_all(data)?;
    }
    z.finish()?;
    Ok(out.into_inner())
}
pub(crate) fn native_export(w: &Workbook) -> Result<Vec<u8>, IoError> {
    let version = crate::required_native_version(w);
    if version >= 3 {
        return crate::native_resources::export(w, version);
    }
    let content = serde_json::to_vec(w)?;
    let manifest = serde_json::to_vec(&json!({"format":"thinoda","version":2,"resources":[]}))?;
    archive(&[("manifest.json", &manifest), ("content.json", &content)])
}
pub(crate) fn native_import(bytes: &[u8]) -> Result<Workbook, IoError> {
    if bytes.starts_with(b"PK\x03\x04") {
        let mut z = ZipArchive::new(Cursor::new(bytes))?;
        let manifest_bytes = crate::zip_entry_with_limit(&mut z, "manifest.json", 1024 * 1024)?;
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
        let version = manifest
            .get("version")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        if version > crate::NATIVE_FORMAT_VERSION {
            return Err(IoError::NewerVersion(format!("{version}")));
        }
        if manifest
            .get("format")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v != "thinoda")
        {
            return Err(IoError::Invalid("manifest format is not thinoda".into()));
        }
        if version >= 3 {
            crate::native_resources::validate_directory(&z, bytes)?;
        }
        let mut book: Workbook = serde_json::from_slice(&zip_entry(&mut z, "content.json")?)?;
        if version < 36 && book.has_semantic_model() {
            return Err(IoError::NewerVersion(
                "socio-technical data requires native format version 36".into(),
            ));
        }
        if version < 37 && book.has_drawio_page_passthrough() {
            return Err(IoError::NewerVersion(
                "draw.io passthrough requires native format version 37".into(),
            ));
        }
        if version < 56 && book.has_drawio_group_source_ids() {
            return Err(IoError::NewerVersion(
                "draw.io group source IDs require native format version 56".into(),
            ));
        }
        if version < 55 && book.has_drawio_topic_source_ids() {
            return Err(IoError::NewerVersion(
                "draw.io topic source IDs require native format version 55".into(),
            ));
        }
        if version < 54 && book.has_drawio_zone_source_ids() {
            return Err(IoError::NewerVersion(
                "draw.io zone source IDs require native format version 54".into(),
            ));
        }
        if version < 53 && book.drawio.is_some() {
            return Err(IoError::NewerVersion(
                "draw.io document passthrough requires native format version 53".into(),
            ));
        }
        if version < 52 && book.has_card_baseline_overrides() {
            return Err(IoError::NewerVersion(
                "card baseline overrides require native format version 52".into(),
            ));
        }
        if version < 51 && book.has_rich_extra_connector_labels() {
            return Err(IoError::NewerVersion(
                "rich extra connector labels require native format version 51".into(),
            ));
        }
        if version < 50 && book.has_rich_connector_labels() {
            return Err(IoError::NewerVersion(
                "rich connector labels require native format version 50".into(),
            ));
        }
        if version < 49 && book.has_rich_labels() {
            return Err(IoError::NewerVersion(
                "rich labels require native format version 49".into(),
            ));
        }
        if version < 48 && book.has_label_styles() {
            return Err(IoError::NewerVersion(
                "label styles require native format version 48".into(),
            ));
        }
        if version < 47 && book.has_shape_styles() {
            return Err(IoError::NewerVersion(
                "shape styles require native format version 47".into(),
            ));
        }
        if version < 46 && book.has_tables() {
            return Err(IoError::NewerVersion(
                "tables require native format version 46".into(),
            ));
        }
        if version < 45 && book.has_layers() {
            return Err(IoError::NewerVersion(
                "layers require native format version 45".into(),
            ));
        }
        if version < 44 && book.has_object_data() {
            return Err(IoError::NewerVersion(
                "object data requires native format version 44".into(),
            ));
        }
        if version < 43 && book.has_connector_labels() {
            return Err(IoError::NewerVersion(
                "connector labels require native format version 43".into(),
            ));
        }
        if version < 42 && book.has_connector_edge_shapes() {
            return Err(IoError::NewerVersion(
                "connector edge shapes require native format version 42".into(),
            ));
        }
        if version < 41 && book.has_drawio_segment_routes() {
            return Err(IoError::NewerVersion(
                "draw.io segment routes require native format version 41".into(),
            ));
        }
        if version < 40 && book.has_drawio_edge_routes() {
            return Err(IoError::NewerVersion(
                "draw.io edge routes require native format version 40".into(),
            ));
        }
        if version < 39 && book.has_connector_line_jumps() {
            return Err(IoError::NewerVersion(
                "connector line jumps require native format version 39".into(),
            ));
        }
        if version < 38 && book.has_library_connector_markers() {
            return Err(IoError::NewerVersion(
                "draw.io library connector markers require native format version 38".into(),
            ));
        }
        if version >= 3 {
            crate::native_resources::hydrate(&mut z, &manifest_bytes, &mut book)?;
        } else if book.sheets.iter().any(|sheet| !sheet.resources.is_empty()) {
            return Err(IoError::Invalid(
                "embedded resources require native format version 3".into(),
            ));
        }
        if version < 4 && thinoda_core::notes::has_formatted_notes(&book) {
            return Err(IoError::Invalid(
                "formatted notes require native format version 4".into(),
            ));
        }
        if version < 5 && book.sheets.iter().any(|sheet| sheet.fim.is_some()) {
            return Err(IoError::Invalid(
                "FIM documents require native format version 5".into(),
            ));
        }
        if version < 6 && book.sheets.iter().any(crate::uses_board_semantics) {
            return Err(IoError::Invalid(
                "freeform board content requires native format version 6".into(),
            ));
        }
        if version < 7
            && book.sheets.iter().any(|sheet| {
                sheet
                    .zones
                    .iter()
                    .any(|zone| zone.background_color.is_some())
            })
        {
            return Err(IoError::Invalid(
                "independent Zone background colors require native format version 7".into(),
            ));
        }
        if version < 8 && book.has_inline_text() {
            return Err(IoError::Invalid(
                "inline card text requires native format version 8".into(),
            ));
        }
        if version < 9 && has_zone_objects(&book) {
            return Err(IoError::Invalid(
                "explicit Zone object membership requires native format version 9".into(),
            ));
        }
        if version < 10 && book.has_connector_waypoints() {
            return Err(IoError::Invalid(
                "connector waypoints require native format version 10".into(),
            ));
        }
        if version < 11 && book.has_connector_markers() {
            return Err(IoError::Invalid(
                "connector markers require native format version 11".into(),
            ));
        }
        if version < 12 && book.has_extended_connector_markers() {
            return Err(IoError::Invalid(
                "classic and thin connector markers require native format version 12".into(),
            ));
        }
        if version < 13 && book.has_connector_marker_sizes() {
            return Err(IoError::Invalid(
                "connector marker sizes require native format version 13".into(),
            ));
        }
        if version < 14 && book.has_connector_marker_fills() {
            return Err(IoError::Invalid(
                "connector marker fills require native format version 14".into(),
            ));
        }
        if version < 15 && book.has_diamond_connector_markers() {
            return Err(IoError::Invalid(
                "diamond connector markers require native format version 15".into(),
            ));
        }
        if version < 16 && book.has_oval_connector_markers() {
            return Err(IoError::Invalid(
                "oval connector markers require native format version 16".into(),
            ));
        }
        if version < 17 && book.has_half_circle_connector_markers() {
            return Err(IoError::Invalid(
                "half-circle connector markers require native format version 17".into(),
            ));
        }
        if version < 18 && book.has_bar_connector_markers() {
            return Err(IoError::Invalid(
                "bar connector markers require native format version 18".into(),
            ));
        }
        if version < 19 && book.has_shape_rounding() {
            return Err(IoError::Invalid(
                "shape rounding requires native format version 19".into(),
            ));
        }
        if version < 20 && book.has_rectangle_sides() {
            return Err(IoError::Invalid(
                "rectangle outline sides require native format version 20".into(),
            ));
        }
        if version < 21 && book.has_double_ellipses() {
            return Err(IoError::Invalid(
                "double ellipses require native format version 21".into(),
            ));
        }
        if version < 22 && book.has_fixed_connector_ports() {
            return Err(IoError::Invalid(
                "fixed connector ports require native format version 22".into(),
            ));
        }
        if version < 23 && book.has_drawio_rectangle_rounding() {
            return Err(IoError::Invalid(
                "standard draw.io rectangle rounding requires native format version 23".into(),
            ));
        }
        if version < 31 && book.has_scene_fill_last_resize() {
            return Err(IoError::NewerVersion(
                "fill-last stack resizing requires native format version 31".into(),
            ));
        }
        if version < 32 && book.has_scene_grow_parent_resize() {
            return Err(IoError::NewerVersion(
                "grow-parent stack resizing requires native format version 32".into(),
            ));
        }
        if version < 33 && book.has_scene_fit_parent_resize() {
            return Err(IoError::NewerVersion(
                "fit-parent stack resizing requires native format version 33".into(),
            ));
        }
        if version < 34 && book.has_scene_distribute_resize() {
            return Err(IoError::NewerVersion(
                "distributed stack spacing requires native format version 34".into(),
            ));
        }
        if version < 35 && book.has_scene_cross_axis_fill() {
            return Err(IoError::NewerVersion(
                "cross-axis stack fill requires native format version 35".into(),
            ));
        }
        if version < 30 && book.has_scene_layout_policies() {
            return Err(IoError::NewerVersion(
                "scene layout policies require native format version 30".into(),
            ));
        }
        if version < 29 && book.has_scene_collapse() {
            return Err(IoError::NewerVersion(
                "collapsed containers require native format version 29".into(),
            ));
        }
        if version < 27 && book.has_scene_hierarchy() {
            return Err(IoError::NewerVersion(
                "scene hierarchy requires native format version 27".into(),
            ));
        }
        if version < 26 && book.has_stencils() {
            return Err(IoError::Invalid(
                "stencil shapes require native format version 26".into(),
            ));
        }
        if version < 27 && book.has_stencil_definition_hashes() {
            return Err(IoError::Invalid(
                "pinned stencil definitions require native format version 27".into(),
            ));
        }
        if version < 28 && book.has_stencil_definition_closures() {
            return Err(IoError::Invalid(
                "stencil definition closures require native format version 28".into(),
            ));
        }
        if version < 25 && book.has_sankey() {
            return Err(IoError::Invalid(
                "Sankey flows require native format version 25".into(),
            ));
        }
        if version < 24 && book.has_final_states() {
            return Err(IoError::Invalid(
                "final states require native format version 24".into(),
            ));
        }
        book.hydrate_stencil_packages().map_err(IoError::Invalid)?;
        Ok(book)
    } else {
        let book: Workbook = serde_json::from_slice(bytes)?;
        if book.has_drawio_group_source_ids() {
            return Err(IoError::NewerVersion(
                "draw.io group source IDs require native format version 56".into(),
            ));
        }
        if book.has_drawio_topic_source_ids() {
            return Err(IoError::NewerVersion(
                "draw.io topic source IDs require native format version 55".into(),
            ));
        }
        if book.has_drawio_zone_source_ids() {
            return Err(IoError::NewerVersion(
                "draw.io zone source IDs require native format version 54".into(),
            ));
        }
        if book.drawio.is_some() {
            return Err(IoError::NewerVersion(
                "draw.io document passthrough requires native format version 53".into(),
            ));
        }
        if book.has_card_baseline_overrides() {
            return Err(IoError::NewerVersion(
                "card baseline overrides require native format version 52".into(),
            ));
        }
        if book.has_rich_extra_connector_labels() {
            return Err(IoError::NewerVersion(
                "rich extra connector labels require native format version 51".into(),
            ));
        }
        if book.has_rich_connector_labels() {
            return Err(IoError::NewerVersion(
                "rich connector labels require native format version 50".into(),
            ));
        }
        if book.has_rich_labels() {
            return Err(IoError::NewerVersion(
                "rich labels require native format version 49".into(),
            ));
        }
        if book.has_label_styles() {
            return Err(IoError::NewerVersion(
                "label styles require native format version 48".into(),
            ));
        }
        if book.has_shape_styles() {
            return Err(IoError::NewerVersion(
                "shape styles require native format version 47".into(),
            ));
        }
        if book.has_tables() {
            return Err(IoError::NewerVersion(
                "tables require native format version 46".into(),
            ));
        }
        if book.has_layers() {
            return Err(IoError::NewerVersion(
                "layers require native format version 45".into(),
            ));
        }
        if book.has_object_data() {
            return Err(IoError::NewerVersion(
                "object data requires native format version 44".into(),
            ));
        }
        if book.has_connector_labels() {
            return Err(IoError::NewerVersion(
                "connector labels require native format version 43".into(),
            ));
        }
        if book.has_connector_edge_shapes() {
            return Err(IoError::NewerVersion(
                "connector edge shapes require native format version 42".into(),
            ));
        }
        if book.has_drawio_segment_routes() {
            return Err(IoError::NewerVersion(
                "draw.io segment routes require native format version 41".into(),
            ));
        }
        if book.has_drawio_edge_routes() {
            return Err(IoError::NewerVersion(
                "draw.io edge routes require native format version 40".into(),
            ));
        }
        if book.has_connector_line_jumps() {
            return Err(IoError::NewerVersion(
                "connector line jumps require native format version 39".into(),
            ));
        }
        if book.has_library_connector_markers() {
            return Err(IoError::NewerVersion(
                "draw.io library connector markers require native format version 38".into(),
            ));
        }
        if book.has_semantic_model() {
            return Err(IoError::NewerVersion(
                "socio-technical data requires a versioned native archive".into(),
            ));
        }
        if book.has_scene_hierarchy() {
            return Err(IoError::NewerVersion(
                "scene hierarchy requires native format version 27".into(),
            ));
        }
        if book.has_stencils() {
            return Err(IoError::Invalid(
                "stencil shapes require native format version 26".into(),
            ));
        }
        if book.has_stencil_definition_hashes() {
            return Err(IoError::Invalid(
                "pinned stencil definitions require a versioned native archive".into(),
            ));
        }
        if book.has_stencil_definition_closures() {
            return Err(IoError::Invalid(
                "stencil definition closures require a versioned native archive".into(),
            ));
        }
        if book.has_sankey() {
            return Err(IoError::Invalid(
                "Sankey flows require native format version 25".into(),
            ));
        }
        if book.has_final_states() {
            return Err(IoError::Invalid(
                "final states require a versioned native archive".into(),
            ));
        }
        if book.has_fixed_connector_ports() {
            return Err(IoError::Invalid(
                "fixed connector ports require a versioned native archive".into(),
            ));
        }
        if book.has_double_ellipses() {
            return Err(IoError::Invalid(
                "double ellipses require a versioned native archive".into(),
            ));
        }
        if book.has_rectangle_sides() {
            return Err(IoError::Invalid(
                "rectangle outline sides require a versioned native archive".into(),
            ));
        }
        if book.has_shape_rounding() {
            return Err(IoError::Invalid(
                "shape rounding requires a versioned native archive".into(),
            ));
        }
        if book.has_connector_markers() {
            return Err(IoError::Invalid(
                "connector markers require a versioned native archive".into(),
            ));
        }
        if book.has_connector_waypoints() {
            return Err(IoError::Invalid(
                "connector waypoints require a versioned native archive".into(),
            ));
        }
        if has_zone_objects(&book) {
            return Err(IoError::Invalid(
                "explicit Zone object membership requires a versioned native archive".into(),
            ));
        }
        if book.has_inline_text() {
            return Err(IoError::Invalid(
                "inline card text requires a versioned native archive".into(),
            ));
        }
        Ok(book)
    }
}

fn has_zone_objects(book: &Workbook) -> bool {
    book.sheets
        .iter()
        .any(|sheet| sheet.zones.iter().any(|zone| !zone.objects.is_empty()))
}

pub(super) fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn root_outline(s: &Sheet) -> OutlineNode {
    thinoda_core::outline::to_outline(s, s.root)
}
pub(crate) fn text_export(w: &Workbook, o: &ExportOptions) -> Vec<u8> {
    let s = &w.sheets[o.sheet.unwrap_or(w.active_sheet).min(w.sheets.len() - 1)];
    to_indented_text(&[root_outline(s)]).into_bytes()
}
pub(crate) fn text_import(b: &[u8]) -> Result<Workbook, IoError> {
    let text = String::from_utf8_lossy(b);
    if text
        .lines()
        .any(|line| line.chars().take_while(|c| c.is_whitespace()).count() > 8000)
    {
        return Err(IoError::Invalid(
            "plain-text indentation exceeds 2000 levels".into(),
        ));
    }
    let mut nodes = parse_indented_text(&text);
    if nodes.is_empty() {
        nodes.push(OutlineNode::new("Central Topic"));
    }
    let root = nodes.remove(0);
    Ok(Workbook::new(thinoda_core::outline::sheet_from_outline(
        &root.title,
        &root,
    )))
}
pub(crate) fn markdown_export(w: &Workbook, o: &ExportOptions) -> Vec<u8> {
    fn note(source: &thinoda_core::notes::NoteContent, indent: usize, out: &mut String) {
        let padding = " ".repeat(indent);
        out.push('\n');
        if source.is_markdown() {
            for line in source.split('\n') {
                if !line.is_empty() {
                    out.push_str(&padding);
                    out.push_str(line);
                }
                out.push('\n');
            }
        } else {
            // A longer fence keeps literal Markdown/HTML and embedded fences inert.
            let longest = source
                .split(|character| character != '`')
                .map(str::len)
                .max()
                .unwrap_or(0);
            let fence = "`".repeat(longest.saturating_add(1).max(3));
            out.push_str(&padding);
            out.push_str(&fence);
            out.push('\n');
            for line in source.split('\n') {
                out.push_str(&padding);
                out.push_str(line);
                out.push('\n');
            }
            out.push_str(&padding);
            out.push_str(&fence);
            out.push('\n');
        }
        out.push('\n');
    }
    let sheet = &w.sheets[o.sheet.unwrap_or(w.active_sheet).min(w.sheets.len() - 1)];
    let mut out = String::new();
    let root = &sheet.topics[&sheet.root];
    out.push_str("# ");
    out.push_str(&root.title);
    out.push('\n');
    if let Some(notes) = &root.notes {
        note(notes, 0, &mut out);
    }
    fn render_children(sheet: &Sheet, id: TopicId, depth: usize, out: &mut String) {
        for child in &sheet.topics[&id].children {
            let topic = &sheet.topics[child];
            if depth == 1 {
                out.push_str("\n## ");
                out.push_str(&topic.title);
                out.push('\n');
            } else {
                out.push_str(&"  ".repeat(depth - 1));
                out.push_str("- ");
                out.push_str(&topic.title);
                out.push('\n');
            }
            if let Some(notes) = &topic.notes {
                note(notes, if depth == 1 { 0 } else { depth * 2 }, out);
            }
            render_children(sheet, *child, depth + 1, out);
        }
    }
    render_children(sheet, sheet.root, 1, &mut out);
    out.into_bytes()
}
pub(crate) fn markdown_import(b: &[u8]) -> Result<Workbook, IoError> {
    let text = String::from_utf8_lossy(b);
    let mut items: Vec<(usize, String)> = Vec::new();
    let mut first = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t == "---" || t.starts_with("<!--") {
            continue;
        }
        let hashes = line.chars().take_while(|c| *c == '#').count();
        let indent = line.len() - line.trim_start().len();
        if hashes > 2001 || indent / 2 > 2000 {
            return Err(IoError::Invalid(
                "Markdown nesting exceeds 2000 levels".into(),
            ));
        }
        if hashes > 0 && line[hashes..].starts_with(' ') {
            let title = line[hashes..].trim().to_string();
            if first.is_none() {
                first = Some(title);
            } else {
                items.push((hashes.saturating_sub(1), title));
            }
            continue;
        }
        let t = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| t.strip_prefix("+ "))
            .unwrap_or(t)
            .trim();
        let lev = indent / 2 + 1;
        items.push((lev, t.to_owned()));
    }
    let mut tree = thinoda_core::outline::build_tree(&items);
    let title = first.unwrap_or_else(|| {
        tree.first()
            .map(|x| x.title.clone())
            .unwrap_or_else(|| "Central Topic".into())
    });
    let root = if tree.iter().any(|n| n.title == title) {
        tree.remove(0)
    } else {
        OutlineNode::new(title).with_children(tree)
    };
    Ok(Workbook::new(thinoda_core::outline::sheet_from_outline(
        &root.title,
        &root,
    )))
}

#[derive(Default)]
struct XMindMarkNode {
    title: String,
    line: usize,
    indent: isize,
    children: Vec<usize>,
    groups: Vec<String>,
    links: Vec<XMindMarkLink>,
}

#[derive(Clone)]
struct XMindMarkLink {
    number: String,
    source: bool,
    title: Option<String>,
    line: usize,
}

#[derive(Default)]
struct XMindMarkGroup {
    title: Option<String>,
    title_line: Option<usize>,
    summary_node: Option<usize>,
    member_line: Option<usize>,
}

enum XMindMarkSuffix {
    Group(String),
    Link {
        number: String,
        source: bool,
        title: Option<String>,
    },
}

fn xmindmark_error(line: usize, message: &str) -> IoError {
    IoError::Invalid(format!("XMindMark line {line}: {message}"))
}

fn xmindmark_unescape(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\\'
            && chars
                .peek()
                .is_some_and(|next| matches!(next, '\\' | '[' | ']'))
        {
            output.push(chars.next().unwrap_or_default());
        } else {
            output.push(character);
        }
    }
    output
}

fn xmindmark_suffix(text: &str, line: usize) -> Result<(String, Vec<XMindMarkSuffix>), IoError> {
    fn escaped(text: &str, index: usize) -> bool {
        let slashes = text[..index]
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'\\')
            .count();
        slashes % 2 == 1
    }

    let mut end = text.trim_end().len();
    let mut suffixes = Vec::new();
    loop {
        let parsed = if text[..end].ends_with(')') {
            let mut candidate = end - 1;
            let mut found = None;
            while let Some(close_open) = text[..candidate].rfind("](") {
                if !escaped(text, close_open) {
                    let mut marker_end = close_open;
                    while let Some(marker_open) = text[..marker_end].rfind("[^") {
                        let number = &text[marker_open + 2..close_open];
                        if !escaped(text, marker_open)
                            && !number.is_empty()
                            && number.bytes().all(|byte| byte.is_ascii_digit())
                        {
                            found = Some((
                                marker_open,
                                XMindMarkSuffix::Link {
                                    number: number.to_owned(),
                                    source: true,
                                    title: (close_open + 2 < end - 1).then(|| {
                                        xmindmark_unescape(&text[close_open + 2..end - 1])
                                    }),
                                },
                            ));
                            break;
                        }
                        marker_end = marker_open;
                    }
                    if found.is_some() {
                        break;
                    }
                }
                candidate = close_open;
            }
            found
        } else {
            let Some(open) = text[..end].rfind('[') else {
                break;
            };
            if escaped(text, open) {
                break;
            }
            let marker = &text[open..end];
            let Some(number) = marker
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
            else {
                break;
            };
            let parsed = if let Some(number) = number.strip_prefix('^')
                && !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
            {
                Some(XMindMarkSuffix::Link {
                    number: number.to_owned(),
                    source: true,
                    title: None,
                })
            } else if let Some(group) = number.strip_prefix('B')
                && group.bytes().all(|byte| byte.is_ascii_digit())
            {
                Some(XMindMarkSuffix::Group(format!("B{group}")))
            } else if let Some(group) = number.strip_prefix('S')
                && group.bytes().all(|byte| byte.is_ascii_digit())
            {
                Some(XMindMarkSuffix::Group(format!("S{group}")))
            } else if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
                Some(XMindMarkSuffix::Link {
                    number: number.to_owned(),
                    source: false,
                    title: None,
                })
            } else {
                None
            };
            parsed.map(|suffix| (open, suffix))
        };
        let Some((open, parsed)) = parsed else {
            break;
        };
        suffixes.push(parsed);
        end = text[..open].trim_end().len();
    }
    if suffixes.len() > 64 {
        return Err(xmindmark_error(line, "topic has too many trailing markers"));
    }
    Ok((xmindmark_unescape(text[..end].trim_end()), suffixes))
}

fn xmindmark_group_directive(text: &str) -> Option<(&str, &str)> {
    let close = text.find(']')?;
    let key = text.get(1..close)?;
    if !text.starts_with('[') || !matches!(key.chars().next()?, 'B' | 'S') {
        return None;
    }
    if !key[1..].bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let rest = text.get(close + 1..)?.strip_prefix(':')?.trim_start();
    Some((key, rest))
}

fn xmindmark_indentation(line_text: &str, line: usize) -> Result<(usize, &str), IoError> {
    let mut columns = 0usize;
    let mut byte_offset = 0usize;
    for (offset, character) in line_text.char_indices() {
        match character {
            ' ' => columns += 1,
            '\t' => columns += 4,
            _ => {
                byte_offset = offset;
                break;
            }
        }
        byte_offset = offset + character.len_utf8();
    }
    if columns > 8000 {
        return Err(xmindmark_error(line, "indentation exceeds 2000 levels"));
    }
    Ok((columns, &line_text[byte_offset..]))
}

pub(crate) fn xmindmark_import(bytes: &[u8]) -> Result<Workbook, IoError> {
    if bytes.len() > MAX_XMINDMARK_BYTES {
        return Err(IoError::Invalid(
            "XMindMark input exceeds the 32 MiB limit".into(),
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| IoError::Invalid("XMindMark input is not valid UTF-8".into()))?;
    let mut nodes = vec![XMindMarkNode {
        indent: -1,
        ..Default::default()
    }];
    let mut stack = vec![0usize];
    let mut groups = BTreeMap::<(usize, String), XMindMarkGroup>::new();
    let mut root_seen = false;
    for (line_index, raw_line) in text.lines().enumerate() {
        let line = line_index + 1;
        if raw_line.trim().is_empty() {
            continue;
        }
        let (indent, content) = xmindmark_indentation(raw_line, line)?;
        if !root_seen {
            if indent != 0 || content.starts_with("- ") || content.starts_with("* ") {
                return Err(xmindmark_error(
                    line,
                    "the first non-empty line must be the central topic",
                ));
            }
            let (title, suffixes) = xmindmark_suffix(content, line)?;
            if title.is_empty() {
                return Err(xmindmark_error(line, "central topic title is empty"));
            }
            for suffix in suffixes {
                match suffix {
                    XMindMarkSuffix::Link {
                        number,
                        source,
                        title,
                    } => nodes[0].links.push(XMindMarkLink {
                        number,
                        source,
                        title,
                        line,
                    }),
                    XMindMarkSuffix::Group(_) => {
                        return Err(xmindmark_error(
                            line,
                            "the central topic cannot belong to a sibling group",
                        ));
                    }
                }
            }
            nodes[0].title = title;
            nodes[0].line = line;
            root_seen = true;
            continue;
        }

        while stack.len() > 1 && nodes[*stack.last().unwrap()].indent >= indent as isize {
            stack.pop();
        }
        let parent = *stack
            .last()
            .ok_or_else(|| xmindmark_error(line, "topic has no parent"))?;
        if nodes[parent].indent >= indent as isize {
            return Err(xmindmark_error(line, "invalid topic indentation"));
        }
        if let Some((key, value)) = xmindmark_group_directive(content) {
            let group_key = key.to_owned();
            let group_kind = key.chars().next().unwrap_or('B');
            let (title, suffixes) = xmindmark_suffix(value, line)?;
            let metadata = groups.entry((parent, group_key.clone())).or_default();
            if metadata.title_line.is_some() {
                return Err(xmindmark_error(line, "duplicate group title"));
            }
            metadata.title = Some(title);
            metadata.title_line = Some(line);
            let mut summary_node = None;
            if group_kind == 'S' {
                if nodes.len() >= MAX_XMINDMARK_TOPICS {
                    return Err(xmindmark_error(
                        line,
                        "topic count exceeds the supported limit",
                    ));
                }
                let id = nodes.len();
                nodes.push(XMindMarkNode {
                    title: metadata.title.clone().unwrap_or_default(),
                    line,
                    indent: indent as isize,
                    ..Default::default()
                });
                summary_node = Some(id);
                metadata.summary_node = Some(id);
            }
            for suffix in suffixes {
                match suffix {
                    XMindMarkSuffix::Link {
                        number,
                        source,
                        title,
                    } if summary_node.is_some() => {
                        nodes[summary_node.unwrap()].links.push(XMindMarkLink {
                            number,
                            source,
                            title,
                            line,
                        })
                    }
                    XMindMarkSuffix::Link { .. } => {
                        return Err(xmindmark_error(
                            line,
                            "relationships cannot attach to boundary titles",
                        ));
                    }
                    XMindMarkSuffix::Group(_) => {
                        return Err(xmindmark_error(
                            line,
                            "group title lines cannot carry group markers",
                        ));
                    }
                }
            }
            if let Some(summary_node) = summary_node {
                stack.push(summary_node);
            }
            continue;
        }

        let title = if let Some(title) = content.strip_prefix("- ") {
            title
        } else if let Some(title) = content.strip_prefix("* ") {
            title
        } else {
            return Err(xmindmark_error(
                line,
                "expected a '- ' or '* ' topic, or a [B]/[S] group title",
            ));
        };
        let (title, suffixes) = xmindmark_suffix(title.trim_start(), line)?;
        if title.is_empty() {
            return Err(xmindmark_error(line, "topic title is empty"));
        }
        if nodes.len() >= MAX_XMINDMARK_TOPICS {
            return Err(xmindmark_error(
                line,
                "topic count exceeds the supported limit",
            ));
        }
        let id = nodes.len();
        nodes.push(XMindMarkNode {
            title,
            line,
            indent: indent as isize,
            ..Default::default()
        });
        nodes[parent].children.push(id);
        for suffix in suffixes {
            match suffix {
                XMindMarkSuffix::Group(key) => {
                    if nodes[id].groups.contains(&key) {
                        return Err(xmindmark_error(line, "duplicate group marker"));
                    }
                    nodes[id].groups.push(key.clone());
                    groups
                        .entry((parent, key))
                        .or_default()
                        .member_line
                        .get_or_insert(line);
                }
                XMindMarkSuffix::Link {
                    number,
                    source,
                    title,
                } => nodes[id].links.push(XMindMarkLink {
                    number,
                    source,
                    title,
                    line,
                }),
            }
        }
        stack.push(id);
    }
    if !root_seen {
        return Err(xmindmark_error(1, "central topic is missing"));
    }
    for ((_, key), group) in &groups {
        if group.member_line.is_none() {
            return Err(xmindmark_error(
                group.title_line.unwrap_or(nodes[0].line),
                &format!("group [{key}] has no member topics"),
            ));
        }
    }

    let mut sheet = Sheet::new("XMindMark", &nodes[0].title);
    let mut topic_ids = vec![None; nodes.len()];
    topic_ids[0] = Some(sheet.root);
    fn build_children(
        sheet: &mut Sheet,
        parent_index: usize,
        parent_id: TopicId,
        nodes: &[XMindMarkNode],
        groups: &BTreeMap<(usize, String), XMindMarkGroup>,
        topic_ids: &mut [Option<TopicId>],
    ) -> Result<(), IoError> {
        for child_index in &nodes[parent_index].children {
            let id = sheet.add_child(parent_id, None, &nodes[*child_index].title);
            topic_ids[*child_index] = Some(id);
            build_children(sheet, *child_index, id, nodes, groups, topic_ids)?;
        }
        let group_keys: Vec<_> = groups
            .keys()
            .filter(|(parent, _)| *parent == parent_index)
            .map(|(_, key)| key.clone())
            .collect();
        for key in group_keys {
            let mut ranges = Vec::new();
            let mut index = 0;
            while index < nodes[parent_index].children.len() {
                let child = nodes[parent_index].children[index];
                if !nodes[child].groups.contains(&key) {
                    index += 1;
                    continue;
                }
                let start = index;
                while index + 1 < nodes[parent_index].children.len()
                    && nodes[nodes[parent_index].children[index + 1]]
                        .groups
                        .contains(&key)
                {
                    index += 1;
                }
                ranges.push(start..=index);
                index += 1;
            }
            let metadata = &groups[&(parent_index, key.clone())];
            let title = metadata
                .title
                .as_deref()
                .filter(|title| !title.is_empty())
                .unwrap_or("Summary");
            let kind = key.chars().next().unwrap_or('B');
            if kind == 'S' && ranges.len() != 1 {
                return Err(xmindmark_error(
                    metadata.member_line.unwrap_or(1),
                    "summary groups must be contiguous",
                ));
            }
            for range in ranges {
                let members: Vec<_> = range
                    .map(|index| topic_ids[nodes[parent_index].children[index]].unwrap())
                    .collect();
                if kind == 'B' {
                    let id = sheet.add_boundary(&members).map_err(|error| {
                        xmindmark_error(metadata.member_line.unwrap_or(1), &error.to_string())
                    })?;
                    sheet
                        .set_boundary_title(id, metadata.title.clone())
                        .map_err(|error| {
                            xmindmark_error(metadata.member_line.unwrap_or(1), &error.to_string())
                        })?;
                } else {
                    let summary = sheet.add_summary(&members, title).map_err(|error| {
                        xmindmark_error(metadata.member_line.unwrap_or(1), &error.to_string())
                    })?;
                    if let Some(summary_node) = metadata.summary_node {
                        topic_ids[summary_node] = Some(summary);
                        build_children(sheet, summary_node, summary, nodes, groups, topic_ids)?;
                    }
                }
            }
        }
        Ok(())
    }
    let root_id = sheet.root;
    build_children(&mut sheet, 0, root_id, &nodes, &groups, &mut topic_ids)?;

    let mut relationships =
        BTreeMap::<String, (Option<(TopicId, usize)>, Option<(TopicId, String, usize)>)>::new();
    for (node_index, node) in nodes.iter().enumerate() {
        let Some(topic_id) = topic_ids[node_index] else {
            continue;
        };
        for link in &node.links {
            let pair = relationships.entry(link.number.clone()).or_default();
            if link.source {
                if pair
                    .1
                    .replace((topic_id, link.title.clone().unwrap_or_default(), link.line))
                    .is_some()
                {
                    return Err(xmindmark_error(
                        link.line,
                        "duplicate relationship source marker",
                    ));
                }
            } else if pair.0.replace((topic_id, link.line)).is_some() {
                return Err(xmindmark_error(
                    link.line,
                    "duplicate relationship target marker",
                ));
            }
        }
    }
    for (number, (target, source)) in relationships {
        let (from, title, source_line, to, target_line) = match (target, source) {
            (Some((to, target_line)), Some((from, title, source_line))) => {
                (from, title, source_line, to, target_line)
            }
            (Some((_, line)), None) | (None, Some((_, _, line))) => {
                return Err(xmindmark_error(
                    line,
                    &format!("relationship [{number}] is missing its source or target"),
                ));
            }
            (None, None) => continue,
        };
        let relation = sheet
            .add_relationship(from, to)
            .map_err(|error| xmindmark_error(source_line.max(target_line), &error.to_string()))?;
        sheet
            .set_relationship_title(relation, &title)
            .map_err(|error| xmindmark_error(source_line, &error.to_string()))?;
    }
    Ok(Workbook::new(sheet))
}

fn xmindmark_escape(text: &str) -> Result<String, IoError> {
    if text.contains('\n') || text.contains('\r') {
        return Err(IoError::Invalid(
            "XMindMark cannot represent multiline topic or relationship titles".into(),
        ));
    }
    Ok(text
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]"))
}

struct XMindMarkWriter<'a> {
    sheet: &'a Sheet,
    group_tags: HashMap<TopicId, Vec<String>>,
    relation_tags: HashMap<TopicId, Vec<String>>,
    boundaries_by_parent: HashMap<TopicId, Vec<(String, Option<String>)>>,
    summaries_by_parent: HashMap<TopicId, Vec<(String, TopicId)>>,
    output: String,
}

impl XMindMarkWriter<'_> {
    fn render_annotations(&mut self, parent_id: TopicId, indent: usize) -> Result<(), IoError> {
        if let Some(boundaries) = self.boundaries_by_parent.get(&parent_id) {
            for (group, title) in boundaries {
                if let Some(title) = title {
                    self.output.push_str(&"    ".repeat(indent));
                    self.output.push('[');
                    self.output.push_str(group);
                    self.output.push_str("]: ");
                    self.output.push_str(&xmindmark_escape(title)?);
                    self.output.push('\n');
                }
            }
        }
        if let Some(summaries) = self.summaries_by_parent.get(&parent_id).cloned() {
            for (group, summary_id) in summaries {
                let (summary_title, children) = self
                    .sheet
                    .topic(summary_id)
                    .map(|summary| (summary.title.clone(), summary.children.clone()))
                    .ok_or_else(|| IoError::Invalid("summary topic is missing".into()))?;
                self.output.push_str(&"    ".repeat(indent));
                self.output.push('[');
                self.output.push_str(&group);
                self.output.push_str("]: ");
                self.output.push_str(&xmindmark_escape(&summary_title)?);
                if let Some(markers) = self.relation_tags.get(&summary_id)
                    && !markers.is_empty()
                {
                    self.output.push(' ');
                    self.output.push_str(&markers.join(""));
                }
                self.output.push('\n');
                for child in children {
                    self.render_topic(child, indent + 1, false)?;
                }
                self.render_annotations(summary_id, indent + 1)?;
            }
        }
        Ok(())
    }

    fn render_topic(&mut self, id: TopicId, depth: usize, is_root: bool) -> Result<(), IoError> {
        let topic = self
            .sheet
            .topic(id)
            .ok_or_else(|| IoError::Invalid("XMindMark topic is missing".into()))?;
        let title = xmindmark_escape(&topic.title)?;
        let mut markers = self.group_tags.get(&id).cloned().unwrap_or_default();
        markers.extend(self.relation_tags.get(&id).cloned().unwrap_or_default());
        let suffix = if markers.is_empty() {
            String::new()
        } else {
            format!(" {}", markers.join(""))
        };
        if is_root {
            self.output.push_str(&title);
            self.output.push_str(&suffix);
            self.output.push('\n');
        } else {
            self.output.push_str(&"    ".repeat(depth));
            self.output.push_str("- ");
            self.output.push_str(&title);
            self.output.push_str(&suffix);
            self.output.push('\n');
        }
        let child_depth = if is_root { depth } else { depth + 1 };
        for child in &topic.children {
            self.render_topic(*child, child_depth, false)?;
        }
        self.render_annotations(id, child_depth)
    }
}

pub(crate) fn xmindmark_export(
    workbook: &Workbook,
    options: &ExportOptions,
) -> Result<Vec<u8>, IoError> {
    let sheet_index = options
        .sheet
        .unwrap_or(workbook.active_sheet)
        .min(workbook.sheets.len().saturating_sub(1));
    let sheet = workbook
        .sheets
        .get(sheet_index)
        .ok_or_else(|| IoError::Invalid("XMindMark export requires a sheet".into()))?;
    let mut group_tags = HashMap::<TopicId, Vec<String>>::new();
    let mut boundaries_by_parent = HashMap::<TopicId, Vec<(String, Option<String>)>>::new();
    let mut summaries_by_parent = HashMap::<TopicId, Vec<(String, TopicId)>>::new();

    for boundary in &sheet.boundaries {
        let Some(range) = boundary.range else {
            continue;
        };
        let children = &sheet
            .topic(boundary.parent)
            .ok_or_else(|| IoError::Invalid("boundary parent is missing".into()))?
            .children;
        if range.end >= children.len() || range.start > range.end {
            return Err(IoError::Invalid(
                "XMindMark boundary range is invalid".into(),
            ));
        }
        let group = format!(
            "B{}",
            boundaries_by_parent
                .get(&boundary.parent)
                .map_or(1, |groups| groups.len() + 1)
        );
        for id in &children[range.start..=range.end] {
            group_tags
                .entry(*id)
                .or_default()
                .push(format!("[{group}]"));
        }
        boundaries_by_parent
            .entry(boundary.parent)
            .or_default()
            .push((group, boundary.title.clone()));
    }
    for summary in &sheet.summaries {
        let children = &sheet
            .topic(summary.parent)
            .ok_or_else(|| IoError::Invalid("summary parent is missing".into()))?
            .children;
        if summary.range.end >= children.len() || summary.range.start > summary.range.end {
            return Err(IoError::Invalid(
                "XMindMark summary range is invalid".into(),
            ));
        }
        let group = format!(
            "S{}",
            summaries_by_parent
                .get(&summary.parent)
                .map_or(1, |groups| groups.len() + 1)
        );
        for id in &children[summary.range.start..=summary.range.end] {
            group_tags
                .entry(*id)
                .or_default()
                .push(format!("[{group}]"));
        }
        summaries_by_parent
            .entry(summary.parent)
            .or_default()
            .push((group, summary.topic));
    }

    let mut relation_tags = HashMap::<TopicId, Vec<String>>::new();
    for (index, relationship) in sheet.relationships.iter().enumerate() {
        let number = (index + 1).to_string();
        let title = xmindmark_escape(&relationship.title)?;
        relation_tags
            .entry(relationship.from)
            .or_default()
            .push(if title.is_empty() {
                format!("[^{number}]")
            } else {
                format!("[^{number}]({title})")
            });
        relation_tags
            .entry(relationship.to)
            .or_default()
            .push(format!("[{number}]"));
    }
    let mut writer = XMindMarkWriter {
        sheet,
        group_tags,
        relation_tags,
        boundaries_by_parent,
        summaries_by_parent,
        output: String::new(),
    };
    writer.render_topic(sheet.root, 0, true)?;
    Ok(writer.output.into_bytes())
}

pub(crate) fn opml_export(w: &Workbook, o: &ExportOptions) -> Result<Vec<u8>, IoError> {
    let s = &w.sheets[o.sheet.unwrap_or(w.active_sheet).min(w.sheets.len() - 1)];
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><opml version=\"2.0\"><head><title>{}</title></head><body>",
        escape(&s.title)
    );
    fn rec(s: &Sheet, id: TopicId, out: &mut String, depth: usize) {
        if depth > 2000 {
            return;
        }
        let t = &s.topics[&id];
        out.push_str(&format!("<outline text=\"{}\"", escape(&t.title)));
        if let Some(n) = &t.notes {
            let note = escape(n)
                .replace('\r', "&#13;")
                .replace('\n', "&#10;")
                .replace('\t', "&#9;");
            out.push_str(&format!(" _note=\"{note}\""));
        }
        if let Some(l) = &t.hyperlink {
            out.push_str(&format!(" _link=\"{}\"", escape(l)));
        }
        if t.children.is_empty() {
            out.push_str("/>");
        } else {
            out.push('>');
            for c in &t.children {
                rec(s, *c, out, depth + 1);
            }
            out.push_str("</outline>");
        }
    }
    rec(s, s.root, &mut out, 0);
    out.push_str("</body></opml>");
    Ok(out.into_bytes())
}
pub(crate) fn freemind_export(w: &Workbook, o: &ExportOptions) -> Result<Vec<u8>, IoError> {
    let s = &w.sheets[o.sheet.unwrap_or(w.active_sheet).min(w.sheets.len() - 1)];
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><map version=\"1.0.1\">");
    fn rec(s: &Sheet, id: TopicId, out: &mut String, depth: usize) -> Result<(), IoError> {
        if depth > 2000 {
            return Ok(());
        }
        let t = &s.topics[&id];
        out.push_str(&format!(
            "<node TEXT=\"{}\" ID=\"ID_{}\"",
            escape(&t.title),
            id.0
        ));
        if t.collapsed {
            out.push_str(" FOLDED=\"true\"");
        }
        if let Some(l) = &t.hyperlink {
            out.push_str(&format!(" LINK=\"{}\"", escape(l)));
        }
        if t.parent.is_some() {
            out.push_str(" POSITION=\"right\"");
        }
        if t.children.is_empty() && t.notes.is_none() && t.markers.is_empty() {
            out.push_str("/>");
            return Ok(());
        }
        out.push('>');
        if let Some(n) = &t.notes {
            out.push_str("<richcontent TYPE=\"NOTE\">");
            if n.is_markdown() {
                let document = thinoda_core::notes::formatted_document(n).map_err(|reason| {
                    IoError::FreeMindNote {
                        topic: t.title.clone(),
                        reason,
                    }
                })?;
                // FreeMind's HTML renderer collapses whitespace. Do not rely on
                // unverified CSS to promise the same tab/space fidelity as Word.
                if document.paragraphs.iter().any(|paragraph| {
                    let text: String = paragraph.runs.iter().map(|run| run.text.as_str()).collect();
                    text.contains('\t') || text.contains("  ")
                }) {
                    return Err(IoError::FreeMindNote {
                        topic: t.title.clone(),
                        reason: thinoda_core::notes::NoteExportError::Unsupported,
                    });
                }
                out.push_str(&crate::freemind_notes::render(&document));
            } else {
                out.push_str(&format!("<html><body><p>{}</p></body></html>", escape(n)));
            }
            out.push_str("</richcontent>");
        }
        for m in &t.markers {
            let icon = match m.as_str() {
                _ if m.starts_with("priority-") => format!("full-{}", &m[9..]),
                "flag-red" => "flag".into(),
                "symbol-idea" => "idea".into(),
                "task-done" => "button_ok".into(),
                _ => String::new(),
            };
            if !icon.is_empty() {
                out.push_str(&format!("<icon BUILTIN=\"{icon}\"/>"));
            }
        }
        for c in &t.children {
            rec(s, *c, out, depth + 1)?;
        }
        out.push_str("</node>");
        Ok(())
    }
    rec(s, s.root, &mut out, 0)?;
    out.push_str("</map>");
    Ok(out.into_bytes())
}
#[derive(Default)]
struct XmlNode {
    title: String,
    notes: Option<thinoda_core::notes::NoteContent>,
    link: Option<String>,
    folded: bool,
    markers: Vec<String>,
    children: Vec<XmlNode>,
}
fn read_xml_tree(bytes: &[u8], opml: bool) -> Result<(String, Vec<XmlNode>, bool), IoError> {
    use quick_xml::{Reader, events::Event};
    let mut r = Reader::from_reader(bytes);
    r.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut roots = Vec::new();
    let mut title = String::new();
    let mut in_title = false;
    let mut formatted_notes = false;
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if name == "node" || name == "outline" {
                    let mut n = XmlNode::default();
                    for a in e.attributes().flatten() {
                        let k = a.key.as_ref().to_ascii_lowercase();
                        let v = a
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|e| IoError::Xml(e.to_string()))?
                            .to_string();
                        match k.as_str() {
                            "text" => n.title = v,
                            "folded" => n.folded = v == "true",
                            "link" => n.link = Some(v),
                            "_link" => n.link = Some(v),
                            "_note" => n.notes = Some(v.into()),
                            _ => {}
                        }
                    }
                    if stack.len() >= 2000 {
                        return Err(IoError::Invalid("XML nesting exceeds 2000 levels".into()));
                    }
                    stack.push(n);
                } else if name == "title" {
                    in_title = true;
                } else if name == "richcontent" {
                    for a in e.attributes().flatten() {
                        if a.key.as_ref().eq_ignore_ascii_case("TYPE") {
                            let v = a
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap_or_default();
                            if v.eq_ignore_ascii_case("NOTE") {
                                r.config_mut().trim_text(false);
                                let (notes, lossy) = crate::freemind_notes::read(
                                    &mut r,
                                    e.attributes().count() == 1,
                                )?;
                                r.config_mut().trim_text(true);
                                formatted_notes |= lossy;
                                if let Some(node) = stack.last_mut() {
                                    if let Some(previous) = node.notes.take() {
                                        let visible = |note: thinoda_core::notes::NoteContent| {
                                            if note.is_markdown() {
                                                thinoda_core::notes::plain_text_projection(&note)
                                                    .unwrap_or_else(|| note.to_string())
                                            } else {
                                                note.to_string()
                                            }
                                        };
                                        node.notes = Some(
                                            format!("{}\n{}", visible(previous), visible(notes))
                                                .into(),
                                        );
                                        formatted_notes = true;
                                    } else {
                                        node.notes = Some(notes);
                                    }
                                }
                                break;
                            }
                        }
                    }
                } else if name == "icon"
                    && let Some(n) = stack.last_mut()
                {
                    for a in e.attributes().flatten() {
                        if a.key.as_ref().eq_ignore_ascii_case("BUILTIN") {
                            let v = a
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap_or_default();
                            let m = match v.as_ref() {
                                _ if v.starts_with("full-") => format!("priority-{}", &v[5..]),
                                "flag" => "flag-red".into(),
                                "idea" => "symbol-idea".into(),
                                "button_ok" => "task-done".into(),
                                _ => String::new(),
                            };
                            if !m.is_empty() {
                                n.markers.push(m);
                            }
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if name == "node" || name == "outline" {
                    let mut n = XmlNode::default();
                    for a in e.attributes().flatten() {
                        let k = a.key.as_ref().to_ascii_lowercase();
                        let v = a
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .unwrap_or_default()
                            .to_string();
                        if k == "text" {
                            n.title = v;
                        } else if k == "_note" {
                            n.notes = Some(v.into());
                        } else if k == "link" || k == "_link" {
                            n.link = Some(v);
                        }
                    }
                    if let Some(p) = stack.last_mut() {
                        p.children.push(n)
                    } else {
                        roots.push(n)
                    }
                }
            }
            Ok(Event::Text(e)) => {
                let v = e
                    .xml_content(quick_xml::XmlVersion::Implicit1_0)
                    .to_string();
                if in_title {
                    title.push_str(&v);
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                let value = if let Some(character) = reference
                    .resolve_char_ref()
                    .map_err(|error| IoError::Xml(error.to_string()))?
                {
                    character.to_string()
                } else {
                    quick_xml::escape::resolve_predefined_entity(reference.as_ref())
                        .ok_or_else(|| IoError::Xml("unknown XML entity".into()))?
                        .to_owned()
                };
                if in_title {
                    title.push_str(&value);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if name == "title" {
                    in_title = false;
                }
                if (name == "node" || name == "outline")
                    && let Some(n) = stack.pop()
                {
                    if let Some(p) = stack.last_mut() {
                        p.children.push(n)
                    } else {
                        roots.push(n)
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(IoError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }
    if !opml && roots.is_empty() {
        return Err(IoError::Invalid("FreeMind map has no nodes".into()));
    }
    Ok((title, roots, formatted_notes))
}
fn xml_node_to_outline(n: XmlNode) -> OutlineNode {
    OutlineNode {
        title: n.title,
        notes: n.notes,
        link: n.link,
        markers: n.markers,
        children: n.children.into_iter().map(xml_node_to_outline).collect(),
        ..Default::default()
    }
}
pub(crate) fn opml_import(b: &[u8]) -> Result<Workbook, IoError> {
    let (sheet_title, mut outlines, _) = read_xml_tree(b, true)?;
    let mut root = if outlines.is_empty() {
        OutlineNode::new(if sheet_title.is_empty() {
            "Central Topic"
        } else {
            &sheet_title
        })
    } else {
        xml_node_to_outline(outlines.remove(0))
    };
    root.children
        .extend(outlines.into_iter().map(xml_node_to_outline));
    let title = if sheet_title.is_empty() {
        root.title.as_str()
    } else {
        sheet_title.as_str()
    };
    Ok(Workbook::new(thinoda_core::outline::sheet_from_outline(
        title, &root,
    )))
}
pub(crate) fn freemind_import_with_report(b: &[u8]) -> Result<ImportReport, IoError> {
    let (_, mut roots, formatted_notes) = read_xml_tree(b, false)?;
    let root = xml_node_to_outline(roots.remove(0));
    Ok(ImportReport {
        format: crate::Format::FreeMind,
        workbook: Workbook::new(thinoda_core::outline::sheet_from_outline(
            &root.title,
            &root,
        )),
        warnings: if formatted_notes {
            vec![ImportWarning::Formatting]
        } else {
            Vec::new()
        },
    })
}
pub(crate) fn textbundle_export(w: &Workbook, o: &ExportOptions) -> Result<Vec<u8>, IoError> {
    let md = markdown_export(w, o);
    let name = "mindmap";
    let info = serde_json::to_vec(
        &serde_json::json!({"version":2,"type":"net.daringfireball.markdown","transient":false,"creatorURL":"https://thinoda.app"}),
    )?;
    let a = format!("{name}.textbundle/info.json");
    let b = format!("{name}.textbundle/text.md");
    archive(&[(&a, &info), (&b, &md)])
}

pub(crate) fn textbundle_import(bytes: &[u8]) -> Result<ImportReport, IoError> {
    const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
    const MAX_MARKDOWN_BYTES: u64 = 16 * 1024 * 1024;

    if bytes.len() > MAX_ARCHIVE_BYTES || !bytes.starts_with(b"PK\x03\x04") {
        return Err(IoError::Invalid(
            "invalid or oversized TextBundle archive".into(),
        ));
    }

    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let names = crate::validate_document_archive(&mut archive)?;
    let info_paths: Vec<_> = names
        .iter()
        .filter(|name| name.as_str() == "info.json" || name.ends_with(".textbundle/info.json"))
        .cloned()
        .collect();
    if info_paths.len() != 1 {
        return Err(IoError::Invalid(
            "TextBundle must contain exactly one info.json".into(),
        ));
    }

    let info_path = &info_paths[0];
    let root = info_path
        .strip_suffix("info.json")
        .ok_or_else(|| IoError::Invalid("invalid TextBundle info path".into()))?;
    if !root.is_empty() && !root.ends_with(".textbundle/") {
        return Err(IoError::Invalid(
            "TextBundle info.json must be at the bundle root".into(),
        ));
    }
    let info: serde_json::Value = serde_json::from_slice(&crate::zip_entry_with_limit(
        &mut archive,
        info_path,
        1024 * 1024,
    )?)?;
    let version = info.get("version").and_then(serde_json::Value::as_u64);
    if !matches!(version, Some(1 | 2)) {
        return Err(IoError::Invalid("unsupported TextBundle version".into()));
    }
    let content_type = info
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| IoError::Invalid("TextBundle info.json is missing type".into()))?;
    if !matches!(
        content_type,
        "net.daringfireball.markdown" | "text/markdown"
    ) {
        return Err(IoError::Invalid(
            "TextBundle content type is not Markdown".into(),
        ));
    }

    let markdown_path = [format!("{root}text.md"), format!("{root}text.markdown")]
        .into_iter()
        .find(|path| names.contains(path))
        .ok_or_else(|| IoError::Invalid("TextBundle is missing its Markdown document".into()))?;
    let markdown = crate::zip_entry_with_limit(&mut archive, &markdown_path, MAX_MARKDOWN_BYTES)?;
    let text = std::str::from_utf8(&markdown)
        .map_err(|error| IoError::Invalid(format!("TextBundle Markdown is not UTF-8: {error}")))?;

    let mut warnings = BTreeSet::new();
    let expected_info = info_path.as_str();
    let expected_markdown = markdown_path.as_str();
    for path in &names {
        if path == expected_info || path == expected_markdown || path.ends_with('/') {
            continue;
        }
        if path.starts_with(&format!("{root}assets/")) {
            warnings.insert(ImportWarning::Resources);
        } else {
            warnings.insert(ImportWarning::Other);
        }
    }
    if text.contains("![") {
        warnings.insert(ImportWarning::Resources);
    }
    if text.contains("](") {
        warnings.insert(ImportWarning::Links);
    }
    if ["**", "__", "~~", "`", "\n>", "\n|"]
        .iter()
        .any(|marker| text.contains(marker))
    {
        warnings.insert(ImportWarning::Formatting);
    }

    Ok(ImportReport {
        format: crate::Format::TextBundle,
        workbook: markdown_import(text.as_bytes())?,
        warnings: warnings.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic_titles(sheet: &Sheet, parent: TopicId) -> Vec<String> {
        sheet
            .topic(parent)
            .unwrap()
            .children
            .iter()
            .map(|id| sheet.topic(*id).unwrap().title.clone())
            .collect()
    }

    #[test]
    fn plain_text_roots_roundtrip() {
        let w = text_import(b"Root\n\tChild\n").unwrap();
        assert_eq!(w.active().root_topic().title, "Root");
        assert_eq!(w.active().topic_count(), 2);
    }
    #[test]
    fn opml_uses_outline_as_central_topic() {
        let w = opml_import(
            b"<opml><head><title>Map</title></head><body><outline text=\"Root\"/></body></opml>",
        )
        .unwrap();
        assert_eq!(w.active().root_topic().title, "Root");
    }

    #[test]
    fn xmindmark_imports_groups_relationships_escapes_and_summary_children() {
        let source = r"Root \[map\]
- Alpha [B1][1]
- Beta [B1][^1](why \[yes\] and \[^2\])
- Before [S1]
- After [S1]
[B1]: Team
[S1]: Segment
    - Detail
";
        let workbook = xmindmark_import(source.as_bytes()).unwrap();
        let sheet = workbook.active();

        assert_eq!(sheet.root_topic().title, "Root [map]");
        assert_eq!(
            topic_titles(sheet, sheet.root),
            ["Alpha", "Beta", "Before", "After"]
        );
        assert_eq!(sheet.boundaries.len(), 1);
        assert_eq!(sheet.boundaries[0].title.as_deref(), Some("Team"));
        assert_eq!(sheet.summaries.len(), 1);
        let summary = sheet.topic(sheet.summaries[0].topic).unwrap();
        assert_eq!(summary.title, "Segment");
        assert_eq!(topic_titles(sheet, summary.id), ["Detail"]);
        assert_eq!(sheet.relationships.len(), 1);
        assert_eq!(sheet.relationships[0].title, "why [yes] and [^2]");
        assert_eq!(
            sheet.topic(sheet.relationships[0].from).unwrap().title,
            "Beta"
        );
        assert_eq!(
            sheet.topic(sheet.relationships[0].to).unwrap().title,
            "Alpha"
        );
    }

    #[test]
    fn xmindmark_tabs_indicate_four_space_indentation() {
        let workbook = xmindmark_import(b"Root\n- Parent\n\t- Child\n").unwrap();
        let sheet = workbook.active();
        let parent = sheet.topic(sheet.root).unwrap().children[0];
        assert_eq!(topic_titles(sheet, parent), ["Child"]);
    }

    #[test]
    fn xmindmark_is_registered_and_has_a_bounded_file_read_policy() {
        let path = std::path::Path::new("example.xmindmark");
        assert_eq!(
            crate::Format::from_path(path),
            Some(crate::Format::XMindMark)
        );
        assert!(crate::Format::XMindMark.can_import());
        assert!(crate::Format::XMindMark.can_export());
        assert_eq!(crate::import_read_limit(path), Some(MAX_XMINDMARK_BYTES));
    }

    #[test]
    fn xmindmark_reports_malformed_lines_with_source_line_numbers() {
        let error = xmindmark_import(b"Root\n- Good\n    Invalid\n").unwrap_err();
        assert!(error.to_string().contains("XMindMark line 3:"));
    }

    #[test]
    fn xmindmark_rejects_unpaired_and_duplicate_relationship_markers_at_their_lines() {
        let unpaired = xmindmark_import(b"Root\n- Orphan [1]\n").unwrap_err();
        assert!(unpaired.to_string().contains("XMindMark line 2:"));
        let duplicate = xmindmark_import(b"Root\n- One [1]\n- Two [1]\n- From [^1]\n").unwrap_err();
        assert!(duplicate.to_string().contains("XMindMark line 3:"));
    }

    #[test]
    fn xmindmark_roundtrips_relationships_without_titles() {
        let workbook = xmindmark_import(b"Root\n- From [^1]\n- To [1]\n").unwrap();
        assert_eq!(workbook.active().relationships.len(), 1);
        assert_eq!(workbook.active().relationships[0].title, "");
        let output = crate::export_bytes(
            &workbook,
            crate::Format::XMindMark,
            &ExportOptions::default(),
        )
        .unwrap();
        let reopened = xmindmark_import(&output).unwrap();
        assert_eq!(reopened.active().relationships.len(), 1);
        assert_eq!(reopened.active().relationships[0].title, "");
    }

    #[test]
    fn xmindmark_writer_is_deterministic_and_roundtrips_supported_annotations() {
        let source = r"Root \[map\]
- Alpha [B1][1]
- Beta [B1][^1](why \[yes\] and \[^2\])
- Before [S1]
- After [S1]
[B1]: Team
[S1]: Segment
    - Detail
";
        let workbook = xmindmark_import(source.as_bytes()).unwrap();
        let first = crate::export_bytes(
            &workbook,
            crate::Format::XMindMark,
            &ExportOptions::default(),
        )
        .unwrap();
        let second = crate::export_bytes(
            &workbook,
            crate::Format::XMindMark,
            &ExportOptions::default(),
        )
        .unwrap();
        assert_eq!(first, second);

        let reopened =
            crate::import_named_bytes(std::path::Path::new("example.xmindmark"), &first).unwrap();
        let sheet = reopened.active();
        assert_eq!(sheet.root_topic().title, "Root [map]");
        assert_eq!(sheet.boundaries[0].title.as_deref(), Some("Team"));
        assert_eq!(sheet.summaries.len(), 1);
        assert_eq!(
            sheet.topic(sheet.summaries[0].topic).unwrap().title,
            "Segment"
        );
        assert_eq!(sheet.relationships[0].title, "why [yes] and [^2]");
    }

    #[test]
    fn xmindmark_roundtrips_annotations_nested_under_summary_topics() {
        let source = r"Root
- One [B1][S1]
- Two [B1][S1]
[B1]: Outer
[S1]: Group
    - Alpha [B1]
    - Beta [B1]
    - Before [S2]
    - After [S2]
    [B1]: Inner
    [S2]: Nested
        - Leaf
";
        let workbook = xmindmark_import(source.as_bytes()).unwrap();
        let output = crate::export_bytes(
            &workbook,
            crate::Format::XMindMark,
            &ExportOptions::default(),
        )
        .unwrap();
        let reopened =
            crate::import_named_bytes(std::path::Path::new("nested.xmindmark"), &output).unwrap();
        let sheet = reopened.active();
        let group = sheet.topic(sheet.summaries[0].topic).unwrap();
        assert_eq!(group.title, "Group");
        assert_eq!(sheet.boundaries.len(), 2);
        let outer = sheet
            .boundaries
            .iter()
            .find(|boundary| boundary.title.as_deref() == Some("Outer"))
            .unwrap();
        let inner = sheet
            .boundaries
            .iter()
            .find(|boundary| boundary.title.as_deref() == Some("Inner"))
            .unwrap();
        assert_eq!(outer.parent, sheet.root);
        assert_eq!(inner.parent, group.id);
        assert_eq!(sheet.summaries.len(), 2);
        let nested = sheet
            .summaries
            .iter()
            .map(|summary| sheet.topic(summary.topic).unwrap())
            .find(|topic| topic.title == "Nested")
            .unwrap();
        assert_eq!(topic_titles(sheet, nested.id), ["Leaf"]);
    }

    #[test]
    fn xmindmark_exports_only_the_selected_sheet_and_reports_others() {
        let mut workbook = xmindmark_import(b"Root\n- Child\n").unwrap();
        workbook.sheets.push(Sheet::new("Other", "Second root"));
        let mut options = ExportOptions {
            sheet: Some(1),
            ..Default::default()
        };
        let output = crate::export_bytes(&workbook, crate::Format::XMindMark, &options).unwrap();
        let reopened = xmindmark_import(&output).unwrap();
        assert_eq!(reopened.active().root_topic().title, "Second root");
        assert!(
            crate::export_losses(&workbook, crate::Format::XMindMark, &options)
                .contains(&crate::ExportLoss::OtherSheetsDropped(1))
        );
        options.sheet = Some(0);
        assert_eq!(
            xmindmark_import(
                &crate::export_bytes(&workbook, crate::Format::XMindMark, &options,).unwrap()
            )
            .unwrap()
            .active()
            .root_topic()
            .title,
            "Root"
        );
    }

    #[test]
    fn xmindmark_reports_and_omits_master_boundaries() {
        let mut sheet = Sheet::new("One", "Root");
        let root = sheet.root;
        let child = sheet.add_child(root, None, "Child");
        sheet.add_boundary(&[child]).unwrap();
        sheet.boundaries[0].range = None;
        let workbook = Workbook::new(sheet);

        assert!(
            crate::export_losses(
                &workbook,
                crate::Format::XMindMark,
                &ExportOptions::default()
            )
            .contains(&crate::ExportLoss::BoundariesDropped(1))
        );
        let output = crate::export_bytes(
            &workbook,
            crate::Format::XMindMark,
            &ExportOptions::default(),
        )
        .unwrap();
        assert_eq!(xmindmark_import(&output).unwrap().active().topic_count(), 2);
    }
}
