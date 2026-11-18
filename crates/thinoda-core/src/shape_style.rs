//! Shape-only style effects (draw.io `dashed`, `gradientColor`, `shadow`,
//! `glass`, `fillStyle`, `flipH`/`flipV`) and the renderer-neutral geometry
//! the canvas and every exporter share to paint them identically.

use crate::board::{
    CanvasObject, CanvasObjectKind, ObjectStyle, ShapeKind, rectangle_side_paths, rotate_around,
};
use crate::geometry::{Color, Rect, Vec2};
use serde::{Deserialize, Serialize};

/// Most values a dash pattern may hold.
pub const MAX_DASH_VALUES: usize = 8;
/// draw.io's shadow offset in sheet units (`mxConstants.SHADOW_OFFSET_X/Y`).
pub const SHADOW_OFFSET: Vec2 = Vec2::new(2.0, 3.0);
/// draw.io's default shadow opacity (`mxConstants.SHADOW_OPACITY` in Graph.js).
pub const SHADOW_OPACITY: f32 = 0.25;
/// draw.io's default hachure angle for pattern fills, in degrees.
pub const HATCH_ANGLE: f32 = -41.0;
const MAX_PATTERN_LINES: usize = 4000;
const MAX_DASH_SEGMENTS: usize = 100_000;

/// Dash and gap lengths, alternating, starting with a dash.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Vec<f32>", into = "Vec<f32>")]
pub struct DashPattern {
    values: [f32; MAX_DASH_VALUES],
    len: u8,
}

impl Default for DashPattern {
    fn default() -> Self {
        Self::new(&[3.0, 3.0]).expect("default dash pattern is valid")
    }
}

impl DashPattern {
    pub fn new(values: &[f32]) -> Result<Self, String> {
        if values.is_empty() || values.len() > MAX_DASH_VALUES {
            return Err(format!(
                "a dash pattern needs 1 to {MAX_DASH_VALUES} values"
            ));
        }
        if values
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 10_000.0)
        {
            return Err("dash pattern values must be finite and between 0 and 10000".to_owned());
        }
        if values.iter().all(|v| *v == 0.0) {
            return Err("a dash pattern needs a positive value".to_owned());
        }
        let mut stored = [0.0; MAX_DASH_VALUES];
        stored[..values.len()].copy_from_slice(values);
        Ok(Self {
            values: stored,
            len: values.len() as u8,
        })
    }

    pub fn values(&self) -> &[f32] {
        &self.values[..usize::from(self.len)]
    }

    /// Parses draw.io's space (or comma) separated `dashPattern`.
    pub fn parse(text: &str) -> Option<Self> {
        let values: Option<Vec<f32>> = text
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|part| !part.is_empty())
            .map(|part| part.parse().ok())
            .collect();
        Self::new(&values?).ok()
    }

    pub fn to_drawio(&self) -> String {
        self.values()
            .iter()
            .map(|v| {
                let text = format!("{v:.3}");
                text.trim_end_matches('0').trim_end_matches('.').to_owned()
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl TryFrom<Vec<f32>> for DashPattern {
    type Error = String;
    fn try_from(values: Vec<f32>) -> Result<Self, Self::Error> {
        Self::new(&values)
    }
}

impl From<DashPattern> for Vec<f32> {
    fn from(pattern: DashPattern) -> Self {
        pattern.values().to_vec()
    }
}

/// A dashed outline. Lengths scale with the stroke width unless `fixed`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ShapeDash {
    #[serde(default)]
    pub pattern: DashPattern,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
}

impl ShapeDash {
    /// Dash and gap lengths in sheet units for a stroke of `stroke_width`.
    pub fn lengths(&self, stroke_width: f32) -> Vec<f32> {
        let scale = if self.fixed { 1.0 } else { stroke_width };
        self.pattern
            .values()
            .iter()
            .map(|v| (v * scale).max(0.0))
            .collect()
    }
}

/// Where the gradient colour ends up; the fill colour is at the opposite side
/// (or the centre for `Radial`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GradientDirection {
    North,
    #[default]
    South,
    East,
    West,
    Radial,
}

impl GradientDirection {
    pub const ALL: [Self; 5] = [
        Self::South,
        Self::North,
        Self::East,
        Self::West,
        Self::Radial,
    ];

    pub fn drawio_name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::Radial => "radial",
        }
    }

    pub fn from_drawio(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.drawio_name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::North => "Top",
            Self::South => "Bottom",
            Self::East => "Right",
            Self::West => "Left",
            Self::Radial => "Radial",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShapeGradient {
    pub color: Color,
    #[serde(default)]
    pub direction: GradientDirection,
}

/// Native subset of draw.io's non-sketch `fillStyle` patterns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FillPattern {
    #[default]
    Solid,
    Hatch,
    CrossHatch,
    Dots,
    Dashed,
    ZigzagLine,
}

