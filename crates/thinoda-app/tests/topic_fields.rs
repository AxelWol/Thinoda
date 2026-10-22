//! Command availability and localized field routes run in one process-wide matrix.

use egui::accesskit::Role;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{CenterMode, Cmd, FocusField, FormatTab, Services, SidebarFamily, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::{Action, ObjectSelection};
use thinoda_core::{RelationshipId, TopicId};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn assert_blocked(app: &mut ThinodaApp) {
    for field in [FocusField::Notes, FocusField::Hyperlink] {
        let command = Cmd::EditField(field);
        assert!(!app.command_available(&command));
        let before = app.doc().map(|doc| {
            (
                doc.doc.workbook().clone(),
                doc.doc.state_id(),
                doc.view.selection.clone(),
                doc.view.editing_text().map(str::to_owned),
            )
        });
        let mode = app.mode;
        let panel = app.settings.show_format_panel;
        let focus = app.focus_field;
        app.toasts.tick(60.0);
        let warnings = app.toasts.texts().count();
        app.exec(&egui::Context::default(), command);
        assert_eq!(app.mode, mode);
        assert_eq!(app.settings.show_format_panel, panel);
        assert_eq!(app.focus_field, focus);
        assert_eq!(app.toasts.texts().count(), warnings + 1);
        assert_eq!(app.toasts.texts().last(), Some(t("Not possible here")));
        assert_eq!(
            app.doc().map(|doc| (
                doc.doc.workbook().clone(),
                doc.doc.state_id(),
                doc.view.selection.clone(),
                doc.view.editing_text().map(str::to_owned),
            )),
            before
        );
    }
}

fn open_insert(h: &mut Harness<'_, ThinodaApp>) {
    h.get_by_label(thinoda_ui::icons::MENU).click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Insert")).click();
    h.run_steps(3);
}

fn exec(h: &mut Harness<'_, ThinodaApp>, command: Cmd) {
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, command);
    h.run_steps(4);
}

