//! Loss reports for annotations omitted by single-outline and FIM exports.

use std::io::{Cursor, Read, Write};

use thinoda_core::{Card, CardKind, Sheet, SheetRootMode, Vec2, Workbook};
use thinoda_io::{
    ExportLoss, ExportOptions, Format, IoError, export_bytes_with_consent, export_losses, fim,
    import_bytes,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const OUTLINE_FORMATS: &[Format] = &[
    Format::FreeMind,
    Format::Opml,
    Format::Markdown,
    Format::PlainText,
    Format::MindManager,
    Format::TextBundle,
    Format::Word,
    Format::Excel,
    Format::Csv,
];

fn annotated(mode: SheetRootMode) -> Workbook {
    let mut sheet = Sheet::new("Ideas", "Central root");
    sheet.root_mode = mode;
    let parent = match mode {
        SheetRootMode::Central => sheet.root,
        SheetRootMode::Hidden => sheet.add_floating("Visible floating root", Vec2::new(10.0, 20.0)),
    };
    let first = sheet.add_child(parent, None, "KEPT FIRST");
    let second = sheet.add_child(parent, None, "KEPT SECOND");
    let boundary = sheet.add_boundary(&[first, second]).unwrap();
    sheet
        .set_boundary_title(boundary, Some("OMITTED BOUNDARY".into()))
        .unwrap();
    let relationship = sheet.add_relationship(first, second).unwrap();
    sheet
        .set_relationship_title(relationship, "OMITTED RELATIONSHIP")
        .unwrap();
    let summary = sheet
        .add_summary(&[first, second], "OMITTED SUMMARY ROOT")
        .unwrap();
    let summary_child = sheet.add_child(summary, None, "OMITTED SUMMARY CHILD");
    let summary_sibling = sheet.add_child(summary, None, "OMITTED SUMMARY SIBLING");
    let nested = sheet
        .add_summary(&[summary_child, summary_sibling], "OMITTED NESTED SUMMARY")
        .unwrap();
    sheet.add_child(nested, None, "OMITTED NESTED DESCENDANT");
    let book = Workbook::new(sheet);
    book.validate().unwrap();
    book
}

fn annotation_losses(losses: &[ExportLoss]) -> Vec<ExportLoss> {
    losses
        .iter()
        .filter(|loss| {
            matches!(
                loss,
                ExportLoss::SummaryTreesDropped(_)
                    | ExportLoss::RelationshipsDropped(_)
                    | ExportLoss::BoundariesDropped(_)
            )
        })
        .cloned()
        .collect()
}

fn expected_losses() -> Vec<ExportLoss> {
    vec![
        ExportLoss::SummaryTreesDropped(2),
        ExportLoss::RelationshipsDropped(1),
        ExportLoss::BoundariesDropped(1),
    ]
}

fn export(book: &Workbook, format: Format, options: &ExportOptions) -> Vec<u8> {
    export_bytes_with_consent(book, format, options, true)
        .unwrap_or_else(|error| panic!("{format:?}: {error}"))
}

fn all_text(bytes: &[u8]) -> String {
    if !bytes.starts_with(b"PK") {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        text.push_str(&String::from_utf8_lossy(&content));
    }
    text
}

fn archive_entry(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut content = Vec::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_end(&mut content)
        .unwrap();
    content
}

#[test]
fn each_single_outline_export_omits_and_reports_nested_summary_trees_and_annotations() {
    for mode in [SheetRootMode::Central, SheetRootMode::Hidden] {
        let book = annotated(mode);
        for &format in OUTLINE_FORMATS {
            assert_eq!(
                annotation_losses(&export_losses(&book, format, &ExportOptions::default())),
                expected_losses(),
                "{mode:?} {format:?}"
            );
            let bytes = export(&book, format, &ExportOptions::default());
            let text = all_text(&bytes);
            assert!(text.contains("KEPT FIRST"), "{mode:?} {format:?}");
            for omitted in [
                "OMITTED SUMMARY ROOT",
                "OMITTED SUMMARY CHILD",
                "OMITTED SUMMARY SIBLING",
                "OMITTED NESTED SUMMARY",
                "OMITTED NESTED DESCENDANT",
                "OMITTED RELATIONSHIP",
                "OMITTED BOUNDARY",
            ] {
                assert!(!text.contains(omitted), "{mode:?} {format:?}: {omitted}");
            }
        }
    }
}

#[test]
fn omitted_annotations_do_not_change_existing_outline_bytes() {
    for mode in [SheetRootMode::Central, SheetRootMode::Hidden] {
        let with_annotations = annotated(mode);
        let mut without_annotations = with_annotations.clone();
        let sheet = without_annotations.active_mut();
        sheet.relationships.clear();
        sheet.boundaries.clear();
        let outer = sheet.summaries[0].id;
        sheet.remove_summary(outer).unwrap();
        assert!(sheet.summaries.is_empty());
        without_annotations.validate().unwrap();
        for &format in OUTLINE_FORMATS {
            assert_eq!(
                export(&with_annotations, format, &ExportOptions::default()),
                export(&without_annotations, format, &ExportOptions::default()),
                "{mode:?} {format:?}"
            );
        }
    }
}

#[test]
fn reports_only_the_selected_outline_sheet() {
    let mut book = annotated(SheetRootMode::Central);
    book.sheets.push(Sheet::new("Plain", "Plain root"));
    book.active_sheet = 1;
    book.validate().unwrap();
    for &format in OUTLINE_FORMATS {
        assert!(
            annotation_losses(&export_losses(&book, format, &ExportOptions::default())).is_empty(),
            "{format:?}"
        );
        let first = ExportOptions {
            sheet: Some(0),
            ..Default::default()
        };
        assert_eq!(
            annotation_losses(&export_losses(&book, format, &first)),
            expected_losses(),
            "{format:?}"
        );
        assert!(all_text(&export(&book, format, &first)).contains("KEPT FIRST"));
    }
}

#[test]
fn central_floating_cards_and_their_summaries_are_omitted_not_converted() {
    let mut sheet = Sheet::new("Map", "Root");
    let card = sheet
        .add_card(
            "FLOATING CARD",
            Vec2::new(100.0, 100.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0)),
        )
        .unwrap();
    let first = sheet.add_child(card, None, "CARD FIRST");
    let second = sheet.add_child(card, None, "CARD SECOND");
    sheet.add_summary(&[first, second], "CARD SUMMARY").unwrap();
    let mut book = Workbook::new(sheet);
    book.validate().unwrap();
    for &format in OUTLINE_FORMATS {
        let losses = export_losses(&book, format, &ExportOptions::default());
        assert!(
            losses.contains(&ExportLoss::FloatingTopicsDropped(1)),
            "{format:?}"
        );
        assert!(losses.contains(&ExportLoss::CardsDropped(1)), "{format:?}");
        assert!(
            losses.contains(&ExportLoss::SummaryTreesDropped(1)),
            "{format:?}"
        );
        assert!(
            !losses
                .iter()
                .any(|loss| matches!(loss, ExportLoss::CardsConvertedToTopics(_))),
            "{format:?}"
        );
        assert!(
            !all_text(&export(&book, format, &ExportOptions::default())).contains("FLOATING CARD")
        );
    }

    // Hidden-root projection attaches the floating card as an ordinary topic.
    book.active_mut().root_mode = SheetRootMode::Hidden;
    book.validate().unwrap();
    for &format in OUTLINE_FORMATS {
        let losses = export_losses(&book, format, &ExportOptions::default());
        assert!(
            losses.contains(&ExportLoss::CardsConvertedToTopics(1)),
            "{format:?}"
        );
        assert!(
            !losses
                .iter()
                .any(|loss| matches!(loss, ExportLoss::CardsDropped(_)))
        );
        assert!(
            losses.contains(&ExportLoss::SummaryTreesDropped(1)),
            "{format:?}"
        );
        let text = all_text(&export(&book, format, &ExportOptions::default()));
        assert!(text.contains("FLOATING CARD"), "{format:?}");
        assert!(!text.contains("CARD SUMMARY"), "{format:?}");
    }
    let xmind = export_losses(&book, Format::XMind, &ExportOptions::default());
    assert!(xmind.contains(&ExportLoss::CardsConvertedToTopics(1)));
    assert!(
        !xmind
            .iter()
            .any(|loss| matches!(loss, ExportLoss::CardsDropped(_)))
    );
    assert!(annotation_losses(&xmind).is_empty());
}

