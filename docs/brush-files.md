# Brush files: the library, shared brushes and `.sut`

Sue's brushes are files a person can make, share, install and remove.
This page says what those files hold. The brush itself is a
`BrushPreset`, the component `sue.brush` (`HUMANZ.md`, the brush model);
what its paint behaviours store on it is in `docs/brush-behaviours.md`.

## The brush library

The library is an ordinary `.sue` project (`crates/sue-io/src/brushes/library.rs`).
Its arena holds:

| Entity | Class | What it carries |
| --- | --- | --- |
| The library | `sue.folder` | the property `sue.brush.library` (true); named `Brushes` |
| A set | `sue.folder`, a child of the library | its name; the property `sue.brush.tool` when it is made for one tool (`pen`, `pencil`, `brush`, `airbrush`, `decoration`, `eraser`) |
| A brush | `sue.brush`, a child of its set | the `BrushPreset`; its entity name is the preset's name |
| A picture | `sue.brush.picture`, a child of the library | the encoded file (`sue.encoded`, format `png`); its name is the lowercase hex SHA-256 of the file |

A brush refers to each picture it uses (its tip, its paper, its second
tip) with a `sue.ref` relation, and a preset names the same picture by
that SHA-256. Children keep their order, so a set lists its brushes as
the Sub Tool palette shows them.

Sue's own brushes keep the same entity ids in every library: each id is
made from the SHA-256 of `sue.brush.library:` and the preset's id
(`sue.pen.g_pen`), the sets' from `set:<tool>/<group>`. A brush a
person makes, copies or installs gets a new id.

Every change to the library is one undo step: adding, editing, renaming,
copying, moving and deleting a brush, adding and deleting a set, and
installing a shared file.

## A shared brush file

A brush or a set a person shares is a `.sue` file laid out as the
library is, holding only that set (or that one brush in its set) and the
pictures it uses. Installing it adds its sets at the end of the library
with new ids; a picture the library already holds (the same SHA-256) is
used, not copied.

## Built-in brushes

Sue's built-in brushes are written by hand in
`crates/sue-brush/src/presets.rs`. Each has an id that never changes,
`sue.<tool>.<name>`, and the group its tool lists it under:

| Tool | Groups |
| --- | --- |
| `pen` | Pen, Marker |
| `pencil` (the Sketch tool) | Pencil, Pastel |
| `brush` | Watercolor, Thick paint, Ink |
| `airbrush` | Airbrush |
| `decoration` | Effect, Flower, Clothing, Lines |
| `eraser` | Eraser |

`presets.rs` stores one field of its own on a preset:

| Field | Kind | Values | Meaning |
| --- | --- | --- | --- |
| `erase.vector` | choice | `touched`, `intersection`, `whole` | How an eraser takes vector lines away: only where it touches, up to the lines it crosses, or every line it touches. Absent: `touched` |

## Custom sub tools of Sue 0.0.75

Sue 0.0.75 kept a person's own sub tools in `item_bank.txt`, one a line:
the tool's name, the sub tool's name, its stabilization and its brush as
text, tab separated. The first library a person gets has them moved in,
read through `BrushPreset::from_legacy_text`, in a set named `Custom` for
each tool. The old file is only read and stays as a backup.

## Clip Studio Paint sub tools (`.sut`)

A `.sut` is an SQLite database (`crates/sue-io/src/brushes/sut.rs`). What
follows was learnt from files made on this machine: Clip Studio Paint's
own tool database and sub tools it exported. Nothing of Clip Studio's is
kept in Sue: tests build their `.sut` files in code.

| Table | What it holds |
| --- | --- |
| `Manager` | one row: the file's version, the root node |
| `Node` | a row per sub tool: `NodeName`, `NodeVariantID` (its settings now), `NodeInitVariantID` (as first installed) |
| `Variant` | a row of settings per variant, one column per setting |
| `MaterialFile` | a row per material: `OriginalPath`, and `FileData`, a tar archive with `thumbnail/thumbnail.png` and the material's own picture |
| `SueBrush` | written by Sue only: `VariantID` and `Preset`, the whole `BrushPreset` |

What each `Variant` column becomes:

