# Sue VE parity

What Sue VE does of what Premiere Pro, DaVinci Resolve, After Effects and
CapCut do, feature by feature. **Done** works and is tested; **partial**
works in part (the notes say which part); **missing** is not there yet.
Stage 1 (0.0.83) built the core and the desktop editor; stage 2 takes
effects and compositing, color, the mixer and the phone layout
(`docs/sve-handover.md`).

## Premiere Pro

| Feature | Status | Notes |
| --- | --- | --- |
| Project panel with bins, icon and list views | partial | thumbnails and a list; bins as folders in the model, no New Bin command yet |
| Import movies, sound, stills, image sequences | done | MP4 and MOV (H.264, HEVC where Windows has it), WAV, FLAC, MP3, AAC, Ogg; still pictures; numbered runs |
| Link or embed media | done | links by default; `sue_ve.embed_media` keeps the bytes in the `.sue` |
| Relink missing media | missing | a missing file shows its icon; no relink dialog |
| Source monitor with in and out, insert and overwrite | done | `I`, `O`, `,` and `.` |
| Program monitor | done | real time cuts, opacity, transforms and dissolves on the graphics card |
| Timeline with video and audio tracks | done | add tracks; targeting, lock, hide, mute, solo, sync lock |
| Selection, ripple, rolling, slip, slide, razor tools | done | live preview while dragging; one undo step on release |
| Rate stretch tool | missing | speed is set in the inspector or the Clip menu |
| Snapping, linked selection | done | |
| Lift, extract, ripple delete, close gap | done | |
| Add edit (Ctrl+K) | done | on targeted tracks, or the selected clips |
| Speed and duration, reverse | done | ripples what follows |
| Frame hold | done | Add Frame Hold from the playhead |
| Time remapping (speed ramps) | missing | |
| J and L cuts | done | trim or roll one side of a linked pair (linked selection off) |
| Video transitions: cross dissolve, dip to black and white | done | |
| Audio transitions: crossfade | done | |
| Effect Controls: motion, opacity, volume with keyframes | done | keys at the playhead; linear |
| Keyframe curves (Bezier) editing | missing | the shared curves hold Bezier keys; no graph editor yet |
| Essential Graphics titles | partial | a title clip: words, size, color, position; no styles, no rolls |
| Color solid, adjustment layer | partial | solids done; adjustment clips are placeholders until effects come |
| Nested sequences | missing | a sequence in the pool is not played as a clip yet |
| Markers, in and out marks | done | sequence markers; clip markers missing |
| Audio meters | done | peak meters with hold |
| Audio track mixer | missing | stage 2 (Audio page) |
| Lumetri color | missing | stage 2 (Color page) |
| Export media: H.264 MP4 | done | any size and rate, quality or bit rate, in the background with Cancel |
| Export: WAV, still frame, image sequence | done | PNG and JPEG frames, PNG runs |
| Export: HEVC, ProRes, other containers | missing | |
| Proxies | done | half size H.264 copies on request; Use Proxies |
| Multicam | missing | |
| Captions | missing | |
| Keyboard shortcuts customizable | done | in Settings, Keymaps, like every studio |
| Undo history | done | Sue's history per project |

## DaVinci Resolve

| Feature | Status | Notes |
| --- | --- | --- |
| Page bar (Media, Edit, Fusion, Color, Fairlight, Deliver) | partial | Media, Edit and Deliver work; Effects, Color and Audio show what comes later |
| Media page with clip metadata | partial | pool, viewer and facts; no metadata editing |
| Edit page dual viewers | done | source and program side by side |
| Edit page dockable panels | done | Sue's dock: drag tabs, splitters, reset layout |
| Inspector | done | transform, opacity, speed, volume, fades, titles |
| Effects library | partial | transitions; effects in stage 2 |
| Cut page | missing | |
| Fusion page (node compositing) | missing | stage 2 on `NodeGraph` |
| Color page: wheels, curves, nodes, scopes | missing | stage 2 |
| Fairlight page: mixer, buses, plugins | missing | stage 2 |
| Deliver page with render settings | partial | one export at a time; no render queue list beyond Sue's transfers |
| Timeline proxy resolution | done | full, half, quarter preview (View menu, `sue_ve.preview_quality`) |
| Shuttle (J, K, L), loop playback | done | up to 8 times; loop over in to out |
| Hardware decoding | partial | through Media Foundation and Direct3D 11; measured slower than the processor path here, so off by default |

## After Effects

| Feature | Status | Notes |
| --- | --- | --- |
| Layers with transform and opacity keyframes | partial | per clip on tracks; no separate compositions |
| Keyframe graph editor, easing | missing | |
| Masks, track mattes | missing | stage 2 |
| Effects and presets | missing | stage 2 |
| Expressions | missing | |
| Shape and text layers | partial | text through titles |
| Motion tracking | missing | |
| Render queue | partial | exports run in Sue's transfers list |

## CapCut

| Feature | Status | Notes |
| --- | --- | --- |
| Phone layout: preview, one main track, tool strip | missing | stage 2; the desktop layout shows on phones today |
| Split, delete, speed, reverse, freeze | done | from the desktop menus and keys |
| Text, stickers, effects, filters | partial | text titles only |
| Auto captions | missing | |
| Templates | partial | new video presets (HD, 4K, vertical, square) |
| Export to 1080p or 4K MP4 | done | |
