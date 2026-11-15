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
check-for-updates = Check for Updates
settings = Settings
quit-sue = Quit Sue
command-palette-item = Command Palette
# Sue's language, chosen in Settings.
language-system = System ({ $language })
language-follow-sue = Follow Sue ({ $language })
language-changed = Sue speaks { $language }
could-not-change-language = Could not change the language: { $error }
# The Sue menu's Interface submenu: which layout the whole app uses.
interface-menu = Interface
interface-changed = Sue uses the { $interface } interface
could-not-change-interface = Could not change the interface: { $error }
layout-automatic = Automatic ({ $layout })
layout-desktop = Desktop
layout-tablet = Tablet
layout-phone = Phone
layout-phone-shaped = Phone Shaped

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
saved-file = Saved { $name }{ $note }{ $folder }
show-in-folder = Show in Folder
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
# The button over notices folded behind the newest: it shows them all.
toast-more = { $count } more
# The same button once they show: it folds them again.
toast-fewer = Show fewer
## icons/pack.rs: what an icon pack could not read, and why a pack
## action failed. { $file } is a file of the pack, as it is named there.

icon-pack-not-text = { $file } is not a text file, so it is not an SVG.
icon-pack-too-large = { $file } is too large for an icon ({ $bytes } bytes).
icon-pack-no-svg = { $file } has no svg element.
icon-pack-empty = { $file } draws nothing Sue can read.
icon-pack-path-data = In { $file }, the path data of a { $element } cannot be read from byte { $at } on; what comes before it is drawn.
icon-pack-bad-value = In { $file }, the { $attribute } of a { $element } cannot be read.
icon-pack-not-drawn = In { $file }, the { $element } is not drawn: Sue draws paths and shapes only.
icon-pack-ignored = In { $file }, { $attribute } is not used; the shape is drawn without it.
icon-pack-style-sheet = In { $file }, style sheets and classes are not read; write fill and stroke as attributes.
icon-pack-plain-ink = In { $file }, gradients and patterns are drawn in the one ink of the theme.
icon-pack-no-mask = In { $file }, the mask { $id } is not found; what it would hide is drawn.
icon-pack-too-deep = In { $file }, elements are nested more than { $most } deep; what lies deeper is left out.
icon-pack-too-detailed = { $file } has more than { $most } path segments; the shapes after them are left out.
icon-pack-no-manifest = The pack { $file } has no icon-pack.txt; its folder name stands for its name.
icon-pack-manifest-line = Line { $line } of { $file } is not key = value.
icon-pack-unknown-key = { $file } has a key Sue does not know: { $key }.
icon-pack-no-name = { $file } gives the pack no name.
icon-pack-no-format = { $file } names no format; format 1 is assumed.
icon-pack-newer-format = { $file } is for pack format { $format }, newer than this Sue reads; what Sue can read is used.
icon-pack-unknown-icon = { $file } is named after no icon of Sue ({ $name }).
icon-pack-no-solid-twin = { $file } is not used: the icon { $name } has no filled twin.
icon-pack-no-solid = { $file } replaces { $name } but not its filled twin, so Sue's own filled twin is drawn.
icon-pack-skipped = { $file } is no part of an icon pack and was left out.
icon-pack-too-many-files = The pack { $file } has more files than a pack may hold; the rest were left out.
icon-pack-chosen-missing = The icon pack { $id } is no longer installed, so Sue's own icons are drawn.
icon-pack-bad-id = { $id } cannot name an icon pack.
icon-pack-not-installed = No icon pack { $id } is installed.
icon-pack-not-a-pack = This is not an icon pack: it has no icon-pack.txt and no icon Sue knows.
icon-pack-archive = The archive cannot be read. { $why }
icon-pack-pack-too-large = The icon pack is too large.
icon-pack-icon-no-solid = The icon { $name } has no filled twin to replace.
icon-pack-unusable = The SVG cannot be used. { $why }
icon-pack-io = A file could not be read or written: { $why }

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
# Home while there is no work to jump back into: kinds of work that
# start with one click, then three tiles of what is good to know.
hub-start-new = Start Something New
hub-start-new-tip = A new { $kind } in { $studio }
hub-good-to-know = Good to Know
hub-tip-search = Find Any Command
hub-tip-search-detail = Search at the top, or press { $key }
hub-tip-search-plain = Search at the top of the Hub
hub-tip-open = Open Your Files
hub-tip-open-detail = Drop pictures and documents on Sue to open them
hub-tip-yours = Make Sue Yours
hub-tip-yours-detail = Themes, icon packs, layouts and keys
files-transfers = Transfers

