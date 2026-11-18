//! draw.io stencil families bundled with Thinoda (see `stencils/drawio/NOTICE`).
//!
//! `catalog.json` lists every bundled package and the `mxgraph.<ns>.<name>`
//! aliases draw.io writes, so lookup and search never decode a package.
//! `packages.bin` concatenates the independently deflated package JSON; a
//! package is inflated and validated on first use and cached afterwards.
//! Both files are produced by `examples/drawio_stencil_bundle.rs`.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use serde::Deserialize;
use thinoda_core::stencil::{Limits, StencilPackage};

/// Catalog format understood by this build.
pub const BUNDLE_FORMAT: u32 = 1;

/// Generic draw.io families drawn by jgraph without third-party marks.
pub const FAMILIES: [&str; 17] = [
    "basic",
    "arrows",
    "flowchart",
    "bpmn",
    "electrical",
    "pid",
    "floorplan",
    "mockup",
    "cabinets",
    "eip",
    "fluid_power",
    "lean_mapping",
    "sitemap",
    "signs",
    "networks",
    "networks2",
    "bootstrap",
];

const CATALOG: &str = include_str!("../stencils/drawio/catalog.json");
const PACKAGES: &[u8] = include_bytes!("../stencils/drawio/packages.bin");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub format: u32,
    pub revision: u32,
    pub upstream: Upstream,
    pub translator_revision: u32,
    pub license: String,
    pub packages: Vec<BundledPackage>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upstream {
    pub product: String,
    pub version: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundledPackage {
    pub id: String,
    pub title: String,
    pub family: String,
    /// Lowercase draw.io namespace, e.g. `mxgraph.electrical.logic_gates`.
    pub namespace: String,
    /// Upstream file below the stencil directory, with its SHA-256.
    pub source: String,
    pub sha256: String,
    pub offset: usize,
    pub length: usize,
    pub inflated: usize,
    pub shapes: Vec<BundledShape>,
}

/// One renderable shape, stored as `[name, definition, title]`.
#[derive(Debug, Deserialize)]
pub struct BundledShape {
    /// Alias suffix after the namespace: draw.io's lowercase name with `_`.
    pub name: String,
    pub definition: String,
    pub title: String,
}

impl BundledPackage {
    /// The draw.io style name of `shape`.
    pub fn alias(&self, shape: &BundledShape) -> String {
        format!("{}.{}", self.namespace, shape.name)
    }
}

/// The catalog plus a lazily filled package cache.
pub struct Bundle {
    catalog: Catalog,
    aliases: HashMap<String, (usize, usize)>,
    ids: HashMap<String, usize>,
    packages: Vec<OnceLock<Option<Arc<StencilPackage>>>>,
}

impl Bundle {
    fn new() -> Self {
        let catalog: Catalog =
            serde_json::from_str(CATALOG).expect("bundled draw.io catalog is valid");
        assert_eq!(catalog.format, BUNDLE_FORMAT, "bundled catalog format");
        let mut aliases = HashMap::new();
        let mut ids = HashMap::new();
        for (index, package) in catalog.packages.iter().enumerate() {
            ids.entry(package.id.clone()).or_insert(index);
            for (shape_index, shape) in package.shapes.iter().enumerate() {
                aliases
                    .entry(package.alias(shape))
                    .or_insert((index, shape_index));
            }
        }
        let packages = catalog.packages.iter().map(|_| OnceLock::new()).collect();
        Self {
            catalog,
            aliases,
            ids,
            packages,
        }
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// Whether `alias` names a bundled shape; never decodes a package.
    pub fn has_alias(&self, alias: &str) -> bool {
        self.aliases.contains_key(alias)
    }

    /// The package and definition drawing a draw.io style name.
    pub fn lookup(&self, alias: &str) -> Option<(Arc<StencilPackage>, &str)> {
        let &(package, shape) = self.aliases.get(alias)?;
        let definition = &self.catalog.packages[package].shapes[shape].definition;
        Some((self.package(package)?, definition))
    }

    /// The bundled package with this id, decoded on first use.
    pub fn package_by_id(&self, id: &str) -> Option<Arc<StencilPackage>> {
        self.package(*self.ids.get(id)?)
    }

    pub fn package(&self, index: usize) -> Option<Arc<StencilPackage>> {
        self.packages
            .get(index)?
            .get_or_init(|| decode(&self.catalog.packages[index]).map(Arc::new))
            .clone()
    }

    /// Number of packages decoded so far.
    pub fn decoded(&self) -> usize {
        self.packages
            .iter()
            .filter(|slot| slot.get().is_some())
            .count()
    }
}

fn decode(entry: &BundledPackage) -> Option<StencilPackage> {
    let end = entry.offset.checked_add(entry.length)?;
    let bytes = PACKAGES.get(entry.offset..end)?;
    let limit = entry.inflated.min(Limits::DEFAULT.max_package_bytes);
    let json = match miniz_oxide::inflate::decompress_to_vec_with_limit(bytes, limit) {
        Ok(json) if json.len() == entry.inflated => json,
        _ => {
            log::error!("bundled draw.io package {} is corrupt", entry.id);
            return None;
        }
    };
    let text = std::str::from_utf8(&json).ok()?;
    match StencilPackage::from_json(text) {
        Ok(package) if package.id == entry.id => Some(package),
        Ok(_) => {
            log::error!("bundled draw.io package {} has a different id", entry.id);
            None
        }
        Err(error) => {
            log::error!("bundled draw.io package {} is invalid: {error}", entry.id);
            None
        }
    }
}

/// The process-wide bundle.
pub fn bundle() -> &'static Bundle {
    static BUNDLE: OnceLock<Bundle> = OnceLock::new();
    BUNDLE.get_or_init(Bundle::new)
}

#[cfg(test)]
mod tests;
