use super::*;
use thinoda_core::{CanvasObject, Rect, ShapeKind, Vec2};

fn shape() -> CanvasObject {
    CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(30.0, 30.0)),
    )
}

#[test]
fn loss_rows_cover_every_variant_and_do_not_expand_title_placeholders() {
    let losses = vec![
        ExportLoss::ShapesDropped(2),
        ExportLoss::InkDropped(3),
        ExportLoss::ConnectorsDropped(4),
        ExportLoss::CardsConvertedToTopics(5),
        ExportLoss::RotationDropped(6),
        ExportLoss::ViewportSlidesDropped(7),
        ExportLoss::ZonesDropped(8),
        ExportLoss::ZoneClipDropped(9),
        ExportLoss::FloatingTopicsDropped(10),
        ExportLoss::SummaryTreesDropped(11),
        ExportLoss::RelationshipsDropped(12),
        ExportLoss::BoundariesDropped(13),
        ExportLoss::CardsDropped(14),
        ExportLoss::OrientedImageMetadataDropped(15),
        ExportLoss::BoardSettingsDropped,
        ExportLoss::InlineTextDropped(16),
        ExportLoss::ConnectorAttachmentsDetached(17),
        ExportLoss::SynthesizedRoot {
            sheet: 2,
            title: "title {sheet}\nnext".into(),
        },
    ];
    let report = LossReport {
        losses: losses.clone(),
        ..Default::default()
    };
    let lines = loss_lines(&report, Format::Markdown);
    assert_eq!(lines.len(), losses.len());
    for (line, count) in lines.iter().take(14).zip(2..=15) {
        assert!(line.starts_with(&format!("{count} — ")), "{line}");
    }
    assert!(lines[15].starts_with("16 — "), "{}", lines[15]);
    assert!(lines[16].starts_with("17 — "), "{}", lines[16]);
    for (count, label) in [
        (11, "Summary topic trees omitted"),
        (12, "Relationships omitted"),
        (13, "Boundaries omitted"),
        (14, "Cards omitted"),
        (15, "Camera image metadata or color profiles may be omitted"),
    ] {
        assert!(lines.contains(&format!("{count} — {}", t(label))));
    }
    let root = lines.last().unwrap();
    assert!(root.contains("3"));
    assert!(root.contains("title {sheet}\\nnext"), "{root}");
    let fim = loss_lines(&report, Format::FimXdf3);
    assert!(fim[3].contains(t("Card presentation omitted")));
    assert!(
        fim.last().unwrap().contains(
            t("Sheet {sheet}: hidden root mode omitted.")
                .replace("{sheet}", "3")
                .as_str()
        )
    );
    assert!(!fim.last().unwrap().contains("title {sheet}"));
}

#[test]
fn oriented_photo_metadata_loss_requires_visual_export_consent() {
    use image::ImageEncoder;

    let exif = [
        b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder.set_exif_metadata(exif.to_vec()).unwrap();
    encoder
        .write_image(&[127; 8 * 4 * 4], 8, 4, image::ExtendedColorType::Rgba8)
        .unwrap();
    let resource =
        thinoda_core::resources::Resource::new("camera.png", "image/png", bytes).unwrap();
    let mut book = Workbook::default();
    let sheet = book.active_mut();
    thinoda_core::images::set_image(sheet, sheet.root, Some((resource, Vec2::new(8.0, 4.0))))
        .unwrap();
    let options = ExportOptions::default();
    let label = t("Camera image metadata or color profiles may be omitted");
    let message = require_board_consent(&book, Format::Svg, &options, false).unwrap_err();
    assert!(message.contains(label));
    assert!(require_board_consent(&book, Format::Svg, &options, true).is_ok());
    assert!(require_board_consent(&book, Format::Thinoda, &options, false).is_ok());
    assert!(require_board_consent(&book, Format::XMind, &options, false).is_ok());
}

#[test]
fn oriented_photo_metadata_warning_is_localized() {
    let key = "Camera image metadata or color profiles may be omitted";
    for language in i18n::Lang::ALL {
        let value = i18n::t_in(language, key);
        assert!(!value.is_empty());
        if language != i18n::Lang::En {
            assert_ne!(value, key, "missing {language:?} warning");
        }
    }
}

fn connector_attachment_book() -> Workbook {
    use thinoda_core::{CanvasConnector, Endpoint, SheetRootMode};

    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut target = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(250.0, 100.0)),
    );
    target.waypoints = vec![Vec2::new(200.0, 0.0), Vec2::new(200.0, 100.0)].into();
    let target = sheet
        .add_canvas_object(CanvasObject::connector(target))
        .unwrap();
    let mut dependent = CanvasConnector::new(
        Endpoint::Object(target),
        Endpoint::Point(Vec2::new(300.0, 200.0)),
    );
    dependent.label = "Dependent".into();
    sheet
        .add_canvas_object(CanvasObject::connector(dependent))
        .unwrap();
    Workbook::new(sheet)
}

