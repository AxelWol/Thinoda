use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::Workbook;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

mod support;
use support::menu_action;

fn app(language: Language, theme: ThemeMode, scale: f32) -> ThinodaApp {
    let mut app = ThinodaApp::new(
        Settings {
            language,
            theme,
            ui_scale: scale,
            show_format_panel: false,
            show_topic_toolbar: false,
            show_topic_add_buttons: false,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    let mut book = Workbook::default();
    for _ in 1..20 {
        book.sheets.push(Workbook::default().sheets.remove(0));
    }
    for (index, sheet) in book.sheets.iter_mut().enumerate() {
        sheet.title = format!("Sheet {index}: a long sheet name");
    }
    app.new_doc(book);
    app
}

fn harness(
    size: egui::Vec2,
    scale: f32,
    language: Language,
    theme: ThemeMode,
) -> Harness<'static, ThinodaApp> {
    let mut h = Harness::builder().with_size(size * scale).build_ui_state(
        |ui, app: &mut ThinodaApp| app.draw(ui),
        app(language, theme, scale),
    );
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h.set_size(size);
    h.run_steps(4);
    assert!((h.ctx.content_rect().width() - size.x).abs() < 0.01);
    h
}

fn assert_visible(h: &Harness<'_, ThinodaApp>, label: &str) -> egui::Rect {
    let rect = h.get_by_role_and_label(Role::Button, label).rect();
    let tolerance = 1.0 / h.ctx.pixels_per_point();
    assert!(
        h.ctx.content_rect().expand(tolerance).contains_rect(rect),
        "{label}: {rect:?} outside {:?}",
        h.ctx.content_rect()
    );
    rect
}

fn assert_paint(h: &Harness<'_, ThinodaApp>, text: &str, target: egui::Rect) {
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
    let tolerance = 1.0 / h.ctx.pixels_per_point();
    let mut found = false;
    for shape in &h.output().shapes {
        let mut painted = Vec::new();
        texts(&shape.shape, &mut painted);
        for paint in painted
            .into_iter()
            .filter(|paint| paint.galley.text() == text && target.contains(paint.pos))
        {
            found = true;
            let bounds = egui::Rect::from_min_size(paint.pos, paint.galley.size());
            assert!(
                shape.clip_rect.expand(tolerance).contains_rect(bounds),
                "{text}: {bounds:?} clipped by {:?}",
                shape.clip_rect
            );
        }
    }
    assert!(found, "{text}: no paint in {target:?}");
}

fn assert_sheet_paint_is_clipped(h: &Harness<'_, ThinodaApp>, left: f32, right: f32) {
    fn contains_sheet(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.text().starts_with("Sheet "),
            egui::Shape::Vec(shapes) => shapes.iter().any(contains_sheet),
            _ => false,
        }
    }
    let tolerance = 1.0 / h.ctx.pixels_per_point();
    let mut found = false;
    for shape in &h.output().shapes {
        if contains_sheet(&shape.shape) {
            found = true;
            assert!(shape.clip_rect.left() >= left - tolerance);
            assert!(shape.clip_rect.right() <= right + tolerance);
        }
    }
    assert!(found, "Expected at least one painted sheet name");
}

