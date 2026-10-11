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
        "Use the Clan tool and click people on the board to add or remove them. A person can be in several clans.",
        "Mit dem Sippen-Werkzeug Personen auf dem Brett anklicken, um sie hinzuzufügen oder zu entfernen. Eine Person kann in mehreren Sippen sein.",
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
