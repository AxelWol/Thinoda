use super::*;
use egui::accesskit::Role;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

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
fn ai_ribbon_keeps_settings_available_when_generation_is_disabled() {
    let mut h = harness(1800.0, 700.0);
    h.state_mut().settings.ai.provider = thinoda_platform::settings::AiProvider::None;
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("AI")).click();
    h.run_steps(3);
    let brainstorm = h.get_by_role_and_label(Role::Button, t("AI: Brainstorm Subtopics"));
    assert!(brainstorm.accesskit_node().is_disabled());
    let settings = h.get_by_role_and_label(Role::Button, t("AI settings"));
    assert!(!settings.accesskit_node().is_disabled());
    settings.click();
    h.run_steps(3);
    assert!(h.state().dialogs.settings);
    assert_eq!(h.state().dialogs.settings_tab, crate::app::SettingsTab::Ai);
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

#[cfg(target_os = "windows")]
#[test]
fn desktop_document_tabs_keep_new_tab_clear_of_quick_access_and_header_actions() {
    let mut app = ThinodaApp::headless();
    app.desktop_ribbon = true;
    for index in 0..15 {
        if index > 0 {
            app.new_doc(thinoda_core::Workbook::default());
        }
        app.docs.last_mut().unwrap().path = Some(std::path::PathBuf::from(format!(
            "Document {index} with a long title.thinoda"
        )));
    }
    app.active = 0;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    for mode in [
        crate::app::CenterMode::Map,
        crate::app::CenterMode::Outliner,
    ] {
        h.state_mut().mode = mode;
        for (width, scale) in [(720.0, 1.0), (640.0, 1.0), (360.0, 2.0), (1200.0, 1.0)] {
            h.ctx.set_zoom_factor(scale);
            h.set_size(egui::vec2(width, 480.0));
            h.run_steps(4);
            let add = h.get_by_role_and_label(Role::Button, t("New Tab")).rect();
            let quick_access = h
                .get_by_role_and_label(Role::Button, t("Quick Access Toolbar"))
                .rect();
            if h.ctx.content_rect().width() < 400.0 {
                assert_eq!(
                    h.query_by_role_and_label(Role::Button, t("Undo")).is_some(),
                    mode == crate::app::CenterMode::Map,
                    "{mode:?}/{width}/{scale}: unexpected Quick Access fallback"
                );
            }
            assert!(
                h.ctx.content_rect().contains_rect(add),
                "{mode:?}/{width}/{scale}: New Tab {add:?} outside {:?}; header {:?}",
                h.ctx.content_rect(),
                [
                    "Main Menu",
                    "Save",
                    "Undo",
                    "Quick Access Toolbar",
                    "Scroll documents left",
                    "Scroll documents right"
                ]
                .into_iter()
                .filter_map(|label| h
                    .query_by_role_and_label(Role::Button, t(label))
                    .map(|node| (label, node.rect())))
                .collect::<Vec<_>>()
            );
            assert!(
                quick_access.right() <= add.left(),
                "{mode:?}/{width}/{scale}: Quick Access {quick_access:?}, New Tab {add:?}"
            );
            if mode == crate::app::CenterMode::Outliner {
                let menu = h.get_by_role_and_label(Role::Button, t("Main Menu")).rect();
                assert!(
                    menu.right() <= quick_access.left(),
                    "{mode:?}/{width}/{scale}: Main Menu {menu:?}, Quick Access {quick_access:?}"
                );
            }
            for label in [
                "Format Panel",
                "Symbols",
                "Outliner",
                "Export",
                "Zen Mode",
                "Find…",
            ] {
                if let Some(action) = h.query_by_role_and_label(Role::Button, t(label)) {
                    assert!(
                        add.right() <= action.rect().left(),
                        "{mode:?}/{width}/{scale}: New Tab {add:?}, {label} {:?}",
                        action.rect()
                    );
                }
            }
            h.get_by_role_and_label(Role::Button, t("Scroll documents right"))
                .click();
            h.run_steps(3);
            assert_eq!(
                h.get_by_role_and_label(Role::Button, t("New Tab")).rect(),
                add,
                "{mode:?}/{width}/{scale}: New Tab moved while scrolling"
            );
        }
    }
}