#[test]
fn bottom_navigation_stays_reachable_without_overlapping_or_changing_sheet_content() {
    let mut h = harness(
        egui::vec2(320.0, 240.0),
        2.0,
        Language::English,
        ThemeMode::Light,
    );
    let before = h.state().doc().unwrap().doc.workbook().sheets.clone();
    let state = h.state().doc().unwrap().doc.state_id();
    let titles: Vec<_> = before.iter().map(|sheet| sheet.title.clone()).collect();
    for label in ["Scroll sheets left", "Scroll sheets right"] {
        assert_visible(&h, t(label));
    }
    assert!(
        h.get_by_role_and_label(Role::Button, t("Scroll sheets left"))
            .accesskit_node()
            .is_disabled()
    );
    let first_position = h.get_by_role_and_label(Role::Button, &titles[0]).rect();
    h.get_by_role_and_label(Role::Button, t("Scroll sheets right"))
        .click();
    h.run_steps(8);
    assert!(
        h.get_by_role_and_label(Role::Button, &titles[0])
            .rect()
            .left()
            < first_position.left()
    );
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 0);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
    h.get_by_role_and_label(Role::Button, t("Scroll sheets left"))
        .focus();
    h.key_press(Key::Enter);
    h.run_steps(8);
    assert_eq!(
        h.get_by_role_and_label(Role::Button, &titles[0]).rect(),
        first_position
    );
    for _ in 0..100 {
        let right = h.get_by_role_and_label(Role::Button, t("Scroll sheets right"));
        if right.accesskit_node().is_disabled() {
            break;
        }
        right.focus();
        h.key_press(Key::Space);
        h.run_steps(3);
    }
    assert!(
        h.get_by_role_and_label(Role::Button, t("Scroll sheets right"))
            .accesskit_node()
            .is_disabled()
    );
    let arrow_end = assert_visible(&h, &titles[19]);
    assert!(arrow_end.left() >= assert_visible(&h, t("Scroll sheets right")).right());
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 0);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets, before);
    for _ in 0..100 {
        let left = h.get_by_role_and_label(Role::Button, t("Scroll sheets left"));
        if left.accesskit_node().is_disabled() {
            break;
        }
        left.click();
        h.run_steps(3);
    }
    assert!(
        h.get_by_role_and_label(Role::Button, t("Scroll sheets left"))
            .accesskit_node()
            .is_disabled()
    );
    assert_visible(&h, &titles[0]);
    let swipe_start = h
        .get_by_role_and_label(Role::Button, &titles[0])
        .rect()
        .center();
    let before_swipe = h
        .get_by_role_and_label(Role::Button, &titles[0])
        .rect()
        .left();
    h.event(egui::Event::PointerMoved(swipe_start));
    h.event(egui::Event::PointerButton {
        pos: swipe_start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase: egui::TouchPhase::Start,
        pos: swipe_start,
        force: None,
    });
    h.run_steps(1);
    let swipe_middle = swipe_start - egui::vec2(12.0, 0.0);
    h.event(egui::Event::PointerMoved(swipe_middle));
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase: egui::TouchPhase::Move,
        pos: swipe_middle,
        force: None,
    });
    h.run_steps(1);
    let swipe_end = swipe_start - egui::vec2(60.0, 0.0);
    h.event(egui::Event::PointerMoved(swipe_end));
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase: egui::TouchPhase::Move,
        pos: swipe_end,
        force: None,
    });
    h.run_steps(2);
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase: egui::TouchPhase::End,
        pos: swipe_end,
        force: None,
    });
    h.event(egui::Event::PointerButton {
        pos: swipe_end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(8);
    assert!(
        h.get_by_role_and_label(Role::Button, &titles[0])
            .rect()
            .left()
            < before_swipe
    );
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 0);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets, before);
    h.get_by_role_and_label(Role::Button, &titles[0])
        .scroll_to_me();
    h.run_steps(8);
    let add = assert_visible(&h, t("New Sheet"));
    let right_arrow = assert_visible(&h, t("Scroll sheets right"));
    assert_sheet_paint_is_clipped(&h, right_arrow.right(), add.left());
    h.get_by_role_and_label(Role::Button, t("Zoom")).focus();
    h.run_steps(8);
    assert_visible(&h, t("Zoom"));
    let height =
        h.ctx.content_rect().bottom() - h.state().doc().unwrap().view.canvas_rect().bottom();

    h.get_by_role_and_label(Role::Button, &titles[19]).focus();
    h.run_steps(8);
    assert!(
        h.get_by_role_and_label(Role::Button, &titles[19])
            .is_focused()
    );
    let last = assert_visible(&h, &titles[19]);
    assert!(
        last.right() <= add.left() + 1.0 / h.ctx.pixels_per_point(),
        "last={last:?}, add={add:?}"
    );
    h.key_press(Key::Enter);
    h.run_steps(8);
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 19);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::PreviousSheet);
    h.run_steps(8);
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 18);
    assert_visible(&h, &titles[18]);

    // Stable focus/active-sheet state must not pin manual scrolling.
    h.get_by_role_and_label(Role::Button, &titles[0])
        .scroll_to_me();
    h.run_steps(8);
    let first = assert_visible(&h, &titles[0]);
    h.run_steps(8);
    assert_eq!(
        h.get_by_role_and_label(Role::Button, &titles[0]).rect(),
        first
    );
    h.event(egui::Event::PointerMoved(first.center()));
    for horizontal in [false, true] {
        for delta in [-600.0, 600.0] {
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: if horizontal {
                    egui::vec2(delta, 0.0)
                } else {
                    egui::vec2(0.0, delta)
                },
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(8);
            if delta < 0.0 {
                assert!(
                    h.get_by_role_and_label(Role::Button, &titles[0])
                        .rect()
                        .left()
                        < first.left() - 100.0
                );
            }
        }
    }
    assert_visible(&h, &titles[0]);
    let first = h.get_by_role_and_label(Role::Button, &titles[0]).rect();
    h.get_by_role_and_label(Role::Button, t("Zoom Out")).focus();
    h.run_steps(8);
    for label in ["Zoom", "Zoom In", "Fit Map", "Statistics"] {
        h.key_press(Key::Tab);
        h.run_steps(8);
        assert!(
            h.get_by_role_and_label(Role::Button, t(label)).is_focused(),
            "{label}: wrong Tab order"
        );
        let rect = assert_visible(&h, t(label));
        assert!(rect.left() >= add.right());
        assert_eq!(
            h.get_by_role_and_label(Role::Button, &titles[0]).rect(),
            first,
            "focus in controls scrolled sheet tabs"
        );
    }
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(8);
    assert!(
        h.get_by_role_and_label(Role::Button, t("Fit Map"))
            .is_focused()
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets, before);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
    assert_eq!(
        h.ctx.content_rect().bottom() - h.state().doc().unwrap().view.canvas_rect().bottom(),
        height
    );
    let original_title = h.state().doc().unwrap().doc.sheet().title.clone();
    let state = h.state().doc().unwrap().doc.state_id();
    let renamed = "A renamed sheet with a much longer title than before";
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Rename Sheet", |sheet| {
            sheet.title = renamed.into();
            Ok(())
        })
        .unwrap();
    h.run_steps(8);
    assert_visible(&h, renamed);
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(8);
    assert_visible(&h, &original_title);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);

    h.get_by_role_and_label(Role::Button, &titles[19]).focus();
    h.run_steps(8);
    let pos = assert_visible(&h, &titles[19]).center();
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(4);
    for label in [
        "Rename Sheet",
        "Copy Sheet",
        "Paste Sheet",
        "Duplicate Sheet",
        "Save Sheet As…",
        "New Sheet",
        "Move Left",
        "Move Right",
        "Delete Sheet",
    ] {
        menu_action(&h, t(label)).scroll_to_me();
        h.run_steps(8);
        let node = menu_action(&h, t(label));
        assert!(h.ctx.content_rect().expand(1.0).contains_rect(node.rect()));
    }
    h.key_press(Key::Escape);
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets, before);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::ToggleStatusBar);
    h.run_steps(8);
    assert!(h.query_by_role_and_label(Role::Button, t("Zoom")).is_none());
    let expanded = assert_visible(&h, &titles[19]);
    assert!(expanded.width() > last.width());
    assert_visible(&h, t("New Sheet"));
    h.get_by_role_and_label(Role::Button, t("New Sheet"))
        .click();
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 21);
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 20);
    let new_title = h.state().doc().unwrap().doc.sheet().title.clone();
    assert_visible(&h, &new_title);
    assert_eq!(
        h.ctx.content_rect().bottom() - h.state().doc().unwrap().view.canvas_rect().bottom(),
        height
    );
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::ToggleStatusBar);
    h.run_steps(8);
    for size in [egui::vec2(1000.0, 700.0), egui::vec2(320.0, 240.0)] {
        h.set_size(size);
        h.run_steps(8);
        assert_visible(&h, &new_title);
        assert_visible(&h, t("New Sheet"));
    }
    h.state_mut().new_doc(Workbook::default());
    h.run_steps(8);
    let title = h.state().doc().unwrap().doc.sheet().title.clone();
    assert_visible(&h, &title);
    for label in ["Scroll sheets left", "Scroll sheets right"] {
        assert!(
            h.get_by_role_and_label(Role::Button, t(label))
                .accesskit_node()
                .is_disabled()
        );
    }
    h.state_mut().active = 0;
    h.run_steps(8);
    assert_visible(&h, &new_title);

    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for scale in [1.0, 1.5, 2.0] {
                for size in [
                    egui::vec2(320.0, 240.0),
                    egui::vec2(640.0, 480.0),
                    egui::vec2(1000.0, 700.0),
                ] {
                    let mut h = harness(size, scale, language, theme);
                    let add = assert_visible(&h, t("New Sheet"));
                    let left = assert_visible(&h, t("Scroll sheets left"));
                    let right = assert_visible(&h, t("Scroll sheets right"));
                    assert!(left.right() <= right.left());
                    assert!(right.right() < add.left());
                    assert_sheet_paint_is_clipped(&h, right.right(), add.left());
                    h.get_by_role_and_label(Role::Button, t("Zoom")).focus();
                    h.run_steps(8);
                    let zoom = assert_visible(&h, t("Zoom"));
                    assert!(zoom.left() >= add.right());
                    let percent = format!("{}%", h.state().doc().unwrap().view.zoom_percent());
                    assert_paint(&h, &percent, zoom);
                    h.get_by_role_and_label(Role::Button, t("Statistics"))
                        .focus();
                    h.run_steps(8);
                    let statistics = assert_visible(&h, t("Statistics"));
                    assert!(statistics.left() >= add.right());
                    let count = h.state().doc().unwrap().doc.sheet().topics.len();
                    assert_paint(&h, &format!("{}: {count}", t("Topics")), statistics);
                    let first = h.state().doc().unwrap().doc.workbook().sheets[0]
                        .title
                        .clone();
                    let rect = assert_visible(&h, &first);
                    assert!(rect.right() <= add.left());
                    assert_paint(&h, &first, rect);
                    h.get_by_role_and_label(Role::Button, t("Zoom")).focus();
                    h.run_steps(8);
                    let zoom = assert_visible(&h, t("Zoom"));
                    assert_paint(&h, &percent, zoom);
                }
            }
        }
    }
}
