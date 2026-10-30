use super::*;
use crate::doc::OpenDoc;
use thinoda_platform::Paths;

#[test]
fn async_import_identity_race_refuses_association_and_native_xmind_can_bind() {
    let (dir, mut app, _key) = fixture();
    let path = app.doc().unwrap().path.clone().unwrap();
    let original = thinoda_platform::fs::read(&path).unwrap();
    let report =
        thinoda_io::import_bytes_with_report(thinoda_io::Format::Thinoda, &original).unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Replacement", |sheet| {
            sheet.set_title(sheet.root, "Different file version")
        })
        .unwrap();
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let (stale, _) = OpenDoc::from_import_report(&path, report).unwrap();
    assert!(stale.chat_identity.is_none());
    let xmind = dir.path().join("map.xmind");
    app.doc_mut().unwrap().save_to(&xmind, false).unwrap();
    let reopened = OpenDoc::open(&xmind).unwrap();
    assert!(reopened.chat_identity.is_some());
}

fn fixture() -> (tempfile::TempDir, ThinodaApp, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut app = ThinodaApp::headless();
    app.services.paths = Some(Paths::from_root(dir.path().join("app")));
    let path = dir.path().join("map.thinoda");
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let key = app.doc().unwrap().key.clone();
    app.load_chat_history(&key);
    (dir, app, key)
}

fn enable(app: &mut ThinodaApp, key: &str, name: &str) {
    app.enable_chat_history(key);
    app.ai.chat.history.name = name.into();
    app.edit_chat_thread(key, false);
    assert!(app.ai.chat.history.error.is_none());
}

fn pair(app: &mut ThinodaApp, key: &str) {
    let thread = app.ai.chat.threads.get_mut(key).unwrap();
    thread.messages = vec![
        Message::user("Authored prompt"),
        Message {
            role: "assistant".into(),
            content: "Completed answer".into(),
        },
    ];
    thread.draft = "Unsent secret draft".into();
}

#[test]
fn opt_in_restart_named_threads_and_exports_without_drafts_or_provider_settings() {
    let (_dir, mut app, key) = fixture();
    app.settings.ai.api_key = "provider-secret-fixture".into();
    app.ai
        .chat
        .threads
        .entry(key.clone())
        .or_default()
        .messages
        .push(Message::user("Session-only fixture"));
    let path = app.doc().unwrap().path.clone().unwrap();
    let before = thinoda_platform::fs::read(&path).unwrap();
    assert!(!app.services.paths.as_ref().unwrap().data_dir.exists());
    enable(&mut app, &key, "Research");
    assert!(app.ai.chat.threads[&key].messages.is_empty());
    pair(&mut app, &key);
    app.persist_chat_thread(&key);
    let first = app.ai.chat.threads[&key].thread_id.clone();
    app.ai.chat.history.name = "Second".into();
    app.edit_chat_thread(&key, false);
    assert_ne!(app.ai.chat.threads[&key].thread_id, first);
    app.select_chat_thread(&key, &first);
    assert_eq!(app.ai.chat.threads[&key].draft, "Unsent secret draft");
    app.ai.chat.history.name = "Renamed".into();
    app.edit_chat_thread(&key, true);
    let store = Store::load(app.services.paths.as_ref().unwrap()).unwrap();
    let raw = serde_json::to_string(&store).unwrap();
    assert!(raw.contains("Renamed") && raw.contains("Completed answer"));
    for forbidden in [
        "Unsent secret draft",
        "provider-secret-fixture",
        "Session-only fixture",
        "system",
        "context",
    ] {
        assert!(!raw.contains(forbidden));
    }
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    assert_eq!(thinoda_platform::fs::read(&path).unwrap(), before);
    let mut restarted = ThinodaApp::headless();
    restarted.services.paths = app.services.paths.clone();
    restarted.docs = vec![OpenDoc::open(&path).unwrap()];
    restarted.active = 0;
    let reopened = restarted.doc().unwrap().key.clone();
    restarted.load_chat_history(&reopened);
    assert_eq!(restarted.ai.chat.threads[&reopened].messages.len(), 2);
    assert!(restarted.ai.chat.threads[&reopened].draft.is_empty());
    assert!(restarted.ai.chat.pending.is_none());
    app.delete_chat_thread(&key);
    assert_eq!(
        Store::load(app.services.paths.as_ref().unwrap())
            .unwrap()
            .documents[0]
            .threads
            .len(),
        1
    );
}

