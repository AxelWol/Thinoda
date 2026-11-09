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
    let clipboard = copy_topics(&sheet, &[child]).unwrap().unwrap();
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
fn ordinary_subtree_paste_merges_multiple_resources_and_undoes_atomically() {
    let mut source = Sheet::new("Source", "Root");
    let bytes = [b"shared".as_slice(), b"second", b"third", b"second"];
    let mut topics = Vec::new();
    for (index, data) in bytes.iter().enumerate() {
        let name = format!("source-{index}.txt");
        let reference = source
            .resources
            .insert(Resource::new(&name, "text/plain", data.to_vec()).unwrap())
            .unwrap();
        let topic = source.add_child(source.root, None, &name);
        source.topic_mut(topic).unwrap().attachment = Some(thinoda_core::attachments::Attachment {
            resource: reference,
            name,
        });
        topics.push(topic);
    }
    let payload = copy_topics(&source, &topics).unwrap().unwrap();
    let mut target = Sheet::new("Target", "Root");
    let shared = target
        .resources
        .insert(Resource::new("target.txt", "text/plain", b"shared".to_vec()).unwrap())
        .unwrap();
    let mut doc = Document::new(Workbook::new(target));
    let before = doc.workbook().clone();
    let mut malformed = payload.subtrees.clone();
    let resources = &mut malformed[1].resources.entries;
    let (_, resource) = resources.pop_first().unwrap();
    resources.insert("0".repeat(64), resource);
    assert!(
        doc.edit_sheet("Invalid paste", |sheet| sheet.paste(sheet.root, &malformed))
            .is_err()
    );
    assert_eq!(doc.workbook(), &before);
    assert!(!doc.can_undo());

    let pasted = doc
        .edit_sheet("Paste", |sheet| sheet.paste(sheet.root, &payload.subtrees))
        .unwrap();
    assert_eq!(pasted.len(), bytes.len());
    assert_eq!(doc.sheet().resources.entries.len(), 3);
    assert_eq!(
        doc.sheet().resources.get(&shared).unwrap().name,
        "target.txt"
    );
    for (id, expected) in pasted.into_iter().zip(bytes) {
        let attachment = doc.sheet().topic(id).unwrap().attachment.as_ref().unwrap();
        assert_eq!(
            doc.sheet()
                .resources
                .get(&attachment.resource)
                .unwrap()
                .bytes(),
            expected
        );
    }
    doc.workbook().validate().unwrap();
    let after = doc.workbook().clone();
    assert!(doc.undo());
    assert_eq!(doc.workbook(), &before);
    assert!(doc.redo());
    assert_eq!(doc.workbook(), &after);
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
    let duplicate = Resource::new(
        "duplicate.bin",
        "application/octet-stream",
        vec![0; MAX_RESOURCE_BYTES],
    )
    .unwrap();
    assert_eq!(sheet.resources.merged([duplicate]).unwrap(), before);
    assert!(
        sheet
            .resources
            .merged([Resource::new("one-more-byte", "text/plain", vec![99]).unwrap()])
            .is_err()
    );
    let mut over_limit = sheet.resources.clone();
    let extra = Resource::new(
        "extra.bin",
        "application/octet-stream",
        vec![4; MAX_RESOURCE_BYTES],
    )
    .unwrap();
    over_limit.entries.insert(extra.id(), extra);
    let over_limit_before = over_limit.clone();
    assert!(over_limit.merged([]).is_err());
    assert_eq!(over_limit, over_limit_before);
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

#[test]
fn bulk_resource_merge_matches_sequential_insertion_and_keeps_metadata() {
    let first = Resource::new("original.txt", "text/plain", b"first".to_vec()).unwrap();
    let second = Resource::new("second.txt", "text/plain", b"second".to_vec()).unwrap();
    let third = Resource::new("third.txt", "text/plain", b"third".to_vec()).unwrap();
    let mut existing = ResourceStore::default();
    let first_reference = existing.insert(first.clone()).unwrap();
    let before = existing.clone();
    let duplicates = [
        Resource::new("alternate.txt", "text/plain", b"first".to_vec()).unwrap(),
        second.clone(),
        Resource::new("alternate-second.txt", "text/plain", b"second".to_vec()).unwrap(),
        third,
    ];

    let mut sequential = existing.clone();
    for resource in &duplicates {
        sequential.insert(resource.clone()).unwrap();
    }
    let merged = existing.merged(duplicates).unwrap();
    assert_eq!(merged, sequential);
    assert_eq!(existing, before);
    assert_eq!(merged.entries.len(), 3);
    assert_eq!(merged.get(&first_reference).unwrap().name, "original.txt");
    assert_eq!(
        merged
            .get(&format!("thinoda-resource:{}", second.id()))
            .unwrap()
            .name,
        "second.txt"
    );
    merged.validate().unwrap();
}

#[test]
fn bulk_resource_merge_matches_sequential_insertion_for_mixed_orders() {
    let pool: Vec<_> = (0u8..16)
        .map(|index| {
            Resource::new(
                &format!("resource-{index}"),
                "application/octet-stream",
                vec![index; usize::from(index) + 1],
            )
            .unwrap()
        })
        .collect();
    for seed in 0..40 {
        let mut existing = ResourceStore::default();
        for index in 0..seed % 8 {
            existing
                .insert(pool[(seed + index * 3) % pool.len()].clone())
                .unwrap();
        }
        let incoming: Vec<_> = (0..seed % 20)
            .map(|index| pool[(seed * 7 + index * 5) % pool.len()].clone())
            .collect();
        let mut sequential = existing.clone();
        for resource in &incoming {
            sequential.insert(resource.clone()).unwrap();
        }
        assert_eq!(
            existing.merged(incoming).unwrap(),
            sequential,
            "seed {seed}"
        );
    }
}

#[test]
fn bulk_resource_merge_rejects_bad_sources_and_inputs_without_mutating_the_original() {
    let mut existing = ResourceStore::default();
    existing
        .insert(Resource::new("existing", "text/plain", vec![1]).unwrap())
        .unwrap();
    let before = existing.clone();
    let mut bad_name = Resource::new("valid", "text/plain", vec![3]).unwrap();
    bad_name.name = "../invalid".into();
    let mut bad_media_type = Resource::new("valid", "text/plain", vec![4]).unwrap();
    bad_media_type.media_type = "text/plain;invalid".into();
    let mut bad_duplicate = Resource::new("valid", "text/plain", vec![1]).unwrap();
    bad_duplicate.name = "../invalid".into();
    for invalid in [bad_name, bad_media_type, bad_duplicate] {
        let valid = Resource::new("valid", "text/plain", vec![2]).unwrap();
        assert!(existing.merged([valid, invalid]).is_err());
        assert_eq!(existing, before);
    }

    let mut corrupt = existing.clone();
    corrupt.entries.insert(
        "0".repeat(64),
        Resource::new("corrupt", "text/plain", vec![5]).unwrap(),
    );
    let corrupt_before = corrupt.clone();
    assert!(corrupt.merged([]).unwrap_err().contains("digest mismatch"));
    assert_eq!(corrupt, corrupt_before);
    corrupt.version = 2;
    assert!(corrupt.merged([]).unwrap_err().contains("unsupported"));
    let mut bad_existing_metadata = existing.clone();
    bad_existing_metadata
        .entries
        .values_mut()
        .next()
        .unwrap()
        .name = "../invalid".into();
    assert!(
        bad_existing_metadata
            .merged([])
            .unwrap_err()
            .contains("invalid embedded resource display name")
    );
}

#[test]
fn bulk_resource_merge_allows_duplicates_at_the_entry_limit_but_rejects_new_bytes() {
    let mut full = ResourceStore::default();
    for index in 0..MAX_RESOURCES {
        full.insert(
            Resource::new(
                "entry",
                "application/octet-stream",
                index.to_le_bytes().to_vec(),
            )
            .unwrap(),
        )
        .unwrap();
    }
    let before = full.clone();
    let duplicate = Resource::new(
        "different-name",
        "application/octet-stream",
        0usize.to_le_bytes().to_vec(),
    )
    .unwrap();
    assert_eq!(full.merged([duplicate.clone()]).unwrap(), full);
    let overflow = Resource::new("overflow", "application/octet-stream", vec![255]).unwrap();
    assert!(full.merged([duplicate, overflow]).is_err());
    assert_eq!(full, before);
    let mut over_limit = full.clone();
    let extra = Resource::new("overflow", "text/plain", vec![255]).unwrap();
    over_limit.entries.insert(extra.id(), extra);
    let over_limit_before = over_limit.clone();
    assert!(over_limit.merged([]).is_err());
    assert_eq!(over_limit, over_limit_before);
}
