## Le impostazioni di Sue (the rice). Bozza: in attesa di revisione da parte di un madrelingua.

## [general]

general-confirm_on_exit = Conferma all'uscita
    .description = Chiedi prima di chiudere la finestra
general-language = Lingua
    .description = La lingua di Sue, seguita da ogni studio e strumento salvo impostazioni specifiche (sistema segue la lingua del sistema)
general-autosave_seconds = Salvataggio automatico ogni
    .description = Salva il lavoro non salvato a questo intervallo, per recuperarlo dall'Hub (s; 0 disattiva)

## [launchpad]

launchpad-recent_count = File recenti sull'Hub
    .description = Quanti file e progetti recenti visualizza l'Hub (0 nasconde l'elenco)
launchpad-pinned = Apri all'avvio
    .description = Lo studio o lo strumento che Sue apre direttamente all'avvio (la puntina sull'Hub; vuoto apre l'Hub)

## [profile]

profile-hardware = Preferenza di prestazioni
    .description = Frequenza dei fotogrammi e budget degli effetti
    .auto = Automatico
    .high-refresh = Frequenza elevata
    .balanced = Bilanciato
    .legacy-low-spec = Dispositivo meno recente
    .mobile-battery-saver = Risparmio energetico

## [display]

display-frame_rate_cap = Limite frequenza fotogrammi
    .description = Massima frequenza fotogrammi durante le animazioni (0 segue la preferenza di prestazioni)
display-ui_scale = Scala interfaccia
    .description = Scala dell'interfaccia aggiuntiva rispetto alla scala del sistema
display-interface = Interfaccia
    .description = Layout: automatico segue il dispositivo (layout touch su tablet e telefoni); seleziona per cambiare
    .auto = Automatico
    .desktop = Desktop
    .tablet = Tablet
    .phone = Telefono
display-phone_frame = A forma di telefono
    .description = Su computer, disegna il layout del telefono in una colonna centrale larga quanto uno schermo telefonico
display-gpu_preference = Preferenza GPU
    .description = GPU che disegna la finestra; automatico usa la GPU dello schermo (richiede riavvio)
    .auto = Automatico
    .integrated = Integrata
    .discrete = Dedicata

## [motion]

motion-instant_transitions = Transizioni istantanee
    .description = Cambia pagine, menu e finestre in 0 ms senza animazioni
motion-animation_speed = Velocita animazione
    .description = Velocita di ogni animazione a molla
motion-spring_stiffness = Rigidita molla
    .description = Costante elastica k di ogni animazione
motion-spring_damping = Smorzamento molla
    .description = Coefficiente di smorzamento c di ogni animazione
motion-settle_grace_ms = Tolleranza assestamento (ms)
    .description = Continua a ridisegnare per questo tempo dopo l'ultima modifica (ms)
motion-caret_blink_ms = Lampeggio cursore (ms)
    .description = Intervallo di lampeggio del cursore di testo (0 mantiene il cursore fisso)
motion-smooth_caret = Cursore fluido
    .description = Lascia scorrere il cursore di testo verso la nuova posizione anziche saltare
motion-caret_glide_ms = Tempo scorrimento cursore
    .description = Tempo impiegato dal cursore per scorrere verso la nuova posizione (ms; minore e piu veloce)
motion-message_seconds = Durata messaggi
    .description = Durata di permanenza dei messaggi nella barra di stato (s)

## [layout]

layout-padding = Spaziatura interna
    .description = Spazio all'interno di barre, controlli e pulsanti (px)
layout-border_width = Spessore bordo
    .description = Spessore del bordo sottile (px)
layout-control_radius = Raggio controlli
    .description = Arrotondamento degli angoli di pulsanti e campi (px)
layout-pill_radius = Raggio pillole
    .description = Arrotondamento degli angoli di finestre, menu e pulsanti a pillola (px)
layout-pill_height = Altezza pillole
    .description = Altezza dei pulsanti a pillola (px)
layout-touch_target = Altezza bersaglio tocco
    .description = Altezza minima dei controlli touch (px)
layout-sidebar_width = Larghezza barra laterale
    .description = Larghezza predefinita della barra laterale (px)
layout-panel_header_height = Altezza intestazione pannello
    .description = Altezza dell'intestazione sopra un pannello (px)
layout-shadow_radius = Raggio sfumatura ombra
    .description = Raggio di sfocatura delle ombre sotto i pannelli mobili (px)
