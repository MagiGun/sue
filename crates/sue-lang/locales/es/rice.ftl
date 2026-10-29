## Los ajustes de Sue, tal como los muestra Ajustes. Borrador: espera la
## revisión de un hablante nativo.

## [general]

general-confirm_on_exit = Confirmar al salir
    .description = Preguntar antes de cerrar la ventana
general-language = Idioma
    .description = El idioma de Sue, que siguen todos los estudios y herramientas salvo los que tengan uno propio (sistema sigue el idioma del sistema)

## [launchpad]

launchpad-pinned = Abrir al iniciar
    .description = El estudio o la herramienta en que Sue se abre directamente (la chincheta de su cuadro en el Hub; vacío abre el Hub)

## [profile]

profile-hardware = Preferencia de rendimiento
    .description = Ritmo de fotogramas y presupuesto de efectos
    .auto = Automático
    .high-refresh = Alta frecuencia
    .balanced = Equilibrado
    .legacy-low-spec = Equipo antiguo
    .mobile-battery-saver = Ahorro de batería

## [display]

display-frame_rate_cap = Límite de fotogramas
    .description = Fotogramas por segundo máximos al animar (0 sigue la preferencia de rendimiento)
display-ui_scale = Escala de la interfaz
    .description = Escala de la interfaz sobre la escala del sistema
display-interface = Interfaz
    .description = Disposición: automático sigue el dispositivo (disposiciones táctiles en tabletas y teléfonos); elige una para cambiarla
    .auto = Automático
    .desktop = Escritorio
    .tablet = Tableta
    .phone = Teléfono
display-gpu_preference = Preferencia de GPU
    .description = La GPU que dibuja la ventana; automático usa la GPU de la pantalla (se aplica tras reiniciar)
    .auto = Automático
    .integrated = Integrada
    .discrete = Dedicada

## [motion]

motion-instant_transitions = Transiciones instantáneas
    .description = Mostrar páginas, menús y ventanas en 0 ms en lugar de animarlos
motion-animation_speed = Velocidad de animación
    .description = Velocidad de todas las animaciones de resorte
motion-spring_stiffness = Rigidez del resorte
    .description = Constante k del resorte de cada animación
motion-spring_damping = Amortiguación del resorte
    .description = Coeficiente de amortiguación c de cada animación
motion-settle_grace_ms = Margen de reposo (ms)
    .description = Seguir dibujando este tiempo tras el último cambio (ms)
motion-caret_blink_ms = Parpadeo del cursor (ms)
    .description = Intervalo de parpadeo del cursor de texto (0 lo deja fijo)
motion-smooth_caret = Cursor suave
    .description = El cursor de texto se desliza a su nuevo lugar en lugar de saltar
motion-caret_glide_ms = Tiempo de deslizamiento del cursor
    .description = Cuánto tarda el cursor en deslizarse a su nuevo lugar (ms; menos es más rápido)
motion-message_seconds = Duración de los mensajes
    .description = Cuánto tiempo quedan los mensajes en la barra de estado (s)

## [layout]

layout-padding = Relleno
    .description = Espacio dentro de barras, píldoras y controles (px)
layout-border_width = Grosor del borde
    .description = Grosor de los bordes finos (px)
layout-control_radius = Radio de los controles
    .description = Redondeo de las esquinas de botones y campos (px)
layout-pill_radius = Radio de las píldoras
    .description = Redondeo de las esquinas de ventanas, menús y botones píldora (px)
layout-pill_height = Altura de las píldoras
    .description = Altura de los botones píldora (px)
layout-bar_height = Altura de la barra
    .description = Altura de la barra superior con el botón Sue y los menús (px; las disposiciones táctiles la hacen más alta)
layout-status_bar_height = Altura de la barra de estado
    .description = Altura de la barra de estado (px)
layout-settings_sidebar_width = Ancho de la lista de Ajustes
    .description = Ancho de la lista de categorías de Ajustes (px)
