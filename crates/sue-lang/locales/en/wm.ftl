## The shell (crates/sue-wm): the top bar, the Sue menu, the Hub, the
## status bar, the command palette and the shared widgets.

## The Sue menu

hub = Hub
studios = Studios
tools = Tools
new = New
new-file-kind = New { $category }: { $label }
menu-file = File
menu-project = Project
menu-send-to = Send To
open-file = Open File
open-file-ellipsis = Open File...
open-project = Open Project
open-project-ellipsis = Open Project...
save-project = Save Project
save-project-as = Save Project As
save-project-as-ellipsis = Save Project As...
open-at-launch = Open { $name } at Launch
command-palette = Command Palette ({ $command_palette })
settings = Settings
quit-sue = Quit Sue
# The Sue menu's language submenu. Other languages add the English word in
# brackets, so someone who chose a language by mistake finds the way back.
language-menu = Language
language-system = System ({ $language })
language-follow-sue = Follow Sue ({ $language })
language-changed = Sue speaks { $language }
could-not-change-language = Could not change the language: { $error }

## The Hub and the pin

sue-hub-every-studio-tool-files = Sue: the Hub, every studio and tool, files, projects and Settings
pinned-sue-opens-here-launch-click = Pinned: Sue opens here at launch. Click to take the pin away
pin-sue-opens-here-launch = Pin: Sue opens here at launch
sue-opens-hub-launch = Sue opens on the Hub at launch
sue-opens-in-at-launch = Sue opens in { $name } at launch
could-not-pin-it = Could not pin it: { $error }

## The command palette

type-command-mode-entity-action = Type a command, mode or entity action
nothing-matches = Nothing matches
go-to-the-hub = Go to the Hub
open = Open { $name }
pin-open-it-at-launch = Pin { $name }: Open It at Launch
unpin-open-hub-launch = Unpin: Open the Hub at Launch
open-settings = Open Settings
undo = Undo
redo = Redo

## The status bar and messages

saved-click-show = Saved { $name }{ $note } in { $folder }. Click to show it
there-no-studio-tool-called = There is no studio or tool called { $id }
that-is-gone = That is gone
is-not-available = { $command } is not available
nothing-to-redo = Nothing to redo
nothing-to-undo = Nothing to undo
undo-failed = Undo failed: { $error }
entities = { $count ->
    [one] { $count } entity
   *[other] { $count } entities
}
undo-depth = Undo { $depth }
fps = { $fps } FPS
idle = Idle

## Windows, dialogs and widgets

close-esc = Close (Esc)
files = Files
pick-a-color = Pick a color
none = None
curve-pen-pressure = Pen pressure
curve-output = Output
