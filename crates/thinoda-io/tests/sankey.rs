use thinoda_core::{sankey, templates};
use thinoda_io::{ExportLoss, ExportOptions, Format, export_bytes, export_losses, import_bytes};

fn unchecked_native(book: &thinoda_core::Workbook) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file("manifest.json", SimpleFileOptions::default())
        .unwrap();
    archive
        .write_all(br#"{"format":"thinoda","version":25,"resources":[]}"#)
        .unwrap();
    archive
        .start_file("content.json", SimpleFileOptions::default())
        .unwrap();
    archive
        .write_all(&serde_json::to_vec(book).unwrap())
        .unwrap();
    archive.finish().unwrap().into_inner()
}

#[test]
fn sankey_svg_includes_a_standalone_scale_legend_only_when_enabled() {
    let mut book = templates::instantiate("sankey-budget");
    book.active_mut().sankey.as_mut().unwrap().color_scale = false;
    let without_scale =
        String::from_utf8(export_bytes(&book, Format::Svg, &ExportOptions::default()).unwrap())
            .unwrap();
    assert!(!without_scale.contains("sankey-weight-scale-legend"));

    book.active_mut().sankey.as_mut().unwrap().color_scale = true;
    let with_scale =
        String::from_utf8(export_bytes(&book, Format::Svg, &ExportOptions::default()).unwrap())
            .unwrap();
    assert!(with_scale.contains("<g id=\"sankey-weight-scale-legend\">"));
    for mark in sankey::WEIGHT_COLOR_SCALE_MARKS {
        assert!(
            with_scale.contains(&format!(">{mark:.0}%</text>")),
            "{mark}"
        );
    }
    let svg_width = |svg: &str| {
        svg.split_once("width=\"")
            .unwrap()
            .1
            .split('"')
            .next()
            .unwrap()
            .parse::<f32>()
            .unwrap()
    };
    assert!(svg_width(&with_scale) > svg_width(&without_scale) + 150.0);
}

#[test]
fn sankey_native_load_rejects_rotated_and_collapsed_hidden_endpoints_explicitly() {
    use thinoda_core::{
        Sheet, Workbook,
        board::{CanvasConnector, CanvasObject, Endpoint},
    };
    let mut rotated = templates::instantiate("sankey-budget");
    let id = rotated.active().floating[0];
    rotated
        .active_mut()
        .topics
        .get_mut(&id)
        .unwrap()
        .card
        .as_mut()
        .unwrap()
        .rotation = 37.0;
    let mut hidden = Sheet::new("Hidden flow", "Source");
    let ancestor = hidden.add_child(hidden.root, None, "Collapsed parent");
    let endpoint = hidden.add_child(ancestor, None, "Hidden endpoint");
    hidden.sankey = Some(Default::default());
    let mut connector =
        CanvasConnector::new(Endpoint::Topic(hidden.root), Endpoint::Topic(endpoint));
    connector.sankey = Some(sankey::Flow {
        weight: Some(10_000),
    });
    hidden
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    hidden.topics.get_mut(&ancestor).unwrap().collapsed = true;
    for (book, diagnostic) in [
        (rotated, sankey::ROTATED_ENDPOINT_ERROR),
        (Workbook::new(hidden), sankey::HIDDEN_ENDPOINT_ERROR),
    ] {
        let error = import_bytes(Format::Thinoda, &unchecked_native(&book)).unwrap_err();
        assert!(matches!(error, thinoda_io::IoError::Invalid(_)));
        assert!(error.to_string().ends_with(diagnostic), "{error}");
    }
}

