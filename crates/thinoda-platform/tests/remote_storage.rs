#![cfg(not(target_arch = "wasm32"))]

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use storage::{CancelToken, ExpectedRevision, Revision, StorageError, StorageId, StorageProvider};
use thinoda_platform::remote_storage::*;
use thinoda_platform::storage;

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn request(stream: &mut TcpStream) -> Request {
    // Windows can inherit the listener's nonblocking mode on accepted sockets.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        bytes.push(byte[0]);
        assert!(bytes.len() < 64 * 1024);
    }
    let head = String::from_utf8(bytes).unwrap();
    let mut lines = head.split("\r\n");
    let mut start = lines.next().unwrap().split(' ');
    let method = start.next().unwrap().into();
    let path = start.next().unwrap().into();
    let headers: BTreeMap<String, String> = lines
        .filter_map(|line| {
            line.split_once(':')
                .map(|(key, value)| (key.to_lowercase(), value.trim().into()))
        })
        .collect();
    let length = headers
        .get("content-length")
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    Request {
        method,
        path,
        headers,
        body,
    }
}

fn respond(stream: &mut TcpStream, status: u16, headers: &[(&str, &str)], bytes: &[u8]) {
    write!(
        stream,
        "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n",
        bytes.len()
    )
    .unwrap();
    for (key, value) in headers {
        write!(stream, "{key}: {value}\r\n").unwrap();
    }
    stream.write_all(b"\r\n").unwrap();
    stream.write_all(bytes).unwrap();
}

fn server(
    count: usize,
    handler: impl Fn(usize, Request, &mut TcpStream) + Send + 'static,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let thread = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        for index in 0..count {
            let start = std::time::Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            start.elapsed() < Duration::from_secs(10),
                            "expected request {index}"
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            let request = request(&mut stream);
            handler(index, request, &mut stream);
        }
    });
    (url, thread)
}

fn run(future: impl std::future::Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future);
}

fn dav(url: String) -> WebDavProvider {
    WebDavProvider::new(WebDavConfig {
        collection_url: format!("{url}dav/"),
        authentication: Authentication::Basic {
            username: "mock-user".into(),
            password: "mock-password".into(),
        },
    })
    .unwrap()
}

