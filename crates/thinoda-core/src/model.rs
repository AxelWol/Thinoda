//! Workbook / sheet / topic data model.
//!
//! Topics are stored in an arena (`BTreeMap<TopicId, Topic>`) per sheet. Each topic knows
//! its parent and the ordered list of its children. Concepts and identifiers mirror XMind so
//! that `.xmind` files can be imported and exported with high fidelity.

use crate::geometry::{Color, Vec2};
use crate::theme::ThemeSettings;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_type!(TopicId);
id_type!(RelationshipId);
id_type!(BoundaryId);
id_type!(SummaryId);

/// Layout structure of a map or of a subtree (mixed structures are supported).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Structure {
    #[default]
    MapClockwise,
    MapAnticlockwise,
    MapUnbalanced,
    LogicRight,
    LogicLeft,
    BraceRight,
    BraceLeft,
    OrgDown,
    OrgUp,
    TreeRight,
    TreeLeft,
    TimelineHorizontal,
    TimelineVertical,
    FishboneRight,
    FishboneLeft,
    Matrix,
    TreeTable,
}

impl Structure {
    pub const ALL: [Structure; 17] = [
        Structure::MapClockwise,
        Structure::MapAnticlockwise,
        Structure::MapUnbalanced,
        Structure::LogicRight,
        Structure::LogicLeft,
        Structure::BraceRight,
        Structure::BraceLeft,
        Structure::OrgDown,
        Structure::OrgUp,
        Structure::TreeRight,
        Structure::TreeLeft,
        Structure::TimelineHorizontal,
        Structure::TimelineVertical,
        Structure::FishboneRight,
        Structure::FishboneLeft,
        Structure::Matrix,
        Structure::TreeTable,
    ];

    /// XMind `structureClass` identifier.
    pub fn xmind_class(self) -> &'static str {
        match self {
            Structure::MapClockwise => "org.xmind.ui.map.clockwise",
            Structure::MapAnticlockwise => "org.xmind.ui.map.anticlockwise",
            Structure::MapUnbalanced => "org.xmind.ui.map.unbalanced",
            Structure::LogicRight => "org.xmind.ui.logic.right",
            Structure::LogicLeft => "org.xmind.ui.logic.left",
            Structure::BraceRight => "org.xmind.ui.brace.right",
            Structure::BraceLeft => "org.xmind.ui.brace.left",
            Structure::OrgDown => "org.xmind.ui.org-chart.down",
            Structure::OrgUp => "org.xmind.ui.org-chart.up",
            Structure::TreeRight => "org.xmind.ui.tree.right",
            Structure::TreeLeft => "org.xmind.ui.tree.left",
            Structure::TimelineHorizontal => "org.xmind.ui.timeline.horizontal",
            Structure::TimelineVertical => "org.xmind.ui.timeline.vertical",
            Structure::FishboneRight => "org.xmind.ui.fishbone.rightHeaded",
            Structure::FishboneLeft => "org.xmind.ui.fishbone.leftHeaded",
            Structure::Matrix => "org.xmind.ui.spreadsheet",
            Structure::TreeTable => "org.xmind.ui.treetable",
        }
    }

    /// Parses an XMind `structureClass` (lenient: variants map to the closest structure).
    pub fn from_xmind_class(class: &str) -> Option<Structure> {
        let c = class.trim();
        let s = match c {
            _ if c.starts_with("org.xmind.ui.map.anticlockwise")
                || c == "org.xmind.ui.map.floating.anticlockwise" =>
            {
                Structure::MapAnticlockwise
            }
            _ if c.starts_with("org.xmind.ui.map.unbalanced") => Structure::MapUnbalanced,
            _ if c.starts_with("org.xmind.ui.map") => Structure::MapClockwise,
            _ if c.starts_with("org.xmind.ui.logic.left")
                || c.starts_with("org.xmind.ui.logic-chart.left") =>
            {
                Structure::LogicLeft
            }
            _ if c.starts_with("org.xmind.ui.logic") => Structure::LogicRight,
            _ if c.starts_with("org.xmind.ui.brace.left") => Structure::BraceLeft,
            _ if c.starts_with("org.xmind.ui.brace") => Structure::BraceRight,
            _ if c.starts_with("org.xmind.ui.org-chart.up") => Structure::OrgUp,
            _ if c.starts_with("org.xmind.ui.org-chart") => Structure::OrgDown,
            _ if c.starts_with("org.xmind.ui.treetable") => Structure::TreeTable,
            _ if c.starts_with("org.xmind.ui.tree.left")
                || c.starts_with("org.xmind.ui.tree.branch-aligned.left") =>
            {
                Structure::TreeLeft
            }
            _ if c.starts_with("org.xmind.ui.tree") => Structure::TreeRight,
            _ if c.starts_with("org.xmind.ui.timeline.horizontal")
                || c.starts_with("org.xmind.ui.timeline.sided.horizontal") =>
            {
                Structure::TimelineHorizontal
            }
            _ if c.starts_with("org.xmind.ui.timeline") => Structure::TimelineVertical,
            _ if c.starts_with("org.xmind.ui.fishbone.left") => Structure::FishboneLeft,
            _ if c.starts_with("org.xmind.ui.fishbone") => Structure::FishboneRight,
            _ if c.starts_with("org.xmind.ui.spreadsheet")
                || c.starts_with("org.xmind.ui.structure.column.spreadsheet")
                || c.starts_with("org.xmind.ui.structure.spreadsheet") =>
            {
                Structure::Matrix
            }
            _ => return None,
        };
        Some(s)
    }

    /// English display name (UI translates via its own string table).
    pub fn label(self) -> &'static str {
        match self {
            Structure::MapClockwise => "Mind Map (clockwise)",
            Structure::MapAnticlockwise => "Mind Map (anticlockwise)",
            Structure::MapUnbalanced => "Mind Map (unbalanced)",
            Structure::LogicRight => "Logic Chart (right)",
            Structure::LogicLeft => "Logic Chart (left)",
            Structure::BraceRight => "Brace Map (right)",
            Structure::BraceLeft => "Brace Map (left)",
            Structure::OrgDown => "Org Chart (down)",
            Structure::OrgUp => "Org Chart (up)",
            Structure::TreeRight => "Tree Chart (right)",
            Structure::TreeLeft => "Tree Chart (left)",
            Structure::TimelineHorizontal => "Timeline (horizontal)",
            Structure::TimelineVertical => "Timeline (vertical)",
            Structure::FishboneRight => "Fishbone (right headed)",
            Structure::FishboneLeft => "Fishbone (left headed)",
            Structure::Matrix => "Matrix",
            Structure::TreeTable => "Tree Table",
        }
    }

    pub fn is_map(self) -> bool {
        matches!(
            self,
            Structure::MapClockwise | Structure::MapAnticlockwise | Structure::MapUnbalanced
        )
    }
}

