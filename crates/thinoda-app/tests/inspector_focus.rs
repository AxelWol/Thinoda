use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, FormatTab, Services, SidebarFamily, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn assert_visible(h: &Harness<'_, ThinodaApp>, role: Role, label: &str) {
    let rect = h
        .query_all_by_role_and_label(role, label)
        .find(|node| node.is_focused())
        .unwrap_or_else(|| panic!("{label}: control is not focused"))
        .rect();
    assert!(
        h.ctx.content_rect().contains_rect(rect),
        "{label}: {rect:?}"
    );
    assert!(
        h.output().shapes.iter().any(|shape| {
            shape.shape.visual_bounding_rect().intersects(rect)
                && shape.clip_rect.contains_rect(rect)
        }),
        "{label}: no visible paint in control bounds {rect:?}"
    );
}

#[test]
fn inspector_reveals_keyboard_focus_without_changing_document() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for (size, scale) in [
                (egui::vec2(320.0, 240.0), 2.0),
                (egui::vec2(800.0, 400.0), 1.5),
                (egui::vec2(1280.0, 800.0), 1.0),
            ] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: scale,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                app.format_tab = FormatTab::Data;
                let before = app.doc().unwrap().doc.workbook().clone();
                let revision = app.doc().unwrap().doc.state_id();
                let mut h = Harness::builder()
                    .with_size(size * scale)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(4);
                h.set_size(size);
                h.run_steps(4);
                h.get_by_role_and_label(Role::TextInput, t("Hyperlink"))
                    .focus();
                h.run_steps(8);
                assert_visible(&h, Role::TextInput, t("Hyperlink"));
                h.key_press(egui::Key::Tab);
                h.run_steps(8);
                assert!(
                    h.query_all_by_role_and_label(Role::Button, t("Edit labels"))
                        .any(|node| node.is_focused())
                );
                assert_visible(&h, Role::Button, t("Edit labels"));
                h.key_press(egui::Key::Tab);
                h.run_steps(8);
                assert!(
                    h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                        .is_focused()
                );
                assert_visible(&h, Role::MultilineTextInput, t("Notes"));
                h.state_mut().mode = CenterMode::Outliner;
                h.run_steps(4);
                h.get_by_role_and_label(Role::TextInput, t("Hyperlink"))
                    .focus();
                h.run_steps(8);
                let before_rect = h
                    .get_by_role_and_label(Role::TextInput, t("Hyperlink"))
                    .rect();
                h.get_by_role_and_label(Role::Button, t("Add Child"))
                    .focus();
                h.run_steps(8);
                assert_eq!(
                    h.get_by_role_and_label(Role::TextInput, t("Hyperlink"))
                        .rect(),
                    before_rect,
                    "Outline focus scrolled the inspector"
                );
                h.state_mut().format_tab = FormatTab::Map;
                h.run_steps(4);
                h.get_by_role_and_label(Role::Switch, t("Flexible Floating Topics"))
                    .focus();
                h.run_steps(8);
                assert_visible(&h, Role::Switch, t("Flexible Floating Topics"));
                h.get_by_role_and_label(Role::ComboBox, t("Global Font"))
                    .focus();
                h.run_steps(8);
                assert_visible(&h, Role::ComboBox, t("Global Font"));
                let top_rect = h
                    .get_by_role_and_label(Role::ComboBox, t("Global Font"))
                    .rect();
                h.get_by_role_and_label(Role::Switch, t("Flexible Floating Topics"))
                    .scroll_to_me();
                h.run_steps(8);
                let scrolled_rect = h
                    .get_by_role_and_label(Role::ComboBox, t("Global Font"))
                    .rect();
                assert_ne!(top_rect, scrolled_rect);
                h.run_steps(8);
                assert_eq!(
                    h.get_by_role_and_label(Role::ComboBox, t("Global Font"))
                        .rect(),
                    scrolled_rect,
                    "Unchanged focus snapped back after manual scrolling"
                );
                h.state_mut().sidebar_family = SidebarFamily::Symbols;
                h.run_steps(4);
                let last = thinoda_core::markers::GROUPS
                    .last()
                    .unwrap()
                    .markers
                    .last()
                    .unwrap();
                h.get_by_role_and_label(Role::Button, t(last.name)).focus();
                h.run_steps(8);
                assert_visible(&h, Role::Button, t(last.name));
                h.get_by_role_and_label(Role::Button, t("Default symbol color"))
                    .focus();
                h.run_steps(8);
                assert_visible(&h, Role::Button, t("Default symbol color"));
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
                h.state_mut().sidebar_family = SidebarFamily::Format;
                h.state_mut().format_tab = FormatTab::Data;
                h.run_steps(4);
                h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                    .focus();
                h.run_steps(8);
                h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                    .type_text("Reachable inspector draft");
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                h.get_by_role_and_label(Role::TextInput, t("Hyperlink"))
                    .focus();
                h.run_steps(8);
                let d = h.state_mut().doc_mut().unwrap();
                let root = d.doc.sheet().root;
                assert_eq!(
                    d.doc.sheet().topic(root).unwrap().notes.as_deref(),
                    Some("Reachable inspector draft")
                );
                assert!(d.doc.undo());
                assert_eq!(d.doc.workbook(), &before);
            }
        }
    }
}
