//! A self-contained, internal clipboard for mixed board selections.
//!
//! The OS clipboard still receives plain text. The private fragment is a small
//! rootless sheet, with a new empty anchor rather than the source sheet's root.

use super::{ClipboardPayload, MAX_TOPIC_CLIPBOARD_DEPTH, resolve_paste, same_normalized};
use crate::board::{
    BoardMember, CanvasGroupId, CanvasObjectId, CanvasObjectKind, Endpoint, MAX_CANVAS_OBJECTS,
    SheetRootMode,
};
use crate::geometry::Vec2;
use crate::layout::Layout;
use crate::model::{Sheet, Subtree, Topic, TopicId, valid_position};
use crate::scene::{SceneGeometry, SceneId, build_scene, topic_anchor_rect};
use crate::schedule::Weekday;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, Write};

const BOARD_PREFIX: &str = "THINODA-BOARD-CLIPBOARD-1\n";
const INLINE_BOARD_PREFIX: &str = "THINODA-BOARD-CLIPBOARD-2\n";
const WAYPOINT_BOARD_PREFIX: &str = "THINODA-BOARD-CLIPBOARD-3\n";
const MARKER_BOARD_PREFIX: &str = "THINODA-BOARD-CLIPBOARD-4\n";
const EXTENDED_MARKER_BOARD_PREFIX: &str = "THINODA-BOARD-CLIPBOARD-5\n";
const SIZED_MARKER_BOARD_PREFIX: &str = "THINODA-BOARD-CLIPBOARD-6\n";
/// Maximum size of the serialized internal board payload, including its prefix.
pub const MAX_BOARD_CLIPBOARD_BYTES: usize = 16 * 1024 * 1024;
const MAX_BOARD_CLIPBOARD_TOPICS: usize = MAX_CANVAS_OBJECTS;

/// Mixed board content and the plain text to write to the OS clipboard.
///
/// The fragment owns only the selection, its recursively owned summary trees,
/// and resources used by those topics. It never contains the source sheet's
/// hidden anchor. Use [`paste_board_selection`] rather than inserting the
/// fragment directly; all IDs and references must be reassigned on paste.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardClipboardPayload {
    pub text: String,
    fragment: Sheet,
}

impl BoardClipboardPayload {
    fn prefix(&self) -> &'static str {
        if self.fragment.has_connector_marker_sizes() {
            SIZED_MARKER_BOARD_PREFIX
        } else if self.fragment.has_extended_connector_markers() {
            EXTENDED_MARKER_BOARD_PREFIX
        } else if self.fragment.has_connector_markers() {
            MARKER_BOARD_PREFIX
        } else if self.fragment.has_connector_waypoints() {
            WAYPOINT_BOARD_PREFIX
        } else if self.fragment.has_inline_text() {
            INLINE_BOARD_PREFIX
        } else {
            BOARD_PREFIX
        }
    }

    /// Source topic roots after deduplication and group expansion. Useful when
    /// preparing a cut; the caller must still check locks before deleting.
    pub fn source_topic_roots(&self) -> &[TopicId] {
        &self.fragment.floating
    }

    /// Source objects after group expansion, in their original paint order.
    pub fn source_object_ids(&self) -> impl Iterator<Item = CanvasObjectId> + '_ {
        self.fragment.canvas.objects.iter().map(|object| object.id)
    }

    /// Validates the current payload, including any changes to its public text.
    pub fn validate(&self) -> Result<(), String> {
        if self.text.trim().is_empty() {
            return Err("board clipboard text must not be empty".into());
        }
        if self.text.len() > MAX_BOARD_CLIPBOARD_BYTES {
            return Err("board clipboard payload exceeds 16 MiB".into());
        }
        let sheet = &self.fragment;
        if sheet.topics.len() > MAX_BOARD_CLIPBOARD_TOPICS + 1 {
            return Err(format!(
                "board clipboard exceeds {MAX_BOARD_CLIPBOARD_TOPICS} topics"
            ));
        }
        if sheet.floating.len() > sheet.topics.len() || sheet.summaries.len() > sheet.topics.len() {
            return Err("invalid board clipboard topic ownership".into());
        }
        if sheet.root_mode != SheetRootMode::Hidden
            || sheet.fim.is_some()
            || !sheet.title.is_empty()
            || !sheet.zones.is_empty()
            || !sheet.presentation.slides.is_empty()
            || !sheet.board.is_default()
        {
            return Err("invalid board clipboard fragment".into());
        }
        if sheet.topic(sheet.root) != Some(&Topic::new(sheet.root, "")) {
            return Err("board clipboard anchor must be empty".into());
        }
        if sheet.floating.is_empty() && sheet.canvas.objects.is_empty() {
            return Err("board clipboard selection is empty".into());
        }

        // Check the ownership graph independently of Sheet::validate. In
        // particular, summary topics are detached roots, and repeated roots or
        // summary cycles must not be allowed to mask an unreachable topic.
        let mut summary_children: HashMap<TopicId, Vec<TopicId>> = HashMap::new();
        let mut summary_ids = HashSet::new();
        for summary in &sheet.summaries {
            if !summary_ids.insert(summary.id) {
                return Err("duplicate board clipboard summary ID".into());
            }
            summary_children
                .entry(summary.parent)
                .or_default()
                .push(summary.topic);
        }
        let mut visited = HashSet::new();
        let mut pending: Vec<_> = sheet.floating.iter().map(|id| (*id, 0usize)).collect();
        while let Some((id, depth)) = pending.pop() {
            if depth > MAX_TOPIC_CLIPBOARD_DEPTH {
                return Err(format!(
                    "board clipboard topic depth exceeds {MAX_TOPIC_CLIPBOARD_DEPTH}"
                ));
            }
            if id == sheet.root || !visited.insert(id) {
                return Err("invalid or repeated board clipboard topic ownership".into());
            }
            let topic = sheet
                .topic(id)
                .ok_or_else(|| format!("board clipboard topic {id} is missing"))?;
            if topic
                .position
                .is_some_and(|position| !valid_position(position))
            {
                return Err(format!(
                    "board clipboard topic {id} has an invalid position"
                ));
            }
            pending.extend(topic.children.iter().map(|child| (*child, depth + 1)));
            if let Some(summaries) = summary_children.get(&id) {
                pending.extend(summaries.iter().map(|child| (*child, depth + 1)));
            }
        }
        if visited.len() + 1 != sheet.topics.len() {
            return Err("board clipboard contains unowned topics".into());
        }
        if sheet.floating.iter().any(|id| {
            sheet
                .topic(*id)
                .and_then(|topic| topic.position)
                .is_none_or(|position| !valid_position(position))
        }) {
            return Err("board clipboard floating topic has no valid position".into());
        }
        if has_duplicates(
            sheet
                .relationships
                .iter()
                .map(|relationship| relationship.id),
        ) || has_duplicates(sheet.boundaries.iter().map(|boundary| boundary.id))
        {
            return Err("duplicate board clipboard annotation ID".into());
        }

        // Bound serialized input before the more expensive model validation.
        let mut counter = SizeCounter::new(MAX_BOARD_CLIPBOARD_BYTES - self.prefix().len());
        if let Err(error) = serde_json::to_writer(&mut counter, self) {
            if counter.exceeded {
                return Err("board clipboard payload exceeds 16 MiB".into());
            }
            return Err(error.to_string());
        }
        sheet.validate()
    }
}

