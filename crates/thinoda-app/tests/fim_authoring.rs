use egui::accesskit::Role;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    ThinodaApp,
    app::{Cmd, FormatTab},
};
use thinoda_core::{
    Document, Workbook,
    fim::{FimChange, FimKind},
};
use thinoda_io::Format;

fn source() -> Vec<u8> {
    br#"<?xml version="1.0" encoding="UTF-8"?>
<!-- original bytes must survive native save -->
<x:xdatenfelder.stammdatenschema.0102 xmlns:x="urn:xoev-de:fim:standard:xdatenfelder_3.0.0">
<x:stammdatenschema><x:identifikation><x:id>S00000000001</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Schema</x:name>
<x:struktur><x:anzahl>0:*</x:anzahl><x:enthaelt><x:datenfeld><x:identifikation><x:id>F00000000001</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Postal code</x:name><x:beschreibung>Keep all metadata</x:beschreibung></x:datenfeld></x:enthaelt></x:struktur>
</x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>"#.to_vec()
}

fn app() -> ThinodaApp {
    let book = thinoda_io::fim::import_document(&source()).unwrap();
    let mut app = ThinodaApp::headless();
    app.docs[0].doc = Document::new(book);
    app.format_tab = FormatTab::Fim;
    app.settings.show_format_panel = true;
    let root = app.docs[0].doc.sheet().root;
    app.docs[0].view.select_only(root);
    app
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(4);
    harness
}

#[test]
fn fourth_tab_shows_fim_identity_without_installing_or_validating() {
    let mut h = harness(app());
    h.get_by_role_and_label(Role::Tab, "FIM");
    h.get_by_label("FIM · XDF3");
    h.get_by_role_and_label(Role::Label, "S00000000001");
    assert!(h.query_by_label("All source data").is_none());
    h.get_by_role_and_label(Role::Button, "Edit FIM node");
    let header = h.get_by_role_and_label(Role::Label, "FIM · XDF3").rect();
    assert!(h.state().docs[0].view.canvas_rect().right() <= header.left());
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source()
    );
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::ToggleOutliner);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Label, "0:*");
    assert!(h.state().command_available(&Cmd::Export(Format::FimXdf3)));
}

#[test]
fn node_wizard_keeps_its_target_while_the_map_selection_changes() {
    let mut app = app();
    let root = app.docs[0].doc.sheet().root;
    let field = app.docs[0].doc.sheet().topics[&root].children[0];
    app.docs[0].view.select_only(field);
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditFimNode);
    h.run_steps(5);
    h.get_by_role_and_label(Role::TextInput, "Assigned FIM ID");
    h.get_by_role_and_label(Role::TextInput, "Name").focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_role_and_label(Role::TextInput, "Name")
        .type_text("Postal code draft");
    h.run_steps(3);
    assert!(h.state().any_dirty());
    h.state_mut().docs[0].view.select_only(root);
    h.run_steps(4);
    let name = h.get_by_role_and_label(Role::TextInput, "Name");
    assert_eq!(
        name.accesskit_node().value().as_deref(),
        Some("Postal code draft")
    );
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Assigned FIM ID")
            .accesskit_node()
            .value()
            .as_deref(),
        Some("F00000000001"),
    );
    assert!(h.state().command_available(&Cmd::EditFimNode));
    h.state_mut().exec(&ctx, Cmd::EditFimNode);
    h.run_steps(3);
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Assigned FIM ID")
            .accesskit_node()
            .value()
            .as_deref(),
        Some("F00000000001"),
    );
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Window, "Discard FIM draft?");
    h.get_by_role_and_label(Role::Button, "Keep editing")
        .click();
    h.run_steps(4);
    h.get_by_role_and_label(Role::TextInput, "Name");
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Discard draft")
        .click();
    h.run_steps(4);
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Assigned FIM ID")
            .is_none()
    );
    assert!(!h.state().any_dirty());
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source()
    );
}

