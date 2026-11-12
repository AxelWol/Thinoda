use super::*;

fn package(definitions: &str) -> String {
    format!(
        r#"{{"schema":"thinoda.stencil-package","schemaVersion":1,"dslVersion":1,
            "id":"test.pkg","title":"Test","definitions":[{definitions}]}}"#
    )
}

fn load(definitions: &str) -> StencilPackage {
    StencilPackage::from_json(&package(definitions)).expect("package should load")
}

fn load_err(definitions: &str) -> StencilError {
    StencilPackage::from_json(&package(definitions)).expect_err("package should be rejected")
}

fn render(pkg: &StencilPackage, id: &str, w: f64, h: f64) -> StencilRender {
    let params = BTreeMap::new();
    pkg.evaluate(
        id,
        &Instance {
            width: w,
            height: h,
            stroke_width: 2.0,
            parameters: &params,
        },
    )
    .expect("definition should evaluate")
}

fn close(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
}

const RECT_BOX: &str = r#"{"op":"rect","x":0,"y":0,"w":100,"h":100}"#;

#[test]
fn builtin_library_loads_and_every_definition_evaluates() {
    assert_eq!(builtin_packages().len(), builtin_sources().len());
    let general = &builtin_packages()[0];
    assert_eq!(general.id, "thinoda.general");
    for definition in &general.definitions {
        let [w, h] = definition.default_size();
        let params = definition.resolve_parameters(&BTreeMap::new());
        let out = general
            .evaluate(
                &definition.id,
                &Instance {
                    width: w,
                    height: h,
                    stroke_width: 1.0,
                    parameters: &params,
                },
            )
            .unwrap_or_else(|e| panic!("{} failed: {e}", definition.id));
        assert!(!out.paths.is_empty(), "{} paints nothing", definition.id);
        assert!(
            out.paths[0].background,
            "{} background unpainted",
            definition.id
        );
        assert!(
            out.contains(Vec2::new(w as f32 / 2.0, h as f32 * 0.6)),
            "{} centre is not inside",
            definition.id
        );
        assert_eq!(out.handles.len(), definition.handles.len());
        assert!(
            definition.interchange.contains_key("drawio"),
            "{} lacks a draw.io mapping",
            definition.id
        );
    }
}

#[test]
fn json_round_trips_and_hash_is_stable() {
    let general = &builtin_packages()[0];
    let again = StencilPackage::from_json(&general.to_json()).expect("round trip");
    assert_eq!(&again, general);
    let a = StencilPackage::definition_hash(&general.definitions[0]);
    let b = StencilPackage::definition_hash(&again.definitions[0]);
    assert_eq!(a, b);
    assert_ne!(a, StencilPackage::definition_hash(&general.definitions[1]));
}

