use base64::Engine as _;
use thinoda_core::Vec2;
use thinoda_core::stencil::{Instance, PaintRef, StencilError, StencilRender};

use super::*;

#[test]
fn edited_invalid_choices_use_only_matching_shape_defaults() {
    let entries = [
        (STYLE_PACKAGE_KEY, "thinoda.drawio"),
        (STYLE_DEFINITION_KEY, "bpmn-event"),
        ("thinodaStencilParam.outline", "8"),
        ("outline", "invalid"),
    ];
    let mapped = style_reference(entries, "mxgraph.bpmn.event", true).unwrap();
    assert_eq!(mapped.parameters.get("outline"), Some(&0.0));

    let unrelated = style_reference(entries, "anotherShape", true).unwrap();
    assert_eq!(unrelated.parameters.get("outline"), Some(&8.0));
}

const LIBRARY: &str = r##"<shapes name="Test library">
<shape name="Gate &amp; Port" w="200" h="100" aspect="fixed" strokewidth="inherit">
  <connections>
    <constraint x="0.5" y="0" perimeter="1" name="N"/>
    <constraint x="1" y="0.5" perimeter="1" name="E"/>
    <constraint x="0" y="0" perimeter="0" name="NW"/>
    <constraint x="0.5" y="1" perimeter="1" name="N"/>
  </connections>
  <background>
    <path>
      <move x="0" y="0"/>
      <line x="150" y="0"/>
      <arc rx="50" ry="50" x-axis-rotation="0" large-arc-flag="0" sweep-flag="1" x="150" y="100"/>
      <line x="0" y="100"/>
      <close/>
    </path>
  </background>
  <foreground>
    <fillstroke/>
    <save/>
    <strokecolor color="#f00"/>
    <fillcolor color="fill"/>
    <alpha alpha="0.5"/>
    <strokewidth width="3" fixed="1"/>
    <dashed dashed="1"/>
    <dashpattern pattern="4 2"/>
    <linejoin join="round"/>
    <linecap cap="square"/>
    <miterlimit limit="4"/>
    <roundrect x="10" y="10" w="40" h="30" arcsize="25"/>
    <stroke/>
    <restore/>
    <ellipse x="160" y="40" w="20" h="20"/>
    <fill/>
    <path><move x="0" y="50"/><quad x1="20" y1="30" x2="40" y2="50"/><curve x1="50" y1="40" x2="60" y2="60" x3="70" y3="50"/></path>
    <stroke/>
    <fontsize size="14"/><fontstyle style="3"/><fontfamily family="Arial"/><fontcolor color="#336699"/>
    <text str="IN" x="20" y="80" align="center" valign="middle" rotation="0"/>
    <include-shape name="Dot" x="90" y="40" w="20" h="20"/>
  </foreground>
</shape>
<shape name="Dot" w="10" h="10">
  <background><ellipse x="0" y="0" w="10" h="10"/></background>
  <foreground><fillstroke/></foreground>
</shape>
<shape name="Picture" w="10" h="10">
  <foreground><image src="x.png" x="0" y="0" w="10" h="10"/><include-shape name="mxgraph.unknown" x="0" y="0" w="1" h="1"/></foreground>
</shape>
</shapes>"##;

fn evaluate(
    package: &StencilPackage,
    id: &str,
    w: f64,
    h: f64,
) -> Result<StencilRender, StencilError> {
    let params = BTreeMap::new();
    package.evaluate(
        id,
        &Instance {
            width: w,
            height: h,
            stroke_width: 1.0,
            parameters: &params,
        },
    )
}

