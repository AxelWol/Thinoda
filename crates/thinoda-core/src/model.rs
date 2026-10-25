//! Workbook / sheet / topic data model.
//!
//! Topics are stored in an arena (`BTreeMap<TopicId, Topic>`) per sheet. Each topic knows
//! its parent and the ordered list of its children. Concepts and identifiers mirror XMind so
//! that `.xmind` files can be imported and exported with high fidelity.

use crate::geometry::{Color, Rect, Vec2};
use crate::schedule::{SchedulingData, Task, TaskDependency};
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
id_type!(ZoneId);
id_type!(SlideId);

pub const MAX_ZONES: usize = 256;
pub const MAX_ZONE_MEMBERSHIPS: usize = 4096;
pub const MAX_PRESENTATION_SLIDES: usize = 512;
pub const MIN_ZONE_SIZE: f32 = 100.0;
pub const MAX_PRESENTATION_COORDINATE: f32 = 1_000_000.0;
pub const MAX_PRESENTATION_NAME_BYTES: usize = 256;

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
    /// Explicit horizontal and vertical padding, used to preserve topic size across hierarchy moves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<Vec2>,
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

/// A named frame containing one or more independent floating topic trees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub id: ZoneId,
    pub name: String,
    pub frame: Rect,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<TopicId>,
}

/// Origin and size in sheet coordinates for a presentation slide.
pub type SlideFrame = Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresentationSource {
    Branch(TopicId),
    Zone(ZoneId),
}

