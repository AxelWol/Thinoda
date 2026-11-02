use super::*;
use crate::app::Cmd;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{
    CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, Endpoint, ShapeKind, Vec2,
    Workbook,
};

pub(super) fn setup(saved: bool) -> ThinodaApp {
    let mut sheet = Sheet::new("Presentation", "Center");
    sheet.insert_child(sheet.root, "First branch").unwrap();
    sheet.insert_child(sheet.root, "Second branch").unwrap();
    let floating = sheet.add_floating("Floating", Vec2::new(600.0, 200.0));
    sheet
        .create_zone(
            "Zone",
            vec![floating],
            Rect::from_min_size(Vec2::new(450.0, 50.0), Vec2::new(300.0, 300.0)),
        )
        .unwrap();
    if saved {
        sheet.presentation =
            thinoda_io::generate_presentation(&sheet, &ExportOptions::default()).unwrap();
    }
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    doc.doc.replace(Workbook::new(sheet));
    doc.doc.mark_saved();
    doc.view.reset();
    doc.view.select_only(doc.doc.sheet().root);
    app
}

pub(super) fn label_harness(
    connector: bool,
    text: &str,
) -> (Harness<'static, ThinodaApp>, CanvasObjectId) {
    let mut app = setup(true);
    let object = if connector {
        CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(-100.0, -180.0)),
            Endpoint::Point(Vec2::new(100.0, -180.0)),
        ))
    } else {
        CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(-80.0, -220.0), Vec2::new(160.0, 80.0)),
        )
    };
    let doc = app.doc_mut().unwrap();
    let id = doc.doc.add_canvas_object(object).unwrap();
    doc.doc
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            Rect::from_min_size(Vec2::new(-1000.0, -1000.0), Vec2::new(2000.0, 2000.0)),
        )
        .unwrap();
    doc.doc.mark_saved();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 850.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    let doc = h.state_mut().doc_mut().unwrap();
    doc.view.select_board_object(id);
    assert!(doc.view.begin_board_label(&doc.doc, id));
    h.run_steps(2);
    h.get_by_label(t("Board object label")).type_text(text);
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().view.board_label_draft().unwrap().2,
        text
    );
    (h, id)
}

fn change_label_outside_draft(app: &mut ThinodaApp, id: CanvasObjectId) {
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Concurrent label change", id, |object| {
            match &mut object.kind {
                CanvasObjectKind::Shape { label, .. } => *label = "Changed elsewhere".into(),
                CanvasObjectKind::Connector(connector) => {
                    connector.label = "Changed elsewhere".into()
                }
                CanvasObjectKind::Ink(_) => unreachable!(),
            }
        })
        .unwrap();
}

