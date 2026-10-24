use egui::{
    Event, Modifiers, PointerButton, TouchDeviceId, TouchId, TouchPhase, accesskit::Role, vec2,
};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, EXPORT_MENU, FormatTab, Services, SidebarFamily, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

mod support;
use support::menu_action;

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
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
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
    touch_release(h, pos, contacts, cancel, 2);
}

#[test]
fn rejected_settings_actions_preserve_profile_and_network_drafts() {
    use thinoda_app::app::SettingsTab;
    use thinoda_platform::Paths;

    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(root.path());
        paths.ensure().unwrap();
        let mut h = harness(theme, 1.0);
        h.set_size(vec2(1000.0, 900.0));
        h.state_mut().services.paths = Some(paths.clone());
        h.state_mut().save_settings();
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Settings);
        h.run_steps(4);
        let original = h.state().settings.clone();
        let persisted = Settings::load(&paths).unwrap();
        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            for label in ["Autosave", "Keep backup on save", "Confirm before deleting"] {
                let pos = h.get_by_role_and_label(Role::Switch, label).rect().center();
                rejected_tap(&mut h, pos, contacts, cancel);
                assert_eq!(h.state().settings, original, "{label}");
                assert_eq!(Settings::load(&paths).unwrap(), persisted, "{label}");
            }
            for label in ["Editor", "AI Assistant", "Cloud", "Keyboard Shortcuts"] {
                let pos = h.get_by_role_and_label(Role::Button, label).rect().center();
                rejected_tap(&mut h, pos, contacts, cancel);
                assert_eq!(h.state().dialogs.settings_tab, SettingsTab::General);
                assert!(!h.state().dialogs.shortcuts);
            }
        }
        h.get_by_role_and_label(Role::Switch, "Autosave").click();
        h.run_steps(4);
        assert_ne!(h.state().settings.autosave, original.autosave);
        assert_eq!(
            Settings::load(&paths).unwrap().autosave,
            h.state().settings.autosave
        );
        h.get_by_role_and_label(Role::Button, "AI Assistant")
            .click();
        h.run_steps(4);
        h.get_by_role_and_label(Role::DisclosureTriangle, "AI Assistant")
            .click();
        h.run_steps(4);
        h.state_mut().ai.api_key_draft = Some("local-test-draft".into());
        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            for label in ["Save API Key", "Clear", "Test Connection"] {
                let pos = h.get_by_role_and_label(Role::Button, label).rect().center();
                rejected_tap(&mut h, pos, contacts, cancel);
                assert_eq!(
                    h.state().ai.api_key_draft.as_deref(),
                    Some("local-test-draft")
                );
                assert!(h.state().ai.test.is_none());
                assert_eq!(
                    Settings::load(&paths).unwrap().autosave,
                    h.state().settings.autosave
                );
            }
        }
    }
}

#[test]
fn settings_slider_waits_for_release_and_rolls_back_interrupted_gestures() {
    use thinoda_platform::Paths;

    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for interruption in [
            "cancel",
            "multitouch",
            "focus",
            "hidden",
            "modal",
            "profile",
            "clean",
        ] {
            let root = tempfile::tempdir().unwrap();
            let paths = Paths::from_root(root.path());
            paths.ensure().unwrap();
            let mut h = harness(theme, 1.0);
            h.set_size(vec2(1000.0, 900.0));
            h.state_mut().services.paths = Some(paths.clone());
            h.state_mut().save_settings();
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Settings);
            h.run_steps(4);
            let mut original = h.state().settings.clone();
            let mut persisted = Settings::load(&paths).unwrap();
            let slider = h.get_by_role_and_label(Role::Slider, "UI Scale").rect();
            let start = egui::pos2(slider.left() + 10.0, slider.center().y);
            let end = start + vec2(60.0, 0.0);
            h.input_mut().events.extend([
                Event::PointerMoved(start),
                Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase: TouchPhase::Start,
                    pos: start,
                    force: None,
                },
            ]);
            h.run_steps(2);
            h.input_mut().events.extend([
                Event::PointerMoved(end),
                Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase: TouchPhase::Move,
                    pos: end,
                    force: None,
                },
            ]);
            h.run_steps(2);
            assert_eq!(h.state().settings, original, "{interruption}: provisional");
            assert_eq!(Settings::load(&paths).unwrap(), persisted);
            match interruption {
                "multitouch" => {
                    h.input_mut().events.push(Event::Touch {
                        device_id: TouchDeviceId(1),
                        id: TouchId(2),
                        phase: TouchPhase::Start,
                        pos: end + vec2(40.0, 0.0),
                        force: None,
                    });
                    h.run_steps(2);
                    h.input_mut().events.push(Event::Touch {
                        device_id: TouchDeviceId(1),
                        id: TouchId(2),
                        phase: TouchPhase::End,
                        pos: end + vec2(40.0, 0.0),
                        force: None,
                    });
                }
                "focus" => {
                    h.input_mut().events.push(Event::WindowFocused(false));
                    h.run_steps(2);
                }
                "hidden" => {
                    h.state_mut().dialogs.settings = false;
                    h.run_steps(2);
                }
                "modal" => {
                    h.state_mut().dialogs.shortcuts = true;
                    h.run_steps(2);
                }
                "profile" => {
                    h.state_mut()
                        .settings
                        .recent_templates
                        .push("external-change".into());
                    h.state_mut().save_settings();
                    original = h.state().settings.clone();
                    persisted = Settings::load(&paths).unwrap();
                    h.run_steps(2);
                }
                _ => {}
            }
            h.input_mut().events.extend([
                Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase: if interruption == "cancel" {
                        TouchPhase::Cancel
                    } else {
                        TouchPhase::End
                    },
                    pos: end,
                    force: None,
                },
                Event::PointerButton {
                    pos: end,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ]);
            h.run_steps(4);
            if interruption == "clean" {
                assert_ne!(h.state().settings.ui_scale, original.ui_scale);
                assert_eq!(
                    Settings::load(&paths).unwrap().ui_scale,
                    h.state().settings.ui_scale
                );
            } else {
                assert_eq!(h.state().settings, original, "{interruption}");
                assert_eq!(Settings::load(&paths).unwrap(), persisted);
            }
        }
    }
}

fn touch_release(
    h: &mut Harness<'_, ThinodaApp>,
    pos: egui::Pos2,
    contacts: u64,
    cancel: bool,
    hold_steps: usize,
) {
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
        h.run_steps(if phase == TouchPhase::Start {
            hold_steps
        } else {
            2
        });
    }
    h.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
}

