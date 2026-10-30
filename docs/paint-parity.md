# Sue Paint parity with Clip Studio Paint EX

Everything Clip Studio Paint EX can do, and where Sue Paint stands on
each. The list comes from Clip Studio Paint EX Ver. 5: the menus of the
running app, read on 2026-10-30, the official user guide for Ver. 5.0
(help.clip-studio.com), the Ver. 5.0 and 5.1 release notes
(clipstudio.net/en/dl/release_note) and the EX function list
(clipstudio.net/en/functional_list). Where Clip Studio marks a command
EX only, the row says `EX`; Sue Paint aims for all of EX, and Sue's own
rows are at the end.

- **Sue**: `done`, `partial` (some of it works; the note says what is
  missing) or `missing`.
- The **Clip Studio** column says where the feature lives there: a menu
  path, a tool or a palette.
- Where a feature belongs to all of Sue (fonts, file dialogs, sharing),
  the note names the shared crate it lives in or should.
- Statuses were set on 2026-10-30 from Sue's code (`crates/sue-paint`,
  its menu table `menus.rs` and the handlers in `studio.rs`), from the
  guide in `HUMANS.md`, and from the commit history. They were checked
  against the code again the same day, after Advanced Fill, the tool
  palette and the filter set landed on `main`. Every menu command
  Sue shows has a handler, so `partial` usually means "the command runs,
  but nobody has checked it against Clip Studio yet"; such notes say
  **verify**. Check a row against the running apps before you build on
  it, and take the word verify out of the note when you have.

## Progress

| Area | done | partial | missing | percent |
| --- | --- | --- | --- | --- |
| File: new, open and save | 10 | 1 | 5 | 66 |
| File: export | 10 | 1 | 7 | 58 |
| File: import, batch and scanning | 3 | 4 | 8 | 33 |
| Edit: undo, clipboard and fills | 8 | 5 | 0 | 81 |
| Edit: transform and canvas size | 8 | 6 | 0 | 79 |
| Tonal correction and color | 10 | 1 | 1 | 88 |
| Filters | 19 | 0 | 1 | 95 |
| Layers | 17 | 5 | 0 | 89 |
| Selection | 9 | 0 | 0 | 100 |
| Rulers and symmetry | 7 | 1 | 0 | 94 |
| Comics: frames, balloons and webtoons | 7 | 1 | 0 | 94 |
| Story: multi-page works (EX) | 4 | 5 | 4 | 50 |
| Animation | 12 | 1 | 1 | 89 |
| Tools and drawing | 26 | 7 | 3 | 82 |
| Text | 5 | 3 | 2 | 65 |
| Vector | 6 | 2 | 0 | 88 |
| 3D | 5 | 0 | 8 | 38 |
| Colors | 9 | 0 | 1 | 90 |
| View and navigation | 9 | 3 | 0 | 88 |
| Window, palettes and workspaces | 14 | 3 | 1 | 86 |
| Materials and assets | 6 | 3 | 2 | 68 |
| Print | 0 | 2 | 2 | 25 |
| Preferences and settings | 5 | 2 | 1 | 75 |
| File formats | 9 | 2 | 3 | 71 |
| Sue's own, beyond Clip Studio | 7 | 0 | 0 | 100 |
| **All of Sue Paint** | 225 | 58 | 50 | 76 |

A row counts 1 as `done`, half as `partial` and 0 as `missing`:
percent = (done + partial / 2) / (done + partial + missing) x 100,
rounded to the nearest whole percent. Recount the sections with:

```bash
awk -F'|' '/^## / {if (name) print name ": " d " done, " p " partial, " m " missing";
  name = substr($0, 4); d = p = m = 0; next}
  /\| done \|/ {d++} /\| partial \|/ {p++} /\| missing \|/ {m++}
  END {print name ": " d " done, " p " partial, " m " missing"}' docs/paint-parity.md | tail -n +2
```

(the first line it prints is this Progress section; leave it out). When
you change a row, recount, and write the new numbers and percents here
and in the row's section if it carries one.

## How to check and update this list

Any agent can take a row. The steps:

1. Read the board first, per AGENTS.md Rule 9: read every claim and
   message in `$(git rev-parse --git-common-dir)/sue-agents/`, then
   write your own claim. `crates/sue-paint/` is often owned by another
   agent; the parity list itself (`docs/paint-parity.md`) and the shared
   files are additions only.
