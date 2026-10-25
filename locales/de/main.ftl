# German user interface text.

## toolbar

tool-select = Auswählen
tool-connect = Verbinden
tool-auto-layout = Auto-Layout
tool-fit-to-window = An Fenster anpassen
tool-settings = Einstellungen
tool-fit = Einpassen
tool-fit-to-window-tooltip = An Fenster anpassen (F)
tool-auto-layout-tooltip = Alle nach Generation und Sippe anordnen
tool-hide-sidebar = Seitenleiste ausblenden
tool-show-sidebar = Seitenleiste einblenden
tool-switch-workspaces = Arbeitsbereiche wechseln oder erstellen
tool-search-placeholder = Personen suchen…  Strg+F
tool-light-mode = Heller Modus
tool-dark-mode = Dunkler Modus
tool-timeline = Zeitleiste
tool-timeline-tooltip = Das Brett so zeigen, wie es in einem gewählten Jahr aussah

## board overlays

zoom-in-tooltip = Vergrössern (+)
zoom-out-tooltip = Verkleinern (−)
zoom-reset-tooltip = Auf 100 % zurücksetzen (0)
hint-add-person = Irgendwo auf das Brett klicken, um eine Person zu platzieren
hint-connect = Von einer Person auf eine andere ziehen, um sie zu verbinden
hint-pick-clan = Zuerst eine Sippe wählen
hint-members = Personen anklicken, um sie { $clan } hinzuzufügen oder daraus zu entfernen
hint-add-person-in-clan = Irgendwo auf das Brett klicken, um eine Person in { $clan } zu platzieren
hint-add-clan = Auf das Brett klicken, um eine neue Sippe zu platzieren
hint-name-line = Die neue Linie rechts im Bereich benennen
empty-board-title = Starte dein Familienbrett
empty-board-body = Füge die erste Person hinzu, verbinde dann Verwandte und fasse sie in Sippen zusammen.
empty-board-add-person = Person hinzufügen
timeline-play = Abspielen
timeline-pause = Anhalten
timeline-close = Zeitleiste schliessen
timeline-living = { $count ->
        [one] { $count } lebt
       *[other] { $count } leben
    }

## sidebar

sidebar-clans = SIPPEN
sidebar-people = PERSONEN
sidebar-books = BÜCHER
sidebar-more-use-search = … { $count } weitere – Suche verwenden
list-more = … { $count } weitere
sidebar-add-clan = Sippe hinzufügen
sidebar-empty-clan = leere Sippe
sidebar-deselect-clan = Sippe abwählen – neue Personen kommen in keine Sippe
sidebar-new-clan = Neue Sippe
sidebar-new-book = Neues Buch
sidebar-new-page = Neue Seite
sidebar-no-clans = Noch keine Sippen.
sidebar-no-people = Noch keine Personen.
sidebar-no-match = Niemand passt zur Suche.

## inspector - empty

inspector-nothing-selected = Nichts ausgewählt
inspector-nothing-selected-body = Eine Person, Linie oder Sippe auswählen, um ihre Details hier zu sehen und zu bearbeiten.
inspector-getting-started = ERSTE SCHRITTE
help-add-people = Personen hinzufügen
help-add-people-body = + Person: auf das Brett klicken, um jemanden zu platzieren.
help-connect = Verwandte verbinden
help-connect-body = Verbinden: von einer Person (oder vom Griff der ausgewählten Person) auf eine andere Person ziehen und die Linie benennen.
help-clans = In Sippen gruppieren
help-clans-body = Sippe: auf das Brett klicken, um eine Sippe zu platzieren, dann mit + Person in ihr Feld klicken. Neue Personen kommen in die links gewählte Sippe; erneut anklicken für keine Sippe.
help-lines = Linien formen
help-lines-body = Auswählen: Linienbeschriftung ziehen, um sie zu verschieben, oder ein Linienstück ziehen, um einen Knick hinzuzufügen/zu verschieben.
inspector-keyboard = TASTATUR
key-zoom = Vergrössern / verkleinern
key-search = Personen suchen
key-delete = Auswahl löschen
key-cancel = Abbrechen / Auswahl aufheben
key-wheel = Mausrad
key-wheel-zoom = Zoom
key-drag = Leere Fläche ziehen
key-drag-pan = Verschieben

## inspector - line