#[test]
fn same_path_save_advances_binding_but_save_as_and_recovery_detach() {
    let (dir, mut app, key) = fixture();
    enable(&mut app, &key, "Research");
    pair(&mut app, &key);
    app.persist_chat_thread(&key);
    let original = app.ai.chat.history.documents[&key].binding.clone();
    let path = app.doc().unwrap().path.clone().unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Fixture", |sheet| sheet.set_title(sheet.root, "Changed"))
        .unwrap();
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    app.sync_chat_bindings();
    let changed = app.ai.chat.history.documents[&key].binding.clone();
    assert_ne!(original, changed);
    assert_eq!(changed, Binding::read(&path).unwrap());
    let copy = dir.path().join("copy.thinoda");
    app.doc_mut().unwrap().save_to(&copy, false).unwrap();
    app.sync_chat_bindings();
    assert!(!app.ai.chat.history.documents.contains_key(&key));
    assert_eq!(app.ai.chat.threads[&key].messages.len(), 2);
    app.persist_chat_thread(&key);
    let disk = Store::load(app.services.paths.as_ref().unwrap()).unwrap();
    assert_eq!(disk.documents[0].binding, changed);
    let mut recovered = OpenDoc::new(app.doc().unwrap().doc.workbook().clone());
    recovered.path = Some(path);
    assert!(recovered.chat_identity.is_none());
    app.docs.push(recovered);
    app.active = app.docs.len() - 1;
    let recovery_key = app.doc().unwrap().key.clone();
    app.load_chat_history(&recovery_key);
    assert!(!app.ai.chat.history.documents.contains_key(&recovery_key));
}

#[test]
fn failed_delete_disable_and_clear_leave_memory_and_disk_unchanged() {
    let (_dir, mut app, key) = fixture();
    enable(&mut app, &key, "Research");
    pair(&mut app, &key);
    app.persist_chat_thread(&key);
    let paths = app.services.paths.as_ref().unwrap().clone();
    let before = Store::load(&paths).unwrap();
    let lock = paths.data_dir.join("ai-conversations.lock");
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();
    app.delete_chat_thread(&key);
    assert_eq!(app.ai.chat.threads[&key].messages.len(), 2);
    app.disable_chat_history(&key);
    assert!(app.ai.chat.history.documents.contains_key(&key));
    let thread = app.ai.chat.threads.remove(&key).unwrap();
    assert!(!app.clear_durable_chat(&key, &thread));
    app.ai.chat.threads.insert(key.clone(), thread);
    assert_eq!(Store::load(&paths).unwrap(), before);
    assert!(app.ai.chat.history.error.is_some());
}

#[test]
fn thread_name_duplicates_limits_and_session_restore_are_safe() {
    let (_dir, mut app, key) = fixture();
    app.ai.chat.threads.entry(key.clone()).or_default().draft = "Session draft".into();
    enable(&mut app, &key, "Research");
    let id = app.ai.chat.threads[&key].thread_id.clone();
    app.ai.chat.history.name = "RESEARCH".into();
    app.edit_chat_thread(&key, false);
    assert!(app.ai.chat.history.error.is_some());
    assert_eq!(app.ai.chat.threads[&key].thread_id, id);
    app.select_session_chat(&key);
    assert_eq!(app.ai.chat.threads[&key].draft, "Session draft");
    app.select_chat_thread(&key, &id);
    pair(&mut app, &key);
    app.disable_chat_history(&key);
    assert!(!app.ai.chat.history.documents.contains_key(&key));
    assert_eq!(app.ai.chat.threads[&key].messages.len(), 2);
    assert!(
        Store::load(app.services.paths.as_ref().unwrap())
            .unwrap()
            .documents
            .is_empty()
    );
    assert_eq!(
        app.ai.chat.history.detached[&key].session.draft,
        "Session draft"
    );
}

