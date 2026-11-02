//! Fetch with redirects rejected and ambient cookies omitted. Reqwest's wasm
//! backend currently cannot configure those redirect semantics.
use super::*;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;

struct AbortGuard {
    controller: web_sys::AbortController,
    window: web_sys::Window,
    timer: i32,
    _callback: Closure<dyn FnMut()>,
}

impl Drop for AbortGuard {
    fn drop(&mut self) {
        self.window.clear_interval_with_handle(self.timer);
        self.controller.abort();
    }
}

pub(super) async fn send(
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: Vec<u8>,
    cancel: &CancelToken,
    limit: usize,
) -> Result<Response> {
    cancel.check()?;
    let window = web_sys::window().ok_or(StorageError::Unsupported)?;
    let controller =
        web_sys::AbortController::new().map_err(|_| remote("Fetch initialization failed"))?;
    let token = cancel.clone();
    let abort = controller.clone();
    let started = web_time::Instant::now();
    let callback = Closure::<dyn FnMut()>::new(move || {
        if token.is_cancelled() || started.elapsed().as_secs() >= 120 {
            abort.abort();
        }
    });
    let timer = window
        .set_interval_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            25,
        )
        .map_err(|_| remote("Fetch timer initialization failed"))?;
    let guard = AbortGuard {
        controller,
        window,
        timer,
        _callback: callback,
    };
    let init = web_sys::RequestInit::new();
    init.set_method(method.as_str());
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    init.set_redirect(web_sys::RequestRedirect::Error);
    init.set_signal(Some(&guard.controller.signal()));
    let browser_headers = web_sys::Headers::new().map_err(|_| remote("Fetch headers failed"))?;
    for (name, value) in &headers {
        // Browser Fetch calculates these forbidden headers itself.
        if name.as_str() == "content-length" {
            continue;
        }
        browser_headers
            .set(
                name.as_str(),
                value.to_str().map_err(|_| remote("invalid Fetch header"))?,
            )
            .map_err(|_| remote("Fetch header rejected"))?;
    }
    init.set_headers(browser_headers.as_ref());
    if method != Method::GET && method != Method::HEAD {
        let bytes = js_sys::Uint8Array::from(body.as_slice());
        init.set_body(bytes.as_ref());
    }
    let request = web_sys::Request::new_with_str_and_init(url.as_str(), &init)
        .map_err(|_| remote("Fetch request initialization failed"))?;
    let fetched = JsFuture::from(guard.window.fetch_with_request(&request)).await;
    cancel.check()?;
    let response: web_sys::Response = fetched
        .map_err(|_| remote("Fetch failed (network, CORS, redirect or timeout)"))?
        .dyn_into()
        .map_err(|_| remote("invalid Fetch response"))?;
    if response.redirected() {
        return Err(remote("Fetch unexpectedly followed redirect"));
    }
    let status = response.status();
    let mut headers = HeaderMap::new();
    for name in ["etag", "location", "dropbox-api-result"] {
        if let Some(value) = response
            .headers()
            .get(name)
            .map_err(|_| remote("Fetch response headers failed"))?
        {
            headers.insert(
                reqwest::header::HeaderName::from_static(name),
                HeaderValue::from_str(&value).map_err(|_| remote("invalid response header"))?,
            );
        }
    }
    let mut bytes = Vec::new();
    if (200..300).contains(&status)
        && let Some(stream) = response.body()
    {
        let reader: web_sys::ReadableStreamDefaultReader = stream
            .get_reader()
            .dyn_into()
            .map_err(|_| remote("Fetch stream initialization failed"))?;
        loop {
            cancel.check()?;
            let chunk = JsFuture::from(reader.read()).await;
            cancel.check()?;
            let chunk = chunk.map_err(|_| remote("Fetch body failed"))?;
            if js_sys::Reflect::get(&chunk, &JsValue::from_str("done"))
                .map_err(|_| remote("invalid Fetch chunk"))?
                .as_bool()
                == Some(true)
            {
                break;
            }
            let value = js_sys::Reflect::get(&chunk, &JsValue::from_str("value"))
                .map_err(|_| remote("invalid Fetch chunk"))?;
            let chunk = value
                .dyn_into::<js_sys::Uint8Array>()
                .map_err(|_| remote("invalid Fetch byte chunk"))?;
            if chunk.length() as usize > limit.saturating_sub(bytes.len()) {
                return Err(remote("response exceeds configured limit"));
            }
            let offset = bytes.len();
            bytes.resize(offset + chunk.length() as usize, 0);
            chunk.copy_to(&mut bytes[offset..]);
        }
        reader.release_lock();
    }
    cancel.check()?;
    Ok(Response {
        status,
        headers,
        bytes,
    })
}
