//! Browser replacements for operating-system integrations.
//!
//! * Printing renders the prepared page as an image inside a hidden `<iframe>`
//!   and opens the browser's print dialog (which also offers "Save as PDF").
//! * Saving and exporting hand the bytes to the browser as a file download.
//!
//! The HTML and filename helpers are target-independent and unit-tested natively.

use base64::Engine as _;

/// Open the web install panel; the browser prompt itself runs from a DOM click.
#[cfg(target_arch = "wasm32")]
pub fn install_app(
    title: &str,
    close: &str,
    instructions: &str,
    storage: &str,
    unavailable: &str,
) -> Result<(), String> {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(inline_js = "
        export function thinoda_install_panel(...args) {
            if (!window.thinodaPwa) throw new Error('Thinoda PWA integration is unavailable');
            window.thinodaPwa.open(...args);
        }
    ")]
    extern "C" {
        #[wasm_bindgen(catch)]
        fn thinoda_install_panel(
            title: &str,
            close: &str,
            instructions: &str,
            storage: &str,
            unavailable: &str,
        ) -> Result<(), JsValue>;
    }
    thinoda_install_panel(title, close, instructions, storage, unavailable)
        .map_err(|error| format!("{error:?}"))
}

/// Escape text for HTML element content and double-quoted attributes.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A printable HTML document showing one PNG page at its exact physical size.
/// `@page` fixes paper size and removes browser margins so the page's own
/// margins (already rendered into the image) are preserved.
pub fn print_document(title: &str, png: &[u8], size_mm: [f64; 2]) -> String {
    let [w, h] = size_mm;
    let data = base64::engine::general_purpose::STANDARD.encode(png);
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{title}</title><style>\
         @page{{size:{w:.1}mm {h:.1}mm;margin:0}}\
         html,body{{margin:0;padding:0;background:#fff}}\
         img{{display:block;width:{w:.1}mm;height:{h:.1}mm;page-break-after:avoid;break-after:avoid}}\
         </style></head><body><img alt=\"{title}\" src=\"data:image/png;base64,{data}\"></body></html>",
        title = escape_html(title),
    )
}

/// Reduce a user-supplied name to a safe download filename with `extension`.
/// Returns `None` when nothing usable remains.
pub fn sanitize_file_name(name: &str, extension: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        return None;
    }
    let extension = extension.trim_start_matches('.');
    if extension.is_empty()
        || cleaned
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(extension))
    {
        Some(cleaned.to_owned())
    } else {
        Some(format!("{cleaned}.{extension}"))
    }
}

