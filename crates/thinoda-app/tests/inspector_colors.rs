use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{Cmd, FormatTab, Services, ThinodaApp};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

// Locale is process-wide; keep translated UI coverage out of English-only suites.
#[test]
fn german_fill_and_border_controls_edit_independently() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for scale in [1.0, 1.5, 2.0] {
            let mut app = ThinodaApp::new(
                Settings {
                    language: Language::German,
                    theme,
                    ui_scale: scale,
                    show_format_panel: true,
                    show_topic_toolbar: false,
                    ..Default::default()
                },
                Services::default(),
                &[],
            );
            app.exec(&egui::Context::default(), Cmd::New);
            app.format_tab = FormatTab::Style;
            let doc = app.doc_mut().unwrap();
            let child = doc.doc.sheet().children(doc.doc.sheet().root)[0];
            doc.view.selection = vec![child];
            let before = doc.doc.workbook().clone();
            let mut h = Harness::builder()
                .with_size(egui::vec2(1200.0, 1000.0) * scale)
                .with_step_dt(1.0 / 60.0)
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            h.run_steps(8);
            h.set_size(egui::vec2(1200.0, 1000.0));
            h.run_steps(8);
            h.get_by_role_and_label(Role::Label, "Füllung");
            h.get_by_role_and_label(Role::Label, "Rahmen");
            h.query_all_by_role_and_label(Role::ColorWell, "#2E7CF6")
                .next()
                .unwrap()
                .scroll_to_me();
            h.run_steps(8);
            h.query_all_by_role_and_label(Role::ColorWell, "#2E7CF6")
                .next()
                .unwrap()
                .click();
            h.run_steps(4);
            let blue = thinoda_core::Color::from_hex("#2E7CF6").unwrap();
            let doc = h.state().doc().unwrap();
            assert_eq!(doc.doc.sheet().topic(child).unwrap().style.fill, Some(blue));
            assert_eq!(
                doc.doc.sheet().topic(child).unwrap().style.border_color,
                before.sheets[0].topic(child).unwrap().style.border_color
            );
            let filled = doc.doc.workbook().clone();
            h.query_all_by_role_and_label(Role::ColorWell, "#2BB673")
                .nth(1)
                .unwrap()
                .scroll_to_me();
            h.run_steps(8);
            h.query_all_by_role_and_label(Role::ColorWell, "#2BB673")
                .nth(1)
                .unwrap()
                .click();
            h.run_steps(4);
            let style = &h
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .topic(child)
                .unwrap()
                .style;
            assert_eq!(style.fill, Some(blue));
            assert_eq!(
                style.border_color,
                Some(thinoda_core::Color::from_hex("#2BB673").unwrap())
            );
            let ctx = h.ctx.clone();
            h.state_mut()
                .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
            h.run_steps(4);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &filled);
            h.state_mut()
                .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
            h.run_steps(4);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }
    }
}
