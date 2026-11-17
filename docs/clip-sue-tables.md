# Sue's own tables inside a `.clip`

A `.clip` file holds an SQLite database. Besides the tables Clip Studio
Paint reads, Sue writes a few tables of its own into it. Clip Studio
ignores tables it does not know, so a file stays a `.clip` that Clip
Studio opens and shows by its pixels; Sue reads its own tables back and
gets each layer as it was made: a fill stays a fill, text stays text,
a timeline stays a timeline.

Sue keeps its own tables for everything whose storage in Clip Studio's
tables has not been observed yet. Where it has been (fill layers,
linear gradients, text, vector lines, palette color and the layer
color's second color; see "What Clip Studio sees" below), Sue writes
Clip Studio's own storage too, and its own part as well, for what Clip
Studio has no place for. Whatever Clip Studio itself wrote in a file
(its own tables, columns and chunks) is kept untouched beside them and
written back.

This page describes only Sue's tables. Anyone may read or write them.

## The tables

| Table | Columns | What a row is |
| --- | --- | --- |
| `SueProject` | `Version INTEGER, Data BLOB` | The whole picture as a `.sue` project, in files older Sue versions wrote. |
| `SueLayer` | `LayerId INTEGER, Draft INTEGER, Reference INTEGER` | A layer marked draft or reference (1 or 0). |
| `SueLayerPart` | `LayerId INTEGER, Kind TEXT, Version INTEGER, Data BLOB` | One part of one layer. |
| `SueDocumentPart` | `Kind TEXT, Version INTEGER, Data BLOB` | One part of the whole document. |

`LayerId` is the `MainId` of the layer's row in Clip Studio's `Layer`
table.

A reader takes a part row only when all of these hold; otherwise it keeps
the row as it is and writes it back unchanged:

- `Kind` is a name it knows and `Version` is a version it reads (every
  part below is version 1);
- `Data` is laid out exactly as below, with no byte left over;
- the layer it names exists, and has no part of the same kind yet (a
  layer has at most one content part);
- no other column of the row holds anything.

So a newer Sue can add part kinds or versions, and an older Sue keeps
them without understanding them.

## How numbers are written

Each part's `Data` is a run of these, in the order given:

| Name | Bytes |
| --- | --- |
| byte | one byte, 0 to 255 |
| flag | one byte, 0 or 1 |
| number | an unsigned LEB128 varint |
| signed | a zigzag LEB128 varint |
| count | a number: how many items follow |
| float | an IEEE 754 single, little-endian |
| double | an IEEE 754 double, little-endian |
| text | a count of bytes, then UTF-8 |
| bytes | a count of bytes, then the bytes |
| point | two floats: across, down, in canvas pixels |
| points | a count, then that many points |
| color | four bytes: red, green, blue, alpha (straight) |
| stops | a count, then per stop a float (0 to 1) and a color |
| path | a count, then that many numbers: where a layer is in the tree, as its index among the root's layers (bottom first), then among that folder's layers, and so on |

A byte of bits uses only the bits named for it, and a choice only the
values listed. A reader that meets another bit or value, or bytes left
after the part, does not read the part and keeps its row as it was.

## Layer parts

### Content parts

A layer has at most one. Its pixels in Clip Studio's tables are what
Clip Studio shows for it. A `frame`, `animation-folder` or `camera`
layer is a folder.

| Kind | Data |
| --- | --- |
| `fill` | color |
| `gradient` | byte shape (0 linear, 1 circle, 2 ellipse), point start, point end, float ellipse height over width, byte past the ends (0 the end colors go on, 1 repeat, 2 reflect, 3 nothing), stops |
| `correction` | byte kind, then its settings (below) |
| `text` | a text (below) |
| `vector` | count of lines; per line: count of points, per point a point, float pressure (0 to 1), float width (times the line's), float opacity (0 to 1) and flag corner; then float width in pixels, color, text brush name, flag closed |
| `frame` | count of panels; per panel: points of its outline, float border width, color |
| `balloon` | byte shape (0 ellipse, 1 rectangle, 2 round rectangle, 3 cloud, 4 flash), four floats left, top, width, height, count of tails (per tail: points from the balloon to the tip, float width), float line width, color line, color fill, flag has text, then a text when it has one |
| `3d` | bytes: Sue's own 3D scene, opaque here |
| `animation-folder` | count of cels; per cel: number frame (from 0), number child (0 for a blank cel, else the index of the folder's layer it shows plus 1) |
| `camera` | point center, float zoom (1 for none), float turn in radians |

Correction kinds and their settings:

| Byte | Kind | Settings |
| --- | --- | --- |
| 0 | Brightness and contrast | float brightness, float contrast (each -100 to 100) |
| 1 | Hue, saturation, luminosity | float hue (-180 to 180), float saturation, float luminosity (-100 to 100) |
| 2 | Posterization | number levels |
| 3 | Reverse gradient (invert) | nothing |
| 4 | Level correction | point input black and white, float gamma, point output black and white (0 to 255) |
| 5 | Tone curve | four points lists: all channels, red, green, blue (0 to 255 each way) |
| 6 | Color balance | nine floats: shadows, midtones, highlights, each cyan-red, magenta-green, yellow-blue (-100 to 100); flag keep luminosity |
| 7 | Binarization | float threshold (0 to 255) |
| 8 | Gradient map | stops |

A text is: count of runs (per run: text, text font family, float size in
points, color, byte style (the bit worth 1: bold, 2: italic)), point top
left corner, flag has a box, point box size when it has one, flag
vertical, byte alignment (0 start, 1 center, 2 end, 3 justify), float line
spacing (1 for the font's own).

### Other layer parts

| Kind | Data |
| --- | --- |
| `effects` | byte which effects follow (the bit worth 1: border, 2: tone, 4: extract line, 8: layer color sub color), then for each present, in that order: border float width and color; tone float lines per inch, float angle in degrees, byte dot (0 round, 1 square, 2 diamond, 3 line, 4 noise); extract line float threshold; sub color three bytes red, green, blue |
| `palette-color` | three bytes: red, green, blue |
| `rulers` | count of rulers; per ruler: byte kind (0 linear, 1 curve, 2 figure, 3 parallel, 4 concentric, 5 radial, 6 symmetry, 7 perspective), points (vanishing points for a perspective), number lines (for symmetry), byte flags (the bit worth 1: snaps, 2: shown on every layer) |
| `thumbnail` | a small picture (below) |

A small picture is: number width, number height (each at most 4096),
bytes holding its straight RGBA pixels, row by row, zlib compressed.

## Document parts

| Kind | Data |
| --- | --- |
| `comps` | count of layer comps; per comp: text name, count of layers, per layer a path, flag shown, float opacity (0 to 1), signed blend number (Clip Studio's numbering) |
| `timeline` | text name, float frames per second, number frame count, number first and number last frame played; count of tracks (per track: path of the layer, text property name, count of keyframes, per keyframe number frame, double value, byte how it goes on: 0 hold, 1 linear, 2 smooth); count of sounds (per sound: text name, number start frame, float volume, flag muted, bytes the sound file as imported) |
| `preview` | a small picture of the whole canvas |

Track property names are Sue's: `opacity`, `x`, `y`, `scale`, `angle`,
and `camera.x`, `camera.y`, `camera.scale`, `camera.angle` for the
camera.

How Sue Paint fills these in (since 0.0.8):

- A layer's `angle` is in degrees, clockwise; `center.x` and
  `center.y` are the point it turns and scales about. `opacity` is 0
  to 1, `scale` 1 for as it is, `x` and `y` in canvas pixels.
- The camera may also have `camera.pivot.x`, `camera.pivot.y` (its
  centre of rotation) and `camera.opacity`; `camera.angle` is in
  radians.
- `shown` says on which frames a layer shows at all: its keys hold
  (each is "hold"), 1 shows the layer from that frame on, 0 hides it.
  A layer with no `shown` track shows on every frame.
- A sound's file is a WAV of 32-bit float samples, as Sue holds sound.
- A vector line's brush name is Sue's brush as `key=value` pairs
  joined by `;` (the tip a vector line keeps), so Sue gets the same
  brush back; another program may leave it empty.
- Rulers hang on the layer they belong to, or on the bottom layer when
  they belong to the whole picture (then each is shown on every
  layer). A parallel ruler's two points give its direction, and a
  symmetrical ruler's two points lie on its mirror line.

## What Clip Studio sees

Clip Studio reads its own tables only. Since 0.0.8 Sue also writes, in
Clip Studio's own storage, the parts of a layer whose storage was seen
in files Clip Studio made:

| Part | Written as Clip Studio stores it |
| --- | --- |
| `fill` | a fill layer: `LayerType` 2, a white mask, the color in `GradationFillInfo` with its plain color on |
| `gradient`, linear and holding its end colors | a gradient layer: `LayerType` 2, a white mask, the stops and the line in `GradationFillInfo` |
| `text` | a text layer: `LayerType` 0, the words in `TextLayerString`, the font, size, color and place in `TextLayerAttributes` (one style for all the words) |
| `vector` | a vector layer: `LayerType` 0, a `VectorObjectList` row whose strokes (points, width, taper, color, opacity) sit in a chunk of their own, drawn with a plain pen Sue adds to the document's brush tables once |
| `palette-color` | `LayerUsePaletteColor` and the three palette color columns |
| `effects`, the second color of the layer color | the three `DrawColorSub` columns |

Sue's own part is written beside each, so Sue gets back what Clip Studio
has no place for (a text's runs, bold and italic, its box, alignment and
line spacing; a vector line's own brush, pressure and corners). When a
file is opened, a layer whose Clip Studio storage says something other
than Sue's part (it was changed in Clip Studio after Sue wrote it) is
read from Clip Studio's storage; a file Clip Studio saved, which has
lost Sue's tables, is read from Clip Studio's storage alone.

For any other content part (a circle or ellipse gradient, a gradient
that repeats, a correction, a frame, a balloon, a 3D layer, an animation
folder, the camera), Clip Studio sees a raster layer (a folder for
`frame`, `animation-folder` and `camera`) holding the pixels Sue drew for
it. Rulers, the other effects, comps and the timeline are not live in
Clip Studio until Sue writes them in Clip Studio's own storage, which
needs that storage observed first.

Whether Clip Studio opens the layers Sue writes in its own storage the
way the survey's files open is not yet checked by hand: the test files
are written by `write_native_kinds_for_the_hands_on_check` (run with
`SUE_CLIP_INBOX`).

## Opened in Clip Studio (checked 2026-11-16)

The P5-1 test files (`write_files_for_the_hands_on_check` and
`write_kinds_for_the_hands_on_check`, run with `SUE_CLIP_HANDS_ON`)
were opened in Clip Studio Paint EX 5.0.4 on Windows, by passing each
file to the program; nothing was saved over them.

Found and fixed in Sue on the way:

- Every file with a folder (a plain folder, an animation folder, a
  camera folder) closed Clip Studio at once, with no message. A folder
  in a `.clip` needs a bitmap, an empty one at the least; Sue wrote
  folders with none. The writer now gives such a folder an empty
  bitmap.
- The paper showed as a clear canvas. Clip Studio draws a paper layer
  as its bitmap seen through the paper color (a light pixel comes out
  lighter than the color, a clear one shows nothing), so Sue now
  writes the paper's bitmap opaque black, which shows exactly the
  paper's color. Checked with red, cream and white paper.

With those fixes, what Clip Studio shows:

| File | Opens | What shows |
| --- | --- | --- |
| 01-fill | yes | `Fill` as a raster layer at 50%, the disc bluish through it, white paper |
| 02-gradient | yes | `Gradient` as a raster layer holding the gradient as Sue drew it |
| 03-corrections | yes | `Posterize` and `Hue` as empty raster layers; the orange and the dark disc unchanged |
| 04-text | yes | the words as pixels on a raster layer `Sue Paint` |
| 05-vector | yes | the bent black line and the red line as pixels on `Vector` |
| 06-effects | yes | the layer color effect is on (Clip Studio's own setting): the dark disc shows blue, lighter than the set color because Clip Studio mixes the layer color by the pixel's lightness; the border, palette color and tone do not show |
| 07-comps | yes | the layers as usual, no layer comps |
| 08-rulers | yes | the layers as usual, no rulers |
| 09-everything | yes | every layer and folder; the animation folder and the camera folder as ordinary folders (`Folder 1`, `Camera 1`), the three cels inside the first; no timeline |
| 10-bouncing-ball | yes | the five cels as raster layers in an ordinary folder, all shown at once; the sketch at 30%; no timeline |
| sue-picture | yes | folders with their blend mode and opacity, the clipped layer, the glow mode, a mask, and the draft and reference layers |
| raster-subset-through-sue | yes | every layer and the 27 blend layers; a clear area at the top left, where the generated `Gray` layer lies (300 by 200 at 5, 7). That layer goes back unchanged, so the file still holds it as a one channel bitmap, which the test kit builds by itself and which was never seen in a file Clip Studio saved; Sue reads it as opaque gray. Open: either a gray layer is not stored that way by Clip Studio, or Sue reads such a layer wrong. Check with a gray layer made in Clip Studio |

Each file also has the layer `Layer 1` that a new Sue picture starts
with. Clip Studio's own Layer Property palette was not opened (the
check could click but not type), so whether it lists the layer color
as on was read from the canvas and the layer's icon only.
