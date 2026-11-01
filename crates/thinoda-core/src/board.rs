//! Freeform board model: root modes, the canvas object layer (shapes, ink and free
//! connectors), cards on floating topics, limits and the editing helpers used by
//! the board UI. See `docs/FREEFORM.md`.
//!
//! Every new field defaults to the legacy behaviour and is skipped when serialising
//! default values, so maps without board content serialise byte-identically.

use crate::geometry::{Color, Rect, Vec2};
use crate::model::{PresentationSource, Sheet, Topic, TopicId, ZoneId, id_type, valid_position};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

id_type!(CanvasObjectId);
id_type!(CanvasGroupId);

/// Objects per sheet.
pub const MAX_CANVAS_OBJECTS: usize = 20_000;
/// Groups per sheet (a group needs at least two objects).
pub const MAX_CANVAS_GROUPS: usize = MAX_CANVAS_OBJECTS / 2;
/// Points of one ink stroke.
pub const MAX_STROKE_POINTS: usize = 20_000;
/// Ink points of all strokes of a sheet.
pub const MAX_TOTAL_INK_POINTS: usize = 2_000_000;
/// Absolute limit for board coordinates and sizes.
pub const MAX_BOARD_COORDINATE: f32 = 1_000_000.0;
/// Characters of a shape/connector label or an alt text.
pub const MAX_BOARD_TEXT_CHARS: usize = 10_000;
pub const MIN_STROKE_WIDTH: f32 = 0.1;
pub const MAX_STROKE_WIDTH: f32 = 200.0;
pub const MIN_GRID_SIZE: f32 = 4.0;
pub const MAX_GRID_SIZE: f32 = 512.0;
pub const DEFAULT_GRID_SIZE: f32 = 24.0;
/// Smallest card edge in map units.
pub const MIN_CARD_SIZE: f32 = 16.0;

/// Whether the sheet shows its central topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SheetRootMode {
    #[default]
    Central,
    /// The root is an empty, invisible anchor; content lives in floating trees.
    Hidden,
}

impl SheetRootMode {
    pub fn is_central(&self) -> bool {
        *self == SheetRootMode::Central
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GridKind {
    #[default]
    None,
    Dots,
    Lines,
}

/// Per-sheet board preferences.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoardSettings {
    pub grid: GridKind,
    pub grid_size: f32,
    pub snap: bool,
}

impl Default for BoardSettings {
    fn default() -> Self {
        Self {
            grid: GridKind::None,
            grid_size: DEFAULT_GRID_SIZE,
            snap: false,
        }
    }
}

impl BoardSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Paint band of a canvas object relative to the map (tree) layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Band {
    Below,
    #[default]
    Above,
}

impl Band {
    fn is_default(&self) -> bool {
        *self == Band::Above
    }
}

/// Free objects of a sheet. Vec order within a band is the paint order (bottom first).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CanvasLayer {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<CanvasObject>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<CanvasGroup>,
}

impl CanvasLayer {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.groups.is_empty()
    }

    pub fn object(&self, id: CanvasObjectId) -> Option<&CanvasObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn object_mut(&mut self, id: CanvasObjectId) -> Option<&mut CanvasObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    pub fn group(&self, id: CanvasGroupId) -> Option<&CanvasGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// Members of `group` in paint order.
    pub fn group_members(&self, group: CanvasGroupId) -> Vec<CanvasObjectId> {
        self.objects
            .iter()
            .filter(|o| o.group == Some(group))
            .map(|o| o.id)
            .collect()
    }

    /// `ids` plus every member of the groups they belong to, in paint order.
    pub fn expand_to_groups(&self, ids: &[CanvasObjectId]) -> Vec<CanvasObjectId> {
        let wanted: HashSet<_> = ids.iter().copied().collect();
        let groups: HashSet<_> = self
            .objects
            .iter()
            .filter(|o| wanted.contains(&o.id))
            .filter_map(|o| o.group)
            .collect();
        self.objects
            .iter()
            .filter(|o| wanted.contains(&o.id) || o.group.is_some_and(|g| groups.contains(&g)))
            .map(|o| o.id)
            .collect()
    }

    /// An object is locked directly or through its group.
    pub fn is_locked(&self, id: CanvasObjectId) -> bool {
        self.object(id).is_some_and(|o| {
            o.locked
                || o.group
                    .and_then(|g| self.group(g))
                    .is_some_and(|g| g.locked)
        })
    }

    /// Total number of stored ink points.
    pub fn ink_point_count(&self) -> usize {
        self.objects
            .iter()
            .map(|o| match &o.kind {
                CanvasObjectKind::Ink(ink) => ink.points.len(),
                _ => 0,
            })
            .sum()
    }
}

/// A group of canvas objects; groups are flat (no nesting, so no cycles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasGroup {
    pub id: CanvasGroupId,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObjectStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Color>,
    pub stroke_width: f32,
    pub opacity: f32,
}

impl Default for ObjectStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            stroke_width: 2.0,
            opacity: 1.0,
        }
    }
}

/// A free object on the board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasObject {
    pub id: CanvasObjectId,
    pub kind: CanvasObjectKind,
    /// Axis-aligned, unrotated frame in sheet coordinates.
    pub rect: Rect,
    /// Degrees clockwise around the frame centre, normalised to `[0, 360)`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f32,
    #[serde(default, skip_serializing_if = "Band::is_default")]
    pub band: Band,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<CanvasGroupId>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    /// Accessibility description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub alt_text: String,
    #[serde(default)]
    pub style: ObjectStyle,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

impl CanvasObject {
    /// New object with default style in the `Above` band; the id is assigned on insertion.
    pub fn new(kind: CanvasObjectKind, rect: Rect) -> Self {
        Self {
            id: CanvasObjectId(0),
            kind,
            rect,
            rotation: 0.0,
            band: Band::Above,
            group: None,
            locked: false,
            alt_text: String::new(),
            style: ObjectStyle::default(),
        }
    }

    /// Shape with the given label.
    pub fn shape(shape: ShapeKind, rect: Rect) -> Self {
        Self::new(
            CanvasObjectKind::Shape {
                shape,
                label: String::new(),
            },
            rect,
        )
    }

    /// Ink stroke; the frame is the bounding box of the points.
    pub fn ink(stroke: InkStroke) -> Self {
        let rect = points_bounds(&stroke.points).unwrap_or_default();
        Self::new(CanvasObjectKind::Ink(stroke), rect)
    }

    /// Free connector; its frame is updated from the endpoints when possible.
    pub fn connector(connector: CanvasConnector) -> Self {
        let rect = match (connector.from, connector.to) {
            (Endpoint::Point(a), Endpoint::Point(b)) => points_bounds(&[a, b]).unwrap_or_default(),
            (Endpoint::Point(a), _) | (_, Endpoint::Point(a)) => Rect::from_min_size(a, Vec2::ZERO),
            _ => Rect::default(),
        };
        Self::new(CanvasObjectKind::Connector(connector), rect)
    }

    pub fn is_connector(&self) -> bool {
        matches!(self.kind, CanvasObjectKind::Connector(_))
    }

    /// Short text of the object (shape or connector label).
    pub fn label(&self) -> &str {
        match &self.kind {
            CanvasObjectKind::Shape { label, .. } => label,
            CanvasObjectKind::Connector(c) => &c.label,
            CanvasObjectKind::Ink(_) => "",
        }
    }

