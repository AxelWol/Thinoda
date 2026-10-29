use crate::{IoError, zip_entry};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{Cursor, Write};
use thinoda_core::{
    Boundary, ChildRange, Color, LineShape, Sheet, Structure, Summary, TextAlign, TopicId,
    TopicShape, Vec2, Workbook, images,
    resources::{
        MAX_RESOURCE_BYTES, MAX_RESOURCES, MAX_SHEET_RESOURCE_BYTES, MAX_WORKBOOK_RESOURCE_BYTES,
        Resource,
    },
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAX_XMIND_ARCHIVE_ENTRIES: usize = MAX_RESOURCES + 5;

type PackageResources = HashMap<String, Resource>;

struct TopicImportContext<'a> {
    resources: &'a PackageResources,
    theme: &'a Value,
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

fn image_dimensions(image: &Value) -> Result<ImageAttributes, IoError> {
    let src = image
        .get("src")
        .and_then(Value::as_str)
        .ok_or_else(|| IoError::Invalid("XMind image is missing a string src".into()))?;
    let dimension = |name| -> Result<Option<f32>, IoError> {
        let Some(value) = image.get(name) else {
            return Ok(None);
        };
        let number = value.as_f64().filter(|number| {
            number.is_finite() && *number > 0.0 && *number <= f64::from(images::MAX_DISPLAY_SIZE)
        });
        number
            .map(|number| Some(number as f32))
            .ok_or_else(|| IoError::Invalid(format!("invalid XMind image {name}")))
    };
    let below = match image.get("align") {
        None => false,
        Some(Value::String(align)) if align == "top" => false,
        Some(Value::String(align)) if align == "bottom" => true,
        Some(Value::String(_)) => {
            return Err(IoError::Invalid(
                "unsupported XMind image alignment; expected top or bottom".into(),
            ));
        }
        Some(_) => return Err(IoError::Invalid("invalid XMind image alignment".into())),
    };
    Ok(ImageAttributes {
        src: src.to_owned(),
        width: dimension("width")?,
        height: dimension("height")?,
        below,
    })
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

fn collect_topic_resource_paths(
    topic: &Value,
    paths: &mut BTreeSet<String>,
) -> Result<(), IoError> {
    if topic.get("attachment").is_some() || topic.get("attachments").is_some() {
        return Err(IoError::Invalid(
            "XMind attachment topics are not supported; their package schema is unverified".into(),
        ));
    }
    if topic
        .get("extensions")
        .is_some_and(|extensions| contains_key(extensions, "resourceRefs"))
    {
        return Err(IoError::Invalid(
            "unsupported XMind extension resource references".into(),
        ));
    }
    if topic
        .get("notes")
        .is_some_and(|notes| contains_key(notes, "image"))
    {
        return Err(IoError::Invalid(
            "unsupported inline XMind note images".into(),
        ));
    }
    if let Some(image) = topic.get("image") {
        if !image.is_object() {
            return Err(IoError::Invalid(
                "unsupported XMind topic image representation".into(),
            ));
        }
        let attributes = image_dimensions(image)?;
        let (_, path) = image_component(&attributes.src)?;
        paths.insert(path);
    }
    if let Some(children) = topic.get("children").and_then(Value::as_object) {
        for kind in ["attached", "summary", "detached"] {
            if let Some(values) = children.get(kind).and_then(Value::as_array) {
                for child in values {
                    collect_topic_resource_paths(child, paths)?;
                }
            }
        }
        for (kind, values) in children {
            if !matches!(kind.as_str(), "attached" | "summary" | "detached")
                && values.is_array()
                && contains_key(values, "image")
            {
                return Err(IoError::Invalid(
                    "unsupported XMind image in an unrecognized topic collection".into(),
                ));
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

fn package_resources<R: std::io::Read + std::io::Seek>(
    content: &Value,
    archive: &mut ZipArchive<R>,
    names: &BTreeSet<String>,
) -> Result<PackageResources, IoError> {
    let mut references = BTreeSet::new();
    for sheet in content
        .as_array()
        .ok_or_else(|| IoError::Invalid("XMind content must be an array".into()))?
    {
        if let Some(root) = sheet.get("rootTopic") {
            collect_topic_resource_paths(root, &mut references)?;
        }
    }

    let mut declared = BTreeSet::new();
    if names.contains("manifest.json") {
        let manifest: Value = serde_json::from_slice(&crate::zip_entry_with_limit(
            archive,
            "manifest.json",
            4 * 1024 * 1024,
        )?)?;
        let entries = manifest
            .get("file-entries")
            .and_then(Value::as_object)
            .ok_or_else(|| IoError::Invalid("XMind manifest is missing file-entries".into()))?;
        if !entries.contains_key("content.json") {
            return Err(IoError::Invalid(
                "XMind manifest does not declare content.json".into(),
            ));
        }
        for (path, metadata) in entries {
            if !safe_manifest_path(path) || !metadata.is_object() {
                return Err(IoError::Invalid(format!(
                    "unsafe or invalid XMind manifest path {path}"
                )));
            }
            let resource_component = if let Some(component) = path.strip_prefix("resources/") {
                let (_, expected) = image_component(&format!("xap:resources/{component}"))?;
                if expected != *path {
                    return Err(IoError::Invalid(format!(
                        "unsafe XMind manifest resource path {path}"
                    )));
                }
                declared.insert(path.clone());
                Some(component)
            } else {
                match path.as_str() {
                    "content.json" | "metadata.json" | "content.xml" => None,
                    _ => {
                        return Err(IoError::Invalid(format!(
                            "unsupported XMind manifest entry {path}"
                        )));
                    }
                }
            };
            if !names.contains(path) {
                return Err(IoError::Invalid(format!(
                    "XMind manifest entry is missing from archive: {path}"
                )));
            }
            if resource_component.is_some() && declared.len() > MAX_RESOURCES {
                return Err(IoError::Invalid(
                    "XMind archive exceeds the 128 embedded image limit".into(),
                ));
            }
        }
    } else if !references.is_empty() {
        return Err(IoError::Invalid(
            "XMind image resources require manifest.json file-entries".into(),
        ));
    }

    for path in names {
        if path == "resources/" {
            continue;
        }
        if path.starts_with("resources/") {
            let component = path.strip_prefix("resources/").unwrap_or_default();
            let (_, expected) = image_component(&format!("xap:resources/{component}"))?;
            if expected != *path || !declared.contains(path) {
                return Err(IoError::Invalid(format!(
                    "unlisted or unsafe XMind resource archive entry {path}"
                )));
            }
        }
    }
    if references != declared {
        let missing: Vec<_> = references.difference(&declared).cloned().collect();
        if !missing.is_empty() {
            return Err(IoError::Invalid(format!(
                "XMind image resource is not declared in manifest: {}",
                missing.join(", ")
            )));
        }
        return Err(IoError::Invalid(
            "XMind package contains unreferenced image resources".into(),
        ));
    }

    let mut resources = HashMap::new();
    let mut total_bytes = 0usize;
    for path in declared {
        let bytes = crate::zip_entry_with_limit(archive, &path, MAX_RESOURCE_BYTES as u64)?;
        if bytes.is_empty() {
            return Err(IoError::Invalid(
                "XMind image resource must contain at least one byte".into(),
            ));
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_WORKBOOK_RESOURCE_BYTES {
            return Err(IoError::Invalid(
                "XMind embedded images exceed the 64 MiB workbook limit".into(),
            ));
        }
        let decoded = images::decode(&bytes).map_err(IoError::Invalid)?;
        let component = path.strip_prefix("resources/").unwrap_or_default();
        let resource =
            Resource::new(component, decoded.media_type, bytes).map_err(IoError::Invalid)?;
        resources.insert(path, resource);
    }
    Ok(resources)
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
        if sheet
            .topics
            .values()
            .any(|topic| topic.attachment.is_some())
        {
            return Err(IoError::Invalid(
                "XMind export does not support attachment topics; save as .thinoda to preserve them"
                    .into(),
            ));
        }
        images::validate_visual(sheet).map_err(IoError::Invalid)?;
        let mut referenced = HashSet::new();
        let mut sheet_bytes = 0usize;
        for (id, topic) in &sheet.topics {
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
            let bytes = resource.bytes().to_vec();
            if bytes.is_empty() || bytes.len() > MAX_RESOURCE_BYTES {
                return Err(IoError::Invalid(
                    "XMind image must contain 1 byte to 8 MiB".into(),
                ));
            }
            sheet_bytes = sheet_bytes.saturating_add(bytes.len());
            workbook_bytes = workbook_bytes.saturating_add(bytes.len());
            if sheet_bytes > MAX_SHEET_RESOURCE_BYTES {
                return Err(IoError::Invalid(
                    "XMind sheet embedded images exceed the 32 MiB limit".into(),
                ));
            }
            if workbook_bytes > MAX_WORKBOOK_RESOURCE_BYTES {
                return Err(IoError::Invalid(
                    "XMind embedded images exceed the 64 MiB workbook limit".into(),
                ));
            }
            files.insert(path, bytes);
            if files.len() > MAX_RESOURCES {
                return Err(IoError::Invalid(
                    "XMind workbook exceeds the 128 embedded image limit".into(),
                ));
            }
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
        // Earlier Thinoda builds wrote the UI label to this private property.
        _ => None,
    }
}

fn public_content(book: &Workbook) -> Result<Value, IoError> {
    fn topic(sheet: &Sheet, id: TopicId, ids: &HashMap<TopicId, String>) -> Result<Value, IoError> {
        let model = &sheet.topics[&id];
        let mut children = serde_json::Map::new();
        let attached: Vec<_> = model
            .children
            .iter()
            .map(|child| topic(sheet, *child, ids))
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
                    let mut value = topic(sheet, *floating_id, ids)?;
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
            "id": ids[&id], "class": "topic", "title": model.title, "children": children,
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
            Ok(json!({
                "id": format!("sheet-{sheet_index}"), "class": "sheet", "title": sheet.title,
                "rootTopic": topic(sheet, sheet.root, &ids)?,
                "relationships": sheet.relationships.iter().map(|relationship| json!({
                    "id": format!("rel-{sheet_index}-{}", relationship.id.0),
                    "class": "relationship",
                    "end1Id": ids.get(&relationship.from), "end2Id": ids.get(&relationship.to),
                    "title": relationship.title,
                    "style": {"properties": relationship.color.map(|color| json!({"line-color": color.to_hex()})).unwrap_or_else(|| json!({}))}
                })).collect::<Vec<_>>()
            }))
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

pub(crate) fn export(book: &Workbook) -> Result<Vec<u8>, IoError> {
    let resource_files = export_resource_files(book)?;
    let stable = serde_json::to_string(book)?;
    let sheets = public_content(book)?;
    let content = serde_json::to_vec(&sheets)?;
    let metadata = serde_json::to_vec(&json!({
        "creator": {"name": "Thinoda", "version": env!("CARGO_PKG_VERSION")},
        "dataStructureVersion": "2",
        "thinodaWorkbook": stable,
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
    context: &TopicImportContext<'_>,
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
    let image = v.get("image").map(image_dimensions).transpose()?;
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
    if let Some(attributes) = image {
        let (_, path) = image_component(&attributes.src)?;
        let resource = context
            .resources
            .get(&path)
            .ok_or_else(|| IoError::Invalid(format!("missing XMind image resource {path}")))?;
        let decoded = images::decode(resource.bytes()).map_err(IoError::Invalid)?;
        if decoded.media_type != resource.media_type {
            return Err(IoError::Invalid(
                "XMind image media type does not match its content".into(),
            ));
        }
        let source_size = Vec2::new(decoded.width as f32, decoded.height as f32);
        let size = match (attributes.width, attributes.height) {
            (Some(width), Some(height)) => Vec2::new(width, height),
            (Some(width), None) => Vec2::new(width, width * source_size.y / source_size.x),
            (None, Some(height)) => Vec2::new(height * source_size.x / source_size.y, height),
            (None, None) => images::default_size(decoded.width, decoded.height),
        };
        if !images::valid_size(size) {
            return Err(IoError::Invalid(
                "XMind image display dimensions exceed limits".into(),
            ));
        }
        let reference = s
            .resources
            .insert(resource.clone())
            .map_err(IoError::Invalid)?;
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
                context,
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
    }
    Ok(id)
}
pub(crate) fn import(bytes: &[u8]) -> Result<Workbook, IoError> {
    if bytes.starts_with(b"PK\x03\x04") {
        let mut z = ZipArchive::new(Cursor::new(bytes))?;
        let names = archive_names(&mut z)?;
        if names.contains("content.json") {
            let doc: Value = serde_json::from_slice(&zip_entry(&mut z, "content.json")?)?;
            let resources = package_resources(&doc, &mut z, &names)?;
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
                match serde_json::from_str::<Workbook>(w) {
                    Ok(workbook) => {
                        if let Err(e) = workbook.validate() {
                            log::warn!("Ignoring invalid private XMind snapshot: {e}");
                        } else {
                            let public_matches = match public_content(&workbook) {
                                Ok(content) => content == doc,
                                Err(e) => {
                                    log::warn!(
                                        "Ignoring private XMind snapshot that cannot be serialized: {e}"
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
                                    expected.len() == resources.len()
                                        && expected.iter().all(|(path, bytes)| {
                                            resources.get(path).is_some_and(|resource| {
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
                            if unchanged && resources_match {
                                return Ok(workbook);
                            }
                            log::warn!(
                                "XMind public content or image resources changed; ignoring stale private snapshot"
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
                let context = TopicImportContext {
                    resources: &resources,
                    theme: &v["theme"],
                };
                parse_topic(root, &mut s, None, 0, &mut ids, &context, "centralTopic")?;
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
    node: thinoda_core::OutlineNode,
    collapsed: bool,
    structure: Option<Structure>,
    style: thinoda_core::TopicStyle,
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
                if matches!(name.as_str(), "image" | "attachment" | "attachments") {
                    return Err(IoError::Invalid(
                        "legacy XMind image/attachment package content is not supported".into(),
                    ));
                }
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
                if matches!(name.as_str(), "image" | "attachment" | "attachments") {
                    return Err(IoError::Invalid(
                        "legacy XMind image/attachment package content is not supported".into(),
                    ));
                }
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
                        "plain" => topic.node.notes = Some(value.into()),
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
        let imported = import_legacy_xml(xml.as_bytes()).unwrap();
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
    fn package_resource_validation_rejects_unsafe_missing_mismatched_and_unsupported_data() {
        let (book, _) = image_workbook();
        let original = archive_entries(&export(&book).unwrap());

        for src in [
            "https://example.invalid/image.png",
            "resources/image.png",
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
            assert!(import(&make_archive(&entries)).is_err(), "{src}");
        }

        for align in ["left", "center"] {
            let mut entries = original.clone();
            update_content(&mut entries, |content| {
                content[0]["rootTopic"]["image"]["align"] = json!(align);
            });
            assert!(
                import(&make_archive(&entries))
                    .unwrap_err()
                    .to_string()
                    .contains("unsupported XMind image alignment"),
                "{align}"
            );
        }

        let mut missing_manifest = original.clone();
        missing_manifest.retain(|(name, _)| name != "manifest.json");
        assert!(
            import(&make_archive(&missing_manifest))
                .unwrap_err()
                .to_string()
                .contains("require manifest.json")
        );

        let mut undeclared = original.clone();
        update_content(&mut undeclared, |content| {
            content[0]["rootTopic"]["image"]["src"] = json!("xap:resources/unknown");
        });
        assert!(
            import(&make_archive(&undeclared))
                .unwrap_err()
                .to_string()
                .contains("not declared")
        );

        let mut unsafe_manifest = original.clone();
        update_manifest(&mut unsafe_manifest, |manifest| {
            let entries = manifest["file-entries"].as_object_mut().unwrap();
            let old = image_resource_path(book.active().root);
            let value = entries.remove(&old).unwrap();
            entries.insert("resources/../escape".into(), value);
        });
        assert!(import(&make_archive(&unsafe_manifest)).is_err());

        let mut missing = original.clone();
        missing.retain(|(name, _)| name != &image_resource_path(book.active().root));
        assert!(
            import(&make_archive(&missing))
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
        assert!(import(&make_archive(&unsupported)).is_err());

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
    fn package_images_enforce_display_pixel_and_encoded_size_bounds() {
        let (book, _) = image_workbook();
        let original = archive_entries(&export(&book).unwrap());

        for width in [0.0, -1.0, 1024.00001] {
            let mut invalid_display = original.clone();
            update_content(&mut invalid_display, |content| {
                content[0]["rootTopic"]["image"]["width"] = json!(width);
            });
            assert!(import(&make_archive(&invalid_display)).is_err(), "{width}");
        }

        let mut oversized_pixels = original.clone();
        let path = image_resource_path(book.active().root);
        oversized_pixels
            .iter_mut()
            .find(|(name, _)| name == &path)
            .unwrap()
            .1 = png([0, 0, 0], images::MAX_DIMENSION + 1, 1);
        assert!(import(&make_archive(&oversized_pixels)).is_err());

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
    fn changed_archive_image_does_not_restore_stale_private_bytes_and_attachments_are_rejected() {
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
        let error = export(&Workbook::new(with_attachment)).unwrap_err();
        assert!(error.to_string().contains("attachment topics"));

        let mut attachment_content = archive_entries(&export(&book).unwrap());
        update_content(&mut attachment_content, |content| {
            content[0]["rootTopic"]["attachment"] = json!({"src": "xap:resources/anything"});
        });
        assert!(
            import(&make_archive(&attachment_content))
                .unwrap_err()
                .to_string()
                .contains("attachment topics")
        );

        assert!(
            import_legacy_xml(
                b"<xmap-content><sheet><topic><image src=\"x\"/></topic></sheet></xmap-content>"
            )
            .unwrap_err()
            .to_string()
            .contains("legacy XMind image")
        );
    }
}
