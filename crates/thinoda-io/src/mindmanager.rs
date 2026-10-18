//! Bounded MindManager conversion using the Application/2003 vocabulary.
use crate::{ImportReport, ImportWarning, IoError, zip_entry_with_limit};
use quick_xml::events::Event;
use std::{collections::BTreeSet, io::Cursor};
use thinoda_core::{Color, Sheet, TextAlign, Topic, TopicId, TopicShape, Workbook};

const APPLICATION: &[u8] = b"http://schemas.mindjet.com/MindManager/Application/2003";
const MAX_INPUT: usize = 64 * 1024 * 1024;
const MAX_XML: u64 = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_DEPTH: usize = 256;
const MAX_TOPICS: usize = 100_000;

fn invalid() -> IoError {
    IoError::Invalid("invalid or unsafe MindManager document".into())
}

struct Node {
    topic: Topic,
    children: Vec<Node>,
    properties: BTreeSet<Vec<u8>>,
}

pub(crate) fn import(bytes: &[u8], format: crate::Format) -> Result<ImportReport, IoError> {
    if bytes.len() > MAX_INPUT {
        return Err(invalid());
    }
    if (format == crate::Format::MindManager) != bytes.starts_with(b"PK") {
        return Err(invalid());
    }
    let mut warnings = BTreeSet::new();
    let xml = if bytes.starts_with(b"PK") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        if archive.len() > MAX_ENTRIES {
            return Err(invalid());
        }
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let entry_name = entry.name()?;
            let name = entry_name.as_ref();
            if name.is_empty()
                || name.starts_with('/')
                || name.contains(['\\', ':', '\0'])
                || name
                    .trim_end_matches('/')
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
                || !names.insert(name.to_lowercase())
                || entry
                    .unix_mode()
                    .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err(invalid());
            }
            total = total.checked_add(entry.size()).ok_or_else(invalid)?;
            if total > MAX_TOTAL || entry.size() > crate::MAX_ENTRY_SIZE {
                return Err(IoError::Invalid(
                    "MindManager archive exceeds size limit".into(),
                ));
            }
            // These are auxiliary package parts, not editable map content.
            if !entry.is_dir()
                && name != "Document.xml"
                && name != "Preview.png"
                && !name.starts_with("xsd/")
            {
                warnings.insert(ImportWarning::Resources);
            }
        }
        if archive.by_name("Document.xml")?.size() > MAX_XML {
            return Err(invalid());
        }
        zip_entry_with_limit(&mut archive, "Document.xml", MAX_XML)?
    } else {
        if bytes.len() as u64 > MAX_XML {
            return Err(invalid());
        }
        bytes.to_vec()
    };
    let mut report = parse(&xml, warnings)?;
    report.format = format;
    Ok(report)
}

