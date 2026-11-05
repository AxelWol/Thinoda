use super::*;
use crate::app::Cmd;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::Vec2;

fn setup() -> (ThinodaApp, Context, TopicId) {
    let mut app = ThinodaApp::headless();
    let ctx = Context::default();
    let doc = app.doc_mut().unwrap();
    let topic = doc
        .doc
        .edit_sheet("Floating", |sheet| {
            Ok(sheet.add_floating("Content", Vec2::new(400.0, 200.0)))
        })
        .unwrap();
    doc.view.select_only(topic);
    (app, ctx, topic)
}

fn draft(app: &mut ThinodaApp, ctx: &Context, id: Option<ZoneId>) -> ZoneEditor {
    app.begin_zone_editor(ctx, id);
    app.zones.editor.take().unwrap()
}

#[test]
fn zone_drafts_apply_frame_and_content_atomically_with_one_undo() {
    let (mut app, ctx, topic) = setup();
    let initial = app.doc().unwrap().doc.workbook().clone();
    let editor = draft(&mut app, &ctx, None);
    app.apply_zone_draft(&editor).unwrap();
    let doc = app.doc_mut().unwrap();
    let id = doc.doc.sheet().zones[0].id;
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &initial);
    assert!(doc.doc.redo());
    let before = doc.doc.workbook().clone();
    let mut editor = draft(&mut app, &ctx, Some(id));
    editor.name = "Renamed".into();
    editor.frame.min = editor.frame.min + Vec2::new(50.0, 75.0);
    editor.frame.size = editor.frame.size + Vec2::new(100.0, 80.0);
    app.apply_zone_draft(&editor).unwrap();
    let doc = app.doc_mut().unwrap();
    let zone = &doc.doc.sheet().zones[0];
    assert_eq!(zone.name, "Renamed");
    assert_eq!(zone.frame, editor.frame);
    assert_eq!(
        doc.doc.sheet().topic(topic).unwrap().position.unwrap(),
        before.sheets[0].topic(topic).unwrap().position.unwrap() + Vec2::new(50.0, 75.0)
    );
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &initial);
}

#[test]
fn zone_drafts_apply_independent_colors_as_one_undoable_change() {
    let (mut app, ctx, _) = setup();
    let initial = app.doc().unwrap().doc.workbook().clone();
    let mut editor = draft(&mut app, &ctx, None);
    editor.frame_color = Some(thinoda_core::Color::rgb(20, 80, 180));
    editor.background_color = Some(thinoda_core::Color::rgb(240, 220, 180));
    app.apply_zone_draft(&editor).unwrap();

    let doc = app.doc_mut().unwrap();
    let zone = &doc.doc.sheet().zones[0];
    assert_eq!(zone.color, editor.frame_color);
    assert_eq!(zone.background_color, editor.background_color);
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &initial);
    assert!(doc.doc.redo());
    assert_eq!(
        doc.doc.sheet().zones[0].background_color,
        editor.background_color
    );
}

#[test]
fn zone_editor_exposes_separate_frame_and_background_color_controls() {
    let (app, _, _) = setup();
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::CreateZone);
    h.run_steps(3);

    assert!(h.query_by_label("Frame color").is_some());
    assert!(h.query_by_label("Background color").is_some());
}

