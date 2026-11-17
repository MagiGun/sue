## El shell (crates/sue-wm). Borrador: espera la revisión de un hablante nativo.

## El menú Sue

hub = Hub
studios = Estudios
tools = Herramientas
new = Nuevo
new-file-kind = Nuevo { $category }: { $label }
menu-file = Archivo
menu-project = Proyecto
menu-send-to = Enviar a
open-file = Abrir archivo
open-file-ellipsis = Abrir archivo...
open-project = Abrir proyecto
open-project-ellipsis = Abrir proyecto...
save-project = Guardar proyecto
save-project-as = Guardar proyecto como
save-project-as-ellipsis = Guardar proyecto como...
settings = Ajustes
quit-sue = Salir de Sue
language-system = Sistema ({ $language })
language-follow-sue = Seguir a Sue ({ $language })
could-not-change-language = No se pudo cambiar el idioma: { $error }
layout-automatic = Automática ({ $layout })
layout-desktop = Escritorio
layout-tablet = Tableta
layout-phone = Teléfono
layout-phone-shaped = Con forma de teléfono

## El Hub y la chincheta

sue-hub-every-studio-tool-files = Sue: el Hub, cada estudio y herramienta, archivos, proyectos y Ajustes
pinned-sue-opens-here-launch-click = Fijado: Sue se abre aquí al iniciar. Haz clic para quitar la chincheta
pin-sue-opens-here-launch = Fijar: Sue se abre aquí al iniciar
could-not-pin-it = No se pudo fijar: { $error }

## La paleta de comandos

type-command-mode-entity-action = Escribe un comando, un modo o una acción
nothing-matches = Nada coincide
go-to-the-hub = Ir al Hub
open = Abrir { $name }
pin-open-it-at-launch = Fijar { $name }: abrirlo al iniciar
unpin-open-hub-launch = Quitar la chincheta: abrir el Hub al iniciar
open-settings = Abrir Ajustes
undo = Deshacer
redo = Rehacer

## La barra de estado y los mensajes

there-no-studio-tool-called = No hay ningún estudio ni herramienta llamado { $id }
that-is-gone = Eso ya no existe
is-not-available = { $command } no está disponible
nothing-to-redo = Nada que rehacer
nothing-to-undo = Nada que deshacer
undo-failed = No se pudo deshacer: { $error }
entities = { $count ->
    [one] { $count } entidad
   *[other] { $count } entidades
}
undo-depth = Deshacer { $depth }
fps = { $fps } FPS
idle = En reposo

## Ventanas, diálogos y controles

close-esc = Cerrar (Esc)
files = Archivos
pick-a-color = Elegir un color
none = Ninguno
curve-pen-pressure = Presión del lápiz
curve-output = Salida
color-wheel-show-triangle = Mostrar el triángulo HLS
color-wheel-show-square = Mostrar el cuadrado HSV
window-minimize = Minimizar
window-maximize = Maximizar
window-restore = Restaurar
window-close = Cerrar

## Las listas del Hub

ago-just-now = ahora mismo
ago-yesterday = ayer
ago-minutes = { $count ->
    [one] hace un minuto
   *[other] hace { $count } minutos
}
ago-hours = { $count ->
    [one] hace una hora
   *[other] hace { $count } horas
}
ago-days = { $count ->
    [one] hace un día
   *[other] hace { $count } días
}
discard = Descartar
really-discard = Descartar de verdad
recover = Recuperar
files-projects-you-open-save-show = Aquí aparecen los archivos y proyectos que abres o guardas.
in-folder = { $lead } en { $folder }
kept-never-saved = Guardado { $value }, nunca guardado por ti
kept-newer-than = Guardado { $value }, más reciente que { $value2 }
open-recent = Abrir recientes
opened = Abierto
projects-you-save-sue-menu-project = Aquí aparecen los proyectos que guardas (menú Sue, Proyecto, Guardar proyecto).
recent = Recientes
saved-2 = Guardado
saved-in-folder = {" "}en { $folder }
saved-projects = Proyectos guardados
unsaved-work = Trabajo sin guardar
untitled = Sin título
work-sue-keeps-you-before-saved = Aquí aparece el trabajo que Sue guarda por ti antes de que lo guardes, para recuperarlo o descartarlo.

