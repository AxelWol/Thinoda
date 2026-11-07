use std::sync::Mutex;

use egui::{Key, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, FormatTab, Services, ThinodaApp},
    doc::OpenDoc,
    file_ops::{FileBusy, FileWorkMode},
    i18n::{self, Lang, t, t_in},
};
use thinoda_core::{
    CanvasConnector, CanvasObject, Card, CardKind, Color, Endpoint, InkStroke, InkTool,
    PresentationSource, Rect, ShapeKind, Sheet, SheetRootMode, Vec2, Workbook,
};
use thinoda_io::{ExportOptions, Format, export_bytes, import_bytes};
use thinoda_platform::settings::{Language, Settings};

static UI_LOCK: Mutex<()> = Mutex::new(());

fn lock_ui() -> std::sync::MutexGuard<'static, ()> {
    UI_LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

fn board() -> Workbook {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut card = Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0));
    card.rotation = 15.0;
    let card = sheet.add_card("Idea", Vec2::new(200.0, 0.0), card).unwrap();
    let shape = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(80.0, 50.0)),
        ))
        .unwrap();
    sheet
        .add_canvas_object(CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::BLACK,
            2.0,
            vec![Vec2::ZERO, Vec2::new(15.0, 15.0)],
        )))
        .unwrap();
    sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Topic(card),
        )))
        .unwrap();
    let zone = sheet
        .create_board_frame(
            "Frame",
            Rect::from_min_size(Vec2::new(-20.0, -20.0), Vec2::new(400.0, 200.0)),
        )
        .unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|item| item.id == zone)
        .unwrap()
        .clip = true;
    sheet.board.snap = !sheet.board.snap;
    sheet
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(640.0, 480.0)),
        )
        .unwrap();
    let workbook = Workbook::new(sheet);
    workbook.validate().unwrap();
    workbook
}

fn app(book: Workbook, language: Language) -> ThinodaApp {
    let mut app = ThinodaApp::new(
        Settings {
            language,
            backup_on_save: false,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    app.docs.clear();
    app.new_doc(book);
    app
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1000.0))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h
}

#[track_caller]
fn click(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    let node = h
        .query_all(By::new().role(Role::Button).label(label))
        .find(|node| !node.accesskit_node().is_disabled());
    assert!(node.is_some(), "no enabled button: {label}");
    node.unwrap().click();
    h.run_steps(3);
}

#[track_caller]
fn has_label(h: &Harness<'_, ThinodaApp>, text: &str) {
    assert!(
        h.query_all(By::new().label(text)).next().is_some(),
        "missing label: {text}"
    );
}

fn lacks_label(h: &Harness<'_, ThinodaApp>, text: &str) -> bool {
    h.query_all(By::new().label(text)).next().is_none()
}

fn edit_shape_label(h: &mut Harness<'_, ThinodaApp>, text: &str) -> thinoda_core::CanvasObjectId {
    let doc = h.state_mut().doc_mut().unwrap();
    let id = doc
        .doc
        .sheet()
        .canvas
        .objects
        .iter()
        .find(|object| matches!(object.kind, thinoda_core::CanvasObjectKind::Shape { .. }))
        .unwrap()
        .id;
    doc.view.select_board_object(id);
    assert!(doc.view.begin_board_label(&doc.doc, id));
    h.run_steps(2);
    h.get_by_label(t("Board object label")).type_text(text);
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().view.board_label_draft().unwrap().2,
        text
    );
    id
}

fn right_click_sheet_tab(h: &mut Harness<'_, ThinodaApp>, title: &str) {
    let position = h
        .query_all(By::new().role(Role::Button).label(title))
        .max_by(|a, b| a.rect().center().y.total_cmp(&b.rect().center().y))
        .unwrap()
        .rect()
        .center();
    h.event(egui::Event::PointerMoved(position));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(3);
}

fn board_with_other_sheet() -> Workbook {
    let mut book = board();
    book.sheets.push(Sheet::new("Other sheet", "Other root"));
    book
}

fn annotated_central_map() -> Workbook {
    let mut sheet = Sheet::new("Annotated map", "Root");
    let first = sheet.add_child(sheet.root, None, "First");
    let second = sheet.add_child(sheet.root, None, "Second");
    sheet.add_relationship(first, second).unwrap();
    sheet.add_boundary(&[first, second]).unwrap();
    let summary = sheet.add_summary(&[first, second], "Summary").unwrap();
    let child = sheet.add_child(summary, None, "Summary child");
    let sibling = sheet.add_child(summary, None, "Summary sibling");
    let nested = sheet
        .add_summary(&[child, sibling], "Nested summary")
        .unwrap();
    sheet.add_child(nested, None, "Nested descendant");
    let book = Workbook::new(sheet);
    book.validate().unwrap();
    book
}

#[test]
fn sheet_tab_switch_commits_a_pending_board_label() {
    let _guard = lock_ui();
    let mut h = harness(app(board_with_other_sheet(), Language::English));
    let id = edit_shape_label(&mut h, "Saved before switching");
    click(&mut h, "Other sheet");

    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.active_index(), 1);
    assert_eq!(
        doc.doc.workbook().sheets[0]
            .canvas
            .object(id)
            .unwrap()
            .label(),
        "Saved before switching"
    );
    assert!(doc.view.board_label_draft().is_none());
}

#[test]
fn failed_board_label_commit_blocks_sheet_tab_and_context_switches() {
    let _guard = lock_ui();
    let mut h = harness(app(board_with_other_sheet(), Language::English));
    let draft = "x".repeat(thinoda_core::board::MAX_BOARD_TEXT_CHARS + 1);
    let id = edit_shape_label(&mut h, &draft);
    click(&mut h, "Other sheet");

    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.active_index(), 0);
    assert_eq!(doc.view.editing_board_object(), Some(id));
    assert_eq!(doc.view.board_label_draft().unwrap().2, draft);
    assert!(
        h.state()
            .toasts
            .texts()
            .any(|text| text.contains("exceeds"))
    );

    right_click_sheet_tab(&mut h, "Other sheet");
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.active_index(), 0);
    assert_eq!(doc.view.editing_board_object(), Some(id));
    assert_eq!(doc.view.board_label_draft().unwrap().2, draft);
    assert!(lacks_label(&h, t("Move Left")));
}