/// Shape of a topic box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TopicShape {
    #[default]
    RoundedRect,
    Rect,
    Ellipse,
    Capsule,
    Diamond,
    Underline,
    None,
}

impl TopicShape {
    pub const ALL: [TopicShape; 7] = [
        TopicShape::RoundedRect,
        TopicShape::Rect,
        TopicShape::Ellipse,
        TopicShape::Capsule,
        TopicShape::Diamond,
        TopicShape::Underline,
        TopicShape::None,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TopicShape::RoundedRect => "Rounded rectangle",
            TopicShape::Rect => "Rectangle",
            TopicShape::Ellipse => "Ellipse",
            TopicShape::Capsule => "Capsule",
            TopicShape::Diamond => "Diamond",
            TopicShape::Underline => "Underline",
            TopicShape::None => "None",
        }
    }
}

/// Shape of branch connector lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineShape {
    #[default]
    Curve,
    Straight,
    Elbow,
    RoundedElbow,
}

impl LineShape {
    pub const ALL: [LineShape; 4] = [
        LineShape::Curve,
        LineShape::Straight,
        LineShape::Elbow,
        LineShape::RoundedElbow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LineShape::Curve => "Curve",
            LineShape::Straight => "Straight",
            LineShape::Elbow => "Elbow",
            LineShape::RoundedElbow => "Rounded elbow",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// Per-topic style overrides. `None` means "use the theme".
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TopicStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<TopicShape>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_shape: Option<LineShape>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_width: Option<f32>,
    /// Fixed topic width ("Length" in XMind's Style panel).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
}

impl TopicStyle {
    pub fn is_default(&self) -> bool {
        *self == TopicStyle::default()
    }
}

/// Explicit side of a branch attached to a split-map parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BranchSide {
    Left,
    Right,
}

/// A topic (node) of the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Topic {
    pub id: TopicId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<TopicId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TopicId>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// XMind compatible marker ids, e.g. `priority-1`, `task-half`, `flag-red`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<String>,
    /// Explicit colors indexed by marker position; absent entries retain legacy rendering.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub marker_colors: std::collections::BTreeMap<usize, Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<String>,
    #[serde(default, skip_serializing_if = "TopicStyle::is_default")]
    pub style: TopicStyle,
    /// Center position for floating topics (sheet coordinates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Vec2>,
    /// Structure override for this subtree (mixed structures).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<Structure>,
    /// Explicit attachment side when the parent uses a split-map structure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_side: Option<BranchSide>,
    /// To-Do checkbox state (`None` = no checkbox).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo: Option<bool>,
    /// Image reference (resource path inside the file or URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

