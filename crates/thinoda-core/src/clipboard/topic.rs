//! Portable, versioned encoding for the existing topic clipboard payload.

use super::{ClipboardPayload, MAX_BOARD_CLIPBOARD_BYTES};
use crate::board::{
    CardKind, MAX_BOARD_COORDINATE, MAX_CANVAS_OBJECTS, MAX_STROKE_WIDTH, validate_board,
};
use crate::model::{Sheet, Topic, TopicId, valid_position};
use crate::resources::{MAX_RESOURCE_BYTES, MAX_RESOURCES};
use crate::rich_text::MAX_INLINE_FONT_SIZE;
use crate::schedule::Weekday;
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

mod strict_json;

/// Version 1 of the portable topic clipboard, including its terminating LF.
pub const TOPIC_CLIPBOARD_PREFIX: &str = "THINODA-TOPIC-CLIPBOARD-1\n";

const SIZE_ERROR: &str = "topic clipboard payload exceeds 16 MiB";
const MAX_TOPIC_CLIPBOARD_TOPICS: usize = MAX_CANVAS_OBJECTS;
/// Maximum copied topic depth in edges, with the selection root at depth zero.
pub const MAX_TOPIC_CLIPBOARD_DEPTH: usize = 256;

/// Encodes a self-contained topic selection without changing its text or trees.
///
/// The returned string, including the version prefix, is at most 16 MiB.
pub fn encode_topic_payload(payload: &ClipboardPayload) -> Result<String, String> {
    validate_topic_count(payload)?;
    // Resource serialization base64-encodes each blob. Check their individual
    // lengths first, so that even this temporary allocation is bounded. Full
    // resource validation follows the bounded serialization below.
    for subtree in &payload.subtrees {
        if subtree.resources.entries.len() > MAX_RESOURCES
            || subtree.resources.entries.values().any(|resource| {
                resource.bytes().is_empty() || resource.bytes().len() > MAX_RESOURCE_BYTES
            })
        {
            return Err("topic clipboard contains an oversized or empty resource".into());
        }
    }

    let mut output = BoundedWriter::new();
    output
        .write_all(TOPIC_CLIPBOARD_PREFIX.as_bytes())
        .map_err(|error| error.to_string())?;
    if let Err(error) = serde_json::to_writer(&mut output, payload) {
        return Err(if output.exceeded {
            SIZE_ERROR.into()
        } else {
            error.to_string()
        });
    }
    validate_payload(payload)?;
    String::from_utf8(output.bytes).map_err(|error| error.to_string())
}

