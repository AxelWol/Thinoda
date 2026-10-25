use super::*;
use crate::app::Cmd;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{Vec2, Workbook};

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

fn player_harness(width: f32, presenter: bool) -> Harness<'static, Playing> {
    let mut app = setup(true);
    app.open_pitch(&Context::default(), presenter);
    let player = app.pitch.player.take().unwrap();
    Harness::builder()
        .with_size(egui::vec2(width, 650.0))
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
fn stale_playback_closes_without_touching_the_revised_document_or_root_window() {
    let mut app = setup(true);
    let ctx = Context::default();
    app.open_pitch(&ctx, false);
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("External edit", |sheet| sheet.set_title(root, "Revised"))
        .unwrap();
    let before = doc.doc.workbook().clone();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| app.pitch_window(&ctx));
    output.textures_delta.clear();
    assert!(app.pitch.player.is_none());
    assert_eq!(app.docs.len(), 1);
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    assert!(
        !output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close)
    );
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