impl Topic {
    pub fn new(id: TopicId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            parent: None,
            children: Vec::new(),
            collapsed: false,
            notes: None,
            labels: Vec::new(),
            markers: Vec::new(),
            marker_colors: Default::default(),
            hyperlink: None,
            style: TopicStyle::default(),
            position: None,
            structure: None,
            branch_side: None,
            todo: None,
            image: None,
        }
    }

    pub fn has_notes(&self) -> bool {
        self.notes.as_deref().is_some_and(|n| !n.trim().is_empty())
    }
}

/// Free connection line between two topics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relationship {
    pub id: RelationshipId,
    pub from: TopicId,
    pub to: TopicId,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// Cubic controls relative to the from/to topic centers; None uses automatic routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_points: Option<[Vec2; 2]>,
}

impl Relationship {
    /// Bounds each finite offset to one million map units to keep curve arithmetic usable.
    pub fn valid_control_points(points: Option<[Vec2; 2]>) -> bool {
        points.is_none_or(|points| {
            points.iter().all(|p| {
                p.x.is_finite()
                    && p.y.is_finite()
                    && p.x.abs() <= 1_000_000.0
                    && p.y.abs() <= 1_000_000.0
            })
        })
    }
}

/// Inclusive range of child indices of a parent topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildRange {
    pub start: usize,
    pub end: usize,
}

impl ChildRange {
    pub fn contains(&self, i: usize) -> bool {
        i >= self.start && i <= self.end
    }
}

/// Outline around a range of sibling topics, or around a whole topic (`range == None`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Boundary {
    pub id: BoundaryId,
    pub parent: TopicId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<ChildRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

/// Bracket summarising a range of sibling topics into a summary topic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub id: SummaryId,
    pub parent: TopicId,
    pub range: ChildRange,
    /// Detached topic holding the summary text (may have its own children).
    pub topic: TopicId,
}

/// A detached piece of a tree (used for clipboard, undo-free moves and import).
/// `topics[0]` is the subtree root; the remaining entries are its descendants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subtree {
    pub root: TopicId,
    pub topics: Vec<Topic>,
}

impl Subtree {
    /// Gives every topic in the subtree a fresh id from `sheet`.
    pub fn reassign_ids(&mut self, sheet: &mut Sheet) {
        let map: BTreeMap<TopicId, TopicId> = self
            .topics
            .iter()
            .map(|t| (t.id, sheet.alloc_topic_id()))
            .collect();
        let m = |id: TopicId| *map.get(&id).unwrap_or(&id);
        for t in &mut self.topics {
            t.id = m(t.id);
            t.parent = t.parent.map(m);
            for c in &mut t.children {
                *c = m(*c);
            }
        }
        self.root = m(self.root);
    }

    pub fn root_topic(&self) -> &Topic {
        self.topics
            .iter()
            .find(|t| t.id == self.root)
            .expect("subtree root present")
    }

    pub fn len(&self) -> usize {
        self.topics.len()
    }

    pub fn is_empty(&self) -> bool {
        self.topics.is_empty()
    }
}

/// One canvas ("tab") of a workbook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    pub title: String,
    pub root: TopicId,
    pub topics: BTreeMap<TopicId, Topic>,
    #[serde(default)]
    pub floating: Vec<TopicId>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    #[serde(default)]
    pub boundaries: Vec<Boundary>,
    #[serde(default)]
    pub summaries: Vec<Summary>,
    #[serde(default)]
    pub structure: Structure,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    next_id: u64,
}

impl Sheet {
    /// Creates a sheet containing only a central topic.
    pub fn new(title: impl Into<String>, root_title: impl Into<String>) -> Self {
        let root = TopicId(1);
        let mut topics = BTreeMap::new();
        topics.insert(root, Topic::new(root, root_title));
        Self {
            title: title.into(),
            root,
            topics,
            floating: Vec::new(),
            relationships: Vec::new(),
            boundaries: Vec::new(),
            summaries: Vec::new(),
            structure: Structure::default(),
            theme: ThemeSettings::default(),
            next_id: 2,
        }
    }

