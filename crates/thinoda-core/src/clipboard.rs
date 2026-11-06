//! Clipboard payloads: internal subtrees plus a plain-text representation for the OS.

use crate::model::{Sheet, Subtree, TopicId};
use crate::outline::{OutlineNode, parse_indented_text, to_indented_text, to_outline};
use serde::{Deserialize, Serialize};

mod board;
mod topic;
pub use board::{
    BoardClipboardPayload, BoardPasteResult, MAX_BOARD_CLIPBOARD_BYTES, PreparedBoardSelection,
    ResolvedBoardPaste, copy_board_selection, decode_board_payload, encode_board_payload,
    paste_board_selection, paste_floating_subtrees, prepare_board_selection, resolve_board_paste,
};
pub use topic::{
    MAX_TOPIC_CLIPBOARD_DEPTH, TOPIC_CLIPBOARD_PREFIX, decode_topic_payload, encode_topic_payload,
};

const SHEET_PREFIX: &str = "THINODA-SHEET-1\n";
const RESOURCE_SHEET_PREFIX: &str = "THINODA-SHEET-2\n";
const FORMATTED_SHEET_PREFIX: &str = "THINODA-SHEET-3\n";
const FIM_SHEET_PREFIX: &str = "THINODA-SHEET-4\n";
const INLINE_SHEET_PREFIX: &str = "THINODA-SHEET-5\n";
const MAX_SHEET_CLIPBOARD_BYTES: usize = 16 * 1024 * 1024;

