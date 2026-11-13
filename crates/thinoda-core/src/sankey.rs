//! Semantic flows on topic-to-topic canvas connectors.
//!
//! The mind-map ownership tree is deliberately not the flow graph. Derived mass,
//! port offsets and ribbon polygons are disposable views, never document state.
use crate::board::{CanvasObjectId, CanvasObjectKind, Endpoint};
use crate::{Color, Sheet, TopicId, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const WEIGHT_COLOR_SCALE_MAX: f64 = 100.0;
pub const WEIGHT_COLOR_SCALE_MARKS: [f64; 5] = [0.0, 25.0, 50.0, 75.0, WEIGHT_COLOR_SCALE_MAX];
pub const WEIGHT_COLOR_SCALE_GRADIENT_STEPS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SankeyMode {
    #[default]
    Standard,
    Alluvial,
    Probability,
    Circular,
}

impl SankeyMode {
    pub const ALL: [Self; 4] = [
        Self::Standard,
        Self::Alluvial,
        Self::Probability,
        Self::Circular,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard Sankey",
            Self::Alluvial => "Alluvial",
            Self::Probability => "Conditional probability",
            Self::Circular => "Circular Sankey",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SankeySettings {
    pub mode: SankeyMode,
    pub color_scale: bool,
    /// Global, linear sheet-unit scale. It may shrink uniformly to fit node ports.
    pub pixels_per_unit: f32,
    /// Explicit mutually exclusive source masses, only in probability mode.
    pub sources: BTreeMap<TopicId, u32>,
    /// Alluvial stage identity is attached to topic IDs, never display labels.
    pub stages: BTreeMap<TopicId, u32>,
    /// Optional display names keyed by stage number.
    #[serde(default)]
    pub stage_names: BTreeMap<u32, String>,
}

impl Default for SankeySettings {
    fn default() -> Self {
        Self {
            mode: SankeyMode::Standard,
            color_scale: false,
            pixels_per_unit: 2.0,
            sources: BTreeMap::new(),
            stages: BTreeMap::new(),
            stage_names: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    /// None is unassigned, not zero and not an equal-share request.
    pub weight: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlowValues {
    pub mass: BTreeMap<CanvasObjectId, f64>,
    pub incoming: BTreeMap<TopicId, f64>,
    pub outgoing: BTreeMap<TopicId, f64>,
    pub nodes: BTreeSet<TopicId>,
    pub order: Vec<TopicId>,
}

pub fn has_sankey(sheet: &Sheet) -> bool {
    sheet.sankey.is_some()
        || sheet
            .canvas
            .objects
            .iter()
            .any(|o| matches!(&o.kind, CanvasObjectKind::Connector(c) if c.sankey.is_some()))
}

pub fn endpoints(c: &crate::board::CanvasConnector) -> Result<(TopicId, TopicId), String> {
    match (c.from, c.to) {
        (Endpoint::Topic(from), Endpoint::Topic(to)) => Ok((from, to)),
        _ => Err("Sankey links must connect two existing topics.".into()),
    }
}

/// Thousandths of a percentage point: exact decimal edits and exact caps.
pub const FULL_WEIGHT: u32 = 100_000;

pub fn percent(v: u32) -> f64 {
    f64::from(v) / 1000.0
}

pub fn from_percent(v: f64) -> Result<u32, String> {
    if !v.is_finite() || !(0.0..=100.0).contains(&v) {
        return Err("Weights must be finite percentages between 0 and 100.".into());
    }
    let units = v * 1000.0;
    // Tolerate only relative floating-evaluation noise, not a fixed epsilon
    // which could erase small derived supply (including near a nonzero unit).
    let tolerance = 64.0 * f64::EPSILON * units.abs().max(1.0);
    if (v > 0.0 && units.round() == 0.0) || (units - units.round()).abs() > tolerance {
        return Err(
            "This conversion needs more than three decimal places; no weights were changed.".into(),
        );
    }
    Ok(units.round() as u32)
}

fn node_name(sheet: &Sheet, id: TopicId) -> &str {
    sheet.topic(id).map_or("?", |topic| topic.title.as_str())
}

pub const ROTATED_ENDPOINT_ERROR: &str = "Sankey nodes cannot be rotated. Remove their flow links before rotating; existing rotation is not changed.";
pub const HIDDEN_ENDPOINT_ERROR: &str = "Sankey endpoints are hidden by collapsed ancestors. Expand those ancestors before opening or pasting this diagram.";

fn cap_error(sheet: &Sheet, id: TopicId, direction: &str, total: f64) -> String {
    format!(
        "{} — {direction} total {total:.3}% exceeds 100%; reduce by {:.3}%. The whole edit was rejected.",
        node_name(sheet, id),
        total - 100.0
    )
}

/// Validate without mutation; callers commit the entire edit or reject it.
pub fn evaluate(sheet: &Sheet) -> Result<FlowValues, String> {
    let mut result = FlowValues::default();
    let Some(settings) = &sheet.sankey else {
        return if has_sankey(sheet) {
            Err("Sankey links require sheet settings.".into())
        } else {
            Ok(result)
        };
    };
    if !settings.pixels_per_unit.is_finite() || !(0.1..=10.0).contains(&settings.pixels_per_unit) {
        return Err("Sankey width scale must be between 0.1 and 10.".into());
    }
    let mut edges = Vec::new();
    let mut degree = BTreeMap::<TopicId, usize>::new();
    let mut conditional = BTreeMap::<TopicId, u64>::new();
    let mut absolute_in = BTreeMap::<TopicId, u64>::new();
    let mut adjacency = BTreeMap::<TopicId, Vec<usize>>::new();
    for object in &sheet.canvas.objects {
        let CanvasObjectKind::Connector(c) = &object.kind else {
            continue;
        };
        let Some(flow) = c.sankey else { continue };
        let (from, to) = endpoints(c)?;
        if !sheet.contains(from)
            || !sheet.contains(to)
            || sheet.is_anchor(from)
            || sheet.is_anchor(to)
        {
            return Err("Sankey link references a missing or hidden topic.".into());
        }
        for id in [from, to] {
            if sheet
                .topic(id)
                .and_then(|t| t.card)
                .is_some_and(|c| c.rotation != 0.0)
            {
                return Err(format!(
                    "{}: {ROTATED_ENDPOINT_ERROR}",
                    node_name(sheet, id)
                ));
            }
            if !sheet.is_visible(id) {
                return Err(format!("{}: {HIDDEN_ENDPOINT_ERROR}", node_name(sheet, id)));
            }
        }
        if flow.weight.is_some_and(|v| v > FULL_WEIGHT) {
            return Err("Weights must be finite percentages between 0 and 100.".into());
        }
        result.nodes.extend([from, to]);
        degree.entry(from).or_default();
        *degree.entry(to).or_default() += 1;
        *conditional.entry(from).or_default() += u64::from(flow.weight.unwrap_or(0));
        *absolute_in.entry(to).or_default() += u64::from(flow.weight.unwrap_or(0));
        adjacency.entry(from).or_default().push(edges.len());
        edges.push((object.id, from, to, percent(flow.weight.unwrap_or(0))));
        if settings.mode == SankeyMode::Alluvial {
            match (settings.stages.get(&from), settings.stages.get(&to)) {
                (Some(a), Some(b)) if a.checked_add(1) == Some(*b) => {}
                _ => {
                    return Err(format!(
                        "Alluvial links must connect adjacent stages: {} (stage {:?}) → {} (stage {:?}). Assign consecutive earlier/later stages.",
                        node_name(sheet, from),
                        settings.stages.get(&from),
                        node_name(sheet, to),
                        settings.stages.get(&to)
                    ));
                }
            }
        }
    }
    for (&id, &mass) in &settings.sources {
        if !sheet.contains(id) || sheet.is_anchor(id) || mass > FULL_WEIGHT {
            return Err("Invalid Sankey probability source.".into());
        }
        if settings.mode != SankeyMode::Probability {
            return Err("Source masses require probability mode; convert explicitly.".into());
        }
        if degree.get(&id).copied().unwrap_or(0) != 0 {
            return Err("A probability source cannot have incoming links. Remove its explicit source mass before adding a parent.".into());
        }
        result.nodes.insert(id);
        degree.entry(id).or_default();
    }
    if settings
        .stages
        .keys()
        .any(|id| !sheet.contains(*id) || sheet.is_anchor(*id))
    {
        return Err("Alluvial stage references a missing topic.".into());
    }
    if settings.mode == SankeyMode::Alluvial {
        for &id in settings.stages.keys() {
            result.nodes.insert(id);
            degree.entry(id).or_default();
        }
    }
    let mut queue: VecDeque<_> = degree
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(id, _)| *id)
        .collect();
    while let Some(id) = queue.pop_front() {
        result.order.push(id);
        for &index in adjacency.get(&id).into_iter().flatten() {
            let to = edges[index].2;
            let n = degree.get_mut(&to).ok_or("Missing flow node")?;
            *n -= 1;
            if *n == 0 {
                queue.push_back(to);
            }
        }
    }
    let cyclic = result.order.len() != result.nodes.len();
    if cyclic && settings.mode != SankeyMode::Circular {
        return Err(
            "Flow cycle detected. Use Circular Sankey; cyclic probability evaluation is ambiguous."
                .into(),
        );
    }
    if settings.mode == SankeyMode::Probability {
        if settings
            .sources
            .values()
            .map(|v| u64::from(*v))
            .sum::<u64>()
            > u64::from(FULL_WEIGHT)
        {
            return Err("Combined source mass would exceed 100%. Reduce another explicit source mass first.".into());
        }
        for (&id, &total) in &conditional {
            if total > u64::from(FULL_WEIGHT) {
                return Err(cap_error(
                    sheet,
                    id,
                    "Children conditional",
                    total as f64 / 1000.0,
                ));
            }
        }
        for &id in &result.order {
            let incoming = result.incoming.get(&id).copied().unwrap_or(0.0);
            let mass = settings
                .sources
                .get(&id)
                .copied()
                .map(percent)
                .unwrap_or(incoming);
            if !absolute_in.contains_key(&id) && !settings.sources.contains_key(&id) {
                return Err(
                    "Every probability source needs an explicit source mass (including zero)."
                        .into(),
                );
            }
            for &index in adjacency.get(&id).into_iter().flatten() {
                let (edge, from, to, weight) = edges[index];
                let value = mass * weight / 100.0;
                result.mass.insert(edge, value);
                *result.incoming.entry(to).or_default() += value;
                *result.outgoing.entry(from).or_default() += value;
            }
        }
    } else {
        for (direction, totals) in [("Children", &conditional), ("Parents", &absolute_in)] {
            for (&id, &total) in totals {
                if total > u64::from(FULL_WEIGHT) {
                    return Err(cap_error(sheet, id, direction, total as f64 / 1000.0));
                }
            }
        }
        for &(edge, from, to, value) in &edges {
            result.mass.insert(edge, value);
            *result.incoming.entry(to).or_default() += value;
            *result.outgoing.entry(from).or_default() += value;
        }
    }
    if result
        .incoming
        .values()
        .chain(result.outgoing.values())
        .any(|v| *v > 100.0 + 1e-9)
    {
        return Err("Parents or Children total exceeds 100%; the whole edit was rejected.".into());
    }
    Ok(result)
}

/// Explicit mass-preserving conversion. Never reinterpret conditional weights.
pub fn convert_mode(sheet: &mut Sheet, mode: SankeyMode) -> Result<(), String> {
    let mut candidate = sheet.clone();
    let values = evaluate(sheet)?;
    let old = sheet.sankey.as_ref().map(|s| s.mode).unwrap_or_default();
    let mut settings = sheet.sankey.clone().unwrap_or_default();
    if old == SankeyMode::Probability && mode != old {
        // Absolute links alone cannot retain unused root supply or conditional
        // values on a zero-mass source. Reject rather than silently erase them.
        for (&id, &source) in &settings.sources {
            let outgoing = values.outgoing.get(&id).copied().unwrap_or(0.0);
            if (percent(source) - outgoing).abs() > 1e-9 {
                return Err(format!(
                    "Cannot convert {}: source allocation {:.3}% includes {:.3}% unallocated supply that absolute mode cannot preserve. Allocate it explicitly or keep probability mode; nothing was changed.",
                    node_name(sheet, id),
                    percent(source),
                    percent(source) - outgoing
                ));
            }
        }
        for o in &sheet.canvas.objects {
            if let CanvasObjectKind::Connector(c) = &o.kind
                && let Some(Flow {
                    weight: Some(weight),
                }) = c.sankey
                && weight != 0
                && values.mass.get(&o.id).copied().unwrap_or(0.0) == 0.0
            {
                return Err("Cannot convert: a zero-mass branch has a nonzero conditional percentage that absolute mode cannot preserve. Nothing was changed.".into());
            }
        }
        for o in &mut candidate.canvas.objects {
            if let CanvasObjectKind::Connector(c) = &mut o.kind
                && let Some(flow) = &mut c.sankey
                && flow.weight.is_some()
            {
                let mass = values.mass.get(&o.id).copied().unwrap_or(0.0);
                flow.weight = Some(from_percent(mass)?);
            }
        }
        settings.sources.clear();
    } else if mode == SankeyMode::Probability && old != mode {
        if values.order.len() != values.nodes.len() {
            return Err("Probability conversion requires an acyclic graph.".into());
        }
        // Explicit conversion rule: root supply equals its absolute outgoing
        // total. Downstream supply is its derived incoming total. No equal
        // shares, independence assumptions or normalization of deficits.
        for &id in &values.nodes {
            let outgoing = values.outgoing.get(&id).copied().unwrap_or(0.0);
            if let Some(incoming) = values.incoming.get(&id) {
                if outgoing > *incoming + 1e-9 {
                    return Err(
                        "Cannot convert: a downstream node sends more mass than it receives."
                            .into(),
                    );
                }
            } else {
                settings.sources.insert(id, from_percent(outgoing)?);
            }
        }
        if settings
            .sources
            .values()
            .map(|v| u64::from(*v))
            .sum::<u64>()
            > u64::from(FULL_WEIGHT)
        {
            return Err("Cannot convert: combined root outgoing mass exceeds 100%.".into());
        }
        for o in &mut candidate.canvas.objects {
            if let CanvasObjectKind::Connector(c) = &mut o.kind
                && c.sankey.is_some()
            {
                let (from, _) = endpoints(c)?;
                let supply = settings
                    .sources
                    .get(&from)
                    .copied()
                    .map(percent)
                    .unwrap_or_else(|| values.incoming.get(&from).copied().unwrap_or(0.0));
                if let Some(flow) = &mut c.sankey
                    && flow.weight.is_some()
                {
                    let mass = values.mass.get(&o.id).copied().unwrap_or(0.0);
                    flow.weight = Some(from_percent(if supply == 0.0 {
                        0.0
                    } else {
                        mass * 100.0 / supply
                    })?);
                }
            }
        }
    }
    if mode == SankeyMode::Alluvial && old != mode && settings.stages.is_empty() {
        let mut adjacency = BTreeMap::<TopicId, Vec<TopicId>>::new();
        for o in &candidate.canvas.objects {
            if let CanvasObjectKind::Connector(c) = &o.kind
                && c.sankey.is_some()
                && let Ok((from, to)) = endpoints(c)
            {
                adjacency.entry(from).or_default().push(to);
            }
        }
        for &id in &values.order {
            let rank = *settings.stages.entry(id).or_default();
            for &child in adjacency.get(&id).into_iter().flatten() {
                settings
                    .stages
                    .entry(child)
                    .and_modify(|r| *r = (*r).max(rank + 1))
                    .or_insert(rank + 1);
            }
        }
    }
    settings.mode = mode;
    candidate.sankey = Some(settings);
    reconcile(sheet, &mut candidate)?;
    *sheet = candidate;
    Ok(())
}

/// Alluvial stage columns in ascending order and flow nodes without a stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageSummary {
    pub stages: Vec<(u32, Vec<TopicId>)>,
    pub unassigned: Vec<TopicId>,
}

fn flow_links(sheet: &Sheet) -> Vec<(TopicId, TopicId)> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|o| match &o.kind {
            CanvasObjectKind::Connector(c) if c.sankey.is_some() => endpoints(c).ok(),
            _ => None,
        })
        .collect()
}

pub fn stage_summary(sheet: &Sheet) -> StageSummary {
    let Some(settings) = &sheet.sankey else {
        return StageSummary::default();
    };
    let mut columns = BTreeMap::<u32, Vec<TopicId>>::new();
    for (&id, &stage) in &settings.stages {
        columns.entry(stage).or_default().push(id);
    }
    let unassigned: BTreeSet<_> = flow_links(sheet)
        .into_iter()
        .flat_map(|(a, b)| [a, b])
        .filter(|id| !settings.stages.contains_key(id))
        .collect();
    StageSummary {
        stages: columns.into_iter().collect(),
        unassigned: unassigned.into_iter().collect(),
    }
}

fn commit_stages(sheet: &mut Sheet, stages: BTreeMap<TopicId, u32>) -> Result<(), String> {
    let mut candidate = sheet.clone();
    if let Some(settings) = &mut candidate.sankey {
        settings.stages = stages;
    }
    reconcile(sheet, &mut candidate)?;
    evaluate(&candidate)?;
    *sheet = candidate;
    Ok(())
}

/// Derive stages for unassigned flow nodes from linked neighbours: a child is
/// one stage after its parent and a parent one stage before its child. Each
/// connected component is solved as a whole, so the result does not depend on
/// topic order; components without any stage start at 0. Existing stages
/// never change, and the whole edit is rejected if the result is not valid.
pub fn assign_missing_stages(sheet: &mut Sheet) -> Result<usize, String> {
    let Some(settings) = &sheet.sankey else {
        return Err("Enable Sankey flows first.".into());
    };
    if settings.mode != SankeyMode::Alluvial {
        return Err("Stages require alluvial mode; convert explicitly.".into());
    }
    let mut neighbours = BTreeMap::<TopicId, Vec<(TopicId, i64)>>::new();
    for (from, to) in flow_links(sheet) {
        neighbours.entry(from).or_default().push((to, 1));
        neighbours.entry(to).or_default().push((from, -1));
    }
    let mut stages = settings.stages.clone();
    let mut visited = BTreeSet::new();
    let mut added = 0;
    for &start in neighbours.keys() {
        if stages.contains_key(&start) || !visited.insert(start) {
            continue;
        }
        // Relative offsets within the component; a conflict means no
        // adjacent-stage placement exists (including directed cycles).
        let mut offset = BTreeMap::from([(start, 0_i64)]);
        let mut queue = VecDeque::from([start]);
        while let Some(id) = queue.pop_front() {
            for &(other, step) in &neighbours[&id] {
                let want = offset[&id] + step;
                match offset.get(&other) {
                    Some(&have) if have != want => {
                        return Err(format!(
                            "{} cannot be placed on a stage adjacent to all its links. Adjust stages or remove a link.",
                            node_name(sheet, other)
                        ));
                    }
                    Some(_) => {}
                    None => {
                        offset.insert(other, want);
                        visited.insert(other);
                        queue.push_back(other);
                    }
                }
            }
        }
        let anchor = offset
            .iter()
            .find_map(|(id, rel)| stages.get(id).map(|&s| i64::from(s) - rel))
            .unwrap_or_else(|| -offset.values().copied().min().unwrap_or(0));
        for (id, rel) in offset {
            if stages.contains_key(&id) {
                continue;
            }
            let stage = u32::try_from(anchor + rel).map_err(|_| {
                format!(
                    "{} would need a stage before 0. Raise the stages of the linked nodes first.",
                    node_name(sheet, id)
                )
            })?;
            stages.insert(id, stage);
            added += 1;
        }
    }
    if added == 0 {
        evaluate(sheet)?;
        return Ok(0);
    }
    commit_stages(sheet, stages)?;
    Ok(added)
}
/// Renumber used stages consecutively from 0. Adjacent stages stay adjacent.
pub fn compact_stages(sheet: &mut Sheet) -> Result<bool, String> {
    let Some(settings) = &sheet.sankey else {
        return Ok(false);
    };
    let used: BTreeSet<u32> = settings.stages.values().copied().collect();
    let map: BTreeMap<u32, u32> = used.into_iter().zip(0..).collect();
    if map.iter().all(|(a, b)| a == b) {
        return Ok(false);
    }
    let stages = settings
        .stages
        .iter()
        .map(|(&id, stage)| (id, map[stage]))
        .collect();
    let mut candidate = sheet.clone();
    if let Some(settings) = &mut candidate.sankey {
        settings.stages = stages;
        settings.stage_names = settings
            .stage_names
            .iter()
            .filter_map(|(&stage, name)| {
                map.get(&stage).map(|&new_stage| (new_stage, name.clone()))
            })
            .collect();
    }
    reconcile(sheet, &mut candidate)?;
    *sheet = candidate;
    Ok(true)
}

pub fn weight_color(value: f64) -> Color {
    let v = (value / WEIGHT_COLOR_SCALE_MAX).clamp(0.0, 1.0) as f32;
    Color::rgb(
        (45.0 + 175.0 * v) as u8,
        (155.0 - 80.0 * v) as u8,
        (210.0 - 135.0 * v) as u8,
    )
}

/// A layer slot: a flow node, or a virtual waypoint on a rank-skipping link
/// (identified by the link's index and its rank) so long links take part in
/// crossing reduction without occupying vertical space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LayerSlot {
    Node(TopicId),
    Waypoint(usize, u32),
}

/// Upper bound on virtual waypoints so long links cannot inflate layout cost;
/// links beyond the budget are left out of crossing reduction.
const MAX_LAYOUT_WAYPOINTS: usize = 20_000;

fn reorder_layer(
    layers: &mut BTreeMap<u32, Vec<LayerSlot>>,
    positions: &mut BTreeMap<LayerSlot, usize>,
    rank: u32,
    neighbors: &BTreeMap<LayerSlot, Vec<(LayerSlot, f64)>>,
) {
    let Some(slots) = layers.get_mut(&rank) else {
        return;
    };
    let mut order: Vec<_> = slots
        .iter()
        .enumerate()
        .map(|(old_index, slot)| {
            let (weighted_position, weight) = neighbors
                .get(slot)
                .into_iter()
                .flatten()
                .filter_map(|(neighbor, edge_weight)| {
                    positions
                        .get(neighbor)
                        .map(|position| (*position as f64 * edge_weight, *edge_weight))
                })
                .fold((0.0, 0.0), |(sum, total), (value, weight)| {
                    (sum + value, total + weight)
                });
            let barycenter = if weight > 0.0 {
                weighted_position / weight
            } else {
                old_index as f64
            };
            (*slot, barycenter, old_index)
        })
        .collect();
    order.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.cmp(&b.2)));
    slots.clear();
    slots.extend(order.into_iter().map(|(slot, _, _)| slot));
    for (index, slot) in slots.iter().enumerate() {
        positions.insert(*slot, index);
    }
}

