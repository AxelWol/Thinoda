use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};
use thinoda_core::{Task, WorkCalendar};
use thinoda_platform::settings::{Language, ThemeMode};

// Keep the locale matrix in its own test executable: the UI language is process-wide.
#[test]
fn date_pickers_follow_work_calendar_in_every_language_and_appearance() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            let mut app = ThinodaApp::headless();
            app.settings.language = language;
            app.settings.theme = theme;
            let topic = app.doc().unwrap().view.primary().unwrap();
            let mut calendar = WorkCalendar::default();
            calendar
                .exceptions
                .insert("2026-10-27".parse().unwrap(), false);
            calendar
                .exceptions
                .insert("2026-10-31".parse().unwrap(), true);
            app.doc_mut()
                .unwrap()
                .doc
                .set_work_calendar(calendar)
                .unwrap();
            app.doc_mut()
                .unwrap()
                .doc
                .set_task(
                    topic,
                    Some(Task {
                        start_date: Some("2026-10-26".parse().unwrap()),
                        duration_days: Some(1),
                        ..Default::default()
                    }),
                )
                .unwrap();
            let mut h = Harness::builder()
                .with_size(egui::vec2(360.0, 640.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::TaskPlanning);
            let before = h.state().doc().unwrap().doc.workbook().clone();
            h.run_steps(3);
            let labels = thinoda_app::i18n::calendar_labels();
            assert_eq!(
                labels
                    .weekdays
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                7
            );
            h.query_all_by_role_and_label(egui::Role::Button, t("Open calendar"))
                .next()
                .unwrap()
                .click();
            h.run_steps(3);
            for day in ["2026-10-27", "2026-10-25"] {
                let label = format!("{day}, {}", t("Non-working day"));
                let disabled = h.get_by_label(&label);
                assert!(h.ctx.content_rect().contains_rect(disabled.rect()));
                disabled.click();
                h.run_steps(2);
                assert!(
                    egui::Popup::is_any_open(&h.ctx),
                    "calendar dismissed after unavailable {day}, {language:?}/{theme:?}"
                );
            }
            h.get_by_label("2026-10-31").click();
            h.run_steps(2);
            h.get_by_role_and_label(egui::Role::TextInput, t("Due date"))
                .type_text("2026-10-31");
            h.run_steps(2);
            h.query_all_by_role_and_label(egui::Role::Button, t("Open calendar"))
                .nth(1)
                .unwrap()
                .click();
            h.run_steps(2);
            h.get_by_label(t("Next month")).click();
            h.run_steps(2);
            h.get_by_label("2026-11-02").click();
            h.run_steps(2);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            h.get_by_role_and_label(egui::Role::Button, t("Apply"))
                .click();
            h.run_steps(2);
            assert_eq!(
                h.state().doc().unwrap().doc.sheet().scheduling.tasks[&topic].start_date,
                Some("2026-10-31".parse().unwrap())
            );
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .compute_schedule()
                    .unwrap()[&topic]
                    .finish_date,
                Some("2026-11-02".parse().unwrap())
            );
            h.state_mut().doc_mut().unwrap().doc.undo();
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }
    }
}
