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
open-at-launch = Abrir { $name } na Inicialização
command-palette = Paleta de Comandos ({ $command_palette })
check-for-updates = Verificar Atualizações
settings = Configurações
quit-sue = Sair do Sue
language-menu = Idioma (Language)
language-system = Idioma do Sistema ({ $language })
language-follow-sue = Seguir o Sue ({ $language })
language-changed = O Sue agora fala { $language }
could-not-change-language = Não foi possível alterar o idioma: { $error }
layout-menu = Layout
layout-automatic = Automático ({ $layout })
layout-desktop = Desktop
layout-tablet = Tablet
layout-phone = Celular
layout-phone-shaped = Formato Celular
layout-changed = O Sue agora usa o layout { $layout }
could-not-change-layout = Não foi possível alterar o layout: { $error }
sue-hub-every-studio-tool-files = Sue: o Hub, todos os estúdios e ferramentas, arquivos, projetos e Configurações
pinned-sue-opens-here-launch-click = Fixado: o Sue abre aqui na inicialização. Clique para desafixar
pin-sue-opens-here-launch = Fixar: o Sue abre aqui na inicialização
sue-opens-hub-launch = O Sue abre no Hub na inicialização
sue-opens-in-at-launch = O Sue abre em { $name } na inicialização
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
click-to-show-it = {"."} Clique para exibir
opened = Aberto
saved = Salvo { $name }{ $note }{ $folder }{ $click }
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
