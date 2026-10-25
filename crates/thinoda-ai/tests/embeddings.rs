#![cfg(not(target_arch = "wasm32"))]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use thinoda_ai::embeddings::*;
use thinoda_ai::{AiConfig, AiEmbeddingJob, AiError, Client, Result};

fn config(base_url: String) -> AiConfig {
    AiConfig {
        base_url,
        // Must never be used by embeddings.
        model: "chat-model-not-for-embeddings".into(),
        api_key: "embedding-test-key".into(),
        timeout_secs: 2,
        ..AiConfig::default()
    }
}

fn texts() -> Vec<String> {
    vec!["first".into(), "second".into()]
}

fn read_request(stream: &TcpStream) -> (String, String, Value) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut path = String::new();
    reader.read_line(&mut path).unwrap();
    let mut length = 0;
    let mut auth = String::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.trim().is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').unwrap();
        if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().unwrap();
        }
        if name.eq_ignore_ascii_case("authorization") {
            auth = value.trim().into();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    (path, auth, serde_json::from_slice(&body).unwrap())
}

fn respond(stream: &mut TcpStream, status: u16, body: &[u8]) {
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    // Oversize/cancellation tests may close the connection before the body is sent.
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn server(
    status: u16,
    body: Vec<u8>,
) -> (
    String,
    Receiver<(String, String, Value)>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = tx.send(read_request(&stream));
        respond(&mut stream, status, &body);
    });
    (url, rx, worker)
}

fn collect(job: &AiEmbeddingJob) -> Result<Vec<Vec<f32>>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = job.poll() {
            assert!(job.is_finished());
            assert_eq!(job.poll(), None, "terminal result must be consumed once");
            return result;
        }
        assert!(Instant::now() < deadline, "embedding job did not finish");
        thread::sleep(Duration::from_millis(1));
    }
}

fn exercise(status: u16, body: &[u8], inputs: &[String]) -> Vec<Result<Vec<Vec<f32>>>> {
    [false, true].into_iter().map(|job| {
        let (url, received, worker) = server(status, body.to_vec());
        let result = if job {
            collect(&AiEmbeddingJob::spawn(config(url), "explicit-embedding-model".into(), inputs.to_vec()))
        } else {
            Client::new(config(url)).embeddings("explicit-embedding-model", inputs)
        };
        let (path, auth, body) = received.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(path.trim(), "POST /v1/embeddings HTTP/1.1");
        assert_eq!(auth, "Bearer embedding-test-key");
        assert_eq!(body, json!({
            "model": "explicit-embedding-model", "input": inputs, "encoding_format": "float",
        }));
        worker.join().unwrap();
        result
    }).collect()
}

#[test]
fn returns_ordered_and_reordered_vectors_without_modifying_them() {
    for entries in [
        json!([{"index":0,"embedding":[1.25,-2.5]},{"index":1,"embedding":[0.0,3.0]}]),
        json!([{"index":1,"embedding":[0.0,3.0]},{"index":0,"embedding":[1.25,-2.5]}]),
    ] {
        let body = json!({"data":entries,"usage":{"total_tokens":2}}).to_string();
        for result in exercise(200, body.as_bytes(), &texts()) {
            assert_eq!(result.unwrap(), vec![vec![1.25, -2.5], vec![0.0, 3.0]]);
        }
    }
}