| Columns | Sue |
| --- | --- |
| `BrushSize`, `BrushSizeUnit` | `size` in pixels: unit 0 pixels, 1 centimeters, 2 millimeters, 3 inches, 4 points, 5 Q, at 350 dpi |
| `Opacity`, `BrushHardness`, `BrushFlow`, `BrushThickness` (0 to 100) | `opacity`, `hardness`, `flow`, `aspect` |
| `BrushInterval`, `BrushAutoIntervalType` | `spacing` as a share of the size; type 0 is a fixed spacing (`ratio`), the others the engine's own (`auto`) |
| `BrushRotation`, `BrushRotationEffector`, `BrushRotationRandomScale` | `angle`, and added bindings of the angle: bit 16 pressure, 32 the line's direction, 64 the pen's tilt direction, 128 random (half a turn either way times the scale) |
| `...Effector` of size, opacity, density, spacing, thickness, texture density, amount and density of paint, particle size and density | bindings of that parameter: bit 16 pressure, 32 speed, 64 tilt, 128 random; header words 3 to 6 their minimums in percent; words 8, 9 and 11 the sizes of the pressure, speed and tilt curves that follow the header, each a list of points as two doubles |
| `AntiAlias` | 0 none, 1 weak, 2 medium, 3 strong |
| `CompositeMode` | 0 normal, 27 erase; other numbers are kept |
| `FlickerReduction`, `FlickerReductionBySpeed` | stabilization and its loosening with speed |
| `BrushUseRevision`, `BrushRevision` | post correction, a tenth of the number |
| `BrushUseIn`, `BrushInLength`, `BrushInRatio`, `BrushUseOut`, ... `BrushInOutType`, `BrushInOutBySpeed` | starting and ending: by length (type 0), by percent (1) or fading (2) |
| `BrushUsePatternImage`, `BrushPatternImageArray`, `BrushPatternOrderType`, `BrushChangePatternColor`, `BrushRibbon`, flips, `BrushBlendPatternByDarken` | the tip's pictures, as the materials the array names; how they are taken; whether they keep their colors; the ribbon |
| `TextureImage`, `TextureScale`, `TextureRotate`, `TextureDensity`, `TextureBrightness`, `TextureContrast`, `TextureReverseDensity`, `TextureStressDensity`, `TextureForPlot` | the paper |
| `BrushUseWaterColor`, `BrushMixColor`, `BrushMixAlpha`, `BrushMixColorExtension`, `BrushBlur` | mixing: amount of paint, density of paint, color stretch, blur |
| `BrushUseSpray`, `BrushSpraySize`, `BrushSpraySizeSyncBrushSize`, `BrushSprayDensity`, `BrushSprayBias`, `BrushRotationEffectorInSpray` | the spraying effect |
| `BrushUseWaterEdge`, `BrushWaterEdgeRadius`, `...AlphaPower`, `...ValuePower`, `BrushWaterEdgeBlur`, `...AfterDrag` | the watercolor edge |
| `BrushHueChange`, `BrushSaturationChange`, `BrushValueChange`, `BrushSubColor`, `BrushChangeStrokeColor` and the stroke's own | color jitter |
| `UseDualBrush`, `DualSize`, `SyncDualBrushSize`, `DualInterval`, `DualHardness`, `DualPatternImageArray` | the second tip |
| `BrushAdjustFlowByInterval`, `BrushContinuousPlot`, `BrushSharpenCorner`, `BrushEraseAllLayer`, `BrushUseVectorEraser`, `BrushVectorEraserType` | density by gap, continuous spraying, sharp corners, erasing on every layer, `erase.vector` |

A preset read from a `.sut` keeps what it came from, so nothing is lost
on the way back:

| Preset field | What it keeps |
| --- | --- |
| `sut.variant` | the whole `Variant` row: each column's name, declared type and value |
| `sut.initial` | the row it was first installed with |
| `sut.node` | its `Node` row |
| `sut.manager` | the file's `Manager` row |
| `sut.materials` | the material archives its pictures came from, with their paths |

Writing a `.sut` starts from those rows and rewrites only the settings
changed in Sue, so every column Sue does not know comes back as it was.
A Sue brush written as a `.sut` also carries itself in `SueBrush`;
reading it back gives the same brush, with any setting changed in Clip
Studio meanwhile taken from the file.

What a `.sut` cannot say the same way:

- A material's full picture is in a layer format of Clip Studio's own
  that Sue does not read. Sue paints with the material's thumbnail (up to
  300 pixels a side) and keeps the archive, so it goes back unchanged. A
  picture made in Sue goes out as an archive holding only the thumbnail.
- Some numbers have no meaning Sue knows yet (other blend, paper and
  second tip modes, the speed effector's word 10, the start and end
  targets). They are kept, not mapped.
- Sue's own papers are made by rule and have no file, so a brush on one
  of them goes out without a paper.
- Files written by Sue for Clip Studio have not yet been opened in Clip
  Studio itself.
