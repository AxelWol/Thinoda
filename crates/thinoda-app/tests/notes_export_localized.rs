//! Locale-changing UI tests live in their own process: the translator is process-global.

use egui::{Key, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::{Sheet, Workbook};
use thinoda_io::Format;
use thinoda_platform::settings::{Language, Settings};

const WARNING: &str = "This format exports formatted notes as Markdown source, not rich text. The complete source is retained, but formatting will not be displayed. The source workbook is unchanged. Save as Thinoda to preserve note formats.";

#[test]
fn note_warning_is_localized_cancelable_and_blocks_edits_in_every_language() {
    for language in Language::ALL {
        let mut app = ThinodaApp::new(
            Settings {
                language,
                ..Default::default()
            },
            Services::default(),
            &[],
        );
        let mut sheet = Sheet::new("Notes", "Research");
        sheet
            .set_markdown_notes(sheet.root, Some("**café**\n\n3. First\tTab".into()))
            .unwrap();
        let book = Workbook::new(sheet);
        app.new_doc(book.clone());
        let revision = app.doc().unwrap().doc.state_id();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        for format in [Format::Csv, Format::Opml] {
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Export(format));
            h.run_steps(3);
            assert_eq!(h.state().dialogs.note_export, Some((format, false)));
            assert!(h.query_by_label(t(WARNING)).is_some(), "{language:?}");
            h.key_press(Key::Insert);
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            h.query_all(By::new().role(Role::Button).label(t("Cancel")))
                .find(|node| !node.accesskit_node().is_disabled())
                .unwrap()
                .click();
            h.run_steps(3);
            assert!(h.state().dialogs.note_export.is_none());
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
        }
    }
}
