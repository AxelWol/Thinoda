use super::*;

fn boxes(sizes: &[(f32, f32)]) -> Vec<Rect> {
    sizes
        .iter()
        .enumerate()
        .map(|(index, &(w, h))| {
            Rect::from_min_size(
                Vec2::new(index as f32 * 13.0, (index % 3) as f32 * 7.0),
                Vec2::new(w, h),
            )
        })
        .collect()
}

fn placed(centers: &[Vec2], nodes: &[Rect]) -> Vec<Rect> {
    centers
        .iter()
        .zip(nodes)
        .map(|(center, rect)| Rect::from_center_size(*center, rect.size))
        .collect()
}

fn assert_separated(rects: &[Rect], spacing: f32) {
    for (a, first) in rects.iter().enumerate() {
        for (b, second) in rects.iter().enumerate().skip(a + 1) {
            let grown = first.expand(spacing / 2.0 - 0.01);
            let other = second.expand(spacing / 2.0 - 0.01);
            assert!(
                !grown.intersects(&other),
                "{a} and {b} overlap: {first:?} {second:?}"
            );
        }
    }
}

fn assert_anchored(rects: &[Rect], nodes: &[Rect]) {
    let union = |rects: &[Rect]| rects.iter().copied().reduce(|a, b| a.union(&b)).unwrap();
    let (before, after) = (union(nodes), union(rects));
    assert!(
        (before.min - after.min).length() < 0.01,
        "{before:?} {after:?}"
    );
}

const SIZES: [(f32, f32); 8] = [
    (120.0, 60.0),
    (40.0, 40.0),
    (200.0, 30.0),
    (60.0, 140.0),
    (80.0, 80.0),
    (30.0, 30.0),
    (150.0, 90.0),
    (70.0, 20.0),
];
const EDGES: [(usize, usize); 8] = [
    (0, 1),
    (0, 2),
    (1, 3),
    (1, 4),
    (2, 5),
    (4, 6),
    (6, 0),
    (3, 7),
];

#[test]
fn every_layout_separates_unequal_items_and_keeps_the_top_left() {
    let nodes = boxes(&SIZES);
    for kind in GraphLayout::ALL {
        let centers = layout_graph(kind, &nodes, &EDGES, 30.0).unwrap();
        assert_eq!(centers.len(), nodes.len());
        let rects = placed(&centers, &nodes);
        assert_separated(&rects, 30.0);
        assert_anchored(&rects, &nodes);
        assert_eq!(
            layout_graph(kind, &nodes, &EDGES, 30.0).unwrap(),
            centers,
            "{kind:?}"
        );
    }
}

#[test]
fn trees_place_children_after_parents_and_centre_parents() {
    let nodes = boxes(&[(60.0, 30.0); 4]);
    let edges = [(0, 1), (0, 2), (2, 3)];
    let down = layout_graph(GraphLayout::TreeDown, &nodes, &edges, 20.0).unwrap();
    for (parent, child) in edges {
        assert!(down[child].y > down[parent].y);
    }
    assert!((down[0].x - (down[1].x + down[2].x) / 2.0).abs() < 0.01);
    let right = layout_graph(GraphLayout::TreeRight, &nodes, &edges, 20.0).unwrap();
    for (parent, child) in edges {
        assert!(right[child].x > right[parent].x);
    }
}

#[test]
fn rootless_cycles_and_unreachable_items_are_still_placed() {
    // 0 -> 1 -> 2 -> 0 with 3 -> 1 upstream and 4 isolated.
    let nodes = boxes(&[(50.0, 50.0); 5]);
    let edges = [(0, 1), (1, 2), (2, 0), (3, 1)];
    for kind in [
        GraphLayout::TreeDown,
        GraphLayout::FlowDown,
        GraphLayout::Radial,
    ] {
        let centers = layout_graph(kind, &nodes, &edges, 10.0).unwrap();
        assert_separated(&placed(&centers, &nodes), 10.0);
    }
}

#[test]
fn flow_layers_follow_acyclic_edge_direction() {
    let nodes = boxes(&[(40.0, 40.0); 5]);
    let edges = [(0, 1), (0, 2), (1, 3), (2, 3), (3, 4), (0, 4)];
    let down = layout_graph(GraphLayout::FlowDown, &nodes, &edges, 20.0).unwrap();
    for (from, to) in edges {
        assert!(down[to].y > down[from].y, "{from} -> {to}");
    }
    let right = layout_graph(GraphLayout::FlowRight, &nodes, &edges, 20.0).unwrap();
    for (from, to) in edges {
        assert!(right[to].x > right[from].x, "{from} -> {to}");
    }
}