/// Arrange standalone flow cards without changing mind-map ownership.
pub fn arrange(sheet: &mut Sheet) -> Result<(), String> {
    use crate::board::BoardMember;
    let values = evaluate(sheet)?;
    let settings = sheet.sankey.as_ref().ok_or("Enable Sankey flows first")?;
    for &id in &values.nodes {
        if sheet.topic(id).is_none_or(|t| t.card.is_none()) {
            return Err("Automatic flow layout needs floating cards. Mind-map topic ownership was not changed.".into());
        }
        if sheet.is_board_member_locked(BoardMember::Card(id)) {
            return Err("Unlock all flow nodes before arranging.".into());
        }
    }
    let mut ranks = BTreeMap::<TopicId, u32>::new();
    if settings.mode == SankeyMode::Alluvial {
        ranks = settings.stages.clone();
    } else if settings.mode == SankeyMode::Circular {
        // Keep a deliberate column identity for circular graphs; feedback edges
        // are routed outside these columns. No cyclic ranking recursion.
        let mut columns: Vec<_> = values
            .nodes
            .iter()
            .filter_map(|id| sheet.topic(*id).and_then(|t| t.position).map(|p| p.x))
            .collect();
        columns.sort_by(f32::total_cmp);
        columns.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        for &id in &values.nodes {
            let x = sheet
                .topic(id)
                .and_then(|t| t.position)
                .map_or(0.0, |p| p.x);
            let rank = columns
                .iter()
                .position(|v| (*v - x).abs() < 1.0)
                .unwrap_or(0) as u32;
            ranks.insert(id, rank);
        }
    } else {
        let mut adjacency = BTreeMap::<TopicId, Vec<TopicId>>::new();
        for o in &sheet.canvas.objects {
            if let CanvasObjectKind::Connector(c) = &o.kind
                && c.sankey.is_some()
                && let Ok((a, b)) = endpoints(c)
            {
                adjacency.entry(a).or_default().push(b);
            }
        }
        for id in &values.order {
            let next = ranks.get(id).copied().unwrap_or(0) + 1;
            for &child in adjacency.get(id).into_iter().flatten() {
                ranks
                    .entry(child)
                    .and_modify(|rank| *rank = (*rank).max(next))
                    .or_insert(next);
            }
        }
    }
    let topic_y = |id: TopicId| {
        sheet
            .topic(id)
            .and_then(|topic| topic.position)
            .map_or(0.0, |position| position.y)
    };
    // Initial vertical order per layer: existing y, waypoints interpolated
    // between their link's endpoints.
    let mut layer_y = BTreeMap::<u32, Vec<(f32, LayerSlot)>>::new();
    for &id in &values.nodes {
        layer_y
            .entry(ranks.get(&id).copied().unwrap_or(0))
            .or_default()
            .push((topic_y(id), LayerSlot::Node(id)));
    }

    let mut predecessors = BTreeMap::<LayerSlot, Vec<(LayerSlot, f64)>>::new();
    let mut successors = BTreeMap::<LayerSlot, Vec<(LayerSlot, f64)>>::new();
    let mut waypoint_budget = MAX_LAYOUT_WAYPOINTS;
    for (index, object) in sheet.canvas.objects.iter().enumerate() {
        let CanvasObjectKind::Connector(connector) = &object.kind else {
            continue;
        };
        let Some(flow) = connector.sankey else {
            continue;
        };
        let Ok((from, to)) = endpoints(connector) else {
            continue;
        };
        let rank_of = |id| ranks.get(&id).copied().unwrap_or(0);
        let (from_rank, to_rank) = (rank_of(from), rank_of(to));
        // Backward and same-rank (feedback) links are routed outside columns.
        if to_rank <= from_rank {
            continue;
        }
        let weight = if flow.weight.is_some() {
            values.mass.get(&object.id).copied().unwrap_or_default()
        } else {
            1.0
        };
        if weight == 0.0 {
            continue;
        }
        let waypoints = (to_rank - from_rank - 1) as usize;
        if waypoints > waypoint_budget {
            continue;
        }
        waypoint_budget -= waypoints;
        let (from_y, to_y) = (topic_y(from), topic_y(to));
        let span = (to_rank - from_rank) as f32;
        let mut previous = LayerSlot::Node(from);
        for rank in from_rank + 1..=to_rank {
            let slot = if rank == to_rank {
                LayerSlot::Node(to)
            } else {
                let waypoint = LayerSlot::Waypoint(index, rank);
                let t = (rank - from_rank) as f32 / span;
                layer_y
                    .entry(rank)
                    .or_default()
                    .push((from_y + (to_y - from_y) * t, waypoint));
                waypoint
            };
            predecessors
                .entry(slot)
                .or_default()
                .push((previous, weight));
            successors.entry(previous).or_default().push((slot, weight));
            previous = slot;
        }
    }
    let mut layers: BTreeMap<u32, Vec<LayerSlot>> = layer_y
        .into_iter()
        .map(|(rank, mut slots)| {
            slots.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            (rank, slots.into_iter().map(|(_, slot)| slot).collect())
        })
        .collect();
    let mut positions: BTreeMap<_, _> = layers
        .values()
        .flat_map(|slots| slots.iter().enumerate().map(|(index, slot)| (*slot, index)))
        .collect();
    let layer_ranks: Vec<_> = layers.keys().copied().collect();
    for _ in 0..4 {
        for &rank in &layer_ranks {
            reorder_layer(&mut layers, &mut positions, rank, &predecessors);
        }
        for &rank in layer_ranks.iter().rev() {
            reorder_layer(&mut layers, &mut positions, rank, &successors);
        }
    }

    let scale = settings.pixels_per_unit;
    let mut column_y = BTreeMap::<u32, f32>::new();
    let mut placements = Vec::new();
    for (&rank, slots) in &layers {
        for &slot in slots {
            let LayerSlot::Node(id) = slot else {
                continue;
            };
            let mass = values
                .incoming
                .get(&id)
                .copied()
                .unwrap_or(0.0)
                .max(values.outgoing.get(&id).copied().unwrap_or(0.0));
            let height = (mass as f32 * scale).max(56.0);
            let y = column_y.entry(rank).or_default();
            let position = Vec2::new(rank as f32 * 420.0, *y + height / 2.0);
            if !crate::board::valid_board_position(position)
                || *y + height > crate::board::MAX_BOARD_COORDINATE
            {
                return Err(
                    "Flow layout exceeds the supported board area; no nodes were moved.".into(),
                );
            }
            placements.push((id, position, height));
            *y += height + 48.0;
        }
    }
    for (id, position, height) in placements {
        if let Some(topic) = sheet.topics.get_mut(&id)
            && let Some(card) = &mut topic.card
        {
            card.size = Vec2::new(155.0, height);
            topic.position = Some(position);
        }
    }
    Ok(())
}