#[test]
fn sankey_drawio_rotated_cell_falls_back_without_resetting_rotation_or_cells() {
    let book = templates::instantiate("sankey-budget");
    let mut xml =
        String::from_utf8(export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap())
            .unwrap();
    let start = xml
        .find(&format!("<mxCell id=\"t{}\"", book.active().floating[0].0))
        .unwrap();
    let end = start + xml[start..].find("</mxCell>").unwrap();
    let old = &xml[start..end];
    assert!(!old.contains("rotation="));
    let rotated = old.replacen("style=\"", "style=\"rotation=37;", 1);
    assert_ne!(rotated, old);
    xml.replace_range(start..end, &rotated);
    let report = thinoda_io::import_bytes_with_report(Format::DrawIo, xml.as_bytes()).unwrap();
    assert!(!report.workbook.has_sankey());
    assert!(
        report
            .warnings
            .contains(&thinoda_io::ImportWarning::DrawioSankeyStale)
    );
    let sheet = report.workbook.active();
    assert_eq!(
        sheet.canvas.objects.len(),
        book.active().canvas.objects.len()
    );
    assert!(sheet.topics.values().any(|topic| {
        topic
            .card
            .as_ref()
            .is_some_and(|card| card.rotation == 37.0)
    }));
    report.workbook.validate().unwrap();
}

#[test]
fn sankey_drawio_zero_and_unset_scale_colors_match_shared_scene_and_svg() {
    use thinoda_core::board::CanvasObjectKind;
    for weight in [None, Some(0)] {
        let mut book = templates::instantiate("sankey-budget");
        book.active_mut().sankey.as_mut().unwrap().color_scale = true;
        let CanvasObjectKind::Connector(c) = &mut book.active_mut().canvas.objects[0].kind else {
            panic!()
        };
        c.sankey.as_mut().unwrap().weight = weight;
        c.color = Some(thinoda_core::Color::rgb(255, 0, 0));
        let id = book.active().canvas.objects[0].id;
        let xml = String::from_utf8(
            export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap(),
        )
        .unwrap();
        let start = xml.find(&format!("<mxCell id=\"o{}\"", id.0)).unwrap();
        let cell = &xml[start..start + xml[start..].find("</mxCell>").unwrap()];
        let color = sankey::weight_color(0.0).to_hex();
        assert!(cell.contains(&format!("strokeColor={color};")), "{cell}");
        if weight.is_none() {
            let svg = String::from_utf8(
                export_bytes(&book, Format::Svg, &ExportOptions::default()).unwrap(),
            )
            .unwrap();
            assert!(svg.contains(&color), "{color}");
        } else {
            assert!(cell.contains("opacity=0;"));
        }
    }
}

#[test]
fn sankey_native_and_xmind_private_snapshot_round_trip() {
    for template in [
        "sankey-budget",
        "sankey-probability",
        "sankey-learning",
        "sankey-circular",
    ] {
        let book = templates::instantiate(template);
        assert_eq!(thinoda_io::required_native_version(&book), 25);
        for format in [Format::Thinoda, Format::XMind] {
            let bytes = export_bytes(&book, format, &ExportOptions::default()).unwrap();
            let restored = import_bytes(format, &bytes).unwrap();
            assert_eq!(restored.active(), book.active(), "{template} {format:?}");
        }
    }
}

#[test]
fn sankey_drawio_extension_preserves_editable_mass_and_topic_metadata() {
    for template in [
        "sankey-budget",
        "sankey-probability",
        "sankey-learning",
        "sankey-circular",
    ] {
        let mut book = templates::instantiate(template);
        let id = book.active().floating[0];
        let topic = book.active_mut().topics.get_mut(&id).unwrap();
        topic.notes = Some("An original fictional example".into());
        topic.hyperlink = Some("https://example.invalid/flow".into());
        let bytes = export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap();
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .contains("thinodaSankey=")
        );
        let restored = import_bytes(Format::DrawIo, &bytes).unwrap();
        restored.validate().unwrap();
        let old = sankey::evaluate(book.active()).unwrap();
        let new = sankey::evaluate(restored.active()).unwrap();
        let mut old_values: Vec<_> = old.mass.values().copied().collect();
        let mut new_values: Vec<_> = new.mass.values().copied().collect();
        old_values.sort_by(f64::total_cmp);
        new_values.sort_by(f64::total_cmp);
        assert_eq!(old_values, new_values, "{template}");
        let topic = restored
            .active()
            .topics
            .values()
            .find(|t| t.title == book.active().topics[&id].title)
            .unwrap();
        assert_eq!(topic.notes, book.active().topics[&id].notes);
        assert_eq!(topic.hyperlink, book.active().topics[&id].hyperlink);
    }
}

