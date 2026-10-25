//! Bounded OpenAI-compatible server-sent event streaming.

use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};

use serde_json::Value;

use crate::{
    AiConfig, AiError, AiResult, AiTask, Message, Result, configured_key, finish, join_url,
    parse_models, pick_model,
};

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 64 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_STREAM_EVENTS: usize = 4096;
const MAX_PENDING_EVENTS: usize = 4096;

/// One increment of preview text or the request's terminal result.
#[derive(Debug, PartialEq, Eq)]
pub enum AiStreamEvent<T = AiResult> {
    /// Newly generated assistant text; append it to the current preview.
    Delta(String),
    /// Exactly one terminal event is emitted for each job.
    Finished(Result<T>),
}

/// A cancellable streaming completion. Poll regularly to receive preview deltas.
///
/// Undelivered deltas are discarded when cancelled. Native requests are aborted,
/// and browser requests use `AbortController`. Responses are limited to 2 MiB,
/// individual SSE events to 64 KiB, generated text to 1 MiB, and data events to 4096.
pub struct AiStreamJob<T = AiResult> {
    shared: Arc<Shared<T>>,
}

struct State<T> {
    events: VecDeque<AiStreamEvent<T>>,
    queued_text_bytes: usize,
    finished: bool,
    #[cfg(not(target_arch = "wasm32"))]
    abort: Option<tokio::task::AbortHandle>,
    #[cfg(target_arch = "wasm32")]
    abort: Option<web_sys::AbortController>,
    #[cfg(target_arch = "wasm32")]
    timeout: Option<WebTimeout>,
}

#[cfg(target_arch = "wasm32")]
struct WebTimeout {
    window: web_sys::Window,
    id: i32,
    _callback: wasm_bindgen::closure::Closure<dyn FnMut()>,
}

struct Shared<T> {
    cancelled: AtomicBool,
    state: Mutex<State<T>>,
}

