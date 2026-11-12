use thinoda_core::{
    ApproxMeasure, Document, Sheet, Vec2,
    board::{
        BoardMember, CanvasConnector, CanvasObject, CanvasObjectKind, Card, CardKind, Endpoint,
    },
    clipboard, layout_sheet,
    sankey::{self, SankeyMode},
    templates,
};

fn flow_mut(sheet: &mut Sheet, index: usize) -> &mut CanvasConnector {
    let CanvasObjectKind::Connector(c) = &mut sheet.canvas.objects[index].kind else {
        panic!("connector")
    };
    c
}

#[test]
fn sankey_invalid_rotated_clipboard_is_rejected_without_changing_destination() {
    let book = templates::instantiate("sankey-budget");
    let sheet = book.active();
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    let payload = clipboard::copy_board_selection(sheet, &layout, &sheet.floating, &[])
        .unwrap()
        .unwrap();
    let mut json = serde_json::to_value(payload).unwrap();
    let topics = json["fragment"]["topics"].as_object_mut().unwrap();
    let topic = topics.get_mut(&sheet.floating[0].0.to_string()).unwrap();
    topic["card"]["rotation"] = serde_json::json!(37.0);
    let payload = serde_json::from_value(json).unwrap();
    let mut destination = Sheet::new("Paste", "Unchanged destination");
    let before = destination.clone();
    let error =
        clipboard::paste_board_selection(&mut destination, &payload, Vec2::ZERO).unwrap_err();
    assert!(error.ends_with(sankey::ROTATED_ENDPOINT_ERROR), "{error}");
    assert_eq!(destination, before);
}

#[test]
fn sankey_cap_diagnostic_only_claims_remainder_when_changed_edge_is_known() {
    let book = templates::instantiate("sankey-budget");
    let mut invalid = book.active().clone();
    flow_mut(&mut invalid, 0).sankey.as_mut().unwrap().weight = Some(60_000);
    let error = sankey::evaluate(&invalid).unwrap_err();
    assert!(error.contains("reduce by 5.000%"), "{error}");
    assert!(!error.contains("remainder"), "{error}");
    let before = book.active().clone();
    let mut doc = Document::new(book);
    let error = doc
        .edit_sheet("Over cap", |sheet| {
            flow_mut(sheet, 0).sankey.as_mut().unwrap().weight = Some(60_000);
            Ok(())
        })
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("remainder for this link is 55.000%"),
        "{error}"
    );
    assert!(!error.contains("remainder: 0%"), "{error}");
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn sankey_unset_color_scale_is_shared_with_scene_without_recoloring_ordinary_links() {
    use thinoda_core::scene::{self, SceneGeometry, SceneId};
    let mut sheet = templates::instantiate("sankey-budget").active().clone();
    sheet.sankey.as_mut().unwrap().color_scale = true;
    let id = sheet.canvas.objects[0].id;
    let connector = flow_mut(&mut sheet, 0);
    connector.color = Some(thinoda_core::Color::rgb(255, 0, 0));
    connector.sankey.as_mut().unwrap().weight = None;
    for semantic in [true, false] {
        if !semantic {
            flow_mut(&mut sheet, 0).sankey = None;
        }
        let scene = scene::build_scene(&sheet, &layout_sheet(&sheet, &ApproxMeasure, false));
        let SceneGeometry::Polyline { color, .. } =
            &scene.item(SceneId::Object(id)).unwrap().geometry
        else {
            panic!("unset/ordinary connector must remain visible");
        };
        assert_eq!(*color, semantic.then(|| sankey::weight_color(0.0)));
    }
}

#[test]
fn sankey_six_starters_have_original_explanatory_notes() {
    let starters: Vec<_> = templates::TEMPLATES
        .iter()
        .filter(|t| t.category == templates::TemplateCategory::Sankey)
        .collect();
    assert_eq!(starters.len(), 6);
    for template in starters {
        let book = templates::instantiate(template.id);
        assert!(book.active().floating.iter().all(|id| {
            book.active().topics[id]
                .notes
                .as_ref()
                .is_some_and(|n| n.contains("100%"))
        }));
    }
}

#[test]
fn sankey_lossy_probability_conversion_rejects_without_mutation() {
    let mut sheet = templates::instantiate("sankey-budget").active().clone();
    sankey::convert_mode(&mut sheet, SankeyMode::Probability).unwrap();
    let exact = sheet.clone();
    sankey::convert_mode(&mut sheet, SankeyMode::Standard).unwrap();
    sankey::convert_mode(&mut sheet, SankeyMode::Probability).unwrap();
    assert_eq!(sheet, exact);
    flow_mut(&mut sheet, 0).sankey.as_mut().unwrap().weight = Some(5_000);
    let before = sheet.clone();
    let error = sankey::convert_mode(&mut sheet, SankeyMode::Standard).unwrap_err();
    assert!(error.contains("unallocated") && error.contains("Available budget"));
    assert_eq!(sheet, before);
}