2. Work on a branch. Build and run Sue with `cargo run -p sue`, open
   **Sue Paint** from the Hub, and try the feature the way the row's
   note says. Compare against Clip Studio Paint EX if the human has it
   open; ask before driving that window yourself, and prefer the
   official guide at help.clip-studio.com instead of automating it.
3. Move the row: `done` when a Clip Studio user would use it and not
   look for the rest, `partial` when it exists but falls clearly short
   of Clip Studio's (say what is missing in the note), `missing` when
   there is nothing there. Take **verify** out of a note you checked.
4. Recount and update the Progress table (the command above).
5. Update `HUMANS.md` when the feature changed what people can do, and
   add a line for people who use Sue under Unreleased in
   `CHANGELOG.md`.
6. Follow Rule 8: commit only when the human has asked, on a branch,
   never force.

A row can also be wrong: Clip Studio moves features between versions
(Ver. 5.1 renamed Sub Tool to Tool Group and Tool Property to Tool
Settings; QUMARION support went away). When your copy differs from this
list, follow your copy, note the version, and fix the row.

The order of work is the plan at the end.

## File: new, open and save

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| New canvas with its six projects (Illustration, Webtoon, Comic, Printing of fanzine, Show all comic settings, Animation) | File > New | done | New is laid out as Clip Studio's, each project at its usual size |
| Create New from Clipboard | File > Create New from Clipboard | done | A new canvas from the picture on the clipboard |
| Create new from movie | File > Create new from movie | missing | An animation project from a video |
| Open pictures and projects | File > Open | done | PNG, JPEG, JPEG XL and Sue projects through the system's own dialog (`FilePick`) |
| Open .clip files | File > Open | partial | Layers, folders, blend modes, opacity, clipping and tones come in; pixel, text and vector layers arrive as the pixels Clip Studio last drew, and correction, gradient and fill layers as empty layers |
| Recent files, Clear Recent Files | File > Recent | done | |
| Close | File > Close | done | |
| Save (keeps the .clip format) | File > Save | done | Save keeps a .clip that Clip Studio opens with everything; the file also carries Sue's own copy of the picture |
| Save as | File > Save as | done | A Sue project, a Photoshop document or a flat picture |
| Save Duplicate: BMP, JPEG, PNG, TIFF, Targa, WebP, PSB, PSD | File > Save Duplicate | done | JPEG XL too |
| Revert | File > Revert | done | Reloads the last saved file, and says so when nothing was saved yet |
| Manage fonts | File > Manage fonts | missing | Text reads the installed fonts from `sue_wm::fonts`; no manager yet |
| Connect to smartphone | File > Connect to smartphone | missing | Companion Mode, a phone as a second screen |
| Files in the CLIP STUDIO cloud | File > Manage files in Clip Studio | missing | Sue's projects and package manager serve the same end |
| Tab-Mate and TABMATE hardware | File > Tab-Mate Controller | missing | Peripheral devices |
| Quit | File > Quit | done | Back to the Hub first |

## File: export

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Export single layer: BMP, JPEG, PNG, TIFF, Targa, WebP, PSB, PSD | File > Export (Single Layer) | done | JPEG XL too |
| Export animation: image sequence | File > Export animation > Image sequence | done | |
| Export animation: Animated GIF | File > Export animation > Animated GIf | done | |
| Export animation: movie | File > Export animation > Movie | done | Verify the sound track rides along |
| Export animation: Animated sticker (APNG) | File > Export animation | missing | |
| Export animation: Animated WebP | File > Export animation | missing | |
| Export animation: OpenToonz Scene File | File > Export animation | missing | |
| Export animation: Exposure sheet | File > Export animation | missing | |
| Export animation cels | File > Export animation > Export animation cels | done | Every cel as its own picture |
| Export multiple pages: PDF (EX) | File > Export multiple pages > .pdf | done | A book of every page, manga read right to left |
| Export multiple pages: EPUB (EX) | File > Export multiple pages > Export EPUB data | done | |
| Batch export of pages (EX) | File > Export multiple pages > Batch export | partial | Export All Pages saves every page; Clip Studio's dialog sets format and layer comps per page |
| Export fanzine printing data (EX) | File > Export multiple pages | missing | |
| Export webtoon | File > Special Export > Export webtoon | done | Numbered pictures at the height and width you choose |
| Export vectors | File > Special Export | missing | |
| Export layer comp (EX) | File > Special Export | missing | Layer comps themselves are missing too |
| Record timelapse | File > Timelapse > Record timelapse | done | A picture of the canvas after every change |
| Export timelapse (video or GIF) | File > Timelapse > Export timelapse | done | Fitted to a length you choose; Clip Studio's presets run to 300 seconds |

