use std::{
    collections::HashSet,
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};
use thinoda_io::{ExportOptions, Format, export_bytes, import_file_with_report};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 2 {
        return Err("Usage: drawio_visual_probe <example-directory> <output-directory>".into());
    }
    probe(&args[0], &args[1])
}

fn reference_page(name: &str, path: &Path) -> Option<usize> {
    if !path.is_file() {
        return None;
    }
    let filename = path.file_name()?.to_str()?;
    if filename == format!("{name}.png") {
        return Some(0);
    }
    let index = filename
        .strip_prefix(&format!("{name}.page-"))?
        .strip_suffix(".png")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)?;
    (index >= 1).then_some(index)
}

fn probe(source: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir(output)?;

    let entries: Vec<_> = fs::read_dir(source)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    let mut files = entries.clone();
    files.retain(|path| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "drawio" | "xml"))
            && path
                .file_stem()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    entries
                        .iter()
                        .any(|entry| reference_page(name, entry).is_some())
                })
    });
    files.sort();

    let mut results: Vec<Value> = Vec::new();
    let mut claimed = HashSet::new();
    for path in files {
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or("Invalid filename")?;
        let imported = import_file_with_report(&path);
        let mut references: Vec<_> = entries
            .iter()
            .filter_map(|entry| {
                Some((
                    reference_page(name, entry)?,
                    entry.file_name()?.to_str()?.to_owned(),
                ))
            })
            .collect();
        references.sort();
        for (page, filename) in &references {
            if !claimed.insert(filename.clone()) {
                results.push(json!({
                    "source": path,
                    "reference": filename,
                    "page": page + 1,
                    "error": "Reference PNG matches more than one source file",
                }));
                continue;
            }
            let result = match &imported {
                Err(error) => json!({
                    "source": path,
                    "reference": filename,
                    "page": page + 1,
                    "error": format!("Import failed: {error}"),
                }),
                Ok(report) if *page >= report.workbook.sheets.len() => json!({
                    "source": path,
                    "reference": filename,
                    "page": page + 1,
                    "error": "The imported document has no corresponding page",
                }),
                Ok(report) => {
                    let options = ExportOptions {
                        sheet: Some(*page),
                        ..Default::default()
                    };
                    match export_bytes(&report.workbook, Format::Png, &options) {
                        Ok(bytes) => {
                            fs::write(output.join(filename), bytes)?;
                            json!({
                                "source": path,
                                "reference": filename,
                                "page": page + 1,
                                "warnings": format!("{:?}", report.warnings),
                            })
                        }
                        Err(error) => json!({
                            "source": path,
                            "reference": filename,
                            "page": page + 1,
                            "error": format!("Render failed: {error}"),
                            "warnings": format!("{:?}", report.warnings),
                        }),
                    }
                }
            };
            results.push(result);
        }
        if let Ok(report) = imported {
            let missing = (0..report.workbook.sheets.len())
                .filter(|page| !references.iter().any(|(index, _)| index == page))
                .count();
            if missing > 0 {
                results.push(json!({
                    "source": path,
                    "error": format!("{missing} imported page(s) have no reference PNG"),
                }));
            }
        }
        eprintln!("{name}: {} reference page(s)", references.len());
    }
    fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    let failed = results
        .iter()
        .filter(|result| result.get("error").is_some())
        .count();
    if results.is_empty() || failed > 0 {
        return Err(format!(
            "Visual probe produced {} record(s), with {failed} failure(s)",
            results.len()
        )
        .into());
    }
    println!("Rendered {} reference page(s).", results.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIAGRAM: &str = r#"<mxfile><diagram name="Page"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="Box" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="50" as="geometry"/></mxCell></root></mxGraphModel></diagram></mxfile>"#;

    #[test]
    fn writes_pngs_and_a_report_without_overwriting_existing_output() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let output = directory.path().join("output");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("sample.drawio"), DIAGRAM).unwrap();
        fs::write(source.join("sample.png"), b"reference").unwrap();

        probe(&source, &output).unwrap();
        let png = fs::read(output.join("sample.png")).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let results: Vec<Value> =
            serde_json::from_slice(&fs::read(output.join("results.json")).unwrap()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["reference"], "sample.png");
        assert_eq!(results[0]["page"], 1);
        assert!(results[0].get("error").is_none());
        assert!(probe(&source, &output).is_err());
        assert_eq!(fs::read(output.join("sample.png")).unwrap(), png);
    }

    #[test]
    fn reports_missing_pages_and_fails_when_no_references_exist() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("sample.drawio"), DIAGRAM).unwrap();
        let empty = directory.path().join("empty");
        assert!(probe(&source, &empty).is_err());
        fs::write(source.join("sample.png"), b"reference").unwrap();
        fs::write(source.join("sample.page-3.png"), b"reference").unwrap();

        let output = directory.path().join("output");
        assert!(probe(&source, &output).is_err());
        let results: Vec<Value> =
            serde_json::from_slice(&fs::read(output.join("results.json")).unwrap()).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[1]["page"], 3);
        assert_eq!(
            results[1]["error"],
            "The imported document has no corresponding page"
        );
    }

    #[test]
    fn reports_a_missing_first_reference_and_duplicate_source_names() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("sample.drawio"), DIAGRAM).unwrap();
        fs::write(source.join("sample.page-3.png"), b"reference").unwrap();
        let missing = directory.path().join("missing");
        assert!(probe(&source, &missing).is_err());
        let results: Vec<Value> =
            serde_json::from_slice(&fs::read(missing.join("results.json")).unwrap()).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["page"], 3);
        assert_eq!(
            results[1]["error"],
            "1 imported page(s) have no reference PNG"
        );

        fs::write(source.join("sample.xml"), DIAGRAM).unwrap();
        fs::write(source.join("sample.png"), b"reference").unwrap();
        let duplicate = directory.path().join("duplicate");
        assert!(probe(&source, &duplicate).is_err());
        let results: Vec<Value> =
            serde_json::from_slice(&fs::read(duplicate.join("results.json")).unwrap()).unwrap();
        assert!(results.iter().any(|result| {
            result["error"] == "Reference PNG matches more than one source file"
        }));
    }
}
