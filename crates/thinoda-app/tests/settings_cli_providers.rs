//! Sequential locale/appearance matrix; no real CLI is executed.
use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{AiProvider, Language, Settings, ThemeMode};

#[test]
fn provider_settings_are_localized_and_preserve_api_settings_in_all_appearances() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for provider in AiProvider::ALL {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.settings.ai.provider = provider;
                app.settings.ai.model = "preserved-api-model".into();
                app.settings.ai.embedding_model = "preserved-embedding-model".into();
                let original = app.settings.ai.clone();
                let mut h = Harness::builder()
                    .with_size(egui::vec2(700.0, 1100.0))
                    .build_ui_state(
                        |ui, app: &mut ThinodaApp| {
                            // False suppresses credential/probe actions, just as an inactive
                            // dialog surface does; this is a layout test, not a live CLI test.
                            app.ai_settings_section(ui, false);
                        },
                        app,
                    );
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(3);
                h.get_by_role_and_label(Role::ComboBox, t("Generation provider"));
                h.get_by_role_and_label(Role::TextInput, t("Server URL"));
                h.get_by_role_and_label(Role::TextInput, t("Embedding model"));
                h.get_by_role_and_label(Role::Button, t("Test Connection"));
                if provider != AiProvider::OpenAiApi {
                    h.get_by_role_and_label(Role::Button, t("Check CLI status"));
                    h.get_by_role_and_label(Role::Button, t("Copy login command"));
                }
                assert_eq!(h.state().settings.ai, original);
            }
        }
    }
}
