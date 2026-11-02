use egui::accesskit::Role;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::app::{Cmd, FormatTab, ThinodaApp};
use thinoda_core::{
    CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, Card, CardKind, Color,
    Endpoint, InkStroke, InkTool, Rect, ShapeKind, Sheet, TopicId, TopicStyle, Vec2, Workbook,
    board::BoardMember, quick_styles::preset,
};
use thinoda_io::{Format, export_bytes, import_bytes};

fn app_with_sheet(sheet: Sheet) -> ThinodaApp {
    let workbook = Workbook::new(sheet);
    workbook.validate().unwrap();
    let mut app = ThinodaApp::headless();
    app.docs.clear();
    app.new_doc(workbook);
    app.settings.show_format_panel = true;
    app.format_tab = FormatTab::Style;
    app
}

fn add_card(sheet: &mut Sheet, title: &str, kind: CardKind, x: f32) -> TopicId {
    sheet
        .add_card(
            title,
            Vec2::new(x, 100.0),
            Card::new(kind, Vec2::new(120.0, 80.0)),
        )
        .unwrap()
}

fn add_shape(sheet: &mut Sheet, kind: ShapeKind, x: f32) -> CanvasObjectId {
    let mut object = CanvasObject::shape(
        kind,
        Rect::from_min_size(Vec2::new(x, 0.0), Vec2::new(100.0, 70.0)),
    );
    if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
        *label = format!("{kind:?}");
    }
    object.rotation = 15.0;
    object.style.opacity = 0.6;
    sheet.add_canvas_object(object).unwrap()
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1000.0))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(5);
    h
}

fn click_swatch(h: &mut Harness<'_, ThinodaApp>, color: &str, row: usize) {
    h.query_all_by_role_and_label(Role::ColorWell, color)
        .nth(row)
        .unwrap()
        .scroll_to_me();
    h.run_steps(5);
    let well = h
        .query_all_by_role_and_label(Role::ColorWell, color)
        .nth(row)
        .unwrap();
    assert!(!well.accesskit_node().is_disabled());
    well.click();
    h.run_steps(5);
}

#[test]
fn quick_styles_cover_every_shape_and_round_trip_as_one_edit() {
    let mut sheet = Sheet::new("Shapes", "Central");
    let ids: Vec<_> = ShapeKind::ALL
        .into_iter()
        .enumerate()
        .map(|(index, kind)| add_shape(&mut sheet, kind, index as f32 * 120.0))
        .collect();
    let mut app = app_with_sheet(sheet);
    let doc = app.doc_mut().unwrap();
    doc.view.selection.clear();
    doc.view.board_selection = ids.clone();
    let before = doc.doc.workbook().clone();
    let originals: Vec<_> = ids
        .iter()
        .map(|id| doc.doc.sheet().canvas.object(*id).unwrap().clone())
        .collect();
    let style = preset("Warning").unwrap().style;
    assert!(app.command_available(&Cmd::ApplyQuickStyle(style.clone())));
    app.exec(
        &egui::Context::default(),
        Cmd::ApplyQuickStyle(style.clone()),
    );
    app.exec(
        &egui::Context::default(),
        Cmd::ApplyQuickStyle(style.clone()),
    );

    let doc = app.doc_mut().unwrap();
    for (id, mut expected) in ids.iter().zip(originals) {
        expected.style.fill = style.fill;
        expected.style.stroke = None;
        expected.style.stroke_width = 2.0;
        expected.style.text_color = style.text_color;
        assert_eq!(doc.doc.sheet().canvas.object(*id).unwrap(), &expected);
    }
    let after = doc.doc.workbook().clone();
    let bytes = export_bytes(&after, Format::Thinoda, &Default::default()).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), after);
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
    assert!(!doc.doc.can_undo());
    assert!(doc.doc.redo());
    assert_eq!(doc.doc.workbook(), &after);
}

