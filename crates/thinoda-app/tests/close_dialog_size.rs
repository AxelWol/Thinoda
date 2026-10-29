use egui::{Id, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use thinoda_app::{
    app::{CloseIntent, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings};

#[test]
fn unsaved_exit_confirmation_is_compact_and_keeps_actions_visible() {
    for language in Language::ALL {
        for size in [egui::vec2(1280.0, 900.0), egui::vec2(480.0, 360.0)] {
            let mut app = ThinodaApp::new(
                Settings {
                    language,
                    ..Default::default()
                },
                Services::default(),
                &[],
            );
            app.new_doc(thinoda_core::Workbook::default());
            let root = app.doc().unwrap().doc.sheet().root;
            app.doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Unsaved work", |sheet| {
                    sheet.set_title(root, "Keep this work")
                })
                .unwrap();
            let before = app.doc().unwrap().doc.workbook().clone();
            app.dialogs.close = Some(CloseIntent::Quit);
            let mut h = Harness::builder()
                .with_size(size)
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(6);
            let rect = h
                .ctx
                .memory(|memory| memory.area_rect(Id::unique("thinoda_close")))
                .unwrap();
            assert!(
                h.ctx.content_rect().expand(1.0).contains_rect(rect),
                "{language:?}: {rect:?}"
            );
            assert!(rect.width() <= 480.0, "{language:?}: {rect:?}");
            assert!(rect.height() <= 260.0, "{language:?}: {rect:?}");
            for label in ["Save", "Don't Save", "Cancel"] {
                let button = h
                    .query_all(By::new().role(Role::Button).label(t(label)))
                    .find(|button| rect.contains_rect(button.rect()))
                    .unwrap();
                assert!(rect.contains_rect(button.rect()), "{language:?}: {label}");
            }
            h.query_all(By::new().role(Role::Button).label(t("Cancel")))
                .find(|button| rect.contains_rect(button.rect()))
                .unwrap()
                .click();
            h.run_steps(3);
            assert!(h.state().dialogs.close.is_none());
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            assert!(h.state().doc().unwrap().is_dirty());
        }
    }
}