    fn bump_next_id(&mut self) {
        let max = self
            .topics
            .keys()
            .map(|t| t.0)
            .chain(self.relationships.iter().map(|r| r.id.0))
            .chain(self.boundaries.iter().map(|b| b.id.0))
            .chain(self.summaries.iter().map(|s| s.id.0))
            .max()
            .unwrap_or(0);
        if self.next_id <= max {
            self.next_id = max + 1;
        }
    }

    fn alloc(&mut self) -> u64 {
        self.bump_next_id();
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn alloc_topic_id(&mut self) -> TopicId {
        TopicId(self.alloc())
    }

    pub fn alloc_relationship_id(&mut self) -> RelationshipId {
        RelationshipId(self.alloc())
    }

    pub fn alloc_boundary_id(&mut self) -> BoundaryId {
        BoundaryId(self.alloc())
    }

    pub fn alloc_summary_id(&mut self) -> SummaryId {
        SummaryId(self.alloc())
    }

    pub fn topic(&self, id: TopicId) -> Option<&Topic> {
        self.topics.get(&id)
    }

    pub fn topic_mut(&mut self, id: TopicId) -> Option<&mut Topic> {
        self.topics.get_mut(&id)
    }

    pub fn contains(&self, id: TopicId) -> bool {
        self.topics.contains_key(&id)
    }

    pub fn root_topic(&self) -> &Topic {
        &self.topics[&self.root]
    }

    pub fn children(&self, id: TopicId) -> &[TopicId] {
        self.topics
            .get(&id)
            .map(|t| t.children.as_slice())
            .unwrap_or(&[])
    }

    pub fn parent(&self, id: TopicId) -> Option<TopicId> {
        self.topics.get(&id).and_then(|t| t.parent)
    }

    pub fn topic_count(&self) -> usize {
        self.topics.len()
    }

    pub fn index_in_parent(&self, id: TopicId) -> Option<usize> {
        let p = self.parent(id)?;
        self.children(p).iter().position(|c| *c == id)
    }

    /// Ancestors from the direct parent up to the top-level topic.
    pub fn ancestors(&self, id: TopicId) -> Vec<TopicId> {
        let mut out = Vec::new();
        let mut cur = self.parent(id);
        while let Some(p) = cur {
            if out.contains(&p) {
                break; // defensive against corrupt cycles
            }
            out.push(p);
            cur = self.parent(p);
        }
        out
    }

    /// `true` if `ancestor` is `id` or one of its ancestors.
    pub fn is_ancestor_or_self(&self, ancestor: TopicId, id: TopicId) -> bool {
        ancestor == id || self.ancestors(id).contains(&ancestor)
    }

    /// Depth below the top-level topic of the tree (root = 0).
    pub fn depth(&self, id: TopicId) -> usize {
        self.ancestors(id).len()
    }

    /// Top level topic (root, floating topic or summary topic) owning `id`.
    pub fn top_of(&self, id: TopicId) -> TopicId {
        self.ancestors(id).last().copied().unwrap_or(id)
    }

    pub fn is_floating(&self, id: TopicId) -> bool {
        self.floating.contains(&id)
    }

    pub fn summary_for_topic(&self, id: TopicId) -> Option<&Summary> {
        self.summaries.iter().find(|s| s.topic == id)
    }

    /// Main branch (child of the central topic) that contains `id`.
    pub fn main_branch_of(&self, id: TopicId) -> Option<TopicId> {
        let mut chain = vec![id];
        chain.extend(self.ancestors(id));
        let pos = chain.iter().position(|t| *t == self.root)?;
        if pos == 0 { None } else { Some(chain[pos - 1]) }
    }

    /// Preorder list of `id` and all descendants.
    pub fn subtree_ids(&self, id: TopicId) -> Vec<TopicId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(t) = stack.pop() {
            if !self.contains(t) || out.contains(&t) {
                continue;
            }
            out.push(t);
            for c in self.children(t).iter().rev() {
                stack.push(*c);
            }
        }
        out
    }

    /// Topics not hidden by a collapsed ancestor.
    pub fn is_visible(&self, id: TopicId) -> bool {
        self.ancestors(id).iter().all(|a| !self.topics[a].collapsed)
    }

    /// Roots of all independent trees: central topic, floating topics and summary topics.
    pub fn top_level_ids(&self) -> Vec<TopicId> {
        let mut v = vec![self.root];
        v.extend(self.floating.iter().copied());
        v.extend(self.summaries.iter().map(|s| s.topic));
        v
    }

    /// Every topic in document order (central tree, floating trees, summary trees).
    pub fn all_ids_ordered(&self) -> Vec<TopicId> {
        self.top_level_ids()
            .into_iter()
            .flat_map(|t| self.subtree_ids(t))
            .collect()
    }

