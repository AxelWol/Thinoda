use super::*;
use thinoda_core::{
    Band, CanvasObject, CanvasObjectId, Card, CardKind, SceneHit, ShapeKind,
    board::rotated_corners, scene::build_scene,
};

#[test]
fn topmost_canvas_object_blocks_topic_hit() {
    let mut sheet = Sheet::new("Board", "Topic");
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let rect = layout.rect(sheet.root).unwrap();
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect);
    object.id = CanvasObjectId(20);
    object.band = Band::Above;
    sheet.canvas.objects.push(object);

    let scene = build_scene(&sheet, &layout);
    let point = rect.center();
    assert_eq!(
        scene.hit_test(point),
        Some(SceneHit::Object(CanvasObjectId(20)))
    );
    assert_eq!(scene_topic_hit(&scene, point), None);
}

#[test]
fn rotated_card_hit_matches_painted_card_geometry() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
    let card = sheet.add_floating("Sticky", Vec2::new(120.0, 80.0));
    sheet.topic_mut(card).unwrap().card = Some(Card {
        kind: CardKind::Sticky,
        size: Vec2::new(140.0, 90.0),
        rotation: 45.0,
        color: None,
    });
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let rect = layout.rect(card).unwrap();
    let corner = rotated_corners(rect, 45.0)[0];
    assert!(!rect.contains(corner));

    let scene = build_scene(&sheet, &layout);
    assert_eq!(scene_topic_hit(&scene, corner), Some(card));
}
