//! A separate executable isolates the production process-global locale from unit tests.
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, ThinodaApp},
    i18n::{self, t},
};
use thinoda_core::{PresentationScope, Rect, Sheet, Vec2, Workbook};
use thinoda_platform::settings::{Language, ThemeMode};

fn setup(language: Language, theme: ThemeMode) -> ThinodaApp {
    let mut app = ThinodaApp::headless();
    app.settings.language = language;
    app.settings.theme = theme;
    i18n::set_lang(i18n::resolve(language));
    let mut sheet = Sheet::new("Presentation", "Center");
    sheet.insert_child(sheet.root, "Opening").unwrap();
    sheet.insert_child(sheet.root, "Closing").unwrap();
    let floating = sheet.add_floating("Floating", Vec2::new(600.0, 200.0));
    sheet
        .create_zone(
            "Zone",
            vec![floating],
            Rect::from_min_size(Vec2::new(450.0, 50.0), Vec2::new(300.0, 300.0)),
        )
        .unwrap();
    let doc = app.doc_mut().unwrap();
    doc.doc.replace(Workbook::new(sheet));
    doc.doc.mark_saved();
    doc.view.reset();
    doc.view.select_only(doc.doc.sheet().root);
    app
}

#[test]
fn localized_editor_interactions_fit_narrow_and_wide_in_all_ten_languages_and_themes() {
    for language in Language::ALL
        .into_iter()
        .filter(|language| *language != Language::Auto)
    {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for size in [egui::vec2(360.0, 560.0), egui::vec2(1100.0, 900.0)] {
                let app = setup(language, theme);
                let before = app.doc().unwrap().doc.workbook().clone();
                let mut h = Harness::builder()
                    .with_size(size)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::EditPresentation);
                h.run_steps(20);
                let click = |h: &mut Harness<'_, ThinodaApp>, label: &'static str| {
                    h.query_all_by_label(t(label))
                        .find(|node| !node.accesskit_node().is_disabled())
                        .expect(label)
                        .scroll_to_me();
                    h.run_steps(20);
                    let node = h
                        .query_all_by_label(t(label))
                        .find(|node| !node.accesskit_node().is_disabled())
                        .expect(label);
                    assert!(
                        h.ctx.content_rect().contains_rect(node.rect()),
                        "{language:?} {theme:?} {size:?} {label}: {:?}",
                        node.rect()
                    );
                    node.click();
                    h.run_steps(3);
                };
                click(&mut h, "Generate Slides");
                click(&mut h, "Duplicate Slide");
                click(&mut h, "Auto-fit content");
                click(&mut h, "Map context");
                click(&mut h, "Topic visibility");
                h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Center")
                    .scroll_to_me();
                h.run_steps(20);
                h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Center")
                    .click();
                h.run_steps(3);
                h.get_by_label(t("Hidden topics"));
                click(&mut h, "Restore topic");
                assert!(h.query_all_by_label(t("Restore topic")).next().is_none());
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                click(&mut h, "Apply");
                let sheet = h.state().doc().unwrap().doc.sheet();
                assert_eq!(
                    sheet.presentation.slides.len(),
                    4,
                    "{language:?} {theme:?} {size:?}"
                );
                assert!(!sheet.presentation.slides[1].auto_frame);
                assert_eq!(
                    sheet.presentation.slides[1].scope,
                    PresentationScope::Context
                );
                assert!(sheet.presentation.slides[1].hidden_topics.is_empty());
                assert!(h.state_mut().doc_mut().unwrap().doc.undo());
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert!(!h.state_mut().doc_mut().unwrap().doc.undo());
                h.state_mut().exec(&ctx, Cmd::EditPresentation);
                h.run_steps(20);
                click(&mut h, "Generate Slides");
                click(&mut h, "Cancel");
                assert!(h.query_all_by_label(t("Slide title")).next().is_none());
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            }
        }
    }
}
