//! Desktop entry point (Windows, macOS, Linux).

use std::path::PathBuf;

use thinoda_app::{ThinodaApp, doc, i18n::t, start_services};
use thinoda_platform::{Paths, single_instance::SingleInstance};

const HELP: &str = "Thinoda - Give your thoughts shape.

USAGE:
    thinoda [FILES...]
    thinoda --convert <INPUT> <OUTPUT> [--allow-lossy] [--allow-symbol-color-loss]
    thinoda --version
    thinoda --help
";

pub fn main() -> anyhow::Result<()> {
    quiet_gpu_probing();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    thinoda_app::i18n::set_lang(thinoda_app::i18n::resolve(
        thinoda_platform::settings::Language::Auto,
    ));
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => {
            print!(
                "{HELP}\n{}\n{}\n",
                t(
                    "Only the current sheet is exported. Colors, assets, relationships, advanced layout and unsupported data may be omitted. The source workbook is unchanged."
                ),
                t(
                    "Import requires confirmation. Use --allow-lossy only if you accept omitted content."
                )
            );
            return Ok(());
        }
        Some("--version" | "-V") => {
            println!("thinoda {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--convert") => {
            let (Some(i), Some(o)) = (args.get(1), args.get(2)) else {
                anyhow::bail!(
                    "thinoda --convert <INPUT> <OUTPUT> [--allow-lossy] [--allow-symbol-color-loss]"
                );
            };
            if args.len() > 5
                || args
                    .iter()
                    .skip(3)
                    .any(|a| !matches!(a.as_str(), "--allow-lossy" | "--allow-symbol-color-loss"))
            {
                anyhow::bail!(
                    "thinoda --convert <INPUT> <OUTPUT> [--allow-lossy] [--allow-symbol-color-loss]"
                );
            }
            doc::convert_with_consent(
                &PathBuf::from(i),
                &PathBuf::from(o),
                &thinoda_io::ExportOptions {
                    allow_symbol_color_loss: args
                        .iter()
                        .skip(3)
                        .any(|a| a == "--allow-symbol-color-loss"),
                    ..Default::default()
                },
                args.iter().skip(3).any(|a| a == "--allow-lossy"),
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
            thinoda_app::install_context(&cc.egui_ctx);
            Ok(Box::new(ThinodaApp::launch(startup, &files)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
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
