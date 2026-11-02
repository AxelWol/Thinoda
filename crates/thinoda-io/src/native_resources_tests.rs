use super::{Entry, Manifest, hydrate};
use crate::{Format, IoError, formats::archive, import_bytes};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};
use thinoda_core::{
    Sheet, Workbook,
    resources::{
        MAX_RESOURCE_BYTES, MAX_RESOURCES, MAX_SHEET_RESOURCE_BYTES, MAX_WORKBOOK_RESOURCE_BYTES,
        Resource,
    },
};
use zip::ZipArchive;

fn empty_book(sheets: usize) -> Workbook {
    let mut book = Workbook::new(Sheet::new("First", "Root"));
    for index in 1..sheets {
        book.sheets
            .push(Sheet::new(format!("Sheet {index}"), "Root"));
    }
    book
}

fn entry(sheet: usize, name: &str, bytes: &[u8]) -> Entry {
    let media_type = "application/octet-stream";
    let resource = Resource::new(name, media_type, bytes.to_vec()).unwrap();
    Entry {
        sheet,
        id: resource.id(),
        name: name.into(),
        media_type: media_type.into(),
        size: bytes.len(),
    }
}

fn fixture_archive(
    book: &Workbook,
    entries: &[Entry],
    blobs: &BTreeMap<String, Vec<u8>>,
    include_unlisted: bool,
) -> (Vec<u8>, Vec<u8>) {
    let manifest = serde_json::to_vec(&Manifest {
        format: "thinoda".into(),
        version: 3,
        resources: entries.to_vec(),
    })
    .unwrap();
    let content = serde_json::to_vec(book).unwrap();
    let listed: BTreeSet<_> = entries.iter().map(|entry| entry.id.as_str()).collect();
    let resource_files: Vec<_> = blobs
        .iter()
        .filter(|(id, _)| include_unlisted || listed.contains(id.as_str()))
        .map(|(id, bytes)| (format!("resources/{id}"), bytes.as_slice()))
        .collect();
    let mut files = vec![
        ("manifest.json", manifest.as_slice()),
        ("content.json", content.as_slice()),
    ];
    files.extend(
        resource_files
            .iter()
            .map(|(name, bytes)| (name.as_str(), *bytes)),
    );
    (archive(&files).unwrap(), manifest)
}

fn hydrate_fixture(book: &mut Workbook, bytes: &[u8], manifest: &[u8]) -> Result<(), IoError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    hydrate(&mut archive, manifest, book)
}

fn assert_rejected_unchanged(
    book: &mut Workbook,
    entries: &[Entry],
    blobs: &BTreeMap<String, Vec<u8>>,
    include_unlisted: bool,
    expected: &str,
) {
    let before = book.clone();
    let (bytes, manifest) = fixture_archive(book, entries, blobs, include_unlisted);
    let error = hydrate_fixture(book, &bytes, &manifest).unwrap_err();
    assert!(
        error.to_string().contains(expected),
        "expected {expected:?}, got {error}"
    );
    assert_eq!(*book, before, "failed hydration changed the workbook");
}

#[test]
fn hydrates_multiple_resources_and_shared_bytes_with_per_sheet_metadata() {
    let mut book = empty_book(2);
    let first = entry(0, "first.bin", b"first resource");
    let second = entry(0, "second.bin", b"second resource");
    let shared = entry(1, "shared-copy.bin", b"first resource");
    let third = entry(1, "third.bin", b"third resource");
    let entries = [first.clone(), second.clone(), shared.clone(), third.clone()];
    let blobs = BTreeMap::from([
        (first.id.clone(), b"first resource".to_vec()),
        (second.id.clone(), b"second resource".to_vec()),
        (third.id.clone(), b"third resource".to_vec()),
    ]);
    let (bytes, manifest) = fixture_archive(&book, &entries, &blobs, false);

    hydrate_fixture(&mut book, &bytes, &manifest).unwrap();
    assert_eq!(book.sheets[0].resources.entries.len(), 2);
    assert_eq!(book.sheets[1].resources.entries.len(), 2);
    assert_eq!(
        book.sheets[0].resources.entries[&first.id].name,
        "first.bin"
    );
    assert_eq!(
        book.sheets[1].resources.entries[&shared.id].name,
        "shared-copy.bin"
    );
    assert_eq!(
        book.sheets[0].resources.entries[&second.id].bytes(),
        b"second resource"
    );
    assert_eq!(
        book.sheets[1].resources.entries[&third.id].bytes(),
        b"third resource"
    );
    book.validate().unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), book);
}

