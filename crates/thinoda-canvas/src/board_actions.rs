//! Board commands are transactions over the existing sheet editing APIs.

use super::{Action, BoardAction, FloatingArrangement};
use crate::view::{MapView, ObjectSelection};
use std::collections::HashSet;
use thinoda_core::{
    CanvasObjectId, Document, EditError, EditResult, Rect, SceneId, Sheet, TopicId, Vec2,
    board::{BoardMember, MIN_CARD_SIZE},
    clipboard::{self, BoardClipboardPayload},
    scene::build_scene,
};

const PASTE_OFFSET: Vec2 = Vec2::new(40.0, 40.0);

impl MapView {
    /// Returns the internal board clipboard so a host can share it between views.
    ///
    /// When a host observes any external clipboard change, it must clear this
    /// payload and the legacy topic clipboard. Matching text is not proof of
    /// ownership. An unverified text-paste event also clears this payload.
    pub fn board_clipboard(&self) -> Option<&BoardClipboardPayload> {
        self.board_clipboard.as_ref()
    }

    /// Replaces the internal board clipboard, clearing the topic clipboard when set.
    ///
    /// Hosts managing multiple document views can copy this payload between them.
    /// Use `Action::Paste(None)` only after the host has verified ownership;
    /// otherwise provide an encoded board payload or treat OS text as external.
    pub fn set_board_clipboard(&mut self, clipboard: Option<BoardClipboardPayload>) {
        if clipboard.is_some() {
            self.clipboard = None;
        }
        self.board_clipboard = clipboard;
    }

    /// Invalidates pending/internal clipboards after an external clipboard change.
    ///
    /// A host with multiple document views must invalidate each one.
    pub fn invalidate_clipboard(&mut self) {
        self.board_clipboard = None;
        self.clipboard = None;
        self.clipboard_out = None;
    }

    /// Validates and pastes a rich board representation read from the clipboard.
    ///
    /// Pass only data obtained from the clipboard's separate board format; do
    /// not infer ownership by comparing plain text. A rejected payload does not
    /// reuse an earlier internal clipboard.
    pub fn paste_encoded_board(&mut self, doc: &mut Document, encoded: &str) -> EditResult<bool> {
        self.invalidate_clipboard();
        let payload = clipboard::decode_board_payload(encoded).map_err(EditError::Invalid)?;
        self.paste_board_payload(doc, payload)
    }

    /// Pastes a validated board representation obtained from the clipboard.
    ///
    /// The caller must establish that it came from the clipboard's separate
    /// board format, rather than inferring ownership from plain text.
    pub fn paste_board_payload(
        &mut self,
        doc: &mut Document,
        payload: BoardClipboardPayload,
    ) -> EditResult<bool> {
        self.invalidate_clipboard();
        payload.validate().map_err(EditError::Invalid)?;
        if self.has_title_changes() {
            self.try_commit_edit(doc)?;
        }
        self.try_commit_board_label(doc)?;
        self.prune_selection(doc.sheet());
        let pasted = paste_board(doc, self, &payload, "Paste board items")?;
        self.set_board_clipboard(Some(payload));
        Ok(pasted)
    }
}

/// Ordinary floating topics are not board members and retain their existing behavior.
fn floating_topics(sheet: &Sheet, view: &MapView) -> Vec<TopicId> {
    view.selection
        .iter()
        .copied()
        .filter(|id| {
            sheet.is_floating(*id)
                && sheet.is_visible(*id)
                && !sheet.is_anchor(*id)
                && sheet.board_member_for_topic(*id).is_none()
        })
        .collect()
}

/// Resolves the view's two selection lists into complete card/object groups.
pub(crate) fn selected_board_members(sheet: &Sheet, view: &MapView) -> Vec<BoardMember> {
    let members: Vec<_> = view
        .board_selection
        .iter()
        .copied()
        .filter(|id| sheet.canvas.object(*id).is_some())
        .map(BoardMember::Object)
        .chain(
            view.selection
                .iter()
                .filter_map(|id| sheet.board_member_for_topic(*id)),
        )
        .collect();
    if members.is_empty() {
        Vec::new()
    } else {
        sheet.expand_board_selection(&members).unwrap_or_default()
    }
}

/// A direct lock on any member makes its entire group immovable.
pub(crate) fn member_is_movable(sheet: &Sheet, member: BoardMember) -> bool {
    if let Some(group) = sheet.board_member_group(member) {
        sheet
            .board_group_members(group)
            .iter()
            .all(|member| !sheet.is_board_member_locked(*member))
    } else {
        !sheet.is_board_member_locked(member)
    }
}

