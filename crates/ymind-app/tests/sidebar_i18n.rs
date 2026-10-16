//! Separate process because the app's current UI language is process-wide.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use ymind_app::app::{Cmd, FormatTab, Services, SidebarFamily, YMindApp};
use ymind_platform::settings::{Language, Settings};

#[test]
fn german_sidebar_names_switch_and_edit_the_same_document() {
    let settings = Settings {
        language: Language::German,
        ..Settings::default()
    };
    let mut app = YMindApp::new(settings, Services::default(), &[]);
    app.exec(&egui::Context::default(), Cmd::New);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Classic").click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().theme.theme_id,
        "classic"
    );
    h.get_by_role_and_label(Role::Button, "Symbole").click();
    h.run_steps(3);
    assert_eq!(h.state().sidebar_family, SidebarFamily::Symbols);
    h.get_by_role_and_label(Role::Button, "Richtig");
    h.get_by_role_and_label(Role::Button, "Priorität 1").click();
    h.run_steps(3);
    let sheet = h.state().doc().unwrap().doc.sheet();
    assert_eq!(sheet.topic(sheet.root).unwrap().markers, vec!["priority-1"]);
    h.get_by_role_and_label(Role::Button, "Formatbereich")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().format_tab, FormatTab::Map);
    h.get_by_role_and_label(Role::Tab, "Stil").click();
    h.run_steps(3);
    assert_eq!(h.state().format_tab, FormatTab::Style);
}