fn has_duplicates<T: Eq + std::hash::Hash>(values: impl Iterator<Item = T>) -> bool {
    let mut seen = HashSet::new();
    values.into_iter().any(|value| !seen.insert(value))
}

struct SizeCounter {
    bytes: usize,
    limit: usize,
    exceeded: bool,
}

impl SizeCounter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: 0,
            limit,
            exceeded: false,
        }
    }
}

impl Write for SizeCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            self.exceeded = true;
            return Err(io::Error::other("board clipboard size limit"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Optional versioned encoding for storing/transferring the internal payload.
/// Normal OS text clipboard operations should use [`BoardClipboardPayload::text`].
pub fn encode_board_payload(payload: &BoardClipboardPayload) -> Result<String, String> {
    payload.validate()?;
    let json = serde_json::to_string(payload).map_err(|error| error.to_string())?;
    Ok(format!("{}{json}", payload.prefix()))
}

/// Decodes and validates a versioned board payload. Both LF and Windows CRLF
/// after the prefix are accepted; legacy topic/sheet payloads are not changed.
pub fn decode_board_payload(text: &str) -> Result<BoardClipboardPayload, String> {
    if text.len() > MAX_BOARD_CLIPBOARD_BYTES {
        return Err("board clipboard payload exceeds 16 MiB".into());
    }
    let (version, json) = text
        .strip_prefix(SIZED_MARKER_BOARD_PREFIX)
        .or_else(|| text.strip_prefix("THINODA-BOARD-CLIPBOARD-6\r\n"))
        .map(|json| (6, json))
        .or_else(|| {
            text.strip_prefix(EXTENDED_MARKER_BOARD_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-BOARD-CLIPBOARD-5\r\n"))
                .map(|json| (5, json))
        })
        .or_else(|| {
            text.strip_prefix(MARKER_BOARD_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-BOARD-CLIPBOARD-4\r\n"))
                .map(|json| (4, json))
        })
        .or_else(|| {
            text.strip_prefix(WAYPOINT_BOARD_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-BOARD-CLIPBOARD-3\r\n"))
                .map(|json| (3, json))
        })
        .or_else(|| {
            text.strip_prefix(INLINE_BOARD_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-BOARD-CLIPBOARD-2\r\n"))
                .map(|json| (2, json))
        })
        .or_else(|| {
            text.strip_prefix(BOARD_PREFIX)
                .or_else(|| text.strip_prefix("THINODA-BOARD-CLIPBOARD-1\r\n"))
                .map(|json| (1, json))
        })
        .ok_or("clipboard does not contain a supported Thinoda board selection")?;
    let payload: BoardClipboardPayload =
        serde_json::from_str(json).map_err(|error| error.to_string())?;
    if version < 4 && payload.fragment.has_connector_markers() {
        return Err("connector markers require board clipboard version 4".into());
    }
    if version < 5 && payload.fragment.has_extended_connector_markers() {
        return Err("classic and thin connector markers require board clipboard version 5".into());
    }
    if version < 6 && payload.fragment.has_connector_marker_sizes() {
        return Err("connector marker sizes require board clipboard version 6".into());
    }
    if version < 3 && payload.fragment.has_connector_waypoints() {
        return Err("connector waypoints require board clipboard version 3".into());
    }
    if version < 2 && payload.fragment.has_inline_text() {
        return Err("inline card text requires board clipboard version 2".into());
    }
    payload.validate()?;
    Ok(payload)
}

/// The result of resolving the internal clipboard against OS text.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedBoardPaste<'a> {
    Board(&'a BoardClipboardPayload),
    /// Legacy subtrees or arbitrary OS text, resolved exactly as before.
    Topics(Vec<Subtree>),
}

/// Prefer board content only while its OS text still matches; otherwise use
/// the existing topic/text resolver. With no OS text, use the internal board
/// payload if present, just as the legacy resolver does for topic payloads.
///
/// Text equality is only a clipboard-ownership heuristic: identical text from
/// another application is indistinguishable. A host should keep one shared
/// internal clipboard for all documents, and clear the board payload whenever
/// it replaces the internal clipboard with another kind of content.
pub fn resolve_board_paste<'a>(
    board: Option<&'a BoardClipboardPayload>,
    legacy: Option<&ClipboardPayload>,
    os_text: Option<&str>,
) -> Result<ResolvedBoardPaste<'a>, String> {
    if os_text.is_some_and(|text| text.len() > MAX_BOARD_CLIPBOARD_BYTES) {
        return Err("clipboard text exceeds 16 MiB".into());
    }
    if let Some(board) = board
        && os_text.is_none_or(|text| same_normalized(text, &board.text))
    {
        board.validate()?;
        return Ok(ResolvedBoardPaste::Board(board));
    }
    Ok(ResolvedBoardPaste::Topics(resolve_paste(legacy, os_text)?))
}

/// Copies selected topics and canvas objects into a standalone board fragment.
///
/// `layout` must describe `sheet` as currently displayed: it is used to
/// preserve the rendered position of non-floating topic roots and to detach
/// connectors from unselected targets at their resolved path endpoints.
/// Descendants and recursively owned summaries travel with their selected
/// topic; complete mixed card/object groups are expanded automatically.
///
/// Copying locked members is allowed because it does not change them, and
/// their direct/group locks are retained. For a cut, the caller must reject
/// locked members before publishing or deleting anything. Hidden anchors are
/// ignored, just as in [`super::copy_topics`].
///
/// Hosts that would otherwise compute a layout for the copy should call
/// [`prepare_board_selection`] first, so excessive depth is rejected before
/// recursive layout runs.
pub fn copy_board_selection(
    sheet: &Sheet,
    layout: &Layout,
    topic_ids: &[TopicId],
    object_ids: &[CanvasObjectId],
) -> Result<Option<BoardClipboardPayload>, String> {
    prepare_board_selection(sheet, topic_ids, object_ids)?
        .map(|selection| selection.copy(layout))
        .transpose()
}

/// A checked board selection tied to its unchanged source sheet.
pub struct PreparedBoardSelection<'a> {
    sheet: &'a Sheet,
    roots: Vec<TopicId>,
    owned: HashSet<TopicId>,
    objects: HashSet<CanvasObjectId>,
}

/// Resolves groups and checks topic ownership and depth without building a layout.
pub fn prepare_board_selection<'a>(
    sheet: &'a Sheet,
    topic_ids: &[TopicId],
    object_ids: &[CanvasObjectId],
) -> Result<Option<PreparedBoardSelection<'a>>, String> {
    if sheet.fim.is_some() {
        return Err("FIM selections cannot be copied as board fragments".into());
    }
    sheet.validate()?;

    let mut topics = Vec::new();
    let mut topic_set = HashSet::new();
    let mut members = Vec::new();
    let floating: HashSet<_> = sheet.floating.iter().copied().collect();
    for &id in topic_ids {
        if sheet.is_anchor(id) {
            continue;
        }
        if !sheet.contains(id) {
            return Err(format!("topic {id} not found"));
        }
        if topic_set.insert(id) {
            topics.push(id);
        }
        if id != sheet.root
            && floating.contains(&id)
            && sheet
                .topic(id)
                .is_some_and(|topic| topic.parent.is_none() && topic.card.is_some())
        {
            members.push(BoardMember::Card(id));
        }
    }
    members.extend(object_ids.iter().copied().map(BoardMember::Object));
    let mut objects = HashSet::new();
    if !members.is_empty() {
        for member in sheet.expand_board_selection(&members)? {
            match member {
                BoardMember::Card(id) => {
                    if topic_set.insert(id) {
                        topics.push(id);
                    }
                }
                BoardMember::Object(id) => {
                    objects.insert(id);
                }
            }
        }
    }
    if topics.is_empty() && objects.is_empty() {
        return Ok(None);
    }

    let mut summary_owner = HashMap::new();
    let mut summary_children: HashMap<TopicId, Vec<TopicId>> = HashMap::new();
    for summary in &sheet.summaries {
        if summary_owner
            .insert(summary.topic, summary.parent)
            .is_some()
        {
            return Err("summary topic has multiple owners".into());
        }
        summary_children
            .entry(summary.parent)
            .or_default()
            .push(summary.topic);
    }
    let parent_of = |id: TopicId| {
        sheet
            .topic(id)
            .and_then(|topic| topic.parent)
            .or_else(|| summary_owner.get(&id).copied())
    };
    let mut roots = Vec::new();
    for id in topics {
        let mut ancestor = parent_of(id);
        let mut seen = HashSet::from([id]);
        let mut covered = false;
        while let Some(parent) = ancestor {
            if !seen.insert(parent) {
                return Err("cyclic topic ownership in board selection".into());
            }
            if topic_set.contains(&parent) {
                covered = true;
                break;
            }
            ancestor = parent_of(parent);
        }
        if !covered {
            roots.push(id);
        }
    }

    let mut owned = HashSet::new();
    let mut pending: Vec<_> = roots.iter().map(|id| (*id, 0usize)).collect();
    while let Some((id, depth)) = pending.pop() {
        if depth > MAX_TOPIC_CLIPBOARD_DEPTH {
            return Err(format!(
                "board clipboard topic depth exceeds {MAX_TOPIC_CLIPBOARD_DEPTH}"
            ));
        }
        if !owned.insert(id) {
            return Err("invalid or repeated board clipboard topic ownership".into());
        }
        if owned.len() > MAX_BOARD_CLIPBOARD_TOPICS {
            return Err(format!(
                "board clipboard exceeds {MAX_BOARD_CLIPBOARD_TOPICS} topics"
            ));
        }
        let topic = sheet
            .topic(id)
            .ok_or_else(|| format!("topic {id} not found"))?;
        pending.extend(topic.children.iter().map(|child| (*child, depth + 1)));
        if let Some(summaries) = summary_children.get(&id) {
            pending.extend(summaries.iter().map(|child| (*child, depth + 1)));
        }
    }

    Ok(Some(PreparedBoardSelection {
        sheet,
        roots,
        owned,
        objects,
    }))
}

impl PreparedBoardSelection<'_> {
    /// Builds the fragment using a layout of the prepared source sheet.
    pub fn copy(self, layout: &Layout) -> Result<BoardClipboardPayload, String> {
        copy_prepared_board_selection(self, layout)
    }
}

