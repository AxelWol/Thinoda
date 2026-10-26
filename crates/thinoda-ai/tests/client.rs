//! Client tests against the built-in mock server, plus an opt-out live test against a
//! running LM Studio (skipped automatically when no server is reachable).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use thinoda_ai::mock::MockServer;
use thinoda_ai::{
    AiConfig, AiError, AiJob, AiOutput, AiStreamEvent, AiStreamJob, AiTask, Client, Message,
    OutputKind, TopicContext,
};

fn ctx() -> TopicContext {
    TopicContext {
        path: vec!["Product launch".into()],
        children: vec![],
        subtree: vec![],
    }
}

#[test]
fn brainstorm_against_mock_server() {
    let srv = MockServer::start(
        &["text-embedding-x", "chat-model"],
        "Sure!\n- Marketing\n- Pricing\n  - Tiers\n- Launch event",
    );
    let cfg = AiConfig {
        base_url: srv.base_url.clone(),
        api_key: "secret".into(),
        ..AiConfig::default()
    };
    let r = AiJob::spawn(cfg, AiTask::Brainstorm(ctx())).wait().unwrap();
    assert_eq!(r.model, "chat-model", "embedding models are skipped");
    assert_eq!(r.kind, OutputKind::Subtopics);
    let AiOutput::Outline(o) = r.output else {
        panic!("outline expected")
    };
    assert_eq!(o.items.len(), 3);
    assert_eq!(o.items[1].children[0].title, "Tiers");
    let reqs = srv.requests.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[1].path, "/v1/chat/completions");
    assert_eq!(reqs[1].authorization.as_deref(), Some("Bearer secret"));
    let body: serde_json::Value = serde_json::from_str(&reqs[1].body).unwrap();
    assert_eq!(body["model"], "chat-model");
    assert_eq!(body["stream"], false);
    assert!(
        body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Product launch")
    );
}

#[test]
fn explicit_model_skips_listing_and_note_output() {
    let srv = MockServer::start(
        &[],
        "<think>hmm</think>A launch is the public release of a product.",
    );
    let cfg = AiConfig {
        base_url: srv.base_url.clone(),
        model: "m".into(),
        ..AiConfig::default()
    };
    let r = Client::new(cfg).run(&AiTask::Explain(ctx())).unwrap();
    assert_eq!(
        r.output,
        AiOutput::Text("A launch is the public release of a product.".into())
    );
    assert_eq!(srv.requests.lock().unwrap().len(), 1);
}

#[test]
fn empty_answers_and_missing_models_are_errors() {
    let srv = MockServer::start(&["embed-only"], "I cannot help with that.");
    let c = Client::new(AiConfig {
        base_url: srv.base_url.clone(),
        ..AiConfig::default()
    });
    assert_eq!(c.run(&AiTask::Brainstorm(ctx())), Err(AiError::NoModel));
    let c = Client::new(AiConfig {
        base_url: srv.base_url.clone(),
        model: "m".into(),
        ..AiConfig::default()
    });
    assert_eq!(c.run(&AiTask::Brainstorm(ctx())), Err(AiError::Empty));
}

#[test]
fn http_errors_are_reported_with_status() {
    let srv = MockServer::start(&[], "");
    let c = Client::new(AiConfig {
        base_url: format!("{}/nope", srv.base_url),
        ..AiConfig::default()
    });
    assert!(matches!(c.models(), Err(AiError::Status(404, m)) if m == "not found"));
}

fn stream_server(
    status: &str,
    body: &[u8],
    pause_after: Option<(usize, mpsc::Receiver<()>)>,
) -> (String, mpsc::Receiver<(String, String, String)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let status = status.to_string();
    let body = body.to_vec();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                return;
            }
            let mut request_parts = request_line.split_whitespace();
            let method = request_parts.next().unwrap_or("");
            let path = request_parts.next().unwrap_or("").to_string();
            let mut authorization = String::new();
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("authorization") {
                        authorization = value.trim().to_string();
                    }
                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut request_body = vec![0; content_length];
            if reader.read_exact(&mut request_body).is_err() {
                return;
            }
            let _ = tx.send((
                path.clone(),
                authorization,
                String::from_utf8_lossy(&request_body).into_owned(),
            ));

            if method == "GET" && path.ends_with("/models") {
                let models = serde_json::json!({
                    "data": [
                        {"id": "text-embedding-model"},
                        {"id": "auto-chat-model"}
                    ]
                })
                .to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{models}",
                    models.len()
                );
                continue;
            }

            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if let Some((split, release)) = pause_after {
                let _ = stream.write_all(&body[..split]);
                let _ = stream.flush();
                let _ = release.recv_timeout(Duration::from_secs(5));
                let _ = stream.write_all(&body[split..]);
            } else {
                let _ = stream.write_all(&body);
            }
            break;
        }
    });
    (base_url, rx)
}

