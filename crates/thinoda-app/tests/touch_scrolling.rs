use egui::{
    Event, Modifiers, PointerButton, TouchDeviceId, TouchId, TouchPhase, accesskit::Role, vec2,
};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, EXPORT_MENU, Services, SidebarFamily, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn harness(theme: ThemeMode, scale: f32) -> Harness<'static, ThinodaApp> {
    let mut app = ThinodaApp::new(
        Settings {
            language: Language::English,
            theme,
            ui_scale: scale,
            show_format_panel: false,
            show_topic_toolbar: false,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    app.exec(&egui::Context::default(), Cmd::New);
    let mut h = Harness::builder()
        .with_size(vec2(640.0, 360.0) * scale)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.set_size(vec2(640.0, 360.0));
    h.run_steps(4);
    h
}

fn swipe(h: &mut Harness<'_, ThinodaApp>, start: egui::Pos2, delta: egui::Vec2) {
    for (phase, fraction) in [
        (TouchPhase::Start, 0.0),
        (TouchPhase::Move, 0.2),
        (TouchPhase::Move, 1.0),
        (TouchPhase::End, 1.0),
    ] {
        let pos = start + delta * fraction;
        h.event(Event::PointerMoved(pos));
        h.event(Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        });
        if matches!(phase, TouchPhase::Start | TouchPhase::End) {
            h.event(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: phase == TouchPhase::Start,
                modifiers: Modifiers::NONE,
            });
        }
        h.run_steps(1);
    }
    h.run_steps(8);
}

fn rejected_tap(h: &mut Harness<'_, ThinodaApp>, pos: egui::Pos2, contacts: u64, cancel: bool) {
    h.event(Event::PointerMoved(pos));
    h.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    for phase in [
        TouchPhase::Start,
        if cancel {
            TouchPhase::Cancel
        } else {
            TouchPhase::End
        },
    ] {
        for id in 1..=contacts {
            h.event(Event::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(id),
                phase,
                pos: pos + vec2((id - 1) as f32 * 40.0, 0.0),
                force: None,
            });
        }
        h.run_steps(2);
    }
    h.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
}

#[test]
fn first_touch_swipes_scroll_menus_and_settings_without_activating_controls() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for scale in [1.0, 1.5, 2.0] {
            let mut h = harness(theme, scale);
            h.get_by_role_and_label(Role::Button, t("Export")).click();
            h.run_steps(4);
            let first = t(EXPORT_MENU.first().unwrap().label());
            let button = h.get_by_role_and_label(Role::Button, first).rect();
            let layer = h.ctx.layer_id_at(button.center()).unwrap();
            let popup = h.ctx.memory(|memory| memory.area_rect(layer.id)).unwrap();
            let start = egui::pos2(popup.center().x, popup.bottom() - 24.0);
            let before = h.state().doc().unwrap().doc.workbook().clone();
            let revision = h.state().doc().unwrap().doc.state_id();
            swipe(&mut h, start, vec2(0.0, -90.0));
            assert!(h.get_by_role_and_label(Role::Button, first).rect().top() < button.top());
            assert!(
                egui::Popup::is_any_open(&h.ctx),
                "swipe must not execute an export"
            );
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            let last = t(EXPORT_MENU.last().unwrap().label());
            for _ in 0..20 {
                if popup.contains_rect(h.get_by_role_and_label(Role::Button, last).rect()) {
                    break;
                }
                swipe(&mut h, start, vec2(0.0, -90.0));
            }
            assert!(
                popup.contains_rect(h.get_by_role_and_label(Role::Button, last).rect()),
                "touch scrolling must reach the last export format"
            );
            h.key_press(egui::Key::Escape);
            h.run_steps(4);
            assert!(!egui::Popup::is_any_open(&h.ctx));

            let mut h = harness(theme, scale);
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Settings);
            h.run_steps(4);
            let control = h.get_by_role_and_label(Role::Button, t("General")).rect();
            let window = h
                .ctx
                .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_settings")))
                .unwrap();
            swipe(
                &mut h,
                egui::pos2(window.center().x, window.bottom() - 24.0),
                vec2(0.0, -90.0),
            );
            assert!(
                h.get_by_role_and_label(Role::Button, t("General"))
                    .rect()
                    .top()
                    < control.top()
            );
            assert!(h.state().dialogs.settings);
            assert!(h.state().settings.confirm_on_delete);
            assert!(h.state().ai.test.is_none());
            h.get_by_role_and_label(Role::Switch, t("Confirm before deleting"))
                .scroll_to_me();
            h.run_steps(8);
            let toggle = h
                .get_by_role_and_label(Role::Switch, t("Confirm before deleting"))
                .rect()
                .center();
            for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                rejected_tap(&mut h, toggle, contacts, cancel);
                assert!(
                    h.state().settings.confirm_on_delete,
                    "canceled/multitouch release must not change a Settings switch"
                );
            }
            h.get_by_role_and_label(Role::Switch, t("Confirm before deleting"))
                .click();
            h.run_steps(4);
            assert!(!h.state().settings.confirm_on_delete);
            for (role, label) in [
                (
                    Role::Tab,
                    if theme == ThemeMode::Light {
                        t("Dark")
                    } else {
                        t("Light")
                    },
                ),
                (Role::ColorWell, "#00A89D"),
            ] {
                h.get_by_role_and_label(role, label).scroll_to_me();
                h.run_steps(8);
                let target = h.get_by_role_and_label(role, label).rect().center();
                let accent = h.state().accent;
                for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                    rejected_tap(&mut h, target, contacts, cancel);
                    assert_eq!(h.state().settings.theme, theme);
                    assert_eq!(h.state().accent, accent);
                }
                h.get_by_role_and_label(role, label).focus();
                h.key_press(egui::Key::Space);
                h.run_steps(4);
                if role == Role::Tab {
                    assert_ne!(h.state().settings.theme, theme);
                    h.state_mut().settings.theme = theme;
                    h.run_steps(4);
                } else {
                    assert_eq!(h.state().accent, egui::Color32::from_rgb(0, 0xA8, 0x9D));
                }
            }
            inspector_symbols(&mut harness(theme, scale));
        }
    }
}

