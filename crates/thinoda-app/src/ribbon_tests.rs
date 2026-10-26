use super::*;
use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};

fn harness(width: f32, height: f32) -> Harness<'static, ThinodaApp> {
    Harness::builder()
        .with_size(egui::vec2(width, height))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, app: &mut ThinodaApp| {
                app.show_ribbon(ui);
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("Map workspace");
                });
            },
            ThinodaApp::headless(),
        )
}

#[test]
fn ribbon_toggle_is_aligned_with_task_tabs() {
    for width in [640.0, 1200.0] {
        let mut h = harness(width, 700.0);
        h.state_mut().settings.ribbon_collapsed = false;
        h.run_steps(3);
        let toggle = h
            .get_by_role_and_label(Role::Button, t("Collapse Ribbon"))
            .rect();
        let tab = h.get_by_role_and_label(Role::Button, t("File")).rect();
        let caption_center = h
            .output()
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text() == t("File")
                {
                    Some(text.pos.y + text.galley.size().y * 0.5)
                } else {
                    None
                }
            })
            .expect("visible task-tab caption");
        assert!(
            (toggle.center().y - caption_center).abs() < 0.1,
            "{toggle:?}; tab={tab:?}; text center={caption_center}"
        );
        assert!(
            (toggle.center().y - tab.center().y).abs() < 0.1,
            "{toggle:?}, {tab:?}"
        );
    }
}

#[cfg(target_os = "windows")]
#[test]
fn windows_desktop_uses_only_ribbon_with_compact_fallback_for_headless_hosts() {
    for (width, height) in [(1280.0, 800.0), (400.0, 300.0)] {
        let mut app = ThinodaApp::headless();
        app.desktop_ribbon = true;
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, height))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(3);
        assert!(h.query_by_label(crate::i18n::t("Main Menu")).is_none());
        if height < 400.0 {
            h.get_by_role_and_label(Role::Button, t("Ribbon"));
        } else {
            h.get_by_role_and_label(Role::Button, t("File"));
            h.get_by_role_and_label(Role::Button, t("Home"));
        }
        h.state_mut().desktop_ribbon = false;
        h.run_steps(3);
        assert!(h.query_by_label(crate::i18n::t("Main Menu")).is_some());
        assert!(
            h.query_by_role_and_label(Role::Button, t("Ribbon"))
                .is_none()
        );
    }
}

#[test]
fn explanatory_captions_are_tooltips_not_ribbon_controls() {
    let help = t("Copies explicit overrides; inherited theme styles stay automatic.");
    let mut h = harness(1800.0, 700.0);
    h.state_mut().ribbon.tab = 1;
    h.run_steps(3);
    assert!(!h.output().shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == help)
    }));
    let footer = h.get_by_role_and_label(Role::Label, t("Styles")).rect();
    h.hover_at(footer.center());
    h.run_steps(90);
    assert!(
        h.output().shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == help)
        }),
        "group explanation should be available on hover"
    );
    let command = h
        .get_by_role_and_label(Role::Button, t("Copy Style"))
        .rect();
    h.hover_at(command.center());
    h.run_steps(90);
    let tooltips: Vec<_> = h
        .output()
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text().contains(help) => {
                Some(text.galley.text())
            }
            _ => None,
        })
        .collect();
    assert_eq!(tooltips.len(), 1, "one tooltip per command: {tooltips:?}");
    assert!(tooltips[0].contains(t("Copy Style")));
    h.state_mut().ribbon.tab = 5;
    h.event(egui::Event::PointerGone);
    h.run_steps(3);
    let help = t(
        "Appends complete sheets. Destination metadata is kept; external image references are not embedded.",
    );
    assert!(!h.output().shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == help)
    }));
    let first = h.get_by_role_and_label(Role::Button, t("Map Shot")).rect();
    let second = h
        .get_by_role_and_label(Role::Button, t("Create Masked Copy…"))
        .rect();
    assert!(first.height() > second.height());
    assert!((first.top() - second.top()).abs() < 0.1);
}