    /// Low-level: inserts a new child topic. Index `None` appends.
    pub fn add_child(
        &mut self,
        parent: TopicId,
        index: Option<usize>,
        title: impl Into<String>,
    ) -> TopicId {
        let id = self.alloc_topic_id();
        let mut t = Topic::new(id, title);
        t.parent = Some(parent);
        self.topics.insert(id, t);
        let p = self.topics.get_mut(&parent).expect("parent exists");
        let idx = index.unwrap_or(p.children.len()).min(p.children.len());
        p.children.insert(idx, id);
        self.on_child_inserted(parent, idx);
        id
    }

    /// Low-level: adds a floating topic centered at `position`.
    pub fn add_floating(&mut self, title: impl Into<String>, position: Vec2) -> TopicId {
        let id = self.alloc_topic_id();
        let mut t = Topic::new(id, title);
        t.position = Some(position);
        self.topics.insert(id, t);
        self.floating.push(id);
        id
    }

    /// Clones the subtree rooted at `id`.
    pub fn copy_subtree(&self, id: TopicId) -> Option<Subtree> {
        if !self.contains(id) {
            return None;
        }
        let topics = self
            .subtree_ids(id)
            .into_iter()
            .map(|t| self.topics[&t].clone())
            .collect();
        Some(Subtree { root: id, topics })
    }

    /// Removes the subtree rooted at `id` from the sheet and returns it. Relationships,
    /// boundaries and summaries referencing removed topics are dropped as well and
    /// ranges of the former parent are adjusted.
    pub fn detach_subtree(&mut self, id: TopicId) -> Option<Subtree> {
        if id == self.root {
            return None;
        }
        let sub = self.copy_subtree(id)?;
        let parent = self.parent(id);
        let index = self.index_in_parent(id);

        if let Some(p) = parent {
            self.topics
                .get_mut(&p)
                .unwrap()
                .children
                .retain(|c| *c != id);
        }
        self.floating.retain(|f| *f != id);

        let removed: HashSet<TopicId> = sub.topics.iter().map(|t| t.id).collect();
        for t in &removed {
            self.topics.remove(t);
        }
        // summary topics owned by removed parents are removed as well
        let orphan_summaries: Vec<TopicId> = self
            .summaries
            .iter()
            .filter(|s| removed.contains(&s.parent))
            .map(|s| s.topic)
            .collect();
        self.summaries
            .retain(|s| !removed.contains(&s.parent) && !removed.contains(&s.topic));
        for st in orphan_summaries {
            self.detach_subtree(st);
        }
        let alive = |s: &Sheet, t: TopicId| s.topics.contains_key(&t);
        let rels: Vec<_> = self
            .relationships
            .iter()
            .filter(|r| alive(self, r.from) && alive(self, r.to))
            .cloned()
            .collect();
        self.relationships = rels;
        self.boundaries.retain(|b| !removed.contains(&b.parent));

        if let (Some(p), Some(i)) = (parent, index) {
            self.on_child_removed(p, i);
        }
        let mut sub = sub;
        if let Some(t) = sub.topics.iter_mut().find(|t| t.id == id) {
            t.parent = None;
        }
        Some(sub)
    }

    /// Re-inserts a detached subtree as a child of `parent` at `index` (clamped).
    pub fn attach_child(&mut self, parent: TopicId, index: usize, sub: Subtree) -> TopicId {
        let root = sub.root;
        for mut t in sub.topics {
            if t.id == root {
                t.parent = Some(parent);
                t.position = None;
            }
            self.topics.insert(t.id, t);
        }
        let p = self.topics.get_mut(&parent).expect("parent exists");
        let idx = index.min(p.children.len());
        p.children.insert(idx, root);
        self.on_child_inserted(parent, idx);
        self.bump_next_id();
        root
    }

    /// Re-inserts a detached subtree as a floating topic.
    pub fn attach_floating(&mut self, position: Vec2, sub: Subtree) -> TopicId {
        let root = sub.root;
        for mut t in sub.topics {
            if t.id == root {
                t.parent = None;
                t.position = Some(position);
            }
            self.topics.insert(t.id, t);
        }
        self.floating.push(root);
        self.bump_next_id();
        root
    }

