//! Converts draw.io stencil XML (`<shapes>`/`<shape>`, or the compressed
//! payload of an inline `shape=stencil(...)` style) into a declarative
//! [`StencilPackage`]. The conversion is purely structural: every draw.io
//! drawing element maps to a DSL operation, and anything the interpreter cannot
//! draw yet is declared as a definition capability so the definition loads but
//! refuses to render with an explicit diagnostic instead of drawing silently
//! wrong geometry.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use sha2::{Digest, Sha256};
use thinoda_core::stencil::{
    Aspect, Connection, DSL_VERSION, Expr, Frame, LineCap, LineJoin, Op, PACKAGE_SCHEMA, PathCmd,
    SCHEMA_VERSION, StencilDefinition, StencilPackage, StrokeKeyword, StrokeWidthPolicy, TextAlign,
    TextVAlign, builtin_packages,
};
use thinoda_core::stencil_object::StencilRef;

use crate::IoError;
use crate::drawio::{BoundedDecodeError, Element, decode_compressed_bounded, parse_dom};

/// Capability declared by definitions that draw `<image>` elements.
pub const CAP_IMAGE: &str = "drawio.image";
/// Capability declared by definitions that include shapes from outside the file.
pub const CAP_EXTERNAL_INCLUDE: &str = "drawio.include-shape";
/// Extension key holding the bounded original `<shape>` XML.
pub const SOURCE_EXTENSION: &str = "drawio.source";
const MAX_SOURCE_BYTES: usize = 32 * 1024;
/// Upper bound for encoded and decoded stencil library text.
const MAX_STENCIL_BYTES: usize = 16 * 1024 * 1024;

/// A converted package plus non-fatal conversion notes.
#[derive(Debug, Clone)]
pub struct StencilImport {
    pub package: StencilPackage,
    pub diagnostics: Vec<String>,
}

/// Converts draw.io stencil XML into a package with the given namespaced ID.
pub fn import_drawio_stencils(
    xml: &str,
    package_id: &str,
    title: &str,
) -> Result<StencilImport, IoError> {
    let trimmed = xml.trim();
    if trimmed.len() > MAX_STENCIL_BYTES {
        return Err(IoError::Invalid(format!(
            "draw.io stencil library exceeds {} MiB",
            MAX_STENCIL_BYTES / (1024 * 1024)
        )));
    }
    let decoded;
    let text = if trimmed.starts_with('<') {
        trimmed
    } else {
        let mut work = MAX_STENCIL_BYTES.saturating_mul(6);
        decoded =
            decode_compressed_bounded(trimmed, MAX_STENCIL_BYTES, &mut work).map_err(|error| {
                match error {
                    BoundedDecodeError::Limit => IoError::Invalid(
                        "compressed draw.io stencil expands beyond the size limit".into(),
                    ),
                    BoundedDecodeError::Invalid => IoError::Invalid(
                        "draw.io stencil is neither XML nor compressed data".into(),
                    ),
                }
            })?;
        decoded.as_str()
    };
    let root = parse_dom(text)?;
    let shapes: Vec<&Element> = match root.name.as_str() {
        "shapes" => root.children.iter().filter(|c| c.name == "shape").collect(),
        "shape" => vec![&root],
        other => {
            return Err(IoError::Invalid(format!(
                "draw.io stencil XML must start with <shapes> or <shape>, found <{other}>"
            )));
        }
    };
    if shapes.is_empty() {
        return Err(IoError::Invalid(
            "draw.io stencil library has no shapes".into(),
        ));
    }

    let mut ids = BTreeMap::new();
    let mut used = BTreeSet::new();
    for (index, shape) in shapes.iter().enumerate() {
        let name = shape.attr("name").unwrap_or_default();
        let mut id = slug(name);
        if id.is_empty() {
            id = format!("shape-{}", index + 1);
        }
        let base = id.clone();
        let mut n = 2;
        while !used.insert(id.clone()) {
            id = format!("{base}-{n}");
            n += 1;
        }
        ids.entry(name.to_string()).or_insert(id);
    }

    let mut diagnostics = Vec::new();
    let mut definitions = Vec::with_capacity(shapes.len());
    for (index, shape) in shapes.iter().enumerate() {
        let name = shape.attr("name").unwrap_or_default();
        let id = if name.is_empty() {
            format!("shape-{}", index + 1)
        } else {
            ids[name].clone()
        };
        let mut cx = Convert {
            ids: &ids,
            requires: BTreeSet::new(),
            notes: Vec::new(),
        };
        let definition = cx.shape(shape, id.clone(), name);
        diagnostics.extend(cx.notes.into_iter().map(|n| format!("{id}: {n}")));
        definitions.push(definition);
    }

    let package = StencilPackage {
        schema: PACKAGE_SCHEMA.into(),
        schema_version: SCHEMA_VERSION,
        dsl_version: DSL_VERSION,
        id: package_id.into(),
        title: if title.is_empty() {
            root.attr("name").unwrap_or("draw.io stencils").into()
        } else {
            title.into()
        },
        revision: 1,
        license: None,
        requires: Vec::new(),
        definitions,
        extensions: BTreeMap::new(),
    };
    package
        .validate(&thinoda_core::stencil::Limits::DEFAULT)
        .map_err(|e| IoError::Invalid(e.to_string()))?;
    Ok(StencilImport {
        package,
        diagnostics,
    })
}

