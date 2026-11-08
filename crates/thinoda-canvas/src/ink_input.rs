//! Transient freehand input and scene-based eraser/lasso geometry.
//!
//! Positions and tolerances are in sheet coordinates. A controller owns an
//! [`InkInput`] outside the document, pushes every pointer sample, and calls
//! [`InkInput::finish`] on release. Finish does not consume the input: retain it
//! until `Document::add_canvas_object` succeeds, or explicitly cancel it. This
//! also permits retrying a failed finish with different options.

use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::fmt;

use thinoda_core::board::{
    MAX_BOARD_COORDINATE, MAX_CANVAS_OBJECTS, MAX_STROKE_POINTS, MAX_STROKE_WIDTH,
    MAX_TOTAL_INK_POINTS, MIN_STROKE_WIDTH, distance_to_segment, point_in_polygon, rotate_around,
    rotated_corners, validate_object,
};
use thinoda_core::scene::{SceneGeometry, connector_stroke_path};
use thinoda_core::{
    CanvasObject, CanvasObjectId, CanvasObjectKind, Color, InkStroke, InkTool, Rect, Scene,
    SceneHit, SceneId, SceneItem, Vec2,
};

/// Bound the transient buffer independently of the smaller persisted-stroke limit.
pub const MAX_INPUT_SAMPLES: usize = MAX_STROKE_POINTS * 4;
/// Bound worst-case Ramer–Douglas–Peucker work; zero tolerance bypasses it.
pub const MAX_SIMPLIFICATION_CHECKS: usize = 4_000_000;
/// Bound the total geometry work of one eraser or lasso query.
pub const MAX_GEOMETRY_CHECKS: usize = 4_000_000;
const PRESSURE_TOLERANCE: f64 = 1.0 / 255.0;
const MAX_SAFE_SCENE_COORDINATE: f32 = 1.0e15;
const MAX_CLIPPED_LASSO_SAMPLES: usize = MAX_INPUT_SAMPLES * 2;

/// One pointer sample. Pressure is optional and, when present, must be in `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkSample {
    pub position: Vec2,
    pub pressure: Option<f32>,
}

impl InkSample {
    pub const fn new(position: Vec2, pressure: Option<f32>) -> Self {
        Self { position, pressure }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRejection {
    InvalidPosition,
    InvalidPressure,
    BoundsExceeded,
    TooManySamples { limit: usize },
}

/// The rejected sample is returned intact; previously accepted samples are untouched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RejectedSample {
    pub sample: InkSample,
    pub reason: SampleRejection,
}

impl fmt::Display for RejectedSample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason {
            SampleRejection::InvalidPosition => write!(f, "ink sample position is out of range"),
            SampleRejection::InvalidPressure => write!(f, "ink sample pressure is out of range"),
            SampleRejection::BoundsExceeded => write!(f, "ink sample would exceed the frame limit"),
            SampleRejection::TooManySamples { limit } => {
                write!(f, "ink input exceeds {limit} samples")
            }
        }
    }
}

impl std::error::Error for RejectedSample {}

#[derive(Debug, Clone, PartialEq)]
pub enum InkInputError {
    InvalidWidth(f32),
    InvalidTolerance(f32),
    EmptyStroke,
    TooManyPoints { count: usize, limit: usize },
    SimplificationTooComplex { limit: usize },
    InvalidStroke(String),
}

impl fmt::Display for InkInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWidth(width) => write!(f, "invalid ink width {width}"),
            Self::InvalidTolerance(tolerance) => {
                write!(f, "invalid ink simplification tolerance {tolerance}")
            }
            Self::EmptyStroke => write!(f, "cannot finish an empty ink stroke"),
            Self::TooManyPoints { count, limit } => {
                write!(f, "ink stroke has {count} points; the limit is {limit}")
            }
            Self::SimplificationTooComplex { limit } => {
                write!(f, "ink simplification exceeds {limit} checks")
            }
            Self::InvalidStroke(error) => write!(f, "invalid ink stroke: {error}"),
        }
    }
}

impl std::error::Error for InkInputError {}

/// Options applied only when producing a stroke; the original samples remain intact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeFinishOptions {
    /// Maximum geometric error in sheet units. Zero retains every sampled point.
    pub tolerance: f32,
    /// Apply one weighted moving-average pass, preserving the endpoints.
    pub smooth: bool,
}

impl Default for StrokeFinishOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.5,
            smooth: false,
        }
    }
}

/// A bounded, non-document buffer for one mouse, touch, or pen gesture.
#[must_use]
#[derive(Debug, Clone)]
pub struct InkInput {
    tool: InkTool,
    color: Color,
    width: f32,
    samples: Vec<InkSample>,
    bounds: Option<Rect>,
}

impl InkInput {
    pub fn new(tool: InkTool, color: Color, width: f32) -> Result<Self, InkInputError> {
        if !valid_width(width) {
            return Err(InkInputError::InvalidWidth(width));
        }
        Ok(Self {
            tool,
            color,
            width,
            samples: Vec::new(),
            bounds: None,
        })
    }

    pub fn tool(&self) -> InkTool {
        self.tool
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    /// The original samples, including duplicates, for preview or recovery.
    pub fn samples(&self) -> &[InkSample] {
        &self.samples
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Accept a sample or return it unchanged with an explicit reason.
    pub fn push(&mut self, sample: InkSample) -> Result<(), RejectedSample> {
        let reject = |reason| Err(RejectedSample { sample, reason });
        if !valid_board_position(sample.position) {
            return reject(SampleRejection::InvalidPosition);
        }
        if sample
            .pressure
            .is_some_and(|pressure| !pressure.is_finite() || !(0.0..=1.0).contains(&pressure))
        {
            return reject(SampleRejection::InvalidPressure);
        }
        if self.samples.len() >= MAX_INPUT_SAMPLES {
            return reject(SampleRejection::TooManySamples {
                limit: MAX_INPUT_SAMPLES,
            });
        }
        let point_rect = Rect::from_min_size(sample.position, Vec2::ZERO);
        let bounds = self
            .bounds
            .map_or(point_rect, |bounds| bounds.union(&point_rect));
        if bounds.size.x > MAX_BOARD_COORDINATE || bounds.size.y > MAX_BOARD_COORDINATE {
            return reject(SampleRejection::BoundsExceeded);
        }
        self.samples.push(sample);
        self.bounds = Some(bounds);
        Ok(())
    }

    /// Build one core-validated stroke without changing this input.
    ///
    /// Missing pressure values are linearly interpolated between measured samples,
    /// using the nearest measurement at either end. If no sample has pressure,
    /// the stroke has no pressure data. Simplification retains pressure deviations
    /// above `1/255` as well as geometry, and indexes both arrays with the same
    /// original sample indices.
    /// On any error the entire original buffer remains available to retry or split.
    pub fn finish(&self, options: StrokeFinishOptions) -> Result<InkStroke, InkInputError> {
        if !options.tolerance.is_finite()
            || !(0.0..=MAX_BOARD_COORDINATE).contains(&options.tolerance)
        {
            return Err(InkInputError::InvalidTolerance(options.tolerance));
        }
        if self.samples.is_empty() {
            return Err(InkInputError::EmptyStroke);
        }
        let mut positions: Vec<Vec2> = self.samples.iter().map(|sample| sample.position).collect();
        if options.smooth {
            smooth_positions(&mut positions);
        }
        let pressures = interpolated_pressures(&self.samples);
        let indices = simplify_indices(
            &positions,
            pressures.as_deref(),
            options.tolerance,
            MAX_SIMPLIFICATION_CHECKS,
        )?;
        if indices.len() > MAX_STROKE_POINTS {
            return Err(InkInputError::TooManyPoints {
                count: indices.len(),
                limit: MAX_STROKE_POINTS,
            });
        }
        let points = indices.iter().map(|&index| positions[index]).collect();
        let pressure = pressures.map(|values| {
            indices
                .iter()
                .map(|&index| values[index])
                .collect::<Vec<_>>()
                .into()
        });
        let stroke = InkStroke {
            tool: self.tool,
            color: self.color,
            width: self.width,
            points,
            pressure,
        };
        validate_object(&CanvasObject::ink(stroke.clone()))
            .map_err(InkInputError::InvalidStroke)?;
        Ok(stroke)
    }

    /// Discard this gesture without making a document edit.
    pub fn cancel(self) {
        drop(self);
    }
}

fn valid_board_position(point: Vec2) -> bool {
    point.x.is_finite()
        && point.y.is_finite()
        && point.x.abs() <= MAX_BOARD_COORDINATE
        && point.y.abs() <= MAX_BOARD_COORDINATE
}

fn valid_width(width: f32) -> bool {
    width.is_finite() && (MIN_STROKE_WIDTH..=MAX_STROKE_WIDTH).contains(&width)
}

fn smooth_positions(points: &mut [Vec2]) {
    if points.len() < 3 {
        return;
    }
    let original = points.to_vec();
    for index in 1..points.len() - 1 {
        let [before, current, after] = [original[index - 1], original[index], original[index + 1]];
        let average = |a: f32, b: f32, c: f32| {
            ((f64::from(a) + 2.0 * f64::from(b) + f64::from(c)) / 4.0) as f32
        };
        points[index] = Vec2::new(
            average(before.x, current.x, after.x),
            average(before.y, current.y, after.y),
        );
    }
}

fn interpolated_pressures(samples: &[InkSample]) -> Option<Vec<f32>> {
    let (first_index, first_value) = samples
        .iter()
        .enumerate()
        .find_map(|(index, sample)| sample.pressure.map(|value| (index, value)))?;
    let mut values = vec![first_value; samples.len()];
    let mut previous_index = first_index;
    let mut previous_value = first_value;
    for (index, sample) in samples.iter().enumerate().skip(first_index + 1) {
        let Some(value) = sample.pressure else {
            continue;
        };
        let span = (index - previous_index) as f64;
        for (offset, out) in values[previous_index + 1..index].iter_mut().enumerate() {
            let fraction = (offset + 1) as f64 / span;
            *out =
                (f64::from(previous_value) * (1.0 - fraction) + f64::from(value) * fraction) as f32;
        }
        values[index] = value;
        previous_index = index;
        previous_value = value;
    }
    values[previous_index + 1..].fill(previous_value);
    Some(values)
}

fn simplify_indices(
    points: &[Vec2],
    pressure: Option<&[f32]>,
    tolerance: f32,
    max_checks: usize,
) -> Result<Vec<usize>, InkInputError> {
    if points.len() <= 2 || tolerance == 0.0 {
        return Ok((0..points.len()).collect());
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut ranges = vec![(0, points.len() - 1)];
    let mut checks = 0usize;
    while let Some((start, end)) = ranges.pop() {
        let interior = end - start - 1;
        if interior == 0 {
            continue;
        }
        checks = checks.saturating_add(interior);
        if checks > max_checks {
            return Err(InkInputError::SimplificationTooComplex { limit: max_checks });
        }
        let mut best = None;
        let mut best_score = 1.0f64;
        for index in start + 1..end {
            let distance = distance_to_segment(points[index], points[start], points[end]);
            let geometry_score = f64::from(distance) / f64::from(tolerance);
            let pressure_score = pressure.map_or(0.0, |values| {
                let fraction = (index - start) as f64 / (end - start) as f64;
                let expected =
                    f64::from(values[start]) * (1.0 - fraction) + f64::from(values[end]) * fraction;
                (f64::from(values[index]) - expected).abs() / PRESSURE_TOLERANCE
            });
            let score = geometry_score.max(pressure_score);
            if score > best_score {
                best = Some(index);
                best_score = score;
            }
        }
        if let Some(index) = best {
            keep[index] = true;
            ranges.push((index, end));
            ranges.push((start, index));
        }
    }
    Ok(keep
        .into_iter()
        .enumerate()
        .filter_map(|(index, keep)| keep.then_some(index))
        .collect())
}

#[derive(Debug, Clone, PartialEq)]
pub enum GeometryError {
    EmptyPath,
    DegenerateLasso,
    TooManyPoints { count: usize, limit: usize },
    ClippedLassoTooComplex { limit: usize },
    InvalidPoint { index: usize, point: Vec2 },
    InvalidRadius(f32),
    InvalidSceneGeometry(SceneId),
    TooComplex { limit: usize },
}

impl fmt::Display for GeometryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath => write!(f, "geometry path is empty"),
            Self::DegenerateLasso => write!(f, "lasso needs three non-collinear points"),
            Self::TooManyPoints { count, limit } => {
                write!(f, "geometry path has {count} points; the limit is {limit}")
            }
            Self::ClippedLassoTooComplex { limit } => {
                write!(f, "clipped lasso exceeds {limit} vertices")
            }
            Self::InvalidPoint { index, .. } => write!(f, "geometry point {index} is out of range"),
            Self::InvalidRadius(radius) => write!(f, "invalid eraser radius {radius}"),
            Self::InvalidSceneGeometry(id) => write!(f, "invalid scene geometry for {id:?}"),
            Self::TooComplex { limit } => write!(f, "geometry query exceeds {limit} checks"),
        }
    }
}

impl std::error::Error for GeometryError {}

/// Transform and eraser settings for splitting one stroke.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelEraseOptions {
    /// The original object's frame center in sheet coordinates.
    pub center: Vec2,
    /// The original object's clockwise rotation, normalized to `[0, 360)`.
    pub rotation: f32,
    /// Radius of the eraser in sheet units.
    pub radius: f32,
    /// Optional scene frame; eraser and stroke centerlines are restricted to it.
    pub clip: Option<Rect>,
}