#[test]
fn sankey_drawio_never_infers_weight_from_an_ordinary_stroke() {
    let xml=br#"<mxfile><diagram><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="a" value="Source" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="100" as="geometry"/></mxCell>
        <mxCell id="b" value="Destination" vertex="1" parent="1"><mxGeometry x="300" y="0" width="100" height="100" as="geometry"/></mxCell>
        <mxCell id="e" edge="1" source="a" target="b" parent="1" style="strokeWidth=20;"><mxGeometry relative="1" as="geometry"/></mxCell>
        </root></mxGraphModel></diagram></mxfile>"#;
    let book = import_bytes(Format::DrawIo, xml).unwrap();
    assert!(!book.has_sankey());
}

#[test]
fn sankey_preflight_is_honest_about_extension_and_flattening() {
    let book = templates::instantiate("sankey-budget");
    let opts = ExportOptions::default();
    assert!(
        export_losses(&book, Format::DrawIo, &opts)
            .iter()
            .any(|loss| matches!(loss, ExportLoss::SankeyDrawioExtension(1)))
    );
    assert!(
        export_losses(&book, Format::Svg, &opts)
            .iter()
            .any(|loss| matches!(loss, ExportLoss::SankeySemanticsDropped(1)))
    );
    assert!(export_losses(&book, Format::Thinoda, &opts).is_empty());
}

#[test]
fn sankey_svg_uses_filled_ribbons_and_numeric_mass() {
    let book = templates::instantiate("sankey-probability");
    let bytes = export_bytes(&book, Format::Svg, &ExportOptions::default()).unwrap();
    let svg = String::from_utf8(bytes).unwrap();
    assert!(svg.contains("72.000%"));
    assert!(svg.contains("10.000%"));
    assert!(svg.contains("canvas-object"));
}

