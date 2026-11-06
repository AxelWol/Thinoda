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

impl Lang {
    pub const ALL: [Lang; 10] = [
        Lang::En,
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

    /// English name of the language, used to ask AI models to reply in it.
    pub fn english_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "German",
            Lang::Es => "Spanish",
            Lang::Fr => "French",
            Lang::Da => "Danish",
            Lang::Nl => "Dutch",
            Lang::Pt => "European Portuguese",
            Lang::It => "Italian",
            Lang::Pl => "Polish",
            Lang::PtBr => "Brazilian Portuguese",
        }
    }

    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) const fn language_tag(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
            Lang::Es => "es",
            Lang::Fr => "fr",
            Lang::Da => "da",
            Lang::Nl => "nl",
            Lang::Pt => "pt-PT",
            Lang::It => "it",
            Lang::Pl => "pl",
            Lang::PtBr => "pt-BR",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Resolves the configured language (Auto = system or browser locale).
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
        Language::Auto => {
            #[cfg(target_arch = "wasm32")]
            let locale = web_sys::window()
                .and_then(|window| window.navigator().language())
                .unwrap_or_default();
            #[cfg(not(target_arch = "wasm32"))]
            let locale = sys_locale::get_locale().unwrap_or_default();
            resolve_locale(&locale)
        }
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
    #[cfg(target_arch = "wasm32")]
    {
        let root = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element());
        match root {
            Some(root) => {
                if let Err(error) = root.set_attribute("lang", lang.language_tag()) {
                    log::warn!("could not set browser document language: {error:?}");
                }
            }
            None => log::warn!("browser document language could not be updated"),
        }
    }
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

/// Whether a dynamic label key has German and all additional-locale entries.
#[cfg(test)]
pub(crate) fn has_translations(en: &str) -> bool {
    german().contains_key(en) && additional().contains_key(en)
}

/// Translated title-row button labels for [`thinoda_ui::Dialog`].
pub fn dialog_labels() -> thinoda_ui::DialogLabels {
    thinoda_ui::DialogLabels {
        hide: t("Minimize").into(),
        show: t("Expand").into(),
        maximize: t("Maximize").into(),
        restore: t("Restore").into(),
        close: t("Close Window").into(),
    }
}