layout-shadow_blur = Desenfoque de la sombra
    .description = Desenfoque de la sombra bajo menús y ventanas (px)

## [colors]

colors-void = Fondo
    .description = Fondo de la página
colors-panel = Panel
    .description = Barras y paneles
colors-pill = Píldora
    .description = Menús, cuadros y superficies flotantes
colors-hover = Al pasar
    .description = Controles bajo el puntero
colors-active = Activo
    .description = Controles pulsados y elegidos
colors-border = Borde
    .description = Bordes finos
colors-border_active = Borde activo
    .description = Bordes de los cuadros bajo el puntero y de los controles con foco
colors-text = Texto
    .description = Texto principal e iconos activos
colors-muted = Atenuado
    .description = Texto secundario
colors-selection = Selección
    .description = Resaltado de la selección de texto
colors-selection_text = Texto seleccionado
    .description = Texto dentro de una selección
colors-grid = Cuadrícula
    .description = Líneas de cuadrícula en los editores de curvas
colors-shadow = Sombra
    .description = Sombra bajo las superficies flotantes

## [typography]

typography-ui_font = Fuente de la interfaz
    .description = Fuente de la interfaz (incluida)
typography-ui_size = Tamaño de la interfaz
    .description = Tamaño del texto de la interfaz (px)
typography-small_size = Tamaño pequeño
    .description = Tamaño del texto de leyendas y de la barra de estado (px)
typography-heading_size = Tamaño de los títulos
    .description = Tamaño de los títulos (px)
typography-mono_size = Tamaño monoespaciado
    .description = Tamaño del texto monoespaciado (px)

## [input]

input-wheel_line_px = Línea de rueda (px)
    .description = Píxeles desplazados por cada paso de la rueda del ratón en las listas

## [io]

io-jxl_effort = Esfuerzo JPEG XL
    .description = Esfuerzo de JPEG XL en las conversiones (más alto da archivos más pequeños y más lentos)
io-jpeg_lossless_transcode = Transcodificación JPEG sin pérdidas
    .description = Recomprimir JPEG a JPEG XL bit a bit cuando hay un códec de recompresión instalado
io-jpeg_quality = Calidad JPEG
    .description = Calidad JPEG en las conversiones a .jpg
io-byte_map_width = Ancho del mapa de bytes
    .description = Columnas del mapa de bytes de los archivos desconocidos
io-max_ingest_mib = Tamaño máximo (MiB)
    .description = El archivo más grande que abre Sue (MiB)
io-convert_dir = Carpeta de conversión
    .description = Carpeta para los archivos convertidos (vacío guarda junto al original)

## [audio]

audio-output_device = Dispositivo de salida
    .description = Dispositivo de salida (vacío usa el predeterminado del sistema)
audio-sample_rate = Frecuencia de muestreo
    .description = Frecuencia de muestreo (Hz)
audio-buffer_frames = Tamaño del búfer
    .description = Tamaño del búfer (muestras por llamada)
audio-realtime_priority = Prioridad en tiempo real
    .description = Pedir al sistema que ejecute el hilo de audio en tiempo real
audio-volume = Volumen
    .description = Volumen de todo lo que reproduce Sue

## [packages]

packages-require_signatures = Exigir firmas
    .description = Aceptar solo paquetes con una firma Ed25519 válida

## [sue_paint]

sue_paint-width = Ancho
    .description = Ancho de las ilustraciones nuevas (px)
sue_paint-height = Alto
    .description = Alto de las ilustraciones nuevas (px)
sue_paint-dpi = PPP
    .description = Resolución de las ilustraciones nuevas (ppp)
sue_paint-theme = Tema
    .description = Los colores de Sue Paint: Gris, el tema de Sue (lo sigue), un preajuste o Personalizado (abajo)
    .gray = Gris
    .sue-theme = Tema de Sue
    .void = Vacío
    .graphite = Grafito
    .paper = Papel
    .custom = Personalizado
