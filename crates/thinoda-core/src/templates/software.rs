//! Original, unbound structural examples using the shared free-container model.

use crate::stencil_object::StencilRef;
use crate::{
    CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, Color, ConnectorRoute,
    Endpoint, Rect, ShapeKind, Sheet, SheetRootMode, Vec2, Workbook,
};

struct Example {
    lanes: [&'static str; 3],
    steps: [&'static str; 4],
}

const EXAMPLES: [Example; 3] = [
    Example {
        lanes: [
            "Customer (human)",
            "Support (human)",
            "Service/API (software)",
        ],
        steps: [
            "Submit request",
            "Check request",
            "Fulfil request",
            "Receive result",
        ],
    },
    Example {
        lanes: [
            "Requester (human)",
            "Reviewer (human)",
            "Approval API (software)",
        ],
        steps: [
            "Submit request",
            "Approve request?",
            "Record approval",
            "Notify requester",
        ],
    },
    Example {
        lanes: [
            "Monitor (software)",
            "On-call (human)",
            "Recovery API (software)",
        ],
        steps: [
            "Detect incident",
            "Assess impact",
            "Restore service",
            "Verify recovery",
        ],
    },
];

const LEGEND: &str = "Editable structural example. Labels are descriptive; typed semantics and automatic lane layout are not enabled.";
const INK: Color = Color::rgb(38, 48, 62);

fn shape(
    sheet: &mut Sheet,
    label: String,
    rect: Rect,
    fill: Color,
    header: bool,
) -> CanvasObjectId {
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect);
    if let CanvasObjectKind::Shape { label: value, .. } = &mut object.kind {
        *value = label;
    }
    object.style.fill = Some(fill);
    object.style.stroke = Some(INK);
    object.style.text_color = Some(INK);
    if header {
        object.stencil = Some(StencilRef::new("thinoda.general", "header-frame"));
    }
    object.id = sheet.alloc_canvas_object_id();
    let id = object.id;
    sheet.canvas.objects.push(object);
    id
}

pub(super) fn build(id: &str, title: &str, tr: &dyn Fn(&'static str) -> String) -> Workbook {
    let index = match id {
        "software-handoff" => 0,
        "software-approval" => 1,
        "software-incident" => 2,
        _ => unreachable!("unregistered Software template: {id}"),
    };
    let example = &EXAMPLES[index];
    let mut sheet = Sheet::new(title, "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = shape(
        &mut sheet,
        title.to_owned(),
        Rect::from_min_size(Vec2::ZERO, Vec2::new(1100.0, 660.0)),
        Color::rgb(238, 241, 245),
        true,
    );
    sheet.canvas.scene.containers.insert(root);
    let mut lanes = Vec::new();
    for (row, label) in example.lanes.iter().enumerate() {
        let lane = shape(
            &mut sheet,
            tr(label),
            Rect::from_min_size(
                Vec2::new(20.0, 55.0 + row as f32 * 195.0),
                Vec2::new(1060.0, 180.0),
            ),
            Color::rgb(227, 238, 249),
            true,
        );
        sheet.canvas.scene.containers.insert(lane);
        sheet.canvas.scene.parents.insert(lane, root);
        lanes.push(lane);
    }
    let rows = [0, 1, 2, 0];
    let columns = [0, 1, 2, 2];
    let mut steps = Vec::new();
    for (step, label) in example.steps.iter().enumerate() {
        let node = shape(
            &mut sheet,
            tr(label),
            Rect::from_min_size(
                Vec2::new(
                    80.0 + columns[step] as f32 * 360.0,
                    130.0 + rows[step] as f32 * 195.0,
                ),
                Vec2::new(200.0, 65.0),
            ),
            Color::rgb(255, 255, 255),
            false,
        );
        if index == 1
            && step == 1
            && let Some(object) = sheet.canvas.object_mut(node)
        {
            object.kind = CanvasObjectKind::Shape {
                shape: ShapeKind::Diamond,
                label: tr(label),
            };
        }
        sheet.canvas.scene.parents.insert(node, lanes[rows[step]]);
        steps.push(node);
    }
    let mut links = vec![(0, 1, ""), (1, 2, ""), (2, 3, "")];
    if index == 1 {
        links[1].2 = "Yes";
        links.push((1, 3, "No"));
    } else if index == 2 {
        links.push((3, 0, ""));
    }
    for (from, to, label) in links {
        let mut connector =
            CanvasConnector::new(Endpoint::Object(steps[from]), Endpoint::Object(steps[to]));
        connector.route = ConnectorRoute::Elbow;
        if !label.is_empty() {
            connector.label = tr(label);
        }
        connector.color = Some(INK);
        let mut object = CanvasObject::connector(connector);
        object.id = sheet.alloc_canvas_object_id();
        sheet.canvas.scene.parents.insert(
            object.id,
            if rows[from] == rows[to] {
                lanes[rows[from]]
            } else {
                root
            },
        );
        sheet.canvas.objects.push(object);
    }
    shape(
        &mut sheet,
        tr(LEGEND),
        Rect::from_min_size(Vec2::new(0.0, 685.0), Vec2::new(1100.0, 65.0)),
        Color::rgb(255, 249, 225),
        false,
    );
    sheet.theme.theme_id = "clarity".into();
    sheet.theme.tapered_lines = false;
    Workbook::new(sheet)
}

pub(super) fn strings() -> Vec<&'static str> {
    EXAMPLES
        .iter()
        .flat_map(|example| example.lanes.iter().chain(&example.steps))
        .copied()
        .chain([LEGEND])
        .collect()
}
