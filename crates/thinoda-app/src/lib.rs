//! Thinoda application: document management, chrome and OS integration on top of the reusable crates.

pub mod ai;
pub mod app;
mod attachments;
pub mod brand;
mod command_icons;
mod command_ids;
pub mod doc;
mod equations;
pub mod file_ops;
pub mod files;
mod fim;
mod fim_schemas;
mod gantt;
mod help;
pub mod i18n;
mod images;
mod keymap_editor;
pub mod keymaps;
mod labels;
mod legal;
mod legal_markdown;
mod map_shot;
pub mod menus;
pub mod native_menus;
mod navigation;
mod notes;
mod pitch;
mod planning;
mod powerpoint_preview;
mod printing;
mod quick_styles;
#[cfg(any(target_os = "windows", target_os = "macos", test))]
mod ribbon;
mod save_all;
mod shortcut_browser;
pub mod shortcut_conflicts;
pub mod startup;
mod structure_picker;
mod symbols;
mod template_gallery;
mod theme_editor;
mod tree_table;
pub mod ui;
mod updates;
mod zones;

pub use app::{Startup, ThinodaApp, start_services};

/// Shared egui context setup for the desktop and web entry points.
pub fn install_context(ctx: &egui::Context) {
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    thinoda_ui::fonts::install_fallbacks(ctx);
}

/// Whether any open document has unsaved changes, updated every frame. The web
/// shell reads it in its `beforeunload` handler to warn before the tab closes.
pub static UNSAVED_CHANGES: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

impl eframe::App for ThinodaApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Production lifecycle only: headless draw tests must never start live networking.
        self.updates.poll(ui.ctx(), self.services.paths.is_some());
        #[cfg(target_os = "windows")]
        {
            self.desktop_ribbon = true;
        }
        #[cfg(target_os = "macos")]
        let root_native_menu_focused = ui
            .ctx()
            .input(|input| input.viewport().focused.unwrap_or(true));
        #[cfg(target_os = "macos")]
        if root_native_menu_focused {
            self.poll_native_menus(ui.ctx());
        }
        self.draw(ui);
        #[cfg(target_os = "macos")]
        if root_native_menu_focused {
            self.sync_native_menus(ui.ctx());
        }
        self.run_native_print(frame, ui.ctx());
        for url in ThinodaApp::take_open_urls(ui.ctx()) {
            self.open_link(&url);
        }
        UNSAVED_CHANGES.store(self.any_dirty(), std::sync::atomic::Ordering::Relaxed);
    }

    fn on_exit(&mut self) {
        self.shutdown();
    }
}
