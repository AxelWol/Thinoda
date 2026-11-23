//! Board shapes drawn by a data-defined [`crate::stencil`] definition.
//!
//! A stencil object is an ordinary [`CanvasObjectKind::Shape`] that also carries
//! a [`StencilRef`]. The `shape` stays the legacy fallback: unknown packages,
//! definitions or capabilities render, hit-test and export as that shape, so a
//! document never loses an object because a library is missing.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, OnceLock},
};

use serde::{Deserialize, Serialize};

use crate::board::{CanvasObject, CanvasObjectKind, ShapeGeometry};
use crate::geometry::{Color, Rect, Vec2};
use crate::stencil::{
    Instance, LineCap, LineJoin, PaintRef, StencilDefinition, StencilPackage, StencilRender,
    StrokeKeyword, StrokeWidthPolicy, TextAlign, TextVAlign, builtin_packages,
};

/// Longest package or definition identifier stored in a document.
pub const MAX_STENCIL_ID_BYTES: usize = 128;
/// Most parameter overrides stored on one object.
pub const MAX_STENCIL_PARAMETERS: usize = 64;

/// Which definition draws an object, with its parameter overrides.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StencilRef {
    pub package: String,
    pub definition: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_resource: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, f64>,
    #[serde(skip)]
    resolved_package: Option<Arc<StencilPackage>>,
    #[serde(skip)]
    resolved_resource: Option<String>,
}

impl std::fmt::Debug for StencilRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StencilRef")
            .field("package", &self.package)
            .field("definition", &self.definition)
            .field("definition_hash", &self.definition_hash)
            .field("package_resource", &self.package_resource)
            .field("parameters", &self.parameters)
            .finish()
    }
}

impl PartialEq for StencilRef {
    fn eq(&self, other: &Self) -> bool {
        self.package == other.package
            && self.definition == other.definition
            && self.definition_hash == other.definition_hash
            && self.package_resource == other.package_resource
            && self.parameters == other.parameters
    }
}

/// The shared instance of an installed package. References holding this exact
/// instance pin a built-in revision and need no embedded closure.
pub fn installed_package(id: &str) -> Option<Arc<StencilPackage>> {
    static PACKAGES: OnceLock<Vec<Arc<StencilPackage>>> = OnceLock::new();
    PACKAGES
        .get_or_init(|| builtin_packages().iter().cloned().map(Arc::new).collect())
        .iter()
        .find(|package| package.id == id)
        .cloned()
}

