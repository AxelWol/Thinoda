//! Deterministic automatic layouts for a small graph of rectangles.
//!
//! The algorithms place node centres only; callers own the document edit.
//! Every result keeps the previous top-left corner of the nodes' bounding box,
//! and a final separation pass guarantees that no two node rectangles, grown
//! by half the spacing, overlap.

use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};

use crate::{Rect, Vec2};

/// Largest number of nodes a single layout accepts.
pub const MAX_LAYOUT_NODES: usize = 500;
/// Largest number of edges a single layout accepts.
pub const MAX_LAYOUT_EDGES: usize = 5_000;
/// Default gap between neighbouring nodes in sheet units.
pub const DEFAULT_LAYOUT_SPACING: f32 = 40.0;
const MAX_SPACING: f32 = 10_000.0;
const MAX_ORGANIC_WORK: usize = 5_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphLayout {
    /// Parents above their children.
    TreeDown,
    /// Parents left of their children.
    TreeRight,
    /// Layered flow top to bottom; cycles are broken only for placement.
    FlowDown,
    /// Layered flow left to right.
    FlowRight,
    /// Rings of increasing depth around the best-connected node.
    Radial,
    /// All nodes on one circle in their current angular order.
    Circle,
    /// Force-directed placement starting from the current positions.
    Organic,
}

impl GraphLayout {
    pub const ALL: [Self; 7] = [
        Self::TreeDown,
        Self::TreeRight,
        Self::FlowDown,
        Self::FlowRight,
        Self::Radial,
        Self::Circle,
        Self::Organic,
    ];
}

/// Returns the new centre of every node, in input order.
///
/// `edges` are directed `(from, to)` node indices; self-loops and duplicates
/// are ignored. Tree and flow layouts follow edge direction, the others treat
/// edges as undirected.
pub fn layout_graph(
    kind: GraphLayout,
    nodes: &[Rect],
    edges: &[(usize, usize)],
    spacing: f32,
) -> Result<Vec<Vec2>, String> {
    if nodes.len() > MAX_LAYOUT_NODES {
        return Err(format!("layouts support at most {MAX_LAYOUT_NODES} items"));
    }
    if edges.len() > MAX_LAYOUT_EDGES {
        return Err(format!(
            "layouts support at most {MAX_LAYOUT_EDGES} connections"
        ));
    }
    if !(spacing.is_finite() && (0.0..=MAX_SPACING).contains(&spacing)) {
        return Err("layout spacing is out of range".into());
    }
    for rect in nodes {
        let finite = [rect.min.x, rect.min.y, rect.size.x, rect.size.y]
            .into_iter()
            .all(f32::is_finite);
        if !finite || rect.size.x < 0.0 || rect.size.y < 0.0 {
            return Err("layout item bounds are invalid".into());
        }
    }
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    let graph = Graph::new(nodes, edges)?;
    let mut centers = match kind {
        GraphLayout::TreeDown => tree(&graph, nodes, spacing, false),
        GraphLayout::TreeRight => tree(&graph, nodes, spacing, true),
        GraphLayout::FlowDown => flow(&graph, nodes, spacing, false),
        GraphLayout::FlowRight => flow(&graph, nodes, spacing, true),
        GraphLayout::Radial => radial(&graph, nodes, spacing),
        GraphLayout::Circle => circle(nodes, spacing),
        GraphLayout::Organic => organic(&graph, nodes, spacing),
    };
    separate(&mut centers, nodes, spacing);
    anchor(&mut centers, nodes);
    if centers.iter().any(|c| !c.x.is_finite() || !c.y.is_finite()) {
        return Err("layout produced invalid positions".into());
    }
    Ok(centers)
}

struct Graph {
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
    undirected: Vec<Vec<usize>>,
    /// Node indices in reading order: top to bottom, then left to right.
    reading: Vec<usize>,
    rank: Vec<usize>,
}