fn drag_control_with_touch(
    h: &mut Harness<'_, ThinodaApp>,
    start: egui::Pos2,
    delta: egui::Vec2,
    contacts: u64,
    cancel: bool,
) {
    let revision = h.state().doc().unwrap().doc.state_id();
    h.event(Event::PointerMoved(start));
    h.event(Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.event(Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase: TouchPhase::Start,
        pos: start,
        force: None,
    });
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);

    let moved = start + delta;
    h.event(Event::PointerMoved(moved));
    h.event(Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase: TouchPhase::Move,
        pos: moved,
        force: None,
    });
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().doc.state_id(),
        revision,
        "slider should keep touch changes provisional until release"
    );

    if contacts > 1 {
        for id in 2..=contacts {
            h.event(Event::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(id),
                phase: TouchPhase::Start,
                pos: moved + vec2((id - 1) as f32 * 40.0, 0.0),
                force: None,
            });
        }
        h.run_steps(1);
    }
    for id in 1..=contacts {
        h.event(Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(id),
            phase: if cancel {
                TouchPhase::Cancel
            } else {
                TouchPhase::End
            },
            pos: moved + vec2((id - 1) as f32 * 40.0, 0.0),
            force: None,
        });
    }
    h.run_steps(2);
    if cancel || contacts > 1 {
        assert_eq!(
            h.state().doc().unwrap().doc.state_id(),
            revision,
            "rejected release must discard the provisional slider value"
        );
    }
    h.event(Event::PointerButton {
        pos: moved,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
}

fn drag_control_with_mouse(h: &mut Harness<'_, ThinodaApp>, start: egui::Pos2, delta: egui::Vec2) {
    h.event(Event::PointerMoved(start));
    h.event(Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    let moved = start + delta;
    h.event(Event::PointerMoved(moved));
    h.run_steps(2);
    h.event(Event::PointerButton {
        pos: moved,
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
            inspector_previews(&mut harness(theme, scale));
            sheet_controls(&mut harness(theme, scale));
            status_controls(&mut harness(theme, scale));
        }
    }
}

fn status_controls(h: &mut Harness<'_, ThinodaApp>) {
    h.state_mut().doc_mut().unwrap().view.set_zoom(1.2);
    h.state_mut().doc_mut().unwrap().view.offset = vec2(135.0, -80.0);
    h.run_steps(4);
    let mode = h.state().mode;
    let zoom = h.state().doc().unwrap().view.zoom_percent();
    let offset = h.state().doc().unwrap().view.offset;
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let revision = h.state().doc().unwrap().doc.state_id();
    for label in [
        format!("{}: {}", t("View"), t("Map")),
        t("Zoom Out").into(),
        t("Zoom In").into(),
        t("Fit Map").into(),
        t("Zoom").into(),
        t("Statistics").into(),
    ] {
        h.get_by_role_and_label(Role::Button, &label).scroll_to_me();
        h.run_steps(8);
        let target = h
            .get_by_role_and_label(Role::Button, &label)
            .rect()
            .center();
        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            rejected_tap(h, target, contacts, cancel);
            assert_eq!(h.state().mode, mode, "rejected status control: {label}");
            assert_eq!(
                h.state().doc().unwrap().view.zoom_percent(),
                zoom,
                "rejected zoom control: {label}"
            );
            assert!(
                !egui::Popup::is_any_open(&h.ctx),
                "rejected status menu: {label}"
            );
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            assert_eq!(h.state().doc().unwrap().view.offset, offset);
        }
    }
    h.get_by_role_and_label(Role::Button, t("Zoom"))
        .scroll_to_me();
    h.run_steps(8);
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        h.get_by_role_and_label(Role::Button, t("Zoom")).click();
        h.run_steps(4);
        let target = h.get_by_label("500%").rect().center();
        rejected_tap(h, target, contacts, cancel);
        assert_eq!(h.state().doc().unwrap().view.zoom_percent(), zoom);
        assert_eq!(h.state().doc().unwrap().view.offset, offset);
        h.key_press(egui::Key::Escape);
        h.run_steps(4);
    }
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        h.get_by_role_and_label(Role::Button, t("Zoom")).click();
        h.run_steps(4);
        menu_action(h, t("Fit Map")).scroll_to_me();
        h.run_steps(8);
        let target = menu_action(h, t("Fit Map")).rect().center();
        rejected_tap(h, target, contacts, cancel);
        assert_eq!(h.state().doc().unwrap().view.zoom_percent(), zoom);
        assert_eq!(h.state().doc().unwrap().view.offset, offset);
        h.key_press(egui::Key::Escape);
        h.run_steps(4);
    }
    h.get_by_role_and_label(Role::Button, t("Zoom")).focus();
    h.run_steps(4);
    h.key_press(egui::Key::Space);
    h.run_steps(4);
    h.get_by_label("500%").focus();
    h.run_steps(4);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().view.zoom_percent(), 500);
    h.get_by_role_and_label(Role::Button, t("Zoom Out")).focus();
    h.run_steps(4);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert!(h.state().doc().unwrap().view.zoom_percent() < 500);
    let smaller = h.state().doc().unwrap().view.zoom_percent();
    h.get_by_role_and_label(Role::Button, t("Zoom In"))
        .scroll_to_me();
    h.run_steps(8);
    let target = h
        .get_by_role_and_label(Role::Button, t("Zoom In"))
        .rect()
        .center();
    touch_release(h, target, 1, false, 2);
    assert!(h.state().doc().unwrap().view.zoom_percent() > smaller);
    h.get_by_role_and_label(Role::Button, t("Fit Map")).click();
    h.run_steps(4);
    assert_ne!(h.state().doc().unwrap().view.offset, offset);
    h.get_by_role_and_label(Role::Button, &format!("{}: {}", t("View"), t("Map")))
        .focus();
    h.run_steps(4);
    h.key_press(egui::Key::Space);
    h.run_steps(4);
    assert_ne!(h.state().mode, mode);
    h.get_by_role_and_label(Role::Button, t("Statistics"))
        .focus();
    h.run_steps(8);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    h.get_by_label(t("Current sheet"));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
}

