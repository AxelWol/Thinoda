//! Converts draw.io stencil XML (`<shapes>`/`<shape>`, or the compressed
//! payload of an inline `shape=stencil(...)` style) into a declarative
//! [`StencilPackage`]. The conversion is purely structural: every draw.io
//! drawing element maps to a DSL operation, and anything the interpreter cannot
//! draw yet is declared as a definition capability so the definition loads but
//! refuses to render with an explicit diagnostic instead of drawing silently
//! wrong geometry.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

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
/// Revision of the conversion rules; bundled packages record the revision
/// that generated them, and a change must regenerate the bundle.
pub const TRANSLATOR_REVISION: u32 = 1;
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
    let (import, _) = convert_stencils(xml, package_id, title)?;
    import
        .package
        .validate(&thinoda_core::stencil::Limits::DEFAULT)
        .map_err(|e| IoError::Invalid(e.to_string()))?;
    Ok(import)
}

/// A draw.io stencil library file converted into installable packages.
#[derive(Debug)]
pub struct StencilLibraryImport {
    /// One package, or numbered parts when the library exceeds package limits.
    pub packages: Vec<StencilPackage>,
    /// The `<shapes name>` namespace, e.g. `mxgraph.aws4`.
    pub namespace: Option<String>,
    pub diagnostics: Vec<String>,
}

/// Prefix of user-installed draw.io library packages, distinct from the
/// bundled `drawio.<family>` packages so neither can shadow the other.
pub const USER_LIBRARY_PREFIX: &str = "user.drawio.";

/// Converts a whole draw.io stencil library (`<shapes name="mxgraph.x">`) the
/// way draw.io registers it: every shape gets the alias
/// `lowercase(namespace + ".") + lowercase(name with "_")`, recorded in the
/// package's `drawio.import` map and as the definition's native draw.io style.
/// Libraries beyond the package limits (including the compact JSON size) are
/// split into `<id>.part-N` packages that keep each definition with the
/// definitions it includes; store them with `serde_json::to_string`.
pub fn import_drawio_stencil_library(
    xml: &str,
    title: &str,
) -> Result<StencilLibraryImport, IoError> {
    let (import, namespace) = convert_stencils(xml, "user.drawio.library", title)?;
    let mut diagnostics = import.diagnostics;
    let mut package = import.package;
    let local = namespace
        .as_deref()
        .map(|ns| {
            let lower = ns.to_lowercase();
            lower.strip_prefix("mxgraph.").unwrap_or(&lower).to_owned()
        })
        .map(|local| slug(&local))
        .filter(|local| !local.is_empty())
        .unwrap_or_else(|| {
            let title = slug(&package.title.to_lowercase());
            if title.is_empty() {
                "library".into()
            } else {
                title
            }
        });
    package.id = format!("{USER_LIBRARY_PREFIX}{local}");
    let prefix = namespace
        .as_deref()
        .map(|ns| format!("{}.", ns.to_lowercase()))
        .unwrap_or_default();

    // A later shape with the same name replaces the earlier, like draw.io.
    let mut aliases: BTreeMap<String, String> = BTreeMap::new();
    for definition in &mut package.definitions {
        definition.extensions.remove(SOURCE_EXTENSION);
        let Some(name) = definition
            .interchange
            .get("drawio")
            .and_then(|v| v.get("stencilName"))
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let alias = format!("{prefix}{}", name.replace(' ', "_").to_lowercase());
        definition.interchange.insert(
            "drawio".into(),
            serde_json::json!({ "stencilName": name, "style": format!("shape={alias};") }),
        );
        if definition.requires.is_empty() {
            aliases.insert(alias, definition.id.clone());
        }
    }

    let limits = thinoda_core::stencil::Limits::DEFAULT;
    let with_aliases = |mut package: StencilPackage| {
        let map: serde_json::Map<String, Value> = aliases
            .iter()
            .filter(|(_, id)| package.definition(id).is_some())
            .map(|(alias, id)| (alias.clone(), Value::String(id.clone())))
            .collect();
        package
            .extensions
            .insert(IMPORT_EXTENSION.into(), Value::Object(map));
        package
    };
    // Installed packages reload through `from_json`, which also bounds the
    // compact serialized size that `validate` does not check.
    let fits = |package: &StencilPackage| -> Result<(), String> {
        package.validate(&limits).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(package).map_err(|e| e.to_string())?;
        if bytes.len() > limits.max_package_bytes {
            return Err("package size limit".into());
        }
        Ok(())
    };
    let whole = with_aliases(package.clone());
    if fits(&whole).is_ok() {
        return Ok(StencilLibraryImport {
            packages: vec![whole],
            namespace,
            diagnostics,
        });
    }

    let mut parts = Vec::new();
    let mut pending = vec![include_groups(std::mem::take(&mut package.definitions))];
    while let Some(groups) = pending.pop() {
        let mut part = package.clone();
        part.definitions = groups.iter().flatten().cloned().collect();
        let part = with_aliases(part);
        match fits(&part) {
            Ok(()) => parts.push(part),
            Err(_) if groups.len() > 1 => {
                let mut groups = groups;
                let second = groups.split_off(groups.len() / 2);
                pending.push(second);
                pending.push(groups);
            }
            Err(error) => diagnostics.push(format!(
                "{}: skipped, exceeds package limits: {error}",
                groups[0][0].id
            )),
        }
    }
    if parts.is_empty() {
        return Err(IoError::Invalid(
            "no shape of the draw.io stencil library fits the package limits".into(),
        ));
    }
    let count = parts.len();
    for (index, part) in parts.iter_mut().enumerate() {
        part.id = format!("{}.part-{}", package.id, index + 1);
        part.title = format!("{} ({}/{count})", package.title, index + 1);
    }
    Ok(StencilLibraryImport {
        packages: parts,
        namespace,
        diagnostics,
    })
}