#[test]
fn sheet_move_commits_valid_drafts_and_preserves_a_failed_title_draft() {
    let _guard = lock_ui();
    let mut h = harness(app(board_with_other_sheet(), Language::English));
    let id = edit_shape_label(&mut h, "Saved before moving");
    right_click_sheet_tab(&mut h, "Board");
    h.get_by_label(t("Move Right")).scroll_to_me();
    h.run_steps(3);
    click(&mut h, t("Move Right"));
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.active_index(), 1);
    assert_eq!(doc.doc.sheet().title, "Board");
    assert_eq!(
        doc.doc.sheet().canvas.object(id).unwrap().label(),
        "Saved before moving"
    );

    right_click_sheet_tab(&mut h, "Board");
    let doc = h.state_mut().doc_mut().unwrap();
    let topic = doc
        .doc
        .sheet()
        .topics
        .values()
        .find(|topic| topic.title == "Idea")
        .unwrap()
        .id;
    doc.view.begin_edit(doc.doc.sheet(), topic, false);
    doc.view.set_editing_text("Pending title");
    doc.doc
        .edit_sheet("Concurrent title", |sheet| {
            sheet.set_title(topic, "Changed elsewhere")
        })
        .unwrap();
    h.get_by_label(t("Move Left")).scroll_to_me();
    h.run_steps(3);
    click(&mut h, t("Move Left"));
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.active_index(), 1);
    assert_eq!(doc.doc.sheet().title, "Board");
    assert_eq!(doc.view.editing_text(), Some("Pending title"));
    assert_eq!(
        doc.doc.sheet().topic(topic).unwrap().title,
        "Changed elsewhere"
    );
    assert!(
        h.state()
            .toasts
            .texts()
            .any(|text| text.contains("draft retained"))
    );
}

#[test]
fn export_commits_active_board_label_before_the_loss_dialog() {
    let _guard = lock_ui();
    let mut h = harness(app(board(), Language::English));
    let id = edit_shape_label(&mut h, "Edited board label");
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Export(Format::Markdown));
    h.run_steps(3);

    let doc = h.state().doc().unwrap();
    assert_eq!(
        doc.doc.sheet().canvas.object(id).unwrap().label(),
        "Edited board label"
    );
    assert!(doc.view.board_label_draft().is_none());
    has_label(&h, t("Board export changes"));
    has_label(&h, &format!("1 — {}", t("Shapes omitted")));
    click(&mut h, t("Cancel"));
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .label(),
        "Edited board label"
    );
}

#[test]
fn export_rejects_invalid_board_label_and_retains_the_draft() {
    let _guard = lock_ui();
    let mut h = harness(app(board(), Language::English));
    let draft = "x".repeat(thinoda_core::board::MAX_BOARD_TEXT_CHARS + 1);
    let id = edit_shape_label(&mut h, &draft);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Export(Format::Markdown));
    h.run_steps(3);

    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.sheet().canvas.object(id).unwrap().label(), "");
    assert_eq!(doc.view.board_label_draft().unwrap().2, draft);
    assert!(lacks_label(&h, t("Board export changes")));
    assert!(
        h.state()
            .toasts
            .texts()
            .any(|text| text.contains("exceeds")),
        "the validation error must be visible"
    );
}

#[test]
fn sheet_save_includes_a_board_label_draft_without_mutating_the_source() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sheet.thinoda");
    let mut h = harness(app(board(), Language::English));
    let id = edit_shape_label(&mut h, "Saved draft");
    assert!(h.state_mut().save_sheet_to(&path));

    let saved = OpenDoc::open(&path).unwrap();
    assert_eq!(
        saved.doc.sheet().canvas.object(id).unwrap().label(),
        "Saved draft"
    );
    let source = h.state().doc().unwrap();
    assert_eq!(source.doc.sheet().canvas.object(id).unwrap().label(), "");
    assert_eq!(source.view.board_label_draft().unwrap().2, "Saved draft");
}

#[test]
fn sheet_save_rejects_an_invalid_board_label_before_writing() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sheet.thinoda");
    let mut h = harness(app(board(), Language::English));
    let draft = "x".repeat(thinoda_core::board::MAX_BOARD_TEXT_CHARS + 1);
    let id = edit_shape_label(&mut h, &draft);
    assert!(!h.state_mut().save_sheet_to(&path));
    assert!(!path.exists());

    let source = h.state().doc().unwrap();
    assert_eq!(source.doc.sheet().canvas.object(id).unwrap().label(), "");
    assert_eq!(source.view.board_label_draft().unwrap().2, draft);
    assert!(
        h.state()
            .toasts
            .texts()
            .any(|text| text.contains("exceeds"))
    );
}

#[test]
fn itemized_dialog_is_localized_accessible_and_escape_cancels() {
    let _guard = lock_ui();
    for language in Language::ALL {
        let book = board();
        let mut h = harness(app(book.clone(), language));
        let revision = h.state().doc().unwrap().doc.state_id();
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Export(Format::Markdown));
        h.run_steps(3);
        has_label(&h, t("Board export changes"));
        for key in [
            "Shapes omitted",
            "Ink strokes omitted",
            "Connectors omitted",
            "Cards converted to ordinary topics",
            "Card rotations omitted",
            "Viewport slides omitted",
            "Zones omitted",
        ] {
            has_label(&h, &format!("1 — {}", t(key)));
        }
        has_label(&h, t("Board grid and snapping settings omitted"));
        has_label(
            &h,
            &t("Sheet {sheet}: a root named “{title}” will be created.")
                .replace("{sheet}", "1")
                .replace("{title}", "Board"),
        );
        for label in ["Confirm", "Cancel"] {
            assert!(
                h.query_all(By::new().role(Role::Button).label(t(label)))
                    .any(|node| !node.accesskit_node().is_disabled()),
                "missing enabled button: {}",
                t(label)
            );
        }
        h.key_press(Key::Insert);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
        h.key_press(Key::Escape);
        h.run_steps(3);
        assert!(lacks_label(&h, t("Board export changes")));
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
    }
}

#[test]
fn omitted_outline_annotations_alone_open_accessible_localized_preflight() {
    let _guard = lock_ui();
    for language in Language::ALL {
        let book = annotated_central_map();
        let mut h = harness(app(book.clone(), language));
        let revision = h.state().doc().unwrap().doc.state_id();
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Export(Format::Markdown));
        h.run_steps(3);
        has_label(&h, t("Board export changes"));
        for (count, label) in [
            (2, "Summary topic trees omitted"),
            (1, "Relationships omitted"),
            (1, "Boundaries omitted"),
        ] {
            let line = format!("{count} — {}", t(label));
            assert!(
                h.query_all(By::new().role(Role::Label).label(&line))
                    .next()
                    .is_some(),
                "{language:?}: inaccessible loss row {line}"
            );
        }
        assert!(lacks_label(&h, &format!("1 — {}", t("Shapes omitted"))));
        for label in ["Confirm", "Cancel"] {
            assert!(
                h.query_all(By::new().role(Role::Button).label(t(label)))
                    .any(|node| !node.accesskit_node().is_disabled()),
                "{language:?}: missing enabled {label}"
            );
        }
        h.key_press(Key::Escape);
        h.run_steps(3);
        assert!(lacks_label(&h, t("Board export changes")));
        assert!(h.state().file_activity().is_empty());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
    }
}

