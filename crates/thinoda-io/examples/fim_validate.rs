//! Development oracle runner; reads installed official bundles without networking.
use std::{env, fs, process::ExitCode};
use thinoda_io::fim::{Dialect, ExchangeValidator};

fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() < 4 {
        return Err(
            "Usage: fim_validate xdf2|xdf3 SCHEMA_ZIP SUPPLEMENT_ZIP|- DOCUMENT_XML...".into(),
        );
    }
    let dialect = match args[0].as_str() {
        "xdf2" => Dialect::Xdf2,
        "xdf3" => Dialect::Xdf3,
        _ => return Err("Dialect must be xdf2 or xdf3".into()),
    };
    let exchange = fs::read(&args[1])?;
    let supplement = if args[2] == "-" {
        None
    } else {
        Some(fs::read(&args[2])?)
    };
    let validator =
        ExchangeValidator::from_official_archives(dialect, &exchange, supplement.as_deref())?;
    let mut results = Vec::new();
    let mut all_conformant = true;
    for path in &args[3..] {
        let bytes = fs::read(path)?;
        match validator.validate(&bytes) {
            Ok(report) => {
                all_conformant &= report.is_exchange_conformant();
                results.push(serde_json::json!({ "file": path, "report": report }));
            }
            Err(error) => {
                all_conformant = false;
                results.push(serde_json::json!({ "file": path, "error": error.to_string() }));
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(if all_conformant {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