impl Graph {
    fn new(nodes: &[Rect], edges: &[(usize, usize)]) -> Result<Self, String> {
        let n = nodes.len();
        let mut reading: Vec<_> = (0..n).collect();
        reading.sort_by(|&a, &b| {
            let (ca, cb) = (nodes[a].center(), nodes[b].center());
            ca.y.total_cmp(&cb.y)
                .then(ca.x.total_cmp(&cb.x))
                .then(a.cmp(&b))
        });
        let mut rank = vec![0; n];
        for (position, &node) in reading.iter().enumerate() {
            rank[node] = position;
        }
        let mut outgoing = vec![Vec::new(); n];
        let mut incoming = vec![Vec::new(); n];
        let mut undirected = vec![Vec::new(); n];
        for &(from, to) in edges {
            if from >= n || to >= n {
                return Err("layout connection refers to a missing item".into());
            }
            if from == to {
                continue;
            }
            outgoing[from].push(to);
            incoming[to].push(from);
            undirected[from].push(to);
            undirected[to].push(from);
        }
        for list in outgoing
            .iter_mut()
            .chain(incoming.iter_mut())
            .chain(undirected.iter_mut())
        {
            list.sort_by_key(|&node| rank[node]);
            list.dedup();
        }
        Ok(Self {
            outgoing,
            incoming,
            undirected,
            reading,
            rank,
        })
    }

    fn len(&self) -> usize {
        self.rank.len()
    }
}

/// Size along the layout's depth and cross axes.
fn extents(rect: &Rect, horizontal: bool) -> (f32, f32) {
    if horizontal {
        (rect.size.x, rect.size.y)
    } else {
        (rect.size.y, rect.size.x)
    }
}

fn axis_point(depth: f32, cross: f32, horizontal: bool) -> Vec2 {
    if horizontal {
        Vec2::new(depth, cross)
    } else {
        Vec2::new(cross, depth)
    }
}

/// Spanning forest: in-degree-free roots first, then any still unplaced node.
fn spanning_forest(graph: &Graph, undirected: bool) -> (Vec<usize>, Vec<Vec<usize>>, Vec<usize>) {
    let n = graph.len();
    let mut placed = vec![false; n];
    let mut children = vec![Vec::new(); n];
    let mut depth = vec![0; n];
    let mut roots = Vec::new();
    let candidates: Vec<usize> = if undirected {
        let mut order = graph.reading.clone();
        // Stable sort keeps reading order among equally connected nodes.
        order.sort_by_key(|&node| std::cmp::Reverse(graph.undirected[node].len()));
        order
    } else {
        graph
            .reading
            .iter()
            .copied()
            .filter(|&node| graph.incoming[node].is_empty())
            .chain(graph.reading.iter().copied())
            .collect()
    };
    for root in candidates {
        if placed[root] {
            continue;
        }
        placed[root] = true;
        roots.push(root);
        let mut queue = VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            let next = if undirected {
                &graph.undirected[node]
            } else {
                &graph.outgoing[node]
            };
            for &child in next {
                if !placed[child] {
                    placed[child] = true;
                    depth[child] = depth[node] + 1;
                    children[node].push(child);
                    queue.push_back(child);
                }
            }
        }
    }
    (roots, children, depth)
}

/// Post-order traversal of a forest without recursion.
fn post_order(roots: &[usize], children: &[Vec<usize>]) -> Vec<usize> {
    let mut order = Vec::with_capacity(children.len());
    let mut stack: Vec<(usize, bool)> = roots.iter().rev().map(|&r| (r, false)).collect();
    while let Some((node, expanded)) = stack.pop() {
        if expanded {
            order.push(node);
        } else {
            stack.push((node, true));
            stack.extend(children[node].iter().rev().map(|&c| (c, false)));
        }
    }
    order
}

fn layer_offsets(layer_of: &[usize], nodes: &[Rect], spacing: f32, horizontal: bool) -> Vec<f32> {
    let layers = layer_of.iter().max().map_or(0, |max| max + 1);
    let mut thickness = vec![0.0f32; layers];
    for (node, &layer) in layer_of.iter().enumerate() {
        thickness[layer] = thickness[layer].max(extents(&nodes[node], horizontal).0);
    }
    let gap = spacing * 1.5;
    let mut centers = Vec::with_capacity(layers);
    let mut offset = 0.0;
    for thickness in thickness {
        centers.push(offset + thickness / 2.0);
        offset += thickness + gap;
    }
    centers
}

