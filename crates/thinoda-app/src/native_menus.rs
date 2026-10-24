//! Snapshot and deferred dispatch shared by native menu adapters.

use crate::{
    app::{CenterMode, Cmd, ThinodaApp},
    i18n::t,
    menus::MenuEntry,
};
use egui::{Context, Event, Key, Modifiers};
use thinoda_canvas::Action;
use thinoda_platform::native_menu::MenuItem;

#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    Command(Cmd),
    ClearRecent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuSnapshot {
    pub items: Vec<MenuItem>,
    pub actions: Vec<MenuAction>,
    owner: Option<MenuOwner>,
    mode: CenterMode,
    text_focus: Option<egui::Id>,
}

#[derive(Debug, Clone, PartialEq)]
struct MenuOwner {
    document: String,
    sheet: usize,
    revision: u64,
    selection: Vec<thinoda_core::TopicId>,
    object: Option<thinoda_canvas::ObjectSelection>,
}

#[cfg(any(target_os = "windows", target_os = "macos", test))]
impl MenuSnapshot {
    pub(crate) fn has_text_focus(&self) -> bool {
        self.text_focus.is_some()
    }

    pub(crate) fn command_id(&self, command: &Cmd) -> Option<u16> {
        self.actions
            .iter()
            .position(|action| {
                matches!(action,
            MenuAction::Command(candidate) if candidate == command)
            })
            .and_then(|index| u16::try_from(index + 1).ok())
    }

    pub(crate) fn command_enabled(&self, command: &Cmd) -> bool {
        fn find(items: &[MenuItem], target: u16) -> Option<bool> {
            items.iter().find_map(|item| match item {
                MenuItem::Action { id, enabled, .. } if *id == target => Some(*enabled),
                MenuItem::Submenu { items, .. } => find(items, target),
                _ => None,
            })
        }
        self.command_id(command)
            .and_then(|id| find(&self.items, id))
            .unwrap_or(false)
    }

    pub(crate) fn command_checked(&self, command: &Cmd) -> bool {
        fn find(items: &[MenuItem], target: u16) -> bool {
            items.iter().any(|item| match item {
                MenuItem::Action { id, checked, .. } if *id == target => *checked,
                MenuItem::Submenu { items, .. } => find(items, target),
                _ => false,
            })
        }
        self.command_id(command)
            .is_some_and(|id| find(&self.items, id))
    }
}

fn focused_text(ctx: &Context) -> Option<egui::Id> {
    ctx.text_edit_focused()
        .then(|| ctx.memory(|memory| memory.focused()))
        .flatten()
}

fn text_command(command: &Cmd) -> Option<(Key, Modifiers)> {
    if matches!(command, Cmd::Canvas(Action::Delete)) {
        return Some((Key::Delete, Modifiers::NONE));
    }
    let (key, shift) = match command {
        Cmd::Canvas(Action::Undo) => (Key::Z, false),
        Cmd::Canvas(Action::Redo) => (Key::Z, true),
        Cmd::Canvas(Action::SelectAll) => (Key::A, false),
        _ => return None,
    };
    Some((
        key,
        if shift {
            Modifiers::COMMAND.plus(Modifiers::SHIFT)
        } else {
            Modifiers::COMMAND
        },
    ))
}

fn is_text_command(command: &Cmd) -> bool {
    text_command(command).is_some()
        || matches!(
            command,
            Cmd::Canvas(Action::Copy | Action::Cut) | Cmd::MenuPaste
        )
}

impl ThinodaApp {
    #[cfg(any(target_os = "windows", target_os = "macos", test))]
    pub(crate) fn dispatch_ribbon_command(
        &mut self,
        ctx: &Context,
        snapshot: &MenuSnapshot,
        command: &Cmd,
    ) {
        if self.native_menu_owner() == snapshot.owner
            && self.mode == snapshot.mode
            && is_text_command(command)
            && focused_text(ctx).is_none_or(|focus| Some(focus) == snapshot.text_focus)
            && let Some(field) = snapshot.text_focus
        {
            ctx.memory_mut(|memory| memory.request_focus(field));
        }
        if let Some(id) = snapshot.command_id(command) {
            self.dispatch_native_menu(ctx, snapshot, id);
        }
    }

    fn native_menu_owner(&self) -> Option<MenuOwner> {
        self.doc().map(|doc| MenuOwner {
            document: doc.key.clone(),
            sheet: doc.doc.active_index(),
            revision: doc.doc.state_id(),
            selection: doc.view.selection.clone(),
            object: doc.view.selected_object,
        })
    }