impl FillPattern {
    pub const ALL: [Self; 6] = [
        Self::Solid,
        Self::Hatch,
        Self::CrossHatch,
        Self::Dots,
        Self::Dashed,
        Self::ZigzagLine,
    ];

    pub fn is_solid(&self) -> bool {
        *self == Self::Solid
    }

    pub fn drawio_name(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Hatch => "hatch",
            Self::CrossHatch => "cross-hatch",
            Self::Dots => "dots",
            Self::Dashed => "dashed",
            Self::ZigzagLine => "zigzag-line",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Hatch => "Hatch",
            Self::CrossHatch => "Cross-hatch",
            Self::Dots => "Dots",
            Self::Dashed => "Dashed",
            Self::ZigzagLine => "Zigzag",
        }
    }

    /// Maps a draw.io `fillStyle`; the flag is true when the value is only
    /// approximated (sketch-only styles).
    pub fn from_drawio(name: &str) -> Option<(Self, bool)> {
        Some(match name {
            "auto" | "solid" => (Self::Solid, false),
            "hatch" => (Self::Hatch, false),
            "cross-hatch" => (Self::CrossHatch, false),
            "dots" => (Self::Dots, false),
            "dashed" => (Self::Dashed, false),
            "zigzag-line" => (Self::ZigzagLine, false),
            "zigzag" => (Self::ZigzagLine, true),
            "hachure" | "sunburst" => (Self::Hatch, true),
            _ => return None,
        })
    }
}

/// Mirrors an unrotated sheet point about the centre of `rect`.
pub fn mirror(point: Vec2, rect: Rect, flip_h: bool, flip_v: bool) -> Vec2 {
    let center = rect.center();
    Vec2::new(
        if flip_h {
            2.0 * center.x - point.x
        } else {
            point.x
        },
        if flip_v {
            2.0 * center.y - point.y
        } else {
            point.y
        },
    )
}

/// Places an unrotated, unflipped frame point: mirrored by the style's flips,
/// then rotated around the frame centre.
pub fn place(style: &ObjectStyle, point: Vec2, rect: Rect, rotation: f32) -> Vec2 {
    rotate_around(
        mirror(point, rect, style.flip_h, style.flip_v),
        rect.center(),
        rotation,
    )
}

/// Inverse of [`place`].
pub fn unplace(style: &ObjectStyle, point: Vec2, rect: Rect, rotation: f32) -> Vec2 {
    mirror(
        rotate_around(point, rect.center(), -rotation),
        rect,
        style.flip_h,
        style.flip_v,
    )
}

/// Dash lengths of the style's outline for `stroke_width`.
pub fn dash_lengths(style: &ObjectStyle, stroke_width: f32) -> Option<Vec<f32>> {
    style.dash.map(|dash| dash.lengths(stroke_width))
}