#[test]
fn mixed_quick_styles_expand_unlocked_groups_and_skip_locked_or_unsupported_items() {
    let mut sheet = Sheet::new("Mixed", "Central");
    let ordinary = sheet.add_child(sheet.root, None, "Ordinary");
    let sticky = add_card(&mut sheet, "Sticky", CardKind::Sticky, 100.0);
    let text = add_card(&mut sheet, "Text", CardKind::Text, 250.0);
    let locked_card = add_card(&mut sheet, "Locked", CardKind::Sticky, 400.0);
    let group_card = add_card(&mut sheet, "Grouped", CardKind::Text, 550.0);
    let locked_group_card = add_card(&mut sheet, "Locked group", CardKind::Text, 700.0);
    let shape = add_shape(&mut sheet, ShapeKind::Diamond, 0.0);
    let group_shape = add_shape(&mut sheet, ShapeKind::Star, 150.0);
    let locked_group_shape = add_shape(&mut sheet, ShapeKind::Cloud, 300.0);
    let ink = sheet
        .add_canvas_object(CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::BLACK,
            2.0,
            vec![Vec2::ZERO, Vec2::new(20.0, 20.0)],
        )))
        .unwrap();
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(0.0, 200.0)),
            Endpoint::Point(Vec2::new(100.0, 200.0)),
        )))
        .unwrap();
    sheet
        .group_board_members(&[
            BoardMember::Card(group_card),
            BoardMember::Object(group_shape),
        ])
        .unwrap();
    sheet
        .group_board_members(&[
            BoardMember::Card(locked_group_card),
            BoardMember::Object(locked_group_shape),
        ])
        .unwrap();
    sheet
        .set_board_members_locked(
            &[
                BoardMember::Card(locked_card),
                BoardMember::Card(locked_group_card),
            ],
            true,
        )
        .unwrap();
    let mut app = app_with_sheet(sheet);
    let doc = app.doc_mut().unwrap();
    doc.view.selection = vec![sticky, text, group_card, locked_card, ordinary];
    doc.view.board_selection = vec![shape, locked_group_shape, ink, connector];
    let before = doc.doc.workbook().clone();
    let style = preset("Emphasis").unwrap().style;
    app.exec(
        &egui::Context::default(),
        Cmd::ApplyQuickStyle(style.clone()),
    );

    let doc = app.doc_mut().unwrap();
    for id in [ordinary, sticky, text, group_card] {
        let topic = doc.doc.sheet().topic(id).unwrap();
        assert_eq!(topic.style, style);
        if let Some(card) = topic.card {
            assert_eq!(card.color, style.fill);
        }
    }
    for id in [shape, group_shape] {
        let object = doc.doc.sheet().canvas.object(id).unwrap();
        assert_eq!(object.style.fill, style.fill);
        assert_eq!(object.style.text_color, style.text_color);
        assert_eq!(object.style.stroke, None);
        assert_eq!(object.style.opacity, 0.6);
    }
    let prior = &before.sheets[0];
    for id in [locked_card, locked_group_card] {
        assert_eq!(doc.doc.sheet().topic(id), prior.topic(id));
    }
    for id in [locked_group_shape, ink, connector] {
        assert_eq!(doc.doc.sheet().canvas.object(id), prior.canvas.object(id));
    }
    let after = doc.doc.workbook().clone();
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
    assert!(!doc.doc.can_undo());
    assert!(doc.doc.redo());
    assert_eq!(doc.doc.workbook(), &after);

    app.exec(&egui::Context::default(), Cmd::ResetStyle);
    let doc = app.doc_mut().unwrap();
    for id in [ordinary, sticky, text, group_card] {
        let topic = doc.doc.sheet().topic(id).unwrap();
        assert_eq!(topic.style, TopicStyle::default());
        if let Some(card) = topic.card {
            assert_eq!(card.color, None);
        }
    }
    for id in [shape, group_shape] {
        assert_eq!(
            doc.doc.sheet().canvas.object(id).unwrap().style,
            Default::default()
        );
    }
    for id in [locked_group_shape, ink, connector] {
        assert_eq!(doc.doc.sheet().canvas.object(id), prior.canvas.object(id));
    }
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &after);

    for (topics, objects) in [
        (vec![locked_card], vec![]),
        (vec![], vec![locked_group_shape]),
        (vec![], vec![ink, connector]),
    ] {
        let doc = app.doc_mut().unwrap();
        doc.view.selection = topics;
        doc.view.board_selection = objects;
        assert!(!app.command_available(&Cmd::ApplyQuickStyle(TopicStyle::default())));
        assert!(!app.command_available(&Cmd::ResetStyle));
        app.exec(
            &egui::Context::default(),
            Cmd::ApplyQuickStyle(style.clone()),
        );
        assert_eq!(app.doc().unwrap().doc.workbook(), &after);
    }
}

