//! Data-driven board templates (gallery category "Boards", see `docs/FREEFORM.md` §7).
//!
//! Every board starts with a hidden central topic (an empty anchor) and is described
//! by plain data: frames (zones) with their cards, free cards, floating topic trees,
//! shapes and connectors. All visible text is an English catalog key that is passed
//! through the caller's translator, so the app creates boards in the UI language.

use crate::TopicId;
use crate::board::{
    CanvasConnector, CanvasObject, CanvasObjectKind, Card, CardKind, ConnectorRoute, Endpoint,
    GridKind, ShapeKind, SheetRootMode,
};
use crate::geometry::{Color, Rect, Vec2};
use crate::model::{Sheet, Structure, Workbook};

const STICKY: Vec2 = Vec2::new(150.0, 100.0);
const NOTE_H: f32 = 70.0;

/// Pastel sticky colours with dark text: readable on Light and Dark backgrounds.
const YELLOW: Color = Color::rgb(0xFF, 0xE5, 0x8F);
const GREEN: Color = Color::rgb(0xB7, 0xEB, 0x8F);
const BLUE: Color = Color::rgb(0xA5, 0xD8, 0xFF);
const PINK: Color = Color::rgb(0xFF, 0xC1, 0xD9);
const ORANGE: Color = Color::rgb(0xFF, 0xD5, 0x91);
pub(crate) const STICKY_TEXT: Color = Color::rgb(0x26, 0x26, 0x26);

/// Translucent mid-tone frame tints: visible on Light and Dark backgrounds.
const TINT_GREEN: Color = Color::rgba(0x4C, 0xAF, 0x50, 0x30);
const TINT_ORANGE: Color = Color::rgba(0xFF, 0x98, 0x00, 0x30);
const TINT_BLUE: Color = Color::rgba(0x21, 0x96, 0xF3, 0x30);
const TINT_RED: Color = Color::rgba(0xE5, 0x39, 0x35, 0x30);
const TINT_GRAY: Color = Color::rgba(0x80, 0x80, 0x80, 0x24);

const SHAPE_STROKE: Color = Color::rgb(0x5A, 0x6B, 0x7D);