/// MIME type for a download based on the filename extension.
pub fn mime_for(file_name: &str) -> &'static str {
    match file_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("md") => "text/markdown",
        Some("txt") => "text/plain",
        Some("opml" | "xml") => "application/xml",
        Some("html" | "htm") => "text/html",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        _ => "application/octet-stream",
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use wasm_bindgen::{JsCast, JsValue};

    fn err(e: JsValue) -> String {
        e.as_string().unwrap_or_else(|| format!("{e:?}"))
    }

    fn document() -> Result<web_sys::Document, String> {
        web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| "No browser document".to_owned())
    }

    /// Offer bytes to the user as a file download.
    pub fn download(file_name: &str, bytes: &[u8]) -> Result<(), String> {
        let array = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::of1(&array);
        let options = web_sys::BlobPropertyBag::new();
        options.set_type(super::mime_for(file_name));
        let blob =
            web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(err)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(err)?;
        let document = document()?;
        let anchor: web_sys::HtmlAnchorElement = document
            .create_element("a")
            .map_err(err)?
            .dyn_into()
            .map_err(|_| "Could not create a download link".to_owned())?;
        anchor.set_href(&url);
        anchor.set_download(file_name);
        anchor
            .style()
            .set_property("display", "none")
            .map_err(err)?;
        let body = document.body().ok_or("No document body")?;
        body.append_child(&anchor).map_err(err)?;
        anchor.click();
        anchor.remove();
        // Revoke after the browser has started the download.
        let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
            let _ = web_sys::Url::revoke_object_url(&url);
        });
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                revoke.unchecked_ref(),
                60_000,
            );
        }
        Ok(())
    }

    /// Print an HTML document through a hidden iframe and the browser print dialog.
    pub fn print_html(html: &str) -> Result<(), String> {
        let document = document()?;
        if let Some(old) = document.get_element_by_id("thinoda-print-frame") {
            old.remove();
        }
        let frame: web_sys::HtmlIFrameElement = document
            .create_element("iframe")
            .map_err(err)?
            .dyn_into()
            .map_err(|_| "Could not create a print frame".to_owned())?;
        frame.set_id("thinoda-print-frame");
        frame.set_title("Thinoda print preview");
        let style = frame.style();
        for (k, v) in [
            ("position", "fixed"),
            ("right", "0"),
            ("bottom", "0"),
            ("width", "0"),
            ("height", "0"),
            ("border", "0"),
            ("visibility", "hidden"),
        ] {
            style.set_property(k, v).map_err(err)?;
        }
        let onload = wasm_bindgen::closure::Closure::once_into_js(move || {
            if let Some(window) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("thinoda-print-frame"))
                .and_then(|e| e.dyn_into::<web_sys::HtmlIFrameElement>().ok())
                .and_then(|f| f.content_window())
            {
                let _ = window.focus();
                if let Err(e) = window.print() {
                    log::warn!("Browser print failed: {e:?}");
                }
            }
        });
        frame.set_onload(Some(onload.unchecked_ref()));
        frame.set_srcdoc(html);
        document
            .body()
            .ok_or("No document body")?
            .append_child(&frame)
            .map_err(err)?;
        Ok(())
    }

    /// Ask for a filename with the browser's prompt; `None` when cancelled.
    pub fn prompt_file_name(message: &str, default: &str) -> Option<String> {
        web_sys::window()?
            .prompt_with_message_and_default(message, default)
            .ok()
            .flatten()
    }
}

#[cfg(target_arch = "wasm32")]
pub use imp::{download, print_html, prompt_file_name};

/// Print a prepared PNG page with the browser's print dialog.
#[cfg(target_arch = "wasm32")]
pub fn print_png(title: &str, png: &[u8], size_mm: [f64; 2]) -> Result<(), String> {
    print_html(&print_document(title, png, size_mm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_document_sets_exact_paper_size_and_escapes_title() {
        let html = print_document("<Plan & \"Map\">", &[137, 80, 78, 71], [210.0, 297.0]);
        assert!(html.contains("@page{size:210.0mm 297.0mm;margin:0}"));
        assert!(html.contains("width:210.0mm;height:297.0mm"));
        assert!(html.contains("<title>&lt;Plan &amp; &quot;Map&quot;&gt;</title>"));
        assert!(html.contains("src=\"data:image/png;base64,iVBORw==\""));
        assert!(!html.contains("<Plan"));
        let landscape = print_document("x", &[], [279.4, 215.9]);
        assert!(landscape.contains("size:279.4mm 215.9mm"));
    }

    #[test]
    fn file_names_are_sanitized_and_extended() {
        assert_eq!(
            sanitize_file_name("Plan", "xmind").as_deref(),
            Some("Plan.xmind")
        );
        assert_eq!(
            sanitize_file_name("Plan.XMIND", ".xmind").as_deref(),
            Some("Plan.XMIND")
        );
        assert_eq!(
            sanitize_file_name("../dir\\a<b>:c?.png", "png").as_deref(),
            Some("abc.png")
        );
        assert_eq!(sanitize_file_name(" .. ", "png"), None);
        assert_eq!(sanitize_file_name("notes", "").as_deref(), Some("notes"));
    }

    #[test]
    fn download_mime_types_follow_extensions() {
        assert_eq!(mime_for("a.PNG"), "image/png");
        assert_eq!(mime_for("a.pdf"), "application/pdf");
        assert_eq!(mime_for("a.xmind"), "application/octet-stream");
        assert_eq!(mime_for("noext"), "application/octet-stream");
    }
}
