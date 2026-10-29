use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{CenterMode, Cmd, FormatTab, Services, ThinodaApp},
    doc::OpenDoc,
    i18n::t,
};
use thinoda_platform::{
    Paths,
    recovery::RecoveryManager,
    settings::{Language, Settings, ThemeMode},
};

static LANGUAGE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn harness(mut app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    app.format_tab = FormatTab::Data;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1000.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h
}

fn click(h: &mut Harness<'_, ThinodaApp>, label: &'static str) {
    let node = h
        .query_all_by_role_and_label(Role::Button, t(label))
        .min_by(|a, b| {
            let order = a.rect().left().total_cmp(&b.rect().left());
            if matches!(label, "Maximize" | "Restore") {
                order.reverse()
            } else {
                order
            }
        })
        .unwrap();
    node.scroll_to_me();
    h.run_steps(6);
    h.query_all_by_role_and_label(Role::Button, t(label))
        .min_by(|a, b| {
            let order = a.rect().left().total_cmp(&b.rect().left());
            if matches!(label, "Maximize" | "Restore") {
                order.reverse()
            } else {
                order
            }
        })
        .unwrap()
        .click();
    h.run_steps(4);
}

fn text(h: &mut Harness<'_, ThinodaApp>, label: &'static str, text: &str) {
    h.query_all_by_role_and_label(Role::MultilineTextInput, t(label))
        .find(|node| !node.accesskit_node().is_disabled())
        .unwrap()
        .focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    h.query_all_by_role_and_label(Role::MultilineTextInput, t(label))
        .find(|node| !node.accesskit_node().is_disabled())
        .unwrap()
        .type_text(text);
    h.run_steps(3);
}

#[test]
fn enlarged_notes_cancel_apply_undo_save_and_recovery_keep_explicit_commit_boundary() {
    let _language = LANGUAGE.lock().unwrap();
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.thinoda");
        app.doc_mut().unwrap().save_to(&path, false).unwrap();
        let paths = Paths::from_root(dir.path().join("profile"));
        let recovery = RecoveryManager::start(&paths).unwrap();
        let mut h = harness(app);
        let original = h.state().doc().unwrap().doc.workbook().clone();
        click(&mut h, "Enlarge notes editor");
        text(&mut h, "Notes", "cancel this staged text");
        click(&mut h, "Cancel notes edit");
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &original);
        click(&mut h, "Enlarge notes editor");
        text(&mut h, "Notes", "café staged notes");
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Save);
        h.run_steps(3);
        assert_eq!(OpenDoc::open(&path).unwrap().doc.workbook(), &original);
        assert!(!h.state().doc().unwrap().is_dirty());
        click(&mut h, "Apply notes");
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("café staged notes")
        );
        assert!(
            !h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap()
                .is_markdown()
        );
        h.state_mut().doc_mut().unwrap().snapshot(&recovery);
        recovery.suspend().unwrap();
        let snapshots = RecoveryManager::find_orphans(&paths);
        assert_eq!(
            snapshots[0].docs[0]
                .load()
                .unwrap()
                .active()
                .root_topic()
                .notes
                .as_deref(),
            Some("café staged notes")
        );
        drop(snapshots);
        recovery.resume().unwrap();
        h.state_mut().exec(&ctx, Cmd::Save);
        h.state_mut()
            .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &original);
        assert_eq!(
            OpenDoc::open(&path)
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("café staged notes")
        );
        recovery.finish().unwrap();
    }
}

#[test]
fn equation_enlarge_transfers_scratch_and_cancel_does_not_apply() {
    let _language = LANGUAGE.lock().unwrap();
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        let mut h = harness(app);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        click(&mut h, "Edit notes equation");
        text(&mut h, "Equation source", "\\frac{x}{y}+z^{3}");
        click(&mut h, "Enlarge equation editor");
        assert_eq!(
            h.get_by_role_and_label(Role::MultilineTextInput, t("Equation source"))
                .accesskit_node()
                .value(),
            Some("\\frac{x}{y}+z^{3}".to_owned())
        );
        click(&mut h, "Cancel equation edit");
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        click(&mut h, "Edit notes equation");
        text(&mut h, "Equation source", "x^{2}");
        click(&mut h, "Enlarge equation editor");
        click(&mut h, "Apply notes equation");
        assert!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref()
                .unwrap()
                .contains("x^{2}")
        );
        let ctx = h.ctx.clone();
        h.state_mut()
            .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn enlarged_editors_invalidate_on_selection_revision_and_tab_without_applying_elsewhere() {
    let _language = LANGUAGE.lock().unwrap();
    for equation in [false, true] {
        for stale in 0..6 {
            let mut h = harness(ThinodaApp::headless());
            if equation {
                click(&mut h, "Edit notes equation");
                text(&mut h, "Equation source", "x^{4}");
                click(&mut h, "Enlarge equation editor");
            } else {
                click(&mut h, "Enlarge notes editor");
                text(&mut h, "Notes", "must never apply to another owner");
            }
            let doc = h.state_mut().doc_mut().unwrap();
            let root = doc.doc.sheet().root;
            match stale {
                0 => {
                    let child = doc.doc.sheet().children(root)[0];
                    doc.view.select_only(child);
                }
                1 => {
                    h.get_by_role_and_label(Role::Button, t("Subtopic")).click();
                }
                2 => {
                    h.get_by_role_and_label(Role::Tab, t("Style")).click();
                }
                3 => {
                    let ctx = h.ctx.clone();
                    h.state_mut().exec(&ctx, Cmd::New);
                }
                4 => {
                    doc.doc
                        .edit_sheet("Source change", |sheet| {
                            sheet.set_notes(root, Some("newer source".into()))
                        })
                        .unwrap();
                }
                _ => {
                    doc.doc
                        .edit_sheet("Format change", |sheet| {
                            sheet.set_markdown_notes(root, Some("newer formatted source".into()))
                        })
                        .unwrap();
                }
            }
            h.run_steps(5);
            assert!(
                h.query_by_role_and_label(
                    Role::Button,
                    t(if equation {
                        "Apply notes equation"
                    } else {
                        "Apply notes"
                    })
                )
                .is_none()
            );
            if stale < 4 {
                assert!(
                    h.state()
                        .doc()
                        .unwrap()
                        .doc
                        .sheet()
                        .topics
                        .values()
                        .all(|topic| topic.notes.is_none())
                );
            } else {
                assert_ne!(
                    h.state()
                        .doc()
                        .unwrap()
                        .doc
                        .sheet()
                        .root_topic()
                        .notes
                        .as_deref(),
                    Some("must never apply to another owner")
                );
            }
        }
    }
}