#[test]
fn preserving_formats_do_not_claim_these_annotations_are_omitted() {
    for mode in [SheetRootMode::Central, SheetRootMode::Hidden] {
        let book = annotated(mode);
        for format in [
            Format::Thinoda,
            Format::XMind,
            Format::Svg,
            Format::Png,
            Format::Jpeg,
            Format::Pdf,
            Format::PowerPoint,
        ] {
            assert!(
                annotation_losses(&export_losses(&book, format, &ExportOptions::default()))
                    .is_empty(),
                "{mode:?} {format:?}"
            );
        }
        let native = export(&book, Format::Thinoda, &ExportOptions::default());
        assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
        let svg = all_text(&export(&book, Format::Svg, &ExportOptions::default()));
        for visible in [
            "OMITTED SUMMARY ROOT",
            "OMITTED SUMMARY CHILD",
            "OMITTED NESTED SUMMARY",
            "OMITTED NESTED DESCENDANT",
            "OMITTED RELATIONSHIP",
            "OMITTED BOUNDARY",
        ] {
            assert!(svg.contains(visible), "{mode:?}: {visible}");
        }
    }
}

#[test]
fn xmind_public_content_preserves_annotations_and_nested_summary_topics() {
    for mode in [SheetRootMode::Central, SheetRootMode::Hidden] {
        let book = annotated(mode);
        let bytes = export(&book, Format::XMind, &ExportOptions::default());
        let content = archive_entry(&bytes, "content.json");
        let public: serde_json::Value = serde_json::from_slice(&content).unwrap();
        let sheet = &public[0];
        let root = &sheet["rootTopic"];
        let parent = if mode == SheetRootMode::Hidden {
            &root["children"]["detached"][0]
        } else {
            root
        };
        assert_eq!(parent["boundaries"][0]["title"], "OMITTED BOUNDARY");
        assert_eq!(sheet["relationships"][0]["title"], "OMITTED RELATIONSHIP");
        assert_eq!(parent["summaries"].as_array().unwrap().len(), 1);
        let summary = &parent["children"]["summary"][0];
        assert_eq!(summary["title"], "OMITTED SUMMARY ROOT");
        assert_eq!(
            summary["children"]["attached"][0]["title"],
            "OMITTED SUMMARY CHILD"
        );
        assert_eq!(summary["summaries"].as_array().unwrap().len(), 1);
        let nested = &summary["children"]["summary"][0];
        assert_eq!(nested["title"], "OMITTED NESTED SUMMARY");
        assert_eq!(
            nested["children"]["attached"][0]["title"],
            "OMITTED NESTED DESCENDANT"
        );

        // Verify the ordinary public content, independently of Thinoda's private snapshot.
        let mut public_archive = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut public_archive);
            writer
                .start_file("content.json", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&content).unwrap();
            writer.finish().unwrap();
        }
        let imported = import_bytes(Format::XMind, &public_archive.into_inner()).unwrap();
        let sheet = imported.active();
        assert_eq!(sheet.summaries.len(), 2, "{mode:?}");
        assert_eq!(sheet.relationships.len(), 1, "{mode:?}");
        assert_eq!(sheet.boundaries.len(), 1, "{mode:?}");
        for title in [
            "OMITTED SUMMARY ROOT",
            "OMITTED SUMMARY CHILD",
            "OMITTED NESTED SUMMARY",
            "OMITTED NESTED DESCENDANT",
        ] {
            assert!(
                sheet.topics.values().any(|topic| topic.title == title),
                "{mode:?}: {title}"
            );
        }
    }
}