#[test]
fn map_typing_opens_a_pinned_draft_without_changing_source_or_history() {
    let mut app = app();
    let field = app.docs[0].doc.sheet().topics[&app.docs[0].doc.sheet().root].children[0];
    app.docs[0].view.select_only(field);
    let mut h = harness(app);
    let state = h.state().docs[0].doc.state_id();
    h.ctx
        .memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
    h.event(egui::Event::Text("Typed draft".into()));
    h.run_steps(3);
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Name")
            .value()
            .as_deref(),
        Some("Typed draft")
    );
    h.ctx
        .memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
    h.event(egui::Event::Text("Must not replace the draft".into()));
    h.run_steps(3);
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Name")
            .value()
            .as_deref(),
        Some("Typed draft")
    );
    assert_eq!(h.state().docs[0].doc.state_id(), state);
    assert!(!h.state().docs[0].doc.can_undo());
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source()
    );
}

#[test]
fn map_add_handle_opens_creation_without_inserting_an_unvalidated_node() {
    let mut app = app();
    app.settings.show_topic_add_buttons = true;
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Button, "Add child topic")
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Window, "Create FIM node");
    assert!(
        h.get_by_role_and_label(Role::TextInput, "Assigned FIM ID")
            .value()
            .unwrap_or_default()
            .is_empty()
    );
    assert!(!h.state().docs[0].doc.can_undo());
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source()
    );
}

#[test]
fn releasing_a_fim_drag_outside_the_canvas_keeps_source_and_history() {
    let mut app = app();
    app.settings.show_topic_toolbar = false;
    let field = app.docs[0].doc.sheet().topics[&app.docs[0].doc.sheet().root].children[0];
    app.docs[0].view.select_only(field);
    let mut h = harness(app);
    let doc = &h.state().docs[0];
    let center = doc.view.selected_topic_rect(&doc.doc).unwrap().center();
    let state = doc.doc.state_id();
    h.hover_at(center);
    h.drag_at(center);
    h.run_steps(1);
    let outside = egui::pos2(1.0, 1.0);
    h.hover_at(outside);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: outside,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(h.state().docs[0].doc.state_id(), state);
    assert!(!h.state().docs[0].doc.can_undo());
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source()
    );
}

#[test]
fn map_double_click_opens_the_schema_editor_instead_of_inline_title_editing() {
    let mut app = app();
    let field = app.docs[0].doc.sheet().topics[&app.docs[0].doc.sheet().root].children[0];
    app.docs[0].view.select_only(field);
    let mut h = harness(app);
    let doc = &h.state().docs[0];
    let center = doc.view.selected_topic_rect(&doc.doc).unwrap().center();
    assert!(doc.view.canvas_rect().contains(center));
    h.hover_at(center);
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
    }
    h.run_steps(3);
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Assigned FIM ID")
            .value()
            .as_deref(),
        Some("F00000000001")
    );
    assert!(h.state().docs[0].view.editing_topic().is_none());
    assert!(!h.state().docs[0].doc.can_undo());
}

#[test]
fn node_wizard_cannot_apply_without_official_schemas() {
    let mut app = app();
    let root = app.docs[0].doc.sheet().root;
    let field = app.docs[0].doc.sheet().topics[&root].children[0];
    app.docs[0].view.select_only(field);
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditFimNode);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Window, "Download official schemas?");
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source()
    );
    // No download was authorized by opening or applying the form.
    assert!(h.query_by_label("Validation passed").is_none());
}

#[test]
fn structure_opens_the_same_node_wizard_without_the_fim_sidebar() {
    let mut app = app();
    app.settings.show_format_panel = false;
    app.mode = thinoda_app::app::CenterMode::Outliner;
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Button, "Edit FIM node")
        .click();
    h.run_steps(4);
    h.get_by_role_and_label(Role::TextInput, "Assigned FIM ID");
    h.get_by_role_and_label(Role::Button, "More schema properties…")
        .click();
    h.run_steps(4);
    h.get_by_label(
        "Advanced source editing. Changes are staged with the node and validated before Apply.",
    );
    assert!(h.query_by_label("All source data").is_none());
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source()
    );
}

