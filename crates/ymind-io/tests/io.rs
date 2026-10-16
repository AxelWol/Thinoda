use std::io::{Cursor, Write};
use ymind_core::{
    Boundary, ChildRange, Color, Relationship, Sheet, Summary, Topic, TopicShape, Vec2, Workbook,
};
use ymind_io::{ExportOptions, Format, detect_format, export_bytes, import_bytes};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn sample() -> Workbook {
    let mut s = Sheet::new("First sheet", "Central & topic");
    let a = s.add_child(s.root, None, "Branch <A>");
    let b = s.add_child(s.root, None, "Branch B");
    let child = s.add_child(a, None, "Nested");
    {
        let t = s.topic_mut(a).unwrap();
        t.notes = Some("Notes & details".into());
        t.hyperlink = Some("https://example.com/?a=1&b=2".into());
        t.labels = vec!["work".into(), "important".into()];
        t.markers = vec!["priority-1".into(), "flag-red".into()];
        t.collapsed = true;
        t.style.fill = Some(Color::rgb(10, 20, 30));
        t.style.shape = Some(TopicShape::Diamond);
        t.style.bold = Some(true);
    }
    s.topic_mut(child).unwrap().style.font_size = Some(18.0);
    let floating = s.add_floating("Floating", Vec2::new(200.0, -30.0));
    s.topic_mut(floating).unwrap().notes = Some("float note".into());
    let rid = s.alloc_relationship_id();
    s.relationships.push(Relationship {
        id: rid,
        from: a,
        to: b,
        title: "connects".into(),
        color: Some(Color::rgb(1, 2, 3)),
    });
    let bid = s.alloc_boundary_id();
    s.boundaries.push(Boundary {
        id: bid,
        parent: s.root,
        range: Some(ChildRange { start: 0, end: 1 }),
        title: Some("Group".into()),
        color: Some(Color::rgb(3, 4, 5)),
    });
    let summary_topic = s.alloc_topic_id();
    s.topics
        .insert(summary_topic, Topic::new(summary_topic, "Summary"));
    let sid = s.alloc_summary_id();
    s.summaries.push(Summary {
        id: sid,
        parent: s.root,
        range: ChildRange { start: 0, end: 1 },
        topic: summary_topic,
    });
    let mut w = Workbook::new(s);
    w.sheets.push(Sheet::new("Second", "Another root"));
    w.active_sheet = 1;
    w
}
fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut c = Cursor::new(Vec::new());
    let mut z = ZipWriter::new(&mut c);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        z.start_file(*name, opts).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
    c.into_inner()
}

#[test]
fn native_archive_roundtrips_every_model_field() {
    let w = sample();
    let bytes = export_bytes(&w, Format::YMind, &Default::default()).unwrap();
    let restored = import_bytes(Format::YMind, &bytes).unwrap();
    assert_eq!(restored, w);
}
#[test]
fn native_rejects_newer_major_version() {
    let manifest = br#"{"format":"ymind","version":2}"#;
    let bytes = zip_with(&[("manifest.json", manifest)]);
    assert!(matches!(
        import_bytes(Format::YMind, &bytes),
        Err(ymind_io::IoError::NewerVersion(_))
    ));
}
#[test]
fn xmind_export_import_roundtrips_model() {
    let w = sample();
    let bytes = export_bytes(&w, Format::XMind, &Default::default()).unwrap();
    let restored = import_bytes(Format::XMind, &bytes).unwrap();
    assert_eq!(restored, w);
}

#[test]
fn xmind_export_uses_xmind_metadata_and_public_content() {
    // XMind mutates creator.name while loading; a string creator rejects the archive.
    let bytes = export_bytes(&sample(), Format::XMind, &Default::default()).unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let metadata: serde_json::Value =
        serde_json::from_reader(archive.by_name("metadata.json").unwrap()).unwrap();
    assert_eq!(metadata["creator"]["name"], "yMind");
    assert!(metadata["creator"]["version"].is_string());
    assert_eq!(metadata["dataStructureVersion"], "2");
    let manifest: serde_json::Value =
        serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
    assert!(manifest["file-entries"]["content.json"].is_object());
    assert!(manifest["file-entries"]["metadata.json"].is_object());

    let mut content = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("content.json").unwrap(), &mut content)
        .unwrap();
    // Do not let the private lossless workbook bypass parsing the exported XMind tree.
    let public_archive = zip_with(&[("content.json", &content)]);
    let restored = import_bytes(Format::XMind, &public_archive).unwrap();
    assert_eq!(restored.sheets.len(), 2);
    let sheet = &restored.sheets[0];
    assert_eq!(sheet.root_topic().title, "Central & topic");
    let branch = sheet.topic(sheet.root_topic().children[0]).unwrap();
    assert_eq!(branch.title, "Branch <A>");
    assert_eq!(branch.notes.as_deref(), Some("Notes & details"));
    assert_eq!(branch.markers, vec!["priority-1", "flag-red"]);
    assert_eq!(branch.labels, vec!["work", "important"]);
    assert_eq!(sheet.floating.len(), 1);
    assert_eq!(sheet.relationships.len(), 1);
    assert_eq!(sheet.boundaries.len(), 1);
    assert_eq!(sheet.summaries.len(), 1);
}