/// A split produces unrotated, sheet-coordinate strokes; `Unchanged` retains the
/// original object and its transform without any replacement.
#[derive(Debug, Clone, PartialEq)]
pub enum PixelEraseResult {
    Unchanged,
    Erased,
    Fragments(Vec<InkStroke>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PixelEraseError {
    Geometry(GeometryError),
    InvalidTransform { center: Vec2, rotation: f32 },
    InvalidClip(Rect),
    InvalidStroke(String),
    TooManyFragments { count: usize, limit: usize },
    TooManyPoints { count: usize, limit: usize },
    InvalidFragment { index: usize, reason: String },
}

impl fmt::Display for PixelEraseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Geometry(error) => write!(f, "{error}"),
            Self::InvalidTransform { center, rotation } => {
                write!(
                    f,
                    "invalid ink transform at {center:?} with rotation {rotation}"
                )
            }
            Self::InvalidClip(clip) => write!(f, "invalid ink clipping frame {clip:?}"),
            Self::InvalidStroke(error) => write!(f, "invalid ink stroke: {error}"),
            Self::TooManyFragments { count, limit } => {
                write!(
                    f,
                    "pixel erase produces {count} fragments; the limit is {limit}"
                )
            }
            Self::TooManyPoints { count, limit } => {
                write!(
                    f,
                    "pixel erase produces {count} points; the limit is {limit}"
                )
            }
            Self::InvalidFragment { index, reason } => {
                write!(f, "pixel erase fragment {index} is invalid: {reason}")
            }
        }
    }
}

impl std::error::Error for PixelEraseError {}

impl From<GeometryError> for PixelEraseError {
    fn from(error: GeometryError) -> Self {
        Self::Geometry(error)
    }
}

/// Split a stroke against a point or swept eraser path without mutating it.
///
/// The erased centerline is the path expanded by `radius + stroke.width / 2`,
/// so the resulting round caps do not paint back over the eraser. Boundary
/// points and optional pressure are linearly interpolated; tangency alone does
/// not erase. The returned fragments contain sheet-coordinate points and must
/// be inserted with rotation zero; copy the original object's style, band, and
/// other metadata separately. A fragment may be a single point.
///
/// With `clip`, only stroke centerlines and eraser center paths inside the
/// frame participate, matching scene hit-testing without deleting hidden ink.
/// The helper enforces per-stroke, per-sheet, object-count, and work limits on
/// its own output. The caller must additionally check the existing sheet's
/// remaining capacity and apply the replacement atomically. Any error leaves
/// the original stroke and path untouched.
pub fn pixel_erase_stroke(
    stroke: &InkStroke,
    path: &[Vec2],
    options: PixelEraseOptions,
) -> Result<PixelEraseResult, PixelEraseError> {
    validate_input_path(path)?;
    if !options.radius.is_finite() || !(0.0..=MAX_BOARD_COORDINATE).contains(&options.radius) {
        return Err(GeometryError::InvalidRadius(options.radius).into());
    }
    if !valid_board_position(options.center)
        || !options.rotation.is_finite()
        || !(0.0..360.0).contains(&options.rotation)
    {
        return Err(PixelEraseError::InvalidTransform {
            center: options.center,
            rotation: options.rotation,
        });
    }
    if let Some(clip) = options.clip
        && (!valid_rect(clip)
            || !valid_board_position(clip.min)
            || !valid_board_position(clip.max())
            || clip.size.x <= 0.0
            || clip.size.y <= 0.0
            || clip.size.x > MAX_BOARD_COORDINATE
            || clip.size.y > MAX_BOARD_COORDINATE)
    {
        return Err(PixelEraseError::InvalidClip(clip));
    }
    let object = CanvasObject::new(
        CanvasObjectKind::Ink(stroke.clone()),
        Rect::from_min_size(options.center, Vec2::ZERO),
    );
    validate_object(&object).map_err(PixelEraseError::InvalidStroke)?;

    let points: Vec<_> = stroke
        .points
        .iter()
        .map(|&point| rotate_around(point, options.center, options.rotation))
        .collect();
    let path_bounds = precise_points_bounds(path).ok_or(GeometryError::EmptyPath)?;
    let stroke_bounds = precise_points_bounds(&points)
        .ok_or_else(|| PixelEraseError::InvalidStroke("ink stroke has no points".to_owned()))?;
    let radius = f64::from(options.radius) + f64::from(stroke.width) / 2.0;
    if !precise_bounds_near(stroke_bounds, path_bounds, radius)
        || options.clip.is_some_and(|clip| {
            let max = clip.max();
            let bounds = (
                f64::from(clip.min.x),
                f64::from(clip.min.y),
                f64::from(max.x),
                f64::from(max.y),
            );
            !precise_bounds_near(bounds, path_bounds, 0.0)
        })
    {
        return Ok(PixelEraseResult::Unchanged);
    }

    let mut budget = GeometryBudget::new();
    if points.len() == 1 {
        if options.clip.is_some_and(|clip| !clip.contains(points[0])) {
            return Ok(PixelEraseResult::Unchanged);
        }
        for index in 0..segment_count(path, false) {
            budget.charge(1)?;
            let (a, b) = segment(path, false, index);
            let Some((a, b)) = options
                .clip
                .map_or(Some((a, b)), |clip| clip_segment(a, b, clip))
            else {
                continue;
            };
            if point_segment_distance_squared(points[0], a, b) < radius * radius {
                return Ok(PixelEraseResult::Erased);
            }
        }
        return Ok(PixelEraseResult::Unchanged);
    }

    let pressures = stroke.pressure.as_deref();
    let mut fragments = PixelFragments::default();
    let mut changed = false;
    for index in 0..points.len() - 1 {
        let a = PixelSample {
            point: points[index],
            pressure: pressures.map_or(0.0, |values| values[index]),
        };
        let b = PixelSample {
            point: points[index + 1],
            pressure: pressures.map_or(0.0, |values| values[index + 1]),
        };
        let mut cuts = Vec::new();
        let mut removes_a = false;
        let mut removes_b = false;
        let visible = options.clip.map_or(Some((0.0, 1.0)), |clip| {
            clip_segment_parameters(a.point, b.point, clip)
        });
        if let Some((visible_start, visible_end)) = visible {
            for path_index in 0..segment_count(path, false) {
                budget.charge(1)?;
                let (c, d) = segment(path, false, path_index);
                let Some((c, d)) = options
                    .clip
                    .map_or(Some((c, d)), |clip| clip_segment(c, d, clip))
                else {
                    continue;
                };
                if !segment_boxes_near(a.point, b.point, c, d, radius) {
                    continue;
                }
                if let Some((start, end)) = capsule_interval(a.point, b.point, c, d, radius) {
                    let start = start.max(visible_start);
                    let end = end.min(visible_end);
                    if start >= end {
                        continue;
                    }
                    let middle = (start + end) / 2.0;
                    let x = f64::from(a.point.x)
                        + middle * (f64::from(b.point.x) - f64::from(a.point.x));
                    let y = f64::from(a.point.y)
                        + middle * (f64::from(b.point.y) - f64::from(a.point.y));
                    if point_segment_distance_squared_xy(x, y, c, d) >= radius * radius {
                        continue;
                    }
                    removes_a |= start == 0.0
                        && point_segment_distance_squared(a.point, c, d) < radius * radius;
                    removes_b |= end == 1.0
                        && point_segment_distance_squared(b.point, c, d) < radius * radius;
                    cuts.push((start, end));
                }
            }
        }
        if cuts.len() > 1 {
            let sort_factor = usize::BITS as usize - (cuts.len() - 1).leading_zeros() as usize;
            budget.charge(cuts.len().saturating_mul(sort_factor))?;
            cuts.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        }

        let mut cursor = 0.0f64;
        let mut cut_index = 0;
        while cut_index < cuts.len() {
            let (start, mut end) = cuts[cut_index];
            cut_index += 1;
            while cut_index < cuts.len() && cuts[cut_index].0 < end {
                end = end.max(cuts[cut_index].1);
                cut_index += 1;
            }
            changed = true;
            if start == 0.0 && !removes_a && fragments.active.is_empty() {
                fragments.push(a)?;
            }
            if start == cursor && cursor > 0.0 && fragments.active.is_empty() {
                fragments.push(interpolate_pixel_sample(a, b, cursor))?;
            }
            if start > cursor {
                if fragments.active.is_empty() {
                    fragments.push(interpolate_pixel_sample(a, b, cursor))?;
                }
                fragments.push(interpolate_pixel_sample(a, b, start))?;
            }
            fragments.finish_active()?;
            cursor = cursor.max(end);
        }
        if cursor < 1.0 {
            if fragments.active.is_empty() {
                fragments.push(interpolate_pixel_sample(a, b, cursor))?;
            }
            fragments.push(b)?;
        } else if !removes_b {
            fragments.push(b)?;
        }
    }
    fragments.finish_active()?;
    if !changed {
        return Ok(PixelEraseResult::Unchanged);
    }
    if fragments.completed.is_empty() {
        return Ok(PixelEraseResult::Erased);
    }
    let mut result = Vec::with_capacity(fragments.completed.len());
    for (index, samples) in fragments.completed.into_iter().enumerate() {
        let points = samples.iter().map(|sample| sample.point).collect();
        let pressure = pressures.map(|_| {
            samples
                .iter()
                .map(|sample| sample.pressure)
                .collect::<Vec<_>>()
                .into()
        });
        let fragment = InkStroke {
            tool: stroke.tool,
            color: stroke.color,
            width: stroke.width,
            points,
            pressure,
        };
        validate_object(&CanvasObject::ink(fragment.clone()))
            .map_err(|reason| PixelEraseError::InvalidFragment { index, reason })?;
        result.push(fragment);
    }
    Ok(PixelEraseResult::Fragments(result))
}

/// Find whole ink objects touched by an eraser point or swept path, in paint order.
///
/// Shapes, topics, connectors, and frames do not obscure ink for this query.
/// Like scene hit-testing, a clipped item is only hit where the eraser's
/// center path lies inside its frame, even when its radius extends beyond it.
/// The scene has no lock information: the caller must filter `sheet.canvas.is_locked`
/// before passing the returned ids to a document edit.
pub fn eraser_hits(
    scene: &Scene,
    path: &[Vec2],
    radius: f32,
) -> Result<Vec<CanvasObjectId>, GeometryError> {
    validate_input_path(path)?;
    if !radius.is_finite() || !(0.0..=MAX_BOARD_COORDINATE).contains(&radius) {
        return Err(GeometryError::InvalidRadius(radius));
    }
    let path_bounds = precise_points_bounds(path).ok_or(GeometryError::EmptyPath)?;
    let mut budget = GeometryBudget::new();
    let mut seen = HashSet::new();
    let mut hits = Vec::new();
    for item in &scene.items {
        let (
            SceneId::Object(id),
            SceneGeometry::Ink {
                points,
                width,
                center,
                rotation,
            },
        ) = (item.id, &item.geometry)
        else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        if !ink_may_overlap(item, path_bounds, f64::from(radius), &mut budget)? {
            continue;
        }
        let max_distance = f64::from(radius) + f64::from(*width) / 2.0;
        if paths_touch(
            points,
            false,
            |point| rotate_around(point, *center, *rotation),
            SegmentPath {
                points: path,
                closed: false,
                clip: item.clip,
            },
            max_distance,
            &mut budget,
        )? {
            seen.insert(id);
            hits.push(id);
        }
    }
    Ok(hits)
}

