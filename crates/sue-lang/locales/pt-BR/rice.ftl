## Configuracoes do Sue (the rice). Rascunho: aguardando revisao de falante nativo.

## [general]

general-confirm_on_exit = Confirmar ao sair
    .description = Perguntar antes de fechar a janela
general-language = Idioma
    .description = O idioma do Sue, seguido por todos os estudios e ferramentas salvo configuracao propria (sistema segue o idioma do sistema)
general-autosave_seconds = Salvamento automatico a cada
    .description = Salvar o trabalho nao salvo com esta frequencia para que o Hub possa recupera-lo (s; 0 desativa)

## [launchpad]

launchpad-recent_count = Arquivos recentes no Hub
    .description = Quantos arquivos e projetos recentes o Hub lista (0 oculta a lista)
launchpad-pinned = Abrir ao iniciar
    .description = O estudio ou ferramenta em que o Sue abre diretamente (o alfinete no Hub; vazio abre o Hub)

## [profile]

profile-hardware = Preferencia de desempenho
    .description = Ritmo de quadros e consumo de efeitos
    .auto = Automatico
    .high-refresh = Alta taxa de atualizacao
    .balanced = Equilibrado
    .legacy-low-spec = Dispositivo antigo
    .mobile-battery-saver = Economia de bateria

## [display]

display-frame_rate_cap = Limite de taxa de quadros
    .description = Taxa maxima de quadros durante animacoes (0 segue a preferencia de desempenho)
display-ui_scale = Escala da interface
    .description = Escala da interface sobre a escala do sistema
display-interface = Interface
    .description = Disposicao: automatico segue o dispositivo (layouts de toque em tablets e celulares); escolha para alternar
    .auto = Automatico
    .desktop = Computador
    .tablet = Tablet
    .phone = Celular
display-phone_frame = Formato de celular
    .description = No computador, exibe a interface de celular em uma coluna central com largura de telefone
display-gpu_preference = Preferencia de GPU
    .description = GPU que desenha a janela; automatico usa a GPU da tela (aplica apos reiniciar)
    .auto = Automatico
    .integrated = Integrada
    .discrete = Dedicada

## [motion]

motion-instant_transitions = Transicoes instantaneas
    .description = Muda paginas, menus e janelas em 0 ms sem animacoes
motion-animation_speed = Velocidade da animacao
    .description = Velocidade de todas as animacoes de mola
motion-spring_stiffness = Rigidez da mola
    .description = Constante elastica k de cada animacao
motion-spring_damping = Amortecimento da mola
    .description = Coeficiente de amortecimento c de cada animacao
motion-settle_grace_ms = Tolerancia de estabilizacao (ms)
    .description = Continua desenhando por este tempo apos a ultima mudanca (ms)
motion-caret_blink_ms = Piscar do cursor (ms)
    .description = Intervalo de intermitencia do cursor de texto (0 mantem solido)
motion-smooth_caret = Cursor suave
    .description = Faz o cursor de texto deslizar suavemente ate a nova posicao em vez de pular
motion-caret_glide_ms = Tempo de deslizamento do cursor
    .description = Quanto tempo o cursor leva para deslizar ate o novo local (ms; menor e mais rapido)
motion-message_seconds = Duracao das mensagens
    .description = Quanto tempo as mensagens permanecem na barra de status (s)

## [layout]

layout-padding = Espacamento interno
    .description = Espaco dentro de barras, botoes e controles (px)
layout-border_width = Espessura da borda
    .description = Espessura das bordas finas (px)
layout-control_radius = Raio dos controles
    .description = Arredondamento dos cantos de botoes e campos (px)
layout-pill_radius = Raio dos botoes em capsula
    .description = Arredondamento dos cantos de janelas, menus e botoes em capsula (px)
layout-pill_height = Altura dos botoes em capsula
    .description = Altura dos botoes em capsula (px)
layout-touch_target = Altura do alvo de toque
    .description = Altura minima dos controles de toque (px)
layout-sidebar_width = Largura da barra lateral
    .description = Largura padrao da barra lateral (px)
layout-panel_header_height = Altura do cabecalho do painel
    .description = Altura da barra superior do painel (px)
layout-shadow_radius = Raio da sombra
    .description = Raio de suavizacao da sombra sob superficies flutuantes (px)