#[test]
fn rejects_unknown_ops_fields_and_versions() {
    assert!(matches!(
        load_err(r#"{"id":"a","title":"A","foreground":[{"op":"explode"}]}"#),
        StencilError::Invalid(_)
    ));
    assert!(matches!(
        load_err(r#"{"id":"a","title":"A","color":"red"}"#),
        StencilError::Invalid(_)
    ));
    let newer = package("").replace(r#""dslVersion":1"#, r#""dslVersion":99"#);
    assert!(matches!(
        StencilPackage::from_json(&newer),
        Err(StencilError::UnsupportedVersion(_))
    ));
    let needs = package("").replace(
        r#""definitions""#,
        r#""requires":["future.physics"],"definitions""#,
    );
    assert!(matches!(
        StencilPackage::from_json(&needs),
        Err(StencilError::UnsupportedCapability(_))
    ));
}

#[test]
fn rejects_semantic_errors() {
    // Unknown variable.
    assert!(matches!(
        load_err(
            r#"{"id":"a","title":"A","background":{"op":"rect","x":0,"y":0,"w":"wat","h":1}}"#
        ),
        StencilError::Invalid(_)
    ));
    // Wrong arity.
    assert!(matches!(
        load_err(
            r#"{"id":"a","title":"A","background":{"op":"rect","x":{"op":"sub","args":[1]},"y":0,"w":1,"h":1}}"#
        ),
        StencilError::Invalid(_)
    ));
    // Background must be geometry.
    assert!(matches!(
        load_err(r#"{"id":"a","title":"A","background":{"op":"fill"}}"#),
        StencilError::Invalid(_)
    ));
    // Duplicate ids.
    assert!(matches!(
        load_err(r#"{"id":"a","title":"A"},{"id":"a","title":"B"}"#),
        StencilError::Invalid(_)
    ));
    // Include cycle.
    assert!(matches!(
        load_err(
            r#"{"id":"a","title":"A","foreground":[{"op":"include","definition":"b","x":0,"y":0,"w":1,"h":1}]},
               {"id":"b","title":"B","foreground":[{"op":"include","definition":"a","x":0,"y":0,"w":1,"h":1}]}"#
        ),
        StencilError::Invalid(_)
    ));
    // Path must start with move.
    assert!(matches!(
        load_err(
            r#"{"id":"a","title":"A","background":{"op":"path","commands":[{"op":"line","x":1,"y":1}]}}"#
        ),
        StencilError::Invalid(_)
    ));
    // Extension keys must be namespaced.
    assert!(matches!(
        load_err(r#"{"id":"a","title":"A","extensions":{"plain":1}}"#),
        StencilError::Invalid(_)
    ));
}

#[test]
fn division_by_zero_is_reported_not_rendered() {
    let pkg = load(
        r#"{"id":"a","title":"A","background":{"op":"rect","x":0,"y":0,"w":{"op":"div","args":[1,0]},"h":1},
            "foreground":[{"op":"fill"}]}"#,
    );
    let params = BTreeMap::new();
    let result = pkg.evaluate(
        "a",
        &Instance {
            width: 10.0,
            height: 10.0,
            stroke_width: 1.0,
            parameters: &params,
        },
    );
    assert!(matches!(result, Err(StencilError::NonFinite(_))));
}

#[test]
fn budgets_bound_evaluation() {
    let pkg = load(&format!(
        r#"{{"id":"a","title":"A","foreground":[{}]}}"#,
        vec![r#"{"op":"ellipse","x":0,"y":0,"w":100,"h":100},{"op":"stroke"}"#; 40].join(",")
    ));
    let params = BTreeMap::new();
    let instance = Instance {
        width: 10.0,
        height: 10.0,
        stroke_width: 1.0,
        parameters: &params,
    };
    let tight = Limits {
        max_points: 100,
        ..Limits::DEFAULT
    };
    assert!(matches!(
        pkg.evaluate_with_limits("a", &instance, &tight),
        Err(StencilError::Limit(_))
    ));
    assert!(pkg.evaluate("a", &instance).is_ok());
}

#[test]
fn unsupported_definition_capability_fails_only_that_definition() {
    let pkg = load(&format!(
        r#"{{"id":"img","title":"Image","requires":["drawio.image"]}},
           {{"id":"box","title":"Box","background":{RECT_BOX},"foreground":[{{"op":"fill"}}]}}"#
    ));
    let params = BTreeMap::new();
    let instance = Instance {
        width: 10.0,
        height: 10.0,
        stroke_width: 1.0,
        parameters: &params,
    };
    assert_eq!(
        pkg.evaluate("img", &instance),
        Err(StencilError::UnsupportedCapability(vec![
            "drawio.image".into()
        ]))
    );
    assert!(pkg.evaluate("box", &instance).is_ok());
}

#[test]
fn fixed_aspect_scales_uniformly_and_centres() {
    let pkg = load(&format!(
        r#"{{"id":"a","title":"A","frame":{{"w":100,"h":100,"aspect":"fixed"}},
            "background":{RECT_BOX},"foreground":[{{"op":"fill"}}]}}"#
    ));
    let out = render(&pkg, "a", 200.0, 100.0);
    let points = &out.outline[0].points;
    assert!(close(points[0], Vec2::new(50.0, 0.0)), "{points:?}");
    assert!(points.iter().any(|p| close(*p, Vec2::new(150.0, 100.0))));
}

#[test]
fn stroke_width_inherit_and_scale_policies() {
    let def = |policy: &str| {
        format!(
            r#"{{"id":"a","title":"A","frame":{{"w":100,"h":100,"strokeWidth":{policy}}},
                "background":{RECT_BOX},"foreground":[{{"op":"stroke"}}]}}"#
        )
    };
    let inherit = render(&load(&def(r#""inherit""#)), "a", 50.0, 50.0);
    assert_eq!(inherit.paths[0].stroke.as_ref().map(|s| s.width), Some(2.0));
    // Numeric policy follows draw.io: factor x frame scale (0.5); the style
    // width only applies to `inherit`.
    let scaled = render(&load(&def("2")), "a", 50.0, 50.0);
    assert_eq!(scaled.paths[0].stroke.as_ref().map(|s| s.width), Some(1.0));
    let scaled = render(&load(&def("4")), "a", 50.0, 50.0);
    assert_eq!(scaled.paths[0].stroke.as_ref().map(|s| s.width), Some(2.0));
}

#[test]
fn coordinates_overflowing_f32_are_rejected() {
    let pkg = load(
        r#"{"id":"a","title":"A","frame":{"units":"instance"},
            "background":{"op":"rect","x":0,"y":0,"w":1e100,"h":1},"foreground":[{"op":"fill"}]}"#,
    );
    let params = BTreeMap::new();
    let result = pkg.evaluate(
        "a",
        &Instance {
            width: 10.0,
            height: 10.0,
            stroke_width: 1.0,
            parameters: &params,
        },
    );
    assert!(
        matches!(result, Err(StencilError::NonFinite(_))),
        "{result:?}"
    );
}

#[test]
fn fixed_aspect_ports_follow_the_centred_frame() {
    let pkg = load(&format!(
        r#"{{"id":"a","title":"A","frame":{{"w":100,"h":100,"aspect":"fixed"}},
            "background":{RECT_BOX},"foreground":[{{"op":"fill"}}],
            "connections":[{{"x":0,"y":0}},{{"x":1,"y":0.5,"perimeter":true}}]}}"#
    ));
    let out = render(&pkg, "a", 200.0, 100.0);
    assert!(
        close(out.ports[0].at, Vec2::new(50.0, 0.0)),
        "{:?}",
        out.ports
    );
    assert!(
        close(out.ports[1].at, Vec2::new(150.0, 50.0)),
        "{:?}",
        out.ports
    );
}

#[test]
fn include_uses_child_scale_and_restores_parent_paint() {
    let pkg = load(&format!(
        r##"{{"id":"inner","title":"Inner","frame":{{"w":100,"h":100,"strokeWidth":2}},
             "background":{RECT_BOX},"foreground":[{{"op":"strokeColor","color":"#ff0000"}},{{"op":"stroke"}}]}},
           {{"id":"outer","title":"Outer","frame":{{"units":"instance","strokeWidth":3}},
             "foreground":[{{"op":"include","definition":"inner","x":0,"y":0,"w":50,"h":50}},
                           {RECT_BOX},{{"op":"stroke"}}]}}"##
    ));
    let out = render(&pkg, "outer", 100.0, 100.0);
    let inner = out.paths[0].stroke.as_ref().expect("inner stroke");
    assert_eq!(inner.width, 1.0, "2 x child scale 0.5");
    let outer = out.paths[1].stroke.as_ref().expect("outer stroke");
    assert_eq!(outer.width, 3.0);
    assert_eq!(outer.color, PaintRef::Style("stroke".into()));
}

