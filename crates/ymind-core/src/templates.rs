//! Built-in templates for new maps (File → New).

use crate::model::{Sheet, Structure, Workbook};

/// A template entry shown in the "New" gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub structure: Structure,
}

pub const TEMPLATES: &[TemplateInfo] = &[
    TemplateInfo {
        id: "mind-map",
        name: "Mind Map",
        structure: Structure::MapClockwise,
    },
    TemplateInfo {
        id: "logic-chart",
        name: "Logic Chart",
        structure: Structure::LogicRight,
    },
    TemplateInfo {
        id: "brace-map",
        name: "Brace Map",
        structure: Structure::BraceRight,
    },
    TemplateInfo {
        id: "org-chart",
        name: "Org Chart",
        structure: Structure::OrgDown,
    },
    TemplateInfo {
        id: "tree-chart",
        name: "Tree Chart",
        structure: Structure::TreeRight,
    },
    TemplateInfo {
        id: "timeline",
        name: "Timeline",
        structure: Structure::TimelineHorizontal,
    },
    TemplateInfo {
        id: "fishbone",
        name: "Fishbone",
        structure: Structure::FishboneRight,
    },
    TemplateInfo {
        id: "matrix",
        name: "Matrix",
        structure: Structure::Matrix,
    },
    TemplateInfo {
        id: "tree-table",
        name: "Tree Table",
        structure: Structure::TreeTable,
    },
    TemplateInfo {
        id: "swot",
        name: "SWOT Analysis",
        structure: Structure::MapClockwise,
    },
    TemplateInfo {
        id: "meeting",
        name: "Meeting Notes",
        structure: Structure::LogicRight,
    },
    TemplateInfo {
        id: "project",
        name: "Project Plan",
        structure: Structure::TimelineHorizontal,
    },
];

/// Blank workbook: central topic plus `main_count` main topics.
pub fn blank(sheet_title: &str, central: &str, main: &str, main_count: usize) -> Workbook {
    let mut s = Sheet::new(sheet_title, central);
    for i in 0..main_count {
        s.add_child(s.root, None, format!("{main} {}", i + 1));
    }
    Workbook::new(s)
}

/// Instantiates a template by id (unknown ids fall back to a blank mind map).
pub fn instantiate(id: &str) -> Workbook {
    let info = TEMPLATES
        .iter()
        .find(|t| t.id == id)
        .copied()
        .unwrap_or(TEMPLATES[0]);
    let mut wb = match info.id {
        "swot" => with_children(
            "SWOT Analysis",
            &[
                ("Strengths", &["Advantage", "Resource"][..]),
                ("Weaknesses", &["Gap"]),
                ("Opportunities", &["Trend"]),
                ("Threats", &["Competitor"]),
            ],
        ),
        "meeting" => with_children(
            "Meeting",
            &[
                ("Agenda", &["Topic 1", "Topic 2"][..]),
                ("Attendees", &[]),
                ("Decisions", &[]),
                ("Action Items", &[]),
            ],
        ),
        "project" => with_children(
            "Project",
            &[
                ("Kick-off", &["Goals"][..]),
                ("Planning", &["Scope", "Budget"]),
                ("Execution", &[]),
                ("Launch", &[]),
            ],
        ),
        "fishbone" => with_children(
            "Problem",
            &[
                ("People", &["Cause"][..]),
                ("Process", &["Cause"]),
                ("Equipment", &["Cause"]),
                ("Materials", &["Cause"]),
            ],
        ),
        "timeline" => blank("Map 1", "Timeline", "Event", 4),
        "org-chart" => blank("Map 1", "Organization", "Department", 3),
        _ => blank("Map 1", "Central Topic", "Main Topic", 4),
    };
    wb.active_mut().structure = info.structure;
    wb
}

fn with_children(central: &str, items: &[(&str, &[&str])]) -> Workbook {
    let mut s = Sheet::new("Map 1", central);
    for (main, subs) in items {
        let m = s.add_child(s.root, None, *main);
        for sub in *subs {
            s.add_child(m, None, *sub);
        }
    }
    Workbook::new(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_has_four_main_topics() {
        let wb = Workbook::default();
        assert_eq!(wb.active().children(wb.active().root).len(), 4);
        wb.validate().unwrap();
    }

    #[test]
    fn all_templates_valid() {
        for t in TEMPLATES {
            let wb = instantiate(t.id);
            wb.validate().unwrap();
            assert_eq!(wb.active().structure, t.structure);
        }
    }
}
