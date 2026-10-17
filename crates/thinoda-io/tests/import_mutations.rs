//! Bounded, deterministic mutation smoke tests, not a coverage-guided fuzz campaign.

use std::{
    collections::BTreeSet,
    io::{Cursor, Write},
};
use thinoda_core::Workbook;
use thinoda_io::{ExportOptions, Format, export_bytes, import_bytes};

fn archive(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap().into_inner()
}

fn seeds() -> Vec<(Format, Vec<u8>)> {
    let mut workbook = Workbook::default();
    let sheet = workbook.active_mut();
    let main = sheet.children(sheet.root)[0];
    sheet.set_title(main, "Quoted <&> \"topic\"").unwrap();
    sheet.add_child(main, None, "Child");
    let mut seeds: Vec<_> = [
        Format::Thinoda,
        Format::XMind,
        Format::FreeMind,
        Format::Opml,
        Format::Markdown,
        Format::PlainText,
    ]
    .into_iter()
    .map(|format| {
        (
            format,
            export_bytes(&workbook, format, &ExportOptions::default()).unwrap(),
        )
    })
    .collect();
    seeds.push((Format::Thinoda, serde_json::to_vec(&workbook).unwrap()));
    seeds.push((Format::MindManager, archive("Document.xml",
        br#"<ap:Map xmlns:ap="http://schemas.mindjet.com/MindManager/Application/2003"><ap:OneTopic><ap:Topic><ap:Text PlainText="Root"/><ap:SubTopics><ap:Topic><ap:Text PlainText="Child"/></ap:Topic></ap:SubTopics></ap:Topic></ap:OneTopic></ap:Map>"#)));
    seeds
}

fn check(format: Format, bytes: &[u8]) {
    if let Ok(workbook) = import_bytes(format, bytes) {
        workbook.validate().unwrap();
        let native = export_bytes(&workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(
            import_bytes(Format::Thinoda, &native).unwrap(),
            workbook,
            "{format:?}"
        );
    }
}

#[test]
fn every_importer_handles_systematic_truncation_of_valid_documents() {
    for (format, seed) in seeds() {
        import_bytes(format, &seed).unwrap();
        let mut lengths: BTreeSet<_> = (0..seed.len().min(32)).collect();
        lengths.extend((0..=64).map(|index| seed.len() * index / 64));
        lengths.extend(seed.len().saturating_sub(32)..=seed.len());
        for length in lengths {
            check(format, &seed[..length]);
        }
    }
}

#[test]
fn deterministic_byte_mutations_never_return_invalid_workbooks() {
    let mut state = 0x594d_494e_445f_494fu64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for (format, seed) in seeds() {
        for index in 0..128 {
            let mut bytes = seed.clone();
            let at = next() as usize % bytes.len();
            match index % 4 {
                0 => bytes[at] ^= (next() as u8) | 1,
                1 => {
                    bytes.remove(at);
                }
                2 => bytes.insert(at, next() as u8),
                _ => {
                    let end = (at + 1 + next() as usize % 16).min(bytes.len());
                    bytes[at..end].fill(0xff);
                }
            }
            check(format, &bytes);
        }
    }
}

#[test]
fn structured_xml_and_xmind_json_variations_are_validated() {
    for title in [
        "",
        "A &amp; B",
        "&#x1F4A1;",
        "&unknown;",
        "&#0;",
        "\"quoted\"",
    ] {
        for depth in [0, 1, 8, 64] {
            let node = format!("<node TEXT=\"{title}\">");
            let free = format!(
                "<map>{}{}</map>",
                node.repeat(depth + 1),
                "</node>".repeat(depth + 1)
            );
            check(Format::FreeMind, free.as_bytes());
            let node = format!("<outline text=\"{title}\">");
            let opml = format!(
                "<opml><body>{}{}</body></opml>",
                node.repeat(depth + 1),
                "</outline>".repeat(depth + 1)
            );
            check(Format::Opml, opml.as_bytes());
        }
    }
    for content in [
        "null",
        "{}",
        "[]",
        "[null]",
        "[42]",
        r#"[{"rootTopic":null}]"#,
        r#"[{"rootTopic":{"id":"a","title":[],"children":{"attached":[null]}}}]"#,
        r#"[{"rootTopic":{"id":"a","title":"Root","children":{"attached":[{"id":"a","title":"Duplicate"}]}}}]"#,
        r#"[{"rootTopic":{"id":"root","title":"Root"},"relationships":[{"end1Id":"missing","end2Id":"root"}]}]"#,
    ] {
        check(Format::XMind, &archive("content.json", content.as_bytes()));
    }
}

#[test]
fn oversized_declared_zip_entries_are_rejected_before_decompression() {
    for (format, name, content) in [
        (
            Format::XMind,
            "content.json",
            br#"[{"rootTopic":{"id":"root","title":"Root"}}]"#.as_slice(),
        ),
        (Format::Thinoda, "manifest.json", b"{}".as_slice()),
        (Format::MindManager, "Document.xml", b"<Map/>".as_slice()),
    ] {
        let mut bytes = archive(name, content);
        let central = bytes
            .windows(4)
            .position(|bytes| bytes == b"PK\x01\x02")
            .unwrap();
        let local = bytes
            .windows(4)
            .position(|bytes| bytes == b"PK\x03\x04")
            .unwrap();
        // Forge only the declared uncompressed length; no large buffer is allocated.
        let oversized = (200u32 * 1024 * 1024 + 1).to_le_bytes();
        bytes[central + 24..central + 28].copy_from_slice(&oversized);
        bytes[local + 22..local + 26].copy_from_slice(&oversized);
        let error = import_bytes(format, &bytes).unwrap_err().to_string();
        assert!(error.contains("exceeds size limit"), "{format:?}: {error}");
    }
}
