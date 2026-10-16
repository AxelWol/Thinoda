//! Automatic layout of mind-map structures.
//!
//! Each subtree is laid out recursively into a [`Block`] relative to its root topic center,
//! which is then translated into place. This supports mixed structures (a subtree may use a
//! different structure than its parent). The result is toolkit independent and used by the
//! canvas renderer, hit testing, keyboard navigation and SVG/PNG export.

use crate::geometry::{Rect, Vec2};
use crate::model::{
    BoundaryId, LineShape, RelationshipId, Sheet, Structure, SummaryId, TopicId, TopicShape,
};
use crate::theme::{Level, ResolvedStyle, level_of, resolve_style};
use std::collections::HashMap;

/// Text measuring backend (the GUI uses real font metrics, export uses [`ApproxMeasure`]).
pub trait TextMeasure {
    /// Size of `text` wrapped at `max_width`.
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> Vec2;
}

/// Font-independent approximation of text metrics.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApproxMeasure;

impl ApproxMeasure {
    pub const LINE_HEIGHT: f32 = 1.25;

    pub fn char_width(c: char, font_size: f32, bold: bool) -> f32 {
        let f = if is_wide(c) {
            1.0
        } else if c == ' ' || "il.,;:'|!".contains(c) {
            0.3
        } else if c.is_uppercase() || "mwMW".contains(c) {
            0.68
        } else {
            0.55
        };
        f * font_size * if bold { 1.06 } else { 1.0 }
    }

    /// Greedy word wrap; returns the lines.
    pub fn wrap(text: &str, font_size: f32, bold: bool, max_width: f32) -> Vec<(String, f32)> {
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            let mut w = 0.0;
            for word in split_keep_spaces(para) {
                let ww: f32 = word
                    .chars()
                    .map(|c| Self::char_width(c, font_size, bold))
                    .sum();
                if w + ww > max_width && !line.trim().is_empty() {
                    lines.push((line.trim_end().to_string(), w));
                    line = String::new();
                    w = 0.0;
                    if word.trim().is_empty() {
                        continue;
                    }
                }
                if ww > max_width {
                    // hard break very long words
                    for c in word.chars() {
                        let cw = Self::char_width(c, font_size, bold);
                        if w + cw > max_width && !line.is_empty() {
                            lines.push((std::mem::take(&mut line), w));
                            w = 0.0;
                        }
                        line.push(c);
                        w += cw;
                    }
                } else {
                    line.push_str(word);
                    w += ww;
                }
            }
            let tw: f32 = line
                .trim_end()
                .chars()
                .map(|c| Self::char_width(c, font_size, bold))
                .sum();
            lines.push((line.trim_end().to_string(), tw));
        }
        lines
    }
}

fn is_wide(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F300..=0x1FAFF)
}

fn split_keep_spaces(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == ' ' || is_wide(c) {
            if start < i {
                out.push(&s[start..i]);
            }
            out.push(&s[i..i + c.len_utf8()]);
            start = i + c.len_utf8();
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

impl TextMeasure for ApproxMeasure {
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> Vec2 {
        let lines = Self::wrap(text, font_size, bold, max_width);
        let w = lines.iter().map(|l| l.1).fold(0.0, f32::max);
        Vec2::new(
            w.max(font_size * 0.5),
            lines.len() as f32 * font_size * Self::LINE_HEIGHT,
        )
    }
}

/// Direction in which a topic's children grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dir {
    Right,
    Left,
    Down,
    Up,
}

impl Dir {
    pub fn is_horizontal(self) -> bool {
        matches!(self, Dir::Right | Dir::Left)
    }
}

/// Small indicator icons shown inside topics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Link,
    Notes,
}

/// Placed topic with its content regions (all in sheet coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct TopicLayout {
    pub id: TopicId,
    pub rect: Rect,
    pub level: Level,
    pub style: ResolvedStyle,
    pub grow: Dir,
    pub title_rect: Rect,
    pub image_rect: Option<Rect>,
    pub todo_rect: Option<Rect>,
    pub markers: Vec<(String, Rect)>,
    pub icons: Vec<(Icon, Rect)>,
    pub labels: Vec<(String, Rect)>,
    /// Number of descendants hidden because the topic is collapsed.
    pub hidden_children: usize,
}

