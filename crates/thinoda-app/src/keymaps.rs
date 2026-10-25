use egui::{Key, KeyboardShortcut, Modifiers, os::OperatingSystem};
use thinoda_canvas::Action;
use thinoda_platform::keymaps::{KeyBinding, KeymapPreset, Keymaps};

use crate::app::{CenterMode, Cmd, ThinodaApp};

pub fn decode(binding: &KeyBinding) -> Result<KeyboardShortcut, String> {
    let key = Key::ALL
        .iter()
        .copied()
        .find(|key| key.name() == binding.key)
        .ok_or_else(|| format!("Unknown keyboard key: {}", binding.key))?;
    Ok(KeyboardShortcut::new(
        Modifiers {
            command: binding.command,
            ctrl: binding.ctrl,
            alt: binding.alt,
            shift: binding.shift,
            ..Default::default()
        },
        key,
    ))
}

pub fn encode(shortcut: KeyboardShortcut, os: OperatingSystem) -> KeyBinding {
    let modifiers = shortcut.modifiers;
    KeyBinding {
        key: shortcut.logical_key.name().into(),
        command: modifiers.command,
        // Host events set both ctrl and command on Windows/Linux.
        ctrl: modifiers.ctrl && (os == OperatingSystem::Mac || !modifiers.command),
        alt: modifiers.alt,
        shift: modifiers.shift,
    }
}

fn preset_overrides(
    preset: KeymapPreset,
    os: OperatingSystem,
) -> Vec<(&'static str, Vec<KeyboardShortcut>)> {
    if preset == KeymapPreset::Standard {
        return Vec::new();
    }
    if os == OperatingSystem::Mac {
        vec![
            (
                "edit.find-replace",
                vec![KeyboardShortcut::new(Modifiers::SHIFT, Key::F)],
            ),
            (
                "tools.map-shot",
                vec![KeyboardShortcut::new(
                    Modifiers::COMMAND | Modifiers::SHIFT,
                    Key::P,
                )],
            ),
            (
                "edit.command-palette",
                vec![KeyboardShortcut::new(
                    Modifiers::COMMAND | Modifiers::ALT,
                    Key::P,
                )],
            ),
        ]
    } else {
        vec![
            ("edit.find", Vec::new()),
            (
                "edit.find-replace",
                vec![KeyboardShortcut::new(Modifiers::COMMAND, Key::F)],
            ),
        ]
    }
}

fn replace(
    bindings: &mut Vec<(KeyboardShortcut, Cmd)>,
    command: Cmd,
    shortcuts: impl IntoIterator<Item = KeyboardShortcut>,
) {
    let id = command.shortcut_id();
    bindings.retain(|(_, existing)| existing.shortcut_id() != id);
    let weight = |modifiers: Modifiers| {
        usize::from(modifiers.command || modifiers.ctrl || modifiers.mac_cmd)
            + usize::from(modifiers.alt)
            + usize::from(modifiers.shift)
    };
    for shortcut in shortcuts {
        let index = bindings
            .iter()
            .position(|(existing, _)| {
                existing.logical_key == shortcut.logical_key
                    && weight(existing.modifiers) < weight(shortcut.modifiers)
            })
            .unwrap_or(bindings.len());
        bindings.insert(index, (shortcut, command.clone()));
    }
}

fn physical(modifiers: Modifiers, os: OperatingSystem) -> Modifiers {
    let mac = os == OperatingSystem::Mac;
    let ctrl = modifiers.ctrl || (!mac && modifiers.command);
    let mac_cmd = mac && (modifiers.mac_cmd || modifiers.command);
    Modifiers {
        ctrl,
        mac_cmd,
        command: if mac { mac_cmd } else { ctrl },
        ..modifiers
    }
}

pub(crate) fn binding_hint(
    ctx: &egui::Context,
    bindings: &[(KeyboardShortcut, Cmd)],
    command: &Cmd,
    mode: CenterMode,
) -> Option<String> {
    binding_hints(ctx, bindings, command, mode)
        .into_iter()
        .next()
}

pub(crate) fn binding_aliases(
    ctx: &egui::Context,
    bindings: &[(KeyboardShortcut, Cmd)],
    command: &Cmd,
    mode: CenterMode,
) -> Option<String> {
    let hints = binding_hints(ctx, bindings, command, mode);
    (!hints.is_empty()).then(|| hints.join(" / "))
}