fn object_label_in_svg(svg: &str, id: CanvasObjectId) -> String {
    let marker = format!("<g class=\"canvas-object\" id=\"object-{}\">", id.0);
    let object = svg
        .split_once(&marker)
        .expect("board object is rendered")
        .1
        .split_once("</g>")
        .expect("board object group is closed")
        .0;
    object
        .split("<text ")
        .skip(1)
        .map(|text| {
            text.split_once('>')
                .expect("text element is opened")
                .1
                .split_once("</text>")
                .expect("text element is closed")
                .0
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn pitch_entry_commits_board_label_drafts_before_snapshotting() {
    for connector in [false, true] {
        for player in [false, true] {
            let (mut h, id) = label_harness(connector, "Pending board label");
            let ctx = h.ctx.clone();
            let previous_revision = h.state().doc().unwrap().doc.state_id();
            if player {
                h.state_mut().open_pitch(&ctx, false);
            } else {
                h.state_mut().begin_presentation_editor(&ctx);
            }
            let app = h.state();
            let doc = app.doc().unwrap();
            assert!(doc.doc.state_id() != previous_revision);
            assert_eq!(doc.view.editing_board_object(), None);
            assert_eq!(
                doc.doc.sheet().canvas.object(id).unwrap().label(),
                "Pending board label"
            );
            let (sheet, revision) = if player {
                let player = app.pitch.player.as_ref().unwrap();
                (player.sheet.as_ref(), player.owner.revision)
            } else {
                let editor = app.pitch.editor.as_ref().unwrap();
                (&editor.sheet, editor.owner.revision)
            };
            assert_eq!(revision, doc.doc.state_id());
            assert_eq!(
                sheet.canvas.object(id).unwrap().label(),
                "Pending board label"
            );
            let svg = thinoda_io::render_presentation_slide_svg(
                sheet,
                sheet.presentation.slides.len() - 1,
                &ExportOptions::default(),
            )
            .unwrap();
            assert_eq!(
                object_label_in_svg(&svg, id),
                "Pending board label",
                "connector={connector}, player={player}"
            );
        }
    }
}

#[test]
fn pitch_entry_rejects_stale_board_labels_without_discarding_the_draft() {
    for player in [false, true] {
        let (mut h, id) = label_harness(false, "Unsaved draft");
        change_label_outside_draft(h.state_mut(), id);
        let ctx = h.ctx.clone();
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let revision = h.state().doc().unwrap().doc.state_id();
        if player {
            h.state_mut().open_pitch(&ctx, false);
        } else {
            h.state_mut().begin_presentation_editor(&ctx);
        }
        let app = h.state();
        assert!(app.pitch.editor.is_none());
        assert!(app.pitch.player.is_none());
        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.doc.state_id(), revision);
        assert_eq!(
            doc.view.board_label_draft(),
            Some((id, "", "Unsaved draft"))
        );
    }
}

#[test]
fn presentation_editor_rejects_a_new_pending_board_label() {
    let (mut h, id) = label_harness(false, "Saved label");
    let ctx = h.ctx.clone();
    h.state_mut().begin_presentation_editor(&ctx);
    let editor = h.state_mut().pitch.editor.take().unwrap();
    let doc = h.state_mut().doc_mut().unwrap();
    assert!(doc.view.begin_board_label(&doc.doc, id));
    assert!(
        h.state()
            .presentation_snapshot_has_pending_edit(&editor.owner)
    );
    assert!(h.state_mut().apply_presentation_draft(&editor).is_err());
    assert_eq!(
        h.state().doc().unwrap().view.editing_board_object(),
        Some(id)
    );
}

#[test]
fn player_export_refreshes_a_pending_label_and_retains_invalid_drafts() {
    let (mut h, id) = label_harness(true, "Original label");
    let ctx = h.ctx.clone();
    h.state_mut().open_pitch(&ctx, false);
    let mut player = h.state_mut().pitch.player.take().unwrap();
    let doc = h.state_mut().doc_mut().unwrap();
    assert!(doc.view.begin_board_label(&doc.doc, id));
    h.run_steps(2);
    h.get_by_label(t("Board object label"))
        .type_text("Updated label");
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().view.board_label_draft().unwrap().2,
        "Updated label"
    );
    h.state_mut()
        .prepare_pitch_player_export(&ctx, &mut player)
        .unwrap();
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.view.editing_board_object(), None);
    assert_eq!(
        player.sheet.canvas.object(id).unwrap().label(),
        "Updated label"
    );
    assert_eq!(player.owner.revision, doc.doc.state_id());
    let svg = thinoda_io::render_presentation_slide_svg(
        &player.sheet,
        player.sheet.presentation.slides.len() - 1,
        &ExportOptions::default(),
    )
    .unwrap();
    assert_eq!(object_label_in_svg(&svg, id), "Updated label");

    let doc = h.state_mut().doc_mut().unwrap();
    assert!(doc.view.begin_board_label(&doc.doc, id));
    h.run_steps(2);
    h.get_by_label(t("Board object label"))
        .type_text("Retained draft");
    h.run_steps(2);
    change_label_outside_draft(h.state_mut(), id);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    assert!(
        h.state_mut()
            .prepare_pitch_player_export(&ctx, &mut player)
            .is_err()
    );
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.workbook(), &before);
    assert_eq!(
        doc.view.board_label_draft(),
        Some((id, "Updated label", "Retained draft"))
    );
}

#[test]
fn player_export_leaves_an_unrelated_sheet_label_draft_untouched() {
    let (mut h, _) = label_harness(false, "Player sheet label");
    let ctx = h.ctx.clone();
    h.state_mut().open_pitch(&ctx, false);
    let mut player = h.state_mut().pitch.player.take().unwrap();
    let mut other = Sheet::new("Other sheet", "Other root");
    let id = other
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(-80.0, -220.0), Vec2::new(160.0, 80.0)),
        ))
        .unwrap();
    let doc = h.state_mut().doc_mut().unwrap();
    doc.doc
        .edit_book("Add sheet", |book| {
            book.sheets.push(other);
            Ok(())
        })
        .unwrap();
    doc.switch_sheet(1).unwrap();
    doc.view.select_board_object(id);
    assert!(doc.view.begin_board_label(&doc.doc, id));
    h.run_steps(2);
    h.get_by_label(t("Board object label"))
        .type_text("Other sheet draft");
    h.run_steps(2);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let revision = h.state().doc().unwrap().doc.state_id();
    h.state_mut()
        .prepare_pitch_player_export(&ctx, &mut player)
        .unwrap();
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.workbook(), &before);
    assert_eq!(doc.doc.state_id(), revision);
    assert_eq!(
        doc.view.board_label_draft(),
        Some((id, "", "Other sheet draft"))
    );
    assert_eq!(player.owner.sheet, 0);
}

