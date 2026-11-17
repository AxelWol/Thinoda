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
fn stencil_handles_map_rotated_sheet_pointer_to_clamped_parameter_value() {
    let package = find_package("thinoda.general").unwrap();
    let definition = package.definition("rounded-rectangle").unwrap();
    let mut object = new_object(package, definition, Vec2::ZERO);
    object.rect = Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 50.0));

    assert_eq!(
        drag_handle(&object, 0, Vec2::new(15.0, 0.0)).unwrap(),
        ("arcSize".into(), 30.0)
    );
    object.rotation = 90.0;
    assert_eq!(
        drag_handle(&object, 0, Vec2::new(75.0, 25.0)).unwrap(),
        ("arcSize".into(), 50.0)
    );
    assert!(drag_handle(&object, 1, Vec2::ZERO).is_err());

    object.rotation = 0.0;
    let updated = with_handle_drag(&object, 0, Vec2::new(15.0, 0.0)).unwrap();
    assert_eq!(
        updated.stencil.as_ref().unwrap().parameters.get("arcSize"),
        Some(&30.0)
    );
    let reset = with_handle_drag(&updated, 0, Vec2::new(7.5, 0.0)).unwrap();
    assert!(reset.stencil.as_ref().unwrap().parameters.is_empty());
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
fn stencil_definition_hash_mismatch_falls_back_without_rendering_another_revision() {
    let mut object = message();
    let expected = object
        .stencil
        .as_ref()
        .unwrap()
        .definition_hash
        .as_ref()
        .unwrap();
    assert_eq!(expected.len(), 64);
    object.stencil.as_mut().unwrap().definition_hash = Some("0".repeat(64));
    assert!(render(&object).is_none());
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
    object.stencil.as_mut().unwrap().definition_hash = Some("not-a-hash".into());
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
    assert_eq!(
        json["stencil"]["definitionHash"],
        StencilPackage::definition_hash(
            find_package("thinoda.general")
                .unwrap()
                .definition("message")
                .unwrap()
        )
    );
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
