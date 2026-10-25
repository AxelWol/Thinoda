//! Minimal OpenAI-compatible HTTP server for tests (std only, no async runtime).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// A request received by the mock server.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: String,
}

/// Serves `/v1/models` with `models` and answers every `/v1/chat/completions` with `reply`.
pub struct MockServer {
    pub base_url: String,
    pub requests: Arc<Mutex<Vec<Recorded>>>,
}

enum MockReply {
    Json(String),
    Sse(String),
}

impl MockServer {
    /// Serves `/v1/models` and a regular non-streaming completion.
    pub fn start(models: &[&str], reply: &str) -> Self {
        Self::start_with_reply(models, MockReply::Json(reply.to_string()))
    }

    /// Serves `/v1/models` and SSE `data:` events for streaming completions.
    ///
    /// Each entry is one SSE data payload, typically a serialized OpenAI
    /// `choices[0].delta` object. A terminal `[DONE]` event is appended unless
    /// the final entry already is `[DONE]`.
    ///
    /// ```ignore
    /// let server = MockServer::start_sse(
    ///     &["chat-model"],
    ///     &[r#"{"choices":[{"delta":{"content":"Hello"}}]}"#],
    /// );
    /// ```
    pub fn start_sse(models: &[&str], events: &[&str]) -> Self {
        let mut payload = String::new();
        for event in events {
            payload.push_str("data: ");
            payload.push_str(event);
            payload.push_str("\r\n\r\n");
        }
        if events.last().copied() != Some("[DONE]") {
            payload.push_str("data: [DONE]\r\n\r\n");
        }
        Self::start_with_reply(models, MockReply::Sse(payload))
    }

    fn start_with_reply(models: &[&str], reply: MockReply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let rec = requests.clone();
        let models: Vec<String> = models.iter().map(|s| s.to_string()).collect();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let mut len = 0usize;
                let mut auth = None;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                        break;
                    }
                    let lower = h.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                    if lower.starts_with("authorization:") {
                        auth = Some(h["authorization:".len()..].trim().to_string());
                    }
                }
                let mut body = vec![0; len];
                let _ = reader.read_exact(&mut body);
                let body = String::from_utf8_lossy(&body).to_string();
                rec.lock().unwrap().push(Recorded {
                    method,
                    path: path.clone(),
                    authorization: auth,
                    body,
                });
                let (status, content_type, payload) = if path == "/v1/models" {
                    let data: Vec<_> = models
                        .iter()
                        .map(|m| serde_json::json!({ "id": m, "object": "model" }))
                        .collect();
                    (
                        "200 OK",
                        "application/json",
                        serde_json::json!({ "object": "list", "data": data }).to_string(),
                    )
                } else if path == "/v1/chat/completions" {
                    match &reply {
                        MockReply::Json(reply) => (
                            "200 OK",
                            "application/json",
                            serde_json::json!({ "choices": [{ "index": 0, "message": { "role": "assistant", "content": reply } }] }).to_string(),
                        ),
                        MockReply::Sse(payload) => {
                            ("200 OK", "text/event-stream", payload.clone())
                        }
                    }
                } else {
                    (
                        "404 Not Found",
                        "application/json",
                        serde_json::json!({ "error": { "message": "not found" } }).to_string(),
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
            }
        });
        Self { base_url, requests }
    }
}
