# The `clip.foreign` component

What a foreign file format holds that Sue does not map, kept on the
entity it was imported with, so an export to that format can put it
back. Sue Paint's `.clip` converter is its first user. This page is for
anyone who writes another converter that keeps data the same way, or who
reads a `.sue` file without Sue.

## Where it lives

`clip.foreign` is an extension component (`sue_model::Foreign`,
`crates/sue-model/src/foreign.rs`), version 1, registered by
`sue_model::register`. A build that does not know it keeps it as opaque
bytes, as every unknown component (Rule 3). It is written inside the
entity in `ARNA`, like any extension component.

## The component's bytes (version 1)

All numbers use Sue's codec (`crates/sue-core/src/codec.rs`): `varint` is
LEB128, a string or byte run is a varint length and then the bytes.

| Field | Encoding | Meaning |
| --- | --- | --- |
| `format` | string | The converter that kept it: `clip` |
| `part` | string | What part of the file: `layer` or `picture` |
| `fingerprint` | 32 raw bytes | A SHA-256 of the entity, in the converter's terms, as it was right after the import |
| `data` | byte run | What was kept, in the converter's own encoding (below) |

## How the fingerprint decides

On export the converter takes the fingerprint of the entity as it is now
and compares it with the stored one. The same fingerprint means nobody
changed the entity in Sue: `data` goes back into the file as it was, with
everything Sue does not understand. A different fingerprint means it was
changed: the entity is written from Sue's model and its `data` is left
out, for that entity only.

A fingerprint is a hash of content, not an arena revision. Revisions are
not saved, so only a hash can still tell after the picture was saved as
`.sue` and opened again.

The `.clip` converter's fingerprints (`crates/sue-paint/src/clip_io/print.rs`)
are SHA-256 over framed pieces (each run of bytes preceded by its length
as a little-endian 64-bit number):

- a layer (`part` = `layer`): the text `layer`, then the layer entity's
  name, class, encoded properties and pixels (width, height, pixel
  format, bytes), then its mask entity the same way, or a zero when it
  has none. Where it stands in the stack and which folder holds it are
  not part of it: moving a layer keeps its data.
- a picture (`part` = `picture`): the text `picture`, the width and the
  height.

## What the `.clip` converter keeps

`crates/sue-paint/src/clip_io/keep.rs`. Both encodings start with a
version byte, 1.

A **layer** (`part` = `layer`) is one row of the `.clip`'s layer tree as
it was read, without what a folder holds: its `MainId`, name, the type,
folder, lock, clip, opacity, blend, visibility, offset, pixel offset and
draw color numbers as the file has them, its uuid, Sue's draft and
reference marks, its pixels and its mask (each with the ids and other
columns of its mipmap rows, the stored bitmap's id, the attribute and the
blocks still compressed), and every other column of its row by name.

A **picture** (`part` = `picture`) is what belongs to no layer: the
canvas size and resolution, the canvas row's id and other columns, the
`Head` chunk, the root folder's row (as a layer, above), and everything
the reader kept as foreign: the shapes of the tables it reads, whole
tables it does not read, rows no layer reached, chunks of unknown kinds,
the extra columns of `ExternalChunk`, the columns of a `SueProject` row,
and the bytes before, between and after the chunks.

Values from the database are a kind byte and then the value: 0 null, 1
whole number (zigzag varint), 2 real (64-bit float), 3 text, 4 bytes.

## Writing another converter

Use your own `format` name, keep the `part` names your format needs,
start your `data` with a version byte, and fingerprint only what your
converter maps, in a fixed order with every piece framed. Say what you
keep in a page like this one.