#[test]
fn circle_keeps_the_current_angular_order() {
    // A square listed clockwise from the top-left corner.
    let corners = [(0.0, 0.0), (200.0, 0.0), (200.0, 200.0), (0.0, 200.0)];
    let nodes: Vec<_> = corners
        .iter()
        .map(|&(x, y)| Rect::from_min_size(Vec2::new(x, y), Vec2::new(40.0, 40.0)))
        .collect();
    let centers = layout_graph(GraphLayout::Circle, &nodes, &[], 20.0).unwrap();
    let centroid = centers.iter().fold(Vec2::ZERO, |sum, c| sum + *c) * 0.25;
    let angles: Vec<_> = centers
        .iter()
        .map(|c| (c.y - centroid.y).atan2(c.x - centroid.x))
        .collect();
    // Clockwise on screen is an increasing atan2 angle because y grows down.
    for index in 0..4 {
        let next = (index + 1) % 4;
        let step = (angles[next] - angles[index]).rem_euclid(TAU);
        assert!((step - PI / 2.0).abs() < 0.01, "{angles:?}");
    }
}

#[test]
fn coincident_items_are_separated_deterministically() {
    let nodes = vec![Rect::from_min_size(Vec2::new(5.0, 5.0), Vec2::new(30.0, 30.0)); 6];
    for kind in GraphLayout::ALL {
        let centers = layout_graph(kind, &nodes, &[(0, 1)], 10.0).unwrap();
        assert_separated(&placed(&centers, &nodes), 10.0);
        assert_eq!(
            layout_graph(kind, &nodes, &[(0, 1)], 10.0).unwrap(),
            centers
        );
    }
}

#[test]
fn radial_rings_surround_the_best_connected_item() {
    let nodes = boxes(&[(30.0, 30.0); 6]);
    let edges = [(1, 0), (1, 2), (1, 3), (1, 4), (4, 5)];
    let centers = layout_graph(GraphLayout::Radial, &nodes, &edges, 10.0).unwrap();
    let distance = |node: usize| (centers[node] - centers[1]).length();
    for leaf in [0, 2, 3, 4] {
        assert!(distance(leaf) < distance(5), "{leaf}");
    }
}

#[test]
fn invalid_and_oversized_inputs_are_rejected() {
    let nodes = boxes(&[(10.0, 10.0); 2]);
    assert!(layout_graph(GraphLayout::Circle, &nodes, &[(0, 2)], 10.0).is_err());
    assert!(layout_graph(GraphLayout::Circle, &nodes, &[], f32::NAN).is_err());
    assert!(layout_graph(GraphLayout::Circle, &nodes, &[], -1.0).is_err());
    let bad = [Rect::from_min_size(Vec2::ZERO, Vec2::new(-1.0, 1.0))];
    assert!(layout_graph(GraphLayout::Circle, &bad, &[], 10.0).is_err());
    let many = vec![Rect::from_min_size(Vec2::ZERO, Vec2::new(1.0, 1.0)); MAX_LAYOUT_NODES + 1];
    assert!(layout_graph(GraphLayout::Organic, &many, &[], 10.0).is_err());
    assert_eq!(
        layout_graph(GraphLayout::Organic, &[], &[], 10.0).unwrap(),
        vec![]
    );
}

#[test]
fn the_largest_organic_layout_stays_separated() {
    let nodes: Vec<_> = (0..MAX_LAYOUT_NODES)
        .map(|index| {
            Rect::from_min_size(
                Vec2::new((index % 25) as f32 * 3.0, (index / 25) as f32 * 3.0),
                Vec2::new(20.0 + (index % 7) as f32 * 5.0, 15.0),
            )
        })
        .collect();
    let edges: Vec<_> = (1..MAX_LAYOUT_NODES)
        .map(|index| (index / 2, index))
        .collect();
    let centers = layout_graph(GraphLayout::Organic, &nodes, &edges, 10.0).unwrap();
    assert_separated(&placed(&centers, &nodes), 10.0);
}