fn tree(graph: &Graph, nodes: &[Rect], spacing: f32, horizontal: bool) -> Vec<Vec2> {
    let (roots, children, depth) = spanning_forest(graph, false);
    let cross = |node: usize| extents(&nodes[node], horizontal).1;
    let mut width = vec![0.0f32; graph.len()];
    for node in post_order(&roots, &children) {
        let span: f32 = children[node].iter().map(|&c| width[c]).sum::<f32>()
            + spacing * children[node].len().saturating_sub(1) as f32;
        width[node] = cross(node).max(span);
    }
    let layer_centers = layer_offsets(&depth, nodes, spacing, horizontal);
    let mut centers = vec![Vec2::ZERO; graph.len()];
    let mut start = 0.0;
    let mut stack = Vec::new();
    for &root in &roots {
        stack.push((root, start));
        start += width[root] + spacing;
    }
    while let Some((node, left)) = stack.pop() {
        centers[node] = axis_point(
            layer_centers[depth[node]],
            left + width[node] / 2.0,
            horizontal,
        );
        let span: f32 = children[node].iter().map(|&c| width[c]).sum::<f32>()
            + spacing * children[node].len().saturating_sub(1) as f32;
        let mut child_left = left + (width[node] - span) / 2.0;
        for &child in &children[node] {
            stack.push((child, child_left));
            child_left += width[child] + spacing;
        }
    }
    centers
}

fn flow(graph: &Graph, nodes: &[Rect], spacing: f32, horizontal: bool) -> Vec<Vec2> {
    let n = graph.len();
    // Break cycles by ignoring DFS back edges; the document is never changed.
    let mut state = vec![0u8; n];
    let mut forward: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &start in &graph.reading {
        if state[start] != 0 {
            continue;
        }
        state[start] = 1;
        let mut stack = vec![(start, 0usize)];
        while let Some((node, next)) = stack.last_mut() {
            let node = *node;
            if let Some(&child) = graph.outgoing[node].get(*next) {
                *next += 1;
                match state[child] {
                    0 => {
                        forward[node].push(child);
                        state[child] = 1;
                        stack.push((child, 0));
                    }
                    2 => forward[node].push(child),
                    _ => {}
                }
            } else {
                state[node] = 2;
                stack.pop();
            }
        }
    }
    let mut indegree = vec![0usize; n];
    for targets in &forward {
        for &target in targets {
            indegree[target] += 1;
        }
    }
    let mut layer = vec![0usize; n];
    let mut queue: VecDeque<_> = graph
        .reading
        .iter()
        .copied()
        .filter(|&node| indegree[node] == 0)
        .collect();
    while let Some(node) = queue.pop_front() {
        for &target in &forward[node] {
            layer[target] = layer[target].max(layer[node] + 1);
            indegree[target] -= 1;
            if indegree[target] == 0 {
                queue.push_back(target);
            }
        }
    }
    let layers = layer.iter().max().map_or(0, |max| max + 1);
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); layers];
    let mut by_cross = graph.reading.clone();
    by_cross.sort_by(|&a, &b| {
        let (ca, cb) = (nodes[a].center(), nodes[b].center());
        let (ka, kb) = if horizontal {
            (ca.y, cb.y)
        } else {
            (ca.x, cb.x)
        };
        ka.total_cmp(&kb).then(graph.rank[a].cmp(&graph.rank[b]))
    });
    for node in by_cross {
        rows[layer[node]].push(node);
    }
    let mut position = vec![0.0f32; n];
    let index_rows = |rows: &[Vec<usize>], position: &mut [f32]| {
        for row in rows {
            for (index, &node) in row.iter().enumerate() {
                position[node] = index as f32;
            }
        }
    };
    index_rows(&rows, &mut position);
    for sweep in 0..4 {
        let downward = sweep % 2 == 0;
        let order: Vec<usize> = if downward {
            (1..layers).collect()
        } else {
            (0..layers.saturating_sub(1)).rev().collect()
        };
        for row in order {
            let neighbours = |node: usize| {
                if downward {
                    &graph.incoming[node]
                } else {
                    &graph.outgoing[node]
                }
            };
            let reference = if downward { row - 1 } else { row + 1 };
            let key: Vec<(usize, f32)> = rows[row]
                .iter()
                .map(|&node| {
                    let adjacent: Vec<f32> = neighbours(node)
                        .iter()
                        .filter(|&&other| layer[other] == reference)
                        .map(|&other| position[other])
                        .collect();
                    let barycenter = if adjacent.is_empty() {
                        position[node]
                    } else {
                        adjacent.iter().sum::<f32>() / adjacent.len() as f32
                    };
                    (node, barycenter)
                })
                .collect();
            let mut sorted = key;
            sorted.sort_by(|a, b| {
                a.1.total_cmp(&b.1)
                    .then(graph.rank[a.0].cmp(&graph.rank[b.0]))
            });
            rows[row] = sorted.into_iter().map(|(node, _)| node).collect();
            for (index, &node) in rows[row].iter().enumerate() {
                position[node] = index as f32;
            }
        }
    }
    let layer_centers = layer_offsets(&layer, nodes, spacing, horizontal);
    let mut centers = vec![Vec2::ZERO; n];
    for (row_index, row) in rows.iter().enumerate() {
        let total: f32 = row
            .iter()
            .map(|&node| extents(&nodes[node], horizontal).1)
            .sum::<f32>()
            + spacing * row.len().saturating_sub(1) as f32;
        let mut cursor = -total / 2.0;
        for &node in row {
            let size = extents(&nodes[node], horizontal).1;
            centers[node] = axis_point(layer_centers[row_index], cursor + size / 2.0, horizontal);
            cursor += size + spacing;
        }
    }
    centers
}

