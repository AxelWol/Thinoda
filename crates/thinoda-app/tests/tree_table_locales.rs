use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{ThinodaApp, app::CenterMode, i18n::t};
use thinoda_core::Structure;
use thinoda_platform::settings::{Language, ThemeMode};

// Locale is process-wide; isolate the matrix from other UI test executables.
#[test]
fn tree_table_is_discoverable_narrow_localized_and_read_only_until_apply() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let mut app = ThinodaApp::headless();
            app.settings.language = language;
            app.settings.theme = theme;
            app.settings.show_format_panel = false;
            app.doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Structure", |sheet| {
                    sheet.structure = Structure::TreeTable;
                    Ok(())
                })
                .unwrap();
            let before = app.doc().unwrap().doc.workbook().clone();
            let root = app.doc().unwrap().doc.sheet().root;
            let title = app.doc().unwrap().doc.sheet().topics[&root].title.clone();
            let mut h = Harness::builder()
                .with_size(egui::vec2(360.0, 640.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            h.run_steps(3);
            let label = format!("{}: {title}", t("Topic title"));
            let cell = h.get_by_label(&label);
            assert!(
                h.ctx.content_rect().contains_rect(cell.rect()),
                "{language:?}/{theme:?}"
            );
            cell.click();
            h.run_steps(2);
            h.get_by_role_and_label(egui::Role::TextInput, &label)
                .type_text(" draft");
            h.run_steps(2);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            h.key_press(egui::Key::Escape);
            h.run_steps(2);
            h.get_by_label(t("Tree Table help")).click();
            h.run_steps(2);
            h.get_by_label(t("Tree Table help")).click();
            h.run_steps(2);
            h.get_by_role_and_label(egui::Role::Button, t("Map"))
                .click();
            h.run_steps(2);
            h.get_by_role_and_label(egui::Role::Button, t("Tree Table"))
                .click();
            h.run_steps(3);
            assert_eq!(h.state().mode, CenterMode::Map);
            assert_eq!(h.state().doc().unwrap().view.primary(), Some(root));
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }
    }
}
