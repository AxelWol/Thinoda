//! Marker catalog (XMind compatible marker ids rendered with emoji / symbols).

use crate::Color;

pub fn contrast_threshold(id: &str) -> f32 {
    if matches!(group_of(id), "month" | "week") {
        4.5
    } else {
        3.0
    }
}

/// Legacy primary ink. Explicit colors override this, not the category or progress.
pub fn default_color(id: &str) -> Color {
    let named = |name| match name {
        "red" => Color::rgb(0xE5, 0x48, 0x4D),
        "orange" => Color::rgb(0xF5, 0x8A, 0x1F),
        "yellow" => Color::rgb(0xF2, 0xC0, 0x1E),
        "green" => Color::rgb(0x3C, 0xB3, 0x71),
        "blue" => Color::rgb(0x2E, 0x7C, 0xF6),
        "dark-blue" => Color::rgb(0x23, 0x45, 0x78),
        "purple" => Color::rgb(0x8E, 0x5C, 0xE6),
        _ => Color::rgb(0x8A, 0x8F, 0x98),
    };
    let (group, rest) = id.split_once('-').unwrap_or((id, ""));
    match group {
        "priority" => {
            let n = rest.parse::<usize>().unwrap_or(1);
            named(["red", "orange", "blue", "green", "purple"][(n.saturating_sub(1)) % 5])
        }
        "task" => named("green"),
        "flag" | "people" | "star" => named(rest),
        _ => Color::rgb(0x44, 0x4C, 0x58),
    }
}

pub fn task_fraction(id: &str) -> f32 {
    match id.strip_prefix("task-").unwrap_or(id) {
        "start" => 0.0,
        "oct" => 0.125,
        "quarter" => 0.25,
        "3oct" => 0.375,
        "half" => 0.5,
        "5oct" => 0.625,
        "3quar" => 0.75,
        "7oct" => 0.875,
        _ => 1.0,
    }
}

/// A marker group. Only one marker per group can be attached to a topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerGroup {
    pub id: &'static str,
    pub name: &'static str,
    pub markers: &'static [Marker],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    pub id: &'static str,
    /// Glyph used to render the marker.
    pub glyph: &'static str,
    pub name: &'static str,
}

const fn m(id: &'static str, glyph: &'static str, name: &'static str) -> Marker {
    Marker { id, glyph, name }
}

pub const GROUPS: &[MarkerGroup] = &[
    MarkerGroup {
        id: "priority",
        name: "Priority",
        markers: &[
            m("priority-1", "①", "Priority 1"),
            m("priority-2", "②", "Priority 2"),
            m("priority-3", "③", "Priority 3"),
            m("priority-4", "④", "Priority 4"),
            m("priority-5", "⑤", "Priority 5"),
            m("priority-6", "⑥", "Priority 6"),
            m("priority-7", "⑦", "Priority 7"),
            m("priority-8", "⑧", "Priority 8"),
            m("priority-9", "⑨", "Priority 9"),
        ],
    },
    MarkerGroup {
        id: "task",
        name: "Task",
        markers: &[
            m("task-start", "○", "Start"),
            m("task-oct", "◔", "1/8 done"),
            m("task-quarter", "◔", "1/4 done"),
            m("task-3oct", "◑", "3/8 done"),
            m("task-half", "◑", "Half done"),
            m("task-5oct", "◕", "5/8 done"),
            m("task-3quar", "◕", "3/4 done"),
            m("task-7oct", "◕", "7/8 done"),
            m("task-pause", "Ⅱ", "Paused"),
            m("task-done", "●", "Done"),
        ],
    },
    MarkerGroup {
        id: "flag",
        name: "Flag",
        markers: &[
            m("flag-red", "🚩", "Red flag"),
            m("flag-orange", "🏳", "Orange flag"),
            m("flag-yellow", "🏳", "Yellow flag"),
            m("flag-green", "🏳", "Green flag"),
            m("flag-blue", "🏳", "Blue flag"),
            m("flag-purple", "🏳", "Purple flag"),
            m("flag-gray", "🏳", "Gray flag"),
            m("flag-dark-blue", "🏳", "Dark blue flag"),
        ],
    },
    MarkerGroup {
        id: "star",
        name: "Star",
        markers: &[
            m("star-red", "★", "Red star"),
            m("star-orange", "★", "Orange star"),
            m("star-yellow", "⭐", "Yellow star"),
            m("star-green", "★", "Green star"),
            m("star-blue", "★", "Blue star"),
            m("star-purple", "★", "Purple star"),
            m("star-gray", "☆", "Gray star"),
            m("star-dark-blue", "★", "Dark blue star"),
        ],
    },
    MarkerGroup {
        id: "smiley",
        name: "Smiley",
        markers: &[
            m("smiley-smile", "🙂", "Smile"),
            m("smiley-laugh", "😄", "Laugh"),
            m("smiley-angry", "😠", "Angry"),
            m("smiley-cry", "😢", "Cry"),
            m("smiley-surprise", "😮", "Surprise"),
            m("smiley-boring", "😐", "Boring"),
            m("smiley-embarrass", "😳", "Embarrassed"),
        ],
    },
    MarkerGroup {
        id: "people",
        name: "People",
        markers: &[
            m("people-red", "♟", "Red person"),
            m("people-orange", "♟", "Orange person"),
            m("people-yellow", "♟", "Yellow person"),
            m("people-green", "♟", "Green person"),
            m("people-blue", "♟", "Blue person"),
            m("people-purple", "♟", "Purple person"),
            m("people-dark-blue", "♟", "Dark blue person"),
        ],
    },
    MarkerGroup {
        id: "arrow",
        name: "Arrow",
        markers: &[
            m("arrow-left", "←", "Arrow left"),
            m("arrow-right", "→", "Arrow right"),
            m("arrow-up", "↑", "Arrow up"),
            m("arrow-down", "↓", "Arrow down"),
            m("arrow-left-right", "↔", "Arrow left and right"),
            m("arrow-up-down", "↕", "Arrow up and down"),
            m("arrow-refresh", "⟳", "Refresh arrow"),
        ],
    },
    MarkerGroup {
        id: "symbol",
        name: "Symbol",
        markers: &[
            m("symbol-plus", "➕", "Plus"),
            m("symbol-minus", "➖", "Minus"),
            m("symbol-question", "❓", "Question"),
            m("symbol-exclam", "❗", "Exclamation"),
            m("symbol-info", "ℹ", "Info"),
            m("symbol-wrong", "✖", "Wrong"),
            m("symbol-right", "✔", "Correct"),
            m("symbol-like", "👍", "Like"),
            m("symbol-dislike", "👎", "Dislike"),
            m("symbol-idea", "💡", "Idea"),
            m("symbol-attention", "⚠", "Attention"),
            m("symbol-money", "💰", "Money"),
        ],
    },
    MarkerGroup {
        id: "month",
        name: "Month",
        markers: &[
            m("month-jan", "Jan", "January"),
            m("month-feb", "Feb", "February"),
            m("month-mar", "Mar", "March"),
            m("month-apr", "Apr", "April"),
            m("month-may", "May", "May"),
            m("month-jun", "Jun", "June"),
            m("month-jul", "Jul", "July"),
            m("month-aug", "Aug", "August"),
            m("month-sep", "Sep", "September"),
            m("month-oct", "Oct", "October"),
            m("month-nov", "Nov", "November"),
            m("month-dec", "Dec", "December"),
        ],
    },
    MarkerGroup {
        id: "week",
        name: "Week",
        markers: &[
            m("week-mon", "Mo", "Monday"),
            m("week-tue", "Tu", "Tuesday"),
            m("week-wed", "We", "Wednesday"),
            m("week-thu", "Th", "Thursday"),
            m("week-fri", "Fr", "Friday"),
            m("week-sat", "Sa", "Saturday"),
            m("week-sun", "Su", "Sunday"),
        ],
    },
];

