#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
mod native_main;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    native_main::main()
}

/// The browser build starts from the `thinoda-web` crate instead.
#[cfg(target_arch = "wasm32")]
fn main() {}