fn radius(rect: &Rect) -> f32 {
    rect.size.length() / 2.0
}

fn radial(graph: &Graph, nodes: &[Rect], spacing: f32) -> Vec<Vec2> {
    let (roots, children, depth) = spanning_forest(graph, true);
    let n = graph.len();
    let mut leaves = vec![0usize; n];
    for node in post_order(&roots, &children) {
        leaves[node] = children[node]
            .iter()
            .map(|&c| leaves[c])
            .sum::<usize>()
            .max(1);
    }
    let mut centers = vec![Vec2::ZERO; n];
    let mut cursor = 0.0f32;
    for &root in &roots {
        // Angular wedge of each node in this component.
        let mut wedge = vec![(0.0f32, 0.0f32); n];
        wedge[root] = (0.0, TAU);
        let mut members = vec![root];
        let mut index = 0;
        while index < members.len() {
            let node = members[index];
            index += 1;
            let (start, size) = wedge[node];
            let mut offset = start;
            for &child in &children[node] {
                let share = size * leaves[child] as f32 / leaves[node] as f32;
                wedge[child] = (offset, share);
                offset += share;
                members.push(child);
            }
        }
        let rings = members.iter().map(|&m| depth[m]).max().unwrap_or(0) + 1;
        let mut ring_radius = vec![0.0f32; rings];
        for &node in &members {
            ring_radius[depth[node]] = ring_radius[depth[node]].max(radius(&nodes[node]));
        }
        let mut ring = vec![0.0f32; rings];
        for d in 1..rings {
            let mut needed = ring[d - 1] + ring_radius[d - 1] + ring_radius[d] + spacing;
            for &node in members.iter().filter(|&&m| depth[m] == d) {
                let angle = wedge[node].1.min(PI);
                let half = (angle / 2.0).sin().max(1e-3);
                needed = needed.max((2.0 * ring_radius[d] + spacing) / (2.0 * half));
            }
            ring[d] = needed;
        }
        let extent = ring[rings - 1] + ring_radius[rings - 1];
        let origin = Vec2::new(cursor + extent, extent);
        for &node in &members {
            let (start, size) = wedge[node];
            // Start at the top and proceed clockwise on screen.
            let angle = start + size / 2.0 - PI / 2.0;
            let r = ring[depth[node]];
            centers[node] = origin + Vec2::new(angle.cos() * r, angle.sin() * r);
        }
        cursor += 2.0 * extent + spacing;
    }
    centers
}

fn circle(nodes: &[Rect], spacing: f32) -> Vec<Vec2> {
    let n = nodes.len();
    if n == 1 {
        return vec![nodes[0].center()];
    }
    let centroid = nodes
        .iter()
        .fold(Vec2::ZERO, |sum, rect| sum + rect.center())
        * (1.0 / n as f32);
    let angle = |index: usize| {
        let d = nodes[index].center() - centroid;
        if d.length() < 1e-3 {
            0.0
        } else {
            // Angle measured clockwise from the top, in [0, TAU).
            (d.y.atan2(d.x) + PI / 2.0).rem_euclid(TAU)
        }
    };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| angle(a).total_cmp(&angle(b)).then(a.cmp(&b)));
    let half = (PI / n as f32).sin();
    let mut r = 0.0f32;
    for (index, &node) in order.iter().enumerate() {
        let next = order[(index + 1) % n];
        r = r.max((radius(&nodes[node]) + radius(&nodes[next]) + spacing) / (2.0 * half));
    }
    let mut centers = vec![Vec2::ZERO; n];
    for (index, &node) in order.iter().enumerate() {
        let a = TAU * index as f32 / n as f32 - PI / 2.0;
        centers[node] = Vec2::new(a.cos() * r, a.sin() * r);
    }
    centers
}

