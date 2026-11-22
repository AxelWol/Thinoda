use super::*;
use egui::{Role, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{CanvasObject, Rect as BoardRect, ShapeKind, Vec2};

fn harness(template: &str) -> Harness<'static, ThinodaApp> {
    harness_at(template, vec2(1000.0, 700.0), false)
}

fn harness_at(template: &str, size: egui::Vec2, dark: bool) -> Harness<'static, ThinodaApp> {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = false;
    app.settings.theme = if dark {
        SettingsTheme::Dark
    } else {
        SettingsTheme::Light
    };
    app.new_doc(templates::instantiate(template));
    let builder = Harness::builder().with_size(size);
    #[cfg(feature = "gpu-tests")]
    let builder = if std::env::var_os("THINODA_CONTEXT_ARTIFACTS").is_some() {
        builder.wgpu()
    } else {
        builder
    };
    let mut h = builder.build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(6);
    h
}

#[test]
fn compact_selection_controls_fit_without_scrolling_the_menu() {
    for dark in [false, true] {
        for size in [vec2(320.0, 240.0), vec2(640.0, 360.0)] {
            for kind in ["node", "stencil", "card", "editing-card"] {
                let mut h = harness_at(
                    if kind == "node" {
                        "mind-map"
                    } else {
                        "blank-canvas"
                    },
                    size,
                    dark,
                );
                if kind == "stencil" {
                    add_shape(&mut h, true);
                } else if kind.contains("card") {
                    let doc = h.state_mut().doc_mut().unwrap();
                    let id = doc
                        .doc
                        .edit_sheet("Card", |sheet| {
                            sheet
                                .add_card(
                                    "Card title",
                                    Vec2::ZERO,
                                    thinoda_core::Card::new(
                                        thinoda_core::CardKind::Sticky,
                                        Vec2::new(180.0, 120.0),
                                    ),
                                )
                                .map_err(thinoda_core::EditError::Invalid)
                        })
                        .unwrap();
                    doc.view.select_only(id);
                    if kind == "editing-card" {
                        doc.view.begin_edit(doc.doc.sheet(), id, false);
                    }
                    h.run_steps(6);
                }
                let mut compact_popup = false;
                if h.query_by_role_and_label(Role::Button, "Bold").is_none() {
                    let menu = h
                        .ctx
                        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
                        .unwrap();
                    let compact = !h
                        .query_all_by_role_and_label(Role::Button, "Text")
                        .any(|node| menu.contains_rect(node.rect()));
                    compact_popup = compact;
                    if compact {
                        h.query_all_by_role_and_label(Role::Button, "Format")
                            .find(|node| menu.contains_rect(node.rect()))
                            .unwrap_or_else(|| {
                                panic!(
                                    "{size:?} {dark} {kind}: no compact Format button in {menu:?}"
                                )
                            })
                            .click();
                        h.run_steps(3);
                    }
                    h.query_all_by_role_and_label(Role::Button, "Text")
                        .find(|node| compact || menu.contains_rect(node.rect()))
                        .unwrap_or_else(|| {
                            panic!("{size:?} {dark} {kind}: no Text tab in {menu:?}")
                        })
                        .click();
                    h.run_steps(5);
                }
                let canvas = h.state().doc().unwrap().view.canvas_rect();
                let control_bounds = if compact_popup {
                    h.ctx.content_rect()
                } else {
                    canvas
                };
                for (role, label) in [
                    (Role::Button, "Bold"),
                    (Role::ComboBox, "Font Family"),
                    (Role::Button, "Text Color"),
                ] {
                    let control = h.query_by_role_and_label(role, label).unwrap_or_else(|| {
                        panic!(
                            "{size:?} {dark} {kind}: missing {label}; popup open: {}",
                            egui::Popup::is_any_open(&h.ctx)
                        )
                    });
                    assert!(
                        control_bounds.contains_rect(control.rect()),
                        "{size:?} {dark} {kind} {label}: {:?} {control_bounds:?}",
                        control.rect()
                    );
                }
                let menu = h
                    .ctx
                    .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
                    .unwrap();
                assert!(
                    canvas.contains_rect(menu),
                    "{size:?} {dark} {kind}: {menu:?} {canvas:?}"
                );
                #[cfg(feature = "gpu-tests")]
                if let Some(directory) = std::env::var_os("THINODA_CONTEXT_ARTIFACTS") {
                    h.render()
                        .expect("context menu render")
                        .save(std::path::PathBuf::from(directory).join(format!(
                            "selection-{}-{}-{}.png",
                            kind,
                            if dark { "dark" } else { "light" },
                            size.x
                        )))
                        .unwrap();
                }
            }
        }
    }
}

