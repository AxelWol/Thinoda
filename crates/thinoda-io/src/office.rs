use crate::{ExportOptions, ImportReport, ImportWarning, IoError};
use std::{
    collections::{BTreeSet, HashMap},
    io::{Cursor, Write},
};
use thinoda_core::{OutlineNode, TopicId, Workbook};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(super) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn sheet<'a>(w: &'a Workbook, o: &ExportOptions) -> &'a thinoda_core::Sheet {
    &w.sheets[o.sheet.unwrap_or(w.active_sheet).min(w.sheets.len() - 1)]
}
type TopicRow = (
    usize,
    TopicId,
    String,
    Option<String>,
    String,
    String,
    String,
);

#[derive(Default)]
struct DocxParagraph {
    style: Option<String>,
    numbering: Option<u32>,
    list_level: usize,
    text: String,
}

fn xml_value(event: &quick_xml::events::BytesStart<'_>) -> Result<Option<String>, IoError> {
    xml_attribute(event, "val")
}

fn xml_style_id(event: &quick_xml::events::BytesStart<'_>) -> Result<Option<String>, IoError> {
    xml_attribute(event, "styleId")
}

fn xml_attribute(
    event: &quick_xml::events::BytesStart<'_>,
    name: &str,
) -> Result<Option<String>, IoError> {
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| IoError::Xml(error.to_string()))?;
        if attribute.key.local_name().as_ref() == name {
            return Ok(Some(
                attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map_err(|error| IoError::Xml(error.to_string()))?
                    .to_string(),
            ));
        }
    }
    Ok(None)
}

fn docx_styles(bytes: &[u8]) -> Result<HashMap<String, usize>, IoError> {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut styles = HashMap::new();
    let mut style_id: Option<String> = None;
    let mut xml_depth = 0usize;
    let mut root_count = 0usize;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(quick_xml::events::Event::Start(event)) => {
                if xml_depth == 0 {
                    root_count += 1;
                }
                xml_depth += 1;
                if event.local_name().as_ref() == "style" {
                    style_id = xml_style_id(&event)?;
                }
            }
            Ok(quick_xml::events::Event::Empty(event)) => {
                if xml_depth == 0 {
                    root_count += 1;
                }
                if event.local_name().as_ref() == "style" {
                    let id = xml_style_id(&event)?;
                    if let Some(id) = id
                        && let Some(level) = heading_style_level(&id)
                    {
                        styles.insert(id, level);
                    }
                } else if event.local_name().as_ref() == "outlineLvl"
                    && let (Some(id), Some(value)) = (style_id.as_ref(), xml_value(&event)?)
                    && let Ok(level) = value.parse::<usize>()
                    && level < 9
                {
                    styles.insert(id.clone(), level + 1);
                }
            }
            Ok(quick_xml::events::Event::End(event)) => {
                if xml_depth == 0 {
                    return Err(IoError::Invalid("malformed DOCX styles XML".into()));
                }
                xml_depth -= 1;
                if event.local_name().as_ref() == "style" {
                    style_id = None;
                }
            }
            Ok(quick_xml::events::Event::DocType(_)) => {
                return Err(IoError::Invalid(
                    "DOCX styles must not contain a document type declaration".into(),
                ));
            }
            Ok(quick_xml::events::Event::Eof) => {
                if xml_depth != 0 || root_count != 1 {
                    return Err(IoError::Invalid("malformed DOCX styles XML".into()));
                }
                break;
            }
            Err(error) => return Err(IoError::Xml(error.to_string())),
            _ => {}
        }
        buffer.clear();
    }
    Ok(styles)
}

fn heading_style_level(style: &str) -> Option<usize> {
    if style.eq_ignore_ascii_case("Title") {
        return Some(1);
    }
    style
        .strip_prefix("Heading")
        .or_else(|| style.strip_prefix("heading"))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|level| (1..=9).contains(level))
}