/// Reconcile destructive edits before ordinary connector detachment. A semantic
/// flow cannot become a free point: deleting a node removes its incident links.
pub fn reconcile(before: &Sheet, after: &mut Sheet) -> Result<(), String> {
    use crate::board::BoardMember;
    if !has_sankey(before) && !has_sankey(after) {
        return Ok(());
    }
    for object in &before.canvas.objects {
        if let CanvasObjectKind::Connector(c) = &object.kind
            && c.sankey.is_some()
            && (after.canvas.object(object.id).is_none() ||
                endpoints(c).is_ok_and(|(a,b)| !after.contains(a) || !after.contains(b)))
            && (before.canvas.is_locked(object.id) || [c.from,c.to].iter().any(|e|
                matches!(e,Endpoint::Topic(id) if before.is_board_member_locked(BoardMember::Card(*id)))))
        {
            return Err("Unlock the link and both endpoints before removing a Sankey link.".into());
        }
    }
    let node_settings: BTreeSet<_> = before
        .sankey
        .iter()
        .chain(after.sankey.iter())
        .flat_map(|s| s.sources.keys().chain(s.stages.keys()))
        .copied()
        .collect();
    for id in &node_settings {
        let old_source = before.sankey.as_ref().and_then(|s| s.sources.get(id));
        let old_stage = before.sankey.as_ref().and_then(|s| s.stages.get(id));
        let source = after.sankey.as_ref().and_then(|s| s.sources.get(id));
        let stage = after.sankey.as_ref().and_then(|s| s.stages.get(id));
        if (source != old_source || stage != old_stage)
            && before.is_board_member_locked(BoardMember::Card(*id))
        {
            return Err("Unlock the node before editing its source mass or stage.".into());
        }
    }
    for object in &after.canvas.objects {
        let CanvasObjectKind::Connector(c) = &object.kind else {
            continue;
        };
        let old_object = before.canvas.object(object.id);
        let old = old_object.and_then(|o| match &o.kind {
            CanvasObjectKind::Connector(c) => Some(c),
            _ => None,
        });
        let mut changed = old_object != Some(object);
        if changed
            && let Some(previous) = old_object
            && previous.locked != object.locked
        {
            // Permission toggles must remain possible, including unlocking a
            // protected link. They must not carry a simultaneous content edit.
            let mut lock_only = previous.clone();
            lock_only.locked = object.locked;
            changed = lock_only != *object;
        }
        if (c.sankey.is_some() || old.is_some_and(|c| c.sankey.is_some())) && changed {
            for connection in old.into_iter().chain(std::iter::once(c)) {
                for endpoint in [connection.from, connection.to] {
                    if let Endpoint::Topic(id) = endpoint
                        && before.contains(id)
                        && before.is_board_member_locked(BoardMember::Card(id))
                    {
                        return Err(
                            "Unlock both endpoint topics before editing a Sankey link.".into()
                        );
                    }
                }
            }
            if before.canvas.is_locked(object.id) {
                return Err("Unlock the Sankey link before editing it.".into());
            }
        }
    }
    let existing: BTreeSet<_> = after.topics.keys().copied().collect();
    after.canvas.objects.retain(|o| match &o.kind {
        CanvasObjectKind::Connector(c) if c.sankey.is_some() => {
            !endpoints(c).is_ok_and(|(a, b)| !existing.contains(&a) || !existing.contains(&b))
        }
        _ => true,
    });
    if let Some(settings) = &mut after.sankey {
        settings.sources.retain(|id, _| existing.contains(id));
        settings.stages.retain(|id, _| existing.contains(id));
        if settings.mode == SankeyMode::Probability {
            // Removing the final supply to an existing branch leaves zero mass,
            // not a new implicit 100% source. Record that zero explicitly.
            let mut incoming = BTreeSet::new();
            let mut outgoing = BTreeSet::new();
            for o in &after.canvas.objects {
                if let CanvasObjectKind::Connector(c) = &o.kind
                    && c.sankey.is_some()
                    && let Ok((a, b)) = endpoints(c)
                {
                    incoming.insert(b);
                    outgoing.insert(a);
                }
            }
            for id in outgoing.difference(&incoming) {
                let lost_supply = before.canvas.objects.iter().any(|o| {
                    matches!(&o.kind, CanvasObjectKind::Connector(c) if c.sankey.is_some()
                        && c.to == Endpoint::Topic(*id))
                });
                if lost_supply {
                    settings.sources.entry(*id).or_insert(0);
                }
            }
        }
    }
    evaluate(after).map(|_| ()).map_err(|error| {
        let probability = after
            .sankey
            .as_ref()
            .is_some_and(|s| s.mode == SankeyMode::Probability);
        for object in &after.canvas.objects {
            let CanvasObjectKind::Connector(c) = &object.kind else {
                continue;
            };
            let Some(Flow {
                weight: Some(weight),
            }) = c.sankey
            else {
                continue;
            };
            let old = before.canvas.object(object.id).and_then(|o| match &o.kind {
                CanvasObjectKind::Connector(c) => c.sankey,
                _ => None,
            });
            if old == c.sankey {
                continue;
            }
            let Ok((from, to)) = endpoints(c) else {
                continue;
            };
            for (id, parents) in [(from, false), (to, true)] {
                if probability && parents {
                    continue;
                }
                let other: u64 = after
                    .canvas
                    .objects
                    .iter()
                    .filter(|o| o.id != object.id)
                    .filter_map(|o| match &o.kind {
                        CanvasObjectKind::Connector(c)
                            if if parents {
                                c.to == Endpoint::Topic(id)
                            } else {
                                c.from == Endpoint::Topic(id)
                            } =>
                        {
                            c.sankey.and_then(|f| f.weight).map(u64::from)
                        }
                        _ => None,
                    })
                    .sum();
                let remaining = u64::from(FULL_WEIGHT).saturating_sub(other);
                if u64::from(weight) > remaining {
                    return format!(
                        "{} — {}: allowed remainder for this link is {:.3}%. {error}",
                        node_name(after, id),
                        if parents { "Parents" } else { "Children" },
                        remaining as f64 / 1000.0
                    );
                }
            }
        }
        error
    })
}

