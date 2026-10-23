use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{CenterMode, Cmd, ThinodaApp};

fn draft(
    mode: CenterMode,
    field: Role,
    label: &str,
    text: &str,
    path: &std::path::Path,
) -> Harness<'static, ThinodaApp> {
    let mut app = ThinodaApp::headless();
    app.mode = mode;
    app.doc_mut().unwrap().save_to(path, false).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.get_by_role_and_label(field, label).focus();
    h.run_steps(4);
    h.get_by_role_and_label(field, label).type_text(text);
    h.run_steps(4);
    assert!(
        h.state().doc().unwrap().is_dirty(),
        "Focused draft must be unsaved work"
    );
    assert!(
        !h.state().doc().unwrap().doc.is_dirty(),
        "Typing remains a draft until commit"
    );
    h
}

#[test]
fn save_includes_focused_notes_without_requiring_focus_loss() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.thinoda");
        let mut h = draft(
            mode,
            Role::MultilineTextInput,
            "Notes",
            "Keep these unsaved notes",
            &path,
        );
        let before = h.state().doc().unwrap().doc.workbook().clone();
        for (modifiers, key) in [
            (Modifiers::COMMAND, Key::B),
            (Modifiers::COMMAND | Modifiers::SHIFT, Key::B),
            (Modifiers::COMMAND, Key::Num1),
        ] {
            h.key_press_modifiers(modifiers, key);
            h.run_steps(3);
            assert_eq!(
                h.state().doc().unwrap().doc.workbook(),
                &before,
                "Inspector text focus must own insertion/priority keys"
            );
        }
        h.key_press_modifiers(Modifiers::COMMAND, Key::S);
        h.run_steps(4);
        let saved = thinoda_app::doc::OpenDoc::open(&path).unwrap();
        assert_eq!(
            saved.doc.sheet().root_topic().notes.as_deref(),
            Some("Keep these unsaved notes"),
            "{mode:?}: Save must include the focused inspector draft"
        );
        assert!(!h.state().doc().unwrap().is_dirty());
        assert!(
            h.get_by_role_and_label(Role::MultilineTextInput, "Notes")
                .is_focused()
        );
        let ctx = h.ctx.clone();
        h.state_mut()
            .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
        h.run_steps(4);
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            None
        );
        h.key_press_modifiers(Modifiers::COMMAND, Key::S);
        h.run_steps(4);
        assert_eq!(
            thinoda_app::doc::OpenDoc::open(&path)
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes,
            None,
            "Saving after Undo must not resurrect the stale focused text buffer"
        );
        h.state_mut()
            .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Redo));
        h.run_steps(4);
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("Keep these unsaved notes")
        );
        h.get_by_label("Undo").click();
        h.run_steps(4);
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            None,
            "Toolbar Undo must not recommit the focused buffer on focus loss"
        );
        h.get_by_label("Redo").click();
        h.run_steps(4);
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("Keep these unsaved notes")
        );
    }
}

#[test]
fn toolbar_undo_redo_accept_keyboard_activation_after_a_focused_save() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for key in [Key::Enter, Key::Space] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("keyboard-toolbar.thinoda");
            let mut h = draft(
                mode,
                Role::MultilineTextInput,
                "Notes",
                "Keyboard-only notes",
                &path,
            );
            h.key_press_modifiers(Modifiers::COMMAND, Key::S);
            h.run_steps(4);
            h.get_by_role_and_label(Role::Button, "Undo").focus();
            h.run_steps(2);
            h.key_press(key);
            h.run_steps(3);
            assert_eq!(
                h.state().doc().unwrap().doc.sheet().root_topic().notes,
                None
            );
            h.get_by_role_and_label(Role::Button, "Redo").focus();
            h.run_steps(2);
            h.key_press(key);
            h.run_steps(3);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .root_topic()
                    .notes
                    .as_deref(),
                Some("Keyboard-only notes")
            );
        }
    }
}

