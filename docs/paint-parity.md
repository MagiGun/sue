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
| File: new, open and save | 11 | 1 | 3 | 77 |
| File: export | 15 | 1 | 2 | 86 |
| File: import, batch and scanning | 6 | 4 | 5 | 53 |
| Edit: undo, clipboard and fills | 9 | 5 | 0 | 82 |
| Edit: transform and canvas size | 8 | 6 | 0 | 79 |
| Tonal correction and color | 11 | 1 | 0 | 96 |
| Filters | 20 | 0 | 0 | 100 |
| Layers | 17 | 5 | 0 | 89 |
| Selection | 9 | 0 | 0 | 100 |
| Rulers and symmetry | 7 | 1 | 0 | 94 |
| Comics: frames, balloons and webtoons | 14 | 1 | 0 | 97 |
| Story: multi-page works (EX) | 8 | 5 | 2 | 70 |
| Animation | 21 | 1 | 0 | 98 |
| Tools and drawing | 36 | 8 | 1 | 89 |
| Text | 6 | 3 | 1 | 75 |
| Vector | 11 | 2 | 0 | 92 |
| 3D | 7 | 2 | 4 | 62 |
| Colors | 9 | 0 | 1 | 90 |
| View and navigation | 10 | 3 | 0 | 88 |
| Window, palettes and workspaces | 26 | 3 | 1 | 92 |
| Materials and assets | 6 | 3 | 1 | 75 |
| Print | 0 | 2 | 2 | 25 |
| Preferences and settings | 5 | 2 | 1 | 75 |
| File formats | 9 | 3 | 2 | 75 |
| Sue's own, beyond Clip Studio | 7 | 0 | 0 | 100 |
| **All of Sue Paint** | 277 | 61 | 26 | 84 |

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
(Ver. 5.1 may call Sub Tool Tool Group and Tool Property Tool Settings,
while every tutorial Sue follows shows Sub Tool and Tool Property, the
names Sue uses; QUMARION support went away). When your copy differs
from this list, follow your copy, note the version, and fix the row.

The order of work is the plan at the end.