fn binding_hints(
    ctx: &egui::Context,
    bindings: &[(KeyboardShortcut, Cmd)],
    command: &Cmd,
    mode: CenterMode,
) -> Vec<String> {
    let Some(id) = command.shortcut_id() else {
        return Vec::new();
    };
    let hints: Vec<_> = bindings
        .iter()
        .filter(|(shortcut, candidate)| {
            candidate.shortcut_id() == Some(id)
                && bindings
                    .iter()
                    .find(|(key, _)| {
                        key.logical_key == shortcut.logical_key
                            && physical(shortcut.modifiers, ctx.os())
                                .matches_logically(key.modifiers)
                    })
                    .is_some_and(|(_, first)| first.shortcut_id() == Some(id))
        })
        .map(|(key, _)| ctx.format_shortcut(key))
        .collect();
    if hints.is_empty()
        && matches!(
            command,
            Cmd::MenuPaste | Cmd::Canvas(Action::Copy | Action::Cut | Action::Paste(_))
        )
    {
        return ThinodaApp::shortcut_binding_for_platform(command, mode, ctx.os())
            .map(|shortcut| ctx.format_shortcut(&shortcut))
            .into_iter()
            .collect();
    }
    hints
}

pub(crate) fn reachable(
    bindings: &[(KeyboardShortcut, Cmd)],
    shortcut: KeyboardShortcut,
    command: &Cmd,
    os: OperatingSystem,
) -> bool {
    bindings
        .iter()
        .find(|(key, _)| {
            key.logical_key == shortcut.logical_key
                && physical(shortcut.modifiers, os).matches_logically(key.modifiers)
        })
        .is_some_and(|(_, first)| first.shortcut_id() == command.shortcut_id())
}

pub(crate) fn pressed(ctx: &egui::Context, shortcut: KeyboardShortcut) -> bool {
    ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(event, egui::Event::Key { key, pressed: true, modifiers, .. }
            if *key == shortcut.logical_key && modifiers.matches_logically(shortcut.modifiers))
        })
    })
}

pub fn reserved(shortcut: KeyboardShortcut, os: OperatingSystem, web: bool) -> bool {
    let m = physical(shortcut.modifiers, os);
    shortcut.logical_key == Key::Escape
        || (m.ctrl && shortcut.logical_key == Key::F1)
        || (m.alt
            && matches!(
                shortcut.logical_key,
                Key::Num1
                    | Key::Num2
                    | Key::Num3
                    | Key::Num4
                    | Key::Num5
                    | Key::Num6
                    | Key::Num7
                    | Key::Num8
            ))
        || (m.alt && shortcut.logical_key == Key::F4)
        || (m.command && shortcut.logical_key == Key::Space)
        || (os == OperatingSystem::Mac
            && m.command
            && matches!(shortcut.logical_key, Key::Tab | Key::H | Key::M))
        || (web
            && m.command
            && matches!(
                shortcut.logical_key,
                Key::L | Key::R | Key::W | Key::N | Key::T | Key::Q | Key::Tab
            ))
        || (web && matches!(shortcut.logical_key, Key::F5 | Key::F6 | Key::F11))
        || (web && m.alt && matches!(shortcut.logical_key, Key::ArrowLeft | Key::ArrowRight))
        || (m.ctrl && m.alt && shortcut.logical_key == Key::Delete)
}

pub(crate) fn text_shortcut_command(
    shortcut: KeyboardShortcut,
    os: OperatingSystem,
) -> Option<&'static str> {
    let modifiers = physical(shortcut.modifiers, os);
    if !modifiers.command || modifiers.alt {
        return None;
    }
    match shortcut.logical_key {
        Key::C => Some("edit.copy"),
        Key::X => Some("edit.cut"),
        Key::V => Some("edit.paste"),
        Key::A => Some("selection.all"),
        Key::Z if modifiers.shift => Some("edit.redo"),
        Key::Z => Some("edit.undo"),
        Key::Y => Some("edit.redo"),
        _ => None,
    }
}

impl ThinodaApp {
    /// Runtime dispatch and all displayed accelerators share this resolver.
    pub fn profile_bindings(
        &self,
        maps: &Keymaps,
        mode: CenterMode,
        os: OperatingSystem,
    ) -> Vec<(KeyboardShortcut, Cmd)> {
        let mut bindings = Self::global_shortcut_bindings_for_platform(self.active, os);
        bindings.extend(
            Self::insertion_shortcut_bindings()
                .into_iter()
                .map(|(key, action)| (key, Cmd::Canvas(action))),
        );
        let actions = match mode {
            CenterMode::Map => thinoda_canvas::shortcut_bindings(),
            CenterMode::Outliner => Self::contextual_shortcut_bindings_for_mode(mode),
        };
        for (key, action) in actions {
            let binding = (key, Cmd::Canvas(action));
            if !bindings.contains(&binding) {
                bindings.push(binding);
            }
        }
        let overrides = preset_overrides(maps.preset(), os);
        let registry = if overrides.is_empty() && maps.profile().is_none() {
            Vec::new()
        } else {
            self.shortcut_commands()
        };
        for (id, shortcuts) in overrides {
            if let Some((_, command)) = registry.iter().find(|(candidate, _)| *candidate == id) {
                replace(&mut bindings, command.clone(), shortcuts);
            }
        }
        if let Some(profile) = maps.profile() {
            for (id, shortcuts) in &profile.bindings {
                if let Some((_, command)) = registry.iter().find(|(candidate, _)| *candidate == id)
                {
                    // Invalid serialized keys are diagnosed before activation.
                    replace(
                        &mut bindings,
                        command.clone(),
                        shortcuts.iter().filter_map(|s| decode(s).ok()),
                    );
                }
            }
        }
        bindings
    }