#[test]
fn sankey_unset_is_selectable_placeholder_but_zero_has_no_scene_item() {
    use thinoda_core::scene::{self, SceneGeometry, SceneId};
    let mut sheet = templates::instantiate("sankey-budget").active().clone();
    let id = sheet.canvas.objects[0].id;
    flow_mut(&mut sheet, 0).sankey.as_mut().unwrap().weight = None;
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let scene = scene::build_scene(&sheet, &layout);
    let item = scene.item(SceneId::Object(id)).unwrap();
    assert!(matches!(
        item.geometry,
        SceneGeometry::Polyline {
            label_bounds: Some(_),
            ..
        }
    ));
    assert!(sankey::flow_label(flow_mut(&mut sheet, 0), 0.0).contains("—%"));
    flow_mut(&mut sheet, 0).sankey.as_mut().unwrap().weight = Some(0);
    assert!(
        scene::build_scene(&sheet, &layout)
            .item(SceneId::Object(id))
            .is_none()
    );
}

#[test]
fn sankey_label_bounds_and_feedback_origin_follow_real_scene() {
    use thinoda_core::scene::{self, SceneGeometry, SceneId};
    let mut sheet = templates::instantiate("sankey-circular").active().clone();
    let id = sheet.canvas.objects[3].id;
    flow_mut(&mut sheet, 3).label =
        "A long descriptive recycling link requiring multiple wrapped lines".into();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let original = scene::build_scene(&sheet, &layout);
    let item = original.item(SceneId::Object(id)).unwrap();
    let SceneGeometry::Ribbon { label_bounds, .. } = &item.geometry else {
        panic!()
    };
    assert!(item.bounds.contains(label_bounds.min));
    assert!(item.bounds.contains(label_bounds.max()));
    let first = item.bounds;
    for topic in sheet.topics.values_mut() {
        if let Some(position) = &mut topic.position {
            position.y += 1000.0;
        }
    }
    let shifted = scene::build_scene(&sheet, &layout_sheet(&sheet, &ApproxMeasure, false));
    let second = shifted.item(SceneId::Object(id)).unwrap().bounds;
    assert!((second.min.y - first.min.y - 1000.0).abs() < 0.1);
}