#[test]
fn pitch_does_not_silently_hide_embedded_resources() {
    let mut app = setup(true);
    let doc = app.doc_mut().unwrap();
    doc.doc
        .edit_sheet("Embed", |sheet| {
            let reference = sheet
                .resources
                .insert(
                    thinoda_core::resources::Resource::new("evidence.png", "image/png", vec![1])
                        .unwrap(),
                )
                .map_err(thinoda_core::edit::EditError::Invalid)?;
            sheet.topic_mut(sheet.root).unwrap().image = Some(reference);
            Ok(())
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let state = app.doc().unwrap().doc.state_id();
    app.open_pitch(&Context::default(), false);
    assert!(app.pitch.player.is_none());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    assert_eq!(app.doc().unwrap().doc.state_id(), state);
}

#[test]
fn pitch_accepts_valid_local_images_without_changing_source_or_history() {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        40,
        20,
        image::Rgb([250, 10, 20]),
    ))
    .write_to(&mut bytes, image::ImageFormat::Png)
    .unwrap();
    let resource =
        thinoda_core::resources::Resource::new("evidence.png", "image/png", bytes.into_inner())
            .unwrap();
    let mut app = setup(true);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Image", |sheet| {
            thinoda_core::images::set_image(
                sheet,
                sheet.root,
                Some((resource, Vec2::new(160.0, 80.0))),
            )
            .map_err(thinoda_core::edit::EditError::Invalid)
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let state = app.doc().unwrap().doc.state_id();
    app.open_pitch(&Context::default(), false);
    assert!(app.pitch.player.is_some());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    assert_eq!(app.doc().unwrap().doc.state_id(), state);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn pitch_player_preflight_is_visible_and_cancels_when_player_closes_or_stales() {
    let mut app = setup(true);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Board setting", |sheet| {
            sheet.board.snap = !sheet.board.snap;
            Ok(())
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    ctx.set_embed_viewports(true);
    h.state_mut().exec(&ctx, Cmd::Pitch);
    h.run_steps(4);
    let request = |h: &mut Harness<'_, ThinodaApp>| {
        h.query_all_by_role_and_label(egui::accesskit::Role::Button, t("Export"))
            .last()
            .expect("player export control")
            .click();
        h.run_steps(2);
        h.get_by_label(t("Export PDF")).click();
        h.run_steps(3);
        assert!(h.state().dialogs.board_export.is_some());
        h.get_by_role_and_label(egui::accesskit::Role::Window, t("Board export changes"));
    };
    request(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, t("Cancel"))
        .click();
    h.run_steps(3);
    assert!(h.state().dialogs.board_export.is_none());
    assert!(h.state().pitch.player.is_some());
    assert!(h.state().pitch.output.is_none());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);

    request(&mut h);
    let close_ctx = Context::default();
    close_ctx.set_embed_viewports(true);
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let mut output = close_ctx.run_ui(input, |ui| h.state_mut().pitch_window(ui.ctx()));
    output.textures_delta.clear();
    assert!(h.state().pitch.player.is_none());
    assert!(h.state().dialogs.board_export.is_none());
    assert!(h.state().pitch.output.is_none());

    h.state_mut().open_pitch(&ctx, false);
    h.run_steps(3);
    request(&mut h);
    h.state_mut().pitch.player.as_mut().unwrap().owner.root = TopicId(u64::MAX);
    h.run_steps(3);
    assert!(h.state().pitch.player.is_none());
    assert!(h.state().dialogs.board_export.is_none());
    assert!(h.state().pitch.output.is_none());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn presentation_generation_frames_and_order_apply_as_one_undo_without_changing_topics() {
    let mut app = setup(false);
    let before = app.doc().unwrap().doc.workbook().clone();
    let ctx = Context::default();
    app.begin_presentation_editor(&ctx);
    let mut editor = app.pitch.editor.take().unwrap();
    editor.sheet.presentation =
        thinoda_io::generate_presentation(&editor.sheet, &ExportOptions::default()).unwrap();
    assert_eq!(editor.sheet.presentation.slides.len(), 3);
    editor.sheet.presentation.slides.swap(0, 2);
    editor.sheet.presentation.slides[0].title = "Opening".into();
    editor.sheet.presentation.slides[0].frame.min = Vec2::new(400.0, 30.0);
    editor.sheet.presentation.slides[0].frame.size = Vec2::new(400.0, 350.0);
    app.apply_presentation_draft(&editor).unwrap();
    let doc = app.doc_mut().unwrap();
    assert_eq!(doc.doc.sheet().presentation, editor.sheet.presentation);
    let mut no_slides = doc.doc.workbook().clone();
    no_slides.sheets[0].presentation = Default::default();
    assert_eq!(no_slides, before);
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
    assert!(!doc.doc.undo());
}