fn collect_stream(job: &AiStreamJob<String>) -> (String, Result<String, AiError>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut preview = String::new();
    loop {
        if let Some(event) = job.poll() {
            match event {
                AiStreamEvent::Delta(delta) => preview.push_str(&delta),
                AiStreamEvent::Finished(result) => return (preview, result),
            }
        } else if Instant::now() >= deadline {
            panic!("stream did not finish");
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

#[test]
fn streaming_chat_reports_preview_and_final_text() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hello \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, request) = stream_server("200 OK", body.as_bytes(), None);
    let job = AiStreamJob::chat(
        AiConfig {
            base_url,
            model: "test-model".into(),
            api_key: "stream-secret".into(),
            ..AiConfig::default()
        },
        "test-model".into(),
        vec![Message::user("hello")],
    );
    let (preview, result) = collect_stream(&job);
    assert_eq!(preview, "hello world");
    assert_eq!(result.unwrap(), "hello world");
    let (path, authorization, request_body) = request.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(path, "/v1/chat/completions");
    assert_eq!(authorization, "Bearer stream-secret");
    let request_body: serde_json::Value = serde_json::from_str(&request_body).unwrap();
    assert_eq!(request_body["stream"], true);
    assert_eq!(request_body["model"], "test-model");
}

#[test]
fn empty_stream_model_uses_the_existing_auto_model_resolver() {
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"automatic\"}}]}\n\ndata: [DONE]\n\n";
    let (base_url, requests) = stream_server("200 OK", body.as_bytes(), None);
    let job = AiStreamJob::chat(
        AiConfig {
            base_url,
            ..AiConfig::default()
        },
        String::new(),
        vec![Message::user("hello")],
    );
    let (_, result) = collect_stream(&job);
    assert_eq!(result.unwrap(), "automatic");

    let (models_path, _, _) = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    let (chat_path, _, chat_body) = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(models_path, "/v1/models");
    assert_eq!(chat_path, "/v1/chat/completions");
    let chat_body: serde_json::Value = serde_json::from_str(&chat_body).unwrap();
    assert_eq!(chat_body["model"], "auto-chat-model");
    assert_eq!(chat_body["stream"], true);
}

#[test]
fn streaming_task_keeps_the_existing_typed_result() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"- Alpha\\n- Beta\"}}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, _) = stream_server("200 OK", body.as_bytes(), None);
    let job = AiStreamJob::spawn(
        AiConfig {
            base_url,
            model: "test-model".into(),
            ..AiConfig::default()
        },
        AiTask::Brainstorm(ctx()),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(AiStreamEvent::Finished(result)) = job.poll() {
            let result = result.unwrap();
            assert_eq!(result.kind, OutputKind::Subtopics);
            let AiOutput::Outline(outline) = result.output else {
                panic!("outline result expected");
            };
            assert_eq!(
                outline
                    .items
                    .iter()
                    .map(|node| node.title.as_str())
                    .collect::<Vec<_>>(),
                ["Alpha", "Beta"]
            );
            break;
        }
        if Instant::now() >= deadline {
            panic!("stream task did not finish");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn public_mock_server_supports_streaming_task_requests() {
    let server = MockServer::start_sse(
        &["chat-model"],
        &[r#"{"choices":[{"delta":{"content":"- Alpha\n- Beta"}}]}"#],
    );
    let job = AiStreamJob::spawn(
        AiConfig {
            base_url: server.base_url.clone(),
            model: "chat-model".into(),
            ..AiConfig::default()
        },
        AiTask::Brainstorm(ctx()),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(AiStreamEvent::Finished(result)) = job.poll() {
            let result = result.unwrap();
            let AiOutput::Outline(outline) = result.output else {
                panic!("outline result expected");
            };
            assert_eq!(outline.items.len(), 2);
            break;
        }
        if Instant::now() >= deadline {
            panic!("mock SSE task did not finish");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/chat/completions");
    let body: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(body["stream"], true);
}

#[test]
fn streaming_errors_and_missing_terminal_marker_are_not_successes() {
    let (base_url, _) = stream_server(
        "500 Internal Server Error",
        b"private provider detail",
        None,
    );
    let job = AiStreamJob::chat(
        AiConfig {
            base_url,
            model: "test-model".into(),
            ..AiConfig::default()
        },
        "test-model".into(),
        vec![Message::user("hello")],
    );
    let (_, result) = collect_stream(&job);
    assert_eq!(
        result,
        Err(AiError::Status(500, "chat request failed".into()))
    );

    let (base_url, _) = stream_server(
        "200 OK",
        b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
        None,
    );
    let job = AiStreamJob::chat(
        AiConfig {
            base_url,
            model: "test-model".into(),
            ..AiConfig::default()
        },
        "test-model".into(),
        vec![Message::user("hello")],
    );
    let (preview, result) = collect_stream(&job);
    assert_eq!(preview, "partial");
    assert!(matches!(result, Err(AiError::Parse(message)) if message.contains("[DONE]")));
}

#[test]
fn cancellation_before_input_and_during_read_discards_late_deltas() {
    // Never accepted, so the request stays pending until cancellation instead of
    // racing a fast connection refusal.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let before_input = AiStreamJob::chat(
        AiConfig {
            base_url: format!("http://{}/v1", silent.local_addr().unwrap()),
            model: "test-model".into(),
            ..AiConfig::default()
        },
        "test-model".into(),
        vec![Message::user("hello")],
    );
    before_input.cancel();
    assert_eq!(
        before_input.poll(),
        Some(AiStreamEvent::Finished(Err(AiError::Cancelled)))
    );
    assert!(before_input.poll().is_none());

    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"preview\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\" late\"}}]}\n\n",
        "data: [DONE]\n\n"
    );
    let split = body[1..].find("data: ").unwrap() + 1;
    let (release_late_chunk, release) = mpsc::channel();
    let (base_url, _) = stream_server("200 OK", body.as_bytes(), Some((split, release)));
    let during_read = AiStreamJob::chat(
        AiConfig {
            base_url,
            model: "test-model".into(),
            ..AiConfig::default()
        },
        "test-model".into(),
        vec![Message::user("hello")],
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if matches!(during_read.poll(), Some(AiStreamEvent::Delta(delta)) if delta == "preview") {
            break;
        }
        if Instant::now() >= deadline {
            panic!("preview delta was not received");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    during_read.cancel();
    release_late_chunk.send(()).unwrap();
    assert_eq!(
        during_read.poll(),
        Some(AiStreamEvent::Finished(Err(AiError::Cancelled)))
    );
    std::thread::sleep(Duration::from_millis(350));
    assert!(during_read.poll().is_none());
}

/// Live endpoints tried in order: `THINODA_AI_TEST_URL`, the CUDA LM Studio hosts, local LM Studio.
fn live_client() -> Option<Client> {
    let mut urls: Vec<String> = std::env::var("THINODA_AI_TEST_URL")
        .ok()
        .into_iter()
        .collect();
    urls.push("http://192.168.1.216:1234/v1".into());
    urls.push("http://172.27.128.1:1234/v1".into());
    urls.push(thinoda_ai::DEFAULT_BASE_URL.into());
    for url in urls {
        let probe = Client::new(AiConfig {
            base_url: url.clone(),
            timeout_secs: 3,
            ..AiConfig::default()
        });
        if probe.models().is_ok() {
            eprintln!("live AI test using {url}");
            return Some(Client::new(AiConfig {
                base_url: url,
                model: std::env::var("THINODA_AI_TEST_MODEL").unwrap_or_default(),
                temperature: 0.2,
                timeout_secs: 300,
                ..AiConfig::default()
            }));
        }
    }
    None
}

#[test]
fn live_lm_studio_brainstorm_and_map() {
    if std::env::var("THINODA_AI_SKIP_LIVE").is_ok() {
        return;
    }
    let Some(c) = live_client() else {
        eprintln!("no LM Studio server reachable - skipping live AI test");
        return;
    };
    let r = c
        .run(&AiTask::Brainstorm(TopicContext {
            path: vec!["Healthy breakfast".into()],
            ..ctx()
        }))
        .unwrap();
    let AiOutput::Outline(o) = &r.output else {
        panic!()
    };
    eprintln!(
        "brainstorm ({}): {:?}",
        r.model,
        o.items.iter().map(|n| &n.title).collect::<Vec<_>>()
    );
    assert!(o.items.len() >= 3, "expected several ideas, got {o:?}");

    let r = c
        .run(&AiTask::GenerateMap("Plan a small team offsite".into()))
        .unwrap();
    let AiOutput::Outline(o) = &r.output else {
        panic!()
    };
    eprintln!("map: {:?} / {} main topics", o.title, o.items.len());
    assert!(o.title.is_some() && o.items.len() >= 2, "{o:?}");
}