/// Shared sampled ribbon outline for painting, hit testing, bounds and export.
pub fn ribbons(
    sheet: &Sheet,
    layout: &crate::layout::Layout,
) -> BTreeMap<CanvasObjectId, Vec<Vec2>> {
    let Ok(values) = evaluate(sheet) else {
        return BTreeMap::new();
    };
    ribbons_with_values(sheet, layout, &values)
}

pub(crate) fn ribbons_with_values(
    sheet: &Sheet,
    layout: &crate::layout::Layout,
    values: &FlowValues,
) -> BTreeMap<CanvasObjectId, Vec<Vec2>> {
    let Some(_settings) = &sheet.sankey else {
        return BTreeMap::new();
    };
    let scale = effective_scale(sheet, layout, values);
    let mut input = BTreeMap::<TopicId, f32>::new();
    let mut output = BTreeMap::<TopicId, f32>::new();
    let mut result = BTreeMap::new();
    let top = layout
        .topics
        .values()
        .map(|t| t.rect.min.y)
        .reduce(f32::min)
        .unwrap_or(0.0);
    let mut feedback = 0usize;
    for o in &sheet.canvas.objects {
        let CanvasObjectKind::Connector(c) = &o.kind else {
            continue;
        };
        let Some(flow) = c.sankey else {
            continue;
        };
        // Unset flows use a visible dashed ordinary connector, not a band.
        if flow.weight.is_none() {
            continue;
        }
        result.insert(o.id, Vec::new());
        let Ok((from, to)) = endpoints(c) else {
            continue;
        };
        let (Some(a), Some(b)) = (layout.rect(from), layout.rect(to)) else {
            continue;
        };
        let mass = values.mass.get(&o.id).copied().unwrap_or(0.0);
        if mass == 0.0 {
            continue;
        }
        let width = mass as f32 * scale;
        let out = output.entry(from).or_default();
        let inc = input.entry(to).or_default();
        let start = Vec2::new(a.max().x, a.min.y + *out);
        let end = Vec2::new(b.min.x, b.min.y + *inc);
        *out += width;
        *inc += width;
        let mut upper = Vec::new();
        let mut lower = Vec::new();
        if end.x > start.x + 10.0 {
            let dx = (end.x - start.x) * 0.5;
            for i in 0..=32 {
                let t = i as f32 / 32.0;
                let p = bezier(
                    start,
                    start + Vec2::new(dx, 0.0),
                    end - Vec2::new(dx, 0.0),
                    end,
                    t,
                );
                upper.push(p);
                lower.push(p + Vec2::new(0.0, width));
            }
        } else {
            // Dedicated outside lane for feedback/self loops, never recursive.
            let lane = top - 50.0 - feedback as f32 * (100.0 * scale + 24.0);
            feedback += 1;
            let dx = 40.0 + width;
            let points = [
                start,
                Vec2::new(start.x + dx, start.y),
                Vec2::new(start.x + dx, lane),
                Vec2::new(end.x - dx, lane),
                Vec2::new(end.x - dx, end.y),
                end,
            ];
            upper.push(start);
            for i in 1..points.len() - 1 {
                let a = (points[i - 1] + points[i]) * 0.5;
                let b = (points[i] + points[i + 1]) * 0.5;
                for j in 0..=12 {
                    let t = j as f32 / 12.0;
                    upper.push(
                        a * (1.0 - t).powi(2) + points[i] * (2.0 * (1.0 - t) * t) + b * t * t,
                    );
                }
            }
            upper.push(end);
            let centerline: Vec<_> = upper
                .iter()
                .map(|p| *p + Vec2::new(0.0, width / 2.0))
                .collect();
            upper.clear();
            for (i, p) in centerline.iter().enumerate() {
                let prev = centerline[i.saturating_sub(1)];
                let next = centerline[(i + 1).min(centerline.len() - 1)];
                let tangent = next - prev;
                let length = tangent.length().max(0.001);
                let normal = Vec2::new(-tangent.y, tangent.x) * (width / (2.0 * length));
                upper.push(*p - normal);
                lower.push(*p + normal);
            }
        }
        lower.reverse();
        upper.extend(lower);
        result.insert(o.id, upper);
    }
    result
}