    /// Axis-aligned bounds of the rotated frame (connectors: their stored frame).
    pub fn rotated_bounds(&self) -> Rect {
        rotated_bounds(self.rect, self.rotation)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasObjectKind {
    Shape {
        shape: ShapeKind,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        label: String,
    },
    Ink(InkStroke),
    Connector(CanvasConnector),
}

/// Basic and flowchart shapes. Every kind has a geometry generator ([`ShapeKind::unit_outline`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShapeKind {
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Diamond,
    Triangle,
    RightTriangle,
    Parallelogram,
    Trapezoid,
    Pentagon,
    Hexagon,
    Octagon,
    Star,
    Plus,
    Arrow,
    DoubleArrow,
    Chevron,
    Cloud,
    Callout,
    Cylinder,
    Document,
    Process,
    PredefinedProcess,
    Decision,
    Terminator,
    Data,
    ManualInput,
    Preparation,
    Delay,
    OffPageConnector,
}

impl ShapeKind {
    pub const ALL: [ShapeKind; 29] = [
        ShapeKind::Rectangle,
        ShapeKind::RoundedRectangle,
        ShapeKind::Ellipse,
        ShapeKind::Diamond,
        ShapeKind::Triangle,
        ShapeKind::RightTriangle,
        ShapeKind::Parallelogram,
        ShapeKind::Trapezoid,
        ShapeKind::Pentagon,
        ShapeKind::Hexagon,
        ShapeKind::Octagon,
        ShapeKind::Star,
        ShapeKind::Plus,
        ShapeKind::Arrow,
        ShapeKind::DoubleArrow,
        ShapeKind::Chevron,
        ShapeKind::Cloud,
        ShapeKind::Callout,
        ShapeKind::Cylinder,
        ShapeKind::Document,
        ShapeKind::Process,
        ShapeKind::PredefinedProcess,
        ShapeKind::Decision,
        ShapeKind::Terminator,
        ShapeKind::Data,
        ShapeKind::ManualInput,
        ShapeKind::Preparation,
        ShapeKind::Delay,
        ShapeKind::OffPageConnector,
    ];

    /// Flowchart shapes (ISO 5807 subset).
    pub fn is_flowchart(self) -> bool {
        matches!(
            self,
            ShapeKind::Process
                | ShapeKind::PredefinedProcess
                | ShapeKind::Decision
                | ShapeKind::Terminator
                | ShapeKind::Data
                | ShapeKind::ManualInput
                | ShapeKind::Preparation
                | ShapeKind::Delay
                | ShapeKind::OffPageConnector
                | ShapeKind::Document
                | ShapeKind::Cylinder
        )
    }

    /// Stable identifier (matches the serialised name).
    pub fn id(self) -> &'static str {
        match self {
            ShapeKind::Rectangle => "rectangle",
            ShapeKind::RoundedRectangle => "rounded-rectangle",
            ShapeKind::Ellipse => "ellipse",
            ShapeKind::Diamond => "diamond",
            ShapeKind::Triangle => "triangle",
            ShapeKind::RightTriangle => "right-triangle",
            ShapeKind::Parallelogram => "parallelogram",
            ShapeKind::Trapezoid => "trapezoid",
            ShapeKind::Pentagon => "pentagon",
            ShapeKind::Hexagon => "hexagon",
            ShapeKind::Octagon => "octagon",
            ShapeKind::Star => "star",
            ShapeKind::Plus => "plus",
            ShapeKind::Arrow => "arrow",
            ShapeKind::DoubleArrow => "double-arrow",
            ShapeKind::Chevron => "chevron",
            ShapeKind::Cloud => "cloud",
            ShapeKind::Callout => "callout",
            ShapeKind::Cylinder => "cylinder",
            ShapeKind::Document => "document",
            ShapeKind::Process => "process",
            ShapeKind::PredefinedProcess => "predefined-process",
            ShapeKind::Decision => "decision",
            ShapeKind::Terminator => "terminator",
            ShapeKind::Data => "data",
            ShapeKind::ManualInput => "manual-input",
            ShapeKind::Preparation => "preparation",
            ShapeKind::Delay => "delay",
            ShapeKind::OffPageConnector => "off-page-connector",
        }
    }