/// A deterministic unit direction for a pair of nodes at the same position.
fn tie_direction(a: usize, b: usize) -> Vec2 {
    let angle = (a as f32 * 0.618_034 + b as f32 * 0.381_966).fract() * TAU;
    Vec2::new(angle.cos(), angle.sin())
}

fn organic(graph: &Graph, nodes: &[Rect], spacing: f32) -> Vec<Vec2> {
    let n = graph.len();
    let mut centers: Vec<Vec2> = nodes.iter().map(Rect::center).collect();
    if n < 2 {
        return centers;
    }
    let ideal = nodes.iter().map(|rect| 2.0 * radius(rect)).sum::<f32>() / n as f32 + spacing;
    let ideal = ideal.max(1.0);
    let iterations = (MAX_ORGANIC_WORK / (n * n)).clamp(10, 150);
    let mut temperature = ideal * (n as f32).sqrt();
    let cooling = temperature / iterations as f32;
    let mut forces = vec![Vec2::ZERO; n];
    for _ in 0..iterations {
        forces.fill(Vec2::ZERO);
        for a in 0..n {
            for b in a + 1..n {
                let mut delta = centers[a] - centers[b];
                let mut distance = delta.length();
                if distance < 1e-3 {
                    delta = tie_direction(a, b);
                    distance = 1e-3;
                }
                let direction = delta * (1.0 / distance);
                let push = direction * (ideal * ideal / distance);
                forces[a] = forces[a] + push;
                forces[b] = forces[b] - push;
            }
        }
        for a in 0..n {
            for &b in &graph.undirected[a] {
                if b <= a {
                    continue;
                }
                let delta = centers[a] - centers[b];
                let distance = delta.length();
                if distance < 1e-3 {
                    continue;
                }
                let pull = delta * (distance / ideal);
                forces[a] = forces[a] - pull;
                forces[b] = forces[b] + pull;
            }
        }
        for (center, force) in centers.iter_mut().zip(&forces) {
            let length = force.length();
            if length > 1e-6 {
                *center = *center + *force * (length.min(temperature) / length);
            }
        }
        temperature = (temperature - cooling).max(ideal * 0.05);
    }
    centers
}

/// Scales positions about their centroid until no two rectangles, each grown
/// by half the spacing, overlap. Uniform scaling preserves the arrangement.
fn separate(centers: &mut [Vec2], nodes: &[Rect], spacing: f32) {
    let n = centers.len();
    if n < 2 {
        return;
    }
    // Separate coincident positions first; scaling cannot.
    for a in 0..n {
        for b in a + 1..n {
            if (centers[a] - centers[b]).length() < 1e-3 {
                let need = radius(&nodes[a]) + radius(&nodes[b]) + spacing;
                centers[b] = centers[a] + tie_direction(a, b) * need.max(1.0);
            }
        }
    }
    let centroid = centers.iter().fold(Vec2::ZERO, |sum, c| sum + *c) * (1.0 / n as f32);
    let mut factor = 1.0f32;
    for a in 0..n {
        for b in a + 1..n {
            let dx = (centers[a].x - centers[b].x).abs();
            let dy = (centers[a].y - centers[b].y).abs();
            let need_x = (nodes[a].size.x + nodes[b].size.x) / 2.0 + spacing;
            let need_y = (nodes[a].size.y + nodes[b].size.y) / 2.0 + spacing;
            if dx >= need_x || dy >= need_y {
                continue;
            }
            let scale_x = if dx > 0.0 { need_x / dx } else { f32::INFINITY };
            let scale_y = if dy > 0.0 { need_y / dy } else { f32::INFINITY };
            factor = factor.max(scale_x.min(scale_y));
        }
    }
    if factor > 1.0 {
        let factor = factor * 1.000_1;
        for center in centers.iter_mut() {
            *center = centroid + (*center - centroid) * factor;
        }
    }
}

fn bounds(centers: &[Vec2], nodes: &[Rect]) -> Rect {
    centers
        .iter()
        .zip(nodes)
        .map(|(center, rect)| Rect::from_center_size(*center, rect.size))
        .reduce(|a, b| a.union(&b))
        .unwrap_or_default()
}

fn anchor(centers: &mut [Vec2], nodes: &[Rect]) {
    let before = nodes
        .iter()
        .copied()
        .reduce(|a, b| a.union(&b))
        .unwrap_or_default();
    let shift = before.min - bounds(centers, nodes).min;
    for center in centers {
        *center = *center + shift;
    }
}

#[cfg(test)]
mod tests;