#[test]
fn converts_every_drawio_drawing_element() {
    let import = import_drawio_stencils(LIBRARY, "user.test", "").expect("import");
    let package = &import.package;
    assert_eq!(package.title, "Test library");
    let ids: Vec<_> = package.definitions.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, ["gate-port", "dot", "picture"]);

    let gate = &package.definitions[0];
    assert_eq!(gate.frame.aspect, Aspect::Fixed);
    assert_eq!(
        gate.frame.stroke_width,
        StrokeWidthPolicy::Keyword(StrokeKeyword::Inherit)
    );
    assert_eq!(gate.connections.len(), 4);
    // Duplicate constraint names keep the first.
    assert_eq!(gate.connections[3].name, None);
    assert!(gate.requires.is_empty(), "{:?}", gate.requires);
    assert!(
        gate.extensions[SOURCE_EXTENSION]
            .as_str()
            .is_some_and(|s| s.starts_with("<shape name=\"Gate &amp; Port\""))
    );

    let out = evaluate(package, "gate-port", 200.0, 100.0).expect("gate renders");
    assert!(out.paths[0].background);
    // The arc bulges to x = 200 at mid-height.
    assert!(out.contains(Vec2::new(195.0, 50.0)));
    assert!(!out.contains(Vec2::new(195.0, 5.0)));
    let red = out.paths[1].stroke.as_ref().expect("roundrect stroke");
    assert!(matches!(red.color, PaintRef::Rgb(_)));
    assert_eq!(red.dashed, Some(true));
    assert_eq!(red.dash_pattern, vec![4.0, 2.0]);
    assert_eq!(red.join, LineJoin::Round);
    assert_eq!(red.cap, LineCap::Square);
    // Restore brought back the style stroke for later paths.
    assert_eq!(
        out.paths[3].stroke.as_ref().map(|s| s.color.clone()),
        Some(PaintRef::Style("stroke".into()))
    );
    assert_eq!(out.texts.len(), 1);
    assert_eq!(out.texts[0].text, "IN");
    assert_eq!(out.texts[0].font_style, 3);
    // The include of "Dot" draws a circle around (100, 50).
    let dot = out.paths.last().expect("include");
    assert!(
        dot.subpaths[0]
            .points
            .iter()
            .all(|p| (p.x - 100.0).hypot(p.y - 50.0) <= 10.5)
    );
    assert_eq!(out.ports[0].name.as_deref(), Some("N"));
    assert!(out.ports[0].at.y.abs() < 1e-3);
}

#[test]
fn unsupported_elements_become_capabilities_not_silent_skips() {
    let import = import_drawio_stencils(LIBRARY, "user.test", "Mine").expect("import");
    assert_eq!(import.package.title, "Mine");
    let picture = import.package.definition("picture").expect("picture");
    assert_eq!(picture.requires, [CAP_IMAGE, CAP_EXTERNAL_INCLUDE]);
    assert!(matches!(
        evaluate(&import.package, "picture", 10.0, 10.0),
        Err(StencilError::UnsupportedCapability(_))
    ));
    assert!(
        import
            .diagnostics
            .iter()
            .any(|d| d.starts_with("picture: "))
    );
    // Other definitions still render.
    assert!(evaluate(&import.package, "dot", 10.0, 10.0).is_ok());
}

#[test]
fn inline_compressed_stencil_and_single_shape_roots_import() {
    let shape = r#"<shape name="Box" w="10" h="10"><background><rect x="0" y="0" w="10" h="10"/></background><foreground><fillstroke/></foreground></shape>"#;
    let encoded = crate::drawio_clipboard::encode_uri_component(shape);
    let deflated = miniz_oxide::deflate::compress_to_vec(encoded.as_bytes(), 6);
    let payload = base64::engine::general_purpose::STANDARD.encode(deflated);
    let import = import_drawio_stencils(&payload, "user.inline", "").expect("inline");
    assert_eq!(import.package.definitions[0].id, "box");
    assert!(evaluate(&import.package, "box", 20.0, 20.0).is_ok());
}

