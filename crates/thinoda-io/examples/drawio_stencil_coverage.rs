//! Measures how many draw.io XML stencils convert and render through the
//! shared JSON stencil interpreter.
//!
//! Usage: `drawio_stencil_coverage <stencil-xml-directory> [--json]`
//!
//! The directory holds draw.io `<shapes>` XML files, in any sub-folders (for
//! example a checkout of `src/main/webapp/stencils`). The report counts,
//! per file, the shapes converted into definitions, the definitions that
//! evaluate successfully, and the reasons for the remainder. Files that fail
//! as a whole (for example over a package limit) are reported separately,
//! with the number of shapes they contain.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use thinoda_core::stencil::Instance;
use thinoda_io::drawio_stencil::import_drawio_stencils;

#[derive(Default)]
struct Tally {
    files: usize,
    shapes: usize,
    rendered: usize,
    failed_files: usize,
    /// Shapes in files that failed as a whole; not part of `shapes`.
    failed_file_shapes: usize,
    reasons: BTreeMap<String, usize>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let Some(dir) = args.next().map(PathBuf::from) else {
        return Err("Usage: drawio_stencil_coverage <stencil-xml-directory> [--json]".into());
    };
    let json = args.next().is_some_and(|flag| flag == "--json");
    let mut files = Vec::new();
    collect(&dir, &mut files)?;
    files.sort();

    let mut total = Tally::default();
    let mut families: BTreeMap<String, Tally> = BTreeMap::new();
    for path in &files {
        let relative = path.strip_prefix(&dir).unwrap_or(path);
        let family = relative
            .components()
            .next()
            .map(|c| {
                c.as_os_str()
                    .to_string_lossy()
                    .trim_end_matches(".xml")
                    .to_string()
            })
            .unwrap_or_default();
        let tally = families.entry(family).or_default();
        let xml = fs::read_to_string(path)?;
        tally.files += 1;
        total.files += 1;
        let id = format!("probe.{}", total.files);
        let import = match import_drawio_stencils(&xml, &id, "probe") {
            Ok(import) => import,
            Err(error) => {
                let shapes = xml.matches("<shape ").count();
                tally.failed_files += 1;
                total.failed_files += 1;
                tally.failed_file_shapes += shapes;
                total.failed_file_shapes += shapes;
                *total.reasons.entry(format!("file: {error}")).or_default() += 1;
                continue;
            }
        };
        let empty = BTreeMap::new();
        for definition in &import.package.definitions {
            tally.shapes += 1;
            total.shapes += 1;
            let [w, h] = definition.default_size();
            let instance = Instance {
                width: w.max(1.0),
                height: h.max(1.0),
                stroke_width: 1.0,
                parameters: &empty,
            };
            match import.package.evaluate(&definition.id, &instance) {
                Ok(render) if !render.paths.is_empty() || !render.texts.is_empty() => {
                    tally.rendered += 1;
                    total.rendered += 1;
                }
                Ok(_) => {
                    *total.reasons.entry("renders nothing".into()).or_default() += 1;
                    *tally.reasons.entry("renders nothing".into()).or_default() += 1;
                }
                Err(error) => {
                    let reason = reason(&error.to_string());
                    *total.reasons.entry(reason.clone()).or_default() += 1;
                    *tally.reasons.entry(reason).or_default() += 1;
                }
            }
        }
    }

    if json {
        let families: BTreeMap<_, _> = families
            .iter()
            .map(|(name, t)| {
                (
                    name.clone(),
                    serde_json::json!({
                        "files": t.files,
                        "shapes": t.shapes,
                        "rendered": t.rendered,
                        "failedFiles": t.failed_files,
                        "failedFileShapes": t.failed_file_shapes,
                        "reasons": t.reasons,
                    }),
                )
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "files": total.files,
                "failedFiles": total.failed_files,
                "failedFileShapes": total.failed_file_shapes,
                "shapes": total.shapes,
                "rendered": total.rendered,
                "reasons": total.reasons,
                "families": families,
            }))?
        );
    } else {
        println!("family                 files  shapes rendered");
        for (name, t) in &families {
            println!(
                "{name:<22} {:>5} {:>7} {:>8}",
                t.files, t.shapes, t.rendered
            );
        }
        println!(
            "TOTAL                  {:>5} {:>7} {:>8}  ({} files with {} shapes failed as a whole)",
            total.files, total.shapes, total.rendered, total.failed_files, total.failed_file_shapes
        );
        let unique: BTreeSet<_> = total.reasons.iter().collect();
        for (reason, count) in unique {
            println!("  {count:>6}  {reason}");
        }
    }
    Ok(())
}

fn reason(message: &str) -> String {
    // Group messages that differ only in definition-specific detail.
    message.split(" in '").next().unwrap_or(message).to_string()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "xml") {
            out.push(path);
        }
    }
    Ok(())
}