#[test]
fn folder_separator_closes_the_tab() {
    let general = &builtin_packages()[0];
    let folder = general.definition("folder").expect("folder");
    for (position, x0, x1) in [(0.0, 0.0, 40.0), (1.0, 80.0, 120.0)] {
        let params =
            folder.resolve_parameters(&BTreeMap::from([("tabPosition".to_string(), position)]));
        let out = general
            .evaluate(
                "folder",
                &Instance {
                    width: 120.0,
                    height: 80.0,
                    stroke_width: 1.0,
                    parameters: &params,
                },
            )
            .expect("folder");
        let separator = &out.paths[1].subpaths[0].points;
        assert!(close(separator[0], Vec2::new(x0, 14.0)), "{separator:?}");
        assert!(close(separator[1], Vec2::new(x1, 14.0)), "{separator:?}");
    }
}

#[test]
fn first_paint_consumes_background_and_later_geometry_replaces_it() {
    let pkg = load(&format!(
        r#"{{"id":"a","title":"A","background":{RECT_BOX},"foreground":[
            {{"op":"fillStroke"}},
            {{"op":"stroke"}},
            {{"op":"ellipse","x":0,"y":0,"w":10,"h":10}},
            {{"op":"fill"}}
        ]}}"#
    ));
    let out = render(&pkg, "a", 100.0, 100.0);
    // The second stroke has no current path and paints nothing.
    assert_eq!(out.paths.len(), 2);
    assert!(out.paths[0].background);
    assert!(out.paths[0].fill.is_some() && out.paths[0].stroke.is_some());
    assert!(!out.paths[1].background);
    assert!(out.paths[1].fill.is_some() && out.paths[1].stroke.is_none());
}