impl<T> Shared<T> {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            state: Mutex::new(State {
                events: VecDeque::new(),
                queued_text_bytes: 0,
                finished: false,
                #[cfg(not(target_arch = "wasm32"))]
                abort: None,
                #[cfg(target_arch = "wasm32")]
                abort: None,
                #[cfg(target_arch = "wasm32")]
                timeout: None,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push_delta(&self, delta: String) -> Result<()> {
        if delta.is_empty() {
            return Ok(());
        }
        let mut state = self.lock();
        if state.finished || self.cancelled.load(Ordering::Acquire) {
            return Err(AiError::Cancelled);
        }
        if state.events.len() >= MAX_PENDING_EVENTS
            || state.queued_text_bytes.saturating_add(delta.len()) > MAX_TEXT_BYTES
        {
            return Err(stream_error("preview queue limit exceeded"));
        }
        state.queued_text_bytes += delta.len();
        state.events.push_back(AiStreamEvent::Delta(delta));
        Ok(())
    }

    fn complete(&self, result: Result<T>) {
        let mut state = self.lock();
        if state.finished {
            return;
        }
        state.finished = true;
        #[cfg(target_arch = "wasm32")]
        if let Some(timeout) = state.timeout.take() {
            timeout.window.clear_timeout_with_handle(timeout.id);
        }
        #[cfg(target_arch = "wasm32")]
        {
            state.abort = None;
        }
        let result = if self.cancelled.load(Ordering::Acquire) {
            Err(AiError::Cancelled)
        } else {
            result
        };
        state.events.push_back(AiStreamEvent::Finished(result));
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_abort(&self, abort: tokio::task::AbortHandle) {
        let mut state = self.lock();
        if self.cancelled.load(Ordering::Acquire) {
            abort.abort();
        } else {
            state.abort = Some(abort);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn set_abort(&self, abort: web_sys::AbortController) {
        let mut state = self.lock();
        if self.cancelled.load(Ordering::Acquire) {
            abort.abort();
        } else {
            state.abort = Some(abort);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn clear_abort(&self) {
        let mut state = self.lock();
        state.abort = None;
        if let Some(timeout) = state.timeout.take() {
            timeout.window.clear_timeout_with_handle(timeout.id);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn set_timeout(
        &self,
        window: web_sys::Window,
        id: i32,
        callback: wasm_bindgen::closure::Closure<dyn FnMut()>,
    ) {
        let mut state = self.lock();
        if self.cancelled.load(Ordering::Acquire) {
            window.clear_timeout_with_handle(id);
        } else {
            state.timeout = Some(WebTimeout {
                window,
                id,
                _callback: callback,
            });
        }
    }
}

impl<T> AiStreamJob<T> {
    fn new() -> (Self, Arc<Shared<T>>) {
        let shared = Arc::new(Shared::new());
        (
            Self {
                shared: shared.clone(),
            },
            shared,
        )
    }

    /// Cancel the request and discard any deltas not yet returned by `poll`.
    pub fn cancel(&self) {
        let mut state = self.shared.lock();
        if state.finished {
            return;
        }
        self.shared.cancelled.store(true, Ordering::Release);
        state.events.clear();
        state.queued_text_bytes = 0;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(abort) = state.abort.take() {
            abort.abort();
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(abort) = state.abort.take() {
            abort.abort();
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(timeout) = state.timeout.take() {
            timeout.window.clear_timeout_with_handle(timeout.id);
        }
        state.finished = true;
        state
            .events
            .push_back(AiStreamEvent::Finished(Err(AiError::Cancelled)));
    }

    /// Return the next preview delta or terminal result without blocking.
    pub fn poll(&self) -> Option<AiStreamEvent<T>> {
        let mut state = self.shared.lock();
        let event = state.events.pop_front()?;
        if let AiStreamEvent::Delta(text) = &event {
            state.queued_text_bytes = state.queued_text_bytes.saturating_sub(text.len());
        }
        Some(event)
    }

    /// Whether a terminal result has been queued.
    pub fn is_finished(&self) -> bool {
        self.shared.lock().finished
    }
}

impl AiStreamJob<AiResult> {
    /// Stream an existing mind-map task and parse its final response into `AiResult`.
    pub fn spawn(cfg: AiConfig, task: AiTask) -> Self {
        let messages = task.messages();
        spawn_job(cfg, move |cfg, shared| async move {
            let (model, text) = stream_chat(cfg, None, messages, shared).await?;
            finish(&task, &text, model)
        })
    }
}

impl AiStreamJob<String> {
    /// Stream a chat completion; an empty model selects the configured or first chat model.
    pub fn chat(cfg: AiConfig, model: String, messages: Vec<Message>) -> Self {
        spawn_job(cfg, move |cfg, shared| async move {
            let model = (!model.trim().is_empty()).then_some(model);
            let (_, text) = stream_chat(cfg, model, messages, shared).await?;
            Ok(text)
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_job<T, F, Fut>(cfg: AiConfig, work: F) -> AiStreamJob<T>
where
    T: Send + 'static,
    F: FnOnce(AiConfig, Arc<Shared<T>>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T>> + Send + 'static,
{
    let (job, shared) = AiStreamJob::new();
    std::thread::Builder::new()
        .name("thinoda-ai-stream".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => {
                    shared.complete(Err(stream_error("could not start stream runtime")));
                    return;
                }
            };
            let task = runtime.spawn(work(cfg, shared.clone()));
            shared.set_abort(task.abort_handle());
            let result = runtime.block_on(task).unwrap_or_else(|_| {
                if shared.is_cancelled() {
                    Err(AiError::Cancelled)
                } else {
                    Err(stream_error("stream worker stopped"))
                }
            });
            shared.complete(result);
        })
        .expect("spawn AI stream worker");
    job
}

#[cfg(target_arch = "wasm32")]
fn spawn_job<T: 'static, F, Fut>(cfg: AiConfig, work: F) -> AiStreamJob<T>
where
    F: FnOnce(AiConfig, Arc<Shared<T>>) -> Fut + 'static,
    Fut: std::future::Future<Output = Result<T>> + 'static,
{
    let (job, shared) = AiStreamJob::new();
    let worker_shared = shared.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let result = work(cfg, worker_shared.clone()).await;
        worker_shared.complete(result);
    });
    job
}

fn stream_error(message: &str) -> AiError {
    AiError::Parse(message.into())
}

fn append_deltas<T>(shared: &Shared<T>, text: &mut String, deltas: Vec<String>) -> Result<()> {
    for delta in deltas {
        if text.len().saturating_add(delta.len()) > MAX_TEXT_BYTES {
            return Err(stream_error("stream text size limit exceeded"));
        }
        shared.push_delta(delta.clone())?;
        text.push_str(&delta);
    }
    Ok(())
}

fn stream_body(cfg: &AiConfig, model: &str, messages: &[Message]) -> Value {
    serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": cfg.temperature,
        "max_tokens": cfg.max_tokens,
        "stream": true,
    })
}

struct SseDecoder {
    line: Vec<u8>,
    data: Vec<u8>,
    has_data: bool,
    swallow_lf: bool,
    total_bytes: usize,
    events: usize,
    done: bool,
}

impl SseDecoder {
    fn new() -> Self {
        Self {
            line: Vec::new(),
            data: Vec::new(),
            has_data: false,
            swallow_lf: false,
            total_bytes: 0,
            events: 0,
            done: false,
        }
    }

    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        self.total_bytes = self.total_bytes.saturating_add(bytes.len());
        if self.total_bytes > MAX_RESPONSE_BYTES {
            return Err(stream_error("stream response size limit exceeded"));
        }
        let mut deltas = Vec::new();
        for &byte in bytes {
            if self.done {
                break;
            }
            if self.swallow_lf {
                self.swallow_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => {
                    self.process_line(&mut deltas)?;
                    self.swallow_lf = true;
                }
                b'\n' => self.process_line(&mut deltas)?,
                _ => {
                    self.line.push(byte);
                    if self.line.len() > MAX_EVENT_BYTES {
                        return Err(stream_error("stream event size limit exceeded"));
                    }
                }
            }
        }
        Ok(deltas)
    }

    fn process_line(&mut self, deltas: &mut Vec<String>) -> Result<()> {
        let line = std::mem::take(&mut self.line);
        if line.is_empty() {
            self.dispatch(deltas)?;
            return Ok(());
        }
        if line.first() == Some(&b':') {
            return Ok(());
        }
        let Some(colon) = line.iter().position(|byte| *byte == b':') else {
            return Ok(());
        };
        if &line[..colon] != b"data" {
            return Ok(());
        }
        let mut value = &line[colon + 1..];
        if value.first() == Some(&b' ') {
            value = &value[1..];
        }
        let next_size = self
            .data
            .len()
            .saturating_add(value.len())
            .saturating_add(usize::from(self.has_data));
        if next_size > MAX_EVENT_BYTES {
            return Err(stream_error("stream event size limit exceeded"));
        }
        if self.has_data {
            self.data.push(b'\n');
        }
        self.data.extend_from_slice(value);
        self.has_data = true;
        Ok(())
    }

    fn dispatch(&mut self, deltas: &mut Vec<String>) -> Result<()> {
        if !self.has_data {
            return Ok(());
        }
        self.events += 1;
        if self.events > MAX_STREAM_EVENTS {
            return Err(stream_error("stream event count limit exceeded"));
        }
        let mut data = std::mem::take(&mut self.data);
        self.has_data = false;
        if data.last() == Some(&b'\n') {
            data.pop();
        }
        let data = std::str::from_utf8(&data)
            .map_err(|_| stream_error("stream event is not valid UTF-8"))?;
        if data == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let value: Value =
            serde_json::from_str(data).map_err(|_| stream_error("malformed stream event"))?;
        if value.get("error").is_some() {
            return Err(stream_error("server returned a stream error"));
        }
        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| stream_error("stream event is missing choices"))?;
        if let Some(choice) = choices.first() {
            let delta = choice
                .get("delta")
                .and_then(Value::as_object)
                .ok_or_else(|| stream_error("stream event is missing delta"))?;
            match delta.get("content") {
                None | Some(Value::Null) => {}
                Some(Value::String(text)) if !text.is_empty() => deltas.push(text.clone()),
                Some(Value::String(_)) => {}
                Some(_) => return Err(stream_error("stream content is not text")),
            }
        }
        Ok(())
    }

    fn finish(&self) -> Result<()> {
        if self.done {
            Ok(())
        } else {
            Err(stream_error("stream ended before [DONE]"))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_client(cfg: &AiConfig) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(cfg.timeout_secs.max(1)))
        .build()
        .map_err(|_| stream_error("could not create stream client"))
}

#[cfg(not(target_arch = "wasm32"))]
fn authorize(request: reqwest::RequestBuilder, cfg: &AiConfig) -> reqwest::RequestBuilder {
    if let Some(key) = configured_key(cfg) {
        request.bearer_auth(key)
    } else {
        request
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn read_json(response: reqwest::Response) -> Result<Value> {
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| stream_error("could not read model response"))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(stream_error("model response size limit exceeded"));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| stream_error("malformed model response"))
}

#[cfg(not(target_arch = "wasm32"))]
async fn resolve_native_model(
    cfg: &AiConfig,
    client: &reqwest::Client,
    shared: &Shared<impl Sized>,
) -> Result<String> {
    if shared.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    if !cfg.model.trim().is_empty() {
        return pick_model(cfg, None);
    }
    let request = client.get(join_url(&cfg.base_url, "models"));
    let response = authorize(request, cfg)
        .send()
        .await
        .map_err(|_| stream_error("model request failed"))?;
    if !response.status().is_success() {
        return Err(AiError::Status(
            response.status().as_u16(),
            "model request failed".into(),
        ));
    }
    if shared.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    let models = parse_models(&read_json(response).await?)?;
    pick_model(cfg, Some(models))
}

#[cfg(not(target_arch = "wasm32"))]
async fn stream_chat(
    cfg: AiConfig,
    requested_model: Option<String>,
    messages: Vec<Message>,
    shared: Arc<Shared<impl Sized>>,
) -> Result<(String, String)> {
    let client = native_client(&cfg)?;
    let model = match requested_model {
        Some(model) => model,
        None => resolve_native_model(&cfg, &client, &shared).await?,
    };
    if shared.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    let response = authorize(
        client.post(join_url(&cfg.base_url, "chat/completions")),
        &cfg,
    )
    .json(&stream_body(&cfg, &model, &messages))
    .send()
    .await
    .map_err(|_| stream_error("chat request failed"))?;
    if !response.status().is_success() {
        return Err(AiError::Status(
            response.status().as_u16(),
            "chat request failed".into(),
        ));
    }
    if shared.is_cancelled() {
        return Err(AiError::Cancelled);
    }

    let mut response = response;
    let mut decoder = SseDecoder::new();
    let mut text = String::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| stream_error("stream read failed"))?
    {
        if shared.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        append_deltas(&shared, &mut text, decoder.feed(&chunk)?)?;
        if decoder.done {
            break;
        }
    }
    decoder.finish()?;
    Ok((model, text))
}

#[cfg(target_arch = "wasm32")]
async fn web_response(
    cfg: &AiConfig,
    path: &str,
    method: &str,
    body: Option<&Value>,
    shared: &Shared<impl Sized>,
) -> Result<web_sys::Response> {
    use wasm_bindgen::{JsCast, JsValue, closure::Closure};
    use wasm_bindgen_futures::JsFuture;

    if shared.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    let controller = web_sys::AbortController::new()
        .map_err(|_| stream_error("could not create stream abort controller"))?;
    shared.set_abort(controller.clone());
    let window = web_sys::window().ok_or_else(|| stream_error("browser window unavailable"))?;
    let timeout_controller = controller.clone();
    let callback = Closure::wrap(Box::new(move || timeout_controller.abort()) as Box<dyn FnMut()>);
    let timeout_ms = cfg
        .timeout_secs
        .max(1)
        .saturating_mul(1000)
        .min(i32::MAX as u64) as i32;
    let timeout_id = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            timeout_ms,
        )
        .map_err(|_| stream_error("could not set stream timeout"))?;
    shared.set_timeout(window.clone(), timeout_id, callback);
    let init = web_sys::RequestInit::new();
    init.set_method(method);
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_signal(Some(&controller.signal()));
    let headers = web_sys::Headers::new()
        .map_err(|_| stream_error("could not create stream request headers"))?;
    if let Some(key) = configured_key(cfg) {
        headers
            .set("Authorization", &format!("Bearer {key}"))
            .map_err(|_| stream_error("could not set stream authorization"))?;
    }
    if let Some(body) = body {
        headers
            .set("Content-Type", "application/json")
            .map_err(|_| stream_error("could not set stream content type"))?;
        init.set_body(&JsValue::from_str(&body.to_string()));
    }
    init.set_headers(&headers);
    let request = web_sys::Request::new_with_str_and_init(&join_url(&cfg.base_url, path), &init)
        .map_err(|_| stream_error("could not create stream request"))?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| {
            if shared.is_cancelled() {
                AiError::Cancelled
            } else {
                stream_error("stream request failed")
            }
        })?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| stream_error("invalid stream response"))?;
    if !response.ok() {
        return Err(AiError::Status(
            response.status(),
            "stream request failed".into(),
        ));
    }
    Ok(response)
}

#[cfg(target_arch = "wasm32")]
async fn read_web_body<F>(
    response: web_sys::Response,
    shared: &Shared<impl Sized>,
    mut on_chunk: F,
) -> Result<()>
where
    F: FnMut(&[u8]) -> Result<bool>,
{
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    let body = response
        .body()
        .ok_or_else(|| stream_error("stream response has no body"))?;
    let reader = body
        .get_reader()
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|_| stream_error("could not read stream response"))?;
    let mut received = 0usize;
    loop {
        if shared.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        let result = JsFuture::from(reader.read()).await.map_err(|_| {
            if shared.is_cancelled() {
                AiError::Cancelled
            } else {
                stream_error("stream read failed")
            }
        })?;
        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if done {
            break;
        }
        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))
            .map_err(|_| stream_error("invalid stream chunk"))?;
        let chunk = js_sys::Uint8Array::new(&value).to_vec();
        received = received.saturating_add(chunk.len());
        if received > MAX_RESPONSE_BYTES {
            return Err(stream_error("stream response size limit exceeded"));
        }
        if on_chunk(&chunk)? {
            break;
        }
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn resolve_web_model(cfg: &AiConfig, shared: &Shared<impl Sized>) -> Result<String> {
    if !cfg.model.trim().is_empty() {
        return pick_model(cfg, None);
    }
    let response = web_response(cfg, "models", "GET", None, shared).await?;
    let mut body = Vec::new();
    read_web_body(response, shared, |chunk| {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(stream_error("model response size limit exceeded"));
        }
        body.extend_from_slice(chunk);
        Ok(false)
    })
    .await?;
    shared.clear_abort();
    let value: Value =
        serde_json::from_slice(&body).map_err(|_| stream_error("malformed model response"))?;
    pick_model(cfg, Some(parse_models(&value)?))
}

#[cfg(target_arch = "wasm32")]
async fn stream_chat(
    cfg: AiConfig,
    requested_model: Option<String>,
    messages: Vec<Message>,
    shared: Arc<Shared<impl Sized>>,
) -> Result<(String, String)> {
    let model = match requested_model {
        Some(model) => model,
        None => resolve_web_model(&cfg, &shared).await?,
    };
    if shared.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    let response = web_response(
        &cfg,
        "chat/completions",
        "POST",
        Some(&stream_body(&cfg, &model, &messages)),
        &shared,
    )
    .await?;
    let mut decoder = SseDecoder::new();
    let mut text = String::new();
    read_web_body(response, &shared, |chunk| {
        append_deltas(&shared, &mut text, decoder.feed(chunk)?)?;
        Ok(decoder.done)
    })
    .await?;
    decoder.finish()?;
    Ok((model, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(content: &str) -> Vec<u8> {
        format!(
            "data: {}\n\n",
            serde_json::json!({"choices":[{"delta":{"content":content}}]})
        )
        .into_bytes()
    }

    #[test]
    fn decoder_handles_split_utf8_and_sse_frames() {
        let mut decoder = SseDecoder::new();
        let mut bytes = frame("café 🧠");
        bytes.extend_from_slice(b"data: [DONE]\r\n\r\n");
        let split = bytes.iter().position(|b| *b >= 0x80).unwrap() + 1;
        let mut deltas = decoder.feed(&bytes[..split]).unwrap();
        deltas.extend(decoder.feed(&bytes[split..]).unwrap());
        assert_eq!(deltas, ["café 🧠"]);
        decoder.finish().unwrap();
    }

    #[test]
    fn decoder_supports_multiline_data_and_ignores_comments() {
        let mut decoder = SseDecoder::new();
        let mut deltas = Vec::new();
        for chunk in [
            b": keepalive\r".as_slice(),
            b"\ndata: {\"choices\":[{\"delta\":\r\n".as_slice(),
            b"data: {\"content\":\"hello\"}}]}\r\n\r\n".as_slice(),
            b"data: [DONE]\n\n".as_slice(),
        ] {
            deltas.extend(decoder.feed(chunk).unwrap());
        }
        assert_eq!(deltas, ["hello"]);
        decoder.finish().unwrap();
    }

    #[test]
    fn decoder_rejects_malformed_truncated_and_oversized_streams() {
        let mut malformed = SseDecoder::new();
        assert!(malformed.feed(b"data: {broken}\n\n").is_err());

        let mut truncated = SseDecoder::new();
        truncated.feed(&frame("partial")).unwrap();
        assert!(truncated.finish().is_err());

        let mut oversized = SseDecoder::new();
        assert!(oversized.feed(&vec![b'x'; MAX_RESPONSE_BYTES + 1]).is_err());

        let mut oversized_frame = SseDecoder::new();
        assert!(
            oversized_frame
                .feed(&vec![b'x'; MAX_EVENT_BYTES + 1])
                .is_err()
        );
    }

    #[test]
    fn decoder_rejects_stream_errors_and_event_limit_overflow() {
        let mut server_error = SseDecoder::new();
        assert!(
            server_error
                .feed(b"data: {\"error\":{\"message\":\"private detail\"}}\n\n")
                .is_err()
        );

        let mut too_many = SseDecoder::new();
        let event = b"data: {\"choices\":[]}\n\n";
        for _ in 0..MAX_STREAM_EVENTS {
            too_many.feed(event).unwrap();
        }
        assert!(too_many.feed(event).is_err());
    }

    #[test]
    fn accumulated_text_limit_is_enforced() {
        let shared = Shared::<String>::new();
        let mut text = String::new();
        assert!(
            append_deltas(
                &shared,
                &mut text,
                vec!["x".repeat(MAX_TEXT_BYTES), "y".into()]
            )
            .is_err()
        );
        assert_eq!(text.len(), MAX_TEXT_BYTES);
    }
}