#[test]
fn drawio_connector_attachment_loss_requires_localized_consent() {
    let book = connector_attachment_book();
    let options = ExportOptions::default();
    let key = "Connector-to-connector attachments detached";
    let message = require_board_consent(&book, Format::DrawIo, &options, false).unwrap_err();
    assert!(
        i18n::Lang::ALL
            .into_iter()
            .any(|language| message.contains(&format!("1 — {}", i18n::t_in(language, key)))),
        "{message}"
    );
    assert!(require_board_consent(&book, Format::DrawIo, &options, true).is_ok());
    assert!(require_board_consent(&book, Format::Thinoda, &options, false).is_ok());
    assert!(
        !SaveConsent {
            board: false,
            mindmanager: true,
        }
        .for_io(Format::DrawIo)
    );
    assert!(
        !SaveConsent {
            board: true,
            mindmanager: false,
        }
        .for_io(Format::MindManager)
    );
    for language in i18n::Lang::ALL {
        let value = i18n::t_in(language, key);
        assert!(!value.is_empty());
        if language != i18n::Lang::En {
            assert_ne!(value, key, "missing {language:?} warning");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn drawio_attachment_consent_reaches_document_and_sheet_saves() {
    use thinoda_core::{CanvasObjectKind, Endpoint};

    let directory = tempfile::tempdir().unwrap();
    let mut doc = OpenDoc::new(connector_attachment_book());
    let sheet_path = directory.path().join("sheet.drawio");
    assert!(
        doc.save_sheet_to_with_consent(&sheet_path, false, false, false)
            .is_err()
    );
    assert!(!sheet_path.exists());
    doc.save_sheet_to_with_consent(&sheet_path, false, true, false)
        .unwrap();

    let document_path = directory.path().join("document.drawio");
    assert!(
        doc.save_to_with_backup_directory_and_consent(
            &document_path,
            false,
            None,
            SaveConsent::default(),
        )
        .is_err()
    );
    assert!(!document_path.exists());
    doc.save_to_with_backup_directory_and_consent(
        &document_path,
        false,
        None,
        SaveConsent {
            board: true,
            mindmanager: false,
        },
    )
    .unwrap();

    for path in [&sheet_path, &document_path] {
        let report =
            thinoda_io::import_bytes_with_report(Format::DrawIo, &std::fs::read(path).unwrap())
                .unwrap();
        let dependent = report.workbook.sheets[0]
            .canvas
            .objects
            .iter()
            .find_map(|object| match &object.kind {
                CanvasObjectKind::Connector(connector) if connector.label == "Dependent" => {
                    Some(connector)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(dependent.from, Endpoint::Point(Vec2::new(175.0, 0.0)));
    }
}

#[test]
fn each_omitted_annotation_independently_requires_consent() {
    for label in [
        "Summary topic trees omitted",
        "Relationships omitted",
        "Boundaries omitted",
    ] {
        let mut sheet = Sheet::new("Map", "Root");
        let first = sheet.add_child(sheet.root, None, "First");
        let second = sheet.add_child(sheet.root, None, "Second");
        match label {
            "Summary topic trees omitted" => {
                let summary = sheet.add_summary(&[first, second], "Summary").unwrap();
                sheet.add_child(summary, None, "Summary detail");
            }
            "Relationships omitted" => {
                sheet.add_relationship(first, second).unwrap();
            }
            "Boundaries omitted" => {
                sheet.add_boundary(&[first, second]).unwrap();
            }
            _ => unreachable!(),
        }
        let book = Workbook::new(sheet);
        book.validate().unwrap();
        let options = ExportOptions::default();
        let message = require_board_consent(&book, Format::Markdown, &options, false).unwrap_err();
        assert!(message.contains(&format!("1 — {}", t(label))), "{message}");
        assert!(require_board_consent(&book, Format::Markdown, &options, true).is_ok());
        assert!(require_board_consent(&book, Format::XMind, &options, false).is_ok());
        assert!(require_board_consent(&book, Format::Thinoda, &options, false).is_ok());
    }
}

#[test]
fn powerpoint_reports_flattened_viewport_slides_even_without_other_losses() {
    let mut sheet = Sheet::new("Presentation", "Root");
    let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0));
    sheet
        .add_presentation_slide("View 1", PresentationSource::Viewport, frame)
        .unwrap();
    sheet
        .add_presentation_slide("View 2", PresentationSource::Viewport, frame)
        .unwrap();
    let workbook = Workbook::new(sheet);
    let report = export_loss_report(&workbook, Format::PowerPoint, &ExportOptions::default());
    assert!(report.losses.is_empty());
    assert_eq!(report.flattened_viewport_slides, 2);
    assert_eq!(
        loss_lines(&report, Format::PowerPoint),
        vec![format!(
            "2 — {}",
            t("Viewport slides flattened to non-editable images")
        )]
    );
    assert!(
        export_loss_report(&workbook, Format::Pdf, &ExportOptions::default()).is_empty(),
        "PDF has no editable-slide promise"
    );
}

#[test]
fn clipping_warns_for_powerpoint_but_not_other_visual_formats() {
    let mut sheet = Sheet::new("Map", "Root");
    let frame = sheet
        .create_board_frame(
            "Frame",
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0)),
        )
        .unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|zone| zone.id == frame)
        .unwrap()
        .clip = true;
    let mut workbook = Workbook::new(sheet);

    for format in [Format::Svg, Format::Png, Format::Jpeg, Format::Pdf] {
        assert!(
            export_loss_report(&workbook, format, &ExportOptions::default()).is_empty(),
            "clipping is preserved in {format:?}"
        );
    }
    let report = export_loss_report(&workbook, Format::PowerPoint, &ExportOptions::default());
    assert_eq!(report.losses, vec![ExportLoss::ZoneClipDropped(1)]);
    assert_eq!(
        loss_lines(&report, Format::PowerPoint),
        vec![format!("1 — {}", t("Zone clipping not fully applied"))]
    );

    let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0));
    workbook
        .active_mut()
        .add_presentation_slide("View", PresentationSource::Viewport, frame)
        .unwrap();
    let report = export_loss_report(&workbook, Format::PowerPoint, &ExportOptions::default());
    assert!(report.losses.is_empty(), "viewport clipping is preserved");
    assert_eq!(report.flattened_viewport_slides, 1);

    let root = workbook.active().root;
    let branch = workbook.active_mut().add_child(root, None, "Branch");
    workbook
        .active_mut()
        .add_presentation_slide("Branch", PresentationSource::Branch(branch), frame)
        .unwrap();
    let report = export_loss_report(&workbook, Format::PowerPoint, &ExportOptions::default());
    assert_eq!(report.losses, vec![ExportLoss::ZoneClipDropped(1)]);
    assert_eq!(report.flattened_viewport_slides, 1);
}

