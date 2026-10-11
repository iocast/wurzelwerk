//! Minimal i18n: the English UI text is the key, other languages are lookup tables.
//!
//! `t("Select")` returns the translation for the active language (or the key itself),
//! `tf("{} people", &[&n])` fills `{}` placeholders in order after translating.
//! To add a language: add a [`Lang`] variant and a table next to [`DE`].

use std::collections::HashMap;
use std::fmt::{self, Display};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    De,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::De];

    /// Picks the system language, English when unknown.
    pub fn detect() -> Self {
        match sys_locale::get_locale() {
            Some(l) if l.to_lowercase().starts_with("de") => Lang::De,
            _ => Lang::En,
        }
    }
}

impl Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
        })
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn get() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::De,
        _ => Lang::En,
    }
}

pub fn t(key: &'static str) -> &'static str {
    match get() {
        Lang::En => key,
        Lang::De => german().get(key).copied().unwrap_or(key),
    }
}

/// Translates `key`, then replaces each `{}` with the next argument.
pub fn tf(key: &'static str, args: &[&dyn Display]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = t(key);
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

fn german() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| DE.iter().copied().collect())
}

const DE: &[(&str, &str)] = &[
    // toolbar
    ("Select", "Auswählen"),
    ("+ Person", "+ Person"),
    ("Connect", "Verbinden"),
    ("Clan", "Sippe"),
    ("Auto-layout", "Auto-Layout"),
    ("Fit to window", "An Fenster anpassen"),
    ("Search people…", "Personen suchen…"),
    ("Settings", "Einstellungen"),
    // sidebar
    ("CLANS", "SIPPEN"),
    ("PEOPLE", "PERSONEN"),
    ("BOOKS", "BÜCHER"),
    ("+ New clan", "+ Neue Sippe"),
    ("+ New book", "+ Neues Buch"),
    ("+ page", "+ Seite"),
    ("… {} more - use search", "… {} weitere – Suche verwenden"),
    ("… {} more", "… {} weitere"),
    ("N", "N"),
    ("F", "D"),
    // inspector - empty
    ("WURZELWERK", "WURZELWERK"),
    (
        "Click a person to edit details, clans, lines and files.",
        "Person anklicken, um Details, Sippen, Linien und Dateien zu bearbeiten.",
    ),
    (
        "+ Person: click on the board to place someone.",
        "+ Person: auf das Brett klicken, um jemanden zu platzieren.",
    ),
    (
        "Connect: drag from a person (or a handle of the selected person) onto another person, then name the line.",
        "Verbinden: von einer Person (oder vom Griff der ausgewählten Person) auf eine andere Person ziehen und die Linie benennen.",
    ),
    (
        "Select: drag a line label to move it, or drag a line segment to add/move a bend.",
        "Auswählen: Linienbeschriftung ziehen, um sie zu verschieben, oder ein Linienstück ziehen, um einen Knick hinzuzufügen/zu verschieben.",
    ),
    (
        "Clan: pick a clan on the left, then click people to add or remove them.",
        "Sippe: links eine Sippe wählen, dann Personen anklicken, um sie hinzuzufügen oder zu entfernen.",
    ),
    (
        "Wheel = zoom · drag empty space / right mouse = pan · select a line then Delete to remove it, or select a person to delete them.",
        "Mausrad = Zoom · leere Fläche ziehen / rechte Maustaste = verschieben · Linie auswählen und Entf drücken, um sie zu löschen, oder eine Person auswählen, um sie zu löschen.",
    ),
    // inspector - line
    ("NAME THE LINE", "LINIE BENENNEN"),
    ("e.g. godfather of", "z. B. Taufpate von"),
    ("Create line", "Linie erstellen"),
    ("Cancel", "Abbrechen"),
    ("SELECTED LINE", "AUSGEWÄHLTE LINIE"),
    ("Missing person", "Person fehlt"),
    (
        "Drag a label to move it or drag a segment to reshape the route.",
        "Beschriftung ziehen, um sie zu verschieben, oder ein Linienstück ziehen, um den Verlauf zu ändern.",
    ),
    (
        "Selected corner: press Delete or remove it below.",
        "Ausgewählte Ecke: Entf drücken oder unten entfernen.",
    ),
    ("Remove selected corner", "Ausgewählte Ecke entfernen"),
    (
        "Click a route corner to select and remove it.",
        "Eine Ecke des Verlaufs anklicken, um sie auszuwählen und zu entfernen.",
    ),
    ("Reset to automatic route", "Auf automatischen Verlauf zurücksetzen"),
    ("Press Delete to remove this line.", "Entf drücken, um diese Linie zu löschen."),
    ("Remove line", "Linie entfernen"),
    // inspector - person
    ("Name", "Name"),
    ("born", "geboren"),
    ("died", "gestorben"),
    ("+ clan", "+ Sippe"),
    ("LINES", "LINIEN"),
    (
        "Drag a handle onto another person to add a line.",
        "Einen Griff auf eine andere Person ziehen, um eine Linie hinzuzufügen.",
    ),
    (
        "On the board, drag a line label to reposition it or drag a segment to change the route.",
        "Auf dem Brett eine Linienbeschriftung ziehen, um sie zu verschieben, oder ein Linienstück ziehen, um den Verlauf zu ändern.",
    ),
    ("Edit notes (markdown)", "Notizen bearbeiten (Markdown)"),
    ("FILES · {}", "DATEIEN · {}"),
    ("Drop files here or choose…", "Dateien hierher ziehen oder auswählen…"),
    ("Delete person", "Person löschen"),
    ("Attach files", "Dateien anhängen"),
    // inspector - clan
    ("CLAN", "SIPPE"),
    ("Clan name", "Name der Sippe"),
    ("Change colour", "Farbe ändern"),
    ("{} members", "{} Mitglieder"),
    ("Fit to clan", "An Sippe anpassen"),
    (
        "Click “Add / remove members”, then click people on the board. A person can be in several clans.",
        "„Mitglieder hinzufügen / entfernen“ klicken, dann Personen auf dem Brett anklicken. Eine Person kann in mehreren Sippen sein.",
    ),
    ("Delete clan", "Sippe löschen"),
    // notes
    ("← Board", "← Brett"),
    ("Book name", "Name des Buchs"),
    ("Page name", "Name der Seite"),
    ("Delete page", "Seite löschen"),
    ("Notes · {}", "Notizen · {}"),
    // defaults and notices
    ("New person", "Neue Person"),
    ("Person", "Person"),
    ("Clan {}", "Sippe {}"),
    (
        "Select a clan in the sidebar, then click people to add/remove them.",
        "Eine Sippe in der Seitenleiste wählen, dann Personen anklicken, um sie hinzuzufügen/zu entfernen.",
    ),
    ("Select a person first.", "Zuerst eine Person auswählen."),
    (
        "Select a person, then drop files to attach them.",
        "Eine Person auswählen und Dateien darauf ziehen, um sie anzuhängen.",
    ),
    // status bar
    (
        "{} people · {} clans · {} lines · {}",
        "{} Personen · {} Sippen · {} Linien · {}",
    ),
    ("all changes saved", "alle Änderungen gespeichert"),
    ("saving… ({})", "speichern… ({})"),
    ("save error: {}", "Speicherfehler: {}"),
    // welcome
    ("Welcome to Wurzelwerk", "Willkommen bei Wurzelwerk"),
    (
        "Create a workspace to get started. A workspace is a folder that holds your families, clans, notes and files.",
        "Erstelle zum Start einen Arbeitsbereich. Ein Arbeitsbereich ist ein Ordner, der deine Familien, Sippen, Notizen und Dateien enthält.",
    ),
    ("Workspace location", "Ort des Arbeitsbereichs"),
    ("Choose location…", "Ort wählen…"),
    ("Create workspace", "Arbeitsbereich erstellen"),
    ("Choose workspace folder", "Ordner des Arbeitsbereichs wählen"),
    ("Please enter a location.", "Bitte einen Ort angeben."),
    ("cannot create {}: {}", "{} kann nicht erstellt werden: {}"),
    ("cannot open the database in {}: {}", "Datenbank in {} kann nicht geöffnet werden: {}"),
    // settings dialog
    ("General", "Allgemein"),
    ("Workspace", "Arbeitsbereich"),
    ("Updates", "Updates"),
    ("About", "Info"),
    ("Close", "Schliessen"),
    ("Language", "Sprache"),
    (
        "Language of the user interface.",
        "Sprache der Benutzeroberfläche.",
    ),
    ("Current workspace", "Aktueller Arbeitsbereich"),
    ("Switch workspace", "Arbeitsbereich wechseln"),
    (
        "Pick a folder to open it as workspace. A new or empty folder becomes a new workspace.",
        "Wähle einen Ordner, um ihn als Arbeitsbereich zu öffnen. Ein neuer oder leerer Ordner wird zu einem neuen Arbeitsbereich.",
    ),
    ("Browse…", "Durchsuchen…"),
    ("Open", "Öffnen"),
    ("Recent workspaces", "Zuletzt verwendet"),
    ("No other workspaces yet.", "Noch keine weiteren Arbeitsbereiche."),
    ("Check for updates on start", "Beim Start nach Updates suchen"),
    ("Check now", "Jetzt prüfen"),
    ("Checking…", "Prüfe…"),
    ("You are up to date.", "Du bist auf dem neuesten Stand."),
    ("Version {} is available.", "Version {} ist verfügbar."),
    ("Download", "Herunterladen"),
    ("Dismiss", "Ausblenden"),
    ("Update check failed: {}", "Update-Prüfung fehlgeschlagen: {}"),
    ("Update failed: {}", "Update fehlgeschlagen: {}"),
    ("Downloading version {}…", "Version {} wird heruntergeladen…"),
    ("Version {} is ready to install.", "Version {} ist bereit zur Installation."),
    ("Restart now", "Jetzt neu starten"),
    ("Current version: {}", "Aktuelle Version: {}"),
    ("Version {}", "Version {}"),
    ("Clans & families board", "Sippen- und Familienbrett"),
    ("Settings file", "Einstellungsdatei"),
    ("Settings folder", "Einstellungsordner"),
    (
        "Global settings are stored here:",
        "Die globalen Einstellungen werden hier gespeichert:",
    ),
    ("Licence", "Lizenz"),
    ("Source code", "Quellcode"),
    ("Open folder", "Ordner öffnen"),
    ("Could not save settings: {}", "Einstellungen konnten nicht gespeichert werden: {}"),
    // redesign: toolbar, board overlays
    ("Fit", "Einpassen"),
    ("Fit to window (F)", "An Fenster anpassen (F)"),
    ("Arrange everyone by generation and clan", "Alle nach Generation und Sippe anordnen"),
    ("Hide sidebar", "Seitenleiste ausblenden"),
    ("Show sidebar", "Seitenleiste einblenden"),
    ("Switch or create workspaces", "Arbeitsbereiche wechseln oder erstellen"),
    ("Search people…  Ctrl+F", "Personen suchen…  Strg+F"),
    ("Light mode", "Heller Modus"),
    ("Dark mode", "Dunkler Modus"),
    ("Zoom in (+)", "Vergrössern (+)"),
    ("Zoom out (−)", "Verkleinern (−)"),
    ("Reset to 100% (0)", "Auf 100 % zurücksetzen (0)"),
    ("Click anywhere on the board to place a person", "Irgendwo auf das Brett klicken, um eine Person zu platzieren"),
    ("Drag from one person onto another to connect them", "Von einer Person auf eine andere ziehen, um sie zu verbinden"),
    ("Pick a clan first", "Zuerst eine Sippe wählen"),
    (
        "Click people to add them to or remove them from {}",
        "Personen anklicken, um sie {} hinzuzufügen oder daraus zu entfernen",
    ),
    (
        "Click anywhere on the board to place a person in {}",
        "Irgendwo auf das Brett klicken, um eine Person in {} zu platzieren",
    ),
    ("Click on the board to place a new clan", "Auf das Brett klicken, um eine neue Sippe zu platzieren"),
    ("Add clan", "Sippe hinzufügen"),
    ("empty clan", "leere Sippe"),
    (
        "Deselect clan – new people join no clan",
        "Sippe abwählen – neue Personen kommen in keine Sippe",
    ),
    (
        "Clan: click on the board to place a clan, then use + Person inside its box. New people join the clan selected on the left; click it again for no clan.",
        "Sippe: auf das Brett klicken, um eine Sippe zu platzieren, dann mit + Person in ihr Feld klicken. Neue Personen kommen in die links gewählte Sippe; erneut anklicken für keine Sippe.",
    ),
    ("Name the new line in the panel on the right", "Die neue Linie rechts im Bereich benennen"),
    ("Start your family board", "Starte dein Familienbrett"),
    (
        "Add the first person, then connect relatives and group them into clans.",
        "Füge die erste Person hinzu, verbinde dann Verwandte und fasse sie in Sippen zusammen.",
    ),
    ("Add person", "Person hinzufügen"),
    ("New clan", "Neue Sippe"),
    ("New book", "Neues Buch"),
    ("New page", "Neue Seite"),
    ("Back to board", "Zurück zum Brett"),
    ("Saving…", "Speichern…"),
    ("Saved", "Gespeichert"),
    // redesign: sidebar
    ("No clans yet.", "Noch keine Sippen."),
    ("No people yet.", "Noch keine Personen."),
    ("Nobody matches your search.", "Niemand passt zur Suche."),
    // redesign: inspector
    ("Nothing selected", "Nichts ausgewählt"),
    (
        "Select a person, line or clan to see and edit its details here.",
        "Eine Person, Linie oder Sippe auswählen, um ihre Details hier zu sehen und zu bearbeiten.",
    ),
    ("GETTING STARTED", "ERSTE SCHRITTE"),
    ("Add people", "Personen hinzufügen"),
    ("Connect relatives", "Verwandte verbinden"),
    ("Group into clans", "In Sippen gruppieren"),
    ("Shape the lines", "Linien formen"),
    ("KEYBOARD", "TASTATUR"),
    ("Zoom in / out", "Vergrössern / verkleinern"),
    ("Search people", "Personen suchen"),
    ("Delete selection", "Auswahl löschen"),
    ("Cancel / deselect", "Abbrechen / Auswahl aufheben"),
    ("Wheel", "Mausrad"),
    ("Zoom", "Zoom"),
    ("Drag empty space", "Leere Fläche ziehen"),
    ("Pan", "Verschieben"),
    ("RELATIONSHIP", "BEZIEHUNG"),
    ("Or pick a common one:", "Oder eine häufige wählen:"),
    ("ROUTE", "VERLAUF"),
    ("DETAILS", "DETAILS"),
    ("Born", "Geboren"),
    ("Died", "Gestorben"),
    ("e.g. 1921", "z. B. 1921"),
    ("e.g. 1999", "z. B. 1999"),
    ("Dates unknown", "Daten unbekannt"),
    ("Remove from clan", "Aus Sippe entfernen"),
    ("Create a clan in the sidebar first.", "Zuerst in der Seitenleiste eine Sippe erstellen."),
    ("LINES · {}", "LINIEN · {}"),
    ("Edit line", "Linie bearbeiten"),
    ("Delete file", "Datei löschen"),
    ("Open notes", "Notizen öffnen"),
    ("Write notes", "Notizen schreiben"),
    ("Markdown, saved automatically", "Markdown, wird automatisch gespeichert"),
    (
        "Removes the person with their lines, notes and files.",
        "Entfernt die Person mit ihren Linien, Notizen und Dateien.",
    ),
    ("COLOUR", "FARBE"),
    ("Add / remove members", "Mitglieder hinzufügen / entfernen"),
    ("Done adding", "Fertig"),
    ("MEMBERS · {}", "MITGLIEDER · {}"),
    ("No members yet.", "Noch keine Mitglieder."),
    (
        "Members stay on the board; only the clan is removed.",
        "Die Mitglieder bleiben auf dem Brett; nur die Sippe wird entfernt.",
    ),
    // redesign: confirmation
    ("Delete {}?", "{} löschen?"),
    ("Delete", "Löschen"),
    (
        "Their lines, notes and attached files are removed as well. This cannot be undone.",
        "Linien, Notizen und angehängte Dateien werden ebenfalls entfernt. Dies kann nicht rückgängig gemacht werden.",
    ),
    (
        "The members stay on the board. The clan and its notes are removed. This cannot be undone.",
        "Die Mitglieder bleiben auf dem Brett. Die Sippe und ihre Notizen werden entfernt. Dies kann nicht rückgängig gemacht werden.",
    ),
    (
        "The page is removed from the book. This cannot be undone.",
        "Die Seite wird aus dem Buch entfernt. Dies kann nicht rückgängig gemacht werden.",
    ),
    ("{} people · {} clans · {} lines", "{} Personen · {} Sippen · {} Linien"),
    // redesign: welcome and settings
    ("WORKSPACE LOCATION", "ORT DES ARBEITSBEREICHS"),
    (
        "Pick an existing workspace folder to open it, or a new folder to start fresh.",
        "Wähle einen bestehenden Arbeitsbereich zum Öffnen oder einen neuen Ordner für einen Neustart.",
    ),
    ("RECENT WORKSPACES", "ZULETZT VERWENDET"),
    ("Folder path", "Ordnerpfad"),
    ("APPEARANCE", "DARSTELLUNG"),
    (
        "Choose a light or dark look, or follow the system.",
        "Helle oder dunkle Darstellung wählen oder dem System folgen.",
    ),
    ("System", "System"),
    ("Light", "Hell"),
    ("Dark", "Dunkel"),
    ("LANGUAGE", "SPRACHE"),
    ("CURRENT WORKSPACE", "AKTUELLER ARBEITSBEREICH"),
    (
        "Everything you see on the board is stored in this folder.",
        "Alles, was du auf dem Brett siehst, ist in diesem Ordner gespeichert.",
    ),
    ("SWITCH WORKSPACE", "ARBEITSBEREICH WECHSELN"),
    (
        "Open one of the workspaces you used before.",
        "Einen früher verwendeten Arbeitsbereich öffnen.",
    ),
    ("VERSION", "VERSION"),
    ("AUTOMATIC CHECK", "AUTOMATISCHE PRÜFUNG"),
    (
        "Looks for a new release when Wurzelwerk starts and downloads it in the background.",
        "Sucht beim Start von Wurzelwerk nach einer neuen Version und lädt sie im Hintergrund herunter.",
    ),
    ("SETTINGS FILE", "EINSTELLUNGSDATEI"),
    ("LICENCE", "LIZENZ"),
    ("SOURCE CODE", "QUELLCODE"),
    ("Open on GitHub", "Auf GitHub öffnen"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_match_and_keys_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (en, de) in DE {
            assert!(seen.insert(*en), "duplicate key {en}");
            assert_eq!(en.matches("{}").count(), de.matches("{}").count(), "{en}");
        }
    }

    #[test]
    fn formats_in_order() {
        set(Lang::En);
        assert_eq!(tf("Clan {}", &[&3]), "Clan 3");
        set(Lang::De);
        assert_eq!(tf("Clan {}", &[&3]), "Sippe 3");
        set(Lang::En);
    }
}