/// Select scene topics and objects overlapping a closed, even-odd lasso, in paint order.
///
/// The final point is implicitly connected to the first. Zone frames are not
/// selection targets, matching `Scene::objects_in`; lock and group policy belongs
/// to the caller. A complexity error returns no partial selection.
pub fn lasso_hits(scene: &Scene, polygon: &[Vec2]) -> Result<Vec<SceneHit>, GeometryError> {
    validate_input_path(polygon)?;
    if polygon.len() < 3 || !has_non_collinear_points(polygon) {
        return Err(GeometryError::DegenerateLasso);
    }
    let lasso_bounds = precise_points_bounds(polygon).ok_or(GeometryError::EmptyPath)?;
    let mut budget = GeometryBudget::new();
    let mut clipped_lassos: HashMap<[u32; 4], Vec<Vec2>> = HashMap::new();
    let mut seen = HashSet::new();
    let mut hits = Vec::new();
    for item in &scene.items {
        let hit = match item.id {
            SceneId::Zone(_) => continue,
            SceneId::Topic(id) => SceneHit::Topic(id),
            SceneId::Object(id) => SceneHit::Object(id),
        };
        if seen.contains(&hit) {
            continue;
        }
        if matches!(item.geometry, SceneGeometry::Ink { .. }) {
            if !ink_may_overlap(item, lasso_bounds, 0.0, &mut budget)? {
                continue;
            }
        } else {
            let Some(visible_bounds) = visible_bounds(item)? else {
                continue;
            };
            if !precise_bounds_near(precise_rect_bounds(visible_bounds), lasso_bounds, 0.0) {
                continue;
            }
            validate_scene_geometry(item, &mut budget)?;
        }
        let polygon = if let Some(clip) = item.clip {
            let key = [
                clip.min.x.to_bits(),
                clip.min.y.to_bits(),
                clip.size.x.to_bits(),
                clip.size.y.to_bits(),
            ];
            match clipped_lassos.entry(key) {
                Entry::Occupied(entry) => entry.into_mut().as_slice(),
                Entry::Vacant(entry) => entry
                    .insert(clip_polygon(polygon, clip, &mut budget)?)
                    .as_slice(),
            }
        } else {
            polygon
        };
        if polygon.is_empty() {
            continue;
        }
        let intersects = match &item.geometry {
            SceneGeometry::Rect { rect, rotation } => {
                let corners = rotated_corners(*rect, *rotation);
                if corners.iter().any(|&point| !finite_point(point)) {
                    return Err(GeometryError::InvalidSceneGeometry(item.id));
                }
                polygon_touches_lasso(&corners, polygon, &mut budget)?
            }
            SceneGeometry::Polygon { outline, details } => {
                let mut intersects = polygon_touches_lasso(outline, polygon, &mut budget)?;
                for detail in details {
                    if intersects {
                        break;
                    }
                    intersects = path_touches_lasso(
                        detail,
                        false,
                        |point| point,
                        0.0,
                        polygon,
                        &mut budget,
                    )?;
                }
                intersects
            }
            SceneGeometry::Ink {
                points,
                width,
                center,
                rotation,
            } => path_touches_lasso(
                points,
                false,
                |point| rotate_around(point, *center, *rotation),
                f64::from(*width) / 2.0,
                polygon,
                &mut budget,
            )?,
            SceneGeometry::Polyline {
                points,
                width,
                markers,
            } => {
                let trimmed;
                let stroke = if markers.iter().any(Option::is_some) {
                    trimmed = connector_stroke_path(points, markers);
                    trimmed.as_slice()
                } else {
                    points.as_slice()
                };
                let mut intersects = path_touches_lasso(
                    stroke,
                    false,
                    |point| point,
                    f64::from(*width) / 2.0,
                    polygon,
                    &mut budget,
                )?;
                for marker in markers.iter().flatten() {
                    if intersects {
                        break;
                    }
                    intersects = if let Some((center, radius)) = marker.circle() {
                        circle_touches_lasso(
                            center,
                            radius,
                            marker.filled,
                            f64::from(*width) / 2.0,
                            polygon,
                            &mut budget,
                        )?
                    } else if let Some((path, error)) = marker.flattened_curve(*width) {
                        budget.charge(path.len())?;
                        path_touches_lasso(
                            &path,
                            false,
                            |point| point,
                            f64::from(*width) / 2.0 + f64::from(error),
                            polygon,
                            &mut budget,
                        )?
                    } else {
                        (marker.filled
                            && polygon_touches_lasso(marker.points(), polygon, &mut budget)?)
                            || path_touches_lasso(
                                marker.points(),
                                marker.closed,
                                |point| point,
                                f64::from(*width) / 2.0,
                                polygon,
                                &mut budget,
                            )?
                    };
                }
                intersects
            }
        };
        if intersects {
            seen.insert(hit);
            hits.push(hit);
        }
    }
    Ok(hits)
}

struct GeometryBudget {
    remaining: usize,
}

impl GeometryBudget {
    fn new() -> Self {
        Self {
            remaining: MAX_GEOMETRY_CHECKS,
        }
    }

    fn charge(&mut self, count: usize) -> Result<(), GeometryError> {
        self.remaining = self
            .remaining
            .checked_sub(count)
            .ok_or(GeometryError::TooComplex {
                limit: MAX_GEOMETRY_CHECKS,
            })?;
        Ok(())
    }
}

fn validate_input_path(points: &[Vec2]) -> Result<(), GeometryError> {
    if points.is_empty() {
        return Err(GeometryError::EmptyPath);
    }
    if points.len() > MAX_INPUT_SAMPLES {
        return Err(GeometryError::TooManyPoints {
            count: points.len(),
            limit: MAX_INPUT_SAMPLES,
        });
    }
    if let Some((index, &point)) = points
        .iter()
        .enumerate()
        .find(|(_, point)| !valid_board_position(**point))
    {
        return Err(GeometryError::InvalidPoint { index, point });
    }
    Ok(())
}

fn finite_point(point: Vec2) -> bool {
    point.x.is_finite()
        && point.y.is_finite()
        && point.x.abs() <= MAX_SAFE_SCENE_COORDINATE
        && point.y.abs() <= MAX_SAFE_SCENE_COORDINATE
}

fn valid_rect(rect: Rect) -> bool {
    finite_point(rect.min)
        && finite_point(rect.size)
        && rect.size.x >= 0.0
        && rect.size.y >= 0.0
        && finite_point(rect.max())
}

fn visible_bounds(item: &SceneItem) -> Result<Option<Rect>, GeometryError> {
    if !valid_rect(item.bounds) || item.clip.is_some_and(|clip| !valid_rect(clip)) {
        return Err(GeometryError::InvalidSceneGeometry(item.id));
    }
    Ok(item.visible_bounds())
}

fn validate_scene_geometry(
    item: &SceneItem,
    budget: &mut GeometryBudget,
) -> Result<(), GeometryError> {
    let invalid = || GeometryError::InvalidSceneGeometry(item.id);
    match &item.geometry {
        SceneGeometry::Rect { rect, rotation } => {
            budget.charge(4)?;
            if !valid_rect(*rect) || !rotation.is_finite() {
                return Err(invalid());
            }
        }
        SceneGeometry::Polygon { outline, details } => {
            let count = details.iter().fold(outline.len(), |sum, detail| {
                sum.saturating_add(detail.len())
            });
            if outline.is_empty() || count > MAX_INPUT_SAMPLES {
                return Err(invalid());
            }
            budget.charge(count)?;
            if outline
                .iter()
                .chain(details.iter().flatten())
                .any(|&point| !finite_point(point))
            {
                return Err(invalid());
            }
        }
        SceneGeometry::Ink {
            points,
            width,
            center,
            rotation,
        } => {
            if points.is_empty()
                || points.len() > MAX_STROKE_POINTS
                || !valid_width(*width)
                || !valid_board_position(*center)
                || !rotation.is_finite()
            {
                return Err(invalid());
            }
            budget.charge(points.len())?;
            if points.iter().any(|&point| !valid_board_position(point)) {
                return Err(invalid());
            }
        }
        SceneGeometry::Polyline {
            points,
            width,
            markers,
        } => {
            if points.is_empty() || points.len() > MAX_INPUT_SAMPLES || !valid_width(*width) {
                return Err(invalid());
            }
            budget.charge(points.len())?;
            if points.iter().any(|&point| !finite_point(point)) {
                return Err(invalid());
            }
            for marker in markers.iter().flatten() {
                budget.charge(marker.points().len())?;
                let invalid_circle = marker.circle().is_some_and(|(center, radius)| {
                    !finite_point(center)
                        || !radius.is_finite()
                        || radius < 0.0
                        || !(center.x + radius).is_finite()
                        || !(center.x - radius).is_finite()
                        || !(center.y + radius).is_finite()
                        || !(center.y - radius).is_finite()
                });
                if invalid_circle
                    || marker
                        .curve()
                        .is_some_and(|curve| curve.iter().any(|&point| !finite_point(point)))
                    || marker.points().iter().any(|&point| !finite_point(point))
                    || !marker.line_inset.is_finite()
                    || marker.line_inset < 0.0
                {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

fn ink_may_overlap(
    item: &SceneItem,
    query_bounds: (f64, f64, f64, f64),
    query_radius: f64,
    budget: &mut GeometryBudget,
) -> Result<bool, GeometryError> {
    // Scene rects are stored as f32 min + size and can round inward for long
    // strokes; derive the broad-phase bounds from the actual painted points.
    let _ = visible_bounds(item)?;
    if item
        .clip
        .is_some_and(|clip| !precise_bounds_near(precise_rect_bounds(clip), query_bounds, 0.0))
    {
        return Ok(false);
    }
    let SceneGeometry::Ink {
        points,
        width,
        center,
        rotation,
    } = &item.geometry
    else {
        unreachable!("ink overlap requires ink geometry");
    };
    validate_scene_geometry(item, budget)?;
    let bounds = precise_iter_bounds(
        points
            .iter()
            .map(|&point| rotate_around(point, *center, *rotation)),
    )
    .ok_or(GeometryError::InvalidSceneGeometry(item.id))?;
    Ok(precise_bounds_near(
        bounds,
        query_bounds,
        query_radius + f64::from(*width) / 2.0,
    ))
}

fn precise_rect_bounds(rect: Rect) -> (f64, f64, f64, f64) {
    let max = rect.max();
    (
        f64::from(rect.min.x),
        f64::from(rect.min.y),
        f64::from(max.x),
        f64::from(max.y),
    )
}

fn precise_points_bounds(points: &[Vec2]) -> Option<(f64, f64, f64, f64)> {
    precise_iter_bounds(points.iter().copied())
}

fn precise_iter_bounds(points: impl IntoIterator<Item = Vec2>) -> Option<(f64, f64, f64, f64)> {
    let mut points = points.into_iter();
    let first = points.next().filter(|&point| finite_point(point))?;
    let mut bounds = (
        f64::from(first.x),
        f64::from(first.y),
        f64::from(first.x),
        f64::from(first.y),
    );
    for point in points {
        if !finite_point(point) {
            return None;
        }
        bounds.0 = bounds.0.min(f64::from(point.x));
        bounds.1 = bounds.1.min(f64::from(point.y));
        bounds.2 = bounds.2.max(f64::from(point.x));
        bounds.3 = bounds.3.max(f64::from(point.y));
    }
    Some(bounds)
}

fn precise_bounds_near(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64), distance: f64) -> bool {
    a.0 <= b.2 + distance && b.0 <= a.2 + distance && a.1 <= b.3 + distance && b.1 <= a.3 + distance
}

fn has_non_collinear_points(points: &[Vec2]) -> bool {
    let first = points[0];
    let Some(second) = points.iter().copied().find(|&point| point != first) else {
        return false;
    };
    points
        .iter()
        .any(|&point| orientation(first, second, point) != 0.0)
}

fn clip_polygon(
    polygon: &[Vec2],
    clip: Rect,
    budget: &mut GeometryBudget,
) -> Result<Vec<Vec2>, GeometryError> {
    let max = clip.max();
    let mut current = polygon.to_vec();
    let mut next = Vec::new();
    for (x_axis, edge, keep_greater) in [
        (true, clip.min.x, true),
        (true, max.x, false),
        (false, clip.min.y, true),
        (false, max.y, false),
    ] {
        let Some(&last) = current.last() else {
            break;
        };
        next.clear();
        let component = |point: Vec2| if x_axis { point.x } else { point.y };
        let inside = |point: Vec2| {
            if keep_greater {
                component(point) >= edge
            } else {
                component(point) <= edge
            }
        };
        let mut previous = last;
        let mut previous_inside = inside(previous);
        for &point in &current {
            budget.charge(1)?;
            let point_inside = inside(point);
            if previous_inside != point_inside {
                let fraction = (f64::from(edge) - f64::from(component(previous)))
                    / (f64::from(component(point)) - f64::from(component(previous)));
                let interpolate = |a: f32, b: f32| {
                    (f64::from(a) + fraction * (f64::from(b) - f64::from(a))) as f32
                };
                let intersection = if x_axis {
                    Vec2::new(edge, interpolate(previous.y, point.y))
                } else {
                    Vec2::new(interpolate(previous.x, point.x), edge)
                };
                push_clipped_point(&mut next, intersection)?;
            }
            if point_inside {
                push_clipped_point(&mut next, point)?;
            }
            previous = point;
            previous_inside = point_inside;
        }
        if next.len() > 1 && next.first() == next.last() {
            next.pop();
        }
        std::mem::swap(&mut current, &mut next);
    }
    Ok(current)
}

fn push_clipped_point(points: &mut Vec<Vec2>, point: Vec2) -> Result<(), GeometryError> {
    if points.last() == Some(&point) {
        return Ok(());
    }
    if points.len() >= MAX_CLIPPED_LASSO_SAMPLES {
        return Err(GeometryError::ClippedLassoTooComplex {
            limit: MAX_CLIPPED_LASSO_SAMPLES,
        });
    }
    points.push(point);
    Ok(())
}

fn clip_segment_parameters(a: Vec2, b: Vec2, clip: Rect) -> Option<(f64, f64)> {
    let max = clip.max();
    let (ax, ay) = (f64::from(a.x), f64::from(a.y));
    let (dx, dy) = (f64::from(b.x) - ax, f64::from(b.y) - ay);
    let mut enter = 0.0f64;
    let mut leave = 1.0f64;
    for (direction, distance) in [
        (-dx, ax - f64::from(clip.min.x)),
        (dx, f64::from(max.x) - ax),
        (-dy, ay - f64::from(clip.min.y)),
        (dy, f64::from(max.y) - ay),
    ] {
        if direction == 0.0 {
            if distance < 0.0 {
                return None;
            }
            continue;
        }
        let fraction = distance / direction;
        if direction < 0.0 {
            enter = enter.max(fraction);
        } else {
            leave = leave.min(fraction);
        }
        if enter > leave {
            return None;
        }
    }
    Some((enter, leave))
}

fn clip_segment(a: Vec2, b: Vec2, clip: Rect) -> Option<(Vec2, Vec2)> {
    let (enter, leave) = clip_segment_parameters(a, b, clip)?;
    let max = clip.max();
    let (ax, ay) = (f64::from(a.x), f64::from(a.y));
    let (dx, dy) = (f64::from(b.x) - ax, f64::from(b.y) - ay);
    let point = |fraction: f64| {
        Vec2::new(
            ((ax + dx * fraction) as f32).clamp(clip.min.x, max.x),
            ((ay + dy * fraction) as f32).clamp(clip.min.y, max.y),
        )
    };
    Some((point(enter), point(leave)))
}

#[derive(Clone, Copy)]
struct PixelSample {
    point: Vec2,
    pressure: f32,
}

#[derive(Default)]
struct PixelFragments {
    active: Vec<PixelSample>,
    completed: Vec<Vec<PixelSample>>,
    total_points: usize,
}

impl PixelFragments {
    fn push(&mut self, sample: PixelSample) -> Result<(), PixelEraseError> {
        let count = self.active.len() + 1;
        if count > MAX_STROKE_POINTS {
            return Err(PixelEraseError::TooManyPoints {
                count,
                limit: MAX_STROKE_POINTS,
            });
        }
        let count = self.total_points + 1;
        if count > MAX_TOTAL_INK_POINTS {
            return Err(PixelEraseError::TooManyPoints {
                count,
                limit: MAX_TOTAL_INK_POINTS,
            });
        }
        self.active.push(sample);
        self.total_points = count;
        Ok(())
    }

    fn finish_active(&mut self) -> Result<(), PixelEraseError> {
        if self.active.is_empty() {
            return Ok(());
        }
        let count = self.completed.len() + 1;
        if count > MAX_CANVAS_OBJECTS {
            return Err(PixelEraseError::TooManyFragments {
                count,
                limit: MAX_CANVAS_OBJECTS,
            });
        }
        self.completed.push(std::mem::take(&mut self.active));
        Ok(())
    }
}

fn interpolate_pixel_sample(a: PixelSample, b: PixelSample, fraction: f64) -> PixelSample {
    if fraction <= 0.0 {
        return a;
    }
    if fraction >= 1.0 {
        return b;
    }
    let interpolate =
        |a: f32, b: f32| (f64::from(a) * (1.0 - fraction) + f64::from(b) * fraction) as f32;
    PixelSample {
        point: Vec2::new(
            interpolate(a.point.x, b.point.x),
            interpolate(a.point.y, b.point.y),
        ),
        pressure: interpolate(a.pressure, b.pressure).clamp(0.0, 1.0),
    }
}

fn disk_interval(a: Vec2, b: Vec2, center: Vec2, radius: f64) -> Option<(f64, f64)> {
    let (dx, dy) = (
        f64::from(b.x) - f64::from(a.x),
        f64::from(b.y) - f64::from(a.y),
    );
    let (fx, fy) = (
        f64::from(a.x) - f64::from(center.x),
        f64::from(a.y) - f64::from(center.y),
    );
    let length_squared = dx * dx + dy * dy;
    let radius_squared = radius * radius;
    if length_squared == 0.0 {
        return (fx * fx + fy * fy <= radius_squared).then_some((0.0, 1.0));
    }
    let projection = -(fx * dx + fy * dy) / length_squared;
    let (x, y) = (fx + projection * dx, fy + projection * dy);
    let distance_squared = x * x + y * y;
    if distance_squared > radius_squared {
        return None;
    }
    let half_span = ((radius_squared - distance_squared).max(0.0) / length_squared).sqrt();
    let start = (projection - half_span).max(0.0);
    let end = (projection + half_span).min(1.0);
    (start <= end).then_some((start, end))
}

fn constrain_interval(
    interval: &mut (f64, f64),
    offset: f64,
    slope: f64,
    lower: f64,
    upper: f64,
) -> bool {
    if slope == 0.0 {
        return (lower..=upper).contains(&offset);
    }
    let first = (lower - offset) / slope;
    let second = (upper - offset) / slope;
    interval.0 = interval.0.max(first.min(second));
    interval.1 = interval.1.min(first.max(second));
    interval.0 <= interval.1
}

fn union_interval(first: Option<(f64, f64)>, second: Option<(f64, f64)>) -> Option<(f64, f64)> {
    match (first, second) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))),
        (Some(interval), None) | (None, Some(interval)) => Some(interval),
        (None, None) => None,
    }
}

