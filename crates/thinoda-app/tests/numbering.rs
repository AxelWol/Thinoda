//! Topic numbering in the Outliner and format panel across locales and themes.

use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::numbering::{NumberFormat, Numbering};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn app(
    language: Language,
    theme: ThemeMode,
    panel: bool,
) -> (ThinodaApp, [thinoda_core::TopicId; 3]) {
    let mut app = ThinodaApp::new(
        Settings {
            language,
            theme,
            show_format_panel: panel,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    app.exec(&egui::Context::default(), Cmd::New);
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    let mut ids = [root; 3];
    doc.doc
        .edit_sheet("Fixture", |sheet| {
            ids[0] = sheet.add_child(root, None, "Alpha");
            ids[1] = sheet.add_child(root, None, "Beta");
            ids[2] = sheet.add_child(ids[1], None, "Gamma");
            Ok(())
        })
        .unwrap();
    (app, ids)
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h
}

#[test]
fn outliner_shows_derived_numbers_without_changing_titles_and_undo_restores() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let (mut app, ids) = app(language, theme, false);
            app.mode = CenterMode::Outliner;
            let doc = app.doc_mut().unwrap();
            doc.doc
                .edit_sheet("Numbering", |sheet| {
                    sheet.set_sibling_numbering(
                        ids[0],
                        Some(Numbering {
                            format: NumberFormat::UpperRoman,
                            ..Default::default()
                        }),
                    )?;
                    sheet.set_sibling_numbering(
                        ids[2],
                        Some(Numbering {
                            tiered: true,
                            ..Default::default()
                        }),
                    )
                })
                .unwrap();
            let labels = doc.doc.sheet().numbering_labels();
            let (alpha, beta, gamma) = (
                labels[&ids[0]].clone(),
                labels[&ids[1]].clone(),
                labels[&ids[2]].clone(),
            );
            assert!(alpha.chars().all(|c| "IVXLCDM".contains(c)), "{alpha}");
            assert_eq!(gamma, format!("{beta}.1"));
            let mut h = harness(app);
            for label in [&alpha, &beta, &gamma] {
                assert!(
                    h.query_by_label(label).is_some(),
                    "{language:?}/{theme:?}: missing {label}"
                );
            }
            let sheet = h.state().doc().unwrap().doc.sheet().clone();
            assert_eq!(sheet.topic(ids[0]).unwrap().title, "Alpha");
            assert_eq!(sheet.topic(ids[2]).unwrap().title, "Gamma");

            h.state_mut().doc_mut().unwrap().doc.undo();
            h.run_steps(3);
            assert!(h.query_by_label(&gamma).is_none(), "{language:?}: undo");
            assert!(h.query_by_label(&beta).is_none(), "{language:?}: undo");
        }
    }
}

#[test]
fn format_panel_offers_numbering_for_a_selected_subtopic() {
    for language in Language::ALL {
        let (mut app, ids) = app(language, ThemeMode::Dark, true);
        app.doc_mut().unwrap().view.select_only(ids[0]);
        let h = harness(app);
        assert!(
            h.query_all_by_label(t("Numbering")).next().is_some(),
            "{language:?}: numbering section"
        );
    }
}