#[test]
fn topic_fields_share_selection_guards_and_preserve_inline_edits() {
    let mut app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    assert_blocked(&mut app);
    app.exec(&egui::Context::default(), Cmd::New);
    let root = app.doc().unwrap().doc.sheet().root;
    let child = app.doc().unwrap().doc.sheet().children(root)[0];
    app.doc_mut().unwrap().view.selection.clear();
    assert_blocked(&mut app);
    app.doc_mut().unwrap().view.selection = vec![root, child];
    assert_blocked(&mut app);
    app.doc_mut().unwrap().view.selection = vec![TopicId(u64::MAX)];
    assert_blocked(&mut app);
    app.doc_mut().unwrap().view.select_only(root);
    app.doc_mut().unwrap().view.selected_object =
        Some(ObjectSelection::Relationship(RelationshipId(u64::MAX)));
    assert_blocked(&mut app);
    app.doc_mut().unwrap().view.select_only(root);
    app.dialogs.settings = true;
    assert_blocked(&mut app);
    app.dialogs.settings = false;
    app.dialogs.about = true;
    assert_blocked(&mut app);
    app.dialogs.about = false;
    app.dialogs.shortcuts = true;
    assert_blocked(&mut app);
    app.dialogs.shortcuts = false;
    app.dialogs.guide.open = true;
    assert_blocked(&mut app);
    app.dialogs.guide.open = false;

    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for field in [FocusField::Notes, FocusField::Hyperlink] {
            app.mode = mode;
            let doc = app.doc_mut().unwrap();
            let before = doc.doc.sheet().clone();
            doc.view.begin_edit(doc.doc.sheet(), root, false);
            doc.view
                .set_editing_text("Committed before opening a topic field");
            app.exec(&egui::Context::default(), Cmd::EditField(field));
            let doc = app.doc().unwrap();
            assert_eq!(doc.view.editing_topic(), None);
            assert_eq!(
                doc.doc.sheet().topic(root).unwrap().title,
                "Committed before opening a topic field"
            );
            assert_eq!(doc.view.primary(), Some(root));
            assert_eq!(app.mode, CenterMode::Map);
            assert_eq!(app.focus_field, Some(field));
            app.exec(&egui::Context::default(), Cmd::Canvas(Action::Undo));
            assert_eq!(app.doc().unwrap().doc.sheet(), &before);
            app.focus_field = None;
        }
    }

    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (mode, scale) in [
                (CenterMode::Map, 1.0),
                (CenterMode::Outliner, 1.5),
                (CenterMode::Outliner, 2.0),
            ] {
                for (field, label, value) in [
                    (FocusField::Notes, "Notes", "A durable topic note"),
                    (FocusField::Hyperlink, "Hyperlink", "https://example.com/"),
                ] {
                    let mut app = ThinodaApp::new(
                        Settings {
                            language,
                            theme,
                            ui_scale: scale,
                            show_format_panel: false,
                            ..Default::default()
                        },
                        Services::default(),
                        &[],
                    );
                    app.exec(&egui::Context::default(), Cmd::New);
                    app.mode = mode;
                    let mut h = Harness::builder()
                        .with_size(egui::vec2(1280.0, 800.0))
                        .with_context_setup(|ctx| {
                            ctx.set_missing_glyph_policy(
                                egui::epaint::text::MissingGlyphPolicy::Tofu,
                            );
                        })
                        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                    h.run_steps(4);
                    let before = h.state().doc().unwrap().doc.sheet().clone();
                    let selected = h.state().doc().unwrap().view.primary().unwrap();
                    let menu_label = format!(
                        "{} {}",
                        t(label),
                        ThinodaApp::shortcut_hint(&h.ctx, &Cmd::EditField(field)).unwrap()
                    );
                    open_insert(&mut h);
                    let node = h.get_by_role_and_label(Role::Button, &menu_label);
                    assert!(!node.accesskit_node().is_disabled());
                    node.scroll_to_me();
                    h.run_steps(6);
                    h.get_by_role_and_label(Role::Button, &menu_label).click();
                    h.run_steps(4);
                    assert_eq!(h.state().mode, CenterMode::Map);
                    assert!(h.state().settings.show_format_panel);
                    assert_eq!(h.state().sidebar_family, SidebarFamily::Format);
                    assert_eq!(h.state().format_tab, FormatTab::Style);
                    assert_eq!(h.state().doc().unwrap().view.primary(), Some(selected));
                    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
                    let role = match field {
                        FocusField::Notes => Role::MultilineTextInput,
                        FocusField::Hyperlink => Role::TextInput,
                    };
                    let input = h.get_by_role_and_label(role, t(label));
                    assert!(
                        input.is_focused(),
                        "{language:?} {theme:?} {mode:?} scale={scale} {field:?}: field={:?}, focus={:?}",
                        input.rect(),
                        h.ctx
                            .memory(|memory| memory.focused())
                            .and_then(|id| h.ctx.read_response(id))
                            .map(|response| response.rect)
                    );
                    input.type_text(value);
                    h.run_steps(4);
                    h.get_by_role_and_label(Role::Tab, t("Style")).focus();
                    h.run_steps(4);
                    let topic = h
                        .state()
                        .doc()
                        .unwrap()
                        .doc
                        .sheet()
                        .topic(selected)
                        .unwrap();
                    match field {
                        FocusField::Notes => assert_eq!(topic.notes.as_deref(), Some(value)),
                        FocusField::Hyperlink => {
                            assert_eq!(topic.hyperlink.as_deref(), Some(value))
                        }
                    }
                    let after = h.state().doc().unwrap().doc.sheet().clone();
                    exec(&mut h, Cmd::Canvas(Action::Undo));
                    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
                    exec(&mut h, Cmd::Canvas(Action::Redo));
                    assert_eq!(h.state().doc().unwrap().doc.sheet(), &after);

                    h.state_mut().doc_mut().unwrap().view.selection.clear();
                    h.run_steps(3);
                    open_insert(&mut h);
                    assert!(
                        h.get_by_role_and_label(Role::Button, &menu_label)
                            .accesskit_node()
                            .is_disabled()
                    );
                    h.key_press(egui::Key::Escape);
                    h.run_steps(3);
                    h.get_by_role_and_label(Role::Button, t("More insert actions"))
                        .click();
                    h.run_steps(3);
                    assert!(
                        h.get_by_role_and_label(Role::Button, t(label))
                            .accesskit_node()
                            .is_disabled()
                    );
                }
            }
        }
    }
}
