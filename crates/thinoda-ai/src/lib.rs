//! Thinoda AI assistant.
//!
//! A small blocking client for OpenAI-compatible chat APIs (LM Studio, Ollama, llama.cpp
//! server, vLLM, OpenAI, Azure OpenAI via a compatible endpoint) plus the mind-map tasks
//! and a tolerant parser for the returned outlines. Requests run on background threads
//! via [`AiJob`] or [`AiStreamJob`] so the UI never blocks.
//! Explicit-model [`embeddings`] batches use the cancellable [`AiEmbeddingJob`].

pub mod embeddings;
#[doc(hidden)]
#[cfg(not(target_arch = "wasm32"))]
pub mod mock;
pub mod outline;
mod stream;
pub mod tasks;
#[cfg(target_arch = "wasm32")]
mod web;

use std::sync::mpsc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
pub use web::Client;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use embeddings::AiEmbeddingJob;
pub use outline::{Outline, OutlineNode, parse_outline, strip_reasoning};
pub use stream::{AiStreamEvent, AiStreamJob};
pub use tasks::{AiTask, OutputKind, TopicContext};

/// Default endpoint: LM Studio's local server.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:1234/v1";
/// Environment variable consulted when no API key is configured.
pub const API_KEY_ENV: &str = "THINODA_AI_API_KEY";

/// Connection settings for an OpenAI-compatible endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// Base URL including the API version path, e.g. `http://127.0.0.1:1234/v1`.
    pub base_url: String,
    /// Model id; empty = first chat model reported by `/models`.
    pub model: String,
    /// Optional bearer token (falls back to `THINODA_AI_API_KEY`).
    pub api_key: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout_secs: u64,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.into(),
            model: String::new(),
            api_key: String::new(),
            temperature: 0.7,
            max_tokens: 1500,
            timeout_secs: 120,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiError {
    #[error("connection failed: {0}")]
    Http(String),
    #[error("server returned {0}: {1}")]
    Status(u16, String),
    #[error("unexpected response: {0}")]
    Parse(String),
    #[error("no chat model available on the server")]
    NoModel,
    #[error("the model returned no usable content")]
    Empty,
    #[error("AI request was cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, AiError>;

/// One chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(c: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: c.into(),
        }
    }
    pub fn user(c: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: c.into(),
        }
    }
}

/// Result content of a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiOutput {
    Outline(Outline),
    Text(String),
}

/// A finished task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiResult {
    pub kind: OutputKind,
    pub output: AiOutput,
    pub model: String,
}

/// Blocking OpenAI-compatible client (the browser build uses an async `fetch` client).
#[cfg(not(target_arch = "wasm32"))]
pub struct Client {
    cfg: AiConfig,
    agent: ureq::Agent,
}

fn is_chat_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    !(id.contains("embed") || id.contains("rerank") || id.contains("whisper") || id.contains("tts"))
}

/// `base_url` joined with an endpoint path.
fn join_url(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim().trim_end_matches('/'), path)
}

/// Configured key, falling back to `THINODA_AI_API_KEY` (desktop only).
fn configured_key(cfg: &AiConfig) -> Option<String> {
    let k = cfg.api_key.trim();
    if !k.is_empty() {
        return Some(k.to_string());
    }
    std::env::var(API_KEY_ENV)
        .ok()
        .filter(|k| !k.trim().is_empty())
}

/// Interpret an HTTP status and body as JSON, extracting server error messages.
fn parse_response(status: u16, body: &str) -> Result<Value> {
    if !(200..300).contains(&status) {
        let msg = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .or(v.get("error"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| body.chars().take(300).collect());
        return Err(AiError::Status(status, msg));
    }
    serde_json::from_str(body).map_err(|e| AiError::Parse(e.to_string()))
}

fn parse_models(v: &Value) -> Result<Vec<String>> {
    let list = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::Parse("missing \"data\"".into()))?;
    Ok(list
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .collect())
}

fn pick_model(cfg: &AiConfig, models: Option<Vec<String>>) -> Result<String> {
    let m = cfg.model.trim();
    if !m.is_empty() {
        return Ok(m.to_string());
    }
    models
        .unwrap_or_default()
        .into_iter()
        .find(|m| is_chat_model(m))
        .ok_or(AiError::NoModel)
}

fn chat_body(cfg: &AiConfig, model: &str, messages: &[Message]) -> Value {
    json!({
        "model": model,
        "messages": messages,
        "temperature": cfg.temperature,
        "max_tokens": cfg.max_tokens,
        "stream": false,
    })
}

fn parse_chat(v: &Value) -> Result<String> {
    v.pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AiError::Parse("missing choices[0].message.content".into()))
}

/// Turn a model reply into the task's result type.
fn finish(task: &AiTask, text: &str, model: String) -> Result<AiResult> {
    let kind = task.output_kind();
    let output = match kind {
        OutputKind::Note => {
            let t = strip_reasoning(text);
            if t.is_empty() {
                return Err(AiError::Empty);
            }
            AiOutput::Text(t)
        }
        OutputKind::Map => {
            let o = parse_outline(text).into_map();
            if o.items.is_empty() {
                return Err(AiError::Empty);
            }
            AiOutput::Outline(o)
        }
        OutputKind::Subtopics | OutputKind::Todos => {
            let o = parse_outline(text);
            if o.items.is_empty() {
                return Err(AiError::Empty);
            }
            AiOutput::Outline(o)
        }
    };
    Ok(AiResult {
        kind,
        output,
        model,
    })
}

