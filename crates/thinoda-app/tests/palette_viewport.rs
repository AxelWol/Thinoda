//! Palette localization checks run sequentially in one integration test.

use egui::{Key, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn exec(h: &mut Harness<'_, ThinodaApp>, command: Cmd) {
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, command);
    h.run_steps(4);
}

fn assert_fit(h: &Harness<'_, ThinodaApp>) {
    let viewport = h.ctx.content_rect().expand(1.0 / h.ctx.pixels_per_point());
    let rect = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_command_palette")))
        .expect("palette area");
    assert!(
        viewport.contains_rect(rect),
        "{rect:?} outside {viewport:?}"
    );
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

#[test]
fn palette_is_modal_bounded_and_keyboard_results_remain_reachable() {
    for (mode, draft) in [
        (CenterMode::Map, false),
        (CenterMode::Outliner, false),
        (CenterMode::Map, true),
        (CenterMode::Outliner, true),
    ] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 700.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        if draft {
            let doc = h.state_mut().doc_mut().unwrap();
            doc.view
                .begin_edit(doc.doc.sheet(), doc.doc.sheet().root, false);
            doc.view.set_editing_text("Uncommitted title draft");
            h.run_steps(4);
        }
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let selection = h.state().doc().unwrap().view.selection.clone();
        let editing = h
            .state()
            .doc()
            .unwrap()
            .view
            .editing_text()
            .map(str::to_owned);
        exec(&mut h, Cmd::Palette);
        assert!(
            h.get_by_label(thinoda_ui::icons::MENU)
                .accesskit_node()
                .is_disabled()
        );
        h.state_mut().palette.set_query(t("Save"));
        h.run_steps(4);
        h.key_press(Key::F2);
        h.key_press(Key::Delete);
        h.run_steps(4);
        h.key_press(Key::Escape);
        h.run_steps(4);
        assert!(!h.state().palette.open, "Escape must close the palette");
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert_eq!(h.state().doc().unwrap().view.selection, selection);
        assert_eq!(
            h.state().doc().unwrap().view.editing_text(),
            editing.as_deref()
        );
        assert_eq!(h.state().mode, mode);
        exec(&mut h, Cmd::Shortcuts);
        h.key_press_modifiers(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            Key::P,
        );
        h.run_steps(4);
        assert!(!h.state().palette.open, "another modal owns the shortcut");
        assert!(h.state().dialogs.shortcuts);
        assert!(!h.state().command_available(&Cmd::Palette));
        exec(&mut h, Cmd::Palette);
        assert!(
            !h.state().palette.open,
            "direct commands cannot stack the palette"
        );
        assert!(h.state().dialogs.shortcuts);
        h.get_by_label(thinoda_ui::icons::MENU).click();
        h.run_steps(4);
        assert!(
            !egui::Popup::is_any_open(&h.ctx),
            "modal blocks background menu clicks"
        );
        assert!(
            !h.state().dialogs.shortcuts,
            "the backdrop dismisses only the modal"
        );
        assert!(!h.state().palette.open);
        for about in [false, true] {
            h.state_mut().dialogs.settings = !about;
            h.state_mut().dialogs.about = about;
            assert!(!h.state().command_available(&Cmd::Palette));
            exec(&mut h, Cmd::Palette);
            assert!(!h.state().palette.open);
            h.state_mut().dialogs.settings = false;
            h.state_mut().dialogs.about = false;
        }
    }

    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (size, scale) in [
                (egui::vec2(640.0, 360.0), 1.0),
                (egui::vec2(960.0, 540.0), 1.5),
                (egui::vec2(1280.0, 480.0), 2.0),
                (egui::vec2(640.0, 360.0), 2.0),
            ] {
                let app = ThinodaApp::new(
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
                let mut h = Harness::builder()
                    .with_size(size)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(4);
                h.set_size(size / scale);
                h.run_steps(4);
                for mode in [None, Some(CenterMode::Map), Some(CenterMode::Outliner)] {
                    if let Some(mode) = mode {
                        if h.state().docs.is_empty() {
                            exec(&mut h, Cmd::New);
                        }
                        h.state_mut().mode = mode;
                    }
                    let before = h.state().doc().map(|doc| {
                        (
                            doc.doc.workbook().clone(),
                            doc.doc.state_id(),
                            doc.view.selection.clone(),
                        )
                    });
                    exec(&mut h, Cmd::Palette);
                    assert_fit(&h);
                    assert!(
                        (h.ctx.content_rect().height() - size.y / scale).abs() <= 1.0,
                        "{size:?}/{scale}, actual {:?}, zoom {}",
                        h.ctx.content_rect(),
                        h.ctx.zoom_factor()
                    );
                    let commands: Vec<_> = ThinodaApp::palette_commands()
                        .into_iter()
                        .filter(|(command, _)| h.state().command_available(command))
                        .map(|(_, command)| command)
                        .collect();
                    let matches = h.state().palette.filter(&commands);
                    for command in [
                        matches.first().unwrap(),
                        matches.iter().take(12).next_back().unwrap(),
                    ] {
                        let label = format!(
                            "{} {}",
                            command.title,
                            command.shortcut.as_deref().unwrap_or_default()
                        );
                        h.get_by_role_and_label(Role::Button, &label).scroll_to_me();
                        h.run_steps(8);
                        assert!(
                            h.ctx.content_rect().contains_rect(
                                h.get_by_role_and_label(Role::Button, &label).rect()
                            ),
                            "{label}"
                        );
                        assert_fit(&h);
                        assert_paint(
                            &h,
                            &command.title,
                            h.get_by_role_and_label(Role::Button, &label).rect(),
                        );
                    }
                    h.key_press(Key::Escape);
                    h.run_steps(4);
                    assert!(!h.state().palette.open);
                    assert_eq!(
                        h.state().doc().map(|doc| (
                            doc.doc.workbook().clone(),
                            doc.doc.state_id(),
                            doc.view.selection.clone()
                        )),
                        before
                    );
                }
            }
        }
    }
}
