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
    apply(&mut app, &ctx, editor);
    assert!(app.fim.editor.is_none());
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
    app.docs[0]
        .view
        .select_only(source.topic_for_record(movable).unwrap());
    app.canvas_action(&ctx, thinoda_canvas::Action::MoveUp);
    assert_eq!(
        thinoda_io::fim::export_document(app.docs[0].doc.sheet()).unwrap(),
        sample,
        "hierarchy proposals must not enter the live undo stack"
    );
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
    assert!(app.docs[0].doc.undo());
    assert_eq!(
        thinoda_io::fim::export_document(app.docs[0].doc.sheet()).unwrap(),
        sample
    );
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
