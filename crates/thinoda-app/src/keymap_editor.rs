use egui::{Event, Key, KeyboardShortcut};
use thinoda_platform::keymaps::{Keymaps, STANDARD, XMIND};

use crate::{app::ThinodaApp, i18n::t};

pub struct KeymapEditor {
    original: Keymaps,
    draft: Keymaps,
    name: String,
    query: String,
    capture: Option<String>,
    error: Option<String>,
    confirm_delete: bool,
    composing: bool,
}

impl KeymapEditor {
    pub fn new(maps: &Keymaps) -> Self {
        Self {
            original: maps.clone(),
            draft: maps.clone(),
            name: String::new(),
            query: String::new(),
            capture: None,
            error: None,
            confirm_delete: false,
            composing: false,
        }
    }
}

impl ThinodaApp {
    fn apply_keymap_settings(
        &mut self,
        ctx: &egui::Context,
        keymaps: Keymaps,
    ) -> Result<(), String> {
        self.validate_keymaps(&keymaps, ctx.os(), cfg!(target_arch = "wasm32"))?;
        let mut settings = self.settings.clone();
        settings.keymaps = keymaps;
        settings.format_tab = self.format_tab;
        settings.sidebar_family = self.sidebar_family;
        if let Some(paths) = &self.services.paths {
            settings
                .save_with_credentials(paths, &thinoda_platform::credentials::NativeCredentialStore)
                .map_err(|error| error.to_string())?;
        }
        self.dialogs.settings_gesture = None;
        self.settings = settings;
        self.prepare_keymap(ctx);
        for viewport in
            std::iter::once(egui::ViewportId::ROOT).chain(self.document_windows.values().copied())
        {
            ctx.request_repaint_of(viewport);
        }
        Ok(())
    }