fn click(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    h.get_by_role_and_label(Role::Button, label).click();
    h.run_steps(5);
}

fn add_shape(h: &mut Harness<'_, ThinodaApp>, stencil: bool) -> thinoda_core::CanvasObjectId {
    let mut object = CanvasObject::shape(
        ShapeKind::Rectangle,
        BoardRect::from_center_size(Vec2::ZERO, Vec2::new(140.0, 80.0)),
    );
    if stencil {
        let package = &thinoda_core::stencil::builtin_packages()[0];
        object.stencil = Some(thinoda_core::stencil_object::StencilRef::new(
            &package.id,
            &package.definitions[0].id,
        ));
    }
    let doc = h.state_mut().doc_mut().unwrap();
    let id = doc.doc.add_canvas_object(object).unwrap();
    doc.view.select_board_object(id);
    h.run_steps(6);
    id
}

#[test]
fn shape_selection_menu_keeps_quick_connect_and_transform_handles_clickable() {
    let mut h = harness_at("blank-canvas", vec2(900.0, 610.0), false);
    let source = add_shape(&mut h, false);
    h.state_mut().doc_mut().unwrap().view.offset = vec2(-180.0, 95.0);
    h.run_steps(8);
    let menu = h
        .ctx
        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
        .unwrap();
    for label in [
        "Add connected shape to the right",
        "Add connected shape above",
        "Add connected shape below",
        "Add connected shape to the left",
        "Resize board item",
        "Rotate board item",
    ] {
        let handle = h.get_by_role_and_label(Role::Button, label).rect();
        assert!(
            !menu.intersects(handle),
            "{label}: menu {menu:?} obscures handle {handle:?}"
        );
    }

    h.get_by_role_and_label(Role::Button, "Add connected shape to the right")
        .click();
    h.run_steps(6);
    let doc = h.state().doc().unwrap();
    let [connected] = doc.view.board_selection[..] else {
        panic!("expected the new shape to be selected");
    };
    assert_ne!(connected, source);
    assert_eq!(doc.doc.sheet().canvas.objects.len(), 3);
    assert_eq!(doc.doc.undo_label(), Some("Add connected shape"));
}