fn parse(xml: &[u8], mut warnings: BTreeSet<ImportWarning>) -> Result<ImportReport, IoError> {
    use quick_xml::name::ResolveResult;
    let mut reader = quick_xml::NsReader::from_reader(xml);
    let mut path: Vec<Vec<u8>> = Vec::new();
    let mut stack: Vec<Node> = Vec::new();
    let mut roots = Vec::new();
    let mut topics = 0usize;
    let mut root_seen = false;
    let mut metadata = thinoda_core::model::Metadata::default();
    let mut sheet_title = None;
    loop {
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|e| IoError::Xml(e.to_string()))?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(event, Event::Empty(_));
                let local = e.local_name();
                let name = local.as_ref().as_bytes();
                let parent = path.last().map(Vec::as_slice);
                let application = match &namespace {
                    ResolveResult::Bound(ns) => ns.as_ref().as_bytes() == APPLICATION,
                    ResolveResult::Unbound => {
                        warnings.insert(ImportWarning::Other);
                        true // Older unqualified inputs remain importable, with disclosure.
                    }
                    ResolveResult::Unknown(_) => return Err(invalid()),
                };
                if path.is_empty() {
                    if root_seen || name != b"Map" {
                        return Err(invalid());
                    }
                    root_seen = true;
                    if !matches!(namespace, ResolveResult::Bound(_)) {
                        warnings.insert(ImportWarning::Other);
                    }
                }
                if path.len() >= MAX_DEPTH {
                    return Err(invalid());
                }
                // A foreign extension must not impersonate a structural topic or text.
                let in_extension = path.iter().any(|p| p.starts_with(b"!"));
                let supported = application && !in_extension;
                let mut represented = supported;
                let harmless = path.iter().any(|p| p.starts_with(b"~"))
                    || (supported
                        && matches!(
                            (name, parent),
                            (b"RevisionGroup", Some(b"Topic"))
                                | (b"MapViewGroup", Some(b"Map"))
                                | (b"Statistics" | b"Version", Some(b"DocumentGroup"))
                        ));
                if harmless {
                    for attr in e.attributes() {
                        attr.map_err(|e| IoError::Xml(e.to_string()))?
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|e| IoError::Xml(e.to_string()))?;
                    }
                    if !empty {
                        let mut entry = vec![b'~'];
                        entry.extend_from_slice(name);
                        path.push(entry);
                    }
                    continue;
                }
                let mut attributes = Vec::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| IoError::Xml(e.to_string()))?;
                    let value = attr
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|e| IoError::Xml(e.to_string()))?
                        .into_owned();
                    let key = attr.key.as_ref().as_bytes().to_vec();
                    if key != b"xmlns"
                        && !key.starts_with(b"xmlns:")
                        && !matches!(
                            key.as_slice(),
                            b"OId" | b"Gen" | b"Dirty" | b"xsi:schemaLocation"
                        )
                        && !matches!(
                            (name, key.as_slice()),
                            (b"Text", b"PlainText")
                                | (b"Hyperlink", b"Url" | b"FriendlyName")
                                | (b"NotesXhtmlData", b"PreviewPlainText")
                                | (
                                    b"Font",
                                    b"Bold"
                                        | b"Italic"
                                        | b"Underline"
                                        | b"Strikethrough"
                                        | b"Name"
                                        | b"Size"
                                        | b"Color"
                                )
                                | (b"Color", b"FillColor" | b"LineColor")
                                | (b"Text", b"TextAlignment" | b"ReadOnly")
                                | (b"Collapsed", b"Collapsed")
                                | (b"TopicViewGroup", b"ViewIndex")
                                | (b"SubTopicShape", b"SubTopicShape")
                                | (b"Task", b"TaskPriority" | b"TaskPercentage")
                                | (b"TextLabel", b"TextLabelName")
                                | (b"Creator", b"UserName")
                                | (b"Description", b"Comments")
                                | (b"Name", b"Name")
                        )
                    {
                        warnings.insert(feature(name));
                    }
                    attributes.push((key, value));
                }
                let attr = |key: &[u8]| {
                    attributes
                        .iter()
                        .find(|(k, _)| k == key)
                        .map(|(_, v)| v.clone())
                };
                if supported
                    && matches!(
                        name,
                        b"Text"
                            | b"Font"
                            | b"Color"
                            | b"Collapsed"
                            | b"SubTopicShape"
                            | b"NotesXhtmlData"
                    )
                    && let Some(node) = stack.last_mut()
                    && !node.properties.insert(name.to_vec())
                {
                    warnings.insert(feature(name));
                }
                if supported {
                    match name {
                        b"Map" if parent.is_none() => {}
                        b"OneTopic" if parent == Some(b"Map") => {}
                        b"SubTopics" if parent == Some(b"Topic") => {}
                        b"Topic" => {
                            if !matches!(parent, Some(b"OneTopic" | b"SubTopics")) {
                                return Err(invalid());
                            }
                            topics += 1;
                            if topics > MAX_TOPICS {
                                return Err(invalid());
                            }
                            stack.push(Node {
                                topic: Topic::new(TopicId(0), ""),
                                children: Vec::new(),
                                properties: BTreeSet::new(),
                            });
                        }
                        b"Text" if parent == Some(b"Topic") => {
                            let topic = stack.last_mut().ok_or_else(invalid)?;
                            topic.topic.title = attr(b"PlainText").ok_or_else(invalid)?;
                            if let Some(align) = attr(b"TextAlignment") {
                                topic.topic.style.align = Some(match align.as_str() {
                                    "urn:mindjet:Left" => TextAlign::Left,
                                    "urn:mindjet:Center" => TextAlign::Center,
                                    "urn:mindjet:Right" => TextAlign::Right,
                                    _ => {
                                        warnings.insert(ImportWarning::Formatting);
                                        TextAlign::Center
                                    }
                                });
                            }
                            if attr(b"ReadOnly").is_some_and(|v| v == "true" || v == "1") {
                                warnings.insert(ImportWarning::Other);
                            }
                        }
                        b"Font" if parent == Some(b"Text") => {
                            if let Some(node) = stack.last_mut() {
                                let style = &mut node.topic.style;
                                style.font_family = attr(b"Name");
                                style.font_size = attr(b"Size")
                                    .map(|v| v.parse::<f32>().map_err(|_| invalid()))
                                    .transpose()?;
                                if style.font_size.is_some_and(|size| {
                                    !size.is_finite() || size <= 0.0 || size > 10_000.0
                                }) {
                                    return Err(invalid());
                                }
                                if let Some(value) = attr(b"Color") {
                                    style.text_color = opaque_color(&value)?;
                                    if style.text_color.is_none() {
                                        warnings.insert(ImportWarning::Formatting);
                                    }
                                }
                                for (key, field) in [
                                    (b"Bold".as_slice(), &mut style.bold),
                                    (b"Italic".as_slice(), &mut style.italic),
                                    (b"Underline".as_slice(), &mut style.underline),
                                    (b"Strikethrough".as_slice(), &mut style.strikethrough),
                                ] {
                                    *field = attr(key).map(|v| parse_bool(&v)).transpose()?;
                                }
                            }
                        }
                        b"Color" if parent == Some(b"Topic") => {
                            if let Some(node) = stack.last_mut()
                                && let Some(value) = attr(b"FillColor")
                            {
                                node.topic.style.fill = opaque_color(&value)?;
                                if node.topic.style.fill.is_none() {
                                    warnings.insert(ImportWarning::Formatting);
                                }
                            }
                            if attr(b"LineColor").is_some() {
                                warnings.insert(ImportWarning::Formatting);
                            }
                        }
                        b"TopicViewGroup" if parent == Some(b"Topic") => {
                            if attr(b"ViewIndex").is_some_and(|value| value != "0") {
                                warnings.insert(ImportWarning::Formatting);
                            }
                        }
                        b"Collapsed" if parent == Some(b"TopicViewGroup") => {
                            if let Some(node) = stack.last_mut() {
                                node.topic.collapsed = attr(b"Collapsed")
                                    .map(|v| parse_bool(&v))
                                    .transpose()?
                                    .unwrap_or(false);
                            }
                        }
                        b"SubTopicShape" if parent == Some(b"Topic") => {
                            if let Some(node) = stack.last_mut()
                                && let Some(value) = attr(b"SubTopicShape")
                            {
                                node.topic.style.shape = parse_shape(&value);
                                if node.topic.style.shape.is_none() {
                                    warnings.insert(ImportWarning::Formatting);
                                }
                            }
                        }
                        b"Task" if parent == Some(b"Topic") => {
                            if let Some(node) = stack.last_mut() {
                                if let Some(value) = attr(b"TaskPriority") {
                                    if let Some(value) = value.strip_prefix("urn:mindjet:Prio")
                                        && ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
                                            .contains(&value)
                                    {
                                        node.topic.markers.push(format!("priority-{value}"));
                                    } else {
                                        warnings.insert(ImportWarning::Markers);
                                    }
                                }
                                if let Some(value) = attr(b"TaskPercentage") {
                                    if let Some(marker) = progress_marker(&value) {
                                        node.topic.markers.push(marker.into());
                                    } else {
                                        warnings.insert(ImportWarning::Markers);
                                    }
                                }
                            }
                        }
                        b"TextLabels" if parent == Some(b"Topic") => {}
                        b"TextLabel" if parent == Some(b"TextLabels") => {
                            if let Some(node) = stack.last_mut() {
                                node.topic
                                    .labels
                                    .push(attr(b"TextLabelName").ok_or_else(invalid)?);
                            }
                        }
                        b"DocumentGroup" if parent == Some(b"Map") => {}
                        b"Creator" if parent == Some(b"DocumentGroup") => {
                            metadata.creator = attr(b"UserName").unwrap_or_default();
                        }
                        b"Description" if parent == Some(b"DocumentGroup") => {
                            metadata.description = attr(b"Comments");
                        }
                        b"Name" if parent == Some(b"Map") => sheet_title = attr(b"Name"),
                        b"Hyperlink" => {
                            if let Some(topic) = stack.last_mut() {
                                if let Some(url) = attr(b"Url") {
                                    if topic.topic.hyperlink.is_some()
                                        || !["https://", "http://", "mailto:"].iter().any(
                                            |scheme| url.to_ascii_lowercase().starts_with(scheme),
                                        )
                                    {
                                        warnings.insert(ImportWarning::Links);
                                    } else {
                                        topic.topic.hyperlink = Some(url);
                                    }
                                } else {
                                    warnings.insert(ImportWarning::Links);
                                }
                            }
                        }
                        b"NotesGroup" if parent == Some(b"Topic") => {}
                        b"NotesXhtmlData" if parent == Some(b"NotesGroup") => {
                            if let Some(topic) = stack.last_mut() {
                                topic.topic.notes = attr(b"PreviewPlainText");
                            }
                            if !empty && stack.last().is_none_or(|node| node.topic.notes.is_none())
                            {
                                warnings.insert(ImportWarning::Notes);
                            }
                        }
                        _ => {
                            represented = false;
                            warnings.insert(
                                if path.iter().any(|p| p.as_slice() == b"NotesXhtmlData") {
                                    ImportWarning::Notes
                                } else {
                                    feature(name)
                                },
                            );
                        }
                    }
                } else {
                    warnings.insert(if path.iter().any(|p| p.as_slice() == b"NotesXhtmlData") {
                        ImportWarning::Notes
                    } else {
                        feature(name)
                    });
                }
                if empty {
                    if represented && name == b"Topic" {
                        finish_topic(&mut stack, &mut roots)?;
                    }
                } else {
                    let mut entry = name.to_vec();
                    if !represented {
                        entry.insert(0, b'!');
                    }
                    path.push(entry);
                }
            }
            Event::End(e) => {
                let name = path.pop().ok_or_else(invalid)?;
                if name == b"Topic" && e.local_name().as_ref() == "Topic" {
                    finish_topic(&mut stack, &mut roots)?;
                }
            }
            Event::DocType(_) => return Err(invalid()),
            Event::Text(e) => {
                if path.iter().any(|p| p.starts_with(b"~")) {
                    continue;
                }
                if !e.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) {
                    if path.is_empty() {
                        return Err(invalid());
                    }
                    if path.iter().any(|p| p.as_slice() == b"NotesXhtmlData") {
                        if stack.last().is_none_or(|node| node.topic.notes.is_none()) {
                            warnings.insert(ImportWarning::Notes);
                        }
                    } else {
                        warnings.insert(ImportWarning::Other);
                    }
                }
            }
            Event::GeneralRef(ref entity) => {
                if path.is_empty()
                    || (!matches!(entity.as_ref(), "amp" | "lt" | "gt" | "apos" | "quot")
                        && entity.resolve_char_ref().map_err(|_| invalid())?.is_none())
                {
                    return Err(invalid());
                }
                if !path.iter().any(|p| p.starts_with(b"~")) {
                    warnings.insert(ImportWarning::Notes);
                }
            }
            Event::CData(_) => {
                if path.is_empty() {
                    return Err(invalid());
                }
                if !path.iter().any(|p| p.starts_with(b"~")) {
                    warnings.insert(if path.iter().any(|p| p.as_slice() == b"NotesXhtmlData") {
                        ImportWarning::Notes
                    } else {
                        ImportWarning::Other
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !path.is_empty() || !stack.is_empty() || roots.is_empty() {
        return Err(invalid());
    }
    let sheets = roots
        .into_iter()
        .map(|node| {
            let mut sheet = Sheet::new(
                sheet_title
                    .clone()
                    .unwrap_or_else(|| node.topic.title.clone()),
                "",
            );
            let root = sheet.root;
            insert_node(&mut sheet, root, node);
            sheet
        })
        .collect();
    Ok(ImportReport {
        format: crate::Format::MindManager,
        workbook: Workbook {
            sheets,
            metadata,
            ..Workbook::default()
        },
        warnings: warnings.into_iter().collect(),
    })
}

fn finish_topic(stack: &mut Vec<Node>, roots: &mut Vec<Node>) -> Result<(), IoError> {
    let topic = stack.pop().ok_or_else(invalid)?;
    if let Some(parent) = stack.last_mut() {
        parent.children.push(topic);
    } else {
        roots.push(topic);
    }
    Ok(())
}

fn feature(name: &[u8]) -> ImportWarning {
    match name {
        b"NotesGroup" | b"NotesXhtmlData" => ImportWarning::Notes,
        b"Hyperlink" => ImportWarning::Links,
        b"OneImage" | b"Image" | b"Images" | b"ImageGroup" | b"Attachment" | b"Attachments"
        | b"AttachmentGroup" => ImportWarning::Resources,
        b"Icon" | b"Icons" | b"IconsGroup" | b"Task" | b"TaskGroup" | b"Marker" | b"Markers"
        | b"MarkersGroup" => ImportWarning::Markers,
        b"Font" | b"Color" | b"Shape" | b"SubTopicShape" | b"Style" | b"Theme" | b"Background"
        | b"TopicViewGroup" | b"ViewGroup" => ImportWarning::Formatting,
        _ => ImportWarning::Other,
    }
}

fn insert_node(sheet: &mut Sheet, id: TopicId, node: Node) {
    let parent = sheet.topics[&id].parent;
    let mut topic = node.topic;
    topic.id = id;
    topic.parent = parent;
    sheet.topics.insert(id, topic);
    for child in node.children {
        let child_id = sheet.add_child(id, None, "");
        insert_node(sheet, child_id, child);
    }
}

fn parse_bool(value: &str) -> Result<bool, IoError> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(invalid()),
    }
}

fn parse_shape(value: &str) -> Option<TopicShape> {
    Some(match value {
        "urn:mindjet:None" => TopicShape::None,
        "urn:mindjet:Line" => TopicShape::Underline,
        "urn:mindjet:Rectangle" => TopicShape::Rect,
        "urn:mindjet:RoundedRectangle" => TopicShape::RoundedRect,
        "urn:mindjet:Oval" => TopicShape::Ellipse,
        "urn:mindjet:Diamond" => TopicShape::Diamond,
        "urn:mindjet:Capsule" => TopicShape::Capsule,
        _ => return None,
    })
}

fn shape(value: TopicShape) -> &'static str {
    match value {
        TopicShape::None => "urn:mindjet:None",
        TopicShape::Underline => "urn:mindjet:Line",
        TopicShape::Rect => "urn:mindjet:Rectangle",
        TopicShape::RoundedRect => "urn:mindjet:RoundedRectangle",
        TopicShape::Ellipse => "urn:mindjet:Oval",
        TopicShape::Diamond => "urn:mindjet:Diamond",
        TopicShape::Capsule => "urn:mindjet:Capsule",
    }
}