#[test]
fn save_restore_scopes_colors() {
    let pkg = load(
        r##"{"id":"a","title":"A","foreground":[
            {"op":"save"},
            {"op":"fillColor","color":"#ff0000"},
            {"op":"rect","x":0,"y":0,"w":10,"h":10},{"op":"fill"},
            {"op":"restore"},
            {"op":"restore"},
            {"op":"rect","x":0,"y":0,"w":10,"h":10},{"op":"fill"}
        ]}"##,
    );
    let out = render(&pkg, "a", 100.0, 100.0);
    let fills: Vec<_> = out
        .paths
        .iter()
        .map(|p| p.fill.as_ref().map(|f| f.color.clone()))
        .collect();
    assert!(matches!(fills[0], Some(PaintRef::Rgb(_))));
    assert_eq!(fills[1], Some(PaintRef::Style("fill".into())));
}

#[test]
fn roundrect_radius_uses_smaller_side_percentage() {
    let pkg = load(
        r#"{"id":"a","title":"A","frame":{"units":"instance"},
            "background":{"op":"roundrect","x":0,"y":0,"w":"w","h":"h","arcsize":20},
            "foreground":[{"op":"fill"}]}"#,
    );
    let out = render(&pkg, "a", 200.0, 50.0);
    let points = &out.outline[0].points;
    // Radius = min(200, 50) * 0.2 = 10: no point is closer to the corner.
    assert!(points.iter().all(|p| p.x >= 0.0 && p.y >= 0.0));
    assert!(points.iter().any(|p| close(*p, Vec2::new(10.0, 0.0))));
    assert!(points.iter().any(|p| close(*p, Vec2::new(0.0, 10.0))));
    assert!(!points.iter().any(|p| close(*p, Vec2::ZERO)));
}

#[test]
fn arcs_reach_their_endpoints() {
    let pkg = load(
        r#"{"id":"a","title":"A","frame":{"units":"instance"},"foreground":[
            {"op":"path","commands":[
                {"op":"move","x":0,"y":50},
                {"op":"arc","rx":50,"ry":50,"largeArcFlag":0,"sweepFlag":1,"x":100,"y":50}
            ]},
            {"op":"stroke"}
        ]}"#,
    );
    let out = render(&pkg, "a", 100.0, 100.0);
    let points = &out.paths[0].subpaths[0].points;
    assert!(close(
        *points.last().expect("points"),
        Vec2::new(100.0, 50.0)
    ));
    // Sweep 1 in a y-down space bulges upwards through the top.
    assert!(
        points
            .iter()
            .any(|p| (p.y - 0.0).abs() < 0.5 && (p.x - 50.0).abs() < 3.0)
    );
}

#[test]
fn message_ports_match_drawio_constraints() {
    let general = &builtin_packages()[0];
    let out = render(general, "message", 60.0, 40.0);
    assert_eq!(out.ports.len(), 12);
    let expected = [
        (9.0, 2.0),
        (30.0, 0.0),
        (51.0, 2.0),
        (0.0, 12.0),
        (0.0, 20.0),
        (0.0, 28.0),
        (60.0, 12.0),
        (60.0, 20.0),
        (60.0, 28.0),
        (9.0, 38.0),
        (30.0, 40.0),
        (51.0, 38.0),
    ];
    for (port, (x, y)) in out.ports.iter().zip(expected) {
        assert!(close(port.at, Vec2::new(x, y)), "{port:?} != ({x},{y})");
    }
}

