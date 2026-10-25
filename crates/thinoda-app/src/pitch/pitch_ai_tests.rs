use super::*;
use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};

const REPLY: &str =
    r#"{"slides":[{"candidate":2,"title":"Opening Zone"},{"candidate":0,"title":"Next Steps"}]}"#;

fn fixture(reply: &str) -> (ThinodaApp, thinoda_ai::mock::MockServer) {
    let delta = serde_json::json!({"choices":[{"delta":{"content":reply}}]}).to_string();
    let server = thinoda_ai::mock::MockServer::start_sse(&["mock-chat"], &[&delta]);
    let mut app = super::super::tests::setup(true);
    app.settings.ai.base_url = server.base_url.clone();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Private metadata", |sheet| {
            sheet.set_notes(root, Some("Private notes never transmitted".into()))
        })
        .unwrap();
    app.begin_presentation_editor(&Context::default());
    app.pitch.editor.as_mut().unwrap().ai.instruction =
        "Open with the Zone, then next steps.".into();
    (app, server)
}

fn start(app: &mut ThinodaApp) {
    let mut editor = app.pitch.editor.take().unwrap();
    app.start_pitch_ai(&mut editor).unwrap();
    app.pitch.editor = Some(editor);
}

fn wait(app: &mut ThinodaApp) {
    let ctx = Context::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.pitch.editor.as_ref().unwrap().ai.pending.is_some() {
        assert!(std::time::Instant::now() < deadline);
        app.poll_pitch_ai(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn pitch_suggestions_preserve_real_frames_and_wait_for_review_then_one_undoable_apply() {
    let (mut app, server) = fixture(REPLY);
    let before = app.doc().unwrap().doc.workbook().clone();
    let original = app
        .pitch
        .editor
        .as_ref()
        .unwrap()
        .sheet
        .presentation
        .clone();
    start(&mut app);
    wait(&mut app);
    assert!(app.pitch.editor.as_ref().unwrap().ai.proposal.is_some());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    assert_eq!(
        app.pitch.editor.as_ref().unwrap().sheet.presentation,
        original
    );
    let mut editor = app.pitch.editor.take().unwrap();
    app.use_pitch_ai(&mut editor).unwrap();
    assert_eq!(editor.sheet.presentation.slides.len(), 2);
    for (slide, index) in editor.sheet.presentation.slides.iter().zip([2, 0]) {
        assert_eq!(slide.id, original.slides[index].id);
        assert_eq!(slide.source, original.slides[index].source);
        assert_eq!(slide.frame, original.slides[index].frame);
    }
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.apply_presentation_draft(&editor).unwrap();
    let mut expected = before.clone();
    expected.sheets[0].presentation = editor.sheet.presentation;
    assert_eq!(app.doc().unwrap().doc.workbook(), &expected);
    assert!(app.doc_mut().unwrap().doc.undo());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    let requests = server.requests.lock().unwrap();
    let request = requests
        .iter()
        .find(|request| request.path.ends_with("/chat/completions"))
        .unwrap();
    let text = request.body.to_string();
    assert!(text.contains("First branch") && text.contains("Zone"));
    assert!(!text.contains("Private notes"));
    // The system prompt prohibits returning frame fields; no frame coordinates are sent.
    assert!(!text.contains("\"min\"") && !text.contains("\"size\""));
}

#[test]
fn pitch_rejects_unknown_candidates_duplicate_sources_and_malformed_or_oversized_titles() {
    let (app, _server) = fixture(REPLY);
    let candidates = &app.pitch.editor.as_ref().unwrap().sheet.presentation;
    for reply in [
        "{}",
        r#"{"slides":[]}"#,
        r#"{"slides":[{"candidate":99,"title":"Invented"}]}"#,
        r#"{"slides":[{"candidate":-1,"title":"Negative"}]}"#,
        r#"{"slides":[{"candidate":0.5,"title":"Fraction"}]}"#,
        r#"{"slides":[{"candidate":0,"title":"A"},{"candidate":0,"title":"B"}]}"#,
        r#"{"slides":[{"candidate":0,"title":" "}]} "#,
        r#"{"slides":[{"candidate":0,"title":"A\nB"}]}"#,
        r#"{"slides":[{"candidate":0,"title":"A","frame":{}}]}"#,
        r#"{"slides":[{"candidate":0,"title":"A"}],"source":"invented"}"#,
    ] {
        assert!(parse(reply, candidates).is_err(), "{reply}");
    }
    let response =
        |title: String| serde_json::json!({"slides":[{"candidate":0,"title":title}]}).to_string();
    assert!(parse(&response("x".repeat(256)), candidates).is_ok());
    assert!(parse(&response("x".repeat(257)), candidates).is_err());
    assert!(parse(&" ".repeat(MAX_BYTES + 1), candidates).is_err());
}

#[test]
fn pitch_cancel_draft_source_config_and_disable_changes_discard_late_requests() {
    for change in [
        "cancel",
        "draft",
        "instruction",
        "source",
        "config",
        "disable",
        "close",
    ] {
        let (mut app, _server) = fixture(REPLY);
        start(&mut app);
        match change {
            "cancel" => app.pitch.editor.as_mut().unwrap().ai.pending = None,
            "draft" => app.pitch.editor.as_mut().unwrap().generation += 1,
            "instruction" => app.pitch.editor.as_mut().unwrap().ai.instruction.push('!'),
            "source" => {
                let doc = app.doc_mut().unwrap();
                let root = doc.doc.sheet().root;
                doc.doc
                    .edit_sheet("Change", |sheet| sheet.set_title(root, "Changed"))
                    .unwrap();
            }
            "config" => app.settings.ai.model = "other-model".into(),
            "disable" => app.settings.ai.enabled = false,
            "close" => app.docs.clear(),
            _ => unreachable!(),
        }
        app.poll_pitch_ai(&Context::default());
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.poll_pitch_ai(&Context::default());
        let editor = app.pitch.editor.as_ref().unwrap();
        assert!(editor.ai.pending.is_none());
        assert!(editor.ai.proposal.is_none());
    }
    let (mut app, _server) = fixture(REPLY);
    start(&mut app);
    wait(&mut app);
    let mut editor = app.pitch.editor.take().unwrap();
    editor.generation += 1;
    assert!(app.use_pitch_ai(&mut editor).is_err());
}

#[test]
fn pitch_bounds_and_uncommitted_source_drafts_fail_before_dispatch() {
    let (mut app, server) = fixture(REPLY);
    let mut editor = app.pitch.editor.take().unwrap();
    editor.ai.instruction = "x".repeat(8193);
    assert!(app.start_pitch_ai(&mut editor).is_err());
    editor.ai.instruction = "Arrange".into();
    let original = editor.sheet.presentation.clone();
    editor.sheet.presentation.slides.clear();
    assert!(app.start_pitch_ai(&mut editor).is_err());
    editor.sheet.presentation = original.clone();
    for index in 3..65 {
        let mut slide = original.slides[0].clone();
        slide.id = thinoda_core::SlideId(index + 1);
        editor.sheet.presentation.slides.push(slide);
    }
    assert!(app.start_pitch_ai(&mut editor).is_err());
    editor.sheet.presentation = original;
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.view
        .begin_edit_with(doc.doc.sheet(), root, "Uncommitted");
    assert!(app.start_pitch_ai(&mut editor).is_err());
    assert!(app.apply_presentation_draft(&editor).is_err());
    assert!(server.requests.lock().unwrap().is_empty());

    let (mut app, _server) = fixture(REPLY);
    let mut editor = app.pitch.editor.take().unwrap();
    let original = editor.sheet.presentation.slides[0].clone();
    for index in 3..64 {
        let mut slide = original.clone();
        slide.id = thinoda_core::SlideId(index + 1);
        editor.sheet.presentation.slides.push(slide);
    }
    editor.ai.instruction = "x".repeat(8192);
    app.start_pitch_ai(&mut editor).unwrap();
    app.pitch.editor = Some(editor);
    wait(&mut app);
    assert!(app.pitch.editor.as_ref().unwrap().ai.proposal.is_some());
}

#[test]
fn pitch_malformed_and_oversized_terminal_results_report_errors_without_changing_drafts() {
    for reply in ["not an outline".to_string(), "x".repeat(MAX_BYTES + 1)] {
        let (mut app, _server) = fixture(&reply);
        let before = app.doc().unwrap().doc.workbook().clone();
        start(&mut app);
        wait(&mut app);
        let editor = app.pitch.editor.as_ref().unwrap();
        assert!(editor.ai.proposal.is_none());
        assert!(editor.error.is_some());
        assert_eq!(editor.sheet.presentation, before.sheets[0].presentation);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn pitch_ai_ui_is_offline_until_suggest_and_cancel_keeps_the_saved_presentation() {
    for os in [
        egui::os::OperatingSystem::Windows,
        egui::os::OperatingSystem::Mac,
        egui::os::OperatingSystem::Nix,
    ] {
        let (app, server) = fixture(REPLY);
        let before = app.doc().unwrap().doc.workbook().clone();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 1400.0))
            .with_context_setup(move |ctx| ctx.set_os(os))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.run_steps(3);
        h.get_by_label("AI Pitch outline").scroll_to_me();
        h.run_steps(3);
        h.get_by_label("AI Pitch outline").click();
        h.run_steps(20);
        assert!(server.requests.lock().unwrap().is_empty());
        h.get_by_role_and_label(Role::Button, "Suggest Pitch outline")
            .scroll_to_me();
        h.run_steps(20);
        assert!(
            h.ctx.content_rect().contains_rect(
                h.get_by_role_and_label(Role::Button, "Suggest Pitch outline")
                    .rect()
            )
        );
        h.get_by_role_and_label(Role::Button, "Suggest Pitch outline")
            .click();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            h.run_steps(3);
            let editor = h
                .state()
                .pitch
                .editor
                .as_ref()
                .expect("Suggest must keep the presentation draft open");
            assert!(editor.error.is_none(), "{:?}", editor.error);
            if editor.ai.proposal.is_some() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Suggest did not produce a reviewed proposal; pending={}, requests={:?}",
                editor.ai.pending.is_some(),
                server.requests.lock().unwrap()
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(
            server
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|request| request.path == "/v1/chat/completions")
                .count(),
            1
        );
        h.run_steps(20);
        h.get_by_role_and_label(Role::Button, "Cancel")
            .scroll_to_me();
        h.run_steps(20);
        h.get_by_role_and_label(Role::Button, "Cancel").click();
        h.run_steps(3);
        assert!(h.state().pitch.editor.is_none());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}