fn progress_marker(value: &str) -> Option<&'static str> {
    Some(match value {
        "0" => "task-start",
        "25" => "task-quarter",
        "50" => "task-half",
        "75" => "task-3quar",
        "100" => "task-done",
        _ => return None,
    })
}

fn progress_value(marker: &str) -> Option<&'static str> {
    Some(match marker {
        "task-start" => "0",
        "task-quarter" => "25",
        "task-half" => "50",
        "task-3quar" => "75",
        "task-done" => "100",
        _ => return None,
    })
}

/// Unsupported workbook fields require explicit conversion consent.
pub(crate) fn export(
    book: &Workbook,
    options: &crate::ExportOptions,
    allow_lossy: bool,
) -> Result<Vec<u8>, IoError> {
    let sheet = book
        .sheets
        .get(options.sheet.unwrap_or(book.active_sheet))
        .ok_or_else(invalid)?;
    let mut lossy = (book.sheets.len() > 1 && options.sheet.is_none())
        || book.metadata.created.is_some()
        || book.metadata.modified.is_some()
        || !sheet.floating.is_empty()
        || !sheet.relationships.is_empty()
        || !sheet.boundaries.is_empty()
        || !sheet.summaries.is_empty()
        || sheet.structure != thinoda_core::Structure::default()
        || sheet.theme != thinoda_core::theme::ThemeSettings::default();
    for topic in sheet.topics.values() {
        let style = &topic.style;
        lossy |= !topic.marker_colors.is_empty()
            || topic.image.is_some()
            || topic.position.is_some()
            || topic.structure.is_some()
            || topic.branch_side.is_some()
            || topic.todo.is_some()
            || topic
                .hyperlink
                .as_ref()
                .is_some_and(|link| !safe_link(link))
            || topic
                .markers
                .iter()
                .any(|marker| priority_value(marker).is_none() && progress_value(marker).is_none())
            || topic
                .markers
                .iter()
                .filter(|marker| priority_value(marker).is_some())
                .count()
                > 1
            || topic
                .markers
                .iter()
                .filter(|marker| progress_value(marker).is_some())
                .count()
                > 1
            || style.fill.is_some_and(|color| color.a != 255)
            || style.text_color.is_some_and(|color| color.a != 255)
            || style.border_color.is_some()
            || style.border_width.is_some()
            || style.line_color.is_some()
            || style.line_shape.is_some()
            || style.line_width.is_some()
            || style.width.is_some();
    }
    if lossy && !allow_lossy {
        return Err(IoError::MindManagerExportNeedsConfirmation);
    }
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ap:Map xmlns:ap=\"{}\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\"><ap:OneTopic>",
        std::str::from_utf8(APPLICATION).map_err(|_| invalid())?
    );
    write_topic(sheet, sheet.root, 0, &mut xml)?;
    xml.push_str("</ap:OneTopic><ap:DocumentGroup><ap:Creator");
    attribute(&mut xml, "UserName", &book.metadata.creator)?;
    xml.push_str("/>");
    if let Some(description) = &book.metadata.description {
        xml.push_str("<ap:Description");
        attribute(&mut xml, "Comments", description)?;
        xml.push_str("/>");
    }
    xml.push_str("</ap:DocumentGroup><ap:Name");
    attribute(&mut xml, "Name", &sheet.title)?;
    xml.push_str("/></ap:Map>");
    if xml.len() as u64 > MAX_XML {
        return Err(invalid());
    }
    crate::formats::archive(&[("Document.xml", xml.as_bytes())])
}

