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
                if h.query_by_role_and_label(Role::Button, "Bold").is_none() {
                    let menu = h
                        .ctx
                        .memory(|memory| memory.area_rect(format_menu::area_id(&h.ctx)))
                        .unwrap();
                    h.query_all_by_role_and_label(Role::Button, "Text")
                        .find(|node| menu.contains_rect(node.rect()))
                        .unwrap()
                        .click();
                    h.run_steps(5);
                }
                let canvas = h.state().doc().unwrap().view.canvas_rect();
                for (role, label) in [
                    (Role::Button, "Bold"),
                    (Role::ComboBox, "Font Family"),
                    (Role::Button, "Text Color"),
                ] {
                    let control = h.get_by_role_and_label(role, label);
                    assert!(
                        canvas.contains_rect(control.rect()),
                        "{size:?} {dark} {kind} {label}: {:?} {canvas:?}",
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