layout-status_bar = Barra de status
    .description = Exibe a barra de status fina na parte inferior da janela (sem ela as mensagens surgem como aviso)

## [colors]

## [typography]

typography-ui_font = Fonte da interface
    .description = Fonte integrada da interface
typography-ui_size = Tamanho da interface
    .description = Tamanho do texto da interface (px)
typography-small_size = Tamanho do texto pequeno
    .description = Tamanho do texto de legendas e barra de status (px)
typography-heading_size = Tamanho dos titulos
    .description = Tamanho do texto dos titulos (px)
typography-mono_size = Tamanho monoespacado
    .description = Tamanho do texto monoespacado (px)

## [input]

## [io]

## [audio]

audio-buffer_frames = Quadros do buffer de audio
    .description = Amostras enviadas a placa de som por vez (0 usa o padrao da placa)
audio-sample_rate = Taxa de amostragem de audio
    .description = Taxa interna de processamento de som (Hz; 0 segue a placa)
audio-channels = Canais de audio
    .description = Quantos canais o motor de audio processa (0 segue a placa)
audio-exclusive = Modo de audio exclusivo
    .description = Assumir controle exclusivo do dispositivo de audio para menor latencia (Windows WASAPI)
audio-master_volume = Volume principal
    .description = Volume geral de saida do Sue (%)

## [packages]

## [updates]

updates-check_automatically = Verificar automaticamente
    .description = Procurar uma versao mais recente do Sue ao iniciar, no maximo uma vez ao dia

## [sue_paint]

sue_paint-width = Largura
    .description = Largura de novas ilustracoes (px)
sue_paint-height = Altura
    .description = Altura de novas ilustracoes (px)
sue_paint-dpi = Resolucao
    .description = Resolucao de novas ilustracoes (dpi)
sue_paint-materials_dir = Pasta de materiais
    .description = Onde o Sue Paint guarda materiais e pontas de pinceis (vazio: pasta propria)
sue_paint-save_format = Formato de salvamento
    .description = Como salvar uma imagem: arquivo .clip do Clip Studio Paint ou projeto Sue
    .clip = Clip Studio Paint (.clip)
    .sue = Projeto Sue (.sue)
sue_paint-touch_mode = Modo de toque
    .description = Sue Paint em celulares e tablets: Modo Simples ou Modo Estudio
    .simple = Simples
    .studio = Estudio
sue_paint-custom_key_1 = Tecla personalizada 1
    .description = Comando que a tecla T1 do Modo Estudio executa
sue_paint-custom_key_2 = Tecla personalizada 2
    .description = Comando que a tecla T2 do Modo Estudio executa
sue_paint-custom_key_3 = Tecla personalizada 3
    .description = Comando que a tecla T3 do Modo Estudio executa
sue_paint-custom_key_4 = Tecla personalizada 4
    .description = Comando que a tecla T4 do Modo Estudio executa
sue_paint-custom_key_5 = Tecla personalizada 5
    .description = Comando que a tecla T5 do Modo Estudio executa
sue_paint-custom_key_6 = Tecla personalizada 6
    .description = Comando que a tecla T6 do Modo Estudio executa
sue_paint-custom_key_7 = Tecla personalizada 7
    .description = Comando que a tecla T7 do Modo Estudio executa
sue_paint-theme = Tema
    .description = Cores do Sue Paint: Cinza, tema do Sue (segue), predefinicao ou Personalizado
    .gray = Cinza
    .sue-theme = Tema do Sue
    .void = Vazio
    .graphite = Grafite
    .paper = Papel
    .custom = Personalizado
sue_paint-pen_pressure = Pressao da caneta
    .description = Curva de pressao da caneta para cada ferramenta (Arquivo, Ajustar nivel de pressao da caneta)
sue_paint-shift_line = Shift para linhas retas
    .description = Shift e arrastar desenha uma linha reta enquanto e tracada (Preferencias, Ferramentas)
sue_paint-two_finger_tap = Toque com dois dedos
    .description = O que um toque rapido de dois dedos na tela faz (Preferencias, Gestos de toque)
    .undo = Desfazer
    .none = Nada
sue_paint-three_finger_tap = Toque com tres dedos
    .description = O que um toque rapido de tres dedos na tela faz (Preferencias, Gestos de toque)
    .redo = Refazer
    .none = Nada