    pub fn native_menu_snapshot(&self, ctx: &Context) -> MenuSnapshot {
        let text_focus = ctx.text_edit_focused();
        let blocked = self.launch.is_some()
            || self.any_modal_open()
            || self.dialogs.settings
            || self.dialogs.about
            || self.palette.open;
        let mut actions = Vec::new();
        fn convert(
            app: &ThinodaApp,
            ctx: &Context,
            entries: &[MenuEntry],
            blocked: bool,
            text_focus: bool,
            actions: &mut Vec<MenuAction>,
        ) -> Vec<MenuItem> {
            entries
                .iter()
                .map(|entry| match entry {
                    MenuEntry::Command { label, command } => {
                        actions.push(MenuAction::Command(command.clone()));
                        let hint = ThinodaApp::shortcut_hint_for_mode(ctx, command, app.mode);
                        let label = if cfg!(target_os = "macos") {
                            label.clone()
                        } else {
                            hint.map_or_else(|| label.clone(), |hint| format!("{label}\t{hint}"))
                        };
                        let enabled = if text_focus && is_text_command(command) {
                            app.launch.is_none()
                        } else {
                            !blocked && app.command_available(command)
                        };
                        let checked = match command {
                            Cmd::ToggleOutliner => app.mode == CenterMode::Outliner,
                            Cmd::ToggleZen => app.zen,
                            Cmd::ToggleFormatPanel => app.settings.show_format_panel,
                            Cmd::ToggleSymbolsPanel => {
                                app.settings.show_format_panel
                                    && app.sidebar_family == crate::app::SidebarFamily::Symbols
                            }
                            Cmd::ToggleStatusBar => app.settings.show_status_bar,
                            _ => false,
                        };
                        MenuItem::Action {
                            id: actions.len() as u16,
                            label,
                            enabled,
                            checked,
                        }
                    }
                    MenuEntry::Submenu { label, entries } => MenuItem::Submenu {
                        label: label.clone(),
                        enabled: app.menu_entries_available(entries),
                        items: convert(app, ctx, entries, blocked, text_focus, actions),
                    },
                    MenuEntry::Separator => MenuItem::Separator,
                    MenuEntry::Caption(text) => MenuItem::Caption(text.clone()),
                    MenuEntry::ClearRecent => {
                        actions.push(MenuAction::ClearRecent);
                        MenuItem::Action {
                            id: actions.len() as u16,
                            label: t("Clear Recent").into(),
                            enabled: !blocked,
                            checked: false,
                        }
                    }
                })
                .collect()
        }
        let items = convert(
            self,
            ctx,
            &self.main_menu_entries(),
            blocked,
            text_focus,
            &mut actions,
        );
        MenuSnapshot {
            items,
            actions,
            text_focus: focused_text(ctx),
            mode: self.mode,
            owner: self.native_menu_owner(),
        }
    }

    pub fn dispatch_native_menu(&mut self, ctx: &Context, snapshot: &MenuSnapshot, id: u16) {
        let Some(action) = id
            .checked_sub(1)
            .and_then(|id| snapshot.actions.get(usize::from(id)))
        else {
            log::warn!("unknown native menu command: {id}");
            return;
        };
        if self.native_menu_owner() != snapshot.owner
            || self.mode != snapshot.mode
            || snapshot.text_focus != focused_text(ctx)
        {
            self.toasts.warning(t("The document changed. Try again."));
            return;
        }
        if let MenuAction::Command(command) = action
            && snapshot.text_focus.is_some()
            && ctx.text_edit_focused()
            && is_text_command(command)
        {
            match command {
                Cmd::Canvas(Action::Copy) => ctx.input_mut(|input| input.events.push(Event::Copy)),
                Cmd::Canvas(Action::Cut) => ctx.input_mut(|input| input.events.push(Event::Cut)),
                Cmd::MenuPaste => ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste),
                _ => {
                    if let Some((key, modifiers)) = text_command(command) {
                        ctx.input_mut(|input| {
                            input.events.push(Event::Key {
                                key,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers,
                            })
                        });
                    }
                }
            }
            return;
        }
        if self.launch.is_some()
            || self.any_modal_open()
            || self.dialogs.settings
            || self.dialogs.about
            || self.palette.open
        {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        match action {
            MenuAction::Command(command) => self.exec(ctx, command.clone()),
            MenuAction::ClearRecent => {
                self.settings.clear_recent();
                self.save_settings();
            }
        }
    }
}

