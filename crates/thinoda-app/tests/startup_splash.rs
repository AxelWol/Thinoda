//! Startup splash lifecycle: phases, readiness, errors and recovery prompts.
//! Language changes are global; keep the localized cases in one test.
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    Startup, ThinodaApp,
    app::{CloseIntent, Services},
    i18n::t,
    startup::LaunchPhase,
};
use thinoda_platform::{
    Paths,
    recovery::{OrphanSession, RecoveryManager, SnapshotMeta},
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
    harness_with_theme(app, egui::vec2(1100.0, 760.0), egui::Theme::Dark)
}

fn harness_with_theme(
    app: ThinodaApp,
    size: egui::Vec2,
    system_theme: egui::Theme,
) -> Harness<'static, Gate> {
    let mut h = gated_harness(app, size);
    h.input_mut().system_theme = Some(system_theme);
    h.state_mut().live = true;
    h.step();
    h
}

fn gated_harness(app: ThinodaApp, size: egui::Vec2) -> Harness<'static, Gate> {
    Harness::builder()
        .with_size(size)
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
        )
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
    for language in Language::ALL {
        for (theme, system_theme, expected) in [
            (ThemeMode::Light, egui::Theme::Dark, egui::Theme::Light),
            (ThemeMode::Dark, egui::Theme::Light, egui::Theme::Dark),
            (ThemeMode::System, egui::Theme::Light, egui::Theme::Light),
            (ThemeMode::System, egui::Theme::Dark, egui::Theme::Dark),
        ] {
            for (width, scale) in [(640.0, 1.0), (1280.0, 1.5)] {
                let mut launch = startup(language, theme, Vec::new());
                launch.settings.ui_scale = scale;
                let mut h = harness_with_theme(
                    ThinodaApp::launch(launch, &[]),
                    egui::vec2(width, 800.0),
                    system_theme,
                );
                assert_eq!(h.ctx.theme(), expected, "{language:?}/{theme:?}");
                h.get_by_label(t("Starting…"));
                h.get_by_label(t("Give your thoughts shape."));
                let logo = h.get_by_role_and_label(Role::Image, t("Thinoda logo"));
                assert!(
                    h.ctx.content_rect().contains_rect(logo.rect()),
                    "{language:?}/{theme:?}/{width}/{scale}: splash logo out of view"
                );
                // Without launch files the second frame already presents the main UI.
                h.step();
                assert_eq!(h.state().app.launch_phase(), None);
                assert_eq!(h.ctx.theme(), expected);
                assert_eq!(h.state().app.settings.language, language);
                assert_eq!(h.state().app.settings.theme, theme);
            }
        }
    }
    thinoda_app::i18n::set_lang(thinoda_app::i18n::Lang::En);
}

#[test]
fn headless_and_direct_construction_are_ready_immediately() {
    // Default settings resolve Auto to the OS locale, which changes the global language.
    let _language = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(ThinodaApp::headless().launch_phase(), None);
    let app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    assert_eq!(app.launch_phase(), None);
    thinoda_app::i18n::set_lang(thinoda_app::i18n::Lang::En);
}

