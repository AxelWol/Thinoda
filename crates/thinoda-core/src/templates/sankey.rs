//! Original, fictional examples. Labels are translation keys like board labels.
use crate::board::{Band, CanvasConnector, CanvasObject, Card, CardKind, Endpoint, SheetRootMode};
use crate::sankey::{Flow, SankeyMode, SankeySettings};
use crate::{Sheet, Vec2, Workbook};

type NodeDef = (&'static str, u32, f32);
type LinkDef = (usize, usize, u32);
type DiagramDef<'a> = (SankeyMode, &'a [NodeDef], &'a [LinkDef]);

pub(super) fn strings() -> &'static [&'static str] {
    &[
        "Available budget",
        "Essentials",
        "Projects",
        "Reserve",
        "Energy supply",
        "Useful energy",
        "Heat losses",
        "Services",
        "Production",
        "Exploring",
        "Practising",
        "Confident",
        "All deliveries",
        "Clear route",
        "Delayed route",
        "On time",
        "Late",
        "Fresh feed",
        "In use",
        "Recycling",
        "Final output",
        "New flow node",
        "Normalized basis: 100% of the available budget. Edit each shared link from either endpoint; unallocated amounts are not normalized.",
        "Normalized basis: 100% of supplied energy. Useful energy and losses share one linear width scale.",
        "Normalized basis: 100% of learners. Aggregate stage transitions do not preserve full cohort paths. Repeated names at different stages are distinct nodes.",
        "Normalized basis: 100% of mutually exclusive deliveries. Child percentages are conditional; ribbons show overall probability. Merges sum their incoming contributions.",
        "Normalized basis: 100% reference throughput. Fresh feed is 60%; the 20% recycling loop makes In use receive and send 80%. Feedback is explicit, not probability.",
        "Choose a shared normalized 100% basis for this diagram. Add nodes and links in Data > Weights; unset links are dashed, and zero links have no band.",
    ]
}

pub(super) fn build(id: &str, title: &str, tr: &dyn Fn(&'static str) -> String) -> Workbook {
    let (mode, nodes, links): DiagramDef<'_> = match id {
        "sankey-blank" => (SankeyMode::Standard, &[("New flow node", 0, 0.0)], &[]),
        "sankey-energy" => (
            SankeyMode::Standard,
            &[
                ("Energy supply", 0, 0.0),
                ("Useful energy", 1, 0.0),
                ("Heat losses", 1, 250.0),
                ("Services", 2, 0.0),
                ("Production", 2, 200.0),
            ],
            &[
                (0, 1, 65_000),
                (0, 2, 35_000),
                (1, 3, 40_000),
                (1, 4, 25_000),
            ],
        ),
        "sankey-learning" => (
            SankeyMode::Alluvial,
            &[
                ("Exploring", 0, 0.0),
                ("Practising", 1, 0.0),
                ("Exploring", 1, 250.0),
                ("Practising", 2, 0.0),
                ("Confident", 2, 250.0),
            ],
            &[
                (0, 1, 60_000),
                (0, 2, 40_000),
                (1, 3, 25_000),
                (1, 4, 35_000),
                (2, 3, 15_000),
                (2, 4, 25_000),
            ],
        ),
        "sankey-probability" => (
            SankeyMode::Probability,
            &[
                ("All deliveries", 0, 0.0),
                ("Clear route", 1, 0.0),
                ("Delayed route", 1, 250.0),
                ("On time", 2, 0.0),
                ("Late", 2, 250.0),
            ],
            &[
                (0, 1, 80_000),
                (0, 2, 20_000),
                (1, 3, 90_000),
                (1, 4, 10_000),
                (2, 3, 50_000),
                (2, 4, 50_000),
            ],
        ),
        "sankey-circular" => (
            SankeyMode::Circular,
            &[
                ("Fresh feed", 0, 0.0),
                ("In use", 1, 0.0),
                ("Recycling", 2, 0.0),
                ("Final output", 2, 250.0),
            ],
            &[
                (0, 1, 60_000),
                (1, 2, 20_000),
                (1, 3, 60_000),
                (2, 1, 20_000),
            ],
        ),
        _ => (
            SankeyMode::Standard,
            &[
                ("Available budget", 0, 0.0),
                ("Essentials", 1, 0.0),
                ("Projects", 1, 250.0),
                ("Reserve", 1, 500.0),
            ],
            &[(0, 1, 55_000), (0, 2, 25_000), (0, 3, 20_000)],
        ),
    };
    let mut sheet = Sheet::new(title, "");
    sheet.root_mode = SheetRootMode::Hidden;
    sheet.theme.theme_id = "clarity".into();
    sheet.theme.tapered_lines = false;
    sheet.sankey = Some(SankeySettings {
        mode,
        color_scale: true,
        ..Default::default()
    });
    let mut ids = Vec::new();
    let note = match id {
        "sankey-blank" => {
            "Choose a shared normalized 100% basis for this diagram. Add nodes and links in Data > Weights; unset links are dashed, and zero links have no band."
        }
        "sankey-energy" => {
            "Normalized basis: 100% of supplied energy. Useful energy and losses share one linear width scale."
        }
        "sankey-learning" => {
            "Normalized basis: 100% of learners. Aggregate stage transitions do not preserve full cohort paths. Repeated names at different stages are distinct nodes."
        }
        "sankey-probability" => {
            "Normalized basis: 100% of mutually exclusive deliveries. Child percentages are conditional; ribbons show overall probability. Merges sum their incoming contributions."
        }
        "sankey-circular" => {
            "Normalized basis: 100% reference throughput. Fresh feed is 60%; the 20% recycling loop makes In use receive and send 80%. Feedback is explicit, not probability."
        }
        _ => {
            "Normalized basis: 100% of the available budget. Edit each shared link from either endpoint; unallocated amounts are not normalized."
        }
    };
    for &(label, stage, y) in nodes {
        // All nodes have enough space for the common 2 sheet-units/% scale.
        if let Ok(id) = sheet.add_card(
            &tr(label),
            Vec2::new(stage as f32 * 420.0, y),
            Card::new(CardKind::Text, Vec2::new(155.0, 210.0)),
        ) {
            ids.push(id);
            if let Some(topic) = sheet.topics.get_mut(&id) {
                topic.notes = Some(tr(note).into());
            }
        }
    }
    if let Some(settings) = &mut sheet.sankey {
        if mode == SankeyMode::Probability {
            settings.sources.insert(ids[0], 100_000);
        }
        if mode == SankeyMode::Alluvial {
            for (i, &(_, stage, _)) in nodes.iter().enumerate() {
                settings.stages.insert(ids[i], stage);
            }
        }
    }
    for &(from, to, weight) in links {
        let mut c = CanvasConnector::new(Endpoint::Topic(ids[from]), Endpoint::Topic(ids[to]));
        c.sankey = Some(Flow {
            weight: Some(weight),
        });
        c.arrow_end = false;
        let mut object = CanvasObject::connector(c);
        object.band = Band::Below;
        // Construction is validated as a complete graph, not in partial states.
        object.id = crate::board::CanvasObjectId(sheet.alloc());
        sheet.canvas.objects.push(object);
    }
    // The known-valid original examples use the same layout action as the UI.
    crate::sankey::arrange(&mut sheet).expect("built-in Sankey example must have a valid layout");
    Workbook::new(sheet)
}