/// Lowercase ASCII identifier from a draw.io stencil name.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_') {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(96).collect()
}

struct Convert<'a> {
    ids: &'a BTreeMap<String, String>,
    requires: BTreeSet<String>,
    notes: Vec<String>,
}

impl Convert<'_> {
    fn shape(&mut self, shape: &Element, id: String, name: &str) -> StencilDefinition {
        let w = self.positive(shape, "w", 100.0);
        let h = self.positive(shape, "h", 100.0);
        let stroke_width = match shape.attr("strokewidth") {
            Some("inherit") => StrokeWidthPolicy::Keyword(StrokeKeyword::Inherit),
            Some(v) => StrokeWidthPolicy::Scale(self.parse(v, "strokewidth").max(0.0)),
            None => StrokeWidthPolicy::Scale(1.0),
        };
        let frame = Frame {
            w,
            h,
            aspect: if shape.attr("aspect") == Some("fixed") {
                Aspect::Fixed
            } else {
                Aspect::Variable
            },
            units: Default::default(),
            stroke_width,
        };

        let mut background = None;
        if let Some(bg) = shape.child("background") {
            for child in &bg.children {
                match self.geometry(child) {
                    Some(op) if background.is_none() => background = Some(op),
                    Some(_) => self.notes.push(
                        "background has extra geometry; only the first is the outline".into(),
                    ),
                    None => self.unsupported(&child.name),
                }
            }
        }
        let foreground = shape
            .child("foreground")
            .map(|fg| self.ops(&fg.children))
            .unwrap_or_default();

        let mut names = BTreeSet::new();
        let connections = shape
            .child("connections")
            .map(|c| {
                c.children
                    .iter()
                    .filter(|e| e.name == "constraint")
                    .map(|e| {
                        let name = e
                            .attr("name")
                            .filter(|n| !n.is_empty() && names.insert(n.to_string()))
                            .map(str::to_string);
                        Connection {
                            x: self.num(e, "x").clamp(0.0, 1.0),
                            y: self.num(e, "y").clamp(0.0, 1.0),
                            dx: 0.0,
                            dy: 0.0,
                            perimeter: e.attr("perimeter") == Some("1"),
                            name,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut extensions = BTreeMap::new();
        let mut source = String::new();
        write_xml(shape, &mut source);
        if source.len() <= MAX_SOURCE_BYTES {
            extensions.insert(SOURCE_EXTENSION.to_string(), Value::String(source));
        } else {
            self.notes
                .push("original XML is too large to preserve for round trips".into());
        }
        let mut interchange = BTreeMap::new();
        if !name.is_empty() {
            interchange.insert(
                "drawio".to_string(),
                serde_json::json!({ "stencilName": name }),
            );
        }

        StencilDefinition {
            id,
            title: if name.is_empty() {
                "Untitled shape".into()
            } else {
                name.into()
            },
            tags: Vec::new(),
            requires: std::mem::take(&mut self.requires).into_iter().collect(),
            frame,
            default_size: None,
            parameters: BTreeMap::new(),
            background,
            foreground,
            label: None,
            connections,
            handles: Vec::new(),
            metadata: BTreeMap::new(),
            interchange,
            extensions,
        }
    }

    fn geometry(&mut self, e: &Element) -> Option<Op> {
        let n = |cx: &mut Self, key| Expr::Num(cx.num(e, key));
        Some(match e.name.as_str() {
            "rect" => Op::Rect {
                x: n(self, "x"),
                y: n(self, "y"),
                w: n(self, "w"),
                h: n(self, "h"),
            },
            "roundrect" => Op::Roundrect {
                x: n(self, "x"),
                y: n(self, "y"),
                w: n(self, "w"),
                h: n(self, "h"),
                arcsize: e
                    .attr("arcsize")
                    .map(|v| Expr::Num(self.parse(v, "arcsize"))),
            },
            "ellipse" => Op::Ellipse {
                x: n(self, "x"),
                y: n(self, "y"),
                w: n(self, "w"),
                h: n(self, "h"),
            },
            "path" => Op::Path {
                commands: self.path(e),
            },
            _ => return None,
        })
    }

    fn path(&mut self, e: &Element) -> Vec<PathCmd> {
        let mut commands = Vec::new();
        for c in &e.children {
            let n = |cx: &mut Self, key| Expr::Num(cx.num(c, key));
            let command = match c.name.as_str() {
                "move" => PathCmd::Move {
                    x: n(self, "x"),
                    y: n(self, "y"),
                },
                "line" => PathCmd::Line {
                    x: n(self, "x"),
                    y: n(self, "y"),
                },
                "quad" => PathCmd::Quad {
                    x1: n(self, "x1"),
                    y1: n(self, "y1"),
                    x2: n(self, "x2"),
                    y2: n(self, "y2"),
                },
                "curve" => PathCmd::Curve {
                    x1: n(self, "x1"),
                    y1: n(self, "y1"),
                    x2: n(self, "x2"),
                    y2: n(self, "y2"),
                    x3: n(self, "x3"),
                    y3: n(self, "y3"),
                },
                "arc" => PathCmd::Arc {
                    rx: n(self, "rx"),
                    ry: n(self, "ry"),
                    x_axis_rotation: n(self, "x-axis-rotation"),
                    large_arc_flag: n(self, "large-arc-flag"),
                    sweep_flag: n(self, "sweep-flag"),
                    x: n(self, "x"),
                    y: n(self, "y"),
                },
                "close" => PathCmd::Close,
                other => {
                    self.unsupported(&format!("path.{other}"));
                    continue;
                }
            };
            // draw.io tolerates paths that do not start with a move.
            if commands.is_empty() && !matches!(command, PathCmd::Move { .. }) {
                commands.push(PathCmd::Move {
                    x: Expr::Num(0.0),
                    y: Expr::Num(0.0),
                });
                self.notes
                    .push("path did not start with <move>; started at 0,0".into());
            }
            commands.push(command);
        }
        if commands.is_empty() {
            commands.push(PathCmd::Move {
                x: Expr::Num(0.0),
                y: Expr::Num(0.0),
            });
        }
        commands
    }

    fn ops(&mut self, elements: &[Element]) -> Vec<Op> {
        let mut ops = Vec::with_capacity(elements.len());
        for e in elements {
            if let Some(geometry) = self.geometry(e) {
                ops.push(geometry);
                continue;
            }
            let color = |cx: &mut Self| cx.color(e);
            let op = match e.name.as_str() {
                "fill" => Op::Fill,
                "stroke" => Op::Stroke,
                "fillstroke" => Op::FillStroke,
                "save" => Op::Save,
                "restore" => Op::Restore,
                "strokecolor" => Op::StrokeColor { color: color(self) },
                "fillcolor" => Op::FillColor { color: color(self) },
                "fontcolor" => Op::FontColor { color: color(self) },
                "alpha" => Op::Alpha {
                    value: Expr::Num(self.num(e, "alpha").clamp(0.0, 1.0)),
                },
                "fillalpha" => Op::FillAlpha {
                    value: Expr::Num(self.num(e, "alpha").clamp(0.0, 1.0)),
                },
                "strokealpha" => Op::StrokeAlpha {
                    value: Expr::Num(self.num(e, "alpha").clamp(0.0, 1.0)),
                },
                "strokewidth" => Op::StrokeWidth {
                    width: Expr::Num(self.num(e, "width").max(0.0)),
                    fixed: e.attr("fixed") == Some("1"),
                },
                "dashed" => Op::Dashed {
                    on: e.attr("dashed") == Some("1"),
                },
                "dashpattern" => {
                    let pattern: Vec<f64> = e
                        .attr("pattern")
                        .unwrap_or_default()
                        .split_whitespace()
                        .filter_map(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite() && *v >= 0.0)
                        .collect();
                    if pattern.is_empty() || pattern.iter().all(|v| *v == 0.0) {
                        self.notes.push("ignored empty dash pattern".into());
                        continue;
                    }
                    Op::DashPattern { pattern }
                }
                "linejoin" => Op::LineJoin {
                    join: match e.attr("join") {
                        Some("round") => LineJoin::Round,
                        Some("bevel") => LineJoin::Bevel,
                        _ => LineJoin::Miter,
                    },
                },
                "linecap" => Op::LineCap {
                    cap: match e.attr("cap") {
                        Some("round") => LineCap::Round,
                        Some("square") => LineCap::Square,
                        _ => LineCap::Flat,
                    },
                },
                "miterlimit" => Op::MiterLimit {
                    limit: self.num(e, "limit").max(1.0),
                },
                "fontsize" => Op::FontSize {
                    size: Expr::Num(self.num(e, "size").max(1.0)),
                },
                "fontstyle" => Op::FontStyle {
                    style: (self.num(e, "style") as i64).clamp(0, 255) as u8,
                },
                "fontfamily" => Op::FontFamily {
                    family: e.attr("family").unwrap_or_default().into(),
                },
                "text" => {
                    if e.attr("placeholders") == Some("1") {
                        self.notes
                            .push("text placeholders are drawn literally".into());
                    }
                    if e.attr("localized") == Some("1") {
                        self.notes
                            .push("localized text is drawn untranslated".into());
                    }
                    Op::Text {
                        text: e.attr("str").unwrap_or_default().into(),
                        x: Expr::Num(self.num(e, "x")),
                        y: Expr::Num(self.num(e, "y")),
                        align: match e.attr("align") {
                            Some("center") => TextAlign::Center,
                            Some("right") => TextAlign::Right,
                            _ => TextAlign::Left,
                        },
                        valign: match e.attr("valign") {
                            Some("middle") => TextVAlign::Middle,
                            Some("bottom") => TextVAlign::Bottom,
                            _ => TextVAlign::Top,
                        },
                        rotation: self.num(e, "rotation"),
                        vertical: e.attr("vertical") == Some("1"),
                    }
                }
                "include-shape" => {
                    let name = e.attr("name").unwrap_or_default();
                    match self.ids.get(name) {
                        Some(id) => Op::Include {
                            definition: id.clone(),
                            x: Expr::Num(self.num(e, "x")),
                            y: Expr::Num(self.num(e, "y")),
                            w: Expr::Num(self.num(e, "w")),
                            h: Expr::Num(self.num(e, "h")),
                        },
                        None => {
                            self.requires.insert(CAP_EXTERNAL_INCLUDE.into());
                            self.notes
                                .push(format!("includes shape '{name}' from outside this library"));
                            continue;
                        }
                    }
                }
                other => {
                    self.unsupported(other);
                    continue;
                }
            };
            ops.push(op);
        }
        ops
    }

    /// Declares an element the interpreter cannot draw, so the definition
    /// refuses to render instead of silently dropping instructions.
    fn unsupported(&mut self, element: &str) {
        if element == "image" {
            self.requires.insert(CAP_IMAGE.into());
            self.notes.push("contains an image element".into());
        } else {
            self.requires
                .insert(format!("drawio.element.{}", slug(element)));
            self.notes.push(format!("unsupported element <{element}>"));
        }
    }

    fn color(&mut self, e: &Element) -> String {
        let value = e.attr("color").unwrap_or("none").trim();
        let valid_hex = value.starts_with('#') && thinoda_core::Color::from_hex(value).is_some();
        let valid_name = !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
        if valid_hex || valid_name {
            value.to_string()
        } else {
            self.notes
                .push(format!("invalid color '{value}' replaced by none"));
            "none".into()
        }
    }

    fn num(&mut self, e: &Element, key: &str) -> f64 {
        e.attr(key).map_or(0.0, |v| self.parse(v, key))
    }

    fn positive(&mut self, e: &Element, key: &str, default: f64) -> f64 {
        match e.attr(key) {
            Some(v) => {
                let value = self.parse(v, key);
                if value > 0.0 { value } else { default }
            }
            None => default,
        }
    }

    fn parse(&mut self, value: &str, key: &str) -> f64 {
        match value.trim().parse::<f64>() {
            Ok(v) if v.is_finite() => v,
            _ => {
                self.notes
                    .push(format!("invalid number '{value}' for {key}; used 0"));
                0.0
            }
        }
    }
}

fn write_xml(e: &Element, out: &mut String) {
    out.push('<');
    out.push_str(&e.name);
    for (k, v) in &e.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape(v, out);
        out.push('"');
    }
    if e.children.is_empty() && e.text.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    escape(&e.text, out);
    for c in &e.children {
        write_xml(c, out);
    }
    out.push_str("</");
    out.push_str(&e.name);
    out.push('>');
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Stencil references in diagram cell styles

/// Style keys that carry a Thinoda stencil reference through draw.io. draw.io
/// ignores them and draws the definition's `interchange.drawio.style`.
pub(crate) const STYLE_PACKAGE_KEY: &str = "thinodaStencilPackage";
pub(crate) const STYLE_DEFINITION_KEY: &str = "thinodaStencilDefinition";
pub(crate) const STYLE_PARAMETER_PREFIX: &str = "thinodaStencilParam.";

fn drawio_binding(definition: &StencilDefinition) -> Option<&serde_json::Map<String, Value>> {
    definition.interchange.get("drawio")?.as_object()
}

/// The definition's native draw.io style, when it declares one.
pub(crate) fn native_style(stencil: &StencilRef) -> Option<String> {
    let (_, definition) = stencil.resolve()?;
    let style = drawio_binding(definition)?.get("style")?.as_str()?;
    let mut out = style.to_owned();
    if !out.is_empty() && !out.ends_with(';') {
        out.push(';');
    }
    Some(out)
}

/// Package of stencils converted from cells' inline `shape=stencil(...)`.
pub(crate) const INLINE_PACKAGE_ID: &str = "drawio.inline";

/// draw.io keys for bound parameters plus the lossless reference keys.
pub(crate) fn reference_style(stencil: &StencilRef) -> String {
    let mut out = String::new();
    if let Some((_, definition)) = stencil.resolve()
        && let Some(bindings) = drawio_binding(definition)
            .and_then(|binding| binding.get("parameters"))
            .and_then(Value::as_object)
    {
        let values = definition.resolve_parameters(&stencil.parameters);
        for (name, binding) in bindings {
            let Some(&value) = values.get(name) else {
                continue;
            };
            match binding {
                Value::String(key) => out.push_str(&format!("{key}={value};")),
                Value::Object(choice) => {
                    let key = choice.get("style").and_then(Value::as_str);
                    let option = choice
                        .get("values")
                        .and_then(Value::as_array)
                        .and_then(|options| options.get(value.max(0.0) as usize))
                        .and_then(Value::as_str);
                    if let (Some(key), Some(option)) = (key, option) {
                        out.push_str(&format!("{key}={option};"));
                    }
                }
                _ => {}
            }
        }
    }
    out.push_str(&format!(
        "{STYLE_PACKAGE_KEY}={};{STYLE_DEFINITION_KEY}={};",
        stencil.package, stencil.definition
    ));
    for (name, value) in &stencil.parameters {
        out.push_str(&format!("{STYLE_PARAMETER_PREFIX}{name}={value};"));
    }
    out
}

/// Reads an explicit stencil reference from cell style entries. A draw.io-edited
/// bound parameter wins over the stored override, so edits made in draw.io survive.
fn explicit_reference(entries: &BTreeMap<&str, &str>) -> Option<StencilRef> {
    let mut stencil = StencilRef::new(
        *entries.get(STYLE_PACKAGE_KEY)?,
        *entries.get(STYLE_DEFINITION_KEY)?,
    );
    read_parameter_overrides(&mut stencil, entries);
    apply_bound_parameters(&mut stencil, entries, None);
    stencil.validate().ok().map(|()| stencil)
}

fn read_parameter_overrides(stencil: &mut StencilRef, entries: &BTreeMap<&str, &str>) {
    for (key, value) in entries {
        if let Some(name) = key.strip_prefix(STYLE_PARAMETER_PREFIX)
            && let Ok(value) = value.parse::<f64>()
            && value.is_finite()
        {
            stencil.parameters.insert(name.to_owned(), value);
        }
    }
}

/// Reads the definition's draw.io-bound parameters from `entries`. A key the
/// cell omits takes its `absent` value, which is the default draw.io itself
/// draws (it may differ from the definition default, or depend on other keys).
fn apply_bound_parameters(
    stencil: &mut StencilRef,
    entries: &BTreeMap<&str, &str>,
    absent: Option<&serde_json::Map<String, Value>>,
) {
    let parameter_updates = stencil
        .resolve()
        .and_then(|(_, definition)| {
            let bindings = drawio_binding(definition)
                .and_then(|binding| binding.get("parameters"))
                .and_then(Value::as_object)?;
            Some(
                bindings
                    .iter()
                    .filter_map(|(name, binding)| {
                        let parameter = definition.parameters.get(name)?;
                        let value = match binding {
                            Value::String(key) => entries
                                .get(key.as_str())
                                .and_then(|value| value.parse::<f64>().ok()),
                            Value::Object(choice) => choice
                                .get("style")
                                .and_then(Value::as_str)
                                .and_then(|key| entries.get(key))
                                .and_then(|selected| {
                                    choice
                                        .get("values")
                                        .and_then(Value::as_array)?
                                        .iter()
                                        .position(|option| option.as_str() == Some(selected))
                                        .map(|index| index as f64)
                                }),
                            _ => None,
                        };
                        let value = value
                            .filter(|value| value.is_finite())
                            .or_else(|| absent_value(absent?.get(name)?, entries))?;
                        let value = parameter.clamp(value);
                        (stencil.parameters.contains_key(name) || value != parameter.default)
                            .then(|| (name.clone(), value))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    for (name, value) in parameter_updates {
        stencil.parameters.insert(name, value);
    }
}

/// `absent` is a number or a list of `{"if": {key: value}, "value": n}` rules;
/// the first rule whose keys all match the cell style wins.
fn absent_value(rule: &Value, entries: &BTreeMap<&str, &str>) -> Option<f64> {
    match rule {
        Value::Number(number) => number.as_f64(),
        Value::Array(rules) => rules.iter().find_map(|rule| {
            let matches = rule
                .get("if")
                .and_then(Value::as_object)
                .is_none_or(|conditions| {
                    conditions.iter().all(|(key, expected)| {
                        let actual = entries.get(key.as_str()).copied();
                        match expected {
                            Value::String(expected) => actual == Some(expected.as_str()),
                            Value::Number(expected) => actual
                                .and_then(|actual| actual.parse::<f64>().ok())
                                .is_some_and(|actual| Some(actual) == expected.as_f64()),
                            Value::Null => actual.is_none(),
                            _ => false,
                        }
                    })
                });
            matches.then(|| rule.get("value")?.as_f64()).flatten()
        }),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

/// Package extension mapping draw.io shape names to definitions, so plain
/// draw.io cells resolve to data-defined stencils without per-shape code:
/// `{"folder": "folder"}` or `{"folder": {"definition": "folder", "absent": {...}}}`.
pub const IMPORT_EXTENSION: &str = "drawio.import";

struct ImportTarget {
    package: Arc<StencilPackage>,
    definition: String,
    absent: Option<&'static serde_json::Map<String, Value>>,
}

fn import_index() -> &'static HashMap<String, ImportTarget> {
    static INDEX: OnceLock<HashMap<String, ImportTarget>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        for package in builtin_packages() {
            let Some(map) = package
                .extensions
                .get(IMPORT_EXTENSION)
                .and_then(Value::as_object)
            else {
                continue;
            };
            let Some(shared) = thinoda_core::stencil_object::installed_package(&package.id) else {
                continue;
            };
            for (shape, target) in map {
                let (definition, absent) = match target {
                    Value::String(definition) => (definition.as_str(), None),
                    Value::Object(target) => match target.get("definition").and_then(Value::as_str)
                    {
                        Some(definition) => {
                            (definition, target.get("absent").and_then(Value::as_object))
                        }
                        None => continue,
                    },
                    _ => continue,
                };
                if package.definition(definition).is_none() {
                    continue;
                }
                // Earlier packages win, so a later library cannot hijack a name.
                index.entry(shape.clone()).or_insert_with(|| ImportTarget {
                    package: Arc::clone(&shared),
                    definition: definition.to_owned(),
                    absent,
                });
            }
        }
        index
    })
}

/// Whether a built-in package maps the draw.io shape name to a definition.
pub fn maps_drawio_shape(shape: &str) -> bool {
    import_index().contains_key(shape) || inline_payload(shape).is_some()
}

fn inline_payload(shape: &str) -> Option<&str> {
    shape
        .strip_prefix("stencil(")?
        .strip_suffix(')')
        .filter(|payload| !payload.is_empty())
}

const MAX_INLINE_CACHE: usize = 256;
/// Longest inline payload copied into the definition's draw.io style, so the
/// definition re-exports as the same inline stencil.
const MAX_INLINE_STYLE_BYTES: usize = 64 * 1024;

/// Converts an inline `stencil(...)` payload once. Inline stencils share one
/// package whose definition ids are derived from the payload, so equal
/// stencils share a definition and a page can embed all of them in one closure.
fn inline_package(shape: &str) -> Option<Arc<StencilPackage>> {
    type Cache = Mutex<HashMap<String, Option<Arc<StencilPackage>>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let payload = inline_payload(shape)?;
    let cache = CACHE.get_or_init(Default::default);
    if let Some(cached) = cache.lock().ok()?.get(payload) {
        return cached.clone();
    }
    let digest = Sha256::digest(payload.as_bytes());
    let hex: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
    let converted = import_drawio_stencils(payload, INLINE_PACKAGE_ID, "Inline")
        .ok()
        .and_then(|import| {
            let mut package = import.package;
            package.definitions.truncate(1);
            let definition = package.definitions.first_mut()?;
            definition.id = format!("stencil-{hex}");
            if payload.len() <= MAX_INLINE_STYLE_BYTES {
                let binding = definition
                    .interchange
                    .entry("drawio".into())
                    .or_insert_with(|| Value::Object(Default::default()));
                if let Value::Object(binding) = binding {
                    binding.insert("style".into(), Value::String(format!("shape={shape};")));
                }
            }
            package
                .validate(&thinoda_core::stencil::Limits::DEFAULT)
                .ok()?;
            Some(Arc::new(package))
        });
    let mut cache = cache.lock().ok()?;
    if cache.len() >= MAX_INLINE_CACHE {
        cache.clear();
    }
    cache.insert(payload.to_owned(), converted.clone());
    converted
}

/// One package with every inline stencil among `shapes`, so a page embeds a
/// single closure resource however many inline stencils it draws.
pub(crate) fn inline_bundle<'a>(
    shapes: impl IntoIterator<Item = &'a str>,
) -> Option<Arc<StencilPackage>> {
    let mut template = None;
    let mut definitions = BTreeMap::new();
    for shape in shapes {
        let Some(package) = inline_package(shape) else {
            continue;
        };
        for definition in &package.definitions {
            definitions
                .entry(definition.id.clone())
                .or_insert_with(|| definition.clone());
        }
        template.get_or_insert(package);
    }
    let template: Arc<StencilPackage> = template?;
    let mut bundle = StencilPackage::clone(&template);
    bundle.definitions = definitions.into_values().collect();
    bundle
        .validate(&thinoda_core::stencil::Limits::DEFAULT)
        .ok()?;
    Some(Arc::new(bundle))
}

/// Resolves a cell style to a stencil reference. An explicit Thinoda reference
/// wins; when its package is not installed, a matching inline stencil in the
/// same style draws it. With `implicit`, a plain draw.io shape name resolves
/// through the built-in `drawio.import` maps or its inline stencil payload.
pub(crate) fn style_reference<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
    shape: &str,
    implicit: bool,
) -> Option<StencilRef> {
    let entries: BTreeMap<&str, &str> = entries.into_iter().collect();
    if entries.contains_key(STYLE_PACKAGE_KEY) {
        let explicit = explicit_reference(&entries)?;
        if explicit.resolve().is_some() {
            return Some(explicit);
        }
        if let Some(package) = inline_package(shape)
            && package.id == explicit.package
            && package.definition(&explicit.definition).is_some()
        {
            let mut stencil = StencilRef::with_package(package, explicit.definition.clone());
            stencil.parameters = explicit.parameters.clone();
            apply_bound_parameters(&mut stencil, &entries, None);
            if stencil.resolve().is_some() && stencil.validate().is_ok() {
                return Some(stencil);
            }
        }
        return Some(explicit);
    }
    if !implicit {
        return None;
    }
    let stencil = if let Some(package) = inline_package(shape) {
        let definition = package.definitions.first()?.id.clone();
        StencilRef::with_package(package, definition)
    } else {
        let target = import_index().get(shape)?;
        let mut stencil =
            StencilRef::with_package(Arc::clone(&target.package), target.definition.clone());
        apply_bound_parameters(&mut stencil, &entries, target.absent);
        stencil
    };
    stencil.validate().ok()?;
    stencil.resolve()?;
    Some(stencil)
}

#[cfg(test)]
mod tests;
