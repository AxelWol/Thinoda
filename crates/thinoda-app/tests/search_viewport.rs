//! Sequential locale coverage because UI language is process-wide.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::Action;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn assert_visible(h: &Harness<'_, ThinodaApp>, role: Role, label: &'static str) {
    let rect = h.get_by_role_and_label(role, t(label)).rect();
    assert!(
        h.ctx.content_rect().contains_rect(rect),
        "{label}: {rect:?} outside {:?}",
        h.ctx.content_rect()
    );
    if label == "Replace All" {
        assert!(
            h.output().shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                if text.galley.text() == t(label)
                && rect.contains(text.pos)
                && shape.clip_rect.expand(1.0 / h.ctx.pixels_per_point())
                    .contains_rect(text.galley.rect.translate(text.pos.to_vec2())))
            }),
            "Replace All caption is clipped"
        );
    }
    let area = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_search")))
        .unwrap();
    assert!(
        h.ctx.content_rect().expand(0.5).contains_rect(area),
        "search area {area:?} outside {:?}",
        h.ctx.content_rect()
    );
}

#[test]
fn find_and_replace_remain_reachable_in_short_windows() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for (size, scale) in [
                (egui::vec2(320.0, 180.0), 2.0),
                (egui::vec2(640.0, 360.0), 1.5),
                (egui::vec2(960.0, 600.0), 1.0),
                (egui::vec2(1280.0, 800.0), 1.5),
                (egui::vec2(1280.0, 800.0), 1.0),
            ] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: scale,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                let doc = app.doc_mut().unwrap();
                doc.doc
                    .edit_sheet("Fixture", |sheet| {
                        sheet.title = "Long sheet title ".repeat(50);
                        sheet.set_title(sheet.root, "Needle root")?;
                        sheet.add_child(sheet.root, None, "Needle child");
                        Ok(())
                    })
                    .unwrap();
                let before = doc.doc.workbook().clone();
                let revision = doc.doc.state_id();
                let mut h = Harness::builder()
                    .with_size(size * scale)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.set_size(size);
                h.run_steps(3);
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::FindReplace);
                h.run_steps(5);
                h.get_by_role_and_label(Role::TextInput, t("Search"))
                    .type_text("Needle");
                h.run_steps(3);
                h.state_mut().search.all_sheets = true;
                h.state_mut().search.dim = true;
                h.run_steps(3);
                assert_eq!(h.state().search.hits.len(), 2);
                if size.y / scale >= 500.0 {
                    assert_visible(&h, Role::TextInput, "Search");
                    assert_visible(&h, Role::TextInput, "Replace with");
                    assert_visible(&h, Role::Button, "Replace All");
                    h.state_mut().search.show_replace = false;
                    h.run_steps(4);
                    h.state_mut().search.show_replace = true;
                    h.run_steps(4);
                    assert_visible(&h, Role::TextInput, "Search");
                    assert_visible(&h, Role::TextInput, "Replace with");
                    assert_visible(&h, Role::Button, "Replace All");
                }

                for (mode, panel) in [(CenterMode::Map, true), (CenterMode::Outliner, false)] {
                    h.state_mut().mode = mode;
                    h.state_mut().settings.show_format_panel = panel;
                    h.run_steps(4);
                    h.get_by_role_and_label(Role::TextInput, t("Search"))
                        .focus();
                    h.run_steps(6);
                    assert_visible(&h, Role::TextInput, "Search");
                    for label in ["Previous result", "Next result", "Close search"] {
                        h.key_press(egui::Key::Tab);
                        h.run_steps(6);
                        assert!(
                            h.get_by_role_and_label(Role::Button, t(label)).is_focused(),
                            "Tab order: {label}/{language:?}/{theme:?}/{size:?}/{mode:?}; previous={}, close={}, search={}, focused={:?}, next={:?}",
                            h.get_by_role_and_label(Role::Button, t("Previous result"))
                                .is_focused(),
                            h.get_by_role_and_label(Role::Button, t("Close search"))
                                .is_focused(),
                            h.get_by_role_and_label(Role::TextInput, t("Search"))
                                .is_focused(),
                            h.ctx
                                .memory(|m| m.focused())
                                .and_then(|id| h.ctx.read_response(id))
                                .map(|r| r.rect),
                            h.get_by_role_and_label(Role::Button, t("Next result"))
                                .rect(),
                        );
                        assert_visible(&h, Role::Button, label);
                    }
                    for (role, label) in [
                        (Role::CheckBox, "All sheets"),
                        (Role::CheckBox, "Include labels"),
                        (Role::TextInput, "Replace with"),
                        (Role::Button, "Replace All"),
                    ] {
                        h.get_by_role_and_label(role, t(label)).focus();
                        h.run_steps(8);
                        assert_visible(&h, role, label);
                    }
                }
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
                h.get_by_role_and_label(Role::TextInput, t("Replace with"))
                    .scroll_to_me();
                h.run_steps(8);
                h.get_by_role_and_label(Role::TextInput, t("Replace with"))
                    .click();
                h.run_steps(2);
                h.get_by_role_and_label(Role::TextInput, t("Replace with"))
                    .type_text("Plan");
                h.run_steps(3);
                h.set_size(egui::vec2(320.0, 240.0));
                h.run_steps(8);
                assert_eq!(h.state().search.query, "Needle");
                assert_eq!(h.state().search.replace, "Plan");
                assert!(h.state().search.all_sheets);
                assert!(h.state().search.dim);
                h.get_by_role_and_label(Role::Button, t("Replace All"))
                    .scroll_to_me();
                h.run_steps(8);
                assert_visible(&h, Role::Button, "Replace All");
                h.get_by_role_and_label(Role::Button, t("Replace All"))
                    .click();
                h.run_steps(3);
                let sheet = h.state().doc().unwrap().doc.sheet();
                assert_eq!(sheet.topic(sheet.root).unwrap().title, "Plan root");
                h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                assert!(!h.state().search.open);
                assert!(h.state().doc().unwrap().view.filter.is_none());
                h.state_mut().exec(&ctx, Cmd::Find);
                h.run_steps(6);
                h.get_by_role_and_label(Role::Button, t("Close search"))
                    .scroll_to_me();
                h.run_steps(8);
                h.get_by_role_and_label(Role::Button, t("Close search"))
                    .click();
                h.run_steps(3);
                assert!(!h.state().search.open);
            }
        }
    }
}
