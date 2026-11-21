//! Locale changes are process-wide; keep this matrix sequential.

use egui::{Event, PointerButton, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

mod support;
use support::menu_action;

fn right_click(h: &mut Harness<'_, ThinodaApp>, pos: egui::Pos2) {
    h.event(Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Default::default(),
        });
        h.run_steps(1);
    }
    h.run_steps(4);
}

fn assert_popups_fit(h: &Harness<'_, ThinodaApp>) {
    let ppp = h.ctx.pixels_per_point();
    // egui snaps popups to physical pixels; allow one pixel of rounding slack.
    let viewport = h.ctx.content_rect().expand(1.0 / ppp);
    h.ctx.memory(|memory| {
        // Hidden areas keep their last rect, which may predate a scale change.
        for layer in memory.layer_ids().filter(|layer| {
            layer.order == egui::Order::Foreground && memory.areas().visible_last_frame(layer)
        }) {
            if let Some(rect) = memory.area_rect(layer.id) {
                assert!(
                    viewport.contains_rect(rect),
                    "Popup outside viewport: {rect:?}"
                );
            }
        }
    });
}

fn reveal(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    menu_action(h, label).scroll_to_me();
    h.run_steps(8);
    let rect = menu_action(h, label).rect();
    assert!(
        h.ctx.content_rect().contains_rect(rect),
        "{label} outside viewport: {rect:?}"
    );
    assert_popups_fit(h);
}

