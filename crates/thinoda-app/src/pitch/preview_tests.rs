use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::Vec2;

fn sheet() -> Sheet {
    let mut sheet = Sheet::new("Preview", "Center");
    let branch = sheet.insert_child(sheet.root, "Branch").unwrap();
    sheet
        .add_presentation_slide(
            "Slide",
            PresentationSource::Branch(branch),
            Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 100.0)),
        )
        .unwrap();
    sheet
}

#[test]
fn preview_accepts_only_the_current_generation_and_exposes_worker_errors() {
    for stale in [false, true] {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(Err("Deliberate renderer failure".to_string()))
            .unwrap();
        let preview = SlidePreview {
            pending: Some(((0, 0, false), receiver)),
            rendered: None,
        };
        let sheet = sheet();
        let before = sheet.clone();
        let mut h = Harness::builder()
            .with_size(egui::vec2(360.0, 300.0))
            .build_ui_state(
                move |ui, preview: &mut SlidePreview| {
                    preview.show(
                        ui,
                        &sheet,
                        0,
                        u64::from(stale),
                        &ExportOptions::default(),
                        egui::vec2(300.0, 200.0),
                    );
                    assert_eq!(sheet, before);
                },
                preview,
            );
        h.run_steps(1);
        if stale {
            assert!(
                h.state()
                    .rendered
                    .as_ref()
                    .is_none_or(|(key, _)| key.0 == 1)
            );
            assert!(h.state().pending.as_ref().is_none_or(|(key, _)| key.0 == 1));
        } else {
            assert!(
                matches!(&h.state().rendered, Some(((0, 0, false), Err(error))) if error == "Deliberate renderer failure")
            );
            assert!(h.state().pending.is_none());
        }
    }
}

#[test]
fn decoded_preview_is_fitted_inside_the_requested_region() {
    let (sender, receiver) = mpsc::channel();
    sender
        .send(Ok(egui::ColorImage::filled(
            [200, 100],
            egui::Color32::WHITE,
        )))
        .unwrap();
    let preview = SlidePreview {
        pending: Some(((0, 0, false), receiver)),
        rendered: None,
    };
    let sheet = sheet();
    let mut h = Harness::builder()
        .with_size(egui::vec2(360.0, 300.0))
        .build_ui_state(
            move |ui, preview: &mut SlidePreview| {
                preview.show(
                    ui,
                    &sheet,
                    0,
                    0,
                    &ExportOptions::default(),
                    egui::vec2(300.0, 200.0),
                );
            },
            preview,
        );
    h.run_steps(2);
    assert!(matches!(h.state().rendered, Some((_, Ok(_)))));
    assert!(h.state().pending.is_none());
    let rect = h.get_by_label("Slide preview").rect();
    assert!(h.ctx.content_rect().contains_rect(rect));
    assert!(rect.width() <= 300.0 && rect.height() <= 200.0);
    assert!((rect.width() / rect.height() - 2.0).abs() < 0.01);
}