#[test]
fn invalid_and_stale_zone_drafts_never_partially_apply() {
    let (mut app, ctx, topic) = setup();
    let editor = draft(&mut app, &ctx, None);
    app.apply_zone_draft(&editor).unwrap();
    let id = app.doc().unwrap().doc.sheet().zones[0].id;
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut editor = draft(&mut app, &ctx, Some(id));
    editor.name = "Must not apply".into();
    editor.frame.min = editor.frame.min + Vec2::new(300.0, 400.0);
    editor.frame.size.x = 99.0;
    assert!(app.apply_zone_draft(&editor).is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    let editor = draft(&mut app, &ctx, Some(id));
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("External change", |sheet| {
            sheet.set_title(topic, "External change")
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(app.apply_zone_draft(&editor).is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn short_zone_dialog_cancel_discards_changes_and_candidate_keys_stay_in_ime() {
    for key in [egui::Key::Enter, egui::Key::Tab, egui::Key::Escape] {
        let (app, _, _) = setup();
        let mut h = Harness::builder()
            .with_size(egui::vec2(360.0, 300.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        let ctx = h.ctx.clone();
        let before = h.state().doc().unwrap().doc.workbook().clone();
        h.state_mut().exec(&ctx, Cmd::CreateZone);
        h.run_steps(3);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "zone".into(),
            active_range_chars: None,
        }));
        h.key_press(key);
        h.run_steps(2);
        assert!(h.state().zones.editing());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }));
        h.run_steps(2);
        let cancel = h.get_by_role_and_label(egui::accesskit::Role::Button, "Cancel");
        assert!(h.ctx.content_rect().contains_rect(cancel.rect()));
        cancel.click();
        h.run_steps(3);
        assert!(!h.state().zones.editing());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn child_zone_draft_returns_to_root_without_losing_edits() {
    let (mut app, _, _) = setup();
    let ctx = Context::default();
    let child = egui::ViewportId::from_hash_of("zone-child");
    let mut editor = draft(&mut app, &ctx, None);
    editor.owner.viewport = child;
    editor.name = "Kept on return".into();
    app.zones.editor = Some(editor);
    app.return_zone_viewport(child);
    let editor = app.zones.editor.take().unwrap();
    assert_eq!(editor.owner.viewport, egui::ViewportId::ROOT);
    app.apply_zone_draft(&editor).unwrap();
    assert_eq!(
        app.doc().unwrap().doc.sheet().zones[0].name,
        "Kept on return"
    );
}

#[test]
fn zone_draft_keeps_the_underlying_map_read_only() {
    let (app, _, _) = setup();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::CreateZone);
    h.run_steps(3);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.ctx
        .memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
    h.key_press(egui::Key::Tab);
    h.run_steps(3);
    assert!(h.state().zones.editing());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

fn add_shape(app: &mut ThinodaApp) -> CanvasObjectId {
    app.doc_mut()
        .unwrap()
        .doc
        .add_canvas_object(thinoda_core::CanvasObject::shape(
            thinoda_core::ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(700.0, 100.0), Vec2::new(120.0, 80.0)),
        ))
        .unwrap()
}

#[test]
fn mixed_zone_membership_transfers_without_moving_and_removes_with_one_undo() {
    let (mut app, ctx, topic) = setup();
    let shape = add_shape(&mut app);
    app.doc_mut().unwrap().view.board_selection = vec![shape];
    let editor = draft(&mut app, &ctx, None);
    app.apply_zone_draft(&editor).unwrap();
    let first = app.doc().unwrap().doc.sheet().zones[0].id;
    let second = app
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Target", |sheet| {
            sheet
                .create_board_frame("Target", editor.frame.translate(Vec2::new(500.0, 0.0)))
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut membership = app.zone_membership_draft(&ctx).unwrap();
    membership.target = Some(second);
    app.apply_zone_membership(&membership, true).unwrap();
    let doc = app.doc_mut().unwrap();
    let sheet = doc.doc.sheet();
    assert!(
        sheet
            .zones
            .iter()
            .find(|zone| zone.id == first)
            .unwrap()
            .roots
            .is_empty()
    );
    let target = sheet.zones.iter().find(|zone| zone.id == second).unwrap();
    assert_eq!(target.roots, vec![topic]);
    assert_eq!(target.objects, vec![shape]);
    assert_eq!(
        sheet.topic(topic).unwrap().position,
        before.sheets[0].topic(topic).unwrap().position
    );
    assert_eq!(
        sheet.canvas.object(shape),
        before.sheets[0].canvas.object(shape)
    );
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
    assert!(doc.doc.redo());
    let assigned = doc.doc.workbook().clone();
    assert!(app.command_available(&Cmd::RemoveFromZone));
    app.exec(&ctx, Cmd::RemoveFromZone);
    let doc = app.doc_mut().unwrap();
    assert_eq!(doc.doc.sheet().zones.len(), 2);
    assert!(
        doc.doc
            .sheet()
            .zones
            .iter()
            .all(|zone| zone.roots.is_empty() && zone.objects.is_empty())
    );
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &assigned);
}

#[test]
fn membership_rejects_stale_drafts_and_cancel_keeps_document_unchanged() {
    let (mut app, ctx, topic) = setup();
    let editor = draft(&mut app, &ctx, None);
    app.apply_zone_draft(&editor).unwrap();
    let membership = app.zone_membership_draft(&ctx).unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Change", |sheet| sheet.set_title(topic, "Changed"))
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(app.apply_zone_membership(&membership, true).is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);

    let mut h = Harness::builder()
        .with_size(egui::vec2(640.0, 480.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::AddToZone);
    h.run_steps(3);
    assert!(h.state().zones.editing());
    assert!(!h.state().command_available(&Cmd::RemoveFromZone));
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Cancel")
        .click();
    h.run_steps(3);
    assert!(!h.state().zones.editing());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn shape_only_zone_creation_and_locked_membership_availability() {
    let (mut app, ctx, _) = setup();
    let shape = add_shape(&mut app);
    app.doc_mut().unwrap().view.selection.clear();
    app.doc_mut().unwrap().view.board_selection = vec![shape];
    assert!(app.command_available(&Cmd::CreateZone));
    let editor = draft(&mut app, &ctx, None);
    assert!(editor.roots.is_empty());
    assert_eq!(editor.objects, vec![shape]);
    app.apply_zone_draft(&editor).unwrap();
    assert!(app.command_available(&Cmd::AddToZone));
    assert!(app.command_available(&Cmd::RemoveFromZone));
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Lock", |sheet| {
            sheet
                .set_canvas_objects_locked(&[shape], true)
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    assert!(!app.command_available(&Cmd::CreateZone));
    assert!(!app.command_available(&Cmd::AddToZone));
    assert!(!app.command_available(&Cmd::RemoveFromZone));
}

#[test]
fn remove_zone_keeps_contents_and_is_undoable() {
    let (mut app, ctx, topic) = setup();
    let shape = add_shape(&mut app);
    app.doc_mut().unwrap().view.board_selection = vec![shape];
    let editor = draft(&mut app, &ctx, None);
    app.apply_zone_draft(&editor).unwrap();
    let id = app.doc().unwrap().doc.sheet().zones[0].id;
    app.doc_mut()
        .unwrap()
        .view
        .select_object(thinoda_canvas::ObjectSelection::Zone(id));
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(app.command_available(&Cmd::RemoveZone));
    app.exec(&ctx, Cmd::RemoveZone);
    let doc = app.doc_mut().unwrap();
    assert!(doc.doc.sheet().zones.is_empty());
    assert!(doc.doc.sheet().topic(topic).is_some());
    assert!(doc.doc.sheet().canvas.object(shape).is_some());
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
}

#[test]
fn dedicated_zone_menu_and_native_snapshot_share_all_commands() {
    let (app, ctx, _) = setup();
    let entries = app.main_menu_entries();
    let zone = entries
        .iter()
        .find_map(|entry| match entry {
            crate::menus::MenuEntry::Submenu { label, entries } if label == "Zone" => Some(entries),
            _ => None,
        })
        .unwrap();
    let snapshot = app.native_menu_snapshot(&ctx);
    for (command, label) in ThinodaApp::zone_menu_commands() {
        assert!(zone.iter().any(|entry| matches!(entry,
            crate::menus::MenuEntry::Command { label: actual, command: actual_command }
            if actual == label && actual_command == &command)));
        assert!(snapshot.command_id(&command).is_some());
        assert_eq!(
            snapshot.command_enabled(&command),
            app.command_available(&command)
        );
    }
}

#[test]
fn picker_applies_selected_target_and_returns_from_child_viewport() {
    let (mut app, ctx, topic) = setup();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Zones", |sheet| {
            sheet
                .create_board_frame(
                    "First",
                    Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
                )
                .map_err(thinoda_core::EditError::Invalid)?;
            sheet
                .create_board_frame(
                    "Destination",
                    Rect::from_min_size(Vec2::new(600.0, 0.0), Vec2::new(300.0, 200.0)),
                )
                .map_err(thinoda_core::EditError::Invalid)?;
            Ok(())
        })
        .unwrap();
    let child = egui::ViewportId::from_hash_of("zone-membership-child");
    app.begin_zone_membership(&ctx);
    app.zones.membership.as_mut().unwrap().owner.viewport = child;
    app.return_zone_viewport(child);
    assert_eq!(
        app.zones.membership.as_ref().unwrap().owner.viewport,
        egui::ViewportId::ROOT
    );
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(640.0, 480.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_label("Destination").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Apply")
        .click();
    h.run_steps(3);
    assert!(!h.state().zones.editing());
    let doc = h.state_mut().doc_mut().unwrap();
    assert!(doc.doc.sheet().zones[0].roots.is_empty());
    assert_eq!(doc.doc.sheet().zones[1].roots, vec![topic]);
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
}

#[test]
fn removing_last_legacy_zone_member_preserves_zone_and_topics() {
    let (mut app, ctx, topic) = setup();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Legacy Zone", |sheet| {
            sheet
                .create_zone(
                    "Legacy",
                    vec![topic],
                    Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
                )
                .map_err(thinoda_core::EditError::Invalid)?;
            Ok(())
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(!app.doc().unwrap().doc.sheet().zones[0].keep_empty);
    app.exec(&ctx, Cmd::RemoveFromZone);
    let doc = app.doc_mut().unwrap();
    assert_eq!(doc.doc.sheet().zones.len(), 1);
    assert!(doc.doc.sheet().zones[0].keep_empty);
    assert!(doc.doc.sheet().zones[0].roots.is_empty());
    assert!(doc.doc.sheet().topic(topic).is_some());
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
}

#[test]
fn zone_selection_includes_group_peers_and_rejects_central_root() {
    let (mut app, ctx, topic) = setup();
    let first = add_shape(&mut app);
    let second = add_shape(&mut app);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Group", |sheet| {
            sheet
                .group_board_members(&[BoardMember::Object(first), BoardMember::Object(second)])
                .map_err(thinoda_core::EditError::Invalid)?;
            Ok(())
        })
        .unwrap();
    app.doc_mut().unwrap().view.board_selection = vec![first];
    let (roots, objects) = app.selected_zone_members().unwrap();
    assert_eq!(roots, vec![topic]);
    assert!(objects.contains(&first));
    assert!(objects.contains(&second));
    assert_eq!(objects.len(), 2);
    let editor = draft(&mut app, &ctx, None);
    app.apply_zone_draft(&editor).unwrap();
    assert_eq!(app.doc().unwrap().doc.sheet().zones[0].objects.len(), 2);

    let root = app.doc().unwrap().doc.sheet().root;
    app.doc_mut().unwrap().view.select_only(root);
    assert!(!app.command_available(&Cmd::AddToZone));
    assert!(!app.command_available(&Cmd::RemoveFromZone));
    assert!(!app.command_available(&Cmd::CreateZone));
}