pub(crate) fn movable_board_members(sheet: &Sheet, view: &MapView) -> Vec<BoardMember> {
    let members = selected_board_members(sheet, view);
    let locked_groups: HashSet<_> = members
        .iter()
        .filter(|member| sheet.is_board_member_locked(**member))
        .filter_map(|member| sheet.board_member_group(*member))
        .collect();
    members
        .into_iter()
        .filter(|member| {
            !sheet.is_board_member_locked(*member)
                && sheet
                    .board_member_group(*member)
                    .is_none_or(|group| !locked_groups.contains(&group))
        })
        .collect()
}

fn movable_objects(sheet: &Sheet, view: &MapView) -> Vec<CanvasObjectId> {
    movable_board_members(sheet, view)
        .into_iter()
        .filter_map(|member| match member {
            BoardMember::Object(id) => Some(id),
            BoardMember::Card(_) => None,
        })
        .collect()
}

fn has_board_selection(sheet: &Sheet, view: &MapView) -> bool {
    !view.board_selection.is_empty()
        || view
            .selection
            .iter()
            .any(|id| sheet.board_member_for_topic(*id).is_some())
}

fn plain_selected_topics(sheet: &Sheet, view: &MapView) -> Vec<TopicId> {
    view.selection
        .iter()
        .copied()
        .filter(|id| {
            *id != sheet.root && sheet.contains(*id) && sheet.board_member_for_topic(*id).is_none()
        })
        .collect()
}

fn uses_board_clipboard(sheet: &Sheet, view: &MapView) -> bool {
    view.board_selection
        .iter()
        .any(|id| sheet.canvas.object(*id).is_some())
        || view
            .selection
            .iter()
            .any(|id| sheet.board_member_for_topic(*id).is_some())
        || (!sheet.root_mode.is_central()
            && view
                .selection
                .iter()
                .any(|id| sheet.contains(*id) && !sheet.is_anchor(*id)))
}

fn cut_has_locked_members(sheet: &Sheet, view: &MapView) -> bool {
    selected_board_members(sheet, view)
        .iter()
        .any(|member| !member_is_movable(sheet, *member))
}

fn grouping_units(sheet: &Sheet, members: &[BoardMember]) -> usize {
    let mut groups = HashSet::new();
    members
        .iter()
        .filter(|member| {
            sheet
                .board_member_group(**member)
                .is_none_or(|group| groups.insert(group))
        })
        .count()
}

fn selected_zone(view: &MapView) -> Option<thinoda_core::ZoneId> {
    if view.selection.is_empty()
        && view.board_selection.is_empty()
        && let Some(ObjectSelection::Zone(id)) = view.selected_object
    {
        return Some(id);
    }
    None
}

/// Whether the selection belongs to the board gesture domain, even if locked.
pub(crate) fn has_duplicate_target(sheet: &Sheet, view: &MapView) -> bool {
    selected_zone(view).is_some_and(|id| {
        sheet
            .zones
            .iter()
            .any(|zone| zone.id == id && (!sheet.root_mode.is_central() || zone.keep_empty))
    }) || !selected_board_members(sheet, view).is_empty()
        || (!sheet.root_mode.is_central() && !floating_topics(sheet, view).is_empty())
}

fn selected_resizable(sheet: &Sheet, view: &MapView) -> Option<SceneId> {
    if let Some(zone) = selected_zone(view)
        && sheet.zones.iter().any(|item| item.id == zone)
    {
        return Some(SceneId::Zone(zone));
    }
    let members = selected_board_members(sheet, view);
    let [member] = members.as_slice() else {
        return None;
    };
    if !member_is_movable(sheet, *member) {
        return None;
    }
    match *member {
        BoardMember::Card(id) if view.selection == [id] && view.board_selection.is_empty() => {
            Some(SceneId::Topic(id))
        }
        BoardMember::Object(id)
            if view.selection.is_empty()
                && view.board_selection == [id]
                && sheet.canvas.object(id).is_some_and(|o| !o.is_connector()) =>
        {
            Some(SceneId::Object(id))
        }
        _ => None,
    }
}

