use super::*;
use crate::app::Cmd;
use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};

fn harness(
    reply: &str,
) -> (
    Harness<'static, ThinodaApp>,
    thinoda_ai::mock::MockServer,
    String,
) {
    let delta = serde_json::json!({"choices":[{"delta":{"content":reply}}]}).to_string();
    let server =
        thinoda_ai::mock::MockServer::start_sse(&["text-embedding-x", "mock-chat"], &[&delta]);
    with_server(server, egui::vec2(1000.0, 720.0))
}

fn with_server(
    server: thinoda_ai::mock::MockServer,
    size: egui::Vec2,
) -> (
    Harness<'static, ThinodaApp>,
    thinoda_ai::mock::MockServer,
    String,
) {
    let mut app = ThinodaApp::headless();
    app.settings.ai.base_url = server.base_url.clone();
    let key = app.doc().unwrap().key.clone();
    app.ai.chat.open = true;
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(3);
    (h, server, key)
}

fn wait(h: &mut Harness<'static, ThinodaApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.state().ai.chat.pending.is_some() {
        assert!(std::time::Instant::now() < deadline, "chat did not finish");
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run_steps(1);
    }
    h.run_steps(3);
}

#[test]
fn scoped_chat_context_preserves_hierarchy_without_notes_or_unselected_trees() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let (inside, outside, floating) = doc
        .doc
        .edit_sheet("Fixture", |sheet| {
            let inside = sheet.insert_child(sheet.root, "Inside")?;
            let outside = sheet.insert_child(sheet.root, "Outside")?;
            let mut parent = inside;
            for depth in 0..10 {
                parent = sheet.insert_child(parent, &format!("Nested {depth}"))?;
            }
            sheet.set_notes(inside, Some("Private note".into()))?;
            let floating = sheet.add_floating("Floating", thinoda_core::Vec2::new(900.0, 400.0));
            Ok((inside, outside, floating))
        })
        .unwrap();
    doc.view.select_only(inside);
    let selected = context(doc, Scope::Selection).unwrap();
    assert!(selected.contains("Inside") && selected.contains("Nested 9"));
    assert!(!selected.contains("Outside") && !selected.contains("Floating"));
    assert!(!selected.contains("Private note"));
    let parsed: serde_json::Value = serde_json::from_str(&selected).unwrap();
    assert_eq!(parsed["roots"], serde_json::json!([inside]));
    assert!(parsed["topics"][0]["children"].as_array().unwrap().len() == 1);
    let sheet = context(doc, Scope::Sheet).unwrap();
    assert!(sheet.contains("Outside") && sheet.contains("Floating"));
    assert!(!sheet.contains("Private note"));
    assert_ne!(inside, outside);
    assert_ne!(outside, floating);
}

#[test]
fn chat_rejects_large_or_uncommitted_context_instead_of_truncating() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.view
        .begin_edit_with(doc.doc.sheet(), root, "Uncommitted");
    assert!(context(doc, Scope::Sheet).is_err());
    doc.view.commit_edit(&mut doc.doc);
    doc.doc
        .edit_sheet("Large fixture", |sheet| {
            for _ in 0..350 {
                sheet.insert_child(root, &"Long title ".repeat(20))?;
            }
            Ok(())
        })
        .unwrap();
    assert!(context(doc, Scope::Sheet).is_err());
}

#[test]
fn chat_topic_count_limit_includes_the_selected_root() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    let initial: serde_json::Value =
        serde_json::from_str(&context(doc, Scope::Sheet).unwrap()).unwrap();
    let existing = initial["topics"].as_array().unwrap().len();
    doc.doc
        .edit_sheet("Count fixture", |sheet| {
            for _ in existing..MAX_TOPICS {
                sheet.insert_child(root, "x")?;
            }
            Ok(())
        })
        .unwrap();
    assert!(context(doc, Scope::Sheet).is_ok());
    doc.doc
        .edit_sheet("One too many", |sheet| sheet.insert_child(root, "x"))
        .unwrap();
    assert!(context(doc, Scope::Sheet).is_err());
}