/// Saved frame and ordering for one generated or user-curated slide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationSlide {
    pub id: SlideId,
    pub title: String,
    pub source: PresentationSource,
    pub frame: SlideFrame,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Presentation {
    pub slides: Vec<PresentationSlide>,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tasks: BTreeMap<TopicId, Task>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<TaskDependency>,
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
        self.tasks = std::mem::take(&mut self.tasks)
            .into_iter()
            .map(|(topic, task)| (m(topic), task))
            .collect();
        self.dependencies.retain_mut(|dependency| {
            let Some(predecessor) = map.get(&dependency.predecessor).copied() else {
                return false;
            };
            let Some(successor) = map.get(&dependency.successor).copied() else {
                return false;
            };
            dependency.predecessor = predecessor;
            dependency.successor = successor;
            true
        });
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
    pub zones: Vec<Zone>,
    #[serde(default)]
    pub presentation: Presentation,
    #[serde(default)]
    pub summaries: Vec<Summary>,
    #[serde(default)]
    pub structure: Structure,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    pub scheduling: SchedulingData,
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
            zones: Vec::new(),
            presentation: Presentation::default(),
            summaries: Vec::new(),
            structure: Structure::default(),
            theme: ThemeSettings::default(),
            scheduling: SchedulingData::default(),
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
            .chain(self.zones.iter().map(|z| z.id.0))
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

    pub fn alloc_zone_id(&mut self) -> ZoneId {
        ZoneId(self.alloc())
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
        let ids = self.subtree_ids(id);
        let owned: HashSet<_> = ids.iter().copied().collect();
        let topics = ids.iter().map(|t| self.topics[t].clone()).collect();
        let tasks = self
            .scheduling
            .tasks
            .iter()
            .filter(|(topic, _)| owned.contains(topic))
            .map(|(topic, task)| (*topic, task.clone()))
            .collect();
        let dependencies = self
            .scheduling
            .dependencies
            .iter()
            .filter(|dependency| {
                owned.contains(&dependency.predecessor) && owned.contains(&dependency.successor)
            })
            .copied()
            .collect();
        Some(Subtree {
            root: id,
            topics,
            tasks,
            dependencies,
        })
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
            self.scheduling.tasks.remove(t);
        }
        self.scheduling.dependencies.retain(|dependency| {
            !removed.contains(&dependency.predecessor) && !removed.contains(&dependency.successor)
        });
        for zone in &mut self.zones {
            zone.roots.retain(|root| !removed.contains(root));
        }
        self.presentation.slides.retain(|slide| match slide.source {
            PresentationSource::Branch(root) => !removed.contains(&root),
            PresentationSource::Zone(zone) => self.zones.iter().any(|item| item.id == zone),
        });
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
        let Subtree {
            root,
            topics,
            tasks,
            dependencies,
        } = sub;
        for mut t in topics {
            if t.id == root {
                t.parent = Some(parent);
                t.position = None;
            }
            self.topics.insert(t.id, t);
        }
        self.scheduling.tasks.extend(tasks);
        self.scheduling.dependencies.extend(dependencies);
        let p = self.topics.get_mut(&parent).expect("parent exists");
        let idx = index.min(p.children.len());
        p.children.insert(idx, root);
        self.on_child_inserted(parent, idx);
        self.bump_next_id();
        root
    }

    /// Re-inserts a detached subtree as a floating topic.
    pub fn attach_floating(&mut self, position: Vec2, sub: Subtree) -> TopicId {
        let Subtree {
            root,
            topics,
            tasks,
            dependencies,
        } = sub;
        for mut t in topics {
            if t.id == root {
                t.parent = None;
                t.position = Some(position);
            }
            self.topics.insert(t.id, t);
        }
        self.scheduling.tasks.extend(tasks);
        self.scheduling.dependencies.extend(dependencies);
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
        if self.zones.len() > MAX_ZONES {
            return Err(format!("zone count exceeds {MAX_ZONES}"));
        }
        let mut zone_ids = HashSet::new();
        let mut zone_roots = HashSet::new();
        let mut membership_count = 0usize;
        for zone in &self.zones {
            if !zone_ids.insert(zone.id) {
                return Err(format!("duplicate zone id {}", zone.id));
            }
            validate_presentation_name(&zone.name, "zone")?;
            validate_frame(zone.frame, true)?;
            for root in &zone.roots {
                membership_count += 1;
                if membership_count > MAX_ZONE_MEMBERSHIPS {
                    return Err(format!(
                        "zone membership count exceeds {MAX_ZONE_MEMBERSHIPS}"
                    ));
                }
                if !zone_roots.insert(*root) {
                    return Err(format!(
                        "floating topic {root} belongs to more than one zone"
                    ));
                }
                if *root == self.root
                    || !self.floating.contains(root)
                    || self.parent(*root).is_some()
                {
                    return Err(format!(
                        "zone {} member {root} is not a floating root",
                        zone.id
                    ));
                }
            }
        }
        self.presentation.validate(self)?;
        self.scheduling
            .validate(self)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

impl Presentation {
    pub fn validate(&self, sheet: &Sheet) -> Result<(), String> {
        if self.slides.len() > MAX_PRESENTATION_SLIDES {
            return Err(format!(
                "presentation slide count exceeds {MAX_PRESENTATION_SLIDES}"
            ));
        }
        let mut ids = HashSet::new();
        for slide in &self.slides {
            if !ids.insert(slide.id) {
                return Err(format!("duplicate presentation slide id {}", slide.id));
            }
            validate_presentation_name(&slide.title, "slide")?;
            validate_frame(slide.frame, false)?;
            match slide.source {
                PresentationSource::Branch(root)
                    if sheet.contains(root) && sheet.main_branch_of(root).is_some() => {}
                PresentationSource::Branch(root) => {
                    return Err(format!(
                        "presentation slide {} has invalid branch source {root}",
                        slide.id
                    ));
                }
                PresentationSource::Zone(zone)
                    if sheet.zones.iter().any(|item| item.id == zone) => {}
                PresentationSource::Zone(zone) => {
                    return Err(format!(
                        "presentation slide {} references missing zone {zone}",
                        slide.id
                    ));
                }
            }
        }
        Ok(())
    }
}

fn validate_presentation_name(name: &str, kind: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("{kind} name cannot be empty"));
    }
    if name.len() > MAX_PRESENTATION_NAME_BYTES {
        return Err(format!(
            "{kind} name exceeds {MAX_PRESENTATION_NAME_BYTES} bytes"
        ));
    }
    Ok(())
}