pub(super) fn shared_available(doc: &Document, view: &MapView, action: &Action) -> Option<bool> {
    let sheet = doc.sheet();
    match action {
        Action::Copy | Action::Cut | Action::Duplicate if uses_board_clipboard(sheet, view) => {
            Some(
                sheet.fim.is_none()
                    && (*action != Action::Cut
                        || (!cut_has_locked_members(sheet, view)
                            && !view.selection.contains(&sheet.root))),
            )
        }
        Action::Delete if has_board_selection(sheet, view) => Some(
            !movable_board_members(sheet, view).is_empty()
                || !plain_selected_topics(sheet, view).is_empty(),
        ),
        Action::AlignFloating(arrangement) if has_board_selection(sheet, view) => {
            Some(available(doc, view, BoardAction::Arrange(*arrangement)))
        }
        Action::StartEdit if view.selection.is_empty() && view.board_selection.len() == 1 => {
            let id = view.board_selection[0];
            Some(
                member_is_movable(sheet, BoardMember::Object(id))
                    && sheet.canvas.object(id).is_some_and(|object| {
                        !matches!(object.kind, thinoda_core::CanvasObjectKind::Ink(_))
                    }),
            )
        }
        Action::StartEdit if view.board_selection.is_empty() && view.selection.len() == 1 => sheet
            .board_member_for_topic(view.selection[0])
            .map(|member| member_is_movable(sheet, member)),
        Action::StartEdit if !view.board_selection.is_empty() => Some(false),
        _ => None,
    }
}

pub(super) fn available(doc: &Document, view: &MapView, action: BoardAction) -> bool {
    let sheet = doc.sheet();
    let members = || movable_board_members(sheet, view);
    let objects = || movable_objects(sheet, view);
    let topics = || floating_topics(sheet, view);
    let any_movable = || !members().is_empty() || !topics().is_empty();
    match action {
        BoardAction::Move(delta) => {
            delta.x.is_finite()
                && delta.y.is_finite()
                && (any_movable() || selected_zone(view).is_some())
        }
        BoardAction::Resize(delta) => {
            delta.x.is_finite() && delta.y.is_finite() && selected_resizable(sheet, view).is_some()
        }
        BoardAction::Rotate(degrees) => {
            degrees.is_finite() && {
                let members = members();
                members.iter().any(|member| match *member {
                    BoardMember::Card(_) => true,
                    BoardMember::Object(id) => sheet
                        .canvas
                        .object(id)
                        .is_some_and(|object| !object.is_connector()),
                }) || (members.len() > 1
                    && members.iter().any(|member| match *member {
                        BoardMember::Object(id) => sheet.canvas.object(id).is_some_and(|object| {
                            matches!(
                                &object.kind,
                                thinoda_core::CanvasObjectKind::Connector(connector)
                                    if matches!(connector.from, thinoda_core::Endpoint::Point(_))
                                        || matches!(connector.to, thinoda_core::Endpoint::Point(_))
                            )
                        }),
                        BoardMember::Card(_) => false,
                    }))
            }
        }
        BoardAction::Arrange(arrangement) => {
            let minimum = if matches!(
                arrangement,
                FloatingArrangement::DistributeHorizontal | FloatingArrangement::DistributeVertical
            ) {
                3
            } else {
                2
            };
            topics().len() + grouping_units(sheet, &members()) >= minimum
        }
        BoardAction::ZOrder(_) | BoardAction::SetBand(_) => !objects().is_empty(),
        BoardAction::Group => {
            let selected = selected_board_members(sheet, view);
            view.selection
                .iter()
                .all(|id| sheet.board_member_for_topic(*id).is_some())
                && selected.len() == members().len()
                && grouping_units(sheet, &selected) >= 2
        }
        BoardAction::Ungroup => members()
            .iter()
            .any(|member| sheet.board_member_group(*member).is_some()),
        BoardAction::Lock(_) => !selected_board_members(sheet, view).is_empty(),
    }
}

pub(crate) fn move_selection(doc: &mut Document, view: &MapView, delta: Vec2) -> EditResult<bool> {
    if delta == Vec2::ZERO {
        return Ok(false);
    }
    if let Some(zone) = selected_zone(view) {
        doc.move_zone(zone, delta)?;
        return Ok(true);
    }
    let topics = floating_topics(doc.sheet(), view);
    let members = movable_board_members(doc.sheet(), view);
    if topics.is_empty() && members.is_empty() {
        return Ok(false);
    }
    doc.edit_sheet("Move board items", |sheet| {
        for id in &topics {
            let position = sheet
                .topic(*id)
                .and_then(|topic| topic.position)
                .ok_or(EditError::NoSuchTopic(*id))?;
            sheet.set_position(*id, position + delta)?;
        }
        if !members.is_empty() {
            sheet
                .move_board_members(&members, delta)
                .map_err(EditError::Invalid)?;
        }
        Ok(())
    })?;
    Ok(true)
}

