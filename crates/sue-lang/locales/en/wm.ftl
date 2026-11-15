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
check-for-updates = Check for Updates
settings = Settings
quit-sue = Quit Sue
# The Sue menu's language submenu. Other languages add the English word in
# brackets, so someone who chose a language by mistake finds the way back.
language-menu = Language
language-system = System ({ $language })
language-follow-sue = Follow Sue ({ $language })
language-changed = Sue speaks { $language }
could-not-change-language = Could not change the language: { $error }
# The Sue menu's layout submenu: which layout the whole app uses.
layout-menu = Layout
layout-automatic = Automatic ({ $layout })
layout-desktop = Desktop
layout-tablet = Tablet
layout-phone = Phone
layout-phone-shaped = Phone Shaped
layout-changed = Sue uses the { $layout } layout
could-not-change-layout = Could not change the layout: { $error }

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
color-wheel-show-triangle = Show the HLS triangle
color-wheel-show-square = Show the HSV square
window-minimize = Minimize
window-maximize = Maximize
window-restore = Restore Down
window-close = Close

## shell.rs

in-folder = { $lead } in { $folder }
saved-in-folder = {" "}in { $folder }
click-to-show-it = {"."} Click to show it
saved = Saved { $name }{ $note }{ $folder }{ $click }
open-recent = Open Recent
recent = Recent
files-projects-you-open-save-show = Files and projects you open or save show up here.
saved-projects = Saved Projects
projects-you-save-sue-menu-project = Projects you save (Sue menu, Project, Save Project) show up here.
unsaved-work = Unsaved Work
work-sue-keeps-you-before-saved = Work Sue keeps for you before it is saved shows up here, to recover or discard.
saved-2 = Saved
opened = Opened
kept-newer-than = Kept { $value }, newer than { $value2 }
kept-never-saved = Kept { $value }, never saved
really-discard = Really Discard
discard = Discard
recover = Recover

## How long ago (the Hub's lists)

ago-just-now = just now
ago-yesterday = yesterday
ago-minutes = { $count ->
    [one] a minute ago
   *[other] { $count } minutes ago
}
ago-hours = { $count ->
    [one] an hour ago
   *[other] { $count } hours ago
}
ago-days = { $count ->
    [one] a day ago
   *[other] { $count } days ago
}
untitled = Untitled

## work.rs: labels

label-work = Work

## transfers.rs

transfer-import = Import
transfer-export = Export
transfer-copy = Copy
unsaved-save-it = { $name } has been changed. Do you want to save it?
do-not-save = Do Not Save
save = Save
cancel = Cancel
shortcuts-custom = Keys of commands that have no default key

## files/: the files browser (copied from paint.ftl, which keeps its own until Sue Paint's start page goes)

start-add-label = Add a label
start-all = All
start-cancel = Cancel
start-clear-completed = Clear Completed Transfers
start-data-transfers = Data Transfers
start-date-modified = Date modified
start-delete-label = Delete the label
start-details = Details
start-discard = Discard this unsaved work
start-favorite = Favorite
start-kind = Kind
start-last-worked-on = Last worked on
start-move-chosen-here = Put the chosen projects here
start-move-to = Move to
start-name = Name
start-new-label = New label
start-no-projects = No projects yet. Click Draw to start one; pictures and projects you open or save show here.
start-no-transfers = Nothing has been brought in, written out or copied yet. Files Sue Paint opens, saves, exports and copies show here.
start-none-match = Nothing here matches.
start-open = Open
start-really-discard = Really discard
start-really-remove = Really remove
start-remove-from-list = Remove from list
start-remove-hint = The files stay where they are; only the list forgets them
start-rename-label = Rename the label
start-search-projects = Search projects
start-select-all = Select All
start-show-in-folder = Show in folder
start-sort-direction = Reverse the order
start-studio = Studio
start-take-off-label = Take off this label
start-thumbnail = Thumbnail
start-transfer-cancelled = Cancelled
start-transfer-done = Done { $when }
start-transfer-failed = Failed: { $why }
start-transfer-running = Running
start-type = Type
start-type-picture = Picture
start-type-file = File
start-type-project = Project
start-type-unsaved = Unsaved work
start-view-large = Large cards
start-view-list = List
start-view-small = Small cards
start-where = Where
## ui/input.rs, ui/slider.rs

field-clear = Clear
field-choices = Choices
range-start = Start of { $name }
range-end = End of { $name }
## menubar.rs: the menus every studio shares, and notes on menu items

menu-edit = Edit
menu-view = View
menu-window = Window
menu-help = Help
menu-new = New...
menu-open = Open...
menu-open-recent = Open Recent
menu-close = Close
menu-save = Save
menu-save-as = Save As...
menu-export = Export...
menu-print = Print...
menu-settings = Settings...
menu-back-to-hub = Back to the Hub
menu-quit = Quit Sue
menu-undo = Undo
menu-redo = Redo
menu-cut = Cut
menu-copy = Copy
menu-paste = Paste
menu-select-all = Select All
menu-guide = { $studio } Guide
menu-keyboard-shortcuts = Keyboard Shortcuts
menu-check-for-updates = Check for Updates
menu-about = About { $studio }
menu-note-coming-later = Coming later
menu-note-needs-selection = Needs a selection
## ui/dialog.rs, ui/toast.rs

ok = OK
dialog-close = Close
toast-dismiss = Dismiss
notices = Notices

## Manage Fonts: the shared font library as a list
search-fonts = Search fonts
finding-installed-fonts = Finding the fonts installed on this device...
font-families-count = { $count ->
    [one] { $count } font family
   *[other] { $count } font families
}
font-sample = The quick brown fox jumps over the lazy dog 0123456789
font-bundled = Comes with Sue
font-faces = Faces: { $faces }
fonts-come-from-the-device = Sue sets text in the fonts installed on this device. A font installed in the system shows here and in every studio.
## shell/hub.rs, shell/hub/*.rs, shell/bar.rs: the Hub's places

hub-home = Home
hub-files = Files
hub-welcome = Welcome to Sue
hub-new = New
hub-new-hint = Start something new in any studio
hub-open = Open
hub-open-hint = Open a file from your computer
hub-jump-back-in = Jump Back In
hub-all-files = All Files
hub-your-work = Your Work
hub-projects = Projects
hub-unsaved-work = Unsaved Work
hub-favorites = Favorites
hub-folder = Folder
hub-folder-detail = Sue's folder, in your file manager
hub-count = { $count ->
    [0] Nothing yet
    [one] { $count } item
   *[other] { $count } items
}
hub-to-recover = { $count ->
    [one] { $count } to recover
   *[other] { $count } to recover
}
hub-file-gone = { $path } is gone; it is taken off the list
hub-upkeep = More
hub-rebuild-thumbnails = Rebuild Thumbnails
hub-thumbnails-rebuilt = The thumbnails are drawn again
hub-forget-missing = Forget Files That Are Gone
hub-forgot = { $count ->
    [0] Every file on the list is still there
    [one] One file that is gone was taken off the list
   *[other] { $count } files that are gone were taken off the list
}
hub-search = Search or run a command
hub-search-files = Search files
hub-search-hint = Find a command, a setting or a studio (Tab)
files-transfers = Transfers
