//! Tolerant parser that turns model output (Markdown headings / bullet lists) into a topic tree.

/// One topic suggested by the model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutlineNode {
    pub title: String,
    pub children: Vec<OutlineNode>,
}

impl OutlineNode {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            children: Vec::new(),
        }
    }

    /// Number of topics in this subtree (including itself).
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Self::count).sum::<usize>()
    }
}

/// A parsed outline: an optional `# Title` plus the list of top-level items.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outline {
    pub title: Option<String>,
    pub items: Vec<OutlineNode>,
}

impl Outline {
    /// Normalizes a whole-map answer: if there is no `# Title` but exactly one top-level
    /// item, that item becomes the central topic.
    pub fn into_map(mut self) -> Outline {
        if self.title.is_none() && self.items.len() == 1 {
            let root = self.items.remove(0);
            return Outline {
                title: Some(root.title),
                items: root.children,
            };
        }
        self
    }
}

/// Removes reasoning blocks (`<think>…</think>`) that some local models emit.
pub fn strip_reasoning(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        match rest.find("<think>") {
            Some(start) => {
                out.push_str(&rest[..start]);
                match rest[start..].find("</think>") {
                    Some(end) => rest = &rest[start + end + "</think>".len()..],
                    None => break, // unterminated: drop the remainder
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out.trim().to_string()
}

/// Strips Markdown emphasis, checkboxes, quotes and trailing colons from an item title.
pub fn clean_title(s: &str) -> String {
    let mut t = s.trim();
    for prefix in ["[ ] ", "[x] ", "[X] "] {
        if let Some(r) = t.strip_prefix(prefix) {
            t = r;
        }
    }
    let mut t = t.replace("**", "").replace("__", "").replace('`', "");
    let trimmed = t.trim().trim_end_matches(':').trim();
    t = trimmed.to_string();
    for (a, b) in [('"', '"'), ('“', '”'), ('\'', '\'')] {
        if t.len() >= 2 && t.starts_with(a) && t.ends_with(b) {
            t = t[a.len_utf8()..t.len() - b.len_utf8()].trim().to_string();
        }
    }
    t
}

/// Returns (indent width, item text) for a bullet or numbered list line.
fn bullet(line: &str) -> Option<(usize, &str)> {
    let width: usize = line
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum();
    let s = line.trim_start();
    for marker in ["- ", "* ", "+ ", "• ", "– "] {
        if let Some(r) = s.strip_prefix(marker) {
            return Some((width, r));
        }
    }
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits < 4 {
        let r = &s[digits..];
        if let Some(r) = r.strip_prefix(". ").or_else(|| r.strip_prefix(") ")) {
            return Some((width, r));
        }
    }
    None
}

fn push_at(items: &mut Vec<OutlineNode>, depth: usize, node: OutlineNode) {
    let mut level = items;
    for _ in 0..depth {
        if level.is_empty() {
            break;
        }
        level = &mut level.last_mut().unwrap().children;
    }
    level.push(node);
}

/// Parses Markdown-ish model output. Headings (`#`, `##`, …) and bullets (`-`, `*`, `1.`)
/// are both understood; indentation of any width defines nesting; prose lines are ignored.
pub fn parse_outline(text: &str) -> Outline {
    let text = strip_reasoning(text);
    let mut out = Outline::default();
    // Depth of the most recent heading item (bullets nest below it).
    let mut heading_depth: Option<usize> = None;
    // Indent widths of the currently open bullet levels.
    let mut stack: Vec<usize> = Vec::new();
    let mut last_depth: Option<usize> = None;
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with("```") {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|c| *c == '#').count();
            let title = clean_title(trimmed[level..].trim());
            if title.is_empty() {
                continue;
            }
            stack.clear();
            if level == 1 && out.title.is_none() && out.items.is_empty() {
                out.title = Some(title);
                heading_depth = None;
                last_depth = None;
                continue;
            }
            let depth = level.saturating_sub(2).min(last_depth.map_or(0, |d| d + 1));
            push_at(&mut out.items, depth, OutlineNode::new(title));
            heading_depth = Some(depth);
            last_depth = Some(depth);
            continue;
        }
        let Some((width, text)) = bullet(line) else {
            continue;
        };
        let title = clean_title(text);
        if title.is_empty() {
            continue;
        }
        while stack.last().is_some_and(|&top| width <= top) {
            stack.pop();
        }
        let level = stack.len();
        stack.push(width);
        let base = heading_depth.map_or(0, |d| d + 1);
        let depth = (base + level).min(last_depth.map_or(0, |d| d + 1));
        push_at(&mut out.items, depth, OutlineNode::new(title));
        last_depth = Some(depth);
    }
    out
}

/// Renders a topic tree as an indented Markdown list (used to give the model context).
pub fn to_markdown(nodes: &[OutlineNode]) -> String {
    fn walk(out: &mut String, nodes: &[OutlineNode], depth: usize) {
        for n in nodes {
            out.push_str(&"  ".repeat(depth));
            out.push_str("- ");
            out.push_str(n.title.trim());
            out.push('\n');
            walk(out, &n.children, depth + 1);
        }
    }
    let mut s = String::new();
    walk(&mut s, nodes, 0);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(v: &[OutlineNode]) -> Vec<&str> {
        v.iter().map(|n| n.title.as_str()).collect()
    }

    #[test]
    fn flat_bullets_and_numbers() {
        let o = parse_outline("Here are ideas:\n- One\n* **Two**\n3. Three:\n4) `Four`\n");
        assert_eq!(o.title, None);
        assert_eq!(titles(&o.items), ["One", "Two", "Three", "Four"]);
    }

    #[test]
    fn nested_bullets_any_indent() {
        let o = parse_outline("- A\n    - A1\n        - A1a\n    - A2\n- B\n\t- B1\n");
        assert_eq!(titles(&o.items), ["A", "B"]);
        assert_eq!(titles(&o.items[0].children), ["A1", "A2"]);
        assert_eq!(titles(&o.items[0].children[0].children), ["A1a"]);
        assert_eq!(titles(&o.items[1].children), ["B1"]);
    }

    #[test]
    fn headings_and_bullets_mixed() {
        let md = "<think>plan the map</think>\n```markdown\n# Trip to Japan\n## Planning\n- Budget\n- Visa\n## Places\n- Tokyo\n  - Shibuya\n```";
        let o = parse_outline(md);
        assert_eq!(o.title.as_deref(), Some("Trip to Japan"));
        assert_eq!(titles(&o.items), ["Planning", "Places"]);
        assert_eq!(titles(&o.items[0].children), ["Budget", "Visa"]);
        assert_eq!(titles(&o.items[1].children[0].children), ["Shibuya"]);
    }

    #[test]
    fn over_indented_first_item_is_clamped() {
        let o = parse_outline("    - Deep\n- Top\n");
        assert_eq!(titles(&o.items), ["Deep", "Top"]);
    }

    #[test]
    fn single_root_becomes_map_title() {
        let o = parse_outline("- Root\n  - A\n  - B\n").into_map();
        assert_eq!(o.title.as_deref(), Some("Root"));
        assert_eq!(titles(&o.items), ["A", "B"]);
    }

    #[test]
    fn strip_reasoning_handles_unterminated() {
        assert_eq!(strip_reasoning("a<think>x</think>b"), "ab");
        assert_eq!(strip_reasoning("a<think>never ends"), "a");
    }

    #[test]
    fn clean_title_variants() {
        assert_eq!(clean_title("[ ] Buy milk"), "Buy milk");
        assert_eq!(clean_title("\"Quoted\""), "Quoted");
        assert_eq!(clean_title("**Bold**:"), "Bold");
    }

    #[test]
    fn markdown_roundtrip() {
        let nodes = vec![
            OutlineNode {
                title: "A".into(),
                children: vec![OutlineNode::new("A1")],
            },
            OutlineNode::new("B"),
        ];
        let md = to_markdown(&nodes);
        assert_eq!(parse_outline(&md).items, nodes);
        assert_eq!(nodes[0].count(), 2);
    }
}