/// Resizes a single item in one transaction. Cards use their topic position.
pub(crate) fn resize_item(doc: &mut Document, item: SceneId, rect: Rect) -> EditResult<bool> {
    doc.edit_sheet("Resize board item", |sheet| {
        match item {
            SceneId::Topic(id) => {
                let mut card = sheet
                    .topic(id)
                    .and_then(|topic| topic.card)
                    .ok_or(EditError::NoSuchTopic(id))?;
                card.size = rect.size;
                sheet.set_position(id, rect.center())?;
                sheet
                    .set_topic_card(id, Some(card))
                    .map_err(EditError::Invalid)?;
            }
            SceneId::Object(id) => {
                sheet
                    .resize_canvas_object(id, rect)
                    .map_err(EditError::Invalid)?;
            }
            SceneId::Zone(id) => {
                let zone = sheet
                    .zones
                    .iter()
                    .find(|zone| zone.id == id)
                    .ok_or(EditError::NotFound)?;
                // Frame gestures keep their top-left fixed, as does resize_zone.
                if zone.frame.min != rect.min {
                    return Err(EditError::Invalid("frame origin changed".into()));
                }
                sheet
                    .resize_zone(id, rect.size)
                    .map_err(EditError::Invalid)?;
            }
        }
        Ok(())
    })?;
    Ok(true)
}

fn resize_selection(doc: &mut Document, view: &mut MapView, delta: Vec2) -> EditResult<bool> {
    let Some(item) = selected_resizable(doc.sheet(), view) else {
        return Ok(false);
    };
    let rect = match item {
        SceneId::Topic(id) => {
            let layout = view.layout_for(doc);
            let Some(rect) = layout.rect(id) else {
                return Ok(false);
            };
            rect
        }
        SceneId::Object(id) => doc.sheet().canvas.object(id).unwrap().rect,
        SceneId::Zone(id) => {
            doc.sheet()
                .zones
                .iter()
                .find(|zone| zone.id == id)
                .unwrap()
                .frame
        }
    };
    let minimum = match item {
        SceneId::Zone(_) => thinoda_core::MIN_ZONE_SIZE,
        _ => MIN_CARD_SIZE,
    };
    let size = Vec2::new(
        (rect.size.x + delta.x).max(minimum),
        (rect.size.y + delta.y).max(minimum),
    );
    if size == rect.size {
        return Ok(false);
    }
    resize_item(doc, item, Rect::from_min_size(rect.min, size))
}

fn rotate_selection(doc: &mut Document, view: &mut MapView, delta: f32) -> EditResult<bool> {
    if delta == 0.0 {
        return Ok(false);
    }
    let members = movable_board_members(doc.sheet(), view);
    if members.len() > 1 {
        if delta % 360.0 == 0.0 {
            return Ok(false);
        }
        let layout = view.layout_for(doc);
        let scene = build_scene(doc.sheet(), &layout);
        let ids: HashSet<_> = members
            .iter()
            .map(|member| match *member {
                BoardMember::Card(id) => SceneId::Topic(id),
                BoardMember::Object(id) => SceneId::Object(id),
            })
            .collect();
        let mut found = 0;
        let mut bounds: Option<Rect> = None;
        for item in &scene.items {
            if ids.contains(&item.id) {
                found += 1;
                // Include clipped portions: clipping is a display rule, not
                // group membership or a constraint on the transform.
                bounds = Some(bounds.map_or(item.bounds, |bounds| bounds.union(&item.bounds)));
            }
        }
        if found != ids.len() {
            return Err(EditError::Invalid(
                "cannot resolve the selected board geometry".into(),
            ));
        }
        let mut pivot_members = members.clone();
        pivot_members.sort_unstable();
        // A rotating selection's axis-aligned bounds change shape. Retain the
        // original center while the same selection and document state are used,
        // rather than moving the pivot on each successive rotation.
        let pivot = view
            .board_rotation_pivot(doc, &pivot_members)
            .unwrap_or_else(|| bounds.expect("multiple selected members").center());
        let state = doc.state_id();
        doc.edit_sheet("Rotate board items", |sheet| {
            sheet
                .rotate_board_members(&members, pivot, delta)
                .map_err(EditError::Invalid)
        })?;
        let changed = doc.state_id() != state;
        if changed {
            view.remember_board_rotation_pivot(doc, pivot_members, pivot, state);
        }
        return Ok(changed);
    }

    // Preserve the established single-item behavior (including connectors,
    // which cannot be rotated on their own).
    view.clear_board_rotation_pivot();
    let topics: Vec<_> = members
        .iter()
        .filter_map(|member| match member {
            BoardMember::Card(id) => Some(*id),
            BoardMember::Object(_) => None,
        })
        .collect();
    let objects: Vec<_> = members
        .into_iter()
        .filter_map(|member| match member {
            BoardMember::Object(id)
                if doc
                    .sheet()
                    .canvas
                    .object(id)
                    .is_some_and(|object| !object.is_connector()) =>
            {
                Some(id)
            }
            _ => None,
        })
        .collect();
    if topics.is_empty() && objects.is_empty() {
        return Ok(false);
    }
    doc.edit_sheet("Rotate board items", |sheet| {
        for id in topics {
            let mut card = sheet.topic(id).and_then(|t| t.card).unwrap();
            card.rotation = thinoda_core::board::normalize_rotation(card.rotation + delta);
            sheet
                .set_topic_card(id, Some(card))
                .map_err(EditError::Invalid)?;
        }
        for id in objects {
            let degrees = sheet.canvas.object(id).unwrap().rotation + delta;
            sheet
                .rotate_canvas_object(id, degrees)
                .map_err(EditError::Invalid)?;
        }
        Ok(())
    })?;
    Ok(true)
}