#[cfg(target_os = "windows")]
type HostMenu = thinoda_platform::native_menu::WindowMenu;
#[cfg(target_os = "macos")]
type HostMenu = thinoda_platform::native_menu::ApplicationMenu;

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[derive(Default)]
pub(crate) struct NativeMenus {
    active: Option<(HostMenu, MenuSnapshot)>,
    unavailable: bool,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl ThinodaApp {
    pub(crate) fn native_menu_active(&self) -> bool {
        self.native_menus.active.is_some()
    }

    pub(crate) fn poll_native_menus(&mut self, ctx: &Context) {
        let pending = self
            .native_menus
            .active
            .as_ref()
            .map(|(menu, snapshot)| (menu.drain(), snapshot.clone()));
        if let Some((ids, snapshot)) = pending {
            for id in ids {
                self.dispatch_native_menu(ctx, &snapshot, id);
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl ThinodaApp {
    pub(crate) fn sync_native_menus(&mut self, ctx: &Context, frame: &eframe::Frame) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if self.native_menus.unavailable || self.launch.is_some() {
            return;
        }
        let snapshot = self.native_menu_snapshot(ctx);
        if let Some((_, old)) = &self.native_menus.active
            && old == &snapshot
        {
            return;
        }
        let result = if let Some((menu, old)) = &mut self.native_menus.active {
            if menu.is_tracking() {
                return;
            }
            if old.items == snapshot.items && old.actions == snapshot.actions {
                *old = snapshot;
                return;
            }
            menu.replace(&snapshot.items)
                .map(|()| *old = snapshot.clone())
        } else {
            (|| {
                let handle = frame.window_handle().map_err(std::io::Error::other)?;
                let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                    return Err(std::io::Error::other("Expected a Win32 window"));
                };
                let wake = ctx.clone();
                // SAFETY: eframe invokes this on the owning window thread.
                let menu = unsafe {
                    thinoda_platform::native_menu::WindowMenu::attach(
                        handle.hwnd.get() as _,
                        &snapshot.items,
                        move || wake.request_repaint(),
                    )
                }?;
                self.native_menus.active = Some((menu, snapshot));
                Ok(())
            })()
        };
        if let Err(error) = result {
            log::error!("native menu integration: {error}");
            self.toasts
                .error(format!("{}: {error}", t("Not possible here")));
            self.native_menus.active = None;
            self.native_menus.unavailable = true;
        }
    }
}

#[cfg(any(target_os = "macos", test))]
impl ThinodaApp {
    fn macos_menu_model(
        &self,
        snapshot: &MenuSnapshot,
    ) -> std::io::Result<thinoda_platform::native_menu::MacMenuModel> {
        use thinoda_platform::native_menu::{MacAccelerator, MacMenuLabels, MacMenuModel};
        let command_id = |command| {
            snapshot
                .command_id(&command)
                .ok_or_else(|| std::io::Error::other("Missing standard application menu command"))
        };
        let shortcuts = snapshot
            .actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                let MenuAction::Command(command) = action else {
                    return None;
                };
                let binding = Self::shortcut_binding_for_platform(
                    command,
                    self.mode,
                    egui::os::OperatingSystem::Mac,
                )?;
                Some((
                    u16::try_from(index + 1).ok()?,
                    MacAccelerator {
                        key: binding.logical_key.name().into(),
                        command: binding.modifiers.command || binding.modifiers.mac_cmd,
                        control: binding.modifiers.ctrl,
                        alt: binding.modifiers.alt,
                        shift: binding.modifiers.shift,
                    },
                ))
            })
            .collect();
        Ok(MacMenuModel {
            items: snapshot.items.clone(),
            application_commands: [
                command_id(Cmd::About)?,
                command_id(Cmd::Settings)?,
                command_id(Cmd::Quit)?,
            ],
            shortcuts,
            labels: MacMenuLabels {
                application: "Thinoda".into(),
                services: t("Services").into(),
                window: t("Window").into(),
                hide: t("Hide Thinoda").into(),
                hide_others: t("Hide Others").into(),
                show_all: t("Show All").into(),
                minimize: t("Minimize").into(),
                zoom: t("Zoom").into(),
                bring_all: t("Bring All to Front").into(),
            },
        })
    }
}

#[cfg(target_os = "macos")]
impl ThinodaApp {
    pub(crate) fn sync_native_menus(&mut self, ctx: &Context, _: &eframe::Frame) {
        if self.native_menus.unavailable || self.launch.is_some() {
            return;
        }
        let snapshot = self.native_menu_snapshot(ctx);
        if self
            .native_menus
            .active
            .as_ref()
            .is_some_and(|(_, old)| *old == snapshot)
        {
            return;
        }
        let result = (|| {
            let model = self.macos_menu_model(&snapshot)?;
            if let Some((menu, old)) = &mut self.native_menus.active {
                if menu.replace(&model)? {
                    *old = snapshot;
                }
            } else {
                let wake = ctx.clone();
                let menu =
                    thinoda_platform::native_menu::ApplicationMenu::attach(&model, move || {
                        wake.request_repaint()
                    })?;
                self.native_menus.active = Some((menu, snapshot));
            }
            Ok::<(), std::io::Error>(())
        })();
        if let Err(error) = result {
            log::error!("native macOS menu integration: {error}");
            self.toasts
                .error(format!("{}: {error}", t("Not possible here")));
            self.native_menus.active = None;
            self.native_menus.unavailable = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_application_aliases_and_accelerators_use_shared_command_bindings() {
        let app = ThinodaApp::headless();
        let snapshot = app.native_menu_snapshot(&Context::default());
        let model = app.macos_menu_model(&snapshot).unwrap();
        for (index, command) in [Cmd::About, Cmd::Settings, Cmd::Quit]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                model.application_commands[index],
                snapshot.command_id(&command).unwrap()
            );
        }
        let save = &model.shortcuts[&snapshot.command_id(&Cmd::Save).unwrap()];
        assert!(save.command);
        assert!(!save.control);
        assert_eq!(save.key, "S");
        assert!(snapshot.actions.contains(&MenuAction::Command(Cmd::Quit)));
    }

