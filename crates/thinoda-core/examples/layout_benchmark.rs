//! Repeatable CPU baseline, deliberately separate from GPU frame-rate measurements.

use serde::Serialize;
use std::{hint::black_box, path::PathBuf, time::Instant};
use thinoda_core::{
    Document, Sheet, TopicId, Workbook,
    layout::{ApproxMeasure, layout_sheet},
};

const USAGE: &str = "cargo run --release -p thinoda-core --example layout_benchmark -- \
[--topics 10000] [--samples 30] [--output report.json] [--layout-budget-ms N]

Measures balanced and wide maps with ApproxMeasure in light mode.
Reports CPU layout, undoable leaf rename, and undo latency, not GPU frame time.
An optional p95 layout budget fails the command after writing its report.";

struct Config {
    topics: usize,
    samples: usize,
    output: Option<PathBuf>,
    budget_ms: Option<f64>,
}

impl Config {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Self>, String> {
        let mut config = Self {
            topics: 10_000,
            samples: 30,
            output: None,
            budget_ms: None,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--help" {
                return Ok(None);
            }
            if !matches!(
                arg.as_str(),
                "--topics" | "--samples" | "--output" | "--layout-budget-ms"
            ) {
                return Err(format!("unknown argument: {arg}\n{USAGE}"));
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{arg} requires a value"))?;
            match arg.as_str() {
                "--topics" => {
                    config.topics = value.parse().map_err(|_| "topics must be an integer")?
                }
                "--samples" => {
                    config.samples = value.parse().map_err(|_| "samples must be an integer")?
                }
                "--output" => config.output = Some(value.into()),
                "--layout-budget-ms" => {
                    config.budget_ms = Some(value.parse().map_err(|_| "budget must be a number")?)
                }
                _ => unreachable!(),
            }
        }
        if !(2..=100_000).contains(&config.topics) {
            return Err("topics must be between 2 and 100000".into());
        }
        if !(5..=200).contains(&config.samples) {
            return Err("samples must be between 5 and 200".into());
        }
        if config
            .budget_ms
            .is_some_and(|budget| !budget.is_finite() || budget <= 0.0)
        {
            return Err("layout budget must be finite and positive".into());
        }
        Ok(Some(config))
    }
}

