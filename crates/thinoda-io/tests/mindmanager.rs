//! Original synthetic fixtures; no user map or vendor schema is redistributed.
use std::io::{Cursor, Read, Write};
use thinoda_core::{Sheet, TextAlign, TopicShape, Workbook};
use thinoda_io::{
    ExportOptions, Format, ImportWarning, IoError, detect_format, export_bytes,
    export_bytes_with_consent, import_bytes, import_bytes_with_report,
};

const NS: &str = "http://schemas.mindjet.com/MindManager/Application/2003";

fn xml(content: &str) -> Vec<u8> {
    format!("<ap:Map xmlns:ap=\"{NS}\"><ap:OneTopic>{content}</ap:OneTopic></ap:Map>").into_bytes()
}

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn document(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    assert_eq!(
        zip.len(),
        1,
        "no invented OPC parts or private workbook snapshot"
    );
    let mut xml = String::new();
    zip.by_name("Document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

#[test]
fn package_and_xml_import_preserve_every_root_and_order() {
    let source = xml(
        r#"<ap:Topic OId="AAAAAAAAAAAAAAAAAAAAAA=="><ap:Text PlainText="Root"/><ap:SubTopics><ap:Topic><ap:Text PlainText="First"/></ap:Topic><ap:Topic><ap:Text PlainText="Second"/></ap:Topic></ap:SubTopics></ap:Topic><ap:Topic><ap:Text PlainText="Another root"/></ap:Topic>"#,
    );
    for (format, bytes) in [
        (Format::MindManager, archive(&[("Document.xml", &source)])),
        (Format::MindManagerXml, source),
    ] {
        let book = import_bytes(format, &bytes).unwrap();
        assert_eq!(book.sheets.len(), 2);
        assert_eq!(book.sheets[1].root_topic().title, "Another root");
        let sheet = book.active();
        let children: Vec<_> = sheet
            .root_topic()
            .children
            .iter()
            .map(|id| sheet.topics[id].title.as_str())
            .collect();
        assert_eq!(children, ["First", "Second"]);
    }
}

#[test]
fn original_writer_has_schema_required_font_order_and_view_index() {
    let mut sheet = Sheet::new("Sheet", "Root");
    let child = sheet.add_child(sheet.root, None, "Child");
    sheet.topic_mut(child).unwrap().collapsed = true;
    let book = Workbook::new(sheet);
    let bytes = export_bytes(&book, Format::MindManager, &ExportOptions::default()).unwrap();
    assert_eq!(
        bytes,
        export_bytes(&book, Format::MindManager, &ExportOptions::default()).unwrap()
    );
    let xml = document(&bytes);
    // Independently specified grammar checks, not assertions through our importer.
    assert!(xml.contains(&format!("xmlns:ap=\"{NS}\"")));
    assert!(xml.contains("<ap:TopicViewGroup ViewIndex=\"0\"><ap:Collapsed Collapsed=\"true\"/></ap:TopicViewGroup><ap:Text PlainText=\"Child\"><ap:Font/></ap:Text>"));
    assert!(xml.find("<ap:SubTopics>").unwrap() < xml.find("PlainText=\"Root\"").unwrap());
    assert!(xml.contains("</ap:OneTopic><ap:DocumentGroup>"));
    assert!(xml.contains("<ap:Name Name=\"Sheet\"/>"));
    assert!(
        !xml.contains("OId"),
        "optional IDs and ID references must both be absent"
    );
    let restored = import_bytes(Format::MindManager, &bytes).unwrap();
    assert_eq!(book, restored);
}

#[test]
fn supported_fields_unicode_and_plain_notes_roundtrip() {
    let mut sheet = Sheet::new("Feuille 日本語", "Grüße <&> 🧠");
    let root = sheet.root;
    let topic = sheet.topic_mut(root).unwrap();
    topic.title.push_str("\nLine\tTwo");
    topic.notes = Some("Line <&> \"quotes\"\n\nNext 日本語\tEnd\r".into());
    topic.hyperlink = Some("https://example.org/?a=1&b=2".into());
    topic.labels = vec!["étiquette <&>".into()];
    topic.markers = vec!["priority-9".into(), "task-3quar".into()];
    topic.style.font_family = Some("Arial".into());
    topic.style.font_size = Some(17.5);
    topic.style.bold = Some(true);
    topic.style.italic = Some(false);
    topic.style.underline = Some(true);
    topic.style.strikethrough = Some(false);
    topic.style.align = Some(TextAlign::Right);
    topic.style.shape = Some(TopicShape::Diamond);
    topic.style.fill = Some(thinoda_core::Color::rgb(255, 0, 0));
    topic.style.text_color = Some(thinoda_core::Color::WHITE);
    let mut book = Workbook::new(sheet);
    book.metadata.creator = "Original fixture".into();
    book.metadata.description = Some("Description\nsecond line".into());
    let bytes = export_bytes(&book, Format::MindManager, &ExportOptions::default()).unwrap();
    let report = import_bytes_with_report(Format::MindManager, &bytes).unwrap();
    assert_eq!(report.workbook, book);
    let xml = document(&bytes);
    assert!(xml.contains("urn:mindjet:Prio9"));
    assert!(xml.contains("TaskPercentage=\"75\""));
    assert!(xml.contains("FillColor=\"ffff0000\""));
    assert!(xml.contains("Color=\"ffffffff\""));
    assert!(xml.contains("&#10;"));
    assert!(!xml.contains("thinoda"));
}

#[test]
fn features_require_consent_but_best_effort_conversion_remains_available() {
    let source = xml(
        r#"<ap:Topic><ap:Text PlainText="Root"><ap:Font Bold="true"/></ap:Text><ap:Color FillColor="FF112233" LineColor="FF000000"/><ap:Task StartDate="2026-01-01T00:00:00"/><ap:OneImage/><ap:Hyperlink Url="https://example.org"/><ap:Hyperlink Url="mmarch://bin/file"/><ap:NotesGroup><ap:NotesXhtmlData PreviewPlainText="Preview"><html xmlns="http://www.w3.org/1999/xhtml"><body><p>Rich note</p></body></html></ap:NotesXhtmlData></ap:NotesGroup></ap:Topic>"#,
    );
    let bytes = archive(&[("Document.xml", &source), ("bin/asset", b"asset")]);
    assert!(matches!(
        import_bytes(Format::MindManager, &bytes),
        Err(IoError::ImportNeedsConfirmation(_))
    ));
    let report = import_bytes_with_report(Format::MindManager, &bytes).unwrap();
    for warning in [
        ImportWarning::Formatting,
        ImportWarning::Markers,
        ImportWarning::Resources,
        ImportWarning::Links,
    ] {
        assert!(report.warnings.contains(&warning), "{warning:?}");
    }
    assert_eq!(
        report.workbook.active().root_topic().notes.as_deref(),
        Some("Preview")
    );
    assert_eq!(
        report.workbook.active().root_topic().hyperlink.as_deref(),
        Some("https://example.org")
    );
}

#[test]
fn unsupported_export_is_not_silently_destructive_and_sheet_selection_is_explicit() {
    let mut book = Workbook::new(Sheet::new("First", "First"));
    book.sheets.push(Sheet::new("Second", "Second"));
    assert!(matches!(
        export_bytes(&book, Format::MindManager, &ExportOptions::default()),
        Err(IoError::MindManagerExportNeedsConfirmation)
    ));
    let options = ExportOptions {
        sheet: Some(1),
        ..Default::default()
    };
    let bytes = export_bytes(&book, Format::MindManager, &options).unwrap();
    assert_eq!(
        import_bytes(Format::MindManager, &bytes)
            .unwrap()
            .active()
            .root_topic()
            .title,
        "Second"
    );
    book.active_mut().root_topic();
    let root = book.active().root;
    book.active_mut().topic_mut(root).unwrap().image = Some("private-resource.png".into());
    assert!(export_bytes(&book, Format::MindManager, &ExportOptions::default()).is_err());
    assert!(
        export_bytes_with_consent(&book, Format::MindManager, &ExportOptions::default(), true)
            .is_ok()
    );
}

#[test]
fn rejects_unsafe_archives_and_malformed_xml_without_panicking() {
    let valid = xml("<ap:Topic><ap:Text PlainText=\"Root\"/></ap:Topic>");
    for name in [
        "../escape",
        "/absolute",
        "C:/drive",
        "bin\\escape",
        "bin/../escape",
        "bin//escape",
    ] {
        let bytes = archive(&[("Document.xml", &valid), (name, b"unused")]);
        assert!(
            import_bytes_with_report(Format::MindManager, &bytes).is_err(),
            "{name}"
        );
    }
    let duplicates = archive(&[("Document.xml", &valid), ("document.xml", &valid)]);
    assert!(import_bytes_with_report(Format::MindManager, &duplicates).is_err());
    for source in [
        b"<Map><OneTopic><Topic>".as_slice(),
        b"<!DOCTYPE Map [<!ENTITY bad SYSTEM \"file:///secret\">]><Map/>",
        b"<Map><OneTopic><Topic><Text PlainText=\"&unknown;\"/></Topic></OneTopic></Map>",
        b"<Map><OneTopic><Topic><Text PlainText=\"a\" PlainText=\"b\"/></Topic></OneTopic></Map>",
        b"<Map><OneTopic><Topic/></OneTopic></Map><Map/>",
    ] {
        assert!(import_bytes_with_report(Format::MindManagerXml, source).is_err());
    }
    let deep = format!(
        "<Map><OneTopic>{}<Topic/>{}</OneTopic></Map>",
        "<Topic><SubTopics>".repeat(256),
        "</SubTopics></Topic>".repeat(256)
    );
    assert!(import_bytes_with_report(Format::MindManagerXml, deep.as_bytes()).is_err());
    let mut oversized = archive(&[("Document.xml", &valid)]);
    let central = oversized
        .windows(4)
        .position(|b| b == b"PK\x01\x02")
        .unwrap();
    oversized[central + 24..central + 28].copy_from_slice(&(32u32 * 1024 * 1024 + 1).to_le_bytes());
    assert!(import_bytes_with_report(Format::MindManager, &oversized).is_err());
}

#[test]
fn namespace_spoofing_cannot_replace_topic_data() {
    let source = xml(
        r#"<ap:Topic><ap:Text PlainText="Real"/><ap:SubTopics xmlns:ap="urn:foreign"><ap:Topic><ap:Text PlainText="Spoof"/></ap:Topic></ap:SubTopics></ap:Topic>"#,
    );
    let report = import_bytes_with_report(Format::MindManagerXml, &source).unwrap();
    assert_eq!(report.workbook.active().topic_count(), 1);
    assert_eq!(report.workbook.active().root_topic().title, "Real");
    assert!(!report.warnings.is_empty());
    let mixed = xml(
        r#"<ap:Topic><ap:Text PlainText="First"/><Text PlainText="Legacy override"/></ap:Topic>"#,
    );
    assert!(matches!(
        import_bytes(Format::MindManagerXml, &mixed),
        Err(IoError::ImportNeedsConfirmation(_))
    ));
}

#[test]
fn extension_filters_and_signature_detection_distinguish_raw_xml_from_package() {
    let source = xml("<ap:Topic/>");
    assert_eq!(
        Format::from_path(std::path::Path::new("map.XMMAP")),
        Some(Format::MindManagerXml)
    );
    assert_eq!(
        detect_format(std::path::Path::new("unknown"), &source),
        Some(Format::MindManagerXml)
    );
    assert_eq!(
        detect_format(
            std::path::Path::new("unknown"),
            &archive(&[("Document.xml", &source)])
        ),
        Some(Format::MindManager)
    );
    assert!(Format::MindManager.can_import() && Format::MindManager.can_export());
    assert!(Format::MindManagerXml.can_import() && !Format::MindManagerXml.can_export());
    assert!(import_bytes(Format::MindManager, &source).is_err());
    assert!(
        import_bytes(
            Format::MindManagerXml,
            &archive(&[("Document.xml", &source)])
        )
        .is_err()
    );
}

#[test]
fn alpha_is_not_guessed_and_invalid_fonts_colors_characters_are_rejected() {
    let source = xml(
        r#"<ap:Topic><ap:Text PlainText="Root"><ap:Font Color="80112233"/></ap:Text><ap:Color FillColor="00112233"/></ap:Topic>"#,
    );
    let report = import_bytes_with_report(Format::MindManagerXml, &source).unwrap();
    assert_eq!(report.warnings, [ImportWarning::Formatting]);
    let style = &report.workbook.active().root_topic().style;
    assert!(style.fill.is_none() && style.text_color.is_none());
    let mut book = Workbook::new(Sheet::new("Map", "Root"));
    let root = book.active().root;
    book.active_mut().topic_mut(root).unwrap().style.fill = Some(thinoda_core::Color {
        r: 1,
        g: 2,
        b: 3,
        a: 128,
    });
    assert!(matches!(
        export_bytes(&book, Format::MindManager, &ExportOptions::default()),
        Err(IoError::MindManagerExportNeedsConfirmation)
    ));
    let bytes =
        export_bytes_with_consent(&book, Format::MindManager, &ExportOptions::default(), true)
            .unwrap();
    assert!(!document(&bytes).contains("FillColor"));
    for size in ["NaN", "inf", "-1", "0", "10001"] {
        let source = xml(&format!(
            r#"<ap:Topic><ap:Text PlainText="Root"><ap:Font Size="{size}"/></ap:Text></ap:Topic>"#
        ));
        assert!(
            import_bytes_with_report(Format::MindManagerXml, &source).is_err(),
            "{size}"
        );
    }
    for size in [f32::NAN, f32::INFINITY, -1.0, 0.0, 10001.0] {
        book.active_mut().topic_mut(root).unwrap().style.font_size = Some(size);
        assert!(
            export_bytes_with_consent(&book, Format::MindManager, &ExportOptions::default(), true)
                .is_err()
        );
    }
    book.active_mut().topic_mut(root).unwrap().style.font_size = None;
    book.active_mut().topic_mut(root).unwrap().title = "Invalid\u{1}".into();
    assert!(
        export_bytes_with_consent(&book, Format::MindManager, &ExportOptions::default(), true)
            .is_err()
    );
    for color in ["FF0000", "FFGG0000", "FFFFFFFFFF"] {
        let source = xml(&format!(
            r#"<ap:Topic><ap:Color FillColor="{color}"/></ap:Topic>"#
        ));
        assert!(
            import_bytes_with_report(Format::MindManagerXml, &source).is_err(),
            "{color}"
        );
    }
}

#[test]
fn unsupported_subtrees_warn_without_corrupting_supported_hierarchy() {
    let source = xml(
        r#"<ap:Topic><ap:Text PlainText="Root"/><ap:FloatingTopics><ap:Topic><ap:Text PlainText="Floating"/></ap:Topic></ap:FloatingTopics><ap:Extension><ap:SubTopics><ap:Topic><ap:Text PlainText="Extension topic"/></ap:Topic></ap:SubTopics></ap:Extension><ap:SubTopics><ap:Topic><ap:Text PlainText="Child"/></ap:Topic></ap:SubTopics><ap:RevisionGroup><ap:Creator UserName="Revision history"/></ap:RevisionGroup></ap:Topic>"#,
    );
    let report = import_bytes_with_report(Format::MindManagerXml, &source).unwrap();
    assert_eq!(report.workbook.active().topic_count(), 2);
    assert_eq!(report.workbook.active().root_topic().title, "Root");
    assert_eq!(report.warnings, [ImportWarning::Other]);
    let harmless = xml(
        r#"<ap:Topic Gen="0000000000000000"><ap:Text PlainText="Root"/><ap:RevisionGroup><ap:Creator UserName="Revision history"/></ap:RevisionGroup></ap:Topic>"#,
    );
    assert!(import_bytes(Format::MindManagerXml, &harmless).is_ok());
}

#[test]
fn archive_entry_count_aggregate_size_and_symlinks_are_rejected() {
    let valid = xml("<ap:Topic/>");
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("Document.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&valid).unwrap();
    zip.add_symlink(
        "bin/link",
        "target",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    assert!(
        import_bytes_with_report(Format::MindManager, &zip.finish().unwrap().into_inner()).is_err()
    );
    let entries: Vec<_> = (0..4097).map(|i| format!("entry{i}")).collect();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in &entries {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
    }
    assert!(
        import_bytes_with_report(Format::MindManager, &zip.finish().unwrap().into_inner()).is_err()
    );
    let mut oversized = archive(&[("Document.xml", &valid), ("bin/a", b""), ("bin/b", b"")]);
    let offsets: Vec<_> = oversized
        .windows(4)
        .enumerate()
        .filter_map(|(i, bytes)| (bytes == b"PK\x01\x02").then_some(i))
        .collect();
    for offset in &offsets[1..] {
        oversized[offset + 24..offset + 28].copy_from_slice(&(129u32 * 1024 * 1024).to_le_bytes());
    }
    assert!(import_bytes_with_report(Format::MindManager, &oversized).is_err());
}

#[test]
fn writer_depth_bound_keeps_its_output_readable_and_failed_writes_preserve_targets() {
    let mut sheet = Sheet::new("Deep", "Root");
    let mut id = sheet.root;
    for _ in 0..123 {
        id = sheet.add_child(id, None, "Child");
    }
    sheet.topic_mut(id).unwrap().notes = Some("Deep note".into());
    let mut book = Workbook::new(sheet);
    let bytes = export_bytes(&book, Format::MindManager, &ExportOptions::default()).unwrap();
    assert_eq!(
        import_bytes_with_report(Format::MindManager, &bytes)
            .unwrap()
            .workbook,
        book
    );
    book.active_mut().add_child(id, None, "Too deep");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preserved.mmap");
    std::fs::write(&path, b"Preserve destination").unwrap();
    assert!(
        thinoda_io::export_file(&book, &path, Format::MindManager, &ExportOptions::default())
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"Preserve destination");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    let destination = dir.path().join("directory.mmap");
    std::fs::create_dir(&destination).unwrap();
    assert!(
        thinoda_io::export_file(
            &Workbook::default(),
            &destination,
            Format::MindManager,
            &ExportOptions::default()
        )
        .is_err()
    );
    assert!(destination.is_dir());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
