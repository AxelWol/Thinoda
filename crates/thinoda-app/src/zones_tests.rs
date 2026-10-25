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