fn validate_frame(frame: Rect, zone: bool) -> Result<(), String> {
    let max = frame.max();
    let min_size = if zone { MIN_ZONE_SIZE } else { 1.0 };
    let valid_coordinate =
        |value: f32| value.is_finite() && value.abs() <= MAX_PRESENTATION_COORDINATE;
    if !valid_coordinate(frame.min.x)
        || !valid_coordinate(frame.min.y)
        || !valid_coordinate(max.x)
        || !valid_coordinate(max.y)
        || !frame.size.x.is_finite()
        || !frame.size.y.is_finite()
        || frame.size.x < min_size
        || frame.size.y < min_size
        || frame.size.x > MAX_PRESENTATION_COORDINATE
        || frame.size.y > MAX_PRESENTATION_COORDINATE
    {
        return Err(if zone {
            "zone frame must be finite, bounded and at least 100 by 100".into()
        } else {
            "slide frame must be finite, bounded and at least 1 by 1".into()
        });
    }
    Ok(())
}

impl Sheet {
    pub fn create_zone(
        &mut self,
        name: &str,
        roots: Vec<TopicId>,
        frame: Rect,
    ) -> Result<ZoneId, String> {
        validate_presentation_name(name, "zone")?;
        validate_frame(frame, true)?;
        if self.zones.len() >= MAX_ZONES {
            return Err(format!("zone count exceeds {MAX_ZONES}"));
        }
        if roots.is_empty() {
            return Err("a zone needs at least one floating topic".into());
        }
        if roots.len() > MAX_ZONE_MEMBERSHIPS
            || self
                .zones
                .iter()
                .map(|zone| zone.roots.len())
                .sum::<usize>()
                .saturating_add(roots.len())
                > MAX_ZONE_MEMBERSHIPS
        {
            return Err(format!(
                "zone membership count exceeds {MAX_ZONE_MEMBERSHIPS}"
            ));
        }
        let existing: HashSet<_> = self
            .zones
            .iter()
            .flat_map(|zone| zone.roots.iter().copied())
            .collect();
        let mut new_roots = HashSet::new();
        for root in &roots {
            if *root == self.root
                || !self.floating.contains(root)
                || self.parent(*root).is_some()
                || !new_roots.insert(*root)
                || existing.contains(root)
            {
                return Err(format!("topic {root} is not an available floating root"));
            }
        }
        let id = self.alloc_zone_id();
        self.zones.push(Zone {
            id,
            name: name.trim().to_string(),
            frame,
            roots,
        });
        Ok(id)
    }

    pub fn rename_zone(&mut self, id: ZoneId, name: &str) -> Result<(), String> {
        validate_presentation_name(name, "zone")?;
        let zone = self
            .zones
            .iter_mut()
            .find(|zone| zone.id == id)
            .ok_or_else(|| format!("zone {id} not found"))?;
        zone.name = name.trim().to_string();
        Ok(())
    }

    pub fn reorder_zone(&mut self, id: ZoneId, index: usize) -> Result<(), String> {
        let current = self
            .zones
            .iter()
            .position(|zone| zone.id == id)
            .ok_or_else(|| format!("zone {id} not found"))?;
        if index >= self.zones.len() {
            return Err("zone order index out of bounds".into());
        }
        let zone = self.zones.remove(current);
        self.zones.insert(index, zone);
        Ok(())
    }

