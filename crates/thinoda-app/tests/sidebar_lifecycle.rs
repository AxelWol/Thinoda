//! Separate process because the app's current UI language is process-wide.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, FormatTab, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::{
    Paths,
    settings::{Language, Settings, ThemeMode},
};

#[test]
fn restored_sidebar_reopens_across_home_views_languages_and_appearance() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(dir.path());
    paths.ensure().unwrap();

    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (visible, width, scale) in [
                (false, 640.0, 1.0),
                (true, 640.0, 1.0),
                (false, 1280.0, 1.5),
                (true, 1280.0, 1.5),
            ] {
                Settings {
                    language,
                    theme,
                    ui_scale: scale,
                    show_format_panel: visible,
                    ..Settings::default()
                }
                .save(&paths)
                .unwrap();
                let settings = Settings::load(&paths).unwrap();
                assert_eq!(settings.show_format_panel, visible);
                let mut app = ThinodaApp::new(
                    settings,
                    Services {
                        paths: Some(paths.clone()),
                        ..Services::default()
                    },
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                let mut h = Harness::builder()
                    .with_size(egui::vec2(width, 800.0))
                    .with_context_setup(|ctx| {
                        ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    })
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.run_steps(3);
                assert_eq!(
                    h.query_by_role_and_label(Role::Tab, t("Style")).is_some(),
                    visible,
                    "{language:?}/{theme:?}/{visible}"
                );
                let ctx = h.ctx.clone();
                if visible {
                    h.get_by_role_and_label(Role::Button, t("Format Panel"))
                        .click();
                    h.run_steps(3);
                }
                assert!(!Settings::load(&paths).unwrap().show_format_panel);

                for tab in [FormatTab::Style, FormatTab::Map] {
                    h.get_by_role_and_label(Role::Button, t("Format Panel"))
                        .click();
                    h.run_steps(3);
                    assert!(h.state().settings.show_format_panel);
                    let label = match tab {
                        FormatTab::Style => "Style",
                        FormatTab::Map => "Map",
                    };
                    let tab_rect = h.get_by_role_and_label(Role::Tab, t(label)).rect();
                    assert!(h.ctx.content_rect().contains_rect(tab_rect), "{tab_rect:?}");
                    h.get_by_role_and_label(Role::Tab, t(label)).click();
                    h.run();
                    assert_eq!(
                        h.state().format_tab,
                        tab,
                        "tab click: {language:?}/{theme:?}/{visible} {tab_rect:?}"
                    );
                    let selected = h.state().doc().unwrap().view.selection.clone();
                    let workbook = h.state().doc().unwrap().doc.workbook().clone();
                    let root_title = {
                        let sheet = h.state().doc().unwrap().doc.sheet();
                        sheet.topic(sheet.root).unwrap().title.clone()
                    };
                    h.state_mut().exec(&ctx, Cmd::ToggleOutliner);
                    h.run_steps(3);
                    h.get_by_role_and_label(Role::Button, t("Format Panel"))
                        .click();
                    h.run_steps(3);
                    assert!(!h.state().settings.show_format_panel);
                    h.state_mut().exec(&ctx, Cmd::Home);
                    h.run_steps(3);
                    h.get_by_role_and_label(Role::Button, t("Format Panel"))
                        .click();
                    h.run_steps(3);
                    h.get_by_label(t("Open or select a map to show the Format Panel"));
                    assert!(!h.state().settings.show_format_panel);
                    h.get_by_role_and_label(Role::Button, &root_title).click();
                    h.run_steps(3);
                    h.get_by_role_and_label(Role::Button, t("Format Panel"))
                        .click();
                    h.run_steps(3);
                    assert_eq!(
                        h.state().format_tab,
                        tab,
                        "reopen: {language:?}/{theme:?}/{visible}"
                    );
                    h.get_by_role_and_label(Role::Tab, t(label));
                    assert_eq!(h.state().doc().unwrap().view.selection, selected);
                    assert_eq!(h.state().doc().unwrap().doc.workbook(), &workbook);
                    assert!(Settings::load(&paths).unwrap().show_format_panel);
                    h.get_by_role_and_label(Role::Button, t("Format Panel"))
                        .click();
                    h.run_steps(3);
                }
            }
        }
    }
}
