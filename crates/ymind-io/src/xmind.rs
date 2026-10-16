use crate::{IoError, zip_entry};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Cursor, Write};
use ymind_core::{Boundary, ChildRange, Color, Sheet, Structure, Summary, TopicId, Vec2, Workbook};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(crate) fn export(book: &Workbook) -> Result<Vec<u8>, IoError> {
    fn topic(sheet: &Sheet, id: TopicId, ids: &HashMap<TopicId, String>) -> Value {
        let model = &sheet.topics[&id];
        let mut children = serde_json::Map::new();
        let attached: Vec<_> = model
            .children
            .iter()
            .map(|child| topic(sheet, *child, ids))
            .collect();
        if !attached.is_empty() {
            children.insert("attached".into(), json!(attached));
        }
        if id == sheet.root {
            let detached: Vec<_> = sheet
                .floating
                .iter()
                .map(|floating_id| {
                    let floating = &sheet.topics[floating_id];
                    let position = floating.position.unwrap_or(Vec2::ZERO);
                    let mut value = topic(sheet, *floating_id, ids);
                    value["position"] = json!({"x": position.x, "y": position.y});
                    value
                })
                .collect();
            if !detached.is_empty() {
                children.insert("detached".into(), json!(detached));
            }
        }
        let style = &model.style;
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
        if style.bold == Some(true) {
            properties.insert("fo:font-weight".into(), json!("bold"));
        }
        if style.italic == Some(true) {
            properties.insert("fo:font-style".into(), json!("italic"));
        }
        if let Some(color) = style.border_color {
            properties.insert("svg:stroke".into(), json!(color.to_hex()));
        }
        if let Some(width) = style.border_width {
            properties.insert("svg:stroke-width".into(), json!(width));
        }
        if let Some(color) = style.line_color {
            properties.insert("line-color".into(), json!(color.to_hex()));
        }
        if let Some(width) = style.line_width {
            properties.insert("line-width".into(), json!(width));
        }
        if let Some(shape) = style.shape {
            properties.insert(
                "shape-class".into(),
                json!(format!("org.xmind.topicShape.{}", shape.label())),
            );
        }
        if let Some(line_shape) = style.line_shape {
            properties.insert("line-shape".into(), json!(line_shape.label()));
        }
        let mut value = json!({
            "id": ids[&id], "class": "topic", "title": model.title, "children": children,
            "markers": model.markers.iter().map(|marker| json!({"markerId": marker})).collect::<Vec<_>>(),
            "labels": model.labels, "branch": if model.collapsed { "folded" } else { "" },
            "style": {"properties": properties}
        });
        if let Some(notes) = &model.notes {
            value["notes"] = json!({"plain":{"content": notes}});
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
        let boundaries: Vec<_> = sheet.boundaries.iter().filter(|boundary| boundary.parent == id).map(|boundary| {
            json!({
                "id": format!("boundary-{}", boundary.id.0),
                "range": boundary.range.map(|range| format!("({}, {})", range.start, range.end)).unwrap_or_else(|| "master".into()),
                "title": boundary.title,
                "style": {"properties": {"svg:fill": boundary.color.map(|color| color.to_hex())}}
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
                        "id": format!("summary-{}", summary.id.0),
                        "range": format!("({}, {})", summary.range.start, summary.range.end),
                        "topicId": ids.get(&summary.topic)
                    }))
                    .collect::<Vec<_>>()
            );
            value["children"]["summary"] = json!(
                sheet_summaries
                    .iter()
                    .map(|summary| topic(sheet, summary.topic, ids))
                    .collect::<Vec<_>>()
            );
        }
        value
    }
    let stable = serde_json::to_string(book)?;
    let sheets: Vec<Value> = book
        .sheets
        .iter()
        .enumerate()
        .map(|(sheet_index, sheet)| {
            let ids: HashMap<TopicId, String> = sheet
                .topics
                .keys()
                .map(|id| (*id, format!("topic-{sheet_index}-{}", id.0)))
                .collect();
            json!({
                "id": format!("sheet-{sheet_index}"), "class": "sheet", "title": sheet.title,
                "rootTopic": topic(sheet, sheet.root, &ids),
                "relationships": sheet.relationships.iter().map(|relationship| json!({
                    "id": format!("rel-{}", relationship.id.0),
                    "end1Id": ids.get(&relationship.from), "end2Id": ids.get(&relationship.to),
                    "title": relationship.title
                })).collect::<Vec<_>>()
            })
        })
        .collect();
    let content = serde_json::to_vec(&sheets)?;
    let metadata = serde_json::to_vec(&json!({"creator":"yMind","ymindWorkbook":stable}))?;
    let manifest =
        serde_json::to_vec(&json!({"file-entries":{"content.json":{},"metadata.json":{}}}))?;
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
        out.push_str(&format!(
            "<topic id=\"{}\"{}>",
            id.0,
            if t.collapsed {
                " branch=\"folded\""
            } else {
                ""
            }
        ));
        out.push_str("<title>");
        out.push_str(&xml_escape(&t.title));
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
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><xmap-content xmlns:xlink=\"http://www.w3.org/1999/xlink\" version=\"2.0\">",
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
fn parse_topic(
    v: &Value,
    s: &mut Sheet,
    parent: Option<TopicId>,
    depth: usize,
    idmap: &mut HashMap<String, TopicId>,
) -> Result<TopicId, IoError> {
    if depth > 2000 {
        return Err(IoError::Invalid("XMind topic nesting exceeds 2000".into()));
    }
    let title = v.get("title").and_then(Value::as_str).unwrap_or("Untitled");
    let id = match parent {
        Some(parent_id) => s.add_child(parent_id, None, title),
        None => s.root,
    };
    if parent.is_none() {
        s.topics.get_mut(&id).unwrap().title = title.to_string();
    }
    if let Some(key) = v.get("id").and_then(Value::as_str) {
        idmap.insert(key.to_string(), id);
    }
    let t = s.topic_mut(id).unwrap();
    t.collapsed = v.get("branch").and_then(Value::as_str) == Some("folded");
    t.hyperlink = v.get("href").and_then(Value::as_str).map(str::to_owned);
    if let Some(n) = v
        .get("notes")
        .and_then(|x| x.get("plain"))
        .and_then(|x| x.get("content"))
        .and_then(Value::as_str)
    {
        t.notes = Some(n.to_owned());
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
    if let Some(props) = v
        .get("style")
        .and_then(|x| x.get("properties"))
        .and_then(Value::as_object)
    {
        let style = &mut s.topic_mut(id).unwrap().style;
        style.fill = props
            .get("svg:fill")
            .and_then(Value::as_str)
            .and_then(color);
        style.text_color = props
            .get("fo:color")
            .and_then(Value::as_str)
            .and_then(color);
        style.border_color = props
            .get("svg:stroke")
            .and_then(Value::as_str)
            .and_then(color);
        style.border_width = props
            .get("svg:stroke-width")
            .and_then(Value::as_f64)
            .map(|x| x as f32);
        style.bold = props
            .get("fo:font-weight")
            .and_then(Value::as_str)
            .map(|v| v == "bold");
        style.italic = props
            .get("fo:font-style")
            .and_then(Value::as_str)
            .map(|v| v == "italic");
        style.font_size = props
            .get("fo:font-size")
            .and_then(Value::as_str)
            .and_then(|x| x.trim_end_matches("pt").parse().ok());
        style.line_color = props
            .get("line-color")
            .and_then(Value::as_str)
            .and_then(color);
        style.line_width = props
            .get("line-width")
            .and_then(Value::as_f64)
            .map(|x| x as f32);
        style.shape = props
            .get("shape-class")
            .and_then(Value::as_str)
            .and_then(|shape| {
                let shape = shape
                    .rsplit('.')
                    .next()
                    .unwrap_or(shape)
                    .to_ascii_lowercase();
                match shape.as_str() {
                    "roundedrect" | "rounded-rect" => Some(ymind_core::TopicShape::RoundedRect),
                    "rect" => Some(ymind_core::TopicShape::Rect),
                    "ellipse" => Some(ymind_core::TopicShape::Ellipse),
                    "capsule" => Some(ymind_core::TopicShape::Capsule),
                    "diamond" => Some(ymind_core::TopicShape::Diamond),
                    "underline" => Some(ymind_core::TopicShape::Underline),
                    "none" => Some(ymind_core::TopicShape::None),
                    _ => None,
                }
            });
    }
    let attached = v
        .get("children")
        .and_then(|x| x.get("attached"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for child in attached {
        parse_topic(&child, s, Some(id), depth + 1, idmap)?;
    }
    if let Some(boundaries) = v.get("boundaries").and_then(Value::as_array) {
        for boundary in boundaries {
            let range = boundary.get("range").and_then(parse_range);
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
                    .and_then(|x| x.get("svg:fill"))
                    .and_then(Value::as_str)
                    .and_then(color),
            });
        }
    }
    if let Some(summaries) = v
        .get("children")
        .and_then(|x| x.get("summary"))
        .and_then(Value::as_array)
    {
        for summary_topic in summaries {
            let summary_id = parse_topic(summary_topic, s, Some(id), depth + 1, idmap)?;
            s.topic_mut(id)
                .unwrap()
                .children
                .retain(|child| *child != summary_id);
            s.topic_mut(summary_id).unwrap().parent = None;
        }
    }
    if let Some(summaries) = v.get("summaries").and_then(Value::as_array) {
        for summary in summaries {
            let topic_id = summary
                .get("topicId")
                .and_then(Value::as_str)
                .and_then(|key| idmap.get(key))
                .copied();
            let Some(topic_id) = topic_id else {
                continue;
            };
            let range = summary
                .get("range")
                .and_then(parse_range)
                .unwrap_or(ChildRange { start: 0, end: 0 });
            let summary_id = s.alloc_summary_id();
            s.summaries.push(Summary {
                id: summary_id,
                parent: id,
                range,
                topic: topic_id,
            });
        }
    }
    if parent.is_none() {
        let detached = v
            .get("children")
            .and_then(|x| x.get("detached"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for d in detached {
            let p = d
                .get("position")
                .map(|p| {
                    Vec2::new(
                        p.get("x").and_then(Value::as_f64).unwrap_or(0.) as f32,
                        p.get("y").and_then(Value::as_f64).unwrap_or(0.) as f32,
                    )
                })
                .unwrap_or(Vec2::ZERO);
            let fid = parse_topic(&d, s, Some(id), depth + 1, idmap)?;
            s.topic_mut(id)
                .unwrap()
                .children
                .retain(|child| *child != fid);
            let floating = s.topic_mut(fid).unwrap();
            floating.parent = None;
            floating.position = Some(p);
            s.floating.push(fid);
        }
    }
    Ok(id)
}
pub(crate) fn import(bytes: &[u8]) -> Result<Workbook, IoError> {
    if bytes.starts_with(b"PK\x03\x04") {
        let mut z = ZipArchive::new(Cursor::new(bytes))?;
        if z.file_names().any(|n| n.is_ok_and(|v| v == "content.json")) {
            let meta = if z
                .file_names()
                .any(|n| n.is_ok_and(|v| v == "metadata.json"))
            {
                serde_json::from_slice::<Value>(&zip_entry(&mut z, "metadata.json")?).ok()
            } else {
                None
            };
            if let Some(w) = meta
                .as_ref()
                .and_then(|m| m.get("ymindWorkbook"))
                .and_then(Value::as_str)
            {
                return Ok(serde_json::from_str(w)?);
            }
            let doc: Value = serde_json::from_slice(&zip_entry(&mut z, "content.json")?)?;
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
                parse_topic(root, &mut s, None, 0, &mut ids)?;
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
                            s.relationships.push(ymind_core::Relationship {
                                id: rid,
                                from: a,
                                to: b,
                                title: r.get("title").and_then(Value::as_str).unwrap_or("").into(),
                                color: None,
                            });
                        }
                    }
                }
                sheets.push(s);
            }
            if sheets.is_empty() {
                return Err(IoError::Invalid("XMind workbook has no sheets".into()));
            }
            return Ok(Workbook {
                sheets,
                active_sheet: 0,
                metadata: Default::default(),
            });
        }
        let xml = zip_entry(&mut z, "content.xml")?;
        return import_legacy_xml(&xml);
    }
    if bytes.starts_with(b"<") {
        return import_legacy_xml(bytes);
    }
    Err(IoError::Invalid("not an XMind archive".into()))
}
#[derive(Default)]
struct LegacyTopic {
    node: ymind_core::OutlineNode,
    collapsed: bool,
    structure: Option<Structure>,
    style: ymind_core::TopicStyle,
    children: Vec<LegacyTopic>,
}
fn import_legacy_xml(bytes: &[u8]) -> Result<Workbook, IoError> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut sheet_title = String::from("Sheet");
    let mut stack: Vec<LegacyTopic> = Vec::new();
    let mut roots = Vec::new();
    let mut capture: Option<String> = None;
    let mut capture_kind: Option<&str> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase().to_string();
                if name == "sheet" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref().eq_ignore_ascii_case("title") {
                            sheet_title = attr
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap_or_default()
                                .to_string();
                        }
                    }
                } else if name == "topic" {
                    if stack.len() >= 2000 {
                        return Err(IoError::Invalid("XMind nesting exceeds 2000 levels".into()));
                    }
                    let mut topic = LegacyTopic::default();
                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref().to_ascii_lowercase();
                        let value = attr
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .unwrap_or_default()
                            .to_string();
                        match key.as_str() {
                            "branch" => topic.collapsed = value == "folded",
                            "structure-class" => {
                                topic.structure = Structure::from_xmind_class(&value)
                            }
                            _ if key.ends_with("href") => topic.node.link = Some(value),
                            _ => {}
                        }
                    }
                    stack.push(topic);
                } else if name == "title" || name == "plain" || name == "label" {
                    capture_kind = Some(if name == "title" {
                        "title"
                    } else if name == "plain" {
                        "plain"
                    } else {
                        "label"
                    });
                    capture = Some(String::new());
                } else if name == "marker-ref" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref().eq_ignore_ascii_case("marker-id")
                            && let Some(topic) = stack.last_mut()
                        {
                            topic.node.markers.push(
                                attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .unwrap_or_default()
                                    .to_string(),
                            );
                        }
                    }
                } else if name == "topic-properties"
                    && let Some(topic) = stack.last_mut()
                {
                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref().to_ascii_lowercase();
                        let value = attr
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .unwrap_or_default()
                            .to_string();
                        match key.as_str() {
                            "fo:color" => topic.style.text_color = Color::from_hex(&value),
                            "svg:fill" | "background-color" => {
                                topic.style.fill = Color::from_hex(&value)
                            }
                            "svg:stroke" => topic.style.border_color = Color::from_hex(&value),
                            "fo:font-size" => {
                                topic.style.font_size = value.trim_end_matches("pt").parse().ok()
                            }
                            "fo:font-weight" => topic.style.bold = Some(value == "bold"),
                            "fo:font-style" => topic.style.italic = Some(value == "italic"),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase().to_string();
                if name == "marker-ref" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref().eq_ignore_ascii_case("marker-id")
                            && let Some(topic) = stack.last_mut()
                        {
                            topic.node.markers.push(
                                attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .unwrap_or_default()
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if let Some(capture) = capture.as_mut() {
                    capture.push_str(&e.xml_content(quick_xml::XmlVersion::Implicit1_0));
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase().to_string();
                if (name == "title" || name == "plain" || name == "label")
                    && let (Some(kind), Some(value), Some(topic)) =
                        (capture_kind.take(), capture.take(), stack.last_mut())
                {
                    match kind {
                        "title" => topic.node.title = value,
                        "plain" => topic.node.notes = Some(value),
                        "label" => topic.node.labels.push(value),
                        _ => {}
                    }
                } else if name == "topic"
                    && let Some(topic) = stack.pop()
                {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(topic);
                    } else {
                        roots.push(topic);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(IoError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }
    let mut roots = roots.into_iter();
    let root = roots
        .next()
        .ok_or_else(|| IoError::Invalid("XMind XML has no topics".into()))?;
    let mut workbook = Workbook::new(Sheet::new(&sheet_title, &root.node.title));
    fn apply(sheet: &mut Sheet, id: TopicId, topic: LegacyTopic) {
        let LegacyTopic {
            node,
            collapsed,
            structure,
            style,
            children,
        } = topic;
        let root_id = sheet.root;
        {
            let model = sheet.topic_mut(id).unwrap();
            model.notes = node.notes;
            model.hyperlink = node.link;
            model.labels = node.labels;
            model.markers = node.markers;
            model.collapsed = collapsed;
            model.style = style;
            if id != root_id {
                model.structure = structure;
            }
        }
        if id == root_id
            && let Some(structure) = structure
        {
            sheet.structure = structure;
        }
        for child in children {
            let child_id = sheet.add_child(id, None, child.node.title.clone());
            apply(sheet, child_id, child);
        }
    }
    let root_id = workbook.active().root;
    apply(workbook.active_mut(), root_id, root);
    for extra in roots {
        let id = workbook
            .active_mut()
            .add_floating(extra.node.title.clone(), Vec2::ZERO);
        apply(workbook.active_mut(), id, extra);
    }
    Ok(workbook)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_xml_export_has_topic_content() {
        let mut w = Workbook::new(Sheet::new("S", "Root & title"));
        let root = w.active().root;
        w.active_mut().add_child(root, None, "Child");
        let bytes = export(&w).unwrap();
        let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let xml = crate::zip_entry(&mut z, "content.xml").unwrap();
        let xml = String::from_utf8(xml).unwrap();
        assert!(xml.contains("Root &amp; title"));
        assert!(xml.contains("Child"));
    }
}
