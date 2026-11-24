use super::*;
use crate::{ExportLoss, ExportOptions, detect_format, export_bytes, export_losses};
use std::{collections::BTreeMap, path::Path};
use thinoda_core::{PresentationSource, board::CanvasCardState};

/// Written by draw.io's algorithm (deflateRaw of encodeURIComponent) in Python,
/// independently of this codec.
const GOLDEN_COMPRESSED: &str = "xZTNboMwDMefJndKpqo7rqzdaSeeICUuiRYIMm5L9/RzICtUtPuQJu2AiD/+TvzDQcis6l5QNebVa3AiTUq0WshnkaYLfqIjt+8wONMkeOVGyAy9p2FVdRm4IP6U9knp9k40Fm4UQk0/EahBcFTuEI+Rk0ISQbZUVSPkOhzUD2ktnV1M4zK2adlYn4wlyBtVhMiJG2afocqxteDl3jqXeeex10mtYLUv2N8S+jeYRJbFCnb7fsP+SIAE3d22FhdYTBl8BYRnTomCh6Hr5HxtnqwmE/URTGLAliYWXUafage7vBQeEfIiUrxNdDcjKjIpHrM6vJ62BQ/DDCb6Q61BR2LfAP0DPP/Jp5iPki4hj6ZHMr70tXKb0bse+SS3WIQCX5Pg/fwBC7gaeh70Emjy1ea8EJwie7yu/ovu2Rzvch+b/BHk5gM=";

fn page(cells: &str) -> String {
    format!(
        "<mxfile><diagram name=\"P\"><mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>{cells}</root></mxGraphModel></diagram></mxfile>"
    )
}

fn load(xml: &str) -> ImportReport {
    import(xml.as_bytes()).unwrap_or_else(|e| panic!("{e}"))
}

fn clean(xml: &str) -> Sheet {
    let report = load(xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    report.workbook.sheets.into_iter().next().unwrap()
}

fn shapes(sheet: &Sheet) -> Vec<&CanvasObject> {
    sheet
        .canvas
        .objects
        .iter()
        .filter(|o| matches!(o.kind, CanvasObjectKind::Shape { .. }))
        .collect()
}

fn connectors(sheet: &Sheet) -> Vec<&CanvasConnector> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|o| match &o.kind {
            CanvasObjectKind::Connector(c) => Some(c),
            _ => None,
        })
        .collect()
}

/// Native version an import needs for its features, ignoring the draw.io
/// passthrough (ids, unknown keys) that every imported page now carries.
fn feature_version(book: &Workbook) -> u64 {
    let mut book = book.clone();
    book.drawio = None;
    for sheet in &mut book.sheets {
        sheet.drawio = None;
        for object in &mut sheet.canvas.objects {
            object.drawio = None;
        }
    }
    crate::required_native_version(&book)
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h))
}

#[test]
fn drawio_relative_html_font_weights_warn_and_require_confirmation() {
    for (parent_weight, weight, relative) in [
        ("100", "bolder", true),
        ("900", "lighter", true),
        ("400", "bolder", true),
        ("700", "lighter", true),
        ("400", "bold", false),
        ("700", "normal", false),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="&lt;span style=&quot;font-weight:{parent_weight}&quot;&gt;&lt;span style=&quot;font-weight:{weight}&quot;&gt;Text&lt;/span&gt;&lt;/span&gt;" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Text");
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            relative,
            "{weight}: {:?}",
            report.warnings
        );
        if relative {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings.contains(&ImportWarning::Formatting)
            ));
            assert!(
                crate::drawio_clipboard::decode(&xml)
                    .unwrap()
                    .warnings
                    .contains(&ImportWarning::Formatting)
            );
        }
    }
}

#[test]
fn drawio_html_nonstandard_numeric_font_weights_warn() {
    for (weight, approximate) in [
        ("100", true),
        ("401", true),
        ("500", true),
        ("600", true),
        ("900", true),
        ("400", false),
        ("700", false),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="&lt;span style=&quot;font-weight:{weight}&quot;&gt;Text&lt;/span&gt;" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Text");
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            approximate,
            "{weight}: {:?}",
            report.warnings
        );
        if approximate {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings.contains(&ImportWarning::Formatting)
            ));
            assert!(
                crate::drawio_clipboard::decode(&xml)
                    .unwrap()
                    .warnings
                    .contains(&ImportWarning::Formatting)
            );
        }
    }
}

#[test]
fn drawio_html_oblique_font_style_warns_about_italic_approximation() {
    for (font_style, approximate) in [("oblique", true), ("italic", false), ("normal", false)] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="&lt;span style=&quot;font-style:{font_style}&quot;&gt;Text&lt;/span&gt;" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        let shape = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(shape.label(), "Text");
        assert_eq!(
            shape.label_rich_text.as_ref().unwrap().runs[0].style.italic,
            Some(font_style != "normal")
        );
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            approximate,
            "{font_style}: {:?}",
            report.warnings
        );
        if approximate {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings.contains(&ImportWarning::Formatting)
            ));
            assert!(
                crate::drawio_clipboard::decode(&xml)
                    .unwrap()
                    .warnings
                    .contains(&ImportWarning::Formatting)
            );
        }
    }
}

#[test]
fn drawio_html_relative_font_sizes_use_the_parent_size() {
    for (parent, child, expected, approximate) in [
        ("20px", "150%", Some(30.0), false),
        ("20px", "0.5em", Some(10.0), false),
        ("20px", "100%", Some(20.0), false),
        ("", "1em", None, false),
        ("", "150%", Some(12.0), true),
        ("20px", "2000%", Some(12.0), true),
        ("20px", "-1em", Some(12.0), true),
    ] {
        let parent_style = if parent.is_empty() {
            String::new()
        } else {
            format!("font-size:{parent}")
        };
        let xml = page(&format!(
            r#"<mxCell id="a" value="&lt;span style=&quot;{parent_style}&quot;&gt;&lt;span style=&quot;font-size:12px;font-size:{child}&quot;&gt;Text&lt;/span&gt;&lt;/span&gt;" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            approximate,
            "{parent}, {child}: {:?}",
            report.warnings
        );
        let shape = shapes(&report.workbook.sheets[0])[0];
        let size = shape
            .label_rich_text
            .as_ref()
            .and_then(|rich| rich.runs.first())
            .and_then(|run| run.style.font_size);
        assert_eq!(size, expected, "{parent}, {child}");
        if approximate {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings.contains(&ImportWarning::Formatting)
            ));
            assert!(
                crate::drawio_clipboard::decode(&xml)
                    .unwrap()
                    .warnings
                    .contains(&ImportWarning::Formatting)
            );
        }
    }
}

#[test]
fn drawio_html_relative_font_sizes_use_explicit_whole_label_size() {
    let xml = page(
        r#"<mxCell id="a" value="&lt;span style=&quot;font-size:150%&quot;&gt;A&lt;span style=&quot;font-size:0.5em&quot;&gt;B&lt;/span&gt;&lt;/span&gt;C" style="shape=rectangle;html=1;fontSize=20;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert!(
        !report.warnings.contains(&ImportWarning::Formatting),
        "{:?}",
        report.warnings
    );
    let shape = shapes(&report.workbook.sheets[0])[0];
    assert_eq!(shape.label(), "ABC");
    let rich = shape.label_rich_text.as_ref().unwrap();
    assert_eq!(rich.runs[0].style.font_size, Some(30.0));
    assert_eq!(rich.runs[1].style.font_size, Some(15.0));
    assert!(
        !rich
            .runs
            .iter()
            .any(|run| run.start <= 2 && run.end > 2 && run.style.font_size.is_some())
    );
    assert!(crate::import_bytes(Format::DrawIo, xml.as_bytes()).is_ok());
}

#[test]
fn drawio_html_relative_font_size_uses_suppressed_stylesheet_default() {
    let xml = page(
        r#"<mxCell id="a" value="&lt;span style=&quot;font-size:150%&quot;&gt;Text&lt;/span&gt;" style=";shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    let shape = shapes(&report.workbook.sheets[0])[0];
    assert_eq!(
        shape.label_rich_text.as_ref().unwrap().runs[0]
            .style
            .font_size,
        Some(16.5)
    );
    assert!(
        !report.warnings.contains(&ImportWarning::Formatting),
        "{:?}",
        report.warnings
    );
}

#[test]
fn drawio_html_inherited_font_properties_keep_parent_values() {
    for keyword in ["inherit", "unset"] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="&lt;span style=&quot;font-family:Courier New;font-size:16px;font-weight:normal;font-style:italic;color:#ff0000&quot;&gt;&lt;b style=&quot;font-family:Arial;font-family:{keyword};font-size:20px;font-size:{keyword};font-weight:{keyword};font-style:normal;font-style:{keyword};color:#0000ff;color:{keyword}&quot;&gt;Text&lt;/b&gt;&lt;/span&gt;" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(
            !report.warnings.contains(&ImportWarning::Formatting),
            "{keyword}: {:?}",
            report.warnings
        );
        let shape = shapes(&report.workbook.sheets[0])[0];
        let style = &shape.label_rich_text.as_ref().unwrap().runs[0].style;
        assert_eq!(style.font_family.as_deref(), Some("Courier New"));
        assert_eq!(style.font_size, Some(16.0));
        assert_eq!(style.bold, Some(false));
        assert_eq!(style.italic, Some(true));
        assert_eq!(style.color, Some(Color::rgb(255, 0, 0)));
        assert!(crate::import_bytes(Format::DrawIo, xml.as_bytes()).is_ok());
    }

    for (family, warned) in [
        ("initial", true),
        ("revert", true),
        ("&apos;inherit&apos;", false),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="&lt;span style=&quot;font-family:{family}&quot;&gt;Text&lt;/span&gt;" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            warned,
            "{family}"
        );
        if !warned {
            let shape = shapes(&report.workbook.sheets[0])[0];
            assert_eq!(
                shape.label_rich_text.as_ref().unwrap().runs[0]
                    .style
                    .font_family
                    .as_deref(),
                Some("inherit")
            );
        }
    }
}

#[test]
fn drawio_html_font_family_lists_warn_without_misclassifying_quoted_names() {
    for (family, expected, approximate) in [
        ("Arial, sans-serif", "Arial", true),
        ("&apos;A,B&apos;, sans-serif", "A,B", true),
        ("&apos;A,B&apos;", "A,B", false),
        ("Courier New", "Courier New", false),
        (r"Courier\20 New, serif", "Courier New", true),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="&lt;span style=&quot;font-family:{family}&quot;&gt;Text&lt;/span&gt;" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        let shape = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(shape.label(), "Text");
        assert_eq!(
            shape.label_rich_text.as_ref().unwrap().runs[0]
                .style
                .font_family
                .as_deref(),
            Some(expected)
        );
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            approximate,
            "{family}: {:?}",
            report.warnings
        );
        if approximate {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings.contains(&ImportWarning::Formatting)
            ));
            assert!(
                crate::drawio_clipboard::decode(&xml)
                    .unwrap()
                    .warnings
                    .contains(&ImportWarning::Formatting)
            );
        }
    }
}

#[test]
fn drawio_html_nested_decorations_propagate_from_ancestors() {
    for (html, underline, strike) in [
        (
            "&lt;u&gt;&lt;span style=&quot;text-decoration:none&quot;&gt;Text&lt;/span&gt;&lt;/u&gt;",
            Some(true),
            Some(false),
        ),
        (
            "&lt;s&gt;&lt;span style=&quot;text-decoration:underline&quot;&gt;Text&lt;/span&gt;&lt;/s&gt;",
            Some(true),
            Some(true),
        ),
        (
            "&lt;u&gt;&lt;span style=&quot;text-decoration:underline line-through&quot;&gt;Text&lt;/span&gt;&lt;/u&gt;",
            Some(true),
            Some(true),
        ),
        (
            "&lt;u&gt;&lt;span style=&quot;text-decoration:none&quot;&gt;&lt;span style=&quot;text-decoration:line-through&quot;&gt;Text&lt;/span&gt;&lt;/span&gt;&lt;/u&gt;",
            Some(true),
            Some(true),
        ),
        (
            "&lt;u&gt;&lt;span data-thinoda-underline=&quot;inherit&quot; style=&quot;text-decoration:none&quot;&gt;Text&lt;/span&gt;&lt;/u&gt;",
            Some(true),
            Some(false),
        ),
        (
            "&lt;span style=&quot;text-decoration:none&quot;&gt;Text&lt;/span&gt;",
            Some(false),
            Some(false),
        ),
        (
            "&lt;u style=&quot;text-decoration:none&quot;&gt;Text&lt;/u&gt;",
            Some(false),
            Some(false),
        ),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="{html}" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        let shape = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(shape.label(), "Text");
        let style = &shape.label_rich_text.as_ref().unwrap().runs[0].style;
        assert_eq!(style.underline, underline, "{html}");
        assert_eq!(style.strikethrough, strike, "{html}");
        assert!(
            !report.warnings.contains(&ImportWarning::Formatting),
            "{html}: {:?}",
            report.warnings
        );
        assert!(crate::import_bytes(Format::DrawIo, xml.as_bytes()).is_ok());
        assert!(
            !crate::drawio_clipboard::decode(&xml)
                .unwrap()
                .warnings
                .contains(&ImportWarning::Formatting)
        );
    }
}

#[test]
fn drawio_html_nested_decoration_color_loss_warns() {
    for (html, approximate) in [
        (
            "&lt;u&gt;&lt;span style=&quot;color:#ff0000&quot;&gt;Text&lt;/span&gt;&lt;/u&gt;",
            true,
        ),
        (
            "&lt;span style=&quot;color:#0000ff;text-decoration:line-through&quot;&gt;&lt;span style=&quot;color:#ff0000&quot;&gt;Text&lt;/span&gt;&lt;/span&gt;",
            true,
        ),
        (
            "&lt;span style=&quot;color:#ff0000;text-decoration:underline&quot;&gt;&lt;span style=&quot;color:#ff0000&quot;&gt;Text&lt;/span&gt;&lt;/span&gt;",
            false,
        ),
        (
            "&lt;span style=&quot;color:#ff0000&quot;&gt;Text&lt;/span&gt;",
            false,
        ),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="{html}" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Text");
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            approximate,
            "{html}: {:?}",
            report.warnings
        );
        if approximate {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings.contains(&ImportWarning::Formatting)
            ));
            assert!(
                crate::drawio_clipboard::decode(&xml)
                    .unwrap()
                    .warnings
                    .contains(&ImportWarning::Formatting)
            );
        }
    }
}

#[test]
fn drawio_html_nested_decoration_does_not_leak_into_siblings() {
    let xml = page(
        r#"<mxCell id="a" value="&lt;u&gt;A&lt;span style=&quot;text-decoration:none&quot;&gt;B&lt;/span&gt;C&lt;/u&gt;D" style="shape=rectangle;html=1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    let shape = shapes(&report.workbook.sheets[0])[0];
    assert_eq!(shape.label(), "ABCD");
    let rich = shape.label_rich_text.as_ref().unwrap();
    assert!(
        rich.runs
            .iter()
            .any(|run| run.start <= 1 && run.end > 1 && run.style.underline == Some(true))
    );
    assert!(
        !rich
            .runs
            .iter()
            .any(|run| run.start <= 3 && run.end > 3 && run.style.underline == Some(true))
    );
    assert!(!report.warnings.contains(&ImportWarning::Formatting));
}

#[test]
fn drawio_shape_effects_import_natively_and_round_trip() {
    use thinoda_core::shape_style::{DashPattern, GradientDirection};
    let xml = page(concat!(
        r#"<mxCell id="a" value="A" style="rounded=0;whiteSpace=wrap;html=1;fillColor=#FFFFFF;strokeColor=#000000;dashed=1;dashPattern=8 4 1 4;fixDash=1;gradientColor=#0000FF;gradientDirection=east;shadow=1;glass=1;flipH=1;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="b" style="triangle;whiteSpace=wrap;html=1;fillColor=#FF0000;strokeColor=#000000;fillStyle=cross-hatch;flipH=1;" vertex="1" parent="1"><mxGeometry x="200" y="10" width="40" height="80" as="geometry"/></mxCell>"#,
        r#"<mxCell id="c" style="ellipse;whiteSpace=wrap;html=1;fillColor=#FFFFFF;strokeColor=#000000;dashed=1;" vertex="1" parent="1"><mxGeometry x="300" y="10" width="40" height="40" as="geometry"/></mxCell>"#,
    ));
    let report = load(&xml);
    assert!(
        !report.warnings.contains(&ImportWarning::Formatting),
        "{:?}",
        report.warnings
    );
    let sheet = &report.workbook.sheets[0];
    let by_id = |id: &str| {
        sheet
            .canvas
            .objects
            .iter()
            .find(|o| o.drawio.as_ref().and_then(|d| d.source_id.as_deref()) == Some(id))
            .unwrap()
    };
    let a = by_id("a");
    let dash = a.style.dash.unwrap();
    assert_eq!(
        dash.pattern,
        DashPattern::new(&[8.0, 4.0, 1.0, 4.0]).unwrap()
    );
    assert!(dash.fixed);
    let gradient = a.style.gradient.unwrap();
    assert_eq!(gradient.color, Color::rgb(0, 0, 255));
    assert_eq!(gradient.direction, GradientDirection::East);
    assert!(a.style.shadow && a.style.glass && a.style.flip_h && !a.style.flip_v);
    for object in [a, by_id("b"), by_id("c")] {
        let style = &object.drawio.as_ref().unwrap().style;
        assert!(
            style
                .iter()
                .all(|(key, _)| !SHAPE_EFFECT_STYLE_KEYS.contains(&key.as_str())),
            "{style:?}"
        );
    }
    // The east-pointing triangle is a turned native triangle, so draw.io's
    // horizontal flip is the frame's vertical one.
    let b = by_id("b");
    assert_eq!(
        b.style.fill_pattern,
        thinoda_core::board::FillPattern::CrossHatch
    );
    assert!(!b.style.flip_h && b.style.flip_v);
    assert_eq!(
        by_id("c").style.dash.unwrap().pattern,
        DashPattern::default()
    );
    assert_eq!(feature_version(&report.workbook), 47);

    let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    for needle in [
        "dashed=1;dashPattern=8 4 1 4;fixDash=1;gradientColor=#0000FF;gradientDirection=east;shadow=1;glass=1;flipH=1;",
        "fillStyle=cross-hatch;flipV=1;",
    ] {
        assert!(exported.contains(needle), "{needle} in {exported}");
    }
    let reopened = load(&exported);
    let objects = |book: &Workbook| {
        book.sheets[0]
            .canvas
            .objects
            .iter()
            .map(|o| (o.rect, o.rotation, o.style))
            .collect::<Vec<_>>()
    };
    assert_eq!(objects(&reopened.workbook), objects(&report.workbook));
}

#[test]
fn drawio_label_text_style_imports_natively_and_round_trips() {
    use thinoda_core::label_style::LabelVAlign;
    let xml = page(concat!(
        r##"<mxCell id="a" value="Styled label" style="rounded=0;html=1;fillColor=#FFFFFF;strokeColor=#000000;fontColor=#000000;fontFamily=Courier New;fontSize=18;fontStyle=15;align=left;verticalAlign=bottom;labelPosition=right;verticalLabelPosition=top;spacing=4;spacingTop=1;spacingLeft=2;spacingBottom=3;spacingRight=5;labelBackgroundColor=#FFFF00;labelBorderColor=#FF0000;horizontal=0;overflow=hidden;textDirection=rtl;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="40" as="geometry"/></mxCell>"##,
        r##"<mxCell id="b" value="Wrapped" style="rounded=0;whiteSpace=wrap;html=1;fillColor=#FFFFFF;strokeColor=#000000;fontColor=#000000;fontSize=14;" vertex="1" parent="1"><mxGeometry x="200" y="10" width="80" height="40" as="geometry"/></mxCell>"##,
        r##"<mxCell id="c" value="Plain" style="rounded=0;whiteSpace=wrap;html=1;fillColor=#FFFFFF;strokeColor=#000000;fontColor=#000000;align=center;fontSize=12;" vertex="1" parent="1"><mxGeometry x="300" y="10" width="80" height="40" as="geometry"/></mxCell>"##,
    ))
    .replacen("<mxGraphModel>", r#"<mxGraphModel adaptiveColors="none">"#, 1);
    let report = load(&xml);
    // `overflow` and `textDirection` stay passthrough with a warning.
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    let sheet = &report.workbook.sheets[0];
    let by_id = |id: &str| {
        sheet
            .canvas
            .objects
            .iter()
            .find(|o| o.drawio.as_ref().and_then(|d| d.source_id.as_deref()) == Some(id))
            .unwrap()
    };
    let a = by_id("a");
    let label = a.label_style.as_deref().unwrap();
    assert_eq!(label.font_family.as_deref(), Some("Courier New"));
    assert_eq!(label.font_size, Some(18.0));
    assert!(label.bold && label.italic && label.underline && label.strikethrough);
    assert_eq!(label.align, TextAlign::Left);
    assert_eq!(label.vertical_align, LabelVAlign::Bottom);
    assert_eq!(label.position, TextAlign::Right);
    assert_eq!(label.vertical_position, LabelVAlign::Top);
    assert_eq!(
        [
            label.spacing,
            label.spacing_top,
            label.spacing_left,
            label.spacing_bottom,
            label.spacing_right
        ],
        [4.0, 1.0, 2.0, 3.0, 5.0]
    );
    assert_eq!(label.background, Some(Color::rgb(255, 255, 0)));
    assert_eq!(label.border, Some(Color::rgb(255, 0, 0)));
    assert!(label.vertical && !label.wrap);
    let rest = &a.drawio.as_ref().unwrap().style;
    assert!(rest.contains(&("overflow".into(), "hidden".into())));
    assert!(rest.contains(&("textDirection".into(), "rtl".into())));
    assert!(
        rest.iter().all(|(key, _)| !matches!(
            key.as_str(),
            "fontFamily" | "fontSize" | "fontStyle" | "align" | "labelPosition" | "spacing"
        )),
        "{rest:?}"
    );
    let b = by_id("b").label_style.as_deref().unwrap();
    assert!(b.wrap && b.font_size == Some(14.0));
    assert!(by_id("c").label_style.is_none());
    assert_eq!(feature_version(&report.workbook), 48);

    let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    for needle in [
        "whiteSpace=nowrap;html=1;fontFamily=Courier New;fontSize=18;fontStyle=15;align=left;verticalAlign=bottom;labelPosition=right;verticalLabelPosition=top;spacing=4;spacingTop=1;spacingLeft=2;spacingBottom=3;spacingRight=5;labelBackgroundColor=#FFFF00;labelBorderColor=#FF0000;horizontal=0;",
        "overflow=hidden;",
        "whiteSpace=wrap;html=1;fontSize=14;",
    ] {
        assert!(exported.contains(needle), "{needle} in {exported}");
    }
    let reopened = load(&exported);
    let labels = |book: &Workbook| {
        book.sheets[0]
            .canvas
            .objects
            .iter()
            .map(|o| o.label_style.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(&reopened.workbook), labels(&report.workbook));
}

#[test]
fn drawio_shape_effects_keep_unmapped_values_and_warn() {
    let xml = page(concat!(
        r#"<mxCell id="a" style="rounded=0;html=1;fillColor=#FFFFFF;strokeColor=#000000;fillStyle=zigzag;" vertex="1" parent="1"><mxGeometry width="80" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="b" style="rounded=0;html=1;fillColor=#FFFFFF;strokeColor=#000000;fillStyle=future;gradientColor=#00FF00;gradientDirection=diagonal;" vertex="1" parent="1"><mxGeometry x="100" width="80" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="t" value="Text" style="text;html=1;shadow=1;" vertex="1" parent="1"><mxGeometry x="200" width="80" height="40" as="geometry"/></mxCell>"#,
    ));
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    let sheet = &report.workbook.sheets[0];
    let style = |index: usize| {
        let object = &sheet.canvas.objects[index];
        (object.style, object.drawio.as_ref().unwrap().style.clone())
    };
    let (a, a_rest) = style(0);
    assert_eq!(a.fill_pattern, thinoda_core::board::FillPattern::ZigzagLine);
    assert!(a_rest.contains(&("fillStyle".into(), "zigzag".into())));
    let (b, b_rest) = style(1);
    assert!(b.fill_pattern.is_solid() && b.gradient.is_none());
    for key in ["fillStyle", "gradientColor", "gradientDirection"] {
        assert!(b_rest.iter().any(|(k, _)| k == key), "{key}: {b_rest:?}");
    }
    let (t, t_rest) = style(2);
    assert!(!t.shadow);
    assert!(t_rest.contains(&("shadow".into(), "1".into())));
    let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    for needle in [
        "fillStyle=zigzag-line;",
        "fillStyle=future;",
        "gradientDirection=diagonal;",
        "shadow=1;",
    ] {
        assert!(exported.contains(needle), "{needle} in {exported}");
    }
}

#[test]
fn drawio_placeholder_data_loss_requires_confirmation() {
    for tag in ["object", "UserObject"] {
        let xml = page(&format!(
            r#"<{tag} id="a" label="Width %width% on %date%" placeholders="1"><mxCell style="shape=rectangle;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell></{tag}>"#
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings,
            vec![ImportWarning::DrawioPlaceholdersUnsupported],
            "{tag}"
        );
        assert_eq!(
            shapes(&report.workbook.sheets[0])[0].label(),
            "Width %width% on %date%"
        );
        assert!(matches!(
            crate::import_bytes(Format::DrawIo, xml.as_bytes()),
            Err(IoError::ImportNeedsConfirmation(warnings))
                if warnings == vec![ImportWarning::DrawioPlaceholdersUnsupported]
        ));
        let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
        assert_eq!(
            pasted.warnings,
            vec![ImportWarning::DrawioPlaceholdersUnsupported]
        );
    }

    let disabled = page(
        r#"<object id="a" label="Width %width%" placeholders="0"><mxCell style="shape=rectangle;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell></object>"#,
    );
    assert!(load(&disabled).warnings.is_empty());
    assert!(crate::import_bytes(Format::DrawIo, disabled.as_bytes()).is_ok());

    let malformed = disabled.replace(r#"placeholders="0""#, r#"placeholders="true""#);
    assert_eq!(load(&malformed).warnings, vec![ImportWarning::Other]);

    let bound_label = disabled.replace(r#"placeholders="0""#, r#"placeholder="status""#);
    // The unknown attribute itself is preserved, so only the placeholder
    // behavior is reported.
    assert_eq!(
        load(&bound_label).warnings,
        vec![ImportWarning::DrawioPlaceholdersUnsupported]
    );
}

#[test]
fn drawio_grid_settings_follow_editor_defaults_and_report_adjustments() {
    for (attributes, grid, size, snap, adjusted) in [
        ("", GridKind::Lines, 10.0, true, false),
        (
            r#"grid="0" gridSize="20""#,
            GridKind::None,
            20.0,
            false,
            false,
        ),
        (
            r#"grid="1" gridSize="10.5""#,
            GridKind::Lines,
            10.5,
            true,
            false,
        ),
        (
            r#"grid="false" gridSize="12px""#,
            GridKind::Lines,
            12.0,
            true,
            true,
        ),
        (r#"grid="1" gridSize="3""#, GridKind::Lines, 4.0, true, true),
        (
            r#"grid="0" gridSize="1000""#,
            GridKind::None,
            512.0,
            false,
            true,
        ),
        (r#"gridSize="0""#, GridKind::Lines, 10.0, true, true),
        (r#"gridSize="-1""#, GridKind::Lines, 10.0, true, true),
        (r#"gridSize="NaN""#, GridKind::Lines, 10.0, true, true),
        (r#"gridSize="Infinity""#, GridKind::Lines, 10.0, true, true),
        (r#"gridSize="1e309""#, GridKind::Lines, 10.0, true, true),
        (r#"gridSize="0x10""#, GridKind::Lines, 10.0, true, true),
        (
            r#"gridSize=" +12.5tail""#,
            GridKind::Lines,
            12.5,
            true,
            true,
        ),
        (r#"gridSize=".5e+2px""#, GridKind::Lines, 50.0, true, true),
        (r#"gridSize="1e""#, GridKind::Lines, 4.0, true, true),
        (r#"gridSize="1e2""#, GridKind::Lines, 100.0, true, false),
        (r#"gridSize="4.e1""#, GridKind::Lines, 40.0, true, false),
    ] {
        let xml = format!(
            r#"<mxGraphModel {attributes}><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel>"#
        );
        let report = load(&xml);
        let board = report.workbook.sheets[0].board;
        assert_eq!(
            (board.grid, board.grid_size, board.snap),
            (grid, size, snap),
            "{attributes}"
        );
        assert_eq!(
            report.warnings,
            if adjusted {
                vec![ImportWarning::DrawioGridAdjusted]
            } else {
                vec![]
            },
            "{attributes}"
        );
        if adjusted {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings == vec![ImportWarning::DrawioGridAdjusted]
            ));
        }
        report.workbook.validate().unwrap();
    }
    assert_eq!(grid_number("\u{feff}12"), Some((12.0, false)));
    assert_eq!(grid_number("\u{85}12"), None);
    assert_eq!(grid_number("12\u{85}"), Some((12.0, true)));
}

#[test]
fn thinoda_grid_settings_round_trip_and_stale_extensions_never_override_drawio() {
    for grid in [GridKind::None, GridKind::Dots, GridKind::Lines] {
        for snap in [false, true] {
            let mut sheet = Sheet::new("Grid", "");
            sheet.root_mode = SheetRootMode::Hidden;
            sheet.board.grid = grid;
            sheet.board.snap = snap;
            sheet.board.grid_size = 12.5;
            let expected = sheet.board;
            let xml = String::from_utf8(export(&Workbook::new(sheet)).unwrap()).unwrap();
            let report = load(&xml);
            assert!(
                report.warnings.is_empty(),
                "{grid:?}, {snap}: {:?}",
                report.warnings
            );
            assert_eq!(
                report.workbook.sheets[0].board, expected,
                "{grid:?}, {snap}"
            );

            let external = xml
                .replace(
                    &format!(
                        " thinodaGrid=\"{}\"",
                        match grid {
                            GridKind::None => "none",
                            GridKind::Dots => "dots",
                            GridKind::Lines => "lines",
                        }
                    ),
                    "",
                )
                .replace(&format!(" thinodaSnap=\"{}\"", u8::from(snap)), "");
            let imported = load(&external);
            assert!(imported.warnings.is_empty());
            let visible = grid != GridKind::None;
            assert_eq!(
                (
                    imported.workbook.sheets[0].board.grid,
                    imported.workbook.sheets[0].board.snap
                ),
                (
                    if visible {
                        GridKind::Lines
                    } else {
                        GridKind::None
                    },
                    visible
                ),
                "{grid:?}, {snap}"
            );
        }
    }

    for (attributes, grid, snap) in [
        (
            r#"grid="0" thinodaGrid="dots" thinodaSnap="1""#,
            GridKind::None,
            false,
        ),
        (
            r#"grid="1" thinodaGrid="none" thinodaSnap="1""#,
            GridKind::Lines,
            true,
        ),
        (r#"grid="1" thinodaGrid="dots""#, GridKind::Lines, true),
        (
            r#"grid="1" thinodaGrid="dots" thinodaSnap="invalid""#,
            GridKind::Lines,
            true,
        ),
    ] {
        let xml = format!(
            r#"<mxGraphModel {attributes}><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel>"#
        );
        let report = load(&xml);
        assert_eq!(
            (
                report.workbook.sheets[0].board.grid,
                report.workbook.sheets[0].board.snap
            ),
            (grid, snap)
        );
        assert_eq!(
            report.warnings,
            vec![ImportWarning::DrawioGridAdjusted],
            "{attributes}"
        );
    }
}

#[test]
fn drawio_grid_loss_reports_every_combination_and_exported_page() {
    let mut book = Workbook::new({
        let mut sheet = Sheet::new("Grid", "");
        sheet.root_mode = SheetRootMode::Hidden;
        sheet
    });
    for grid in [GridKind::None, GridKind::Dots, GridKind::Lines] {
        for snap in [false, true] {
            book.sheets[0].board.grid = grid;
            book.sheets[0].board.snap = snap;
            let expected = if grid == GridKind::Dots || snap != (grid != GridKind::None) {
                vec![ExportLoss::DrawioGridSettingsApproximated(1)]
            } else {
                vec![]
            };
            for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
                assert_eq!(
                    export_losses(&book, format, &ExportOptions::default()),
                    expected,
                    "{format:?}, {grid:?}, snap={snap}"
                );
            }
            assert!(export_losses(&book, Format::Thinoda, &ExportOptions::default()).is_empty());
        }
    }

    book.sheets[0].board.grid = GridKind::Lines;
    book.sheets[0].board.snap = true;
    let mut second = book.sheets[0].clone();
    second.title = "Dots".into();
    second.board.grid = GridKind::Dots;
    let mut third = second.clone();
    third.title = "Hidden snap".into();
    third.board.grid = GridKind::None;
    third.board.snap = true;
    book.sheets.extend([second, third]);
    book.active_sheet = 2;
    for (index, expected) in [
        (0, vec![]),
        (1, vec![ExportLoss::DrawioGridSettingsApproximated(1)]),
        (2, vec![ExportLoss::DrawioGridSettingsApproximated(1)]),
    ] {
        let options = ExportOptions {
            sheet: Some(index),
            ..Default::default()
        };
        assert_eq!(
            export_losses(&book, Format::DrawIo, &options),
            vec![ExportLoss::DrawioGridSettingsApproximated(2)]
        );
        for format in [Format::DrawIoSvg, Format::DrawIoPng] {
            assert_eq!(
                export_losses(&book, format, &options),
                expected,
                "{format:?}, {index}"
            );
        }
    }
    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![ExportLoss::DrawioGridSettingsApproximated(1)],
            "{format:?}: active sheet"
        );
    }
}

#[test]
fn editable_drawio_images_preserve_selected_sheet_grid_settings() {
    let mut first = Sheet::new("Other", "");
    first.root_mode = SheetRootMode::Hidden;
    first.board.grid = GridKind::Lines;
    first.board.snap = true;
    let mut second = first.clone();
    second.title = "Selected".into();
    second
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 100.0, 60.0),
        ))
        .unwrap();
    let mut book = Workbook::new(first);
    book.sheets.push(second);
    let options = ExportOptions {
        sheet: Some(1),
        ..Default::default()
    };
    for (grid, snap) in [
        (GridKind::Dots, true),
        (GridKind::None, true),
        (GridKind::Lines, false),
    ] {
        book.sheets[1].board.grid = grid;
        book.sheets[1].board.grid_size = 12.5;
        book.sheets[1].board.snap = snap;
        for format in [Format::DrawIoSvg, Format::DrawIoPng] {
            let bytes = export_bytes(&book, format, &options).unwrap();
            let report = import(&bytes).unwrap();
            assert!(report.warnings.is_empty(), "{format:?}, {grid:?}, {snap}");
            assert_eq!(report.workbook.sheets.len(), 1);
            assert_eq!(report.workbook.sheets[0].board, book.sheets[1].board);
        }
    }
}

#[test]
fn drawio_background_import_reports_page_and_color_approximations() {
    let amber = Color::rgb(244, 194, 122);
    for (attributes, expected, adjusted) in [
        ("", None, false),
        (r#"background="""#, None, false),
        (r#"background="none""#, None, false),
        (
            r#"background="none" backgroundGradient="none" backgroundImage="""#,
            None,
            false,
        ),
        (
            r##"page="0" background="#F4C27A" adaptiveColors="simple""##,
            Some(amber),
            false,
        ),
        (
            r##"page="0" background="#F4C27A" adaptiveColors="none""##,
            Some(amber),
            false,
        ),
        (
            r#"page="0" background="rgb(244,194,122)" adaptiveColors="simple""#,
            Some(amber),
            false,
        ),
        (
            r#"page="0" background="hsl(0,100%,50%)" adaptiveColors="simple""#,
            Some(Color::rgb(255, 0, 0)),
            false,
        ),
        (
            r#"page="0" background="hwb(240 0% 0%)" adaptiveColors="simple""#,
            Some(Color::rgb(0, 0, 255)),
            true,
        ),
        (
            r#"page="0" background="rgb(1,/* comment */2,3)" adaptiveColors="simple""#,
            Some(Color::rgb(1, 2, 3)),
            true,
        ),
        (r##"page="0" background="#F4C27A""##, Some(amber), true),
        (
            r##"page="0" background="#F4C27A" adaptiveColors="future""##,
            Some(amber),
            true,
        ),
        (
            r##"page="0" background="light-dark(#F4C27A,#F4C27A)""##,
            Some(amber),
            false,
        ),
        (
            r##"page="0" background="light-dark(rgb(244,194,122),#F4C27A)""##,
            Some(amber),
            false,
        ),
        (
            r##"page="0" background="LIGHT-DARK(rgb(244,194,122),#f4c27a)""##,
            Some(amber),
            true,
        ),
        (
            r#"page="0" background="light-dark(light-dark(red,red),rgb(255,0,0))""#,
            Some(Color::rgb(255, 0, 0)),
            true,
        ),
        (
            r##"page="0" background="light-dark(#F4C27A,#683D00)""##,
            Some(amber),
            true,
        ),
        (
            r#"page="0" background="light-dark(light-dark(red,blue),red)""#,
            Some(Color::rgb(255, 0, 0)),
            true,
        ),
        (
            r#"page="0" background="light-dark(red,currentColor)""#,
            Some(Color::rgb(255, 0, 0)),
            true,
        ),
        (
            r##"page="0" background="light-dark(#fff,var(--unknown))""##,
            None,
            true,
        ),
        (
            r##"page="0" background="#11223380" adaptiveColors="simple""##,
            Some(Color::rgba(17, 34, 51, 128)),
            true,
        ),
        (
            r#"page="0" background="transparent" adaptiveColors="simple""#,
            Some(Color::TRANSPARENT),
            true,
        ),
        (
            r##"background="#F4C27A" adaptiveColors="simple""##,
            Some(amber),
            true,
        ),
        (
            r##"page="1" background="#F4C27A" adaptiveColors="simple""##,
            Some(amber),
            true,
        ),
        (
            r##"page="false" background="#F4C27A" adaptiveColors="simple""##,
            Some(amber),
            true,
        ),
        (r#"page="0" background="currentColor""#, None, true),
        (r#"page="0" background="rgb(1,2)""#, None, true),
        (r##"backgroundGradient="#ffffff""##, None, true),
        (r#"backgroundImage="{}""#, None, true),
        (r#"backgroundImage="none""#, None, true),
    ] {
        let xml = format!(
            r#"<mxGraphModel {attributes}><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel>"#
        );
        let report = load(&xml);
        assert_eq!(
            report.workbook.active().theme.background,
            expected,
            "{attributes}"
        );
        let warnings = if adjusted {
            vec![ImportWarning::DrawioBackgroundAdjusted]
        } else {
            vec![]
        };
        assert_eq!(report.warnings, warnings, "{attributes}");
        let strict = crate::import_bytes(Format::DrawIo, xml.as_bytes());
        if adjusted {
            assert!(
                matches!(strict, Err(IoError::ImportNeedsConfirmation(ref actual)) if *actual == warnings),
                "{attributes}: {strict:?}"
            );
        } else {
            assert!(strict.is_ok(), "{attributes}: {strict:?}");
        }
        report.workbook.validate().unwrap();
    }
}

#[test]
fn fixed_background_pairs_follow_drawio_color_recognition() {
    for (value, fixed) in [
        ("light-dark(red,#ff0000)", true),
        ("light-dark(rgb(255,0,0),#ff0000)", true),
        ("light-dark(#f4c27a,#F4C27A)", true),
        ("light-dark(red,blue)", false),
        ("LIGHT-DARK(red,red)", false),
        ("light-dark(RGB(255,0,0),red)", false),
        ("light-dark(hsl(0,100%,50%),red)", false),
        ("light-dark(light-dark(red,red),red)", false),
        ("light-dark(red,currentColor)", false),
        ("light-dark(red,red,red)", false),
    ] {
        assert_eq!(drawio_fixed_background_pair(value), fixed, "{value}");
    }
}

#[test]
fn published_drawio_saves_and_reopens_a_fixed_background() {
    let xml = include_bytes!("../../tests/fixtures/published-static-background.drawio");
    let report = import(xml).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(
        report.workbook.active().theme.background,
        Some(Color::rgb(244, 194, 122))
    );
    assert_eq!(shapes(report.workbook.active())[0].label(), "Synthetic");
    let exported = export(&report.workbook).unwrap();
    let restored = import(&exported).unwrap();
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    assert_eq!(
        restored.workbook.active().theme.background,
        report.workbook.active().theme.background
    );
}

#[test]
fn drawio_backgrounds_round_trip_and_preflight_the_exported_pages() {
    let mut first = Sheet::new("Default", "");
    first.root_mode = SheetRootMode::Hidden;
    first.board.grid = GridKind::Lines;
    first.board.snap = true;
    first
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 100.0, 60.0),
        ))
        .unwrap();
    let backgrounds = [
        None,
        Some(Color::rgb(244, 194, 122)),
        Some(Color::rgba(17, 34, 51, 128)),
        Some(Color::rgba(90, 80, 70, 0)),
    ];
    let mut book = Workbook::new(first);
    for (index, background) in backgrounds.iter().enumerate().skip(1) {
        let mut sheet = book.sheets[0].clone();
        sheet.title = format!("Background {index}");
        sheet.theme.background = *background;
        book.sheets.push(sheet);
    }
    book.active_sheet = 2;
    book.validate().unwrap();
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(xml.contains(r#"background="light-dark(#F4C27A,#F4C27A)""#));
    assert!(xml.contains(r#"background="light-dark(#11223380,#11223380)""#));
    assert!(xml.contains(r#"background="light-dark(#5A504600,#5A504600)""#));
    assert!(!xml.contains("adaptiveColors="));
    let report = load(&xml);
    assert_eq!(
        report.warnings,
        vec![ImportWarning::DrawioBackgroundAdjusted]
    );
    assert_eq!(report.workbook.sheets.len(), backgrounds.len());
    for (sheet, expected) in report.workbook.sheets.iter().zip(backgrounds) {
        assert_eq!(sheet.theme.background, expected);
    }
    let native = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(crate::import_bytes(Format::Thinoda, &native).unwrap(), book);

    for (index, background) in backgrounds.into_iter().enumerate() {
        let options = ExportOptions {
            sheet: Some(index),
            ..Default::default()
        };
        let selected_loss = if background.is_some_and(|color| color.a < 255) {
            vec![ExportLoss::DrawioBackgroundAppearanceApproximated(1)]
        } else {
            vec![]
        };
        assert_eq!(
            export_losses(&book, Format::DrawIo, &options),
            vec![ExportLoss::DrawioBackgroundAppearanceApproximated(2)]
        );
        for format in [Format::DrawIoSvg, Format::DrawIoPng] {
            assert_eq!(export_losses(&book, format, &options), selected_loss);
            let bytes = export_bytes(&book, format, &options).unwrap();
            let report = import(&bytes).unwrap();
            assert_eq!(report.workbook.sheets.len(), 1);
            assert_eq!(report.workbook.active().theme.background, background);
            assert_eq!(
                report
                    .warnings
                    .contains(&ImportWarning::DrawioBackgroundAdjusted),
                !selected_loss.is_empty(),
                "{format:?}, sheet {index}: {:?}",
                report.warnings
            );
        }
        assert!(
            export_losses(&book, Format::Thinoda, &options).is_empty(),
            "native backgrounds are not approximated"
        );
    }
    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![ExportLoss::DrawioBackgroundAppearanceApproximated(1)]
        );
    }
}

#[test]
fn imported_page_background_is_used_by_visual_exports() {
    use thinoda_core::theme::Theme;

    for (value, color) in [
        ("#F4C27A", Color::rgb(244, 194, 122)),
        ("#11223380", Color::rgba(17, 34, 51, 128)),
    ] {
        let xml = format!(
            r#"<mxGraphModel page="0" background="light-dark({value},{value})"><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="a" vertex="1" parent="1" style="shape=rectangle;"><mxGeometry width="100" height="60" as="geometry"/></mxCell></root></mxGraphModel>"#
        );
        let book = load(&xml).workbook;
        assert_eq!(book.active().theme.background, Some(color));
        for dark in [false, true] {
            let options = ExportOptions {
                dark,
                ..Default::default()
            };
            let expected =
                Theme::for_settings(&book.active().theme).background(&book.active().theme, dark);
            let svg =
                String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
            assert!(svg.contains(&format!("fill=\"{}\"", expected.to_hex())));
            let png = export_bytes(&book, Format::Png, &options).unwrap();
            let pixel = image::load_from_memory(&png)
                .unwrap()
                .to_rgba8()
                .get_pixel(0, 0)
                .0;
            assert_eq!(pixel, [expected.r, expected.g, expected.b, expected.a]);
        }
    }
}

#[test]
fn sticky_and_text_card_alignment_survives_drawio_interchange() {
    use thinoda_core::TextAlign;

    for kind in [CardKind::Sticky, CardKind::Text] {
        for (align, value) in [
            (TextAlign::Left, "left"),
            (TextAlign::Center, "center"),
            (TextAlign::Right, "right"),
        ] {
            let mut sheet = Sheet::new("Board", "Root");
            sheet.root_mode = SheetRootMode::Hidden;
            let id = sheet
                .add_card(
                    "Aligned card",
                    Vec2::ZERO,
                    Card::new(kind, Vec2::new(240.0, 140.0)),
                )
                .unwrap();
            sheet.topic_mut(id).unwrap().style.align = Some(align);
            let xml = String::from_utf8(export(&Workbook::new(sheet)).unwrap()).unwrap();
            assert!(xml.contains(&format!("align={value};")), "{kind:?}: {xml}");
            let imported = load(&xml);
            let warnings = if kind == CardKind::Sticky {
                vec![ImportWarning::DrawioFoldedNotesApproximated]
            } else {
                vec![]
            };
            assert_eq!(imported.warnings, warnings, "{kind:?} {align:?}");
            let card = imported.workbook.sheets[0]
                .topics
                .values()
                .find(|topic| topic.title == "Aligned card")
                .unwrap();
            assert_eq!(card.card.unwrap().kind, kind);
            assert_eq!(card.style.align, Some(align));
        }
    }
}

#[test]
fn unsupported_drawio_alignment_is_still_reported() {
    for style in [
        "shape=note;align=justify;",
        "shape=rectangle;align=justify;",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="Label" style="{style}" vertex="1" parent="1"><mxGeometry x="0" y="0" width="240" height="140" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn ordinary_drawio_note_alignment_is_imported() {
    let xml = page(
        r#"<mxCell id="a" value="External note" style="shape=note;align=right;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="240" height="140" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(
        report.warnings,
        vec![ImportWarning::DrawioFoldedNotesApproximated]
    );
    let topic = report.workbook.sheets[0]
        .topics
        .values()
        .find(|topic| topic.title == "External note")
        .unwrap();
    assert_eq!(topic.card.unwrap().kind, CardKind::Sticky);
    assert_eq!(topic.style.align, Some(thinoda_core::TextAlign::Right));
}

#[test]
fn drawio_folded_notes_become_editable_cards_with_disclosed_appearance() {
    for style in [
        "shape=note;",
        "shape=note2;boundedLbl=1;size=15;",
        "note2;boundedLbl=1;size=15;",
        "shape=note;thinodaCard=sticky;backgroundOutline=1;size=14;",
    ] {
        let xml = page(&format!(
            r##"<mxCell id="a" value="Folded" style="{style}fillColor=#F4C27A;align=right;rotation=30;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="180" height="120" as="geometry"/></mxCell>
            <mxCell id="b" value="Target" style="shape=rectangle;" vertex="1" parent="1"><mxGeometry x="250" y="20" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="e" edge="1" parent="1" source="a" target="b" style="endArrow=none;"><mxGeometry relative="1" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings,
            vec![ImportWarning::DrawioFoldedNotesApproximated],
            "{style}"
        );
        assert!(matches!(
            crate::import_bytes(Format::DrawIo, xml.as_bytes()),
            Err(IoError::ImportNeedsConfirmation(warnings))
                if warnings == report.warnings
        ));
        let sheet = report.workbook.active();
        let note = sheet
            .topics
            .values()
            .find(|topic| topic.title == "Folded")
            .unwrap();
        assert_eq!(note.card.unwrap().kind, CardKind::Sticky);
        assert_eq!(note.card.unwrap().size, Vec2::new(180.0, 120.0));
        assert_eq!(note.card.unwrap().rotation, 30.0);
        assert_eq!(note.card.unwrap().color, Some(Color::rgb(244, 194, 122)));
        assert_eq!(note.position, Some(Vec2::new(100.0, 80.0)));
        assert_eq!(note.style.align, Some(TextAlign::Right));
        assert_eq!(connectors(sheet)[0].from, Endpoint::Topic(note.id));
        report.workbook.validate().unwrap();

        let native =
            export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(
            crate::import_bytes(Format::Thinoda, &native).unwrap(),
            report.workbook
        );
        let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
        assert_eq!(pasted.warnings, report.warnings);
        assert!(pasted.losses.is_empty());
    }
}

#[test]
fn folded_note_warnings_cover_container_and_image_fallbacks() {
    let inline = format!("data:image/png,{}", b64(&tiny_png(4, 4)));
    for style in [
        "shape=note2;container=1;".to_owned(),
        "shape=note;image=https://example.invalid/note.png;".to_owned(),
        format!("shape=note2;image={inline};"),
        "shape=note;thinodaCard=text;".to_owned(),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="Note" style="{style}" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>
            <mxCell id="b" value="Keep" style="shape=rectangle;" vertex="1" parent="1"><mxGeometry x="350" width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(
            report
                .warnings
                .contains(&ImportWarning::DrawioFoldedNotesApproximated),
            "{style}: {:?}",
            report.warnings
        );
        report.workbook.validate().unwrap();
        let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
        assert!(
            pasted
                .warnings
                .contains(&ImportWarning::DrawioFoldedNotesApproximated),
            "{style}: {:?}",
            pasted.warnings
        );
    }
}

#[test]
fn folded_notes_below_card_minimum_keep_their_center_and_warn() {
    let xml = page(
        r#"<mxCell id="a" value="Tiny" style="shape=note2;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="8" height="4" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(
        report.warnings,
        vec![ImportWarning::DrawioFoldedNotesApproximated]
    );
    let note = report
        .workbook
        .active()
        .topics
        .values()
        .find(|topic| topic.title == "Tiny")
        .unwrap();
    assert_eq!(note.position, Some(Vec2::new(14.0, 22.0)));
    assert_eq!(
        note.card.unwrap().size,
        Vec2::new(MIN_CARD_SIZE, MIN_CARD_SIZE)
    );
}

#[test]
fn folded_note_warnings_ignore_omitted_clipboard_pages() {
    let first = concat!(
        r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
        r#"<mxCell id="shape" value="Rectangle" style="shape=rectangle;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
        r#"</root></mxGraphModel>"#
    );
    let later = concat!(
        r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
        r#"<mxCell id="note" value="Later note" style="shape=note2;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
        r#"</root></mxGraphModel>"#
    );
    let file = format!(
        r#"<mxfile><diagram name="First">{first}</diagram><diagram name="Later">{later}</diagram></mxfile>"#
    );
    for text in [
        file.clone(),
        crate::drawio_clipboard::encode_uri_component(&file),
    ] {
        let pasted = crate::drawio_clipboard::decode(&text).unwrap();
        assert_eq!(
            pasted.losses,
            vec![crate::drawio_clipboard::DrawioPasteLoss::AdditionalPages(1)]
        );
        assert_eq!(pasted.warnings, vec![ImportWarning::Other]);
        let mut sheet = Sheet::new("Paste", "");
        sheet.root_mode = SheetRootMode::Hidden;
        thinoda_core::clipboard::paste_board_selection(&mut sheet, &pasted.payload, Vec2::ZERO)
            .unwrap();
        assert!(
            !sheet
                .topics
                .values()
                .any(|topic| topic.title == "Later note")
        );
    }
    let report = load(&file);
    assert_eq!(
        report.warnings,
        vec![ImportWarning::DrawioFoldedNotesApproximated]
    );
}

#[test]
fn drawio_note_fallbacks_report_only_cards_on_exported_pages() {
    let mut first = Sheet::new("First", "");
    first.root_mode = SheetRootMode::Hidden;
    first.board.grid = GridKind::Lines;
    first.board.snap = true;
    for (index, kind) in [
        CardKind::Sticky,
        CardKind::Sticky,
        CardKind::Text,
        CardKind::Image,
    ]
    .into_iter()
    .enumerate()
    {
        first
            .add_card(
                &format!("Card {index}"),
                Vec2::new(index as f32 * 200.0, 0.0),
                Card::new(kind, Vec2::new(120.0, 80.0)),
            )
            .unwrap();
    }
    let image = first
        .add_card(
            "Image with pixels",
            Vec2::new(800.0, 0.0),
            Card::new(CardKind::Image, Vec2::new(120.0, 80.0)),
        )
        .unwrap();
    let resource = Resource::new("image.png", "image/png", tiny_png(4, 4)).unwrap();
    images::set_image(&mut first, image, Some((resource, Vec2::new(4.0, 4.0)))).unwrap();

    let mut second = Sheet::new("Second", "");
    second.root_mode = SheetRootMode::Hidden;
    second.board.grid = GridKind::Lines;
    second.board.snap = true;
    second
        .add_card(
            "Another sticky",
            Vec2::ZERO,
            Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0)),
        )
        .unwrap();
    let mut book = Workbook::new(first);
    book.sheets.push(second);
    book.active_sheet = 1;
    book.validate().unwrap();

    for (index, expected) in [(0, 3), (1, 1)] {
        let options = ExportOptions {
            sheet: Some(index),
            ..Default::default()
        };
        assert_eq!(
            export_losses(&book, Format::DrawIo, &options),
            vec![ExportLoss::DrawioCardAppearanceApproximated(4)]
        );
        for format in [Format::DrawIoSvg, Format::DrawIoPng] {
            assert_eq!(
                export_losses(&book, format, &options),
                vec![ExportLoss::DrawioCardAppearanceApproximated(expected)],
                "{format:?}, sheet {index}"
            );
        }
        assert!(export_losses(&book, Format::Thinoda, &options).is_empty());
    }
    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![ExportLoss::DrawioCardAppearanceApproximated(1)]
        );
    }

    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert_eq!(xml.matches("shape=note;thinodaCard=sticky;").count(), 3);
    assert_eq!(xml.matches("shape=note;thinodaCard=image;").count(), 1);
    assert_eq!(xml.matches("shape=image;thinodaCard=image;").count(), 1);
}

#[test]
fn final_state_import_preserves_editable_geometry_colors_and_connectors() {
    let xml = page(
        r##"<mxCell id="a" value="" style="shape=endState;perimeter=ellipsePerimeter;fillColor=#CC2233;strokeColor=#2244AA;strokeWidth=2;rotation=30;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="60" height="40" as="geometry"/></mxCell>
        <mxCell id="b" value="" style="ellipse;fillColor=#ffffff;strokeColor=#000000;" vertex="1" parent="1"><mxGeometry x="200" y="20" width="100" height="60" as="geometry"/></mxCell>
        <mxCell id="e" style="endArrow=none;exitX=1;exitY=0.5;exitPerimeter=1;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"##,
    )
    .replacen(
        "<mxGraphModel>",
        r#"<mxGraphModel adaptiveColors="none">"#,
        1,
    );
    let report = load(&xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let sheet = &report.workbook.sheets[0];
    let objects = shapes(sheet);
    assert!(matches!(
        objects[0].kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::FinalState,
            ..
        }
    ));
    assert_eq!(objects[0].rect, rect(10.0, 20.0, 60.0, 40.0));
    assert_eq!(objects[0].rotation, 30.0);
    assert_eq!(objects[0].style.fill, Some(Color::rgb(204, 34, 51)));
    assert_eq!(objects[0].style.stroke, Some(Color::rgb(34, 68, 170)));
    assert_eq!(objects[0].style.stroke_width, 2.0);
    assert_eq!(connectors(sheet)[0].from, Endpoint::Object(objects[0].id));
    assert_eq!(connectors(sheet)[0].to, Endpoint::Object(objects[1].id));
    assert!(connectors(sheet)[0].from_port.is_some());
    report.workbook.validate().unwrap();
    assert_eq!(feature_version(&report.workbook), 24);
    let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    assert!(exported.contains("shape=endState;perimeter=ellipsePerimeter;"));
    let reimported = load(&exported);
    let restored = shapes(&reimported.workbook.sheets[0]);
    assert!(matches!(
        restored[0].kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::FinalState,
            ..
        }
    ));
    assert_eq!(restored[0].rect, objects[0].rect);
    assert_eq!(restored[0].rotation, objects[0].rotation);
    assert_eq!(restored[0].style, objects[0].style);
    assert!(
        connectors(&reimported.workbook.sheets[0])[0]
            .from_port
            .is_some()
    );
}

#[test]
fn final_state_reports_dynamic_color_and_unsupported_label_layout() {
    let linked_color = page(
        r##"<mxCell id="a" value="" style="shape=endState;perimeter=ellipsePerimeter;fillColor=strokeColor;fillOpacity=80;strokeColor=#334455;strokeOpacity=20;" vertex="1" parent="1"><mxGeometry width="30" height="30" as="geometry"/></mxCell>"##,
    );
    let report = load(&linked_color);
    let shape = shapes(&report.workbook.sheets[0])[0];
    assert_eq!(shape.style.fill, Some(Color::rgba(51, 68, 85, 204)));
    assert_eq!(shape.style.stroke, Some(Color::rgba(51, 68, 85, 51)));
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    assert!(
        !report
            .warnings
            .contains(&ImportWarning::UnsupportedDrawioShapes)
    );

    for unsupported in ["margin=8;", "labelPosition=left;spacingRight=7;"] {
        let xml = page(&format!(
            r##"<mxCell id="a" value="End" style="shape=endState;perimeter=ellipsePerimeter;fillColor=#000000;strokeColor=#000000;fontColor=#000000;{unsupported}" vertex="1" parent="1"><mxGeometry width="30" height="30" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{unsupported}: {:?}",
            report.warnings
        );
    }

    let no_fill = linked_color.replace("fillColor=strokeColor;fillOpacity=80;", "fillColor=none;");
    let shape = shapes(&load(&no_fill).workbook.sheets[0])[0].style;
    assert_eq!(shape.fill, Some(Color::TRANSPARENT));
    assert_eq!(shape.stroke, Some(Color::rgba(51, 68, 85, 51)));
}

#[test]
fn double_ellipse_import_preserves_inset_rotation_and_connectors() {
    for (margin_style, expected_margin) in [
        ("", None),
        ("margin=0;", Some(0.0)),
        ("margin=8.5;", Some(8.5)),
        ("margin=30;", Some(30.0)),
        ("margin=none;", None),
    ] {
        let xml = page(&format!(
            r##"<mxCell id="a" value="End" style="shape=doubleEllipse;perimeter=ellipsePerimeter;fillColor=#ffffcc;strokeColor=#123456;fontColor=#000000;strokeWidth=2;rotation=30;{margin_style}" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="b" value="Next" style="ellipse;fillColor=#ffffff;strokeColor=#000000;" vertex="1" parent="1"><mxGeometry x="200" y="20" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="e" style="endArrow=none;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"##
        ))
        .replacen(
            "<mxGraphModel>",
            r#"<mxGraphModel adaptiveColors="none">"#,
            1,
        );
        let report = load(&xml);
        assert!(
            report.warnings.is_empty(),
            "{margin_style}: {:?}",
            report.warnings
        );
        let sheet = &report.workbook.sheets[0];
        let objects = shapes(sheet);
        assert_eq!(objects.len(), 2);
        assert!(matches!(
            objects[0].kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::DoubleEllipse,
                ..
            }
        ));
        assert_eq!(objects[0].label(), "End");
        assert_eq!(objects[0].rect, rect(10.0, 20.0, 100.0, 60.0));
        assert_eq!(objects[0].rotation, 30.0);
        assert_eq!(objects[0].double_ellipse_margin, expected_margin);
        assert_eq!(objects[0].style.fill, Some(Color::rgb(255, 255, 204)));
        assert_eq!(objects[0].style.stroke, Some(Color::rgb(18, 52, 86)));
        assert_eq!(objects[0].style.stroke_width, 2.0);
        assert_eq!(connectors(sheet)[0].from, Endpoint::Object(objects[0].id));
        assert_eq!(connectors(sheet)[0].to, Endpoint::Object(objects[1].id));
        report.workbook.validate().unwrap();
        assert!(crate::import_bytes(Format::DrawIo, xml.as_bytes()).is_ok());

        let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
        let linked = crate::import_drawio_link(&link).unwrap();
        assert_eq!(linked.warnings, report.warnings);
        assert_eq!(linked.workbook, report.workbook);
        assert!(
            crate::drawio_clipboard::decode(&xml)
                .unwrap()
                .warnings
                .is_empty()
        );
        let native =
            export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(feature_version(&report.workbook), 21);
        assert_eq!(
            crate::import_bytes(Format::Thinoda, &native).unwrap(),
            report.workbook
        );
        let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
        assert!(exported.contains("shape=doubleEllipse;perimeter=ellipsePerimeter;"));
        if let Some(margin) = expected_margin {
            assert!(exported.contains(&format!("margin={};", num(margin))));
        } else {
            assert!(!exported.contains("margin="));
        }
        let reimported = load(&exported);
        assert_eq!(
            shapes(&reimported.workbook.sheets[0])[0].double_ellipse_margin,
            expected_margin
        );
    }
}

#[test]
fn double_ellipse_connector_perimeter_differences_are_reported() {
    for (target_style, fixed, perimeter, warned) in [
        (
            "shape=doubleEllipse;perimeter=ellipsePerimeter;",
            false,
            true,
            false,
        ),
        ("shape=doubleEllipse;", false, true, true),
        (
            "shape=doubleEllipse;perimeter=rectanglePerimeter;",
            false,
            true,
            true,
        ),
        (
            "shape=doubleEllipse;perimeter=ellipsePerimeter;",
            true,
            true,
            false,
        ),
        ("shape=doubleEllipse;", true, true, true),
        ("shape=doubleEllipse;", true, false, false),
    ] {
        for source in [true, false] {
            let (terminal, prefix) = if source {
                (r#"source="a""#, "exit")
            } else {
                (r#"target="a""#, "entry")
            };
            let constraint = if fixed {
                format!(
                    "{prefix}X=1;{prefix}Y=1;{prefix}Perimeter={};",
                    u8::from(perimeter)
                )
            } else {
                String::new()
            };
            let xml = page(&format!(
                r#"<mxCell id="a" style="{target_style}fillColor=#ffffff;strokeColor=#000000;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>
                <mxCell id="e" style="endArrow=none;{constraint}" edge="1" {terminal} parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="200" y="20" as="targetPoint"/></mxGeometry></mxCell>"#
            ))
            .replacen(
                "<mxGraphModel>",
                r#"<mxGraphModel adaptiveColors="none">"#,
                1,
            );
            let report = load(&xml);
            assert_eq!(
                report.warnings.contains(&ImportWarning::Formatting),
                warned,
                "{target_style}, fixed={fixed}, perimeter={perimeter}, source={source}: {:?}",
                report.warnings
            );
            let connector = connectors(&report.workbook.sheets[0])[0];
            assert_eq!(
                if source {
                    connector.from_port
                } else {
                    connector.to_port
                }
                .is_some(),
                fixed
            );
            report.workbook.validate().unwrap();
            let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
            assert_eq!(
                pasted.warnings.contains(&ImportWarning::Formatting),
                warned,
                "clipboard: {target_style}, fixed={fixed}, source={source}"
            );
            let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
            let linked = crate::import_drawio_link(&link).unwrap();
            assert_eq!(
                linked.warnings.contains(&ImportWarning::Formatting),
                warned,
                "link: {target_style}, fixed={fixed}, source={source}"
            );
        }
    }
}

#[test]
fn double_ellipse_reports_unrepresentable_insets_and_unresolved_colors() {
    for margin in ["-1", "NaN", "Infinity", "invalid", "1000001"] {
        let xml = page(&format!(
            r##"<mxCell id="a" value="End" style="shape=doubleEllipse;fillColor=#fff;strokeColor=#000;fontColor=#000;margin={margin};" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
        ))
        .replacen(
            "<mxGraphModel>",
            r#"<mxGraphModel adaptiveColors="none">"#,
            1,
        );
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{margin}");
        assert_eq!(
            shapes(&report.workbook.sheets[0])[0].double_ellipse_margin,
            None
        );
    }
    let collapsed = page(
        r##"<mxCell id="a" value="End" style="shape=doubleEllipse;fillColor=#fff;strokeColor=#000;fontColor=#000;margin=31;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
    )
    .replacen("<mxGraphModel>", r#"<mxGraphModel adaptiveColors="none">"#, 1);
    let report = load(&collapsed);
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    assert_eq!(
        shapes(&report.workbook.sheets[0])[0].double_ellipse_margin,
        Some(31.0)
    );

    let defaults = page(
        r#"<mxCell id="a" value="End" style="shape=doubleEllipse;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&defaults);
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    let object = shapes(&report.workbook.sheets[0])[0];
    assert_eq!(object.style.fill, Some(Color::WHITE));
    assert_eq!(object.style.stroke, Some(Color::BLACK));
}

#[test]
fn double_ellipse_reports_unsupported_label_layout() {
    for (extra, warning) in [
        ("whiteSpace=wrap;", false),
        ("whiteSpace=nowrap;", false),
        ("spacingLeft=12;", false),
        ("spacing=0;spacingTop=none;", false),
        ("labelPosition=right;", false),
        ("verticalLabelPosition=top;", false),
        ("overflow=hidden;", true),
        ("horizontal=0;", false),
        ("noLabel=1;", true),
        ("textDirection=rtl;", true),
        ("labelWidth=40;", true),
        ("fontSize=11;", false),
        ("fontSize=12;", false),
        ("fontSize=none;", false),
        ("fontSize=invalid;", true),
        ("fontStyle=invalid;", true),
    ] {
        let xml = page(&format!(
            r##"<mxCell id="a" value="End" style="shape=doubleEllipse;fillColor=#fff;strokeColor=#000;fontColor=#000;{extra}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
        ))
        .replacen(
            "<mxGraphModel>",
            r#"<mxGraphModel adaptiveColors="none">"#,
            1,
        );
        let report = load(&xml);
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            warning,
            "{extra}: {:?}",
            report.warnings
        );
        assert!(matches!(
            shapes(&report.workbook.sheets[0])[0].kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::DoubleEllipse,
                ..
            }
        ));
    }

    let suppressed_default = page(
        r##"<mxCell id="a" value="End" style=";shape=doubleEllipse;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
    )
    .replacen("<mxGraphModel>", r#"<mxGraphModel adaptiveColors="none">"#, 1);
    let report = load(&suppressed_default);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let label_style = shapes(&report.workbook.sheets[0])[0]
        .label_style
        .clone()
        .unwrap();
    assert_eq!(label_style.font_size, Some(11.0));
    assert!(!label_style.wrap);
    let restored_default =
        suppressed_default.replace("shape=doubleEllipse;", "shape=doubleEllipse;fontSize=12;");
    assert!(load(&restored_default).warnings.is_empty());
    let without_label = suppressed_default.replace(r#"value="End""#, r#"value="""#);
    assert!(load(&without_label).warnings.is_empty());
    let empty_eleven =
        without_label.replace("shape=doubleEllipse;", "shape=doubleEllipse;fontSize=11;");
    assert!(load(&empty_eleven).warnings.is_empty());
}

#[test]
fn html_label_text_retains_visible_entities_and_block_boundaries() {
    for (source, expected, warned) in [
        ("A<div>B</div>C", "A\nB\nC", false),
        ("A<DIV>B</DIV>C", "A\nB\nC", false),
        ("<div>A</div><div><br></div><div>B</div>", "A\n\nB", false),
        ("<div>A<br></div><div>B</div>", "A\nB", false),
        ("A<br><div>B</div>", "A\nB", false),
        ("A<div><br></div>B", "A\n\nB", false),
        ("<p>A</p><p>B</p>", "A\n\nB", true),
        ("A<!-- hidden > comment -->B", "AB", false),
        (
            r#"<span title="2 > 1">Hello &copy;</span>"#,
            "Hello \u{a9}",
            true,
        ),
        (
            "&#x1f642; &NotEqualTilde; &CounterClockwiseContourIntegral; &amp;lt; &nbsp;",
            "\u{1f642} \u{2242}\u{338} \u{2233} &lt; \u{a0}",
            false,
        ),
        ("&#128; &#x91; &#x97;", "\u{20ac} \u{2018} \u{2014}", false),
        ("&#0; &#xD800;", "\u{fffd} \u{fffd}", true),
        (
            "A&lt;img src=x onerror=alert(1)&gt;B",
            "A<img src=x onerror=alert(1)>B",
            false,
        ),
        (
            "A<script>not visible</script><style>.x { color: red }</style>B",
            "AB",
            true,
        ),
        ("A<span hidden>not visible</span>B", "AB", true),
        ("<xmp>A &copy;</xmp>", "A &copy;", true),
        ("<div>not closed", "not closed", true),
        ("&amp", "&amp", true),
        ("&notit;", "&notit;", true),
        ("&unknown;", "&unknown;", false),
        ("1 < 2", "1 < 2", false),
        ("A  B", "A  B", true),
        (" A ", " A ", true),
        ("A\nB", "A\nB", true),
        ("A\tB", "A\tB", true),
        ("A&#32;&#32;B", "A  B", true),
        ("A&#10;B", "A\nB", true),
        ("A&nbsp;&nbsp;B", "A\u{a0}\u{a0}B", false),
        ("A&nbsp; B", "A\u{a0} B", false),
        ("A <span>B</span>", "A B", false),
        ("<span>A</span><span> B</span>", "A B", false),
        ("A<span> </span><span></span>B", "A B", false),
        ("A<span> </span><span> B</span>", "A  B", true),
        ("A<div> B</div> C", "A\n B\n C", true),
        ("A <br>B", "A \nB", true),
        ("A<br> B", "A\n B", true),
        ("<div> A</div>", " A", true),
        ("<div>A </div><div>B</div>", "A \nB", true),
    ] {
        let mut warnings = BTreeSet::new();
        assert_eq!(strip_html(source, &mut warnings), expected, "{source}");
        assert_eq!(
            warnings.contains(&ImportWarning::Formatting),
            warned,
            "{source}: {warnings:?}"
        );
    }
}

#[test]
fn rich_shape_html_imports_and_round_trips_styles_lists_and_newlines() {
    use thinoda_core::{InlineBaseline, InlineParagraph, InlineText, ListKind};

    let markup = "<ul><li><b>A🙂</b></li><li><sup>two</sup></li></ul>";
    let xml = page(&format!(
        r##"<mxCell id="rich" value="{}" style="shape=rectangle;html=1;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="120" height="80" as="geometry"/></mxCell>"##,
        esc(markup),
    ))
    .replacen(
        "<mxGraphModel>",
        r#"<mxGraphModel adaptiveColors="none">"#,
        1,
    );
    let report = load(&xml);
    assert!(!report.warnings.contains(&ImportWarning::Formatting));
    let object = shapes(&report.workbook.sheets[0])[0];
    assert_eq!(object.label(), "A🙂\ntwo");
    let rich = object.label_rich_text.as_ref().unwrap();
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(
        rich.runs[1].style.baseline,
        Some(InlineBaseline::Superscript)
    );
    assert_eq!(rich.paragraphs.len(), 2);
    assert_eq!(feature_version(&report.workbook), 49);
    let exported = export(&report.workbook).unwrap();
    let restored = import(&exported).unwrap().workbook;
    let restored = shapes(&restored.sheets[0])[0];
    assert_eq!(restored.label(), object.label());
    assert_eq!(restored.label_rich_text, object.label_rich_text);

    let mut sheet = Sheet::new("P", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 120.0, 80.0));
    object.set_label("Hi🙂\n\nend").unwrap();
    let mut rich = InlineText {
        paragraphs: vec![
            InlineParagraph {
                index: 0,
                list: ListKind::Bulleted,
            },
            InlineParagraph {
                index: 1,
                list: ListKind::Bulleted,
            },
            InlineParagraph {
                index: 2,
                list: ListKind::Numbered,
            },
        ],
        ..Default::default()
    };
    rich.update_range(object.label(), 0..5, |style| {
        style.bold = Some(true);
        style.strikethrough = Some(true);
        style.font_family = Some("Example';url(https://example.invalid/x)".into());
        style.font_size = Some(24.5);
        style.color = Some(Color::rgba(30, 40, 50, 128));
    })
    .unwrap();
    object.label_style = Some(Arc::new(LabelStyle {
        underline: true,
        ..Default::default()
    }));
    object.label_rich_text = Some(Arc::new(rich.clone()));
    sheet.add_canvas_object(object).unwrap();
    let book = Workbook::new(sheet);
    let exported = export(&book).unwrap();
    assert!(!String::from_utf8_lossy(&exported).contains("url(https:"));
    let restored = import(&exported).unwrap().workbook;
    let object = shapes(&restored.sheets[0])[0];
    assert_eq!(object.label(), "Hi🙂\n\nend");
    assert_eq!(object.label_rich_text.as_deref(), Some(&rich));
}

#[test]
fn rich_card_html_imports_and_round_trips_styles_and_lists() {
    let markup = "<ul><li><b>A🙂</b></li><li><i>two</i></li></ul>";
    let xml = page(&format!(
        r##"<mxCell id="rich-card" value="{}" style="shape=note;thinodaCard=sticky;html=1;fillColor=#FFFF88;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="160" height="100" as="geometry"/></mxCell>"##,
        esc(markup),
    ));
    let report = load(&xml);
    assert!(
        !report.warnings.contains(&ImportWarning::Formatting),
        "{:?}",
        report.warnings
    );
    let book = report.workbook;
    let card = book.sheets[0]
        .topics
        .values()
        .find(|topic| topic.card.is_some())
        .unwrap();
    assert_eq!(card.title, "A🙂\ntwo");
    let rich = card.inline_text.clone();
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(rich.runs[1].style.italic, Some(true));
    assert_eq!(rich.paragraphs.len(), 2);

    let losses = export_losses(&book, Format::DrawIo, &ExportOptions::default());
    assert!(
        !losses
            .iter()
            .any(|loss| matches!(loss, ExportLoss::InlineTextDropped(_))),
        "{losses:?}"
    );
    let exported = export(&book).unwrap();
    let restored = import(&exported).unwrap().workbook;
    let restored = restored.sheets[0]
        .topics
        .values()
        .find(|topic| topic.card.is_some())
        .unwrap();
    assert_eq!(restored.title, card.title);
    assert_eq!(restored.inline_text, rich);
    book.validate().unwrap();
}

#[test]
fn rich_card_baseline_and_other_styles_round_trip() {
    let markup = "<b>Bold</b> <sup><i>up</i></sup> <sub>down</sub> <span style=\"vertical-align:baseline\">normal</span>";
    let xml = page(&format!(
        r#"<mxCell id="card" value="{}" style="shape=note;html=1;" vertex="1" parent="1"><mxGeometry width="160" height="100" as="geometry"/></mxCell>"#,
        esc(markup)
    ));
    let report = load(&xml);
    assert!(!report.warnings.contains(&ImportWarning::Formatting));
    report.workbook.validate().unwrap();
    let card = report.workbook.sheets[0]
        .topics
        .values()
        .find(|topic| topic.card.is_some())
        .unwrap();
    assert_eq!(card.title, "Bold up down normal");
    assert!(card.inline_text.runs.iter().any(|run| run.style.baseline
        == Some(thinoda_core::InlineBaseline::Superscript)
        && run.style.italic == Some(true)));
    assert!(
        card.inline_text
            .runs
            .iter()
            .any(|run| run.style.baseline == Some(thinoda_core::InlineBaseline::Subscript))
    );
    assert!(
        card.inline_text
            .runs
            .iter()
            .any(|run| run.style.baseline == Some(thinoda_core::InlineBaseline::Normal))
    );
    assert!(
        card.inline_text
            .runs
            .iter()
            .any(|run| run.style.bold == Some(true))
    );
    assert!(
        card.inline_text
            .runs
            .iter()
            .any(|run| run.style.italic == Some(true))
    );
    let bytes = export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
    let restored = crate::import_bytes(Format::Thinoda, &bytes).unwrap();
    assert_eq!(restored, report.workbook);
    let drawio = export(&restored).unwrap();
    let reimported = import(&drawio).unwrap().workbook;
    let reimported_card = reimported.sheets[0]
        .topics
        .values()
        .find(|t| t.card.is_some())
        .unwrap();
    assert_eq!(reimported_card.inline_text, card.inline_text);
}

#[test]
fn rich_connector_html_imports_and_round_trips_primary_label() {
    use thinoda_core::InlineBaseline;

    let markup = "<ul><li><b>A🙂</b></li><li><sup>two</sup></li></ul>";
    let xml = page(&format!(
        r##"<mxCell id="edge" value="{}" style="html=1;endArrow=none;strokeColor=#000;fontColor=#000;fontSize=12;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"##,
        esc(markup)
    ));
    let report = load(&xml);
    let object = report.workbook.sheets[0]
        .canvas
        .objects
        .iter()
        .find(|object| object.is_connector())
        .unwrap();
    assert_eq!(object.label(), "A🙂\ntwo");
    let rich = object.label_rich_text.as_ref().unwrap();
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(
        rich.runs[1].style.baseline,
        Some(InlineBaseline::Superscript)
    );
    assert_eq!(rich.paragraphs.len(), 2);
    assert_eq!(feature_version(&report.workbook), 50);
    let exported = export(&report.workbook).unwrap();
    let restored = import(&exported).unwrap().workbook;
    let restored = restored.sheets[0]
        .canvas
        .objects
        .iter()
        .find(|object| object.is_connector())
        .unwrap();
    assert_eq!(restored.label(), object.label());
    assert_eq!(restored.label_rich_text, object.label_rich_text);
}

#[test]
fn rich_extra_connector_html_imports_and_round_trips() {
    let markup = "<ul><li><b>A🙂</b></li><li><sup>two</sup></li></ul>";
    let xml = page(&format!(
        r##"<mxCell id="edge" style="endArrow=none;strokeColor=#000;fontColor=#000000;fontSize=12;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell><mxCell id="label" value="{}" style="edgeLabel;html=1;align=center;verticalAlign=middle;resizable=0;points=[];fontSize=12;fontColor=#000000;" vertex="1" parent="edge"><mxGeometry x="0.5" y="-25" relative="1" as="geometry"/></mxCell>"##,
        esc(markup)
    ));
    let report = load(&xml);
    let label = &connectors(&report.workbook.sheets[0])[0].extra_labels[0];
    assert_eq!(label.text, "A🙂\ntwo");
    let rich = label.rich_text.as_ref().unwrap();
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(
        rich.runs[1].style.baseline,
        Some(thinoda_core::InlineBaseline::Superscript)
    );
    assert_eq!(rich.paragraphs.len(), 2);
    assert_eq!(feature_version(&report.workbook), 51);
    let exported = export(&report.workbook).unwrap();
    let restored = import(&exported).unwrap().workbook;
    assert_eq!(connectors(&restored.sheets[0])[0].extra_labels[0], *label);
}

#[test]
fn empty_rich_extra_connector_list_imports_and_round_trips() {
    let xml = page(&format!(
        r##"<mxCell id="edge" style="endArrow=none;strokeColor=#000;fontColor=#000000;fontSize=12;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell><mxCell id="label" value="{}" style="edgeLabel;html=1;align=center;verticalAlign=middle;resizable=0;points=[];fontSize=12;fontColor=#000000;" vertex="1" parent="edge"><mxGeometry x="0.5" y="-25" relative="1" as="geometry"/></mxCell>"##,
        esc("<ul><li></li></ul>")
    ));
    let report = load(&xml);
    let label = &connectors(&report.workbook.sheets[0])[0].extra_labels[0];
    assert!(label.text.is_empty());
    assert!(label.has_list());
    let restored = import(&export(&report.workbook).unwrap()).unwrap().workbook;
    assert_eq!(connectors(&restored.sheets[0])[0].extra_labels[0], *label);
}

#[test]
fn rich_connector_empty_list_placement_and_unsupported_children_are_disclosed() {
    let xml = page(&format!(
        r##"<mxCell id="e" value="{}" style="html=1;endArrow=none;strokeColor=#000;fontColor=#000;fontSize=12;" edge="1" parent="1"><mxGeometry x="0.5" y="-10" relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"##,
        esc("<ul><li></li></ul>"),
    ));
    let report = load(&xml);
    let object = report.workbook.sheets[0]
        .canvas
        .objects
        .iter()
        .find(|object| object.is_connector())
        .unwrap();
    assert!(object.has_label_list());
    let CanvasObjectKind::Connector(connector) = &object.kind else {
        unreachable!()
    };
    assert!(connector.label_placement.is_some());
    let exported = export(&report.workbook).unwrap();
    let restored = import(&exported).unwrap().workbook;
    let restored = restored.sheets[0]
        .canvas
        .objects
        .iter()
        .find(|object| object.is_connector())
        .unwrap();
    assert!(restored.has_label_list());
    let CanvasObjectKind::Connector(back) = &restored.kind else {
        unreachable!()
    };
    assert_eq!(back.label_placement, connector.label_placement);

    let folded = xml.replace(
        "</root>",
        r#"<mxCell id="child" value="child" vertex="1" parent="e"><mxGeometry width="20" height="10" as="geometry"/></mxCell></root>"#,
    );
    let report = load(&folded);
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    let object = report.workbook.sheets[0]
        .canvas
        .objects
        .iter()
        .find(|object| object.is_connector())
        .unwrap();
    assert_eq!(object.label(), "\nchild");
    assert!(object.has_label_list());
}

#[test]
fn rich_connector_unsupported_html_is_inert_and_warned() {
    let markup = r#"<b>safe</b><img src="https://invalid.example/x"><script>bad()</script>"#;
    let xml = page(&format!(
        r##"<mxCell id="e" value="{}" style="html=1;endArrow=none;strokeColor=#000;fontColor=#000;fontSize=12;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"##,
        esc(markup),
    ));
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    let object = report.workbook.sheets[0]
        .canvas
        .objects
        .iter()
        .find(|object| object.is_connector())
        .unwrap();
    assert_eq!(object.label(), "safe");
    assert_eq!(
        object.label_rich_text.as_ref().unwrap().runs[0].style.bold,
        Some(true)
    );
    let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    assert!(!exported.contains("invalid.example"));
    assert!(!exported.contains("bad()"));
}

#[test]
fn rich_sankey_drawio_export_styles_only_the_authored_label() {
    let mut book = thinoda_core::templates::instantiate("sankey-budget");
    let object = book
        .active_mut()
        .canvas
        .objects
        .iter_mut()
        .find(|object| {
            matches!(&object.kind, CanvasObjectKind::Connector(c) if c.sankey.is_some_and(|f| f.weight.is_some()))
        })
        .unwrap();
    object.set_label("Ribbon").unwrap();
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("Ribbon", 0..6, |style| style.italic = Some(true))
        .unwrap();
    object.label_rich_text = Some(Arc::new(rich));
    let mut extra_rich = thinoda_core::InlineText::default();
    extra_rich
        .update_range("Extra", 0..5, |style| style.bold = Some(true))
        .unwrap();
    let CanvasObjectKind::Connector(connector) = &mut object.kind else {
        panic!()
    };
    connector.extra_labels.push(ConnectorLabel {
        text: "Extra".into(),
        placement: ConnectorLabelPlacement {
            orthogonal: -60.0,
            ..Default::default()
        },
        rich_text: Some(Arc::new(extra_rich)),
    });
    let object_id = object.id;
    let values = thinoda_core::sankey::evaluate(book.active()).unwrap();
    let object = book.active().canvas.object(object_id).unwrap();
    let CanvasObjectKind::Connector(connector) = &object.kind else {
        unreachable!()
    };
    let display = thinoda_core::sankey::flow_label(connector, values.mass[&object_id]);
    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    let restored = import(exported.as_bytes()).unwrap().workbook;
    let object = restored.sheets[0]
        .canvas
        .objects
        .iter()
        .find(|object| object.label() == "Ribbon")
        .unwrap();
    assert!(matches!(&object.kind, CanvasObjectKind::Connector(c) if c.sankey.is_some()));
    let CanvasObjectKind::Connector(connector) = &object.kind else {
        panic!()
    };
    assert_eq!(connector.extra_labels[0].text, "Extra");
    assert_eq!(
        connector.extra_labels[0].rich_text.as_ref().unwrap().runs[0]
            .style
            .bold,
        Some(true)
    );
    let rich = object.label_rich_text.as_ref().unwrap();
    assert_eq!(rich.runs.len(), 1);
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 6));
    assert_eq!(rich.runs[0].style.italic, Some(true));

    let original = esc(&html_rich_label(&display, rich, None));
    let mut edited = thinoda_core::InlineText::default();
    edited
        .update_range(&display, 0..display.chars().count(), |style| {
            style.italic = Some(true)
        })
        .unwrap();
    let changed = esc(&html_rich_label(&display, &edited, None));
    assert!(exported.contains(&original));
    let edited_xml = exported.replacen(&original, &changed, 1);
    let report = import(edited_xml.as_bytes()).unwrap();
    assert!(report.warnings.contains(&ImportWarning::DrawioSankeyStale));
    let object = report.workbook.sheets[0]
        .canvas
        .objects
        .iter()
        .find(|object| object.label() == display)
        .unwrap();
    assert!(matches!(&object.kind, CanvasObjectKind::Connector(c) if c.sankey.is_none()));
    assert_eq!(
        object.label_rich_text.as_ref().unwrap().runs[0].end as usize,
        display.chars().count()
    );
}

#[test]
fn html_whitespace_loss_is_reported_through_file_link_and_clipboard() {
    let source = "A<span> </span><span> B</span>";
    let xml = page(&format!(
        r##"<mxCell id="a" value="{}" style="shape=rectangle;html=1;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
        esc(source)
    ));
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "A  B");
    assert!(matches!(
        crate::import_bytes(Format::DrawIo, xml.as_bytes()),
        Err(IoError::ImportNeedsConfirmation(warnings))
            if warnings == vec![ImportWarning::Formatting]
    ));

    let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
    assert_eq!(pasted.warnings, report.warnings);
    let linked = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&linked).unwrap();
    assert_eq!(linked.warnings, report.warnings);
    assert_eq!(linked.workbook, report.workbook);
}

#[test]
fn html_label_entities_remain_intact_across_parser_chunks() {
    let source = format!(
        "<span>{}&NotEqualTilde;{}</span>",
        "a".repeat(32_760),
        "b".repeat(32_760)
    );
    let mut warnings = BTreeSet::new();
    assert_eq!(
        strip_html(&source, &mut warnings),
        format!(
            "{}\u{2242}\u{338}{}",
            "a".repeat(32_760),
            "b".repeat(32_760)
        )
    );
    assert!(warnings.is_empty());

    let source = format!("<span>{}\u{1f642}</span>", "a".repeat(16_377));
    assert_eq!(
        strip_html(&source, &mut warnings),
        format!("{}\u{1f642}", "a".repeat(16_377))
    );
    assert!(warnings.is_empty());
}

#[test]
fn html_whitespace_spanning_parser_chunks_is_reported() {
    let source = format!("<span>{}  B</span>", "a".repeat(16_377));
    let mut warnings = BTreeSet::new();
    assert_eq!(
        strip_html(&source, &mut warnings),
        format!("{}  B", "a".repeat(16_377))
    );
    assert_eq!(warnings, BTreeSet::from([ImportWarning::Formatting]));
}

#[test]
fn non_html_labels_remain_literal() {
    for source in ["A &copy;<div>B</div>", " A  B "] {
        let xml = page(&format!(
            r##"<mxCell id="a" value="{}" style="shape=rectangle;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
            esc(source)
        ));
        let report = load(&xml);
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), source);
        assert!(!report.warnings.contains(&ImportWarning::Formatting));
    }
}

#[test]
fn decoded_html_labels_obey_the_board_text_limit() {
    let source = format!("{}&copy;", "a".repeat(MAX_BOARD_TEXT_CHARS));
    let xml = page(&format!(
        r##"<mxCell id="a" value="{}" style="shape=rectangle;html=1;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
        esc(&source)
    ));
    let report = load(&xml);
    assert_eq!(
        shapes(&report.workbook.sheets[0])[0].label(),
        "a".repeat(MAX_BOARD_TEXT_CHARS)
    );
    assert!(report.warnings.contains(&ImportWarning::Other));
}

#[test]
fn html_labels_survive_file_link_clipboard_and_drawio_round_trip() {
    let shape = r#"Before<div title="2 > 1">A &copy;</div>After"#;
    let edge = "Line 1<div>Line 2 &NotEqualTilde;</div>Tail";
    let card = "<div>Note &nbsp; one</div><div>Note two</div>";
    let xml = format!(
        r##"<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="s" value="{}" style="shape=rectangle;html=1;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="120" height="60" as="geometry"/></mxCell>
        <mxCell id="e" value="{}" style="html=1;endArrow=none;fontSize=12;strokeColor=#000;fontColor=#000;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="100" as="sourcePoint"/><mxPoint x="120" y="100" as="targetPoint"/></mxGeometry></mxCell>
        <mxCell id="c" value="{}" style="shape=note;html=1;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry x="160" width="120" height="80" as="geometry"/></mxCell>
        </root></mxGraphModel>"##,
        esc(shape),
        esc(edge),
        esc(card),
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    let sheet = &report.workbook.sheets[0];
    assert_eq!(shapes(sheet)[0].label(), "Before\nA \u{a9}\nAfter");
    assert_eq!(
        connectors(sheet)[0].label,
        "Line 1\nLine 2 \u{2242}\u{338}\nTail"
    );
    assert!(
        sheet
            .topics
            .values()
            .any(|topic| topic.title == "Note \u{a0} one\nNote two")
    );
    assert!(matches!(
        crate::import_bytes(Format::DrawIo, xml.as_bytes()),
        Err(IoError::ImportNeedsConfirmation(_))
    ));

    let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
    assert_eq!(pasted.warnings, report.warnings);
    let linked = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&linked).unwrap();
    assert_eq!(linked.warnings, report.warnings);
    assert_eq!(linked.workbook, report.workbook);

    let exported = export(&report.workbook).unwrap();
    let restored = import(&exported).unwrap().workbook;
    let sheet = &restored.sheets[0];
    assert_eq!(shapes(sheet)[0].label(), "Before\nA \u{a9}\nAfter");
    assert_eq!(
        connectors(sheet)[0].label,
        "Line 1\nLine 2 \u{2242}\u{338}\nTail"
    );
    assert!(
        sheet
            .topics
            .values()
            .any(|topic| topic.title == "Note \u{a0} one\nNote two")
    );
}

#[test]
fn vertex_label_font_size_loss_is_reported_for_shapes_and_text() {
    for (shape, prefix, font, label, warned) in [
        ("rectangle", "", "", "Label", false),
        ("rectangle", "", "fontSize=12;", "Label", false),
        ("rectangle", "", "fontSize=11;", "Label", false),
        ("ellipse", "", "fontSize=11;", "Label", false),
        ("text", "", "fontSize=11;", "Label", false),
        ("rectangle", "", "fontSize=none;", "Label", false),
        ("rectangle", ";", "", "Label", false),
        ("rectangle", ";", "fontSize=12;", "Label", false),
        ("rectangle", "", "fontSize=invalid;", "Label", true),
        ("rectangle", "", "fontSize=11;", "", false),
        ("rectangle", ";", "", "", false),
    ] {
        let xml = page(&format!(
            r##"<mxCell id="a" value="{label}" style="{prefix}shape={shape};fillColor=#fff;strokeColor=#000;fontColor=#000;{font}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
        ))
        .replacen(
            "<mxGraphModel>",
            r#"<mxGraphModel adaptiveColors="none">"#,
            1,
        );
        let report = load(&xml);
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            warned,
            "{shape} {prefix} {font} {label:?}: {:?}",
            report.warnings
        );
        if warned {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(_))
            ));
        }
        let linked = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
        assert_eq!(
            crate::import_drawio_link(&linked).unwrap().warnings,
            report.warnings
        );
        assert_eq!(
            crate::drawio_clipboard::decode(&xml).unwrap().warnings,
            report.warnings
        );
    }
}

#[test]
fn named_vertex_styles_with_lost_label_appearance_are_reported() {
    for (style, label, warned) in [
        ("ellipse;", "Label", false),
        ("rhombus;", "Label", false),
        ("triangle;", "Label", false),
        ("text;", "Label", true),
        ("text;fontSize=12;", "Label", true),
        ("text;fontSize=11;", "Label", true),
        ("label;", "Label", true),
        ("edgeLabel;", "Label", true),
        ("defaultEdge;", "Label", true),
        ("customStyle;", "Label", true),
        ("ellipse;edgeLabel;", "Label", true),
        ("ellipse;defaultVertex;", "Label", true),
        ("ellipse;fontSize=12;edgeLabel;", "Label", true),
        ("ellipse;edgeLabel;fontSize=12;", "Label", true),
        ("ellipse;customStyle;fontSize=12;", "Label", true),
        ("ellipse;edgeLabel;", "", false),
        ("text;", "", false),
    ] {
        let xml = page(&format!(
            r##"<mxCell id="a" value="{label}" style="{style}fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            warned,
            "{style} {label:?}: {:?}",
            report.warnings
        );
        if warned {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(_))
            ));
        }
        let linked = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
        assert_eq!(
            crate::import_drawio_link(&linked).unwrap().warnings,
            report.warnings
        );
        assert_eq!(
            crate::drawio_clipboard::decode(&xml).unwrap().warnings,
            report.warnings
        );
    }
}

#[test]
fn edge_label_font_size_loss_and_folded_children_are_reported() {
    for (parent_font, parent_label, child_style, child_label, warned) in [
        ("", "Label", "", "", true),
        ("fontSize=11;", "Label", "", "", true),
        ("fontSize=none;", "Label", "", "", true),
        ("fontSize=12;", "Label", "", "", false),
        ("fontSize=11;", "", "", "", false),
        ("fontSize=11;", "", "fontSize=12;", "Child", true),
        ("fontSize=12;", "", "fontSize=11;", "Child", true),
        ("fontSize=12;", "", "fontSize=none;", "Child", true),
        ("fontSize=12;", "", "edgeLabel;", "Child", true),
        ("fontSize=12;", "", "edgeLabel;fontSize=12;", "Child", true),
        ("fontSize=12;", "", ";fontSize=12;", "Child", true),
        ("fontSize=12;", "", "", "Child", true),
        ("defaultEdge;fontSize=12;", "Label", "", "", true),
        ("fontSize=12;defaultEdge;", "Label", "", "", true),
    ] {
        let child = if child_label.is_empty() {
            String::new()
        } else {
            format!(
                r#"<mxCell id="label" value="{child_label}" style="{child_style}" vertex="1" parent="e"><mxGeometry relative="1" as="geometry"/></mxCell>"#
            )
        };
        let xml = page(&format!(
            r##"<mxCell id="e" value="{parent_label}" style="endArrow=none;strokeColor=#000;fontColor=#000;{parent_font}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="100" y="0" as="targetPoint"/></mxGeometry></mxCell>{child}"##
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            warned,
            "{parent_font} {parent_label:?} {child_style} {child_label:?}: {:?}",
            report.warnings
        );
        if warned {
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(_))
            ));
        }
        let linked = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
        assert_eq!(
            crate::import_drawio_link(&linked).unwrap().warnings,
            report.warnings
        );
        assert_eq!(
            crate::drawio_clipboard::decode(&xml).unwrap().warnings,
            report.warnings
        );
    }
}

#[test]
fn multipage_clipboard_reports_font_size_loss_only_from_the_pasted_page() {
    let graph = |font: &str| {
        format!(
            r##"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="e" value="Label" style="endArrow=none;strokeColor=#000;fontColor=#000;fontSize={font};" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="100" y="0" as="targetPoint"/></mxGeometry></mxCell></root></mxGraphModel>"##
        )
    };
    for (first, later, first_warned) in [("12", "11", false), ("11", "12", true)] {
        let xml = format!(
            r#"<mxfile><diagram name="First">{}</diagram><diagram name="Later">{}</diagram></mxfile>"#,
            graph(first),
            graph(later)
        );
        let report = load(&xml);
        assert!(report.warnings.contains(&ImportWarning::Formatting));
        let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
        assert_eq!(
            pasted.warnings.contains(&ImportWarning::Formatting),
            first_warned,
            "{first} {later}: {:?}",
            pasted.warnings
        );
        assert!(pasted.warnings.contains(&ImportWarning::Other));
    }
}

#[test]
fn drawio_export_uses_canvas_font_size_for_board_connector_labels() {
    let mut sheet = Sheet::new("Connectors", "");
    sheet.root_mode = SheetRootMode::Hidden;
    for (y, label) in [(0.0, "Label"), (40.0, "")] {
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::new(0.0, y)),
            Endpoint::Point(Vec2::new(100.0, y)),
        );
        connector.arrow_end = false;
        connector.label = label.into();
        let mut object = CanvasObject::connector(connector);
        object.style.text_color = Some(Color::BLACK);
        sheet.add_canvas_object(object).unwrap();
    }

    let book = Workbook::new(sheet);
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    let labelled = xml
        .split("<mxCell ")
        .find(|cell| cell.contains(r#"value="Label""#))
        .unwrap();
    assert!(labelled.contains("fontSize=12;"), "{labelled}");
    let unlabelled = xml
        .split("<mxCell ")
        .find(|cell| cell.contains(r#"value="""#) && cell.contains(r#"edge="1""#))
        .unwrap();
    assert!(!unlabelled.contains("fontSize="), "{unlabelled}");
    let report = load(&xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(connectors(&report.workbook.sheets[0])[0].label, "Label");

    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        let losses = export_losses(&book, format, &ExportOptions::default());
        assert!(
            !losses
                .iter()
                .any(|loss| matches!(loss, ExportLoss::DrawioLabelAppearanceApproximated(_))),
            "{format:?}: {losses:?}"
        );
        let bytes = export_bytes(&book, format, &ExportOptions::default()).unwrap();
        let restored = import(&bytes).unwrap();
        assert!(
            restored.warnings.is_empty(),
            "{format:?}: {:?}",
            restored.warnings
        );
        assert_eq!(connectors(&restored.workbook.sheets[0])[0].label, "Label");
    }
}

#[test]
fn drawio_export_matches_relationship_label_font_size_at_normal_zoom() {
    let mut sheet = Sheet::new("Map", "Root");
    let first = sheet.add_child(sheet.root, None, "First");
    let second = sheet.add_child(sheet.root, None, "Second");
    let relation = sheet.add_relationship(first, second).unwrap();
    sheet.set_relationship_title(relation, "Relates").unwrap();
    let book = Workbook::new(sheet);

    let losses = export_losses(&book, Format::DrawIo, &ExportOptions::default());
    assert!(
        losses
            .iter()
            .any(|loss| matches!(loss, ExportLoss::MapElementsFlattened(_)))
    );
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    let relation = xml
        .split("<mxCell ")
        .find(|cell| cell.contains(r#"value="Relates""#))
        .unwrap();
    assert!(relation.contains("fontSize=12;"), "{relation}");
    let report = load(&xml);
    assert!(
        connectors(&report.workbook.sheets[0])
            .iter()
            .any(|connector| connector.label == "Relates")
    );
}

#[test]
fn final_state_appearance_losses_cover_visible_fill_outline_and_label() {
    let mut sheet = Sheet::new("Final states", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let frame = rect(0.0, 0.0, 100.0, 60.0);
    let mut automatic = CanvasObject::shape(ShapeKind::FinalState, frame);
    automatic.style.fill = None;
    automatic.style.stroke = None;
    automatic.style.stroke_width = 17.0;
    sheet.add_canvas_object(automatic).unwrap();
    let mut fill = CanvasObject::shape(ShapeKind::FinalState, frame);
    fill.style.fill = Some(Color::rgb(20, 40, 60));
    fill.style.stroke = None;
    sheet.add_canvas_object(fill).unwrap();
    let mut outline = CanvasObject::shape(ShapeKind::FinalState, frame);
    outline.style.fill = Some(Color::TRANSPARENT);
    sheet.add_canvas_object(outline).unwrap();
    let mut label = CanvasObject::shape(ShapeKind::FinalState, frame);
    label.style.fill = Some(Color::TRANSPARENT);
    label.style.stroke = Some(Color::TRANSPARENT);
    if let CanvasObjectKind::Shape { label, .. } = &mut label.kind {
        *label = "Visible label".into();
    }
    sheet.add_canvas_object(label).unwrap();
    let mut invisible = CanvasObject::shape(ShapeKind::FinalState, frame);
    invisible.style.fill = Some(Color::TRANSPARENT);
    invisible.style.stroke = Some(Color::TRANSPARENT);
    sheet.add_canvas_object(invisible).unwrap();
    let mut transparent = CanvasObject::shape(ShapeKind::FinalState, frame);
    transparent.style.opacity = 0.0;
    sheet.add_canvas_object(transparent).unwrap();

    let book = Workbook::new(sheet);
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![ExportLoss::DrawioFinalStateAppearanceApproximated(4)],
            "{format:?}"
        );
    }
    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    let automatic = exported
        .split("<mxCell ")
        .find(|cell| cell.contains("shape=endState;") && cell.contains("strokeWidth=2;"))
        .unwrap();
    assert!(automatic.contains("fillColor=none;"));
    let restored = load(&exported);
    assert_eq!(
        shapes(&restored.workbook.sheets[0])[0]
            .style
            .effective_shape_stroke_width(),
        2.0
    );
    assert!(export_losses(&book, Format::Thinoda, &ExportOptions::default()).is_empty());
}

#[test]
fn double_ellipse_appearance_losses_cover_visible_fill_outline_and_label() {
    let mut sheet = Sheet::new("Ellipses", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let frame = rect(0.0, 0.0, 100.0, 60.0);
    let mut automatic = CanvasObject::shape(ShapeKind::DoubleEllipse, frame);
    automatic.style.stroke_width = 17.0;
    sheet.add_canvas_object(automatic).unwrap();
    let mut fill = CanvasObject::shape(ShapeKind::DoubleEllipse, frame);
    fill.style.fill = Some(Color::rgb(20, 40, 60));
    sheet.add_canvas_object(fill).unwrap();
    let mut outline = CanvasObject::shape(ShapeKind::DoubleEllipse, frame);
    outline.style.fill = Some(Color::TRANSPARENT);
    outline.style.stroke = Some(Color::BLACK);
    sheet.add_canvas_object(outline).unwrap();
    let mut label = CanvasObject::shape(ShapeKind::DoubleEllipse, frame);
    label.style.fill = Some(Color::TRANSPARENT);
    label.style.stroke = Some(Color::TRANSPARENT);
    if let CanvasObjectKind::Shape { label, .. } = &mut label.kind {
        *label = "Visible label".into();
    }
    sheet.add_canvas_object(label).unwrap();
    let mut invisible = CanvasObject::shape(ShapeKind::DoubleEllipse, frame);
    invisible.style.fill = Some(Color::TRANSPARENT);
    invisible.style.stroke = Some(Color::TRANSPARENT);
    sheet.add_canvas_object(invisible).unwrap();
    let mut transparent = CanvasObject::shape(ShapeKind::DoubleEllipse, frame);
    transparent.style.opacity = 0.0;
    sheet.add_canvas_object(transparent).unwrap();

    let book = Workbook::new(sheet);
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![ExportLoss::DrawioDoubleEllipseAppearanceApproximated(4)],
            "{format:?}"
        );
    }
    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    let automatic = exported
        .split("<mxCell ")
        .find(|cell| cell.contains("shape=doubleEllipse;") && cell.contains("strokeWidth=2;"))
        .unwrap();
    assert!(automatic.contains("fillColor=none;"));
    let restored = load(&exported);
    assert_eq!(
        shapes(&restored.workbook.sheets[0])[0]
            .style
            .effective_shape_stroke_width(),
        2.0
    );
    assert!(export_losses(&book, Format::Thinoda, &ExportOptions::default()).is_empty());
}

#[test]
fn simple_ellipse_and_flowchart_ellipse_stencils_do_not_gain_the_warning() {
    for shape in [
        "ellipse",
        "mxgraph.flowchart.start_1",
        "mxgraph.flowchart.start_2",
        "mxgraph.flowchart.on-page_reference",
    ] {
        let xml = page(&format!(
            r##"<mxCell id="a" value="Start" style="shape={shape};fillColor=#fff;strokeColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
        ))
        .replacen(
            "<mxGraphModel>",
            r#"<mxGraphModel adaptiveColors="none">"#,
            1,
        );
        let report = load(&xml);
        assert!(report.warnings.is_empty(), "{shape}: {:?}", report.warnings);
        assert!(matches!(
            shapes(&report.workbook.sheets[0])[0].kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::Ellipse,
                ..
            }
        ));
    }
}

#[test]
fn flowchart_process_import_reports_rounded_corner_loss() {
    let xml = page(
        r##"<mxCell id="a" value="Task" style="shape=mxgraph.flowchart.process;fillColor=#ffffff;strokeColor=#123456;rotation=30;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="300" height="200" as="geometry"/></mxCell>"##,
    )
    .replacen(
        "<mxGraphModel>",
        r#"<mxGraphModel adaptiveColors="none">"#,
        1,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    let object = shapes(&report.workbook.sheets[0])[0];
    assert!(matches!(
        object.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Process,
            ..
        }
    ));
    assert_eq!(object.label(), "Task");
    assert_eq!(object.rect, rect(10.0, 20.0, 300.0, 200.0));
    assert_eq!(object.rotation, 30.0);
    assert_eq!(object.style.fill, Some(Color::WHITE));
    assert_eq!(object.style.stroke, Some(Color::rgb(18, 52, 86)));
    assert!(matches!(
        crate::import_bytes(Format::DrawIo, xml.as_bytes()),
        Err(IoError::ImportNeedsConfirmation(warnings))
            if warnings == vec![ImportWarning::Formatting]
    ));
    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    assert_eq!(
        crate::import_drawio_link(&link).unwrap().warnings,
        report.warnings
    );
    assert_eq!(
        crate::drawio_clipboard::decode(&xml).unwrap().warnings,
        report.warnings
    );
    let native =
        export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        crate::import_bytes(Format::Thinoda, &native).unwrap(),
        report.workbook
    );
}

#[test]
fn flowchart_process_direction_and_collapsed_frames_still_warn() {
    for (direction, width, height, expected_frame, rotation) in [
        ("east", 300, 200, rect(10.0, 20.0, 300.0, 200.0), 0.0),
        ("west", 300, 200, rect(10.0, 20.0, 300.0, 200.0), 180.0),
        ("south", 300, 200, rect(60.0, -30.0, 200.0, 300.0), 90.0),
        ("north", 300, 200, rect(60.0, -30.0, 200.0, 300.0), 270.0),
        ("east", 0, 200, rect(10.0, 20.0, 0.0, 200.0), 0.0),
        ("east", 300, 0, rect(10.0, 20.0, 300.0, 0.0), 0.0),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" style="shape=mxgraph.flowchart.process;direction={direction};" vertex="1" parent="1"><mxGeometry x="10" y="20" width="{width}" height="{height}" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(object.rect, expected_frame, "{direction} {width} {height}");
        assert_eq!(object.rotation, rotation, "{direction} {width} {height}");
    }
}

#[test]
fn native_process_exports_as_a_square_rectangle_with_roundtrip_identity() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut process = shape(ShapeKind::Process, rect(10.0, 20.0, 300.0, 200.0), "Task");
    process.style.fill = Some(Color::WHITE);
    process.style.stroke = Some(Color::rgb(18, 52, 86));
    process.rotation = 30.0;
    sheet.add_canvas_object(process.clone()).unwrap();
    let exported = String::from_utf8(export(&Workbook::new(sheet)).unwrap()).unwrap();
    assert!(
        exported.contains("shape=rectangle;rounded=0;thinodaShape=process;"),
        "{exported}"
    );
    assert!(!exported.contains("shape=mxgraph.flowchart.process;"));
    let report = load(&exported);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let restored = shapes(&report.workbook.sheets[0])[0];
    assert_eq!(restored.kind, process.kind);
    assert_eq!(restored.rect, process.rect);
    assert_eq!(restored.rotation, process.rotation);
    assert_eq!(restored.style, process.style);
}

#[test]
fn collapsed_native_process_outline_is_visible_in_png() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut process = shape(ShapeKind::Process, rect(20.0, 20.0, 0.0, 100.0), "");
    process.style.stroke = Some(Color::BLACK);
    process.style.stroke_width = 4.0;
    sheet.add_canvas_object(process).unwrap();
    let book = Workbook::new(sheet);
    let options = ExportOptions {
        transparent: true,
        ..Default::default()
    };
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    let values: Vec<f32> = svg
        .split("viewBox=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .split_whitespace()
        .map(|part| part.parse().unwrap())
        .collect();
    let png = export_bytes(&book, Format::Png, &options).unwrap();
    let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
    let x = (20.0 - values[0]).floor() as u32;
    let y = (70.0 - values[1]).floor() as u32;
    assert!(pixels.get_pixel(x, y).0[3] > 0);
}

#[test]
fn drawio_export_reports_visible_collapsed_process_outlines() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    for (frame, stroke, fill, opacity) in [
        (rect(0.0, 0.0, 0.0, 100.0), Some(Color::BLACK), None, 1.0),
        (rect(50.0, 0.0, 100.0, 0.0), None, None, 1.0),
        (
            rect(0.0, 200.0, 100.0, 100.0),
            Some(Color::BLACK),
            None,
            1.0,
        ),
        (
            rect(0.0, 400.0, 0.0, 100.0),
            Some(Color::TRANSPARENT),
            None,
            1.0,
        ),
        (rect(50.0, 400.0, 0.0, 100.0), Some(Color::BLACK), None, 0.0),
        (
            rect(100.0, 400.0, 0.0, 100.0),
            None,
            Some(Color::WHITE),
            1.0,
        ),
    ] {
        let mut process = shape(ShapeKind::Process, frame, "");
        process.style.stroke = stroke;
        process.style.fill = fill;
        process.style.opacity = opacity;
        sheet.add_canvas_object(process).unwrap();
    }
    let book = Workbook::new(sheet);
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![ExportLoss::DrawioCollapsedProcessApproximated(2)],
            "{format:?}"
        );
    }
    assert!(export_losses(&book, Format::Thinoda, &ExportOptions::default()).is_empty());
}

#[test]
fn collapsed_process_loss_respects_editable_image_sheet_scope() {
    let mut first = Sheet::new("First", "");
    first.root_mode = SheetRootMode::Hidden;
    first
        .add_canvas_object(shape(ShapeKind::Process, rect(0.0, 0.0, 0.0, 100.0), ""))
        .unwrap();
    let mut second = Sheet::new("Second", "");
    second.root_mode = SheetRootMode::Hidden;
    for x in [0.0, 100.0] {
        second
            .add_canvas_object(shape(ShapeKind::Process, rect(x, 0.0, 100.0, 0.0), ""))
            .unwrap();
    }
    let mut book = Workbook::new(first);
    book.sheets.push(second);
    for (sheet, expected) in [(0, 1), (1, 2)] {
        let options = ExportOptions {
            sheet: Some(sheet),
            ..Default::default()
        };
        for format in [Format::DrawIoSvg, Format::DrawIoPng] {
            assert_eq!(
                export_losses(&book, format, &options),
                vec![ExportLoss::DrawioCollapsedProcessApproximated(expected)],
                "{format:?}, sheet {sheet}"
            );
        }
        assert_eq!(
            export_losses(&book, Format::DrawIo, &options),
            vec![ExportLoss::DrawioCollapsedProcessApproximated(3)]
        );
    }
    book.active_sheet = 1;
    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![ExportLoss::DrawioCollapsedProcessApproximated(2)]
        );
    }
}

#[test]
fn native_process_marker_never_overrides_changed_drawio_geometry() {
    for (style, expected, warning) in [
        (
            "shape=rectangle;rounded=0;thinodaShape=process;",
            ShapeKind::Process,
            None,
        ),
        (
            "shape=rectangle;rounded=1;thinodaShape=process;",
            ShapeKind::RoundedRectangle,
            None,
        ),
        (
            "shape=ellipse;rounded=0;thinodaShape=process;",
            ShapeKind::Ellipse,
            None,
        ),
        (
            "shape=rectangle;thinodaShape=process;",
            ShapeKind::Rectangle,
            None,
        ),
        (
            "shape=label;rounded=0;thinodaShape=process;",
            ShapeKind::Rectangle,
            None,
        ),
        (
            "shape=mxgraph.flowchart.process;thinodaShape=process;",
            ShapeKind::Process,
            Some(ImportWarning::Formatting),
        ),
        (
            "mxgraph.flowchart.process;",
            ShapeKind::Rectangle,
            Some(ImportWarning::Other),
        ),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" style="{style}" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#
        ))
        .replacen(
            "<mxGraphModel>",
            r#"<mxGraphModel adaptiveColors="none">"#,
            1,
        );
        let report = load(&xml);
        let expected_warnings = match warning {
            Some(ImportWarning::Other) => {
                vec![ImportWarning::UnsupportedDrawioShapes, ImportWarning::Other]
            }
            other => other.into_iter().collect(),
        };
        assert_eq!(report.warnings, expected_warnings, "{style}");
        assert!(
            matches!(
                shapes(&report.workbook.sheets[0])[0].kind,
                CanvasObjectKind::Shape { shape, .. } if shape == expected
            ),
            "{style}"
        );
    }
}

#[test]
fn flowchart_annotation_imports_as_an_editable_open_bracket() {
    let xml = page(
        r##"<mxCell id="a" value="Note" style="shape=mxgraph.flowchart.annotation_1;fillColor=#ff0000;strokeColor=#336699;strokeWidth=4;strokeOpacity=50;rotation=180;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>
        <mxCell id="b" style="shape=rectangle;" vertex="1" parent="1"><mxGeometry x="200" y="20" width="100" height="60" as="geometry"/></mxCell>
        <mxCell id="e" style="endArrow=none;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"##,
    )
    .replacen(
        "<mxGraphModel>",
        r#"<mxGraphModel adaptiveColors="none">"#,
        1,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    let sheet = &report.workbook.sheets[0];
    let objects = shapes(sheet);
    let bracket = objects[0];
    assert!(matches!(
        bracket.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            ..
        }
    ));
    assert_eq!(bracket.label(), "Note");
    assert_eq!(bracket.rect, rect(10.0, 20.0, 100.0, 60.0));
    assert_eq!(bracket.rotation, 180.0);
    assert_eq!(bracket.style.fill, Some(Color::TRANSPARENT));
    assert_eq!(bracket.style.stroke, Some(Color::rgba(51, 102, 153, 128)));
    assert_eq!(bracket.style.stroke_width, 4.0);
    assert_eq!(
        bracket.rectangle_sides,
        Some(RectangleSides {
            top: true,
            right: false,
            bottom: true,
            left: true,
        })
    );
    assert_eq!(connectors(sheet)[0].from, Endpoint::Object(bracket.id));
    assert_eq!(connectors(sheet)[0].to, Endpoint::Object(objects[1].id));
    report.workbook.validate().unwrap();
    assert_eq!(feature_version(&report.workbook), 20);
    assert!(matches!(
        crate::import_bytes(Format::DrawIo, xml.as_bytes()),
        Err(IoError::ImportNeedsConfirmation(warnings))
            if warnings == vec![ImportWarning::Formatting]
    ));

    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    assert_eq!(
        crate::import_drawio_link(&link).unwrap().warnings,
        report.warnings
    );
    assert_eq!(
        crate::drawio_clipboard::decode(&xml).unwrap().warnings,
        report.warnings
    );
    let native =
        export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        crate::import_bytes(Format::Thinoda, &native).unwrap(),
        report.workbook
    );
    let drawio = export_bytes(&report.workbook, Format::DrawIo, &ExportOptions::default()).unwrap();
    let reopened = import(&drawio).unwrap();
    let restored = shapes(&reopened.workbook.sheets[0])[0];
    assert_eq!(restored.rectangle_sides, bracket.rectangle_sides);
    assert_eq!(restored.rect, bracket.rect);
    assert_eq!(restored.rotation, bracket.rotation);
    assert_eq!(restored.style.fill, bracket.style.fill);
    assert_eq!(restored.style.stroke, bracket.style.stroke);
}

#[test]
fn flowchart_annotation_resolves_visible_and_removed_strokes() {
    for (prefix, style, expected) in [
        ("", "", Color::BLACK),
        ("", "strokeOpacity=0;", Color::TRANSPARENT),
        ("", "strokeColor=none;", Color::TRANSPARENT),
        (
            "",
            "strokeColor=#abcdef;strokeOpacity=50;",
            Color::rgba(171, 205, 239, 128),
        ),
        (";", "", Color::TRANSPARENT),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" style="{prefix}shape=mxgraph.flowchart.annotation_1;{style}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{prefix}{style}"
        );
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(
            object.style.fill,
            Some(Color::TRANSPARENT),
            "{prefix}{style}"
        );
        assert_eq!(object.style.stroke, Some(expected), "{prefix}{style}");
    }
}

#[test]
fn flowchart_annotation_identifier_is_not_a_named_style() {
    let xml = page(
        r#"<mxCell id="a" style="mxgraph.flowchart.annotation_1;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(
        report.warnings,
        vec![ImportWarning::UnsupportedDrawioShapes, ImportWarning::Other]
    );
    assert_eq!(shapes(&report.workbook.sheets[0])[0].rectangle_sides, None);
}

#[test]
fn collapsed_flowchart_annotations_do_not_gain_an_outline() {
    for (width, height) in [(0, 60), (100, 0), (0, 0)] {
        let xml = page(&format!(
            r##"<mxCell id="a" style="shape=mxgraph.flowchart.annotation_1;strokeColor=#336699;" vertex="1" parent="1"><mxGeometry width="{width}" height="{height}" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(
            object.rectangle_sides,
            Some(RectangleSides {
                top: false,
                right: false,
                bottom: false,
                left: false,
            })
        );
        assert_eq!(object.style.stroke, Some(Color::rgb(51, 102, 153)));
    }
}

#[test]
fn flowchart_annotation_direction_preserves_the_drawio_bounds() {
    for (direction, frame, rotation) in [
        ("east", rect(10.0, 20.0, 100.0, 60.0), 0.0),
        ("west", rect(10.0, 20.0, 100.0, 60.0), 180.0),
        ("south", rect(30.0, 0.0, 60.0, 100.0), 90.0),
        ("north", rect(30.0, 0.0, 60.0, 100.0), 270.0),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" style="shape=mxgraph.flowchart.annotation_1;direction={direction};" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(object.rect, frame, "{direction}");
        assert_eq!(object.rotation, rotation, "{direction}");
    }
}

#[test]
fn flowchart_annotation_visual_exports_keep_the_open_side_and_transparent_interior() {
    let options = ExportOptions {
        transparent: true,
        ..Default::default()
    };
    for rotation in [0, 180] {
        let xml = page(&format!(
            r##"<mxCell id="a" style="shape=mxgraph.flowchart.annotation_1;fillColor=#ff0000;strokeColor=#336699;strokeWidth=4;rotation={rotation};" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>"##
        ));
        let book = load(&xml).workbook;
        let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
        assert!(!svg.contains("#FF0000"));
        let view_box = svg
            .split("viewBox=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let values: Vec<f32> = view_box
            .split_whitespace()
            .map(|part| part.parse().unwrap())
            .collect();
        let png = export_bytes(&book, Format::Png, &options).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        let alpha = |x: f32, y: f32| {
            pixels
                .get_pixel(
                    (x - values[0]).floor() as u32,
                    (y - values[1]).floor() as u32,
                )
                .0[3]
        };
        assert!(alpha(60.0, 20.0) > 0, "{rotation}: missing top");
        assert!(alpha(60.0, 80.0) > 0, "{rotation}: missing bottom");
        let (closed, open) = if rotation == 0 {
            (10.0, 110.0)
        } else {
            (110.0, 10.0)
        };
        assert!(alpha(closed, 50.0) > 0, "{rotation}: missing side");
        assert_eq!(alpha(open, 50.0), 0, "{rotation}: side should be open");
        assert_eq!(alpha(60.0, 50.0), 0, "{rotation}: unexpected fill");
    }
}

#[test]
fn removed_font_color_uses_the_drawio_default_across_board_items() {
    let xml = page(
        r##"<mxCell id="a" value="Shape" style="shape=rectangle;fontColor=none;textOpacity=40;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="60" as="geometry"/></mxCell>
        <mxCell id="b" value="Text" style="text;fontColor=none;textOpacity=0;" vertex="1" parent="1"><mxGeometry x="140" y="0" width="100" height="60" as="geometry"/></mxCell>
        <mxCell id="c" value="Note" style="shape=note;fontColor=none;textOpacity=40;" vertex="1" parent="1"><mxGeometry x="0" y="100" width="120" height="80" as="geometry"/></mxCell>
        <mxCell id="d" value="Line" style="shape=line;fontColor=none;textOpacity=40;" vertex="1" parent="1"><mxGeometry x="0" y="220" width="100" height="20" as="geometry"/></mxCell>
        <mxCell id="e" value="Edge" style="endArrow=none;fontColor=none;textOpacity=40;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"##,
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    let sheet = &report.workbook.sheets[0];
    for (label, expected) in [
        ("Shape", Color::rgba(0, 0, 0, 102)),
        ("Text", Color::TRANSPARENT),
        ("Line", Color::rgba(0, 0, 0, 102)),
        ("Edge", Color::rgba(0, 0, 0, 102)),
    ] {
        let object = sheet
            .canvas
            .objects
            .iter()
            .find(|object| object.label() == label)
            .unwrap();
        assert_eq!(object.style.text_color, Some(expected), "{label}");
    }
    let note = sheet
        .topics
        .values()
        .find(|topic| topic.title == "Note")
        .unwrap();
    assert_eq!(note.style.text_color, Some(Color::rgba(0, 0, 0, 102)));
    report.workbook.validate().unwrap();

    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert_eq!(linked.workbook, report.workbook);
    assert_eq!(linked.warnings, report.warnings);
}

#[test]
fn unresolved_or_invalid_default_text_opacity_is_reported() {
    for style in [
        "shape=rectangle;textOpacity=40;",
        "shape=note;fontColor=default;textOpacity=50;",
        "shape=line;fontColor=inherit;textOpacity=10;",
        "shape=rectangle;fontColor=#ff0000;textOpacity=invalid;",
        "shape=rectangle;fontColor=inherit;",
        "shape=rectangle;fontColor=invalid;",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="Label" style="{style}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn appearance_colors_that_cannot_be_preserved_are_reported() {
    for key in ["fillColor", "strokeColor", "fontColor"] {
        for value in [
            "",
            "inherit",
            "swimlane",
            "indicated",
            "default",
            "fff",
            "##fff",
            "light-dark(#aabbcc,#ddeeff)",
        ] {
            let xml = page(&format!(
                r##"<mxCell id="a" value="Label" style="shape=rectangle;fillColor=#fff;strokeColor=#000;fontColor=#000;{key}={value};" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
            ));
            let report = load(&xml);
            assert_eq!(
                report.warnings,
                vec![ImportWarning::Formatting],
                "{key}={value}"
            );
            assert_eq!(shapes(&report.workbook.sheets[0]).len(), 1);
        }
    }
}

#[test]
fn appearance_opacity_and_stroke_width_losses_are_reported() {
    for (element, style) in [
        ("vertex", "shape=rectangle;fillOpacity=40;"),
        ("vertex", "shape=rectangle;strokeOpacity=40;"),
        (
            "vertex",
            "shape=rectangle;fillColor=#fff;fillOpacity=invalid;",
        ),
        (
            "vertex",
            "shape=rectangle;strokeColor=#000;strokeOpacity=NaN;",
        ),
        ("vertex", "shape=rectangle;opacity=invalid;"),
        ("vertex", "shape=rectangle;opacity=NaN;"),
        ("vertex", "shape=rectangle;opacity=-1;"),
        ("vertex", "shape=rectangle;opacity=101;"),
        ("vertex", "shape=rectangle;textOpacity=101;"),
        ("vertex", "shape=rectangle;strokeWidth=0;"),
        ("vertex", "shape=rectangle;strokeWidth=201;"),
        ("vertex", "shape=rectangle;strokeWidth=inf;"),
        ("edge", "strokeColor=inherit;endArrow=none;"),
        ("edge", "strokeOpacity=40;endArrow=none;"),
        ("edge", "strokeColor=#000;strokeWidth=0;endArrow=none;"),
        ("edge", "thinodaInk=1;strokeColor=inherit;endArrow=none;"),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" style="{style}" {element}="1" parent="1"><mxGeometry width="100" height="60" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="100" y="0" as="targetPoint"/></mxGeometry></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings,
            vec![ImportWarning::Formatting],
            "{element} {style}"
        );
        assert_eq!(report.workbook.sheets[0].canvas.objects.len(), 1);
    }
}

#[test]
fn appearance_swimlane_body_colors_that_cannot_be_preserved_are_reported() {
    for value in ["", "inherit", "default", "light-dark(#aabbcc,#ddeeff)"] {
        let xml = page(&format!(
            r#"<mxCell id="lane" value="Lane" style="swimlane;swimlaneFillColor={value};" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings,
            vec![ImportWarning::Formatting],
            "swimlaneFillColor={value}"
        );
        assert_eq!(report.workbook.sheets[0].zones.len(), 1);
    }

    for value in ["none", "transparent"] {
        let xml = page(&format!(
            r#"<mxCell id="lane" value="Lane" style="swimlane;startSize=24;collapsible=0;swimlaneFillColor={value};" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(report.warnings.is_empty(), "{value}: {:?}", report.warnings);
        report.workbook.validate().unwrap();
    }

    for value in ["#abcdef", "red", "rgb(10,20,30)"] {
        let xml = page(&format!(
            r#"<mxCell id="lane" value="Lane" style="swimlane;swimlaneFillColor={value};" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{value}");
        assert!(
            report.workbook.sheets[0].zones[0]
                .background_color
                .is_some()
        );
        report.workbook.validate().unwrap();
    }
}

#[test]
fn appearance_supported_explicit_colors_and_opacities_stay_unwarned() {
    for style in [
        "fillColor=#ff0000;fillOpacity=40;strokeColor=#00ff00;strokeOpacity=60;fontColor=#000000;textOpacity=100;",
        "fillColor=none;fillOpacity=40;strokeColor=none;strokeOpacity=60;",
        "strokeColor=#000;strokeWidth=0.1;opacity=100;",
        "strokeColor=#000;strokeWidth=200;opacity=0;",
        "fillOpacity=100;strokeOpacity=100;",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" style="shape=rectangle;{style}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        report.workbook.validate().unwrap();
    }
}

#[test]
fn appearance_warning_reaches_strict_import_links_and_clipboard() {
    let xml = page(
        r##"<mxCell id="a" value="Theme" style="shape=rectangle;fillColor=light-dark(#ffffff,#111111);strokeColor=inherit;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    assert_eq!(
        shapes(&report.workbook.sheets[0])[0].style.fill,
        Some(Color::WHITE)
    );
    assert!(matches!(
        crate::import_bytes(Format::DrawIo, xml.as_bytes()),
        Err(IoError::ImportNeedsConfirmation(warnings))
            if warnings == vec![ImportWarning::Formatting]
    ));
    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert_eq!(linked.warnings, report.warnings);
    assert_eq!(linked.workbook, report.workbook);
    assert_eq!(
        crate::drawio_clipboard::decode(&xml).unwrap().warnings,
        report.warnings
    );
    let native =
        export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        crate::import_bytes(Format::Thinoda, &native).unwrap(),
        report.workbook
    );
}

#[test]
fn css_color_values_import_as_explicit_colors() {
    for (value, expected) in [
        ("rgb(255, 255, 255)", Color::WHITE),
        ("rgb(100%,0%,50%)", Color::rgb(255, 0, 128)),
        ("rgba(10,20,30,0.5)", Color::rgba(10, 20, 30, 128)),
        (
            "rgba(10,20,30,0.5);fillOpacity=50",
            Color::rgba(10, 20, 30, 64),
        ),
        (
            "rgba(10,20,30,0.3);fillOpacity=50",
            Color::rgba(10, 20, 30, 39),
        ),
        ("hsl(120,100%,50%)", Color::rgb(0, 255, 0)),
        ("hsl(0.5turn 100% 50% / 25%)", Color::rgba(0, 255, 255, 64)),
        (
            "hsl(0 100% 50% / 30%);fillOpacity=50",
            Color::rgba(255, 0, 0, 39),
        ),
        ("hwb(240 0% 0%)", Color::rgb(0, 0, 255)),
        (
            "hwb(240 0% 0% / 30%);fillOpacity=50",
            Color::rgba(0, 0, 255, 39),
        ),
        ("rgb(255 0 0 / 50%)", Color::rgba(255, 0, 0, 128)),
        ("rgb(1,/* ( ) */2,3)", Color::rgb(1, 2, 3)),
        ("cornflowerblue", Color::rgb(100, 149, 237)),
        ("ReD", Color::rgb(255, 0, 0)),
        ("transparent", Color::TRANSPARENT),
        ("#f008", Color::rgba(255, 0, 0, 136)),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="Color" style="shape=rectangle;fillColor={value};strokeColor=blue;fontColor=green;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(report.warnings.is_empty(), "{value}: {:?}", report.warnings);
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(object.style.fill, Some(expected), "{value}");
        assert_eq!(object.style.stroke, Some(Color::rgb(0, 0, 255)), "{value}");
        assert_eq!(
            object.style.text_color,
            Some(Color::rgb(0, 128, 0)),
            "{value}"
        );
        report.workbook.validate().unwrap();
    }
}

#[test]
fn malformed_and_unsupported_css_colors_are_reported_without_inventing_colors() {
    for value in [
        "rgb(1,2)",
        "rgb(1,2,3,4,5)",
        "rgb(1,2,3) trailing",
        "rgb(1,2,3",
        "rgb(1,2,3)/*",
        "rgb(100%,0,0)",
        "hsl(1e999,100%,50%)",
        "currentColor",
        "var(--unknown)",
        "color(display-p3 1 0 0)",
        "light-dark(rgb(1,2,3),rgb(4,5,6)",
        "light-dark(#fff,)",
        "light-dark(#fff,#000,#123)",
        "light-dark(#fff,var(--unknown))",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" style="shape=rectangle;fillColor={value};" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{value}");
        assert_eq!(
            shapes(&report.workbook.sheets[0])[0].style.fill,
            None,
            "{value}"
        );
        report.workbook.validate().unwrap();
    }

    for (value, expected) in [("fff", Color::WHITE), ("##123", Color::rgb(17, 34, 51))] {
        let style = Style::parse(&format!("fillColor={value};"));
        assert_eq!(style.color("fillColor", "fillOpacity"), Some(expected));
        assert!(style.color_appearance_loss("fillColor", "fillOpacity"));
    }
}

#[test]
fn css_colors_preserve_the_light_alternative_and_report_the_dark_one() {
    let xml = page(
        r#"<mxCell id="a" style="shape=rectangle;fillColor=light-dark(rgb(10,20,30),rgba(40,50,60,0.5));strokeColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    assert_eq!(
        shapes(&report.workbook.sheets[0])[0].style.fill,
        Some(Color::rgb(10, 20, 30))
    );
    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert_eq!(linked.warnings, report.warnings);
    assert_eq!(linked.workbook, report.workbook);
    assert_eq!(
        crate::drawio_clipboard::decode(&xml).unwrap().warnings,
        report.warnings
    );

    for (value, expected) in [
        (
            "light-dark(light-dark(red,blue),currentColor)",
            Color::rgb(255, 0, 0),
        ),
        (
            "LIGHT-DARK(rgb(10,20,30),color(display-p3 1 0 0))",
            Color::rgb(10, 20, 30),
        ),
    ] {
        let style = Style::parse(&format!("fillColor={value};"));
        assert_eq!(style.color("fillColor", "fillOpacity"), Some(expected));
        assert!(style.color_appearance_loss("fillColor", "fillOpacity"));
    }
    let deeply_nested = format!("{}red{}", "light-dark(".repeat(40), ",blue)".repeat(40));
    let style = Style::parse(&format!("fillColor={deeply_nested};"));
    assert_eq!(style.color("fillColor", "fillOpacity"), None);
    assert!(style.color_appearance_loss("fillColor", "fillOpacity"));
    let bounded = format!(
        "{}rgb(1,2,3){}",
        "light-dark(".repeat(MAX_COLOR_NESTING - 1),
        ",blue)".repeat(MAX_COLOR_NESTING - 1)
    );
    let style = Style::parse(&format!("fillColor={bounded};"));
    assert_eq!(
        style.color("fillColor", "fillOpacity"),
        Some(Color::rgb(1, 2, 3))
    );
    assert!(style.color_appearance_loss("fillColor", "fillOpacity"));
}

#[test]
fn css_colors_cover_cards_connectors_ink_and_swimlane_bodies() {
    let xml = page(
        r#"<mxCell id="card" value="Card" style="shape=note;fillColor=rgba(10,20,30,0.5);fontColor=blue;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="80" as="geometry"/></mxCell>
        <mxCell id="edge" style="strokeColor=rgb(255,0,0);endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="100" as="sourcePoint"/><mxPoint x="100" y="100" as="targetPoint"/></mxGeometry></mxCell>
        <mxCell id="ink" style="thinodaInk=pen;strokeColor=hsl(120,100%,50%);endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="120" as="sourcePoint"/><mxPoint x="100" y="120" as="targetPoint"/></mxGeometry></mxCell>
        <mxCell id="lane" value="Lane" style="swimlane;swimlaneFillColor=rgba(40,50,60,0.5);" vertex="1" parent="1"><mxGeometry x="150" width="300" height="200" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(
        report.warnings,
        vec![
            ImportWarning::Formatting,
            ImportWarning::DrawioFoldedNotesApproximated,
        ]
    );
    let sheet = &report.workbook.sheets[0];
    let card = sheet
        .topics
        .values()
        .find(|topic| topic.title == "Card")
        .unwrap();
    assert_eq!(card.card.unwrap().color, Some(Color::rgba(10, 20, 30, 128)));
    assert_eq!(card.style.text_color, Some(Color::rgb(0, 0, 255)));
    assert_eq!(connectors(sheet)[0].color, Some(Color::rgb(255, 0, 0)));
    let ink = sheet
        .canvas
        .objects
        .iter()
        .find_map(|object| match &object.kind {
            CanvasObjectKind::Ink(ink) => Some(ink),
            _ => None,
        })
        .unwrap();
    assert_eq!(ink.color, Color::rgb(0, 255, 0));
    assert_eq!(
        sheet.zones[0].background_color,
        Some(Color::rgba(40, 50, 60, 128))
    );
    let native =
        export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        crate::import_bytes(Format::Thinoda, &native).unwrap(),
        report.workbook
    );
}

#[test]
fn transparent_card_and_swimlane_colors_are_not_silently_defaulted() {
    for kind in ["shape=note", "thinodaCard=text"] {
        for value in ["none", "transparent", "rgba(10,20,30,0)"] {
            let xml = page(&format!(
                r#"<mxCell id="card" value="Card" style="{kind};fillColor={value};" vertex="1" parent="1"><mxGeometry width="100" height="80" as="geometry"/></mxCell>"#
            ));
            let report = load(&xml);
            let warnings = if kind == "shape=note" {
                vec![
                    ImportWarning::Formatting,
                    ImportWarning::DrawioFoldedNotesApproximated,
                ]
            } else {
                vec![ImportWarning::Formatting]
            };
            assert_eq!(report.warnings, warnings, "{kind}, {value}");
            let card = report.workbook.sheets[0]
                .topics
                .values()
                .find(|topic| topic.title == "Card")
                .unwrap();
            assert_eq!(card.card.unwrap().color.unwrap().a, 0, "{kind}, {value}");
            assert_eq!(card.style.fill, card.card.unwrap().color);
            report.workbook.validate().unwrap();
        }
    }

    for style in [
        "swimlane;strokeColor=transparent;",
        "swimlane;fillColor=red;",
        "swimlane;fontColor=blue;",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="lane" value="Lane" style="{style}" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
    }

    for style in [
        "shape=note;strokeColor=red;",
        "thinodaCard=text;strokeColor=none;",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="card" value="Card" style="{style}" vertex="1" parent="1"><mxGeometry width="100" height="80" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        let warnings = if style.starts_with("shape=note;") {
            vec![
                ImportWarning::Formatting,
                ImportWarning::DrawioFoldedNotesApproximated,
            ]
        } else {
            vec![ImportWarning::Formatting]
        };
        assert_eq!(report.warnings, warnings, "{style}");
    }
}

#[test]
fn transparent_css_colors_keep_hidden_rgb_through_drawio_export() {
    let xml = page(
        r#"<mxCell id="shape" value="Shape" style="shape=rectangle;fillColor=rgba(10,20,30,0);strokeColor=rgba(40,50,60,0);" vertex="1" parent="1"><mxGeometry width="100" height="80" as="geometry"/></mxCell>
        <mxCell id="card" value="Card" style="shape=note;fillColor=rgba(70,80,90,0);" vertex="1" parent="1"><mxGeometry x="120" width="100" height="80" as="geometry"/></mxCell>"#,
    );
    let original = load(&xml);
    assert_eq!(
        original.warnings,
        vec![
            ImportWarning::Formatting,
            ImportWarning::DrawioFoldedNotesApproximated,
        ]
    );
    let exported = String::from_utf8(export(&original.workbook).unwrap()).unwrap();
    assert!(exported.contains("fillColor=#0A141E;fillOpacity=0;"));
    assert!(exported.contains("strokeColor=#28323C;strokeOpacity=0;"));
    assert!(exported.contains("fillColor=#46505A;fillOpacity=0;"));
    let restored = load(&exported);
    assert_eq!(
        shapes(&restored.workbook.sheets[0])[0].style.fill,
        Some(Color::rgba(10, 20, 30, 0))
    );
    assert_eq!(
        shapes(&restored.workbook.sheets[0])[0].style.stroke,
        Some(Color::rgba(40, 50, 60, 0))
    );
    let card = restored.workbook.sheets[0]
        .topics
        .values()
        .find(|topic| topic.title == "Card")
        .unwrap();
    assert_eq!(card.card.unwrap().color, Some(Color::rgba(70, 80, 90, 0)));
}

#[test]
fn css_colors_are_recognized_by_mockup_and_partial_rectangles() {
    for (shape, suffix) in [
        ("mxgraph.mockup.rrect", "rSize=0;"),
        ("partialRectangle", ""),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="Color" style="shape={shape};{suffix}fillColor=red;strokeColor=blue;fontColor=green;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ))
        .replacen("<mxGraphModel>", r#"<mxGraphModel adaptiveColors="none">"#, 1);
        let report = load(&xml);
        assert!(report.warnings.is_empty(), "{shape}: {:?}", report.warnings);
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(object.style.fill, Some(Color::rgb(255, 0, 0)));
        assert_eq!(object.style.stroke, Some(Color::rgb(0, 0, 255)));
        assert_eq!(object.style.text_color, Some(Color::rgb(0, 128, 0)));
    }
}

#[test]
fn css_colors_are_used_by_visual_export_and_native_roundtrip() {
    let xml = page(
        r#"<mxCell id="a" value="Color" style="shape=rectangle;fillColor=rgba(10,20,30,0.5);fillOpacity=50;strokeColor=rgb(100%,0%,0%);fontColor=blue;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let options = ExportOptions {
        transparent: true,
        ..Default::default()
    };
    let svg =
        String::from_utf8(export_bytes(&report.workbook, Format::Svg, &options).unwrap()).unwrap();
    assert!(svg.contains(r##"fill="#0A141E40""##), "{svg}");
    assert!(svg.contains(r##"stroke="#FF0000""##), "{svg}");
    assert!(svg.contains(r##"fill="#0000FF">Color</text>"##), "{svg}");
    let native =
        export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        crate::import_bytes(Format::Thinoda, &native).unwrap(),
        report.workbook
    );
    let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
    assert!(pasted.warnings.is_empty(), "{:?}", pasted.warnings);
}

#[test]
fn drawio_independent_label_opacity_and_default_connector_color_are_reported() {
    for (element, style, value, warning) in [
        ("vertex", "shape=rectangle;opacity=40;", "Shape", true),
        (
            "vertex",
            "shape=rectangle;opacity=40;fontColor=#ff0000;textOpacity=60;",
            "Shape",
            true,
        ),
        (
            "vertex",
            "shape=rectangle;opacity=40;fontColor=#ff0000;textOpacity=0;",
            "Shape",
            false,
        ),
        ("vertex", "shape=rectangle;opacity=40;", "", false),
        ("vertex", "shape=note;opacity=40;", "", true),
        ("vertex", "shape=line;strokeColor=#ff0000;", "Line", true),
        ("vertex", "shape=line;strokeColor=#000000;", "Line", true),
        (
            "vertex",
            "shape=line;strokeColor=#ff0000;fontColor=#000000;",
            "Line",
            true,
        ),
        (
            "vertex",
            "shape=line;strokeColor=#ff0000;fontColor=#000000;textOpacity=0;",
            "Line",
            false,
        ),
        ("edge", "strokeColor=#ff0000;endArrow=none;", "Edge", true),
        ("edge", "strokeColor=#000000;endArrow=none;", "Edge", true),
        (
            "edge",
            "strokeColor=#ff0000;fontColor=default;endArrow=none;",
            "Edge",
            true,
        ),
        (
            "edge",
            "strokeColor=#000000;strokeOpacity=40;endArrow=none;",
            "Edge",
            true,
        ),
        (
            "edge",
            "strokeColor=#ff0000;fontColor=#000000;endArrow=none;",
            "Edge",
            false,
        ),
        (
            "edge",
            "strokeColor=#ff0000;fontColor=#000000;textOpacity=50;endArrow=none;",
            "Edge",
            true,
        ),
        (
            "edge",
            "strokeColor=#ff0000;fontColor=#000000;textOpacity=0;endArrow=none;",
            "Edge",
            false,
        ),
        (
            "edge",
            "strokeColor=#ff0000;fontColor=#000000;labelBackgroundColor=none;endArrow=none;",
            "Edge",
            true,
        ),
        (
            "edge",
            "strokeColor=#ff0000;fontColor=#000000;textOpacity=0;labelBackgroundColor=none;endArrow=none;",
            "Edge",
            false,
        ),
        ("edge", "endArrow=none;", "Edge", true),
        ("edge", "opacity=40;endArrow=none;", "Edge", true),
    ] {
        let edge_font = if element == "edge" {
            "fontSize=12;"
        } else {
            ""
        };
        let xml = page(&format!(
            r#"<mxCell id="a" value="{value}" style="{style}{edge_font}" {element}="1" parent="1"><mxGeometry x="0" y="0" width="100" height="60" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="100" y="0" as="targetPoint"/></mxGeometry></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            warning,
            "{element} {style} {value:?}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn zero_opacity_hides_text_even_when_font_color_is_unresolved() {
    for style in [
        "shape=rectangle;textOpacity=0;",
        "shape=rectangle;fontColor=default;textOpacity=0;",
        "shape=rectangle;fontColor=inherit;textOpacity=0;",
        "shape=rectangle;textOpacity=-20;",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="Hidden" style="{style}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}"
        );
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(object.style.text_color, Some(Color::TRANSPARENT), "{style}");
    }
}

#[test]
fn imported_text_opacity_is_used_by_visual_exports() {
    let options = ExportOptions {
        transparent: true,
        ..Default::default()
    };
    for (style, svg_color, visible) in [
        ("fontColor=none;textOpacity=40;", "#00000066", true),
        ("textOpacity=0;", "#00000000", false),
        ("fontColor=#0c2238;textOpacity=0;", "#0C223800", false),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" value="Label" style="shape=rectangle;fillColor=none;strokeColor=none;{style}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let book = load(&xml).workbook;
        let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
        assert!(svg.contains(&format!(r#"fill="{svg_color}">Label</text>"#)));
        let png = export_bytes(&book, Format::Png, &options).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        let max_alpha = pixels.pixels().map(|pixel| pixel.0[3]).max().unwrap();
        if visible {
            assert!(max_alpha > 0 && max_alpha <= 102, "{style}: {max_alpha}");
        } else {
            assert_eq!(max_alpha, 0, "{style}");
        }
    }
}

#[test]
fn connector_visual_exports_separate_text_color_from_the_line_and_backdrop() {
    let options = ExportOptions {
        transparent: true,
        ..Default::default()
    };
    let mut line_pixel = None;
    for (explicit, label_color, backdrop_alpha) in [
        (None, "#C81E2840", 255),
        (Some(Color::rgba(12, 34, 56, 128)), "#0C2238", 64),
        (Some(Color::rgba(12, 34, 56, 0)), "#0C2238", 0),
    ] {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::new(0.0, 0.0)),
            Endpoint::Point(Vec2::new(120.0, 0.0)),
        );
        connector.label = "Edge".into();
        connector.color = Some(Color::rgba(200, 30, 40, 128));
        connector.arrow_end = false;
        let mut object = CanvasObject::connector(connector);
        object.style.text_color = explicit;
        object.style.opacity = 0.5;
        sheet.add_canvas_object(object).unwrap();
        let book = Workbook::new(sheet);
        let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
        assert!(svg.contains(r##"stroke="#C81E2840""##), "{svg}");
        let label = svg
            .find(&format!("fill=\"{label_color}\">Edge</text>"))
            .unwrap();
        let backdrop = svg[..label].rsplit("<rect ").next().unwrap();
        let attribute = |source: &str, name: &str| {
            source
                .split(&format!("{name}=\""))
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned()
        };
        let fill = attribute(backdrop, "fill");
        assert_eq!(fill.len(), 7, "{backdrop}");
        if backdrop_alpha != 255 {
            assert!(
                svg.contains(&format!(
                    "<g opacity=\"{:.6}\">",
                    f32::from(backdrop_alpha) / 255.0
                )),
                "{svg}"
            );
        }

        let view_box = attribute(&svg, "viewBox");
        let values: Vec<f32> = view_box
            .split_whitespace()
            .map(|part| part.parse().unwrap())
            .collect();
        let x: f32 = attribute(backdrop, "x").parse().unwrap();
        let y: f32 = attribute(backdrop, "y").parse().unwrap();
        let width: f32 = attribute(backdrop, "width").parse().unwrap();
        let height: f32 = attribute(backdrop, "height").parse().unwrap();
        let pixels = image::load_from_memory(&export_bytes(&book, Format::Png, &options).unwrap())
            .unwrap()
            .to_rgba8();
        let sample = |x: f32, y: f32| {
            pixels.get_pixel(
                (x - values[0]).floor() as u32,
                (y - values[1]).floor() as u32,
            )
        };
        let alpha = sample(x + 2.5, y + height - 4.0).0[3];
        assert!(
            (i16::from(alpha) - backdrop_alpha).abs() <= 2,
            "{explicit:?}: {alpha}"
        );
        let line = *sample(10.0, -0.5);
        assert!(line.0[3] > 0);
        if let Some(previous) = line_pixel {
            assert_eq!(line, previous, "{explicit:?}");
        }
        line_pixel = Some(line);
        if backdrop_alpha == 64 {
            let mut saw_text = false;
            for iy in (y as i32 + 2)..-2 {
                for ix in (x as i32 + 3)..((x + width) as i32 - 3) {
                    let pixel = sample(ix as f32 + 0.5, iy as f32 + 0.5).0;
                    assert!(pixel[3] <= 65, "overlapping label alpha: {pixel:?}");
                    saw_text |= pixel[3] > 0 && pixel[0] < 180;
                }
            }
            assert!(saw_text, "the checked region must contain label glyphs");
        }
    }
}

#[test]
fn drawio_exports_warn_when_object_opacity_changes_label_appearance() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    for (label, opacity, color) in [
        ("Shape", 0.4, Some(Color::rgba(12, 34, 56, 128))),
        ("Automatic", 0.6, None),
        ("Hidden", 0.4, Some(Color::TRANSPARENT)),
        ("Opaque", 1.0, Some(Color::BLACK)),
        ("", 0.4, Some(Color::BLACK)),
    ] {
        let mut object = shape(ShapeKind::Rectangle, rect(0.0, 0.0, 100.0, 60.0), label);
        object.style.opacity = opacity;
        object.style.text_color = color;
        sheet.add_canvas_object(object).unwrap();
    }
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.label = "Edge".into();
    connector.arrow_end = false;
    let mut object = CanvasObject::connector(connector);
    object.style.opacity = 0.5;
    sheet.add_canvas_object(object).unwrap();
    let mut colored_connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    colored_connector.label = "Colored edge".into();
    colored_connector.color = Some(Color::rgb(200, 30, 40));
    colored_connector.arrow_end = false;
    sheet
        .add_canvas_object(CanvasObject::connector(colored_connector))
        .unwrap();
    let mut automatic_connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    automatic_connector.label = "Automatic edge".into();
    automatic_connector.arrow_end = false;
    sheet
        .add_canvas_object(CanvasObject::connector(automatic_connector))
        .unwrap();
    let mut black_connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    black_connector.label = "Black edge".into();
    black_connector.color = Some(Color::BLACK);
    black_connector.arrow_end = false;
    sheet
        .add_canvas_object(CanvasObject::connector(black_connector))
        .unwrap();
    let mut translucent_connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    translucent_connector.label = "Translucent edge".into();
    translucent_connector.arrow_end = false;
    let mut translucent_object = CanvasObject::connector(translucent_connector);
    translucent_object.style.text_color = Some(Color::rgba(0, 0, 255, 128));
    sheet.add_canvas_object(translucent_object).unwrap();
    for (label, color) in [
        ("Opaque edge", Color::rgb(0, 0, 255)),
        ("Invisible edge", Color::TRANSPARENT),
    ] {
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.label = label.into();
        connector.arrow_end = false;
        let mut object = CanvasObject::connector(connector);
        object.style.text_color = Some(color);
        sheet.add_canvas_object(object).unwrap();
    }
    let book = Workbook::new(sheet);
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![ExportLoss::DrawioLabelAppearanceApproximated(7)],
            "{format:?}"
        );
    }
    assert!(export_losses(&book, Format::Thinoda, &ExportOptions::default()).is_empty());
}

#[test]
fn drawio_connector_export_uses_the_effective_stroke_color() {
    let fallback = Color::rgba(40, 80, 120, 128);
    let override_color = Color::rgb(200, 30, 40);
    for (connector_color, style_stroke, expected) in [
        (None, None, None),
        (None, Some(fallback), Some(fallback)),
        (Some(override_color), Some(fallback), Some(override_color)),
        (None, Some(Color::TRANSPARENT), Some(Color::TRANSPARENT)),
    ] {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.color = connector_color;
        connector.arrow_end = false;
        let mut object = CanvasObject::connector(connector);
        object.style.stroke = style_stroke;
        sheet.add_canvas_object(object).unwrap();
        let restored = roundtrip(&Workbook::new(sheet));
        assert_eq!(
            connectors(&restored.sheets[0])[0].color,
            expected,
            "connector={connector_color:?}, style={style_stroke:?}"
        );
    }
}

#[test]
fn transparent_text_is_exported_with_zero_opacity_and_round_trips() {
    use std::io::Read;

    for transparent in [Color::TRANSPARENT, Color::rgba(12, 34, 56, 0)] {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut object = shape(ShapeKind::Rectangle, rect(0.0, 0.0, 100.0, 60.0), "Shape");
        object.style.text_color = Some(transparent);
        let object_id = sheet.add_canvas_object(object).unwrap();
        let card = sheet
            .add_card(
                "Card",
                Vec2::new(240.0, 30.0),
                Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0)),
            )
            .unwrap();
        sheet.topic_mut(card).unwrap().style.text_color = Some(transparent);
        let mut connector =
            CanvasConnector::new(Endpoint::Object(object_id), Endpoint::Topic(card));
        connector.label = "Edge".into();
        let mut edge = CanvasObject::connector(connector);
        edge.style.text_color = Some(transparent);
        sheet.add_canvas_object(edge).unwrap();
        let original = Workbook::new(sheet);

        let xml = String::from_utf8(export(&original).unwrap()).unwrap();
        assert_eq!(xml.matches("textOpacity=0;").count(), 3, "{xml}");
        assert!(!xml.contains("fontColor=none;"), "{xml}");
        let report = load(&xml);
        let sheet = &report.workbook.sheets[0];
        for label in ["Shape", "Edge"] {
            let object = sheet
                .canvas
                .objects
                .iter()
                .find(|object| object.label() == label)
                .unwrap();
            assert_eq!(object.style.text_color, Some(transparent), "{label}");
        }
        let card = sheet
            .topics
            .values()
            .find(|topic| topic.title == "Card")
            .unwrap();
        assert_eq!(card.style.text_color, Some(transparent));

        let mut presentation = report.workbook.clone();
        presentation
            .active_mut()
            .add_presentation_slide(
                "Text alpha",
                PresentationSource::Branch(card.id),
                rect(-50.0, -100.0, 450.0, 260.0),
            )
            .unwrap();
        let powerpoint =
            export_bytes(&presentation, Format::PowerPoint, &ExportOptions::default()).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(powerpoint)).unwrap();
        let mut slide = String::new();
        archive
            .by_name("ppt/slides/slide1.xml")
            .unwrap()
            .read_to_string(&mut slide)
            .unwrap();
        for label in ["Shape", "Card", "Edge"] {
            let text = slide
                .split("<p:sp>")
                .find(|fragment| fragment.contains(&format!("<a:t>{label}</a:t>")))
                .unwrap_or_else(|| panic!("missing {label} in PowerPoint slide"));
            assert!(text.contains(r#"<a:alpha val="0"/>"#), "{label}: {text}");
        }
        let edge_background = slide
            .split("<p:sp>")
            .find(|fragment| fragment.contains("Connector label background"))
            .unwrap();
        assert!(edge_background.contains(r#"<a:alpha val="0"/>"#));
    }
}

const INVISIBLE_ANCHORS: &[&str] = &[
    "mxgraph.mockup.anchor",
    "mxgraph.mockup.forms.anchor",
    "mxgraph.mockup.containers.anchor",
    "mxgraph.mockup.misc.anchor",
    "mxgraph.mockup.graphics.anchor",
    "mxgraph.mockup.navigation.anchor",
    "mxgraph.ios.anchor",
    "mxgraph.ios7ui.anchor",
    "mxgraph.android.anchor",
    "mxgraph.bootstrap.anchor",
    "mxgraph.eip.anchor",
    "mxgraph.er.anchor",
];

#[test]
fn invisible_anchors_preserve_labels_geometry_and_connector_targets() {
    for alias in INVISIBLE_ANCHORS {
        let xml = page(&format!(
            r##"<mxCell id="a" value="Anchor label" style="shape={alias};fillColor=#ff0000;strokeColor=#00ff00;fontColor=#0000ff;rotation=30;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="b" style="shape={alias};fillColor=#ffffff;strokeColor=#000000;" vertex="1" parent="1"><mxGeometry x="200" y="20" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="e" style="endArrow=none;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{alias}"
        );
        assert!(!report.warnings.contains(&ImportWarning::Other), "{alias}");
        let sheet = &report.workbook.sheets[0];
        let objects = shapes(sheet);
        assert_eq!(objects.len(), 2, "{alias}");
        assert_eq!(objects[0].label(), "Anchor label");
        assert_eq!(objects[0].rect, rect(10.0, 20.0, 100.0, 60.0));
        assert_eq!(objects[0].rotation, 30.0);
        assert_eq!(objects[0].style.text_color, Some(Color::rgb(0, 0, 255)));
        for object in &objects {
            assert!(matches!(
                object.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::Rectangle,
                    ..
                }
            ));
            assert_eq!(object.style.fill, Some(Color::TRANSPARENT), "{alias}");
            assert_eq!(object.style.stroke, Some(Color::TRANSPARENT), "{alias}");
        }
        let edge = connectors(sheet)[0];
        assert_eq!(edge.from, Endpoint::Object(objects[0].id));
        assert_eq!(edge.to, Endpoint::Object(objects[1].id));

        report.workbook.validate().unwrap();
        let native =
            export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(
            crate::import_bytes(Format::Thinoda, &native).unwrap(),
            report.workbook
        );
        let exported = export(&report.workbook).unwrap();
        let reopened = import(&exported).unwrap();
        let reopened_objects = shapes(&reopened.workbook.sheets[0]);
        assert_eq!(reopened_objects.len(), 2, "{alias}");
        assert_eq!(reopened_objects[0].label(), "Anchor label");
        assert_eq!(reopened_objects[0].rect, objects[0].rect);
        assert_eq!(reopened_objects[0].rotation, 30.0);
        for object in &reopened_objects {
            assert_eq!(object.style.fill, Some(Color::TRANSPARENT), "{alias}");
            assert_eq!(object.style.stroke, Some(Color::TRANSPARENT), "{alias}");
        }
        let reopened_edge = connectors(&reopened.workbook.sheets[0])[0];
        assert_eq!(reopened_edge.from, Endpoint::Object(reopened_objects[0].id));
        assert_eq!(reopened_edge.to, Endpoint::Object(reopened_objects[1].id));
    }
}

#[test]
fn unrecognized_anchor_names_remain_reported_placeholders() {
    let xml = page(
        r#"<mxCell id="a" style="shape=mxgraph.example.anchor;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert!(
        report
            .warnings
            .contains(&ImportWarning::UnsupportedDrawioShapes)
    );
    let object = shapes(&report.workbook.sheets[0])[0];
    assert_ne!(object.style.stroke, Some(Color::TRANSPARENT));
}

#[test]
fn unsupported_drawio_shapes_report_rectangle_placeholders_on_all_import_paths() {
    let xml = page(
        r##"<mxCell id="chair" value="Chair" style="shape=mxgraph.floorplan.office_chair;fillColor=#ffffff;strokeColor=#000000;rotation=35;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="40" height="43" as="geometry"/></mxCell>
        <mxCell id="custom" value="Custom" style="unregisteredStyle;" vertex="1" parent="1"><mxGeometry x="100" y="20" width="60" height="40" as="geometry"/></mxCell>
        <mxCell id="known" value="Known" style="shape=rectangle;" vertex="1" parent="1"><mxGeometry x="200" y="20" width="60" height="40" as="geometry"/></mxCell>"##,
    );
    let report = load(&xml);
    let expected = vec![
        ImportWarning::Formatting,
        ImportWarning::UnsupportedDrawioShapes,
        ImportWarning::Other,
    ];
    assert_eq!(report.warnings, expected);
    let objects = shapes(&report.workbook.sheets[0]);
    assert_eq!(objects.len(), 3);
    assert_eq!(objects[0].label(), "Chair");
    assert_eq!(objects[0].rect, rect(10.0, 20.0, 40.0, 43.0));
    assert_eq!(objects[0].rotation, 35.0);
    assert_eq!(objects[0].style.fill, Some(Color::WHITE));
    assert_eq!(objects[0].style.stroke, Some(Color::BLACK));
    assert!(objects.iter().all(|object| matches!(
        object.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            ..
        }
    )));
    report.workbook.validate().unwrap();

    assert!(matches!(
        crate::import_bytes(Format::DrawIo, xml.as_bytes()),
        Err(IoError::ImportNeedsConfirmation(warnings)) if warnings == expected
    ));
    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert_eq!(linked.warnings, expected);
    assert_eq!(linked.workbook, report.workbook);
    let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
    assert_eq!(pasted.warnings, expected);

    let native =
        export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        crate::import_bytes(Format::Thinoda, &native).unwrap(),
        report.workbook
    );
    let exported = export(&report.workbook).unwrap();
    let reopened = import(&exported).unwrap();
    assert!(
        !reopened
            .warnings
            .contains(&ImportWarning::UnsupportedDrawioShapes),
        "the fallback no longer retains the source stencil identity"
    );
}

#[test]
fn recognized_approximations_do_not_claim_to_be_unknown_shapes() {
    let xml = page(
        r#"<mxCell id="folder" value="Folder" style="shape=folder;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert!(
        report.warnings.is_empty(),
        "a mapped stencil is exact: {:?}",
        report.warnings
    );
}

#[test]
fn plain_drawio_shapes_resolve_through_the_stencil_import_map() {
    let cell = |id: &str, style: &str| {
        format!(
            r#"<mxCell id="{id}" value="{id}" style="{style}" vertex="1" parent="1"><mxGeometry x="10" y="10" width="100" height="80" as="geometry"/></mxCell>"#
        )
    };
    let cells = [
        cell("folder", "shape=folder;"),
        cell("left", "shape=folder;tabPosition=left;tabWidth=110;"),
        cell("tape", "shape=tape;"),
        cell("storage", "shape=dataStorage;"),
        cell("fixed", "shape=dataStorage;fixedSize=1;"),
        cell("arrow", "shape=mxgraph.arrows2.arrow;dy=0.6;dx=30;notch=0;"),
    ]
    .concat();
    let sheet = clean(&page(&cells));
    let objects = shapes(&sheet);
    let label_of = |object: &CanvasObject| match &object.kind {
        CanvasObjectKind::Shape { label, .. } => label.clone(),
        _ => String::new(),
    };
    let stencil = |label: &str| {
        objects
            .iter()
            .find(|object| label_of(object) == label)
            .and_then(|object| object.stencil.clone())
    };
    let param = |label: &str, key: &str| stencil(label).unwrap().parameters.get(key).copied();

    let folder = stencil("folder").unwrap();
    assert_eq!(
        (folder.package.as_str(), folder.definition.as_str()),
        ("thinoda.general", "folder")
    );
    // draw.io's class defaults apply to keys absent from the style.
    assert_eq!(param("folder", "tabWidth"), Some(60.0));
    assert_eq!(param("folder", "tabHeight"), Some(20.0));
    assert_eq!(param("folder", "tabPosition"), Some(1.0));
    assert_eq!(param("left", "tabPosition").unwrap_or(0.0), 0.0);
    assert_eq!(param("left", "tabWidth"), Some(110.0));

    let tape = stencil("tape").unwrap();
    assert_eq!(
        (tape.package.as_str(), tape.definition.as_str()),
        ("thinoda.drawio", "tape")
    );
    assert_eq!(param("storage", "size").unwrap_or(0.1), 0.1);
    assert_eq!(param("fixed", "size"), Some(20.0));
    assert_eq!(param("arrow", "dx"), Some(30.0));
    for object in &objects {
        if object.stencil.is_some() {
            assert!(thinoda_core::stencil_object::render(object).is_some());
        }
    }

    let xml = String::from_utf8(export(&Workbook::new(sheet.clone())).unwrap()).unwrap();
    assert!(xml.contains("shape=tape;"));
    assert_eq!(
        shapes(&clean(&xml))
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>(),
        objects
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn drawio_js_shapes_bind_numeric_and_choice_style_keys() {
    let cell = |id: &str, style: &str| {
        format!(
            r#"<mxCell id="{id}" value="{id}" style="{style}" vertex="1" parent="1"><mxGeometry x="10" y="10" width="100" height="80" as="geometry"/></mxCell>"#
        )
    };
    let cells = [
        cell("wall", "shape=mxgraph.floorplan.wall;wallThickness=20;"),
        cell(
            "end",
            "shape=mxgraph.bpmn.event;outline=end;symbol=terminate;",
        ),
        cell("bare", "shape=mxgraph.bpmn.event;"),
        cell(
            "bogus",
            "shape=mxgraph.bpmn.event;outline=standard;symbol=bogus;",
        ),
        cell("gateway", "shape=mxgraph.bpmn.gateway2;gwType=parallel;"),
        cell("task", "shape=mxgraph.bpmn.task2;taskMarker=user;"),
        cell("person", "shape=mxgraph.c4.person2;"),
    ]
    .concat();
    let sheet = clean(&page(&cells));
    let objects = shapes(&sheet);
    let stencil = |label: &str| {
        objects
            .iter()
            .find(|object| object.label() == label)
            .and_then(|object| object.stencil.clone())
            .unwrap_or_else(|| panic!("{label} has no stencil"))
    };
    let param = |label: &str, key: &str| stencil(label).parameters.get(key).copied();
    let defined = |label: &str| stencil(label).definition;

    assert_eq!(defined("wall"), "floorplan-wall");
    assert_eq!(param("wall", "wallThickness"), Some(20.0));
    assert_eq!(param("end", "outline"), Some(8.0));
    assert_eq!(param("end", "symbol"), Some(12.0));
    // draw.io draws no outline when the key is missing; Thinoda's own default is a circle.
    assert_eq!(param("bare", "outline"), Some(0.0));
    assert_eq!(param("bogus", "symbol").unwrap_or(0.0), 0.0);
    assert_eq!(param("gateway", "gwType"), Some(2.0));
    assert_eq!(defined("task"), "bpmn-task");
    assert_eq!(defined("person"), "c4-person2");
    for object in &objects {
        assert!(thinoda_core::stencil_object::render(object).is_some());
    }

    let xml = String::from_utf8(export(&Workbook::new(sheet.clone())).unwrap()).unwrap();
    assert!(xml.contains("outline=end;symbol=terminate;"), "{xml}");
    assert!(xml.contains("gwType=parallel;"));
    assert!(xml.contains("wallThickness=20;"));
    assert_eq!(
        shapes(&clean(&xml))
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>(),
        objects
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn drawio_electrical_buffer_round_trips_and_scales() {
    let cell = |id: &str, width: u32, height: u32, negating: &str| {
        format!(
            r#"<mxCell id="{id}" value="{id}" style="shape=mxgraph.electrical.logic_gates.buffer2;{negating}" vertex="1" parent="1"><mxGeometry x="0" y="0" width="{width}" height="{height}" as="geometry"/></mxCell>"#
        )
    };
    let xml = page(
        &[
            cell("normal", 100, 80, ""),
            cell("inverted", 200, 40, "negating=1;"),
            cell("invalid", 100, 80, "negating=2;"),
        ]
        .concat(),
    );
    let sheet = clean(&xml);
    let objects = shapes(&sheet);
    assert_eq!(objects.len(), 3);
    for (object, expected_negation) in objects.iter().zip([false, true, false]) {
        let reference = object.stencil.as_ref().unwrap();
        assert_eq!(reference.definition, "electrical-buffer-v2");
        let render = thinoda_core::stencil_object::render(object).unwrap();
        assert_eq!(render.ports.len(), 2);
        assert_eq!(render.ports[0].at, Vec2::new(0.0, object.rect.size.y / 2.0));
        assert_eq!(
            render.ports[1].at,
            Vec2::new(object.rect.size.x, object.rect.size.y / 2.0)
        );
        assert!(render.ports.iter().all(|port| port.perimeter));
        let visible: Vec<_> = render
            .paths
            .iter()
            .filter(|path| {
                path.stroke.is_some() || path.fill.as_ref().is_some_and(|fill| fill.alpha > 0.0)
            })
            .collect();
        assert_eq!(visible.len(), if expected_negation { 3 } else { 2 });
        assert_eq!(visible[0].subpaths.len(), 2);
        assert_eq!(
            visible[0].subpaths[0].points[0],
            Vec2::new(0.0, object.rect.size.y * 0.5)
        );
        assert_eq!(
            visible[0].subpaths[0].points[1],
            Vec2::new(object.rect.size.x * 0.2, object.rect.size.y * 0.5)
        );
        assert_eq!(
            visible[1].subpaths[0].points[0],
            Vec2::new(object.rect.size.x * 0.2, 0.0)
        );
        if expected_negation {
            let points = &visible[2].subpaths[0].points;
            let min_x = points
                .iter()
                .map(|point| point.x)
                .fold(f32::INFINITY, f32::min);
            let max_x = points
                .iter()
                .map(|point| point.x)
                .fold(f32::NEG_INFINITY, f32::max);
            let diameter = (object.rect.size.x * 0.04).min(object.rect.size.y * 0.07);
            assert!((min_x - object.rect.size.x * 0.8).abs() < 0.01);
            assert!((max_x - min_x - diameter).abs() < 0.01);
        }
    }
    let book = Workbook::new(sheet.clone());
    let native = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(crate::import_bytes(Format::Thinoda, &native).unwrap(), book);
    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(exported.contains("negating=1;"));
    let reopened = clean(&exported);
    assert_eq!(
        shapes(&reopened)
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>(),
        objects
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>()
    );
    for (replacement, expected_paths) in [("negating=0;", 2), ("negating=2;", 2), ("", 3)] {
        let edited = exported.replacen("negating=1;", replacement, 1);
        let edited_sheet = clean(&edited);
        let inverted = shapes(&edited_sheet)
            .into_iter()
            .find(|object| object.label() == "inverted")
            .unwrap();
        let render = thinoda_core::stencil_object::render(inverted).unwrap();
        assert_eq!(
            render
                .paths
                .iter()
                .filter(|path| path.stroke.is_some())
                .count(),
            expected_paths,
            "{replacement:?}"
        );
    }
}

#[test]
fn drawio_electrical_test_point_preserves_geometry_and_stroke_fill() {
    let cell = |id: &str, width: u32, height: u32| {
        format!(
            r##"<mxCell id="{id}" value="{id}" style="shape=mxgraph.electrical.transmission.testPoint;fillColor=#FF0000;strokeColor=#0000FF;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="{width}" height="{height}" as="geometry"/></mxCell>"##
        )
    };
    let sheet = clean(&page(
        &[
            cell("tall", 20, 40),
            cell("wide", 40, 20),
            cell("square", 20, 20),
        ]
        .concat(),
    ));
    let objects = shapes(&sheet);
    for (object, has_lead) in objects.iter().zip([true, false, false]) {
        assert_eq!(
            object.stencil.as_ref().unwrap().definition,
            "electrical-test-point-v2"
        );
        let render = thinoda_core::stencil_object::render(object).unwrap();
        let slots =
            thinoda_core::stencil_object::slots(object, Color::rgb(0, 0, 0), Color::rgb(0, 0, 0));
        let (paths, _) = thinoda_core::stencil_object::primitives(object, &render, &slots);
        assert_eq!(paths.len(), if has_lead { 2 } else { 1 });
        assert_eq!(paths[0].fill, Some(Color::rgb(0, 0, 255)));
        assert_eq!(
            paths[0].stroke.as_ref().unwrap().color,
            Color::rgb(0, 0, 255)
        );
        let points = &paths[0].subpaths[0].0;
        let left = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let right = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        let diameter = object.rect.size.x.min(object.rect.size.y);
        assert!((left - (object.rect.size.x - diameter) / 2.0).abs() < 0.01);
        assert!((right - left - diameter).abs() < 0.01);
        if has_lead {
            assert_eq!(
                paths[1].subpaths[0].0,
                vec![Vec2::new(10.0, 20.0), Vec2::new(10.0, 40.0)]
            );
        }
        assert_eq!(render.ports[0].at, Vec2::new(object.rect.size.x / 2.0, 0.0));
        assert_eq!(
            render.ports[1].at,
            Vec2::new(object.rect.size.x / 2.0, object.rect.size.y)
        );
        assert!(render.ports.iter().all(|port| port.perimeter));
    }
    let book = Workbook::new(sheet.clone());
    let native = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(crate::import_bytes(Format::Thinoda, &native).unwrap(), book);
    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(exported.contains("shape=mxgraph.electrical.transmission.testPoint;"));
    let reopened = clean(&exported);
    assert_eq!(
        shapes(&reopened)
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>(),
        objects
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn drawio_electrical_dc_source_preserves_polarity_and_ports() {
    let cell = |id: &str, width: u32, height: u32| {
        format!(
            r#"<mxCell id="{id}" value="{id}" style="shape=mxgraph.electrical.signal_sources.dc_source_3_v2;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="{width}" height="{height}" as="geometry"/></mxCell>"#
        )
    };
    let sheet = clean(&page(
        &[cell("large", 200, 100), cell("small", 40, 20)].concat(),
    ));
    let objects = shapes(&sheet);
    for object in &objects {
        assert_eq!(
            object.stencil.as_ref().unwrap().definition,
            "electrical-dc-source-v2"
        );
        let render = thinoda_core::stencil_object::render(object).unwrap();
        let width = object.rect.size.x;
        let height = object.rect.size.y;
        let half_sign = (0.05 * width.min(height)).max(3.0);
        assert_eq!(render.paths.len(), 2);
        assert!(render.paths[0].fill.is_some() && render.paths[0].stroke.is_some());
        let signs = &render.paths[1].subpaths;
        assert_eq!(signs.len(), 3);
        let expected = [
            [
                Vec2::new(width / 2.0 - half_sign, height * 0.05 + 3.0),
                Vec2::new(width / 2.0 + half_sign, height * 0.05 + 3.0),
            ],
            [
                Vec2::new(width / 2.0, height * 0.05 + 3.0 - half_sign),
                Vec2::new(width / 2.0, height * 0.05 + 3.0 + half_sign),
            ],
            [
                Vec2::new(width / 2.0 - half_sign, height * 0.95 - 3.0),
                Vec2::new(width / 2.0 + half_sign, height * 0.95 - 3.0),
            ],
        ];
        for (subpath, expected) in signs.iter().zip(expected) {
            assert_eq!(subpath.points, expected);
        }
        assert!((render.ports[0].at - Vec2::new(width / 2.0, 0.0)).length() < 0.001);
        assert!((render.ports[1].at - Vec2::new(width / 2.0, height)).length() < 0.001);
        assert!(render.ports.iter().all(|port| port.perimeter));
    }
    let book = Workbook::new(sheet.clone());
    let native = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(crate::import_bytes(Format::Thinoda, &native).unwrap(), book);
    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(exported.contains("shape=mxgraph.electrical.signal_sources.dc_source_3_v2;"));
    let reopened = clean(&exported);
    assert_eq!(
        shapes(&reopened)
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>(),
        objects
            .iter()
            .map(|object| object.stencil.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn drawio_electrical_stroke_width_inherits_without_changing_legacy_definitions() {
    let aliases = [
        (
            "mxgraph.electrical.logic_gates.buffer2",
            "electrical-buffer",
        ),
        (
            "mxgraph.electrical.transmission.testPoint",
            "electrical-test-point",
        ),
        (
            "mxgraph.electrical.signal_sources.dc_source_3_v2",
            "electrical-dc-source",
        ),
    ];
    for (alias, legacy) in aliases {
        let default_cell = format!(
            r#"<mxCell id="shape" style="shape={alias};" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="80" as="geometry"/></mxCell>"#
        );
        let default_sheet = clean(&page(&default_cell));
        let default_object = shapes(&default_sheet)[0];
        let default_render = thinoda_core::stencil_object::render(default_object).unwrap();
        assert!(
            default_render
                .paths
                .iter()
                .filter_map(|path| path.stroke.as_ref())
                .all(|stroke| stroke.width == 1.0),
            "{alias}: draw.io default stroke width"
        );
        let mut edited_default = default_object.clone();
        edited_default.style.stroke_width = 4.0;
        let edited_render = thinoda_core::stencil_object::render(&edited_default).unwrap();
        assert!(
            edited_render
                .paths
                .iter()
                .filter_map(|path| path.stroke.as_ref())
                .all(|stroke| stroke.width == 4.0),
            "{alias}: edited stroke width with automatic colours"
        );
        let default_book = Workbook::new(default_sheet);
        let exported_default = String::from_utf8(export(&default_book).unwrap()).unwrap();
        let reopened_default = clean(&exported_default);
        let reopened_render =
            thinoda_core::stencil_object::render(shapes(&reopened_default)[0]).unwrap();
        assert!(
            reopened_render
                .paths
                .iter()
                .filter_map(|path| path.stroke.as_ref())
                .all(|stroke| stroke.width == 1.0),
            "{alias}: default stroke width after draw.io round-trip"
        );
        let native =
            export_bytes(&default_book, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(
            crate::import_bytes(Format::Thinoda, &native).unwrap(),
            default_book
        );
        let cell = |extra: &str| {
            format!(
                r##"<mxCell id="shape" style="shape={alias};fillColor=#ffffff;strokeColor=#000000;strokeWidth=4;{extra}" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="80" as="geometry"/></mxCell>"##
            )
        };
        let sheet = clean(&page(&cell("")));
        let object = shapes(&sheet)[0].clone();
        assert_eq!(
            object.stencil.as_ref().unwrap().definition,
            format!("{legacy}-v2")
        );
        for width in [0.5, 2.0, 4.0] {
            let mut edited = object.clone();
            edited.style.stroke_width = width;
            let render = thinoda_core::stencil_object::render(&edited).unwrap();
            let strokes: Vec<_> = render
                .paths
                .iter()
                .filter_map(|path| path.stroke.as_ref())
                .collect();
            assert!(!strokes.is_empty());
            assert!(
                strokes.iter().all(|stroke| stroke.width == width),
                "{alias}: {width}"
            );
        }

        let old_sheet = clean(&page(&cell(&format!(
            "thinodaStencilPackage=thinoda.drawio;thinodaStencilDefinition={legacy};"
        ))));
        let old = shapes(&old_sheet)[0];
        assert_eq!(old.stencil.as_ref().unwrap().definition, legacy);
        assert!(
            thinoda_core::stencil_object::render(old)
                .unwrap()
                .paths
                .iter()
                .filter_map(|path| path.stroke.as_ref())
                .all(|stroke| stroke.width == 1.0)
        );
        let book = Workbook::new(old_sheet);
        let native = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(crate::import_bytes(Format::Thinoda, &native).unwrap(), book);
        let drawio = String::from_utf8(export(&book).unwrap()).unwrap();
        assert_eq!(
            shapes(&clean(&drawio))[0]
                .stencil
                .as_ref()
                .unwrap()
                .definition,
            legacy
        );
    }
}

#[test]
fn inline_drawio_stencils_import_render_and_round_trip() {
    use base64::Engine as _;
    let encode = |stencil: &str| {
        let deflated = miniz_oxide::deflate::compress_to_vec(
            crate::drawio_clipboard::encode_uri_component(stencil).as_bytes(),
            6,
        );
        base64::engine::general_purpose::STANDARD.encode(deflated)
    };
    let payload = encode(
        r#"<shape name="Gem" aspect="variable" w="40" h="40" strokewidth="inherit"><background><path><move x="20" y="0"/><line x="40" y="20"/><line x="20" y="40"/><line x="0" y="20"/><close/></path></background><foreground><fillstroke/></foreground></shape>"#,
    );
    let other = encode(
        r#"<shape name="Bar" aspect="variable" w="40" h="20" strokewidth="inherit"><background><rect x="0" y="0" w="40" h="20"/></background><foreground><fillstroke/></foreground></shape>"#,
    );
    let xml = page(&format!(
        r#"<mxCell id="2" value="gem" style="shape=stencil({payload});whiteSpace=wrap;html=1;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="80" as="geometry"/></mxCell><mxCell id="3" style="shape=stencil({other});" vertex="1" parent="1"><mxGeometry x="200" y="10" width="80" height="40" as="geometry"/></mxCell><mxCell id="4" style="shape=stencil({payload});" vertex="1" parent="1"><mxGeometry x="300" y="10" width="40" height="40" as="geometry"/></mxCell>"#
    ));
    let report = load(&xml);
    assert!(
        !report
            .warnings
            .contains(&ImportWarning::UnsupportedDrawioShapes),
        "{:?}",
        report.warnings
    );
    let sheet = report.workbook.sheets.into_iter().next().unwrap();
    let object = shapes(&sheet)[0].clone();
    let reference = object.stencil.clone().unwrap();
    assert_eq!(reference.package, "drawio.inline");
    let render = thinoda_core::stencil_object::render(&object).unwrap();
    assert!(render.contains(Vec2::new(40.0, 40.0)));
    assert!(!render.contains(Vec2::new(2.0, 2.0)));
    // Every inline stencil on a page shares one bundled package resource.
    let stencils: Vec<_> = shapes(&sheet)
        .into_iter()
        .map(|shape| shape.stencil.clone().unwrap())
        .collect();
    assert_eq!(stencils.len(), 3);
    assert_ne!(stencils[0].definition, stencils[1].definition);
    assert_eq!(stencils[0].definition, stencils[2].definition);
    assert!(reference.package_resource.is_some());
    assert!(
        stencils
            .iter()
            .all(|stencil| stencil.package_resource == reference.package_resource)
    );
    assert_eq!(sheet.resources.entries.len(), 1);

    let book = Workbook::new(sheet.clone());
    export_bytes(&book, Format::Svg, &ExportOptions::default()).unwrap();
    let native = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(crate::import_bytes(Format::Thinoda, &native).unwrap(), book);

    let exported = String::from_utf8(export(&Workbook::new(sheet)).unwrap()).unwrap();
    assert!(exported.contains("shape=stencil("));
    let again = clean(&exported);
    let reopened = shapes(&again)[0];
    assert_eq!(reopened.stencil.as_ref(), Some(&reference));
    assert!(thinoda_core::stencil_object::render(reopened).is_some());
}

#[test]
fn bundled_drawio_shapes_embed_one_closure_per_package_per_page() {
    let bundle = crate::drawio_bundle::bundle();
    let aliases: Vec<String> = bundle
        .catalog()
        .packages
        .iter()
        .flat_map(|package| package.shapes.iter().map(move |shape| package.alias(shape)))
        .step_by(7)
        .take(170)
        .collect();
    // Shapes a native kind draws exactly stay native; the bundle draws the rest.
    let (aliases, native): (Vec<String>, Vec<String>) = aliases.into_iter().partition(|alias| {
        let sheet = clean(&page(&format!(
            r#"<mxCell id="x" style="shape={alias};" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#
        )));
        shapes(&sheet)[0].stencil.is_some()
    });
    assert!(native.len() < 5, "{native:?}");
    let mut cells = String::new();
    for (i, alias) in aliases.iter().enumerate() {
        cells.push_str(&format!(
            r#"<mxCell id="s{i}" style="shape={alias};html=1;" vertex="1" parent="1"><mxGeometry x="{}" y="{}" width="40" height="40" as="geometry"/></mxCell>"#,
            (i % 20) * 50,
            (i / 20) * 50
        ));
    }
    for i in 0..3 {
        cells.push_str(&format!(
            r#"<mxCell id="p{i}" style="shape=image;image=data:image/png,{};" vertex="1" parent="1"><mxGeometry x="{}" y="500" width="20" height="20" as="geometry"/></mxCell>"#,
            b64(&tiny_png(4 + i, 4)),
            i * 30
        ));
    }
    let report = load(&page(&cells));
    assert!(
        !report
            .warnings
            .contains(&ImportWarning::UnsupportedDrawioShapes),
        "{:?}",
        report.warnings
    );
    let sheet = report.workbook.sheets.into_iter().next().unwrap();
    let objects = shapes(&sheet);
    assert_eq!(objects.len(), aliases.len());
    let stencils: Vec<_> = objects
        .iter()
        .map(|object| object.stencil.clone().expect("bundled stencil"))
        .collect();
    let definitions: BTreeSet<_> = stencils
        .iter()
        .map(|stencil| (stencil.package.clone(), stencil.definition.clone()))
        .collect();
    let packages: BTreeSet<_> = stencils.iter().map(|s| s.package.clone()).collect();
    assert!(definitions.len() > 128, "{}", definitions.len());
    assert!(
        objects
            .iter()
            .all(|object| thinoda_core::stencil_object::render(object).is_some())
    );
    assert!(stencils.iter().all(|s| s.package_resource.is_some()));
    assert_eq!(image_cards(&sheet).len(), 3);
    assert_eq!(sheet.resources.entries.len(), packages.len() + 3);

    let book = Workbook::new(sheet.clone());
    let native = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(crate::import_bytes(Format::Thinoda, &native).unwrap(), book);

    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    for alias in &aliases {
        assert!(exported.contains(&format!("shape={alias};")), "{alias}");
    }
    let again = clean(&exported);
    let reopened: Vec<_> = shapes(&again)
        .into_iter()
        .map(|object| object.stencil.clone().unwrap())
        .collect();
    assert_eq!(reopened, stencils);
}

#[test]
fn ignored_named_styles_with_explicit_shapes_report_formatting_loss() {
    for style in [
        "ellipse;shape=cloud;",
        "customStyle;shape=rectangle;fillColor=#ffffff;strokeColor=#000000;",
        "shape=ellipse;customStyle;",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="shape" style="{style}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
        assert!(matches!(
            crate::import_bytes(Format::DrawIo, xml.as_bytes()),
            Err(IoError::ImportNeedsConfirmation(warnings))
                if warnings == vec![ImportWarning::Formatting]
        ));
        let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
        assert_eq!(
            crate::import_drawio_link(&link).unwrap().warnings,
            report.warnings
        );
        assert_eq!(
            crate::drawio_clipboard::decode(&xml).unwrap().warnings,
            report.warnings
        );
    }

    let ordinary = page(
        r#"<mxCell id="shape" style="shape=rectangle;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    assert!(load(&ordinary).warnings.is_empty());
}

#[test]
fn bare_anchor_names_are_not_treated_as_registered_shapes() {
    for alias in INVISIBLE_ANCHORS {
        let xml = page(&format!(
            r##"<mxCell id="a" style="{alias};fillColor=#ff0000;strokeColor=#00ff00;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert!(report.warnings.contains(&ImportWarning::Other), "{alias}");
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(object.style.fill, Some(Color::rgb(255, 0, 0)), "{alias}");
        assert_eq!(object.style.stroke, Some(Color::rgb(0, 255, 0)), "{alias}");
    }
}

#[test]
fn invisible_anchor_removed_font_color_uses_drawio_default() {
    for alias in INVISIBLE_ANCHORS {
        let xml = page(&format!(
            r##"<mxCell id="a" value="Readable" style="shape={alias};fillColor=#ff0000;strokeColor=#00ff00;fontColor=none;textOpacity=40;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{alias}"
        );
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(object.label(), "Readable");
        assert_eq!(object.style.text_color, Some(Color::rgba(0, 0, 0, 102)));
        let exported = export(&report.workbook).unwrap();
        let reopened = import(&exported).unwrap();
        assert_eq!(
            shapes(&reopened.workbook.sheets[0])[0].style.text_color,
            object.style.text_color,
            "{alias}"
        );
    }
}

#[test]
fn invisible_anchor_children_and_links_keep_their_board_geometry() {
    let xml = page(
        r##"<mxCell id="a" value="Parent anchor" style="shape=mxgraph.mockup.forms.anchor;container=1;image=https://example.invalid/ignored.png;fillColor=#ff0000;strokeColor=#00ff00;" vertex="1" parent="1"><mxGeometry x="100" y="200" width="120" height="80" as="geometry"/></mxCell>
        <mxCell id="b" value="Child" style="shape=rectangle;" vertex="1" parent="a"><mxGeometry x="10" y="20" width="50" height="30" as="geometry"/></mxCell>
        <mxCell id="e" style="endArrow=none;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"##,
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    assert!(!report.warnings.contains(&ImportWarning::Resources));
    let sheet = &report.workbook.sheets[0];
    assert!(sheet.zones.is_empty());
    let objects = shapes(sheet);
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].label(), "Parent anchor");
    assert_eq!(objects[0].style.fill, Some(Color::TRANSPARENT));
    assert_eq!(objects[0].style.stroke, Some(Color::TRANSPARENT));
    assert_eq!(objects[1].rect, rect(110.0, 220.0, 50.0, 30.0));
    assert!(objects[0].group.is_some());
    assert_eq!(objects[0].group, objects[1].group);
    let edge = connectors(sheet)[0];
    assert_eq!(edge.from, Endpoint::Object(objects[0].id));
    assert_eq!(edge.to, Endpoint::Object(objects[1].id));

    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert_eq!(linked.workbook, report.workbook);
    assert_eq!(linked.warnings, report.warnings);
}

#[test]
fn invisible_anchor_exports_paint_the_label_but_not_its_shape() {
    use std::io::Read;

    let make = |label: &str| {
        let xml = page(&format!(
            r##"<mxCell id="a" value="{label}" style="shape=mxgraph.mockup.forms.anchor;fillColor=#ff0000;strokeColor=#00ff00;fontColor=#0000ff;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>"##
        ));
        load(&xml).workbook
    };
    let options = ExportOptions {
        transparent: true,
        ..Default::default()
    };
    let svg =
        String::from_utf8(export_bytes(&make("Anchor label"), Format::Svg, &options).unwrap())
            .unwrap();
    assert!(svg.contains(">Anchor</text>") && svg.contains(">label</text>"));
    assert!(svg.contains("#0000FF"));
    assert!(!svg.contains("#FF0000"));
    assert!(!svg.contains("#00FF00"));

    let png = export_bytes(&make(""), Format::Png, &options).unwrap();
    let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
    assert!(pixels.pixels().all(|pixel| pixel.0[3] == 0));

    let pdf = export_bytes(&make("Anchor label"), Format::Pdf, &options).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));

    let mut book = make("Anchor label");
    let sheet = book.active_mut();
    let object_id = sheet.canvas.objects[0].id;
    let root = sheet.add_floating("Slide", Vec2::new(60.0, 130.0));
    sheet
        .add_presentation_slide(
            "Anchor",
            PresentationSource::Branch(root),
            rect(-40.0, -40.0, 220.0, 220.0),
        )
        .unwrap();
    let powerpoint = export_bytes(&book, Format::PowerPoint, &options).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(powerpoint)).unwrap();
    let mut slide = String::new();
    archive
        .by_name("ppt/slides/slide1.xml")
        .unwrap()
        .read_to_string(&mut slide)
        .unwrap();
    let shape = slide
        .split("<p:sp>")
        .find(|fragment| fragment.contains(&format!("Canvas object {object_id}")))
        .unwrap();
    assert!(shape.matches(r#"<a:alpha val="0"/>"#).count() >= 2);
    assert!(slide.contains(r#"val="0000FF""#));
    assert!(slide.contains("<a:t>Anchor</a:t>"));
    assert!(slide.contains("<a:t>label</a:t>"));
}

const MOCKUP_RECTANGLES: &[&str] = &[
    "mxgraph.mockup.rrect",
    "mxgraph.mockup.forms.rrect",
    "mxgraph.mockup.containers.rrect",
    "mxgraph.mockup.misc.rrect",
    "mxgraph.mockup.graphics.rrect",
    "mxgraph.mockup.text.rrect",
    "mxgraph.ios.rrect",
];

#[test]
fn mockup_rectangle_aliases_preserve_square_geometry_and_attachment() {
    for alias in MOCKUP_RECTANGLES {
        let xml = page(&format!(
            r##"<mxCell id="a" value="Label" style="shape={alias};rSize=0;fillColor=#ff0000;strokeColor=#008000;fontColor=#0000ff;strokeWidth=2;rotation=30;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="b" style="shape={alias};rSize=0;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry x="200" y="20" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="e" style="endArrow=none;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"##
        ));
        let sheet = clean(&xml);
        let objects = shapes(&sheet);
        assert_eq!(objects.len(), 2, "{alias}");
        for object in &objects {
            assert!(matches!(
                object.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::Rectangle,
                    ..
                }
            ));
        }
        assert_eq!(objects[0].label(), "Label");
        assert_eq!(objects[0].rect, rect(10.0, 20.0, 100.0, 60.0));
        assert_eq!(objects[0].rotation, 30.0);
        assert_eq!(objects[0].style.fill, Some(Color::rgb(255, 0, 0)));
        assert_eq!(objects[0].style.stroke, Some(Color::rgb(0, 128, 0)));
        assert_eq!(objects[0].style.text_color, Some(Color::rgb(0, 0, 255)));
        assert_eq!(objects[0].style.stroke_width, 2.0);
        let edge = connectors(&sheet)[0];
        assert_eq!(edge.from, Endpoint::Object(objects[0].id));
        assert_eq!(edge.to, Endpoint::Object(objects[1].id));

        let book = Workbook::new(sheet);
        book.validate().unwrap();
        let native = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
        let restored = crate::import_bytes(Format::Thinoda, &native).unwrap();
        assert_eq!(restored, book);
    }
}

#[test]
fn mockup_rectangle_radius_uses_drawio_style_resolution() {
    use std::cmp::Ordering::{Equal, Greater, Less};

    for (value, expected) in [
        ("0", Some(Equal)),
        ("  -0.5", Some(Equal)),
        ("+0X000tail", Some(Equal)),
        ("0.9", Some(Equal)),
        (".9", Some(Equal)),
        ("-1e-2", Some(Equal)),
        ("1e-6", Some(Equal)),
        ("1e-999", Some(Equal)),
        ("0b10", Some(Equal)),
        ("0e99", Some(Equal)),
        ("0x10", Some(Greater)),
        ("1e-7", Some(Greater)),
        ("1e2", Some(Greater)),
        ("1e999", Some(Greater)),
        ("123ignored", Some(Greater)),
        ("999999999999999999999999999999999999", Some(Greater)),
        ("none", Some(Greater)),
        ("-1e-7", Some(Less)),
        ("-0x10", Some(Less)),
        ("-1", Some(Less)),
        ("", None),
        ("0x", None),
        ("NaN", None),
        ("--1", None),
    ] {
        assert_eq!(mockup_radius_sign(value), expected, "{value:?}");
    }
}

#[test]
fn drawio_standard_rounding_imports_and_exports_arc_modes() {
    for (style, mode, radius, perimeter) in [
        ("rounded=1;", ShapeRoundingMode::Relative, 15.0, false),
        (
            "shape=rect;rounded=1;arcSize=12.5;roundedPerimeter=1;",
            ShapeRoundingMode::Relative,
            12.5,
            true,
        ),
        (
            "shape=rectangle;rounded=1;absoluteArcSize=1;",
            ShapeRoundingMode::AbsoluteArc,
            20.0,
            false,
        ),
        (
            "shape=label;rounded=1.0;absoluteArcSize=1;arcSize=21.5;roundedPerimeter=1;",
            ShapeRoundingMode::AbsoluteArc,
            21.5,
            true,
        ),
        (
            "rounded=1;arcSize=none;roundedPerimeter=0;",
            ShapeRoundingMode::Relative,
            15.0,
            false,
        ),
    ] {
        let xml = page(&format!(
            r##"<mxCell id="a" style="{style}fillColor=#fff;strokeColor=#000;" vertex="1" parent="1"><mxGeometry x="20" y="120" width="100" height="60" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        let expected = ShapeRounding {
            radius,
            stencil: DrawioRectangleStencil::Standard,
            mode,
            rounded_perimeter: perimeter,
        };
        assert_eq!(
            shapes(&report.workbook.sheets[0])[0].rounding,
            Some(expected)
        );
        assert_eq!(feature_version(&report.workbook), 23);
        let exported = export(&report.workbook).unwrap();
        let text = std::str::from_utf8(&exported).unwrap();
        assert!(
            text.contains(&format!(
                "shape=rectangle;rounded=1;arcSize={};absoluteArcSize={};roundedPerimeter={};",
                num(radius),
                u8::from(mode == ShapeRoundingMode::AbsoluteArc),
                u8::from(perimeter)
            )),
            "{text}"
        );
        let reopened = import(&exported).unwrap();
        assert_eq!(
            shapes(&reopened.workbook.sheets[0])[0].rounding,
            Some(expected)
        );
    }
}

#[test]
fn drawio_standard_rounding_warns_on_unrepresentable_values_and_perimeters() {
    for (style, expected_radius) in [
        ("rounded=1;arcSize=-2;", 0.0),
        ("rounded=1;arcSize=not-a-number;", 0.0),
        ("rounded=1;arcSize=1e30;", MAX_BOARD_COORDINATE),
        ("rounded=1;roundedPerimeter=2;", 15.0),
        ("rounded=1;absoluteArcSize=unknown;", 15.0),
    ] {
        let xml = page(&format!(
            r##"<mxCell id="a" style="{style}fillColor=#fff;strokeColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}"
        );
        assert_eq!(
            shapes(&report.workbook.sheets[0])[0]
                .rounding
                .unwrap()
                .radius,
            expected_radius,
            "{style}"
        );
    }
    let xml = page(
        r##"<mxCell id="a" style="rounded=1;roundedPerimeter=1;perimeter=ellipsePerimeter;fillColor=#fff;strokeColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell><mxCell id="edge" source="a" style="endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="200" y="10" as="targetPoint"/></mxGeometry></mxCell>"##,
    );
    assert!(load(&xml).warnings.contains(&ImportWarning::Formatting));
}

#[test]
fn native_rounded_rectangle_hint_yields_to_external_rounding_edits() {
    let mut sheet = Sheet::new("Native", "");
    sheet.root_mode = SheetRootMode::Hidden;
    sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            rect(0.0, 0.0, 200.0, 60.0),
        ))
        .unwrap();
    let exported = String::from_utf8(export(&Workbook::new(sheet)).unwrap()).unwrap();
    assert!(exported.contains("rounded=1;thinodaShape=nativeRoundedRectangle;"));
    let reopened = load(&exported);
    assert_eq!(shapes(&reopened.workbook.sheets[0])[0].rounding, None);

    let edited = exported.replacen(
        "rounded=1;thinodaShape=nativeRoundedRectangle;",
        "rounded=1;arcSize=30;roundedPerimeter=1;thinodaShape=nativeRoundedRectangle;",
        1,
    );
    let reopened = load(&edited);
    assert_eq!(
        shapes(&reopened.workbook.sheets[0])[0].rounding,
        Some(ShapeRounding {
            radius: 30.0,
            stencil: DrawioRectangleStencil::Standard,
            mode: ShapeRoundingMode::Relative,
            rounded_perimeter: true,
        })
    );
}

#[test]
fn drawio_rounded_perimeter_is_used_for_imported_fixed_ports() {
    let xml = page(
        r##"<mxCell id="a" style="rounded=1;arcSize=30;roundedPerimeter=1;fillColor=#fff;strokeColor=#000;" vertex="1" parent="1"><mxGeometry x="20" y="120" width="100" height="60" as="geometry"/></mxCell><mxCell id="edge" source="a" style="endArrow=none;exitX=1;exitY=0;exitPerimeter=1;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="220" y="20" as="targetPoint"/></mxGeometry></mxCell>"##,
    );
    let report = load(&xml);
    let sheet = &report.workbook.sheets[0];
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    let endpoint = thinoda_core::connector_path(sheet, &layout, connectors(sheet)[0])[0];
    assert!((endpoint - Vec2::new(114.7279, 125.2721)).length() < 0.001);
}

#[test]
fn mockup_rectangle_radius_is_bounded_and_preserved_on_drawio_round_trip() {
    for (value, expected) in [
        ("none", 10.0),
        ("0", 0.0),
        ("0.9", 0.0),
        ("-1e-2", 0.0),
        ("0x10", 16.0),
        ("1e2", 100.0),
        ("1e-7", 1.0),
        ("1.7e-7", 1.0),
        ("1e21", 1.0),
        ("1e999", 1.0),
        ("123ignored", 123.0),
        ("0xFFFFFFFFFFFFFFFF", MAX_BOARD_COORDINATE),
        ("9999999", MAX_BOARD_COORDINATE),
        ("-12", 0.0),
        ("NaN", 0.0),
        ("", 0.0),
    ] {
        assert_eq!(mockup_radius(value), expected, "{value:?}");
    }

    for (alias, stencil) in MOCKUP_RECTANGLES.iter().zip([
        DrawioRectangleStencil::Mockup,
        DrawioRectangleStencil::Forms,
        DrawioRectangleStencil::Containers,
        DrawioRectangleStencil::Misc,
        DrawioRectangleStencil::Graphics,
        DrawioRectangleStencil::Text,
        DrawioRectangleStencil::Ios,
    ]) {
        for (source_radius, expected_radius) in [("0", 0.0), ("17", 17.0), ("0x10", 16.0)] {
            let xml = page(&format!(
                r##"<mxCell id="a" value="Label" style="shape={alias};rSize={source_radius};fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>"##
            ));
            let imported = load(&xml);
            let expected = ShapeRounding {
                radius: expected_radius,
                stencil,
                ..Default::default()
            };
            assert_eq!(
                shapes(&imported.workbook.sheets[0])[0].rounding,
                Some(expected)
            );
            let exported = export_bytes(
                &imported.workbook,
                Format::DrawIo,
                &ExportOptions::default(),
            )
            .unwrap();
            let exported_text = std::str::from_utf8(&exported).unwrap();
            assert!(exported_text.contains(&format!("shape={alias};rSize={expected_radius};")));
            let reopened = import(&exported).unwrap();
            assert_eq!(
                shapes(&reopened.workbook.sheets[0])[0].rounding,
                Some(expected)
            );
            assert_eq!(
                shapes(&reopened.workbook.sheets[0])[0].rect,
                rect(10.0, 20.0, 100.0, 60.0)
            );
        }
    }
}

#[test]
fn mockup_rectangle_radius_and_inherited_color_report_approximations() {
    for alias in MOCKUP_RECTANGLES {
        for (radius, shape, warning) in [
            ("", ShapeKind::RoundedRectangle, true),
            ("rSize=none;", ShapeKind::RoundedRectangle, true),
            ("rSize=0;", ShapeKind::Rectangle, false),
            ("rSize=0.9;", ShapeKind::Rectangle, false),
            ("rSize=.9;", ShapeKind::Rectangle, false),
            ("rSize=1e-2;", ShapeKind::Rectangle, false),
            ("rSize=0b10;", ShapeKind::Rectangle, false),
            ("rSize=0x10;", ShapeKind::RoundedRectangle, true),
            ("rSize=1e-7;", ShapeKind::RoundedRectangle, true),
            ("rSize=1e2;", ShapeKind::RoundedRectangle, true),
            ("rSize=-1;", ShapeKind::Rectangle, true),
            ("rSize=NaN;", ShapeKind::Rectangle, true),
        ] {
            let xml = page(&format!(
                r##"<mxCell id="a" style="shape={alias};{radius}fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
            ));
            let report = load(&xml);
            assert_eq!(
                report.warnings,
                if warning {
                    vec![ImportWarning::Formatting]
                } else {
                    vec![]
                },
                "{alias};{radius}"
            );
            assert!(matches!(
                shapes(&report.workbook.sheets[0])[0].kind,
                CanvasObjectKind::Shape { shape: actual, .. } if actual == shape
            ));
            let paste = crate::drawio_clipboard::decode(&xml).unwrap();
            assert_eq!(paste.warnings, report.warnings);
            let mut pasted_sheet = Sheet::new("Paste", "");
            pasted_sheet.root_mode = SheetRootMode::Hidden;
            thinoda_core::clipboard::paste_board_selection(
                &mut pasted_sheet,
                &paste.payload,
                Vec2::ZERO,
            )
            .unwrap();
            assert!(matches!(
                shapes(&pasted_sheet)[0].kind,
                CanvasObjectKind::Shape { shape: actual, .. } if actual == shape
            ));
            let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
            let linked = crate::import_drawio_link(&link).unwrap();
            assert_eq!(linked.warnings, report.warnings);
            assert_eq!(linked.workbook, report.workbook);
            let native =
                export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
            let restored = crate::import_bytes(Format::Thinoda, &native).unwrap();
            assert_eq!(restored, report.workbook);
        }
    }

    for key in ["fillColor", "strokeColor", "fontColor", "gradientColor"] {
        for value in [
            "inherit",
            "swimlane",
            "indicated",
            "fillColor",
            "strokeColor",
            "red",
            "bad",
            "fff",
            "##fff",
            "light-dark(#aabbcc,#ddeeff)",
        ] {
            if value == "red" {
                continue;
            }
            let xml = page(&format!(
                r##"<mxCell id="a" style="shape=mxgraph.mockup.rrect;rSize=0;fillColor=#fff;strokeColor=#000;fontColor=#000;{key}={value};" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
            ));
            let report = load(&xml);
            assert_eq!(
                report.warnings,
                vec![ImportWarning::Formatting],
                "{key}={value}"
            );
            assert!(matches!(
                shapes(&report.workbook.sheets[0])[0].kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::Rectangle,
                    ..
                }
            ));
            let paste = crate::drawio_clipboard::decode(&xml).unwrap();
            assert_eq!(paste.warnings, report.warnings);
            let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
            let linked = crate::import_drawio_link(&link).unwrap();
            assert_eq!(linked.warnings, report.warnings);
            assert_eq!(linked.workbook, report.workbook);
        }
    }
}

#[test]
fn mockup_rectangle_warns_about_unresolved_default_colors_and_opacity() {
    for (prefix, style, warning) in [
        ("", "", true),
        (";", "", true),
        (
            "",
            "fillColor=default;strokeColor=#000;fontColor=#000;",
            true,
        ),
        ("", "fillColor=#fff;strokeColor=#000;fontColor=none;", true),
        ("", "strokeColor=#000;fontColor=#000;fillOpacity=40;", true),
        ("", "fillColor=#fff;fontColor=#000;strokeOpacity=40;", true),
        ("", "fillColor=#fff;strokeColor=#000;textOpacity=40;", true),
        (
            "",
            "fillColor=#fff;strokeColor=#000;fontColor=#000;fillOpacity=invalid;",
            true,
        ),
        ("", "fillColor=#fff;strokeColor=#000;fontColor=#000;", false),
        (";", "fillColor=#fff;strokeColor=#000;fontColor=#000;", true),
        (
            "",
            "fillColor=none;strokeColor=none;fontColor=#000;fillOpacity=40;",
            false,
        ),
        (
            "",
            "fillColor=#ff0000;strokeColor=#000;fontColor=#000;fillOpacity=40;",
            false,
        ),
        (
            "",
            "fillColor=#fff;strokeColor=#008000;fontColor=#000;strokeOpacity=40;",
            false,
        ),
        (
            "",
            "fillColor=#fff;strokeColor=#000;fontColor=#0000ff;textOpacity=40;",
            false,
        ),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" style="{prefix}shape=mxgraph.mockup.rrect;rSize=0;{style}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings,
            if warning {
                vec![ImportWarning::Formatting]
            } else {
                vec![]
            },
            "{prefix}{style}"
        );
        if style.starts_with("fillColor=#ff0000;") {
            assert_eq!(
                shapes(&report.workbook.sheets[0])[0].style.fill,
                Some(Color::rgba(255, 0, 0, 102))
            );
        }
    }
}

#[test]
fn mockup_none_font_color_uses_drawio_black_and_text_opacity() {
    for alias in MOCKUP_RECTANGLES {
        for radius in ["0", "10"] {
            let xml = page(&format!(
                r##"<mxCell id="a" value="Readable" style="shape={alias};rSize={radius};fillColor=#fff;strokeColor=#000;fontColor=none;textOpacity=40;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
            ));
            let report = load(&xml);
            assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
            let object = shapes(&report.workbook.sheets[0])[0];
            assert_eq!(object.label(), "Readable");
            assert_eq!(object.style.text_color, Some(Color::rgba(0, 0, 0, 102)));
            let native =
                export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
            let restored = crate::import_bytes(Format::Thinoda, &native).unwrap();
            assert_eq!(restored, report.workbook);
        }
    }
}

#[test]
fn mockup_rectangle_warns_about_unresolved_named_styles() {
    let xml = page(
        r##"<mxCell id="a" style="shape=mxgraph.mockup.rrect;customStyle;rSize=0;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    assert!(matches!(
        shapes(&report.workbook.sheets[0])[0].kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            ..
        }
    ));
}

#[test]
fn mockup_shape_identifiers_are_not_treated_as_named_styles() {
    for alias in MOCKUP_RECTANGLES {
        let xml = page(&format!(
            r##"<mxCell id="a" style="{alias};rSize=10;fillColor=#fff;strokeColor=#000;fontColor=#000;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings,
            vec![ImportWarning::UnsupportedDrawioShapes, ImportWarning::Other],
            "{alias}"
        );
        assert!(matches!(
            shapes(&report.workbook.sheets[0])[0].kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::Rectangle,
                ..
            }
        ));
    }
}

#[test]
fn supplied_requirements_flow_classifies_real_mockup_rectangles() {
    let dir = std::env::var_os("THINODA_DRAWIO_CORPUS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("draw-io-examples")
        });
    let path = dir.join("requirements-flow.drawio");
    if !path.is_file() {
        eprintln!("optional requirements-flow.drawio is absent; skipping");
        return;
    }
    let bytes = std::fs::read(path).unwrap();
    let pages = read_pages(&bytes).unwrap();
    let mut squares = 0;
    let mut rounded = 0;
    for source_page in &pages.pages {
        let cells = read_cells(&source_page.model, &mut BTreeSet::new());
        for cell in &cells {
            if !cell
                .style
                .get("shape")
                .is_some_and(|shape| MOCKUP_RECTANGLES.contains(&shape))
            {
                continue;
            }
            let expected = match cell.style.get("rSize") {
                Some("0") => {
                    assert!(matches!(
                        classify(&cell.style),
                        Kind::Shape(ShapeKind::Rectangle, ..)
                            | Kind::Approx(ShapeKind::Rectangle, ..)
                    ));
                    squares += 1;
                    ShapeKind::Rectangle
                }
                Some(value) if value.parse::<u32>().is_ok_and(|value| value > 0) => {
                    assert!(matches!(
                        classify(&cell.style),
                        Kind::Approx(ShapeKind::RoundedRectangle, ..)
                    ));
                    rounded += 1;
                    ShapeKind::RoundedRectangle
                }
                _ => continue,
            };
            let mut parts = cell.style.tokens.clone();
            parts.extend(
                cell.style
                    .map
                    .iter()
                    .map(|(key, value)| format!("{key}={value}")),
            );
            let mut style = parts.join(";");
            if cell.style.suppress_defaults {
                style.insert(0, ';');
            }
            let xml = page(&format!(
                r#"<mxCell id="a" style="{}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
                esc(&style)
            ));
            let converted = load(&xml);
            let object = shapes(&converted.workbook.sheets[0])[0];
            assert!(matches!(
                object.kind,
                CanvasObjectKind::Shape { shape, .. } if shape == expected
            ));
            assert_eq!(
                object.rounding,
                Some(ShapeRounding {
                    radius: cell.style.get("rSize").map_or(10.0, mockup_radius),
                    stencil: mockup_rectangle_stencil(&cell.style).unwrap(),
                    ..Default::default()
                })
            );
            if expected == ShapeKind::RoundedRectangle {
                assert!(converted.warnings.contains(&ImportWarning::Formatting));
            }
        }
    }
    eprintln!("classified {squares} square and {rounded} rounded supplied mockup rectangles");
    let report = import(&bytes).unwrap();
    report.workbook.validate().unwrap();
    let rounding_counts = |book: &Workbook| {
        let mut counts = BTreeMap::new();
        for rounding in book
            .sheets
            .iter()
            .flat_map(|sheet| &sheet.canvas.objects)
            .filter_map(|object| object.rounding)
        {
            *counts
                .entry((format!("{:?}", rounding.stencil), rounding.radius.to_bits()))
                .or_insert(0_usize) += 1;
        }
        counts
    };
    let exported =
        export_bytes(&report.workbook, Format::DrawIo, &ExportOptions::default()).unwrap();
    let reopened = import(&exported).unwrap();
    assert_eq!(
        rounding_counts(&reopened.workbook),
        rounding_counts(&report.workbook)
    );
}

#[test]
fn supplied_requirements_flow_imports_invisible_anchors() {
    let dir = std::env::var_os("THINODA_DRAWIO_CORPUS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("draw-io-examples")
        });
    let path = dir.join("requirements-flow.drawio");
    if !path.is_file() {
        eprintln!("optional requirements-flow.drawio is absent; skipping");
        return;
    }
    let bytes = std::fs::read(path).unwrap();
    let mut count = 0;
    for source_page in read_pages(&bytes).unwrap().pages {
        for cell in read_cells(&source_page.model, &mut BTreeSet::new()) {
            if !cell
                .style
                .get("shape")
                .is_some_and(|shape| INVISIBLE_ANCHORS.contains(&shape))
            {
                continue;
            }
            assert_eq!(classify(&cell.style), Kind::Anchor);
            let mut parts = cell.style.tokens.clone();
            parts.extend(
                cell.style
                    .map
                    .iter()
                    .map(|(key, value)| format!("{key}={value}")),
            );
            let mut style = parts.join(";");
            if cell.style.suppress_defaults {
                style.insert(0, ';');
            }
            let xml = page(&format!(
                r#"<mxCell id="a" style="{}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
                esc(&style)
            ));
            let report = load(&xml);
            let object = shapes(&report.workbook.sheets[0])[0];
            assert_eq!(object.style.fill, Some(Color::TRANSPARENT));
            assert_eq!(object.style.stroke, Some(Color::TRANSPARENT));
            assert!(report.warnings.contains(&ImportWarning::Formatting));
            count += 1;
        }
    }
    eprintln!("classified {count} supplied invisible anchor cells");
    assert!(count > 0, "supplied example contains no recognized anchors");
    let report = import(&bytes).unwrap();
    report.workbook.validate().unwrap();
}

fn shape(kind: ShapeKind, frame: Rect, label: &str) -> CanvasObject {
    let mut object = CanvasObject::shape(kind, frame);
    if let CanvasObjectKind::Shape { label: text, .. } = &mut object.kind {
        *text = label.into();
    }
    object
}

/// A board exercising every native draw.io mapping.
fn board() -> Workbook {
    let mut s = Sheet::new("Board", "");
    s.root_mode = SheetRootMode::Hidden;
    s.board.grid = GridKind::Lines;
    s.board.grid_size = 16.0;
    let mut cloud = shape(
        ShapeKind::Cloud,
        rect(0.0, 300.0, 120.0, 80.0),
        "Cloud & <co>\nline 2",
    );
    cloud.style.fill = Some(Color::rgba(0x33, 0x99, 0xFF, 128));
    cloud.style.stroke = Some(Color::rgb(1, 2, 3));
    cloud.style.text_color = Some(Color::WHITE);
    cloud.style.stroke_width = 3.5;
    cloud.style.opacity = 0.5;
    cloud.rotation = 30.0;
    cloud.alt_text = "A cloud".into();
    let cloud = s.add_canvas_object(cloud).unwrap();
    let mut triangle = shape(ShapeKind::Triangle, rect(200.0, 300.0, 60.0, 40.0), "");
    triangle.band = Band::Below;
    let triangle = s.add_canvas_object(triangle).unwrap();
    let mut text = shape(
        ShapeKind::Rectangle,
        rect(400.0, 0.0, 100.0, 30.0),
        "Free text",
    );
    text.style.fill = Some(Color::TRANSPARENT);
    text.style.stroke = Some(Color::TRANSPARENT);
    s.add_canvas_object(text).unwrap();
    for (i, kind) in [
        ShapeKind::Hexagon,
        ShapeKind::Star,
        ShapeKind::Decision,
        ShapeKind::Cylinder,
        ShapeKind::Chevron,
        ShapeKind::RightTriangle,
    ]
    .into_iter()
    .enumerate()
    {
        s.add_canvas_object(shape(kind, rect(i as f32 * 80.0, 600.0, 60.0, 50.0), ""))
            .unwrap();
    }
    let mut card = Card::new(CardKind::Sticky, Vec2::new(160.0, 120.0));
    card.rotation = 8.0;
    card.color = Some(Color::rgb(0xFF, 0xF2, 0xA8));
    let sticky = s
        .add_card("Sticky", Vec2::new(-200.0, 100.0), card)
        .unwrap();
    s.topic_mut(sticky).unwrap().style.fill = card.color;
    s.topic_mut(sticky).unwrap().hyperlink = Some("https://example.com/?a=1&b=2".into());
    let text_card = s
        .add_card(
            "Text card",
            Vec2::new(-200.0, 400.0),
            Card::new(CardKind::Text, Vec2::new(120.0, 40.0)),
        )
        .unwrap();
    s.add_canvas_object(CanvasObject::ink(InkStroke::new(
        InkTool::Highlighter,
        Color::rgb(250, 220, 0),
        12.0,
        vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(40.0, 30.0),
            Vec2::new(90.0, 10.0),
        ],
    )))
    .unwrap();
    let mut connector = CanvasConnector::new(Endpoint::Object(cloud), Endpoint::Topic(sticky));
    connector.label = "links".into();
    connector.route = ConnectorRoute::Elbow;
    connector.arrow_start = true;
    connector.color = Some(Color::rgb(200, 0, 0));
    connector.width = 4.0;
    s.add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let mut curve = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-50.0, -50.0)),
        Endpoint::Object(triangle),
    );
    curve.route = ConnectorRoute::Curve;
    curve.arrow_end = false;
    s.add_canvas_object(CanvasObject::connector(curve)).unwrap();
    // A locked group of a shape and a card inside a clipped zone.
    let zone = s
        .create_board_frame("Lane", rect(1000.0, 0.0, 600.0, 400.0))
        .unwrap();
    let boxed = s
        .add_canvas_object(shape(
            ShapeKind::RoundedRectangle,
            rect(1100.0, 100.0, 100.0, 60.0),
            "In zone",
        ))
        .unwrap();
    let zone_card = s
        .add_card(
            "Zone card",
            Vec2::new(1400.0, 200.0),
            Card::new(CardKind::Sticky, Vec2::new(100.0, 100.0)),
        )
        .unwrap();
    s.set_zone_members(zone, vec![zone_card], vec![boxed])
        .unwrap();
    {
        let zone = s.zones.iter_mut().find(|z| z.id == zone).unwrap();
        zone.clip = true;
        zone.color = Some(Color::rgb(0x6C, 0x8E, 0xBF));
        zone.background_color = Some(Color::rgb(0xDA, 0xE8, 0xFC));
    }
    let group = s
        .group_board_members(&[BoardMember::Object(boxed), BoardMember::Card(zone_card)])
        .unwrap();
    s.canvas
        .groups
        .iter_mut()
        .find(|g| g.id == group)
        .unwrap()
        .locked = true;
    s.set_board_members_locked(&[BoardMember::Card(text_card)], true)
        .unwrap();
    let mut second = Sheet::new("Second & <page>", "");
    second.root_mode = SheetRootMode::Hidden;
    second
        .add_canvas_object(shape(ShapeKind::Ellipse, rect(0.0, 0.0, 50.0, 50.0), "Two"))
        .unwrap();
    let mut book = Workbook::new(s);
    book.sheets.push(second);
    book.validate().unwrap();
    book
}

fn roundtrip(book: &Workbook) -> Workbook {
    roundtrip_with_warnings(book, &[])
}

fn roundtrip_with_warnings(book: &Workbook, expected: &[ImportWarning]) -> Workbook {
    let bytes = export(book).unwrap();
    let report = import(&bytes).unwrap();
    assert_eq!(report.warnings, expected);
    report.workbook.validate().unwrap();
    report.workbook
}

type CardSummary = (String, Card, Option<Vec2>, Option<String>, Option<Color>);

fn cards(sheet: &Sheet) -> Vec<CardSummary> {
    sheet
        .topics
        .values()
        .filter(|t| t.card.is_some())
        .map(|t| {
            (
                t.title.clone(),
                t.card.unwrap(),
                t.position,
                t.hyperlink.clone(),
                t.style.fill,
            )
        })
        .collect()
}

fn shape_summaries(sheet: &Sheet) -> Vec<String> {
    let mut out: Vec<String> = shapes(sheet)
        .iter()
        .map(|o| {
            format!(
                "{:?}|{:?}|{}|{:?}|{:?}|{}",
                o.kind, o.rect, o.rotation, o.band, o.style, o.alt_text
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn board_round_trips_with_reported_color_approximations_and_is_idempotent() {
    let original = board();
    let warnings = [
        ImportWarning::Formatting,
        ImportWarning::DrawioFoldedNotesApproximated,
    ];
    let first = roundtrip_with_warnings(&original, &warnings);
    let second = roundtrip_with_warnings(&first, &warnings);
    assert_eq!(first, second, "re-export changed the imported board");
    assert_eq!(export(&first).unwrap(), export(&second).unwrap());

    let sheet = &first.sheets[0];
    let source = &original.sheets[0];
    assert_eq!(first.sheets.len(), 2);
    assert_eq!(sheet.title, "Board");
    assert_eq!(first.sheets[1].title, "Second & <page>");
    assert_eq!(sheet.root_mode, SheetRootMode::Hidden);
    assert_eq!(sheet.board.grid, GridKind::Lines);
    assert_eq!(sheet.board.grid_size, 16.0);
    assert_eq!(shape_summaries(sheet), shape_summaries(source));

    let imported_cards = cards(sheet);
    assert_eq!(imported_cards.len(), 3);
    for card in cards(source) {
        assert!(
            imported_cards.contains(&card),
            "missing {card:?} in {imported_cards:?}"
        );
    }

    let links = connectors(sheet);
    assert_eq!(links.len(), 2);
    let elbow = links
        .iter()
        .find(|c| c.route == ConnectorRoute::Elbow)
        .unwrap();
    assert!(matches!(elbow.from, Endpoint::Object(_)));
    assert!(matches!(elbow.to, Endpoint::Topic(_)));
    assert_eq!(
        (elbow.label.as_str(), elbow.arrow_start, elbow.arrow_end),
        ("links", true, true)
    );
    assert_eq!(
        (elbow.color, elbow.width),
        (Some(Color::rgb(200, 0, 0)), 4.0)
    );
    let curve = links
        .iter()
        .find(|c| c.route == ConnectorRoute::Curve)
        .unwrap();
    assert_eq!(curve.from, Endpoint::Point(Vec2::new(-50.0, -50.0)));
    assert!(matches!(curve.to, Endpoint::Object(_)));
    assert!(!curve.arrow_end);

    let ink: Vec<_> = sheet
        .canvas
        .objects
        .iter()
        .filter_map(|o| match &o.kind {
            CanvasObjectKind::Ink(ink) => Some(ink),
            _ => None,
        })
        .collect();
    assert_eq!(ink.len(), 1);
    assert_eq!(ink[0].tool, InkTool::Highlighter);
    assert_eq!(ink[0].width, 12.0);
    assert_eq!(ink[0].points.len(), 3);
    assert_eq!(ink[0].points[1], Vec2::new(40.0, 30.0));

    assert_eq!(sheet.zones.len(), 1);
    let zone = &sheet.zones[0];
    assert_eq!(zone.name, "Lane");
    assert_eq!(zone.frame, rect(1000.0, 0.0, 600.0, 400.0));
    assert!(zone.clip);
    assert_eq!(zone.color, Some(Color::rgb(0x6C, 0x8E, 0xBF)));
    assert_eq!(zone.background_color, Some(Color::rgb(0xDA, 0xE8, 0xFC)));
    assert_eq!((zone.roots.len(), zone.objects.len()), (1, 1));
    assert_eq!(sheet.canvas.groups.len(), 1);
    assert!(sheet.canvas.groups[0].locked);
    assert_eq!(
        sheet.board_group_members(sheet.canvas.groups[0].id).len(),
        2
    );
    let locked_cards: Vec<_> = sheet
        .canvas
        .card_states
        .iter()
        .filter(|s: &&CanvasCardState| s.locked)
        .map(|s| sheet.topic(s.topic).unwrap().title.clone())
        .collect();
    assert_eq!(locked_cards, ["Text card"]);
}

#[test]
fn export_is_well_formed_uncompressed_and_escaped() {
    let text = String::from_utf8(export(&board()).unwrap()).unwrap();
    let root = parse_dom(&text).unwrap();
    assert_eq!(root.name, "mxfile");
    assert_eq!(
        root.children.iter().filter(|c| c.name == "diagram").count(),
        2
    );
    assert!(text.contains(
        "label=\"Cloud &amp;amp; &amp;lt;co&amp;gt;&lt;br&gt;line 2\" id=\"o2\" tooltip=\"A cloud\""
    ));
    assert!(text.contains("name=\"Second &amp; &lt;page&gt;\""));
    assert!(text.contains("shape=cloud;"));
    assert!(text.contains("thinodaCard=sticky;"));
    assert!(text.contains("thinodaInk=highlighter;"));
    assert!(text.contains("thinodaClip=1;"));
    assert!(text.contains("link=\"https://example.com/?a=1&amp;b=2\""));
    let report = load(&text);
    let label = report.workbook.sheets[0]
        .canvas
        .objects
        .iter()
        .map(|o| o.label())
        .find(|l| l.starts_with("Cloud"))
        .map(str::to_owned);
    assert_eq!(label.as_deref(), Some("Cloud & <co>\nline 2"));
}

#[test]
fn golden_compressed_page_imports() {
    let xml = format!(
        "<mxfile host=\"app.diagrams.net\"><diagram id=\"x\" name=\"Flow\">{GOLDEN_COMPRESSED}</diagram></mxfile>"
    );
    let sheet = clean(&xml);
    assert_eq!(sheet.title, "Flow");
    assert_eq!(sheet.board.grid, GridKind::Lines);
    assert_eq!(sheet.board.grid_size, 20.0);
    let objects = shapes(&sheet);
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].label(), "Start & go");
    assert_eq!(objects[0].style.fill, Some(Color::rgb(0xDA, 0xE8, 0xFC)));
    assert_eq!(objects[1].label(), "Ünïcode");
    assert!(matches!(
        &objects[1].kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::RoundedRectangle,
            ..
        }
    ));
    let links = connectors(&sheet);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].route, ConnectorRoute::Elbow);
    assert_eq!(links[0].from, Endpoint::Object(objects[0].id));
}

#[test]
fn published_link_from_the_editor_imports_without_network() {
    // Created using File > Publish > Link > Include a copy of my diagram.
    let published = concat!(
        "https://viewer.diagrams.net/?tags=%7B%7D&lightbox=1&highlight=0000ff",
        "&edit=_blank&layers=1&nav=1&dark=auto#R%3Cmxfile%3E%3Cdiagram%20",
        "name%3D%22Page-1%22%20id%3D%22ccow51MBEuUA37MJHwmB%22%3E",
        "jZLLboMwEEW%2FxstIgFPSbssjXbRSVRZRV5WFJ9iSwciYAvn6mjIEUFopG2vmzMu%2BY0Kjsj8aVos3zUGRwOM9oTEJgsfw4M4RDAj2wQQKI%2FmE%2FAVk8gIIPaSt5NBsEq3Wysp6C3NdVZDbDWPG6G6bdtZqO7VmBdyALGfqlp4ktwJfERwW%2FgKyEPNkP3yaIiWbk%2FEljWBcdytEE0Ijo7WdrLKPQI3azbpMdek%2F0evFDFT2noJLnMafH0l12n0F2Xubvp6jbPdHF0SNHWYNjG4rDmMbj9DnTkgLWc3yMdq5pTsmbKmc5ztzqv5mqsVqBGAs9KsJeMkj6BKsGVyKWOkYomjdork%2FC4ld6AP6%2BLP2HvoMV15cWy%2BqOAOFmd1lAb%2Bx1S%2BmyQ8%3D",
        "%3C%2Fdiagram%3E%3C%2Fmxfile%3E",
    );
    for link in [
        published.to_owned(),
        format!("#{}", published.split_once('#').unwrap().1),
        published.replacen("https://viewer.diagrams.net", "https://example.invalid", 1),
    ] {
        let report = import_link(&link).unwrap();
        assert_eq!(report.format, Format::DrawIo);
        assert_eq!(report.workbook.sheets.len(), 1);
        assert_eq!(report.workbook.sheets[0].title, "Page-1");
        assert_eq!(shapes(&report.workbook.sheets[0]).len(), 1);
        report.workbook.validate().unwrap();
    }
}

#[test]
fn published_links_accept_multiple_pages_and_compressed_graphs() {
    let xml = String::from_utf8(export(&board()).unwrap()).unwrap();
    let link = format!(
        "https://example.invalid/view?mode=test#R{}",
        percent_encode(&xml)
    );
    let report = import_link(&link).unwrap();
    assert_eq!(report.workbook.sheets.len(), 2);
    assert_eq!(report.workbook.sheets[1].title, "Second & <page>");
    assert!(
        shapes(&report.workbook.sheets[0])
            .iter()
            .any(|shape| shape.label() == "Cloud & <co>\nline 2")
    );

    let compressed = format!("#R{}", percent_encode(GOLDEN_COMPRESSED));
    let sheet = &import_link(&compressed).unwrap().workbook.sheets[0];
    assert_eq!(shapes(sheet)[0].label(), "Start & go");
    assert_eq!(shapes(sheet)[1].label(), "Ünïcode");
}

#[test]
fn published_links_reject_invalid_or_external_data() {
    for link in [
        "",
        "https://viewer.diagrams.net/",
        "https://viewer.diagrams.net/#Uhttps%3A%2F%2Fexample.invalid%2Fmap.drawio",
        "https://viewer.diagrams.net/#R",
        "https://viewer.diagrams.net/#R%ZZ",
        "https://viewer.diagrams.net/#R%FF",
        "https://viewer.diagrams.net/#Rnot-base64",
        "file:///tmp/diagram#R%3Cmxfile%2F%3E",
        "https://viewer.diagrams.net/#R%3Cmxfile%3E",
    ] {
        assert!(import_link(link).is_err(), "{link}");
    }
    let dtd = percent_encode("<!DOCTYPE mxfile><mxfile><diagram/></mxfile>");
    assert!(import_link(&format!("#R{dtd}")).is_err());
    assert!(import_link(&format!("#R{}", "x".repeat(MAX_LINK_BYTES))).is_err());
}

#[test]
fn zlib_wrapped_page_and_bare_graph_model_import() {
    let inner = "<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/><mxCell id=\"v\" value=\"Z\" vertex=\"1\" parent=\"1\"><mxGeometry width=\"10\" height=\"10\" as=\"geometry\"/></mxCell></root></mxGraphModel>";
    let data = miniz_oxide::deflate::compress_to_vec_zlib(inner.as_bytes(), 6);
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let sheet = clean(&format!(
        "<mxfile><diagram>\n{encoded}\n</diagram></mxfile>"
    ));
    assert_eq!(sheet.title, "Page-1");
    assert_eq!(shapes(&sheet)[0].label(), "Z");
    let bare = clean(inner);
    assert_eq!(shapes(&bare)[0].label(), "Z");
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            b => format!("%{b:02X}"),
        })
        .collect()
}

fn png_chunk(png: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
    png.extend((data.len() as u32).to_be_bytes());
    png.extend(kind);
    png.extend(data);
    png.extend([0; 4]);
}

#[test]
fn svg_and_png_containers_are_detected_and_imported() {
    let model = page(
        r#"<mxCell id="v" value="Embedded" style="ellipse;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#,
    );
    let escaped = model
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;");
    let svg = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\"><svg xmlns=\"http://www.w3.org/2000/svg\" content=\"{escaped}\"><rect/></svg>"
    );
    assert_eq!(
        detect_format(Path::new("a.drawio.svg"), svg.as_bytes()),
        Some(Format::DrawIo)
    );
    assert_eq!(shapes(&clean(&svg))[0].label(), "Embedded");
    let plain_svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>";
    assert_eq!(
        detect_format(Path::new("a.svg"), plain_svg.as_bytes()),
        Some(Format::Svg)
    );

    let mut png = PNG_SIGNATURE.to_vec();
    png_chunk(&mut png, b"IHDR", &[0; 13]);
    let mut text = b"mxfile\0".to_vec();
    text.extend(percent_encode(&model).bytes());
    png_chunk(&mut png, b"tEXt", &text);
    png_chunk(&mut png, b"IEND", &[]);
    assert_eq!(
        detect_format(Path::new("a.drawio.png"), &png),
        Some(Format::DrawIo)
    );
    let report = import(&png).unwrap();
    assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Embedded");
    let mut plain_png = PNG_SIGNATURE.to_vec();
    png_chunk(&mut plain_png, b"IEND", &[]);
    assert_eq!(
        detect_format(Path::new("a.png"), &plain_png),
        Some(Format::Png)
    );

    for name in ["a.xml", "diagram", "a.dio", "a.drawio"] {
        assert_eq!(
            detect_format(Path::new(name), model.as_bytes()),
            Some(Format::DrawIo),
            "{name}"
        );
    }
}

#[test]
fn legacy_svg_uri_and_base64_content_are_detected_and_imported() {
    let label = "Legacy \u{fc}";
    let model = page(&format!(
        r#"<mxCell id="v" value="{label}" style="ellipse;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#
    ));
    let xml = format!("<?xml version=\"1.0\"?>{model}");
    let uri = percent_encode(&xml);
    let compressed = miniz_oxide::deflate::compress_to_vec(uri.as_bytes(), 6);
    let encoded = [
        ("URI", uri.clone()),
        (
            "base64 XML",
            base64::engine::general_purpose::STANDARD.encode(xml.as_bytes()),
        ),
        (
            "base64 URI",
            base64::engine::general_purpose::STANDARD.encode(uri.as_bytes()),
        ),
        (
            "base64 compressed",
            base64::engine::general_purpose::STANDARD.encode(compressed),
        ),
    ];
    for (encoding, content) in encoded {
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" content="{content}"/>"#);
        assert_eq!(
            detect_format(Path::new("legacy.svg"), svg.as_bytes()),
            Some(Format::DrawIo),
            "{encoding}"
        );
        let report = import(svg.as_bytes()).unwrap();
        assert!(
            report.warnings.is_empty(),
            "{encoding}: {:?}",
            report.warnings
        );
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), label);
    }

    let unrelated = r#"<svg content="&lt;mxfileUnexpected/&gt;"/>"#;
    assert_eq!(
        detect_format(Path::new("picture.svg"), unrelated.as_bytes()),
        Some(Format::Svg)
    );
    assert!(import(br#"<svg content="%3Cmxfile%ZZ"/>"#).is_err());
}

#[test]
fn legacy_png_keyword_is_detected_and_imported() {
    let label = "Legacy \u{fc}";
    let model = page(&format!(
        r#"<mxCell id="v" value="{label}" style="ellipse;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#
    ));
    let text = percent_encode(&model);
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(text.as_bytes(), 6);
    for (kind, flag) in [(b"tEXt", 0), (b"zTXt", 1), (b"iTXt", 0), (b"iTXt", 1)] {
        let mut data = b"mxGraphModel\0".to_vec();
        match kind {
            b"zTXt" => data.push(0),
            b"iTXt" => data.extend_from_slice(&[flag, 0, 0, 0]),
            _ => {}
        }
        data.extend_from_slice(if flag == 1 {
            &compressed
        } else if kind == b"tEXt" {
            text.as_bytes()
        } else {
            model.as_bytes()
        });
        let mut png = PNG_SIGNATURE.to_vec();
        png_chunk(&mut png, b"IHDR", &[0; 13]);
        png_chunk(&mut png, kind, &data);
        png_chunk(&mut png, b"IEND", &[]);
        for name in ["legacy.png", "legacy.drawio.png"] {
            assert_eq!(detect_format(Path::new(name), &png), Some(Format::DrawIo));
        }
        let report = import(&png).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), label);
    }

    let bare_model = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="v" value="Bare" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell></root></mxGraphModel>"#;
    let mut text = b"mxGraphModel\0".to_vec();
    text.extend_from_slice(percent_encode(bare_model).as_bytes());
    let mut png = PNG_SIGNATURE.to_vec();
    png_chunk(&mut png, b"tEXt", &text);
    png_chunk(&mut png, b"IEND", &[]);
    let report = import(&png).unwrap();
    assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Bare");

    for (flag, method) in [(2, 0), (0, 1), (1, 1)] {
        let mut data = b"mxGraphModel\0".to_vec();
        data.extend_from_slice(&[flag, method, 0, 0]);
        data.extend_from_slice(model.as_bytes());
        let mut png = PNG_SIGNATURE.to_vec();
        png_chunk(&mut png, b"iTXt", &data);
        png_chunk(&mut png, b"IEND", &[]);
        assert_eq!(
            detect_format(Path::new("invalid.png"), &png),
            Some(Format::Png)
        );
        assert!(import(&png).is_err());
    }
}

#[test]
fn png_compressed_and_international_text_chunks_import_diagrams() {
    let label = "Embedded \u{fc}";
    let model = page(&format!(
        r#"<mxCell id="v" value="{label}" style="ellipse;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#
    ));
    let text = percent_encode(&model);
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(text.as_bytes(), 6);
    for (kind, flag) in [(b"zTXt", 1), (b"iTXt", 0), (b"iTXt", 1)] {
        let mut data = b"mxfile\0".to_vec();
        if kind == b"zTXt" {
            data.push(0);
        } else {
            data.extend([flag, 0]);
            data.extend_from_slice(b"en\0Diagram\0");
        }
        data.extend_from_slice(if flag == 1 {
            &compressed
        } else {
            model.as_bytes()
        });
        let mut png = PNG_SIGNATURE.to_vec();
        png_chunk(&mut png, b"IHDR", &[0; 13]);
        png_chunk(&mut png, kind, &data);
        png_chunk(&mut png, b"IEND", &[]);
        assert_eq!(
            detect_format(Path::new("embedded.drawio.png"), &png),
            Some(Format::DrawIo)
        );
        let report = import(&png).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), label);
    }
}

#[test]
fn published_drawio_png_export_imports_its_embedded_diagram() {
    let png = include_bytes!("../../tests/fixtures/published-rectangle.drawio.png");
    assert_eq!(
        detect_format(Path::new("published-rectangle.drawio.png"), png),
        Some(Format::DrawIo)
    );
    let report = import(png).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let sheet = &report.workbook.sheets[0];
    assert_eq!(sheet.title, "Page-1");
    let objects = shapes(sheet);
    assert_eq!(objects.len(), 1);
    assert!(matches!(
        objects[0].kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            ..
        }
    ));
    assert_eq!(objects[0].label(), "Published PNG");
    assert_eq!(objects[0].style.fill, Some(Color::rgb(0xd5, 0xe8, 0xd4)));
    assert_eq!(objects[0].rect, rect(350.0, 380.0, 120.0, 60.0));
}

fn html_viewer(xml: &str) -> String {
    let data = serde_json::json!({ "xml": xml, "edit": "_blank" }).to_string();
    format!(
        r#"<div class="example mxgraph" data-mxgraph="{}"></div>"#,
        html_escape::encode_double_quoted_attribute(&data)
    )
}

fn legacy_html_viewer(data: &str) -> String {
    format!(
        r#"<div class="mxgraph"><div style="width:1px;height:1px;overflow:hidden;">{data}</div></div>"#
    )
}

#[test]
fn published_drawio_html_export_imports_its_embedded_diagram() {
    let html = include_bytes!("../../tests/fixtures/published-rectangle.drawio.html");
    for name in ["published.html", "published.HTM", "published.drawio.html"] {
        assert_eq!(Format::from_path(Path::new(name)), Some(Format::DrawIoHtml));
        assert_eq!(
            detect_format(Path::new(name), html),
            Some(Format::DrawIoHtml)
        );
        let report = crate::import_named_bytes_with_report(Path::new(name), html).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        let sheet = &report.workbook.sheets[0];
        assert_eq!(sheet.title, "Page-1");
        let objects = shapes(sheet);
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].label(), "Published PNG");
        assert_eq!(objects[0].style.fill, Some(Color::rgb(0xd5, 0xe8, 0xd4)));
        assert_eq!(objects[0].rect, rect(350.0, 380.0, 120.0, 60.0));
    }
    assert_eq!(
        detect_format(Path::new("without-extension"), html),
        Some(Format::DrawIoHtml)
    );
    assert!(Format::DrawIoHtml.can_import());
    assert!(!Format::DrawIoHtml.can_export());
}

#[test]
fn html_viewer_preserves_entities_pages_and_ignores_inert_markup() {
    let xml = r#"<mxfile><diagram name="First"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="v" value="A &amp; B &quot;C&quot; ü" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell></root></mxGraphModel></diagram><diagram name="Second"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#;
    let fake = r#"<div class="mxgraph" data-mxgraph="not a diagram"></div>"#;
    let html = format!(
        "<!DOCTYPE html><html><head><style>{fake}</style><script>{fake}</script></head><body><!-- {fake} --><textarea>{fake}</textarea><img src=\"https://example.invalid/image\">{}</body></html>",
        html_viewer(xml)
    );
    let report = import_html(html.as_bytes()).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.workbook.sheets.len(), 2);
    assert_eq!(report.workbook.sheets[0].title, "First");
    assert_eq!(report.workbook.sheets[1].title, "Second");
    assert_eq!(
        shapes(&report.workbook.sheets[0])[0].label(),
        "A & B \"C\" ü"
    );
}

#[test]
fn html_viewer_rejects_missing_remote_malformed_and_ambiguous_diagrams() {
    let valid = html_viewer(&page(""));
    for html in [
        r#"<html><body>No diagram</body></html>"#.to_owned(),
        r#"<div class="mxgraph"></div>"#.to_owned(),
        r#"<div class="mxgraph" data-mxgraph="not JSON"></div>"#.to_owned(),
        r#"<div class="mxgraph" data-mxgraph='{"url":"https://example.invalid/diagram"}'></div>"#
            .to_owned(),
        r#"<div class="mxgraph" data-mxgraph='{"xml":42}'></div>"#.to_owned(),
        format!("{valid}{valid}"),
        format!(r#"<div class="mxgraph"></div>{valid}"#),
    ] {
        assert!(import_html(html.as_bytes()).is_err(), "{html}");
    }
    let remote =
        r#"<div class="mxgraph" data-mxgraph='{"url":"https://example.invalid/diagram"}'></div>"#;
    assert!(matches!(
        import_html(remote.as_bytes()),
        Err(IoError::DrawIoHtml(
            crate::DrawIoHtmlError::NoEmbeddedDiagram
        ))
    ));
    assert!(matches!(
        import_html(format!("{valid}{valid}").as_bytes()),
        Err(IoError::DrawIoHtml(crate::DrawIoHtmlError::MultipleViewers))
    ));
    assert!(matches!(
        import_html(b"<div>\xff</div>"),
        Err(IoError::DrawIoHtml(crate::DrawIoHtmlError::InvalidEncoding))
    ));
    let dtd = html_viewer(r#"<!DOCTYPE mxfile [<!ENTITY x SYSTEM "file:///secret">]><mxfile/>"#);
    assert!(import_html(dtd.as_bytes()).is_err());
}

#[test]
fn legacy_html_viewers_import_compressed_and_uri_encoded_multipage_diagrams() {
    let xml = r#"<mxfile><diagram name="First"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="v" value="A &amp; B &quot;C&quot; ü" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell></root></mxGraphModel></diagram><diagram name="Second"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#;
    let uri = percent_encode(xml);
    let compressed = base64::engine::general_purpose::STANDARD
        .encode(miniz_oxide::deflate::compress_to_vec(uri.as_bytes(), 6));
    let escaped_xml = xml
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    for data in [uri, compressed, escaped_xml] {
        let html = format!(
            r#"<!doctype html><DIV class="example mx&#103;raph"><section><DIV>{data}</DIV><div>preview only</div></section></DIV>"#
        );
        for name in ["legacy.html", "legacy.HTM", "without-extension"] {
            assert_eq!(
                detect_format(Path::new(name), html.as_bytes()),
                Some(Format::DrawIoHtml),
                "{name}"
            );
            let report =
                crate::import_named_bytes_with_report(Path::new(name), html.as_bytes()).unwrap();
            assert!(report.warnings.is_empty(), "{:?}", report.warnings);
            assert_eq!(report.workbook.sheets.len(), 2);
            assert_eq!(report.workbook.sheets[0].title, "First");
            assert_eq!(report.workbook.sheets[1].title, "Second");
            assert_eq!(
                shapes(&report.workbook.sheets[0])[0].label(),
                "A & B \"C\" ü"
            );
        }
    }
}

#[test]
fn legacy_html_viewer_imports_independently_encoded_bare_model() {
    let html = legacy_html_viewer(GOLDEN_COMPRESSED);
    let report = import_html(html.as_bytes()).unwrap();
    let sheet = &report.workbook.sheets[0];
    assert_eq!(shapes(sheet)[0].label(), "Start & go");
    assert_eq!(shapes(sheet)[1].label(), "Ünïcode");
}

#[test]
fn legacy_html_viewer_decodes_entities_across_streaming_chunks() {
    let xml = page(
        r#"<mxCell id="v" value="Chunk ü" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#,
    );
    let data = percent_encode(&xml).replacen('%', "&#37;", 1);
    let viewer = legacy_html_viewer(&data);
    let entity = viewer.find("&#37;").unwrap();
    let html = format!("{}{viewer}", " ".repeat(16 * 1024 - entity - 2));
    let report = import_html(html.as_bytes()).unwrap();
    assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Chunk ü");
}

#[test]
fn legacy_html_viewers_reject_ambiguous_or_malformed_markup() {
    let data = percent_encode(&page(""));
    let valid = legacy_html_viewer(&data);
    let current = html_viewer(&page(""));
    for html in [
        format!(r#"<div class="mxgraph"><div>{data}</div>"#),
        format!(r#"<div class="mxgraph"></div><div>{data}</div>"#),
        format!(r#"<div class="mxgraph"><div></div><div>{data}</div></div>"#),
        format!(r#"<div class="mxgraph"><div><span>{data}</span></div></div>"#),
        format!(r#"<div class="mxgraph"><div><!--comment-->{data}</div></div>"#),
        r#"<div class="mxgraph"><div>%ZZ</div></div>"#.to_owned(),
        r#"<div class="mxgraph"><div>not-compressed</div></div>"#.to_owned(),
        format!("{valid}{valid}"),
        format!("{current}{valid}"),
        format!(r#"<div class="mxgraph" data-mxgraph="invalid"><div>{data}</div></div>"#),
    ] {
        assert!(import_html(html.as_bytes()).is_err(), "{html}");
    }
    assert!(matches!(
        import_html(format!("{valid}{current}").as_bytes()),
        Err(IoError::DrawIoHtml(crate::DrawIoHtmlError::MultipleViewers))
    ));
    let dtd = percent_encode(r#"<!DOCTYPE mxfile [<!ENTITY x SYSTEM "file:///secret">]><mxfile/>"#);
    assert!(import_html(legacy_html_viewer(&dtd).as_bytes()).is_err());
}

#[test]
fn editable_image_exports_embed_only_the_selected_sheet() {
    let mut first = Sheet::new("First", "");
    first.root_mode = SheetRootMode::Hidden;
    first.board.grid = GridKind::Lines;
    first
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(10.0, 20.0, 120.0, 60.0),
            "First page secret",
        ))
        .unwrap();
    let mut second = Sheet::new("Second", "");
    second.root_mode = SheetRootMode::Hidden;
    second.board.grid = GridKind::Lines;
    second.board.snap = true;
    let label = "A < B & \"C\" \u{fc}";
    second
        .add_canvas_object(shape(
            ShapeKind::Ellipse,
            rect(40.0, 50.0, 100.0, 80.0),
            label,
        ))
        .unwrap();
    let mut book = Workbook::new(first);
    book.sheets.push(second);
    book.active_sheet = 1;
    book.drawio = Some(Arc::new(DrawioDocumentPassthrough {
        attributes: vec![("owner".into(), "team & <design>".into())],
    }));
    let opts = ExportOptions {
        sheet: Some(1),
        ..Default::default()
    };
    for (format, name) in [
        (Format::DrawIoSvg, "diagram.drawio.svg"),
        (Format::DrawIoPng, "diagram.drawio.png"),
    ] {
        assert_eq!(Format::from_path(Path::new(name)), Some(format));
        assert!(format.can_import() && format.can_export());
        assert!(export_losses(&book, format, &opts).is_empty());
        let bytes = export_bytes(&book, format, &opts).unwrap();
        assert_eq!(
            bytes,
            export_bytes(&book, format, &ExportOptions::default()).unwrap()
        );
        assert_eq!(detect_format(Path::new(name), &bytes), Some(Format::DrawIo));
        if format == Format::DrawIoPng {
            let first_chunk_len = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
            let second_chunk = 8 + 12 + first_chunk_len;
            assert_eq!(&bytes[second_chunk + 4..second_chunk + 8], b"tEXt");
            assert_eq!(&bytes[second_chunk + 8..second_chunk + 15], b"mxfile\0");
            let preview = image::load_from_memory(&bytes).unwrap().to_rgba8();
            let plain = export_bytes(&book, Format::Png, &opts).unwrap();
            assert_eq!(preview, image::load_from_memory(&plain).unwrap().to_rgba8());
            let other = export_bytes(
                &book,
                Format::Png,
                &ExportOptions {
                    sheet: Some(0),
                    ..opts.clone()
                },
            )
            .unwrap();
            assert_ne!(preview, image::load_from_memory(&other).unwrap().to_rgba8());
        } else {
            let svg = std::str::from_utf8(&bytes).unwrap();
            resvg::usvg::Tree::from_str(svg, &Default::default()).unwrap();
            let (_, visible) = svg
                .strip_prefix("<svg content=\"")
                .unwrap()
                .split_once('"')
                .unwrap();
            let plain = export_bytes(&book, Format::Svg, &opts).unwrap();
            assert_eq!(format!("<svg{visible}").as_bytes(), plain);
            let other = export_bytes(
                &book,
                Format::Svg,
                &ExportOptions {
                    sheet: Some(0),
                    ..opts.clone()
                },
            )
            .unwrap();
            assert_ne!(plain, other);
        }
        let report = import(&bytes).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.workbook.drawio, book.drawio);
        assert_eq!(report.workbook.sheets.len(), 1);
        assert_eq!(report.workbook.sheets[0].title, "Second");
        let objects = shapes(&report.workbook.sheets[0]);
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].label(), label);
        assert!(!String::from_utf8_lossy(&bytes).contains("First page secret"));
    }
}

#[test]
fn editable_image_exports_reject_invalid_render_containers_and_oversized_output() {
    let sheet = Sheet::new("Board", "");
    assert!(embedded_image_fits(MAX_BYTES - 3, 2, 1));
    assert!(!embedded_image_fits(MAX_BYTES - 3, 3, 1));
    assert!(!embedded_image_fits(usize::MAX, 1, 0));
    for svg in [
        "",
        "<svg>",
        "<svg xmlns=\"x\"><g></svg>",
        "<svg content=\"existing\"></svg>",
        "<svg xmlns=\"x\"></svg><svg xmlns=\"x\"></svg>",
    ] {
        assert!(embed_svg(svg.to_owned(), &sheet, None).is_err(), "{svg}");
    }

    let png = image::DynamicImage::new_rgba8(1, 1);
    let mut buffer = std::io::Cursor::new(Vec::new());
    png.write_to(&mut buffer, image::ImageFormat::Png).unwrap();
    let png = buffer.into_inner();
    assert!(embed_png(png.clone(), &sheet, None).is_ok());
    let invalid_document = DrawioDocumentPassthrough {
        attributes: vec![("bad\" name".into(), "x".into())],
    };
    assert!(
        embed_svg(
            "<svg xmlns=\"x\"></svg>".into(),
            &sheet,
            Some(&invalid_document)
        )
        .is_err()
    );
    assert!(embed_png(png.clone(), &sheet, Some(&invalid_document)).is_err());
    for invalid in [
        vec![],
        PNG_SIGNATURE.to_vec(),
        [PNG_SIGNATURE, b"\0\0\0\0IEND\xaeB\x60\x82"].concat(),
        png[..png.len() - 1].to_vec(),
        {
            let mut corrupted = png.clone();
            corrupted[20] ^= 1;
            corrupted
        },
        {
            let mut trailing = png.clone();
            trailing.push(0);
            trailing
        },
    ] {
        assert!(embed_png(invalid, &sheet, None).is_err());
    }
    let oversized_svg = format!("<svg xmlns=\"x\">{}</svg>", " ".repeat(MAX_BYTES));
    assert!(
        embed_svg(oversized_svg, &sheet, None)
            .unwrap_err()
            .to_string()
            .contains("size limit")
    );
    let base_svg = "<svg xmlns=\"x\"></svg>";
    let embedded_length = embed_svg(base_svg.into(), &sheet, None).unwrap().len();
    let padded_svg = format!(
        "<svg xmlns=\"x\">{}</svg>",
        " ".repeat(MAX_BYTES - embedded_length)
    );
    let document = DrawioDocumentPassthrough {
        attributes: vec![("owner".into(), "&".repeat(1024))],
    };
    assert!(
        embed_svg(padded_svg, &sheet, Some(&document))
            .unwrap_err()
            .to_string()
            .contains("size limit")
    );
    let oversized_png = {
        let mut bytes = png;
        bytes.resize(MAX_BYTES + 1, 0);
        bytes
    };
    assert!(
        embed_png(oversized_png, &sheet, None)
            .unwrap_err()
            .to_string()
            .contains("size limit")
    );
}

#[test]
fn editable_image_guards_apply_to_the_selected_sheet_only() {
    let mut selected = Sheet::new("Selected", "");
    selected.root_mode = SheetRootMode::Hidden;
    selected.board.grid = GridKind::Lines;
    selected.board.snap = false;
    selected
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 100.0, 60.0),
            "Selected",
        ))
        .unwrap();
    let mut book = Workbook::new(selected);
    book.sheets.push(Sheet::new("Other", "Root"));
    let selected_opts = ExportOptions {
        sheet: Some(0),
        ..Default::default()
    };
    let other_opts = ExportOptions {
        sheet: Some(1),
        ..Default::default()
    };

    let mut equations = book.clone();
    let sheet = &mut equations.sheets[1];
    let source = thinoda_core::equations::replace_note("", Some("x^{2}")).unwrap();
    sheet.set_markdown_notes(sheet.root, Some(source)).unwrap();

    let mut resources = book.clone();
    let sheet = &mut resources.sheets[1];
    let resource = Resource::new("picture.png", "image/png", tiny_png(4, 4)).unwrap();
    let root = sheet.root;
    images::set_image(sheet, root, Some((resource, Vec2::new(4.0, 4.0)))).unwrap();

    let mut symbols = book.clone();
    let sheet = &mut symbols.sheets[1];
    sheet
        .toggle_marker_color(&[sheet.root], "star-red", Some(Color::rgb(20, 40, 60)))
        .unwrap();

    let fim_xml = format!(
        "<x:xdatenfelder.datenfeld.0104 xmlns:x='{}'><x:header/><x:datenfeld><x:identifikation><x:id>F123</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Field</x:name></x:datenfeld></x:xdatenfelder.datenfeld.0104>",
        thinoda_core::fim::Dialect::Xdf3.namespace()
    );
    let mut fim = book.clone();
    fim.sheets[1] = thinoda_core::fim::FimDocument::parse(fim_xml.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();

    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        for case in [&equations, &resources, &symbols, &fim] {
            assert!(
                crate::export_bytes_with_consent(case, format, &selected_opts, true).is_ok(),
                "{format:?}: unselected sheet blocked export"
            );
        }
        assert!(matches!(
            crate::export_bytes_with_consent(&equations, format, &other_opts, true),
            Err(IoError::EquationNotesUnsupported)
        ));
        assert!(
            crate::export_bytes_with_consent(&resources, format, &other_opts, true)
                .unwrap_err()
                .to_string()
                .contains("embedded resources")
        );
        assert!(!crate::symbol_color_loss_with_options(
            &symbols,
            format,
            &selected_opts
        ));
        assert!(crate::symbol_color_loss_with_options(
            &symbols,
            format,
            &other_opts
        ));
        assert!(
            crate::export_bytes_with_consent(&symbols, format, &other_opts, true)
                .unwrap_err()
                .to_string()
                .contains("symbol colors")
        );
        let accepted = ExportOptions {
            allow_symbol_color_loss: true,
            ..other_opts.clone()
        };
        assert!(crate::export_bytes_with_consent(&symbols, format, &accepted, true).is_ok());
        assert!(
            crate::export_bytes_with_consent(&fim, format, &other_opts, true)
                .unwrap_err()
                .to_string()
                .contains("FIM")
        );
    }

    assert!(matches!(
        crate::export_bytes_with_consent(&equations, Format::DrawIo, &selected_opts, true),
        Err(IoError::EquationNotesUnsupported)
    ));
}

#[test]
fn nested_connector_waypoints_import_and_round_trip_in_parent_coordinates() {
    let xml = page(
        r#"
        <mxCell id="g" style="group" vertex="1" parent="1"><mxGeometry x="100" y="200" width="300" height="200" as="geometry"/></mxCell>
        <mxCell id="a" value="A" vertex="1" parent="g"><mxGeometry width="20" height="20" as="geometry"/></mxCell>
        <mxCell id="e" value="Route" style="edgeStyle=none;endArrow=none;fontColor=#000000;fontSize=12;" edge="1" parent="g"><mxGeometry relative="1" as="geometry"><mxPoint x="50" y="10" as="sourcePoint"/><mxPoint x="150" y="90" as="targetPoint"/><Array as="points"><mxPoint x="80" y="10"/><mxPoint x="80" y="90"/></Array></mxGeometry></mxCell>
        "#,
    );
    let sheet = clean(&xml);
    let connector = connectors(&sheet)[0];
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(150.0, 210.0)));
    assert_eq!(connector.to, Endpoint::Point(Vec2::new(250.0, 290.0)));
    assert_eq!(
        connector.waypoints.as_ref(),
        &[Vec2::new(180.0, 210.0), Vec2::new(180.0, 290.0)]
    );
    let restored = roundtrip(&Workbook::new(sheet));
    let connector = connectors(&restored.sheets[0])[0];
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(150.0, 210.0)));
    assert_eq!(
        connector.waypoints.as_ref(),
        &[Vec2::new(180.0, 210.0), Vec2::new(180.0, 290.0)]
    );
}

#[test]
fn malformed_waypoints_are_reported_and_do_not_create_fabricated_points() {
    let xml = page(
        r#"
        <mxCell id="e" style="edgeStyle=none;endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/><Array as="points"><mxPoint x="5" y="6"/><mxPoint x="NaN" y="3"/><mxPoint x="5" y="invalid"/><mxPoint y="9"/></Array></mxGeometry></mxCell>
        "#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    assert_eq!(
        connectors(&report.workbook.sheets[0])[0].waypoints.as_ref(),
        &[Vec2::new(5.0, 6.0), Vec2::new(0.0, 9.0)]
    );
}

#[test]
fn out_of_range_waypoints_and_ink_points_are_discarded_after_translation() {
    let xml = page(
        r#"<mxCell id="g" style="group" vertex="1" parent="1"><mxGeometry x="100" y="200" width="200" height="200" as="geometry"/></mxCell>
        <mxCell id="e" style="edgeStyle=none;endArrow=none;" edge="1" parent="g"><mxGeometry relative="1" as="geometry"><mxPoint x="-1000050" as="sourcePoint"/><mxPoint x="-1000000" as="targetPoint"/><Array as="points"><mxPoint x="-1000100"/><mxPoint x="999901"/><mxPoint x="-1000050" y="-1000201"/><mxPoint x="-100"/></Array></mxGeometry></mxCell>
        <mxCell id="ink" style="thinodaInk=1;endArrow=none;" edge="1" parent="g"><mxGeometry relative="1" as="geometry"><mxPoint x="999901" as="sourcePoint"/><Array as="points"><mxPoint x="-100"/><mxPoint x="0"/></Array><mxPoint x="50" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    let sheet = &report.workbook.sheets[0];
    let edge = connectors(sheet)[0];
    assert_eq!(edge.from, Endpoint::Point(Vec2::new(-999_950.0, 200.0)));
    assert_eq!(edge.to, Endpoint::Point(Vec2::new(-999_900.0, 200.0)));
    assert_eq!(
        edge.waypoints.as_ref(),
        &[Vec2::new(-1_000_000.0, 200.0), Vec2::new(0.0, 200.0)]
    );
    let ink = sheet
        .canvas
        .objects
        .iter()
        .find_map(|object| match &object.kind {
            CanvasObjectKind::Ink(ink) => Some(ink),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        ink.points.as_ref(),
        &[
            Vec2::new(0.0, 200.0),
            Vec2::new(100.0, 200.0),
            Vec2::new(150.0, 200.0)
        ]
    );
    assert!(matches!(
        crate::import_bytes(Format::DrawIo, xml.as_bytes()),
        Err(IoError::ImportNeedsConfirmation(warnings))
            if warnings == vec![ImportWarning::Other]
    ));
    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    assert_eq!(
        crate::import_drawio_link(&link).unwrap().warnings,
        report.warnings
    );
    assert_eq!(
        crate::drawio_clipboard::decode(&xml).unwrap().warnings,
        report.warnings
    );
    sheet.validate().unwrap();
}

#[test]
fn overflowing_translated_waypoints_do_not_remove_usable_connectors() {
    let xml = page(
        r#"<mxCell id="g" style="group" vertex="1" parent="1"><mxGeometry x="3e38" as="geometry"/></mxCell>
        <mxCell id="e" style="edgeStyle=none;endArrow=none;" edge="1" parent="g"><mxGeometry relative="1" as="geometry"><mxPoint x="-3e38" as="sourcePoint"/><mxPoint x="-3e38" as="targetPoint"/><Array as="points"><mxPoint x="3e38"/><mxPoint x="-3e38"/></Array></mxGeometry></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    let edge = connectors(&report.workbook.sheets[0])[0];
    assert_eq!(edge.waypoints.as_ref(), &[Vec2::new(0.0, 0.0)]);
    report.workbook.sheets[0].validate().unwrap();
}

#[test]
fn out_of_range_terminal_points_are_discarded_without_losing_valid_attachments() {
    let shape = r#"<mxCell id="shape" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#;
    for (terminal, expect_connector) in [
        (r#" source="shape""#, true),
        (r#" source="missing""#, false),
        ("", false),
    ] {
        let xml = page(&format!(
            r#"{shape}<mxCell id="e" style="endArrow=none;" edge="1" parent="1"{terminal} target="shape"><mxGeometry relative="1" as="geometry"><mxPoint x="1000001" as="sourcePoint"/></mxGeometry></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Other]);
        let sheet = &report.workbook.sheets[0];
        assert_eq!(connectors(sheet).len(), usize::from(expect_connector));
        if expect_connector {
            assert_eq!(
                connectors(sheet)[0].from,
                Endpoint::Object(shapes(sheet)[0].id)
            );
        }
        sheet.validate().unwrap();
    }
}

#[test]
fn all_unrepresentable_ink_points_are_omitted() {
    let xml = page(
        r#"<mxCell id="ink" style="thinodaInk=1;endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="1000001" as="sourcePoint"/><Array as="points"><mxPoint y="-1000001"/></Array><mxPoint x="1000002" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    assert!(report.workbook.sheets[0].canvas.objects.is_empty());
    report.workbook.sheets[0].validate().unwrap();
}

#[test]
fn out_of_range_group_center_uses_only_a_usable_explicit_fallback() {
    let cells = r#"<mxCell id="g" style="group" vertex="1" parent="1"><mxGeometry x="2000000" width="100" height="100" as="geometry"/></mxCell>
        <mxCell id="safe" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#;
    for fallback in [None, Some("7"), Some("1000001")] {
        let point = fallback
            .map(|x| format!(r#"<mxPoint x="{x}" y="8" as="sourcePoint"/>"#))
            .unwrap_or_default();
        let edge = format!(
            r#"<mxCell id="e" style="endArrow=none;" edge="1" parent="1" source="g" target="safe"><mxGeometry relative="1" as="geometry">{point}</mxGeometry></mxCell>"#
        );
        for edge_first in [true, false] {
            let xml = if edge_first {
                page(&format!("{edge}{cells}"))
            } else {
                page(&format!("{cells}{edge}"))
            };
            let report = load(&xml);
            let sheet = &report.workbook.sheets[0];
            let edges = connectors(sheet);
            if fallback == Some("7") {
                assert_eq!(
                    report.warnings,
                    vec![ImportWarning::DetachedConnectors, ImportWarning::Other]
                );
                assert_eq!(edges.len(), 1);
                assert_eq!(edges[0].from, Endpoint::Point(Vec2::new(7.0, 8.0)));
                assert_eq!(edges[0].to, Endpoint::Object(shapes(sheet)[0].id));
            } else {
                assert_eq!(report.warnings, vec![ImportWarning::Other]);
                assert!(edges.is_empty());
            }
            sheet.validate().unwrap();
        }
    }
}

#[test]
fn malformed_connector_endpoints_are_reported_without_fabricating_points() {
    let shape = r#"<mxCell id="shape" vertex="1" parent="1"><mxGeometry x="100" width="40" height="40" as="geometry"/></mxCell>"#;
    for (point, source, target) in [
        ("sourcePoint", "", r#" target="shape""#),
        ("sourcePoint", r#" source="missing""#, r#" target="shape""#),
        ("targetPoint", r#" source="shape""#, ""),
        ("targetPoint", r#" source="shape""#, r#" target="missing""#),
    ] {
        for coordinate in ["x", "y"] {
            for value in ["", "   ", "invalid", "NaN", "inf", "1e1000"] {
                let xml = page(&format!(
                    r#"{shape}<mxCell id="e" style="endArrow=none;" edge="1" parent="1"{source}{target}><mxGeometry relative="1" as="geometry"><mxPoint {coordinate}="{value}" as="{point}"/></mxGeometry></mxCell>"#
                ));
                let report = load(&xml);
                assert_eq!(
                    report.warnings,
                    vec![ImportWarning::Other],
                    "{point} {coordinate}={value} {source} {target}"
                );
                assert!(matches!(
                    crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                    Err(IoError::ImportNeedsConfirmation(warnings))
                        if warnings == vec![ImportWarning::Other]
                ));
                let sheet = &report.workbook.sheets[0];
                assert!(connectors(sheet).is_empty());
                assert_eq!(shapes(sheet).len(), 1);
                sheet.validate().unwrap();
            }
        }
    }
}

#[test]
fn omitted_point_coordinates_still_default_to_zero() {
    let xml = page(
        r#"<mxCell id="e" style="endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint y="  -3.5  " as="sourcePoint"/><mxPoint x="10" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let sheet = clean(&xml);
    let edge = connectors(&sheet)[0];
    assert_eq!(edge.from, Endpoint::Point(Vec2::new(0.0, -3.5)));
    assert_eq!(edge.to, Endpoint::Point(Vec2::new(10.0, 0.0)));
}

#[test]
fn malformed_points_are_reported_consistently_by_links_and_clipboard() {
    let xml = page(
        r#"<mxCell id="shape" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>
        <mxCell id="e" style="endArrow=none;" edge="1" parent="1" target="shape"><mxGeometry relative="1" as="geometry"><mxPoint x="invalid" y="5" as="sourcePoint"/></mxGeometry></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    assert!(connectors(&report.workbook.sheets[0]).is_empty());

    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert_eq!(linked.warnings, report.warnings);
    assert_eq!(linked.workbook, report.workbook);
    let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
    assert_eq!(pasted.warnings, report.warnings);
}

#[test]
fn malformed_unused_endpoint_points_are_reported_but_attached_edges_survive() {
    for (point, source, target, free_point) in [
        (
            "sourcePoint",
            r#" source="shape""#,
            "",
            r#"<mxPoint x="150" y="12" as="targetPoint"/>"#,
        ),
        (
            "targetPoint",
            "",
            r#" target="shape""#,
            r#"<mxPoint x="150" y="12" as="sourcePoint"/>"#,
        ),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="shape" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>
            <mxCell id="e" style="endArrow=none;" edge="1" parent="1"{source}{target}><mxGeometry relative="1" as="geometry"><mxPoint x="invalid" y="5" as="{point}"/>{free_point}</mxGeometry></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Other]);
        let sheet = &report.workbook.sheets[0];
        let edge = connectors(sheet)[0];
        let attached = Endpoint::Object(shapes(sheet)[0].id);
        let free = Endpoint::Point(Vec2::new(150.0, 12.0));
        if point == "sourcePoint" {
            assert_eq!((edge.from, edge.to), (attached, free));
        } else {
            assert_eq!((edge.from, edge.to), (free, attached));
        }
        sheet.validate().unwrap();
    }
}

#[test]
fn malformed_offsets_and_ink_points_do_not_keep_partial_coordinates() {
    let xml = page(
        r#"<mxCell id="group" style="group" vertex="1" parent="1"><mxGeometry x="100" y="200" width="120" height="80" as="geometry"/></mxCell>
        <mxCell id="child" vertex="1" parent="group"><mxGeometry x="0.5" y="0.5" width="20" height="10" relative="1" as="geometry"><mxPoint x="invalid" y="12" as="offset"/></mxGeometry></mxCell>
        <mxCell id="ink" style="thinodaInk=1;endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="NaN" y="5" as="sourcePoint"/><Array as="points"><mxPoint x="10" y="20"/></Array><mxPoint x="30" y="40" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    let sheet = &report.workbook.sheets[0];
    assert_eq!(shapes(sheet)[0].rect, rect(160.0, 240.0, 20.0, 10.0));
    let ink = sheet
        .canvas
        .objects
        .iter()
        .find_map(|object| match &object.kind {
            CanvasObjectKind::Ink(ink) => Some(ink),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        ink.points.as_ref(),
        &[Vec2::new(10.0, 20.0), Vec2::new(30.0, 40.0)]
    );
    sheet.validate().unwrap();
}

#[test]
fn malformed_vertex_geometry_is_reported_without_fabricating_a_shape() {
    for (index, field) in ["x", "y", "width", "height"].into_iter().enumerate() {
        for value in ["", "  ", "invalid", "NaN", "inf", "1e1000"] {
            let mut numbers = ["10", "20", "40", "50"];
            numbers[index] = value;
            let [x, y, width, height] = numbers;
            let xml = page(&format!(
                r#"<mxCell id="bad" value="Bad" vertex="1" parent="1"><mxGeometry x="{x}" y="{y}" width="{width}" height="{height}" as="geometry"/></mxCell>
                <mxCell id="safe" value="Safe" vertex="1" parent="1"><mxGeometry x="100" y="50" width="40" height="50" as="geometry"/></mxCell>"#
            ));
            let report = load(&xml);
            assert_eq!(
                report.warnings,
                vec![ImportWarning::Other],
                "{field}={value}"
            );
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(warnings))
                    if warnings == vec![ImportWarning::Other]
            ));
            let sheet = &report.workbook.sheets[0];
            assert_eq!(shapes(sheet).len(), 1, "{field}={value}");
            assert_eq!(shapes(sheet)[0].label(), "Safe");
            sheet.validate().unwrap();
        }
    }
}

#[test]
fn malformed_vertex_geometry_is_reported_by_links_and_clipboard() {
    let xml = page(
        r#"<mxCell id="bad" value="Bad" vertex="1" parent="1"><mxGeometry x="invalid" width="40" height="50" as="geometry"/></mxCell>
        <mxCell id="safe" value="Safe" vertex="1" parent="1"><mxGeometry x="100" width="40" height="50" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert_eq!(linked.warnings, report.warnings);
    assert_eq!(linked.workbook, report.workbook);
    let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
    assert_eq!(pasted.warnings, report.warnings);
}

#[test]
fn negative_dimensions_are_rejected_but_zero_sizes_and_negative_positions_remain_valid() {
    for (width, height) in [("-2", "20"), ("20", "-0.5")] {
        let xml = page(&format!(
            r#"<mxCell id="bad" vertex="1" parent="1"><mxGeometry x="-10" y="-20" width="{width}" height="{height}" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Other]);
        assert!(shapes(&report.workbook.sheets[0]).is_empty());
    }
    let sheet = clean(&page(
        r#"<mxCell id="zero" vertex="1" parent="1"><mxGeometry x="-10" y="-20" width="0" height="0" as="geometry"/></mxCell>"#,
    ));
    assert_eq!(shapes(&sheet)[0].rect, rect(-10.0, -20.0, 0.0, 0.0));
}

#[test]
fn invalid_parent_geometry_omits_its_subtree_and_resolves_connectors_conservatively() {
    let cells = r#"<mxCell id="group" style="group" vertex="1" parent="1"><mxGeometry x="invalid" y="200" width="200" height="120" as="geometry"/></mxCell>
        <mxCell id="child" value="Child" vertex="1" parent="group"><mxGeometry x="10" y="20" width="30" height="40" as="geometry"/></mxCell>
        <mxCell id="safe" value="Safe" vertex="1" parent="1"><mxGeometry x="500" y="50" width="40" height="50" as="geometry"/></mxCell>"#;
    for edge_first in [true, false] {
        for fallback in [true, false] {
            let point = if fallback {
                r#"<mxPoint x="7" y="8" as="sourcePoint"/>"#
            } else {
                ""
            };
            let edge = format!(
                r#"<mxCell id="edge" edge="1" source="child" target="safe" parent="1" style="endArrow=none;"><mxGeometry relative="1" as="geometry">{point}</mxGeometry></mxCell>"#
            );
            let xml = if edge_first {
                page(&format!("{edge}{cells}"))
            } else {
                page(&format!("{cells}{edge}"))
            };
            let report = load(&xml);
            assert!(report.warnings.contains(&ImportWarning::Other));
            assert_eq!(
                report.warnings.contains(&ImportWarning::DetachedConnectors),
                fallback
            );
            let sheet = &report.workbook.sheets[0];
            assert_eq!(shapes(sheet).len(), 1);
            assert_eq!(shapes(sheet)[0].label(), "Safe");
            let edges = connectors(sheet);
            assert_eq!(edges.len(), usize::from(fallback));
            if fallback {
                assert_eq!(edges[0].from, Endpoint::Point(Vec2::new(7.0, 8.0)));
                assert_eq!(edges[0].to, Endpoint::Object(shapes(sheet)[0].id));
            }
            sheet.validate().unwrap();
        }
    }
}

#[test]
fn invalid_unused_edge_geometry_warns_without_omitting_attached_connectors_or_labels() {
    let xml = page(
        r#"<mxCell id="a" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>
        <mxCell id="b" vertex="1" parent="1"><mxGeometry x="100" width="40" height="40" as="geometry"/></mxCell>
        <mxCell id="e" edge="1" source="a" target="b" parent="1" style="endArrow=none;fontColor=#000000;"><mxGeometry x="NaN" relative="1" as="geometry"/></mxCell>
        <mxCell id="label" value="Label" vertex="1" parent="e"><mxGeometry x="invalid" relative="1" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(
        report.warnings,
        vec![ImportWarning::Formatting, ImportWarning::Other]
    );
    let sheet = &report.workbook.sheets[0];
    let shapes = shapes(sheet);
    let edge = connectors(sheet)[0];
    assert_eq!(edge.from, Endpoint::Object(shapes[0].id));
    assert_eq!(edge.to, Endpoint::Object(shapes[1].id));
    assert_eq!(edge.label, "Label");
    sheet.validate().unwrap();
}

#[test]
fn curved_waypoints_round_trip_and_orthogonal_hints_report_approximation() {
    let cell = |style: &str| {
        format!(
            r#"<mxCell id="e" style="{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="200" y="0" as="targetPoint"/><Array as="points"><mxPoint x="50" y="80"/><mxPoint x="150" y="80"/></Array></mxGeometry></mxCell>"#
        )
    };
    let curved = clean(&page(&cell("curved=1;edgeStyle=none;endArrow=none;")));
    let connector = connectors(&curved)[0];
    assert_eq!(connector.route, ConnectorRoute::Curve);
    let expected = [Vec2::new(50.0, 80.0), Vec2::new(150.0, 80.0)];
    assert_eq!(connector.waypoints.as_ref(), &expected);
    let restored = roundtrip(&Workbook::new(curved));
    let connector = connectors(&restored.sheets[0])[0];
    assert_eq!(connector.route, ConnectorRoute::Curve);
    assert_eq!(connector.waypoints.as_ref(), &expected);

    let report = load(&page(&cell("edgeStyle=orthogonalEdgeStyle;endArrow=none;")));
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    let connector = connectors(&report.workbook.sheets[0])[0];
    assert_eq!(connector.route, ConnectorRoute::Elbow);
    assert_eq!(connector.waypoints.as_ref(), &expected);
    let bytes = export(&report.workbook).unwrap();
    let restored = import(&bytes).unwrap();
    assert_eq!(restored.warnings, vec![ImportWarning::Formatting]);
    assert_eq!(
        connectors(&restored.workbook.sheets[0])[0]
            .waypoints
            .as_ref(),
        &expected
    );
}

#[test]
fn connector_marker_and_port_approximations_require_confirmation() {
    let edge = |style: &str| {
        page(&format!(
            r#"<mxCell id="e" style="{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ))
    };
    for style in [
        "endArrow=sysMLReqInt;",
        "startArrow=mermaidCircle;",
        "startFill=0;",
        "endArrow=none;endFill=0;",
        "endArrow=sysMLProvInt;endFill=0;",
        "endFill=invalid;",
        "endFill=2;",
        "startSize=12;",
        "endSize=invalid;",
        "endSize=-1;",
        "endSize=1000;",
        "endSize=NaN;",
        "endArrow=none;endSize=12;",
        "exitX=0.25;",
        "exitY=0.5;",
        "exitDx=5;",
        "exitDy=5;",
        "exitPerimeter=0;",
        "entryX=0.75;",
        "entryY=0.5;",
        "entryDx=5;",
        "entryDy=5;",
        "entryPerimeter=0;",
        "sourcePort=port1;",
        "targetPort=port2;",
        "sourcePortConstraint=north;",
        "targetPortConstraint=south;",
        "sourcePerimeterSpacing=5;",
        "targetPerimeterSpacing=5;",
    ] {
        let report = load(&edge(style));
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
        assert_eq!(connectors(&report.workbook.sheets[0]).len(), 1, "{style}");
    }

    for style in [
        "",
        "startArrow=none;endArrow=none;",
        "startArrow=classic;endArrow=classic;",
        "startFill=1;endFill=1;startSize=6;endSize=6;",
        "endFill=0;",
        "startArrow=block;startFill=0;endArrow=open;endFill=0;",
        "startArrow=block;startSize=12.5;endSize=0;",
        "startArrow=diamond;startFill=0;endArrow=diamondThin;endFill=1;",
        "startArrow=oval;startFill=0;endArrow=oval;endFill=1;",
        "startArrow=halfCircle;startFill=0;endArrow=halfCircle;endFill=1;",
        "startArrow=baseDash;startFill=0;endArrow=dash;endFill=1;",
        "portConstraintRotation=0;perimeterSpacing=0;sourcePerimeterSpacing=0;targetPerimeterSpacing=0;",
    ] {
        let report = load(&edge(style));
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        assert_eq!(connectors(&report.workbook.sheets[0]).len(), 1, "{style}");
        let restored = import(&export(&report.workbook).unwrap()).unwrap();
        assert!(
            restored.warnings.is_empty(),
            "{style}: {:?}",
            restored.warnings
        );
    }
}

#[test]
fn marker_fills_round_trip_independently_for_every_supported_shape() {
    let names = [
        "classic",
        "classicThin",
        "open",
        "openThin",
        "block",
        "blockThin",
        "diamond",
        "diamondThin",
        "oval",
        "halfCircle",
        "baseDash",
        "dash",
    ];
    for kind in names {
        for start_fill in [0, 1] {
            for end_fill in [0, 1] {
                let xml = page(&format!(
                    r#"<mxCell id="e" style="startArrow={kind};startSize=12.5;startFill={start_fill};endArrow={kind};endSize=0;endFill={end_fill};" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
                ));
                let report = load(&xml);
                assert!(
                    report.warnings.is_empty(),
                    "{kind}, {start_fill}, {end_fill}: {:?}",
                    report.warnings
                );
                let connector = connectors(&report.workbook.sheets[0])[0];
                assert_eq!(connector.start_marker_unfilled, start_fill == 0);
                assert_eq!(connector.end_marker_unfilled, end_fill == 0);
                assert_eq!(connector.start_marker_size, Some(12.5));
                assert_eq!(connector.end_marker_size, Some(0.0));
                assert_eq!(
                    feature_version(&report.workbook),
                    if matches!(kind, "baseDash" | "dash") {
                        18
                    } else if kind == "halfCircle" {
                        17
                    } else if kind == "oval" {
                        16
                    } else if kind.starts_with("diamond") {
                        15
                    } else if start_fill == 0 || end_fill == 0 {
                        14
                    } else {
                        13
                    }
                );
                let exported = export(&report.workbook).unwrap();
                let restored = import(&exported).unwrap();
                assert!(
                    restored.warnings.is_empty(),
                    "{kind}: {:?}",
                    restored.warnings
                );
                let restored = connectors(&restored.workbook.sheets[0])[0];
                assert_eq!(
                    restored.start_marker_unfilled,
                    connector.start_marker_unfilled
                );
                assert_eq!(restored.end_marker_unfilled, connector.end_marker_unfilled);
                assert_eq!(restored.start_marker_size, connector.start_marker_size);
                assert_eq!(restored.end_marker_size, connector.end_marker_size);
                assert_eq!(restored.start_marker, connector.start_marker);
                assert_eq!(restored.end_marker, connector.end_marker);
            }
        }
    }
}

#[test]
fn custom_marker_sizes_round_trip_for_every_supported_shape() {
    let names = [
        "classic",
        "classicThin",
        "open",
        "openThin",
        "block",
        "blockThin",
        "diamond",
        "diamondThin",
        "oval",
        "halfCircle",
        "baseDash",
        "dash",
    ];
    for kind in names {
        for size in [0.0, 6.0, 12.5, 999.0] {
            let xml = page(&format!(
                r#"<mxCell id="e" style="startArrow={kind};startSize={size};endArrow={kind};endSize=12.5;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
            ));
            let report = load(&xml);
            assert!(
                report.warnings.is_empty(),
                "{kind}, {size}: {:?}",
                report.warnings
            );
            let connector = connectors(&report.workbook.sheets[0])[0];
            assert_eq!(connector.start_marker_size, (size != 6.0).then_some(size));
            assert_eq!(connector.end_marker_size, Some(12.5));
            assert_eq!(
                feature_version(&report.workbook),
                if matches!(kind, "baseDash" | "dash") {
                    18
                } else if kind == "halfCircle" {
                    17
                } else if kind == "oval" {
                    16
                } else if kind.starts_with("diamond") {
                    15
                } else {
                    13
                }
            );
            let exported = export(&report.workbook).unwrap();
            let restored = import(&exported).unwrap();
            assert!(
                restored.warnings.is_empty(),
                "{kind}, {size}: {:?}",
                restored.warnings
            );
            let restored = connectors(&restored.workbook.sheets[0])[0];
            assert_eq!(restored.start_marker_size, connector.start_marker_size);
            assert_eq!(restored.end_marker_size, connector.end_marker_size);
            assert_eq!(restored.start_marker, connector.start_marker);
            assert_eq!(restored.end_marker, connector.end_marker);
        }
    }
}

#[test]
fn supported_connector_markers_round_trip_with_both_ends() {
    let marker = |name| match name {
        "classic" => Some(ConnectorMarkerKind::Classic),
        "classicThin" => Some(ConnectorMarkerKind::ClassicThin),
        "open" => Some(ConnectorMarkerKind::Open),
        "openThin" => Some(ConnectorMarkerKind::OpenThin),
        "block" => Some(ConnectorMarkerKind::Block),
        "blockThin" => Some(ConnectorMarkerKind::BlockThin),
        "diamond" => Some(ConnectorMarkerKind::Diamond),
        "diamondThin" => Some(ConnectorMarkerKind::DiamondThin),
        "oval" => Some(ConnectorMarkerKind::Oval),
        "halfCircle" => Some(ConnectorMarkerKind::HalfCircle),
        "baseDash" => Some(ConnectorMarkerKind::BaseDash),
        "dash" => Some(ConnectorMarkerKind::Dash),
        _ => None,
    };
    let names = [
        "none",
        "classic",
        "classicThin",
        "open",
        "openThin",
        "block",
        "blockThin",
        "diamond",
        "diamondThin",
        "oval",
        "halfCircle",
        "baseDash",
        "dash",
    ];
    for start in names {
        for end in names {
            let xml = page(&format!(
                r#"<mxCell id="e" style="startArrow={start};endArrow={end};startSize=6;endSize=6;startFill=1;endFill=1;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/><Array as="points"><mxPoint x="50" y="20"/></Array></mxGeometry></mxCell>"#
            ));
            let report = load(&xml);
            assert!(
                report.warnings.is_empty(),
                "{start}, {end}: {:?}",
                report.warnings
            );
            let connector = connectors(&report.workbook.sheets[0])[0];
            assert_eq!(connector.arrow_start, start != "none");
            assert_eq!(connector.arrow_end, end != "none");
            assert_eq!(
                connector.start_marker.map(|value| value.kind),
                marker(start)
            );
            assert_eq!(connector.end_marker.map(|value| value.kind), marker(end));
            let native_version = feature_version(&report.workbook);
            assert_eq!(
                native_version,
                if [marker(start), marker(end)]
                    .into_iter()
                    .flatten()
                    .any(ConnectorMarkerKind::is_bar)
                {
                    18
                } else if [marker(start), marker(end)]
                    .into_iter()
                    .flatten()
                    .any(|kind| kind == ConnectorMarkerKind::HalfCircle)
                {
                    17
                } else if [marker(start), marker(end)]
                    .into_iter()
                    .flatten()
                    .any(|kind| kind == ConnectorMarkerKind::Oval)
                {
                    16
                } else if [marker(start), marker(end)]
                    .into_iter()
                    .flatten()
                    .any(ConnectorMarkerKind::is_diamond)
                {
                    15
                } else if [marker(start), marker(end)]
                    .into_iter()
                    .flatten()
                    .any(ConnectorMarkerKind::requires_extended_format)
                {
                    12
                } else if marker(start).is_some() || marker(end).is_some() {
                    11
                } else {
                    10
                }
            );
            let exported = export(&report.workbook).unwrap();
            let exported_text = std::str::from_utf8(&exported).unwrap();
            assert!(exported_text.contains(&format!("endArrow={end};")));
            if start != "none" {
                assert!(exported_text.contains(&format!("startArrow={start};")));
            }
            let restored = import(&exported).unwrap();
            assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
            let restored = connectors(&restored.workbook.sheets[0])[0];
            assert_eq!(restored.start_marker.map(|value| value.kind), marker(start));
            assert_eq!(restored.end_marker.map(|value| value.kind), marker(end));
            assert_eq!(restored.arrow_start, start != "none");
            assert_eq!(restored.arrow_end, end != "none");
        }
    }
}

#[test]
fn drawio_default_classic_and_legacy_triangle_are_distinct() {
    let cell = |style: &str| {
        page(&format!(
            r#"<mxCell id="e" style="{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ))
    };
    for style in ["", "defaultEdge", "endArrow=classic;"] {
        let report = load(&cell(style));
        let connector = connectors(&report.workbook.sheets[0])[0];
        assert_eq!(
            connector.end_marker.unwrap().kind,
            ConnectorMarkerKind::Classic,
            "{style}"
        );
        assert_eq!(feature_version(&report.workbook), 12);
    }
    let no_default = load(&cell(";"));
    let connector = connectors(&no_default.workbook.sheets[0])[0];
    assert!(!connector.arrow_end);
    assert!(connector.end_marker.is_none());

    let mut sheet = Sheet::new("Legacy", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.arrow_start = true;
    sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let book = Workbook::new(sheet);
    assert_eq!(
        export_losses(&book, Format::DrawIo, &ExportOptions::default()),
        vec![ExportLoss::LegacyConnectorMarkersApproximated(2)]
    );
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(xml.contains("thinodaLegacyStartArrow=1;"));
    assert!(xml.contains("thinodaLegacyEndArrow=1;"));
    let restored = load(&xml);
    let connector = connectors(&restored.workbook.sheets[0])[0];
    assert!(connector.arrow_start && connector.arrow_end);
    assert!(connector.start_marker.is_none() && connector.end_marker.is_none());

    let changed = xml.replace("endArrow=classic;", "endArrow=block;");
    let restored = load(&changed);
    let connector = connectors(&restored.workbook.sheets[0])[0];
    assert_eq!(
        connector.end_marker.unwrap().kind,
        ConnectorMarkerKind::Block
    );
    let resized = xml.replace(
        "thinodaLegacyEndArrow=1;",
        "thinodaLegacyEndArrow=1;endSize=12;",
    );
    let restored = load(&resized);
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    let connector = connectors(&restored.workbook.sheets[0])[0];
    assert_eq!(
        connector.end_marker.unwrap().kind,
        ConnectorMarkerKind::Classic
    );
    assert_eq!(connector.end_marker_size, Some(12.0));
}

#[test]
fn supported_connector_markers_survive_link_and_clipboard_conversion() {
    for (start, end, start_kind, end_kind) in [
        (
            "block",
            "open",
            ConnectorMarkerKind::Block,
            ConnectorMarkerKind::Open,
        ),
        (
            "halfCircle",
            "oval",
            ConnectorMarkerKind::HalfCircle,
            ConnectorMarkerKind::Oval,
        ),
        (
            "baseDash",
            "dash",
            ConnectorMarkerKind::BaseDash,
            ConnectorMarkerKind::Dash,
        ),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="e" style="startArrow={start};startSize=12.5;startFill=0;endArrow={end};endSize=0;endFill=0;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ));
        let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
        let linked = crate::import_drawio_link(&link).unwrap();
        assert!(linked.warnings.is_empty(), "{:?}", linked.warnings);
        let connector = connectors(&linked.workbook.sheets[0])[0];
        assert_eq!(connector.start_marker.unwrap().kind, start_kind);
        assert_eq!(connector.end_marker.unwrap().kind, end_kind);
        assert_eq!(connector.start_marker_size, Some(12.5));
        assert_eq!(connector.end_marker_size, Some(0.0));
        assert!(connector.start_marker_unfilled);
        assert!(connector.end_marker_unfilled);

        let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
        assert!(pasted.warnings.is_empty(), "{:?}", pasted.warnings);
        let assert_markers = |payload: &thinoda_core::clipboard::BoardClipboardPayload| {
            let mut sheet = Sheet::new("Paste", "");
            sheet.root_mode = SheetRootMode::Hidden;
            thinoda_core::clipboard::paste_board_selection(&mut sheet, payload, Vec2::ZERO)
                .unwrap();
            let connector = connectors(&sheet)[0];
            assert_eq!(connector.start_marker.unwrap().kind, start_kind);
            assert_eq!(connector.end_marker.unwrap().kind, end_kind);
            assert_eq!(connector.start_marker_size, Some(12.5));
            assert_eq!(connector.end_marker_size, Some(0.0));
            assert!(connector.start_marker_unfilled);
            assert!(connector.end_marker_unfilled);
        };
        assert_markers(&pasted.payload);
        let copy = crate::drawio_clipboard::encode(&pasted.payload).unwrap();
        let restored = crate::drawio_clipboard::decode(&copy).unwrap();
        assert_markers(&restored.payload);
    }
}

#[test]
fn editable_drawio_images_preserve_supported_connector_markers() {
    for (start, end, start_kind, end_kind, start_unfilled, end_unfilled) in [
        (
            "block",
            "open",
            ConnectorMarkerKind::Block,
            ConnectorMarkerKind::Open,
            true,
            false,
        ),
        (
            "classicThin",
            "openThin",
            ConnectorMarkerKind::ClassicThin,
            ConnectorMarkerKind::OpenThin,
            false,
            true,
        ),
        (
            "diamond",
            "diamondThin",
            ConnectorMarkerKind::Diamond,
            ConnectorMarkerKind::DiamondThin,
            true,
            false,
        ),
        (
            "oval",
            "oval",
            ConnectorMarkerKind::Oval,
            ConnectorMarkerKind::Oval,
            true,
            false,
        ),
        (
            "halfCircle",
            "oval",
            ConnectorMarkerKind::HalfCircle,
            ConnectorMarkerKind::Oval,
            true,
            false,
        ),
        (
            "baseDash",
            "dash",
            ConnectorMarkerKind::BaseDash,
            ConnectorMarkerKind::Dash,
            true,
            false,
        ),
    ] {
        let report = load(&page(&format!(
            r#"<mxCell id="e" style="startArrow={start};startSize=12.5;startFill={};endArrow={end};endSize=0;endFill={};" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#,
            u8::from(!start_unfilled),
            u8::from(!end_unfilled),
        )));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        for format in [Format::DrawIoSvg, Format::DrawIoPng] {
            let bytes = export_bytes(&report.workbook, format, &ExportOptions::default()).unwrap();
            let restored = import(&bytes).unwrap();
            assert!(
                restored.warnings.is_empty(),
                "{format:?}: {:?}",
                restored.warnings
            );
            let connector = connectors(&restored.workbook.sheets[0])[0];
            assert_eq!(connector.start_marker.unwrap().kind, start_kind);
            assert_eq!(connector.end_marker.unwrap().kind, end_kind);
            assert_eq!(connector.start_marker_size, Some(12.5));
            assert_eq!(connector.end_marker_size, Some(0.0));
            assert_eq!(connector.start_marker_unfilled, start_unfilled);
            assert_eq!(connector.end_marker_unfilled, end_unfilled);
        }
    }
}

#[test]
fn shape_connection_constraints_report_approximation() {
    for style in [
        "portConstraint=north;",
        "portConstraintRotation=1;",
        "perimeterSpacing=5;",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="v" style="{style}" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
        assert_eq!(shapes(&report.workbook.sheets[0]).len(), 1);
    }
}

#[test]
fn empty_arrow_names_disable_the_corresponding_marker() {
    let xml = page(
        r#"<mxCell id="e" style="startArrow=;endArrow=;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let report = load(&xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let connector = connectors(&report.workbook.sheets[0])[0];
    assert!(!connector.arrow_start);
    assert!(!connector.arrow_end);
    let restored = import(&export(&report.workbook).unwrap()).unwrap();
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    assert!(!connectors(&restored.workbook.sheets[0])[0].arrow_end);
}

#[test]
fn edge_default_suppression_and_named_default_control_the_end_marker() {
    for (style, arrow) in [
        ("", true),
        (";edgeStyle=none;", false),
        (";endArrow=classic;", true),
        (";defaultEdge;", true),
        (";defaultEdge;endArrow=none;", false),
        (";endArrow=none;defaultEdge;", true),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="e" style="{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ));
        let report = load(&xml);
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        assert_eq!(
            connectors(&report.workbook.sheets[0])[0].arrow_end,
            arrow,
            "{style}"
        );
        let restored = import(&export(&report.workbook).unwrap()).unwrap();
        assert!(
            restored.warnings.is_empty(),
            "{style}: {:?}",
            restored.warnings
        );
        assert_eq!(
            connectors(&restored.workbook.sheets[0])[0].arrow_end,
            arrow,
            "{style}"
        );
    }
}

#[test]
fn attached_fixed_connector_ports_survive_file_link_and_clipboard_conversion() {
    let xml = page(
        r#"<mxCell id="a" vertex="1" parent="1"><mxGeometry x="10" y="20" width="60" height="40" as="geometry"/></mxCell>
        <mxCell id="b" vertex="1" parent="1"><mxGeometry x="200" y="20" width="60" height="40" as="geometry"/></mxCell>
        <mxCell id="e" style="endArrow=open;endSize=14;exitX=1;exitY=0.5;entryX=0;entryY=0.25;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    report.workbook.validate().unwrap();
    let connector = connectors(&report.workbook.sheets[0])[0];
    assert!(matches!(connector.from, Endpoint::Object(_)));
    assert!(matches!(connector.to, Endpoint::Object(_)));
    assert!(connector.arrow_end);
    assert_eq!(
        connector.from_port,
        Some(ConnectorPort {
            position: Vec2::new(1.0, 0.5),
            offset: Vec2::ZERO,
            perimeter: true,
        })
    );
    assert_eq!(
        connector.to_port,
        Some(ConnectorPort {
            position: Vec2::new(0.0, 0.25),
            offset: Vec2::ZERO,
            perimeter: true,
        })
    );
    assert_eq!(feature_version(&report.workbook), 22);
    let paste = crate::drawio_clipboard::decode(&xml).unwrap();
    assert!(paste.warnings.is_empty(), "{:?}", paste.warnings);
    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert!(linked.warnings.is_empty(), "{:?}", linked.warnings);
    assert_eq!(
        connectors(&linked.workbook.sheets[0])[0].from_port,
        connector.from_port
    );

    let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    assert!(exported.contains("endArrow=open;"));
    assert!(exported.contains("exitX=1;exitY=0.5;"));
    assert!(exported.contains("entryX=0;entryY=0.25;"));
    let restored = import(exported.as_bytes()).unwrap();
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    assert_eq!(
        connectors(&restored.workbook.sheets[0])[0].from_port,
        connector.from_port
    );
    assert_eq!(
        connectors(&restored.workbook.sheets[0])[0].to_port,
        connector.to_port
    );
}

#[test]
fn default_connection_points_round_trip_for_supported_shape_families() {
    let rectangle = [
        (0.0, 0.0),
        (0.25, 0.0),
        (0.5, 0.0),
        (0.75, 0.0),
        (1.0, 0.0),
        (1.0, 0.25),
        (1.0, 0.5),
        (1.0, 0.75),
        (1.0, 1.0),
        (0.75, 1.0),
        (0.5, 1.0),
        (0.25, 1.0),
        (0.0, 1.0),
        (0.0, 0.75),
        (0.0, 0.5),
        (0.0, 0.25),
    ];
    let perimeter = [
        (0.0, 0.0),
        (0.5, 0.0),
        (1.0, 0.0),
        (1.0, 0.5),
        (1.0, 1.0),
        (0.5, 1.0),
        (0.0, 1.0),
        (0.0, 0.5),
    ];
    let mut sheet = Sheet::new("Points", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let target = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(700.0, 0.0), Vec2::new(120.0, 80.0)),
        ))
        .unwrap();
    let mut expected = Vec::new();
    for (index, (kind, positions)) in [
        (ShapeKind::Rectangle, &rectangle[..]),
        (ShapeKind::RoundedRectangle, &rectangle[..]),
        (ShapeKind::Ellipse, &perimeter[..]),
        (ShapeKind::DoubleEllipse, &perimeter[..]),
        (ShapeKind::Diamond, &perimeter[..]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut object = CanvasObject::shape(
            kind,
            Rect::from_min_size(Vec2::new(index as f32 * 160.0, 0.0), Vec2::new(120.0, 80.0)),
        );
        if kind == ShapeKind::DoubleEllipse {
            object.style.fill = Some(Color::TRANSPARENT);
            object.style.stroke = Some(Color::TRANSPARENT);
        }
        let source = sheet.add_canvas_object(object).unwrap();
        for &(x, y) in positions {
            let port = ConnectorPort {
                position: Vec2::new(x, y),
                offset: Vec2::ZERO,
                perimeter: true,
            };
            let mut connector =
                CanvasConnector::new(Endpoint::Object(source), Endpoint::Object(target));
            connector.arrow_end = false;
            connector.from_port = Some(port);
            connector.to_port = Some(ConnectorPort {
                position: Vec2::new(0.0, 0.5),
                offset: Vec2::ZERO,
                perimeter: true,
            });
            sheet
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            expected.push((kind, port));
        }
    }
    let original = Workbook::new(sheet);
    original.validate().unwrap();
    let bytes = export(&original).unwrap();
    let restored = import(&bytes).unwrap();
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    let sheet = &restored.workbook.sheets[0];
    for connector in connectors(sheet) {
        let Endpoint::Object(source) = connector.from else {
            panic!("fixed source was detached");
        };
        let CanvasObjectKind::Shape { shape, .. } = sheet.canvas.object(source).unwrap().kind
        else {
            panic!("fixed source is not a shape");
        };
        let entry = (shape, connector.from_port.unwrap());
        let index = expected
            .iter()
            .position(|candidate| *candidate == entry)
            .expect("unexpected or changed default connection point");
        expected.swap_remove(index);
        assert_eq!(
            connector.to_port,
            Some(ConnectorPort {
                position: Vec2::new(0.0, 0.5),
                offset: Vec2::ZERO,
                perimeter: true,
            })
        );
    }
    assert!(
        expected.is_empty(),
        "missing connection points: {expected:?}"
    );
}

#[test]
fn fixed_connector_ports_on_collapsed_topics_require_consent_and_keep_visible_points() {
    let mut sheet = Sheet::new("Collapsed", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let parent = sheet.add_floating("Parent", Vec2::new(200.0, 100.0));
    let child = sheet.add_child(parent, None, "Hidden child");
    sheet.topic_mut(parent).unwrap().collapsed = true;
    let mut connector = CanvasConnector::new(Endpoint::Topic(child), Endpoint::Topic(child));
    connector.arrow_end = false;
    connector.from_port = Some(ConnectorPort {
        position: Vec2::new(0.75, 0.25),
        offset: Vec2::new(3.0, -2.0),
        perimeter: false,
    });
    connector.to_port = Some(ConnectorPort {
        position: Vec2::new(0.25, 0.75),
        offset: Vec2::ZERO,
        perimeter: true,
    });
    sheet
        .add_canvas_object(CanvasObject::connector(connector.clone()))
        .unwrap();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let original = thinoda_core::scene::connector_path(&sheet, &layout, &connector);
    assert!(original.len() >= 2);
    let expected = [original[0], *original.last().unwrap()];
    let book = Workbook::new(sheet);
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        let losses = export_losses(&book, format, &ExportOptions::default());
        assert!(
            losses.contains(&ExportLoss::FixedConnectorAttachmentsDetached(2)),
            "{format:?}: {losses:?}"
        );
        assert!(matches!(
            export_bytes(&book, format, &ExportOptions::default()),
            Err(IoError::DrawioFixedConnectorAttachmentsNeedConfirmation(ref losses))
                if losses == &[ExportLoss::FixedConnectorAttachmentsDetached(2)]
        ));
        let bytes =
            crate::export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                .unwrap();
        let restored = import(&bytes).unwrap().workbook;
        let edge = connectors(&restored.sheets[0])[0];
        for (end, point) in [edge.from, edge.to].into_iter().zip(expected) {
            let Endpoint::Point(actual) = end else {
                panic!("{format:?}: expected a detached point, got {end:?}");
            };
            assert!(
                (actual - point).length() < 0.01,
                "{format:?}: {actual:?} != {point:?}"
            );
        }
        assert_eq!(edge.from_port, None);
        assert_eq!(edge.to_port, None);
    }

    let mut combined = book.clone();
    let target = combined
        .active_mut()
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        )))
        .unwrap();
    combined
        .active_mut()
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(target),
            Endpoint::Point(Vec2::new(200.0, 0.0)),
        )))
        .unwrap();
    assert!(matches!(
        export_bytes(&combined, Format::DrawIo, &ExportOptions::default()),
        Err(IoError::DrawioFixedConnectorAttachmentsNeedConfirmation(ref losses))
            if losses == &[
                ExportLoss::ConnectorAttachmentsDetached(1),
                ExportLoss::FixedConnectorAttachmentsDetached(2),
            ]
    ));

    let mut visible = book;
    visible.active_mut().topic_mut(parent).unwrap().collapsed = false;
    assert!(
        !export_losses(&visible, Format::DrawIo, &ExportOptions::default())
            .iter()
            .any(|loss| matches!(loss, ExportLoss::FixedConnectorAttachmentsDetached(_)))
    );
}

#[test]
fn fixed_connector_port_offsets_and_invalid_constraints_are_disclosed() {
    let cells = |style: &str, source: &str| {
        page(&format!(
            r#"<mxCell id="a" vertex="1" parent="1"><mxGeometry x="10" y="20" width="60" height="40" as="geometry"/></mxCell>
            <mxCell id="e" style="endArrow=none;{style}" edge="1" {source} parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="200" y="20" as="targetPoint"/></mxGeometry></mxCell>"#
        ))
    };
    let xml = cells(
        "exitX=0.25;exitY=0.75;exitDx=5;exitDy=-3;exitPerimeter=0;",
        r#"source="a""#,
    );
    let report = load(&xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let expected = ConnectorPort {
        position: Vec2::new(0.25, 0.75),
        offset: Vec2::new(5.0, -3.0),
        perimeter: false,
    };
    assert_eq!(
        connectors(&report.workbook.sheets[0])[0].from_port,
        Some(expected)
    );
    let restored = import(&export(&report.workbook).unwrap()).unwrap();
    assert_eq!(
        connectors(&restored.workbook.sheets[0])[0].from_port,
        Some(expected)
    );

    for (style, source) in [
        ("exitX=0.25;", r#"source="a""#),
        ("exitX=1.1;exitY=0.5;", r#"source="a""#),
        ("exitX=0.5;exitY=NaN;", r#"source="a""#),
        ("exitX=0.5;exitY=0.5;exitDx=Infinity;", r#"source="a""#),
        ("exitX=0.5;exitY=0.5;exitPerimeter=2;", r#"source="a""#),
        ("exitX=0.5;exitY=0.5;", ""),
    ] {
        let report = load(&cells(style, source));
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
        assert_eq!(connectors(&report.workbook.sheets[0])[0].from_port, None);
        assert_ne!(feature_version(&report.workbook), 22);
    }
}

#[test]
fn fixed_connector_port_perimeter_and_orientation_differences_are_disclosed() {
    let diagram = |target_style: &str, perimeter: bool, source: bool| {
        let (terminal, prefix) = if source {
            (r#"source="a""#, "exit")
        } else {
            (r#"target="a""#, "entry")
        };
        page(&format!(
            r#"<mxCell id="a" style="{target_style}" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="e" style="endArrow=none;{prefix}X=1;{prefix}Y=1;{prefix}Perimeter={};" edge="1" {terminal} parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="200" y="20" as="targetPoint"/></mxGeometry></mxCell>"#,
            u8::from(perimeter)
        ))
    };

    for (target_style, perimeter, warned) in [
        ("rounded=1;", true, false),
        ("ellipse;", true, false),
        ("rhombus;", true, false),
        ("shape=ellipse;perimeter=ellipsePerimeter;", true, false),
        ("perimeter=rectanglePerimeter;ellipse;", true, false),
        ("shape=ellipse;", true, true),
        ("ellipse;perimeter=rectanglePerimeter;", true, true),
        (";shape=ellipse;", true, true),
        ("shape=mxgraph.basic.star;", true, true),
        ("shape=mxgraph.basic.star;", false, false),
        ("shape=waypoint;", false, true),
        ("triangle;direction=north;", false, true),
        ("ellipse;flipH=1;", false, false),
    ] {
        for source in [true, false] {
            let xml = diagram(target_style, perimeter, source);
            let report = load(&xml);
            assert_eq!(
                report.warnings.contains(&ImportWarning::Formatting),
                warned,
                "{target_style} perimeter={perimeter} source={source}: {:?}",
                report.warnings
            );
            let connector = connectors(&report.workbook.sheets[0])[0];
            assert!(
                if source {
                    connector.from_port.is_some()
                } else {
                    connector.to_port.is_some()
                },
                "{target_style} perimeter={perimeter} source={source}"
            );
        }
    }

    let xml = diagram("shape=ellipse;", true, true);
    let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
    assert!(pasted.warnings.contains(&ImportWarning::Formatting));
    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert!(linked.warnings.contains(&ImportWarning::Formatting));
}

#[test]
fn fixed_connector_port_target_geometry_changes_are_disclosed() {
    let png = b64(&tiny_png(40, 20));
    for (target_style, width, height, warned) in [
        ("shape=waypoint;".to_owned(), 20, 20, true),
        ("shape=note;thinodaCard=text;".to_owned(), 10, 4, true),
        ("shape=note;thinodaCard=text;".to_owned(), 100, 60, false),
        (
            format!("shape=image;image=data:image/png,{png};"),
            80,
            80,
            true,
        ),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="a" style="{target_style}" vertex="1" parent="1"><mxGeometry x="10" y="20" width="{width}" height="{height}" as="geometry"/></mxCell>
            <mxCell id="e" style="endArrow=none;exitX=1;exitY=1;exitPerimeter=0;" edge="1" source="a" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="200" y="20" as="targetPoint"/></mxGeometry></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            warned,
            "{target_style}: {:?}",
            report.warnings
        );
        assert!(
            connectors(&report.workbook.sheets[0])[0]
                .from_port
                .is_some()
        );
        report.workbook.validate().unwrap();
    }
}

#[test]
fn fixed_connector_port_export_discloses_position_changes_and_requires_consent() {
    let mut sheet = Sheet::new("Ports", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let port = |perimeter| ConnectorPort {
        position: Vec2::new(0.25, 0.75),
        offset: Vec2::ZERO,
        perimeter,
    };
    for (index, (kind, perimeter)) in [
        (ShapeKind::Rectangle, true),
        (ShapeKind::RoundedRectangle, true),
        (ShapeKind::Ellipse, true),
        (ShapeKind::Diamond, true),
        (ShapeKind::Star, true),
        (ShapeKind::DoubleEllipse, true),
        (ShapeKind::Triangle, false),
        (ShapeKind::Triangle, true),
        (ShapeKind::Star, false),
    ]
    .into_iter()
    .enumerate()
    {
        let target = sheet
            .add_canvas_object(CanvasObject::shape(
                kind,
                Rect::from_min_size(Vec2::new(index as f32 * 120.0, 0.0), Vec2::new(100.0, 60.0)),
            ))
            .unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Object(target),
            Endpoint::Point(Vec2::new(100.0, 200.0)),
        );
        connector.arrow_end = false;
        connector.from_port = Some(port(perimeter));
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
    }
    for (index, (shape, perimeter)) in [
        (TopicShape::Ellipse, true),
        (TopicShape::Diamond, true),
        (TopicShape::Rect, true),
        (TopicShape::Ellipse, false),
    ]
    .into_iter()
    .enumerate()
    {
        let target = sheet.add_floating("Topic", Vec2::new(index as f32 * 200.0, 300.0));
        sheet.topic_mut(target).unwrap().style.shape = Some(shape);
        let mut connector = CanvasConnector::new(
            Endpoint::Topic(target),
            Endpoint::Point(Vec2::new(100.0, 500.0)),
        );
        connector.arrow_end = false;
        connector.from_port = Some(port(perimeter));
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
    }
    let book = Workbook::new(sheet);
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        let losses = export_losses(&book, format, &ExportOptions::default());
        assert!(
            losses.contains(&ExportLoss::DrawioFixedConnectorPositionApproximated(5)),
            "{format:?}: {losses:?}"
        );
        assert!(matches!(
            export_bytes(&book, format, &ExportOptions::default()),
            Err(IoError::DrawioFixedConnectorAttachmentsNeedConfirmation(ref losses))
                if losses == &[ExportLoss::DrawioFixedConnectorPositionApproximated(5)]
        ));
        let bytes =
            crate::export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                .unwrap();
        assert!(!bytes.is_empty(), "{format:?}");
    }
    assert!(
        !export_losses(&book, Format::Thinoda, &ExportOptions::default())
            .iter()
            .any(|loss| matches!(
                loss,
                ExportLoss::DrawioFixedConnectorPositionApproximated(_)
            ))
    );
}

#[test]
fn ink_edges_report_unsupported_connector_appearance() {
    let edge = |style: &str| {
        page(&format!(
            r#"<mxCell id="e" style="thinodaInk=1;{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ))
    };
    for style in [
        "",
        "endArrow=open;",
        "endArrow=classic;",
        "endArrow=none;startArrow=classic;",
        "endArrow=none;exitX=0.5;",
    ] {
        let report = load(&edge(style));
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
    }
    let report = load(&edge("endArrow=none;"));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let report = load(&edge("endArrow=;"));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let suppressed = page(
        r#"<mxCell id="e" style=";thinodaInk=1;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let report = load(&suppressed);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn excessive_connector_waypoints_are_reported() {
    let points =
        r#"<mxPoint x="10" y="20"/>"#.repeat(thinoda_core::board::MAX_CONNECTOR_WAYPOINTS + 1);
    let xml = page(&format!(
        r#"<mxCell id="e" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/><Array as="points">{points}</Array></mxGeometry></mxCell>"#
    ));
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    assert!(connectors(&report.workbook.sheets[0]).is_empty());
}

#[test]
fn waypoint_connector_attachments_export_at_path_midpoint_with_loss_warning() {
    let mut sheet = Sheet::new("Attachments", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut target = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(250.0, 100.0)),
    );
    target.waypoints = vec![Vec2::new(200.0, 0.0), Vec2::new(200.0, 100.0)].into();
    target.label = "Target".into();
    let target_id = sheet
        .add_canvas_object(CanvasObject::connector(target))
        .unwrap();
    let mut dependent = CanvasConnector::new(
        Endpoint::Object(target_id),
        Endpoint::Point(Vec2::new(300.0, 200.0)),
    );
    dependent.label = "Dependent".into();
    sheet
        .add_canvas_object(CanvasObject::connector(dependent))
        .unwrap();
    let book = Workbook::new(sheet);
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![
                ExportLoss::ConnectorAttachmentsDetached(1),
                ExportLoss::LegacyConnectorMarkersApproximated(2),
                ExportLoss::DrawioLabelAppearanceApproximated(2),
            ]
        );
        assert!(matches!(
            export_bytes(&book, format, &ExportOptions::default()),
            Err(IoError::DrawioConnectorAttachmentsNeedConfirmation(1))
        ));
        let bytes =
            crate::export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                .unwrap();
        let restored = import(&bytes).unwrap().workbook;
        let dependent = connectors(&restored.sheets[0])
            .into_iter()
            .find(|connector| connector.label == "Dependent")
            .unwrap();
        assert_eq!(dependent.from, Endpoint::Point(Vec2::new(175.0, 0.0)));
    }
}

#[test]
fn nested_geometry_groups_containers_and_edge_labels() {
    let xml = page(
        r#"
        <mxCell id="g" style="group" vertex="1" connectable="0" parent="1"><mxGeometry x="100" y="100" width="200" height="100" as="geometry"/></mxCell>
        <mxCell id="a" value="A" vertex="1" parent="g"><mxGeometry x="0" y="0" width="50" height="50" as="geometry"/></mxCell>
        <mxCell id="b" value="B" style="ellipse;" vertex="1" parent="g"><mxGeometry x="150" y="50" width="50" height="50" as="geometry"/></mxCell>
        <mxCell id="e" value="" style="endArrow=none;" edge="1" parent="1" source="a" target="later"><mxGeometry relative="1" as="geometry"/></mxCell>
        <mxCell id="l" value="edge label" style="edgeLabel;" vertex="1" connectable="0" parent="e"><mxGeometry x="-0.2" relative="1" as="geometry"/></mxCell>
        <mxCell id="lane" value="&lt;b&gt;&lt;/b&gt;Pool&lt;br&gt;" style="swimlane;html=1;startSize=20;" vertex="1" parent="1"><mxGeometry x="500" y="0" width="300" height="200" as="geometry"/></mxCell>
        <mxCell id="later" value="Inside" vertex="1" parent="lane"><mxGeometry x="20" y="40" width="60" height="30" as="geometry"/></mxCell>
        <mxCell id="pin" value="pin" style="ellipse;" vertex="1" parent="later"><mxGeometry x="1" y="0.5" width="10" height="10" relative="1" as="geometry"><mxPoint x="-5" y="-5" as="offset"/></mxGeometry></mxCell>
        <mxCell id="free" edge="1" parent="lane"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="10" y="10" as="targetPoint"/></mxGeometry></mxCell>
        "#,
    );
    let report = load(&xml);
    // The formatting and non-connectable group cannot both be preserved.
    assert_eq!(
        report.warnings,
        vec![ImportWarning::Formatting, ImportWarning::Other]
    );
    let sheet = &report.workbook.sheets[0];
    let find = |label: &str| {
        shapes(sheet)
            .into_iter()
            .find(|o| o.label() == label)
            .unwrap_or_else(|| panic!("{label}"))
    };
    assert_eq!(find("A").rect, rect(100.0, 100.0, 50.0, 50.0));
    assert_eq!(find("B").rect, rect(250.0, 150.0, 50.0, 50.0));
    assert_eq!(find("Inside").rect, rect(520.0, 40.0, 60.0, 30.0));
    assert_eq!(find("pin").rect, rect(575.0, 50.0, 10.0, 10.0));
    let a = find("A").id;
    let b = find("B").id;
    let inside = find("Inside").id;
    let pin = find("pin").id;
    assert_eq!(sheet.canvas.groups.len(), 2);
    let groups: Vec<Vec<BoardMember>> = sheet
        .canvas
        .groups
        .iter()
        .map(|g| sheet.board_group_members(g.id))
        .collect();
    assert!(groups.contains(&vec![BoardMember::Object(a), BoardMember::Object(b)]));
    assert!(groups.contains(&vec![BoardMember::Object(inside), BoardMember::Object(pin)]));
    let zone = &sheet.zones[0];
    assert_eq!(zone.name, "Pool");
    assert_eq!(zone.objects.len(), 3, "inside, pin and the free edge");
    let links = connectors(sheet);
    let deferred = links
        .iter()
        .find(|c| {
            c.extra_labels
                .first()
                .is_some_and(|l| l.text == "edge label")
        })
        .unwrap();
    assert_eq!(deferred.extra_labels[0].placement.along, -0.2);
    assert_eq!(
        (deferred.from, deferred.to),
        (Endpoint::Object(a), Endpoint::Object(inside))
    );
    assert!(!deferred.arrow_end);
    let free = links
        .iter()
        .find(|c| c.label.is_empty() && c.extra_labels.is_empty())
        .unwrap();
    assert_eq!(free.from, Endpoint::Point(Vec2::new(500.0, 0.0)));
    assert_eq!(free.to, Endpoint::Point(Vec2::new(510.0, 10.0)));
}

#[test]
fn group_and_frame_connector_detachments_are_reported() {
    let xml = page(
        r#"
        <mxCell id="edge" value="Detached" style="endArrow=none;" edge="1" parent="1" source="group" target="lane"><mxGeometry relative="1" as="geometry"/></mxCell>
        <mxCell id="group" style="group" vertex="1" parent="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>
        <mxCell id="a" vertex="1" parent="group"><mxGeometry width="20" height="20" as="geometry"/></mxCell>
        <mxCell id="b" vertex="1" parent="group"><mxGeometry x="60" width="20" height="20" as="geometry"/></mxCell>
        <mxCell id="lane" value="Lane" style="swimlane;startSize=24;collapsible=0;" vertex="1" parent="1"><mxGeometry x="200" y="50" width="100" height="100" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert!(
        report.warnings.contains(&ImportWarning::DetachedConnectors),
        "{:?}",
        report.warnings
    );
    let sheet = &report.workbook.sheets[0];
    let edge = connectors(sheet)
        .into_iter()
        .find(|edge| edge.label == "Detached")
        .unwrap();
    assert_eq!(edge.from, Endpoint::Point(Vec2::new(60.0, 50.0)));
    assert_eq!(edge.to, Endpoint::Point(Vec2::new(250.0, 100.0)));
    sheet.validate().unwrap();

    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    assert_eq!(
        crate::import_drawio_link(&link).unwrap().warnings,
        report.warnings
    );
    let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
    assert!(pasted.warnings.contains(&ImportWarning::DetachedConnectors));

    let omitted = page(
        r#"<mxCell id="edge" edge="1" parent="1" source="group" target="missing"><mxGeometry relative="1" as="geometry"/></mxCell>
        <mxCell id="group" style="group" vertex="1" parent="1"><mxGeometry width="100" height="100" as="geometry"/></mxCell>"#,
    );
    let omitted = load(&omitted);
    assert!(omitted.warnings.contains(&ImportWarning::Other));
    assert!(
        !omitted
            .warnings
            .contains(&ImportWarning::DetachedConnectors)
    );
    assert!(connectors(&omitted.workbook.sheets[0]).is_empty());

    let invalid = page(
        r#"<mxCell id="group" style="group" vertex="1" parent="1"><mxGeometry width="100" height="100" as="geometry"/></mxCell>
        <mxCell id="edge" edge="1" parent="1" source="group"><mxGeometry relative="1" as="geometry"><mxPoint x="1e20" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let invalid = load(&invalid);
    assert!(invalid.warnings.contains(&ImportWarning::Other));
    assert!(
        !invalid
            .warnings
            .contains(&ImportWarning::DetachedConnectors)
    );
    assert!(connectors(&invalid.workbook.sheets[0]).is_empty());
}

#[test]
fn shaped_container_fallback_keeps_its_connector_attachment() {
    let xml = page(
        r#"<mxCell id="lane" value="Small" style="swimlane;" vertex="1" parent="1"><mxGeometry width="100" height="80" as="geometry"/></mxCell>
        <mxCell id="shape" vertex="1" parent="1"><mxGeometry x="200" width="50" height="50" as="geometry"/></mxCell>
        <mxCell id="edge" style="endArrow=none;" edge="1" parent="1" source="lane" target="shape"><mxGeometry relative="1" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert!(!report.warnings.contains(&ImportWarning::DetachedConnectors));
    let sheet = &report.workbook.sheets[0];
    assert!(sheet.zones.is_empty());
    let lane = shapes(sheet)
        .into_iter()
        .find(|object| object.label() == "Small")
        .unwrap();
    assert_eq!(connectors(sheet)[0].from, Endpoint::Object(lane.id));
}

#[test]
fn missing_connector_terminals_with_fallback_points_are_reported() {
    let shape = r#"<mxCell id="shape" vertex="1" parent="1"><mxGeometry x="100" width="40" height="40" as="geometry"/></mxCell>"#;
    for (source, target, point) in [
        (" source=\"missing\"", " target=\"shape\"", "sourcePoint"),
        (" source=\"shape\"", " target=\"missing\"", "targetPoint"),
    ] {
        let xml = page(&format!(
            r#"{shape}<mxCell id="edge" style="endArrow=none;" edge="1" parent="1"{source}{target}><mxGeometry relative="1" as="geometry"><mxPoint x="7" y="8" as="{point}"/></mxGeometry></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::DetachedConnectors]);
        assert!(matches!(
            crate::import_bytes(Format::DrawIo, xml.as_bytes()),
            Err(IoError::ImportNeedsConfirmation(warnings))
                if warnings == vec![ImportWarning::DetachedConnectors]
        ));
        let edge = connectors(&report.workbook.sheets[0])[0];
        let expected = Endpoint::Point(Vec2::new(7.0, 8.0));
        assert_eq!(
            if point == "sourcePoint" {
                edge.from
            } else {
                edge.to
            },
            expected
        );
    }

    for source in ["", " source=\"\""] {
        let free = page(&format!(
            r#"{shape}<mxCell id="edge" style="endArrow=none;" edge="1" parent="1"{source} target="shape"><mxGeometry relative="1" as="geometry"><mxPoint x="7" y="8" as="sourcePoint"/></mxGeometry></mxCell>"#
        ));
        assert!(load(&free).warnings.is_empty());
    }

    let omitted = page(&format!(
        r#"{shape}<mxCell id="edge" style="endArrow=none;" edge="1" parent="1" source="shape" target="missing"><mxGeometry relative="1" as="geometry"/></mxCell>"#
    ));
    let report = load(&omitted);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert!(!report.warnings.contains(&ImportWarning::DetachedConnectors));
    assert!(connectors(&report.workbook.sheets[0]).is_empty());
}

#[test]
fn shaped_containers_report_their_frame_approximation() {
    for style in [
        "shape=partialRectangle;container=1;left=0;",
        "shape=folder;container=1;tabWidth=80;tabHeight=20;",
        "shape=ellipse;container=1;",
        "shape=doubleEllipse;container=1;",
        "swimlane;shape=folder;container=1;",
    ] {
        let xml = page(&format!(
            r##"<mxCell id="parent" value="Parent" style="{style}fillColor=#123456;swimlaneFillColor=#abcdef;" vertex="1" parent="1"><mxGeometry x="100" y="200" width="300" height="200" as="geometry"/></mxCell>
            <mxCell id="child" value="Child" style="shape=rectangle;" vertex="1" parent="parent"><mxGeometry x="10" y="20" width="50" height="40" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
        let sheet = &report.workbook.sheets[0];
        assert_eq!(sheet.zones.len(), 1, "{style}");
        assert_eq!(
            sheet.zones[0].background_color,
            Some(Color::rgb(0x12, 0x34, 0x56)),
            "{style}"
        );
        let child = shapes(sheet)[0];
        assert_eq!(child.rect, rect(110.0, 220.0, 50.0, 40.0));
        assert_eq!(sheet.zones[0].objects, vec![child.id]);
        sheet.validate().unwrap();

        let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
        let linked = crate::import_drawio_link(&link).unwrap();
        assert_eq!(linked.warnings, report.warnings);
        assert_eq!(linked.workbook, report.workbook);
    }

    let native_lane = page(
        r#"<mxCell id="parent" value="Parent" style="swimlane;startSize=24;collapsible=0;" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#,
    );
    let report = load(&native_lane);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.workbook.sheets[0].zones.len(), 1);
}

#[test]
fn nested_swimlanes_preserve_scene_hierarchy_and_header_orientation() {
    let xml = page(
        r#"
        <mxCell id="pool" value="Pool" style="swimlane;startSize=24;collapsible=0;" vertex="1" parent="1"><mxGeometry x="100" y="80" width="500" height="320" as="geometry"/></mxCell>
        <mxCell id="lane" value="Lane" style="swimlane;startSize=24;horizontal=0;collapsible=0;" vertex="1" parent="pool"><mxGeometry x="30" y="40" width="420" height="240" as="geometry"/></mxCell>
        <mxCell id="sub-lane" value="Sub-lane" style="swimlane;startSize=18;horizontal=1;collapsible=0;" vertex="1" parent="lane"><mxGeometry x="40" y="50" width="300" height="160" as="geometry"/></mxCell>
        <mxCell id="task" value="Task" style="shape=rectangle;" vertex="1" parent="sub-lane"><mxGeometry x="20" y="30" width="100" height="50" as="geometry"/></mxCell>
        <mxCell id="edge" edge="1" source="pool" target="task" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert!(!report.warnings.contains(&ImportWarning::Other));

    let sheet = &report.workbook.sheets[0];
    assert!(sheet.zones.is_empty());
    assert_eq!(sheet.canvas.scene.containers.len(), 3);
    assert_eq!(sheet.canvas.scene.parents.len(), 4);
    let objects = &sheet.canvas.objects;
    let pool = objects
        .iter()
        .find(|object| object.label() == "Pool")
        .unwrap();
    let lane = objects
        .iter()
        .find(|object| object.label() == "Lane")
        .unwrap();
    let sub_lane = objects
        .iter()
        .find(|object| object.label() == "Sub-lane")
        .unwrap();
    let task = objects
        .iter()
        .find(|object| object.label() == "Task")
        .unwrap();
    assert_eq!(pool.rect, rect(100.0, 80.0, 500.0, 320.0));
    assert_eq!(lane.rect, rect(130.0, 120.0, 420.0, 240.0));
    assert_eq!(sub_lane.rect, rect(170.0, 170.0, 300.0, 160.0));
    assert_eq!(task.rect, rect(190.0, 200.0, 100.0, 50.0));
    assert!(sheet.canvas.scene.containers.contains(&pool.id));
    assert!(sheet.canvas.scene.containers.contains(&lane.id));
    assert!(sheet.canvas.scene.containers.contains(&sub_lane.id));
    assert_eq!(sheet.canvas.scene.parents.get(&lane.id), Some(&pool.id));
    assert_eq!(sheet.canvas.scene.parents.get(&sub_lane.id), Some(&lane.id));
    assert_eq!(sheet.canvas.scene.parents.get(&task.id), Some(&sub_lane.id));
    let connector_id = sheet
        .canvas
        .objects
        .iter()
        .find(|object| matches!(object.kind, CanvasObjectKind::Connector(_)))
        .unwrap()
        .id;
    assert_eq!(
        objects.iter().map(|object| object.id).collect::<Vec<_>>(),
        vec![pool.id, lane.id, sub_lane.id, task.id, connector_id]
    );
    assert_eq!(
        sheet.canvas.scene.parents.get(&connector_id),
        Some(&pool.id)
    );
    assert_eq!(connectors(sheet).len(), 1);
    assert_eq!(connectors(sheet)[0].from, Endpoint::Object(pool.id));
    assert_eq!(connectors(sheet)[0].to, Endpoint::Object(task.id));
    assert_eq!(
        lane.stencil
            .as_ref()
            .and_then(|stencil| stencil.parameters.get("headerHeight")),
        Some(&24.0)
    );
    assert_eq!(
        lane.stencil
            .as_ref()
            .and_then(|stencil| stencil.parameters.get("headerSide")),
        Some(&3.0)
    );
    sheet.validate().unwrap();
    let native =
        export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        crate::import_bytes(Format::Thinoda, &native).unwrap(),
        report.workbook
    );
    let drawio = export_bytes(&report.workbook, Format::DrawIo, &ExportOptions::default()).unwrap();
    let round_trip = crate::import_bytes(Format::DrawIo, &drawio).unwrap();
    let restored = &round_trip.sheets[0];
    assert_eq!(
        restored.canvas.scene.containers,
        sheet.canvas.scene.containers
    );
    assert_eq!(restored.canvas.scene.parents, sheet.canvas.scene.parents);
    assert_eq!(
        restored
            .canvas
            .objects
            .iter()
            .map(|object| (object.id, object.rect))
            .collect::<Vec<_>>(),
        sheet
            .canvas
            .objects
            .iter()
            .map(|object| (object.id, object.rect))
            .collect::<Vec<_>>()
    );
    let restored_lane = restored
        .canvas
        .objects
        .iter()
        .find(|object| object.label() == "Lane")
        .unwrap();
    let restored_sub_lane = restored
        .canvas
        .objects
        .iter()
        .find(|object| object.label() == "Sub-lane")
        .unwrap();
    assert_eq!(
        restored_lane
            .stencil
            .as_ref()
            .and_then(|stencil| stencil.parameters.get("headerSide")),
        Some(&3.0)
    );
    assert_eq!(
        restored_sub_lane
            .stencil
            .as_ref()
            .and_then(|stencil| stencil.parameters.get("headerSide")),
        Some(&0.0)
    );
    assert_eq!(connectors(restored)[0].from, connectors(sheet)[0].from);
    assert_eq!(connectors(restored)[0].to, connectors(sheet)[0].to);
    let mut right_header = report.workbook.clone();
    right_header.sheets[0]
        .canvas
        .objects
        .iter_mut()
        .find(|object| object.id == lane.id)
        .unwrap()
        .stencil
        .as_mut()
        .unwrap()
        .parameters
        .insert("headerSide".into(), 1.0);
    assert!(matches!(
        export_bytes(&right_header, Format::DrawIo, &ExportOptions::default()),
        Err(IoError::Invalid(message)) if message.contains("right or bottom scene headers")
    ));

    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    assert_eq!(
        crate::import_drawio_link(&link).unwrap().warnings,
        report.warnings
    );
    let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
    assert!(pasted.warnings.contains(&ImportWarning::Other));
    assert!(
        pasted
            .losses
            .contains(&crate::drawio_clipboard::DrawioPasteLoss::SceneHierarchy(
                sheet.canvas.scene.parents.len()
            ))
    );
}

#[test]
fn isolated_scene_container_with_shape_child_round_trips() {
    let mut sheet = Sheet::new("Board", "Root");
    sheet.root_mode = SheetRootMode::Hidden;
    let container = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(100.0, 80.0, 400.0, 260.0),
            "Pool",
        ))
        .unwrap();
    let task = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(150.0, 140.0, 120.0, 60.0),
            "Task",
        ))
        .unwrap();
    sheet.canvas.scene.containers.insert(container);
    sheet.canvas.scene.parents.insert(task, container);
    let workbook = Workbook::new(sheet);

    let xml = String::from_utf8(
        export_bytes(&workbook, Format::DrawIo, &ExportOptions::default()).unwrap(),
    )
    .unwrap();
    assert!(xml.contains("thinodaSceneContainer=1;"));
    let imported = load(&xml);
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    let restored = imported.workbook;
    let sheet = &restored.sheets[0];
    assert_eq!(sheet.canvas.scene.parents.get(&task), Some(&container));
    assert!(sheet.canvas.scene.containers.contains(&container));
    assert_eq!(
        sheet
            .canvas
            .objects
            .iter()
            .find(|object| object.id == task)
            .unwrap()
            .rect,
        rect(150.0, 140.0, 120.0, 60.0)
    );
    restored.validate().unwrap();
}

#[test]
fn multiple_isolated_scene_containers_with_shape_children_round_trip() {
    let mut sheet = Sheet::new("Board", "Root");
    sheet.root_mode = SheetRootMode::Hidden;
    let first = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(40.0, 60.0, 300.0, 200.0),
            "First pool",
        ))
        .unwrap();
    let first_task = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(70.0, 110.0, 100.0, 50.0),
            "First task",
        ))
        .unwrap();
    let second = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(420.0, 60.0, 300.0, 200.0),
            "Second pool",
        ))
        .unwrap();
    let second_task = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(450.0, 110.0, 100.0, 50.0),
            "Second task",
        ))
        .unwrap();
    sheet.canvas.scene.containers.extend([first, second]);
    sheet.canvas.scene.parents.insert(first_task, first);
    sheet.canvas.scene.parents.insert(second_task, second);
    let workbook = Workbook::new(sheet);

    let exported = export_bytes(&workbook, Format::DrawIo, &ExportOptions::default()).unwrap();
    let imported = load(&String::from_utf8(exported).unwrap());
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    let restored = &imported.workbook.sheets[0];
    assert_eq!(
        restored.canvas.scene.containers,
        workbook.sheets[0].canvas.scene.containers
    );
    assert_eq!(
        restored.canvas.scene.parents,
        workbook.sheets[0].canvas.scene.parents
    );
    assert_eq!(
        restored
            .canvas
            .objects
            .iter()
            .map(|object| (object.id, object.rect))
            .collect::<Vec<_>>(),
        workbook.sheets[0]
            .canvas
            .objects
            .iter()
            .map(|object| (object.id, object.rect))
            .collect::<Vec<_>>()
    );
}

#[test]
fn isolated_scene_container_with_connector_is_refused() {
    let mut sheet = Sheet::new("Board", "Root");
    sheet.root_mode = SheetRootMode::Hidden;
    let container = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(100.0, 80.0, 400.0, 260.0),
            "Pool",
        ))
        .unwrap();
    let task = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(150.0, 140.0, 120.0, 60.0),
            "Task",
        ))
        .unwrap();
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(task),
            Endpoint::Point(Vec2::new(350.0, 200.0)),
        )))
        .unwrap();
    sheet.canvas.scene.containers.insert(container);
    sheet.canvas.scene.parents.insert(task, container);
    sheet.canvas.scene.parents.insert(edge, container);
    let workbook = Workbook::new(sheet);
    assert!(matches!(
        export_bytes(&workbook, Format::DrawIo, &ExportOptions::default()),
        Err(IoError::Invalid(message)) if message.contains("connectors inside isolated containers")
    ));
}

#[test]
fn isolated_scene_container_combined_with_nested_scene_is_refused() {
    let mut sheet = Sheet::new("Board", "Root");
    sheet.root_mode = SheetRootMode::Hidden;
    let isolated = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 300.0, 200.0),
            "Isolated",
        ))
        .unwrap();
    let isolated_child = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(20.0, 40.0, 100.0, 50.0),
            "Isolated child",
        ))
        .unwrap();
    let parent = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(400.0, 0.0, 300.0, 200.0),
            "Pool",
        ))
        .unwrap();
    let lane = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(420.0, 30.0, 260.0, 150.0),
            "Lane",
        ))
        .unwrap();
    sheet
        .canvas
        .scene
        .containers
        .extend([isolated, parent, lane]);
    sheet.canvas.scene.parents.insert(isolated_child, isolated);
    sheet.canvas.scene.parents.insert(lane, parent);
    let workbook = Workbook::new(sheet);
    assert!(matches!(
        export_bytes(&workbook, Format::DrawIo, &ExportOptions::default()),
        Err(IoError::Invalid(message)) if message.contains("isolated container is combined")
    ));
}

#[test]
fn isolated_scene_containers_across_pages_are_refused() {
    let isolated_sheet = |title: &str| {
        let mut sheet = Sheet::new(title, "Root");
        sheet.root_mode = SheetRootMode::Hidden;
        let container = sheet
            .add_canvas_object(shape(
                ShapeKind::Rectangle,
                rect(0.0, 0.0, 300.0, 200.0),
                "Pool",
            ))
            .unwrap();
        let child = sheet
            .add_canvas_object(shape(
                ShapeKind::Rectangle,
                rect(20.0, 40.0, 100.0, 50.0),
                "Task",
            ))
            .unwrap();
        sheet.canvas.scene.containers.insert(container);
        sheet.canvas.scene.parents.insert(child, container);
        sheet
    };
    let workbook = Workbook {
        sheets: vec![isolated_sheet("First"), isolated_sheet("Second")],
        ..Workbook::default()
    };
    assert!(matches!(
        export_bytes(&workbook, Format::DrawIo, &ExportOptions::default()),
        Err(IoError::Invalid(message)) if message.contains("isolated across multiple pages")
    ));
}

#[test]
fn nested_drawio_cards_are_imported_with_a_specific_containment_warning() {
    let xml = page(
        r#"
        <mxCell id="pool" value="Pool" style="swimlane;startSize=24;collapsible=0;" vertex="1" parent="1"><mxGeometry x="100" y="80" width="500" height="320" as="geometry"/></mxCell>
        <mxCell id="lane" value="Lane" style="swimlane;startSize=24;horizontal=0;collapsible=0;" vertex="1" parent="pool"><mxGeometry x="30" y="40" width="420" height="240" as="geometry"/></mxCell>
        <mxCell id="note" value="Nested note" style="shape=note;" vertex="1" parent="lane"><mxGeometry x="20" y="30" width="160" height="100" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert!(
        report
            .warnings
            .contains(&ImportWarning::DrawioSceneCardContainmentNotPreserved)
    );
    assert!(!report.warnings.contains(&ImportWarning::Other));

    let sheet = &report.workbook.sheets[0];
    assert_eq!(sheet.canvas.scene.containers.len(), 2);
    assert_eq!(sheet.canvas.scene.parents.len(), 1);
    let note = sheet
        .topics
        .values()
        .find(|topic| topic.title == "Nested note")
        .unwrap();
    assert_eq!(note.card.unwrap().kind, CardKind::Sticky);
}

#[test]
fn nested_swimlanes_preserve_mapped_layout_and_warn_for_missing_collapsed_bounds() {
    let xml = page(
        r#"
        <mxCell id="pool" value="Pool" style="swimlane;childLayout=stackLayout;horizontalStack=0;stackSpacing=8;stackBorder=1;marginLeft=10;marginRight=10;marginTop=10;marginBottom=10;" collapsed="1" vertex="1" parent="1"><mxGeometry width="400" height="240" as="geometry"/></mxCell>
        <mxCell id="lane" value="Lane" style="swimlane;" vertex="1" parent="pool"><mxGeometry x="20" y="30" width="300" height="180" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert!(
        !report
            .warnings
            .contains(&ImportWarning::DrawioSceneLayoutNotPreserved)
    );
    assert!(
        report
            .warnings
            .contains(&ImportWarning::DrawioCollapsedSceneExpanded)
    );
    let sheet = &report.workbook.sheets[0];
    assert_eq!(sheet.canvas.scene.containers.len(), 2);
    let pool = sheet
        .canvas
        .objects
        .iter()
        .find(|object| object.label() == "Pool")
        .unwrap();
    assert_eq!(
        sheet.canvas.scene.layout_policies.get(&pool.id),
        Some(&thinoda_core::scene_layout::ContainerLayoutPolicy {
            header: thinoda_core::scene_layout::HeaderPolicy {
                side: thinoda_core::scene_layout::HeaderSide::Top,
                extent: 40.0,
            },
            layout: thinoda_core::scene_layout::ContainerLayout::Stack {
                axis: thinoda_core::scene_layout::StackAxis::Vertical,
                spacing: 8.0,
                padding: 11.0,
                cross_axis: thinoda_core::scene_layout::StackCrossAxisPolicy::Fill,
                resize: thinoda_core::scene_layout::StackResizePolicy::GrowParent,
            },
        })
    );
    assert!(
        !report
            .warnings
            .contains(&ImportWarning::DrawioSceneLayoutNotPreserved)
    );
    let drawio = export_bytes(&report.workbook, Format::DrawIo, &ExportOptions::default()).unwrap();
    let round_trip = crate::import_bytes_with_report(Format::DrawIo, &drawio).unwrap();
    assert_eq!(
        round_trip.workbook.sheets[0].canvas.scene.layout_policies,
        sheet.canvas.scene.layout_policies
    );
    let native =
        export_bytes(&report.workbook, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        crate::import_bytes(Format::Thinoda, &native).unwrap(),
        report.workbook
    );
}

#[test]
fn nested_stack_layout_maps_drawio_resize_flags() {
    use thinoda_core::scene_layout::{ContainerLayout, StackCrossAxisPolicy, StackResizePolicy};
    let header = thinoda_core::scene_layout::HeaderPolicy {
        side: thinoda_core::scene_layout::HeaderSide::Top,
        extent: 24.0,
    };
    let layout = |style: &str| {
        let (policy, unsupported) = Style::parse(style).scene_layout_policy(header);
        assert!(!unsupported);
        match policy.unwrap().layout {
            stack @ ContainerLayout::Stack { .. } => stack,
            ContainerLayout::Free => unreachable!(),
        }
    };
    let resize = |style: &str| match layout(style) {
        ContainerLayout::Stack { resize, .. } => resize,
        ContainerLayout::Free => unreachable!(),
    };
    assert_eq!(
        resize("childLayout=stackLayout"),
        StackResizePolicy::GrowParent
    );
    assert_eq!(
        resize("childLayout=stackLayout;resizeParent=0;resizeLast=1"),
        StackResizePolicy::FillLast
    );
    assert_eq!(
        resize("childLayout=stackLayout;resizeParent=1;resizeParentMax=0"),
        StackResizePolicy::FitParent
    );
    assert_eq!(
        resize("childLayout=stackLayout;resizeParent=0;resizeLast=0"),
        StackResizePolicy::Preserve
    );
    assert_eq!(
        match layout("childLayout=stackLayout;fill=0") {
            ContainerLayout::Stack { cross_axis, .. } => cross_axis,
            ContainerLayout::Free => unreachable!(),
        },
        StackCrossAxisPolicy::Preserve
    );
    assert_eq!(
        match layout("childLayout=stackLayout") {
            ContainerLayout::Stack { cross_axis, .. } => cross_axis,
            ContainerLayout::Free => unreachable!(),
        },
        StackCrossAxisPolicy::Fill
    );
    assert!(
        Style::parse("childLayout=stackLayout;allowGaps=1")
            .scene_layout_policy(header)
            .1
    );
    assert!(
        Style::parse("childLayout=stackLayout;fill=invalid")
            .scene_layout_policy(header)
            .1
    );
    assert_eq!(
        Style::parse("childLayout=stackLayout;horizontalStack=2").scene_layout_policy(header),
        (None, true)
    );
}

#[test]
fn drawio_export_refuses_unmapped_distributed_gap_layouts() {
    use thinoda_core::scene_layout::{
        ContainerLayout, ContainerLayoutPolicy, HeaderPolicy, HeaderSide, StackAxis,
        StackCrossAxisPolicy, StackResizePolicy,
    };

    let mut sheet = Sheet::new("Board", "Root");
    let container = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 300.0, 200.0),
        ))
        .unwrap();
    sheet.set_canvas_container(container, true).unwrap();
    sheet
        .set_canvas_container_layout_policy(
            container,
            Some(ContainerLayoutPolicy {
                header: HeaderPolicy {
                    side: HeaderSide::Top,
                    extent: 24.0,
                },
                layout: ContainerLayout::Stack {
                    axis: StackAxis::Vertical,
                    spacing: 5.0,
                    padding: 10.0,
                    cross_axis: StackCrossAxisPolicy::Fill,
                    resize: StackResizePolicy::Distribute,
                },
            }),
        )
        .unwrap();

    let error = export_bytes(
        &Workbook::new(sheet),
        Format::DrawIo,
        &ExportOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(error, IoError::Invalid(message) if message.contains("distributed stack spacing"))
    );
}

#[test]
fn nested_swimlanes_warn_and_drop_unmapped_gap_policy() {
    let xml = page(
        r#"
        <mxCell id="pool" value="Pool" style="swimlane;childLayout=stackLayout;allowGaps=1;" vertex="1" parent="1"><mxGeometry width="400" height="240" as="geometry"/></mxCell>
        <mxCell id="lane" value="Lane" style="swimlane;" vertex="1" parent="pool"><mxGeometry x="20" y="30" width="300" height="180" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert!(
        report
            .warnings
            .contains(&ImportWarning::DrawioSceneLayoutNotPreserved)
    );
    let sheet = &report.workbook.sheets[0];
    let pool = sheet
        .canvas
        .objects
        .iter()
        .find(|object| object.label() == "Pool")
        .unwrap();
    assert!(!sheet.canvas.scene.layout_policies.contains_key(&pool.id));
}

#[test]
fn collapsed_nested_swimlanes_preserve_alternate_bounds_on_round_trip() {
    let xml = page(
        r#"
        <mxCell id="pool" value="Pool" style="swimlane;startSize=24;collapsible=1;" collapsed="1" vertex="1" parent="1"><mxGeometry x="100" y="80" width="160" height="120" as="geometry"><mxRectangle x="100" y="80" width="500" height="320" as="alternateBounds"/></mxGeometry></mxCell>
        <mxCell id="lane" value="Lane" style="swimlane;startSize=24;horizontal=0;collapsible=1;" collapsed="1" vertex="1" parent="pool"><mxGeometry x="30" y="40" width="120" height="100" as="geometry"><mxRectangle x="30" y="40" width="420" height="240" as="alternateBounds"/></mxGeometry></mxCell>
        <mxCell id="task" value="Task" style="shape=rectangle;" vertex="1" parent="lane"><mxGeometry x="20" y="30" width="100" height="50" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert!(
        !report
            .warnings
            .contains(&ImportWarning::DrawioCollapsedSceneExpanded),
        "{:?}",
        report.warnings
    );

    let sheet = &report.workbook.sheets[0];
    let object = |label: &str| {
        sheet
            .canvas
            .objects
            .iter()
            .find(|object| object.label() == label)
            .unwrap()
    };
    let pool = object("Pool");
    let lane = object("Lane");
    let task = object("Task");
    assert_eq!(pool.rect, rect(100.0, 80.0, 160.0, 120.0));
    assert_eq!(lane.rect, rect(130.0, 120.0, 120.0, 100.0));
    assert_eq!(task.rect, rect(150.0, 150.0, 100.0, 50.0));
    assert_eq!(sheet.canvas.scene.parents.get(&lane.id), Some(&pool.id));
    assert_eq!(sheet.canvas.scene.parents.get(&task.id), Some(&lane.id));
    assert_eq!(
        sheet.canvas.scene.collapsed.get(&pool.id),
        Some(&rect(100.0, 80.0, 500.0, 320.0))
    );
    assert_eq!(
        sheet.canvas.scene.collapsed.get(&lane.id),
        Some(&rect(130.0, 120.0, 420.0, 240.0))
    );
    report.workbook.validate().unwrap();

    let exported =
        export_bytes(&report.workbook, Format::DrawIo, &ExportOptions::default()).unwrap();
    let exported = String::from_utf8(exported).unwrap();
    assert!(exported.contains("collapsed=\"1\""));
    assert!(exported.contains("as=\"alternateBounds\""));
    let restored = load(&exported);
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    let restored_sheet = &restored.workbook.sheets[0];
    assert_eq!(
        restored_sheet.canvas.scene.collapsed,
        sheet.canvas.scene.collapsed
    );
    assert_eq!(
        restored_sheet.canvas.scene.parents,
        sheet.canvas.scene.parents
    );
    for label in ["Pool", "Lane", "Task"] {
        assert_eq!(
            object(label).rect,
            restored_sheet
                .canvas
                .objects
                .iter()
                .find(|item| item.label() == label)
                .unwrap()
                .rect
        );
    }
    restored.workbook.validate().unwrap();
}

#[test]
fn nested_stack_layout_does_not_approximate_asymmetric_margins() {
    let xml = page(
        r#"
        <mxCell id="pool" value="Pool" style="swimlane;childLayout=stackLayout;stackSpacing=4;marginLeft=2;marginRight=6;" vertex="1" parent="1"><mxGeometry width="400" height="240" as="geometry"/></mxCell>
        <mxCell id="lane" value="Lane" style="swimlane;" vertex="1" parent="pool"><mxGeometry x="20" y="30" width="300" height="180" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert!(
        report
            .warnings
            .contains(&ImportWarning::DrawioSceneLayoutNotPreserved)
    );
    assert!(
        report.workbook.sheets[0]
            .canvas
            .scene
            .layout_policies
            .is_empty()
    );
}

#[test]
fn notes_text_lines_directions_and_user_objects() {
    let xml = page(
        r#"
        <mxCell id="n" value="Note" style="shape=note;fillColor=#fff2cc;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="80" as="geometry"/></mxCell>
        <mxCell id="t" value="Label" style="text;html=1;" vertex="1" parent="1"><mxGeometry x="0" y="100" width="60" height="20" as="geometry"/></mxCell>
        <mxCell id="ln" style="line;strokeWidth=2;direction=south;" vertex="1" parent="1"><mxGeometry x="0" y="200" width="10" height="100" as="geometry"/></mxCell>
        <mxCell id="tri" style="triangle;direction=south;" vertex="1" parent="1"><mxGeometry x="0" y="400" width="60" height="40" as="geometry"/></mxCell>
        <UserObject label="Linked" tooltip="Tip" link="https://example.com" id="u"><mxCell style="shape=note;" vertex="1" parent="1"><mxGeometry x="300" y="0" width="100" height="80" as="geometry"/></mxCell></UserObject>
        "#,
    );
    let report = load(&xml);
    // The named text style, card tooltip and folded outlines are approximated.
    assert_eq!(
        report.warnings,
        vec![
            ImportWarning::Formatting,
            ImportWarning::DrawioFoldedNotesApproximated,
            ImportWarning::Other,
        ]
    );
    let sheet = &report.workbook.sheets[0];
    let note = sheet.topics.values().find(|t| t.title == "Note").unwrap();
    let card = note.card.unwrap();
    assert_eq!(card.kind, CardKind::Sticky);
    assert_eq!(card.color, Some(Color::rgb(0xFF, 0xF2, 0xCC)));
    assert_eq!(note.position, Some(Vec2::new(50.0, 40.0)));
    let linked = sheet.topics.values().find(|t| t.title == "Linked").unwrap();
    assert_eq!(linked.hyperlink.as_deref(), Some("https://example.com"));
    let label = shapes(sheet)
        .into_iter()
        .find(|o| o.label() == "Label")
        .unwrap();
    assert_eq!(label.style.fill, Some(Color::TRANSPARENT));
    let tri = shapes(sheet)
        .into_iter()
        .find(|o| {
            matches!(
                o.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::Triangle,
                    ..
                }
            )
        })
        .unwrap();
    // draw.io triangles point east and Thinoda's point north; south turns them down.
    assert_eq!(tri.rotation, 180.0);
    assert_eq!(tri.rect, rect(0.0, 400.0, 60.0, 40.0));
    let line = connectors(sheet)[0];
    assert_eq!(line.from, Endpoint::Point(Vec2::new(5.0, 200.0)));
    assert_eq!(line.to, Endpoint::Point(Vec2::new(5.0, 300.0)));
    assert!(!line.arrow_end);
}

#[test]
fn unsupported_content_is_reported_not_dropped_silently() {
    let xml = page(
        r#"
        <mxCell id="d" value="Patterned" style="rounded=0;fillStyle=future;" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="k" value="Unknown" style="shape=mxgraph.aws4.lambda;" vertex="1" parent="1"><mxGeometry y="20" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="i" style="shape=image;image=https://example.com/a.png;" vertex="1" parent="1"><mxGeometry y="40" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="h" value="Hidden" vertex="1" visible="0" parent="1"><mxGeometry y="60" width="10" height="10" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert_eq!(
        report.warnings,
        vec![
            ImportWarning::Formatting,
            ImportWarning::Resources,
            ImportWarning::UnsupportedDrawioShapes,
            ImportWarning::Other
        ]
    );
    assert_eq!(
        shapes(&report.workbook.sheets[0]).len(),
        3,
        "unknown and image shapes become placeholders"
    );
}

fn kinds_by_label(sheet: &Sheet) -> Vec<(String, ShapeKind)> {
    shapes(sheet)
        .iter()
        .filter_map(|o| match &o.kind {
            CanvasObjectKind::Shape { shape, label } => Some((label.clone(), *shape)),
            _ => None,
        })
        .collect()
}

#[test]
fn built_in_shape_aliases_map_to_their_thinoda_equivalents() {
    let cell = |id: &str, style: &str, y: u32| {
        format!(
            r#"<mxCell id="{id}" value="{id}" style="{style}" vertex="1" parent="1"><mxGeometry y="{y}" width="40" height="20" as="geometry"/></mxCell>"#
        )
    };
    let exact = [
        (
            "pr",
            "shape=partialRectangle;fillColor=#FFFFFF;strokeColor=#000000;fontColor=#000000;",
        ),
        ("br", "shape=mxgraph.basic.rect;"),
        ("bs", "shape=mxgraph.bootstrap.rect;"),
        ("bt", "shape=mxgraph.bootstrap.rrect;"),
        ("dl", "shape=delay;"),
    ];
    let cells: String = exact
        .iter()
        .enumerate()
        .map(|(n, (id, style))| cell(id, style, n as u32 * 30))
        .collect();
    let fixed_colors = page(&cells).replacen(
        "<mxGraphModel>",
        "<mxGraphModel adaptiveColors=\"simple\">",
        1,
    );
    let sheet = clean(&fixed_colors);
    use ShapeKind as S;
    assert_eq!(
        kinds_by_label(&sheet),
        vec![
            ("pr".into(), S::Rectangle),
            ("br".into(), S::Rectangle),
            ("bs".into(), S::Rectangle),
            ("bt".into(), S::RoundedRectangle),
            ("dl".into(), S::Delay),
        ]
    );

    let approximate = [
        ("fo", "shape=folder;"),
        ("cu", "shape=cube;"),
        ("me", "shape=message;"),
        ("uf", "shape=umlFrame;"),
        ("es", "shape=endState;"),
        ("pc", "shape=mxgraph.basic.partConcEllipse;"),
        ("ar", "shape=mxgraph.arrows2.arrow;"),
        ("ps", "shape=partialRectangle;top=0;left=0;"),
    ];
    let cells: String = approximate
        .iter()
        .enumerate()
        .map(|(n, (id, style))| cell(id, style, n as u32 * 30))
        .collect();
    let report = load(&page(&cells));
    assert_eq!(
        report.warnings,
        vec![ImportWarning::Formatting],
        "approximations are formatting losses, not unknown content"
    );
    let sheet = &report.workbook.sheets[0];
    assert_eq!(
        kinds_by_label(sheet),
        vec![
            ("fo".into(), S::Rectangle),
            ("cu".into(), S::Rectangle),
            ("me".into(), S::Rectangle),
            ("uf".into(), S::Rectangle),
            ("es".into(), S::FinalState),
            ("pc".into(), S::Rectangle),
            ("ar".into(), S::Rectangle),
            ("ps".into(), S::Rectangle),
        ]
    );
    let stencils: Vec<_> = shapes(sheet)
        .iter()
        .map(|object| object.stencil.as_ref().map(|s| s.definition.as_str()))
        .collect();
    assert_eq!(
        stencils,
        [
            Some("folder"),
            Some("cube"),
            Some("message"),
            Some("uml-frame"),
            None,
            Some("part-concentric-ellipse"),
            Some("arrow"),
            None,
        ]
    );
    let partial = shapes(sheet).last().copied().unwrap();
    assert_eq!(partial.style.stroke, Some(Color::BLACK));
    assert_eq!(
        partial.rectangle_sides,
        Some(RectangleSides {
            top: false,
            right: true,
            bottom: true,
            left: false,
        })
    );
}

#[test]
fn partial_rectangle_side_values_follow_published_stylesheet_resolution() {
    for (value, visible) in [
        ("1", true),
        ("1.0", true),
        ("01", true),
        (" 1 ", true),
        ("1e0", true),
        ("+1", true),
        ("none", true),
        ("0", false),
        ("2", false),
        ("-1", false),
        ("true", false),
        ("false", false),
        ("0x1", false),
        ("", false),
        (" none ", false),
        ("unknown", false),
    ] {
        let style = Style::parse(&format!("shape=partialRectangle;top={value};"));
        assert_eq!(style.rectangle_sides().top, visible, "{value:?}");
    }
    let style = Style::parse("shape=partialRectangle;top=0;top=none;right=0;right=1.0;");
    assert_eq!(style.rectangle_sides(), RectangleSides::default());
}

#[test]
fn partial_rectangle_masks_directions_and_opacity_survive_drawio_roundtrip() {
    for mask in 0..16 {
        for (direction, degrees) in [
            ("east", 0.0),
            ("south", 90.0),
            ("west", 180.0),
            ("north", 270.0),
        ] {
            let sides = RectangleSides {
                top: mask & 1 != 0,
                right: mask & 2 != 0,
                bottom: mask & 4 != 0,
                left: mask & 8 != 0,
            };
            let xml = page(&format!(
                r##"<mxCell id="p" style="shape=partialRectangle;direction={direction};rotation=30;top={};right={};bottom={};left={};fillColor=#224466;fillOpacity=40;strokeColor=#AA5500;strokeOpacity=60;strokeWidth=4;" vertex="1" parent="1"><mxGeometry x="20" y="30" width="100" height="60" as="geometry"/></mxCell>"##,
                u8::from(sides.top),
                u8::from(sides.right),
                u8::from(sides.bottom),
                u8::from(sides.left),
            ));
            let report = load(&xml);
            assert_eq!(
                report.warnings,
                vec![ImportWarning::Formatting],
                "{mask:04b} {direction}"
            );
            let original = shapes(&report.workbook.sheets[0])[0];
            assert_eq!(original.rectangle_sides, Some(sides));
            let expected_frame = if degrees == 90.0 || degrees == 270.0 {
                rect(40.0, 10.0, 60.0, 100.0)
            } else {
                rect(20.0, 30.0, 100.0, 60.0)
            };
            assert_eq!(original.rect, expected_frame);
            assert_eq!(original.rotation, (degrees + 30.0) % 360.0);
            assert_eq!(
                original.style.fill,
                Some(Color::rgba(0x22, 0x44, 0x66, 102))
            );
            assert_eq!(original.style.stroke, Some(Color::rgba(0xAA, 0x55, 0, 153)));
            let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
            assert!(exported.contains("shape=partialRectangle;"), "{exported}");
            let roundtrip = load(&exported);
            assert_eq!(roundtrip.warnings, vec![ImportWarning::Formatting]);
            let restored = shapes(&roundtrip.workbook.sheets[0])[0];
            assert_eq!(restored.rectangle_sides, original.rectangle_sides);
            assert_eq!(restored.rect, original.rect);
            assert_eq!(restored.rotation, original.rotation);
            assert_eq!(restored.style, original.style);
        }
    }
}

#[test]
fn ordinary_and_partial_rectangles_normalize_imported_rotations() {
    for (rotation, expected) in [("450", 90.0), ("-90", 270.0), ("720", 0.0), ("-450", 270.0)] {
        for shape in ["rectangle", "partialRectangle"] {
            let xml = page(&format!(
                r#"<mxCell id="p" style="shape={shape};rotation={rotation};" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
            ));
            let report = load(&xml);
            let original = shapes(&report.workbook.sheets[0])[0];
            assert_eq!(original.rotation, expected, "{shape}, {rotation}");
            let exported = export(&report.workbook).unwrap();
            let restored = load(std::str::from_utf8(&exported).unwrap());
            assert_eq!(
                shapes(&restored.workbook.sheets[0])[0].rotation,
                expected,
                "{shape}, {rotation}"
            );
        }
    }
}

#[test]
fn partial_rectangle_adaptive_color_warnings_respect_page_mode_and_visibility() {
    let cases = [
        ("fillColor=#224466;strokeColor=none;", "", true),
        ("fillColor=none;strokeColor=#AA5500;", "", true),
        (
            "fillColor=none;strokeColor=#AA5500;top=0;right=0;bottom=0;left=0;",
            "",
            false,
        ),
        (
            "fillColor=none;strokeColor=none;fontColor=#AA5500;",
            "Label",
            true,
        ),
        ("fillColor=none;strokeColor=none;", "", false),
    ];
    for (attribute, adaptive) in [
        ("", true),
        (r#" adaptiveColors="auto""#, true),
        (r#" adaptiveColors="simple""#, false),
        (r#" adaptiveColors="none""#, false),
        (r#" adaptiveColors="future""#, true),
    ] {
        for (style, label, visible) in cases {
            let xml = page(&format!(
                r#"<mxCell id="p" value="{label}" style="shape=partialRectangle;{style}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
            ))
            .replacen("<mxGraphModel>", &format!("<mxGraphModel{attribute}>"), 1);
            let report = load(&xml);
            assert_eq!(
                report.warnings.contains(&ImportWarning::Formatting),
                adaptive && visible,
                "{attribute}, {style}, {label}: {:?}",
                report.warnings
            );
        }
    }
}

#[test]
fn partial_rectangle_hidden_nonblack_colors_survive_drawio_roundtrip() {
    let xml = page(
        r##"<mxCell id="p" style="shape=partialRectangle;fillColor=#123456;fillOpacity=0;strokeColor=#ABCDEF;strokeOpacity=0;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
    );
    let report = load(&xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let original = shapes(&report.workbook.sheets[0])[0];
    assert_eq!(original.style.fill, Some(Color::rgba(0x12, 0x34, 0x56, 0)));
    assert_eq!(
        original.style.stroke,
        Some(Color::rgba(0xAB, 0xCD, 0xEF, 0))
    );
    let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    assert!(exported.contains("fillColor=#123456;fillOpacity=0;"));
    assert!(exported.contains("strokeColor=#ABCDEF;strokeOpacity=0;"));
    let restored = load(&exported);
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    assert_eq!(
        shapes(&restored.workbook.sheets[0])[0].style,
        original.style
    );
}

#[test]
fn partial_rectangle_appearance_losses_cover_visible_fill_outline_and_label() {
    let mut sheet = Sheet::new("Rectangles", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let frame = rect(0.0, 0.0, 100.0, 60.0);
    let hidden_sides = RectangleSides {
        top: false,
        right: false,
        bottom: false,
        left: false,
    };
    let mut automatic = CanvasObject::shape(ShapeKind::Rectangle, frame);
    automatic.rectangle_sides = Some(RectangleSides::default());
    automatic.style.stroke_width = 17.0;
    sheet.add_canvas_object(automatic).unwrap();
    let mut fill = CanvasObject::shape(ShapeKind::Rectangle, frame);
    fill.rectangle_sides = Some(hidden_sides);
    fill.style.fill = Some(Color::rgb(20, 40, 60));
    sheet.add_canvas_object(fill).unwrap();
    let mut outline = CanvasObject::shape(ShapeKind::Rectangle, frame);
    outline.rectangle_sides = Some(RectangleSides::default());
    outline.style.fill = Some(Color::TRANSPARENT);
    outline.style.stroke = Some(Color::BLACK);
    sheet.add_canvas_object(outline).unwrap();
    let mut label = CanvasObject::shape(ShapeKind::Rectangle, frame);
    label.rectangle_sides = Some(hidden_sides);
    label.style.fill = Some(Color::TRANSPARENT);
    label.style.stroke = Some(Color::TRANSPARENT);
    if let CanvasObjectKind::Shape { label, .. } = &mut label.kind {
        *label = "Visible label".into();
    }
    sheet.add_canvas_object(label).unwrap();
    let mut invisible = CanvasObject::shape(ShapeKind::Rectangle, frame);
    invisible.rectangle_sides = Some(hidden_sides);
    invisible.style.fill = Some(Color::TRANSPARENT);
    invisible.style.stroke = Some(Color::TRANSPARENT);
    sheet.add_canvas_object(invisible).unwrap();
    let book = Workbook::new(sheet);
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![ExportLoss::DrawioRectangleAppearanceApproximated(4)],
            "{format:?}"
        );
    }
    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(exported.contains("strokeWidth=2;"), "{exported}");
    assert!(export_losses(&book, Format::Thinoda, &ExportOptions::default()).is_empty());
}

#[test]
fn partial_rectangle_default_colors_are_resolved_or_reported() {
    for (style, fill, stroke, warn) in [
        ("shape=partialRectangle;", Color::WHITE, Color::BLACK, true),
        (
            ";shape=partialRectangle;",
            Color::TRANSPARENT,
            Color::TRANSPARENT,
            false,
        ),
        (
            "shape=partialRectangle;fillColor=none;strokeColor=none;",
            Color::TRANSPARENT,
            Color::TRANSPARENT,
            false,
        ),
        (
            "shape=partialRectangle;fillColor=default;strokeColor=default;fillOpacity=40;strokeOpacity=60;",
            Color::rgba(255, 255, 255, 102),
            Color::rgba(0, 0, 0, 153),
            true,
        ),
    ] {
        let report = load(&page(&format!(
            r#"<mxCell id="p" style="{style}" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        )));
        let object = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(object.style.fill, Some(fill), "{style}");
        assert_eq!(object.style.stroke, Some(stroke), "{style}");
        assert_eq!(
            report.warnings.contains(&ImportWarning::Formatting),
            warn,
            "{style}"
        );
    }
}

#[test]
fn waypoint_dots_follow_drawio_size_and_fill_alpha() {
    for (size, width, diameter) in [
        ("6", "1", 6.0),
        ("12", "2", 14.0),
        ("1", "3", 6.0),
        ("-4", "4", 8.0),
        ("1", "0", 0.0),
        ("none", "1", 6.0),
    ] {
        let xml = page(&format!(
            r##"<mxCell id="w" style="shape=waypoint;size={size};strokeWidth={width};strokeColor=#FF0000;fillColor=none;fillOpacity=50;strokeOpacity=20;" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="40" as="geometry"/></mxCell>"##
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "waypoint identity and original cell bounds are not preserved"
        );
        let sheet = &report.workbook.sheets[0];
        let dot = shapes(sheet)[0];
        assert!(matches!(
            dot.kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::Ellipse,
                ..
            }
        ));
        assert_eq!(
            dot.rect,
            Rect::from_center_size(Vec2::new(110.0, 70.0), Vec2::new(diameter, diameter)),
            "size={size}, strokeWidth={width}"
        );
        assert_eq!(dot.style.fill, Some(Color::rgba(255, 0, 0, 128)));
        assert_eq!(dot.style.stroke, Some(Color::TRANSPARENT));
        report.workbook.validate().unwrap();
    }
}

#[test]
fn waypoint_invalid_sizes_are_bounded_and_reported() {
    for (size, width) in [
        ("NaN", "1"),
        ("Infinity", "1"),
        ("1e30", "1"),
        ("6", "1e30"),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="w" style="shape=waypoint;size={size};strokeWidth={width};" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="40" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(report.warnings.contains(&ImportWarning::Formatting));
        assert_eq!(
            shapes(&report.workbook.sheets[0])[0].rect,
            Rect::from_center_size(Vec2::new(110.0, 70.0), Vec2::new(6.0, 6.0)),
            "size={size}, strokeWidth={width}"
        );
        report.workbook.validate().unwrap();
    }
}

#[test]
fn waypoint_dots_use_the_resolved_stroke_color() {
    for (style, color) in [
        ("shape=waypoint;", Color::BLACK),
        ("shape=waypoint;strokeColor=none;", Color::TRANSPARENT),
        (";shape=waypoint;", Color::TRANSPARENT),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="w" style="{style}" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="40" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        let dot = shapes(&report.workbook.sheets[0])[0];
        assert_eq!(dot.style.fill, Some(color), "{style}");
        assert_eq!(dot.style.stroke, Some(Color::TRANSPARENT), "{style}");
    }
}

#[test]
fn waypoint_svg_and_png_render_only_the_filled_dot() {
    let xml = page(
        r##"<mxCell id="w" style="shape=waypoint;size=12;strokeWidth=2;strokeColor=#FF0000;fillColor=none;" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="40" as="geometry"/></mxCell>"##,
    );
    let book = load(&xml).workbook;
    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            transparent: true,
            ..Default::default()
        };
        let svg = String::from_utf8(
            crate::export_bytes_with_consent(&book, Format::Svg, &options, true).unwrap(),
        )
        .unwrap();
        let id = shapes(book.active())[0].id;
        let group = svg
            .split_once(&format!("id=\"object-{}\"", id.0))
            .unwrap()
            .1
            .split_once("</g>")
            .unwrap()
            .0;
        assert!(
            group.contains(r##"fill="#FF0000" stroke="#00000000""##),
            "{group}"
        );
        let png = image::load_from_memory(
            &crate::export_bytes_with_consent(&book, Format::Png, &options, true).unwrap(),
        )
        .unwrap()
        .to_rgba8();
        let red: Vec<_> = png
            .enumerate_pixels()
            .filter(|(_, _, pixel)| {
                pixel[0] > 240 && pixel[1] < 20 && pixel[2] < 20 && pixel[3] > 0
            })
            .map(|(x, y, _)| (x, y))
            .collect();
        let width = red.iter().map(|(x, _)| x).max().unwrap()
            - red.iter().map(|(x, _)| x).min().unwrap()
            + 1;
        let height = red.iter().map(|(_, y)| y).max().unwrap()
            - red.iter().map(|(_, y)| y).min().unwrap()
            + 1;
        assert!((13..=15).contains(&width), "dark={dark}, width={width}");
        assert!((13..=15).contains(&height), "dark={dark}, height={height}");
    }
}

#[test]
fn waypoint_and_deferred_connector_follow_source_paint_order() {
    let dot = r##"<mxCell id="w" style="shape=waypoint;size=12;strokeColor=#FF0000;perimeter=centerPerimeter;" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="40" as="geometry"/></mxCell>"##;
    let edge = r##"<mxCell id="e" style="endArrow=none;strokeColor=#00FF00;strokeWidth=4;" edge="1" target="w" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="70" as="sourcePoint"/></mxGeometry></mxCell>"##;
    for edge_first in [true, false] {
        let cells = if edge_first {
            format!("{edge}{dot}")
        } else {
            format!("{dot}{edge}")
        };
        let book = load(&page(&cells)).workbook;
        let mut books = vec![("import", book.clone())];
        for format in [Format::Thinoda, Format::DrawIo] {
            let bytes =
                crate::export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                    .unwrap();
            let restored = crate::import_bytes(format, &bytes).unwrap();
            if format == Format::Thinoda {
                assert_eq!(restored, book);
            }
            books.push((
                if format == Format::Thinoda {
                    "native"
                } else {
                    "draw.io"
                },
                restored,
            ));
        }
        for (source, book) in books {
            book.validate().unwrap();
            let sheet = book.active();
            let dot = shapes(sheet)[0].id;
            let connector = sheet
                .canvas
                .objects
                .iter()
                .find(|object| object.is_connector())
                .unwrap()
                .id;
            let ids: Vec<_> = sheet
                .canvas
                .objects
                .iter()
                .map(|object| object.id)
                .collect();
            assert_eq!(
                ids,
                if edge_first {
                    vec![connector, dot]
                } else {
                    vec![dot, connector]
                },
                "{source}"
            );
            let layout = layout_sheet(sheet, &ApproxMeasure, false);
            let scene = thinoda_core::scene::build_scene(sheet, &layout);
            assert_eq!(
                scene.hit_test(Vec2::new(110.0, 70.0)),
                Some(thinoda_core::scene::SceneHit::Object(if edge_first {
                    dot
                } else {
                    connector
                })),
                "{source}"
            );

            let options = ExportOptions {
                transparent: true,
                ..Default::default()
            };
            let svg = String::from_utf8(
                crate::export_bytes_with_consent(&book, Format::Svg, &options, true).unwrap(),
            )
            .unwrap();
            let dot_position = svg.find(&format!("id=\"object-{}\"", dot.0)).unwrap();
            let edge_position = svg.find(&format!("id=\"object-{}\"", connector.0)).unwrap();
            assert_eq!(edge_position < dot_position, edge_first, "{source}");
            let png = image::load_from_memory(
                &crate::export_bytes_with_consent(&book, Format::Png, &options, true).unwrap(),
            )
            .unwrap()
            .to_rgba8();
            let red: Vec<_> = png
                .enumerate_pixels()
                .filter(|(_, _, pixel)| pixel[0] > 240 && pixel[1] < 20 && pixel[2] < 20)
                .map(|(x, y, _)| (x, y))
                .collect();
            let x = (red.iter().map(|(x, _)| x).min().unwrap()
                + red.iter().map(|(x, _)| x).max().unwrap())
                / 2;
            let y = (red.iter().map(|(_, y)| y).min().unwrap()
                + red.iter().map(|(_, y)| y).max().unwrap())
                / 2;
            let center = png.get_pixel(x, y);
            assert!(
                if edge_first {
                    center[0] > 240 && center[1] < 20
                } else {
                    center[1] > 240 && center[0] < 20
                },
                "{source}, edge_first={edge_first}, center={center:?}"
            );
        }
    }
}

#[test]
fn deferred_edges_follow_nested_cell_order_not_global_xml_order() {
    let xml = page(
        r#"<mxCell id="g" style="group;locked=1;" vertex="1" parent="1"><mxGeometry width="150" height="100" as="geometry"/></mxCell>
        <mxCell id="outside" value="outside" vertex="1" parent="1"><mxGeometry x="200" width="40" height="40" as="geometry"/></mxCell>
        <mxCell id="first" value="first" edge="1" source="a" target="b" parent="g"><mxGeometry relative="1" as="geometry"/></mxCell>
        <mxCell id="a" value="a" style="thinodaBand=below;" vertex="1" parent="g"><mxGeometry width="30" height="30" as="geometry"/></mxCell>
        <mxCell id="middle" value="middle" style="thinodaBand=below;" edge="1" source="a" target="b" parent="g"><mxGeometry relative="1" as="geometry"/></mxCell>
        <mxCell id="b" value="b" vertex="1" parent="g"><mxGeometry x="100" width="30" height="30" as="geometry"/></mxCell>
        <mxCell id="last" value="last" edge="1" source="a" target="b" parent="g"><mxGeometry relative="1" as="geometry"/></mxCell>"#,
    );
    let book = load(&xml).workbook;
    let mut books = vec![("import", book.clone())];
    for format in [Format::Thinoda, Format::DrawIo] {
        let bytes =
            crate::export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                .unwrap();
        let report = crate::import_bytes_with_report(format, &bytes).unwrap();
        if format == Format::DrawIo {
            assert!(report.warnings.contains(&ImportWarning::Formatting));
        }
        books.push((
            if format == Format::Thinoda {
                "native"
            } else {
                "draw.io"
            },
            report.workbook,
        ));
    }
    for (source, book) in books {
        book.validate().unwrap();
        let sheet = book.active();
        let labels: Vec<_> = sheet
            .canvas
            .objects
            .iter()
            .map(CanvasObject::label)
            .collect();
        assert_eq!(
            labels,
            ["first", "a", "middle", "b", "last", "outside"],
            "{source}"
        );
        let group = sheet.canvas.groups.first().unwrap();
        assert!(group.locked, "{source}");
        for object in &sheet.canvas.objects {
            assert_eq!(
                object.group,
                (object.label() != "outside").then_some(group.id),
                "{source}: {}",
                object.label()
            );
        }
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        let scene = thinoda_core::scene::build_scene(sheet, &layout);
        let scene_labels: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| match item.id {
                thinoda_core::scene::SceneId::Object(id) => {
                    Some(sheet.canvas.object(id).unwrap().label())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            scene_labels,
            ["a", "middle", "first", "b", "last", "outside"],
            "{source}"
        );
    }
}

#[test]
fn waypoint_center_perimeter_connectors_attach_to_the_dot_center() {
    let center = Vec2::new(110.0, 70.0);
    for (source, constraint) in [
        (true, ""),
        (false, ""),
        (true, "exitX=1;exitY=0;exitDx=5;exitPerimeter=1;"),
        (false, "entryX=1;entryY=0;entryDy=5;entryPerimeter=1;"),
    ] {
        let terminal = if source {
            r#"source="w""#
        } else {
            r#"target="w""#
        };
        let xml = page(&format!(
            r#"<mxCell id="w" style="shape=waypoint;size=12;perimeter=centerPerimeter;" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="40" as="geometry"/></mxCell>
            <mxCell id="e" style="endArrow=none;{constraint}" edge="1" {terminal} parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="250" y="70" as="sourcePoint"/><mxPoint x="250" y="70" as="targetPoint"/></mxGeometry></mxCell>"#
        ));
        let report = load(&xml);
        let sheet = &report.workbook.sheets[0];
        let connector = connectors(sheet)[0];
        let port = if source {
            connector.from_port
        } else {
            connector.to_port
        };
        assert_eq!(
            port,
            Some(ConnectorPort {
                position: Vec2::new(0.5, 0.5),
                offset: Vec2::ZERO,
                perimeter: false,
            }),
            "source={source}, constraint={constraint}"
        );
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        let path = thinoda_core::scene::connector_path(sheet, &layout, connector);
        let endpoint = if source {
            path[0]
        } else {
            *path.last().unwrap()
        };
        assert_eq!(endpoint, center, "source={source}, constraint={constraint}");
        report.workbook.validate().unwrap();
    }
}

#[test]
fn supplied_circuit_waypoints_keep_centered_connector_endpoints() {
    let report = import(include_bytes!(
        "../../../../docs/draw-io-examples/waypoint-shape.drawio"
    ))
    .unwrap();
    let sheet = &report.workbook.sheets[0];
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    for center in [
        Vec2::new(230.0, 220.0),
        Vec2::new(470.0, 220.0),
        Vec2::new(230.0, 60.0),
        Vec2::new(230.0, 390.0),
    ] {
        let dot = shapes(sheet)
            .into_iter()
            .find(|object| {
                matches!(
                    object.kind,
                    CanvasObjectKind::Shape {
                        shape: ShapeKind::Ellipse,
                        ..
                    }
                ) && object.rect.center() == center
                    && object.rect.size == Vec2::new(6.0, 6.0)
            })
            .expect("circuit waypoint");
        let mut attached = 0;
        for connector in connectors(sheet) {
            let path = thinoda_core::scene::connector_path(sheet, &layout, connector);
            if connector.from == Endpoint::Object(dot.id) {
                assert_eq!(path[0], center);
                attached += 1;
            }
            if connector.to == Endpoint::Object(dot.id) {
                assert_eq!(*path.last().unwrap(), center);
                attached += 1;
            }
        }
        assert!(attached >= 2, "center={center:?}, attached={attached}");
    }
    report.workbook.validate().unwrap();
}

#[test]
fn waypoint_absolute_constraints_use_original_cell_bounds() {
    for (vertex_style, expected) in [
        ("", Vec2::new(123.0, 46.0)),
        ("rotation=90;", Vec2::new(134.0, 83.0)),
        ("direction=south;", Vec2::new(124.0, 93.0)),
        ("direction=south;flipH=1;", Vec2::new(96.0, 93.0)),
        (
            "direction=south;anchorPointDirection=0;flipH=1;",
            Vec2::new(123.0, 94.0),
        ),
        ("perimeterSpacing=2;", Vec2::new(125.0, 44.0)),
    ] {
        for source in [true, false] {
            let (terminal, prefix) = if source {
                (r#"source="w""#, "exit")
            } else {
                (r#"target="w""#, "entry")
            };
            let xml = page(&format!(
                r#"<mxCell id="w" style="shape=waypoint;perimeter=centerPerimeter;{vertex_style}" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="40" as="geometry"/></mxCell>
                <mxCell id="e" style="endArrow=none;{prefix}X=1;{prefix}Y=0;{prefix}Dx=3;{prefix}Dy=-4;{prefix}Perimeter=0;" edge="1" {terminal} parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="250" y="70" as="sourcePoint"/><mxPoint x="250" y="70" as="targetPoint"/></mxGeometry></mxCell>"#
            ));
            let report = load(&xml);
            assert!(report.warnings.contains(&ImportWarning::Formatting));
            let sheet = &report.workbook.sheets[0];
            let connector = connectors(sheet)[0];
            let port = if source {
                connector.from_port
            } else {
                connector.to_port
            }
            .unwrap();
            assert_eq!(port.position, Vec2::new(0.5, 0.5));
            assert!(!port.perimeter);
            let layout = layout_sheet(sheet, &ApproxMeasure, false);
            let path = thinoda_core::scene::connector_path(sheet, &layout, connector);
            let point = if source {
                path[0]
            } else {
                *path.last().unwrap()
            };
            assert!(
                (point - expected).length() < 0.001,
                "vertex_style={vertex_style}, source={source}, point={point:?}"
            );
            report.workbook.validate().unwrap();
        }
    }
}

#[test]
fn waypoint_connector_positions_survive_moves_and_format_round_trips() {
    let xml = page(
        r#"<mxCell id="w" style="shape=waypoint;perimeter=centerPerimeter;" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="40" as="geometry"/></mxCell>
        <mxCell id="absolute" style="endArrow=none;exitX=1;exitY=0;exitDx=3;exitDy=-4;exitPerimeter=0;" edge="1" source="w" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="250" y="70" as="targetPoint"/></mxGeometry></mxCell>
        <mxCell id="center" style="endArrow=none;" edge="1" target="w" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="70" as="sourcePoint"/></mxGeometry></mxCell>"#,
    );
    let mut book = load(&xml).workbook;
    let dot = shapes(book.active())[0].id;
    let delta = Vec2::new(10.0, 20.0);
    book.active_mut()
        .move_canvas_objects(&[dot], delta)
        .unwrap();
    assert_eq!(feature_version(&book), 22);
    for format in [Format::Thinoda, Format::DrawIo] {
        let bytes =
            crate::export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                .unwrap();
        let restored = crate::import_bytes(format, &bytes).unwrap();
        if format == Format::Thinoda {
            assert_eq!(restored, book);
        }
        let sheet = restored.active();
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        let mut points = Vec::new();
        for connector in connectors(sheet) {
            let path = thinoda_core::scene::connector_path(sheet, &layout, connector);
            if matches!(connector.from, Endpoint::Object(_)) {
                points.push(path[0]);
            }
            if matches!(connector.to, Endpoint::Object(_)) {
                points.push(*path.last().unwrap());
            }
        }
        assert_eq!(points.len(), 2);
        for expected in [
            Vec2::new(123.0, 46.0) + delta,
            Vec2::new(110.0, 70.0) + delta,
        ] {
            assert!(
                points
                    .iter()
                    .any(|point| (*point - expected).length() < 0.001),
                "{format:?}: missing {expected:?} in {points:?}"
            );
        }
    }
}

#[test]
fn waypoint_unrepresentable_fixed_ports_fall_back_with_a_warning() {
    let xml = page(
        r#"<mxCell id="w" style="shape=waypoint;perimeterSpacing=1000000;" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="40" as="geometry"/></mxCell>
        <mxCell id="e" style="endArrow=none;exitX=1;exitY=0;exitDx=10;exitPerimeter=0;" edge="1" source="w" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="250" y="70" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    assert_eq!(
        connectors(&report.workbook.sheets[0])[0].from_port,
        Some(ConnectorPort {
            position: Vec2::new(1.0, 0.0),
            offset: Vec2::new(10.0, 0.0),
            perimeter: false,
        })
    );
    report.workbook.validate().unwrap();
}

fn tiny_png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn image_cards(sheet: &Sheet) -> Vec<&thinoda_core::Topic> {
    sheet
        .topics
        .values()
        .filter(|t| t.card.is_some_and(|c| c.kind == CardKind::Image))
        .collect()
}

#[test]
fn inline_png_and_svg_images_become_transparent_image_cards() {
    let png = tiny_png(40, 20);
    let svg = b64(br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle cx="5" cy="5" r="4" fill="blue"/></svg>"#);
    let xml = page(&format!(
        r#"
        <mxCell id="p" value="Logo" style="shape=image;verticalLabelPosition=bottom;image=data:image/png,{};" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="80" as="geometry"/></mxCell>
        <mxCell id="s" style="shape=image;imageAspect=0;image=data:image/svg+xml,{svg};" vertex="1" parent="1"><mxGeometry x="200" y="10" width="60" height="30" as="geometry"/></mxCell>
        <mxCell id="e" style="endArrow=classic;" edge="1" parent="1" source="p" target="s"><mxGeometry relative="1" as="geometry"/></mxCell>
        "#,
        b64(&png)
    ));
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Resources]);
    assert!(matches!(
        crate::import_bytes(Format::DrawIo, xml.as_bytes()),
        Err(IoError::ImportNeedsConfirmation(warnings)) if warnings.contains(&ImportWarning::Resources)
    ));
    let sheet = report.workbook.sheets.into_iter().next().unwrap();
    let cards = image_cards(&sheet);
    assert_eq!(cards.len(), 2);
    let logo = cards.iter().find(|t| t.title == "Logo").unwrap();
    // 2:1 pixels fitted into an 80x80 cell keep their aspect.
    assert_eq!(logo.image_size, Some(Vec2::new(80.0, 40.0)));
    assert_eq!(logo.card.unwrap().color, Some(Color::TRANSPARENT));
    let resource = sheet.resources.get(logo.image.as_deref().unwrap()).unwrap();
    assert_eq!(resource.media_type, "image/png");
    assert_eq!(resource.bytes(), png.as_slice());
    let vector = cards.iter().find(|t| t.title.is_empty()).unwrap();
    assert_eq!(
        vector.image_size,
        Some(Vec2::new(60.0, 30.0)),
        "imageAspect=0 stretches"
    );
    let raster = sheet
        .resources
        .get(vector.image.as_deref().unwrap())
        .unwrap();
    let decoded = thinoda_core::images::decode(raster.bytes()).unwrap();
    assert_eq!((decoded.width, decoded.height), (120, 120));
    assert!(
        decoded.rgba.chunks(4).any(|p| p[2] > 200 && p[3] == 255),
        "circle rendered"
    );
    assert_eq!(
        connectors(&sheet).len(),
        1,
        "edges still attach to image cards"
    );
    sheet.validate().unwrap();
}

#[test]
fn inline_static_gif_and_webp_are_recovered_on_images_and_image_cards() {
    for (media, format) in [
        ("image/gif", image::ImageFormat::Gif),
        ("image/webp", image::ImageFormat::WebP),
    ] {
        let pixels = image::RgbaImage::from_pixel(4, 2, image::Rgba([200, 30, 30, 255]));
        let mut encoded = std::io::Cursor::new(Vec::new());
        pixels.write_to(&mut encoded, format).unwrap();
        let bytes = encoded.into_inner();
        let expected = image::load_from_memory(&bytes).unwrap().to_rgba8();
        let data = b64(&bytes);
        let xml = page(&format!(
            r#"<mxCell id="a" style="shape=image;image=data:{media},{data};" vertex="1" parent="1"><mxGeometry width="40" height="20" as="geometry"/></mxCell>
            <mxCell id="b" style="shape=image;thinodaCard=image;image=data:{media},{data};" vertex="1" parent="1"><mxGeometry x="50" width="40" height="20" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::Resources),
            "{media}"
        );
        assert!(matches!(
            crate::import_bytes(Format::DrawIo, xml.as_bytes()),
            Err(IoError::ImportNeedsConfirmation(warnings)) if warnings.contains(&ImportWarning::Resources)
        ));
        let sheet = &report.workbook.sheets[0];
        let cards = image_cards(sheet);
        assert_eq!(cards.len(), 2, "{media}");
        for card in cards {
            let resource = sheet.resources.get(card.image.as_deref().unwrap()).unwrap();
            assert_eq!(resource.media_type, "image/png");
            assert_eq!(
                image::load_from_memory(resource.bytes())
                    .unwrap()
                    .to_rgba8(),
                expected
            );
        }
        sheet.validate().unwrap();
    }
}

#[test]
fn invalid_and_animated_inline_images_warn_for_both_object_paths() {
    let mut animated = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut animated);
        for value in [10, 20] {
            let pixels = image::RgbaImage::from_pixel(2, 2, image::Rgba([value, 0, 0, 255]));
            encoder.encode_frame(image::Frame::new(pixels)).unwrap();
        }
    }
    let mut still = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(2, 2, image::Rgba([30, 0, 0, 255]))
        .write_to(&mut still, image::ImageFormat::WebP)
        .unwrap();
    let mut frame = vec![0; 16];
    frame[6] = 1;
    frame[9] = 1;
    frame[12] = 1;
    frame.extend_from_slice(&still.into_inner()[12..]);
    let mut payload = b"WEBPVP8X".to_vec();
    payload.extend_from_slice(&10u32.to_le_bytes());
    payload.extend_from_slice(&[2, 0, 0, 0, 1, 0, 0, 1, 0, 0]);
    payload.extend_from_slice(b"ANIM");
    payload.extend_from_slice(&6u32.to_le_bytes());
    payload.extend_from_slice(&[0; 6]);
    payload.extend_from_slice(b"ANMF");
    payload.extend_from_slice(&(frame.len() as u32).to_le_bytes());
    payload.extend_from_slice(&frame);
    let mut animated_webp = b"RIFF".to_vec();
    animated_webp.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    animated_webp.extend_from_slice(&payload);
    for (media, data) in [
        ("image/gif", b64(&animated)),
        ("image/webp", b64(&animated_webp)),
        ("image/gif", b64(b"bad gif")),
        ("image/webp", b64(b"bad webp")),
    ] {
        for style in ["shape=image;", "shape=image;thinodaCard=image;"] {
            let xml = page(&format!(
                r#"<mxCell id="a" style="{style}image=data:{media},{data};" vertex="1" parent="1"><mxGeometry width="40" height="20" as="geometry"/></mxCell>"#
            ));
            let report = load(&xml);
            assert!(
                report.warnings.contains(&ImportWarning::Resources),
                "{media} {style}"
            );
            assert!(matches!(
                crate::import_bytes(Format::DrawIo, xml.as_bytes()),
                Err(IoError::ImportNeedsConfirmation(warnings)) if warnings.contains(&ImportWarning::Resources)
            ));
            let sheet = &report.workbook.sheets[0];
            assert!(sheet.topics.values().all(|topic| topic.image.is_none()));
            sheet.validate().unwrap();
        }
    }
}

#[test]
fn remote_and_unsupported_images_stay_placeholders_with_their_source() {
    let svgz = b64(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0]);
    let file_ref = b64(br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="4" height="4"><image width="4" height="4" xlink:href="Cargo.toml"/></svg>"#);
    let xml = page(&format!(
        r#"
        <mxCell id="r" style="shape=image;image=https://example.com/a.png;" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="l" style="shape=image;image=img/lib/azure2/compute/VM.svg;" vertex="1" parent="1"><mxGeometry y="20" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="g" style="shape=image;image=data:image/gif,R0lGODlhAQABAAAAACw=;" vertex="1" parent="1"><mxGeometry y="40" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="z" style="shape=image;image=data:image/svg+xml,{svgz};" vertex="1" parent="1"><mxGeometry y="60" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="b" style="shape=image;image=data:image/png,bm90IGEgcG5n;" vertex="1" parent="1"><mxGeometry y="80" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="f" style="shape=image;image=data:image/svg+xml,{file_ref};" vertex="1" parent="1"><mxGeometry y="100" width="10" height="10" as="geometry"/></mxCell>
        "#
    ));
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Resources]);
    let sheet = &report.workbook.sheets[0];
    let mut alts: Vec<_> = shapes(sheet).iter().map(|o| o.alt_text.clone()).collect();
    alts.sort();
    assert_eq!(
        alts,
        vec![
            "",
            "",
            "",
            "https://example.com/a.png",
            "img/lib/azure2/compute/VM.svg"
        ],
        "data URIs are not echoed into alt text"
    );
    // The SVG referencing a local file still renders, but without reading the file.
    let cards = image_cards(sheet);
    assert_eq!(cards.len(), 1);
    let raster = sheet
        .resources
        .get(cards[0].image.as_deref().unwrap())
        .unwrap();
    let decoded = thinoda_core::images::decode(raster.bytes()).unwrap();
    assert!(
        decoded.rgba.chunks(4).all(|p| p[3] == 0),
        "file href was not loaded"
    );
}

#[test]
fn image_cards_round_trip_their_pixels_and_size() {
    let png = tiny_png(30, 30);
    let xml = page(&format!(
        r#"<mxCell id="p" value="Pic" style="shape=image;image=data:image/png,{};" vertex="1" parent="1"><mxGeometry x="10" y="10" width="50" height="50" as="geometry"/></mxCell>"#,
        b64(&png)
    ));
    let book = load(&xml).workbook;
    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(exported.contains("shape=image;thinodaCard=image;"));
    assert!(exported.contains(&format!("image=data:image/png,{};", b64(&png))));
    let again = roundtrip(&book);
    let before = image_cards(&book.sheets[0]);
    let after = image_cards(&again.sheets[0]);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].image_size, before[0].image_size);
    assert_eq!(after[0].card, before[0].card);
    let bytes = |sheet: &Sheet, topic: &thinoda_core::Topic| {
        sheet
            .resources
            .get(topic.image.as_deref().unwrap())
            .unwrap()
            .bytes()
            .to_vec()
    };
    assert_eq!(bytes(&again.sheets[0], after[0]), png);
    assert_eq!(bytes(&book.sheets[0], before[0]), png);

    // The public gate accepts card pictures but still refuses resources draw.io would drop.
    let opts = crate::ExportOptions::default();
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        let encoded = crate::export_bytes(&book, format, &opts).unwrap();
        let restored = import(&encoded).unwrap().workbook;
        let card = image_cards(&restored.sheets[0])[0];
        assert_eq!(bytes(&restored.sheets[0], card), png, "{format:?}");
    }
    let mut lossy = book.clone();
    let sheet = &mut lossy.sheets[0];
    let reference = before[0].image.clone();
    let root = sheet.root;
    sheet.topic_mut(root).unwrap().image = reference;
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert!(crate::export_bytes(&lossy, format, &opts).is_err());
    }
}

#[test]
fn hostile_inputs_are_rejected() {
    let dtd = "<!DOCTYPE mxfile [<!ENTITY x \"boom\">]><mxfile><diagram>&x;</diagram></mxfile>";
    assert!(import(dtd.as_bytes()).is_err());
    assert!(import(b"<mxfile><diagram>&unknown;</diagram></mxfile>").is_err());
    let deep = format!(
        "<mxfile>{}{}</mxfile>",
        "<a>".repeat(100),
        "</a>".repeat(100)
    );
    assert!(import(deep.as_bytes()).is_err());
    assert!(import(b"<mxfile></mxfile>").is_err());
    assert!(import(b"<html/>").is_err());
    assert!(import(b"<mxfile><diagram>!!notbase64!!</diagram></mxfile>").is_err());
    assert!(import(PNG_SIGNATURE).is_err());
    let mut truncated = PNG_SIGNATURE.to_vec();
    truncated.extend(u32::MAX.to_be_bytes());
    truncated.extend(b"tEXt");
    assert!(!sniff(&truncated));
    let bomb = miniz_oxide::deflate::compress_to_vec(&vec![b'a'; MAX_BYTES + 1], 9);
    let encoded = base64::engine::general_purpose::STANDARD.encode(bomb);
    let page = format!("<mxfile><diagram>{encoded}</diagram></mxfile>");
    assert!(import(page.as_bytes()).is_err());
    assert!(matches!(
        import_html(legacy_html_viewer(&encoded).as_bytes()),
        Err(IoError::DrawIoHtml(crate::DrawIoHtmlError::InvalidViewer))
    ));
}

#[test]
fn cyclic_and_self_parented_cells_terminate() {
    let xml = page(
        r#"
        <mxCell id="x" value="X" vertex="1" parent="y"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="y" value="Y" vertex="1" parent="x"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="s" value="S" vertex="1" parent="s"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="ok" value="OK" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert!(
        shapes(&report.workbook.sheets[0])
            .iter()
            .any(|o| o.label() == "OK")
    );
}

#[test]
fn export_reports_flattened_map_content() {
    let mut book = board();
    assert_eq!(
        export_losses(&book, Format::DrawIo, &ExportOptions::default()),
        vec![
            ExportLoss::LegacyConnectorMarkersApproximated(2),
            ExportLoss::DrawioLabelAppearanceApproximated(2),
            ExportLoss::DrawioGridSettingsApproximated(1),
            ExportLoss::DrawioCardAppearanceApproximated(2),
        ]
    );
    let sheet = &mut book.sheets[0];
    let idea = sheet.add_floating("Idea", Vec2::new(200.0, -300.0));
    let detail = sheet.add_child(idea, None, "Detail");
    sheet.topic_mut(detail).unwrap().notes = Some("note".into());
    sheet
        .add_presentation_slide(
            "All",
            PresentationSource::Viewport,
            rect(0.0, 0.0, 100.0, 100.0),
        )
        .unwrap();
    assert_eq!(
        export_losses(&book, Format::DrawIo, &ExportOptions::default()),
        vec![
            ExportLoss::MapElementsFlattened(2),
            ExportLoss::LegacyConnectorMarkersApproximated(2),
            ExportLoss::DrawioLabelAppearanceApproximated(2),
            ExportLoss::DrawioGridSettingsApproximated(1),
            ExportLoss::DrawioCardAppearanceApproximated(2),
            ExportLoss::TopicDetailsDropped(1),
            ExportLoss::SlidesDropped(1),
        ]
    );
    let bytes = export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap();
    let report = import(&bytes).unwrap();
    assert_eq!(
        report.warnings,
        vec![
            ImportWarning::Formatting,
            ImportWarning::DrawioFoldedNotesApproximated,
        ]
    );
    let sheet = &report.workbook.sheets[0];
    assert!(shapes(sheet).iter().any(|o| o.label() == "Idea"));
    assert!(shapes(sheet).iter().any(|o| o.label() == "Detail"));
    assert_eq!(
        connectors(sheet).len(),
        3,
        "two board connectors and one branch"
    );
}

#[test]
fn stencil_objects_round_trip_through_native_drawio_styles() {
    let package = thinoda_core::stencil_object::find_package("thinoda.general").unwrap();
    let mut sheet = Sheet::new("Stencils", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut expected = Vec::new();
    for (index, definition) in package.definitions.iter().enumerate() {
        let mut object = thinoda_core::stencil_object::new_object(
            package,
            definition,
            Vec2::new(120.0 + 160.0 * index as f32, 100.0),
        );
        object.rotation = 30.0;
        if let Some((name, parameter)) = definition.parameters.iter().next() {
            let value = parameter.clamp(parameter.default + 1.0);
            object
                .stencil
                .as_mut()
                .unwrap()
                .parameters
                .insert(name.clone(), value);
        }
        sheet.add_canvas_object(object).unwrap();
        let inserted = sheet.canvas.objects.last().unwrap();
        expected.push((inserted.rect, inserted.stencil.clone().unwrap()));
    }
    let xml = String::from_utf8(export(&Workbook::new(sheet)).unwrap()).unwrap();
    assert!(
        xml.contains("shape=cube;"),
        "draw.io draws the native shape"
    );
    assert!(xml.contains("thinodaStencilDefinition=cube;"));
    let imported = clean(&xml);
    let actual: Vec<_> = shapes(&imported)
        .into_iter()
        .map(|object| {
            assert_eq!(object.rotation, 30.0);
            assert!(object.rounding.is_none());
            (object.rect, object.stencil.clone().unwrap())
        })
        .collect();
    assert_eq!(actual, expected);

    // A parameter edited in draw.io wins over the stored Thinoda override.
    let edited = xml.replacen(
        "shape=cube;whiteSpace=wrap;html=1;size=21;",
        "shape=cube;whiteSpace=wrap;html=1;size=35;",
        1,
    );
    assert_ne!(edited, xml);
    let cube = clean(&edited)
        .canvas
        .objects
        .iter()
        .find_map(|object| object.stencil.clone().filter(|s| s.definition == "cube"))
        .unwrap();
    assert_eq!(cube.parameters.get("size"), Some(&35.0));

    // Unknown packages keep the reference and fall back to the legacy shape.
    let report = load(&page(
        r#"<mxCell id="2" value="x" style="shape=cube;thinodaStencilPackage=vendor.pkg;thinodaStencilDefinition=gear;thinodaStencilParam.teeth=12;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="60" as="geometry"/></mxCell>"#,
    ));
    let unknown = report.workbook.sheets.into_iter().next().unwrap();
    let object = shapes(&unknown)[0];
    let stencil = object.stencil.as_ref().unwrap();
    assert_eq!(
        (stencil.package.as_str(), stencil.definition.as_str()),
        ("vendor.pkg", "gear")
    );
    assert_eq!(stencil.parameters.get("teeth"), Some(&12.0));
    assert!(thinoda_core::stencil_object::render(object).is_none());
}

#[test]
fn stencil_import_keeps_orientation_and_unknown_fallback_shapes() {
    let cell = |style: &str| {
        format!(
            r#"<mxCell id="2" value="x" style="{style}" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="60" as="geometry"/></mxCell>"#
        )
    };
    // draw.io turns `direction=south` frames by a quarter turn with swapped sides.
    let folder = clean(&page(&cell(
        "shape=folder;direction=south;thinodaStencilPackage=thinoda.general;thinodaStencilDefinition=folder;",
    )));
    let object = shapes(&folder)[0];
    assert_eq!(object.rotation, 90.0);
    assert_eq!(
        object.rect,
        Rect::from_center_size(Vec2::new(50.0, 40.0), Vec2::new(60.0, 80.0))
    );
    assert!(thinoda_core::stencil_object::render(object).is_some());

    // A note-styled unknown stencil stays a shape so its reference survives.
    let note = load(&page(&cell(
        "shape=note;thinodaStencilPackage=vendor.pkg;thinodaStencilDefinition=memo;",
    )));
    let sheet = note.workbook.sheets.into_iter().next().unwrap();
    assert_eq!(
        shapes(&sheet)[0].stencil.as_ref().unwrap().definition,
        "memo"
    );

    // An unknown stencil keeps its classified fallback through a round trip.
    let triangle = load(&page(&cell(
        "triangle;direction=north;thinodaStencilPackage=vendor.pkg;thinodaStencilDefinition=arrowhead;",
    )));
    let sheet = triangle.workbook.sheets.into_iter().next().unwrap();
    let before = shapes(&sheet)[0].clone();
    assert!(matches!(
        before.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Triangle,
            ..
        }
    ));
    let xml = String::from_utf8(export(&Workbook::new(sheet)).unwrap()).unwrap();
    let again = load(&xml).workbook.sheets.into_iter().next().unwrap();
    let after = shapes(&again)[0];
    assert_eq!(
        (&after.kind, after.rect, after.rotation),
        (&before.kind, before.rect, before.rotation)
    );
    assert_eq!(after.stencil, before.stencil);
}

#[test]
fn installed_user_stencil_libraries_resolve_plain_drawio_shapes() {
    let cell = r#"<mxCell id="x" style="shape=mxgraph.thinodauser.gear_box;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#;
    let unresolved = load(&page(cell));
    assert!(
        unresolved
            .warnings
            .contains(&ImportWarning::UnsupportedDrawioShapes)
    );
    let library = crate::drawio_stencil::import_drawio_stencil_library(
        r#"<shapes name="mxgraph.thinodaUser"><shape name="Gear Box" w="10" h="10"><foreground><rect x="0" y="0" w="10" h="10"/><fillstroke/></foreground></shape></shapes>"#,
        "",
    )
    .unwrap();
    crate::drawio_stencil::set_user_stencil_packages(
        library.packages.into_iter().map(std::sync::Arc::new),
    );
    let sheet = clean(&page(cell));
    let stencil = shapes(&sheet)[0].stencil.clone().expect("user stencil");
    assert_eq!(stencil.package, "user.drawio.thinodauser");
    assert!(thinoda_core::stencil_object::render(shapes(&sheet)[0]).is_some());
    let exported = String::from_utf8(export(&Workbook::new(sheet)).unwrap()).unwrap();
    assert!(exported.contains("shape=mxgraph.thinodauser.gear_box;"));
    crate::drawio_stencil::set_user_stencil_packages([]);
}

#[test]
fn drawio_passthrough_round_trips_unknown_style_attributes_and_ids() {
    let xml = page(concat!(
        r#"<UserObject label="A" id="node-a" owner="ops" placeholders="1"><mxCell style="rounded=0;whiteSpace=wrap;html=1;customKey=alpha;fillColor=#ff0000;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="40" as="geometry"/></mxCell></UserObject>"#,
        r#"<mxCell id="e1" value="B" style="ellipse;whiteSpace=wrap;html=1;anotherKey=7;" vertex="1" parent="1"><mxGeometry x="200" y="10" width="80" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="edge-1" style="endArrow=classic;html=1;edgeKey=x;" edge="1" parent="1" source="node-a" target="e1"><mxGeometry relative="1" as="geometry"/></mxCell>"#,
    ));
    let xml = xml
        .replace("<diagram name=\"P\">", "<diagram id=\"diag-1\" name=\"P\">")
        .replace("<mxGraphModel>", "<mxGraphModel futureFlag=\"on\">");
    let report = load(&xml);
    let sheet = &report.workbook.sheets[0];
    let a = sheet
        .canvas
        .objects
        .iter()
        .find(|o| o.drawio.as_ref().and_then(|d| d.source_id.as_deref()) == Some("node-a"))
        .unwrap();
    let drawio = a.drawio.as_ref().unwrap();
    assert!(
        drawio.style.contains(&("customKey".into(), "alpha".into())),
        "{:?}",
        drawio.style
    );
    assert!(
        !drawio.style.iter().any(|(k, _)| k == "fillColor"),
        "{:?}",
        drawio.style
    );
    assert!(drawio.attributes.is_empty(), "{:?}", drawio.attributes);
    let data = a.data.as_ref().unwrap();
    assert_eq!(data.properties, [("owner".into(), "ops".into())]);
    assert!(data.placeholders);
    let page = sheet.drawio.as_ref().unwrap();
    assert_eq!(page.diagram_id.as_deref(), Some("diag-1"));
    assert_eq!(page.model_attributes, [("futureFlag".into(), "on".into())]);

    let native = crate::formats::native_export(&report.workbook).unwrap();
    let reopened = crate::formats::native_import(&native).unwrap();
    assert!(reopened.has_drawio_passthrough());
    assert_eq!(crate::required_native_version(&reopened), 44);
    let content = serde_json::to_vec(&reopened).unwrap();
    let old = crate::formats::archive(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":2,"resources":[]}"#,
        ),
        ("content.json", &content),
    ])
    .unwrap();
    assert!(matches!(
        crate::formats::native_import(&old),
        Err(IoError::NewerVersion(_))
    ));
    let out = String::from_utf8(export(&reopened).unwrap()).unwrap();
    for needle in [
        "<diagram id=\"diag-1\"",
        "futureFlag=\"on\"",
        "id=\"node-a\" placeholders=\"1\" owner=\"ops\"",
        "customKey=alpha;",
        "id=\"e1\"",
        "anotherKey=7;",
        "id=\"edge-1\"",
        "edgeKey=x;",
        "source=\"node-a\" target=\"e1\"",
    ] {
        assert!(out.contains(needle), "missing {needle}: {out}");
    }
    let again = load(&out);
    let a = again.workbook.sheets[0]
        .canvas
        .objects
        .iter()
        .find(|o| o.drawio.as_ref().and_then(|d| d.source_id.as_deref()) == Some("node-a"))
        .unwrap();
    assert_eq!(a.data, reopened.sheets[0].canvas.objects[0].data);
}

#[test]
fn drawio_object_data_imports_edits_and_exports() {
    let xml = page(concat!(
        r#"<UserObject label="Web" id="web" tooltip="Front door" link="https://example.com/web" tags="prod  web prod" subnet="10.0.0.0/24" placeholder="1"><mxCell style="rounded=0;whiteSpace=wrap;html=1;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="40" as="geometry"/></mxCell></UserObject>"#,
        r#"<mxCell id="db" value="DB" style="ellipse;whiteSpace=wrap;html=1;" vertex="1" parent="1"><mxGeometry x="200" y="10" width="80" height="40" as="geometry"/></mxCell>"#,
        r#"<UserObject label="reads" id="flow" link="data:page/id,page-2" protocol="tcp"><mxCell style="endArrow=classic;html=1;" edge="1" parent="1" source="web" target="db"><mxGeometry relative="1" as="geometry"/></mxCell></UserObject>"#,
    ));
    let report = load(&xml);
    assert!(
        !report.warnings.contains(&ImportWarning::Links),
        "{:?}",
        report.warnings
    );
    let sheet = &report.workbook.sheets[0];
    let source = |id: &str| {
        sheet
            .canvas
            .objects
            .iter()
            .find(|o| o.drawio.as_ref().and_then(|d| d.source_id.as_deref()) == Some(id))
            .unwrap()
    };
    let web = source("web");
    assert_eq!(web.alt_text, "Front door");
    let data = web.data.as_ref().unwrap();
    assert_eq!(data.link.as_deref(), Some("https://example.com/web"));
    assert_eq!(data.tags, ["prod", "web"]);
    assert_eq!(data.properties, [("subnet".into(), "10.0.0.0/24".into())]);
    // `placeholder` has a draw.io meaning of its own and stays passthrough.
    assert_eq!(
        web.drawio.as_ref().unwrap().attributes,
        [("placeholder".into(), "1".into())]
    );
    let flow = source("flow").data.as_ref().unwrap();
    assert_eq!(flow.page_link(), Some("page-2"));
    assert_eq!(flow.property("protocol"), Some("tcp"));
    assert_eq!(source("db").data, None);

    let mut book = report.workbook.clone();
    let id = source("web").id;
    let object = book.sheets[0].canvas.object_mut(id).unwrap();
    let mut edited = (**object.data.as_ref().unwrap()).clone();
    edited.tags = vec!["edited".into()];
    edited.properties.push(("owner".into(), "Ada & Bob".into()));
    object.data = Some(Arc::new(edited));
    let out = String::from_utf8(export(&book).unwrap()).unwrap();
    for needle in [
        r#"tooltip="Front door""#,
        r#"link="https://example.com/web" tags="edited" subnet="10.0.0.0/24" owner="Ada &amp; Bob" placeholder="1""#,
        r#"link="data:page/id,page-2" protocol="tcp""#,
    ] {
        assert!(out.contains(needle), "missing {needle}: {out}");
    }
    let again = load(&out).workbook;
    let reopened: Vec<_> = again.sheets[0]
        .canvas
        .objects
        .iter()
        .map(|o| o.data.clone())
        .collect();
    let expected: Vec<_> = book.sheets[0]
        .canvas
        .objects
        .iter()
        .map(|o| o.data.clone())
        .collect();
    assert_eq!(reopened, expected);
    assert!(!book.masked_copy().has_object_data());
}

#[test]
fn drawio_document_attributes_round_trip_through_native_workbooks() {
    let xml = r#"<mxfile host="app.diagrams.net" type="device" customOwner="team-a"><diagram name="Page"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#;
    let report = crate::import_bytes_with_report(Format::DrawIo, xml.as_bytes()).unwrap();
    assert!(report.warnings.is_empty());
    assert_eq!(
        report.workbook.drawio.as_ref().unwrap().attributes,
        [("customOwner".into(), "team-a".into())]
    );
    assert_eq!(crate::required_native_version(&report.workbook), 53);

    let native = crate::formats::native_export(&report.workbook).unwrap();
    let reopened = crate::formats::native_import(&native).unwrap();
    assert_eq!(reopened.drawio, report.workbook.drawio);
    let content = serde_json::to_vec(&reopened).unwrap();
    assert!(matches!(
        crate::formats::native_import(&content),
        Err(IoError::NewerVersion(_))
    ));
    let v52 = crate::formats::archive(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":52,"resources":[]}"#,
        ),
        ("content.json", &content),
    ])
    .unwrap();
    assert!(matches!(
        crate::formats::native_import(&v52),
        Err(IoError::NewerVersion(_))
    ));
    let exported = String::from_utf8(export(&reopened).unwrap()).unwrap();
    assert!(exported.contains("customOwner=\"team-a\""), "{exported}");
    assert!(
        exported.contains("host=\"Thinoda\" type=\"device\""),
        "{exported}"
    );
    assert!(reopened.masked_copy().drawio.is_none());
    let mut invalid = reopened.clone();
    invalid.drawio = Some(std::sync::Arc::new(
        thinoda_core::drawio_passthrough::DrawioDocumentPassthrough {
            attributes: vec![("bad\" name".into(), "x".into())],
        },
    ));
    assert!(export(&invalid).is_err());

    let unsupported = r#"<mxfile xmlns:future="urn:future"><diagram><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#;
    let report = crate::import_bytes_with_report(Format::DrawIo, unsupported.as_bytes()).unwrap();
    assert_eq!(report.warnings, [ImportWarning::Other]);
}

#[test]
fn drawio_page_passthrough_reports_dropped_attributes_and_invalid_ids() {
    let large = "x".repeat(thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2);
    let xml = format!(
        r#"<mxfile><diagram id="{}" name="Page"><mxGraphModel xmlns:future="urn:future" oversized="{large}" kept="yes"><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#,
        "p".repeat(thinoda_core::drawio_passthrough::MAX_SOURCE_ID_BYTES + 1)
    );
    let report = import(xml.as_bytes()).unwrap();
    assert!(report.warnings.contains(&ImportWarning::Other));
    let page = report.workbook.active().drawio.as_ref().unwrap();
    assert!(page.diagram_id.is_none());
    assert_eq!(page.model_attributes, [("kept".into(), "yes".into())]);
    let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    assert!(exported.contains("kept=\"yes\""));
    assert!(!exported.contains("oversized="));
    assert!(!exported.contains("xmlns:future"));
}

#[test]
fn drawio_document_passthrough_reports_oversized_attributes_without_losing_other_data() {
    let large = "x".repeat(thinoda_core::drawio_passthrough::MAX_DOCUMENT_PASSTHROUGH_BYTES);
    let xml = format!(
        r#"<mxfile oversized="{large}" kept="A &amp; B"><diagram><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#
    );
    let report = import(xml.as_bytes()).unwrap();
    assert_eq!(report.warnings, [ImportWarning::Other]);
    assert_eq!(
        report.workbook.drawio.as_ref().unwrap().attributes,
        [("kept".into(), "A & B".into())]
    );
    let native = crate::formats::native_export(&report.workbook).unwrap();
    let reopened = crate::formats::native_import(&native).unwrap();
    let exported = String::from_utf8(export(&reopened).unwrap()).unwrap();
    assert!(exported.contains("kept=\"A &amp; B\""));
    assert!(!exported.contains("oversized="));

    let at_limit =
        "x".repeat(thinoda_core::drawio_passthrough::MAX_DOCUMENT_PASSTHROUGH_BYTES - "full".len());
    let xml = format!(
        r#"<mxfile full="{at_limit}" extra="x"><diagram><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#
    );
    let report = import(xml.as_bytes()).unwrap();
    assert_eq!(report.warnings, [ImportWarning::Other]);
    let document = report.workbook.drawio.as_ref().unwrap();
    assert_eq!(
        document.byte_len(),
        thinoda_core::drawio_passthrough::MAX_DOCUMENT_PASSTHROUGH_BYTES
    );
    assert_eq!(document.attributes.len(), 1);
    assert_eq!(document.attributes[0].0, "full");
}

#[test]
fn drawio_page_passthrough_rejects_reserved_and_oversized_attributes() {
    let mut book = Workbook::new(Sheet::new("Page", "Root"));
    book.active_mut().drawio = Some(Arc::new(DrawioPagePassthrough {
        model_attributes: vec![("grid".into(), "0".into())],
        ..Default::default()
    }));
    assert!(book.validate().is_err());
    assert!(export(&book).is_err());
    book.active_mut().drawio = Some(Arc::new(DrawioPagePassthrough {
        model_attributes: vec![(
            "large".into(),
            "x".repeat(thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES),
        )],
        ..Default::default()
    }));
    assert!(book.active().canvas.objects.is_empty());
    assert!(book.validate().is_err());
    assert!(export(&book).is_err());
}

#[test]
fn drawio_import_reports_unsupported_document_and_page_xml() {
    let graph =
        r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel>"#;
    let cases = [
        format!(r#"<mxfile><future/><diagram>{graph}</diagram></mxfile>"#),
        format!(r#"<mxfile>unexpected<diagram>{graph}</diagram></mxfile>"#),
        format!(r#"<mxfile><diagram custom="future">{graph}</diagram></mxfile>"#),
        format!(r#"<mxfile><diagram>{graph}<future/></diagram></mxfile>"#),
        r#"<mxfile><diagram><mxGraphModel><future/><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#.into(),
        r#"<mxfile><diagram><mxGraphModel><root custom="future"><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#.into(),
    ];
    for xml in cases {
        let report = import(xml.as_bytes()).unwrap();
        assert!(report.warnings.contains(&ImportWarning::Other), "{xml}");
    }

    let xml = format!(
        r#"<mxfile><diagram name="First">{graph}</diagram><diagram name="Second" custom="future">{graph}</diagram></mxfile>"#
    );
    let (report, first) = import_with_first_page_warnings(xml.as_bytes()).unwrap();
    assert!(first.is_empty());
    assert_eq!(report.warnings, [ImportWarning::Other]);
}

#[test]
fn drawio_import_reports_unrepresented_cell_and_geometry_xml() {
    let geometry = r#"<mxGeometry x="0" y="0" width="100" height="60" as="geometry"/>"#;
    let baseline =
        format!(r#"<mxCell id="shape" value="Label" vertex="1" parent="1">{geometry}</mxCell>"#);
    assert!(load(&page(&baseline)).warnings.is_empty());
    let cases = [
        baseline.replace("vertex=\"1\"", "vertex=\"1\" custom=\"data\""),
        baseline.replace("vertex=\"1\"", "vertex=\"1\" connectable=\"0\""),
        baseline.replace("</mxCell>", "<future/></mxCell>"),
        baseline.replace(geometry, &format!("{geometry}{geometry}")),
        baseline.replace("as=\"geometry\"", "as=\"geometry\" custom=\"data\""),
        baseline.replace("as=\"geometry\"", "as=\"geometry\" relative=\"true\""),
        baseline.replace(
            geometry,
            r#"<mxGeometry width="100" height="60" as="geometry"><mxPoint as="offset" x="1" custom="data"/></mxGeometry>"#,
        ),
        baseline.replace(
            geometry,
            r#"<mxGeometry width="100" height="60" as="geometry"><mxPoint as="offset" x="1"/><mxPoint as="offset" x="2"/></mxGeometry>"#,
        ),
        baseline.replace(
            geometry,
            r#"<mxGeometry width="100" height="60" as="geometry"><Array as="points"><future/></Array></mxGeometry>"#,
        ),
        format!(
            r#"<object id="shape" label="Label"><mxCell vertex="1" parent="1">{geometry}</mxCell><future/></object>"#
        ),
    ];
    for cell in cases {
        let report = load(&page(&cell));
        assert!(report.warnings.contains(&ImportWarning::Other), "{cell}");
    }
}

#[test]
fn drawio_import_keeps_parentless_vertices_and_edges_with_a_warning() {
    let xml = page(concat!(
        r#"<mxCell id="shape" value="Parentless" vertex="1"><mxGeometry x="10" y="20" width="100" height="60" as="geometry"/></mxCell>"#,
        r#"<mxCell id="edge" edge="1"><mxGeometry relative="1" as="geometry"><mxPoint x="10" y="20" as="sourcePoint"/><mxPoint x="100" y="80" as="targetPoint"/></mxGeometry></mxCell>"#,
    ));
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    let sheet = &report.workbook.sheets[0];
    assert_eq!(shapes(sheet).len(), 1);
    assert_eq!(shapes(sheet)[0].label(), "Parentless");
    assert_eq!(connectors(sheet).len(), 1);
    let reopened = import(&export(&report.workbook).unwrap()).unwrap();
    assert_eq!(shapes(&reopened.workbook.sheets[0]).len(), 1);
    assert_eq!(connectors(&reopened.workbook.sheets[0]).len(), 1);
}

#[test]
fn drawio_import_reports_missing_cell_ids() {
    let xml = page(
        r#"<mxCell value="No id" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "No id");
}

#[test]
fn drawio_import_reports_unrepresented_root_and_layer_data() {
    let root = r#"<mxCell id="0"/><mxCell id="1" parent="0"/>"#;
    let wrap = |cells: &str| {
        format!(
            "<mxfile><diagram><mxGraphModel><root>{cells}</root></mxGraphModel></diagram></mxfile>"
        )
    };
    assert!(load(&wrap(root)).warnings.is_empty());
    let cases = [
        root.replace(r#"id="0"/>"#, r#"id="0" value="root data"/>"#),
        root.replace(r#"id="0"/>"#, r#"id="0" visible="0"/>"#),
        root.replace(
            r#"id="0"/>"#,
            r#"id="0"><mxGeometry as="geometry"/></mxCell>"#,
        ),
        root.replace(r#"id="1""#, r#"id="base""#),
        root.replace(r#"id="1""#, r#"id="1" style="custom=1;""#),
        root.replace(r#"id="1""#, r#"id="1" value="Background""#),
        format!(r#"{root}<mxCell id="l" value="  Layer  " parent="0"/>"#),
        format!(r#"{root}<mxCell id="l" value="Layer" style="custom=1;" parent="0"/>"#),
        r#"<mxCell id="0"/><mxCell id="1" value="Named" parent="0"/>"#.into(),
    ];
    for cells in cases {
        let report = load(&wrap(&cells));
        assert!(report.warnings.contains(&ImportWarning::Other), "{cells}");
    }
    let supported =
        format!(r#"{root}<mxCell id="l" value="Locked" style="locked=1;" parent="0"/>"#);
    assert!(load(&wrap(&supported)).warnings.is_empty());
}

#[test]
fn drawio_import_reports_flattened_nested_structural_cells() {
    for metadata in ["", r#" value="hidden data""#, r#" style="custom=1;""#] {
        let xml = page(&format!(
            r#"<mxCell id="structural" parent="1"{metadata}/><mxCell id="shape" value="Kept" vertex="1" parent="structural"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert!(report.warnings.contains(&ImportWarning::Other), "{xml}");
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Kept");
    }
}

#[test]
fn drawio_import_reports_hidden_or_empty_edge_children() {
    let edge = r##"<mxCell id="e" style="endArrow=none;strokeColor=#000;fontColor=#000000;fontSize=12;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"##;
    let label = r#"<mxCell id="child" value="Label" style="edgeLabel;fontSize=12;fontColor=#000000;" vertex="1" parent="e"><mxGeometry x="0.5" relative="1" as="geometry"/></mxCell>"#;
    assert!(load(&page(&format!("{edge}{label}"))).warnings.is_empty());
    for child in [
        label.replace("vertex=\"1\"", "vertex=\"1\" visible=\"0\""),
        label.replace("value=\"Label\"", "value=\"\""),
    ] {
        let report = load(&page(&format!("{edge}{child}")));
        assert!(report.warnings.contains(&ImportWarning::Other), "{child}");
        assert_eq!(connectors(&report.workbook.sheets[0]).len(), 1);
    }
}

#[test]
fn drawio_import_reports_unrepresented_edge_label_geometry() {
    let edge = r##"<mxCell id="e" style="endArrow=none;strokeColor=#000;fontColor=#000000;fontSize=12;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"##;
    for (attributes, geometry) in [
        (r#" collapsed="1""#, ""),
        (r#" source="e""#, ""),
        ("", r#"<mxPoint x="4" y="5" as="sourcePoint"/>"#),
        ("", r#"<mxPoint x="4" y="5" as="targetPoint"/>"#),
        ("", r#"<Array as="points"><mxPoint x="4" y="5"/></Array>"#),
        (
            "",
            r#"<mxRectangle x="4" y="5" width="6" height="7" as="alternateBounds"/>"#,
        ),
    ] {
        let label = format!(
            r##"<mxCell id="child" value="Label" style="edgeLabel;fontSize=12;fontColor=#000000;" vertex="1" parent="e"{attributes}><mxGeometry x="0.5" relative="1" as="geometry">{geometry}</mxGeometry></mxCell>"##
        );
        let report = load(&page(&format!("{edge}{label}")));
        assert!(report.warnings.contains(&ImportWarning::Other), "{label}");
        assert_eq!(
            connectors(&report.workbook.sheets[0])[0].extra_labels[0].text,
            "Label"
        );
    }
}

#[test]
fn drawio_import_warns_about_edge_label_background_colors() {
    let edge = r##"<mxCell id="e" style="endArrow=none;strokeColor=#000;fontColor=#000000;fontSize=12;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"##;
    for color in ["none", "#ff0000"] {
        let label = format!(
            r##"<mxCell id="child" value="Label" style="edgeLabel;fontSize=12;fontColor=#000000;labelBackgroundColor={color};" vertex="1" parent="e"><mxGeometry relative="1" as="geometry"/></mxCell>"##
        );
        let report = load(&page(&format!("{edge}{label}")));
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{color}"
        );
        assert_eq!(
            connectors(&report.workbook.sheets[0])[0].extra_labels[0].text,
            "Label"
        );
    }
}

#[test]
fn drawio_import_warns_about_invalid_edge_label_text_opacity() {
    let edge = r##"<mxCell id="e" style="endArrow=none;strokeColor=#000;fontColor=#000000;fontSize=12;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"##;
    for opacity in ["invalid", "101"] {
        let label = format!(
            r##"<mxCell id="child" value="Label" style="edgeLabel;fontSize=12;fontColor=#000000;textOpacity={opacity};" vertex="1" parent="e"><mxGeometry relative="1" as="geometry"/></mxCell>"##
        );
        let report = load(&page(&format!("{edge}{label}")));
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{opacity}"
        );
        assert_eq!(
            connectors(&report.workbook.sheets[0])[0].extra_labels[0].text,
            "Label"
        );
    }
}

#[test]
fn drawio_passthrough_never_overrides_edits_or_reuses_ids() {
    let xml = page(concat!(
        r#"<mxCell id="t1" value="A" style="rounded=0;whiteSpace=wrap;html=1;dashed=1;dashPattern=1 4;fillColor=#ff0000;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="gone" value="B" style="rounded=0;whiteSpace=wrap;html=1;" vertex="1" parent="1"><mxGeometry x="200" y="10" width="80" height="40" as="geometry"/></mxCell>"#,
    ));
    let mut book = load(&xml).workbook;
    let sheet = &mut book.sheets[0];
    let removed = sheet
        .canvas
        .objects
        .iter()
        .find(|o| o.drawio.as_ref().and_then(|d| d.source_id.as_deref()) == Some("gone"))
        .unwrap()
        .id;
    sheet.canvas.objects.retain(|o| o.id != removed);
    let first = &mut sheet.canvas.objects[0];
    first.style.fill = None;
    let mut copy = first.clone();
    copy.drawio = Passthrough::for_copy(&copy.drawio);
    assert!(copy.drawio.as_ref().is_none_or(|d| d.source_id.is_none()));
    let out = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(!out.contains("fillColor=#ff0000"), "{out}");
    assert!(out.contains("dashed=1;dashPattern=1 4;"), "{out}");
    assert!(!out.contains("id=\"gone\""), "{out}");
    // The hidden root no longer steals the shape's source id.
    assert!(out.contains("id=\"t1\""), "{out}");
    let masked = book.masked_copy();
    assert!(!masked.has_drawio_passthrough());
}

#[test]
fn drawio_generated_edge_labels_do_not_reuse_deleted_object_ids() {
    let xml = page(concat!(
        r##"<mxCell id="e" value="main" style="endArrow=none;strokeColor=#000;fontColor=#000000;fontSize=12;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"##,
        r#"<mxCell id="extra" value="Extra" style="edgeLabel;fontSize=12;fontColor=#000000;" vertex="1" parent="e"><mxGeometry x="0.5" relative="1" as="geometry"/></mxCell>"#,
        r#"<mxCell id="e-label-1" value="Delete" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
    ));
    let mut book = load(&xml).workbook;
    let sheet = book.active_mut();
    assert!(
        sheet
            .drawio
            .as_ref()
            .unwrap()
            .cell_ids
            .iter()
            .any(|id| id == "e-label-1")
    );
    sheet.canvas.objects.retain(|object| {
        object
            .drawio
            .as_ref()
            .and_then(|passthrough| passthrough.source_id.as_deref())
            != Some("e-label-1")
    });
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(!xml.contains(r#"id="e-label-1""#), "{xml}");
    assert!(xml.contains(r#"id="e-label-1x""#), "{xml}");
    let reopened = import(xml.as_bytes()).unwrap();
    assert_eq!(
        connectors(reopened.workbook.active())[0].extra_labels.len(),
        1
    );
}

#[test]
fn drawio_generated_zones_and_groups_do_not_reuse_source_ids() {
    let mut sheet = Sheet::new("Page", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let zone = sheet
        .create_board_frame("Zone", rect(100.0, 100.0, 400.0, 300.0))
        .unwrap();
    let first = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(140.0, 150.0, 80.0, 60.0),
            "First",
        ))
        .unwrap();
    let second = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(260.0, 180.0, 80.0, 60.0),
            "Second",
        ))
        .unwrap();
    sheet
        .set_zone_members(zone, vec![], vec![first, second])
        .unwrap();
    let group = sheet.group_canvas_objects(&[first, second]).unwrap();
    let zone_id = format!("z{}", zone.0);
    let group_id = format!("g{}", group.0);
    sheet.canvas.object_mut(first).unwrap().drawio = Some(Arc::new(Passthrough {
        source_id: Some(group_id.clone()),
        ..Default::default()
    }));
    sheet.drawio = Some(Arc::new(DrawioPagePassthrough {
        cell_ids: vec![zone_id.clone(), format!("{zone_id}-1")],
        ..Default::default()
    }));
    let book = Workbook::new(sheet);
    book.validate().unwrap();
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(!xml.contains(&format!(r#"id="{zone_id}""#)), "{xml}");
    assert!(xml.contains(&format!(r#"id="{zone_id}-2""#)), "{xml}");
    assert!(xml.contains(&format!(r#"id="{group_id}-1""#)), "{xml}");
    assert!(xml.contains(&format!(r#"parent="{zone_id}-2""#)), "{xml}");
    assert!(xml.contains(&format!(r#"parent="{group_id}-1""#)), "{xml}");
    assert!(xml.contains(&format!(r#"id="{group_id}""#)), "{xml}");
    let reopened = load(&xml).workbook;
    let objects = shapes(reopened.active());
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].rect.min, Vec2::new(140.0, 150.0));
    assert_eq!(objects[1].rect.min, Vec2::new(260.0, 180.0));
}

#[test]
fn drawio_generated_map_cells_avoid_source_ids() {
    let mut sheet = Sheet::new("Root", "Root");
    let first = sheet.add_child(sheet.root, None, "First");
    let second = sheet.add_child(sheet.root, None, "Second");
    sheet.add_boundary(&[first, second]).unwrap();
    sheet.add_relationship(first, second).unwrap();
    sheet.add_summary(&[first, second], "Summary").unwrap();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    assert!(!layout.boundaries.is_empty());
    assert!(!layout.connectors.is_empty());
    assert!(!layout.relationships.is_empty());
    assert!(!layout.summaries.is_empty());
    let mut ids = Vec::new();
    ids.extend(layout.boundaries.iter().map(|b| format!("b{}", b.id.0)));
    ids.extend((0..layout.connectors.len()).map(|n| format!("e{n}")));
    ids.extend(layout.relationships.iter().map(|r| format!("r{}", r.id.0)));
    ids.extend(layout.summaries.iter().map(|s| format!("s{}", s.id.0)));
    let object = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(400.0, 400.0, 80.0, 60.0),
            "Source e0",
        ))
        .unwrap();
    sheet.canvas.object_mut(object).unwrap().drawio = Some(Arc::new(Passthrough {
        source_id: Some("e0".into()),
        ..Default::default()
    }));
    sheet.drawio = Some(Arc::new(DrawioPagePassthrough {
        cell_ids: ids.iter().filter(|id| *id != "e0").cloned().collect(),
        ..Default::default()
    }));
    let book = Workbook::new(sheet);
    book.validate().unwrap();
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    for id in ids {
        assert_eq!(
            xml.matches(&format!(r#"id="{id}""#)).count(),
            usize::from(id == "e0"),
            "{xml}"
        );
        assert!(xml.contains(&format!(r#"id="{id}-1""#)), "{xml}");
    }
    let restored = load(&xml);
    assert!(connectors(restored.workbook.active()).len() >= layout.connectors.len());
}

#[test]
fn drawio_generated_topic_ids_avoid_sources_and_keep_connector_targets() {
    let mut sheet = Sheet::new("Page", "Root");
    let root = sheet.root;
    let child = sheet.add_child(root, None, "Child");
    let original_root = format!("t{}", root.0);
    let original_child = format!("t{}", child.0);
    let object = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(400.0, 400.0, 80.0, 60.0),
            "Source object",
        ))
        .unwrap();
    sheet.canvas.object_mut(object).unwrap().drawio = Some(Arc::new(Passthrough {
        source_id: Some(original_root.clone()),
        ..Default::default()
    }));
    sheet.drawio = Some(Arc::new(DrawioPagePassthrough {
        cell_ids: vec![original_child.clone()],
        ..Default::default()
    }));
    let book = Workbook::new(sheet);
    book.validate().unwrap();
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(xml.contains(&format!(r#"id="{original_root}""#)), "{xml}");
    assert!(xml.contains(&format!(r#"id="{original_root}-1""#)), "{xml}");
    assert!(!xml.contains(&format!(r#"id="{original_child}""#)), "{xml}");
    assert!(
        xml.contains(&format!(r#"id="{original_child}-1""#)),
        "{xml}"
    );
    assert!(
        xml.contains(&format!(r#"source="{original_root}-1""#)),
        "{xml}"
    );
    assert!(
        xml.contains(&format!(r#"target="{original_child}-1""#)),
        "{xml}"
    );
    let restored = load(&xml);
    assert_eq!(connectors(restored.workbook.active()).len(), 1);
}

#[test]
fn sankey_shape_node_conversion_discloses_lost_shape_and_retains_identity() {
    let mut book = thinoda_core::templates::instantiate("sankey-budget");
    let node = *thinoda_core::sankey::evaluate(book.active())
        .unwrap()
        .nodes
        .iter()
        .find(|id| book.active().topics[id].card.is_some())
        .unwrap();
    let topic = book.active_mut().topics.get_mut(&node).unwrap();
    topic.hyperlink = Some("https://example.com/retained".into());
    topic
        .inline_text
        .update_range(&topic.title, 0..1, |style| style.bold = Some(true))
        .unwrap();
    let title = topic.title.clone();
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    let xml = sankey_node_as_shape(&xml, node);
    let report = load(&xml);
    assert!(report.workbook.has_sankey());
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    assert!(!report.warnings.contains(&ImportWarning::Other));
    assert!(!report.warnings.contains(&ImportWarning::DrawioSankeyStale));
    let topic = report
        .workbook
        .active()
        .topics
        .values()
        .find(|topic| topic.title == title)
        .unwrap();
    assert_eq!(
        topic.hyperlink.as_deref(),
        Some("https://example.com/retained")
    );
    assert_eq!(
        topic.drawio_source_id.as_deref(),
        Some(format!("t{}", node.0).as_str())
    );
    assert_eq!(topic.inline_text.runs[0].style.bold, Some(true));
    assert_eq!(topic.card.unwrap().color, Some(Color::rgb(255, 255, 255)));
    assert_eq!(topic.style.text_color, Some(Color::rgb(18, 52, 86)));
    assert_eq!(topic.style.align, Some(TextAlign::Right));

    let native = crate::formats::native_export(&report.workbook).unwrap();
    let reopened = crate::formats::native_import(&native).unwrap();
    let exported = export(&reopened).unwrap();
    let restored = import(&exported).unwrap();
    assert!(restored.workbook.has_sankey());
    let topic = restored
        .workbook
        .active()
        .topics
        .values()
        .find(|topic| topic.title == title)
        .unwrap();
    assert_eq!(
        topic.hyperlink.as_deref(),
        Some("https://example.com/retained")
    );
    assert_eq!(
        topic.drawio_source_id.as_deref(),
        Some(format!("t{}", node.0).as_str())
    );
    assert_eq!(topic.inline_text.runs[0].style.bold, Some(true));
    assert_eq!(topic.card.unwrap().color, Some(Color::rgb(255, 255, 255)));
    assert_eq!(topic.style.text_color, Some(Color::rgb(18, 52, 86)));
    assert_eq!(topic.style.align, Some(TextAlign::Right));
}

fn sankey_node_as_shape(xml: &str, node: TopicId) -> String {
    let id = format!(r#"id="t{}""#, node.0);
    let line = xml.lines().find(|line| line.contains(&id)).unwrap();
    let start = line.find("style=\"").unwrap() + "style=\"".len();
    let end = start + line[start..].find('"').unwrap();
    let replacement = format!(
        "{}shape=rectangle;html=1;fillColor=#fff;strokeColor=#000;fontColor=#123456;align=right;{}",
        &line[..start],
        &line[end..]
    );
    xml.replacen(line, &replacement, 1)
}

#[test]
fn sankey_shape_node_metadata_refuses_conversion_without_losing_prior_nodes() {
    let mut book = thinoda_core::templates::instantiate("sankey-budget");
    let mut nodes: Vec<_> = thinoda_core::sankey::evaluate(book.active())
        .unwrap()
        .nodes
        .iter()
        .filter(|id| book.active().topics[id].card.is_some())
        .copied()
        .collect();
    nodes.sort();
    let [first, last, ..] = nodes.as_slice() else {
        panic!("fixture requires two Sankey cards")
    };
    book.active_mut().topics.get_mut(last).unwrap().hyperlink =
        Some("https://example.com/retained".into());
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    let xml = sankey_node_as_shape(&sankey_node_as_shape(&xml, *first), *last);
    let needle = format!(r#"id="t{}" link="https://example.com/retained""#, last.0);
    assert!(xml.contains(&needle));
    let xml = xml.replacen(
        &needle,
        &format!(r#"{needle} tooltip="Retained tooltip" custom="Retained data""#),
        1,
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::DrawioSankeyStale));
    assert!(!report.warnings.contains(&ImportWarning::Formatting));
    assert!(!report.warnings.contains(&ImportWarning::Other));
    assert!(!report.workbook.has_sankey());
    for node in [first, last] {
        let id = format!("t{}", node.0);
        assert!(
            report
                .workbook
                .active()
                .canvas
                .objects
                .iter()
                .any(|object| {
                    object
                        .drawio
                        .as_ref()
                        .and_then(|data| data.source_id.as_deref())
                        == Some(id.as_str())
                })
        );
    }
    let object = report
        .workbook
        .active()
        .canvas
        .objects
        .iter()
        .find(|object| object.alt_text == "Retained tooltip")
        .unwrap();
    let data = object.data.as_ref().unwrap();
    assert_eq!(data.link.as_deref(), Some("https://example.com/retained"));
    assert_eq!(data.property("custom"), Some("Retained data"));
    let native = crate::formats::native_export(&report.workbook).unwrap();
    let reopened = crate::formats::native_import(&native).unwrap();
    let restored = import(&export(&reopened).unwrap()).unwrap();
    let object = restored
        .workbook
        .active()
        .canvas
        .objects
        .iter()
        .find(|object| object.alt_text == "Retained tooltip")
        .unwrap();
    assert_eq!(
        object.data.as_ref().unwrap().link.as_deref(),
        Some("https://example.com/retained")
    );
    assert_eq!(
        object.data.as_ref().unwrap().property("custom"),
        Some("Retained data")
    );
}

#[test]
fn sankey_shape_node_retains_tooltip_and_unknown_style() {
    for kind in ["tooltip", "style"] {
        let mut book = thinoda_core::templates::instantiate("sankey-budget");
        let node = *thinoda_core::sankey::evaluate(book.active())
            .unwrap()
            .nodes
            .iter()
            .find(|id| book.active().topics[id].card.is_some())
            .unwrap();
        let topic = book.active_mut().topics.get_mut(&node).unwrap();
        if kind == "tooltip" {
            topic.hyperlink = Some("https://example.com".into());
        }
        let xml = String::from_utf8(export(&book).unwrap()).unwrap();
        let mut xml = sankey_node_as_shape(&xml, node);
        match kind {
            "tooltip" => {
                xml = xml.replacen(
                    r#"link="https://example.com""#,
                    r#"tooltip="Retained tooltip""#,
                    1,
                );
            }
            "style" => {
                let id = format!(r#"id="t{}""#, node.0);
                let line = xml.lines().find(|line| line.contains(&id)).unwrap();
                let changed =
                    line.replacen("shape=rectangle;", "shape=rectangle;customKey=retained;", 1);
                xml = xml.replacen(line, &changed, 1);
            }
            _ => {}
        }
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::DrawioSankeyStale),
            "{kind}: {:?}",
            report.warnings
        );
        let id = format!("t{}", node.0);
        let object = report
            .workbook
            .active()
            .canvas
            .objects
            .iter()
            .find(|object| {
                object
                    .drawio
                    .as_ref()
                    .and_then(|data| data.source_id.as_deref())
                    == Some(id.as_str())
            })
            .unwrap();
        match kind {
            "tooltip" => assert_eq!(object.alt_text, "Retained tooltip"),
            "style" => assert!(
                object
                    .drawio
                    .as_ref()
                    .unwrap()
                    .style
                    .contains(&("customKey".into(), "retained".into()))
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn sankey_shape_node_retains_lock_and_container_membership() {
    for kind in ["locked", "layer", "group", "zone", "scene"] {
        let book = thinoda_core::templates::instantiate("sankey-budget");
        let node = *thinoda_core::sankey::evaluate(book.active())
            .unwrap()
            .nodes
            .iter()
            .find(|id| book.active().topics[id].card.is_some())
            .unwrap();
        let xml = String::from_utf8(export(&book).unwrap()).unwrap();
        let mut xml = sankey_node_as_shape(&xml, node);
        let id = format!(r#"id="t{}""#, node.0);
        let line = xml.lines().find(|line| line.contains(&id)).unwrap();
        let changed = if kind == "locked" {
            line.replacen("shape=rectangle;", "shape=rectangle;locked=1;", 1)
        } else {
            line.replacen(r#"parent="1""#, r#"parent="holder""#, 1)
        };
        assert_ne!(line, changed);
        xml = xml.replacen(line, &changed, 1);
        let holder = match kind {
            "layer" => r#"<mxCell id="holder" value="Layer" parent="0"/>"#,
            "group" => concat!(
                r#"<mxCell id="holder" style="group;" vertex="1" parent="1"><mxGeometry width="600" height="400" as="geometry"/></mxCell>"#,
                r#"<mxCell id="peer" value="Peer" style="shape=rectangle;" vertex="1" parent="holder"><mxGeometry width="20" height="20" as="geometry"/></mxCell>"#
            ),
            "zone" => {
                r#"<mxCell id="holder" value="Zone" style="swimlane;" vertex="1" parent="1"><mxGeometry width="600" height="400" as="geometry"/></mxCell>"#
            }
            "scene" => {
                r#"<mxCell id="holder" value="Container" style="swimlane;thinodaSceneContainer=1;" vertex="1" parent="1"><mxGeometry width="600" height="400" as="geometry"/></mxCell>"#
            }
            _ => "",
        };
        xml = xml.replacen("</root>", &format!("{holder}</root>"), 1);
        let report = load(&xml);
        assert!(
            report.warnings.contains(&ImportWarning::DrawioSankeyStale),
            "{kind}: {:?}",
            report.warnings
        );
        let sheet = report.workbook.active();
        let source = format!("t{}", node.0);
        let object = sheet
            .canvas
            .objects
            .iter()
            .find(|object| {
                object
                    .drawio
                    .as_ref()
                    .and_then(|data| data.source_id.as_deref())
                    == Some(source.as_str())
            })
            .unwrap();
        match kind {
            "locked" => assert!(object.locked),
            "layer" => assert!(object.layer.is_some()),
            "group" => assert!(object.group.is_some()),
            "zone" => assert!(
                sheet
                    .zones
                    .iter()
                    .any(|zone| zone.objects.contains(&object.id))
            ),
            "scene" => assert!(sheet.canvas.scene.parents.contains_key(&object.id)),
            _ => unreachable!(),
        }
    }
}

#[test]
fn drawio_sankey_source_id_collisions_remap_without_losing_semantics() {
    for template in [
        "sankey-budget",
        "sankey-probability",
        "sankey-learning",
        "sankey-circular",
    ] {
        let mut book = thinoda_core::templates::instantiate(template);
        let before = thinoda_core::sankey::evaluate(book.active()).unwrap();
        let node = *before.nodes.iter().next().unwrap();
        let edge = book
            .active()
            .canvas
            .objects
            .iter()
            .find(|object| matches!(&object.kind, CanvasObjectKind::Connector(c) if c.sankey.is_some()))
            .unwrap()
            .id;
        let node_id = format!("t{}", node.0);
        let edge_id = format!("o{}", edge.0);
        book.active_mut().drawio = Some(Arc::new(DrawioPagePassthrough {
            cell_ids: vec![node_id.clone(), edge_id.clone()],
            ..Default::default()
        }));
        let object = book
            .active_mut()
            .add_canvas_object(shape(
                ShapeKind::Rectangle,
                rect(800.0, 800.0, 80.0, 60.0),
                "Source object",
            ))
            .unwrap();
        book.active_mut().canvas.object_mut(object).unwrap().drawio = Some(Arc::new(Passthrough {
            source_id: Some(edge_id.clone()),
            ..Default::default()
        }));
        let xml = String::from_utf8(export(&book).unwrap()).unwrap();
        assert!(!xml.contains(&format!(r#"id="{node_id}""#)), "{template}");
        assert_eq!(xml.matches(&format!(r#"id="{edge_id}""#)).count(), 1);
        let report = load(&xml);
        assert!(
            !report.warnings.contains(&ImportWarning::DrawioSankeyStale),
            "{template}: {:?}",
            report.warnings
        );
        assert!(report.workbook.has_sankey(), "{template}");
        let after = thinoda_core::sankey::evaluate(report.workbook.active()).unwrap();
        let mut before_mass: Vec<_> = before.mass.values().copied().collect();
        let mut after_mass: Vec<_> = after.mass.values().copied().collect();
        before_mass.sort_by(f64::total_cmp);
        after_mass.sort_by(f64::total_cmp);
        assert_eq!(after_mass, before_mass, "{template}");
        assert_eq!(before.nodes.len(), after.nodes.len(), "{template}");
        if template == "sankey-budget" {
            for format in [Format::DrawIoSvg, Format::DrawIoPng] {
                let bytes = crate::export_bytes_with_consent(
                    &book,
                    format,
                    &ExportOptions::default(),
                    true,
                )
                .unwrap();
                let restored = import(&bytes).unwrap();
                assert!(restored.workbook.has_sankey(), "{format:?}");
                assert!(
                    !restored
                        .warnings
                        .contains(&ImportWarning::DrawioSankeyStale),
                    "{format:?}: {:?}",
                    restored.warnings
                );
            }
        }
    }

    // A live Sankey edge may keep its own numeric source ID.
    let mut book = thinoda_core::templates::instantiate("sankey-budget");
    let edge = book
        .active()
        .canvas
        .objects
        .iter()
        .find(|object| matches!(&object.kind, CanvasObjectKind::Connector(c) if c.sankey.is_some()))
        .unwrap()
        .id;
    let id = format!("o{}", edge.0);
    book.active_mut().drawio = Some(Arc::new(DrawioPagePassthrough {
        cell_ids: vec![id.clone()],
        ..Default::default()
    }));
    book.active_mut().canvas.object_mut(edge).unwrap().drawio = Some(Arc::new(Passthrough {
        source_id: Some(id.clone()),
        ..Default::default()
    }));
    let bytes = export(&book).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains(&format!(r#"id="{id}""#)));
    assert!(import(&bytes).unwrap().workbook.has_sankey());

    let duplicate = book
        .active_mut()
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(800.0, 800.0, 80.0, 60.0),
            "Conflicting source",
        ))
        .unwrap();
    book.active_mut()
        .canvas
        .object_mut(duplicate)
        .unwrap()
        .drawio = Some(Arc::new(Passthrough {
        source_id: Some(format!("o{}", edge.0)),
        ..Default::default()
    }));
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert_eq!(xml.matches(&format!(r#"id="{id}""#)).count(), 1);
    assert!(load(&xml).workbook.has_sankey());
}

#[test]
fn drawio_sankey_extension_preserves_inactive_stage_settings() {
    let mut book = thinoda_core::templates::instantiate("sankey-budget");
    let extra = book
        .active_mut()
        .add_card(
            "Stored stage",
            Vec2::new(900.0, 900.0),
            Card::new(CardKind::Text, Vec2::new(100.0, 60.0)),
        )
        .unwrap();
    book.active_mut()
        .sankey
        .as_mut()
        .unwrap()
        .stages
        .insert(extra, 3);
    assert!(
        !thinoda_core::sankey::evaluate(book.active())
            .unwrap()
            .nodes
            .contains(&extra)
    );
    book.active_mut().drawio = Some(Arc::new(DrawioPagePassthrough {
        cell_ids: vec![format!("t{}", extra.0)],
        ..Default::default()
    }));
    let report = import(&export(&book).unwrap()).unwrap();
    assert!(
        !report.warnings.contains(&ImportWarning::DrawioSankeyStale),
        "{:?}",
        report.warnings
    );
    let sheet = report.workbook.active();
    assert!(
        sheet
            .sankey
            .as_ref()
            .unwrap()
            .stages
            .iter()
            .any(|(id, stage)| {
                *stage == 3
                    && sheet
                        .topic(*id)
                        .is_some_and(|topic| topic.title == "Stored stage")
            })
    );
}

#[test]
fn passthrough_style_keeps_generated_keys_and_drops_orphans() {
    let drawio = Passthrough {
        source_id: None,
        style: vec![
            ("strokeColor".into(), "#ff0000".into()),
            ("dashPattern".into(), "1 4".into()),
            ("jettySize".into(), "auto".into()),
            ("custom".into(), "1".into()),
        ],
        attributes: Vec::new(),
    };
    let mut style = String::from("edgeStyle=none;strokeColor=#000000;");
    passthrough_style(&mut style, Some(&drawio));
    assert_eq!(style, "edgeStyle=none;strokeColor=#000000;custom=1;");
}

#[test]
fn library_connector_markers_round_trip_and_require_native_version_thirty_eight() {
    use ConnectorMarkerKind as K;
    for (name, kind) in [
        ("ERone", K::ErOne),
        ("ERmandOne", K::ErMandOne),
        ("ERmany", K::ErMany),
        ("ERoneToMany", K::ErOneToMany),
        ("ERzeroToOne", K::ErZeroToOne),
        ("ERzeroToMany", K::ErZeroToMany),
        ("manyOptional", K::ManyOptional),
        ("box", K::Box),
        ("cross", K::Cross),
        ("circle", K::Circle),
        ("circlePlus", K::CirclePlus),
        ("async", K::Async),
        ("openAsync", K::OpenAsync),
        ("doubleBlock", K::DoubleBlock),
        ("sysMLx", K::SysmlX),
        ("sysMLLost", K::SysmlLost),
        ("sysMLFound", K::SysmlFound),
        ("sysMLPackCont", K::SysmlPackCont),
    ] {
        for fill in [0, 1] {
            let xml = page(&format!(
                r#"<mxCell id="e" style="startArrow={name};startFill={fill};endArrow={name};endFill={fill};endSize=10;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
            ));
            let report = load(&xml);
            assert!(report.warnings.is_empty(), "{name}: {:?}", report.warnings);
            let connector = connectors(&report.workbook.sheets[0])[0].clone();
            assert_eq!(connector.start_marker.unwrap().kind, kind);
            assert_eq!(connector.end_marker.unwrap().kind, kind);
            assert_eq!(connector.end_marker_size, Some(10.0));
            assert_eq!(connector.end_marker_unfilled, fill == 0);
            assert_eq!(crate::required_native_version(&report.workbook), 38);
            let exported = export(&report.workbook).unwrap();
            let restored = import(&exported).unwrap();
            assert!(
                restored.warnings.is_empty(),
                "{name}: {:?}",
                restored.warnings
            );
            let restored = connectors(&restored.workbook.sheets[0])[0].clone();
            assert_eq!(restored.start_marker, connector.start_marker);
            assert_eq!(restored.end_marker, connector.end_marker);
            assert_eq!(restored.end_marker_size, connector.end_marker_size);
            assert_eq!(restored.end_marker_unfilled, connector.end_marker_unfilled);
            let native = crate::formats::native_export(&report.workbook).unwrap();
            assert_eq!(
                crate::formats::native_import(&native).unwrap(),
                report.workbook
            );
        }
    }

    let report = load(&page(
        r#"<mxCell id="e" style="endArrow=ERmany;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#,
    ));
    let content = serde_json::to_vec(&report.workbook).unwrap();
    let older = crate::formats::archive(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":37,"resources":[]}"#,
        ),
        ("content.json", &content),
    ])
    .unwrap();
    assert!(matches!(
        crate::formats::native_import(&older),
        Err(IoError::NewerVersion(message)) if message.contains("version 38")
    ));
    assert!(crate::formats::native_import(&content).is_err());
}

#[test]
fn drawio_edge_styles_round_trip_and_warn_only_when_approximated() {
    let edge = |style: &str, points: &str| {
        page(&format!(
            r#"<mxCell id="e" style="endArrow=none;{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/>{points}</mxGeometry></mxCell>"#
        ))
    };
    let hint = r#"<Array as="points"><mxPoint x="40" y="70"/><mxPoint x="60" y="80"/></Array>"#;
    for (style, expected) in [
        ("edgeStyle=elbowEdgeStyle;", ConnectorRoute::ElbowHorizontal),
        (
            "edgeStyle=elbowEdgeStyle;elbow=horizontal;",
            ConnectorRoute::ElbowHorizontal,
        ),
        (
            "edgeStyle=elbowEdgeStyle;elbow=vertical;",
            ConnectorRoute::ElbowVertical,
        ),
        ("edgeStyle=sideToSideEdgeStyle;", ConnectorRoute::SideToSide),
        (
            "edgeStyle=topToBottomEdgeStyle;",
            ConnectorRoute::TopToBottom,
        ),
        (
            "edgeStyle=entityRelationEdgeStyle;",
            ConnectorRoute::EntityRelation,
        ),
        (
            "edgeStyle=entityRelationEdgeStyle;segment=30;",
            ConnectorRoute::EntityRelation,
        ),
        ("edgeStyle=segmentEdgeStyle;", ConnectorRoute::Segment),
        (
            "edgeStyle=isometricEdgeStyle;",
            ConnectorRoute::IsometricHorizontal,
        ),
        (
            "edgeStyle=isometricEdgeStyle;elbow=vertical;",
            ConnectorRoute::IsometricVertical,
        ),
    ] {
        // Waypoints are routing hints draw.io honours the same way.
        let report = load(&edge(style, hint));
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        let connector = connectors(&report.workbook.sheets[0])[0];
        assert_eq!(connector.route, expected, "{style}");
        assert_eq!(connector.waypoints.len(), 2);
        assert_eq!(
            feature_version(&report.workbook),
            if expected.is_segment_or_isometric() {
                41
            } else {
                40
            }
        );
        let exported = export(&report.workbook).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&exported).contains("elbow=vertical"),
            matches!(
                expected,
                ConnectorRoute::ElbowVertical | ConnectorRoute::IsometricVertical
            )
        );
        let restored = import(&exported).unwrap();
        assert!(
            restored.warnings.is_empty(),
            "{style}: {:?}",
            restored.warnings
        );
        let restored = connectors(&restored.workbook.sheets[0])[0];
        assert_eq!(restored.route, expected, "{style}");
        assert_eq!(restored.waypoints, connector.waypoints, "{style}");
    }
    // Routers Thinoda only approximates with its orthogonal elbow still warn,
    // as do non-default entity-relation segments and hints on orthogonal edges.
    for (style, points) in [
        ("edgeStyle=loopEdgeStyle;", ""),
        ("edgeStyle=entityRelationEdgeStyle;segment=12;", ""),
        ("edgeStyle=orthogonalEdgeStyle;", hint),
    ] {
        let report = load(&edge(style, points));
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}: {:?}",
            report.warnings
        );
    }
    let report = load(&edge("edgeStyle=loopEdgeStyle;", ""));
    assert_eq!(
        connectors(&report.workbook.sheets[0])[0].route,
        ConnectorRoute::Elbow
    );
    // Without a routed style the orientation key means nothing and is dropped.
    let report = load(&edge("elbow=vertical;", ""));
    assert_eq!(
        connectors(&report.workbook.sheets[0])[0].route,
        ConnectorRoute::Straight
    );
    assert!(
        !String::from_utf8(export(&report.workbook).unwrap())
            .unwrap()
            .contains("elbow=")
    );
}

#[test]
fn self_loops_import_silently_unless_the_loop_is_customised() {
    let looped = |style: &str| {
        page(&format!(
            r#"<mxCell id="v" value="" style="rounded=0;" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell><mxCell id="e" style="endArrow=none;{style}" edge="1" parent="1" source="v" target="v"><mxGeometry relative="1" as="geometry"/></mxCell>"#
        ))
    };
    for style in ["", "direction=west;segment=10;"] {
        let report = load(&looped(style));
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        let connector = connectors(&report.workbook.sheets[0])[0];
        assert_eq!(connector.from, connector.to);
    }
    for style in ["direction=north;", "segment=20;"] {
        let report = load(&looped(style));
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn line_jumps_round_trip_and_warn_for_unmodelled_jump_options() {
    use thinoda_core::board::{LineJumpStyle, LineJumps};

    let edge = |style: &str| {
        page(&format!(
            r#"<mxCell id="e" style="endArrow=none;{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ))
    };
    for (style, expected) in [
        ("jumpStyle=arc;", Some(LineJumps::new(LineJumpStyle::Arc))),
        (
            "jumpStyle=gap;jumpSize=6;",
            Some(LineJumps::new(LineJumpStyle::Gap)),
        ),
        (
            "jumpStyle=sharp;jumpSize=12.9;",
            Some(LineJumps {
                style: LineJumpStyle::Sharp,
                size: 12.0,
            }),
        ),
        (
            "jumpStyle=line;jumpSize=0;",
            Some(LineJumps {
                style: LineJumpStyle::Line,
                size: 0.0,
            }),
        ),
        // draw.io keeps but does not paint jumps on curves; so does Thinoda.
        (
            "jumpStyle=gap;curved=1;",
            Some(LineJumps::new(LineJumpStyle::Gap)),
        ),
        ("jumpStyle=none;jumpSize=20;", None),
    ] {
        let report = load(&edge(style));
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        let connector = connectors(&report.workbook.sheets[0])[0];
        assert_eq!(connector.line_jumps, expected, "{style}");
        if expected.is_some() {
            assert_eq!(feature_version(&report.workbook), 39);
        }
        let exported = export(&report.workbook).unwrap();
        let restored = import(&exported).unwrap();
        assert!(
            restored.warnings.is_empty(),
            "{style}: {:?}",
            restored.warnings
        );
        assert_eq!(
            connectors(&restored.workbook.sheets[0])[0].line_jumps,
            expected
        );
    }
    for style in [
        "jumpStyle=wave;",
        "jumpStyle=arc;jumpSize=1000;",
        "jumpStyle=arc;jumpSize=big;",
        "noJump=1;",
        "jumpStyle=arc;jumpLayers=1;",
    ] {
        let report = load(&edge(style));
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn edge_shapes_round_trip_and_warn_for_unrepresentable_parameters() {
    use thinoda_core::board::{EdgeShape, EdgeShapeKind};

    let edge = |style: &str| {
        page(&format!(
            r#"<mxCell id="e" style="endArrow=classic;{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ))
    };
    let shape = |kind| Some(EdgeShape::new(kind));
    for (style, expected) in [
        ("shape=flexArrow;", shape(EdgeShapeKind::FlexArrow)),
        (
            "shape=flexArrow;width=14;endWidth=8.5;fillColor=#00994D;",
            Some(EdgeShape {
                width: Some(14.0),
                end_width: Some(8.5),
                fill: Some(Color::rgb(0x00, 0x99, 0x4d)),
                ..EdgeShape::new(EdgeShapeKind::FlexArrow)
            }),
        ),
        (
            "shape=taperedArrow;startWidth=20;endWidth=0;",
            Some(EdgeShape {
                start_width: Some(20.0),
                end_width: Some(0.0),
                ..EdgeShape::new(EdgeShapeKind::TaperedArrow)
            }),
        ),
        (
            "shape=link;width=6;fillColor=#FF0000;",
            Some(EdgeShape {
                width: Some(6.0),
                ..EdgeShape::new(EdgeShapeKind::Link)
            }),
        ),
        (
            "shape=pipe;fillColor=#FFFFFF;",
            Some(EdgeShape {
                fill: Some(Color::WHITE),
                ..EdgeShape::new(EdgeShapeKind::Pipe)
            }),
        ),
        (
            "shape=filledEdge;fillColor=none;",
            shape(EdgeShapeKind::FilledEdge),
        ),
        ("shape=wire;width=9;", shape(EdgeShapeKind::Wire)),
        ("shape=connector;", None),
    ] {
        let report = load(&edge(style));
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        let connector = connectors(&report.workbook.sheets[0])[0];
        assert_eq!(connector.edge_shape, expected, "{style}");
        if expected.is_some() {
            assert_eq!(feature_version(&report.workbook), 42, "{style}");
        }
        let exported = export(&report.workbook).unwrap();
        let xml = String::from_utf8(exported.clone()).unwrap();
        assert!(!xml.contains("width=9"), "{style}: {xml}");
        let restored = import(&exported).unwrap();
        assert!(
            restored.warnings.is_empty(),
            "{style}: {:?}",
            restored.warnings
        );
        assert_eq!(
            connectors(&restored.workbook.sheets[0])[0].edge_shape,
            expected,
            "{style}"
        );
    }
    for style in [
        "shape=arrow;",
        "shape=flexArrow;width=1000;",
        "shape=taperedArrow;endWidth=wide;",
    ] {
        let report = load(&edge(style));
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn connector_labels_import_positioned_and_round_trip() {
    use thinoda_core::board::{ConnectorLabel, ConnectorLabelPlacement};

    let child = |id: &str, value: &str, style: &str, geometry: &str| {
        format!(
            r#"<mxCell id="{id}" value="{value}" style="{style}" vertex="1" connectable="0" parent="e"><mxGeometry {geometry} as="geometry"><mxPoint x="3" y="-4" as="offset"/></mxGeometry></mxCell>"#
        )
    };
    let label_style = "edgeLabel;html=1;align=center;verticalAlign=middle;resizable=0;points=[];fontSize=12;fontColor=#000000;";
    let xml = page(&format!(
        r##"<mxCell id="e" value="main" style="endArrow=none;strokeColor=#000;fontColor=#000000;fontSize=12;" edge="1" parent="1"><mxGeometry x="0.5" y="-10" relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/><mxPoint x="1" y="2" as="offset"/></mxGeometry></mxCell>{}{}"##,
        child("c1", "1", label_style, r#"x="-0.8" y="6" relative="1""#),
        child(
            "c2",
            "n",
            label_style,
            r#"x="1" width="20" height="10" relative="1""#
        ),
    ));
    let sheet = clean(&xml);
    let connector = connectors(&sheet)[0];
    assert_eq!(connector.label, "main");
    assert_eq!(
        connector.label_placement,
        Some(ConnectorLabelPlacement {
            along: 0.5,
            orthogonal: -10.0,
            offset: Vec2::new(1.0, 2.0),
        })
    );
    assert_eq!(
        connector.extra_labels,
        vec![
            ConnectorLabel {
                text: "1".into(),
                placement: ConnectorLabelPlacement {
                    along: -0.8,
                    orthogonal: 6.0,
                    offset: Vec2::new(3.0, -4.0),
                },
                rich_text: None,
            },
            ConnectorLabel {
                text: "n".into(),
                placement: ConnectorLabelPlacement {
                    along: 1.0,
                    orthogonal: 0.0,
                    offset: Vec2::new(13.0, 1.0),
                },
                rich_text: None,
            },
        ]
    );
    let book = load(&xml).workbook;
    assert_eq!(feature_version(&book), 43);
    let exported = export(&book).unwrap();
    let restored = import(&exported).unwrap();
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    let back = connectors(&restored.workbook.sheets[0])[0];
    assert_eq!(back.label_placement, connector.label_placement);
    assert_eq!(back.extra_labels, connector.extra_labels);

    // Clamped positions, absolute children and styled children are disclosed.
    for extra in [
        child("c", "x", label_style, r#"x="2" relative="1""#),
        child(
            "c",
            "x",
            label_style,
            r#"x="10" y="10" width="5" height="5""#,
        ),
        child(
            "c",
            "x",
            &format!("{label_style}fontStyle=1;"),
            r#"relative="1""#,
        ),
        child(
            "c",
            "x",
            "edgeLabel;fontSize=11;fontColor=#000000;",
            r#"relative="1""#,
        ),
    ] {
        let xml = page(&format!(
            r##"<mxCell id="e" style="endArrow=none;strokeColor=#000;fontColor=#000000;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>{extra}"##
        ));
        assert!(
            load(&xml).warnings.contains(&ImportWarning::Formatting),
            "{extra}"
        );
    }
}

#[test]
fn absolute_edge_children_keep_inline_formatting_when_folded() {
    let xml = page(concat!(
        r#"<mxCell id="e" value="&lt;b&gt;Ä&lt;/b&gt;" style="html=1;endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#,
        r#"<mxCell id="c" value="&lt;i&gt;猫&lt;/i&gt;" style="html=1;" vertex="1" parent="e"><mxGeometry x="10" y="10" width="5" height="5" as="geometry"/></mxCell>"#,
    ));
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    let object = report
        .workbook
        .active()
        .canvas
        .objects
        .iter()
        .find(|object| matches!(&object.kind, CanvasObjectKind::Connector(_)))
        .unwrap();
    assert_eq!(object.label(), "Ä\n猫");
    let rich = object.label_rich_text.as_ref().unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 1));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!((rich.runs[1].start, rich.runs[1].end), (2, 3));
    assert_eq!(rich.runs[1].style.italic, Some(true));
    let reopened = import(&export(&report.workbook).unwrap()).unwrap();
    let restored = reopened
        .workbook
        .active()
        .canvas
        .objects
        .iter()
        .find(|object| matches!(&object.kind, CanvasObjectKind::Connector(_)))
        .unwrap();
    assert_eq!(restored.label_rich_text, object.label_rich_text);
}

#[test]
fn folded_edge_child_lists_keep_paragraph_offsets_and_truncated_styles() {
    use thinoda_core::ListKind;

    let parent = format!("<b>{}</b>", "a".repeat(9995));
    let child = "<ul><li><i>猫🙂</i></li><li>last</li></ul>";
    let xml = page(&format!(
        r#"<mxCell id="e" value="{}" style="html=1;endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell><mxCell id="c" value="{}" style="html=1;" vertex="1" parent="e"><mxGeometry width="5" height="5" as="geometry"/></mxCell>"#,
        esc(&parent),
        esc(child),
    ));
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    let object = report
        .workbook
        .active()
        .canvas
        .objects
        .iter()
        .find(|object| matches!(&object.kind, CanvasObjectKind::Connector(_)))
        .unwrap();
    assert_eq!(object.label().chars().count(), 10_000);
    assert!(object.label().ends_with("\n猫🙂\nl"));
    let rich = object.label_rich_text.as_ref().unwrap();
    assert!(rich.validate(object.label()).is_ok());
    assert_eq!((rich.runs[1].start, rich.runs[1].end), (9996, 9998));
    assert_eq!(rich.runs[1].style.italic, Some(true));
    assert_eq!(
        rich.paragraphs
            .iter()
            .map(|p| (p.index, p.list))
            .collect::<Vec<_>>(),
        vec![(1, ListKind::Bulleted), (2, ListKind::Bulleted)]
    );
}

#[test]
fn folded_edge_child_exceeding_run_limit_keeps_primary_formatting() {
    let parent = "<b>a</b><i>b</i>".repeat(512);
    let xml = page(&format!(
        r#"<mxCell id="e" value="{}" style="html=1;endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell><mxCell id="c" value="{}" style="html=1;" vertex="1" parent="e"><mxGeometry width="5" height="5" as="geometry"/></mxCell>"#,
        esc(&parent),
        esc("<b>child</b>"),
    ));
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Formatting));
    let object = report
        .workbook
        .active()
        .canvas
        .objects
        .iter()
        .find(|object| matches!(&object.kind, CanvasObjectKind::Connector(_)))
        .unwrap();
    assert!(object.label().ends_with("\nchild"));
    let rich = object.label_rich_text.as_ref().unwrap();
    assert_eq!(rich.runs.len(), 1024);
    assert_eq!(rich.runs.last().unwrap().end, 1024);
    assert!(rich.validate(object.label()).is_ok());
}

#[test]
fn drawio_layers_import_as_named_hidden_and_locked_board_layers_and_round_trip() {
    let xml = concat!(
        r#"<mxfile><diagram name="P"><mxGraphModel><root><mxCell id="0"/>"#,
        r#"<mxCell id="1" parent="0"/>"#,
        r#"<mxCell id="base-a" value="A" style="rounded=0;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="40" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="notes" value="Notes" parent="0"/>"#,
        r#"<mxCell id="n1" value="N" style="ellipse;" vertex="1" parent="notes"><mxGeometry x="100" y="0" width="40" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="secret" value="Secret" visible="0" parent="0"/>"#,
        r#"<mxCell id="s1" value="S" style="rounded=0;" vertex="1" parent="secret"><mxGeometry x="200" y="0" width="40" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="fixed" value="Fixed" style="locked=1;" parent="0"/>"#,
        r#"<mxCell id="f1" value="F" style="rounded=0;" vertex="1" parent="fixed"><mxGeometry x="300" y="0" width="40" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="f2" value="F2" style="rounded=0;" vertex="1" parent="fixed"><mxGeometry x="400" y="0" width="40" height="40" as="geometry"/></mxCell>"#,
        r#"</root></mxGraphModel></diagram></mxfile>"#,
    );
    let report = load(xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let sheet = &report.workbook.sheets[0];
    let layers: Vec<_> = sheet
        .canvas
        .layers
        .iter()
        .map(|l| (l.name.as_str(), l.visible, l.locked, l.source_id.as_deref()))
        .collect();
    assert_eq!(
        layers,
        [
            ("Notes", true, false, Some("notes")),
            ("Secret", false, false, Some("secret")),
            ("Fixed", true, true, Some("fixed")),
        ]
    );
    let layer_of = |sheet: &Sheet, source: &str| {
        let object = sheet
            .canvas
            .objects
            .iter()
            .find(|o| o.drawio.as_ref().and_then(|d| d.source_id.as_deref()) == Some(source))
            .unwrap_or_else(|| panic!("missing {source}"));
        object
            .layer
            .map(|id| sheet.canvas.board_layer(id).unwrap().name.clone())
    };
    assert_eq!(layer_of(sheet, "base-a"), None);
    assert_eq!(layer_of(sheet, "n1").as_deref(), Some("Notes"));
    assert_eq!(layer_of(sheet, "s1").as_deref(), Some("Secret"));
    assert_eq!(layer_of(sheet, "f2").as_deref(), Some("Fixed"));
    let f1 = sheet
        .canvas
        .objects
        .iter()
        .find(|o| o.drawio.as_ref().and_then(|d| d.source_id.as_deref()) == Some("f1"))
        .unwrap();
    assert!(sheet.canvas.is_locked(f1.id));
    assert!(!f1.locked);
    assert_eq!(crate::required_native_version(&report.workbook), 45);

    let native = crate::formats::native_export(&report.workbook).unwrap();
    let reopened = crate::formats::native_import(&native).unwrap();
    assert_eq!(reopened.sheets[0].canvas.layers, sheet.canvas.layers);
    let content = serde_json::to_vec(&reopened).unwrap();
    let old = crate::formats::archive(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":44,"resources":[]}"#,
        ),
        ("content.json", &content),
    ])
    .unwrap();
    assert!(crate::formats::native_import(&old).is_err());

    let out = String::from_utf8(export(&reopened).unwrap()).unwrap();
    for needle in [
        r#"<mxCell id="notes" value="Notes" parent="0"/>"#,
        r#"<mxCell id="secret" value="Secret" visible="0" parent="0"/>"#,
        r#"<mxCell id="fixed" value="Fixed" style="locked=1;" parent="0"/>"#,
        r#"id="n1""#,
        r#"id="s1""#,
    ] {
        assert!(out.contains(needle), "missing {needle}: {out}");
    }
    let layer_pos = out.find(r#"id="secret""#).unwrap();
    assert!(out.find(r#"id="s1""#).unwrap() > layer_pos);
    assert!(out.contains(r#"parent="secret""#), "{out}");
    let again = load(&out);
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    let again = &again.workbook.sheets[0];
    assert_eq!(again.canvas.layers, sheet.canvas.layers);
    for source in ["base-a", "n1", "s1", "f1", "f2"] {
        assert_eq!(layer_of(again, source), layer_of(sheet, source), "{source}");
    }
}

#[test]
fn drawio_unrepresented_container_metadata_is_reported() {
    for (style, children) in [
        ("swimlane;", ""),
        (
            "group;",
            r#"<mxCell id="child-a" vertex="1" parent="container"><mxGeometry width="50" height="40" as="geometry"/></mxCell><mxCell id="child-b" vertex="1" parent="container"><mxGeometry x="100" width="50" height="40" as="geometry"/></mxCell>"#,
        ),
    ] {
        let document = |attributes: &str| {
            let label = if style == "group;" { "" } else { "Container" };
            page(&format!(
                r#"<UserObject id="container" label="{label}" {attributes}><mxCell style="{style}" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell></UserObject>{children}"#
            ))
        };
        assert!(
            !load(&document("")).warnings.contains(&ImportWarning::Other),
            "{style}"
        );
        for attribute in [
            r#"link="https://example.com""#,
            r#"tooltip="Important details""#,
            r#"placeholders="1""#,
        ] {
            assert!(
                load(&document(attribute))
                    .warnings
                    .contains(&ImportWarning::Other),
                "{style}: {attribute}"
            );
        }
    }

    let card = page(
        r#"<UserObject id="card" label="Card" link="https://example.com"><mxCell style="shape=note;thinodaCard=sticky;backgroundOutline=1;size=14;" vertex="1" parent="1"><mxGeometry width="120" height="60" as="geometry"/></mxCell></UserObject>"#,
    );
    let report = load(&card);
    assert!(!report.warnings.contains(&ImportWarning::Other));
    assert_eq!(
        report
            .workbook
            .active()
            .topics
            .values()
            .find(|topic| topic.card.is_some())
            .unwrap()
            .hyperlink
            .as_deref(),
        Some("https://example.com")
    );
    let object = page(
        r#"<UserObject id="object" label="Object" link="https://example.com" tooltip="Details" placeholders="1"><mxCell vertex="1" parent="1"><mxGeometry width="120" height="60" as="geometry"/></mxCell></UserObject>"#,
    );
    assert!(!load(&object).warnings.contains(&ImportWarning::Other));
}

#[test]
fn drawio_group_discloses_discarded_content_and_styles() {
    let document = |style: &str, attributes: &str| {
        page(&format!(
            r#"<mxCell id="g" {attributes} style="{style}" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell><mxCell id="a" vertex="1" parent="g"><mxGeometry width="50" height="40" as="geometry"/></mxCell><mxCell id="b" vertex="1" parent="g"><mxGeometry x="100" width="50" height="40" as="geometry"/></mxCell>"#
        ))
    };
    for style in ["group;", "group;locked=0;", "group;locked=1;"] {
        let report = load(&document(style, ""));
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        assert_eq!(report.workbook.active().canvas.groups.len(), 1);
    }
    for style in [
        "group;fillColor=#ff0000;",
        "group;custom=value;",
        "group;locked=invalid;",
        "group;otherStyle;",
        ";group;",
    ] {
        let report = load(&document(style, ""));
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}: {:?}",
            report.warnings
        );
    }
    for attributes in [r#"value="Important""#, r#"collapsed="1""#] {
        let report = load(&document("group;", attributes));
        assert!(
            report.warnings.contains(&ImportWarning::Other),
            "{attributes}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn drawio_zone_discloses_unsupported_explicit_styles() {
    let document = |style: &str| {
        page(&format!(
            r#"<mxCell id="zone" value="Lane" style="{style}" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#
        ))
    };
    let baseline = "swimlane;whiteSpace=wrap;html=1;startSize=24;collapsible=0;";
    let report = load(&document(baseline));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.workbook.active().zones.len(), 1);
    for style in [
        "swimlane;collapsible=0;",
        "swimlane;startSize=24;",
        "swimlane;startSize=40;",
        "swimlane;startSize=invalid;",
        "swimlane;horizontal=0;",
        "swimlane;collapsible=1;",
        "swimlane;strokeWidth=4;",
        "swimlane;custom=value;",
        "swimlane;otherStyle;",
        ";swimlane;",
    ] {
        let report = load(&document(style));
        assert!(
            report.warnings.contains(&ImportWarning::Formatting),
            "{style}: {:?}",
            report.warnings
        );
        assert_eq!(report.workbook.active().zones.len(), 1, "{style}");
    }
}

#[test]
fn drawio_zone_source_id_survives_native_and_drawio_round_trips() {
    let xml = page(concat!(
        r#"<mxCell id="source-zone" value="Lane" style="swimlane;" vertex="1" parent="1"><mxGeometry x="100" y="100" width="300" height="200" as="geometry"/></mxCell>"#,
        r#"<mxCell id="child" value="Child" vertex="1" parent="source-zone"><mxGeometry x="20" y="30" width="80" height="40" as="geometry"/></mxCell>"#,
    ));
    let report = load(&xml);
    let book = report.workbook;
    let sheet = book.active();
    assert_eq!(
        sheet.zones[0].drawio_source_id.as_deref(),
        Some("source-zone")
    );
    assert!(
        sheet
            .drawio
            .as_ref()
            .unwrap()
            .cell_ids
            .iter()
            .any(|id| id == "source-zone")
    );
    assert_eq!(crate::required_native_version(&book), 54);

    let native = crate::formats::native_export(&book).unwrap();
    let reopened = crate::formats::native_import(&native).unwrap();
    let content = serde_json::to_vec(&reopened).unwrap();
    assert!(matches!(
        crate::formats::native_import(&content),
        Err(IoError::NewerVersion(_))
    ));
    let old = crate::formats::archive(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":53,"resources":[]}"#,
        ),
        ("content.json", &content),
    ])
    .unwrap();
    assert!(matches!(
        crate::formats::native_import(&old),
        Err(IoError::NewerVersion(_))
    ));
    let exported = String::from_utf8(export(&reopened).unwrap()).unwrap();
    assert!(exported.contains(r#"id="source-zone""#), "{exported}");
    assert!(exported.contains(r#"parent="source-zone""#), "{exported}");
    let again = load(&exported).workbook;
    assert_eq!(
        again.active().zones[0].drawio_source_id.as_deref(),
        Some("source-zone")
    );
    assert!(
        again.masked_copy().active().zones[0]
            .drawio_source_id
            .is_none()
    );

    let mut doc = thinoda_core::Document::new(reopened);
    let id = doc.sheet().zones[0].id;
    doc.remove_zone(id).unwrap();
    assert!(doc.undo());
    assert_eq!(
        doc.sheet().zones[0].drawio_source_id.as_deref(),
        Some("source-zone")
    );
    assert!(doc.redo());
    assert!(
        doc.sheet()
            .drawio
            .as_ref()
            .unwrap()
            .cell_ids
            .iter()
            .any(|id| id == "source-zone")
    );
}

#[test]
fn drawio_deleted_zone_source_id_is_not_reused_by_new_objects() {
    let document = |id: &str| {
        page(&format!(
            r#"<mxCell id="{id}" value="Lane" style="swimlane;" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#
        ))
    };
    let replace_zone = |book: &mut Workbook| {
        let sheet = book.active_mut();
        let zone = sheet.zones[0].id;
        sheet.remove_zone(zone).unwrap();
        sheet
            .add_canvas_object(shape(
                ShapeKind::Rectangle,
                rect(400.0, 100.0, 80.0, 60.0),
                "New",
            ))
            .unwrap()
    };
    let mut probe = load(&document("old-zone")).workbook;
    let source = format!("o{}", replace_zone(&mut probe).0);
    let report = load(&document(&source));
    let native = crate::formats::native_export(&report.workbook).unwrap();
    let mut reopened = crate::formats::native_import(&native).unwrap();
    assert_eq!(format!("o{}", replace_zone(&mut reopened).0), source);
    let xml = String::from_utf8(export(&reopened).unwrap()).unwrap();
    assert!(!xml.contains(&format!(r#"id="{source}""#)), "{xml}");
    assert!(xml.contains(&format!(r#"id="{source}-1""#)), "{xml}");
}

#[test]
fn drawio_zone_source_id_budget_omissions_are_reported() {
    let budget = thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2;
    let padding = "x".repeat(budget - "padding".len() - 1);
    let xml = format!(
        r#"<mxfile><diagram><mxGraphModel padding="{padding}"><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="zone-source" value="Lane" style="swimlane;" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell></root></mxGraphModel></diagram></mxfile>"#
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert_eq!(
        report.workbook.active().zones[0]
            .drawio_source_id
            .as_deref(),
        Some("zone-source")
    );
    assert!(
        !report
            .workbook
            .active()
            .drawio
            .as_ref()
            .unwrap()
            .cell_ids
            .iter()
            .any(|id| id == "zone-source")
    );
    let xml = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    assert!(xml.contains(r#"id="zone-source""#));

    let long = "x".repeat(thinoda_core::drawio_passthrough::MAX_SOURCE_ID_BYTES + 1);
    let xml = page(&format!(
        r#"<mxCell id="{long}" value="Lane" style="swimlane;" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#
    ));
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert!(report.workbook.active().zones[0].drawio_source_id.is_none());
}

#[test]
fn drawio_zone_source_id_conflicts_do_not_duplicate_cells() {
    let xml = page(concat!(
        r#"<mxCell id="zone-a" value="Lane" style="swimlane;" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#,
        r#"<mxCell id="object-a" value="Object" vertex="1" parent="1"><mxGeometry x="400" width="80" height="40" as="geometry"/></mxCell>"#,
    ));
    let mut book = load(&xml).workbook;
    book.active_mut().zones[0].drawio_source_id = Some("object-a".into());
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert_eq!(xml.matches(r#"id="object-a""#).count(), 1, "{xml}");
    let report = load(&xml);
    assert_eq!(report.workbook.active().zones.len(), 1);
    assert_eq!(shapes(report.workbook.active()).len(), 1);
}

#[test]
fn drawio_zone_source_ids_reserve_sankey_node_and_edge_ids() {
    for node_collision in [true, false] {
        let mut book = thinoda_core::templates::instantiate("sankey-budget");
        let before = thinoda_core::sankey::evaluate(book.active()).unwrap();
        let node = *before.nodes.iter().next().unwrap();
        let edge = book
            .active()
            .canvas
            .objects
            .iter()
            .find(|object| matches!(&object.kind, CanvasObjectKind::Connector(c) if c.sankey.is_some()))
            .unwrap()
            .id;
        let source = if node_collision {
            format!("t{}", node.0)
        } else {
            format!("o{}", edge.0)
        };
        let sheet = book.active_mut();
        sheet
            .create_board_frame("Lane", rect(900.0, 900.0, 300.0, 200.0))
            .unwrap();
        sheet.zones.last_mut().unwrap().drawio_source_id = Some(source.clone());
        let xml = String::from_utf8(export(&book).unwrap()).unwrap();
        assert_eq!(xml.matches(&format!(r#"id="{source}""#)).count(), 1);
        let report = load(&xml);
        assert!(!report.warnings.contains(&ImportWarning::DrawioSankeyStale));
        assert_eq!(
            report.workbook.active().zones[0]
                .drawio_source_id
                .as_deref(),
            Some(source.as_str())
        );
        let after = thinoda_core::sankey::evaluate(report.workbook.active()).unwrap();
        let mut before_mass: Vec<_> = before.mass.values().copied().collect();
        let mut after_mass: Vec<_> = after.mass.values().copied().collect();
        before_mass.sort_by(f64::total_cmp);
        after_mass.sort_by(f64::total_cmp);
        assert_eq!(before_mass, after_mass);
    }
}

#[test]
fn drawio_card_source_ids_round_trip_and_reserve_deleted_ids() {
    let xml = page(concat!(
        r#"<mxCell id="t3" value="Card" style="text;thinodaCard=text;" vertex="1" parent="1"><mxGeometry width="120" height="60" as="geometry"/></mxCell>"#,
        r#"<mxCell id="edge" edge="1" parent="1" source="t3"><mxGeometry relative="1" as="geometry"><mxPoint x="250" y="40" as="targetPoint"/></mxGeometry></mxCell>"#,
    ));
    let mut probe = load(&xml).workbook;
    let sheet = probe.active_mut();
    sheet.detach_subtree(sheet.floating[0]).unwrap();
    let next = sheet
        .add_card(
            "Replacement",
            Vec2::new(300.0, 100.0),
            Card::new(CardKind::Text, Vec2::new(120.0, 60.0)),
        )
        .unwrap();
    let source = format!("t{}", next.0);
    let xml = xml.replace("t3", &source);
    let report = load(&xml);
    let sheet = report.workbook.active();
    let card = *sheet.floating.first().unwrap();
    assert_eq!(
        sheet.topic(card).unwrap().drawio_source_id.as_deref(),
        Some(source.as_str())
    );
    assert_eq!(crate::required_native_version(&report.workbook), 55);
    let native = crate::formats::native_export(&report.workbook).unwrap();
    let mut reopened = crate::formats::native_import(&native).unwrap();
    let content = serde_json::to_vec(&reopened).unwrap();
    let old = crate::formats::archive(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":54,"resources":[]}"#,
        ),
        ("content.json", &content),
    ])
    .unwrap();
    assert!(matches!(
        crate::formats::native_import(&old),
        Err(IoError::NewerVersion(_))
    ));
    assert!(crate::formats::native_import(&content).is_err());
    for _ in 0..2 {
        let out = String::from_utf8(export(&reopened).unwrap()).unwrap();
        assert_eq!(out.matches(&format!(r#"id="{source}""#)).count(), 1);
        assert!(out.contains(&format!(r#"source="{source}""#)));
        reopened = load(&out).workbook;
        assert_eq!(
            reopened
                .active()
                .topic(reopened.active().floating[0])
                .unwrap()
                .drawio_source_id
                .as_deref(),
            Some(source.as_str())
        );
    }
    let sheet = reopened.active_mut();
    let card = sheet.floating[0];
    sheet.detach_subtree(card).unwrap();
    let replacement = sheet
        .add_card(
            "Replacement",
            Vec2::new(300.0, 100.0),
            Card::new(CardKind::Text, Vec2::new(120.0, 60.0)),
        )
        .unwrap();
    let out = String::from_utf8(export(&reopened).unwrap()).unwrap();
    assert_eq!(replacement, next);
    assert!(!out.contains(&format!(r#"id="{source}""#)));
    assert!(out.contains(&format!(r#"id="t{}-1""#, replacement.0)));
}

#[test]
fn drawio_card_source_id_collisions_do_not_steal_other_owners() {
    let mut book = Workbook::default();
    let sheet = book.active_mut();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(100.0, 100.0),
            Card::new(CardKind::Text, Vec2::new(120.0, 60.0)),
        )
        .unwrap();
    sheet.topic_mut(card).unwrap().drawio_source_id = Some("shared".into());
    let shape = sheet
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(300.0, 100.0, 80.0, 60.0),
            "Shape",
        ))
        .unwrap();
    sheet.canvas.object_mut(shape).unwrap().drawio = Some(Arc::new(Passthrough {
        source_id: Some("shared".into()),
        ..Default::default()
    }));
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert_eq!(xml.matches(r#"id="shared""#).count(), 1);
    let restored = load(&xml);
    assert_eq!(cards(restored.workbook.active()).len(), 1);
    assert!(shapes(restored.workbook.active()).iter().any(|object| {
        object.label() == "Shape"
            && object
                .drawio
                .as_ref()
                .and_then(|data| data.source_id.as_deref())
                == Some("shared")
    }));
}

#[test]
fn drawio_card_invalid_source_id_is_disclosed() {
    let id = "x".repeat(thinoda_core::drawio_passthrough::MAX_SOURCE_ID_BYTES + 1);
    let xml = page(&format!(
        r#"<mxCell id="{id}" value="Card" style="text;thinodaCard=text;" vertex="1" parent="1"><mxGeometry width="120" height="60" as="geometry"/></mxCell>"#
    ));
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert!(
        report
            .workbook
            .active()
            .topics
            .values()
            .all(|topic| topic.drawio_source_id.is_none())
    );
}

#[test]
fn drawio_card_source_id_survives_when_tombstone_budget_is_exhausted() {
    let budget = thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2;
    let padding = "x".repeat(budget - "padding".len() - 1);
    let xml = format!(
        r#"<mxfile><diagram><mxGraphModel padding="{padding}"><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="card-source" value="Card" style="text;thinodaCard=text;" vertex="1" parent="1"><mxGeometry width="120" height="60" as="geometry"/></mxCell></root></mxGraphModel></diagram></mxfile>"#
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    let sheet = report.workbook.active();
    assert!(
        sheet
            .topics
            .values()
            .any(|topic| topic.drawio_source_id.as_deref() == Some("card-source"))
    );
    assert!(
        !sheet
            .drawio
            .as_ref()
            .unwrap()
            .cell_ids
            .iter()
            .any(|id| id == "card-source")
    );
    let xml = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    assert!(xml.contains(r#"id="card-source""#));
}

#[test]
fn drawio_sankey_topic_source_ids_remain_stable_with_semantics() {
    let mut book = thinoda_core::templates::instantiate("sankey-budget");
    let node = *thinoda_core::sankey::evaluate(book.active())
        .unwrap()
        .nodes
        .iter()
        .find(|id| book.active().topics[id].card.is_some())
        .unwrap();
    book.active_mut().topic_mut(node).unwrap().drawio_source_id = Some("t9876".into());
    let before = thinoda_core::sankey::evaluate(book.active()).unwrap();
    for _ in 0..2 {
        let xml = String::from_utf8(export(&book).unwrap()).unwrap();
        assert_eq!(xml.matches(r#"id="t9876""#).count(), 1);
        let report = load(&xml);
        assert!(!report.warnings.contains(&ImportWarning::DrawioSankeyStale));
        assert!(
            report
                .workbook
                .active()
                .topics
                .values()
                .any(|topic| { topic.drawio_source_id.as_deref() == Some("t9876") })
        );
        let after = thinoda_core::sankey::evaluate(report.workbook.active()).unwrap();
        let mut before_mass: Vec<_> = before.mass.values().copied().collect();
        let mut after_mass: Vec<_> = after.mass.values().copied().collect();
        before_mass.sort_by(f64::total_cmp);
        after_mass.sort_by(f64::total_cmp);
        assert_eq!(before_mass, after_mass);
        book = report.workbook;
    }
}

#[test]
fn drawio_group_source_ids_round_trip_and_reserve_deleted_ids() {
    let template = concat!(
        r#"<mxCell id="g3" style="group" vertex="1" parent="1"><mxGeometry width="300" height="120" as="geometry"/></mxCell>"#,
        r#"<mxCell id="a" value="A" vertex="1" parent="g3"><mxGeometry width="60" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="b" value="B" vertex="1" parent="g3"><mxGeometry x="100" width="60" height="40" as="geometry"/></mxCell>"#,
    );
    let regroup = |book: &mut Workbook| {
        let sheet = book.active_mut();
        let group = sheet.canvas.groups[0].id;
        let members = sheet.board_group_members(group);
        sheet.ungroup_board_members(group).unwrap();
        sheet.group_board_members(&members).unwrap()
    };
    let mut probe = load(&page(template)).workbook;
    let next = regroup(&mut probe);
    let source = format!("g{}", next.0);
    let xml = page(&template.replace("g3", &source));
    let mut book = load(&xml).workbook;
    assert_eq!(
        book.active().canvas.groups[0].drawio_source_id.as_deref(),
        Some(source.as_str())
    );
    assert_eq!(crate::required_native_version(&book), 56);
    let native = crate::formats::native_export(&book).unwrap();
    book = crate::formats::native_import(&native).unwrap();
    let content = serde_json::to_vec(&book).unwrap();
    let old = crate::formats::archive(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":55,"resources":[]}"#,
        ),
        ("content.json", &content),
    ])
    .unwrap();
    assert!(matches!(
        crate::formats::native_import(&old),
        Err(IoError::NewerVersion(_))
    ));
    assert!(crate::formats::native_import(&content).is_err());
    for _ in 0..2 {
        let xml = String::from_utf8(export(&book).unwrap()).unwrap();
        assert_eq!(xml.matches(&format!(r#"id="{source}""#)).count(), 1);
        assert_eq!(xml.matches(&format!(r#"parent="{source}""#)).count(), 2);
        book = load(&xml).workbook;
        assert_eq!(
            book.active().canvas.groups[0].drawio_source_id.as_deref(),
            Some(source.as_str())
        );
    }
    assert_eq!(regroup(&mut book), next);
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(!xml.contains(&format!(r#"id="{source}""#)));
    assert!(xml.contains(&format!(r#"id="{source}-1""#)));
}

#[test]
fn drawio_nested_and_small_groups_disclose_unretained_identity() {
    for children in [
        "",
        r#"<mxCell id="a" vertex="1" parent="outer"><mxGeometry width="60" height="40" as="geometry"/></mxCell>"#,
        concat!(
            r#"<mxCell id="inner" style="group" vertex="1" parent="outer"><mxGeometry width="200" height="100" as="geometry"/></mxCell>"#,
            r#"<mxCell id="a" vertex="1" parent="inner"><mxGeometry width="60" height="40" as="geometry"/></mxCell>"#,
            r#"<mxCell id="b" vertex="1" parent="inner"><mxGeometry x="100" width="60" height="40" as="geometry"/></mxCell>"#,
        ),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="outer" style="group" vertex="1" parent="1"><mxGeometry width="300" height="120" as="geometry"/></mxCell>{children}"#
        ));
        let report = load(&xml);
        assert!(report.warnings.contains(&ImportWarning::Other));
        if children.contains("inner") {
            assert_eq!(
                report.workbook.active().canvas.groups[0]
                    .drawio_source_id
                    .as_deref(),
                Some("outer")
            );
            assert!(
                !report
                    .workbook
                    .active()
                    .drawio
                    .as_ref()
                    .unwrap()
                    .cell_ids
                    .iter()
                    .any(|id| id == "inner")
            );
        } else {
            assert!(report.workbook.active().canvas.groups.is_empty());
        }
    }
}

#[test]
fn drawio_group_source_ids_handle_collisions_and_budget_limits() {
    let cells = concat!(
        r#"<mxCell id="group-source" style="group" vertex="1" parent="1"><mxGeometry width="300" height="120" as="geometry"/></mxCell>"#,
        r#"<mxCell id="a" value="A" vertex="1" parent="group-source"><mxGeometry width="60" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="b" value="B" vertex="1" parent="group-source"><mxGeometry x="100" width="60" height="40" as="geometry"/></mxCell>"#,
    );
    let mut book = load(&page(cells)).workbook;
    let sheet = book.active_mut();
    let object = sheet
        .canvas
        .objects
        .iter_mut()
        .find(|object| object.label() == "A")
        .unwrap();
    object.drawio = Some(Arc::new(Passthrough {
        source_id: Some("group-source".into()),
        ..Default::default()
    }));
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert_eq!(xml.matches(r#"id="group-source""#).count(), 1);
    let report = load(&xml);
    assert!(
        report
            .workbook
            .active()
            .canvas
            .objects
            .iter()
            .any(|object| {
                object.label() == "A"
                    && object
                        .drawio
                        .as_ref()
                        .and_then(|data| data.source_id.as_deref())
                        == Some("group-source")
            })
    );
    assert_ne!(
        report.workbook.active().canvas.groups[0]
            .drawio_source_id
            .as_deref(),
        Some("group-source")
    );

    let budget = thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2;
    let padding = "x".repeat(budget - "padding".len() - 1);
    let xml = format!(
        r#"<mxfile><diagram><mxGraphModel padding="{padding}"><root><mxCell id="0"/><mxCell id="1" parent="0"/>{cells}</root></mxGraphModel></diagram></mxfile>"#
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    let sheet = report.workbook.active();
    assert_eq!(
        sheet.canvas.groups[0].drawio_source_id.as_deref(),
        Some("group-source")
    );
    assert!(
        !sheet
            .drawio
            .as_ref()
            .unwrap()
            .cell_ids
            .iter()
            .any(|id| id == "group-source")
    );
    let xml = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    assert!(xml.contains(r#"id="group-source""#));
}

#[test]
fn drawio_group_source_ids_reserve_sankey_node_and_edge_ids() {
    for node_collision in [true, false] {
        let mut book = thinoda_core::templates::instantiate("sankey-budget");
        let before = thinoda_core::sankey::evaluate(book.active()).unwrap();
        let node = *before.nodes.iter().next().unwrap();
        let edge = book.active().canvas.objects.iter().find(|object| {
            matches!(&object.kind, CanvasObjectKind::Connector(c) if c.sankey.is_some())
        }).unwrap().id;
        let source = if node_collision {
            format!("t{}", node.0)
        } else {
            format!("o{}", edge.0)
        };
        let sheet = book.active_mut();
        let first = sheet
            .add_canvas_object(shape(
                ShapeKind::Rectangle,
                rect(900.0, 900.0, 80.0, 60.0),
                "A",
            ))
            .unwrap();
        let second = sheet
            .add_canvas_object(shape(
                ShapeKind::Rectangle,
                rect(1000.0, 900.0, 80.0, 60.0),
                "B",
            ))
            .unwrap();
        sheet
            .group_board_members(&[BoardMember::Object(first), BoardMember::Object(second)])
            .unwrap();
        sheet.canvas.groups[0].drawio_source_id = Some(source.clone());
        let xml = String::from_utf8(export(&book).unwrap()).unwrap();
        assert_eq!(xml.matches(&format!(r#"id="{source}""#)).count(), 1);
        let report = load(&xml);
        assert!(!report.warnings.contains(&ImportWarning::DrawioSankeyStale));
        assert_eq!(
            report.workbook.active().canvas.groups[0]
                .drawio_source_id
                .as_deref(),
            Some(source.as_str())
        );
        let after = thinoda_core::sankey::evaluate(report.workbook.active()).unwrap();
        let mut before_mass: Vec<_> = before.mass.values().copied().collect();
        let mut after_mass: Vec<_> = after.mass.values().copied().collect();
        before_mass.sort_by(f64::total_cmp);
        after_mass.sort_by(f64::total_cmp);
        assert_eq!(before_mass, after_mass);
    }
}

#[test]
fn drawio_scene_container_ids_survive_as_page_tombstones() {
    let xml = page(concat!(
        r#"<mxCell id="card-source" value="Card" style="shape=note;thinodaCard=sticky;backgroundOutline=1;size=14;" vertex="1" parent="1"><mxGeometry width="120" height="60" as="geometry"/></mxCell>"#,
        r#"<mxCell id="zone-source" value="Zone" style="swimlane;" vertex="1" parent="1"><mxGeometry x="200" width="300" height="200" as="geometry"/></mxCell>"#,
        r#"<mxCell id="group-source" style="group;" vertex="1" parent="1"><mxGeometry x="600" width="200" height="100" as="geometry"/></mxCell>"#,
        r#"<mxCell id="group-child-a" vertex="1" parent="group-source"><mxGeometry width="60" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="group-child-b" vertex="1" parent="group-source"><mxGeometry x="100" width="60" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="scene-source" value="Scene" style="swimlane;thinodaSceneContainer=1;" vertex="1" parent="1"><mxGeometry x="900" width="300" height="200" as="geometry"/></mxCell>"#,
    ));
    let report = load(&xml);
    let sheet = report.workbook.active();
    assert_eq!(cards(sheet).len(), 1);
    assert_eq!(sheet.zones.len(), 1);
    assert_eq!(sheet.canvas.groups.len(), 1);
    assert_eq!(sheet.canvas.scene.containers.len(), 1);
    let native = crate::formats::native_export(&report.workbook).unwrap();
    let reopened = crate::formats::native_import(&native).unwrap();
    let ids = &reopened.active().drawio.as_ref().unwrap().cell_ids;
    for id in [
        "card-source",
        "group-source",
        "group-child-a",
        "group-child-b",
        "scene-source",
        "zone-source",
    ] {
        assert!(ids.iter().any(|retained| retained == id), "missing {id}");
    }
    let first = export(&reopened).unwrap();
    assert!(String::from_utf8_lossy(&first).contains(r#"id="scene-source""#));
    let second = export(&import(&first).unwrap().workbook).unwrap();
    assert!(String::from_utf8_lossy(&second).contains(r#"id="scene-source""#));
}

#[test]
fn drawio_deleted_scene_container_source_id_is_not_reused_by_new_objects() {
    let document = |id: &str| {
        page(&format!(
            r#"<mxCell id="{id}" value="Scene" style="swimlane;thinodaSceneContainer=1;" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#
        ))
    };
    let replace_container = |book: &mut Workbook| {
        let sheet = book.active_mut();
        let container = *sheet.canvas.scene.containers.iter().next().unwrap();
        sheet.remove_canvas_objects(&[container]).unwrap();
        sheet
            .add_canvas_object(shape(
                ShapeKind::Rectangle,
                rect(200.0, 100.0, 80.0, 60.0),
                "New",
            ))
            .unwrap()
    };
    let mut probe = load(&document("old-container")).workbook;
    let source = format!("o{}", replace_container(&mut probe).0);

    let report = load(&document(&source));
    let native = crate::formats::native_export(&report.workbook).unwrap();
    let mut reopened = crate::formats::native_import(&native).unwrap();
    assert_eq!(format!("o{}", replace_container(&mut reopened).0), source);
    let xml = String::from_utf8(export(&reopened).unwrap()).unwrap();
    assert!(!xml.contains(&format!(r#"id="{source}""#)), "{xml}");
    assert!(xml.contains(&format!(r#"id="{source}-1""#)), "{xml}");
    assert_eq!(shapes(load(&xml).workbook.active()).len(), 1);
}

#[test]
fn drawio_scene_container_tombstones_warn_when_page_budget_is_exhausted() {
    let budget = thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2;
    let padding = "x".repeat(budget - "padding".len() - 1);
    let xml = format!(
        r#"<mxfile><diagram><mxGraphModel padding="{padding}"><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="scene-source" value="Scene" style="swimlane;thinodaSceneContainer=1;" vertex="1" parent="1"><mxGeometry width="300" height="200" as="geometry"/></mxCell></root></mxGraphModel></diagram></mxfile>"#
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert_eq!(report.workbook.active().canvas.scene.containers.len(), 1);
    assert!(
        !report
            .workbook
            .active()
            .drawio
            .as_ref()
            .unwrap()
            .cell_ids
            .iter()
            .any(|id| id == "scene-source")
    );
    report.workbook.validate().unwrap();
}

#[test]
fn drawio_deleted_layer_ids_are_not_reused_by_new_layers() {
    let document = |id: &str| {
        format!(
            r#"<mxfile><diagram><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="{id}" value="Old" parent="0"/></root></mxGraphModel></diagram></mxfile>"#
        )
    };
    let mut probe = load(&document("old")).workbook;
    let old = probe.active().canvas.layers[0].id;
    probe.active_mut().remove_board_layer(old).unwrap();
    let new = probe.active_mut().add_board_layer("New").unwrap();
    let source_id = format!("l{}", new.0);

    let report = load(&document(&source_id));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(
        report
            .workbook
            .active()
            .drawio
            .as_ref()
            .unwrap()
            .cell_ids
            .contains(&source_id)
    );
    let native = crate::formats::native_export(&report.workbook).unwrap();
    let mut book = crate::formats::native_import(&native).unwrap();
    let old = book.active().canvas.layers[0].id;
    book.active_mut().remove_board_layer(old).unwrap();
    let new = book.active_mut().add_board_layer("New").unwrap();
    assert_eq!(format!("l{}", new.0), source_id);

    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(!xml.contains(&format!(r#"id="{source_id}""#)), "{xml}");
    assert!(xml.contains(&format!(r#"id="{source_id}-1""#)), "{xml}");
    let reopened = load(&xml);
    assert_eq!(reopened.workbook.active().canvas.layers[0].name, "New");
}

#[test]
fn drawio_layer_tombstones_obey_the_page_budget_and_warn_when_dropped() {
    let budget = thinoda_core::drawio_passthrough::MAX_SHEET_PASSTHROUGH_BYTES / 2;
    let padding = "x".repeat(budget - "padding".len() - 1);
    let xml = format!(
        r#"<mxfile><diagram><mxGraphModel padding="{padding}"><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="layer-source" value="Layer" parent="0"/></root></mxGraphModel></diagram></mxfile>"#
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    let sheet = report.workbook.active();
    assert_eq!(
        sheet.canvas.layers[0].source_id.as_deref(),
        Some("layer-source")
    );
    assert!(
        !sheet
            .drawio
            .as_ref()
            .unwrap()
            .cell_ids
            .contains(&"layer-source".to_owned())
    );
    assert!(report.workbook.validate().is_ok());
}

#[test]
fn drawio_named_first_layer_and_layered_cards_stay_explicit() {
    let xml = concat!(
        r#"<mxfile><diagram name="P"><mxGraphModel><root><mxCell id="0"/>"#,
        r#"<mxCell id="1" value="Sketch" parent="0"/>"#,
        r#"<mxCell id="a" value="A" style="rounded=0;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#,
        r#"<mxCell id="2" value="Cards" parent="0"/>"#,
        r#"<mxCell id="c" value="Card" style="shape=note;thinodaCard=sticky;backgroundOutline=1;size=14;" vertex="1" parent="2"><mxGeometry x="100" width="120" height="60" as="geometry"/></mxCell>"#,
        r#"</root></mxGraphModel></diagram></mxfile>"#,
    );
    let mut report = load(xml);
    assert!(
        report
            .warnings
            .contains(&ImportWarning::DrawioLayerContentFlattened),
        "{:?}",
        report.warnings
    );
    assert_eq!(cards(&report.workbook.sheets[0]).len(), 1);
    let sheet = report.workbook.sheets.remove(0);
    assert_eq!(sheet.canvas.layers.len(), 2);
    assert_eq!(sheet.canvas.layers[0].name, "Sketch");
    // Reserved ids are never kept as layer source ids.
    assert_eq!(sheet.canvas.layers[0].source_id, None);
    assert!(sheet.canvas.objects.iter().all(|o| o.layer.is_some()));
    let out = String::from_utf8(export(&Workbook::new(sheet)).unwrap()).unwrap();
    assert!(out.contains(r#"value="Sketch" parent="0"/>"#), "{out}");
}

type TableSummary = (
    Vec<f32>,
    Vec<f32>,
    Vec<(u16, u16, u16, u16, String)>,
    thinoda_core::scene_table::TableStripes,
);

/// Grid, spans and labels of the only table, independent of object ids.
fn table_summary(sheet: &Sheet) -> TableSummary {
    assert_eq!(sheet.canvas.scene.tables.len(), 1);
    let (id, table) = sheet.canvas.scene.tables.iter().next().unwrap();
    assert!(table.validate().is_ok());
    assert!(sheet.canvas.scene.containers.contains(id));
    let cells = table
        .cells
        .iter()
        .map(|cell| {
            assert_eq!(sheet.canvas.scene.parents.get(&cell.id), Some(id));
            let object = sheet.canvas.object(cell.id).unwrap();
            (
                cell.row,
                cell.column,
                cell.row_span,
                cell.column_span,
                object.label().to_string(),
            )
        })
        .collect();
    (
        table.rows.clone(),
        table.columns.clone(),
        cells,
        table.stripes,
    )
}

#[test]
fn drawio_tables_import_merges_stripes_and_cell_content_and_round_trip() {
    let cell = |id: &str, parent: &str, x: f32, w: f32, h: f32, extra: &str, visible: bool| {
        format!(
            r#"<mxCell id="{id}" value="{}" style="shape=partialRectangle;html=1;whiteSpace=wrap;connectable=0;strokeColor=inherit;overflow=hidden;fillColor=none;top=0;left=0;bottom=0;right=0;pointerEvents=1;fontColor=#000000;{extra}" {} vertex="1" parent="{parent}"><mxGeometry x="{x}" width="{w}" height="{h}" as="geometry"><mxRectangle width="{}" height="40" as="alternateBounds"/></mxGeometry></mxCell>"#,
            if visible { id } else { "" },
            if visible { "" } else { r#"visible="0""# },
            if extra.contains("colspan") { 60.0 } else { w },
        )
    };
    let row = |id: &str, y: f32| {
        format!(
            r#"<mxCell id="{id}" style="shape=tableRow;horizontal=0;startSize=0;swimlaneHead=0;swimlaneBody=0;strokeColor=inherit;top=0;left=0;bottom=0;right=0;collapsible=0;dropTarget=0;fillColor=none;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;" vertex="1" parent="t"><mxGeometry y="{y}" width="180" height="40" as="geometry"/></mxCell>"#
        )
    };
    let xml = page(&format!(
        r#"<mxCell id="t" value="Table" style="shape=table;startSize=30;container=1;collapsible=0;childLayout=tableLayout;strokeColor=#333333;fillColor=#FFFFFF;fontColor=#000000;evenRowColor=#EEEEEE;oddColumnColor=#DDEEFF;" vertex="1" parent="1"><mxGeometry x="100" y="50" width="180" height="150" as="geometry"/></mxCell>
        {}{}{}{}
        {}{}{}{}
        <mxCell id="note" value="Inside" style="rounded=0;whiteSpace=wrap;html=1;fillColor=#FFFFFF;strokeColor=#000000;fontColor=#000000;" vertex="1" parent="c10"><mxGeometry x="5" y="5" width="40" height="20" as="geometry"/></mxCell>
        {}{}{}{}"#,
        row("r0", 30.0),
        cell("c00", "r0", 0.0, 120.0, 40.0, "colspan=2;", true),
        cell("c01", "r0", 60.0, 60.0, 40.0, "", false),
        cell("c02", "r0", 120.0, 60.0, 40.0, "", true),
        row("r1", 70.0),
        cell("c10", "r1", 0.0, 60.0, 40.0, "", true),
        cell("c11", "r1", 60.0, 60.0, 40.0, "", true),
        cell("c12", "r1", 120.0, 60.0, 80.0, "rowspan=2;", true),
        row("r2", 110.0),
        cell("c20", "r2", 0.0, 60.0, 40.0, "", true),
        cell("c21", "r2", 60.0, 60.0, 40.0, "", true),
        cell("c22", "r2", 120.0, 60.0, 40.0, "", false),
    ))
    .replace("<mxGraphModel>", r#"<mxGraphModel adaptiveColors="none">"#);
    let sheet = clean(&xml);
    let retained = &sheet.drawio.as_ref().unwrap().cell_ids;
    for source in ["t", "c00", "c02", "c10", "c11", "c12", "c20", "c21"] {
        assert!(retained.iter().any(|id| id == source), "missing {source}");
    }
    let summary = table_summary(&sheet);
    let label = |r: u16, c: u16, rs: u16, cs: u16, label: &str| (r, c, rs, cs, label.to_string());
    assert_eq!(summary.0, vec![40.0; 3]);
    assert_eq!(summary.1, vec![60.0; 3]);
    assert_eq!(
        summary.2,
        vec![
            label(0, 0, 1, 2, "c00"),
            label(0, 2, 1, 1, "c02"),
            label(1, 0, 1, 1, "c10"),
            label(1, 1, 1, 1, "c11"),
            label(1, 2, 2, 1, "c12"),
            label(2, 0, 1, 1, "c20"),
            label(2, 1, 1, 1, "c21"),
        ]
    );
    assert_eq!(summary.3.even_rows, Some(Color::rgb(0xEE, 0xEE, 0xEE)));
    assert_eq!(summary.3.odd_columns, Some(Color::rgb(0xDD, 0xEE, 0xFF)));
    let (&table, _) = sheet.canvas.scene.tables.iter().next().unwrap();
    assert_eq!(
        thinoda_core::scene_table::table_header(&sheet.canvas, table).map(|h| h.extent),
        Some(30.0)
    );
    let note = sheet
        .canvas
        .objects
        .iter()
        .find(|o| o.label() == "Inside")
        .unwrap();
    let host = sheet
        .canvas
        .objects
        .iter()
        .find(|o| o.label() == "c10")
        .unwrap();
    assert_eq!(sheet.canvas.scene.parents.get(&note.id), Some(&host.id));
    assert_eq!(host.rect, rect(100.0, 120.0, 60.0, 40.0));
    assert_eq!(
        host.style.stroke,
        Some(Color::rgb(0x33, 0x33, 0x33)),
        "inherited grid stroke"
    );
    assert_eq!(note.rect.min, Vec2::new(105.0, 125.0));
    assert!(sheet.validate().is_ok());

    let drawio = String::from_utf8(
        export_bytes(
            &Workbook::new(sheet.clone()),
            Format::DrawIo,
            &ExportOptions::default(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(drawio.contains("childLayout=tableLayout"));
    assert_eq!(drawio.matches("shape=tableRow").count(), 3);
    assert_eq!(drawio.matches(r#"visible="0""#).count(), 2);
    assert!(drawio.contains("colspan=2;") && drawio.contains("rowspan=2;"));
    let mut with_tombstone = sheet.clone();
    let page = with_tombstone
        .drawio
        .get_or_insert_with(|| Arc::new(DrawioPagePassthrough::default()));
    Arc::make_mut(page).cell_ids.push("t-row0".into());
    let reserved = String::from_utf8(export(&Workbook::new(with_tombstone)).unwrap()).unwrap();
    assert!(!reserved.contains(r#"id="t-row0""#), "{reserved}");
    assert!(reserved.contains(r#"id="t-row0x""#), "{reserved}");
    assert_eq!(table_summary(load(&reserved).workbook.active()), summary);
    // The export drops the page's adaptiveColors="none", so fixed colours warn.
    let report = load(&drawio);
    assert_eq!(
        report.warnings.iter().collect::<Vec<_>>(),
        vec![&ImportWarning::Formatting]
    );
    let round_trip = report.workbook.sheets.into_iter().next().unwrap();
    assert_eq!(table_summary(&round_trip), summary);
    assert_eq!(
        round_trip
            .canvas
            .objects
            .iter()
            .find(|o| o.label() == "Inside")
            .unwrap()
            .rect,
        note.rect
    );
}

#[test]
fn native_tables_export_to_drawio_and_back() {
    use thinoda_core::scene_table::{TableAxis, TableStripes};
    let mut sheet = Sheet::new("Board", "Root");
    let table = sheet
        .insert_canvas_table(Vec2::new(20.0, 20.0), 3, 3, None)
        .unwrap();
    let ids: Vec<_> = sheet.canvas.scene.tables[&table]
        .cells
        .iter()
        .map(|c| c.id)
        .collect();
    for (n, id) in ids.iter().enumerate() {
        if let CanvasObjectKind::Shape { label, .. } =
            &mut sheet.canvas.object_mut(*id).unwrap().kind
        {
            *label = format!("cell {n}");
        }
    }
    sheet.merge_canvas_table_cells(table, &ids[0..2]).unwrap();
    sheet
        .merge_canvas_table_cells(table, &[ids[5], ids[8]])
        .unwrap();
    sheet
        .resize_canvas_table_track(table, TableAxis::Column, 0, 80.0)
        .unwrap();
    sheet
        .set_canvas_table_stripes(
            table,
            TableStripes {
                even_rows: Some(Color::rgb(240, 240, 240)),
                ..TableStripes::default()
            },
        )
        .unwrap();
    let summary = table_summary(&sheet);
    let frame = sheet.canvas.object(table).unwrap().rect;
    let drawio = export_bytes(
        &Workbook::new(sheet),
        Format::DrawIo,
        &ExportOptions::default(),
    )
    .unwrap();
    let round_trip = clean(std::str::from_utf8(&drawio).unwrap());
    assert_eq!(table_summary(&round_trip), summary);
    let (&id, _) = round_trip.canvas.scene.tables.iter().next().unwrap();
    assert_eq!(round_trip.canvas.object(id).unwrap().rect, frame);
    assert!(thinoda_core::scene_table::table_header(&round_trip.canvas, id).is_none());
}
