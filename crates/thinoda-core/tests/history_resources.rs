use thinoda_core::{
    Color, Document, Vec2,
    board::{CanvasObject, InkStroke, InkTool},
    edit::EditError,
    resources::Resource,
};

const RESOURCE_BYTES: usize = 128 * 1024;

fn replace_resource(document: &mut Document, value: u8) {
    let resource = Resource::new(
        "attachment.bin",
        "application/octet-stream",
        vec![value; RESOURCE_BYTES],
    )
    .unwrap();
    document
        .edit_sheet("Replace attachment", |sheet| {
            let root = sheet.root;
            sheet.topic_mut(root).unwrap().attachment = None;
            sheet.resources.entries.clear();
            let reference = sheet
                .resources
                .insert(resource)
                .map_err(EditError::Invalid)?;
            sheet.topic_mut(root).unwrap().attachment =
                Some(thinoda_core::attachments::Attachment {
                    resource: reference,
                    name: "attachment.bin".into(),
                });
            Ok(())
        })
        .unwrap();
}

fn rename_root(document: &mut Document, title: &str) {
    let root = document.sheet().root;
    document
        .edit_sheet("Rename", |sheet| sheet.set_title(root, title))
        .unwrap();
}

fn add_ink(document: &mut Document, point_count: usize) {
    let points = (0..point_count)
        .map(|index| Vec2::new(index as f32 * 0.01, (index % 7) as f32))
        .collect();
    let mut stroke = InkStroke::new(InkTool::Pen, Color::rgb(10, 20, 30), 2.0, points);
    stroke.pressure = Some(vec![0.5; point_count].into());
    document
        .edit_sheet("Draw", |sheet| {
            sheet
                .add_canvas_object(CanvasObject::ink(stroke))
                .map(|_| ())
                .map_err(EditError::Invalid)
        })
        .unwrap();
}

#[test]
fn history_counts_distinct_resources_once_across_shared_snapshots() {
    let mut document = Document::default();
    replace_resource(&mut document, 1);
    let before_resource_is_retained = document.history_bytes();
    rename_root(&mut document, "First");
    let first = document.history_bytes();
    assert!(first >= before_resource_is_retained + RESOURCE_BYTES);

    rename_root(&mut document, "Second");
    let shared = document.history_bytes();
    assert!(
        shared - first < RESOURCE_BYTES,
        "shared resource bytes were counted more than once"
    );

    replace_resource(&mut document, 2);
    let before_second_is_retained = document.history_bytes();
    rename_root(&mut document, "Third");
    let distinct = document.history_bytes();
    assert!(distinct >= before_second_is_retained + RESOURCE_BYTES);

    document.duplicate_sheet(0).unwrap();
    let before_shared_sheet = document.history_bytes();
    document.duplicate_sheet(0).unwrap();
    assert!(
        document.history_bytes() - before_shared_sheet < RESOURCE_BYTES,
        "resource data shared across sheets was counted twice"
    );
}

#[test]
fn equal_resource_contents_in_distinct_allocations_are_counted_separately() {
    let mut first = Document::default();
    let mut second = Document::default();
    replace_resource(&mut first, 1);
    replace_resource(&mut second, 1);
    let first_bytes = first
        .sheet()
        .resources
        .entries
        .values()
        .next()
        .unwrap()
        .bytes();
    let second_bytes = second
        .sheet()
        .resources
        .entries
        .values()
        .next()
        .unwrap()
        .bytes();
    assert_eq!(first_bytes, second_bytes);
    assert_ne!(first_bytes.as_ptr(), second_bytes.as_ptr());

    let mut workbook = first.workbook().clone();
    workbook.sheets.push(second.sheet().clone());
    let mut document = Document::new(workbook);
    rename_root(&mut document, "First sheet");
    let before_second_snapshot = document.history_bytes();
    document.set_active(1);
    rename_root(&mut document, "Second sheet");
    assert!(document.history_bytes() >= before_second_snapshot + RESOURCE_BYTES);
}

#[test]
fn redo_retains_accounted_resources_and_respects_the_history_budget() {
    let mut document = Document::default();
    replace_resource(&mut document, 1);
    rename_root(&mut document, "First");
    replace_resource(&mut document, 2);
    rename_root(&mut document, "Second");

    while document.undo() {}
    assert!(document.can_redo());
    assert!(
        document.history_bytes() >= 2 * RESOURCE_BYTES,
        "redo still retains both resource buffers"
    );

    let budget = RESOURCE_BYTES + 32 * 1024;
    document.set_history_byte_budget(budget);
    assert!(document.history_bytes() <= budget);
    assert!(document.redo(), "the nearest redo must remain available");
    assert!(document.undo(), "the last redo must remain undoable");
    assert!(document.redo());
    rename_root(&mut document, "New edit");
    assert!(!document.can_redo());
    assert!(document.history_bytes() <= budget);
}

#[test]
fn large_note_contents_are_included_in_snapshot_estimates() {
    let mut document = Document::default();
    let root = document.sheet().root;
    document
        .edit_sheet("Add notes", |sheet| {
            sheet.topic_mut(root).unwrap().notes = Some("n".repeat(RESOURCE_BYTES).into());
            Ok(())
        })
        .unwrap();
    let before = document.history_bytes();
    rename_root(&mut document, "With notes");
    assert!(document.history_bytes() >= before + RESOURCE_BYTES);
}

