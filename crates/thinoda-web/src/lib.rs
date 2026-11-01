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
    use wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async move {
        match initialize_browser_documents().await {
            Ok(report) => start_app(report),
            Err(error) => {
                // StorageError messages are fixed texts without paths/content.
                log::error!("browser document storage initialization failed: {error}");
                if let Some(loading) = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.get_element_by_id("loading"))
                    .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    loading.set_inner_text(&format!(
                        "Thinoda could not verify browser document storage ({error}). Legacy \
                         data was retained. Retry after checking storage permissions and free \
                         space; do not clear site data to fix this error."
                    ));
                    loading.set_class_name("error");
                }
            }
        }
    });
}