#[test]
fn context_actions_fit_short_localized_windows_without_losing_commands() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (width, height, scale) in [
                (640.0, 360.0, 1.0),
                (960.0, 540.0, 1.5),
                (1280.0, 480.0, 2.0),
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
                app.settings.ai.enabled = true;
                app.exec(&egui::Context::default(), Cmd::New);
                let mut h = Harness::builder()
                    .with_size(egui::vec2(width, height))
                    .with_context_setup(|ctx| {
                        ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    })
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.run_steps(4);
                let before = h.state().doc().unwrap().doc.workbook().clone();
                let selection = h.state().doc().unwrap().view.selection.clone();
                let pos = {
                    let d = h.state().doc().unwrap();
                    d.view.selected_topic_rect(&d.doc).unwrap().center()
                };
                right_click(&mut h, pos);
                for label in [
                    "Topic",
                    "Subtopic",
                    "Cut",
                    "Paste",
                    "Add",
                    "Structure",
                    "Styles",
                    "Zones",
                    "Board",
                    "Grid",
                    "Align",
                    "Select",
                    "Tools",
                    "View",
                    "Command Palette…",
                    "Delete",
                ] {
                    reveal(&mut h, t(label));
                }
                for (submenu, labels) in [
                    (
                        "Styles",
                        &[
                            "Topic style",
                            "Format Panel",
                            "Copy Style",
                            "Paste Style",
                            "Reset Style",
                            "Save Quick Style…",
                        ][..],
                    ),
                    ("Structure", &["Indent", "Outdent", "New Sheet from Topic"]),
                    ("Add", &["To-Do", "Task Planning", "Edit notes", "Boundary"]),
                    ("View", &["Fit Map", "Expand All", "Collapse All"]),
                    ("Board", &["Zoom to selection", "Hide central topic"]),
                    ("Align", &["Align Left", "Distribute Vertical Centers"]),
                    ("Tools", &["Sticky note", "Image card", "Pen", "Lasso"]),
                ] {
                    reveal(&mut h, t(submenu));
                    menu_action(&h, t(submenu)).click();
                    h.run_steps(3);
                    for label in labels {
                        reveal(&mut h, t(label));
                    }
                    h.key_press(egui::Key::Escape);
                    h.run_steps(3);
                    right_click(&mut h, pos);
                }
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
                h.state_mut().settings.ai.provider = thinoda_platform::settings::AiProvider::None;
                right_click(&mut h, pos);
                let ai = t("AI").to_string();
                reveal(&mut h, &ai);
                h.get_by_role_and_label(Role::Button, &ai).click();
                h.run_steps(3);
                reveal(&mut h, t(thinoda_app::ai::AiCommand::Summarize.label()));
                for action in thinoda_app::ai::AiCommand::ALL
                    .into_iter()
                    .filter(|action| *action != thinoda_app::ai::AiCommand::GenerateMap)
                {
                    assert!(
                        menu_action(&h, t(action.label()))
                            .accesskit_node()
                            .is_disabled()
                    );
                }
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);

                let title = h.state().doc().unwrap().doc.sheet().title.clone();
                let tab = h.get_by_label(&title).rect().center();
                right_click(&mut h, tab);
                reveal(&mut h, t("Delete Sheet"));
                reveal(&mut h, t("Rename Sheet"));
                menu_action(&h, t("New Sheet")).focus();
                h.run_steps(8);
                let rect = menu_action(&h, t("New Sheet")).rect();
                assert!(h.ctx.content_rect().contains_rect(rect));
                h.key_press(egui::Key::Enter);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 2);
                let ctx = h.ctx.clone();
                h.state_mut()
                    .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                h.run_steps(3);
                let canvas = h.state().doc().unwrap().view.canvas_rect();
                for edge in [
                    canvas.left_top() + egui::vec2(8.0, 80.0),
                    canvas.right_bottom() - egui::vec2(8.0, 8.0),
                ] {
                    right_click(&mut h, edge);
                    for label in [
                        "Floating Topic",
                        "Paste",
                        "Tools",
                        "Board",
                        "Grid",
                        "Align",
                        "View",
                    ] {
                        reveal(&mut h, t(label));
                    }
                    h.key_press(egui::Key::Escape);
                    h.run_steps(3);
                    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                }
            }
        }
    }

    let mut app = ThinodaApp::headless();
    app.settings.show_format_panel = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1600.0))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    let pos = {
        let d = h.state().doc().unwrap();
        d.view.selected_topic_rect(&d.doc).unwrap().center()
    };
    right_click(&mut h, pos);
    let menu_order = [
        "Topic",
        "Cut",
        "Add",
        "Structure",
        "Styles",
        "Zones",
        "Board",
        "Grid",
        "Align",
        "Select",
        "Tools",
        "View",
        "Command Palette…",
        "Delete",
    ];
    let positions: Vec<_> = menu_order
        .iter()
        .map(|label| menu_action(&h, t(label)).rect().min.y)
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(h.state().style_clipboard.is_none());
    menu_action(&h, t("Styles")).click();
    h.run_steps(3);
    let style_order = [
        "Topic style",
        "Format Panel",
        "Copy Style",
        "Paste Style",
        "Reset Style",
        "Save Quick Style…",
    ];
    let positions: Vec<_> = style_order
        .iter()
        .map(|label| menu_action(&h, t(label)).rect().min.y)
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    menu_action(&h, t("Copy Style")).click();
    h.run_steps(3);
    assert!(h.state().style_clipboard.is_some());

    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let blank = h.state().doc().unwrap().view.canvas_rect().left_top() + egui::vec2(8.0, 80.0);
    right_click(&mut h, blank);
    menu_action(&h, t("Tools")).click();
    h.run_steps(3);
    reveal(&mut h, t("Sticky note"));
    menu_action(&h, t("Sticky note")).click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().view.board_tool,
        thinoda_canvas::BoardTool::Sticky
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(3);

    let shape = {
        use thinoda_core::{CanvasObject, Rect, ShapeKind, Vec2};
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.set_board_tool(thinoda_canvas::BoardTool::Select);
        doc.doc
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(-400.0, -250.0), Vec2::new(100.0, 70.0)),
            ))
            .unwrap()
    };
    h.run_steps(3);
    let pos = {
        let view = &h.state().doc().unwrap().view;
        view.canvas_rect().center() + view.offset + egui::vec2(-350.0, -215.0) * view.zoom
    };
    right_click(&mut h, pos);
    assert_eq!(h.state().doc().unwrap().view.board_selection, vec![shape]);
    for label in [
        "Cut", "Styles", "Zones", "Board", "Grid", "Align", "Tools", "Delete",
    ] {
        reveal(&mut h, t(label));
    }
    let order = ["Styles", "Zones", "Board", "Grid", "Align"];
    let positions: Vec<_> = order
        .iter()
        .map(|label| menu_action(&h, t(label)).rect().min.y)
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    menu_action(&h, t("Tools")).click();
    h.run_steps(3);
    reveal(&mut h, t("Image card"));
    assert!(
        !menu_action(&h, t("Image card"))
            .accesskit_node()
            .is_disabled()
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    right_click(&mut h, pos);
    menu_action(&h, t("Zones")).click();
    h.run_steps(3);
    for label in [
        "Create Zone",
        "Add to Zone…",
        "Remove from Zone",
        "Manage Zones",
    ] {
        reveal(&mut h, t(label));
    }
    menu_action(&h, t("Create Zone")).click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Cancel")).click();
    h.run_steps(3);
    right_click(&mut h, pos);
    menu_action(&h, t("Grid")).click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::CheckBox, t("Dot grid"))
        .click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().board.grid,
        thinoda_core::GridKind::Dots
    );
    right_click(&mut h, pos);
    menu_action(&h, t("Align")).click();
    h.run_steps(3);
    reveal(&mut h, t("Align Left"));
    reveal(&mut h, t("Distribute Vertical Centers"));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    right_click(&mut h, pos);
    menu_action(&h, t("Board")).click();
    h.run_steps(3);
    reveal(&mut h, t("Lock board selection"));
    let lock = menu_action(&h, t("Lock board selection"));
    assert!(!lock.accesskit_node().is_disabled());
    lock.click();
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .locked
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    right_click(&mut h, pos);
    menu_action(&h, t("Board")).click();
    h.run_steps(3);
    reveal(&mut h, t("Unlock board selection"));
}

