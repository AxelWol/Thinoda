use super::*;
use egui_kittest::{Harness, kittest::Queryable};

fn harness(template: &str, node_index: usize) -> Harness<'static, ThinodaApp> {
    harness_with_size(template, node_index, egui::vec2(440.0, 1600.0))
}

fn harness_with_size(
    template: &str,
    node_index: usize,
    size: egui::Vec2,
) -> Harness<'static, ThinodaApp> {
    crate::i18n::set_lang(crate::i18n::Lang::En);
    let mut app = ThinodaApp::headless();
    app.new_doc(thinoda_core::templates::instantiate(template));
    let doc = app.doc_mut().unwrap();
    let id = doc.doc.sheet().floating[node_index];
    doc.view.select_only(id);
    let mut harness = Harness::builder()
        .with_size(size)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
        })
        .build_ui_state(
            |ui, app: &mut ThinodaApp| {
                app.sankey_properties(ui);
                assert!(
                    !ui.input(|input| input.key_pressed(egui::Key::Enter)),
                    "Number completion must not leak Enter to canvas shortcuts"
                );
            },
            app,
        );
    harness.run_steps(3);
    harness
}

#[test]
fn sankey_inspector_shows_both_sides_and_exact_totals() {
    let h = harness("sankey-budget", 0);
    assert!(h.query_by_label("Parents").is_some());
    assert!(h.query_by_label("Children").is_some());
    assert!(
        h.query_by_label("Total: 100.000% · Unallocated: 0.000%")
            .is_some()
    );
    assert!(h.query_by_label("Weight color scale").is_some());
}

#[test]
fn sankey_probability_parent_list_shows_derived_mass() {
    let h = harness("sankey-probability", 3);
    assert!(
        h.query_by_label("Total: 82.000% · Unallocated: 18.000%")
            .is_some()
    );
    assert!(h.query_by_label("Overall mass: 72.000%").is_some());
    assert!(h.query_by_label("Overall mass: 10.000%").is_some());
    assert!(
        h.query_by_role_and_label(egui::Role::SpinButton, "Clear route")
            .is_none()
    );
    assert!(
        h.query_by_role_and_label(egui::Role::SpinButton, "Delayed route")
            .is_none()
    );
}

#[test]
fn sankey_inspector_edits_one_shared_weight_from_both_sides_and_rejects_caps() {
    let mut h = harness("sankey-budget", 0);
    let before = h.state().doc().unwrap().doc.sheet().clone();
    h.get_by_role_and_label(egui::Role::SpinButton, "Essentials — Weight")
        .set_value_accesskit(60.0);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
    h.get_by_label("Apply");
    h.get_by_label("Cancel");
    h.get_by_role_and_label(egui::Role::SpinButton, "Essentials — Weight")
        .set_value_accesskit(40.0);
    h.run_steps(3);
    let doc = h.state_mut().doc_mut().unwrap();
    let child = doc.doc.sheet().floating[1];
    doc.view.select_only(child);
    h.run_steps(3);
    let before = h.state().doc().unwrap().doc.sheet().clone();
    h.get_by_role_and_label(egui::Role::SpinButton, "Available budget — Weight")
        .set_value_accesskit(60.0);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
    h.get_by_label("Apply");
    h.get_by_label("Cancel");
    h.get_by_role_and_label(egui::Role::SpinButton, "Available budget — Weight")
        .set_value_accesskit(30.0);
    h.run_steps(3);
    let doc = &mut h.state_mut().doc_mut().unwrap().doc;
    let CanvasObjectKind::Connector(c) = &doc.sheet().canvas.objects[0].kind else {
        panic!()
    };
    assert_eq!(c.sankey.unwrap().weight, Some(30_000));
    assert!(doc.undo());
    let CanvasObjectKind::Connector(c) = &doc.sheet().canvas.objects[0].kind else {
        panic!()
    };
    assert_eq!(c.sankey.unwrap().weight, Some(40_000));
}

