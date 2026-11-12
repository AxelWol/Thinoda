//! Board shapes drawn by a data-defined [`crate::stencil`] definition.
//!
//! A stencil object is an ordinary [`CanvasObjectKind::Shape`] that also carries
//! a [`StencilRef`]. The `shape` stays the legacy fallback: unknown packages,
//! definitions or capabilities render, hit-test and export as that shape, so a
//! document never loses an object because a library is missing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::board::{CanvasObject, CanvasObjectKind, ShapeGeometry};
use crate::geometry::{Color, Rect, Vec2};
use crate::stencil::{
    Instance, PaintRef, StencilDefinition, StencilPackage, StencilRender, TextAlign, TextVAlign,
    builtin_packages,
};

/// Longest package or definition identifier stored in a document.
pub const MAX_STENCIL_ID_BYTES: usize = 128;
/// Most parameter overrides stored on one object.
pub const MAX_STENCIL_PARAMETERS: usize = 64;

/// Which definition draws an object, with its parameter overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StencilRef {
    pub package: String,
    pub definition: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, f64>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_STENCIL_ID_BYTES
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'/' | b':'))
}

impl StencilRef {
    pub fn new(package: impl Into<String>, definition: impl Into<String>) -> Self {
        Self {
            package: package.into(),
            definition: definition.into(),
            parameters: BTreeMap::new(),
        }
    }

    /// Structural checks applied at every load, paste and recovery boundary.
    /// Unknown packages are valid: they fall back to the legacy shape.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.package) || !valid_id(&self.definition) {
            return Err("invalid stencil reference".into());
        }
        if self.parameters.len() > MAX_STENCIL_PARAMETERS {
            return Err(format!(
                "stencil reference exceeds {MAX_STENCIL_PARAMETERS} parameters"
            ));
        }
        if self
            .parameters
            .iter()
            .any(|(key, value)| !valid_id(key) || !value.is_finite())
        {
            return Err("invalid stencil parameter".into());
        }
        Ok(())
    }

    /// The installed package and definition, when it can be evaluated here.
    pub fn resolve(&self) -> Option<(&'static StencilPackage, &'static StencilDefinition)> {
        let package = find_package(&self.package)?;
        let definition = package.definition(&self.definition)?;
        definition
            .missing_capabilities()
            .is_empty()
            .then_some((package, definition))
    }
}

/// An installed package by id.
pub fn find_package(id: &str) -> Option<&'static StencilPackage> {
    builtin_packages().iter().find(|package| package.id == id)
}

/// Evaluates the object's stencil, or `None` to use its legacy shape.
pub fn render(object: &CanvasObject) -> Option<StencilRender> {
    let stencil = object.stencil.as_ref()?;
    if !matches!(object.kind, CanvasObjectKind::Shape { .. }) {
        return None;
    }
    let (package, definition) = stencil.resolve()?;
    let parameters = definition.resolve_parameters(&stencil.parameters);
    package
        .evaluate(
            &definition.id,
            &Instance {
                width: f64::from(object.rect.size.x),
                height: f64::from(object.rect.size.y),
                stroke_width: f64::from(object.style.effective_shape_stroke_width()),
                parameters: &parameters,
            },
        )
        .ok()
}

fn closed(mut points: Vec<Vec2>, close: bool) -> Vec<Vec2> {
    if close && points.len() > 2 && points.first() != points.last() {
        points.push(points[0]);
    }
    points
}

/// Sheet-space outline (hit region and connector perimeter) and every painted
/// subpath as an open polyline, so bounds and stroke hits cover the drawing.
pub fn geometry(object: &CanvasObject, render: &StencilRender) -> ShapeGeometry {
    let map = |points: &[Vec2]| -> Vec<Vec2> {
        points
            .iter()
            .map(|p| render.to_sheet(*p, object.rect, object.rotation))
            .collect()
    };
    let mut outlines = render.outline.iter().filter(|s| s.points.len() > 2);
    let outline = outlines.next().map_or_else(
        || crate::board::rotated_corners(object.rect, object.rotation).to_vec(),
        |subpath| map(&subpath.points),
    );
    let details = outlines
        .map(|subpath| closed(map(&subpath.points), true))
        .chain(
            render
                .paths
                .iter()
                .flat_map(|path| &path.subpaths)
                .filter(|s| s.points.len() > 1)
                .map(|s| closed(map(&s.points), s.closed)),
        )
        .collect();
    ShapeGeometry { outline, details }
}

/// Unrotated sheet rect of the object's label.
pub fn label_rect(object: &CanvasObject, render: &StencilRender) -> Rect {
    let min = render.to_sheet(render.label.min, object.rect, 0.0);
    let max = render.to_sheet(render.label.max(), object.rect, 0.0);
    Rect::from_min_size(min, max - min)
}