#[test]
fn clearing_another_conversation_does_not_cancel_the_active_chat() {
    let delta = r#"{"choices":[{"delta":{"content":"part "}}]}"#;
    let events = vec![delta; 4095];
    let server = thinoda_ai::mock::MockServer::start_sse(&["mock-chat"], &events);
    let (mut h, _server, key) = with_server(server, egui::vec2(1000.0, 720.0));
    h.state_mut()
        .ai
        .chat
        .threads
        .entry(key.clone())
        .or_default()
        .draft = "Discuss".into();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Send").click();
    h.run_steps(1);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::New);
    let other = h.state().doc().unwrap().key.clone();
    h.state_mut()
        .ai
        .chat
        .threads
        .entry(other.clone())
        .or_default()
        .messages = vec![
        Message::user("Earlier question"),
        Message {
            role: "assistant".into(),
            content: "Earlier answer".into(),
        },
    ];
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Clear conversation")
        .click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Confirm Clear")
        .click();
    h.run_steps(2);
    assert_eq!(h.state().ai.chat.pending.as_ref().unwrap().doc_key, key);
    assert!(!h.state().ai.chat.threads.contains_key(&other));
    h.state_mut().cancel_ai_chat();
}

#[test]
fn chat_ui_sends_only_deliberately_and_retains_history_without_map_edits() {
    let (mut h, server, key) = harness("A local discussion answer.");
    assert!(
        server.requests.lock().unwrap().is_empty(),
        "opening chat is offline"
    );
    let before = {
        let doc = h.state().doc().unwrap();
        (doc.doc.workbook().clone(), doc.doc.state_id())
    };
    h.state_mut().settings.ai.response_mode = AiResponseMode::Deep;
    h.state_mut()
        .ai
        .chat
        .threads
        .entry(key.clone())
        .or_default()
        .draft = "What is missing?".into();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Send").click();
    h.run_steps(1);
    wait(&mut h);
    let messages = &h.state().ai.chat.threads[&key].messages;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].content, "A local discussion answer.");
    assert!(h.state().ai.pending.is_none() && h.state().ai.review.is_none());
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::AiChat);
    assert!(!h.state().ai.chat.open);
    h.state_mut().exec(&ctx, Cmd::AiChat);
    assert_eq!(h.state().ai.chat.threads[&key].messages.len(), 2);
    h.state_mut().ai.chat.threads.get_mut(&key).unwrap().draft = "A follow-up question".into();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Send").click();
    h.run_steps(1);
    wait(&mut h);
    assert_eq!(h.state().ai.chat.threads[&key].messages.len(), 4);
    let requests = server.requests.lock().unwrap();
    let chats: Vec<serde_json::Value> = requests
        .iter()
        .filter(|request| request.path.ends_with("/chat/completions"))
        .map(|request| serde_json::from_str(&request.body).unwrap())
        .collect();
    assert_eq!(chats.len(), 2);
    assert_eq!(chats[0]["model"], "mock-chat");
    assert_eq!(chats[0]["stream"], true);
    assert!(chats[0].get("reasoning_effort").is_none());
    assert!(
        chats[0]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("structured answer")
    );
    assert_eq!(chats[1]["messages"][2]["role"], "assistant");
    assert_eq!(
        chats[1]["messages"][2]["content"],
        "A local discussion answer."
    );
    let doc = h.state().doc().unwrap();
    assert_eq!((doc.doc.workbook().clone(), doc.doc.state_id()), before);
}

#[test]
fn chat_completion_stays_with_its_document_and_closed_sources_are_pruned() {
    let (mut h, _server, key) = harness("Original document answer");
    h.state_mut()
        .ai
        .chat
        .threads
        .entry(key.clone())
        .or_default()
        .draft = "Discuss".into();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Send").click();
    h.run_steps(1);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::New);
    let other = h.state().doc().unwrap().key.clone();
    wait(&mut h);
    assert_eq!(h.state().ai.chat.threads[&key].messages.len(), 2);
    assert!(!h.state().ai.chat.threads.contains_key(&other));
    h.state_mut().docs.retain(|doc| doc.key != key);
    h.state_mut().active = 0;
    h.state_mut().poll_ai(&ctx);
    assert!(!h.state().ai.chat.threads.contains_key(&key));
}

#[test]
fn chat_ime_candidate_enter_cannot_send_and_clear_requires_confirmation() {
    let (mut h, server, key) = harness("No automatic request");
    h.state_mut()
        .ai
        .chat
        .threads
        .entry(key.clone())
        .or_default()
        .draft = "Draft".into();
    h.ctx.memory_mut(|memory| {
        memory.request_focus(egui::Id::unique("thinoda_ai_chat_input").with(&key))
    });
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "候補".into(),
        active_range_chars: None,
    }));
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
    h.run_steps(2);
    assert!(server.requests.lock().unwrap().is_empty());
    assert!(h.state().ai.chat.pending.is_none());
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: String::new(),
        active_range_chars: None,
    }));
    h.state_mut()
        .ai
        .chat
        .threads
        .get_mut(&key)
        .unwrap()
        .messages = vec![
        Message::user("Saved question"),
        Message {
            role: "assistant".into(),
            content: "Saved answer".into(),
        },
    ];
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Clear conversation")
        .click();
    h.run_steps(2);
    assert_eq!(h.state().ai.chat.threads[&key].messages.len(), 2);
    h.get_by_role_and_label(Role::Button, "Confirm Clear")
        .click();
    h.run_steps(2);
    let thread = &h.state().ai.chat.threads[&key];
    assert!(thread.messages.is_empty());
    assert_eq!(thread.draft, "Draft");
}

