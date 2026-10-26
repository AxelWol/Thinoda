use egui::{accesskit::Role, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn fixture(language: Language, theme: ThemeMode) -> ThinodaApp {
    let mut app = ThinodaApp::new(
        Settings {
            language,
            theme,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    app.exec(&egui::Context::default(), Cmd::New);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Checklist", |sheet| {
            sheet.set_todo(sheet.root, Some(false))?;
            for done in [true, false] {
                let id = sheet.add_child(sheet.root, None, "Checklist item");
                sheet.set_todo(id, Some(done))?;
            }
            Ok(())
        })
        .unwrap();
    app
}

#[test]
fn localized_subtree_controls_work_in_all_themes_and_views_with_one_undo() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            let app = fixture(language, theme);
            let before = app.doc().unwrap().doc.sheet().clone();
            let root = before.root;
            let mut h = Harness::builder()
                .with_size(vec2(1000.0, 800.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            h.run_steps(4);
            for mode in [CenterMode::Map, CenterMode::Outliner] {
                h.state_mut().mode = mode;
                h.set_size(vec2(360.0, 640.0));
                h.run_steps(5);
                h.get_by_role_and_label(Role::Button, t("Complete subtree To-Dos"))
                    .focus();
                h.run_steps(8);
                assert!(
                    h.ctx.content_rect().contains_rect(
                        h.get_by_role_and_label(Role::Button, t("Complete subtree To-Dos"))
                            .rect()
                    )
                );
                h.key_press(egui::Key::Enter);
                h.run_steps(4);
                assert!(h.state().doc().unwrap().doc.sheet().todo_progress()[&root].complete());
                h.get_by_role_and_label(Role::Button, t("Reopen subtree To-Dos"))
                    .focus();
                h.run_steps(8);
                h.key_press(egui::Key::Enter);
                h.run_steps(4);
                assert_eq!(
                    h.state().doc().unwrap().doc.sheet().todo_progress()[&root].completed,
                    0
                );
                let doc = &mut h.state_mut().doc_mut().unwrap().doc;
                assert!(doc.undo());
                assert!(doc.sheet().todo_progress()[&root].complete());
                assert!(doc.undo());
                assert_eq!(doc.sheet(), &before);
            }
            h.state_mut()
                .doc_mut()
                .unwrap()
                .view
                .selection
                .push(before.children(root)[0]);
            h.run_steps(4);
            assert!(
                h.query_by_role_and_label(Role::Button, t("Complete subtree To-Dos"))
                    .is_none(),
                "subtree controls require an unambiguous single-topic selection"
            );
        }
    }
}

#[test]
fn canvas_parent_checkbox_completes_and_reopens_descendants_with_one_undo() {
    let mut app = fixture(Language::English, ThemeMode::Dark);
    app.settings.show_format_panel = false;
    let before = app.doc().unwrap().doc.sheet().clone();
    let root = before.root;
    let mut h = Harness::builder()
        .with_size(vec2(1000.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(5);
    for done in [true, false] {
        let point = {
            let doc = h.state_mut().doc_mut().unwrap();
            let layout = doc.view.layout_for(&doc.doc);
            let point = layout.topics[&root].todo_rect.unwrap().center();
            doc.view.canvas_rect().center()
                + doc.view.offset
                + vec2(point.x, point.y) * doc.view.zoom
        };
        h.event(egui::Event::PointerMoved(point));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(2);
        }
        h.run_steps(4);
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().todo_progress()[&root].complete(),
            done
        );
    }
    let doc = &mut h.state_mut().doc_mut().unwrap().doc;
    assert!(doc.undo());
    assert!(doc.sheet().todo_progress()[&root].complete());
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
}