/// Looks up a marker by id.
pub fn find(id: &str) -> Option<&'static Marker> {
    GROUPS
        .iter()
        .flat_map(|g| g.markers.iter())
        .find(|m| m.id == id)
}

/// Group id of a marker id (`priority-1` → `priority`).
pub fn group_of(id: &str) -> &str {
    id.split_once('-').map(|(g, _)| g).unwrap_or(id)
}

/// Glyph for a marker, falling back to a bullet for unknown ids.
pub fn glyph(id: &str) -> &'static str {
    find(id).map(|m| m.glyph).unwrap_or("•")
}

/// Toggles `marker` in `markers`, replacing any other marker of the same group.
pub fn toggle(markers: &mut Vec<String>, marker: &str) {
    if let Some(pos) = markers.iter().position(|m| m == marker) {
        markers.remove(pos);
        return;
    }
    let g = group_of(marker).to_string();
    match markers.iter().position(|m| group_of(m) == g) {
        Some(pos) => markers[pos] = marker.to_string(),
        None => markers.push(marker.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_and_group() {
        assert_eq!(find("priority-1").unwrap().glyph, "①");
        assert_eq!(group_of("task-half"), "task");
        assert_eq!(glyph("unknown-x"), "•");
        let ids: std::collections::HashSet<_> = GROUPS
            .iter()
            .flat_map(|g| g.markers.iter().map(|m| m.id))
            .collect();
        assert_eq!(
            ids.len(),
            GROUPS.iter().map(|g| g.markers.len()).sum::<usize>()
        );
    }

    #[test]
    fn toggle_replaces_same_group() {
        let mut v = vec![];
        toggle(&mut v, "priority-1");
        toggle(&mut v, "flag-red");
        toggle(&mut v, "priority-3");
        assert_eq!(v, vec!["priority-3", "flag-red"]);
        toggle(&mut v, "priority-3");
        assert_eq!(v, vec!["flag-red"]);
    }

    #[test]
    fn new_xmind_marker_ids_have_color_and_pause_semantics() {
        assert_eq!(
            default_color("people-dark-blue"),
            Color::rgb(0x23, 0x45, 0x78)
        );
        assert_eq!(
            default_color("flag-dark-blue"),
            Color::rgb(0x23, 0x45, 0x78)
        );
        for id in [
            "task-pause",
            "flag-dark-blue",
            "star-dark-blue",
            "smiley-embarrass",
            "people-red",
            "people-dark-blue",
            "arrow-left",
            "arrow-refresh",
        ] {
            assert!(find(id).is_some(), "{id}");
            assert!(!glyph(id).is_empty(), "{id}");
        }
    }
}