#[test]
fn invalid_and_stale_presentation_drafts_cannot_partially_apply() {
    let mut app = setup(true);
    let ctx = Context::default();
    app.begin_presentation_editor(&ctx);
    let mut editor = app.pitch.editor.take().unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    editor.sheet.presentation.slides[0].title = "Must not apply".into();
    editor.sheet.presentation.slides[0].frame.size.x = 0.0;
    assert!(app.apply_presentation_draft(&editor).is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.begin_presentation_editor(&ctx);
    let editor = app.pitch.editor.take().unwrap();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Changed", |sheet| sheet.set_title(root, "Changed"))
        .unwrap();
    let before = doc.doc.workbook().clone();
    assert!(app.apply_presentation_draft(&editor).is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn presentation_order_controls_commit_only_on_apply() {
    let app = setup(true);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let first = before.sheets[0].presentation.slides[0].id;
    h.state_mut().exec(&ctx, Cmd::EditPresentation);
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Move Down")
        .click();
    h.run_steps(2);
    assert_eq!(
        h.state()
            .pitch
            .editor
            .as_ref()
            .unwrap()
            .sheet
            .presentation
            .slides[1]
            .id,
        first
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Apply")
        .click();
    h.run_steps(3);
    assert!(h.state().pitch.editor.is_none());
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().presentation.slides[1].id,
        first
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(!h.state_mut().doc_mut().unwrap().doc.undo());
}

#[test]
fn small_presentation_dialog_is_read_only_until_apply_and_cancel_is_reachable() {
    let app = setup(false);
    let mut h = Harness::builder()
        .with_size(egui::vec2(360.0, 300.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.state_mut().exec(&ctx, Cmd::EditPresentation);
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Generate Slides")
        .click();
    h.run_steps(3);
    assert_eq!(
        h.state()
            .pitch
            .editor
            .as_ref()
            .unwrap()
            .sheet
            .presentation
            .slides
            .len(),
        3
    );
    h.ctx
        .memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
    h.key_press(egui::Key::Tab);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    let cancel = h.get_by_role_and_label(egui::accesskit::Role::Button, "Cancel");
    assert!(h.ctx.content_rect().contains_rect(cancel.rect()));
    cancel.click();
    h.run_steps(3);
    assert!(h.state().pitch.editor.is_none());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn presentation_ime_candidate_keys_do_not_apply_or_discard_the_draft() {
    for key in [egui::Key::Enter, egui::Key::Tab, egui::Key::Escape] {
        let app = setup(false);
        let mut h = Harness::builder()
            .with_size(egui::vec2(800.0, 650.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        let ctx = h.ctx.clone();
        let before = h.state().doc().unwrap().doc.workbook().clone();
        h.state_mut().exec(&ctx, Cmd::EditPresentation);
        h.run_steps(3);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "slide".into(),
            active_range_chars: None,
        }));
        h.key_press(key);
        h.run_steps(3);
        assert!(h.state().pitch.editor.is_some(), "{key:?}");
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}

struct Playing {
    app: ThinodaApp,
    player: PitchPlayer,
    open: bool,
}

#[test]
fn player_export_button_commits_or_preserves_a_board_label_draft() {
    for invalid in [false, true] {
        let (mut map, id) = label_harness(false, "First label");
        let ctx = map.ctx.clone();
        map.state_mut().open_pitch(&ctx, false);
        let player = map.state_mut().pitch.player.take().unwrap();
        let doc = map.state_mut().doc_mut().unwrap();
        doc.doc
            .edit_sheet("Board setting", |sheet| {
                sheet.board.snap = !sheet.board.snap;
                Ok(())
            })
            .unwrap();
        assert!(doc.view.begin_board_label(&doc.doc, id));
        map.run_steps(2);
        map.get_by_label(t("Board object label"))
            .type_text("Player draft");
        map.run_steps(2);
        if invalid {
            change_label_outside_draft(map.state_mut(), id);
        }
        let app = std::mem::replace(map.state_mut(), ThinodaApp::headless());
        let mut h = Harness::builder()
            .with_size(egui::vec2(1100.0, 700.0))
            .build_ui_state(
                |ui, state: &mut Playing| {
                    if state.open {
                        state.open = state.app.pitch_player_ui(ui, &mut state.player);
                    }
                },
                Playing {
                    app,
                    player,
                    open: true,
                },
            );
        h.run_steps(3);
        h.get_by_label(t("Export")).click();
        h.run_steps(2);
        h.get_by_label(t("Export PDF")).click();
        h.run_steps(2);
        let state = h.state();
        let doc = state.app.doc().unwrap();
        if invalid {
            assert!(state.app.dialogs.board_export.is_none());
            assert!(state.app.pitch.output.is_none());
            assert_eq!(
                doc.view.board_label_draft(),
                Some((id, "First label", "Player draft"))
            );
            assert_eq!(
                doc.doc.sheet().canvas.object(id).unwrap().label(),
                "Changed elsewhere"
            );
            assert!(
                state
                    .app
                    .toasts
                    .texts()
                    .any(|text| text.contains(t("Export failed")))
            );
        } else {
            assert!(state.app.dialogs.board_export.is_some());
            assert_eq!(doc.view.editing_board_object(), None);
            assert_eq!(
                state.player.sheet.canvas.object(id).unwrap().label(),
                "Player draft"
            );
            assert_eq!(state.player.owner.revision, doc.doc.state_id());
        }
    }
}

#[test]
fn powerpoint_editor_action_applies_one_draft_and_opens_the_existing_export_flow() {
    let app = setup(true);
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditPresentation);
    h.run_steps(3);
    h.state_mut()
        .pitch
        .editor
        .as_mut()
        .unwrap()
        .sheet
        .presentation
        .slides[0]
        .title = "Export this draft".into();
    h.run_steps(3);
    let export = h.get_by_label("Apply and export PowerPoint");
    assert!(
        h.ctx.content_rect().contains_rect(export.rect()),
        "{:?} {:?}",
        export.rect(),
        h.ctx.content_rect()
    );
    export.click();
    h.run_steps(3);
    assert!(
        h.state().pitch.editor.is_none(),
        "{:?}",
        h.state()
            .pitch
            .editor
            .as_ref()
            .and_then(|editor| editor.error.as_ref())
    );
    assert!(h.state().dialogs.powerpoint_export);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().presentation.slides[0].title,
        "Export this draft"
    );
    let doc = h.state_mut().doc_mut().unwrap();
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
    assert!(!doc.doc.undo());
}

#[test]
fn viewport_editor_footer_actions_are_clickable_at_wide_and_narrow_sizes() {
    for size in [
        egui::vec2(1280.0, 1000.0),
        egui::vec2(800.0, 650.0),
        egui::vec2(400.0, 350.0),
        egui::vec2(320.0, 260.0),
    ] {
        let mut sheet = Sheet::new("Presentation", "Root");
        sheet
            .add_presentation_slide(
                "View",
                PresentationSource::Viewport,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
            )
            .unwrap();
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        doc.doc.replace(Workbook::new(sheet));
        doc.doc.mark_saved();
        doc.view.reset();
        let mut h = Harness::builder()
            .with_size(size)
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::EditPresentation);
        h.run_steps(4);
        for label in ["Apply", "Cancel", "Apply and export PowerPoint"] {
            let rect = h
                .get_by_role_and_label(egui::accesskit::Role::Button, t(label))
                .rect();
            assert!(
                ctx.content_rect().contains(rect.center()),
                "{label} is off-screen at {size:?}: {rect:?}"
            );
            assert!(
                ctx.interactive_rects_last_pass().iter().any(|hit| {
                    hit.contains(rect.center())
                        && hit.width() <= rect.width() + 1.0
                        && hit.height() <= rect.height() + 1.0
                }),
                "{label} is clipped at {size:?}: {rect:?}"
            );
        }
        h.get_by_role_and_label(
            egui::accesskit::Role::Button,
            t("Apply and export PowerPoint"),
        )
        .click();
        h.run_steps(3);
        assert!(
            h.state().dialogs.powerpoint_export,
            "PowerPoint action did not activate at {size:?}"
        );
    }
}

fn player_harness(width: f32, presenter: bool) -> Harness<'static, Playing> {
    let mut app = setup(true);
    app.open_pitch(&Context::default(), presenter);
    let player = app.pitch.player.take().unwrap();
    Harness::builder()
        .with_size(egui::vec2(width, 650.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, state: &mut Playing| {
                if state.open {
                    state.open = state.app.pitch_player_ui(ui, &mut state.player);
                }
            },
            Playing {
                app,
                player,
                open: true,
            },
        )
}

#[test]
fn playback_stage_fits_the_embedded_player_instead_of_the_host_viewport() {
    struct EmbeddedPlayer {
        playing: Playing,
        bounds: egui::Rect,
    }

    for (size, presenter) in [
        (egui::vec2(850.0, 420.0), false),
        (egui::vec2(850.0, 420.0), true),
        (egui::vec2(360.0, 420.0), false),
        (egui::vec2(360.0, 420.0), true),
    ] {
        let mut app = setup(true);
        app.open_pitch(&Context::default(), presenter);
        let player = app.pitch.player.take().unwrap();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1200.0, 1100.0))
            .build_ui_state(
                move |ui, state: &mut EmbeddedPlayer| {
                    ui.allocate_ui(size, |ui| {
                        state.bounds = ui.max_rect();
                        let playing = &mut state.playing;
                        if playing.open {
                            playing.open = playing.app.pitch_player_ui(ui, &mut playing.player);
                        }
                    });
                },
                EmbeddedPlayer {
                    playing: Playing {
                        app,
                        player,
                        open: true,
                    },
                    bounds: egui::Rect::NOTHING,
                },
            );
        h.run_steps(3);
        let stage = h.get_by_label("Slide preview").rect();
        let bounds = h.state().bounds;
        assert!(
            stage.min.x >= bounds.min.x - 1.0
                && stage.max.x <= bounds.max.x + 1.0
                && stage.min.y >= bounds.min.y - 1.0
                && stage.max.y <= bounds.max.y + 1.0,
            "stage {stage:?} overflows {bounds:?} at {size:?}, presenter={presenter}"
        );
    }
}

#[test]
fn playback_pointer_and_keyboard_navigation_preserve_document_and_exit_without_quitting() {
    for width in [360.0, 1100.0] {
        let mut h = player_harness(width, true);
        h.run_steps(3);
        let before = h.state().app.doc().unwrap().doc.workbook().clone();
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Next Slide")
            .click();
        h.run_steps(2);
        assert_eq!(h.state().player.selected, 1);
        h.key_press(egui::Key::ArrowRight);
        h.run_steps(2);
        assert_eq!(h.state().player.selected, 2);
        h.key_press(egui::Key::Space);
        h.run_steps(2);
        assert_eq!(h.state().player.selected, 2);
        h.key_press(egui::Key::ArrowLeft);
        h.run_steps(2);
        assert_eq!(h.state().player.selected, 1);
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(!h.state().open);
        assert_eq!(h.state().app.docs.len(), 1);
        assert_eq!(h.state().app.doc().unwrap().doc.workbook(), &before);
        assert!(!h.state().app.doc().unwrap().doc.is_dirty());
        assert!(
            !h.output().viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::Close)
        );
    }
}

#[test]
fn playback_first_last_and_build_controls_are_reachable_and_view_only() {
    for width in [360.0, 1100.0] {
        let mut h = player_harness(width, false);
        h.run_steps(3);
        let before = h.state().app.doc().unwrap().doc.workbook().clone();
        for label in ["Last Slide", "Reduce motion", "Build topics"] {
            assert!(
                h.ctx
                    .content_rect()
                    .contains_rect(h.get_by_label(label).rect())
            );
        }
        h.get_by_label("Last Slide").click();
        h.run_steps(2);
        assert_eq!(h.state().player.selected, 2);
        h.key_press(egui::Key::Home);
        h.run_steps(2);
        assert_eq!(h.state().player.selected, 0);
        h.get_by_label("Build topics").click();
        h.run_steps(2);
        assert!(h.state().player.playback.build_topics);
        assert_eq!(h.state().player.playback.step, 1);
        h.key_press(egui::Key::End);
        h.run_steps(2);
        assert_eq!(h.state().player.selected, 2);
        assert_eq!(h.state().player.playback.step, usize::MAX);
        h.get_by_label("First Slide").click();
        h.run_steps(2);
        assert_eq!(h.state().player.selected, 0);
        assert_eq!(h.state().player.playback.step, 1);
        assert_eq!(h.state().app.doc().unwrap().doc.workbook(), &before);
        assert!(!h.state().app.doc().unwrap().doc.is_dirty());
    }
}

#[test]
fn playback_popup_does_not_consume_slide_shortcuts() {
    let mut h = player_harness(360.0, false);
    h.run_steps(3);
    h.get_by_label("Transition").click();
    h.run_steps(2);
    assert!(egui::Popup::is_any_open(&h.ctx));
    h.key_press(egui::Key::End);
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(h.state().player.selected, 0);
    assert!(h.state().open);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().open);
}

