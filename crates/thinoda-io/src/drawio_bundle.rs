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
const MAX_INDEXED_ALIAS_BYTES: usize = 4 * 1024 * 1024;

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

#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
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
        let catalog = parse_catalog(CATALOG.as_bytes()).expect("bundled draw.io catalog is valid");
        assert_eq!(catalog.format, BUNDLE_FORMAT, "bundled catalog format");
        Self::from_catalog(catalog).expect("bundled draw.io catalog index is within limits")
    }

    fn from_catalog(catalog: Catalog) -> Option<Self> {
        let mut alias_bytes = 0usize;
        for package in &catalog.packages {
            for shape in &package.shapes {
                let next = alias_bytes
                    .checked_add(package.namespace.len())
                    .and_then(|bytes| bytes.checked_add(1))
                    .and_then(|bytes| bytes.checked_add(shape.name.len()));
                let Some(next) = next.filter(|&bytes| bytes <= MAX_INDEXED_ALIAS_BYTES) else {
                    log::error!("bundled draw.io catalog alias index exceeds its byte limit");
                    return None;
                };
                alias_bytes = next;
            }
        }
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
        Some(Self {
            catalog,
            aliases,
            ids,
            packages,
        })
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
            .get_or_init(|| decode(&self.catalog.packages[index], PACKAGES).map(Arc::new))
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

fn parse_catalog(bytes: &[u8]) -> Result<Catalog, serde_json::Error> {
    serde_json::from_slice(bytes)
}

fn decode(entry: &BundledPackage, blob: &[u8]) -> Option<StencilPackage> {
    let Some(end) = entry.offset.checked_add(entry.length) else {
        log::error!(
            "bundled draw.io package {} has an overflowing range",
            entry.id
        );
        return None;
    };
    let Some(bytes) = blob.get(entry.offset..end) else {
        log::error!("bundled draw.io package {} is outside the blob", entry.id);
        return None;
    };
    let limit = entry.inflated.min(Limits::DEFAULT.max_package_bytes);
    let json = match miniz_oxide::inflate::decompress_to_vec_with_limit(bytes, limit) {
        Ok(json) if json.len() == entry.inflated => json,
        _ => {
            log::error!("bundled draw.io package {} is corrupt", entry.id);
            return None;
        }
    };
    let Ok(text) = std::str::from_utf8(&json) else {
        log::error!("bundled draw.io package {} is not UTF-8", entry.id);
        return None;
    };
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

/// Internal entry points for the separate fuzzing workspace.
#[cfg(any(test, feature = "fuzzing"))]
#[doc(hidden)]
pub fn fuzzing_catalog(bytes: &[u8]) -> Option<Bundle> {
    let catalog = parse_catalog(bytes).ok()?;
    if catalog.format != BUNDLE_FORMAT {
        return None;
    }
    Bundle::from_catalog(catalog)
}

#[cfg(any(test, feature = "fuzzing"))]
#[doc(hidden)]
pub fn fuzzing_decode(entry: &BundledPackage, blob: &[u8]) -> Option<StencilPackage> {
    decode(entry, blob)
}

/// The process-wide bundle.
pub fn bundle() -> &'static Bundle {
    static BUNDLE: OnceLock<Bundle> = OnceLock::new();
    BUNDLE.get_or_init(Bundle::new)
}

#[cfg(test)]
mod tests;