fn sheet_controls(h: &mut Harness<'_, ThinodaApp>) {
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::NewSheet);
    h.run_steps(8);
    let doc = h.state().doc().unwrap();
    let titles: Vec<_> = doc
        .doc
        .workbook()
        .sheets
        .iter()
        .map(|s| s.title.clone())
        .collect();
    let before = doc.doc.workbook().clone();
    let revision = doc.doc.state_id();
    for label in [t("New Sheet"), titles[0].as_str(), titles[1].as_str()] {
        h.get_by_role_and_label(Role::Button, label).scroll_to_me();
        h.run_steps(8);
        let target = h.get_by_role_and_label(Role::Button, label).rect().center();
        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            for _ in 0..2 {
                rejected_tap(h, target, contacts, cancel);
            }
            assert_eq!(
                h.state().doc().unwrap().doc.state_id(),
                revision,
                "rejected sheet control must not create or edit sheets"
            );
            assert_eq!(
                h.state().doc().unwrap().doc.workbook(),
                &before,
                "rejected sheet control must not switch sheets"
            );
            assert!(h.state().dialogs.rename_sheet.is_none());
            assert!(!egui::Popup::is_any_open(&h.ctx));
        }
    }
    h.state_mut().settings.confirm_on_delete = false;
    let tab = h
        .get_by_role_and_label(Role::Button, &titles[1])
        .rect()
        .center();
    for contacts in [2, 3] {
        touch_release(h, tab, contacts, false, 60);
        assert!(
            !egui::Popup::is_any_open(&h.ctx),
            "multitouch hold must not open sheet context actions"
        );
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
    touch_release(h, tab, 1, false, 60);
    assert!(
        egui::Popup::is_any_open(&h.ctx),
        "ordinary single-finger hold must retain sheet context actions"
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    for label in [
        "Rename Sheet",
        "Duplicate Sheet",
        "New Sheet",
        "Move Left",
        "Delete Sheet",
    ] {
        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            let tab = h
                .get_by_role_and_label(Role::Button, &titles[1])
                .rect()
                .center();
            h.event(Event::PointerMoved(tab));
            for pressed in [true, false] {
                h.event(Event::PointerButton {
                    pos: tab,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
                h.run_steps(1);
            }
            h.run_steps(4);
            menu_action(h, t(label)).scroll_to_me();
            h.run_steps(8);
            let target = menu_action(h, t(label)).rect().center();
            rejected_tap(h, target, contacts, cancel);
            assert_eq!(
                h.state().doc().unwrap().doc.state_id(),
                revision,
                "rejected sheet menu action: {label}"
            );
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            assert!(h.state().dialogs.rename_sheet.is_none());
            h.key_press(egui::Key::Escape);
            h.run_steps(4);
        }
    }
    h.get_by_role_and_label(Role::Button, &titles[0]).click();
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 0);
    h.get_by_role_and_label(Role::Button, t("New Sheet"))
        .focus();
    h.key_press(egui::Key::Space);
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 3);
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(4);
    assert_eq!(
        h.state().doc().unwrap().doc.workbook().sheets,
        before.sheets
    );
    for _ in 0..12 {
        h.state_mut().exec(&ctx, Cmd::NewSheet);
    }
    h.run_steps(8);
    let doc = h.state().doc().unwrap();
    let before_scroll = doc.doc.workbook().clone();
    let revision = doc.doc.state_id();
    h.get_by_role_and_label(Role::Button, &titles[0])
        .scroll_to_me();
    h.run_steps(8);
    let tab = h.get_by_role_and_label(Role::Button, &titles[0]).rect();
    let right = h
        .get_by_role_and_label(Role::Button, t("Scroll sheets right"))
        .rect()
        .center();
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        rejected_tap(h, right, contacts, cancel);
        assert_eq!(
            h.get_by_role_and_label(Role::Button, &titles[0]).rect(),
            tab,
            "rejected arrow taps must not scroll the strip"
        );
    }
    h.get_by_role_and_label(Role::Button, t("Scroll sheets right"))
        .focus();
    h.key_press(egui::Key::Enter);
    h.run_steps(8);
    assert!(
        h.get_by_role_and_label(Role::Button, &titles[0])
            .rect()
            .left()
            < tab.left()
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before_scroll);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
}

fn inspector_previews(h: &mut Harness<'_, ThinodaApp>) {
    h.state_mut().settings.show_format_panel = true;
    h.set_size(vec2(1000.0, 700.0));
    h.run_steps(8);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let revision = h.state().doc().unwrap().doc.state_id();
    let label = format!("{}: {}", t("Quick Style"), t("Warning"));
    h.get_by_role_and_label(Role::Button, &label).scroll_to_me();
    h.run_steps(8);
    let target = h
        .get_by_role_and_label(Role::Button, &label)
        .rect()
        .center();
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        rejected_tap(h, target, contacts, cancel);
        assert_eq!(
            h.state().doc().unwrap().doc.state_id(),
            revision,
            "rejected preview release must not apply Quick Style"
        );
    }
    h.get_by_role_and_label(Role::Button, &label).focus();
    h.key_press(egui::Key::Space);
    h.run_steps(4);
    assert_ne!(h.state().doc().unwrap().doc.workbook(), &before);
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.state_mut().format_tab = FormatTab::Map;
    h.run_steps(8);
    let picker = format!("{}: {}", t("Choose Structure"), t("Mind Map"));
    h.get_by_role_and_label(Role::Button, &picker)
        .scroll_to_me();
    h.run_steps(8);
    let launcher = h
        .get_by_role_and_label(Role::Button, &picker)
        .rect()
        .center();
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        rejected_tap(h, launcher, contacts, cancel);
        assert!(
            !egui::Popup::is_any_open(&h.ctx),
            "rejected touch must not open the structure gallery"
        );
    }
    h.get_by_role_and_label(Role::Button, &picker).click();
    h.run_steps(4);
    let label = t("Logic Chart (right)");
    h.get_by_role_and_label(Role::Button, label).scroll_to_me();
    h.run_steps(8);
    let target = h.get_by_role_and_label(Role::Button, label).rect().center();
    let revision = h.state().doc().unwrap().doc.state_id();
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        rejected_tap(h, target, contacts, cancel);
        assert_eq!(
            h.state().doc().unwrap().doc.state_id(),
            revision,
            "rejected preview release must not change map structure"
        );
    }
    h.get_by_role_and_label(Role::Button, label).focus();
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().structure,
        thinoda_core::Structure::LogicRight
    );
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    let label = t("Classic");
    h.get_by_role_and_label(Role::Button, label).scroll_to_me();
    h.run_steps(8);
    let target = h.get_by_role_and_label(Role::Button, label).rect().center();
    let revision = h.state().doc().unwrap().doc.state_id();
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        rejected_tap(h, target, contacts, cancel);
        assert_eq!(
            h.state().doc().unwrap().doc.state_id(),
            revision,
            "rejected preview release must not apply a sheet theme"
        );
    }
    h.get_by_role_and_label(Role::Button, label).click();
    h.run_steps(4);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().theme.theme_id,
        "classic"
    );
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    let count = h.state().docs.len();
    h.state_mut().exec(&ctx, Cmd::Home);
    h.run_steps(8);
    let label = t("Mind Map");
    h.get_by_role_and_label(Role::Button, label).scroll_to_me();
    h.run_steps(8);
    let target = h.get_by_role_and_label(Role::Button, label).rect().center();
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        for _ in 0..2 {
            rejected_tap(h, target, contacts, cancel);
        }
        assert_eq!(
            h.state().docs.len(),
            count,
            "rejected double tap must not create a document"
        );
        assert_eq!(h.state().template_choice, None);
        assert_eq!(h.state().docs[0].doc.workbook(), &before);
    }
    h.get_by_role_and_label(Role::Button, label).focus();
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().template_choice, Some("mind-map"));
    assert_eq!(h.state().docs.len(), count);
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

