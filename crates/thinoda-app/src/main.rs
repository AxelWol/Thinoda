#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use thinoda_app::{ThinodaApp, doc, start_services};
use thinoda_platform::{Paths, single_instance::SingleInstance};

const HELP: &str = "Thinoda - Give your thoughts shape.

USAGE:
    thinoda [FILES...]
    thinoda --convert <INPUT> <OUTPUT> [--allow-symbol-color-loss]
                                        convert between formats (by extension)
    thinoda --version
    thinoda --help
";

fn main() -> anyhow::Result<()> {
    quiet_gpu_probing();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => {
            print!("{HELP}");
            return Ok(());
        }
        Some("--version" | "-V") => {
            println!("thinoda {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--convert") => {
            let (Some(i), Some(o)) = (args.get(1), args.get(2)) else {
                anyhow::bail!("usage: thinoda --convert <INPUT> <OUTPUT>");
            };
            doc::convert(
                &PathBuf::from(i),
                &PathBuf::from(o),
                &thinoda_io::ExportOptions {
                    allow_symbol_color_loss: args
                        .iter()
                        .skip(3)
                        .any(|a| a == "--allow-symbol-color-loss"),
                    ..Default::default()
                },
            )
            .map_err(anyhow::Error::msg)?;
            return Ok(());
        }
        _ => {}
    }
    let files: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .collect();

    let paths = Paths::new()?;
    // Hand files to an already running instance.
    let instance = SingleInstance::acquire(&paths).ok();
    if let Some(inst) = &instance
        && !inst.is_primary()
        && files.iter().all(|f| inst.forward_open(f).is_ok())
    {
        return Ok(());
    }
    let startup = start_services(paths, instance);
    let (w, h) = startup
        .settings
        .window
        .as_ref()
        .map_or((1280.0, 800.0), |w| (w.width, w.height));
    // macOS draws the Dock icon from this image even inside the bundle, so use the
    // Apple-grid variant there; other platforms use the full-bleed logo.
    #[cfg(target_os = "macos")]
    let icon_png: &[u8] = include_bytes!("../../../assets/icon-macos-512.png");
    #[cfg(not(target_os = "macos"))]
    let icon_png: &[u8] = include_bytes!("../../../assets/icon-256.png");
    let icon = eframe::icon_data::from_png_bytes(icon_png).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Thinoda")
        .with_inner_size([w, h])
        .with_min_inner_size([720.0, 480.0])
        .with_drag_and_drop(true)
        .with_app_id("thinoda");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        centered: true,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Thinoda",
        options,
        Box::new(move |cc| {
            egui_extras_install(&cc.egui_ctx);
            let mut app = ThinodaApp::new(startup.settings, startup.services, &files);
            app.dialogs.recovery = startup.orphans;
            if let Some(w) = startup.warning {
                app.toasts.warning(w);
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

fn egui_extras_install(ctx: &egui::Context) {
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    thinoda_ui::fonts::install_fallbacks(ctx);
}

/// wgpu probes every GPU backend at startup; on Linux without a native driver
/// (e.g. WSL) libEGL prints harmless warnings for the paths it rejects.
/// Silence them unless the user configured EGL logging explicitly.
fn quiet_gpu_probing() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("EGL_LOG_LEVEL").is_none() {
        // SAFETY: called first thing in main, before any thread is spawned.
        unsafe { std::env::set_var("EGL_LOG_LEVEL", "fatal") };
    }
}