    /// Closed outline polygon in the unit square `[0,1]²` (y down, clockwise on screen).
    /// Curves are flattened; the first point is not repeated at the end.
    pub fn unit_outline(self) -> Vec<Vec2> {
        let p = Vec2::new;
        match self {
            ShapeKind::Rectangle | ShapeKind::Process | ShapeKind::PredefinedProcess => {
                vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)]
            }
            ShapeKind::RoundedRectangle => rounded_rect(0.15, 0.15),
            ShapeKind::Terminator => rounded_rect(0.5, 0.5),
            ShapeKind::Ellipse => ellipse_arc(0.5, 0.5, 0.5, 0.5, 0.0, 360.0, 48),
            ShapeKind::Diamond | ShapeKind::Decision => {
                vec![p(0.5, 0.0), p(1.0, 0.5), p(0.5, 1.0), p(0.0, 0.5)]
            }
            ShapeKind::Triangle => vec![p(0.5, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
            ShapeKind::RightTriangle => vec![p(0.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
            ShapeKind::Parallelogram | ShapeKind::Data => {
                vec![p(0.2, 0.0), p(1.0, 0.0), p(0.8, 1.0), p(0.0, 1.0)]
            }
            ShapeKind::Trapezoid => vec![p(0.2, 0.0), p(0.8, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
            ShapeKind::Pentagon => regular_polygon(5, -90.0),
            ShapeKind::Hexagon => {
                vec![
                    p(0.25, 0.0),
                    p(0.75, 0.0),
                    p(1.0, 0.5),
                    p(0.75, 1.0),
                    p(0.25, 1.0),
                    p(0.0, 0.5),
                ]
            }
            ShapeKind::Preparation => {
                vec![
                    p(0.15, 0.0),
                    p(0.85, 0.0),
                    p(1.0, 0.5),
                    p(0.85, 1.0),
                    p(0.15, 1.0),
                    p(0.0, 0.5),
                ]
            }
            ShapeKind::Octagon => {
                let a = 0.29;
                vec![
                    p(a, 0.0),
                    p(1.0 - a, 0.0),
                    p(1.0, a),
                    p(1.0, 1.0 - a),
                    p(1.0 - a, 1.0),
                    p(a, 1.0),
                    p(0.0, 1.0 - a),
                    p(0.0, a),
                ]
            }
            ShapeKind::Star => (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { 0.5 } else { 0.2 };
                    let a = (-90.0 + 36.0 * i as f32).to_radians();
                    p(0.5 + r * a.cos(), 0.5 + r * a.sin())
                })
                .collect(),
            ShapeKind::Plus => {
                let (a, b) = (1.0 / 3.0, 2.0 / 3.0);
                vec![
                    p(a, 0.0),
                    p(b, 0.0),
                    p(b, a),
                    p(1.0, a),
                    p(1.0, b),
                    p(b, b),
                    p(b, 1.0),
                    p(a, 1.0),
                    p(a, b),
                    p(0.0, b),
                    p(0.0, a),
                    p(a, a),
                ]
            }
            ShapeKind::Arrow => vec![
                p(0.0, 0.25),
                p(0.65, 0.25),
                p(0.65, 0.0),
                p(1.0, 0.5),
                p(0.65, 1.0),
                p(0.65, 0.75),
                p(0.0, 0.75),
            ],
            ShapeKind::DoubleArrow => vec![
                p(0.0, 0.5),
                p(0.3, 0.0),
                p(0.3, 0.25),
                p(0.7, 0.25),
                p(0.7, 0.0),
                p(1.0, 0.5),
                p(0.7, 1.0),
                p(0.7, 0.75),
                p(0.3, 0.75),
                p(0.3, 1.0),
            ],
            ShapeKind::Chevron => vec![
                p(0.0, 0.0),
                p(0.7, 0.0),
                p(1.0, 0.5),
                p(0.7, 1.0),
                p(0.0, 1.0),
                p(0.3, 0.5),
            ],
            ShapeKind::Cloud => (0..96)
                .map(|i| {
                    let t = (i as f32 / 96.0) * std::f32::consts::TAU;
                    let r = 0.5 * (0.86 + 0.14 * (5.0 * t).cos().abs());
                    p(0.5 + r * t.cos(), 0.5 + r * t.sin())
                })
                .collect(),
            ShapeKind::Callout => {
                let mut v = vec![
                    p(0.0, 0.0),
                    p(1.0, 0.0),
                    p(1.0, 0.75),
                    p(0.45, 0.75),
                    p(0.2, 1.0),
                    p(0.25, 0.75),
                    p(0.0, 0.75),
                ];
                v.dedup();
                v
            }
            ShapeKind::Cylinder => {
                let ry = 0.12;
                let mut v = ellipse_arc(0.5, ry, 0.5, ry, 180.0, 360.0, 16);
                v.extend(ellipse_arc(0.5, 1.0 - ry, 0.5, ry, 0.0, 180.0, 16));
                v
            }
            ShapeKind::Document => {
                let mut v = vec![p(0.0, 0.0), p(1.0, 0.0)];
                v.extend((0..=24).map(|i| {
                    let x = 1.0 - i as f32 / 24.0;
                    p(x, 0.88 - 0.08 * (x * std::f32::consts::TAU).sin())
                }));
                v
            }
            ShapeKind::ManualInput => vec![p(0.0, 0.25), p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
            ShapeKind::Delay => {
                let mut v = vec![p(0.0, 0.0), p(0.5, 0.0)];
                v.extend(ellipse_arc(0.5, 0.5, 0.5, 0.5, -90.0, 90.0, 24));
                v.push(p(0.0, 1.0));
                v.dedup();
                v
            }
            ShapeKind::OffPageConnector => {
                vec![
                    p(0.0, 0.0),
                    p(1.0, 0.0),
                    p(1.0, 0.7),
                    p(0.5, 1.0),
                    p(0.0, 0.7),
                ]
            }
        }
    }

    /// Extra open polylines drawn inside the outline (unit square), e.g. the front rim
    /// of a cylinder or the bars of a predefined process.
    pub fn unit_details(self) -> Vec<Vec<Vec2>> {
        let p = Vec2::new;
        match self {
            ShapeKind::Cylinder => vec![ellipse_arc(0.5, 0.12, 0.5, 0.12, 0.0, 180.0, 16)],
            ShapeKind::PredefinedProcess => vec![
                vec![p(0.1, 0.0), p(0.1, 1.0)],
                vec![p(0.9, 0.0), p(0.9, 1.0)],
            ],
            _ => Vec::new(),
        }
    }

    /// Text area in the unit square.
    pub fn unit_label_rect(self) -> Rect {
        let (x0, y0, x1, y1) = match self {
            ShapeKind::Ellipse | ShapeKind::Cloud => (0.15, 0.15, 0.85, 0.85),
            ShapeKind::Diamond | ShapeKind::Decision => (0.25, 0.25, 0.75, 0.75),
            ShapeKind::Triangle => (0.25, 0.45, 0.75, 0.95),
            ShapeKind::RightTriangle => (0.05, 0.5, 0.5, 0.95),
            ShapeKind::Parallelogram | ShapeKind::Data | ShapeKind::Trapezoid => {
                (0.2, 0.05, 0.8, 0.95)
            }
            ShapeKind::Star => (0.3, 0.3, 0.7, 0.75),
            ShapeKind::Plus => (0.34, 0.34, 0.66, 0.66),
            ShapeKind::Arrow => (0.05, 0.25, 0.7, 0.75),
            ShapeKind::DoubleArrow => (0.3, 0.25, 0.7, 0.75),
            ShapeKind::Chevron => (0.3, 0.05, 0.75, 0.95),
            ShapeKind::Callout => (0.05, 0.05, 0.95, 0.7),
            ShapeKind::Cylinder => (0.05, 0.25, 0.95, 0.9),
            ShapeKind::Document => (0.05, 0.05, 0.95, 0.8),
            ShapeKind::PredefinedProcess => (0.12, 0.05, 0.88, 0.95),
            ShapeKind::Pentagon | ShapeKind::Hexagon | ShapeKind::Octagon => {
                (0.15, 0.15, 0.85, 0.85)
            }
            ShapeKind::Preparation => (0.15, 0.05, 0.85, 0.95),
            ShapeKind::ManualInput => (0.05, 0.3, 0.95, 0.95),
            ShapeKind::Delay => (0.05, 0.1, 0.8, 0.9),
            ShapeKind::OffPageConnector => (0.05, 0.05, 0.95, 0.7),
            ShapeKind::Terminator => (0.2, 0.05, 0.8, 0.95),
            _ => (0.05, 0.05, 0.95, 0.95),
        };
        Rect::from_min_size(Vec2::new(x0, y0), Vec2::new(x1 - x0, y1 - y0))
    }
}

fn rounded_rect(rx: f32, ry: f32) -> Vec<Vec2> {
    let mut v = Vec::new();
    v.extend(ellipse_arc(1.0 - rx, ry, rx, ry, -90.0, 0.0, 8));
    v.extend(ellipse_arc(1.0 - rx, 1.0 - ry, rx, ry, 0.0, 90.0, 8));
    v.extend(ellipse_arc(rx, 1.0 - ry, rx, ry, 90.0, 180.0, 8));
    v.extend(ellipse_arc(rx, ry, rx, ry, 180.0, 270.0, 8));
    v.dedup();
    v
}

/// Points of an elliptical arc from `start` to `end` degrees (inclusive unless a full turn).
fn ellipse_arc(cx: f32, cy: f32, rx: f32, ry: f32, start: f32, end: f32, n: usize) -> Vec<Vec2> {
    let full = (end - start).abs() >= 360.0;
    let count = if full { n } else { n + 1 };
    (0..count)
        .map(|i| {
            let a = (start + (end - start) * i as f32 / n as f32).to_radians();
            Vec2::new(cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

fn regular_polygon(n: usize, start: f32) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let a = (start + 360.0 * i as f32 / n as f32).to_radians();
            Vec2::new(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin())
        })
        .collect()
}

/// Shape geometry in sheet coordinates, already rotated.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeGeometry {
    /// Closed outline polygon.
    pub outline: Vec<Vec2>,
    /// Open interior polylines.
    pub details: Vec<Vec<Vec2>>,
}

/// Maps a unit-square shape onto `rect` and rotates it by `rotation` degrees around the
/// centre. Shared by rendering, hit-testing and export.
pub fn shape_geometry(kind: ShapeKind, rect: Rect, rotation: f32) -> ShapeGeometry {
    let center = rect.center();
    let map = |u: Vec2| {
        rotate_around(
            Vec2::new(
                rect.min.x + u.x * rect.size.x,
                rect.min.y + u.y * rect.size.y,
            ),
            center,
            rotation,
        )
    };
    ShapeGeometry {
        outline: kind.unit_outline().into_iter().map(map).collect(),
        details: kind
            .unit_details()
            .into_iter()
            .map(|line| line.into_iter().map(map).collect())
            .collect(),
    }
}

/// Unrotated text area of a shape inside `rect`.
pub fn shape_label_rect(kind: ShapeKind, rect: Rect) -> Rect {
    let u = kind.unit_label_rect();
    Rect::from_min_size(
        Vec2::new(
            rect.min.x + u.min.x * rect.size.x,
            rect.min.y + u.min.y * rect.size.y,
        ),
        Vec2::new(u.size.x * rect.size.x, u.size.y * rect.size.y),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InkTool {
    #[default]
    Pen,
    Highlighter,
}

/// A freehand stroke. Point data is shared immutably so undo snapshots copy references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InkStroke {
    #[serde(default)]
    pub tool: InkTool,
    pub color: Color,
    pub width: f32,
    /// Absolute sheet coordinates (unrotated frame).
    pub points: Arc<[Vec2]>,
    /// Optional pressure per point (`0..=1`), same length as `points`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<Arc<[f32]>>,
}

impl InkStroke {
    pub fn new(tool: InkTool, color: Color, width: f32, points: Vec<Vec2>) -> Self {
        Self {
            tool,
            color,
            width,
            points: points.into(),
            pressure: None,
        }
    }
}

/// End of a free connector.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Endpoint {
    Topic(TopicId),
    Object(CanvasObjectId),
    Point(Vec2),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectorRoute {
    #[default]
    Straight,
    Elbow,
    Curve,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasConnector {
    pub from: Endpoint,
    pub to: Endpoint,
    #[serde(default)]
    pub route: ConnectorRoute,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub arrow_start: bool,
    #[serde(default = "default_true")]
    pub arrow_end: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(default = "default_connector_width")]
    pub width: f32,
}

fn default_true() -> bool {
    true
}

fn default_connector_width() -> f32 {
    2.0
}

impl CanvasConnector {
    pub fn new(from: Endpoint, to: Endpoint) -> Self {
        Self {
            from,
            to,
            route: ConnectorRoute::Straight,
            arrow_start: false,
            arrow_end: true,
            label: String::new(),
            color: None,
            width: default_connector_width(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CardKind {
    #[default]
    Sticky,
    Text,
    Image,
}

/// Card presentation of a floating root topic: fixed size, text wraps inside.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Card {
    #[serde(default)]
    pub kind: CardKind,
    pub size: Vec2,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

impl Card {
    pub fn new(kind: CardKind, size: Vec2) -> Self {
        Self {
            kind,
            size,
            rotation: 0.0,
            color: None,
        }
    }
}

/// Root projection policy for outline-based consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RootProjection {
    /// Hidden-root sheets get a synthesized root titled with the sheet title.
    #[default]
    Synthesize,
}

/// Z-order command within a band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZOrder {
    Forward,
    Backward,
    Front,
    Back,
}

// ---------------------------------------------------------------------------------------
// Geometry helpers

/// Normalises degrees to `[0, 360)`; non-finite input becomes `0`.
pub fn normalize_rotation(degrees: f32) -> f32 {
    if !degrees.is_finite() {
        return 0.0;
    }
    let r = degrees.rem_euclid(360.0);
    if r >= 360.0 { 0.0 } else { r }
}

/// Rotates `p` clockwise on screen (y down) by `degrees` around `center`.
pub fn rotate_around(p: Vec2, center: Vec2, degrees: f32) -> Vec2 {
    if degrees == 0.0 {
        return p;
    }
    let (s, c) = degrees.to_radians().sin_cos();
    let d = p - center;
    Vec2::new(center.x + d.x * c - d.y * s, center.y + d.x * s + d.y * c)
}

/// Corners of `rect` rotated around its centre (top-left, top-right, bottom-right, bottom-left).
pub fn rotated_corners(rect: Rect, degrees: f32) -> [Vec2; 4] {
    let c = rect.center();
    let m = rect.max();
    [
        rect.min,
        Vec2::new(m.x, rect.min.y),
        m,
        Vec2::new(rect.min.x, m.y),
    ]
    .map(|p| rotate_around(p, c, degrees))
}

/// Axis-aligned bounds of `rect` rotated around its centre.
pub fn rotated_bounds(rect: Rect, degrees: f32) -> Rect {
    if degrees == 0.0 {
        return rect;
    }
    points_bounds(&rotated_corners(rect, degrees)).unwrap_or(rect)
}

/// `true` if `p` lies inside `rect` rotated by `degrees` around its centre.
pub fn rotated_rect_contains(rect: Rect, degrees: f32, p: Vec2) -> bool {
    rect.contains(rotate_around(p, rect.center(), -degrees))
}

pub fn points_bounds(points: &[Vec2]) -> Option<Rect> {
    let first = *points.first()?;
    let (mut min, mut max) = (first, first);
    for p in &points[1..] {
        min = Vec2::new(min.x.min(p.x), min.y.min(p.y));
        max = Vec2::new(max.x.max(p.x), max.y.max(p.y));
    }
    Some(Rect::from_min_size(min, max - min))
}

pub fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 <= f32::EPSILON {
        return (p - a).length();
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// Distance to an open polyline (a single point counts as a dot).
pub fn distance_to_polyline(p: Vec2, points: &[Vec2]) -> f32 {
    match points {
        [] => f32::INFINITY,
        [a] => (p - *a).length(),
        _ => points
            .windows(2)
            .map(|w| distance_to_segment(p, w[0], w[1]))
            .fold(f32::INFINITY, f32::min),
    }
}

/// Even-odd point in polygon test.
pub fn point_in_polygon(p: Vec2, polygon: &[Vec2]) -> bool {
    let mut inside = false;
    let n = polygon.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Distance to the closed outline of `polygon`.
pub fn distance_to_outline(p: Vec2, polygon: &[Vec2]) -> f32 {
    if polygon.len() < 2 {
        return distance_to_polyline(p, polygon);
    }
    let closing = distance_to_segment(p, polygon[polygon.len() - 1], polygon[0]);
    distance_to_polyline(p, polygon).min(closing)
}

fn valid_coordinate(v: f32) -> bool {
    v.is_finite() && v.abs() <= MAX_BOARD_COORDINATE
}

fn valid_rect(rect: Rect) -> bool {
    valid_position(rect.min)
        && valid_coordinate(rect.size.x)
        && valid_coordinate(rect.size.y)
        && rect.size.x >= 0.0
        && rect.size.y >= 0.0
        && valid_position(rect.max())
}

fn valid_width(w: f32) -> bool {
    w.is_finite() && (MIN_STROKE_WIDTH..=MAX_STROKE_WIDTH).contains(&w)
}

fn valid_text(text: &str, what: &str) -> Result<(), String> {
    if text.chars().count() > MAX_BOARD_TEXT_CHARS {
        return Err(format!("{what} exceeds {MAX_BOARD_TEXT_CHARS} characters"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Validation

/// Validates one object in isolation (ranges, sizes, text lengths, ink data).
pub fn validate_object(object: &CanvasObject) -> Result<(), String> {
    let id = object.id;
    if !valid_rect(object.rect) {
        return Err(format!("canvas object {id} has an invalid frame"));
    }
    if !object.rotation.is_finite() || !(0.0..360.0).contains(&object.rotation) {
        return Err(format!("canvas object {id} has an invalid rotation"));
    }
    valid_text(&object.alt_text, "alt text")?;
    let style = &object.style;
    if !valid_width(style.stroke_width) {
        return Err(format!("canvas object {id} has an invalid stroke width"));
    }
    if !style.opacity.is_finite() || !(0.0..=1.0).contains(&style.opacity) {
        return Err(format!("canvas object {id} has an invalid opacity"));
    }
    match &object.kind {
        CanvasObjectKind::Shape { label, .. } => valid_text(label, "shape label")?,
        CanvasObjectKind::Ink(ink) => {
            if ink.points.is_empty() {
                return Err(format!("ink stroke {id} has no points"));
            }
            if ink.points.len() > MAX_STROKE_POINTS {
                return Err(format!("ink stroke exceeds {MAX_STROKE_POINTS} points"));
            }
            if !valid_width(ink.width) {
                return Err(format!("ink stroke {id} has an invalid width"));
            }
            if ink.points.iter().any(|p| !valid_position(*p)) {
                return Err(format!("ink stroke {id} has an invalid point"));
            }
            if let Some(pressure) = &ink.pressure
                && (pressure.len() != ink.points.len()
                    || pressure
                        .iter()
                        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
            {
                return Err(format!("ink stroke {id} has invalid pressure data"));
            }
        }
        CanvasObjectKind::Connector(c) => {
            valid_text(&c.label, "connector label")?;
            if !valid_width(c.width) {
                return Err(format!("connector {id} has an invalid width"));
            }
            for end in [c.from, c.to] {
                match end {
                    Endpoint::Point(p) if !valid_position(p) => {
                        return Err(format!("connector {id} has an invalid end point"));
                    }
                    Endpoint::Object(o) if o == id => {
                        return Err(format!("connector {id} is connected to itself"));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Validates a card in isolation.
pub fn validate_card(card: &Card) -> Result<(), String> {
    if !card.size.x.is_finite()
        || !card.size.y.is_finite()
        || card.size.x < MIN_CARD_SIZE
        || card.size.y < MIN_CARD_SIZE
        || card.size.x > MAX_BOARD_COORDINATE
        || card.size.y > MAX_BOARD_COORDINATE
    {
        return Err("card size must be finite and at least 16 by 16".into());
    }
    if !card.rotation.is_finite() || !(0.0..360.0).contains(&card.rotation) {
        return Err("card rotation is invalid".into());
    }
    Ok(())
}

/// Board limits and invariants of a sheet: root mode/anchor, cards, board settings and
/// the canvas layer. Called by [`Sheet::validate`] (import, paste) and by every
/// [`crate::Document`] edit.
pub fn validate_board(sheet: &Sheet) -> Result<(), String> {
    let board = &sheet.board;
    if !board.grid_size.is_finite() || !(MIN_GRID_SIZE..=MAX_GRID_SIZE).contains(&board.grid_size) {
        return Err("board grid size is out of range".into());
    }
    if sheet.root_mode == SheetRootMode::Hidden {
        validate_anchor(sheet)?;
    }
    for (id, topic) in &sheet.topics {
        if let Some(card) = &topic.card {
            validate_card(card)?;
            if *id == sheet.root || topic.parent.is_some() || !sheet.floating.contains(id) {
                return Err(format!("card topic {id} is not a floating root"));
            }
        }
    }
    let canvas = &sheet.canvas;
    if canvas.objects.len() > MAX_CANVAS_OBJECTS {
        return Err(format!("canvas exceeds {MAX_CANVAS_OBJECTS} objects"));
    }
    if canvas.groups.len() > MAX_CANVAS_GROUPS {
        return Err(format!("canvas exceeds {MAX_CANVAS_GROUPS} groups"));
    }
    let mut groups = HashSet::new();
    for g in &canvas.groups {
        if !groups.insert(g.id) {
            return Err(format!("duplicate canvas group id {}", g.id));
        }
    }
    let mut ids = HashSet::new();
    let mut total_points = 0usize;
    for o in &canvas.objects {
        if !ids.insert(o.id) {
            return Err(format!("duplicate canvas object id {}", o.id));
        }
        validate_object(o)?;
        if let CanvasObjectKind::Ink(ink) = &o.kind {
            total_points = total_points.saturating_add(ink.points.len());
            if total_points > MAX_TOTAL_INK_POINTS {
                return Err(format!(
                    "ink exceeds {MAX_TOTAL_INK_POINTS} points per sheet"
                ));
            }
        }
        if let Some(g) = o.group
            && !groups.contains(&g)
        {
            return Err(format!(
                "canvas object {} references missing group {g}",
                o.id
            ));
        }
    }
    for o in &canvas.objects {
        if let CanvasObjectKind::Connector(c) = &o.kind {
            for end in [c.from, c.to] {
                match end {
                    Endpoint::Topic(t) if !sheet.contains(t) || sheet.is_anchor(t) => {
                        return Err(format!("connector {} references missing topic {t}", o.id));
                    }
                    Endpoint::Object(x) if !ids.contains(&x) => {
                        return Err(format!("connector {} references missing object {x}", o.id));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// In Hidden mode the root must not carry any content.
fn validate_anchor(sheet: &Sheet) -> Result<(), String> {
    if let Some(reason) = anchor_content(sheet) {
        return Err(format!("hidden central topic must be empty ({reason})"));
    }
    Ok(())
}

/// First kind of content that prevents the root from being an anchor.
fn anchor_content(sheet: &Sheet) -> Option<&'static str> {
    let root = sheet.root;
    let t = sheet.topic(root)?;
    let checks: [(bool, &'static str); 15] = [
        (!t.children.is_empty(), "children"),
        (t.notes.is_some(), "notes"),
        (!t.labels.is_empty(), "labels"),
        (!t.markers.is_empty(), "markers"),
        (t.image.is_some(), "image"),
        (t.attachment.is_some(), "attachment"),
        (t.todo.is_some(), "to-do"),
        (t.hyperlink.is_some(), "hyperlink"),
        (t.child_numbering.is_some(), "numbering"),
        (t.card.is_some(), "card"),
        (
            sheet
                .relationships
                .iter()
                .any(|r| r.from == root || r.to == root),
            "relationships",
        ),
        (
            sheet.boundaries.iter().any(|b| b.parent == root),
            "boundaries",
        ),
        (
            sheet.summaries.iter().any(|s| s.parent == root),
            "summaries",
        ),
        (
            sheet.scheduling.tasks.contains_key(&root)
                || sheet
                    .scheduling
                    .dependencies
                    .iter()
                    .any(|d| d.predecessor == root || d.successor == root),
            "tasks",
        ),
        (
            sheet
                .presentation
                .slides
                .iter()
                .any(|s| s.source == PresentationSource::Branch(root)),
            "slides",
        ),
    ];
    checks
        .into_iter()
        .find(|(present, _)| *present)
        .map(|(_, reason)| reason)
}

/// Masks free text of the canvas layer (see [`crate::Workbook::masked_copy`]).
pub(crate) fn mask_canvas(sheet: &mut Sheet) {
    for (index, object) in sheet.canvas.objects.iter_mut().enumerate() {
        object.alt_text.clear();
        match &mut object.kind {
            CanvasObjectKind::Shape { label, .. } if !label.is_empty() => {
                *label = format!("Shape {}", index + 1);
            }
            CanvasObjectKind::Connector(c) if !c.label.is_empty() => {
                c.label = format!("Connector {}", index + 1);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------------------
// Root queries and board edits

impl Sheet {
    /// `true` for the invisible root of a Hidden-mode sheet.
    pub fn is_anchor(&self, id: TopicId) -> bool {
        self.root_mode == SheetRootMode::Hidden && id == self.root
    }

    /// Content roots in reading order: `[root]` in Central mode; in Hidden mode the
    /// floating roots, zone members first (by zone order), then the rest, each by y, x.
    pub fn semantic_roots(&self) -> Vec<TopicId> {
        if self.root_mode == SheetRootMode::Central {
            return vec![self.root];
        }
        let pos = |id: &TopicId| self.topic(*id).and_then(|t| t.position).unwrap_or_default();
        let by_reading = |a: &TopicId, b: &TopicId| {
            let (pa, pb) = (pos(a), pos(b));
            pa.y.total_cmp(&pb.y)
                .then(pa.x.total_cmp(&pb.x))
                .then(a.cmp(b))
        };
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for zone in &self.zones {
            let mut members: Vec<_> = zone
                .roots
                .iter()
                .copied()
                .filter(|r| self.floating.contains(r) && seen.insert(*r))
                .collect();
            members.sort_by(by_reading);
            out.extend(members);
        }
        let mut rest: Vec<_> = self
            .floating
            .iter()
            .copied()
            .filter(|r| self.contains(*r) && seen.insert(*r))
            .collect();
        rest.sort_by(by_reading);
        out.extend(rest);
        out
    }

    /// The single root used by features that need exactly one (`None` on an empty board).
    pub fn primary_content_root(&self) -> Option<TopicId> {
        match self.root_mode {
            SheetRootMode::Central => Some(self.root),
            SheetRootMode::Hidden => self.semantic_roots().first().copied(),
        }
    }

    /// Root (Central mode only) followed by every floating root.
    pub fn all_content_roots(&self) -> Vec<TopicId> {
        let mut v = Vec::with_capacity(self.floating.len() + 1);
        if self.root_mode == SheetRootMode::Central {
            v.push(self.root);
        }
        v.extend(self.floating.iter().copied().filter(|f| self.contains(*f)));
        v
    }

    /// Topics in document order that are not the anchor and not hidden by a collapsed
    /// ancestor.
    pub fn visible_topic_ids(&self) -> Vec<TopicId> {
        self.all_ids_ordered()
            .into_iter()
            .filter(|id| !self.is_anchor(*id) && self.is_visible(*id))
            .collect()
    }

    pub fn alloc_canvas_object_id(&mut self) -> CanvasObjectId {
        CanvasObjectId(self.alloc())
    }

    pub fn alloc_canvas_group_id(&mut self) -> CanvasGroupId {
        CanvasGroupId(self.alloc())
    }

    /// Why the central topic cannot be hidden directly (`None` = it can).
    pub fn central_topic_content(&self) -> Option<&'static str> {
        anchor_content(self)
    }

    /// Switches to Hidden mode. If the root carries content, it is first moved into a new
    /// floating topic at the root position (children, notes, links, relationships,
    /// boundaries, summaries, tasks and connectors are retargeted; branch slides become
    /// viewport slides). Returns that new topic. Callers wrap this in one edit.
    pub fn hide_central_topic(&mut self) -> Result<Option<TopicId>, String> {
        if self.root_mode == SheetRootMode::Hidden {
            return Ok(None);
        }
        if self.fim.is_some() {
            return Err("FIM documents always show their central topic".into());
        }
        if anchor_content(self).is_none() {
            self.root_mode = SheetRootMode::Hidden;
            return Ok(None);
        }
        let root = self.root;
        let id = self.alloc_topic_id();
        let old = self.topics.get_mut(&root).expect("root exists");
        let mut moved = Topic::new(id, old.title.clone());
        moved.children = std::mem::take(&mut old.children);
        moved.collapsed = std::mem::take(&mut old.collapsed);
        moved.notes = old.notes.take();
        moved.labels = std::mem::take(&mut old.labels);
        moved.markers = std::mem::take(&mut old.markers);
        moved.marker_colors = std::mem::take(&mut old.marker_colors);
        moved.hyperlink = old.hyperlink.take();
        moved.style = old.style.clone();
        moved.structure = Some(old.structure.unwrap_or(self.structure));
        moved.todo = old.todo.take();
        moved.image = old.image.take();
        moved.attachment = old.attachment.take();
        moved.image_size = old.image_size.take();
        moved.image_below = std::mem::take(&mut old.image_below);
        moved.child_numbering = old.child_numbering.take();
        old.card = None;
        old.structure = None;
        moved.position = Some(Vec2::ZERO);
        for child in moved.children.clone() {
            if let Some(c) = self.topics.get_mut(&child) {
                c.parent = Some(id);
            }
        }
        self.topics.insert(id, moved);
        self.floating.push(id);
        for r in &mut self.relationships {
            if r.from == root {
                r.from = id;
            }
            if r.to == root {
                r.to = id;
            }
        }
        for b in &mut self.boundaries {
            if b.parent == root {
                b.parent = id;
            }
        }
        for s in &mut self.summaries {
            if s.parent == root {
                s.parent = id;
            }
        }
        if let Some(task) = self.scheduling.tasks.remove(&root) {
            self.scheduling.tasks.insert(id, task);
        }
        for d in &mut self.scheduling.dependencies {
            if d.predecessor == root {
                d.predecessor = id;
            }
            if d.successor == root {
                d.successor = id;
            }
        }
        for slide in &mut self.presentation.slides {
            if matches!(slide.source, PresentationSource::Branch(_)) {
                slide.source = PresentationSource::Viewport;
            }
        }
        for o in &mut self.canvas.objects {
            if let CanvasObjectKind::Connector(c) = &mut o.kind {
                for end in [&mut c.from, &mut c.to] {
                    if *end == Endpoint::Topic(root) {
                        *end = Endpoint::Topic(id);
                    }
                }
            }
        }
        self.root_mode = SheetRootMode::Hidden;
        Ok(Some(id))
    }

    /// Restores Central mode (the anchor becomes a visible, empty central topic).
    pub fn show_central_topic(&mut self) {
        self.root_mode = SheetRootMode::Central;
    }

    pub fn set_board_settings(&mut self, settings: BoardSettings) -> Result<(), String> {
        if !settings.grid_size.is_finite()
            || !(MIN_GRID_SIZE..=MAX_GRID_SIZE).contains(&settings.grid_size)
        {
            return Err("board grid size is out of range".into());
        }
        self.board = settings;
        Ok(())
    }

    /// Sets or clears the card presentation of a floating root.
    pub fn set_topic_card(&mut self, id: TopicId, card: Option<Card>) -> Result<(), String> {
        if let Some(card) = &card {
            validate_card(card)?;
            if id == self.root || self.parent(id).is_some() || !self.floating.contains(&id) {
                return Err(format!("topic {id} is not a floating root"));
            }
        }
        let topic = self
            .topic_mut(id)
            .ok_or_else(|| format!("topic {id} not found"))?;
        topic.card = card.map(|mut c| {
            c.rotation = normalize_rotation(c.rotation);
            c
        });
        Ok(())
    }

    /// Adds a floating card topic centred at `center`.
    pub fn add_card(&mut self, title: &str, center: Vec2, card: Card) -> Result<TopicId, String> {
        validate_card(&card)?;
        if !valid_position(center) {
            return Err("card position is out of range".into());
        }
        let id = self.add_floating(title, center);
        self.set_topic_card(id, Some(card))?;
        Ok(id)
    }

    /// Inserts `object` (its id is replaced by a fresh one) on top of its band.
    pub fn add_canvas_object(
        &mut self,
        mut object: CanvasObject,
    ) -> Result<CanvasObjectId, String> {
        if self.canvas.objects.len() >= MAX_CANVAS_OBJECTS {
            return Err(format!("canvas exceeds {MAX_CANVAS_OBJECTS} objects"));
        }
        object.id = self.alloc_canvas_object_id();
        object.rotation = normalize_rotation(object.rotation);
        object.group = None;
        validate_object(&object)?;
        if let CanvasObjectKind::Ink(ink) = &object.kind
            && self
                .canvas
                .ink_point_count()
                .saturating_add(ink.points.len())
                > MAX_TOTAL_INK_POINTS
        {
            return Err(format!(
                "ink exceeds {MAX_TOTAL_INK_POINTS} points per sheet"
            ));
        }
        if let CanvasObjectKind::Connector(c) = &object.kind {
            for end in [c.from, c.to] {
                self.check_endpoint(end)?;
            }
        }
        let id = object.id;
        self.canvas.objects.push(object);
        Ok(id)
    }

    fn check_endpoint(&self, end: Endpoint) -> Result<(), String> {
        match end {
            Endpoint::Topic(t) if !self.contains(t) || self.is_anchor(t) => {
                Err(format!("topic {t} not found"))
            }
            Endpoint::Object(o) if self.canvas.object(o).is_none() => {
                Err(format!("canvas object {o} not found"))
            }
            Endpoint::Point(p) if !valid_position(p) => Err("connector point out of range".into()),
            _ => Ok(()),
        }
    }

    fn require_unlocked(&self, ids: &[CanvasObjectId]) -> Result<(), String> {
        for id in ids {
            if self.canvas.object(*id).is_none() {
                return Err(format!("canvas object {id} not found"));
            }
            if self.canvas.is_locked(*id) {
                return Err(format!("canvas object {id} is locked"));
            }
        }
        Ok(())
    }

    /// Removes objects. Connectors attached to them keep a free end at the removed
    /// object's centre; emptied groups are dissolved.
    pub fn remove_canvas_objects(&mut self, ids: &[CanvasObjectId]) -> Result<(), String> {
        if ids.is_empty() {
            return Err("nothing selected".into());
        }
        self.require_unlocked(ids)?;
        let removed: HashSet<_> = ids.iter().copied().collect();
        let centers: HashMap<_, _> = self
            .canvas
            .objects
            .iter()
            .filter(|o| removed.contains(&o.id))
            .map(|o| (o.id, o.rect.center()))
            .collect();
        self.canvas.objects.retain(|o| !removed.contains(&o.id));
        for o in &mut self.canvas.objects {
            if let CanvasObjectKind::Connector(c) = &mut o.kind {
                for end in [&mut c.from, &mut c.to] {
                    if let Endpoint::Object(x) = *end
                        && let Some(center) = centers.get(&x)
                    {
                        *end = Endpoint::Point(*center);
                    }
                }
            }
        }
        self.prune_canvas_groups();
        Ok(())
    }

    /// Moves objects (and the free ends of moved connectors) by `delta`.
    pub fn move_canvas_objects(
        &mut self,
        ids: &[CanvasObjectId],
        delta: Vec2,
    ) -> Result<(), String> {
        if !valid_position(delta) {
            return Err("movement out of range".into());
        }
        self.require_unlocked(ids)?;
        let set: HashSet<_> = ids.iter().copied().collect();
        let mut candidate = self.canvas.clone();
        for o in candidate.objects.iter_mut().filter(|o| set.contains(&o.id)) {
            o.rect = o.rect.translate(delta);
            match &mut o.kind {
                CanvasObjectKind::Ink(ink) => {
                    ink.points = ink.points.iter().map(|p| *p + delta).collect();
                }
                CanvasObjectKind::Connector(c) => {
                    for end in [&mut c.from, &mut c.to] {
                        if let Endpoint::Point(p) = end {
                            *p = *p + delta;
                        }
                    }
                }
                CanvasObjectKind::Shape { .. } => {}
            }
            validate_object(o)?;
        }
        self.canvas = candidate;
        Ok(())
    }

    /// Sets the unrotated frame; ink points are scaled with it.
    pub fn resize_canvas_object(&mut self, id: CanvasObjectId, rect: Rect) -> Result<(), String> {
        self.require_unlocked(&[id])?;
        if !valid_rect(rect) {
            return Err("object frame out of range".into());
        }
        let mut object = self.canvas.object(id).cloned().expect("checked");
        let old = object.rect;
        let scale = |v: f32, min: f32, size: f32, new_min: f32, new_size: f32| {
            if size.abs() <= f32::EPSILON {
                new_min + (v - min)
            } else {
                new_min + (v - min) * new_size / size
            }
        };
        if let CanvasObjectKind::Ink(ink) = &mut object.kind {
            ink.points = ink
                .points
                .iter()
                .map(|p| {
                    Vec2::new(
                        scale(p.x, old.min.x, old.size.x, rect.min.x, rect.size.x),
                        scale(p.y, old.min.y, old.size.y, rect.min.y, rect.size.y),
                    )
                })
                .collect();
        }
        object.rect = rect;
        validate_object(&object)?;
        *self.canvas.object_mut(id).expect("checked") = object;
        Ok(())
    }

    /// Sets the absolute rotation in degrees (normalised). Connectors cannot rotate.
    pub fn rotate_canvas_object(&mut self, id: CanvasObjectId, degrees: f32) -> Result<(), String> {
        self.require_unlocked(&[id])?;
        if !degrees.is_finite() {
            return Err("rotation must be finite".into());
        }
        let object = self.canvas.object_mut(id).expect("checked");
        if object.is_connector() {
            return Err("connectors cannot be rotated".into());
        }
        object.rotation = normalize_rotation(degrees);
        Ok(())
    }

    /// Applies `f` to one object and validates the result (style, label, alt text, ...).
    /// Id, group and lock state cannot be changed this way.
    pub fn update_canvas_object(
        &mut self,
        id: CanvasObjectId,
        f: impl FnOnce(&mut CanvasObject),
    ) -> Result<(), String> {
        let mut object = self
            .canvas
            .object(id)
            .cloned()
            .ok_or_else(|| format!("canvas object {id} not found"))?;
        let (group, locked) = (object.group, object.locked);
        f(&mut object);
        object.id = id;
        object.group = group;
        object.locked = locked;
        object.rotation = normalize_rotation(object.rotation);
        validate_object(&object)?;
        if let CanvasObjectKind::Connector(c) = &object.kind {
            for end in [c.from, c.to] {
                self.check_endpoint(end)?;
            }
        }
        if let CanvasObjectKind::Ink(ink) = &object.kind {
            let others = self.canvas.ink_point_count()
                - match &self.canvas.object(id).expect("checked").kind {
                    CanvasObjectKind::Ink(old) => old.points.len(),
                    _ => 0,
                };
            if others.saturating_add(ink.points.len()) > MAX_TOTAL_INK_POINTS {
                return Err(format!(
                    "ink exceeds {MAX_TOTAL_INK_POINTS} points per sheet"
                ));
            }
        }
        *self.canvas.object_mut(id).expect("checked") = object;
        Ok(())
    }

    /// Changes the paint order of `ids` within their bands.
    pub fn reorder_canvas_objects(
        &mut self,
        ids: &[CanvasObjectId],
        order: ZOrder,
    ) -> Result<(), String> {
        let set: HashSet<_> = ids.iter().copied().collect();
        if set.is_empty() || set.iter().any(|id| self.canvas.object(*id).is_none()) {
            return Err("canvas object not found".into());
        }
        let objects = &mut self.canvas.objects;
        match order {
            ZOrder::Front | ZOrder::Back => {
                let (picked, rest): (Vec<_>, Vec<_>) = std::mem::take(objects)
                    .into_iter()
                    .partition(|o| set.contains(&o.id));
                *objects = if order == ZOrder::Front {
                    rest.into_iter().chain(picked).collect()
                } else {
                    picked.into_iter().chain(rest).collect()
                };
            }
            ZOrder::Forward | ZOrder::Backward => {
                for band in [Band::Below, Band::Above] {
                    let slots: Vec<usize> = objects
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| o.band == band)
                        .map(|(i, _)| i)
                        .collect();
                    let n = slots.len();
                    if n < 2 {
                        continue;
                    }
                    if order == ZOrder::Forward {
                        for k in (0..n - 1).rev() {
                            if set.contains(&objects[slots[k]].id)
                                && !set.contains(&objects[slots[k + 1]].id)
                            {
                                objects.swap(slots[k], slots[k + 1]);
                            }
                        }
                    } else {
                        for k in 1..n {
                            if set.contains(&objects[slots[k]].id)
                                && !set.contains(&objects[slots[k - 1]].id)
                            {
                                objects.swap(slots[k], slots[k - 1]);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Moves objects to another band, next to the map: on top of `Below` or at the
    /// bottom of `Above`.
    pub fn set_canvas_band(&mut self, ids: &[CanvasObjectId], band: Band) -> Result<(), String> {
        let set: HashSet<_> = ids.iter().copied().collect();
        if set.is_empty() || set.iter().any(|id| self.canvas.object(*id).is_none()) {
            return Err("canvas object not found".into());
        }
        let (mut picked, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.canvas.objects)
            .into_iter()
            .partition(|o| set.contains(&o.id) && o.band != band);
        for o in &mut picked {
            o.band = band;
        }
        self.canvas.objects = match band {
            Band::Below => rest.into_iter().chain(picked).collect(),
            Band::Above => picked.into_iter().chain(rest).collect(),
        };
        Ok(())
    }

    /// Groups at least two objects into a new group (previous groups are left).
    pub fn group_canvas_objects(
        &mut self,
        ids: &[CanvasObjectId],
    ) -> Result<CanvasGroupId, String> {
        let set: HashSet<_> = ids.iter().copied().collect();
        if set.len() < 2 {
            return Err("select at least two objects to group".into());
        }
        if set.iter().any(|id| self.canvas.object(*id).is_none()) {
            return Err("canvas object not found".into());
        }
        let id = self.alloc_canvas_group_id();
        self.canvas.groups.push(CanvasGroup { id, locked: false });
        for o in self
            .canvas
            .objects
            .iter_mut()
            .filter(|o| set.contains(&o.id))
        {
            o.group = Some(id);
        }
        self.prune_canvas_groups();
        if self.canvas.groups.len() > MAX_CANVAS_GROUPS {
            return Err(format!("canvas exceeds {MAX_CANVAS_GROUPS} groups"));
        }
        Ok(id)
    }

    /// Dissolves a group; its members keep their position and order.
    pub fn ungroup_canvas_objects(&mut self, group: CanvasGroupId) -> Result<(), String> {
        if self.canvas.group(group).is_none() {
            return Err(format!("canvas group {group} not found"));
        }
        for o in &mut self.canvas.objects {
            if o.group == Some(group) {
                o.group = None;
            }
        }
        self.canvas.groups.retain(|g| g.id != group);
        Ok(())
    }

    pub fn set_canvas_objects_locked(
        &mut self,
        ids: &[CanvasObjectId],
        locked: bool,
    ) -> Result<(), String> {
        let set: HashSet<_> = ids.iter().copied().collect();
        if set.is_empty() || set.iter().any(|id| self.canvas.object(*id).is_none()) {
            return Err("canvas object not found".into());
        }
        for o in self
            .canvas
            .objects
            .iter_mut()
            .filter(|o| set.contains(&o.id))
        {
            o.locked = locked;
        }
        Ok(())
    }

    pub fn set_canvas_group_locked(
        &mut self,
        group: CanvasGroupId,
        locked: bool,
    ) -> Result<(), String> {
        let g = self
            .canvas
            .groups
            .iter_mut()
            .find(|g| g.id == group)
            .ok_or_else(|| format!("canvas group {group} not found"))?;
        g.locked = locked;
        Ok(())
    }

    /// Dissolves groups with fewer than two members.
    fn prune_canvas_groups(&mut self) {
        let mut counts: BTreeMap<CanvasGroupId, usize> = BTreeMap::new();
        for o in &self.canvas.objects {
            if let Some(g) = o.group {
                *counts.entry(g).or_default() += 1;
            }
        }
        let alive: HashSet<_> = self
            .canvas
            .groups
            .iter()
            .map(|g| g.id)
            .filter(|g| counts.get(g).copied().unwrap_or(0) >= 2)
            .collect();
        self.canvas.groups.retain(|g| alive.contains(&g.id));
        for o in &mut self.canvas.objects {
            if o.group.is_some_and(|g| !alive.contains(&g)) {
                o.group = None;
            }
        }
    }

    /// `true` if a connector references a topic or object missing from this sheet.
    pub fn has_dangling_endpoints(&self) -> bool {
        let objects: HashSet<_> = self.canvas.objects.iter().map(|o| o.id).collect();
        self.canvas.objects.iter().any(|o| match &o.kind {
            CanvasObjectKind::Connector(c) => [c.from, c.to].into_iter().any(|end| match end {
                Endpoint::Topic(t) => !self.contains(t) || self.is_anchor(t),
                Endpoint::Object(x) => !objects.contains(&x),
                Endpoint::Point(_) => false,
            }),
            _ => false,
        })
    }

    /// Converts connector ends whose topic or object was deleted into free points at the
    /// target's centre in `before` (the sheet state before the deletion). Used by every
    /// deletion path so a deletion and its endpoint conversion form one edit.
    /// Returns the number of converted ends.
    pub fn fix_dangling_endpoints(&mut self, before: &Sheet) -> usize {
        if !self.has_dangling_endpoints() {
            return 0;
        }
        let layout = crate::layout::layout_sheet(before, &crate::layout::ApproxMeasure, false);
        let objects: HashSet<_> = self.canvas.objects.iter().map(|o| o.id).collect();
        let mut converted = 0;
        let alive_topic = |t: TopicId| self.contains(t) && !self.is_anchor(t);
        let mut ends = Vec::new();
        for (index, o) in self.canvas.objects.iter().enumerate() {
            let CanvasObjectKind::Connector(c) = &o.kind else {
                continue;
            };
            for (side, end) in [c.from, c.to].into_iter().enumerate() {
                let center = match end {
                    Endpoint::Topic(t) if !alive_topic(t) => Some(
                        crate::scene::topic_anchor_rect(before, &layout, t)
                            .map(|r| r.center())
                            .or_else(|| before.topic(t).and_then(|t| t.position))
                            .unwrap_or(o.rect.center()),
                    ),
                    Endpoint::Object(x) if !objects.contains(&x) => Some(
                        before
                            .canvas
                            .object(x)
                            .map(|b| b.rect.center())
                            .unwrap_or(o.rect.center()),
                    ),
                    _ => None,
                };
                if let Some(center) = center {
                    ends.push((index, side, center));
                }
            }
        }
        for (index, side, center) in ends {
            if let CanvasObjectKind::Connector(c) = &mut self.canvas.objects[index].kind {
                let end = if side == 0 { &mut c.from } else { &mut c.to };
                *end = Endpoint::Point(center);
                converted += 1;
            }
        }
        converted
    }

    /// Zone of a floating root, if any.
    pub fn zone_of(&self, id: TopicId) -> Option<ZoneId> {
        self.zones
            .iter()
            .find(|z| z.roots.contains(&id))
            .map(|z| z.id)
    }
}

#[cfg(test)]
mod tests;
