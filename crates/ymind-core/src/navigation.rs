//! Keyboard navigation between topics (arrow keys).

use crate::geometry::Vec2;
use crate::layout::{Dir, Layout};
use crate::model::{Sheet, TopicId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Finds the topic to select when pressing an arrow key.
///
/// Structural moves are preferred (parent/child/sibling) when they lie in the requested
/// direction; otherwise the geometrically nearest topic in that direction is chosen.
pub fn navigate(sheet: &Sheet, layout: &Layout, from: TopicId, dir: Direction) -> Option<TopicId> {
    let cur = layout.topics.get(&from)?;
    let c = cur.rect.center();
    let along = |p: Vec2| -> Option<(f32, f32)> {
        let d = p - c;
        let (main, cross) = match dir {
            Direction::Right => (d.x, d.y),
            Direction::Left => (-d.x, d.y),
            Direction::Down => (d.y, d.x),
            Direction::Up => (-d.y, d.x),
        };
        (main > 1.0).then_some((main, cross.abs()))
    };

    // structural shortcut: into the first visible child when the subtree grows that way
    let grow_matches = matches!(
        (cur.grow, dir),
        (Dir::Right, Direction::Right)
            | (Dir::Left, Direction::Left)
            | (Dir::Down, Direction::Down)
            | (Dir::Up, Direction::Up)
    );
    if grow_matches
        && cur.hidden_children == 0
        && from != sheet.root
        && let Some(first) = sheet
            .children(from)
            .iter()
            .find(|k| layout.topics.contains_key(k))
    {
        return Some(*first);
    }
    // structural shortcut: back to the parent
    if let Some(p) = sheet.parent(from)
        && let Some(pl) = layout.topics.get(&p)
        && let Some((main, cross)) = along(pl.rect.center())
        && cross < main
        && !sheet.children(p).iter().any(|s| {
            *s != from
                && layout
                    .topics
                    .get(s)
                    .and_then(|sl| along(sl.rect.center()))
                    .is_some_and(|(m, x)| x < m && m < main)
        })
    {
        return Some(p);
    }

    layout
        .order
        .iter()
        .filter(|id| **id != from)
        .filter_map(|id| {
            let (main, cross) = along(layout.topics[id].rect.center())?;
            // penalise lateral distance more strongly
            Some((main + cross * 2.5, *id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{ApproxMeasure, layout_sheet};
    use crate::model::{Sheet, Structure};

    #[test]
    fn map_navigation() {
        let mut s = Sheet::new("S", "Root");
        let a = s.add_child(s.root, None, "A");
        let b = s.add_child(s.root, None, "B");
        let a1 = s.add_child(a, None, "A1");
        let l = layout_sheet(&s, &ApproxMeasure, false);
        assert_eq!(navigate(&s, &l, s.root, Direction::Right), Some(a));
        assert_eq!(navigate(&s, &l, s.root, Direction::Left), Some(b));
        assert_eq!(navigate(&s, &l, a, Direction::Right), Some(a1));
        assert_eq!(navigate(&s, &l, a1, Direction::Left), Some(a));
        assert_eq!(navigate(&s, &l, a, Direction::Left), Some(s.root));
        assert_eq!(navigate(&s, &l, b, Direction::Right), Some(s.root));
    }

    #[test]
    fn sibling_navigation() {
        let mut s = Sheet::new("S", "Root");
        s.structure = Structure::LogicRight;
        let a = s.add_child(s.root, None, "A");
        let b = s.add_child(s.root, None, "B");
        let l = layout_sheet(&s, &ApproxMeasure, false);
        assert_eq!(navigate(&s, &l, a, Direction::Down), Some(b));
        assert_eq!(navigate(&s, &l, b, Direction::Up), Some(a));
        assert_eq!(navigate(&s, &l, a, Direction::Up), None);
    }
}