fn accent_entry_rect(h: &Harness<'_, ThinodaApp>, identity: &str) -> Option<egui::Rect> {
    let label = h.get_by_role_and_label(Role::Label, identity).rect();
    h.output().shapes.iter().find_map(|shape| {
        if let egui::Shape::Rect(frame) = &shape.shape
            && frame.stroke.color == h.state().accent
            && frame.stroke.width == 2.0
            && frame.rect.contains_rect(label)
            && shape.clip_rect.contains_rect(frame.rect)
        {
            Some(frame.rect)
        } else {
            None
        }
    })
}

#[test]
fn map_selection_reveals_accent_outlined_fim_entries_in_both_themes() {
    use thinoda_platform::settings::ThemeMode;
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut app = app();
        app.settings.theme = theme;
        app.accent = egui::Color32::from_rgb(194, 66, 122);
        let root = app.docs[0].doc.sheet().root;
        let field = app.docs[0].doc.sheet().topics[&root].children[0];
        let state = app.docs[0].doc.state_id();
        let mut h = harness(app);
        assert!(accent_entry_rect(&h, "S00000000001").is_some());

        h.state_mut().docs[0].view.select_only(field);
        h.run_steps(5);
        assert!(accent_entry_rect(&h, "F00000000001").is_some());
        let field_position = {
            let doc = &h.state().docs[0];
            doc.view.selected_topic_rect(&doc.doc).unwrap().center()
        };
        h.state_mut().docs[0].view.select_only(root);
        h.run_steps(5);
        assert!(accent_entry_rect(&h, "S00000000001").is_some());
        h.event(egui::Event::PointerMoved(field_position));
        h.event(egui::Event::PointerButton {
            pos: field_position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
        h.event(egui::Event::PointerButton {
            pos: field_position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(5);
        assert_eq!(h.state().docs[0].view.primary(), Some(field));
        assert!(accent_entry_rect(&h, "F00000000001").is_some());

        h.state_mut().accent = egui::Color32::from_rgb(24, 152, 108);
        h.run_steps(4);
        assert!(accent_entry_rect(&h, "F00000000001").is_some());
        h.get_by_label("Document metadata").click();
        h.run_steps(4);
        let row = h
            .get_by_role_and_label(Role::Button, "F · Postal code")
            .rect();
        assert!(h.output().shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(frame)
                if frame.stroke.color == h.state().accent
                    && frame.stroke.width == 2.0
                    && frame.rect.expand(0.1).contains_rect(row))
        }));

        h.state_mut().format_tab = FormatTab::Style;
        h.run_steps(3);
        assert!(h.query_by_label("FIM · XDF3").is_none());
        h.state_mut().docs[0].view.select_only(root);
        h.run_steps(3);
        h.state_mut().format_tab = FormatTab::Fim;
        h.run_steps(5);
        assert!(accent_entry_rect(&h, "S00000000001").is_some());
        assert_eq!(h.state().docs[0].doc.state_id(), state);
        assert_eq!(
            thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
            source()
        );
    }
}

#[test]
fn selection_highlight_does_not_rebind_a_pending_fim_draft() {
    let mut app = app();
    let root = app.docs[0].doc.sheet().root;
    let field = app.docs[0].doc.sheet().topics[&root].children[0];
    app.docs[0].view.select_only(field);
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Button, "0:*").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::TextInput, "Multiplicity (Min:Max)")
        .focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_role_and_label(Role::TextInput, "Multiplicity (Min:Max)")
        .type_text("1:3");
    h.run_steps(3);
    h.state_mut().docs[0].view.select_only(root);
    h.run_steps(5);
    assert!(accent_entry_rect(&h, "S00000000001").is_some());
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Multiplicity (Min:Max)")
            .value()
            .as_deref(),
        Some("1:3")
    );
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source()
    );
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run_steps(4);
    let fim = h.state().docs[0].doc.sheet().fim.as_ref().unwrap();
    h.get_by_role_and_label(Role::Window, "Download official schemas?");
    assert_eq!(
        fim.topic_metadata(field).unwrap().cardinality.as_deref(),
        Some("0:*")
    );
    assert!(fim.topic_metadata(root).unwrap().cardinality.is_none());
}

