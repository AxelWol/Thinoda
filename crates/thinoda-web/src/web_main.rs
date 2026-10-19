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
            Err(e) => show_error(&format!(
                "Thinoda could not start. WebGL may be unavailable in this browser. ({e:?})"
            )),
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

fn show_error(message: &str) {
    log::error!("{message}");
    if let Some(loading) = element("loading") {
        loading.set_inner_text(message);
        loading.set_class_name("error");
    }
}