#[test]
fn omitted_central_floating_card_is_not_described_as_converted() {
    let _guard = lock_ui();
    let mut sheet = Sheet::new("Map", "Root");
    sheet
        .add_card(
            "Card",
            Vec2::new(100.0, 100.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet);
    book.validate().unwrap();
    let mut h = harness(app(book.clone(), Language::English));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Export(Format::Markdown));
    h.run_steps(3);
    has_label(&h, "1 — Cards omitted");
    has_label(&h, "1 — Floating root trees omitted");
    assert!(lacks_label(&h, "1 — Cards converted to ordinary topics"));
    click(&mut h, t("Cancel"));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn narrow_preflight_keeps_both_actions_inside_the_window() {
    let _guard = lock_ui();
    for language in Language::ALL {
        for (size, scale) in [
            (egui::vec2(480.0, 360.0), 1.0),
            (egui::vec2(600.0, 450.0), 1.5),
        ] {
            let book = board();
            let mut app = app(book.clone(), language);
            app.settings.ui_scale = scale;
            let mut h = Harness::builder()
                .with_size(size)
                .allow_missing_glyphs()
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Export(Format::Markdown));
            h.run_steps(5);
            let rect = ctx
                .memory(|memory| memory.area_rect(egui::Id::unique("board_export_preflight")))
                .unwrap();
            assert!(
                ctx.content_rect().expand(1.0).contains_rect(rect),
                "{language:?}, scale {scale}: {rect:?}"
            );
            for label in ["Confirm", "Cancel"] {
                let button = h
                    .query_all(By::new().role(Role::Button).label(t(label)))
                    .find(|button| rect.contains_rect(button.rect()))
                    .unwrap_or_else(|| panic!("{language:?}, scale {scale}: {label}"));
                assert!(
                    ctx.content_rect().contains_rect(button.rect()),
                    "{language:?}, scale {scale}: {label}"
                );
            }
            click(&mut h, t("Cancel"));
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
        }
    }
}

#[test]
fn all_catalogs_preserve_preflight_keys_and_placeholders() {
    let _guard = lock_ui();
    let keys = [
        "Board export changes",
        "Editable image export",
        "Only the current sheet is included; other sheets are not exported.",
        "This image contains the editable diagram, including content outside the visible preview. It is not redacted.",
        "The selected format changes or omits board content. The board in memory stays unchanged; save as Thinoda to preserve it.",
        "Shapes omitted",
        "Ink strokes omitted",
        "Connectors omitted",
        "Cards converted to ordinary topics",
        "Card presentation omitted",
        "Cards omitted",
        "Card rotations omitted",
        "Viewport slides omitted",
        "Zones omitted",
        "Zone clipping not fully applied",
        "Viewport slides flattened to non-editable images",
        "XMind stores a Thinoda-only copy of the complete board. Other apps may not display it and may discard it when saving. This export is not redacted.",
        "Floating root trees omitted",
        "Summary topic trees omitted",
        "Relationships omitted",
        "Boundaries omitted",
        "Board grid and snapping settings omitted",
        "Sheet {sheet}: a root named “{title}” will be created.",
        "Sheet {sheet}: hidden root mode omitted.",
        "Autosave skipped because this format cannot preserve board content. Save as Thinoda to keep it.",
        "Use --allow-lossy only if you accept these changes.",
    ];
    for lang in Lang::ALL {
        for key in keys {
            let translated = t_in(lang, key);
            assert!(!translated.trim().is_empty(), "{lang:?}: {key}");
            if lang != Lang::En {
                assert_ne!(translated, key, "{lang:?}: untranslated {key}");
            }
            for placeholder in ["{sheet}", "{title}", "--allow-lossy"] {
                assert_eq!(
                    translated.matches(placeholder).count(),
                    key.matches(placeholder).count(),
                    "{lang:?}: {key}"
                );
            }
        }
    }
}

#[test]
fn export_menu_preflights_every_lossy_format_before_opening_a_picker() {
    let _guard = lock_ui();
    for &format in Format::EXPORTABLE {
        if matches!(format, Format::Thinoda | Format::FimXdf2 | Format::FimXdf3) {
            continue;
        }
        let book = board();
        assert!(
            !thinoda_io::export_losses(&book, format, &ExportOptions::default()).is_empty(),
            "{format:?}"
        );
        let mut h = harness(app(book.clone(), Language::English));
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Export(format));
        h.run_steps(3);
        if matches!(format, Format::MindManager | Format::PowerPoint) {
            click(&mut h, t("Export"));
        }
        let title = if format.is_drawio_image() {
            "Editable image export"
        } else {
            "Board export changes"
        };
        has_label(&h, t(title));
        if format.is_drawio_image() {
            has_label(
                &h,
                t("Only the current sheet is included; other sheets are not exported."),
            );
            has_label(
                &h,
                t(
                    "This image contains the editable diagram, including content outside the visible preview. It is not redacted.",
                ),
            );
        }
        if matches!(
            format,
            Format::Svg | Format::Png | Format::Jpeg | Format::Pdf | Format::PowerPoint
        ) {
            has_label(&h, t("Board grid and snapping settings omitted"));
            let clip_loss = format!("1 — {}", t("Zone clipping not fully applied"));
            // This fixture has a viewport-only deck; its PPTX slide uses the
            // clipped raster path too. Flattening is reported separately.
            assert!(lacks_label(&h, &clip_loss), "{format:?}");
            if format == Format::PowerPoint {
                has_label(
                    &h,
                    &format!(
                        "1 — {}",
                        t("Viewport slides flattened to non-editable images")
                    ),
                );
            }
            assert!(lacks_label(&h, &format!("1 — {}", t("Shapes omitted"))));
        } else if format.is_drawio() {
            // draw.io keeps cards, shapes, ink, connectors and frames editable.
            has_label(&h, t("Board grid and snapping settings omitted"));
            has_label(&h, &format!("1 — {}", t("Presentation slides omitted")));
            assert!(lacks_label(&h, &format!("1 — {}", t("Shapes omitted"))));
        } else {
            has_label(&h, &format!("1 — {}", t("Shapes omitted")));
        }
        click(&mut h, t("Cancel"));
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &book, "{format:?}");
    }
}

#[test]
fn editable_image_disclosure_is_localized_and_mandatory_without_losses() {
    let _guard = lock_ui();
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    sheet.board.grid = thinoda_core::GridKind::Lines;
    sheet.board.snap = false;
    sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(80.0, 50.0)),
        ))
        .unwrap();
    let book = Workbook::new(sheet);
    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        assert!(thinoda_io::export_losses(&book, format, &ExportOptions::default()).is_empty());
        for language in Language::ALL {
            let mut h = harness(app(book.clone(), language));
            let revision = h.state().doc().unwrap().doc.state_id();
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Export(format));
            h.run_steps(3);
            has_label(&h, t("Editable image export"));
            has_label(&h, t(format.label()));
            has_label(
                &h,
                t("Only the current sheet is included; other sheets are not exported."),
            );
            has_label(
                &h,
                t(
                    "This image contains the editable diagram, including content outside the visible preview. It is not redacted.",
                ),
            );
            for label in ["Confirm", "Cancel"] {
                assert!(
                    h.query_all(By::new().role(Role::Button).label(t(label)))
                        .any(|node| !node.accesskit_node().is_disabled()),
                    "{format:?}, {language:?}: missing enabled {label}"
                );
            }
            h.key_press(Key::Escape);
            h.run_steps(3);
            assert!(lacks_label(&h, t("Editable image export")));
            assert!(h.state().file_activity().is_empty());
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
        }
    }
}