#[test]
fn rejects_non_stencil_xml_and_tolerates_sloppy_paths() {
    assert!(import_drawio_stencils("<mxGraphModel/>", "user.x", "").is_err());
    assert!(import_drawio_stencils("<shapes/>", "user.x", "").is_err());
    assert!(
        import_drawio_stencils("<!DOCTYPE x><shape/>", "user.x", "").is_err(),
        "DTDs stay rejected"
    );
    let sloppy = r#"<shape w="abc"><foreground><path><line x="5" y="5"/></path><stroke/><blink/></foreground></shape>"#;
    let import = import_drawio_stencils(sloppy, "user.x", "").expect("sloppy");
    let def = &import.package.definitions[0];
    assert_eq!(def.id, "shape-1");
    assert_eq!(def.frame.w, 100.0);
    assert!(def.requires.is_empty(), "draw.io ignores unknown elements");
    assert!(import.diagnostics.iter().any(|d| d.contains("<blink>")));
    assert!(
        import
            .diagnostics
            .iter()
            .any(|d| d.contains("invalid number"))
    );
    assert!(import.diagnostics.iter().any(|d| d.contains("<move>")));
}

#[test]
fn nested_unsupported_elements_and_oversized_payloads_are_not_skipped() {
    let xml = r#"<shapes><shape name="Bg"><background><image src="x.png"/></background><foreground><fill/></foreground></shape><shape name="P"><foreground><path><move x="0" y="0"/><fill/></path><stroke/></foreground></shape></shapes>"#;
    let import = import_drawio_stencils(xml, "user.x", "").expect("import");
    assert_eq!(import.package.definitions[0].requires, [CAP_IMAGE]);
    assert_eq!(
        import.package.definitions[1].requires,
        ["drawio.element.path.fill"],
        "painting inside a path is drawn by draw.io, so it is not dropped"
    );
    let huge = "A".repeat(17 * 1024 * 1024);
    assert!(import_drawio_stencils(&huge, "user.x", "").is_err());
}

#[test]
fn background_state_and_extra_paths_run_before_foreground() {
    let xml = r##"<shapes name="t">
<shape name="Twin" w="10" h="10"><background><save/><fillcolor color="#ff0000"/><rect x="0" y="0" w="10" h="10"/><fillstroke/><ellipse x="2" y="2" w="6" h="6"/></background><foreground><stroke/></foreground></shape>
<shape name="Twin" w="10" h="10"><background><rect x="0" y="0" w="10" h="10"/></background><foreground><fillstroke/></foreground></shape>
</shapes>"##;
    let import = import_drawio_stencils(xml, "user.t", "").expect("import");
    let ids: Vec<_> = import
        .package
        .definitions
        .iter()
        .map(|d| d.id.as_str())
        .collect();
    assert_eq!(ids, ["twin", "twin-2"]);
    let out = evaluate(&import.package, "twin", 10.0, 10.0).expect("renders");
    // The outline is filled with the colour set before it, then the extra
    // background ellipse is stroked by the foreground.
    assert!(out.paths[0].background);
    assert!(matches!(
        out.paths[0].fill.as_ref().map(|f| &f.color),
        Some(PaintRef::Rgb(_))
    ));
    assert_eq!(out.paths.len(), 2);
    assert!(
        import
            .diagnostics
            .iter()
            .any(|d| d.contains("extra geometry"))
    );
    assert!(evaluate(&import.package, "twin-2", 10.0, 10.0).is_ok());
}