#[test]
fn shared_definition_highlight_identifies_only_the_selected_occurrence() {
    let mut app = app();
    let doc = &mut app.docs[0];
    let root = doc.doc.sheet().root;
    let field = doc.doc.sheet().topics[&root].children[0];
    doc.doc
        .edit_sheet("Duplicate occurrence", |sheet| {
            sheet
                .apply_fim_change(FimChange::Duplicate { topic: field })
                .map_err(|error| thinoda_core::EditError::Invalid(error.to_string()))
        })
        .unwrap();
    let occurrences = doc.doc.sheet().topics[&root].children.clone();
    assert_eq!(occurrences.len(), 2);
    doc.view.select_only(field);
    let before = thinoda_io::fim::export_document(doc.doc.sheet()).unwrap();
    let mut h = harness(app);
    h.get_by_label("Document metadata").click();
    h.run_steps(4);
    for (selected, topic) in occurrences.into_iter().enumerate() {
        h.state_mut().docs[0].view.select_only(topic);
        h.run_steps(5);
        assert!(accent_entry_rect(&h, "F00000000001").is_some());
        let rows: Vec<_> = h
            .query_all_by_role_and_label(Role::Button, "F · Postal code")
            .map(|node| node.rect())
            .collect();
        assert_eq!(rows.len(), 2);
        for (index, row) in rows.into_iter().enumerate() {
            let highlighted = h.output().shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(frame)
                    if frame.stroke.color == h.state().accent
                        && frame.stroke.width == 2.0
                        && frame.rect.expand(0.1).contains_rect(row))
            });
            assert_eq!(highlighted, index == selected);
        }
    }
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        before
    );
}

#[test]
fn map_title_cardinality_and_undo_preserve_source_and_native_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    let doc = &mut app.docs[0];
    let fim = doc.doc.sheet().fim.as_ref().unwrap();
    let field = fim
        .records()
        .into_iter()
        .find(|record| record.kind == FimKind::Field)
        .unwrap();
    let topic = fim.topic_for_record(field.record).unwrap();
    doc.doc
        .edit_sheet("Cardinality", |sheet| {
            sheet
                .apply_fim_change(FimChange::SetCardinality {
                    topic,
                    value: "1:3".into(),
                })
                .map_err(|error| thinoda_core::EditError::Invalid(error.to_string()))
        })
        .unwrap();
    assert_eq!(
        doc.doc
            .sheet()
            .fim
            .as_ref()
            .unwrap()
            .topic_metadata(topic)
            .unwrap()
            .cardinality
            .as_deref(),
        Some("1:3")
    );
    let before = doc.doc.sheet().clone();
    assert!(
        doc.doc
            .edit_sheet("Invalid cardinality", |sheet| sheet
                .apply_fim_change(FimChange::SetCardinality {
                    topic,
                    value: "3:1".into(),
                })
                .map_err(|error| thinoda_core::EditError::Invalid(
                    error.to_string()
                )))
            .is_err()
    );
    assert_eq!(doc.doc.sheet(), &before);
    doc.doc.undo();
    assert_eq!(
        thinoda_io::fim::export_document(doc.doc.sheet()).unwrap(),
        source()
    );
    doc.view.begin_edit(doc.doc.sheet(), topic, false);
    doc.view.set_editing_text("ZIP code");
    let path = dir.path().join("fim.thinoda");
    doc.save_to(&path, false).unwrap();
    let reopened = thinoda_app::doc::OpenDoc::open(&path).unwrap();
    assert_eq!(reopened.doc.sheet().topic(topic).unwrap().title, "ZIP code");
    assert!(
        String::from_utf8(thinoda_io::fim::export_document(reopened.doc.sheet()).unwrap())
            .unwrap()
            .contains("Keep all metadata")
    );
    let masked: Workbook = reopened.doc.workbook().masked_copy();
    assert!(masked.active().fim.is_none());
}

