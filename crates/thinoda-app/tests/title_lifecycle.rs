use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{CenterMode, CloseIntent, Cmd, ThinodaApp};
use thinoda_app::doc::OpenDoc;
use thinoda_canvas::Action;

fn saved_app(directory: &Path, mode: CenterMode) -> ThinodaApp {
    let mut app = ThinodaApp::headless();
    app.settings.backup_on_save = false;
    for index in 0..2 {
        if index > 0 {
            app.exec(&egui::Context::default(), Cmd::New);
        }
        app.doc_mut()
            .unwrap()
            .save_to(&directory.join(format!("map-{index}.thinoda")), false)
            .unwrap();
    }
    app.active = 0;
    app.mode = mode;
    app
}

fn draft(app: &mut ThinodaApp, index: usize, text: &str) {
    let doc = &mut app.docs[index];
    let root = doc.doc.sheet().root;
    doc.view.begin_edit(doc.doc.sheet(), root, false);
    doc.view.set_editing_text(text);
    if doc.doc.sheet().topic(root).unwrap().title != text {
        assert!(
            doc.is_dirty(),
            "uncommitted title drafts must participate in close protection"
        );
    }
}

fn title(doc: &OpenDoc) -> &str {
    doc.doc
        .sheet()
        .topic(doc.doc.sheet().root)
        .unwrap()
        .title
        .as_str()
}

#[test]
fn rejected_title_commit_retains_draft_and_blocks_saving() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = saved_app(dir.path(), CenterMode::Map);
    draft(&mut app, 0, "My pending title");
    let doc = &mut app.docs[0];
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Concurrent edit", |sheet| {
            sheet.set_title(root, "Updated elsewhere")
        })
        .unwrap();
    assert!(!doc.view.commit_edit(&mut doc.doc));
    assert_eq!(doc.view.editing_text(), Some("My pending title"));
    assert!(
        doc.view
            .take_title_error()
            .unwrap()
            .contains("draft retained")
    );
    assert!(
        doc.save_to(&dir.path().join("blocked.thinoda"), false)
            .is_err()
    );
    assert!(!dir.path().join("blocked.thinoda").exists());
    assert_eq!(title(doc), "Updated elsewhere");
    doc.view.cancel_edit();
    doc.save_to(&dir.path().join("resolved.thinoda"), false)
        .unwrap();
}

fn close_request(app: &mut ThinodaApp, close: bool) -> egui::FullOutput {
    let ctx = egui::Context::default();
    let mut input = egui::RawInput::default();
    if close {
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
    }
    let mut output = ctx.run_ui(input, |ui| app.handle_close_request(ui.ctx()));
    output.textures_delta.clear();
    output
}

fn cancels_close(output: &egui::FullOutput) -> bool {
    output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::CancelClose)
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h
}

fn choose(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    let dialog = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_close")))
        .unwrap();
    h.query_all_by_role_and_label(Role::Button, label)
        .find(|button| dialog.contains_rect(button.rect()))
        .expect("save-changes dialog choice")
        .click();
    h.run_steps(4);
}

#[test]
fn sheet_rename_keeps_ime_candidates_in_the_editor() {
    use egui::{Event, ImeEvent, Key, Modifiers};
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for key in [Key::Enter, Key::Tab, Key::Escape] {
            for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                let dir = tempfile::tempdir().unwrap();
                let mut h = harness(saved_app(dir.path(), mode));
                let before = h.state().doc().unwrap().doc.workbook().clone();
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::RenameSheet);
                h.run_steps(3);
                h.key_press_modifiers(Modifiers::COMMAND, Key::A);
                h.run_steps(1);
                h.event(Event::Ime(ImeEvent::Preedit {
                    text: "nihon".into(),
                    active_range_chars: Some(0..5),
                }));
                h.run_steps(3);
                h.key_press_modifiers(modifiers, key);
                h.run_steps(3);
                assert!(
                    h.state().dialogs.rename_sheet.is_some(),
                    "{mode:?} {modifiers:?}+{key:?}"
                );
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                h.input_mut().events.extend([
                    Event::Ime(ImeEvent::Commit("日本".into())),
                    Event::Key {
                        key: Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                ]);
                h.run_steps(3);
                assert!(h.state().dialogs.rename_sheet.is_some());
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert!(
                    h.get_by_role_and_label(Role::TextInput, "Rename Sheet")
                        .is_focused(),
                    "{mode:?} {modifiers:?}+{key:?} lost input focus"
                );
                h.key_press(Key::Enter);
                h.run_steps(3);
                assert!(
                    h.state().dialogs.rename_sheet.is_none(),
                    "{mode:?} {modifiers:?}+{key:?} blocked final Enter"
                );
                assert_eq!(h.state().doc().unwrap().doc.sheet().title, "日本");
                h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            }
        }
    }
}

