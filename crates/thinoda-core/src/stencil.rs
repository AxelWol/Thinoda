//! Declarative JSON stencil packages and their bounded interpreter.
//!
//! A [`StencilPackage`] is pure data: every object's geometry, paint state,
//! text, parameters (modifiers), handles, connection points, composition and
//! interchange hints live in versioned JSON. Rust only implements the generic
//! language operations, validation and budgets defined here; renderers consume
//! the renderer-neutral [`StencilRender`] and never switch on definition IDs.
//!
//! The drawing model follows the official draw.io stencil language (a frame
//! with `w`/`h`, `aspect`, `strokewidth`, a single `background` geometry that is
//! painted by the first foreground paint operation, and a canvas state machine
//! with `save`/`restore`), extended with typed expressions so parameters can
//! drive geometry without executable strings.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::geometry::{Color, Rect, Vec2};

/// Value of [`StencilPackage::schema`].
pub const PACKAGE_SCHEMA: &str = "thinoda.stencil-package";
/// Highest supported [`StencilPackage::schema_version`].
pub const SCHEMA_VERSION: u32 = 1;
/// Highest supported [`StencilPackage::dsl_version`].
pub const DSL_VERSION: u32 = 1;
/// Capabilities this interpreter implements. Unknown package requirements
/// reject the package; unknown definition requirements make only that
/// definition unrenderable (with a diagnostic), so newer libraries still load.
pub const CAPABILITIES: &[&str] = &[
    "core.geometry",
    "core.paint",
    "core.text",
    "core.include",
    "core.conditions",
    "core.parameters",
    "core.handles",
    "core.connections",
];

/// Trusted engine limits. Libraries can never raise them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_package_bytes: usize,
    pub max_definitions: usize,
    pub max_static_nodes: usize,
    pub max_expr_depth: usize,
    pub max_op_depth: usize,
    pub max_eval_steps: usize,
    pub max_points: usize,
    pub max_include_depth: usize,
    pub max_state_depth: usize,
    pub max_text_chars: usize,
    pub max_connections: usize,
    pub max_id_chars: usize,
}