fn docx_paragraphs(bytes: &[u8]) -> Result<(Vec<DocxParagraph>, BTreeSet<ImportWarning>), IoError> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut paragraphs = Vec::new();
    let mut current: Option<DocxParagraph> = None;
    let mut text_depth = 0usize;
    let mut warnings = BTreeSet::new();
    warnings.insert(ImportWarning::Formatting);
    let mut xml_depth = 0usize;
    let mut root_count = 0usize;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                if xml_depth == 0 {
                    root_count += 1;
                }
                xml_depth += 1;
                let name = event.local_name();
                match name.as_ref() {
                    "p" => {
                        if current.is_some() {
                            return Err(IoError::Invalid(
                                "DOCX paragraphs are nested unexpectedly".into(),
                            ));
                        }
                        current = Some(DocxParagraph::default());
                    }
                    "t" if current.is_some() => text_depth += 1,
                    "pStyle" if current.is_some() => {
                        if let Some(value) = xml_value(&event)? {
                            current.as_mut().unwrap().style = Some(value);
                        }
                    }
                    "numId" if current.is_some() => {
                        if let Some(value) = xml_value(&event)? {
                            current.as_mut().unwrap().numbering = value.parse().ok();
                        }
                    }
                    "ilvl" if current.is_some() => {
                        if let Some(value) = xml_value(&event)?
                            && let Ok(level) = value.parse::<usize>()
                        {
                            if level > 2000 {
                                return Err(IoError::Invalid(
                                    "DOCX list nesting exceeds 2000 levels".into(),
                                ));
                            }
                            current.as_mut().unwrap().list_level = level;
                        }
                    }
                    "rPr" | "tbl" => {
                        warnings.insert(ImportWarning::Formatting);
                    }
                    "hyperlink" if current.is_some() => {
                        warnings.insert(ImportWarning::Links);
                    }
                    "drawing" | "pict" | "object" | "altChunk" => {
                        warnings.insert(ImportWarning::Resources);
                    }
                    "delText" | "instrText" => {
                        warnings.insert(ImportWarning::Other);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(event)) => {
                if xml_depth == 0 {
                    root_count += 1;
                }
                let name = event.local_name();
                match name.as_ref() {
                    "p" => {
                        paragraphs.push(DocxParagraph::default());
                    }
                    "pStyle" if current.is_some() => {
                        if let Some(value) = xml_value(&event)? {
                            current.as_mut().unwrap().style = Some(value);
                        }
                    }
                    "numId" if current.is_some() => {
                        if let Some(value) = xml_value(&event)? {
                            current.as_mut().unwrap().numbering = value.parse().ok();
                        }
                    }
                    "ilvl" if current.is_some() => {
                        if let Some(value) = xml_value(&event)?
                            && let Ok(level) = value.parse::<usize>()
                        {
                            if level > 2000 {
                                return Err(IoError::Invalid(
                                    "DOCX list nesting exceeds 2000 levels".into(),
                                ));
                            }
                            current.as_mut().unwrap().list_level = level;
                        }
                    }
                    "tab" | "br" | "cr" if current.is_some() => {
                        current.as_mut().unwrap().text.push(' ');
                    }
                    "rPr" | "tbl" => {
                        warnings.insert(ImportWarning::Formatting);
                    }
                    "drawing" | "pict" | "object" | "altChunk" => {
                        warnings.insert(ImportWarning::Resources);
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(event)) if text_depth > 0 => {
                if let Some(paragraph) = current.as_mut() {
                    let decoded = event.xml_content(quick_xml::XmlVersion::Implicit1_0);
                    paragraph.text.push_str(&decoded);
                }
            }
            Ok(Event::GeneralRef(event)) if text_depth > 0 => {
                if let Some(paragraph) = current.as_mut() {
                    if let Some(character) = event
                        .resolve_char_ref()
                        .map_err(|error| IoError::Xml(error.to_string()))?
                    {
                        paragraph.text.push(character);
                    } else {
                        let reference = format!("&{};", event.as_ref());
                        let decoded = quick_xml::escape::unescape(&reference)
                            .map_err(|error| IoError::Xml(error.to_string()))?;
                        paragraph.text.push_str(&decoded);
                    }
                }
            }
            Ok(Event::End(event)) => {
                if xml_depth == 0 {
                    return Err(IoError::Invalid("malformed DOCX document XML".into()));
                }
                xml_depth -= 1;
                match event.local_name().as_ref() {
                    "t" => text_depth = text_depth.saturating_sub(1),
                    "p" => {
                        if let Some(paragraph) = current.take() {
                            paragraphs.push(paragraph);
                            if paragraphs.len() > 100_000 {
                                return Err(IoError::Invalid(
                                    "DOCX contains too many paragraphs".into(),
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::DocType(_)) => {
                return Err(IoError::Invalid(
                    "DOCX document must not contain a document type declaration".into(),
                ));
            }
            Ok(Event::Eof) => {
                if xml_depth != 0 || root_count != 1 || current.is_some() {
                    return Err(IoError::Invalid("malformed DOCX document XML".into()));
                }
                break;
            }
            Err(error) => return Err(IoError::Xml(error.to_string())),
            _ => {}
        }
        buffer.clear();
    }
    Ok((paragraphs, warnings))
}

pub(crate) fn docx_import(bytes: &[u8]) -> Result<ImportReport, IoError> {
    const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
    const MAX_DOCUMENT_XML_BYTES: u64 = 16 * 1024 * 1024;
    const MAX_STYLES_XML_BYTES: u64 = 8 * 1024 * 1024;

    if bytes.len() > MAX_ARCHIVE_BYTES || !bytes.starts_with(b"PK\x03\x04") {
        return Err(IoError::Invalid("invalid or oversized DOCX archive".into()));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let names = crate::validate_document_archive(&mut archive)?;
    if !names.contains("word/document.xml") {
        return Err(IoError::Invalid(
            "DOCX archive is missing word/document.xml".into(),
        ));
    }
    let document =
        crate::zip_entry_with_limit(&mut archive, "word/document.xml", MAX_DOCUMENT_XML_BYTES)?;
    let styles = if names.contains("word/styles.xml") {
        docx_styles(&crate::zip_entry_with_limit(
            &mut archive,
            "word/styles.xml",
            MAX_STYLES_XML_BYTES,
        )?)?
    } else {
        HashMap::new()
    };
    let (paragraphs, mut warnings) = docx_paragraphs(&document)?;

    for path in &names {
        if path.starts_with("word/media/") || path.starts_with("word/embeddings/") {
            warnings.insert(ImportWarning::Resources);
        } else if path.starts_with("word/header")
            || path.starts_with("word/footer")
            || path.starts_with("word/footnotes")
            || path.starts_with("word/endnotes")
        {
            warnings.insert(ImportWarning::Notes);
        } else if !matches!(
            path.as_str(),
            "[Content_Types].xml"
                | "_rels/.rels"
                | "docProps/app.xml"
                | "docProps/core.xml"
                | "word/document.xml"
                | "word/_rels/document.xml.rels"
                | "word/styles.xml"
                | "word/numbering.xml"
                | "word/settings.xml"
                | "word/theme/theme1.xml"
                | "word/fontTable.xml"
                | "word/webSettings.xml"
        ) && !path.ends_with('/')
        {
            warnings.insert(ImportWarning::Other);
        }
    }

    let mut first_content = None;
    for (index, paragraph) in paragraphs.iter().enumerate() {
        if !paragraph.text.trim().is_empty() {
            first_content = Some(index);
            break;
        }
    }
    let first_heading = first_content
        .and_then(|index| paragraphs.get(index))
        .and_then(|paragraph| {
            paragraph.style.as_deref().and_then(|style| {
                styles
                    .get(style)
                    .copied()
                    .or_else(|| heading_style_level(style))
            })
        })
        .filter(|level| *level == 1);
    let root_title = first_heading
        .and(first_content)
        .map(|index| paragraphs[index].text.trim().to_owned())
        .unwrap_or_else(|| "Document".to_owned());

    let mut items = Vec::new();
    let mut heading_depth = 0usize;
    for (index, paragraph) in paragraphs.into_iter().enumerate() {
        let title = paragraph.text.trim();
        if title.is_empty() {
            continue;
        }
        let heading = paragraph.style.as_deref().and_then(|style| {
            styles
                .get(style)
                .copied()
                .or_else(|| heading_style_level(style))
        });
        if first_heading.is_some() && Some(index) == first_content {
            continue;
        }
        if let Some(level) = heading {
            heading_depth = level.saturating_sub(1);
            items.push((heading_depth, title.to_owned()));
        } else if paragraph.numbering.is_some_and(|numbering| numbering != 0) {
            warnings.insert(ImportWarning::Formatting);
            let depth = heading_depth
                .saturating_add(1)
                .saturating_add(paragraph.list_level);
            items.push((depth, title.to_owned()));
        } else {
            items.push((heading_depth.saturating_add(1), title.to_owned()));
        }
        if items.len() > 100_000 {
            return Err(IoError::Invalid("DOCX contains too many topics".into()));
        }
    }

    let tree = thinoda_core::outline::build_tree(&items);
    let root = OutlineNode::new(root_title).with_children(tree);
    Ok(ImportReport {
        format: crate::Format::Word,
        workbook: Workbook::new(thinoda_core::outline::sheet_from_outline(
            &root.title,
            &root,
        )),
        warnings: warnings.into_iter().collect(),
    })
}

fn entries(w: &Workbook, o: &ExportOptions) -> Vec<TopicRow> {
    let s = sheet(w, o);
    let mut out = Vec::new();
    let mut stack = vec![(s.root, 0usize, 0usize)];
    while let Some((id, depth, level)) = stack.pop() {
        if level > 2000 {
            continue;
        }

        let t = &s.topics[&id];
        out.push((
            depth,
            id,
            t.title.clone(),
            t.notes.as_deref().map(str::to_owned),
            t.labels.join(", "),
            t.markers.join(", "),
            t.hyperlink.clone().unwrap_or_default(),
        ));
        for c in t.children.iter().rev() {
            stack.push((*c, depth + 1, level + 1));
        }
    }
    out
}
fn quote(s: &str) -> String {
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}
pub(crate) fn csv(w: &Workbook, o: &ExportOptions) -> Vec<u8> {
    let rows = entries(w, o);
    let max = rows.iter().map(|x| x.0).max().unwrap_or(0);
    let mut out = String::new();
    for d in 0..=max {
        if d > 0 {
            out.push(',');
        }
        out.push_str(&format!("Depth {}", d + 1));
    }
    out.push_str(",Notes,Labels,Markers,Link\r\n");
    for (depth, _, title, note, labels, markers, link) in rows {
        let mut cols = vec![String::new(); max + 1];
        cols[depth] = title;
        cols.extend([note.unwrap_or_default(), labels, markers, link]);
        out.push_str(&cols.iter().map(|v| quote(v)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out.into_bytes()
}
fn zip(files: &[(&str, String)]) -> Result<Vec<u8>, IoError> {
    let mut c = Cursor::new(Vec::new());
    let mut z = ZipWriter::new(&mut c);
    let opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (n, v) in files {
        z.start_file(*n, opt)?;
        z.write_all(v.as_bytes())?;
    }
    z.finish()?;
    Ok(c.into_inner())
}
pub(crate) fn docx(w: &Workbook, o: &ExportOptions) -> Result<Vec<u8>, IoError> {
    let mut body = String::new();
    let mut notes = crate::office_notes::WordNotes::default();
    for (depth, id, title, note, labels, markers, _) in entries(w, o) {
        let (pstyle, heading) = if depth < 6 {
            (
                format!("<w:pPr><w:pStyle w:val=\"Heading{}\"/></w:pPr>", depth + 1),
                title,
            )
        } else {
            (String::new(), format!("* {title}"))
        };
        body.push_str(&format!(
            "<w:p>{pstyle}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            esc(&heading)
        ));
        if let Some(n) = note {
            if sheet(w, o).topics[&id]
                .notes
                .as_ref()
                .is_some_and(|note| note.is_markdown())
            {
                let document = thinoda_core::notes::formatted_document(&n).map_err(|reason| {
                    IoError::WordNote {
                        topic: sheet(w, o).topics[&id].title.clone(),
                        reason,
                    }
                })?;
                notes.render(&document, &mut body);
            } else {
                body.push_str(&format!(
                    "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                    esc(&n)
                ));
            }
        }
        let extra = [labels, markers]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
        if !extra.is_empty() {
            body.push_str(&format!("<w:p><w:r><w:t>{}</w:t></w:r></w:p>", esc(&extra)));
        }
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:body></w:document>"
    );
    let mut styles = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    );
    for i in 1..=6 {
        styles.push_str(&format!("<w:style w:type=\"paragraph\" w:styleId=\"Heading{i}\"><w:name w:val=\"heading {i}\"/><w:basedOn w:val=\"Normal\"/><w:uiPriority w:val=\"{i}\"/><w:qFormat/><w:pPr><w:outlineLvl w:val=\"{}\"/></w:pPr></w:style>", i - 1));
    }
    styles.push_str("</w:styles>");
    let mut types = "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>".to_owned();
    let mut relationships = "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>".to_owned();
    let mut files = vec![
        ("_rels/.rels", "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>".into()),
        ("word/document.xml", document),
        ("word/styles.xml", styles),
    ];
    if let Some(numbering) = notes.numbering() {
        types.push_str("<Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/>");
        relationships.push_str("<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/>");
        files.push(("word/numbering.xml", numbering));
    }
    types.push_str("</Types>");
    relationships.push_str("</Relationships>");
    files.push(("[Content_Types].xml", types));
    files.push(("word/_rels/document.xml.rels", relationships));
    zip(&files)
}
fn col_name(n: usize) -> String {
    let mut n = n + 1;
    let mut s = String::new();
    while n > 0 {
        let r = (n - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    s
}
pub(crate) fn xlsx(w: &Workbook, o: &ExportOptions) -> Result<Vec<u8>, IoError> {
    let rows = entries(w, o);
    let depth = rows.iter().map(|r| r.0).max().unwrap_or(0) + 1;
    let extra = ["Notes", "Labels", "Markers", "Link"];
    let mut sheetdata = String::from("<row r=\"1\">");
    for i in 0..depth {
        sheetdata.push_str(&format!(
            "<c r=\"{}1\" t=\"inlineStr\"><is><t>Depth {}</t></is></c>",
            col_name(i),
            i + 1
        ));
    }
    for (i, h) in extra.iter().enumerate() {
        sheetdata.push_str(&format!(
            "<c r=\"{}1\" t=\"inlineStr\"><is><t>{h}</t></is></c>",
            col_name(depth + i)
        ));
    }
    sheetdata.push_str("</row>");
    for (ri, (d, _, title, note, labels, markers, link)) in rows.into_iter().enumerate() {
        let row = ri + 2;
        sheetdata.push_str(&format!("<row r=\"{row}\">"));
        let values = [
            (d, title),
            (depth, note.unwrap_or_default()),
            (depth + 1, labels),
            (depth + 2, markers),
            (depth + 3, link),
        ];
        for (col, value) in values {
            sheetdata.push_str(&format!(
                "<c r=\"{}{row}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                col_name(col),
                esc(&value)
            ));
        }
        sheetdata.push_str("</row>");
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheetData>{sheetdata}</sheetData></worksheet>"
    );
    let workbook = "<?xml version=\"1.0\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"Mind Map\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>";
    zip(&[("[Content_Types].xml", "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>".into()), ("_rels/.rels", "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>".into()), ("xl/workbook.xml", workbook.into()), ("xl/_rels/workbook.xml.rels", "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>".into()), ("xl/worksheets/sheet1.xml", xml)])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_quotes_comma_and_quote() {
        let w = Workbook::new(thinoda_core::Sheet::new("S", "A, \"B\""));
        let out = String::from_utf8(csv(&w, &ExportOptions::default())).unwrap();
        assert!(out.contains(r#""A, ""B""""#));
    }
}
