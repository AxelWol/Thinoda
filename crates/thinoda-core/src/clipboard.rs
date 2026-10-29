//! Clipboard payloads: internal subtrees plus a plain-text representation for the OS.

use crate::model::{Sheet, Subtree, TopicId};
use crate::outline::{OutlineNode, parse_indented_text, to_indented_text, to_outline};
use serde::{Deserialize, Serialize};

const SHEET_PREFIX: &str = "THINODA-SHEET-1\n";
const RESOURCE_SHEET_PREFIX: &str = "THINODA-SHEET-2\n";
const FORMATTED_SHEET_PREFIX: &str = "THINODA-SHEET-3\n";
const MAX_SHEET_CLIPBOARD_BYTES: usize = 16 * 1024 * 1024;

/// A versioned OS text payload retaining every persisted sheet field.
pub fn encode_sheet(sheet: &Sheet) -> Result<String, String> {
    sheet.validate()?;
    let prefix = if sheet.topics.values().any(|topic| {
        topic
            .notes
            .as_ref()
            .is_some_and(crate::notes::NoteContent::is_markdown)
    }) {
        FORMATTED_SHEET_PREFIX
    } else if sheet.resources.is_empty() {
        SHEET_PREFIX
    } else {
        RESOURCE_SHEET_PREFIX
    };
    let json = serde_json::to_string(sheet).map_err(|e| e.to_string())?;
    if json.len() + prefix.len() > MAX_SHEET_CLIPBOARD_BYTES {
        return Err("sheet clipboard payload exceeds 16 MiB".into());
    }
    Ok(format!("{prefix}{json}"))
}