line-name-title = LINIE BENENNEN
line-relationship = BEZIEHUNG
line-name-placeholder = z. B. Taufpate von
line-common = Oder eine häufige wählen:
line-create = Linie erstellen
cancel = Abbrechen
line-selected = AUSGEWÄHLTE LINIE
line-missing-person = Person fehlt
line-route-help = Beschriftung ziehen, um sie zu verschieben, oder ein Linienstück ziehen, um den Verlauf zu ändern.
line-corner-selected = Ausgewählte Ecke: Entf drücken oder unten entfernen.
line-remove-corner = Ausgewählte Ecke entfernen
line-corner-help = Eine Ecke des Verlaufs anklicken, um sie auszuwählen und zu entfernen.
line-reset-route = Auf automatischen Verlauf zurücksetzen
line-route = VERLAUF
line-years = ZEITRAUM
line-start-year = Von Jahr
line-end-year = Bis Jahr
line-year-placeholder = optional
line-year-not-a-year = Ein Jahr wie 1921 eingeben oder das Feld leer lassen.
line-year-too-early = Muss { $year } oder später sein, solange beide Personen lebten.
line-year-too-late = Muss { $year } oder früher sein, solange beide Personen lebten.
line-year-end-before-start = Das Endjahr darf nicht vor dem Startjahr liegen.
line-delete-help = Entf drücken, um diese Linie zu löschen.
line-remove = Linie entfernen

## line presets

link-married = verheiratet
link-parents-of = Eltern von
link-father-of = Vater von
link-mother-of = Mutter von
link-siblings = Geschwister
link-partner = Partner
link-godfather-of = Taufpate von
link-godmother-of = Taufpatin von

## inspector - person

person-details = DETAILS
person-name = Name
person-born = Geboren
person-died = Gestorben
person-born-placeholder = z. B. 1921
person-died-placeholder = z. B. 1999
person-dates-unknown = Daten unbekannt
person-add-clan = + Sippe
person-remove-from-clan = Aus Sippe entfernen
person-no-clans = Zuerst in der Seitenleiste eine Sippe erstellen.
person-lines = LINIEN · { $count }
person-edit-line = Linie bearbeiten
person-lines-help = Einen Griff auf eine andere Person ziehen, um eine Linie hinzuzufügen.
person-lines-route-help = Auf dem Brett eine Linienbeschriftung ziehen, um sie zu verschieben, oder ein Linienstück ziehen, um den Verlauf zu ändern.
person-files = DATEIEN · { $count }
person-delete-file = Datei löschen
person-drop-files = Dateien hierher ziehen oder auswählen…
person-attach-files = Dateien anhängen
person-open-notes = Notizen öffnen
person-write-notes = Notizen schreiben
person-notes-hint = Markdown, wird automatisch gespeichert
person-delete-help = Entfernt die Person mit ihren Linien, Notizen und Dateien.
person-delete = Person löschen

## inspector - clan

clan = Sippe
clan-name = Name der Sippe
clan-member-count = { $count ->
        [one] { $count } Mitglied
       *[other] { $count } Mitglieder
    }
clan-colour = FARBE
clan-edit-members = Mitglieder hinzufügen / entfernen
clan-done-adding = Fertig
clan-members = MITGLIEDER · { $count }
clan-no-members = Noch keine Mitglieder.
clan-fit = An Sippe anpassen
clan-members-help = „Mitglieder hinzufügen / entfernen“ klicken, dann Personen auf dem Brett anklicken. Eine Person kann in mehreren Sippen sein.
clan-delete-help = Die Mitglieder bleiben auf dem Brett; nur die Sippe wird entfernt.
clan-delete = Sippe löschen

## notes

notes-back = Zurück zum Brett
notes-book-name = Name des Buchs
notes-page-name = Name der Seite
notes-delete-page = Seite löschen
notes-title = Notizen · { $title }
notes-saving = Speichern…
notes-saved = Gespeichert

## defaults and notices

default-person-name = Neue Person
default-person = Person
default-clan-name = Sippe { $number }
notice-select-clan = Eine Sippe in der Seitenleiste wählen, dann Personen anklicken, um sie hinzuzufügen/zu entfernen.
notice-select-person = Zuerst eine Person auswählen.
notice-drop-files = Eine Person auswählen und Dateien darauf ziehen, um sie anzuhängen.
notice-no-dates = Für die Zeitleiste bei Personen Geburts- oder Todesjahre eintragen.

## confirmation

confirm-delete-title = { $name } löschen?
confirm-delete = Löschen
confirm-delete-person = Linien, Notizen und angehängte Dateien werden ebenfalls entfernt. Dies kann nicht rückgängig gemacht werden.
confirm-delete-clan = Die Mitglieder bleiben auf dem Brett. Die Sippe und ihre Notizen werden entfernt. Dies kann nicht rückgängig gemacht werden.
confirm-delete-page = Die Seite wird aus dem Buch entfernt. Dies kann nicht rückgängig gemacht werden.