check-for-updates = Buscar actualizaciones
label-work = Trabajo
transfer-copy = Copiar
transfer-export = Exportar
transfer-import = Importar

## files/: the files browser (copied from paint.ftl, which keeps its own until Sue Paint's start page goes)

start-add-label = Añadir una etiqueta
start-all = Todo
start-cancel = Cancelar
start-clear-completed = Borrar transferencias completadas
start-data-transfers = Transferencias de datos
start-date-modified = Fecha de modificación
start-delete-label = Eliminar la etiqueta
start-details = Detalles
start-discard = Descartar este trabajo sin guardar
start-favorite = Favorito
start-kind = Tipo
start-last-worked-on = Último trabajo
start-move-chosen-here = Poner los proyectos elegidos aquí
start-move-to = Mover a
start-name = Nombre
start-new-label = Nueva etiqueta
start-no-projects = Aún no hay proyectos. Haz clic en Dibujar para empezar uno; las imágenes y proyectos que abras o guardes se muestran aquí.
start-no-transfers = Aún no se ha importado, guardado ni copiado nada. Los archivos que Sue Paint abre, guarda, exporta y copia se muestran aquí.
start-none-match = Nada aquí coincide.
start-open = Abrir
start-really-discard = Descartar de verdad
start-really-remove = Eliminar de verdad
start-remove-from-list = Quitar de la lista
start-remove-hint = Los archivos permanecen donde están; solo se eliminan de la lista
start-rename-label = Cambiar nombre de la etiqueta
start-search-projects = Buscar proyectos
start-select-all = Seleccionar todo
start-show-in-folder = Mostrar en carpeta
start-sort-direction = Invertir el orden
start-studio = Estudio
start-take-off-label = Quitar esta etiqueta
start-thumbnail = Miniatura
start-transfer-cancelled = Cancelado
start-transfer-done = Completado { $when }
start-transfer-failed = Fallido: { $why }
start-transfer-running = En ejecución
start-type = Tipo
start-type-picture = Imagen
start-type-file = Archivo
start-type-project = Proyecto
start-type-unsaved = Trabajo no guardado
start-view-large = Tarjetas grandes
start-view-list = Lista
start-view-small = Tarjetas pequeñas
start-where = Dónde
## ui/input.rs, ui/slider.rs

field-clear = Borrar
field-choices = Opciones
range-start = Inicio de { $name }
range-end = Fin de { $name }
## menubar.rs: the menus every studio shares, and notes on menu items

menu-edit = Editar
menu-view = Ver
menu-window = Ventana
menu-help = Ayuda
menu-new = Nuevo...
menu-open = Abrir...
menu-open-recent = Abrir reciente
menu-close = Cerrar
menu-save = Guardar
menu-save-as = Guardar como...
menu-export = Exportar...
menu-print = Imprimir...
menu-settings = Ajustes...
menu-back-to-hub = Volver al Hub
menu-quit = Salir de Sue
menu-undo = Deshacer
menu-redo = Rehacer
menu-cut = Cortar
menu-copy = Copiar
menu-paste = Pegar
menu-select-all = Seleccionar todo
menu-guide = Guía de { $studio }
menu-keyboard-shortcuts = Atajos de teclado
menu-check-for-updates = Buscar actualizaciones
menu-about = Acerca de { $studio }
menu-note-coming-later = Llegará más adelante
menu-note-needs-selection = Necesita una selección
## ui/dialog.rs, ui/toast.rs

ok = Aceptar
cancel = Cancelar
dialog-close = Cerrar
toast-dismiss = Descartar
notices = Avisos
