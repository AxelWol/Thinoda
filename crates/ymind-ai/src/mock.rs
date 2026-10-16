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

impl MockServer {
    pub fn start(models: &[&str], reply: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let rec = requests.clone();
        let models: Vec<String> = models.iter().map(|s| s.to_string()).collect();
        let reply = reply.to_string();
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
                let (status, json) = if path == "/v1/models" {
                    let data: Vec<_> = models
                        .iter()
                        .map(|m| serde_json::json!({ "id": m, "object": "model" }))
                        .collect();
                    (
                        "200 OK",
                        serde_json::json!({ "object": "list", "data": data }),
                    )
                } else if path == "/v1/chat/completions" {
                    (
                        "200 OK",
                        serde_json::json!({ "choices": [{ "index": 0, "message": { "role": "assistant", "content": reply } }] }),
                    )
                } else {
                    (
                        "404 Not Found",
                        serde_json::json!({ "error": { "message": "not found" } }),
                    )
                };
                let payload = json.to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
            }
        });
        Self { base_url, requests }
    }
}