#[test]
fn shape_selection_menu_clears_visible_handles_after_moving_zooming_and_switching_sections() {
    for (window, offset, zoom, rotation) in [
        (vec2(900.0, 610.0), vec2(-180.0, 95.0), 1.0, 0.0),
        (vec2(1000.0, 700.0), vec2(160.0, -50.0), 1.5, 37.0),
        (vec2(720.0, 480.0), vec2(180.0, 40.0), 0.8, 30.0),
    ] {
        let mut h = harness_at("blank-canvas", window, false);
        let source = add_shape(&mut h, false);
        let doc = h.state_mut().doc_mut().unwrap();
        doc.doc.rotate_canvas_object(source, rotation).unwrap();
        doc.view.offset = offset;
        doc.view.set_zoom(zoom);
        h.run_steps(8);
        for section in ["Style", "Text", "Arrange", "Actions"] {
            let menu = h
                .ctx
                .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
                .unwrap();
            h.query_all_by_role_and_label(Role::Button, section)
                .find(|node| menu.contains_rect(node.rect()))
                .unwrap()
                .click();
            h.run_steps(6);
            let menu = h
                .ctx
                .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
                .unwrap();
            for label in [
                "Add connected shape to the right",
                "Add connected shape above",
                "Add connected shape below",
                "Add connected shape to the left",
                "Resize board item",
                "Rotate board item",
            ] {
                if let Some(handle) = h.query_by_role_and_label(Role::Button, label) {
                    let rect = handle.rect();
                    let canvas = h.state().doc().unwrap().view.canvas_rect();
                    if canvas.intersects(rect) {
                        assert!(
                            !menu.intersects(rect),
                            "{window:?} {offset:?} zoom {zoom} rotation {rotation} \
                             {section} {label}: {menu:?} overlaps {rect:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn compact_shape_menu_keeps_handles_clear_and_opens_formatting_on_demand() {
    let mut h = harness_at("blank-canvas", vec2(320.0, 240.0), false);
    let source = add_shape(&mut h, true);
    let menu = h
        .ctx
        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
        .unwrap();
    assert!(menu.width() < 60.0 && menu.height() < 60.0, "{menu:?}");
    for label in [
        "Add connected shape to the right",
        "Add connected shape above",
        "Add connected shape below",
        "Add connected shape to the left",
        "Resize board item",
        "Rotate board item",
    ] {
        if let Some(handle) = h.query_by_role_and_label(Role::Button, label) {
            let rect = handle.rect();
            if h.state().doc().unwrap().view.canvas_rect().intersects(rect) {
                assert!(
                    !menu.intersects(rect),
                    "{label}: {menu:?} overlaps {rect:?}"
                );
            }
        }
    }

    h.query_all_by_role_and_label(Role::Button, "Format")
        .find(|node| menu.contains_rect(node.rect()))
        .unwrap()
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Text").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(source)
            .unwrap()
            .label_style
            .as_ref()
            .unwrap()
            .bold
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert!(!format_menu::compact_open(&h.ctx));
    assert_eq!(h.state().doc().unwrap().view.board_selection, [source]);
    h.set_size(vec2(900.0, 610.0));
    h.run_steps(8);
    let expanded = h
        .ctx
        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
        .unwrap();
    assert!(expanded.width() > 200.0, "{expanded:?}");
    let right = h
        .get_by_role_and_label(Role::Button, "Add connected shape to the right")
        .rect();
    assert!(!expanded.intersects(right));
    h.set_size(vec2(320.0, 240.0));
    h.run_steps(8);
    h.get_by_role_and_label(Role::Button, "Add connected shape to the right")
        .click();
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.sheet().canvas.objects.len(), 3);
}

#[test]
fn compact_menu_state_clears_when_its_selection_disappears() {
    let mut h = harness_at("blank-canvas", vec2(320.0, 240.0), false);
    add_shape(&mut h, true);
    let menu = h
        .ctx
        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
        .unwrap();
    h.query_all_by_role_and_label(Role::Button, "Format")
        .find(|node| menu.contains_rect(node.rect()))
        .unwrap()
        .click();
    h.run_steps(3);
    assert!(format_menu::compact_open(&h.ctx));

    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .board_selection
        .clear();
    h.run_steps(4);
    assert!(!format_menu::compact_open(&h.ctx));
}

#[test]
fn topic_and_card_menus_keep_add_resize_and_rotation_handles_clear() {
    for kind in ["node", "card"] {
        for offset in [vec2(-140.0, 60.0), vec2(80.0, -50.0), vec2(170.0, 50.0)] {
            let mut h = harness_at(
                if kind == "node" {
                    "mind-map"
                } else {
                    "blank-canvas"
                },
                vec2(900.0, 610.0),
                false,
            );
            if kind == "card" {
                let doc = h.state_mut().doc_mut().unwrap();
                let id = doc
                    .doc
                    .edit_sheet("Card", |sheet| {
                        sheet
                            .add_card(
                                "Card",
                                Vec2::ZERO,
                                thinoda_core::Card::new(
                                    thinoda_core::CardKind::Sticky,
                                    Vec2::new(180.0, 100.0),
                                ),
                            )
                            .map_err(thinoda_core::EditError::Invalid)
                    })
                    .unwrap();
                doc.view.select_only(id);
            }
            h.state_mut().doc_mut().unwrap().view.offset = offset;
            h.run_steps(8);
            let menu = h
                .ctx
                .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
                .unwrap();
            let mut checked = 0;
            for label in [
                "Add child topic",
                "Add sibling topic",
                "Resize board item",
                "Rotate board item",
            ] {
                if let Some(handle) = h.query_by_role_and_label(Role::Button, label) {
                    let rect = handle.rect();
                    if h.state().doc().unwrap().view.canvas_rect().intersects(rect) {
                        checked += 1;
                        assert!(
                            !menu.intersects(rect),
                            "{kind} {offset:?} {label}: {menu:?} overlaps {rect:?}"
                        );
                    }
                }
            }
            assert!(checked > 0, "{kind} {offset:?}: no visible handles");
        }
    }
}

#[test]
fn compact_card_menu_preserves_handle_access_and_whole_card_formatting() {
    let mut h = harness_at("blank-canvas", vec2(320.0, 240.0), false);
    let id = {
        let doc = h.state_mut().doc_mut().unwrap();
        let id = doc
            .doc
            .edit_sheet("Card", |sheet| {
                sheet
                    .add_card(
                        "Card",
                        Vec2::ZERO,
                        thinoda_core::Card::new(
                            thinoda_core::CardKind::Sticky,
                            Vec2::new(180.0, 120.0),
                        ),
                    )
                    .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        doc.view.select_only(id);
        id
    };
    h.run_steps(8);
    let menu = h
        .ctx
        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
        .unwrap();
    assert!(menu.width() < 60.0 && menu.height() < 60.0, "{menu:?}");
    for label in ["Resize board item", "Rotate board item", "Add child topic"] {
        if let Some(handle) = h.query_by_role_and_label(Role::Button, label) {
            let rect = handle.rect();
            if h.state().doc().unwrap().view.canvas_rect().intersects(rect) {
                assert!(
                    !menu.intersects(rect),
                    "{label}: {menu:?} overlaps {rect:?}"
                );
            }
        }
    }
    h.query_all_by_role_and_label(Role::Button, "Format")
        .find(|node| menu.contains_rect(node.rect()))
        .unwrap()
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Text").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .style
            .bold,
        Some(true)
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .style
            .bold,
        None
    );
}

#[test]
fn relationship_menu_keeps_curve_and_endpoint_handles_usable() {
    let mut h = harness_at("blank-canvas", vec2(900.0, 610.0), false);
    let id = {
        let doc = h.state_mut().doc_mut().unwrap();
        let id = doc
            .doc
            .edit_sheet("Relationship", |sheet| {
                let first = sheet.add_floating("First", Vec2::new(-200.0, 0.0));
                let second = sheet.add_floating("Second", Vec2::new(200.0, 0.0));
                sheet.add_relationship(first, second)
            })
            .unwrap();
        doc.view.selection.clear();
        doc.view.board_selection.clear();
        doc.view.selected_object = Some(thinoda_canvas::ObjectSelection::Relationship(id));
        id
    };
    h.run_steps(8);
    let menu = h
        .ctx
        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
        .unwrap();
    for label in [
        "Relationship start",
        "First curve control",
        "Second curve control",
        "Relationship end",
    ] {
        let handle = h.get_by_role_and_label(Role::Button, label).rect();
        assert!(
            !menu.intersects(handle),
            "{label}: {menu:?} overlaps {handle:?}"
        );
    }
    h.get_by_role_and_label(Role::Button, "First curve control")
        .click();
    h.run_steps(2);
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(2);
    let relationship = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .relationships
        .iter()
        .find(|relationship| relationship.id == id)
        .unwrap();
    assert!(relationship.control_points.is_some());
}

#[test]
fn topic_image_resize_handle_is_in_the_menu_clearance_geometry() {
    let mut h = harness_at("mind-map", vec2(900.0, 610.0), false);
    let root = {
        let doc = h.state_mut().doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(160, 80)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let resource =
            thinoda_core::resources::Resource::new("test.png", "image/png", bytes.into_inner())
                .unwrap();
        doc.doc
            .edit_sheet("Image", |sheet| {
                thinoda_core::images::set_image(
                    sheet,
                    root,
                    Some((resource, Vec2::new(160.0, 80.0))),
                )
                .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        doc.view.select_only(root);
        root
    };
    h.run_steps(8);
    let handle = h.get_by_role_and_label(Role::Button, "Resize image").rect();
    let menu = h
        .ctx
        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
        .unwrap();
    let doc = h.state_mut().doc_mut().unwrap();
    let controls = doc.view.selected_control_rects(&doc.doc);
    assert!(controls.contains(&handle), "{handle:?}, {controls:?}");
    assert!(!menu.intersects(handle), "{menu:?} overlaps {handle:?}");
    let start = handle.center();
    let end = start + vec2(30.0, 15.0);
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(2);
    h.hover_at(end);
    h.run_steps(2);
    h.drop_at(end);
    h.run_steps(3);
    assert_ne!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(root)
            .unwrap()
            .image_size,
        Some(Vec2::new(160.0, 80.0))
    );
}

#[test]
fn compact_mind_map_menu_does_not_cover_the_add_handle() {
    let mut h = harness_at("mind-map", vec2(320.0, 240.0), false);
    let menu = h
        .ctx
        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
        .unwrap();
    let handle = h
        .get_by_role_and_label(Role::Button, "Add child topic")
        .rect();
    assert!(!menu.intersects(handle), "{menu:?} overlaps {handle:?}");
    let count = h.state().doc().unwrap().doc.sheet().topics.len();
    h.get_by_role_and_label(Role::Button, "Add child topic")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.sheet().topics.len(), count + 1);
}

#[test]
fn existing_node_actions_and_text_use_the_same_nearby_surface() {
    let mut h = harness("mind-map");
    assert!(
        h.query_by_role_and_label(Role::Button, "Add subtopic")
            .is_some()
    );
    let id = format_menu::area_id(&h.ctx);
    let actions = h.ctx.memory(|memory| memory.area_rect(id)).unwrap();
    assert!(
        h.state()
            .doc()
            .unwrap()
            .view
            .canvas_rect()
            .contains_rect(actions)
    );
    click(&mut h, "Text");
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_some());
    assert!(
        h.query_by_role_and_label(Role::Button, "Arrow at end")
            .is_none()
    );
    let before = h.state().doc().unwrap().doc.state_id();
    click(&mut h, "Bold");
    assert_ne!(h.state().doc().unwrap().doc.state_id(), before);
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    h.run_steps(3);
    click(&mut h, "Actions");
    assert!(
        h.query_by_role_and_label(Role::Button, "Add subtopic")
            .is_some()
    );
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
}

#[test]
fn stencil_uses_shape_menu_without_losing_its_definition() {
    let mut h = harness("blank-canvas");
    let id = add_shape(&mut h, true);
    let original = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap()
        .stencil
        .clone();
    assert!(
        h.query_by_role_and_label(Role::Button, "Outline color")
            .is_some()
    );
    assert!(
        h.query_by_role_and_label(Role::Button, "Arrow at start")
            .is_none()
    );
    click(&mut h, "Text");
    click(&mut h, "Bold");
    let object = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap();
    assert_eq!(object.stencil, original);
    assert!(object.label_style.as_ref().unwrap().bold);
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .label_style
            .is_none()
    );
}

#[test]
fn locked_shape_exposes_actions_instead_of_unusable_format_controls() {
    let mut h = harness("blank-canvas");
    let id = add_shape(&mut h, false);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .set_canvas_objects_locked(&[id], true)
        .unwrap();
    h.run_steps(5);
    assert!(h.query_by_role_and_label(Role::Button, "Actions").is_some());
    let menu_id = format_menu::area_id(&h.ctx);
    let menu = h.ctx.memory(|memory| memory.area_rect(menu_id)).unwrap();
    for label in ["Text", "Style", "Outline color", "Arrow at end"] {
        assert!(
            h.query_all_by_role_and_label(Role::Button, label)
                .all(|control| !menu.contains_rect(control.rect())),
            "{label}"
        );
    }
    click(&mut h, "More actions");
    h.get_by_role_and_label(Role::Button, "Board").hover();
    h.run_steps(6);
    assert!(
        h.query_by_role_and_label(Role::Button, "Unlock board selection")
            .is_some()
    );
}

#[test]
fn mixed_shape_text_toggle_applies_to_every_object_in_one_undo() {
    let mut h = harness("blank-canvas");
    let first = add_shape(&mut h, false);
    let second = add_shape(&mut h, true);
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.doc
            .update_canvas_object("Bold", first, |object| {
                object.label_style =
                    Some(std::sync::Arc::new(thinoda_core::label_style::LabelStyle {
                        bold: true,
                        ..Default::default()
                    }));
            })
            .unwrap();
        doc.view.select_board_object(first);
        doc.view.board_selection.push(second);
    }
    h.run_steps(5);
    click(&mut h, "Text");
    assert!(h.query_by_role_and_label(Role::Label, "Mixed").is_some());
    click(&mut h, "Bold");
    for id in [first, second] {
        assert!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .label_style
                .as_ref()
                .unwrap()
                .bold
        );
    }
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    let sheet = h.state().doc().unwrap().doc.sheet();
    assert!(
        sheet
            .canvas
            .object(first)
            .unwrap()
            .label_style
            .as_ref()
            .unwrap()
            .bold
    );
    assert!(sheet.canvas.object(second).unwrap().label_style.is_none());
}

#[test]
fn multi_card_fill_changes_only_the_common_property_in_one_undo() {
    let mut h = harness("blank-canvas");
    let ids = {
        let doc = h.state_mut().doc_mut().unwrap();
        let ids = doc
            .doc
            .edit_sheet("Cards", |sheet| {
                [thinoda_core::CardKind::Sticky, thinoda_core::CardKind::Text]
                    .into_iter()
                    .enumerate()
                    .map(|(index, kind)| {
                        let mut card = thinoda_core::Card::new(
                            kind,
                            Vec2::new(180.0 + index as f32 * 40.0, 120.0),
                        );
                        card.color = (index == 0).then_some(thinoda_core::Color::rgb(200, 30, 40));
                        sheet
                            .add_card("Card", Vec2::new(index as f32 * 250.0, 0.0), card)
                            .map_err(thinoda_core::EditError::Invalid)
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap();
        doc.view.select_only(ids[0]);
        doc.view.selection.push(ids[1]);
        ids
    };
    h.run_steps(6);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    click(&mut h, "Style");
    assert!(h.query_by_role_and_label(Role::Label, "Mixed").is_some());
    assert!(
        h.query_by_role_and_label(Role::Button, "More style options")
            .is_none()
    );
    click(&mut h, "Fill");
    h.get_by_role_and_label(Role::ColorWell, "#2E7CF6").click();
    h.run_steps(4);
    for id in ids {
        let doc = &h.state().doc().unwrap().doc;
        assert_eq!(
            doc.sheet().topic(id).unwrap().card.unwrap().color,
            Some(thinoda_core::Color::rgb(0x2e, 0x7c, 0xf6))
        );
    }
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn sticky_and_text_card_menus_stay_beside_the_card_on_a_narrow_canvas() {
    // A tall, narrow canvas (format panel open) once pushed the card menu to
    // the canvas corner because the handles blocked every adjacent position.
    for kind in [thinoda_core::CardKind::Sticky, thinoda_core::CardKind::Text] {
        for offset in [vec2(0.0, 0.0), vec2(-300.0, 250.0), vec2(-500.0, -200.0)] {
            let mut app = ThinodaApp::headless();
            app.settings.theme = SettingsTheme::Dark;
            app.new_doc(templates::instantiate("brainstorm-wall"));
            let mut h = Harness::builder()
                .with_size(vec2(1000.0, 1400.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(6);
            let doc = h.state_mut().doc_mut().unwrap();
            let id = doc
                .doc
                .edit_sheet("Card", |sheet| {
                    sheet
                        .add_card(
                            "Test",
                            Vec2::new(700.0, -250.0),
                            thinoda_core::Card::new(kind, Vec2::new(180.0, 150.0)),
                        )
                        .map_err(thinoda_core::EditError::Invalid)
                })
                .unwrap();
            doc.view.select_only(id);
            doc.view.set_zoom(1.5);
            doc.view.offset = offset;
            h.run_steps(12);
            let menu = h
                .ctx
                .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
                .expect("nearby menu");
            let doc = h.state().doc().unwrap();
            let card = doc.view.selected_topic_rect(&doc.doc).unwrap();
            let dx = (card.left() - menu.right())
                .max(menu.left() - card.right())
                .max(0.0);
            let dy = (card.top() - menu.bottom())
                .max(menu.top() - card.bottom())
                .max(0.0);
            assert!(
                dx.hypot(dy) < 60.0,
                "{kind:?} {offset:?}: menu {menu:?} is far from card {card:?}"
            );
            assert!(!menu.contains(card.center()), "{kind:?} {offset:?}");
        }
    }
}