#[test]
fn rejects_invalid_responses_on_both_native_paths() {
    let bodies = [
        ("not JSON", "invalid response"),
        ("{}", "data"),
        (r#"{"data":[]}"#, "expected 2"),
        (r#"{"data":[{"index":0,"embedding":[1]}]}"#, "expected 2"),
        (
            r#"{"data":[{"index":0,"embedding":[1]},{"index":0,"embedding":[2]}]}"#,
            "duplicate",
        ),
        (
            r#"{"data":[{"index":0,"embedding":[1]},{"index":2,"embedding":[2]}]}"#,
            "out of range",
        ),
        (
            r#"{"data":[{"embedding":[1]},{"index":1,"embedding":[2]}]}"#,
            "index",
        ),
        (
            r#"{"data":[{"index":-1,"embedding":[1]},{"index":1,"embedding":[2]}]}"#,
            "invalid response",
        ),
        (
            r#"{"data":[{"index":0.5,"embedding":[1]},{"index":1,"embedding":[2]}]}"#,
            "invalid response",
        ),
        (
            r#"{"data":[{"index":"0","embedding":[1]},{"index":1,"embedding":[2]}]}"#,
            "invalid response",
        ),
        (
            r#"{"data":[{"index":null,"embedding":[1]},{"index":1,"embedding":[2]}]}"#,
            "invalid response",
        ),
        (
            r#"{"data":[{"index":0,"embedding":[]},{"index":1,"embedding":[]}]}"#,
            "nonzero",
        ),
        (
            r#"{"data":[{"index":0,"embedding":[1]},{"index":1,"embedding":[1,2]}]}"#,
            "inconsistent",
        ),
        (
            r#"{"data":[{"index":0,"embedding":[1e39]},{"index":1,"embedding":[2]}]}"#,
            "finite",
        ),
        (
            r#"{"data":[{"index":0,"embedding":[NaN]},{"index":1,"embedding":[2]}]}"#,
            "invalid response",
        ),
        (
            r#"{"data":[{"index":0,"embedding":[Infinity]},{"index":1,"embedding":[2]}]}"#,
            "invalid response",
        ),
        (
            r#"{"data":[{"index":0,"embedding":[null]},{"index":1,"embedding":[2]}]}"#,
            "invalid response",
        ),
        (
            r#"{"data":[{"index":0,"embedding":["1"]},{"index":1,"embedding":[2]}]}"#,
            "invalid response",
        ),
        (
            r#"{"data":[{"index":0,"embedding":"base64"},{"index":1,"embedding":[2]}]}"#,
            "invalid response",
        ),
    ];
    for (body, message) in bodies {
        for result in exercise(200, body.as_bytes(), &texts()) {
            assert!(
                matches!(result, Err(AiError::Parse(ref text)) if text.contains(message)),
                "{body}: {result:?}"
            );
        }
    }
}

#[test]
fn bounds_response_count_dimensions_and_raw_bytes() {
    for (count, dimensions, expected) in [(129, 1, "128"), (2, 4097, "4096"), (3, 1, "expected 2")]
    {
        let data: Vec<_> = (0..count)
            .map(|index| json!({"index":index,"embedding":vec![0;dimensions]}))
            .collect();
        let body = json!({"data":data}).to_string();
        for result in exercise(200, body.as_bytes(), &texts()) {
            assert!(
                matches!(result, Err(AiError::Parse(ref text)) if text.contains(expected)),
                "{result:?}"
            );
        }
    }
    let mut body =
        br#"{"data":[{"index":0,"embedding":[1]},{"index":1,"embedding":[2]}]}"#.to_vec();
    body.resize(MAX_RESPONSE_BYTES, b' ');
    for result in exercise(200, &body, &texts()) {
        assert!(result.is_ok(), "{result:?}");
    }
    body.push(b' ');
    for status in [200, 500] {
        for result in exercise(status, &body, &texts()) {
            assert!(
                matches!(result, Err(AiError::Parse(ref text)) if text.contains("2097152")),
                "{result:?}"
            );
        }
    }
}

#[test]
fn accepts_maximum_vector_allocation_and_input_boundaries() {
    let inputs = vec!["x".into(); MAX_INPUTS];
    let data: Vec<_> = (0..MAX_INPUTS)
        .rev()
        .map(|index| json!({"index":index,"embedding":vec![0;MAX_DIMENSIONS]}))
        .collect();
    for result in exercise(200, json!({"data":data}).to_string().as_bytes(), &inputs) {
        let vectors = result.unwrap();
        assert_eq!(vectors.len(), MAX_INPUTS);
        assert!(vectors.iter().all(|v| v.len() == MAX_DIMENSIONS));
        assert_eq!(
            vectors
                .iter()
                .map(|v| v.len() * size_of::<f32>())
                .sum::<usize>(),
            MAX_VECTOR_BYTES
        );
    }
    // Exactly 16 KiB/text and 256 KiB/batch, including multibyte UTF-8.
    let inputs = vec!["é".repeat(MAX_TEXT_BYTES / 2); MAX_INPUT_BYTES / MAX_TEXT_BYTES];
    let data: Vec<_> = (0..inputs.len())
        .map(|index| json!({"index":index,"embedding":[1]}))
        .collect();
    assert!(
        exercise(200, json!({"data":data}).to_string().as_bytes(), &inputs)
            .into_iter()
            .all(|v| v.is_ok())
    );
}

#[test]
fn invalid_inputs_never_reach_the_loopback_listener_or_model_discovery() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let cfg = config(format!("http://{}/v1", listener.local_addr().unwrap()));
    for (model, inputs, expected) in [
        ("".into(), texts(), "explicit model"),
        (" \n".into(), texts(), "explicit model"),
        ("m".repeat(MAX_MODEL_BYTES + 1), texts(), "explicit model"),
        ("m".into(), vec![], "1..=128"),
        ("m".into(), vec!["x".into(); MAX_INPUTS + 1], "1..=128"),
        ("m".into(), vec![" ".into()], "nonblank"),
        (
            "m".into(),
            vec!["é".repeat(MAX_TEXT_BYTES / 2 + 1)],
            "16384",
        ),
        ("m".into(), vec!["x".repeat(MAX_TEXT_BYTES); 17], "262144"),
    ] {
        let direct = Client::new(cfg.clone()).embeddings(&model, &inputs);
        let job = collect(&AiEmbeddingJob::spawn(cfg.clone(), model, inputs));
        for result in [direct, job] {
            assert!(
                matches!(result, Err(AiError::Parse(ref text)) if text.contains(expected)),
                "{result:?}"
            );
        }
        assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    }
}

#[test]
fn reports_http_unsupported_auth_and_server_failures_without_fallback() {
    for status in [400, 401, 404, 405, 415, 422, 429, 500, 501] {
        for result in exercise(
            status,
            br#"{"error":{"message":"embedding model unsupported"}}"#,
            &texts(),
        ) {
            assert_eq!(
                result,
                Err(AiError::Status(
                    status,
                    "embedding model unsupported".into()
                ))
            );
        }
    }
    for result in exercise(404, b"<html>not supported</html>", &texts()) {
        assert!(
            matches!(result, Err(AiError::Status(404, text)) if text.contains("endpoint rejected"))
        );
    }
}

#[test]
fn cancellation_aborts_pending_io_and_suppresses_late_completion() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let cfg = config(format!("http://{}/v1", listener.local_addr().unwrap()));
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&stream);
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        respond(
            &mut stream,
            200,
            br#"{"data":[{"index":0,"embedding":[1]},{"index":1,"embedding":[2]}]}"#,
        );
    });
    let job = AiEmbeddingJob::spawn(cfg, "embedding-model".into(), texts());
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(!job.is_finished());
    job.cancel();
    job.cancel();
    assert_eq!(collect(&job), Err(AiError::Cancelled));
    release_tx.send(()).unwrap();
    worker.join().unwrap();
    thread::sleep(Duration::from_millis(30));
    assert_eq!(job.poll(), None);
}

#[test]
fn completion_wins_over_late_cancel_and_drop_aborts_pending_io() {
    let (url, _, worker) = server(200, br#"{"data":[{"index":0,"embedding":[1]}]}"#.to_vec());
    let job = AiEmbeddingJob::spawn(config(url), "m".into(), vec!["x".into()]);
    let deadline = Instant::now() + Duration::from_secs(3);
    while !job.is_finished() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    job.cancel();
    assert_eq!(collect(&job).unwrap(), vec![vec![1.0]]);
    worker.join().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let cfg = config(format!("http://{}/v1", listener.local_addr().unwrap()));
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&stream);
        tx.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(
            stream.read(&mut byte).unwrap(),
            0,
            "dropping must close native socket"
        );
    });
    let job = AiEmbeddingJob::spawn(cfg, "m".into(), texts());
    rx.recv_timeout(Duration::from_secs(3)).unwrap();
    drop(job);
    worker.join().unwrap();
}