## File: import, batch and scanning

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Import image | File > Import > Image | done | |
| Import image as separate layers | File > Import | partial | Command exists; verify one layer per image |
| Pattern from Image | File > Import > Pattern from Image | partial | Command exists; verify it lands as a tileable material |
| Import vector | File > Import > Vector | partial | Command exists; verify what it reads |
| Import audio | File > Import > Audio | done | A sound on the timeline from the frame you are on |
| Import movie | File > Import > Movie | partial | Command exists; an animation from a movie is a phone template |
| Import 3D data (OBJ) | File > Import > 3D Data | done | |
| FBX import (5.1) | File > Import | missing | |
| 3D pose from Posemaniacs | File > Import > 3D pose | missing | |
| Pose Scanner (image to pose) | File > Import > Pose Scanner | missing | |
| Create file object | File > Import > Create file object | missing | Another file embedded as an object layer |
| Batch import (EX) | File > Import > Batch import | missing | |
| Scan, Continuous Scan, Select Scan Device | File > Import > Scan | missing | A scanner talks through `sue-platform` |
| Exposure sheet import | File > Import > Exposure sheet | missing | |
| Batch process (EX) | File > Batch process | missing | Run commands on many pages or files at once |

## Edit: undo, clipboard and fills

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Undo, Redo | Edit | done | A history per document; exact inverse deltas, never a snapshot |
| Cut, Copy, Paste | Edit | done | |
| Paste to shown position | Edit > Paste to shown position | done | |
| Delete | Edit | done | |
| Delete Outside Selection | Edit | done | Shift+Delete |
| Fill with the main color | Edit > Fill | done | Alt+Delete |
| Advanced Fill dialog (EX) | Edit > Fill (Advanced) | partial | Every option but Fill up to vector path and Exclude |
| Smart Smoothing (5.0) | Edit > Smart Smoothing | done | Smooths jagged lines, keeps their thickness and darkness |
| Convert to Drawing Color | Edit > Convert to drawing color | partial | Command exists; verify |
| Outline Selection | Edit > Outline Selection | partial | Command exists; verify |
| Convert Brightness to Opacity | Edit > Convert brightness to opacity | partial | Command exists; verify |
| Clear Work History | Edit > Clear Work History | partial | Command exists; verify |
| Register Material: Image and Brush Tip | Edit > Register Material | done | |

## Edit: transform and canvas size

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Scale/Rotate | Edit > Transform | done | Drag the box, or type the size and angle |
| Scale alone, Rotate alone | Edit > Transform | partial | Commands exist; verify |
| Free Transform | Edit > Transform | done | Each corner on its own |
| Distort, Skew | Edit > Transform | partial | Commands exist; verify |
| Perspective | Edit > Transform | partial | Command exists; verify |
| Mesh Transformation | Edit > Transform | partial | Command exists; Clip Studio 5.1 made its points editable |
| Puppet Warp | Edit > Transform | done | Pins, Alt removes one |
| Flip Horizontal, Flip Vertical, Rotate 180 (the layer) | Edit > Transform | done | |
| Change Image Resolution | Edit > Change Image Resolution | done | |
| Change Canvas Size | Edit > Change Canvas Size | done | Around a reference point |
| Change Canvas Height (webtoons) | Edit > Change Canvas Height | partial | It opens Change Canvas Size to do it |
| Crop | Edit > Crop | done | To the selection |
| Canvas Properties | Edit > Canvas Properties | partial | Command exists; verify |
| Align/Distribute a layer's drawing | Edit > Align/Distribute | done | To the canvas edges and middle |

## Tonal correction and color

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Brightness/Contrast | Edit > Tonal Correction | done | |
| Level Correction | Edit > Tonal Correction | done | |
| Tone Curve | Edit > Tonal Correction | done | Points dragged, live preview |
| Hue/Saturation/Luminosity | Edit > Tonal Correction | done | |
| Color Balance | Edit > Tonal Correction | done | |
| Reverse Gradient | Edit > Tonal Correction | done | |
| Posterization | Edit > Tonal Correction | done | |
| Binarization | Edit > Tonal Correction | done | |
| Gradient Map | Edit > Tonal Correction | done | |
| Correction layers for all of the above | Layer > New Correction Layer | done | Hide or fade one to weaken it; double-click to change it |
| Tonal correction across several layers at once (5.0) | Edit > Tonal Correction | missing | |
| Color profile preview | View > Color Profile | partial | Commands exist; verify the preview settings |

