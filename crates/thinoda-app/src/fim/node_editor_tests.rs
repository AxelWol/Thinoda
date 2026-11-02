use super::*;
use thinoda_core::Document;

#[test]
fn advanced_metadata_includes_the_incoming_edge_without_expanding_definitions() {
    let book = thinoda_io::fim::import_document(br#"<x:xdatenfelder.stammdatenschema.0102 xmlns:x="urn:xoev-de:fim:standard:xdatenfelder_3.0.0"><x:stammdatenschema><x:identifikation><x:id>S00000000001</x:id></x:identifikation><x:name>Schema</x:name><x:struktur><x:anzahl>0:*</x:anzahl><x:bezug link="https://example.invalid/legal">Legal reference</x:bezug><x:enthaelt><x:datenfeld><x:identifikation><x:id>F00000000001</x:id></x:identifikation><x:name>Field</x:name></x:datenfeld></x:enthaelt></x:struktur></x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>"#).unwrap();
    let sheet = book.active();
    let source = sheet.fim.as_ref().unwrap();
    let field = source
        .record_for_topic(sheet.topics[&sheet.root].children[0])
        .unwrap();
    let editor = AdvancedEditor::new(source, field);
    let edge = source.node_edge_fields(field);
    assert!(edge.iter().any(|property| property.name.ends_with("bezug")));
    assert!(edge.iter().all(|property| {
        editor
            .fields
            .iter()
            .any(|visible| visible.record == property.record)
    }));
    let multiplicity = edge
        .iter()
        .find(|property| property.name.ends_with("anzahl"))
        .unwrap();
    assert_eq!(editor.common[&multiplicity.record], "anzahl");
    assert!(
        !editor
            .fields
            .iter()
            .any(|property| property.value == "S00000000001")
    );
}

#[test]
fn repair_comparison_counts_duplicate_errors_and_distinguishes_paths() {
    assert_eq!(
        new_error_messages(
            [("missing", None)].into_iter(),
            [("missing", None), ("missing", None)].into_iter()
        ),
        ["missing"]
    );
    assert_eq!(
        new_error_messages(
            [("missing", Some("/old")), ("missing", None)].into_iter(),
            [("missing", Some("/new")), ("missing", None)].into_iter()
        ),
        ["/new: missing"]
    );
    assert!(
        new_error_messages(
            [("missing", None), ("missing", None)].into_iter(),
            [("missing", None)].into_iter()
        )
        .is_empty()
    );
}

fn start_editor(app: &mut ThinodaApp, ctx: &Context, record: RecordId) -> NodeEditor {
    app.begin_fim_node(ctx, Some(record), None);
    app.fim.editor.take().expect("node draft")
}

fn apply(app: &mut ThinodaApp, ctx: &Context, mut editor: NodeEditor) {
    app.start_fim_node_apply(ctx, &mut editor, None);
    assert!(
        editor.work.is_some(),
        "{}",
        editor.error.as_deref().unwrap_or("no worker")
    );
    app.fim.editor = Some(editor);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while app
        .fim
        .editor
        .as_ref()
        .is_some_and(|editor| editor.work.is_some())
    {
        assert!(std::time::Instant::now() < deadline, "node worker timeout");
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| app.fim_node_dialog(ui.ctx()),
        );
        output.textures_delta.clear();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn draft_app() -> (ThinodaApp, Context, RecordId) {
    let book = thinoda_io::fim::import_document(br#"<x:xdatenfelder.stammdatenschema.0102 xmlns:x="urn:xoev-de:fim:standard:xdatenfelder_3.0.0"><x:stammdatenschema><x:identifikation><x:id>S00000000001</x:id></x:identifikation><x:name>Schema</x:name><x:struktur><x:anzahl>0:*</x:anzahl><x:enthaelt><x:datenfeld><x:identifikation><x:id>F00000000001</x:id></x:identifikation><x:name>Field</x:name></x:datenfeld></x:enthaelt></x:struktur></x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>"#).unwrap();
    let mut app = ThinodaApp::headless();
    app.docs[0].doc = Document::new(book);
    let source = app.docs[0].doc.sheet().fim.as_ref().unwrap();
    let record = source
        .records()
        .into_iter()
        .find(|record| record.kind == FimKind::Field)
        .unwrap()
        .record;
    let topic = source.topic_for_record(record).unwrap();
    app.docs[0].view.select_only(topic);
    (app, Context::default(), record)
}

fn draw_node_dialog(app: &mut ThinodaApp, ctx: &Context) {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| app.fim_node_dialog(ui.ctx()),
    );
    output.textures_delta.clear();
}

fn board_shape(label: &str) -> thinoda_core::CanvasObject {
    let mut shape = thinoda_core::CanvasObject::shape(
        thinoda_core::ShapeKind::Rectangle,
        thinoda_core::Rect::from_min_size(
            thinoda_core::Vec2::new(100.0, 100.0),
            thinoda_core::Vec2::new(120.0, 60.0),
        ),
    );
    if let thinoda_core::CanvasObjectKind::Shape {
        label: shape_label, ..
    } = &mut shape.kind
    {
        *shape_label = label.into();
    }
    shape
}

#[test]
fn node_draft_rebases_onto_the_accepted_board_preview() {
    let (mut app, ctx, record) = draft_app();
    let mut editor = start_editor(&mut app, &ctx, record);
    editor.draft.name = "Field draft".into();
    let old_owner = editor.owner.clone();
    app.fim.editor = Some(editor);

    let mut preview = app.docs[0].doc.preview();
    let shape = preview
        .add_canvas_object(board_shape("Board content"))
        .unwrap();
    app.accept_fim_canvas_preview(&ctx, preview);
    draw_node_dialog(&mut app, &ctx);

    let editor = app.fim.editor.take().unwrap();
    assert_ne!(editor.owner, old_owner);
    assert_eq!(Some(&editor.owner), app.fim_owner().as_ref());
    assert_eq!(editor.draft.name, "Field draft");
    assert_eq!(
        editor.original.canvas.object(shape).unwrap().label(),
        "Board content"
    );

    // Exercise the same sheet candidate used by the validation worker without
    // pretending that this unit test provides official XSD validation.
    let mut candidate = editor.original.clone();
    candidate
        .apply_fim_change(FimChange::AuthorNode {
            target: editor.target.clone(),
            draft: Box::new(editor.draft.clone()),
            timestamp: "2026-01-02T03:04:05Z".into(),
        })
        .unwrap();
    assert_eq!(
        candidate
            .fim
            .as_ref()
            .unwrap()
            .node_draft(record)
            .unwrap()
            .name,
        "Field draft"
    );
    assert_eq!(
        candidate.canvas.object(shape).unwrap().label(),
        "Board content"
    );
    app.docs[0]
        .doc
        .edit_sheet("Install candidate", |sheet| {
            *sheet = candidate;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .label(),
        "Board content"
    );
    assert!(app.docs[0].doc.undo());
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .label(),
        "Board content",
        "undoing the FIM change must not undo the earlier board edit"
    );
}

#[test]
fn hierarchy_proposal_blocks_board_edits_until_it_is_resolved() {
    let book = thinoda_io::fim::import_document(br#"<x:xdatenfelder.stammdatenschema.0102 xmlns:x="urn:xoev-de:fim:standard:xdatenfelder_3.0.0"><x:stammdatenschema><x:identifikation><x:id>S00000000001</x:id></x:identifikation><x:name>Schema</x:name><x:struktur><x:anzahl>0:*</x:anzahl><x:enthaelt><x:datenfeld><x:identifikation><x:id>F00000000001</x:id></x:identifikation><x:name>First field</x:name></x:datenfeld></x:enthaelt></x:struktur><x:struktur><x:anzahl>0:*</x:anzahl><x:enthaelt><x:datenfeld><x:identifikation><x:id>F00000000002</x:id></x:identifikation><x:name>Second field</x:name></x:datenfeld></x:enthaelt></x:struktur></x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>"#).unwrap();
    let mut app = ThinodaApp::headless();
    app.docs[0].doc = Document::new(book);
    let ctx = Context::default();
    let root = app.docs[0].doc.sheet().root;
    let original_order = app.docs[0].doc.sheet().topics[&root].children.clone();
    let shape = app.docs[0]
        .doc
        .add_canvas_object(board_shape("Existing board content"))
        .unwrap();
    app.docs[0].view.select_only(original_order[1]);
    app.canvas_action(&ctx, thinoda_canvas::Action::MoveUp);

    let editor = app.fim.editor.as_ref().expect("hierarchy review");
    let proposal = editor.proposed_sheet.as_ref().expect("staged hierarchy");
    assert_ne!(proposal.topics[&root].children, original_order);
    assert_eq!(
        proposal.canvas.object(shape).unwrap().label(),
        "Existing board content"
    );
    assert_eq!(Some(&editor.owner), app.fim_owner().as_ref());
    assert!(app.any_modal_open());

    app.docs[0].view.select_board_object(shape);
    let before = app.docs[0].doc.state_id();
    let rect = app.docs[0].doc.sheet().canvas.object(shape).unwrap().rect;
    let command = crate::app::Cmd::BoardAction(thinoda_canvas::BoardAction::Move(
        thinoda_core::Vec2::new(30.0, 10.0),
    ));
    assert!(!app.command_available(&command));
    app.exec(&ctx, command);
    draw_node_dialog(&mut app, &ctx);
    let editor = app.fim.editor.take().unwrap();
    assert_eq!(app.docs[0].doc.state_id(), before);
    assert_eq!(Some(&editor.owner), app.fim_owner().as_ref());
    assert_eq!(
        app.docs[0].doc.sheet().canvas.object(shape).unwrap().rect,
        rect
    );

    // Install the exact staged candidate to exercise the edit/undo boundary;
    // this unit test does not claim official XSD validation.
    let candidate = editor.proposed_sheet.unwrap();
    let proposed_order = candidate.topics[&root].children.clone();
    app.docs[0]
        .doc
        .edit_sheet("Install hierarchy candidate", |sheet| {
            *sheet = candidate;
            Ok(())
        })
        .unwrap();
    let command = crate::app::Cmd::BoardAction(thinoda_canvas::BoardAction::Move(
        thinoda_core::Vec2::new(30.0, 10.0),
    ));
    assert!(app.command_available(&command));
    app.exec(&ctx, command);
    let sheet = app.docs[0].doc.sheet();
    assert_eq!(sheet.topics[&root].children, proposed_order);
    assert_ne!(sheet.canvas.object(shape).unwrap().rect, rect);
    assert_eq!(
        sheet.canvas.object(shape).unwrap().label(),
        "Existing board content"
    );
    assert!(app.docs[0].doc.undo());
    assert_eq!(
        app.docs[0].doc.sheet().topics[&root].children,
        proposed_order
    );
    assert!(app.docs[0].doc.undo());
    assert_eq!(
        app.docs[0].doc.sheet().topics[&root].children,
        original_order
    );
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .label(),
        "Existing board content"
    );
}

#[test]
fn hierarchy_review_waits_for_a_pending_image_request() {
    let (mut app, ctx, record) = draft_app();
    let editor = start_editor(&mut app, &ctx, record);
    let mut draft = editor.draft;
    draft.name = "Proposed field".into();
    let target = editor.target;
    app.image_request = crate::images::ImageRequest::capture(&app.docs[0], 42);
    assert!(app.image_request.is_some());

    let mut preview = app.docs[0].doc.preview();
    preview
        .edit_sheet("Proposed source change", |sheet| {
            sheet
                .apply_fim_change(FimChange::AuthorNode {
                    target,
                    draft: Box::new(draft),
                    timestamp: "2026-01-02T03:04:05Z".into(),
                })
                .map_err(|error| thinoda_core::EditError::Invalid(error.to_string()))
        })
        .unwrap();
    let state = app.docs[0].doc.state_id();
    app.accept_fim_canvas_preview(&ctx, preview.preview());
    assert!(app.fim.editor.is_none());
    assert!(app.image_request.is_some());
    assert_eq!(app.docs[0].doc.state_id(), state);

    app.image_request = None;
    app.accept_fim_canvas_preview(&ctx, preview);
    assert!(app.fim_hierarchy_review_open());
    assert_eq!(app.docs[0].doc.state_id(), state);
}

#[test]
fn failed_node_validation_rebases_after_a_newer_board_edit() {
    let (mut app, ctx, record) = draft_app();
    let mut editor = start_editor(&mut app, &ctx, record);
    editor.draft.name = "Field draft".into();
    let old_owner = editor.owner.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    editor.work = Some(NodeWork { receiver });
    app.fim.editor = Some(editor);
    let shape = app.docs[0]
        .doc
        .add_canvas_object(board_shape("Newer board content"))
        .unwrap();

    draw_node_dialog(&mut app, &ctx);
    let editor = app.fim.editor.as_ref().unwrap();
    assert!(editor.work.is_some());
    assert_eq!(editor.owner, old_owner);
    assert!(editor.original.canvas.object(shape).is_none());

    sender.send(Err("validation failed".into())).unwrap();
    draw_node_dialog(&mut app, &ctx);
    let editor = app.fim.editor.as_ref().unwrap();
    assert!(editor.work.is_none());
    assert_eq!(Some(&editor.owner), app.fim_owner().as_ref());
    assert_eq!(editor.draft.name, "Field draft");
    assert_eq!(
        editor.original.canvas.object(shape).unwrap().label(),
        "Newer board content"
    );
    assert_eq!(editor.error.as_deref(), Some("validation failed"));
}

#[test]
fn node_validation_waits_for_pending_board_input() {
    let (mut app, ctx, record) = draft_app();
    let shape = app.docs[0]
        .doc
        .add_canvas_object(board_shape("Label"))
        .unwrap();
    let mut editor = start_editor(&mut app, &ctx, record);
    editor.draft.name = "Field draft".into();
    let (sender, receiver) = std::sync::mpsc::channel();
    editor.work = Some(NodeWork { receiver });
    app.fim.editor = Some(editor);
    sender.send(Err("validation failed".into())).unwrap();

    app.docs[0]
        .view
        .set_board_tool(thinoda_canvas::BoardTool::Connector);
    draw_node_dialog(&mut app, &ctx);
    assert!(app.fim.editor.as_ref().unwrap().work.is_some());
    app.docs[0]
        .view
        .set_board_tool(thinoda_canvas::BoardTool::Select);
    {
        let doc = &mut app.docs[0];
        assert!(doc.view.begin_board_label(&doc.doc, shape));
    }
    draw_node_dialog(&mut app, &ctx);
    assert!(app.fim.editor.as_ref().unwrap().work.is_some());
    {
        let doc = &mut app.docs[0];
        doc.view.try_commit_board_label(&mut doc.doc).unwrap();
    }
    app.image_request = crate::images::ImageRequest::capture(&app.docs[0], 42);
    draw_node_dialog(&mut app, &ctx);
    assert!(app.fim.editor.as_ref().unwrap().work.is_some());
    app.image_request = None;

    let pos = egui::pos2(20.0, 20.0);
    let pointer = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let input = |event| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 900.0),
        )),
        events: vec![event],
        ..Default::default()
    };
    let mut output = ctx.run_ui(input(pointer(true)), |ui| app.fim_node_dialog(ui.ctx()));
    output.textures_delta.clear();
    assert!(app.fim.editor.as_ref().unwrap().work.is_some());

    let mut output = ctx.run_ui(input(pointer(false)), |ui| app.fim_node_dialog(ui.ctx()));
    output.textures_delta.clear();
    let editor = app.fim.editor.as_ref().unwrap();
    assert!(editor.work.is_none());
    assert_eq!(editor.error.as_deref(), Some("validation failed"));
    assert_eq!(editor.draft.name, "Field draft");
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .label(),
        "Label"
    );
}