#[test]
fn editable_image_disclosure_rejects_a_changed_document_or_active_sheet() {
    let _guard = lock_ui();
    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        for change_sheet in [false, true] {
            let mut book = board_with_other_sheet();
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source.drawio");
            std::fs::write(&source, b"source diagram").unwrap();
            let mut app = app(book.clone(), Language::English);
            app.doc_mut().unwrap().import_source = Some(source.clone());
            let mut h = harness(app);
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Export(format));
            h.run_steps(3);
            has_label(&h, t("Editable image export"));
            if change_sheet {
                h.state_mut().doc_mut().unwrap().doc.set_active(1);
                book.active_sheet = 1;
            } else {
                h.state_mut()
                    .doc_mut()
                    .unwrap()
                    .doc
                    .edit_sheet("Change board", |sheet| {
                        sheet.board.snap = !sheet.board.snap;
                        Ok(())
                    })
                    .unwrap();
                book.active_mut().board.snap = !book.active().board.snap;
            }
            h.run_steps(3);
            assert!(lacks_label(&h, t("Editable image export")));
            assert!(h.state().file_activity().is_empty());
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
            assert_eq!(std::fs::read(&source).unwrap(), b"source diagram");
            assert!(
                h.state()
                    .doc()
                    .unwrap()
                    .ensure_export_destination(&source)
                    .is_err()
            );
        }
    }
}

#[test]
fn editable_image_disclosure_includes_connector_losses() {
    let _guard = lock_ui();
    let mut book = board();
    let target = book
        .active()
        .canvas
        .objects
        .iter()
        .find(|object| matches!(object.kind, thinoda_core::CanvasObjectKind::Connector(_)))
        .unwrap()
        .id;
    book.active_mut()
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(target),
            Endpoint::Point(Vec2::new(400.0, 100.0)),
        )))
        .unwrap();
    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        let mut h = harness(app(book.clone(), Language::English));
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Export(format));
        h.run_steps(3);
        has_label(&h, t("Editable image export"));
        has_label(
            &h,
            &format!("1 — {}", t("Connector-to-connector attachments detached")),
        );
        click(&mut h, t("Cancel"));
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
    }
}

#[test]
fn editable_image_disclosure_actions_fit_small_scaled_windows() {
    let _guard = lock_ui();
    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        for language in Language::ALL {
            for (size, scale) in [
                (egui::vec2(480.0, 360.0), 1.0),
                (egui::vec2(600.0, 450.0), 1.5),
            ] {
                let mut app = app(board(), language);
                app.settings.ui_scale = scale;
                let mut h = Harness::builder()
                    .with_size(size)
                    .allow_missing_glyphs()
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::Export(format));
                h.run_steps(5);
                let rect = ctx
                    .memory(|memory| memory.area_rect(egui::Id::unique("board_export_preflight")))
                    .unwrap();
                assert!(
                    ctx.content_rect().expand(1.0).contains_rect(rect),
                    "{format:?}, {language:?}, scale {scale}: {rect:?}"
                );
                for label in ["Confirm", "Cancel"] {
                    let button = h
                        .query_all(By::new().role(Role::Button).label(t(label)))
                        .find(|button| rect.contains_rect(button.rect()))
                        .unwrap_or_else(|| {
                            panic!("{format:?}, {language:?}, scale {scale}: {label}")
                        });
                    assert!(
                        ctx.content_rect().contains_rect(button.rect()),
                        "{format:?}, {language:?}, scale {scale}: {label}"
                    );
                }
                click(&mut h, t("Cancel"));
            }
        }
    }
}

#[test]
fn sheet_save_cancellation_and_stale_consent_preserve_destination() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("sheet.xmind");
    std::fs::write(&destination, b"existing").unwrap();
    let book = board();
    let mut app = app(book.clone(), Language::English);
    assert!(!app.save_sheet_to(&destination));
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
    let mut h = harness(app);
    click(&mut h, t("Cancel"));
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);

    assert!(!h.state_mut().save_sheet_to(&destination));
    h.run_steps(3);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Change board", |sheet| {
            sheet.board.snap = !sheet.board.snap;
            Ok(())
        })
        .unwrap();
    h.run_steps(3);
    assert!(lacks_label(&h, t("Board export changes")));
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
}

#[test]
fn sheet_save_accepts_only_the_selected_sheet_and_preserves_source() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("sheet.xmind");
    std::fs::write(&destination, b"existing").unwrap();
    let mut book = board();
    let mut other = board().sheets.remove(0);
    other.title = "Second board".into();
    other.canvas.objects.clear();
    book.sheets.push(other);
    book.active_sheet = 1;
    let mut app = app(book.clone(), Language::English);
    assert!(!app.save_sheet_to(&destination));
    let mut h = harness(app);
    assert!(
        lacks_label(&h, &format!("1 — {}", t("Shapes omitted"))),
        "the other sheet's shape must not be reported for a sheet-only save"
    );
    has_label(&h, "Sheet 1: a root named “Second board” will be created.");
    click(&mut h, t("Confirm"));
    assert_ne!(std::fs::read(&destination).unwrap(), b"existing");
    let result = OpenDoc::open(&destination).unwrap();
    assert_eq!(result.doc.workbook().sheets.len(), 1);
    assert_eq!(result.doc.workbook().active(), book.active());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
    assert!(h.state().doc().unwrap().path.is_none());
}

