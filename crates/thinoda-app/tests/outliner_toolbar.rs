//! Localized mode/toolbar coverage must be sequential.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, SidebarFamily, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::Action;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn exec(h: &mut Harness<'_, ThinodaApp>, command: Cmd) {
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, command);
    h.run_steps(4);
}

fn click(h: &mut Harness<'_, ThinodaApp>, label: &'static str) {
    h.get_by_role_and_label(Role::Button, t(label)).click();
    h.run_steps(4);
}

fn assert_outline_controls(h: &Harness<'_, ThinodaApp>) {
    assert_eq!(h.state().mode, CenterMode::Outliner);
    assert!(
        h.query_by_role_and_label(Role::Button, t("More insert actions"))
            .is_none()
    );
    let visible = h.ctx.memory(|memory| memory.areas().visible_layer_ids());
    assert!(
        !visible
            .iter()
            .any(|layer| { layer.id == egui::Id::unique("thinoda_editing_toolbar") })
    );
    let viewport = h.ctx.content_rect();
    let panel = h
        .query_by_role_and_label(Role::Tab, t("Style"))
        .map(|node| node.rect().left());
    for label in [
        "Add Sibling",
        "Add Child",
        "Edit Topic",
        "Move Up",
        "Move Down",
        "Indent",
        "Outdent",
        "Delete Topic",
        "Delete Topic Only",
    ] {
        let rect = h.get_by_role_and_label(Role::Button, t(label)).rect();
        assert!(viewport.contains_rect(rect), "{label}: {rect:?}");
        if let Some(left) = panel {
            assert!(rect.right() < left, "{label}: overlaps inspector");
        }
        for layer in &visible {
            if layer.order == egui::Order::Foreground {
                let area = h.ctx.memory(|memory| memory.area_rect(layer.id)).unwrap();
                assert!(!area.intersects(rect), "{label}: covered by {area:?}");
            }
        }
        assert!(
            h.output().shapes.iter().any(|shape| {
                matches!(
                                &shape.shape,
                                egui::Shape::Text(text) if text.galley.job.text.chars().all(|c| c >= '\u{f0000}')
                            ) && rect.contains_rect(shape.shape.visual_bounding_rect())
                    && shape.clip_rect.contains_rect(rect)
            }),
            "{label}: missing unclipped command icon"
        );
    }
}

#[test]
fn outline_navigation_skips_hidden_rows_and_declared_redo_restores_edits() {
    let mut app = ThinodaApp::headless();
    app.mode = CenterMode::Outliner;
    app.settings.show_format_panel = false;
    let doc = app.doc_mut().unwrap();
    doc.doc
        .edit_sheet("Collapsed fixture", |sheet| {
            let branch = sheet.add_child(sheet.root, None, "Collapsed branch");
            sheet.add_child(branch, None, "Hidden descendant");
            sheet.set_collapsed(branch, true)?;
            sheet.add_child(sheet.root, None, "Visible sibling");
            Ok(())
        })
        .unwrap();
    let branch = doc
        .doc
        .sheet()
        .topics
        .values()
        .find(|topic| topic.title == "Collapsed branch")
        .unwrap()
        .id;
    let sibling = doc
        .doc
        .sheet()
        .topics
        .values()
        .find(|topic| topic.title == "Visible sibling")
        .unwrap()
        .id;
    doc.view.select_only(branch);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "Collapsed branch")
        .click();
    h.run_steps(3);
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.primary(), Some(sibling));
    assert!(
        h.get_by_role_and_label(Role::Button, "Visible sibling")
            .is_focused()
    );
    h.key_press(egui::Key::ArrowUp);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.primary(), Some(branch));
    assert!(
        h.get_by_role_and_label(Role::Button, "Collapsed branch")
            .is_focused()
    );
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.key_press(egui::Key::Delete);
    h.run_steps(3);
    let deleted = h.state().doc().unwrap().doc.workbook().clone();
    assert_ne!(before, deleted);
    h.get_by_role_and_label(Role::Button, t("Format Panel"))
        .focus();
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(
        h.get_by_role_and_label(Role::Button, t("Format Panel"))
            .is_focused()
    );
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Y);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &deleted);
    assert!(
        h.get_by_role_and_label(Role::Button, t("Format Panel"))
            .is_focused()
    );
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::Z,
    );
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &deleted);
    assert!(
        h.get_by_role_and_label(Role::Button, t("Format Panel"))
            .is_focused()
    );
}