#[test]
fn changed_source_and_reordered_duplicate_sheets_are_not_rebased() {
    let (mut app, ctx, record) = draft_app();
    let mut peer = app.docs[0].doc.sheet().clone();
    peer.add_canvas_object(board_shape("Other sheet")).unwrap();
    app.docs[0]
        .doc
        .edit_book("Duplicate sheet", |book| {
            book.sheets.push(peer);
            Ok(())
        })
        .unwrap();
    let mut editor = start_editor(&mut app, &ctx, record);
    editor.draft.name = "Field draft".into();
    app.fim.editor = Some(editor);
    app.docs[0]
        .doc
        .add_canvas_object(board_shape("Original sheet"))
        .unwrap();
    draw_node_dialog(&mut app, &ctx);
    assert_eq!(
        Some(&app.fim.editor.as_ref().unwrap().owner),
        app.fim_owner().as_ref(),
        "an unchanged duplicate must not block a safe board rebase"
    );
    let owner = app.fim.editor.as_ref().unwrap().owner.clone();

    app.docs[0].doc.move_sheet(0, 1).unwrap();
    app.docs[0].doc.set_active(0);
    draw_node_dialog(&mut app, &ctx);
    let editor = app.fim.editor.as_ref().unwrap();
    assert_eq!(editor.owner, owner);
    assert_ne!(Some(&editor.owner), app.fim_owner().as_ref());
    assert_eq!(editor.draft.name, "Field draft");
    assert_eq!(editor.original.canvas.objects[0].label(), "Original sheet");

    let (mut app, ctx, record) = draft_app();
    let mut editor = start_editor(&mut app, &ctx, record);
    editor.draft.name = "Field draft".into();
    let owner = editor.owner.clone();
    let mut other_draft = editor.baseline.clone();
    other_draft.name = "Changed source".into();
    let target = editor.target.clone();
    app.fim.editor = Some(editor);
    app.docs[0]
        .doc
        .edit_sheet("Change source", |sheet| {
            sheet
                .apply_fim_change(FimChange::AuthorNode {
                    target,
                    draft: Box::new(other_draft),
                    timestamp: "2026-01-02T03:04:05Z".into(),
                })
                .map_err(|error| thinoda_core::EditError::Invalid(error.to_string()))
        })
        .unwrap();
    draw_node_dialog(&mut app, &ctx);
    let editor = app.fim.editor.as_ref().unwrap();
    assert_eq!(editor.owner, owner);
    assert_ne!(Some(&editor.owner), app.fim_owner().as_ref());
    assert_eq!(editor.draft.name, "Field draft");
}