impl TopicLayout {
    /// Position of the collapse/expand badge.
    pub fn badge_center(&self) -> Vec2 {
        let r = self.rect;
        let underline = self.style.shape == TopicShape::Underline;
        match self.grow {
            Dir::Right => Vec2::new(
                r.max().x + 9.0,
                if underline { r.max().y } else { r.center().y },
            ),
            Dir::Left => Vec2::new(
                r.min.x - 9.0,
                if underline { r.max().y } else { r.center().y },
            ),
            Dir::Down => Vec2::new(r.center().x, r.max().y + 9.0),
            Dir::Up => Vec2::new(r.center().x, r.min.y - 9.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKind {
    Curve,
    Straight,
    Elbow,
    RoundedElbow,
    Brace,
    Tree,
}

impl From<LineShape> for ConnectorKind {
    fn from(l: LineShape) -> Self {
        match l {
            LineShape::Curve => ConnectorKind::Curve,
            LineShape::Straight => ConnectorKind::Straight,
            LineShape::Elbow => ConnectorKind::Elbow,
            LineShape::RoundedElbow => ConnectorKind::RoundedElbow,
        }
    }
}

/// Branch line between a parent and a child.
#[derive(Debug, Clone, PartialEq)]
pub struct Connector {
    pub parent: TopicId,
    pub child: TopicId,
    pub kind: ConnectorKind,
    pub vertical: bool,
    pub start: Vec2,
    pub end: Vec2,
    pub color: crate::geometry::Color,
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryLayout {
    pub id: BoundaryId,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SummaryLayout {
    pub id: SummaryId,
    pub topic: TopicId,
    pub dir: Dir,
    /// Bounds of the summarised sibling range, including visible descendants.
    pub range_rect: Rect,
    /// Bracket end points and tip.
    pub start: Vec2,
    pub end: Vec2,
    pub tip: Vec2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationshipLayout {
    pub id: RelationshipId,
    pub start: Vec2,
    pub c1: Vec2,
    pub c2: Vec2,
    pub end: Vec2,
    pub mid: Vec2,
}

/// Complete layout of a sheet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub topics: HashMap<TopicId, TopicLayout>,
    /// Paint order (parents before children).
    pub order: Vec<TopicId>,
    pub connectors: Vec<Connector>,
    pub boundaries: Vec<BoundaryLayout>,
    pub summaries: Vec<SummaryLayout>,
    pub relationships: Vec<RelationshipLayout>,
    pub bounds: Rect,
}

impl Layout {
    /// Topmost topic under `p`.
    pub fn hit_test(&self, p: Vec2) -> Option<TopicId> {
        self.order
            .iter()
            .rev()
            .copied()
            .find(|id| self.topics[id].rect.contains(p))
    }

    /// Topic whose collapse badge is under `p`.
    pub fn hit_badge(&self, p: Vec2) -> Option<TopicId> {
        self.order.iter().rev().copied().find(|id| {
            let t = &self.topics[id];
            t.hidden_children > 0 && (t.badge_center() - p).length() <= 10.0
        })
    }

    pub fn rect(&self, id: TopicId) -> Option<Rect> {
        self.topics.get(&id).map(|t| t.rect)
    }

    /// Bounding rect of the visible subtree of `id`.
    pub fn subtree_rect(&self, sheet: &Sheet, id: TopicId) -> Option<Rect> {
        let mut r: Option<Rect> = None;
        for t in sheet.subtree_ids(id) {
            if let Some(tr) = self.rect(t) {
                r = Some(r.map_or(tr, |x| x.union(&tr)));
            }
        }
        r
    }

    /// Topics intersecting `area` (rubber band selection).
    pub fn topics_in(&self, area: &Rect) -> Vec<TopicId> {
        self.order
            .iter()
            .copied()
            .filter(|id| self.topics[id].rect.intersects(area))
            .collect()
    }
}

// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Content {
    title: Rect,
    image: Option<Rect>,
    todo: Option<Rect>,
    markers: Vec<(String, Rect)>,
    icons: Vec<(Icon, Rect)>,
    labels: Vec<(String, Rect)>,
}

#[derive(Debug, Clone, Default)]
struct Block {
    items: Vec<(TopicId, Vec2)>,
    bounds: Rect,
    connectors: Vec<Connector>,
}

impl Block {
    fn translate(mut self, d: Vec2) -> Block {
        for (_, c) in &mut self.items {
            *c = *c + d;
        }
        self.bounds = self.bounds.translate(d);
        for c in &mut self.connectors {
            c.start = c.start + d;
            c.end = c.end + d;
        }
        self
    }

    fn merge(&mut self, o: Block) {
        self.bounds = self.bounds.union(&o.bounds);
        self.items.extend(o.items);
        self.connectors.extend(o.connectors);
    }
}

pub const IMAGE_SIZE: Vec2 = Vec2::new(160.0, 100.0);

struct Engine<'a> {
    sheet: &'a Sheet,
    measure: &'a dyn TextMeasure,
    dark: bool,
    spacing: f32,
    info: HashMap<TopicId, (Vec2, Content, ResolvedStyle, Level)>,
    grow: HashMap<TopicId, Dir>,
}

/// Lays out the whole sheet.
pub fn layout_sheet(sheet: &Sheet, measure: &dyn TextMeasure, dark: bool) -> Layout {
    let mut e = Engine {
        sheet,
        measure,
        dark,
        spacing: sheet.theme.spacing.clamp(0.3, 4.0),
        info: HashMap::new(),
        grow: HashMap::new(),
    };
    let mut items: Vec<(TopicId, Vec2)> = Vec::new();
    let mut connectors = Vec::new();

    let root_block = e.node(sheet.root, sheet.structure, Dir::Right);
    items.extend(root_block.items);
    connectors.extend(root_block.connectors);

    for f in &sheet.floating {
        let st = sheet
            .topic(*f)
            .and_then(|t| t.structure)
            .unwrap_or(Structure::LogicRight);
        let pos = sheet.topic(*f).and_then(|t| t.position).unwrap_or_default();
        let b = e.node(*f, st, Dir::Right).translate(pos);
        items.extend(b.items);
        connectors.extend(b.connectors);
    }

    let mut layout = Layout::default();
    e.finish_items(&mut layout, items);

    // summaries (may be nested: summary topics with summaries, so iterate until stable)
    let mut pending: Vec<_> = sheet.summaries.iter().collect();
    let mut progress = true;
    while progress && !pending.is_empty() {
        progress = false;
        pending.retain(|s| {
            let Some(parent) = layout.topics.get(&s.parent) else {
                return true;
            };
            if parent.hidden_children > 0 {
                return false;
            }
            let kids = sheet.children(s.parent);
            if s.range.start > s.range.end || s.range.end >= kids.len() {
                return false;
            }
            let mut r: Option<Rect> = None;
            for k in &kids[s.range.start..=s.range.end] {
                if let Some(kr) = layout.subtree_rect(sheet, *k) {
                    r = Some(r.map_or(kr, |x| x.union(&kr)));
                }
            }
            let Some(r) = r else { return false };
            // Split maps can place the selected siblings on the opposite side of
            // their parent. Use their attachment side, not their own growth
            // direction (a child may override its structure). Mixed-side ranges
            // keep the parent's direction rather than inventing a new range.
            let attachment_dir = |id: &TopicId| {
                let child = layout.rect(*id)?.center();
                let center = parent.rect.center();
                Some(if child.x < center.x {
                    Dir::Left
                } else {
                    Dir::Right
                })
            };
            let structure = sheet
                .topic(s.parent)
                .and_then(|t| t.structure)
                .or((s.parent == sheet.root).then_some(sheet.structure));
            let dir = if matches!(
                structure,
                Some(
                    Structure::MapClockwise
                        | Structure::MapAnticlockwise
                        | Structure::MapUnbalanced
                )
            ) {
                attachment_dir(&kids[s.range.start])
                    .filter(|dir| {
                        kids[s.range.start..=s.range.end]
                            .iter()
                            .all(|id| attachment_dir(id) == Some(*dir))
                    })
                    .unwrap_or(parent.grow)
            } else {
                parent.grow
            };
            let (start, end, tip) = match dir {
                Dir::Right => (
                    Vec2::new(r.max().x + 8.0, r.min.y),
                    Vec2::new(r.max().x + 8.0, r.max().y),
                    Vec2::new(r.max().x + 22.0, r.center().y),
                ),
                Dir::Left => (
                    Vec2::new(r.min.x - 8.0, r.min.y),
                    Vec2::new(r.min.x - 8.0, r.max().y),
                    Vec2::new(r.min.x - 22.0, r.center().y),
                ),
                Dir::Down => (
                    Vec2::new(r.min.x, r.max().y + 8.0),
                    Vec2::new(r.max().x, r.max().y + 8.0),
                    Vec2::new(r.center().x, r.max().y + 22.0),
                ),
                Dir::Up => (
                    Vec2::new(r.min.x, r.min.y - 8.0),
                    Vec2::new(r.max().x, r.min.y - 8.0),
                    Vec2::new(r.center().x, r.min.y - 22.0),
                ),
            };
            let st = match dir {
                Dir::Left => Structure::LogicLeft,
                Dir::Down => Structure::OrgDown,
                Dir::Up => Structure::OrgUp,
                Dir::Right => Structure::LogicRight,
            };
            let st = sheet.topic(s.topic).and_then(|t| t.structure).unwrap_or(st);
            let b = e.node(s.topic, st, dir);
            let off = match dir {
                Dir::Right => Vec2::new(tip.x + 10.0 - b.bounds.min.x, tip.y),
                Dir::Left => Vec2::new(tip.x - 10.0 - b.bounds.max().x, tip.y),
                Dir::Down => Vec2::new(tip.x, tip.y + 10.0 - b.bounds.min.y),
                Dir::Up => Vec2::new(tip.x, tip.y - 10.0 - b.bounds.max().y),
            };
            let b = b.translate(off);
            layout.connectors.extend(b.connectors.clone());
            e.finish_items(&mut layout, b.items);
            layout.summaries.push(SummaryLayout {
                id: s.id,
                topic: s.topic,
                dir,
                range_rect: r,
                start,
                end,
                tip,
            });
            progress = true;
            false
        });
    }
    layout.connectors.splice(0..0, connectors);

    for b in &sheet.boundaries {
        if !layout.topics.contains_key(&b.parent) {
            continue;
        }
        let rect = match b.range {
            None => layout.subtree_rect(sheet, b.parent),
            Some(range) => {
                if layout.topics[&b.parent].hidden_children > 0 {
                    continue;
                }
                let kids = sheet.children(b.parent);
                let mut r: Option<Rect> = None;
                for k in kids
                    .iter()
                    .skip(range.start)
                    .take(range.end + 1 - range.start)
                {
                    if let Some(kr) = layout.subtree_rect(sheet, *k) {
                        r = Some(r.map_or(kr, |x| x.union(&kr)));
                    }
                }
                r
            }
        };
        if let Some(r) = rect {
            let top = if b.title.is_some() { 22.0 } else { 0.0 };
            let r = r.expand(10.0);
            let r = Rect::from_min_size(
                Vec2::new(r.min.x, r.min.y - top),
                Vec2::new(r.size.x, r.size.y + top),
            );
            layout.boundaries.push(BoundaryLayout { id: b.id, rect: r });
        }
    }

    for rel in &sheet.relationships {
        let (Some(a), Some(b)) = (layout.rect(rel.from), layout.rect(rel.to)) else {
            continue;
        };
        layout.relationships.push(relationship_curve(rel.id, a, b));
    }

    let mut bounds: Option<Rect> = None;
    let mut add = |r: Rect| bounds = Some(bounds.map_or(r, |b: Rect| b.union(&r)));
    for t in layout.topics.values() {
        add(t.rect);
    }
    for b in &layout.boundaries {
        add(b.rect);
    }
    for r in &layout.relationships {
        for p in [r.c1, r.c2] {
            add(Rect::from_center_size(p, Vec2::new(1.0, 1.0)));
        }
    }
    layout.bounds = bounds.unwrap_or_default();
    layout
}

fn relationship_curve(id: RelationshipId, a: Rect, b: Rect) -> RelationshipLayout {
    let ca = a.center();
    let cb = b.center();
    let d = cb - ca;
    let len = d.length().max(1.0);
    let perp = Vec2::new(-d.y / len, d.x / len) * (len * 0.25).min(120.0);
    let c1 = ca + d * 0.25 + perp;
    let c2 = ca + d * 0.75 + perp;
    let start = clip_to_rect(a, c1);
    let end = clip_to_rect(b, c2);
    let mid = cubic(start, c1, c2, end, 0.5);
    RelationshipLayout {
        id,
        start,
        c1,
        c2,
        end,
        mid,
    }
}

/// Point on the border of `r` in direction of `toward` from the center.
pub fn clip_to_rect(r: Rect, toward: Vec2) -> Vec2 {
    let c = r.center();
    let d = toward - c;
    if d.length() < 1e-3 {
        return c;
    }
    let hx = r.size.x / 2.0;
    let hy = r.size.y / 2.0;
    let tx = if d.x.abs() > 1e-6 {
        hx / d.x.abs()
    } else {
        f32::INFINITY
    };
    let ty = if d.y.abs() > 1e-6 {
        hy / d.y.abs()
    } else {
        f32::INFINITY
    };
    c + d * tx.min(ty)
}

/// Evaluates a cubic Bézier curve.
pub fn cubic(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

impl Engine<'_> {
    fn info(&mut self, id: TopicId) -> &(Vec2, Content, ResolvedStyle, Level) {
        if !self.info.contains_key(&id) {
            let style = resolve_style(self.sheet, id, self.dark);
            let level = level_of(self.sheet, id);
            let (size, content) = self.measure_topic(id, &style, level);
            self.info.insert(id, (size, content, style, level));
        }
        &self.info[&id]
    }

    fn size(&mut self, id: TopicId) -> Vec2 {
        self.info(id).0
    }

    fn measure_topic(&self, id: TopicId, style: &ResolvedStyle, level: Level) -> (Vec2, Content) {
        let t = self.sheet.topic(id).expect("topic exists");
        let fs = style.font_size;
        let (mut px, mut py) = match level {
            Level::Central => (24.0, 16.0),
            Level::Main => (16.0, 10.0),
            Level::Floating | Level::Summary => (14.0, 9.0),
            Level::Sub => (8.0, 5.0),
        };
        match style.shape {
            TopicShape::Ellipse => {
                px *= 1.6;
                py *= 1.6;
            }
            TopicShape::Diamond => {
                px *= 2.2;
                py *= 2.2;
            }
            TopicShape::Capsule => px += fs * 0.6,
            _ => {}
        }
        let max_w = style.width.unwrap_or(if level == Level::Central {
            420.0
        } else {
            300.0
        });
        let title = if t.title.trim().is_empty() {
            " "
        } else {
            t.title.as_str()
        };
        let mut ts = self.measure.measure(title, fs, style.bold, max_w);
        if let Some(w) = style.width {
            ts.x = w;
        }
        let ms = (fs * 1.1).round();
        let gap = 4.0;
        let mut left = 0.0;
        if t.todo.is_some() {
            left += ms + gap;
        }
        left += t.markers.len() as f32 * (ms + gap);
        let isz = (fs * 0.85).round();
        let n_icons = t.hyperlink.is_some() as usize + t.has_notes() as usize;
        let right = n_icons as f32 * (isz + gap);
        let row_h =
            ts.y.max(if left > 0.0 { ms } else { 0.0 })
                .max(if right > 0.0 { isz } else { 0.0 });
        let lfs = (fs * 0.72).max(9.0);
        let lh = lfs * 1.3 + 4.0;
        let label_ws: Vec<f32> = t
            .labels
            .iter()
            .map(|l| self.measure.measure(l, lfs, false, 400.0).x + 10.0)
            .collect();
        let labels_w =
            label_ws.iter().sum::<f32>() + (label_ws.len().saturating_sub(1)) as f32 * gap;
        let image_h = if t.image.is_some() {
            IMAGE_SIZE.y + 6.0
        } else {
            0.0
        };
        let row_w = left + ts.x + right;
        let inner_w = row_w
            .max(labels_w)
            .max(if t.image.is_some() { IMAGE_SIZE.x } else { 0.0 });
        let labels_h = if t.labels.is_empty() { 0.0 } else { lh + gap };
        let w = (inner_w + 2.0 * px).max(fs * 2.0);
        let h = 2.0 * py + image_h + row_h + labels_h;
        let inner_x = (w - inner_w) / 2.0;

        let mut c = Content::default();
        let mut y = py;
        if t.image.is_some() {
            c.image = Some(Rect::from_min_size(
                Vec2::new(inner_x + (inner_w - IMAGE_SIZE.x) / 2.0, y),
                IMAGE_SIZE,
            ));
            y += image_h;
        }
        let mut x = inner_x + (inner_w - row_w) / 2.0;
        let cy = y + row_h / 2.0;
        let sq = |x: f32, s: f32| Rect::from_min_size(Vec2::new(x, cy - s / 2.0), Vec2::new(s, s));
        if t.todo.is_some() {
            c.todo = Some(sq(x, ms));
            x += ms + gap;
        }
        for m in &t.markers {
            c.markers.push((m.clone(), sq(x, ms)));
            x += ms + gap;
        }
        c.title = Rect::from_min_size(Vec2::new(x, cy - ts.y / 2.0), ts);
        x += ts.x + gap;
        if t.hyperlink.is_some() {
            c.icons.push((Icon::Link, sq(x, isz)));
            x += isz + gap;
        }
        if t.has_notes() {
            c.icons.push((Icon::Notes, sq(x, isz)));
        }
        if !t.labels.is_empty() {
            let mut lx = inner_x + (inner_w - labels_w) / 2.0;
            let ly = y + row_h + gap;
            for (l, lw) in t.labels.iter().zip(label_ws) {
                c.labels.push((
                    l.clone(),
                    Rect::from_min_size(Vec2::new(lx, ly), Vec2::new(lw, lh)),
                ));
                lx += lw + gap;
            }
        }
        (Vec2::new(w.round(), h.round()), c)
    }

    fn finish_items(&mut self, layout: &mut Layout, items: Vec<(TopicId, Vec2)>) {
        for (id, center) in items {
            let (size, content, style, level) = self.info(id).clone();
            let rect = Rect::from_center_size(center, size);
            let off = rect.min;
            let mv = |r: Rect| r.translate(off);
            let topic = self.sheet.topic(id).unwrap();
            let hidden = if topic.collapsed {
                self.sheet.subtree_ids(id).len() - 1
            } else {
                0
            };
            layout.topics.insert(
                id,
                TopicLayout {
                    id,
                    rect,
                    level,
                    style,
                    grow: *self.grow.get(&id).unwrap_or(&Dir::Right),
                    title_rect: mv(content.title),
                    image_rect: content.image.map(mv),
                    todo_rect: content.todo.map(mv),
                    markers: content
                        .markers
                        .into_iter()
                        .map(|(m, r)| (m, mv(r)))
                        .collect(),
                    icons: content.icons.into_iter().map(|(m, r)| (m, mv(r))).collect(),
                    labels: content
                        .labels
                        .into_iter()
                        .map(|(m, r)| (m, mv(r)))
                        .collect(),
                    hidden_children: hidden,
                },
            );
            layout.order.push(id);
        }
    }

    fn single(&mut self, id: TopicId) -> Block {
        let size = self.size(id);
        Block {
            items: vec![(id, Vec2::ZERO)],
            bounds: Rect::from_center_size(Vec2::ZERO, size),
            connectors: vec![],
        }
    }

    /// Effective structure used to arrange the children of `id`.
    fn node(&mut self, id: TopicId, inherited: Structure, dir: Dir) -> Block {
        let topic = self.sheet.topic(id).expect("topic exists");
        let st = topic.structure.unwrap_or(inherited);
        let grow = match st {
            Structure::LogicLeft
            | Structure::BraceLeft
            | Structure::TreeLeft
            | Structure::FishboneRight => Dir::Left,
            Structure::OrgDown | Structure::TimelineVertical => Dir::Down,
            Structure::OrgUp => Dir::Up,
            Structure::MapClockwise | Structure::MapAnticlockwise | Structure::MapUnbalanced => dir,
            _ => Dir::Right,
        };
        self.grow.insert(id, grow);
        let mut block = self.single(id);
        let kids = topic.children.clone();
        if topic.collapsed || kids.is_empty() {
            return block;
        }
        let is_root = id == self.sheet.root;
        let sp = self.spacing;
        let gm = if is_root { 56.0 } else { 34.0 } * sp;
        let gc = if is_root { 20.0 } else { 10.0 } * sp;
        match st {
            Structure::MapClockwise | Structure::MapAnticlockwise | Structure::MapUnbalanced => {
                let (right, left) = self.split_sides(&kids, st);
                self.stack(
                    &mut block,
                    id,
                    &right,
                    Dir::Right,
                    Structure::LogicRight,
                    gm,
                    gc,
                    None,
                );
                self.stack(
                    &mut block,
                    id,
                    &left,
                    Dir::Left,
                    Structure::LogicLeft,
                    gm,
                    gc,
                    None,
                );
            }
            Structure::LogicRight | Structure::Matrix | Structure::TreeTable => self.stack(
                &mut block,
                id,
                &kids,
                Dir::Right,
                Structure::LogicRight,
                gm,
                gc,
                None,
            ),
            Structure::LogicLeft => self.stack(
                &mut block,
                id,
                &kids,
                Dir::Left,
                Structure::LogicLeft,
                gm,
                gc,
                None,
            ),
            Structure::BraceRight => self.stack(
                &mut block,
                id,
                &kids,
                Dir::Right,
                st,
                gm,
                gc,
                Some(ConnectorKind::Brace),
            ),
            Structure::BraceLeft => self.stack(
                &mut block,
                id,
                &kids,
                Dir::Left,
                st,
                gm,
                gc,
                Some(ConnectorKind::Brace),
            ),
            Structure::OrgDown => self.stack(
                &mut block,
                id,
                &kids,
                Dir::Down,
                st,
                gm * 0.8,
                gc * 1.6,
                None,
            ),
            Structure::OrgUp => {
                self.stack(&mut block, id, &kids, Dir::Up, st, gm * 0.8, gc * 1.6, None)
            }
            Structure::TreeRight => self.tree(&mut block, id, &kids, false, st),
            Structure::TreeLeft => self.tree(&mut block, id, &kids, true, st),
            Structure::TimelineHorizontal => self.timeline(&mut block, id, &kids, false, gm),
            Structure::TimelineVertical => self.timeline(&mut block, id, &kids, true, gm),
            Structure::FishboneRight => self.fishbone(&mut block, id, &kids, true, gm),
            Structure::FishboneLeft => self.fishbone(&mut block, id, &kids, false, gm),
        }
        block
    }

    fn split_sides(&mut self, kids: &[TopicId], st: Structure) -> (Vec<TopicId>, Vec<TopicId>) {
        let n = kids.len();
        if self.sheet.theme.balanced && n > 1 {
            let heights: Vec<f32> = kids
                .iter()
                .map(|k| {
                    let b = self.node(*k, Structure::LogicRight, Dir::Right);
                    b.bounds.size.y
                })
                .collect();
            let total: f32 = heights.iter().sum();
            let mut acc = 0.0;
            let mut cut = n;
            for (i, h) in heights.iter().enumerate() {
                if acc >= total / 2.0 && i > 0 {
                    cut = i;
                    break;
                }
                acc += h;
            }
            let right = kids[..cut].to_vec();
            let left = kids[cut..].iter().rev().copied().collect();
            return (right, left);
        }
        match st {
            Structure::MapAnticlockwise => {
                let cut = n.div_ceil(2);
                let left = kids[..cut].to_vec();
                let right = kids[cut..].iter().rev().copied().collect();
                (right, left)
            }
            Structure::MapUnbalanced => {
                let cut = n - n / 3;
                (
                    kids[..cut].to_vec(),
                    kids[cut..].iter().rev().copied().collect(),
                )
            }
            _ => {
                let cut = n.div_ceil(2);
                (
                    kids[..cut].to_vec(),
                    kids[cut..].iter().rev().copied().collect(),
                )
            }
        }
    }

    fn attach(&mut self, id: TopicId, center: Vec2, dir: Dir, outgoing: bool) -> Vec2 {
        let (size, _, style, _) = self.info(id);
        let r = Rect::from_center_size(center, *size);
        let underline = style.shape == TopicShape::Underline;
        let toward_right = (dir == Dir::Right) == outgoing;
        match dir {
            Dir::Right | Dir::Left => {
                let x = if toward_right { r.max().x } else { r.min.x };
                let y = if underline { r.max().y } else { r.center().y };
                Vec2::new(x, y)
            }
            Dir::Down => {
                if outgoing {
                    r.bottom_center()
                } else {
                    r.top_center()
                }
            }
            Dir::Up => {
                if outgoing {
                    r.top_center()
                } else {
                    r.bottom_center()
                }
            }
        }
    }

    fn connector(
        &mut self,
        parent: TopicId,
        child: TopicId,
        kind: Option<ConnectorKind>,
        vertical: bool,
        start: Vec2,
        end: Vec2,
    ) -> Connector {
        let pstyle = self.info(parent).2.clone();
        let cstyle = self.info(child).2.clone();
        Connector {
            parent,
            child,
            kind: kind.unwrap_or(pstyle.line_shape.into()),
            vertical,
            start,
            end,
            color: cstyle.line_color,
            width: pstyle.line_width,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stack(
        &mut self,
        block: &mut Block,
        id: TopicId,
        kids: &[TopicId],
        dir: Dir,
        child_st: Structure,
        gm: f32,
        gc: f32,
        kind: Option<ConnectorKind>,
    ) {
        if kids.is_empty() {
            return;
        }
        let half = self.size(id) * 0.5;
        let blocks: Vec<Block> = kids.iter().map(|k| self.node(*k, child_st, dir)).collect();
        let horizontal = dir.is_horizontal();
        let extent = |b: &Block| {
            if horizontal {
                b.bounds.size.y
            } else {
                b.bounds.size.x
            }
        };
        let total: f32 = blocks.iter().map(extent).sum::<f32>() + gc * (blocks.len() - 1) as f32;
        let mut cur = -total / 2.0;
        for (b, k) in blocks.into_iter().zip(kids) {
            let off = match dir {
                Dir::Right => Vec2::new(half.x + gm - b.bounds.min.x, cur - b.bounds.min.y),
                Dir::Left => Vec2::new(-half.x - gm - b.bounds.max().x, cur - b.bounds.min.y),
                Dir::Down => Vec2::new(cur - b.bounds.min.x, half.y + gm - b.bounds.min.y),
                Dir::Up => Vec2::new(cur - b.bounds.min.x, -half.y - gm - b.bounds.max().y),
            };
            cur += extent(&b) + gc;
            let start = self.attach(id, Vec2::ZERO, dir, true);
            let end = self.attach(*k, off, dir, false);
            let c = self.connector(id, *k, kind, !horizontal, start, end);
            block.merge(b.translate(off));
            block.connectors.push(c);
        }
    }

    fn tree(
        &mut self,
        block: &mut Block,
        id: TopicId,
        kids: &[TopicId],
        left: bool,
        st: Structure,
    ) {
        let half = self.size(id) * 0.5;
        let sp = self.spacing;
        let indent = 26.0 * sp;
        let gap = 10.0 * sp;
        let dir = if left { Dir::Left } else { Dir::Right };
        let mut y = half.y + gap;
        for k in kids {
            let b = self.node(*k, st, dir);
            let off = if left {
                Vec2::new(half.x - indent - b.bounds.max().x, y - b.bounds.min.y)
            } else {
                Vec2::new(-half.x + indent - b.bounds.min.x, y - b.bounds.min.y)
            };
            y += b.bounds.size.y + gap;
            let sx = if left {
                half.x - indent / 2.0
            } else {
                -half.x + indent / 2.0
            };
            let start = Vec2::new(sx, half.y);
            let end = self.attach(*k, off, dir, false);
            let c = self.connector(id, *k, Some(ConnectorKind::Tree), true, start, end);
            block.merge(b.translate(off));
            block.connectors.push(c);
        }
    }

    fn timeline(
        &mut self,
        block: &mut Block,
        id: TopicId,
        kids: &[TopicId],
        vertical: bool,
        gm: f32,
    ) {
        let half = self.size(id) * 0.5;
        let mut prev = id;
        let mut prev_center = Vec2::ZERO;
        let mut cur = if vertical { half.y + gm } else { half.x + gm };
        for k in kids {
            let (b, off) = if vertical {
                let b = self.node(*k, Structure::LogicRight, Dir::Right);
                let off = Vec2::new(0.0, cur - b.bounds.min.y);
                cur += b.bounds.size.y + gm * 0.6;
                (b, off)
            } else {
                let b = self.node(*k, Structure::TreeRight, Dir::Right);
                let off = Vec2::new(cur - b.bounds.min.x, 0.0);
                cur += b.bounds.size.x + gm * 0.6;
                (b, off)
            };
            let d = if vertical { Dir::Down } else { Dir::Right };
            let start = self.attach(prev, prev_center, d, true);
            let end = self.attach(*k, off, d, false);
            let c = self.connector(id, *k, Some(ConnectorKind::Straight), vertical, start, end);
            block.merge(b.translate(off));
            block.connectors.push(c);
            prev = *k;
            prev_center = off;
        }
    }

    fn fishbone(
        &mut self,
        block: &mut Block,
        id: TopicId,
        kids: &[TopicId],
        head_right: bool,
        gm: f32,
    ) {
        let half = self.size(id) * 0.5;
        let child_st = if head_right {
            Structure::LogicLeft
        } else {
            Structure::LogicRight
        };
        let dir = if head_right { Dir::Left } else { Dir::Right };
        let s = if head_right { -1.0 } else { 1.0 };
        let mut cursor = half.x + gm;
        let mut far = cursor;
        for pair in kids.chunks(2) {
            let mut col_w: f32 = 0.0;
            for (i, k) in pair.iter().enumerate() {
                let b = self.node(*k, child_st, dir);
                col_w = col_w.max(b.bounds.size.x);
                let up = i == 0;
                let (kx, ky) = if head_right {
                    (
                        -cursor - b.bounds.max().x,
                        if up {
                            -half.y - 30.0 - b.bounds.max().y
                        } else {
                            half.y + 30.0 - b.bounds.min.y
                        },
                    )
                } else {
                    (
                        cursor - b.bounds.min.x,
                        if up {
                            -half.y - 30.0 - b.bounds.max().y
                        } else {
                            half.y + 30.0 - b.bounds.min.y
                        },
                    )
                };
                let off = Vec2::new(kx, ky);
                let ksize = self.size(*k);
                let kr = Rect::from_center_size(off, ksize);
                let end = if up {
                    kr.bottom_center()
                } else {
                    kr.top_center()
                };
                let start = Vec2::new(end.x - s * 30.0, 0.0);
                let c = self.connector(id, *k, Some(ConnectorKind::Straight), true, start, end);
                block.merge(b.translate(off));
                block.connectors.push(c);
            }
            cursor += col_w + gm * 0.5;
            far = cursor;
        }
        let start = Vec2::new(s * half.x, 0.0);
        let end = Vec2::new(s * far, 0.0);
        let c = self.connector(id, id, Some(ConnectorKind::Straight), false, start, end);
        block.connectors.insert(0, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Sheet, Structure};

    fn sheet(n: usize) -> (Sheet, Vec<TopicId>) {
        let mut s = Sheet::new("S", "Central Topic");
        let kids: Vec<_> = (0..n)
            .map(|i| s.add_child(s.root, None, format!("Main Topic {i}")))
            .collect();
        (s, kids)
    }

    fn no_overlaps(l: &Layout) {
        let ids: Vec<_> = l.order.clone();
        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                let ra = l.topics[a].rect.expand(-0.5);
                let rb = l.topics[b].rect.expand(-0.5);
                assert!(!ra.intersects(&rb), "{a:?} {ra:?} overlaps {b:?} {rb:?}");
            }
        }
    }

    #[test]
    fn approx_measure_wraps() {
        let m = ApproxMeasure;
        let one = m.measure("hello", 14.0, false, 1000.0);
        let lines = m.measure("hello world foo bar baz qux", 14.0, false, 60.0);
        assert!(lines.y > one.y * 2.0);
        assert!(lines.x <= 60.0 + 14.0);
        let multi = m.measure("a\nb", 14.0, false, 1000.0);
        assert_eq!(multi.y, 2.0 * 14.0 * ApproxMeasure::LINE_HEIGHT);
    }

    #[test]
    fn clockwise_map_puts_first_half_right() {
        let (s, k) = sheet(4);
        let l = layout_sheet(&s, &ApproxMeasure, false);
        let x = |id| l.topics[&id].rect.center().x;
        assert!(x(k[0]) > 0.0 && x(k[1]) > 0.0);
        assert!(x(k[2]) < 0.0 && x(k[3]) < 0.0);
        // right side ordered top→bottom, left side bottom→top
        let y = |id| l.topics[&id].rect.center().y;
        assert!(y(k[0]) < y(k[1]));
        assert!(y(k[3]) < y(k[2]));
        assert_eq!(l.topics[&s.root].rect.center(), Vec2::ZERO);
        assert_eq!(l.connectors.len(), 4);
        no_overlaps(&l);
    }

    #[test]
    fn all_structures_layout_without_overlap() {
        for st in Structure::ALL {
            let (mut s, k) = sheet(5);
            s.structure = st;
            for (i, kk) in k.iter().enumerate() {
                for j in 0..(i % 3) {
                    let c = s.add_child(*kk, None, format!("Subtopic {j}"));
                    if j == 1 {
                        s.add_child(c, None, "deep");
                    }
                }
            }
            let l = layout_sheet(&s, &ApproxMeasure, false);
            assert_eq!(l.topics.len(), s.topic_count(), "{st:?}");
            no_overlaps(&l);
        }
    }

    #[test]
    fn logic_right_children_are_right() {
        let (mut s, k) = sheet(3);
        s.structure = Structure::LogicRight;
        let l = layout_sheet(&s, &ApproxMeasure, false);
        let root = l.topics[&s.root].rect;
        for id in k {
            assert!(l.topics[&id].rect.min.x > root.max().x);
        }
    }

    #[test]
    fn org_down_children_are_below() {
        let (mut s, k) = sheet(3);
        s.structure = Structure::OrgDown;
        let l = layout_sheet(&s, &ApproxMeasure, false);
        let root = l.topics[&s.root].rect;
        for id in k {
            assert!(l.topics[&id].rect.min.y > root.max().y);
        }
        assert!(l.connectors.iter().all(|c| c.vertical));
    }

    #[test]
    fn collapsed_hides_children() {
        let (mut s, k) = sheet(2);
        let c = s.add_child(k[0], None, "child");
        s.topic_mut(k[0]).unwrap().collapsed = true;
        let l = layout_sheet(&s, &ApproxMeasure, false);
        assert!(!l.topics.contains_key(&c));
        assert_eq!(l.topics[&k[0]].hidden_children, 1);
        let badge = l.topics[&k[0]].badge_center();
        assert_eq!(l.hit_badge(badge), Some(k[0]));
    }

    #[test]
    fn hit_test_finds_topic() {
        let (s, k) = sheet(2);
        let l = layout_sheet(&s, &ApproxMeasure, false);
        let c = l.topics[&k[1]].rect.center();
        assert_eq!(l.hit_test(c), Some(k[1]));
        assert_eq!(l.hit_test(Vec2::new(10_000.0, 0.0)), None);
        assert!(l.bounds.contains(c));
    }

    #[test]
    fn floating_boundaries_summaries_relationships() {
        let (mut s, k) = sheet(4);
        let f = s.add_floating("Floating", Vec2::new(0.0, 400.0));
        s.add_boundary(&[k[0], k[1]]).unwrap();
        let st = s.add_summary(&[k[0], k[1]], "Summary").unwrap();
        s.add_relationship(k[0], f).unwrap();
        let l = layout_sheet(&s, &ApproxMeasure, false);
        assert!((l.topics[&f].rect.center().y - 400.0).abs() < 0.01);
        assert_eq!(l.boundaries.len(), 1);
        let b = l.boundaries[0].rect;
        assert!(b.contains(l.topics[&k[0]].rect.center()));
        assert!(b.contains(l.topics[&k[1]].rect.center()));
        assert_eq!(l.summaries.len(), 1);
        assert!(l.topics[&st].rect.min.x > l.topics[&k[0]].rect.max().x);
        assert_eq!(l.relationships.len(), 1);
    }

    #[test]
    fn summary_bounds_include_visible_descendants_in_each_direction() {
        for (structure, dir) in [
            (Structure::LogicRight, Dir::Right),
            (Structure::LogicLeft, Dir::Left),
            (Structure::OrgDown, Dir::Down),
            (Structure::OrgUp, Dir::Up),
        ] {
            let (mut s, k) = sheet(3);
            s.structure = structure;
            let child = s.add_child(k[0], None, "A wide descendant with independent text");
            let deep = s.add_child(child, None, "Deep");
            s.add_child(k[1], None, "Other descendant");
            let summary = s.add_summary(&[k[1], k[0]], "Conclusion").unwrap();
            let summary_child = s.insert_child(summary, "Next step").unwrap();
            let l = layout_sheet(&s, &ApproxMeasure, false);
            let sl = &l.summaries[0];
            let expected = l
                .subtree_rect(&s, k[0])
                .unwrap()
                .union(&l.subtree_rect(&s, k[1]).unwrap());
            assert_eq!(sl.range_rect, expected);
            assert_eq!(sl.dir, dir);
            assert!(sl.range_rect.contains(l.rect(deep).unwrap().max()));
            assert!(!sl.range_rect.intersects(&l.rect(summary).unwrap()));
            assert!(l.rect(summary_child).is_some());
            match dir {
                Dir::Right => {
                    assert_eq!(sl.start, Vec2::new(expected.max().x + 8.0, expected.min.y));
                    assert_eq!(sl.end.y, expected.max().y);
                    assert!(l.rect(summary).unwrap().min.x > sl.tip.x);
                }
                Dir::Left => {
                    assert_eq!(sl.start, Vec2::new(expected.min.x - 8.0, expected.min.y));
                    assert_eq!(sl.end.y, expected.max().y);
                    assert!(l.rect(summary).unwrap().max().x < sl.tip.x);
                }
                Dir::Down => {
                    assert_eq!(sl.start, Vec2::new(expected.min.x, expected.max().y + 8.0));
                    assert_eq!(sl.end.x, expected.max().x);
                    assert!(l.rect(summary).unwrap().min.y > sl.tip.y);
                }
                Dir::Up => {
                    assert_eq!(sl.start, Vec2::new(expected.min.x, expected.min.y - 8.0));
                    assert_eq!(sl.end.x, expected.max().x);
                    assert!(l.rect(summary).unwrap().max().y < sl.tip.y);
                }
            }
            s.set_collapsed(k[0], true).unwrap();
            let collapsed = layout_sheet(&s, &ApproxMeasure, false);
            assert!(collapsed.rect(deep).is_none());
            assert_eq!(
                collapsed.summaries[0].range_rect,
                collapsed
                    .rect(k[0])
                    .unwrap()
                    .union(&collapsed.subtree_rect(&s, k[1]).unwrap())
            );
            s.set_collapsed(s.root, true).unwrap();
            let hidden = layout_sheet(&s, &ApproxMeasure, false);
            assert!(hidden.summaries.is_empty());
            assert!(hidden.rect(summary).is_none());
            assert!(hidden.rect(summary_child).is_none());
        }
    }

    #[test]
    fn summary_uses_selected_map_side_even_with_child_structure_override() {
        for structure in [
            Structure::MapClockwise,
            Structure::MapAnticlockwise,
            Structure::MapUnbalanced,
        ] {
            let (mut s, k) = sheet(6);
            s.structure = structure;
            let base = layout_sheet(&s, &ApproxMeasure, false);
            let left: Vec<_> = k
                .iter()
                .copied()
                .filter(|id| base.rect(*id).unwrap().center().x < 0.0)
                .collect();
            assert!(!left.is_empty());
            // A left-side topic may grow its children rightward; its summary
            // still belongs on the outside of the left-side sibling range.
            s.topic_mut(left[0]).unwrap().structure = Some(Structure::LogicRight);
            let summary = s.add_summary(&left, "Left conclusion").unwrap();
            let l = layout_sheet(&s, &ApproxMeasure, false);
            assert_eq!(l.summaries[0].dir, Dir::Left, "{structure:?}");
            assert!(l.rect(summary).unwrap().max().x < l.summaries[0].range_rect.min.x);
        }
    }

    #[test]
    fn summary_preserves_non_split_structure_direction() {
        for structure in Structure::ALL {
            if matches!(
                structure,
                Structure::MapClockwise | Structure::MapAnticlockwise | Structure::MapUnbalanced
            ) {
                continue;
            }
            let (mut s, k) = sheet(3);
            s.structure = structure;
            s.set_title(s.root, "A very wide central topic compared to the children")
                .unwrap();
            s.add_summary(&k[..2], "Conclusion").unwrap();
            let l = layout_sheet(&s, &ApproxMeasure, false);
            assert_eq!(l.summaries[0].dir, l.topics[&s.root].grow, "{structure:?}");
        }
    }

    #[test]
    fn summary_uses_nested_map_side_under_vertical_layout() {
        let (mut s, k) = sheet(1);
        s.structure = Structure::OrgDown;
        s.topic_mut(k[0]).unwrap().structure = Some(Structure::MapClockwise);
        let kids: Vec<_> = (0..4).map(|_| s.add_child(k[0], None, "Branch")).collect();
        let summary = s.add_summary(&kids[2..], "Left conclusion").unwrap();
        let l = layout_sheet(&s, &ApproxMeasure, false);
        assert_eq!(l.topics[&k[0]].grow, Dir::Down);
        assert_eq!(l.summaries[0].dir, Dir::Left);
        assert!(l.rect(summary).unwrap().max().x < l.summaries[0].range_rect.min.x);
    }

    #[test]
    fn content_regions_inside_topic() {
        let (mut s, k) = sheet(1);
        {
            let t = s.topic_mut(k[0]).unwrap();
            t.markers = vec!["priority-1".into(), "flag-red".into()];
            t.labels = vec!["label".into()];
            t.notes = Some("n".into());
            t.hyperlink = Some("https://example.com".into());
            t.todo = Some(false);
        }
        let l = layout_sheet(&s, &ApproxMeasure, false);
        let t = &l.topics[&k[0]];
        let r = t.rect.expand(0.5);
        assert_eq!(t.markers.len(), 2);
        assert_eq!(t.icons.len(), 2);
        for rr in t
            .markers
            .iter()
            .map(|m| m.1)
            .chain(t.icons.iter().map(|i| i.1))
            .chain(t.labels.iter().map(|i| i.1))
        {
            assert!(
                r.contains(rr.min) && r.contains(rr.max()),
                "{rr:?} not in {r:?}"
            );
        }
        assert!(t.todo_rect.unwrap().max().x <= t.markers[0].1.min.x);
    }

    #[test]
    fn clip_and_cubic() {
        let r = Rect::from_center_size(Vec2::ZERO, Vec2::new(20.0, 10.0));
        assert_eq!(clip_to_rect(r, Vec2::new(100.0, 0.0)), Vec2::new(10.0, 0.0));
        assert_eq!(
            clip_to_rect(r, Vec2::new(0.0, -100.0)),
            Vec2::new(0.0, -5.0)
        );
        let p = cubic(
            Vec2::ZERO,
            Vec2::ZERO,
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 1.0),
            0.5,
        );
        assert_eq!(p, Vec2::new(0.5, 0.5));
    }
}
