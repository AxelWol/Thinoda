//! Browser `fetch` client with the same API as the desktop client, but async.

use super::{
    AiConfig, AiError, AiResult, AiTask, Message, Result, chat_body, configured_key, finish,
    join_url, parse_chat, parse_models, parse_response, pick_model,
};
use serde_json::Value;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// OpenAI-compatible client using the browser's `fetch`. Cross-origin servers
/// must send CORS headers (LM Studio and Ollama can be configured to do so).
pub struct Client {
    cfg: AiConfig,
}

fn js_error(e: JsValue) -> AiError {
    let text = e
        .dyn_ref::<js_sys::Error>()
        .map(|e| String::from(e.message()))
        .or_else(|| e.as_string())
        .unwrap_or_else(|| format!("{e:?}"));
    AiError::Http(format!(
        "{text} (the server must be reachable from the browser and allow CORS)"
    ))
}

impl Client {
    pub fn new(cfg: AiConfig) -> Self {
        Self { cfg }
    }

    pub fn config(&self) -> &AiConfig {
        &self.cfg
    }

    async fn request(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let init = web_sys::RequestInit::new();
        init.set_method(method);
        init.set_mode(web_sys::RequestMode::Cors);
        let headers = web_sys::Headers::new().map_err(js_error)?;
        if let Some(key) = configured_key(&self.cfg) {
            headers
                .set("Authorization", &format!("Bearer {key}"))
                .map_err(js_error)?;
        }
        if let Some(body) = body {
            headers
                .set("Content-Type", "application/json")
                .map_err(js_error)?;
            init.set_body(&JsValue::from_str(&body.to_string()));
        }
        init.set_headers(&headers);
        let timeout_ms = (self.cfg.timeout_secs.max(1) * 1000).min(u64::from(u32::MAX)) as u32;
        init.set_signal(Some(&web_sys::AbortSignal::timeout_with_u32(timeout_ms)));
        let request =
            web_sys::Request::new_with_str_and_init(&join_url(&self.cfg.base_url, path), &init)
                .map_err(js_error)?;
        let window = web_sys::window().ok_or_else(|| AiError::Http("no browser window".into()))?;
        let response: web_sys::Response = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(js_error)?
            .dyn_into()
            .map_err(js_error)?;
        let text = JsFuture::from(response.text().map_err(js_error)?)
            .await
            .map_err(js_error)?
            .as_string()
            .unwrap_or_default();
        parse_response(response.status(), &text)
    }

    /// Model ids offered by the server (`GET /models`).
    pub async fn models(&self) -> Result<Vec<String>> {
        parse_models(&self.request("GET", "models", None).await?)
    }

    /// Configured model, or the first chat model the server offers.
    pub async fn resolve_model(&self) -> Result<String> {
        if !self.cfg.model.trim().is_empty() {
            return pick_model(&self.cfg, None);
        }
        pick_model(&self.cfg, Some(self.models().await?))
    }

    /// One non-streaming chat completion; returns the assistant text.
    pub async fn chat(&self, model: &str, messages: &[Message]) -> Result<String> {
        let body = chat_body(&self.cfg, model, messages);
        parse_chat(
            &self
                .request("POST", "chat/completions", Some(&body))
                .await?,
        )
    }

    /// Runs a mind-map task end to end.
    pub async fn run(&self, task: &AiTask) -> Result<AiResult> {
        let model = self.resolve_model().await?;
        let text = self
            .chat(&model, &task.messages_in(&self.cfg.reply_language))
            .await?;
        finish(task, &text, model)
    }
}
