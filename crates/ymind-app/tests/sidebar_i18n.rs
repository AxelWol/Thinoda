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
    h.state_mut().exec(&ctx, Cmd::Labels);
    h.run_steps(4);
    h.get_by_label("Neue Labels")
        .type_text("Prüfung, Prüfung, erledigt");
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Übernehmen").click();
    h.run_steps(3);
    let d = h.state().doc().unwrap();
    assert_eq!(
        d.doc
            .sheet()
            .topic(d.view.primary().unwrap())
            .unwrap()
            .labels,
        ["Prüfung", "erledigt"]
    );
    h.state_mut().exec(&ctx, Cmd::Labels);
    h.run_steps(3);
    h.get_by_label("Label entfernen: Prüfung");
    h.get_by_role_and_label(Role::Button, "Abbrechen").click();
    h.run_steps(2);
    assert!(h.state().command_available(&Cmd::Labels));
    let d = h.state_mut().doc_mut().unwrap();
    let root = d.doc.sheet().root;
    let relationship = d
        .doc
        .edit_sheet("Beziehung", |sheet| {
            let child = sheet.insert_child(root, "Ziel")?;
            let id = sheet.add_relationship(root, child)?;
            sheet.set_relationship_control_points(
                id,
                Some([
                    ymind_core::Vec2::new(40.0, -50.0),
                    ymind_core::Vec2::new(-40.0, -50.0),
                ]),
            )?;
            Ok(id)
        })
        .unwrap();
    d.view
        .select_object(ymind_canvas::ObjectSelection::Relationship(relationship));
    h.run_steps(4);
    h.get_by_label("Beziehungsanfang");
    h.get_by_label("Erster Kurvengriff");
    h.get_by_label("Beziehungstitel");
    h.get_by_label("Von Thema");
    h.get_by_label("Erster Kurvengriff X");
    h.get_by_role_and_label(Role::Button, "Kurve zurücksetzen")
        .scroll_to_me();
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Kurve zurücksetzen")
        .click();
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .relationships
            .iter()
            .find(|r| r.id == relationship)
            .unwrap()
            .control_points
            .is_none()
    );
    let d = h.state_mut().doc_mut().unwrap();
    let root = d.doc.sheet().root;
    let branch = d
        .doc
        .edit_sheet("Zweigseite", |s| {
            s.set_structure(None, Some(ymind_core::Structure::MapClockwise))?;
            s.insert_child(root, "Zweig")
        })
        .unwrap();
    d.view.select_only(branch);
    h.run_steps(3);
    h.get_by_label("Zweigseite").scroll_to_me();
    h.run_steps(5);
    h.get_by_label("Zweigseite").click();
    h.run_steps(2);
    h.get_by_label("Links").click();
    h.run_steps(2);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(branch)
            .unwrap()
            .branch_side,
        Some(ymind_core::BranchSide::Left)
    );
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Switch, "Flexible freie Themen")
        .scroll_to_me();
    h.run_steps(6);
    h.get_by_role_and_label(Role::Switch, "Flexible freie Themen")
        .click();
    h.run_steps(2);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .theme
            .flexible_floating_topics
    );
    h.get_by_label("Globale Schriftart").scroll_to_me();
    h.run_steps(6);
    h.get_by_label("Globale Schriftart").click();
    h.run_steps(2);
    h.get_by_label("Nichtproportional").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Switch, "Automatische Zweigbreite")
        .scroll_to_me();
    h.run_steps(6);
    h.get_by_role_and_label(Role::Switch, "Automatische Zweigbreite")
        .click();
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().theme.line_width,
        Some(1.5)
    );
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .theme
            .font_family
            .as_deref(),
        Some("monospace")
    );
    h.get_by_role_and_label(Role::Tab, "Stil").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Schnellstil: Hervorhebung")
        .scroll_to_me();
    h.run_steps(6);
    h.get_by_role_and_label(Role::Button, "Schnellstil: Hervorhebung")
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Schnellstil speichern…")
        .click();
    h.run_steps(3);
    h.get_by_label("Stilname").click();
    h.run_steps(1);
    h.get_by_label("Stilname").type_text("Prüfung");
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Speichern").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.quick_styles[0].name, "Prüfung");
    h.get_by_role_and_label(Role::Button, "Entfernen…").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Abbrechen").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.quick_styles.len(), 1);
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Eigenes Design bearbeiten…")
        .scroll_to_me();
    h.run_steps(6);
    h.get_by_role_and_label(Role::Button, "Eigenes Design bearbeiten…")
        .click();
    h.run_steps(3);
    h.get_by_label("Designname");
    h.get_by_label("Themenrolle");
    let save = h
        .get_by_role_and_label(Role::Button, "In Bibliothek speichern")
        .rect();
    assert!(h.ctx.content_rect().contains_rect(save), "{save:?}");
    h.get_by_role_and_label(Role::Button, "In Bibliothek speichern")
        .click();
    h.run_steps(3);
    assert_eq!(
        h.get_by_role_and_label(Role::Button, "In Bibliothek speichern")
            .rect(),
        save
    );
    assert_eq!(h.state().settings.custom_themes.len(), 1);
    h.get_by_role_and_label(Role::Button, "Design übernehmen")
        .click();
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .theme
            .custom_theme
            .is_some()
    );
}