/// Groups definitions so each group holds every definition it includes;
/// groups keep the library order of their first definition.
fn include_groups(definitions: Vec<StencilDefinition>) -> Vec<Vec<StencilDefinition>> {
    let index: HashMap<String, usize> = definitions
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id.clone(), i))
        .collect();
    let mut parent: Vec<usize> = (0..definitions.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (i, definition) in definitions.iter().enumerate() {
        let mut includes = Vec::new();
        collect_includes(definition.background.iter(), &mut includes);
        collect_includes(definition.foreground.iter(), &mut includes);
        for include in includes {
            if let Some(&j) = index.get(include) {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                parent[a.max(b)] = a.min(b);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<StencilDefinition>> = BTreeMap::new();
    for (i, definition) in definitions.into_iter().enumerate() {
        let r = root(&mut parent, i);
        groups.entry(r).or_default().push(definition);
    }
    groups.into_values().collect()
}

fn collect_includes<'a>(ops: impl Iterator<Item = &'a Op>, out: &mut Vec<&'a str>) {
    for op in ops {
        match op {
            Op::Include { definition, .. } => out.push(definition),
            Op::If {
                then, otherwise, ..
            } => {
                collect_includes(then.iter(), out);
                collect_includes(otherwise.iter(), out);
            }
            _ => {}
        }
    }
}

/// Converts without validating limits; also returns the `<shapes>` name.
fn convert_stencils(
    xml: &str,
    package_id: &str,
    title: &str,
) -> Result<(StencilImport, Option<String>), IoError> {
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
    let mut shape_ids = Vec::with_capacity(shapes.len());
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
        // Includes resolve to the first shape carrying a name, like draw.io.
        ids.entry(name.to_string()).or_insert_with(|| id.clone());
        shape_ids.push(id);
    }

    let mut diagnostics = Vec::new();
    let mut definitions = Vec::with_capacity(shapes.len());
    for (shape, id) in shapes.iter().zip(shape_ids) {
        let name = shape.attr("name").unwrap_or_default();
        let mut cx = Convert {
            ids: &ids,
            requires: BTreeSet::new(),
            notes: Vec::new(),
            hoisted: Vec::new(),
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
    let namespace = (root.name == "shapes")
        .then(|| root.attr("name").map(str::to_owned))
        .flatten();
    Ok((
        StencilImport {
            package,
            diagnostics,
        },
        namespace,
    ))
}

fn is_geometry(name: &str) -> bool {
    matches!(name, "rect" | "roundrect" | "ellipse" | "path")
}

fn is_paint(name: &str) -> bool {
    matches!(name, "fill" | "stroke" | "fillstroke")
}

fn is_path_command(name: &str) -> bool {
    matches!(name, "move" | "line" | "quad" | "curve" | "arc" | "close")
}

/// Canvas state setters. draw.io applies them when a path is painted, so one
/// that appears inside a `<path>` is equivalent to one placed before it.
fn is_state(name: &str) -> bool {
    matches!(
        name,
        "strokecolor"
            | "fillcolor"
            | "fontcolor"
            | "alpha"
            | "fillalpha"
            | "strokealpha"
            | "strokewidth"
            | "dashed"
            | "dashpattern"
            | "linejoin"
            | "linecap"
            | "miterlimit"
            | "fontsize"
            | "fontstyle"
            | "fontfamily"
    )
}

/// Elements draw.io's renderer draws or applies; it ignores every other name.
fn is_known(name: &str) -> bool {
    is_geometry(name)
        || is_paint(name)
        || is_path_command(name)
        || is_state(name)
        || matches!(
            name,
            "save" | "restore" | "text" | "image" | "include-shape"
        )
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
    /// State setters found inside the last converted `<path>`, to be emitted
    /// before it.
    hoisted: Vec<Op>,
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

        // draw.io runs <background> as ordinary instructions before
        // <foreground>; its first geometry is the outline. Thinoda seeds the
        // outline as the current path, so the remaining background
        // instructions (state, paint, later geometry) become a foreground
        // prelude that keeps draw.io's execution order.
        let mut background = None;
        let mut foreground = Vec::new();
        if let Some(bg) = shape.child("background") {
            let children = &bg.children;
            let outline = children.iter().position(|c| is_geometry(&c.name));
            let split = outline.unwrap_or(children.len());
            if children[..split].iter().any(|c| is_paint(&c.name)) {
                self.notes
                    .push("background paints before its outline geometry".into());
            }
            foreground.extend(self.ops(&children[..split]));
            if let Some(index) = outline {
                background = self.geometry(&children[index]);
                foreground.append(&mut self.hoisted);
                let rest = &children[index + 1..];
                if rest.iter().any(|c| is_geometry(&c.name)) {
                    self.notes.push(
                        "background has extra geometry; only the first is the outline".into(),
                    );
                }
                foreground.extend(self.ops(rest));
            }
        }
        if let Some(fg) = shape.child("foreground") {
            foreground.extend(self.ops(&fg.children));
        }

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
            "path" => self.path(e),
            _ => return None,
        })
    }

    /// Converts a `<path>`. draw.io draws its children in the same canvas:
    /// state setters apply at paint time and are hoisted before the path, and
    /// a shape element replaces the path being built, as it does in draw.io.
    fn path(&mut self, e: &Element) -> Op {
        if e.attr("rounded") == Some("1") {
            self.notes
                .push("rounded path corners are drawn sharp".into());
        }
        let mut commands = Vec::new();
        let mut replaced: Option<Op> = None;
        for c in &e.children {
            let name = c.name.as_str();
            if is_state(name) {
                let state = self.ops(std::slice::from_ref(c));
                self.hoisted.extend(state);
                continue;
            }
            if is_geometry(name) {
                self.notes.push(format!(
                    "<{name}> inside a path replaces the path, as in draw.io"
                ));
                let hoisted = std::mem::take(&mut self.hoisted);
                replaced = self.geometry(c);
                self.hoisted.splice(0..0, hoisted);
                commands.clear();
                continue;
            }
            if replaced.is_some() && is_path_command(name) {
                continue;
            }
            if !is_known(name) {
                self.ignored(name);
                continue;
            }
            let n = |cx: &mut Self, key| Expr::Num(cx.num(c, key));
            let command = match name {
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
        if let Some(op) = replaced {
            return op;
        }
        if commands.is_empty() {
            commands.push(PathCmd::Move {
                x: Expr::Num(0.0),
                y: Expr::Num(0.0),
            });
        }
        Op::Path { commands }
    }

    /// A `move`/`line`/... outside `<path>` extends the open path in draw.io,
    /// or is ignored when no path is open.
    fn stray_command(&mut self, e: &Element, open: Option<&mut Op>) {
        let Some(Op::Path { commands }) = open else {
            self.notes.push(format!(
                "<{}> outside a path is ignored, as in draw.io",
                e.name
            ));
            return;
        };
        let n = |cx: &mut Self, key| Expr::Num(cx.num(e, key));
        commands.push(match e.name.as_str() {
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
            _ => PathCmd::Close,
        });
    }

    fn ops(&mut self, elements: &[Element]) -> Vec<Op> {
        let mut ops = Vec::with_capacity(elements.len());
        // Index of the path that later stray path commands extend.
        let mut open: Option<usize> = None;
        for e in elements {
            if let Some(geometry) = self.geometry(e) {
                ops.append(&mut self.hoisted);
                open = matches!(geometry, Op::Path { .. }).then_some(ops.len());
                ops.push(geometry);
                continue;
            }
            if is_path_command(&e.name) {
                let target = open.and_then(|i| ops.get_mut(i));
                self.stray_command(e, target);
                continue;
            }
            if is_paint(&e.name) || e.name == "include-shape" {
                open = None;
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
                // draw.io applies fillalpha and strokealpha as the global
                // alpha; matching it keeps converted shapes looking the same.
                "alpha" | "fillalpha" | "strokealpha" => Op::Alpha {
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
                "image" => {
                    self.unsupported("image");
                    continue;
                }
                other => {
                    self.ignored(other);
                    continue;
                }
            };
            ops.push(op);
        }
        ops
    }

    /// draw.io's renderer ignores elements it does not know; so does the
    /// conversion, with a diagnostic.
    fn ignored(&mut self, element: &str) {
        self.notes.push(format!(
            "ignored unknown element <{element}>, as in draw.io"
        ));
    }

    /// Declares an element draw.io draws but the interpreter cannot, so the
    /// definition refuses to render instead of silently dropping it.
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

/// draw.io style keys whose value the stencil reference owns: the definition's
/// native style keys and its bound parameter keys.
pub(crate) fn bound_style_keys(stencil: &StencilRef) -> HashSet<String> {
    let mut keys = HashSet::new();
    let Some((_, definition)) = stencil.resolve() else {
        return keys;
    };
    let Some(binding) = drawio_binding(definition) else {
        return keys;
    };
    if let Some(style) = binding.get("style").and_then(Value::as_str) {
        keys.extend(
            style
                .split(';')
                .filter_map(|part| part.split_once('='))
                .map(|(key, _)| key.trim().to_owned()),
        );
    }
    if let Some(bindings) = binding.get("parameters").and_then(Value::as_object) {
        for binding in bindings.values() {
            match binding {
                Value::String(key) => {
                    keys.insert(key.clone());
                }
                Value::Object(choice) => {
                    if let Some(key) = choice.get("style").and_then(Value::as_str) {
                        keys.insert(key.to_owned());
                    }
                }
                _ => {}
            }
        }
    }
    keys
}

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
fn explicit_reference(entries: &BTreeMap<&str, &str>, shape: &str) -> Option<StencilRef> {
    let mut stencil = StencilRef::new(
        *entries.get(STYLE_PACKAGE_KEY)?,
        *entries.get(STYLE_DEFINITION_KEY)?,
    );
    read_parameter_overrides(&mut stencil, entries);
    apply_explicit_bound_parameters(&mut stencil, entries, shape);
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
    preserve_missing: bool,
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
                        let style_key = match binding {
                            Value::String(key) => Some(key.as_str()),
                            Value::Object(choice) => choice.get("style").and_then(Value::as_str),
                            _ => None,
                        }?;
                        if preserve_missing && !entries.contains_key(style_key) {
                            return None;
                        }
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
                        let absent = if preserve_missing && !binding.is_object() {
                            None
                        } else {
                            absent
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

fn apply_explicit_bound_parameters(
    stencil: &mut StencilRef,
    entries: &BTreeMap<&str, &str>,
    shape: &str,
) {
    let absent = import_index().get(shape).and_then(|target| {
        (target.package.id == stencil.package && target.definition == stencil.definition)
            .then_some(target.absent)
            .flatten()
    });
    apply_bound_parameters(stencil, entries, absent, true);
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

#[derive(Default)]
struct UserRegistry {
    packages: HashMap<String, Arc<StencilPackage>>,
    aliases: HashMap<String, (Arc<StencilPackage>, String)>,
}

fn user_registry() -> &'static RwLock<UserRegistry> {
    static REGISTRY: OnceLock<RwLock<UserRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// Replaces the process-wide set of user-installed stencil packages. Their
/// `drawio.import` aliases resolve plain draw.io shapes after the built-in
/// and bundled packages, and their ids resolve explicit Thinoda references,
/// so diagrams using an installed vendor library open as stencils.
pub fn set_user_stencil_packages(packages: impl IntoIterator<Item = Arc<StencilPackage>>) {
    let mut registry = UserRegistry::default();
    for package in packages {
        if registry.packages.contains_key(&package.id) {
            continue;
        }
        if let Some(map) = package
            .extensions
            .get(IMPORT_EXTENSION)
            .and_then(Value::as_object)
        {
            for (shape, target) in map {
                let Some(definition) = target.as_str() else {
                    continue;
                };
                if package.definition(definition).is_some() {
                    // Earlier packages win, like the built-in index.
                    registry
                        .aliases
                        .entry(shape.clone())
                        .or_insert_with(|| (Arc::clone(&package), definition.to_owned()));
                }
            }
        }
        registry.packages.insert(package.id.clone(), package);
    }
    if let Ok(mut current) = user_registry().write() {
        *current = registry;
    }
}

fn user_alias(shape: &str) -> Option<(Arc<StencilPackage>, String)> {
    user_registry().read().ok()?.aliases.get(shape).cloned()
}

fn user_package(id: &str) -> Option<Arc<StencilPackage>> {
    user_registry().read().ok()?.packages.get(id).cloned()
}

/// Whether a built-in package, the bundled draw.io families, an installed
/// user library or the shape's inline payload define the draw.io shape name.
/// Never decodes a package.
pub fn maps_drawio_shape(shape: &str) -> bool {
    maps_builtin_drawio_shape(shape)
        || crate::drawio_bundle::bundle().has_alias(shape)
        || user_alias(shape).is_some()
}

/// Whether a built-in package or the shape's inline payload defines the
/// draw.io shape name, ignoring the bundled draw.io families.
pub fn maps_builtin_drawio_shape(shape: &str) -> bool {
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
/// same style draws it, then a bundled draw.io package of that id, then an
/// installed user library. With `implicit`, a plain draw.io shape name
/// resolves through its inline stencil payload, the built-in `drawio.import`
/// maps, the bundled families or the installed user libraries.
pub(crate) fn style_reference<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
    shape: &str,
    implicit: bool,
) -> Option<StencilRef> {
    let entries: BTreeMap<&str, &str> = entries.into_iter().collect();
    if entries.contains_key(STYLE_PACKAGE_KEY) {
        let explicit = explicit_reference(&entries, shape)?;
        if explicit.resolve().is_some() {
            return Some(explicit);
        }
        let fallback = inline_package(shape)
            .filter(|package| package.id == explicit.package)
            .or_else(|| crate::drawio_bundle::bundle().package_by_id(&explicit.package))
            .or_else(|| user_package(&explicit.package));
        if let Some(package) = fallback
            && package.definition(&explicit.definition).is_some()
        {
            let mut stencil = StencilRef::with_package(package, explicit.definition.clone());
            stencil.parameters = explicit.parameters.clone();
            apply_explicit_bound_parameters(&mut stencil, &entries, shape);
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
    } else if let Some(target) = import_index().get(shape) {
        let mut stencil =
            StencilRef::with_package(Arc::clone(&target.package), target.definition.clone());
        apply_bound_parameters(&mut stencil, &entries, target.absent, false);
        stencil
    } else {
        let (package, definition) = crate::drawio_bundle::bundle()
            .lookup(shape)
            .map(|(package, definition)| (package, definition.to_owned()))
            .or_else(|| user_alias(shape))?;
        StencilRef::with_package(package, definition)
    };
    stencil.validate().ok()?;
    stencil.resolve()?;
    Some(stencil)
}

#[cfg(test)]
mod tests;
