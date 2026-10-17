use crate::{EguiMeasure, MapCanvas, MapView};
use egui_kittest::Harness;
use thinoda_core::{Document, Sheet, TextMeasure, Workbook};

#[test]
fn named_family_is_used_for_title_measurement_paint_and_inline_editing() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Fonts", "iiiiiiiiiiiiiiiiiiii")));
    doc.edit_sheet("Family", |sheet| {
        sheet.theme.font_family = Some("Late font".into());
        Ok(())
    })
    .unwrap();
    let root = doc.sheet().root;
    let mut h = Harness::new_ui_state(
        |ui, state: &mut (Document, MapView)| {
            MapCanvas::show(ui, &mut state.0, &mut state.1);
        },
        (doc, MapView::default()),
    );
    h.run_steps(3);
    let initial = {
        let (doc, view) = h.state_mut();
        view.layout_for(doc).topics[&root].title_rect.size.x
    };
    let mut defs = egui::FontDefinitions::default();
    defs.font_data
        .insert("late font data".into(), defs.font_data["Hack"].clone());
    let family = egui::FontFamily::Name("Late font".into());
    defs.families
        .insert(family.clone(), vec!["late font data".into()]);
    h.ctx.set_fonts(defs);
    h.run_steps(3);
    let layout = {
        let (doc, view) = h.state_mut();
        view.layout_for(doc)
    };
    let topic = &layout.topics[&root];
    assert!(topic.title_rect.size.x > initial * 1.5);
    let expected = EguiMeasure::new(&h.ctx).measure_with_family(
        "iiiiiiiiiiiiiiiiiiii",
        topic.style.font_size,
        topic.style.bold,
        400.0,
        Some("Late font"),
    );
    assert_eq!(topic.title_rect.size, expected);
    fn uses_family(shape: &egui::Shape, family: &egui::FontFamily) -> bool {
        match shape {
            egui::Shape::Text(text) => text
                .galley
                .job
                .sections
                .iter()
                .any(|section| &section.format.font_id.family == family),
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| uses_family(shape, family)),
            _ => false,
        }
    }
    assert!(
        h.output()
            .shapes
            .iter()
            .any(|shape| uses_family(&shape.shape, &family))
    );
    let (doc, view) = h.state_mut();
    view.begin_edit(doc.sheet(), root, false);
    h.run_steps(3);
    assert!(
        h.output()
            .shapes
            .iter()
            .any(|shape| uses_family(&shape.shape, &family))
    );
    assert_eq!(
        h.state().0.sheet().theme.font_family.as_deref(),
        Some("Late font")
    );
    h.state_mut().1.cancel_edit();
    let mut defs = h.ctx.fonts(|fonts| fonts.definitions().clone());
    defs.font_data.insert(
        "late font data".into(),
        defs.font_data["Ubuntu-Light"].clone(),
    );
    h.ctx.set_fonts(defs);
    h.run_steps(3);
    let (doc, view) = h.state_mut();
    assert_eq!(
        view.layout_for(doc).topics[&root].title_rect.size.x,
        initial
    );
}