#[test]
fn playback_controls_reject_canceled_and_multitouch_releases() {
    for label in ["Build topics", "Reduce motion", "Next Slide"] {
        for (contacts, cancel) in [(1, true), (2, false), (1, false)] {
            let mut h = player_harness(360.0, false);
            h.run_steps(3);
            let pos = h.get_by_label(label).rect().center();
            h.event(egui::Event::PointerMoved(pos));
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            for phase in [
                egui::TouchPhase::Start,
                if cancel {
                    egui::TouchPhase::Cancel
                } else {
                    egui::TouchPhase::End
                },
            ] {
                for id in 1..=contacts {
                    h.event(egui::Event::Touch {
                        device_id: egui::TouchDeviceId(1),
                        id: egui::TouchId(id),
                        phase,
                        pos: pos + egui::vec2((id - 1) as f32 * 40.0, 0.0),
                        force: None,
                    });
                }
                h.run_steps(2);
            }
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(4);
            let activated = contacts == 1 && !cancel;
            match label {
                "Build topics" => assert_eq!(h.state().player.playback.build_topics, activated),
                "Reduce motion" => assert_eq!(h.state().player.playback.reduce_motion, !activated),
                _ => assert_eq!(h.state().player.selected, usize::from(activated)),
            }
        }
    }
}