/// Splits a polyline into its visible dashes.
pub fn dash_polyline(points: &[Vec2], closed: bool, lengths: &[f32]) -> Vec<Vec<Vec2>> {
    let mut points = points.to_vec();
    if closed && points.len() > 1 && points.first() != points.last() {
        points.push(points[0]);
    }
    let period: f32 = lengths.iter().sum();
    let total: f32 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    if points.len() < 2
        || lengths.is_empty()
        || !period.is_finite()
        || period <= 0.0
        || total / period * lengths.len() as f32 > MAX_DASH_SEGMENTS as f32
    {
        return vec![points];
    }
    let mut out = Vec::new();
    let mut index = 0;
    let mut remaining = lengths[0];
    let mut on = true;
    let mut current = vec![points[0]];
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = (b - a).length();
        let mut travelled = 0.0;
        while length - travelled > remaining {
            travelled += remaining;
            let point = a + (b - a) * (travelled / length);
            if on {
                current.push(point);
                out.push(std::mem::take(&mut current));
            } else {
                current = vec![point];
            }
            on = !on;
            index = (index + 1) % lengths.len();
            remaining = lengths[index];
        }
        remaining -= length - travelled;
        if on {
            current.push(b);
        }
    }
    if on && current.len() > 1 {
        out.push(current);
    }
    out.retain(|dash| dash.windows(2).any(|w| w[0] != w[1]));
    out
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color::rgba(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b), mix(a.a, b.a))
}

/// A gradient in sheet coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapePaint {
    /// `start` at `from`, `end` at `to`, clamped outside.
    Linear {
        from: Vec2,
        to: Vec2,
        start: Color,
        end: Color,
    },
    /// `start` at the centre, `end` on the ellipse with `radii` turned by
    /// `rotation` degrees and beyond.
    Radial {
        center: Vec2,
        radii: Vec2,
        rotation: f32,
        start: Color,
        end: Color,
    },
}

impl ShapePaint {
    pub fn color_at(&self, point: Vec2) -> Color {
        match *self {
            Self::Linear {
                from,
                to,
                start,
                end,
            } => {
                let axis = to - from;
                let length = axis.x * axis.x + axis.y * axis.y;
                let t = if length > 0.0 {
                    let d = point - from;
                    (d.x * axis.x + d.y * axis.y) / length
                } else {
                    0.0
                };
                lerp_color(start, end, t)
            }
            Self::Radial {
                center,
                radii,
                rotation,
                start,
                end,
            } => {
                let local = rotate_around(point, center, -rotation) - center;
                let x = if radii.x > 0.0 {
                    local.x / radii.x
                } else {
                    0.0
                };
                let y = if radii.y > 0.0 {
                    local.y / radii.y
                } else {
                    0.0
                };
                lerp_color(start, end, (x * x + y * y).sqrt())
            }
        }
    }

    pub fn start(&self) -> Color {
        match self {
            Self::Linear { start, .. } | Self::Radial { start, .. } => *start,
        }
    }

    pub fn end(&self) -> Color {
        match self {
            Self::Linear { end, .. } | Self::Radial { end, .. } => *end,
        }
    }
}

fn faded(color: Color, opacity: f32) -> Color {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        1.0
    };
    Color {
        a: (f32::from(color.a) * opacity).round() as u8,
        ..color
    }
}

fn linear(style: &ObjectStyle, rect: Rect, rotation: f32, from: Vec2, to: Vec2) -> (Vec2, Vec2) {
    (
        place(style, from, rect, rotation),
        place(style, to, rect, rotation),
    )
}

/// The gradient that replaces a solid `fill` (already faded by opacity).
pub fn gradient_paint(
    style: &ObjectStyle,
    rect: Rect,
    rotation: f32,
    fill: Color,
) -> Option<ShapePaint> {
    let gradient = style.gradient?;
    let end = faded(gradient.color, style.opacity);
    let (min, max, center) = (rect.min, rect.max(), rect.center());
    let (from, to) = match gradient.direction {
        GradientDirection::South => (Vec2::new(center.x, min.y), Vec2::new(center.x, max.y)),
        GradientDirection::North => (Vec2::new(center.x, max.y), Vec2::new(center.x, min.y)),
        GradientDirection::East => (Vec2::new(min.x, center.y), Vec2::new(max.x, center.y)),
        GradientDirection::West => (Vec2::new(max.x, center.y), Vec2::new(min.x, center.y)),
        GradientDirection::Radial => {
            return Some(ShapePaint::Radial {
                center,
                radii: rect.size * 0.5,
                rotation,
                start: fill,
                end,
            });
        }
    };
    let (from, to) = linear(style, rect, rotation, from, to);
    Some(ShapePaint::Linear {
        from,
        to,
        start: fill,
        end,
    })
}