fn copy_prepared_board_selection(
    selection: PreparedBoardSelection<'_>,
    layout: &Layout,
) -> Result<BoardClipboardPayload, String> {
    let PreparedBoardSelection {
        sheet,
        roots,
        owned,
        objects,
    } = selection;
    let floating: HashSet<_> = sheet.floating.iter().copied().collect();
    let mut fragment = Sheet::new("", "");
    // Choose an anchor that cannot be mistaken for any copied topic. It is
    // never written as a selected root, text, or a connector target.
    let mut anchor = 0u64;
    while owned.contains(&TopicId(anchor)) || TopicId(anchor) == sheet.root {
        anchor = anchor
            .checked_add(1)
            .ok_or("no available board clipboard anchor ID")?;
    }
    fragment.topics.clear();
    fragment.root = TopicId(anchor);
    fragment
        .topics
        .insert(fragment.root, Topic::new(fragment.root, ""));
    fragment.root_mode = SheetRootMode::Hidden;
    for &id in &owned {
        fragment
            .topics
            .insert(id, sheet.topic(id).expect("owned topic exists").clone());
    }
    for &id in &roots {
        let original = sheet.topic(id).expect("selected topic exists");
        let position = if floating.contains(&id) {
            original
                .position
                .or_else(|| layout.rect(id).map(|rect| rect.center()))
        } else {
            topic_anchor_rect(sheet, layout, id).map(|rect| rect.center())
        }
        .filter(|position| valid_position(*position))
        .ok_or_else(|| format!("cannot resolve the position of topic {id}"))?;
        let topic = fragment.topics.get_mut(&id).expect("root was copied");
        topic.parent = None;
        topic.position = Some(position);
        fragment.floating.push(id);
    }

    fragment.relationships = sheet
        .relationships
        .iter()
        .filter(|relationship| {
            owned.contains(&relationship.from) && owned.contains(&relationship.to)
        })
        .cloned()
        .collect();
    fragment.boundaries = sheet
        .boundaries
        .iter()
        .filter(|boundary| owned.contains(&boundary.parent))
        .cloned()
        .collect();
    fragment.summaries = sheet
        .summaries
        .iter()
        .filter(|summary| owned.contains(&summary.parent) && owned.contains(&summary.topic))
        .cloned()
        .collect();
    fragment.scheduling.tasks = sheet
        .scheduling
        .tasks
        .iter()
        .filter(|(id, _)| owned.contains(id))
        .map(|(id, task)| (*id, task.clone()))
        .collect();
    if !fragment.scheduling.tasks.is_empty() {
        // The source calendar is not copied; the destination is checked on paste.
        fragment.scheduling.calendar.working_weekdays = Weekday::ALL.into();
    }
    fragment.scheduling.dependencies = sheet
        .scheduling
        .dependencies
        .iter()
        .filter(|dependency| {
            owned.contains(&dependency.predecessor) && owned.contains(&dependency.successor)
        })
        .copied()
        .collect();
    for id in &owned {
        let topic = &sheet.topics[id];
        for reference in topic
            .image
            .as_deref()
            .into_iter()
            .chain(topic.attachment.as_ref().map(|a| a.resource.as_str()))
        {
            if let Some(id) = reference.strip_prefix(crate::resources::RESOURCE_PREFIX) {
                let resource = sheet
                    .resources
                    .entries
                    .get(id)
                    .ok_or_else(|| format!("missing embedded resource {id}"))?;
                fragment
                    .resources
                    .entries
                    .insert(id.to_owned(), resource.clone());
            }
        }
    }

    let copied_objects: Vec<_> = sheet
        .canvas
        .objects
        .iter()
        .filter(|object| objects.contains(&object.id))
        .cloned()
        .collect();
    let requires_detachment = copied_objects.iter().any(|object| {
        if let CanvasObjectKind::Connector(connector) = &object.kind {
            [connector.from, connector.to]
                .into_iter()
                .any(|end| is_external(end, &owned, &objects))
        } else {
            false
        }
    });
    let paths = if requires_detachment {
        build_scene(sheet, layout)
            .items
            .into_iter()
            .filter_map(|item| match (item.id, item.geometry) {
                (SceneId::Object(id), SceneGeometry::Polyline { points, .. })
                    if objects.contains(&id) =>
                {
                    Some((id, points))
                }
                _ => None,
            })
            .collect::<HashMap<_, _>>()
    } else {
        HashMap::new()
    };
    for mut object in copied_objects {
        if let CanvasObjectKind::Connector(connector) = &mut object.kind {
            let from_external = is_external(connector.from, &owned, &objects);
            let to_external = is_external(connector.to, &owned, &objects);
            if from_external || to_external {
                let path = paths
                    .get(&object.id)
                    .filter(|path| path.len() >= 2)
                    .ok_or_else(|| format!("cannot resolve connector {}", object.id))?;
                if from_external {
                    connector.from = Endpoint::Point(path[0]);
                }
                if to_external {
                    connector.to = Endpoint::Point(*path.last().expect("path is nonempty"));
                }
            }
        }
        fragment.canvas.objects.push(object);
    }
    fragment.canvas.card_states = sheet
        .canvas
        .card_states
        .iter()
        .filter(|state| owned.contains(&state.topic))
        .copied()
        .collect();
    let groups: HashSet<_> = fragment
        .canvas
        .objects
        .iter()
        .filter_map(|object| object.group)
        .chain(
            fragment
                .canvas
                .card_states
                .iter()
                .filter_map(|state| state.group),
        )
        .collect();
    fragment.canvas.groups = sheet
        .canvas
        .groups
        .iter()
        .filter(|group| groups.contains(&group.id))
        .copied()
        .collect();

    let text = plain_text(sheet, &roots, &fragment.canvas.objects)?;
    let payload = BoardClipboardPayload { text, fragment };
    payload.validate()?;
    Ok(payload)
}