    fn on_child_inserted(&mut self, parent: TopicId, index: usize) {
        let adjust = |r: &mut ChildRange| {
            if index <= r.start {
                r.start += 1;
                r.end += 1;
            } else if index <= r.end {
                r.end += 1;
            }
        };
        for b in self.boundaries.iter_mut().filter(|b| b.parent == parent) {
            if let Some(r) = b.range.as_mut() {
                adjust(r);
            }
        }
        for s in self.summaries.iter_mut().filter(|s| s.parent == parent) {
            adjust(&mut s.range);
        }
    }

    fn on_child_removed(&mut self, parent: TopicId, index: usize) {
        // returns false if the range becomes empty
        let adjust = |r: &mut ChildRange| -> bool {
            if index < r.start {
                r.start -= 1;
                r.end -= 1;
                true
            } else if index <= r.end {
                if r.start == r.end {
                    false
                } else {
                    r.end -= 1;
                    true
                }
            } else {
                true
            }
        };
        self.boundaries
            .retain_mut(|b| match (b.parent == parent, b.range.as_mut()) {
                (true, Some(r)) => adjust(r),
                _ => true,
            });
        let mut dead_topics = Vec::new();
        self.summaries.retain_mut(|s| {
            if s.parent != parent {
                return true;
            }
            let keep = adjust(&mut s.range);
            if !keep {
                dead_topics.push(s.topic);
            }
            keep
        });
        for st in dead_topics {
            self.detach_subtree(st);
        }
    }

    /// Checks structural invariants; used by tests and after imports.
    pub fn validate(&self) -> Result<(), String> {
        if !self.contains(self.root) {
            return Err("root topic missing".into());
        }
        self.theme.validate()?;
        if self.root_topic().parent.is_some() {
            return Err("root has a parent".into());
        }
        for (id, t) in &self.topics {
            if t.marker_colors
                .keys()
                .any(|index| *index >= t.markers.len())
            {
                return Err(format!("topic {id} has a color for a missing marker"));
            }
            if t.id != *id {
                return Err(format!("topic key {id} != id {}", t.id));
            }
            for c in &t.children {
                let ct = self.topic(*c).ok_or(format!("child {c} of {id} missing"))?;
                if ct.parent != Some(*id) {
                    return Err(format!("child {c} has wrong parent {:?}", ct.parent));
                }
            }
            match t.parent {
                Some(p) => {
                    let pt = self.topic(p).ok_or(format!("parent {p} of {id} missing"))?;
                    if pt.children.iter().filter(|c| **c == *id).count() != 1 {
                        return Err(format!("{id} not listed exactly once in parent {p}"));
                    }
                }
                None => {
                    let is_top = *id == self.root
                        || self.floating.contains(id)
                        || self.summaries.iter().any(|s| s.topic == *id);
                    if !is_top {
                        return Err(format!("orphan topic {id}"));
                    }
                }
            }
        }
        let mut reachable = 0;
        for top in self.top_level_ids() {
            reachable += self.subtree_ids(top).len();
        }
        if reachable != self.topics.len() {
            return Err(format!(
                "{} topics but {reachable} reachable",
                self.topics.len()
            ));
        }
        for r in &self.relationships {
            if !self.contains(r.from) || !self.contains(r.to) {
                return Err(format!("relationship {:?} dangling", r.id));
            }
            if !Relationship::valid_control_points(r.control_points) {
                return Err(format!(
                    "relationship {:?} has invalid curve controls",
                    r.id
                ));
            }
        }
        for b in &self.boundaries {
            let n = self.children(b.parent).len();
            if !self.contains(b.parent) {
                return Err(format!("boundary {:?} dangling", b.id));
            }
            if let Some(r) = b.range
                && (r.start > r.end || r.end >= n)
            {
                return Err(format!("boundary {:?} range out of bounds", b.id));
            }
        }
        for s in &self.summaries {
            let n = self.children(s.parent).len();
            if !self.contains(s.parent) || !self.contains(s.topic) {
                return Err(format!("summary {:?} dangling", s.id));
            }
            if s.range.start > s.range.end || s.range.end >= n {
                return Err(format!("summary {:?} range out of bounds", s.id));
            }
        }
        Ok(())
    }
}

/// Document metadata.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub creator: String,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub description: Option<String>,
}

/// A Thinoda document: a set of sheets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    #[serde(default)]
    pub active_sheet: usize,
    #[serde(default)]
    pub metadata: Metadata,
}

impl Workbook {
    pub fn new(sheet: Sheet) -> Self {
        Self {
            sheets: vec![sheet],
            active_sheet: 0,
            metadata: Metadata::default(),
        }
    }

    pub fn active(&self) -> &Sheet {
        &self.sheets[self.active_sheet.min(self.sheets.len() - 1)]
    }