#[cfg(not(target_arch = "wasm32"))]
impl Client {
    pub fn new(cfg: AiConfig) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(cfg.timeout_secs.max(1))))
            .timeout_connect(Some(Duration::from_secs(5)))
            .http_status_as_error(false)
            .build()
            .into();
        Self { cfg, agent }
    }

    pub fn config(&self) -> &AiConfig {
        &self.cfg
    }

    fn url(&self, path: &str) -> String {
        join_url(&self.cfg.base_url, path)
    }

    fn api_key(&self) -> Option<String> {
        configured_key(&self.cfg)
    }

    fn read(
        resp: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<Value> {
        let mut resp = resp.map_err(|e| AiError::Http(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| AiError::Http(e.to_string()))?;
        parse_response(status, &body)
    }

    /// Model ids offered by the server (`GET /models`).
    pub fn models(&self) -> Result<Vec<String>> {
        let mut req = self.agent.get(self.url("models"));
        if let Some(k) = self.api_key() {
            req = req.header("Authorization", format!("Bearer {k}"));
        }
        parse_models(&Self::read(req.call())?)
    }

    /// Configured model, or the first chat model the server offers.
    pub fn resolve_model(&self) -> Result<String> {
        if !self.cfg.model.trim().is_empty() {
            return pick_model(&self.cfg, None);
        }
        pick_model(&self.cfg, Some(self.models()?))
    }

    /// One non-streaming chat completion; returns the assistant text.
    pub fn chat(&self, model: &str, messages: &[Message]) -> Result<String> {
        let body = chat_body(&self.cfg, model, messages);
        let mut req = self.agent.post(self.url("chat/completions"));
        if let Some(k) = self.api_key() {
            req = req.header("Authorization", format!("Bearer {k}"));
        }
        parse_chat(&Self::read(req.send_json(&body))?)
    }

    /// Runs a mind-map task end to end.
    pub fn run(&self, task: &AiTask) -> Result<AiResult> {
        let model = self.resolve_model()?;
        let text = self.chat(&model, &task.messages())?;
        finish(task, &text, model)
    }
}

/// A request running on a background thread (a browser task on the web).
pub struct AiJob<T = AiResult> {
    rx: mpsc::Receiver<Result<T>>,
    pub started: Instant,
}

impl<T: Send + 'static> AiJob<T> {
    /// Runs `f` with a fresh client on a background thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_with(
        cfg: AiConfig,
        f: impl FnOnce(&Client) -> Result<T> + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("thinoda-ai".into())
            .spawn(move || {
                let _ = tx.send(f(&Client::new(cfg)));
            })
            .expect("spawn AI worker");
        Self {
            rx,
            started: Instant::now(),
        }
    }

    /// The result, once available.
    pub fn poll(&self) -> Option<Result<T>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(AiError::Http("AI worker stopped".into())))
            }
        }
    }

    /// Blocks until the result is available (tests and CLI).
    pub fn wait(self) -> Result<T> {
        self.rx
            .recv()
            .unwrap_or_else(|_| Err(AiError::Http("AI worker stopped".into())))
    }
}

impl AiJob<AiResult> {
    /// Run a mind-map task.
    pub fn spawn(cfg: AiConfig, task: AiTask) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        return Self::spawn_with(cfg, move |c| c.run(&task));
        #[cfg(target_arch = "wasm32")]
        Self::spawn_local(async move { Client::new(cfg).run(&task).await })
    }
}

impl AiJob<Vec<String>> {
    /// List the models offered by the server.
    pub fn models(cfg: AiConfig) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        return Self::spawn_with(cfg, |c| c.models());
        #[cfg(target_arch = "wasm32")]
        Self::spawn_local(async move { Client::new(cfg).models().await })
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: 'static> AiJob<T> {
    fn spawn_local(work: impl std::future::Future<Output = Result<T>> + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = tx.send(work.await);
        });
        Self {
            rx,
            started: Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_model_filter() {
        assert!(is_chat_model("google/gemma-4-e4b"));
        assert!(!is_chat_model("text-embedding-nomic-embed-text-v1.5"));
    }

    #[test]
    fn url_joining() {
        let c = Client::new(AiConfig {
            base_url: " http://h:1/v1/ ".into(),
            ..AiConfig::default()
        });
        assert_eq!(c.url("models"), "http://h:1/v1/models");
    }

    #[test]
    fn shared_response_parsing_used_by_desktop_and_web_clients() {
        let ok = parse_response(200, r#"{"data":[{"id":"embed-x"},{"id":"chat-y"}]}"#).unwrap();
        let models = parse_models(&ok).unwrap();
        assert_eq!(models, ["embed-x", "chat-y"]);
        let cfg = AiConfig::default();
        assert_eq!(pick_model(&cfg, Some(models)).unwrap(), "chat-y");
        assert_eq!(pick_model(&cfg, Some(vec![])), Err(AiError::NoModel));
        assert_eq!(
            parse_response(401, r#"{"error":{"message":"bad key"}}"#),
            Err(AiError::Status(401, "bad key".into()))
        );
        assert!(matches!(
            parse_response(200, "nope"),
            Err(AiError::Parse(_))
        ));
        let chat = parse_response(200, r#"{"choices":[{"message":{"content":"- A"}}]}"#).unwrap();
        assert_eq!(parse_chat(&chat).unwrap(), "- A");
        let body = chat_body(&cfg, "m", &[Message::user("hi")]);
        assert_eq!(body["model"], "m");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn unreachable_server_reports_http_error() {
        let c = Client::new(AiConfig {
            base_url: "http://127.0.0.1:9/v1".into(),
            timeout_secs: 2,
            ..AiConfig::default()
        });
        assert!(matches!(c.models(), Err(AiError::Http(_))));
    }
}