fn safe_link(link: &str) -> bool {
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| link.to_ascii_lowercase().starts_with(scheme))
}

fn priority_value(marker: &str) -> Option<&str> {
    let value = marker.strip_prefix("priority-")?;
    ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
        .contains(&value)
        .then_some(value)
}

fn attribute(xml: &mut String, name: &str, value: &str) -> Result<(), IoError> {
    xml.push(' ');
    xml.push_str(name);
    xml.push_str("=\"");
    escape(xml, value, true)?;
    xml.push('"');
    Ok(())
}

fn escape(xml: &mut String, value: &str, attribute: bool) -> Result<(), IoError> {
    for ch in value.chars() {
        match ch {
            '&' => xml.push_str("&amp;"),
            '<' => xml.push_str("&lt;"),
            '>' => xml.push_str("&gt;"),
            '"' => xml.push_str("&quot;"),
            '\'' => xml.push_str("&apos;"),
            '\n' if attribute => xml.push_str("&#10;"),
            '\t' if attribute => xml.push_str("&#9;"),
            '\r' => xml.push_str("&#13;"),
            '\t'
            | '\n'
            | '\u{20}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}' => xml.push(ch),
            _ => return Err(invalid()),
        }
        if xml.len() as u64 > MAX_XML {
            return Err(invalid());
        }
    }
    Ok(())
}