    fn id(snapshot: &MenuSnapshot, command: Cmd) -> u16 {
        u16::try_from(
            snapshot
                .actions
                .iter()
                .position(|action| *action == MenuAction::Command(command.clone()))
                .unwrap()
                + 1,
        )
        .unwrap()
    }

    fn enabled(items: &[MenuItem], id: u16) -> Option<bool> {
        items.iter().find_map(|item| match item {
            MenuItem::Action {
                id: found, enabled, ..
            } if id == *found => Some(*enabled),
            MenuItem::Submenu { items, .. } => enabled(items, id),
            _ => None,
        })
    }

    #[test]
    fn native_and_compact_menus_share_commands_and_edit_settings() {
        let mut app = ThinodaApp::headless();
        app.docs.clear();
        let groups = app.main_menu_entries();
        assert_eq!(groups.len(), 8);
        let MenuEntry::Submenu { entries, .. } = &groups[1] else {
            panic!("Edit group")
        };
        assert!(entries.iter().any(|item| matches!(
            item,
            MenuEntry::Command {
                command: Cmd::Settings,
                ..
            }
        )));
        let snapshot = app.native_menu_snapshot(&Context::default());
        assert_eq!(snapshot.items.len(), groups.len());
        fn submenu_enabled(items: &[MenuItem], target: &str) -> Option<bool> {
            items.iter().find_map(|item| match item {
                MenuItem::Submenu { label, enabled, .. } if label == target => Some(*enabled),
                MenuItem::Submenu { items, .. } => submenu_enabled(items, target),
                _ => None,
            })
        }
        assert_eq!(submenu_enabled(&snapshot.items, t("Export")), Some(false));
        assert_eq!(submenu_enabled(&snapshot.items, t("Arrange")), Some(true));
        let mut ids = Vec::new();
        fn collect(items: &[MenuItem], ids: &mut Vec<u16>) {
            for item in items {
                match item {
                    MenuItem::Action { id, .. } => ids.push(*id),
                    MenuItem::Submenu { items, .. } => collect(items, ids),
                    _ => {}
                }
            }
        }
        collect(&snapshot.items, &mut ids);
        assert_eq!(ids, (1..=snapshot.actions.len() as u16).collect::<Vec<_>>());
        for format in crate::app::EXPORT_MENU {
            assert!(
                snapshot
                    .actions
                    .contains(&MenuAction::Command(Cmd::Export(*format)))
            );
        }
        assert_eq!(
            enabled(&snapshot.items, id(&snapshot, Cmd::Save)),
            Some(false)
        );
        assert_eq!(
            enabled(&snapshot.items, id(&snapshot, Cmd::New)),
            Some(true)
        );
    }

