## The shell (crates/sue-wm).

hub = Hub
studios = Estúdios
tools = Ferramentas
new = Novo
new-file-kind = Novo { $category }: { $label }
menu-file = Arquivo
menu-project = Projeto
menu-send-to = Enviar Para
open-file = Abrir Arquivo
open-file-ellipsis = Abrir Arquivo...
open-project = Abrir Projeto
open-project-ellipsis = Abrir Projeto...
save-project = Salvar Projeto
save-project-as = Salvar Projeto Como
save-project-as-ellipsis = Salvar Projeto Como...
check-for-updates = Verificar Atualizações
settings = Configurações
quit-sue = Sair do Sue
language-system = Idioma do Sistema ({ $language })
language-follow-sue = Seguir o Sue ({ $language })
could-not-change-language = Não foi possível alterar o idioma: { $error }
layout-automatic = Automático ({ $layout })
layout-desktop = Desktop
layout-tablet = Tablet
layout-phone = Celular
layout-phone-shaped = Formato Celular
sue-hub-every-studio-tool-files = Sue: o Hub, todos os estúdios e ferramentas, arquivos, projetos e Configurações
pinned-sue-opens-here-launch-click = Fixado: o Sue abre aqui na inicialização. Clique para desafixar
pin-sue-opens-here-launch = Fixar: o Sue abre aqui na inicialização
could-not-pin-it = Não foi possível fixar: { $error }
type-command-mode-entity-action = Digite um comando, modo ou ação
nothing-matches = Nenhuma correspondência
go-to-the-hub = Ir para o Hub
open = Abrir { $name }
pin-open-it-at-launch = Fixar { $name }: Abrir na Inicialização
unpin-open-hub-launch = Desafixar: Abrir o Hub na Inicialização
open-settings = Abrir Configurações
label-work = Trabalho
transfer-copy = Copiar
transfer-export = Exportar
transfer-import = Importar
recent = Recentes
files = Arquivos
open-recent = Abrir Recentes
saved-projects = Projetos Salvos
untitled = Sem Título
work-sue-keeps-you-before-saved = O trabalho mantido automaticamente antes de salvar aparece aqui para recuperação ou descarte.
unsaved-work = Trabalho Não Salvo
files-projects-you-open-save-show = Arquivos e projetos que você abre ou salva aparecem aqui.
projects-you-save-sue-menu-project = Projetos que você salva (Menu Sue -> Projeto -> Salvar Projeto) aparecem aqui.
ago-just-now = agora mesmo
ago-minutes = { $count ->
    [one] há um minuto
   *[other] há { $count } minutos
}
ago-hours = { $count ->
    [one] há uma hora
   *[other] há { $count } horas
}
ago-yesterday = ontem
ago-days = { $count ->
    [one] há um dia
   *[other] há { $count } dias
}
in-folder = { $lead } em { $folder }
opened = Aberto
saved-2 = Salvo
saved-in-folder = {" "}em { $folder }
recover = Recuperar
discard = Descartar
really-discard = Confirmar Descarte
kept-newer-than = Mantido { $value }, mais recente que { $value2 }
kept-never-saved = Mantido { $value }, nunca salvo
that-is-gone = O item foi removido
entities = { $count ->
    [one] { $count } entidade
   *[other] { $count } entidades
}
curve-pen-pressure = Pressão da caneta
curve-output = Saída
color-wheel-show-triangle = Mostrar o triângulo HLS
color-wheel-show-square = Mostrar o quadrado HSV
window-minimize = Minimizar
window-maximize = Maximizar
window-restore = Restaurar
window-close = Fechar
pick-a-color = Escolher uma cor
undo = Desfazer
nothing-to-undo = Nada para desfazer
redo = Refazer
nothing-to-redo = Nada para refazer
undo-depth = Desfazer { $depth }
undo-failed = Falha ao desfazer: { $error }
idle = Ocioso
fps = { $fps } FPS
close-esc = Fechar (Esc)
none = Nenhum
there-no-studio-tool-called = Nenhum estúdio ou ferramenta chamado { $id }
is-not-available = { $command } não está disponível

## files/: the files browser (copied from paint.ftl, which keeps its own until Sue Paint's start page goes)

start-add-label = Adicionar um rótulo
start-all = Tudo
start-cancel = Cancelar
start-clear-completed = Limpar Transferências Concluídas
start-data-transfers = Transferências de Dados
start-date-modified = Data de modificação
start-delete-label = Excluir o rótulo
start-details = Detalhes
start-discard = Descartar este trabalho não salvo
start-favorite = Favorito
start-kind = Tipo
start-last-worked-on = Última modificação
start-move-chosen-here = Colocar os projetos escolhidos aqui
start-move-to = Mover para
start-name = Nome
start-new-label = Novo rótulo
start-no-projects = Nenhum projeto ainda. Clique em Desenhar para começar; imagens e projetos que você abrir ou salvar aparecerão aqui.
start-no-transfers = Nada foi importado, exportado ou copiado ainda. Arquivos que o Sue Paint abre, salva, exporta e copia aparecem aqui.
start-none-match = Nada aqui corresponde à busca.
start-open = Abrir
start-really-discard = Confirmar descarte
start-really-remove = Confirmar remoção
start-remove-from-list = Remover da lista
start-remove-hint = Os arquivos permanecem onde estão; apenas a lista os esquece
start-rename-label = Renomear o rótulo
start-search-projects = Pesquisar projetos
start-select-all = Selecionar Tudo
start-show-in-folder = Mostrar na pasta
start-sort-direction = Inverter a ordem
start-studio = Estúdio
start-take-off-label = Remover este rótulo
start-thumbnail = Miniatura
start-transfer-cancelled = Cancelado
start-transfer-done = Concluído { $when }
start-transfer-failed = Falhou: { $why }
start-transfer-running = Em execução
start-type = Tipo
start-type-picture = Imagem
start-type-file = Arquivo
start-type-project = Projeto
start-type-unsaved = Trabalho não salvo
start-view-large = Cartões grandes
start-view-list = Lista
start-view-small = Cartões pequenos
start-where = Local
## ui/input.rs, ui/slider.rs

field-clear = Limpar
field-choices = Opções
range-start = Início de { $name }
range-end = Fim de { $name }
## menubar.rs: the menus every studio shares, and notes on menu items

menu-edit = Editar
menu-view = Exibir
menu-window = Janela
menu-help = Ajuda
menu-new = Novo...
menu-open = Abrir...
menu-open-recent = Abrir Recente
menu-close = Fechar
menu-save = Salvar
menu-save-as = Salvar Como...
menu-export = Exportar...
menu-print = Imprimir...
menu-settings = Configurações...
menu-back-to-hub = Voltar ao Hub
menu-quit = Sair do Sue
menu-undo = Desfazer
menu-redo = Refazer
menu-cut = Recortar
menu-copy = Copiar
menu-paste = Colar
menu-select-all = Selecionar Tudo
menu-guide = Guia do { $studio }
menu-keyboard-shortcuts = Atalhos do Teclado
menu-check-for-updates = Verificar Atualizações
menu-about = Sobre o { $studio }
menu-note-coming-later = Disponível em breve
menu-note-needs-selection = Requer uma seleção
## ui/dialog.rs, ui/toast.rs

ok = OK
cancel = Cancelar
dialog-close = Fechar
toast-dismiss = Dispensar
notices = Avisos