#[test]
fn sankey_probability_parent_can_navigate_to_edit_upstream_conditional() {
    let mut h = harness("sankey-probability", 3);
    h.get_all_by_role_and_label(egui::Role::Button, "Edit upstream conditional")
        .next()
        .unwrap()
        .click();
    h.run_steps(3);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.view.primary(), Some(doc.doc.sheet().floating[1]));
    h.get_by_role_and_label(egui::Role::SpinButton, "On time — Weight");
}

#[test]
fn sankey_parent_cap_draft_can_be_corrected_or_cancelled() {
    let mut h = harness("sankey-budget", 1);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("merge", |sheet| {
            let mut c = CanvasConnector::new(
                Endpoint::Topic(sheet.floating[3]),
                Endpoint::Topic(sheet.floating[1]),
            );
            c.sankey = Some(Flow {
                weight: Some(40_000),
            });
            sheet
                .add_canvas_object(CanvasObject::connector(c))
                .map_err(thinoda_core::EditError::Invalid)?;
            Ok(())
        })
        .unwrap();
    h.run_steps(3);
    let before = h.state().doc().unwrap().doc.sheet().clone();
    h.get_by_role_and_label(egui::Role::SpinButton, "Reserve — Weight")
        .set_value_accesskit(50.0);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.query_by_label("Cancel").is_none());
    h.get_by_role_and_label(egui::Role::SpinButton, "Reserve — Weight")
        .set_value_accesskit(45.0);
    h.run_steps(3);
    let sheet = h.state().doc().unwrap().doc.sheet();
    assert_eq!(
        sankey::evaluate(sheet).unwrap().incoming[&sheet.floating[1]],
        100.0
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
}