#[test]
fn closing_first_splash_preserves_orphan_snapshots_and_unopened_sources() {
    let _language = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(dir.path());
        paths.ensure().unwrap();
        let workbook = thinoda_core::Workbook::default();
        let crashed = RecoveryManager::start(&paths).unwrap();
        crashed
            .snapshot(
                "unsaved-map",
                SnapshotMeta {
                    title: "Earlier unsaved work".into(),
                    dirty: true,
                    ..Default::default()
                },
                &workbook,
            )
            .unwrap();
        let snapshot = crashed.session_dir().join("docs").join("unsaved-map.json");
        let snapshot_bytes = std::fs::read(&snapshot).unwrap();
        assert!(crashed.try_flush_for_crash());
        drop(crashed);
        let orphans = RecoveryManager::find_orphans(&paths);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].docs.len(), 1);

        let source = dir.path().join("launch.opml");
        std::fs::write(&source, OPML).unwrap();
        let recovery = std::sync::Arc::new(RecoveryManager::start(&paths).unwrap());
        let session = recovery.session_dir().to_path_buf();
        let mut launch = startup(Language::English, theme, orphans);
        launch.services = Services {
            paths: Some(paths.clone()),
            recovery: Some(recovery.clone()),
            ..Services::default()
        };
        let mut h = gated_harness(
            ThinodaApp::launch(launch, std::slice::from_ref(&source)),
            egui::vec2(640.0, 800.0),
        );
        h.input_mut()
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        h.state_mut().live = true;
        h.step();

        assert!(h.state().app.launch_phase().is_some());
        assert!(h.state().app.docs.is_empty(), "no launch result consumed");
        assert!(h.state().app.dialogs.close.is_none());
        assert!(h.state().app.dialogs.recovery.is_empty());
        assert!(h.state().app.toasts.is_empty());
        assert!(h.state().app.services.recovery.is_none());
        assert!(!session.exists(), "empty current session closes cleanly");
        assert!(
            !h.output()
                .viewport_output
                .values()
                .flat_map(|output| &output.commands)
                .any(|command| *command == egui::ViewportCommand::CancelClose),
            "closing before any document opens must not be cancelled"
        );
        drop(h);
        drop(recovery);

        assert_eq!(std::fs::read(&snapshot).unwrap(), snapshot_bytes);
        assert_eq!(std::fs::read_to_string(&source).unwrap(), OPML);
        let recovered = RecoveryManager::find_orphans(&paths);
        assert_eq!(recovered.len(), 1, "prior work is still recoverable");
        assert_eq!(recovered[0].docs.len(), 1);
        assert_eq!(recovered[0].docs[0].load().unwrap(), workbook);
    }
}

#[test]
fn closing_during_import_keeps_unsaved_documents_and_cancel_returns_to_the_map() {
    let _language = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let sources: Vec<_> = (0..3)
        .map(|index| {
            let path = dir.path().join(format!("launch-{index}.opml"));
            std::fs::write(&path, OPML).unwrap();
            path
        })
        .collect();
    let mut h = harness(ThinodaApp::launch(
        startup(Language::English, ThemeMode::Light, Vec::new()),
        &sources,
    ));
    let deadline = Instant::now() + Duration::from_secs(20);
    while h.state().app.docs.is_empty() {
        assert!(Instant::now() < deadline, "first launch file never opened");
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(h.state().app.docs.len(), 1);
    assert!(h.state().app.docs[0].is_dirty());
    h.input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    h.step();
    assert!(h.state().app.launch_phase().is_some());
    assert_eq!(h.state().app.dialogs.close, Some(CloseIntent::Quit));
    assert!(
        h.output()
            .viewport_output
            .values()
            .flat_map(|output| &output.commands)
            .any(|command| *command == egui::ViewportCommand::CancelClose),
        "close must be cancelled once an unsaved import exists"
    );
    step_until_ready(&mut h);
    assert_eq!(h.state().app.docs.len(), sources.len());
    assert!(h.state().app.docs.iter().all(|doc| doc.is_dirty()));
    let workbooks: Vec<_> = h
        .state()
        .app
        .docs
        .iter()
        .map(|doc| doc.doc.workbook().clone())
        .collect();
    h.get_by_role_and_label(Role::Button, t("Cancel")).click();
    h.run_steps(3);
    assert!(h.state().app.dialogs.close.is_none());
    assert!(h.state().app.doc().is_some());
    for (doc, expected) in h.state().app.docs.iter().zip(&workbooks) {
        assert_eq!(doc.doc.workbook(), expected);
        assert!(doc.is_dirty());
    }
    for source in sources {
        assert_eq!(std::fs::read_to_string(source).unwrap(), OPML);
    }
}
