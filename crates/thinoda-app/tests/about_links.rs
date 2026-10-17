//! About dialog links open the mail client / browser in every language and theme.
//! Separate process because the app's current UI language is process-wide.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{Cmd, Services, ThinodaApp};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

const MAILTO: &str = "mailto:axelwolters@hotmail.com";
const PROJECT: &str = "https://github.com/AxelWol/Thinoda";

/// Mirrors `eframe::App::ui`: draw, then hand the frame's `OpenUrl` commands to the opener.
struct Shell {
    app: ThinodaApp,
    opened: Vec<String>,
}

fn harness(language: Language, theme: ThemeMode) -> Harness<'static, Shell> {
    let settings = Settings {
        language,
        theme,
        ..Settings::default()
    };
    let mut app = ThinodaApp::new(settings, Services::default(), &[]);
    app.exec(&egui::Context::default(), Cmd::New);
    app.dialogs.about = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(
            |ui, s: &mut Shell| {
                s.app.draw(ui);
                s.opened.extend(ThinodaApp::take_open_urls(ui.ctx()));
            },
            Shell {
                app,
                opened: Vec::new(),
            },
        );
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h
}

fn luminance(c: egui::Color32) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

fn contrast(a: egui::Color32, b: egui::Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn about_links_open_mail_client_and_browser_in_all_languages_and_themes() {
    for (language, contact) in [
        (Language::English, "Contact: axelwolters@hotmail.com"),
        (Language::German, "Kontakt: axelwolters@hotmail.com"),
    ] {
        for (theme, egui_theme) in [
            (ThemeMode::Light, egui::Theme::Light),
            (ThemeMode::Dark, egui::Theme::Dark),
        ] {
            let mut h = harness(language, theme);
            assert_eq!(h.ctx.theme(), egui_theme, "{language:?}/{theme:?}");
            let v = h.ctx.style_of(egui_theme).visuals.clone();
            let ratio = contrast(v.hyperlink_color, v.window_fill);
            assert!(
                ratio >= 3.0,
                "{language:?}/{theme:?}: link contrast {ratio:.2} too low"
            );

            h.get_by_role_and_label(Role::Link, contact).click();
            h.run_steps(2);
            assert_eq!(h.state().opened, [MAILTO], "{language:?}/{theme:?}");

            h.get_by_role_and_label(Role::Link, PROJECT).click();
            h.run_steps(2);
            assert_eq!(
                h.state().opened,
                [MAILTO, PROJECT],
                "{language:?}/{theme:?}"
            );
            assert!(
                !h.output()
                    .platform_output
                    .commands
                    .iter()
                    .any(|c| matches!(c, egui::OutputCommand::OpenUrl(_))),
                "opened links are consumed, not left for eframe to drop"
            );
            for url in &h.state().opened {
                assert!(thinoda_platform::system::is_openable_url(url), "{url}");
            }
            assert!(h.state().app.dialogs.about, "dialog stays open");
        }
    }
}