#[test]
fn expired_threads_detach_without_losing_unsaved_memory() {
    let (_dir, mut app, key) = fixture();
    enable(&mut app, &key, "Old");
    pair(&mut app, &key);
    let mut next = app.ai.chat.history.store.clone().unwrap();
    next.documents[0].threads[0].updated = 1;
    next.expire(now());
    assert!(app.chat_commit(next));
    app.reconcile_chat_retention();
    assert_eq!(app.ai.chat.threads[&key].messages.len(), 2);
    assert_eq!(app.ai.chat.threads[&key].draft, "Unsent secret draft");
    assert!(!app.ai.chat.history.documents.contains_key(&key));
}

fn send(app: &mut ThinodaApp, key: &str, reply: &str) -> thinoda_ai::mock::MockServer {
    let delta = serde_json::json!({"choices":[{"delta":{"content": reply}}]}).to_string();
    let server = thinoda_ai::mock::MockServer::start_sse(&["mock-chat"], &[&delta]);
    app.settings.ai.base_url = server.base_url.clone();
    app.settings.ai.model = "mock-chat".into();
    let mut thread = app.ai.chat.threads.remove(key).unwrap_or_default();
    thread.draft = "Explicit question".into();
    let scope = context(app.doc().unwrap(), thread.scope).unwrap();
    app.send_ai_chat(key, &mut thread, &scope);
    app.ai.chat.threads.insert(key.into(), thread);
    assert!(app.ai.chat.pending.is_some());
    server
}

