#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use ymind_app::{YMindApp, doc, start_services};
use ymind_platform::{Paths, single_instance::SingleInstance};

const HELP: &str = "yMind - mind mapping

USAGE:
    ymind [FILES...]
    ymind --convert <INPUT> <OUTPUT>   convert between formats (by extension)
    ymind --version
    ymind --help
";

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => {
            print!("{HELP}");
            return Ok(());
        }
        Some("--version" | "-V") => {
            println!("ymind {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--convert") => {
            let (Some(i), Some(o)) = (args.get(1), args.get(2)) else {
                anyhow::bail!("usage: ymind --convert <INPUT> <OUTPUT>");
            };
            doc::convert(
                &PathBuf::from(i),
                &PathBuf::from(o),
                &ymind_io::ExportOptions::default(),
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
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon-256.png")).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("yMind")
        .with_inner_size([w, h])
        .with_min_inner_size([720.0, 480.0])
        .with_drag_and_drop(true)
        .with_app_id("ymind");
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
        "yMind",
        options,
        Box::new(move |cc| {
            egui_extras_install(&cc.egui_ctx);
            let mut app = YMindApp::new(startup.settings, startup.services, &files);
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
    ymind_ui::fonts::install_fallbacks(ctx);
}