fn is_external(
    endpoint: Endpoint,
    topics: &HashSet<TopicId>,
    objects: &HashSet<CanvasObjectId>,
) -> bool {
    match endpoint {
        Endpoint::Topic(id) => !topics.contains(&id),
        Endpoint::Object(id) => !objects.contains(&id),
        Endpoint::Point(_) => false,
    }
}

fn plain_text(
    sheet: &Sheet,
    roots: &[TopicId],
    objects: &[crate::board::CanvasObject],
) -> Result<String, String> {
    fn append(text: &mut String, title: &str, depth: usize) -> Result<(), String> {
        if text
            .len()
            .saturating_add(depth)
            .saturating_add(title.len())
            .saturating_add(1)
            > MAX_BOARD_CLIPBOARD_BYTES
        {
            return Err("board clipboard payload exceeds 16 MiB".into());
        }
        text.extend(std::iter::repeat_n('\t', depth));
        text.extend(title.chars().map(|ch| if ch == '\n' { ' ' } else { ch }));
        text.push('\n');
        Ok(())
    }

    let mut text = String::new();
    let mut pending: Vec<_> = roots.iter().rev().map(|id| (*id, 0usize)).collect();
    let mut visited = HashSet::new();
    while let Some((id, depth)) = pending.pop() {
        if !visited.insert(id) {
            return Err("invalid or repeated board clipboard topic ownership".into());
        }
        let topic = &sheet.topics[&id];
        append(&mut text, &topic.title, depth)?;
        pending.extend(topic.children.iter().rev().map(|id| (*id, depth + 1)));
    }
    for object in objects {
        if !object.label().trim().is_empty() {
            append(&mut text, object.label(), 0)?;
        }
    }
    if text.trim().is_empty() {
        text = "[Board selection]\n".into();
    }
    Ok(text)
}

