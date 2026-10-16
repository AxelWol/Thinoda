//! Editing actions shared by keyboard shortcuts, menus, toolbars and the command palette.

use crate::view::{MapView, ObjectSelection};
use ymind_core::{
    Document, EditError, TopicId, clipboard,
    geometry::Vec2,
    navigation::{Direction, navigate},
};

/// A user-level command on the map.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    InsertChild,
    InsertSibling,
    InsertSiblingBefore,
    InsertParent,
    InsertFloating(Vec2),
    Delete,
    DeleteKeepChildren,
    Duplicate,
    StartEdit,
    ToggleCollapse,
    ExpandAll,
    CollapseAll,
    /// Expands everything if any branch is folded, otherwise folds all branches (Ctrl+Alt+/).
    ToggleCollapseAll,
    Navigate(Direction),
    MoveUp,
    MoveDown,
    Indent,
    Outdent,
    SelectAll,
    SelectRoot,
    SelectNone,
    StartRelationship,
    AddBoundary,
    AddSummary,
    ToggleTodo,
    ToggleMarker(String),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Fit,
    Undo,
    Redo,
    Copy,
    Cut,
    /// Paste with the given OS clipboard text (if any).
    Paste(Option<String>),
}

impl Action {
    /// Whether the action requires at least one selected topic.
    pub fn needs_selection(&self) -> bool {
        !matches!(
            self,
            Action::InsertFloating(_)
                | Action::SelectAll
                | Action::SelectRoot
                | Action::SelectNone
                | Action::ZoomIn
                | Action::ZoomOut
                | Action::ZoomReset
                | Action::Fit
                | Action::Undo
                | Action::Redo
                | Action::Paste(_)
                | Action::ExpandAll
                | Action::CollapseAll
                | Action::ToggleCollapseAll
        )
    }
}

