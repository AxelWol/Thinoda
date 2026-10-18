//! Startup splash lifecycle: phases, readiness, errors and recovery prompts.
//! Language changes are global; keep the localized cases in one test.
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{Startup, ThinodaApp, app::Services, i18n::t, startup::LaunchPhase};
use thinoda_platform::{
    recovery::OrphanSession,
    settings::{Language, Settings, ThemeMode},
};

const OPML: &str = r#"<?xml version="1.0"?>
<opml version="2.0"><head><title>Splash</title></head>
<body><outline text="Launch root"/></body></opml>"#;

fn startup(language: Language, theme: ThemeMode, orphans: Vec<OrphanSession>) -> Startup {
    Startup {
        settings: Settings {
            language,
            theme,
            ..Default::default()
        },
        services: Services::default(),
        warning: Some("settings warning".into()),
        orphans,
    }
}

/// The UI language is process-global; tests that set or read it must not interleave.
static LANGUAGE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Holds the app back until `live`, so the harness' settle-on-build cannot skip the splash.
struct Gate {
    live: bool,
    app: ThinodaApp,
}

fn harness(app: ThinodaApp) -> Harness<'static, Gate> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
        })
        .build_ui_state(
            |ui, gate: &mut Gate| {
                if gate.live {
                    gate.app.draw(ui)
                }
            },
            Gate { live: false, app },
        );
    h.state_mut().live = true;
    h.step();
    h
}

/// Steps until startup is ready; panics instead of hanging if it never gets there.
fn step_until_ready(h: &mut Harness<'_, Gate>) -> Vec<LaunchPhase> {
    let mut phases = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while let Some(phase) = h.state().app.launch_phase() {
        if phases.last() != Some(&phase) {
            phases.push(phase);
        }
        assert!(Instant::now() < deadline, "startup never became ready");
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    phases
}

#[test]
fn launch_files_open_in_order_errors_surface_and_recovery_waits_for_ready() {
    let _language = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("launch.opml");
    std::fs::write(&good, OPML).unwrap();
    let missing = dir.path().join("missing.opml");
    let orphan = OrphanSession {
        session_id: "earlier-session".into(),
        docs: Vec::new(),
    };
    let app = ThinodaApp::launch(
        startup(Language::English, ThemeMode::Light, vec![orphan]),
        &[missing.clone(), good.clone(), good.clone()],
    );
    assert_eq!(app.launch_phase(), Some(LaunchPhase::Starting));
    assert!(app.docs.is_empty());
    // The first live frame must already be the splash.
    let mut h = harness(app);
    assert!(
        h.state().app.dialogs.recovery.is_empty(),
        "recovery behind splash"
    );
    assert!(
        h.state().app.toasts.is_empty(),
        "warnings wait for the main UI"
    );
    h.get_by_label(t("Starting…"));
    h.get_by_label("Thinoda");

    let phases = step_until_ready(&mut h);
    assert!(phases.contains(&LaunchPhase::Opening {
        index: 1,
        total: 2,
        name: "missing.opml".into(),
    }));
    assert!(
        phases
            .iter()
            .all(|p| !matches!(p, LaunchPhase::Opening { total, .. } if *total != 2)),
        "duplicate launch files are counted once: {phases:?}"
    );

    let app = &h.state().app;
    assert_eq!(app.docs.len(), 1, "the good file opened exactly once");
    assert_eq!(app.docs[0].doc.sheet().title, "Splash");
    assert!(app.docs[0].path.is_none(), "foreign imports stay unsaved");
    assert_eq!(app.dialogs.recovery.len(), 1, "recovery prompt after ready");
    let toasts: Vec<_> = app.toasts.texts().collect();
    assert!(
        toasts
            .iter()
            .any(|t| t.contains("Could not open") && t.contains("missing.opml")),
        "{toasts:?}"
    );
    assert!(toasts.contains(&"settings warning"), "{toasts:?}");
    h.run_steps(2);
    assert!(
        h.query_by_label(t("Starting…")).is_none(),
        "splash hidden once ready"
    );
}

#[test]
fn splash_follows_language_and_theme_and_needs_no_minimum_wait() {
    let _language = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    for (language, theme, dark) in [
        (Language::English, ThemeMode::Dark, true),
        (Language::German, ThemeMode::Light, false),
        (Language::German, ThemeMode::Dark, true),
    ] {
        let mut h = harness(ThinodaApp::launch(
            startup(language, theme, Vec::new()),
            &[],
        ));
        assert_eq!(h.ctx.global_style().visuals.dark_mode, dark);
        h.get_by_label(t("Starting…"));
        h.get_by_label(t("Give your thoughts shape."));
        h.get_by_role_and_label(Role::Image, t("Thinoda logo"));
        // Without launch files the second frame already presents the main UI.
        h.step();
        assert_eq!(h.state().app.launch_phase(), None);
    }
    thinoda_app::i18n::set_lang(thinoda_app::i18n::Lang::En);
}

#[test]
fn headless_and_direct_construction_are_ready_immediately() {
    assert_eq!(ThinodaApp::headless().launch_phase(), None);
    let app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    assert_eq!(app.launch_phase(), None);
}
