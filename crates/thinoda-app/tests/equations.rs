use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    ThinodaApp,
    app::{CenterMode, Cmd},
    doc::OpenDoc,
    i18n::t,
};
use thinoda_canvas::Action;
use thinoda_core::equations;
use thinoda_platform::{
    Paths,
    recovery::RecoveryManager,
    settings::{Language, Settings, ThemeMode},
};

fn harness(mut app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    app.format_tab = thinoda_app::app::FormatTab::Data;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1100.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h
}

fn click(h: &mut Harness<'_, ThinodaApp>, label: &'static str) {
    h.get_by_role_and_label(Role::Button, t(label))
        .scroll_to_me();
    h.run_steps(8);
    h.get_by_role_and_label(Role::Button, t(label)).click();
    h.run_steps(4);
}

fn source(h: &mut Harness<'_, ThinodaApp>, source: &str) {
    h.get_by_role_and_label(Role::MultilineTextInput, t("Equation source"))
        .focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    h.get_by_role_and_label(Role::MultilineTextInput, t("Equation source"))
        .type_text(source);
    h.run_steps(3);
}

#[test]
fn equations_author_edit_remove_undo_redo_save_reopen_and_recovery_in_both_views() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        let before = app.doc().unwrap().doc.workbook().clone();
        let mut h = harness(app);
        click(&mut h, "Edit notes equation");
        source(&mut h, "\\frac{a}{b}+x_{i}^{2}");
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        click(&mut h, "Apply notes equation");
        let first = h
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .clone()
            .unwrap();
        assert!(first.is_markdown());
        assert_eq!(
            equations::split_note(&first).unwrap().unwrap().1,
            "\\frac{a}{b}+x_{i}^{2}"
        );
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Redo));
        h.run_steps(4);
        click(&mut h, "Edit notes equation");
        source(&mut h, "y^{3}");
        click(&mut h, "Apply notes equation");
        let second = h
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .clone();
        assert_ne!(second, Some(first.clone()));
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            Some(first)
        );
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Redo));
        h.run_steps(4);

        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(dir.path().join("profile"));
        let recovery = RecoveryManager::start(&paths).unwrap();
        h.state_mut().doc_mut().unwrap().snapshot(&recovery);
        recovery.suspend().unwrap();
        let recovered = RecoveryManager::find_orphans(&paths);
        assert_eq!(
            recovered[0].docs[0]
                .load()
                .unwrap()
                .active()
                .root_topic()
                .notes,
            second
        );
        drop(recovered);
        recovery.resume().unwrap();
        let path = dir.path().join("equation.thinoda");
        h.state_mut()
            .doc_mut()
            .unwrap()
            .save_to(&path, false)
            .unwrap();
        assert_eq!(
            OpenDoc::open(&path).unwrap().doc.sheet().root_topic().notes,
            second
        );
        recovery.finish().unwrap();

        click(&mut h, "Edit notes equation");
        click(&mut h, "Remove notes equation");
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            None
        );
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            second
        );
    }
}

