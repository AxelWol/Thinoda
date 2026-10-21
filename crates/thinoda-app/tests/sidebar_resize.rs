//! Resize regressions run sequentially because localization is process-wide.

use egui::{Event, PointerButton, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, FormatTab, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn resize(h: &mut Harness<'_, ThinodaApp>, width: f32) {
    let start = h.get_by_role(Role::Splitter).rect().center() + egui::vec2(2.0, 0.0);
    let end = egui::pos2(h.ctx.content_rect().right() - width, start.y);
    h.event(Event::PointerMoved(start));
    h.run_steps(2);
    assert!(matches!(
        h.output().platform_output.cursor_icon,
        egui::CursorIcon::ResizeHorizontal
            | egui::CursorIcon::ResizeWest
            | egui::CursorIcon::ResizeEast
    ));
    h.event(Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    h.run_steps(2);
    h.event(Event::PointerMoved(end));
    h.run_steps(3);
    assert_geometry(h);
    h.event(Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    h.run_steps(5);
    assert_geometry(h);
}

fn harness(settings: Settings) -> Harness<'static, ThinodaApp> {
    let scale = settings.ui_scale;
    let mut app = ThinodaApp::new(settings, Services::default(), &[]);
    app.exec(&egui::Context::default(), Cmd::New);
    app.format_tab = FormatTab::Map;
    Harness::builder()
        .with_size(egui::vec2(1280.0, 1000.0) * scale)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app)
}

fn assert_geometry(h: &mut Harness<'_, ThinodaApp>) {
    h.ctx.request_repaint();
    h.step();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let tolerance = 0.5 / h.ctx.pixels_per_point();
    let panel = egui::Rect::from_min_max(
        egui::pos2(canvas.right(), canvas.top()),
        egui::pos2(h.ctx.content_rect().right(), canvas.bottom()),
    );
    assert!(
        panel.left() >= panel.right() - 420.0 - tolerance,
        "{panel:?}"
    );
    let panel = panel.expand(tolerance);
    assert!(canvas.width() >= 200.0, "{canvas:?}");
    for tab in ["Style", "Map"] {
        let rect = h.get_by_role_and_label(Role::Tab, t(tab)).rect();
        assert!(
            panel.contains_rect(rect),
            "{tab}: {rect:?} outside {panel:?}"
        );
        assert_paint(h, t(tab), rect);
    }
    if h.state().format_tab == FormatTab::Map {
        for id in thinoda_core::theme::Theme::IDS {
            let label = t(thinoda_core::theme::Theme::builtin(id).name);
            let rect = h.get_by_role_and_label(Role::Button, label).rect();
            assert!(
                panel.left() <= rect.left() && rect.right() <= panel.right(),
                "{label}: {rect:?} outside {panel:?}"
            );
            assert_paint(h, label, rect);
        }
    }
}

fn assert_paint(h: &Harness<'_, ThinodaApp>, label: &str, card: egui::Rect) -> bool {
    // Inspect galley paint geometry as well as accessibility/hit rectangles.
    fn texts<'a>(shape: &'a egui::Shape, result: &mut Vec<&'a egui::epaint::TextShape>) {
        match shape {
            egui::Shape::Text(text) => result.push(text),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    texts(shape, result);
                }
            }
            _ => {}
        }
    }
    let mut found = false;
    for shape in &h.output().shapes {
        let mut painted_texts = Vec::new();
        texts(&shape.shape, &mut painted_texts);
        for text in painted_texts.into_iter().filter(|text| {
            text.galley.text() == label && (card.top()..card.bottom()).contains(&text.pos.y)
        }) {
            found = true;
            let painted = egui::Rect::from_min_size(text.pos, text.galley.size());
            assert!(
                card.contains_rect(painted),
                "{label}: {painted:?} outside {card:?}"
            );
            if shape.clip_rect.contains(text.pos) {
                assert!(
                    shape.clip_rect.left() <= painted.left()
                        && painted.right() <= shape.clip_rect.right(),
                    "{label}: {painted:?} clipped by {:?}",
                    shape.clip_rect
                );
            }
        }
    }
    let tabs = h.get_by_role_and_label(Role::Tab, t("Map")).rect();
    let style = h.get_by_role_and_label(Role::Tab, t("Style")).rect();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    if card == tabs
        || card == style
        || (card.top() > tabs.bottom() + 8.0 && card.bottom() < canvas.bottom())
    {
        assert!(found, "{label}: missing visible paint for {card:?}");
    }
    found
}

#[test]
fn map_inspector_reflows_after_resizing_and_restoring() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for scale in [1.0, 1.5, 2.0] {
                let settings = Settings {
                    language,
                    theme,
                    ui_scale: scale,
                    ..Default::default()
                };
                let mut h = harness(settings.clone());
                h.run_steps(5);
                let workbook = h.state().doc().unwrap().doc.workbook().clone();
                let selection = h.state().doc().unwrap().view.selection.clone();
                let revision = h.state().doc().unwrap().doc.state_id();
                for width in [420.0, 240.0, 360.0, 420.0] {
                    resize(&mut h, width);
                }
                let maximum = h.get_by_role(Role::Splitter).rect().center().x;
                assert!((h.ctx.content_rect().right() - maximum - 420.0).abs() <= 1.0);
                for tab in ["Style", "Map"] {
                    h.get_by_role_and_label(Role::Tab, t(tab)).click();
                    h.run_steps(5);
                    assert_geometry(&mut h);
                }
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::ToggleFormatPanel);
                h.run_steps(3);
                h.state_mut().exec(&ctx, Cmd::ToggleFormatPanel);
                h.run_steps(5);
                assert_geometry(&mut h);
                h.set_size(egui::vec2(720.0, 650.0) * scale);
                h.run_steps(8);
                assert_geometry(&mut h);
                for id in thinoda_core::theme::Theme::IDS {
                    let label = t(thinoda_core::theme::Theme::builtin(id).name);
                    h.get_by_role_and_label(Role::Button, label).scroll_to_me();
                    h.run_steps(8);
                    let rect = h.get_by_role_and_label(Role::Button, label).rect();
                    assert!(assert_paint(&h, label, rect), "{label}: missing card paint");
                }
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &workbook);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);

                // The native host restores this egui memory with the panel's persisted size.
                let memory = h.ctx.memory(Clone::clone);
                let mut restored = harness(settings);
                restored.ctx.memory_mut(|target| *target = memory);
                restored.run_steps(8);
                assert_geometry(&mut restored);
                let edge = restored.get_by_role(Role::Splitter).rect().center().x;
                assert!((restored.ctx.content_rect().right() - edge - 420.0).abs() <= 1.0);
            }
        }
    }

    let mut h = harness(Settings {
        language: Language::German,
        ..Default::default()
    });
    h.run_steps(5);
    resize(&mut h, 240.0);
    let before = h.state().doc().unwrap().doc.sheet().clone();
    for id in thinoda_core::theme::Theme::IDS {
        let label = t(thinoda_core::theme::Theme::builtin(id).name);
        h.get_by_role_and_label(Role::Button, label).scroll_to_me();
        h.run_steps(8);
        let node = h.get_by_role_and_label(Role::Button, label);
        let rect = node.rect();
        node.focus();
        h.key_press(egui::Key::Enter);
        h.run_steps(5);
        assert_eq!(h.state().doc().unwrap().doc.sheet().theme.theme_id, id);
        assert_paint(&h, label, rect);
    }
    let ctx = h.ctx.clone();
    while h.state().doc().unwrap().doc.sheet() != &before {
        assert!(h.state().doc().unwrap().doc.can_undo());
        h.state_mut()
            .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    }
}