#[test]
fn floating_loss_counts_root_trees_not_their_descendants() {
    let mut sheet = Sheet::new("Map", "Root");
    let floating = sheet.add_floating("Floating root", Vec2::new(10.0, 20.0));
    let child = sheet.add_child(floating, None, "Child");
    sheet.add_child(child, None, "Grandchild");
    let report = export_loss_report(
        &Workbook::new(sheet),
        Format::Markdown,
        &ExportOptions::default(),
    );
    assert!(
        report
            .losses
            .contains(&ExportLoss::FloatingTopicsDropped(1))
    );
    assert!(
        loss_lines(&report, Format::Markdown)
            .contains(&format!("1 — {}", t("Floating root trees omitted")))
    );
}

#[test]
fn presentation_preflight_analyzes_the_draft_and_rejects_stale_owner() {
    for (images, format) in [(false, Format::Pdf), (true, Format::Png)] {
        let mut app = ThinodaApp::headless();
        let ctx = Context::default();
        let doc = app.doc().unwrap();
        let key = doc.key.clone();
        let sheet_index = doc.doc.active_index();
        let revision = doc.doc.state_id();
        let mut draft = doc.doc.sheet().clone();
        draft.board.snap = !draft.board.snap;
        assert!(
            thinoda_io::export_losses(doc.doc.workbook(), format, &ExportOptions::default())
                .is_empty()
        );
        app.request_pitch_export(
            &ctx,
            (&key, sheet_index, revision),
            draft,
            ExportOptions {
                sheet: Some(999),
                ..Default::default()
            },
            images,
        );
        let pending = app.dialogs.board_export.as_ref().unwrap();
        assert_eq!(pending.format, format);
        assert_eq!(
            pending.losses.losses,
            vec![ExportLoss::BoardSettingsDropped]
        );
        assert!(matches!(pending.action, Action::Presentation { .. }));
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Change", |sheet| sheet.set_title(sheet.root, "Changed"))
            .unwrap();
        assert!(app.board_preflight_ui(&ctx));
        assert!(app.dialogs.board_export.is_none());
    }
}

