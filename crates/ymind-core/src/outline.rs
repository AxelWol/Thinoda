//! Outline representation (Outliner view, Markdown/OPML/text import & export).

use crate::model::{Sheet, Topic, TopicId};
use serde::{Deserialize, Serialize};

/// Simple tree used for interchange formats.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OutlineNode {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<OutlineNode>,
}

impl OutlineNode {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    pub fn with_children(mut self, children: Vec<OutlineNode>) -> Self {
        self.children = children;
        self
    }

    pub fn count(&self) -> usize {
        1 + self.children.iter().map(|c| c.count()).sum::<usize>()
    }
}

/// Converts the subtree of `id` into an outline.
pub fn to_outline(sheet: &Sheet, id: TopicId) -> OutlineNode {
    let t = &sheet.topics[&id];
    OutlineNode {
        title: t.title.clone(),
        notes: t.notes.clone(),
        link: t.hyperlink.clone(),
        labels: t.labels.clone(),
        markers: t.markers.clone(),
        children: t.children.iter().map(|c| to_outline(sheet, *c)).collect(),
    }
}

/// Builds a new sheet from an outline (root becomes the central topic).
pub fn sheet_from_outline(title: &str, root: &OutlineNode) -> Sheet {
    let mut s = Sheet::new(title, root.title.clone());
    let r = s.root;
    apply_fields(s.topic_mut(r).unwrap(), root);
    for c in &root.children {
        add_outline(&mut s, r, c);
    }
    s
}

fn apply_fields(t: &mut Topic, n: &OutlineNode) {
    t.notes = n.notes.clone();
    t.hyperlink = n.link.clone();
    t.labels = n.labels.clone();
    t.markers = n.markers.clone();
}

/// Appends an outline as a child of `parent`; returns the new topic id.
pub fn add_outline(sheet: &mut Sheet, parent: TopicId, node: &OutlineNode) -> TopicId {
    let id = sheet.add_child(parent, None, node.title.clone());
    apply_fields(sheet.topic_mut(id).unwrap(), node);
    for c in &node.children {
        add_outline(sheet, id, c);
    }
    id
}

/// Renders an outline as indented plain text (tab per level).
pub fn to_indented_text(nodes: &[OutlineNode]) -> String {
    fn rec(n: &OutlineNode, depth: usize, out: &mut String) {
        for _ in 0..depth {
            out.push('\t');
        }
        out.push_str(&n.title.replace('\n', " "));
        out.push('\n');
        for c in &n.children {
            rec(c, depth + 1, out);
        }
    }
    let mut out = String::new();
    for n in nodes {
        rec(n, 0, &mut out);
    }
    out
}

/// Parses indented text (tabs, spaces, bullets `-`, `*`, `+`, `•`, numbered `1.`) into
/// outline nodes. Indentation width is detected from the first indented line.
pub fn parse_indented_text(text: &str) -> Vec<OutlineNode> {
    let mut items: Vec<(usize, String)> = Vec::new();
    let mut unit: Option<usize> = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut width = 0;
        for c in line.chars() {
            match c {
                '\t' => width += 4,
                ' ' => width += 1,
                _ => break,
            }
        }
        if width > 0 && unit.is_none() {
            unit = Some(width);
        }
        let mut title = line.trim();
        for b in ["- [ ] ", "- [x] ", "- ", "* ", "+ ", "• ", "– "] {
            if let Some(r) = title.strip_prefix(b) {
                title = r;
                break;
            }
        }
        if let Some(pos) = title.find(". ")
            && pos > 0
            && pos <= 3
            && title[..pos].chars().all(|c| c.is_ascii_digit())
        {
            title = &title[pos + 2..];
        }
        items.push((width, title.trim().to_string()));
    }
    let unit = unit.unwrap_or(4).max(1);
    let levels: Vec<(usize, String)> = items
        .into_iter()
        .map(|(w, t)| (w.div_ceil(unit), t))
        .collect();
    build_tree(&levels)
}

/// Builds a tree from (level, title) pairs. Level jumps are clamped.
pub fn build_tree(items: &[(usize, String)]) -> Vec<OutlineNode> {
    fn take(items: &[(usize, String)], pos: &mut usize, level: usize) -> Vec<OutlineNode> {
        let mut out = Vec::new();
        while *pos < items.len() {
            let (l, title) = &items[*pos];
            if *l < level {
                break;
            }
            *pos += 1;
            let mut n = OutlineNode::new(title.clone());
            if *pos < items.len() && items[*pos].0 > *l {
                n.children = take(items, pos, l + 1);
            }
            out.push(n);
        }
        out
    }
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < items.len() {
        let lvl = items[pos].0;
        out.extend(take(items, &mut pos, lvl));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indented_roundtrip() {
        let text = "Root\n\tA\n\t\tA1\n\tB\n";
        let nodes = parse_indented_text(text);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].count(), 4);
        assert_eq!(to_indented_text(&nodes), text);
    }

    #[test]
    fn bullets_and_spaces() {
        let text = "- Root\n  - A\n    - A1\n  - B\n1. Second";
        let nodes = parse_indented_text(text);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].children[0].children[0].title, "A1");
        assert_eq!(nodes[1].title, "Second");
    }

    #[test]
    fn level_jumps_are_tolerated() {
        let nodes = build_tree(&[(0, "a".into()), (3, "b".into()), (1, "c".into())]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].children.len(), 2);
    }

    #[test]
    fn sheet_roundtrip() {
        let mut root = OutlineNode::new("R");
        root.children = vec![
            OutlineNode {
                notes: Some("n".into()),
                ..OutlineNode::new("A")
            },
            OutlineNode::new("B"),
        ];
        let s = sheet_from_outline("S", &root);
        s.validate().unwrap();
        assert_eq!(to_outline(&s, s.root), root);
    }
}