#[test]
fn clearing_redo_releases_buffers_and_clone_keeps_its_own_accounting() {
    let mut document = Document::default();
    replace_resource(&mut document, 1);
    rename_root(&mut document, "First");
    replace_resource(&mut document, 2);
    rename_root(&mut document, "Second");
    assert!(document.undo());
    assert!(document.undo());
    let retained = document.history_bytes();
    let clone = document.clone();
    assert_eq!(clone.history_bytes(), retained);

    rename_root(&mut document, "Different future");
    assert!(!document.can_redo());
    assert!(document.history_bytes() + RESOURCE_BYTES / 2 < retained);
    assert_eq!(clone.history_bytes(), retained);

    document.replace(thinoda_core::Workbook::default());
    assert_eq!(document.history_bytes(), 0);
    assert!(!document.can_undo());
    assert!(!document.can_redo());
}

#[test]
fn reload_clears_old_history_and_accounts_for_new_snapshots() {
    let mut document = Document::default();
    replace_resource(&mut document, 1);
    rename_root(&mut document, "Saved");
    let encoded = serde_json::to_vec(document.workbook()).unwrap();
    replace_resource(&mut document, 2);
    assert!(document.history_bytes() >= RESOURCE_BYTES);

    let reloaded = serde_json::from_slice(&encoded).unwrap();
    document.replace(reloaded);
    assert_eq!(document.history_bytes(), 0);
    assert!(!document.can_undo());
    assert!(!document.can_redo());
    assert_eq!(document.sheet().root_topic().title, "Saved");
    assert!(document.sheet().resources.entries.values().any(|resource| {
        resource.bytes().len() == RESOURCE_BYTES && resource.bytes().iter().all(|byte| *byte == 1)
    }));

    rename_root(&mut document, "After reload");
    assert!(document.history_bytes() >= RESOURCE_BYTES);
    assert!(document.undo());
    assert_eq!(document.sheet().root_topic().title, "Saved");
    assert!(document.redo());
    assert_eq!(document.sheet().root_topic().title, "After reload");
}

#[test]
fn undoable_reload_accounts_for_workbook_metadata_and_shared_buffers() {
    const POINT_COUNT: usize = 8 * 1024;
    const INK_BYTES: usize =
        POINT_COUNT * (std::mem::size_of::<Vec2>() + std::mem::size_of::<f32>());

    let mut original = Document::default();
    replace_resource(&mut original, 1);
    add_ink(&mut original, POINT_COUNT);
    let mut workbook = original.workbook().clone();
    workbook.metadata.description = Some("m".repeat(RESOURCE_BYTES));
    let mut document = Document::new(workbook);
    rename_root(&mut document, "Local edit");
    let before_reload = document.history_bytes();

    let mut external = Document::default();
    replace_resource(&mut external, 2);
    let external = external.workbook().clone();
    document.reload(external.clone()).unwrap();

    let after_reload = document.history_bytes();
    let added = after_reload - before_reload;
    assert!(added >= RESOURCE_BYTES, "workbook metadata must be counted");
    assert!(
        added < RESOURCE_BYTES + INK_BYTES / 2,
        "the workbook snapshot must share ink, pressure and resource buffers"
    );
    assert!(!document.is_dirty());

    assert!(document.undo());
    assert!(document.is_dirty());
    assert_eq!(document.sheet().root_topic().title, "Local edit");
    assert!(
        document.history_bytes() >= 2 * RESOURCE_BYTES + INK_BYTES,
        "undo and redo must account for both distinct resources and shared ink"
    );
    assert!(document.redo());
    assert_eq!(document.workbook(), &external);
    assert!(!document.is_dirty());
    assert_eq!(document.history_bytes(), after_reload);
}

#[test]
fn preview_starts_without_history_and_keeps_the_source_budget() {
    let mut document = Document::default();
    rename_root(&mut document, "Original");
    document.set_history_byte_budget(1);

    let mut preview = document.preview();
    assert_eq!(preview.history_bytes(), 0);
    assert!(!preview.can_undo());
    rename_root(&mut preview, "First proposal");
    rename_root(&mut preview, "Second proposal");
    assert!(preview.undo());
    assert_eq!(preview.sheet().root_topic().title, "First proposal");
    assert!(!preview.can_undo(), "the tiny budget must trim older edits");
    assert_eq!(document.sheet().root_topic().title, "Original");
}

#[test]
fn tiny_budget_preserves_the_nearest_operation_on_each_side() {
    let mut document = Document::default();
    replace_resource(&mut document, 1);
    rename_root(&mut document, "First");
    replace_resource(&mut document, 2);
    rename_root(&mut document, "Second");
    assert!(document.undo());

    document.set_history_byte_budget(1);
    assert!(document.can_undo());
    assert!(document.can_redo());
    assert!(document.history_bytes() > 1);
    assert!(document.redo());
    assert_eq!(document.sheet().root_topic().title, "Second");
    assert!(document.undo());
    assert_eq!(document.sheet().root_topic().title, "First");
}
