//! Browser startup gate. The binary must call this before `web_main::main`.
//! Calling that existing startup function from the callback preserves its
//! synchronous settings/recovery VFS and all page lifecycle handlers.

#[cfg(target_arch = "wasm32")]
pub async fn initialize_browser_documents() -> Result<
    thinoda_platform::browser_storage::MigrationReport,
    thinoda_platform::storage::StorageError,
> {
    thinoda_platform::browser_storage::initialize_documents().await
}

/// Migration must complete before the callback creates paths, starts services,
/// loads recents or launches the app. No persistence/picker consent at startup.
///
/// Integration in `src/main.rs`:
/// `thinoda_web::start_after_storage_ready(|report| { thinoda_app::files::record_browser_migration(report); web_main::main() });`
///
/// A failure leaves the loading element as a text-only error and does not run
/// the callback. Retained localStorage can then be recovered without silently
/// accepting non-durable document saves.
#[cfg(target_arch = "wasm32")]
pub fn start_after_storage_ready(
    start_app: impl FnOnce(thinoda_platform::browser_storage::MigrationReport) + 'static,
) {
    set_loading_phase("Preparing browser documents…");
    wasm_bindgen_futures::spawn_local(async move {
        match initialize_browser_documents().await {
            Ok(report) => {
                set_loading_phase("Starting Thinoda…");
                start_app(report);
            }
            Err(error) => {
                // StorageError messages are fixed texts without paths/content.
                log::error!("browser document storage initialization failed: {error}");
                show_loading_error(|loading| {
                    loading.set_inner_text(&format!(
                        "Thinoda could not verify browser document storage ({error}). Thinoda \
                         did not delete legacy data. Retry after checking storage permissions and free \
                         space; do not clear site data to fix this error."
                    ));
                });
            }
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn set_loading_phase(message: &str) {
    if let Some(status) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("loading-message"))
    {
        status.set_text_content(Some(message));
    }
}

#[cfg(target_arch = "wasm32")]
pub fn show_loading_error(render: impl FnOnce(&web_sys::HtmlElement)) {
    use wasm_bindgen::JsCast as _;

    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(loading) = document
        .get_element_by_id("loading")
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };

    loading.set_inner_text("");
    loading.set_class_name("error");
    for (name, value) in [("role", "alert"), ("aria-live", "assertive")] {
        if let Err(error) = loading.set_attribute(name, value) {
            log::warn!("could not announce browser startup error: {error:?}");
        }
    }
    render(&loading);
    let Ok(retry) = document.create_element("a") else {
        log::warn!("could not create browser startup retry link");
        return;
    };
    if let Err(error) = retry.set_attribute("href", "") {
        log::warn!("could not configure browser startup retry link: {error:?}");
        return;
    }
    retry.set_text_content(Some("Reload Thinoda"));
    if let Err(error) = loading.append_child(&retry) {
        log::warn!("could not add browser startup retry link: {error:?}");
    }
}
