use egui::{Event, Modifiers, PointerButton, TouchDeviceId, TouchId, TouchPhase, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use std::path::PathBuf;
use thinoda_app::{
    app::{Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::Workbook;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn harness(size: egui::Vec2) -> Harness<'static, ThinodaApp> {
    let mut app = ThinodaApp::new(
        Settings {
            language: Language::English,
            theme: ThemeMode::Dark,
            show_format_panel: false,
            show_topic_toolbar: false,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    for index in 0..20 {
        app.new_doc(Workbook::default());
        app.docs.last_mut().unwrap().path = Some(PathBuf::from(format!(
            "File {index}: a long document name.thinoda"
        )));
    }
    app.active = 0;
    let mut h = Harness::builder()
        .with_size(size)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h
}

#[test]
fn document_tabs_scroll_without_switching_and_keep_actions_pinned() {
    let mut h = harness(egui::vec2(720.0, 480.0));
    let contents: Vec<_> = h
        .state()
        .docs
        .iter()
        .map(|doc| (doc.doc.workbook().clone(), doc.doc.state_id()))
        .collect();
    let first = "File 0: a long document name";
    let last = "File 19: a long document name";
    let left = t("Scroll documents left");
    let right = t("Scroll documents right");
    let new_tab = t("New Tab");
    let close_tab = t("Close Tab");

    assert!(
        h.get_by_role_and_label(Role::Button, left)
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        !h.get_by_role_and_label(Role::Button, right)
            .accesskit_node()
            .is_disabled()
    );
    let first_before = h.get_by_role_and_label(Role::Button, first).rect();
    h.get_by_role_and_label(Role::Button, right).click();
    h.run_steps(8);
    assert!(
        h.get_by_role_and_label(Role::Button, first).rect().left() < first_before.left(),
        "right arrow should scroll the document names"
    );
    assert_eq!(h.state().active, 0, "browsing must not switch documents");

    for _ in 0..100 {
        let arrow = h.get_by_role_and_label(Role::Button, right);
        if arrow.accesskit_node().is_disabled() {
            break;
        }
        arrow.click();
        h.run_steps(3);
    }
    assert!(
        h.get_by_role_and_label(Role::Button, right)
            .accesskit_node()
            .is_disabled()
    );
    let last_rect = h.get_by_role_and_label(Role::Button, last).rect();
    let right_rect = h.get_by_role_and_label(Role::Button, right).rect();
    let close_rect = h.get_by_role_and_label(Role::Button, close_tab).rect();
    let new_rect = h.get_by_role_and_label(Role::Button, new_tab).rect();
    assert!(last_rect.left() >= right_rect.right());
    assert!(last_rect.right() <= close_rect.left());
    assert!(close_rect.right() <= new_rect.left());
    for rect in [right_rect, close_rect, new_rect] {
        assert!(h.ctx.content_rect().contains_rect(rect));
    }
    assert_eq!(h.state().active, 0);

    for _ in 0..100 {
        let arrow = h.get_by_role_and_label(Role::Button, left);
        if arrow.accesskit_node().is_disabled() {
            break;
        }
        arrow.click();
        h.run_steps(3);
    }
    let first_before_swipe = h.get_by_role_and_label(Role::Button, first).rect();
    let swipe_start = first_before_swipe.center();
    let moved = swipe_start - egui::vec2(80.0, 0.0);
    for (phase, pos) in [
        (TouchPhase::Start, swipe_start),
        (TouchPhase::Move, swipe_start - egui::vec2(12.0, 0.0)),
        (TouchPhase::Move, moved),
    ] {
        h.event(Event::PointerMoved(pos));
        h.event(Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        });
        if phase == TouchPhase::Start {
            h.event(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
        }
        h.run_steps(1);
    }
    h.event(Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase: TouchPhase::End,
        pos: moved,
        force: None,
    });
    h.event(Event::PointerButton {
        pos: moved,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(8);
    assert!(h.get_by_role_and_label(Role::Button, first).rect().left() < first_before_swipe.left());
    assert_eq!(h.state().active, 0);

    h.get_by_role_and_label(Role::Button, last).focus();
    h.run_steps(4);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().active, 19);
    assert_eq!(h.state().docs.len(), 20);
    for (doc, (workbook, state_id)) in h.state().docs.iter().zip(contents.iter()) {
        assert_eq!(doc.doc.workbook(), workbook);
        assert_eq!(doc.doc.state_id(), *state_id);
    }

    for _ in 0..100 {
        let arrow = h.get_by_role_and_label(Role::Button, left);
        if arrow.accesskit_node().is_disabled() {
            break;
        }
        arrow.click();
        h.run_steps(3);
    }
    h.get_by_role_and_label(Role::Button, first).click();
    h.run_steps(4);
    assert_eq!(h.state().active, 0, "pointer selection switches documents");
    h.get_by_role_and_label(Role::Button, last).focus();
    h.run_steps(4);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().active, 19);
    h.get_by_role_and_label(Role::Button, close_tab).click();
    h.run_steps(8);
    assert_eq!(
        h.state().docs.len(),
        19,
        "pinned Close Tab closes the active file"
    );
    for (doc, (workbook, state_id)) in h.state().docs.iter().zip(contents.iter().take(19)) {
        assert_eq!(doc.doc.workbook(), workbook);
        assert_eq!(doc.doc.state_id(), *state_id);
    }

    h.get_by_role_and_label(Role::Button, new_tab).click();
    h.run_steps(4);
    assert_eq!(h.state().active, 19, "New Tab remains pinned at the end");
    assert_eq!(h.state().docs.len(), 19);
}

#[test]
fn document_tabs_keep_navigation_controls_reachable_in_narrow_windows() {
    let h = harness(egui::vec2(320.0, 240.0));
    for label in [
        t("Scroll documents left"),
        t("Scroll documents right"),
        t("Close Tab"),
        t("New Tab"),
    ] {
        let rect = h.get_by_role_and_label(Role::Button, label).rect();
        assert!(
            h.ctx.content_rect().contains_rect(rect),
            "{label}: {rect:?} outside {:?}",
            h.ctx.content_rect()
        );
    }
    assert_eq!(h.state().active, 0);
    assert_eq!(h.state().docs.len(), 20);
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
                pos: pos + egui::vec2((id - 1) as f32 * 40.0, 0.0),
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
fn topbar_view_action_rejects_canceled_and_multitouch_releases() {
    let mut h = harness(egui::vec2(720.0, 480.0));
    let initial = h.state().mode;
    let outliner = t("Outliner");
    h.get_by_role_and_label(Role::Button, outliner)
        .scroll_to_me();
    h.run_steps(4);
    let target = h
        .get_by_role_and_label(Role::Button, outliner)
        .rect()
        .center();
    for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
        rejected_tap(&mut h, target, contacts, cancel);
        assert_eq!(h.state().mode, initial);
    }
    h.get_by_role_and_label(Role::Button, outliner).click();
    h.run_steps(4);
    assert_ne!(
        h.state().mode,
        initial,
        "ordinary click still switches view"
    );
}
