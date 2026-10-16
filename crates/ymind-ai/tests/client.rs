//! Client tests against the built-in mock server, plus an opt-out live test against a
//! running LM Studio (skipped automatically when no server is reachable).

use ymind_ai::mock::MockServer;
use ymind_ai::{AiConfig, AiError, AiJob, AiOutput, AiTask, Client, OutputKind, TopicContext};

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

/// Live endpoints tried in order: `YMIND_AI_TEST_URL`, the CUDA LM Studio hosts, local LM Studio.
fn live_client() -> Option<Client> {
    let mut urls: Vec<String> = std::env::var("YMIND_AI_TEST_URL")
        .ok()
        .into_iter()
        .collect();
    urls.push("http://192.168.1.216:1234/v1".into());
    urls.push("http://172.27.128.1:1234/v1".into());
    urls.push(ymind_ai::DEFAULT_BASE_URL.into());
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
                model: std::env::var("YMIND_AI_TEST_MODEL").unwrap_or_default(),
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
    if std::env::var("YMIND_AI_SKIP_LIVE").is_ok() {
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