#[test]
fn sheet_rename_restores_navigation_and_cancellation_after_ime_dismissal() {
    use egui::{Event, ImeEvent, Key, Modifiers};
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for cancellation in [
            ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            },
            ImeEvent::Commit(String::new()),
        ] {
            for tab_to_cancel in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let mut h = harness(saved_app(dir.path(), mode));
                let before = h.state().doc().unwrap().doc.workbook().clone();
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::RenameSheet);
                h.run_steps(3);
                h.get_by_role_and_label(Role::TextInput, "Rename Sheet")
                    .type_text(" Draft");
                h.run_steps(2);
                h.event(Event::Ime(ImeEvent::Preedit {
                    text: "candidate".into(),
                    active_range_chars: None,
                }));
                h.run_steps(3);
                h.input_mut().events.extend([
                    Event::Ime(cancellation.clone()),
                    Event::Key {
                        key: Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                ]);
                h.run_steps(3);
                assert!(h.state().dialogs.rename_sheet.is_some());
                if tab_to_cancel {
                    h.key_press(Key::Tab);
                    h.run_steps(3);
                    assert!(h.get_by_role_and_label(Role::Button, "OK").is_focused());
                    h.key_press(Key::Tab);
                    h.run_steps(3);
                    assert!(h.get_by_role_and_label(Role::Button, "Cancel").is_focused());
                    h.key_press(Key::Enter);
                } else {
                    h.key_press(Key::Escape);
                }
                h.run_steps(3);
                assert!(
                    h.state().dialogs.rename_sheet.is_none(),
                    "{mode:?} {cancellation:?} tab_to_cancel={tab_to_cancel}"
                );
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            }
        }
    }
}

#[test]
fn ime_candidate_confirmation_keeps_both_title_editors_open() {
    use egui::{Event, ImeEvent, Key, Modifiers};
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for key in [Key::Enter, Key::Tab, Key::Escape] {
            for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                let dir = tempfile::tempdir().unwrap();
                let mut app = saved_app(dir.path(), mode);
                let doc = app.doc_mut().unwrap();
                let root = doc.doc.sheet().root;
                let before = doc.doc.sheet().clone();
                doc.view.begin_edit(doc.doc.sheet(), root, true);
                doc.view.set_editing_text("");
                let mut h = harness(app);
                h.event(Event::Ime(ImeEvent::Preedit {
                    text: "nihon".into(),
                    active_range_chars: Some(0..5),
                }));
                h.run_steps(3);
                h.key_press_modifiers(modifiers, key);
                h.run_steps(3);
                assert_eq!(
                    h.state().doc().unwrap().view.editing_topic(),
                    Some(root),
                    "{mode:?}: {modifiers:?}+{key:?} must stay in composition"
                );
                assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
                h.input_mut().events.extend([
                    Event::Ime(ImeEvent::Commit("日本".into())),
                    Event::Key {
                        key: Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                ]);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().view.editing_text(), Some("日本"));
                assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
                h.key_press(Key::Escape);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);
                assert_eq!(title(h.state().doc().unwrap()), "日本");
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
            }
        }
    }
}

