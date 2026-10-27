//! Opaque local files. Names are display/download metadata, never paths to resolve.
use crate::{Sheet, TopicId, edit::EditError, resources::Resource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub resource: String,
    /// Per-topic name: identical bytes may have different original filenames.
    pub name: String,
}

/// Portable basename, including Windows reserved names and Unicode direction controls.
/// This is also applied when saving loaded metadata, not just at file selection.
pub fn safe_name(input: &str) -> String {
    let basename = input.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name: String = basename
        .chars()
        .map(|c| {
            if c.is_control()
                || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    while name.len() > 255 {
        name.pop();
    }
    name = name
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .to_owned();
    if name.is_empty() || matches!(name.as_str(), "." | "..") {
        return "attachment".into();
    }
    let stem = name.split('.').next().unwrap_or_default().trim_end();
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            upper.strip_prefix(prefix).is_some_and(|n| {
                matches!(
                    n,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        })
    {
        name.insert(0, '_');
    }
    while name.len() > 255 {
        name.pop();
    }
    name.trim_end_matches(|c: char| c.is_whitespace() || c == '.')
        .to_owned()
}

impl Attachment {
    pub fn validate(&self, sheet: &Sheet) -> Result<(), String> {
        if self.name != safe_name(&self.name) {
            return Err("invalid attachment filename".into());
        }
        sheet.resources.get(&self.resource)?;
        Ok(())
    }
}

/// Called inside a Document transaction: resource, child and expansion are one action.
/// No sniffing/decoding: even documents, archives and executable files stay opaque.
pub fn insert(
    sheet: &mut Sheet,
    parent: TopicId,
    name: &str,
    bytes: Vec<u8>,
) -> Result<TopicId, EditError> {
    if !sheet.contains(parent) {
        return Err(EditError::NotFound);
    }
    let name = safe_name(name);
    let resource =
        Resource::new(&name, "application/octet-stream", bytes).map_err(EditError::Invalid)?;
    let reference = sheet
        .resources
        .insert(resource)
        .map_err(EditError::Invalid)?;
    let id = sheet.insert_child(parent, &format!("📎 {name}"))?;
    sheet
        .topic_mut(parent)
        .ok_or(EditError::NotFound)?
        .collapsed = false;
    sheet.topic_mut(id).ok_or(EditError::NotFound)?.attachment = Some(Attachment {
        resource: reference,
        name,
    });
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Workbook, clipboard};

    #[test]
    fn portable_names_and_opaque_types() {
        for (input, expected) in [
            ("../evidence.docm", "evidence.docm"),
            ("C:\\folder\\CON.txt", "_CON.txt"),
            ("a\u{202e}exe.txt", "a_exe.txt"),
            ("bad:*?.bin ", "bad___.bin"),
            ("..", "attachment"),
            ("", "attachment"),
        ] {
            assert_eq!(safe_name(input), expected);
        }
        let name = safe_name(&"é".repeat(300));
        assert!(name.len() <= 255);
        assert_eq!(safe_name(&name), name);
        for input in [
            format!("CON.{}", "a".repeat(300)),
            format!("{} .more", "a".repeat(254)),
        ] {
            let name = safe_name(&input);
            assert!(name.len() <= 255);
            assert_eq!(safe_name(&name), name);
        }
    }

    #[test]
    fn attachment_transaction_dedup_names_undo_clipboard_and_masking() {
        let mut doc = Document::new(Workbook::new(Sheet::new("Evidence", "Root")));
        let root = doc.sheet().root;
        let before = doc.workbook().clone();
        let bytes = b"\0PK\x03\x04opaque macro data\xff".to_vec();
        let id = doc
            .edit_sheet("Attachment", |s| {
                insert(s, root, "evidence.docm", bytes.clone())
            })
            .unwrap();
        let attached = doc.workbook().clone();
        doc.undo();
        assert_eq!(doc.workbook(), &before);
        doc.redo();
        assert_eq!(doc.workbook(), &attached);
        doc.edit_sheet("Attachment", |s| {
            insert(s, root, "different.exe", bytes.clone())
        })
        .unwrap();
        assert_eq!(doc.sheet().resources.entries.len(), 1);
        let sheet =
            clipboard::decode_sheet(&clipboard::encode_sheet(doc.sheet()).unwrap()).unwrap();
        assert_eq!(&sheet, doc.sheet());
        let subtree = sheet.copy_subtree(id).unwrap();
        let mut target = Sheet::new("Target", "Root");
        let pasted = target.paste(target.root, &[subtree]).unwrap()[0];
        let attachment = target.topic(pasted).unwrap().attachment.as_ref().unwrap();
        assert_eq!(attachment.name, "evidence.docm");
        assert_eq!(
            target.resources.get(&attachment.resource).unwrap().bytes(),
            bytes
        );
        target.validate().unwrap();
        target.resources.entries.clear();
        assert!(target.validate().is_err());
        let subtree = doc.sheet().copy_subtree(id).unwrap();
        let mut missing = subtree.clone();
        missing.resources.entries.clear();
        let mut destination = Sheet::new("Destination", "Root");
        let before = destination.clone();
        assert!(destination.paste(destination.root, &[missing]).is_err());
        assert_eq!(destination, before);
        assert!(destination.paste(destination.root, &[subtree]).is_ok());
        let masked = doc.workbook().masked_copy();
        assert!(
            masked
                .sheets
                .iter()
                .all(|s| s.topics.values().all(|t| t.attachment.is_none()))
        );
    }

    #[test]
    fn invalid_sizes_and_missing_parent_leave_document_unchanged() {
        let mut doc = Document::new(Workbook::new(Sheet::new("Evidence", "Root")));
        let root = doc.sheet().root;
        let before = doc.workbook().clone();
        for bytes in [vec![], vec![0; crate::resources::MAX_RESOURCE_BYTES + 1]] {
            assert!(
                doc.edit_sheet("Attachment", |s| insert(s, root, "file", bytes))
                    .is_err()
            );
            assert_eq!(doc.workbook(), &before);
        }
        assert!(
            doc.edit_sheet("Attachment", |s| insert(
                s,
                TopicId(u64::MAX),
                "file",
                vec![0]
            ))
            .is_err()
        );
        assert_eq!(doc.workbook(), &before);
    }

    #[test]
    fn maximum_resource_and_entry_limit_are_transactional() {
        let mut doc = Document::new(Workbook::new(Sheet::new("Evidence", "Root")));
        let root = doc.sheet().root;
        doc.edit_sheet("Attachment", |s| {
            insert(
                s,
                root,
                "large.bin",
                vec![0; crate::resources::MAX_RESOURCE_BYTES],
            )
        })
        .unwrap();
        for n in 1..crate::resources::MAX_RESOURCES {
            doc.edit_sheet("Attachment", |s| {
                insert(s, root, "small.bin", n.to_le_bytes().to_vec())
            })
            .unwrap();
        }
        let before = doc.workbook().clone();
        assert!(
            doc.edit_sheet("Attachment", |s| insert(
                s,
                root,
                "overflow.bin",
                vec![255; 20]
            ))
            .is_err()
        );
        assert_eq!(doc.workbook(), &before);
        assert_eq!(
            doc.sheet().resources.entries.len(),
            crate::resources::MAX_RESOURCES
        );
    }

    #[test]
    fn image_removal_keeps_shared_attachment_bytes() {
        let mut sheet = Sheet::new("Evidence", "Root");
        let root = sheet.root;
        let id = insert(&mut sheet, root, "opaque.png", vec![1, 2, 3]).unwrap();
        let reference = sheet
            .topic(id)
            .unwrap()
            .attachment
            .as_ref()
            .unwrap()
            .resource
            .clone();
        sheet.topic_mut(root).unwrap().image = Some(reference.clone());
        crate::images::set_image(&mut sheet, root, None).unwrap();
        assert_eq!(sheet.resources.get(&reference).unwrap().bytes(), [1, 2, 3]);
        sheet.validate().unwrap();
        assert!(crate::images::validate_visual(&sheet).is_err());
    }
}
