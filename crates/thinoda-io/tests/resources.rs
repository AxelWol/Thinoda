use std::io::{Cursor, Read, Write};
use thinoda_core::{Sheet, Workbook, resources::Resource};
use thinoda_io::{ExportOptions, Format, export_bytes, import_bytes};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn sample() -> (Workbook, String) {
    let mut sheet = Sheet::new("Evidence", "Root");
    let reference = sheet
        .resources
        .insert(Resource::new("evidence.png", "image/png", b"evidence bytes".to_vec()).unwrap())
        .unwrap();
    sheet.topic_mut(sheet.root).unwrap().image = Some(reference.clone());
    (Workbook::new(sheet), reference)
}

fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).unwrap();
            let name = file.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}

fn archive(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn mutate_manifest(entries: &mut [(String, Vec<u8>)], f: impl FnOnce(&mut serde_json::Value)) {
    let (_, bytes) = entries
        .iter_mut()
        .find(|(name, _)| name == "manifest.json")
        .unwrap();
    let mut manifest = serde_json::from_slice(bytes).unwrap();
    f(&mut manifest);
    *bytes = serde_json::to_vec(&manifest).unwrap();
}

#[test]
fn v3_roundtrip_uses_raw_deduplicated_blobs_with_per_sheet_metadata() {
    let (mut book, reference) = sample();
    let mut other = book.active().clone();
    other.title = "Second".into();
    other.resources.entries.values_mut().next().unwrap().name = "alternate.png".into();
    book.sheets.push(other);
    let bytes = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), book);
    let files = entries(&bytes);
    assert_eq!(files.len(), 3);
    let manifest: serde_json::Value = serde_json::from_slice(&files[0].1).unwrap();
    assert_eq!(manifest["version"], 3);
    assert_eq!(manifest["resources"].as_array().unwrap().len(), 2);
    let content: serde_json::Value = serde_json::from_slice(&files[1].1).unwrap();
    assert!(content["sheets"][0].get("resources").is_none());
    let id = reference.strip_prefix("thinoda-resource:").unwrap();
    assert_eq!(
        files[2],
        (format!("resources/{id}"), b"evidence bytes".to_vec())
    );
    // Recovery/plain JSON shares the validated representation rather than external file paths.
    let raw = serde_json::to_vec(&book).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &raw).unwrap(), book);
}

#[test]
fn v3_rejects_missing_corrupt_unsafe_duplicate_and_oversized_manifest_resources() {
    let (book, _) = sample();
    let bytes = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
    let original = entries(&bytes);
    let mut missing = original.clone();
    missing.pop();
    assert!(import_bytes(Format::Thinoda, &archive(&missing)).is_err());
    let mut corrupt = original.clone();
    corrupt[2].1[0] ^= 1;
    assert!(
        import_bytes(Format::Thinoda, &archive(&corrupt))
            .unwrap_err()
            .to_string()
            .contains("digest mismatch")
    );
    for (field, value) in [
        ("id", serde_json::json!("../escape")),
        ("id", serde_json::json!("F".repeat(64))),
        ("size", serde_json::json!(0)),
        ("size", serde_json::json!(1)),
        ("size", serde_json::json!(8 * 1024 * 1024 + 1)),
        ("sheet", serde_json::json!(99)),
        ("name", serde_json::json!("../escape")),
        ("media_type", serde_json::json!("invalid")),
    ] {
        let mut files = original.clone();
        mutate_manifest(&mut files, |manifest| {
            manifest["resources"][0][field] = value
        });
        assert!(
            import_bytes(Format::Thinoda, &archive(&files)).is_err(),
            "{field}"
        );
    }
    let mut files = original.clone();
    mutate_manifest(&mut files, |manifest| {
        let duplicate = manifest["resources"][0].clone();
        manifest["resources"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
    });
    assert!(
        import_bytes(Format::Thinoda, &archive(&files))
            .unwrap_err()
            .to_string()
            .contains("duplicate embedded")
    );
    let mut files = original.clone();
    mutate_manifest(&mut files, |manifest| {
        manifest["resources"] = serde_json::json!(vec![manifest["resources"][0].clone(); 129]);
    });
    assert!(import_bytes(Format::Thinoda, &archive(&files)).is_err());
    let mut files = original.clone();
    for index in 0..128 {
        files.push((format!("extra/{index}"), vec![]));
    }
    assert!(
        import_bytes(Format::Thinoda, &archive(&files))
            .unwrap_err()
            .to_string()
            .contains("too many entries")
    );
    let mut inline = original.clone();
    inline[1].1 = serde_json::to_vec(&book).unwrap();
    assert!(
        import_bytes(Format::Thinoda, &archive(&inline))
            .unwrap_err()
            .to_string()
            .contains("not inline")
    );
    let mut unlisted = original.clone();
    mutate_manifest(&mut unlisted, |manifest| {
        manifest["resources"] = serde_json::json!([])
    });
    assert!(import_bytes(Format::Thinoda, &archive(&unlisted)).is_err());
    let mut duplicate = original.clone();
    duplicate.push(("_ontent.json".into(), duplicate[1].1.clone()));
    let mut duplicate = archive(&duplicate);
    for offset in 0..duplicate.len().saturating_sub(12) {
        if &duplicate[offset..offset + 12] == b"_ontent.json" {
            duplicate[offset] = b'c';
        }
    }
    assert!(import_bytes(Format::Thinoda, &duplicate).is_err());
}

#[test]
fn resource_free_v2_and_legacy_image_references_keep_loading() {
    let mut book = Workbook::default();
    let sheet = book.active_mut();
    sheet.topic_mut(sheet.root).unwrap().image = Some("legacy/path.png".into());
    let bytes = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
    let files = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(&files[0].1).unwrap();
    assert_eq!(manifest["version"], 2);
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), book);
    let (embedded, _) = sample();
    let mut files = files;
    files[1].1 = serde_json::to_vec(&embedded).unwrap();
    assert!(import_bytes(Format::Thinoda, &archive(&files)).is_err());
}

#[test]
fn exports_printing_and_pitch_fail_explicitly_instead_of_dropping_bytes() {
    let (book, _) = sample();
    for format in Format::EXPORTABLE {
        if *format != Format::Thinoda {
            assert!(
                export_bytes(&book, *format, &Default::default())
                    .unwrap_err()
                    .to_string()
                    .contains("embedded resources"),
                "{format:?}"
            );
        }
    }
    let options = ExportOptions::default();
    let mut sheet = book.active().clone();
    let branch = sheet.add_child(sheet.root, None, "Branch");
    sheet
        .add_presentation_slide(
            "Branch",
            thinoda_core::PresentationSource::Branch(branch),
            thinoda_core::Rect::from_min_size(
                thinoda_core::Vec2::ZERO,
                thinoda_core::Vec2::new(200.0, 100.0),
            ),
        )
        .unwrap();
    assert!(thinoda_io::presentation::render_presentation_slide_svg(&sheet, 0, &options).is_err());
    assert!(thinoda_io::presentation::export_presentation_png_zip(&sheet, &options).is_err());
    assert!(thinoda_io::presentation::export_presentation_pdf(&sheet, &options).is_err());
    assert!(thinoda_io::print::render_print(&book, 0, Default::default()).is_err());
}
