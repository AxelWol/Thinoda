use thinoda_core::{Document, edit::EditError, resources::Resource};

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
