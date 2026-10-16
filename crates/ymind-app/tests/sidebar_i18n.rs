//! Separate process because the app's current UI language is process-wide.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use ymind_app::app::{Cmd, FormatTab, Services, SidebarFamily, YMindApp};
use ymind_platform::settings::{Language, Settings};

#[test]
fn german_sidebar_names_switch_and_edit_the_same_document() {
    let settings = Settings {
        language: Language::German,
        ..Settings::default()
    };
    let mut app = YMindApp::new(settings, Services::default(), &[]);
    app.exec(&egui::Context::default(), Cmd::New);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Struktur auswählen: Mindmap")
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::DisclosureTriangle, "Mindmaps");
    h.get_by_role_and_label(Role::Button, "Logikdiagramm (links)")
        .scroll_to_me();
    h.run_steps(8);
    h.get_by_role_and_label(Role::Button, "Logikdiagramm (links)")
        .click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().structure,
        ymind_core::Structure::LogicLeft
    );
    h.get_by_role_and_label(Role::Button, "Classic").click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().theme.theme_id,
        "classic"
    );
    h.get_by_role_and_label(Role::Button, "Symbole").click();
    h.run_steps(3);
    assert_eq!(h.state().sidebar_family, SidebarFamily::Symbols);
    h.get_by_role_and_label(Role::Button, "Richtig");
    h.get_by_role_and_label(Role::Button, "Priorität 1").click();
    h.run_steps(3);
    let sheet = h.state().doc().unwrap().doc.sheet();
    assert_eq!(sheet.topic(sheet.root).unwrap().markers, vec!["priority-1"]);
    h.get_by_role_and_label(Role::Button, "Formatbereich")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().format_tab, FormatTab::Map);
    h.get_by_role_and_label(Role::Tab, "Stil").click();
    h.run_steps(3);
    assert_eq!(h.state().format_tab, FormatTab::Style);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Settings);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Allgemein");
    h.get_by_role_and_label(Role::Button, "Editor").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Switch, "Themen-Symbolleiste anzeigen")
        .click();
    h.run_steps(3);
    assert!(!h.state().settings.show_topic_toolbar);
    h.get_by_role_and_label(Role::Switch, "Thema-hinzufügen-Buttons anzeigen");
    h.get_by_label("Letzte Struktur für neue leere Maps verwenden");
    h.get_by_label("Letztes Design für neue leere Maps verwenden");
    h.state_mut().dialogs.settings = false;
    h.state_mut().dialogs.about = true;
    h.run_steps(3);
    h.get_by_label("Geschrieben von Axel Wolters");
    h.get_by_role_and_label(Role::Link, "Kontakt: axelwolters@hotmail.com");
    h.state_mut().dialogs.about = false;
    h.run_steps(2);
    h.get_by_label(ymind_ui::icons::MENU).click();
    h.run_steps(2);
    h.get_by_label("Bearbeiten").click();
    h.run_steps(2);
    for label in [
        "Stil kopieren Ctrl+Alt+C",
        "Stil einfügen Ctrl+Alt+V",
        "Stil zurücksetzen",
        "Einrücken",
        "Ausrücken",
    ] {
        h.get_by_role_and_label(Role::Button, label);
    }
    h.get_by_role_and_label(Role::Button, "Stil kopieren Ctrl+Alt+C")
        .click();
    h.run_steps(2);
    assert!(h.state().style_clipboard.is_some());
    h.get_by_label(ymind_ui::icons::MENU).click();
    h.run_steps(2);
    h.get_by_label("Ansicht").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Vorheriges Blatt Ctrl+PageUp");
    h.get_by_role_and_label(Role::Button, "Nächstes Blatt Ctrl+PageDown");
    h.key_press(egui::Key::Escape);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.get_by_label(ymind_ui::icons::MENU).click();
    h.run_steps(2);
    h.get_by_label("Blatt").click();
    h.run_steps(2);
    for label in ["Blatt kopieren", "Blatt einfügen", "Blatt speichern unter…"] {
        h.get_by_role_and_label(Role::Button, label);
    }
    h.key_press(egui::Key::Escape);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.get_by_label(ymind_ui::icons::MENU).click();
    h.run_steps(2);
    h.get_by_label("Bearbeiten").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Anordnen").click();
    h.run_steps(2);
    for label in [
        "Unterthemen aufsteigend sortieren",
        "Mittelpunkte horizontal verteilen",
        "Oben ausrichten",
    ] {
        h.get_by_role_and_label(Role::Button, label);
    }
    h.key_press(egui::Key::Escape);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.get_by_label(ymind_ui::icons::MENU).click();
    h.run_steps(2);
    h.get_by_label("Werkzeuge").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Map-Dateien zusammenführen…");
    h.get_by_role_and_label(Role::Button, "Maskierte Kopie erstellen…")
        .click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Vorschau erstellen");
    h.get_by_role_and_label(Role::Button, "Abbrechen").click();
    h.run_steps(2);
    assert!(!h.state().dialogs.masked_copy);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Home);
    h.run_steps(3);
    h.get_by_label("Vorlagenkategorie").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Lernen und Bildung")
        .scroll_to_me();
    h.run_steps(8);
    h.get_by_role_and_label(Role::Button, "Lernen und Bildung")
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Lernplan").click();
    h.run_steps(3);
    assert!(h.state().doc().is_none());
    h.get_by_role_and_label(Role::Button, "Erstellen").click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.workbook(),
        &ymind_core::templates::instantiate("study-plan")
    );
    assert_eq!(h.state().settings.recent_templates, ["study-plan"]);
    h.state_mut().exec(&ctx, Cmd::MapShot);
    h.run_steps(1);
    let callback = h
        .output()
        .viewport_output
        .values()
        .flat_map(|v| &v.commands)
        .find_map(|cmd| match cmd {
            egui::ViewportCommand::Screenshot(callback) => Some(callback.clone()),
            _ => None,
        })
        .unwrap();
    callback.complete(std::sync::Arc::new(egui::ColorImage::filled(
        [1280, 800],
        egui::Color32::WHITE,
    )));
    h.run_steps(4);
    h.get_by_label("Bild kopieren");
    h.get_by_label("PNG speichern…");
    h.get_by_label("Bereichsbreite");
    h.get_by_label("Rahmenstärke");
    h.get_by_role_and_label(Role::Button, "Abbrechen")
        .scroll_to_me();
    h.run_steps(6);
    h.get_by_role_and_label(Role::Button, "Abbrechen").click();
    h.run_steps(2);
    assert!(h.state().command_available(&Cmd::MapShot));
}