#[test]
fn sankey_keyboard_number_draft_commits_only_on_completion() {
    let mut h = harness("sankey-budget", 0);
    let before = h.state().doc().unwrap().doc.sheet().clone();
    h.get_by_role_and_label(egui::Role::SpinButton, "Essentials — Weight")
        .focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("40".into()));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let doc = &mut h.state_mut().doc_mut().unwrap().doc;
    assert_ne!(doc.sheet(), &before);
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn sankey_fresh_probability_root_can_add_unset_child_with_explicit_zero_supply() {
    let mut h = harness("sankey-blank", 0);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("probability", |sheet| {
            sankey::convert_mode(sheet, SankeyMode::Probability)
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    h.run_steps(3);
    h.get_by_label("Add flow node").click();
    h.run_steps(3);
    h.get_by_label("Explicit source mass").click();
    h.run_steps(3);
    let doc = h.state_mut().doc_mut().unwrap();
    let root = doc.doc.sheet().floating[0];
    doc.view.select_only(root);
    h.run_steps(3);
    h.get_all_by_role_and_label(egui::Role::ComboBox, "Choose topic")
        .last()
        .unwrap()
        .click();
    h.run_steps(3);
    h.get_by_label("New flow node").click();
    h.run_steps(3);
    h.get_all_by_role_and_label(egui::Role::Button, "Add child")
        .last()
        .unwrap()
        .click();
    h.run_steps(3);
    let sheet = h.state().doc().unwrap().doc.sheet();
    assert_eq!(sheet.sankey.as_ref().unwrap().sources.get(&root), Some(&0));
    assert_eq!(sheet.canvas.objects.len(), 1);
    let CanvasObjectKind::Connector(c) = &sheet.canvas.objects[0].kind else {
        panic!()
    };
    assert_eq!(c.sankey.unwrap().weight, None);
    h.get_by_label("Assigned").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::Role::SpinButton, "New flow node — Weight")
        .set_value_accesskit(40.0);
    h.run_steps(3);
    let CanvasObjectKind::Connector(c) =
        &h.state().doc().unwrap().doc.sheet().canvas.objects[0].kind
    else {
        panic!()
    };
    assert_eq!(c.sankey.unwrap().weight, Some(40_000));
    assert_eq!(
        sankey::evaluate(h.state().doc().unwrap().doc.sheet())
            .unwrap()
            .outgoing[&root],
        0.0
    );
    h.get_by_role_and_label(egui::Role::Button, "Remove child link")
        .click();
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .is_empty()
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    let restored = h.state().doc().unwrap().doc.sheet();
    assert_eq!(restored.canvas.objects.len(), 1);
    let CanvasObjectKind::Connector(c) = &restored.canvas.objects[0].kind else {
        panic!()
    };
    assert_eq!(c.sankey.unwrap().weight, Some(40_000));
}

#[test]
fn sankey_parent_controls_add_edit_remove_and_undo() {
    let mut h = harness("sankey-blank", 0);
    h.get_by_label("Add flow node").click();
    h.run_steps(3);
    let (root, child) = {
        let doc = h.state_mut().doc_mut().unwrap();
        let root = doc.doc.sheet().floating[0];
        let child = doc.doc.sheet().floating[1];
        doc.view.select_only(child);
        (root, child)
    };
    h.run_steps(3);
    h.get_all_by_role_and_label(egui::Role::ComboBox, "Choose topic")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    h.get_all_by_label("New flow node").next().unwrap().click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::Button, "Add parent")
        .click();
    h.run_steps(3);
    let sheet = h.state().doc().unwrap().doc.sheet();
    assert_eq!(sheet.canvas.objects.len(), 1);
    let CanvasObjectKind::Connector(c) = &sheet.canvas.objects[0].kind else {
        panic!()
    };
    assert_eq!(c.from, Endpoint::Topic(root));
    assert_eq!(c.to, Endpoint::Topic(child));
    h.get_by_label("Assigned").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::Role::SpinButton, "New flow node — Weight")
        .set_value_accesskit(35.0);
    h.run_steps(3);
    let CanvasObjectKind::Connector(c) =
        &h.state().doc().unwrap().doc.sheet().canvas.objects[0].kind
    else {
        panic!()
    };
    assert_eq!(c.sankey.unwrap().weight, Some(35_000));
    h.get_by_role_and_label(egui::Role::Button, "Remove parent link")
        .click();
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .is_empty()
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    let sheet = h.state().doc().unwrap().doc.sheet();
    let CanvasObjectKind::Connector(c) = &sheet.canvas.objects[0].kind else {
        panic!()
    };
    assert_eq!(c.sankey.unwrap().weight, Some(35_000));
}

#[test]
fn sankey_inspector_can_construct_and_edit_a_flow_in_a_narrow_panel() {
    let mut h = harness_with_size("sankey-blank", 0, egui::vec2(280.0, 1600.0));
    h.get_by_label("Add flow node").click();
    h.run_steps(3);
    let (root, child) = {
        let doc = h.state_mut().doc_mut().unwrap();
        let root = doc.doc.sheet().floating[0];
        let child = doc.doc.sheet().floating[1];
        doc.view.select_only(root);
        (root, child)
    };
    h.run_steps(3);
    h.get_all_by_role_and_label(egui::Role::ComboBox, "Choose topic")
        .last()
        .unwrap()
        .click();
    h.run_steps(3);
    h.get_by_label("New flow node").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::Role::Button, "Add child")
        .click();
    h.run_steps(3);
    h.get_by_label("Assigned").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::Role::SpinButton, "New flow node — Weight")
        .set_value_accesskit(35.0);
    h.run_steps(3);

    let sheet = h.state().doc().unwrap().doc.sheet();
    assert_eq!(sheet.canvas.objects.len(), 1);
    assert_eq!(
        sankey::endpoints(match &sheet.canvas.objects[0].kind {
            CanvasObjectKind::Connector(connector) => connector,
            _ => panic!("expected Sankey connector"),
        })
        .unwrap(),
        (root, child)
    );
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.objects[0].kind else {
        panic!("expected Sankey connector")
    };
    assert_eq!(connector.sankey.unwrap().weight, Some(35_000));
    h.get_by_role_and_label(egui::Role::Button, "Remove child link")
        .click();
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .is_empty()
    );
}

