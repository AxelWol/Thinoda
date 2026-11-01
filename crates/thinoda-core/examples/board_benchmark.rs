//! Repeatable CPU baseline for Freeform boards (scene, hit-test, undo, serde).

use serde::Serialize;
use std::{hint::black_box, path::PathBuf, time::Instant};
use thinoda_core::{
    CanvasObject, Card, CardKind, Document, InkStroke, InkTool, Sheet, SheetRootMode, Workbook,
    build_scene,
    document::approx_sheet_bytes,
    geometry::{Color, Vec2},
    layout::{ApproxMeasure, layout_sheet},
};

const USAGE: &str = "cargo run --release -p thinoda-core --example board_benchmark -- \
[--samples 20] [--output report.json]

Measures hidden-root boards with 1k and 10k ink strokes (64 points each) and 50 image
cards: layout + scene build, 1000 hit-tests, 300 undoable moves, serde round trip.
Reports CPU time and approximate memory, not GPU frame time.";

const POINTS_PER_STROKE: usize = 64;
const CARDS: usize = 50;
const UNDO_PUSHES: usize = 300;
const HIT_TESTS: usize = 1000;

struct Config {
    samples: usize,
    output: Option<PathBuf>,
}

impl Config {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Self>, String> {
        let mut config = Self {
            samples: 20,
            output: None,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--help" {
                return Ok(None);
            }
            if !matches!(arg.as_str(), "--samples" | "--output") {
                return Err(format!("unknown argument: {arg}\n{USAGE}"));
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{arg} requires a value"))?;
            match arg.as_str() {
                "--samples" => {
                    config.samples = value.parse().map_err(|_| "samples must be an integer")?
                }
                "--output" => config.output = Some(value.into()),
                _ => unreachable!(),
            }
        }
        if !(3..=200).contains(&config.samples) {
            return Err("samples must be between 3 and 200".into());
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

fn fixture(strokes: usize) -> Workbook {
    let mut sheet = Sheet::new("Board benchmark", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let columns = 100;
    for i in 0..strokes {
        let origin = Vec2::new((i % columns) as f32 * 120.0, (i / columns) as f32 * 120.0);
        let points = (0..POINTS_PER_STROKE)
            .map(|p| {
                let t = p as f32 / (POINTS_PER_STROKE - 1) as f32;
                origin + Vec2::new(t * 100.0, (t * std::f32::consts::TAU * 2.0).sin() * 30.0)
            })
            .collect();
        sheet
            .add_canvas_object(CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::rgb(30, 30, 30),
                3.0,
                points,
            )))
            .expect("valid stroke");
    }
    for i in 0..CARDS {
        sheet
            .add_card(
                &format!("Image {i}"),
                Vec2::new(i as f32 * 260.0, -400.0),
                Card::new(CardKind::Image, Vec2::new(240.0, 180.0)),
            )
            .expect("valid card");
    }
    Workbook::new(sheet)
}

#[derive(Serialize)]
struct Case {
    strokes: usize,
    points: usize,
    cards: usize,
    samples: usize,
    layout_and_scene: Distribution,
    hit_test_1000: Distribution,
    undo_push_300: Distribution,
    serialize: Distribution,
    deserialize: Distribution,
    json_bytes: usize,
    ink_point_bytes: usize,
    approx_snapshot_bytes: usize,
    history_bytes_after_300: usize,
}

fn benchmark(config: &Config, strokes: usize) -> Result<Case, Box<dyn std::error::Error>> {
    let workbook = fixture(strokes);
    workbook.validate()?;
    let mut doc = Document::new(workbook);
    let target = doc.sheet().canvas.objects[0].id;
    let (mut scene_ms, mut hit_ms, mut undo_ms, mut ser_ms, mut de_ms) =
        (vec![], vec![], vec![], vec![], vec![]);
    let mut json_bytes = 0;
    let mut history_bytes = 0;
    for _ in 0..config.samples {
        let start = Instant::now();
        let layout = layout_sheet(doc.sheet(), &ApproxMeasure, false);
        let scene = black_box(build_scene(doc.sheet(), &layout));
        scene_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        if scene.items.len() != strokes + CARDS {
            return Err("scene is missing items".into());
        }
        let bounds = scene.bounds().ok_or("empty scene")?;
        let start = Instant::now();
        let mut hits = 0;
        for i in 0..HIT_TESTS {
            let t = i as f32 / HIT_TESTS as f32;
            let p = bounds.min + Vec2::new(bounds.size.x * t, bounds.size.y * ((t * 7.0) % 1.0));
            hits += black_box(scene.hit_test(p)).is_some() as usize;
        }
        hit_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        black_box(hits);

        let start = Instant::now();
        for _ in 0..UNDO_PUSHES {
            doc.move_canvas_objects(&[target], Vec2::new(1.0, 0.0))?;
        }
        undo_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        history_bytes = doc.history_bytes();
        while doc.undo() {}

        let start = Instant::now();
        let json = black_box(serde_json::to_string(doc.workbook())?);
        ser_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        json_bytes = json.len();
        let start = Instant::now();
        let back: Workbook = black_box(serde_json::from_str(&json)?);
        de_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        if back.sheets[0].canvas.objects.len() != strokes {
            return Err("round trip lost objects".into());
        }
    }
    Ok(Case {
        strokes,
        points: strokes * POINTS_PER_STROKE,
        cards: CARDS,
        samples: config.samples,
        layout_and_scene: distribution(scene_ms),
        hit_test_1000: distribution(hit_ms),
        undo_push_300: distribution(undo_ms),
        serialize: distribution(ser_ms),
        deserialize: distribution(de_ms),
        json_bytes,
        ink_point_bytes: strokes * POINTS_PER_STROKE * std::mem::size_of::<Vec2>(),
        approx_snapshot_bytes: approx_sheet_bytes(doc.sheet()),
        history_bytes_after_300: history_bytes,
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
    cases: Vec<Case>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(config) = Config::parse(std::env::args().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    let cases = vec![benchmark(&config, 1_000)?, benchmark(&config, 10_000)?];
    let report = Report {
        schema_version: 1,
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        package_version: env!("CARGO_PKG_VERSION"),
        debug_assertions: cfg!(debug_assertions),
        measurement: "CPU; ApproxMeasure; no GPU, UI frame timing or file I/O",
        cases,
    };
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = config.output {
        std::fs::write(path, format!("{json}\n"))?;
    }
    println!("{json}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_deterministic_and_valid() {
        let book = fixture(20);
        assert_eq!(book, fixture(20));
        book.validate().unwrap();
        let sheet = book.active();
        assert_eq!(sheet.canvas.objects.len(), 20);
        assert_eq!(sheet.canvas.ink_point_count(), 20 * POINTS_PER_STROKE);
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        assert_eq!(build_scene(sheet, &layout).items.len(), 20 + CARDS);
    }

    #[test]
    fn invalid_cli_values_are_explicit_errors() {
        for args in [
            vec!["--samples", "2"],
            vec!["--samples", "many"],
            vec!["--output"],
            vec!["--unknown", "1"],
        ] {
            assert!(Config::parse(args.into_iter().map(str::to_string)).is_err());
        }
        assert_eq!(
            Config::parse(Vec::<String>::new())
                .unwrap()
                .unwrap()
                .samples,
            20
        );
        assert!(Config::parse(["--help".to_string()]).unwrap().is_none());
    }
}
