//! Disposable native UI acceptance host using the production desktop entry point.

#[cfg(not(target_arch = "wasm32"))]
#[path = "../src/native_main.rs"]
mod native_main;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    let home = tempfile::Builder::new()
        .prefix("thinoda-native-acceptance-")
        .tempdir()?;
    eprintln!("Disposable Thinoda home: {}", home.path().display());
    // SAFETY: this launcher is still single-threaded; native_main starts services later.
    unsafe { std::env::set_var("THINODA_HOME", home.path()) };
    native_main::main()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
