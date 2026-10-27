use std::io::{Cursor, Write};

use thinoda_core::{OutlineNode, Workbook};
use thinoda_io::{ExportOptions, Format, ImportWarning, export_bytes, import_bytes_with_report};
use zip::{ZipWriter, write::SimpleFileOptions};

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn topic_depths(workbook: &Workbook) -> Vec<(usize, String)> {
    fn visit(
        sheet: &thinoda_core::Sheet,
        id: thinoda_core::TopicId,
        depth: usize,
        topics: &mut Vec<(usize, String)>,
    ) {
        let topic = &sheet.topics[&id];
        topics.push((depth, topic.title.clone()));
        for child in &topic.children {
            visit(sheet, *child, depth + 1, topics);
        }
    }

    let sheet = workbook.active();
    let mut topics = Vec::new();
    visit(sheet, sheet.root, 0, &mut topics);
    topics
}

fn docx(document: &[u8], styles: Option<&[u8]>, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut entries = vec![
        ("[Content_Types].xml", b"<Types/>".as_slice()),
        ("word/document.xml", document),
    ];
    if let Some(styles) = styles {
        entries.push(("word/styles.xml", styles));
    }
    entries.extend_from_slice(extra);
    archive(&entries)
}

#[test]
fn textbundle_export_import_preserves_topic_hierarchy() {
    let root = OutlineNode::new("Root").with_children(vec![
        OutlineNode::new("Child").with_children(vec![OutlineNode::new("Grandchild")]),
    ]);
    let sheet = thinoda_core::outline::sheet_from_outline("Ideas", &root);
    let workbook = Workbook::new(sheet);

    let bytes = export_bytes(&workbook, Format::TextBundle, &ExportOptions::default()).unwrap();
    let report = import_bytes_with_report(Format::TextBundle, &bytes).unwrap();

    assert_eq!(
        topic_depths(&report.workbook),
        [
            (0, "Root".into()),
            (1, "Child".into()),
            (2, "Grandchild".into())
        ]
    );
    assert!(report.warnings.is_empty());
}

#[test]
fn textbundle_accepts_root_layout_and_reports_lossy_markdown_and_assets() {
    let info = br#"{"version":1,"type":"text/markdown"}"#;
    let markdown = b"# Root\n\n## Child **bold**\n\n[link](https://example.test)\n\n![image](assets/image.png)\n";
    let bytes = archive(&[
        ("info.json", info),
        ("text.md", markdown),
        ("assets/image.png", b"not decoded"),
    ]);

    let report = import_bytes_with_report(Format::TextBundle, &bytes).unwrap();

    assert!(report.warnings.contains(&ImportWarning::Formatting));
    assert!(report.warnings.contains(&ImportWarning::Resources));
    assert!(report.warnings.contains(&ImportWarning::Links));
    assert_eq!(topic_depths(&report.workbook)[0], (0, "Root".into()));
}

#[test]
fn textbundle_rejects_bad_metadata_invalid_utf8_and_unsafe_paths() {
    let wrong_type = archive(&[
        ("info.json", br#"{"version":2,"type":"text/html"}"#),
        ("text.md", b"# Root"),
    ]);
    assert!(import_bytes_with_report(Format::TextBundle, &wrong_type).is_err());

    let invalid_utf8 = archive(&[
        ("info.json", br#"{"version":2,"type":"text/markdown"}"#),
        ("text.md", b"\xff"),
    ]);
    assert!(import_bytes_with_report(Format::TextBundle, &invalid_utf8).is_err());

    let unsafe_path = archive(&[
        (
            "bundle.textbundle/info.json",
            br#"{"version":2,"type":"text/markdown"}"#,
        ),
        ("bundle.textbundle/text.md", b"# Root"),
        ("bundle.textbundle/../escape", b"unsafe"),
    ]);
    assert!(import_bytes_with_report(Format::TextBundle, &unsafe_path).is_err());
}

#[test]
fn docx_import_maps_headings_lists_unicode_and_surfaces_losses() {
    let document = br#"
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>R&#248;ot &amp; Root</w:t></w:r></w:p>
            <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>Child</w:t></w:r></w:p>
            <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>List item</w:t></w:r></w:p>
            <w:p><w:hyperlink r:id="rId1"><w:r><w:t>Linked text</w:t></w:r></w:hyperlink></w:p>
          </w:body>
        </w:document>
    "#;
    let styles = br#"
        <w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:style w:type="paragraph" w:styleId="CustomHeading">
            <w:pPr><w:outlineLvl w:val="0"/></w:pPr>
          </w:style>
        </w:styles>
    "#;
    let relationships = br#"
        <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
          <Relationship Id="rId1" Type="hyperlink" Target="https://example.test" TargetMode="External"/>
        </Relationships>
    "#;
    let bytes = docx(
        document,
        Some(styles),
        &[("word/_rels/document.xml.rels", relationships)],
    );

    let report = import_bytes_with_report(Format::Word, &bytes).unwrap();

    assert_eq!(
        topic_depths(&report.workbook),
        [
            (0, "Røot & Root".into()),
            (1, "Child".into()),
            (2, "List item".into()),
            (2, "Linked text".into())
        ]
    );
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    assert!(report.warnings.contains(&ImportWarning::Links));
}

#[test]
fn docx_import_uses_custom_outline_styles_and_rejects_malformed_xml() {
    let document = br#"
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body><w:p><w:pPr><w:pStyle w:val="CustomHeading"/></w:pPr><w:r><w:t>Custom root</w:t></w:r></w:p></w:body>
        </w:document>
    "#;
    let styles = br#"
        <w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:style w:type="paragraph" w:styleId="CustomHeading">
            <w:pPr><w:outlineLvl w:val="0"/></w:pPr>
          </w:style>
        </w:styles>
    "#;
    let bytes = docx(document, Some(styles), &[]);

    let report = import_bytes_with_report(Format::Word, &bytes).unwrap();
    assert_eq!(topic_depths(&report.workbook), [(0, "Custom root".into())]);

    let malformed = docx(b"<w:document><w:body>", None, &[]);
    assert!(import_bytes_with_report(Format::Word, &malformed).is_err());
}
