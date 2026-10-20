//! Thinoda as a static web app. Build with `trunk build --release` (see docs/WEB.md).

#[cfg(any(target_arch = "wasm32", test))]
mod cursor;

#[cfg(target_arch = "wasm32")]
mod web_main;

#[cfg(target_arch = "wasm32")]
fn main() {
    web_main::main();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "thinoda-web targets WebAssembly. Run `trunk serve` or `trunk build --release` in crates/thinoda-web."
    );
}