/// Applies `action`. Returns `Ok(true)` when something happened.
pub fn apply_action(
    doc: &mut Document,
    view: &mut MapView,
    action: Action,
) -> Result<bool, EditError> {
    view.prune_selection(doc.sheet());
    if action.needs_selection()
        && view.selection.is_empty()
        && !(action == Action::Delete && view.selected_object.is_some())
    {
        if matches!(action, Action::Navigate(_)) {
            view.select_only(doc.sheet().root);
            return Ok(true);
        }
        return Ok(false);
    }
    let primary = view.selection.last().copied();
    let sel = view.selection.clone();
    let titles = view.titles.clone();
    let child_title = |sheet: &ymind_core::Sheet, parent: TopicId| {
        if parent == sheet.root {
            titles.main.clone()
        } else {
            titles.sub.clone()
        }
    };
    match action {
        Action::InsertChild => {
            let p = primary.unwrap();
            let title = child_title(doc.sheet(), p);
            let id = doc.edit_sheet("Insert Topic", |s| {
                if s.topic(p).is_some_and(|t| t.collapsed) {
                    s.set_collapsed(p, false)?;
                }
                s.insert_child(p, &title)
            })?;
            view.select_only(id);
            view.begin_edit(doc.sheet(), id, true);
        }
        Action::InsertSibling | Action::InsertSiblingBefore => {
            let p = primary.unwrap();
            let sheet = doc.sheet();
            let Some(parent) = sheet.parent(p) else {
                // Root or floating: XMind inserts a child instead.
                return apply_action(doc, view, Action::InsertChild);
            };
            let title = child_title(sheet, parent);
            let before = action == Action::InsertSiblingBefore;
            let id = doc.edit_sheet("Insert Topic", |s| {
                if before {
                    s.insert_sibling_before(p, &title)
                } else {
                    s.insert_sibling_after(p, &title)
                }
            })?;
            view.select_only(id);
            view.begin_edit(doc.sheet(), id, true);
        }
        Action::InsertParent => {
            let p = primary.unwrap();
            let title = titles.main.clone();
            let id = doc.edit_sheet("Insert Parent Topic", |s| s.insert_parent(p, &title))?;
            view.select_only(id);
            view.begin_edit(doc.sheet(), id, true);
        }
        Action::InsertFloating(pos) => {
            let title = titles.floating.clone();
            let id = doc.edit_sheet("Insert Floating Topic", |s| Ok(s.add_floating(title, pos)))?;
            view.select_only(id);
            view.begin_edit(doc.sheet(), id, true);
        }
        Action::Delete => {
            if let Some(object) = view.selected_object {
                match object {
                    ObjectSelection::Relationship(id) => {
                        doc.edit_sheet("Delete Relationship", |s| s.remove_relationship(id))?;
                    }
                    ObjectSelection::Boundary(id) => {
                        doc.edit_sheet("Delete Boundary", |s| s.remove_boundary(id))?;
                    }
                    ObjectSelection::Summary(id) => {
                        doc.edit_sheet("Delete Summary", |s| s.remove_summary(id))?;
                    }
                }
                view.selected_object = None;
                return Ok(true);
            }
            let next = doc.edit_sheet("Delete", |s| s.remove_topics(&sel))?;
            view.selection = next.into_iter().collect();
        }
        Action::DeleteKeepChildren => {
            let p = primary.unwrap();
            let parent = doc.sheet().parent(p);
            doc.edit_sheet("Delete Topic Only", |s| s.remove_topic_keep_children(p))?;
            view.selection = parent.into_iter().collect();
        }
        Action::Duplicate => {
            let p = primary.unwrap();
            let id = doc.edit_sheet("Duplicate", |s| s.duplicate(p))?;
            view.select_only(id);
        }
        Action::StartEdit => {
            let p = primary.unwrap();
            view.begin_edit(doc.sheet(), p, true);
        }
        Action::ToggleCollapse => {
            let ids: Vec<TopicId> = sel
                .iter()
                .copied()
                .filter(|id| !doc.sheet().children(*id).is_empty())
                .collect();
            if ids.is_empty() {
                return Ok(false);
            }
            let collapse = !doc.sheet().topic(ids[0]).is_some_and(|t| t.collapsed);
            doc.edit_sheet(if collapse { "Collapse" } else { "Expand" }, |s| {
                for id in &ids {
                    s.set_collapsed(*id, collapse)?;
                }
                Ok(())
            })?;
        }
        Action::ToggleCollapseAll => {
            let sheet = doc.sheet();
            let any_folded = sheet
                .all_ids_ordered()
                .iter()
                .any(|id| sheet.topic(*id).is_some_and(|t| t.collapsed));
            let targets = sheet.top_level_ids();
            doc.edit_sheet(
                if any_folded {
                    "Expand All"
                } else {
                    "Collapse All"
                },
                |s| {
                    for id in &targets {
                        s.set_collapsed_all(*id, !any_folded)?;
                    }
                    Ok(())
                },
            )?;
            if !any_folded {
                let sheet = doc.sheet();
                view.selection.retain(|id| sheet.is_visible(*id));
            }
        }
        Action::ExpandAll | Action::CollapseAll => {
            let collapse = action == Action::CollapseAll;
            let targets: Vec<TopicId> = if sel.is_empty() {
                doc.sheet().top_level_ids()
            } else {
                sel.clone()
            };
            doc.edit_sheet(
                if collapse {
                    "Collapse All"
                } else {
                    "Expand All"
                },
                |s| {
                    for id in &targets {
                        s.set_collapsed_all(*id, collapse)?;
                    }
                    Ok(())
                },
            )?;
            if collapse {
                // Keep the selection on visible topics.
                let sheet = doc.sheet();
                view.selection.retain(|id| sheet.is_visible(*id));
            }
        }
        Action::Navigate(dir) => {
            let p = primary.unwrap();
            let layout = view.layout_for(doc);
            if let Some(n) = navigate(doc.sheet(), &layout, p, dir) {
                view.select_only(n);
                view.reveal(n);
            } else {
                return Ok(false);
            }
        }
        Action::MoveUp | Action::MoveDown => {
            let p = primary.unwrap();
            let d = if action == Action::MoveUp { -1 } else { 1 };
            doc.edit_sheet("Move Topic", |s| s.reorder(p, d))?;
        }
        Action::Indent | Action::Outdent => {
            if sel.len() != 1 {
                return Err(EditError::Invalid(
                    "select one topic to indent or outdent".into(),
                ));
            }
            let topic = primary.unwrap();
            doc.edit_sheet(
                if action == Action::Indent {
                    "Indent"
                } else {
                    "Outdent"
                },
                |s| {
                    if action == Action::Indent {
                        s.indent(topic)
                    } else {
                        s.outdent(topic)
                    }
                },
            )?;
            view.reveal(topic);
        }
        Action::SelectAll => {
            view.selected_object = None;
            let sheet = doc.sheet();
            view.selection = sheet
                .all_ids_ordered()
                .into_iter()
                .filter(|id| sheet.is_visible(*id))
                .collect();
        }
        Action::SelectRoot => {
            let r = doc.sheet().root;
            view.select_only(r);
            view.reveal(r);
        }
        Action::SelectNone => {
            view.selection.clear();
            view.selected_object = None;
            view.relationship_from = None;
        }
        Action::StartRelationship => {
            view.relationship_from = primary;
        }
        Action::AddBoundary => {
            doc.edit_sheet("Add Boundary", |s| s.add_boundary(&sel))?;
        }
        Action::AddSummary => {
            let title = titles.summary.clone();
            let id = doc.edit_sheet("Add Summary", |s| s.add_summary(&sel, &title))?;
            view.select_only(id);
            view.begin_edit(doc.sheet(), id, true);
        }
        Action::ToggleTodo => {
            let sheet = doc.sheet();
            let next = match sheet.topic(primary.unwrap()).and_then(|t| t.todo) {
                None => Some(false),
                Some(false) => Some(true),
                Some(true) => None,
            };
            doc.edit_sheet("To-Do", |s| {
                for id in &sel {
                    s.set_todo(*id, next)?;
                }
                Ok(())
            })?;
        }
        Action::ToggleMarker(m) => {
            doc.edit_sheet("Marker", |s| s.toggle_marker(&sel, &m))?;
        }
        Action::ZoomIn => view.zoom_by(1.2, None),
        Action::ZoomOut => view.zoom_by(1.0 / 1.2, None),
        Action::ZoomReset => view.set_zoom(1.0),
        Action::Fit => view.request_fit(),
        Action::Undo => {
            view.cancel_edit();
            return Ok(doc.undo());
        }
        Action::Redo => {
            view.cancel_edit();
            return Ok(doc.redo());
        }
        Action::Copy | Action::Cut => {
            let Some(payload) = clipboard::copy_topics(doc.sheet(), &sel) else {
                return Ok(false);
            };
            view.clipboard_out = Some(payload.text.clone());
            view.clipboard = Some(payload);
            if action == Action::Cut {
                let roots: Vec<TopicId> = sel
                    .iter()
                    .copied()
                    .filter(|id| *id != doc.sheet().root)
                    .collect();
                if !roots.is_empty() {
                    let next = doc.edit_sheet("Cut", |s| s.remove_topics(&roots))?;
                    view.selection = next.into_iter().collect();
                }
            }
        }
        Action::Paste(text) => {
            let subtrees = clipboard::resolve_paste(view.clipboard.as_ref(), text.as_deref());
            if subtrees.is_empty() {
                return Ok(false);
            }
            let target = primary.unwrap_or(doc.sheet().root);
            let ids = doc.edit_sheet("Paste", |s| s.paste(target, &subtrees))?;
            view.selection = ids;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ymind_core::Workbook;

    fn setup() -> (Document, MapView) {
        let doc = Document::new(Workbook::default());
        let mut view = MapView::default();
        view.select_only(doc.sheet().root);
        (doc, view)
    }

    #[test]
    fn insert_child_and_sibling() {
        let (mut doc, mut view) = setup();
        let root = doc.sheet().root;
        let n = doc.sheet().children(root).len();
        apply_action(&mut doc, &mut view, Action::InsertChild).unwrap();
        assert_eq!(doc.sheet().children(root).len(), n + 1);
        let new = view.selection[0];
        assert_eq!(doc.sheet().topic(new).unwrap().title, "Main Topic");
        assert_eq!(view.editing_topic(), Some(new));
        view.commit_edit(&mut doc);
        apply_action(&mut doc, &mut view, Action::InsertChild).unwrap();
        let sub = view.selection[0];
        assert_eq!(doc.sheet().topic(sub).unwrap().title, "Subtopic");
        apply_action(&mut doc, &mut view, Action::InsertSibling).unwrap();
        assert_eq!(doc.sheet().parent(view.selection[0]), Some(new));
    }

    #[test]
    fn structural_objects_delete_without_topic_selection_and_undo_in_one_step() {
        for kind in 0..3 {
            let (mut doc, mut view) = setup();
            let children = doc.sheet().children(doc.sheet().root).to_vec();
            let object = doc
                .edit_sheet("Annotation", |s| {
                    Ok(match kind {
                        0 => ObjectSelection::Relationship(
                            s.add_relationship(children[0], children[1])?,
                        ),
                        1 => ObjectSelection::Boundary(s.add_boundary(&children[..2])?),
                        _ => {
                            let topic = s.add_summary(&children[..2], "Summary")?;
                            s.add_child(topic, None, "Summary child");
                            ObjectSelection::Summary(s.summaries[0].id)
                        }
                    })
                })
                .unwrap();
            let before = doc.sheet().clone();
            view.select_object(object);
            assert!(view.selection.is_empty());
            assert!(apply_action(&mut doc, &mut view, Action::Delete).unwrap());
            assert!(!object.exists(doc.sheet()));
            assert!(view.selected_object.is_none());
            for topic in &children {
                assert_eq!(doc.sheet().topic(*topic), before.topic(*topic));
            }
            doc.sheet().validate().unwrap();
            assert!(apply_action(&mut doc, &mut view, Action::Undo).unwrap());
            assert_eq!(doc.sheet(), &before);
            assert!(apply_action(&mut doc, &mut view, Action::Redo).unwrap());
            assert!(!object.exists(doc.sheet()));
            doc.sheet().validate().unwrap();
        }
    }

    #[test]
    fn stale_object_selection_cannot_delete_topics_or_another_sheet_object() {
        let (mut doc, mut view) = setup();
        let object = doc
            .edit_sheet("Boundary", |s| s.add_boundary(&[s.children(s.root)[0]]))
            .unwrap();
        view.select_object(ObjectSelection::Boundary(object));
        doc.add_sheet(ymind_core::Sheet::new("Other", "Root"));
        doc.set_active(1);
        let before = doc.sheet().clone();
        assert!(!apply_action(&mut doc, &mut view, Action::Delete).unwrap());
        assert!(view.selected_object.is_none());
        assert_eq!(doc.sheet(), &before);
    }

    #[test]
    fn delete_and_undo() {
        let (mut doc, mut view) = setup();
        let root = doc.sheet().root;
        let first = doc.sheet().children(root)[0];
        view.select_only(first);
        apply_action(&mut doc, &mut view, Action::Delete).unwrap();
        assert!(doc.sheet().topic(first).is_none());
        assert_eq!(view.selection.len(), 1);
        apply_action(&mut doc, &mut view, Action::Undo).unwrap();
        assert!(doc.sheet().topic(first).is_some());
    }

    #[test]
    fn navigate_and_collapse() {
        let (mut doc, mut view) = setup();
        let root = doc.sheet().root;
        apply_action(&mut doc, &mut view, Action::Navigate(Direction::Right)).unwrap();
        let sel = view.selection[0];
        assert_eq!(doc.sheet().parent(sel), Some(root));
        apply_action(&mut doc, &mut view, Action::Navigate(Direction::Left)).unwrap();
        assert_eq!(view.selection[0], root);
        view.select_only(sel);
        apply_action(&mut doc, &mut view, Action::InsertChild).unwrap();
        view.commit_edit(&mut doc);
        view.select_only(sel);
        apply_action(&mut doc, &mut view, Action::ToggleCollapse).unwrap();
        assert!(doc.sheet().topic(sel).unwrap().collapsed);
    }

    #[test]
    fn copy_paste_roundtrip() {
        let (mut doc, mut view) = setup();
        let root = doc.sheet().root;
        let first = doc.sheet().children(root)[0];
        let n = doc.sheet().topics.len();
        view.select_only(first);
        apply_action(&mut doc, &mut view, Action::Copy).unwrap();
        let text = view.clipboard_out.take();
        assert!(text.is_some());
        view.select_only(root);
        apply_action(&mut doc, &mut view, Action::Paste(text)).unwrap();
        assert_eq!(doc.sheet().topics.len(), n + 1);
        // Plain text from other apps becomes topics.
        apply_action(&mut doc, &mut view, Action::Paste(Some("A\n\tB\nC".into()))).unwrap();
        assert_eq!(doc.sheet().topics.len(), n + 4);
    }

    #[test]
    fn todo_cycle_and_marker() {
        let (mut doc, mut view) = setup();
        let root = doc.sheet().root;
        for expected in [Some(false), Some(true), None] {
            apply_action(&mut doc, &mut view, Action::ToggleTodo).unwrap();
            assert_eq!(doc.sheet().topic(root).unwrap().todo, expected);
        }
        apply_action(
            &mut doc,
            &mut view,
            Action::ToggleMarker("priority-1".into()),
        )
        .unwrap();
        assert_eq!(
            doc.sheet().topic(root).unwrap().markers,
            vec!["priority-1".to_string()]
        );
    }

    #[test]
    fn floating_and_relationship_actions() {
        let (mut doc, mut view) = setup();
        apply_action(
            &mut doc,
            &mut view,
            Action::InsertFloating(Vec2::new(400.0, 300.0)),
        )
        .unwrap();
        let f = view.selection[0];
        assert!(doc.sheet().is_floating(f));
        view.commit_edit(&mut doc);
        apply_action(&mut doc, &mut view, Action::StartRelationship).unwrap();
        assert_eq!(view.relationship_from, Some(f));
    }
}