#[test]
fn equations_invalid_draft_cancel_revision_and_selection_changes_never_replace_saved_source() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    let note = equations::replace_note("**Keep exactly**\r\n", Some("x^{2}")).unwrap();
    doc.doc
        .edit_sheet("Fixture", |sheet| {
            sheet.set_markdown_notes(root, Some(note))
        })
        .unwrap();
    let before = doc.doc.workbook().clone();
    let mut h = harness(app);
    click(&mut h, "Edit notes equation");
    source(&mut h, "\\input{secret}");
    assert!(h.query_by_label(t(equations::SYNTAX_ERROR)).is_some());
    assert!(
        h.get_by_role_and_label(Role::Button, t("Apply notes equation"))
            .accesskit_node()
            .is_disabled()
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    let oversized = "x".repeat(equations::MAX_SOURCE_BYTES + 1);
    source(&mut h, &oversized);
    assert!(h.query_by_label(t(equations::LIMIT_ERROR)).is_some());
    assert_eq!(
        h.get_by_role_and_label(Role::MultilineTextInput, t("Equation source"))
            .value()
            .as_deref(),
        Some(oversized.as_str()),
    );
    assert!(
        h.get_by_role_and_label(Role::Button, t("Apply notes equation"))
            .accesskit_node()
            .is_disabled()
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    click(&mut h, "Cancel equation edit");
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    click(&mut h, "Edit notes equation");
    source(&mut h, "z^{4}");
    h.key_press(Key::Escape);
    h.run_steps(4);
    assert!(
        h.query_by_role_and_label(Role::Button, t("Apply notes equation"))
            .is_none()
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    click(&mut h, "Edit notes equation");
    source(&mut h, "y^{3}");
    let ctx = h.ctx.clone();
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Revision", |sheet| {
            sheet.set_title(root, "revision changed")
        })
        .unwrap();
    h.run_steps(4);
    assert!(
        h.query_by_role_and_label(Role::Button, t("Apply notes equation"))
            .is_none()
    );
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().root_topic().notes,
        before.active().root_topic().notes
    );
    let child = {
        let doc = h.state_mut().doc_mut().unwrap();
        let mut child = root;
        doc.doc
            .edit_sheet("Child", |sheet| {
                child = sheet.add_child(root, None, "Other topic");
                Ok(())
            })
            .unwrap();
        child
    };
    h.run_steps(4);
    click(&mut h, "Edit notes equation");
    h.state_mut().doc_mut().unwrap().view.select_only(child);
    h.run_steps(4);
    assert!(
        h.query_by_role_and_label(Role::Button, t("Apply notes equation"))
            .is_none()
    );
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(root)
            .unwrap()
            .notes,
        before.active().root_topic().notes
    );
    click(&mut h, "Edit notes equation");
    h.state_mut().exec(&ctx, Cmd::New);
    h.run_steps(4);
    assert!(
        h.query_by_role_and_label(Role::Button, t("Apply notes equation"))
            .is_none()
    );
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().root_topic().notes,
        None
    );
}

#[test]
fn equations_plain_notes_are_not_reinterpreted_and_map_shot_is_blocked() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    let note = equations::replace_note("", Some("x")).unwrap();
    doc.doc
        .edit_sheet("Literal", |sheet| sheet.set_notes(root, Some(note.clone())))
        .unwrap();
    let before = doc.doc.workbook().clone();
    let mut h = harness(app);
    assert!(
        h.get_by_role_and_label(Role::Button, t("Edit notes equation"))
            .accesskit_node()
            .is_disabled()
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Explicit", |sheet| {
            sheet.set_markdown_notes(root, Some(note.clone()))
        })
        .unwrap();
    h.run_steps(4);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::MapShot);
    h.run_steps(4);
    assert!(h.query_by_label(t(equations::EXPORT_ERROR)).is_some());
    assert!(!h.output().viewport_output.values().any(|output| {
        output
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Screenshot(_)))
    }));
}

#[test]
fn equations_authoring_is_localized_and_theme_aware_in_all_ten_languages() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let mut app = ThinodaApp::new(
                Settings {
                    language,
                    theme,
                    ..Default::default()
                },
                Default::default(),
                &[],
            );
            app.exec(&egui::Context::default(), Cmd::New);
            let mut h = harness(app);
            click(&mut h, "Edit notes equation");
            assert!(
                h.query_by_role_and_label(Role::MultilineTextInput, t("Equation source"))
                    .is_some()
            );
            click(&mut h, "Apply notes equation");
            assert!(equations::sheet_has_equations(
                h.state().doc().unwrap().doc.sheet()
            ));
            let revision = h.state().doc().unwrap().doc.state_id();
            click(&mut h, "Edit notes equation");
            click(&mut h, "Cancel equation edit");
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
        }
    }
}

#[test]
fn equations_open_after_focused_note_edits_and_apply_as_a_separate_undo_step() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Fixture", |sheet| {
            sheet.set_markdown_notes(root, Some("**Original**".into()))
        })
        .unwrap();
    let mut h = harness(app);
    h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
        .focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
        .type_text("*Updated*");
    h.run_steps(3);
    click(&mut h, "Edit notes equation");
    source(&mut h, "\\frac{1}{2}");
    click(&mut h, "Apply notes equation");
    let note = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .root_topic()
        .notes
        .clone()
        .unwrap();
    assert_eq!(
        equations::split_note(&note).unwrap(),
        Some(("*Updated*", "\\frac{1}{2}"))
    );
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_deref(),
        Some("*Updated*")
    );
    h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_deref(),
        Some("**Original**")
    );
}
