//! Toolbar popup localization is process-wide; keep the matrix sequential.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, EXPORT_MENU, Services, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::Action;
use thinoda_core::markers;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn popup_rects(h: &Harness<'_, ThinodaApp>) -> Vec<egui::Rect> {
    let visible = h.ctx.memory(|memory| memory.areas().visible_layer_ids());
    h.ctx.memory(|memory| {
        visible
            .iter()
            .filter(|layer| {
                layer.order == egui::Order::Foreground
                    && layer.id != egui::Id::unique("thinoda_editing_toolbar")
                    && layer.id != egui::Id::unique("thinoda_topic_toolbar")
            })
            .filter_map(|layer| memory.area_rect(layer.id))
            .collect()
    })
}

fn map_toolbars_stay_below_auxiliary_windows() {
    for name in ["thinoda_editing_toolbar", "thinoda_topic_toolbar"] {
        let toolbar = egui::Id::unique(name);
        let window = egui::Id::unique(("auxiliary-feature", name));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 900.0))
            .build_ui_state(
                move |ui, app: &mut ThinodaApp| {
                    app.draw(ui);
                    if let Some(rect) = ui.ctx().memory(|memory| memory.area_rect(toolbar)) {
                        egui::Window::new("Auxiliary feature window")
                            .id(window)
                            .fixed_rect(rect.expand(16.0))
                            .show(ui.ctx(), |ui| {
                                ui.label("Window controls");
                            });
                    }
                },
                ThinodaApp::headless(),
            );
        h.run_steps(5);
        let rect = h
            .ctx
            .memory(|memory| memory.area_rect(toolbar))
            .expect(name);
        assert_eq!(
            h.ctx.layer_id_at(rect.center()).unwrap().id,
            window,
            "{name} must not intercept an overlapping feature window"
        );
    }
}

fn assert_fit(h: &Harness<'_, ThinodaApp>, label: &str) {
    // Constrained origins and frame extents can round independently by one pixel.
    let viewport = h.ctx.content_rect().expand(1.0 / h.ctx.pixels_per_point());
    let popups = popup_rects(h);
    assert!(!popups.is_empty(), "No visible popup to check");
    for rect in popups {
        assert!(
            viewport.contains_rect(rect),
            "{label}: {rect:?} outside {viewport:?}"
        );
    }
}

fn open(h: &mut Harness<'_, ThinodaApp>, label: &'static str) {
    h.get_by_role_and_label(Role::Button, t(label))
        .scroll_to_me();
    h.run_steps(8);
    h.get_by_role_and_label(Role::Button, t(label)).click();
    h.run_steps(4);
    assert_fit(h, label);
}

fn reveal(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(8);
    let rect = h.get_by_label(label).rect();
    assert!(
        h.ctx.content_rect().contains_rect(rect),
        "{label}: {rect:?}"
    );
    assert_fit(h, label);
}

fn dismiss(h: &mut Harness<'_, ThinodaApp>) {
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert!(!egui::Popup::is_any_open(&h.ctx));
}

#[test]
fn toolbar_menus_remain_bounded_and_operable_in_short_windows() {
    map_toolbars_stay_below_auxiliary_windows();
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (height, scale) in [(240.0, 1.0), (360.0, 1.5), (480.0, 2.0), (800.0, 1.0)] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: scale,
                        show_format_panel: false,
                        show_topic_toolbar: false,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                let mut h = Harness::builder()
                    .with_size(egui::vec2(1280.0, height))
                    .with_context_setup(|ctx| {
                        ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    })
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.run_steps(4);
                let before = h.state().doc().unwrap().doc.workbook().clone();
                let selection = h.state().doc().unwrap().view.selection.clone();

                open(&mut h, "Export");
                reveal(&mut h, t(EXPORT_MENU.last().unwrap().label()));
                reveal(&mut h, t(EXPORT_MENU.first().unwrap().label()));
                dismiss(&mut h);

                open(&mut h, "More insert actions");
                assert!(popup_rects(&h).iter().any(|rect| rect.width() >= 190.0));
                reveal(&mut h, t("Find and Replace…"));
                h.get_by_role_and_label(Role::Button, t("Find and Replace…"))
                    .focus();
                h.run_steps(4);
                assert_fit(&h, "More insert actions focus");
                reveal(&mut h, t("Notes"));
                dismiss(&mut h);

                open(&mut h, "Marker");
                let first = markers::GROUPS.first().unwrap().markers.first().unwrap();
                let last = markers::GROUPS.last().unwrap().markers.last().unwrap();
                reveal(&mut h, t(first.name));
                assert!(popup_rects(&h).iter().all(|rect| rect.width() <= 350.0));
                reveal(&mut h, t(last.name));
                h.get_by_role_and_label(Role::Button, t(last.name)).click();
                h.run_steps(4);
                let doc = h.state().doc().unwrap();
                assert!(
                    doc.doc
                        .sheet()
                        .topic(doc.doc.sheet().root)
                        .unwrap()
                        .markers
                        .iter()
                        .any(|marker| marker == last.id)
                );
                if egui::Popup::is_any_open(&h.ctx) {
                    dismiss(&mut h);
                }
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
                h.run_steps(4);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);

                open(&mut h, "Zoom");
                reveal(&mut h, "500%");
                reveal(&mut h, "10%");
                h.get_by_label("10%").focus();
                h.key_press(egui::Key::Enter);
                h.run_steps(4);
                assert_eq!(h.state().doc().unwrap().view.zoom_percent(), 10);
                assert_eq!(h.state().settings.ui_scale, scale);

                open(&mut h, "Statistics");
                reveal(
                    &mut h,
                    t("Words split at whitespace; characters count Unicode scalar values."),
                );
                reveal(&mut h, t("Current sheet"));
                dismiss(&mut h);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
            }
        }
    }
}