#[derive(Clone)]
struct ArrangeUnit {
    bounds: Rect,
    topics: Vec<TopicId>,
    members: Vec<BoardMember>,
}

fn arrange(
    doc: &mut Document,
    view: &mut MapView,
    arrangement: FloatingArrangement,
) -> EditResult<bool> {
    let layout = view.layout_for(doc);
    let sheet = doc.sheet();
    let scene = build_scene(sheet, &layout);
    let mut units = Vec::new();
    for id in floating_topics(sheet, view) {
        if let Some(item) = scene.item(SceneId::Topic(id)) {
            units.push(ArrangeUnit {
                bounds: item.bounds,
                topics: vec![id],
                members: Vec::new(),
            });
        }
    }
    let selected = movable_board_members(sheet, view);
    let mut seen = HashSet::new();
    for member in &selected {
        if !seen.insert(*member) {
            continue;
        }
        let members = sheet
            .board_member_group(*member)
            .map(|group| sheet.board_group_members(group))
            .unwrap_or_else(|| vec![*member]);
        seen.extend(members.iter().copied());
        let bounds = members
            .iter()
            .filter_map(|member| {
                let id = match *member {
                    BoardMember::Card(id) => SceneId::Topic(id),
                    BoardMember::Object(id) => SceneId::Object(id),
                };
                scene.item(id).map(|item| item.bounds)
            })
            .reduce(|a, b| a.union(&b));
        if let Some(bounds) = bounds {
            units.push(ArrangeUnit {
                bounds,
                topics: Vec::new(),
                members,
            });
        }
    }
    let distribute = matches!(
        arrangement,
        FloatingArrangement::DistributeHorizontal | FloatingArrangement::DistributeVertical
    );
    if units.len() < if distribute { 3 } else { 2 } {
        return Ok(false);
    }
    let bounds = units
        .iter()
        .map(|unit| unit.bounds)
        .reduce(|a, b| a.union(&b))
        .unwrap();
    let horizontal = arrangement == FloatingArrangement::DistributeHorizontal;
    let coordinate = |rect: Rect| {
        if horizontal {
            rect.center().x
        } else {
            rect.center().y
        }
    };
    if distribute {
        units.sort_by(|a, b| coordinate(a.bounds).total_cmp(&coordinate(b.bounds)));
    }
    let start = coordinate(units[0].bounds);
    let end = coordinate(units.last().unwrap().bounds);
    let step = (end - start) / (units.len() - 1) as f32;
    let moves: Vec<_> = units
        .into_iter()
        .enumerate()
        .map(|(index, unit)| {
            let rect = unit.bounds;
            let mut delta = Vec2::ZERO;
            match arrangement {
                FloatingArrangement::Left => delta.x = bounds.min.x - rect.min.x,
                FloatingArrangement::HorizontalCenter => {
                    delta.x = bounds.center().x - rect.center().x
                }
                FloatingArrangement::Right => delta.x = bounds.max().x - rect.max().x,
                FloatingArrangement::Top => delta.y = bounds.min.y - rect.min.y,
                FloatingArrangement::VerticalCenter => {
                    delta.y = bounds.center().y - rect.center().y
                }
                FloatingArrangement::Bottom => delta.y = bounds.max().y - rect.max().y,
                FloatingArrangement::DistributeHorizontal => {
                    delta.x = start + step * index as f32 - rect.center().x
                }
                FloatingArrangement::DistributeVertical => {
                    delta.y = start + step * index as f32 - rect.center().y
                }
            }
            (unit, delta)
        })
        .filter(|(_, delta)| *delta != Vec2::ZERO)
        .collect();
    if moves.is_empty() {
        return Ok(false);
    }
    doc.edit_sheet("Arrange board items", |sheet| {
        for (unit, delta) in moves {
            for id in unit.topics {
                let position = sheet.topic(id).and_then(|t| t.position).unwrap();
                sheet.set_position(id, position + delta)?;
            }
            if !unit.members.is_empty() {
                sheet
                    .move_board_members(&unit.members, delta)
                    .map_err(EditError::Invalid)?;
            }
        }
        Ok(())
    })?;
    Ok(true)
}