#[cfg(target_os = "windows")]
#[test]
fn desktop_outliner_replaces_the_main_ribbon_and_keeps_quick_access() {
    let mut app = ThinodaApp::headless();
    app.desktop_ribbon = true;
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    let original = doc.doc.sheet().topic(root).unwrap().title.clone();
    doc.doc
        .edit_sheet("Rename", |sheet| {
            sheet.set_title(root, "Selected outline topic")
        })
        .unwrap();
    doc.view.select_only(root);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, t("File"));
    h.get_by_role_and_label(Role::Button, t("Outliner")).click();
    h.run_steps(4);
    assert_eq!(h.state().mode, crate::app::CenterMode::Outliner);
    assert!(
        h.query_by_role_and_label(Role::Button, t("File")).is_none(),
        "the main ribbon must not remain above the outline ribbon"
    );
    h.get_by_role_and_label(Role::Button, t("Main Menu"));
    let heading = format!("{}: Selected outline topic", t("Selected Topic"));
    let heading_rect = h.get_by_role_and_label(Role::Label, &heading).rect();
    let action_rect = h.get_by_role_and_label(Role::Button, t("Add Child")).rect();
    assert!(
        action_rect.top() - heading_rect.bottom() >= 4.0,
        "heading {heading_rect:?}, actions {action_rect:?}"
    );
    assert!(h.output().shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text)
            if text.galley.text() == heading
                && text.galley.job.sections.iter().all(|section| section.format.font_id.size >= 14.0))
    }));
    let inspector = h.get_by_role_and_label(Role::Tab, t("Style")).rect();
    assert!(
        action_rect.bottom() <= inspector.top(),
        "outline ribbon {action_rect:?} should be above inspector {inspector:?}; panel {:?}; heading {heading_rect:?}; controls {:?}",
        h.state().ribbon.rect,
        [
            "Collapse Ribbon",
            "Add Sibling",
            "Add Child",
            "Edit Topic",
            "Move Up"
        ]
        .into_iter()
        .map(|label| (
            label,
            h.get_by_role_and_label(Role::Button, t(label)).rect()
        ))
        .collect::<Vec<_>>()
    );

    h.get_by_role_and_label(Role::Button, t("Undo")).click();
    h.run_steps(8);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(root)
            .unwrap()
            .title,
        original,
        "Quick Access Undo must still dispatch in outliner mode"
    );
    h.get_by_role_and_label(Role::Button, t("Main Menu"))
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("File"));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Outliner")).click();
    h.run_steps(4);
    assert_eq!(h.state().mode, crate::app::CenterMode::Map);
    h.get_by_role_and_label(Role::Button, t("File"));
    assert!(
        h.query_by_role_and_label(Role::Button, t("Add Child"))
            .is_none(),
        "the outline ribbon must disappear in map mode"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn desktop_outliner_ribbon_respects_the_shared_collapse_preference() {
    let mut app = ThinodaApp::headless();
    app.desktop_ribbon = true;
    app.mode = crate::app::CenterMode::Outliner;
    app.settings.ribbon_collapsed = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, t("Add Child"));
    h.state_mut().dialogs.settings = true;
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::CTRL, Key::F1);
    h.run_steps(3);
    assert!(!h.state().settings.ribbon_collapsed);
    assert!(
        h.get_by_role_and_label(Role::Button, t("Collapse Ribbon"))
            .accesskit_node()
            .is_disabled()
    );
    h.state_mut().dialogs.settings = false;
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::CTRL, Key::F1);
    h.run_steps(4);
    assert!(h.state().settings.ribbon_collapsed);
    assert!(
        h.query_by_role_and_label(Role::Button, t("Add Child"))
            .is_none()
    );
    h.get_by_role_and_label(Role::Button, t("Expand Ribbon"));
    h.get_by_role_and_label(Role::Button, t("Outliner")).click();
    h.run_steps(4);
    assert_eq!(h.state().mode, crate::app::CenterMode::Map);
    assert!(h.state().settings.ribbon_collapsed);
    h.get_by_role_and_label(Role::Button, t("Outliner")).click();
    h.run_steps(4);
    assert_eq!(h.state().mode, crate::app::CenterMode::Outliner);
    h.get_by_role_and_label(Role::Button, t("Expand Ribbon"))
        .focus();
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert!(!h.state().settings.ribbon_collapsed);
    h.get_by_role_and_label(Role::Button, t("Add Child"));
}

