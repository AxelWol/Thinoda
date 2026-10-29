use std::io::{Cursor, Read, Write};
use thinoda_core::{Workbook, notes::NoteContent, resources::Resource};
use thinoda_io::{Format, IoError, export_bytes, import_bytes};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn entry(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = Vec::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    output
}

#[test]
fn formatted_notes_use_native_v4_with_or_without_embedded_resources() {
    for resources in [false, true] {
        let mut book = Workbook::default();
        let root = book.active().root;
        let source = "**café**\r\n\n- 日本語\n\n<script>inert</script>";
        book.active_mut()
            .set_markdown_notes(root, Some(source.into()))
            .unwrap();
        if resources {
            book.active_mut()
                .resources
                .insert(
                    Resource::new("evidence.bin", "application/octet-stream", vec![0, 1, 255])
                        .unwrap(),
                )
                .unwrap();
        }
        let bytes = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&entry(&bytes, "manifest.json")).unwrap();
        assert_eq!(manifest["version"], 4);
        let content: serde_json::Value =
            serde_json::from_slice(&entry(&bytes, "content.json")).unwrap();
        assert_eq!(
            content["sheets"][0]["topics"][root.0.to_string()]["notes"]["format"],
            "markdown-v1"
        );
        assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), book);
        assert_eq!(
            import_bytes(Format::Thinoda, &serde_json::to_vec(&book).unwrap()).unwrap(),
            book
        );
        let oversized_source = "x".repeat(thinoda_core::notes::MAX_PROJECTION_BYTES + 1);
        book.active_mut()
            .set_notes(root, Some(oversized_source.clone()))
            .unwrap();
        let bytes = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
        let reopened = import_bytes(Format::Thinoda, &bytes).unwrap();
        assert_eq!(reopened, book);
        assert_eq!(
            reopened.active().root_topic().notes.as_deref(),
            Some(oversized_source.as_str())
        );
        assert!(
            reopened
                .active()
                .root_topic()
                .notes
                .as_ref()
                .unwrap()
                .is_markdown()
        );
    }
}

#[test]
fn legacy_notes_keep_their_literal_source_and_native_version() {
    let mut book = Workbook::default();
    let source = "**not automatically converted**\r\n日本語";
    let root = book.active().root;
    book.active_mut()
        .set_notes(root, Some(source.into()))
        .unwrap();
    let bytes = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&entry(&bytes, "manifest.json")).unwrap();
    assert_eq!(manifest["version"], 2);
    let reopened = import_bytes(Format::Thinoda, &bytes).unwrap();
    assert_eq!(reopened, book);
    assert_eq!(
        reopened.active().root_topic().notes,
        Some(NoteContent::Plain(source.into()))
    );
}

#[test]
fn formatted_notes_cannot_masquerade_as_an_older_native_version() {
    let mut book = Workbook::default();
    let root = book.active().root;
    book.active_mut()
        .set_markdown_notes(root, Some("**source**".into()))
        .unwrap();
    for version in [2, 3, 5] {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, value) in [
            (
                "manifest.json",
                serde_json::json!({"format": "thinoda", "version": version, "resources": []}),
            ),
            ("content.json", serde_json::to_value(&book).unwrap()),
        ] {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer
                .write_all(&serde_json::to_vec(&value).unwrap())
                .unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        let error = import_bytes(Format::Thinoda, &bytes).unwrap_err();
        if version == 5 {
            assert!(matches!(error, IoError::NewerVersion(_)));
        } else {
            assert!(
                error
                    .to_string()
                    .contains("formatted notes require native format version 4")
            );
        }
    }
}

#[test]
fn xmind_private_roundtrip_preserves_format_and_public_content_stays_a_string() {
    let mut book = Workbook::default();
    let root = book.active().root;
    let source = "**café**\n\n- first\n- 日本語";
    book.active_mut()
        .set_markdown_notes(root, Some(source.into()))
        .unwrap();
    let bytes = export_bytes(&book, Format::XMind, &Default::default()).unwrap();
    let content: serde_json::Value =
        serde_json::from_slice(&entry(&bytes, "content.json")).unwrap();
    assert_eq!(content[0]["rootTopic"]["notes"]["plain"]["content"], source);
    assert_eq!(import_bytes(Format::XMind, &bytes).unwrap(), book);
}
