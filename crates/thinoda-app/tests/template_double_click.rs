//! Language changes are process-wide, so exercise all variants in one test.
use egui::{PointerButton, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::templates::{self, TEMPLATES};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn click_at(h: &mut Harness<'_, ThinodaApp>, position: egui::Pos2) {
    h.hover_at(position);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
}

#[test]
fn double_click_creates_each_template_once_and_preserves_unsaved_maps_in_both_languages_and_themes()
{
    for language in [Language::English, Language::German] {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let mut app = ThinodaApp::new(
                Settings {
                    language,
                    theme,
                    ..Default::default()
                },
                Services::default(),
                &[],
            );
            app.exec(&egui::Context::default(), Cmd::New);
            let source = app.doc_mut().unwrap();
            let root = source.doc.sheet().root;
            source
                .doc
                .edit_sheet("Rename", |sheet| sheet.set_title(root, "Unsaved source"))
                .unwrap();
            let source_key = source.key.clone();
            let source_map = source.doc.workbook().clone();
            app.exec(&egui::Context::default(), Cmd::Home);
            let mut h = Harness::builder()
                .with_size(egui::vec2(1000.0, 800.0))
                .with_step_dt(1.0 / 60.0)
                .with_context_setup(|ctx| {
                    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
                })
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            let ctx = h.ctx.clone();

            h.run_steps(30);
            let position = h
                .get_by_role_and_label(Role::Button, t(TEMPLATES[0].name))
                .rect()
                .center();
            click_at(&mut h, position);
            h.run_steps(30);
            click_at(&mut h, position);
            assert_eq!(h.state().docs.len(), 1, "slow clicks only select");
            h.get_by_role_and_label(Role::Button, t("Cancel")).click();
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().key, source_key);
            assert!(h.state().settings.recent_templates.is_empty());

            for template in TEMPLATES {
                h.state_mut().exec(&ctx, Cmd::Home);
                h.state_mut().template_category = Some(template.category);
                h.run_steps(30);
                h.get_by_role_and_label(Role::Button, t(template.name))
                    .scroll_to_me();
                h.run_steps(8);
                let position = h
                    .get_by_role_and_label(Role::Button, t(template.name))
                    .rect()
                    .center();
                let count = h.state().docs.len();
                click_at(&mut h, position);
                assert_eq!(h.state().docs.len(), count);
                assert_eq!(h.state().template_choice, Some(template.id));
                click_at(&mut h, position);
                h.run_steps(3);
                assert_eq!(h.state().docs.len(), count + 1, "{}", template.id);
                assert_eq!(
                    h.state().doc().unwrap().doc.workbook(),
                    &templates::instantiate(template.id)
                );
                assert!(h.state().template_choice.is_none());
                assert_eq!(h.state().settings.recent_templates[0], template.id);
                assert_eq!(
                    h.state()
                        .settings
                        .recent_templates
                        .iter()
                        .filter(|id| id.as_str() == template.id)
                        .count(),
                    1
                );
            }

            h.state_mut().exec(&ctx, Cmd::Home);
            h.state_mut().template_category = None;
            h.run_steps(30);
            let overview = TEMPLATES[0];
            h.get_by_role_and_label(Role::Button, t(overview.name))
                .scroll_to_me();
            h.run_steps(8);
            let position = h
                .get_by_role_and_label(Role::Button, t(overview.name))
                .rect()
                .center();
            let count = h.state().docs.len();
            click_at(&mut h, position);
            click_at(&mut h, position);
            h.run_steps(3);
            assert_eq!(h.state().docs.len(), count + 1);
            assert_eq!(
                h.state().doc().unwrap().doc.workbook(),
                &templates::instantiate(overview.id)
            );

            h.state_mut().exec(&ctx, Cmd::Home);
            h.run_steps(30);
            let template = TEMPLATES.last().unwrap();
            let recent_label = format!("{}: {}", t("Recent template"), t(template.name));
            h.get_by_role_and_label(Role::Button, &recent_label)
                .scroll_to_me();
            h.run_steps(8);
            let position = h
                .get_by_role_and_label(Role::Button, &recent_label)
                .rect()
                .center();
            let count = h.state().docs.len();
            click_at(&mut h, position);
            assert_eq!(h.state().docs.len(), count);
            click_at(&mut h, position);
            h.run_steps(3);
            assert_eq!(h.state().docs.len(), count + 1);
            assert_eq!(
                h.state().doc().unwrap().doc.workbook(),
                &templates::instantiate(template.id)
            );
            let original = h
                .state()
                .docs
                .iter()
                .find(|doc| doc.key == source_key)
                .unwrap();
            assert_eq!(original.doc.workbook(), &source_map);
            assert!(original.doc.is_dirty());
            assert!(original.path.is_none());
        }
    }
}