#[test]
fn perimeter_ports_project_onto_the_outline() {
    let pkg = load(
        r#"{"id":"a","title":"A","background":{"op":"ellipse","x":0,"y":0,"w":100,"h":100},
            "foreground":[{"op":"fill"}],
            "connections":[{"x":1,"y":0,"perimeter":true},{"x":1,"y":0}]}"#,
    );
    let out = render(&pkg, "a", 100.0, 100.0);
    let projected = out.ports[0].at;
    let r = ((projected.x - 50.0).powi(2) + (projected.y - 50.0).powi(2)).sqrt();
    assert!((r - 50.0).abs() < 1.0, "{projected:?}");
    assert!(close(out.ports[1].at, Vec2::new(100.0, 0.0)));
}

#[test]
fn handle_drag_updates_and_clamps_parameters() {
    let general = &builtin_packages()[0];
    let params = BTreeMap::new();
    let instance = Instance {
        width: 80.0,
        height: 100.0,
        stroke_width: 1.0,
        parameters: &params,
    };
    let (name, value) = general
        .drag_handle("card", 0, &instance, Vec2::new(12.0, 0.0))
        .expect("drag");
    assert_eq!((name.as_str(), value), ("size", 12.0));
    let (_, value) = general
        .drag_handle("card", 0, &instance, Vec2::new(-30.0, 0.0))
        .expect("drag");
    assert_eq!(value, 0.0);
    assert!(
        general
            .drag_handle("card", 9, &instance, Vec2::ZERO)
            .is_err()
    );
}

#[test]
fn parameters_drive_geometry_and_choices() {
    let general = &builtin_packages()[0];
    let folder = general.definition("folder").expect("folder");
    let left = folder.resolve_parameters(&BTreeMap::new());
    let right = folder.resolve_parameters(&BTreeMap::from([("tabPosition".to_string(), 1.0)]));
    let eval = |params: &BTreeMap<String, f64>| {
        general
            .evaluate(
                "folder",
                &Instance {
                    width: 120.0,
                    height: 80.0,
                    stroke_width: 1.0,
                    parameters: params,
                },
            )
            .expect("folder")
    };
    let (l, r) = (eval(&left), eval(&right));
    // The tab sits at the top-left or top-right.
    assert!(l.contains(Vec2::new(10.0, 5.0)) && !l.contains(Vec2::new(110.0, 5.0)));
    assert!(r.contains(Vec2::new(110.0, 5.0)) && !r.contains(Vec2::new(10.0, 5.0)));
    // Out-of-range overrides are clamped.
    let clamped = folder.resolve_parameters(&BTreeMap::from([("tabWidth".to_string(), -5.0)]));
    assert_eq!(clamped["tabWidth"], 0.0);
}

#[test]
fn include_draws_into_a_box() {
    let pkg = load(&format!(
        r#"{{"id":"inner","title":"Inner","background":{RECT_BOX},"foreground":[{{"op":"fill"}}]}},
           {{"id":"outer","title":"Outer","frame":{{"units":"instance"}},
             "foreground":[{{"op":"include","definition":"inner","x":10,"y":20,"w":30,"h":40}}]}}"#
    ));
    let out = render(&pkg, "outer", 100.0, 100.0);
    let points = &out.paths[0].subpaths[0].points;
    assert!(points.iter().any(|p| close(*p, Vec2::new(10.0, 20.0))));
    assert!(points.iter().any(|p| close(*p, Vec2::new(40.0, 60.0))));
}

#[test]
fn hit_testing_follows_the_outline() {
    let general = &builtin_packages()[0];
    let out = render(general, "card", 80.0, 100.0);
    assert!(!out.contains(Vec2::new(2.0, 2.0)), "cut corner is outside");
    assert!(out.contains(Vec2::new(40.0, 50.0)));
    let rotated = out.to_sheet(
        Vec2::new(80.0, 50.0),
        Rect::from_min_size(Vec2::ZERO, Vec2::new(80.0, 100.0)),
        90.0,
    );
    assert!(close(rotated, Vec2::new(40.0, 90.0)), "{rotated:?}");
}