#[test]
fn undoing_an_uncommitted_draft_does_not_restore_it_on_the_next_frame() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for toolbar in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("undo-draft.thinoda");
            let mut h = draft(
                mode,
                Role::MultilineTextInput,
                "Notes",
                "Undo this draft",
                &path,
            );
            if toolbar {
                h.get_by_label("Undo").click();
            } else {
                let ctx = h.ctx.clone();
                h.state_mut()
                    .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
            }
            h.run_steps(4);
            h.key_press_modifiers(Modifiers::COMMAND, Key::S);
            h.run_steps(4);
            assert_eq!(
                thinoda_app::doc::OpenDoc::open(&path)
                    .unwrap()
                    .doc
                    .sheet()
                    .root_topic()
                    .notes,
                None
            );
        }
    }
}

#[test]
fn hyperlink_save_and_escape_keep_their_existing_commit_cancel_semantics() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for cancel in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("link.thinoda");
            let mut h = draft(
                mode,
                Role::TextInput,
                "Hyperlink",
                "https://example.org/notes",
                &path,
            );
            if cancel {
                h.key_press(Key::Escape);
                h.run_steps(4);
                assert!(!h.state().doc().unwrap().is_dirty());
            }
            h.key_press_modifiers(Modifiers::COMMAND, Key::S);
            h.run_steps(4);
            let saved = thinoda_app::doc::OpenDoc::open(&path).unwrap();
            assert_eq!(
                saved.doc.sheet().root_topic().hyperlink.as_deref(),
                if cancel {
                    None
                } else {
                    Some("https://example.org/notes")
                }
            );
        }
    }
}

#[test]
fn native_close_and_recovery_include_drafts_without_focus_loss() {
    use thinoda_platform::{Paths, recovery::RecoveryManager};
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.thinoda");
        let mut h = draft(
            mode,
            Role::MultilineTextInput,
            "Notes",
            "Recover this draft",
            &path,
        );
        let paths = Paths::from_root(dir.path().join("profile"));
        let recovery = RecoveryManager::start(&paths).unwrap();
        h.state_mut().doc_mut().unwrap().snapshot(&recovery);
        recovery.flush().unwrap();
        recovery.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        let recovered = orphans[0].docs[0].load().unwrap();
        assert_eq!(
            recovered.active().root_topic().notes.as_deref(),
            Some("Recover this draft")
        );
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            None,
            "Recovery must not commit or alter undo history"
        );
        drop(orphans);
        recovery.resume().unwrap();
        h.get_by_role_and_label(Role::MultilineTextInput, "Notes")
            .type_text(" updated");
        h.run_steps(4);
        h.state_mut().doc_mut().unwrap().snapshot(&recovery);
        recovery.flush().unwrap();
        recovery.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert_eq!(
            orphans[0].docs[0]
                .load()
                .unwrap()
                .active()
                .root_topic()
                .notes
                .as_deref(),
            Some("Recover this draft updated"),
            "A second draft revision must replace recovery even without a model revision"
        );
        drop(orphans);
        recovery.resume().unwrap();
        recovery.finish().unwrap();
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(input, |ui| h.state_mut().handle_close_request(ui.ctx()));
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::CancelClose)
        );
        output.textures_delta.clear();
        assert!(h.state().dialogs.close.is_some());
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("Recover this draft updated")
        );
        let original = thinoda_app::doc::OpenDoc::open(&path).unwrap();
        assert_eq!(original.doc.sheet().root_topic().notes, None);
        h.run_steps(4);
        h.get_by_role_and_label(Role::Button, "Cancel").click();
        h.run_steps(4);
        assert!(h.state().dialogs.close.is_none());
        assert!(h.state().doc().unwrap().is_dirty());
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("Recover this draft updated")
        );
    }
}

#[test]
fn switching_identical_documents_never_retargets_or_loses_an_inspector_draft() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("first.thinoda");
        let second_path = dir.path().join("second.thinoda");
        let mut h = draft(
            mode,
            Role::MultilineTextInput,
            "Notes",
            "Only in the first map",
            &path,
        );
        let book = h.state().doc().unwrap().doc.workbook().clone();
        h.state_mut().new_doc(book);
        h.state_mut()
            .doc_mut()
            .unwrap()
            .save_to(&second_path, false)
            .unwrap();
        h.run_steps(5);
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            None
        );
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::SaveAll);
        h.run_steps(5);
        assert_eq!(
            thinoda_app::doc::OpenDoc::open(&path)
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("Only in the first map")
        );
        assert_eq!(
            thinoda_app::doc::OpenDoc::open(&second_path)
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes,
            None
        );
    }
}
