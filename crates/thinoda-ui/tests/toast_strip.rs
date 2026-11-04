use egui_kittest::{Harness, kittest::Queryable};
use thinoda_ui::{ToastKind, Toasts, icons};

#[test]
fn bottom_strip_bounds_and_scrolls_stacked_warnings_without_losing_dismissal() {
    for size in [egui::vec2(240.0, 200.0), egui::vec2(320.0, 240.0)] {
        let mut toasts = Toasts::default();
        for index in 0..3 {
            toasts.sticky(
                ToastKind::Warning,
                format!(
                    "Warning {index}: Keep the original data and save the current document before retrying."
                ),
            );
        }
        let mut h = Harness::builder()
            .with_size(size)
            .with_context_setup(|ctx| {
                ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
                thinoda_ui::fonts::install_fallbacks(ctx);
            })
            .build_ui_state(
                |ui, toasts: &mut Toasts| {
                    if let Some(strip) = toasts.bottom_strip(ui.ctx()) {
                        toasts.show_in_bottom_strip(ui.ctx(), strip);
                    }
                },
                toasts,
            );
        h.run_steps(8);
        for index in (0..3).rev() {
            let strip = h.state().bottom_strip(&h.ctx).unwrap();
            let area = h
                .ctx
                .memory(|memory| memory.area_rect(egui::Id::unique(("thinoda_toasts", false))))
                .expect("warning strip");
            assert!(strip.expand(1.0).contains_rect(area), "{size:?}: {area:?}");
            h.query_all_by_label(icons::CLOSE)
                .nth(index)
                .expect("dismiss control")
                .focus();
            h.run_steps(8);
            let dismiss = h.query_all_by_label(icons::CLOSE).nth(index).unwrap();
            assert!(
                strip.contains_rect(dismiss.rect()),
                "{size:?}: {:?} outside {strip:?}",
                dismiss.rect()
            );
            dismiss.click();
            h.run_steps(4);
            assert_eq!(h.state().len(), index);
        }
        assert!(h.state().bottom_strip(&h.ctx).is_none());
    }
}

#[test]
fn keyboard_tab_reveals_and_dismisses_stacked_warnings() {
    let mut toasts = Toasts::default();
    for index in 0..3 {
        toasts.sticky(
            ToastKind::Warning,
            format!("Warning {index}: Keep the original data and save the current document."),
        );
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(240.0, 200.0))
        .with_context_setup(|ctx| {
            thinoda_ui::fonts::install_fallbacks(ctx);
        })
        .build_ui_state(
            |ui, toasts: &mut Toasts| {
                if let Some(strip) = toasts.bottom_strip(ui.ctx()) {
                    toasts.show_in_bottom_strip(ui.ctx(), strip);
                }
            },
            toasts,
        );
    h.run_steps(8);
    h.query_all_by_label(icons::CLOSE).next().unwrap().focus();
    h.run_steps(4);
    for index in 1..3 {
        h.key_press(egui::Key::Tab);
        h.run_steps(8);
        let dismiss = h.query_all_by_label(icons::CLOSE).nth(index).unwrap();
        assert!(dismiss.is_focused(), "Tab did not reach warning {index}");
        assert!(
            h.state()
                .bottom_strip(&h.ctx)
                .unwrap()
                .contains_rect(dismiss.rect()),
            "focused warning {index} is not visible: {:?}",
            dismiss.rect()
        );
    }
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().len(), 2);
}