#[test]
fn xmind_export_reports_losses_across_all_sheets() {
    let _guard = lock_ui();
    let mut book = board();
    let mut second = board().sheets.remove(0);
    second.title = "Another board".into();
    book.sheets.push(second);
    let mut h = harness(app(book.clone(), Language::English));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Export(Format::XMind));
    h.run_steps(3);
    has_label(&h, &format!("2 — {}", t("Shapes omitted")));
    has_label(&h, "Sheet 1: a root named “Board” will be created.");
    has_label(&h, "Sheet 2: a root named “Another board” will be created.");
    has_label(
        &h,
        t(
            "XMind stores a Thinoda-only copy of the complete board. Other apps may not display it and may discard it when saving. This export is not redacted.",
        ),
    );
    click(&mut h, t("Cancel"));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn full_xmind_save_and_close_require_board_consent() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.xmind");
    let mut app = app(Workbook::default(), Language::English);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let original = std::fs::read(&path).unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Add shape", |sheet| {
            sheet
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 40.0)),
                ))
                .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(app.doc_mut().unwrap().save_to(&path, false).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    app.request_close_tab(0);
    let mut h = harness(app);
    click(&mut h, t("Save"));
    has_label(&h, &format!("1 — {}", t("Shapes omitted")));
    click(&mut h, t("Cancel"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(h.state().doc().unwrap().is_dirty());
    assert_eq!(h.state().docs.len(), 1);
    h.state_mut().request_close_tab(0);
    h.run_steps(3);
    click(&mut h, t("Save"));
    click(&mut h, t("Confirm"));
    assert!(h.state().docs.is_empty());
    assert_ne!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn background_save_waits_for_board_consent_before_queuing() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("background.xmind");
    let mut app = app(Workbook::default(), Language::English);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let original = std::fs::read(&path).unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Add shape", |sheet| {
            sheet
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 40.0)),
                ))
                .unwrap();
            Ok(())
        })
        .unwrap();
    app.set_file_work_mode(FileWorkMode::Background);
    let mut h = harness(app);
    h.state().pause_file_workers(true);
    let ctx = h.ctx.clone();

    h.state_mut().exec(&ctx, Cmd::Save);
    h.run_steps(3);
    has_label(&h, t("Board export changes"));
    assert!(h.state().file_activity().is_empty());
    click(&mut h, t("Cancel"));
    assert!(h.state().file_activity().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), original);

    h.state_mut().exec(&ctx, Cmd::Save);
    h.run_steps(3);
    click(&mut h, t("Confirm"));
    assert!(matches!(
        h.state().file_activity().as_slice(),
        [FileBusy::Saving { .. }]
    ));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    h.state().pause_file_workers(false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.state().file_activity().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "save worker timed out"
        );
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_ne!(std::fs::read(&path).unwrap(), original);
    assert!(!h.state().doc().unwrap().is_dirty());
}

#[test]
fn mindmanager_save_keeps_both_confirmations_and_acceptance_writes() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sheet.mmap");
    std::fs::write(&path, b"existing").unwrap();
    let book = board();
    let mut app = app(book.clone(), Language::English);
    assert!(!app.save_sheet_to(&path));
    let mut h = harness(app);
    assert!(h.state().dialogs.mindmanager_save.is_some());
    click(&mut h, t("Save"));
    has_label(&h, t("Board export changes"));
    assert_eq!(std::fs::read(&path).unwrap(), b"existing");
    let board_rect = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("board_export_preflight")))
        .expect("board dialog bounds");
    let cancel = h
        .query_all(By::new().role(Role::Button).label(t("Cancel")))
        .find(|node| board_rect.contains_rect(node.rect()))
        .expect("board dialog cancel");
    cancel.click();
    h.run_steps(3);
    assert!(lacks_label(&h, t("Board export changes")));
    assert_eq!(std::fs::read(&path).unwrap(), b"existing");
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);

    assert!(!h.state_mut().save_sheet_to(&path));
    assert!(h.state().dialogs.mindmanager_save.is_some());
    h.run_steps(3);
    assert!(
        h.state().dialogs.mindmanager_save.is_some(),
        "MindManager confirmation was dismissed unexpectedly"
    );
    click(&mut h, t("Save"));
    click(&mut h, t("Confirm"));
    assert_ne!(std::fs::read(&path).unwrap(), b"existing");
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn mindmanager_annotation_only_save_cancellation_never_writes() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("annotated.mmap");
    std::fs::write(&path, b"existing").unwrap();
    let book = annotated_central_map();
    let mut app = app(book.clone(), Language::English);
    assert!(!app.save_sheet_to(&path));
    let mut h = harness(app);
    assert!(h.state().dialogs.mindmanager_save.is_some());
    click(&mut h, t("Save"));
    has_label(&h, "2 — Summary topic trees omitted");
    has_label(&h, "1 — Relationships omitted");
    has_label(&h, "1 — Boundaries omitted");
    assert_eq!(std::fs::read(&path).unwrap(), b"existing");
    click(&mut h, t("Cancel"));
    assert_eq!(std::fs::read(&path).unwrap(), b"existing");
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);

    assert!(!h.state_mut().save_sheet_to(&path));
    h.run_steps(3);
    click(&mut h, t("Save"));
    click(&mut h, t("Confirm"));
    assert_ne!(std::fs::read(&path).unwrap(), b"existing");
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn save_all_pauses_for_consent_and_cancel_does_not_save_other_documents() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let xmind = directory.path().join("first.xmind");
    let native = directory.path().join("second.thinoda");
    let mut app = app(Workbook::default(), Language::English);
    app.doc_mut().unwrap().save_to(&xmind, false).unwrap();
    let xmind_before = std::fs::read(&xmind).unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Add shape", |sheet| {
            sheet
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 40.0)),
                ))
                .unwrap();
            Ok(())
        })
        .unwrap();
    app.new_doc(Workbook::default());
    app.doc_mut().unwrap().save_to(&native, false).unwrap();
    let native_before = std::fs::read(&native).unwrap();
    let root = app.doc().unwrap().doc.sheet().root;
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Change title", |sheet| sheet.set_title(root, "Changed"))
        .unwrap();
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::SaveAll);
    h.run_steps(3);
    has_label(&h, t("Board export changes"));
    assert_eq!(h.state().active, 1, "the original active tab is restored");
    click(&mut h, t("Cancel"));
    assert_eq!(std::fs::read(&xmind).unwrap(), xmind_before);
    assert_eq!(std::fs::read(&native).unwrap(), native_before);
    assert!(h.state().docs.iter().all(OpenDoc::is_dirty));

    h.state_mut().exec(&ctx, Cmd::SaveAll);
    h.run_steps(3);
    click(&mut h, t("Confirm"));
    assert!(h.state().docs.iter().all(|doc| !doc.is_dirty()));
    assert_ne!(std::fs::read(&xmind).unwrap(), xmind_before);
    assert_ne!(std::fs::read(&native).unwrap(), native_before);
    assert_eq!(h.state().active, 1);
}