#[test]
fn enlarged_notes_capture_focused_source_after_blur_and_reuse_formatting_tools() {
    let _language = LANGUAGE.lock().unwrap();
    let mut h = harness(ThinodaApp::headless());
    text(&mut h, "Notes", "café before enlarge");
    click(&mut h, "Enlarge notes editor");
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_deref(),
        Some("café before enlarge")
    );
    text(&mut h, "Notes", "日本語 staged note");
    click(&mut h, "Bold notes");
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_deref(),
        Some("café before enlarge")
    );
    click(&mut h, "Apply notes");
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_ref()
            .unwrap()
            .is_markdown()
    );
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_deref(),
        Some("café before enlarge")
    );
}

#[test]
fn enlarged_editors_resize_by_pointer_without_mutating_the_document() {
    let _language = LANGUAGE.lock().unwrap();
    for equation in [false, true] {
        let mut h = harness(ThinodaApp::headless());
        if equation {
            click(&mut h, "Edit notes equation");
        }
        click(
            &mut h,
            if equation {
                "Enlarge equation editor"
            } else {
                "Enlarge notes editor"
            },
        );
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let id = if equation {
            egui::Id::unique("notes-equation-draft").with("dialog")
        } else {
            egui::Id::unique("enlarged-notes-draft")
        };
        let rect = h.ctx.memory(|m| m.area_rect(id)).unwrap();
        let start = rect.right_bottom() - egui::vec2(2.0, 2.0);
        let end = start + egui::vec2(100.0, 80.0);
        h.event(egui::Event::PointerMoved(start));
        h.run_steps(2);
        h.event(egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        h.run_steps(2);
        h.event(egui::Event::PointerMoved(end));
        h.run_steps(3);
        h.event(egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        h.run_steps(5);
        let resized = h.ctx.memory(|m| m.area_rect(id)).unwrap();
        assert!(thinoda_ui::DialogState::load(&h.ctx, id).user_sized);
        assert!(
            resized.width() > rect.width() + 50.0,
            "{resized:?} vs {rect:?}"
        );
        assert!(
            resized.height() > rect.height() + 40.0,
            "{resized:?} vs {rect:?}"
        );
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}
#[test]
fn enlarged_editors_share_localized_theme_aware_caption_controls_and_geometry() {
    let _language = LANGUAGE.lock().unwrap();
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for equation in [false, true] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                let mut h = harness(app);
                if equation {
                    click(&mut h, "Edit notes equation");
                }
                click(
                    &mut h,
                    if equation {
                        "Enlarge equation editor"
                    } else {
                        "Enlarge notes editor"
                    },
                );
                let id = if equation {
                    egui::Id::unique("notes-equation-draft").with("dialog")
                } else {
                    egui::Id::unique("enlarged-notes-draft")
                };
                let initial = h.ctx.memory(|m| m.area_rect(id)).unwrap();
                click(&mut h, "Maximize");
                assert!(thinoda_ui::DialogState::load(&h.ctx, id).maximized);
                click(&mut h, "Restore");
                assert!(
                    !thinoda_ui::DialogState::load(&h.ctx, id).maximized,
                    "{language:?}/{theme:?}/equation={equation}"
                );
                let restored = h.ctx.memory(|m| m.area_rect(id)).unwrap();
                assert!((initial.size() - restored.size()).length() < 4.0);
                click(&mut h, "Minimize");
                assert!(thinoda_ui::DialogState::load(&h.ctx, id).hidden);
                click(&mut h, "Expand");
                assert!(!thinoda_ui::DialogState::load(&h.ctx, id).hidden);
                h.set_size(egui::vec2(400.0, 300.0));
                h.run_steps(6);
                let rect = h.ctx.memory(|m| m.area_rect(id)).unwrap();
                assert!(
                    h.ctx.content_rect().expand(1.0).contains_rect(rect),
                    "{language:?}/{theme:?}: {rect:?}"
                );
                click(&mut h, "Close Window");
                assert!(
                    h.query_by_role_and_label(
                        Role::Button,
                        t(if equation {
                            "Apply notes equation"
                        } else {
                            "Apply notes"
                        })
                    )
                    .is_none()
                );
            }
        }
    }
}
