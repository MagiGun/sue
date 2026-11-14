## Settings (bins/sue): its pages, groups, notes, facts and tables. The
## settings themselves are named in rice.ftl.

## engine.rs

general = General
startup = Startup
sue-opens-hub-straight-into-studio = Sue opens on the Hub, or straight into the studio or tool whose pin you set there.
quitting = Quitting
language = Language
each-studio-tool-follows-sues-language = Each studio and tool follows Sue's language, or speaks one of its own.
sue = Sue
language-sue-every-studio-tool-follows = The language of Sue and of every studio and tool that follows it. System uses the system's language when Sue has it.
studios-and-tools = Studios and Tools
theme = Theme
colors = Colors
reload-from-disk = Reload from Disk
export-ellipsis = Export...
open-folder = Open Folder
performance = Performance
graphics-card = Graphics Card
restart-sue = Restart Sue
motion = Motion
output = Output
status = Status
opening-files = Opening Files
trust = Trust
packages = Packages
updates = Updates
check-for-updates = Check for Updates
every-action-can-have-as-many = Every action can have as many shortcuts as you like: + adds one, x takes one away, and clicking one changes it.
the-hub = The Hub
system-default = System default
format = Format
abilities = Abilities
library = Library
lives-in = Lives in
what-it-does = What it does
hub-studios = Hub, Studios
hub-tools = Hub, Tools
window = Window
package = Package
version = Version
signature = Signature
no-packages-are-installed-yet-everything = No packages are installed yet. Everything above is built in.
file = File
active-profile = Active profile
hz-display-fps-animation-target = { $value } ({ $display_refresh_hz } Hz display, { $target } FPS animation target)
gpu = ({ $kind } GPU)
this-machine-has-no-gpu = { $running }. This machine has no { $wanted } GPU
restart-sue-draw-gpu = { $running }. Restart Sue to draw on the { $wanted } GPU
drawing-on = Drawing on
gpus-here = GPUs here
not-probed-in-this-run = Not probed in this run
hz-channels = { $device }, { $sample_rate } Hz, { $channels } channels{ $value }
frames = , { $frames } frames
off = Off
requested-granted-when-playback-starts = Requested; granted when playback starts
granted-by-the-os = Granted by the OS
refused-os-default-priority-applies = Refused by the OS; the default priority applies
unavailable = Unavailable: { $error }
not-running = Not running
closed-opens-when-something-plays = Closed. It opens when something plays.
stream = Stream
real-time-priority = Real-time priority
policy = Policy
unsigned-tampered-packages-are-refused = Unsigned or tampered packages are refused
unsigned-packages-are-accepted-marked-unverified = Unsigned packages are accepted and marked unverified
active-generation = Active generation
n-0-the-factory-install = 0, the factory install
rollback = Rollback
healthy-no-update-has-run-so = Healthy: no update has run, so there is nothing to roll back

## settings.rs

other = Other
press-keys-ellipsis = Press keys...
click-then-press-shortcut-put-here = Click, then press the shortcut to put here
take-this-shortcut-away = Take this shortcut away
none = None
add-another-shortcut-as-many-as = Add another shortcut for this (as many as you like)
filter-settings-ellipsis = Filter settings...
results-for = Results for "{ $filter }"
settings-reset = Reset
settings-reset-hint = Back to how Sue comes
settings-reset-page = Reset This Page
settings-reset-all = Reset All Settings
settings-reset-all-confirm = Click Again to Reset Every Setting
all-settings = All settings

applet-name = Settings
applet-summary = Everything you can change in Sue.

## engine.rs

unsaved-work = Unsaved Work
sue-keeps-work-you-have-not = Sue keeps work you have not saved by itself. The Hub lists it under Unsaved Work, to recover or discard.

## engine.rs: the pages of Settings (0.0.8)

appearance = Appearance
text-and-size = Text and Size
rounded-corners = Rounded Corners
icons = Icons
sizes-and-spacing = Sizes and Spacing
more-text-sizes = More Text Sizes
animation-details = Animation Details
display-and-performance = Display and Performance
interface = Interface
scrolling = Scrolling
sound = Sound
files = Files
saving-and-export = Saving and Export
built-in-formats = Built-in Formats
unknown-files = Unknown Files
keyboard-shortcuts = Keyboard Shortcuts
extensions = Extensions
advanced = Advanced
settings-file = Settings File
studio-or-tool = Studio or tool

## settings/keys.rs: Keyboard Shortcuts

keys-sue-key = Sue's { $name }
keys-group-held-twice = { $group } ({ $count } held twice)
keys-held-twice = { $chord } also runs { $other }; a press runs only one of them.
keys-take-from = Take { $chord } from { $other }
keys-taken-notice = { $chord } is now a key of { $command }, and { $others } holds it too. Take it from one of them below.

## settings/icons.rs: Appearance, Icons

icons-note = Every icon can be drawn by someone else: choose an icon pack, or replace single icons. A pack is a folder of SVG files; what it leaves out stays Sue's own.
icons-sues-own = Sue's own icons
icons-show-sheet = Show All Icons
icons-hide-sheet = Hide the Icons
icons-about = By { $author }, { $licence }, version { $version }. Draws { $count } icons; { $notes } things could not be read.
icons-remove = Remove
icons-remove-confirm = Click Again to Remove
icons-removed = The pack is removed.
icons-install-folder = Install from Folder...
icons-install-archive = Install from Archive...
icons-export = Export Sue's Icons as a Template...
icons-replace-one = Replace one icon
icons-line = Line
icons-solid = Solid
icons-choose-svg = Choose SVG...
icons-give-back = Give Back
icons-installed = Installed { $name }; { $count } things could not be read.
icons-exported = Sue's icons are in { $path }.
icons-replaced = { $name } is replaced; { $count } things could not be read.
icons-failed = That did not work: { $error }
icons-not-read = { $count } things in the icons in use could not be read; Sue's own stand in for them.
