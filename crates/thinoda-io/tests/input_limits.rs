use std::path::Path;

use thinoda_io::{
    Format, IoError, fim, import_bytes_with_report, import_file_with_report,
    import_named_bytes_with_report, import_read_limit,
};

const ARCHIVE_LIMIT: usize = 64 * 1024 * 1024;

#[test]
fn import_limits_cover_format_hints_without_restricting_native_formats() {
    for name in [
        "map.mmap",
        "map.XMMAP",
        "map.textbundle",
        "map.textpack",
        "map.docx",
        "diagram.html",
        "diagram.HTM",
        "map.xml",
        "map.unknown",
        "map",
    ] {
        assert_eq!(
            import_read_limit(Path::new(name)),
            Some(ARCHIVE_LIMIT),
            "{name}"
        );
    }
    for name in ["map.xdf2", "map.XDF3"] {
        assert_eq!(
            import_read_limit(Path::new(name)),
            Some(fim::MAX_XML_BYTES),
            "{name}"
        );
    }
    for name in ["map.thinoda", "map.xmind", "map.mm", "map.opml", "map.md"] {
        assert_eq!(import_read_limit(Path::new(name)), None, "{name}");
    }
}

#[test]
fn metadata_validation_preserves_named_and_file_import_errors() {
    let xml = Path::new("map.xml");
    assert!(thinoda_io::validate_import_size(xml, ARCHIVE_LIMIT as u64).is_ok());
    assert!(matches!(
        thinoda_io::validate_import_size(xml, ARCHIVE_LIMIT as u64 + 1),
        Err(IoError::Invalid(message)) if message == "import archive exceeds size limit"
    ));
    let xdf = Path::new("map.xdf2");
    assert!(thinoda_io::validate_import_size(xdf, fim::MAX_XML_BYTES as u64).is_ok());
    assert!(matches!(
        thinoda_io::validate_import_size(xdf, fim::MAX_XML_BYTES as u64 + 1),
        Err(IoError::UnsupportedFormat(name)) if name == "map.xdf2"
    ));
    let mindmanager = Path::new("map.mmap");
    assert!(matches!(
        thinoda_io::validate_import_size(mindmanager, ARCHIVE_LIMIT as u64 + 1),
        Err(IoError::Invalid(message)) if message == "import archive exceeds size limit"
    ));
    assert!(matches!(
        thinoda_io::validate_import_file_size(mindmanager, ARCHIVE_LIMIT as u64 + 1),
        Err(IoError::Invalid(message)) if message == "MindManager file exceeds size limit"
    ));
    assert!(thinoda_io::validate_import_size(Path::new("map.xmind"), u64::MAX).is_ok());
}

#[test]
fn named_import_boundaries_keep_existing_error_variants() {
    let bytes = vec![b' '; ARCHIVE_LIMIT + 1];
    for name in [
        "map.mmap",
        "map.xmmap",
        "map.textbundle",
        "map.docx",
        "diagram.html",
        "diagram.htm",
        "map.xml",
    ] {
        assert!(
            matches!(
                import_named_bytes_with_report(Path::new(name), &bytes),
                Err(IoError::Invalid(message)) if message == "import archive exceeds size limit"
            ),
            "{name}"
        );
    }
    assert!(matches!(
        import_named_bytes_with_report(Path::new("map.mmap"), &bytes[..ARCHIVE_LIMIT]),
        Err(IoError::MindManager(_))
    ));
    for name in ["map.xdf2", "map.xdf3"] {
        assert!(
            matches!(
                import_named_bytes_with_report(Path::new(name), &bytes[..fim::MAX_XML_BYTES + 1]),
                Err(IoError::UnsupportedFormat(path)) if path == name
            ),
            "{name}"
        );
    }
    assert!(matches!(
        import_bytes_with_report(Format::FimXdf2, &bytes[..fim::MAX_XML_BYTES + 1]),
        Err(IoError::Fim(fim::FimError::Limit("XML bytes")))
    ));
}

#[test]
fn xdf_detection_accepts_exactly_its_limit_and_rejects_one_byte_more() {
    let mut xml = format!(
        "<x:xdatenfelder.stammdatenschema.0102 xmlns:x='{}'>1</x:xdatenfelder.stammdatenschema.0102>",
        fim::Dialect::Xdf3.namespace()
    )
    .into_bytes();
    xml.resize(fim::MAX_XML_BYTES, b' ');
    let name = Path::new("map.xdf3");
    assert_eq!(thinoda_io::detect_format(name, &xml), Some(Format::FimXdf3));
    assert!(!matches!(
        import_named_bytes_with_report(name, &xml),
        Err(IoError::UnsupportedFormat(_))
    ));
    xml.push(b' ');
    assert!(matches!(
        import_named_bytes_with_report(name, &xml),
        Err(IoError::UnsupportedFormat(path)) if path == "map.xdf3"
    ));
}

#[test]
fn unknown_xml_above_fim_limit_can_still_import_other_xml_formats() {
    let mut xml = br#"<map><node TEXT="Root"/></map>"#.to_vec();
    xml.resize(fim::MAX_XML_BYTES + 1, b' ');
    let report = import_named_bytes_with_report(Path::new("map.xml"), &xml).unwrap();
    assert_eq!(report.format, Format::FreeMind);
    assert_eq!(report.workbook.active().title, "Root");
}

#[test]
fn oversized_files_keep_their_format_specific_errors() {
    let dir = tempfile::tempdir().unwrap();
    for (name, limit, message) in [
        (
            "map.mmap",
            ARCHIVE_LIMIT,
            Some("MindManager file exceeds size limit"),
        ),
        (
            "map.xmmap",
            ARCHIVE_LIMIT,
            Some("MindManager file exceeds size limit"),
        ),
        (
            "map.textbundle",
            ARCHIVE_LIMIT,
            Some("import archive exceeds size limit"),
        ),
        (
            "map.docx",
            ARCHIVE_LIMIT,
            Some("import archive exceeds size limit"),
        ),
        (
            "map.xml",
            ARCHIVE_LIMIT,
            Some("import archive exceeds size limit"),
        ),
        ("map.xdf2", fim::MAX_XML_BYTES, None),
        ("map.xdf3", fim::MAX_XML_BYTES, None),
    ] {
        let path = dir.path().join(name);
        std::fs::File::create(&path)
            .unwrap()
            .set_len(limit as u64 + 1)
            .unwrap();
        let error = import_file_with_report(&path).unwrap_err();
        match message {
            Some(expected) => {
                assert!(
                    matches!(error, IoError::Invalid(actual) if actual == expected),
                    "{name}"
                );
            }
            None => {
                assert!(
                    matches!(error, IoError::UnsupportedFormat(actual) if actual == path.display().to_string()),
                    "{name}"
                );
            }
        }
        assert_eq!(std::fs::metadata(&path).unwrap().len(), limit as u64 + 1);
        std::fs::remove_file(path).unwrap();
    }
}