fn assert_context_menu_above_nearby_menu(h: &Harness<'_, ThinodaApp>) {
    let popup = egui::LayerId::new(
        egui::Order::Foreground,
        thinoda_canvas::MapCanvas::id().with("popup"),
    );
    let nearby = egui::LayerId::new(
        thinoda_canvas::format_menu::ORDER,
        thinoda_canvas::format_menu::area_id(&h.ctx),
    );
    h.ctx.memory(|memory| {
        assert!(
            memory.areas().visible_last_frame(&popup),
            "context menu closed"
        );
        if !memory.areas().visible_last_frame(&nearby) {
            return;
        }
        let order: Vec<_> = memory.layer_ids().collect();
        let index = |layer| order.iter().position(|candidate| *candidate == layer);
        assert!(
            index(popup) > index(nearby),
            "nearby menu painted above the context menu"
        );
    });
}

#[test]
fn right_click_selection_keeps_the_context_menu_above_the_nearby_menu() {
    let mut app = ThinodaApp::headless();
    app.settings.show_format_panel = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    let shapes = {
        use thinoda_core::{CanvasObject, Rect, ShapeKind, Vec2};
        let doc = h.state_mut().doc_mut().unwrap();
        [
            (ShapeKind::Rectangle, Vec2::new(-400.0, -250.0)),
            (ShapeKind::Ellipse, Vec2::new(250.0, 120.0)),
        ]
        .map(|(kind, min)| {
            doc.doc
                .add_canvas_object(CanvasObject::shape(
                    kind,
                    Rect::from_min_size(min, Vec2::new(100.0, 70.0)),
                ))
                .unwrap()
        })
    };
    h.run_steps(3);
    let centre = |h: &Harness<'_, ThinodaApp>, offset: egui::Vec2| {
        let view = &h.state().doc().unwrap().view;
        view.canvas_rect().center() + view.offset + offset * view.zoom
    };
    let targets = [
        (shapes[0], egui::vec2(-350.0, -215.0)),
        (shapes[1], egui::vec2(300.0, 155.0)),
    ];
    for round in 0..3 {
        for (shape, offset) in targets {
            // Raise the nearby menu in egui's order, as a click on it would.
            if round > 0 {
                let menu = egui::LayerId::new(
                    thinoda_canvas::format_menu::ORDER,
                    thinoda_canvas::format_menu::area_id(&h.ctx),
                );
                h.ctx.move_to_top(menu);
                h.run_steps(1);
            }
            let pos = centre(&h, offset);
            right_click(&mut h, pos);
            assert_eq!(h.state().doc().unwrap().view.board_selection, vec![shape]);
            assert_context_menu_above_nearby_menu(&h);
            h.run_steps(4);
            assert_context_menu_above_nearby_menu(&h);
            h.key_press(egui::Key::Escape);
            h.run_steps(3);
        }
    }
}

#[test]
fn right_click_on_the_nearby_menu_opens_the_selection_menu_not_a_stale_marker() {
    use thinoda_canvas::Action;
    let mut app = ThinodaApp::headless();
    app.settings.show_format_panel = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    let ctx = h.ctx.clone();
    h.state_mut().exec(
        &ctx,
        Cmd::Canvas(Action::ToggleMarkerColor("flag-red".into(), None)),
    );
    h.run_steps(5);
    let (root, marker) = {
        let d = h.state().doc().unwrap();
        let root = d.doc.sheet().root;
        (root, d.view.symbol_rect(&d.doc, root, 0).unwrap().center())
    };
    right_click(&mut h, marker);
    assert_eq!(
        h.state().doc().unwrap().view.context_marker,
        Some((root, 0))
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    let nearby = egui::LayerId::new(
        thinoda_canvas::format_menu::ORDER,
        thinoda_canvas::format_menu::area_id(&h.ctx),
    );
    let rect = h.ctx.memory(|memory| {
        assert!(
            memory.areas().visible_last_frame(&nearby),
            "nearby menu hidden"
        );
        memory.area_rect(nearby.id).unwrap()
    });
    right_click(&mut h, rect.left_top() + egui::vec2(3.0, 3.0));
    assert_eq!(h.state().doc().unwrap().view.context_marker, None);
    assert_context_menu_above_nearby_menu(&h);
    assert!(
        h.query_all_by_role_and_label(Role::Button, t("Remove Symbol"))
            .next()
            .is_none(),
        "forwarded right-click reopened the marker menu"
    );
}
