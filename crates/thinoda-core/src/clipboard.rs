//! Clipboard payloads: internal subtrees plus a plain-text representation for the OS.

use crate::model::{Sheet, Subtree, Topic, TopicId};
use crate::outline::{OutlineNode, indented_text_lines, to_indented_text, to_outline};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

mod board;
mod topic;
pub use board::{
    BoardClipboardPayload, BoardPasteResult, MAX_BOARD_CLIPBOARD_BYTES, PreparedBoardSelection,
    ResolvedBoardPaste, copy_board_selection, decode_board_payload, encode_board_payload,
    paste_board_selection, paste_floating_subtrees, prepare_board_selection, resolve_board_paste,
};
pub use topic::{
    MAX_TOPIC_CLIPBOARD_DEPTH, MAX_TOPIC_CLIPBOARD_TOPICS, TOPIC_CLIPBOARD_PREFIX,
    decode_topic_payload, encode_topic_payload,
};

const SHEET_PREFIX: &str = "THINODA-SHEET-1\n";
const RESOURCE_SHEET_PREFIX: &str = "THINODA-SHEET-2\n";
const FORMATTED_SHEET_PREFIX: &str = "THINODA-SHEET-3\n";
const FIM_SHEET_PREFIX: &str = "THINODA-SHEET-4\n";
const INLINE_SHEET_PREFIX: &str = "THINODA-SHEET-5\n";
const WAYPOINT_SHEET_PREFIX: &str = "THINODA-SHEET-6\n";
const MARKER_SHEET_PREFIX: &str = "THINODA-SHEET-7\n";
const EXTENDED_MARKER_SHEET_PREFIX: &str = "THINODA-SHEET-8\n";
const SIZED_MARKER_SHEET_PREFIX: &str = "THINODA-SHEET-9\n";
const FILLED_MARKER_SHEET_PREFIX: &str = "THINODA-SHEET-10\n";
const DIAMOND_MARKER_SHEET_PREFIX: &str = "THINODA-SHEET-11\n";
const OVAL_MARKER_SHEET_PREFIX: &str = "THINODA-SHEET-12\n";
const HALF_CIRCLE_MARKER_SHEET_PREFIX: &str = "THINODA-SHEET-13\n";
const BAR_MARKER_SHEET_PREFIX: &str = "THINODA-SHEET-14\n";
const ROUNDING_SHEET_PREFIX: &str = "THINODA-SHEET-15\n";
const RECTANGLE_SIDES_SHEET_PREFIX: &str = "THINODA-SHEET-16\n";
const DOUBLE_ELLIPSE_SHEET_PREFIX: &str = "THINODA-SHEET-17\n";
const FIXED_PORT_SHEET_PREFIX: &str = "THINODA-SHEET-18\n";
const MAX_SHEET_CLIPBOARD_BYTES: usize = 16 * 1024 * 1024;

