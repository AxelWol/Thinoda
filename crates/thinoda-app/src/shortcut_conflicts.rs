//! Read-only audit of default key dispatch with a focused, idle topic and no popup,
//! modal, search or text editor. This is not an OS/browser reserved-key detector.

use egui::{KeyboardShortcut, Modifiers, os::OperatingSystem};

use crate::app::{CenterMode, Cmd, ThinodaApp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutConflictKind {
    Duplicate,
    Shadowed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShortcutConflict {
    pub shortcut: KeyboardShortcut,
    pub command: Cmd,
    pub intercepted_by: Cmd,
    pub intercepting_shortcut: KeyboardShortcut,
    pub kind: ShortcutConflictKind,
}

fn ordered_bindings(mode: CenterMode) -> Vec<(KeyboardShortcut, Cmd)> {
    let mut bindings = ThinodaApp::global_shortcut_bindings(0);
    bindings.extend(
        ThinodaApp::insertion_shortcut_bindings()
            .into_iter()
            .map(|(shortcut, action)| (shortcut, Cmd::Canvas(action))),
    );
    let contextual = match mode {
        CenterMode::Map => thinoda_canvas::shortcut_bindings(),
        CenterMode::Outliner => ThinodaApp::outline_shortcut_bindings(),
    };
    bindings.extend(
        contextual
            .into_iter()
            .map(|(shortcut, action)| (shortcut, Cmd::Canvas(action))),
    );
    bindings
}

/// Turn a logical binding into the modifiers delivered by the host integration.
fn pressed_modifiers(modifiers: Modifiers, os: OperatingSystem) -> Modifiers {
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

fn first_match(
    bindings: &[(KeyboardShortcut, Cmd)],
    shortcut: KeyboardShortcut,
    os: OperatingSystem,
) -> Option<&(KeyboardShortcut, Cmd)> {
    let pressed = pressed_modifiers(shortcut.modifiers, os);
    bindings.iter().find(|(binding, _)| {
        binding.logical_key == shortcut.logical_key && pressed.matches_logically(binding.modifiers)
    })
}

impl ThinodaApp {
    /// Diagnose an event against defaults, not saved overrides. Ownership assumes
    /// a focused topic in the supplied view; clipboard events and row traversal
    /// are not command-key dispatch and are deliberately excluded.
    pub fn default_shortcut_command(
        mode: CenterMode,
        os: OperatingSystem,
        shortcut: KeyboardShortcut,
    ) -> Option<Cmd> {
        static MAP: std::sync::OnceLock<Vec<(KeyboardShortcut, Cmd)>> = std::sync::OnceLock::new();
        static OUTLINE: std::sync::OnceLock<Vec<(KeyboardShortcut, Cmd)>> =
            std::sync::OnceLock::new();
        let bindings = match mode {
            CenterMode::Map => &MAP,
            CenterMode::Outliner => &OUTLINE,
        }
        .get_or_init(|| ordered_bindings(mode));
        first_match(bindings, shortcut, os).map(|(_, command)| command.clone())
    }

    /// Report declared gestures intercepted by a different earlier command.
    /// Specific-before-general bindings and redundant same-command aliases are
    /// not conflicts. No keys, preferences or runtime handlers are changed.
    pub fn default_shortcut_conflicts(
        mode: CenterMode,
        os: OperatingSystem,
    ) -> Vec<ShortcutConflict> {
        conflicts(&ordered_bindings(mode), os)
    }
}

fn conflicts(bindings: &[(KeyboardShortcut, Cmd)], os: OperatingSystem) -> Vec<ShortcutConflict> {
    bindings
        .iter()
        .enumerate()
        .filter_map(|(index, (shortcut, command))| {
            let (intercepting_shortcut, intercepted_by) =
                first_match(&bindings[..index], *shortcut, os)?;
            if intercepted_by == command {
                return None;
            }
            let pressed = pressed_modifiers(shortcut.modifiers, os);
            Some(ShortcutConflict {
                shortcut: *shortcut,
                command: command.clone(),
                intercepted_by: intercepted_by.clone(),
                intercepting_shortcut: *intercepting_shortcut,
                kind: if pressed.matches_exact(intercepting_shortcut.modifiers) {
                    ShortcutConflictKind::Duplicate
                } else {
                    ShortcutConflictKind::Shadowed
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, InputState, Key};
    use thinoda_canvas::Action;

    #[test]
    fn physical_command_modifiers_remain_host_specific() {
        let command = Modifiers::COMMAND | Modifiers::SHIFT;
        assert_eq!(
            pressed_modifiers(command, OperatingSystem::Mac),
            Modifiers {
                mac_cmd: true,
                command: true,
                shift: true,
                ..Default::default()
            }
        );
        for os in [OperatingSystem::Windows, OperatingSystem::Nix] {
            assert_eq!(
                pressed_modifiers(command, os),
                Modifiers {
                    ctrl: true,
                    command: true,
                    shift: true,
                    ..Default::default()
                }
            );
        }
        assert_eq!(
            pressed_modifiers(Modifiers::CTRL, OperatingSystem::Mac),
            Modifiers::CTRL
        );
    }

    #[test]
    fn diagnosed_collisions_reproduce_in_the_real_ui_without_changing_defaults() {
        use egui_kittest::Harness;
        for os in [
            OperatingSystem::Windows,
            OperatingSystem::Mac,
            OperatingSystem::Nix,
        ] {
            for mode in [CenterMode::Map, CenterMode::Outliner] {
                let mut app = ThinodaApp::headless();
                app.mode = mode;
                app.settings.show_format_panel = false;
                let mut harness = Harness::builder()
                    .with_size(egui::vec2(1000.0, 800.0))
                    .build_ui_state(
                        move |ui, app: &mut ThinodaApp| {
                            ui.ctx().set_os(os);
                            app.draw(ui);
                        },
                        app,
                    );
                harness
                    .ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                harness.run_steps(4);
                let before = harness.state().doc().unwrap().doc.workbook().clone();
                let ctx = harness.ctx.clone();
                harness
                    .state_mut()
                    .exec(&ctx, Cmd::Canvas(Action::ToggleMarker("priority-1".into())));
                harness.run_steps(3);
                let edited = harness.state().doc().unwrap().doc.workbook().clone();
                assert_ne!(before, edited);
                let modifiers = pressed_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, os);
                harness.key_press_modifiers(modifiers, Key::Z);
                harness.run_steps(3);
                assert_eq!(
                    harness.state().doc().unwrap().doc.workbook(),
                    if mode == CenterMode::Outliner {
                        &before
                    } else {
                        &edited
                    }
                );
                if mode == CenterMode::Outliner {
                    harness.key_press_modifiers(pressed_modifiers(Modifiers::COMMAND, os), Key::Y);
                    harness.run_steps(3);
                    assert_eq!(harness.state().doc().unwrap().doc.workbook(), &edited);
                }
                harness.key_press_modifiers(modifiers, Key::H);
                harness.run_steps(3);
                assert!(harness.state().search.open && harness.state().search.show_replace);
                assert_eq!(harness.state().doc().unwrap().doc.workbook(), &edited);
            }
        }
    }

    #[test]
    fn audit_matches_actual_egui_consumption_for_every_default_and_modifier_variant() {
        for os in [
            OperatingSystem::Windows,
            OperatingSystem::Mac,
            OperatingSystem::Nix,
        ] {
            for mode in [CenterMode::Map, CenterMode::Outliner] {
                let bindings = ordered_bindings(mode);
                for (shortcut, _) in &bindings {
                    for extra in [
                        Modifiers::NONE,
                        Modifiers::SHIFT,
                        Modifiers::ALT,
                        Modifiers::SHIFT | Modifiers::ALT,
                    ] {
                        let candidate =
                            KeyboardShortcut::new(shortcut.modifiers | extra, shortcut.logical_key);
                        let mut input = InputState::default();
                        input.events.push(Event::Key {
                            key: candidate.logical_key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: pressed_modifiers(candidate.modifiers, os),
                        });
                        let actual = bindings
                            .iter()
                            .find(|(binding, _)| input.consume_shortcut(binding))
                            .map(|(_, command)| command.clone());
                        assert_eq!(
                            ThinodaApp::default_shortcut_command(mode, os, candidate),
                            actual,
                            "{os:?}/{mode:?}/{candidate:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn duplicates_shadowing_and_correct_specificity_are_distinct() {
        let save = (KeyboardShortcut::new(Modifiers::COMMAND, Key::S), Cmd::Save);
        let save_as = (
            KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S),
            Cmd::SaveAs,
        );
        for os in [
            OperatingSystem::Windows,
            OperatingSystem::Mac,
            OperatingSystem::Nix,
        ] {
            assert_eq!(
                first_match(&[save.clone(), save_as.clone()], save_as.0, os),
                Some(&save)
            );
            assert_eq!(
                first_match(&[save_as.clone(), save.clone()], save.0, os),
                Some(&save)
            );
            let physical = if os == OperatingSystem::Mac {
                Modifiers::MAC_CMD
            } else {
                Modifiers::CTRL
            };
            let shortcut = KeyboardShortcut::new(physical, Key::S);
            assert!(pressed_modifiers(shortcut.modifiers, os).matches_exact(save.0.modifiers));
            assert_eq!(
                first_match(std::slice::from_ref(&save), shortcut, os),
                Some(&save)
            );
            assert_eq!(
                conflicts(&[save.clone(), (shortcut, Cmd::SaveAll)], os)[0].kind,
                ShortcutConflictKind::Duplicate
            );
            assert_eq!(
                conflicts(&[save.clone(), save_as.clone()], os)[0].kind,
                ShortcutConflictKind::Shadowed
            );
            assert!(conflicts(&[save_as.clone(), save.clone()], os).is_empty());
            assert!(conflicts(&[save.clone(), (shortcut, Cmd::Save)], os).is_empty());
            if os == OperatingSystem::Mac {
                assert!(
                    first_match(
                        std::slice::from_ref(&save),
                        KeyboardShortcut::new(Modifiers::CTRL, Key::S),
                        os
                    )
                    .is_none()
                );
            }
        }
    }

    #[test]
    fn current_collisions_are_reported_without_treating_aliases_as_conflicts() {
        for os in [
            OperatingSystem::Windows,
            OperatingSystem::Mac,
            OperatingSystem::Nix,
        ] {
            let conflicts = ThinodaApp::default_shortcut_conflicts(CenterMode::Map, os);
            assert_eq!(conflicts.len(), 1, "{os:?}: {conflicts:?}");
            assert!(
                conflicts
                    .iter()
                    .any(|conflict| conflict.command == Cmd::Canvas(Action::Fit)
                        && conflict.intercepted_by == Cmd::FindReplace
                        && conflict.kind == ShortcutConflictKind::Shadowed)
            );
            assert!(ThinodaApp::default_shortcut_conflicts(CenterMode::Outliner, os).is_empty());
            assert_eq!(
                ThinodaApp::default_shortcut_command(
                    CenterMode::Outliner,
                    os,
                    KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                ),
                Some(Cmd::Canvas(Action::Undo))
            );
            assert_eq!(
                ThinodaApp::default_shortcut_command(
                    CenterMode::Map,
                    os,
                    KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                ),
                Some(Cmd::Canvas(Action::Redo))
            );
        }
    }
}
