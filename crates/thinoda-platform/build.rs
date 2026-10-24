fn main() {
    println!("cargo:rerun-if-changed=native");
    // Browsers print through their own dialog (see src/web.rs); no native bridge.
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }
    let target = std::env::var("CARGO_CFG_TARGET_OS").expect("target OS");
    let mut build = cc::Build::new();
    match target.as_str() {
        "windows" => {
            build
                .cpp(true)
                .std("c++20")
                .file("native/print_windows.cpp")
                .flag_if_supported("/EHsc");
            for library in [
                "windowsapp",
                "d3d11",
                "d2d1",
                "windowscodecs",
                "ole32",
                "user32",
            ] {
                println!("cargo:rustc-link-lib={library}");
            }
        }
        "macos" => {
            build
                .file("native/print_macos.m")
                .file("native/menu_macos.m")
                .flag("-fobjc-arc")
                .flag("-mmacosx-version-min=11.0");
            println!("cargo:rustc-link-lib=framework=AppKit");
            println!("cargo:rustc-link-lib=framework=ApplicationServices");
        }
        "linux" => {
            for path in pkg_config::Config::new()
                .atleast_version("3.22")
                // Runtime GTK is loaded as one host stack, including its printer
                // modules. Do not bundle a partial GTK into portable AppImages.
                .cargo_metadata(false)
                .probe("gtk+-3.0")
                .expect("native printing requires GTK 3 development libraries (libgtk-3-dev)")
                .include_paths
            {
                build.include(path);
            }
            build.file("native/print_linux.c");
            println!("cargo:rustc-link-lib=dl");
            println!("cargo:rustc-link-lib=m");
        }
        _ => panic!("native printing is supported on Windows, macOS and Linux"),
    }
    build.compile("thinoda_native_print");
}