## File: new, open and save

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| New canvas with its six projects (Illustration, Webtoon, Comic, Printing of fanzine, Show all comic settings, Animation) | File > New | done | New is laid out as Clip Studio's, each project at its usual size |
| Register and Delete canvas presets | File > New | done | Kept with your other Sue Paint settings |
| Create New from Clipboard | File > Create New from Clipboard | done | A new canvas from the picture on the clipboard |
| Create new from movie | File > Create new from movie | missing | An animation project from a video |
| Open pictures and projects | File > Open | done | PNG, JPEG, JPEG XL and Sue projects through the system's own dialog (`FilePick`) |
| Open .clip files | File > Open | partial | Layers, folders with their own blend mode and clipping, masks as masks, paper as paper, gray layers, pixels off the canvas, draft and reference come in, and the layer color effect. A .clip Sue wrote opens live: fill, gradient and correction layers, text, vector lines, effects, palette colors, rulers, layer comps and the timeline (animation folders and cels, keyframes, the 2D camera, sound). From a file Clip Studio saved, text and vector layers still arrive as the pixels Clip Studio last drew and correction, gradient and fill layers as empty layers, each with its Clip Studio data kept for the way back, until Clip Studio's own storage of them is observed. The status bar says per layer what was not exact |
| Recent files, Clear Recent Files | File > Recent | done | |
| Close | File > Close | done | |
| Save | File > Save | done | Writes the Sue project. A canvas opened from a flat picture saves back in its format, and asks first when layers would be merged, as Clip Studio does; one opened from a .clip asks once whether to export back to it or keep a Sue project beside it. The title names the file Save writes |
| Save as | File > Save as | done | A Sue project, a .clip, every flat picture type, then Photoshop Big Document and Photoshop Document with their layers, last as Clip Studio lists them |
| Save Duplicate: BMP, JPEG, PNG, TIFF, Targa, WebP, PSB, PSD | File > Save Duplicate | done | JPEG XL, GIF, QOI, ICO and PPM too; a flat picture goes through the export settings first |
| Show in folder | none (Sue's own) | done | File > Show in Folder shows the file the canvas was opened from or last written to; grayed while it has none |
| Revert | File > Revert | done | Reloads the last saved file, and says so when nothing was saved yet |
| Manage fonts | File > Manage fonts | missing | Text reads the installed fonts from `sue_wm::fonts`; no manager yet |
| Connect to smartphone | File > Connect to smartphone | missing | Companion Mode, a phone as a second screen |
| Quit | File > Quit | done | Back to the Hub first |

## File: export

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Export single layer: BMP, JPEG, PNG, TIFF, Targa, WebP, PSB, PSD | File > Export (Single Layer) | done | JPEG XL, GIF, QOI, ICO and PPM too; a flat picture goes through the export settings first |
| Export settings before a flat export | the export settings dialog | done | Format, output size in six ways, resampling, color or grays, background, JPEG quality, JPEG XL effort; the next export starts from the last one's settings |
| Export animation: audio | File > Export animation | done | The timeline's sound mixed, as WAV, FLAC or AIFF with bits, sample rate, channels and normalizing; grayed without sound |
| Export animation: image sequence | File > Export animation > Image sequence | done | |
| Export animation: Animated GIF | File > Export animation > Animated GIf | done | Its settings first: size, loop count, frames |
| Export animation: movie | File > Export animation > Movie | done | MP4 (H.264 and AAC, through Windows' own encoder) with the timeline's sound; Motion JPEG AVI where Windows has no encoder; size, frame rate, quality, 2D camera, frames |
| Export animation: Animated sticker (APNG) | File > Export animation | done | Loops forever; other viewers show the first frame |
| Export animation: Animated WebP | File > Export animation | done | Lossless frames, looping |
| Export animation: OpenToonz Scene File | File > Export animation | missing | |
| Export animation: Exposure sheet | File > Export animation | done | As CSV: frames down, folders across |
| Export animation cels | File > Export animation > Export animation cels | done | Every cel as its own picture, a folder for each animation folder |
| Export multiple pages: PDF (EX) | File > Export multiple pages > .pdf | done | A book of every page, manga read right to left |
| Export multiple pages: EPUB (EX) | File > Export multiple pages > Export EPUB data | done | |
| Batch export of pages (EX) | File > Export multiple pages > Batch export | partial | Export All Pages saves every page; Clip Studio's dialog sets format and layer comps per page |
| Export fanzine printing data (EX) | File > Export multiple pages | missing | |
| Export webtoon | File > Special Export > Export webtoon | done | Numbered pictures at the height and width you choose |
| Export vectors | File > Special Export | done | Visible vector layers as SVG, each line an outline as wide as drawn |
| Export layer comp (EX) | File > Special Export | done | Layer, Layer Comps keeps named sets of visibility, opacity and blend; each exports as its own picture |
| Record timelapse | File > Timelapse > Record timelapse | done | A picture of the canvas after every change |
| Export timelapse (video or GIF) | File > Timelapse > Export timelapse | done | MP4, AVI or GIF, fitted to a length you choose (the video lasts that length, however few frames); Clip Studio's presets run to 300 seconds. Aspect ratio, a preview that plays, and progress with Cancel while it runs in the background (CTV-7) |

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
| Pose Scanner (image to pose) | File > Import > Pose Scanner | done | Tool Property of a 3D drawing figure; MoveNet finds the body, limbs toward the camera guessed from how short they look |
| Create file object | File > Import > Create file object | missing | Another file embedded as an object layer |
| Batch import (EX) | File > Import > Batch import | done | Every picture in a folder, by name, one a page |
| Scan, Continuous Scan, Select Scan Device | File > Import > Scan | missing | A scanner talks through `sue-platform` |
| Exposure sheet import | File > Import > Exposure sheet | done | CSV, cels found by layer name |
| Batch process (EX) | File > Batch process | missing | Run commands on many pages or files at once |
| Batch convert pictures | none (Sue's own) | done | File > Batch Convert Pictures: picture files to another type in the background, next to each file or into one folder, with a name pattern, and turn, mirror, brightness, contrast and saturation for every picture |

## Edit: undo, clipboard and fills

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Undo, Redo | Edit | done | A history per document; exact inverse deltas, never a snapshot |
| Cut, Copy, Paste | Edit | done | |
| Copy vectors as SVG | Edit > Copy vectors as SVG | done | The vector layer's lines, or those the Operation tool picked, as SVG code on the clipboard |
| Paste to shown position | Edit > Paste to shown position | done | |
| Delete | Edit | done | While a mask is edited it hides on the mask and leaves the layer as it is |
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
| Scale alone, Rotate alone | Edit > Transform | done | Modes of the transform bar; the rotation centre moves anywhere |
| Free Transform | Edit > Transform | done | Each corner on its own |
| Distort, Skew | Edit > Transform | done | Modes of the transform bar; Ctrl on a corner distorts in any mode |
| Perspective | Edit > Transform | done | A corner and the one across its edge move apart |
| Mesh Transformation | Edit > Transform | done | A lattice of 2 to 33 points each way, every point dragged |
| Puppet Warp | Edit > Transform | done | Pins that pick together (Shift), turn (their ring) and bend each part alone; its Tool Property on the bar |
| Flip Horizontal, Flip Vertical, Rotate 180 (the layer) | Edit > Transform | done | |
| Change Image Resolution | Edit > Change Image Resolution | done | Bicubic, through the resampler every export uses |
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
| Tonal correction across several layers at once (5.0) | Edit > Tonal Correction | done | Every layer picked with the current one, one undo step; Layer, Select All Layers |
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
| Glow, Inflate, Drop shadow (5.1) | Filter > Effect | done | Each with a live preview and its own settings |
| Focus Lines, Speed Lines | Filter > Render | done | |
| Screentone | Filter > Render > Screentone | done | |
| Perlin Noise, Clouds | Filter > Render | done | |
| Sharpen, Sharpen (Strong), Unsharp Mask | Filter > Sharpen | done | |
| Convert to Lines and Tones (EX) | Filter, and Layer | done | Line art and tone on layers of their own, from a picture or a 3D render; a dialog with line and tone settings and a live preview |

## Layers

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| New Raster Layer | Layer > New Layer | done | |
| New Vector, Text, Balloon, Tone layers | Layer > New Layer | done | |
| New Gradient, Fill and Paper layers | Layer > New Layer | partial | A gradient layer stays live: the Gradient tool lays its line again or drags an end, Tool Property (Gradient or Object tool) edits its stops, shape and edge; verify fill and paper layers |
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
| Layer Property: border effect, tone, layer color, expression color | Layer > Layer Property | done | The drawing itself never changes, so effects come off again. Ctrl+B (Switch using layer color) turns the layer color on and off |
| Change Selected Layer, Layer Order | Layer | done | |
| Transfer to Lower Layer | Layer | done | |
| Merge with Layer Below, Merge Selected, Merge Visible, Merge Visible to New, Flatten | Layer | done | |
| Rasterize | Layer > Rasterize | done | |
| Convert Layer (kind, expression color) | Layer > Convert Layer | done | Name, raster or vector, expression color, blending mode, Keep original layer |
| Blend modes and opacity | Layer palette | done | Clip Studio's 28 in its order, Glow Dodge and Add (Glow) included, Brightness as it names it; a folder composites by its own mode or Through (the default), and the palette offers a folder Through first. The wheel over the list steps the mode, shown at once |
| Picking several layers, palette colors, row marks | Layer palette | done | A box in each row (the pen on the edited layer), Ctrl+click and Shift+click; lock, show, delete and palette color take every picked layer; locks, draft bars and effect badges in the rows |
| Search Layer, Split Layer List | Window > Search Layer; Layer palette | done | In the Layer palette itself (its search button) with Clip Studio's filter list, not a palette of its own |
| Right-click menu of a layer | Layer palette | done | Its own short list, in Clip Studio's order |
| Layer Property: anti-aliased border, tone options, monochrome inks | Layer Property | done | Density source, posterization, dot shape, Reflect layer opacity, dot position; black and white for Monochrome; Apply Expression Color of Preview |
| Simple tone settings | Layer > New Layer > Tone | partial | Frequency with presets, density, dot shape, angle; the noise size and factor and combining tones of the same settings are missing |

## Selection

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Select All, Deselect, Reselect, Invert Selected Area | Select | done | |
| Expand, Shrink Selected Area | Select | done | Expansion type: round or square corners |
| Blur Border (feather) | Select > Blur border | done | |
| Select Color Gamut | Select > Select color gamut | done | |
| Selection from Layer: create, add, remove, select overlapping area | Select > Selection from Layer | done | |
| Convert to Selection Layer, and back | Select | done | |
| Quick Mask | Select > Quick Mask | done | Paint the selection in red, then turn it back |
| Select Overlapping Vectors, Select Vectors Within Area | Select | done | Pick the lines of a vector layer for the Object tool |
| Selection Launcher bar | View > Selection Launcher | done | Clip Studio's buttons and order; Selection Launcher Settings adds any menu command |
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
| Balloon sub tool set (rounded, ellipse, curve, tails) | Tool | done | Ellipse, rounded rectangle, cloud, flash, curve, polyline, Balloon pen, tail and thought balloon tail |
| Polyline frame | Frame Border tool | done | Click the corners; Enter or the first corner again closes it |
| Edit panels with the Object tool | Object tool | done | Drag a corner, or a panel to move it |
| Saturated line and Speed line tools | Figure tool | done | Each on an effect line layer that stays editable: the Object tool moves its ends, Tool Property sets lines, width, scatter and color |
| Tone layers and the Layer Property tone | Layer > New Layer > Tone | done | Halftone dots at the frequency and angle you choose |
| Webtoon canvas and Export Webtoon | File > New, Special Export | done | |
| Manga binding on the right or left | File > New > Comic | done | |
| Facing pages in the Page Manager | Page Manager | done | |
| Divide Frame Border Equally | Layer > Ruler/Frame | done | On the panel the Object tool picked: panels across and down, the gutters between |
| Combine Frames | Layer > Ruler/Frame | done | The panels picked (Shift adds) become one |
| Frames snap to the default border | Frame Border tool | done | A Rectangle frame's corners snap onto a story page's default border |
| Balloon line and fill color, no edge | Balloon tool | partial | The line takes the drawing color and the inside the other color. Drawing handles a line width of 0 (no edge), but Tool Property's Line width starts at 1, and it has no color, toning or tail rows yet |

## Story: multi-page works (EX)

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Page Manager | Story > Page Manager | done | A view of the canvas area: spreads read from the bound side, or webtoon columns; double-click opens a page and docks the pages beside it; drag a page to move it; the right-click menu in Clip Studio's order |
| Add, Insert (advanced), Duplicate and Delete pages | Story | done | |
| First, Previous, Next and Last page | Story | done | |
| Page Reordering | Story > Page Reordering | partial | Command exists; the Page Manager also reorders |
| Combine and Split Pages | Story | partial | Commands exist; verify |
| Change Project Settings, Page Settings, Story Information | Story | partial | Commands exist; verify |
| Story Editor (text of the whole work) | Story > Story Editor | partial | Command exists; verify |
| Batch Import Text | Story > Batch import text | partial | Command exists; verify |
| Open, Duplicate, Export Page, Export All Pages | Story > Page Management | done | |
| Change binding point and start page | Page Manager | done | The first page alone or facing page 2, also in Change Story Information |
| Folio and page numbers | Change Story Information | done | Size, outer corner or center, from the first page number; on the canvas and in every export |
| 3D Preview for Binding (EX) | Page Manager, File > Export Multiple Pages | done | The book a spread at a time, bleed cut off; zoom and turn the pages |
| Webtoon reading direction | Page Manager, Story | done | Right to left or left to right; the Page Manager stands the pages in columns |
| Sort page file names | Page Manager | missing | |
| Group work and Teamwork (EX) | Story, Page Manager | missing | |

## Animation

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| New Timeline, Enable Timeline | Animation > Timeline | done | |
| Change Frame Rate: frame rate and length | Animation > Timeline > Change Frame Rate | done | Without moving your keys |
| Animation Folder | Animation > New Animation Layer | done | A layer folder holding its cels; drag layers in and out |
| Camera Folder with keyframes | Animation > New Animation Layer > Camera Folder | done | Position, scale and rotation over the timeline; it glides between keys |
| New Animation Cel, Assign Animation Cel | Animation | done | |
| Add and Delete Keyframe, Insert and Delete Frame | Animation > Edit Track | done | |
| Onion Skin and its settings | Animation > Show Animation Cels | done | Before in blue, after in green; how many, their colors, how faint |
| Show edited cel only, Check cel motion by key input, Check surrounding cels by key input | Animation > Show Animation Cels | done | The locked cel shows in place of the frame's; the frames from two cels back play into the current one; each press shows the cel before, the cel after, then the current one. The playhead stays (ABV-9) |
| Light Table and Register Selected Layer | Animation > Light Table | done | Trace from other layers |
| Animation Cels palette's light table: general light table, register file, Light table tool, reset position, opacity | Animation Cels palette | done | Moving a traced layer never moves the layer itself |
| Play, Stop, Loop, Move Frame | Animation > Playback | done | |
| Timeline palette as Clip Studio's | Palette | done | Cels as pictures in six sizes or none, every other second lighter, the current frame a bar with its number, the ruler's seconds and division lines, a track for every layer in the Layer palette's order |
| Timeline navigation: the wheel zooms around the pointer, Shift and the wheel pan, Alt and the wheel scroll the tracks, Space or the middle button and a drag pan, scrollbars | Timeline palette | done | The eye beside each animation folder shows and hides the same folder as the Layer palette's |
| Animation Cels palette | Palette | done | The cel being drawn, then one row per traced layer with its eye, opacity and colors; drag a layer from the Layer palette to register it |
| Sound on the timeline | Animation, File > Import > Audio | done | Plays along, drag to move, right-click to mute or delete |
| Exposure sheet (EX) | Animation | done | Animation > Timeline > Exposure Sheet: cels, blanks and holds, set by clicking |
| Camera track depth (easing curves between keys) | Timeline palette | partial | Keys glide; easing to verify |
| New Timeline and Change Settings dialogs, several timelines | Animation > Timeline | done | Name, frame rate, playback time and range, scene, shot, division line; a picker left of the ruler |
| Editing the timeline: pick, drag, move, Cut, Copy, Paste, Delete | Timeline palette | done | A click picks a cel, a drag picks a box, Shift and a click a run; a drag moves what is picked; Paste lands at the frame shown |
| Clips: Split, Merge, first and last displayed frame | Timeline palette right click | done | |
| Playback range, timeline labels, track labels | Timeline palette | done | Drag the range's ends on the ruler |
| Layer keyframes from the timeline: Smooth, Linear, Hold | Timeline palette | done | The three marks read apart without color |
| Insert Frame dialog, Audio Clip Settings | Animation > Edit Track, Timeline palette | done | Number of frames, Selected layers only, Split clip, Preview; a sound's volume, cuts and fades |
| Timeline palette menu | Timeline palette | done | Clip Studio's list and order in submenus; Show split view, Graph Editor, Manage timeline, Show 2D camera frames, Show camera path, Show all field guides and Thumbnail display settings are left out |
| Audio scrubbing | Animation > Playback Settings | done | Each frame reached by hand plays its own sound |
| Render 2D Camera | Animation > Playback Settings | done | Off, the canvas shows the whole picture with the cameras' frames over it |
| Delete Keyframe from Layer; keyframes cut, copied and pasted with their frames | Animation > Edit Track, Timeline palette right click | done | A camera's keys too; Add Keyframe with the camera picked keys the camera; a key's mark drags along its track |
| Animated GIF export: Dithering | File > Export Animation > Animated GIF | done | Error diffusion over a fixed 252 color palette |
| Folder rows on the timeline | Timeline palette | done | A folder holding animation folders is a parent row with a fold arrow; a folder's keys show in a band over its cels |

## Tools and drawing

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Pen tools with sub tools | Tools | done | Clip Studio's eleven pens, G-pen to Flat marker |
| Sketch tools: Pencil, Mechanical pencil, Charcoal, Crayon, Pastel, Chalk | Tool > Sketch | done | Clip Studio's six sketch tools |
| Brush tools with color mixing | Tools | done | Gouache, oil paint, pastel and the wet wash pick up and drag the color underneath, covering as fully as the pen presses; the watercolor brushes pool at their edge instead |
| Airbrush | Tools | done | A soft sub tool |
| Decoration tool | Tool | done | Eighteen sub tools on Sue's brush: scattered shapes (stars, hearts, sparkles, snow, flowers, bubbles), shapes along the line (leaves, chain, lace, stitches, dotted and dashed lines) and ribbons bent along the stroke (rope, rickrack, film strip, checkered band, stitched ribbon, lace ribbon). Tool Property has Lay as a ribbon, Turn along the line, Angle, Scatter, Size jitter and Turn jitter. Downloaded decorations come in with Add from downloads |
| Tool palette in Clip Studio Paint 4's order, with the main and sub colors and the transparent color beneath | Tool palette | done | Move holds hand, zoom and move layer; Comic holds frame border and balloon |
| Eraser with Vector modes | Tool | done | Only where it touches, up to where lines cross, or whole lines |
| Blend: Blur, Finger Tip, Liquify | Tool | done | Liquify has its own slot in Clip Studio's order; it pushes, swells, pinches and twirls |
| Copy stamp | Blend tool | done | Alt and click sets the source; the first stroke fixes the distance, later strokes keep it |
| Close and fill | Fill tool | done | Every area the lines inside the lasso close off fills, a pixel under the lines; Target color fills only what the editing layer has in that color, the other layers' lines closing the areas |
| Paint unpainted area | Fill tool | done | Inside the lasso, what the layer left unpainted and no line covers fills |
| Ruby (furigana) | Text tool, Tool Property | done | Over the words, or to their right in vertical text; kept on its words as they change |
| Lasso fill | Figure tool | done | |
| Fill bucket: Close gap, Area scaling | Tool | done | Paint stays inside lines with small breaks, no white edge. A drag fills every area it crosses that is like the one it started in, as one step; in an animation it sees only the frame shown; on a mask being edited it shows or hides the layer |
| Auto Select (magic wand) | Tool | done | Looks at reference layers only when set; Refer multiple with all, selected, folder or reference layers, and an Exclude row of five (draft, text, editing, paper, locked) |
| Shrink selection | Selection area tool | done | The seventh sub tool: a lasso whose selection shrinks to the drawing inside it, keeping the holes it closes in |
| Selection area: Aspect, Anti-aliasing, Adjust angle after fixed | Tool Property | done | Free, Square or a ratio of its own; after the drag the rectangle or ellipse turns with the pointer until a click (Enter keeps it straight, Esc drops it) |
| Gradient tool | Tool | done | Twelve sub tools of Sue's own, Contour line fill among them; an editable bar of stops, gradients from set files and kept in My Gradients; Linear, Circle, Ellipse; Do not repeat, Repeat, Reverse, Do not draw; Step of angle, Where to create (a live gradient layer), opacity and blending mode |
| Figure tool (rectangle, ellipse, polygon, polyline) | Tool | done | Straight line, rectangle, ellipse, filled shapes, polygon (corners and roundness of corner), Polyline and Continuous Curve (a click at each corner; a double-click ends it, the first corner closes it), Curve (drag the ends, move to bend, click), Bezier Curve (click or drag out handles). Shift: 45 degrees, square, circle. Follows the rulers and the perspective ruler's plane; on a vector layer a vector line. Line or fill as one setting is not yet (filled shapes are sub tools of their own) |
| Smart Shape (5.0) | Drawing with any tool | done | Hold the pen still: a line, arc, ellipse, circle or polygon; on or off per sub tool |
| Text tool | Tool | done | |
| Ruler tools | Tool | done | |
| Frame Border tool | Tool | done | |
| Balloon tool | Tool | done | |
| Eyedropper, and Move with hand, zoom and move layer | Tools | done | Click again to step through Move's tools |
| Object tool (Operation) | Tool | done | Vector lines, 3D layers and camera folders in one tool |
| Correct Line and Control Point tools | Tool | done | Reshape, pinch, smooth, join, thicken, redraw, give pen pressure |
| Sub tool library breadth | Tools | partial | Clip Studio's eleven pens and six sketch tools are there; breadth grows next |
| Sub Tool palette: each brush's own stroke preview, Add from downloads and the Downloads folder | Sub Tool palette | done | Add from downloads imports Clip Studio sub tools (.sut), Photoshop (.abr), GIMP (.gbr, .gih) and pictures as sub tools, their tips kept in the materials location; tried on real downloaded files |
| A tap with the pen leaves a dot the size of the brush | Drawing | done | Pens no longer start thin with a mouse either |
| Tool Property: size, opacity, hardness, smoothing, pressure | Palette | done | Changes stick to the sub tool |
| Dynamics by pen pressure, speed and chance, along curves | Tool Property | done | Each with a minimum you choose |
| Tilt dynamics: the pen's lean changes size and opacity | Tool Property | partial | On beside pressure and speed, with a minimum and a curve; a pen without tilt (and a mouse) counts as upright, at the minimum. Tilt arrives under Linux (Wayland and X11); Windows Ink and WinTab bring none yet. Verify on a tablet |
| Starting and Ending: pixels, share of the line, fade | Tool Property | done | Thin the line, fade it, or both, longer when you draw fast |
| Color jitter, per dab or per stroke | Tool Property | done | |
| Stabilizer with Adjust by speed | Tool Property | done | The line never falls far behind, and finishes along the pen's way |
| Correction, Taper and the trailing flick | Tool Property | done | Corners stay sharp, ends thin to a point |
| Brush Size: Specify by size on screen, At least 1 pixel | Tool Sliders | done | |
| Create Brush Tip from Image | Edit > Create Brush Tip from Image | partial | Command exists; verify the dialog |
| Brush tip settings: spray, ribbon, direction from line | Advanced Tool Settings | partial | To verify |
| Watercolor edge, with the 5.1 Vivid style | Tool Property, Layer Property | partial | Layer Property's Watercolor edge with width and darkness; the Vivid style is missing |
| Extract line effect | Layer Property | done | Edge threshold and line width; the drawing never changes |
| Double-sided brush tips | Advanced Tool Settings | missing | |
| Pen Pressure Settings | File > Pen Pressure Settings | done | Draw to fit your hand, Stronger and Lighter |
| Windows Ink and WinTab tablets | Peripherals | done | Huion, XP-Pen and Wacom work either way |
| Linux pen tablets: Wayland (tablet-v2) and X11 (XInput2) | Peripherals | partial | Position, pressure, tilt, barrel buttons and the eraser end; XWayland when a compositor has no tablet protocol. Verify on niri, KDE, GNOME and X11 |
| Eraser end of the pen picks the Eraser | Peripherals | partial | Turning the pen back picks the tool before; between strokes only, and a tool picked by hand stays. Under Linux; Windows does not report the eraser end yet. Verify on a tablet |

### Brush parity by hand (P5-3)

Checked on 2026-11-16 against Clip Studio Paint EX 5.0.4 on Windows,
with the mouse at 100%: a slow stroke (three moves about a second
apart, resting before the lift, so the lift is while still), a fast
drag (a lift while moving) and a tap. Clip Studio's sub tools there
carry their owner's own sizes and settings, which were not changed,
so Sue's side was painted by its engine with the same input at the
width Clip Studio's stroke measured. Pen pressure (light, heavy) and
a flick cannot be made with a mouse and were not compared. The side
by side sheets and width profiles are kept outside the repository.
These rows are notes, not counted in the Progress table.

| Sub tool | Clip Studio, mouse | Sue, mouse | Ends |
| --- | --- | --- | --- |
| G-pen | even width, round ends, a tap is a dot the brush's size | the same widths (36 against 36), round ends, the same dot | never bigger than the body in either |
| Mapping pen | thin, even, blunt ends; the tap was hidden under the cursor in the capture | thin, even, but both ends taper to points | Sue's ends are smaller than the body; Clip Studio's tap not compared |
| Turnip pen | thin, even | the same | same |
| Textured pen | grainy, even, with ragged edges; a tap is a dot about two thirds of the line | even, the grain only a mottle inside the line, its edges smooth (open: Sue's grain does not reach the edge as Clip Studio's does); a tap is a dot the brush's size (Sue's own rule that a tap is never smaller than the line) | never bigger than the body |
| Calligraphy | a flat tip at a slant: ends cut on the slant, a tap a slanted sliver | the same tip, ends and sliver; the ends a little rounder | the slant makes the end columns wider, not the tip |
| Felt pen | light, thin, even | the same | same |
| Brush pen | its tip is a slanted flat oval: blunt slanted ends, no taper with a mouse, a tap a small pale blob | tapers to points at both ends with a mouse (on purpose: the brush pen promises a pointed line with any device); a tap is a full dot | Sue's ends are smaller than the body |
| Every sub tool, a slow mouse stroke with points far apart | a smooth arc that does not quite pass through the points (the owner's sub tools there have stabilization 6) | straight legs between the points, with a turn at each one | open: the check's mouse moved once a second, which no real mouse does. Sue's resampler lays its records straight between the device's samples and the stabilizer only rounds the corners within its window, so the legs stay straight. With a real mouse the turns show only on a very fast curve: an S curve drawn in 0.15 s at 60 samples a second (about 40 pixels apart) shows them, the same curve at 125 samples a second does not (engine renders). Laying the records on a curve through the samples instead would change the engine hash, so it is left for the owner to decide |

Not compared here: the Real G-pen (its owner's size there is far
larger than the canvas row), Rough pen, Milli pen (Sue's Fineliner),
Dot pen, Flat marker, the sketch tools, the watercolor, thick paint
and ink brushes other than the brush pen, the airbrushes, erasers and
decorations. Light and heavy pressure and a flick need a tablet.

## Text

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Click and type on the canvas | Text tool | done | |
| Any installed font | Text tool | done | From `sue_wm::fonts` |
| Vertical text | Text tool | done | In columns |
| Edit the words again | Text tool | done | Click the words |
| Words sit in a balloon's middle | Text tool | done | Click inside the balloon |
| Mixed fonts and sizes in one text layer | Text tool | partial | To verify |
| Letter spacing, line spacing, justification | Text tool | done | Tool Property, also for words being typed; kept on the text layer |
| Font list with previews and recent fonts | Tool Property | partial | To verify |
| Text along a curve or inside a shape | Text tool | partial | Text on Curve done; text that wraps inside a shape to come |
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
| Object tool: loop pick, box with eight handles, Scale and Free transform | Tool | done | Adjust line thickness when scaling; Shift keeps a corner in proportion |
| Recolor, resize and reshape picked lines | Object tool, Tool Property | done | Color wheel or a typed hex code; brush size; a sub tool's brush shape |
| Figures on vector layers, Vector magnet | Figure, pen, pencil, brush | done | Lines, rectangles, ellipses, polylines and curves become vector lines with the sub tool's brush; Vector magnet joins drawn ends to nearby ends |
| Vector eraser on several layers | Tool | done | Every picked vector layer at once, one undo step |
| Fill up to vector path | Fill tool | done | Fills in under vector lines to their middle |

## 3D

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| 3D layers with primitives | Layer > New Layer > 3D Layer | done | Cube, sphere, cylinder, cone, plane |
| OBJ import | File > Import > 3D Data | done | |
| Camera: drag to turn, Shift to move, Ctrl to come nearer | Object tool | done | |
| Light and shading settings | Tool Property | done | Shaded, line art or both |
| Convert 3D to Lines and Tones (EX) | Layer > Convert to Lines and Tones | done | Clean line art and tone to draw over |
| Drawing figures with poses and pose materials | Materials, 3D | done | Preset poses for the figure, hand and head; any pose kept as a pose material and used again |
| 3D head models (5.1) | 3D | partial | A posable head (neck, skull, jaw, nose, brow, eyes, ears) with seven presets; simpler than Clip Studio's, no face shapes to edit |
| 3D hand models and Hand Scanner posing (5.0) | 3D, Materials | partial | A posable hand of sixteen joints with seven presets, and the Hand Scanner (MediaPipe's twenty one points, in depth too); one right hand, so a left hand poses as its mirror |
| Painting directly on 3D (5.0, 5.1) | 3D | missing | |
| VRM import and photorealistic rendering | File > Import | missing | |
| Height-based fog, Follow camera light (5.0, 5.1) | 3D settings | done | In the 3D layer's Tool Property |
| Object position, angle and scale with resets, preset angles, Put on the ground | Object tool, Tool Property | done | Kept on the model; buttons in Tool Property move, turn and slide the object along the ground while dragged |
| Camera buttons: turn, move, zoom | Above the model | partial | As drag buttons in Tool Property (arrow keys too); not drawn on the canvas above the model yet |
| Manga Perspective | Tool Property | done | On and off with a strength; the middle keeps its size |
| Body types, body shape chart per body part, head to body ratio following the height | Tool Property, Body shape | done | Male and female; thin to fat and less to more muscle for the full body, head, upper body, arms and legs |
| Register body shape as material, Initialize | Tool Property | done | Body shape materials (`.body`, docs/body-shape-files.md) beside pose materials |
| Object list of several objects on one 3D layer | Tool Property | missing | One object per 3D layer |
| Display settings for editing | Tool Property | missing | Clip Studio's draw quality while a 3D object is edited; Sue Paint draws the model at full quality on every frame of a drag, so there is nothing to lower yet |
| Hand pose on the figure itself, poses dragged onto the model, the 3D object launcher | 3D | missing | Hands are their own 3D layer; poses go on by Paste from the Material palette |
| Characters with physics, 3D backgrounds, motion materials | Materials | missing | Later (risk R6: Sue's own materials only) |
| Batch texture export (EX) | File > Export | missing | |
| FBX import (5.1) | File > Import | missing | |

## Colors

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Color Wheel | Palette | done | |
| Color Slider | Palette | done | |
| Color Sets: Default, Grays, Skin Tones, Pastels, your own | Palette | done | Named sets and colors, empty cells, rename, reorder, Add Standard Set; import a GIMP palette, a Clip Studio Paint `.cls` or a list of color codes, export a GIMP palette (`docs/paint-color-sets.md`) |
| Color History | Palette | done | |
| Intermediate Color | Palette | done | Four colors at its corners |
| Approximate Color | Palette | done | The shades near your color |
| Main and sub color, transparent color, swap | Palette, keys | done | X switches between the main and the sub color, the arrow button swaps them, C takes the transparent color |
| Pick from the picture (Alt, right click) | Canvas | done | |
| Sub View reference pictures pick colors | Sub View palette | done | |
| Lab color space (5.1) | Color Slider | missing | |

## View and navigation

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Zoom: in, out, 100%, 200%, Fit to Screen, Print Size | View | done | |
| Reset Display, Fit to Navigator | View | done | |
| Rotate and flip the view | View > Rotate/Flip | done | Tilting never changes the picture |
| Scroll bars, with zoom and turn controls | View > Scroll Bar | done | |
| Ruler bar and grid, Pixel Grid | View | done | |
| Grid Settings (gap, divisions, origin) | View > Grid Settings | done | Saved with the picture; Snap to Grid follows the finest lines |
| Crop marks, inner border and bleed | View | partial | Commands exist; verify they draw on the canvas |
| Show Tone Area, Paper, Page Image | View | partial | Commands exist; verify |
| Selection, Transform, Text and Object launchers | View | done | Small bars of buttons under what you work on |
| Color profile preview | View > Color Profile | partial | Commands exist; verify |
| Safety Margin, Show Vector Paths, Show Playback FPS, Toggle Special Ruler Snapping | View | done | The margin runs 5 mm inside a page's trim; the rate is what the canvas really shows; the snapping switch is kept with the picture and undoes |
| Tabs show a dot when a picture changed | Canvas tabs | done | |
| Next and previous canvas | Window > Canvas | done | Ctrl+Tab |
| Touch: two fingers move, zoom and turn; two-finger tap undoes | Gestures | done | A pinch never paints and never undoes |

## Window, palettes and workspaces

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Clip Studio's default frame | Workspace | done | The "Clip Studio" layout style: Sub Tool, Tool Property and Brush Size stacked over the six color palettes at the left, Navigator, Layer Property and Layer at the right, the Timeline folded under the canvas, Material in the strip |
| Layout styles | (Sue's own) | done | Window, Layout Style and Settings; "Clip Studio" is built, "Sue Paint" is listed grayed out for later; each style keeps its own layout and workspaces |
| Palette docking, floating windows, folding into the strip | Palette menus | done | The menu also docks at the top or bottom and in a new column on either side; floats sit over the canvas |
| Pop Out into Separate Window | Palette menus | done | A window of its own that goes anywhere, outside Sue's window too |
| Dock at the top, beside the command bar | Palettes | done | Drop a palette on the canvas's top edge, or the menu's Dock at the Top |
| Every palette a palette of its own | Palettes | done | Each opens as itself from the Window menu, none shows in two places, a tab drags off its group; closing one (its menu, or a floating palette's X) hides it for good until the Window menu shows it again |
| Palette menu at the top left with the palette's commands and Hide X Palette | Palette menus | done | Layer and Timeline have their commands; the other palettes' commands come with their own work |
| Roll a floating palette up by a double-click | Palettes | done | |
| Display palette as tab or as pop-up (strip icons) | Palettes | done | Right-click an icon of the strip |
| Command Bar and Tool palette move and resize like any palette | Palettes | done | The tool palette grows more columns as its column widens and lies on its side in a row |
| Drag a palette by its title bar, tab or icon to dock it left, right or under the canvas, beside another or in a column of its own, or to float or fold it | Palettes | done | The shared dock in `sue-wm` (`sue_wm::dock`) lights up where it will go |
| Palette groups: palettes stacked as tabs, reordered by dragging a tab | Palettes | done | Drop a palette onto another's middle or onto a tab |
| The palette layout kept between runs | Workspace | done | `layout.txt` in Sue Paint's data folder, never in a document; Settings, Remember the palette layout |
| Drag column widths and palette heights | Dock | done | Also the widths in the row under the canvas and its height; narrow windows fold the widest palettes by themselves |
| Use Current Workspace as Default, Reset to Default | Window > Workspace | done | Reset goes back to the workspace you chose as the default, and again to Sue Paint's own |
| Register and Manage Workspaces | Window > Workspace | done | Register, switch, rename and delete; layouts saved before tabs still open; the one in use is checked, Reload puts it back; Illustration and Animation ship with Sue |
| What a workspace keeps | Register Workspace | partial | Palettes, the menu bar, the Command Bar and Quick Access, the keyboard shortcuts and the unit of lengths, as the dialog says; not yet the tool order |
| Hide All Palettes, Hide Menu Bar | Window | done | Tab and Shift+Tab; Hide All hides every palette but the command bar and shows them back as they were, reading Show All Palettes meanwhile |
| Command Bar | Window > Command Bar | done | |
| Palettes: Tool, Sub Tool, Tool Property, Brush Size, Tool Sliders | Window | done | The names the tutorials show; the Window menu reads each palette's own title, in Clip Studio's order and groups, and Tool Sliders is Sue's own |
| Palettes: Color Wheel, Color Slider, Color Set, Color History, Intermediate Color, Approximate Color | Window | done | |
| Palettes: Navigator, Sub View, Layer, Layer Property, History | Window | done | |
| Palettes: Timeline, Animation Cels, Material | Window | done | |
| Auto Action palette (recorded sequences) | Window > Auto Action Palette | partial | Records and plays menu commands; it does not record tool strokes as Clip Studio's can |
| Item Bank palette | Window > Item Bank | done | Keeps sub tools to add again |
| Information palette | Window > Information | done | The canvas, the memory its pixels take, the pointer's place and color, a drag's length, in the unit chosen; its menu shows or hides each, so it can be emptied into a spacer |
| Cascade, Tile, New Window | Window > Canvas | done | New Window adds a tab and leaves tiling alone; a canvas can also go into a window of its own (tear its tab off, or Canvas in Its Own Window), on any screen |
| Interface: Automatic, Desktop, Tablet, Phone | Window > Interface | done | In the Sue menu and Settings, so Window has one layout menu, Layout Style; the command palette finds the choices too |
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
| Pose, 3D and texture material kinds | Palette | partial | Poses done, each with a picture of it; 3D models and textures as materials to come |

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
| Preferences dialog | File > Preferences | done | Opens Settings on the Sue Paint page, in named groups; only options that work are shown, and Clip Studio's option names find them in the filter |
| Shortcut Settings | File > Shortcut Settings | done | Settings, Keyboard Shortcuts: grouped by menu and Tools, any command can get a key, a key held twice is marked on both rows with an offer to take it from the other; the menus show what you chose |
| Modifier Key Settings | File > Modifier Key Settings | done | General or per tool rows for keys on the canvas, the wheel, right and middle click; a file people share (docs/paint-modifier-keys.md); no touch rows |
| Command Bar Settings | File > Command Bar Settings | partial | Command exists; verify |
| Pen Pressure Settings | File > Pen Pressure Settings | done | |
| Privacy Settings | File > Privacy Settings | missing | |
| Shortcut list | Help, F1 | done | Alt+I lists them all as set |
| Interface strings in many languages | (Clip Studio ships localized) | done | `sue-lang`, English first |

## File formats

| Feature | Clip Studio | Sue | Notes |
| --- | --- | --- | --- |
| Sue project (.sue) | (Sue's own) | done | |
| Clip Studio (.clip) export | File > Export Back to the Same .clip, Save Duplicate | partial | Layers, folders with blend mode and clipping, masks, paper, draft and reference marks, the layer color effect; a layer not changed in Sue goes back exactly as it was read. Fill, gradient, text and vector layers reach Clip Studio as their pixels, correction layers as empty layers, and effects, palette colors, rulers, comps and the timeline not at all yet; Sue keeps all of them in its own tables of the file and opens them live again. No hidden copy of the Sue project any more. Every test file opens in Clip Studio Paint 5.0.4 with its layers, folders and paper; what shows there is in `docs/clip-sue-tables.md` |
| Clip Studio (.clip) open | File > Open | partial | See File: open, for what arrives and what does not |
| Photoshop (PSD, PSB) save | File > Save Duplicate | done | |
| Photoshop open | File > Open | partial | A reader exists; verify what it keeps |
| BMP, JPEG, PNG, TIFF, Targa, WebP | Save, Export | done | |
| JPEG XL | (Sue adds it) | done | Sue's default for its own image caches |
| QOI, GIF (flat) | Save, Export | done | |
| PDF, EPUB (EX) | Export multiple pages | done | |
| Animated GIF, MP4 | Export animation | done | |
| APNG, Animated WebP | Export animation | done | |
| OpenToonz scene | Export animation | missing | |
| OBJ (3D import) | File > Import > 3D Data | done | |
| FBX (5.1) | File > Import | missing | |

## Sue's own, beyond Clip Studio

| Feature | Sue | Notes |
| --- | --- | --- |
| Projects with labels, search and favorites | done | In the Hub's Files, for every studio's work; Sue Paint has no start page of its own since 0.0.8 |
| Materials as a place of the Hub | done | Phones and tablets have it in the Hub's tab bar; with no canvas Sue Paint shows its workspace with New and Open |
| Simple Mode for phones and tablets | done | Studio Mode for the desktop layout |
| Studio Mode and touch layouts, modifier keys on a slide-out | done | |
| One project holds Paint with every studio | done | Entities by EntityId in the `.sue` file |
| Send To between studios | done | Work moves by EntityId, never by exporting and importing |
| Command button (the palette of commands) | done | In the status bar, since Tab belongs to the palettes |

## Not built on purpose

These are left out for good, so they have no row above and nobody
should build them from this list:

- Browsing or downloading from Clip Studio's online material store, in
  any form: no page of it inside Sue, no button that opens it, no reader
  of its site. Sue 0.0.8 removed Browse Assets and the Download button
  of the tool palette for this reason.
- Downloading a brush or a picture from a link. Brushes and pictures
  that are already on the device are imported instead (Sub Tool
  palette, the Hub's Materials place).
- Anything that needs a Clip Studio account, its cloud files among
  them, and anything for Clip Studio's own hardware: no menu item, no
  settings page.

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
7. Materials: user tags, pose and 3D materials.
8. Text depth: mixed fonts and sizes in one layer, spacing and
   justification.
9. The verify rows: every row whose note says verify, checked against
   the running app and settled.
10. Print: the print dialog and settings, then multiple pages and CMYK
    PDF.

Then the rest, row by row.