#[test]
fn xmind_json_imports_tree_and_metadata() {
    let content=br##"[{"id":"s1","class":"sheet","title":"Fixture","rootTopic":{"id":"r","class":"topic","title":"Root","structureClass":"org.xmind.ui.logic.right","children":{"attached":[{"id":"a","class":"topic","title":"Child","markers":[{"markerId":"priority-2"}],"labels":["label"],"notes":{"plain":{"content":"memo"}},"href":"https://example.org","branch":"folded","style":{"properties":{"svg:fill":"#112233","fo:font-weight":"bold"}}}]}}}]"##;
    let bytes = zip_with(&[("content.json", content)]);
    let w = import_bytes(Format::XMind, &bytes).unwrap();
    assert_eq!(w.sheets[0].title, "Fixture");
    let t = &w.sheets[0].topics[&w.sheets[0].topics[&w.sheets[0].root].children[0]];
    assert_eq!(t.title, "Child");
    assert_eq!(t.notes.as_deref(), Some("memo"));
    assert_eq!(t.markers, vec!["priority-2"]);
    assert_eq!(t.style.fill, Some(Color::rgb(17, 34, 51)));
}
#[test]
fn xmind_json_imports_boundaries_and_summaries() {
    let content = br##"[{"title":"S","rootTopic":{"id":"r","title":"Root","children":{"attached":[{"id":"a","title":"A"}],"summary":[{"id":"sum","title":"A summary"}]},"boundaries":[{"id":"bd","range":"(0, 0)","title":"Group"}],"summaries":[{"id":"sm","range":"(0, 0)","topicId":"sum"}]}}]"##;
    let bytes = zip_with(&[("content.json", content)]);
    let workbook = import_bytes(Format::XMind, &bytes).unwrap();
    let sheet = &workbook.sheets[0];
    assert_eq!(sheet.boundaries.len(), 1);
    assert_eq!(sheet.boundaries[0].title.as_deref(), Some("Group"));
    assert_eq!(sheet.summaries.len(), 1);
    assert_eq!(sheet.topics[&sheet.summaries[0].topic].title, "A summary");
}
#[test]
fn legacy_xmind_xml_imports_topic_titles() {
    let xml=br#"<?xml version="1.0"?><xmap-content xmlns:xlink="http://www.w3.org/1999/xlink"><sheet title="Legacy"><topic structure-class="org.xmind.ui.logic.right" branch="folded"><title>Root</title><children><topics type="attached"><topic><title>Child</title><marker-refs><marker-ref marker-id="priority-3"/></marker-refs><notes><plain>Memo</plain></notes><labels><label>label</label></labels></topic></topics></children></topic></sheet></xmap-content>"#;
    let w = import_bytes(Format::XMind, xml).unwrap();
    let s = &w.sheets[0];
    assert_eq!(s.root_topic().title, "Root");
    assert!(s.root_topic().collapsed);
    assert_eq!(s.structure, ymind_core::Structure::LogicRight);
    let child = &s.topics[&s.root_topic().children[0]];
    assert_eq!(child.title, "Child");
    assert_eq!(child.notes.as_deref(), Some("Memo"));
    assert_eq!(child.labels, vec!["label"]);
    assert_eq!(child.markers, vec!["priority-3"]);
}

