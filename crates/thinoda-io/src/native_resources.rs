use crate::{IoError, formats::archive, zip_entry_with_limit};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Cursor};
use thinoda_core::{
    Workbook,
    resources::{
        MAX_RESOURCE_BYTES, MAX_RESOURCES, MAX_SHEET_RESOURCE_BYTES, MAX_WORKBOOK_RESOURCE_BYTES,
        Resource,
    },
};
use zip::ZipArchive;

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    sheet: usize,
    id: String,
    name: String,
    media_type: String,
    size: usize,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u64,
    resources: Vec<Entry>,
}

// zip 9 indexes by name and silently coalesces duplicates. Count the bounded
// central records as well so ambiguous entries cannot disappear from validation.
pub(crate) fn validate_directory(
    archive: &ZipArchive<Cursor<&[u8]>>,
    bytes: &[u8],
) -> Result<(), IoError> {
    let mut offset = usize::try_from(archive.central_directory_start())
        .map_err(|_| IoError::Invalid("native archive directory offset exceeds limits".into()))?;
    let mut count = 0usize;
    while bytes.get(offset..offset.saturating_add(4)) == Some(b"PK\x01\x02") {
        count += 1;
        if count > thinoda_core::resources::MAX_RESOURCES + 2 {
            return Err(IoError::Invalid(
                "native resource archive contains too many entries".into(),
            ));
        }
        let header = bytes
            .get(offset..offset.saturating_add(46))
            .ok_or_else(|| IoError::Invalid("truncated native archive directory".into()))?;
        let name = usize::from(u16::from_le_bytes([header[28], header[29]]));
        let extra = usize::from(u16::from_le_bytes([header[30], header[31]]));
        let comment = usize::from(u16::from_le_bytes([header[32], header[33]]));
        offset = offset
            .checked_add(46 + name + extra + comment)
            .filter(|offset| *offset <= bytes.len())
            .ok_or_else(|| IoError::Invalid("truncated native archive directory entry".into()))?;
    }
    if count != archive.len() {
        return Err(IoError::Invalid(
            "duplicate or inconsistent native archive entries".into(),
        ));
    }
    Ok(())
}

pub(crate) fn export(book: &Workbook, version: u64) -> Result<Vec<u8>, IoError> {
    let mut content = book.clone();
    let mut entries = Vec::new();
    let mut blobs = BTreeMap::new();
    for (index, sheet) in content.sheets.iter_mut().enumerate() {
        for (id, resource) in std::mem::take(&mut sheet.resources.entries) {
            entries.push(Entry {
                sheet: index,
                id: id.clone(),
                name: resource.name.clone(),
                media_type: resource.media_type.clone(),
                size: resource.bytes().len(),
            });
            blobs.entry(format!("resources/{id}")).or_insert(resource);
        }
    }
    let manifest = serde_json::to_vec(&Manifest {
        format: "thinoda".into(),
        version,

        resources: entries,
    })?;
    let content = serde_json::to_vec(&content)?;
    let mut files: Vec<(&str, &[u8])> =
        vec![("manifest.json", &manifest), ("content.json", &content)];
    files.extend(
        blobs
            .iter()
            .map(|(name, resource)| (name.as_str(), resource.bytes())),
    );
    archive(&files)
}

pub(crate) fn hydrate(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    manifest: &[u8],
    book: &mut Workbook,
) -> Result<(), IoError> {
    let manifest: Manifest = serde_json::from_slice(manifest)?;
    if manifest.format != "thinoda" || !matches!(manifest.version, 3..=crate::NATIVE_FORMAT_VERSION)
    {
        return Err(IoError::Invalid("invalid native resource manifest".into()));
    }
    if archive.len() > MAX_RESOURCES + 2 {
        return Err(IoError::Invalid(
            "native resource archive contains too many entries".into(),
        ));
    }
    if manifest.resources.len() > MAX_RESOURCES {
        return Err(IoError::Invalid(
            "workbook embedded resources exceed 128 entries".into(),
        ));
    }
    if book.sheets.iter().any(|sheet| !sheet.resources.is_empty()) {
        return Err(IoError::Invalid(
            "native resources must be archive entries, not inline data".into(),
        ));
    }
    let mut names = std::collections::BTreeSet::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let name = file.name()?.into_owned();
        if !names.insert(name.clone()) {
            return Err(IoError::Invalid(format!(
                "duplicate native archive entry {name}"
            )));
        }
    }
    names.remove("manifest.json");
    names.remove("content.json");

    // The stores are known to be empty. Validate and populate private copies so
    // an error cannot leave a partially hydrated workbook. Each new resource is
    // verified below before insertion, without rehashing all preceding entries.
    let mut stores: Vec<_> = book
        .sheets
        .iter()
        .map(|sheet| sheet.resources.clone())
        .collect();
    for store in &stores {
        store.validate().map_err(IoError::Invalid)?;
    }
    let mut sheet_totals = vec![0usize; stores.len()];
    let mut total = 0usize;
    for entry in manifest.resources {
        if entry.id.len() != 64
            || !entry
                .id
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(IoError::Invalid("invalid embedded resource ID".into()));
        }
        if entry.size == 0 || entry.size > MAX_RESOURCE_BYTES {
            return Err(IoError::Invalid(
                "embedded resource must contain 1 byte to 8 MiB".into(),
            ));
        }
        total = total.saturating_add(entry.size);
        if total > MAX_WORKBOOK_RESOURCE_BYTES {
            return Err(IoError::Invalid(
                "workbook embedded resources exceed 64 MiB".into(),
            ));
        }
        let store = stores
            .get_mut(entry.sheet)
            .ok_or_else(|| IoError::Invalid("embedded resource sheet does not exist".into()))?;
        if store.entries.contains_key(&entry.id) {
            return Err(IoError::Invalid(
                "duplicate embedded resource manifest entry".into(),
            ));
        }
        let path = format!("resources/{}", entry.id);
        names.remove(&path);
        let bytes = zip_entry_with_limit(archive, &path, entry.size as u64)?;
        if bytes.len() != entry.size {
            return Err(IoError::Invalid("embedded resource size mismatch".into()));
        }
        let resource =
            Resource::new(&entry.name, &entry.media_type, bytes).map_err(IoError::Invalid)?;
        if resource.id() != entry.id {
            return Err(IoError::Invalid("embedded resource digest mismatch".into()));
        }
        let sheet_total = sheet_totals[entry.sheet].saturating_add(resource.bytes().len());
        if store.entries.len() >= MAX_RESOURCES || sheet_total > MAX_SHEET_RESOURCE_BYTES {
            return Err(IoError::Invalid(
                "sheet embedded resources exceed 128 entries or 32 MiB".into(),
            ));
        }
        store.entries.insert(entry.id, resource);
        sheet_totals[entry.sheet] = sheet_total;
    }
    if !names.is_empty() {
        return Err(IoError::Invalid(
            "unlisted native resource archive entries".into(),
        ));
    }
    for (sheet, resources) in book.sheets.iter_mut().zip(stores) {
        sheet.resources = resources;
    }
    Ok(())
}

#[cfg(test)]
#[path = "native_resources_tests.rs"]
mod tests;
