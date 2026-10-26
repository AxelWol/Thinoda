//! Editing actions shared by keyboard shortcuts, menus, toolbars and the command palette.

use crate::view::{MapView, ObjectSelection};
use thinoda_core::{
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
    SelectSiblings,
    SelectDescendants,
    InvertSelection,
    SortAscending,
    SortDescending,
    AlignFloating(FloatingArrangement),
    StartRelationship,
    AddBoundary,
    AddSummary,
    ToggleTodo,
    ToggleMarker(String),
    ToggleMarkerColor(String, Option<thinoda_core::Color>),
    SetMarkerColor(TopicId, usize, Option<thinoda_core::Color>),
    RemoveMarker(TopicId, usize),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatingArrangement {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
    DistributeHorizontal,
    DistributeVertical,
}

impl Action {
    /// Shared contextual availability for commands surfaced by the host.
    pub fn available(&self, doc: &Document, view: &MapView) -> bool {
        let sheet = doc.sheet();
        let valid =
            !view.selection.is_empty() && view.selection.iter().all(|id| sheet.contains(*id));
        let primary = view.primary().filter(|id| sheet.contains(*id));
        match self {
            Self::Undo => doc.can_undo(),
            Self::Redo => doc.can_redo(),
            Self::Delete => {
                view.selected_object
                    .is_some_and(|object| object.exists(sheet))
                    || (valid && view.selection.iter().any(|id| *id != sheet.root))
            }
            Self::DeleteKeepChildren | Self::InsertParent => {
                valid && primary.is_some_and(|id| sheet.parent(id).is_some())
            }
            Self::Indent | Self::MoveUp => {
                valid
                    && primary
                        .and_then(|id| sheet.index_in_parent(id))
                        .is_some_and(|index| index > 0)
                    && (*self != Self::Indent || view.selection.len() == 1)
            }
            Self::Outdent => {
                valid
                    && view.selection.len() == 1
                    && primary
                        .and_then(|id| sheet.parent(id))
                        .and_then(|id| sheet.parent(id))
                        .is_some()
            }
            Self::MoveDown => {
                valid
                    && primary.is_some_and(|id| {
                        sheet
                            .parent(id)
                            .zip(sheet.index_in_parent(id))
                            .is_some_and(|(parent, index)| index + 1 < sheet.children(parent).len())
                    })
            }
            Self::SortAscending | Self::SortDescending => {
                valid
                    && view.selection.len() == 1
                    && primary.is_some_and(|id| sheet.children(id).len() > 1)
            }
            Self::SelectSiblings => valid && primary.and_then(|id| sheet.parent(id)).is_some(),
            Self::SelectDescendants | Self::ToggleCollapse => {
                valid
                    && view
                        .selection
                        .iter()
                        .any(|id| !sheet.children(*id).is_empty())
            }
            Self::AlignFloating(arrangement) => {
                let minimum = if matches!(
                    arrangement,
                    FloatingArrangement::DistributeHorizontal
                        | FloatingArrangement::DistributeVertical
                ) {
                    3
                } else {
                    2
                };
                valid
                    && view.selection.len() >= minimum
                    && view.selection.iter().all(|id| sheet.is_floating(*id))
            }
            Self::AddBoundary if valid && view.selection.len() == 1 => true,
            Self::AddSummary | Self::AddBoundary => {
                valid && {
                    let parent = sheet.parent(view.selection[0]);
                    parent.is_some() && view.selection.iter().all(|id| sheet.parent(*id) == parent)
                }
            }
            Self::Navigate(_) => true,
            _ => !self.needs_selection() || valid,
        }
    }

    /// Whether the action requires at least one selected topic.
    pub fn needs_selection(&self) -> bool {
        !matches!(
            self,
            Action::InsertFloating(_)
                | Action::SelectAll
                | Action::SelectRoot
                | Action::SelectNone
                | Action::InvertSelection
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
    let child_title = |sheet: &thinoda_core::Sheet, parent: TopicId| {
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
                    ObjectSelection::Zone(id) => {
                        doc.remove_zone(id)?;
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
        Action::SelectSiblings | Action::SelectDescendants | Action::InvertSelection => {
            if !action.available(doc, view) {
                return Ok(false);
            }
            let sheet = doc.sheet();
            let ids = match action {
                Action::SelectSiblings => sheet
                    .children(sheet.parent(primary.unwrap()).unwrap())
                    .to_vec(),
                Action::SelectDescendants => sel
                    .iter()
                    .flat_map(|id| sheet.subtree_ids(*id).into_iter().skip(1))
                    .collect(),
                _ => sheet
                    .all_ids_ordered()
                    .into_iter()
                    .filter(|id| !sel.contains(id))
                    .collect(),
            };
            let mut seen = std::collections::HashSet::new();
            view.selection = ids
                .into_iter()
                .filter(|id| sheet.is_visible(*id) && seen.insert(*id))
                .collect();
            view.selected_object = None;
            view.relationship_from = None;
        }
        Action::SortAscending | Action::SortDescending => {
            if !action.available(doc, view) {
                return Ok(false);
            }
            doc.edit_sheet("Sort Topics", |sheet| {
                sheet.sort_children(primary.unwrap(), action == Action::SortDescending)
            })?;
        }
        Action::AlignFloating(arrangement) => {
            if !action.available(doc, view) {
                return Err(EditError::Invalid(
                    "select at least two floating topics (three for distribution)".into(),
                ));
            }
            let layout = view.layout_for(doc);
            let mut rects: Vec<_> = sel
                .iter()
                .map(|id| {
                    layout
                        .topics
                        .get(id)
                        .map(|topic| (*id, topic.rect))
                        .ok_or(EditError::NoSuchTopic(*id))
                })
                .collect::<Result<_, _>>()?;
            let bounds = rects
                .iter()
                .map(|(_, rect)| *rect)
                .reduce(|a, b| a.union(&b))
                .unwrap();
            let horizontal = matches!(arrangement, FloatingArrangement::DistributeHorizontal);
            let distribute = horizontal || arrangement == FloatingArrangement::DistributeVertical;
            let coordinate = |rect: &thinoda_core::Rect| {
                if horizontal {
                    rect.center().x
                } else {
                    rect.center().y
                }
            };
            if distribute {
                rects.sort_by(|a, b| {
                    coordinate(&a.1)
                        .total_cmp(&coordinate(&b.1))
                        .then(a.0.cmp(&b.0))
                });
            }
            let start = coordinate(&rects[0].1);
            let end = coordinate(&rects.last().unwrap().1);
            let step = (end - start) / (rects.len() - 1) as f32;
            let positions: Vec<_> = rects
                .into_iter()
                .enumerate()
                .map(|(index, (id, rect))| {
                    let mut center = rect.center();
                    match arrangement {
                        FloatingArrangement::Left => center.x = bounds.min.x + rect.size.x / 2.0,
                        FloatingArrangement::HorizontalCenter => center.x = bounds.center().x,
                        FloatingArrangement::Right => center.x = bounds.max().x - rect.size.x / 2.0,
                        FloatingArrangement::Top => center.y = bounds.min.y + rect.size.y / 2.0,
                        FloatingArrangement::VerticalCenter => center.y = bounds.center().y,
                        FloatingArrangement::Bottom => {
                            center.y = bounds.max().y - rect.size.y / 2.0
                        }
                        _ => {
                            let target = if index + 1 == sel.len() {
                                end
                            } else {
                                start + index as f32 * step
                            };
                            if horizontal {
                                center.x = target;
                            } else {
                                center.y = target;
                            }
                        }
                    }
                    let position = doc
                        .sheet()
                        .topic(id)
                        .and_then(|topic| topic.position)
                        .unwrap();
                    (id, position + (center - rect.center()))
                })
                .collect();
            doc.edit_sheet("Arrange Floating Topics", |sheet| {
                for (id, position) in positions {
                    sheet.set_position(id, position)?;
                }
                Ok(())
            })?;
        }
        Action::StartRelationship => {
            if let [from, to] = sel.as_slice() {
                let id = doc.edit_sheet("Add Relationship", |s| s.add_relationship(*from, *to))?;
                view.select_object(ObjectSelection::Relationship(id));
            } else {
                view.relationship_from = primary;
            }
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
            let progress = sheet.todo_progress();
            let current = sheet
                .topic(primary.unwrap())
                .and_then(|t| t.todo)
                .map(|done| {
                    progress
                        .get(&primary.unwrap())
                        .map_or(done, |p| p.complete())
                });
            let next = match current {
                None => Some(false),
                Some(false) => Some(true),
                Some(true) => None,
            };
            doc.edit_sheet("To-Do", |s| {
                for id in &sel {
                    s.set_todo(*id, next)?;
                    if next == Some(true) && progress.get(id).is_some_and(|p| p.nested) {
                        s.set_todo_subtree(*id, true)?;
                    }
                }
                Ok(())
            })?;
        }
        Action::ToggleMarker(m) => {
            doc.edit_sheet("Marker", |s| s.toggle_marker(&sel, &m))?;
        }
        Action::ToggleMarkerColor(m, color) => {
            doc.edit_sheet("Symbol color", |s| s.toggle_marker_color(&sel, &m, color))?;
        }
        Action::SetMarkerColor(id, index, color) => {
            doc.edit_sheet("Symbol color", |s| s.set_marker_color(id, index, color))?;
        }
        Action::RemoveMarker(id, index) => {
            doc.edit_sheet("Remove Symbol", |s| s.remove_marker(id, index))?;
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
    use thinoda_core::Workbook;

    fn setup() -> (Document, MapView) {
        let doc = Document::new(Workbook::default());
        let mut view = MapView::default();
        view.select_only(doc.sheet().root);
        (doc, view)
    }

    #[test]
    fn selection_commands_respect_visibility_and_do_not_edit_the_document() {
        let (mut doc, mut view) = setup();
        let root = doc.sheet().root;
        let kids = doc.sheet().children(root).to_vec();
        let (child, hidden, summary, floating) = doc
            .edit_sheet("Prepare", |s| {
                let child = s.insert_child(kids[0], "child")?;
                let hidden = s.insert_child(child, "hidden")?;
                s.set_collapsed(child, true)?;
                let summary = s.add_summary(&kids[..2], "Summary")?;
                let floating = s.add_floating("Floating", Vec2::new(200.0, 400.0));
                Ok((child, hidden, summary, floating))
            })
            .unwrap();
        let before = doc.workbook().clone();
        let state = doc.state_id();
        view.select_only(kids[0]);
        apply_action(&mut doc, &mut view, Action::SelectDescendants).unwrap();
        assert_eq!(view.selection, vec![child]);
        assert!(!view.selection.contains(&hidden));
        view.select_only(kids[1]);
        apply_action(&mut doc, &mut view, Action::SelectSiblings).unwrap();
        assert_eq!(view.selection, kids);
        apply_action(&mut doc, &mut view, Action::InvertSelection).unwrap();
        assert!(view.selection.contains(&root));
        assert!(view.selection.contains(&summary));
        assert!(view.selection.contains(&floating));
        assert!(!view.selection.contains(&hidden));
        assert!(view.selection.iter().all(|id| !kids.contains(id)));
        apply_action(&mut doc, &mut view, Action::SelectNone).unwrap();
        assert!(view.selection.is_empty());
        assert!(!Action::SelectDescendants.available(&doc, &view));
        assert_eq!(doc.workbook(), &before);
        assert_eq!(doc.state_id(), state);
    }

    #[test]
    fn floating_alignment_and_distribution_match_exact_geometry_and_undo() {
        use FloatingArrangement::*;
        for arrangement in [
            Left,
            HorizontalCenter,
            Right,
            Top,
            VerticalCenter,
            Bottom,
            DistributeHorizontal,
            DistributeVertical,
        ] {
            let (mut doc, mut view) = setup();
            let ids = doc
                .edit_sheet("Floating", |s| {
                    let ids = vec![
                        s.add_floating("Wide floating topic", Vec2::new(100.0, 500.0)),
                        s.add_floating("B", Vec2::new(220.0, 180.0)),
                        s.add_floating("C middle", Vec2::new(800.0, 900.0)),
                    ];
                    s.insert_child(ids[0], "child follows")?;
                    s.add_relationship(ids[0], ids[2])?;
                    Ok(ids)
                })
                .unwrap();
            view.selection = ids.clone();
            view.zoom = 3.5;
            view.offset = egui::vec2(200.0, -700.0);
            let before = doc.sheet().clone();
            assert!(Action::AlignFloating(arrangement).available(&doc, &view));
            apply_action(&mut doc, &mut view, Action::AlignFloating(arrangement)).unwrap();
            let aligned = doc.sheet().clone();
            let layout = view.layout_for(&doc);
            let rects: Vec<_> = ids.iter().map(|id| layout.topics[id].rect).collect();
            let coordinate = |rect: &thinoda_core::Rect| match arrangement {
                Left => rect.min.x,
                Right => rect.max().x,
                Top => rect.min.y,
                Bottom => rect.max().y,
                HorizontalCenter | DistributeHorizontal => rect.center().x,
                _ => rect.center().y,
            };
            let mut values: Vec<_> = rects.iter().map(coordinate).collect();
            if matches!(arrangement, DistributeHorizontal | DistributeVertical) {
                values.sort_by(f32::total_cmp);
                assert!(((values[1] - values[0]) - (values[2] - values[1])).abs() < 0.001);
            } else {
                assert!(values.iter().all(|value| (value - values[0]).abs() < 0.001));
            }
            for (id, topic) in &before.topics {
                let mut expected = topic.clone();
                if ids.contains(id) {
                    expected.position = aligned.topics[id].position;
                }
                assert_eq!(aligned.topics[id], expected);
            }
            assert_eq!(aligned.relationships, before.relationships);
            assert_eq!(view.selection, ids);
            assert!(doc.undo());
            assert_eq!(doc.sheet(), &before);
            assert!(doc.redo());
            assert_eq!(doc.sheet(), &aligned);
            doc.sheet().validate().unwrap();
        }
    }

    #[test]
    fn floating_arrangement_rejects_mixed_or_small_selection_without_edits() {
        let (mut doc, mut view) = setup();
        let root = doc.sheet().root;
        let ids = doc
            .edit_sheet("Floating", |s| {
                Ok(vec![
                    s.add_floating("A", Vec2::ZERO),
                    s.add_floating("B", Vec2::new(10.0, 20.0)),
                ])
            })
            .unwrap();
        let before = doc.sheet().clone();
        for (selection, arrangement) in [
            (vec![ids[0]], FloatingArrangement::Left),
            (vec![ids[0], root], FloatingArrangement::Left),
            (ids, FloatingArrangement::DistributeHorizontal),
        ] {
            view.selection = selection;
            let action = Action::AlignFloating(arrangement);
            assert!(!action.available(&doc, &view));
            assert!(apply_action(&mut doc, &mut view, action).is_err());
            assert_eq!(doc.sheet(), &before);
        }
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
        doc.add_sheet(thinoda_core::Sheet::new("Other", "Root"));
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
    fn adding_parent_todo_preserves_children_and_completion_uses_derived_state() {
        let (mut doc, mut view) = setup();
        let root = doc.sheet().root;
        let child = doc.sheet().children(root)[0];
        doc.edit_sheet("Child checklist", |sheet| sheet.set_todo(child, Some(true)))
            .unwrap();
        view.select_only(root);
        apply_action(&mut doc, &mut view, Action::ToggleTodo).unwrap();
        assert_eq!(doc.sheet().topics[&child].todo, Some(true));
        assert_eq!(doc.sheet().topics[&root].todo, Some(false));
        // An already completed derived parent removes its checkbox, not its children.
        apply_action(&mut doc, &mut view, Action::ToggleTodo).unwrap();
        assert_eq!(doc.sheet().topics[&root].todo, None);
        assert_eq!(doc.sheet().topics[&child].todo, Some(true));
        doc.edit_sheet("Reopen child", |sheet| sheet.set_todo(child, Some(false)))
            .unwrap();
        apply_action(&mut doc, &mut view, Action::ToggleTodo).unwrap();
        let before = doc.sheet().clone();
        apply_action(&mut doc, &mut view, Action::ToggleTodo).unwrap();
        assert!(doc.sheet().todo_progress()[&root].complete());
        assert!(doc.undo());
        assert_eq!(doc.sheet(), &before);
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
