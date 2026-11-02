use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};
use thinoda_core::{PresentationSource, Rect, Sheet, SheetRootMode, Vec2, Workbook};
use thinoda_io::{Format, export_bytes, import_bytes};

#[test]
fn pitch_captures_current_view_in_sheet_coordinates_and_saves_adjusted_frame() {
    let mut app = ThinodaApp::headless();
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    sheet.add_floating("Floating root", Vec2::new(200.0, 100.0));
    let doc = app.doc_mut().unwrap();
    doc.doc.replace(Workbook::new(sheet));
    doc.doc.mark_saved();
    doc.view.reset();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 850.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(5);
    {
        let view = &mut h.state_mut().doc_mut().unwrap().view;
        view.zoom = 1.75;
        view.offset = egui::vec2(80.0, -50.0);
    }
    h.run_steps(2);
    let view = &h.state().doc().unwrap().view;
    let canvas = view.canvas_rect();
    let transform = thinoda_canvas::Xf {
        origin: canvas.center() + view.offset,
        zoom: view.zoom,
    };
    let min = transform.sheet(canvas.min);
    let expected = Rect::from_min_size(min, transform.sheet(canvas.max) - min);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditPresentation);
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, t("Capture"))
        .click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);

    h.get_by_label(t("Width")).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(t("Width")).click();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_by_label(t("Width")).type_text("210");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, t("Apply"))
        .click();
    h.run_steps(3);

    let workbook = h.state().doc().unwrap().doc.workbook().clone();
    let slide = &workbook.sheets[0].presentation.slides[0];
    assert_eq!(slide.source, PresentationSource::Viewport);
    assert!(!slide.auto_frame);
    assert_eq!(slide.frame.min, expected.min);
    assert_eq!(slide.frame.size.y, expected.size.y);
    assert_eq!(slide.frame.size.x, 210.0);
    let bytes = export_bytes(&workbook, Format::Thinoda, &Default::default()).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), workbook);
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}