    #[test]
    fn stale_document_and_modal_menu_events_cannot_execute() {
        let ctx = Context::default();
        let mut app = ThinodaApp::headless();
        app.exec(&ctx, Cmd::New);
        let snapshot = app.native_menu_snapshot(&ctx);
        let new_id = id(&snapshot, Cmd::New);
        app.exec(&ctx, Cmd::New);
        let count = app.docs.len();
        app.dispatch_native_menu(&ctx, &snapshot, new_id);
        assert_eq!(app.docs.len(), count);
        let snapshot = app.native_menu_snapshot(&ctx);
        app.dialogs.settings = true;
        app.dispatch_native_menu(&ctx, &snapshot, id(&snapshot, Cmd::New));
        assert_eq!(app.docs.len(), count);
        let disabled = app.native_menu_snapshot(&ctx);
        assert_eq!(
            enabled(&disabled.items, id(&disabled, Cmd::New)),
            Some(false)
        );
        app.dialogs.settings = false;
        let snapshot = app.native_menu_snapshot(&ctx);
        app.dispatch_native_menu(&ctx, &snapshot, id(&snapshot, Cmd::New));
        assert_eq!(app.docs.len(), count + 1);
    }

    #[test]
    fn selection_history_sheet_and_view_changes_invalidate_native_targets() {
        let ctx = Context::default();
        for change in [
            Cmd::Canvas(Action::InsertChild),
            Cmd::NewSheet,
            Cmd::ToggleOutliner,
        ] {
            let mut app = ThinodaApp::headless();
            app.exec(&ctx, Cmd::New);
            let snapshot = app.native_menu_snapshot(&ctx);
            app.exec(&ctx, change);
            let count = app.docs.len();
            app.dispatch_native_menu(&ctx, &snapshot, id(&snapshot, Cmd::New));
            assert_eq!(app.docs.len(), count);
        }
        let mut app = ThinodaApp::headless();
        let snapshot = app.native_menu_snapshot(&ctx);
        app.doc_mut().unwrap().view.selection.clear();
        let count = app.docs.len();
        app.dispatch_native_menu(&ctx, &snapshot, id(&snapshot, Cmd::New));
        assert_eq!(app.docs.len(), count);
    }

    #[test]
    fn native_select_all_targets_focused_text_not_map() {
        struct State {
            app: ThinodaApp,
            text: String,
            pending: Option<(MenuSnapshot, u16)>,
        }
        let mut app = ThinodaApp::headless();
        app.exec(&Context::default(), Cmd::New);
        let before = app.doc().unwrap().doc.workbook().clone();
        let field = egui::Id::unique("native-menu-text");
        let mut harness = egui_kittest::Harness::builder().build_ui_state(
            move |ui, state: &mut State| {
                if let Some((snapshot, command)) = state.pending.take() {
                    state.app.dispatch_native_menu(ui.ctx(), &snapshot, command);
                }
                let label = ui.label("Menu text");
                ui.add(egui::TextEdit::singleline(&mut state.text).id(field))
                    .labelled_by(label.id);
                ui.memory_mut(|memory| memory.request_focus(field));
            },
            State {
                app,
                text: "Original text".into(),
                pending: None,
            },
        );
        harness.run_steps(3);
        assert!(harness.ctx.text_edit_focused());
        let snapshot = harness.state().app.native_menu_snapshot(&harness.ctx);
        let command = id(&snapshot, Cmd::Canvas(Action::SelectAll));
        harness.state_mut().pending = Some((snapshot, command));
        harness.run_steps(2);
        harness.event(Event::Text("Replacement".into()));
        harness.run_steps(2);
        assert_eq!(harness.state().text, "Replacement");
        for (action, expected) in [
            (Action::Undo, "Original text"),
            (Action::Redo, "Replacement"),
        ] {
            let snapshot = harness.state().app.native_menu_snapshot(&harness.ctx);
            let command = id(&snapshot, Cmd::Canvas(action));
            harness.state_mut().pending = Some((snapshot, command));
            harness.run_steps(2);
            assert_eq!(harness.state().text, expected);
        }
        for action in [Action::SelectAll, Action::Delete] {
            let snapshot = harness.state().app.native_menu_snapshot(&harness.ctx);
            let command = id(&snapshot, Cmd::Canvas(action));
            harness.state_mut().pending = Some((snapshot, command));
            harness.run_steps(2);
        }
        assert!(harness.state().text.is_empty());
        assert_eq!(harness.state().app.doc().unwrap().doc.workbook(), &before);
    }
}