/// Intersect the ink segment with the capsule swept by one eraser segment.
fn capsule_interval(a: Vec2, b: Vec2, c: Vec2, d: Vec2, radius: f64) -> Option<(f64, f64)> {
    let mut result = disk_interval(a, b, c, radius);
    let (wx, wy) = (
        f64::from(d.x) - f64::from(c.x),
        f64::from(d.y) - f64::from(c.y),
    );
    let length_squared = wx * wx + wy * wy;
    if length_squared == 0.0 {
        return result;
    }
    result = union_interval(result, disk_interval(a, b, d, radius));
    let (fx, fy) = (
        f64::from(a.x) - f64::from(c.x),
        f64::from(a.y) - f64::from(c.y),
    );
    let (vx, vy) = (
        f64::from(b.x) - f64::from(a.x),
        f64::from(b.y) - f64::from(a.y),
    );
    let mut strip = (0.0, 1.0);
    let projection_ok = constrain_interval(
        &mut strip,
        fx * wx + fy * wy,
        vx * wx + vy * wy,
        0.0,
        length_squared,
    );
    let distance = radius * length_squared.sqrt();
    let distance_ok = constrain_interval(
        &mut strip,
        fx * wy - fy * wx,
        vx * wy - vy * wx,
        -distance,
        distance,
    );
    if projection_ok && distance_ok {
        result = union_interval(result, Some(strip));
    }
    result
}