/// How a shape's styled fill is painted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FillPaint {
    Solid(Color),
    Gradient(ShapePaint),
    /// Pattern lines or dots in this colour, without a background.
    Pattern(Color, FillPattern),
}

/// The paint for a styled `fill` (already faded). Patterns win over gradients.
pub fn fill_paint(style: &ObjectStyle, rect: Rect, rotation: f32, fill: Color) -> FillPaint {
    if !style.fill_pattern.is_solid() {
        return FillPaint::Pattern(fill, style.fill_pattern);
    }
    gradient_paint(style, rect, rotation, fill).map_or(FillPaint::Solid(fill), FillPaint::Gradient)
}

/// Lines and dots of a pattern fill, in sheet coordinates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PatternMarks {
    pub lines: Vec<Vec<Vec2>>,
    pub dots: Vec<Vec2>,
    pub dot_radius: f32,
    pub width: f32,
}

fn inside_even_odd(polygons: &[Vec<Vec2>], point: Vec2) -> bool {
    let mut inside = false;
    for polygon in polygons {
        let n = polygon.len();
        for i in 0..n {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            if (a.y > point.y) != (b.y > point.y)
                && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
            {
                inside = !inside;
            }
        }
    }
    inside
}

fn scanlines(polygons: &[Vec<Vec2>], angle: f32, gap: f32) -> Vec<(Vec2, Vec2)> {
    let (sin, cos) = angle.to_radians().sin_cos();
    let along = Vec2::new(cos, sin);
    let normal = Vec2::new(-sin, cos);
    let dot = |a: Vec2, b: Vec2| a.x * b.x + a.y * b.y;
    let projections = polygons.iter().flatten().map(|p| dot(*p, normal));
    let (low, high) = projections.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    if !low.is_finite() || !high.is_finite() || high <= low {
        return Vec::new();
    }
    let gap = gap.max((high - low) / MAX_PATTERN_LINES as f32);
    let mut out = Vec::new();
    let mut offset = low + gap / 2.0;
    while offset < high {
        let mut hits = Vec::new();
        for polygon in polygons {
            let n = polygon.len();
            for i in 0..n {
                let (a, b) = (polygon[i], polygon[(i + 1) % n]);
                let (sa, sb) = (dot(a, normal) - offset, dot(b, normal) - offset);
                if (sa > 0.0) != (sb > 0.0) {
                    let point = a + (b - a) * (sa / (sa - sb));
                    hits.push(dot(point, along));
                }
            }
        }
        hits.sort_by(f32::total_cmp);
        let base = normal * offset;
        for &[start, end] in hits.as_chunks::<2>().0 {
            if end > start {
                out.push((base + along * start, base + along * end));
            }
        }
        offset += gap;
    }
    out
}