fn paste_board(
    doc: &mut Document,
    view: &mut MapView,
    payload: &BoardClipboardPayload,
    label: &str,
) -> EditResult<bool> {
    paste_board_at(doc, view, payload, PASTE_OFFSET, label)
}

fn paste_board_at(
    doc: &mut Document,
    view: &mut MapView,
    payload: &BoardClipboardPayload,
    delta: Vec2,
    label: &str,
) -> EditResult<bool> {
    let pasted = doc.edit_sheet(label, |sheet| {
        clipboard::paste_board_selection(sheet, payload, delta).map_err(EditError::Invalid)
    })?;
    view.selection = pasted.topics;
    view.board_selection = pasted.objects;
    view.selected_object = None;
    Ok(true)
}

/// Duplicates the movable board selection at an explicit sheet-space offset.
///
/// Unlike ordinary copy, a duplication gesture respects movement locks: a
/// locked member excludes its whole group. The clipboard is left unchanged;
/// the selection changes only after the copy is validated and committed.
pub(crate) fn duplicate_selection(
    doc: &mut Document,
    view: &mut MapView,
    delta: Vec2,
) -> EditResult<bool> {
    if view.has_pending_ink_gesture() {
        return Err(EditError::Invalid(
            "finish or discard the pending ink gesture before duplicating".into(),
        ));
    }
    if delta == Vec2::ZERO {
        return Ok(false);
    }
    if !delta.x.is_finite() || !delta.y.is_finite() {
        return Err(EditError::Invalid(
            "board duplication offset must be finite".into(),
        ));
    }
    if let Some(id) = selected_zone(view) {
        return duplicate_frame(doc, view, id, delta);
    }

    let mut topics = floating_topics(doc.sheet(), view);
    let mut objects = Vec::new();
    for member in movable_board_members(doc.sheet(), view) {
        match member {
            BoardMember::Card(id) => topics.push(id),
            BoardMember::Object(id) => objects.push(id),
        }
    }
    if topics.is_empty() && objects.is_empty() {
        return Ok(false);
    }
    let Some(prepared) = clipboard::prepare_board_selection(doc.sheet(), &topics, &objects)
        .map_err(EditError::Invalid)?
    else {
        return Ok(false);
    };
    let layout = view.layout_for(doc);
    let payload = prepared.copy(&layout).map_err(EditError::Invalid)?;
    paste_board_at(doc, view, &payload, delta, "Duplicate board items")
}

fn duplicate_frame(
    doc: &mut Document,
    view: &mut MapView,
    id: thinoda_core::ZoneId,
    delta: Vec2,
) -> EditResult<bool> {
    let Some(frame) = doc.sheet().zones.iter().find(|zone| zone.id == id).cloned() else {
        return Ok(false);
    };
    // A selected legacy zone on a central map retains its existing behavior.
    if doc.sheet().root_mode.is_central() && !frame.keep_empty {
        return Ok(false);
    }

    let members: Vec<_> = frame
        .roots
        .iter()
        .filter_map(|id| doc.sheet().board_member_for_topic(*id))
        .chain(frame.objects.iter().copied().map(BoardMember::Object))
        .collect();
    let expanded = if members.is_empty() {
        Vec::new()
    } else {
        doc.sheet()
            .expand_board_selection(&members)
            .map_err(EditError::Invalid)?
    };
    if expanded
        .iter()
        .any(|member| !member_is_movable(doc.sheet(), *member))
    {
        return Err(EditError::Invalid(
            "cannot duplicate a frame containing locked board items".into(),
        ));
    }

    let payload = if frame.roots.is_empty() && frame.objects.is_empty() {
        None
    } else {
        let prepared =
            clipboard::prepare_board_selection(doc.sheet(), &frame.roots, &frame.objects)
                .map_err(EditError::Invalid)?
                .ok_or_else(|| EditError::Invalid("frame contents could not be copied".into()))?;
        let layout = view.layout_for(doc);
        Some(prepared.copy(&layout).map_err(EditError::Invalid)?)
    };
    let new_id =
        doc.edit_sheet("Duplicate board frame", |sheet| {
            let mut roots = Vec::new();
            let mut objects = Vec::new();
            if let Some(payload) = &payload {
                let pasted = clipboard::paste_board_selection(sheet, payload, delta)
                    .map_err(EditError::Invalid)?;
                for root in &frame.roots {
                    let index = payload
                        .source_topic_roots()
                        .iter()
                        .position(|id| id == root)
                        .ok_or_else(|| EditError::Invalid("frame root was not copied".into()))?;
                    roots.push(*pasted.topics.get(index).ok_or_else(|| {
                        EditError::Invalid("copied frame root is missing".into())
                    })?);
                }
                let source_objects: Vec<_> = payload.source_object_ids().collect();
                for object in &frame.objects {
                    let index = source_objects
                        .iter()
                        .position(|id| id == object)
                        .ok_or_else(|| EditError::Invalid("frame object was not copied".into()))?;
                    objects.push(*pasted.objects.get(index).ok_or_else(|| {
                        EditError::Invalid("copied frame object is missing".into())
                    })?);
                }
            }
            let translated = frame.frame.translate(delta);
            let id = if frame.keep_empty {
                let id = sheet
                    .create_board_frame(&frame.name, translated)
                    .map_err(EditError::Invalid)?;
                if !roots.is_empty() || !objects.is_empty() {
                    sheet
                        .set_zone_members(id, roots, objects)
                        .map_err(EditError::Invalid)?;
                }
                id
            } else {
                sheet
                    .create_zone(&frame.name, roots, translated)
                    .map_err(EditError::Invalid)?
            };
            let copied = sheet
                .zones
                .iter_mut()
                .find(|zone| zone.id == id)
                .expect("new frame exists");
            copied.color = frame.color;
            copied.background_color = frame.background_color;
            copied.clip = frame.clip;
            Ok(id)
        })?;
    view.select_object(ObjectSelection::Zone(new_id));
    Ok(true)
}