sue_paint-brush_cursor = Cursor do pincel
    .description = O ponteiro sobre a tela ao pintar: contorno do pincel, cruz ou ponto (Preferencias, Cursor)
    .brush = Formato do pincel
    .crosshair = Cruz
    .dot = Ponto
sue_paint-selection_layer_opacity = Opacidade das camadas de selecao
    .description = Opacidade de uma nova camada de selecao (%; Preferencias, Camada/Quadro)
sue_paint-rotate_step = Passo de rotacao
    .description = Quanto Girar para a esquerda e Girar para a direita giram a vista (graus; Preferencias, Tela)
sue_paint-alt_tool = Ferramenta Alt
    .description = O que Alt faz ao desenhar na tela (Arquivo, Configuracoes de teclas modificadoras)
    .eyedropper = Conta-gotas
    .eraser = Borracha
    .none = Nenhum
sue_paint-space_tool = Ferramenta Espaco
    .description = O que a barra de espaco faz na tela (Arquivo, Configuracoes de teclas modificadoras)
    .hand = Mao
    .none = Nenhum
sue_paint-ctrl_shift = Ctrl+Shift
    .description = O que Ctrl+Shift clique faz na tela (Arquivo, Configuracoes de teclas modificadoras)
    .pick_layer = Selecionar camada
    .none = Nenhum
sue_paint-language = Idioma
    .description = Idioma: sue segue o idioma do Sue, ou defina um dedicado

## [sue_paint_colors]

sue_paint_colors-void = Vazio
    .description = Ao redor da imagem
sue_paint_colors-panel = Painel
    .description = Paletas e menus
sue_paint_colors-pill = Capsula
    .description = Campos, fichas e cartoes
sue_paint_colors-hover = Ao passar o mouse
    .description = Controles sob o cursor
sue_paint_colors-active = Ativo
    .description = Controles pressionados e selecionados
sue_paint_colors-border = Borda
    .description = Linhas divisórias finas
sue_paint_colors-border_active = Borda ativa
    .description = Bordas de controles escolhidos e em foco
sue_paint_colors-text = Texto
    .description = Texto principal e icones ativos
sue_paint_colors-muted = Atenuado
    .description = Texto secundario
sue_paint_colors-selection = Selecao
    .description = Realce de selecao de texto
sue_paint_colors-selection_text = Texto selecionado
    .description = Texto dentro de uma selecao
sue_paint_colors-grid = Grade
    .description = Grades e reguas
sue_paint_colors-shadow = Sombra
    .description = Sombra sob superficies flutuantes

## [sue_writer]

sue_writer-theme = Tema
    .description = Cores do Sue Writer: tema do Sue (segue) ou uma predefinicao
    .sue-theme = Tema do Sue
    .gray = Cinza
    .void = Vazio
    .graphite = Grafite
    .paper = Papel
sue_writer-smart_quotes = Aspas inteligentes
    .description = Converter aspas retas em curvas ao digitar
sue_writer-dark_paper = Papel escuro
    .description = Mostrar a pagina nas cores escuras do tema em vez de papel branco
sue_writer-spell_check = Verificacao ortografica
    .description = Sublinhar palavras incorretas ao digitar
sue_writer-language = Idioma
    .description = Idioma: sue segue o idioma do Sue, ou defina um dedicado

## [image_viewer]

image_viewer-language = Idioma
    .description = Idioma: sue segue o idioma do Sue, ou defina um dedicado

## [convert]

convert-language = Idioma
    .description = Idioma: sue segue o idioma do Sue, ou defina um dedicado

## [keys]

keys-command_palette = Paleta de comandos
    .description = Abrir a paleta de comandos
keys-command_palette_alt = Paleta de comandos alt
    .description = Abrir a paleta de comandos (alternativo)
keys-open_settings = Abrir configuracoes
    .description = Abrir Configuracoes
keys-open_file = Abrir arquivo
    .description = Abrir um arquivo
keys-save = Salvar
    .description = Salvar o projeto
keys-quit = Sair
    .description = Sair do Sue
keys-undo = Desfazer
    .description = Desfazer a ultima alteracao
keys-redo = Refazer
    .description = Refazer
keys-redo_alt = Refazer alt
    .description = Refazer (alternativo)

## [sue_paint_keys]

sue_paint_keys-help_shortcuts = Ajuda atalhos
    .description = Configuracoes de atalhos de teclado
sue_paint_keys-file_preferences = Arquivo preferencias
    .description = Preferencias
