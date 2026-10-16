//! Built-in templates for new maps (File → New).

use crate::model::{Sheet, Structure, Workbook};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateCategory {
    Basic,
    Knowledge,
    Meeting,
    Study,
    Project,
    Life,
    Analysis,
    Creativity,
}

impl TemplateCategory {
    pub const ALL: [Self; 8] = [
        Self::Basic,
        Self::Knowledge,
        Self::Meeting,
        Self::Study,
        Self::Project,
        Self::Life,
        Self::Analysis,
        Self::Creativity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Knowledge => "Knowledge management",
            Self::Meeting => "Meeting and planning",
            Self::Study => "Study and education",
            Self::Project => "Project management",
            Self::Life => "Entertainment and life",
            Self::Analysis => "Analysis and decision",
            Self::Creativity => "Creativity",
        }
    }
}

/// A template entry shown in the "New" gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub structure: Structure,
    pub category: TemplateCategory,
}

pub const TEMPLATES: &[TemplateInfo] = &[
    TemplateInfo {
        id: "mind-map",
        category: TemplateCategory::Basic,
        name: "Mind Map",
        structure: Structure::MapClockwise,
    },
    TemplateInfo {
        id: "logic-chart",
        category: TemplateCategory::Basic,
        name: "Logic Chart",
        structure: Structure::LogicRight,
    },
    TemplateInfo {
        id: "brace-map",
        category: TemplateCategory::Basic,
        name: "Brace Map",
        structure: Structure::BraceRight,
    },
    TemplateInfo {
        id: "org-chart",
        category: TemplateCategory::Basic,
        name: "Org Chart",
        structure: Structure::OrgDown,
    },
    TemplateInfo {
        id: "tree-chart",
        category: TemplateCategory::Basic,
        name: "Tree Chart",
        structure: Structure::TreeRight,
    },
    TemplateInfo {
        id: "timeline",
        category: TemplateCategory::Basic,
        name: "Timeline",
        structure: Structure::TimelineHorizontal,
    },
    TemplateInfo {
        id: "fishbone",
        category: TemplateCategory::Basic,
        name: "Fishbone",
        structure: Structure::FishboneRight,
    },
    TemplateInfo {
        id: "matrix",
        category: TemplateCategory::Basic,
        name: "Matrix",
        structure: Structure::Matrix,
    },
    TemplateInfo {
        id: "tree-table",
        category: TemplateCategory::Basic,
        name: "Tree Table",
        structure: Structure::TreeTable,
    },
    TemplateInfo {
        id: "swot",
        category: TemplateCategory::Analysis,
        name: "SWOT Analysis",
        structure: Structure::MapClockwise,
    },
    TemplateInfo {
        id: "meeting",
        category: TemplateCategory::Meeting,
        name: "Meeting Notes",
        structure: Structure::LogicRight,
    },
    TemplateInfo {
        id: "project",
        category: TemplateCategory::Project,
        name: "Project Plan",
        structure: Structure::TimelineHorizontal,
    },
    TemplateInfo {
        id: "knowledge-base",
        name: "Knowledge Base",
        structure: Structure::MapClockwise,
        category: TemplateCategory::Knowledge,
    },
    TemplateInfo {
        id: "study-plan",
        name: "Learning Plan",
        structure: Structure::TreeRight,
        category: TemplateCategory::Study,
    },
    TemplateInfo {
        id: "weekly-plan",
        name: "Weekly Plan",
        structure: Structure::TimelineHorizontal,
        category: TemplateCategory::Life,
    },
    TemplateInfo {
        id: "decision",
        name: "Decision Review",
        structure: Structure::Matrix,
        category: TemplateCategory::Analysis,
    },
    TemplateInfo {
        id: "creative-workflow",
        name: "Creative Workflow",
        structure: Structure::LogicRight,
        category: TemplateCategory::Creativity,
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
        "knowledge-base" => with_children(
            "Knowledge",
            &[
                ("Concepts", &["Definition", "Example"][..]),
                ("Sources", &["Reading"]),
                ("Questions", &["Open question"]),
                ("Connections", &["Related idea"]),
            ],
        ),
        "study-plan" => with_children(
            "Learning Plan",
            &[
                ("Goals", &["Skill to develop"][..]),
                ("Resources", &["Book", "Course"]),
                ("Practice", &["Exercise"]),
                ("Review", &["Self-assessment"]),
            ],
        ),
        "weekly-plan" => with_children(
            "Week",
            &[
                ("Monday", &["Priority"][..]),
                ("Tuesday", &["Focus"]),
                ("Wednesday", &["Progress check"]),
                ("Thursday", &["Follow-up"]),
                ("Friday", &["Review"]),
                ("Weekend", &["Rest", "Personal time"]),
            ],
        ),
        "decision" => with_children(
            "Decision",
            &[
                ("Options", &["Option A", "Option B"][..]),
                ("Criteria", &["Cost", "Benefit"]),
                ("Risks", &["Uncertainty"]),
                ("Next Step", &["Owner", "Review date"]),
            ],
        ),
        "creative-workflow" => with_children(
            "Creative Project",
            &[
                ("Explore", &["Audience", "Inspiration"][..]),
                ("Draft", &["First version"]),
                ("Refine", &["Feedback"]),
                ("Publish", &["Delivery", "Reflection"]),
            ],
        ),
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
    wb.active_mut().theme.theme_id = "clarity".into();
    wb.active_mut().theme.tapered_lines = false;
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
        let mut ids = std::collections::HashSet::new();
        for t in TEMPLATES {
            assert!(ids.insert(t.id), "duplicate template ID {}", t.id);
            let wb = instantiate(t.id);
            wb.validate().unwrap();
            assert_eq!(wb.active().structure, t.structure);
            assert_eq!(wb.active().theme.theme_id, "clarity");
            assert!(!wb.active().theme.tapered_lines);
        }
        for category in TemplateCategory::ALL {
            assert!(
                TEMPLATES
                    .iter()
                    .any(|template| template.category == category)
            );
        }
    }
}