#[test]
fn background_save_all_keeps_both_mindmanager_and_board_consent() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let mindmanager = directory.path().join("first.mmap");
    let native = directory.path().join("second.thinoda");
    let mut app = app(Workbook::default(), Language::English);
    app.doc_mut().unwrap().save_to(&mindmanager, false).unwrap();
    let mindmanager_before = std::fs::read(&mindmanager).unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Change title and add shape", |sheet| {
            sheet.set_title(sheet.root, "Updated MindManager root")?;
            sheet
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 40.0)),
                ))
                .unwrap();
            Ok(())
        })
        .unwrap();
    app.new_doc(Workbook::default());
    app.doc_mut().unwrap().save_to(&native, false).unwrap();
    let native_before = std::fs::read(&native).unwrap();
    let root = app.doc().unwrap().doc.sheet().root;
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Change title", |sheet| sheet.set_title(root, "Changed"))
        .unwrap();
    app.set_file_work_mode(FileWorkMode::Background);
    let mut h = harness(app);
    h.state().pause_file_workers(true);
    let ctx = h.ctx.clone();

    h.state_mut().exec(&ctx, Cmd::SaveAll);
    h.run_steps(3);
    assert!(h.state().dialogs.mindmanager_save.is_some());
    click(&mut h, t("Save"));
    has_label(&h, t("Board export changes"));
    click(&mut h, t("Confirm"));
    assert!(matches!(
        h.state().file_activity().as_slice(),
        [FileBusy::Saving { .. }]
    ));
    assert_eq!(std::fs::read(&mindmanager).unwrap(), mindmanager_before);
    assert_eq!(std::fs::read(&native).unwrap(), native_before);

    h.state().pause_file_workers(false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.state().file_activity().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "save worker timed out"
        );
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(h.state().docs.iter().all(|doc| !doc.is_dirty()));
    let mindmanager_after = std::fs::read(&mindmanager).unwrap();
    let native_after = std::fs::read(&native).unwrap();
    assert_ne!(mindmanager_after, mindmanager_before);
    assert_ne!(native_after, native_before);
    assert_eq!(
        import_bytes(Format::MindManager, &mindmanager_after)
            .unwrap()
            .active()
            .root_topic()
            .title,
        "Updated MindManager root"
    );
    assert_eq!(
        import_bytes(Format::Thinoda, &native_after)
            .unwrap()
            .active()
            .root_topic()
            .title,
        "Changed"
    );
    assert_eq!(h.state().docs[0].doc.sheet().canvas.objects.len(), 1);
    assert_eq!(h.state().active, 1);
}

#[test]
fn native_save_remains_lossless_without_a_preflight() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("board.thinoda");
    let book = board();
    let mut app = app(book.clone(), Language::English);
    assert!(app.save_sheet_to(&path));
    assert_eq!(
        import_bytes(Format::Thinoda, &std::fs::read(path).unwrap()).unwrap(),
        book
    );
}

#[test]
fn symbol_note_and_format_warnings_remain_independent_of_board_consent() {
    let _guard = lock_ui();
    let mut sheet = Sheet::new("Notes", "Root");
    let root = sheet.root;
    sheet
        .set_markdown_notes(root, Some("**bold**".into()))
        .unwrap();
    let topic = sheet.topic_mut(root).unwrap();
    topic.markers.push("priority-1".into());
    topic.marker_colors.insert(0, Color::BLACK);
    sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(20.0, 20.0)),
        ))
        .unwrap();
    let mut h = harness(app(Workbook::new(sheet), Language::English));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Export(Format::Opml));
    h.run_steps(3);
    assert!(h.state().dialogs.symbol_export.is_some());
    click(&mut h, t("Export"));
    assert_eq!(h.state().dialogs.note_export, Some((Format::Opml, true)));
    click(&mut h, t("Export"));
    assert!(h.state().dialogs.note_export.is_none());
    has_label(&h, t("Board export changes"));
    click(&mut h, t("Cancel"));

    h.state_mut().exec(&ctx, Cmd::Export(Format::MindManager));
    h.run_steps(3);
    if h.state().dialogs.symbol_export.is_some() {
        click(&mut h, t("Export"));
    }
    assert!(h.state().dialogs.mindmanager_export);
    click(&mut h, t("Export"));
    has_label(&h, t("Board export changes"));
    click(&mut h, t("Cancel"));

    h.state_mut().exec(&ctx, Cmd::Export(Format::PowerPoint));
    h.run_steps(3);
    // PowerPoint renders shapes, so a plain shape is not a board loss.
    assert!(h.state().dialogs.powerpoint_export);
    click(&mut h, t("Cancel"));
}