pub fn decode_sheet(text: &str) -> Result<Sheet, String> {
    if text.len() > MAX_SHEET_CLIPBOARD_BYTES {
        return Err("sheet clipboard payload exceeds 16 MiB".into());
    }
    // Windows clipboard APIs may normalize line endings.
    let (version, json) = text
        .strip_prefix(FORMATTED_SHEET_PREFIX)
        .or_else(|| text.strip_prefix("THINODA-SHEET-3\r\n"))
        .map(|json| (3, json))
        .or_else(|| {
            text.strip_prefix(RESOURCE_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-2\r\n"))
                .map(|json| (2, json))
        })
        .or_else(|| {
            text.strip_prefix(SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-1\r\n"))
                .map(|json| (1, json))
        })
        .ok_or("clipboard does not contain a supported Thinoda sheet")?;
    let sheet: Sheet = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if version == 1 && !sheet.resources.is_empty() {
        return Err("embedded resources require sheet clipboard version 2".into());
    }
    if version < 3
        && sheet.topics.values().any(|topic| {
            topic
                .notes
                .as_ref()
                .is_some_and(crate::notes::NoteContent::is_markdown)
        })
    {
        return Err("formatted notes require sheet clipboard version 3".into());
    }
    sheet.validate()?;
    Ok(sheet)
}

/// Content copied from the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub subtrees: Vec<Subtree>,
    /// Plain text written to the OS clipboard; used to detect whether the OS clipboard
    /// still holds our content.
    pub text: String,
}

/// Copies the given topics (descendants of other selected topics are skipped).
pub fn copy_topics(sheet: &Sheet, ids: &[TopicId]) -> Option<ClipboardPayload> {
    let roots: Vec<TopicId> = ids
        .iter()
        .copied()
        .filter(|id| sheet.contains(*id))
        .filter(|id| {
            !ids.iter()
                .any(|o| o != id && sheet.ancestors(*id).contains(o))
        })
        .collect();
    if roots.is_empty() {
        return None;
    }
    let subtrees: Vec<Subtree> = roots
        .iter()
        .filter_map(|r| sheet.copy_subtree(*r))
        .collect();
    let outlines: Vec<OutlineNode> = roots.iter().map(|r| to_outline(sheet, *r)).collect();
    Some(ClipboardPayload {
        subtrees,
        text: to_indented_text(&outlines),
    })
}

/// Builds subtrees from arbitrary text (each line becomes a topic).
pub fn subtrees_from_text(text: &str) -> Vec<Subtree> {
    let nodes = parse_indented_text(text);
    nodes
        .iter()
        .map(|n| {
            let mut tmp = Sheet::new("tmp", n.title.clone());
            let root = tmp.root;
            for c in &n.children {
                crate::outline::add_outline(&mut tmp, root, c);
            }
            tmp.copy_subtree(root).expect("root exists")
        })
        .collect()
}

/// Resolves what to paste: internal subtrees when the OS text still matches, otherwise
/// the OS text parsed as an outline.
pub fn resolve_paste(internal: Option<&ClipboardPayload>, os_text: Option<&str>) -> Vec<Subtree> {
    match (internal, os_text) {
        (Some(p), Some(t)) if normalize(t) == normalize(&p.text) => p.subtrees.clone(),
        (_, Some(t)) if !t.trim().is_empty() => subtrees_from_text(t),
        (Some(p), None) => p.subtrees.clone(),
        _ => vec![],
    }
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_sheet_clipboard_roundtrips_and_rejects_invalid_input() {
        let mut sheet = Sheet::new("Sheet \u{00fc}", "Root");
        let a = sheet.add_child(sheet.root, None, "A");
        let b = sheet.add_child(sheet.root, None, "B");
        let summary = sheet.add_summary(&[a, b], "Summary").unwrap();
        let child = sheet.add_child(summary, None, "Summary child");
        sheet.add_summary(&[child], "Nested").unwrap();
        sheet.add_boundary(&[a, b]).unwrap();
        let floating = sheet.add_floating("Floating", crate::Vec2::new(20.0, 30.0));
        sheet.add_relationship(child, floating).unwrap();
        let zone = sheet
            .create_zone(
                "Floating group",
                vec![floating],
                crate::Rect::from_min_size(crate::Vec2::ZERO, crate::Vec2::new(150.0, 120.0)),
            )
            .unwrap();
        sheet
            .add_presentation_slide(
                "Floating group",
                crate::PresentationSource::Zone(zone),
                crate::Rect::from_min_size(crate::Vec2::ZERO, crate::Vec2::new(150.0, 120.0)),
            )
            .unwrap();
        sheet.topic_mut(a).unwrap().notes = Some("Notes".into());
        sheet.topic_mut(b).unwrap().style.bold = Some(true);
        let payload = encode_sheet(&sheet).unwrap();
        assert_eq!(decode_sheet(&payload).unwrap(), sheet);
        assert_eq!(
            decode_sheet(&payload.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        for text in ["plain text", "THINODA-SHEET-2\n{}", "THINODA-SHEET-1\n{"] {
            assert!(decode_sheet(text).is_err());
        }
        let mut invalid = sheet.clone();
        invalid.topics.remove(&invalid.root);
        assert!(encode_sheet(&invalid).is_err());
        assert!(
            decode_sheet(&format!(
                "{SHEET_PREFIX}{}",
                serde_json::to_string(&invalid).unwrap()
            ))
            .is_err()
        );
        assert!(decode_sheet(&"x".repeat(MAX_SHEET_CLIPBOARD_BYTES + 1)).is_err());
        let mut exact_limit = payload.clone();
        exact_limit.extend(std::iter::repeat_n(
            ' ',
            MAX_SHEET_CLIPBOARD_BYTES - payload.len(),
        ));
        assert_eq!(decode_sheet(&exact_limit).unwrap(), sheet);
        exact_limit.push(' ');
        assert!(
            decode_sheet(&exact_limit)
                .unwrap_err()
                .contains("exceeds 16 MiB")
        );
        sheet.topic_mut(a).unwrap().notes = Some("x".repeat(MAX_SHEET_CLIPBOARD_BYTES).into());
        assert!(encode_sheet(&sheet).is_err());
    }

    #[test]
    fn copy_skips_nested_selection() {
        let mut s = Sheet::new("S", "R");
        let a = s.add_child(s.root, None, "A");
        let a1 = s.add_child(a, None, "A1");
        let p = copy_topics(&s, &[a, a1]).unwrap();
        assert_eq!(p.subtrees.len(), 1);
        assert_eq!(p.text, "A\n\tA1\n");
    }

    #[test]
    fn paste_prefers_internal_when_text_matches() {
        let mut s = Sheet::new("S", "R");
        let a = s.add_child(s.root, None, "A");
        s.topic_mut(a).unwrap().markers.push("flag-red".into());
        let p = copy_topics(&s, &[a]).unwrap();
        let subs = resolve_paste(Some(&p), Some("A\r\n"));
        assert_eq!(subs[0].root_topic().markers, vec!["flag-red"]);
        let subs = resolve_paste(Some(&p), Some("X\n\tY"));
        assert_eq!(subs[0].root_topic().title, "X");
        assert_eq!(subs[0].len(), 2);
        assert!(resolve_paste(None, Some("  ")).is_empty());
    }

    #[test]
    fn pasted_text_creates_topics() {
        let mut s = Sheet::new("S", "R");
        let subs = subtrees_from_text("One\nTwo\n\tTwo.1");
        let root = s.root;
        let ids = s.paste(root, &subs).unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(s.children(ids[1]).len(), 1);
        s.validate().unwrap();
    }
}