layout-status_bar = Barra di stato
    .description = Mostra la barra di stato sottile in fondo alla finestra (senza di essa i messaggi appaiono come avvisi)

## [colors]

## [typography]

typography-ui_font = Carattere interfaccia
    .description = Carattere dell'interfaccia (integrato)
typography-ui_size = Dimensione interfaccia
    .description = Dimensione del testo dell'interfaccia (px)
typography-small_size = Dimensione testo piccolo
    .description = Dimensione del testo per didascalie e barra di stato (px)
typography-heading_size = Dimensione intestazioni
    .description = Dimensione del testo per le intestazioni (px)
typography-mono_size = Dimensione testo a spaziatura fissa
    .description = Dimensione del testo a spaziatura fissa (px)

## [input]

## [io]

## [audio]

audio-buffer_frames = Fotogrammi buffer audio
    .description = Campioni inviati alla scheda audio a ogni passaggio (0 per l'impostazione predefinita della scheda)
audio-sample_rate = Frequenza di campionamento
    .description = Frequenza interna dell'elaborazione audio (Hz; 0 per l'impostazione predefinita)
audio-channels = Canali audio
    .description = Quanti canali elabora il motore audio (0 per l'impostazione predefinita)
audio-exclusive = Modalita audio esclusiva
    .description = Prendi il controllo esclusivo del dispositivo audio per ridurre la latenza (Windows WASAPI)
audio-master_volume = Volume principale
    .description = Volume di uscita generale di Sue (%)

## [packages]

## [updates]

updates-check_automatically = Controlla automaticamente
    .description = Cerca una versione piu recente di Sue all'avvio, al massimo una volta al giorno

## [sue_paint]

sue_paint-width = Larghezza
    .description = Larghezza delle nuove illustrazioni (px)
sue_paint-height = Altezza
    .description = Altezza delle nuove illustrazioni (px)
sue_paint-dpi = Risoluzione
    .description = Risoluzione delle nuove illustrazioni (dpi)
sue_paint-materials_dir = Cartella materiali
    .description = Dove Sue Paint conserva materiali e punte dei pennelli (vuoto: la propria cartella)
sue_paint-save_format = Formato di salvataggio
    .description = Come Salva conserva un'immagine: file .clip di Clip Studio Paint o progetto Sue
    .clip = Clip Studio Paint (.clip)
    .sue = Progetto Sue (.sue)
sue_paint-touch_mode = Modalita touch
    .description = Sue Paint su telefoni e tablet: Modalita Semplice o Modalita Studio
    .simple = Semplice
    .studio = Studio
sue_paint-custom_key_1 = Tasto personalizzato 1
    .description = Il comando eseguito dal tasto T1 in Modalita Studio
sue_paint-custom_key_2 = Tasto personalizzato 2
    .description = Il comando eseguito dal tasto T2 in Modalita Studio
sue_paint-custom_key_3 = Tasto personalizzato 3
    .description = Il comando eseguito dal tasto T3 in Modalita Studio
sue_paint-custom_key_4 = Tasto personalizzato 4
    .description = Il comando eseguito dal tasto T4 in Modalita Studio
sue_paint-custom_key_5 = Tasto personalizzato 5
    .description = Il comando eseguito dal tasto T5 in Modalita Studio
sue_paint-custom_key_6 = Tasto personalizzato 6
    .description = Il comando eseguito dal tasto T6 in Modalita Studio
sue_paint-custom_key_7 = Tasto personalizzato 7
    .description = Il comando eseguito dal tasto T7 in Modalita Studio
sue_paint-theme = Tema
    .description = Colori di Sue Paint: Grigio, tema di Sue (lo segue), un preset o Personalizzato
    .gray = Grigio
    .sue-theme = Tema di Sue
    .void = Vuoto
    .graphite = Grafite
    .paper = Carta
    .custom = Personalizzato
sue_paint-pen_pressure = Pressione penna
    .description = Curva di pressione della penna per ogni strumento (regolabile da File, Regolazione pressione penna)
sue_paint-shift_line = Shift per linee rette
    .description = Maiusc e trascina disegna una linea retta, mostrata mentre viene tracciata (Preferenze, Strumenti)
sue_paint-two_finger_tap = Tocco a due dita
    .description = Cosa fa un tocco rapido con due dita sulla tela (Preferenze, Gesti touch)
    .undo = Annulla
    .none = Niente
sue_paint-three_finger_tap = Tocco a tre dita
    .description = Cosa fa un tocco rapido con tre dita sulla tela (Preferenze, Gesti touch)
    .redo = Ripeti
    .none = Niente
sue_paint-brush_cursor = Cursore pennello
    .description = Il puntatore sulla tela durante la pittura: sagoma pennello, mirino o punto (Preferenze, Cursore)
    .brush = Sagoma pennello
    .crosshair = Mirino
    .dot = Punto
sue_paint-selection_layer_opacity = Opacita livelli di selezione
    .description = Opacita di un nuovo livello di selezione (%; Preferenze, Livello/Riquadro)
sue_paint-rotate_step = Passo di rotazione
    .description = Di quanto Ruota a sinistra e Ruota a destra ruotano la vista (gradi; Preferenze, Tela)
sue_paint-alt_tool = Strumento Alt
    .description = Cosa fa Alt durante il disegno sulla tela (File, Impostazioni tasti modificatori)
    .eyedropper = Contagocce
    .eraser = Gomma
    .none = Nessuno
sue_paint-space_tool = Strumento Spazio
    .description = Cosa fa Spazio sulla tela (File, Impostazioni tasti modificatori)
    .hand = Mano
    .none = Nessuno
sue_paint-ctrl_shift = Ctrl+Maiusc
    .description = Cosa fa clic con Ctrl+Maiusc sulla tela (File, Impostazioni tasti modificatori)
    .pick_layer = Seleziona livello
    .none = Nessuno
sue_paint-language = Lingua
    .description = Lingua: sue segue la lingua di Sue, oppure impostane una dedicata

## [sue_paint_colors]

sue_paint_colors-void = Vuoto
    .description = Attorno all'immagine
sue_paint_colors-panel = Pannello
    .description = Tavolozze e menu
sue_paint_colors-pill = Pillola
    .description = Campi, chip e schede
sue_paint_colors-hover = Al passaggio
    .description = Controlli sotto il puntatore
sue_paint_colors-active = Attivo
    .description = Controlli premuti e selezionati
sue_paint_colors-border = Bordo
    .description = Linee di contorno sottili
sue_paint_colors-border_active = Bordo attivo
    .description = Bordi dei controlli scelti e a fuoco
sue_paint_colors-text = Testo
    .description = Testo principale e icone attive
sue_paint_colors-muted = Attenuato
    .description = Testo secondario
sue_paint_colors-selection = Selezione
    .description = Evidenziazione della selezione di testo
sue_paint_colors-selection_text = Testo selezionato
    .description = Testo all'interno di una selezione
sue_paint_colors-grid = Griglia
    .description = Griglie e righelli
sue_paint_colors-shadow = Ombra
    .description = Ombra sotto le superfici mobili

## [sue_writer]

sue_writer-theme = Tema
    .description = Colori di Sue Writer: tema di Sue (lo segue) o un preset
    .sue-theme = Tema di Sue
    .gray = Grigio
    .void = Vuoto
    .graphite = Grafite
    .paper = Carta
sue_writer-smart_quotes = Virgolette tipografiche
    .description = Trasforma le virgolette dritte in curve durante la digitazione
sue_writer-dark_paper = Carta scura
    .description = Mostra la pagina nei colori scuri del tema anziche come carta bianca
sue_writer-spell_check = Controllo ortografico
    .description = Sottolinea le parole errate durante la digitazione
sue_writer-language = Lingua
    .description = Lingua: sue segue la lingua di Sue, oppure impostane una dedicata

## [image_viewer]

image_viewer-language = Lingua
    .description = Lingua: sue segue la lingua di Sue, oppure impostane una dedicata

## [convert]

convert-language = Lingua
    .description = Lingua: sue segue la lingua di Sue, oppure impostane una dedicata

## [keys]

keys-command_palette = Tavolozza dei comandi
    .description = Apri la tavolozza dei comandi
keys-command_palette_alt = Tavolozza dei comandi alt
    .description = Apri la tavolozza dei comandi (alternativa)
keys-open_settings = Apri impostazioni
    .description = Apri Impostazioni
keys-open_file = Apri file
    .description = Apri un file
keys-save = Salva
    .description = Salva il progetto
keys-quit = Esci
    .description = Esci da Sue
keys-undo = Annulla
    .description = Annulla l'ultima modifica
keys-redo = Ripeti
    .description = Ripeti
keys-redo_alt = Ripeti alt
    .description = Ripeti (alternativa)

## [sue_paint_keys]

sue_paint_keys-help_shortcuts = Aiuto scorciatoie
    .description = Impostazioni scorciatoie da tastiera
sue_paint_keys-file_preferences = File preferenze
    .description = Preferenze
sue_paint_keys-file_save_as = File salva con nome
    .description = Salva con nome
sue_paint_keys-file_print = File stampa
    .description = Stampa
sue_paint_keys-layer_new = Livello nuovo
    .description = Livello raster
sue_paint_keys-file_new = File nuovo
    .description = Nuovo
sue_paint_keys-file_close = File chiudi
    .description = Chiudi
sue_paint_keys-edit_transform_free = Modifica trasforma libera
    .description = Trasformazione libera
sue_paint_keys-edit_transform_scale = Modifica trasforma scala
    .description = Scala/Ruota
sue_paint_keys-filter_hue = Filtro tonalita
    .description = Tonalita/Saturazione/Luminosita
sue_paint_keys-select_invert = Selezione inverti
    .description = Inverti area selezionata
sue_paint_keys-filter_invert = Filtro inverti
    .description = Inverti gradiente
sue_paint_keys-edit_cut = Modifica taglia
    .description = Taglia
sue_paint_keys-edit_copy = Modifica copia
    .description = Copia
sue_paint_keys-edit_paste = Modifica incolla
    .description = Incolla
sue_paint_keys-edit_clear = Modifica cancella
    .description = Elimina
sue_paint_keys-edit_fill = Modifica riempi
    .description = Riempi
sue_paint_keys-layer_toggle_clip = Livello attiva ritaglio
    .description = Ritaglia sul livello sottostante
sue_paint_keys-layer_folder_insert = Livello inserisci cartella
    .description = Crea cartella e inserisci livello
sue_paint_keys-layer_merge_visible_here = Livello unisci visibili qui
    .description = Unisci livelli visibili
sue_paint_keys-layer_ungroup = Livello separa
    .description = Separa cartella livelli
sue_paint_keys-layer_merge_selected = Livello unisci selezionati
    .description = Unisci livelli selezionati
sue_paint_keys-layer_merge_down = Livello unisci sotto
    .description = Unisci con livello sottostante
sue_paint_keys-layer_select_above = Livello seleziona sopra
    .description = Seleziona livello sopra
sue_paint_keys-layer_select_below = Livello seleziona sotto
    .description = Seleziona livello sotto
sue_paint_keys-layer_order_up = Livello sposta su
    .description = Sposta su
sue_paint_keys-layer_order_down = Livello sposta giu
    .description = Sposta giu
sue_paint_keys-select_again = Selezione riseleziona
    .description = Riseleziona
sue_paint_keys-select_all = Selezione seleziona tutto
    .description = Seleziona tutto
sue_paint_keys-select_none = Selezione deseleziona
    .description = Deseleziona
sue_paint_keys-view_actual = Vista 100%
    .description = 100%
sue_paint_keys-view_fit = Vista adatta
    .description = Adatta allo schermo
sue_paint_keys-view_rotate_reset = Vista reimposta rotazione
    .description = Reimposta rotazione
sue_paint_keys-view_zoom_in = Vista ingrandisci
    .description = Ingrandisci
sue_paint_keys-view_zoom_out = Vista rimpicciolisci
    .description = Rimpicciolisci
sue_paint_keys-view_ruler_bar = Vista barra righelli
    .description = Barra dei righelli
sue_paint_keys-view_snap_ruler = Vista aggancia righello
    .description = Aggancia al righello
sue_paint_keys-view_snap_special = Vista aggancia righello speciale
    .description = Aggancia al righello speciale
sue_paint_keys-view_snap_grid = Vista aggancia griglia
    .description = Aggancia alla griglia
sue_paint_keys-view_rotate_left = Vista ruota sinistra
    .description = Ruota a sinistra
sue_paint_keys-view_rotate_right = Vista ruota destra
    .description = Ruota a destra
sue_paint_keys-window_prev_tab = Finestra scheda precedente
    .description = Tela precedente
sue_paint_keys-window_next_tab = Finestra scheda successiva
    .description = Tela successiva
sue_paint_keys-window_hide_menus = Finestra nascondi menu
    .description = Nascondi barra dei menu
sue_paint_keys-window_hide_all = Finestra nascondi tutto
    .description = Nascondi tutte le tavolozze
sue_paint_keys-tool_zoom = Strumento zoom
    .description = Strumento Zoom
sue_paint_keys-tool_hand = Strumento mano
    .description = Strumento Mano
sue_paint_keys-tool_rotate = Strumento ruota
    .description = Strumento Ruota
sue_paint_keys-tool_operation = Strumento operazione
    .description = Strumento Operazione
sue_paint_keys-tool_move_layer = Strumento sposta livello
    .description = Strumento Sposta livello
sue_paint_keys-tool_selection = Strumento selezione
    .description = Strumento Selezione
sue_paint_keys-tool_auto_select = Strumento selezione automatica
    .description = Strumento Selezione automatica
sue_paint_keys-tool_eyedropper = Strumento contagocce
    .description = Strumento Contagocce
sue_paint_keys-tool_pen = Strumento penna
    .description = Strumenti Penna e Matita
sue_paint_keys-tool_brush = Strumento pennello
    .description = Strumenti Pennello, Aerografo e Decorazione
sue_paint_keys-tool_eraser = Strumento gomma
    .description = Strumento Gomma
sue_paint_keys-tool_blend = Strumento miscela
    .description = Strumento Miscela colore
sue_paint_keys-tool_fill = Strumento riempi
    .description = Strumenti Riempi e Gradiente
sue_paint_keys-tool_figure = Strumento figura
    .description = Strumenti Figura, Bordo vignetta e Righello
sue_paint_keys-tool_text = Strumento testo
    .description = Strumenti Testo e Fumetto
sue_paint_keys-tool_correct_line = Strumento correggi linea
    .description = Strumento Correzione linea
sue_paint_keys-tool_previous_sub = Strumento sottostrumento precedente
    .description = Sottostrumento precedente
sue_paint_keys-tool_next_sub = Strumento sottostrumento successivo
    .description = Sottostrumento successivo
sue_paint_keys-color_swap = Scambia colore
    .description = Scambia colore principale e secondario
sue_paint_keys-color_transparent = Colore trasparente
    .description = Colore di disegno o trasparente
sue_paint_keys-brush_smaller = Pennello piu piccolo
    .description = Pennello piu piccolo
sue_paint_keys-brush_bigger = Pennello piu grande
    .description = Pennello piu grande

## [sue_writer_keys]

sue_writer_keys-binder_new_folder = Raccoglitore nuova cartella
    .description = Nuova cartella
sue_writer_keys-binder_new_doc = Raccoglitore nuovo documento
    .description = Nuovo documento
sue_writer_keys-compile_open = Compila apri
    .description = Compila
sue_writer_keys-compile_pdf = Compila PDF
    .description = PDF
sue_writer_keys-format_bold = Formato grassetto
    .description = Grassetto
sue_writer_keys-format_italic = Formato corsivo
    .description = Corsivo
sue_writer_keys-format_underline = Formato sottolineato
    .description = Sottolineato
sue_writer_keys-format_strike = Formato barrato
    .description = Barrato
sue_writer_keys-project_split = Progetto dividi
    .description = Dividi nella selezione
sue_writer_keys-format_superscript = Formato apice
    .description = Apice
sue_writer_keys-format_subscript = Formato pedice
    .description = Pedice
sue_writer_keys-format_clear = Formato cancella
    .description = Cancella formattazione diretta
sue_writer_keys-align_center = Allinea centro
    .description = Centra
sue_writer_keys-align_left = Allinea sinistra
    .description = Sinistra
sue_writer_keys-align_right = Allinea destra
    .description = Destra
sue_writer_keys-align_justify = Allinea giustifica
    .description = Giustifica
sue_writer_keys-style_heading1 = Stile intestazione 1
    .description = Intestazione 1
sue_writer_keys-style_heading2 = Stile intestazione 2
    .description = Intestazione 2
sue_writer_keys-style_heading3 = Stile intestazione 3
    .description = Intestazione 3
sue_writer_keys-style_body = Stile corpo
    .description = Testo corpo
sue_writer_keys-insert_footnote = Inserisci nota a pie di pagina
    .description = Nota a pie di pagina
sue_writer_keys-insert_page_break = Inserisci interruzione di pagina
    .description = Interruzione di pagina
sue_writer_keys-insert_link = Inserisci collegamento
    .description = Collegamento
sue_writer_keys-edit_find = Modifica trova
    .description = Trova
sue_writer_keys-edit_replace = Modifica sostituisci
    .description = Sostituisci
sue_writer_keys-edit_find_previous = Modifica trova precedente
    .description = Trova precedente
sue_writer_keys-edit_find_next = Modifica trova successivo
    .description = Trova successivo
sue_writer_keys-view_composition = Vista composizione
    .description = Modalita composizione
sue_writer_keys-view_document = Vista documento
    .description = Testo
sue_writer_keys-view_corkboard = Vista bacheca
    .description = Bacheca
sue_writer_keys-view_outliner = Vista struttura
    .description = Struttura
sue_writer_keys-project_snapshot = Progetto istantanea
    .description = Scatta istantanea
sue_writer_keys-list_bullet = Elenco puntato
    .description = Elenco puntato
sue_writer_keys-review_comment = Revisione commento
    .description = Nuovo commento
sue_writer_keys-insert_endnote = Inserisci nota di chiusura
    .description = Nota di chiusura
sue_writer_keys-review_track = Revisione registra modifiche
    .description = Registra modifiche
sue_writer_keys-review_next_misspelling = Revisione prossimo errore
    .description = Prossimo errore ortografico
sue_writer_keys-format_grow = Formato ingrandisci carattere
    .description = Ingrandisci carattere
sue_writer_keys-format_shrink = Formato rimpicciolisci carattere
    .description = Rimpicciolisci carattere
sue_writer_keys-insert_mark_nbsp = Inserisci spazio unificatore
    .description = Spazio unificatore

## [image_viewer_keys]

image_viewer_keys-viewer_next = Visualizzatore successiva
    .description = Immagine successiva
image_viewer_keys-viewer_previous = Visualizzatore precedente
    .description = Immagine precedente
image_viewer_keys-viewer_fit = Visualizzatore adatta
    .description = Adatta alla finestra
image_viewer_keys-viewer_actual = Visualizzatore dimensioni reali
    .description = Dimensioni reali
image_viewer_keys-viewer_zoom_in = Visualizzatore ingrandisci
    .description = Ingrandisci
image_viewer_keys-viewer_zoom_out = Visualizzatore rimpicciolisci
    .description = Rimpicciolisci
image_viewer_keys-viewer_rotate_right = Visualizzatore ruota destra
    .description = Ruota a destra
image_viewer_keys-viewer_rotate_left = Visualizzatore ruota sinistra
    .description = Ruota a sinistra
image_viewer_keys-viewer_crop = Visualizzatore ritaglia
    .description = Ritaglia
image_viewer_keys-viewer_save_as = Visualizzatore salva con nome
    .description = Salva con nome


layout-bar_height = Altezza barra
    .description = Altezza della barra superiore con il pulsante Sue e i menu (px; i layout touch la rendono piu alta)

layout-status_bar_height = Altezza barra di stato
    .description = Altezza della barra di stato (px)

layout-settings_sidebar_width = Larghezza barra laterale Impostazioni
    .description = Larghezza dell'elenco categorie in Impostazioni (px)

layout-shadow_blur = Sfocatura ombra
    .description = Sfocatura dell'ombra sotto menu e finestre (px)

colors-grid = Griglia
    .description = Linee della griglia negli editor di curve

input-wheel_line_px = Linea rotellina (px)
    .description = Pixel scorsi per scatto della rotellina del mouse negli elenchi

io-jxl_effort = Sforzo JPEG XL
    .description = Sforzo JPEG XL nelle conversioni (piu alto e piu piccolo e lento)

io-jpeg_lossless_transcode = Transcodifica lossless JPEG
    .description = Ricomprimi JPEG in JPEG XL bit per bit se e installato un codec di ricompressione

io-jpeg_quality = Qualita JPEG
    .description = Qualita JPEG per le conversioni in .jpg

io-byte_map_width = Larghezza mappa di byte
    .description = Colonne nella mappa di byte dei file sconosciuti

io-max_ingest_mib = Dimensione massima apertura (MiB)
    .description = Dimensione massima del file aperto da Sue (MiB)

io-convert_dir = Cartella di conversione
    .description = Cartella per i file convertiti (vuoto salva accanto all'originale)

audio-output_device = Dispositivo di uscita
    .description = Dispositivo di uscita (vuoto per il predefinito del sistema)

audio-realtime_priority = Priorita in tempo reale
    .description = Chiedi al sistema operativo di pianificare il thread audio in tempo reale

audio-volume = Volume
    .description = Volume di riproduzione di Sue

packages-require_signatures = Richiedi firme
    .description = Accetta solo pacchetti con una firma Ed25519 valida