#[test]
fn chat_modes_and_scope_are_explicit_accessible_offline_choices() {
    let (mut h, server, key) = harness("Not requested");
    h.get_by_role_and_label(Role::ComboBox, "Response mode: Balanced")
        .click();
    h.run_steps(2);
    h.get_by_label("Quick").click();
    h.run_steps(2);
    assert_eq!(h.state().settings.ai.response_mode, AiResponseMode::Quick);
    h.get_by_role_and_label(Role::ComboBox, "Context scope: Selected topics")
        .click();
    h.run_steps(2);
    h.get_by_label("Current sheet").click();
    h.run_steps(2);
    assert!(h.state().ai.chat.threads[&key].scope == Scope::Sheet);
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn chat_send_is_reachable_in_a_short_narrow_window() {
    let delta = r#"{"choices":[{"delta":{"content":"Small window answer"}}]}"#;
    let server = thinoda_ai::mock::MockServer::start_sse(&["mock-chat"], &[delta]);
    let (mut h, _server, key) = with_server(server, egui::vec2(360.0, 340.0));
    h.state_mut()
        .ai
        .chat
        .threads
        .entry(key.clone())
        .or_default()
        .draft = "Discuss".into();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Send").scroll_to_me();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Send").click();
    h.run_steps(1);
    wait(&mut h);
    assert_eq!(h.state().ai.chat.threads[&key].messages.len(), 2);
}

#[test]
fn cancelled_disabled_and_closed_chat_sources_cannot_receive_late_results() {
    for stop in ["cancel", "disable", "close"] {
        let delta = r#"{"choices":[{"delta":{"content":"part "}}]}"#;
        let events = vec![delta; 4095];
        let server = thinoda_ai::mock::MockServer::start_sse(&["mock-chat"], &events);
        let (mut h, _server, key) = with_server(server, egui::vec2(1000.0, 720.0));
        h.state_mut()
            .ai
            .chat
            .threads
            .entry(key.clone())
            .or_default()
            .draft = "Keep this draft".into();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Send").click();
        h.run_steps(1);
        assert!(h.state().ai.chat.pending.is_some());
        match stop {
            "cancel" => {
                h.run_steps(1);
                h.get_by_role_and_label(Role::Button, "Cancel AI Request")
                    .click();
                h.run_steps(1);
            }
            "disable" => h.state_mut().settings.ai.enabled = false,
            "close" => h.state_mut().docs.clear(),
            _ => unreachable!(),
        }
        let ctx = h.ctx.clone();
        h.state_mut().poll_ai(&ctx);
        assert!(h.state().ai.chat.pending.is_none());
        std::thread::sleep(std::time::Duration::from_millis(30));
        h.state_mut().poll_ai(&ctx);
        if stop == "close" {
            assert!(!h.state().ai.chat.threads.contains_key(&key));
        } else {
            let thread = &h.state().ai.chat.threads[&key];
            assert!(thread.messages.is_empty());
            assert_eq!(thread.draft, "Keep this draft");
        }
    }
}

#[test]
fn chat_request_limits_reject_without_network_and_preserve_input() {
    let (mut h, server, key) = harness("Not requested");
    let scope_context = context(h.state().doc().unwrap(), Scope::Selection).unwrap();
    for (prompt, messages) in [
        ("x".repeat(MAX_PROMPT_BYTES + 1), Vec::new()),
        (
            "Valid draft".into(),
            (0..MAX_MESSAGES)
                .map(|_| Message::user("Previous"))
                .collect(),
        ),
        (
            "Valid draft".into(),
            vec![Message::user("x".repeat(MAX_HISTORY_BYTES))],
        ),
    ] {
        let mut thread = Conversation {
            draft: prompt.clone(),
            messages,
            ..Default::default()
        };
        h.state_mut()
            .send_ai_chat(&key, &mut thread, &scope_context);
        assert!(h.state().ai.chat.pending.is_none());
        assert!(thread.error.is_some());
        assert_eq!(thread.draft, prompt);
    }
    assert!(server.requests.lock().unwrap().is_empty());
}