#[cfg(target_os = "windows")]
#[test]
fn desktop_outliner_access_keys_open_the_matching_main_menu_category() {
    let mut app = ThinodaApp::headless();
    app.desktop_ribbon = true;
    app.mode = crate::app::CenterMode::Outliner;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    for (key, command) in [
        (Key::Num1, "New from Template…"),
        (Key::Num2, "Copy as draw.io"),
        (Key::Num3, "Rename Sheet"),
        (Key::Num4, "Task Planning"),
        (Key::Num5, "AI settings"),
        (Key::Num6, "Work Calendar"),
        (Key::Num7, "Sticky note"),
        (Key::Num8, "Manage Zones"),
    ] {
        h.key_press_modifiers(Modifiers::ALT, key);
        h.run_steps(6);
        let item = h.get_by_role_and_label(Role::Button, t(command));
        assert_eq!(
            h.ctx
                .layer_id_at(item.rect().center())
                .map(|layer| layer.order),
            Some(egui::Order::Foreground),
            "{key:?}/{command} must be in the open category menu"
        );
        assert_eq!(h.state().mode, crate::app::CenterMode::Outliner);
        h.get_by_role_and_label(Role::Button, t("Add Child"));
    }
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    h.get_by_role_and_label(Role::Button, t("Main Menu"))
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Sheet"));
}

#[cfg(target_os = "windows")]
#[test]
fn desktop_outliner_category_menu_supports_keyboard_activation_and_modal_blocking() {
    let mut app = ThinodaApp::headless();
    app.desktop_ribbon = true;
    app.mode = crate::app::CenterMode::Outliner;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.state_mut().dialogs.settings = true;
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::ALT, Key::Num3);
    h.run_steps(3);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    h.state_mut().dialogs.settings = false;
    h.run_steps(3);

    h.state_mut().export(thinoda_io::Format::PowerPoint, false);
    h.run_steps(3);
    assert!(h.state().dialogs.powerpoint_export);
    h.key_press_modifiers(Modifiers::ALT, Key::Num3);
    h.key_press_modifiers(Modifiers::CTRL, Key::F1);
    h.run_steps(3);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert!(!h.state().settings.ribbon_collapsed);
    assert!(
        h.get_by_role_and_label(Role::Button, t("Collapse Ribbon"))
            .accesskit_node()
            .is_disabled()
    );
    h.state_mut().dialogs.powerpoint_export = false;
    h.state_mut().dialogs.export_owner = None;
    h.run_steps(3);

    h.get_by_role_and_label(Role::Button, t("Edit Topic"))
        .click();
    h.run_steps(4);
    assert!(h.ctx.text_edit_focused());
    h.key_press_modifiers(Modifiers::ALT, Key::Num3);
    h.run_steps(3);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert!(h.ctx.text_edit_focused());
    h.key_press(Key::Escape);
    h.run_steps(4);

    let count = h.state().doc().unwrap().doc.workbook().sheets.len();
    h.key_press_modifiers(Modifiers::ALT, Key::Num3);
    h.run_steps(4);
    h.key_press(Key::ArrowDown);
    h.run_steps(4);
    assert!(
        h.query_all_by_role_and_label(Role::Button, t("New Sheet"))
            .any(|node| node.is_focused()
                && h.ctx
                    .layer_id_at(node.rect().center())
                    .map(|layer| layer.order)
                    == Some(egui::Order::Foreground))
    );
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(
        h.state().doc().unwrap().doc.workbook().sheets.len(),
        count + 1
    );
    assert!(!egui::Popup::is_any_open(&h.ctx));
}