#[test]
fn mindmanager_keeps_its_existing_format_specific_confirmation() {
    let book = annotated(SheetRootMode::Central);
    assert!(matches!(
        export_bytes_with_consent(&book, Format::MindManager, &ExportOptions::default(), false),
        Err(IoError::MindManagerExportNeedsConfirmation)
    ));
    assert!(!export(&book, Format::MindManager, &ExportOptions::default()).is_empty());
}

#[test]
fn valid_fim_projection_reports_omitted_map_annotations() {
    let source = br#"<?xml version="1.0" encoding="UTF-8"?>
<x:xdatenfelder.stammdatenschema.0102 xmlns:x="urn:xoev-de:fim:standard:xdatenfelder_3.0.0">
<x:stammdatenschema><x:identifikation><x:id>S00000000001</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Schema</x:name>
<x:struktur><x:anzahl>0:*</x:anzahl><x:enthaelt><x:datenfeld><x:identifikation><x:id>F00000000001</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Field</x:name></x:datenfeld></x:enthaelt></x:struktur>
</x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>"#;
    let mut book = fim::import_document(source).unwrap();
    let sheet = book.active_mut();
    let root = sheet.root;
    let other = *sheet.topics.keys().find(|id| **id != root).unwrap();
    sheet.add_relationship(root, other).unwrap();
    sheet.add_boundary(&[root]).unwrap();
    book.validate().unwrap();
    for format in [Format::FimXdf2, Format::FimXdf3] {
        assert_eq!(
            annotation_losses(&export_losses(&book, format, &ExportOptions::default())),
            vec![
                ExportLoss::RelationshipsDropped(1),
                ExportLoss::BoundariesDropped(1)
            ]
        );
    }
    assert_eq!(fim::export_document(book.active()).unwrap(), source);
}