#[derive(Debug, Clone, Copy)]
pub(crate) struct CardDef {
    pub text: &'static str,
    pub center: Vec2,
    pub kind: CardKind,
    pub size: Vec2,
    pub color: Option<Color>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameDef {
    pub name: &'static str,
    pub rect: Rect,
    pub color: Option<Color>,
    pub cards: &'static [CardDef],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TreeDef {
    pub title: &'static str,
    pub at: Vec2,
    pub structure: Structure,
    pub children: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShapeDef {
    pub shape: ShapeKind,
    pub label: &'static str,
    pub rect: Rect,
    pub fill: Color,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LinkDef {
    /// Indices into [`BoardDef::shapes`].
    pub from: usize,
    pub to: usize,
    pub label: &'static str,
    pub route: ConnectorRoute,
}

/// One board template.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BoardDef {
    pub id: &'static str,
    pub grid: GridKind,
    pub cards: &'static [CardDef],
    pub frames: &'static [FrameDef],
    pub trees: &'static [TreeDef],
    pub shapes: &'static [ShapeDef],
    pub links: &'static [LinkDef],
}

const fn sticky(text: &'static str, x: f32, y: f32, color: Color) -> CardDef {
    CardDef {
        text,
        center: Vec2::new(x, y),
        kind: CardKind::Sticky,
        size: STICKY,
        color: Some(color),
    }
}

const fn note(text: &'static str, x: f32, y: f32, w: f32) -> CardDef {
    CardDef {
        text,
        center: Vec2::new(x, y),
        kind: CardKind::Text,
        size: Vec2::new(w, NOTE_H),
        color: None,
    }
}

const fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect {
        min: Vec2::new(x, y),
        size: Vec2::new(w, h),
    }
}

const fn frame(
    name: &'static str,
    rect: Rect,
    color: Color,
    cards: &'static [CardDef],
) -> FrameDef {
    FrameDef {
        name,
        rect,
        color: Some(color),
        cards,
    }
}

const fn shape(shape: ShapeKind, label: &'static str, rect: Rect, fill: Color) -> ShapeDef {
    ShapeDef {
        shape,
        label,
        rect,
        fill,
    }
}

const fn link(from: usize, to: usize, label: &'static str, route: ConnectorRoute) -> LinkDef {
    LinkDef {
        from,
        to,
        label,
        route,
    }
}

const EMPTY: BoardDef = BoardDef {
    id: "",
    grid: GridKind::Dots,
    cards: &[],
    frames: &[],
    trees: &[],
    shapes: &[],
    links: &[],
};

/// Business Model Canvas column width, gap and heights (canonical 5-column layout).
const BMC_W: f32 = 260.0;
const BMC_GAP: f32 = 10.0;
const BMC_H: f32 = 520.0;
const BMC_HALF: f32 = (BMC_H - BMC_GAP) / 2.0;
const BMC_LOW: f32 = BMC_HALF + BMC_GAP;
const BMC_BOTTOM_Y: f32 = BMC_H + BMC_GAP;
const BMC_BOTTOM_W: f32 = (5.0 * BMC_W + 3.0 * BMC_GAP) / 2.0;
const BMC_BOTTOM_H: f32 = 220.0;

const fn bmc_x(column: usize) -> f32 {
    column as f32 * (BMC_W + BMC_GAP)
}

const fn bmc_prompt(text: &'static str, column: usize, top: f32) -> CardDef {
    note(text, bmc_x(column) + BMC_W / 2.0, top + 90.0, BMC_W - 40.0)
}

pub(crate) const BOARDS: &[BoardDef] = &[
    BoardDef {
        id: "blank-canvas",
        ..EMPTY
    },
    BoardDef {
        id: "multi-root",
        trees: &[
            TreeDef {
                title: "Option A",
                at: Vec2::new(0.0, 0.0),
                structure: Structure::MapClockwise,
                children: &["Benefits", "Costs", "Risks"],
            },
            TreeDef {
                title: "Option B",
                at: Vec2::new(640.0, 0.0),
                structure: Structure::MapClockwise,
                children: &["Benefits", "Costs", "Risks"],
            },
            TreeDef {
                title: "Decision",
                at: Vec2::new(220.0, 320.0),
                structure: Structure::LogicRight,
                children: &["Criteria", "Next step"],
            },
        ],
        ..EMPTY
    },
    BoardDef {
        id: "brainstorm-wall",
        cards: &[note(
            "Brainstorm: describe the challenge here",
            0.0,
            -70.0,
            560.0,
        )],
        frames: &[
            frame(
                "Ideas",
                rect(-560.0, 0.0, 540.0, 380.0),
                TINT_GRAY,
                &[
                    sticky("One idea per sticky", -460.0, 120.0, YELLOW),
                    sticky("Build on other ideas", -290.0, 120.0, YELLOW),
                    sticky("Go for quantity first", -460.0, 250.0, YELLOW),
                ],
            ),
            frame(
                "Questions",
                rect(0.0, 0.0, 270.0, 380.0),
                TINT_BLUE,
                &[sticky("What do we need to learn?", 135.0, 120.0, BLUE)],
            ),
            frame(
                "Parking lot",
                rect(290.0, 0.0, 270.0, 380.0),
                TINT_ORANGE,
                &[sticky("Topics for later", 425.0, 120.0, PINK)],
            ),
        ],
        ..EMPTY
    },
    BoardDef {
        id: "retrospective-board",
        cards: &[note(
            "Retrospective: team and time period",
            560.0,
            -70.0,
            560.0,
        )],
        frames: &[
            frame(
                "Went well",
                rect(0.0, 0.0, 360.0, 420.0),
                TINT_GREEN,
                &[
                    sticky("What helped us?", 95.0, 120.0, GREEN),
                    sticky("Something to celebrate", 265.0, 120.0, GREEN),
                ],
            ),
            frame(
                "To improve",
                rect(380.0, 0.0, 360.0, 420.0),
                TINT_ORANGE,
                &[
                    sticky("What slowed us down?", 475.0, 120.0, ORANGE),
                    sticky("What surprised us?", 645.0, 120.0, ORANGE),
                ],
            ),
            frame(
                "Actions",
                rect(760.0, 0.0, 360.0, 420.0),
                TINT_BLUE,
                &[sticky("Action, owner and due date", 855.0, 120.0, BLUE)],
            ),
        ],
        ..EMPTY
    },
    BoardDef {
        id: "kanban-board",
        frames: &[
            frame(
                "To do",
                rect(0.0, 0.0, 300.0, 520.0),
                TINT_GRAY,
                &[
                    sticky("Describe the next task", 150.0, 120.0, YELLOW),
                    sticky("Add an owner and due date", 150.0, 240.0, YELLOW),
                ],
            ),
            frame(
                "Doing",
                rect(320.0, 0.0, 300.0, 520.0),
                TINT_BLUE,
                &[sticky("Limit work in progress", 470.0, 120.0, BLUE)],
            ),
            frame(
                "Done",
                rect(640.0, 0.0, 300.0, 520.0),
                TINT_GREEN,
                &[sticky("Finished task", 790.0, 120.0, GREEN)],
            ),
        ],
        ..EMPTY
    },
    BoardDef {
        id: "business-model-canvas",
        frames: &[
            frame(
                "Key Partners",
                rect(bmc_x(0), 0.0, BMC_W, BMC_H),
                TINT_GRAY,
                &[bmc_prompt("Who helps you deliver?", 0, 0.0)],
            ),
            frame(
                "Key Activities",
                rect(bmc_x(1), 0.0, BMC_W, BMC_HALF),
                TINT_GRAY,
                &[bmc_prompt("What must you do well?", 1, 0.0)],
            ),
            frame(
                "Key Resources",
                rect(bmc_x(1), BMC_LOW, BMC_W, BMC_HALF),
                TINT_GRAY,
                &[bmc_prompt("What do you need to deliver?", 1, BMC_LOW)],
            ),
            frame(
                "Value Propositions",
                rect(bmc_x(2), 0.0, BMC_W, BMC_H),
                TINT_ORANGE,
                &[bmc_prompt("Which problem do you solve?", 2, 0.0)],
            ),
            frame(
                "Customer Relationships",
                rect(bmc_x(3), 0.0, BMC_W, BMC_HALF),
                TINT_BLUE,
                &[bmc_prompt("How do you work with customers?", 3, 0.0)],
            ),
            frame(
                "Channels",
                rect(bmc_x(3), BMC_LOW, BMC_W, BMC_HALF),
                TINT_BLUE,
                &[bmc_prompt("How do you reach customers?", 3, BMC_LOW)],
            ),
            frame(
                "Customer Segments",
                rect(bmc_x(4), 0.0, BMC_W, BMC_H),
                TINT_BLUE,
                &[bmc_prompt("Who are you creating value for?", 4, 0.0)],
            ),
            frame(
                "Cost Structure",
                rect(0.0, BMC_BOTTOM_Y, BMC_BOTTOM_W, BMC_BOTTOM_H),
                TINT_RED,
                &[note(
                    "What are the main costs?",
                    BMC_BOTTOM_W / 2.0,
                    BMC_BOTTOM_Y + 100.0,
                    BMC_W,
                )],
            ),
            frame(
                "Revenue Streams",
                rect(
                    BMC_BOTTOM_W + BMC_GAP,
                    BMC_BOTTOM_Y,
                    BMC_BOTTOM_W,
                    BMC_BOTTOM_H,
                ),
                TINT_GREEN,
                &[note(
                    "How do customers pay?",
                    BMC_BOTTOM_W * 1.5 + BMC_GAP,
                    BMC_BOTTOM_Y + 100.0,
                    BMC_W,
                )],
            ),
        ],
        ..EMPTY
    },
    BoardDef {
        id: "flowchart",
        grid: GridKind::Lines,
        shapes: &[
            shape(
                ShapeKind::Terminator,
                "Start",
                rect(-80.0, -30.0, 160.0, 60.0),
                GREEN,
            ),
            shape(
                ShapeKind::Process,
                "Step",
                rect(-90.0, 80.0, 180.0, 70.0),
                BLUE,
            ),
            shape(
                ShapeKind::Decision,
                "Decision?",
                rect(-100.0, 200.0, 200.0, 110.0),
                YELLOW,
            ),
            shape(
                ShapeKind::Process,
                "Action",
                rect(-90.0, 360.0, 180.0, 70.0),
                BLUE,
            ),
            shape(
                ShapeKind::Terminator,
                "End",
                rect(-80.0, 480.0, 160.0, 60.0),
                GREEN,
            ),
            shape(
                ShapeKind::Process,
                "Rework",
                rect(200.0, 220.0, 180.0, 70.0),
                ORANGE,
            ),
        ],
        links: &[
            link(0, 1, "", ConnectorRoute::Straight),
            link(1, 2, "", ConnectorRoute::Straight),
            link(2, 3, "Yes", ConnectorRoute::Straight),
            link(3, 4, "", ConnectorRoute::Straight),
            link(2, 5, "No", ConnectorRoute::Straight),
            link(5, 1, "", ConnectorRoute::Straight),
        ],
        ..EMPTY
    },
    BoardDef {
        id: "swot-board",
        frames: &[
            frame(
                "Strengths",
                rect(0.0, 0.0, 420.0, 300.0),
                TINT_GREEN,
                &[sticky("Internal advantages", 105.0, 120.0, GREEN)],
            ),
            frame(
                "Weaknesses",
                rect(430.0, 0.0, 420.0, 300.0),
                TINT_ORANGE,
                &[sticky("Internal gaps", 535.0, 120.0, ORANGE)],
            ),
            frame(
                "Opportunities",
                rect(0.0, 310.0, 420.0, 300.0),
                TINT_BLUE,
                &[sticky("External trends to use", 105.0, 430.0, BLUE)],
            ),
            frame(
                "Threats",
                rect(430.0, 310.0, 420.0, 300.0),
                TINT_RED,
                &[sticky("External risks", 535.0, 430.0, PINK)],
            ),
        ],
        ..EMPTY
    },
];

pub(crate) fn find(id: &str) -> Option<&'static BoardDef> {
    BOARDS.iter().find(|board| board.id == id)
}