    pub(crate) fn keymap_editor(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.dialogs.shortcut_browser.editor.take() else {
            return;
        };
        let touch = thinoda_ui::widgets::touch_activation_allowed(ctx);
        let ime_event = editor.composing
            || ctx.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, Event::Ime(_)))
            });
        if editor.capture.is_some() {
            thinoda_canvas::protect_ime_candidate_keys(
                ctx,
                egui::Id::unique("keymap_capture"),
                &mut editor.composing,
            );
        }
        let catalog = self.command_catalog(ctx);
        let commands: Vec<_> = self
            .shortcut_commands()
            .into_iter()
            .map(|(id, command)| {
                let title = catalog
                    .iter()
                    .find(|(candidate, _)| candidate.shortcut_id() == Some(id))
                    .map(|(_, metadata)| metadata.title.clone())
                    .unwrap_or_else(|| id.replace(['.', '-'], " "));
                (id, command, title)
            })
            .collect();
        if !editor.composing
            && !ime_event
            && let Some(id) = editor.capture.clone()
        {
            // Capture owns keys before any text field or background dispatcher.
            let event = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    Event::Key {
                        key,
                        modifiers,
                        pressed: true,
                        repeat: false,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, modifiers)) = event {
                ctx.input_mut(|input| {
                    input
                        .events
                        .retain(|event| !matches!(event, Event::Key { .. } | Event::Text(_)))
                });
                editor.capture = None;
                if key != Key::Escape {
                    let mut proposed = editor.draft.clone();
                    if let Some(profile) = proposed
                        .profiles
                        .iter_mut()
                        .find(|p| p.id == proposed.active)
                    {
                        profile.bindings.insert(
                            id,
                            vec![crate::keymaps::encode(
                                KeyboardShortcut::new(modifiers, key),
                                ctx.os(),
                            )],
                        );
                        match self.validate_keymaps(
                            &proposed,
                            ctx.os(),
                            cfg!(target_arch = "wasm32"),
                        ) {
                            Ok(()) => {
                                editor.draft = proposed;
                                editor.error = None;
                            }

                            Err(error) => editor.error = Some(error),
                        }
                    }
                }
            }
        }
        let mut cancel = false;
        let mut apply = false;
        let mut operation = Ok(());
        let frame = egui::Frame::popup(&ctx.global_style());
        let response = egui::Modal::new(egui::Id::unique("shortcuts"))
            .frame(frame)
            .show(ctx, |ui| {
                let available = (ctx.content_rect().size() - frame.total_margin().sum()
                    - egui::Vec2::splat(16.0)).max(egui::Vec2::ZERO);
                ui.set_width(available.x.min(720.0));
                ui.set_height(available.y.min(700.0));
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(t("Cancel")).clicked() && touch { cancel = true; }
                        if ui.add_enabled(editor.capture.is_none(), egui::Button::new(t("Apply"))).clicked() && touch {
                            apply = true;
                        }
                    });
                    ui.separator();
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("keymap_editor")
                            .min_scrolled_height(0.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.heading(t("Keyboard mappings"));
                                ui.label(format!("{}: {}", t("View"), t(match self.mode {
                                    crate::app::CenterMode::Map => "Mind Map",
                                    crate::app::CenterMode::Outliner => "Outliner",
                                })));
                                let old_active = editor.draft.active.clone();
                                let active_name = editor.draft.profile().map(|p| p.name.as_str()).unwrap_or(
                                    if editor.draft.active == XMIND { "XMind-compatible" } else { "Thinoda Standard" }
                                ).to_owned();
                                egui::ComboBox::from_id_salt("keymap_profile")
                                    .selected_text(active_name)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut editor.draft.active, STANDARD.into(), "Thinoda Standard");
                                        ui.selectable_value(&mut editor.draft.active, XMIND.into(), "XMind-compatible");
                                        for profile in &editor.draft.profiles {
                                            ui.selectable_value(&mut editor.draft.active, profile.id.clone(), &profile.name);
                                        }
                                    }).response.accessible_name(t("Keyboard profile"));
                                if !touch { editor.draft.active = old_active.clone(); }
                                if editor.draft.active != old_active {
                                    editor.capture = None;
                                    editor.confirm_delete = false;
                                    editor.error = None;
                                }
                                ui.label(t("Built-in profiles are read-only. Duplicate one to customize it."));
                                ui.label(t("XMind-compatible covers supported commands, not every XMind or system shortcut."));
                                ui.add(egui::TextEdit::singleline(&mut editor.name)
                                    .hint_text(t("Profile name"))
                                    .desired_width(ui.available_width()))
                                    .accessible_name(t("Profile name"));
                                let editable = editor.draft.profile().is_some();
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button(t("Duplicate")).clicked() && touch {
                                        operation = editor.draft.duplicate_active(&editor.name);
                                    }
                                    if ui.add_enabled(editable, egui::Button::new(t("Rename"))).clicked() && touch {
                                        operation = editor.draft.rename_active(&editor.name);
                                    }
                                    if ui.add_enabled(editable, egui::Button::new(t("Delete"))).clicked() && touch {
                                        editor.confirm_delete = true;
                                    }
                                    if ui.add_enabled(editable, egui::Button::new(t("Reset All"))).clicked() && touch
                                        && let Some(profile) = editor.draft.profiles.iter_mut().find(|p| p.id == editor.draft.active) {
                                            profile.bindings.clear();
                                            editor.capture = None;
                                        }
                                });
                                if editor.confirm_delete {
                                    ui.label(t("Delete this keyboard profile?"));
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button(t("Confirm deletion")).clicked() && touch {
                                            operation = editor.draft.delete_active();
                                            editor.confirm_delete = false;
                                            editor.capture = None;
                                        }
                                        if ui.button(t("Keep profile")).clicked() && touch {
                                            editor.confirm_delete = false;
                                        }
                                    });
                                }
                                if let Some(error) = &editor.error {
                                    ui.colored_label(ui.visuals().error_fg_color, error);
                                }
                                if editor.capture.is_some() {
                                    ui.strong(t("Press a shortcut. Escape cancels capture."));
                                }
                                ui.separator();
                                ui.add(egui::TextEdit::singleline(&mut editor.query)
                                    .hint_text(t("Search commands or shortcuts"))
                                    .desired_width(ui.available_width()))
                                    .accessible_name(t("Search mappings"));
                                let bindings = self.profile_bindings(&editor.draft, self.mode, ctx.os());
                                let original = self.profile_bindings(&editor.original, self.mode, ctx.os());
                                let mut changes = Vec::new();
                                for mode in [crate::app::CenterMode::Map, crate::app::CenterMode::Outliner] {
                                    let proposed = self.profile_bindings(&editor.draft, mode, ctx.os());
                                    let saved = self.profile_bindings(&editor.original, mode, ctx.os());
                                    for (_, command, title) in &commands {
                                        let before = crate::keymaps::binding_aliases(ctx, &saved, command, mode);
                                        let after = crate::keymaps::binding_aliases(ctx, &proposed, command, mode);
                                        if before != after {
                                            changes.push((mode, title, before, after));
                                        }
                                    }
                                }
                                ui.label(format!("{}: {}", t("Changed bindings"), changes.len()));
                                ui.label(t("Review changed bindings before Apply. Cancel leaves saved mappings unchanged."));
                                for (mode, title, before, after) in changes {
                                    let scope = t(match mode {
                                        crate::app::CenterMode::Map => "Mind Map",
                                        crate::app::CenterMode::Outliner => "Outliner",
                                    });
                                    ui.label(format!("{scope} / {title}: {} -> {}",
                                        before.as_deref().unwrap_or(t("Unassigned")),
                                        after.as_deref().unwrap_or(t("Unassigned"))));
                                }
                                ui.label(t("System clipboard and text-editing shortcuts remain owned by the focused editor."));
                                let query = editor.query.trim().to_lowercase();
                                for (id, command, title) in &commands {
                                    let hint = crate::keymaps::binding_aliases(ctx, &bindings, command, self.mode)
                                        .unwrap_or_else(|| t("Unassigned").into());
                                    if !format!("{id} {title} {hint}").to_lowercase().contains(&query) { continue; }
                                    ui.push_id(id, |ui| {
                                        ui.separator();
                                        ui.label(title);
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(&hint);
                                            if ui.add_enabled(editable, egui::Button::new(t("Assign"))).accessible_name(format!("{}: {title}", t("Assign"))).clicked() && touch {
                                                editor.capture = Some((*id).into());
                                                editor.error = None;
                                                ctx.memory_mut(|memory| {
                                                    if let Some(focused) = memory.focused() { memory.surrender_focus(focused); }
                                                });
                                            }
                                            if ui.add_enabled(editable, egui::Button::new(t("Unassign"))).accessible_name(format!("{}: {title}", t("Unassign"))).clicked() && touch
                                                && let Some(profile) = editor.draft.profiles.iter_mut().find(|p| p.id == editor.draft.active) {
                                                    profile.bindings.insert((*id).into(), vec![]);
                                                }
                                        });
                                        let before = crate::keymaps::binding_aliases(ctx, &original, command, self.mode)
                                            .unwrap_or_else(|| t("Unassigned").into());
                                        if hint != before {
                                            ui.weak(format!("{}: {before}", t("Previously")));
                                        }
                                    });
                                }
                                crate::ui::reveal_changed_focus(ui);
                            });
                    });
                });
            });
        if let Err(error) = operation {
            editor.error = Some(error);
        }
        if apply {
            match self.apply_keymap_settings(ctx, editor.draft.clone()) {
                Ok(()) => {
                    return;
                }
                Err(error) => {
                    log::warn!("Could not apply keyboard mappings: {error}");
                    editor.error = Some(error);
                }
            }
        }
        if !cancel && !(response.should_close() && touch && !editor.composing && !ime_event) {
            self.dialogs.shortcut_browser.editor = Some(editor);
        }
    }
}

#[cfg(test)]
#[path = "keymap_editor_tests.rs"]
mod tests;
