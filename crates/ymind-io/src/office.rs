use crate::{ExportOptions, IoError};
use std::io::{Cursor, Write};
use ymind_core::{TopicId, Workbook};
use zip::{ZipWriter, write::SimpleFileOptions};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn sheet<'a>(w: &'a Workbook, o: &ExportOptions) -> &'a ymind_core::Sheet {
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
            t.notes.clone(),
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
    for (depth, _, title, note, labels, markers, _) in entries(w, o) {
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
            body.push_str(&format!(
                "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                esc(&n)
            ));
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
    zip(&[("[Content_Types].xml", "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/></Types>".into()), ("_rels/.rels", "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>".into()), ("word/document.xml", document), ("word/_rels/document.xml.rels", "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>".into()), ("word/styles.xml", styles)])
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
        let w = Workbook::new(ymind_core::Sheet::new("S", "A, \"B\""));
        let out = String::from_utf8(csv(&w, &ExportOptions::default())).unwrap();
        assert!(out.contains(r#""A, ""B""""#));
    }
}