fn saved_prompt() -> thinoda_platform::ai_prompts::SavedPrompt {
    thinoda_platform::ai_prompts::SavedPrompt {
        id: "test-prompt".into(),
        name: "Planning".into(),
        text: "Suggest practical next steps.".into(),
    }
}

fn button(h: &mut Harness<'_, ThinodaApp>, name: &str) {
    h.get_by_role_and_label(Role::Button, name).scroll_to_me();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, name).click();
    h.run_steps(3);
}

#[test]
fn saved_prompt_use_is_offline_and_preserves_drafts_until_confirmed() {
    let (mut h, server, key) = harness("Not requested");
    h.state_mut().settings.ai.saved_prompts = vec![saved_prompt()];
    h.run_steps(2);
    h.get_by_label("Saved prompts").click();
    h.run_steps(3);
    button(&mut h, "Use prompt");
    assert_eq!(h.state().ai.chat.threads[&key].draft, saved_prompt().text);
    h.state_mut().ai.chat.threads.get_mut(&key).unwrap().draft = "My draft".into();
    button(&mut h, "Use prompt");
    assert_eq!(h.state().ai.chat.threads[&key].draft, "My draft");
    button(&mut h, "Keep draft");
    assert_eq!(h.state().ai.chat.threads[&key].draft, "My draft");
    button(&mut h, "Use prompt");
    button(&mut h, "Replace draft");
    assert_eq!(h.state().ai.chat.threads[&key].draft, saved_prompt().text);
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn prompt_editor_save_cancel_delete_and_restart_do_not_touch_the_map_or_credentials() {
    let (mut h, server, _) = harness("Not requested");
    let temp = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(temp.path());
    paths.ensure().unwrap();
    h.state_mut().services.paths = Some(paths.clone());
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.state_mut().begin_prompt_editor(None);
    h.run_steps(3);
    h.get_by_role_and_label(Role::TextInput, "Prompt name")
        .click();
    h.get_by_role_and_label(Role::TextInput, "Prompt name")
        .type_text("Planning");
    h.run_steps(2);
    h.get_by_role_and_label(Role::MultilineTextInput, "Prompt text")
        .click();
    h.get_by_role_and_label(Role::MultilineTextInput, "Prompt text")
        .type_text("Suggest practical next steps.");
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    button(&mut h, "Cancel");
    assert!(h.state().settings.ai.saved_prompts.is_empty());
    assert!(!paths.config_dir.join("settings.toml").exists());
    h.state_mut().begin_prompt_editor(None);
    h.run_steps(3);
    h.get_by_role_and_label(Role::TextInput, "Prompt name")
        .click();
    h.get_by_role_and_label(Role::TextInput, "Prompt name")
        .type_text("Planning");
    h.run_steps(2);
    h.get_by_role_and_label(Role::MultilineTextInput, "Prompt text")
        .click();
    h.get_by_role_and_label(Role::MultilineTextInput, "Prompt text")
        .type_text("Suggest practical next steps.");
    h.run_steps(2);
    button(&mut h, "Save prompt");
    assert!(h.state().ai.prompt_editor.is_none());
    let loaded = thinoda_platform::ai_prompts::load(&paths).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, saved_prompt().name);
    assert_eq!(loaded[0].text, saved_prompt().text);
    h.state_mut().begin_prompt_editor(Some(loaded[0].clone()));
    h.run_steps(3);
    button(&mut h, "Delete prompt");
    assert_eq!(h.state().settings.ai.saved_prompts.len(), 1);
    button(&mut h, "Keep prompt");
    button(&mut h, "Delete prompt");
    button(&mut h, "Confirm Delete");
    assert!(h.state().settings.ai.saved_prompts.is_empty());
    assert!(
        thinoda_platform::ai_prompts::load(&paths)
            .unwrap()
            .is_empty()
    );
    assert!(server.requests.lock().unwrap().is_empty());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn prompt_conflict_keeps_editor_and_runtime_library_until_explicit_reload() {
    let (mut h, _, _) = harness("Not requested");
    let temp = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(temp.path());
    paths.ensure().unwrap();
    h.state_mut().services.paths = Some(paths.clone());
    h.state_mut().begin_prompt_editor(Some(saved_prompt()));
    let other = thinoda_platform::ai_prompts::SavedPrompt {
        name: "Other window".into(),
        ..saved_prompt()
    };
    thinoda_platform::ai_prompts::save(&paths, &[], std::slice::from_ref(&other)).unwrap();
    h.run_steps(3);
    button(&mut h, "Save prompt");
    assert!(h.state().ai.prompt_editor.is_some());
    assert!(h.state().settings.ai.saved_prompts.is_empty());
    assert_eq!(
        thinoda_platform::ai_prompts::load(&paths).unwrap(),
        vec![other.clone()]
    );
    button(&mut h, "Cancel");
    h.state_mut().reload_saved_prompts();
    assert_eq!(h.state().settings.ai.saved_prompts, vec![other]);
}

#[test]
fn saved_prompt_editor_ime_escape_does_not_dismiss_or_edit_map() {
    let (mut h, server, _) = harness("Not requested");
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.state_mut().begin_prompt_editor(None);
    h.run_steps(3);
    h.get_by_role_and_label(Role::TextInput, "Prompt name")
        .click();
    h.run_steps(2);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "候補".into(),
        active_range_chars: None,
    }));
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().ai.prompt_editor.is_some());
    assert!(h.state().settings.ai.saved_prompts.is_empty());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(server.requests.lock().unwrap().is_empty());
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: String::new(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    button(&mut h, "Cancel");
    assert!(h.state().ai.prompt_editor.is_none());
}