#[test]
fn sankey_alluvial_inspector_distinguishes_repeated_names_by_stage() {
    let h = harness("sankey-learning", 1);
    h.get_by_role_and_label(egui::Role::Label, "Exploring · Stage 0 — Weight");
    h.get_by_role_and_label(egui::Role::Label, "Practising · Stage 2 — Weight");
}

#[test]
fn sankey_inspector_never_overwrites_a_newly_committed_topic_title_draft() {
    let mut h = harness("sankey-budget", 0);
    let doc = h.state_mut().doc_mut().unwrap();
    let id = doc.doc.sheet().floating[0];
    doc.view
        .begin_edit_with(doc.doc.sheet(), id, "Renamed supply");
    h.get_by_label("Weight color scale").click();
    h.run_steps(3);
    let sheet = h.state().doc().unwrap().doc.sheet();
    assert_eq!(sheet.topic(id).unwrap().title, "Renamed supply");
    assert!(!sheet.sankey.as_ref().unwrap().color_scale);
    h.get_by_label("Weight color scale").click();
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .sankey
            .as_ref()
            .unwrap()
            .color_scale
    );
}

#[test]
fn sankey_inspector_color_toggle_is_undoable() {
    let mut h = harness("sankey-budget", 0);
    h.get_by_label("Weight color scale").click();
    h.run_steps(3);
    assert!(
        !h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .sankey
            .as_ref()
            .unwrap()
            .color_scale
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .sankey
            .as_ref()
            .unwrap()
            .color_scale
    );
}

#[test]
fn sankey_alluvial_new_child_gets_adjacent_stage_and_stages_compact() {
    let mut h = harness("sankey-learning", 1);
    let (selected, fresh, lone) = {
        let doc = h.state_mut().doc_mut().unwrap();
        let selected = doc.view.primary().unwrap();
        let mut ids = (None, None);
        doc.doc
            .edit_sheet("cards", |sheet| {
                let card = || {
                    thinoda_core::board::Card::new(
                        thinoda_core::board::CardKind::Text,
                        thinoda_core::Vec2::new(155.0, 100.0),
                    )
                };
                let fresh = sheet
                    .add_card("Mastery", thinoda_core::Vec2::new(0.0, 900.0), card())
                    .unwrap();
                let lone = sheet
                    .add_card("Lone", thinoda_core::Vec2::new(0.0, 1200.0), card())
                    .unwrap();
                sheet.sankey.as_mut().unwrap().stages.insert(lone, 40);
                ids = (Some(fresh), Some(lone));
                Ok(())
            })
            .unwrap();
        (selected, ids.0.unwrap(), ids.1.unwrap())
    };
    h.run_steps(3);
    assert!(h.query_by_label("Stage 40 (1)").is_some());
    let stage = |h: &Harness<'static, ThinodaApp>, id| {
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .sankey
            .as_ref()
            .unwrap()
            .stages
            .get(&id)
            .copied()
    };
    let parent_stage = stage(&h, selected).unwrap();
    h.get_all_by_role_and_label(egui::Role::ComboBox, "Choose topic")
        .last()
        .unwrap()
        .click();
    h.run_steps(3);
    h.get_by_label("Mastery · Stage Unassigned").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::Role::Button, "Add child")
        .click();
    h.run_steps(3);
    assert_eq!(stage(&h, fresh), Some(parent_stage + 1));

    h.get_by_role_and_label(egui::Role::Button, "Compact stage numbers")
        .click();
    h.run_steps(3);
    let sheet = h.state().doc().unwrap().doc.sheet();
    let used: std::collections::BTreeSet<_> = sheet
        .sankey
        .as_ref()
        .unwrap()
        .stages
        .values()
        .copied()
        .collect();
    assert_eq!(used, (0..used.len() as u32).collect());
    assert_eq!(stage(&h, lone), Some(used.len() as u32 - 1));
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(stage(&h, lone), Some(40));
}
