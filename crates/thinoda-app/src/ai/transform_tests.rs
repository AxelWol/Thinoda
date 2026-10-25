use super::*;
use crate::app::Cmd;
use egui::accesskit::Role;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

fn harness(reply: &str) -> (Harness<'static, ThinodaApp>, thinoda_ai::mock::MockServer) {
    let delta = serde_json::json!({"choices":[{"delta":{"content":reply}}]}).to_string();
    let server = thinoda_ai::mock::MockServer::start_sse(&["mock-chat"], &[&delta]);
    let mut app = ThinodaApp::headless();
    app.settings.ai.base_url = server.base_url.clone();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Notes fixture", |sheet| {
            sheet.set_notes(root, Some("Private source notes".into()))
        })
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 720.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(3);
    (h, server)
}

fn start(h: &mut Harness<'static, ThinodaApp>, action: AiCommand) {
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Ai(action));
    h.run_steps(3);
    h.state_mut().ai.prompt =
        Some("Use clear wording; translate into French when requested.".into());
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Send").click();
    h.run_steps(1);
}

fn wait(h: &mut Harness<'static, ThinodaApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.state().ai.pending.is_some() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run_steps(1);
    }
    h.run_steps(3);
}

#[test]
fn all_prompted_actions_wait_for_apply_and_undo_as_one_transaction() {
    for (action, reply) in [
        (AiCommand::Rewrite, "Clear central title"),
        (AiCommand::Grow, "- A fresh idea\n- Another idea"),
        (
            AiCommand::Translate,
            "# Plan traduit\n- Préparer\n  - Vérifier",
        ),
        (AiCommand::Reorganize, "# Clear plan\n- Prepare\n  - Check"),
        (AiCommand::Refine, "# Refined plan\n- Prepare\n  - Check"),
    ] {
        let (mut h, server) = harness(reply);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let key = h.state().doc().unwrap().key.clone();
        start(&mut h, action);
        wait(&mut h);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert!(h.state().ai.review.is_some());
        h.get_by_role_and_label(Role::Button, "Apply AI Result")
            .click();
        h.run_steps(3);
        let doc = h.state().doc().unwrap();
        assert_eq!(doc.key, key);
        match action {
            AiCommand::Rewrite => {
                assert_eq!(
                    doc.doc.sheet().topic(doc.doc.sheet().root).unwrap().title,
                    reply
                );
                assert_eq!(
                    doc.doc
                        .sheet()
                        .topic(doc.doc.sheet().root)
                        .unwrap()
                        .notes
                        .as_deref(),
                    Some("Private source notes")
                );
            }
            AiCommand::Grow => assert_eq!(
                doc.doc.sheet().topics.len(),
                before.sheets[0].topics.len() + 2
            ),
            _ => {
                assert_eq!(doc.doc.workbook().sheets.len(), before.sheets.len() + 1);
                assert_eq!(doc.doc.workbook().sheets[0], before.sheets[0]);
                assert!(
                    doc.doc
                        .sheet()
                        .topics
                        .values()
                        .all(|topic| topic.notes.is_none())
                );
            }
        }
        let requests = server.requests.lock().unwrap();
        let request = requests
            .iter()
            .find(|request| request.path.ends_with("/chat/completions"))
            .unwrap();
        assert!(!request.body.contains("Private source notes"));
        assert!(request.body.contains("Use clear wording"));
        drop(requests);
        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn prompted_actions_are_offline_until_send_and_stale_drafts_cannot_dispatch() {
    let (mut h, server) = harness("# Result\n- Item");
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Ai(AiCommand::Translate));
    h.run_steps(3);
    assert!(server.requests.lock().unwrap().is_empty());
    h.state_mut().ai.prompt = Some("French".into());
    let doc = h.state_mut().doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Changed source", |sheet| sheet.set_title(root, "Changed"))
        .unwrap();
    assert!(h.state_mut().start_prompted_ai("French").is_err());
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Send")
            .accesskit_node()
            .is_disabled()
    );
    assert_eq!(h.state().ai.prompt.as_deref(), Some("French"));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().ai.prompt.is_none());
    assert!(h.state().ai.prompted_target.is_none());
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn malformed_transforms_and_stale_reviews_cannot_mutate_sources() {
    for reply in ["Bad\nmultiline\ntitle", &"x".repeat(1025)] {
        let (mut h, _) = harness(reply);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        start(&mut h, AiCommand::Rewrite);
        wait(&mut h);
        assert!(h.state().ai.review.is_none());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
    let (mut h, _) = harness("# Translated\n- Child");
    start(&mut h, AiCommand::Translate);
    wait(&mut h);
    h.state_mut().settings.ai.model = "different-model".into();
    let review = h.state_mut().ai.review.take().unwrap();
    let before = h.state().doc().unwrap().doc.workbook().clone();
    assert!(
        h.state_mut()
            .apply_ai(&review.target, &review.result)
            .is_err()
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}
