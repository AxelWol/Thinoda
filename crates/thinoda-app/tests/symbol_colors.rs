//! Kept in one test because the UI language is process-wide.
use egui::{Event, PointerButton, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, Services, SidebarFamily, ThinodaApp},
    i18n::{self, t},
};
use thinoda_canvas::Action;
use thinoda_core::Color;
use thinoda_io::Format;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn harness(language: Language, theme: ThemeMode) -> Harness<'static, ThinodaApp> {
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
    app.sidebar_family = SidebarFamily::Symbols;
    app.symbol_color = Some(Color::WHITE);
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    d.doc
        .edit_sheet("Fill", |s| {
            s.update_style(&[root], |style| style.fill = Some(Color::WHITE))
        })
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(5);
    h
}

fn click_at(h: &mut Harness<'_, ThinodaApp>, pos: egui::Pos2, button: PointerButton) {
    h.event(Event::PointerMoved(pos));
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        });
        h.run_steps(1);
    }
    h.run_steps(3);
}

#[test]
fn insertion_instance_contrast_context_removal_and_export_cancellation() {
    let languages: Vec<_> = Language::ALL
        .into_iter()
        .filter(|language| *language != Language::Auto)
        .collect();
    assert_eq!(languages.len(), 10);
    for language in languages {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let mut h = harness(language, theme);
            h.get_by_role_and_label(Role::TextInput, t("Symbol color"))
                .focus();
            h.run_steps(2);
            h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
            h.run_steps(1);
            h.get_by_role_and_label(Role::TextInput, t("Symbol color"))
                .type_text("#invalid");
            h.run_steps(3);
            h.get_by_label(t("Invalid color"));
            assert_eq!(h.state().symbol_color, Some(Color::WHITE));
            h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
            h.run_steps(1);
            h.get_by_role_and_label(Role::TextInput, t("Symbol color"))
                .type_text("#FFFFFF");
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t("Improve contrast"))
                .focus();
            h.key_press(egui::Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().symbol_color, Some(Color::BLACK));
            h.get_by_role_and_label(Role::Button, t("Priority 1"))
                .scroll_to_me();
            h.run_steps(6);
            h.get_by_role_and_label(Role::Button, t("Priority 1"))
                .click();
            h.run_steps(4);
            let root = h.state().doc().unwrap().doc.sheet().root;
            let topic = h.state().doc().unwrap().doc.sheet().topic(root).unwrap();
            assert_eq!(topic.markers, ["priority-1"]);
            assert_eq!(topic.marker_colors[&0], Color::BLACK);
            let ctx = h.ctx.clone();
            h.state_mut().exec(
                &ctx,
                Cmd::Canvas(Action::SetMarkerColor(root, 0, Some(Color::WHITE))),
            );
            h.run_steps(4);
            h.get_by_role_and_label(Role::Button, t("Improve contrast"))
                .scroll_to_me();
            h.run_steps(6);
            h.get_by_role_and_label(Role::Button, t("Improve contrast"))
                .click();
            h.run_steps(4);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(root)
                    .unwrap()
                    .marker_colors[&0],
                Color::BLACK
            );
            h.state_mut().exec(
                &ctx,
                Cmd::Canvas(Action::ToggleMarkerColor(
                    "flag-red".into(),
                    Some(Color::rgb(200, 10, 20)),
                )),
            );
            h.run_steps(5);
            let d = h.state().doc().unwrap();
            let position = d.view.symbol_rect(&d.doc, root, 1).unwrap().center();
            click_at(&mut h, position, PointerButton::Secondary);
            assert_eq!(
                h.state().doc().unwrap().view.context_marker,
                Some((root, 1))
            );
            // The popup and sidebar both expose the same keyboard-operable action.
            let buttons: Vec<_> = h
                .query_all_by_role_and_label(Role::Button, t("Remove Symbol"))
                .collect();
            buttons.last().unwrap().focus();
            h.key_press(egui::Key::Enter);
            h.run_steps(4);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(root)
                    .unwrap()
                    .markers,
                ["priority-1"]
            );
            h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
            h.run_steps(4);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(root)
                    .unwrap()
                    .markers,
                ["priority-1", "flag-red"]
            );
            h.state_mut().exec(&ctx, Cmd::Canvas(Action::Redo));
            h.run_steps(4);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(root)
                    .unwrap()
                    .markers,
                ["priority-1"]
            );
            h.state_mut().settings.show_format_panel = false;
            h.state_mut().symbol_color = Some(Color::rgb(10, 30, 90));
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t("Marker")).click();
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t("Priority 2"))
                .click();
            h.run_steps(4);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(root)
                    .unwrap()
                    .markers,
                ["priority-2"]
            );
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(root)
                    .unwrap()
                    .marker_colors[&0],
                Color::rgb(10, 30, 90)
            );
            h.key_press(egui::Key::Escape);
            h.run_steps(2);
            let before = h.state().doc().unwrap().doc.workbook().clone();
            for format in [
                Format::XMind,
                Format::Markdown,
                Format::FreeMind,
                Format::Opml,
                Format::TextBundle,
                Format::Word,
                Format::Excel,
                Format::Csv,
                Format::PlainText,
            ] {
                h.state_mut().exec(&ctx, Cmd::Export(format));
                h.run_steps(3);
                assert_eq!(
                    h.state().dialogs.symbol_export,
                    Some((h.state().active, format))
                );
                h.query_all_by_role_and_label(Role::Button, t("Cancel"))
                    .find(|node| !node.accesskit_node().is_disabled())
                    .unwrap()
                    .click();
                h.run_steps(3);
                assert!(h.state().dialogs.symbol_export.is_none());
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            }
            if language == Language::German {
                assert_eq!(i18n::t("Remove Symbol"), "Symbol entfernen");
            } else if language != Language::English {
                assert_ne!(i18n::t("Remove Symbol"), "Remove Symbol");
            }
        }
    }
}

#[test]
fn additional_symbol_catalog_rows_cover_every_new_key() {
    let rows: std::collections::HashMap<_, _> = include_str!("../src/locales.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let values: Vec<_> = line.split('|').collect();
            assert_eq!(values.len(), 8);
            assert!(values.iter().all(|s| !s.trim().is_empty()));
            (values[0], values)
        })
        .collect();
    for key in [
        "Symbol color",
        "Default symbol color",
        "Placed symbols",
        "Remove Symbol",
        "Contrast",
        "Pass",
        "Low contrast",
        "Improve contrast",
        "Invalid color",
        "Contrast compares the symbol with its topic background; it is not an overall accessibility assessment.",
        "Graphical symbols: 3:1; month and week text: 4.5:1.",
        "Editable exports lose symbol colors, except Thinoda and unchanged XMind files reopened in Thinoda. XMind itself shows standard icons; third-party edits can discard custom colors.",
    ] {
        assert!(rows.contains_key(key), "{key}");
    }
}