#[cfg(target_os = "windows")]
#[test]
fn desktop_outliner_category_switch_reveals_the_new_menu_start() {
    let mut app = ThinodaApp::headless();
    app.desktop_ribbon = true;
    app.mode = crate::app::CenterMode::Outliner;
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 480.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.key_press_modifiers(Modifiers::ALT, Key::Num7);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, t("Rotate right"))
        .scroll_to_me();
    h.run_steps(5);
    h.key_press_modifiers(Modifiers::ALT, Key::Num4);
    h.run_steps(5);
    let topic = format!(
        "{} {}",
        t("Topic"),
        ThinodaApp::shortcut_hint_for_mode(
            &h.ctx,
            &Cmd::Canvas(thinoda_canvas::Action::InsertSibling),
            crate::app::CenterMode::Outliner,
        )
        .unwrap()
    );
    let node = h.get_by_role_and_label(Role::Button, &topic);
    let popup = h.ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| layer.order == egui::Order::Foreground)
            .filter_map(|layer| memory.area_rect(layer.id))
            .find(|rect| rect.contains(node.rect().center()))
    });
    assert!(
        popup.is_some(),
        "the first item must be visible after switching categories"
    );
    h.key_press(Key::ArrowDown);
    h.run_steps(4);
    assert!(h.get_by_role_and_label(Role::Button, &topic).is_focused());
}

#[cfg(target_os = "windows")]
#[test]
fn desktop_outliner_long_heading_stays_inside_the_ribbon() {
    let mut app = ThinodaApp::headless();
    app.desktop_ribbon = true;
    app.mode = crate::app::CenterMode::Outliner;
    let title = "A long selected topic ".repeat(30);
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Rename", |sheet| sheet.set_title(root, &title))
        .unwrap();
    doc.view.select_only(root);
    let mut h = Harness::builder()
        .with_size(egui::vec2(720.0, 480.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    let heading = format!("{}: {title}", t("Selected Topic"));
    let heading_rect = h.get_by_role_and_label(Role::Label, &heading).rect();
    let toggle = h
        .get_by_role_and_label(Role::Button, t("Collapse Ribbon"))
        .rect();
    let ribbon = h.state().ribbon.rect.unwrap();
    assert!(
        ribbon.contains_rect(heading_rect),
        "{heading_rect:?}, {ribbon:?}"
    );
    assert!(
        !heading_rect.intersects(toggle),
        "{heading_rect:?}, {toggle:?}"
    );
    h.hover_at(heading_rect.center());
    h.run_steps(90);
    assert!(h.output().shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == heading)
    }));
}

#[cfg(target_os = "windows")]
#[test]
fn desktop_outliner_ribbon_keeps_actions_and_outline_reachable_at_small_sizes() {
    for (width, height) in [
        (1280.0, 800.0),
        (1000.0, 700.0),
        (720.0, 480.0),
        (360.0, 240.0),
    ] {
        for compact in [false, true] {
            let mut app = ThinodaApp::headless();
            app.desktop_ribbon = true;
            app.mode = crate::app::CenterMode::Outliner;
            app.settings.compact_ribbon = compact;
            let root = app.doc().unwrap().doc.sheet().root;
            let title = app
                .doc()
                .unwrap()
                .doc
                .sheet()
                .topic(root)
                .unwrap()
                .title
                .clone();
            let mut h = Harness::builder()
                .with_size(egui::vec2(width, height))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(4);
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
                h.get_by_role_and_label(Role::Button, t(label))
                    .scroll_to_me();
                h.run_steps(8);
                let rect = h.get_by_role_and_label(Role::Button, t(label)).rect();
                assert!(
                    h.ctx.content_rect().contains_rect(rect),
                    "{width}x{height}/{compact}: {label} {rect:?}"
                );
                assert!(
                    h.output().shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::Shape::Text(text)
                            if text.galley.job.text.chars().all(|c| c >= '\u{f0000}'))
                            && rect.contains_rect(shape.shape.visual_bounding_rect())
                            && shape
                                .clip_rect
                                .expand(1.0 / h.ctx.pixels_per_point())
                                .contains_rect(shape.shape.visual_bounding_rect())
                    }),
                    "{width}x{height}/{compact}: {label} icon is clipped"
                );
            }
            h.query_all_by_role_and_label(Role::Button, &title)
                .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
                .unwrap()
                .focus();
            h.run_steps(8);
            let topic = h
                .query_all_by_role_and_label(Role::Button, &title)
                .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
                .unwrap()
                .rect();
            assert!(
                h.ctx.content_rect().contains_rect(topic),
                "{width}x{height}/{compact}: selected outline topic is hidden: {topic:?}"
            );
            assert_eq!(h.state().doc().unwrap().view.primary(), Some(root));
        }
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