#[test]
#[ignore = "requires THINODA_FIM_REFERENCE_DIR and THINODA_FIM_SAMPLE"]
fn official_xdf3_worker_advances_dates_preserves_noops_and_rolls_back_invalid_edits() {
    let references = PathBuf::from(std::env::var_os("THINODA_FIM_REFERENCE_DIR").unwrap());
    let sample = std::fs::read(std::env::var_os("THINODA_FIM_SAMPLE").unwrap()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(temp.path());
    let ctx = Context::default();
    let mut app = ThinodaApp::headless();
    app.docs[0].doc = Document::new(thinoda_io::fim::import_document(&sample).unwrap());
    app.fim.schemas = FimSchemas::new(Some(&paths));
    assert!(app.fim.schemas.install_paths(
        Dialect::Xdf3,
        references.join("xdf3-official-schemas.zip"),
        Some(references.join("xdf3-official-examples.zip")),
    ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while app.fim.schemas.busy() {
        assert!(std::time::Instant::now() < deadline);
        app.fim.schemas.poll(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(app.fim.schemas.status(Dialect::Xdf3), &SchemaStatus::Ready);
    let source = app.docs[0].doc.sheet().fim.as_ref().unwrap();
    let record = source
        .records()
        .into_iter()
        .find(|record| record.kind == FimKind::Field && record.usages == 1)
        .unwrap()
        .record;
    let parent = source.node_parent(record).unwrap();
    let topic = source.topic_for_record(record).unwrap();
    app.docs[0].view.select_only(topic);
    let original_state = app.docs[0].doc.state_id();
    let original_date = app.docs[0]
        .doc
        .sheet()
        .fim
        .as_ref()
        .unwrap()
        .node_draft(record)
        .unwrap()
        .last_changed;

    let editor = start_editor(&mut app, &ctx, record);
    apply(&mut app, &ctx, editor);
    assert!(app.fim.editor.is_none());
    assert_eq!(app.docs[0].doc.state_id(), original_state);
    assert_eq!(
        thinoda_io::fim::export_document(app.docs[0].doc.sheet()).unwrap(),
        sample
    );

    let mut editor = start_editor(&mut app, &ctx, record);
    let edge = editor
        .source
        .node_edge_fields(record)
        .into_iter()
        .find(|property| property.name.ends_with("anzahl"))
        .unwrap();
    editor.changes.push(FimChange::SetText {
        record: edge.record,
        value: edge.value,
    });
    apply(&mut app, &ctx, editor);
    assert!(app.fim.editor.is_none());
    assert_eq!(app.docs[0].doc.state_id(), original_state);
    assert_eq!(
        thinoda_io::fim::export_document(app.docs[0].doc.sheet()).unwrap(),
        sample
    );

    let mut editor = start_editor(&mut app, &ctx, record);
    editor.draft.name.push_str(" reviewed");
    let expected_name = editor.draft.name.clone();
    app.fim.editor = Some(editor);
    let mut preview = app.docs[0].doc.preview();
    let shape = preview
        .add_canvas_object(board_shape("Validated board content"))
        .unwrap();
    app.accept_fim_canvas_preview(&ctx, preview);
    draw_node_dialog(&mut app, &ctx);
    let mut editor = app.fim.editor.take().unwrap();
    app.start_fim_node_apply(&ctx, &mut editor, None);
    assert!(editor.work.is_some());
    app.fim.editor = Some(editor);
    let later_shape = app.docs[0]
        .doc
        .add_canvas_object(board_shape("Later board content"))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while app
        .fim
        .editor
        .as_ref()
        .is_some_and(|editor| editor.work.is_some())
    {
        assert!(std::time::Instant::now() < deadline, "node worker timeout");
        draw_node_dialog(&mut app, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let editor = app.fim.editor.take().expect("stale worker retains draft");
    assert_eq!(editor.error.as_deref(), Some(t("Validation is stale")));
    assert_eq!(editor.draft.name, expected_name);
    assert!(app.docs[0].doc.sheet().canvas.object(later_shape).is_some());
    apply(&mut app, &ctx, editor);
    assert!(app.fim.editor.is_none());
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .fim
            .as_ref()
            .unwrap()
            .node_draft(record)
            .unwrap()
            .name,
        expected_name
    );
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .label(),
        "Validated board content"
    );
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(later_shape)
            .unwrap()
            .label(),
        "Later board content"
    );
    let date = app.docs[0]
        .doc
        .sheet()
        .fim
        .as_ref()
        .unwrap()
        .node_draft(record)
        .unwrap()
        .last_changed;
    assert_ne!(date, original_date);
    assert!(app.docs[0].doc.undo());
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .label(),
        "Validated board content"
    );
    assert!(app.docs[0].doc.undo());
    assert!(app.docs[0].doc.sheet().canvas.object(later_shape).is_none());
    assert!(app.docs[0].doc.undo());
    assert_eq!(app.docs[0].doc.state_id(), original_state);
    assert_eq!(
        thinoda_io::fim::export_document(app.docs[0].doc.sheet()).unwrap(),
        sample
    );

    let parent_date = app.docs[0]
        .doc
        .sheet()
        .fim
        .as_ref()
        .unwrap()
        .node_draft(parent)
        .unwrap()
        .last_changed;
    let mut editor = start_editor(&mut app, &ctx, record);
    editor.draft.cardinality = Some("1:3".into());
    apply(&mut app, &ctx, editor);
    assert!(app.fim.editor.is_none());
    let source = app.docs[0].doc.sheet().fim.as_ref().unwrap();
    assert_eq!(
        source.node_draft(record).unwrap().last_changed,
        original_date
    );
    assert_ne!(source.node_draft(parent).unwrap().last_changed, parent_date);
    assert_eq!(
        source.node_draft(record).unwrap().cardinality.as_deref(),
        Some("1:3")
    );
    assert!(app.docs[0].doc.undo());

    let mut editor = start_editor(&mut app, &ctx, record);
    let edge = editor
        .source
        .node_edge_fields(record)
        .into_iter()
        .find(|property| property.name.ends_with("anzahl"))
        .unwrap();
    editor.changes.push(FimChange::SetText {
        record: edge.record,
        value: "1:3".into(),
    });
    apply(&mut app, &ctx, editor);
    assert!(app.fim.editor.is_none());
    let source = app.docs[0].doc.sheet().fim.as_ref().unwrap();
    assert_eq!(
        source.node_draft(record).unwrap().last_changed,
        original_date
    );
    assert_ne!(source.node_draft(parent).unwrap().last_changed, parent_date);
    assert_eq!(
        source.node_draft(record).unwrap().cardinality.as_deref(),
        Some("1:3")
    );
    assert!(app.docs[0].doc.undo());

    let source = app.docs[0].doc.sheet().fim.as_ref().unwrap().clone();
    let movable = source
        .records()
        .into_iter()
        .find(|item| {
            item.kind == FimKind::Field
                && item.usages == 1
                && source.node_parent(item.record).is_some_and(|parent| {
                    source
                        .node_children(parent)
                        .iter()
                        .position(|child| *child == item.record)
                        .is_some_and(|index| index > 0)
                })
        })
        .unwrap()
        .record;
    let move_parent = source.node_parent(movable).unwrap();
    let move_date = source.node_draft(move_parent).unwrap().last_changed;
    let hierarchy_shape = app.docs[0]
        .doc
        .add_canvas_object(board_shape("Hierarchy board content"))
        .unwrap();
    let original_rect = app.docs[0]
        .doc
        .sheet()
        .canvas
        .object(hierarchy_shape)
        .unwrap()
        .rect;
    app.docs[0]
        .view
        .select_only(source.topic_for_record(movable).unwrap());
    app.canvas_action(&ctx, thinoda_canvas::Action::MoveUp);
    assert_eq!(
        thinoda_io::fim::export_document(app.docs[0].doc.sheet()).unwrap(),
        sample,
        "hierarchy proposals must not enter the live undo stack"
    );
    assert!(app.fim_hierarchy_review_open());
    app.docs[0].view.select_board_object(hierarchy_shape);
    let board_move = crate::app::Cmd::BoardAction(thinoda_canvas::BoardAction::Move(
        thinoda_core::Vec2::new(30.0, 10.0),
    ));
    let state_before_blocked_move = app.docs[0].doc.state_id();
    assert!(!app.command_available(&board_move));
    app.exec(&ctx, board_move.clone());
    assert_eq!(app.docs[0].doc.state_id(), state_before_blocked_move);
    let editor = app.fim.editor.take().expect("hierarchy review");
    assert!(editor.proposed_sheet.is_some());
    apply(&mut app, &ctx, editor);
    assert!(app.fim.editor.is_none());
    let after = app.docs[0].doc.sheet().fim.as_ref().unwrap();
    assert_ne!(
        after.node_children(move_parent),
        source.node_children(move_parent)
    );
    assert_ne!(
        after.node_draft(move_parent).unwrap().last_changed,
        move_date
    );
    assert_eq!(
        after.node_draft(movable).unwrap().last_changed,
        source.node_draft(movable).unwrap().last_changed
    );
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(hierarchy_shape)
            .unwrap()
            .label(),
        "Hierarchy board content"
    );
    assert!(app.command_available(&board_move));
    app.exec(&ctx, board_move);
    assert_ne!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(hierarchy_shape)
            .unwrap()
            .rect,
        original_rect
    );
    assert!(app.docs[0].doc.undo());
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .canvas
            .object(hierarchy_shape)
            .unwrap()
            .rect,
        original_rect
    );
    assert!(app.docs[0].doc.undo());
    assert_eq!(
        thinoda_io::fim::export_document(app.docs[0].doc.sheet()).unwrap(),
        sample
    );
    assert!(app.docs[0].doc.undo());
    assert_eq!(app.docs[0].doc.state_id(), original_state);
    app.docs[0].view.select_only(topic);

    let mut editor = start_editor(&mut app, &ctx, record);
    editor.draft.name.push_str(" explicit date");
    editor.draft.last_changed = Some("2025-02-03T04:05:06+01:00".into());
    apply(&mut app, &ctx, editor);
    assert!(app.fim.editor.is_none());
    assert_eq!(
        app.docs[0]
            .doc
            .sheet()
            .fim
            .as_ref()
            .unwrap()
            .node_draft(record)
            .unwrap()
            .last_changed
            .as_deref(),
        Some("2025-02-03T04:05:06+01:00")
    );
    assert!(app.docs[0].doc.undo());

    let mut editor = start_editor(&mut app, &ctx, record);
    editor.draft.name.push_str(" must not commit");
    editor.draft.precision.max_length = Some("0".into());
    apply(&mut app, &ctx, editor);
    let rejected = app.fim.editor.as_ref().unwrap();
    assert!(rejected.error.is_some());
    assert_eq!(
        rejected.draft.last_changed, original_date,
        "automatic timestamps must not contaminate rejected drafts"
    );
    assert_eq!(
        thinoda_io::fim::export_document(app.docs[0].doc.sheet()).unwrap(),
        sample
    );
    assert_eq!(app.docs[0].doc.state_id(), original_state);
}