#[test]
fn outline_actions_are_not_covered_by_the_map_insertion_toolbar() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (scale, sidebar, width) in [
                (1.0, true, 1280.0),
                (1.5, false, 1040.0),
                (2.0, true, 640.0),
            ] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: scale,
                        show_format_panel: sidebar,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                let doc = app.doc_mut().unwrap();
                doc.doc
                    .edit_sheet("Fixture", |sheet| {
                        sheet.add_child(sheet.root, None, "Fixture A");
                        sheet.add_child(sheet.root, None, "Fixture B");
                        sheet.add_child(sheet.root, None, "Fixture C");
                        Ok(())
                    })
                    .unwrap();
                let sheet = doc.doc.sheet();
                let children = sheet.children(sheet.root);
                let selected = children[children.len() - 2];
                let preceding = children[children.len() - 3];
                doc.view.select_only(selected);
                let mut h = Harness::builder()
                    .with_size(egui::vec2(width, 800.0))
                    .with_context_setup(|ctx| {
                        ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    })
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.run_steps(4);
                let before = h.state().doc().unwrap().doc.sheet().clone();
                let revision = h.state().doc().unwrap().doc.state_id();
                h.get_by_role_and_label(Role::Button, t("More insert actions"));
                click(&mut h, "Outliner");
                assert_outline_controls(&h);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
                assert_eq!(h.state().doc().unwrap().view.primary(), Some(selected));

                h.get_by_role_and_label(Role::Button, t("Add Child"))
                    .focus();
                h.run_steps(3);
                h.key_press(egui::Key::Tab);
                h.run_steps(3);
                assert_eq!(
                    h.state().doc().unwrap().doc.sheet(),
                    &before,
                    "Tab from the action toolbar must not insert a topic"
                );
                for label in ["Move Up", "Format Panel"] {
                    h.get_by_role_and_label(Role::Button, t(label)).focus();
                    h.run_steps(3);
                    let selection = h.state().doc().unwrap().view.selection.clone();
                    for key in [egui::Key::Delete, egui::Key::F2, egui::Key::ArrowDown] {
                        h.key_press(key);
                        h.run_steps(3);
                        assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
                        assert_eq!(h.state().doc().unwrap().view.selection, selection);
                        assert!(h.state().doc().unwrap().view.editing_topic().is_none());
                    }
                }
                h.get_by_role_and_label(Role::Button, t("Move Down"))
                    .focus();
                h.run_steps(3);
                h.key_press(egui::Key::Enter);
                h.run_steps(3);
                let moved = h.state().doc().unwrap().doc.sheet();
                assert_eq!(moved.topics.len(), before.topics.len());
                assert_eq!(
                    moved.index_in_parent(selected).unwrap(),
                    before.index_in_parent(selected).unwrap() + 1
                );
                exec(&mut h, Cmd::Canvas(Action::Undo));
                assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
                h.get_by_role_and_label(Role::Button, "Fixture B").click();
                h.run_steps(3);
                assert!(
                    h.get_by_role_and_label(Role::Button, "Fixture B")
                        .is_focused()
                );
                h.key_press(egui::Key::ArrowUp);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().view.primary(), Some(preceding));
                assert!(
                    h.get_by_role_and_label(Role::Button, "Fixture A")
                        .is_focused()
                );
                h.key_press(egui::Key::ArrowDown);
                h.run_steps(3);
                assert!(
                    h.get_by_role_and_label(Role::Button, "Fixture B")
                        .is_focused()
                );
                h.key_press(egui::Key::Tab);
                h.run_steps(3);
                assert_eq!(
                    h.state()
                        .doc()
                        .unwrap()
                        .doc
                        .sheet()
                        .children(selected)
                        .len(),
                    1
                );
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                assert!(h.state().doc().unwrap().view.editing_topic().is_none());
                h.get_by_role_and_label(Role::Button, t("Format Panel"))
                    .focus();
                h.run_steps(3);
                h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);

                for (label, action) in [
                    ("Move Up", Action::MoveUp),
                    ("Move Down", Action::MoveDown),
                    ("Indent", Action::Indent),
                    ("Delete Topic", Action::Delete),
                    ("Add Sibling", Action::InsertSibling),
                    ("Add Child", Action::InsertChild),
                ] {
                    h.state_mut().doc_mut().unwrap().view.select_only(selected);
                    h.run_steps(3);
                    assert!(h.state().command_available(&Cmd::Canvas(action.clone())));
                    click(&mut h, label);
                    h.key_press(egui::Key::Escape);
                    h.run_steps(3);
                    let sheet = h.state().doc().unwrap().doc.sheet();
                    assert_ne!(sheet, &before, "{language:?}/{label}: click did nothing");
                    match action {
                        Action::MoveUp => assert_eq!(
                            sheet.index_in_parent(selected).unwrap() + 1,
                            before.index_in_parent(selected).unwrap()
                        ),
                        Action::MoveDown => assert_eq!(
                            sheet.index_in_parent(selected).unwrap(),
                            before.index_in_parent(selected).unwrap() + 1
                        ),
                        Action::Indent => assert_eq!(sheet.parent(selected), Some(preceding)),
                        Action::Delete => assert!(!sheet.contains(selected)),
                        Action::InsertSibling | Action::InsertChild => {
                            assert_eq!(sheet.topics.len(), before.topics.len() + 1);
                        }
                        _ => unreachable!(),
                    }
                    let after = sheet.clone();
                    exec(&mut h, Cmd::Canvas(Action::Undo));
                    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
                    exec(&mut h, Cmd::Canvas(Action::Redo));
                    assert_eq!(h.state().doc().unwrap().doc.sheet(), &after);
                    exec(&mut h, Cmd::Canvas(Action::Undo));
                }
                h.state_mut().doc_mut().unwrap().view.select_only(selected);
                h.run_steps(3);
                click(&mut h, "Indent");
                click(&mut h, "Outdent");
                assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
                assert_outline_controls(&h);
                click(&mut h, "Outliner");
                assert_eq!(h.state().mode, CenterMode::Map);
                h.get_by_role_and_label(Role::Button, t("More insert actions"));
                assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);

                click(&mut h, "More insert actions");
                assert!(egui::Popup::is_any_open(&h.ctx));
                click(&mut h, "Outliner");
                assert_outline_controls(&h);
                assert!(!egui::Popup::is_any_open(&h.ctx));
                click(&mut h, "Outliner");
                assert!(!egui::Popup::is_any_open(&h.ctx));
                if !sidebar {
                    click(&mut h, "Outliner");
                    click(&mut h, "Format Panel");
                    assert!(h.state().settings.show_format_panel);
                    h.get_by_role_and_label(Role::Tab, t("Style"));
                    click(&mut h, "Format Panel");
                    assert!(!h.state().settings.show_format_panel);
                    click(&mut h, "Symbols");
                    assert_eq!(h.state().sidebar_family, SidebarFamily::Symbols);
                    assert!(h.state().settings.show_format_panel);
                    click(&mut h, "Symbols");
                    assert!(!h.state().settings.show_format_panel);
                    h.key_press_modifiers(
                        egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                        egui::Key::N,
                    );
                    h.run_steps(4);
                    assert!(h.state().settings.show_format_panel);
                    assert_eq!(h.state().mode, CenterMode::Map);
                    assert_eq!(h.state().sidebar_family, SidebarFamily::Format);
                    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
                }
            }
        }
    }
}