/// One effective linear scale, also used by approximate DrawIO strokes.
pub fn effective_scale(sheet: &Sheet, layout: &crate::layout::Layout, values: &FlowValues) -> f32 {
    let mut scale = sheet.sankey.as_ref().map_or(2.0, |s| s.pixels_per_unit);
    for &id in &values.nodes {
        if let Some(rect) = layout.rect(id) {
            let total = values
                .incoming
                .get(&id)
                .copied()
                .unwrap_or(0.0)
                .max(values.outgoing.get(&id).copied().unwrap_or(0.0));
            if total > 0.0 {
                scale = scale.min(rect.size.y / total as f32);
            }
        }
    }
    scale
}

/// Midpoint of matching ribbon boundaries, including an outside feedback lane.
/// A bounding-box centre can lie in the empty interior of a circular flow.
pub fn ribbon_label_anchor(outline: &[Vec2]) -> Option<Vec2> {
    let i = outline.len() / 4;
    Some((*outline.get(i)? + *outline.get(outline.len().checked_sub(i + 1)?)?) * 0.5)
}

pub fn flow_label(connector: &crate::board::CanvasConnector, mass: f64) -> String {
    let value = if connector.sankey.is_some_and(|f| f.weight.is_none()) {
        "—%".to_owned()
    } else {
        format!("{mass:.3}%")
    };
    if connector.label.is_empty() {
        value
    } else {
        format!("{} · {value}", connector.label)
    }
}

