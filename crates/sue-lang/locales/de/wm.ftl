## Die Sue-Oberfläche: Menüs, Hub, Statusleiste und gemeinsame Widgets.

## Sue-Menü
hub = Hub
studios = Studios
tools = Werkzeuge
new = Neu
new-file-kind = Neu: { $category }, { $label }
menu-file = Datei
menu-project = Projekt
menu-send-to = Senden an
open-file = Datei öffnen
open-file-ellipsis = Datei öffnen...
open-project = Projekt öffnen
open-project-ellipsis = Projekt öffnen...
save-project = Projekt speichern
save-project-as = Projekt speichern unter
save-project-as-ellipsis = Projekt speichern unter...
open-at-launch = { $name } beim Start öffnen
command-palette = Befehlspalette ({ $command_palette })
check-for-updates = Nach Updates suchen
settings = Einstellungen
quit-sue = Sue beenden
language-menu = Sprache
language-system = System ({ $language })
language-follow-sue = Sue folgen ({ $language })
language-changed = Sue spricht { $language }
could-not-change-language = Sprache konnte nicht geändert werden: { $error }
layout-menu = Layout
layout-automatic = Automatisch ({ $layout })
layout-desktop = Desktop
layout-tablet = Tablet
layout-phone = Telefon
layout-phone-shaped = Telefonformat
layout-changed = Sue verwendet das Layout { $layout }
could-not-change-layout = Layout konnte nicht geändert werden: { $error }

## Hub
sue-hub-every-studio-tool-files = Sue: Hub, alle Studios und Werkzeuge, Dateien, Projekte und Einstellungen
pinned-sue-opens-here-launch-click = Angeheftet: Sue öffnet sich hier beim Start. Klicken, um die Anheftung zu lösen
pin-sue-opens-here-launch = Anheften: Sue öffnet sich hier beim Start
sue-opens-hub-launch = Sue öffnet beim Start den Hub
sue-opens-in-at-launch = Sue öffnet beim Start { $name }
could-not-pin-it = Konnte nicht angeheftet werden: { $error }

## Befehlspalette
type-command-mode-entity-action = Befehl, Modus oder Aktion eingeben
nothing-matches = Keine Treffer
go-to-the-hub = Zum Hub
open = { $name } öffnen
pin-open-it-at-launch = { $name } anheften und beim Start öffnen
unpin-open-hub-launch = Anheftung lösen und beim Start den Hub öffnen
open-settings = Einstellungen öffnen
undo = Rückgängig
redo = Wiederholen

## Statusleiste
there-no-studio-tool-called = Es gibt kein Studio und Werkzeug namens { $id }
that-is-gone = Das ist nicht mehr vorhanden
is-not-available = { $command } ist nicht verfügbar
nothing-to-redo = Nichts zu wiederholen
nothing-to-undo = Nichts rückgängig zu machen
undo-failed = Rückgängig fehlgeschlagen: { $error }
entities = { $count ->
    [one] { $count } Entität
   *[other] { $count } Entitäten
}
undo-depth = { $depth } Schritte rückgängig
fps = { $fps } FPS
idle = Inaktiv

## Fenster und Widgets
close-esc = Schließen (Esc)
files = Dateien
pick-a-color = Farbe auswählen
none = Keine
curve-pen-pressure = Stiftdruck
curve-output = Ausgabe

## Hub-Listen
open-recent = Zuletzt geöffnete Dateien
recent = Zuletzt verwendet
files-projects-you-open-save-show = Geöffnete und gespeicherte Dateien und Projekte erscheinen hier.
saved-projects = Gespeicherte Projekte
projects-you-save-sue-menu-project = Gespeicherte Projekte (Sue-Menü, Projekt, Projekt speichern) erscheinen hier.
unsaved-work = Ungespeicherte Arbeit
work-sue-keeps-you-before-saved = Sue bewahrt hier ungespeicherte Arbeit zum Wiederherstellen oder Verwerfen auf.
saved-2 = Gespeichert
opened = Geöffnet
kept-newer-than = Aufbewahrt am { $value }, neuer als { $value2 }
kept-never-saved = Aufbewahrt am { $value }, nie gespeichert
really-discard = Wirklich verwerfen
discard = Verwerfen
recover = Wiederherstellen
ago-just-now = gerade eben
ago-yesterday = gestern
ago-minutes = { $count ->
    [one] vor einer Minute
   *[other] vor { $count } Minuten
}
ago-hours = { $count ->
    [one] vor einer Stunde
   *[other] vor { $count } Stunden
}
ago-days = { $count ->
    [one] vor einem Tag
   *[other] vor { $count } Tagen
}
untitled = Unbenannt
label-work = Arbeit
transfer-import = Importieren
transfer-export = Exportieren
transfer-copy = Kopieren
 transfer-copy = Kopieren
click-to-show-it = {"."} Zum Anzeigen klicken
in-folder = { $lead } in { $folder }
saved = { $name } gespeichert{ $note }{ $folder }{ $click }
saved-in-folder = {" "}in { $folder }

