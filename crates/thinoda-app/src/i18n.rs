//! UI translations shared by every native build.
//!
//! English strings are used as keys; other languages map them to translations and
//! report missing translations before falling back to English. Catalog completeness
//! is checked against the English keys in tests.

use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use thinoda_platform::settings::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    De,
    Es,
    Fr,
    Da,
    Nl,
    Pt,
    It,
    Pl,
    PtBr,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Resolves the configured language (Auto = operating system locale).
pub fn resolve(lang: Language) -> Lang {
    match lang {
        Language::English => Lang::En,
        Language::German => Lang::De,
        Language::Spanish => Lang::Es,
        Language::French => Lang::Fr,
        Language::Danish => Lang::Da,
        Language::Dutch => Lang::Nl,
        Language::Portuguese => Lang::Pt,
        Language::PortugueseBrazil => Lang::PtBr,
        Language::Italian => Lang::It,
        Language::Polish => Lang::Pl,
        Language::Auto => resolve_locale(&sys_locale::get_locale().unwrap_or_default()),
    }
}

/// Match a complete language subtag, not arbitrary prefixes such as `den`.
/// Both BCP 47 and Unix locale separators are accepted; all regional variants
/// use the same catalog except Brazilian Portuguese, which has regional wording.
pub fn resolve_locale(locale: &str) -> Lang {
    let locale = locale.split(['.', '@']).next().unwrap_or("");
    let mut subtags = locale.split(['-', '_']);
    let language = subtags.next().unwrap_or("").to_ascii_lowercase();
    let region = match subtags.next() {
        Some(script) if script.len() == 4 => subtags.next(),
        region => region,
    };
    match language.as_str() {
        "de" => Lang::De,
        "es" => Lang::Es,
        "fr" => Lang::Fr,
        "da" => Lang::Da,
        "nl" => Lang::Nl,
        "pt" if region.is_some_and(|region| region.eq_ignore_ascii_case("br")) => Lang::PtBr,
        "pt" => Lang::Pt,
        "it" => Lang::It,
        "pl" => Lang::Pl,
        _ => Lang::En,
    }
}

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::De,
        2 => Lang::Es,
        3 => Lang::Fr,
        4 => Lang::Da,
        5 => Lang::Nl,
        6 => Lang::Pt,
        7 => Lang::It,
        8 => Lang::Pl,
        9 => Lang::PtBr,
        _ => Lang::En,
    }
}

/// Translates an English UI string into the current language.
pub fn t(en: &'static str) -> &'static str {
    t_in(lang(), en)
}

pub fn t_in(lang: Lang, en: &'static str) -> &'static str {
    if lang == Lang::En {
        return en;
    }
    let translated = if lang == Lang::De {
        german().get(en).copied()
    } else if lang == Lang::PtBr {
        brazilian().get(en).copied()
    } else {
        additional()
            .get(en)
            .map(|values| values[lang as usize - Lang::Es as usize])
    };
    translated.unwrap_or_else(|| {
        log::warn!("Missing {lang:?} UI translation: {en}");
        en
    })
}

/// Both Portuguese variants deliberately share wording where it is idiomatic in
/// both regions. Regional entries are explicit, reviewed overrides, not fallbacks.
fn brazilian() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map: HashMap<_, _> = additional()
            .iter()
            .map(|(&key, values)| (key, values[4]))
            .collect();
        let mut seen = std::collections::HashSet::new();
        for line in include_str!("pt-BR.txt")
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
        {
            let (key, text) = line
                .split_once('|')
                .expect("Invalid Brazilian Portuguese translation");
            assert!(
                map.contains_key(key),
                "Unknown Brazilian Portuguese key: {key}"
            );
            assert!(
                seen.insert(key),
                "Duplicate Brazilian Portuguese key: {key}"
            );
            assert!(
                !text.trim().is_empty() && !text.contains('|'),
                "Invalid Brazilian Portuguese translation: {key}"
            );
            map.insert(key, text);
        }
        map
    })
}

fn additional() -> &'static HashMap<&'static str, [&'static str; 7]> {
    static MAP: OnceLock<HashMap<&'static str, [&'static str; 7]>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = HashMap::new();
        for line in include_str!("locales.txt")
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
        {
            let fields: Vec<_> = line.split('|').collect();
            assert_eq!(fields.len(), 8, "Invalid translation row: {line}");
            assert!(
                fields.iter().all(|value| !value.trim().is_empty()),
                "Empty translation: {line}"
            );
            let values = std::array::from_fn(|i| fields[i + 1]);
            assert!(
                map.insert(fields[0], values).is_none(),
                "Duplicate translation: {}",
                fields[0]
            );
        }
        map
    })
}

fn german() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| DE.iter().copied().collect())
}