fn write_topic(sheet: &Sheet, id: TopicId, depth: usize, xml: &mut String) -> Result<(), IoError> {
    // Leave room for Map/OneTopic and the deepest note/style property subtree.
    if depth >= (MAX_DEPTH - 8) / 2 || sheet.topics.len() > MAX_TOPICS {
        return Err(invalid());
    }
    let topic = &sheet.topics[&id];
    xml.push_str("<ap:Topic>");
    if !topic.children.is_empty() {
        xml.push_str("<ap:SubTopics>");
        for child in &topic.children {
            write_topic(sheet, *child, depth + 1, xml)?;
        }
        xml.push_str("</ap:SubTopics>");
    }
    if topic.collapsed {
        xml.push_str("<ap:TopicViewGroup ViewIndex=\"0\"><ap:Collapsed Collapsed=\"true\"/></ap:TopicViewGroup>");
    }
    xml.push_str("<ap:Text");
    attribute(xml, "PlainText", &topic.title)?;
    if let Some(align) = topic.style.align {
        attribute(
            xml,
            "TextAlignment",
            match align {
                TextAlign::Left => "urn:mindjet:Left",
                TextAlign::Center => "urn:mindjet:Center",
                TextAlign::Right => "urn:mindjet:Right",
            },
        )?;
    }
    xml.push_str("><ap:Font");
    let style = &topic.style;
    for (key, value) in [
        ("Bold", style.bold),
        ("Italic", style.italic),
        ("Underline", style.underline),
        ("Strikethrough", style.strikethrough),
    ] {
        if let Some(value) = value {
            attribute(xml, key, if value { "true" } else { "false" })?;
        }
    }
    if let Some(name) = &style.font_family {
        attribute(xml, "Name", name)?;
    }
    if let Some(color) = style.text_color.filter(|color| color.a == 255) {
        attribute(xml, "Color", &native_color(color))?;
    }
    if let Some(size) = style.font_size {
        if !size.is_finite() || size <= 0.0 || size > 10_000.0 {
            return Err(invalid());
        }
        attribute(xml, "Size", &size.to_string())?;
    }
    xml.push_str("/></ap:Text>");
    if let Some(color) = style.fill.filter(|color| color.a == 255) {
        xml.push_str("<ap:Color");
        attribute(xml, "FillColor", &native_color(color))?;
        xml.push_str("/>");
    }
    if let Some(notes) = &topic.notes {
        xml.push_str("<ap:NotesGroup><ap:NotesXhtmlData");
        attribute(xml, "PreviewPlainText", notes)?;
        xml.push_str("><xhtml:html><xhtml:body><xhtml:pre>");
        escape(xml, notes, false)?;
        xml.push_str("</xhtml:pre></xhtml:body></xhtml:html></ap:NotesXhtmlData></ap:NotesGroup>");
    }
    if let Some(value) = style.shape {
        xml.push_str("<ap:SubTopicShape");
        attribute(xml, "SubTopicShape", shape(value))?;
        xml.push_str("/>");
    }
    if let Some(link) = &topic.hyperlink
        && safe_link(link)
    {
        xml.push_str("<ap:Hyperlink");
        attribute(xml, "Url", link)?;
        xml.push_str("/>");
    }
    let priority = topic
        .markers
        .iter()
        .find_map(|marker| priority_value(marker));
    let progress = topic
        .markers
        .iter()
        .find_map(|marker| progress_value(marker));
    if priority.is_some() || progress.is_some() {
        xml.push_str("<ap:Task");
        if let Some(value) = priority {
            attribute(xml, "TaskPriority", &format!("urn:mindjet:Prio{value}"))?;
        }
        if let Some(value) = progress {
            attribute(xml, "TaskPercentage", value)?;
        }
        xml.push_str("/>");
    }
    if !topic.labels.is_empty() {
        xml.push_str("<ap:TextLabels>");
        for label in &topic.labels {
            xml.push_str("<ap:TextLabel");
            attribute(xml, "TextLabelName", label)?;
            xml.push_str("/>");
        }
        xml.push_str("</ap:TextLabels>");
    }
    xml.push_str("</ap:Topic>");
    if xml.len() as u64 > MAX_XML {
        return Err(invalid());
    }
    Ok(())
}

fn opaque_color(value: &str) -> Result<Option<Color>, IoError> {
    if value.len() != 8 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    let value = u32::from_str_radix(value, 16).map_err(|_| invalid())?;
    Ok((value >> 24 == 255)
        .then(|| Color::rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)))
}

fn native_color(color: Color) -> String {
    format!("ff{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}