#[test]
fn style_inspector_combo_rejects_canceled_and_multitouch_releases() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut h = harness(theme, 1.0);
        h.set_size(vec2(1000.0, 520.0));
        h.state_mut().settings.show_format_panel = true;
        h.state_mut().format_tab = FormatTab::Style;
        h.run_steps(8);

        let before = h.state().doc().unwrap().doc.workbook().clone();
        let revision = h.state().doc().unwrap().doc.state_id();
        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            h.get_by_role_and_label(Role::ComboBox, t("Shape"))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::ComboBox, t("Shape")).click();
            h.run_steps(4);
            let target = h.get_by_label(t("Rectangle")).rect().center();
            rejected_tap(&mut h, target, contacts, cancel);
            assert_eq!(
                h.state().doc().unwrap().doc.state_id(),
                revision,
                "rejected touch must not commit an inspector combo selection"
            );
            assert_eq!(
                h.state().doc().unwrap().doc.workbook(),
                &before,
                "rejected touch must preserve the topic style"
            );
        }

        h.get_by_role_and_label(Role::ComboBox, t("Shape"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::ComboBox, t("Shape")).click();
        h.run_steps(4);
        h.get_by_label(t("Rectangle")).click();
        h.run_steps(4);
        let doc = h.state().doc().unwrap();
        let root = doc.doc.sheet().root;
        assert_eq!(
            doc.doc.sheet().topic(root).unwrap().style.shape,
            Some(thinoda_core::TopicShape::Rect),
            "ordinary pointer activation remains available"
        );
    }
}

#[test]
fn style_inspector_slider_discards_canceled_and_multitouch_drags() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut h = harness(theme, 1.0);
        h.set_size(vec2(1000.0, 520.0));
        h.state_mut().settings.show_format_panel = true;
        h.state_mut().format_tab = FormatTab::Style;
        h.run_steps(8);
        h.get_by_role_and_label(Role::Slider, t("Width"))
            .scroll_to_me();
        h.run_steps(8);
        let rect = h.get_by_role_and_label(Role::Slider, t("Width")).rect();
        let start = rect.center();
        let delta = vec2(rect.width() * 0.35, 0.0);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let revision = h.state().doc().unwrap().doc.state_id();

        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            drag_control_with_touch(&mut h, start, delta, contacts, cancel);
            assert_eq!(
                h.state().doc().unwrap().doc.state_id(),
                revision,
                "contacts={contacts}, cancel={cancel}: rejected slider drag must not create an edit"
            );
            assert_eq!(
                h.state().doc().unwrap().doc.workbook(),
                &before,
                "canceled/multitouch slider drag must restore the prior value"
            );
        }

        drag_control_with_touch(&mut h, start, delta, 1, false);
        let touched = h.state().doc().unwrap().doc.workbook().clone();
        assert_ne!(h.state().doc().unwrap().doc.state_id(), revision);
        assert_ne!(touched, before);
        let ctx = h.ctx.clone();
        h.state_mut()
            .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
        h.run_steps(4);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);

        drag_control_with_mouse(&mut h, start, delta);
        assert_ne!(
            h.state().doc().unwrap().doc.workbook(),
            &before,
            "ordinary mouse slider dragging remains available"
        );
    }
}

#[test]
fn style_inspector_numeric_drag_discards_canceled_and_multitouch_input() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut h = harness(theme, 1.0);
        h.set_size(vec2(1000.0, 520.0));
        h.state_mut().settings.show_format_panel = true;
        h.state_mut().format_tab = FormatTab::Style;
        h.run_steps(8);
        h.get_by_role_and_label(Role::SpinButton, t("Font Size"))
            .scroll_to_me();
        h.run_steps(8);
        let rect = h
            .get_by_role_and_label(Role::SpinButton, t("Font Size"))
            .rect();
        let start = rect.center();
        let delta = vec2(rect.width() * 0.4, 0.0);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let revision = h.state().doc().unwrap().doc.state_id();

        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            drag_control_with_touch(&mut h, start, delta, contacts, cancel);
            assert_eq!(
                h.state().doc().unwrap().doc.state_id(),
                revision,
                "canceled/multitouch numeric drag must not create a document edit"
            );
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }

        drag_control_with_touch(&mut h, start, delta, 1, false);
        assert_ne!(h.state().doc().unwrap().doc.state_id(), revision);
        assert_ne!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn map_inspector_switch_rejects_canceled_and_multitouch_releases() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut h = harness(theme, 1.0);
        h.set_size(vec2(1000.0, 520.0));
        h.state_mut().settings.show_format_panel = true;
        h.state_mut().format_tab = FormatTab::Map;
        h.run_steps(8);

        let before = h.state().doc().unwrap().doc.workbook().clone();
        let revision = h.state().doc().unwrap().doc.state_id();
        let rainbow_before = h.state().doc().unwrap().doc.sheet().theme.rainbow;
        h.get_by_role_and_label(Role::Switch, t("Rainbow Branches"))
            .scroll_to_me();
        h.run_steps(8);
        let toggle = h
            .get_by_role_and_label(Role::Switch, t("Rainbow Branches"))
            .rect()
            .center();
        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            rejected_tap(&mut h, toggle, contacts, cancel);
            assert_eq!(
                h.state().doc().unwrap().doc.state_id(),
                revision,
                "rejected Map inspector release must not commit a theme edit"
            );
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().theme.rainbow,
            rainbow_before
        );

        h.get_by_role_and_label(Role::Switch, t("Rainbow Branches"))
            .click();
        h.run_steps(4);
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().theme.rainbow,
            !rainbow_before
        );
        assert_ne!(h.state().doc().unwrap().doc.state_id(), revision);
    }
}

#[test]
fn custom_theme_editor_discards_rejected_preset_activation() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut h = harness(theme, 1.0);
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::EditCustomTheme);
        h.run_steps(8);
        h.get_by_role_and_label(Role::ComboBox, t("Shape"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::ComboBox, t("Shape")).click();
        h.run_steps(4);
        h.get_by_label(t("Rectangle")).click();
        h.run_steps(4);

        h.get_by_label(t("Role preset")).scroll_to_me();
        h.run_steps(8);
        let revision = h.state().doc().unwrap().doc.state_id();
        let before = h.state().doc().unwrap().doc.workbook().clone();
        for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
            h.get_by_label(t("Role preset")).click();
            h.run_steps(4);
            let target = h
                .get_by_role_and_label(Role::Button, t("Warning"))
                .rect()
                .center();
            rejected_tap(&mut h, target, contacts, cancel);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }

        h.get_by_role_and_label(Role::Button, t("Apply Theme"))
            .click();
        h.run_steps(4);
        let doc = h.state().doc().unwrap();
        assert_eq!(doc.doc.state_id(), revision + 1);
        assert_eq!(
            doc.doc
                .sheet()
                .theme
                .custom_theme
                .as_ref()
                .unwrap()
                .roles
                .central
                .shape,
            Some(thinoda_core::TopicShape::Rect),
            "rejected preset activation must not overwrite the staged role style"
        );
    }
}