    /// Moves the zone frame and all its floating topic trees by `delta`.
    pub fn move_zone(&mut self, id: ZoneId, delta: Vec2) -> Result<(), String> {
        if !delta.x.is_finite()
            || !delta.y.is_finite()
            || delta.x.abs() > MAX_PRESENTATION_COORDINATE
            || delta.y.abs() > MAX_PRESENTATION_COORDINATE
        {
            return Err("zone movement must be finite and bounded".into());
        }
        let zone = self
            .zones
            .iter()
            .find(|zone| zone.id == id)
            .ok_or_else(|| format!("zone {id} not found"))?;
        let frame = zone.frame.translate(delta);
        validate_frame(frame, true)?;
        let positions = zone
            .roots
            .iter()
            .map(|root| {
                self.topic(*root)
                    .and_then(|topic| topic.position)
                    .map(|position| (*root, position + delta))
                    .ok_or_else(|| format!("zone {id} has a floating topic without a position"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if positions.iter().any(|(_, position)| {
            !position.x.is_finite()
                || !position.y.is_finite()
                || position.x.abs() > MAX_PRESENTATION_COORDINATE
                || position.y.abs() > MAX_PRESENTATION_COORDINATE
        }) {
            return Err("zone movement would place a topic outside the coordinate limit".into());
        }
        self.zones
            .iter_mut()
            .find(|zone| zone.id == id)
            .expect("zone checked above")
            .frame = frame;
        for (root, position) in positions {
            self.topic_mut(root)
                .expect("zone root checked above")
                .position = Some(position);
        }
        Ok(())
    }

    pub fn resize_zone(&mut self, id: ZoneId, size: Vec2) -> Result<(), String> {
        let zone = self
            .zones
            .iter_mut()
            .find(|zone| zone.id == id)
            .ok_or_else(|| format!("zone {id} not found"))?;
        let frame = Rect::from_min_size(zone.frame.min, size);
        validate_frame(frame, true)?;
        zone.frame = frame;
        Ok(())
    }

    /// Removes only the Zone and its Pitch slides; floating topics are retained.
    pub fn remove_zone(&mut self, id: ZoneId) -> Result<(), String> {
        let index = self
            .zones
            .iter()
            .position(|zone| zone.id == id)
            .ok_or_else(|| format!("zone {id} not found"))?;
        self.zones.remove(index);
        self.presentation
            .slides
            .retain(|slide| slide.source != PresentationSource::Zone(id));
        Ok(())
    }

    pub fn replace_presentation(&mut self, presentation: Presentation) -> Result<(), String> {
        presentation.validate(self)?;
        self.presentation = presentation;
        Ok(())
    }

    pub fn add_presentation_slide(
        &mut self,
        title: &str,
        source: PresentationSource,
        frame: SlideFrame,
    ) -> Result<SlideId, String> {
        if self.presentation.slides.len() >= MAX_PRESENTATION_SLIDES {
            return Err(format!(
                "presentation slide count exceeds {MAX_PRESENTATION_SLIDES}"
            ));
        }
        let next_id = self
            .presentation
            .slides
            .iter()
            .map(|slide| slide.id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| "presentation slide id space exhausted".to_string())?;
        let slide = PresentationSlide {
            id: SlideId(next_id),
            title: title.trim().to_string(),
            source,
            frame,
        };
        validate_presentation_name(&slide.title, "slide")?;
        validate_frame(slide.frame, false)?;
        let mut candidate = self.presentation.clone();
        candidate.slides.push(slide.clone());
        candidate.validate(self)?;
        self.presentation.slides.push(slide);
        Ok(SlideId(next_id))
    }

    pub fn rename_presentation_slide(&mut self, id: SlideId, title: &str) -> Result<(), String> {
        validate_presentation_name(title, "slide")?;
        let slide = self
            .presentation
            .slides
            .iter_mut()
            .find(|slide| slide.id == id)
            .ok_or_else(|| format!("presentation slide {id} not found"))?;
        slide.title = title.trim().to_string();
        Ok(())
    }

    pub fn set_presentation_slide_frame(
        &mut self,
        id: SlideId,
        frame: SlideFrame,
    ) -> Result<(), String> {
        validate_frame(frame, false)?;
        let slide = self
            .presentation
            .slides
            .iter_mut()
            .find(|slide| slide.id == id)
            .ok_or_else(|| format!("presentation slide {id} not found"))?;
        slide.frame = frame;
        Ok(())
    }

    pub fn reorder_presentation_slide(&mut self, id: SlideId, index: usize) -> Result<(), String> {
        let current = self
            .presentation
            .slides
            .iter()
            .position(|slide| slide.id == id)
            .ok_or_else(|| format!("presentation slide {id} not found"))?;
        if index >= self.presentation.slides.len() {
            return Err("presentation order index out of bounds".into());
        }
        let slide = self.presentation.slides.remove(current);
        self.presentation.slides.insert(index, slide);
        Ok(())
    }

    pub fn remove_presentation_slide(&mut self, id: SlideId) -> Result<(), String> {
        let index = self
            .presentation
            .slides
            .iter()
            .position(|slide| slide.id == id)
            .ok_or_else(|| format!("presentation slide {id} not found"))?;
        self.presentation.slides.remove(index);
        Ok(())
    }
}

#[cfg(test)]
mod zone_presentation_tests {
    use super::*;

    fn frame(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, height))
    }

    #[test]
    fn zones_validate_floating_roots_and_allow_overlap() {
        let mut sheet = Sheet::new("Map", "Central");
        let first = sheet.add_floating("First", Vec2::new(120.0, 120.0));
        let second = sheet.add_floating("Second", Vec2::new(200.0, 120.0));
        let first_zone = sheet
            .create_zone("One", vec![first], frame(0.0, 0.0, 100.0, 100.0))
            .unwrap();
        sheet
            .create_zone("Two", vec![second], frame(50.0, 50.0, 100.0, 100.0))
            .unwrap();
        sheet.validate().unwrap();
        assert!(
            sheet
                .create_zone("Small", vec![first], frame(0.0, 0.0, 99.0, 100.0))
                .is_err()
        );
        assert!(
            sheet
                .create_zone("Child", vec![sheet.root], frame(0.0, 0.0, 100.0, 100.0))
                .is_err()
        );
        assert!(sheet.rename_zone(first_zone, "  ").is_err());
    }

    #[test]
    fn moving_resizing_and_removing_zone_preserve_content() {
        let mut sheet = Sheet::new("Map", "Central");
        let floating = sheet.add_floating("Floating", Vec2::new(125.0, 130.0));
        let zone = sheet
            .create_zone("Ideas", vec![floating], frame(0.0, 0.0, 200.0, 200.0))
            .unwrap();
        let slide = sheet
            .add_presentation_slide(
                "Ideas",
                PresentationSource::Zone(zone),
                frame(0.0, 0.0, 200.0, 200.0),
            )
            .unwrap();
        sheet.move_zone(zone, Vec2::new(20.0, -10.0)).unwrap();
        assert_eq!(
            sheet.topic(floating).unwrap().position,
            Some(Vec2::new(145.0, 120.0))
        );
        assert_eq!(sheet.zones[0].frame, frame(20.0, -10.0, 200.0, 200.0));
        assert!(sheet.resize_zone(zone, Vec2::new(99.0, 200.0)).is_err());
        sheet.resize_zone(zone, Vec2::new(250.0, 225.0)).unwrap();
        sheet.remove_zone(zone).unwrap();
        assert!(sheet.contains(floating));
        assert!(sheet.presentation.slides.is_empty());
        assert!(sheet.remove_presentation_slide(slide).is_err());
        sheet.validate().unwrap();
    }

    #[test]
    fn deleting_topic_roots_cleans_zone_and_branch_slide_references() {
        let mut sheet = Sheet::new("Map", "Central");
        let branch = sheet.add_child(sheet.root, None, "Branch");
        let floating = sheet.add_floating("Floating", Vec2::new(120.0, 120.0));
        let zone = sheet
            .create_zone("Ideas", vec![floating], frame(0.0, 0.0, 200.0, 200.0))
            .unwrap();
        sheet
            .add_presentation_slide(
                "Branch",
                PresentationSource::Branch(branch),
                frame(0.0, 0.0, 200.0, 200.0),
            )
            .unwrap();
        sheet
            .add_presentation_slide(
                "Ideas",
                PresentationSource::Zone(zone),
                frame(0.0, 0.0, 200.0, 200.0),
            )
            .unwrap();
        sheet.detach_subtree(branch).unwrap();
        assert_eq!(sheet.presentation.slides.len(), 1);
        sheet.detach_subtree(floating).unwrap();
        assert!(sheet.zones[0].roots.is_empty());
        sheet.validate().unwrap();
    }

    #[test]
    fn saved_zone_and_presentation_roundtrip_and_legacy_sheet_defaults() {
        let mut sheet = Sheet::new("Map", "Central");
        let floating = sheet.add_floating("Floating", Vec2::new(140.0, 150.0));
        let zone = sheet
            .create_zone("Ideas", vec![floating], frame(0.0, 0.0, 300.0, 200.0))
            .unwrap();
        sheet
            .add_presentation_slide(
                "Ideas",
                PresentationSource::Zone(zone),
                frame(0.0, 0.0, 300.0, 200.0),
            )
            .unwrap();
        let json = serde_json::to_value(&sheet).unwrap();
        let restored: Sheet = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(restored, sheet);
        let mut legacy = json;
        legacy.as_object_mut().unwrap().remove("zones");
        legacy.as_object_mut().unwrap().remove("presentation");
        let restored: Sheet = serde_json::from_value(legacy).unwrap();
        assert!(restored.zones.is_empty());
        assert!(restored.presentation.slides.is_empty());
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