#[test]
fn native_pitch_close_and_fullscreen_commands_only_target_the_player() {
    let mut app = setup(true);
    let ctx = Context::default();
    app.open_pitch(&ctx, false);
    let viewport = egui::ViewportId::from_hash_of("thinoda-pitch-player");
    app.current_viewport = viewport;
    assert!(app.command_available(&Cmd::CloseWindow));
    assert!(!app.command_available(&Cmd::CloseTab(0)));
    assert!(!app.command_available(&Cmd::Canvas(thinoda_canvas::Action::Delete)));
    assert!(!app.command_available(&Cmd::Canvas(thinoda_canvas::Action::Undo)));
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut input = egui::RawInput {
        viewport_id: viewport,
        ..Default::default()
    };
    input
        .viewports
        .insert(viewport, egui::ViewportInfo::default());
    let mut output = ctx.run_ui(input, |_| app.exec(&ctx, Cmd::CloseWindow));
    output.textures_delta.clear();
    assert!(
        output.viewport_output[&viewport]
            .commands
            .contains(&egui::ViewportCommand::Close)
    );
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    let mut player = app.pitch.player.take().unwrap();
    let mut input = egui::RawInput {
        viewport_id: viewport,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100.0, 700.0),
        )),
        events: vec![egui::Event::Key {
            key: egui::Key::F11,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    input
        .viewports
        .insert(viewport, egui::ViewportInfo::default());
    let mut output = ctx.run_ui(input, |ui| {
        app.pitch_player_ui(ui, &mut player);
    });
    output.textures_delta.clear();
    assert!(player.fullscreen);
    assert!(
        output.viewport_output[&viewport]
            .commands
            .contains(&egui::ViewportCommand::Fullscreen(true))
    );
}

#[test]
fn presentation_outputs_use_the_captured_frames_without_changing_the_source() {
    let app = setup(true);
    let sheet = app.doc().unwrap().doc.sheet();
    let before = sheet.clone();
    assert!(
        ThinodaApp::pitch_bytes(sheet, &ExportOptions::default(), false)
            .unwrap()
            .starts_with(b"%PDF")
    );
    assert!(
        ThinodaApp::pitch_bytes(sheet, &ExportOptions::default(), true)
            .unwrap()
            .starts_with(b"PK")
    );
    assert_eq!(sheet, &before);
}

#[test]
fn playback_refreshes_map_changes_without_touching_document_or_history() {
    let mut app = setup(true);
    let ctx = Context::default();
    app.open_pitch(&ctx, false);
    let mut player = app.pitch.player.take().unwrap();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("External edit", |sheet| sheet.set_title(root, "Revised"))
        .unwrap();
    let before = doc.doc.workbook().clone();
    let revision = app.doc().unwrap().doc.state_id();
    assert!(app.refresh_pitch(&mut player).unwrap());
    assert_eq!(player.sheet.topic(root).unwrap().title, "Revised");
    assert_eq!(player.owner.revision, revision);
    assert!(!app.refresh_pitch(&mut player).unwrap());
    assert_eq!(app.docs.len(), 1);
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    assert_eq!(app.doc().unwrap().doc.state_id(), revision);
}

#[test]
fn playback_refresh_retains_slide_identity_and_handles_deleted_or_empty_decks() {
    let mut app = setup(true);
    app.open_pitch(&Context::default(), false);
    let mut player = app.pitch.player.take().unwrap();
    player.selected = 1;
    let id = player.sheet.presentation.slides[1].id;
    let doc = app.doc_mut().unwrap();
    let mut presentation = doc.doc.sheet().presentation.clone();
    presentation.slides.swap(0, 1);
    doc.doc.replace_presentation(presentation).unwrap();
    assert!(app.refresh_pitch(&mut player).unwrap());
    assert_eq!(player.selected, 0);
    assert_eq!(player.sheet.presentation.slides[player.selected].id, id);
    let doc = app.doc_mut().unwrap();
    let mut presentation = doc.doc.sheet().presentation.clone();
    presentation.slides.remove(0);
    doc.doc.replace_presentation(presentation).unwrap();
    assert!(app.refresh_pitch(&mut player).unwrap());
    assert_eq!(player.selected, 0);
    app.doc_mut()
        .unwrap()
        .doc
        .replace_presentation(Default::default())
        .unwrap();
    assert!(app.refresh_pitch(&mut player).unwrap());
    assert!(player.sheet.presentation.slides.is_empty());
    let mut h = Harness::builder()
        .with_size(egui::vec2(360.0, 650.0))
        .build_ui_state(
            |ui, state: &mut Playing| {
                if state.open {
                    state.open = state.app.pitch_player_ui(ui, &mut state.player);
                }
            },
            Playing {
                app,
                player,
                open: true,
            },
        );
    h.run_steps(3);
    assert!(h.state().open);
    h.get_by_label("No slides yet. Generate branch and Zone slides, or add the selected branch.");
    h.get_by_label("Exit Pitch").click();
    h.run_steps(2);
    assert!(!h.state().open);
}

#[test]
fn playback_ignores_unrelated_sheet_edits_and_never_switches_to_the_active_sheet() {
    let mut app = setup(true);
    app.open_pitch(&Context::default(), false);
    let mut player = app.pitch.player.take().unwrap();
    let before = Arc::clone(&player.sheet);
    let layout = Arc::clone(&player.layout);
    let doc = app.doc_mut().unwrap();
    doc.doc.add_sheet(Sheet::new("Other", "Unrelated")).unwrap();
    doc.doc.set_active(1);
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Other edit", |sheet| sheet.set_title(root, "Private edit"))
        .unwrap();
    assert!(!app.refresh_pitch(&mut player).unwrap());
    assert!(Arc::ptr_eq(&player.sheet, &before));
    assert!(Arc::ptr_eq(&player.layout, &layout));
    assert_eq!(player.owner.sheet, 0);
    // Refresh the captured sheet even while another sheet remains active.
    let doc = app.doc_mut().unwrap();
    doc.doc.set_active(0);
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Pitch edit", |sheet| sheet.set_title(root, "Live edit"))
        .unwrap();
    doc.doc.set_active(1);
    assert!(app.refresh_pitch(&mut player).unwrap());
    assert_eq!(
        player.sheet.topic(player.sheet.root).unwrap().title,
        "Live edit"
    );
    assert_eq!(app.doc().unwrap().doc.active_index(), 1);
}

#[test]
fn playback_does_not_substitute_a_different_sheet_when_original_is_replaced() {
    let mut app = setup(true);
    app.open_pitch(&Context::default(), false);
    let mut player = app.pitch.player.take().unwrap();
    let before = Arc::clone(&player.sheet);
    // Replacing with an unrelated root must never display that sheet under the old slide.
    let mut unrelated = Sheet::new("Unrelated", "Private");
    let new_root = unrelated
        .insert_child(unrelated.root, "Different root")
        .unwrap();
    unrelated.root = new_root;
    app.doc_mut().unwrap().doc.replace(Workbook::new(unrelated));
    assert!(app.refresh_pitch(&mut player).is_err());
    assert_eq!(*player.sheet, *before);
}

#[test]
fn child_presentation_draft_returns_to_root_with_its_unsaved_frame() {
    let mut app = setup(true);
    let ctx = Context::default();
    app.begin_presentation_editor(&ctx);
    let child = egui::ViewportId::from_hash_of("pitch-child");
    let editor = app.pitch.editor.as_mut().unwrap();
    editor.owner.viewport = child;
    editor.sheet.presentation.slides[0].frame.size = Vec2::new(900.0, 500.0);
    app.return_pitch_viewport(child);
    let editor = app.pitch.editor.take().unwrap();
    assert_eq!(editor.owner.viewport, egui::ViewportId::ROOT);
    app.apply_presentation_draft(&editor).unwrap();
    assert_eq!(
        app.doc().unwrap().doc.sheet().presentation.slides[0]
            .frame
            .size,
        Vec2::new(900.0, 500.0)
    );
}