fn circle_touches_lasso(
    center: Vec2,
    radius: f32,
    filled: bool,
    stroke_radius: f64,
    lasso: &[Vec2],
    budget: &mut GeometryBudget,
) -> Result<bool, GeometryError> {
    if radius <= 0.0 {
        return Ok(false);
    }
    budget.charge(lasso.len())?;
    let outer = f64::from(radius) + stroke_radius;
    let inner = (f64::from(radius) - stroke_radius).max(0.0);
    let distance_squared = |point: Vec2| {
        let dx = f64::from(point.x) - f64::from(center.x);
        let dy = f64::from(point.y) - f64::from(center.y);
        dx * dx + dy * dy
    };
    if (filled && distance_squared(lasso[0]) <= outer * outer)
        || point_in_polygon(Vec2::new(center.x + radius, center.y), lasso)
    {
        return Ok(true);
    }
    for index in 0..segment_count(lasso, true) {
        budget.charge(1)?;
        let (a, b) = segment(lasso, true, index);
        if point_segment_distance_squared(center, a, b) <= outer * outer
            && (filled || distance_squared(a).max(distance_squared(b)) >= inner * inner)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn polygon_touches_lasso(
    outline: &[Vec2],
    lasso: &[Vec2],
    budget: &mut GeometryBudget,
) -> Result<bool, GeometryError> {
    if outline.is_empty() {
        return Ok(false);
    }
    budget.charge(outline.len())?;
    if point_in_polygon(lasso[0], outline) {
        return Ok(true);
    }
    path_touches_lasso(outline, true, |point| point, 0.0, lasso, budget)
}

fn path_touches_lasso(
    points: &[Vec2],
    closed: bool,
    transform: impl Fn(Vec2) -> Vec2,
    radius: f64,
    lasso: &[Vec2],
    budget: &mut GeometryBudget,
) -> Result<bool, GeometryError> {
    let Some(&first) = points.first() else {
        return Ok(false);
    };
    budget.charge(lasso.len())?;
    if point_in_polygon(transform(first), lasso) {
        return Ok(true);
    }
    paths_touch(
        points,
        closed,
        transform,
        SegmentPath {
            points: lasso,
            closed: true,
            clip: None,
        },
        radius,
        budget,
    )
}

struct SegmentPath<'a> {
    points: &'a [Vec2],
    closed: bool,
    clip: Option<Rect>,
}

fn paths_touch(
    first: &[Vec2],
    first_closed: bool,
    transform_first: impl Fn(Vec2) -> Vec2,
    second: SegmentPath<'_>,
    radius: f64,
    budget: &mut GeometryBudget,
) -> Result<bool, GeometryError> {
    for first_index in 0..segment_count(first, first_closed) {
        let (a, b) = segment(first, first_closed, first_index);
        let (a, b) = (transform_first(a), transform_first(b));
        for second_index in 0..segment_count(second.points, second.closed) {
            budget.charge(1)?;
            let (c, d) = segment(second.points, second.closed, second_index);
            let Some((c, d)) = second
                .clip
                .map_or(Some((c, d)), |clip| clip_segment(c, d, clip))
            else {
                continue;
            };
            if segments_near(a, b, c, d, radius) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn segment_count(points: &[Vec2], closed: bool) -> usize {
    match points.len() {
        0 => 0,
        1 => 1,
        len if closed => len,
        len => len - 1,
    }
}

fn segment(points: &[Vec2], closed: bool, index: usize) -> (Vec2, Vec2) {
    if points.len() == 1 {
        return (points[0], points[0]);
    }
    let next = if closed && index + 1 == points.len() {
        0
    } else {
        index + 1
    };
    (points[index], points[next])
}

fn orientation(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    (f64::from(b.x) - f64::from(a.x)) * (f64::from(c.y) - f64::from(a.y))
        - (f64::from(b.y) - f64::from(a.y)) * (f64::from(c.x) - f64::from(a.x))
}

fn point_on_segment(a: Vec2, b: Vec2, point: Vec2) -> bool {
    point.x >= a.x.min(b.x)
        && point.x <= a.x.max(b.x)
        && point.y >= a.y.min(b.y)
        && point.y <= a.y.max(b.y)
}

fn segments_intersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let (abc, abd) = (orientation(a, b, c), orientation(a, b, d));
    let (cda, cdb) = (orientation(c, d, a), orientation(c, d, b));
    let opposite = |x: f64, y: f64| (x < 0.0 && y > 0.0) || (x > 0.0 && y < 0.0);
    (opposite(abc, abd) && opposite(cda, cdb))
        || (abc == 0.0 && point_on_segment(a, b, c))
        || (abd == 0.0 && point_on_segment(a, b, d))
        || (cda == 0.0 && point_on_segment(c, d, a))
        || (cdb == 0.0 && point_on_segment(c, d, b))
}

fn point_segment_distance_squared(point: Vec2, a: Vec2, b: Vec2) -> f64 {
    point_segment_distance_squared_xy(f64::from(point.x), f64::from(point.y), a, b)
}

fn point_segment_distance_squared_xy(x: f64, y: f64, a: Vec2, b: Vec2) -> f64 {
    let (ax, ay) = (f64::from(a.x), f64::from(a.y));
    let (dx, dy) = (f64::from(b.x) - ax, f64::from(b.y) - ay);
    let (px, py) = (x - ax, y - ay);
    let length_squared = dx * dx + dy * dy;
    let fraction = if length_squared == 0.0 {
        0.0
    } else {
        ((px * dx + py * dy) / length_squared).clamp(0.0, 1.0)
    };
    let (x, y) = (px - fraction * dx, py - fraction * dy);
    x * x + y * y
}

fn segment_boxes_near(a: Vec2, b: Vec2, c: Vec2, d: Vec2, radius: f64) -> bool {
    let separated = |a: f32, b: f32, c: f32, d: f32| {
        f64::from(a.max(b)) + radius < f64::from(c.min(d))
            || f64::from(c.max(d)) + radius < f64::from(a.min(b))
    };
    !separated(a.x, b.x, c.x, d.x) && !separated(a.y, b.y, c.y, d.y)
}

fn segments_near(a: Vec2, b: Vec2, c: Vec2, d: Vec2, radius: f64) -> bool {
    if !segment_boxes_near(a, b, c, d, radius) {
        return false;
    }
    if segments_intersect(a, b, c, d) {
        return true;
    }
    let distance = point_segment_distance_squared(a, c, d)
        .min(point_segment_distance_squared(b, c, d))
        .min(point_segment_distance_squared(c, a, b))
        .min(point_segment_distance_squared(d, a, b));
    distance <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_core::board::{MAX_TOTAL_INK_POINTS, ShapeKind};
    use thinoda_core::{
        ApproxMeasure, CanvasConnector, Document, Endpoint, Sheet, Workbook, build_scene,
        layout_sheet,
    };

    fn input() -> InkInput {
        InkInput::new(InkTool::Pen, Color::BLACK, 2.0).unwrap()
    }

    fn sample(x: f32, y: f32, pressure: Option<f32>) -> InkSample {
        InkSample::new(Vec2::new(x, y), pressure)
    }

    fn options(tolerance: f32) -> StrokeFinishOptions {
        StrokeFinishOptions {
            tolerance,
            smooth: false,
        }
    }

    fn bounds(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, height))
    }

    fn rectangle(x: f32, y: f32, width: f32, height: f32) -> [Vec2; 4] {
        [
            Vec2::new(x, y),
            Vec2::new(x + width, y),
            Vec2::new(x + width, y + height),
            Vec2::new(x, y + height),
        ]
    }

    fn scene(sheet: &Sheet) -> Scene {
        build_scene(sheet, &layout_sheet(sheet, &ApproxMeasure, false))
    }

    fn pixel_stroke(points: &[(f32, f32)], pressure: Option<Vec<f32>>) -> InkStroke {
        let mut stroke = InkStroke::new(
            InkTool::Highlighter,
            Color::rgb(10, 20, 30),
            2.0,
            points.iter().map(|&(x, y)| Vec2::new(x, y)).collect(),
        );
        stroke.pressure = pressure.map(Into::into);
        stroke
    }

    fn pixel_options(radius: f32) -> PixelEraseOptions {
        PixelEraseOptions {
            center: Vec2::ZERO,
            rotation: 0.0,
            radius,
            clip: None,
        }
    }

    fn pixel_fragments(result: PixelEraseResult) -> Vec<InkStroke> {
        match result {
            PixelEraseResult::Fragments(fragments) => fragments,
            other => panic!("expected fragments, got {other:?}"),
        }
    }

    fn near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1.0e-4,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn pixel_erase_splits_a_point_hit_and_interpolates_pressure() {
        let stroke = pixel_stroke(&[(0.0, 0.0), (10.0, 0.0)], Some(vec![0.0, 1.0]));
        let original = stroke.clone();
        let path = [Vec2::new(5.0, 0.0)];
        let fragments =
            pixel_fragments(pixel_erase_stroke(&stroke, &path, pixel_options(1.0)).unwrap());
        assert_eq!(stroke, original);
        assert_eq!(path, [Vec2::new(5.0, 0.0)]);
        assert_eq!(fragments.len(), 2);
        assert_eq!(&*fragments[0].points, &[Vec2::ZERO, Vec2::new(3.0, 0.0)]);
        assert_eq!(
            &*fragments[1].points,
            &[Vec2::new(7.0, 0.0), Vec2::new(10.0, 0.0)]
        );
        assert_eq!(&**fragments[0].pressure.as_ref().unwrap(), &[0.0, 0.3]);
        assert_eq!(&**fragments[1].pressure.as_ref().unwrap(), &[0.7, 1.0]);
        for fragment in fragments {
            assert_eq!(fragment.tool, stroke.tool);
            assert_eq!(fragment.color, stroke.color);
            assert_eq!(fragment.width, stroke.width);
            validate_object(&CanvasObject::ink(fragment)).unwrap();
        }
    }

    #[test]
    fn pixel_erase_sweeps_multiple_cuts_and_retains_intermediate_vertices() {
        let points: Vec<_> = (0..=10).map(|i| (i as f32 * 2.0, 0.0)).collect();
        let pressure: Vec<_> = (0..=10).map(|i| i as f32 / 10.0).collect();
        let stroke = pixel_stroke(&points, Some(pressure));
        let path = [
            Vec2::new(5.0, -10.0),
            Vec2::new(5.0, 10.0),
            Vec2::new(15.0, 10.0),
            Vec2::new(15.0, -10.0),
        ];
        let first = pixel_erase_stroke(&stroke, &path, pixel_options(0.5)).unwrap();
        assert_eq!(
            first,
            pixel_erase_stroke(&stroke, &path, pixel_options(0.5)).unwrap()
        );
        let fragments = pixel_fragments(first);
        let x_values: Vec<Vec<f32>> = fragments
            .iter()
            .map(|fragment| fragment.points.iter().map(|point| point.x).collect())
            .collect();
        assert_eq!(
            x_values,
            vec![
                vec![0.0, 2.0, 3.5],
                vec![6.5, 8.0, 10.0, 12.0, 13.5],
                vec![16.5, 18.0, 20.0]
            ]
        );
        for fragment in fragments {
            for (point, pressure) in fragment
                .points
                .iter()
                .zip(fragment.pressure.unwrap().iter())
            {
                near(*pressure, point.x / 20.0);
            }
        }
    }

    #[test]
    fn pixel_erase_preserves_duplicate_samples_and_missing_pressure() {
        let points = [(0.0, 0.0), (2.0, 0.0), (2.0, 0.0), (4.0, 0.0), (10.0, 0.0)];
        let stroke = pixel_stroke(&points, Some(vec![0.0, 0.2, 0.3, 0.4, 1.0]));
        let path = [Vec2::new(7.0, 0.0)];
        let fragments =
            pixel_fragments(pixel_erase_stroke(&stroke, &path, pixel_options(0.0)).unwrap());
        assert_eq!(
            &*fragments[0].points,
            &[
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(4.0, 0.0),
                Vec2::new(6.0, 0.0)
            ]
        );
        assert_eq!(
            &**fragments[0].pressure.as_ref().unwrap(),
            &[0.0, 0.2, 0.3, 0.4, 0.6]
        );
        assert_eq!(&**fragments[1].pressure.as_ref().unwrap(), &[0.8, 1.0]);

        let mouse = pixel_stroke(&points, None);
        let fragments =
            pixel_fragments(pixel_erase_stroke(&mouse, &path, pixel_options(0.0)).unwrap());
        assert!(fragments.iter().all(|fragment| fragment.pressure.is_none()));
    }

    #[test]
    fn pixel_erase_flattens_rotation_into_sheet_coordinates() {
        let stroke = pixel_stroke(&[(0.0, 0.0), (10.0, 0.0)], Some(vec![0.0, 1.0]));
        let options = PixelEraseOptions {
            center: Vec2::new(5.0, 0.0),
            rotation: 90.0,
            ..pixel_options(1.0)
        };
        let fragments =
            pixel_fragments(pixel_erase_stroke(&stroke, &[Vec2::new(5.0, 0.0)], options).unwrap());
        assert_eq!(fragments.len(), 2);
        for (fragment, expected_y) in fragments.iter().zip([[-5.0, -2.0], [2.0, 5.0]]) {
            for (point, y) in fragment.points.iter().zip(expected_y) {
                near(point.x, 5.0);
                near(point.y, y);
            }
            let object = CanvasObject::ink(fragment.clone());
            assert_eq!(object.rotation, 0.0);
            validate_object(&object).unwrap();
        }
        near(fragments[0].pressure.as_ref().unwrap()[1], 0.3);
        near(fragments[1].pressure.as_ref().unwrap()[0], 0.7);
    }

    #[test]
    fn pixel_erase_handles_dots_tangencies_and_one_point_fragments() {
        let dot = pixel_stroke(&[(0.0, 0.0)], Some(vec![0.4]));
        assert_eq!(
            pixel_erase_stroke(&dot, &[Vec2::new(2.0, 0.0)], pixel_options(1.0)).unwrap(),
            PixelEraseResult::Unchanged
        );
        assert_eq!(
            pixel_erase_stroke(&dot, &[Vec2::new(1.99, 0.0)], pixel_options(1.0)).unwrap(),
            PixelEraseResult::Erased
        );

        let line = pixel_stroke(&[(0.0, 0.0), (10.0, 0.0)], Some(vec![0.2, 0.8]));
        let fragments = pixel_fragments(
            pixel_erase_stroke(&line, &[Vec2::new(5.0, 0.0)], pixel_options(4.0)).unwrap(),
        );
        assert_eq!(fragments.len(), 2);
        assert_eq!(&*fragments[0].points, &[Vec2::ZERO]);
        assert_eq!(&*fragments[1].points, &[Vec2::new(10.0, 0.0)]);
        assert_eq!(&**fragments[0].pressure.as_ref().unwrap(), &[0.2]);
        assert_eq!(&**fragments[1].pressure.as_ref().unwrap(), &[0.8]);

        let tangent = pixel_stroke(&[(-10.0, 0.0), (10.0, 0.0)], None);
        assert_eq!(
            pixel_erase_stroke(&tangent, &[Vec2::new(0.0, 2.0)], pixel_options(1.0)).unwrap(),
            PixelEraseResult::Unchanged
        );
        assert_eq!(
            pixel_erase_stroke(
                &tangent,
                &[Vec2::new(-8.0, 2.0), Vec2::new(8.0, 2.0)],
                pixel_options(1.0)
            )
            .unwrap(),
            PixelEraseResult::Unchanged
        );
        let repeated = pixel_stroke(&[(0.0, 0.0), (0.0, 0.0), (0.0, 0.0)], None);
        assert_eq!(
            pixel_erase_stroke(&repeated, &[Vec2::new(0.0, 0.0)], pixel_options(1.0)).unwrap(),
            PixelEraseResult::Erased
        );
        assert_eq!(
            pixel_erase_stroke(&repeated, &[Vec2::new(2.0, 0.0)], pixel_options(1.0)).unwrap(),
            PixelEraseResult::Unchanged
        );
    }

    #[test]
    fn pixel_erase_preserves_an_isolated_vertex_between_two_cuts() {
        let path = [
            Vec2::new(2.5, 0.0),
            Vec2::new(2.5, 10.0),
            Vec2::new(7.5, 10.0),
            Vec2::new(7.5, 0.0),
        ];
        for stroke in [
            pixel_stroke(
                &[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)],
                Some(vec![0.0, 0.5, 1.0]),
            ),
            pixel_stroke(&[(0.0, 0.0), (10.0, 0.0)], Some(vec![0.0, 1.0])),
        ] {
            let fragments =
                pixel_fragments(pixel_erase_stroke(&stroke, &path, pixel_options(1.5)).unwrap());
            assert_eq!(fragments.len(), 3);
            for (fragment, (x, pressure)) in
                fragments.iter().zip([(0.0, 0.0), (5.0, 0.5), (10.0, 1.0)])
            {
                assert_eq!(&*fragment.points, &[Vec2::new(x, 0.0)]);
                assert_eq!(&**fragment.pressure.as_ref().unwrap(), &[pressure]);
                validate_object(&CanvasObject::ink(fragment.clone())).unwrap();
            }
        }
    }

    #[test]
    fn pixel_erase_restricts_both_paths_to_the_clip() {
        let stroke = pixel_stroke(&[(0.0, 0.0), (100.0, 0.0)], Some(vec![0.0, 1.0]));
        let options = PixelEraseOptions {
            clip: Some(bounds(40.0, -10.0, 20.0, 20.0)),
            ..pixel_options(1.0)
        };
        let path = [Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0)];
        let fragments = pixel_fragments(pixel_erase_stroke(&stroke, &path, options).unwrap());
        assert_eq!(&*fragments[0].points, &[Vec2::ZERO, Vec2::new(40.0, 0.0)]);
        assert_eq!(
            &*fragments[1].points,
            &[Vec2::new(60.0, 0.0), Vec2::new(100.0, 0.0)]
        );
        near(fragments[0].pressure.as_ref().unwrap()[1], 0.4);
        near(fragments[1].pressure.as_ref().unwrap()[0], 0.6);
        assert_eq!(
            pixel_erase_stroke(&stroke, &[Vec2::new(39.0, 0.0)], options).unwrap(),
            PixelEraseResult::Unchanged
        );
    }

    #[test]
    fn pixel_erase_capsules_cover_sweeps_and_round_endpoints() {
        let stroke = pixel_stroke(&[(0.0, 0.0), (20.0, 0.0)], None);
        let path = [Vec2::new(5.0, 1.0), Vec2::new(15.0, 1.0)];
        let fragments =
            pixel_fragments(pixel_erase_stroke(&stroke, &path, pixel_options(1.0)).unwrap());
        near(fragments[0].points.last().unwrap().x, 5.0 - 3.0f32.sqrt());
        near(fragments[1].points.first().unwrap().x, 15.0 + 3.0f32.sqrt());

        for (a, b, c, d, radius) in [
            (
                Vec2::new(-5.0, 0.0),
                Vec2::new(5.0, 0.0),
                Vec2::ZERO,
                Vec2::ZERO,
                2.0,
            ),
            (
                Vec2::new(-5.0, 1.0),
                Vec2::new(5.0, 1.0),
                Vec2::new(-2.0, 0.0),
                Vec2::new(2.0, 0.0),
                2.0,
            ),
            (
                Vec2::new(-3.0, -3.0),
                Vec2::new(3.0, 3.0),
                Vec2::new(0.0, -2.0),
                Vec2::new(0.0, 2.0),
                0.5,
            ),
            (
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                1.5,
            ),
            (
                Vec2::new(-5.0, 3.0),
                Vec2::new(5.0, 3.0),
                Vec2::new(-2.0, 0.0),
                Vec2::new(2.0, 0.0),
                1.0,
            ),
        ] {
            let interval = capsule_interval(a, b, c, d, radius);
            for i in 0..=100 {
                let t = f64::from(i) / 100.0;
                let x = f64::from(a.x) + t * (f64::from(b.x) - f64::from(a.x));
                let y = f64::from(a.y) + t * (f64::from(b.y) - f64::from(a.y));
                let distance = point_segment_distance_squared_xy(x, y, c, d).sqrt();
                let in_interval =
                    interval.is_some_and(|(start, end)| t >= start - 1.0e-10 && t <= end + 1.0e-10);
                if distance < radius - 1.0e-9 {
                    assert!(in_interval, "missed {t} in {interval:?}");
                } else if distance > radius + 1.0e-9 {
                    assert!(!in_interval, "included {t} in {interval:?}");
                }
            }
        }
    }

    #[test]
    fn pixel_erase_does_not_miss_small_hits_on_long_paths() {
        let mut long_stroke = pixel_stroke(&[(-1_000_000.0, 0.0), (0.03, 0.0)], None);
        long_stroke.width = 0.1;
        let fragments = pixel_fragments(
            pixel_erase_stroke(&long_stroke, &[Vec2::new(0.06, 0.0)], pixel_options(0.0)).unwrap(),
        );
        assert_eq!(fragments.len(), 1);
        near(fragments[0].points.last().unwrap().x, 0.01);

        let mut dot = pixel_stroke(&[(0.06, 0.0)], None);
        dot.width = 0.1;
        assert_eq!(
            pixel_erase_stroke(
                &dot,
                &[Vec2::new(-1_000_000.0, 0.0), Vec2::new(0.03, 0.0)],
                pixel_options(0.0),
            )
            .unwrap(),
            PixelEraseResult::Erased
        );
    }

    #[test]
    fn pixel_erase_rejects_bad_inputs_and_unrepresentable_rotated_fragments() {
        let stroke = pixel_stroke(&[(0.0, 0.0), (10.0, 0.0)], Some(vec![0.0, 1.0]));
        let original = stroke.clone();
        let path = [Vec2::new(5.0, 0.0)];
        assert!(matches!(
            pixel_erase_stroke(&stroke, &[], pixel_options(1.0)),
            Err(PixelEraseError::Geometry(GeometryError::EmptyPath))
        ));
        assert!(matches!(
            pixel_erase_stroke(&stroke, &[Vec2::new(f32::NAN, 0.0)], pixel_options(1.0)),
            Err(PixelEraseError::Geometry(GeometryError::InvalidPoint {
                index: 0,
                ..
            }))
        ));
        let too_long = vec![Vec2::ZERO; MAX_INPUT_SAMPLES + 1];
        assert!(matches!(
            pixel_erase_stroke(&stroke, &too_long, pixel_options(1.0)),
            Err(PixelEraseError::Geometry(
                GeometryError::TooManyPoints { .. }
            ))
        ));
        for radius in [-1.0, f32::NAN, f32::INFINITY, MAX_BOARD_COORDINATE + 1.0] {
            assert!(matches!(
                pixel_erase_stroke(&stroke, &path, pixel_options(radius)),
                Err(PixelEraseError::Geometry(GeometryError::InvalidRadius(_)))
            ));
        }
        for rotation in [-1.0, 360.0, f32::NAN] {
            assert!(matches!(
                pixel_erase_stroke(
                    &stroke,
                    &path,
                    PixelEraseOptions {
                        rotation,
                        ..pixel_options(1.0)
                    }
                ),
                Err(PixelEraseError::InvalidTransform { .. })
            ));
        }
        assert!(matches!(
            pixel_erase_stroke(
                &stroke,
                &path,
                PixelEraseOptions {
                    center: Vec2::new(f32::INFINITY, 0.0),
                    ..pixel_options(1.0)
                }
            ),
            Err(PixelEraseError::InvalidTransform { .. })
        ));
        assert!(matches!(
            pixel_erase_stroke(
                &stroke,
                &path,
                PixelEraseOptions {
                    clip: Some(bounds(0.0, 0.0, 0.0, 1.0)),
                    ..pixel_options(1.0)
                }
            ),
            Err(PixelEraseError::InvalidClip(_))
        ));
        let mut bad_pressure = stroke.clone();
        bad_pressure.pressure = Some(vec![0.5].into());
        assert!(matches!(
            pixel_erase_stroke(&bad_pressure, &path, pixel_options(1.0)),
            Err(PixelEraseError::InvalidStroke(_))
        ));

        let rotated = pixel_stroke(&[(900_000.0, 100_000.0), (1_000_000.0, 0.0)], None);
        let rotated_original = rotated.clone();
        let options = PixelEraseOptions {
            center: Vec2::new(950_000.0, 50_000.0),
            rotation: 45.0,
            ..pixel_options(100.0)
        };
        assert!(matches!(
            pixel_erase_stroke(&rotated, &[Vec2::new(900_000.0, 50_000.0)], options),
            Err(PixelEraseError::InvalidFragment { .. })
        ));
        assert_eq!(rotated, rotated_original);
        assert_eq!(stroke, original);
        assert_eq!(path, [Vec2::new(5.0, 0.0)]);
    }

    #[test]
    fn pixel_erase_enforces_complexity_and_fragment_limits_without_mutation() {
        let stroke = pixel_stroke(&vec![(0.0, 0.0); 1_000], None);
        let original = stroke.clone();
        let path = vec![Vec2::ZERO; 2_000];
        assert_eq!(
            pixel_erase_stroke(&stroke, &path, pixel_options(1.0)),
            Err(PixelEraseError::Geometry(GeometryError::TooComplex {
                limit: MAX_GEOMETRY_CHECKS
            }))
        );
        assert_eq!(stroke, original);
        assert!(path.iter().all(|point| *point == Vec2::ZERO));

        let mut stroke = pixel_stroke(&[(0.0, 0.0), (60_004.0, 0.0)], None);
        stroke.width = 0.1;
        let original = stroke.clone();
        let mut path = Vec::with_capacity(MAX_CANVAS_OBJECTS * 2);
        for i in 0..MAX_CANVAS_OBJECTS {
            let x = i as f32 * 3.0 + 1.0;
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            path.extend([Vec2::new(x, -10.0 * sign), Vec2::new(x, 10.0 * sign)]);
        }
        assert_eq!(
            pixel_erase_stroke(&stroke, &path, pixel_options(0.05)),
            Err(PixelEraseError::TooManyFragments {
                count: MAX_CANVAS_OBJECTS + 1,
                limit: MAX_CANVAS_OBJECTS
            })
        );
        assert_eq!(stroke, original);
        assert_eq!(path.len(), MAX_CANVAS_OBJECTS * 2);

        let sample = PixelSample {
            point: Vec2::ZERO,
            pressure: 0.5,
        };
        let mut full_fragment = PixelFragments {
            active: vec![sample; MAX_STROKE_POINTS],
            total_points: MAX_STROKE_POINTS,
            ..PixelFragments::default()
        };
        assert_eq!(
            full_fragment.push(sample),
            Err(PixelEraseError::TooManyPoints {
                count: MAX_STROKE_POINTS + 1,
                limit: MAX_STROKE_POINTS
            })
        );
        let mut full_output = PixelFragments {
            total_points: MAX_TOTAL_INK_POINTS,
            ..PixelFragments::default()
        };
        assert_eq!(
            full_output.push(sample),
            Err(PixelEraseError::TooManyPoints {
                count: MAX_TOTAL_INK_POINTS + 1,
                limit: MAX_TOTAL_INK_POINTS
            })
        );
    }

    #[test]
    fn samples_are_transient_and_a_committed_stroke_is_one_undo_step() {
        let mut document = Document::default();
        let mut drawing = InkInput::new(InkTool::Highlighter, Color::rgb(10, 20, 30), 8.0).unwrap();
        drawing.push(sample(0.0, 0.0, None)).unwrap();
        drawing.push(sample(2.0, 2.0, None)).unwrap();
        drawing.push(sample(4.0, 0.0, None)).unwrap();
        assert_eq!(drawing.len(), 3);
        assert!(!drawing.is_empty());
        assert_eq!(drawing.tool(), InkTool::Highlighter);
        assert_eq!(drawing.color(), Color::rgb(10, 20, 30));
        assert_eq!(drawing.width(), 8.0);
        assert!(!document.can_undo());
        assert!(document.sheet().canvas.objects.is_empty());

        let mut cancelled = input();
        cancelled.push(sample(50.0, 50.0, None)).unwrap();
        cancelled.cancel();
        assert!(!document.can_undo());
        assert!(document.sheet().canvas.objects.is_empty());

        let stroke = drawing.finish(options(0.0)).unwrap();
        assert_eq!(drawing.samples().len(), 3);
        document
            .add_canvas_object(CanvasObject::ink(stroke))
            .unwrap();
        assert_eq!(document.undo_label(), Some("Draw"));
        assert_eq!(document.sheet().canvas.objects.len(), 1);
        drawing.cancel();
        assert!(document.undo());
        assert!(document.sheet().canvas.objects.is_empty());
        assert!(!document.can_undo());
        assert!(document.redo());
        assert_eq!(document.sheet().canvas.objects.len(), 1);
    }

    #[test]
    fn missing_pressure_is_optional_and_interpolated_without_losing_measurements() {
        let mut drawing = input();
        for sample in [
            sample(0.0, 0.0, None),
            sample(1.0, 0.0, Some(0.2)),
            sample(2.0, 0.0, None),
            sample(3.0, 0.0, Some(0.8)),
            sample(4.0, 0.0, None),
        ] {
            drawing.push(sample).unwrap();
        }
        let stroke = drawing.finish(options(0.0)).unwrap();
        let pressures = stroke.pressure.as_ref().unwrap();
        assert_eq!(pressures.len(), stroke.points.len());
        assert_eq!(pressures[0], 0.2);
        assert_eq!(pressures[1], 0.2);
        assert!((pressures[2] - 0.5).abs() < 1.0e-6);
        assert_eq!(pressures[3], 0.8);
        assert_eq!(pressures[4], 0.8);
        assert_eq!(drawing.samples()[0].pressure, None);
        assert_eq!(drawing.samples()[2].pressure, None);

        let mut mouse = input();
        mouse.push(sample(1.0, 1.0, None)).unwrap();
        assert!(mouse.finish(options(0.0)).unwrap().pressure.is_none());
    }

    #[test]
    fn simplification_preserves_endpoints_pressure_peaks_and_alignment() {
        let mut drawing = input();
        let pressures = [0.2, 0.2, 1.0, 0.2, 0.2];
        for (index, pressure) in pressures.iter().copied().enumerate() {
            drawing
                .push(sample(index as f32, 0.0, Some(pressure)))
                .unwrap();
        }
        let first = drawing.finish(options(100.0)).unwrap();
        let second = drawing.finish(options(100.0)).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.points.first(), Some(&Vec2::ZERO));
        assert_eq!(first.points.last(), Some(&Vec2::new(4.0, 0.0)));
        let kept_pressure = first.pressure.as_ref().unwrap();
        assert_eq!(first.points.len(), kept_pressure.len());
        for (point, &pressure) in first.points.iter().zip(kept_pressure.iter()) {
            assert_eq!(pressure, pressures[point.x as usize]);
        }
        assert!(kept_pressure.contains(&1.0));

        let mut mouse = input();
        for index in 0..5 {
            mouse.push(sample(index as f32, 0.0, None)).unwrap();
        }
        let stroke = mouse.finish(options(100.0)).unwrap();
        assert_eq!(&*stroke.points, &[Vec2::ZERO, Vec2::new(4.0, 0.0)]);
        assert!(stroke.pressure.is_none());
    }

    #[test]
    fn smoothing_is_opt_in_and_keeps_endpoints_and_paired_pressure() {
        let mut drawing = input();
        drawing.push(sample(0.0, 0.0, Some(0.1))).unwrap();
        drawing.push(sample(10.0, 10.0, Some(0.8))).unwrap();
        drawing.push(sample(20.0, 0.0, Some(0.3))).unwrap();
        let stroke = drawing
            .finish(StrokeFinishOptions {
                tolerance: 0.0,
                smooth: true,
            })
            .unwrap();
        assert_eq!(
            &*stroke.points,
            &[Vec2::ZERO, Vec2::new(10.0, 5.0), Vec2::new(20.0, 0.0)]
        );
        assert_eq!(&**stroke.pressure.as_ref().unwrap(), &[0.1, 0.8, 0.3]);
        assert_eq!(drawing.samples()[1].position, Vec2::new(10.0, 10.0));
    }

    #[test]
    fn dots_duplicate_points_and_closed_strokes_remain_valid() {
        let mut dot = input();
        dot.push(sample(3.0, 4.0, Some(0.0))).unwrap();
        let stroke = dot.finish(options(1.0)).unwrap();
        assert_eq!(&*stroke.points, &[Vec2::new(3.0, 4.0)]);
        assert_eq!(&**stroke.pressure.as_ref().unwrap(), &[0.0]);
        validate_object(&CanvasObject::ink(stroke)).unwrap();

        let mut repeated = input();
        for _ in 0..5 {
            repeated.push(sample(3.0, 4.0, None)).unwrap();
        }
        let stroke = repeated.finish(options(1.0)).unwrap();
        assert_eq!(&*stroke.points, &[Vec2::new(3.0, 4.0); 2]);
        validate_object(&CanvasObject::ink(stroke)).unwrap();

        let mut looped = input();
        for point in [(0.0, 0.0), (10.0, 10.0), (0.0, 0.0)] {
            looped.push(sample(point.0, point.1, None)).unwrap();
        }
        let stroke = looped.finish(options(0.5)).unwrap();
        assert_eq!(
            &*stroke.points,
            &[Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::ZERO]
        );
    }

    #[test]
    fn invalid_samples_are_returned_and_do_not_mutate_the_buffer() {
        let mut drawing = input();
        let initial = sample(10.0, 20.0, Some(0.5));
        drawing.push(initial).unwrap();
        for position in [
            Vec2::new(f32::NAN, 1.0),
            Vec2::new(0.0, f32::INFINITY),
            Vec2::new(MAX_BOARD_COORDINATE + 1.0, 0.0),
        ] {
            let rejected = drawing.push(InkSample::new(position, None)).unwrap_err();
            assert_eq!(rejected.reason, SampleRejection::InvalidPosition);
            assert_eq!(drawing.samples(), &[initial]);
        }
        for pressure in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
            let rejected = drawing
                .push(sample(11.0, 20.0, Some(pressure)))
                .unwrap_err();
            assert_eq!(rejected.reason, SampleRejection::InvalidPressure);
            assert!(rejected.sample.pressure.unwrap().to_bits() == pressure.to_bits());
            assert_eq!(drawing.samples(), &[initial]);
        }
        drawing.push(sample(11.0, 20.0, Some(1.0))).unwrap();
        assert_eq!(drawing.len(), 2);
    }

    #[test]
    fn frame_limit_rejects_the_new_sample_without_changing_existing_points() {
        let mut drawing = input();
        let first = sample(-MAX_BOARD_COORDINATE, 0.0, None);
        let last = sample(0.0, 0.0, None);
        drawing.push(first).unwrap();
        drawing.push(last).unwrap();
        let beyond = sample(0.25, 0.0, None);
        let rejected = drawing.push(beyond).unwrap_err();
        assert_eq!(rejected.sample, beyond);
        assert_eq!(rejected.reason, SampleRejection::BoundsExceeded);
        assert_eq!(drawing.samples(), &[first, last]);
        validate_object(&CanvasObject::ink(drawing.finish(options(0.0)).unwrap())).unwrap();
    }

    #[test]
    fn width_and_finish_errors_are_explicit_and_retryable() {
        for width in [f32::NAN, f32::INFINITY, 0.0, MAX_STROKE_WIDTH + 1.0] {
            assert!(matches!(
                InkInput::new(InkTool::Pen, Color::BLACK, width),
                Err(InkInputError::InvalidWidth(_))
            ));
        }
        assert!(InkInput::new(InkTool::Pen, Color::BLACK, MIN_STROKE_WIDTH).is_ok());
        assert!(InkInput::new(InkTool::Pen, Color::BLACK, MAX_STROKE_WIDTH).is_ok());

        let mut drawing = input();
        assert_eq!(
            drawing.finish(options(0.0)),
            Err(InkInputError::EmptyStroke)
        );
        let point = sample(1.0, 2.0, None);
        drawing.push(point).unwrap();
        for tolerance in [f32::NAN, f32::INFINITY, -1.0, MAX_BOARD_COORDINATE + 1.0] {
            assert!(matches!(
                drawing.finish(options(tolerance)),
                Err(InkInputError::InvalidTolerance(_))
            ));
            assert_eq!(drawing.samples(), &[point]);
        }
        assert_eq!(
            &*drawing.finish(options(0.0)).unwrap().points,
            &[point.position]
        );
    }

    #[test]
    fn long_strokes_simplify_below_the_persistent_limit_or_can_be_retried() {
        let mut drawing = input();
        for index in 0..=MAX_STROKE_POINTS {
            drawing.push(sample(index as f32, 0.0, None)).unwrap();
        }
        assert_eq!(
            drawing.finish(options(0.0)),
            Err(InkInputError::TooManyPoints {
                count: MAX_STROKE_POINTS + 1,
                limit: MAX_STROKE_POINTS,
            })
        );
        assert_eq!(drawing.len(), MAX_STROKE_POINTS + 1);
        let stroke = drawing.finish(options(0.5)).unwrap();
        assert_eq!(
            &*stroke.points,
            &[Vec2::ZERO, Vec2::new(MAX_STROKE_POINTS as f32, 0.0)]
        );
        validate_object(&CanvasObject::ink(stroke)).unwrap();
    }

    #[test]
    fn transient_sample_cap_returns_the_rejected_sample() {
        let mut drawing = input();
        let point = sample(1.0, 2.0, None);
        for _ in 0..MAX_INPUT_SAMPLES {
            drawing.push(point).unwrap();
        }
        let rejected = drawing.push(sample(2.0, 3.0, None)).unwrap_err();
        assert_eq!(
            rejected.reason,
            SampleRejection::TooManySamples {
                limit: MAX_INPUT_SAMPLES
            }
        );
        assert_eq!(rejected.sample, sample(2.0, 3.0, None));
        assert_eq!(drawing.len(), MAX_INPUT_SAMPLES);
        let stroke = drawing.finish(options(0.5)).unwrap();
        assert_eq!(&*stroke.points, &[point.position, point.position]);
        assert_eq!(drawing.samples().last(), Some(&point));
    }

    #[test]
    fn pathological_simplification_is_bounded_and_can_retry_without_simplifying() {
        let mut drawing = input();
        for index in 0..MAX_STROKE_POINTS {
            let y = if index % 2 == 0 { 0.0 } else { 10.0 };
            drawing.push(sample(index as f32, y, None)).unwrap();
        }
        assert_eq!(
            drawing.finish(options(0.1)),
            Err(InkInputError::SimplificationTooComplex {
                limit: MAX_SIMPLIFICATION_CHECKS,
            })
        );
        assert_eq!(drawing.len(), MAX_STROKE_POINTS);
        let stroke = drawing.finish(options(0.0)).unwrap();
        assert_eq!(stroke.points.len(), MAX_STROKE_POINTS);
        assert_eq!(stroke.points[0], drawing.samples()[0].position);
        assert_eq!(
            stroke.points[MAX_STROKE_POINTS - 1],
            drawing.samples()[MAX_STROKE_POINTS - 1].position
        );
    }

    #[test]
    fn failed_document_insertion_keeps_the_gesture_for_retry() {
        let mut workbook = Workbook::default();
        let full_stroke = InkStroke::new(
            InkTool::Pen,
            Color::BLACK,
            1.0,
            vec![Vec2::ZERO; MAX_STROKE_POINTS],
        );
        for _ in 0..MAX_TOTAL_INK_POINTS / MAX_STROKE_POINTS {
            workbook
                .active_mut()
                .add_canvas_object(CanvasObject::ink(full_stroke.clone()))
                .unwrap();
        }
        let first = workbook.active().canvas.objects[0].id;
        let mut document = Document::new(workbook);
        let mut drawing = input();
        drawing.push(sample(20.0, 30.0, Some(0.4))).unwrap();
        drawing.push(sample(25.0, 35.0, Some(0.8))).unwrap();
        let pending = drawing.finish(options(0.0)).unwrap();
        assert!(
            document
                .add_canvas_object(CanvasObject::ink(pending))
                .is_err()
        );
        assert!(!document.can_undo());
        assert_eq!(
            document.sheet().canvas.ink_point_count(),
            MAX_TOTAL_INK_POINTS
        );
        assert_eq!(drawing.len(), 2);

        document.remove_canvas_objects(&[first]).unwrap();
        document
            .add_canvas_object(CanvasObject::ink(drawing.finish(options(0.0)).unwrap()))
            .unwrap();
        assert_eq!(document.undo_label(), Some("Draw"));
        assert!(document.undo());
        assert_eq!(
            document.sheet().canvas.ink_point_count(),
            MAX_TOTAL_INK_POINTS - MAX_STROKE_POINTS
        );
        assert_eq!(drawing.samples()[1], sample(25.0, 35.0, Some(0.8)));
    }

    #[test]
    fn eraser_follows_swept_paths_rotated_ink_and_stroke_width() {
        let mut sheet = Sheet::new("Eraser", "Root");
        let line = || {
            CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::BLACK,
                2.0,
                vec![Vec2::new(1000.0, 0.0), Vec2::new(1100.0, 0.0)],
            ))
        };
        let horizontal = sheet.add_canvas_object(line()).unwrap();
        let rotated = sheet.add_canvas_object(line()).unwrap();
        sheet.rotate_canvas_object(rotated, 90.0).unwrap();
        sheet.set_canvas_objects_locked(&[rotated], true).unwrap();
        let dot = sheet
            .add_canvas_object(CanvasObject::ink(InkStroke::new(
                InkTool::Highlighter,
                Color::BLACK,
                10.0,
                vec![Vec2::new(1200.0, 0.0)],
            )))
            .unwrap();
        let covering_shape = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                bounds(1035.0, -15.0, 30.0, 30.0),
            ))
            .unwrap();
        let scene = scene(&sheet);
        assert_eq!(
            scene.hit_test(Vec2::new(1050.0, 0.0)),
            Some(SceneHit::Object(covering_shape))
        );
        let hits = eraser_hits(
            &scene,
            &[Vec2::new(1020.0, -30.0), Vec2::new(1080.0, 30.0)],
            0.0,
        )
        .unwrap();
        assert_eq!(hits, vec![horizontal, rotated]);
        assert_eq!(
            lasso_hits(&scene, &rectangle(1048.0, -2.0, 4.0, 4.0)).unwrap(),
            vec![
                SceneHit::Object(horizontal),
                SceneHit::Object(rotated),
                SceneHit::Object(covering_shape)
            ]
        );
        let unlocked: Vec<_> = hits
            .into_iter()
            .filter(|&id| !sheet.canvas.is_locked(id))
            .collect();
        assert_eq!(unlocked, vec![horizontal]);
        assert_eq!(
            eraser_hits(
                &scene,
                &[Vec2::new(1030.0, -10.0), Vec2::new(1030.0, 10.0)],
                0.0
            )
            .unwrap(),
            vec![horizontal]
        );
        assert_eq!(
            eraser_hits(&scene, &[Vec2::new(1207.0, 0.0)], 2.0).unwrap(),
            vec![dot]
        );
        assert!(
            eraser_hits(&scene, &[Vec2::new(1207.25, 0.0)], 2.0)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn lasso_uses_scene_geometry_not_only_bounds_and_includes_tangencies() {
        let mut sheet = Sheet::new("Lasso", "Root");
        let diamond = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Diamond,
                bounds(1000.0, 0.0, 100.0, 100.0),
            ))
            .unwrap();
        let ink = sheet
            .add_canvas_object(CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::BLACK,
                10.0,
                vec![Vec2::new(1200.0, 0.0), Vec2::new(1300.0, 0.0)],
            )))
            .unwrap();
        let connector = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(1400.0, 0.0)),
                Endpoint::Point(Vec2::new(1500.0, 0.0)),
            )))
            .unwrap();
        let scene = scene(&sheet);

        assert!(
            lasso_hits(&scene, &rectangle(1001.0, 1.0, 9.0, 9.0))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            lasso_hits(&scene, &rectangle(1040.0, -10.0, 20.0, 20.0)).unwrap(),
            vec![SceneHit::Object(diamond)]
        );
        assert_eq!(
            lasso_hits(&scene, &rectangle(1048.0, 48.0, 4.0, 4.0)).unwrap(),
            vec![SceneHit::Object(diamond)]
        );
        assert_eq!(
            lasso_hits(&scene, &rectangle(990.0, -10.0, 120.0, 120.0)).unwrap(),
            vec![SceneHit::Object(diamond)]
        );
        assert_eq!(
            lasso_hits(&scene, &rectangle(1240.0, 5.0, 20.0, 10.0)).unwrap(),
            vec![SceneHit::Object(ink)]
        );
        assert_eq!(
            lasso_hits(&scene, &rectangle(1440.0, -2.0, 20.0, 4.0)).unwrap(),
            vec![SceneHit::Object(connector)]
        );
    }

    #[test]
    fn lasso_selects_connector_marker_wings() {
        use thinoda_core::{ConnectorMarker, ConnectorMarkerKind, SheetRootMode};

        let mut sheet = Sheet::new("Markers", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let scene = scene(&sheet);
        assert_eq!(
            lasso_hits(&scene, &rectangle(89.0, 3.5, 1.5, 1.5)).unwrap(),
            vec![SceneHit::Object(id)]
        );
    }

    #[test]
    fn lasso_selects_sized_connector_marker_wings() {
        use thinoda_core::{ConnectorMarker, ConnectorMarkerKind, SheetRootMode};

        let mut sheet = Sheet::new("Sized markers", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        connector.end_marker_size = Some(12.5);
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let scene = scene(&sheet);
        assert_eq!(
            lasso_hits(&scene, &rectangle(83.0, 7.0, 1.5, 1.5)).unwrap(),
            vec![SceneHit::Object(id)]
        );
    }

    #[test]
    fn lasso_uses_the_closed_outline_not_the_empty_hollow_marker_interior() {
        use thinoda_core::{ConnectorMarker, ConnectorMarkerKind, SheetRootMode};

        let mut sheet = Sheet::new("Hollow markers", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.width = 0.1;
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Classic,
        });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let scene = scene(&sheet);
        assert!(
            lasso_hits(&scene, &rectangle(94.8, -0.2, 0.4, 0.4))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            lasso_hits(&scene, &rectangle(88.8, 3.1, 0.2, 0.2)).unwrap(),
            vec![SceneHit::Object(id)]
        );
    }

    #[test]
    fn lasso_respects_hollow_diamond_marker_interiors_and_outlines() {
        use thinoda_core::{ConnectorMarker, ConnectorMarkerKind, SheetRootMode};

        for kind in [
            ConnectorMarkerKind::Diamond,
            ConnectorMarkerKind::DiamondThin,
        ] {
            let mut sheet = Sheet::new("Hollow diamond", "");
            sheet.root_mode = SheetRootMode::Hidden;
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.width = 0.1;
            connector.end_marker = Some(ConnectorMarker { kind });
            connector.end_marker_size = Some(12.5);
            connector.end_marker_unfilled = true;
            let id = sheet
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let scene = scene(&sheet);
            let SceneGeometry::Polyline { markers, .. } =
                &scene.item(SceneId::Object(id)).unwrap().geometry
            else {
                panic!("expected connector");
            };
            let points = markers[1].unwrap();
            let interior = (points.points()[0] + points.points()[2]) * 0.5;
            let outline = (points.points()[3] + points.points()[0]) * 0.5;
            assert!(
                lasso_hits(
                    &scene,
                    &rectangle(interior.x - 0.1, interior.y - 0.1, 0.2, 0.2),
                )
                .unwrap()
                .is_empty(),
                "{kind:?}"
            );
            assert_eq!(
                lasso_hits(
                    &scene,
                    &rectangle(outline.x - 0.1, outline.y - 0.1, 0.2, 0.2),
                )
                .unwrap(),
                vec![SceneHit::Object(id)],
                "{kind:?}"
            );
        }
    }

    #[test]
    fn lasso_respects_half_circle_marker_outline_and_open_interior() {
        use thinoda_core::{ConnectorMarker, ConnectorMarkerKind, SheetRootMode};

        let mut sheet = Sheet::new("Half-circle marker", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.width = 0.1;
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::HalfCircle,
        });
        connector.end_marker_size = Some(12.5);
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let scene = scene(&sheet);
        assert!(
            lasso_hits(&scene, &rectangle(99.9, 0.9, 0.2, 0.2))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            lasso_hits(&scene, &rectangle(89.7, 10.1, 0.2, 0.2)).unwrap(),
            vec![SceneHit::Object(id)]
        );
        assert_eq!(
            lasso_hits(&scene, &rectangle(80.0, -20.0, 30.0, 40.0)).unwrap(),
            vec![SceneHit::Object(id)]
        );
    }

    #[test]
    fn lasso_selects_bar_marker_strokes_without_filling_their_bounds() {
        use thinoda_core::{ConnectorMarker, ConnectorMarkerKind, SheetRootMode};

        for kind in [ConnectorMarkerKind::BaseDash, ConnectorMarkerKind::Dash] {
            let mut sheet = Sheet::new("Bar marker", "");
            sheet.root_mode = SheetRootMode::Hidden;
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.width = 0.1;
            connector.end_marker = Some(ConnectorMarker { kind });
            connector.end_marker_size = Some(12.5);
            let id = sheet
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let scene = scene(&sheet);
            let SceneGeometry::Polyline { markers, .. } =
                &scene.item(SceneId::Object(id)).unwrap().geometry
            else {
                panic!("expected connector");
            };
            let points = markers[1].unwrap();
            let on_marker = points.points()[0] * 0.8 + points.points()[1] * 0.2;
            let off_marker = Vec2::new(100.0, 10.0);
            assert_eq!(
                lasso_hits(
                    &scene,
                    &rectangle(on_marker.x - 0.1, on_marker.y - 0.1, 0.2, 0.2),
                )
                .unwrap(),
                vec![SceneHit::Object(id)],
                "{kind:?}"
            );
            assert!(
                lasso_hits(
                    &scene,
                    &rectangle(off_marker.x - 0.1, off_marker.y - 0.1, 0.2, 0.2),
                )
                .unwrap()
                .is_empty(),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn half_circle_lasso_work_is_budgeted() {
        use thinoda_core::{ConnectorMarker, ConnectorMarkerKind, SheetRootMode};

        let mut sheet = Sheet::new("Half-circle marker budget", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::new(-2000.0, 0.0)),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.width = 0.1;
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::HalfCircle,
        });
        connector.end_marker_size = Some(999.0);
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let scene = scene(&sheet);
        let mut polygon = vec![Vec2::new(99.0, 10.0); 4096];
        polygon.extend([
            Vec2::new(99.1, 10.0),
            Vec2::new(99.1, 10.1),
            Vec2::new(99.0, 10.1),
        ]);
        assert_eq!(
            lasso_hits(&scene, &polygon),
            Err(GeometryError::TooComplex {
                limit: MAX_GEOMETRY_CHECKS,
            })
        );
    }

    #[test]
    fn lasso_respects_oval_marker_interiors_outlines_and_enclosure() {
        use thinoda_core::{ConnectorMarker, ConnectorMarkerKind, SheetRootMode};

        for filled in [false, true] {
            let mut sheet = Sheet::new("Oval marker", "");
            sheet.root_mode = SheetRootMode::Hidden;
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.width = 0.1;
            connector.end_marker = Some(ConnectorMarker {
                kind: ConnectorMarkerKind::Oval,
            });
            connector.end_marker_size = Some(12.5);
            connector.end_marker_unfilled = !filled;
            let id = sheet
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            let scene = scene(&sheet);
            let interior = lasso_hits(&scene, &rectangle(99.9, 0.9, 0.2, 0.2)).unwrap();
            assert_eq!(
                interior,
                if filled {
                    vec![SceneHit::Object(id)]
                } else {
                    vec![]
                }
            );
            assert_eq!(
                lasso_hits(&scene, &rectangle(99.9, 6.15, 0.2, 0.2)).unwrap(),
                vec![SceneHit::Object(id)]
            );
        }

        let center = Vec2::new(100.0, 0.0);
        for (lasso, filled, expected) in [
            (rectangle(93.0, -7.0, 14.0, 14.0), false, true),
            (rectangle(99.9, 0.9, 0.2, 0.2), false, false),
            (rectangle(99.9, 0.9, 0.2, 0.2), true, true),
            (rectangle(110.0, 10.0, 1.0, 1.0), true, false),
        ] {
            assert_eq!(
                circle_touches_lasso(
                    center,
                    6.25,
                    filled,
                    0.05,
                    &lasso,
                    &mut GeometryBudget::new()
                )
                .unwrap(),
                expected
            );
        }
        assert!(
            !circle_touches_lasso(
                center,
                0.0,
                true,
                0.05,
                &rectangle(99.0, -1.0, 2.0, 2.0),
                &mut GeometryBudget::new(),
            )
            .unwrap()
        );
    }

    #[test]
    fn lasso_respects_the_classic_arrow_notch() {
        use thinoda_core::{ConnectorMarker, ConnectorMarkerKind, SheetRootMode};

        let mut sheet = Sheet::new("Markers", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.width = 0.1;
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Classic,
        });
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let scene = scene(&sheet);
        assert_eq!(
            lasso_hits(&scene, &rectangle(93.7, 2.9, 0.2, 0.2)).unwrap(),
            vec![SceneHit::Object(id)]
        );
        assert!(
            lasso_hits(&scene, &rectangle(94.1, 0.8, 0.2, 0.2))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn lasso_selects_rotated_ink_and_topics_but_not_zone_frames() {
        let mut sheet = Sheet::new("Lasso", "Root");
        let ink = sheet
            .add_canvas_object(CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::BLACK,
                2.0,
                vec![Vec2::new(1000.0, 0.0), Vec2::new(1100.0, 0.0)],
            )))
            .unwrap();
        sheet.rotate_canvas_object(ink, 90.0).unwrap();
        let mut scene = scene(&sheet);
        assert_eq!(
            lasso_hits(&scene, &rectangle(1048.0, 38.0, 4.0, 4.0)).unwrap(),
            vec![SceneHit::Object(ink)]
        );
        assert!(
            lasso_hits(&scene, &rectangle(1005.0, -2.0, 4.0, 4.0))
                .unwrap()
                .is_empty()
        );

        let root_item = scene.item(SceneId::Topic(sheet.root)).unwrap().clone();
        scene.items.insert(
            0,
            SceneItem {
                id: SceneId::Zone(thinoda_core::ZoneId(999)),
                band: thinoda_core::SceneBand::Frames,
                bounds: root_item.bounds,
                clip: None,
                geometry: root_item.geometry,
            },
        );
        let center = root_item.bounds.center();
        assert_eq!(
            lasso_hits(&scene, &rectangle(center.x - 1.0, center.y - 1.0, 2.0, 2.0)).unwrap(),
            vec![SceneHit::Topic(sheet.root)]
        );
    }

    #[test]
    fn eraser_and_lasso_respect_clipped_scene_items() {
        let mut sheet = Sheet::new("Clipped", "Root");
        let floating = sheet.add_floating("Member", Vec2::new(1050.0, 50.0));
        let frame = bounds(1000.0, 0.0, 100.0, 100.0);
        let zone = sheet.create_zone("Frame", vec![floating], frame).unwrap();
        sheet
            .zones
            .iter_mut()
            .find(|item| item.id == zone)
            .unwrap()
            .clip = true;
        let ink = sheet
            .add_canvas_object(CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::BLACK,
                2.0,
                vec![Vec2::new(900.0, 50.0), Vec2::new(1200.0, 50.0)],
            )))
            .unwrap();
        let shape = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                bounds(900.0, 10.0, 300.0, 80.0),
            ))
            .unwrap();
        let scene = scene(&sheet);
        assert_eq!(scene.item(SceneId::Object(ink)).unwrap().clip, Some(frame));
        assert_eq!(
            scene.item(SceneId::Object(shape)).unwrap().clip,
            Some(frame)
        );
        assert!(
            eraser_hits(&scene, &[Vec2::new(950.0, 50.0)], 0.0)
                .unwrap()
                .is_empty()
        );
        assert!(
            eraser_hits(
                &scene,
                &[Vec2::new(950.0, 50.0), Vec2::new(990.0, 50.0)],
                20.0
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            eraser_hits(
                &scene,
                &[Vec2::new(950.0, 50.0), Vec2::new(1150.0, 50.0)],
                0.0
            )
            .unwrap(),
            vec![ink]
        );

        let outside = lasso_hits(&scene, &rectangle(920.0, 45.0, 20.0, 10.0)).unwrap();
        assert!(!outside.contains(&SceneHit::Object(ink)));
        assert!(!outside.contains(&SceneHit::Object(shape)));
        let straddling = lasso_hits(&scene, &rectangle(990.0, 45.0, 20.0, 10.0)).unwrap();
        assert!(straddling.contains(&SceneHit::Object(ink)));
        assert!(straddling.contains(&SceneHit::Object(shape)));
        let enclosing = lasso_hits(&scene, &rectangle(990.0, -10.0, 120.0, 120.0)).unwrap();
        assert!(enclosing.contains(&SceneHit::Object(ink)));
        assert!(enclosing.contains(&SceneHit::Object(shape)));

        let concave = [
            Vec2::new(900.0, 40.0),
            Vec2::new(950.0, 40.0),
            Vec2::new(950.0, 80.0),
            Vec2::new(1010.0, 100.0),
            Vec2::new(900.0, 100.0),
        ];
        let outside_geometry = lasso_hits(&scene, &concave).unwrap();
        assert!(!outside_geometry.contains(&SceneHit::Object(ink)));
        assert!(!outside_geometry.contains(&SceneHit::Object(shape)));
    }

    #[test]
    fn geometry_rejects_bad_inputs_and_reports_complexity_without_partial_results() {
        let scene = Scene::default();
        assert_eq!(eraser_hits(&scene, &[], 1.0), Err(GeometryError::EmptyPath));
        assert!(matches!(
            eraser_hits(&scene, &[Vec2::ZERO], -1.0),
            Err(GeometryError::InvalidRadius(_))
        ));
        assert!(matches!(
            eraser_hits(&scene, &[Vec2::new(f32::NAN, 0.0)], 1.0),
            Err(GeometryError::InvalidPoint { index: 0, .. })
        ));
        assert_eq!(
            lasso_hits(&scene, &[Vec2::ZERO, Vec2::new(1.0, 1.0)]),
            Err(GeometryError::DegenerateLasso)
        );
        assert_eq!(
            lasso_hits(
                &scene,
                &[Vec2::ZERO, Vec2::new(1.0, 1.0), Vec2::new(2.0, 2.0)]
            ),
            Err(GeometryError::DegenerateLasso)
        );
        assert!(
            lasso_hits(
                &scene,
                &[
                    Vec2::ZERO,
                    Vec2::new(10.0, 10.0),
                    Vec2::new(0.0, 10.0),
                    Vec2::new(10.0, 0.0),
                ]
            )
            .is_ok()
        );
        let too_many = vec![Vec2::ZERO; MAX_INPUT_SAMPLES + 1];
        assert_eq!(
            eraser_hits(&scene, &too_many, 1.0),
            Err(GeometryError::TooManyPoints {
                count: MAX_INPUT_SAMPLES + 1,
                limit: MAX_INPUT_SAMPLES,
            })
        );

        let mut budget = GeometryBudget { remaining: 1 };
        assert_eq!(
            paths_touch(
                &[Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0)],
                false,
                |point| point,
                SegmentPath {
                    points: &[
                        Vec2::new(0.0, 1.0),
                        Vec2::new(1.0, 1.0),
                        Vec2::new(2.0, 1.0),
                    ],
                    closed: false,
                    clip: None,
                },
                0.0,
                &mut budget
            ),
            Err(GeometryError::TooComplex {
                limit: MAX_GEOMETRY_CHECKS,
            })
        );
    }

    #[test]
    fn geometry_detects_short_segments_crossings_and_collinear_boundaries() {
        assert!(segments_near(
            Vec2::ZERO,
            Vec2::new(0.0001, 0.0),
            Vec2::new(0.00005, 0.00001),
            Vec2::new(0.00005, 0.00001),
            0.00002
        ));
        assert!(segments_near(
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.0, 2.0),
            Vec2::new(2.0, 0.0),
            0.0
        ));
        assert!(segments_near(
            Vec2::ZERO,
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(3.0, 0.0),
            0.0
        ));
        assert!(!segments_near(
            Vec2::ZERO,
            Vec2::new(2.0, 0.0),
            Vec2::new(2.1, 0.0),
            Vec2::new(3.0, 0.0),
            0.0
        ));
    }

    #[test]
    fn lasso_complexity_error_does_not_return_earlier_hits() {
        let mut polygon = vec![Vec2::ZERO; MAX_INPUT_SAMPLES - 2];
        polygon.extend([Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)]);
        let item = |id: u64, point: Vec2, count: usize| SceneItem {
            id: SceneId::Object(CanvasObjectId(id)),
            band: thinoda_core::SceneBand::AboveMap,
            bounds: Rect::from_min_size(point, Vec2::ZERO).expand(0.05),
            clip: None,
            geometry: SceneGeometry::Ink {
                points: vec![point; count].into(),
                width: MIN_STROKE_WIDTH,
                center: point,
                rotation: 0.0,
            },
        };
        let scene = Scene {
            grid: None,
            items: vec![
                item(1, Vec2::new(1.0, 1.0), 1),
                item(2, Vec2::new(9.0, 9.0), 100),
            ],
        };
        assert_eq!(
            lasso_hits(&scene, &polygon),
            Err(GeometryError::TooComplex {
                limit: MAX_GEOMETRY_CHECKS,
            })
        );
    }

    #[test]
    fn invalid_scene_geometry_is_an_error_not_an_ignored_hit() {
        let scene = Scene {
            grid: None,
            items: vec![SceneItem {
                id: SceneId::Object(CanvasObjectId(1)),
                band: thinoda_core::SceneBand::AboveMap,
                bounds: bounds(-1.0, -1.0, 2.0, 2.0),
                clip: None,
                geometry: SceneGeometry::Ink {
                    points: vec![Vec2::new(f32::NAN, 0.0)].into(),
                    width: 1.0,
                    center: Vec2::ZERO,
                    rotation: 0.0,
                },
            }],
        };
        assert_eq!(
            eraser_hits(&scene, &[Vec2::ZERO], 1.0),
            Err(GeometryError::InvalidSceneGeometry(SceneId::Object(
                CanvasObjectId(1)
            )))
        );
        assert_eq!(
            lasso_hits(&scene, &rectangle(-1.0, -1.0, 2.0, 2.0)),
            Err(GeometryError::InvalidSceneGeometry(SceneId::Object(
                CanvasObjectId(1)
            )))
        );
    }
}