#[test]
fn collapse_temporary_open_escape_and_keyboard_access_keys() {
    let mut h = harness(1000.0, 700.0);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Collapse Ribbon"))
        .click();
    h.run_steps(2);
    assert!(h.state().settings.ribbon_collapsed);
    assert!(!h.state().ribbon.temporary);
    h.get_by_role_and_label(Role::Button, t("Home")).click();
    h.run_steps(2);
    assert_eq!(h.state().ribbon.tab, 1);
    assert!(h.state().ribbon.temporary);
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(!h.state().ribbon.temporary);
    h.key_press_modifiers(Modifiers::ALT, Key::Num3);
    h.run_steps(2);
    assert_eq!(h.state().ribbon.tab, 2);
    assert!(h.state().ribbon.temporary);
    h.key_press_modifiers(Modifiers::CTRL, Key::F1);
    h.run_steps(2);
    assert!(!h.state().settings.ribbon_collapsed);
}

#[test]
fn ribbon_invokes_shared_sheet_command_once_and_preserves_undo() {
    let mut h = harness(1000.0, 700.0);
    h.state_mut().ribbon.tab = 2;
    h.run_steps(3);
    let count = h.state().doc().unwrap().doc.workbook().sheets.len();
    h.get_by_role_and_label(Role::Button, t("New Sheet"))
        .click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.workbook().sheets.len(),
        count + 1
    );
    h.state_mut().exec(
        &egui::Context::default(),
        Cmd::Canvas(thinoda_canvas::Action::Undo),
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), count);
}

#[test]
fn pin_persists_and_keeps_the_ribbon_open_after_actions() {
    let mut h = harness(1400.0, 700.0);
    h.state_mut().settings.ribbon_collapsed = true;
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Sheet")).click();
    h.run_steps(3);
    assert!(h.state().ribbon.temporary);
    h.get_by_role_and_label(Role::Button, t("Pin Ribbon"))
        .click();
    h.run_steps(3);
    assert!(!h.state().settings.ribbon_collapsed);
    assert!(!h.state().ribbon.temporary);
    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::paths::Paths::from_root(directory.path());
    h.state().settings.save(&paths).unwrap();
    assert!(
        !thinoda_platform::settings::Settings::load(&paths)
            .unwrap()
            .ribbon_collapsed
    );
    let before = h.state().doc().unwrap().doc.workbook().sheets.len();
    h.get_by_role_and_label(Role::Button, t("New Sheet"))
        .click();
    h.run_steps(4);
    assert_eq!(
        h.state().doc().unwrap().doc.workbook().sheets.len(),
        before + 1
    );
    h.get_by_role_and_label(Role::Button, t("New Sheet"));
    h.get_by_role_and_label(Role::Button, t("Unpin Ribbon"))
        .click();
    h.run_steps(3);
    assert!(h.state().settings.ribbon_collapsed);
    assert!(h.state().ribbon.temporary);
    h.get_by_role_and_label(Role::Button, t("New Sheet"))
        .click();
    h.run_steps(4);
    assert!(!h.state().ribbon.temporary);
    assert!(
        h.query_by_role_and_label(Role::Button, t("New Sheet"))
            .is_none()
    );
}

#[test]
fn compact_ribbon_defaults_on_and_ignores_the_legacy_key() {
    assert!(thinoda_platform::settings::Settings::default().compact_ribbon);
    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::paths::Paths::from_root(directory.path());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(
        paths.config_dir.join("settings.toml"),
        "compact_ribbon = false\n",
    )
    .unwrap();
    assert!(
        thinoda_platform::settings::Settings::load(&paths)
            .unwrap()
            .compact_ribbon
    );
}

#[test]
fn compact_ribbon_settings_toggle_persists() {
    let mut app = ThinodaApp::headless();
    app.dialogs.settings = true;
    app.dialogs.settings_tab = crate::app::SettingsTab::Editor;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Switch, t("Compact Ribbon"))
        .scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Switch, t("Compact Ribbon"))
        .click();
    h.run_steps(3);
    assert!(!h.state().settings.compact_ribbon);
    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::paths::Paths::from_root(directory.path());
    h.state().settings.save(&paths).unwrap();
    assert!(
        !thinoda_platform::settings::Settings::load(&paths)
            .unwrap()
            .compact_ribbon
    );
    h.get_by_role_and_label(Role::Switch, t("Compact Ribbon"))
        .click();
    h.run_steps(3);
    assert!(h.state().settings.compact_ribbon);
}