#[test]
fn export_consent_cannot_follow_a_changed_source() {
    for symbols in [false, true] {
        for change in [
            "active-document",
            "revision",
            "active-sheet",
            "removed-document",
        ] {
            let mut h = harness(ThemeMode::Light, 1.0);
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::New);
            h.state_mut().active = 0;
            h.state_mut().exec(&ctx, Cmd::NewSheet);
            h.state_mut().doc_mut().unwrap().doc.set_active(0);
            if symbols {
                h.state_mut()
                    .doc_mut()
                    .unwrap()
                    .doc
                    .edit_sheet("Symbol color", |sheet| {
                        let topic = sheet.topic_mut(sheet.root).unwrap();
                        topic.markers.push("priority-1".into());
                        topic
                            .marker_colors
                            .insert(0, thinoda_core::Color::rgb(20, 30, 40));
                        Ok(())
                    })
                    .unwrap();
            }
            h.state_mut()
                .exec(&ctx, Cmd::Export(thinoda_io::Format::MindManager));
            h.run_steps(4);
            assert!(
                h.state().dialogs.symbol_export.is_some() || h.state().dialogs.mindmanager_export
            );
            match change {
                "active-document" => h.state_mut().active = 1,
                "removed-document" => {
                    h.state_mut().docs.remove(0);
                }
                "active-sheet" => h.state_mut().doc_mut().unwrap().doc.set_active(1),
                "revision" => {
                    h.state_mut()
                        .doc_mut()
                        .unwrap()
                        .doc
                        .edit_sheet("New work", |sheet| {
                            sheet.set_title(sheet.root, "Changed after consent opened")
                        })
                        .unwrap();
                }
                _ => unreachable!(),
            }
            let before: Vec<_> = h
                .state()
                .docs
                .iter()
                .map(|doc| doc.doc.workbook().clone())
                .collect();
            h.run_steps(4);
            assert!(h.state().dialogs.symbol_export.is_none(), "{change}");
            assert!(!h.state().dialogs.mindmanager_export, "{change}");
            assert!(!h.state().dialogs.mindmanager_symbol_color_loss);
            for (doc, expected) in h.state().docs.iter().zip(before) {
                assert_eq!(doc.doc.workbook(), &expected);
            }
        }
    }
}

#[test]
fn export_consent_stays_with_its_document_after_tab_reordering() {
    use egui_kittest::kittest::NodeT;
    let mut h = harness(ThemeMode::Light, 1.0);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::New);
    h.state_mut().active = 0;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Symbol color", |sheet| {
            let topic = sheet.topic_mut(sheet.root).unwrap();
            topic.markers.push("priority-1".into());
            topic
                .marker_colors
                .insert(0, thinoda_core::Color::rgb(20, 30, 40));
            Ok(())
        })
        .unwrap();
    let key = h.state().doc().unwrap().key.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Export(thinoda_io::Format::MindManager));
    h.run_steps(4);
    assert!(h.state().dialogs.symbol_export.is_some());
    h.state_mut().docs.swap(0, 1);
    h.state_mut().active = 1;
    h.run_steps(4);
    h.query_all_by_role_and_label(Role::Button, t("Export"))
        .find(|node| !node.accesskit_node().is_disabled())
        .unwrap()
        .click();
    h.run_steps(4);
    assert!(h.state().dialogs.symbol_export.is_none());
    assert!(h.state().dialogs.mindmanager_export);
    assert!(h.state().dialogs.mindmanager_symbol_color_loss);
    assert_eq!(h.state().doc().unwrap().key, key);
    h.get_by_role_and_label(Role::Button, t("Cancel")).click();
    h.run_steps(4);
    assert!(!h.state().dialogs.mindmanager_export);
    assert!(!h.state().dialogs.mindmanager_symbol_color_loss);
}

#[test]
fn label_popup_rejects_canceled_and_multitouch_actions() {
    use egui_kittest::kittest::NodeT;
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for label in [
            "Apply",
            "Cancel",
            "Add labels",
            "Auto sort",
            "Remove label: Existing",
            "Use label: Keep this label",
            "backdrop",
        ] {
            let mut h = harness(theme, 1.0);
            let root = h.state().doc().unwrap().doc.sheet().root;
            h.state_mut()
                .doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Initial labels", |sheet| {
                    let child = sheet.children(root)[0];
                    sheet.set_labels(root, vec!["Existing".into()])?;
                    sheet.set_labels(child, vec!["Keep this label".into()])
                })
                .unwrap();
            h.state_mut().doc_mut().unwrap().view.selection = vec![root];
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Labels);
            h.run_steps(4);
            h.get_by_label(t("New labels")).type_text("Keep this label");
            h.run_steps(4);
            let before = h.state().doc().unwrap().doc.workbook().clone();
            let revision = h.state().doc().unwrap().doc.state_id();
            let sort = h.get_by_label(t("Auto sort")).accesskit_node().toggled();
            for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                let target = if label == "backdrop" {
                    egui::pos2(5.0, 5.0)
                } else {
                    h.get_by_label(t(label)).rect().center()
                };
                rejected_tap(&mut h, target, contacts, cancel);
                assert_eq!(
                    h.get_by_label(t("New labels")).value().as_deref(),
                    Some("Keep this label")
                );
                assert_eq!(
                    h.get_by_label(t("Auto sort")).accesskit_node().toggled(),
                    sort
                );
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            }
            h.get_by_label(t("Apply")).click();
            h.run_steps(4);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(root)
                    .unwrap()
                    .labels,
                ["Existing", "Keep this label"]
            );
            assert!(h.state_mut().doc_mut().unwrap().doc.undo());
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }
    }
}

#[test]
fn sheet_rename_rejects_canceled_and_multitouch_decisions() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for choice in ["OK", "Cancel"] {
            let mut h = harness(theme, 1.0);
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::RenameSheet);
            h.run_steps(4);
            h.get_by_role_and_label(Role::TextInput, t("Rename Sheet"))
                .type_text(" Draft");
            h.run_steps(4);
            let name = h
                .get_by_role_and_label(Role::TextInput, t("Rename Sheet"))
                .value()
                .unwrap();
            let before = h.state().doc().unwrap().doc.workbook().clone();
            let revision = h.state().doc().unwrap().doc.state_id();
            let target = h
                .get_by_role_and_label(Role::Button, t(choice))
                .rect()
                .center();
            for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                rejected_tap(&mut h, target, contacts, cancel);
                assert!(h.state().dialogs.rename_sheet.is_some());
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
                assert_eq!(
                    h.get_by_role_and_label(Role::TextInput, t("Rename Sheet"))
                        .value()
                        .unwrap(),
                    name
                );
            }
            touch_release(&mut h, target, 1, false, 2);
            assert!(h.state().dialogs.rename_sheet.is_none());
            if choice == "OK" {
                assert_eq!(h.state().doc().unwrap().doc.sheet().title, name);
                assert!(h.state_mut().doc_mut().unwrap().doc.undo());
            }
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }
    }
}