sue_paint_keys-file_save_as = Arquivo salvar como
    .description = Salvar como
sue_paint_keys-file_print = Arquivo imprimir
    .description = Imprimir
sue_paint_keys-layer_new = Camada nova
    .description = Camada raster
sue_paint_keys-file_new = Arquivo novo
    .description = Novo
sue_paint_keys-file_close = Arquivo fechar
    .description = Fechar
sue_paint_keys-edit_transform_free = Editar transformacao livre
    .description = Transformacao livre
sue_paint_keys-edit_transform_scale = Editar transformar escala
    .description = Escalar/Girar
sue_paint_keys-filter_hue = Filtro matiz
    .description = Matiz/Saturacao/Luminosidade
sue_paint_keys-select_invert = Selecao inverter
    .description = Inverter area selecionada
sue_paint_keys-filter_invert = Filtro inverter
    .description = Inverter degradê
sue_paint_keys-edit_cut = Editar recortar
    .description = Recortar
sue_paint_keys-edit_copy = Editar copiar
    .description = Copiar
sue_paint_keys-edit_paste = Editar colar
    .description = Colar
sue_paint_keys-edit_clear = Editar limpar
    .description = Excluir
sue_paint_keys-edit_fill = Editar preencher
    .description = Preencher
sue_paint_keys-layer_toggle_clip = Camada alternar recorte
    .description = Recortar na camada abaixo
sue_paint_keys-layer_folder_insert = Camada inserir pasta
    .description = Criar pasta e inserir camada
sue_paint_keys-layer_merge_visible_here = Camada mesclar visiveis aqui
    .description = Mesclar camadas visiveis
sue_paint_keys-layer_ungroup = Camada desagrupar
    .description = Desagrupar pasta de camadas
sue_paint_keys-layer_merge_selected = Camada mesclar selecionadas
    .description = Mesclar camadas selecionadas
sue_paint_keys-layer_merge_down = Camada mesclar abaixo
    .description = Mesclar com camada abaixo
sue_paint_keys-layer_select_above = Camada selecionar acima
    .description = Selecionar camada acima
sue_paint_keys-layer_select_below = Camada selecionar abaixo
    .description = Selecionar camada abaixo
sue_paint_keys-layer_order_up = Camada mover para cima
    .description = Mover para cima
sue_paint_keys-layer_order_down = Camada mover para baixo
    .description = Mover para baixo
sue_paint_keys-select_again = Selecao selecionar novamente
    .description = Selecionar novamente
sue_paint_keys-select_all = Selecao selecionar tudo
    .description = Selecionar tudo
sue_paint_keys-select_none = Selecao desmarcar
    .description = Desmarcar selecao
sue_paint_keys-view_actual = Exibir 100%
    .description = 100%
sue_paint_keys-view_fit = Exibir ajustar
    .description = Ajustar a tela
sue_paint_keys-view_rotate_reset = Exibir redefinir rotacao
    .description = Redefinir rotacao
sue_paint_keys-view_zoom_in = Exibir ampliar
    .description = Ampliar
sue_paint_keys-view_zoom_out = Exibir reduzir
    .description = Reduzir
sue_paint_keys-view_ruler_bar = Exibir barra de reguas
    .description = Barra de reguas
sue_paint_keys-view_snap_ruler = Exibir alinhar a regua
    .description = Alinhar a regua
sue_paint_keys-view_snap_special = Exibir alinhar a regua especial
    .description = Alinhar a regua especial
sue_paint_keys-view_snap_grid = Exibir alinhar a grade
    .description = Alinhar a grade
sue_paint_keys-view_rotate_left = Exibir girar para a esquerda
    .description = Girar para a esquerda
sue_paint_keys-view_rotate_right = Exibir girar para a direita
    .description = Girar para a direita
sue_paint_keys-window_prev_tab = Janela tela anterior
    .description = Tela anterior
sue_paint_keys-window_next_tab = Janela proxima tela
    .description = Proxima tela
sue_paint_keys-window_hide_menus = Janela ocultar menus
    .description = Ocultar barra de menus
sue_paint_keys-window_hide_all = Janela ocultar tudo
    .description = Ocultar todas as paletas
sue_paint_keys-tool_zoom = Ferramenta zoom
    .description = Ferramenta Zoom
sue_paint_keys-tool_hand = Ferramenta mao
    .description = Ferramenta Mao