/// Translated labels for [`thinoda_ui::date_field`] calendars.
pub fn calendar_labels() -> thinoda_ui::CalendarLabels {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    thinoda_ui::CalendarLabels {
        months: MONTHS.map(|month| t(month).to_owned()),
        weekdays: WEEKDAYS.map(|day| t(day).to_owned()),
        previous_month: t("Previous month").into(),
        next_month: t("Next month").into(),
        today: t("Today").into(),
        open_calendar: t("Open calendar").into(),
        unavailable: t("Non-working day").into(),
        first_weekday: 0,
    }
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
    // Pending freehand input and recovery.
    (
        "A recovery copy could not be removed. It may reappear after restart.",
        "Eine Wiederherstellungskopie konnte nicht entfernt werden. Sie kann nach einem Neustart erneut angezeigt werden.",
    ),
    (
        "Crash recovery is unavailable. Save or download your work.",
        "Die Wiederherstellung nach einem Absturz ist nicht verfügbar. Speichern Sie Ihre Arbeit oder laden Sie sie herunter.",
    ),
    (
        "Some recovery data could not be checked. Keep existing storage and save your work.",
        "Einige Wiederherstellungsdaten konnten nicht geprüft werden. Löschen Sie keine gespeicherten Daten und speichern Sie Ihre Arbeit.",
    ),
    ("Download raw record", "Rohdatensatz herunterladen"),
    (
        "Raw downloads contain a browser storage record (UTF-16LE code units, no BOM), not a .thinoda document. The original stays in browser storage.",
        "Der Rohdaten-Download enthält einen Browser-Speicherdatensatz (UTF-16LE-Codeeinheiten, ohne BOM), kein .thinoda-Dokument. Das Original bleibt im Browserspeicher.",
    ),
    (
        "Raw download failed. Keep recovery data, check browser storage and downloads, close other tabs, and try again.",
        "Der Rohdaten-Download ist fehlgeschlagen. Bewahren Sie die Wiederherstellungsdaten auf, prüfen Sie den Browserspeicher und Downloads, schließen Sie andere Tabs und versuchen Sie es erneut.",
    ),
    (
        "Download requested. Check browser downloads; the original recovery data was kept.",
        "Download angefordert. Prüfen Sie die Browser-Downloads; die ursprünglichen Wiederherstellungsdaten bleiben erhalten.",
    ),
    ("Pending ink gesture", "Ausstehende Freihandgeste"),
    (
        "Ink is not saved or included in crash recovery until it is finished.",
        "Die Freihandeingabe wird nicht gespeichert und nicht in die Wiederherstellung nach einem Absturz aufgenommen, bis die Geste abgeschlossen ist.",
    ),
    (
        "Finish the pending ink gesture before continuing. Retry it or discard it.",
        "Schließen Sie die ausstehende Freihandgeste ab, bevor Sie fortfahren. Versuchen Sie es erneut oder verwerfen Sie sie.",
    ),
    (
        "Resolve pending ink before continuing. Retry, copy, or discard it.",
        "Die ausstehende Freihandgeste muss bearbeitet werden, bevor Sie fortfahren können. Versuchen Sie es erneut, kopieren Sie die Messwerte oder verwerfen Sie die Geste.",
    ),
    (
        "The pointer is still down. Release it before retrying.",
        "Die Zeigereingabe ist noch aktiv. Lassen Sie die Maustaste, den Stift oder den Finger los, bevor Sie es erneut versuchen.",
    ),
    (
        "Some ink input was rejected. This gesture cannot be retried; copy its samples or discard it.",
        "Ein Teil der Freihandeingabe wurde abgelehnt. Diese Geste kann nicht erneut ausgeführt werden; kopieren Sie ihre Messwerte oder verwerfen Sie sie.",
    ),
    (
        "The ink gesture could not be completed. Retry it or discard it.",
        "Die Freihandgeste konnte nicht abgeschlossen werden. Versuchen Sie es erneut oder verwerfen Sie sie.",
    ),
    (
        "The ink gesture was interrupted. Retry it or discard it.",
        "Die Freihandgeste wurde unterbrochen. Versuchen Sie es erneut oder verwerfen Sie sie.",
    ),
    ("Ink input details", "Details zur Freihandeingabe"),
    ("Ink settings", "Einstellungen der Freihandeingabe"),
    ("Retry ink", "Freihandgeste erneut versuchen"),
    ("Discard ink", "Freihandgeste verwerfen"),
    ("Copy ink samples", "Freihand-Messwerte kopieren"),
    ("Dismiss ink feedback", "Freihandmeldung ausblenden"),
    ("Ink samples copied", "Freihand-Messwerte kopiert"),
    ("Accepted samples", "Akzeptierte Messwerte"),
    ("Rejected input", "Abgelehnte Eingabe"),
    ("Sample position", "Position des Messwerts"),
    ("Pressure", "Druck"),
    ("No pressure", "Kein Druck"),
    ("Simplification tolerance", "Vereinfachungstoleranz"),
    (
        "Additional rejected input details could not be retained.",
        "Weitere Details zur abgelehnten Eingabe konnten nicht aufbewahrt werden.",
    ),
    (
        "The document changed during the gesture. Copy its samples or discard it.",
        "Das Dokument wurde während der Geste geändert. Kopieren Sie die Messwerte oder verwerfen Sie die Geste.",
    ),
    (
        "The canvas changed during the gesture. Copy its samples or discard it.",
        "Die Zeichenfläche wurde während der Geste geändert. Kopieren Sie die Messwerte oder verwerfen Sie die Geste.",
    ),
    (
        "Ink could not be applied to a locked object. Copy the samples or discard the gesture.",
        "Die Freihandeingabe konnte nicht auf ein gesperrtes Objekt angewendet werden. Kopieren Sie die Messwerte oder verwerfen Sie die Geste.",
    ),
    (
        "This operation would change frame clipping. Copy the samples or discard the gesture.",
        "Dieser Vorgang würde die Beschneidung durch den Rahmen ändern. Kopieren Sie die Messwerte oder verwerfen Sie die Geste.",
    ),
    (
        "Adjust ink simplification and retry, or copy the samples and discard the gesture.",
        "Passen Sie die Vereinfachung der Freihandeingabe an und versuchen Sie es erneut, oder kopieren Sie die Messwerte und verwerfen Sie die Geste.",
    ),
    (
        "Finish the active text editor before drawing again.",
        "Beenden Sie die aktive Textbearbeitung, bevor Sie erneut zeichnen.",
    ),
    // Offline board guide.
    (
        "If the selected-topic toolbar cannot fit, use the main menu or the topic's context menu.",
        "Wenn die Werkzeugleiste des ausgewählten Themas keinen Platz findet, verwenden Sie das Hauptmenü oder das Kontextmenü des Themas.",
    ),
    (
        "Pen and Highlighter draw freehand. Adjust Ink color, Ink width and Opacity, or Eraser radius for erasing; settings are captured when each gesture begins.",
        "Mit Stift und Textmarker zeichnen Sie freihändig. Stellen Sie Tintenfarbe, Strichbreite und Deckkraft oder zum Radieren den Radius des Radierers ein; die Einstellungen werden zu Beginn jeder Geste übernommen.",
    ),
    (
        "Object eraser removes whole ink strokes; Pixel eraser removes parts of strokes. Lasso selects topics and objects. Image card opens a local PNG/JPEG picker.",
        "Objektradierer entfernt ganze Freihandstriche; Pixelradierer entfernt Teile davon. Lasso wählt Themen und Objekte aus. Bildkarte öffnet eine lokale Dateiauswahl für PNG/JPEG.",
    ),
    (
        "Use the board toolbar to add sticky notes, text cards, shapes, connectors and frames. Use the select tool to move items and the hand tool to pan. The Board menu includes grouping, locking, grid and minimap options.",
        "Verwenden Sie die Board-Werkzeugleiste, um Haftnotizen, Textkarten, Formen, Verbindungslinien und Rahmen hinzuzufügen. Mit dem Auswahlwerkzeug verschieben Sie Elemente, mit dem Handwerkzeug die Ansicht. Das Menü Board bietet Gruppierung, Sperren, Raster und Minimap.",
    ),
    (
        "Board > Hide central topic moves any central-topic content to a floating topic. Board > Show central topic shows the central topic again; floating topics stay separate. FIM sheets always show a central topic.",
        "Board > Zentrales Thema ausblenden verschiebt vorhandene Inhalte des zentralen Themas in ein freies Thema. Board > Zentrales Thema einblenden zeigt das zentrale Thema wieder an; freie Themen bleiben getrennt. FIM-Blätter zeigen immer ein zentrales Thema.",
    ),
    (
        "Save boards as .thinoda to preserve cards, shapes, connectors, frames and board settings. Other formats may change or omit board content; review the export warnings.",
        "Speichern Sie Boards als .thinoda, um Karten, Formen, Verbindungslinien, Rahmen und Board-Einstellungen zu erhalten. Andere Formate können Board-Inhalte verändern oder auslassen; beachten Sie die Exportwarnungen.",
    ),
    (
        "Inline card text and list formatting omitted",
        "Formatierung und Listen im Kartentext ausgelassen",
    ),
    (
        "Mind-map topics and links flattened to shapes",
        "Mind-Map-Themen und -Verbindungen in Formen umgewandelt",
    ),
    (
        "Topic notes, labels, markers or tasks omitted",
        "Notizen, Beschriftungen, Markierungen oder Aufgaben von Themen ausgelassen",
    ),
    (
        "Presentation slides omitted",
        "Präsentationsfolien ausgelassen",
    ),
    (
        "Formatted card titles cannot be changed until rich-text editing is supported",
        "Formatierte Kartentitel können erst geändert werden, wenn die Rich-Text-Bearbeitung unterstützt wird.",
    ),
    (
        "Frame clipping is visual, not secure redaction. Content outside a frame can remain in saved files or exports, and some exports may not apply clipping.",
        "Das Beschneiden durch Rahmen wirkt nur visuell und ist keine sichere Schwärzung. Inhalte außerhalb eines Rahmens können in gespeicherten Dateien oder Exporten erhalten bleiben; manche Exportformate wenden die Beschneidung nicht an.",
    ),
    // Freeform board tools, properties, and accessible labels.
    ("Board", "Board"),
    ("Above map", "Über der Mindmap"),
    ("Below map", "Unter der Mindmap"),
    ("Select tool", "Auswahlwerkzeug"),
    ("Hand tool", "Handwerkzeug"),
    ("Sticky note", "Haftnotiz"),
    ("Text card", "Textkarte"),
    ("Image card", "Bildkarte"),
    ("Shape tool", "Formenwerkzeug"),
    ("Shape library", "Formenbibliothek"),
    ("Search shapes", "Formen suchen"),
    ("Connector", "Verbindungslinie"),
    ("Connector tool", "Verbindungswerkzeug"),
    ("Frame", "Rahmen"),
    ("Frame tool", "Rahmenwerkzeug"),
    ("Pen", "Stift"),
    ("Highlighter", "Textmarker"),
    ("Object eraser", "Objektradierer"),
    ("Pixel eraser", "Pixelradierer"),
    ("Lasso", "Lasso"),
    ("Stroke type", "Strichart"),
    ("Ink color", "Tintenfarbe"),
    ("Ink width", "Strichbreite"),
    ("Eraser radius", "Radius des Radierers"),
    ("Collapse board toolbar", "Board-Werkzeugleiste einklappen"),
    ("Expand board toolbar", "Board-Werkzeugleiste ausklappen"),
    ("Hide central topic", "Zentrales Thema ausblenden"),
    ("Show central topic", "Zentrales Thema einblenden"),
    ("No grid", "Kein Raster"),
    ("Dot grid", "Punktraster"),
    ("Line grid", "Linienraster"),
    ("Snap to grid", "Am Raster einrasten"),
    ("Zoom to selection", "Auf Auswahl zoomen"),
    ("Board minimap", "Board-Minimap"),
    (
        "Board minimap. Click or drag to pan; arrow keys move the view; Shift makes larger steps.",
        "Board-Minimap. Klicken oder ziehen, um die Ansicht zu verschieben; die Pfeiltasten bewegen die Ansicht; die Umschalttaste vergrößert die Schritte.",
    ),
    (
        "Click or drag to pan. Arrow keys move the view; Shift makes larger steps.",
        "Klicken oder ziehen, um die Ansicht zu verschieben. Die Pfeiltasten bewegen die Ansicht; die Umschalttaste vergrößert die Schritte.",
    ),
    (
        "The current view is outside the minimap.",
        "Die aktuelle Ansicht liegt außerhalb der Minimap.",
    ),
    ("Group board selection", "Board-Auswahl gruppieren"),
    (
        "Ungroup board selection",
        "Gruppierung der Board-Auswahl aufheben",
    ),
    ("Lock board selection", "Board-Auswahl sperren"),
    ("Unlock board selection", "Board-Auswahl entsperren"),
    ("Bring forward", "Eine Ebene nach vorne"),
    ("Send backward", "Eine Ebene nach hinten"),
    ("Bring to front", "In den Vordergrund"),
    ("Send to back", "In den Hintergrund"),
    ("Rotate left", "Nach links drehen"),
    ("Rotate right", "Nach rechts drehen"),
    ("Triangle", "Dreieck"),
    ("Right triangle", "Rechtwinkliges Dreieck"),
    ("Parallelogram", "Parallelogramm"),
    ("Trapezoid", "Trapez"),
    ("Pentagon", "Fünfeck"),
    ("Hexagon", "Sechseck"),
    ("Octagon", "Achteck"),
    ("Arrow", "Pfeil"),
    ("Double arrow", "Doppelpfeil"),
    ("Chevron", "Winkel"),
    ("Callout", "Sprechblase"),
    ("Cylinder", "Zylinder"),
    ("Document", "Dokument"),
    ("Process", "Prozess"),
    ("Predefined process", "Vordefinierter Prozess"),
    ("Terminator", "Start/Ende"),
    ("Manual input", "Manuelle Eingabe"),
    ("Preparation", "Vorbereitung"),
    ("Delay", "Verzögerung"),
    ("Off-page connector", "Seitenübergreifender Verbinder"),
    ("Ink stroke", "Freihandstrich"),
    ("Selected", "Ausgewählt"),
    ("Locked", "Gesperrt"),
    ("From", "Von"),
    ("To", "Nach"),
    ("Free point", "Freier Punkt"),
    ("Resize board item", "Größe des Board-Elements ändern"),
    ("Rotate board item", "Board-Element drehen"),
    (
        "Add connected shape above",
        "Verbundene Form oberhalb hinzufügen",
    ),
    (
        "Add connected shape to the right",
        "Verbundene Form rechts hinzufügen",
    ),
    (
        "Add connected shape below",
        "Verbundene Form unterhalb hinzufügen",
    ),
    (
        "Add connected shape to the left",
        "Verbundene Form links hinzufügen",
    ),
    (
        "Click adds a connected copy; Ctrl+click adds only a copy; Shift+click adds only a connector. Drag to connect to another item; Ctrl+drag drops a connected copy. Use Command on macOS.",
        "Klicken fügt eine verbundene Kopie hinzu; Strg+Klick fügt nur eine Kopie hinzu; Umschalt+Klick fügt nur einen Verbinder hinzu. Ziehen verbindet mit einem anderen Element; Strg+Ziehen legt eine verbundene Kopie ab. Unter macOS Befehlstaste verwenden.",
    ),
    ("Board object label", "Beschriftung des Board-Objekts"),
    (
        "Select one board object to edit its properties.",
        "Wählen Sie ein Board-Objekt aus, um seine Eigenschaften zu bearbeiten.",
    ),
    ("Description", "Beschreibung"),
    ("Opacity", "Deckkraft"),
    ("Stroke", "Strich"),
    ("Connector route", "Verlauf der Verbindungslinie"),
    ("Connector color", "Farbe der Verbindungslinie"),
    ("Connector width", "Breite der Verbindungslinie"),
    ("Arrow at start", "Pfeil am Anfang"),
    ("Arrow at end", "Pfeil am Ende"),
    ("Card", "Karte"),
    ("Rotation", "Drehung"),
    ("Custom color", "Benutzerdefinierte Farbe"),
    (
        "Replaces topic titles, board labels and descriptions.",
        "Ersetzt Thementitel, Board-Beschriftungen und Beschreibungen.",
    ),
    (
        "AI > AI settings opens provider settings. Ollama uses its own chat and embedding models. CLI providers use their own login and separate API embeddings; browsers cannot run CLIs. Unconfigured AI actions and Copilot generation are disabled.",
        "KI > KI-Einstellungen öffnet die Anbietereinstellungen. Ollama nutzt eigene Chat- und Embedding-Modelle. CLI-Anbieter nutzen ihre eigene Anmeldung und separate API-Embeddings; Browser können keine CLIs ausführen. Nicht konfigurierte KI-Aktionen und Copilot-Generierung sind deaktiviert.",
    ),
    ("AI settings", "KI-Einstellungen"),
    (
        "AI provider is not ready. Check AI settings.",
        "Der KI-Anbieter ist nicht bereit. Prüfen Sie die KI-Einstellungen.",
    ),
    (
        "Ollama uses its local OpenAI-compatible API. No API key is sent.",
        "Ollama nutzt seine lokale OpenAI-kompatible API. Es wird kein API-Schlüssel gesendet.",
    ),
    ("Compare", "Vergleichen"),
    (
        "Browser storage migration conflict",
        "Konflikt bei der Browser-Speichermigration",
    ),
    ("Downloading file:", "Datei wird heruntergeladen:"),
    ("Reload file", "Datei neu laden"),
    ("Keep my edits", "Meine Änderungen behalten"),
    (
        "Reload the changed file? Your current content remains available through Undo.",
        "Geänderte Datei neu laden? Der aktuelle Inhalt bleibt über Rückgängig verfügbar.",
    ),
    ("Sync conflict copies", "Synchronisationskonfliktkopien"),
    (
        "These files are kept by your sync client. Thinoda never deletes them.",
        "Diese Dateien werden vom Synchronisationsprogramm aufbewahrt. Thinoda löscht sie niemals.",
    ),
    ("Sheets added", "Hinzugefügte Blätter"),
    ("Sheets removed", "Entfernte Blätter"),
    ("Sheets changed", "Geänderte Blätter"),
    ("Topics added", "Hinzugefügte Themen"),
    ("Topics removed", "Entfernte Themen"),
    ("Topics changed", "Geänderte Themen"),
    (
        "Sheet positions and topic IDs are compared; this summary does not merge content.",
        "Blattpositionen und Themen-IDs werden verglichen; diese Übersicht führt keine Inhalte zusammen.",
    ),
    ("Previous", "Zurück"),
    ("Next", "Weiter"),
    ("Append", "Am Ende einfügen"),
    ("Insert before", "Einfügen vor"),
    (
        "Open an XDF2 or XDF3 document to edit FIM data.",
        "Öffnen Sie ein XDF2- oder XDF3-Dokument, um FIM-Daten zu bearbeiten.",
    ),
    ("FIM validation", "FIM-Validierung"),
    ("Dependencies", "Abhängigkeiten"),
    ("Make independent", "Unabhängig machen"),
    ("New identity", "Neue Kennung"),
    ("Ambiguous reference", "Mehrdeutige Referenz"),
    ("Unresolved reference", "Nicht aufgelöste Referenz"),
    ("External reference", "Externe Referenz"),
    (
        "Edit only this occurrence",
        "Nur dieses Vorkommen bearbeiten",
    ),
    ("FIM XDatenfelder 2", "FIM XDatenfelder 2"),
    ("FIM XDatenfelder 3", "FIM XDatenfelder 3"),
    ("Validate document", "Dokument validieren"),
    ("Export validated XDF", "Validiertes XDF exportieren"),
    (
        "Install official schemas",
        "Offizielle Schemas installieren",
    ),
    (
        "Download official schemas?",
        "Offizielle Schemas herunterladen?",
    ),
    (
        "Only official schema archives are downloaded from XRepository. Your document stays on this device. Validation then works offline.",
        "Nur offizielle Schema-Archive werden von XRepository heruntergeladen. Ihr Dokument bleibt auf diesem Gerät. Die Validierung funktioniert danach offline.",
    ),
    ("Download and install", "Herunterladen und installieren"),
    ("Install from ZIP files", "Aus ZIP-Dateien installieren"),
    ("Not checked", "Nicht geprüft"),
    ("Validation passed", "Validierung bestanden"),
    ("Validation failed", "Validierung fehlgeschlagen"),
    ("Validation unavailable", "Validierung nicht verfügbar"),
    ("Validation is stale", "Validierung ist veraltet"),
    ("Semantic diagnostics", "Semantische Diagnose"),
    ("Source fields", "Quelldaten"),
    ("Shared definition", "Gemeinsam verwendete Definition"),
    ("Cardinality", "Kardinalität"),
    ("Field", "Datenfeld"),
    ("Rule", "Regel"),
    ("Code list", "Codeliste"),
    ("Add FIM element", "FIM-Element hinzufügen"),
    ("Document metadata", "Dokumentmetadaten"),
    ("All source data", "Alle Quelldaten"),
    ("Attribute", "Attribut"),
    ("Value", "Wert"),
    (
        "Validation does not certify the document.",
        "Die Validierung zertifiziert das Dokument nicht.",
    ),
    (
        "Save drafts as .thinoda. XDF export requires valid schemas and semantics.",
        "Entwürfe als .thinoda speichern. Der XDF-Export erfordert gültige Schemas und Semantik.",
    ),
    (
        "Apply before changing selection; Cancel discards this draft.",
        "Vor einem Auswahlwechsel übernehmen; Abbrechen verwirft diesen Entwurf.",
    ),
    ("FIM data", "FIM-Daten"),
    (
        "Enter a unique thread name (1–80 bytes, no control characters).",
        "Einen eindeutigen Gesprächsnamen eingeben (1–80 Bytes, keine Steuerzeichen).",
    ),
    ("Local chat history", "Lokaler Chatverlauf"),
    (
        "Local chat history is enabled.",
        "Der lokale Chatverlauf ist aktiviert.",
    ),
    (
        "History operation failed; transcripts kept in memory.",
        "Verlaufsaktion fehlgeschlagen; Gespräche bleiben im Arbeitsspeicher.",
    ),
    (
        "Prompts and replies may contain sensitive data. Local history is not encrypted. Never included in maps or exports.",
        "Fragen und Antworten können sensible Daten enthalten. Der lokale Verlauf ist nicht verschlüsselt und wird nie in Maps oder Exporte aufgenommen.",
    ),
    (
        "Save the document first. Save As, moved files and recovery do not inherit history.",
        "Zuerst das Dokument speichern. Speichern unter, verschobene Dateien und Wiederherstellungen übernehmen keinen Verlauf.",
    ),
    (
        "Close and reopen this document before enabling history again. Session transcripts remain available here.",
        "Vor erneutem Aktivieren das Dokument schließen und wieder öffnen. Sitzungsgespräche bleiben hier verfügbar.",
    ),
    ("Enable local history", "Lokalen Verlauf aktivieren"),
    (
        "Session-only conversation",
        "Gespräch nur für diese Sitzung",
    ),
    ("Conversation", "Gespräch"),
    ("New thread", "Neues Gespräch"),
    ("Thread name", "Gesprächsname"),
    ("Delete thread", "Gespräch löschen"),
    (
        "Disable and forget this file",
        "Für diese Datei deaktivieren und vergessen",
    ),
    ("days", "Tage"),
    (
        "Retention is applied only on confirmation; it deletes older threads across all files.",
        "Die Aufbewahrungsfrist wird erst nach Bestätigung angewandt; ältere Gespräche aller Dateien werden gelöscht.",
    ),
    (
        "Delete all local chat history",
        "Alle lokalen Chatverläufe löschen",
    ),
    (
        "Confirm local history change?",
        "Änderung des lokalen Verlaufs bestätigen?",
    ),
    ("Confirm", "Bestätigen"),
    (
        "Existing session messages are not copied. Create a named thread to save future completed exchanges.",
        "Bisherige Sitzungsnachrichten werden nicht kopiert. Ein benanntes Gespräch speichert künftige abgeschlossene Dialoge.",
    ),
    ("Slides", "Folien"),
    ("Duplicate Slide", "Folie duplizieren"),
    ("Slide source", "Folienquelle"),
    ("Slide content", "Folieninhalt"),
    ("Source subtree", "Quellzweig"),
    ("Map context", "Kartenkontext"),
    ("Auto-fit content", "Inhalt automatisch einpassen"),
    ("Limit topic depth", "Thementiefe begrenzen"),
    ("Topic depth", "Thementiefe"),
    ("Hidden topics", "Ausgeblendete Themen"),
    ("Missing topic", "Fehlendes Thema"),
    ("Restore topic", "Thema wieder einblenden"),
    ("Restore all topics", "Alle Themen wieder einblenden"),
    ("Topic visibility", "Themensichtbarkeit"),
    (
        "Apply and export PowerPoint",
        "Anwenden und als PowerPoint exportieren",
    ),
    (
        "The presentation sheet is unavailable.",
        "Das Präsentationsblatt ist nicht verfügbar.",
    ),
    (
        "Presentation refreshed from the current map.",
        "Präsentation anhand der aktuellen Karte aktualisiert.",
    ),
    (
        "Slide source is unavailable.",
        "Die Folienquelle ist nicht verfügbar.",
    ),
    (
        "Click a topic to hide its subtree.",
        "Klicken Sie auf ein Thema, um seinen Zweig auszublenden.",
    ),
    ("Equation preview in notes", "Gleichungsvorschau in Notizen"),
    ("Cancel equation edit", "Gleichungsbearbeitung abbrechen"),
    (
        "Notes equations render in visual exports for visible topics. XMind preserves source; CSV/OPML require note-format-loss consent. Map Shot and unsupported formats remain blocked.",
        "Notizgleichungen werden in visuellen Exporten für sichtbare Themen dargestellt. XMind erhält den Quelltext; CSV/OPML erfordern die Zustimmung zum Verlust der Notizformatierung. Map Shot und nicht unterstützte Formate bleiben gesperrt.",
    ),
    (
        "512 bytes maximum. Use groups, \\frac{a}{b}, ^{...} and _{...}. Limited syntax, not general LaTeX or chemistry.",
        "Maximal 512 Bytes. Gruppen, \\frac{a}{b}, ^{...} und _{...} verwenden. Begrenzte Syntax, kein allgemeines LaTeX und keine Chemie.",
    ),
    (
        "Format this nonempty plain note as Markdown explicitly before adding an equation.",
        "Diese nicht leere Klartextnotiz vor dem Einfügen einer Gleichung ausdrücklich als Markdown formatieren.",
    ),
    ("Edit notes equation", "Notizgleichung bearbeiten"),
    ("Equation source", "Gleichungsquelltext"),
    ("Apply notes equation", "Notizgleichung übernehmen"),
    ("Remove notes equation", "Notizgleichung entfernen"),
    (
        "This format cannot preserve or render notes equations. Save as Thinoda or use a supported vector or source-preserving format.",
        "Dieses Format kann Notizgleichungen weder erhalten noch darstellen. Als Thinoda speichern oder ein unterstütztes Vektor- bzw. quelltexterhaltendes Format verwenden.",
    ),
    (
        "Invalid equation. Use letters, digits, operators, groups, \\frac{a}{b}, ^{...} and _{...}.",
        "Ungültige Gleichung. Buchstaben, Ziffern, Operatoren, Gruppen, \\frac{a}{b}, ^{...} und _{...} verwenden.",
    ),
    (
        "Equation limit reached (512 bytes, 128 nodes, 8 levels).",
        "Gleichungsgrenze erreicht (512 Bytes, 128 Knoten, 8 Ebenen).",
    ),
    (
        "Keep one equation block at the end of the note, or edit the note source directly.",
        "Einen Gleichungsblock am Ende der Notiz behalten oder den Notizquelltext direkt bearbeiten.",
    ),
    ("New version available", "Neue Version verfügbar"),
    ("Update ready", "Update bereit"),
    (
        "Update installed successfully",
        "Update erfolgreich installiert",
    ),
    ("Current version", "Aktuelle Version"),
    ("Version and changelog", "Version und Änderungsprotokoll"),
    ("Download new version", "Neue Version herunterladen"),
    ("Dismiss", "Ausblenden"),
    (
        "Save your work, then close all app windows and reopen to apply the update.",
        "Speichern Sie Ihre Arbeit, schließen Sie alle App-Fenster und öffnen Sie die App erneut, um das Update anzuwenden.",
    ),
    ("PowerPoint slide size", "PowerPoint-Folienformat"),
    (
        "Slides use editable text, vector shapes and paths. Photos remain images. The frame expands to the selected ratio without cropping or stretching.",
        "Folien enthalten bearbeitbaren Text, Vektorformen und Pfade. Fotos bleiben Bilder. Der Rahmen wird ohne Beschnitt oder Verzerrung auf das gewählte Seitenverhältnis erweitert.",
    ),
    ("Widescreen (16:9)", "Breitbild (16:9)"),
    ("Standard (4:3)", "Standard (4:3)"),
    ("Attachment", "Anhang"),
    ("Save/Download attachment", "Anhang speichern/herunterladen"),
    ("bytes", "Bytes"),
    (
        "Opaque file (not inspected)",
        "Dateiinhalt nicht untersucht",
    ),
    (
        "Could not insert attachment",
        "Anhang konnte nicht eingefügt werden",
    ),
    (
        "Could not save attachment",
        "Anhang konnte nicht gespeichert werden",
    ),
    (
        "Attachment request expired. Select a topic and try again.",
        "Anhangsanfrage abgelaufen. Thema auswählen und erneut versuchen.",
    ),
    (
        "Files stay embedded. Save only files you trust; nothing is opened automatically.",
        "Dateien bleiben eingebettet. Nur vertrauenswürdige Dateien speichern; nichts wird automatisch geöffnet.",
    ),
    (
        "Select a topic, then Insert > Attachment to add a local file as a child topic (1 byte to 8 MiB). Style > Attachment offers a separate Save/Download action. Files are opaque: nothing is opened or executed. Save as .thinoda to retain attachments; other exports are not supported yet.",
        "Ein Thema auswählen und über Einfügen > Anhang eine lokale Datei als Unterthema hinzufügen (1 Byte bis 8 MiB). Stil > Anhang bietet eine separate Aktion zum Speichern/Herunterladen. Dateiinhalte werden nicht interpretiert: Nichts wird geöffnet oder ausgeführt. Als .thinoda speichern, um Anhänge zu behalten; andere Exporte werden noch nicht unterstützt.",
    ),
    ("Local Image", "Lokales Bild"),
    ("Image preview", "Bildvorschau"),
    ("Resize image", "Bildgröße ändern"),
    (
        "Resize a selected image with its lower-right handle. Escape cancels; arrow keys resize a focused handle. Style > Local Image provides keyboard/touch width controls.",
        "Ändern Sie die Größe eines ausgewählten Bildes am Griff unten rechts. Escape bricht ab; Pfeiltasten ändern die Größe bei fokussiertem Griff. Stil > Lokales Bild bietet eine Breitensteuerung für Tastatur und Touch.",
    ),
    ("Insert image", "Bild einfügen"),
    ("Replace image", "Bild ersetzen"),
    ("Remove image", "Bild entfernen"),
    ("Image width", "Bildbreite"),
    ("Image below topic", "Bild unter dem Thema"),
    ("Reset Size", "Größe zurücksetzen"),
    (
        "PNG/JPEG only. Resizing keeps the aspect ratio.",
        "Nur PNG/JPEG. Größenänderungen behalten das Seitenverhältnis bei.",
    ),
    (
        "Could not insert image",
        "Bild konnte nicht eingefügt werden",
    ),
    (
        "Image request expired. Select a topic and try again.",
        "Bildanfrage abgelaufen. Thema auswählen und erneut versuchen.",
    ),
    ("Generation provider", "Anbieter für Textgenerierung"),
    ("CLI default", "CLI-Standard"),
    (
        "CLI replies are buffered. Tools are disabled; no local files are supplied.",
        "CLI-Antworten werden gepuffert. Werkzeuge sind deaktiviert; lokale Dateien werden nicht übergeben.",
    ),
    ("Check CLI status", "CLI-Status prüfen"),
    ("Checking CLI…", "CLI wird geprüft…"),
    (
        "Signed in according to the CLI. Access and subscription are not verified.",
        "Laut CLI angemeldet. Zugriff und Abonnement wurden nicht geprüft.",
    ),
    (
        "Not signed in. Run the login command in your terminal, then check again.",
        "Nicht angemeldet. Den Anmeldebefehl im Terminal ausführen, dann erneut prüfen.",
    ),
    (
        "Authentication status is unknown. This CLI may not support a non-inference check.",
        "Anmeldestatus unbekannt. Diese CLI unterstützt möglicherweise keine Prüfung ohne Modellanfrage.",
    ),
    (
        "Thinoda never starts login or installation automatically. CLI credentials remain managed by the CLI.",
        "Thinoda startet Anmeldung oder Installation nie automatisch. Die CLI verwaltet ihre Zugangsdaten selbst.",
    ),
    ("Copy login command", "Anmeldebefehl kopieren"),
    (
        "Install and login help",
        "Hilfe zu Installation und Anmeldung",
    ),
    ("OpenAI-compatible API", "OpenAI-kompatible API"),
    (
        "Embeddings always use this API, never a CLI. Switching providers preserves these settings and credentials.",
        "Embeddings verwenden immer diese API, nie eine CLI. Beim Anbieterwechsel bleiben diese Einstellungen und Zugangsdaten erhalten.",
    ),
    (
        "Native CLIs are unavailable in the browser. The selection is preserved.",
        "Native CLIs sind im Browser nicht verfügbar. Die Auswahl bleibt erhalten.",
    ),
    (
        "CLI not found on PATH. Install it, restart Thinoda, then check again.",
        "CLI nicht im PATH gefunden. Installieren, Thinoda neu starten und erneut prüfen.",
    ),
    (
        "This CLI installation cannot be launched safely. Use a supported native or npm installation.",
        "Diese CLI-Installation kann nicht sicher gestartet werden. Eine unterstützte native oder npm-Installation verwenden.",
    ),
    (
        "Copilot CLI has no documented isolated mode that disables all user MCP servers and hooks. Generation is unavailable.",
        "Copilot CLI hat keinen dokumentierten isolierten Modus, der alle Benutzer-MCP-Server und Hooks deaktiviert. Textgenerierung ist nicht verfügbar.",
    ),
    (
        "CLI check failed or is unsupported. Authentication could not be verified.",
        "CLI-Prüfung fehlgeschlagen oder nicht unterstützt. Die Anmeldung konnte nicht geprüft werden.",
    ),
    (
        "CLI request timed out.",
        "Zeitüberschreitung bei der CLI-Anfrage.",
    ),
    (
        "CLI output exceeded the safety limit.",
        "CLI-Ausgabe hat die Sicherheitsgrenze überschritten.",
    ),
    (
        "CLI request failed. Check authentication and update the CLI using its official documentation.",
        "CLI-Anfrage fehlgeschlagen. Anmeldung prüfen und die CLI gemäß ihrer offiziellen Dokumentation aktualisieren.",
    ),
    (
        "Could not start the isolated CLI process.",
        "Der isolierte CLI-Prozess konnte nicht gestartet werden.",
    ),
    (
        "Settings > AI selects the generation provider. Native Claude and Codex CLIs use their own login; replies are buffered. Copilot generation is blocked until a documented safe mode is available. Embeddings always use the separate OpenAI-compatible API. Browsers preserve CLI selections but cannot run them.",
        "Unter Einstellungen > KI wird der Anbieter für Textgenerierung gewählt. Native Claude- und Codex-CLIs nutzen ihre eigene Anmeldung; Antworten werden gepuffert. Copilot-Generierung bleibt bis zu einem dokumentierten sicheren Modus gesperrt. Embeddings nutzen immer die separate OpenAI-kompatible API. Browser behalten CLI-Auswahlen bei, können sie aber nicht ausführen.",
    ),
    ("View All Notes", "Alle Notizen anzeigen"),
    ("Filters", "Filter"),
    ("Incomplete", "Nicht abgeschlossen"),
    ("Completed", "Abgeschlossen"),
    ("Clear search and filters", "Suche und Filter zurücksetzen"),
    (
        "Any selected value per group; all selected groups must match.",
        "Beliebiger gewählter Wert je Gruppe; alle gewählten Gruppen müssen zutreffen.",
    ),
    (
        "First 100 results shown. Use Previous or Next for all results.",
        "Die ersten 100 Ergebnisse werden angezeigt. Mit Zurück oder Weiter sind alle erreichbar.",
    ),
    ("Edit mappings", "Tastenbelegungen bearbeiten"),
    ("Rename", "Umbenennen"),
    ("Active shortcuts", "Aktive Tastenkombinationen"),
    (
        "Shortcut examples below use Thinoda Standard; the active profile is shown above.",
        "Die folgenden Tastaturbeispiele verwenden Thinoda Standard; das aktive Profil steht oben.",
    ),
    ("Keyboard mappings", "Tastenbelegungen"),
    ("Keyboard profile", "Tastaturprofil"),
    ("Profile name", "Profilname"),
    ("Assign", "Zuweisen"),
    ("Unassign", "Zuweisung entfernen"),
    ("Reset All", "Alle zurücksetzen"),
    ("Changed bindings", "Geänderte Belegungen"),
    ("Previously", "Bisher"),
    ("Search mappings", "Belegungen suchen"),
    (
        "Delete this keyboard profile?",
        "Dieses Tastaturprofil löschen?",
    ),
    ("Confirm deletion", "Löschen bestätigen"),
    ("Keep profile", "Profil behalten"),
    (
        "Press a shortcut. Escape cancels capture.",
        "Tastenkombination drücken. Escape bricht die Erfassung ab.",
    ),
    (
        "Built-in profiles are read-only. Duplicate one to customize it.",
        "Integrierte Profile sind schreibgeschützt. Zum Anpassen ein Profil duplizieren.",
    ),
    (
        "XMind-compatible covers supported commands, not every XMind or system shortcut.",
        "XMind-kompatibel umfasst unterstützte Befehle, nicht jede XMind- oder Systemtastenkombination.",
    ),
    (
        "Review changed bindings before Apply. Cancel leaves saved mappings unchanged.",
        "Geänderte Belegungen vor dem Übernehmen prüfen. Abbrechen lässt gespeicherte Belegungen unverändert.",
    ),
    (
        "System clipboard and text-editing shortcuts remain owned by the focused editor.",
        "Systemtastenkombinationen für Zwischenablage und Textbearbeitung bleiben beim fokussierten Editor.",
    ),
    (
        "Choose Edit mappings to create or change a keyboard profile.",
        "Zum Erstellen oder Ändern eines Tastaturprofils Tastenbelegungen bearbeiten wählen.",
    ),
    ("Close Window", "Fenster schließen"),
    ("Move to New Window", "In neues Fenster verschieben"),
    ("Window", "Fenster"),
    ("Services", "Dienste"),
    ("Hide Thinoda", "Thinoda ausblenden"),
    ("Hide Others", "Andere ausblenden"),
    ("Show All", "Alle einblenden"),
    ("Minimize", "Minimieren"),
    ("Maximize", "Maximieren"),
    ("Expand", "Erweitern"),
    ("Mon", "Mo"),
    ("Tue", "Di"),
    ("Wed", "Mi"),
    ("Thu", "Do"),
    ("Fri", "Fr"),
    ("Sat", "Sa"),
    ("Sun", "So"),
    ("Today", "Heute"),
    ("Previous month", "Vorheriger Monat"),
    ("Next month", "Nächster Monat"),
    ("Open calendar", "Kalender öffnen"),
    ("Bring All to Front", "Alle nach vorne bringen"),
    (
        "On Windows and macOS, Ctrl+F1 collapses or expands the ribbon. Alt+1 through Alt+8 selects a category. Escape closes a temporarily opened ribbon.",
        "Unter Windows und macOS klappt Strg+F1 das Menüband ein oder aus. Alt+1 bis Alt+8 wählt eine Kategorie. Escape schließt ein vorübergehend geöffnetes Menüband.",
    ),
    ("Expand Ribbon", "Menüband erweitern"),
    ("Pin Ribbon", "Menüband anheften"),
    ("Compact Ribbon", "Kompaktes Menüband"),
    ("Unpin Ribbon", "Menüband lösen"),
    ("Ribbon", "Menüband"),
    ("Collapse Ribbon", "Menüband reduzieren"),
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
        "This file exceeds the supported size limit.",
        "Diese Datei überschreitet die unterstützte Größenbeschränkung.",
    ),
    (
        "Could not read this MindManager file. It may be invalid, encrypted, too large or unsafe.",
        "Diese MindManager-Datei konnte nicht gelesen werden. Sie ist möglicherweise ungültig, verschlüsselt, zu groß oder unsicher.",
    ),
    (
        "Choose a .thinoda, .xmind, .mmap or .drawio destination.",
        "Ein Ziel mit .thinoda, .xmind, .mmap oder .drawio wählen.",
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
        "This import may omit unsupported content. The source file is preserved.",
        "Bei diesem Import können nicht unterstützte Inhalte ausgelassen werden. Die Quelldatei bleibt erhalten.",
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
    (
        "Converted outline preview",
        "Vorschau der konvertierten Gliederung",
    ),
    (
        "showing the first 8 topics",
        "die ersten 8 Themen werden angezeigt",
    ),
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
    ("draw.io", "draw.io"),
    ("SVG", "SVG"),
    ("PNG", "PNG"),
    ("JPEG", "JPEG"),
    ("PDF", "PDF"),
    ("Word", "Word"),
    ("PowerPoint", "PowerPoint"),
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
    ("Background color", "Hintergrundfarbe"),
    ("Use theme default", "Themenstandard verwenden"),
    ("Use frame color", "Rahmenfarbe verwenden"),
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
    ("Home", "Start"),
    ("Capture", "Aufnahme"),
    ("Clipboard", "Zwischenablage"),
    ("Editing", "Bearbeitung"),
    ("Styles", "Stile"),
    (
        "Quick Access Toolbar",
        "Symbolleiste für den Schnellzugriff",
    ),
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
    ("Boards", "Boards"),
    ("Blank Canvas", "Leere Leinwand"),
    ("Multi-root Map", "Karte mit mehreren Wurzeln"),
    ("Brainstorm Wall", "Brainstorming-Wand"),
    ("Retrospective Board", "Retrospektive-Board"),
    ("Kanban Board", "Kanban-Board"),
    ("Business Model Canvas", "Business Model Canvas"),
    ("Flowchart", "Flussdiagramm"),
    ("SWOT Board", "SWOT-Board"),
    ("Affinity Diagram", "Affinitätsdiagramm"),
    ("Eisenhower Matrix", "Eisenhower-Matrix"),
    ("Meeting Board", "Besprechungsboard"),
    ("Swimlane", "Swimlane-Diagramm"),
    ("Venn Diagram", "Venn-Diagramm"),
    ("Compare two sets", "Zwei Mengen vergleichen"),
    ("Set A", "Menge A"),
    ("Set B", "Menge B"),
    ("Shared", "Gemeinsam"),
    ("Empathy Map", "Empathiekarte"),
    (
        "Empathy map: whose perspective?",
        "Empathiekarte: Wessen Perspektive?",
    ),
    ("Says", "Sagt"),
    ("Thinks", "Denkt"),
    ("Does", "Tut"),
    ("Feels", "Fühlt"),
    ("What do they say?", "Was sagt die Person?"),
    ("What might they think?", "Was könnte die Person denken?"),
    ("What do they do?", "Was tut die Person?"),
    ("How might they feel?", "Wie könnte sich die Person fühlen?"),
    ("Storyboard", "Storyboard"),
    (
        "Select an image card, then choose Local Image",
        "Bildkarte auswählen, dann „Lokales Bild“ wählen",
    ),
    ("Scene 1", "Szene 1"),
    ("Scene 2", "Szene 2"),
    ("Scene 3", "Szene 3"),
    ("Scene 1 image", "Bild für Szene 1"),
    ("Scene 2 image", "Bild für Szene 2"),
    ("Scene 3 image", "Bild für Szene 3"),
    ("Scene 1 action", "Handlung in Szene 1"),
    ("Scene 2 action", "Handlung in Szene 2"),
    ("Scene 3 action", "Handlung in Szene 3"),
    ("Moodboard", "Stimmungsboard"),
    ("Image 1", "Bild 1"),
    ("Image 2", "Bild 2"),
    ("Image 3", "Bild 3"),
    ("Image 4", "Bild 4"),
    ("Image 5", "Bild 5"),
    ("Image 6", "Bild 6"),
    ("User Journey Map", "Nutzerreisekarte"),
    (
        "Journey: person and scenario",
        "Nutzerreise: Person und Szenario",
    ),
    ("Stage 1", "Phase 1"),
    ("Stage 2", "Phase 2"),
    ("Stage 3", "Phase 3"),
    ("User actions", "Handlungen des Nutzers"),
    ("Feelings", "Gefühle"),
    ("What could be improved?", "Was könnte verbessert werden?"),
    (
        "Name roles and map the handoffs",
        "Rollen benennen und Übergaben darstellen",
    ),
    ("Role 1", "Rolle 1"),
    ("Role 2", "Rolle 2"),
    ("Role 3", "Rolle 3"),
    ("Meeting: purpose and date", "Besprechung: Ziel und Datum"),
    ("Agenda", "Tagesordnung"),
    ("Decisions", "Entscheidungen"),
    ("Topic and timebox", "Thema und Zeitrahmen"),
    ("Key points and questions", "Wichtige Punkte und Fragen"),
    ("Decision and reason", "Entscheidung und Begründung"),
    (
        "Group related ideas, then name each theme",
        "Gruppiere verwandte Ideen und benenne jedes Thema",
    ),
    ("An idea or observation", "Eine Idee oder Beobachtung"),
    ("Ideas to sort", "Ideen zum Sortieren"),
    ("Theme 1", "Thema 1"),
    ("Theme 2", "Thema 2"),
    ("Theme 3", "Thema 3"),
    (
        "Do: urgent and important",
        "Erledigen: dringend und wichtig",
    ),
    (
        "Schedule: important, not urgent",
        "Planen: wichtig, nicht dringend",
    ),
    (
        "Delegate: urgent, not important",
        "Delegieren: dringend, nicht wichtig",
    ),
    (
        "Eliminate: neither urgent nor important",
        "Streichen: weder dringend noch wichtig",
    ),
    ("Option A", "Option A"),
    ("Option B", "Option B"),
    ("Benefits", "Vorteile"),
    ("Costs", "Kosten"),
    ("Risks", "Risiken"),
    ("Decision", "Entscheidung"),
    ("Criteria", "Kriterien"),
    ("Next step", "Nächster Schritt"),
    (
        "Brainstorm: describe the challenge here",
        "Brainstorming: Herausforderung hier beschreiben",
    ),
    ("Ideas", "Ideen"),
    ("Questions", "Fragen"),
    ("Parking lot", "Themenspeicher"),
    ("One idea per sticky", "Eine Idee pro Notizzettel"),
    ("Build on other ideas", "Auf anderen Ideen aufbauen"),
    ("Go for quantity first", "Zuerst auf Menge setzen"),
    ("What do we need to learn?", "Was müssen wir herausfinden?"),
    ("Topics for later", "Themen für später"),
    (
        "Retrospective: team and time period",
        "Retrospektive: Team und Zeitraum",
    ),
    ("Went well", "Lief gut"),
    ("To improve", "Zu verbessern"),
    ("Actions", "Maßnahmen"),
    ("What helped us?", "Was hat uns geholfen?"),
    ("Something to celebrate", "Etwas zum Feiern"),
    ("What slowed us down?", "Was hat uns gebremst?"),
    ("What surprised us?", "Was hat uns überrascht?"),
    (
        "Action, owner and due date",
        "Maßnahme, Verantwortung und Termin",
    ),
    ("To do", "Zu erledigen"),
    ("Doing", "In Arbeit"),
    ("Describe the next task", "Nächste Aufgabe beschreiben"),
    (
        "Add an owner and due date",
        "Verantwortung und Termin ergänzen",
    ),
    ("Limit work in progress", "Laufende Arbeit begrenzen"),
    ("Finished task", "Erledigte Aufgabe"),
    ("Key Partners", "Schlüsselpartner"),
    ("Key Activities", "Schlüsselaktivitäten"),
    ("Key Resources", "Schlüsselressourcen"),
    ("Value Propositions", "Wertangebote"),
    ("Customer Relationships", "Kundenbeziehungen"),
    ("Channels", "Kanäle"),
    ("Customer Segments", "Kundensegmente"),
    ("Cost Structure", "Kostenstruktur"),
    ("Revenue Streams", "Einnahmequellen"),
    ("Who helps you deliver?", "Wer hilft bei der Umsetzung?"),
    ("What must you do well?", "Was müssen Sie gut können?"),
    (
        "What do you need to deliver?",
        "Was wird für die Umsetzung benötigt?",
    ),
    ("Which problem do you solve?", "Welches Problem lösen Sie?"),
    (
        "How do you work with customers?",
        "Wie arbeiten Sie mit Kunden?",
    ),
    ("How do you reach customers?", "Wie erreichen Sie Kunden?"),
    (
        "Who are you creating value for?",
        "Für wen schaffen Sie Wert?",
    ),
    (
        "What are the main costs?",
        "Was sind die wichtigsten Kosten?",
    ),
    ("How do customers pay?", "Wie zahlen Kunden?"),
    ("Step", "Schritt"),
    ("Decision?", "Entscheidung?"),
    ("Action", "Aktion"),
    ("End", "Ende"),
    ("Rework", "Nacharbeit"),
    ("Yes", "Ja"),
    ("No", "Nein"),
    ("Strengths", "Stärken"),
    ("Weaknesses", "Schwächen"),
    ("Opportunities", "Chancen"),
    ("Threats", "Bedrohungen"),
    ("Internal advantages", "Interne Vorteile"),
    ("Internal gaps", "Interne Lücken"),
    ("External trends to use", "Externe Trends nutzen"),
    ("External risks", "Externe Risiken"),
    (
        "The Boards category starts canvas maps without a visible central topic: a blank canvas, several independent mind maps, sticky-note walls, retrospectives, Kanban, the Business Model Canvas, flowcharts and SWOT. Board text uses the interface language.",
        "Die Kategorie Boards erstellt Leinwand-Karten ohne sichtbares Hauptthema: leere Leinwand, mehrere unabhängige Mindmaps, Notizzettel-Wände, Retrospektiven, Kanban, Business Model Canvas, Flussdiagramme und SWOT. Board-Texte verwenden die Sprache der Oberfläche.",
    ),
    (
        "Affinity Diagram and Eisenhower Matrix provide visual frames. Moving a note into a frame does not automatically group it or schedule a task.",
        "Affinitätsdiagramm und Eisenhower-Matrix bieten visuelle Rahmen. Eine Notiz im Rahmen wird weder automatisch gruppiert noch als Aufgabe geplant.",
    ),
    (
        "Meeting Board has visual areas for agenda, notes, decisions and actions. Its frames do not group notes automatically, and it has no timer.",
        "Das Besprechungsboard hat visuelle Bereiche für Tagesordnung, Notizen, Entscheidungen und Maßnahmen. Seine Rahmen gruppieren Notizen nicht automatisch; ein Timer ist nicht enthalten.",
    ),
    (
        "Swimlane shows handoffs across role lanes. Lanes are visual guides; moving a shape does not assign a role or automate a workflow.",
        "Das Swimlane-Diagramm zeigt Übergaben zwischen Rollen. Die Bahnen sind visuelle Hilfen; das Verschieben einer Form weist keine Rolle zu und automatisiert keinen Ablauf.",
    ),
    (
        "Venn Diagram has overlapping circles and independent labels. Moving a circle does not move labels or assign set membership.",
        "Das Venn-Diagramm hat überlappende Kreise und unabhängige Beschriftungen. Das Verschieben eines Kreises bewegt keine Beschriftungen und weist keine Mengenzugehörigkeit zu.",
    ),
    (
        "Empathy Map has four visual sections for what someone says, thinks, does and feels. Moving a note does not assign a category.",
        "Die Empathiekarte hat vier visuelle Bereiche für das, was jemand sagt, denkt, tut und fühlt. Das Verschieben einer Notiz weist keine Kategorie zu.",
    ),
    (
        "Storyboard has three visual scenes with image cards. Select a card and choose Local Image to add a picture; frames do not group or time the scenes.",
        "Das Storyboard hat drei visuelle Szenen mit Bildkarten. Eine Karte auswählen und über „Lokales Bild“ ein Bild hinzufügen; Rahmen gruppieren oder zeitlich steuern die Szenen nicht.",
    ),
    (
        "Moodboard has six independent image cards and an ideas note. Select a card and choose Local Image to add a picture; cards do not arrange themselves automatically.",
        "Das Stimmungsboard enthält sechs unabhängige Bildkarten und eine Ideennotiz. Eine Karte auswählen und über „Lokales Bild“ ein Bild hinzufügen; die Karten ordnen sich nicht automatisch an.",
    ),
    (
        "User Journey Map has three visual stages for actions, feelings and opportunities. Frames are visual guides: moving a frame does not move notes, and moving a note does not assign a stage.",
        "Die Nutzerreisekarte hat drei visuelle Phasen für Handlungen, Gefühle und Chancen. Rahmen sind visuelle Hilfen: Das Verschieben eines Rahmens bewegt keine Notizen, das Verschieben einer Notiz weist keine Phase zu.",
    ),
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
        "On boards, Alt-drag or Alt+arrow duplicates movable items (Shift makes arrow steps larger). Ctrl+Alt+Left/Right rotates by 1° (Shift: 15°); use Command+Option on macOS. Locked items and groups cannot be duplicated by these gestures.",
        "Auf Boards duplizieren Alt+Ziehen oder Alt+Pfeiltaste bewegliche Elemente (Umschalt vergrößert die Pfeilschritte). Strg+Alt+Links/Rechts dreht um 1° (Umschalt: 15°); unter macOS Command+Option verwenden. Gesperrte Elemente und Gruppen lassen sich mit diesen Gesten nicht duplizieren.",
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
        "Quick Styles and Shape/Text colors work on cards and shapes. For shapes, presets change fill, border and label colors without changing geometry. Locked or unsupported items are skipped; the custom color picker remains available. Each change is one undo step.",
        "Schnellstile und die Farben unter Form/Text funktionieren für Karten und Formen. Bei Formen ändern Vorlagen Füllung, Rand und Beschriftungsfarbe, ohne die Geometrie zu ändern. Gesperrte oder nicht unterstützte Elemente werden übersprungen; die eigene Farbauswahl bleibt verfügbar. Jede Änderung ist ein Rückgängig-Schritt.",
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
        "Cloud settings connect explicitly configured providers. Open and save are revision-checked; uploads and sharing require user actions. Live provider and sync-client compatibility remain unverified; real-time collaboration is not implemented.",
        "Cloud-Einstellungen verbinden ausdrücklich konfigurierte Anbieter. Öffnen und Speichern werden anhand der Revision geprüft; Uploads und Freigaben erfordern eine Nutzeraktion. Die Kompatibilität mit Live-Anbietern und Synchronisierungsclients ist nicht verifiziert; Zusammenarbeit in Echtzeit ist nicht implementiert.",
    ),
    (
        "Integrated cloud accounts are disabled by default while publisher registrations are pending. Local sync folders, backups and browser document storage remain available.",
        "Integrierte Cloud-Konten sind standardmäßig deaktiviert, solange die Registrierungen des Herausgebers noch ausstehen. Lokale Synchronisierungsordner, Sicherungen und der Dokumentspeicher des Browsers bleiben verfügbar.",
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
    ("Duration", "Dauer"),
    ("Table columns", "Tabellenspalten"),
    (
        "Sort siblings (view only)",
        "Geschwister sortieren (nur Ansicht)",
    ),
    ("Document order", "Dokumentreihenfolge"),
    ("Tree Table help", "Hilfe zur Baumtabelle"),
    ("Invalid task value", "Ungültiger Aufgabenwert"),
    (
        "Displayed dates are computed; editing Start date changes its explicit constraint.",
        "Angezeigte Daten sind berechnet; das Bearbeiten des Startdatums ändert dessen explizite Vorgabe.",
    ),
    (
        "Map image, PDF and print output render the map, not this table. Use Gantt export for scheduled task output.",
        "Kartenbild, PDF und Druck geben die Karte aus, nicht diese Tabelle. Für geplante Aufgaben den Gantt-Export verwenden.",
    ),
    (
        "Tree Table edits real topics and tasks. Tab moves between cells; arrows navigate; Enter edits or applies; Escape cancels. Blank cells do not schedule untouched topics. Sorting and columns are local views, not hierarchy edits.",
        "Die Baumtabelle bearbeitet echte Themen und Aufgaben. Tab wechselt Zellen; Pfeile navigieren; Eingabe bearbeitet oder übernimmt; Escape verwirft. Leere Zellen planen unberührte Themen nicht ein. Sortierung und Spalten sind lokale Ansichten, keine Hierarchieänderungen.",
    ),
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
    ("Thinoda on GitHub", "Thinoda auf GitHub"),
    (
        "Click empty space to add · Esc to finish",
        "Zum Hinzufügen auf eine freie Stelle klicken · Esc zum Beenden",
    ),
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
    ("Bold notes", "Notiztext fett"),
    ("Strikethrough notes", "Notiztext durchstreichen"),
    (
        "Cannot apply strikethrough here. Select text within one paragraph outside code, or shorten the note.",
        "Hier kann nicht durchgestrichen werden. Wählen Sie Text innerhalb eines Absatzes außerhalb von Code oder kürzen Sie die Notiz.",
    ),
    (
        "Cannot apply formatting to this selection. The formatting action was canceled.",
        "Diese Auswahl kann nicht formatiert werden. Die Formatierungsaktion wurde abgebrochen.",
    ),
    ("Board export changes", "Änderungen beim Board-Export"),
    (
        "The selected format changes or omits board content. The board in memory stays unchanged; save as Thinoda to preserve it.",
        "Das gewählte Format verändert Board-Inhalte oder lässt sie weg. Das Board im Arbeitsspeicher bleibt unverändert; speichern Sie es als Thinoda, um es zu erhalten.",
    ),
    ("Shapes omitted", "Ausgelassene Formen"),
    (
        "Camera image metadata or color profiles may be omitted",
        "Metadaten oder Farbprofile von Kamerabildern können entfallen",
    ),
    (
        "Card presentation omitted",
        "Ausgelassene Kartendarstellung",
    ),
    (
        "Sheet {sheet}: hidden root mode omitted.",
        "Blatt {sheet}: Der Modus mit verborgenem Wurzelthema entfällt.",
    ),
    (
        "Autosave skipped because this format cannot preserve board content. Save as Thinoda to keep it.",
        "Automatisches Speichern übersprungen, weil dieses Format Board-Inhalte nicht erhalten kann. Speichern Sie als Thinoda, um sie zu bewahren.",
    ),
    (
        "Use --allow-lossy only if you accept these changes.",
        "Verwenden Sie --allow-lossy nur, wenn Sie diese Änderungen akzeptieren.",
    ),
    ("Ink strokes omitted", "Ausgelassene Freihandstriche"),
    ("Connectors omitted", "Ausgelassene Verbindungslinien"),
    (
        "Cards converted to ordinary topics",
        "In normale Themen umgewandelte Karten",
    ),
    ("Card rotations omitted", "Ausgelassene Kartendrehungen"),
    (
        "Viewport slides omitted",
        "Ausgelassene Folien mit Ansichtsausschnitt",
    ),
    ("Zones omitted", "Ausgelassene Zonen"),
    (
        "Zone clipping not fully applied",
        "Zonenbeschneidung nicht vollständig angewendet",
    ),
    (
        "Viewport slides flattened to non-editable images",
        "Ansichtsausschnitt-Folien in nicht bearbeitbare Bilder umgewandelt",
    ),
    (
        "XMind stores a Thinoda-only copy of the complete board. Other apps may not display it and may discard it when saving. This export is not redacted.",
        "XMind speichert eine nur für Thinoda lesbare Kopie des vollständigen Boards. Andere Apps zeigen sie möglicherweise nicht an oder entfernen sie beim Speichern. Dieser Export ist nicht bereinigt.",
    ),
    (
        "Floating root trees omitted",
        "Ausgelassene freie Themenbäume",
    ),
    ("Cards omitted", "Ausgelassene Karten"),
    (
        "Summary topic trees omitted",
        "Ausgelassene Themenbäume von Zusammenfassungen",
    ),
    ("Relationships omitted", "Ausgelassene Beziehungen"),
    ("Boundaries omitted", "Ausgelassene Umrandungen"),
    (
        "Board grid and snapping settings omitted",
        "Ausgelassene Einstellungen für Board-Raster und Einrasten",
    ),
    (
        "Sheet {sheet}: a root named “{title}” will be created.",
        "Blatt {sheet}: Ein Wurzelthema namens „{title}“ wird erstellt.",
    ),
    ("Export note source", "Notizquelle exportieren"),
    (
        "This format cannot preserve note formatting. Use Export with confirmation, or save as Thinoda.",
        "Dieses Format kann die Notizformatierung nicht erhalten. Verwenden Sie Exportieren mit Bestätigung oder speichern Sie als Thinoda.",
    ),
    (
        "This format exports formatted notes as Markdown source, not rich text. The complete source is retained, but formatting will not be displayed. The source workbook is unchanged. Save as Thinoda to preserve note formats.",
        "Dieses Format exportiert formatierte Notizen als Markdown-Quelltext, nicht als Rich Text. Der vollständige Quelltext bleibt erhalten, die Formatierung wird jedoch nicht angezeigt. Die Quellarbeitsmappe bleibt unverändert. Speichern Sie als Thinoda, um Notizformate zu erhalten.",
    ),
    (
        "Excel note export limit reached. The source is unchanged; shorten the note or save as Thinoda.",
        "Excel-Notizexportlimit erreicht. Die Quelle bleibt unverändert; kürzen Sie die Notiz oder speichern Sie als Thinoda.",
    ),
    (
        "Excel cannot preserve this note's formatting. Links, images, HTML, tabs and advanced Markdown are not supported. The source is unchanged; save as Thinoda.",
        "Excel kann die Formatierung dieser Notiz nicht erhalten. Links, Bilder, HTML, Tabulatoren und erweitertes Markdown werden nicht unterstützt. Die Quelle bleibt unverändert; speichern Sie als Thinoda.",
    ),
    (
        "FreeMind note export limit reached. The source is unchanged; shorten the note or save as Thinoda.",
        "Limit für den FreeMind-Notizexport erreicht. Die Quelle bleibt unverändert; kürzen Sie die Notiz oder speichern Sie als Thinoda.",
    ),
    (
        "FreeMind cannot preserve this note's formatting. Links, images, HTML, advanced Markdown, tabs and repeated spaces are not supported. The source is unchanged; save as Thinoda.",
        "FreeMind kann die Formatierung dieser Notiz nicht erhalten. Links, Bilder, HTML, erweitertes Markdown, Tabulatoren und wiederholte Leerzeichen werden nicht unterstützt. Die Quelle bleibt unverändert; speichern Sie als Thinoda.",
    ),
    (
        "Word note export limit reached. The source is unchanged; shorten the note or save as Thinoda.",
        "Limit für den Word-Notizexport erreicht. Die Quelle bleibt unverändert; kürzen Sie die Notiz oder speichern Sie als Thinoda.",
    ),
    (
        "Word cannot preserve this note's formatting. Links, images, HTML and advanced Markdown are not supported. The source is unchanged; save as Thinoda.",
        "Word kann die Formatierung dieser Notiz nicht erhalten. Links, Bilder, HTML und erweitertes Markdown werden nicht unterstützt. Die Quelle bleibt unverändert; speichern Sie als Thinoda.",
    ),
    ("Copy note as plain text", "Notiz als Klartext kopieren"),
    (
        "Notes search uses visible Markdown text; oversized notes search the original source.",
        "Die Suche verwendet sichtbaren Markdown-Text; bei zu großen Notizen wird der Originaltext durchsucht.",
    ),
    (
        "Copies Markdown text without formatting or link/image destinations. The saved note is unchanged.",
        "Kopiert Markdown-Text ohne Formatierung oder Link-/Bildziele. Die gespeicherte Notiz bleibt unverändert.",
    ),
    ("Italic notes", "Notiztext kursiv"),
    ("Bulleted list", "Aufzählung"),
    ("Numbered list", "Nummerierte Liste"),
    ("Markdown preview", "Markdown-Vorschau"),
    (
        "Formatting is stored as Markdown text; other views and exports may show markup.",
        "Formatierung wird als Markdown-Text gespeichert; andere Ansichten und Exporte können die Auszeichnung anzeigen.",
    ),
    (
        "Preview does not open links or load images.",
        "Die Vorschau öffnet keine Links und lädt keine Bilder.",
    ),
    (
        "Note preview limit reached. The full source is preserved and editable.",
        "Vorschaulimit der Notiz erreicht. Der vollständige Quelltext bleibt erhalten und bearbeitbar.",
    ),
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
    ("Format whole card text", "Gesamten Kartentext formatieren"),
    ("Whole-card text", "Gesamter Kartentext"),
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
    ("Review AI Result", "KI-Ergebnis prüfen"),
    ("AI Pitch outline", "KI-Präsentationsgliederung"),
    ("Pitch instructions", "Anweisungen für die Präsentation"),
    (
        "Suggest Pitch outline",
        "Präsentationsgliederung vorschlagen",
    ),
    (
        "Use suggested outline",
        "Vorgeschlagene Gliederung verwenden",
    ),
    (
        "Discard suggested outline",
        "Vorgeschlagene Gliederung verwerfen",
    ),
    (
        "AI suggests slide order and titles only. Frames stay unchanged. Send transmits slide titles and instructions.",
        "Die KI schlägt nur Reihenfolge und Titel vor. Ausschnitte bleiben unverändert. Senden überträgt Folientitel und Anweisungen.",
    ),
    (
        "AI Pitch requires 1 to 64 slides and at most 64 KiB of titles.",
        "KI-Präsentationen benötigen 1 bis 64 Folien und höchstens 64 KiB Titel.",
    ),
    (
        "AI Pitch returned an invalid outline. Retry with candidate numbers and titles only.",
        "Die KI lieferte eine ungültige Gliederung. Erneut nur mit Kandidatennummern und Titeln versuchen.",
    ),
    ("AI Chat", "KI-Chat"),
    ("Embedding model", "Embedding-Modell"),
    ("Embedding models", "Embedding-Modelle"),
    (
        "No embedding models found on this server",
        "Auf diesem Server wurden keine Embedding-Modelle gefunden",
    ),
    (
        "Required for semantic tools",
        "Für semantische Werkzeuge erforderlich",
    ),
    ("Semantic tools", "Semantische Werkzeuge"),
    ("Build semantic index", "Semantischen Index erstellen"),
    (
        "Build sends current-sheet titles to the embedding model. No background indexing.",
        "Erstellen sendet die Titel des aktuellen Blatts an das Embedding-Modell. Keine Hintergrundindexierung.",
    ),
    (
        "Set an explicit embedding model in Settings before indexing.",
        "Wähle vor der Indexierung ein Embedding-Modell in den Einstellungen.",
    ),
    (
        "Semantic indexing is limited to 500 topics and 64 KiB of titles.",
        "Die semantische Indexierung ist auf 500 Themen und 64 KiB Titel begrenzt.",
    ),
    (
        "Semantic indexing requires nonempty titles of at most 16 KiB each.",
        "Die semantische Indexierung erfordert nichtleere Titel mit jeweils höchstens 16 KiB.",
    ),
    (
        "Add at least one topic before indexing.",
        "Füge vor der Indexierung mindestens ein Thema hinzu.",
    ),
    (
        "The embedding model returned an unusable vector.",
        "Das Embedding-Modell hat einen unbrauchbaren Vektor geliefert.",
    ),
    (
        "The embedding model changed vector dimensions.",
        "Das Embedding-Modell hat die Vektordimension geändert.",
    ),
    (
        "Build the semantic index first.",
        "Erstelle zuerst den semantischen Index.",
    ),
    (
        "Semantic source or settings changed. Rebuild the index.",
        "Semantische Quelle oder Einstellungen wurden geändert. Erstelle den Index neu.",
    ),
    (
        "Cancel indexing or search",
        "Indexierung oder Suche abbrechen",
    ),
    ("Semantic query", "Semantische Suchanfrage"),
    ("Search meaning", "Nach Bedeutung suchen"),
    ("Cluster count", "Gruppenanzahl"),
    ("Preview clusters", "Gruppen vorschauen"),
    (
        "Apply clusters to new sheet",
        "Gruppen auf neuem Blatt anwenden",
    ),
    ("Semantic clusters", "Semantische Gruppen"),
    ("Group", "Gruppe"),
    ("AI: Rewrite Topic…", "KI: Thema umformulieren…"),
    ("AI: Translate Branch…", "KI: Zweig übersetzen…"),
    ("AI: Grow Ideas…", "KI: Ideen erweitern…"),
    ("AI: Reorganize Branch…", "KI: Zweig neu ordnen…"),
    ("AI: Refine Branch…", "KI: Zweig verfeinern…"),
    (
        "Target language and translation instructions",
        "Zielsprache und Übersetzungsanweisungen",
    ),
    (
        "Instructions for this transformation",
        "Anweisungen für diese Umwandlung",
    ),
    (
        "Only titles and hierarchy are sent. Notes and attachments stay local.",
        "Nur Titel und Hierarchie werden gesendet. Notizen und Anhänge bleiben lokal.",
    ),
    (
        "Apply creates a new sheet. The source stays unchanged.",
        "Anwenden erstellt ein neues Blatt. Die Quelle bleibt unverändert.",
    ),
    ("Saved prompts", "Gespeicherte Prompts"),
    ("New prompt", "Neuer Prompt"),
    ("Edit prompt", "Prompt bearbeiten"),
    ("Use prompt", "Prompt verwenden"),
    ("Reload prompts", "Prompts neu laden"),
    ("Edit saved prompt", "Gespeicherten Prompt bearbeiten"),
    ("Prompt name", "Prompt-Name"),
    ("Prompt text", "Prompt-Text"),
    ("Save prompt", "Prompt speichern"),
    ("Delete prompt", "Prompt löschen"),
    (
        "Delete this saved prompt?",
        "Diesen gespeicherten Prompt löschen?",
    ),
    ("Confirm Delete", "Löschen bestätigen"),
    ("Keep prompt", "Prompt behalten"),
    (
        "Replace the unsent draft with this prompt?",
        "Den ungesendeten Entwurf durch diesen Prompt ersetzen?",
    ),
    ("Replace draft", "Entwurf ersetzen"),
    ("Keep draft", "Entwurf behalten"),
    (
        "Use prompt fills the draft without sending. Review it before Send.",
        "Prompt verwenden füllt den Entwurf ohne Senden aus. Prüfe ihn vor dem Senden.",
    ),
    (
        "Prompts are saved locally in settings. Do not include passwords or secrets.",
        "Prompts werden lokal in den Einstellungen gespeichert. Keine Passwörter oder Geheimnisse eingeben.",
    ),
    (
        "Could not load saved prompts",
        "Gespeicherte Prompts konnten nicht geladen werden",
    ),
    (
        "Could not save prompts",
        "Prompts konnten nicht gespeichert werden",
    ),
    (
        "Save at most 32 prompts.",
        "Speichere höchstens 32 Prompts.",
    ),
    (
        "Saved prompt IDs must be unique and valid.",
        "Gespeicherte Prompt-IDs müssen eindeutig und gültig sein.",
    ),
    (
        "Enter a prompt name of at most 80 bytes.",
        "Gib einen Prompt-Namen mit höchstens 80 Bytes ein.",
    ),
    (
        "Prompt names must be unique.",
        "Prompt-Namen müssen eindeutig sein.",
    ),
    (
        "Saved prompts exceed the 128 KiB library limit.",
        "Gespeicherte Prompts überschreiten das Bibliothekslimit von 128 KiB.",
    ),
    (
        "Saved prompts changed elsewhere. Cancel and reload prompts.",
        "Gespeicherte Prompts wurden andernorts geändert. Brich ab und lade die Prompts neu.",
    ),
    (
        "Secure legacy credentials in Settings before saving prompts.",
        "Sichere alte Zugangsdaten in den Einstellungen vor dem Speichern von Prompts.",
    ),
    ("Response mode", "Antwortmodus"),
    ("Context scope", "Kontextumfang"),
    (
        "Chat history is session-only.",
        "Der Chatverlauf gilt nur für diese Sitzung.",
    ),
    ("Selected topics", "Ausgewählte Themen"),
    ("Quick", "Schnell"),
    ("Balanced", "Ausgewogen"),
    ("Deep", "Vertieft"),
    (
        "Finish the current edit before sending.",
        "Beende die aktuelle Bearbeitung vor dem Senden.",
    ),
    (
        "AI context is too large. Select a smaller branch.",
        "Der KI-Kontext ist zu groß. Wähle einen kleineren Zweig.",
    ),
    (
        "AI reply is too large. Try a shorter request.",
        "Die KI-Antwort ist zu groß. Versuche eine kürzere Anfrage.",
    ),
    (
        "Enter a prompt of at most 8192 bytes.",
        "Gib eine Anfrage mit höchstens 8192 Bytes ein.",
    ),
    (
        "Conversation is full. Clear it before sending again.",
        "Der Chat ist voll. Leere ihn vor dem erneuten Senden.",
    ),
    (
        "Conversation limit reached. Close another document.",
        "Das Chat-Limit ist erreicht. Schließe ein anderes Dokument.",
    ),
    (
        "Modes guide detail, not provider reasoning settings.",
        "Modi steuern den Detailgrad, nicht die Denkfunktionen des Anbieters.",
    ),
    (
        "Chat never edits maps. Context includes titles, hierarchy and this chat's history, not notes or attachments.",
        "Der Chat bearbeitet keine Maps. Der Kontext enthält Titel, Hierarchie und diesen Chatverlauf, aber keine Notizen oder Anhänge.",
    ),
    ("Context to send", "Zu sendender Kontext"),
    ("Ask about this map", "Frage zu dieser Map stellen"),
    ("Send", "Senden"),
    ("Clear conversation", "Chat leeren"),
    ("Clear this conversation?", "Diesen Chat leeren?"),
    ("Confirm Clear", "Leeren bestätigen"),
    ("Assistant", "Assistent"),
    ("You", "Du"),
    ("Cancel AI Request", "KI-Anfrage abbrechen"),
    ("Apply AI Result", "KI-Ergebnis anwenden"),
    ("Discard AI Result", "KI-Ergebnis verwerfen"),
    (
        "Nothing changes until you apply this result.",
        "Erst beim Anwenden dieses Ergebnisses wird etwas geändert.",
    ),
    (
        "Review the current AI result first.",
        "Prüfe zuerst das aktuelle KI-Ergebnis.",
    ),
    (
        "AI settings changed. Try again.",
        "Die KI-Einstellungen wurden geändert. Versuche es erneut.",
    ),
    (
        "The AI response has an unexpected format.",
        "Die KI-Antwort hat ein unerwartetes Format.",
    ),
    (
        "This result creates a new map.",
        "Dieses Ergebnis erstellt eine neue Map.",
    ),
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
    (
        "Complete subtree To-Dos",
        "Untergeordnete Aufgaben abschließen",
    ),
    (
        "Reopen subtree To-Dos",
        "Untergeordnete Aufgaben wieder öffnen",
    ),
    (
        "Counts deepest To-Dos only; scheduled Tasks stay unchanged.",
        "Zählt nur Aufgaben auf der untersten Ebene; die Aufgabenplanung bleibt unverändert.",
    ),
    ("Numbering", "Nummerierung"),
    ("Main Menu", "Hauptmenü"),
    ("Pin", "Anheften"),
    ("Unpin", "Lösen"),
    ("Tiered numbers", "Mehrstufige Nummern"),
    ("Start at", "Beginnen bei"),
    (
        "Select a subtopic to number it together with its siblings.",
        "Wählen Sie ein Unterthema, um es zusammen mit seinen Geschwistern zu nummerieren.",
    ),
    (
        "Applies to the selected topic and its siblings; numbers update as topics move.",
        "Gilt für das ausgewählte Thema und seine Geschwister; Nummern passen sich beim Verschieben an.",
    ),
    ("Open Link", "Link öffnen"),
    ("Main Topic", "Hauptthema"),
    ("Central Topic", "Zentrales Thema"),
    // Format panel
    ("Style", "Stil"),
    ("Data", "Daten"),
    ("Equation", "Gleichung"),
    ("Enlarge notes editor", "Notizeneditor vergrößern"),
    ("Enlarge equation editor", "Gleichungseditor vergrößern"),
    ("Notes editor", "Notizeneditor"),
    (
        "Changing the selection, tab or document revision closes enlarged editors without applying their edits. Apply before editing elsewhere.",
        "Ein Wechsel der Auswahl, des Tabs oder der Dokumentrevision schließt vergrößerte Editoren ohne Übernahme. Übernimm Änderungen vor dem Bearbeiten an anderer Stelle.",
    ),
    ("Equation editor", "Gleichungseditor"),
    ("Apply notes", "Notizen übernehmen"),
    ("Cancel notes edit", "Notizenbearbeitung abbrechen"),
    (
        "Apply to keep edits. Cancel discards them. Unapplied dialog edits are not saved or recovered.",
        "Übernehmen behält Änderungen. Abbrechen verwirft sie. Nicht übernommene Dialogänderungen werden weder gespeichert noch wiederhergestellt.",
    ),
    (
        "Data is between Style and Map. It groups images, attachments, hyperlinks, labels, notes, notes equations and Task Planning in Map and Outliner.",
        "Daten liegt zwischen Stil und Map. Hier findest du Bilder, Anhänge, Hyperlinks, Labels, Notizen, Notizgleichungen und Aufgabenplanung in Map und Gliederung.",
    ),
    (
        "Data > Local Image provides width, placement and reset controls. Resize on the map with the lower-right handle; Escape cancels and arrow keys resize a focused handle.",
        "Data > Lokales Bild bietet Breite, Platzierung und Zurücksetzen. Auf der Map änderst du die Größe am Griff unten rechts; Escape bricht ab und Pfeiltasten ändern die Größe eines fokussierten Griffs.",
    ),
    (
        "Insert > Attachment embeds a local file as a child topic (1 byte to 8 MiB). Data > Attachment saves or downloads it separately. Nothing is opened or executed. Save as .thinoda to retain attachments; other exports are not supported.",
        "Einfügen > Anhang bettet eine lokale Datei als Unterthema ein (1 Byte bis 8 MiB). Data > Anhang speichert oder lädt sie separat herunter. Nichts wird geöffnet oder ausgeführt. Speichere als .thinoda, um Anhänge zu erhalten; andere Exporte werden nicht unterstützt.",
    ),
    (
        "Select a topic to edit its data.",
        "Wähle ein Thema aus, um seine Daten zu bearbeiten.",
    ),
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
        "Changes apply to unlocked selected items that support each setting.",
        "Änderungen gelten für entsperrte ausgewählte Elemente, die die jeweilige Einstellung unterstützen.",
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
    ("Pitch", "Pitch"),
    (
        "Embedded resources are not supported in Pitch yet. Save as .thinoda to preserve them.",
        "Eingebettete Ressourcen werden in Pitch noch nicht unterstützt. Speichern Sie als .thinoda, um sie zu erhalten.",
    ),
    ("Edit Presentation", "Präsentation bearbeiten"),
    ("Presenter View", "Referentenansicht"),
    ("Generate Slides", "Folien erzeugen"),
    (
        "Add Selected Branch or Zone",
        "Ausgewählten Zweig oder Zone hinzufügen",
    ),
    (
        "Generate replaces the slide draft. Apply saves one undoable change; Cancel keeps the saved presentation.",
        "Erzeugen ersetzt den Folienentwurf. Anwenden speichert eine rückgängig machbare Änderung; Abbrechen behält die gespeicherte Präsentation.",
    ),
    (
        "No slides yet. Generate branch and Zone slides, or add the selected branch.",
        "Noch keine Folien. Erzeuge Zweig- und Zonenfolien oder füge den ausgewählten Zweig hinzu.",
    ),
    ("Slide title", "Folientitel"),
    ("Fit Slide to Content", "Folie an Inhalt anpassen"),
    ("Remove Slide", "Folie entfernen"),
    ("Export PDF", "PDF exportieren"),
    ("Export Slide Images", "Folienbilder exportieren"),
    ("Slide preview", "Folienvorschau"),
    ("Slide preview failed", "Folienvorschau fehlgeschlagen"),
    ("Retry", "Erneut versuchen"),
    ("Rendering slide…", "Folie wird gerendert…"),
    (
        "The document changed. Restart Pitch to show the current map.",
        "Das Dokument wurde geändert. Starte Pitch neu, um die aktuelle Map zu zeigen.",
    ),
    ("Previous Slide", "Vorherige Folie"),
    ("First Slide", "Erste Folie"),
    ("Last Slide", "Letzte Folie"),
    ("Transition", "Übergang"),
    ("Fade", "Überblenden"),
    ("Slide", "Einschieben"),
    ("Push", "Verschieben"),
    ("Reduce motion", "Bewegung reduzieren"),
    ("Build topics", "Themen schrittweise zeigen"),
    (
        "Right / Space: next · Left: previous · Home / End: first / last · Escape: exit",
        "Rechts / Leertaste: weiter · Links: zurück · Pos1 / Ende: erste / letzte · Escape: beenden",
    ),
    ("Next Slide", "Nächste Folie"),
    ("Restore Player", "Player verkleinern"),
    ("Expand Player", "Player vergrößern"),
    ("Leave Fullscreen", "Vollbild verlassen"),
    ("Fullscreen", "Vollbild"),
    ("Exit Pitch", "Pitch beenden"),
    (
        "Right / Space: next · Left: previous · Escape: exit",
        "Rechts / Leertaste: weiter · Links: zurück · Escape: beenden",
    ),
    ("Zone", "Zone"),
    ("Zones", "Zonen"),
    ("Create Zone", "Zone erstellen"),
    ("Add to Zone…", "Zur Zone hinzufügen…"),
    ("Remove from Zone", "Aus Zone entfernen"),
    ("Manage Zones", "Zonen verwalten"),
    (
        "Select floating topics or board objects.",
        "Schwebende Themen oder Board-Objekte auswählen.",
    ),
    (
        "Choose a Zone. Items stay in place; existing memberships are transferred.",
        "Eine Zone auswählen. Elemente bleiben an ihrer Position; bestehende Zuordnungen werden übertragen.",
    ),
    (
        "Zones group floating topics and board objects. Removing a Zone keeps its contents.",
        "Zonen gruppieren schwebende Themen und Board-Objekte. Beim Entfernen einer Zone bleiben ihre Inhalte erhalten.",
    ),
    ("Edit Zone", "Zone bearbeiten"),
    ("Remove Zone", "Zone entfernen"),
    ("Name", "Name"),
    ("Height", "Höhe"),
    ("X", "X"),
    ("Y", "Y"),
    (
        "Select floating topics to create a Zone.",
        "Wähle freie Themen aus, um eine Zone zu erstellen.",
    ),
    (
        "Zones group floating topics and their descendants. Removing a Zone keeps its topics.",
        "Zonen gruppieren freie Themen und deren Unterthemen. Beim Entfernen einer Zone bleiben ihre Themen erhalten.",
    ),
    (
        "No Zones yet. Select floating topics, then create a Zone.",
        "Noch keine Zonen. Wähle freie Themen aus und erstelle eine Zone.",
    ),
    (
        "Moving a Zone moves its topics. Minimum size is 100 × 100.",
        "Beim Verschieben einer Zone werden ihre Themen mitverschoben. Mindestgröße: 100 × 100.",
    ),
    ("Task Planning", "Aufgabenplanung"),
    ("Gantt Chart", "Gantt-Diagramm"),
    ("Work Calendar", "Arbeitskalender"),
    ("Start date", "Startdatum"),
    ("Due date", "Fälligkeitsdatum"),
    ("Progress", "Fortschritt"),
    (
        "Working days (0 = unscheduled)",
        "Arbeitstage (0 = ungeplant)",
    ),
    ("Priority (0 = none)", "Priorität (0 = keine)"),
    ("Finish-to-start predecessors", "Ende-Anfang-Vorgänger"),
    ("Remove Task", "Aufgabenplanung entfernen"),
    ("Dated exceptions", "Datierte Ausnahmen"),
    ("Exception date", "Ausnahmedatum"),
    ("Working day", "Arbeitstag"),
    ("Add Exception", "Ausnahme hinzufügen"),
    ("YYYY-MM-DD", "JJJJ-MM-TT"),
    (
        "Set a start date before a due date.",
        "Vor dem Fälligkeitsdatum ein Startdatum festlegen.",
    ),
    ("Timeline zoom", "Zeitleistenzoom"),
    (
        "Export worker stopped unexpectedly",
        "Exportprozess wurde unerwartet beendet",
    ),
    (
        "Fit the complete Gantt chart on one page.",
        "Das vollständige Gantt-Diagramm auf eine Seite einpassen.",
    ),
    ("Unscheduled", "Ungeplant"),
    (
        "Too many tasks for the interactive chart. Use Gantt export.",
        "Zu viele Aufgaben für das interaktive Diagramm. Gantt-Export verwenden.",
    ),
    ("Non-working day", "Arbeitsfreier Tag"),
    (
        "Dates use YYYY-MM-DD. Duration counts inclusive working days.",
        "Datumsformat: JJJJ-MM-TT. Die Dauer zählt Arbeitstage einschließlich Start- und Endtag.",
    ),
    (
        "Removing a task also removes its dependencies. To-Do stays unchanged.",
        "Das Entfernen einer Aufgabenplanung entfernt auch ihre Abhängigkeiten. To-Do bleibt unverändert.",
    ),
    (
        "This calendar belongs to the current sheet and is saved with the map.",
        "Dieser Kalender gehört zum aktuellen Blatt und wird mit der Map gespeichert.",
    ),
    (
        "Timeline too wide. Reduce zoom or narrow the schedule.",
        "Zeitleiste zu breit. Zoom reduzieren oder Planung eingrenzen.",
    ),
    (
        "No scheduled tasks yet. Select a topic in the map and choose Insert > Task Planning.",
        "Noch keine geplanten Aufgaben. Ein Thema auswählen und Einfügen > Aufgabenplanung wählen.",
    ),
    (
        "Select a task to edit dates, progress and dependencies. Changes update the map and Undo history.",
        "Eine Aufgabe auswählen, um Termine, Fortschritt und Abhängigkeiten zu bearbeiten. Änderungen aktualisieren die Map und den Rückgängig-Verlauf.",
    ),
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
    ("Keep recovery data", "Wiederherstellungsdaten behalten"),
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
    ("Cloud", "Wolke"),
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
    ("Saving file:", "Datei wird gespeichert:"),
    (
        "The file writer stopped unexpectedly",
        "Das Speichern der Datei wurde unerwartet abgebrochen",
    ),
    (
        "Wait until the file is saved",
        "Warten Sie, bis die Datei gespeichert ist",
    ),
    ("Box", "Box"),
    ("Browse cloud files…", "Cloud-Dateien durchsuchen…"),
    ("Browser document storage", "Dokumentspeicher des Browsers"),
    (
        "Browser storage information is unavailable.",
        "Informationen zum Browserspeicher sind nicht verfügbar.",
    ),
    (
        "Browser storage is private to this site. Check quota or request persistence explicitly; the browser may decline.",
        "Der Browserspeicher ist nur für diese Website zugänglich. Prüfen Sie das Kontingent oder fordern Sie ausdrücklich dauerhafte Speicherung an; der Browser kann dies ablehnen.",
    ),
    (
        "Browser storage persistence is granted.",
        "Dauerhafte Speicherung im Browser wurde gewährt.",
    ),
    (
        "Browser storage persistence is not granted.",
        "Dauerhafte Speicherung im Browser wurde nicht gewährt.",
    ),
    (
        "Browser storage persistence request failed.",
        "Die Anfrage für dauerhafte Speicherung im Browser ist fehlgeschlagen.",
    ),
    (
        "Browser storage persistence status is unavailable.",
        "Der Status der dauerhaften Browserspeicherung ist nicht verfügbar.",
    ),
    ("Check storage usage", "Speichernutzung prüfen"),
    (
        "Cloud sign-in configuration is invalid. Check the server address or contact the publisher.",
        "Die Cloud-Anmeldekonfiguration ist ungültig. Prüfen Sie die Serveradresse oder wenden Sie sich an den Herausgeber.",
    ),
    (
        "Complete sign-in after the browser redirect.",
        "Schließen Sie die Anmeldung nach der Browser-Umleitung ab.",
    ),
    ("Checking browser storage…", "Browserspeicher wird geprüft…"),
    (
        "Choose a backup folder outside detected cloud sync folders. This does not change the provider's synchronization or version history.",
        "Wählen Sie einen Sicherungsordner außerhalb erkannter Cloud-Synchronisierungsordner. Dies ändert weder die Synchronisierung des Anbieters noch den Versionsverlauf.",
    ),
    (
        "Choose a single file name ending in .thinoda.",
        "Wählen Sie einen einzelnen Dateinamen mit der Endung .thinoda.",
    ),
    (
        "Choose an existing folder outside every detected cloud sync folder.",
        "Wählen Sie einen vorhandenen Ordner außerhalb aller erkannten Cloud-Synchronisierungsordner.",
    ),
    ("Choose backup folder…", "Sicherungsordner auswählen…"),
    ("Cloud accounts", "Cloud-Konten"),
    (
        "Cloud accounts are optional. Selecting a provider or opening Settings never connects.",
        "Cloud-Konten sind optional. Durch die Auswahl eines Anbieters oder das Öffnen der Einstellungen wird niemals eine Verbindung hergestellt.",
    ),
    (
        "Cloud connection changed. Downloaded content was not installed.",
        "Die Cloud-Verbindung wurde geändert. Heruntergeladene Inhalte wurden nicht übernommen.",
    ),
    ("Cloud document", "Cloud-Dokument"),
    ("Cloud document saved.", "Cloud-Dokument gespeichert."),
    ("Cloud save conflict", "Cloud-Speicherkonflikt"),
    (
        "The cloud revision changed or the new file name already exists. Your edits remain open. No overwrite or retry will be attempted. Cancel this save, then use Save As locally or upload a new copy with a different name.",
        "Die Cloud-Revision hat sich geändert oder der neue Dateiname ist bereits vorhanden. Ihre Änderungen bleiben geöffnet. Es wird weder überschrieben noch erneut versucht. Brechen Sie diesen Speichervorgang ab und verwenden Sie lokal „Speichern unter“ oder laden Sie eine neue Kopie unter einem anderen Namen hoch.",
    ),
    (
        "Cloud file changed. No overwrite was attempted; open the remote file separately or upload a new copy.",
        "Die Cloud-Datei wurde geändert. Es wurde kein Überschreiben versucht; öffnen Sie die entfernte Datei separat oder laden Sie eine neue Kopie hoch.",
    ),
    ("Cloud file name", "Name der Cloud-Datei"),
    ("Cloud files", "Cloud-Dateien"),
    ("Cloud folder listed.", "Cloud-Ordner aufgelistet."),
    ("Cloud operation", "Cloud-Vorgang"),
    (
        "Cloud operation cancelled. An upload may already have reached the server; reopen before saving again.",
        "Cloud-Vorgang abgebrochen. Ein Upload könnte den Server bereits erreicht haben; öffnen Sie die Datei erneut, bevor Sie wieder speichern.",
    ),
    (
        "Cloud creation returned no stable identity or revision. The upload may have reached the server; reopen before saving again.",
        "Die Cloud-Erstellung lieferte keine stabile Identität oder Revision. Der Upload könnte den Server bereits erreicht haben; öffnen Sie die Datei erneut, bevor Sie wieder speichern.",
    ),
    ("Cloud provider", "Cloud-Anbieter"),
    (
        "Cloud request failed. Your local edits are unchanged.",
        "Cloud-Anfrage fehlgeschlagen. Ihre lokalen Änderungen sind unverändert.",
    ),
    ("Cloud sign-in cancelled.", "Cloud-Anmeldung abgebrochen."),
    (
        "Cloud sign-in failed. Live provider authentication has not been verified for this build.",
        "Cloud-Anmeldung fehlgeschlagen. Die Authentifizierung bei Live-Anbietern wurde für diesen Build nicht verifiziert.",
    ),
    (
        "Cloud snapshot uploaded. Newer edits or the local source still need saving.",
        "Cloud-Momentaufnahme hochgeladen. Neuere Änderungen oder die lokale Quelldatei müssen noch gespeichert werden.",
    ),
    (
        "Cloud worker could not start. No request was made.",
        "Der Cloud-Prozess konnte nicht gestartet werden. Es wurde keine Anfrage gesendet.",
    ),
    (
        "Cloud worker stopped. Your local edits are unchanged.",
        "Der Cloud-Prozess wurde beendet. Ihre lokalen Änderungen sind unverändert.",
    ),
    ("Connect", "Verbinden"),
    (
        "Provider sign-in is not enabled in this build. The publisher must configure a registered OAuth application before you can connect.",
        "Die Anmeldung beim Anbieter ist in diesem Build nicht aktiviert. Der Herausgeber muss eine registrierte OAuth-Anwendung konfigurieren, bevor Sie eine Verbindung herstellen können.",
    ),
    (
        "Connect opens the selected provider’s sign-in page so you can authorize Thinoda. Account folders are chosen automatically; no registration IDs are needed.",
        "„Verbinden“ öffnet die Anmeldeseite des ausgewählten Anbieters, damit Sie Thinoda autorisieren können. Kontoordner werden automatisch ausgewählt; Registrierungs-IDs sind nicht erforderlich.",
    ),
    (
        "Web sign-in redirects this tab. Save local work before connecting.",
        "Bei der Webanmeldung wird dieser Tab weitergeleitet. Speichern Sie Ihre lokale Arbeit, bevor Sie eine Verbindung herstellen.",
    ),
    ("Connect cloud account", "Cloud-Konto verbinden"),
    (
        "Connect contacts your configured provider and may open its sign-in page. Browsing sends folder identifiers; Open downloads content. Save and Upload send document content only when you request them.",
        "Beim Verbinden wird Ihr konfigurierter Anbieter kontaktiert und möglicherweise dessen Anmeldeseite geöffnet. Beim Durchsuchen werden Ordnerkennungen gesendet; Öffnen lädt Inhalte herunter. Speichern und Hochladen übertragen Dokumentinhalte nur auf Ihre Anforderung.",
    ),
    (
        "Connected. Choose List folder to browse; no files are downloaded automatically.",
        "Verbunden. Wählen Sie „Ordner auflisten“, um ihn zu durchsuchen; Dateien werden nicht automatisch heruntergeladen.",
    ),
    ("Connecting cloud account…", "Cloud-Konto wird verbunden…"),
    ("Copy link", "Link kopieren"),
    (
        "Could not commit document drafts. Your edits are unchanged.",
        "Dokumententwürfe konnten nicht übernommen werden. Ihre Änderungen sind unverändert.",
    ),
    (
        "Could not encode the cloud document. Your local edits are unchanged.",
        "Das Cloud-Dokument konnte nicht codiert werden. Ihre lokalen Änderungen sind unverändert.",
    ),
    (
        "Could not import the cloud document. No existing document was changed.",
        "Das Cloud-Dokument konnte nicht importiert werden. Kein vorhandenes Dokument wurde geändert.",
    ),
    (
        "Could not open the cloud sign-in page.",
        "Die Cloud-Anmeldeseite konnte nicht geöffnet werden.",
    ),
    (
        "Create restricted share link",
        "Eingeschränkten Freigabelink erstellen",
    ),
    ("Create share link", "Freigabelink erstellen"),
    (
        "Creating restricted share link…",
        "Eingeschränkter Freigabelink wird erstellt…",
    ),
    (
        "Credentials use the OS keychain on desktop and this tab's session storage on web. No cloud autosave or background reconnection.",
        "Anmeldedaten werden auf dem Desktop im Schlüsselbund des Betriebssystems und im Web im Sitzungsspeicher dieses Tabs abgelegt. Es gibt weder automatisches Speichern in der Cloud noch eine Wiederverbindung im Hintergrund.",
    ),
    ("Disconnect", "Trennen"),
    ("Disconnect cloud account", "Cloud-Konto trennen"),
    (
        "Disconnected locally. Provider grants and remote files were not deleted; revoke access in the provider's account settings.",
        "Lokal getrennt. Anbieterberechtigungen und entfernte Dateien wurden nicht gelöscht; widerrufen Sie den Zugriff in den Kontoeinstellungen des Anbieters.",
    ),
    ("Disconnecting cloud account…", "Cloud-Konto wird getrennt…"),
    (
        "Downloading cloud document…",
        "Cloud-Dokument wird heruntergeladen…",
    ),
    ("Dropbox", "Dropbox"),
    (
        "Dropbox's starting folder must be an absolute path or empty for the app root.",
        "Der Startordner von Dropbox muss ein absoluter Pfad oder leer sein, um den App-Stammordner zu verwenden.",
    ),
    ("File ID", "Datei-ID"),
    ("Folder", "Ordner"),
    ("Google Drive", "Google Drive"),
    (
        "Google Drive allows duplicate file names. This upload creates a distinct new file in the selected folder, even if the same name already exists. Existing files are not replaced. Later Save targets only the returned file ID.",
        "Google Drive erlaubt doppelte Dateinamen. Dieser Upload erstellt im ausgewählten Ordner eine eigene neue Datei, auch wenn bereits eine Datei mit demselben Namen vorhanden ist. Vorhandene Dateien werden nicht ersetzt. Spätere Speichervorgänge gelten nur für die zurückgegebene Datei-ID.",
    ),
    ("I agree to this action", "Ich stimme dieser Aktion zu"),
    (
        "I authorize Thinoda to contact this provider, open sign-in if needed, and store credentials securely. This does not browse, upload or share any document.",
        "Ich ermächtige Thinoda, diesen Anbieter zu kontaktieren, bei Bedarf die Anmeldung zu öffnen und Anmeldedaten sicher zu speichern. Dabei wird kein Dokument durchsucht, hochgeladen oder freigegeben.",
    ),
    ("List folder", "Ordner auflisten"),
    ("Listing cloud folder…", "Cloud-Ordner wird aufgelistet…"),
    (
        "Loading version metadata…",
        "Versionsmetadaten werden geladen…",
    ),
    ("Local backups", "Lokale Sicherungen"),
    (
        "Nextcloud / ownCloud / WebDAV",
        "Nextcloud / ownCloud / WebDAV",
    ),
    (
        "Nextcloud default: remote.php/dav/files. Leave empty if the server URL is the complete DAV collection.",
        "Nextcloud-Standard: remote.php/dav/files. Lassen Sie das Feld leer, wenn die Server-URL bereits die vollständige DAV-Sammlung bezeichnet.",
    ),
    (
        "No stored credentials. Start a new sign-in.",
        "Keine gespeicherten Anmeldedaten. Starten Sie eine neue Anmeldung.",
    ),
    ("OneDrive", "OneDrive"),
    (
        "OneDrive connection locates the app folder and may create it. No document is transferred, and access is not broadened if discovery fails.",
        "Beim Verbinden mit OneDrive wird der App-Ordner gesucht und gegebenenfalls erstellt. Es werden keine Dokumente übertragen und der Zugriff wird nicht erweitert, falls die Suche fehlschlägt.",
    ),
    (
        "OneDrive requires the drive ID and app-folder item ID for your registration.",
        "OneDrive benötigt die Laufwerk-ID und die Element-ID des App-Ordners für Ihre Registrierung.",
    ),
    ("Open", "Öffnen"),
    ("Open remote copy", "Entfernte Kopie öffnen"),
    ("Parent folder", "Übergeordneter Ordner"),
    (
        "Progress is indeterminate. Cancel cannot undo an upload already accepted by the provider.",
        "Der Fortschritt ist nicht bestimmbar. Abbrechen kann einen bereits vom Anbieter angenommenen Upload nicht rückgängig machen.",
    ),
    (
        "Remove this connection and its stored credentials on this device/tab. Documents and unsaved edits remain open. Remote files and provider grants are not deleted.",
        "Diese Verbindung und ihre gespeicherten Anmeldedaten auf diesem Gerät bzw. in diesem Tab entfernen. Dokumente und ungespeicherte Änderungen bleiben geöffnet. Entfernte Dateien und Anbieterberechtigungen werden nicht gelöscht.",
    ),
    (
        "Request persistent storage",
        "Dauerhafte Speicherung anfordern",
    ),
    (
        "Requesting persistent storage…",
        "Dauerhafte Speicherung wird angefordert…",
    ),
    ("Restricted share link", "Eingeschränkter Freigabelink"),
    (
        "Restricted share link created. It was not opened or copied automatically.",
        "Eingeschränkter Freigabelink erstellt. Er wurde nicht automatisch geöffnet oder kopiert.",
    ),
    ("Revision", "Revision"),
    ("Root folder", "Stammordner"),
    (
        "Save local work before redirecting this tab to sign in.",
        "Speichern Sie Ihre lokale Arbeit, bevor dieser Tab zur Anmeldung weitergeleitet wird.",
    ),
    (
        "Secure credential storage is unavailable. No connection was retained.",
        "Sichere Speicherung von Anmeldedaten ist nicht verfügbar. Es wurde keine Verbindung gespeichert.",
    ),
    (
        "Select and configure a cloud provider first.",
        "Wählen Sie zuerst einen Cloud-Anbieter aus und konfigurieren Sie ihn.",
    ),
    ("Storage quota (bytes)", "Speicherkontingent (Bytes)"),
    ("Storage used (bytes)", "Belegter Speicher (Bytes)"),
    (
        "Stored credentials do not match this configuration. Start a new sign-in.",
        "Die gespeicherten Anmeldedaten stimmen nicht mit dieser Konfiguration überein. Starten Sie eine neue Anmeldung.",
    ),
    (
        "This changes sharing on the selected remote file. Box uses existing collaborators; OneDrive uses organization-only view access. No anonymous/public link is requested. The provider may reject this permission.",
        "Dies ändert die Freigabe der ausgewählten entfernten Datei. Box verwendet vorhandene Mitwirkende; OneDrive gewährt nur der Organisation lesenden Zugriff. Es wird kein anonymer/öffentlicher Link angefordert. Der Anbieter kann diese Berechtigung ablehnen.",
    ),
    (
        "This document's cloud connection was disconnected. Save As locally or upload a new copy; reconnecting does not retarget it.",
        "Die Cloud-Verbindung dieses Dokuments wurde getrennt. Speichern Sie es lokal unter einem anderen Namen oder laden Sie eine neue Kopie hoch; durch erneutes Verbinden wird das Dokument nicht neu zugeordnet.",
    ),
    (
        "This folder has no safe new-file upload path. Choose another folder.",
        "Für diesen Ordner gibt es keinen sicheren Weg, eine neue Datei hochzuladen. Wählen Sie einen anderen Ordner.",
    ),
    (
        "This operation is unavailable for this provider or file size.",
        "Dieser Vorgang ist bei diesem Anbieter oder für diese Dateigröße nicht verfügbar.",
    ),
    ("Unavailable", "Nicht verfügbar"),
    (
        "Unsupported source content may be omitted. The cloud original is not changed.",
        "Nicht unterstützte Quellinhalte können ausgelassen werden. Das Cloud-Original wird nicht geändert.",
    ),
    ("Upload", "Hochladen"),
    (
        "Upload a snapshot of the current document to the chosen folder as a new .thinoda file. Existing files are never overwritten. Local documents remain local; unsaved documents gain an explicit cloud Save destination.",
        "Laden Sie eine Momentaufnahme des aktuellen Dokuments als neue .thinoda-Datei in den ausgewählten Ordner hoch. Vorhandene Dateien werden niemals überschrieben. Lokale Dokumente bleiben lokal; ungespeicherte Dokumente erhalten ein ausdrücklich gewähltes Cloud-Speicherziel.",
    ),
    ("Upload current document", "Aktuelles Dokument hochladen"),
    ("Upload current document…", "Aktuelles Dokument hochladen…"),
    ("Uploading…", "Wird hochgeladen…"),
    (
        "Use HTTPS without credentials, a query or a fragment.",
        "Verwenden Sie HTTPS ohne Anmeldedaten, Abfrageparameter oder Fragment.",
    ),
    (
        "Use an app password instead of browser sign-in",
        "Verwenden Sie anstelle der Browseranmeldung ein App-Passwort",
    ),
    (
        "Use a public native/SPA registration with PKCE, never a client secret. Permissions may restrict which files appear.",
        "Verwenden Sie eine öffentliche Registrierung für native Apps/SPAs mit PKCE, niemals ein Clientgeheimnis. Berechtigungen können einschränken, welche Dateien angezeigt werden.",
    ),
    (
        "Use previously stored credentials",
        "Zuvor gespeicherte Anmeldedaten verwenden",
    ),
    (
        "Version metadata loaded. No version content was downloaded.",
        "Versionsmetadaten geladen. Es wurden keine Versionsinhalte heruntergeladen.",
    ),
    ("Versions", "Versionen"),
    (
        "Versions and sharing are unavailable where the adapter does not implement them.",
        "Versionen und Freigaben sind nicht verfügbar, wenn der Adapter sie nicht implementiert.",
    ),
    ("WebDAV app password", "WebDAV-App-Passwort"),
    ("WebDAV collection prefix", "WebDAV-Sammlungspräfix"),
    ("WebDAV server URL", "WebDAV-Server-URL"),
    ("WebDAV username", "WebDAV-Benutzername"),
    ("Online-only", "Nur online"),
    (
        "Local copy; sync status unavailable",
        "Lokale Kopie; Synchronisierungsstatus nicht verfügbar",
    ),
    (
        "Sync status unavailable",
        "Synchronisierungsstatus nicht verfügbar",
    ),
    (
        "Choose a backup folder outside every detected cloud folder.",
        "Wählen Sie einen Sicherungsordner außerhalb aller erkannten Cloud-Ordner.",
    ),
    (
        "Use the asynchronous file workflow for browser documents.",
        "Verwenden Sie für Browserdokumente den asynchronen Datei-Workflow.",
    ),
    // FIM node wizard: labels, dynamic choices, and draft feedback.
    ("Edit FIM node", "FIM-Knoten bearbeiten"),
    ("Add FIM child", "FIM-Unterknoten hinzufügen"),
    ("Insert FIM node before", "FIM-Knoten davor einfügen"),
    ("Insert FIM node after", "FIM-Knoten danach einfügen"),
    ("Create FIM node", "FIM-Knoten erstellen"),
    ("Add child", "Unterknoten hinzufügen"),
    ("Insert after", "Danach einfügen"),
    ("Field constraints", "Feldeinschränkungen"),
    ("Reuse and structure", "Wiederverwendung und Struktur"),
    ("Rules and code lists", "Regeln und Codelisten"),
    ("Legal basis", "Rechtsnormgebunden"),
    ("Harmonized", "Harmonisiert"),
    ("Abstract", "Abstrakt"),
    ("Choice group", "Auswahlgruppe"),
    ("Input", "Eingabe"),
    ("Static", "Statisch"),
    ("Selection", "Auswahl"),
    ("Hidden", "Verborgen"),
    ("Boolean", "Wahrheitswert"),
    ("Date", "Datum"),
    ("Decimal number", "Dezimalzahl"),
    ("Integer", "Ganzzahl"),
    ("Currency", "Währung"),
    ("Object", "Objekt"),
    ("Date and time", "Datum und Uhrzeit"),
    ("Time", "Uhrzeit"),
    ("String.Latin+", "Zeichenfolge (String.Latin+)"),
    ("Validation", "Validierung"),
    ("Multiplicity", "Multiplizität"),
    ("Calculation", "Berechnung"),
    ("Complex rule", "Komplexe Regel"),
    (
        "Apply or cancel the open FIM node draft first.",
        "Übernehmen oder verwerfen Sie zuerst den offenen FIM-Knotenentwurf.",
    ),
    (
        "Select a FIM node first.",
        "Wählen Sie zuerst einen FIM-Knoten aus.",
    ),
    (
        "The root has no sibling insertion point.",
        "Neben dem Wurzelknoten können keine Geschwisterknoten eingefügt werden.",
    ),
    (
        "FIM validation worker stopped",
        "FIM-Validierungsprozess wurde beendet",
    ),
    (
        "The document changed. The FIM draft was kept; cancel and reopen it.",
        "Das Dokument wurde geändert. Der FIM-Entwurf wurde beibehalten; brechen Sie die Bearbeitung ab und öffnen Sie ihn erneut.",
    ),
    ("Show in map", "In der Map anzeigen"),
    (
        "This draft stays attached to this node while you navigate the map.",
        "Dieser Entwurf bleibt diesem Knoten zugeordnet, während Sie in der Map navigieren.",
    ),
    (
        "Validating FIM changes…",
        "FIM-Änderungen werden validiert…",
    ),
    ("More schema properties…", "Weitere Schemaeigenschaften…"),
    ("Delete FIM node…", "FIM-Knoten löschen…"),
    ("Discard FIM draft?", "FIM-Entwurf verwerfen?"),
    (
        "Only the unapplied node draft is discarded. The document is unchanged.",
        "Nur der noch nicht übernommene Knotenentwurf wird verworfen. Das Dokument bleibt unverändert.",
    ),
    ("Keep editing", "Weiter bearbeiten"),
    ("Discard draft", "Entwurf verwerfen"),
    ("Delete FIM node?", "FIM-Knoten löschen?"),
    ("Delete FIM node", "FIM-Knoten löschen"),
    (
        "Delete this occurrence and its contained nodes? Shared parent usages are affected. Dangling references and invalid schemas block deletion.",
        "Dieses Vorkommen und seine enthaltenen Knoten löschen? Gemeinsam genutzte übergeordnete Strukturen sind betroffen. Verwaiste Referenzen und ungültige Schemas verhindern das Löschen.",
    ),
    (
        "Confirm the shared-definition impact or make this occurrence independent.",
        "Bestätigen Sie die Auswirkungen auf die gemeinsam genutzte Definition oder machen Sie dieses Vorkommen unabhängig.",
    ),
    (
        "Install the official schemas before applying FIM changes.",
        "Installieren Sie die offiziellen Schemas, bevor Sie FIM-Änderungen übernehmen.",
    ),
    ("Node type", "Knotentyp"),
    (
        "XDF3 code lists use canonical references, not FIM registry IDs.",
        "XDF3-Codelisten verwenden kanonische Referenzen statt FIM-Register-IDs.",
    ),
    ("Assigned FIM ID", "Zugewiesene FIM-ID"),
    ("Definition", "Definition"),
    ("Input label", "Eingabebezeichnung"),
    ("Output label", "Ausgabebezeichnung"),
    ("Input help text", "Eingabehilfetext"),
    ("Output help text", "Ausgabehilfetext"),
    ("Structure element type", "Strukturelementtyp"),
    ("Field type", "Feldart"),
    ("Data type", "Datentyp"),
    ("Precision", "Präzisierung"),
    ("Minimum length", "Minimale Länge"),
    ("Maximum length", "Maximale Länge"),
    ("Minimum value", "Minimaler Wert"),
    ("Maximum value", "Maximaler Wert"),
    ("Pattern", "Muster"),
    ("Rule type", "Regelart"),
    ("Rule text", "Regeltext"),
    (
        "Rule script (not executed)",
        "Regelskript (wird nicht ausgeführt)",
    ),
    (
        "Only properties defined for this node type are changed.",
        "Nur die für diesen Knotentyp definierten Eigenschaften werden geändert.",
    ),
    ("BOB number range (60)", "BOB-Nummernkreis (60)"),
    (
        "Reuse evidence comes from IDs and preserved relations; it does not prove registry provenance.",
        "Hinweise auf Wiederverwendung stammen aus IDs und erhaltenen Beziehungen; sie belegen keine Herkunft aus einem Register.",
    ),
    (
        "Update all shared usages",
        "Alle gemeinsamen Verwendungen aktualisieren",
    ),
    (
        "Make this occurrence independent",
        "Dieses Vorkommen unabhängig machen",
    ),
    (
        "A new assigned ID is required. Nested reuse and provenance are preserved.",
        "Eine neue zugewiesene ID ist erforderlich. Verschachtelte Wiederverwendung und Herkunft bleiben erhalten.",
    ),
    ("Parent", "Übergeordneter Knoten"),
    ("Insertion position", "Einfügeposition"),
    (
        "Reuse existing definition",
        "Vorhandene Definition wiederverwenden",
    ),
    (
        "XDF3 groups require an initial child. Choose an existing field or group.",
        "XDF3-Gruppen benötigen einen ersten Unterknoten. Wählen Sie ein vorhandenes Feld oder eine Gruppe aus.",
    ),
    ("Initial child", "Erster Unterknoten"),
    ("Multiplicity (Min:Max)", "Multiplizität (Min:Max)"),
    ("Usage justification", "Verwendungsbegründung"),
    (
        "Multiplicity and usage justification belong to the incoming structure, not the definition.",
        "Multiplizität und Verwendungsbegründung gehören zur eingehenden Struktur, nicht zur Definition.",
    ),
    ("Associated rules", "Zugeordnete Regeln"),
    (
        "No rule definitions in this document.",
        "Dieses Dokument enthält keine Regeldefinitionen.",
    ),
    ("Code list reference", "Codelistenreferenz"),
    ("Canonical identification", "Kanonische Identifikation"),
    ("Canonical version URI", "Kanonische Versions-URI"),
    ("Code column", "Codespalte"),
    ("Name column", "Namensspalte"),
    ("Help column", "Hilfespalte"),
    (
        "External code lists are referenced, never downloaded automatically.",
        "Externe Codelisten werden referenziert und niemals automatisch heruntergeladen.",
    ),
    ("More schema properties", "Weitere Schemaeigenschaften"),
    (
        "Advanced source editing. Changes are staged with the node and validated before Apply.",
        "Erweiterte Quelltextbearbeitung. Änderungen werden mit dem Knoten vorgemerkt und vor dem Übernehmen validiert.",
    ),
    (
        "Use as insertion parent",
        "Als übergeordneten Einfügeknoten verwenden",
    ),
    ("Add schema property", "Schemaeigenschaft hinzufügen"),
    ("XML element name", "XML-Elementname"),
    ("Namespace", "Namensraum"),
    ("Stage property", "Eigenschaft vormerken"),
    ("Stage attribute", "Attribut vormerken"),
    (
        "Edit the canonical reference on the Rules and code lists page.",
        "Bearbeiten Sie die kanonische Referenz auf der Seite „Regeln und Codelisten“.",
    ),
    ("Message metadata", "Nachrichtenmetadaten"),
    (
        "Use More schema properties to edit message metadata without expanding the node tree.",
        "Verwenden Sie „Weitere Schemaeigenschaften“, um Nachrichtenmetadaten zu bearbeiten, ohne den Knotenbaum aufzuklappen.",
    ),
    ("Default value", "Standardwert"),
    ("Prefill", "Vorbefüllung"),
    ("Optional", "Optional"),
    ("Required", "Erforderlich"),
    ("Maximum file size (bytes)", "Maximale Dateigröße (Bytes)"),
    ("Allowed media types", "Zulässige Medientypen"),
    ("Code list version", "Codelistenversion"),
    (
        "Create and edit more properties…",
        "Erstellen und weitere Eigenschaften bearbeiten…",
    ),
    (
        "Reusing a definition preserves its metadata. Edit it after insertion.",
        "Bei der Wiederverwendung einer Definition bleiben ihre Metadaten erhalten. Bearbeiten Sie sie nach dem Einfügen.",
    ),
    ("Remove schema property", "Schemaeigenschaft entfernen"),
    ("Remove attribute", "Attribut entfernen"),
    ("XML fragment", "XML-Fragment"),
    ("Stage XML fragment", "XML-Fragment vormerken"),
    (
        "Insert one complete schema element with explicit namespaces. FIM definitions must use the node wizard.",
        "Fügen Sie ein vollständiges Schemaelement mit expliziten Namensräumen ein. Verwenden Sie für FIM-Definitionen den Knotenassistenten.",
    ),
    (
        "Stage or clear unfinished schema properties before applying.",
        "Merken Sie unvollständige Schemaeigenschaften vor oder leeren Sie die Eingaben, bevor Sie die Änderungen übernehmen.",
    ),
    (
        "Hierarchy changes are staged. Apply validates the whole document; Cancel keeps the original hierarchy.",
        "Hierarchieänderungen sind vorgemerkt. „Übernehmen“ validiert das gesamte Dokument; „Abbrechen“ behält die ursprüngliche Hierarchie bei.",
    ),
    ("Review hierarchy changes", "Hierarchieänderungen prüfen"),
    ("Add", "Hinzufügen"),
    ("Format", "Format"),
    ("Layers", "Ebenen"),
    ("Protection", "Schutz"),
    ("Draw", "Zeichnen"),
    ("Canvas settings", "Zeichenflächeneinstellungen"),
    ("Grid", "Raster"),
    ("Menu usage", "Menünutzung"),
    ("Count context-menu use", "Kontextmenü-Nutzung zählen"),
    (
        "Optional. Counts stay on this device, are never uploaded and never reorder menus.",
        "Optional. Die Zählungen bleiben auf diesem Gerät, werden nie hochgeladen und ändern nie die Menüreihenfolge.",
    ),
    ("Show counts", "Zählungen anzeigen"),
    ("Hide counts", "Zählungen ausblenden"),
    ("Clear counts", "Zählungen löschen"),
    (
        "Clear all menu usage counts?",
        "Alle Menünutzungszählungen löschen?",
    ),
    ("No counts recorded yet.", "Noch keine Zählungen erfasst."),
    (
        "Could not update menu usage counts",
        "Menünutzungszählungen konnten nicht aktualisiert werden",
    ),
    (
        "Could not read menu usage counts",
        "Menünutzungszählungen konnten nicht gelesen werden",
    ),
    (
        "Could not clear menu usage counts",
        "Menünutzungszählungen konnten nicht gelöscht werden",
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
    fn fim_wizard_dynamic_labels_are_in_every_catalog() {
        for key in [
            "General",
            "Field constraints",
            "Reuse and structure",
            "Rules and code lists",
            "Legal basis",
            "Harmonized",
            "Abstract",
            "Choice group",
            "Input",
            "Static",
            "Selection",
            "Hidden",
            "Locked",
            "Text",
            "Boolean",
            "Date",
            "Decimal number",
            "Integer",
            "Currency",
            "File",
            "Object",
            "Date and time",
            "Time",
            "String.Latin+",
            "Validation",
            "Multiplicity",
            "Calculation",
            "Complex rule",
            "Add child",
            "Insert after",
            "Edit FIM node",
            "Add FIM child",
            "Insert FIM node before",
            "Insert FIM node after",
            "Create FIM node",
            "Default value",
            "Prefill",
            "None",
            "Optional",
            "Required",
            "Maximum file size (bytes)",
            "Allowed media types",
            "Code list version",
            "Message metadata",
        ] {
            assert!(german().contains_key(key), "Missing German FIM key: {key}");
            assert!(additional().contains_key(key), "Missing FIM key: {key}");
            assert!(
                brazilian().contains_key(key),
                "Missing Brazilian FIM key: {key}"
            );
        }
        assert_eq!(
            t_in(Lang::De, "Multiplicity (Min:Max)"),
            "Multiplizität (Min:Max)"
        );
    }

    #[test]
    fn formatted_note_export_errors_are_translated_in_every_catalog() {
        for language in TRANSLATED {
            assert_ne!(t_in(language, "Export note source"), "Export note source");
            for key in [
                crate::notes::WORD_LIMIT,
                crate::notes::WORD_UNSUPPORTED,
                crate::notes::FREEMIND_LIMIT,
                crate::notes::FREEMIND_UNSUPPORTED,
                crate::notes::EXCEL_LIMIT,
                crate::notes::EXCEL_UNSUPPORTED,
                crate::notes::EXPORT_SOURCE_WARNING,
                crate::notes::EXPORT_SOURCE_REQUIRED,
            ] {
                assert_ne!(t_in(language, key), key, "{language:?}: {key}");
                assert!(t_in(language, key).contains("Thinoda"));
            }
        }
    }

    #[test]
    fn document_size_limit_feedback_is_translated_in_every_catalog() {
        let key = crate::file_ops::DOCUMENT_TOO_LARGE;
        for language in TRANSLATED {
            assert_ne!(t_in(language, key), key, "{language:?}: {key}");
        }
    }

    #[test]
    fn raw_recovery_format_guidance_is_translated_in_every_catalog() {
        let key = "Raw downloads contain a browser storage record (UTF-16LE code units, no BOM), not a .thinoda document. The original stays in browser storage.";
        for language in TRANSLATED {
            let text = t_in(language, key);
            assert_ne!(text, key, "{language:?}: {key}");
            for token in ["UTF-16LE", "BOM", ".thinoda"] {
                assert!(text.contains(token), "{language:?}: {token}");
            }
        }
    }

    #[test]
    fn recovery_unavailable_warning_is_translated_in_every_catalog() {
        let key = "Crash recovery is unavailable. Save or download your work.";
        for language in TRANSLATED {
            assert_ne!(t_in(language, key), key, "{language:?}: {key}");
        }
    }

    #[test]
    fn recovery_cleanup_warning_is_translated_in_every_catalog() {
        let key = crate::app::RECOVERY_CLEANUP_WARNING;
        for language in TRANSLATED {
            assert_ne!(t_in(language, key), key, "{language:?}: {key}");
        }
    }

    #[test]
    fn new_frame_board_templates_use_localized_starter_content() {
        let affinity_labels = [
            "Affinity Diagram",
            "Group related ideas, then name each theme",
            "An idea or observation",
            "Ideas to sort",
            "Theme 1",
            "Theme 2",
            "Theme 3",
        ];
        let matrix_labels = [
            "Do: urgent and important",
            "Schedule: important, not urgent",
            "Delegate: urgent, not important",
            "Eliminate: neither urgent nor important",
        ];
        for language in TRANSLATED {
            for key in affinity_labels
                .iter()
                .copied()
                .chain(["Eisenhower Matrix"])
                .chain(matrix_labels)
            {
                assert_ne!(t_in(language, key), key, "{language:?}: {key}");
            }
            let affinity =
                thinoda_core::templates::instantiate_localized("affinity-diagram", &|key| {
                    t_in(language, key).to_owned()
                });
            let sheet = affinity.active();
            assert_eq!(sheet.zones.len(), affinity_labels.len() - 3);
            assert!(
                sheet
                    .content_topics()
                    .any(|topic| { topic.title == t_in(language, "An idea or observation") })
            );
            assert!(sheet.content_topics().any(|topic| {
                topic.title == t_in(language, "Group related ideas, then name each theme")
            }));
            for (zone, key) in sheet.zones.iter().zip(&affinity_labels[3..]) {
                assert_eq!(zone.name, t_in(language, key));
            }
            let matrix =
                thinoda_core::templates::instantiate_localized("eisenhower-matrix", &|key| {
                    t_in(language, key).to_owned()
                });
            assert_eq!(matrix.active().zones.len(), matrix_labels.len());
            for (zone, key) in matrix.active().zones.iter().zip(matrix_labels) {
                assert_eq!(zone.name, t_in(language, key));
            }
            affinity.validate().unwrap();
            matrix.validate().unwrap();
        }
    }

    #[test]
    fn meeting_board_uses_localized_content_in_every_language() {
        let sections = [
            ("Agenda", "Topic and timebox"),
            ("Notes", "Key points and questions"),
            ("Decisions", "Decision and reason"),
            ("Actions", "Action, owner and due date"),
        ];
        for language in Lang::ALL {
            let wb = thinoda_core::templates::instantiate_localized("meeting-board", &|key| {
                t_in(language, key).to_owned()
            });
            let sheet = wb.active();
            assert_eq!(sheet.title, t_in(language, "Meeting Board"));
            assert_eq!(sheet.zones.len(), sections.len());
            assert_eq!(sheet.content_topics().count(), sections.len() + 1);
            assert!(
                sheet
                    .content_topics()
                    .any(|topic| { topic.title == t_in(language, "Meeting: purpose and date") })
            );
            for (zone, (name, prompt)) in sheet.zones.iter().zip(sections) {
                assert_eq!(zone.name, t_in(language, name));
                assert!(zone.keep_empty && zone.roots.is_empty());
                assert!(
                    sheet
                        .content_topics()
                        .any(|topic| topic.title == t_in(language, prompt))
                );
            }
            wb.validate().unwrap();
        }
        assert_eq!(t_in(Lang::De, "Meeting Board"), "Besprechungsboard");
        assert_eq!(t_in(Lang::PtBr, "Agenda"), "Pauta");
    }

    #[test]
    fn swimlane_uses_localized_content_in_every_language() {
        use thinoda_core::board::CanvasObjectKind;

        let lane_names = ["Role 1", "Role 2", "Role 3"];
        let shape_names = ["Start", "Step", "Step", "Action", "End"];
        for language in Lang::ALL {
            let wb = thinoda_core::templates::instantiate_localized("swimlane", &|key| {
                t_in(language, key).to_owned()
            });
            let sheet = wb.active();
            assert_eq!(sheet.title, t_in(language, "Swimlane"));
            assert_eq!(sheet.zones.len(), lane_names.len());
            for (zone, name) in sheet.zones.iter().zip(lane_names) {
                assert_eq!(zone.name, t_in(language, name));
                assert!(zone.keep_empty && zone.roots.is_empty());
            }
            assert_eq!(sheet.content_topics().count(), 1);
            assert_eq!(
                sheet.content_topics().next().unwrap().title,
                t_in(language, "Name roles and map the handoffs")
            );
            let labels: Vec<_> = sheet
                .canvas
                .objects
                .iter()
                .filter_map(|object| match &object.kind {
                    CanvasObjectKind::Shape { label, .. } => Some(label.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                labels,
                shape_names.map(|name| t_in(language, name)),
                "{language:?}"
            );
            wb.validate().unwrap();
        }
        for language in TRANSLATED {
            for key in [
                "Swimlane",
                "Name roles and map the handoffs",
                "Role 1",
                "Role 2",
                "Role 3",
                "Swimlane shows handoffs across role lanes. Lanes are visual guides; moving a shape does not assign a role or automate a workflow.",
            ] {
                assert_ne!(t_in(language, key), key, "{language:?}: {key}");
            }
        }
    }

    #[test]
    fn empathy_map_uses_localized_sections_and_prompts_in_every_language() {
        let sections = [
            ("Says", "What do they say?"),
            ("Thinks", "What might they think?"),
            ("Does", "What do they do?"),
            ("Feels", "How might they feel?"),
        ];
        let heading = "Empathy map: whose perspective?";
        for language in Lang::ALL {
            let wb = thinoda_core::templates::instantiate_localized("empathy-map", &|key| {
                t_in(language, key).to_owned()
            });
            let sheet = wb.active();
            assert_eq!(sheet.title, t_in(language, "Empathy Map"));
            assert_eq!(sheet.zones.len(), sections.len());
            assert_eq!(sheet.content_topics().count(), sections.len() + 1);
            assert!(
                sheet
                    .content_topics()
                    .any(|topic| topic.title == t_in(language, heading))
            );
            for (zone, (name, prompt)) in sheet.zones.iter().zip(sections) {
                assert_eq!(zone.name, t_in(language, name));
                assert!(zone.keep_empty && zone.roots.is_empty());
                assert!(
                    sheet
                        .content_topics()
                        .any(|topic| topic.title == t_in(language, prompt))
                );
            }
            wb.validate().unwrap();
        }
        for language in TRANSLATED {
            for key in ["Empathy Map", heading]
                .into_iter()
                .chain(sections.into_iter().flat_map(|(name, prompt)| [name, prompt]))
                .chain(["Empathy Map has four visual sections for what someone says, thinks, does and feels. Moving a note does not assign a category."])
            {
                assert_ne!(t_in(language, key), key, "{language:?}: {key}");
            }
        }
    }

    #[test]
    fn storyboard_localizes_scene_cards_and_keeps_them_inside_frames() {
        use thinoda_core::{ApproxMeasure, CardKind, layout_sheet};

        let scenes = [
            ("Scene 1", "Scene 1 image", "Scene 1 action"),
            ("Scene 2", "Scene 2 image", "Scene 2 action"),
            ("Scene 3", "Scene 3 image", "Scene 3 action"),
        ];
        let instructions = "Select an image card, then choose Local Image";
        for language in Lang::ALL {
            let wb = thinoda_core::templates::instantiate_localized("storyboard", &|key| {
                t_in(language, key).to_owned()
            });
            let sheet = wb.active();
            assert_eq!(sheet.title, t_in(language, "Storyboard"));
            assert!(sheet.resources.is_empty());
            assert_eq!(sheet.zones.len(), scenes.len());
            assert_eq!(sheet.content_topics().count(), 7);
            assert!(
                sheet
                    .content_topics()
                    .any(|topic| topic.title == t_in(language, instructions))
            );
            let layout = layout_sheet(sheet, &ApproxMeasure, false);
            for (zone, (name, image, action)) in sheet.zones.iter().zip(scenes) {
                assert_eq!(zone.name, t_in(language, name));
                assert!(zone.keep_empty && zone.roots.is_empty());
                for (title, kind) in [(image, CardKind::Image), (action, CardKind::Sticky)] {
                    let topic = sheet
                        .content_topics()
                        .find(|topic| topic.title == t_in(language, title))
                        .unwrap();
                    assert_eq!(topic.card.unwrap().kind, kind);
                    assert!(topic.image.is_none());
                    let rect = layout.topics[&topic.id].rect;
                    assert!(
                        zone.frame.contains(rect.min) && zone.frame.contains(rect.max()),
                        "{language:?}: {title}"
                    );
                }
            }
            wb.validate().unwrap();
        }
    }

    #[test]
    fn moodboard_localizes_independent_nonoverlapping_cards() {
        use thinoda_core::{ApproxMeasure, CardKind, layout_sheet};

        let images = [
            "Image 1", "Image 2", "Image 3", "Image 4", "Image 5", "Image 6",
        ];
        let help = "Moodboard has six independent image cards and an ideas note. Select a card and choose Local Image to add a picture; cards do not arrange themselves automatically.";
        for language in Lang::ALL {
            let wb = thinoda_core::templates::instantiate_localized("moodboard", &|key| {
                t_in(language, key).to_owned()
            });
            let sheet = wb.active();
            assert_eq!(sheet.title, t_in(language, "Moodboard"));
            assert_eq!(sheet.content_topics().count(), 8);
            assert!(sheet.zones.is_empty());
            assert!(sheet.resources.is_empty());
            assert!(sheet.content_topics().any(|topic| {
                topic.title == t_in(language, "Ideas")
                    && topic.card.is_some_and(|card| card.kind == CardKind::Sticky)
            }));
            let layout = layout_sheet(sheet, &ApproxMeasure, false);
            let mut rects = Vec::new();
            for key in images {
                let topic = sheet
                    .content_topics()
                    .find(|topic| topic.title == t_in(language, key))
                    .unwrap_or_else(|| panic!("{language:?}: {key}"));
                assert_eq!(topic.card.unwrap().kind, CardKind::Image);
                assert!(topic.image.is_none());
                let rect = layout.topics[&topic.id].rect;
                assert!(
                    rects.iter().all(|other| !rect.intersects(other)),
                    "{language:?}: {key}"
                );
                rects.push(rect);
            }
            wb.validate().unwrap();
        }
        for language in TRANSLATED {
            assert_ne!(t_in(language, "Moodboard"), "Moodboard", "{language:?}");
            assert_ne!(t_in(language, help), help, "{language:?}");
        }
    }

    #[test]
    fn user_journey_map_localizes_all_stages_rows_and_prompts() {
        use thinoda_core::{ApproxMeasure, CardKind, layout_sheet};

        let rows = [
            ("User actions", "What do they do?"),
            ("Feelings", "How might they feel?"),
            ("Opportunities", "What could be improved?"),
        ];
        let stages = ["Stage 1", "Stage 2", "Stage 3"];
        for language in Lang::ALL {
            let wb = thinoda_core::templates::instantiate_localized("user-journey-map", &|key| {
                t_in(language, key).to_owned()
            });
            let sheet = wb.active();
            assert_eq!(sheet.title, t_in(language, "User Journey Map"));
            assert_eq!(sheet.content_topics().count(), 13);
            assert_eq!(sheet.zones.len(), stages.len());
            assert!(sheet.content_topics().any(|topic| {
                topic.title == t_in(language, "Journey: person and scenario")
                    && topic.card.is_some_and(|card| card.kind == CardKind::Text)
            }));
            let layout = layout_sheet(sheet, &ApproxMeasure, false);
            for (zone, stage) in sheet.zones.iter().zip(stages) {
                assert_eq!(zone.name, t_in(language, stage));
                assert!(zone.keep_empty && zone.roots.is_empty());
            }
            for (label, prompt) in rows {
                assert!(
                    sheet.content_topics().any(|topic| {
                        topic.title == t_in(language, label)
                            && topic.card.is_some_and(|card| card.kind == CardKind::Text)
                    }),
                    "{language:?}: {label}"
                );
                let prompts: Vec<_> = sheet
                    .content_topics()
                    .filter(|topic| topic.title == t_in(language, prompt))
                    .collect();
                assert_eq!(prompts.len(), stages.len(), "{language:?}: {prompt}");
                for (zone, topic) in sheet.zones.iter().zip(prompts) {
                    assert_eq!(topic.card.unwrap().kind, CardKind::Sticky);
                    let rect = layout.topics[&topic.id].rect;
                    assert!(
                        zone.frame.contains(rect.min) && zone.frame.contains(rect.max()),
                        "{language:?}: {prompt}"
                    );
                }
            }
            wb.validate().unwrap();
        }
        for language in TRANSLATED {
            for key in [
                "User Journey Map",
                "Journey: person and scenario",
                "Stage 1",
                "Stage 2",
                "Stage 3",
                "User actions",
                "Feelings",
                "What could be improved?",
                "User Journey Map has three visual stages for actions, feelings and opportunities. Frames are visual guides: moving a frame does not move notes, and moving a note does not assign a stage.",
            ] {
                assert_ne!(t_in(language, key), key, "{language:?}: {key}");
            }
        }
    }

    #[test]
    fn venn_diagram_uses_localized_labels_and_descriptions_in_every_language() {
        for language in Lang::ALL {
            let wb = thinoda_core::templates::instantiate_localized("venn-diagram", &|key| {
                t_in(language, key).to_owned()
            });
            let sheet = wb.active();
            assert_eq!(sheet.title, t_in(language, "Venn Diagram"));
            for key in ["Compare two sets", "Set A", "Set B", "Shared"] {
                assert!(
                    sheet
                        .content_topics()
                        .any(|topic| topic.title == t_in(language, key)),
                    "{language:?}: {key}"
                );
            }
            for (circle, key) in sheet.canvas.objects.iter().zip(["Set A", "Set B"]) {
                assert_eq!(circle.alt_text, t_in(language, key));
                assert!(circle.label().is_empty());
            }
            wb.validate().unwrap();
        }
        for language in TRANSLATED {
            for key in [
                "Venn Diagram",
                "Compare two sets",
                "Set A",
                "Set B",
                "Shared",
                "Venn Diagram has overlapping circles and independent labels. Moving a circle does not move labels or assign set membership.",
            ] {
                assert_ne!(t_in(language, key), key, "{language:?}: {key}");
            }
        }
    }

    #[test]
    fn freeform_board_labels_exist_in_every_catalog() {
        let shape_names = thinoda_core::ShapeKind::ALL
            .into_iter()
            .map(crate::board_ui::shape_name);
        let command_names = crate::app::ThinodaApp::board_commands()
            .into_iter()
            .map(|(_, label)| label);
        let other_labels = [
            "Board",
            "Replaces topic titles, board labels and descriptions.",
            "Zoom to selection",
            "Shape",
            "Shape library",
            "Search shapes",
            "Connector",
            "Ink stroke",
            "Frame",
            "Selected",
            "Locked",
            "From",
            "To",
            "Free point",
            "Resize board item",
            "Rotate board item",
            "Add connected shape above",
            "Add connected shape to the right",
            "Add connected shape below",
            "Add connected shape to the left",
            "Click adds a connected copy; Ctrl+click adds only a copy; Shift+click adds only a connector. Drag to connect to another item; Ctrl+drag drops a connected copy. Use Command on macOS.",
            "Board object label",
            "Board minimap. Click or drag to pan; arrow keys move the view; Shift makes larger steps.",
            "Click or drag to pan. Arrow keys move the view; Shift makes larger steps.",
            "The current view is outside the minimap.",
            "Select one board object to edit its properties.",
            "Description",
            "Opacity",
            "Fill",
            "Stroke",
            "Connector route",
            "Connector color",
            "Connector width",
            "Straight",
            "Elbow",
            "Curve",
            "Arrow at start",
            "Arrow at end",
            "Card",
            "Width",
            "Height",
            "Rotation",
            "Custom color",
            "Image card",
            "Pen",
            "Highlighter",
            "Object eraser",
            "Pixel eraser",
            "Lasso",
            "Stroke type",
            "Ink color",
            "Ink width",
            "Eraser radius",
            "Collapse board toolbar",
            "Expand board toolbar",
        ];

        for key in shape_names.chain(command_names).chain(other_labels) {
            for language in Lang::ALL {
                let present = match language {
                    Lang::En => true,
                    Lang::De => german().contains_key(key),
                    Lang::PtBr => brazilian().contains_key(key),
                    _ => additional().contains_key(key),
                };
                assert!(present, "Missing {language:?} board translation: {key}");
                assert!(
                    !t_in(language, key).trim().is_empty(),
                    "Empty {language:?} board translation: {key}"
                );
            }
        }
    }

    #[test]
    fn pending_ink_feedback_exists_in_every_catalog() {
        let keys = [
            "Pending ink gesture",
            "Ink is not saved or included in crash recovery until it is finished.",
            "Finish the pending ink gesture before continuing. Retry it or discard it.",
            "Resolve pending ink before continuing. Retry, copy, or discard it.",
            "The pointer is still down. Release it before retrying.",
            "Some ink input was rejected. This gesture cannot be retried; copy its samples or discard it.",
            "The ink gesture could not be completed. Retry it or discard it.",
            "The ink gesture was interrupted. Retry it or discard it.",
            "Ink input details",
            "Ink settings",
            "Retry ink",
            "Discard ink",
            "Copy ink samples",
            "Dismiss ink feedback",
            "Ink samples copied",
            "Accepted samples",
            "Rejected input",
            "Sample position",
            "Pressure",
            "No pressure",
            "Simplification tolerance",
            "Additional rejected input details could not be retained.",
            "The document changed during the gesture. Copy its samples or discard it.",
            "The canvas changed during the gesture. Copy its samples or discard it.",
            "Ink could not be applied to a locked object. Copy the samples or discard the gesture.",
            "This operation would change frame clipping. Copy the samples or discard the gesture.",
            "Adjust ink simplification and retry, or copy the samples and discard the gesture.",
            "Finish the active text editor before drawing again.",
        ];
        for key in keys {
            for language in Lang::ALL {
                let present = match language {
                    Lang::En => true,
                    Lang::De => german().contains_key(key),
                    Lang::PtBr => brazilian().contains_key(key),
                    _ => additional().contains_key(key),
                };
                assert!(present, "Missing {language:?} ink translation: {key}");
                assert!(
                    !t_in(language, key).trim().is_empty(),
                    "Empty {language:?} ink translation: {key}"
                );
            }
        }
    }

    #[test]
    fn board_tool_guide_contains_localized_control_names() {
        let paragraphs: [(&str, &[&str]); 2] = [
            (
                crate::help::BOARD_INK_GUIDE,
                &[
                    "Pen",
                    "Highlighter",
                    "Ink color",
                    "Ink width",
                    "Opacity",
                    "Eraser radius",
                ],
            ),
            (
                crate::help::BOARD_TOOLS_GUIDE,
                &["Object eraser", "Pixel eraser", "Lasso", "Image card"],
            ),
        ];
        for (paragraph, labels) in paragraphs {
            assert!(
                crate::help::translation_keys().any(|key| key == paragraph),
                "Board guidance is missing from the guide: {paragraph}"
            );
            for language in Lang::ALL {
                let present = match language {
                    Lang::En => true,
                    Lang::De => german().contains_key(paragraph),
                    Lang::PtBr => brazilian().contains_key(paragraph),
                    _ => additional().contains_key(paragraph),
                };
                assert!(
                    present,
                    "Missing {language:?} guide translation: {paragraph}"
                );
                let text = t_in(language, paragraph);
                assert!(
                    !text.trim().is_empty(),
                    "Empty {language:?} guide translation: {paragraph}"
                );
                for &label in labels {
                    let translated_label = t_in(language, label);
                    assert!(
                        text.contains(translated_label),
                        "{language:?} guide omits {label}: {translated_label}"
                    );
                }
            }
        }
    }

    #[test]
    fn board_duplication_and_toolbar_fallback_guides_are_localized() {
        for key in [
            crate::help::BOARD_DUPLICATION_GUIDE,
            crate::help::BOARD_TOOLBAR_FALLBACK_GUIDE,
        ] {
            assert!(crate::help::translation_keys().any(|entry| entry == key));
            for language in TRANSLATED {
                assert_ne!(t_in(language, key), key, "{language:?}: {key}");
            }
        }
    }

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
    fn browser_language_tags_match_their_catalogs() {
        for (language, tag) in [
            (Lang::En, "en"),
            (Lang::De, "de"),
            (Lang::Es, "es"),
            (Lang::Fr, "fr"),
            (Lang::Da, "da"),
            (Lang::Nl, "nl"),
            (Lang::Pt, "pt-PT"),
            (Lang::It, "it"),
            (Lang::Pl, "pl"),
            (Lang::PtBr, "pt-BR"),
        ] {
            assert_eq!(language.language_tag(), tag);
            assert_eq!(resolve_locale(tag), language);
        }
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
        let outline_export_losses = [
            "Cards omitted",
            "Summary topic trees omitted",
            "Relationships omitted",
            "Boundaries omitted",
            "Inline card text and list formatting omitted",
            "Mind-map topics and links flattened to shapes",
            "Topic notes, labels, markers or tasks omitted",
            "Presentation slides omitted",
        ];
        for key in outline_export_losses {
            assert!(
                include_str!("pt-BR.txt")
                    .lines()
                    .filter_map(|line| line.split_once('|'))
                    .any(|(entry, text)| entry == key && !text.trim().is_empty()),
                "Missing explicit Brazilian Portuguese translation: {key}"
            );
        }
        keys.extend(outline_export_losses);
        let formatted_title_errors =
            ["Formatted card titles cannot be changed until rich-text editing is supported"];
        for key in formatted_title_errors {
            assert!(
                include_str!("pt-BR.txt")
                    .lines()
                    .filter_map(|line| line.split_once('|'))
                    .any(|(entry, text)| entry == key && !text.trim().is_empty()),
                "Missing explicit Brazilian Portuguese translation: {key}"
            );
        }
        keys.extend(formatted_title_errors);
        keys.extend(thinoda_core::templates::board_strings());
        keys.extend(crate::help::translation_keys());
        let kanban = thinoda_core::templates::instantiate_localized("kanban-board", &|s| {
            t_in(Lang::De, s).to_owned()
        });
        let sheet = kanban.active();
        assert!(sheet.root_topic().title.is_empty());
        assert_eq!(sheet.zones[0].name, "Zu erledigen");
        kanban.validate().unwrap();
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
            include_str!("updates.rs"),
            include_str!("ui.rs"),
            include_str!("app.rs"),
            include_str!("board_ui.rs"),
            include_str!("ai.rs"),
            include_str!("ai/review.rs"),
            include_str!("ai/chat.rs"),
            include_str!("ai/chat_history.rs"),
            include_str!("ai/prompts.rs"),
            include_str!("ai/semantic.rs"),
            include_str!("help.rs"),
            include_str!("labels.rs"),
            include_str!("images.rs"),
            include_str!("notes.rs"),
            include_str!("equations.rs"),
            include_str!("doc.rs"),
            include_str!("map_shot.rs"),
            include_str!("printing.rs"),
            include_str!("quick_styles.rs"),
            include_str!("theme_editor.rs"),
            include_str!("template_gallery.rs"),
            include_str!("structure_picker.rs"),
            include_str!("shortcut_browser.rs"),
            include_str!("keymap_editor.rs"),
            include_str!("planning.rs"),
            include_str!("fim.rs"),
            include_str!("fim/node_editor.rs"),
            include_str!("zones.rs"),
            include_str!("pitch.rs"),
            include_str!("pitch/pitch_editor.rs"),
            include_str!("pitch/pitch_player.rs"),
            include_str!("pitch/pitch_preview.rs"),
            include_str!("pitch/pitch_ai.rs"),
            include_str!("gantt.rs"),
            include_str!("symbols.rs"),
            include_str!("menus.rs"),
            include_str!("native_menus.rs"),
            include_str!("navigation.rs"),
            include_str!("ribbon.rs"),
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