#[test]
fn validated_xdf_export_is_disabled_for_ordinary_maps() {
    let app = ThinodaApp::headless();
    assert!(!app.command_available(&Cmd::Export(Format::FimXdf2)));
    assert!(!app.command_available(&Cmd::Export(Format::FimXdf3)));
}

#[test]
fn cardinality_draft_can_be_corrected_without_committing_unvalidated_changes() {
    let mut app = app();
    let field = app.docs[0]
        .doc
        .sheet()
        .topics
        .values()
        .find(|topic| topic.title == "Postal code")
        .unwrap()
        .id;
    app.docs[0].view.select_only(field);
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Button, "0:*").click();
    h.run_steps(3);
    for value in ["3:1", "1:3"] {
        h.get_by_role_and_label(Role::TextInput, "Multiplicity (Min:Max)")
            .focus();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(2);
        h.get_by_role_and_label(Role::TextInput, "Multiplicity (Min:Max)")
            .type_text(value);
        h.run_steps(3);
        assert!(h.state().any_dirty());
        assert_eq!(
            h.state().docs[0]
                .doc
                .sheet()
                .fim
                .as_ref()
                .unwrap()
                .topic_metadata(field)
                .unwrap()
                .cardinality
                .as_deref(),
            Some("0:*"),
        );
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, "Multiplicity (Min:Max)")
                .value()
                .as_deref(),
            Some(value)
        );
    }
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Discard draft")
        .click();
    h.run_steps(3);
    assert!(!h.state_mut().docs[0].doc.undo());
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source(),
    );
}

#[test]
fn separate_sheet_save_includes_pending_title_without_changing_original() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    let doc = &mut app.docs[0];
    let root = doc.doc.sheet().root;
    doc.view.begin_edit(doc.doc.sheet(), root, false);
    doc.view.set_editing_text("Pending schema name");
    let output = dir.path().join("sheet.thinoda");
    doc.save_sheet_to(&output, false).unwrap();
    let saved = thinoda_app::doc::OpenDoc::open(&output).unwrap();
    assert_eq!(saved.doc.sheet().root_topic().title, "Pending schema name");
    assert_eq!(doc.doc.sheet().root_topic().title, "Schema");
    assert!(doc.view.has_title_changes());
}

#[test]
fn associated_rule_is_reachable_from_structure_without_selecting_an_unrelated_topic() {
    let mut app = app();
    let xml = String::from_utf8(source()).unwrap().replace("</x:stammdatenschema>",
        "<x:regel><x:identifikation><x:id>R00000000001</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Review rule</x:name><x:skript>inert()</x:skript></x:regel></x:stammdatenschema>");
    app.docs[0].doc = Document::new(thinoda_io::fim::import_document(xml.as_bytes()).unwrap());
    let root = app.docs[0].doc.sheet().root;
    app.docs[0].view.select_only(root);
    app.mode = thinoda_app::app::CenterMode::Outliner;
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Label, "R");
    let sidebar_left = h
        .get_by_role_and_label(Role::Label, "FIM · XDF3")
        .rect()
        .left();
    h.query_all_by_role_and_label(Role::Button, "Review rule")
        .find(|node| node.rect().right() < sidebar_left)
        .expect("associated rule in Structure")
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Label, "R00000000001");
    assert!(accent_entry_rect(&h, "R00000000001").is_none());
    assert_eq!(h.state().docs[0].view.primary(), Some(root));
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        xml.as_bytes()
    );
}