#[test]
fn sheet_rename_cannot_follow_a_changed_document_or_revision() {
    for change in [
        "active-document",
        "removed-document",
        "revision",
        "active-sheet",
    ] {
        let mut h = harness(ThemeMode::Light, 1.0);
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::New);
        h.state_mut().active = 0;
        if change == "active-sheet" {
            h.state_mut().exec(&ctx, Cmd::NewSheet);
            h.state_mut().doc_mut().unwrap().doc.set_active(0);
        }
        h.state_mut().exec(&ctx, Cmd::RenameSheet);
        h.run_steps(4);
        h.get_by_role_and_label(Role::TextInput, t("Rename Sheet"))
            .type_text(" Stale");
        h.run_steps(4);
        let target = h
            .get_by_role_and_label(Role::Button, t("OK"))
            .rect()
            .center();
        match change {
            "active-document" => h.state_mut().active = 1,
            "removed-document" => {
                h.state_mut().docs.remove(0);
            }
            "revision" => {
                h.state_mut()
                    .doc_mut()
                    .unwrap()
                    .doc
                    .edit_sheet("External edit", |sheet| {
                        sheet.set_title(sheet.root, "Newer work")
                    })
                    .unwrap();
            }
            "active-sheet" => h.state_mut().doc_mut().unwrap().doc.set_active(1),
            _ => unreachable!(),
        }
        let before: Vec<_> = h
            .state()
            .docs
            .iter()
            .map(|doc| doc.doc.workbook().clone())
            .collect();
        touch_release(&mut h, target, 1, false, 2);
        assert!(h.state().dialogs.rename_sheet.is_none());
        for (doc, expected) in h.state().docs.iter().zip(before) {
            assert_eq!(doc.doc.workbook(), &expected, "stale rename after {change}");
        }
    }
}

#[test]
fn recovery_choices_reject_canceled_and_multitouch_releases() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for choice in ["Restore", "Restore All", "Discard All"] {
            let dir = tempfile::tempdir().unwrap();
            let paths = thinoda_platform::Paths::from_root(dir.path());
            let recovery = thinoda_platform::recovery::RecoveryManager::start(&paths).unwrap();
            let mut h = harness(theme, 1.0);
            let doc = h.state_mut().doc_mut().unwrap();
            doc.doc
                .edit_sheet("Unsaved", |sheet| {
                    sheet.set_title(sheet.root, "Recovery must survive")
                })
                .unwrap();
            let expected = doc.doc.workbook().clone();
            doc.snapshot(&recovery);
            assert!(recovery.try_flush_for_crash());
            recovery.suspend().unwrap();
            drop(recovery);
            let current = std::sync::Arc::new(
                thinoda_platform::recovery::RecoveryManager::start(&paths).unwrap(),
            );
            h.state_mut().services.recovery = Some(current);
            h.state_mut().docs.clear();
            h.state_mut().dialogs.recovery =
                thinoda_platform::recovery::RecoveryManager::find_orphans(&paths);
            assert_eq!(h.state().dialogs.recovery.len(), 1);
            h.run_steps(4);
            let target = h
                .get_by_role_and_label(Role::Button, t(choice))
                .rect()
                .center();
            for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                rejected_tap(&mut h, target, contacts, cancel);
                assert_eq!(
                    h.state().dialogs.recovery.len(),
                    1,
                    "rejected {choice} preserves recovery"
                );
                assert!(h.state().docs.is_empty());
                assert_eq!(
                    h.state().dialogs.recovery[0].docs[0].load().unwrap(),
                    expected
                );
            }
            touch_release(&mut h, target, 1, false, 2);
            assert!(h.state().dialogs.recovery.is_empty());
            if choice == "Discard All" {
                assert!(h.state().docs.is_empty());
            } else {
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &expected);
                assert!(h.state().doc().unwrap().is_dirty());
            }
            assert!(thinoda_platform::recovery::RecoveryManager::find_orphans(&paths).is_empty());
        }
    }
}

#[test]
fn library_gallery_buttons_reject_canceled_and_multitouch_releases() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for custom_theme in [false, true] {
            let mut h = harness(theme, 1.0);
            h.set_size(vec2(1280.0, 1000.0));
            h.state_mut().settings.show_format_panel = true;
            let doc = h.state_mut().doc_mut().unwrap();
            doc.view.selection = vec![doc.doc.sheet().root];
            h.state_mut().format_tab = if custom_theme {
                FormatTab::Map
            } else {
                FormatTab::Style
            };
            if custom_theme {
                let mut saved = thinoda_core::custom_theme::CustomTheme::from_settings(
                    &h.state().doc().unwrap().doc.sheet().theme,
                );
                saved.name = "Saved theme".into();
                h.state_mut().settings.custom_themes.push(saved);
            } else {
                let mut saved = thinoda_core::quick_styles::preset("Warning").unwrap();
                saved.name = "Saved style".into();
                h.state_mut().settings.quick_styles.push(saved);
            }
            h.run_steps(8);
            let before = h.state().doc().unwrap().doc.workbook().clone();
            let revision = h.state().doc().unwrap().doc.state_id();
            let labels: &[&str] = if custom_theme {
                &["Edit Custom Theme…", "Edit…", "Remove…"]
            } else {
                &["Save Quick Style…", "Remove…"]
            };
            for label in labels {
                h.get_by_role_and_label(Role::Button, t(label))
                    .scroll_to_me();
                h.run_steps(8);
                let target = h
                    .get_by_role_and_label(Role::Button, t(label))
                    .rect()
                    .center();
                for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                    rejected_tap(&mut h, target, contacts, cancel);
                    assert!(
                        h.state().command_available(&Cmd::Palette),
                        "rejected {label} cannot open a modal"
                    );
                    assert_eq!(
                        h.state().settings.custom_themes.len(),
                        usize::from(custom_theme)
                    );
                    assert_eq!(
                        h.state().settings.quick_styles.len(),
                        usize::from(!custom_theme)
                    );
                    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                    assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
                }
                h.get_by_role_and_label(Role::Button, t(label)).click();
                h.run_steps(4);
                assert!(
                    !h.state().command_available(&Cmd::Palette),
                    "clean {label} opens a modal"
                );
                h.key_press(egui::Key::Escape);
                h.run_steps(4);
            }
        }
    }
}