#[test]
fn webdav_list_read_and_atomic_conditions() {
    let stored = Arc::new(Mutex::new(b"original".to_vec()));
    let remote = stored.clone();
    let (url, thread) = server(6, move |index, request, stream| {
        assert!(request.headers["authorization"].starts_with("Basic "));
        match index {
            0 => {
                assert_eq!(request.method, "PROPFIND");
                assert_eq!(request.path, "/dav/");
                assert_eq!(request.headers["depth"], "1");
                assert!(
                    String::from_utf8(request.body)
                        .unwrap()
                        .contains("<d:getetag/>")
                );
                respond(stream, 207, &[], br#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/dav/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response><d:response><d:href>/dav/map.thinoda</d:href><d:propstat><d:prop><d:displayname>Map</d:displayname><d:getcontentlength>8</d:getcontentlength><d:getetag>"r1"</d:getetag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat><d:propstat><d:prop><d:getetag>"bad"</d:getetag></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response></d:multistatus>"#);
            }
            1 => {
                assert_eq!(request.method, "GET");
                respond(
                    stream,
                    200,
                    &[("ETag", "\"download-revision\"")],
                    &remote.lock().unwrap(),
                );
            }
            2 | 3 => {
                assert_eq!(request.method, "PUT");
                assert_eq!(request.headers["if-match"], "\"stale\"");
                assert_eq!(request.body, b"local edits");
                respond(
                    stream,
                    if index == 2 { 412 } else { 409 },
                    &[("ETag", "\"r1\"")],
                    b"private error body",
                );
            }
            4 => {
                assert_eq!(request.headers["if-none-match"], "*");
                assert!(!request.headers.contains_key("if-match"));
                respond(stream, 412, &[], b"already exists");
            }
            5 => {
                assert_eq!(request.headers["if-match"], "\"r1\"");
                *remote.lock().unwrap() = request.body;
                respond(stream, 204, &[("ETag", "\"r2\"")], b"");
            }
            _ => unreachable!(),
        }
    });
    let provider = dav(url);
    run(async {
        let cancel = CancelToken::default();
        let root = provider.id("").unwrap();
        let list = provider.list(&root, cancel.clone()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Map");
        assert_eq!(list[0].size, 8);
        assert_eq!(list[0].revision, Some(Revision("\"r1\"".into())));
        let id = provider.id("map.thinoda").unwrap();
        assert_eq!(list[0].id, id);
        let read = provider.read(&id, cancel.clone()).await.unwrap();
        assert_eq!(read.bytes, b"original");
        assert_eq!(read.revision, Revision("\"download-revision\"".into()));
        for _ in 0..2 {
            assert!(matches!(
                provider.write(&id, b"local edits", ExpectedRevision::Match(Revision("\"stale\"".into())), cancel.clone()).await,
                Err(StorageError::Conflict { actual: Some(Revision(ref value)) }) if value == "\"r1\""
            ));
            assert_eq!(*stored.lock().unwrap(), b"original");
        }
        assert!(matches!(
            provider
                .write(
                    &id,
                    b"local edits",
                    ExpectedRevision::Missing,
                    cancel.clone()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
        assert_eq!(
            provider
                .write(
                    &id,
                    b"local edits",
                    ExpectedRevision::Match(Revision("\"r1\"".into())),
                    cancel.clone()
                )
                .await
                .unwrap(),
            Revision("\"r2\"".into())
        );
    });
    thread.join().unwrap();
}

#[test]
fn webdav_errors_are_redacted_and_redirects_rejected() {
    let (url, thread) = server(2, |index, request, stream| {
        assert_eq!(request.method, "GET");
        if index == 0 {
            respond(stream, 401, &[], b"SECRET account contents");
        } else {
            respond(
                stream,
                302,
                &[("Location", "https://untrusted.invalid/steal")],
                b"",
            );
        }
    });
    let provider = dav(url);
    run(async {
        let cancel = CancelToken::default();
        let id = provider.id("map").unwrap();
        for status in [401, 302] {
            let error = provider.read(&id, cancel.clone()).await.unwrap_err();
            assert!(
                matches!(error, StorageError::Remote(ref value) if value == &format!("HTTP {status}"))
            );
            assert!(!error.to_string().contains("SECRET"));
        }
    });
    thread.join().unwrap();
}

#[test]
fn configuration_and_precancel_do_not_contact_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let provider = dav(format!("http://{}/", listener.local_addr().unwrap()));
    run(async {
        let cancel = CancelToken::default();
        cancel.cancel();
        assert!(matches!(
            provider
                .read(&provider.id("file").unwrap(), cancel.clone())
                .await,
            Err(StorageError::Cancelled)
        ));
        assert!(
            provider
                .read(
                    &StorageId("https://untrusted.invalid/file".into()),
                    CancelToken::default()
                )
                .await
                .is_err()
        );
    });
    assert!(listener.accept().is_err());
    for url in [
        "http://example.invalid/",
        "https://user:password@example.invalid/",
        "https://example.invalid/#secret",
    ] {
        assert!(
            WebDavProvider::new(WebDavConfig {
                collection_url: url.into(),
                authentication: Authentication::Bearer("mock".into())
            })
            .is_err()
        );
    }
}

#[test]
fn cancellation_interrupts_waiting_for_response() {
    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let (url, thread) = server(1, move |_, _, _| {
        seen_tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    let provider = dav(url);
    let cancel = CancelToken::default();
    let other = cancel.clone();
    let trigger = thread::spawn(move || {
        seen_rx.recv().unwrap();
        other.cancel();
    });
    run(async {
        assert!(matches!(
            provider
                .read(&provider.id("file").unwrap(), cancel.clone())
                .await,
            Err(StorageError::Cancelled)
        ));
    });
    trigger.join().unwrap();
    thread.join().unwrap();
}

#[test]
fn dropbox_wire_contract_and_revision_conflict() {
    let stored = Arc::new(Mutex::new(b"original".to_vec()));
    let remote = stored.clone();
    let (url, thread) = server(5, move |index, request, stream| {
        assert_eq!(request.headers["authorization"], "Bearer mock-token");
        assert_eq!(request.method, "POST");
        match index {
            0 => {
                assert_eq!(request.path, "/api/files/list_folder");
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(body["path"], "");
                assert_eq!(body["recursive"], false);
                respond(stream, 200, &[], br#"{"entries":[{".tag":"file","id":"id:stable","name":"Map","size":8,"rev":"r1"}],"has_more":true,"cursor":"cursor1"}"#);
            }
            1 => {
                assert_eq!(request.path, "/api/files/list_folder/continue");
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["cursor"],
                    "cursor1"
                );
                respond(stream, 200, &[], br#"{"entries":[],"has_more":false}"#);
            }
            2 => {
                assert_eq!(request.path, "/content/files/download");
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&request.headers["dropbox-api-arg"])
                        .unwrap()["path"],
                    "id:stable"
                );
                respond(
                    stream,
                    200,
                    &[("Dropbox-API-Result", r#"{"rev":"download-rev"}"#)],
                    &remote.lock().unwrap(),
                );
            }
            3 | 4 => {
                assert_eq!(request.path, "/content/files/upload");
                let arg: serde_json::Value =
                    serde_json::from_str(&request.headers["dropbox-api-arg"]).unwrap();
                assert_eq!(arg["mode"][".tag"], "update");
                assert_eq!(
                    arg["mode"]["update"],
                    if index == 3 { "stale" } else { "r1" }
                );
                assert_eq!(arg["autorename"], false);
                assert_eq!(arg["strict_conflict"], true);
                assert_eq!(request.body, b"local edits");
                if index == 3 {
                    respond(stream, 409, &[], b"SECRET conflict");
                } else {
                    *remote.lock().unwrap() = request.body;
                    respond(stream, 200, &[], br#"{"rev":"r2"}"#);
                }
            }
            _ => unreachable!(),
        }
    });
    let provider = DropboxProvider::new(DropboxConfig {
        api_url: format!("{url}api/"),
        content_url: format!("{url}content/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    run(async {
        let cancel = CancelToken::default();
        let entries = provider
            .list(&StorageId("".into()), cancel.clone())
            .await
            .unwrap();
        assert_eq!(entries[0].id, StorageId("id:stable".into()));
        assert_eq!(entries[0].size, 8);
        assert_eq!(entries[0].revision, Some(Revision("r1".into())));
        let read = provider.read(&entries[0].id, cancel.clone()).await.unwrap();
        assert_eq!(read.bytes, b"original");
        assert_eq!(read.revision, Revision("download-rev".into()));
        assert!(matches!(
            provider
                .write(
                    &entries[0].id,
                    b"local edits",
                    ExpectedRevision::Match(Revision("stale".into())),
                    cancel.clone()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
        assert_eq!(*stored.lock().unwrap(), b"original");
        assert_eq!(
            provider
                .write(
                    &entries[0].id,
                    b"local edits",
                    ExpectedRevision::Match(Revision("r1".into())),
                    cancel.clone()
                )
                .await
                .unwrap(),
            Revision("r2".into())
        );
    });
    thread.join().unwrap();
}

#[test]
fn box_conditions_multipart_versions_and_explicit_sharing() {
    let (url, thread) = server(5, |index, request, stream| {
        assert_eq!(request.headers["authorization"], "Bearer mock-token");
        match index {
            0 => {
                assert_eq!(request.method, "GET");
                assert!(request.path.starts_with("/api/folders/0/items?"));
                assert!(
                    request
                        .path
                        .contains("fields=id%2Cname%2Csize%2Cetag%2Ctype")
                );
                respond(stream, 200, &[], br#"{"entries":[{"type":"file","id":"file1","name":"Map","size":8,"etag":"1"}],"next_marker":null}"#);
            }
            1 => {
                assert_eq!(request.path, "/api/files/file1/content");
                respond(stream, 200, &[("ETag", "download-etag")], b"original");
            }
            2 => {
                assert_eq!(request.method, "POST");
                assert_eq!(request.path, "/upload/files/file1/content");
                assert_eq!(request.headers["if-match"], "stale");
                let body = String::from_utf8(request.body).unwrap();
                assert!(
                    body.find("name=\"attributes\"").unwrap() < body.find("name=\"file\"").unwrap()
                );
                assert!(body.contains("\r\n\r\n{}\r\n"));
                assert!(body.contains("\r\n\r\nlocal edits\r\n"));
                respond(stream, 412, &[], b"private conflict");
            }
            3 => {
                assert!(request.path.starts_with("/api/files/file1/versions?"));
                respond(stream, 200, &[], br#"{"entries":[{"id":"version1","size":8,"modified_at":"2026-01-01"}],"total_count":1}"#);
            }
            4 => {
                assert_eq!(request.method, "PUT");
                let data: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(data["shared_link"]["access"], "collaborators");
                // Loopback mock links must stay on the explicitly configured mock origin.
                let host = &request.headers["host"];
                let data = format!(r#"{{"shared_link":{{"url":"http://{host}/share/mock"}}}}"#);
                respond(stream, 200, &[], data.as_bytes());
            }
            _ => unreachable!(),
        }
    });
    let provider = BoxProvider::new(BoxConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    run(async {
        let cancel = CancelToken::default();
        let entries = provider
            .list(&StorageId("0".into()), cancel.clone())
            .await
            .unwrap();
        assert_eq!(entries[0].id, StorageId("file1".into()));
        assert_eq!(entries[0].size, 8);
        assert_eq!(entries[0].revision, Some(Revision("1".into())));
        let read = provider.read(&entries[0].id, cancel.clone()).await.unwrap();
        assert_eq!(read.bytes, b"original");
        assert_eq!(read.revision, Revision("download-etag".into()));
        assert!(matches!(
            provider
                .write(
                    &entries[0].id,
                    b"local edits",
                    ExpectedRevision::Match(Revision("stale".into())),
                    cancel.clone()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
        assert_eq!(
            provider
                .list_versions(&entries[0].id, cancel.clone())
                .await
                .unwrap()[0]
                .id,
            "version1"
        );
        assert!(
            provider
                .create_share_link(
                    &entries[0].id,
                    BoxShareAccess::Collaborators,
                    cancel.clone()
                )
                .await
                .unwrap()
                .0
                .ends_with("/share/mock")
        );
        assert!(matches!(
            provider
                .write(&entries[0].id, b"x", ExpectedRevision::Missing, cancel)
                .await,
            Err(StorageError::Unsupported)
        ));
    });
    thread.join().unwrap();
}

#[test]
fn drive_v3_uses_media_etag_not_list_or_postread_metadata() {
    let (url, thread) = server(3, |index, request, stream| {
        assert_eq!(request.headers["authorization"], "Bearer mock-token");
        match index {
            0 => {
                assert!(request.path.starts_with("/api/files?"));
                assert!(
                    request
                        .path
                        .contains("fields=nextPageToken%2Cfiles%28id%2Cname%2Csize%2CmimeType%29")
                );
                respond(stream, 200, &[], br#"{"files":[{"id":"file1","name":"Map","size":"8","mimeType":"application/octet-stream","version":"not-an-etag"}]}"#);
            }
            1 => {
                assert_eq!(request.method, "GET");
                assert_eq!(request.path, "/api/files/file1?alt=media");
                respond(stream, 200, &[("ETag", "\"media-rev\"")], b"original");
            }
            2 => {
                assert_eq!(request.method, "PATCH");
                assert_eq!(request.path, "/upload/files/file1?uploadType=media");
                assert_eq!(request.headers["if-match"], "\"media-rev\"");
                assert_eq!(request.body, b"local edits");
                respond(stream, 412, &[], b"SECRET error");
            }
            _ => unreachable!(),
        }
    });
    let provider = GoogleDriveProvider::new(GoogleDriveConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    run(async {
        let cancel = CancelToken::default();
        let entries = provider
            .list(&StorageId("root".into()), cancel.clone())
            .await
            .unwrap();
        assert_eq!(entries[0].size, 8);
        assert_eq!(entries[0].revision, None);
        let read = provider.read(&entries[0].id, cancel.clone()).await.unwrap();
        assert_eq!(read.bytes, b"original");
        assert_eq!(read.revision, Revision("\"media-rev\"".into()));
        assert!(matches!(
            provider
                .write(
                    &entries[0].id,
                    b"local edits",
                    ExpectedRevision::Match(read.revision),
                    cancel.clone()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
        assert!(matches!(
            provider
                .write(&entries[0].id, b"x", ExpectedRevision::Missing, cancel)
                .await,
            Err(StorageError::Unsupported)
        ));
    });
    thread.join().unwrap();
}

fn graph(url: &str) -> GraphProvider {
    GraphProvider::new(GraphConfig {
        api_url: format!("{url}v1.0/"),
        drive_id: "drive1".into(),
        access_token: "mock-token".into(),
    })
    .unwrap()
}

#[test]
fn graph_list_download_conditions_versions_and_explicit_share() {
    let (url, thread) = server(6, |index, request, stream| {
        if index == 2 {
            assert!(!request.headers.contains_key("authorization"));
        } else {
            assert_eq!(request.headers["authorization"], "Bearer mock-token");
        }
        match index {
            0 => {
                assert!(
                    request
                        .path
                        .starts_with("/v1.0/drives/drive1/items/root/children?")
                );
                respond(stream, 200, &[], br#"{"value":[{"id":"file1","name":"Map","size":8,"eTag":"\"r1\"","file":{}}]}"#);
            }
            1 => {
                assert_eq!(request.path, "/v1.0/drives/drive1/items/file1/content");
                let target = format!(
                    "http://{}/signed-download?mock-capability=1",
                    request.headers["host"]
                );
                respond(stream, 302, &[("Location", &target)], b"");
            }
            2 => {
                assert_eq!(request.path, "/signed-download?mock-capability=1");
                respond(stream, 200, &[("ETag", "\"bytes-rev\"")], b"original");
            }
            3 => {
                assert_eq!(request.method, "PUT");
                assert_eq!(request.headers["if-match"], "\"bytes-rev\"");
                assert_eq!(request.body, b"local edits");
                respond(stream, 412, &[], b"SECRET");
            }
            4 => {
                assert_eq!(request.path, "/v1.0/drives/drive1/items/file1/versions");
                respond(
                    stream,
                    200,
                    &[],
                    br#"{"value":[{"id":"1.0","size":8,"lastModifiedDateTime":"2026-01-01"}]}"#,
                );
            }
            5 => {
                assert_eq!(request.method, "POST");
                assert_eq!(request.path, "/v1.0/drives/drive1/items/file1/createLink");
                let data: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(
                    data,
                    serde_json::json!({"scope":"organization","type":"view"})
                );
                respond(
                    stream,
                    201,
                    &[],
                    format!(
                        r#"{{"link":{{"webUrl":"http://{}/share/mock"}}}}"#,
                        request.headers["host"]
                    )
                    .as_bytes(),
                );
            }
            _ => unreachable!(),
        }
    });
    let provider = graph(&url);
    run(async {
        let cancel = CancelToken::default();
        let entries = provider
            .list(&StorageId("root".into()), cancel.clone())
            .await
            .unwrap();
        assert_eq!(entries[0].id, StorageId("file1".into()));
        assert_eq!(entries[0].size, 8);
        assert_eq!(entries[0].revision, Some(Revision("\"r1\"".into())));
        let read = provider.read(&entries[0].id, cancel.clone()).await.unwrap();
        assert_eq!(read.bytes, b"original");
        assert_eq!(read.revision, Revision("\"bytes-rev\"".into()));
        assert!(matches!(
            provider
                .write(
                    &entries[0].id,
                    b"local edits",
                    ExpectedRevision::Match(read.revision),
                    cancel.clone()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
        assert_eq!(
            provider
                .list_versions(&entries[0].id, cancel.clone())
                .await
                .unwrap()[0]
                .id,
            "1.0"
        );
        assert!(
            provider
                .create_share_link(&entries[0].id, GraphShareScope::Organization, cancel)
                .await
                .unwrap()
                .0
                .ends_with("/share/mock")
        );
    });
    thread.join().unwrap();
}

#[test]
fn graph_large_upload_is_sequential_aligned_and_never_sends_bearer_to_session() {
    const CHUNK: usize = 10 * 327_680;
    let bytes: Vec<_> = (0..(CHUNK * 2 + 13)).map(|n| (n % 251) as u8).collect();
    let collected = Arc::new(Mutex::new(Vec::new()));
    let remote = collected.clone();
    let total = bytes.len();
    let (url, thread) = server(4, move |index, request, stream| {
        if index == 0 {
            assert_eq!(request.method, "POST");
            assert_eq!(
                request.path,
                "/v1.0/drives/drive1/items/file1/createUploadSession"
            );
            assert_eq!(request.headers["authorization"], "Bearer mock-token");
            assert_eq!(request.headers["if-match"], "\"r1\"");
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&request.body).unwrap(),
                serde_json::json!({"item":{"@microsoft.graph.conflictBehavior":"fail"}})
            );
            respond(stream, 200, &[], format!(r#"{{"uploadUrl":"http://{}/upload-session?mock-capability=1","expirationDateTime":"2030-01-01T00:00:00Z"}}"#, request.headers["host"]).as_bytes());
        } else {
            assert_eq!(request.method, "PUT");
            assert_eq!(request.path, "/upload-session?mock-capability=1");
            assert!(!request.headers.contains_key("authorization"));
            assert!(!request.headers.contains_key("if-match"));
            let start = (index - 1) * CHUNK;
            let end = (start + CHUNK).min(total);
            assert_eq!(
                request.headers["content-range"],
                format!("bytes {start}-{}/{total}", end - 1)
            );
            assert_eq!(request.headers["content-length"], (end - start).to_string());
            assert_eq!(request.body.len(), end - start);
            assert!(request.body.len() < 60 * 1024 * 1024);
            remote.lock().unwrap().extend_from_slice(&request.body);
            if end < total {
                assert_eq!(request.body.len() % 327_680, 0);
                respond(
                    stream,
                    202,
                    &[],
                    format!(r#"{{"nextExpectedRanges":["{end}-"]}}"#).as_bytes(),
                );
            } else {
                respond(stream, 201, &[], br#"{"eTag":"\"r2\""}"#);
            }
        }
    });
    let provider = graph(&url);
    run(async {
        assert_eq!(
            provider
                .write(
                    &StorageId("file1".into()),
                    &bytes,
                    ExpectedRevision::Match(Revision("\"r1\"".into())),
                    CancelToken::default()
                )
                .await
                .unwrap(),
            Revision("\"r2\"".into())
        );
    });
    thread.join().unwrap();
    assert_eq!(*collected.lock().unwrap(), bytes);
}

#[test]
fn graph_session_stale_and_untrusted_urls_never_upload_bytes() {
    let unexpected = TcpListener::bind("127.0.0.1:0").unwrap();
    unexpected.set_nonblocking(true).unwrap();
    let target = format!("http://{}/steal", unexpected.local_addr().unwrap());
    let (url, thread) = server(2, move |index, request, stream| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.headers["if-match"], "\"stale\"");
        assert!(!request.body.windows(11).any(|v| v == b"local edits"));
        if index == 0 {
            respond(stream, 412, &[], b"SECRET conflict");
        } else {
            respond(
                stream,
                200,
                &[],
                format!(r#"{{"uploadUrl":"{target}"}}"#).as_bytes(),
            );
        }
    });
    let provider = graph(&url);
    run(async {
        let id = StorageId("file1".into());
        assert!(matches!(
            provider
                .write_large(
                    &id,
                    b"local edits",
                    ExpectedRevision::Match(Revision("\"stale\"".into())),
                    CancelToken::default()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
        assert!(
            matches!(provider.write_large(&id, b"local edits", ExpectedRevision::Match(Revision("\"stale\"".into())), CancelToken::default()).await, Err(StorageError::Remote(ref error)) if error == "untrusted provider URL")
        );
    });
    thread.join().unwrap();
    assert!(unexpected.accept().is_err());
}

#[test]
fn graph_invalid_acknowledgement_aborts_session_without_retry() {
    let (url, thread) = server(3, |index, request, stream| match index {
        0 => respond(
            stream,
            200,
            &[],
            format!(
                r#"{{"uploadUrl":"http://{}/session"}}"#,
                request.headers["host"]
            )
            .as_bytes(),
        ),
        1 => {
            assert_eq!(request.method, "PUT");
            assert!(!request.headers.contains_key("authorization"));
            respond(stream, 202, &[], br#"{"nextExpectedRanges":["1-"]}"#);
        }
        2 => {
            assert_eq!(request.method, "DELETE");
            assert_eq!(request.path, "/session");
            assert!(!request.headers.contains_key("authorization"));
            respond(stream, 204, &[], b"");
        }
        _ => unreachable!(),
    });
    let provider = graph(&url);
    run(async {
        assert!(
            matches!(provider.write_large(&StorageId("file1".into()), &vec![0; 4 * 1024 * 1024], ExpectedRevision::Any, CancelToken::default()).await, Err(StorageError::Remote(ref error)) if error == "Graph upload returned unexpected next range")
        );
    });
    thread.join().unwrap();
}

#[test]
fn webdav_namespace_escaped_etags_cdata_and_failed_properties() {
    let (url, thread) = server(1, |_, _, stream| {
        respond(
            stream,
            207,
            &[],
            br#"<multistatus xmlns="DAV:">
          <response><href>/dav/escaped</href><propstat><prop>
            <displayname>A &amp; B <![CDATA[<Map>]]></displayname>
            <getcontentlength> 12 </getcontentlength><getetag>&quot;rev&#49;&quot;</getetag>
          </prop><status>HTTP/1.1 200 OK</status></propstat></response>
          <response><href>/dav/failed</href><propstat><prop><getetag>"ignored"</getetag></prop>
          <status>HTTP/1.1 403 Forbidden</status></propstat></response>
        </multistatus>"#,
        );
    });
    let provider = dav(url);
    run(async {
        let entries = provider
            .list(&provider.id("").unwrap(), CancelToken::default())
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "A & B <Map>");
        assert_eq!(entries[0].size, 12);
        assert_eq!(entries[0].revision, Some(Revision("\"rev1\"".into())));
    });
    thread.join().unwrap();
}

#[test]
fn webdav_malformed_xml_and_untrusted_hrefs_fail_closed() {
    let responses: [&[u8]; 3] = [
        br#"<multistatus xmlns="not-DAV"/>"#,
        br#"<!DOCTYPE x [<!ENTITY secret SYSTEM "file:///private">]><multistatus xmlns="DAV:"/>"#,
        br#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>https://untrusted.invalid/map</d:href><d:propstat><d:prop/><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#,
    ];
    let (url, thread) = server(3, move |index, _, stream| {
        respond(stream, 207, &[], responses[index])
    });
    let provider = dav(url);
    run(async {
        for _ in 0..3 {
            assert!(matches!(
                provider
                    .list(&provider.id("").unwrap(), CancelToken::default())
                    .await,
                Err(StorageError::Remote(_))
            ));
        }
    });
    thread.join().unwrap();
}

#[test]
fn webdav_committed_put_without_etag_fails_closed_without_retry_or_metadata_fallback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let worker_listener = listener.try_clone().unwrap();
    let stored = Arc::new(Mutex::new(b"original".to_vec()));
    let remote = stored.clone();
    let worker = thread::spawn(move || {
        let (mut stream, _) = worker_listener.accept().unwrap();
        let request = request(&mut stream);
        assert_eq!(request.method, "PUT");
        assert_eq!(request.path, "/dav/map.thinoda");
        assert_eq!(request.headers["if-match"], "\"r1\"");
        assert_eq!(request.body, b"local edits");
        // The server has committed the bytes, but cannot identify the resulting
        // revision. This is ambiguous success, not a safe-to-retry failed PUT.
        *remote.lock().unwrap() = request.body;
        respond(&mut stream, 204, &[], b"");
    });
    let provider = dav(url);
    let local_bytes = b"local edits".to_vec();
    run(async {
        let result = provider
            .write(
                &provider.id("map.thinoda").unwrap(),
                &local_bytes,
                ExpectedRevision::Match(Revision("\"r1\"".into())),
                CancelToken::default(),
            )
            .await;
        assert!(matches!(
            result,
            Err(StorageError::Remote(ref message))
                if message == "write succeeded without an ETag; reread before saving again"
        ));
    });
    worker.join().unwrap();
    assert_eq!(*stored.lock().unwrap(), b"local edits");
    assert_eq!(local_bytes, b"local edits");
    // Keep the mock endpoint listening after the response: any PUT retry, HEAD,
    // GET, or PROPFIND metadata fallback would leave another queued connection.
    listener.set_nonblocking(true).unwrap();
    assert!(matches!(
        listener.accept(),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
    ));
}

#[test]
fn no_etag_does_not_invent_a_read_revision() {
    let (url, thread) = server(3, |_, _, stream| respond(stream, 200, &[], b"original"));
    let webdav = dav(url.clone());
    let google = GoogleDriveProvider::new(GoogleDriveConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    let graph = graph(&url);
    run(async {
        assert!(matches!(
            webdav
                .read(&webdav.id("file").unwrap(), CancelToken::default())
                .await,
            Err(StorageError::Remote(_))
        ));
        assert!(matches!(
            google
                .read(&StorageId("file".into()), CancelToken::default())
                .await,
            Err(StorageError::Remote(_))
        ));
        assert!(matches!(
            graph
                .read(&StorageId("file".into()), CancelToken::default())
                .await,
            Err(StorageError::Remote(_))
        ));
    });
    thread.join().unwrap();
}

#[test]
fn rest_configuration_and_cancel_do_not_make_background_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let dropbox = DropboxProvider::new(DropboxConfig {
        api_url: format!("{url}api/"),
        content_url: format!("{url}content/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    let box_api = BoxProvider::new(BoxConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    let google = GoogleDriveProvider::new(GoogleDriveConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    let graph = graph(&url);
    run(async {
        let cancel = CancelToken::default();
        cancel.cancel();
        for provider in [&dropbox as &dyn StorageProvider, &box_api, &google, &graph] {
            assert!(matches!(
                provider
                    .list(&StorageId("root".into()), cancel.clone())
                    .await,
                Err(StorageError::Cancelled)
            ));
        }
    });
    assert!(listener.accept().is_err());
}

#[test]
fn cancellation_interrupts_a_streaming_body() {
    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let (url, thread) = server(1, move |_, _, stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nETag: \"r1\"\r\nContent-Length: 100\r\n\r\npart")
            .unwrap();
        stream.flush().unwrap();
        seen_tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    let provider = dav(url);
    let cancel = CancelToken::default();
    let token = cancel.clone();
    let trigger = thread::spawn(move || {
        seen_rx.recv().unwrap();
        token.cancel();
    });
    run(async {
        assert!(matches!(
            provider
                .read(&provider.id("file").unwrap(), cancel.clone())
                .await,
            Err(StorageError::Cancelled)
        ));
    });
    trigger.join().unwrap();
    thread.join().unwrap();
}

#[test]
fn graph_and_box_metadata_downloads_use_no_account_credentials_or_metadata_revision() {
    let (url, thread) = server(4, |index, request, stream| {
        assert_eq!(request.method, "GET");
        match index {
            0 | 2 => {
                assert_eq!(request.headers["authorization"], "Bearer mock-token");
                if index == 0 {
                    assert_eq!(
                        request.path,
                        "/v1.0/drives/drive1/items/file1?%24select=id%2C%40microsoft.graph.downloadUrl"
                    );
                } else {
                    assert_eq!(request.path, "/api/files/file1?fields=id%2Cdownload_url");
                }
                let field = if index == 0 {
                    "@microsoft.graph.downloadUrl"
                } else {
                    "download_url"
                };
                let metadata = serde_json::json!({
                    "id": "file1", "eTag": "\"metadata-newer\"", "etag": "metadata-newer",
                    field: format!("http://{}/signed?mock-capability={index}", request.headers["host"]),
                });
                respond(
                    stream,
                    200,
                    &[("ETag", "\"metadata-header\"")],
                    &serde_json::to_vec(&metadata).unwrap(),
                );
            }
            1 | 3 => {
                assert!(request.path.starts_with("/signed?mock-capability="));
                assert!(!request.headers.contains_key("authorization"));
                assert!(!request.headers.contains_key("cookie"));
                assert!(!request.headers.contains_key("if-match"));
                respond(stream, 200, &[("ETag", "\"download-bytes\"")], b"original");
            }
            _ => unreachable!(),
        }
    });
    let graph = graph(&url);
    let box_api = BoxProvider::new(BoxConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    run(async {
        let id = StorageId("file1".into());
        let reads = [
            graph
                .read_via_download_url(&id, CancelToken::default())
                .await
                .unwrap(),
            box_api
                .read_via_download_url(&id, CancelToken::default())
                .await
                .unwrap(),
        ];
        for read in reads {
            assert_eq!(read.bytes, b"original");
            assert_eq!(read.revision, Revision("\"download-bytes\"".into()));
        }
    });
    thread.join().unwrap();
}

#[test]
fn metadata_download_rejects_untrusted_hosts_wrong_ids_and_missing_direct_urls() {
    let unexpected = TcpListener::bind("127.0.0.1:0").unwrap();
    unexpected.set_nonblocking(true).unwrap();
    let target = format!("http://{}/steal", unexpected.local_addr().unwrap());
    let (url, thread) = server(6, move |index, request, stream| {
        assert_eq!(request.headers["authorization"], "Bearer mock-token");
        let field = if index < 3 {
            "@microsoft.graph.downloadUrl"
        } else {
            "download_url"
        };
        let metadata = match index % 3 {
            0 => serde_json::json!({"id": "file1", field: target}),
            1 => serde_json::json!({"id": "different", field: target}),
            // Box shared links must never be used as a direct-url fallback.
            _ => serde_json::json!({"id": "file1", "shared_link": {"download_url": target}}),
        };
        respond(stream, 200, &[], &serde_json::to_vec(&metadata).unwrap());
    });
    let graph = graph(&url);
    let box_api = BoxProvider::new(BoxConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    run(async {
        let id = StorageId("file1".into());
        for index in 0..6 {
            let result = if index < 3 {
                graph
                    .read_via_download_url(&id, CancelToken::default())
                    .await
            } else {
                box_api
                    .read_via_download_url(&id, CancelToken::default())
                    .await
            };
            match index % 3 {
                0 => assert!(
                    matches!(result, Err(StorageError::Remote(ref message)) if message == "untrusted provider URL")
                ),
                1 => assert!(
                    matches!(result, Err(StorageError::Remote(ref message)) if message == "download metadata returned a different file")
                ),
                _ => assert!(matches!(result, Err(StorageError::Unsupported))),
            }
        }
    });
    thread.join().unwrap();
    assert!(unexpected.accept().is_err());
}

#[test]
fn metadata_download_rejects_redirects_and_missing_byte_bound_revision() {
    let (url, thread) = server(4, |index, request, stream| {
        if index % 2 == 0 {
            assert_eq!(request.headers["authorization"], "Bearer mock-token");
            let field = if index == 0 {
                "@microsoft.graph.downloadUrl"
            } else {
                "download_url"
            };
            let metadata = serde_json::json!({
                "id": "file1", "eTag": "\"not-bytes\"", "etag": "not-bytes",
                field: format!("http://{}/signed", request.headers["host"])
            });
            respond(stream, 200, &[], &serde_json::to_vec(&metadata).unwrap());
        } else {
            assert!(!request.headers.contains_key("authorization"));
            if index == 1 {
                respond(
                    stream,
                    302,
                    &[("Location", "https://untrusted.invalid/steal")],
                    b"",
                );
            } else {
                respond(stream, 200, &[], b"original");
            }
        }
    });
    let graph = graph(&url);
    let box_api = BoxProvider::new(BoxConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    run(async {
        let id = StorageId("file1".into());
        assert!(
            matches!(graph.read_via_download_url(&id, CancelToken::default()).await, Err(StorageError::Remote(ref message)) if message == "HTTP 302")
        );
        assert!(
            matches!(box_api.read_via_download_url(&id, CancelToken::default()).await, Err(StorageError::Remote(ref message)) if message == "metadata download has no byte-bound ETag")
        );
    });
    thread.join().unwrap();
}

#[test]
fn graph_creation_returns_server_id_and_rejects_same_name_without_overwrite() {
    let stored = Arc::new(Mutex::new(Vec::new()));
    let remote = stored.clone();
    let (url, worker) = server(3, move |index, request, stream| {
        assert_eq!(request.method, "PUT");
        assert_eq!(request.headers["authorization"], "Bearer mock-token");
        if index < 2 {
            assert_eq!(
                request.path,
                "/v1.0/drives/drive1/items/root:/Map%20&%20Notes.thinoda:/content?%40microsoft.graph.conflictBehavior=fail"
            );
            assert_eq!(request.headers["if-none-match"], "*");
            assert!(!request.headers.contains_key("if-match"));
            if index == 0 {
                *remote.lock().unwrap() = request.body;
                respond(stream, 201, &[], br#"{"id":"new-server-id","name":"Map & Notes.thinoda","size":8,"eTag":"\"created-r1\""}"#);
            } else {
                // A competing writer/name collision must never get a new name
                // or overwrite the successfully created bytes.
                assert_eq!(request.body, b"racing bytes");
                respond(stream, 409, &[], b"private nameAlreadyExists");
            }
        } else {
            assert_eq!(
                request.path,
                "/v1.0/drives/drive1/items/new-server-id/content"
            );
            assert_eq!(request.headers["if-match"], "\"created-r1\"");
            respond(stream, 200, &[], br#"{"eTag":"\"r2\""}"#);
        }
    });
    let provider = graph(&url);
    run(async {
        let parent = StorageId("root".into());
        let entry = provider
            .create_file(
                &parent,
                "Map & Notes.thinoda",
                b"original",
                CancelToken::default(),
            )
            .await
            .unwrap();
        assert_eq!(entry.id, StorageId("new-server-id".into()));
        assert_eq!(entry.name, "Map & Notes.thinoda");
        assert_eq!(entry.size, 8);
        assert_eq!(entry.revision, Some(Revision("\"created-r1\"".into())));
        assert!(matches!(
            provider
                .create_file(
                    &parent,
                    "Map & Notes.thinoda",
                    b"racing bytes",
                    CancelToken::default()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
        assert_eq!(*stored.lock().unwrap(), b"original");
        assert_eq!(
            provider
                .write(
                    &entry.id,
                    b"next",
                    ExpectedRevision::Match(entry.revision.unwrap()),
                    CancelToken::default()
                )
                .await
                .unwrap(),
            Revision("\"r2\"".into())
        );
    });
    worker.join().unwrap();
}

fn box_attributes(request: &Request) -> serde_json::Value {
    let body = String::from_utf8_lossy(&request.body);
    let attrs = body.find("name=\"attributes\"").unwrap();
    let file = body.find("name=\"file\"").unwrap();
    assert!(attrs < file);
    let start = body[attrs..].find("\r\n\r\n").unwrap() + attrs + 4;
    let end = body[start..].find("\r\n--").unwrap() + start;
    serde_json::from_str(&body[start..end]).unwrap()
}

#[test]
fn box_creation_multipart_returns_server_id_and_same_name_is_conflict() {
    let stored = Arc::new(Mutex::new(Vec::new()));
    let remote = stored.clone();
    let (url, worker) = server(2, move |index, request, stream| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/upload/files/content");
        assert_eq!(request.headers["authorization"], "Bearer mock-token");
        assert_eq!(request.headers["if-none-match"], "*");
        let attributes = box_attributes(&request);
        assert_eq!(
            attributes,
            serde_json::json!({"name":"Map \"quoted\".thinoda","parent":{"id":"0"}})
        );
        assert!(
            String::from_utf8_lossy(&request.body).contains(if index == 0 {
                "\r\n\r\noriginal\r\n"
            } else {
                "\r\n\r\nrace\r\n"
            })
        );
        if index == 0 {
            *remote.lock().unwrap() = b"original".to_vec();
            respond(stream, 201, &[], br#"{"entries":[{"id":"box-new-id","name":"Map \"quoted\".thinoda","size":8,"etag":"1"}]}"#);
        } else {
            respond(
                stream,
                409,
                &[],
                br#"{"code":"item_name_in_use","context_info":{"conflicts":{"id":"private-id"}}}"#,
            );
        }
    });
    let provider = BoxProvider::new(BoxConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    run(async {
        let parent = StorageId("0".into());
        let entry = provider
            .create_file(
                &parent,
                "Map \"quoted\".thinoda",
                b"original",
                CancelToken::default(),
            )
            .await
            .unwrap();
        assert_eq!(entry.id, StorageId("box-new-id".into()));
        assert_eq!(entry.revision, Some(Revision("1".into())));
        assert_eq!(entry.size, 8);
        assert!(matches!(
            provider
                .create_file(
                    &parent,
                    "Map \"quoted\".thinoda",
                    b"race",
                    CancelToken::default()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
        assert_eq!(*stored.lock().unwrap(), b"original");
    });
    worker.join().unwrap();
}

#[test]
fn dav_and_dropbox_share_explicit_creation_api_and_safe_missing_semantics() {
    let (url, worker) = server(4, |index, request, stream| match index {
        0 | 1 => {
            assert_eq!(request.method, "PUT");
            assert_eq!(request.path, "/dav/folder/Map.thinoda");
            assert_eq!(request.headers["if-none-match"], "*");
            if index == 0 {
                respond(stream, 201, &[("ETag", "\"dav-created\"")], b"");
            } else {
                respond(stream, 412, &[], b"exists");
            }
        }
        2 | 3 => {
            assert_eq!(request.method, "POST");
            assert_eq!(request.path, "/content/files/upload");
            let argument: serde_json::Value =
                serde_json::from_str(&request.headers["dropbox-api-arg"]).unwrap();
            assert_eq!(argument["path"], "id:folder/Map.thinoda");
            assert_eq!(argument["mode"], "add");
            assert_eq!(argument["strict_conflict"], true);
            assert_eq!(argument["autorename"], false);
            if index == 2 {
                respond(stream, 200, &[], br#"{"id":"id:created-file","name":"Map.thinoda","rev":"dropbox-created","size":8}"#);
            } else {
                respond(stream, 409, &[], b"exists");
            }
        }
        _ => unreachable!(),
    });
    let webdav = dav(url.clone());
    let dropbox = DropboxProvider::new(DropboxConfig {
        api_url: format!("{url}api/"),
        content_url: format!("{url}content/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    run(async {
        let parent = webdav.id("folder").unwrap();
        let entry = webdav
            .create_file(&parent, "Map.thinoda", b"original", CancelToken::default())
            .await
            .unwrap();
        assert!(entry.id.0.ends_with("/dav/folder/Map.thinoda"));
        assert_eq!(entry.revision, Some(Revision("\"dav-created\"".into())));
        assert!(matches!(
            webdav
                .create_file(&parent, "Map.thinoda", b"race", CancelToken::default())
                .await,
            Err(StorageError::Conflict { .. })
        ));
        let parent = StorageId("id:folder".into());
        let entry = dropbox
            .create_file(&parent, "Map.thinoda", b"original", CancelToken::default())
            .await
            .unwrap();
        assert_eq!(entry.id, StorageId("id:created-file".into()));
        assert_eq!(entry.revision, Some(Revision("dropbox-created".into())));
        assert!(matches!(
            dropbox
                .create_file(&parent, "Map.thinoda", b"race", CancelToken::default())
                .await,
            Err(StorageError::Conflict { .. })
        ));
    });
    worker.join().unwrap();
}

#[test]
fn graph_large_creation_uses_missing_session_and_returns_final_identity() {
    const CHUNK: usize = 10 * 327_680;
    let bytes = vec![42; CHUNK + 1_000_000];
    let total = bytes.len();
    let (url, worker) = server(3, move |index, request, stream| {
        if index == 0 {
            assert_eq!(request.method, "POST");
            assert_eq!(
                request.path,
                "/v1.0/drives/drive1/items/root:/Large.thinoda:/createUploadSession"
            );
            assert_eq!(request.headers["if-none-match"], "*");
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&request.body).unwrap(),
                serde_json::json!({
                    "item": {"name": "Large.thinoda", "@microsoft.graph.conflictBehavior": "fail"}
                })
            );
            respond(
                stream,
                200,
                &[],
                format!(
                    r#"{{"uploadUrl":"http://{}/session"}}"#,
                    request.headers["host"]
                )
                .as_bytes(),
            );
        } else {
            assert_eq!(request.method, "PUT");
            assert_eq!(request.path, "/session");
            assert!(!request.headers.contains_key("authorization"));
            let start = if index == 1 { 0 } else { CHUNK };
            let end = if index == 1 { CHUNK } else { total };
            assert_eq!(
                request.headers["content-range"],
                format!("bytes {start}-{}/{total}", end - 1)
            );
            assert_eq!(request.body, vec![42; end - start]);
            if index == 1 {
                respond(
                    stream,
                    202,
                    &[],
                    format!(r#"{{"nextExpectedRanges":["{CHUNK}-"]}}"#).as_bytes(),
                );
            } else {
                respond(stream, 201, &[], br#"{"id":"large-server-id","name":"Large.thinoda","eTag":"\"large-created\""}"#);
            }
        }
    });
    let provider = graph(&url);
    run(async {
        let entry = provider
            .create_file(
                &StorageId("root".into()),
                "Large.thinoda",
                &bytes,
                CancelToken::default(),
            )
            .await
            .unwrap();
        assert_eq!(entry.id, StorageId("large-server-id".into()));
        assert_eq!(entry.revision, Some(Revision("\"large-created\"".into())));
        assert_eq!(entry.size, total as u64);
    });
    worker.join().unwrap();
}

#[test]
fn graph_large_creation_commit_race_is_conflict_and_cleans_session_without_retry() {
    let (url, worker) = server(4, |index, request, stream| {
        match index {
            0 => {
                assert_eq!(request.headers["if-none-match"], "*");
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["item"]["@microsoft.graph.conflictBehavior"],
                    "fail"
                );
                respond(
                    stream,
                    200,
                    &[],
                    format!(
                        r#"{{"uploadUrl":"http://{}/session"}}"#,
                        request.headers["host"]
                    )
                    .as_bytes(),
                );
            }
            1 => {
                assert_eq!(request.method, "PUT");
                assert!(!request.headers.contains_key("authorization"));
                respond(stream, 202, &[], br#"{"nextExpectedRanges":["3276800-"]}"#);
            }
            2 => {
                // Name became occupied after session creation but before commit.
                assert_eq!(request.method, "PUT");
                respond(stream, 409, &[], b"name conflict");
            }
            3 => {
                assert_eq!(request.method, "DELETE");
                assert!(!request.headers.contains_key("authorization"));
                respond(stream, 204, &[], b"");
            }
            _ => unreachable!(),
        }
    });
    let provider = graph(&url);
    run(async {
        assert!(matches!(
            provider
                .create_file(
                    &StorageId("root".into()),
                    "Large.thinoda",
                    &vec![0; 4_300_000],
                    CancelToken::default()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
    });
    worker.join().unwrap();
}

#[test]
fn creation_invalid_names_and_precancel_never_contact_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let graph = graph(&url);
    let webdav = dav(url.clone());
    let box_api = BoxProvider::new(BoxConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    let dropbox = DropboxProvider::new(DropboxConfig {
        api_url: format!("{url}api/"),
        content_url: format!("{url}content/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    let drive = GoogleDriveProvider::new(GoogleDriveConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    run(async {
        let parent = StorageId("root".into());
        for name in ["", " ", "..", "../escape", "escape\\file", "bad\nname"] {
            assert!(
                drive
                    .create_file(&parent, name, b"x", CancelToken::default())
                    .await
                    .is_err()
            );
            assert!(
                graph
                    .create_file(&parent, name, b"x", CancelToken::default())
                    .await
                    .is_err()
            );
            assert!(
                box_api
                    .create_file(&parent, name, b"x", CancelToken::default())
                    .await
                    .is_err()
            );
            assert!(
                webdav
                    .create_file(&webdav.id("").unwrap(), name, b"x", CancelToken::default())
                    .await
                    .is_err()
            );
            assert!(
                dropbox
                    .create_file(&parent, name, b"x", CancelToken::default())
                    .await
                    .is_err()
            );
        }
        let cancel = CancelToken::default();
        cancel.cancel();
        assert!(matches!(
            graph
                .create_file(&parent, "Map", b"x", cancel.clone())
                .await,
            Err(StorageError::Cancelled)
        ));
        assert!(matches!(
            box_api
                .create_file(&parent, "Map", b"x", cancel.clone())
                .await,
            Err(StorageError::Cancelled)
        ));
        assert!(matches!(
            drive
                .create_file(&parent, "Map", b"x", cancel.clone())
                .await,
            Err(StorageError::Cancelled)
        ));
        assert!(matches!(
            webdav
                .create_file(&webdav.id("").unwrap(), "Map", b"x", cancel.clone())
                .await,
            Err(StorageError::Cancelled)
        ));
        assert!(matches!(
            dropbox.create_file(&parent, "Map", b"x", cancel).await,
            Err(StorageError::Cancelled)
        ));
    });
    assert!(listener.accept().is_err());
}

#[test]
fn creation_missing_revision_or_renamed_identity_never_reports_saved() {
    let (url, worker) = server(3, |index, request, stream| {
        assert_eq!(request.headers["if-none-match"], "*");
        match index {
            0 => respond(stream, 201, &[], br#"{"id":"new-id","name":"Map.thinoda"}"#),
            1 => respond(
                stream,
                201,
                &[],
                br#"{"id":"new-id","name":"Map (1).thinoda","eTag":"\"r1\""}"#,
            ),
            2 => respond(
                stream,
                201,
                &[],
                br#"{"entries":[{"id":"new-id","name":"Map.thinoda"}]}"#,
            ),
            _ => unreachable!(),
        }
    });
    let graph = graph(&url);
    let box_api = BoxProvider::new(BoxConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap();
    let bytes = b"local edits".to_vec();
    run(async {
        let parent = StorageId("root".into());
        assert!(matches!(
            graph
                .create_file(&parent, "Map.thinoda", &bytes, CancelToken::default())
                .await,
            Err(StorageError::Remote(_))
        ));
        assert!(
            matches!(graph.create_file(&parent, "Map.thinoda", &bytes, CancelToken::default()).await, Err(StorageError::Remote(ref message)) if message == "creation succeeded with unexpected identity; reread before retrying")
        );
        assert!(matches!(
            box_api
                .create_file(&parent, "Map.thinoda", &bytes, CancelToken::default())
                .await,
            Err(StorageError::Remote(_))
        ));
        assert_eq!(bytes, b"local edits");
    });
    worker.join().unwrap();
}

#[test]
fn graph_large_creation_rejects_untrusted_session_before_sending_bytes() {
    let unexpected = TcpListener::bind("127.0.0.1:0").unwrap();
    unexpected.set_nonblocking(true).unwrap();
    let target = format!("http://{}/steal", unexpected.local_addr().unwrap());
    let (url, worker) = server(1, move |_, request, stream| {
        assert_eq!(request.method, "POST");
        assert!(request.path.ends_with("/createUploadSession"));
        assert_eq!(request.headers["if-none-match"], "*");
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["item"]["@microsoft.graph.conflictBehavior"], "fail");
        respond(
            stream,
            200,
            &[],
            format!(r#"{{"uploadUrl":"{target}"}}"#).as_bytes(),
        );
    });
    let provider = graph(&url);
    run(async {
        assert!(
            matches!(provider.create_file(&StorageId("root".into()), "Map.thinoda", &vec![0; 4_300_000], CancelToken::default()).await, Err(StorageError::Remote(ref message)) if message == "untrusted provider URL")
        );
    });
    worker.join().unwrap();
    assert!(unexpected.accept().is_err());
}

fn drive(url: &str) -> GoogleDriveProvider {
    GoogleDriveProvider::new(GoogleDriveConfig {
        api_url: format!("{url}api/"),
        upload_url: format!("{url}upload/"),
        access_token: "mock-token".into(),
    })
    .unwrap()
}

fn related_parts(request: &Request) -> (serde_json::Value, Vec<u8>) {
    let kind = &request.headers["content-type"];
    assert!(kind.starts_with("multipart/related; boundary="));
    let boundary = kind.split_once("boundary=").unwrap().1;
    let start = format!("--{boundary}\r\nContent-Type: application/json\r\n\r\n");
    assert!(request.body.starts_with(start.as_bytes()));
    let separator = format!("\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n");
    let split = request
        .body
        .windows(separator.len())
        .position(|part| part == separator.as_bytes())
        .unwrap();
    let metadata = serde_json::from_slice(&request.body[start.len()..split]).unwrap();
    let trailer = format!("\r\n--{boundary}--\r\n");
    assert!(request.body.ends_with(trailer.as_bytes()));
    let bytes = request.body[split + separator.len()..request.body.len() - trailer.len()].to_vec();
    (metadata, bytes)
}

#[test]
fn drive_creates_distinct_ids_for_duplicate_names_without_lookup_or_replacement() {
    let existing = Arc::new(Mutex::new(b"existing document".to_vec()));
    let original = existing.clone();
    let (url, worker) = server(3, move |index, request, stream| {
        assert_eq!(request.headers["authorization"], "Bearer mock-token");
        if index < 2 {
            assert_eq!(request.method, "POST");
            assert_eq!(
                request.path,
                "/upload/files?uploadType=multipart&fields=id%2Cname"
            );
            assert!(!request.headers.contains_key("if-match"));
            assert!(!request.headers.contains_key("if-none-match"));
            let (metadata, bytes) = related_parts(&request);
            assert_eq!(
                metadata,
                serde_json::json!({
                    "name": "Map \"quoted\".thinoda", "parents": ["folder1"],
                    "mimeType": "application/octet-stream",
                })
            );
            assert_eq!(
                bytes,
                if index == 0 {
                    b"first".to_vec()
                } else {
                    b"second".to_vec()
                }
            );
            assert_eq!(*original.lock().unwrap(), b"existing document");
            let data = serde_json::json!({"id": format!("new-id-{index}"), "name": "Map \"quoted\".thinoda"});
            respond(
                stream,
                if index == 0 { 200 } else { 201 },
                &[("ETag", "\"created-etag\"")],
                &serde_json::to_vec(&data).unwrap(),
            );
        } else {
            assert_eq!(request.method, "PATCH");
            assert_eq!(request.path, "/upload/files/new-id-0?uploadType=media");
            assert_eq!(request.headers["if-match"], "\"created-etag\"");
            assert!(!request.headers.contains_key("if-none-match"));
            assert_eq!(request.body, b"next edit");
            respond(stream, 412, &[], b"stale stable-ID save");
        }
    });
    let provider = drive(&url);
    run(async {
        let parent = StorageId("folder1".into());
        let first = provider
            .create_file(
                &parent,
                "Map \"quoted\".thinoda",
                b"first",
                CancelToken::default(),
            )
            .await
            .unwrap();
        let second = provider
            .create_file(
                &parent,
                "Map \"quoted\".thinoda",
                b"second",
                CancelToken::default(),
            )
            .await
            .unwrap();
        assert_eq!(first.id, StorageId("new-id-0".into()));
        assert_eq!(second.id, StorageId("new-id-1".into()));
        assert_ne!(first.id, second.id);
        assert_eq!(first.revision, Some(Revision("\"created-etag\"".into())));
        assert_eq!(first.size, 5);
        assert_eq!(second.size, 6);
        assert!(matches!(
            provider
                .write(
                    &first.id,
                    b"next edit",
                    ExpectedRevision::Match(first.revision.unwrap()),
                    CancelToken::default()
                )
                .await,
            Err(StorageError::Conflict { .. })
        ));
        assert!(matches!(
            provider
                .write(
                    &second.id,
                    b"never create through write",
                    ExpectedRevision::Missing,
                    CancelToken::default()
                )
                .await,
            Err(StorageError::Unsupported)
        ));
    });
    worker.join().unwrap();
    assert_eq!(*existing.lock().unwrap(), b"existing document");
}

#[test]
fn drive_creation_errors_no_revision_or_redirect_never_retry_or_bind_existing_ids() {
    let (url, worker) = server(4, |index, request, stream| {
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.path,
            "/upload/files?uploadType=multipart&fields=id%2Cname"
        );
        let (metadata, bytes) = related_parts(&request);
        assert_eq!(metadata["parents"][0], "root");
        assert_eq!(bytes, b"local edits");
        match index {
            0 => respond(stream, 403, &[], b"PRIVATE service body"),
            1 => respond(stream, 409, &[], b"service conflict"),
            2 => respond(
                stream,
                200,
                &[],
                br#"{"id":"created","name":"Map.thinoda","version":"not-a-revision"}"#,
            ),
            3 => respond(
                stream,
                307,
                &[("Location", "https://untrusted.invalid/upload")],
                b"",
            ),
            _ => unreachable!(),
        }
    });
    let provider = drive(&url);
    run(async {
        let parent = StorageId("root".into());
        for index in 0..4 {
            let result = provider
                .create_file(
                    &parent,
                    "Map.thinoda",
                    b"local edits",
                    CancelToken::default(),
                )
                .await;
            match index {
                0 => assert!(
                    matches!(result, Err(StorageError::Remote(ref message)) if message == "HTTP 403")
                ),
                1 => assert!(matches!(result, Err(StorageError::Conflict { .. }))),
                2 => assert!(
                    matches!(result, Err(StorageError::Remote(ref message)) if message == "Drive creation succeeded without an ETag; reread before retrying")
                ),
                _ => assert!(
                    matches!(result, Err(StorageError::Remote(ref message)) if message == "HTTP 307")
                ),
            }
        }
    });
    worker.join().unwrap();
}

#[test]
fn drive_creation_parent_payload_guards_and_precancel_make_no_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let provider = drive(&format!("http://{}/", listener.local_addr().unwrap()));
    run(async {
        let parent = StorageId("root".into());
        assert!(
            provider
                .create_file(&StorageId("".into()), "Map", b"x", CancelToken::default())
                .await
                .is_err()
        );
        assert!(matches!(
            provider
                .create_file(&parent, "Map", &vec![0; 5_000_001], CancelToken::default())
                .await,
            Err(StorageError::Unsupported)
        ));
        let cancel = CancelToken::default();
        cancel.cancel();
        assert!(matches!(
            provider.create_file(&parent, "Map", b"x", cancel).await,
            Err(StorageError::Cancelled)
        ));
    });
    assert!(listener.accept().is_err());
}

#[test]
fn drive_creation_cancellation_interrupts_pending_response() {
    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let (url, worker) = server(1, move |_, request, _| {
        assert_eq!(request.method, "POST");
        assert_eq!(related_parts(&request).1, b"local edits");
        seen_tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    let provider = drive(&url);
    let cancel = CancelToken::default();
    let token = cancel.clone();
    let trigger = thread::spawn(move || {
        seen_rx.recv().unwrap();
        token.cancel();
    });
    run(async {
        assert!(matches!(
            provider
                .create_file(
                    &StorageId("root".into()),
                    "Map.thinoda",
                    b"local edits",
                    cancel
                )
                .await,
            Err(StorageError::Cancelled)
        ));
    });
    trigger.join().unwrap();
    worker.join().unwrap();
}
