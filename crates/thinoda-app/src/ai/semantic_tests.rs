use super::*;
use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

struct Server {
    url: String,
    requests: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn start(mode: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (records, done) = (requests.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(stream) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("loopback accept: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                let (path, body) = loop {
                    let count = match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break (String::new(), None),
                        Ok(count) => count,
                    };
                    bytes.extend_from_slice(&buffer[..count]);
                    assert!(bytes.len() < 1_048_576);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length: usize = headers
                            .lines()
                            .find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                key.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            break (
                                headers
                                    .lines()
                                    .next()
                                    .unwrap()
                                    .split_whitespace()
                                    .nth(1)
                                    .unwrap()
                                    .to_string(),
                                Some(
                                    serde_json::from_slice::<serde_json::Value>(
                                        &bytes[end + 4..end + 4 + length],
                                    )
                                    .unwrap(),
                                ),
                            );
                        }
                    }
                };
                let Some(body) = body else {
                    continue;
                };
                let mut records = records.lock().unwrap();
                records.push((path, body.clone()));
                let sequence = records.len();
                drop(records);
                if mode == "slow" {
                    std::thread::sleep(Duration::from_millis(80));
                }
                let inputs = body["input"].as_array().unwrap();
                let data: Vec<_> = inputs
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(index, text)| {
                        let lower = text.as_str().unwrap().to_lowercase();
                        let vector = if mode == "zero" {
                            vec![0.0, 0.0]
                        } else if mode == "dimensions" && sequence > 1 {
                            vec![1.0, 0.0, 0.0]
                        } else if lower.contains("apple") || lower.contains("fruit") {
                            vec![1.0, 0.0]
                        } else if lower.contains("tool") {
                            vec![0.0, 1.0]
                        } else {
                            vec![-1.0, 0.0]
                        };
                        serde_json::json!({"index":index, "embedding":vector})
                    })
                    .collect();
                let response = if mode == "unsupported" {
                    serde_json::json!({"error":{"message":"embedding model unsupported"}})
                } else {
                    serde_json::json!({"data":data})
                }
                .to_string();
                let status = if mode == "unsupported" {
                    "400 Bad Request"
                } else {
                    "200 OK"
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                );
                // Cancellation can close the socket before the fixture writes its response.
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        Self {
            url,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn app(server: &Server) -> ThinodaApp {
    let mut app = ThinodaApp::headless();
    app.settings.ai.base_url = server.url.clone();
    app.settings.ai.embedding_model = "explicit-vectors".into();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Titles", |sheet| {
            sheet.set_title(root, "Fruit plan")?;
            sheet.set_notes(root, Some("Never transmit private notes".into()))?;
            sheet.insert_child(root, "Apple review")?;
            sheet.insert_child(root, "Tool design")?;
            Ok(())
        })
        .unwrap();
    app
}

