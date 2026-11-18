# Sue VE parity

What Sue VE does of what Premiere Pro, DaVinci Resolve, After Effects and
CapCut do, feature by feature. **Done** works and is tested; **partial**
works in part (the notes say which part); **missing** is not there yet.
Stage 1 (0.0.83) built the core and the desktop editor; stage 2 (also
0.0.83) built effects and compositing, the Color and Audio pages and the
phone and tablet layout (`docs/sve-handover.md` says what comes next).

## Premiere Pro

| Feature | Status | Notes |
| --- | --- | --- |
| Project panel with bins, icon and list views | done | thumbnails and a list; File, New Bin (`Ctrl+B`) |
| Import movies, sound, stills, image sequences | done | MP4 and MOV (H.264, HEVC where Windows has it), WAV, FLAC, MP3, AAC, Ogg; still pictures; numbered runs |
| Link or embed media | done | links by default; `sue_ve.embed_media` keeps the bytes in the `.sue` |
| Relink missing media | done | File, Relink Media; the other missing files are found by name in the folder picked |
| Source monitor with in and out, insert and overwrite | done | `I`, `O`, `,` and `.` |
| Program monitor | done | plain frames on the graphics card; frames with effects drawn by the export's renderer and kept in a render cache |
| Timeline with video and audio tracks | done | add tracks; targeting, lock, hide, mute, solo, sync lock |
| Selection, ripple, rolling, rate stretch, slip, slide, razor tools | done | live preview while dragging; one undo step on release |
| Snapping, linked selection | done | |
| Lift, extract, ripple delete, close gap | done | |
| Add edit (Ctrl+K) | done | on targeted tracks, or the selected clips |
| Speed and duration, reverse | done | ripples what follows |
| Frame hold | done | Add Frame Hold from the playhead |
| Time remapping (speed ramps) | done | Effect Controls' Time Remapping; a speed from the playhead; eases in the graph editor |
| J and L cuts | done | trim or roll one side of a linked pair (linked selection off) |
| Video transitions | done | cross dissolve, dips to black and white, wipe, slide, push, cross zoom, blur dissolve |
| Audio transitions: crossfade | done | |
| Effect Controls with keyframes | done | Motion with anchor point and motion blur, Opacity with blend modes, Time Remapping, each effect, masks |
| Keyframe curves (Bezier) editing | done | the graph editor: value and speed graphs, Easy Ease, ease in and out, linear, hold, handles |
| Video effects | done | 19 kinds (see After Effects below); search, presets as files |
| Essential Graphics titles | partial | a title clip with text animators; no styles library, no rolls |
| Shapes | done | rectangle (rounded), ellipse, star, polygon; fill and outline |
| Color solid, adjustment layer | done | adjustment clips apply their effects and grade to what is under them |
| Nested sequences | done | Clip, Nest; a sequence from the pool plays as a clip; a sequence that holds itself stops |
| Markers, in and out marks | done | sequence markers and clip markers (`Shift+M`) |
| Render bar, render in to out | done | red and green on the ruler; `Enter`; paused playback renders ahead; in memory only (no render files) |
| Playback resolution | done | full, half, quarter; frames needing the processor play at half while not rendered |
| Audio meters | done | peak meters with hold; per track on the Audio page |
| Audio track mixer | done | the Audio page |
| Rubber band volume keys | done | drag the line; `Ctrl` and click adds a key |
| Lumetri color | done | the Color page's correctors |
| Workspaces | done | Editing, Effects, Color, Audio |
| Export media: H.264 MP4 | done | any size and rate, quality or bit rate, in the background with Cancel |
| Export: WAV, still frame, image sequence | done | PNG and JPEG frames, PNG runs |
| Export: HEVC, ProRes, other containers | missing | |
| Proxies | done | half size H.264 copies on request; Use Proxies |
| Multicam | missing | |
| Captions | missing | the phone's Auto Captions says it comes later |
| Keyboard shortcuts customizable | done | in Settings, Keymaps, like every studio |
| Undo history | done | Sue's history per project |

## DaVinci Resolve