#[test]
fn sankey_rejects_rotation_and_hiding_without_resetting_card() {
    let mut doc = Document::new(templates::instantiate("sankey-budget"));
    let before = doc.sheet().clone();
    let node = before.floating[0];
    let error = doc
        .edit_sheet("rotate", |s| {
            s.topics
                .get_mut(&node)
                .unwrap()
                .card
                .as_mut()
                .unwrap()
                .rotation = 45.0;
            Ok(())
        })
        .unwrap_err();
    assert!(error.to_string().contains("rotated"));
    assert_eq!(doc.sheet(), &before);
    let mut sheet = Sheet::new("Map", "Root");
    let parent = sheet.add_child(sheet.root, None, "Visible parent");
    let child = sheet.add_child(parent, None, "Weighted descendant");
    sheet.sankey = Some(Default::default());
    let mut c = CanvasConnector::new(Endpoint::Topic(parent), Endpoint::Topic(child));
    c.sankey = Some(sankey::Flow {
        weight: Some(10_000),
    });
    sheet.add_canvas_object(CanvasObject::connector(c)).unwrap();
    let mut doc = Document::new(thinoda_core::Workbook::new(sheet));
    let before = doc.sheet().clone();
    let error = doc
        .edit_sheet("collapse", |s| {
            s.topics.get_mut(&parent).unwrap().collapsed = true;
            Ok(())
        })
        .unwrap_err();
    assert!(error.to_string().contains("Weighted descendant"));
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn sankey_isolated_locked_source_settings_are_protected_without_edges() {
    let mut book = templates::instantiate("sankey-blank");
    let sheet = book.active_mut();
    let id = sheet.floating[0];
    sheet.sankey.as_mut().unwrap().mode = SankeyMode::Probability;
    sheet.sankey.as_mut().unwrap().sources.insert(id, 0);
    sheet
        .set_board_members_locked(&[BoardMember::Card(id)], true)
        .unwrap();
    let mut doc = Document::new(book);
    let before = doc.sheet().clone();
    assert!(
        doc.edit_sheet("source", |s| {
            s.sankey.as_mut().unwrap().sources.insert(id, 1000);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn sankey_original_templates_validate_and_share_scene_ribbons() {
    for template in templates::TEMPLATES
        .iter()
        .filter(|t| t.category == templates::TemplateCategory::Sankey)
    {
        let book = templates::instantiate(template.id);
        book.validate().unwrap();
        let sheet = book.active();
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        let scene = thinoda_core::scene::build_scene(sheet, &layout);
        let ribbons = scene
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item.geometry,
                    thinoda_core::scene::SceneGeometry::Ribbon { .. }
                )
            })
            .count();
        assert_eq!(ribbons, sheet.canvas.objects.len(), "{}", template.id);
        for item in scene.items {
            assert!(item.bounds.min.x.is_finite() && item.bounds.size.y.is_finite());
        }
    }
}

#[test]
fn sankey_fixed_decimal_caps_and_atomic_undo_redo() {
    let mut book = templates::instantiate("sankey-budget");
    for (i, w) in [33_300, 33_300, 33_400].into_iter().enumerate() {
        flow_mut(book.active_mut(), i)
            .sankey
            .as_mut()
            .unwrap()
            .weight = Some(w);
    }
    book.validate().unwrap();
    let mut doc = Document::new(book);
    let before = doc.sheet().clone();
    assert!(
        doc.edit_sheet("over cap", |s| {
            flow_mut(s, 0).sankey.as_mut().unwrap().weight = Some(33_301);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    doc.edit_sheet("weight", |s| {
        flow_mut(s, 0).sankey.as_mut().unwrap().weight = Some(30_000);
        Ok(())
    })
    .unwrap();
    let after = doc.sheet().clone();
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &after);
}

#[test]
fn sankey_probability_propagates_and_merges_mass_not_conditionals() {
    let book = templates::instantiate("sankey-probability");
    let sheet = book.active();
    let values = sankey::evaluate(sheet).unwrap();
    let on_time = sheet
        .topics
        .values()
        .find(|t| t.title == "On time")
        .unwrap()
        .id;
    let late = sheet
        .topics
        .values()
        .find(|t| t.title == "Late")
        .unwrap()
        .id;
    assert!((values.incoming[&on_time] - 82.0).abs() < 1e-10);
    assert!((values.incoming[&late] - 18.0).abs() < 1e-10);
    assert_eq!(values.mass[&sheet.canvas.objects[4].id], 10.0);
    assert_eq!(values.mass[&sheet.canvas.objects[2].id], 72.0);
}

#[test]
fn sankey_probability_root_mass_and_upstream_edits_validate_atomically() {
    let mut book = templates::instantiate("sankey-probability");
    let sheet = book.active_mut();
    let mut doc = Document::new(thinoda_core::Workbook::new(sheet.clone()));
    let before = doc.sheet().clone();
    assert!(
        doc.edit_sheet("invalid conditional", |s| {
            flow_mut(s, 0).sankey.as_mut().unwrap().weight = Some(90_000);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    sheet.sankey.as_mut().unwrap().sources.clear();
    assert!(
        sankey::evaluate(sheet)
            .unwrap_err()
            .contains("explicit source")
    );
    let root = match flow_mut(sheet, 0).from {
        Endpoint::Topic(id) => id,
        _ => panic!(),
    };
    let child = match flow_mut(sheet, 0).to {
        Endpoint::Topic(id) => id,
        _ => panic!(),
    };
    sheet.sankey.as_mut().unwrap().sources.insert(root, 100_000);
    sheet.sankey.as_mut().unwrap().sources.insert(child, 1_000);
    assert!(
        sankey::evaluate(sheet)
            .unwrap_err()
            .contains("Remove its explicit")
    );
}

#[test]
fn sankey_cycles_require_circular_mode_and_probability_never_recurses() {
    let mut book = templates::instantiate("sankey-circular");
    let sheet = book.active_mut();
    assert!(sankey::evaluate(sheet).is_ok());
    for mode in [SankeyMode::Standard, SankeyMode::Probability] {
        sheet.sankey.as_mut().unwrap().mode = mode;
        assert!(sankey::evaluate(sheet).unwrap_err().contains("cycle"));
    }
}

#[test]
fn sankey_mode_conversion_preserves_mass_or_rejects_without_mutation() {
    let mut book = templates::instantiate("sankey-probability");
    let sheet = book.active_mut();
    let masses = sankey::evaluate(sheet).unwrap().mass;
    sankey::convert_mode(sheet, SankeyMode::Standard).unwrap();
    assert_eq!(sankey::evaluate(sheet).unwrap().mass, masses);
    let before = sheet.clone();
    sankey::convert_mode(sheet, SankeyMode::Probability).unwrap();
    assert_eq!(
        sankey::evaluate(sheet).unwrap().mass,
        sankey::evaluate(&before).unwrap().mass
    );
    let mut book = templates::instantiate("sankey-probability");
    let sheet = book.active_mut();
    flow_mut(sheet, 0).sankey.as_mut().unwrap().weight = Some(33_333);
    flow_mut(sheet, 2).sankey.as_mut().unwrap().weight = Some(33_333);
    let before = sheet.clone();
    assert!(sankey::convert_mode(sheet, SankeyMode::Standard).is_err());
    assert_eq!(sheet, &before);
}

#[test]
fn sankey_alluvial_identity_and_adjacent_stage_validation() {
    let mut book = templates::instantiate("sankey-learning");
    let sheet = book.active_mut();
    let repeated: Vec<_> = sheet
        .topics
        .values()
        .filter(|t| t.title == "Exploring")
        .collect();
    assert_eq!(repeated.len(), 2);
    assert_ne!(repeated[0].id, repeated[1].id);
    let to = match flow_mut(sheet, 0).to {
        Endpoint::Topic(id) => id,
        _ => panic!(),
    };
    sheet.sankey.as_mut().unwrap().stages.insert(to, 2);
    assert!(sankey::evaluate(sheet).unwrap_err().contains("adjacent"));
}

#[test]
fn sankey_zero_and_absent_are_distinct_and_invisible() {
    let mut book = templates::instantiate("sankey-budget");
    let sheet = book.active_mut();
    flow_mut(sheet, 0).sankey.as_mut().unwrap().weight = None;
    flow_mut(sheet, 1).sankey.as_mut().unwrap().weight = Some(0);
    let json = serde_json::to_string(sheet).unwrap();
    let restored: Sheet = serde_json::from_str(&json).unwrap();
    assert_eq!(&restored, sheet);
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    let scene = thinoda_core::scene::build_scene(sheet, &layout);
    assert_eq!(
        scene
            .items
            .iter()
            .filter(|item| matches!(
                item.geometry,
                thinoda_core::scene::SceneGeometry::Ribbon { .. }
            ))
            .count(),
        1
    );
}

#[test]
fn sankey_sheet_clipboard_gates_and_preserves_every_field() {
    let book = templates::instantiate("sankey-probability");
    let text = clipboard::encode_sheet(book.active()).unwrap();
    assert!(text.starts_with("THINODA-SHEET-21\n"));
    assert_eq!(clipboard::decode_sheet(&text).unwrap(), *book.active());
    assert!(clipboard::decode_sheet(&text.replacen("SHEET-21", "SHEET-20", 1)).is_err());
}

#[test]
fn sankey_node_clipboard_keeps_internal_links_and_remaps_graph() {
    let book = templates::instantiate("sankey-budget");
    let sheet = book.active();
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    let payload = clipboard::copy_board_selection(sheet, &layout, &sheet.floating, &[])
        .unwrap()
        .unwrap();
    assert_eq!(payload.source_object_ids().count(), 3);
    let text = clipboard::encode_board_payload(&payload).unwrap();
    assert!(text.starts_with("THINODA-BOARD-CLIPBOARD-18\n"));
    assert!(
        clipboard::decode_board_payload(&text.replacen("CLIPBOARD-18", "CLIPBOARD-17", 1)).is_err()
    );
    let mut destination = Sheet::new("Paste", "Root");
    let result =
        clipboard::paste_board_selection(&mut destination, &payload, Vec2::new(100.0, 100.0))
            .unwrap();
    assert_eq!(result.objects.len(), 3);
    destination.validate().unwrap();
    for object in &destination.canvas.objects {
        let CanvasObjectKind::Connector(c) = &object.kind else {
            panic!()
        };
        let (a, b) = sankey::endpoints(c).unwrap();
        assert!(destination.contains(a) && destination.contains(b));
        assert!(!sheet.contains(a) && !sheet.contains(b));
    }
}

#[test]
fn sankey_probability_clipboard_cap_failure_is_atomic() {
    let book = templates::instantiate("sankey-probability");
    let sheet = book.active();
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    let payload = clipboard::copy_board_selection(sheet, &layout, &sheet.floating, &[])
        .unwrap()
        .unwrap();
    let mut destination = sheet.clone();
    let before = destination.clone();
    assert!(
        clipboard::paste_board_selection(&mut destination, &payload, Vec2::new(100.0, 0.0))
            .is_err()
    );
    assert_eq!(destination, before);
}

#[test]
fn sankey_deletion_removes_incident_edges_in_one_undo() {
    let book = templates::instantiate("sankey-budget");
    let topic = book.active().floating[1];
    let before = book.active().clone();
    let mut doc = Document::new(book);
    doc.edit_sheet("delete node", |s| s.remove_topics(&[topic]))
        .unwrap();
    assert_eq!(doc.sheet().canvas.objects.len(), 2);
    doc.sheet().validate().unwrap();
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn sankey_endpoint_locks_protect_weight_edits() {
    let mut book = templates::instantiate("sankey-budget");
    let sheet = book.active_mut();
    let id = sheet.floating[1];
    sheet
        .set_board_members_locked(&[BoardMember::Card(id)], true)
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(book);
    assert!(
        doc.edit_sheet("weight", |s| {
            flow_mut(s, 0).sankey.as_mut().unwrap().weight = Some(1);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    assert!(
        doc.edit_sheet("flow appearance", |s| {
            s.canvas.objects[0].style.opacity = 0.5;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    doc.edit_sheet("unlock endpoint", |s| {
        s.set_board_members_locked(&[BoardMember::Card(id)], false)
            .map_err(thinoda_core::EditError::Invalid)
    })
    .unwrap();
    doc.edit_sheet("lock flow", |s| {
        s.canvas.objects[0].locked = true;
        Ok(())
    })
    .unwrap();
    assert!(
        doc.edit_sheet("locked flow appearance", |s| {
            s.canvas.objects[0].style.opacity = 0.5;
            Ok(())
        })
        .is_err()
    );
    doc.edit_sheet("unlock flow", |s| {
        s.canvas.objects[0].locked = false;
        Ok(())
    })
    .unwrap();
    doc.edit_sheet("flow appearance", |s| {
        s.canvas.objects[0].style.opacity = 0.5;
        Ok(())
    })
    .unwrap();
}

#[test]
fn sankey_ordinary_connector_width_is_not_a_weight() {
    let mut sheet = Sheet::new("Map", "Root");
    let child = sheet.add_child(sheet.root, None, "Child");
    let mut c = CanvasConnector::new(Endpoint::Topic(sheet.root), Endpoint::Topic(child));
    c.width = 20.0;
    sheet.add_canvas_object(CanvasObject::connector(c)).unwrap();
    assert!(!sheet.has_sankey());
    assert!(sankey::evaluate(&sheet).unwrap().mass.is_empty());
}

#[test]
fn sankey_conversion_rejects_precision_loss_and_new_mass_atomically() {
    assert_eq!(sankey::from_percent(0.0).unwrap(), 0);
    assert_eq!(sankey::from_percent(0.001).unwrap(), 1);
    assert_eq!(sankey::from_percent(0.1 + 0.2).unwrap(), 300);
    assert!(sankey::from_percent(1e-12).is_err());
    assert!(sankey::from_percent(0.001 + 1e-12).is_err());
    let mut book = templates::instantiate("sankey-energy");
    let before = book.active().clone();
    assert!(
        sankey::convert_mode(book.active_mut(), SankeyMode::Probability)
            .unwrap_err()
            .contains("three decimal")
    );
    assert_eq!(book.active(), &before);
    flow_mut(book.active_mut(), 2)
        .sankey
        .as_mut()
        .unwrap()
        .weight = Some(70_000);
    flow_mut(book.active_mut(), 3)
        .sankey
        .as_mut()
        .unwrap()
        .weight = Some(0);
    let before = book.active().clone();
    assert!(
        sankey::convert_mode(book.active_mut(), SankeyMode::Probability)
            .unwrap_err()
            .contains("more mass")
    );
    assert_eq!(book.active(), &before);
    // Two individually legal absolute sources cannot silently become 200% supply.
    let mut book = templates::instantiate("sankey-budget");
    flow_mut(book.active_mut(), 0)
        .sankey
        .as_mut()
        .unwrap()
        .weight = Some(100_000);
    flow_mut(book.active_mut(), 1)
        .sankey
        .as_mut()
        .unwrap()
        .weight = Some(75_000);
    flow_mut(book.active_mut(), 2)
        .sankey
        .as_mut()
        .unwrap()
        .weight = Some(25_000);
    // Make the second source independent, rather than a downstream node.
    let root = book.active().root;
    let source = book.active_mut().add_child(root, None, "Second source");
    flow_mut(book.active_mut(), 1).from = Endpoint::Topic(source);
    flow_mut(book.active_mut(), 2).from = Endpoint::Topic(source);
    let before = book.active().clone();
    assert!(
        sankey::convert_mode(book.active_mut(), SankeyMode::Probability)
            .unwrap_err()
            .contains("combined root")
    );
    assert_eq!(book.active(), &before);
}

#[test]
fn sankey_alluvial_conversion_rejects_skip_stages_atomically() {
    let mut book = templates::instantiate("sankey-energy");
    // Root -> service skips the intermediate Useful energy stage.
    let to = flow_mut(book.active_mut(), 2).to;
    flow_mut(book.active_mut(), 1).to = to;
    let before = book.active().clone();
    assert!(
        sankey::convert_mode(book.active_mut(), SankeyMode::Alluvial)
            .unwrap_err()
            .contains("adjacent")
    );
    assert_eq!(book.active(), &before);
}

#[test]
fn sankey_incident_deletion_obeys_other_endpoint_lock() {
    let mut book = templates::instantiate("sankey-budget");
    let source = book.active().floating[0];
    let child = book.active().floating[1];
    book.active_mut()
        .set_board_members_locked(&[BoardMember::Card(child)], true)
        .unwrap();
    let before = book.active().clone();
    let mut doc = Document::new(book);
    assert!(
        doc.edit_sheet("delete source", |s| s.remove_topics(&[source]))
            .is_err()
    );
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn sankey_removing_final_probability_supply_records_zero_not_implicit_mass() {
    let book = templates::instantiate("sankey-probability");
    let child = match &book.active().canvas.objects[0].kind {
        CanvasObjectKind::Connector(c) => sankey::endpoints(c).unwrap().1,
        _ => panic!(),
    };
    let before = book.active().clone();
    let mut doc = Document::new(book);
    doc.edit_sheet("remove supply", |s| {
        s.canvas.objects.remove(0);
        Ok(())
    })
    .unwrap();
    assert_eq!(doc.sheet().sankey.as_ref().unwrap().sources[&child], 0);
    assert_eq!(sankey::evaluate(doc.sheet()).unwrap().outgoing[&child], 0.0);
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn sankey_ribbon_widths_hits_and_feedback_bounds_use_shared_geometry() {
    use thinoda_core::scene::{self, SceneGeometry, SceneHit, SceneId};
    let book = templates::instantiate("sankey-budget");
    let sheet = book.active();
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    let scene = scene::build_scene(sheet, &layout);
    let mut widths = Vec::new();
    for object in &sheet.canvas.objects {
        let item = scene.item(SceneId::Object(object.id)).unwrap();
        let SceneGeometry::Ribbon { outline, .. } = &item.geometry else {
            panic!()
        };
        widths.push((outline[0].y - outline.last().unwrap().y).abs());
        // Mid-curve is away from topic cards and within the filled band.
        let point = (outline[16] + outline[outline.len() - 17]) * 0.5;
        assert_eq!(scene.hit_test(point), Some(SceneHit::Object(object.id)));
        assert_eq!(
            scene::object_bounds(sheet, &layout, object),
            item.visible_bounds()
        );
    }

    assert!((widths[0] / widths[1] - 55.0 / 25.0).abs() < 0.0001);
    let book = templates::instantiate("sankey-circular");
    let sheet = book.active();
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    let bounds = scene::canvas_bounds(sheet, &layout).unwrap();
    assert!(
        bounds.min.y
            < layout
                .topics
                .values()
                .map(|t| t.rect.min.y)
                .fold(f32::INFINITY, f32::min)
    );
    assert!(layout.bounds.min.y <= bounds.min.y);
    assert!(layout.bounds.max().y >= bounds.max().y);
    let scene = scene::build_scene(sheet, &layout);
    let id = sheet.canvas.objects.last().unwrap().id;
    let SceneGeometry::Ribbon { outline, .. } = &scene.item(SceneId::Object(id)).unwrap().geometry
    else {
        panic!()
    };
    let anchor = sankey::ribbon_label_anchor(outline).unwrap();
    assert_eq!(scene.hit_test(anchor), Some(SceneHit::Object(id)));
}

#[test]
fn sankey_locked_graph_sheets_can_be_copied_moved_and_removed() {
    let mut book = templates::instantiate("sankey-budget");
    let id = book.active().floating[0];
    book.active_mut()
        .set_board_members_locked(&[BoardMember::Card(id)], true)
        .unwrap();
    book.sheets.push(Sheet::new("Other sheet", "Root"));
    let mut doc = Document::new(book);
    doc.duplicate_sheet(0).unwrap();
    doc.move_sheet(0, 2).unwrap();
    doc.remove_sheet(0).unwrap();
    doc.workbook().validate().unwrap();
    assert!(doc.undo());
    assert!(doc.undo());
    assert!(doc.undo());
    assert_eq!(doc.workbook().sheets.len(), 2);
}

#[test]
fn sankey_mindmap_subtree_clipboard_preserves_ownership_metadata_and_internal_flow() {
    let mut sheet = Sheet::new("Map", "Root");
    let first = sheet.add_child(sheet.root, None, "First");
    let second = sheet.add_child(first, None, "Second");
    sheet.topics.get_mut(&second).unwrap().notes = Some("Shared topic notes".into());
    sheet.sankey = Some(Default::default());
    let mut c = CanvasConnector::new(Endpoint::Topic(first), Endpoint::Topic(second));
    c.sankey = Some(sankey::Flow {
        weight: Some(40_000),
    });
    sheet.add_canvas_object(CanvasObject::connector(c)).unwrap();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let payload = clipboard::copy_board_selection(&sheet, &layout, &[first], &[])
        .unwrap()
        .unwrap();
    let mut target = Sheet::new("Paste", "Root");
    clipboard::paste_board_selection(&mut target, &payload, Vec2::new(50.0, 50.0)).unwrap();
    target.validate().unwrap();
    let parent = target.topics.values().find(|t| t.title == "First").unwrap();
    let child = target
        .topics
        .values()
        .find(|t| t.title == "Second")
        .unwrap();
    assert_eq!(parent.children, vec![child.id]);
    assert_eq!(child.notes.as_deref(), Some("Shared topic notes"));
    assert_eq!(
        sankey::evaluate(&target)
            .unwrap()
            .mass
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![40.0]
    );
    sheet.topics.get_mut(&first).unwrap().collapsed = true;
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let scene = thinoda_core::scene::build_scene(&sheet, &layout);
    assert!(
        scene
            .item(thinoda_core::scene::SceneId::Object(
                sheet.canvas.objects[0].id
            ))
            .is_none()
    );
}

#[test]
fn sankey_auto_arrange_reduces_adjacent_layer_crossings() {
    let mut sheet = Sheet::new("Crossing flow", "Root");
    sheet.sankey = Some(Default::default());
    let add_node = |sheet: &mut Sheet, title, x, y| {
        sheet
            .add_card(
                title,
                Vec2::new(x, y),
                Card::new(CardKind::Text, Vec2::new(155.0, 100.0)),
            )
            .unwrap()
    };
    let source_top = add_node(&mut sheet, "Source top", 0.0, 0.0);
    let source_bottom = add_node(&mut sheet, "Source bottom", 0.0, 250.0);
    let middle_top = add_node(&mut sheet, "Middle top", 420.0, 0.0);
    let middle_bottom = add_node(&mut sheet, "Middle bottom", 420.0, 250.0);
    let target_top = add_node(&mut sheet, "Target top", 840.0, 0.0);
    let target_bottom = add_node(&mut sheet, "Target bottom", 840.0, 250.0);

    for (from, to) in [
        (source_top, middle_top),
        (source_bottom, middle_bottom),
        (middle_top, target_bottom),
        (middle_bottom, target_top),
    ] {
        let mut connector = CanvasConnector::new(Endpoint::Topic(from), Endpoint::Topic(to));
        connector.sankey = Some(sankey::Flow {
            weight: Some(50_000),
        });
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
    }

    sankey::arrange(&mut sheet).unwrap();
    let y = |id| sheet.topic(id).unwrap().position.unwrap().y;
    assert!(y(source_top) < y(source_bottom));
    assert!(y(middle_top) < y(middle_bottom));
    assert!(
        y(target_bottom) < y(target_top),
        "the connected targets should be reordered to remove the crossing"
    );
}

#[test]
fn sankey_probability_layout_orders_by_overall_flow_mass() {
    let mut sheet = Sheet::new("Probability ordering", "Root");
    sheet.sankey = Some(thinoda_core::sankey::SankeySettings {
        mode: SankeyMode::Probability,
        ..Default::default()
    });
    let add_node = |sheet: &mut Sheet, title, x, y| {
        sheet
            .add_card(
                title,
                Vec2::new(x, y),
                Card::new(CardKind::Text, Vec2::new(155.0, 100.0)),
            )
            .unwrap()
    };
    let source_top = add_node(&mut sheet, "Source top", 0.0, 0.0);
    let source_middle = add_node(&mut sheet, "Source middle", 0.0, 250.0);
    let source_bottom = add_node(&mut sheet, "Source bottom", 0.0, 500.0);
    let branch_top = add_node(&mut sheet, "Branch top", 420.0, 0.0);
    let branch_middle = add_node(&mut sheet, "Branch middle", 420.0, 250.0);
    let branch_bottom = add_node(&mut sheet, "Branch bottom", 420.0, 500.0);
    let target_x = add_node(&mut sheet, "Target X", 840.0, 0.0);
    let target_y = add_node(&mut sheet, "Target Y", 840.0, 250.0);
    sheet.sankey.as_mut().unwrap().sources.extend([
        (source_top, 98_000),
        (source_middle, 1_000),
        (source_bottom, 1_000),
    ]);

    for (from, to, weight) in [
        (source_top, branch_top, 100_000),
        (source_middle, branch_middle, 100_000),
        (source_bottom, branch_bottom, 100_000),
        (branch_top, target_x, 1_000),
        (branch_middle, target_y, 100_000),
        (branch_bottom, target_x, 2_000),
    ] {
        let mut connector = CanvasConnector::new(Endpoint::Topic(from), Endpoint::Topic(to));
        connector.sankey = Some(sankey::Flow {
            weight: Some(weight),
        });
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
    }

    sankey::arrange(&mut sheet).unwrap();
    assert!(
        sheet.topic(target_x).unwrap().position.unwrap().y
            < sheet.topic(target_y).unwrap().position.unwrap().y,
        "layout should use derived overall mass rather than conditional percentages"
    );
}

fn alluvial_chain(
    titles: &[&str],
    links: &[(usize, usize)],
) -> (Sheet, Vec<thinoda_core::TopicId>) {
    let mut sheet = Sheet::new("Stages", "Root");
    sheet.sankey = Some(sankey::SankeySettings {
        mode: SankeyMode::Alluvial,
        ..Default::default()
    });
    let ids: Vec<_> = titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            sheet
                .add_card(
                    title,
                    Vec2::new(420.0 * i as f32, 0.0),
                    Card::new(CardKind::Text, Vec2::new(155.0, 100.0)),
                )
                .unwrap()
        })
        .collect();
    for &(from, to) in links {
        let mut c = CanvasConnector::new(Endpoint::Topic(ids[from]), Endpoint::Topic(ids[to]));
        c.sankey = Some(sankey::Flow {
            weight: Some(10_000),
        });
        sheet.add_canvas_object(CanvasObject::connector(c)).unwrap();
    }
    (sheet, ids)
}

#[test]
fn sankey_assign_missing_stages_derives_from_neighbours_and_seeds_new_components() {
    let (mut sheet, ids) = alluvial_chain(
        &["A", "B", "C", "D", "X", "Y"],
        &[(0, 1), (1, 2), (2, 3), (4, 5)],
    );
    sheet.sankey.as_mut().unwrap().stages.insert(ids[1], 4);
    let summary = sankey::stage_summary(&sheet);
    assert_eq!(summary.stages, vec![(4, vec![ids[1]])]);
    assert_eq!(summary.unassigned.len(), 5);

    assert_eq!(sankey::assign_missing_stages(&mut sheet).unwrap(), 5);
    let stages = &sheet.sankey.as_ref().unwrap().stages;
    let got: Vec<_> = ids.iter().map(|id| stages[id]).collect();
    assert_eq!(got, vec![3, 4, 5, 6, 0, 1]);
    sankey::evaluate(&sheet).unwrap();
    assert_eq!(sankey::assign_missing_stages(&mut sheet).unwrap(), 0);
}

#[test]
fn sankey_assign_missing_stages_rejects_conflicts_and_locks_atomically() {
    let (mut sheet, ids) = alluvial_chain(&["A", "B", "C"], &[(0, 2), (1, 2)]);
    let stages = &mut sheet.sankey.as_mut().unwrap().stages;
    stages.insert(ids[0], 0);
    stages.insert(ids[1], 2);
    let before = sheet.clone();
    assert!(
        sankey::assign_missing_stages(&mut sheet)
            .unwrap_err()
            .contains("adjacent")
    );
    assert_eq!(sheet, before);

    let (mut sheet, ids) = alluvial_chain(&["A", "B"], &[(0, 1)]);
    sheet.sankey.as_mut().unwrap().stages.insert(ids[0], 0);
    sheet
        .set_board_members_locked(&[BoardMember::Card(ids[1])], true)
        .unwrap();
    let before = sheet.clone();
    assert!(
        sankey::assign_missing_stages(&mut sheet)
            .unwrap_err()
            .contains("Unlock")
    );
    assert_eq!(sheet, before);

    let (mut standard, _) = alluvial_chain(&["A", "B"], &[(0, 1)]);
    standard.sankey.as_mut().unwrap().mode = SankeyMode::Standard;
    assert!(sankey::assign_missing_stages(&mut standard).is_err());
}

#[test]
fn sankey_compact_stages_closes_gaps_and_keeps_adjacency() {
    let (mut sheet, ids) = alluvial_chain(&["A", "B", "Lone"], &[(0, 1)]);
    let stages = &mut sheet.sankey.as_mut().unwrap().stages;
    stages.extend([(ids[0], 2), (ids[1], 3), (ids[2], 7)]);
    sankey::evaluate(&sheet).unwrap();
    assert!(sankey::compact_stages(&mut sheet).unwrap());
    let stages = &sheet.sankey.as_ref().unwrap().stages;
    assert_eq!(
        ids.iter().map(|id| stages[id]).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    sankey::evaluate(&sheet).unwrap();
    assert!(!sankey::compact_stages(&mut sheet).unwrap());
}

#[test]
fn sankey_assign_missing_stages_is_order_independent_and_always_validates() {
    // A→C, B→D, D→C under both topic-ID orderings of the two roots.
    for (a, b) in [(0, 1), (1, 0)] {
        let mut links = vec![(a, 2), (b, 3), (3, 2)];
        links.sort();
        let (mut sheet, ids) = alluvial_chain(&["R0", "R1", "C", "D"], &links);
        assert_eq!(sankey::assign_missing_stages(&mut sheet).unwrap(), 4);
        let stages = &sheet.sankey.as_ref().unwrap().stages;
        assert_eq!(
            [ids[a], ids[b], ids[3], ids[2]].map(|id| stages[&id]),
            [1, 0, 1, 2]
        );
    }

    let (mut cycle, _) = alluvial_chain(&["A", "B"], &[(0, 1), (1, 0)]);
    let before = cycle.clone();
    assert!(sankey::assign_missing_stages(&mut cycle).is_err());
    assert_eq!(cycle, before);

    let (mut skipped, ids) = alluvial_chain(&["A", "B"], &[(0, 1)]);
    let stages = &mut skipped.sankey.as_mut().unwrap().stages;
    stages.extend([(ids[0], 0), (ids[1], 2)]);
    assert!(
        sankey::assign_missing_stages(&mut skipped)
            .unwrap_err()
            .contains("adjacent")
    );

    let (mut below_zero, ids) = alluvial_chain(&["A", "B"], &[(0, 1)]);
    below_zero.sankey.as_mut().unwrap().stages.insert(ids[1], 0);
    assert!(
        sankey::assign_missing_stages(&mut below_zero)
            .unwrap_err()
            .contains("before 0")
    );
}