#[test]
fn existing_shape_swatches_and_quick_styles_remain_usable() {
    let mut sheet = Sheet::new("Controls", "Central");
    let id = add_shape(&mut sheet, ShapeKind::Cloud, 0.0);
    let mut app = app_with_sheet(sheet);
    app.doc_mut().unwrap().view.selection.clear();
    app.doc_mut().unwrap().view.board_selection = vec![id];
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = harness(app);
    click_swatch(&mut h, "#2E7CF6", 0);
    let after_fill = h.state().doc().unwrap().doc.workbook().clone();
    click_swatch(&mut h, "#2BB673", 1);
    let after_border = h.state().doc().unwrap().doc.workbook().clone();
    click_swatch(&mut h, "#F5A623", 2);
    let object = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap();
    assert_eq!(object.style.fill, Color::from_hex("#2E7CF6"));
    assert_eq!(object.style.stroke, Color::from_hex("#2BB673"));
    assert_eq!(object.style.text_color, Color::from_hex("#F5A623"));
    let doc = h.state_mut().doc_mut().unwrap();
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &after_border);
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &after_fill);
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.workbook(), &before);
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Quick Style: Warning")
        .scroll_to_me();
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Quick Style: Warning")
        .click();
    h.run_steps(5);
    let object = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap();
    let warning = preset("Warning").unwrap().style;
    assert_eq!(object.style.fill, warning.fill);
    assert_eq!(object.style.text_color, warning.text_color);
}

#[test]
fn mixed_topic_and_card_swatches_affect_only_supported_fields() {
    let mut sheet = Sheet::new("Mixed controls", "Central");
    let topic = sheet.add_child(sheet.root, None, "Primary topic");
    let card = add_card(&mut sheet, "Card", CardKind::Sticky, 100.0);
    let mut app = app_with_sheet(sheet);
    app.doc_mut().unwrap().view.selection = vec![card, topic];
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Label, "Primary topic");
    click_swatch(&mut h, "#2E7CF6", 0);
    click_swatch(&mut h, "#2BB673", 1);
    click_swatch(&mut h, "#F5A623", 2);
    let sheet = h.state().doc().unwrap().doc.sheet();
    let blue = Color::from_hex("#2E7CF6");
    let green = Color::from_hex("#2BB673");
    let orange = Color::from_hex("#F5A623");
    assert_eq!(sheet.topic(topic).unwrap().style.fill, blue);
    assert_eq!(sheet.topic(topic).unwrap().style.border_color, green);
    assert_eq!(sheet.topic(topic).unwrap().style.text_color, orange);
    assert_eq!(sheet.topic(card).unwrap().card.unwrap().color, blue);
    assert_eq!(sheet.topic(card).unwrap().style.border_color, None);
    assert_eq!(sheet.topic(card).unwrap().style.text_color, orange);

    h.state_mut().doc_mut().unwrap().view.selection = vec![card];
    h.run_steps(5);
    let border = h
        .query_all_by_role_and_label(Role::ColorWell, "#2BB673")
        .nth(1)
        .unwrap();
    assert!(border.accesskit_node().is_disabled());
}

#[test]
fn captured_card_style_uses_its_visible_fill_and_automatic_text_contrast() {
    let mut sheet = Sheet::new("Capture", "Central");
    let card = add_card(&mut sheet, "Dark card", CardKind::Sticky, 100.0);
    let fill = Color::rgb(10, 20, 30);
    sheet.topic_mut(card).unwrap().card.as_mut().unwrap().color = Some(fill);
    let mut app = app_with_sheet(sheet);
    app.doc_mut().unwrap().view.selection = vec![card];
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Button, "Save Quick Style…")
        .scroll_to_me();
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Save Quick Style…")
        .click();
    h.run_steps(5);
    h.get_by_label("Style name").click();
    h.get_by_label("Style name").type_text("Captured card");
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Save").click();
    h.run_steps(5);
    let captured = &h.state().settings.quick_styles[0].style;
    assert_eq!(captured.fill, Some(fill));
    assert_eq!(captured.text_color, Some(Color::WHITE));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn locked_board_shape_exposes_disabled_style_controls() {
    let mut sheet = Sheet::new("Locked controls", "Central");
    let shape = add_shape(&mut sheet, ShapeKind::Rectangle, 0.0);
    sheet
        .set_board_members_locked(&[BoardMember::Object(shape)], true)
        .unwrap();
    let mut app = app_with_sheet(sheet);
    app.doc_mut().unwrap().view.selection.clear();
    app.doc_mut().unwrap().view.board_selection = vec![shape];
    assert!(!app.command_available(&Cmd::ApplyQuickStyle(TopicStyle::default())));
    assert!(!app.command_available(&Cmd::ResetStyle));
    let h = harness(app);
    for well in h.query_all_by_role_and_label(Role::ColorWell, "#2E7CF6") {
        assert!(well.accesskit_node().is_disabled());
    }
    assert!(
        h.query_all_by_role_and_label(Role::ColorWell, "#2E7CF6")
            .count()
            >= 3
    );
}