#[test]
fn recovery_never_commits_unvalidated_node_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    let field = app.docs[0]
        .doc
        .sheet()
        .topics
        .values()
        .find(|topic| topic.title == "Postal code")
        .unwrap()
        .id;
    app.docs[0]
        .save_to(&dir.path().join("working.thinoda"), false)
        .unwrap();
    app.docs[0]
        .doc
        .edit_sheet("Cosmetic recovery change", |sheet| {
            let root = sheet.root;
            sheet.topics.get_mut(&root).unwrap().style.bold = Some(true);
            Ok(())
        })
        .unwrap();
    app.docs[0].view.select_only(field);
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Button, "0:*").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::TextInput, "Multiplicity (Min:Max)")
        .focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_role_and_label(Role::TextInput, "Multiplicity (Min:Max)")
        .type_text("1:3");
    h.run_steps(3);
    let paths = thinoda_platform::Paths::from_root(dir.path().join("recovery"));
    let recovery = thinoda_platform::recovery::RecoveryManager::start(&paths).unwrap();
    h.state_mut().docs[0].snapshot(&recovery);
    recovery.flush().unwrap();
    recovery.suspend().unwrap();
    let orphans = thinoda_platform::recovery::RecoveryManager::find_orphans(&paths);
    let recovered = orphans[0].docs[0].load().unwrap();
    assert_eq!(
        recovered
            .active()
            .fim
            .as_ref()
            .unwrap()
            .topic_metadata(field)
            .unwrap()
            .cardinality
            .as_deref(),
        Some("0:*")
    );
    assert!(
        h.state().any_dirty(),
        "live drafts still block accidental close"
    );
    drop(orphans);
    recovery.resume().unwrap();
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Discard draft")
        .click();
    h.run_steps(3);
    h.state_mut().docs[0]
        .save_to(&dir.path().join("working.thinoda"), false)
        .unwrap();
    h.state_mut().docs[0].snapshot(&recovery);
    recovery.flush().unwrap();
    recovery.suspend().unwrap();
    for orphan in thinoda_platform::recovery::RecoveryManager::find_orphans(&paths) {
        for saved in orphan.docs {
            assert_eq!(
                saved
                    .load()
                    .unwrap()
                    .active()
                    .fim
                    .as_ref()
                    .unwrap()
                    .topic_metadata(field)
                    .unwrap()
                    .cardinality
                    .as_deref(),
                Some("0:*")
            );
        }
    }
    assert!(!h.state().docs[0].is_dirty());
}

#[test]
fn unfinished_advanced_inputs_survive_reopening_and_block_apply() {
    let mut h = harness(app());
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditFimNode);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "More schema properties…")
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::TextInput, "XML element name")
        .focus();
    h.run_steps(2);
    h.get_by_role_and_label(Role::TextInput, "XML element name")
        .type_text("unfinished");
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Done").click();
    h.run_steps(3);
    assert!(h.state().any_dirty());
    h.get_by_role_and_label(Role::Button, "More schema properties…")
        .click();
    h.run_steps(3);
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "XML element name")
            .value()
            .as_deref(),
        Some("unfinished")
    );
    h.get_by_role_and_label(Role::Button, "Done").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run_steps(3);
    h.get_by_label("Stage or clear unfinished schema properties before applying.");
    assert!(
        h.query_by_role_and_label(Role::Window, "Download official schemas?")
            .is_none()
    );
    assert_eq!(
        thinoda_io::fim::export_document(h.state().docs[0].doc.sheet()).unwrap(),
        source()
    );
}