fn copy_cut_or_duplicate(
    doc: &mut Document,
    view: &mut MapView,
    action: &Action,
) -> EditResult<bool> {
    if *action == Action::Cut
        && (cut_has_locked_members(doc.sheet(), view) || view.selection.contains(&doc.sheet().root))
    {
        return Ok(false);
    }
    let Some(prepared) =
        clipboard::prepare_board_selection(doc.sheet(), &view.selection, &view.board_selection)
            .map_err(EditError::Invalid)?
    else {
        return Ok(false);
    };
    let layout = view.layout_for(doc);
    let payload = prepared.copy(&layout).map_err(EditError::Invalid)?;
    if *action == Action::Duplicate {
        return paste_board(doc, view, &payload, "Duplicate board items");
    }
    // Build the exact portable form before cut changes the document. The
    // payload's validation also enforces its serialized-size limit.
    clipboard::encode_board_payload(&payload).map_err(EditError::Invalid)?;
    if *action == Action::Cut {
        let members: Vec<_> = payload
            .source_topic_roots()
            .iter()
            .filter_map(|id| doc.sheet().board_member_for_topic(*id))
            .chain(payload.source_object_ids().map(BoardMember::Object))
            .collect();
        let topics: Vec<_> = payload
            .source_topic_roots()
            .iter()
            .copied()
            .filter(|id| {
                *id != doc.sheet().root && doc.sheet().board_member_for_topic(*id).is_none()
            })
            .collect();
        if members.is_empty() && topics.is_empty() {
            return Ok(false);
        }
        doc.edit_sheet("Cut board items", |sheet| {
            if !members.is_empty() {
                sheet
                    .remove_board_members(&members)
                    .map_err(EditError::Invalid)?;
            }
            if !topics.is_empty() {
                sheet.remove_topics(&topics)?;
            }
            Ok(())
        })?;
        view.selection.retain(|id| doc.sheet().contains(*id));
        view.board_selection
            .retain(|id| doc.sheet().canvas.object(*id).is_some());
    }
    view.clipboard_out = Some(payload.text.clone());
    view.set_board_clipboard(Some(payload));
    Ok(true)
}

pub(super) fn apply(
    doc: &mut Document,
    view: &mut MapView,
    action: BoardAction,
) -> EditResult<bool> {
    match action {
        BoardAction::Move(delta) => move_selection(doc, view, delta),
        BoardAction::Resize(delta) => resize_selection(doc, view, delta),
        BoardAction::Rotate(delta) => rotate_selection(doc, view, delta),
        BoardAction::Arrange(arrangement) => arrange(doc, view, arrangement),
        BoardAction::ZOrder(order) => {
            let ids = movable_objects(doc.sheet(), view);
            if ids.is_empty() {
                Ok(false)
            } else {
                doc.reorder_canvas_objects(&ids, order).map(|()| true)
            }
        }
        BoardAction::SetBand(band) => {
            let ids = movable_objects(doc.sheet(), view);
            if ids.is_empty() {
                Ok(false)
            } else {
                doc.set_canvas_band(&ids, band).map(|()| true)
            }
        }
        BoardAction::Group => {
            let members = selected_board_members(doc.sheet(), view);
            if members.len() < 2 {
                Ok(false)
            } else {
                doc.group_board_members(&members).map(|_| true)
            }
        }
        BoardAction::Ungroup => {
            let groups: std::collections::BTreeSet<_> = movable_board_members(doc.sheet(), view)
                .iter()
                .filter_map(|member| doc.sheet().board_member_group(*member))
                .collect();
            if groups.is_empty() {
                Ok(false)
            } else {
                doc.edit_sheet("Ungroup", |sheet| {
                    for group in groups {
                        sheet
                            .ungroup_board_members(group)
                            .map_err(EditError::Invalid)?;
                    }
                    Ok(())
                })
                .map(|()| true)
            }
        }
        BoardAction::Lock(locked) => {
            let members = selected_board_members(doc.sheet(), view);
            let groups: std::collections::BTreeSet<_> = members
                .iter()
                .filter_map(|member| doc.sheet().board_member_group(*member))
                .collect();
            if members.is_empty() {
                Ok(false)
            } else {
                doc.edit_sheet(if locked { "Lock" } else { "Unlock" }, |sheet| {
                    for group in groups {
                        sheet
                            .set_canvas_group_locked(group, locked)
                            .map_err(EditError::Invalid)?;
                    }
                    sheet
                        .set_board_members_locked(&members, locked)
                        .map_err(EditError::Invalid)
                })
                .map(|()| true)
            }
        }
    }
}