/// English → German.
const DE: &[(&str, &str)] = &[
    ("File name", "Dateiname"),
    ("Download failed", "Download fehlgeschlagen"),
    ("Download started", "Download gestartet"),
    (
        "Close the browser tab to quit",
        "Zum Beenden den Browser-Tab schließen",
    ),
    (
        "Browser print dialog opened",
        "Druckdialog des Browsers geöffnet",
    ),
    (
        "Continue opens the browser print dialog. Select the same paper size and orientation there and set its margins to none or default; the page already contains the margins chosen here. Use Save print PDF… for exact page output.",
        "Weiter öffnet den Druckdialog des Browsers. Wählen Sie dort dasselbe Papierformat und dieselbe Ausrichtung und stellen Sie die Ränder auf „Keine“ oder „Standard“; die Seite enthält bereits die hier gewählten Ränder. Verwenden Sie „Druck-PDF speichern…“ für eine exakte Seitenausgabe.",
    ),
    (
        "MindManager export cannot preserve all content. Use --allow-lossy only if you accept omitted content.",
        "Der MindManager-Export kann nicht alle Inhalte erhalten. --allow-lossy nur verwenden, wenn Sie ausgelassene Inhalte akzeptieren.",
    ),
    (
        "Could not write this MindManager file. The map may be too large or contain invalid data.",
        "Diese MindManager-Datei konnte nicht geschrieben werden. Die Map ist möglicherweise zu groß oder enthält ungültige Daten.",
    ),
    (
        "Theme colors and appearance may differ. Use .thinoda to preserve all Thinoda data.",
        "Designfarben und Darstellung können abweichen. Verwenden Sie .thinoda, um alle Thinoda-Daten zu erhalten.",
    ),
    ("MindManager XML", "MindManager XML"),
    (
        "Import requires confirmation",
        "Import erfordert Bestätigung",
    ),
    (
        "Could not read this MindManager file. It may be invalid, encrypted, too large or unsafe.",
        "Diese MindManager-Datei konnte nicht gelesen werden. Sie ist möglicherweise ungültig, verschlüsselt, zu groß oder unsicher.",
    ),
    (
        "Choose a .thinoda, .xmind or .mmap destination.",
        "Ein Ziel mit .thinoda, .xmind oder .mmap wählen.",
    ),
    (
        "MindManager saves one sheet. Export the current sheet or save the complete workbook as .thinoda.",
        "MindManager speichert ein Blatt. Das aktuelle Blatt exportieren oder die vollständige Arbeitsmappe als .thinoda speichern.",
    ),
    (
        "Choose a different destination to preserve the imported source.",
        "Ein anderes Ziel wählen, um die importierte Quelldatei zu erhalten.",
    ),
    (
        "Import requires confirmation. Use --allow-lossy only if you accept omitted content.",
        "Import erfordert Bestätigung. --allow-lossy nur verwenden, wenn ausgelassene Inhalte akzeptiert werden.",
    ),
    (
        "MindManager conversion may omit unsupported content. The source file is preserved.",
        "Die MindManager-Konvertierung kann nicht unterstützte Inhalte auslassen. Die Quelldatei bleibt erhalten.",
    ),
    (
        "MindManager export cannot preserve all content. Use Export with confirmation, or save as .thinoda.",
        "Der MindManager-Export kann nicht alle Inhalte erhalten. Export mit Bestätigung verwenden oder als .thinoda speichern.",
    ),
    ("Export MindManager copy", "MindManager-Kopie exportieren"),
    (
        "Only the current sheet is exported. Colors, assets, relationships, advanced layout and unsupported data may be omitted. The source workbook is unchanged.",
        "Nur das aktuelle Blatt wird exportiert. Farben, Ressourcen, Beziehungen, erweiterte Layouts und nicht unterstützte Daten können ausgelassen werden. Die Quellarbeitsmappe bleibt unverändert.",
    ),
    ("Import converted copy", "Konvertierte Kopie importieren"),
    ("Formatting and styles", "Formatierung und Stile"),
    ("Rich notes", "Formatierte Notizen"),
    ("Images and attachments", "Bilder und Anhänge"),
    ("Markers and task data", "Markierungen und Aufgabendaten"),
    (
        "Additional or unsupported links",
        "Zusätzliche oder nicht unterstützte Links",
    ),
    (
        "Other unsupported map data",
        "Weitere nicht unterstützte Map-Daten",
    ),
    ("Symbol color", "Symbolfarbe"),
    ("Default symbol color", "Standard-Symbolfarbe"),
    ("Placed symbols", "Platzierte Symbole"),
    ("Remove Symbol", "Symbol entfernen"),
    ("Contrast", "Kontrast"),
    ("Pass", "Erfüllt"),
    ("Low contrast", "Kontrast zu gering"),
    ("Improve contrast", "Kontrast verbessern"),
    ("Invalid color", "Ungültige Farbe"),
    (
        "Contrast compares the symbol with its topic background; it is not an overall accessibility assessment.",
        "Der Kontrast vergleicht das Symbol mit seinem Themenhintergrund; dies ist keine umfassende Bewertung der Barrierefreiheit.",
    ),
    (
        "Graphical symbols: 3:1; month and week text: 4.5:1.",
        "Grafische Symbole: 3:1; Monats- und Wochentext: 4.5:1.",
    ),
    (
        "Editable exports lose symbol colors, except Thinoda and unchanged XMind files reopened in Thinoda. XMind itself shows standard icons; third-party edits can discard custom colors.",
        "Bearbeitbare Exporte verlieren Symbolfarben, außer im Thinoda-Format und bei unveränderten XMind-Dateien, die erneut in Thinoda geöffnet werden. XMind selbst zeigt Standardsymbole; Änderungen in anderen Anwendungen können benutzerdefinierte Farben verwerfen.",
    ),
    ("Plain text", "Klartext"),
    ("Thinoda", "Thinoda"),
    ("XMind", "XMind"),
    ("FreeMind / Freeplane", "FreeMind / Freeplane"),
    ("OPML", "OPML"),
    ("Markdown", "Markdown"),
    ("MindManager", "MindManager"),
    ("TextBundle", "TextBundle"),
    ("SVG", "SVG"),
    ("PNG", "PNG"),
    ("JPEG", "JPEG"),
    ("PDF", "PDF"),
    ("Word", "Word"),
    ("Excel", "Excel"),
    ("CSV", "CSV"),
    ("Legal documents", "Rechtliche Dokumente"),
    ("License", "Lizenz"),
    ("Third-party notices", "Drittanbieterhinweise"),
    ("Legal notice", "Impressum"),
    (
        "Original legal texts are shown unchanged.",
        "Die rechtlichen Originaltexte werden unverändert angezeigt.",
    ),
    ("Rounded rectangle", "Abgerundetes Rechteck"),
    ("Rectangle", "Rechteck"),
    ("Ellipse", "Ellipse"),
    ("Capsule", "Kapselform"),
    ("Diamond", "Raute"),
    ("Curve", "Kurve"),
    ("Straight", "Gerade"),
    ("Elbow", "Rechtwinklig"),
    ("Rounded elbow", "Abgerundet rechtwinklig"),
    ("Clarity", "Klarheit"),
    ("Twilight", "Dämmerung"),
    ("Ocean", "Ozean"),
    ("Forest", "Wald"),
    ("Classic", "Klassisch"),
    ("Monochrome", "Monochrom"),
    ("Neon", "Neon"),
    ("Pastel", "Pastell"),
    ("Print", "Drucken"),
    (
        "Print (Ctrl+P) previews the current sheet on one page, including offscreen topics. Set paper, orientation, margins, source resolution and colors, then open the native OS printer dialog or save a PDF. Choose a printer and its driver settings there. The whole preview page fits proportionally inside the selected printable area; OS paper changes scale it without reflow. Submitted means accepted by the OS, not physically printed. Native dialogs use the OS language and appearance.",
        "Drucken (Strg+P) zeigt das aktuelle Blatt einschließlich der Themen außerhalb des Bildschirms auf einer Seite an. Papierformat, Ausrichtung, Ränder, Quellauflösung und Farben einstellen und dann den nativen Betriebssystem-Druckdialog öffnen oder ein PDF speichern. Dort Drucker und Treibereinstellungen wählen. Die gesamte Vorschauseite wird proportional in den gewählten bedruckbaren Bereich eingepasst; Änderungen am Papierformat skalieren sie ohne neuen Umbruch. Übermittelt bedeutet vom Betriebssystem angenommen, nicht tatsächlich ausgedruckt. Native Dialoge verwenden Sprache und Darstellung des Betriebssystems.",
    ),
    ("Retry preview", "Vorschau erneut versuchen"),
    ("Page setup", "Seite einrichten"),
    ("Paper size", "Papierformat"),
    ("A4", "A4"),
    ("A3", "A3"),
    ("Letter", "US Letter"),
    ("Legal", "US Legal"),
    ("Orientation", "Ausrichtung"),
    ("Portrait", "Hochformat"),
    ("Landscape", "Querformat"),
    ("Margins", "Seitenränder"),
    ("Resolution", "Auflösung"),
    ("Map appearance", "Darstellung der Map"),
    ("Dark map", "Dunkle Map"),
    ("Map background", "Map-Hintergrund"),
    ("Grayscale", "Graustufen"),
    ("Print preview", "Druckvorschau"),
    ("1 page", "1 Seite"),
    (
        "Preparing print preview…",
        "Druckvorschau wird vorbereitet…",
    ),
    (
        "Could not prepare print preview",
        "Druckvorschau konnte nicht vorbereitet werden",
    ),
    (
        "Print worker stopped unexpectedly",
        "Druckverarbeitung wurde unerwartet beendet",
    ),
    ("Printing failed", "Drucken fehlgeschlagen"),
    (
        "Preparing print document…",
        "Druckdokument wird vorbereitet…",
    ),
    ("Continue to print…", "Weiter zum Drucken…"),
    ("Save print PDF…", "Druck-PDF speichern…"),
    ("Print PDF saved", "Druck-PDF gespeichert"),
    (
        "Print PDF must be saved with a .pdf extension",
        "Druck-PDF muss mit der Dateiendung .pdf gespeichert werden",
    ),
    (
        "Fit the current sheet on one page. Offscreen topics are included; collapsed branches stay collapsed.",
        "Das aktuelle Blatt wird auf eine Seite eingepasst. Themen außerhalb des Bildschirms sind enthalten; eingeklappte Zweige bleiben eingeklappt.",
    ),
    (
        "Continue opens the OS print dialog with printer selection and driver settings. The whole preview page, including margins, is fitted proportionally inside the selected printable area. Changing OS paper or orientation scales the page without reflow; driver options may further change output. Resolution here controls source detail, not printer quality.",
        "Weiter öffnet den Betriebssystem-Druckdialog mit Druckerauswahl und Treibereinstellungen. Die gesamte Vorschauseite samt Rändern wird proportional in den gewählten bedruckbaren Bereich eingepasst. Papierformat oder Ausrichtung im Systemdialog skalieren die Seite ohne neuen Umbruch; Treiberoptionen können die Ausgabe zusätzlich verändern. Die Auflösung hier bestimmt den Detailgrad der Quelle, nicht die Druckqualität.",
    ),
    ("Printing cancelled", "Drucken abgebrochen"),
    (
        "Print job submitted to the OS. Physical output is not confirmed.",
        "Druckauftrag an das Betriebssystem übermittelt. Ein tatsächlicher Ausdruck ist nicht bestätigt.",
    ),
    (
        "Map Shot must be saved with a .png extension",
        "Map Shot muss mit der Dateiendung .png gespeichert werden",
    ),
    ("Map Shot", "Map Shot"),
    ("Map Shot preview", "Map-Shot-Vorschau"),
    ("Capturing visible map…", "Sichtbare Map wird aufgenommen…"),
    (
        "Visible map only. Drag a region or edit pixel coordinates. Use Export for the complete sheet.",
        "Nur die sichtbare Map. Bereich ziehen oder Pixelkoordinaten bearbeiten. Für das vollständige Blatt den Export verwenden.",
    ),
    ("Square (1:1)", "Quadratisch (1:1)"),
    ("Reset region", "Bereich zurücksetzen"),
    ("Region X", "Bereich X"),
    ("Region Y", "Bereich Y"),
    ("Region width", "Bereichsbreite"),
    ("Region height", "Bereichshöhe"),
    ("Frame thickness", "Rahmenstärke"),
    ("Frame color", "Rahmenfarbe"),
    ("Copy image", "Bild kopieren"),
    ("Save PNG…", "PNG speichern…"),
    (
        "Image sent to system clipboard",
        "Bild an die System-Zwischenablage gesendet",
    ),
    (
        "Map Shot capture failed",
        "Map-Shot-Aufnahme fehlgeschlagen",
    ),
    (
        "Map Shot capture timed out; this renderer may not support screenshots",
        "Zeitüberschreitung bei Map Shot; dieser Renderer unterstützt möglicherweise keine Bildschirmaufnahmen",
    ),
    ("Invalid Map Shot region", "Ungültiger Map-Shot-Bereich"),
    (
        "Map Shot image exceeds limits or has invalid dimensions",
        "Map-Shot-Bild überschreitet Grenzwerte oder hat ungültige Abmessungen",
    ),
    ("Editor", "Editor"),
    ("Indent", "Einrücken"),
    ("Outdent", "Ausrücken"),
    ("Tools", "Werkzeuge"),
    ("All", "Alle"),
    ("Create", "Erstellen"),
    ("Basic", "Grundlagen"),
    ("Knowledge management", "Wissensmanagement"),
    ("Meeting and planning", "Besprechung und Planung"),
    ("Study and education", "Lernen und Bildung"),
    ("Project management", "Projektmanagement"),
    ("Entertainment and life", "Freizeit und Alltag"),
    ("Analysis and decision", "Analyse und Entscheidung"),
    ("Recently used templates", "Zuletzt verwendete Vorlagen"),
    ("Recent template", "Zuletzt verwendete Vorlage"),
    ("Template Category", "Vorlagenkategorie"),
    ("Browse templates", "Vorlagen durchsuchen"),
    ("All templates", "Alle Vorlagen"),
    (
        "Select a template, then choose Create.",
        "Vorlage auswählen und dann Erstellen wählen.",
    ),
    ("Unknown template", "Unbekannte Vorlage"),
    ("Logic Chart", "Logikdiagramm"),
    ("Brace Map", "Klammerdiagramm"),
    ("Org Chart", "Organigramm"),
    ("Tree Chart", "Baumdiagramm"),
    ("Timeline", "Zeitachse"),
    ("Fishbone", "Fischgräte"),
    ("SWOT Analysis", "SWOT-Analyse"),
    ("Meeting Notes", "Besprechungsnotizen"),
    ("Project Plan", "Projektplan"),
    ("Knowledge Base", "Wissensbasis"),
    ("Learning Plan", "Lernplan"),
    ("Weekly Plan", "Wochenplan"),
    ("Decision Review", "Entscheidungsprüfung"),
    ("Creative Workflow", "Kreativer Arbeitsablauf"),
    ("Problem Solving", "Problemlösung"),
    ("Concept Framework", "Begriffsrahmen"),
    ("Software Architecture", "Softwarearchitektur"),
    ("Business Plan", "Geschäftsplan"),
    ("Workshop", "Workshop"),
    ("Dissertation Plan", "Dissertationsplan"),
    ("School Timetable", "Stundenplan"),
    ("Reading Notes", "Lesenotizen"),
    ("Language Learning", "Sprachenlernen"),
    ("Project Retrospective", "Projektrückblick"),
    ("Shopping List", "Einkaufsliste"),
    ("Mood Journal", "Stimmungstagebuch"),
    ("Service Comparison", "Dienstleistungsvergleich"),
    ("Cause and Effect", "Ursache und Wirkung"),
    ("Comparison", "Vergleich"),
    ("Social Media Plan", "Social-Media-Plan"),
    ("Recipe", "Rezept"),
    ("Film Production", "Filmproduktion"),
    ("User Guide", "Benutzerhandbuch"),
    ("Quick Start", "Schnellstart"),
    ("Send Feedback…", "Feedback senden…"),
    ("Project News…", "Projektneuigkeiten…"),
    ("Guide section", "Handbuchabschnitt"),
    ("All sections", "Alle Abschnitte"),
    ("Search the guide", "Handbuch durchsuchen"),
    (
        "No matching guide sections",
        "Keine passenden Handbuchabschnitte",
    ),
    ("Editing and Navigation", "Bearbeitung und Navigation"),
    ("Styles and Themes", "Stile und Designs"),
    ("Files and Recovery", "Dateien und Wiederherstellung"),
    ("Import and Export", "Import und Export"),
    ("Platform and Privacy", "Plattform und Datenschutz"),
    (
        "Create a blank map with Ctrl+N, or open Home, select a template and choose Create. Cancel returns without creating a document.",
        "Mit Strg+N eine leere Map erstellen oder auf der Startseite eine Vorlage auswählen und Erstellen wählen. Abbrechen kehrt ohne neues Dokument zurück.",
    ),
    (
        "Select a topic. Tab adds a child; Enter adds a sibling. Space or F2 edits its title. Escape cancels an edit or clears selection.",
        "Ein Thema auswählen. Tab fügt ein Unterthema hinzu, Enter ein Geschwisterthema. Leertaste oder F2 bearbeitet den Titel. Escape bricht die Bearbeitung ab oder hebt die Auswahl auf.",
    ),
    (
        "Use the Style sidebar for topic appearance and the Map sidebar for layout and theme. Select a topic to see its current properties.",
        "Die Stil-Seitenleiste steuert das Aussehen eines Themas, die Map-Seitenleiste Layout und Design. Ein Thema auswählen, um seine aktuellen Eigenschaften zu sehen.",
    ),
    (
        "Save with Ctrl+S to a .thinoda file. Use .xmind for interchange. Save As creates a separate destination; export formats may omit unsupported features.",
        "Mit Strg+S als .thinoda speichern. Für den Austausch .xmind verwenden. Speichern unter wählt ein separates Ziel; Exportformate können nicht unterstützte Funktionen auslassen.",
    ),
    (
        "Use Ctrl+Z to undo map edits. On macOS, use Command instead of Ctrl for command shortcuts. Help > Keyboard Shortcuts lists the available bindings.",
        "Strg+Z macht Map-Änderungen rückgängig. Unter macOS für Befehlstastenkürzel Command statt Strg verwenden. Hilfe > Tastenkürzel zeigt die verfügbaren Belegungen.",
    ),
    (
        "Drag a topic onto a parent or sibling insertion slot to move its branch. The preview shows the proposed placement; Escape cancels the drag.",
        "Ein Thema auf ein übergeordnetes Thema oder einen Einfügeplatz zwischen Geschwistern ziehen, um den Zweig zu verschieben. Die Vorschau zeigt das Ziel; Escape bricht das Ziehen ab.",
    ),
    (
        "Relationships connect topics. Boundaries group siblings; summaries attach a separate summary topic to a sibling range. Select their line or outline to edit or delete the object.",
        "Beziehungen verbinden Themen. Umrandungen gruppieren Geschwister; Zusammenfassungen verknüpfen einen Geschwisterbereich mit einem eigenen Thema. Linie oder Umriss auswählen, um das Objekt zu bearbeiten oder zu löschen.",
    ),
    (
        "Labels organize topics without changing their titles. Notes and hyperlinks are available in the sidebar. Ctrl+F searches; Ctrl+H opens replacement, with optional all-sheet scope.",
        "Labels ordnen Themen ohne Titeländerung. Notizen und Hyperlinks sind in der Seitenleiste verfügbar. Strg+F sucht; Strg+H öffnet das Ersetzen, wahlweise über alle Blätter.",
    ),
    (
        "Sheet tabs support rename, duplicate, copy, paste, delete and separate save. New Sheet from Topic copies a branch; external relationships stay in the original sheet.",
        "Blattregister erlauben Umbenennen, Duplizieren, Kopieren, Einfügen, Löschen und separates Speichern. Neues Blatt aus Thema kopiert einen Zweig; externe Beziehungen bleiben im Ursprungsblatt.",
    ),
    (
        "Topic styles override map and theme defaults. Reset Style removes explicit topic overrides. Copy/Paste Style transfers appearance, not text or hierarchy.",
        "Themenstile haben Vorrang vor Map- und Designvorgaben. Stil zurücksetzen entfernt explizite Themenvorgaben. Stil kopieren/einfügen überträgt Aussehen, nicht Text oder Hierarchie.",
    ),
    (
        "Quick Styles offers original presets and named local styles captured from resolved appearance. Applying a style to several selected topics is one undo step.",
        "Schnellstile bieten eigene Vorlagen und benannte lokale Stile aus dem aufgelösten Aussehen. Ein Stil für mehrere ausgewählte Themen wird in einem Schritt rückgängig gemacht.",
    ),
    (
        "Map > Custom Themes opens a staged editor. Apply Theme embeds the theme in the sheet; Save to Library stores a separate local copy without changing the map.",
        "Map > Eigene Designs öffnet einen Entwurfseditor. Design übernehmen bettet das Design im Blatt ein; In Bibliothek speichern legt eine separate lokale Kopie ohne Map-Änderung ab.",
    ),
    (
        "Custom themes provide palette, light/dark backgrounds, fonts and role defaults. Explicit map and topic overrides take precedence. Removing a library theme does not change existing maps.",
        "Eigene Designs enthalten Palette, helle/dunkle Hintergründe, Schriften und Rollenvorgaben. Explizite Map- und Themenvorgaben haben Vorrang. Das Entfernen eines Bibliotheksdesigns ändert keine bestehenden Maps.",
    ),
    (
        "Select a built-in theme to remove custom defaults. Optional recent-theme reuse affects new blank maps, not named templates. Missing local fonts keep their names and show a warning.",
        "Ein integriertes Design auswählen, um eigene Designvorgaben zu entfernen. Die optionale Wiederverwendung des letzten Designs gilt für neue leere Maps, nicht benannte Vorlagen. Fehlende lokale Schriften behalten ihren Namen und zeigen eine Warnung.",
    ),
    (
        "Use .thinoda for full-fidelity native storage. Crash-recovery snapshots are stored locally; available recovery copies are offered at startup. Recovery is not a substitute for saving or backups.",
        "Für vollständige native Speicherung .thinoda verwenden. Wiederherstellungskopien werden lokal gespeichert und beim Start angeboten. Wiederherstellung ersetzt weder Speichern noch Sicherungen.",
    ),
    (
        "When an open file changes externally, Thinoda offers conflict handling rather than silently replacing your edits. Inspect both versions before deciding which to keep.",
        "Wird eine geöffnete Datei extern geändert, bietet Thinoda eine Konfliktbehandlung statt Ihre Änderungen still zu ersetzen. Beide Versionen prüfen, bevor Sie eine behalten.",
    ),
    (
        "Cloud-folder support uses locally synchronized folders supplied by your cloud client. OAuth sharing and live collaboration are not implemented.",
        "Cloud-Ordner verwenden lokal synchronisierte Verzeichnisse Ihres Cloud-Clients. OAuth-Freigaben und Live-Zusammenarbeit sind nicht implementiert.",
    ),
    (
        "Merge Map Files appends sheets only after every input loads successfully. Create Masked Copy creates an unsaved preview with identifying text removed; inspect every sheet before sharing because visual structure can still reveal information.",
        "Map-Dateien zusammenführen hängt Blätter erst nach erfolgreichem Laden aller Eingaben an. Maskierte Kopie erstellen öffnet eine ungespeicherte Vorschau ohne identifizierende Texte; vor dem Teilen jedes Blatt prüfen, da die Struktur weiterhin Informationen verraten kann.",
    ),
    (
        "The Open and Export menus list supported formats. Native and unchanged Thinoda-exported XMind files preserve Thinoda-specific data. Other applications may not understand those extensions.",
        "Öffnen und Exportieren zeigen unterstützte Formate. Native und unveränderte von Thinoda exportierte XMind-Dateien erhalten Thinoda-spezifische Daten. Andere Anwendungen verstehen diese Erweiterungen möglicherweise nicht.",
    ),
    (
        "Public XMind export materializes supported topic styles. Editable custom-theme registries, native curve interchange and exact XMind visual parity are not verified.",
        "Der öffentliche XMind-Export schreibt unterstützte Themenstile aus. Bearbeitbare eigene Designregister, nativer Kurvenaustausch und exakte visuelle XMind-Parität sind nicht verifiziert.",
    ),
    (
        "Map Shot (F7) captures the visible GPU-rendered canvas, with a crop preview and PNG or clipboard output. Export renders the complete sheet, including offscreen topics.",
        "Map Shot (F7) erfasst die sichtbare GPU-gerenderte Zeichenfläche mit Zuschnittvorschau und PNG- oder Zwischenablageausgabe. Export rendert das gesamte Blatt einschließlich nicht sichtbarer Themen.",
    ),
    (
        "External image references are not embedded attachments. Check imported content and exported files before relying on them in another application.",
        "Externe Bildverweise sind keine eingebetteten Anhänge. Importierte Inhalte und exportierte Dateien vor der Verwendung in einer anderen Anwendung prüfen.",
    ),
    (
        "Settings controls system/light/dark appearance, language, scaling and editor preferences. Installed fonts are local to each machine; companion devices may need the same fonts.",
        "Einstellungen steuern System-/Hell-/Dunkelmodus, Sprache, Skalierung und Editoroptionen. Installierte Schriften sind lokal; andere Geräte benötigen möglicherweise dieselben Schriften.",
    ),
    (
        "AI commands require a configured provider and may send selected topic content to that provider. Review provider settings and do not submit confidential content without permission.",
        "KI-Befehle benötigen einen konfigurierten Anbieter und können ausgewählte Themeninhalte an ihn senden. Anbietereinstellungen prüfen und vertrauliche Inhalte nicht ohne Erlaubnis übermitteln.",
    ),
    (
        "Send Feedback opens the public GitHub issue chooser in your browser. It does not upload maps, settings or logs. Review anything you include before submitting an issue.",
        "Feedback senden öffnet die öffentliche GitHub-Issue-Auswahl im Browser. Maps, Einstellungen und Protokolle werden nicht hochgeladen. Alle Angaben vor dem Absenden prüfen.",
    ),
    (
        "Project News opens the repository commit history, not an automatic updater. Signing, native platform installation checks and additional cloud services remain under development.",
        "Projektneuigkeiten öffnet den Commit-Verlauf des Repositorys, keinen automatischen Updater. Signierung, native Installationsprüfungen und zusätzliche Cloud-Dienste sind noch in Entwicklung.",
    ),
    ("Choose Structure", "Struktur auswählen"),
    ("Mind maps", "Mindmaps"),
    ("Logic charts", "Logikdiagramme"),
    ("Brace maps", "Klammerdiagramme"),
    ("Org charts", "Organigramme"),
    ("Tree charts", "Baumdiagramme"),
    ("Timelines", "Zeitachsen"),
    ("Fishbones", "Fischgrätendiagramme"),
    ("Tree tables", "Baumtabellen"),
    ("Matrices", "Matrizen"),
    (
        "Mind Map (anticlockwise)",
        "Mindmap (gegen den Uhrzeigersinn)",
    ),
    ("Mind Map (one side)", "Mindmap (einseitig)"),
    ("Logic Chart (right)", "Logikdiagramm (rechts)"),
    ("Logic Chart (left)", "Logikdiagramm (links)"),
    ("Brace Map (right)", "Klammerdiagramm (rechts)"),
    ("Brace Map (left)", "Klammerdiagramm (links)"),
    ("Org Chart (down)", "Organigramm (nach unten)"),
    ("Org Chart (up)", "Organigramm (nach oben)"),
    ("Tree Chart (right)", "Baumdiagramm (rechts)"),
    ("Tree Chart (left)", "Baumdiagramm (links)"),
    ("Timeline (horizontal)", "Zeitachse (horizontal)"),
    ("Timeline (vertical)", "Zeitachse (vertikal)"),
    ("Fishbone (right)", "Fischgräte (rechts)"),
    ("Fishbone (left)", "Fischgräte (links)"),
    ("Matrix", "Matrix"),
    ("Tree Table", "Baumtabelle"),
    ("Create Masked Copy…", "Maskierte Kopie erstellen…"),
    ("Create Preview", "Vorschau erstellen"),
    (
        "Masked copy created; inspect it before saving or sharing",
        "Maskierte Kopie erstellt; vor dem Speichern oder Teilen prüfen",
    ),
    (
        "Creates a new unsaved document. The original is not changed.",
        "Erstellt ein neues ungespeichertes Dokument. Das Original bleibt unverändert.",
    ),
    (
        "Replaces all topic, sheet and relationship titles. Removes notes, labels, links, images, markers, custom font names, boundary titles and document metadata.",
        "Ersetzt alle Themen-, Blatt- und Beziehungstitel. Entfernt Notizen, Labels, Links, Bilder, Symbole, eigene Schriftnamen, Umrandungstitel und Dokumentmetadaten.",
    ),
    (
        "Structure, colors and task states remain visible. This does not guarantee anonymity; inspect every sheet before sharing.",
        "Struktur, Farben und Aufgabenstatus bleiben sichtbar. Dies garantiert keine Anonymität; vor dem Teilen jedes Blatt prüfen.",
    ),
    ("Merge Map Files…", "Map-Dateien zusammenführen…"),
    (
        "Select map files and open a destination document first",
        "Zuerst Map-Dateien auswählen und ein Zieldokument öffnen",
    ),
    (
        "Choose .thinoda or .xmind files",
        ".thinoda- oder .xmind-Dateien auswählen",
    ),
    (
        "Could not merge map files",
        "Map-Dateien konnten nicht zusammengeführt werden",
    ),
    (
        "Merged sheets; original files are unchanged",
        "Blätter zusammengeführt; Originaldateien sind unverändert",
    ),
    (
        "Appends complete sheets. Destination metadata is kept; external image references are not embedded.",
        "Fügt vollständige Blätter an. Metadaten des Ziels bleiben erhalten; externe Bildreferenzen werden nicht eingebettet.",
    ),
    ("Select", "Auswählen"),
    ("Select None", "Nichts auswählen"),
    ("Invert Selection", "Auswahl umkehren"),
    ("Select Siblings", "Geschwister auswählen"),
    ("Select Descendants", "Nachkommen auswählen"),
    ("Arrange", "Anordnen"),
    ("Move Up", "Nach oben verschieben"),
    ("Move Down", "Nach unten verschieben"),
    ("Add Sibling", "Geschwisterthema hinzufügen"),
    ("Add Child", "Unterthema hinzufügen"),
    ("Edit Topic", "Thema bearbeiten"),
    ("Delete Topic", "Thema löschen"),
    ("Select a topic to edit", "Thema zum Bearbeiten auswählen"),
    ("Selected Topic", "Ausgewähltes Thema"),
    ("Topic title", "Thementitel"),
    (
        "Sort Children Ascending",
        "Unterthemen aufsteigend sortieren",
    ),
    (
        "Sort Children Descending",
        "Unterthemen absteigend sortieren",
    ),
    ("Align Horizontal Centers", "Horizontal mittig ausrichten"),
    ("Align Top", "Oben ausrichten"),
    ("Align Vertical Centers", "Vertikal mittig ausrichten"),
    ("Align Bottom", "Unten ausrichten"),
    (
        "Distribute Horizontal Centers",
        "Mittelpunkte horizontal verteilen",
    ),
    (
        "Distribute Vertical Centers",
        "Mittelpunkte vertikal verteilen",
    ),
    (
        "Alignment needs floating topics only; distribution spaces centers, not edges.",
        "Ausrichten erfordert freie Themen; verteilt werden Mittelpunkte, nicht Kanten.",
    ),
    ("New Sheet from Topic", "Neues Blatt aus Thema"),
    (
        "External relationships remain only in the original sheet",
        "Externe Beziehungen verbleiben nur im ursprünglichen Blatt",
    ),
    ("Copy Sheet", "Blatt kopieren"),
    ("Paste Sheet", "Blatt einfügen"),
    (
        "Ignored a late sheet paste; paste again to retry",
        "Verspätetes Einfügen ignoriert; zum Wiederholen erneut einfügen",
    ),
    ("Save Sheet As…", "Blatt speichern unter…"),
    (
        "Copied sheet to clipboard",
        "Blatt in die Zwischenablage kopiert",
    ),
    ("Could not copy sheet", "Blatt konnte nicht kopiert werden"),
    (
        "Could not paste sheet",
        "Blatt konnte nicht eingefügt werden",
    ),
    ("Saved sheet to", "Blatt gespeichert unter"),
    (
        "Could not save sheet",
        "Blatt konnte nicht gespeichert werden",
    ),
    (
        "Clipboard did not provide sheet data",
        "Zwischenablage hat keine Blattdaten geliefert",
    ),
    (
        "Sheet paste cancelled because the document changed or a dialog opened",
        "Einfügen des Blatts abgebrochen, da das Dokument gewechselt oder ein Dialog geöffnet wurde",
    ),
    ("Copy Style", "Stil kopieren"),
    ("Paste Style", "Stil einfügen"),
    (
        "Copied topic style overrides",
        "Themen-Stilanpassungen kopiert",
    ),
    (
        "Copies explicit overrides; inherited theme styles stay automatic.",
        "Kopiert explizite Anpassungen; geerbte Designstile bleiben automatisch.",
    ),
    ("Previous Sheet", "Vorheriges Blatt"),
    ("Next Sheet", "Nächstes Blatt"),
    ("Written by", "Geschrieben von"),
    ("Delete Relationship", "Beziehung löschen"),
    ("Delete Boundary", "Umrandung löschen"),
    ("Delete Summary", "Zusammenfassung löschen"),
    (
        "Deletes the summary topic and its children, not the covered branches.",
        "Löscht das Zusammenfassungsthema und seine Unterthemen, nicht die zusammengefassten Zweige.",
    ),
    ("Contact", "Kontakt"),
    ("Write an email", "E-Mail schreiben"),
    ("Open in web browser", "Im Webbrowser öffnen"),
    ("All sheets", "Alle Blätter"),
    ("Include labels", "Beschriftungen einbeziehen"),
    ("Replaces topic titles only.", "Ersetzt nur Thementitel."),
    ("Statistics", "Statistik"),
    ("Current sheet", "Aktuelles Blatt"),
    ("Characters", "Zeichen"),
    (
        "Topic titles only, including collapsed and floating topics.",
        "Nur Thementitel, einschließlich eingeklappter und freier Themen.",
    ),
    (
        "Words split at whitespace; characters count Unicode scalar values.",
        "Wörter sind durch Leerraum getrennt; Zeichen zählen Unicode-Skalarwerte.",
    ),
    ("Zoom", "Zoom"),
    ("Show topic toolbar", "Themen-Symbolleiste anzeigen"),
    (
        "Show topic add buttons",
        "Thema-hinzufügen-Buttons anzeigen",
    ),
    ("Initial canvas zoom", "Anfänglicher Map-Zoom"),
    (
        "Applies to new and newly opened maps, not current views.",
        "Gilt für neue und neu geöffnete Maps, nicht für aktuelle Ansichten.",
    ),
    (
        "Reuse last structure for new blank maps",
        "Letzte Struktur für neue leere Maps verwenden",
    ),
    (
        "Reuse last theme for new blank maps",
        "Letztes Design für neue leere Maps verwenden",
    ),
    ("None", "Keine"),
    ("Bold", "Fett"),
    ("Italic", "Kursiv"),
    ("Underline", "Unterstrichen"),
    ("Strikethrough", "Durchgestrichen"),
    ("Align Left", "Linksbündig"),
    ("Align Center", "Zentriert"),
    ("Align Right", "Rechtsbündig"),
    (
        "Map settings apply to the entire sheet.",
        "Map-Einstellungen gelten für das gesamte Blatt.",
    ),
    (
        "Showing the primary topic; edits apply to all selected topics.",
        "Eigenschaften des Hauptauswahlthemas; Änderungen gelten für alle ausgewählten Themen.",
    ),
    ("Symbols", "Symbole"),
    (
        "Open or select a map to show Symbols",
        "Eine Map öffnen oder auswählen, um Symbole anzuzeigen",
    ),
    (
        "Select a topic to add symbols.",
        "Ein Thema auswählen, um Symbole hinzuzufügen.",
    ),
    (
        "Click a symbol to add or remove it. One per group.",
        "Ein Symbol zum Hinzufügen oder Entfernen anklicken. Eines pro Gruppe.",
    ),
    ("Priority", "Priorität"),
    ("Task", "Aufgabe"),
    ("Flag", "Flagge"),
    ("Star", "Stern"),
    ("Smiley", "Smiley"),
    ("Symbol", "Symbol"),
    ("Month", "Monat"),
    ("Week", "Woche"),
    ("Priority 1", "Priorität 1"),
    ("Priority 2", "Priorität 2"),
    ("Priority 3", "Priorität 3"),
    ("Priority 4", "Priorität 4"),
    ("Priority 5", "Priorität 5"),
    ("Priority 6", "Priorität 6"),
    ("Priority 7", "Priorität 7"),
    ("Priority 8", "Priorität 8"),
    ("Priority 9", "Priorität 9"),
    ("Start", "Beginn"),
    ("1/8 done", "1/8 erledigt"),
    ("1/4 done", "1/4 erledigt"),
    ("3/8 done", "3/8 erledigt"),
    ("Half done", "Zur Hälfte erledigt"),
    ("5/8 done", "5/8 erledigt"),
    ("3/4 done", "3/4 erledigt"),
    ("7/8 done", "7/8 erledigt"),
    ("Done", "Erledigt"),
    ("Red flag", "Rote Flagge"),
    ("Orange flag", "Orange Flagge"),
    ("Yellow flag", "Gelbe Flagge"),
    ("Green flag", "Grüne Flagge"),
    ("Blue flag", "Blaue Flagge"),
    ("Purple flag", "Violette Flagge"),
    ("Gray flag", "Graue Flagge"),
    ("Red star", "Roter Stern"),
    ("Orange star", "Oranger Stern"),
    ("Yellow star", "Gelber Stern"),
    ("Green star", "Grüner Stern"),
    ("Blue star", "Blauer Stern"),
    ("Purple star", "Violetter Stern"),
    ("Gray star", "Grauer Stern"),
    ("Smile", "Lächeln"),
    ("Laugh", "Lachen"),
    ("Angry", "Wütend"),
    ("Cry", "Weinen"),
    ("Surprise", "Überraschung"),
    ("Boring", "Gelangweilt"),
    ("Plus", "Plus"),
    ("Minus", "Minus"),
    ("Question", "Frage"),
    ("Exclamation", "Ausrufezeichen"),
    ("Info", "Info"),
    ("Wrong", "Falsch"),
    ("Correct", "Richtig"),
    ("Like", "Gefällt mir"),
    ("Dislike", "Gefällt mir nicht"),
    ("Idea", "Idee"),
    ("Attention", "Achtung"),
    ("Money", "Geld"),
    ("January", "Januar"),
    ("February", "Februar"),
    ("March", "März"),
    ("April", "April"),
    ("May", "Mai"),
    ("June", "Juni"),
    ("July", "Juli"),
    ("August", "August"),
    ("September", "September"),
    ("October", "Oktober"),
    ("November", "November"),
    ("December", "Dezember"),
    ("Monday", "Montag"),
    ("Tuesday", "Dienstag"),
    ("Wednesday", "Mittwoch"),
    ("Thursday", "Donnerstag"),
    ("Friday", "Freitag"),
    ("Saturday", "Samstag"),
    ("Sunday", "Sonntag"),
    (
        "Open or select a map to show the Format Panel",
        "Eine Map öffnen oder auswählen, um den Formatbereich anzuzeigen",
    ),
    (
        "Zen Mode: press Escape to exit",
        "Zen-Modus: Escape zum Beenden drücken",
    ),
    ("Expand ideas", "Ideen ausbauen"),
    ("Add subtopic", "Unterthema hinzufügen"),
    ("Add child topic", "Unterthema am Anschluss hinzufügen"),
    ("Add sibling topic", "Nachbarthema am Anschluss hinzufügen"),
    ("Connect topics", "Themen verbinden"),
    ("Edit hyperlink", "Hyperlink bearbeiten"),
    ("Edit notes", "Notizen bearbeiten"),
    ("Toggle to-do", "Aufgabe umschalten"),
    ("Topic style", "Themenstil"),
    ("More insert actions", "Weitere Einfügeaktionen"),
    ("Find and Replace…", "Suchen und Ersetzen…"),
    ("Previous result", "Vorheriges Ergebnis"),
    ("Next result", "Nächstes Ergebnis"),
    ("Close search", "Suche schließen"),
    ("Replace with", "Ersetzen durch"),
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
    ("Install web app", "Web-App installieren"),
    (
        "Install from your browser menu. On iPhone/iPad use Share > Add to Home Screen; on Mac Safari use Share > Add to Dock. On Windows/Linux use Chrome or Edge.",
        "Installation über das Browsermenü. Auf iPhone/iPad: Teilen > Zum Home-Bildschirm; in Safari auf dem Mac: Teilen > Zum Dock hinzufügen. Unter Windows/Linux Chrome oder Edge verwenden.",
    ),
    (
        "Installation keeps browser storage limits. Download backups; clearing site data can delete maps. Offline use needs one complete online load.",
        "Die Speichergrenzen des Browsers gelten weiterhin. Sicherungen herunterladen; das Löschen von Website-Daten kann Maps löschen. Offline-Nutzung erfordert einmaliges vollständiges Online-Laden.",
    ),
    (
        "No install prompt is available. The app may already be installed, or your browser requires its menu. Use HTTPS or localhost.",
        "Kein Installationsdialog verfügbar. Die App ist möglicherweise bereits installiert oder erfordert das Browsermenü. HTTPS oder localhost verwenden.",
    ),
    (
        "Search commands or shortcuts",
        "Befehle oder Tastenkürzel suchen",
    ),
    ("Shortcut category", "Tastenkürzelkategorie"),
    ("Unassigned only", "Nur unbelegte"),
    ("Unassigned", "Unbelegt"),
    ("Commands", "Befehle"),
    ("Navigation", "Navigation"),
    ("No matching commands", "Keine passenden Befehle"),
    (
        "This is a reference list. Shortcut rebinding is not available yet.",
        "Dies ist eine Referenzliste. Tastenkürzel können noch nicht neu belegt werden.",
    ),
    ("About Thinoda", "Über Thinoda"),
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
    ("AI", "KI"),
    ("AI: Brainstorm Subtopics", "KI: Unterthemen brainstormen"),
    ("AI: Break Down into To-Dos", "KI: In Aufgaben zerlegen"),
    ("AI: Explain Topic", "KI: Thema erklären"),
    ("AI: Summarize Branch", "KI: Zweig zusammenfassen"),
    ("AI: Generate Map…", "KI: Map erstellen…"),
    ("AI Assistant", "KI-Assistent"),
    ("Enable AI assistant", "KI-Assistent aktivieren"),
    (
        "Any OpenAI-compatible server: LM Studio, Ollama, llama.cpp, OpenAI…",
        "Jeder OpenAI-kompatible Server: LM Studio, Ollama, llama.cpp, OpenAI…",
    ),
    ("Server URL", "Server-URL"),
    ("Model", "Modell"),
    ("automatic", "automatisch"),
    ("API Key", "API-Schlüssel"),
    ("Save API Key", "API-Schlüssel speichern"),
    ("Clear", "Löschen"),
    (
        "API keys are stored in your operating system's secure credential store.",
        "API-Schlüssel werden im sicheren Anmeldedatenspeicher des Betriebssystems gespeichert.",
    ),
    (
        "Secure credential storage is unavailable. Save a key to retry, or clear the saved key.",
        "Der sichere Anmeldedatenspeicher ist nicht verfügbar. Speichern Sie einen Schlüssel erneut oder löschen Sie den gespeicherten Schlüssel.",
    ),
    (
        "Could not save API key securely",
        "API-Schlüssel konnte nicht sicher gespeichert werden",
    ),
    (
        "Secure credential storage is unavailable",
        "Der sichere Anmeldedatenspeicher ist nicht verfügbar",
    ),
    (
        "Could not save settings securely",
        "Einstellungen konnten nicht sicher gespeichert werden",
    ),
    ("Creativity", "Kreativität"),
    ("Test Connection", "Verbindung testen"),
    ("Connected", "Verbunden"),
    ("Generate Map with AI", "Map mit KI erstellen"),
    (
        "Describe the topic of your mind map:",
        "Beschreiben Sie das Thema Ihrer Mindmap:",
    ),
    (
        "e.g. Launch plan for a mobile app",
        "z. B. Launch-Plan für eine Mobile-App",
    ),
    ("Generate", "Erstellen"),
    ("Server", "Server"),
    (
        "AI server not reachable — check Settings › AI",
        "KI-Server nicht erreichbar — Einstellungen › KI prüfen",
    ),
    (
        "AI assistant is disabled in Settings",
        "Der KI-Assistent ist in den Einstellungen deaktiviert",
    ),
    ("AI is still working…", "Die KI arbeitet noch…"),
    ("Select a topic first", "Bitte zuerst ein Thema auswählen"),
    ("AI map created", "KI-Map erstellt"),
    (
        "AI settings changed. Test the connection again.",
        "Die KI-Einstellungen wurden geändert. Bitte die Verbindung erneut testen.",
    ),
    (
        "The document changed. Try again.",
        "Das Dokument wurde geändert. Bitte erneut versuchen.",
    ),
    (
        "The topic no longer exists",
        "Das Thema existiert nicht mehr",
    ),
    ("Go to Central Topic", "Zum Hauptthema"),
    // Topic insertion
    ("Topic", "Thema"),
    ("Subtopic", "Unterthema"),
    ("Topic Before", "Thema davor"),
    ("Parent Topic", "Übergeordnetes Thema"),
    ("Floating Topic", "Freies Thema"),
    ("Relationship", "Beziehung"),
    ("Branch side", "Zweigseite"),
    ("Advanced Layout", "Erweitertes Layout"),
    ("Flexible Floating Topics", "Flexible freie Themen"),
    (
        "Place floating topics without attaching. Turn off to convert between attached and floating topics.",
        "Freie Themen ohne Verknüpfung platzieren. Zum Umwandeln zwischen angehängten und freien Themen ausschalten.",
    ),
    ("Relationship start", "Beziehungsanfang"),
    ("Relationship end", "Beziehungsende"),
    ("First curve control", "Erster Kurvengriff"),
    ("Second curve control", "Zweiter Kurvengriff"),
    ("Relationship title", "Beziehungstitel"),
    ("From topic", "Von Thema"),
    ("To topic", "Zu Thema"),
    ("First control X", "Erster Kurvengriff X"),
    ("First control Y", "Erster Kurvengriff Y"),
    ("Second control X", "Zweiter Kurvengriff X"),
    ("Second control Y", "Zweiter Kurvengriff Y"),
    ("Automatic line color", "Automatische Linienfarbe"),
    ("Reset curve", "Kurve zurücksetzen"),
    ("Invalid curve coordinate", "Ungültige Kurvenkoordinate"),
    (
        "Expand both endpoints to edit automatic curve controls.",
        "Beide Endpunkte aufklappen, um automatische Kurvengriffe zu bearbeiten.",
    ),
    (
        "Curve coordinates are offsets from each endpoint topic center.",
        "Kurvenkoordinaten sind Abstände zur Mitte des jeweiligen Endpunkt-Themas.",
    ),
    (
        "Drag the round endpoints to reconnect. Drag square controls to reshape.",
        "Runde Endpunkte zum Neuverbinden ziehen. Quadratische Kurvengriffe zum Umformen ziehen.",
    ),
    (
        "Focused handles: arrows move controls by 1 map unit (Shift: 10); endpoint arrows cycle visible topics.",
        "Fokussierte Griffe: Pfeiltasten verschieben Kurvengriffe um 1 Map-Einheit (Umschalt: 10); Endpunkt-Pfeiltasten wechseln sichtbare Themen.",
    ),
    ("Summary", "Zusammenfassung"),
    ("Boundary", "Umrandung"),
    ("Marker", "Marker"),
    ("Markers", "Marker"),
    ("Label", "Label"),
    ("Labels", "Labels"),
    ("Edit labels", "Labels bearbeiten"),
    ("Auto sort", "Automatisch sortieren"),
    (
        "Sort labels by Unicode code-point order (case-sensitive).",
        "Labels nach Unicode-Codepunkten sortieren (Groß-/Kleinschreibung beachten).",
    ),
    ("New labels", "Neue Labels"),
    ("Add labels", "Labels hinzufügen"),
    ("Remove label", "Label entfernen"),
    ("Use label", "Label verwenden"),
    ("Labels on this sheet", "Labels auf diesem Blatt"),
    ("Apply", "Übernehmen"),
    (
        "Label order is kept. Matching is case-sensitive; empty and duplicate labels are removed.",
        "Die Reihenfolge bleibt erhalten. Groß-/Kleinschreibung wird beachtet; leere und doppelte Labels werden entfernt.",
    ),
    (
        "Label editing canceled because the document, selection or labels changed",
        "Label-Bearbeitung abgebrochen, weil Dokument, Auswahl oder Labels geändert wurden",
    ),
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
    ("Font Family", "Schriftart"),
    ("Global Font", "Globale Schriftart"),
    ("Default CJK font", "Standard-CJK-Schriftart"),
    ("Show alignment guides", "Ausrichtungshilfen anzeigen"),
    (
        "Visual guides while moving topics freely; positions do not snap.",
        "Visuelle Hilfen beim freien Verschieben von Themen; Positionen rasten nicht ein.",
    ),
    (
        "Fallback only; choose an installed font covering your script. Document fonts stay unchanged.",
        "Nur Ersatzschrift; wählen Sie eine installierte Schrift für Ihr Schriftsystem. Dokumentschriften bleiben unverändert.",
    ),
    ("Branches", "Zweige"),
    ("Automatic branch width", "Automatische Zweigbreite"),
    ("Branch Line Width", "Zweiglinienbreite"),
    ("Invalid branch width", "Ungültige Zweigbreite"),
    ("Default", "Standard"),
    ("Monospace", "Nichtproportional"),
    ("Font unavailable", "Schriftart nicht verfügbar"),
    (
        "Using the default font; the requested name is preserved.",
        "Die Standardschrift wird verwendet; der angeforderte Name bleibt erhalten.",
    ),
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
    ("Quick Style", "Schnellstil"),
    ("Quick Styles", "Schnellstile"),
    ("Custom Theme", "Eigenes Design"),
    ("Custom Themes", "Eigene Designs"),
    ("Current theme", "Aktuelles Design"),
    ("Custom Theme Editor", "Design-Editor"),
    ("Edit Custom Theme…", "Eigenes Design bearbeiten…"),
    ("Theme name", "Designname"),
    ("Base theme", "Basisdesign"),
    ("Theme Font", "Design-Schriftart"),
    ("Light background", "Heller Hintergrund"),
    ("Dark background", "Dunkler Hintergrund"),
    ("Palette color", "Palettenfarbe"),
    ("Add color", "Farbe hinzufügen"),
    ("Remove last color", "Letzte Farbe entfernen"),
    ("Topic role", "Themenrolle"),
    ("Role preset", "Rollenvorlage"),
    ("Reset role", "Rolle zurücksetzen"),
    ("Apply Theme", "Design übernehmen"),
    ("Save to Library", "In Bibliothek speichern"),
    ("Theme saved", "Design gespeichert"),
    (
        "Theme saved for this session",
        "Design für diese Sitzung gespeichert",
    ),
    (
        "Could not save theme",
        "Design konnte nicht gespeichert werden",
    ),
    ("Remove Custom Theme?", "Eigenes Design entfernen?"),
    ("Edit…", "Bearbeiten…"),
    (
        "Theme editing canceled because the document changed.",
        "Die Designbearbeitung wurde abgebrochen, weil das Dokument geändert wurde.",
    ),
    (
        "Topic overrides and explicit map settings take precedence. Choose a built-in theme to remove custom defaults.",
        "Themenanpassungen und explizite Map-Einstellungen haben Vorrang. Ein integriertes Design entfernt eigene Standardwerte.",
    ),
    ("Override Fill", "Füllung überschreiben"),
    ("Override Text Color", "Textfarbe überschreiben"),
    ("Override Border Color", "Rahmenfarbe überschreiben"),
    ("Override Line Color", "Linienfarbe überschreiben"),
    ("Override Font Size", "Schriftgröße überschreiben"),
    ("Override Border Width", "Rahmenbreite überschreiben"),
    ("Override Line Width", "Linienbreite überschreiben"),
    ("Override Topic Width", "Themenbreite überschreiben"),
    ("Line Shape", "Linienform"),
    ("Alignment", "Ausrichtung"),
    ("On", "Ein"),
    ("Off", "Aus"),
    ("Save Quick Style", "Schnellstil speichern"),
    ("Save Quick Style…", "Schnellstil speichern…"),
    ("Remove Quick Style…", "Schnellstil entfernen…"),
    ("Remove Quick Style?", "Schnellstil entfernen?"),
    ("Style name", "Stilname"),
    ("Remove", "Entfernen"),
    ("Remove…", "Entfernen…"),
    ("Quick style saved", "Schnellstil gespeichert"),
    (
        "Quick style saved for this session",
        "Schnellstil für diese Sitzung gespeichert",
    ),
    (
        "Could not save quick style",
        "Schnellstil konnte nicht gespeichert werden",
    ),
    (
        "Select one topic to save a quick style.",
        "Wähle ein Thema, um einen Schnellstil zu speichern.",
    ),
    (
        "Replaces topic appearance only. Reset Style restores theme inheritance.",
        "Ersetzt nur das Aussehen. Stil zurücksetzen stellt die Designvererbung wieder her.",
    ),
    (
        "Saves fixed colors and typography, without topic content.",
        "Speichert feste Farben und Schrift, ohne Themeninhalt.",
    ),
    ("Emphasis", "Hervorhebung"),
    ("Note", "Notiz"),
    ("Warning", "Warnung"),
    ("Muted", "Dezent"),
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
    ("Scroll sheets left", "Blattleiste nach links scrollen"),
    ("Scroll sheets right", "Blattleiste nach rechts scrollen"),
    (
        "Scroll documents left",
        "Dokumentleiste nach links scrollen",
    ),
    (
        "Scroll documents right",
        "Dokumentleiste nach rechts scrollen",
    ),
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
        "Thinoda was not closed properly. The following documents can be restored:",
        "Thinoda wurde nicht ordnungsgemäß beendet. Folgende Dokumente können wiederhergestellt werden:",
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
        "Pick a target topic, or click blank space to create a connected floating topic",
        "Zielthema wählen oder auf eine freie Stelle klicken, um ein verbundenes freies Thema zu erstellen",
    ),
    // About
    ("Version", "Version"),
    ("Give your thoughts shape.", "Gib deinen Gedanken Form."),
    ("Thinoda logo", "Thinoda-Logo"),
    (
        "A fast, open mind mapping app.",
        "Eine schnelle, offene Mindmapping-App.",
    ),
    ("Close Tab", "Tab schließen"),
    ("New Tab", "Neuer Tab"),
    ("Next Document", "Nächstes Dokument"),
    ("Show in Folder", "Im Ordner anzeigen"),
    ("Starting…", "Wird gestartet…"),
    ("Opening file:", "Datei wird geöffnet:"),
    (
        "The file loader stopped unexpectedly",
        "Das Laden der Datei wurde unerwartet abgebrochen",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSLATED: [Lang; 9] = [
        Lang::De,
        Lang::Es,
        Lang::Fr,
        Lang::Da,
        Lang::Nl,
        Lang::Pt,
        Lang::It,
        Lang::Pl,
        Lang::PtBr,
    ];

    #[test]
    fn locale_subtags_and_explicit_preferences() {
        for (locale, expected) in [
            ("en-US", Lang::En),
            ("DE_at.UTF-8", Lang::De),
            ("es-MX", Lang::Es),
            ("fr_CA.UTF-8", Lang::Fr),
            ("da-DK", Lang::Da),
            ("nl-BE", Lang::Nl),
            ("pt-PT", Lang::Pt),
            ("pt_BR.UTF-8", Lang::PtBr),
            ("pt-br", Lang::PtBr),
            ("pt-Latn-BR", Lang::PtBr),
            ("pt_Latn_BR.UTF-8", Lang::PtBr),
            ("pt-x-br", Lang::Pt),
            ("pt@br", Lang::Pt),
            ("pt-AO", Lang::Pt),
            ("it_IT@euro", Lang::It),
            ("pl-PL", Lang::Pl),
            ("", Lang::En),
            ("C.UTF-8", Lang::En),
            ("den", Lang::En),
            ("ja-JP", Lang::En),
            ("zh-CN", Lang::En),
        ] {
            assert_eq!(resolve_locale(locale), expected, "{locale}");
        }
        let mut names = std::collections::HashSet::new();
        let mut resolved = std::collections::HashSet::new();
        for language in Language::ALL {
            assert!(names.insert(language.native_name()));
            if language != Language::Auto {
                assert!(resolved.insert(resolve(language) as u8));
            }
        }
        assert_eq!(resolved.len(), 10);
    }

    #[test]
    fn every_catalog_covers_all_english_keys_and_dynamic_labels() {
        let mut keys: Vec<_> = DE.iter().map(|&(key, _)| key).collect();
        keys.extend(additional().keys().copied());
        keys.extend(
            thinoda_core::TopicShape::ALL
                .iter()
                .map(|shape| shape.label()),
        );
        keys.extend(
            thinoda_core::LineShape::ALL
                .iter()
                .map(|shape| shape.label()),
        );
        keys.extend(
            thinoda_core::theme::Theme::IDS
                .iter()
                .map(|id| thinoda_core::theme::Theme::builtin(id).name),
        );
        for group in thinoda_core::markers::GROUPS {
            keys.push(group.name);
            keys.extend(group.markers.iter().map(|marker| marker.name));
        }
        for template in thinoda_core::templates::TEMPLATES {
            keys.push(template.name);
            keys.push(template.category.label());
        }
        keys.extend(
            thinoda_io::Format::IMPORTABLE
                .iter()
                .map(|format| format.label()),
        );
        keys.extend(
            thinoda_io::Format::EXPORTABLE
                .iter()
                .map(|format| format.label()),
        );
        for language in TRANSLATED {
            for &key in &keys {
                let present = match language {
                    Lang::De => german().contains_key(key),
                    Lang::PtBr => brazilian().contains_key(key),
                    _ => additional().contains_key(key),
                };
                assert!(present, "Missing {language:?} translation: {key}");
                {
                    let text = t_in(language, key);
                    assert!(!text.trim().is_empty());
                    assert!(!text.contains('\u{fffd}'), "Invalid Unicode: {key}");
                    for token in [
                        ".pdf", ".png", ".thinoda", ".xmind", "Ctrl+", "F11", "F7", "OAuth",
                        "Unicode", "OpenAI",
                    ] {
                        if language == Lang::De && token == "Ctrl+" {
                            assert_eq!(text.matches("Strg+").count(), key.matches(token).count());
                            continue;
                        }
                        assert_eq!(
                            text.matches(token).count(),
                            key.matches(token).count(),
                            "{language:?}: {token}: {key}"
                        );
                    }
                }
            }
        }
        assert_eq!(t_in(Lang::Pt, "Save"), "Guardar");
        assert_eq!(t_in(Lang::PtBr, "Save"), "Salvar");
        assert_eq!(t_in(Lang::PtBr, "Language"), "Idioma");
    }

    #[test]
    fn default_fonts_cover_catalog_alphabets() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            ui.fonts_mut(|fonts| {
                let font = egui::FontId::proportional(14.0);
                let mut letters = std::collections::BTreeSet::new();
                for values in additional().values() {
                    letters.extend(
                        values
                            .iter()
                            .flat_map(|text| text.chars())
                            .filter(|c| c.is_alphabetic()),
                    );
                }
                for text in brazilian().values() {
                    letters.extend(text.chars().filter(|c| c.is_alphabetic()));
                }
                for letter in letters {
                    assert!(
                        fonts.has_glyph(&font, letter),
                        "Missing default UI glyph: {letter}"
                    );
                }
            });
        });
        output.textures_delta.clear();
    }

    #[test]
    fn every_literal_translation_call_is_in_the_catalog() {
        for source in [
            include_str!("ui.rs"),
            include_str!("app.rs"),
            include_str!("ai.rs"),
            include_str!("help.rs"),
            include_str!("labels.rs"),
            include_str!("doc.rs"),
            include_str!("map_shot.rs"),
            include_str!("printing.rs"),
            include_str!("quick_styles.rs"),
            include_str!("theme_editor.rs"),
            include_str!("template_gallery.rs"),
            include_str!("structure_picker.rs"),
            include_str!("shortcut_browser.rs"),
            include_str!("symbols.rs"),
        ] {
            for (offset, _) in source.match_indices("t(") {
                if offset > 0
                    && (source.as_bytes()[offset - 1].is_ascii_alphanumeric()
                        || source.as_bytes()[offset - 1] == b'_')
                {
                    continue;
                }
                let call = source[offset + 2..].trim_start();
                if let Some(literal) = call.strip_prefix('"') {
                    let key = literal.split('"').next().unwrap();
                    assert!(german().contains_key(key), "Missing German UI key: {key}");
                    assert!(
                        additional().contains_key(key),
                        "Missing additional UI key: {key}"
                    );
                }
            }
        }
    }

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

    #[test]
    fn every_print_label_and_message_has_a_german_translation() {
        // Includes paragraph text as well as labels; English keys are the EN UI.
        let source = include_str!("printing.rs");
        for (offset, _) in source.match_indices("t(\"") {
            // Exclude identifiers ending in t, such as from_id_salt or expect.
            if offset > 0 && source.as_bytes()[offset - 1].is_ascii_alphabetic()
                || offset > 0 && source.as_bytes()[offset - 1] == b'_'
            {
                continue;
            }
            let key = source[offset + 3..].split('"').next().unwrap();
            assert!(
                german().contains_key(key),
                "missing German print translation: {key}"
            );
        }
        for key in [
            "A4",
            "A3",
            "Letter",
            "Legal",
            "Print PDF saved",
            "Printing cancelled",
            "Print job submitted to the OS. Physical output is not confirmed.",
        ] {
            assert!(
                german().contains_key(key),
                "missing German print translation: {key}"
            );
        }
    }
}
