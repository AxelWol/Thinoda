use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::Vec2;

struct Preview {
    sheet: Sheet,
    layout: thinoda_core::Layout,
    hit: Option<TopicId>,
}

fn harness(dark: bool) -> Harness<'static, Preview> {
    let mut sheet = Sheet::new("Preview", "Center");
    let branch = sheet.insert_child(sheet.root, "Branch").unwrap();
    sheet.insert_child(branch, "Detail").unwrap();
    sheet.insert_child(sheet.root, "Excluded sibling").unwrap();
    sheet
        .add_presentation_slide(
            "Slide",
            PresentationSource::Branch(branch),
            Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 100.0)),
        )
        .unwrap();
    let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, dark);
    Harness::builder()
        .with_size(egui::vec2(360.0, 300.0))
        .build_ui_state(
            move |ui, state: &mut Preview| {
                ui.ctx().set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                if let Some(hit) =
                    SlidePreview.show(ui, &state.sheet, &state.layout, 0, egui::vec2(300.0, 200.0))
                {
                    state.hit = Some(hit);
                }
            },
            Preview {
                sheet,
                layout,
                hit: None,
            },
        )
}

#[test]
fn vector_preview_is_fitted_and_hit_tests_only_resolved_topics_in_both_themes() {
    for dark in [false, true] {
        let mut h = harness(dark);
        h.run_steps(2);
        let before = h.state().sheet.clone();
        let rect = h.get_by_label("Slide preview").rect();
        assert!(h.ctx.content_rect().contains_rect(rect));
        assert_eq!(rect.size(), egui::vec2(300.0, 200.0));
        let state = h.state();
        let resolved = thinoda_core::resolve_presentation_slide(
            &state.sheet,
            &state.sheet.presentation.slides[0],
            &state.layout,
        )
        .unwrap();
        assert_eq!(resolved.layout.order.len(), 2);
        let inner = rect.shrink(12.0);
        let zoom =
            (inner.width() / resolved.frame.size.x).min(inner.height() / resolved.frame.size.y);
        let center = resolved.frame.center();
        let xf = thinoda_canvas::Xf {
            origin: inner.center() - egui::vec2(center.x, center.y) * zoom,
            zoom,
        };
        let topic = resolved.layout.order[0];
        let pos = xf.rect(resolved.layout.rect(topic).unwrap()).center();
        h.event(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(2);
        }
        assert_eq!(h.state().hit, Some(topic));
        assert_eq!(h.state().sheet, before);
    }
}

#[test]
fn missing_preview_source_is_explicit_and_never_falls_back_to_full_map() {
    let mut h = harness(false);
    h.state_mut().sheet.presentation.slides[0].source =
        PresentationSource::Branch(TopicId(u64::MAX));
    h.run_steps(2);
    h.get_by_label("Slide source is unavailable.");
    assert!(h.state().hit.is_none());
}