sue_paint_keys-tool_rotate = Ferramenta girar
    .description = Ferramenta Girar
sue_paint_keys-tool_operation = Ferramenta operacao
    .description = Ferramenta Operacao
sue_paint_keys-tool_move_layer = Ferramenta mover camada
    .description = Ferramenta Mover camada
sue_paint_keys-tool_selection = Ferramenta selecao
    .description = Ferramenta Selecao
sue_paint_keys-tool_auto_select = Ferramenta selecao automatica
    .description = Ferramenta Selecao automatica
sue_paint_keys-tool_eyedropper = Ferramenta conta-gotas
    .description = Ferramenta Conta-gotas
sue_paint_keys-tool_pen = Ferramenta caneta
    .description = Ferramentas Caneta e Lapis
sue_paint_keys-tool_brush = Ferramenta pincel
    .description = Ferramentas Pincel, Aerografo e Decoracao
sue_paint_keys-tool_eraser = Ferramenta borracha
    .description = Ferramenta Borracha
sue_paint_keys-tool_blend = Ferramenta mistura
    .description = Ferramenta Mistura de cores
sue_paint_keys-tool_fill = Ferramenta preenchimento
    .description = Ferramentas Preenchimento e Degradê
sue_paint_keys-tool_figure = Ferramenta figura
    .description = Ferramentas Figura, Borda do quadro e Regua
sue_paint_keys-tool_text = Ferramenta texto
    .description = Ferramentas Texto e Balao
sue_paint_keys-tool_correct_line = Ferramenta corrigir linha
    .description = Ferramenta Corrigir linha
sue_paint_keys-tool_previous_sub = Ferramenta subferramenta anterior
    .description = Subferramenta anterior
sue_paint_keys-tool_next_sub = Ferramenta proxima subferramenta
    .description = Proxima subferramenta
sue_paint_keys-color_swap = Alternar cor
    .description = Alternar cor principal e secundaria
sue_paint_keys-color_transparent = Cor transparente
    .description = Cor de desenho ou transparente
sue_paint_keys-brush_smaller = Pincel menor
    .description = Reduzir tamanho do pincel
sue_paint_keys-brush_bigger = Pincel maior
    .description = Aumentar tamanho do pincel

## [sue_writer_keys]

sue_writer_keys-binder_new_folder = Fichario nova pasta
    .description = Nova pasta
sue_writer_keys-binder_new_doc = Fichario novo doc
    .description = Novo documento
sue_writer_keys-compile_open = Compilar abrir
    .description = Compilar
sue_writer_keys-compile_pdf = Compilar PDF
    .description = PDF
sue_writer_keys-format_bold = Formatar negrito
    .description = Negrito
sue_writer_keys-format_italic = Formatar italico
    .description = Italico
sue_writer_keys-format_underline = Formatar sublinhado
    .description = Sublinhado
sue_writer_keys-format_strike = Formatar tachado
    .description = Tachado
sue_writer_keys-project_split = Projeto dividir
    .description = Dividir na selecao
sue_writer_keys-format_superscript = Formatar sobrescrito
    .description = Sobrescrito
sue_writer_keys-format_subscript = Formatar subscrito
    .description = Subscrito
sue_writer_keys-format_clear = Formatar limpar
    .description = Limpar formatacao direta
sue_writer_keys-align_center = Alinhar centro
    .description = Centralizar
sue_writer_keys-align_left = Alinhar esquerda
    .description = Esquerda
sue_writer_keys-align_right = Alinhar direita
    .description = Direita
sue_writer_keys-align_justify = Alinhar justificar
    .description = Justificar
sue_writer_keys-style_heading1 = Estilo titulo 1
    .description = Titulo 1
sue_writer_keys-style_heading2 = Estilo titulo 2
    .description = Titulo 2
sue_writer_keys-style_heading3 = Estilo titulo 3
    .description = Titulo 3
sue_writer_keys-style_body = Estilo corpo
    .description = Texto do corpo
sue_writer_keys-insert_footnote = Inserir nota de rodape
    .description = Nota de rodape
sue_writer_keys-insert_page_break = Inserir quebra de pagina
    .description = Quebra de pagina
sue_writer_keys-insert_link = Inserir link
    .description = Link
sue_writer_keys-edit_find = Editar localizar
    .description = Localizar