sue_paint-pen_pressure = Presión del lápiz
    .description = Curva de presión del lápiz para todas las herramientas, como puntos presión,salida (Archivo, Ajustes de presión del lápiz la dibuja)
sue_paint-alt_tool = Herramienta con Alt
    .description = Qué hace Alt al dibujar en el lienzo (Archivo, Ajustes de teclas modificadoras)
    .eyedropper = Cuentagotas
    .eraser = Borrador
    .none = Nada
sue_paint-space_tool = Herramienta con Espacio
    .description = Qué hace Espacio en el lienzo (Archivo, Ajustes de teclas modificadoras)
    .hand = Mano
    .none = Nada
sue_paint-ctrl_shift = Ctrl Mayús
    .description = Qué hace Ctrl+Mayús+clic en el lienzo (Archivo, Ajustes de teclas modificadoras)
    .pick_layer = Elegir capa
    .none = Nada
sue_paint-language = Idioma
    .description = Idioma: sue sigue el idioma de Sue; una etiqueta de idioma le da uno propio

## [sue_paint_colors]

sue_paint_colors-void = Fondo
    .description = Alrededor de la imagen
sue_paint_colors-panel = Panel
    .description = Paletas y menús
sue_paint_colors-pill = Píldora
    .description = Campos, fichas y tarjetas
sue_paint_colors-hover = Al pasar
    .description = Controles bajo el puntero
sue_paint_colors-active = Activo
    .description = Controles pulsados y elegidos
sue_paint_colors-border = Borde
    .description = Bordes finos
sue_paint_colors-border_active = Borde activo
    .description = Bordes de los controles elegidos y con foco
sue_paint_colors-text = Texto
    .description = Texto principal e iconos activos
sue_paint_colors-muted = Atenuado
    .description = Texto secundario
sue_paint_colors-selection = Selección
    .description = Resaltado de la selección de texto
sue_paint_colors-selection_text = Texto seleccionado
    .description = Texto dentro de una selección
sue_paint_colors-grid = Cuadrícula
    .description = Cuadrículas y reglas
sue_paint_colors-shadow = Sombra
    .description = Sombra bajo las superficies flotantes

## [sue_writer]

sue_writer-theme = Tema
    .description = Los colores de Sue Writer: el tema de Sue (lo sigue) o un preajuste
    .sue-theme = Tema de Sue
    .gray = Gris
    .void = Vacío
    .graphite = Grafito
    .paper = Papel
sue_writer-smart_quotes = Comillas tipográficas
    .description = Convertir las comillas rectas en curvas al escribir
sue_writer-dark_paper = Papel oscuro
    .description = Mostrar la página con los colores oscuros del tema en lugar de papel blanco
sue_writer-language = Idioma
    .description = Idioma: sue sigue el idioma de Sue; una etiqueta de idioma le da uno propio

## [image_viewer] y [convert]

image_viewer-language = Idioma
    .description = Idioma: sue sigue el idioma de Sue; una etiqueta de idioma le da uno propio
convert-language = Idioma
    .description = Idioma: sue sigue el idioma de Sue; una etiqueta de idioma le da uno propio

## [keys]

keys-command_palette = Paleta de comandos
    .description = Abrir la paleta de comandos
keys-command_palette_alt = Paleta de comandos (alternativa)
    .description = Abrir la paleta de comandos (alternativa)
keys-open_settings = Abrir Ajustes
    .description = Abrir Ajustes
keys-open_file = Abrir archivo
    .description = Abrir un archivo
keys-save = Guardar
    .description = Guardar el proyecto
keys-quit = Salir
    .description = Salir de Sue
keys-undo = Deshacer
    .description = Deshacer el último cambio
keys-redo = Rehacer
    .description = Rehacer
keys-redo_alt = Rehacer (alternativa)
    .description = Rehacer (alternativa)