pub(super) fn apply_shared(
    doc: &mut Document,
    view: &mut MapView,
    action: &Action,
) -> Option<EditResult<bool>> {
    if let Action::Paste(text) = action {
        if text.is_some() {
            // Text equality cannot establish ownership of either internal
            // clipboard. Rich board data has its own explicit entry point.
            view.invalidate_clipboard();
        } else if let Some(payload) = view.board_clipboard.clone() {
            // The host may use this path only after proving clipboard ownership.
            return Some(paste_board(doc, view, &payload, "Paste board items"));
        }
        if !doc.sheet().root_mode.is_central() {
            let subtrees = clipboard::resolve_paste(view.clipboard.as_ref(), text.as_deref());
            if subtrees.is_empty() {
                return Some(Ok(false));
            }
            let origin = view.xf().sheet(view.canvas_rect().center());
            return Some(
                doc.edit_sheet("Paste board topics", |sheet| {
                    clipboard::paste_floating_subtrees(sheet, &subtrees, origin)
                        .map_err(EditError::Invalid)
                })
                .map(|topics| {
                    view.selection = topics;
                    view.board_selection.clear();
                    view.selected_object = None;
                    true
                }),
            );
        }
    }
    let result = match *action {
        Action::Copy | Action::Cut | Action::Duplicate
            if uses_board_clipboard(doc.sheet(), view) =>
        {
            copy_cut_or_duplicate(doc, view, action)
        }
        Action::Delete if has_board_selection(doc.sheet(), view) => {
            let members = movable_board_members(doc.sheet(), view);
            let topics = plain_selected_topics(doc.sheet(), view);
            if members.is_empty() && topics.is_empty() {
                return Some(Ok(false));
            }
            doc.edit_sheet("Delete board items", |sheet| {
                if !members.is_empty() {
                    sheet
                        .remove_board_members(&members)
                        .map_err(EditError::Invalid)?;
                }
                if !topics.is_empty() {
                    sheet.remove_topics(&topics)?;
                }
                Ok(())
            })
            .map(|()| {
                view.selection.retain(|id| doc.sheet().contains(*id));
                view.board_selection
                    .retain(|id| doc.sheet().canvas.object(*id).is_some());
                true
            })
        }
        Action::StartEdit if view.selection.is_empty() && view.board_selection.len() == 1 => {
            let id = view.board_selection[0];
            Ok(view.begin_board_label(doc, id))
        }
        Action::StartEdit if !view.board_selection.is_empty() => Ok(false),
        Action::AlignFloating(arrangement) if has_board_selection(doc.sheet(), view) => {
            arrange(doc, view, arrangement)
        }
        Action::SelectAll
            if !doc.sheet().root_mode.is_central()
                || !doc.sheet().canvas.objects.is_empty()
                || !view.board_selection.is_empty() =>
        {
            let layout = view.layout_for(doc);
            let sheet = doc.sheet();
            let scene = build_scene(sheet, &layout);
            view.selected_object = None;
            view.selection.clear();
            view.board_selection.clear();
            for item in scene
                .items
                .iter()
                .filter(|item| item.visible_bounds().is_some())
            {
                match item.id {
                    SceneId::Topic(id) if !sheet.is_anchor(id) && sheet.is_visible(id) => {
                        view.selection.push(id);
                    }
                    SceneId::Object(id) => view.board_selection.push(id),
                    _ => {}
                }
            }
            crate::view::board_input::expand_selection(view, sheet);
            Ok(true)
        }
        _ => return None,
    };
    Some(result)
}
