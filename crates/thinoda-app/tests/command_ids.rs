//! Locale changes are process-wide, so keep the locale matrix in one test.
use std::collections::BTreeMap;

use egui::{Key, KeyboardShortcut, Modifiers};
use thinoda_app::app::{CenterMode, Cmd, ThinodaApp};
use thinoda_canvas::Action;
use thinoda_core::{TopicStyle, Vec2};
use thinoda_io::Format;
use thinoda_platform::settings::Language;

#[test]
fn stable_shortcut_identity_contract() {
    stable_ids_cover_every_catalog_and_declared_binding_across_locales_and_views();
    stable_namespace_is_pinned_independently_of_palette_order();
    active_tab_resolution_and_non_palette_targets_are_explicit();
    payloads_and_legacy_keys_cannot_alias_builtin_targets_or_activate_overrides();
}

fn stable_ids_cover_every_catalog_and_declared_binding_across_locales_and_views() {
    let mut app = ThinodaApp::headless();
    let initial_catalog: Vec<_> = ThinodaApp::palette_commands()
        .into_iter()
        .map(|(cmd, metadata)| (cmd, metadata.id))
        .collect();
    let expected: BTreeMap<_, _> = app.shortcut_commands().into_iter().collect();
    assert!(!expected.is_empty());
    let catalog_count = initial_catalog.len();
    for language in Language::ALL {
        app.settings.language = language;
        thinoda_app::i18n::set_lang(thinoda_app::i18n::resolve(language));
        for mode in [CenterMode::Map, CenterMode::Outliner] {
            app.mode = mode;
            let registry = app.shortcut_commands();
            assert_eq!(registry.len(), expected.len());
            assert_eq!(registry.into_iter().collect::<BTreeMap<_, _>>(), expected);
            let mut commands: Vec<_> = ThinodaApp::palette_commands()
                .into_iter()
                .map(|(cmd, _)| cmd)
                .collect();
            commands.extend(
                ThinodaApp::global_shortcut_bindings(app.active)
                    .into_iter()
                    .map(|(_, cmd)| cmd),
            );
            commands.extend(
                ThinodaApp::contextual_shortcut_bindings_for_mode(mode)
                    .into_iter()
                    .map(|(_, action)| Cmd::Canvas(action)),
            );
            for command in commands {
                let id = command
                    .shortcut_id()
                    .unwrap_or_else(|| panic!("Missing ID: {command:?}"));
                assert_eq!(expected.get(id), Some(&command), "{id}/{command:?}");
                assert_eq!(app.shortcut_command(id), Some(command));
                assert!(id.contains('.'));
                assert!(id.bytes().all(|byte| byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || byte == b'.'
                    || byte == b'-'));
            }
            let mut reordered = initial_catalog.clone();
            reordered.reverse();
            for (command, palette_id) in &reordered {
                assert_eq!(
                    app.shortcut_command(command.shortcut_id().unwrap()),
                    Some(command.clone())
                );
                assert_eq!(
                    ThinodaApp::palette_commands()
                        .into_iter()
                        .find(|(cmd, _)| cmd == command)
                        .unwrap()
                        .1
                        .id,
                    *palette_id
                );
            }
            assert_eq!(
                ThinodaApp::palette_commands().len(),
                catalog_count,
                "Reference-only targets must not add palette actions"
            );
        }
    }
}

fn stable_namespace_is_pinned_independently_of_palette_order() {
    let app = ThinodaApp::headless();
    let actual = app
        .shortcut_commands()
        .into_iter()
        .map(|(id, _)| id)
        .collect::<std::collections::BTreeSet<_>>();
    // This list is a persistence contract, not a snapshot to regenerate on rename.
    let expected = "
        ai.brainstorm ai.breakdown ai.explain ai.generate-map ai.summarize
        appearance.dark appearance.light appearance.system
        arrange.bottom arrange.distribute-horizontal arrange.distribute-vertical
        arrange.horizontal-center arrange.left arrange.right arrange.top arrange.vertical-center
        edit.command-palette edit.copy edit.cut edit.find edit.find-replace edit.paste edit.presentation edit.redo edit.settings edit.undo edit.zone
        export.csv export.excel export.freemind export.jpeg export.markdown export.mindmanager
        export.opml export.pdf export.png export.svg export.text export.textbundle export.thinoda export.word export.xmind
        file.close-tab file.home file.import file.masked-copy file.merge file.new file.open file.print
        file.quit file.save file.save-all file.save-as file.show-in-folder
        help.about help.feedback help.project-news help.quick-start help.shortcuts help.user-guide
        insert.boundary insert.hyperlink insert.labels insert.notes insert.priority-1 insert.priority-2 insert.priority-3
        insert.relationship insert.summary insert.task insert.todo insert.zone
        navigation.down navigation.left navigation.right navigation.up
        selection.all selection.descendants selection.invert selection.none selection.root selection.siblings
        sheet.copy sheet.delete sheet.duplicate sheet.from-topic sheet.new sheet.next sheet.paste sheet.previous sheet.rename sheet.save-as
        style.copy style.edit-theme style.paste style.reset style.save-quick-style style.topic
        tools.map-shot tools.work-calendar tools.zones
        topic.collapse-all topic.delete topic.delete-only topic.duplicate topic.edit topic.expand-all topic.indent
        topic.insert-child topic.insert-floating topic.insert-parent topic.insert-sibling topic.insert-sibling-before
        topic.move-down topic.move-up topic.open-link topic.outdent topic.sort-ascending topic.sort-descending
        topic.toggle-collapse topic.toggle-collapse-all
        view.fit view.format-panel view.gantt view.next-document view.outliner view.pitch view.pitch-presenter view.status-bar view.symbols-panel
        view.zen view.zoom-in view.zoom-out view.zoom-reset
        window.close window.move-to-new-window
    ".split_whitespace().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual, expected);
}