#[derive(Serialize)]
struct Distribution {
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

fn distribution(mut samples: Vec<f64>) -> Distribution {
    samples.sort_by(f64::total_cmp);
    let rank = |percent: usize| (samples.len() * percent).div_ceil(100) - 1;
    Distribution {
        p50_ms: samples[rank(50)],
        p95_ms: samples[rank(95)],
        max_ms: *samples.last().unwrap(),
    }
}

fn fixture(topics: usize, wide: bool) -> (Workbook, TopicId) {
    let mut sheet = Sheet::new("Benchmark", "Topic 0");
    let mut ids = vec![sheet.root];
    for index in 1..topics {
        let parent = if wide {
            sheet.root
        } else {
            ids[(index - 1) / 6]
        };
        ids.push(sheet.add_child(parent, None, format!("Topic {index}")));
    }
    (Workbook::new(sheet), *ids.last().unwrap())
}

#[derive(Serialize)]
struct Case {
    shape: &'static str,
    topics: usize,
    samples: usize,
    layout: Distribution,
    leaf_rename_transaction: Distribution,
    undo: Distribution,
}

fn benchmark(config: &Config, wide: bool) -> Result<Case, Box<dyn std::error::Error>> {
    let (workbook, leaf) = fixture(config.topics, wide);
    workbook.validate()?;
    let mut doc = Document::new(workbook);
    let original_title = doc.sheet().topic(leaf).unwrap().title.clone();
    black_box(layout_sheet(doc.sheet(), &ApproxMeasure, false));
    let mut layout = Vec::with_capacity(config.samples);
    let mut edits = Vec::with_capacity(config.samples);
    let mut undos = Vec::with_capacity(config.samples);
    for _ in 0..config.samples {
        let start = Instant::now();
        let result = black_box(layout_sheet(black_box(doc.sheet()), &ApproxMeasure, false));
        layout.push(start.elapsed().as_secs_f64() * 1000.0);
        if result.topics.len() != config.topics
            || !result.bounds.size.x.is_finite()
            || !result.bounds.size.y.is_finite()
        {
            return Err("layout did not contain all topics with finite bounds".into());
        }
        let start = Instant::now();
        doc.edit_sheet("Benchmark rename", |sheet| {
            sheet.set_title(leaf, "Edited topic")
        })?;
        edits.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        let undone = doc.undo();
        undos.push(start.elapsed().as_secs_f64() * 1000.0);
        if !undone || doc.sheet().topic(leaf).unwrap().title != original_title || doc.can_undo() {
            return Err("undo did not restore the original title/history".into());
        }
    }
    Ok(Case {
        shape: if wide { "wide" } else { "balanced-six-way" },
        topics: config.topics,
        samples: config.samples,
        layout: distribution(layout),
        leaf_rename_transaction: distribution(edits),
        undo: distribution(undos),
    })
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    os: &'static str,
    arch: &'static str,
    package_version: &'static str,
    debug_assertions: bool,
    measurement: &'static str,
    layout_budget_ms: Option<f64>,
    passed_budget: bool,
    cases: Vec<Case>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(config) = Config::parse(std::env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    let cases = vec![benchmark(&config, false)?, benchmark(&config, true)?];
    let passed_budget = config
        .budget_ms
        .is_none_or(|budget| cases.iter().all(|case| case.layout.p95_ms <= budget));
    let report = Report {
        schema_version: 1,
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        package_version: env!("CARGO_PKG_VERSION"),
        debug_assertions: cfg!(debug_assertions),
        measurement: "CPU; ApproxMeasure; light mode; no GPU, UI frame timing or file I/O",
        layout_budget_ms: config.budget_ms,
        passed_budget,
        cases,
    };
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = config.output {
        std::fs::write(path, format!("{json}\n"))?;
    }
    println!("{json}");
    if !passed_budget {
        return Err("p95 layout budget exceeded; inspect the JSON report".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles_use_the_full_sample() {
        let result = distribution((1..=20).rev().map(f64::from).collect());
        assert_eq!(result.p50_ms, 10.0);
        assert_eq!(result.p95_ms, 19.0);
        assert_eq!(result.max_ms, 20.0);
    }

    #[test]
    fn fixtures_are_deterministic_valid_and_have_the_requested_shapes() {
        for wide in [false, true] {
            let (book, leaf) = fixture(100, wide);
            assert_eq!(book, fixture(100, wide).0);
            book.validate().unwrap();
            let sheet = book.active();
            assert_eq!(sheet.topics.len(), 100);
            assert_eq!(sheet.topic(leaf).unwrap().title, "Topic 99");
            assert_eq!(sheet.children(sheet.root).len(), if wide { 99 } else { 6 });
            assert_eq!(layout_sheet(sheet, &ApproxMeasure, false).topics.len(), 100);
        }
    }

    #[test]
    fn invalid_cli_values_are_explicit_errors() {
        for args in [
            vec!["--topics", "1"],
            vec!["--topics", "100001"],
            vec!["--topics", "many"],
            vec!["--samples", "0"],
            vec!["--samples", "201"],
            vec!["--layout-budget-ms", "NaN"],
            vec!["--layout-budget-ms", "0"],
            vec!["--output"],
            vec!["--unknown", "1"],
        ] {
            assert!(Config::parse(args.into_iter().map(str::to_string)).is_err());
        }
        let default = Config::parse(Vec::<String>::new()).unwrap().unwrap();
        assert_eq!(default.topics, 10000);
        assert_eq!(default.samples, 30);
        assert!(Config::parse(["--help".to_string()]).unwrap().is_none());
    }
}