    pub fn active_mut(&mut self) -> &mut Sheet {
        let i = self.active_sheet.min(self.sheets.len() - 1);
        &mut self.sheets[i]
    }

    /// Creates a derived, text-masked document. Structure, colors and task states
    /// remain observable; this is not a guarantee of anonymity.
    pub fn masked_copy(&self) -> Self {
        let mut copy = self.clone();
        copy.metadata = Metadata::default();
        for (sheet_index, sheet) in copy.sheets.iter_mut().enumerate() {
            sheet.title = format!("Sheet {}", sheet_index + 1);
            sheet.theme.font_family = None;
            if let Some(custom) = &mut sheet.theme.custom_theme {
                custom.name = "Custom Theme".into();
                custom.font_family = None;
                for role in [
                    crate::theme::Level::Central,
                    crate::theme::Level::Main,
                    crate::theme::Level::Sub,
                    crate::theme::Level::Floating,
                    crate::theme::Level::Summary,
                ] {
                    custom.roles.get_mut(role).font_family = None;
                }
            }
            if !crate::theme::Theme::IDS.contains(&sheet.theme.theme_id.as_str()) {
                sheet.theme.theme_id = crate::theme::ThemeSettings::default().theme_id;
            }
            for (index, topic) in sheet.topics.values_mut().enumerate() {
                topic.title = format!("Topic {}", index + 1);
                topic.notes = None;
                topic.labels.clear();
                topic.hyperlink = None;
                topic.image = None;
                topic.markers.clear();
                topic.marker_colors.clear();
                topic.style.font_family = None;
            }
            for (index, relationship) in sheet.relationships.iter_mut().enumerate() {
                relationship.title = format!("Relationship {}", index + 1);
            }
            for boundary in &mut sheet.boundaries {
                boundary.title = None;
            }
        }
        copy
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.sheets.is_empty() {
            return Err("workbook has no sheets".into());
        }
        for (i, s) in self.sheets.iter().enumerate() {
            s.validate().map_err(|e| format!("sheet {i}: {e}"))?;
        }
        Ok(())
    }
}

