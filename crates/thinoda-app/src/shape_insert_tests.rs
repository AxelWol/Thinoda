use super::*;
use egui::{Key, Modifiers, PointerButton, Pos2, Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{Rect as BoardRect, Vec2, templates};

fn app_with_shape() -> (ThinodaApp, CanvasObjectId) {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = false;
    app.new_doc(templates::instantiate("blank-canvas"));
    let doc = app.doc_mut().unwrap();
    let id = doc
        .doc
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            BoardRect::from_center_size(Vec2::new(0.0, 0.0), Vec2::new(160.0, 90.0)),
        ))
        .unwrap();
    doc.view.select_board_object(id);
    (app, id)
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut harness = Harness::builder()
        .with_size(vec2(1000.0, 700.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(6);
    harness
}

fn objects(app: &ThinodaApp) -> Vec<CanvasObject> {
    app.doc().unwrap().doc.sheet().canvas.objects.clone()
}

fn shape_kind(object: &CanvasObject) -> Option<ShapeKind> {
    match object.kind {
        CanvasObjectKind::Shape { shape, .. } => Some(shape),
        _ => None,
    }
}

fn click(h: &mut Harness<'_, ThinodaApp>, role: Role, label: &str) {
    h.get_by_role_and_label(role, label).scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(role, label).click();
    h.run_steps(4);
}

fn open_library(h: &mut Harness<'_, ThinodaApp>) {
    if h.query_by_label("Shape library").is_none() {
        click(h, Role::Button, "Expand board toolbar");
    }
    click(h, Role::Button, "Shape library");
    assert!(h.query_by_label("Search shapes").is_some());
}

fn pointer(h: &mut Harness<'_, ThinodaApp>, pos: Pos2, pressed: Option<bool>) {
    h.event(egui::Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(1);
}

#[test]
fn shift_enter_on_a_library_tile_inserts_beside_the_selection_without_arming() {
    let (app, original) = app_with_shape();
    let mut h = harness(app);
    let tool = h.state().doc().unwrap().view.board_tool;
    open_library(&mut h);
    assert!(
        h.query_by_label("Drag onto the board or press Shift+Enter to insert")
            .is_some()
    );
    h.get_by_label("Triangle").focus();
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    h.run_steps(4);

    assert!(h.query_by_label("Search shapes").is_none());
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.view.board_tool, tool);
    let objects = objects(h.state());
    assert_eq!(objects.len(), 2);
    let inserted = objects.iter().find(|o| o.id != original).unwrap();
    assert_eq!(shape_kind(inserted), Some(ShapeKind::Triangle));
    assert_eq!(doc.view.board_selection, vec![inserted.id]);
    let source = objects.iter().find(|o| o.id == original).unwrap();
    assert!(inserted.rect.min.x >= source.rect.max().x);
    assert!(!inserted.rect.intersects(&source.rect));

    // A second Shift+Enter continues the row beside the new shape.
    open_library(&mut h);
    h.get_by_label("Diamond").focus();
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    h.run_steps(4);
    let objects_after = self::objects(h.state());
    assert_eq!(objects_after.len(), 3);
    let diamond = objects_after
        .iter()
        .find(|o| shape_kind(o) == Some(ShapeKind::Diamond))
        .unwrap();
    assert!(diamond.rect.min.x >= inserted.rect.max().x);

    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(self::objects(h.state()).len(), 1);
}

#[test]
fn plain_activation_only_arms_and_change_shape_keeps_choosing() {
    let (app, id) = app_with_shape();
    let mut h = harness(app);
    let state = h.state().doc().unwrap().doc.state_id();
    open_library(&mut h);
    h.get_by_label("Triangle").focus();
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(
        h.state().doc().unwrap().view.board_tool,
        BoardTool::Shape(ShapeKind::Triangle)
    );
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);

    let doc = h.state_mut().doc_mut().unwrap();
    doc.view.board_tool = BoardTool::Select;
    doc.view.select_board_object(id);
    h.run_steps(3);
    click(&mut h, Role::Button, "Change shape");
    assert!(
        h.query_by_label("Drag onto the board or press Shift+Enter to insert")
            .is_none()
    );
    h.get_by_role_and_label(Role::Button, "Diamond").focus();
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    h.run_steps(4);
    let objects = objects(h.state());
    assert_eq!(objects.len(), 1);
    assert_eq!(shape_kind(&objects[0]), Some(ShapeKind::Diamond));
}

#[test]
fn dragging_a_library_tile_onto_the_board_creates_one_shape_at_the_drop() {
    let (app, original) = app_with_shape();
    let mut h = harness(app);
    let tool = h.state().doc().unwrap().view.board_tool;
    let before = h.state().doc().unwrap().doc.workbook().clone();
    open_library(&mut h);
    let start = h.get_by_label("Triangle").rect().center();
    pointer(&mut h, start, None);
    pointer(&mut h, start, Some(true));
    for step in 1..=4 {
        pointer(&mut h, start + vec2(0.0, 6.0 * step as f32), None);
    }
    h.run_steps(2);
    assert!(
        h.query_by_label("Search shapes").is_none(),
        "menu closes on drag"
    );
    let drop = Pos2::new(760.0, 520.0);
    pointer(&mut h, drop, None);
    h.run_steps(2);
    assert_eq!(objects(h.state()).len(), 1, "hovering only previews");
    pointer(&mut h, drop, Some(false));
    h.run_steps(3);

    let doc = h.state().doc().unwrap();
    assert_eq!(doc.view.board_tool, tool);
    let objects = objects(h.state());
    assert_eq!(objects.len(), 2);
    let inserted = objects.iter().find(|o| o.id != original).unwrap();
    assert_eq!(shape_kind(inserted), Some(ShapeKind::Triangle));
    assert_eq!(doc.view.board_selection, vec![inserted.id]);
    let source = objects.iter().find(|o| o.id == original).unwrap();
    assert!(inserted.rect.center().x > source.rect.center().x);
    assert!(inserted.rect.center().y > source.rect.center().y);

    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}