## Filters

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Quick Filters | Filter > Quick Filters | done | Vivid, Sepia, Night and the rest, previewed on a small copy |
| Quick Effects | Filter > Quick Effects | done | Glow, Halftone, Pixel art and the rest; apply to all works on a merged layer |
| Blur, Blur (Strong) | Filter > Blur | done | |
| Gaussian Blur | Filter > Blur | done | |
| Motion, Radial, Zoom and Lens blur | Filter > Blur | done | Angle, distance and amount, each with its dialog |
| Smooth | Filter > Blur | done | A light smoothing, as Clip Studio's |
| Remove Dust | Filter > Correction | done | Clears the specks on a raster layer |
| Remove JPEG Noise, Line Width | Filter > Correction | done | |
| Distort: Wave, Pinch, Polar Coordinates, Ripple, Fisheye, Twirl, Zigzag | Filter > Distort | done | Each with its own settings |
| Mosaic, Crystallize | Filter > Effect | done | |
| Add Noise | Filter > Effect | done | |
| Line Extraction | Filter > Effect > Line extraction | done | A picture into clean black lines |
| Chromatic Aberration | Filter > Effect | done | |
| Retro | Filter > Effect | done | |
| Glow, Inflate, Drop shadow (5.1) | Filter > Effect | missing | Quick Effects already has a look like Glow |
| Focus Lines, Speed Lines | Filter > Render | done | |
| Screentone | Filter > Render > Screentone | done | |
| Perlin Noise, Clouds | Filter > Render | done | |
| Sharpen, Sharpen (Strong), Unsharp Mask | Filter > Sharpen | done | |
| Convert to Lines and Tones (EX) | Filter, and Layer | done | Line art and tone on layers of their own, from a picture or a 3D render |

## Layers

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| New Raster Layer | Layer > New Layer | done | |
| New Vector, Text, Balloon, Tone layers | Layer > New Layer | done | |
| New Gradient, Fill and Paper layers | Layer > New Layer | partial | Commands exist; verify the kinds behave as Clip Studio's |
| New 3D Layer | Layer > New Layer | done | Cube, sphere, cylinder, cone, plane |
| New Frame Border Folder (EX) | Layer > New Layer | done | |
| New Correction Layer | Layer > New Correction Layer | done | |
| New Layer Folder, Create folder and insert layer | Layer | done | Ctrl+G |
| Ungroup Layer Folder | Layer | done | |
| Duplicate Layer, Delete Layer | Layer | done | |
| Delete Layers: Empty, Hidden | Layer > Delete Layers | partial | Commands exist; verify |
| Hide All Visible Draft Layers | Layer | partial | Command exists; verify |
| Layer Mask: from selection, outside selection, apply, delete, enable, edit | Layer > Layer Mask | done | Paint the mask with pen and eraser |
| Ruler/Frame: delete, show, perspective, symmetrical | Layer > Ruler/Frame | done | |
| Concentric Circle Ruler | Layer > Ruler/Frame | partial | Command exists; verify |
| Layer Settings: show, rename, clip to layer below, reference, draft, lock, lock transparent pixels | Layer > Layer Settings | done | |
| Layer Property: border effect, tone, layer color, expression color | Layer > Layer Property | done | The drawing itself never changes, so effects come off again |
| Change Selected Layer, Layer Order | Layer | done | |
| Transfer to Lower Layer | Layer | done | |
| Merge with Layer Below, Merge Selected, Merge Visible, Merge Visible to New, Flatten | Layer | done | |
| Rasterize | Layer > Rasterize | done | |
| Convert Layer (kind, expression color) | Layer > Convert Layer | partial | Command exists; verify the dialog |
| Blend modes and opacity | Layer palette | done | |

## Selection

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Select All, Deselect, Reselect, Invert Selected Area | Select | done | |
| Expand, Shrink Selected Area | Select | done | |
| Blur Border (feather) | Select > Blur border | done | |
| Select Color Gamut | Select > Select color gamut | done | |
| Selection from Layer: create, add, remove | Select > Selection from Layer | done | |
| Convert to Selection Layer, and back | Select | done | |
| Quick Mask | Select > Quick Mask | done | Paint the selection in red, then turn it back |
| Selection Launcher bar | View > Selection Launcher | done | |
| Select the layer under the pointer | Canvas | done | Ctrl+Shift+click |