#[test]
fn quick_style_save_dialog_rejects_canceled_and_multitouch_releases() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for choice in ["Save", "Cancel", "backdrop"] {
            let dir = tempfile::tempdir().unwrap();
            let paths = thinoda_platform::Paths::from_root(dir.path());
            let mut h = harness(theme, 1.0);
            h.state().settings.save(&paths).unwrap();
            h.state_mut().services.paths = Some(paths.clone());
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::SaveQuickStyle);
            h.run_steps(4);
            h.get_by_label(t("Style name")).click();
            h.get_by_label(t("Style name")).type_text("Keep my draft");
            h.run_steps(4);
            let before = h.state().doc().unwrap().doc.workbook().clone();
            let revision = h.state().doc().unwrap().doc.state_id();
            let target = if choice == "backdrop" {
                egui::pos2(5.0, 5.0)
            } else {
                h.get_by_role_and_label(Role::Button, t(choice))
                    .rect()
                    .center()
            };
            for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                rejected_tap(&mut h, target, contacts, cancel);
                assert!(h.state().settings.quick_styles.is_empty());
                assert!(
                    thinoda_platform::Settings::load(&paths)
                        .unwrap()
                        .quick_styles
                        .is_empty()
                );
                assert!(!h.state().command_available(&Cmd::Palette));
                assert_eq!(
                    h.get_by_label(t("Style name")).value().as_deref(),
                    Some("Keep my draft")
                );
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            }
            match choice {
                "Save" => h.get_by_role_and_label(Role::Button, t("Save")).click(),
                "Cancel" => {
                    h.get_by_label(t("Style name")).click();
                    h.key_press(egui::Key::Enter);
                }
                "backdrop" => {
                    let target = h
                        .get_by_role_and_label(Role::Button, t("Save"))
                        .rect()
                        .center();
                    touch_release(&mut h, target, 1, false, 2);
                }
                _ => unreachable!(),
            }
            h.run_steps(4);
            assert_eq!(h.state().settings.quick_styles[0].name, "Keep my draft");
            assert_eq!(
                thinoda_platform::Settings::load(&paths)
                    .unwrap()
                    .quick_styles,
                h.state().settings.quick_styles
            );
            assert!(h.state().command_available(&Cmd::Palette));
        }
    }
}

#[test]
fn conflict_confirmation_rejects_canceled_and_multitouch_overwrites() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conflict.thinoda");
        let mut h = harness(theme, 1.0);
        h.state_mut()
            .doc_mut()
            .unwrap()
            .save_to(&path, false)
            .unwrap();
        h.state_mut()
            .doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Edit", |sheet| sheet.set_title(sheet.root, "Local changes"))
            .unwrap();
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let revision = h.state().doc().unwrap().doc.state_id();
        let external = b"newer external bytes";
        std::fs::write(&path, external).unwrap();
        h.state_mut().save_active(false);
        h.run_steps(4);
        let key = h.state().doc().unwrap().key.clone();
        assert_eq!(h.state().dialogs.conflict.as_ref(), Some(&key));
        for choice in ["Overwrite", "Save As Copy", "Cancel"] {
            let target = h
                .get_by_role_and_label(Role::Button, t(choice))
                .rect()
                .center();
            for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                rejected_tap(&mut h, target, contacts, cancel);
                assert_eq!(h.state().dialogs.conflict.as_ref(), Some(&key));
                assert!(!h.state().doc().unwrap().ignore_conflict);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
                assert!(h.state().doc().unwrap().is_dirty());
                assert_eq!(std::fs::read(&path).unwrap(), external);
            }
        }
        let target = h
            .get_by_role_and_label(Role::Button, t("Overwrite"))
            .rect()
            .center();
        touch_release(&mut h, target, 1, false, 2);
        assert!(h.state().dialogs.conflict.is_none());
        assert!(!h.state().doc().unwrap().is_dirty());
        assert_eq!(
            thinoda_app::doc::OpenDoc::open(&path)
                .unwrap()
                .doc
                .workbook(),
            &before
        );
    }
}

#[test]
fn close_confirmation_rejects_canceled_and_multitouch_choices() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for (choice, input) in ["Save", "Don't Save", "Cancel"]
            .into_iter()
            .flat_map(|choice| ["keyboard", "mouse", "touch"].map(|input| (choice, input)))
        {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("close.thinoda");
            let mut h = harness(theme, 1.0);
            h.state_mut()
                .doc_mut()
                .unwrap()
                .save_to(&path, false)
                .unwrap();
            h.state_mut()
                .doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Edit", |sheet| {
                    sheet.set_title(sheet.root, "Keep this work")
                })
                .unwrap();
            let original = std::fs::read(&path).unwrap();
            let before = h.state().doc().unwrap().doc.workbook().clone();
            let revision = h.state().doc().unwrap().doc.state_id();
            let key = h.state().doc().unwrap().key.clone();
            h.state_mut().request_close_tab(0);
            h.run_steps(4);
            let target = h
                .get_by_role_and_label(Role::Button, t(choice))
                .rect()
                .center();
            for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                rejected_tap(&mut h, egui::pos2(5.0, 5.0), contacts, cancel);
                assert!(
                    h.state().dialogs.close.is_some(),
                    "rejected backdrop cannot dismiss"
                );
                rejected_tap(&mut h, target, contacts, cancel);
                assert_eq!(h.state().docs.len(), 1, "rejected {choice} cannot close");
                assert_eq!(
                    h.state().dialogs.close,
                    Some(thinoda_app::app::CloseIntent::Tab(key.clone())),
                    "rejected {choice} cannot resolve the prompt"
                );
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
                assert!(h.state().doc().unwrap().is_dirty());
                assert_eq!(std::fs::read(&path).unwrap(), original);
            }
            match input {
                "keyboard" => {
                    h.get_by_role_and_label(Role::Button, t(choice)).focus();
                    h.key_press(egui::Key::Space);
                }
                "mouse" => h.get_by_role_and_label(Role::Button, t(choice)).click(),
                "touch" => touch_release(&mut h, target, 1, false, 2),
                _ => unreachable!(),
            }
            h.run_steps(4);
            assert!(h.state().dialogs.close.is_none());
            if choice == "Cancel" {
                assert_eq!(h.state().docs.len(), 1);
                assert!(h.state().doc().unwrap().is_dirty());
            } else {
                assert!(h.state().docs.is_empty());
            }
            if choice == "Save" {
                assert_eq!(
                    thinoda_app::doc::OpenDoc::open(&path)
                        .unwrap()
                        .doc
                        .workbook(),
                    &before
                );
            } else {
                assert_eq!(std::fs::read(&path).unwrap(), original);
            }
        }
    }
}