#[test]
fn powerpoint_viewport_flattening_alone_requires_consent() {
    let _guard = lock_ui();
    let mut sheet = Sheet::new("Presentation", "Root");
    sheet
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet);
    assert!(
        thinoda_io::export_losses(&book, Format::PowerPoint, &ExportOptions::default()).is_empty()
    );
    let mut h = harness(app(book.clone(), Language::English));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Export(Format::PowerPoint));
    h.run_steps(3);
    assert!(h.state().dialogs.powerpoint_export);
    click(&mut h, t("Export"));
    has_label(
        &h,
        &format!(
            "1 — {}",
            t("Viewport slides flattened to non-editable images")
        ),
    );
    click(&mut h, t("Cancel"));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn powerpoint_mixed_slides_itemize_clipping_and_flattening() {
    let _guard = lock_ui();
    let mut book = board();
    let branch = book.active().semantic_roots()[0];
    book.active_mut()
        .add_presentation_slide(
            "Branch",
            PresentationSource::Branch(branch),
            Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
        )
        .unwrap();
    let mut h = harness(app(book.clone(), Language::English));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Export(Format::PowerPoint));
    h.run_steps(3);
    click(&mut h, t("Export"));
    has_label(&h, &format!("1 — {}", t("Zone clipping not fully applied")));
    has_label(
        &h,
        &format!(
            "1 — {}",
            t("Viewport slides flattened to non-editable images")
        ),
    );
    click(&mut h, t("Cancel"));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn presentation_apply_and_export_powerpoint_warns_before_the_picker() {
    let _guard = lock_ui();
    let mut sheet = Sheet::new("Presentation", "Root");
    sheet
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet);
    let mut h = harness(app(book.clone(), Language::English));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditPresentation);
    h.run_steps(4);
    let export = h
        .query_all(
            By::new()
                .role(Role::Button)
                .label(t("Apply and export PowerPoint")),
        )
        .find(|node| !node.accesskit_node().is_disabled())
        .unwrap();
    assert!(
        ctx.content_rect().contains_rect(export.rect()),
        "presentation export button outside viewport: {:?}",
        export.rect()
    );
    let button_rect = export.rect();
    let editor_rect =
        ctx.memory(|memory| memory.area_rect(egui::Id::unique("presentation-editor")));
    let button_layer = ctx.layer_id_at(button_rect.center());
    let hit_rects: Vec<_> = ctx
        .interactive_rects_last_pass()
        .into_iter()
        .filter(|rect| {
            rect.intersects(button_rect)
                && rect.width() <= button_rect.width() + 1.0
                && rect.height() <= button_rect.height() + 1.0
        })
        .collect();
    assert!(
        hit_rects
            .iter()
            .any(|rect| rect.contains(button_rect.center())),
        "presentation export button is clipped: button {button_rect:?}, editor {editor_rect:?}, \
         layer {button_layer:?}, hit rects {hit_rects:?}"
    );
    click(&mut h, t("Apply and export PowerPoint"));
    assert!(
        h.state().dialogs.powerpoint_export,
        "presentation export did not open: {:?}",
        h.state().toasts.texts().collect::<Vec<_>>(),
    );
    let options_rect = ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("powerpoint_export")))
        .expect("PowerPoint options bounds");
    let export = h
        .query_all(By::new().role(Role::Button).label(t("Export")))
        .find(|node| {
            !node.accesskit_node().is_disabled() && options_rect.contains_rect(node.rect())
        })
        .expect("PowerPoint options export");
    let button_rect = export.rect();
    let options_layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("powerpoint_export"),
    );
    let top_modal = ctx.memory(|memory| memory.top_modal_layer());
    let layer_interactive = ctx.memory(|memory| memory.allows_interaction(options_layer));
    let button_layer = ctx.layer_id_at(button_rect.center());
    let hit_rects: Vec<_> = ctx
        .interactive_rects_last_pass()
        .into_iter()
        .filter(|rect| rect.intersects(button_rect))
        .collect();
    export.click();
    h.run_steps(3);
    assert!(
        !h.state().dialogs.powerpoint_export,
        "PowerPoint options export did not activate: button {button_rect:?}, \
         options {options_rect:?}, hit rects {hit_rects:?}, top modal {top_modal:?}, \
         button layer {button_layer:?}, options interactive {layer_interactive}, toasts {:?}",
        h.state().toasts.texts().collect::<Vec<_>>(),
    );
    has_label(
        &h,
        &format!(
            "1 — {}",
            t("Viewport slides flattened to non-editable images")
        ),
    );
    click(&mut h, t("Cancel"));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn maximized_powerpoint_options_still_warn_before_export() {
    let _guard = lock_ui();
    let mut sheet = Sheet::new("Presentation", "Root");
    sheet
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet);
    let mut h = harness(app(book.clone(), Language::English));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditPresentation);
    h.run_steps(4);
    click(&mut h, t("Apply and export PowerPoint"));
    assert!(h.state().dialogs.powerpoint_export);

    let options_id = egui::Id::unique("powerpoint_export");
    let options_rect = ctx
        .memory(|memory| memory.area_rect(options_id))
        .expect("PowerPoint options bounds");
    h.query_all(By::new().role(Role::Button).label(t("Maximize")))
        .find(|node| options_rect.contains_rect(node.rect()))
        .expect("PowerPoint options maximize")
        .click();
    h.run_steps(3);
    let options_rect = ctx
        .memory(|memory| memory.area_rect(options_id))
        .expect("maximized PowerPoint options bounds");
    h.query_all(By::new().role(Role::Button).label(t("Export")))
        .find(|node| {
            !node.accesskit_node().is_disabled() && options_rect.contains_rect(node.rect())
        })
        .expect("PowerPoint options export")
        .click();
    h.run_steps(3);
    assert!(!h.state().dialogs.powerpoint_export);
    has_label(
        &h,
        &format!(
            "1 — {}",
            t("Viewport slides flattened to non-editable images")
        ),
    );
    click(&mut h, t("Cancel"));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn fim_menu_and_sidebar_both_check_board_content() {
    let _guard = lock_ui();
    let source = br#"<?xml version="1.0" encoding="UTF-8"?>
<x:xdatenfelder.stammdatenschema.0102 xmlns:x="urn:xoev-de:fim:standard:xdatenfelder_3.0.0">
<x:stammdatenschema><x:identifikation><x:id>S00000000001</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Schema</x:name>
<x:struktur><x:anzahl>0:*</x:anzahl><x:enthaelt><x:datenfeld><x:identifikation><x:id>F00000000001</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Field</x:name></x:datenfeld></x:enthaelt></x:struktur>
</x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>"#;
    let mut book = thinoda_io::fim::import_document(source).unwrap();
    book.active_mut()
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(20.0, 20.0)),
        ))
        .unwrap();
    let sheet = book.active_mut();
    let root = sheet.root;
    let other = *sheet.topics.keys().find(|id| **id != root).unwrap();
    sheet.add_relationship(root, other).unwrap();
    sheet.add_boundary(&[root]).unwrap();
    book.validate().unwrap();
    let mut app = app(book.clone(), Language::English);
    app.format_tab = FormatTab::Fim;
    app.settings.show_format_panel = true;
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Export(Format::FimXdf3));
    h.run_steps(3);
    has_label(&h, &format!("1 — {}", t("Shapes omitted")));
    has_label(&h, &format!("1 — {}", t("Relationships omitted")));
    has_label(&h, &format!("1 — {}", t("Boundaries omitted")));
    click(&mut h, t("Cancel"));
    click(&mut h, t("Export validated XDF"));
    has_label(&h, &format!("1 — {}", t("Shapes omitted")));
    has_label(&h, &format!("1 — {}", t("Relationships omitted")));
    has_label(&h, &format!("1 — {}", t("Boundaries omitted")));
    click(&mut h, t("Cancel"));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn presentation_pdf_and_slide_images_offer_cancelable_preflight() {
    let _guard = lock_ui();
    let book = board();
    let mut h = harness(app(book.clone(), Language::English));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditPresentation);
    h.run_steps(4);
    for label in ["Export PDF", "Export Slide Images"] {
        h.query_all(By::new().role(Role::Button).label(t(label)))
            .find(|node| !node.accesskit_node().is_disabled())
            .unwrap()
            .scroll_to_me();
        h.run_steps(3);
        click(&mut h, t(label));
        assert!(
            !lacks_label(&h, t("Board export changes")),
            "{label}: preflight missing; toasts: {:?}",
            h.state().toasts.texts().collect::<Vec<_>>()
        );
        has_label(&h, t("Board grid and snapping settings omitted"));
        assert!(lacks_label(
            &h,
            &format!("1 — {}", t("Zone clipping not fully applied"))
        ));
        click(&mut h, t("Cancel"));
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
    }
}

#[test]
fn print_and_print_pdf_check_board_before_starting_output() {
    let _guard = lock_ui();
    let book = board();
    let mut h = harness(app(book.clone(), Language::English));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Print);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        h.run_steps(1);
        let ready = h
            .query_all(By::new().role(Role::Button).label(t("Save print PDF…")))
            .any(|node| !node.accesskit_node().is_disabled());
        if ready {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "print preview timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    for label in ["Save print PDF…", "Continue to print…"] {
        click(&mut h, t(label));
        has_label(&h, t("Board export changes"));
        has_label(&h, t("Board grid and snapping settings omitted"));
        assert!(lacks_label(
            &h,
            &format!("1 — {}", t("Zone clipping not fully applied"))
        ));
        click(&mut h, t("Cancel"));
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
    }
}