/// Pattern marks clipped to `polygons` (closed outlines, even-odd). Lines run
/// at draw.io's hachure angle in the shape's own (rotated) frame.
pub fn pattern_marks(
    polygons: &[Vec<Vec2>],
    rect: Rect,
    rotation: f32,
    pattern: FillPattern,
    stroke_width: f32,
) -> PatternMarks {
    let gap = (4.0 * stroke_width).max(4.0);
    let width = (stroke_width / 2.0).max(0.5);
    let center = rect.center();
    let local: Vec<Vec<Vec2>> = polygons
        .iter()
        .filter(|p| p.len() > 2)
        .map(|p| {
            p.iter()
                .map(|q| rotate_around(*q, center, -rotation))
                .collect()
        })
        .collect();
    let back = |p: Vec2| rotate_around(p, center, rotation);
    let mut marks = PatternMarks {
        width,
        dot_radius: width,
        ..PatternMarks::default()
    };
    let segments = |angle| scanlines(&local, angle, gap);
    match pattern {
        FillPattern::Solid => {}
        FillPattern::Hatch | FillPattern::CrossHatch | FillPattern::Dashed => {
            let mut lines = segments(HATCH_ANGLE);
            if pattern == FillPattern::CrossHatch {
                lines.extend(segments(HATCH_ANGLE + 90.0));
            }
            for (a, b) in lines {
                if pattern == FillPattern::Dashed {
                    marks.lines.extend(
                        dash_polyline(&[back(a), back(b)], false, &[gap, gap])
                            .into_iter()
                            .take(MAX_DASH_SEGMENTS),
                    );
                } else {
                    marks.lines.push(vec![back(a), back(b)]);
                }
            }
        }
        FillPattern::ZigzagLine => {
            for (a, b) in segments(HATCH_ANGLE) {
                let length = (b - a).length();
                if length <= 0.0 {
                    continue;
                }
                let along = (b - a) * (1.0 / length);
                let normal = Vec2::new(-along.y, along.x) * (gap / 2.0);
                let steps = ((length / gap).ceil() as usize).clamp(1, 10_000);
                let step = length / steps as f32;
                let mut points = vec![back(a)];
                for i in 0..steps {
                    let mid = a + along * (step * (i as f32 + 0.5));
                    let side = if i % 2 == 0 { normal } else { normal * -1.0 };
                    points.push(back(mid + side));
                }
                points.push(back(b));
                marks.lines.push(points);
            }
        }
        FillPattern::Dots => {
            let bounds = local.iter().flatten().fold(
                (
                    Vec2::new(f32::INFINITY, f32::INFINITY),
                    Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
                ),
                |(lo, hi), p| {
                    (
                        Vec2::new(lo.x.min(p.x), lo.y.min(p.y)),
                        Vec2::new(hi.x.max(p.x), hi.y.max(p.y)),
                    )
                },
            );
            let (lo, hi) = bounds;
            if lo.x.is_finite() && hi.x.is_finite() {
                let cells = ((hi.x - lo.x) / gap).max(0.0) * ((hi.y - lo.y) / gap).max(0.0);
                let gap = if cells > (MAX_PATTERN_LINES * 50) as f32 {
                    gap * (cells / (MAX_PATTERN_LINES * 50) as f32).sqrt()
                } else {
                    gap
                };
                let mut y = lo.y + gap / 2.0;
                while y < hi.y {
                    let mut x = lo.x + gap / 2.0;
                    while x < hi.x {
                        let point = Vec2::new(x, y);
                        if inside_even_odd(&local, point) {
                            marks.dots.push(back(point));
                        }
                        x += gap;
                    }
                    y += gap;
                }
            }
        }
    }
    marks
}

fn is_final_state(object: &CanvasObject) -> bool {
    matches!(
        object.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::FinalState,
            ..
        }
    )
}

/// Closed polygons a built-in shape's styled fill covers.
pub fn builtin_fill_polygons(
    object: &CanvasObject,
    outline: &[Vec2],
    details: &[Vec<Vec2>],
) -> Vec<Vec<Vec2>> {
    if is_final_state(object) {
        details.to_vec()
    } else {
        vec![outline.to_vec()]
    }
}

/// Stroked paths (points, closed) of a built-in shape: its visible rectangle
/// sides or outline, then its details.
pub fn builtin_stroke_paths(
    object: &CanvasObject,
    outline: &[Vec2],
    details: &[Vec<Vec2>],
) -> Vec<(Vec<Vec2>, bool)> {
    let mut paths: Vec<_> = match object.rectangle_sides {
        Some(sides) => rectangle_side_paths(
            object.rect,
            object.rotation,
            sides.flipped(object.style.flip_h, object.style.flip_v),
        )
        .into_iter()
        .map(|path| (path.points, false))
        .collect(),
        None => vec![(outline.to_vec(), true)],
    };
    let closed_details = is_final_state(object);
    paths.extend(details.iter().map(|d| (d.clone(), closed_details)));
    paths
}

