//! Locale selection is process-wide; run the viewport matrix sequentially.

use egui::{Key, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn assert_fit(h: &Harness<'_, ThinodaApp>) {
    let viewport = h.ctx.content_rect().expand(1.0 / h.ctx.pixels_per_point());
    let rect = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("shortcuts")))
        .expect("shortcut dialog area");
    assert!(
        viewport.contains_rect(rect),
        "{rect:?} outside {viewport:?}"
    );
    let close = h.get_by_role_and_label(Role::Button, t("Close")).rect();
    assert!(
        h.ctx.content_rect().contains_rect(close),
        "Close outside viewport: {close:?}"
    );
    let popups = h.ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .iter()
            .filter(|layer| layer.order == egui::Order::Foreground)
            .filter_map(|layer| memory.area_rect(layer.id))
            .collect::<Vec<_>>()
    });
    for popup in popups {
        assert!(
            viewport.contains_rect(popup),
            "{popup:?} outside {viewport:?}"
        );
    }
    assert_paint(h, t("Close"), close);
}

fn assert_paint(h: &Harness<'_, ThinodaApp>, label: &str, target: egui::Rect) {
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
        let mut painted = Vec::new();
        texts(&shape.shape, &mut painted);
        for text in painted
            .into_iter()
            .filter(|text| text.galley.text() == label && target.contains(text.pos))
        {
            found = true;
            let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
            let tolerance = 1.0 / h.ctx.pixels_per_point();
            assert!(
                shape.clip_rect.expand(tolerance).contains_rect(bounds)
                    && h.ctx.content_rect().expand(tolerance).contains_rect(bounds),
                "{label}: {bounds:?} clipped by {:?}",
                shape.clip_rect
            );
        }
    }
    assert!(found, "{label}: missing paint");
}

fn reveal(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    h.get_by_role_and_label(Role::Label, label).scroll_to_me();
    h.run_steps(8);
    assert!(
        h.ctx
            .content_rect()
            .contains_rect(h.get_by_role_and_label(Role::Label, label).rect()),
        "{label}"
    );
    assert_fit(h);
    assert_paint(h, label, h.get_by_role_and_label(Role::Label, label).rect());
}

#[test]
fn shortcut_reference_remains_reachable_in_short_scaled_localized_windows() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (size, scale) in [
                (egui::vec2(640.0, 360.0), 1.0),
                (egui::vec2(960.0, 540.0), 1.5),
                (egui::vec2(1280.0, 480.0), 2.0),
                (egui::vec2(640.0, 360.0), 2.0),
            ] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: scale,
                        show_format_panel: false,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                let ctx = egui::Context::default();
                ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                app.exec(&ctx, Cmd::New);
                app.mode = CenterMode::Outliner;
                let mut h = Harness::builder()
                    .with_size(size)
                    .with_context_setup(|ctx| {
                        ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    })
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(4);
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::Shortcuts);
                h.run_steps(6);
                assert!((h.ctx.content_rect().height() - size.y / scale).abs() <= 1.0);
                let before = h.state().doc().unwrap().doc.workbook().clone();
                let selection = h.state().doc().unwrap().view.selection.clone();
                assert_fit(&h);
                for (role, label) in [
                    (Role::TextInput, "Search commands or shortcuts"),
                    (Role::ComboBox, "Shortcut category"),
                    (Role::CheckBox, "Unassigned only"),
                    (Role::Button, "Close"),
                ] {
                    h.key_press(Key::Tab);
                    h.run_steps(4);
                    assert!(
                        h.get_by_role_and_label(role, t(label)).is_focused(),
                        "{label}: incorrect Tab order"
                    );
                    assert_fit(&h);
                }
                h.key_press_modifiers(egui::Modifiers::SHIFT, Key::Tab);
                h.run_steps(4);
                assert!(
                    h.get_by_role_and_label(Role::CheckBox, t("Unassigned only"))
                        .is_focused()
                );
                reveal(&mut h, t("Central Topic"));
                reveal(&mut h, t("Navigate"));
                reveal(&mut h, t("New"));

                h.get_by_label(t("Shortcut category")).scroll_to_me();
                h.run_steps(8);
                h.get_by_label(t("Shortcut category")).click();
                h.run_steps(4);
                h.get_by_role_and_label(Role::Button, t("Insert"))
                    .scroll_to_me();
                h.run_steps(8);
                assert_fit(&h);
                h.get_by_role_and_label(Role::Button, t("Insert")).click();
                h.run_steps(4);
                assert_eq!(
                    h.state().dialogs.shortcut_browser.category.as_deref(),
                    Some(t("Insert"))
                );
                h.get_by_role_and_label(Role::CheckBox, t("Unassigned only"))
                    .scroll_to_me();
                h.run_steps(8);
                h.get_by_role_and_label(Role::CheckBox, t("Unassigned only"))
                    .click();
                h.run_steps(4);
                reveal(&mut h, t("To-Do"));
                assert!(
                    h.query_by_role_and_label(Role::Label, t("Subtopic"))
                        .is_none()
                );
                h.get_by_label(t("Search commands or shortcuts")).focus();
                h.run_steps(8);
                assert!(
                    h.ctx
                        .content_rect()
                        .contains_rect(h.get_by_label(t("Search commands or shortcuts")).rect())
                );
                h.get_by_label(t("Search commands or shortcuts")).click();
                h.get_by_label(t("Search commands or shortcuts"))
                    .type_text("does-not-match");
                h.run_steps(4);
                reveal(&mut h, t("No matching commands"));
                h.get_by_role_and_label(Role::Button, t("Close")).click();
                h.run_steps(3);
                assert!(!h.state().dialogs.shortcuts);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
                assert_eq!(h.state().mode, CenterMode::Outliner);

                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::Shortcuts);
                h.run_steps(4);
                assert_fit(&h);
                h.set_size(egui::vec2(1280.0, 800.0));
                h.run_steps(5);
                assert_fit(&h);
                h.set_size(size);
                h.run_steps(5);
                assert_fit(&h);
                h.key_press(Key::Escape);
                h.run_steps(3);
                assert!(!h.state().dialogs.shortcuts);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
            }
        }
    }
}
