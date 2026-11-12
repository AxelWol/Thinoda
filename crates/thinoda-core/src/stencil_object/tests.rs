use super::*;
use crate::board::{ShapeKind, validate_object};

fn message() -> CanvasObject {
    let package = find_package("thinoda.general").expect("built-in package");
    let definition = package.definition("message").expect("message definition");
    new_object(package, definition, Vec2::new(100.0, 100.0))
}

#[test]
fn stencil_object_renders_inside_its_rotated_frame() {
    let mut object = message();
    object.rect = Rect::from_center_size(object.rect.center(), Vec2::new(120.0, 40.0));
    object.rotation = 90.0;
    let render = render(&object).expect("built-in stencil resolves");
    let geometry = geometry(&object, &render);
    assert!(geometry.outline.len() >= 4);
    let rotated = Rect::from_center_size(object.rect.center(), Vec2::new(40.0, 120.0)).expand(1.0);
    assert!(geometry.outline.iter().all(|p| rotated.contains(*p)));
    assert!(
        geometry
            .details
            .iter()
            .flatten()
            .all(|p| rotated.contains(*p))
    );
    let label = label_rect(&object, &render);
    assert!(object.rect.expand(0.5).contains(label.min));
    assert!(object.rect.expand(0.5).contains(label.max()));
}

#[test]
fn stencil_slots_follow_transparent_shape_convention() {
    let object = message();
    let render = render(&object).expect("resolves");
    let slots = StencilSlots {
        fill: None,
        stroke: Some(Color::BLACK),
        font: Color::BLACK,
        opacity: 0.5,
    };
    let (paths, _) = primitives(&object, &render, &slots);
    assert!(!paths.is_empty());
    assert!(paths.iter().all(|p| p.fill.is_none()));
    let stroke = paths[0].stroke.as_ref().expect("stroked");
    assert_eq!(stroke.color.a, 128);
}

#[test]
fn unknown_stencil_falls_back_to_legacy_shape() {
    let mut object = CanvasObject::shape(
        ShapeKind::Ellipse,
        Rect::from_center_size(Vec2::ZERO, Vec2::new(40.0, 20.0)),
    );
    object.stencil = Some(StencilRef::new("vendor.missing", "thing"));
    assert!(render(&object).is_none());
    assert!(validate_object(&object).is_ok());
}

#[test]
fn stencil_reference_validation_rejects_bad_data() {
    let mut object = message();
    object
        .stencil
        .as_mut()
        .unwrap()
        .parameters
        .insert("dx".into(), f64::NAN);
    assert!(validate_object(&object).is_err());
    let mut object = message();
    object.stencil.as_mut().unwrap().package = String::new();
    assert!(validate_object(&object).is_err());
    let mut object = message();
    object.double_ellipse_margin = Some(2.0);
    assert!(validate_object(&object).is_err());
    let json = r#"{"package":"a","definition":"b","extra":1}"#;
    assert!(serde_json::from_str::<StencilRef>(json).is_err());
}

#[test]
fn stencil_reference_round_trips_and_is_omitted_when_absent() {
    let object = message();
    let json = serde_json::to_value(&object).unwrap();
    assert_eq!(json["stencil"]["package"], "thinoda.general");
    assert!(json["stencil"].get("parameters").is_none());
    let back: CanvasObject = serde_json::from_value(json).unwrap();
    assert_eq!(back, object);
    let plain = CanvasObject::shape(ShapeKind::Rectangle, object.rect);
    assert!(
        serde_json::to_value(&plain)
            .unwrap()
            .get("stencil")
            .is_none()
    );
}
