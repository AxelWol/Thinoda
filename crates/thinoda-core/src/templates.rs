//! Built-in templates for new maps (File → New).

use crate::model::{Sheet, Structure, Workbook};

mod boards;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateCategory {
    Basic,
    /// Free-form boards: hidden central topic, frames, cards, shapes, several roots.
    Boards,
    Knowledge,
    Meeting,
    Study,
    Project,
    Life,
    Analysis,
    Creativity,
}

impl TemplateCategory {
    pub const ALL: [Self; 9] = [
        Self::Basic,
        Self::Boards,
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
            Self::Boards => "Boards",
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
    TemplateInfo {
        id: "problem-solving",
        name: "Problem Solving",
        structure: Structure::LogicRight,
        category: TemplateCategory::Knowledge,
    },
    TemplateInfo {
        id: "concept-framework",
        name: "Concept Framework",
        structure: Structure::BraceRight,
        category: TemplateCategory::Knowledge,
    },
    TemplateInfo {
        id: "software-architecture",
        name: "Software Architecture",
        structure: Structure::OrgDown,
        category: TemplateCategory::Knowledge,
    },
    TemplateInfo {
        id: "business-plan",
        name: "Business Plan",
        structure: Structure::MapClockwise,
        category: TemplateCategory::Meeting,
    },
    TemplateInfo {
        id: "workshop",
        name: "Workshop",
        structure: Structure::TimelineHorizontal,
        category: TemplateCategory::Meeting,
    },
    TemplateInfo {
        id: "dissertation",
        name: "Dissertation Plan",
        structure: Structure::TreeRight,
        category: TemplateCategory::Study,
    },
    TemplateInfo {
        id: "school-timetable",
        name: "School Timetable",
        structure: Structure::TreeTable,
        category: TemplateCategory::Study,
    },
    TemplateInfo {
        id: "reading-notes",
        name: "Reading Notes",
        structure: Structure::LogicRight,
        category: TemplateCategory::Study,
    },
    TemplateInfo {
        id: "language-learning",
        name: "Language Learning",
        structure: Structure::MapClockwise,
        category: TemplateCategory::Study,
    },
    TemplateInfo {
        id: "project-retrospective",
        name: "Project Retrospective",
        structure: Structure::LogicRight,
        category: TemplateCategory::Project,
    },
    TemplateInfo {
        id: "shopping-list",
        name: "Shopping List",
        structure: Structure::TreeRight,
        category: TemplateCategory::Life,
    },
    TemplateInfo {
        id: "mood-journal",
        name: "Mood Journal",
        structure: Structure::TimelineHorizontal,
        category: TemplateCategory::Life,
    },
    TemplateInfo {
        id: "service-comparison",
        name: "Service Comparison",
        structure: Structure::TreeTable,
        category: TemplateCategory::Life,
    },
    TemplateInfo {
        id: "cause-effect",
        name: "Cause and Effect",
        structure: Structure::FishboneRight,
        category: TemplateCategory::Analysis,
    },
    TemplateInfo {
        id: "comparison",
        name: "Comparison",
        structure: Structure::TreeTable,
        category: TemplateCategory::Analysis,
    },
    TemplateInfo {
        id: "social-media-plan",
        name: "Social Media Plan",
        structure: Structure::LogicRight,
        category: TemplateCategory::Analysis,
    },
    TemplateInfo {
        id: "recipe",
        name: "Recipe",
        structure: Structure::TreeRight,
        category: TemplateCategory::Creativity,
    },
    TemplateInfo {
        id: "film-production",
        name: "Film Production",
        structure: Structure::TimelineHorizontal,
        category: TemplateCategory::Creativity,
    },
    board_info("blank-canvas", "Blank Canvas"),
    board_info("multi-root", "Multi-root Map"),
    board_info("brainstorm-wall", "Brainstorm Wall"),
    board_info("retrospective-board", "Retrospective Board"),
    board_info("kanban-board", "Kanban Board"),
    board_info("business-model-canvas", "Business Model Canvas"),
    board_info("flowchart", "Flowchart"),
    board_info("swot-board", "SWOT Board"),
    board_info("affinity-diagram", "Affinity Diagram"),
    board_info("eisenhower-matrix", "Eisenhower Matrix"),
    board_info("meeting-board", "Meeting Board"),
    board_info("swimlane", "Swimlane"),
    board_info("venn-diagram", "Venn Diagram"),
    board_info("empathy-map", "Empathy Map"),
    board_info("storyboard", "Storyboard"),
];

const fn board_info(id: &'static str, name: &'static str) -> TemplateInfo {
    TemplateInfo {
        id,
        name,
        structure: Structure::MapClockwise,
        category: TemplateCategory::Boards,
    }
}

/// Every translatable content string of the board templates (mind-map template
/// content stays English).
pub fn board_strings() -> Vec<&'static str> {
    boards::board_strings()
}

/// Blank workbook: central topic plus `main_count` main topics.
pub fn blank(sheet_title: &str, central: &str, main: &str, main_count: usize) -> Workbook {
    let mut s = Sheet::new(sheet_title, central);
    for i in 0..main_count {
        s.add_child(s.root, None, format!("{main} {}", i + 1));
    }
    Workbook::new(s)
}

/// Instantiates a template by id (unknown ids fall back to a blank mind map).
/// Board content stays English; see [`instantiate_localized`].
pub fn instantiate(id: &str) -> Workbook {
    instantiate_localized(id, &|s| s.to_owned())
}

