use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::ThinodaApp;
use thinoda_io::{Format, export_bytes, import_bytes};

#[test]
fn flexible_floating_map_setting_is_undoable_per_sheet_and_roundtrips() {
    let mut app = ThinodaApp::headless();
    let d = app.doc_mut().unwrap();
    assert!(!d.doc.sheet().theme.flexible_floating_topics);
    let before = d.doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Switch, "Flexible Floating Topics")
        .scroll_to_me();
    h.run_steps(6);
    h.get_by_role_and_label(Role::Switch, "Flexible Floating Topics")
        .click();
    h.run_steps(2);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .theme
            .flexible_floating_topics
    );
    let after = h.state().doc().unwrap().doc.workbook().clone();
    for format in [Format::Thinoda, Format::XMind] {
        let bytes = export_bytes(&after, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &bytes).unwrap(), after);
    }
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &after);
    let d = h.state_mut().doc_mut().unwrap();
    let original = d.doc.active_index();
    d.doc
        .add_sheet(thinoda_core::Sheet::new("Other sheet", "Other center"))
        .unwrap();
    d.view.reset();
    h.run_steps(3);
    assert!(
        !h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .theme
            .flexible_floating_topics
    );
    h.state_mut().switch_sheet(original);
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .theme
            .flexible_floating_topics
    );
}
