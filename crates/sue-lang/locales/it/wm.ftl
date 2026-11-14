## The shell (crates/sue-wm).

hub = Hub
studios = Studi
tools = Strumenti
new = Nuovo
new-file-kind = Nuovo { $category }: { $label }
menu-file = File
menu-project = Progetto
menu-send-to = Invia a
open-file = Apri file
open-file-ellipsis = Apri file...
open-project = Apri progetto
open-project-ellipsis = Apri progetto...
save-project = Salva progetto
save-project-as = Salva progetto con nome
save-project-as-ellipsis = Salva progetto con nome...
open-at-launch = Apri { $name } all'avvio
command-palette = Tavolozza dei comandi ({ $command_palette })
check-for-updates = Controlla aggiornamenti
settings = Impostazioni
quit-sue = Esci da Sue
language-menu = Lingua (Language)
language-system = Lingua di sistema ({ $language })
language-follow-sue = Segui Sue ({ $language })
language-changed = Sue ora usa la lingua { $language }
could-not-change-language = Impossibile cambiare la lingua: { $error }
layout-menu = Layout
layout-automatic = Automatico ({ $layout })
layout-desktop = Desktop
layout-tablet = Tablet
layout-phone = Telefono
layout-phone-shaped = Formato telefono
layout-changed = Sue ora usa il layout { $layout }
could-not-change-layout = Impossibile modificare il layout: { $error }
sue-hub-every-studio-tool-files = Sue: l'Hub, ogni studio e strumento, file, progetti e Impostazioni
pinned-sue-opens-here-launch-click = Fissato: Sue si apre qui all'avvio. Fai clic per rimuovere
pin-sue-opens-here-launch = Fissa: Sue si apre qui all'avvio
sue-opens-hub-launch = Sue si apre nell'Hub all'avvio
sue-opens-in-at-launch = Sue si apre in { $name } all'avvio
could-not-pin-it = Impossibile fissare: { $error }
type-command-mode-entity-action = Digita un comando, modalità o azione
nothing-matches = Nessun risultato
go-to-the-hub = Vai all'Hub
open = Apri { $name }
pin-open-it-at-launch = Fissa { $name }: Apri all'avvio
unpin-open-hub-launch = Rimuovi: Apri l'Hub all'avvio
open-settings = Apri impostazioni
label-work = Lavoro
transfer-copy = Copia
transfer-export = Esporta
transfer-import = Importa
recent = Recenti
files = File
open-recent = Apri recenti
saved-projects = Progetti salvati
untitled = Senza titolo
work-sue-keeps-you-before-saved = Il lavoro non salvato salvaguardato da Sue appare qui per il recupero o l'eliminazione.
unsaved-work = Lavoro non salvato
files-projects-you-open-save-show = I file e i progetti aperti o salvati appaiono qui.
projects-you-save-sue-menu-project = I progetti salvati (Menu Sue -> Progetto -> Salva progetto) appaiono qui.
ago-just-now = poco fa
ago-minutes = { $count ->
    [one] un minuto fa
   *[other] { $count } minuti fa
}
ago-hours = { $count ->
    [one] un'ora fa
   *[other] { $count } ore fa
}
ago-yesterday = ieri
ago-days = { $count ->
    [one] un giorno fa
   *[other] { $count } giorni fa
}
in-folder = { $lead } in { $folder }
click-to-show-it = {"."} Fai clic per visualizzarlo
opened = Aperto
saved = Salvato { $name }{ $note }{ $folder }{ $click }
saved-2 = Salvato
saved-in-folder = {" "}in { $folder }
recover = Recupera
discard = Elimina
really-discard = Conferma eliminazione
kept-newer-than = Mantenuto { $value }, più recente di { $value2 }
kept-never-saved = Mantenuto { $value }, mai salvato
that-is-gone = L'elemento è stato rimosso
entities = { $count ->
    [one] { $count } entità
   *[other] { $count } entità
}
curve-pen-pressure = Pressione penna
curve-output = Uscita
pick-a-color = Scegli un colore
undo = Annulla
nothing-to-undo = Niente da annullare
redo = Ripeti
nothing-to-redo = Niente da ripetere
undo-depth = Annulla { $depth }
undo-failed = Annullamento non riuscito: { $error }
idle = Inattivo
fps = { $fps } FPS
close-esc = Chiudi (Esc)
none = Nessuno
there-no-studio-tool-called = Nessuno studio o strumento denominato { $id }
is-not-available = { $command } non è disponibile

## files/: the files browser (copied from paint.ftl, which keeps its own until Sue Paint's start page goes)

start-add-label = Aggiungi un'etichetta
start-all = Tutti
start-cancel = Annulla
start-clear-completed = Cancella trasferimenti completati
start-data-transfers = Trasferimenti dati
start-date-modified = Data di modifica
start-delete-label = Elimina l'etichetta
start-details = Dettagli
start-discard = Elimina questo lavoro non salvato
start-favorite = Preferito
start-kind = Tipo
start-last-worked-on = Ultima modifica
start-move-chosen-here = Inserisci qui i progetti selezionati
start-move-to = Sposta in
start-name = Nome
start-new-label = Nuova etichetta
start-no-projects = Nessun progetto al momento. Fai clic su Disegna per crearne uno; le immagini e i progetti aperti o salvati appariranno qui.
start-no-transfers = Nulla e stato ancora importato, salvato o copiato. I file aperti, salvati, esportati e copiati da Sue Paint appariranno qui.
start-none-match = Nessun elemento corrispondente.
start-open = Apri
start-really-discard = Elimina definitivamente
start-really-remove = Rimuovi definitivamente
start-remove-from-list = Rimuovi dall'elenco
start-remove-hint = I file restano dove sono; vengono rimossi solo dall'elenco
start-rename-label = Rinomina l'etichetta
start-search-projects = Cerca progetti
start-select-all = Seleziona tutto
start-show-in-folder = Mostra nella cartella
start-sort-direction = Inverti l'ordine
start-studio = Studio
start-take-off-label = Rimuovi questa etichetta
start-thumbnail = Miniatura
start-transfer-cancelled = Annullato
start-transfer-done = Completato { $when }
start-transfer-failed = Non riuscito: { $why }
start-transfer-running = In corso
start-type = Tipo
start-type-picture = Immagine
start-type-file = File
start-type-project = Progetto
start-type-unsaved = Lavoro non salvato
start-view-large = Schede grandi
start-view-list = Elenco
start-view-small = Schede piccole
start-where = Posizione