#[test]
fn free_mind_opml_markdown_and_text_import() {
    let mm=br#"<map version="1.0.1"><node TEXT="Root"><node TEXT="A" FOLDED="true" LINK="https://a"><icon BUILTIN="full-2"/></node></node></map>"#;
    let w = import_bytes(Format::FreeMind, mm).unwrap();
    assert_eq!(
        w.sheets[0].topics[&w.sheets[0].root_topic().children[0]].title,
        "A"
    );
    let opml=br#"<opml version="2.0"><head><title>Outline</title></head><body><outline text="Root"><outline text="Child" _note="memo"/></outline></body></opml>"#;
    let w = import_bytes(Format::Opml, opml).unwrap();
    assert_eq!(w.sheets[0].root_topic().title, "Root");
    assert_eq!(w.sheets[0].root_topic().children.len(), 1);
    let md = import_bytes(Format::Markdown, b"# Root\n\n## One\n\n### Nested\n").unwrap();
    assert_eq!(md.sheets[0].root_topic().title, "Root");
    let text = import_bytes(Format::PlainText, b"Root\n\tChild\n\t\tNested\n").unwrap();
    assert_eq!(text.sheets[0].topic_count(), 3);
}
#[test]
fn mindmanager_import_from_document_xml() {
    let xml=br#"<ap:Map xmlns:ap="urn:test"><ap:OneTopic><ap:Topic><ap:Text PlainText="Root"/><ap:NotesGroup><ap:NotesXhtmlData PreviewPlainText="root note"/></ap:NotesGroup><ap:SubTopics><ap:Topic><ap:Text PlainText="Child"/><ap:Hyperlink Url="https://example.org"/></ap:Topic></ap:SubTopics></ap:Topic></ap:OneTopic></ap:Map>"#;
    let bytes = zip_with(&[("Document.xml", xml)]);
    let w = import_bytes(Format::MindManager, &bytes).unwrap();
    let sheet = &w.sheets[0];
    assert_eq!(sheet.root_topic().title, "Root");
    assert_eq!(sheet.root_topic().notes.as_deref(), Some("root note"));
    let child = &sheet.topics[&sheet.root_topic().children[0]];
    assert_eq!(child.title, "Child");
    assert_eq!(child.hyperlink.as_deref(), Some("https://example.org"));
}
#[test]
fn svg_png_jpeg_pdf_and_office_exports_are_valid() {
    let w = sample();
    let opts = ExportOptions {
        sheet: Some(0),
        scale: 1.5,
        ..Default::default()
    };
    let svg = export_bytes(&w, Format::Svg, &opts).unwrap();
    let mut reader = quick_xml::Reader::from_reader(svg.as_slice());
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => panic!("bad svg: {e}"),
            _ => {}
        }
        buf.clear();
    }
    let source = String::from_utf8(svg).unwrap();
    assert!(source.contains("Central &amp; topic"));
    assert!(source.contains("Branch &lt;A&gt;"));
    for f in [Format::Png, Format::Jpeg] {
        let bytes = export_bytes(&w, f, &opts).unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        assert!(img.width() > 0 && img.height() > 0);
    }
    assert!(
        export_bytes(&w, Format::Pdf, &opts)
            .unwrap()
            .starts_with(b"%PDF-")
    );
    for (f, entry) in [
        (Format::Word, "word/document.xml"),
        (Format::Excel, "xl/worksheets/sheet1.xml"),
    ] {
        let bytes = export_bytes(&w, f, &opts).unwrap();
        let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut file = z.by_name(entry).unwrap();
        let mut text = String::new();
        use std::io::Read;
        file.read_to_string(&mut text).unwrap();
        assert!(text.contains("Central &amp; topic"));
    }
    assert!(
        String::from_utf8(export_bytes(&w, Format::Csv, &opts).unwrap())
            .unwrap()
            .contains("Branch <A>")
    );
}
#[test]
fn detection_supports_paths_and_signatures() {
    assert_eq!(
        Format::from_path(std::path::Path::new("a.xmind")),
        Some(Format::XMind)
    );
    assert_eq!(
        detect_format(std::path::Path::new("unknown"), b"<opml version=\"2.0\"/>"),
        Some(Format::Opml)
    );
    assert_eq!(
        detect_format(std::path::Path::new("unknown"), b"<map version=\"1.0.1\"/>"),
        Some(Format::FreeMind)
    );
}
#[test]
fn importers_do_not_panic_on_truncated_or_random_bytes() {
    let formats = [
        Format::YMind,
        Format::XMind,
        Format::FreeMind,
        Format::Opml,
        Format::Markdown,
        Format::PlainText,
        Format::MindManager,
    ];
    for format in formats {
        for bytes in [
            &[][..],
            b"PK\x03\x04".as_slice(),
            b"<broken".as_slice(),
            b"not a valid document".as_slice(),
            &[0, 1, 2, 255, 128][..],
        ] {
            let result = std::panic::catch_unwind(|| import_bytes(format, bytes));
            assert!(result.is_ok(), "panic for {format:?}");
        }
    }
}