fn wait(app: &mut ThinodaApp, ctx: &Context) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while app.ai.semantic.pending.is_some() || app.ai.semantic.clustering.is_some() {
        assert!(std::time::Instant::now() < deadline);
        app.poll_semantic(ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn normalized_similarity_and_incremental_clustering_are_deterministic_and_complete() {
    assert!(normalize(vec![vec![0.0, 0.0]]).is_err());
    let vectors = normalize(vec![
        vec![10.0, 0.0],
        vec![2.0, 0.1],
        vec![0.0, 20.0],
        vec![0.1, 3.0],
    ])
    .unwrap();
    assert!((cosine(&vectors[0], &vectors[0]) - 1.0).abs() < 0.0001);
    let run = || {
        let mut job = ClusterJob::new(&vectors, 2);
        for _ in 0..100 {
            if let Some(groups) = job.step(&vectors) {
                return groups;
            }
        }
        panic!("cluster job did not finish");
    };
    let groups = run();
    assert_eq!(groups, vec![vec![0, 1], vec![2, 3]]);
    assert_eq!(groups, run());
    let duplicates = vec![vec![1.0, 0.0]; 40];
    let mut job = ClusterJob::new(&duplicates, 8);
    let mut steps = 0;
    loop {
        steps += 1;
        if let Some(groups) = job.step(&duplicates) {
            assert_eq!(groups, vec![(0..40).collect::<Vec<_>>()]);
            break;
        }
        assert!(steps < 100);
    }
    assert!(
        steps > 8,
        "work is chunked rather than finished in one frame"
    );
}

#[test]
fn semantic_index_query_and_clusters_are_explicit_and_undoable_without_source_loss() {
    let server = Server::start("normal");
    let mut app = app(&server);
    let ctx = Context::default();
    assert!(server.requests.lock().unwrap().is_empty());
    let before = app.doc().unwrap().doc.workbook().clone();
    app.start_semantic_index().unwrap();
    wait(&mut app, &ctx);
    assert!(app.ai.semantic.cache.is_some());
    app.ai.semantic.query = "fruit".into();
    app.start_semantic_query().unwrap();
    wait(&mut app, &ctx);
    let source = app.ai.semantic.cache.as_ref().unwrap();
    let title = |id| {
        &source
            .source
            .topics
            .iter()
            .find(|(topic, _)| *topic == id)
            .unwrap()
            .1
    };
    assert!(title(app.ai.semantic.results[0].0).contains("Fruit"));
    assert!(title(app.ai.semantic.results[1].0).contains("Apple"));
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.ai.semantic.clustering = Some(ClusterJob::new(&source.vectors, 3));
    wait(&mut app, &ctx);
    assert!(!app.ai.semantic.clusters.is_empty());
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|(path, body)| path == "/v1/embeddings"
        && body["model"] == "explicit-vectors"
        && !body.to_string().contains("Never transmit")));
    drop(requests);
    app.apply_semantic_clusters().unwrap();
    assert_eq!(
        app.doc().unwrap().doc.workbook().sheets[0],
        before.sheets[0]
    );
    assert_eq!(
        app.doc().unwrap().doc.workbook().sheets.len(),
        before.sheets.len() + 1
    );
    assert!(app.doc_mut().unwrap().doc.undo());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn semantic_batches_are_bounded_and_dimension_changes_fail_visibly() {
    for mode in ["normal", "dimensions", "zero", "unsupported"] {
        let server = Server::start(mode);
        let mut app = app(&server);
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Batch fixture", |sheet| {
                for index in 0..130 {
                    sheet.insert_child(root, &format!("Title {index}"))?;
                }
                Ok(())
            })
            .unwrap();
        let before = app.doc().unwrap().doc.workbook().clone();
        app.start_semantic_index().unwrap();
        wait(&mut app, &Context::default());
        assert_eq!(
            app.ai.semantic.cache.is_some(),
            mode == "normal",
            "mode={mode}, error={:?}",
            app.ai.semantic.error
        );
        assert_eq!(app.ai.semantic.error.is_some(), mode != "normal");
        assert!(
            server
                .requests
                .lock()
                .unwrap()
                .iter()
                .all(|(_, body)| body["input"].as_array().unwrap().len() <= 128)
        );
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn semantic_cancellation_edits_model_changes_and_source_close_discard_late_work() {
    for stop in ["cancel", "edit", "model", "disable", "close"] {
        let server = Server::start("slow");
        let mut app = app(&server);
        let ctx = Context::default();
        app.start_semantic_index().unwrap();
        match stop {
            "cancel" => app.cancel_semantic(),
            "edit" => {
                let doc = app.doc_mut().unwrap();
                let root = doc.doc.sheet().root;
                doc.doc
                    .edit_sheet("Edit", |sheet| sheet.set_title(root, "Edited"))
                    .unwrap();
            }
            "model" => app.settings.ai.embedding_model = "other-model".into(),
            "disable" => app.settings.ai.enabled = false,
            "close" => app.docs.clear(),
            _ => unreachable!(),
        }
        app.poll_semantic(&ctx);
        assert!(app.ai.semantic.pending.is_none());
        std::thread::sleep(Duration::from_millis(100));
        app.poll_semantic(&ctx);
        assert!(app.ai.semantic.cache.is_none());
    }
}

#[test]
fn semantic_cache_invalidates_after_edit_and_blank_model_never_uses_chat_model() {
    let server = Server::start("normal");
    let mut app = app(&server);
    app.settings.ai.embedding_model.clear();
    app.settings.ai.model = "not-an-embedding-fallback".into();
    assert!(app.start_semantic_index().is_err());
    assert!(server.requests.lock().unwrap().is_empty());
    app.settings.ai.embedding_model = "explicit-vectors".into();
    app.start_semantic_index().unwrap();
    let ctx = Context::default();
    wait(&mut app, &ctx);
    app.settings.ai.embedding_model = "changed".into();
    app.poll_semantic(&ctx);
    assert!(app.ai.semantic.cache.is_none());
    assert!(app.ai.semantic.error.is_some());
}

#[test]
fn semantic_panel_opens_offline_and_build_requires_explicit_ui_activation() {
    let server = Server::start("slow");
    let mut app = app(&server);
    app.ai.chat.open = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 760.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(3);
    h.get_by_label("Semantic tools").click();
    h.run_steps(3);
    assert!(server.requests.lock().unwrap().is_empty());
    h.get_by_role_and_label(Role::Button, "Build semantic index")
        .click();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        h.run_steps(3);
        let semantic = &h.state().ai.semantic;
        assert!(semantic.error.is_none(), "{:?}", semantic.error);
        if semantic.cache.is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Build did not produce an index; pending={}, requests={:?}",
            semantic.pending.is_some(),
            server.requests.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[test]
fn semantic_bounds_are_checked_before_any_network_request() {
    let server = Server::start("normal");
    let mut app = app(&server);
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Maximum titles", |sheet| {
            sheet.set_title(root, &"x".repeat(16_384))?;
            let children = sheet.topics[&root].children.clone();
            sheet.remove_topics(&children)?;
            for _ in 0..3 {
                sheet.insert_child(root, &"x".repeat(16_384))?;
            }
            Ok(())
        })
        .unwrap();
    assert!(app.semantic_source().is_ok(), "exact 64 KiB is accepted");
    let doc = app.doc_mut().unwrap();
    doc.doc
        .edit_sheet("Over total", |sheet| {
            sheet.insert_child(root, "x")?;
            Ok(())
        })
        .unwrap();
    assert!(app.start_semantic_index().is_err());
    assert!(app.doc_mut().unwrap().doc.undo());
    let doc = app.doc_mut().unwrap();
    doc.doc
        .edit_sheet("Over title", |sheet| {
            sheet.set_title(root, &"x".repeat(16_385))
        })
        .unwrap();
    assert!(app.start_semantic_index().is_err());
    assert!(app.doc_mut().unwrap().doc.undo());
    let doc = app.doc_mut().unwrap();
    doc.doc
        .edit_sheet("Topic limit", |sheet| {
            for topic in sheet.topics.values_mut() {
                topic.title = "x".into();
            }
            for _ in 4..500 {
                sheet.insert_child(root, "x")?;
            }
            Ok(())
        })
        .unwrap();
    assert!(
        app.semantic_source().is_ok(),
        "exact 500 topics is accepted"
    );
    let doc = app.doc_mut().unwrap();
    doc.doc
        .edit_sheet("Too many topics", |sheet| {
            sheet.insert_child(root, "x")?;
            Ok(())
        })
        .unwrap();
    assert!(app.start_semantic_index().is_err());
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn semantic_query_changes_and_cached_source_edits_reject_late_results_and_apply() {
    let server = Server::start("slow");
    let mut app = app(&server);
    let ctx = Context::default();
    app.start_semantic_index().unwrap();
    wait(&mut app, &ctx);
    app.ai.semantic.query = "x".repeat(8193);
    assert!(app.start_semantic_query().is_err());
    app.ai.semantic.query = "fruit".into();
    app.start_semantic_query().unwrap();
    app.ai.semantic.query = "changed".into();
    app.poll_semantic(&ctx);
    assert!(app.ai.semantic.pending.is_none());
    std::thread::sleep(Duration::from_millis(100));
    app.poll_semantic(&ctx);
    assert!(app.ai.semantic.results.is_empty());
    let cache = app.ai.semantic.cache.as_ref().unwrap();
    app.ai.semantic.clustering = Some(ClusterJob::new(&cache.vectors, 2));
    wait(&mut app, &ctx);
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Edit cached source", |sheet| {
            sheet.set_title(root, "Changed")
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(app.apply_semantic_clusters().is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.poll_semantic(&ctx);
    assert!(app.ai.semantic.cache.is_none());
    assert!(app.ai.semantic.clusters.is_empty());
}
