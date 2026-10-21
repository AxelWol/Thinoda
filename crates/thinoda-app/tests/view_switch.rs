//! The locale matrix runs sequentially because UI language is process-wide.

use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::Workbook;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn exec(h: &mut Harness<'_, ThinodaApp>, command: Cmd) {
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, command);
    h.run_steps(4);
}

fn assert_control(h: &Harness<'_, ThinodaApp>, label: &'static str) {
    let rect = h
        .get_by_role_and_label(Role::Button, &view_label(label))
        .rect();
    assert!(
        h.ctx
            .content_rect()
            .expand(1.0 / h.ctx.pixels_per_point())
            .contains_rect(rect)
    );
    fn texts<'a>(shape: &'a egui::Shape, result: &mut Vec<&'a egui::epaint::TextShape>) {
        match shape {
            egui::Shape::Text(text) => result.push(text),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    texts(shape, result);
                }
            }
            _ => {}
        }
    }
    let mut found = false;
    for shape in &h.output().shapes {
        let mut painted = Vec::new();
        texts(&shape.shape, &mut painted);
        for text in painted
            .into_iter()
            .filter(|text| rect.contains(text.pos) && text.galley.text().ends_with(t(label)))
        {
            found = true;
            let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
            assert!(
                shape
                    .clip_rect
                    .expand(1.0 / h.ctx.pixels_per_point())
                    .contains_rect(bounds),
                "{label}: {bounds:?} clipped by {:?}",
                shape.clip_rect
            );
        }
    }
    assert!(found, "view label must be painted inside its control");
}

fn view_label(mode: &'static str) -> String {
    format!("{}: {}", t("View"), t(mode))
}

#[test]
fn bottom_view_switch_shares_command_semantics_and_preserves_pending_edits() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for (size, scale) in [
                (egui::vec2(320.0, 240.0), 2.0),
                (egui::vec2(640.0, 480.0), 1.5),
                (egui::vec2(1000.0, 700.0), 1.0),
            ] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: scale,
                        show_format_panel: false,
                        show_topic_toolbar: false,
                        show_topic_add_buttons: false,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.new_doc(Workbook::default());
                let mut h = Harness::builder()
                    .with_size(size * scale)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(4);
                h.set_size(size);
                h.run_steps(4);
                assert!((h.ctx.content_rect().width() - size.x).abs() < 0.01);
                let book = h.state().doc().unwrap().doc.workbook().clone();
                let state = h.state().doc().unwrap().doc.state_id();
                let selection = h.state().doc().unwrap().view.selection.clone();
                assert_control(&h, "Map");
                h.get_by_role_and_label(Role::Button, &view_label("Map"))
                    .click();
                h.run_steps(4);
                assert_eq!(h.state().mode, CenterMode::Outliner);
                assert_control(&h, "Outliner");
                h.get_by_role_and_label(Role::Button, &view_label("Outliner"))
                    .focus();
                h.run_steps(4);
                h.key_press(Key::Enter);
                h.run_steps(4);
                assert_eq!(h.state().mode, CenterMode::Map);
                h.get_by_role_and_label(Role::Button, &view_label("Map"))
                    .focus();
                h.run_steps(4);
                h.key_press(Key::Space);
                h.run_steps(4);
                assert_eq!(h.state().mode, CenterMode::Outliner);
                h.key_press_modifiers(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::O);
                h.run_steps(4);
                assert_eq!(h.state().mode, CenterMode::Map);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
                h.get_by_role_and_label(Role::Button, t("Zoom")).focus();
                h.run_steps(8);
                h.get_by_role_and_label(Role::Button, &view_label("Map"))
                    .focus();
                h.run_steps(8);
                assert_control(&h, "Map");

                // View switching must not flush or discard a pending topic title.
                let doc = h.state_mut().doc_mut().unwrap();
                doc.view
                    .begin_edit(doc.doc.sheet(), doc.doc.sheet().root, false);
                doc.view.set_editing_text("Pending title across views");
                h.run_steps(4);
                h.get_by_role_and_label(Role::Button, &view_label("Map"))
                    .click();
                h.run_steps(4);
                assert_eq!(h.state().mode, CenterMode::Outliner);
                assert_eq!(
                    h.state().doc().unwrap().view.editing_text(),
                    Some("Pending title across views")
                );
                h.get_by_role_and_label(Role::Button, &view_label("Outliner"))
                    .click();
                h.run_steps(4);
                assert_eq!(h.state().mode, CenterMode::Map);
                assert_eq!(
                    h.state().doc().unwrap().view.editing_text(),
                    Some("Pending title across views")
                );
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), state);

                exec(&mut h, Cmd::Palette);
                assert!(
                    h.get_by_role_and_label(Role::Button, &view_label("Map"))
                        .accesskit_node()
                        .is_disabled()
                );
                h.key_press(Key::Escape);
                h.run_steps(4);
                exec(&mut h, Cmd::ToggleStatusBar);
                assert!(
                    h.query_by_role_and_label(Role::Button, &view_label("Map"))
                        .is_none()
                );
                exec(&mut h, Cmd::ToggleStatusBar);
                assert_control(&h, "Map");
                exec(&mut h, Cmd::ToggleZen);
                assert!(
                    h.query_by_role_and_label(Role::Button, &view_label("Map"))
                        .is_none()
                );
                exec(&mut h, Cmd::ToggleZen);
                assert_control(&h, "Map");
                exec(&mut h, Cmd::Home);
                assert!(
                    h.query_by_role_and_label(Role::Button, &view_label("Map"))
                        .is_none()
                );
            }
        }
    }
}