#[test]
fn sankey_native_rejects_older_envelopes_and_unversioned_json() {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    let book = templates::instantiate("sankey-budget");
    let json = serde_json::to_vec(&book).unwrap();
    assert!(import_bytes(Format::Thinoda, &json).is_err());
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file("manifest.json", SimpleFileOptions::default())
        .unwrap();
    archive
        .write_all(br#"{"format":"thinoda","version":24,"resources":[]}"#)
        .unwrap();
    archive
        .start_file("content.json", SimpleFileOptions::default())
        .unwrap();
    archive.write_all(&json).unwrap();
    let bytes = archive.finish().unwrap().into_inner();
    assert!(import_bytes(Format::Thinoda, &bytes).is_err());
}

#[test]
fn sankey_drawio_unknown_extension_version_falls_back_with_explicit_warning() {
    let book = templates::instantiate("sankey-budget");
    let bytes = export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap();
    let xml = String::from_utf8(bytes).unwrap().replacen(
        "&quot;version&quot;:2",
        "&quot;version&quot;:99",
        1,
    );
    let report = thinoda_io::import_bytes_with_report(Format::DrawIo, xml.as_bytes()).unwrap();
    assert!(!report.workbook.has_sankey());
    assert!(
        report
            .warnings
            .contains(&thinoda_io::ImportWarning::DrawioSankeyStale)
    );
}

#[test]
fn sankey_drawio_external_cell_edits_fall_back_atomically() {
    let book = templates::instantiate("sankey-budget");
    let xml =
        String::from_utf8(export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap())
            .unwrap();
    let sheet = book.active();
    let edge = format!("o{}", sheet.canvas.objects[0].id.0);
    let vertex = format!("t{}", sheet.floating[1].0);
    fn cell(xml: &str, id: &str) -> std::ops::Range<usize> {
        let start = xml.find(&format!("<mxCell id=\"{id}\"")).unwrap();
        let end = start + xml[start..].find("</mxCell>").unwrap() + "</mxCell>".len();
        start..end
    }
    let mut cases = Vec::new();
    for (old, new) in [
        ("&quot;opacity&quot;:1.0", "&quot;opacity&quot;:2.0"),
        ("&quot;weight&quot;:55000", "&quot;weight&quot;:100001"),
    ] {
        let invalid = xml.replacen(old, new, 1);
        assert_ne!(invalid, xml);
        cases.push(invalid);
    }
    for id in [&edge, &vertex] {
        let mut edited = xml.clone();
        edited.replace_range(cell(&xml, id), "");
        cases.push(edited);
    }
    for edit in ["reconnect", "nonedge", "width"] {
        let range = cell(&xml, &edge);
        let old = &xml[range.clone()];
        let replacement = match edit {
            "reconnect" => old.replace(
                &format!("target=\"{vertex}\""),
                &format!("target=\"t{}\"", sheet.floating[2].0),
            ),
            "nonedge" => old.replace("edge=\"1\"", "vertex=\"1\""),
            _ => {
                let at = old.find("strokeWidth=").unwrap();
                let end = at + old[at..].find(';').unwrap() + 1;
                let mut s = old.to_owned();
                s.replace_range(at..end, "strokeWidth=7;");
                s
            }
        };
        assert_ne!(replacement, old);
        let mut edited = xml.clone();
        edited.replace_range(range, &replacement);
        cases.push(edited);
    }
    for edited in cases {
        let report =
            thinoda_io::import_bytes_with_report(Format::DrawIo, edited.as_bytes()).unwrap();
        assert!(!report.workbook.has_sankey());
        assert!(
            report
                .warnings
                .contains(&thinoda_io::ImportWarning::DrawioSankeyStale)
        );
        report.workbook.validate().unwrap();
        assert!(!report.workbook.active().canvas.objects.is_empty());
    }
}

#[test]
fn sankey_unset_drawio_is_dashed_not_invisible_and_svg_has_no_weight_label() {
    use thinoda_core::board::CanvasObjectKind;
    let mut book = templates::instantiate("sankey-budget");
    let CanvasObjectKind::Connector(c) = &mut book.active_mut().canvas.objects[0].kind else {
        panic!()
    };
    c.sankey.as_mut().unwrap().weight = None;
    let xml =
        String::from_utf8(export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap())
            .unwrap();
    assert!(xml.contains("dashed=1;"));
    assert!(xml.contains("—%"));
    assert!(!xml.contains("opacity=0;"));
    let restored = import_bytes(Format::DrawIo, xml.as_bytes()).unwrap();
    assert!(restored.has_sankey());
    let svg =
        String::from_utf8(export_bytes(&book, Format::Svg, &ExportOptions::default()).unwrap())
            .unwrap();
    assert!(svg.contains("—%"));
}

#[test]
fn sankey_drawio_preserved_notes_do_not_claim_loss_and_marker_metadata_stays_valid() {
    use thinoda_core::board::{CanvasObjectKind, ConnectorMarkerKind};
    let mut book = templates::instantiate("sankey-budget");
    let id = book.active().floating[0];
    book.active_mut().topics.get_mut(&id).unwrap().notes =
        Some("Preserved in the explicit extension".into());
    let CanvasObjectKind::Connector(c) = &mut book.active_mut().canvas.objects[0].kind else {
        panic!()
    };
    c.arrow_start = true;
    c.start_marker = Some(thinoda_core::ConnectorMarker {
        kind: ConnectorMarkerKind::Diamond,
    });
    c.start_marker_size = Some(12.0);
    let opts = ExportOptions::default();
    assert!(
        !export_losses(&book, Format::DrawIo, &opts)
            .iter()
            .any(|loss| matches!(loss, ExportLoss::TopicDetailsDropped(_)))
    );
    let bytes = export_bytes(&book, Format::DrawIo, &opts).unwrap();
    let restored = import_bytes(Format::DrawIo, &bytes).unwrap();
    restored.validate().unwrap();
}
