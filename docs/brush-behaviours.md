# Brush behaviours: how a brush stores them

A Sue brush is a `BrushPreset` (the component `sue.brush`, described in
`HUMANZ.md`). Its tip, size, dynamics, tapers, color jitter, mixing,
blend mode, erase and build-up are the brush model's own fields. Everything
else that makes one brush paint unlike another (Clip Studio Paint shows it
as the spraying effect, stroke, texture, dual brush, watercolor edge and
correction pages of a sub tool) is stored by the paint behaviours of the
brush engine (`crates/sue-brush/src/paint/settings.rs`) inside the same
preset, so a brush is still one file a person can make, share, install and
remove.

Two kinds of entries are used:

- **Parameters**: numbers a pen can drive. They sit in the preset's
  parameter list beside `size` and `opacity`, so a binding can make any of
  them follow pressure, speed, tilt, direction, random and the other
  inputs. A build registers them with `sue_brush::paint::register`.
- **Fields**: switches and choices, stored as tagged fields of the preset.
  A flag is one byte, 0 or 1. A choice is its name as UTF-8 text. A Sue
  that does not know a field keeps it and writes it back unchanged, and a
  choice whose name a Sue does not know reads as the default and stays in
  the preset.

A name, once given, is never taken back or given another meaning.

## Parameters

| Key | Unit | Default | Range | Meaning |
| --- | --- | --- | --- | --- |
| `paint.paint_density` | share | 1 | 0 to 1 | Density of paint: how fully a mixing brush's paint covers what is under it |
| `paint.amount_of_paint` | share | 1 | 0 to 1 | Multiplies the brush's amount of paint (`mix.load`), so a pen can drive it |
| `paint.color_stretch` | share | 1 | 0 to 1 | Multiplies the brush's color stretch (`mix.pull`) |
| `paint.particle_size` | pixels or share | 0.15 | 0 to 2000 | A sprayed particle's diameter: pixels, or a share of the brush size when `paint.spray_linked` is on |
| `paint.particle_density` | count | 12 | 1 to 256 | Particles in one spray dab (stored with the unit "pixels", as the model has no unit for a count) |
| `paint.spray_deviation` | share | 0 | 0 to 1 | 0: particles spread evenly over the brush; 1: gathered close to its middle |
| `paint.spray_rate` | per second | 60 | 0 to 1000 | Dabs a second while the pen holds still, with continuous spraying |
| `paint.dual_size` | pixels or share | 0.5 | 0 to 2000 | The second tip's size: pixels, or a share of the main size when `paint.dual_linked` is on |
| `paint.dual_spacing` | sizes | 0.5 | 0 to 100 | The gap between the second tip's dabs |
| `paint.dual_scatter` | sizes | 0.5 | 0 to 100 | How far the second tip's dabs fall from the line |
| `paint.watercolor_range` | pixels | 6 | 0 to 200 | How far in from the rim the watercolor edge reaches |
| `paint.watercolor_opacity` | share | 0.55 | 0 to 1 | How opaque the inside of the stroke stays |
| `paint.watercolor_darkness` | share | 0.35 | 0 to 1 | How much darker the rim gets |
| `paint.watercolor_blur` | pixels | 1 | 0 to 50 | How far the rim is softened |
| `paint.tone_cell` | pixels | 6 | 2 to 200 | The width of one halftone cell |
| `paint.tone_angle` | degrees | 45 | -360 to 360 | The halftone screen's angle |
| `paint.texture_brightness` | share | 0 | -1 to 1 | Added to the paper's heights |
| `paint.texture_contrast` | share | 0 | -1 to 1 | -1 flattens the paper, 1 makes every height a valley or a high spot |

The texture's strength (Clip Studio's "texture density") is the brush
model's own parameter `texture_strength`.

## Fields

| Field | Kind | Values | Meaning |
| --- | --- | --- | --- |
| `paint.tip_order` | choice | `repeat`, `back-and-forth`, `clamp`, `random` | How a tip of several pictures takes them, dab by dab |
| `paint.flip_x`, `paint.flip_y` | choice | `off`, `on`, `random` | Flip the tip across, and up and down |
| `paint.gap_density` | flag | | Build-up dabs keep their density however close they fall |
| `paint.tip_blend` | choice | `over`, `darken` | How the dabs of a stroke whose dabs differ in color meet |
| `paint.spray` | flag | | The spraying effect is on |
| `paint.spray_linked` | flag | | The particle size is a share of the brush size (on when absent) |
| `paint.spray_direction` | choice | `brush`, `line`, `random` | Which way particles turn |
| `paint.continuous` | flag | | Continuous spraying |
| `paint.texture_paper` | text | `grain`, `canvas`, `cold-press`, `rough` | Which of Sue's own papers, when the model's texture source is `grain` |
| `paint.texture_mode` | choice | `multiply`, `subtract`, `height`, `darken`, `overlay` | How the paper meets the stroke; the model's own `texture.mode` holds the nearer of its two names for an older Sue |
| `paint.texture_invert` | flag | | High spots become valleys |
| `paint.texture_emphasize` | flag | | Emphasized density: light strokes show more of the paper |
| `paint.texture_per_dab` | flag | | The paper is laid on each dab (Clip Studio's "apply by each plot") |
| `paint.dual` | flag | | The dual brush is on |
| `paint.dual_mode` | choice | `multiply`, `darken`, `subtract`, `overlay`, `height` | How the second tip meets the first |
| `paint.dual_linked` | flag | | The second tip's size follows the main size (on when absent) |
| `paint.dual_hardness` | text | a number, `0.6` | The second tip's hardness when it is round |
| `paint.dual_pictures` | bytes | SHA-256 hashes, 32 bytes each | The second tip's pictures, by the hash of their files |
| `paint.watercolor` | flag | | The watercolor edge is on |
| `paint.watercolor_after` | flag | | The edge is made when the pen lifts, not while drawing |
| `paint.sharp_angles` | flag | | Corners stay sharp (on when absent); off, they are rounded within the brush size |
| `paint.tone` | flag | | The brush lays (or, as an eraser, scrapes) a halftone |
| `paint.tone_shape` | choice | `dot`, `line` | The halftone's dots |
| `paint.behind` | flag | | The paint goes behind what the layer holds |
| `paint.all_layers` | flag | | As an eraser, it erases on every layer it is handed |

## Papers

A paper is a gray picture: dark is a valley, light a high spot. Sue's own
papers (`grain`, `canvas`, `cold-press`, `rough`) are made by rule, the
same on every machine. A person's own paper is any gray picture, kept in
the document or the brush library and named by the SHA-256 of its file
(the model's `texture.source` `picture`); it repeats in both directions,
so a picture that tiles without a seam makes the best paper.

## Blenders

A brush whose engine is `blend` with one of these names mixes the paint
on the picture without laying paint of its own: `blend`, `blur`,
`finger-tip`, `wet-bleed`, `soothing-watercolor`, `painterly`,
`textured`. Any other mixing brush uses the model's `mix` values.
