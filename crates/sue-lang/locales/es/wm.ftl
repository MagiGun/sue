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
open-at-launch = Abrir { $name } al iniciar
command-palette = Paleta de comandos ({ $command_palette })
settings = Ajustes
quit-sue = Salir de Sue
language-menu = Idioma (Language)
language-system = Sistema ({ $language })
language-follow-sue = Seguir a Sue ({ $language })
language-changed = Sue habla { $language }
could-not-change-language = No se pudo cambiar el idioma: { $error }

## El Hub y la chincheta

sue-hub-every-studio-tool-files = Sue: el Hub, cada estudio y herramienta, archivos, proyectos y Ajustes
pinned-sue-opens-here-launch-click = Fijado: Sue se abre aquí al iniciar. Haz clic para quitar la chincheta
pin-sue-opens-here-launch = Fijar: Sue se abre aquí al iniciar
sue-opens-hub-launch = Sue se abre en el Hub al iniciar
sue-opens-in-at-launch = Sue se abre en { $name } al iniciar
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
click-to-show-it = {"."} Haz clic para mostrarlo
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
saved = { $name }{ $note } guardado{ $folder }{ $click }
saved-2 = Guardado
saved-in-folder = {" "}en { $folder }
saved-projects = Proyectos guardados
unsaved-work = Trabajo sin guardar
untitled = Sin título
work-sue-keeps-you-before-saved = Aquí aparece el trabajo que Sue guarda por ti antes de que lo guardes, para recuperarlo o descartarlo.
