use crate::{IoError, zip_entry};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Cursor, Write};
use ymind_core::{
    Boundary, ChildRange, Color, LineShape, Sheet, Structure, Summary, TextAlign, TopicId,
    TopicShape, Vec2, Workbook,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

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
    let suffix = class.rsplit('.').next().unwrap_or(class);
    match suffix
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-')
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .as_str()
    {
        "roundedrect" | "roundedrectangle" => Some(TopicShape::RoundedRect),
        "rect" | "rectangle" => Some(TopicShape::Rect),
        "ellipse" | "ellipserect" => Some(TopicShape::Ellipse),
        "capsule" => Some(TopicShape::Capsule),
        "diamond" => Some(TopicShape::Diamond),
        "underline" => Some(TopicShape::Underline),
        "none" => Some(TopicShape::None),
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

fn parse_line_class(class: &str) -> Option<LineShape> {
    let suffix = class.rsplit('.').next().unwrap_or(class);
    match suffix
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-')
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .as_str()
    {
        "curve" => Some(LineShape::Curve),
        "straight" => Some(LineShape::Straight),
        "elbow" => Some(LineShape::Elbow),
        "roundedelbow" => Some(LineShape::RoundedElbow),
        // Earlier yMind builds wrote the UI label to this private property.
        _ => None,
    }
}

fn public_content(book: &Workbook) -> Value {
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
        if let Some(width) = style.line_width {
            properties.insert("line-width".into(), json!(width.to_string()));
        }
        if let Some(shape) = style.shape {
            properties.insert("shape-class".into(), json!(shape_class(shape)));
        }
        if let Some(line_shape) = style.line_shape {
            properties.insert("line-class".into(), json!(line_class(line_shape)));
        }
        if let Some(font_family) = &style.font_family {
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
                    .map(|summary| topic(sheet, summary.topic, ids))
                    .collect::<Vec<_>>()
            );
        }
        value
    }
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
                    "id": format!("rel-{sheet_index}-{}", relationship.id.0),
                    "class": "relationship",
                    "end1Id": ids.get(&relationship.from), "end2Id": ids.get(&relationship.to),
                    "title": relationship.title,
                    "style": {"properties": relationship.color.map(|color| json!({"line-color": color.to_hex()})).unwrap_or_else(|| json!({}))}
                })).collect::<Vec<_>>()
            })
        })
        .collect();
    json!(sheets)
}

fn content_digest(content: &Value) -> Result<String, IoError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(content)?)
    ))
}

