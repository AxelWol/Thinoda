//! A separate process avoids racing the application's process-wide UI language.
use egui::{Key, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{Cmd, Services, ThinodaApp};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

#[test]
fn german_print_dialog_is_translated_and_accessible_in_both_themes() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut app = ThinodaApp::new(
            Settings {
                language: Language::German,
                theme,
                ..Default::default()
            },
            Services::default(),
            &[],
        );
        app.exec(&egui::Context::default(), Cmd::New);
        let mut h = Harness::builder()
            .with_size(egui::vec2(1100.0, 800.0))
            .with_context_setup(|ctx| {
                ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
            })
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(3);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        h.get_by_label(thinoda_ui::icons::MENU).click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Drucken Ctrl+P")
            .click();
        h.run_steps(2);
        for label in [
            "Seite einrichten",
            "Papierformat",
            "Ausrichtung",
            "Seitenränder",
            "Auflösung",
        ] {
            assert!(h.query_all_by_label(label).count() > 0, "missing {label}");
        }
        for label in [
            "Hochformat",
            "Querformat",
            "Dunkle Map",
            "Map-Hintergrund",
            "Graustufen",
        ] {
            h.get_by_label(label);
        }
        let until = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while h
            .query_by_role_and_label(Role::Image, "Druckvorschau")
            .is_none()
        {
            assert!(std::time::Instant::now() < until, "print preview timed out");
            std::thread::sleep(std::time::Duration::from_millis(20));
            h.run_steps(1);
        }
        h.set_size(egui::vec2(720.0, 480.0));
        h.run_steps(4);
        for label in ["Weiter zum Drucken…", "Druck-PDF speichern…", "Schließen"] {
            let rect = h.get_by_role_and_label(Role::Button, label).rect();
            assert!(
                h.ctx.content_rect().contains_rect(rect),
                "{label}: {rect:?}"
            );
        }
        h.key_press(Key::Escape);
        h.run_steps(2);
        assert!(
            h.query_by_role_and_label(Role::Button, "Weiter zum Drucken…")
                .is_none()
        );
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}