fn builtin_definition_hashes() -> &'static HashMap<(String, String), String> {
    static HASHES: OnceLock<HashMap<(String, String), String>> = OnceLock::new();
    HASHES.get_or_init(|| {
        builtin_packages()
            .iter()
            .flat_map(|package| {
                package.definitions.iter().map(|definition| {
                    (
                        (package.id.clone(), definition.id.clone()),
                        StencilPackage::definition_hash(definition),
                    )
                })
            })
            .collect()
    })
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
        let package = package.into();
        let definition = definition.into();
        let definition_hash = builtin_definition_hashes()
            .get(&(package.clone(), definition.clone()))
            .cloned();
        let resolved_package = installed_package(&package);
        Self {
            package,
            definition,
            definition_hash,
            package_resource: None,
            parameters: BTreeMap::new(),
            resolved_package,
            resolved_resource: None,
        }
    }

    /// A reference drawn by `package` even when it is not installed, e.g. a
    /// stencil converted from an imported file. Inserting the object into a
    /// sheet embeds the definition closure, so it keeps rendering offline.
    pub fn with_package(package: Arc<StencilPackage>, definition: impl Into<String>) -> Self {
        let definition = definition.into();
        let definition_hash = package
            .definition(&definition)
            .map(StencilPackage::definition_hash);
        Self {
            package: package.id.clone(),
            definition,
            definition_hash,
            package_resource: None,
            parameters: BTreeMap::new(),
            resolved_package: Some(package),
            resolved_resource: None,
        }
    }

    /// Whether this pins a built-in definition that this build installs
    /// unchanged, so the sheet need not embed a copy of its closure.
    pub fn is_installed_builtin(&self) -> bool {
        self.package_resource.is_none()
            && self.definition_hash.as_ref().is_some_and(|hash| {
                builtin_definition_hashes().get(&(self.package.clone(), self.definition.clone()))
                    == Some(hash)
            })
            && self.resolved_package.as_ref().is_none_or(|package| {
                installed_package(&package.id)
                    .is_some_and(|installed| Arc::ptr_eq(package, &installed))
            })
    }

    /// Structural checks applied at every load, paste and recovery boundary.
    /// Unknown packages are valid: they fall back to the legacy shape.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.package) || !valid_id(&self.definition) {
            return Err("invalid stencil reference".into());
        }
        if self.definition_hash.as_ref().is_some_and(|hash| {
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err("invalid stencil definition hash".into());
        }
        if self.package_resource.as_ref().is_some_and(|reference| {
            reference
                .strip_prefix(crate::resources::RESOURCE_PREFIX)
                .is_none_or(|id| id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        }) {
            return Err("invalid stencil package resource reference".into());
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
    pub fn resolve(&self) -> Option<(&StencilPackage, &StencilDefinition)> {
        let package =
            if self.package_resource.is_some() && self.resolved_resource == self.package_resource {
                self.resolved_package.as_deref()?
            } else if self.package_resource.is_none()
                && let Some(package) = &self.resolved_package
            {
                package.as_ref()
            } else if self.package_resource.is_none() {
                find_package(&self.package)?
            } else {
                return None;
            };
        if package.id != self.package {
            return None;
        }
        let definition = package.definition(&self.definition)?;
        if self
            .definition_hash
            .as_ref()
            .is_some_and(|expected| StencilPackage::definition_hash(definition) != *expected)
        {
            return None;
        }
        definition
            .missing_capabilities()
            .is_empty()
            .then_some((package, definition))
    }

    /// Draws from `package`, already embedded in the sheet as `reference`.
    /// Sheet insertion verifies the resource, identity and definition hash.
    pub fn embed_package(&mut self, package: Arc<StencilPackage>, reference: String) {
        self.package_resource = Some(reference.clone());
        self.set_embedded_package(package, reference);
    }

    pub(crate) fn set_resolved_package(&mut self, package: Arc<StencilPackage>) {
        self.resolved_package = Some(package);
        self.resolved_resource = None;
    }

    pub(crate) fn set_embedded_package(&mut self, package: Arc<StencilPackage>, reference: String) {
        self.resolved_package = Some(package);
        self.resolved_resource = Some(reference);
    }

    pub(crate) fn resolved_package(&self) -> Option<&Arc<StencilPackage>> {
        self.resolved_package.as_ref()
    }

    pub(crate) fn hydrate_package(
        &mut self,
        resources: &crate::resources::ResourceStore,
    ) -> Result<(), String> {
        let Some(reference) = &self.package_resource else {
            return Ok(());
        };
        let resource = resources.get(reference)?;
        let id = reference
            .strip_prefix(crate::resources::RESOURCE_PREFIX)
            .ok_or("not an embedded resource reference")?;
        if resource.id() != id {
            return Err("stencil package resource digest mismatch".into());
        }
        if resource.media_type != "application/json" {
            return Err("stencil package resource must be JSON".into());
        }
        if self.resolved_resource.as_ref() == Some(reference)
            && let Some(package) = &self.resolved_package
        {
            if package.id != self.package {
                return Err("stencil package resource has a different package id".into());
            }
            let definition = package.definition(&self.definition).ok_or_else(|| {
                format!(
                    "stencil package is missing definition '{}'",
                    self.definition
                )
            })?;
            if self
                .definition_hash
                .as_ref()
                .is_some_and(|hash| StencilPackage::definition_hash(definition) != *hash)
            {
                return Err("stencil package definition hash mismatch".into());
            }
            return Ok(());
        }
        let text = std::str::from_utf8(resource.bytes())
            .map_err(|error| format!("stencil package resource is not UTF-8: {error}"))?;
        let package = StencilPackage::from_json(text).map_err(|error| error.to_string())?;
        if package.id != self.package {
            return Err("stencil package resource has a different package id".into());
        }
        let definition = package.definition(&self.definition).ok_or_else(|| {
            format!(
                "stencil package is missing definition '{}'",
                self.definition
            )
        })?;
        if self
            .definition_hash
            .as_ref()
            .is_some_and(|hash| StencilPackage::definition_hash(definition) != *hash)
        {
            return Err("stencil package definition hash mismatch".into());
        }
        self.resolved_package = Some(Arc::new(package));
        self.resolved_resource = Some(reference.clone());
        Ok(())
    }
}

/// An installed package by id.
pub fn find_package(id: &str) -> Option<&'static StencilPackage> {
    builtin_packages().iter().find(|package| package.id == id)
}

fn instance_stroke_width(object: &CanvasObject, definition: &StencilDefinition) -> f64 {
    f64::from(
        if definition.frame.stroke_width == StrokeWidthPolicy::Keyword(StrokeKeyword::InheritObject)
        {
            object.style.stroke_width
        } else {
            object.style.effective_shape_stroke_width()
        },
    )
}

/// Evaluates the object's stencil, or `None` to use its legacy shape.
pub fn render(object: &CanvasObject) -> Option<StencilRender> {
    render_with_header_policy(object, None)
}

/// Evaluates a stencil with its data-declared container header binding, when
/// the definition exposes one. Unsupported stencils retain their own rendering.
pub fn render_with_header_policy(
    object: &CanvasObject,
    header: Option<crate::scene_layout::HeaderPolicy>,
) -> Option<StencilRender> {
    let stencil = object.stencil.as_ref()?;
    if !matches!(object.kind, CanvasObjectKind::Shape { .. }) {
        return None;
    }
    let (package, definition) = stencil.resolve()?;
    let mut parameters = definition.resolve_parameters(&stencil.parameters);
    let header_parameters = header.and_then(|header| {
        let (extent_name, extent, side_name, side) =
            container_header_parameters(definition, header)?;
        let extent_parameter = definition.parameters.get(&extent_name)?;
        let side_parameter = definition.parameters.get(&side_name)?;
        (extent_parameter.kind == crate::stencil::ParameterKind::Number
            && side_parameter.kind == crate::stencil::ParameterKind::Choice)
            .then_some((
                extent_name,
                extent_parameter.clamp(extent),
                side_name,
                side_parameter.clamp(side),
            ))
    });
    if let Some((extent_name, extent, side_name, side)) = header_parameters {
        parameters.insert(extent_name, extent);
        parameters.insert(side_name, side);
    }
    package
        .evaluate(
            &definition.id,
            &Instance {
                width: f64::from(object.rect.size.x),
                height: f64::from(object.rect.size.y),
                stroke_width: instance_stroke_width(object, definition),
                parameters: &parameters,
            },
        )
        .ok()
}

fn container_header_parameters(
    definition: &StencilDefinition,
    header: crate::scene_layout::HeaderPolicy,
) -> Option<(String, f64, String, f64)> {
    let binding = definition
        .extensions
        .get("thinoda.containerHeader")?
        .as_object()?;
    let extent_name = binding.get("extentParameter")?.as_str()?.to_owned();
    let side_name = binding.get("sideParameter")?.as_str()?.to_owned();
    let side = match header.side {
        crate::scene_layout::HeaderSide::Top => "top",
        crate::scene_layout::HeaderSide::Right => "right",
        crate::scene_layout::HeaderSide::Bottom => "bottom",
        crate::scene_layout::HeaderSide::Left => "left",
    };
    let side_value = binding
        .get("sideValues")?
        .as_object()?
        .get(side)?
        .as_f64()?;
    Some((extent_name, f64::from(header.extent), side_name, side_value))
}

/// Whether this stencil declares a complete, usable container header binding.
pub fn supports_container_header(object: &CanvasObject) -> bool {
    container_header_policy(object).is_some()
}

/// Resolves a stencil's current header parameters into the shared container policy.
pub fn container_header_policy(object: &CanvasObject) -> Option<crate::scene_layout::HeaderPolicy> {
    let stencil = object.stencil.as_ref()?;
    let (_, definition) = stencil.resolve()?;
    let (extent_name, _, side_name, _) = container_header_parameters(
        definition,
        crate::scene_layout::HeaderPolicy {
            side: crate::scene_layout::HeaderSide::Top,
            extent: 0.0,
        },
    )?;
    let extent_parameter = definition.parameters.get(&extent_name)?;
    let side_parameter = definition.parameters.get(&side_name)?;
    if extent_parameter.kind != crate::stencil::ParameterKind::Number
        || side_parameter.kind != crate::stencil::ParameterKind::Choice
        || side_parameter.options.len() != 4
    {
        return None;
    }

    let parameters = definition.resolve_parameters(&stencil.parameters);
    let extent = extent_parameter.clamp(*parameters.get(&extent_name)?);
    let current_side = side_parameter.clamp(*parameters.get(&side_name)?);
    let mut side_values = Vec::with_capacity(4);
    let mut selected_side = None;
    for side in [
        crate::scene_layout::HeaderSide::Top,
        crate::scene_layout::HeaderSide::Right,
        crate::scene_layout::HeaderSide::Bottom,
        crate::scene_layout::HeaderSide::Left,
    ] {
        let (_, _, _, value) = container_header_parameters(
            definition,
            crate::scene_layout::HeaderPolicy { side, extent: 0.0 },
        )?;
        let value = side_parameter.clamp(value);
        if side_values.contains(&value) {
            return None;
        }
        side_values.push(value);
        if value == current_side {
            selected_side = Some(side);
        }
    }
    let side = selected_side?;
    let extent = extent as f32;
    extent
        .is_finite()
        .then_some(crate::scene_layout::HeaderPolicy { side, extent })
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
            .map(|p| local_to_sheet(object, render, *p))
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
    crate::board::flipped_frame_rect(object, Rect::from_min_size(min, max - min))
}

/// Maps an instance-local point to the sheet: scaled into the frame, mirrored
/// by the style's flips, then rotated.
pub fn local_to_sheet(object: &CanvasObject, render: &StencilRender, point: Vec2) -> Vec2 {
    if !object.style.is_flipped() {
        return render.to_sheet(point, object.rect, object.rotation);
    }
    crate::shape_style::place(
        &object.style,
        render.to_sheet(point, object.rect, 0.0),
        object.rect,
        object.rotation,
    )
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
    pub join: LineJoin,
    pub cap: LineCap,
    pub miter_limit: f32,
}

/// One painted path in sheet coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct StencilPrimitive {
    /// Points with their closed flag; closed subpaths are filled together.
    pub subpaths: Vec<(Vec<Vec2>, bool)>,
    pub fill: Option<Color>,
    /// The fill comes from the object's fill slot, so gradients and patterns apply.
    pub styled_fill: bool,
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
    let to_sheet = |p: Vec2| local_to_sheet(object, render, p);
    let paths = render
        .paths
        .iter()
        .filter_map(|path| {
            let fill = path
                .fill
                .as_ref()
                .and_then(|f| resolve(&f.color, slots).map(|c| faded(c, f.alpha * slots.opacity)))
                .filter(|c| c.a > 0);
            let styled_fill = fill.is_some()
                && path.fill.as_ref().is_some_and(|f| {
                    matches!(&f.color, PaintRef::Style(slot) if slot == "fill" || slot == "background")
                });
            let stroke = path.stroke.as_ref().and_then(|s| {
                let color = faded(resolve(&s.color, slots)?, s.alpha * slots.opacity);
                (color.a > 0 && s.width > 0.0).then(|| StencilStroke {
                    color,
                    width: s.width,
                    dash: if s.dashed == Some(true) {
                        Some(
                            s.dash_pattern
                                .iter()
                                .map(|d| (d * s.width).max(0.5))
                                .collect(),
                        )
                    } else if s.dashed.is_none() {
                        object
                            .style
                            .dash
                            .map(|dash| dash.lengths(s.width))
                    } else {
                        None
                    },
                    join: s.join,
                    cap: s.cap,
                    miter_limit: s.miter_limit,
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
                styled_fill,
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
        .map(|port| local_to_sheet(object, render, port.at))
        .collect()
}

/// Resolves an instance-local handle drag into a clamped parameter update.
pub fn drag_handle(
    object: &CanvasObject,
    handle_index: usize,
    pointer: Vec2,
) -> Result<(String, f64), String> {
    let stencil = object
        .stencil
        .as_ref()
        .ok_or_else(|| "canvas object has no stencil".to_owned())?;
    let (package, definition) = stencil
        .resolve()
        .ok_or_else(|| "stencil definition is not available".to_owned())?;
    let parameters = definition.resolve_parameters(&stencil.parameters);
    let instance = Instance {
        width: f64::from(object.rect.size.x),
        height: f64::from(object.rect.size.y),
        stroke_width: instance_stroke_width(object, definition),
        parameters: &parameters,
    };
    let local = crate::shape_style::unplace(&object.style, pointer, object.rect, object.rotation)
        - object.rect.min;
    package
        .drag_handle(&definition.id, handle_index, &instance, local)
        .map_err(|error| error.to_string())
}

/// Returns an instance copy with a dragged handle's parameter updated.
pub fn with_handle_drag(
    object: &CanvasObject,
    handle_index: usize,
    pointer: Vec2,
) -> Result<CanvasObject, String> {
    let (parameter_name, value) = drag_handle(object, handle_index, pointer)?;
    let mut updated = object.clone();
    let stencil = updated
        .stencil
        .as_mut()
        .ok_or_else(|| "canvas object has no stencil".to_owned())?;
    let default = stencil
        .resolve()
        .and_then(|(_, definition)| definition.parameters.get(&parameter_name))
        .map(|parameter| parameter.default)
        .ok_or_else(|| "stencil handle parameter not found".to_owned())?;
    if value == default {
        stencil.parameters.remove(&parameter_name);
    } else {
        stencil.parameters.insert(parameter_name, value);
    }
    Ok(updated)
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
    if let Some(stencil) = &mut object.stencil {
        stencil.definition_hash = Some(StencilPackage::definition_hash(definition));
        let installed = installed_package(&package.id).filter(|installed| **installed == *package);
        stencil.set_resolved_package(installed.unwrap_or_else(|| Arc::new(package.clone())));
    }
    object
}

#[cfg(test)]
mod tests;