/// A versioned OS text payload retaining every persisted sheet field.
pub fn encode_sheet(sheet: &Sheet) -> Result<String, String> {
    sheet.validate()?;
    let prefix = if sheet.has_fixed_connector_ports() {
        FIXED_PORT_SHEET_PREFIX
    } else if sheet.has_double_ellipses() {
        DOUBLE_ELLIPSE_SHEET_PREFIX
    } else if sheet.has_rectangle_sides() {
        RECTANGLE_SIDES_SHEET_PREFIX
    } else if sheet.has_shape_rounding() {
        ROUNDING_SHEET_PREFIX
    } else if sheet.has_bar_connector_markers() {
        BAR_MARKER_SHEET_PREFIX
    } else if sheet.has_half_circle_connector_markers() {
        HALF_CIRCLE_MARKER_SHEET_PREFIX
    } else if sheet.has_oval_connector_markers() {
        OVAL_MARKER_SHEET_PREFIX
    } else if sheet.has_diamond_connector_markers() {
        DIAMOND_MARKER_SHEET_PREFIX
    } else if sheet.has_connector_marker_fills() {
        FILLED_MARKER_SHEET_PREFIX
    } else if sheet.has_connector_marker_sizes() {
        SIZED_MARKER_SHEET_PREFIX
    } else if sheet.has_extended_connector_markers() {
        EXTENDED_MARKER_SHEET_PREFIX
    } else if sheet.has_connector_markers() {
        MARKER_SHEET_PREFIX
    } else if sheet.has_connector_waypoints() {
        WAYPOINT_SHEET_PREFIX
    } else if sheet.has_inline_text() {
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
        .strip_prefix(FIXED_PORT_SHEET_PREFIX)
        .or_else(|| text.strip_prefix("THINODA-SHEET-18\r\n"))
        .map(|json| (18, json))
        .or_else(|| {
            text.strip_prefix(DOUBLE_ELLIPSE_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-17\r\n"))
                .map(|json| (17, json))
        })
        .or_else(|| {
            text.strip_prefix(RECTANGLE_SIDES_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-16\r\n"))
                .map(|json| (16, json))
        })
        .or_else(|| {
            text.strip_prefix(ROUNDING_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-15\r\n"))
                .map(|json| (15, json))
        })
        .or_else(|| {
            text.strip_prefix(BAR_MARKER_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-14\r\n"))
                .map(|json| (14, json))
        })
        .or_else(|| {
            text.strip_prefix(HALF_CIRCLE_MARKER_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-13\r\n"))
                .map(|json| (13, json))
        })
        .or_else(|| {
            text.strip_prefix(OVAL_MARKER_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-12\r\n"))
                .map(|json| (12, json))
        })
        .or_else(|| {
            text.strip_prefix(DIAMOND_MARKER_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-11\r\n"))
                .map(|json| (11, json))
        })
        .or_else(|| {
            text.strip_prefix(FILLED_MARKER_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-10\r\n"))
                .map(|json| (10, json))
        })
        .or_else(|| {
            text.strip_prefix(SIZED_MARKER_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-9\r\n"))
                .map(|json| (9, json))
        })
        .or_else(|| {
            text.strip_prefix(EXTENDED_MARKER_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-8\r\n"))
                .map(|json| (8, json))
        })
        .or_else(|| {
            text.strip_prefix(MARKER_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-7\r\n"))
                .map(|json| (7, json))
        })
        .or_else(|| {
            text.strip_prefix(WAYPOINT_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-6\r\n"))
                .map(|json| (6, json))
        })
        .or_else(|| {
            text.strip_prefix(INLINE_SHEET_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-SHEET-5\r\n"))
                .map(|json| (5, json))
        })
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
    if version < 7 && sheet.has_connector_markers() {
        return Err("connector markers require sheet clipboard version 7".into());
    }
    if version < 8 && sheet.has_extended_connector_markers() {
        return Err("classic and thin connector markers require sheet clipboard version 8".into());
    }
    if version < 9 && sheet.has_connector_marker_sizes() {
        return Err("connector marker sizes require sheet clipboard version 9".into());
    }
    if version < 10 && sheet.has_connector_marker_fills() {
        return Err("connector marker fills require sheet clipboard version 10".into());
    }
    if version < 11 && sheet.has_diamond_connector_markers() {
        return Err("diamond connector markers require sheet clipboard version 11".into());
    }
    if version < 12 && sheet.has_oval_connector_markers() {
        return Err("oval connector markers require sheet clipboard version 12".into());
    }
    if version < 13 && sheet.has_half_circle_connector_markers() {
        return Err("half-circle connector markers require sheet clipboard version 13".into());
    }
    if version < 14 && sheet.has_bar_connector_markers() {
        return Err("bar connector markers require sheet clipboard version 14".into());
    }
    if version < 15 && sheet.has_shape_rounding() {
        return Err("shape rounding requires sheet clipboard version 15".into());
    }
    if version < 16 && sheet.has_rectangle_sides() {
        return Err("rectangle outline sides require sheet clipboard version 16".into());
    }
    if version < 17 && sheet.has_double_ellipses() {
        return Err("double ellipses require sheet clipboard version 17".into());
    }
    if version < 18 && sheet.has_fixed_connector_ports() {
        return Err("fixed connector ports require sheet clipboard version 18".into());
    }
    if version < 6 && sheet.has_connector_waypoints() {
        return Err("connector waypoints require sheet clipboard version 6".into());
    }
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

/// Copies selected topics after checking the portable clipboard's source limits.
pub fn copy_topics(sheet: &Sheet, ids: &[TopicId]) -> Result<Option<ClipboardPayload>, String> {
    try_copy_topics(sheet, ids)
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
    let selected: HashSet<_> = ids.iter().copied().collect();
    ids.iter()
        .copied()
        .filter(|id| sheet.contains(*id) && !sheet.is_anchor(*id))
        .filter(|id| {
            !sheet
                .ancestors(*id)
                .iter()
                .any(|parent| parent != id && selected.contains(parent))
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

/// Builds bounded subtrees from plain text without recursive outline traversal.
pub fn subtrees_from_text(text: &str) -> Result<Vec<Subtree>, String> {
    if text.len() > MAX_BOARD_CLIPBOARD_BYTES {
        return Err("clipboard text exceeds 16 MiB".into());
    }

    let mut subtrees: Vec<Subtree> = Vec::new();
    let mut ancestors: Vec<(usize, usize)> = Vec::new();
    let mut count = 0usize;
    for (level, title) in indented_text_lines(text) {
        count += 1;
        if count > MAX_TOPIC_CLIPBOARD_TOPICS {
            return Err(format!(
                "topic clipboard exceeds {MAX_TOPIC_CLIPBOARD_TOPICS} topics"
            ));
        }
        while ancestors
            .last()
            .is_some_and(|(previous, _)| *previous >= level)
        {
            ancestors.pop();
        }
        if ancestors.len() > MAX_TOPIC_CLIPBOARD_DEPTH {
            return Err(format!(
                "topic clipboard subtree depth exceeds {MAX_TOPIC_CLIPBOARD_DEPTH}"
            ));
        }
        if let Some((_, parent_index)) = ancestors.last().copied() {
            let subtree = subtrees.last_mut().expect("an ancestor has a subtree");
            let parent = subtree.topics[parent_index].id;
            let id = TopicId(subtree.topics.len() as u64 + 1);
            let mut topic = Topic::new(id, title);
            topic.parent = Some(parent);
            subtree.topics[parent_index].children.push(id);
            subtree.topics.push(topic);
            ancestors.push((level, subtree.topics.len() - 1));
        } else {
            let root = TopicId(1);
            subtrees.push(Subtree {
                root,
                topics: vec![Topic::new(root, title)],
                tasks: Default::default(),
                dependencies: Vec::new(),
                resources: Default::default(),
            });
            ancestors.push((level, 0));
        }
    }
    Ok(subtrees)
}

/// Resolves what to paste: internal subtrees when the OS text still matches, otherwise
/// the OS text parsed as an outline. Invalid or over-limit content is rejected.
pub fn resolve_paste(
    internal: Option<&ClipboardPayload>,
    os_text: Option<&str>,
) -> Result<Vec<Subtree>, String> {
    if os_text.is_some_and(|text| text.len() > MAX_BOARD_CLIPBOARD_BYTES) {
        return Err("clipboard text exceeds 16 MiB".into());
    }
    match (internal, os_text) {
        (Some(p), Some(t)) if same_normalized(t, &p.text) => {
            if p.text.len() > MAX_BOARD_CLIPBOARD_BYTES {
                return Err("topic clipboard payload exceeds 16 MiB".into());
            }
            topic::validate_for_paste(p)?;
            Ok(p.subtrees.clone())
        }
        (_, Some(t)) if !t.trim().is_empty() => subtrees_from_text(t),
        (Some(p), None) => {
            if p.text.len() > MAX_BOARD_CLIPBOARD_BYTES {
                return Err("topic clipboard payload exceeds 16 MiB".into());
            }
            topic::validate_for_paste(p)?;
            Ok(p.subtrees.clone())
        }
        _ => Ok(vec![]),
    }
}

/// Checks a central-map paste against destination ancestry and clipboard limits.
pub fn validate_topic_destination(
    sheet: &Sheet,
    parent: TopicId,
    subtrees: &[Subtree],
) -> Result<(), String> {
    let mut summary_owners = HashMap::new();
    for summary in &sheet.summaries {
        if summary_owners
            .insert(summary.topic, summary.parent)
            .is_some()
        {
            return Err("summary topic has multiple owners".into());
        }
    }
    let mut root_depth = 1usize;
    let mut current = parent;
    let mut ancestors = HashSet::new();
    loop {
        if !ancestors.insert(current) {
            return Err("cyclic topic ownership in paste destination".into());
        }
        let topic = sheet
            .topic(current)
            .ok_or_else(|| format!("paste destination topic {current} is missing"))?;
        let Some(next) = topic
            .parent
            .or_else(|| summary_owners.get(&current).copied())
        else {
            break;
        };
        root_depth += 1;
        if root_depth > MAX_TOPIC_CLIPBOARD_DEPTH {
            return Err(format!(
                "pasted topic depth exceeds {MAX_TOPIC_CLIPBOARD_DEPTH}"
            ));
        }
        current = next;
    }

    let mut remaining = MAX_TOPIC_CLIPBOARD_TOPICS;
    for subtree in subtrees {
        remaining = remaining.checked_sub(subtree.topics.len()).ok_or_else(|| {
            format!("topic clipboard exceeds {MAX_TOPIC_CLIPBOARD_TOPICS} topics")
        })?;
        let topics: HashMap<_, _> = subtree
            .topics
            .iter()
            .map(|topic| (topic.id, topic))
            .collect();
        if topics.len() != subtree.topics.len() {
            return Err("duplicate topic clipboard ID".into());
        }
        let root = topics
            .get(&subtree.root)
            .ok_or("topic clipboard subtree root is missing")?;
        if root.card.is_some() {
            return Err("card presentation requires a floating destination".into());
        }
        let mut visited = HashSet::new();
        let mut pending = vec![(subtree.root, root_depth)];
        while let Some((id, depth)) = pending.pop() {
            if depth > MAX_TOPIC_CLIPBOARD_DEPTH {
                return Err(format!(
                    "pasted topic depth exceeds {MAX_TOPIC_CLIPBOARD_DEPTH}"
                ));
            }
            if !visited.insert(id) {
                return Err(format!("repeated topic clipboard ownership of {id}"));
            }
            let topic = topics.get(&id).ok_or("topic clipboard child is missing")?;
            for child in &topic.children {
                if topics
                    .get(child)
                    .is_none_or(|child| child.parent != Some(id))
                {
                    return Err("invalid topic clipboard child ownership".into());
                }
                pending.push((*child, depth + 1));
            }
        }
        if visited.len() != topics.len() {
            return Err("topic clipboard contains unreachable topics".into());
        }
    }
    Ok(())
}

fn same_normalized(left: &str, right: &str) -> bool {
    fn characters(text: &str) -> impl Iterator<Item = char> {
        let mut chars = text.trim_end().chars().peekable();
        std::iter::from_fn(move || {
            let ch = chars.next()?;
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
                Some('\n')
            } else {
                Some(ch)
            }
        })
    }
    characters(left).eq(characters(right))
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
    fn connector_waypoint_sheet_clipboards_require_version_six() {
        use crate::{CanvasConnector, CanvasObject, Endpoint, Vec2};

        for mut sheet in [
            Sheet::new("Plain", "Root"),
            formatted_card_sheet(crate::CardKind::Text),
        ] {
            let old = encode_sheet(&sheet).unwrap();
            assert!(!old.starts_with(WAYPOINT_SHEET_PREFIX));
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.waypoints = vec![Vec2::new(50.0, 50.0)].into();
            sheet
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let encoded = encode_sheet(&sheet).unwrap();
            assert!(encoded.starts_with(WAYPOINT_SHEET_PREFIX));
            assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
            assert_eq!(
                decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
                sheet
            );
            for version in 1..=5 {
                let forged =
                    encoded.replacen("THINODA-SHEET-6", &format!("THINODA-SHEET-{version}"), 1);
                assert!(
                    decode_sheet(&forged)
                        .unwrap_err()
                        .contains("connector waypoints require sheet clipboard version 6")
                );
            }
        }
    }

    #[test]
    fn connector_marker_sheet_clipboards_require_version_seven() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut sheet = formatted_card_sheet(crate::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        connector.waypoints = vec![Vec2::new(50.0, 30.0)].into();
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let encoded = encode_sheet(&sheet).unwrap();
        assert!(encoded.starts_with(MARKER_SHEET_PREFIX));
        assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
        assert_eq!(
            decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        for version in 1..=6 {
            let forged =
                encoded.replacen("THINODA-SHEET-7", &format!("THINODA-SHEET-{version}"), 1);
            assert!(
                decode_sheet(&forged)
                    .unwrap_err()
                    .contains("connector markers require sheet clipboard version 7")
            );
        }
    }

    #[test]
    fn classic_and_thin_marker_sheet_clipboards_require_version_eight() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        for kind in [
            ConnectorMarkerKind::Classic,
            ConnectorMarkerKind::ClassicThin,
            ConnectorMarkerKind::OpenThin,
            ConnectorMarkerKind::BlockThin,
        ] {
            let mut sheet = formatted_card_sheet(crate::CardKind::Text);
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.arrow_start = true;
            connector.start_marker = Some(ConnectorMarker { kind });
            sheet
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let encoded = encode_sheet(&sheet).unwrap();
            assert!(encoded.starts_with(EXTENDED_MARKER_SHEET_PREFIX));
            assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
            assert_eq!(
                decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
                sheet
            );
            let forged = encoded.replacen("THINODA-SHEET-8", "THINODA-SHEET-7", 1);
            assert!(
                decode_sheet(&forged).unwrap_err().contains(
                    "classic and thin connector markers require sheet clipboard version 8"
                )
            );
        }
    }

    #[test]
    fn sized_marker_sheet_clipboards_require_version_nine() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut sheet = formatted_card_sheet(crate::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        connector.end_marker_size = Some(12.5);
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let encoded = encode_sheet(&sheet).unwrap();
        assert!(encoded.starts_with(SIZED_MARKER_SHEET_PREFIX));
        assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
        assert_eq!(
            decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        let forged = encoded.replacen("THINODA-SHEET-9", "THINODA-SHEET-8", 1);
        assert!(
            decode_sheet(&forged)
                .unwrap_err()
                .contains("connector marker sizes require sheet clipboard version 9")
        );
    }

    #[test]
    fn hollow_marker_sheet_clipboards_require_version_ten() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut sheet = formatted_card_sheet(crate::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Classic,
        });
        connector.end_marker_unfilled = true;
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let encoded = encode_sheet(&sheet).unwrap();
        assert!(encoded.starts_with(FILLED_MARKER_SHEET_PREFIX));
        assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
        assert_eq!(
            decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        let forged = encoded.replacen("THINODA-SHEET-10", "THINODA-SHEET-9", 1);
        assert!(
            decode_sheet(&forged)
                .unwrap_err()
                .contains("connector marker fills require sheet clipboard version 10")
        );
    }

    #[test]
    fn diamond_marker_sheet_clipboards_require_version_eleven() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        for kind in [
            ConnectorMarkerKind::Diamond,
            ConnectorMarkerKind::DiamondThin,
        ] {
            let mut sheet = formatted_card_sheet(crate::CardKind::Text);
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.arrow_start = true;
            connector.start_marker = Some(ConnectorMarker { kind });
            connector.start_marker_size = Some(12.5);
            connector.start_marker_unfilled = true;
            sheet
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let encoded = encode_sheet(&sheet).unwrap();
            assert!(encoded.starts_with(DIAMOND_MARKER_SHEET_PREFIX));
            assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
            assert_eq!(
                decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
                sheet
            );
            let forged = encoded.replacen("THINODA-SHEET-11", "THINODA-SHEET-10", 1);
            assert!(
                decode_sheet(&forged)
                    .unwrap_err()
                    .contains("diamond connector markers require sheet clipboard version 11")
            );
        }
    }

    #[test]
    fn oval_marker_sheet_clipboards_require_version_twelve() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut sheet = formatted_card_sheet(crate::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.arrow_start = true;
        connector.start_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Diamond,
        });
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Oval,
        });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let encoded = encode_sheet(&sheet).unwrap();
        assert!(encoded.starts_with(OVAL_MARKER_SHEET_PREFIX));
        assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
        assert_eq!(
            decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        let forged = encoded.replacen("THINODA-SHEET-12", "THINODA-SHEET-11", 1);
        assert!(
            decode_sheet(&forged)
                .unwrap_err()
                .contains("oval connector markers require sheet clipboard version 12")
        );
    }

    #[test]
    fn half_circle_sheet_clipboards_require_version_thirteen() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut sheet = formatted_card_sheet(crate::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.arrow_start = true;
        connector.start_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Oval,
        });
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::HalfCircle,
        });
        connector.start_marker_size = Some(8.0);
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let encoded = encode_sheet(&sheet).unwrap();
        assert!(encoded.starts_with(HALF_CIRCLE_MARKER_SHEET_PREFIX));
        assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
        assert_eq!(
            decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        let forged = encoded.replacen("THINODA-SHEET-13", "THINODA-SHEET-12", 1);
        assert!(
            decode_sheet(&forged)
                .unwrap_err()
                .contains("half-circle connector markers require sheet clipboard version 13")
        );
        for version in 1..12 {
            let forged =
                encoded.replacen("THINODA-SHEET-13", &format!("THINODA-SHEET-{version}"), 1);
            assert!(decode_sheet(&forged).is_err(), "version {version}");
        }
    }

    #[test]
    fn bar_marker_sheet_clipboards_require_version_fourteen() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        for kind in [ConnectorMarkerKind::BaseDash, ConnectorMarkerKind::Dash] {
            for bar_at_start in [false, true] {
                let mut sheet = formatted_card_sheet(crate::CardKind::Text);
                let mut connector = CanvasConnector::new(
                    Endpoint::Point(Vec2::ZERO),
                    Endpoint::Point(Vec2::new(100.0, 0.0)),
                );
                connector.arrow_start = true;
                let bar = Some(ConnectorMarker { kind });
                let half_circle = Some(ConnectorMarker {
                    kind: ConnectorMarkerKind::HalfCircle,
                });
                (connector.start_marker, connector.end_marker) = if bar_at_start {
                    (bar, half_circle)
                } else {
                    (half_circle, bar)
                };
                connector.start_marker_size = Some(0.0);
                connector.end_marker_size = Some(12.5);
                connector.start_marker_unfilled = true;
                connector.end_marker_unfilled = true;
                sheet
                    .add_canvas_object(CanvasObject::connector(connector))
                    .unwrap();
                let encoded = encode_sheet(&sheet).unwrap();
                assert!(encoded.starts_with(BAR_MARKER_SHEET_PREFIX));
                assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
                assert_eq!(
                    decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
                    sheet
                );
                for version in 1..14 {
                    let forged = encoded.replacen(
                        "THINODA-SHEET-14",
                        &format!("THINODA-SHEET-{version}"),
                        1,
                    );
                    let error = decode_sheet(&forged).unwrap_err();
                    if version == 13 {
                        assert!(
                            error.contains(
                                "bar connector markers require sheet clipboard version 14"
                            )
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn shape_rounding_sheet_clipboard_requires_version_fifteen() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind,
            DrawioRectangleStencil, Endpoint, Rect, ShapeKind, ShapeRounding, Vec2,
        };

        let mut sheet = formatted_card_sheet(crate::CardKind::Text);
        let mut shape = CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        shape.rounding = Some(ShapeRounding {
            radius: 12.0,
            stencil: DrawioRectangleStencil::Ios,
        });
        sheet.add_canvas_object(shape).unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Dash,
        });
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let encoded = encode_sheet(&sheet).unwrap();
        assert!(encoded.starts_with(ROUNDING_SHEET_PREFIX));
        assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
        assert_eq!(
            decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        for version in 1..15 {
            let forged =
                encoded.replacen("THINODA-SHEET-15", &format!("THINODA-SHEET-{version}"), 1);
            assert!(decode_sheet(&forged).is_err(), "version {version}");
        }
    }

    #[test]
    fn rectangle_sides_sheet_clipboard_requires_version_sixteen() {
        use crate::{CanvasObject, Rect, RectangleSides, ShapeKind, Vec2};

        let mut sheet = formatted_card_sheet(crate::CardKind::Text);
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        shape.rectangle_sides = Some(RectangleSides {
            top: true,
            right: false,
            bottom: false,
            left: true,
        });
        sheet.add_canvas_object(shape).unwrap();
        let encoded = encode_sheet(&sheet).unwrap();
        assert!(encoded.starts_with(RECTANGLE_SIDES_SHEET_PREFIX));
        assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
        assert_eq!(
            decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        for version in 1..16 {
            let forged =
                encoded.replacen("THINODA-SHEET-16", &format!("THINODA-SHEET-{version}"), 1);
            assert!(decode_sheet(&forged).is_err(), "version {version}");
        }
    }

    #[test]
    fn fixed_connector_port_sheet_clipboard_requires_version_eighteen() {
        use crate::{
            CanvasConnector, CanvasObject, ConnectorPort, Endpoint, Rect, ShapeKind, Vec2,
        };

        let mut sheet = formatted_card_sheet(crate::CardKind::Text);
        let shape = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
            ))
            .unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Point(Vec2::new(200.0, 0.0)),
        );
        connector.from_port = Some(ConnectorPort {
            position: Vec2::new(1.0, 0.25),
            offset: Vec2::new(5.0, 0.0),
            perimeter: false,
        });
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let encoded = encode_sheet(&sheet).unwrap();
        assert!(encoded.starts_with(FIXED_PORT_SHEET_PREFIX));
        assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
        assert_eq!(
            decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            sheet
        );
        for version in 1..18 {
            let forged =
                encoded.replacen("THINODA-SHEET-18", &format!("THINODA-SHEET-{version}"), 1);
            assert!(decode_sheet(&forged).is_err(), "version {version}");
        }
    }

    #[test]
    fn double_ellipse_sheet_clipboard_requires_version_seventeen() {
        use crate::{CanvasObject, Rect, RectangleSides, ShapeKind, Vec2};

        for margin in [None, Some(8.5)] {
            let mut sheet = formatted_card_sheet(crate::CardKind::Text);
            let mut ellipse = CanvasObject::shape(
                ShapeKind::DoubleEllipse,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
            );
            ellipse.double_ellipse_margin = margin;
            sheet.add_canvas_object(ellipse).unwrap();
            let mut rectangle = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(120.0, 0.0), Vec2::new(100.0, 60.0)),
            );
            rectangle.rectangle_sides = Some(RectangleSides::default());
            sheet.add_canvas_object(rectangle).unwrap();
            let encoded = encode_sheet(&sheet).unwrap();
            assert!(encoded.starts_with(DOUBLE_ELLIPSE_SHEET_PREFIX));
            assert_eq!(decode_sheet(&encoded).unwrap(), sheet);
            assert_eq!(
                decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
                sheet
            );
            for version in 1..17 {
                let forged =
                    encoded.replacen("THINODA-SHEET-17", &format!("THINODA-SHEET-{version}"), 1);
                assert!(decode_sheet(&forged).is_err(), "version {version}");
            }
            for value in [serde_json::json!(-1), serde_json::json!("invalid")] {
                let mut malformed = serde_json::to_value(&sheet).unwrap();
                malformed["canvas"]["objects"][0]["double_ellipse_margin"] = value;
                let forged = format!("{DOUBLE_ELLIPSE_SHEET_PREFIX}{malformed}");
                assert!(decode_sheet(&forged).is_err());
            }
        }
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
        let p = copy_topics(&s, &[a, a1]).unwrap().unwrap();
        assert_eq!(p.subtrees.len(), 1);
        assert_eq!(p.text, "A\n\tA1\n");
    }

    #[test]
    fn copy_excludes_hidden_anchor() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = crate::SheetRootMode::Hidden;
        let topic = sheet.add_floating("Idea", crate::Vec2::ZERO);
        assert!(copy_topics(&sheet, &[sheet.root]).unwrap().is_none());
        let copied = copy_topics(&sheet, &[sheet.root, topic]).unwrap().unwrap();
        assert_eq!(copied.subtrees.len(), 1);
        assert_eq!(copied.text, "Idea\n");
    }

    #[test]
    fn paste_prefers_internal_when_text_matches() {
        let mut s = Sheet::new("S", "R");
        let a = s.add_child(s.root, None, "A");
        s.topic_mut(a).unwrap().markers.push("flag-red".into());
        let p = copy_topics(&s, &[a]).unwrap().unwrap();
        let subs = resolve_paste(Some(&p), Some("A\r\n")).unwrap();
        assert_eq!(subs[0].root_topic().markers, vec!["flag-red"]);
        let subs = resolve_paste(Some(&p), Some("X\n\tY")).unwrap();
        assert_eq!(subs[0].root_topic().title, "X");
        assert_eq!(subs[0].len(), 2);
        assert!(resolve_paste(None, Some("  ")).unwrap().is_empty());
    }

    #[test]
    fn normalized_comparison_matches_legacy_line_endings_and_whitespace() {
        let examples = [
            "",
            " ",
            "\r\n",
            "a\r\nb",
            "a\nb",
            "a\rb",
            "a\r\n",
            "a\n ",
            "é\u{2003}",
            "é\r\n\u{2003}",
            "\r\r\n",
            "\n\r",
        ];
        for left in examples {
            for right in examples {
                let old_left = left.replace("\r\n", "\n");
                let old_right = right.replace("\r\n", "\n");
                assert_eq!(
                    same_normalized(left, right),
                    old_left.trim_end() == old_right.trim_end(),
                    "{left:?} vs {right:?}"
                );
            }
        }
    }

    #[test]
    fn pasted_text_creates_topics() {
        let mut s = Sheet::new("S", "R");
        let subs = subtrees_from_text("One\nTwo\n\tTwo.1").unwrap();
        let root = s.root;
        let ids = s.paste(root, &subs).unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(s.children(ids[1]).len(), 1);
        s.validate().unwrap();
    }
}