/// Decodes and validates the portable topic selection.
///
/// Both LF and CRLF after the prefix are accepted. Unknown versions and input
/// exceeding 16 MiB are rejected before attempting to parse JSON.
pub fn decode_topic_payload(text: &str) -> Result<ClipboardPayload, String> {
    if text.len() > MAX_BOARD_CLIPBOARD_BYTES {
        return Err(SIZE_ERROR.into());
    }
    let json = text
        .strip_prefix(TOPIC_CLIPBOARD_PREFIX)
        .or_else(|| text.strip_prefix("THINODA-TOPIC-CLIPBOARD-1\r\n"))
        .ok_or("clipboard does not contain a supported Thinoda topic selection")?;
    let payload: ClipboardPayload = strict_json::from_str(json)?;
    validate_payload(&payload)?;
    Ok(payload)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl BoundedWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            exceeded: false,
        }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BOARD_CLIPBOARD_BYTES.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other("topic clipboard size limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn validate_topic_count(payload: &ClipboardPayload) -> Result<(), String> {
    let mut total = 0usize;
    for subtree in &payload.subtrees {
        total = total
            .checked_add(subtree.topics.len())
            .ok_or("topic clipboard has too many topics")?;
        if total > MAX_TOPIC_CLIPBOARD_TOPICS {
            return Err(format!(
                "topic clipboard exceeds {MAX_TOPIC_CLIPBOARD_TOPICS} topics"
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_source_selection(sheet: &Sheet, roots: &[TopicId]) -> Result<(), String> {
    let mut count = 0usize;
    let mut text_bytes = 0usize;
    for &root in roots {
        let mut visited = HashSet::new();
        let mut pending = vec![(root, 0usize)];
        while let Some((id, depth)) = pending.pop() {
            if depth > MAX_TOPIC_CLIPBOARD_DEPTH {
                return Err(format!(
                    "topic clipboard subtree depth exceeds {MAX_TOPIC_CLIPBOARD_DEPTH}"
                ));
            }
            if !visited.insert(id) {
                return Err(format!("repeated topic clipboard ownership of {id}"));
            }
            count += 1;
            if count > MAX_TOPIC_CLIPBOARD_TOPICS {
                return Err(format!(
                    "topic clipboard exceeds {MAX_TOPIC_CLIPBOARD_TOPICS} topics"
                ));
            }
            let topic = sheet
                .topic(id)
                .ok_or_else(|| format!("topic clipboard topic {id} is missing"))?;
            text_bytes = text_bytes
                .checked_add(depth)
                .and_then(|size| size.checked_add(topic.title.len()))
                .and_then(|size| size.checked_add(1))
                .ok_or(SIZE_ERROR)?;
            if text_bytes > MAX_BOARD_CLIPBOARD_BYTES {
                return Err(SIZE_ERROR.into());
            }
            pending.extend(topic.children.iter().map(|child| (*child, depth + 1)));
        }
    }
    Ok(())
}

fn validate_payload(payload: &ClipboardPayload) -> Result<(), String> {
    if payload.subtrees.is_empty() {
        return Err("topic clipboard selection is empty".into());
    }
    validate_topic_count(payload)?;

    // The validation sheet is never returned or pasted. Its synthetic anchor
    // lets roots (including cards) be validated as floating roots. Like the
    // existing paste path, remap each subtree independently: separate legacy
    // subtrees can legitimately reuse the same source IDs.
    let mut fragment = Sheet::new("", "");
    // Validate references against each subtree's own store before combining
    // stores. Otherwise an unrelated subtree could mask a missing resource.
    let mut resource_fragment = Sheet::new("", "");
    resource_fragment.topics.clear();
    let mut next_id = 2u64;
    // Clipboard subtrees do not carry the source calendar. Any valid start
    // date could have been a working day there, including a weekend.
    fragment.scheduling.calendar.working_weekdays = Weekday::ALL.into();

    for subtree in &payload.subtrees {
        subtree.resources.validate()?;
        let mut topics = HashMap::new();
        let mut remapped = HashMap::new();
        for topic in &subtree.topics {
            if topics.insert(topic.id, topic).is_some() {
                return Err(format!("duplicate topic clipboard ID {}", topic.id));
            }
            remapped.insert(topic.id, TopicId(next_id));
            next_id = next_id
                .checked_add(1)
                .ok_or("topic clipboard exhausts available IDs")?;
        }
        let map_id = |id: TopicId| {
            remapped
                .get(&id)
                .copied()
                .ok_or_else(|| format!("topic clipboard topic {id} is not owned by its subtree"))
        };
        let root = topics
            .get(&subtree.root)
            .ok_or("topic clipboard subtree root is missing")?;
        if root
            .parent
            .is_some_and(|parent| topics.contains_key(&parent))
        {
            return Err("topic clipboard root is owned by a copied topic".into());
        }

        let mut children = HashSet::new();
        for topic in &subtree.topics {
            validate_topic_fields(topic)?;
            if topic.card.is_some() && (topic.id != subtree.root || topic.parent.is_some()) {
                return Err(format!("topic {} is not a floating card root", topic.id));
            }
            for child_id in &topic.children {
                let child = topics
                    .get(child_id)
                    .ok_or_else(|| format!("topic clipboard child {child_id} is missing"))?;
                if *child_id == subtree.root
                    || child.parent != Some(topic.id)
                    || !children.insert(*child_id)
                {
                    return Err(format!(
                        "invalid ownership of topic clipboard child {child_id}"
                    ));
                }
            }
            if topic.id != subtree.root
                && topic
                    .parent
                    .is_none_or(|parent| !topics.contains_key(&parent))
            {
                return Err(format!("topic clipboard parent of {} is missing", topic.id));
            }
        }

        // A disconnected cycle can satisfy parent/child consistency, so walk
        // from the root as well. Bound depth before a destination can feed the
        // tree to recursive layout. This walk is iterative and linear.
        let mut visited = HashSet::new();
        let mut pending = vec![(subtree.root, 0usize)];
        while let Some((id, depth)) = pending.pop() {
            if depth > MAX_TOPIC_CLIPBOARD_DEPTH {
                return Err(format!(
                    "topic clipboard subtree depth exceeds {MAX_TOPIC_CLIPBOARD_DEPTH}"
                ));
            }
            if !visited.insert(id) {
                return Err(format!("repeated topic clipboard ownership of {id}"));
            }
            let topic = topics
                .get(&id)
                .ok_or_else(|| format!("topic clipboard topic {id} is missing"))?;
            pending.extend(topic.children.iter().map(|child| (*child, depth + 1)));
        }
        if visited.len() != topics.len() {
            return Err("topic clipboard contains unreachable topics".into());
        }

        for (&id, task) in &subtree.tasks {
            if !topics.contains_key(&id) {
                return Err(format!("topic clipboard task {id} has no owned topic"));
            }
            fragment.scheduling.tasks.insert(map_id(id)?, task.clone());
        }
        for dependency in &subtree.dependencies {
            if !topics.contains_key(&dependency.predecessor)
                || !topics.contains_key(&dependency.successor)
            {
                return Err("topic clipboard dependency references an unowned topic".into());
            }
            fragment
                .scheduling
                .dependencies
                .push(crate::schedule::TaskDependency {
                    predecessor: map_id(dependency.predecessor)?,
                    successor: map_id(dependency.successor)?,
                });
        }

        resource_fragment.resources = subtree.resources.clone();
        for original in &subtree.topics {
            let mut topic = original.clone();
            topic.id = map_id(original.id)?;
            if original.id == subtree.root {
                // copy_subtree retains a root's parent from the source sheet.
                topic.parent = None;
            } else {
                topic.parent = original.parent.map(map_id).transpose()?;
            }
            topic.children = original
                .children
                .iter()
                .copied()
                .map(map_id)
                .collect::<Result<_, _>>()?;
            resource_fragment.topics.insert(topic.id, topic);
        }
        resource_fragment.validate_resource_references()?;
        fragment.topics.append(&mut resource_fragment.topics);
        for (id, resource) in &subtree.resources.entries {
            fragment
                .resources
                .entries
                .entry(id.clone())
                .or_insert_with(|| resource.clone());
        }
        fragment.floating.push(map_id(subtree.root)?);
    }

    // Reuse the model's resource, scheduling and board validators. In
    // particular, dependencies must refer to tasks and form an acyclic graph.
    fragment.validate_resources()?;
    fragment
        .scheduling
        .validate(&fragment)
        .map_err(|error| error.to_string())?;
    validate_board(&fragment)
}

fn validate_topic_fields(topic: &Topic) -> Result<(), String> {
    if topic
        .position
        .is_some_and(|position| !valid_position(position))
    {
        return Err(format!("topic {} has an invalid position", topic.id));
    }
    // serde_json represents non-finite floats as null. Reject them rather than
    // silently losing an in-memory style override during encoding.
    let style = &topic.style;
    if style
        .font_size
        .is_some_and(|size| !size.is_finite() || size <= 0.0 || size > MAX_INLINE_FONT_SIZE)
    {
        return Err(format!(
            "topic {} font size must be positive and at most {MAX_INLINE_FONT_SIZE}",
            topic.id
        ));
    }
    // Use the broader board stroke/coordinate safety limits, not the narrower
    // ranges for editable quick styles. Negative lengths and extreme values
    // can make downstream layout/paint arithmetic invalid or overflow.
    for (name, value, max) in [
        ("border width", style.border_width, MAX_STROKE_WIDTH),
        ("line width", style.line_width, MAX_STROKE_WIDTH),
        ("width", style.width, MAX_BOARD_COORDINATE),
        (
            "horizontal padding",
            style.padding.map(|padding| padding.x),
            MAX_BOARD_COORDINATE,
        ),
        (
            "vertical padding",
            style.padding.map(|padding| padding.y),
            MAX_BOARD_COORDINATE,
        ),
    ] {
        if value.is_some_and(|value| !value.is_finite() || !(0.0..=max).contains(&value)) {
            return Err(format!(
                "topic {} {name} must be finite and between 0 and {max}",
                topic.id
            ));
        }
    }
    if !topic.inline_text.is_empty() {
        if !topic
            .card
            .is_some_and(|card| matches!(card.kind, CardKind::Sticky | CardKind::Text))
        {
            return Err(format!(
                "topic {} has inline formatting without a sticky or text card",
                topic.id
            ));
        }
        topic
            .inline_text
            .validate(&topic.title)
            .map_err(|error| format!("topic {}: {error}", topic.id))?;
    }
    if topic
        .marker_colors
        .keys()
        .any(|index| *index >= topic.markers.len())
    {
        return Err(format!(
            "topic {} has a color for a missing marker",
            topic.id
        ));
    }
    if topic
        .child_numbering
        .is_some_and(|numbering| !numbering.valid())
    {
        return Err(format!("topic {} has invalid numbering", topic.id));
    }
    Ok(())
}