#[test]
fn prompt_save_failure_keeps_the_draft_and_short_window_controls_reachable() {
    let delta = r#"{"choices":[{"delta":{"content":"Not requested"}}]}"#;
    let server = thinoda_ai::mock::MockServer::start_sse(&["mock-chat"], &[delta]);
    let (mut h, server, _) = with_server(server, egui::vec2(360.0, 340.0));
    let temp = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(temp.path());
    // A file in place of the config directory makes persistence fail deterministically.
    std::fs::create_dir_all(paths.config_dir.parent().unwrap()).unwrap();
    std::fs::write(&paths.config_dir, "not a directory").unwrap();
    h.state_mut().services.paths = Some(paths);
    h.state_mut().begin_prompt_editor(Some(saved_prompt()));
    h.run_steps(3);
    button(&mut h, "Save prompt");
    assert!(h.state().ai.prompt_editor.is_some());
    assert!(h.state().settings.ai.saved_prompts.is_empty());
    assert_eq!(
        std::fs::read_to_string(&h.state().services.paths.as_ref().unwrap().config_dir).unwrap(),
        "not a directory"
    );
    button(&mut h, "Cancel");
    assert!(h.state().ai.prompt_editor.is_none());
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn canceled_touches_cannot_save_or_dismiss_a_prompt_draft() {
    for label in ["Save prompt", "Cancel"] {
        let (mut h, server, _) = harness("Not requested");
        h.state_mut().begin_prompt_editor(Some(saved_prompt()));
        h.run_steps(3);
        let pos = h.get_by_role_and_label(Role::Button, label).rect().center();
        h.event(egui::Event::PointerMoved(pos));
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.event(egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase: egui::TouchPhase::Start,
            pos,
            force: None,
        });
        h.run_steps(2);
        h.event(egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase: egui::TouchPhase::Cancel,
            pos,
            force: None,
        });
        h.run_steps(2);
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(3);
        assert!(h.state().ai.prompt_editor.is_some());
        assert!(h.state().settings.ai.saved_prompts.is_empty());
        assert!(server.requests.lock().unwrap().is_empty());
    }
}

#[test]
fn oversized_chat_reply_is_not_retained_as_a_completed_answer() {
    let chunks: Vec<String> = [MAX_REPLY_BYTES / 2, MAX_REPLY_BYTES / 2 + 1]
        .into_iter()
        .map(|bytes| {
            serde_json::json!({"choices":[{"delta":{"content":"x".repeat(bytes)}}]}).to_string()
        })
        .collect();
    let server = thinoda_ai::mock::MockServer::start_sse(
        &["mock-chat"],
        &chunks.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let (mut h, _server, key) = with_server(server, egui::vec2(1000.0, 720.0));
    h.state_mut()
        .ai
        .chat
        .threads
        .entry(key.clone())
        .or_default()
        .draft = "Keep draft".into();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Send").click();
    h.run_steps(1);
    wait(&mut h);
    let thread = &h.state().ai.chat.threads[&key];
    assert!(thread.messages.is_empty());
    assert_eq!(thread.draft, "Keep draft");
    assert_eq!(
        thread.error.as_deref(),
        Some(t("AI reply is too large. Try a shorter request."))
    );
}
