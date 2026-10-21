//! Locale changes must be sequential because the active language is process-wide.

use egui::{Key, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::Action;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn exec(h: &mut Harness<'_, ThinodaApp>, command: Cmd) {
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, command);
    h.run_steps(8);
}

fn assert_visible(h: &Harness<'_, ThinodaApp>, label: &str) {
    let rect = h.get_by_role_and_label(Role::Button, label).rect();
    assert!(
        h.ctx.content_rect().contains_rect(rect),
        "{label}: {rect:?}"
    );
    assert!(
        h.output().shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
            if text.galley.text() == label
                && rect.contains(text.pos)
                && shape.clip_rect.expand(1.0 / h.ctx.pixels_per_point())
                    .contains_rect(text.galley.rect.translate(text.pos.to_vec2())))
        }),
        "{label}: title paint must be fully visible"
    );
}

#[test]
fn deep_outline_topics_remain_reachable_for_scrolling_keyboard_and_editing() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for (size, scale) in [
                (egui::vec2(420.0, 600.0), 1.0),
                (egui::vec2(640.0, 480.0), 2.0),
                (egui::vec2(1000.0, 700.0), 1.5),
            ] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: scale,
                        show_format_panel: false,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                app.mode = CenterMode::Outliner;
                let doc = app.doc_mut().unwrap();
                let root = doc.doc.sheet().root;
                let mut parent = root;
                let mut topics = Vec::new();
                doc.doc
                    .edit_sheet("Deep fixture", |sheet| {
                        sheet.set_title(root, "Outline root")?;
                        for depth in 0..50 {
                            parent =
                                sheet.add_child(parent, None, format!("Outline depth {depth}"));
                            topics.push(parent);
                        }
                        Ok(())
                    })
                    .unwrap();
                doc.view.select_only(root);
                let before = doc.doc.workbook().clone();
                let revision = doc.doc.state_id();
                let mut h = Harness::builder()
                    .with_size(size * scale)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(4);
                h.set_size(size);
                h.run_steps(4);
                assert!((h.ctx.content_rect().width() - size.x).abs() < 0.01);
                h.get_by_role_and_label(Role::Button, "Outline depth 49")
                    .scroll_to_me();
                h.run_steps(8);
                assert_visible(&h, "Outline depth 49");
                h.get_by_role_and_label(Role::Button, "Outline depth 49")
                    .click();
                h.run_steps(8);
                assert_eq!(h.state().doc().unwrap().view.primary(), Some(topics[49]));
                // Stable selection must not pin the user's manual scroll position.
                h.get_by_role_and_label(Role::Button, "Outline root")
                    .scroll_to_me();
                h.run_steps(8);
                assert_visible(&h, "Outline root");
                h.run_steps(8);
                assert_visible(&h, "Outline root");
                h.key_press(Key::ArrowUp);
                h.run_steps(8);
                assert_eq!(h.state().doc().unwrap().view.primary(), Some(topics[48]));
                assert_visible(&h, "Outline depth 48");
                h.key_press(Key::ArrowDown);
                h.run_steps(8);
                assert_visible(&h, "Outline depth 49");
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);

                exec(&mut h, Cmd::Canvas(Action::StartEdit));
                let edit = h.get_by_role_and_label(Role::TextInput, t("Topic title"));
                assert!(
                    h.ctx.content_rect().contains_rect(edit.rect()),
                    "{:?}",
                    edit.rect()
                );
                edit.type_text(" changed");
                h.run_steps(4);
                // Save the edit without invoking Enter's existing sibling-insertion behavior.
                let doc = h.state_mut().doc_mut().unwrap();
                doc.view.commit_edit(&mut doc.doc);
                h.run_steps(4);
                assert_ne!(h.state().doc().unwrap().doc.workbook(), &before);
                exec(&mut h, Cmd::Canvas(Action::Undo));
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                h.get_by_role_and_label(Role::Button, "Outline depth 49")
                    .scroll_to_me();
                h.run_steps(8);
                let saved_rect = h
                    .get_by_role_and_label(Role::Button, "Outline depth 49")
                    .rect();
                exec(&mut h, Cmd::New);
                let doc = h.state_mut().doc_mut().unwrap();
                doc.doc
                    .edit_sheet("Other document", |sheet| {
                        sheet.set_title(sheet.root, "Other outline root")
                    })
                    .unwrap();
                h.state_mut().mode = CenterMode::Outliner;
                h.run_steps(8);
                assert_visible(&h, "Other outline root");
                h.state_mut().active = 0;
                h.run_steps(8);
                assert_visible(&h, "Outline depth 49");
                let restored_rect = h
                    .get_by_role_and_label(Role::Button, "Outline depth 49")
                    .rect();
                assert!((saved_rect.min - restored_rect.min).length() < 1.0);
            }
        }
    }
}
