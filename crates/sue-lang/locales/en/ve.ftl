## Sue VE (crates/sue-ve).

applet-name = Sue VE
applet-summary = Edit video: cut, trim and arrange clips on a timeline, add titles and transitions, mix the sound and export MP4.

## edit.rs

edit-no-sequence = There is no sequence open
edit-no-clip = That clip is no longer there
edit-no-track = That track is no longer there
edit-locked = The track is locked
edit-no-handles = The media has no more frames past that edge
edit-too-short = A clip cannot be shorter than one frame
edit-not-adjacent = The two clips do not meet
edit-sync-conflict = { $track } is sync locked and has a clip in the way
edit-wrong-kind = A video clip goes on a video track and a sound clip on an audio track
edit-nothing = There is nothing to do there

## media

not-media = { $name } is not a movie, a sound or a picture Sue VE takes
not-a-numbered-run = { $name } is not part of a numbered run of pictures
cancelled = Cancelled

## export.rs

target-mp4 = MP4 movie (H.264 and AAC)
target-wav = WAV sound
target-png = PNG frame
target-jpeg = JPEG frame
target-png-sequence = PNG frames, numbered

## studio.rs

page-media = Media
page-edit = Edit
page-effects = Effects
page-color = Color
page-audio = Audio
page-deliver = Deliver
untitled-project = Untitled Project
sequence-name = Sequence { $number }
import-media = Import Media
could-not-import = Could not import { $name }: { $error }
insert = Insert
overwrite = Overwrite
pick-a-source-first = Pick a clip in the media pool or the source monitor first
no-proxies-to-make = Every movie in the pool has a proxy already
proxy-of = Proxy of { $name }
export-note = Exported, { $frames } frames
video = Video
template-hd = HD Video (1920 x 1080, 30 fps)
template-uhd = 4K Video (3840 x 2160, 30 fps)
template-vertical = Vertical Video (1080 x 1920, 30 fps)
template-square = Square Video (1080 x 1080, 30 fps)
title-name = Title
title-text = Your Title
solid-name = Color Solid
adjustment-name = Adjustment Clip
marker-name = Marker { $number }
no-cut-here = There is no cut on the targeted tracks near the playhead
mark-in-and-out-first = Mark an in point and an out point first (I and O)

## commands.rs

cmd-new-project = New Project
cmd-new-sequence = New Sequence...
cmd-open = Open...
cmd-import = Import Media...
cmd-import-image-sequence = Import Image Sequence...
cmd-new-title = New Title
cmd-new-solid = New Color Solid
cmd-new-adjustment = New Adjustment Clip
cmd-make-proxies = Make Proxies
cmd-save = Save
cmd-save-as = Save As...
cmd-export = Export...
cmd-settings = Settings...
cmd-back-to-hub = Back to the Hub
cmd-quit = Quit Sue
cmd-undo = Undo
cmd-redo = Redo
cmd-cut = Cut
cmd-copy = Copy
cmd-paste = Paste
cmd-delete = Delete
cmd-ripple-delete = Ripple Delete
cmd-select-all = Select All
cmd-deselect = Deselect All
cmd-insert = Insert
cmd-overwrite = Overwrite
cmd-enable = Enable
cmd-link = Link
cmd-unlink = Unlink
cmd-freeze-frame = Add Frame Hold
cmd-speed = Speed
cmd-speed-double = Speed 200 %
cmd-speed-half = Speed 50 %
cmd-speed-normal = Normal Speed
cmd-reverse = Reverse
cmd-sound-fades = Fade Sound In and Out
cmd-add-edit = Add Edit
cmd-lift = Lift
cmd-extract = Extract
cmd-add-dissolve = Add Dissolve
cmd-add-dip-black = Add Dip to Black
cmd-add-dip-white = Add Dip to White
cmd-add-crossfade = Add Crossfade
cmd-close-gap = Close Gap
cmd-snapping = Snapping
cmd-linked-selection = Linked Selection
cmd-add-video-track = Add Video Track
cmd-add-audio-track = Add Audio Track
cmd-add-marker = Add Marker
cmd-remove-marker = Remove Marker
cmd-mark-in = Mark In
cmd-mark-out = Mark Out
cmd-mark-clip = Mark Clip
cmd-clear-marks = Clear In and Out
cmd-go-to-in = Go to In
cmd-go-to-out = Go to Out
cmd-play = Play or Pause
cmd-shuttle-back = Play Backward
cmd-stop = Stop
cmd-shuttle-forward = Play Forward
cmd-previous-frame = Previous Frame
cmd-next-frame = Next Frame
cmd-previous-edit = Previous Edit
cmd-next-edit = Next Edit
cmd-go-to-start = Go to Start
cmd-go-to-end = Go to End
cmd-loop = Loop
cmd-source-monitor = Source Monitor
cmd-program-monitor = Program Monitor
cmd-tool-select = Selection Tool
cmd-tool-ripple = Ripple Edit Tool
cmd-tool-roll = Rolling Edit Tool
cmd-tool-slip = Slip Tool
cmd-tool-slide = Slide Tool
cmd-tool-razor = Razor Tool
cmd-zoom-in = Zoom In
cmd-zoom-out = Zoom Out
cmd-zoom-fit = Zoom to Sequence
cmd-quality-full = Preview at Full Size
cmd-quality-half = Preview at Half Size
cmd-quality-quarter = Preview at Quarter Size
cmd-use-proxies = Use Proxies
cmd-page-media = Media Page
cmd-page-edit = Edit Page
cmd-page-effects = Effects Page
cmd-page-color = Color Page
cmd-page-audio = Audio Page
cmd-page-deliver = Deliver Page
cmd-reset-layout = Reset Layout
cmd-keyboard-shortcuts = Keyboard Shortcuts
cmd-check-for-updates = Check for Updates...
cmd-about = About Sue VE

