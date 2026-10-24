use super::MenuItem;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MacMenuLabels {
    pub application: String,
    pub services: String,
    pub window: String,
    pub hide: String,
    pub hide_others: String,
    pub show_all: String,
    pub minimize: String,
    pub zoom: String,
    pub bring_all: String,
}

/// Key equivalents come from command bindings, never from document/menu titles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacAccelerator {
    pub key: String,
    pub command: bool,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacMenuModel {
    pub items: Vec<MenuItem>,
    /// About, Settings and Quit, in standard application-menu order.
    pub application_commands: [u16; 3],
    pub labels: MacMenuLabels,
    pub shortcuts: BTreeMap<u16, MacAccelerator>,
}

#[cfg(any(target_os = "macos", test))]
impl MacMenuModel {
    pub(super) fn encode(&self) -> std::io::Result<std::ffi::CString> {
        use serde_json::{Value, json};
        use std::{collections::HashSet, io};
        fn key_equivalent(shortcut: Option<&MacAccelerator>) -> (String, u32) {
            let Some(shortcut) = shortcut else {
                return (String::new(), 0);
            };
            // Plain Enter/Tab/arrows belong to the editor, not AppKit's global menu.
            if !(shortcut.command || shortcut.control || shortcut.alt) {
                return (String::new(), 0);
            }
            let key = match shortcut.key.as_str() {
                "ArrowUp" => "\u{f700}".into(),
                "ArrowDown" => "\u{f701}".into(),
                "ArrowLeft" => "\u{f702}".into(),
                "ArrowRight" => "\u{f703}".into(),
                "PageUp" => "\u{f72c}".into(),
                "PageDown" => "\u{f72d}".into(),
                "Home" => "\u{f729}".into(),
                "End" => "\u{f72b}".into(),
                "Enter" => "\r".into(),
                "Tab" => "\t".into(),
                "Space" => " ".into(),
                "Backspace" => "\u{8}".into(),
                "Delete" => "\u{7f}".into(),
                "Escape" => "\u{1b}".into(),
                key if key.chars().count() == 1 => key.to_lowercase(),
                _ => String::new(),
            };
            let modifiers = u32::from(shortcut.shift) << 17
                | u32::from(shortcut.control) << 18
                | u32::from(shortcut.alt) << 19
                | u32::from(shortcut.command) << 20;
            (key, modifiers)
        }
        fn entries(
            items: &[MenuItem],
            shortcuts: &BTreeMap<u16, MacAccelerator>,
            ids: &mut HashSet<u16>,
        ) -> io::Result<Vec<Value>> {
            items.iter().map(|item| Ok(match item {
                MenuItem::Action { id, label, enabled, checked } => {
                    if *id == 0 || !ids.insert(*id) {
                        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Duplicate or zero menu ID"));
                    }
                    let (key, modifiers) = key_equivalent(shortcuts.get(id));
                    json!({"id":id, "label":label, "enabled":enabled, "checked":checked, "key":key, "modifiers":modifiers})
                }
                MenuItem::Submenu { label, enabled, items } => json!({
                    "label":label, "enabled":enabled, "items":entries(items,shortcuts,ids)?
                }),
                MenuItem::Separator => json!({"separator":true}),
                MenuItem::Caption(label) => json!({"label":label}),
            })).collect()
        }
        let mut ids = HashSet::new();
        let items = entries(&self.items, &self.shortcuts, &mut ids)?;
        if self.application_commands.iter().any(|id| !ids.contains(id)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Missing application menu command",
            ));
        }
        if self
            .items
            .iter()
            .any(|item| !matches!(item, MenuItem::Submenu { .. }))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "AppKit requires top-level submenus",
            ));
        }
        let json = serde_json::to_string(&json!({
            "items":items, "application_commands":self.application_commands, "labels":self.labels
        }))
        .map_err(io::Error::other)?;
        std::ffi::CString::new(json).map_err(io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> MacMenuModel {
        MacMenuModel {
            items: vec![MenuItem::Submenu {
                label: "File".into(),
                enabled: true,
                items: (1..=3)
                    .map(|id| MenuItem::Action {
                        id,
                        label: "Untrusted name\t\u{2318}Q".into(),
                        enabled: id != 2,
                        checked: id == 3,
                    })
                    .collect(),
            }],
            application_commands: [1, 2, 3],
            labels: MacMenuLabels {
                application: "Thinoda".into(),
                services: "Services".into(),
                window: "Window".into(),
                hide: "Hide".into(),
                hide_others: "Hide Others".into(),
                show_all: "Show All".into(),
                minimize: "Minimize".into(),
                zoom: "Zoom".into(),
                bring_all: "Bring All to Front".into(),
            },
            shortcuts: BTreeMap::new(),
        }
    }
    #[test]
    fn menu_titles_cannot_invent_accelerators_and_states_are_preserved() {
        let mut model = model();
        model.shortcuts.insert(
            1,
            MacAccelerator {
                key: "S".into(),
                command: true,
                control: false,
                alt: false,
                shift: true,
            },
        );
        model.shortcuts.insert(
            2,
            MacAccelerator {
                key: "Enter".into(),
                command: false,
                control: false,
                alt: false,
                shift: false,
            },
        );
        let value: serde_json::Value =
            serde_json::from_str(model.encode().unwrap().to_str().unwrap()).unwrap();
        let actions = &value["items"][0]["items"];
        assert_eq!(actions[0]["key"], "s");
        assert_eq!(actions[0]["modifiers"], (1 << 20) | (1 << 17));
        assert_eq!(actions[1]["key"], "");
        assert_eq!(actions[2]["key"], "");
        assert_eq!(actions[1]["enabled"], false);
        assert_eq!(actions[2]["checked"], true);
        assert_eq!(actions[2]["label"], "Untrusted name\t\u{2318}Q");
    }
    #[test]
    fn invalid_ids_or_missing_application_commands_are_rejected() {
        let mut model = model();
        model.application_commands[0] = 4;
        assert!(model.encode().is_err());
        model.application_commands = [1, 2, 3];
        let MenuItem::Submenu { items, .. } = &mut model.items[0] else {
            unreachable!()
        };
        items.push(items[0].clone());
        assert!(model.encode().is_err());
    }
}