## Rulers and symmetry

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Linear Ruler | Ruler tool | done | Lines starting near it run along it |
| Curve, Figure and Ruler Pen rulers | Ruler tool | done | On a curve you click out, a shape you drag, a line you draw |
| Special Rulers: parallel, focus, radial | Ruler tool | done | Lines parallel, toward a middle, or round it |
| Perspective Ruler, 1, 2 and 3 points | Ruler tool, Layer > Ruler/Frame | done | Or from two edges of the drawing that meet in the distance |
| Symmetrical Ruler | Ruler tool, Layer > Ruler/Frame | done | Mirrors, or turns about its middle as many times as you set |
| Concentric Circle Ruler | Ruler tool, Layer > Ruler/Frame | partial | Command exists; verify |
| Rulers saved with the picture | File | done | |
| Snap to Ruler, Special Ruler and Grid | View > Snap | done | |

## Comics: frames, balloons and webtoons

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Frame Border Folder with gutters (EX) | Layer > New Layer > Frame Border | done | What you draw under it shows only inside the panels |
| Frame Border tool: draw and divide panels | Tool | done | Drag a line across a panel to divide it |
| Balloon tool | Tool | done | Balloons and their tails as one outlined shape |
| Balloon sub tool set (rounded, ellipse, curve, tails) | Tool | partial | One general balloon; breadth to grow |
| Tone layers and the Layer Property tone | Layer > New Layer > Tone | done | Halftone dots at the frequency and angle you choose |
| Webtoon canvas and Export Webtoon | File > New, Special Export | done | |
| Manga binding on the right or left | File > New > Comic | done | |
| Facing pages in the Page Manager | Page Manager | done | |

## Story: multi-page works (EX)

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Page Manager | Story > Page Manager | done | Pages side by side as spreads; click to choose, double-click to open, move earlier or later |
| Add, Insert (advanced), Duplicate and Delete pages | Story | done | |
| First, Previous, Next and Last page | Story | done | |
| Page Reordering | Story > Page Reordering | partial | Command exists; the Page Manager also reorders |
| Combine and Split Pages | Story | partial | Commands exist; verify |
| Change Project Settings, Page Settings, Story Information | Story | partial | Commands exist; verify |
| Story Editor (text of the whole work) | Story > Story Editor | partial | Command exists; verify |
| Batch Import Text | Story > Batch import text | partial | Command exists; verify |
| Open, Duplicate, Export Page, Export All Pages | Story > Page Management | done | |
| Change binding point and start page | Page Manager | missing | |
| Folio and page numbers | Page Manager | missing | |
| Sort page file names | Page Manager | missing | |
| Group work and Teamwork (EX) | Story, Page Manager | missing | |

## Animation

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| New Timeline, Enable Timeline | Animation > Timeline | done | |
| Edit Timeline: frame rate and length | Animation > Timeline > Edit Timeline | done | Without moving your keys |
| Animation Folder | Animation > New Animation Layer | done | |
| Camera Folder with keyframes | Animation > New Animation Layer > Camera Folder | done | Position, scale and rotation over the timeline; it glides between keys |
| New Animation Cel, Assign Animation Cel | Animation | done | |
| Add and Delete Keyframe, Insert and Delete Frame | Animation > Edit Track | done | |
| Onion Skin and its settings | Animation > Show Animation Cels | done | Before in blue, after in green; how many, their colors, how faint |
| Light Table and Register Selected Layer | Animation > Light Table | done | Trace from other layers |
| Play, Stop, Loop, Move Frame | Animation > Playback | done | |
| Timeline palette as Clip Studio's | Palette | done | Cels as pictures, every other second lighter, the current frame a red bar |
| Animation Cels palette | Palette | done | |
| Sound on the timeline | Animation, File > Import > Audio | done | Plays along, drag to move, right-click to mute or delete |
| Exposure sheet (EX) | Animation | missing | |
| Camera track depth (easing curves between keys) | Timeline palette | partial | Keys glide; easing to verify |

