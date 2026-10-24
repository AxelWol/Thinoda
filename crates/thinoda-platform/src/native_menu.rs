//! Native menu data is independent of the application's command implementation.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    Action {
        id: u16,
        label: String,
        enabled: bool,
        checked: bool,
    },
    Submenu {
        label: String,
        enabled: bool,
        items: Vec<Self>,
    },
    Separator,
    Caption(String),
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowMenu;

mod macos_model;
pub use macos_model::{MacAccelerator, MacMenuLabels, MacMenuModel};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::ApplicationMenu;
