use super::*;
use crate::app::{CenterMode, Cmd};
use egui::{Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};

fn harness(size: egui::Vec2) -> Harness<'static, ThinodaApp> {
    let mut app = ThinodaApp::headless();
    app.settings.show_format_panel = false;
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.state_mut().dialogs.shortcuts = true;
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "Edit mappings")
        .click();
    h.run_steps(4);
    h
}

fn click(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    h.get_by_role_and_label(Role::Button, label).scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, label).click();
    h.run_steps(4);
}

#[test]
fn capture_apply_cancel_and_restart_preserve_named_profiles_and_documents() {
    let mut h = harness(egui::vec2(1000.0, 800.0));
    let before = h.state().settings.keymaps.clone();
    let workbook = h.state().doc().unwrap().doc.workbook().clone();
    h.get_by_role_and_label(Role::TextInput, "Profile name")
        .click();
    h.get_by_role_and_label(Role::TextInput, "Profile name")
        .type_text("Writing");
    h.run_steps(2);
    click(&mut h, "Duplicate");
    h.get_by_role_and_label(Role::TextInput, "Search mappings")
        .click();
    h.get_by_role_and_label(Role::TextInput, "Search mappings")
        .type_text("Subtopic");
    h.run_steps(2);
    click(&mut h, "Assign: Subtopic");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::J);
    h.run_steps(4);
    assert!(
        h.state()
            .dialogs
            .shortcut_browser
            .editor
            .as_ref()
            .unwrap()
            .error
            .is_none()
    );
    assert_eq!(h.state().settings.keymaps, before);
    click(&mut h, "Apply");
    assert!(h.state().dialogs.shortcut_browser.editor.is_none());
    assert_eq!(
        h.state().settings.keymaps.profile().unwrap().name,
        "Writing"
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &workbook);
    let saved = h.state().settings.keymaps.clone();
    let temp = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(temp.path());
    paths.ensure().unwrap();
    h.state().settings.save(&paths).unwrap();
    let reloaded = thinoda_platform::Settings::load(&paths).unwrap();
    assert_eq!(reloaded.keymaps, saved);
    assert_eq!(
        reloaded.keymaps.profile().unwrap().bindings["topic.insert-child"][0].key,
        "J"
    );
    click(&mut h, "Edit mappings");
    click(&mut h, "Reset All");
    click(&mut h, "Cancel");
    assert_eq!(h.state().settings.keymaps, saved);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &workbook);
}