/// Pastes legacy subtrees or parsed OS text as floating roots.
///
/// The first root is centred at `origin`; later roots are placed below it with
/// spacing based on card height. Each subtree is remapped independently,
/// since plain-text parsing may reuse the same IDs in multiple subtrees.
/// Topic styles, cards, tasks and embedded resources survive. Legacy subtrees
/// do not carry board groups or lock state. As with [`paste_board_selection`],
/// validation is atomic and callers can wrap this in `Document::edit_sheet`.
pub fn paste_floating_subtrees(
    sheet: &mut Sheet,
    subtrees: &[Subtree],
    origin: Vec2,
) -> Result<Vec<TopicId>, String> {
    if subtrees.is_empty() {
        return Ok(Vec::new());
    }
    if sheet.fim.is_some() {
        return Err("topics cannot be pasted as floating roots into a FIM sheet".into());
    }
    if !valid_position(origin) {
        return Err("board clipboard origin is out of range".into());
    }
    let topic_count = subtrees
        .iter()
        .try_fold(0usize, |total, subtree| {
            total.checked_add(subtree.topics.len())
        })
        .ok_or("board clipboard has too many topics")?;
    if topic_count > MAX_BOARD_CLIPBOARD_TOPICS || subtrees.len() > MAX_BOARD_CLIPBOARD_TOPICS {
        return Err(format!(
            "board clipboard exceeds {MAX_BOARD_CLIPBOARD_TOPICS} topics"
        ));
    }
    let mut counter = SizeCounter::new(MAX_BOARD_CLIPBOARD_BYTES - BOARD_PREFIX.len());
    if let Err(error) = serde_json::to_writer(&mut counter, subtrees) {
        if counter.exceeded {
            return Err("board clipboard payload exceeds 16 MiB".into());
        }
        return Err(error.to_string());
    }

    // Use temporary IDs above every incoming ID. The eventual board paste
    // allocates above these too, so no pasted identity can alias a source ID.
    let mut next = subtrees
        .iter()
        .flat_map(|subtree| subtree.topics.iter().map(|topic| topic.id.0))
        .max()
        .unwrap_or(1)
        .max(1)
        .checked_add(1)
        .ok_or("board clipboard exhausts available IDs")?;
    let mut fragment = Sheet::new("", "");
    fragment.root_mode = SheetRootMode::Hidden;
    fragment.scheduling.calendar.working_weekdays = Weekday::ALL.into();
    let mut previous_height = 0.0;
    let mut y = origin.y;
    for (index, subtree) in subtrees.iter().enumerate() {
        if subtree.topics.is_empty() {
            return Err("board clipboard subtree is empty".into());
        }
        subtree.resources.validate()?;
        let mut ids = HashMap::new();
        for topic in &subtree.topics {
            let id = TopicId(next);
            next = next
                .checked_add(1)
                .filter(|next| *next < u64::MAX)
                .ok_or("board clipboard exhausts available IDs")?;
            if ids.insert(topic.id, id).is_some() {
                return Err("duplicate topic ID in board clipboard subtree".into());
            }
        }
        let map = |id: TopicId| {
            ids.get(&id)
                .copied()
                .ok_or_else(|| format!("board clipboard subtree topic {id} is not included"))
        };
        let root = subtree
            .topics
            .iter()
            .find(|topic| topic.id == subtree.root)
            .ok_or("board clipboard subtree root is missing")?;
        let height = root.card.map_or(48.0, |card| card.size.y.max(48.0));
        if index != 0 {
            y += previous_height / 2.0 + height / 2.0 + 20.0;
        }
        let position = Vec2::new(origin.x, y);
        if !valid_position(position) {
            return Err("board clipboard position is out of range".into());
        }
        previous_height = height;
        for original in &subtree.topics {
            let mut topic = original.clone();
            topic.id = map(original.id)?;
            if original.id == subtree.root {
                topic.parent = None;
                topic.position = Some(position);
            } else {
                topic.parent = topic.parent.map(map).transpose()?;
            }
            topic.children = topic
                .children
                .iter()
                .copied()
                .map(map)
                .collect::<Result<_, _>>()?;
            fragment.topics.insert(topic.id, topic);
        }
        fragment.floating.push(map(subtree.root)?);
        for (&id, task) in &subtree.tasks {
            fragment.scheduling.tasks.insert(map(id)?, task.clone());
        }
        for dependency in &subtree.dependencies {
            let mut dependency = *dependency;
            dependency.predecessor = map(dependency.predecessor)?;
            dependency.successor = map(dependency.successor)?;
            fragment.scheduling.dependencies.push(dependency);
        }
    }
    fragment.resources = fragment.resources.merged(
        subtrees
            .iter()
            .flat_map(|subtree| subtree.resources.entries.values().cloned()),
    )?;

    // The existing topic-paste path also accepts an embedded resource already
    // in the destination when an older subtree omits its own copy.
    for topic in fragment.topics.values() {
        for reference in topic
            .image
            .as_deref()
            .into_iter()
            .chain(topic.attachment.as_ref().map(|a| a.resource.as_str()))
        {
            if let Some(id) = reference.strip_prefix(crate::resources::RESOURCE_PREFIX)
                && !fragment.resources.entries.contains_key(id)
                && let Some(resource) = sheet.resources.entries.get(id)
            {
                fragment
                    .resources
                    .entries
                    .insert(id.to_owned(), resource.clone());
            }
        }
    }
    let payload = BoardClipboardPayload {
        text: "[Board selection]\n".into(),
        fragment,
    };
    Ok(paste_board_selection(sheet, &payload, Vec2::ZERO)?.topics)
}