pub(crate) fn export(book: &Workbook) -> Result<Vec<u8>, IoError> {
    let stable = serde_json::to_string(book)?;
    let sheets = public_content(book);
    let content = serde_json::to_vec(&sheets)?;
    let metadata = serde_json::to_vec(&json!({
        "creator": {"name": "yMind", "version": env!("CARGO_PKG_VERSION")},
        "dataStructureVersion": "2",
        "ymindWorkbook": stable,
        "ymindContentSha256": content_digest(&sheets)?
    }))?;
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
        let style = &t.style;
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
        if let Some(family) = &style.font_family {
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
        if let Some(width) = style.line_width {
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

fn parse_topic(
    v: &Value,
    s: &mut Sheet,
    parent: Option<TopicId>,
    depth: usize,
    idmap: &mut HashMap<String, TopicId>,
    theme: &Value,
    role: &'static str,
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
    let mut props = theme
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
        style.fill = props
            .get("svg:fill")
            .and_then(Value::as_str)
            .and_then(color);
        style.text_color = props
            .get("fo:color")
            .and_then(Value::as_str)
            .and_then(color);
        style.border_color = props
            .get("border-line-color")
            .or_else(|| props.get("svg:stroke"))
            .and_then(Value::as_str)
            .and_then(color);
        style.border_width = props
            .get("border-line-width")
            .or_else(|| props.get("svg:stroke-width"))
            .and_then(parse_number);
        style.bold = props
            .get("fo:font-weight")
            .and_then(Value::as_str)
            .map(|v| v == "bold" || v == "600" || v == "700" || v == "800" || v == "900");
        style.italic = props
            .get("fo:font-style")
            .and_then(Value::as_str)
            .map(|v| v == "italic");
        style.font_size = props.get("fo:font-size").and_then(parse_number);
        style.font_family = props
            .get("fo:font-family")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(decoration) = props.get("fo:text-decoration").and_then(Value::as_str) {
            if decoration == "none" {
                style.underline = Some(false);
                style.strikethrough = None;
            } else {
                style.underline = decoration.contains("underline").then_some(true);
                style.strikethrough = decoration.contains("line-through").then_some(true);
            }
        }
        style.align = props
            .get("fo:text-align")
            .and_then(Value::as_str)
            .and_then(|value| match value {
                "left" => Some(TextAlign::Left),
                "center" => Some(TextAlign::Center),
                "right" => Some(TextAlign::Right),
                _ => None,
            });
        style.line_color = props
            .get("line-color")
            .and_then(Value::as_str)
            .and_then(color);
        style.line_width = props.get("line-width").and_then(parse_number);
        style.shape = props
            .get("shape-class")
            .and_then(Value::as_str)
            .and_then(parse_shape_class);
        style.line_shape = props
            .get("line-class")
            .or_else(|| props.get("line-shape"))
            .and_then(Value::as_str)
            .and_then(parse_line_class);
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
        parse_topic(&child, s, Some(id), depth + 1, idmap, theme, child_role)?;
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
                    .and_then(|x| x.get("svg:fill").or_else(|| x.get("fill")))
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
            let summary_id = parse_topic(
                summary_topic,
                s,
                Some(id),
                depth + 1,
                idmap,
                theme,
                "summaryTopic",
            )?;
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
            let fid = parse_topic(&d, s, Some(id), depth + 1, idmap, theme, "floatingTopic")?;
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
            let doc: Value = serde_json::from_slice(&zip_entry(&mut z, "content.json")?)?;
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
                .and_then(|m| m.get("ymindWorkbook"))
                .and_then(Value::as_str)
            {
                match serde_json::from_str::<Workbook>(w) {
                    Ok(workbook) => {
                        if let Err(e) = workbook.validate() {
                            log::warn!("Ignoring invalid private XMind snapshot: {e}");
                        } else {
                            let unchanged = match meta
                                .as_ref()
                                .and_then(|m| m.get("ymindContentSha256"))
                            {
                                Some(hash) => hash.as_str() == Some(content_digest(&doc)?.as_str()),
                                // Old yMind exports have no digest. Only trust them when their
                                // generated public representation still matches the actual file.
                                None => public_content(&workbook) == doc,
                            };
                            if unchanged {
                                return Ok(workbook);
                            }
                            log::warn!(
                                "XMind public content changed; ignoring stale private snapshot"
                            );
                        }
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
                parse_topic(root, &mut s, None, 0, &mut ids, &v["theme"], "centralTopic")?;
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
                                control_points: None,
                                title: r.get("title").and_then(Value::as_str).unwrap_or("").into(),
                                color: r
                                    .get("style")
                                    .and_then(|style| style.get("properties"))
                                    .and_then(|props| props.get("line-color"))
                                    .and_then(Value::as_str)
                                    .and_then(color),
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

fn parse_legacy_topic_property(topic: &mut LegacyTopic, key: &str, value: &str) {
    match key {
        "fo:color" => topic.style.text_color = Color::from_hex(value),
        "svg:fill" | "background-color" => topic.style.fill = Color::from_hex(value),
        "svg:stroke" | "border-line-color" => topic.style.border_color = Color::from_hex(value),
        "svg:stroke-width" | "border-line-width" => {
            topic.style.border_width = parse_number(&Value::String(value.to_owned()))
        }
        "fo:font-size" => topic.style.font_size = parse_number(&Value::String(value.to_owned())),
        "fo:font-weight" => topic.style.bold = Some(value == "bold" || value == "600"),
        "fo:font-style" => topic.style.italic = Some(value == "italic"),
        "fo:font-family" => topic.style.font_family = Some(value.to_owned()),
        "fo:text-decoration" => {
            if value == "none" {
                topic.style.underline = Some(false);
                topic.style.strikethrough = None;
            } else {
                topic.style.underline = value.contains("underline").then_some(true);
                topic.style.strikethrough = value.contains("line-through").then_some(true);
            }
        }
        "fo:text-align" => {
            topic.style.align = match value {
                "left" => Some(TextAlign::Left),
                "center" => Some(TextAlign::Center),
                "right" => Some(TextAlign::Right),
                _ => None,
            }
        }
        "line-color" => topic.style.line_color = Color::from_hex(value),
        "line-width" => topic.style.line_width = parse_number(&Value::String(value.to_owned())),
        "line-class" | "line-shape" => topic.style.line_shape = parse_line_class(value),
        "shape-class" => topic.style.shape = parse_shape_class(value),
        _ => {}
    }
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
                        parse_legacy_topic_property(topic, &key, &value);
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
                } else if name == "topic-properties"
                    && let Some(topic) = stack.last_mut()
                {
                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref().to_ascii_lowercase();
                        let value = attr
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .unwrap_or_default()
                            .to_string();
                        parse_legacy_topic_property(topic, &key, &value);
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
        let root = w.active().root;
        let child = w.active_mut().add_child(root, None, "Child");
        w.active_mut().topic_mut(child).unwrap().style = ymind_core::TopicStyle {
            fill: Some(Color::rgb(10, 20, 30)),
            shape: Some(TopicShape::Diamond),
            line_shape: Some(LineShape::RoundedElbow),
            font_family: Some("Noto Sans".into()),
            font_size: Some(14.0),
            ..Default::default()
        };
        let bytes = export(&w).unwrap();
        let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let xml = crate::zip_entry(&mut z, "content.xml").unwrap();
        let xml = String::from_utf8(xml).unwrap();
        assert!(xml.contains("Root &amp; title"));
        assert!(xml.contains("Child"));
        assert!(xml.contains("shape-class=\"org.xmind.topicShape.diamond\""));
        let imported = import_legacy_xml(xml.as_bytes()).unwrap();
        let imported_child = imported
            .active()
            .topic(imported.active().root_topic().children[0])
            .unwrap();
        assert_eq!(imported_child.style.fill, Some(Color::rgb(10, 20, 30)));
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
}