#[test]
fn custom_theme_numeric_drag_restores_the_entire_rejected_gesture() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for (contacts, cancel, lose_focus) in [
            (1, true, false),
            (2, false, false),
            (3, false, false),
            (1, false, false),
            (0, false, false),
            (0, false, true),
        ] {
            let mut h = harness(theme, 1.0);
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::EditCustomTheme);
            h.run_steps(8);
            h.get_by_role_and_label(Role::TextInput, t("Theme name"))
                .focus();
            h.get_by_role_and_label(Role::TextInput, t("Theme name"))
                .type_text(" Keyboard");
            h.run_steps(4);
            h.get_by_role_and_label(Role::CheckBox, t("Override Font Size"))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::CheckBox, t("Override Font Size"))
                .click();
            h.run_steps(4);
            h.get_by_role_and_label(Role::SpinButton, t("Override Font Size"))
                .scroll_to_me();
            h.run_steps(8);
            let control = h.get_by_role_and_label(Role::SpinButton, t("Override Font Size"));
            let before = control.value().unwrap().to_owned();
            let rect = control.rect();
            let workbook = h.state().doc().unwrap().doc.workbook().clone();
            let revision = h.state().doc().unwrap().doc.state_id();
            if lose_focus {
                let start = rect.center();
                let moved = start + vec2(80.0, 0.0);
                h.event(Event::PointerMoved(start));
                h.event(Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                });
                h.run_steps(2);
                h.event(Event::PointerMoved(moved));
                h.run_steps(2);
                h.event(Event::WindowFocused(false));
                h.run_steps(2);
                h.event(Event::WindowFocused(true));
                h.event(Event::PointerButton {
                    pos: moved,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
                h.run_steps(4);
            } else if contacts == 0 {
                drag_control_with_mouse(&mut h, rect.center(), vec2(80.0, 0.0));
            } else {
                drag_control_with_touch(&mut h, rect.center(), vec2(80.0, 0.0), contacts, cancel);
            }
            let after = h
                .get_by_role_and_label(Role::SpinButton, t("Override Font Size"))
                .value()
                .unwrap()
                .to_owned();
            if cancel || contacts > 1 || lose_focus {
                assert_eq!(after, before, "rejected drag must restore earlier frames");
            } else {
                assert_ne!(after, before, "clean touch drag must remain usable");
            }
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &workbook);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            h.get_by_role_and_label(Role::Button, t("Save to Library"))
                .click();
            h.run_steps(4);
            assert_eq!(h.state().settings.custom_themes.len(), 1);
            assert!(
                h.state().settings.custom_themes[0]
                    .name
                    .contains("Keyboard")
            );
            h.get_by_role_and_label(Role::Button, t("Apply Theme"))
                .click();
            h.run_steps(4);
            let doc = h.state().doc().unwrap();
            assert_eq!(
                doc.doc.sheet().theme.custom_theme.as_ref(),
                h.state().settings.custom_themes.first()
            );
            assert_eq!(doc.doc.state_id(), revision + 1);
            let size = doc
                .doc
                .sheet()
                .theme
                .custom_theme
                .as_ref()
                .unwrap()
                .roles
                .central
                .font_size
                .unwrap();
            let before: f32 = before.parse().unwrap();
            if cancel || contacts > 1 || lose_focus {
                assert_eq!(size, before);
            } else {
                assert_ne!(size, before);
            }
            h.state_mut()
                .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
            h.run_steps(4);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &workbook);
        }
    }
}

#[test]
fn custom_theme_color_drag_restores_rejected_gesture() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for (contacts, cancel) in [(1, true), (2, false), (3, false), (1, false)] {
            let mut h = harness(theme, 1.0);
            let before = thinoda_core::custom_theme::CustomTheme::from_settings(
                &h.state().doc().unwrap().doc.sheet().theme,
            );
            let workbook = h.state().doc().unwrap().doc.workbook().clone();
            let revision = h.state().doc().unwrap().doc.state_id();
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::EditCustomTheme);
            h.run_steps(8);
            h.get_by_role_and_label(Role::ColorWell, t("Light background"))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::ColorWell, t("Light background"))
                .click();
            h.run_steps(4);
            let rect = h
                .query_all_by_role(Role::SpinButton)
                .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
                .expect("color picker red channel")
                .rect();
            drag_control_with_touch(&mut h, rect.center(), vec2(-80.0, 0.0), contacts, cancel);
            h.key_press(egui::Key::Escape);
            h.run_steps(4);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &workbook);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            h.get_by_role_and_label(Role::Button, t("Save to Library"))
                .click();
            h.run_steps(4);
            let saved = &h.state().settings.custom_themes[0];
            if cancel || contacts > 1 {
                assert_eq!(
                    saved, &before,
                    "canceled picker drag must restore all color changes"
                );
            } else {
                assert_ne!(saved.light_background, before.light_background);
            }
            h.get_by_role_and_label(Role::Button, t("Apply Theme"))
                .click();
            h.run_steps(4);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .theme
                    .custom_theme
                    .as_ref(),
                h.state().settings.custom_themes.first()
            );
            h.state_mut()
                .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
            h.run_steps(4);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &workbook);
        }
    }
}

#[test]
fn numeric_drag_drafts_cannot_outlive_their_inspector_context() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for interruption in [
            "document",
            "sheet",
            "selection",
            "revision",
            "hidden",
            "modal",
        ] {
            let mut h = harness(theme, 1.0);
            h.set_size(vec2(1000.0, 520.0));
            h.state_mut().settings.show_format_panel = true;
            h.state_mut().format_tab = FormatTab::Style;
            let workbook = h.state().doc().unwrap().doc.workbook().clone();
            h.state_mut().new_doc(workbook);
            h.state_mut().active = 0;
            let sheet = h.state().doc().unwrap().doc.sheet().clone();
            h.state_mut().doc_mut().unwrap().doc.add_sheet(sheet);
            h.state_mut().doc_mut().unwrap().doc.set_active(0);
            h.run_steps(8);
            h.get_by_role_and_label(Role::Slider, t("Width"))
                .scroll_to_me();
            h.run_steps(8);
            let rect = h.get_by_role_and_label(Role::Slider, t("Width")).rect();
            let start = rect.center();
            let moved = start + vec2(rect.width() * 0.35, 0.0);
            let initial_revision = h.state().doc().unwrap().doc.state_id();
            h.event(Event::PointerMoved(start));
            h.event(Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(2);
            h.event(Event::PointerMoved(moved));
            h.run_steps(2);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), initial_revision);

            match interruption {
                "document" => h.state_mut().active = 1,
                "sheet" => h.state_mut().doc_mut().unwrap().doc.set_active(1),
                "selection" => {
                    let doc = h.state_mut().doc_mut().unwrap();
                    let child = doc.doc.sheet().children(doc.doc.sheet().root)[0];
                    doc.view.selection = vec![child];
                }

                "revision" => {
                    let doc = h.state_mut().doc_mut().unwrap();
                    doc.doc
                        .edit_sheet("External edit", |sheet| {
                            sheet.set_title(sheet.root, "Changed during drag")
                        })
                        .unwrap();
                }
                "hidden" => h.state_mut().settings.show_format_panel = false,
                "modal" => {
                    let ctx = h.ctx.clone();
                    h.state_mut().exec(&ctx, Cmd::Settings);
                }
                _ => unreachable!(),
            }
            h.run_steps(3);
            let before: Vec<_> = h
                .state()
                .docs
                .iter()
                .map(|doc| (doc.doc.workbook().clone(), doc.doc.state_id()))
                .collect();
            h.event(Event::PointerButton {
                pos: moved,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(4);
            h.state_mut().dialogs.settings = false;
            h.state_mut().settings.show_format_panel = true;
            h.state_mut().active = 0;
            h.state_mut().doc_mut().unwrap().doc.set_active(0);
            let root = h.state().doc().unwrap().doc.sheet().root;
            h.state_mut().doc_mut().unwrap().view.selection = vec![root];
            h.run_steps(8);
            for (doc, (workbook, revision)) in h.state().docs.iter().zip(&before) {
                assert_eq!(doc.doc.workbook().sheets, workbook.sheets, "{interruption}");
                assert_eq!(
                    doc.doc.workbook().metadata,
                    workbook.metadata,
                    "{interruption}"
                );
                assert_eq!(doc.doc.state_id(), *revision, "{interruption}");
            }
        }
    }
}