#[test]
fn dismissing_ime_restores_normal_title_tab_behavior() {
    use egui::{Event, ImeEvent, Key, Modifiers};
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for cancellation in [
            ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            },
            ImeEvent::Commit(String::new()),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut app = saved_app(dir.path(), mode);
            draft(&mut app, 0, "Keep");
            let root = app.doc().unwrap().doc.sheet().root;
            let child_count = app.doc().unwrap().doc.sheet().children(root).len();
            let mut h = harness(app);
            h.event(Event::Ime(ImeEvent::Preedit {
                text: "candidate".into(),
                active_range_chars: None,
            }));
            h.run_steps(3);
            h.input_mut().events.extend([
                Event::Ime(cancellation),
                Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ]);
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(root));
            assert_eq!(h.state().doc().unwrap().view.editing_text(), Some("Keep"));
            h.key_press(Key::Tab);
            h.run_steps(3);
            let doc = h.state().doc().unwrap();
            assert_eq!(title(doc), "Keep");
            assert_eq!(doc.doc.sheet().children(root).len(), child_count + 1);
            assert_ne!(doc.view.editing_topic(), Some(root));
        }
    }
}

#[test]
fn title_only_edits_are_saved_before_dirty_checks_without_losing_home_or_undo() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for home in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut app = saved_app(dir.path(), mode);
            let original: Vec<_> = app.docs.iter().map(|doc| title(doc).to_owned()).collect();
            draft(&mut app, 0, "First title-only edit");
            draft(&mut app, 1, "Background title-only edit");
            if home {
                app.exec(&egui::Context::default(), Cmd::Home);
            }
            let active = app.active;
            app.exec(&egui::Context::default(), Cmd::SaveAll);
            assert_eq!(app.active, active);
            assert!(!app.any_dirty());
            assert_eq!(title(&app.docs[0]), "First title-only edit");
            assert_eq!(title(&app.docs[1]), "Background title-only edit");
            for (index, before) in original.iter().enumerate() {
                let doc = &app.docs[index];
                assert!(doc.view.editing_topic().is_none());
                let saved = OpenDoc::open(doc.path.as_ref().unwrap()).unwrap();
                assert_eq!(saved.doc.workbook(), doc.doc.workbook());
                app.active = index;
                app.exec(&egui::Context::default(), Cmd::Canvas(Action::Undo));
                assert_eq!(title(&app.docs[index]), before);
                assert!(!app.docs[index].doc.can_undo(), "one title, one undo");
                app.exec(&egui::Context::default(), Cmd::Canvas(Action::Redo));
                assert!(!app.docs[index].is_dirty());
            }
        }
    }
}

#[test]
fn closing_a_background_tab_commits_only_its_title_and_honors_each_choice() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for choice in ["Cancel", "Save", "Don't Save"] {
            let dir = tempfile::tempdir().unwrap();
            let mut app = saved_app(dir.path(), mode);
            let before = title(&app.docs[0]).to_owned();
            let target = app.docs[0].path.clone().unwrap();
            let other = app.docs[1].path.clone().unwrap();
            draft(&mut app, 0, "Closing background title");
            draft(&mut app, 1, "Keep the other draft");
            app.active = 1;
            app.request_close_tab(0);
            assert_eq!(
                app.dialogs.close,
                Some(CloseIntent::Tab(app.docs[0].key.clone()))
            );
            assert_eq!(title(&app.docs[0]), "Closing background title");
            assert_eq!(
                app.docs[1].view.editing_text(),
                Some("Keep the other draft")
            );
            assert!(app.docs[1].is_dirty());
            let mut h = harness(app);
            choose(&mut h, choice);
            assert!(h.state().dialogs.close.is_none());
            let saved = OpenDoc::open(&target).unwrap();
            assert_eq!(
                title(&saved),
                if choice == "Save" {
                    "Closing background title"
                } else {
                    &before
                }
            );
            if choice == "Cancel" {
                assert_eq!(h.state().docs.len(), 2);
                assert_eq!(title(&h.state().docs[0]), "Closing background title");
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
                assert_eq!(title(&h.state().docs[0]), before);
            } else {
                assert_eq!(h.state().docs.len(), 1);
            }
            let other = h
                .state()
                .docs
                .iter()
                .find(|doc| doc.path.as_ref() == Some(&other))
                .unwrap();
            assert_eq!(other.view.editing_text(), Some("Keep the other draft"));
            assert!(other.is_dirty());
        }
    }
}

