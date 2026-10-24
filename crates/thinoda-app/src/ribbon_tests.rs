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
fn collapse_temporary_open_escape_and_keyboard_access_keys() {
    let mut h = harness(1000.0, 700.0);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Collapse Ribbon"))
        .click();
    h.run_steps(2);
    assert!(h.state().settings.ribbon_collapsed);
    assert!(!h.state().ribbon.temporary);
    h.get_by_role_and_label(Role::Button, t("Edit")).click();
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
    assert_eq!(group_details(&counts, 1300.0), vec![GroupDetail::Full; 3]);
    assert!(group_details(&counts, 480.0).contains(&GroupDetail::Compact));
    assert_eq!(group_details(&counts, 180.0), vec![GroupDetail::Menu; 3]);
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
    h.get_by_role_and_label(Role::Button, t("Copy Style"))
        .click();
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
        .with_size(egui::vec2(1000.0, 700.0))
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
