//! Generates the bundled draw.io stencil families shipped by thinoda-io.
//!
//! ```text
//! cargo run -p thinoda-io --example drawio_stencil_bundle -- <xml-dir> [<out-dir>]
//! ```
//!
//! `<xml-dir>` holds draw.io's decoded stencil XML (`basic.xml`,
//! `electrical/logic_gates.xml`, ...) from draw.io [`UPSTREAM`]. Each source
//! file becomes one package `drawio.<namespace>`; packages are deflated
//! independently and concatenated into `packages.bin`, and `catalog.json`
//! lists them with their shape aliases so lookup and search never decode a
//! package. The output is deterministic: files are sorted, serialization is
//! compact and the deflate level is fixed. Regenerating with a different
//! upstream or translator must bump [`BUNDLE_REVISION`] (see docs/DRAWIO.md).

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thinoda_core::stencil::Limits;
use thinoda_io::drawio_bundle::{BUNDLE_FORMAT, FAMILIES};
use thinoda_io::drawio_stencil::{SOURCE_EXTENSION, TRANSLATOR_REVISION, import_drawio_stencils};

/// draw.io release the stencil XML was taken from.
const UPSTREAM: &str = "32.6.10";
const BUNDLE_REVISION: u32 = 1;
const LICENSE: &str = "Apache-2.0";
const DEFLATE_LEVEL: u8 = 9;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (root, out) = match args.as_slice() {
        [root] => (PathBuf::from(root), default_out()),
        [root, out] => (PathBuf::from(root), PathBuf::from(out)),
        _ => {
            eprintln!("usage: drawio_stencil_bundle <xml-dir> [<out-dir>]");
            std::process::exit(2);
        }
    };
    std::fs::create_dir_all(&out).expect("create output directory");
    let mut blob = Vec::new();
    let mut packages = Vec::new();
    let mut shapes = 0usize;
    for family in FAMILIES {
        for file in family_files(&root, family) {
            let source = file
                .strip_prefix(&root)
                .expect("file under root")
                .to_string_lossy()
                .replace('\\', "/");
            let Some(entry) = convert(&file, &source, family, &mut blob) else {
                std::process::exit(1);
            };
            shapes += entry["shapes"].as_array().map_or(0, Vec::len);
            println!(
                "{:48} {:5} {:9}",
                entry["id"].as_str().unwrap_or_default(),
                entry["shapes"].as_array().map_or(0, Vec::len),
                entry["length"]
            );
            packages.push(entry);
        }
    }
    let catalog = json!({
        "format": BUNDLE_FORMAT,
        "revision": BUNDLE_REVISION,
        "upstream": { "product": "draw.io", "version": UPSTREAM, "path": "src/main/webapp/stencils" },
        "translatorRevision": TRANSLATOR_REVISION,
        "license": LICENSE,
        "packages": packages,
    });
    let text = serde_json::to_string(&catalog).expect("serialize catalog") + "\n";
    std::fs::write(out.join("catalog.json"), &text).expect("write catalog");
    std::fs::write(out.join("packages.bin"), &blob).expect("write packages");
    println!(
        "TOTAL {} packages, {shapes} shapes, catalog {} bytes, packages {} bytes",
        packages.len(),
        text.len(),
        blob.len()
    );
}

fn default_out() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("stencils/drawio")
}

fn family_files(root: &Path, family: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let single = root.join(format!("{family}.xml"));
    if single.is_file() {
        files.push(single);
    }
    if let Ok(entries) = std::fs::read_dir(root.join(family)) {
        let mut nested: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "xml"))
            .collect();
        nested.sort();
        files.extend(nested);
    }
    if files.is_empty() {
        eprintln!("family {family}: no stencil files under {}", root.display());
        std::process::exit(1);
    }
    files
}

/// The `name` attribute of the `<shapes>` root, as draw.io reads it.
fn namespace(xml: &str) -> Option<String> {
    let start = xml.find("<shapes")?;
    let tag = &xml[start..start + xml[start..].find('>')?];
    let at = tag.find("name=\"")? + 6;
    Some(tag[at..at + tag[at..].find('"')?].to_owned())
}

fn convert(file: &Path, source: &str, family: &str, blob: &mut Vec<u8>) -> Option<Value> {
    let bytes = std::fs::read(file).ok()?;
    let xml = String::from_utf8(bytes.clone()).ok()?;
    let Some(ns) = namespace(&xml).filter(|ns| ns.to_lowercase().starts_with("mxgraph.")) else {
        eprintln!("{source}: no mxgraph namespace");
        return None;
    };
    // draw.io registers `lowercase(namespace + ".") + lowercase(name with "_")`.
    let prefix = format!("{}.", ns.to_lowercase());
    let local = &prefix["mxgraph.".len()..prefix.len() - 1];
    let id = format!("drawio.{local}");
    let title = local
        .split('.')
        .map(title_case)
        .collect::<Vec<_>>()
        .join(" / ");
    let import = match import_drawio_stencils(&xml, &id, &title) {
        Ok(import) => import,
        Err(error) => {
            eprintln!("{source}: {error}");
            return None;
        }
    };
    let mut package = import.package;
    package.license = Some(LICENSE.into());
    package.revision = BUNDLE_REVISION;
    let mut aliases: Vec<(String, String, String)> = Vec::new();
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
            json!({ "stencilName": name, "style": format!("shape={alias};") }),
        );
        if !definition.requires.is_empty() {
            continue;
        }
        // A later shape with the same name replaces the earlier, like draw.io.
        aliases.retain(|(existing, _, _)| *existing != alias);
        aliases.push((alias, definition.id.clone(), definition.title.clone()));
    }
    if let Err(error) = package.validate(&Limits::DEFAULT) {
        eprintln!("{source}: package {id} exceeds limits: {error}");
        return None;
    }
    aliases.sort();
    let json = serde_json::to_vec(&package).ok()?;
    let deflated = miniz_oxide::deflate::compress_to_vec(&json, DEFLATE_LEVEL);
    let offset = blob.len();
    blob.extend_from_slice(&deflated);
    let shapes: Vec<Value> = aliases
        .into_iter()
        .map(|(alias, definition, title)| json!([&alias[prefix.len()..], definition, title]))
        .collect();
    Some(json!({
        "id": id,
        "title": title,
        "family": family,
        "namespace": prefix.trim_end_matches('.'),
        "source": source,
        "sha256": hex(&Sha256::digest(&bytes)),
        "offset": offset,
        "length": deflated.len(),
        "inflated": json.len(),
        "shapes": shapes,
    }))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn title_case(word: &str) -> String {
    word.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