    pub fn active_shortcut_hint(&self, ctx: &egui::Context, command: &Cmd) -> Option<String> {
        let bindings = self.profile_bindings(&self.settings.keymaps, self.mode, ctx.os());
        binding_hint(ctx, &bindings, command, self.mode)
    }

    pub(crate) fn prepare_keymap(&mut self, ctx: &egui::Context) {
        let id = egui::Id::unique("validated_keymaps");
        let os = ctx.os();
        let previous = ctx.data(|data| data.get_temp::<(Keymaps, OperatingSystem)>(id));
        if previous.as_ref() != Some(&(self.settings.keymaps.clone(), os)) {
            if let Err(error) = self.validate_keymaps(
                &self.settings.keymaps,
                ctx.os(),
                cfg!(target_arch = "wasm32"),
            ) {
                log::warn!("Keyboard profile not activated: {error}");
                self.toasts.error(error);
                self.settings.keymaps.active = thinoda_platform::keymaps::STANDARD.into();
            }
            ctx.data_mut(|data| data.insert_temp(id, (self.settings.keymaps.clone(), os)));
        }
        for mode in [CenterMode::Map, CenterMode::Outliner] {
            let bindings = self.profile_bindings(&self.settings.keymaps, mode, os);
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::unique(("active_keymap", mode)), bindings)
            });
        }
    }

    pub fn validate_keymaps(
        &self,
        maps: &Keymaps,
        os: OperatingSystem,
        web: bool,
    ) -> Result<(), String> {
        maps.validate()?;
        for profile in &maps.profiles {
            for (id, shortcuts) in &profile.bindings {
                if self.shortcut_command(id).is_none() {
                    return Err(format!("Unknown keyboard command: {id}"));
                }
                for shortcut in shortcuts {
                    let key = decode(shortcut)?;
                    if reserved(key, os, web) {
                        return Err(format!(
                            "Reserved keyboard shortcut: {id} ({})",
                            shortcut.key
                        ));
                    }
                    if let Some(owner) = text_shortcut_command(key, os)
                        && owner != id
                    {
                        return Err(format!("Reserved text-editing shortcut: {id} / {owner}"));
                    }
                }
            }
        }
        for profile in &maps.profiles {
            let mut selected = maps.clone();
            selected.active = profile.id.clone();
            for mode in [CenterMode::Map, CenterMode::Outliner] {
                let bindings = self.profile_bindings(&selected, mode, os);
                for (key, command) in &bindings {
                    let id = command.shortcut_id().unwrap_or("");
                    for (other_key, other) in &bindings {
                        let other_id = other.shortcut_id().unwrap_or("");
                        if id != other_id
                            && (profile.bindings.contains_key(id)
                                || profile.bindings.contains_key(other_id))
                            && key.logical_key == other_key.logical_key
                            && (physical(key.modifiers, os).matches_exact(other_key.modifiers)
                                || !reachable(&bindings, *key, command, os))
                        {
                            return Err(format!(
                                "Keyboard shortcut conflict: {id} / {other_id} ({mode:?}, {})",
                                profile.name
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn active_canvas_bindings(
        &self,
        os: OperatingSystem,
    ) -> Vec<(KeyboardShortcut, Action)> {
        self.profile_bindings(&self.settings.keymaps, self.mode, os)
            .into_iter()
            .filter_map(|(key, command)| match command {
                Cmd::Canvas(action) => Some((key, action)),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_and_host_modifiers_roundtrip() {
        for os in [
            OperatingSystem::Windows,
            OperatingSystem::Mac,
            OperatingSystem::Nix,
        ] {
            for key in Key::ALL {
                for modifiers in [
                    Modifiers::NONE,
                    Modifiers::COMMAND | Modifiers::SHIFT,
                    Modifiers::CTRL | Modifiers::ALT,
                ] {
                    let shortcut = KeyboardShortcut::new(modifiers, *key);
                    assert_eq!(decode(&encode(shortcut, os)).unwrap(), shortcut);
                }
            }
        }
    }

    #[test]
    fn close_window_defaults_are_platform_specific_and_can_be_reassigned() {
        let app = ThinodaApp::headless();
        let mut maps = Keymaps::default();
        let close = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
        for os in [
            OperatingSystem::Windows,
            OperatingSystem::Nix,
            OperatingSystem::Mac,
        ] {
            let bindings = app.profile_bindings(&maps, CenterMode::Map, os);
            let command = if os == OperatingSystem::Mac {
                Cmd::CloseWindow
            } else {
                Cmd::CloseTab(app.active)
            };
            assert!(bindings.contains(&(close, command)));
            if os == OperatingSystem::Mac {
                assert!(
                    !bindings
                        .iter()
                        .any(|(_, command)| matches!(command, Cmd::CloseTab(_)))
                );
            }
        }
        maps.duplicate_active("Window controls").unwrap();
        let reassigned = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::ALT, Key::J);
        maps.profiles[0].bindings.insert(
            "window.close".into(),
            vec![encode(reassigned, OperatingSystem::Mac)],
        );
        maps.profiles[0].bindings.insert(
            "file.close-tab".into(),
            vec![encode(close, OperatingSystem::Mac)],
        );
        app.validate_keymaps(&maps, OperatingSystem::Mac, false)
            .unwrap();
        let bindings = app.profile_bindings(&maps, CenterMode::Map, OperatingSystem::Mac);
        assert!(bindings.contains(&(reassigned, Cmd::CloseWindow)));
        assert!(bindings.contains(&(close, Cmd::CloseTab(app.active))));
        maps.profiles[0]
            .bindings
            .insert("window.close".into(), vec![]);
        assert!(
            !app.profile_bindings(&maps, CenterMode::Map, OperatingSystem::Mac)
                .iter()
                .any(|(_, command)| *command == Cmd::CloseWindow)
        );
        for key in [Key::H, Key::M] {
            assert!(reserved(
                KeyboardShortcut::new(Modifiers::COMMAND, key),
                OperatingSystem::Mac,
                false
            ));
            assert!(!reserved(
                KeyboardShortcut::new(Modifiers::COMMAND, key),
                OperatingSystem::Windows,
                false
            ));
        }
    }

    #[test]
    fn explicit_unassignment_removes_aliases_and_legacy_never_activates() {
        let mut app = ThinodaApp::headless();
        app.settings
            .shortcuts_overrides
            .insert("file.save".into(), "Alt+S".into());
        let standard = app.profile_bindings(
            &app.settings.keymaps,
            CenterMode::Map,
            OperatingSystem::Windows,
        );
        assert!(standard.contains(&(KeyboardShortcut::new(Modifiers::COMMAND, Key::S), Cmd::Save)));
        app.settings.keymaps.duplicate_active("Writing").unwrap();
        app.settings.keymaps.profiles[0]
            .bindings
            .insert("topic.insert-child".into(), vec![]);
        assert!(
            !app.active_canvas_bindings(OperatingSystem::Windows)
                .iter()
                .any(|(_, a)| *a == Action::InsertChild)
        );
        assert!(
            app.validate_keymaps(&app.settings.keymaps, OperatingSystem::Windows, false)
                .is_ok()
        );
    }

    #[test]
    fn preset_aliases_and_specific_modified_bindings_remain_reachable() {
        let app = ThinodaApp::headless();
        let maps = Keymaps {
            active: thinoda_platform::keymaps::XMIND.into(),
            ..Default::default()
        };
        let bindings = app.profile_bindings(&maps, CenterMode::Map, OperatingSystem::Mac);
        for (key, command) in [
            (
                KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::P),
                Cmd::MapShot,
            ),
            (
                KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::ALT, Key::P),
                Cmd::Palette,
            ),
        ] {
            assert!(
                reachable(&bindings, key, &command, OperatingSystem::Mac),
                "{command:?}"
            );
        }
        let mut maps = Keymaps::default();
        maps.duplicate_active("Specific").unwrap();
        maps.profiles[0].bindings.insert(
            "file.save-as".into(),
            vec![encode(
                KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S),
                OperatingSystem::Windows,
            )],
        );
        assert!(
            app.validate_keymaps(&maps, OperatingSystem::Windows, false)
                .is_ok()
        );
    }

    #[test]
    fn rejects_conflicts_reserved_and_unknown_keys() {
        let app = ThinodaApp::headless();
        let mut maps = Keymaps::default();
        maps.duplicate_active("Custom").unwrap();
        for shortcut in [
            KeyBinding {
                key: "N".into(),
                command: true,
                ..Default::default()
            },
            KeyBinding {
                key: "Escape".into(),
                ..Default::default()
            },
            KeyBinding {
                key: "NoSuchKey".into(),
                ..Default::default()
            },
        ] {
            maps.profiles[0]
                .bindings
                .insert("file.save".into(), vec![shortcut]);
            assert!(
                app.validate_keymaps(&maps, OperatingSystem::Windows, false)
                    .is_err()
            );
        }
    }
}
