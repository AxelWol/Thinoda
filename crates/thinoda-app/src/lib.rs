//! Thinoda application: document management, chrome and OS integration on top of the reusable crates.

pub mod ai;
pub mod app;
pub mod brand;
pub mod doc;
mod help;
pub mod i18n;
mod labels;
mod legal;
mod map_shot;
mod printing;
mod quick_styles;
mod shortcut_browser;
pub mod startup;
mod structure_picker;
mod template_gallery;
mod theme_editor;
pub mod ui;

pub use app::{Startup, ThinodaApp, start_services};

impl eframe::App for ThinodaApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.draw(ui);
        self.run_native_print(frame, ui.ctx());
        for url in ThinodaApp::take_open_urls(ui.ctx()) {
            self.open_link(&url);
        }
    }

    fn on_exit(&mut self) {
        self.shutdown();
    }
}