#[test]
fn native_close_commits_all_titles_but_non_close_frames_do_not() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for choice in ["Cancel", "Save", "Don't Save"] {
            let dir = tempfile::tempdir().unwrap();
            let mut app = saved_app(dir.path(), mode);
            let original: Vec<_> = app.docs.iter().map(|doc| title(doc).to_owned()).collect();
            draft(&mut app, 0, "First window-close title");
            draft(&mut app, 1, "Second window-close title");
            app.request_close_tab(usize::MAX);
            assert_eq!(app.active, 0);
            assert!(app.dialogs.close.is_none());
            let output = close_request(&mut app, false);
            assert!(!cancels_close(&output));
            assert!(app.any_dirty());
            assert_eq!(
                app.docs[0].view.editing_text(),
                Some("First window-close title")
            );
            assert_eq!(
                app.docs[1].view.editing_text(),
                Some("Second window-close title")
            );
            app.exec(&egui::Context::default(), Cmd::Home);
            let home = app.active;
            let output = close_request(&mut app, true);
            assert!(cancels_close(&output));
            assert_eq!(app.dialogs.close, Some(CloseIntent::Window));
            assert_eq!(app.active, home);
            assert!(app.any_dirty());
            assert!(
                app.docs
                    .iter()
                    .all(|doc| doc.view.editing_topic().is_none())
            );
            let mut h = harness(app);
            choose(&mut h, choice);
            assert!(h.state().dialogs.close.is_none());
            assert_eq!(h.state().docs.len(), 2);
            assert_eq!(h.state().active, home);
            for (index, doc) in h.state().docs.iter().enumerate() {
                let saved = OpenDoc::open(doc.path.as_ref().unwrap()).unwrap();
                assert_eq!(
                    title(&saved),
                    if choice == "Save" {
                        title(doc)
                    } else {
                        &original[index]
                    }
                );
                assert_eq!(doc.is_dirty(), choice == "Cancel");
            }
            if choice != "Cancel" {
                assert!(!cancels_close(&close_request(h.state_mut(), true)));
            }
            if choice == "Cancel" {
                h.state_mut().active = 0;
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
                assert_eq!(title(&h.state().docs[0]), original[0]);
                assert!(!h.state().docs[0].doc.can_undo());
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let mut app = saved_app(dir.path(), mode);
        let unchanged = title(&app.docs[0]).to_owned();
        draft(&mut app, 0, &unchanged);
        app.request_close_tab(0);
        assert!(app.dialogs.close.is_none());
        assert_eq!(app.docs.len(), 1, "unchanged title closes without a prompt");
        let mut app = saved_app(dir.path(), mode);
        for index in 0..2 {
            let unchanged = title(&app.docs[index]).to_owned();
            draft(&mut app, index, &unchanged);
        }
        assert!(!cancels_close(&close_request(&mut app, true)));
        assert!(app.dialogs.close.is_none());
        assert!(
            app.docs
                .iter()
                .all(|doc| !doc.doc.can_undo() && !doc.is_dirty())
        );
    }
}

#[test]
fn quit_command_remains_distinct_from_the_native_window_close_request() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = saved_app(dir.path(), CenterMode::Map);
    draft(&mut app, 0, "Unsaved before quit");
    app.exec(&egui::Context::default(), Cmd::Quit);

    let output = close_request(&mut app, true);
    assert!(cancels_close(&output));
    assert_eq!(app.dialogs.close, Some(CloseIntent::Quit));
    assert!(app.any_dirty());

    let dir = tempfile::tempdir().unwrap();
    let mut app = saved_app(dir.path(), CenterMode::Map);
    draft(&mut app, 0, "Unsaved before window close");
    let output = close_request(&mut app, true);
    assert!(cancels_close(&output));
    assert_eq!(app.dialogs.close, Some(CloseIntent::Window));
    assert!(app.any_dirty());
}