#[test]
fn path_children_and_stray_commands_follow_drawio_canvas_semantics() {
    let xml = r##"<shapes name="t">
<shape name="Dot" w="10" h="10"><foreground><path><ellipse x="2" y="2" w="6" h="6"/></path><fillstroke/></foreground></shape>
<shape name="Join" w="10" h="10"><foreground><path><linejoin join="round"/><move x="0" y="0"/><line x="10" y="10"/></path><stroke/></foreground></shape>
<shape name="Truck" w="10" h="10"><foreground><path><move x="0" y="0"/><line x="5" y="0"/></path><move x="5" y="5"/><line x="10" y="10"/><fillstroke/><line x="1" y="1"/></foreground></shape>
<shape name="Odd" w="10" h="10"><foreground><fillstrokecolor color="#fff"/><rect x="0" y="0" w="10" h="10"/><fillalpha alpha="0.5"/><stroke/></foreground></shape>
</shapes>"##;
    let import = import_drawio_stencils(xml, "user.x", "").expect("import");
    let defs = &import.package.definitions;
    assert!(defs.iter().all(|d| d.requires.is_empty()));

    assert!(matches!(defs[0].foreground[0], Op::Ellipse { .. }));

    assert!(matches!(
        defs[1].foreground[0],
        Op::LineJoin {
            join: LineJoin::Round
        }
    ));
    assert!(matches!(defs[1].foreground[1], Op::Path { .. }));

    let Op::Path { commands } = &defs[2].foreground[0] else {
        panic!("path expected");
    };
    assert_eq!(commands.len(), 4, "stray move/line extend the open path");
    assert_eq!(
        defs[2].foreground.len(),
        2,
        "a line after painting is ignored"
    );

    assert!(matches!(defs[3].foreground[0], Op::Rect { .. }));
    assert!(matches!(defs[3].foreground[1], Op::Alpha { .. }));
    for name in ["Odd", "Truck", "Dot"] {
        let id = &defs.iter().find(|d| d.title == name).unwrap().id;
        assert!(
            import
                .diagnostics
                .iter()
                .any(|d| d.starts_with(id.as_str())),
            "{name} discloses its approximation"
        );
    }
    for d in defs {
        let [w, h] = d.default_size();
        let empty = BTreeMap::new();
        let instance = thinoda_core::stencil::Instance {
            width: w,
            height: h,
            stroke_width: 1.0,
            parameters: &empty,
        };
        assert!(import.package.evaluate(&d.id, &instance).is_ok());
    }
}

/// Coverage gate: one synthetic shape per element draw.io's stencil renderer
/// knows. Every one converts without capabilities and draws something.
#[test]
fn every_drawio_stencil_element_converts_and_renders() {
    let xml = r##"<shapes name="gate">
<shape name="Geometry" w="40" h="20" aspect="fixed" strokewidth="inherit"><background><roundrect x="0" y="0" w="40" h="20" arcsize="10"/></background><foreground><fillstroke/><rect x="1" y="1" w="5" h="5"/><fill/><ellipse x="10" y="1" w="5" h="5"/><stroke/></foreground></shape>
<shape name="Paths" w="40" h="20"><foreground><path><move x="0" y="0"/><line x="10" y="0"/><quad x1="15" y1="0" x2="15" y2="5"/><curve x1="15" y1="10" x2="10" y2="15" x3="5" y3="15"/><arc rx="5" ry="5" x-axis-rotation="0" large-arc-flag="0" sweep-flag="1" x="0" y="10"/><close/></path><fillstroke/></foreground></shape>
<shape name="State" w="40" h="20"><foreground><save/><strokecolor color="#ff0000"/><fillcolor color="none"/><alpha alpha="0.5"/><fillalpha alpha="0.4"/><strokealpha alpha="0.3"/><strokewidth width="2" fixed="1"/><dashed dashed="1"/><dashpattern pattern="2 1"/><linejoin join="bevel"/><linecap cap="square"/><miterlimit limit="4"/><rect x="0" y="0" w="40" h="20"/><stroke/><restore/></foreground></shape>
<shape name="Text" w="40" h="20"><foreground><fontcolor color="#000000"/><fontsize size="8"/><fontstyle style="1"/><fontfamily family="Arial"/><text str="Hi" x="20" y="10" align="center" valign="middle" rotation="0"/></foreground></shape>
<shape name="Include" w="40" h="20"><foreground><include-shape name="Geometry" x="0" y="0" w="20" h="10"/></foreground></shape>
<shape name="Connections" w="40" h="20"><background><rect x="0" y="0" w="40" h="20"/></background><foreground><fillstroke/></foreground><connections><constraint x="0" y="0.5" perimeter="1" name="W"/><constraint x="1" y="0.5" perimeter="1" name="E"/></connections></shape>
</shapes>"##;
    let import = import_drawio_stencils(xml, "user.gate", "").expect("import");
    let empty = BTreeMap::new();
    for definition in &import.package.definitions {
        assert!(
            definition.requires.is_empty(),
            "{} requires {:?}",
            definition.title,
            definition.requires
        );
        let [w, h] = definition.default_size();
        let render = import
            .package
            .evaluate(
                &definition.id,
                &Instance {
                    width: w,
                    height: h,
                    stroke_width: 1.0,
                    parameters: &empty,
                },
            )
            .unwrap_or_else(|e| panic!("{}: {e}", definition.title));
        assert!(
            !render.paths.is_empty() || !render.texts.is_empty(),
            "{} draws nothing",
            definition.title
        );
    }
    let connections = &import.package.definitions[5].connections;
    assert_eq!(connections.len(), 2);
    assert!(import.diagnostics.is_empty(), "{:?}", import.diagnostics);
}