/// Colors a renderer resolves for the object's named style slots. `None`
/// leaves the slot unpainted, like Thinoda's transparent default shapes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StencilSlots {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub font: Color,
    pub opacity: f32,
}

/// Slots for an object's style: Thinoda shapes are transparent by default and
/// only draw the theme's default outline while they have no fill.
pub fn slots(object: &CanvasObject, default_stroke: Color, font: Color) -> StencilSlots {
    let style = &object.style;
    StencilSlots {
        fill: style.fill,
        stroke: style
            .stroke
            .or_else(|| style.fill.is_none().then_some(default_stroke)),
        font,
        opacity: style.opacity,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StencilStroke {
    pub color: Color,
    pub width: f32,
    /// Dash and gap lengths in sheet units; `None` is solid.
    pub dash: Option<Vec<f32>>,
}

/// One painted path in sheet coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct StencilPrimitive {
    /// Points with their closed flag; closed subpaths are filled together.
    pub subpaths: Vec<(Vec<Vec2>, bool)>,
    pub fill: Option<Color>,
    pub stroke: Option<StencilStroke>,
}

/// A text run in sheet coordinates; `rotation` includes the object's.
#[derive(Debug, Clone, PartialEq)]
pub struct StencilText {
    pub text: String,
    pub at: Vec2,
    pub align: TextAlign,
    pub valign: TextVAlign,
    pub rotation: f32,
    pub size: f32,
    pub color: Color,
}

fn faded(color: Color, alpha: f32) -> Color {
    Color {
        a: (f32::from(color.a) * alpha.clamp(0.0, 1.0)).round() as u8,
        ..color
    }
}

fn resolve(paint: &PaintRef, slots: &StencilSlots) -> Option<Color> {
    match paint {
        PaintRef::None => None,
        PaintRef::Rgb(color) => Some(*color),
        PaintRef::Style(slot) => match slot.as_str() {
            "fill" | "background" => slots.fill,
            "font" => Some(slots.font),
            _ => slots.stroke,
        },
    }
}

/// Renderer-neutral paint list shared by the canvas and the exporters.
pub fn primitives(
    object: &CanvasObject,
    render: &StencilRender,
    slots: &StencilSlots,
) -> (Vec<StencilPrimitive>, Vec<StencilText>) {
    let to_sheet = |p: Vec2| render.to_sheet(p, object.rect, object.rotation);
    let paths = render
        .paths
        .iter()
        .filter_map(|path| {
            let fill = path
                .fill
                .as_ref()
                .and_then(|f| resolve(&f.color, slots).map(|c| faded(c, f.alpha * slots.opacity)))
                .filter(|c| c.a > 0);
            let stroke = path.stroke.as_ref().and_then(|s| {
                let color = faded(resolve(&s.color, slots)?, s.alpha * slots.opacity);
                (color.a > 0 && s.width > 0.0).then(|| StencilStroke {
                    color,
                    width: s.width,
                    dash: (s.dashed == Some(true)).then(|| {
                        s.dash_pattern
                            .iter()
                            .map(|d| (d * s.width).max(0.5))
                            .collect()
                    }),
                })
            });
            (fill.is_some() || stroke.is_some()).then(|| StencilPrimitive {
                subpaths: path
                    .subpaths
                    .iter()
                    .filter(|s| s.points.len() > 1)
                    .map(|s| (s.points.iter().copied().map(to_sheet).collect(), s.closed))
                    .collect(),
                fill,
                stroke,
            })
        })
        .collect();
    let texts = render
        .texts
        .iter()
        .filter_map(|text| {
            let color = faded(resolve(&text.color, slots)?, text.alpha * slots.opacity);
            Some(StencilText {
                text: text.text.clone(),
                at: to_sheet(text.at),
                align: text.align,
                valign: text.valign,
                rotation: text.rotation + object.rotation,
                size: text.size,
                color,
            })
        })
        .collect();
    (paths, texts)
}

/// Sheet-space positions of the definition's named connection points.
pub fn ports(object: &CanvasObject, render: &StencilRender) -> Vec<Vec2> {
    render
        .ports
        .iter()
        .map(|port| render.to_sheet(port.at, object.rect, object.rotation))
        .collect()
}

/// A new object drawn by `definition`, at its default size centred on `center`.
pub fn new_object(
    package: &StencilPackage,
    definition: &StencilDefinition,
    center: Vec2,
) -> CanvasObject {
    let [w, h] = definition.default_size();
    let size = Vec2::new(w as f32, h as f32);
    let mut object = CanvasObject::shape(
        crate::board::ShapeKind::Rectangle,
        Rect::from_center_size(center, size),
    );
    object.stencil = Some(StencilRef::new(&package.id, &definition.id));
    object
}

#[cfg(test)]
mod tests;