#[test]
fn compact_ribbon_shortens_all_pages_and_preserves_command_content() {
    for compact in [false, true] {
        let mut h = harness(1600.0, 700.0);
        h.state_mut().settings.compact_ribbon = compact;
        h.run_steps(3);
        for tab in 0..8 {
            h.state_mut().ribbon.tab = tab;
            h.run_steps(3);
            let panel = h.state().ribbon.rect.unwrap();
            assert_eq!(panel.height(), if compact { 160.0 } else { 200.0 });
            let mut captions = 0;
            for shape in &h.output().shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                    if panel.contains(bounds.center()) {
                        assert!(
                            panel.contains_rect(bounds),
                            "tab={tab}, compact={compact}: {}",
                            text.galley.text()
                        );
                        assert!(
                            shape.clip_rect.contains_rect(bounds),
                            "clipped tab={tab}, compact={compact}: {}",
                            text.galley.text()
                        );
                        captions += 1;
                    }
                }
            }
            assert!(captions > 8);
        }
    }
}

#[test]
fn short_viewport_uses_bounded_menu_fallback_and_modals_block_commands() {
    let mut h = harness(400.0, 300.0);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Ribbon")).click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("File")).click();
    h.run_steps(3);
    let label = format!(
        "{} {}",
        t("New"),
        ThinodaApp::shortcut_hint(&h.ctx, &Cmd::New).unwrap()
    );
    h.get_by_role_and_label(Role::Button, &label).scroll_to_me();
    h.run_steps(3);
    assert!(
        h.ctx
            .content_rect()
            .contains_rect(h.get_by_role_and_label(Role::Button, &label).rect())
    );
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.state_mut().dialogs.settings = true;
    h.key_press_modifiers(Modifiers::ALT, Key::Num2);
    h.run_steps(2);
    assert_eq!(h.state().ribbon.tab, 0);
}

#[test]
fn grouping_retains_every_menu_entry_except_separators() {
    let app = ThinodaApp::headless();
    for menu in app.main_menu_entries() {
        let MenuEntry::Submenu { label, entries } = menu else {
            panic!("menu group")
        };
        let mut grouped: Vec<_> = command_groups(&entries, &label)
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .collect();
        for entry in entries
            .iter()
            .filter(|entry| !matches!(entry, MenuEntry::Separator))
        {
            let index = grouped
                .iter()
                .position(|candidate| *candidate == entry)
                .expect("Ribbon lost a menu entry");
            grouped.remove(index);
        }
        assert!(grouped.is_empty(), "Ribbon duplicated a menu entry");
    }
}

#[test]
fn responsive_groups_reduce_detail_without_clipping_or_losing_menu_access() {
    let counts = [4, 5, 3];
    assert_eq!(
        group_details(&counts, 1800.0, false),
        vec![GroupDetail::Full; 3]
    );
    assert!(group_details(&counts, 800.0, false).contains(&GroupDetail::Compact));
    assert_eq!(
        group_details(&counts, 180.0, false),
        vec![GroupDetail::Menu; 3]
    );
    for width in [1600.0, 1000.0, 720.0, 440.0, 320.0] {
        let mut h = harness(width, 700.0);
        for category in 0..8 {
            h.state_mut().ribbon.tab = category;
            h.run_steps(4);
            let ribbon = h.state().ribbon.rect.unwrap();
            let commands: Vec<_> = h
                .query_all_by_role(Role::Button)
                .map(|node| node.rect())
                .filter(|rect| rect.top() > ribbon.top() + 38.0)
                .collect();
            for (index, rect) in commands.iter().enumerate() {
                assert!(
                    ribbon.contains_rect(*rect),
                    "{width}/{category}: {rect:?} outside {ribbon:?}"
                );
                for other in commands.iter().skip(index + 1) {
                    assert!(
                        !rect.shrink(0.5).intersects(*other),
                        "{width}/{category}: overlapping commands {rect:?}/{other:?}"
                    );
                }
            }
            assert!(!commands.is_empty());
        }
    }
    let mut h = harness(320.0, 700.0);
    h.state_mut().ribbon.tab = 1;
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, t("Ribbon")).click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Styles")).click();
    h.run_steps(3);
    let hint = ThinodaApp::shortcut_hint(&h.ctx, &Cmd::ResetStyle);
    let label = hint.map_or_else(
        || t("Reset Style").to_owned(),
        |hint| format!("{} {hint}", t("Reset Style")),
    );
    let reset = h.get_by_role_and_label(Role::Button, &label);
    assert!(h.ctx.content_rect().contains_rect(reset.rect()));
}