#[test]
fn applying_shared_profiles_cancels_stale_settings_gestures_and_refreshes_each_owner() {
    let mut h = harness(egui::vec2(1000.0, 800.0));
    h.state_mut().dialogs.shortcuts = false;
    h.state_mut().dialogs.settings = true;
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: egui::pos2(30.0, 30.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert!(h.state().dialogs.settings_gesture.is_some());
    let temp = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(temp.path());
    paths.ensure().unwrap();
    let ctx = h.ctx.clone();
    let app = h.state_mut();
    app.services.paths = Some(paths.clone());
    app.format_tab = thinoda_platform::settings::FormatTab::Map;
    app.sidebar_family = thinoda_platform::settings::SidebarFamily::Symbols;
    let child = egui::ViewportId::from_hash_of("shared-keymap-test");
    app.document_windows.insert(app.docs[0].key.clone(), child);
    let mut maps = app.settings.keymaps.clone();
    maps.duplicate_active("Shared").unwrap();
    maps.profiles[0].bindings.insert(
        "file.save".into(),
        vec![crate::keymaps::encode(
            KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::ALT, Key::J),
            ctx.os(),
        )],
    );
    app.apply_keymap_settings(&ctx, maps.clone()).unwrap();
    assert!(app.dialogs.settings_gesture.is_none());
    for viewport in [egui::ViewportId::ROOT, child] {
        app.activate_document_window(viewport);
        app.prepare_keymap(&ctx);
        assert!(
            app.profile_bindings(&app.settings.keymaps, app.mode, ctx.os())
                .iter()
                .any(|(binding, command)| *command == Cmd::Save && binding.logical_key == Key::J)
        );
    }
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: egui::pos2(30.0, 30.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(h.state().settings.keymaps, maps);
    let saved = thinoda_platform::Settings::load(&paths).unwrap();
    assert_eq!(saved.keymaps, maps);
    assert_eq!(saved.format_tab, thinoda_platform::settings::FormatTab::Map);
    assert_eq!(
        saved.sidebar_family,
        thinoda_platform::settings::SidebarFamily::Symbols
    );
}

#[test]
fn failed_profile_save_keeps_the_draft_open_and_canonical_settings_unchanged() {
    let mut h = harness(egui::vec2(1000.0, 800.0));
    let before = h.state().settings.clone();
    let root_file = tempfile::NamedTempFile::new().unwrap();
    h.state_mut().services.paths = Some(thinoda_platform::Paths::from_root(root_file.path()));
    h.state_mut()
        .dialogs
        .shortcut_browser
        .editor
        .as_mut()
        .unwrap()
        .draft
        .duplicate_active("Unsaved")
        .unwrap();
    click(&mut h, "Apply");
    assert_eq!(h.state().settings, before);
    let editor = h.state().dialogs.shortcut_browser.editor.as_ref().unwrap();
    assert!(editor.error.is_some());
    assert_eq!(editor.draft.profile().unwrap().name, "Unsaved");
}

#[test]
fn conflict_and_escape_capture_do_not_change_draft_or_document() {
    let mut h = harness(egui::vec2(1000.0, 800.0));
    h.state_mut()
        .dialogs
        .shortcut_browser
        .editor
        .as_mut()
        .unwrap()
        .draft
        .duplicate_active("Test")
        .unwrap();
    h.run_steps(2);
    click(&mut h, "Assign: Save");
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run_steps(4);
    let editor = h.state().dialogs.shortcut_browser.editor.as_ref().unwrap();
    assert!(editor.error.as_deref().unwrap().contains("conflict"));
    assert!(editor.draft.profile().unwrap().bindings.is_empty());
    click(&mut h, "Assign: Save");
    h.key_press(Key::Escape);
    h.run_steps(3);
    let editor = h.state().dialogs.shortcut_browser.editor.as_ref().unwrap();
    assert!(editor.capture.is_none());
    assert!(editor.draft.profile().unwrap().bindings.is_empty());
    assert_eq!(h.state().docs.len(), 1);
}

#[test]
fn editor_footer_stays_reachable_and_cancel_discards_all_drafts() {
    for theme in [
        thinoda_platform::settings::ThemeMode::Light,
        thinoda_platform::settings::ThemeMode::Dark,
    ] {
        for scale in [1.0, 2.0] {
            let mut h = harness(egui::vec2(1000.0, 800.0));
            h.state_mut().settings.theme = theme;
            h.state_mut().settings.ui_scale = scale;
            h.run_steps(3);
            h.set_size(egui::vec2(320.0, 240.0) / scale);
            h.run_steps(4);
            for label in ["Apply", "Cancel"] {
                let button = h.get_by_role_and_label(Role::Button, label);
                assert!(
                    h.ctx.content_rect().contains_rect(button.rect()),
                    "{theme:?}/{scale}: {label}"
                );
            }
            click(&mut h, "Cancel");
            assert!(h.state().dialogs.shortcut_browser.editor.is_none());
            assert_eq!(h.state().settings.keymaps, Keymaps::default());
        }
    }
}

#[test]
fn assigned_canvas_and_global_commands_dispatch_in_both_views_and_update_hints() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut h = harness(egui::vec2(1000.0, 800.0));
        h.state_mut().dialogs.shortcut_browser.editor = None;
        h.state_mut().dialogs.shortcuts = false;
        h.state_mut().mode = mode;
        let maps = &mut h.state_mut().settings.keymaps;
        maps.duplicate_active("Custom").unwrap();
        maps.profiles[0].bindings.insert(
            "view.outliner".into(),
            vec![thinoda_platform::keymaps::KeyBinding {
                key: "F8".into(),
                ..Default::default()
            }],
        );
        maps.profiles[0].bindings.insert(
            "insert.todo".into(),
            vec![thinoda_platform::keymaps::KeyBinding {
                key: "J".into(),
                command: true,
                alt: true,
                ..Default::default()
            }],
        );
        h.run_steps(4);
        let root = h.state().doc().unwrap().doc.sheet().root;
        h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::J);
        h.run_steps(3);
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .topic(root)
                .unwrap()
                .todo,
            Some(false),
            "{mode:?}"
        );
        assert_eq!(
            h.state().active_shortcut_hint(&h.ctx, &Cmd::ToggleOutliner),
            Some("F8".into())
        );
        h.key_press(Key::F8);
        h.run_steps(3);
        assert_ne!(h.state().mode, mode);
    }
}

