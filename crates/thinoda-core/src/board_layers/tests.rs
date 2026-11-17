use super::*;
use crate::board::{CanvasObject, ShapeKind, ZOrder};
use crate::geometry::{Rect, Vec2};
use crate::layout::{ApproxMeasure, layout_sheet};
use crate::model::Workbook;
use crate::scene::{SceneId, build_scene};

fn shape(sheet: &mut Sheet, x: f32) -> CanvasObjectId {
    sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(x, 0.0), Vec2::new(40.0, 40.0)),
        ))
        .unwrap()
}

fn order(sheet: &Sheet) -> Vec<CanvasObjectId> {
    sheet
        .canvas
        .objects
        .iter()
        .map(|object| object.id)
        .collect()
}

fn scene_objects(sheet: &Sheet) -> Vec<CanvasObjectId> {
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    build_scene(sheet, &layout)
        .items
        .iter()
        .filter_map(|item| match item.id {
            SceneId::Object(id) => Some(id),
            _ => None,
        })
        .collect()
}

#[test]
fn moving_to_a_layer_keeps_objects_ordered_by_layer_and_z_order_inside_it() {
    let mut sheet = Sheet::new("Board", "Root");
    let a = shape(&mut sheet, 0.0);
    let b = shape(&mut sheet, 50.0);
    let c = shape(&mut sheet, 100.0);
    let top = sheet.add_board_layer("Top").unwrap();
    sheet.move_canvas_objects_to_layer(&[a], Some(top)).unwrap();
    assert_eq!(order(&sheet), [b, c, a]);
    // Bring to front stays below the objects of higher layers.
    sheet.reorder_canvas_objects(&[b], ZOrder::Front).unwrap();
    assert_eq!(order(&sheet), [c, b, a]);
    sheet.move_canvas_objects_to_layer(&[c], Some(top)).unwrap();
    assert_eq!(order(&sheet), [b, a, c]);
    sheet
        .reorder_canvas_objects(&[c], ZOrder::Backward)
        .unwrap();
    assert_eq!(order(&sheet), [b, c, a]);
    let upper = sheet.add_board_layer("Upper").unwrap();
    sheet
        .move_canvas_objects_to_layer(&[b], Some(upper))
        .unwrap();
    assert_eq!(order(&sheet), [c, a, b]);
    sheet.reorder_board_layer(upper, 0).unwrap();
    assert_eq!(order(&sheet), [b, c, a]);
    validate_layers(&sheet.canvas).unwrap();
    sheet.move_canvas_objects_to_layer(&[a], None).unwrap();
    assert_eq!(order(&sheet), [a, b, c]);
    assert_eq!(sheet.canvas.object(a).unwrap().layer, None);
}

#[test]
fn scene_children_follow_their_container_and_cannot_move_alone() {
    let mut sheet = Sheet::new("Board", "Root");
    let container = shape(&mut sheet, 0.0);
    sheet.set_canvas_container(container, true).unwrap();
    let child = shape(&mut sheet, 10.0);
    sheet
        .reparent_canvas_objects(&[child], Some(container))
        .unwrap();
    let other = shape(&mut sheet, 200.0);
    let layer = sheet.add_board_layer("Containers").unwrap();
    let error = sheet
        .move_canvas_objects_to_layer(&[child], Some(layer))
        .unwrap_err();
    assert!(error.contains("container"), "{error}");
    sheet
        .move_canvas_objects_to_layer(&[container], Some(layer))
        .unwrap();
    assert_eq!(sheet.canvas.object(child).unwrap().layer, Some(layer));
    assert_eq!(order(&sheet), [other, container, child]);
    // Reparenting a base-layer object into the container adopts its layer.
    sheet
        .reparent_canvas_objects(&[other], Some(container))
        .unwrap();
    assert_eq!(sheet.canvas.object(other).unwrap().layer, Some(layer));
    validate_layers(&sheet.canvas).unwrap();
}

#[test]
fn hidden_layers_are_not_in_the_scene_and_locked_layers_lock_their_objects() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let (base, layered) = doc
        .edit_sheet("Setup", |sheet| {
            Ok((shape(sheet, 0.0), shape(sheet, 100.0)))
        })
        .unwrap();
    let layer = doc.add_board_layer("Notes").unwrap();
    doc.move_canvas_objects_to_layer(&[layered], Some(layer))
        .unwrap();
    doc.set_board_layer_visible(layer, false).unwrap();
    assert_eq!(scene_objects(doc.sheet()), [base]);
    assert!(doc.sheet().canvas.is_on_hidden_layer(layered));
    doc.undo();
    assert_eq!(scene_objects(doc.sheet()), [base, layered]);

    doc.set_board_layer_locked(layer, true).unwrap();
    assert!(doc.sheet().canvas.is_locked(layered));
    assert!(!doc.sheet().canvas.is_locked(base));
    assert!(doc.remove_canvas_objects(&[layered]).is_err());
    assert!(
        doc.move_canvas_objects_to_layer(&[base], Some(layer))
            .is_err()
    );
    assert!(doc.remove_board_layer(layer).is_err());
    doc.set_board_layer_locked(layer, false).unwrap();
    doc.remove_board_layer(layer).unwrap();
    assert_eq!(doc.sheet().canvas.object(layered).unwrap().layer, None);
    assert!(doc.sheet().canvas.layers.is_empty());
}

#[test]
fn deleting_a_layer_moves_its_objects_on_top_of_the_layer_below() {
    let mut sheet = Sheet::new("Board", "Root");
    let a = shape(&mut sheet, 0.0);
    let b = shape(&mut sheet, 50.0);
    let c = shape(&mut sheet, 100.0);
    let lower = sheet.add_board_layer("Lower").unwrap();
    let upper = sheet.add_board_layer("Upper").unwrap();
    sheet
        .move_canvas_objects_to_layer(&[b], Some(lower))
        .unwrap();
    sheet
        .move_canvas_objects_to_layer(&[c], Some(upper))
        .unwrap();
    sheet.remove_board_layer(upper).unwrap();
    assert_eq!(order(&sheet), [a, b, c]);
    assert_eq!(sheet.canvas.object(c).unwrap().layer, Some(lower));
    validate_layers(&sheet.canvas).unwrap();
}

#[test]
fn validation_rejects_broken_layer_data() {
    let mut sheet = Sheet::new("Board", "Root");
    let a = shape(&mut sheet, 0.0);
    let b = shape(&mut sheet, 50.0);
    let layer = sheet.add_board_layer("Top").unwrap();
    sheet
        .move_canvas_objects_to_layer(&[a], Some(layer))
        .unwrap();
    assert_eq!(order(&sheet), [b, a]);
    validate_layers(&sheet.canvas).unwrap();

    let mut unordered = sheet.canvas.clone();
    unordered.objects.reverse();
    assert!(validate_layers(&unordered).unwrap_err().contains("ordered"));

    let mut missing = sheet.canvas.clone();
    missing.objects[1].layer = Some(BoardLayerId(9999));
    assert!(validate_layers(&missing).unwrap_err().contains("missing"));

    let mut blank = sheet.canvas.clone();
    blank.layers[0].name = "  ".into();
    assert!(validate_layers(&blank).is_err());
    assert!(sheet.rename_board_layer(layer, "a\nb").is_err());
    assert!(sheet.add_board_layer(&"x".repeat(129)).is_err());

    let mut duplicate = sheet.canvas.clone();
    duplicate.layers.push(duplicate.layers[0].clone());
    assert!(
        validate_layers(&duplicate)
            .unwrap_err()
            .contains("duplicate")
    );
}
