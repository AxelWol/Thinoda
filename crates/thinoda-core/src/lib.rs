//! # thinoda-core
//!
//! UI-independent heart of Thinoda:
//!
//! * [`model`] – workbook / sheet / topic data model (XMind compatible concepts)
//! * [`document`] – snapshot based undo/redo history (operations live in [`edit`])
//! * [`layout`] – structure layouts (mind map, logic chart, org chart, tree, timeline)
//! * [`navigation`], [`search`], [`outline`], [`clipboard`] – editing helpers
//! * [`markers`], [`theme`], [`templates`] – built-in catalogs
//!
//! The crate has no GUI dependency so it can be reused by companion apps,
//! command line tools and servers.

pub mod clipboard;
pub mod custom_theme;
pub mod document;
pub mod edit;
pub mod geometry;
pub mod images;
pub mod layout;
pub mod markers;
pub mod model;
pub mod navigation;
pub mod numbering;
pub mod outline;
pub mod quick_styles;
pub mod resources;
pub mod schedule;
pub mod search;
pub mod templates;
pub mod theme;
pub mod todo;

pub use document::Document;
pub use edit::{EditError, EditResult};
pub use geometry::{Color, Rect, Vec2};
pub use layout::{ApproxMeasure, Layout, TextMeasure, TopicLayout, layout_sheet};
pub use model::{
    Boundary, BoundaryId, BranchSide, ChildRange, LineShape, MAX_PRESENTATION_COORDINATE,
    MAX_PRESENTATION_NAME_BYTES, MAX_PRESENTATION_SLIDES, MAX_ZONE_MEMBERSHIPS, MAX_ZONES,
    MIN_ZONE_SIZE, Presentation, PresentationSlide, PresentationSource, Relationship,
    RelationshipId, Sheet, SlideFrame, SlideId, Structure, Subtree, Summary, SummaryId, TextAlign,
    Topic, TopicId, TopicShape, TopicStyle, Workbook, Zone, ZoneId,
};
pub use outline::OutlineNode;
pub use schedule::{
    ComputedTask, LocalDate, MAX_CALENDAR_EXCEPTIONS, MAX_SCHEDULED_TASKS, MAX_TASK_DEPENDENCIES,
    MAX_TASK_DURATION_DAYS, SCHEDULING_VERSION, ScheduleError, SchedulingData, Task,
    TaskDependency, Weekday, WorkCalendar,
};
