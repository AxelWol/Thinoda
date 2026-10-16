use crate::{ExportOptions, IoError, zip_entry};
use serde_json::json;
use std::io::{Cursor, Write};
use ymind_core::{
    OutlineNode, Sheet, TopicId, Workbook,
    outline::{parse_indented_text, to_indented_text},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn archive(files: &[(&str, &[u8])]) -> Result<Vec<u8>, IoError> {
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
    let content = serde_json::to_vec(w)?;
    let manifest = serde_json::to_vec(&json!({"format":"ymind","version":1,"resources":[]}))?;
    archive(&[("manifest.json", &manifest), ("content.json", &content)])
}
pub(crate) fn native_import(bytes: &[u8]) -> Result<Workbook, IoError> {
    if bytes.starts_with(b"PK\x03\x04") {
        let mut z = ZipArchive::new(Cursor::new(bytes))?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&zip_entry(&mut z, "manifest.json")?)?;
        let version = manifest
            .get("version")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        if version > 1 {
            return Err(IoError::NewerVersion(format!("{version}")));
        }
        if manifest
            .get("format")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v != "ymind")
        {
            return Err(IoError::Invalid("manifest format is not ymind".into()));
        }
        Ok(serde_json::from_slice(&zip_entry(&mut z, "content.json")?)?)
    } else {
        Ok(serde_json::from_slice(bytes)?)
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn root_outline(s: &Sheet) -> OutlineNode {
    ymind_core::outline::to_outline(s, s.root)
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
    Ok(Workbook::new(ymind_core::outline::sheet_from_outline(
        &root.title,
        &root,
    )))
}
pub(crate) fn markdown_export(w: &Workbook, o: &ExportOptions) -> Vec<u8> {
    let sheet = &w.sheets[o.sheet.unwrap_or(w.active_sheet).min(w.sheets.len() - 1)];
    let mut out = String::new();
    let root = &sheet.topics[&sheet.root];
    out.push_str("# ");
    out.push_str(&root.title);
    out.push('\n');
    if let Some(notes) = &root.notes {
        out.push('\n');
        out.push_str(notes);
        out.push('\n');
    }
    fn render_children(sheet: &Sheet, id: TopicId, depth: usize, out: &mut String) {
        for child in &sheet.topics[&id].children {
            let topic = &sheet.topics[child];
            if depth == 1 {
                out.push_str("\n## ");
                out.push_str(&topic.title);
                out.push('\n');
            } else {
                out.push_str(&"  ".repeat(depth - 2));
                out.push_str("- ");
                out.push_str(&topic.title);
                out.push('\n');
            }
            if let Some(notes) = &topic.notes {
                out.push_str(&"  ".repeat(depth.saturating_sub(1)));
                out.push_str("  ");
                out.push_str(notes);
                out.push('\n');
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
    let mut tree = ymind_core::outline::build_tree(&items);
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
    Ok(Workbook::new(ymind_core::outline::sheet_from_outline(
        &root.title,
        &root,
    )))
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
            out.push_str(&format!(" _note=\"{}\"", escape(n)));
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
    fn rec(s: &Sheet, id: TopicId, out: &mut String, depth: usize) {
        if depth > 2000 {
            return;
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
            return;
        }
        out.push('>');
        if let Some(n) = &t.notes {
            out.push_str(&format!(
                "<richcontent TYPE=\"NOTE\"><html><body><p>{}</p></body></html></richcontent>",
                escape(n)
            ));
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
            rec(s, *c, out, depth + 1);
        }
        out.push_str("</node>");
    }
    rec(s, s.root, &mut out, 0);
    out.push_str("</map>");
    Ok(out.into_bytes())
}
#[derive(Default)]
struct XmlNode {
    title: String,
    notes: Option<String>,
    link: Option<String>,
    folded: bool,
    markers: Vec<String>,
    children: Vec<XmlNode>,
}
fn read_xml_tree(bytes: &[u8], opml: bool) -> Result<(String, Vec<XmlNode>), IoError> {
    use quick_xml::{Reader, events::Event};
    let mut r = Reader::from_reader(bytes);
    r.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut roots = Vec::new();
    let mut title = String::new();
    let mut in_title = false;
    let mut note_target: Option<usize> = None;
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
                            "_note" => n.notes = Some(v),
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
                                note_target = Some(stack.len().saturating_sub(1));
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
                            n.notes = Some(v);
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
                } else if let Some(i) = note_target
                    && let Some(n) = stack.get_mut(i)
                {
                    n.notes.get_or_insert_with(String::new).push_str(&v);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if name == "title" {
                    in_title = false;
                }
                if name == "richcontent" {
                    note_target = None;
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
    Ok((title, roots))
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
    let (sheet_title, mut outlines) = read_xml_tree(b, true)?;
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
    Ok(Workbook::new(ymind_core::outline::sheet_from_outline(
        title, &root,
    )))
}
pub(crate) fn freemind_import(b: &[u8]) -> Result<Workbook, IoError> {
    let (_, mut roots) = read_xml_tree(b, false)?;
    let root = xml_node_to_outline(roots.remove(0));
    Ok(Workbook::new(ymind_core::outline::sheet_from_outline(
        &root.title,
        &root,
    )))
}
pub(crate) fn mindmanager_import(b: &[u8]) -> Result<Workbook, IoError> {
    let mut z = ZipArchive::new(Cursor::new(b))?;
    let xml = zip_entry(&mut z, "Document.xml")?;
    let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut stack: Vec<OutlineNode> = Vec::new();
    let mut roots = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = e
                    .name()
                    .as_ref()
                    .rsplit(':')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                if name.eq_ignore_ascii_case("Topic") {
                    if stack.len() >= 2000 {
                        return Err(IoError::Invalid(
                            "MindManager nesting exceeds 2000 levels".into(),
                        ));
                    }
                    stack.push(OutlineNode::default());
                } else if name.eq_ignore_ascii_case("Text") {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref().eq_ignore_ascii_case("PlainText")
                            && let Some(topic) = stack.last_mut()
                        {
                            topic.title = attr
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap_or_default()
                                .to_string();
                        }
                    }
                } else if name.eq_ignore_ascii_case("Hyperlink")
                    || name.eq_ignore_ascii_case("NotesXhtmlData")
                {
                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref();
                        let value = attr
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .unwrap_or_default()
                            .to_string();
                        if let Some(topic) = stack.last_mut() {
                            if name.eq_ignore_ascii_case("Hyperlink")
                                && key.eq_ignore_ascii_case("Url")
                            {
                                topic.link = Some(value.clone());
                            }
                            if name.eq_ignore_ascii_case("NotesXhtmlData")
                                && key.eq_ignore_ascii_case("PreviewPlainText")
                            {
                                topic.notes = Some(value);
                            }
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                let name = e
                    .name()
                    .as_ref()
                    .rsplit(':')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                if name.eq_ignore_ascii_case("Text")
                    || name.eq_ignore_ascii_case("Hyperlink")
                    || name.eq_ignore_ascii_case("NotesXhtmlData")
                {
                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref();
                        let value = attr
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .unwrap_or_default()
                            .to_string();
                        if let Some(topic) = stack.last_mut() {
                            if name.eq_ignore_ascii_case("Text")
                                && key.eq_ignore_ascii_case("PlainText")
                            {
                                topic.title = value.clone();
                            }
                            if name.eq_ignore_ascii_case("Hyperlink")
                                && key.eq_ignore_ascii_case("Url")
                            {
                                topic.link = Some(value.clone());
                            }
                            if name.eq_ignore_ascii_case("NotesXhtmlData")
                                && key.eq_ignore_ascii_case("PreviewPlainText")
                            {
                                topic.notes = Some(value);
                            }
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let name = e
                    .name()
                    .as_ref()
                    .rsplit(':')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                if name.eq_ignore_ascii_case("Topic")
                    && let Some(topic) = stack.pop()
                {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(topic);
                    } else {
                        roots.push(topic);
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(IoError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }
    let root = roots
        .into_iter()
        .next()
        .ok_or_else(|| IoError::Invalid("MindManager document contains no topics".into()))?;
    Ok(Workbook::new(ymind_core::outline::sheet_from_outline(
        &root.title,
        &root,
    )))
}
pub(crate) fn textbundle_export(w: &Workbook, o: &ExportOptions) -> Result<Vec<u8>, IoError> {
    let md = String::from_utf8(markdown_export(w, o)).unwrap_or_default();
    let name = "mindmap";
    let info = serde_json::to_vec(
        &serde_json::json!({"version":2,"type":"net.daringfireball.markdown","transient":false,"creatorURL":"https://ymind.app"}),
    )?;
    let a = format!("{name}.textbundle/info.json");
    let b = format!("{name}.textbundle/text.md");
    archive(&[(&a, &info), (&b, md.as_bytes())])
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