## Tools and drawing

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Pen tools with sub tools | Tools | done | Clip Studio's eleven pens, G-pen to Flat marker |
| Sketch tools: Pencil, Mechanical pencil, Charcoal, Crayon, Pastel, Chalk | Tool > Sketch | done | Clip Studio's six sketch tools |
| Brush tools with color mixing | Tools | done | Watercolor and gouache pick up and drag the color underneath |
| Airbrush | Tools | done | A soft sub tool |
| Decoration tool | Tool | missing | Its slot sits in Clip Studio's order, grayed out |
| Tool palette in Clip Studio Paint 4's order, with the main and sub colors and the transparent color beneath | Tool palette | done | Move holds hand, zoom and move layer; Comic holds frame border and balloon |
| Eraser with Vector modes | Tool | done | Only where it touches, up to where lines cross, or whole lines |
| Blend: Blur, Finger Tip, Liquify | Tool | done | Liquify has its own slot in Clip Studio's order; it pushes, swells, pinches and twirls |
| Fill bucket: Close gap, Area scaling | Tool | done | Paint stays inside lines with small breaks, no white edge |
| Auto Select (magic wand) | Tool | done | Looks at reference layers only when set |
| Gradient tool | Tool | partial | Gradient layers exist; verify the tool's dials |
| Figure tool (rectangle, ellipse, polygon, polyline) | Tool | partial | Shapes exist; verify strokes, fills and polylines |
| Smart Shape (5.0) | Drawing with any tool | missing | Hold a stroke to snap it to a clean line, curve or figure |
| Text tool | Tool | done | |
| Ruler tools | Tool | done | |
| Frame Border tool | Tool | done | |
| Balloon tool | Tool | done | |
| Eyedropper, and Move with hand, zoom and move layer | Tools | done | Click again to step through Move's tools |
| Object tool (Operation) | Tool | done | Vector lines, 3D layers and camera folders in one tool |
| Correct Line and Control Point tools | Tool | done | Reshape, pinch, smooth, join, thicken, redraw, give pen pressure |
| Sub tool library breadth | Tools | partial | Clip Studio's eleven pens and six sketch tools are there; breadth grows next |
| Tool Group palette: each brush's own stroke preview, Add from downloads and the Downloads folder | Tool Group palette | partial | The button does nothing yet, so no sub tools arrive from downloads |
| A tap with the pen leaves a dot the size of the brush | Drawing | done | Pens no longer start thin with a mouse either |
| Tool Settings: size, opacity, hardness, smoothing, pressure | Palette | done | Changes stick to the sub tool |
| Dynamics by pen pressure, speed and chance, along curves | Tool Settings | done | Each with a minimum you choose |
| Starting and Ending: pixels, share of the line, fade | Tool Settings | done | Thin the line, fade it, or both, longer when you draw fast |
| Color jitter, per dab or per stroke | Tool Settings | done | |
| Stabilizer with Adjust by speed | Tool Settings | done | The line never falls far behind, and finishes along the pen's way |
| Correction, Taper and the trailing flick | Tool Settings | done | Corners stay sharp, ends thin to a point |
| Brush Size: Specify by size on screen, At least 1 pixel | Tool Sliders | done | |
| Create Brush Tip from Image | Edit > Create Brush Tip from Image | partial | Command exists; verify the dialog |
| Brush tip settings: spray, ribbon, direction from line | Advanced Tool Settings | partial | To verify |
| Watercolor edge, with the 5.1 Vivid style | Tool Settings, Layer Property | partial | The edge to verify; the Vivid style is missing |
| Double-sided brush tips | Advanced Tool Settings | missing | |
| Pen Pressure Settings | File > Pen Pressure Settings | done | Draw to fit your hand, Stronger and Lighter |
| Windows Ink and WinTab tablets | Peripherals | done | Huion, XP-Pen and Wacom work either way |

## Text

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Click and type on the canvas | Text tool | done | |
| Any installed font | Text tool | done | From `sue_wm::fonts` |
| Vertical text | Text tool | done | In columns |
| Edit the words again | Text tool | done | Click the words |
| Words sit in a balloon's middle | Text tool | done | Click inside the balloon |
| Mixed fonts and sizes in one text layer | Text tool | partial | To verify |
| Letter spacing, line spacing, justification | Text tool | partial | To verify |
| Font list with previews and recent fonts | Tool Settings | partial | To verify |
| Text along a curve or inside a shape | Text tool | missing | |
| Character palette (special glyphs) | Palette | missing | |

## Vector

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Vector layers | Layer > New Layer > Vector Layer | done | Lines kept as lines |
| Vector eraser: touch, up to a crossing, whole lines | Tool | done | Cleaning up crossing lines is one tap |
| Object tool: select, move and delete lines | Tool | done | Shift picks more |
| Control Point: add, delete, corner, widen, fade, split | Tool | done | |
| Pinch, Smooth, Join, Redraw and give pen pressure | Tool | done | |
| Thicken or thin lines | Tool | done | |
| Simplify lines | Tool | partial | To verify |
| Batch edit lines by width or pressure | Tool | partial | To verify |

