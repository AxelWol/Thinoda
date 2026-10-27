use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};
use thinoda_core::{Sheet, Workbook};
use thinoda_platform::settings::{Language, ThemeMode};

#[test]
fn live_pitch_controls_follow_every_language_and_appearance_without_editing_the_map() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            let mut app = ThinodaApp::headless();
            app.settings.language = language;
            app.settings.theme = theme;
            let mut sheet = Sheet::new("Presentation", "Center");
            let first = sheet.insert_child(sheet.root, "Opening").unwrap();
            sheet.insert_child(first, "Detail").unwrap();
            sheet.insert_child(sheet.root, "Closing").unwrap();
            sheet.presentation =
                thinoda_io::generate_presentation(&sheet, &Default::default()).unwrap();
            let doc = app.doc_mut().unwrap();
            doc.doc.replace(Workbook::new(sheet));
            doc.doc.mark_saved();
            doc.view.reset();
            let before = doc.doc.workbook().clone();
            let mut h = Harness::builder()
                .with_size(egui::vec2(360.0, 640.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            let ctx = h.ctx.clone();
            ctx.set_embed_viewports(true);
            h.state_mut().exec(&ctx, Cmd::Pitch);
            h.run_steps(4);
            for label in ["Last Slide", "Build topics", "Reduce motion"] {
                let control = h.get_by_label(t(label));
                assert!(
                    h.ctx.content_rect().contains_rect(control.rect()),
                    "{language:?}/{theme:?}: {label}"
                );
            }
            h.get_by_label(t("Build topics")).click();
            h.run_steps(2);
            h.get_by_label(t("Last Slide")).click();
            h.run_steps(2);
            h.get_by_label(t("First Slide")).click();
            h.run_steps(2);
            h.get_by_label(t("Transition")).click();
            h.run_steps(2);
            h.get_by_label(t("Fade")).click();
            h.run_steps(2);
            h.get_by_label(t("Reduce motion")).click();
            h.run_steps(2);
            h.get_by_label(t("Next Slide")).click();
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            assert!(!h.state().doc().unwrap().doc.is_dirty());
            h.get_by_label(t("Exit Pitch")).click();
            h.run_steps(2);
            assert!(h.query_all_by_label(t("Build topics")).next().is_none());
            assert_eq!(h.state().docs.len(), 1);
        }
    }
}
