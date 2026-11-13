use base64::Engine as _;
use thinoda_core::Vec2;
use thinoda_core::stencil::{Instance, PaintRef, StencilError, StencilRender};

use super::*;

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
    assert_eq!(def.requires, ["drawio.element.blink"]);
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
    let xml = r#"<shapes><shape name="Bg"><background><image src="x.png"/></background><foreground><fill/></foreground></shape><shape name="P"><foreground><path><move x="0" y="0"/><spiral/></path><stroke/></foreground></shape></shapes>"#;
    let import = import_drawio_stencils(xml, "user.x", "").expect("import");
    assert_eq!(import.package.definitions[0].requires, [CAP_IMAGE]);
    assert_eq!(
        import.package.definitions[1].requires,
        ["drawio.element.path.spiral"]
    );
    let huge = "A".repeat(17 * 1024 * 1024);
    assert!(import_drawio_stencils(&huge, "user.x", "").is_err());
}