fn inspector_symbols(h: &mut Harness<'_, ThinodaApp>) {
    h.state_mut().settings.show_format_panel = true;
    h.state_mut().sidebar_family = SidebarFamily::Symbols;
    h.set_size(vec2(1000.0, 480.0));
    h.run_steps(8);
    let first = thinoda_core::markers::GROUPS
        .first()
        .unwrap()
        .markers
        .first()
        .unwrap();
    h.get_by_role_and_label(Role::Button, t(first.name))
        .scroll_to_me();
    h.run_steps(8);
    let button = h.get_by_role_and_label(Role::Button, t(first.name)).rect();
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let revision = h.state().doc().unwrap().doc.state_id();
    swipe(h, button.center(), vec2(0.0, -70.0));
    assert!(
        h.get_by_role_and_label(Role::Button, t(first.name))
            .rect()
            .top()
            < button.top(),
        "first inspector swipe must scroll rather than apply a symbol"
    );
    assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
    h.get_by_role_and_label(Role::Button, t(first.name))
        .scroll_to_me();
    h.run_steps(8);
    let target = h
        .get_by_role_and_label(Role::Button, t(first.name))
        .rect()
        .center();
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        rejected_tap(h, target, contacts, cancel);
        assert_eq!(
            h.state().doc().unwrap().doc.state_id(),
            revision,
            "rejected inspector release must not apply symbols"
        );
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
    h.get_by_role_and_label(Role::Button, t(first.name)).focus();
    h.key_press(egui::Key::Space);
    h.run_steps(4);
    let doc = h.state().doc().unwrap();
    let root = doc.doc.sheet().root;
    assert!(
        doc.doc
            .sheet()
            .topic(root)
            .unwrap()
            .markers
            .iter()
            .any(|m| m == first.id)
    );
    let placed = doc.doc.workbook().clone();
    let placed_revision = doc.doc.state_id();
    h.get_by_role_and_label(Role::Button, t("Remove Symbol"))
        .scroll_to_me();
    h.run_steps(8);
    let remove = h
        .get_by_role_and_label(Role::Button, t("Remove Symbol"))
        .rect()
        .center();
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        rejected_tap(h, remove, contacts, cancel);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), placed_revision);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &placed);
    }
    h.get_by_role_and_label(Role::Button, t("Remove Symbol"))
        .click();
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &placed);
    let doc = h.state().doc().unwrap();
    let background = doc.view.symbol_background(&doc.doc, root, Some(0));
    h.state_mut().exec(
        &ctx,
        Cmd::Canvas(thinoda_canvas::Action::SetMarkerColor(
            root,
            0,
            Some(background),
        )),
    );
    h.run_steps(4);
    let colored = h.state().doc().unwrap().doc.workbook().clone();
    let colored_revision = h.state().doc().unwrap().doc.state_id();
    for label in ["Default symbol color", "Improve contrast"] {
        h.query_all_by_role_and_label(Role::Button, t(label))
            .last()
            .unwrap()
            .scroll_to_me();
        h.run_steps(8);
        let target = h
            .query_all_by_role_and_label(Role::Button, t(label))
            .last()
            .unwrap()
            .rect()
            .center();
        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            rejected_tap(h, target, contacts, cancel);
            assert_eq!(
                h.state().doc().unwrap().doc.state_id(),
                colored_revision,
                "rejected symbol color action must not change map history"
            );
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &colored);
        }
    }
    h.query_all_by_role_and_label(Role::Button, t("Improve contrast"))
        .last()
        .unwrap()
        .focus();
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_ne!(h.state().doc().unwrap().doc.state_id(), colored_revision);
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &colored);
}