fn active_tab_resolution_and_non_palette_targets_are_explicit() {
    let mut app = ThinodaApp::headless();
    app.new_doc(thinoda_core::Workbook::default());
    assert_eq!(
        Cmd::CloseTab(0).shortcut_id(),
        Cmd::CloseTab(99).shortcut_id()
    );
    for active in [0, 1, app.docs.len()] {
        app.active = active;
        assert_eq!(
            app.shortcut_command("file.close-tab"),
            Some(Cmd::CloseTab(active))
        );
    }
    for (id, expected) in [
        ("topic.edit", Cmd::Canvas(Action::StartEdit)),
        ("edit.copy", Cmd::Canvas(Action::Copy)),
        ("edit.cut", Cmd::Canvas(Action::Cut)),
        ("edit.paste", Cmd::MenuPaste),
        (
            "insert.priority-3",
            Cmd::Canvas(Action::ToggleMarker("priority-3".into())),
        ),
    ] {
        assert_eq!(app.shortcut_command(id), Some(expected.clone()));
        assert!(
            !ThinodaApp::palette_commands()
                .iter()
                .any(|(cmd, _)| cmd == &expected)
        );
    }
    assert_eq!(
        Cmd::Canvas(Action::Paste(None)).shortcut_id(),
        Cmd::MenuPaste.shortcut_id()
    );
    for format in Format::EXPORTABLE {
        let command = Cmd::Export(*format);
        assert_eq!(
            app.shortcut_command(command.shortcut_id().unwrap()),
            Some(command)
        );
    }
    for (command, id) in [
        (Cmd::Save, "file.save"),
        (Cmd::SaveAs, "file.save-as"),
        (Cmd::PreviousSheet, "sheet.previous"),
        (Cmd::Canvas(Action::StartEdit), "topic.edit"),
        (Cmd::Canvas(Action::Fit), "view.fit"),
        (Cmd::Canvas(Action::DeleteKeepChildren), "topic.delete-only"),
        (
            Cmd::Canvas(Action::ToggleMarker("priority-1".into())),
            "insert.priority-1",
        ),
        (
            Cmd::Canvas(Action::ToggleMarker("priority-2".into())),
            "insert.priority-2",
        ),
        (
            Cmd::Canvas(Action::ToggleMarker("priority-3".into())),
            "insert.priority-3",
        ),
    ] {
        assert_eq!(command.shortcut_id(), Some(id));
    }
}

fn payloads_and_legacy_keys_cannot_alias_builtin_targets_or_activate_overrides() {
    let mut app = ThinodaApp::headless();
    for command in [
        Cmd::OpenPath("source.thinoda".into()),
        Cmd::NewFromTemplate("arbitrary-template"),
        Cmd::ApplyQuickStyle(TopicStyle::default()),
        Cmd::Canvas(Action::Paste(Some("clipboard text".into()))),
        Cmd::Canvas(Action::InsertFloating(Vec2::new(12.0, 42.0))),
        Cmd::Canvas(Action::ToggleMarker("priority-99".into())),
        Cmd::Export(Format::MindManagerXml),
    ] {
        assert_eq!(command.shortcut_id(), None, "{command:?}");
    }
    for id in [
        "0",
        "save",
        "file.unknown-future",
        "",
        "File.Save",
        " file.save",
    ] {
        assert_eq!(app.shortcut_command(id), None, "{id}");
    }
    app.settings.shortcuts_overrides.extend([
        ("save".into(), "Ctrl+Shift+S".into()),
        ("0".into(), "".into()),
        ("file.save".into(), "Ctrl+Alt+F8".into()),
        ("future.command".into(), "unknown-format".into()),
    ]);
    let original = app.settings.shortcuts_overrides.clone();
    assert_eq!(app.shortcut_command("file.save"), Some(Cmd::Save));
    assert_eq!(
        ThinodaApp::shortcut_binding(&Cmd::Save),
        Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::S))
    );
    assert_eq!(app.settings.shortcuts_overrides, original);
}