fn add_card(sheet: &mut Sheet, def: &CardDef, tr: &dyn Fn(&'static str) -> String) -> TopicId {
    let mut card = Card::new(def.kind, def.size);
    card.color = def.color;
    let id = sheet
        .add_card(&tr(def.text), def.center, card)
        .expect("valid template card");
    if let (Some(fill), Some(topic)) = (def.color, sheet.topic_mut(id)) {
        // The topic renderer paints `style.fill`, so coloured cards look right everywhere.
        topic.style.fill = Some(fill);
        topic.style.text_color = Some(STICKY_TEXT);
    }
    id
}

/// Builds the sheet of a board template with an empty hidden anchor.
pub(crate) fn build(def: &BoardDef, title: &str, tr: &dyn Fn(&'static str) -> String) -> Workbook {
    let mut sheet = Sheet::new(title, "");
    sheet.root_mode = SheetRootMode::Hidden;
    sheet.board.grid = def.grid;
    for card in def.cards {
        add_card(&mut sheet, card, tr);
    }
    for frame in def.frames {
        let roots = frame
            .cards
            .iter()
            .map(|card| add_card(&mut sheet, card, tr))
            .collect();
        let zone = sheet
            .create_zone(&tr(frame.name), roots, frame.rect)
            .expect("valid template frame");
        if let Some(z) = sheet.zones.iter_mut().find(|z| z.id == zone) {
            z.color = frame.color;
        }
    }
    for tree in def.trees {
        let root = sheet.add_floating(tr(tree.title), tree.at);
        if let Some(topic) = sheet.topic_mut(root) {
            topic.structure = Some(tree.structure);
        }
        for child in tree.children {
            sheet.add_child(root, None, tr(child));
        }
    }
    let mut shapes = Vec::with_capacity(def.shapes.len());
    for s in def.shapes {
        let mut object = CanvasObject::shape(s.shape, s.rect);
        if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
            *label = tr(s.label);
        }
        object.style.fill = Some(s.fill);
        object.style.stroke = Some(SHAPE_STROKE);
        shapes.push(
            sheet
                .add_canvas_object(object)
                .expect("valid template shape"),
        );
    }
    for l in def.links {
        let mut connector = CanvasConnector::new(
            Endpoint::Object(shapes[l.from]),
            Endpoint::Object(shapes[l.to]),
        );
        connector.route = l.route;
        if !l.label.is_empty() {
            connector.label = tr(l.label);
        }
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .expect("valid template connector");
    }
    Workbook::new(sheet)
}

/// Every translatable content string of every board template (for catalog tests).
pub fn board_strings() -> Vec<&'static str> {
    let mut out = Vec::new();
    for board in BOARDS {
        let cards = board
            .cards
            .iter()
            .chain(board.frames.iter().flat_map(|f| f.cards.iter()));
        out.extend(cards.map(|c| c.text));
        out.extend(board.frames.iter().map(|f| f.name));
        for tree in board.trees {
            out.push(tree.title);
            out.extend(tree.children.iter().copied());
        }
        out.extend(board.shapes.iter().map(|s| s.label));
        out.extend(
            board
                .links
                .iter()
                .map(|l| l.label)
                .filter(|l| !l.is_empty()),
        );
    }
    out.sort_unstable();
    out.dedup();
    out
}
