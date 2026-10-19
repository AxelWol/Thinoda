//! Browser entry point: mounts the shared Thinoda app on the page canvas.

use std::sync::{Arc, atomic::Ordering};

use eframe::wasm_bindgen::{JsCast, closure::Closure};
use thinoda_app::{ThinodaApp, UNSAVED_CHANGES, start_services};
use thinoda_platform::{Paths, recovery::RecoveryManager, single_instance::SingleInstance};

const CANVAS_ID: &str = "thinoda_canvas";

pub fn main() {
    eframe::WebLogger::init(log::LevelFilter::Warn).ok();
    thinoda_app::i18n::set_lang(thinoda_app::i18n::resolve(
        thinoda_platform::settings::Language::Auto,
    ));
    let startup = match Paths::new() {
        Ok(paths) => {
            let instance = SingleInstance::acquire(&paths).ok();
            start_services(paths, instance)
        }
        Err(e) => {
            show_error(&format!("Thinoda could not start: {e}"));
            return;
        }
    };
    install_page_handlers(startup.services.recovery.clone());

    wasm_bindgen_futures::spawn_local(async move {
        let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(CANVAS_ID))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            show_error("Thinoda could not find its drawing canvas.");
            return;
        };
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    thinoda_app::install_context(&cc.egui_ctx);
                    Ok(Box::new(ThinodaApp::launch(startup, &[])))
                }),
            )
            .await;
        match result {
            Ok(()) => hide_loading(),
            Err(e) => show_graphics_error(&format!("{e:?}")),
        }
    });
}

/// Warns before closing a tab with unsaved changes and keeps the recovery session
/// claimable across reloads: `pagehide` flushes and releases it so the next visit
/// can offer recovery, `pageshow` from the back/forward cache claims it again.
fn install_page_handlers(recovery: Option<Arc<RecoveryManager>>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let before_unload = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
        |event: web_sys::BeforeUnloadEvent| {
            if UNSAVED_CHANGES.load(Ordering::Relaxed) {
                event.prevent_default();
                event.set_return_value("unsaved");
            }
        },
    );
    window
        .add_event_listener_with_callback("beforeunload", before_unload.as_ref().unchecked_ref())
        .ok();
    before_unload.forget();

    if let Some(recovery) = recovery {
        let hide = recovery.clone();
        let page_hide = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            if let Err(e) = hide.suspend() {
                log::warn!("Could not save recovery data: {e}");
            }
        });
        window
            .add_event_listener_with_callback("pagehide", page_hide.as_ref().unchecked_ref())
            .ok();
        page_hide.forget();

        let page_show = Closure::<dyn FnMut(web_sys::PageTransitionEvent)>::new(
            move |event: web_sys::PageTransitionEvent| {
                if event.persisted()
                    && let Err(e) = recovery.resume()
                {
                    log::warn!("Could not resume recovery: {e}");
                }
            },
        );
        window
            .add_event_listener_with_callback("pageshow", page_show.as_ref().unchecked_ref())
            .ok();
        page_show.forget();
    }
}

fn element(id: &str) -> Option<web_sys::HtmlElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id(id)?
        .dyn_into::<web_sys::HtmlElement>()
        .ok()
}

fn hide_loading() {
    if let Some(loading) = element("loading") {
        loading.remove();
    }
}

/// Explains how to re-enable WebGL 2, which is the usual reason the renderer fails
/// to start; the raw error stays available for bug reports.
fn show_graphics_error(details: &str) {
    log::error!("Thinoda could not start: {details}");
    let Some(loading) = element("loading") else {
        return;
    };
    loading.set_inner_html(&graphics_error_html(details));
    loading.set_class_name("error");
}

fn graphics_error_html(details: &str) -> String {
    format!(
        r#"<h2>Thinoda could not start</h2>
<p>Thinoda needs WebGL&nbsp;2 (or WebGPU) to draw, but this browser blocked it.</p>
<ol>
<li>Turn on hardware acceleration: in Edge open <code>edge://settings/system</code>, in Chrome <code>chrome://settings/system</code>, enable <em>Use graphics acceleration when available</em> and restart the browser.</li>
<li>Check for policies that block WebGL: open <code>edge://policy</code> or <code>chrome://policy</code> and look for <code>WebGLAllowed</code> or <code>HardwareAccelerationModeEnabled</code>. If your organization set them, ask your IT department.</li>
<li>Try a private window with extensions turned off. Privacy and anti-fingerprinting extensions often block WebGL.</li>
<li>Update your graphics driver. <code>edge://gpu</code> or <code>chrome://gpu</code> shows whether WebGL&nbsp;2 is available.</li>
<li>Check whether <a href="https://get.webgl.org/webgl2/" target="_blank" rel="noopener">get.webgl.org/webgl2</a> works. If it fails too, the problem is in the browser or system, not in Thinoda.</li>
</ol>
<details><summary>Technical details</summary><pre>{}</pre></details>"#,
        escape_html(details)
    )
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn show_error(message: &str) {
    log::error!("{message}");
    if let Some(loading) = element("loading") {
        loading.set_inner_text(message);
        loading.set_class_name("error");
    }
}