impl Default for Workbook {
    fn default() -> Self {
        crate::templates::blank("Map 1", "Central Topic", "Main Topic", 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Sheet, TopicId, TopicId, TopicId) {
        let mut s = Sheet::new("S", "Root");
        let a = s.add_child(s.root, None, "A");
        let b = s.add_child(s.root, None, "B");
        let a1 = s.add_child(a, None, "A1");
        (s, a, b, a1)
    }

    #[test]
    fn masked_copy_removes_every_free_text_field_without_changing_source_or_structure() {
        let (mut s, a, b, _) = sample();
        let summary = s.add_summary(&[a, b], "SECRET Summary").unwrap();
        s.add_child(summary, None, "SECRET summary child");
        s.add_boundary(&[a, b]).unwrap();
        s.boundaries[0].title = Some("SECRET boundary".into());
        s.add_relationship(a, b).unwrap();
        s.relationships[0].title = "SECRET relationship".into();
        s.title = "SECRET sheet".into();
        s.theme.theme_id = "SECRET custom theme".into();
        s.theme.font_family = Some("SECRET font".into());
        for topic in s.topics.values_mut() {
            topic.title = "SECRET title".into();
            topic.notes = Some("SECRET notes".into());
            topic.labels = vec!["SECRET label".into()];
            topic.markers = vec!["SECRET marker".into()];
            topic.hyperlink = Some("file:///SECRET/file".into());
            topic.image = Some("SECRET image path".into());
            topic.style.font_family = Some("SECRET topic font".into());
            topic.todo = Some(true);
        }
        let mut book = Workbook::new(s);
        book.metadata.creator = "SECRET creator".into();
        book.metadata.description = Some("SECRET description".into());
        book.metadata.created = Some("SECRET date".into());
        book.metadata.modified = Some("SECRET changed".into());
        book.sheets.push(book.sheets[0].clone());
        book.active_sheet = 1;
        let original = book.clone();
        let copy = book.masked_copy();
        copy.validate().unwrap();
        assert!(!serde_json::to_string(&copy).unwrap().contains("SECRET"));
        assert_eq!(copy.metadata, Metadata::default());
        assert_eq!(copy.sheets.len(), 2);
        assert_eq!(copy.active_sheet, 1);
        for (masked, source) in copy.sheets.iter().zip(&book.sheets) {
            assert_eq!(masked.root, source.root);
            assert_eq!(masked.summaries, source.summaries);
            assert_eq!(masked.structure, source.structure);
            for (id, topic) in &masked.topics {
                assert_eq!(topic.parent, source.topics[id].parent);
                assert_eq!(topic.children, source.topics[id].children);
                assert_eq!(topic.todo, Some(true));
            }
        }
        assert_eq!(book, original);
        assert_eq!(copy.masked_copy(), copy);
    }

    #[test]
    fn build_and_query_tree() {
        let (s, a, b, a1) = sample();
        assert_eq!(s.children(s.root), &[a, b]);
        assert_eq!(s.parent(a1), Some(a));
        assert_eq!(s.depth(a1), 2);
        assert_eq!(s.ancestors(a1), vec![a, s.root]);
        assert_eq!(s.index_in_parent(b), Some(1));
        assert_eq!(s.main_branch_of(a1), Some(a));
        assert_eq!(s.main_branch_of(s.root), None);
        assert_eq!(s.subtree_ids(s.root), vec![s.root, a, a1, b]);
        assert!(s.is_ancestor_or_self(a, a1));
        assert!(!s.is_ancestor_or_self(b, a1));
        s.validate().unwrap();
    }

    #[test]
    fn detach_and_attach_roundtrip() {
        let (mut s, a, b, a1) = sample();
        let sub = s.detach_subtree(a).unwrap();
        assert_eq!(sub.len(), 2);
        assert!(!s.contains(a1));
        s.validate().unwrap();
        s.attach_child(b, 0, sub);
        assert_eq!(s.parent(a), Some(b));
        assert_eq!(s.depth(a1), 3);
        s.validate().unwrap();
    }

    #[test]
    fn cannot_detach_root() {
        let (mut s, ..) = sample();
        assert!(s.detach_subtree(s.root).is_none());
    }

    #[test]
    fn detach_removes_dangling_relationships_and_boundaries() {
        let (mut s, a, b, a1) = sample();
        let rid = s.alloc_relationship_id();
        s.relationships.push(Relationship {
            id: rid,
            from: a1,
            to: b,
            title: String::new(),
            color: None,
            control_points: None,
        });
        let bid = s.alloc_boundary_id();
        s.boundaries.push(Boundary {
            id: bid,
            parent: a,
            range: None,
            title: None,
            color: None,
        });
        s.detach_subtree(a).unwrap();
        assert!(s.relationships.is_empty());
        assert!(s.boundaries.is_empty());
        s.validate().unwrap();
    }

    #[test]
    fn ranges_follow_child_insert_and_remove() {
        let mut s = Sheet::new("S", "Root");
        let kids: Vec<_> = (0..4)
            .map(|i| s.add_child(s.root, None, format!("{i}")))
            .collect();
        let bid = s.alloc_boundary_id();
        s.boundaries.push(Boundary {
            id: bid,
            parent: s.root,
            range: Some(ChildRange { start: 1, end: 2 }),
            title: None,
            color: None,
        });
        s.add_child(s.root, Some(0), "new first");
        assert_eq!(s.boundaries[0].range, Some(ChildRange { start: 2, end: 3 }));
        s.detach_subtree(kids[1]).unwrap();
        assert_eq!(s.boundaries[0].range, Some(ChildRange { start: 2, end: 2 }));
        s.detach_subtree(kids[2]).unwrap();
        assert!(s.boundaries.is_empty());
        s.validate().unwrap();
    }

    #[test]
    fn reassign_ids_produces_fresh_ids() {
        let (mut s, a, ..) = sample();
        let mut sub = s.copy_subtree(a).unwrap();
        sub.reassign_ids(&mut s);
        assert_ne!(sub.root, a);
        let root = sub.root;
        s.attach_child(s.root, 0, sub);
        assert_eq!(s.children(root).len(), 1);
        s.validate().unwrap();
    }

    #[test]
    fn structure_class_roundtrip() {
        for st in Structure::ALL {
            assert_eq!(Structure::from_xmind_class(st.xmind_class()), Some(st));
        }
        assert_eq!(
            Structure::from_xmind_class("org.xmind.ui.map"),
            Some(Structure::MapClockwise)
        );
        assert_eq!(
            Structure::from_xmind_class("org.xmind.ui.timeline.vertical.btt"),
            Some(Structure::TimelineVertical)
        );
        assert_eq!(Structure::from_xmind_class("bogus"), None);
    }

    #[test]
    fn serde_roundtrip() {
        let (s, ..) = sample();
        let wb = Workbook::new(s);
        let json = serde_json::to_string(&wb).unwrap();
        let back: Workbook = serde_json::from_str(&json).unwrap();
        assert_eq!(wb, back);
    }
}