#[test]
fn icon_submenus_can_be_opened_from_keyboard() {
    let mut h = harness(1000.0, 700.0);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("New from Template…"))
        .focus();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Basic"));
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!egui::Popup::is_any_open(&h.ctx));
}

#[test]
fn ribbon_submenus_preserve_shared_template_commands() {
    let mut h = harness(1000.0, 700.0);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("New from Template…"))
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Basic")).click();
    h.run_steps(3);
    let count = h.state().docs.len();
    h.get_by_role_and_label(Role::Button, t("Mind Map")).click();
    h.run_steps(3);
    assert_eq!(h.state().docs.len(), count + 1);
}

#[test]
fn ribbon_text_undo_preserves_the_map_and_restores_text() {
    struct State {
        app: ThinodaApp,
        text: String,
    }
    let field = egui::Id::unique("ribbon_text");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 700.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            move |ui, state: &mut State| {
                state.app.show_ribbon(ui);
                egui::CentralPanel::default().show(ui, |ui| {
                    let label = ui.label("Draft");
                    ui.add(egui::TextEdit::singleline(&mut state.text).id(field))
                        .labelled_by(label.id);
                });
            },
            State {
                app: ThinodaApp::headless(),
                text: "Original".into(),
            },
        );
    h.state_mut().app.ribbon.tab = 1;
    h.run_steps(60);
    h.ctx.memory_mut(|memory| memory.request_focus(field));
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("Changed".into()));
    h.run_steps(60);
    assert_eq!(h.state().text, "Changed");
    assert!(h.ctx.text_edit_focused());
    assert!(
        h.state()
            .app
            .ribbon
            .snapshot
            .as_ref()
            .unwrap()
            .has_text_focus()
    );
    let before = h.state().app.doc().unwrap().doc.workbook().clone();
    h.get_by_role_and_label(Role::Button, t("Undo")).click();
    h.run_steps(3);
    assert!(
        h.ctx.text_edit_focused(),
        "Ribbon did not restore text focus: {:?}",
        h.ctx.memory(|memory| memory.focused())
    );
    assert_eq!(h.state().text, "Original");
    assert_eq!(h.state().app.doc().unwrap().doc.workbook(), &before);
}

#[cfg(target_os = "windows")]
#[test]
fn quick_access_customization_persists_and_tabs_support_arrow_navigation() {
    let mut app = ThinodaApp::headless();
    app.desktop_ribbon = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, t("Home")).focus();
    h.key_press(Key::ArrowRight);
    h.run_steps(4);
    assert_eq!(h.state().ribbon.tab, 2);
    h.key_press(Key::ArrowLeft);
    h.run_steps(4);
    assert_eq!(h.state().ribbon.tab, 1);
    h.get_by_role_and_label(Role::Button, t("Quick Access Toolbar"))
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::CheckBox, t("Print")).click();
    h.run_steps(3);
    assert!(!h.state().settings.quick_access.print);
    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::paths::Paths::from_root(directory.path());
    h.state().settings.save(&paths).unwrap();
    let loaded = thinoda_platform::settings::Settings::load(&paths).unwrap();
    assert_eq!(loaded.quick_access, h.state().settings.quick_access);
}
