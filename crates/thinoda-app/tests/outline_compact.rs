//! Sequential locale coverage for short-window Outline controls.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::Action;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

#[test]
fn hidden_board_outliner_skips_the_anchor_and_counts_content() {
    let mut app = ThinodaApp::headless();
    let mut sheet = thinoda_core::Sheet::new("Board", "");
    sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
    let first = sheet.add_floating("First", thinoda_core::Vec2::ZERO);
    sheet.add_floating("Second", thinoda_core::Vec2::new(0.0, 150.0));
    app.docs[app.active] = thinoda_app::doc::OpenDoc::new(thinoda_core::Workbook::new(sheet));
    app.mode = CenterMode::Outliner;
    app.settings.show_status_bar = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "First").focus();
    h.run_steps(2);
    h.key_press(egui::Key::ArrowUp);
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().view.primary(), Some(first));
    h.get_by_role_and_label(Role::Button, "Second");
    h.get_by_role_and_label(Role::Button, t("Statistics"))
        .click();
    h.run_steps(2);
    h.get_by_label("Topics: 2");
}

#[test]
fn long_selected_titles_do_not_hide_outline_actions_or_topics() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for (size, scale) in [
                (egui::vec2(320.0, 240.0), 2.0),
                (egui::vec2(640.0, 360.0), 1.5),
                (egui::vec2(1000.0, 700.0), 1.0),
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
                app.mode = CenterMode::Outliner;
                let doc = app.doc_mut().unwrap();
                let root = doc.doc.sheet().root;
                let mut selected = root;
                doc.doc
                    .edit_sheet("Fixture", |sheet| {
                        sheet.set_title(root, "Visible root")?;
                        selected = sheet.add_child(root, None, "long selected title ".repeat(100));
                        sheet.add_child(root, None, "Other topic");
                        Ok(())
                    })
                    .unwrap();
                doc.view.select_only(selected);
                let before = doc.doc.workbook().clone();
                let revision = doc.doc.state_id();
                let mut h = Harness::builder()
                    .with_size(size * scale)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(4);
                h.set_size(size);
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
                        "{label}: {rect:?}"
                    );
                    assert!(
                        h.output().shapes.iter().any(|shape| {
                            matches!(
                                &shape.shape,
                                egui::Shape::Text(text) if text.galley.job.text.chars().all(|c| c >= '\u{f0000}')
                            ) && rect.contains_rect(shape.shape.visual_bounding_rect())
                                && shape
                                    .clip_rect
                                    .expand(1.0 / h.ctx.pixels_per_point())
                                    .contains_rect(shape.shape.visual_bounding_rect())
                        }),
                        "{label}: command icon is clipped"
                    );
                }
                let root = h.get_by_role_and_label(Role::Button, "Visible root").rect();
                assert!(h.ctx.content_rect().contains_rect(root), "{root:?}");
                // Same-layer scroll regions must not steal one another's focus reveal.
                h.get_by_role_and_label(Role::Button, "Visible root")
                    .focus();
                h.run_steps(8);
                for label in ["Add Child", "Delete Topic Only", "Add Child"] {
                    h.get_by_role_and_label(Role::Button, t(label)).focus();
                    h.run_steps(8);
                    let rect = h.get_by_role_and_label(Role::Button, t(label)).rect();
                    assert!(
                        h.ctx.content_rect().contains_rect(rect),
                        "focus {label}: {rect:?}"
                    );
                    let root = h.get_by_role_and_label(Role::Button, "Visible root").rect();
                    assert!(
                        h.ctx.content_rect().contains_rect(root),
                        "body scrolled by header focus"
                    );
                }
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
                h.get_by_role_and_label(Role::Button, t("Move Down"))
                    .scroll_to_me();
                h.run_steps(8);
                h.get_by_role_and_label(Role::Button, t("Move Down"))
                    .click();
                h.run_steps(4);
                assert_eq!(
                    h.state()
                        .doc()
                        .unwrap()
                        .doc
                        .sheet()
                        .index_in_parent(selected),
                    before
                        .active()
                        .index_in_parent(selected)
                        .map(|index| index + 1)
                );
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
                h.run_steps(4);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            }
        }
    }
}