## export.rs: the export settings dialog

export-accept = Export
export-format = Format
export-output-size = Output size
export-size-keep = Keep the size
export-size-scale = Scale
export-size-width = Width
export-size-height = Height
export-size-fit = Fit inside
export-size-exact = Exact size
export-px = {" "}px
export-resampling = Resampling
export-colors = Colors
export-colors-color = Color
export-colors-grays = Grays
export-background = Background
export-fill-transparency = Fill transparency
export-no-transparency = This format holds no transparency, so the background fills it.
export-quality = Quality
export-effort = Effort
export-effort-hint = Higher is smaller and slower
export-written-at = Written at { $width } by { $height } pixels.
export-fit-note = { $format } pictures are at most { $max } by { $max } pixels; bigger ones are fitted into that.
export-bits = Bits
export-bits-16 = 16-bit (CD)
export-bits-24 = 24-bit (studio)
export-bits-float = 32-bit float
export-sample-rate = Sample rate
export-rate-keep = Keep the rate
export-rate-hz = { $rate } Hz
export-rate-cd = { $rate } Hz (CD)
export-channels = Channels
export-channels-keep = Keep the channels
export-channels-mono = Mono
export-channels-stereo = Stereo
export-channels-count = { $count } channels
export-normalize = Normalize the loudest peak to -1 dB

## job_list.rs: the batch list of Batch Convert Pictures

batch-pictures = Pictures
batch-add-files = Add Files
batch-add-folder = Add a Folder
batch-clear = Clear
batch-empty = No pictures yet
batch-empty-detail = Add picture files, or a folder of them, to convert them all at once.
batch-remove = Take it off the list
batch-show-in = Show it in { $manager }
batch-becomes = Waiting: { $format }
batch-converting = Converting...
batch-done = Done: { $value } ({ $note })
batch-failed = Failed: { $why }
batch-written-note = { $width } by { $height } pixels, { $size }
batch-size-bytes = { $size } B
batch-size-kib = { $size } KiB
batch-size-mib = { $size } MiB
batch-save-to = Save to
batch-folder = Folder
batch-next-to-each = Next to each file
batch-one-folder = One folder
batch-chosen-folder = Chosen folder
batch-change = Change
batch-picked-copies = Sue works on copies of files you pick one by one, so it first asks which folder the results go to.
batch-file-name = File name
batch-pattern-hint = {"{"}name{"}"} is the original's name, as in {"{"}name{"}"}-small
batch-if-it-exists = If it exists
batch-keep-both = Keep both
batch-replace = Replace it
batch-reveal-after = Show the result in { $manager } when done
batch-change-every = Change every picture
batch-turn = Turn
batch-turn-none = None
batch-turn-right = 90° right
batch-turn-half = 180°
batch-turn-left = 90° left
batch-mirror = Mirror
batch-left-to-right = Left to right
batch-top-to-bottom = Top to bottom
batch-brightness = Brightness
batch-contrast = Contrast
batch-saturation = Saturation
batch-convert-to = Convert to
batch-close = Close
batch-convert = Convert
batch-convert-count = { $count ->
    [one] Convert 1 Picture
   *[other] Convert { $count } Pictures
}
batch-nothing-converted = Nothing converted: { $failed } failed
batch-converted-one = Converted into { $value }
batch-converted-into = { $count ->
    [one] 1 picture converted into { $value }
   *[other] { $count } pictures converted into { $value }
}
batch-converted-next-to = { $count ->
    [one] 1 picture converted next to its file
   *[other] { $count } pictures converted next to their files
}
batch-and-failed = { $text }, { $failed } failed
