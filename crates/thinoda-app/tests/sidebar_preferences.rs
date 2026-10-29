//! Locale changes are process-wide; keep restart checks sequential.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, FocusField, FormatTab, Services, SidebarFamily, ThinodaApp},
    i18n::t,
};
use thinoda_core::Workbook;
use thinoda_platform::{
    Paths,
    settings::{Language, Settings, ThemeMode},
};

fn exec(h: &mut Harness<'_, ThinodaApp>, command: Cmd) {
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, command);
    h.run_steps(4);
}

fn restart(paths: &Paths) -> ThinodaApp {
    ThinodaApp::new(
        Settings::load(paths).unwrap(),
        Services {
            paths: Some(paths.clone()),
            ..Default::default()
        },
        &[],
    )
}

#[test]
fn sidebar_choices_survive_restart_without_changing_maps_or_other_preferences() {
    let root = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(root.path());
    paths.ensure().unwrap();
    let config = paths.config_dir.join("settings.toml");
    for source in [
        "show_format_panel = false\nui_scale = 1.25\n",
        "show_format_panel = false\nui_scale = 1.25\nsidebar_family = 'future_assets'\nformat_tab = 'future_pitch'\n",
    ] {
        std::fs::write(&config, source).unwrap();
        let app = restart(&paths);
        assert_eq!(app.sidebar_family, SidebarFamily::Format);
        assert_eq!(app.format_tab, FormatTab::Style);
        assert!(!app.settings.show_format_panel);
        assert_eq!(app.settings.ui_scale, 1.25);
        assert!(
            config.exists(),
            "valid future choices must not quarantine the config"
        );
    }
    for language in [Language::English, Language::German] {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let mut app = ThinodaApp::new(
                Settings {
                    language,
                    theme,
                    ..Default::default()
                },
                Services {
                    paths: Some(paths.clone()),
                    ..Default::default()
                },
                &[],
            );
            app.new_doc(Workbook::default());
            let mut h = Harness::builder()
                .with_size(egui::vec2(1000.0, 700.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            h.run_steps(4);
            let book = h.state().doc().unwrap().doc.workbook().clone();
            let state = h.state().doc().unwrap().doc.state_id();
            let selection = h.state().doc().unwrap().view.selection.clone();
            h.get_by_role_and_label(Role::Tab, t("Map")).click();
            h.run_steps(4);
            assert_eq!(restart(&paths).format_tab, FormatTab::Map);
            exec(&mut h, Cmd::ToggleSymbolsPanel);
            let app = restart(&paths);
            assert_eq!(app.sidebar_family, SidebarFamily::Symbols);
            assert_eq!(app.format_tab, FormatTab::Map);
            assert!(app.settings.show_format_panel);
            exec(&mut h, Cmd::ToggleSymbolsPanel);
            let mut app = restart(&paths);
            assert!(!app.settings.show_format_panel);
            assert_eq!(app.sidebar_family, SidebarFamily::Symbols);
            assert_eq!(app.format_tab, FormatTab::Map);
            app.exec(&egui::Context::default(), Cmd::ToggleFormatPanel);
            assert_eq!(
                app.sidebar_family,
                SidebarFamily::Symbols,
                "Home must not change remembered choice"
            );
            app.new_doc(Workbook::default());
            app.exec(&egui::Context::default(), Cmd::ToggleFormatPanel);
            assert!(app.settings.show_format_panel);
            assert_eq!(app.sidebar_family, SidebarFamily::Format);
            assert_eq!(app.format_tab, FormatTab::Map);
            let app = restart(&paths);
            assert_eq!(app.sidebar_family, SidebarFamily::Format);
            assert_eq!(app.format_tab, FormatTab::Map);
            assert_eq!(app.settings.theme, theme);
            assert_eq!(app.settings.language, language);
            for (command, expected_tab) in [
                (Cmd::TopicStyle, FormatTab::Style),
                (Cmd::EditField(FocusField::Notes), FormatTab::Data),
                (Cmd::EditField(FocusField::Hyperlink), FormatTab::Data),
            ] {
                exec(&mut h, Cmd::ToggleFormatPanel);
                if h.state().settings.show_format_panel {
                    h.get_by_role_and_label(Role::Tab, t("Map")).click();
                    h.run_steps(4);
                }
                exec(&mut h, command);
                let app = restart(&paths);
                assert_eq!(app.sidebar_family, SidebarFamily::Format);
                assert_eq!(app.format_tab, expected_tab);
                assert!(app.settings.show_format_panel);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
            }
            h.get_by_role_and_label(Role::Tab, t("Map")).click();
            h.run_steps(4);
            exec(&mut h, Cmd::ToggleFormatPanel);
            exec(&mut h, Cmd::ToggleFormatPanel);
            assert_eq!(h.state().format_tab, FormatTab::Map);
            // A sentinel in this test's own config detects accidental per-frame saves.
            let bytes = std::fs::read(&config).unwrap();
            let mut sentinel = bytes.clone();
            sentinel.extend_from_slice(b"\n# idle frames must not save\n");
            std::fs::write(&config, &sentinel).unwrap();
            h.run_steps(8);
            assert_eq!(std::fs::read(&config).unwrap(), sentinel);
            let app = restart(&paths);
            assert_eq!(app.format_tab, FormatTab::Map);
            assert_eq!(app.sidebar_family, SidebarFamily::Format);
        }
    }
}
