use crate::{IoError, formats::archive, zip_entry_with_limit};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Cursor};
use thinoda_core::{
    Workbook,
    resources::{MAX_RESOURCE_BYTES, MAX_WORKBOOK_RESOURCE_BYTES, Resource},
};
use zip::ZipArchive;

#[derive(Serialize, Deserialize)]
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

pub(crate) fn export(book: &Workbook) -> Result<Vec<u8>, IoError> {
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
        version: if book.sheets.iter().any(|sheet| sheet.fim.is_some()) {
            5
        } else if thinoda_core::notes::has_formatted_notes(book) {
            4
        } else {
            3
        },
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
    if manifest.format != "thinoda" || !matches!(manifest.version, 3..=5) {
        return Err(IoError::Invalid("invalid native resource manifest".into()));
    }
    if archive.len() > thinoda_core::resources::MAX_RESOURCES + 2 {
        return Err(IoError::Invalid(
            "native resource archive contains too many entries".into(),
        ));
    }
    if manifest.resources.len() > thinoda_core::resources::MAX_RESOURCES {
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
        let sheet = book
            .sheets
            .get_mut(entry.sheet)
            .ok_or_else(|| IoError::Invalid("embedded resource sheet does not exist".into()))?;
        if sheet.resources.entries.contains_key(&entry.id) {
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
        sheet.resources.insert(resource).map_err(IoError::Invalid)?;
    }
    if !names.is_empty() {
        return Err(IoError::Invalid(
            "unlisted native resource archive entries".into(),
        ));
    }
    Ok(())
}