## status bar

status-counts = { $people ->
        [one] { $people } Person
       *[other] { $people } Personen
    } · { $clans ->
        [one] { $clans } Sippe
       *[other] { $clans } Sippen
    } · { $lines ->
        [one] { $lines } Linie
       *[other] { $lines } Linien
    }
status-saved = alle Änderungen gespeichert
status-saving = speichern… ({ $pending })
status-save-error = Speicherfehler: { $error }

## welcome

welcome-title = Willkommen bei Wurzelwerk
welcome-body = Erstelle zum Start einen Arbeitsbereich. Ein Arbeitsbereich ist ein Ordner, der deine Familien, Sippen, Notizen und Dateien enthält.
welcome-location = ORT DES ARBEITSBEREICHS
welcome-location-help = Wähle einen bestehenden Arbeitsbereich zum Öffnen oder einen neuen Ordner für einen Neustart.
welcome-recent = ZULETZT VERWENDET
welcome-folder-path = Ordnerpfad
welcome-create = Arbeitsbereich erstellen
welcome-choose-folder = Ordner des Arbeitsbereichs wählen
welcome-enter-location = Bitte einen Ort angeben.
example-title = BEISPIEL-ARBEITSBEREICH
example-help = Ein generiertes Familienbrett (11 Sippen, rund 600 Personen über vier Jahrhunderte), um das Brett und die Zeitleiste zu erkunden. Es wird beim ersten Öffnen erstellt.
example-open = Beispiel-Arbeitsbereich öffnen
error-create-folder = { $path } kann nicht erstellt werden: { $error }
error-open-database = Datenbank in { $path } kann nicht geöffnet werden: { $error }

## settings dialog

settings-general = Allgemein
settings-workspace = Arbeitsbereich
settings-updates = Updates
settings-about = Info
settings-close = Schliessen
settings-appearance = DARSTELLUNG
settings-appearance-help = Helle oder dunkle Darstellung wählen oder dem System folgen.
settings-system = System
settings-light = Hell
settings-dark = Dunkel
settings-language = SPRACHE
settings-language-help = Sprache der Benutzeroberfläche.
settings-current-workspace = AKTUELLER ARBEITSBEREICH
settings-current-workspace-help = Alles, was du auf dem Brett siehst, ist in diesem Ordner gespeichert.
settings-open-folder = Ordner öffnen
settings-relations = BEZIEHUNGEN
settings-relations-help = Selbst eingegebene Linien-Namen. Sie werden beim Zeichnen einer Linie wieder angeboten; gib jedem einen Namen pro Sprache. Umbenennen benennt auch die Linien um, die ihn verwenden.
settings-no-relations = Noch keine eigenen Beziehungen. Gib beim Erstellen einer Linie einen neuen Namen ein.
settings-remove-relation = Aus den Vorschlägen entfernen (Linien behalten ihren Namen)
settings-switch-workspace = ARBEITSBEREICH WECHSELN
settings-switch-workspace-help = Wähle einen Ordner, um ihn als Arbeitsbereich zu öffnen. Ein neuer oder leerer Ordner wird zu einem neuen Arbeitsbereich.
settings-browse = Durchsuchen…
settings-open = Öffnen
settings-no-recent = Noch keine weiteren Arbeitsbereiche.
settings-recent-help = Einen früher verwendeten Arbeitsbereich öffnen.
settings-version = VERSION
settings-current-version = Aktuelle Version: { $version }
settings-check-now = Jetzt prüfen
settings-checking = Prüfe…
settings-up-to-date = Du bist auf dem neuesten Stand.
settings-automatic-check = AUTOMATISCHE PRÜFUNG
settings-automatic-check-help = Sucht beim Start von Wurzelwerk nach einer neuen Version und lädt sie im Hintergrund herunter.
settings-check-on-start = Beim Start nach Updates suchen
about-version = Version { $version }
about-tagline = Sippen- und Familienbrett
about-settings-file = EINSTELLUNGSDATEI
about-settings-file-help = Die globalen Einstellungen werden hier gespeichert:
about-licence = LIZENZ
about-source-code = QUELLCODE
about-open-github = Auf GitHub öffnen
error-save-settings = Einstellungen konnten nicht gespeichert werden: { $error }

## updates

update-available = Version { $version } ist verfügbar.
update-ready = Version { $version } ist bereit zur Installation.
update-downloading = Version { $version } wird heruntergeladen…
update-download = Herunterladen
update-dismiss = Ausblenden
update-restart = Jetzt neu starten
update-check-failed = Update-Prüfung fehlgeschlagen: { $error }
update-failed = Update fehlgeschlagen: { $error }