## 3D

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| 3D layers with primitives | Layer > New Layer > 3D Layer | done | Cube, sphere, cylinder, cone, plane |
| OBJ import | File > Import > 3D Data | done | |
| Camera: drag to turn, Shift to move, Ctrl to come nearer | Object tool | done | |
| Light and shading settings | Tool Settings | done | Shaded, line art or both |
| Convert 3D to Lines and Tones (EX) | Layer > Convert to Lines and Tones | done | Clean line art and tone to draw over |
| Drawing figures with poses and pose materials | Materials, 3D | missing | |
| 3D head models (5.1) | 3D | missing | |
| 3D hand models and Hand Scanner posing (5.0) | 3D, Materials | missing | |
| Painting directly on 3D (5.0, 5.1) | 3D | missing | |
| VRM import and photorealistic rendering | File > Import | missing | |
| Height-based fog, Follow camera light (5.0, 5.1) | 3D settings | missing | |
| Batch texture export (EX) | File > Export | missing | |
| FBX import (5.1) | File > Import | missing | |

## Colors

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Color Wheel | Palette | done | |
| Color Slider | Palette | done | |
| Color Sets: Default, Grays, Skin Tones, Pastels, your own | Palette | done | Import a GIMP palette or a list of color codes, export one |
| Color History | Palette | done | |
| Intermediate Color | Palette | done | Four colors at its corners |
| Approximate Color | Palette | done | The shades near your color |
| Main and sub color, transparent color, swap | Palette, keys | done | X swaps, C takes the transparent color |
| Pick from the picture (Alt, right click) | Canvas | done | |
| Sub View reference pictures pick colors | Sub View palette | done | |
| Lab color space (5.1) | Color Slider | missing | |

## View and navigation

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Zoom: in, out, 100%, 200%, Fit to Screen, Print Size | View | done | |
| Reset Display, Fit to Navigator | View | done | |
| Rotate and flip the view | View > Rotate/Invert | done | Tilting never changes the picture |
| Scroll bars, with zoom and turn controls | View > Scroll Bar | done | |
| Ruler bar and grid, Pixel Grid | View | done | |
| Crop marks, inner border and bleed | View | partial | Commands exist; verify they draw on the canvas |
| Show Tone Area, Paper, Page Image | View | partial | Commands exist; verify |
| Selection, Transform, Text and Object launchers | View | done | Small bars of buttons under what you work on |
| Color profile preview | View > Color Profile | partial | Commands exist; verify |
| Tabs show a dot when a picture changed | Canvas tabs | done | |
| Next and previous canvas | Window > Canvas | done | Ctrl+Tab |
| Touch: two fingers move, zoom and turn; two-finger tap undoes | Gestures | done | A pinch never paints and never undoes |

## Window, palettes and workspaces

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Clip Studio's default frame | Workspace | done | Tools, Tool Group and colors at the left, Navigator and layers at the right, a strip of folded palettes at the far right |
| Palette docking, floating windows, folding into the strip | Palette menus | done | Floats go anywhere, outside Sue's window too |
| Drag column widths and palette heights | Dock | done | Narrow windows fold the widest palettes by themselves |
| Reset Palette Layout | Window > Reset Palette Layout | done | |
| Register and Manage Workspaces | Window > Workspace | done | |
| Hide All Palettes, Hide Menu Bar | Window | done | Tab and Shift+Tab |
| Command Bar | Window > Command Bar | done | |
| Palettes: Tool, Tool Group, Tool Settings, Tool Sliders, Brush Size | Window | done | Clip Studio 5.1 renamed Sub Tool to Tool Group and Tool Property to Tool Settings; Sue's names already match |
| Palettes: Color Wheel, Color Slider, Color Set, Color History, Intermediate Color, Approximate Color | Window | done | |
| Palettes: Navigator, Sub View, Layer, Layer Property, History | Window | done | |
| Palettes: Timeline, Animation Cels, Material | Window | done | |
| Auto Action palette (recorded sequences) | Window > Auto Action Palette | partial | Records and plays menu commands; it does not record tool strokes as Clip Studio's can |
| Item Bank palette | Window > Item Bank | done | Keeps sub tools to add again |
| Information palette | Window > Information | partial | Canvas size, resolution and physical size; not the cursor position or selection info |
| Cascade, Tile, New Window | Window > Canvas | partial | Commands exist; verify |
| Interface: Automatic, Desktop, Tablet, Phone | Window > Interface | done | |
| Companion Mode | Window | missing | A phone as a second screen |
| Dialogs as movable, resizable windows | Dialogs | done | Their own windows, outside Sue's too |