/// The colour a shape's shadow copy is painted in.
pub fn shadow_color(opacity: f32) -> Color {
    faded(Color::BLACK, SHADOW_OPACITY * opacity)
}

fn signed_area(polygon: &[Vec2]) -> f32 {
    let n = polygon.len();
    (0..n)
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f32>()
        / 2.0
}

/// Sutherland–Hodgman clip of any `subject` against a convex `clip` polygon.
pub fn clip_convex(subject: &[Vec2], clip: &[Vec2]) -> Vec<Vec2> {
    let sign = signed_area(clip).signum();
    if sign == 0.0 {
        return Vec::new();
    }
    let mut output = subject.to_vec();
    if output.len() > 1 && output.first() == output.last() {
        output.pop();
    }
    let n = clip.len();
    for i in 0..n {
        if output.is_empty() {
            break;
        }
        let (a, b) = (clip[i], clip[(i + 1) % n]);
        let side = |p: Vec2| sign * ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x));
        let input = std::mem::take(&mut output);
        for j in 0..input.len() {
            let current = input[j];
            let previous = input[(j + input.len() - 1) % input.len()];
            let (sc, sp) = (side(current), side(previous));
            if sc >= 0.0 {
                if sp < 0.0 {
                    output.push(previous + (current - previous) * (sp / (sp - sc)));
                }
                output.push(current);
            } else if sp >= 0.0 {
                output.push(previous + (current - previous) * (sp / (sp - sc)));
            }
        }
    }
    output
}

