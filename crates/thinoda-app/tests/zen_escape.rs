use egui::{Key, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{CenterMode, Cmd, ThinodaApp};

#[test]
fn escape_exits_zen_with_focused_topics_without_clearing_selection() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.run_steps(4);
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::ToggleZen);
        h.run_steps(4);
        if mode == CenterMode::Outliner {
            let title = h
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .title
                .clone();
            h.get_by_role_and_label(Role::Button, &title).click();
            h.run_steps(3);
        } else {
            let pos = {
                let doc = h.state().doc().unwrap();
                doc.view.selected_topic_rect(&doc.doc).unwrap().center()
            };
            h.event(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
                h.run_steps(1);
            }
            h.run_steps(3);
        }
        assert!(h.ctx.memory(|memory| memory.focused()).is_some());
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let selection = h.state().doc().unwrap().view.selection.clone();
        h.key_press(Key::Escape);
        h.run_steps(3);
        assert!(!h.state().zen, "{mode:?}");
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert_eq!(h.state().doc().unwrap().view.selection, selection);
    }
}

#[test]
fn edit_search_palette_and_modal_escape_take_precedence_over_zen_exit() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.zen = true;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.run_steps(4);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let root = h.state().doc().unwrap().doc.sheet().root;
        let selection = h.state().doc().unwrap().view.selection.clone();
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), root, false);
        h.run_steps(4);
        h.key_press(Key::Escape);
        h.run_steps(4);
        assert!(h.state().zen);
        assert!(h.state().doc().unwrap().view.editing_topic().is_none());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        let ctx = h.ctx.clone();
        for command in [Cmd::FindReplace, Cmd::Palette, Cmd::Shortcuts] {
            h.state_mut().exec(&ctx, command.clone());
            h.run_steps(4);
            h.key_press(Key::Escape);
            h.run_steps(4);
            assert!(h.state().zen, "{mode:?}/{command:?}");
            assert!(!h.state().search.open);
            assert!(!h.state().palette.open);
            assert!(!h.state().dialogs.shortcuts);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            assert_eq!(h.state().doc().unwrap().view.selection, selection);
        }
        h.key_press(Key::Escape);
        h.run_steps(4);
        assert!(!h.state().zen);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn relationship_cancel_and_context_menu_escape_do_not_exit_zen() {
    let mut app = ThinodaApp::headless();
    app.zen = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    let root = h.state().doc().unwrap().doc.sheet().root;
    h.state_mut().doc_mut().unwrap().view.relationship_from = Some(root);
    h.run_steps(3);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().zen);
    assert!(h.state().doc().unwrap().view.relationship_from.is_none());
    h.state_mut().doc_mut().unwrap().view.select_only(root);
    h.run_steps(3);
    let pos = {
        let doc = h.state().doc().unwrap();
        doc.view.selected_topic_rect(&doc.doc).unwrap().center()
    };
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Default::default(),
        });
        h.run_steps(1);
    }
    h.run_steps(3);
    assert!(egui::Popup::is_any_open(&h.ctx));
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let selection = h.state().doc().unwrap().view.selection.clone();
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().zen);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert_eq!(h.state().doc().unwrap().view.selection, selection);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!h.state().zen);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert_eq!(h.state().doc().unwrap().view.selection, selection);
}
