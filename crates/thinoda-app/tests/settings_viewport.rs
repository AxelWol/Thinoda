//! Settings locale checks run sequentially because language is process-wide.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn text_shapes<'a>(shape: &'a egui::Shape, result: &mut Vec<&'a egui::epaint::TextShape>) {
    match shape {
        egui::Shape::Text(text) => result.push(text),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                text_shapes(shape, result);
            }
        }
        _ => {}
    }
}

fn assert_frame(h: &Harness<'_, ThinodaApp>) {
    let rect = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_settings")))
        .expect("Settings area");
    assert!(
        h.ctx
            .content_rect()
            .expand(1.0 / h.ctx.pixels_per_point())
            .contains_rect(rect),
        "{rect:?} outside {:?}",
        h.ctx.content_rect()
    );
}

fn reveal(h: &mut Harness<'_, ThinodaApp>, role: Role, label: &'static str) {
    h.get_by_role_and_label(role, t(label)).scroll_to_me();
    h.run_steps(8);
    let rect = h.get_by_role_and_label(role, t(label)).rect();
    assert!(
        h.ctx.content_rect().contains_rect(rect),
        "{label}: {rect:?}"
    );
    if role == Role::Button {
        let mut found = false;
        for shape in &h.output().shapes {
            let mut texts = Vec::new();
            text_shapes(&shape.shape, &mut texts);
            for text in texts
                .into_iter()
                .filter(|text| rect.contains(text.pos) && text.galley.text() == t(label))
            {
                found = true;
                let ink = text.galley.rect.translate(text.pos.to_vec2());
                assert!(
                    shape
                        .clip_rect
                        .expand(1.0 / h.ctx.pixels_per_point())
                        .contains_rect(ink),
                    "{label}: {ink:?} clipped by {:?}",
                    shape.clip_rect
                );
            }
        }
        assert!(found, "{label} text is not painted inside its button");
    }
    assert_frame(h);
}

fn category(h: &mut Harness<'_, ThinodaApp>, label: &'static str) {
    reveal(h, Role::Tab, label);
    h.get_by_role_and_label(Role::Tab, t(label)).click();
    h.run_steps(4);
}

#[test]
fn every_settings_category_is_reachable_in_short_narrow_windows() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for (size, scale) in [
                (egui::vec2(320.0, 180.0), 1.0),
                (egui::vec2(320.0, 180.0), 2.0),
                (egui::vec2(640.0, 360.0), 1.5),
                (egui::vec2(1280.0, 800.0), 1.0),
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
                app.settings.cloud.preferred_folder = Some(std::path::PathBuf::from(format!(
                    "C:\\{}",
                    "a-very-long-folder-name\\".repeat(16)
                )));
                app.services
                    .cloud
                    .push(thinoda_platform::cloud::CloudFolder {
                        provider: thinoda_platform::cloud::CloudProvider::OneDrive,
                        name: "Test cloud".into(),
                        path: std::path::PathBuf::from(format!(
                            "C:\\{}",
                            "detected-long-folder\\".repeat(16)
                        )),
                    });
                app.exec(&egui::Context::default(), Cmd::New);
                app.exec(&egui::Context::default(), Cmd::Settings);
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
                assert_frame(&h);
                reveal(&mut h, Role::Switch, "Confirm before deleting");
                reveal(&mut h, Role::ComboBox, "Language");
                category(&mut h, "Editor");
                if size.x == 1280.0 {
                    for label in ["Show topic toolbar", "Show topic add buttons"] {
                        let control = h.get_by_role_and_label(Role::Switch, t(label)).rect();
                        let window = h
                            .ctx
                            .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_settings")))
                            .unwrap();
                        assert!(
                            window.contains_rect(control),
                            "{label}: {control:?} outside {window:?}"
                        );
                    }
                    h.get_by_role_and_label(Role::Switch, t("Show topic toolbar"))
                        .click();
                    h.run_steps(4);
                    assert!(!h.state().settings.show_topic_toolbar);
                }
                for label in [
                    "Show topic toolbar",
                    "Show topic add buttons",
                    "Reuse last structure for new blank maps",
                    "Reuse last theme for new blank maps",
                ] {
                    reveal(&mut h, Role::Switch, label);
                }
                h.get_by_role_and_label(Role::Switch, t("Reuse last theme for new blank maps"))
                    .click();
                h.run_steps(4);
                assert!(h.state().settings.reuse_last_theme);
                category(&mut h, "AI Assistant");
                // The AI page is always expanded; no disclosure toggle exists.
                assert!(
                    h.query_by_role_and_label(Role::DisclosureTriangle, t("AI Assistant"))
                        .is_none()
                );
                for label in ["Save API Key", "Clear", "Test Connection"] {
                    reveal(&mut h, Role::Button, label);
                }
                // Focusing a newly selected action must reveal it without executing it.
                h.get_by_role_and_label(Role::Button, t("Save API Key"))
                    .focus();
                h.run_steps(8);
                let key = h
                    .get_by_role_and_label(Role::Button, t("Save API Key"))
                    .rect();
                assert!(h.ctx.content_rect().contains_rect(key));
                let layer = h.ctx.layer_id_at(key.center()).expect("Settings layer");
                let mut traversed = 0;
                for _ in 0..12 {
                    h.key_press(egui::Key::Tab);
                    h.run_steps(8);
                    if let Some(response) = h
                        .ctx
                        .memory(|memory| memory.focused())
                        .and_then(|id| h.ctx.read_response(id))
                        .filter(|response| response.layer_id == layer)
                    {
                        traversed += 1;
                        assert!(
                            h.ctx.content_rect().contains_rect(response.rect),
                            "Tab focus: {:?}",
                            response.rect
                        );
                    }
                }
                assert!(traversed >= 3, "Tab must visit Settings controls");
                assert!(h.state().ai.test.is_none());
                h.state_mut().ai.api_key_draft = Some("unsaved-test-draft".into());
                h.set_size(egui::vec2(1000.0, 700.0));
                h.run_steps(8);
                assert_frame(&h);
                h.set_size(size);
                h.run_steps(8);
                assert_frame(&h);
                assert_eq!(
                    h.state().ai.api_key_draft.as_deref(),
                    Some("unsaved-test-draft")
                );
                category(&mut h, "Cloud");
                reveal(&mut h, Role::Button, "Use");
                h.get_by_role_and_label(Role::Button, t("Use")).click();
                h.run_steps(4);
                assert_eq!(
                    h.state().settings.cloud.preferred_folder,
                    Some(h.state().services.cloud[0].path.clone())
                );
                reveal(&mut h, Role::Button, "Choose…");
                category(&mut h, "General");
                // A live zoom change uses the same viewport measurement as startup.
                h.state_mut().settings.ui_scale = 1.5;
                h.run_steps(8);
                assert_frame(&h);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
                let close = h
                    .get_by_role_and_label(Role::Button, t("Close Window"))
                    .rect();
                assert!(h.ctx.content_rect().contains_rect(close));
                h.get_by_role_and_label(Role::Button, t("Close Window"))
                    .click();
                h.run_steps(4);
                assert!(!h.state().dialogs.settings);
                assert!(h.state().settings.reuse_last_theme);
                assert!(h.state().ai.test.is_none());
            }
        }
    }
}