#[test]
fn stencil_libraries_alias_shapes_like_drawio_and_keep_includes_together() {
    let xml = r#"<shapes name="mxgraph.ThinodaLib">
<shape name="Dot" w="20" h="20"><foreground><ellipse x="0" y="0" w="20" h="20"/><fillstroke/></foreground></shape>
<shape name="Big Box" w="100" h="50"><foreground><rect x="0" y="0" w="100" h="50"/><stroke/><include-shape name="Dot" x="40" y="15" w="20" h="20"/></foreground></shape>
</shapes>"#;
    let import = import_drawio_stencil_library(xml, "").unwrap();
    assert_eq!(import.namespace.as_deref(), Some("mxgraph.ThinodaLib"));
    let [package] = import.packages.as_slice() else {
        panic!("one package");
    };
    assert_eq!(package.id, "user.drawio.thinodalib");
    let aliases = package.extensions[IMPORT_EXTENSION].as_object().unwrap();
    let big = aliases["mxgraph.thinodalib.big_box"].as_str().unwrap();
    let definition = package.definition(big).unwrap();
    assert_eq!(
        definition.interchange["drawio"]["style"],
        "shape=mxgraph.thinodalib.big_box;"
    );
    assert!(!definition.extensions.contains_key(SOURCE_EXTENSION));
    assert!(aliases.contains_key("mxgraph.thinodalib.dot"));
    package
        .validate(&thinoda_core::stencil::Limits::DEFAULT)
        .unwrap();
}

#[test]
fn oversized_stencil_libraries_split_into_valid_parts() {
    let limit = thinoda_core::stencil::Limits::DEFAULT.max_definitions;
    let mut xml = String::from(r#"<shapes name="mxgraph.huge">"#);
    // An include chain must stay inside one part.
    xml.push_str(r#"<shape name="Base" w="10" h="10"><foreground><rect x="0" y="0" w="10" h="10"/><stroke/></foreground></shape>"#);
    for i in 0..limit + 10 {
        xml.push_str(&format!(
            r#"<shape name="S{i}" w="10" h="10"><foreground><ellipse x="0" y="0" w="10" h="10"/><stroke/></foreground></shape>"#
        ));
    }
    xml.push_str(r#"<shape name="Top" w="10" h="10"><foreground><include-shape name="Base" x="0" y="0" w="10" h="10"/></foreground></shape>"#);
    xml.push_str("</shapes>");
    let import = import_drawio_stencil_library(&xml, "Huge").unwrap();
    assert!(import.packages.len() >= 2, "{}", import.packages.len());
    let mut aliases = 0;
    for (index, part) in import.packages.iter().enumerate() {
        assert_eq!(part.id, format!("user.drawio.huge.part-{}", index + 1));
        part.validate(&thinoda_core::stencil::Limits::DEFAULT)
            .unwrap();
        aliases += part.extensions[IMPORT_EXTENSION].as_object().unwrap().len();
    }
    assert_eq!(aliases, limit + 12);
    let holder = import
        .packages
        .iter()
        .find(|part| part.definition("top").is_some())
        .unwrap();
    assert!(holder.definition("base").is_some());
}