#[test]
fn cli_requires_loss_consent_and_preserves_existing_destination() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("board.thinoda");
    let output = directory.path().join("board.md");
    let mut book = board();
    book.active_mut().title = "TEST_PRIVATE_SHEET_TITLE".into();
    let source = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    std::fs::write(&input, &source).unwrap();
    std::fs::write(&output, b"existing").unwrap();
    let refused = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
        .arg("--convert")
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let error = String::from_utf8_lossy(&refused.stderr);
    assert!(error.contains("--allow-lossy"), "{error}");
    assert!(
        Lang::ALL
            .iter()
            .any(|&lang| error.contains(t_in(lang, "Shapes omitted"))),
        "{error}"
    );
    assert!(error.contains("TEST_PRIVATE_SHEET_TITLE"), "{error}");
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");
    let accepted = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
        .arg("--convert")
        .arg(&input)
        .arg(&output)
        .arg("--allow-lossy")
        .output()
        .unwrap();
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let log = String::from_utf8_lossy(&accepted.stderr);
    assert!(
        Lang::ALL
            .iter()
            .any(|&lang| log.contains(t_in(lang, "Shapes omitted"))),
        "{log}"
    );
    assert!(!log.contains("TEST_PRIVATE_SHEET_TITLE"), "{log}");
    assert_ne!(std::fs::read(&output).unwrap(), b"existing");
    assert_eq!(std::fs::read(&input).unwrap(), source);
}

#[test]
fn cli_editable_image_exports_disclose_embedded_data_and_keep_only_the_active_sheet() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("board.thinoda");
    let mut first = Sheet::new("Not exported", "");
    first.root_mode = SheetRootMode::Hidden;
    first.board.grid = thinoda_core::GridKind::Lines;
    first.board.snap = false;
    let mut second = first.clone();
    second.title = "Selected".into();
    second
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(80.0, 50.0)),
        ))
        .unwrap();
    let mut book = Workbook::new(first);
    book.sheets.push(second);
    book.active_sheet = 1;
    let source = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    std::fs::write(&input, &source).unwrap();
    for (name, format) in [
        ("board.drawio.svg", Format::DrawIoSvg),
        ("board.drawio.png", Format::DrawIoPng),
    ] {
        let output = directory.path().join(name);
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
            .arg("--convert")
            .arg(&input)
            .arg(&output)
            .env("RUST_LOG", "warn")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "{format:?}: {stderr}");
        assert!(
            Lang::ALL.iter().any(|&lang| stderr.contains(t_in(
                lang,
                "This image contains the editable diagram, including content outside the visible preview. It is not redacted."
            ))),
            "{format:?}: {stderr}"
        );
        assert!(
            Lang::ALL.iter().any(|&lang| stderr.contains(t_in(
                lang,
                "Only the current sheet is included; other sheets are not exported."
            ))),
            "{format:?}: {stderr}"
        );
        let image = std::fs::read(&output).unwrap();
        let restored = import_bytes(format, &image).unwrap();
        assert_eq!(restored.sheets.len(), 1);
        assert_eq!(restored.active().title, "Selected");
        assert_eq!(std::fs::read(&input).unwrap(), source);
    }
}

#[test]
fn cli_annotation_only_outline_requires_explicit_consent() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("annotated.thinoda");
    let output = directory.path().join("annotated.md");
    let source = export_bytes(
        &annotated_central_map(),
        Format::Thinoda,
        &ExportOptions::default(),
    )
    .unwrap();
    std::fs::write(&input, &source).unwrap();
    std::fs::write(&output, b"existing").unwrap();

    let refused = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
        .arg("--convert")
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let error = String::from_utf8_lossy(&refused.stderr);
    for (count, label) in [
        (2, "Summary topic trees omitted"),
        (1, "Relationships omitted"),
        (1, "Boundaries omitted"),
    ] {
        assert!(
            Lang::ALL
                .iter()
                .any(|&lang| error.contains(&format!("{count} — {}", t_in(lang, label)))),
            "missing {label}: {error}"
        );
    }
    assert!(error.contains("--allow-lossy"), "{error}");
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");

    let accepted = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
        .arg("--convert")
        .arg(&input)
        .arg(&output)
        .arg("--allow-lossy")
        .output()
        .unwrap();
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let converted = std::fs::read_to_string(&output).unwrap();
    assert!(converted.contains("First"));
    assert!(!converted.contains("Summary"));
    assert_eq!(std::fs::read(&input).unwrap(), source);
}

#[test]
fn cli_board_flag_does_not_replace_symbol_or_note_consent() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("board.thinoda");
    let output = directory.path().join("board.opml");
    let mut sheet = Sheet::new("Notes", "Root");
    let root = sheet.root;
    sheet
        .set_markdown_notes(root, Some("**bold**".into()))
        .unwrap();
    let topic = sheet.topic_mut(root).unwrap();
    topic.markers.push("priority-1".into());
    topic.marker_colors.insert(0, Color::BLACK);
    sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(20.0, 20.0)),
        ))
        .unwrap();
    std::fs::write(
        &input,
        export_bytes(
            &Workbook::new(sheet),
            Format::Thinoda,
            &ExportOptions::default(),
        )
        .unwrap(),
    )
    .unwrap();
    for (flags, accepted) in [
        (vec!["--allow-lossy"], false),
        (
            vec!["--allow-symbol-color-loss", "--allow-note-format-loss"],
            false,
        ),
        (
            vec![
                "--allow-lossy",
                "--allow-symbol-color-loss",
                "--allow-note-format-loss",
            ],
            true,
        ),
    ] {
        std::fs::write(&output, b"existing").unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
            .arg("--convert")
            .arg(&input)
            .arg(&output)
            .args(flags)
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            accepted,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if !accepted {
            assert_eq!(std::fs::read(&output).unwrap(), b"existing");
        }
    }
}

#[test]
fn cli_requires_consent_for_rasterized_powerpoint_viewports() {
    let _guard = lock_ui();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("presentation.thinoda");
    let output = directory.path().join("presentation.pptx");
    let mut sheet = Sheet::new("Presentation", "Root");
    sheet
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
        )
        .unwrap();
    std::fs::write(
        &input,
        export_bytes(
            &Workbook::new(sheet),
            Format::Thinoda,
            &ExportOptions::default(),
        )
        .unwrap(),
    )
    .unwrap();
    std::fs::write(&output, b"existing").unwrap();
    let refused = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
        .arg("--convert")
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let error = String::from_utf8_lossy(&refused.stderr);
    assert!(
        Lang::ALL.iter().any(|&lang| error.contains(t_in(
            lang,
            "Viewport slides flattened to non-editable images"
        ))),
        "{error}"
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");
    let accepted = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
        .arg("--convert")
        .arg(&input)
        .arg(&output)
        .arg("--allow-lossy")
        .output()
        .unwrap();
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert_ne!(std::fs::read(&output).unwrap(), b"existing");
}

#[test]
fn language_resolution_exposes_the_actual_catalogs() {
    let _guard = lock_ui();
    assert_eq!(i18n::resolve(Language::PortugueseBrazil), Lang::PtBr);
    assert_eq!(Lang::ALL.len(), 10);
}