/// Like [`instantiate`], but passes board sheet titles and content through `tr`.
pub fn instantiate_localized(id: &str, tr: &dyn Fn(&'static str) -> String) -> Workbook {
    let info = TEMPLATES
        .iter()
        .find(|t| t.id == id)
        .copied()
        .unwrap_or(TEMPLATES[0]);
    if let Some(board) = boards::find(info.id) {
        let mut wb = boards::build(board, &tr(info.name), tr);
        wb.active_mut().structure = info.structure;
        wb.active_mut().theme.theme_id = "clarity".into();
        wb.active_mut().theme.tapered_lines = false;
        return wb;
    }
    let mut wb = match info.id {
        "problem-solving" => with_children(
            "Problem to Solve",
            &[
                (
                    "Define",
                    &["Observed situation", "Desired outcome", "Constraints"][..],
                ),
                ("Investigate", &["Evidence", "Possible causes", "Unknowns"]),
                ("Explore", &["Option A", "Option B", "Trade-offs"]),
                ("Act", &["Small experiment", "Owner", "Success measure"]),
                ("Review", &["Result", "Lessons", "Next adjustment"]),
            ],
        ),
        "concept-framework" => with_children(
            "Concept Framework",
            &[
                ("Purpose", &["Question", "Scope"][..]),
                ("Building Blocks", &["Term", "Definition", "Example"]),
                ("Connections", &["Dependency", "Feedback"]),
                ("Assumptions", &["Supporting evidence", "Limitations"]),
                ("Application", &["Use case", "Counterexample"]),
            ],
        ),
        "software-architecture" => with_children(
            "System",
            &[
                ("Clients", &["User interface", "Accessibility"][..]),
                ("Services", &["Responsibilities", "Interfaces"]),
                ("Data", &["Storage", "Retention", "Recovery"]),
                ("Integration", &["External systems", "Failure handling"]),
                ("Quality", &["Security", "Performance", "Observability"]),
                (
                    "Delivery",
                    &["Deployment", "Compatibility", "Open decisions"],
                ),
            ],
        ),
        "business-plan" => with_children(
            "Business Idea",
            &[
                ("Customers", &["Audience", "Need", "Research"][..]),
                ("Value", &["Offer", "Differentiation"]),
                ("Delivery", &["Activities", "Partners", "Resources"]),
                ("Economics", &["Revenue assumptions", "Costs", "Runway"]),
                ("Go to Market", &["Channels", "First experiment"]),
                ("Risks", &["Assumptions to test", "Review date"]),
            ],
        ),
        "workshop" => with_children(
            "Workshop",
            &[
                ("Prepare", &["Outcome", "Participants", "Materials"][..]),
                ("Open", &["Check-in", "Working agreements"]),
                ("Explore", &["Individual ideas", "Group discussion"]),
                ("Decide", &["Criteria", "Selected direction"]),
                ("Close", &["Actions and owners", "Feedback", "Follow-up"]),
            ],
        ),
        "dissertation" => with_children(
            "Research Project",
            &[
                (
                    "Question",
                    &["Motivation", "Scope", "Research question"][..],
                ),
                (
                    "Literature",
                    &["Search strategy", "Key sources", "Research gap"],
                ),
                ("Method", &["Design", "Ethics review", "Data collection"]),
                ("Analysis", &["Approach", "Findings", "Limitations"]),
                (
                    "Writing",
                    &["Chapter outline", "Citations", "Supervisor feedback"],
                ),
                (
                    "Submission",
                    &["Milestones", "Proofreading", "Defense preparation"],
                ),
            ],
        ),
        "school-timetable" => with_children(
            "School Week",
            &[
                (
                    "Monday",
                    &["Morning subject", "Afternoon subject", "Homework"][..],
                ),
                (
                    "Tuesday",
                    &["Morning subject", "Afternoon subject", "Homework"],
                ),
                (
                    "Wednesday",
                    &["Morning subject", "Afternoon subject", "Homework"],
                ),
                (
                    "Thursday",
                    &["Morning subject", "Afternoon subject", "Homework"],
                ),
                (
                    "Friday",
                    &["Morning subject", "Afternoon subject", "Homework"],
                ),
            ],
        ),
        "reading-notes" => with_children(
            "Reading",
            &[
                (
                    "Source",
                    &["Title and author", "Date", "Location or link"][..],
                ),
                ("Main Ideas", &["Claim", "Supporting evidence"]),
                ("My Notes", &["Paraphrase", "Connection", "Question"]),
                ("Evaluation", &["Strengths", "Limitations"]),
                ("Follow-up", &["Further reading", "Action"]),
            ],
        ),
        "language-learning" => with_children(
            "Language Practice",
            &[
                ("Goal", &["Situation", "Weekly target"][..]),
                ("Vocabulary", &["Word", "Meaning", "Example sentence"]),
                ("Patterns", &["Grammar point", "Practice example"]),
                ("Input", &["Listening", "Reading"]),
                ("Output", &["Speaking", "Writing"]),
                ("Review", &["Recall practice", "Feedback", "Next session"]),
            ],
        ),
        "project-retrospective" => with_children(
            "Project Retrospective",
            &[
                ("Context", &["Goal", "Time period", "Participants"][..]),
                ("Keep", &["What worked", "Evidence"]),
                ("Change", &["Friction", "Contributing factors"]),
                ("Try", &["Experiment", "Owner", "Expected outcome"]),
                ("Follow-up", &["Review date", "Result"]),
            ],
        ),
        "shopping-list" => with_children(
            "Shopping",
            &[
                ("Produce", &["Fruit", "Vegetables"][..]),
                ("Pantry", &["Grains", "Staples"]),
                ("Chilled", &["Item", "Quantity"]),
                ("Household", &["Supplies"]),
                (
                    "Before Leaving",
                    &["Check cupboards", "Budget", "Reusable bags"],
                ),
            ],
        ),
        "mood-journal" => with_children(
            "Daily Reflection",
            &[
                ("Morning", &["Feeling", "Energy", "Intention"][..]),
                ("Afternoon", &["Moment to notice", "What helped"]),
                ("Evening", &["Feeling", "Something appreciated"]),
                ("Tomorrow", &["Small next step", "Time to rest"]),
            ],
        ),
        "service-comparison" => with_children(
            "Choose a Service",
            &[
                ("Requirements", &["Must have", "Nice to have"][..]),
                (
                    "Service A",
                    &["Features", "Total cost", "Support", "Limitations"],
                ),
                (
                    "Service B",
                    &["Features", "Total cost", "Support", "Limitations"],
                ),
                (
                    "Verification",
                    &["Trial result", "Contract terms", "Data portability"],
                ),
                ("Decision", &["Rationale", "Review date"]),
            ],
        ),
        "cause-effect" => with_children(
            "Observed Effect",
            &[
                ("People", &["Possible cause", "Evidence to check"][..]),
                ("Process", &["Possible cause", "Evidence to check"]),
                ("Tools", &["Possible cause", "Evidence to check"]),
                ("Environment", &["Possible cause", "Evidence to check"]),
                ("Measurement", &["Possible cause", "Evidence to check"]),
            ],
        ),
        "comparison" => with_children(
            "Compare Alternatives",
            &[
                ("Criteria", &["Purpose", "Constraints", "Weighting"][..]),
                ("Alternative A", &["Benefits", "Costs", "Risks", "Evidence"]),
                ("Alternative B", &["Benefits", "Costs", "Risks", "Evidence"]),
                (
                    "Conclusion",
                    &["Similarities", "Differences", "Open questions"],
                ),
            ],
        ),
        "social-media-plan" => with_children(
            "Social Media Plan",
            &[
                ("Purpose", &["Audience", "Outcome", "Success measure"][..]),
                ("Content", &["Theme", "Format", "Source permissions"]),
                ("Channels", &["Channel", "Posting schedule"]),
                ("Production", &["Draft", "Review", "Accessible media"]),
                ("Learn", &["Results", "Feedback", "Next experiment"]),
            ],
        ),
        "recipe" => with_children(
            "Recipe",
            &[
                (
                    "Overview",
                    &["Servings", "Preparation time", "Equipment"][..],
                ),
                ("Ingredients", &["Ingredient and quantity", "Substitutions"]),
                ("Preparation", &["Step 1", "Step 2"]),
                ("Cooking", &["Method", "Time and temperature"]),
                ("Serving", &["Finish", "Notes for next time"]),
            ],
        ),
        "film-production" => with_children(
            "Film Project",
            &[
                ("Development", &["Premise", "Audience", "Script"][..]),
                (
                    "Pre-production",
                    &["Budget", "Crew", "Locations and permissions"],
                ),
                (
                    "Production",
                    &["Shot list", "Schedule", "Sound", "Media backups"],
                ),
                (
                    "Post-production",
                    &["Edit", "Sound mix", "Captions", "Credits"],
                ),
                (
                    "Release",
                    &["Rights check", "Delivery formats", "Screening"],
                ),
            ],
        ),
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

    #[test]
    fn scenario_templates_have_original_actionable_content_instead_of_blank_fallbacks() {
        for (id, central, branch) in [
            ("problem-solving", "Problem to Solve", "Investigate"),
            ("concept-framework", "Concept Framework", "Assumptions"),
            ("software-architecture", "System", "Integration"),
            ("business-plan", "Business Idea", "Economics"),
            ("workshop", "Workshop", "Decide"),
            ("dissertation", "Research Project", "Method"),
            ("school-timetable", "School Week", "Wednesday"),
            ("reading-notes", "Reading", "Evaluation"),
            ("language-learning", "Language Practice", "Output"),
            ("project-retrospective", "Project Retrospective", "Try"),
            ("shopping-list", "Shopping", "Before Leaving"),
            ("mood-journal", "Daily Reflection", "Evening"),
            ("service-comparison", "Choose a Service", "Verification"),
            ("cause-effect", "Observed Effect", "Measurement"),
            ("comparison", "Compare Alternatives", "Criteria"),
            ("social-media-plan", "Social Media Plan", "Production"),
            ("recipe", "Recipe", "Ingredients"),
            ("film-production", "Film Project", "Post-production"),
        ] {
            assert!(TEMPLATES.iter().any(|info| info.id == id));
            let workbook = instantiate(id);
            let sheet = workbook.active();
            assert_eq!(sheet.topic(sheet.root).unwrap().title, central, "{id}");
            let main = sheet.children(sheet.root);
            assert!(main.len() >= 4, "{id}");
            assert!(
                main.iter()
                    .any(|id| sheet.topic(*id).unwrap().title == branch)
            );
            for main in main {
                assert!(!sheet.children(*main).is_empty(), "{id}");
            }
            assert!(
                sheet
                    .topics
                    .values()
                    .all(|topic| !topic.title.starts_with("Main Topic"))
            );
            workbook.validate().unwrap();
        }
    }

    fn board_ids() -> Vec<&'static str> {
        TEMPLATES
            .iter()
            .filter(|t| t.category == TemplateCategory::Boards)
            .map(|t| t.id)
            .collect()
    }

    #[test]
    fn boards_category_lists_every_board_template() {
        assert_eq!(
            board_ids(),
            [
                "blank-canvas",
                "multi-root",
                "brainstorm-wall",
                "retrospective-board",
                "kanban-board",
                "business-model-canvas",
                "flowchart",
                "swot-board",
                "affinity-diagram",
                "eisenhower-matrix",
                "meeting-board",
                "swimlane",
                "venn-diagram",
                "empathy-map",
                "storyboard",
            ]
        );
        assert_eq!(boards::BOARDS.len(), board_ids().len());
        for board in boards::BOARDS {
            assert!(board_ids().contains(&board.id), "{}", board.id);
        }
        for (id, name) in [
            ("affinity-diagram", "Affinity Diagram"),
            ("eisenhower-matrix", "Eisenhower Matrix"),
            ("meeting-board", "Meeting Board"),
            ("swimlane", "Swimlane"),
            ("venn-diagram", "Venn Diagram"),
            ("empathy-map", "Empathy Map"),
            ("storyboard", "Storyboard"),
        ] {
            let info = TEMPLATES.iter().find(|info| info.id == id).unwrap();
            assert_eq!(info.name, name);
            assert_eq!(info.category, TemplateCategory::Boards);
            assert!(boards::find(id).is_some());
        }
    }

    #[test]
    fn board_templates_are_valid_deterministic_and_hide_an_empty_anchor() {
        use crate::board::{CanvasObjectKind, SheetRootMode};
        for id in board_ids() {
            let wb = instantiate(id);
            wb.validate().unwrap();
            assert_eq!(wb, instantiate(id), "{id} is not deterministic");
            let sheet = wb.active();
            assert_eq!(sheet.root_mode, SheetRootMode::Hidden, "{id}");
            assert_eq!(sheet.central_topic_content(), None, "{id}");
            assert!(sheet.root_topic().title.is_empty(), "{id}");
            assert!(sheet.children(sheet.root).is_empty(), "{id}");
            assert!(sheet.board.grid != crate::board::GridKind::None, "{id}");
            let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
            assert!(
                !layout.topics.contains_key(&sheet.root),
                "{id} leaks anchor"
            );
            // Content does not overlap: cards/trees are disjoint, cards sit in their frame.
            let roots: Vec<_> = sheet.floating.clone();
            for (i, a) in roots.iter().enumerate() {
                let ra = subtree_rect(sheet, &layout, *a);
                for b in &roots[i + 1..] {
                    let rb = subtree_rect(sheet, &layout, *b);
                    assert!(!ra.intersects(&rb), "{id}: {a:?} overlaps {b:?}");
                }
            }
            for zone in &sheet.zones {
                for root in &zone.roots {
                    let r = layout.topics[root].rect;
                    assert!(
                        zone.frame.contains(r.min) && zone.frame.contains(r.max()),
                        "{id}: card outside frame {}",
                        zone.name
                    );
                }
            }
            for (i, a) in sheet.zones.iter().enumerate() {
                for b in &sheet.zones[i + 1..] {
                    assert!(!a.frame.intersects(&b.frame), "{id}: frames overlap");
                }
            }
            let shapes: Vec<_> = sheet
                .canvas
                .objects
                .iter()
                .filter(|o| matches!(o.kind, CanvasObjectKind::Shape { .. }))
                .collect();
            for (i, a) in shapes.iter().enumerate() {
                for b in &shapes[i + 1..] {
                    if id != "venn-diagram" {
                        assert!(!a.rect.intersects(&b.rect), "{id}: shapes overlap");
                    }
                }
            }
        }
        assert_eq!(instantiate("blank-canvas").active().topics.len(), 1);
        let multi = instantiate("multi-root");
        assert_eq!(multi.active().floating.len(), 3);
        let flow = instantiate("flowchart");
        assert_eq!(flow.active().canvas.objects.len(), 12);
    }

    fn subtree_rect(
        sheet: &crate::model::Sheet,
        layout: &crate::Layout,
        root: crate::TopicId,
    ) -> crate::Rect {
        let mut rect = layout.topics[&root].rect;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            for child in sheet.children(id) {
                rect = rect.union(&layout.topics[child].rect);
                stack.push(*child);
            }
        }
        rect
    }

    #[test]
    fn affinity_diagram_has_independent_stickies_and_persistent_frames() {
        use crate::board::{BoardMember, CardKind};

        let mut wb = instantiate("affinity-diagram");
        let sheet = wb.active();
        assert_eq!(
            sheet
                .zones
                .iter()
                .map(|z| z.name.as_str())
                .collect::<Vec<_>>(),
            ["Ideas to sort", "Theme 1", "Theme 2", "Theme 3"]
        );
        assert!(
            sheet
                .zones
                .iter()
                .all(|zone| zone.keep_empty && zone.roots.is_empty())
        );
        assert!(sheet.canvas.groups.is_empty());

        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let source = sheet.zones[0].frame;
        let target = sheet.zones[1].frame;
        let heading = sheet
            .content_topics()
            .find(|topic| topic.title == "Group related ideas, then name each theme")
            .unwrap();
        assert_eq!(heading.card.unwrap().kind, CardKind::Text);
        assert!(layout.topics[&heading.id].rect.max().y < source.min.y);
        let stickies: Vec<_> = sheet
            .content_topics()
            .filter(|topic| topic.card.is_some_and(|card| card.kind == CardKind::Sticky))
            .map(|topic| topic.id)
            .collect();
        assert_eq!(stickies.len(), 4);
        for id in &stickies {
            let topic = sheet.topic(*id).unwrap();
            let rect = layout.topics[id].rect;
            assert_eq!(topic.title, "An idea or observation");
            assert!(topic.card.unwrap().color.is_some());
            assert!(source.contains(rect.min) && source.contains(rect.max()));
        }

        // Moving notes is visual; it does not silently assign them to a frame.
        let from = sheet.topic(stickies[0]).unwrap().position.unwrap();
        let sheet = wb.active_mut();
        sheet
            .move_board_members(&[BoardMember::Card(stickies[0])], target.center() - from)
            .unwrap();
        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let moved = layout.topics[&stickies[0]].rect;
        assert!(target.contains(moved.min) && target.contains(moved.max()));
        assert!(sheet.zones.iter().all(|zone| zone.roots.is_empty()));
        for id in stickies {
            assert!(sheet.detach_subtree(id).is_some());
        }
        assert_eq!(sheet.zones.len(), 4);
        wb.validate().unwrap();
    }

    #[test]
    fn eisenhower_matrix_has_four_aligned_empty_quadrants() {
        use crate::board::{Card, CardKind};
        use crate::geometry::Vec2;

        let mut wb = instantiate("eisenhower-matrix");
        let sheet = wb.active();
        assert!(sheet.floating.is_empty());
        assert_eq!(sheet.topics.len(), 1);
        assert!(sheet.canvas.objects.is_empty());
        assert!(
            sheet
                .zones
                .iter()
                .all(|zone| zone.keep_empty && zone.roots.is_empty())
        );
        assert_eq!(
            sheet
                .zones
                .iter()
                .map(|z| z.name.as_str())
                .collect::<Vec<_>>(),
            [
                "Do: urgent and important",
                "Schedule: important, not urgent",
                "Delegate: urgent, not important",
                "Eliminate: neither urgent nor important",
            ]
        );
        let [do_first, schedule, delegate, eliminate] = sheet.zones.as_slice() else {
            panic!("expected four quadrants");
        };
        assert_eq!(do_first.frame.size, schedule.frame.size);
        assert_eq!(do_first.frame.size, delegate.frame.size);
        assert_eq!(do_first.frame.size, eliminate.frame.size);
        assert_eq!(do_first.frame.min.y, schedule.frame.min.y);
        assert_eq!(delegate.frame.min.y, eliminate.frame.min.y);
        assert_eq!(do_first.frame.min.x, delegate.frame.min.x);
        assert_eq!(schedule.frame.min.x, eliminate.frame.min.x);
        assert!(do_first.frame.max().x < schedule.frame.min.x);
        assert!(do_first.frame.max().y < delegate.frame.min.y);

        // Even a quadrant that later acquires and loses a member remains a frame.
        let frame_id = do_first.id;
        let center = do_first.frame.center();
        let sheet = wb.active_mut();
        let card = sheet
            .add_card(
                "Example",
                center,
                Card::new(CardKind::Sticky, Vec2::new(150.0, 100.0)),
            )
            .unwrap();
        sheet.set_zone_roots(frame_id, vec![card]).unwrap();
        assert!(sheet.detach_subtree(card).is_some());
        assert_eq!(sheet.zones.len(), 4);
        assert!(sheet.zones.iter().all(|zone| zone.roots.is_empty()));
        wb.validate().unwrap();
    }

    #[test]
    fn meeting_board_has_independent_prompts_and_persistent_visual_frames() {
        use crate::board::{BoardMember, CardKind};

        let mut wb = instantiate("meeting-board");
        let sheet = wb.active();
        assert_eq!(
            sheet
                .zones
                .iter()
                .map(|z| z.name.as_str())
                .collect::<Vec<_>>(),
            ["Agenda", "Notes", "Decisions", "Actions"]
        );
        assert!(
            sheet
                .zones
                .iter()
                .all(|zone| zone.keep_empty && zone.roots.is_empty())
        );
        assert!(sheet.canvas.groups.is_empty());
        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let heading = sheet
            .content_topics()
            .find(|topic| topic.title == "Meeting: purpose and date")
            .unwrap();
        assert_eq!(heading.card.unwrap().kind, CardKind::Text);
        assert!(layout.topics[&heading.id].rect.max().y < sheet.zones[0].frame.min.y);
        let mut stickies = Vec::new();
        for (zone, text) in sheet.zones.iter().zip([
            "Topic and timebox",
            "Key points and questions",
            "Decision and reason",
            "Action, owner and due date",
        ]) {
            let topic = sheet
                .content_topics()
                .find(|topic| topic.title == text)
                .unwrap();
            let card = topic.card.unwrap();
            assert_eq!(card.kind, CardKind::Sticky);
            assert!(card.color.is_some());
            let rect = layout.topics[&topic.id].rect;
            assert!(zone.frame.contains(rect.min) && zone.frame.contains(rect.max()));
            stickies.push(topic.id);
        }
        assert_eq!(sheet.content_topics().count(), 5);

        let from = sheet.topic(stickies[0]).unwrap().position.unwrap();
        let target = sheet.zones[1].frame.max() - crate::Vec2::new(90.0, 90.0);
        let sheet = wb.active_mut();
        sheet
            .move_board_members(&[BoardMember::Card(stickies[0])], target - from)
            .unwrap();
        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let moved = layout.topics[&stickies[0]].rect;
        assert!(sheet.zones[1].frame.contains(moved.min));
        assert!(sheet.zones[1].frame.contains(moved.max()));
        assert!(sheet.zones.iter().all(|zone| zone.roots.is_empty()));
        for id in stickies {
            assert!(sheet.detach_subtree(id).is_some());
        }
        assert_eq!(sheet.zones.len(), 4);
        assert!(sheet.zones.iter().all(|zone| zone.keep_empty));
        wb.validate().unwrap();
    }

    #[test]
    fn empathy_map_has_independent_prompts_and_persistent_visual_sections() {
        use crate::board::{BoardMember, CardKind};

        let mut wb = instantiate("empathy-map");
        let sheet = wb.active();
        assert_eq!(
            sheet
                .zones
                .iter()
                .map(|zone| zone.name.as_str())
                .collect::<Vec<_>>(),
            ["Says", "Thinks", "Does", "Feels"]
        );
        assert!(
            sheet
                .zones
                .iter()
                .all(|zone| zone.keep_empty && zone.roots.is_empty())
        );
        assert!(sheet.canvas.groups.is_empty());
        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let heading = sheet
            .content_topics()
            .find(|topic| topic.title == "Empathy map: whose perspective?")
            .unwrap();
        assert_eq!(heading.card.unwrap().kind, CardKind::Text);
        assert!(layout.topics[&heading.id].rect.max().y < sheet.zones[0].frame.min.y);
        let mut stickies = Vec::new();
        for (zone, text) in sheet.zones.iter().zip([
            "What do they say?",
            "What might they think?",
            "What do they do?",
            "How might they feel?",
        ]) {
            let topic = sheet
                .content_topics()
                .find(|topic| topic.title == text)
                .unwrap();
            assert_eq!(topic.card.unwrap().kind, CardKind::Sticky);
            let rect = layout.topics[&topic.id].rect;
            assert!(zone.frame.contains(rect.min) && zone.frame.contains(rect.max()));
            stickies.push(topic.id);
        }
        assert_eq!(sheet.content_topics().count(), 5);
        let from = sheet.topic(stickies[0]).unwrap().position.unwrap();
        let target = sheet.zones[1].frame.max() - crate::Vec2::new(90.0, 90.0);
        let sheet = wb.active_mut();
        sheet
            .move_board_members(&[BoardMember::Card(stickies[0])], target - from)
            .unwrap();
        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let moved = layout.topics[&stickies[0]].rect;
        assert!(sheet.zones[1].frame.contains(moved.min));
        assert!(sheet.zones[1].frame.contains(moved.max()));
        assert!(sheet.zones.iter().all(|zone| zone.roots.is_empty()));
        for id in stickies {
            assert!(sheet.detach_subtree(id).is_some());
        }
        assert_eq!(sheet.zones.len(), 4);
        assert!(sheet.zones.iter().all(|zone| zone.keep_empty));
        wb.validate().unwrap();
    }

    #[test]
    fn swimlane_has_editable_handoffs_between_persistent_visual_lanes() {
        use crate::board::{BoardMember, CanvasObjectKind, CardKind, ConnectorRoute, Endpoint};
        use crate::geometry::{Rect, Vec2};

        let mut wb = instantiate("swimlane");
        let sheet = wb.active();
        assert_eq!(
            sheet
                .zones
                .iter()
                .map(|z| z.name.as_str())
                .collect::<Vec<_>>(),
            ["Role 1", "Role 2", "Role 3"]
        );
        assert!(
            sheet
                .zones
                .iter()
                .all(|zone| zone.keep_empty && zone.roots.is_empty())
        );
        assert!(sheet.canvas.groups.is_empty());
        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let heading = sheet.content_topics().next().unwrap();
        assert_eq!(heading.title, "Name roles and map the handoffs");
        assert_eq!(heading.card.unwrap().kind, CardKind::Text);
        assert!(layout.topics[&heading.id].rect.max().y < sheet.zones[0].frame.min.y);
        assert_eq!(sheet.content_topics().count(), 1);
        let shapes: Vec<_> = sheet
            .canvas
            .objects
            .iter()
            .filter(|object| matches!(object.kind, CanvasObjectKind::Shape { .. }))
            .collect();
        let links: Vec<_> = sheet
            .canvas
            .objects
            .iter()
            .filter_map(|object| match &object.kind {
                CanvasObjectKind::Connector(connector) => Some(connector),
                _ => None,
            })
            .collect();
        assert_eq!(shapes.len(), 5);
        assert_eq!(links.len(), 4);
        for (shape, lane) in shapes.iter().zip([0, 1, 2, 1, 0]) {
            let frame = sheet.zones[lane].frame;
            assert!(frame.contains(shape.rect.min) && frame.contains(shape.rect.max()));
            assert!(shape.group.is_none() && !shape.locked);
        }
        for (index, link) in links.iter().enumerate() {
            assert_eq!(link.from, Endpoint::Object(shapes[index].id));
            assert_eq!(link.to, Endpoint::Object(shapes[index + 1].id));
            assert_eq!(link.route, ConnectorRoute::Elbow);
            assert!(!link.arrow_start && link.arrow_end);
            let path = crate::scene::connector_path(sheet, &layout, link);
            assert_eq!(path.len(), 4);
            assert_eq!(path[0], shapes[index].rect.right_center());
            assert_eq!(path[3], shapes[index + 1].rect.left_center());
            for segment in path.windows(2) {
                assert!(segment[0].x == segment[1].x || segment[0].y == segment[1].y);
                let min = Vec2::new(
                    segment[0].x.min(segment[1].x),
                    segment[0].y.min(segment[1].y),
                );
                let max = Vec2::new(
                    segment[0].x.max(segment[1].x),
                    segment[0].y.max(segment[1].y),
                );
                let bounds = Rect::from_min_size(min, max - min);
                for (other, shape) in shapes.iter().enumerate() {
                    if other != index && other != index + 1 {
                        assert!(
                            !bounds.intersects(&shape.rect),
                            "link {index} crosses unrelated shape {other}"
                        );
                    }
                }
            }
        }

        let moving = shapes[0].id;
        let old_path = crate::scene::connector_path(sheet, &layout, links[0]);
        let sheet = wb.active_mut();
        sheet
            .move_board_members(&[BoardMember::Object(moving)], Vec2::new(0.0, 190.0))
            .unwrap();
        let moved = sheet
            .canvas
            .objects
            .iter()
            .find(|object| object.id == moving)
            .unwrap();
        assert!(sheet.zones[1].frame.contains(moved.rect.min));
        assert!(sheet.zones[1].frame.contains(moved.rect.max()));
        assert!(
            sheet
                .zones
                .iter()
                .all(|zone| zone.keep_empty && zone.roots.is_empty())
        );
        let link = sheet
            .canvas
            .objects
            .iter()
            .find_map(|object| match &object.kind {
                CanvasObjectKind::Connector(connector)
                    if connector.from == Endpoint::Object(moving) =>
                {
                    Some(connector)
                }
                _ => None,
            })
            .unwrap();
        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let moved_path = crate::scene::connector_path(sheet, &layout, link);
        assert_ne!(moved_path, old_path);
        assert_eq!(moved_path[0], moved.rect.right_center());

        let positions: Vec<_> = sheet
            .canvas
            .objects
            .iter()
            .map(|object| (object.id, object.rect))
            .collect();
        let lane = sheet.zones[0].id;
        sheet.rename_zone(lane, "Requester").unwrap();
        sheet.move_zone(lane, Vec2::new(0.0, 600.0)).unwrap();
        assert_eq!(sheet.zones[0].name, "Requester");
        assert_eq!(
            positions,
            sheet
                .canvas
                .objects
                .iter()
                .map(|object| (object.id, object.rect))
                .collect::<Vec<_>>()
        );
        wb.validate().unwrap();
    }

    #[test]
    fn venn_diagram_has_editable_overlapping_circles_and_independent_labels() {
        use crate::board::{Band, BoardMember, CanvasObjectKind, CardKind, ShapeKind};
        use crate::geometry::{Color, Vec2};

        let mut wb = instantiate("venn-diagram");
        let sheet = wb.active();
        assert!(sheet.zones.is_empty());
        assert_eq!(sheet.floating.len(), 4);
        assert_eq!(sheet.canvas.objects.len(), 2);
        let circles = &sheet.canvas.objects;
        assert!(circles[0].rect.intersects(&circles[1].rect));
        for (circle, description) in circles.iter().zip(["Set A", "Set B"]) {
            assert!(matches!(
                &circle.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::Ellipse,
                    label,
                } if label.is_empty()
            ));
            assert_eq!(circle.alt_text, description);
            assert_eq!(circle.band, Band::Below);
            assert_eq!(circle.style.opacity, 0.55);
            assert!(circle.style.fill.is_some());
        }

        let shared = sheet
            .content_topics()
            .find(|topic| topic.title == "Shared")
            .unwrap();
        assert_eq!(shared.card.unwrap().kind, CardKind::Sticky);
        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let shared_rect = layout.topics[&shared.id].rect;
        for circle in circles {
            let center = circle.rect.center();
            let radius = circle.rect.size.x / 2.0;
            for corner in [
                shared_rect.min,
                Vec2::new(shared_rect.max().x, shared_rect.min.y),
                shared_rect.max(),
                Vec2::new(shared_rect.min.x, shared_rect.max().y),
            ] {
                assert!((corner - center).length() < radius);
            }
        }
        let circle_id = circles[0].id;
        let card_positions: Vec<_> = sheet
            .floating
            .iter()
            .map(|id| (*id, sheet.topic(*id).unwrap().position))
            .collect();

        let sheet = wb.active_mut();
        sheet
            .move_board_members(&[BoardMember::Object(circle_id)], Vec2::new(-40.0, 0.0))
            .unwrap();
        sheet
            .update_canvas_object(circle_id, |circle| {
                circle.style.fill = Some(Color::rgb(40, 100, 220));
            })
            .unwrap();
        assert_eq!(sheet.canvas.object(circle_id).unwrap().style.opacity, 0.55);
        for (id, position) in card_positions {
            assert_eq!(sheet.topic(id).unwrap().position, position);
        }
        wb.validate().unwrap();
    }

    #[test]
    fn storyboard_has_editable_image_cards_and_independent_frames() {
        use crate::board::{BoardMember, CardKind};
        use crate::geometry::Vec2;

        let mut wb = instantiate("storyboard");
        let sheet = wb.active();
        assert_eq!(
            sheet
                .zones
                .iter()
                .map(|zone| zone.name.as_str())
                .collect::<Vec<_>>(),
            ["Scene 1", "Scene 2", "Scene 3"]
        );
        assert!(
            sheet
                .zones
                .iter()
                .all(|zone| zone.keep_empty && zone.roots.is_empty())
        );
        assert!(sheet.resources.is_empty());
        let layout = crate::layout_sheet(sheet, &crate::ApproxMeasure, false);
        let mut images = Vec::new();
        for (index, zone) in sheet.zones.iter().enumerate() {
            for (suffix, kind) in [("image", CardKind::Image), ("action", CardKind::Sticky)] {
                let title = format!("Scene {} {suffix}", index + 1);
                let topic = sheet
                    .content_topics()
                    .find(|topic| topic.title == title)
                    .unwrap();
                assert_eq!(topic.card.unwrap().kind, kind);
                assert!(topic.image.is_none());
                let rect = layout.topics[&topic.id].rect;
                assert!(zone.frame.contains(rect.min) && zone.frame.contains(rect.max()));
                if kind == CardKind::Image {
                    images.push(topic.id);
                }
            }
        }
        let original_frames = sheet.zones.clone();
        let other_positions: Vec<_> = sheet
            .floating
            .iter()
            .filter(|id| **id != images[0])
            .map(|id| (*id, sheet.topic(*id).unwrap().position))
            .collect();
        let original_position = sheet.topic(images[0]).unwrap().position.unwrap();
        let sheet = wb.active_mut();
        sheet
            .move_board_members(&[BoardMember::Card(images[0])], Vec2::new(380.0, 0.0))
            .unwrap();
        assert_eq!(
            sheet.topic(images[0]).unwrap().position,
            Some(original_position + Vec2::new(380.0, 0.0))
        );
        assert_eq!(sheet.zones, original_frames);
        for (id, position) in other_positions {
            assert_eq!(sheet.topic(id).unwrap().position, position);
        }
        wb.validate().unwrap();
    }

    #[test]
    fn additional_board_templates_round_trip_with_localized_content() {
        for id in [
            "affinity-diagram",
            "eisenhower-matrix",
            "meeting-board",
            "swimlane",
            "venn-diagram",
            "empathy-map",
            "storyboard",
        ] {
            let original = instantiate_localized(id, &|key| format!("<{key}>"));
            original.validate().unwrap();
            let json = serde_json::to_string(&original).unwrap();
            let restored: Workbook = serde_json::from_str(&json).unwrap();
            restored.validate().unwrap();
            assert_eq!(original, restored, "{id}");
            assert!(restored.active().title.starts_with('<'));
            assert!(
                restored
                    .active()
                    .zones
                    .iter()
                    .all(|zone| zone.keep_empty && zone.roots.is_empty())
            );
        }
    }

    #[test]
    fn board_content_is_localized_through_the_translator() {
        let strings = board_strings();
        assert!(strings.contains(&"Key Partners"));
        assert!(strings.contains(&"Yes"));
        for key in [
            "Group related ideas, then name each theme",
            "An idea or observation",
            "Ideas to sort",
            "Theme 1",
            "Theme 2",
            "Theme 3",
            "Do: urgent and important",
            "Schedule: important, not urgent",
            "Delegate: urgent, not important",
            "Eliminate: neither urgent nor important",
            "Meeting: purpose and date",
            "Agenda",
            "Notes",
            "Decisions",
            "Actions",
            "Topic and timebox",
            "Key points and questions",
            "Decision and reason",
            "Action, owner and due date",
            "Name roles and map the handoffs",
            "Role 1",
            "Role 2",
            "Role 3",
            "Compare two sets",
            "Set A",
            "Set B",
            "Shared",
            "Empathy map: whose perspective?",
            "Says",
            "Thinks",
            "Does",
            "Feels",
            "What do they say?",
            "What might they think?",
            "What do they do?",
            "How might they feel?",
            "Select an image card, then choose Local Image",
            "Scene 1",
            "Scene 2",
            "Scene 3",
            "Scene 1 image",
            "Scene 2 image",
            "Scene 3 image",
            "Scene 1 action",
            "Scene 2 action",
            "Scene 3 action",
        ] {
            assert!(
                strings.contains(&key),
                "{key} is missing from board strings"
            );
        }
        for id in board_ids() {
            let wb = instantiate_localized(id, &|s| format!("<{s}>"));
            wb.validate().unwrap();
            let sheet = wb.active();
            let info = TEMPLATES.iter().find(|info| info.id == id).unwrap();
            assert_eq!(sheet.title, format!("<{}>", info.name));
            assert!(sheet.root_topic().title.is_empty());
            for topic in sheet.content_topics() {
                assert!(topic.title.starts_with('<'), "{id}: {}", topic.title);
            }
            for zone in &sheet.zones {
                assert!(zone.name.starts_with('<'), "{id}: {}", zone.name);
            }
            for object in &sheet.canvas.objects {
                match &object.kind {
                    crate::board::CanvasObjectKind::Shape { label, .. } => {
                        assert!(label.is_empty() || label.starts_with('<'));
                        assert!(object.alt_text.is_empty() || object.alt_text.starts_with('<'));
                    }
                    crate::board::CanvasObjectKind::Connector(c) => {
                        assert!(c.label.is_empty() || c.label.starts_with('<'))
                    }
                    _ => {}
                }
            }
        }
        // Mind-map templates keep their original English content.
        let swot = instantiate_localized("swot", &|s| format!("<{s}>"));
        assert_eq!(swot, instantiate("swot"));
    }
}