#[cfg(feature = "gpu-tests")]
#[test]
fn fim_sidebar_and_tree_render_with_realistic_source() {
    let output = std::env::var_os("THINODA_FIM_VISUAL_DIR").map(std::path::PathBuf::from);
    if let Some(directory) = &output {
        std::fs::create_dir_all(directory).unwrap();
    }
    for (name, width, height, outline, dark) in [
        ("fim-map-light", 1280.0, 900.0, false, false),
        ("fim-outline-dark", 1280.0, 900.0, true, true),
        ("fim-narrow-dark", 720.0, 900.0, false, true),
        ("fim-small-light", 360.0, 560.0, false, false),
    ] {
        let mut app = app();
        if let Some(sample) = std::env::var_os("THINODA_FIM_SAMPLE") {
            let bytes = std::fs::read(sample).unwrap();
            app.docs[0].doc = Document::new(thinoda_io::fim::import_document(&bytes).unwrap());
            let root = app.docs[0].doc.sheet().root;
            app.docs[0].view.select_only(root);
        }
        app.settings.theme = if dark {
            thinoda_platform::settings::ThemeMode::Dark
        } else {
            thinoda_platform::settings::ThemeMode::Light
        };
        if outline {
            app.mode = thinoda_app::app::CenterMode::Outliner;
        }
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, height))
            .with_context_setup(|ctx| {
                ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            })
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(5);
        let header = h.get_by_role_and_label(Role::Label, "FIM · XDF3").rect();
        assert!(
            header.min.x >= 0.0 && header.max.x <= width,
            "{name}: {header:?}"
        );
        assert!(
            h.output().shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                if text.galley.text() == "FIM · XDF3"
                    && shape.clip_rect.contains_rect(header))
            }),
            "{name}: revealing the initial selection must not scroll the FIM heading out of view"
        );
        if !outline {
            let canvas = h.state().docs[0].view.canvas_rect();
            assert!(
                canvas.right() <= header.left(),
                "{name}: canvas {canvas:?} overlaps FIM {header:?}"
            );
        }
        let sheet = h.state().docs[0].doc.sheet();
        let identity = sheet
            .fim
            .as_ref()
            .unwrap()
            .topic_metadata(sheet.root)
            .unwrap()
            .identity
            .unwrap();
        assert!(accent_entry_rect(&h, &identity).is_some());
        let image = h.render().expect("FIM workspace GPU rendering");
        if let Some(directory) = &output {
            image.save(directory.join(format!("{name}.png"))).unwrap();
        }
        let field = h.state().docs[0]
            .doc
            .sheet()
            .topics
            .values()
            .find(|topic| {
                h.state().docs[0]
                    .doc
                    .sheet()
                    .fim
                    .as_ref()
                    .unwrap()
                    .topic_metadata(topic.id)
                    .is_some_and(|metadata| metadata.kind == FimKind::Field)
            })
            .unwrap()
            .id;
        h.state_mut().docs[0].view.select_only(field);
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::EditFimNode);
        h.run_steps(5);
        for label in ["Assigned FIM ID", "Name"] {
            h.get_by_role_and_label(Role::TextInput, label)
                .scroll_to_me();
            h.run();
            let rect = h.get_by_role_and_label(Role::TextInput, label).rect();
            assert!(
                rect.left() >= 0.0 && rect.right() <= width && rect.bottom() <= height,
                "{name}: {label}: {rect:?}"
            );
            assert!(
                h.output().shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text)
                    if rect.contains(text.pos) && shape.clip_rect.contains(text.pos)
                        && shape.clip_rect.top() <= rect.top() + 6.0
                        && shape.clip_rect.bottom() >= rect.bottom() - 6.0)
                }),
                "{name}: {label} {rect:?} must be reachable inside the painted form viewport"
            );
        }
        let rect = h.get_by_role_and_label(Role::Button, "Apply").rect();
        assert!(
            rect.left() >= 0.0 && rect.right() <= width && rect.bottom() <= height,
            "{name}: Apply: {rect:?}"
        );
        let image = h.render().expect("FIM wizard GPU rendering");
        if let Some(directory) = &output {
            image
                .save(directory.join(format!("{name}-wizard.png")))
                .unwrap();
        }
    }
}