/// The glass highlight over `outline` and its white gradient, or `None` when
/// the style has no glass or no fill (draw.io only glazes filled shapes).
pub fn glass(
    style: &ObjectStyle,
    rect: Rect,
    rotation: f32,
    outline: &[Vec2],
    stroke_width: Option<f32>,
) -> Option<(Vec<Vec2>, ShapePaint)> {
    if !style.glass || style.fill.is_none() || outline.len() < 3 {
        return None;
    }
    let sw = stroke_width.map_or(0.0, |w| (w / 2.0).ceil());
    let (x, y, w, h) = (rect.min.x, rect.min.y, rect.size.x, rect.size.y);
    let mut region = vec![Vec2::new(x - sw, y - sw), Vec2::new(x - sw, y + h * 0.4)];
    let (start, control, end) = (
        Vec2::new(x - sw, y + h * 0.4),
        Vec2::new(x + w * 0.5, y + h * 0.7),
        Vec2::new(x + w + sw, y + h * 0.4),
    );
    for i in 1..=16 {
        let t = i as f32 / 16.0;
        let u = 1.0 - t;
        region.push(start * (u * u) + control * (2.0 * u * t) + end * (t * t));
    }
    region.push(Vec2::new(x + w + sw, y - sw));
    let region: Vec<Vec2> = region
        .into_iter()
        .map(|p| place(style, p, rect, rotation))
        .collect();
    let clipped = clip_convex(outline, &region);
    if clipped.len() < 3 {
        return None;
    }
    let white = |alpha: f32| faded(Color::WHITE, alpha * style.opacity);
    let (from, to) = linear(
        style,
        rect,
        rotation,
        Vec2::new(x, y),
        Vec2::new(x, y + h * 0.6),
    );
    Some((
        clipped,
        ShapePaint::Linear {
            from,
            to,
            start: white(0.9),
            end: white(0.1),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_pattern_validates_and_round_trips_drawio_text() {
        assert_eq!(
            DashPattern::parse("1 4 8 2").unwrap().values(),
            [1.0, 4.0, 8.0, 2.0]
        );
        assert_eq!(DashPattern::parse("2.5,1").unwrap().to_drawio(), "2.5 1");
        assert!(DashPattern::parse("0 0").is_none());
        assert!(DashPattern::parse("-1 2").is_none());
        assert!(DashPattern::parse("1 2 3 4 5 6 7 8 9").is_none());
        let json = serde_json::to_string(&DashPattern::default()).unwrap();
        assert_eq!(json, "[3.0,3.0]");
        assert!(serde_json::from_str::<DashPattern>("[]").is_err());
    }

    #[test]
    fn effects_are_only_valid_on_shapes() {
        use crate::board::{CanvasConnector, Endpoint};
        let mut sheet = crate::model::Sheet::new("Board", "Root");
        let mut connector = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(50.0, 0.0)),
        ));
        connector.style.shadow = true;
        let error = sheet.add_canvas_object(connector).unwrap_err();
        assert!(error.to_string().contains("shape style effects"), "{error}");
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 20.0)),
        );
        shape.style.shadow = true;
        shape.style.flip_v = true;
        sheet.add_canvas_object(shape).unwrap();
        assert!(sheet.has_shape_styles());
    }

    #[test]
    fn dashes_split_a_closed_square_with_carry_over() {
        let square = [
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        let dashes = dash_polyline(&square, true, &[6.0, 4.0]);
        assert_eq!(dashes.len(), 4);
        assert_eq!(dashes[1], vec![Vec2::new(10.0, 0.0), Vec2::new(10.0, 6.0)]);
        let total: f32 = dashes
            .iter()
            .flat_map(|d| d.windows(2).map(|w| (w[1] - w[0]).length()))
            .sum();
        assert!((total - 24.0).abs() < 1e-4);
    }

    #[test]
    fn flips_mirror_before_rotation_and_gradients_follow() {
        let rect = Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 50.0));
        let style = ObjectStyle {
            flip_h: true,
            gradient: Some(ShapeGradient {
                color: Color::BLACK,
                direction: GradientDirection::East,
            }),
            ..ObjectStyle::default()
        };
        assert_eq!(
            place(&style, Vec2::new(10.0, 5.0), rect, 0.0),
            Vec2::new(90.0, 5.0)
        );
        let back = unplace(
            &style,
            place(&style, Vec2::new(10.0, 5.0), rect, 30.0),
            rect,
            30.0,
        );
        assert!((back - Vec2::new(10.0, 5.0)).length() < 1e-3);
        let paint = gradient_paint(&style, rect, 0.0, Color::WHITE).unwrap();
        assert_eq!(paint.color_at(Vec2::new(100.0, 25.0)), Color::WHITE);
        assert_eq!(paint.color_at(Vec2::new(0.0, 25.0)), Color::BLACK);
    }

    #[test]
    fn pattern_marks_stay_inside_the_outline() {
        let rect = Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 40.0));
        let square = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(40.0, 0.0),
            Vec2::new(40.0, 40.0),
            Vec2::new(0.0, 40.0),
        ];
        let inside = |p: &Vec2| (-0.01..=40.01).contains(&p.x) && (-0.01..=40.01).contains(&p.y);
        let hatch = pattern_marks(
            std::slice::from_ref(&square),
            rect,
            0.0,
            FillPattern::Hatch,
            1.0,
        );
        let cross = pattern_marks(
            std::slice::from_ref(&square),
            rect,
            0.0,
            FillPattern::CrossHatch,
            1.0,
        );
        assert!(!hatch.lines.is_empty() && cross.lines.len() > hatch.lines.len());
        assert!(hatch.lines.iter().flatten().all(inside));
        let dots = pattern_marks(&[square], rect, 0.0, FillPattern::Dots, 1.0);
        assert_eq!(dots.dots.len(), 100);
    }

    #[test]
    fn glass_only_glazes_the_top_of_filled_shapes() {
        let rect = Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0));
        let outline = crate::board::rotated_corners(rect, 0.0).to_vec();
        let mut style = ObjectStyle {
            glass: true,
            ..ObjectStyle::default()
        };
        assert!(glass(&style, rect, 0.0, &outline, None).is_none());
        style.fill = Some(Color::rgb(0, 0, 255));
        let (region, _) = glass(&style, rect, 0.0, &outline, None).unwrap();
        assert!(region.iter().all(|p| p.y <= 55.01));
        style.flip_v = true;
        let (region, _) = glass(&style, rect, 0.0, &outline, None).unwrap();
        assert!(region.iter().all(|p| p.y >= 44.99));
    }
}
