use thinoda_core::{
    Document, Sheet, Workbook,
    clipboard::{copy_topics, decode_sheet, encode_sheet},
    resources::{MAX_RESOURCE_BYTES, MAX_RESOURCES, Resource, ResourceStore},
};

fn embedded_sheet() -> (Sheet, thinoda_core::TopicId, String) {
    let mut sheet = Sheet::new("Evidence", "Root");
    let child = sheet.add_child(sheet.root, None, "Evidence");
    let reference = sheet
        .resources
        .insert(Resource::new("evidence.png", "image/png", vec![1, 2, 3]).unwrap())
        .unwrap();
    sheet.topic_mut(child).unwrap().image = Some(reference.clone());
    (sheet, child, reference)
}

#[test]
fn clipboard_branch_copy_merge_and_undo_preserve_only_owned_bytes() {
    let (mut sheet, child, reference) = embedded_sheet();
    sheet
        .resources
        .insert(Resource::new("unrelated", "text/plain", vec![4]).unwrap())
        .unwrap();
    let encoded = encode_sheet(&sheet).unwrap();
    assert!(encoded.starts_with("THINODA-SHEET-2\n"));
    assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
    assert!(decode_sheet(&encoded.replacen("THINODA-SHEET-2", "THINODA-SHEET-1", 1)).is_err());
    assert_eq!(decode_sheet(&encoded.replace('\n', "\r\n")).unwrap(), sheet);
    let branch = sheet.copy_branch_as_sheet(child).unwrap();
    assert_eq!(branch.resources.entries.len(), 1);
    assert_eq!(
        branch.resources.get(&reference).unwrap().bytes(),
        &[1, 2, 3]
    );
    let clipboard = copy_topics(&sheet, &[child]).unwrap();
    assert_eq!(clipboard.subtrees[0].resources.entries.len(), 1);

    let mut doc = Document::new(Workbook::new(Sheet::new("Destination", "Root")));
    let before = doc.workbook().clone();
    let roots = doc
        .edit_sheet("Paste", |destination| {
            destination.paste(destination.root, &clipboard.subtrees)
        })
        .unwrap();
    assert_eq!(
        doc.sheet().topic(roots[0]).unwrap().image.as_deref(),
        Some(reference.as_str())
    );
    assert_eq!(
        doc.sheet().resources.get(&reference).unwrap().bytes(),
        &[1, 2, 3]
    );
    let pasted = doc.workbook().clone();
    assert!(doc.undo());
    assert_eq!(doc.workbook(), &before);
    assert!(doc.redo());
    assert_eq!(doc.workbook(), &pasted);
    doc.edit_sheet("Repeat", |destination| {
        destination.paste(destination.root, &clipboard.subtrees)
    })
    .unwrap();
    assert_eq!(doc.sheet().resources.entries.len(), 1);
    doc.merge_sheets(vec![branch]).unwrap();
    doc.workbook().validate().unwrap();
    assert_eq!(doc.sheet().resources.entries.len(), 1);
    let masked = doc.workbook().masked_copy();
    assert!(masked.sheets.iter().all(|sheet| sheet.resources.is_empty()));
    masked.validate().unwrap();
}

#[test]
fn missing_corrupt_and_excessive_pastes_leave_document_and_history_unchanged() {
    let (source, child, _) = embedded_sheet();
    let mut sub = source.copy_subtree(child).unwrap();
    let mut destination = Sheet::new("Destination", "Root");
    sub.resources = ResourceStore::default();
    let before = destination.clone();
    assert!(destination.paste(destination.root, &[sub]).is_err());
    assert_eq!(destination, before);

    for index in 0..MAX_RESOURCES {
        destination
            .resources
            .insert(Resource::new("entry", "text/plain", index.to_le_bytes().to_vec()).unwrap())
            .unwrap();
    }
    let mut doc = Document::new(Workbook::new(destination));
    let before = doc.workbook().clone();
    assert!(
        doc.edit_sheet("Overflow paste", |sheet| {
            sheet.paste(sheet.root, &[source.copy_subtree(child).unwrap()])
        })
        .is_err()
    );
    assert_eq!(doc.workbook(), &before);
    assert!(!doc.is_dirty());
    assert!(!doc.can_undo());

    assert!(doc.add_sheet(source.clone()).is_err());
    assert!(doc.merge_sheets(vec![source]).is_err());
    assert_eq!(doc.workbook(), &before);
    assert_eq!(doc.active_index(), 0);
    assert!(!doc.can_undo());
}

#[test]
fn workbook_byte_limits_are_atomic_for_edit_merge_and_duplicate() {
    let mut sheet = Sheet::new("Full", "Root");
    for index in 0..4 {
        sheet
            .resources
            .insert(
                Resource::new(
                    "large.bin",
                    "application/octet-stream",
                    vec![index; MAX_RESOURCE_BYTES],
                )
                .unwrap(),
            )
            .unwrap();
    }
    let before = sheet.resources.clone();
    assert!(
        sheet
            .resources
            .insert(Resource::new("one-more-byte", "text/plain", vec![99]).unwrap(),)
            .is_err()
    );
    assert_eq!(sheet.resources, before);
    let mut doc = Document::new(Workbook::new(sheet.clone()));
    doc.add_sheet(sheet.clone()).unwrap();
    doc.mark_saved();
    let before = doc.workbook().clone();
    let state = doc.state_id();
    assert!(doc.duplicate_sheet(0).is_err());
    assert!(doc.merge_sheets(vec![sheet]).is_err());
    assert_eq!(doc.workbook(), &before);
    assert_eq!(doc.state_id(), state);
    assert!(!doc.is_dirty());
    // A small third sheet is allowed, but adding bytes there must still respect the book total.
    doc.add_sheet(Sheet::new("Empty", "Root")).unwrap();
    doc.mark_saved();
    let state = doc.state_id();
    let before = doc.workbook().clone();
    assert!(
        doc.edit_sheet("Over book limit", |sheet| {
            sheet
                .resources
                .insert(Resource::new("extra", "text/plain", vec![99]).unwrap())
                .map_err(thinoda_core::edit::EditError::Invalid)?;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(doc.workbook(), &before);
    assert_eq!(doc.state_id(), state);
    assert!(!doc.is_dirty());
}

#[test]
fn malformed_resource_maps_and_missing_references_are_rejected() {
    let (mut sheet, _, reference) = embedded_sheet();
    let json = serde_json::to_string(&sheet.resources).unwrap();
    let id = reference.strip_prefix("thinoda-resource:").unwrap();
    let value = serde_json::to_string(sheet.resources.get(&reference).unwrap()).unwrap();
    assert!(
        serde_json::from_str::<ResourceStore>(&format!(
            "{{\"version\":1,\"entries\":{{\"{id}\":{value},\"{id}\":{value}}}}}"
        ))
        .is_err()
    );
    let restored: ResourceStore = serde_json::from_str(&json).unwrap();
    restored.validate().unwrap();
    sheet.resources.entries.clear();
    assert!(
        sheet
            .validate()
            .unwrap_err()
            .contains("missing embedded resource")
    );
}