/// Fresh IDs of the pasted topic roots and canvas objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardPasteResult {
    pub topics: Vec<TopicId>,
    pub objects: Vec<CanvasObjectId>,
}

/// Pastes a board selection as independent floating topics and canvas objects.
///
/// `delta` translates the entire copied selection in sheet coordinates. All
/// topic, object, group and annotation IDs are fresh, internal references are
/// remapped, and free connector points and ink points move with the selection.
/// Resource limits, coordinates, groups and the complete result are validated
/// on a clone; failure never changes `sheet`. Wrap this in
/// `Document::edit_sheet` for one undoable operation.
pub fn paste_board_selection(
    sheet: &mut Sheet,
    payload: &BoardClipboardPayload,
    delta: Vec2,
) -> Result<BoardPasteResult, String> {
    payload.validate()?;
    if sheet.fim.is_some() {
        return Err("board fragments cannot be pasted into a FIM sheet".into());
    }
    if !valid_position(delta) {
        return Err("board clipboard offset is out of range".into());
    }
    let source = &payload.fragment;
    let mut candidate = sheet.clone();
    candidate.resources = candidate
        .resources
        .merged(source.resources.entries.values().cloned())?;

    let count = source
        .topics
        .len()
        .saturating_sub(1)
        .checked_add(source.canvas.objects.len())
        .and_then(|n| n.checked_add(source.canvas.groups.len()))
        .and_then(|n| n.checked_add(source.relationships.len()))
        .and_then(|n| n.checked_add(source.boundaries.len()))
        .and_then(|n| n.checked_add(source.summaries.len()))
        .ok_or("board clipboard has too many IDs")?;
    // Reserve a consecutive range using the sheet's shared allocator. Avoid
    // source IDs too, even when pasting into a different, initially empty
    // sheet. Only one full destination scan is needed; its next allocation
    // will bump past all inserted IDs.
    let source_max = source
        .topics
        .keys()
        .filter(|id| **id != source.root)
        .map(|id| id.0)
        .chain(source.canvas.objects.iter().map(|object| object.id.0))
        .chain(source.canvas.groups.iter().map(|group| group.id.0))
        .chain(source.relationships.iter().map(|item| item.id.0))
        .chain(source.boundaries.iter().map(|item| item.id.0))
        .chain(source.summaries.iter().map(|item| item.id.0))
        .max()
        .unwrap_or(0);
    let after_source = source_max
        .checked_add(1)
        .ok_or("board clipboard exhausts available IDs")?;
    // The shared allocator first advances past every existing ID. Guard its
    // arithmetic before calling it on an otherwise-valid but hostile sheet.
    if candidate
        .topics
        .keys()
        .map(|id| id.0)
        .chain(candidate.canvas.objects.iter().map(|object| object.id.0))
        .chain(candidate.canvas.groups.iter().map(|group| group.id.0))
        .chain(candidate.relationships.iter().map(|item| item.id.0))
        .chain(candidate.boundaries.iter().map(|item| item.id.0))
        .chain(candidate.summaries.iter().map(|item| item.id.0))
        .chain(candidate.zones.iter().map(|item| item.id.0))
        .chain(candidate.presentation.slides.iter().map(|item| item.id.0))
        .any(|id| id >= u64::MAX - 1)
    {
        return Err("destination sheet exhausts available IDs".into());
    }
    let first = candidate.alloc().max(after_source);
    let end = first
        .checked_add(count as u64)
        .filter(|end| *end < u64::MAX)
        .ok_or("board clipboard exhausts available IDs")?;
    let mut next = first;
    let mut fresh = || {
        let id = next;
        next += 1;
        id
    };
    let topic_map: BTreeMap<_, _> = source
        .topics
        .keys()
        .copied()
        .filter(|id| *id != source.root)
        .map(|id| (id, TopicId(fresh())))
        .collect();
    let object_map: HashMap<_, _> = source
        .canvas
        .objects
        .iter()
        .map(|object| (object.id, CanvasObjectId(fresh())))
        .collect();
    let group_map: HashMap<_, _> = source
        .canvas
        .groups
        .iter()
        .map(|group| (group.id, CanvasGroupId(fresh())))
        .collect();
    let map_topic = |id: TopicId| {
        topic_map
            .get(&id)
            .copied()
            .ok_or_else(|| format!("board clipboard topic {id} is not included"))
    };
    let map_object = |id: CanvasObjectId| {
        object_map
            .get(&id)
            .copied()
            .ok_or_else(|| format!("board clipboard object {id} is not included"))
    };
    let map_group = |id: CanvasGroupId| {
        group_map
            .get(&id)
            .copied()
            .ok_or_else(|| format!("board clipboard group {id} is not included"))
    };
    let translate = |point: Vec2| {
        let point = point + delta;
        if valid_position(point) {
            Ok(point)
        } else {
            Err("board clipboard position is out of range".to_string())
        }
    };

    for (&id, original) in &source.topics {
        if id == source.root {
            continue;
        }
        let mut topic = original.clone();
        topic.id = map_topic(id)?;
        topic.parent = topic.parent.map(map_topic).transpose()?;
        topic.children = topic
            .children
            .iter()
            .copied()
            .map(&map_topic)
            .collect::<Result<_, _>>()?;
        topic.position = topic.position.map(translate).transpose()?;
        candidate.topics.insert(topic.id, topic);
    }
    let roots: Vec<_> = source
        .floating
        .iter()
        .copied()
        .map(&map_topic)
        .collect::<Result<_, _>>()?;
    candidate.floating.extend(roots.iter().copied());
    for (&id, task) in &source.scheduling.tasks {
        candidate
            .scheduling
            .tasks
            .insert(map_topic(id)?, task.clone());
    }
    for dependency in &source.scheduling.dependencies {
        let mut dependency = *dependency;
        dependency.predecessor = map_topic(dependency.predecessor)?;
        dependency.successor = map_topic(dependency.successor)?;
        candidate.scheduling.dependencies.push(dependency);
    }
    for relationship in &source.relationships {
        let mut relationship = relationship.clone();
        relationship.id = crate::model::RelationshipId(fresh());
        relationship.from = map_topic(relationship.from)?;
        relationship.to = map_topic(relationship.to)?;
        candidate.relationships.push(relationship);
    }
    for boundary in &source.boundaries {
        let mut boundary = boundary.clone();
        boundary.id = crate::model::BoundaryId(fresh());
        boundary.parent = map_topic(boundary.parent)?;
        candidate.boundaries.push(boundary);
    }
    for summary in &source.summaries {
        let mut summary = summary.clone();
        summary.id = crate::model::SummaryId(fresh());
        summary.parent = map_topic(summary.parent)?;
        summary.topic = map_topic(summary.topic)?;
        candidate.summaries.push(summary);
    }
    debug_assert_eq!(next, end);

    for group in &source.canvas.groups {
        let mut group = *group;
        group.id = map_group(group.id)?;
        candidate.canvas.groups.push(group);
    }
    for state in &source.canvas.card_states {
        let mut state = *state;
        state.topic = map_topic(state.topic)?;
        state.group = state.group.map(map_group).transpose()?;
        candidate.canvas.card_states.push(state);
    }
    let mut pasted_objects = Vec::new();
    for original in &source.canvas.objects {
        let mut object = original.clone();
        object.id = map_object(original.id)?;
        object.group = object.group.map(map_group).transpose()?;
        object.rect = object.rect.translate(delta);
        match &mut object.kind {
            CanvasObjectKind::Ink(ink) => {
                ink.points = ink
                    .points
                    .iter()
                    .copied()
                    .map(&translate)
                    .collect::<Result<Vec<_>, _>>()?
                    .into();
            }
            CanvasObjectKind::Connector(connector) => {
                let remap = |end| match end {
                    Endpoint::Topic(id) => map_topic(id).map(Endpoint::Topic),
                    Endpoint::Object(id) => map_object(id).map(Endpoint::Object),
                    Endpoint::Point(point) => translate(point).map(Endpoint::Point),
                };
                connector.from = remap(connector.from)?;
                connector.to = remap(connector.to)?;
                if !connector.waypoints.is_empty() {
                    connector.waypoints = connector
                        .waypoints
                        .iter()
                        .copied()
                        .map(&translate)
                        .collect::<Result<Vec<_>, _>>()?
                        .into();
                }
            }
            CanvasObjectKind::Shape { .. } => {}
        }
        pasted_objects.push(object.id);
        candidate.canvas.objects.push(object);
    }
    candidate.validate()?;
    *sheet = candidate;
    Ok(BoardPasteResult {
        topics: roots,
        objects: pasted_objects,
    })
}

#[cfg(test)]
mod tests;