sue_writer_keys-edit_replace = Editar substituir
    .description = Substituir
sue_writer_keys-edit_find_previous = Editar localizar anterior
    .description = Localizar anterior
sue_writer_keys-edit_find_next = Editar localizar proximo
    .description = Localizar proximo
sue_writer_keys-view_composition = Exibir composicao
    .description = Modo de composicao
sue_writer_keys-view_document = Exibir documento
    .description = Texto
sue_writer_keys-view_corkboard = Exibir mural
    .description = Mural
sue_writer_keys-view_outliner = Exibir topicos
    .description = Topicos
sue_writer_keys-project_snapshot = Projeto captura
    .description = Criar instantaneo
sue_writer_keys-list_bullet = Lista de marcadores
    .description = Lista de marcadores
sue_writer_keys-review_comment = Revisao comentario
    .description = Novo comentario
sue_writer_keys-insert_endnote = Inserir nota de fim
    .description = Nota de fim
sue_writer_keys-review_track = Revisao controlar alteracoes
    .description = Registrar alteracoes
sue_writer_keys-review_next_misspelling = Revisao proximo erro
    .description = Proximo erro ortografico
sue_writer_keys-format_grow = Formatar aumentar fonte
    .description = Aumentar fonte
sue_writer_keys-format_shrink = Formatar diminuir fonte
    .description = Diminuir fonte
sue_writer_keys-insert_mark_nbsp = Inserir espaco inseparavel
    .description = Espaco inseparavel

## [image_viewer_keys]

image_viewer_keys-viewer_next = Visualizador proxima
    .description = Proxima imagem
image_viewer_keys-viewer_previous = Visualizador anterior
    .description = Imagem anterior
image_viewer_keys-viewer_fit = Visualizador ajustar
    .description = Ajustar a janela
image_viewer_keys-viewer_actual = Visualizador tamanho real
    .description = Tamanho real
image_viewer_keys-viewer_zoom_in = Visualizador ampliar
    .description = Ampliar
image_viewer_keys-viewer_zoom_out = Visualizador reduzir
    .description = Reduzir
image_viewer_keys-viewer_rotate_right = Visualizador girar para a direita
    .description = Girar para a direita
image_viewer_keys-viewer_rotate_left = Visualizador girar para a esquerda
    .description = Girar para a esquerda
image_viewer_keys-viewer_crop = Visualizador recortar
    .description = Recortar
image_viewer_keys-viewer_save_as = Visualizador salvar como
    .description = Salvar como


layout-bar_height = Altura da barra
    .description = Altura da barra superior com o botao Sue e os menus (px; layouts de toque tornam mais alta)

layout-status_bar_height = Altura da barra de status
    .description = Altura da barra de status (px)

layout-settings_sidebar_width = Larghezza lateral de Configuracoes
    .description = Largura da lista de categorias em Configuracoes (px)

layout-shadow_blur = Desfoque de sombra
    .description = Desfoque da sombra sob menus e janelas (px)

colors-grid = Grade
    .description = Linhas de grade em editores de curvas

input-wheel_line_px = Linha da roda (px)
    .description = Pixels rolados por entalhe da roda do mouse em listas

io-jxl_effort = Esforco JPEG XL
    .description = Esforco de JPEG XL para conversoes (maior e menor e mais lento)

io-jpeg_lossless_transcode = Transcodificacao JPEG sem perdas
    .description = Recomprimir JPEG para JPEG XL bit a bit quando houver codec instalado

io-jpeg_quality = Qualidade JPEG
    .description = Qualidade JPEG para conversoes para .jpg

io-byte_map_width = Largura do mapa de bytes
    .description = Colunas no mapa de bytes de arquivos desconhecidos

io-max_ingest_mib = Ingestao maxima (MiB)
    .description = Maior arquivo que o Sue abre (MiB)

io-convert_dir = Pasta de conversao
    .description = Pasta para arquivos convertidos (vazio salva ao lado da fonte)

audio-output_device = Dispositivo de saida
    .description = Dispositivo de saida (vazio usa o padrao do sistema)

audio-realtime_priority = Prioridade em tempo real
    .description = Solicitar ao SO o agendamento da thread de audio em tempo real

audio-volume = Volume
    .description = Volume de reproducao geral do Sue

packages-require_signatures = Exigir assinaturas
    .description = Aceitar apenas pacotes com assinatura Ed25519 valida