impl Limits {
    pub const DEFAULT: Limits = Limits {
        max_package_bytes: 4 * 1024 * 1024,
        max_definitions: 2_000,
        max_static_nodes: 400_000,
        max_expr_depth: 48,
        max_op_depth: 16,
        max_eval_steps: 200_000,
        max_points: 200_000,
        max_include_depth: 8,
        max_state_depth: 64,
        max_text_chars: 4_096,
        max_connections: 256,
        max_id_chars: 200,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StencilError {
    #[error("stencil package is invalid: {0}")]
    Invalid(String),
    #[error("stencil package needs a newer Thinoda ({0})")]
    UnsupportedVersion(String),
    #[error("stencil definition requires unsupported capabilities: {0:?}")]
    UnsupportedCapability(Vec<String>),
    #[error("stencil definition '{0}' was not found")]
    UnknownDefinition(String),
    #[error("stencil evaluation produced a non-finite value in '{0}'")]
    NonFinite(String),
    #[error("stencil evaluation exceeded the {0} limit")]
    Limit(&'static str),
}

fn invalid(message: impl Into<String>) -> StencilError {
    StencilError::Invalid(message.into())
}

// ---------------------------------------------------------------------------------------
// Package model

/// A versioned, namespaced collection of definitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StencilPackage {
    pub schema: String,
    pub schema_version: u32,
    pub dsl_version: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    pub definitions: Vec<StencilDefinition>,
    /// Namespaced extension data (`vendor.key`), preserved but not interpreted.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

/// One object definition: everything about the object is data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StencilDefinition {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(default)]
    pub frame: Frame,
    /// Initial instance size; defaults to the frame size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_size: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, Parameter>,
    /// The outline: hit region, perimeter and the path painted by the first
    /// foreground paint operation. Must be a geometry operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Op>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreground: Vec<Op>,
    /// Text region in drawing coordinates; defaults to the whole frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<ExprRect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<Connection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handles: Vec<Handle>,
    /// Typed metadata fields offered to the inspector (preserved verbatim).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
    /// Data-defined interchange bindings, e.g. `{"drawio": {"style": "shape=message"}}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub interchange: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Frame {
    #[serde(default = "hundred")]
    pub w: f64,
    #[serde(default = "hundred")]
    pub h: f64,
    #[serde(default)]
    pub aspect: Aspect,
    /// `frame` scales drawing coordinates from `w`×`h` to the instance like
    /// draw.io stencils; `instance` uses instance units directly.
    #[serde(default)]
    pub units: Units,
    #[serde(default)]
    pub stroke_width: StrokeWidthPolicy,
}

fn hundred() -> f64 {
    100.0
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            w: 100.0,
            h: 100.0,
            aspect: Aspect::Variable,
            units: Units::Frame,
            stroke_width: StrokeWidthPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Aspect {
    #[default]
    Variable,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Units {
    #[default]
    Frame,
    Instance,
}

/// draw.io `strokewidth`: `inherit` keeps the style width when resizing; a
/// number multiplies it by the smaller frame scale.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StrokeWidthPolicy {
    Scale(f64),
    Keyword(StrokeKeyword),
}

impl Default for StrokeWidthPolicy {
    fn default() -> Self {
        Self::Scale(1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StrokeKeyword {
    Inherit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Parameter {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default)]
    pub kind: ParameterKind,
    pub default: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Display labels of a `choice`; its value is the option index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParameterKind {
    #[default]
    Number,
    Choice,
    Toggle,
}

impl Parameter {
    /// Clamps `value` into the parameter's valid domain.
    pub fn clamp(&self, value: f64) -> f64 {
        let value = if value.is_finite() {
            value
        } else {
            self.default
        };
        match self.kind {
            ParameterKind::Number => {
                let lo = self.min.unwrap_or(f64::MIN);
                let hi = self.max.unwrap_or(f64::MAX);
                value.clamp(lo, hi)
            }
            ParameterKind::Choice => value
                .round()
                .clamp(0.0, self.options.len().saturating_sub(1) as f64),
            ParameterKind::Toggle => {
                if value != 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExprRect {
    pub x: Expr,
    pub y: Expr,
    pub w: Expr,
    pub h: Expr,
}

/// draw.io constraint: `x`/`y` relative to the instance bounds plus absolute
/// `dx`/`dy`; `perimeter` projects from the centre onto the outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Connection {
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dx: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dy: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub perimeter: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// A modifier handle: drawn at (`x`, `y`) and mapping a pointer (`px`, `py`
/// in drawing coordinates) to a new parameter `value`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Handle {
    pub parameter: String,
    pub x: Expr,
    pub y: Expr,
    pub value: Expr,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// Drawing program operations, mirroring the draw.io stencil language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Op {
    Rect {
        x: Expr,
        y: Expr,
        w: Expr,
        h: Expr,
    },
    /// `arcsize` is a percentage of the smaller side (draw.io default 15).
    Roundrect {
        x: Expr,
        y: Expr,
        w: Expr,
        h: Expr,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        arcsize: Option<Expr>,
    },
    Ellipse {
        x: Expr,
        y: Expr,
        w: Expr,
        h: Expr,
    },
    Path {
        commands: Vec<PathCmd>,
    },
    Fill,
    Stroke,
    FillStroke,
    Save,
    Restore,
    StrokeColor {
        color: String,
    },
    FillColor {
        color: String,
    },
    FontColor {
        color: String,
    },
    Alpha {
        value: Expr,
    },
    FillAlpha {
        value: Expr,
    },
    StrokeAlpha {
        value: Expr,
    },
    StrokeWidth {
        width: Expr,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fixed: bool,
    },
    Dashed {
        on: bool,
    },
    DashPattern {
        pattern: Vec<f64>,
    },
    LineJoin {
        join: LineJoin,
    },
    LineCap {
        cap: LineCap,
    },
    MiterLimit {
        limit: f64,
    },
    FontSize {
        size: Expr,
    },
    FontStyle {
        style: u8,
    },
    FontFamily {
        family: String,
    },
    Text {
        text: String,
        x: Expr,
        y: Expr,
        #[serde(default)]
        align: TextAlign,
        #[serde(default)]
        valign: TextVAlign,
        #[serde(default, skip_serializing_if = "is_zero")]
        rotation: f64,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        vertical: bool,
    },
    /// Draws another definition of the same package into a box.
    Include {
        definition: String,
        x: Expr,
        y: Expr,
        w: Expr,
        h: Expr,
    },
    If {
        cond: Expr,
        then: Vec<Op>,
        #[serde(default, rename = "else", skip_serializing_if = "Vec::is_empty")]
        otherwise: Vec<Op>,
    },
}

impl Op {
    fn is_geometry(&self) -> bool {
        matches!(
            self,
            Op::Rect { .. } | Op::Roundrect { .. } | Op::Ellipse { .. } | Op::Path { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PathCmd {
    Move {
        x: Expr,
        y: Expr,
    },
    Line {
        x: Expr,
        y: Expr,
    },
    Quad {
        x1: Expr,
        y1: Expr,
        x2: Expr,
        y2: Expr,
    },
    Curve {
        x1: Expr,
        y1: Expr,
        x2: Expr,
        y2: Expr,
        x3: Expr,
        y3: Expr,
    },
    /// SVG elliptical arc; flags are true when non-zero.
    Arc {
        rx: Expr,
        ry: Expr,
        #[serde(default = "zero_expr")]
        x_axis_rotation: Expr,
        large_arc_flag: Expr,
        sweep_flag: Expr,
        x: Expr,
        y: Expr,
    },
    Close,
}

fn zero_expr() -> Expr {
    Expr::Num(0.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineCap {
    Flat,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextVAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Typed expression AST: a number, a variable name (`"w"` or `{"var": "w"}`)
/// or an operator call. There is no string evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Expr {
    Num(f64),
    Name(String),
    Var(VarRef),
    Call(Call),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VarRef {
    pub var: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub op: ExprOp,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExprOp {
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Min,
    Max,
    Abs,
    Clamp,
    Sqrt,
    Sin,
    Cos,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    Not,
    If,
}

impl ExprOp {
    fn arity(self) -> (usize, usize) {
        use ExprOp::*;
        match self {
            Add | Mul | Min | Max | And | Or => (1, 64),
            Sub | Div | Lt | Le | Gt | Ge | Eq | Ne => (2, 2),
            Neg | Abs | Sqrt | Sin | Cos | Not => (1, 1),
            Clamp | If => (3, 3),
        }
    }
}

/// Variables always available to expressions.
pub const BUILTIN_VARIABLES: &[&str] = &["w", "h", "frameW", "frameH", "strokeWidth"];
/// Extra variables available in a handle's `value` expression.
pub const HANDLE_VARIABLES: &[&str] = &["px", "py"];

// ---------------------------------------------------------------------------------------
// Loading and validation

impl StencilPackage {
    /// Parses and validates a package with the default limits.
    pub fn from_json(text: &str) -> Result<Self, StencilError> {
        Self::from_json_with_limits(text, &Limits::DEFAULT)
    }

    pub fn from_json_with_limits(text: &str, limits: &Limits) -> Result<Self, StencilError> {
        if text.len() > limits.max_package_bytes {
            return Err(StencilError::Limit("package size"));
        }
        let package: StencilPackage =
            serde_json::from_str(text).map_err(|error| invalid(error.to_string()))?;
        package.validate(limits)?;
        Ok(package)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn definition(&self, id: &str) -> Option<&StencilDefinition> {
        self.definitions.iter().find(|d| d.id == id)
    }

    /// Structural validation before any document can reference the package.
    pub fn validate(&self, limits: &Limits) -> Result<(), StencilError> {
        if self.schema != PACKAGE_SCHEMA {
            return Err(invalid(format!("unexpected schema '{}'", self.schema)));
        }
        if self.schema_version > SCHEMA_VERSION || self.schema_version == 0 {
            return Err(StencilError::UnsupportedVersion(format!(
                "schema version {}",
                self.schema_version
            )));
        }
        if self.dsl_version > DSL_VERSION || self.dsl_version == 0 {
            return Err(StencilError::UnsupportedVersion(format!(
                "DSL version {}",
                self.dsl_version
            )));
        }
        let missing = unsupported(&self.requires);
        if !missing.is_empty() {
            return Err(StencilError::UnsupportedCapability(missing));
        }
        check_id(&self.id, "package id", limits)?;
        check_extensions(&self.extensions)?;
        if self.definitions.len() > limits.max_definitions {
            return Err(StencilError::Limit("definition count"));
        }
        let mut ids = BTreeSet::new();
        for definition in &self.definitions {
            check_id(&definition.id, "definition id", limits)?;
            if !ids.insert(definition.id.as_str()) {
                return Err(invalid(format!("duplicate definition '{}'", definition.id)));
            }
        }
        let mut nodes = 0usize;
        for definition in &self.definitions {
            definition.validate(&ids, limits, &mut nodes)?;
        }
        self.check_include_cycles()?;
        Ok(())
    }

    fn check_include_cycles(&self) -> Result<(), StencilError> {
        fn includes<'a>(ops: &'a [Op], out: &mut Vec<&'a str>) {
            for op in ops {
                match op {
                    Op::Include { definition, .. } => out.push(definition),
                    Op::If {
                        then, otherwise, ..
                    } => {
                        includes(then, out);
                        includes(otherwise, out);
                    }
                    _ => {}
                }
            }
        }
        let edges: BTreeMap<&str, Vec<&str>> = self
            .definitions
            .iter()
            .map(|d| {
                let mut out = Vec::new();
                includes(&d.foreground, &mut out);
                (d.id.as_str(), out)
            })
            .collect();
        // 0 = unvisited, 1 = on stack, 2 = done
        let mut state: BTreeMap<&str, u8> = BTreeMap::new();
        for start in edges.keys() {
            if state.get(start).copied().unwrap_or(0) != 0 {
                continue;
            }
            let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
            state.insert(start, 1);
            while let Some((node, index)) = stack.pop() {
                let next = edges.get(node).and_then(|e| e.get(index)).copied();
                match next {
                    Some(child) => {
                        stack.push((node, index + 1));
                        match state.get(child).copied().unwrap_or(0) {
                            1 => {
                                return Err(invalid(format!(
                                    "definition '{child}' includes itself"
                                )));
                            }
                            0 => {
                                state.insert(child, 1);
                                stack.push((child, 0));
                            }
                            _ => {}
                        }
                    }
                    None => {
                        state.insert(node, 2);
                    }
                }
            }
        }
        Ok(())
    }

    /// Stable content hash of one definition, used to pin instances.
    pub fn definition_hash(definition: &StencilDefinition) -> String {
        let bytes = serde_json::to_vec(definition).unwrap_or_default();
        let digest = Sha256::digest(&bytes);
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn unsupported(requires: &[String]) -> Vec<String> {
    requires
        .iter()
        .filter(|c| !CAPABILITIES.contains(&c.as_str()))
        .cloned()
        .collect()
}

fn check_id(id: &str, what: &str, limits: &Limits) -> Result<(), StencilError> {
    let valid = !id.is_empty()
        && id.chars().count() <= limits.max_id_chars
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/'));
    if valid {
        Ok(())
    } else {
        Err(invalid(format!("{what} '{id}' is not a valid identifier")))
    }
}

fn check_extensions(extensions: &BTreeMap<String, Value>) -> Result<(), StencilError> {
    match extensions.keys().find(|k| !k.contains('.')) {
        Some(key) => Err(invalid(format!(
            "extension key '{key}' must be namespaced (vendor.key)"
        ))),
        None => Ok(()),
    }
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 64
}

/// Accepted color references: `#rgb`, `#rrggbb`, `none`, `default`, or a
/// style name (`fill`, `stroke`, `font` or a custom color style).
fn check_color(color: &str) -> Result<(), StencilError> {
    if color.starts_with('#') {
        return Color::from_hex(color)
            .map(|_| ())
            .ok_or_else(|| invalid(format!("invalid color '{color}'")));
    }
    if is_identifier(color) {
        Ok(())
    } else {
        Err(invalid(format!("invalid color reference '{color}'")))
    }
}

struct Scope<'a> {
    parameters: &'a BTreeMap<String, Parameter>,
    handle: bool,
}

impl Scope<'_> {
    fn knows(&self, name: &str) -> bool {
        BUILTIN_VARIABLES.contains(&name)
            || self.parameters.contains_key(name)
            || (self.handle && HANDLE_VARIABLES.contains(&name))
    }
}

fn check_expr(
    expr: &Expr,
    scope: &Scope<'_>,
    depth: usize,
    limits: &Limits,
    nodes: &mut usize,
) -> Result<(), StencilError> {
    *nodes += 1;
    if *nodes > limits.max_static_nodes {
        return Err(StencilError::Limit("package node"));
    }
    if depth > limits.max_expr_depth {
        return Err(StencilError::Limit("expression depth"));
    }
    match expr {
        Expr::Num(value) if value.is_finite() => Ok(()),
        Expr::Num(_) => Err(invalid("non-finite number")),
        Expr::Name(name) | Expr::Var(VarRef { var: name }) => {
            if scope.knows(name) {
                Ok(())
            } else {
                Err(invalid(format!("unknown variable '{name}'")))
            }
        }
        Expr::Call(call) => {
            let (lo, hi) = call.op.arity();
            if call.args.len() < lo || call.args.len() > hi {
                return Err(invalid(format!(
                    "operator {:?} takes {lo}..={hi} arguments",
                    call.op
                )));
            }
            for arg in &call.args {
                check_expr(arg, scope, depth + 1, limits, nodes)?;
            }
            Ok(())
        }
    }
}

impl StencilDefinition {
    fn validate(
        &self,
        ids: &BTreeSet<&str>,
        limits: &Limits,
        nodes: &mut usize,
    ) -> Result<(), StencilError> {
        let context = |message: String| invalid(format!("{}: {message}", self.id));
        let frame = &self.frame;
        if !(frame.w.is_finite() && frame.h.is_finite() && frame.w > 0.0 && frame.h > 0.0) {
            return Err(context("frame size must be positive".into()));
        }
        if let StrokeWidthPolicy::Scale(scale) = frame.stroke_width
            && !(scale.is_finite() && scale >= 0.0)
        {
            return Err(context("stroke width scale must be finite".into()));
        }
        if let Some([w, h]) = self.default_size
            && !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0)
        {
            return Err(context("default size must be positive".into()));
        }
        check_extensions(&self.extensions).map_err(|e| context(e.to_string()))?;
        for (name, parameter) in &self.parameters {
            if !is_identifier(name)
                || BUILTIN_VARIABLES.contains(&name.as_str())
                || HANDLE_VARIABLES.contains(&name.as_str())
            {
                return Err(context(format!("invalid parameter name '{name}'")));
            }
            let bounds_ok = [Some(parameter.default), parameter.min, parameter.max]
                .into_iter()
                .flatten()
                .all(f64::is_finite)
                && parameter.min.zip(parameter.max).is_none_or(|(a, b)| a <= b);
            if !bounds_ok {
                return Err(context(format!("parameter '{name}' has an invalid range")));
            }
            if parameter.kind == ParameterKind::Choice && parameter.options.is_empty() {
                return Err(context(format!("choice parameter '{name}' has no options")));
            }
            if parameter.clamp(parameter.default) != parameter.default {
                return Err(context(format!(
                    "parameter '{name}' default is outside its range"
                )));
            }
        }
        let scope = Scope {
            parameters: &self.parameters,
            handle: false,
        };
        if let Some(background) = &self.background {
            if !background.is_geometry() {
                return Err(context("background must be a geometry operation".into()));
            }
            check_ops(
                std::slice::from_ref(background),
                &scope,
                ids,
                0,
                limits,
                nodes,
            )
            .map_err(|e| context(e.to_string()))?;
        }
        check_ops(&self.foreground, &scope, ids, 0, limits, nodes)
            .map_err(|e| context(e.to_string()))?;
        if let Some(label) = &self.label {
            for expr in [&label.x, &label.y, &label.w, &label.h] {
                check_expr(expr, &scope, 0, limits, nodes).map_err(|e| context(e.to_string()))?;
            }
        }
        if self.connections.len() > limits.max_connections {
            return Err(StencilError::Limit("connection count"));
        }
        let mut names = BTreeSet::new();
        for connection in &self.connections {
            if ![connection.x, connection.y, connection.dx, connection.dy]
                .iter()
                .all(|v| v.is_finite())
            {
                return Err(context("connection coordinates must be finite".into()));
            }
            if let Some(name) = &connection.name
                && !names.insert(name.as_str())
            {
                return Err(context(format!("duplicate connection name '{name}'")));
            }
        }
        let handle_scope = Scope {
            parameters: &self.parameters,
            handle: true,
        };
        for handle in &self.handles {
            match self.parameters.get(&handle.parameter) {
                Some(p) if p.kind == ParameterKind::Number => {}
                _ => {
                    return Err(context(format!(
                        "handle parameter '{}' must be a number parameter",
                        handle.parameter
                    )));
                }
            }
            for expr in [&handle.x, &handle.y] {
                check_expr(expr, &scope, 0, limits, nodes).map_err(|e| context(e.to_string()))?;
            }
            check_expr(&handle.value, &handle_scope, 0, limits, nodes)
                .map_err(|e| context(e.to_string()))?;
        }
        Ok(())
    }

    /// Capabilities required by this definition that this interpreter lacks.
    pub fn missing_capabilities(&self) -> Vec<String> {
        unsupported(&self.requires)
    }

    /// Default instance size.
    pub fn default_size(&self) -> [f64; 2] {
        self.default_size.unwrap_or([self.frame.w, self.frame.h])
    }

    /// Parameter defaults overlaid with clamped instance overrides.
    pub fn resolve_parameters(&self, overrides: &BTreeMap<String, f64>) -> BTreeMap<String, f64> {
        self.parameters
            .iter()
            .map(|(name, parameter)| {
                let value = overrides
                    .get(name)
                    .map_or(parameter.default, |v| parameter.clamp(*v));
                (name.clone(), value)
            })
            .collect()
    }
}

fn check_ops(
    ops: &[Op],
    scope: &Scope<'_>,
    ids: &BTreeSet<&str>,
    depth: usize,
    limits: &Limits,
    nodes: &mut usize,
) -> Result<(), StencilError> {
    if depth > limits.max_op_depth {
        return Err(StencilError::Limit("operation nesting"));
    }
    let expr = |e: &Expr, nodes: &mut usize| check_expr(e, scope, 0, limits, nodes);
    for op in ops {
        *nodes += 1;
        if *nodes > limits.max_static_nodes {
            return Err(StencilError::Limit("package node"));
        }
        match op {
            Op::Rect { x, y, w, h } | Op::Ellipse { x, y, w, h } => {
                for e in [x, y, w, h] {
                    expr(e, nodes)?;
                }
            }
            Op::Roundrect {
                x,
                y,
                w,
                h,
                arcsize,
            } => {
                for e in [x, y, w, h].into_iter().chain(arcsize.as_ref()) {
                    expr(e, nodes)?;
                }
            }
            Op::Path { commands } => {
                if !matches!(commands.first(), None | Some(PathCmd::Move { .. })) {
                    return Err(invalid("a path must start with a move"));
                }
                for command in commands {
                    *nodes += 1;
                    for e in command.exprs() {
                        expr(e, nodes)?;
                    }
                }
            }
            Op::StrokeColor { color } | Op::FillColor { color } | Op::FontColor { color } => {
                check_color(color)?
            }
            Op::Alpha { value }
            | Op::FillAlpha { value }
            | Op::StrokeAlpha { value }
            | Op::StrokeWidth { width: value, .. }
            | Op::FontSize { size: value } => expr(value, nodes)?,
            Op::DashPattern { pattern } => {
                if pattern.len() > 16 || !pattern.iter().all(|v| v.is_finite() && *v >= 0.0) {
                    return Err(invalid("dash pattern must have at most 16 finite lengths"));
                }
            }
            Op::MiterLimit { limit } if !limit.is_finite() => {
                return Err(invalid("miter limit must be finite"));
            }
            Op::FontFamily { family } if family.chars().count() > 200 => {
                return Err(invalid("font family is too long"));
            }
            Op::Text { text, x, y, .. } => {
                if text.chars().count() > limits.max_text_chars {
                    return Err(StencilError::Limit("text length"));
                }
                expr(x, nodes)?;
                expr(y, nodes)?;
            }
            Op::Include {
                definition,
                x,
                y,
                w,
                h,
            } => {
                if !ids.contains(definition.as_str()) {
                    return Err(invalid(format!(
                        "unknown included definition '{definition}'"
                    )));
                }
                for e in [x, y, w, h] {
                    expr(e, nodes)?;
                }
            }
            Op::If {
                cond,
                then,
                otherwise,
            } => {
                expr(cond, nodes)?;
                check_ops(then, scope, ids, depth + 1, limits, nodes)?;
                check_ops(otherwise, scope, ids, depth + 1, limits, nodes)?;
            }
            _ => {}
        }
    }
    Ok(())
}

impl PathCmd {
    fn exprs(&self) -> Vec<&Expr> {
        match self {
            PathCmd::Move { x, y } | PathCmd::Line { x, y } => vec![x, y],
            PathCmd::Quad { x1, y1, x2, y2 } => vec![x1, y1, x2, y2],
            PathCmd::Curve {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => vec![x1, y1, x2, y2, x3, y3],
            PathCmd::Arc {
                rx,
                ry,
                x_axis_rotation,
                large_arc_flag,
                sweep_flag,
                x,
                y,
            } => vec![rx, ry, x_axis_rotation, large_arc_flag, sweep_flag, x, y],
            PathCmd::Close => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Renderer-neutral output

/// A color reference resolved by the consumer: a literal, nothing, or a named
/// style slot (`fill`, `stroke`, `font`, `background`, `foreground` or custom).
#[derive(Debug, Clone, PartialEq)]
pub enum PaintRef {
    None,
    Rgb(Color),
    Style(String),
}

impl PaintRef {
    fn parse(color: &str, default_slot: &str) -> PaintRef {
        match color {
            "none" => PaintRef::None,
            "default" => PaintRef::Style(default_slot.to_string()),
            hex if hex.starts_with('#') => {
                Color::from_hex(hex).map_or(PaintRef::None, PaintRef::Rgb)
            }
            name => PaintRef::Style(name.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subpath {
    /// Instance-local points (origin at the instance's top-left corner).
    pub points: Vec<Vec2>,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FillPaint {
    pub color: PaintRef,
    pub alpha: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrokePaint {
    pub color: PaintRef,
    pub alpha: f32,
    pub width: f32,
    /// `None` inherits the instance's dash style.
    pub dashed: Option<bool>,
    pub dash_pattern: Vec<f32>,
    pub join: LineJoin,
    pub cap: LineCap,
    pub miter_limit: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaintedPath {
    pub subpaths: Vec<Subpath>,
    pub fill: Option<FillPaint>,
    pub stroke: Option<StrokePaint>,
    /// Painted from the background outline (shadow source in draw.io).
    pub background: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderText {
    pub text: String,
    pub at: Vec2,
    pub align: TextAlign,
    pub valign: TextVAlign,
    pub rotation: f32,
    pub vertical: bool,
    pub size: f32,
    pub color: PaintRef,
    pub alpha: f32,
    pub font_style: u8,
    pub font_family: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderPort {
    pub name: Option<String>,
    pub at: Vec2,
    pub perimeter: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderHandle {
    pub parameter: String,
    pub label: String,
    pub at: Vec2,
}

/// Everything consumers need, in instance-local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct StencilRender {
    pub size: Vec2,
    pub paths: Vec<PaintedPath>,
    pub texts: Vec<RenderText>,
    /// Flattened background outline used for hit testing and perimeter ports.
    pub outline: Vec<Subpath>,
    pub label: Rect,
    pub ports: Vec<RenderPort>,
    pub handles: Vec<RenderHandle>,
}

impl StencilRender {
    /// Even-odd hit test against the outline (or the bounds without one).
    pub fn contains(&self, point: Vec2) -> bool {
        if self.outline.is_empty() {
            return Rect::from_min_size(Vec2::ZERO, self.size).contains(point);
        }
        let mut inside = false;
        for subpath in &self.outline {
            let points = &subpath.points;
            let n = points.len();
            for i in 0..n {
                let a = points[i];
                let b = points[(i + n - 1) % n];
                if (a.y > point.y) != (b.y > point.y)
                    && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
                {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Maps instance-local points onto a sheet frame rotated clockwise by
    /// `rotation` degrees around its centre (the board convention).
    pub fn to_sheet(&self, point: Vec2, rect: Rect, rotation: f32) -> Vec2 {
        let sx = if self.size.x > 0.0 {
            rect.size.x / self.size.x
        } else {
            1.0
        };
        let sy = if self.size.y > 0.0 {
            rect.size.y / self.size.y
        } else {
            1.0
        };
        let p = Vec2::new(rect.min.x + point.x * sx, rect.min.y + point.y * sy);
        if rotation == 0.0 {
            return p;
        }
        let c = rect.center();
        let (sin, cos) = rotation.to_radians().sin_cos();
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        Vec2::new(c.x + dx * cos - dy * sin, c.y + dx * sin + dy * cos)
    }
}

// ---------------------------------------------------------------------------------------
// Evaluation

/// Instance inputs for evaluation.
#[derive(Debug, Clone, Copy)]
pub struct Instance<'a> {
    pub width: f64,
    pub height: f64,
    /// The instance style's stroke width.
    pub stroke_width: f64,
    pub parameters: &'a BTreeMap<String, f64>,
}

/// Affine scale + translation from drawing coordinates to instance-local ones.
#[derive(Debug, Clone, Copy)]
struct Xform {
    sx: f64,
    sy: f64,
    tx: f64,
    ty: f64,
}

impl Xform {
    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.tx + x * self.sx, self.ty + y * self.sy)
    }

    fn invert(&self, x: f64, y: f64) -> (f64, f64) {
        (
            if self.sx != 0.0 {
                (x - self.tx) / self.sx
            } else {
                0.0
            },
            if self.sy != 0.0 {
                (y - self.ty) / self.sy
            } else {
                0.0
            },
        )
    }
}

#[derive(Debug, Clone)]
struct State {
    fill: PaintRef,
    stroke: PaintRef,
    font: PaintRef,
    alpha: f64,
    fill_alpha: f64,
    stroke_alpha: f64,
    stroke_width: f64,
    dashed: Option<bool>,
    dash: Vec<f64>,
    join: LineJoin,
    cap: LineCap,
    miter: f64,
    font_size: f64,
    font_style: u8,
    font_family: Option<String>,
}

struct Budget {
    steps: usize,
    points: usize,
    limits: Limits,
}

impl Budget {
    fn step(&mut self, n: usize) -> Result<(), StencilError> {
        self.steps += n;
        if self.steps > self.limits.max_eval_steps {
            Err(StencilError::Limit("evaluation step"))
        } else {
            Ok(())
        }
    }

    fn points(&mut self, n: usize) -> Result<(), StencilError> {
        self.points += n;
        if self.points > self.limits.max_points {
            Err(StencilError::Limit("path point"))
        } else {
            Ok(())
        }
    }
}

struct Env<'a> {
    vars: BTreeMap<&'a str, f64>,
}

impl Env<'_> {
    fn eval(&self, expr: &Expr, budget: &mut Budget) -> Result<f64, StencilError> {
        budget.step(1)?;
        let value = match expr {
            Expr::Num(value) => *value,
            Expr::Name(name) | Expr::Var(VarRef { var: name }) => self
                .vars
                .get(name.as_str())
                .copied()
                .ok_or_else(|| invalid(format!("unknown variable '{name}'")))?,
            Expr::Call(call) => {
                let mut args = Vec::with_capacity(call.args.len());
                // `if` evaluates lazily so a guarded branch cannot fail.
                if call.op == ExprOp::If {
                    let cond = self.eval(&call.args[0], budget)?;
                    let branch = if cond != 0.0 { 1 } else { 2 };
                    return self.eval(&call.args[branch], budget);
                }
                for arg in &call.args {
                    args.push(self.eval(arg, budget)?);
                }
                apply(call.op, &args)
            }
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(StencilError::NonFinite(format!("{expr:?}")))
        }
    }
}

fn apply(op: ExprOp, args: &[f64]) -> f64 {
    let bool_value = |b: bool| if b { 1.0 } else { 0.0 };
    match op {
        ExprOp::Add => args.iter().sum(),
        ExprOp::Mul => args.iter().product(),
        ExprOp::Sub => args[0] - args[1],
        ExprOp::Div => args[0] / args[1],
        ExprOp::Neg => -args[0],
        ExprOp::Min => args.iter().copied().fold(f64::INFINITY, f64::min),
        ExprOp::Max => args.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ExprOp::Abs => args[0].abs(),
        ExprOp::Clamp => {
            if args[1] > args[2] {
                f64::NAN
            } else {
                args[0].clamp(args[1], args[2])
            }
        }
        ExprOp::Sqrt => args[0].sqrt(),
        ExprOp::Sin => args[0].to_radians().sin(),
        ExprOp::Cos => args[0].to_radians().cos(),
        ExprOp::Lt => bool_value(args[0] < args[1]),
        ExprOp::Le => bool_value(args[0] <= args[1]),
        ExprOp::Gt => bool_value(args[0] > args[1]),
        ExprOp::Ge => bool_value(args[0] >= args[1]),
        ExprOp::Eq => bool_value(args[0] == args[1]),
        ExprOp::Ne => bool_value(args[0] != args[1]),
        ExprOp::And => bool_value(args.iter().all(|v| *v != 0.0)),
        ExprOp::Or => bool_value(args.iter().any(|v| *v != 0.0)),
        ExprOp::Not => bool_value(args[0] == 0.0),
        ExprOp::If => unreachable!("handled lazily"),
    }
}

/// Output value that stays finite after narrowing to `f32`.
fn finite_f32(value: f64, what: &str) -> Result<f32, StencilError> {
    let narrowed = value as f32;
    if narrowed.is_finite() {
        Ok(narrowed)
    } else {
        Err(StencilError::NonFinite(what.into()))
    }
}

fn finite_vec(x: f64, y: f64) -> Result<Vec2, StencilError> {
    Ok(Vec2::new(
        finite_f32(x, "coordinate")?,
        finite_f32(y, "coordinate")?,
    ))
}

/// Draw scale and initial stroke width of a definition drawn through `xf`.
/// draw.io: `inherit` uses the style width, a number is scaled by `minScale`.
fn frame_paint(frame: &Frame, xf: &Xform, style_width: f64) -> (f64, f64) {
    let min_scale = match frame.units {
        Units::Frame => xf.sx.min(xf.sy).abs(),
        Units::Instance => 1.0,
    };
    let width = match frame.stroke_width {
        StrokeWidthPolicy::Keyword(StrokeKeyword::Inherit) => style_width,
        StrokeWidthPolicy::Scale(width) => width * min_scale,
    };
    (min_scale, width)
}

/// Flattening resolution; points are charged to the evaluation budget.
const QUAD_STEPS: usize = 12;
const CURVE_STEPS: usize = 16;
const ELLIPSE_STEPS: usize = 48;
const CORNER_STEPS: usize = 8;

struct PathBuilder {
    subpaths: Vec<Subpath>,
    current: Vec<Vec2>,
    cursor: (f64, f64),
    start: (f64, f64),
}

impl PathBuilder {
    fn new() -> Self {
        Self {
            subpaths: Vec::new(),
            current: Vec::new(),
            cursor: (0.0, 0.0),
            start: (0.0, 0.0),
        }
    }

    fn flush(&mut self, closed: bool) {
        if self.current.len() > 1 {
            self.subpaths.push(Subpath {
                points: std::mem::take(&mut self.current),
                closed,
            });
        } else {
            self.current.clear();
        }
    }

    fn push(
        &mut self,
        xf: &Xform,
        x: f64,
        y: f64,
        budget: &mut Budget,
    ) -> Result<(), StencilError> {
        if self.current.is_empty() {
            budget.points(2)?;
            let (sx, sy) = xf.apply(self.cursor.0, self.cursor.1);
            self.current.push(finite_vec(sx, sy)?);
        } else {
            budget.points(1)?;
        }
        let (px, py) = xf.apply(x, y);
        self.current.push(finite_vec(px, py)?);
        self.cursor = (x, y);
        Ok(())
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.flush(false);
        self.cursor = (x, y);
        self.start = (x, y);
    }

    fn close(&mut self) {
        self.flush(true);
        self.cursor = self.start;
    }

    fn finish(mut self) -> Vec<Subpath> {
        self.flush(false);
        self.subpaths
    }
}

/// Converts an SVG endpoint arc to center form and samples it (SVG 1.1 F.6.5).
#[allow(clippy::too_many_arguments)]
fn arc_points(
    from: (f64, f64),
    rx: f64,
    ry: f64,
    rotation: f64,
    large: bool,
    sweep: bool,
    to: (f64, f64),
) -> Vec<(f64, f64)> {
    let (x1, y1) = from;
    let (x2, y2) = to;
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || (x1 == x2 && y1 == y2) {
        return vec![to];
    }
    let (sin, cos) = rotation.to_radians().sin_cos();
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = if den == 0.0 {
        0.0
    } else {
        (num / den).max(0.0).sqrt()
    };
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;
    let cx = cos * cxp - sin * cyp + (x1 + x2) / 2.0;
    let cy = sin * cxp + cos * cyp + (y1 + y2) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        if a.is_finite() { a } else { 0.0 }
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    let tau = std::f64::consts::TAU;
    if !sweep && delta > 0.0 {
        delta -= tau;
    } else if sweep && delta < 0.0 {
        delta += tau;
    }
    let steps = ((delta.abs() / (std::f64::consts::PI / 16.0)).ceil() as usize).clamp(2, 64);
    (1..=steps)
        .map(|i| {
            if i == steps {
                return to;
            }
            let t = theta1 + delta * i as f64 / steps as f64;
            let (st, ct) = t.sin_cos();
            (
                cx + rx * ct * cos - ry * st * sin,
                cy + rx * ct * sin + ry * st * cos,
            )
        })
        .collect()
}

struct Evaluator<'a> {
    package: &'a StencilPackage,
    budget: Budget,
    state: State,
    stack: Vec<State>,
    current: Option<(Vec<Subpath>, bool)>,
    paths: Vec<PaintedPath>,
    texts: Vec<RenderText>,
    min_scale: f64,
}

fn frame_xform(frame: &Frame, x: f64, y: f64, w: f64, h: f64) -> Xform {
    match frame.units {
        Units::Instance => Xform {
            sx: 1.0,
            sy: 1.0,
            tx: x,
            ty: y,
        },
        Units::Frame => {
            let mut sx = w / frame.w;
            let mut sy = h / frame.h;
            let (mut tx, mut ty) = (x, y);
            if frame.aspect == Aspect::Fixed {
                let s = sx.min(sy);
                tx += (w - frame.w * s) / 2.0;
                ty += (h - frame.h * s) / 2.0;
                sx = s;
                sy = s;
            }
            Xform { sx, sy, tx, ty }
        }
    }
}

fn definition_env<'a>(
    definition: &'a StencilDefinition,
    w: f64,
    h: f64,
    stroke_width: f64,
    overrides: &BTreeMap<String, f64>,
) -> Env<'a> {
    let mut vars: BTreeMap<&str, f64> = BTreeMap::new();
    vars.insert("w", w);
    vars.insert("h", h);
    vars.insert("frameW", definition.frame.w);
    vars.insert("frameH", definition.frame.h);
    vars.insert("strokeWidth", stroke_width);
    for (name, parameter) in &definition.parameters {
        let value = overrides
            .get(name)
            .map_or(parameter.default, |v| parameter.clamp(*v));
        vars.insert(name.as_str(), value);
    }
    Env { vars }
}

impl<'a> Evaluator<'a> {
    fn geometry(
        &mut self,
        op: &Op,
        env: &Env<'_>,
        xf: &Xform,
    ) -> Result<Vec<Subpath>, StencilError> {
        let mut path = PathBuilder::new();
        let ev = |e: &Expr, b: &mut Budget| env.eval(e, b);
        match op {
            Op::Rect { x, y, w, h } => {
                let (x, y, w, h) = (
                    ev(x, &mut self.budget)?,
                    ev(y, &mut self.budget)?,
                    ev(w, &mut self.budget)?,
                    ev(h, &mut self.budget)?,
                );
                path.move_to(x, y);
                path.push(xf, x + w, y, &mut self.budget)?;
                path.push(xf, x + w, y + h, &mut self.budget)?;
                path.push(xf, x, y + h, &mut self.budget)?;
                path.close();
            }
            Op::Ellipse { x, y, w, h } => {
                let (x, y, w, h) = (
                    ev(x, &mut self.budget)?,
                    ev(y, &mut self.budget)?,
                    ev(w, &mut self.budget)?,
                    ev(h, &mut self.budget)?,
                );
                let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
                path.move_to(cx + rx, cy);
                for i in 1..ELLIPSE_STEPS {
                    let t = std::f64::consts::TAU * i as f64 / ELLIPSE_STEPS as f64;
                    path.push(xf, cx + rx * t.cos(), cy + ry * t.sin(), &mut self.budget)?;
                }
                path.close();
            }
            Op::Roundrect {
                x,
                y,
                w,
                h,
                arcsize,
            } => {
                let (x, y, w, h) = (
                    ev(x, &mut self.budget)?,
                    ev(y, &mut self.budget)?,
                    ev(w, &mut self.budget)?,
                    ev(h, &mut self.budget)?,
                );
                let percent = match arcsize {
                    Some(e) => ev(e, &mut self.budget)?,
                    None => 0.0,
                };
                let percent = if percent == 0.0 { 15.0 } else { percent };
                // draw.io computes the radius from the scaled size, in instance units.
                let (iw, ih) = ((w * xf.sx).abs(), (h * xf.sy).abs());
                let r = (iw * percent / 100.0).min(ih * percent / 100.0).max(0.0);
                let r = r.min(iw / 2.0).min(ih / 2.0);
                let (rx, ry) = (
                    if xf.sx != 0.0 { r / xf.sx.abs() } else { 0.0 },
                    if xf.sy != 0.0 { r / xf.sy.abs() } else { 0.0 },
                );
                let corners = [
                    (x + w - rx, y + ry, -90.0),
                    (x + w - rx, y + h - ry, 0.0),
                    (x + rx, y + h - ry, 90.0),
                    (x + rx, y + ry, 180.0),
                ];
                path.move_to(x + rx, y);
                for (cx, cy, start) in corners {
                    for i in 0..=CORNER_STEPS {
                        let t = (start + 90.0 * i as f64 / CORNER_STEPS as f64).to_radians();
                        path.push(xf, cx + rx * t.cos(), cy + ry * t.sin(), &mut self.budget)?;
                    }
                }
                path.close();
            }
            Op::Path { commands } => {
                for command in commands {
                    self.budget.step(1)?;
                    match command {
                        PathCmd::Move { x, y } => {
                            let p = (ev(x, &mut self.budget)?, ev(y, &mut self.budget)?);
                            path.move_to(p.0, p.1);
                        }
                        PathCmd::Line { x, y } => {
                            let p = (ev(x, &mut self.budget)?, ev(y, &mut self.budget)?);
                            path.push(xf, p.0, p.1, &mut self.budget)?;
                        }
                        PathCmd::Quad { x1, y1, x2, y2 } => {
                            let c = (ev(x1, &mut self.budget)?, ev(y1, &mut self.budget)?);
                            let e = (ev(x2, &mut self.budget)?, ev(y2, &mut self.budget)?);
                            let s = path.cursor;
                            for i in 1..=QUAD_STEPS {
                                let t = i as f64 / QUAD_STEPS as f64;
                                let u = 1.0 - t;
                                path.push(
                                    xf,
                                    u * u * s.0 + 2.0 * u * t * c.0 + t * t * e.0,
                                    u * u * s.1 + 2.0 * u * t * c.1 + t * t * e.1,
                                    &mut self.budget,
                                )?;
                            }
                        }
                        PathCmd::Curve {
                            x1,
                            y1,
                            x2,
                            y2,
                            x3,
                            y3,
                        } => {
                            let c1 = (ev(x1, &mut self.budget)?, ev(y1, &mut self.budget)?);
                            let c2 = (ev(x2, &mut self.budget)?, ev(y2, &mut self.budget)?);
                            let e = (ev(x3, &mut self.budget)?, ev(y3, &mut self.budget)?);
                            let s = path.cursor;
                            for i in 1..=CURVE_STEPS {
                                let t = i as f64 / CURVE_STEPS as f64;
                                let u = 1.0 - t;
                                let (a, b, c, d) =
                                    (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                                path.push(
                                    xf,
                                    a * s.0 + b * c1.0 + c * c2.0 + d * e.0,
                                    a * s.1 + b * c1.1 + c * c2.1 + d * e.1,
                                    &mut self.budget,
                                )?;
                            }
                        }
                        PathCmd::Arc {
                            rx,
                            ry,
                            x_axis_rotation,
                            large_arc_flag,
                            sweep_flag,
                            x,
                            y,
                        } => {
                            let rx = ev(rx, &mut self.budget)?;
                            let ry = ev(ry, &mut self.budget)?;
                            let rot = ev(x_axis_rotation, &mut self.budget)?;
                            let large = ev(large_arc_flag, &mut self.budget)? != 0.0;
                            let sweep = ev(sweep_flag, &mut self.budget)? != 0.0;
                            let to = (ev(x, &mut self.budget)?, ev(y, &mut self.budget)?);
                            for (px, py) in arc_points(path.cursor, rx, ry, rot, large, sweep, to) {
                                path.push(xf, px, py, &mut self.budget)?;
                            }
                        }
                        PathCmd::Close => path.close(),
                    }
                }
            }
            _ => {}
        }
        Ok(path.finish())
    }

    fn paint(&mut self, fill: bool, stroke: bool) -> Result<(), StencilError> {
        let Some((subpaths, background)) = self.current.take() else {
            return Ok(());
        };
        let s = &self.state;
        let width = finite_f32(s.stroke_width.max(0.0), "stroke width")?;
        if !(s.alpha.is_finite() && s.fill_alpha.is_finite() && s.stroke_alpha.is_finite()) {
            return Err(StencilError::NonFinite("alpha".into()));
        }
        let dash_pattern = s
            .dash
            .iter()
            .map(|v| finite_f32(*v, "dash pattern"))
            .collect::<Result<Vec<_>, _>>()?;
        let miter_limit = finite_f32(s.miter, "miter limit")?;
        let fill = (fill && s.fill != PaintRef::None).then(|| FillPaint {
            color: s.fill.clone(),
            alpha: (s.alpha * s.fill_alpha).clamp(0.0, 1.0) as f32,
        });
        let stroke = (stroke && s.stroke != PaintRef::None).then(|| StrokePaint {
            color: s.stroke.clone(),
            alpha: (s.alpha * s.stroke_alpha).clamp(0.0, 1.0) as f32,
            width,
            dashed: s.dashed,
            dash_pattern,
            join: s.join,
            cap: s.cap,
            miter_limit,
        });
        if fill.is_some() || stroke.is_some() {
            self.paths.push(PaintedPath {
                subpaths,
                fill,
                stroke,
                background,
            });
        }
        Ok(())
    }

    fn run(
        &mut self,
        ops: &[Op],
        env: &Env<'_>,
        xf: &Xform,
        include_depth: usize,
    ) -> Result<(), StencilError> {
        for op in ops {
            self.budget.step(1)?;
            match op {
                Op::Rect { .. } | Op::Roundrect { .. } | Op::Ellipse { .. } | Op::Path { .. } => {
                    let subpaths = self.geometry(op, env, xf)?;
                    self.current = Some((subpaths, false));
                }
                Op::Fill => self.paint(true, false)?,
                Op::Stroke => self.paint(false, true)?,
                Op::FillStroke => self.paint(true, true)?,
                Op::Save => {
                    if self.stack.len() >= self.budget.limits.max_state_depth {
                        return Err(StencilError::Limit("state stack"));
                    }
                    self.stack.push(self.state.clone());
                }
                Op::Restore => {
                    if let Some(state) = self.stack.pop() {
                        self.state = state;
                    }
                }
                Op::StrokeColor { color } => {
                    self.state.stroke = PaintRef::parse(color, "foreground")
                }
                Op::FillColor { color } => self.state.fill = PaintRef::parse(color, "background"),
                Op::FontColor { color } => self.state.font = PaintRef::parse(color, "foreground"),
                Op::Alpha { value } => self.state.alpha = env.eval(value, &mut self.budget)?,
                Op::FillAlpha { value } => {
                    self.state.fill_alpha = env.eval(value, &mut self.budget)?
                }
                Op::StrokeAlpha { value } => {
                    self.state.stroke_alpha = env.eval(value, &mut self.budget)?
                }
                Op::StrokeWidth { width, fixed } => {
                    let width = env.eval(width, &mut self.budget)?;
                    self.state.stroke_width = if *fixed {
                        width
                    } else {
                        width * self.min_scale
                    };
                }
                Op::Dashed { on } => self.state.dashed = Some(*on),
                Op::DashPattern { pattern } => self.state.dash = pattern.clone(),
                Op::LineJoin { join } => self.state.join = *join,
                Op::LineCap { cap } => self.state.cap = *cap,
                Op::MiterLimit { limit } => self.state.miter = *limit,
                Op::FontSize { size } => self.state.font_size = env.eval(size, &mut self.budget)?,
                Op::FontStyle { style } => self.state.font_style = *style,
                Op::FontFamily { family } => self.state.font_family = Some(family.clone()),
                Op::Text {
                    text,
                    x,
                    y,
                    align,
                    valign,
                    rotation,
                    vertical,
                } => {
                    let (px, py) = xf.apply(
                        env.eval(x, &mut self.budget)?,
                        env.eval(y, &mut self.budget)?,
                    );
                    self.budget.step(text.len())?;
                    let at = finite_vec(px, py)?;
                    let size = finite_f32(self.state.font_size * self.min_scale, "font size")?;
                    self.texts.push(RenderText {
                        text: text.clone(),
                        at,
                        align: *align,
                        valign: *valign,
                        rotation: finite_f32(*rotation, "text rotation")?,
                        vertical: *vertical,
                        size,
                        color: self.state.font.clone(),
                        alpha: self.state.alpha as f32,
                        font_style: self.state.font_style,
                        font_family: self.state.font_family.clone(),
                    });
                }
                Op::Include {
                    definition,
                    x,
                    y,
                    w,
                    h,
                } => {
                    if include_depth >= self.budget.limits.max_include_depth {
                        return Err(StencilError::Limit("include depth"));
                    }
                    let target = self
                        .package
                        .definition(definition)
                        .ok_or_else(|| StencilError::UnknownDefinition(definition.clone()))?;
                    let missing = target.missing_capabilities();
                    if !missing.is_empty() {
                        return Err(StencilError::UnsupportedCapability(missing));
                    }
                    let (bx, by) = xf.apply(
                        env.eval(x, &mut self.budget)?,
                        env.eval(y, &mut self.budget)?,
                    );
                    let bw = env.eval(w, &mut self.budget)? * xf.sx;
                    let bh = env.eval(h, &mut self.budget)? * xf.sy;
                    let style_width = *env.vars.get("strokeWidth").unwrap_or(&1.0);
                    let inner_env = definition_env(target, bw, bh, style_width, &BTreeMap::new());
                    let inner_xf = frame_xform(&target.frame, bx, by, bw, bh);
                    // The included stencil draws with its own scale and stroke
                    // policy; the parent's paint state is restored afterwards.
                    let (inner_scale, inner_width) =
                        frame_paint(&target.frame, &inner_xf, style_width);
                    let saved_state = self.state.clone();
                    let saved_scale = self.min_scale;
                    let saved_stack = std::mem::take(&mut self.stack);
                    self.min_scale = inner_scale;
                    self.state.stroke_width = inner_width;
                    self.current = None;
                    if let Some(background) = &target.background {
                        let subpaths = self.geometry(background, &inner_env, &inner_xf)?;
                        self.current = Some((subpaths, false));
                    }
                    self.run(&target.foreground, &inner_env, &inner_xf, include_depth + 1)?;
                    self.stack = saved_stack;
                    self.state = saved_state;
                    self.min_scale = saved_scale;
                    self.current = None;
                }
                Op::If {
                    cond,
                    then,
                    otherwise,
                } => {
                    let branch = if env.eval(cond, &mut self.budget)? != 0.0 {
                        then
                    } else {
                        otherwise
                    };
                    self.run(branch, env, xf, include_depth)?;
                }
            }
        }
        Ok(())
    }
}

/// Ray from the centre through `target`, intersected with the outline; the
/// farthest hit along the ray (draw.io projects onto the perimeter).
fn project_to_outline(
    outline: &[Subpath],
    center: Vec2,
    target: Vec2,
    budget: &mut Budget,
) -> Result<Option<Vec2>, StencilError> {
    let dir = Vec2::new(target.x - center.x, target.y - center.y);
    if dir.x == 0.0 && dir.y == 0.0 {
        return Ok(None);
    }
    let mut best: Option<f32> = None;
    for subpath in outline {
        let points = &subpath.points;
        let n = points.len();
        let edges = if subpath.closed {
            n
        } else {
            n.saturating_sub(1)
        };
        budget.step(edges)?;
        for i in 0..edges {
            let a = points[i];
            let b = points[(i + 1) % n];
            let e = Vec2::new(b.x - a.x, b.y - a.y);
            let denom = dir.x * e.y - dir.y * e.x;
            if denom.abs() < 1e-9 {
                continue;
            }
            let ac = Vec2::new(a.x - center.x, a.y - center.y);
            let t = (ac.x * e.y - ac.y * e.x) / denom;
            let u = (ac.x * dir.y - ac.y * dir.x) / denom;
            if t > 0.0 && (-1e-4..=1.0 + 1e-4).contains(&u) {
                best = Some(best.map_or(t, |b| b.max(t)));
            }
        }
    }
    Ok(best.map(|t| Vec2::new(center.x + dir.x * t, center.y + dir.y * t)))
}

impl StencilPackage {
    /// Evaluates a definition with the default limits.
    pub fn evaluate(
        &self,
        definition_id: &str,
        instance: &Instance<'_>,
    ) -> Result<StencilRender, StencilError> {
        self.evaluate_with_limits(definition_id, instance, &Limits::DEFAULT)
    }

    pub fn evaluate_with_limits(
        &self,
        definition_id: &str,
        instance: &Instance<'_>,
        limits: &Limits,
    ) -> Result<StencilRender, StencilError> {
        let definition = self
            .definition(definition_id)
            .ok_or_else(|| StencilError::UnknownDefinition(definition_id.to_string()))?;
        let missing = definition.missing_capabilities();
        if !missing.is_empty() {
            return Err(StencilError::UnsupportedCapability(missing));
        }
        let (w, h) = (instance.width, instance.height);
        if !(w.is_finite() && h.is_finite() && w >= 0.0 && h >= 0.0) {
            return Err(StencilError::NonFinite("instance size".into()));
        }
        let style_width = if instance.stroke_width.is_finite() {
            instance.stroke_width.max(0.0)
        } else {
            1.0
        };
        let env = definition_env(definition, w, h, style_width, instance.parameters);
        let xf = frame_xform(&definition.frame, 0.0, 0.0, w, h);
        let (min_scale, base_width) = frame_paint(&definition.frame, &xf, style_width);
        let mut evaluator = Evaluator {
            package: self,
            budget: Budget {
                steps: 0,
                points: 0,
                limits: *limits,
            },
            state: State {
                fill: PaintRef::Style("fill".into()),
                stroke: PaintRef::Style("stroke".into()),
                font: PaintRef::Style("font".into()),
                alpha: 1.0,
                fill_alpha: 1.0,
                stroke_alpha: 1.0,
                stroke_width: base_width,
                dashed: None,
                dash: vec![3.0, 3.0],
                join: LineJoin::Miter,
                cap: LineCap::Flat,
                miter: 10.0,
                font_size: 12.0,
                font_style: 0,
                font_family: None,
            },
            stack: Vec::new(),
            current: None,
            paths: Vec::new(),
            texts: Vec::new(),
            min_scale,
        };
        let mut outline = Vec::new();
        if let Some(background) = &definition.background {
            outline = evaluator.geometry(background, &env, &xf)?;
            evaluator.current = Some((outline.clone(), true));
        }
        evaluator.run(&definition.foreground, &env, &xf, 0)?;

        let size = finite_vec(w, h)?;
        let label = match &definition.label {
            Some(r) => {
                let budget = &mut evaluator.budget;
                let (x, y) = (env.eval(&r.x, budget)?, env.eval(&r.y, budget)?);
                let (rw, rh) = (env.eval(&r.w, budget)?, env.eval(&r.h, budget)?);
                let (ix, iy) = xf.apply(x, y);
                Rect::from_min_size(
                    finite_vec(ix, iy)?,
                    finite_vec((rw * xf.sx).max(0.0), (rh * xf.sy).max(0.0))?,
                )
            }
            None => Rect::from_min_size(Vec2::ZERO, size),
        };
        // draw.io maps constraints through the effective (fixed-aspect) frame.
        let (fx, fy, fw, fh) = match definition.frame.units {
            Units::Frame => (
                xf.tx,
                xf.ty,
                definition.frame.w * xf.sx,
                definition.frame.h * xf.sy,
            ),
            Units::Instance => (0.0, 0.0, w, h),
        };
        let center = finite_vec(fx + fw / 2.0, fy + fh / 2.0)?;
        let fallback;
        let shape: &[Subpath] = if outline.is_empty() {
            let (a, b) = (finite_vec(fx, fy)?, finite_vec(fx + fw, fy + fh)?);
            fallback = [Subpath {
                points: vec![a, Vec2::new(b.x, a.y), b, Vec2::new(a.x, b.y)],
                closed: true,
            }];
            &fallback
        } else {
            &outline
        };
        let mut ports = Vec::with_capacity(definition.connections.len());
        for c in &definition.connections {
            let raw = finite_vec(fx + c.x * fw + c.dx, fy + c.y * fh + c.dy)?;
            let at = if c.perimeter {
                project_to_outline(shape, center, raw, &mut evaluator.budget)?.unwrap_or(raw)
            } else {
                raw
            };
            ports.push(RenderPort {
                name: c.name.clone(),
                at,
                perimeter: c.perimeter,
            });
        }
        let mut handles = Vec::with_capacity(definition.handles.len());
        for handle in &definition.handles {
            let budget = &mut evaluator.budget;
            let (hx, hy) = xf.apply(env.eval(&handle.x, budget)?, env.eval(&handle.y, budget)?);
            handles.push(RenderHandle {
                parameter: handle.parameter.clone(),
                label: handle.label.clone(),
                at: finite_vec(hx, hy)?,
            });
        }
        Ok(StencilRender {
            size,
            paths: evaluator.paths,
            texts: evaluator.texts,
            outline,
            label,
            ports,
            handles,
        })
    }

    /// New, clamped parameter value when handle `index` is dragged to the
    /// instance-local `pointer`. The caller commits it as an undoable edit.
    pub fn drag_handle(
        &self,
        definition_id: &str,
        index: usize,
        instance: &Instance<'_>,
        pointer: Vec2,
    ) -> Result<(String, f64), StencilError> {
        let definition = self
            .definition(definition_id)
            .ok_or_else(|| StencilError::UnknownDefinition(definition_id.to_string()))?;
        let handle = definition
            .handles
            .get(index)
            .ok_or_else(|| invalid(format!("handle {index} does not exist")))?;
        let parameter = definition
            .parameters
            .get(&handle.parameter)
            .ok_or_else(|| invalid("handle parameter is missing"))?;
        let xf = frame_xform(&definition.frame, 0.0, 0.0, instance.width, instance.height);
        let (px, py) = xf.invert(f64::from(pointer.x), f64::from(pointer.y));
        let mut env = definition_env(
            definition,
            instance.width,
            instance.height,
            instance.stroke_width,
            instance.parameters,
        );
        env.vars.insert("px", px);
        env.vars.insert("py", py);
        let mut budget = Budget {
            steps: 0,
            points: 0,
            limits: Limits::DEFAULT,
        };
        let value = env.eval(&handle.value, &mut budget)?;
        Ok((handle.parameter.clone(), parameter.clamp(value)))
    }
}

// ---------------------------------------------------------------------------------------
// Built-in data-only libraries

const BUILTIN_SOURCES: &[&str] = &[include_str!("../stencils/thinoda-general.json")];

/// Built-in packages shipped as JSON data. Invalid sources are rejected by
/// tests; at runtime they are skipped rather than crashing the editor.
pub fn builtin_packages() -> &'static [StencilPackage] {
    static PACKAGES: OnceLock<Vec<StencilPackage>> = OnceLock::new();
    PACKAGES.get_or_init(|| {
        BUILTIN_SOURCES
            .iter()
            .filter_map(|source| StencilPackage::from_json(source).ok())
            .collect()
    })
}

/// Raw JSON of the built-in packages (for export and validation).
pub fn builtin_sources() -> &'static [&'static str] {
    BUILTIN_SOURCES
}

#[cfg(test)]
mod tests;