## menus.rs

menu-clip = Clip
menu-sequence = Sequence
menu-markers = Markers
menu-playback = Playback

## ui

panel-pool = Media Pool
panel-effects = Effects
panel-inspector = Inspector
panel-timeline = Timeline
panel-meters = Meters
about-text = Sue VE is Sue's video editor: a media pool, source and program monitors, a timeline with ripple, roll, slip, slide and razor tools, titles, transitions, keyframes, sound mixing and MP4 export, on the same data as every other studio.
field-name = Name
field-width = Width
field-height = Height
field-rate = Frame rate
field-size = Size
field-sample-rate = Sample rate
field-duration = Duration
field-markers = Markers
pool-thumbnails = Thumbnails
pool-list = List
pool-empty = The pool is empty
pool-empty-detail = Import movies, sounds and pictures, or send a picture or an animation here from Sue Paint.
monitor-source = Source
monitor-source-empty = Double click a clip in the pool to see it here
monitor-dropped = { $dropped } of { $shown } frames dropped
timeline-empty = Make a sequence to edit on
track-target = Target
track-lock = Lock
track-hide = Show or hide
track-mute = Mute
track-solo = Solo
track-sync-lock = Sync lock
track-change = Change Track
transition-dissolve = Dissolve
transition-dip-black = Dip to Black
transition-dip-white = Dip to White
transition-crossfade = Crossfade
edit-move = Move
edit-trim = Trim
inspector-enabled = Enabled
inspector-transform = Transform
inspector-sound = Sound
inspector-speed = Speed
inspector-scale = Scale
inspector-x = Position X
inspector-y = Position Y
inspector-rotation = Rotation
inspector-opacity = Opacity
inspector-volume = Volume
inspector-fade-in = Fade in
inspector-fade-out = Fade out
inspector-add-key = Add a keyframe at the playhead
inspector-remove-key = Remove the keyframe at the playhead
inspector-frozen = This clip holds one frame
inspector-title = Title
inspector-size = Size
inspector-pick-a-clip = Select a clip to change its settings
effects-video-transitions = Video transitions
effects-audio-transitions = Sound transitions
effects-later = Effects come in a later version
effects-later-detail = Drag a transition onto a cut, or double click it to put it on the cut nearest the playhead.
start-title = Start a video
start-detail = Make a project, then import movies, sounds and pictures into its pool.
deliver-title = Export
deliver-format = Format
deliver-bitrate = Bit rate
deliver-bitrate-note = A bit rate of 0 lets the quality decide the size of the movie.
deliver-range = Range
deliver-whole = Whole sequence (else in to out)
new-sequence-title = New Sequence
media-facts = About the clip
media-path = File
media-kind = Kind
media-length = Length
deliver-sound = Sound
deliver-with-sound = With sound
deliver-export = Start Export
deliver-done = { $name }: { $frames } frames in { $seconds } s
later-effects = Effects and compositing come in a later version
later-effects-detail = This page will hold effects, masks and keyframed compositing in the manner of After Effects. Transitions are on the Edit page now.
later-color = Color grading comes in a later version
later-color-detail = This page will hold color wheels, curves and scopes. Clips play with their own colors until then.
later-audio = The mixer comes in a later version
later-audio-detail = This page will hold the mixer with its faders and effects. Volume, fades, mute and solo are on the Edit page now.
later-back-to-edit = Back to the Edit page

## sample.rs

sample-project = Sample Video
sample-title = Sue VE
inspector-color = Color
kind-movie = Movie
kind-sound = Sound
kind-still = Picture
kind-picture = Sue Paint picture
kind-image-sequence = Image sequence
kind-title = Title
kind-solid = Color solid
kind-adjustment = Adjustment clip
kind-sequence = Sequence
kind-unknown = Not playable yet

## settings_page.rs

settings-group-playback = Playback
settings-group-media = Media
keys-group-other = Other
dock-again = Dock Again