| Feature | Status | Notes |
| --- | --- | --- |
| Page bar (Media, Edit, Fusion, Color, Fairlight, Deliver) | done | Media, Edit, Effects (in place of Fusion), Color, Audio, Deliver |
| Media page with clip metadata | partial | pool, viewer and facts; no metadata editing |
| Edit page dual viewers | done | source and program side by side |
| Edit page dockable panels | done | Sue's dock: drag tabs, splitters, floating palettes, reset layout |
| Inspector | done | transform, opacity, speed, volume, fades, titles; Effect Controls beside it |
| Effects library | done | effects, transitions, presets, with search |
| Cut page | missing | |
| Fusion page (node compositing) | partial | effects stacks per clip, adjustment clips, track mattes and masks; no node graph editor |
| Color page: primaries wheels | done | lift, gamma, gain, offset, with master levels |
| Color page: contrast, pivot, saturation, temperature, tint | done | |
| Color page: curves | done | custom (master, red, green, blue), hue against hue, saturation and luminance |
| Color page: qualifier | done | HSL ranges with softness and invert; no matte view yet |
| Color page: windows (power windows) | partial | clip masks on the Effects page; not on the Color page |
| Color page: nodes | partial | serial correctors with bypass; no parallel or layer nodes |
| Color page: LUTs | done | `.cube` (3D, and 1D turned 3D) imported into the pool |
| Scopes | done | waveform, RGB parade, vectorscope, histogram, off the interface's thread |
| Before and after split, copy and paste grade | done | |
| Fairlight page: mixer | done | faders, pan, mute, solo, meters with LUFS |
| Fairlight: EQ, dynamics | done | five band EQ, gate, compressor per track |
| Fairlight: buses, plugins | missing | |
| Loudness normalize | done | to -16 LUFS, peaks under -1 dBFS |
| Voice over recording | done | from the default microphone onto the targeted audio track |
| Deliver page with render settings | partial | one export at a time; no render queue list beyond Sue's transfers |
| Timeline proxy resolution | done | full, half, quarter preview (View menu, `sue_ve.preview_quality`) |
| Smart cache | done | paused playback renders the next seconds that need the processor |
| Shuttle (J, K, L), loop playback | done | up to 8 times; loop over in to out |
| Hardware decoding | partial | through Media Foundation and Direct3D 11; measured slower than the processor path here, so off by default |

## After Effects

| Feature | Status | Notes |
| --- | --- | --- |
| Layers with transform and opacity keyframes | done | per clip on tracks, with anchor point; no separate compositions beyond nested sequences |
| Keyframe graph editor, easing | done | value and speed graphs, F9 Easy Ease |
| Masks | done | rectangle, ellipse, Bezier path; feather, expansion, opacity, modes, invert; feather, expansion and opacity keyframable; shapes not keyframable yet |
| Mask tracking | missing | a placeholder button says it comes later |
| Track mattes | done | alpha and luma, inverted; the matte track is hidden |
| Blend modes | done | Sue's shared blend modes |
| Motion blur | done | a shutter angle per clip, eight samples |
| Effects and presets | done | 19 kinds; presets as `.sveffect` files with their keys |
| Expressions | missing | |
| Shape and text layers | done | shapes; text animators (position, scale, rotation, opacity, per letter or whole) with a keyframed range |
| Motion tracking | missing | |
| Render queue | partial | exports run in Sue's transfers list |

## CapCut

| Feature | Status | Notes |
| --- | --- | --- |
| Phone layout: preview, timeline with a centred playhead, tool bar | done | phones and tablets; pinch zoom, long press drag |
| Split, delete, speed, reverse, freeze | done | |
| Text, stickers, effects, filters | partial | text with animations, effects, filter looks; no stickers |
| Overlay | done | onto the track over the main one |
| Aspect ratio presets | done | 9:16, 16:9, 1:1, 4:5 |
| Canvas | partial | a color behind the video; no blurred backdrop |
| Auto captions | missing | placeholder |
| Templates | missing | placeholder; the Hub's New offers HD, 4K, vertical and square |
| Export with resolution and frame rate | done | 720p to 2160p, 24 to 60 fps |
