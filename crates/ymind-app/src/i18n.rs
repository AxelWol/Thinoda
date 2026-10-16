//! Minimal UI translation layer.
//!
//! English strings are used as keys; other languages map them to translations and
//! fall back to English when a translation is missing.

use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use ymind_platform::settings::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    De,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Resolves the configured language (Auto = operating system locale).
pub fn resolve(lang: Language) -> Lang {
    match lang {
        Language::English => Lang::En,
        Language::German => Lang::De,
        Language::Auto => {
            let loc = sys_locale::get_locale()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if loc.starts_with("de") {
                Lang::De
            } else {
                Lang::En
            }
        }
    }
}

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 1 {
        Lang::De
    } else {
        Lang::En
    }
}

/// Translates an English UI string into the current language.
pub fn t(en: &'static str) -> &'static str {
    t_in(lang(), en)
}

pub fn t_in(lang: Lang, en: &'static str) -> &'static str {
    match lang {
        Lang::En => en,
        Lang::De => german().get(en).copied().unwrap_or(en),
    }
}

fn german() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| DE.iter().copied().collect())
}

/// English → German.
const DE: &[(&str, &str)] = &[
    // Menu / file
    ("File", "Datei"),
    ("New", "Neu"),
    ("New from Template…", "Neu aus Vorlage…"),
    ("Open…", "Öffnen…"),
    ("Open Recent", "Zuletzt geöffnet"),
    ("Clear Recent", "Liste leeren"),
    ("Save", "Speichern"),
    ("Save As…", "Speichern unter…"),
    ("Save All", "Alle speichern"),
    ("Close", "Schließen"),
    ("Import", "Importieren"),
    ("Export", "Exportieren"),
    ("Settings…", "Einstellungen…"),
    ("Settings", "Einstellungen"),
    ("Help", "Hilfe"),
    ("Keyboard Shortcuts", "Tastenkürzel"),
    ("About yMind", "Über yMind"),
    ("Quit", "Beenden"),
    ("Edit", "Bearbeiten"),
    ("Undo", "Rückgängig"),
    ("Redo", "Wiederholen"),
    ("Cut", "Ausschneiden"),
    ("Copy", "Kopieren"),
    ("Paste", "Einfügen"),
    ("Delete", "Löschen"),
    ("Delete Topic Only", "Nur Thema löschen"),
    ("Duplicate", "Duplizieren"),
    ("Select All", "Alles auswählen"),
    ("Find…", "Suchen…"),
    ("Find & Replace", "Suchen & Ersetzen"),
    ("Command Palette…", "Befehlspalette…"),
    ("View", "Ansicht"),
    ("Insert", "Einfügen"),
    ("Mind Map", "Mindmap"),
    ("Outliner", "Gliederung"),
    ("Zen Mode", "Zen-Modus"),
    ("Format Panel", "Formatbereich"),
    ("Status Bar", "Statusleiste"),
    ("Zoom In", "Vergrößern"),
    ("Zoom Out", "Verkleinern"),
    ("Actual Size", "Originalgröße"),
    ("Fit Map", "Map einpassen"),
    ("Expand All", "Alle aufklappen"),
    ("Collapse All", "Alle zuklappen"),
    ("Collapse / Expand", "Zu-/Aufklappen"),
    ("Collapse / Expand All", "Alle zu-/aufklappen"),
    ("Go to Central Topic", "Zum Hauptthema"),
    // Topic insertion
    ("Topic", "Thema"),
    ("Subtopic", "Unterthema"),
    ("Topic Before", "Thema davor"),
    ("Parent Topic", "Übergeordnetes Thema"),
    ("Floating Topic", "Freies Thema"),
    ("Relationship", "Beziehung"),
    ("Summary", "Zusammenfassung"),
    ("Boundary", "Umrandung"),
    ("Marker", "Marker"),
    ("Markers", "Marker"),
    ("Label", "Label"),
    ("Labels", "Labels"),
    ("Notes", "Notizen"),
    ("Link", "Link"),
    ("Hyperlink", "Hyperlink"),
    ("To-Do", "Aufgabe"),
    ("Open Link", "Link öffnen"),
    ("Main Topic", "Hauptthema"),
    ("Central Topic", "Zentrales Thema"),
    // Format panel
    ("Style", "Stil"),
    ("Map", "Map"),
    ("Structure", "Struktur"),
    ("Shape", "Form"),
    ("Fill", "Füllung"),
    ("Border", "Rahmen"),
    ("Text", "Text"),
    ("Font Size", "Schriftgröße"),
    ("Text Color", "Textfarbe"),
    ("Align", "Ausrichtung"),
    ("Left", "Links"),
    ("Center", "Mitte"),
    ("Right", "Rechts"),
    ("Branch", "Zweig"),
    ("Line", "Linie"),
    ("Line Color", "Linienfarbe"),
    ("Line Width", "Linienbreite"),
    ("Width", "Breite"),
    ("Auto", "Auto"),
    ("Reset Style", "Stil zurücksetzen"),
    ("Theme", "Design"),
    ("Color Variant", "Farbvariante"),
    ("Multicolor", "Mehrfarbig"),
    ("Background", "Hintergrund"),
    ("Rainbow Branches", "Regenbogen-Zweige"),
    ("Tapered Lines", "Verjüngte Linien"),
    ("Balanced Map", "Ausgewogene Map"),
    ("Topic Properties", "Themeneigenschaften"),
    (
        "Select a topic to edit its style.",
        "Wähle ein Thema, um seinen Stil zu bearbeiten.",
    ),
    ("comma separated", "durch Komma getrennt"),
    // Sheets / status
    ("Sheet", "Blatt"),
    ("New Sheet", "Neues Blatt"),
    ("Rename Sheet", "Blatt umbenennen"),
    ("Duplicate Sheet", "Blatt duplizieren"),
    ("Delete Sheet", "Blatt löschen"),
    ("Move Left", "Nach links"),
    ("Move Right", "Nach rechts"),
    ("Topics", "Themen"),
    ("Words", "Wörter"),
    ("Saved", "Gespeichert"),
    ("Unsaved changes", "Ungespeicherte Änderungen"),
    ("Untitled", "Unbenannt"),
    // Search
    ("Search", "Suchen"),
    ("Replace", "Ersetzen"),
    ("Replace All", "Alle ersetzen"),
    ("Match case", "Groß-/Kleinschreibung"),
    ("Whole word", "Ganzes Wort"),
    ("Include notes", "Notizen einbeziehen"),
    ("Dim others", "Andere abblenden"),
    ("results", "Treffer"),
    ("No results", "Keine Treffer"),
    ("Type a command…", "Befehl eingeben…"),
    // Home
    ("Create a new map", "Neue Map erstellen"),
    ("Recent", "Zuletzt verwendet"),
    ("No recent files", "Keine zuletzt verwendeten Dateien"),
    ("Templates", "Vorlagen"),
    ("Blank Map", "Leere Map"),
    ("Open File…", "Datei öffnen…"),
    // Dialogs
    ("Cancel", "Abbrechen"),
    ("OK", "OK"),
    ("Don't Save", "Nicht speichern"),
    ("Discard", "Verwerfen"),
    ("Restore", "Wiederherstellen"),
    ("Restore All", "Alle wiederherstellen"),
    ("Discard All", "Alle verwerfen"),
    (
        "Recover Unsaved Work",
        "Ungespeicherte Arbeit wiederherstellen",
    ),
    (
        "yMind was not closed properly. The following documents can be restored:",
        "yMind wurde nicht ordnungsgemäß beendet. Folgende Dokumente können wiederhergestellt werden:",
    ),
    ("Save changes?", "Änderungen speichern?"),
    (
        "Do you want to save the changes you made? Your changes will be lost if you don't save them.",
        "Möchtest du die Änderungen speichern? Nicht gespeicherte Änderungen gehen verloren.",
    ),
    ("File changed on disk", "Datei wurde extern geändert"),
    (
        "The file was modified by another application or a sync client. Overwrite it?",
        "Die Datei wurde von einer anderen Anwendung oder einem Sync-Client geändert. Überschreiben?",
    ),
    ("Overwrite", "Überschreiben"),
    ("Save As Copy", "Als Kopie speichern"),
    // Settings
    ("General", "Allgemein"),
    ("Appearance", "Darstellung"),
    ("Files", "Dateien"),
    ("Cloud", "Cloud"),
    ("System", "System"),
    ("Light", "Hell"),
    ("Dark", "Dunkel"),
    ("Language", "Sprache"),
    ("Accent Color", "Akzentfarbe"),
    ("UI Scale", "UI-Skalierung"),
    ("Autosave", "Automatisch speichern"),
    ("Autosave interval (s)", "Autosave-Intervall (s)"),
    ("Keep backup on save", "Sicherungskopie beim Speichern"),
    ("Smooth zoom", "Weiches Zoomen"),
    ("Confirm before deleting", "Löschen bestätigen"),
    ("Detected cloud folders", "Erkannte Cloud-Ordner"),
    ("No cloud folders detected", "Keine Cloud-Ordner gefunden"),
    ("Default save folder", "Standard-Speicherordner"),
    ("Choose…", "Wählen…"),
    ("Use", "Verwenden"),
    ("Open settings folder", "Einstellungsordner öffnen"),
    // Toasts
    ("Saved to", "Gespeichert unter"),
    ("Exported to", "Exportiert nach"),
    ("Could not open", "Konnte nicht öffnen"),
    ("Could not save", "Konnte nicht speichern"),
    ("Export failed", "Export fehlgeschlagen"),
    ("Restored", "Wiederhergestellt"),
    ("Autosaved", "Automatisch gespeichert"),
    ("Copied", "Kopiert"),
    ("Not possible here", "Hier nicht möglich"),
    (
        "Pick the target topic for the relationship",
        "Zielthema für die Beziehung wählen",
    ),
    // About
    ("Version", "Version"),
    (
        "A fast, open mind mapping app.",
        "Eine schnelle, offene Mindmapping-App.",
    ),
    ("Close Tab", "Tab schließen"),
    ("New Tab", "Neuer Tab"),
    ("Next Document", "Nächstes Dokument"),
    ("Show in Folder", "Im Ordner anzeigen"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_fallback() {
        assert_eq!(t_in(Lang::De, "Save"), "Speichern");
        assert_eq!(
            t_in(Lang::De, "Something untranslated"),
            "Something untranslated"
        );
        assert_eq!(t_in(Lang::En, "Save"), "Save");
    }

    #[test]
    fn no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in DE {
            assert!(seen.insert(*k), "duplicate key {k}");
        }
    }
}