/// Conservative sheet-coordinate bounds shared with hit/lasso/export crop.
/// 16 characters per line is deliberately more conservative than either painter.
pub fn label_bounds(anchor: Vec2, label: &str) -> crate::Rect {
    let lines: usize = label
        .lines()
        .map(|line| line.chars().count().div_ceil(16).max(1))
        .sum();
    crate::Rect::from_center_size(
        anchor,
        Vec2::new(232.0, (lines.max(1) as f32 * 18.0 + 8.0).max(26.0)),
    )
}

pub fn dash_segments(points: &[Vec2]) -> Vec<Vec<Vec2>> {
    let mut result = Vec::new();
    let mut phase = 0.0f32;
    let period = (points
        .windows(2)
        .map(|p| (p[1] - p[0]).length())
        .sum::<f32>()
        / 10_000.0)
        .max(10.0);
    let painted = period * 0.6;
    for pair in points.windows(2) {
        let delta = pair[1] - pair[0];
        let length = delta.length();
        if length <= 0.0 {
            continue;
        }
        let mut at = 0.0;
        while at < length {
            let on = phase < painted;
            let end = (at + if on { painted - phase } else { period - phase }).min(length);
            if end <= at {
                phase = if on { painted } else { 0.0 };
                continue;
            }
            if on {
                result.push(vec![
                    pair[0] + delta * (at / length),
                    pair[0] + delta * (end / length),
                ]);
            }
            phase = (phase + end - at) % period;
            at = end;
        }
    }
    result
}

fn bezier(a: Vec2, b: Vec2, c: Vec2, d: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    a * u.powi(3) + b * (3.0 * u * u * t) + c * (3.0 * u * t * t) + d * t.powi(3)
}