## Materials and assets

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Material palette with tones and patterns | Palette | done | |
| Own pictures and brush tips as materials | Palette | done | |
| Favorite and Recently used | Palette | done | |
| Labels for materials | Palette | partial | Projects have labels; material labels to verify |
| Double-click to put on a canvas | Palette | done | Tones and patterns in your drawing color |
| Register Material: Image and Brush Tip | Edit > Register Material | done | |
| Materials kept in a folder of your choice | Settings | done | |
| Recently used folder and frequency sorting (5.0) | Palette | partial | To verify |
| User tags and batch tag management (5.1) | Palette | missing | |
| Download materials from CLIP STUDIO ASSETS | Material palette, Browse Assets | partial | Browse Assets reads the pages and opens the site to download; nothing lands in the palette yet |
| Pose, 3D and texture material kinds | Palette | missing | |

## Print

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Print | File > Print | partial | Command exists; nothing written about the dialog yet |
| Print Settings | File > Print Settings | partial | Command exists; verify |
| Print multiple pages (EX) | File > Print Settings | missing | |
| PDF with embedded ICC profiles and CMYK (5.0, EX) | File > Export multiple pages > .pdf | missing | |

## Preferences and settings

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Preferences dialog | File > Preferences | done | |
| Shortcut Settings | File > Shortcut Settings | done | Settings, Keymaps; the menus show what you chose |
| Modifier Key Settings | File > Modifier Key Settings | partial | Command exists; verify |
| Command Bar Settings | File > Command Bar Settings | partial | Command exists; verify |
| Pen Pressure Settings | File > Pen Pressure Settings | done | |
| Privacy Settings | File > Privacy Settings | missing | |
| Shortcut list | Help, F1 | done | Alt+I lists them all as set |
| Interface strings in many languages | (Clip Studio ships localized) | done | `sue-lang`, English first |

## File formats

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Sue project (.sue) | (Sue's own) | done | |
| Clip Studio (.clip) save | File > Save | done | Clip Studio opens it with everything |
| Clip Studio (.clip) open | File > Open | partial | See File: open, for what arrives and what does not |
| Photoshop (PSD, PSB) save | File > Save Duplicate | done | |
| Photoshop open | File > Open | partial | A reader exists; verify what it keeps |
| BMP, JPEG, PNG, TIFF, Targa, WebP | Save, Export | done | |
| JPEG XL | (Sue adds it) | done | Sue's default for its own image caches |
| QOI, GIF (flat) | Save, Export | done | |
| PDF, EPUB (EX) | Export multiple pages | done | |
| Animated GIF, MP4 | Export animation | done | |
| APNG, Animated WebP | Export animation | missing | |
| OpenToonz scene | Export animation | missing | |
| OBJ (3D import) | File > Import > 3D Data | done | |
| FBX (5.1) | File > Import | missing | |

## Sue's own, beyond Clip Studio

| Feature | Sue | Notes |
| --- | --- | --- |
| Start page with Projects, labels, search and favorites | done | The same lists the Hub shows |
| Manage Materials and Browse Assets on the start page | done | |
| Simple Mode for phones and tablets | done | Studio Mode for the desktop layout |
| Studio Mode and touch layouts, modifier keys on a slide-out | done | |
| One project holds Paint with every studio | done | Entities by EntityId in the `.sue` file |
| Send To between studios | done | Work moves by EntityId, never by exporting and importing |
| Command button (the palette of commands) | done | In the status bar, since Tab belongs to the palettes |

## The plan

Work goes by how much people miss each feature, and by what unblocks
the rest:

1. Clip file fidelity: correction, gradient and fill layers, and true
   vector lines, when a .clip opens. The shared picture models in
   `sue-model` carry them.
2. The Story menu's depth: Story Editor, Batch Import Text, Combine and
   Split Pages, project and page settings. Multi-page is what EX sells.
3. Export: APNG and Animated WebP, movie export with its sound, Batch
   process and fanzine printing data, Export vectors and layer comps.
4. The last filters: Glow, Inflate and Drop shadow; the rest of the set
   is built, so check each dialog against Clip Studio's as you go.
5. Smart Shape, Clip Studio 5.0's headline feature, on every pen and
   brush.
6. 3D: drawing figures with poses, painting directly on 3D, heads and
   hands, then the rest of the 3D rows.
7. Materials: downloads from CLIP STUDIO ASSETS into the palette, user
   tags, pose and 3D materials.
8. Text depth: mixed fonts and sizes in one layer, spacing and
   justification.
9. The verify rows: every row whose note says verify, checked against
   the running app and settled.
10. Print: the print dialog and settings, then multiple pages and CMYK
    PDF.

Then the rest, row by row.
