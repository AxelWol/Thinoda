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
}
