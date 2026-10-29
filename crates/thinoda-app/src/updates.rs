//! Nonmodal, document-independent update notices. No native binary is downloaded.
use crate::i18n::t;
use semver::Version;
use std::sync::mpsc::{self, Receiver};
use web_time::{Duration, Instant};

const RELEASES: &str = "https://github.com/AxelWol/Thinoda/releases";
const PERIOD: Duration = Duration::from_secs(6 * 60 * 60);
const RETRY: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    #[cfg(any(not(target_arch = "wasm32"), test))]
    Available,
    Ready,
    #[cfg(any(target_arch = "wasm32", test))]
    Applied,
}

#[derive(Clone, Debug)]
struct Notice {
    kind: Kind,
    version: String,
    release: String,
}

fn stable_version(tag: &str) -> Option<Version> {
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
    (version.pre.is_empty() && version.build.is_empty()).then_some(version)
}

/// Construct links locally; never trust a remote asset URL, body or release name.
#[cfg(any(not(target_arch = "wasm32"), test))]
fn parse_release(text: &str, current: &str) -> Result<Option<Notice>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "invalid release JSON")?;
    if value["draft"].as_bool() != Some(false) || value["prerelease"].as_bool() != Some(false) {
        return Err("not a stable release".into());
    }
    let tag = value["tag_name"].as_str().ok_or("missing release tag")?;
    let version = stable_version(tag).ok_or("invalid stable version")?;
    let current = Version::parse(current).map_err(|_| "invalid running version")?;
    let release = format!("{RELEASES}/tag/{tag}");
    if value["html_url"].as_str() != Some(release.as_str()) {
        return Err("untrusted release URL".into());
    }
    Ok((version > current).then(|| Notice {
        kind: Kind::Available,
        version: version.to_string(),
        release,
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_release() -> Result<Option<Notice>, String> {
    fetch_release_from("https://api.github.com/repos/AxelWol/Thinoda/releases/latest")
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_release_from(endpoint: &str) -> Result<Option<Notice>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .max_redirects(0)
        .build()
        .into();
    let mut response = agent
        .get(endpoint)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", concat!("Thinoda/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|_| "release request failed (offline, timeout or HTTP error)")?;
    if response.status() != ureq::http::StatusCode::OK {
        return Err("unexpected release response status".into());
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_to_string()
        .map_err(|_| "release response exceeds limit or is unreadable")?;
    parse_release(&body, env!("CARGO_PKG_VERSION"))
}

pub(crate) struct Updates {
    notice: Option<Notice>,
    dismissed: Option<String>,
    receiver: Option<Receiver<Result<Option<Notice>, String>>>,
    due: Instant,
    #[cfg(not(target_arch = "wasm32"))]
    wall_due: std::time::SystemTime,
    failures: u32,
    #[cfg(target_arch = "wasm32")]
    watching: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            notice: None,
            dismissed: None,
            receiver: None,
            due: Instant::now(),
            #[cfg(not(target_arch = "wasm32"))]
            wall_due: std::time::SystemTime::now(),
            failures: 0,
            #[cfg(target_arch = "wasm32")]
            watching: false,
        }
    }
}

impl Updates {
    #[cfg(not(target_arch = "wasm32"))]
    fn is_due(&self, now: Instant, wall: std::time::SystemTime) -> bool {
        // Linux's monotonic clock may pause during suspend. Either clock being
        // overdue allows a resume check; clock corrections cannot block forever.
        now >= self.due || wall >= self.wall_due
    }

    fn accept(&mut self, result: Result<Option<Notice>, String>, now: Instant) {
        match result {
            Ok(notice) => {
                self.failures = 0;
                self.due = now + PERIOD;
                self.notice =
                    notice.filter(|notice| self.dismissed.as_deref() != Some(&notice.version));
            }
            Err(error) => {
                self.failures = self.failures.saturating_add(1);
                self.due = now + (RETRY * (1 << self.failures.min(4))).min(PERIOD);
                log::warn!("Update check: {error}");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.wall_due = std::time::SystemTime::now() + self.due.duration_since(now);
        }
    }

    pub(crate) fn poll(&mut self, ctx: &egui::Context, native_services: bool) {
        let now = Instant::now();
        while let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        self.receiver = None;
                    }
                    self.accept(result, now);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.accept(Err("update worker disconnected".into()), now);
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if native_services && self.receiver.is_none() {
            if self.is_due(now, std::time::SystemTime::now()) {
                let (sender, receiver) = mpsc::channel();
                self.receiver = Some(receiver);
                let repaint = ctx.clone();
                // Bounded blocking networking is isolated from the UI; one request at a time.
                if let Err(error) = std::thread::Builder::new()
                    .name("thinoda-updates".into())
                    .spawn(move || {
                        let _ = sender.send(fetch_release());
                        repaint.request_repaint();
                    })
                {
                    self.receiver = None;
                    self.accept(Err(format!("cannot start update worker: {error}")), now);
                    ctx.request_repaint_after(self.due.saturating_duration_since(now));
                }
            } else {
                ctx.request_repaint_after(self.due.saturating_duration_since(now));
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = native_services;
            if !self.watching {
                self.watching = true;
                let (sender, receiver) = mpsc::channel();
                self.receiver = Some(receiver);
                let ctx = ctx.clone();
                thinoda_platform::web::watch_updates(move |message| {
                    let result = parse_web_notice(&message);
                    let _ = sender.send(result);
                    ctx.request_repaint();
                });
            }
        }
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui) {
        self.show_translated(ui, t);
    }

    fn show_translated(&mut self, ui: &mut egui::Ui, t: impl Fn(&'static str) -> &'static str) {
        let Some(notice) = &self.notice else { return };
        let mut dismiss = false;
        egui::Panel::top("thinoda_update_notice").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(match notice.kind {
                    #[cfg(any(not(target_arch = "wasm32"), test))]
                    Kind::Available => t("New version available"),
                    Kind::Ready => t("Update ready"),
                    #[cfg(any(target_arch = "wasm32", test))]
                    Kind::Applied => t("Update installed successfully"),
                });
                ui.label(format!("{}: {}", t("Current version"), env!("CARGO_PKG_VERSION")));
                ui.label(format!("Thinoda {}", notice.version));
                if notice.kind == Kind::Ready {
                    ui.label(t("Save your work, then close all app windows and reopen to apply the update."));
                }
                ui.hyperlink_to(t("Version and changelog"), &notice.release);
                #[cfg(any(not(target_arch = "wasm32"), test))]
                if notice.kind == Kind::Available {
                    ui.hyperlink_to(t("Download new version"), format!("{}#assets", notice.release));
                }
                dismiss = ui.button(t("Dismiss")).clicked();
            });
        });
        if dismiss {
            self.dismissed = self.notice.take().map(|notice| notice.version);
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn parse_web_notice(message: &str) -> Result<Option<Notice>, String> {
    let value: serde_json::Value =
        serde_json::from_str(message).map_err(|_| "invalid web notice")?;
    let version = value["version"].as_str().ok_or("missing web version")?;
    stable_version(version).ok_or("invalid web version")?;
    let revision = value["revision"].as_str().ok_or("missing web revision")?;
    if revision.len() != 16 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid web revision".into());
    }
    let kind = match value["kind"].as_str() {
        Some("ready") => Kind::Ready,
        Some("applied") => Kind::Applied,
        _ => return Err("invalid web update state".into()),
    };
    Ok(Some(Notice {
        kind,
        version: format!("{version} ({revision})"),
        release: format!("{RELEASES}/tag/v{version}"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(tag: &str) -> String {
        serde_json::json!({
            "draft": false, "prerelease": false, "tag_name": tag,
            "html_url": format!("{RELEASES}/tag/{tag}")
        })
        .to_string()
    }

    #[test]
    fn stable_releases_are_strict_ordered_and_repository_bound() {
        assert!(
            parse_release(&payload("v0.18.0"), "0.17.0")
                .unwrap()
                .is_some()
        );
        for tag in ["v0.17.0", "v0.9.9"] {
            assert!(parse_release(&payload(tag), "0.17.0").unwrap().is_none());
        }

        for tag in [
            "v0.18",
            "v0.18.0-rc.1",
            "v0.18.0+build",
            "../evil",
            "999999999999999999999.0.0",
        ] {
            assert!(parse_release(&payload(tag), "0.17.0").is_err());
        }
        for field in ["draft", "prerelease"] {
            let mut value: serde_json::Value = serde_json::from_str(&payload("v1.0.0")).unwrap();
            value[field] = true.into();
            assert!(parse_release(&value.to_string(), "0.17.0").is_err());
        }
        for url in [
            "http://github.com/AxelWol/Thinoda/releases/tag/v1.0.0",
            "https://github.com.evil/AxelWol/Thinoda/releases/tag/v1.0.0",
            "https://github.com/other/Thinoda/releases/tag/v1.0.0",
        ] {
            let mut value: serde_json::Value = serde_json::from_str(&payload("v1.0.0")).unwrap();
            value["html_url"] = url.into();
            assert!(parse_release(&value.to_string(), "0.17.0").is_err());
        }
        assert!(parse_release("{}", "0.17.0").is_err());
        assert!(parse_release("not json", "0.17.0").is_err());
    }

    #[test]
    fn native_http_is_bounded_unauthenticated_and_rejects_errors_and_redirects() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        for (status, body) in [
            ("200 OK", payload("v999.0.0")),
            ("200 OK", "x".repeat(256 * 1024 + 1)),
            ("200 OK", "invalid json".into()),
            ("429 Too Many Requests", payload("v999.0.0")),
            ("302 Found", payload("v999.0.0")),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}/release", listener.local_addr().unwrap());
            listener.set_nonblocking(true).unwrap();
            let expected = status == "200 OK" && body.len() < 256 * 1024 && body != "invalid json";
            let server = std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "No local update request");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..count]);
                    assert!(request.len() < 8192);
                }
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nLocation: https://example.invalid/\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                String::from_utf8(request).unwrap()
            });
            let result = fetch_release_from(&endpoint);
            assert_eq!(result.is_ok(), expected, "{result:?}");
            let request = server.join().unwrap().to_ascii_lowercase();
            assert!(request.starts_with("get /release http/1.1"));
            assert!(request.contains("user-agent: thinoda/"));
            assert!(!request.contains("authorization:"));
            assert!(!request.contains("cookie:"));
        }
    }

    #[test]
    fn completion_backoff_and_dismissal_do_not_touch_documents() {
        let mut updates = Updates::default();
        let now = Instant::now();
        updates.accept(Err("offline".into()), now);
        assert_eq!(updates.due.duration_since(now), RETRY * 2);
        for _ in 0..20 {
            updates.accept(Err("rate limited".into()), now);
        }
        assert!(updates.due.duration_since(now) <= PERIOD);
        let notice = parse_release(&payload("v1.0.0"), "0.17.0").unwrap();
        updates.accept(Ok(notice.clone()), now);
        assert_eq!(updates.due.duration_since(now), PERIOD);
        updates.dismissed = Some("1.0.0".into());
        updates.notice = None;
        updates.accept(Ok(notice), now);
        assert!(updates.notice.is_none());
        updates.accept(parse_release(&payload("v1.1.0"), "0.17.0"), now);
        assert_eq!(updates.notice.as_ref().unwrap().version, "1.1.0");
        updates.accept(Ok(None), now);
        assert!(updates.notice.is_none()); // Withdrawn/no-longer-newer release clears stale UI.
        assert!(!updates.is_due(now, updates.wall_due - Duration::from_secs(1)));
        assert!(updates.is_due(now, updates.wall_due + Duration::from_secs(1)));
        assert!(updates.is_due(now + PERIOD, updates.wall_due - PERIOD));
    }

    #[test]
    fn web_notices_reject_unknown_states_and_untrusted_versions() {
        for kind in ["ready", "applied"] {
            let message =
                serde_json::json!({"kind":kind,"version":"0.18.0","revision":"1234567890abcdef"});
            assert!(parse_web_notice(&message.to_string()).unwrap().is_some());
        }
        for message in [
            r#"{"kind":"success","version":"0.18.0","revision":"1234567890abcdef"}"#,
            r#"{"kind":"applied","version":"https://evil","revision":"1234567890abcdef"}"#,
            "{}",
        ] {
            assert!(parse_web_notice(message).is_err());
        }
    }

    #[test]
    fn worker_completion_is_polled_once_and_headless_apps_never_start_networking() {
        let ctx = egui::Context::default();
        let mut updates = Updates::default();
        let (sender, receiver) = mpsc::channel();
        updates.receiver = Some(receiver);
        updates.poll(&ctx, false);
        assert!(updates.receiver.is_some());
        sender
            .send(parse_release(&payload("v1.0.0"), "0.17.0"))
            .unwrap();
        updates.poll(&ctx, false);
        assert!(updates.receiver.is_none());
        assert_eq!(updates.notice.as_ref().unwrap().version, "1.0.0");
        updates.due = Instant::now();
        updates.poll(&ctx, false);
        assert!(updates.receiver.is_none());
        assert_eq!(updates.failures, 0);
    }

    #[test]
    fn notice_delivery_does_not_change_unsaved_work_or_undo_state() {
        let mut app = crate::ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Unsaved work", |sheet| {
                sheet.set_title(root, "Keep this draft")
            })
            .unwrap();
        let before = app.doc().unwrap().doc.workbook().clone();
        let revision = app.doc().unwrap().doc.state_id();
        assert!(app.doc().unwrap().is_dirty());
        let (sender, receiver) = mpsc::channel();
        app.updates.receiver = Some(receiver);
        sender
            .send(parse_release(&payload("v1.0.0"), "0.17.0"))
            .unwrap();
        let ctx = egui::Context::default();
        crate::install_context(&ctx);
        app.updates.poll(&ctx, false);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw(ui));
        output.textures_delta.clear();
        assert!(app.updates.notice.is_some());
        assert!(app.doc().unwrap().is_dirty());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert_eq!(app.doc().unwrap().doc.state_id(), revision);
        assert!(app.doc_mut().unwrap().doc.undo());
    }

    #[test]
    fn every_language_and_theme_has_nonmodal_notices() {
        use crate::i18n::{Lang, t_in};
        use egui_kittest::{Harness, kittest::Queryable};
        for lang in Lang::ALL {
            for dark in [false, true] {
                for (kind, title) in [
                    (Kind::Available, "New version available"),
                    (Kind::Ready, "Update ready"),
                    (Kind::Applied, "Update installed successfully"),
                ] {
                    for key in [
                        "New version available",
                        "Update ready",
                        "Update installed successfully",
                        "Version and changelog",
                        "Download new version",
                        "Current version",
                        "Dismiss",
                        "Save your work, then close all app windows and reopen to apply the update.",
                    ] {
                        assert!(!t_in(lang, key).is_empty());
                        if lang != Lang::En {
                            assert_ne!(t_in(lang, key), key);
                        }
                    }
                    let mut updates = Updates {
                        notice: Some(Notice {
                            kind,
                            version: "0.18.0".into(),
                            release: format!("{RELEASES}/tag/v0.18.0"),
                        }),
                        ..Default::default()
                    };
                    let mut harness = Harness::builder()
                        .with_size(egui::vec2(360.0, 780.0))
                        .build_ui(|ui| {
                            ui.ctx().set_visuals(if dark {
                                egui::Visuals::dark()
                            } else {
                                egui::Visuals::light()
                            });
                            updates.show_translated(ui, |key| t_in(lang, key));
                            ui.label("Document remains editable");
                        });
                    harness.run();
                    harness.get_by_label(t_in(lang, title));
                    harness.get_by_label(t_in(lang, "Version and changelog"));
                    harness.get_by_label("Document remains editable");
                    harness.get_by_label(t_in(lang, "Dismiss")).click();
                    harness.run();
                    drop(harness);
                    assert!(updates.notice.is_none());
                }
            }
        }
    }
}