/// A versioned OS text payload retaining every persisted sheet field.
pub fn encode_sheet(sheet: &Sheet) -> Result<String, String> {
    sheet.validate()?;
    let prefix = if sheet.has_inline_text() {
        INLINE_SHEET_PREFIX
    } else if sheet.fim.is_some() {
        FIM_SHEET_PREFIX
    } else if sheet.topics.values().any(|topic| {
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
        .strip_prefix(INLINE_SHEET_PREFIX)
        .or_else(|| text.strip_prefix("THINODA-SHEET-5\r\n"))
        .map(|json| (5, json))
        .or_else(|| {
            text.strip_prefix(FIM_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-4\r\n"))
                .map(|json| (4, json))
        })
        .or_else(|| {
            text.strip_prefix(FORMATTED_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-3\r\n"))
                .map(|json| (3, json))
        })
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
    if version < 5 && sheet.has_inline_text() {
        return Err("inline card text requires sheet clipboard version 5".into());
    }
    if version < 4 && sheet.fim.is_some() {
        return Err("FIM documents require sheet clipboard version 4".into());
    }
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
    if sheet.fim.is_some() {
        return None;
    }
    let roots = topic_copy_roots(sheet, ids);
    if roots.is_empty() {
        return None;
    }
    Some(copy_topic_roots(sheet, &roots))
}

/// Copies topics after checking the portable clipboard's source count, depth and text limits.
pub fn try_copy_topics(sheet: &Sheet, ids: &[TopicId]) -> Result<Option<ClipboardPayload>, String> {
    if sheet.fim.is_some() {
        return Ok(None);
    }
    let roots = topic_copy_roots(sheet, ids);
    if roots.is_empty() {
        return Ok(None);
    }
    topic::validate_source_selection(sheet, &roots)?;
    Ok(Some(copy_topic_roots(sheet, &roots)))
}

fn topic_copy_roots(sheet: &Sheet, ids: &[TopicId]) -> Vec<TopicId> {
    ids.iter()
        .copied()
        .filter(|id| sheet.contains(*id) && !sheet.is_anchor(*id))
        .filter(|id| {
            !ids.iter()
                .any(|o| o != id && sheet.ancestors(*id).contains(o))
        })
        .collect()
}

fn copy_topic_roots(sheet: &Sheet, roots: &[TopicId]) -> ClipboardPayload {
    let subtrees: Vec<Subtree> = roots
        .iter()
        .filter_map(|r| sheet.copy_subtree(*r))
        .collect();
    let outlines: Vec<OutlineNode> = roots.iter().map(|r| to_outline(sheet, *r)).collect();
    ClipboardPayload {
        subtrees,
        text: to_indented_text(&outlines),
    }
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

    fn formatted_card_sheet(kind: crate::CardKind) -> Sheet {
        let mut sheet = Sheet::new("Board", "Root");
        let title = "A🦀é\n第二";
        let id = sheet
            .add_card(
                title,
                crate::Vec2::new(100.0, 100.0),
                crate::Card::new(kind, crate::Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let topic = sheet.topic_mut(id).unwrap();
        topic
            .inline_text
            .update_range(title, 0..3, |style| {
                style.bold = Some(true);
                style.font_family = Some("Sans".into());
                style.color = Some(crate::Color::rgb(20, 40, 60));
            })
            .unwrap();
        topic.inline_text.paragraphs.push(crate::InlineParagraph {
            index: 1,
            list: crate::ListKind::Numbered,
        });
        sheet.validate().unwrap();
        sheet
    }

    #[test]
    fn inline_card_sheet_clipboard_versions_and_limits_are_preserved() {
        for kind in [crate::CardKind::Sticky, crate::CardKind::Text] {
            let sheet = formatted_card_sheet(kind);
            let encoded = encode_sheet(&sheet).unwrap();
            assert!(encoded.starts_with(INLINE_SHEET_PREFIX));
            assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
            assert_eq!(
                decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
                sheet
            );
            for version in 1..=4 {
                let old =
                    encoded.replacen("THINODA-SHEET-5", &format!("THINODA-SHEET-{version}"), 1);
                assert!(
                    decode_sheet(&old)
                        .unwrap_err()
                        .contains("inline card text requires sheet clipboard version 5")
                );
            }

            let mut invalid = sheet.clone();
            let card = invalid.floating[0];
            invalid.topic_mut(card).unwrap().inline_text.runs[0].end = 100;
            assert!(encode_sheet(&invalid).is_err());
            let forged = format!(
                "{INLINE_SHEET_PREFIX}{}",
                serde_json::to_string(&invalid).unwrap()
            );
            assert!(decode_sheet(&forged).is_err());
        }

        let mut at_limit = formatted_card_sheet(crate::CardKind::Text);
        let root = at_limit.root;
        at_limit.topic_mut(root).unwrap().notes = Some("".into());
        let base_len = encode_sheet(&at_limit).unwrap().len();
        at_limit.topic_mut(root).unwrap().notes =
            Some("x".repeat(MAX_SHEET_CLIPBOARD_BYTES - base_len).into());
        let mut encoded = encode_sheet(&at_limit).unwrap();
        assert_eq!(encoded.len(), MAX_SHEET_CLIPBOARD_BYTES);
        assert_eq!(decode_sheet(&encoded).unwrap(), at_limit);
        assert!(decode_sheet(&encoded.replacen('\n', "\r\n", 1)).is_err());
        at_limit.topic_mut(root).unwrap().notes =
            Some("x".repeat(MAX_SHEET_CLIPBOARD_BYTES - base_len - 1).into());
        let windows = encode_sheet(&at_limit).unwrap().replacen('\n', "\r\n", 1);
        assert_eq!(windows.len(), MAX_SHEET_CLIPBOARD_BYTES);
        assert_eq!(decode_sheet(&windows).unwrap(), at_limit);
        encoded.push(' ');
        assert!(decode_sheet(&encoded).is_err());
        at_limit.topic_mut(root).unwrap().notes =
            Some("x".repeat(MAX_SHEET_CLIPBOARD_BYTES - base_len + 1).into());
        assert!(encode_sheet(&at_limit).is_err());
    }

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
    fn copy_excludes_hidden_anchor() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = crate::SheetRootMode::Hidden;
        let topic = sheet.add_floating("Idea", crate::Vec2::ZERO);
        assert!(copy_topics(&sheet, &[sheet.root]).is_none());
        let copied = copy_topics(&sheet, &[sheet.root, topic]).unwrap();
        assert_eq!(copied.subtrees.len(), 1);
        assert_eq!(copied.text, "Idea\n");
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