#[test]
fn ime_candidate_and_commit_keys_are_not_captured_or_applied() {
    let mut h = harness(egui::vec2(1000.0, 800.0));
    h.state_mut()
        .dialogs
        .shortcut_browser
        .editor
        .as_mut()
        .unwrap()
        .draft
        .duplicate_active("IME")
        .unwrap();
    h.run_steps(2);
    click(&mut h, "Assign: Save");
    h.event(Event::Ime(egui::ImeEvent::Preedit {
        text: "日本".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    for key in [Key::Enter, Key::Escape, Key::Tab] {
        h.key_press(key);
        h.run_steps(2);
        let editor = h.state().dialogs.shortcut_browser.editor.as_ref().unwrap();
        assert!(editor.capture.is_some());
        assert!(editor.draft.profile().unwrap().bindings.is_empty());
    }
    h.input_mut().events.extend([
        Event::Ime(egui::ImeEvent::Commit("日本".into())),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(2);
    assert!(
        h.state()
            .dialogs
            .shortcut_browser
            .editor
            .as_ref()
            .unwrap()
            .draft
            .profile()
            .unwrap()
            .bindings
            .is_empty()
    );
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::J);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .dialogs
            .shortcut_browser
            .editor
            .as_ref()
            .unwrap()
            .draft
            .profile()
            .unwrap()
            .bindings["file.save"][0]
            .key,
        "J"
    );
    assert_eq!(h.state().settings.keymaps, Keymaps::default());
}

#[test]
fn specific_canvas_bindings_are_not_stolen_by_general_global_or_insert_bindings() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for general in ["view.outliner", "insert.priority-1"] {
            let mut h = harness(egui::vec2(1000.0, 800.0));
            h.state_mut().dialogs.shortcut_browser.editor = None;
            h.state_mut().dialogs.shortcuts = false;
            h.state_mut().mode = mode;
            let os = h.ctx.os();
            let maps = &mut h.state_mut().settings.keymaps;
            maps.duplicate_active("Specific").unwrap();
            maps.profiles[0].bindings.insert(
                general.into(),
                vec![crate::keymaps::encode(
                    KeyboardShortcut::new(Modifiers::ALT, Key::J),
                    os,
                )],
            );
            maps.profiles[0].bindings.insert(
                "insert.todo".into(),
                vec![crate::keymaps::encode(
                    KeyboardShortcut::new(Modifiers::ALT | Modifiers::SHIFT, Key::J),
                    os,
                )],
            );
            h.run_steps(4);
            let root = h.state().doc().unwrap().doc.sheet().root;
            h.key_press_modifiers(Modifiers::ALT | Modifiers::SHIFT, Key::J);
            h.run_steps(3);
            let topic = h.state().doc().unwrap().doc.sheet().topic(root).unwrap();
            assert_eq!(topic.todo, Some(false), "{mode:?}/{general}");
            assert!(topic.markers.is_empty(), "{mode:?}/{general}");
            assert_eq!(h.state().mode, mode);
        }
    }
}

#[test]
fn plain_custom_global_key_yields_to_text_editing_and_consumes_its_text_elsewhere() {
    let mut h = harness(egui::vec2(1000.0, 800.0));
    h.state_mut().dialogs.shortcut_browser.editor = None;
    h.state_mut().dialogs.shortcuts = false;
    let maps = &mut h.state_mut().settings.keymaps;
    maps.duplicate_active("Letters").unwrap();
    maps.profiles[0].bindings.insert(
        "file.new".into(),
        vec![thinoda_platform::keymaps::KeyBinding {
            key: "J".into(),
            ..Default::default()
        }],
    );
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Find);
    h.run_steps(4);
    h.input_mut().events.extend([
        Event::Key {
            key: Key::J,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        Event::Text("j".into()),
    ]);
    h.run_steps(3);
    assert_eq!(h.state().docs.len(), 1);
    assert_eq!(h.state().search.query, "j");
    h.key_press(Key::Escape);
    h.run_steps(3);
    h.input_mut().events.extend([
        Event::Key {
            key: Key::J,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        Event::Text("j".into()),
    ]);
    h.run_steps(3);
    assert_eq!(h.state().docs.len(), 2);
    assert!(h.state().doc().unwrap().view.editing_topic().is_none());
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::QuickStart);
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Label, "J").is_some());
}
