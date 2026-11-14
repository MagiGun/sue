# The stored stroke format

A stroke that Sue may paint again (a line on a vector layer, a
timelapse, a stroke made again after its stabilization changed) is kept
as a *stored stroke*: everything the brush engine (`crates/sue-brush`)
needs to paint it again to the same bytes, and nothing it can work out
for itself. This page describes what a stored stroke holds, its bytes,
how a vector layer keeps it, and the promise that comes with it.

## What a stroke keeps

| Field | What it is |
| --- | --- |
| `engine` | The version of the brush engine that painted it. `0` is the legacy engine of Sue 0.0.75 and before; `1` is the engine of Sue 0.0.8. A stroke is painted again only by the code of its own version. |
| `brush` | The SHA-256 of the brush's bytes (`BrushPreset::to_bytes`, the component `sue.brush`). The document keeps each brush once, beside its strokes; a stroke never changes when the brush in the library is edited later. The brush holds the finish settings too (post correction, flick, stabilization). |
| `seed` | The seed of the stroke's chances. Every random choice (scatter, spray, the pick of a tip picture, color jitter, the `random` input) is a counter hash of this seed and a dab's number, never of where the stroke lies, so a stroke moved elsewhere keeps its look. |
| `device` | A pen (pressure comes with its samples), or a device that presses the same all along (a mouse at 1). |
| `curve` | The person's own pressure curve when the stroke was drawn, as its control points. |
| `scale` | Screen points per layer pixel when it was drawn (the zoom), which the stabilizer's reach and a size measured on the screen depend on. |
| `step` | The arc length between two stroke points, when it was set; absent, the engine's own choice. |
| `colors` | The main color and the sub color, straight sRGB from 0 to 1. |
| `samples` | The pen's raw samples, in layer pixels, with their device time in seconds counted from the landing: a landing, moves, and the lift. Each may say its pressure, its tilt (`[x, y]`, each -1 to 1) and its twist (degrees), or not. Raw, not smoothed: the stabilizer runs again on replay, so it can be changed after drawing. |
| `copies` | The copies a symmetry ruler made, each six numbers `a b c d e f` mapping `(x, y)` to `(a x + b y + e, c x + d y + f)`. Absent for a stroke drawn once. Where copies overlap they lay paint once. |

## The bytes

The bytes lead with the format number (`1`, a variable-length unsigned
integer), then the number of fields, then each field as its name (a
string) and its bytes (a length and the bytes). Numbers are little
endian; `f32` and `f64` are IEEE 754. The fields in format 1:

| Name | Bytes |
| --- | --- |
| `engine` | a variable-length unsigned integer |
| `brush` | 32 bytes |
| `seed` | `u64` |
| `device` | `u8` 0 for a pen; `u8` 1 and an `f32` pressure for a constant device |
| `curve` | the number of points, then each point's `f32` x and y |
| `scale` | `f32` |
| `step` | `f32` (absent: none set) |
| `colors` | six `f32`: the main color's red, green and blue, then the sub color's |
| `samples` | the number of samples, then for each: `u8` kind (0 landing, 1 move, 2 lift), `u8` flags (1 pressure, 2 tilt, 4 twist), `f32` x, `f32` y, `f64` time, then the parts its flags name: `f32` pressure, two `f32` for the tilt, `f32` twist |
| `copies` | a list of `f32`, six for each copy (absent: drawn once) |

A field this build does not know is kept as it was read and written
back unchanged, after the known ones. A format newer than the build's is
refused rather than misread. `engine`, `brush` and `samples` must be
there. The same stroke always gives the same bytes.

`crates/sue-brush/tests/golden/stroke-v1.bin` is a stroke written by
format 1, and a test keeps it reading.

## On a vector layer

A vector layer keeps its lines as geometry (`crates/sue-picture/src/vector.rs`):
one polyline a line, its points at the samples. A line of the legacy
engine keeps what Sue 0.0.75 kept (`pressure`, `speed`, `tilt`, `lean`,
`width`, `alpha` and `mark` on its points; `color`, `sub`, `flick`,
`flicked`, `correction`, `single`, `tip` and `copies` on the line).
Lines of the new engine add, on the line:

| Attribute | What it is |
| --- | --- |
| `engine` | the engine version; absent or `0`: the legacy engine |
| `seed` | the seed, as decimal text |
| `brush` | the brush's SHA-256, as 64 hexadecimal digits |
| `device` | `-1` for a pen, else the constant pressure |
| `scale` | the zoom when drawn |
| `curve` | the pressure curve's points as text, `x,y x,y ...` |

and on its points: `time` (seconds from the landing), `pressure`,
`lean` (the tilt, `[x, y]`) and `twist`, a part that was not said kept
as not a number. The kind of each sample follows from its place: the
first is the landing and the last the lift.

The layer keeps the brushes its lines name once, in the detail
attribute `brushes`: each brush's bytes as hexadecimal text. A brush's
tip pictures and papers that the brush names by hash are kept the same
way in the detail attribute `pictures`, each as `hash width height`
and the picture's 8-bit coverage in hexadecimal, so a line paints the
same without the library it was drawn from.

Old files load unchanged: a file without these attributes holds legacy
lines only, which keep their stored pixels and are painted again by the
frozen legacy engine when they must be (the vector eraser, Correct Line,
a move). The action "Update lines to the new brush engine" turns the
legacy lines of a layer into lines of the new engine, painted again, as
one step that undo takes back.

## The promise

A stored stroke painted again gives the same bytes as the stroke drawn
live, whatever the thread count, frame rate or zoom of the moment, and
on every platform: no value that reaches a pixel goes through the
system's math library (`sin`, `cos`, `exp`, `ln`, `pow` and the rest are
the engine's own, `crates/sue-brush/src/math.rs`, made from the
operations IEEE 754 rounds exactly), and a test reads the engine's
sources to hold that. How a live stroke must be driven for this to hold
is in the documentation of `sue_brush::replay`.

A test paints a fixed set of strokes and compares the hash of their
bytes with the one committed for the engine version in
`crates/sue-brush/tests/golden/engine-hash.txt`. It is recorded on
Windows; Android compares it in Phase 7 of the 0.0.8 plan and the other
platforms in Phase 8. Until the 0.0.8 release the brush steward may
record it again after looking at the stroke sheets; from the release on,
a change to anything that reaches a pixel needs a new engine version.
