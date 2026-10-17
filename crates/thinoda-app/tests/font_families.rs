use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::ThinodaApp;
use thinoda_io::{Format, export_bytes, import_bytes};

#[test]
fn global_and_topic_font_controls_change_measured_layout_with_undo_and_roundtrips() {
    let mut app = ThinodaApp::headless();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    d.doc
        .edit_sheet("Title", |sheet| {
            sheet.set_title(root, "iiiiiiiiiiiiiiiiiiii")
        })
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let initial = {
        let d = h.state_mut().doc_mut().unwrap();
        d.view.layout_for(&d.doc).topics[&root].title_rect.size.x
    };
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(2);
    h.get_by_label("Global Font").scroll_to_me();
    h.run_steps(6);
    h.get_by_label("Global Font").click();
    h.run_steps(2);
    h.get_by_label("Monospace").click();
    h.run_steps(3);
    let d = h.state_mut().doc_mut().unwrap();
    assert_eq!(
        d.doc.sheet().theme.font_family.as_deref(),
        Some("monospace")
    );
    let mono = d.view.layout_for(&d.doc).topics[&root].title_rect.size.x;
    assert!(mono > initial * 1.5, "{initial} -> {mono}");
    assert!(d.doc.undo());
    assert_eq!(d.doc.workbook(), &before);
    assert!(d.doc.redo());
    h.get_by_role_and_label(Role::Tab, "Style").click();
    h.run_steps(2);
    h.get_by_label("Font Family").scroll_to_me();
    h.run_steps(6);
    h.get_by_label("Font Family").click();
    h.run_steps(2);
    h.get_by_label("Monospace").click();
    h.run_steps(2);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(root)
            .unwrap()
            .style
            .font_family
            .as_deref(),
        Some("monospace")
    );
    let book = h.state().doc().unwrap().doc.workbook().clone();
    for format in [Format::Thinoda, Format::XMind] {
        let bytes = export_bytes(&book, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &bytes).unwrap(), book);
    }
    h.get_by_label("Font Family").click();
    h.run_steps(2);
    h.get_by_label("Auto").click();
    h.run_steps(2);
    let d = h.state_mut().doc_mut().unwrap();
    assert_eq!(d.doc.sheet().topic(root).unwrap().style.font_family, None);
    assert_eq!(
        d.view.layout_for(&d.doc).topics[&root].title_rect.size.x,
        mono
    );
}

#[test]
fn unavailable_fonts_warn_without_changing_document_or_requested_name() {
    let mut app = ThinodaApp::headless();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Missing font", |sheet| {
            sheet.theme.font_family = Some("Thinoda Absent Font Family".into());
            Ok(())
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    let message = "Font unavailable: Thinoda Absent Font Family. Using the default font; the requested name is preserved.";
    assert!(h.query_by_label(message).is_some());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.state_mut().doc_mut().unwrap().doc.mark_saved();
    h.run_steps(3);
    assert!(!h.state().doc().unwrap().doc.is_dirty());
    assert_eq!(h.query_all_by_label(message).count(), 1);
}