#[test]
fn presentation_player_can_export_a_non_active_sheet_and_close_cancels() {
    let mut workbook = Workbook::default();
    let mut player_sheet = Sheet::new("Presentation", "Root");
    player_sheet.board.snap = !player_sheet.board.snap;
    workbook.sheets.push(player_sheet.clone());
    let mut app = ThinodaApp::headless();
    app.docs.clear();
    app.new_doc(workbook);
    let doc = app.doc().unwrap();
    let key = doc.key.clone();
    let revision = doc.doc.state_id();
    assert_eq!(doc.doc.active_index(), 0);

    let ctx = Context::default();
    app.request_pitch_export(
        &ctx,
        (&key, 1, revision),
        player_sheet,
        ExportOptions::default(),
        false,
    );
    let pending = app.dialogs.board_export.as_ref().unwrap();
    assert!(app.board_preflight_valid(pending, true));
    app.cancel_board_preflight_viewport(&ctx);
    assert!(app.dialogs.board_export.is_none());
}

#[test]
fn presentation_preflight_freezes_the_source_view_appearance() {
    let mut app = ThinodaApp::headless();
    let accent = egui::Color32::from_rgba_unmultiplied(80, 140, 200, 190);
    let doc = app.doc_mut().unwrap();
    doc.view.accent = accent;
    doc.view.dark = true;
    let key = doc.key.clone();
    let sheet = doc.doc.active_index();
    let revision = doc.doc.state_id();
    let mut draft = doc.doc.sheet().clone();
    draft.board.snap = !draft.board.snap;
    let expected = crate::visual_export::for_view(accent, true);
    let ctx = Context::default();
    app.request_pitch_export(
        &ctx,
        (&key, sheet, revision),
        draft,
        ExportOptions::default(),
        false,
    );
    let doc = app.doc_mut().unwrap();
    doc.view.accent = egui::Color32::from_rgb(240, 40, 100);
    doc.view.dark = false;
    let pending = app.dialogs.board_export.as_ref().unwrap();
    assert!(app.board_preflight_valid(pending, true));
    match &pending.action {
        Action::Presentation { visual, .. } => assert_eq!(*visual, expected),
        _ => panic!("expected a presentation preflight"),
    }
}

#[test]
fn print_preflight_uses_the_rendered_snapshot_and_cancel_starts_no_output() {
    let mut app = ThinodaApp::headless();
    let ctx = Context::default();
    let mut snapshot = Workbook::default();
    snapshot.active_mut().board.snap = !snapshot.active().board.snap;
    let document = Arc::new(PrintDocument {
        settings: Default::default(),
        image: image::RgbImage::new(1, 1),
    });
    app.request_print_output(&ctx, &snapshot, 0, document, "Board".into(), true);
    let pending = app.dialogs.board_export.take().unwrap();
    assert_eq!(
        pending.losses.losses,
        vec![ExportLoss::BoardSettingsDropped]
    );
    assert!(matches!(pending.action, Action::Print { .. }));
    app.finish_board_preflight(&ctx, pending, false);
    assert!(!app.printing.active());
}

#[test]
fn xmind_autosave_never_writes_board_loss_in_inline_or_background_mode() {
    use crate::file_ops::FileWorkMode;

    for mode in [FileWorkMode::Inline, FileWorkMode::Background] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("auto.xmind");
        let mut app = ThinodaApp::headless();
        app.set_file_work_mode(mode);
        app.settings.autosave = true;
        app.settings.autosave_interval_secs = 5;
        app.doc_mut().unwrap().save_to(&path, false).unwrap();
        let original = std::fs::read(&path).unwrap();
        let doc = app.doc_mut().unwrap();
        doc.doc
            .edit_sheet("Shape", |sheet| {
                sheet.add_canvas_object(shape()).unwrap();
                Ok(())
            })
            .unwrap();
        doc.tick();
        doc.backdate_change(6);
        app.housekeeping(&Context::default());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(app.doc().unwrap().is_dirty());
        assert!(!app.file_ops.saving());
        assert!(app.toasts.texts().next().is_some());
    }
}
