//! Clipboard payloads: internal subtrees plus a plain-text representation for the OS.

use crate::model::{Sheet, Subtree, TopicId};
use crate::outline::{OutlineNode, parse_indented_text, to_indented_text, to_outline};
use serde::{Deserialize, Serialize};

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
