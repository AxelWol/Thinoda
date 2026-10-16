//! # ymind-core
//!
//! UI-independent heart of yMind:
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
pub mod document;
pub mod edit;
pub mod geometry;
pub mod layout;
pub mod markers;
pub mod model;
pub mod navigation;
pub mod outline;
pub mod search;
pub mod templates;
pub mod theme;

pub use document::Document;
pub use edit::{EditError, EditResult};
pub use geometry::{Color, Rect, Vec2};
pub use layout::{ApproxMeasure, Layout, TextMeasure, TopicLayout, layout_sheet};
pub use model::{
    Boundary, BoundaryId, BranchSide, ChildRange, LineShape, Relationship, RelationshipId, Sheet,
    Structure, Subtree, Summary, SummaryId, TextAlign, Topic, TopicId, TopicShape, TopicStyle,
    Workbook,
};
pub use outline::OutlineNode;