#[test]
fn rejects_tampering_invalid_metadata_and_missing_or_unlisted_blobs_atomically() {
    let mut book = empty_book(1);
    let first = entry(0, "first.bin", b"first resource");
    let second = entry(0, "second.bin", b"second resource");
    let entries = [first.clone(), second.clone()];
    let blobs = BTreeMap::from([
        (first.id.clone(), b"first resource".to_vec()),
        (second.id.clone(), b"second resource".to_vec()),
    ]);

    let mut tampered = blobs.clone();
    tampered.get_mut(&second.id).unwrap()[0] ^= 1;
    assert_rejected_unchanged(&mut book, &entries, &tampered, false, "digest mismatch");

    let mut bad_size = entries.clone();
    bad_size[1].size += 1;
    assert_rejected_unchanged(
        &mut book,
        &bad_size,
        &blobs,
        false,
        "embedded resource size mismatch",
    );

    let mut bad_id = entries.clone();
    bad_id[1].id = "A".repeat(64);
    assert_rejected_unchanged(
        &mut book,
        &bad_id,
        &blobs,
        false,
        "invalid embedded resource ID",
    );

    let mut bad_sheet = entries.clone();
    bad_sheet[1].sheet = 1;
    assert_rejected_unchanged(
        &mut book,
        &bad_sheet,
        &blobs,
        false,
        "embedded resource sheet does not exist",
    );

    for (name, media_type, expected) in [
        (
            "../escape",
            "application/octet-stream",
            "invalid embedded resource display name",
        ),
        (
            "second.bin",
            "invalid",
            "invalid embedded resource media type",
        ),
    ] {
        let mut bad = entries.clone();
        bad[1].name = name.into();
        bad[1].media_type = media_type.into();
        assert_rejected_unchanged(&mut book, &bad, &blobs, false, expected);
    }

    let mut missing = blobs.clone();
    missing.remove(&second.id);
    assert_rejected_unchanged(&mut book, &entries, &missing, false, "not found");

    let mut unlisted = blobs.clone();
    unlisted.insert("unlisted".into(), b"orphan".to_vec());
    assert_rejected_unchanged(
        &mut book,
        &entries,
        &unlisted,
        true,
        "unlisted native resource archive entries",
    );
}

#[test]
fn rejects_duplicate_manifest_entries_and_invalid_empty_store_version_atomically() {
    let mut book = empty_book(2);
    let first = entry(0, "first.bin", b"first resource");
    let second = entry(0, "second.bin", b"second resource");
    let blobs = BTreeMap::from([
        (first.id.clone(), b"first resource".to_vec()),
        (second.id.clone(), b"second resource".to_vec()),
    ]);
    assert_rejected_unchanged(
        &mut book,
        &[first.clone(), second, first.clone()],
        &blobs,
        false,
        "duplicate embedded resource manifest entry",
    );

    book.sheets[1].resources.version = 2;
    assert_rejected_unchanged(
        &mut book,
        &[first],
        &blobs,
        false,
        "unsupported embedded resource version",
    );
}

#[test]
fn rejects_resource_size_and_manifest_count_limits_atomically() {
    let mut book = empty_book(1);
    let first = entry(0, "first.bin", b"first resource");
    let blobs = BTreeMap::from([(first.id.clone(), b"first resource".to_vec())]);
    for size in [0, MAX_RESOURCE_BYTES + 1] {
        let mut bad = first.clone();
        bad.size = size;
        assert_rejected_unchanged(
            &mut book,
            &[bad],
            &blobs,
            false,
            "embedded resource must contain 1 byte to 8 MiB",
        );
    }
    assert_rejected_unchanged(
        &mut book,
        &vec![first; MAX_RESOURCES + 1],
        &blobs,
        false,
        "workbook embedded resources exceed 128 entries",
    );
}

#[test]
fn accepts_the_exact_resource_count_limit() {
    let mut book = empty_book(1);
    let mut entries = Vec::new();
    let mut blobs = BTreeMap::new();
    for index in 0..MAX_RESOURCES {
        let bytes = (index as u32).to_le_bytes().to_vec();
        let resource = entry(0, &format!("resource-{index}"), &bytes);
        blobs.insert(resource.id.clone(), bytes);
        entries.push(resource);
    }
    let (bytes, manifest) = fixture_archive(&book, &entries, &blobs, false);
    hydrate_fixture(&mut book, &bytes, &manifest).unwrap();
    assert_eq!(book.sheets[0].resources.entries.len(), MAX_RESOURCES);
    book.validate().unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), book);
}

#[test]
fn accepts_exact_sheet_and_workbook_limits_and_rejects_excess_atomically() {
    assert_eq!(MAX_SHEET_RESOURCE_BYTES, 4 * MAX_RESOURCE_BYTES);
    assert_eq!(MAX_WORKBOOK_RESOURCE_BYTES, 2 * MAX_SHEET_RESOURCE_BYTES);
    let book = empty_book(3);
    let mut entries = Vec::new();
    let mut blobs = BTreeMap::new();
    for index in 0..8 {
        let bytes = vec![index as u8 + 1; MAX_RESOURCE_BYTES];
        let resource = entry(index / 4, &format!("blob-{index}.bin"), &bytes);
        blobs.insert(resource.id.clone(), bytes);
        entries.push(resource);
    }

    let (bytes, manifest) = fixture_archive(&book, &entries, &blobs, false);
    let mut at_limit = book.clone();
    hydrate_fixture(&mut at_limit, &bytes, &manifest).unwrap();
    assert_eq!(
        at_limit.sheets[0].resources.byte_len(),
        MAX_SHEET_RESOURCE_BYTES
    );
    assert_eq!(
        at_limit.sheets[1].resources.byte_len(),
        MAX_SHEET_RESOURCE_BYTES
    );
    at_limit.validate().unwrap();
    drop(at_limit);

    let tail_bytes = b"x".to_vec();
    let sheet_tail = entry(0, "sheet-tail.bin", &tail_bytes);
    blobs.insert(sheet_tail.id.clone(), tail_bytes.clone());
    let mut sheet_entries = entries[..4].to_vec();
    sheet_entries.push(sheet_tail);
    let mut unchanged = book.clone();
    assert_rejected_unchanged(
        &mut unchanged,
        &sheet_entries,
        &blobs,
        false,
        "sheet embedded resources exceed 128 entries or 32 MiB",
    );

    let workbook_tail = entry(2, "workbook-tail.bin", &tail_bytes);
    let mut workbook_entries = entries;
    workbook_entries.push(workbook_tail);
    assert_rejected_unchanged(
        &mut unchanged,
        &workbook_entries,
        &blobs,
        false,
        "workbook embedded resources exceed 64 MiB",
    );
}