fn finish(app: &mut ThinodaApp) {
    let ctx = egui::Context::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.ai.chat.pending.is_some() {
        assert!(std::time::Instant::now() < deadline);
        app.poll_ai_chat(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn completed_stream_persists_only_to_owning_document_thread_and_viewport() {
    let (_dir, mut app, key) = fixture();
    enable(&mut app, &key, "First");
    let first = app.ai.chat.threads[&key].thread_id.clone();
    app.ai.chat.history.name = "Second".into();
    app.edit_chat_thread(&key, false);
    let second = app.ai.chat.threads[&key].thread_id.clone();
    app.select_chat_thread(&key, &first);
    app.current_viewport = egui::ViewportId::from_hash_of("chat-test");
    let _server = send(&mut app, &key, "Owned reply");
    assert!(app.ai.chat.pending.as_ref().unwrap().durable);
    app.select_chat_thread(&key, &second); // Refused while in flight.
    app.delete_chat_thread(&key);
    app.disable_chat_history(&key);
    assert_eq!(app.ai.chat.threads[&key].thread_id, first);
    let viewport = app.current_viewport;
    app.docs
        .push(OpenDoc::new(thinoda_core::Workbook::default()));
    app.active = app.docs.len() - 1;
    assert_eq!(app.ai.chat.pending.as_ref().unwrap().viewport, viewport);
    app.return_ai_chat_viewport(viewport);
    assert_eq!(
        app.ai.chat.pending.as_ref().unwrap().viewport,
        egui::ViewportId::ROOT
    );
    finish(&mut app);
    let disk = Store::load(app.services.paths.as_ref().unwrap()).unwrap();
    let threads = &disk.documents[0].threads;
    assert_eq!(
        threads
            .iter()
            .find(|thread| thread.id == first)
            .unwrap()
            .messages
            .len(),
        2
    );
    assert!(
        threads
            .iter()
            .find(|thread| thread.id == second)
            .unwrap()
            .messages
            .is_empty()
    );
}

#[test]
fn cancellation_stale_thread_and_session_stream_never_write_transient_text() {
    let (_dir, mut app, key) = fixture();
    enable(&mut app, &key, "First");
    let _server = send(&mut app, &key, "Cancelled reply");
    app.cancel_ai_chat();
    finish(&mut app);
    assert!(
        Store::load(app.services.paths.as_ref().unwrap())
            .unwrap()
            .documents[0]
            .threads[0]
            .messages
            .is_empty()
    );
    let _server = send(&mut app, &key, "Stale reply");
    app.ai.chat.threads.get_mut(&key).unwrap().thread_id = uuid::Uuid::new_v4().to_string();
    finish(&mut app);
    assert!(app.ai.chat.threads[&key].messages.is_empty());
    assert!(
        Store::load(app.services.paths.as_ref().unwrap())
            .unwrap()
            .documents[0]
            .threads[0]
            .messages
            .is_empty()
    );
    app.select_session_chat(&key);
    let _server = send(&mut app, &key, "Session reply");
    assert!(!app.ai.chat.pending.as_ref().unwrap().durable);
    finish(&mut app);
    assert_eq!(app.ai.chat.threads[&key].messages.len(), 2);
    assert!(
        Store::load(app.services.paths.as_ref().unwrap())
            .unwrap()
            .documents[0]
            .threads[0]
            .messages
            .is_empty()
    );
}

#[test]
fn default_off_send_and_open_make_no_history_writes_or_implicit_network_requests() {
    let (_dir, mut app, key) = fixture();
    let delta = r#"{"choices":[{"delta":{"content":"Only on Send"}}]}"#;
    let server = thinoda_ai::mock::MockServer::start_sse(&["mock-chat"], &[delta]);
    app.settings.ai.base_url = server.base_url.clone();
    app.ai.chat.open = true;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1000.0, 720.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(3);
    assert!(server.requests.lock().unwrap().is_empty());
    assert!(
        !harness
            .state()
            .services
            .paths
            .as_ref()
            .unwrap()
            .data_dir
            .exists()
    );
    let _server = send(harness.state_mut(), &key, "Only session");
    finish(harness.state_mut());
    assert_eq!(harness.state().ai.chat.threads[&key].messages.len(), 2);
    assert!(
        !harness
            .state()
            .services
            .paths
            .as_ref()
            .unwrap()
            .data_dir
            .exists()
    );
}

#[test]
fn failed_completion_write_keeps_completed_exchange_for_explicit_retry() {
    let (_dir, mut app, key) = fixture();
    enable(&mut app, &key, "Research");
    let paths = app.services.paths.as_ref().unwrap().clone();
    let original = Store::load(&paths).unwrap();
    // Simulate a cooperating concurrent client's successful revision.
    original.commit(&paths, original.clone()).unwrap();
    let _server = send(&mut app, &key, "Unpersisted completed answer");
    finish(&mut app);
    assert_eq!(app.ai.chat.threads[&key].messages.len(), 2);
    assert!(app.ai.chat.history.error.is_some());
    assert!(
        Store::load(&paths).unwrap().documents[0].threads[0]
            .messages
            .is_empty()
    );
}

#[test]
fn history_common_dialog_fits_narrow_light_dark_and_never_implicitly_sends() {
    use egui_kittest::kittest::Queryable;
    for dark in [false, true] {
        let (_dir, mut app, key) = fixture();
        app.ai.chat.history.confirm = Some(Action::Enable);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(360.0, 640.0))
            .build_ui_state(
                move |ui, app: &mut ThinodaApp| {
                    ui.ctx().set_visuals(if dark {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    });
                    egui::ScrollArea::vertical().show(ui, |ui| app.chat_history_controls(ui, &key));
                },
                app,
            );
        harness.run_steps(3);
        let confirm = harness.get_by_role_and_label(egui::accesskit::Role::Button, t("Confirm"));
        assert!(harness.ctx.content_rect().contains_rect(confirm.rect()));
        confirm.click();
        harness.run_steps(3);
        assert!(harness.state().ai.chat.pending.is_none());
        assert_eq!(harness.state().ai.chat.history.documents.len(), 1);
        assert!(
            harness
                .state()
                .ai
                .chat
                .history
                .store
                .as_ref()
                .unwrap()
                .documents[0]
                .threads
                .is_empty()
        );
    }
}

#[test]
fn history_labels_and_privacy_are_present_in_all_ten_languages() {
    for lang in crate::i18n::Lang::ALL {
        for label in [
            "Local chat history",
            "Local chat history is enabled.",
            "Thread name",
            "New thread",
            "Session-only conversation",
            "Delete thread",
            "Delete all local chat history",
            "Confirm local history change?",
            "Prompts and replies may contain sensitive data. Local history is not encrypted. Never included in maps or exports.",
        ] {
            let translated = crate::i18n::t_in(lang, label);
            assert!(!translated.is_empty());
            if lang != crate::i18n::Lang::En {
                assert_ne!(translated, label);
            }
        }
    }
}

#[test]
fn failed_binding_advance_cannot_be_hidden_by_a_successful_transcript_write() {
    let (_dir, mut app, key) = fixture();
    enable(&mut app, &key, "Research");
    let paths = app.services.paths.as_ref().unwrap().clone();
    let original = Store::load(&paths).unwrap();
    original.commit(&paths, original.clone()).unwrap();
    let path = app.doc().unwrap().path.clone().unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Change", |sheet| sheet.set_title(sheet.root, "New bytes"))
        .unwrap();
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    app.sync_chat_bindings();
    assert!(app.ai.chat.history.error.is_some());
    pair(&mut app, &key);
    app.persist_chat_thread(&key);
    assert!(app.ai.chat.history.error.is_some());
    assert_eq!(app.ai.chat.threads[&key].messages.len(), 2);
    let disk = Store::load(&paths).unwrap();
    assert!(disk.documents[0].threads[0].messages.is_empty());
}

#[test]
fn history_confirmation_cannot_follow_a_document_or_viewport_switch() {
    let (_dir, mut app, key) = fixture();
    app.ai.chat.history.confirm = Some(Action::Enable);
    app.ai.chat.history.owner = Some((key.clone(), String::new(), egui::ViewportId::ROOT));
    app.current_viewport = egui::ViewportId::from_hash_of("another-window");
    let mut harness = egui_kittest::Harness::builder().build_ui_state(
        move |ui, app: &mut ThinodaApp| app.chat_history_controls(ui, &key),
        app,
    );
    harness.run_steps(2);
    assert!(harness.state().ai.chat.history.confirm.is_none());
    assert!(harness.state().ai.chat.history.documents.is_empty());
}

#[test]
fn enabling_before_creating_a_thread_keeps_the_existing_session_visible() {
    let (_dir, mut app, key) = fixture();
    app.ai.chat.threads.entry(key.clone()).or_default().draft = "Original session draft".into();
    app.enable_chat_history(&key);
    assert_eq!(app.ai.chat.threads[&key].draft, "Original session draft");
    assert!(app.ai.chat.threads[&key].thread_id.is_empty());
    app.ai.chat.history.name = "Research".into();
    app.edit_chat_thread(&key, false);
    assert!(app.ai.chat.threads[&key].draft.is_empty());
    app.select_session_chat(&key);
    assert_eq!(app.ai.chat.threads[&key].draft, "Original session draft");
    let id = app.ai.chat.history.store.as_ref().unwrap().documents[0].threads[0]
        .id
        .clone();
    app.select_chat_thread(&key, &id);
    app.delete_chat_thread(&key);
    assert_eq!(app.ai.chat.threads[&key].draft, "Original session draft");
}
