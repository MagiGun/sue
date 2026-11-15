# Kept work and the autosave journal

Sue keeps work nobody saved, so the Hub can offer it under Unsaved Work
after a crash or a quit. This page describes the files it keeps, for
people who want to read or rescue kept work with their own tools. The
code is `crates/sue-core/src/journal.rs` (the journal) and
`crates/sue-wm/src/work.rs` (the folder).

## Where it lives

Kept work is in Sue's data folder, under `work/autosave/`. Each piece of
kept work has an id, unique to the run that kept it, such as
`1760000500-20276-1`, and up to three files:

| File | What it is |
| --- | --- |
| `<id>.sue` | The work, whole: an ordinary `.sue` project file. |
| `<id>.journal` | The changes since `<id>.sue` was written (optional). |
| `<id>.txt` | A note: when it was kept, the project file it belongs to, what it holds and the studio it was worked on in. |

The note is text. Its first line is `# Sue autosave, format 1`; the
lines after it are `key=value`: `at` (seconds since 1970), `project`
(the project file's path, once it was saved somewhere), `name` and
`page` (the studio's id). Unknown keys are ignored.

Sue 0.0.75 and older kept only `<id>.sue` and `<id>.txt`. Sue 0.0.8
reads such work as it is.

## The journal

Keeping the whole document again after every change would cost the
whole document each time. So after the first keep, Sue appends a record
of what changed to `<id>.journal`, and only writes `<id>.sue` again
(compaction) when the journal has grown larger than the whole file (or
4 MiB, whichever is larger) or holds 256 records.

All numbers are little-endian.

```text
magic     8 bytes   "SUE\0JRNL"
version   u32       1
base      u64       the length of <id>.sue in bytes
digest    32 bytes  SHA-256 of <id>.sue
records   repeated, to the end of the file:
  length  u64       bytes in the payload
  payload           a .sue file (see below)
  check   u32       CRC-32 (the one of zlib and PNG) of the payload
```

The digest names the whole file the journal belongs to. When it does not
match `<id>.sue` (a compaction rewrote the whole file and was stopped
before it removed the old journal), the journal is ignored: the whole
file is the newer state.

Each payload is a complete `.sue` file holding the work's chunks as they
stood when the record was written, with two differences:

- its `PIXL` chunk (the pixel store) holds only the tiles that were not
  in the work before the record, or that changed under the same key;
- it holds no `THMB` chunk; the preview of `<id>.sue` stands.

## Reading kept work

1. Read `<id>.sue`. This is the work if there is no journal, or if the
   journal's header does not name this file.
2. For each record in turn: stop at the first one that is cut short or
   whose check does not match. Otherwise take the record's chunks in
   place of the work's chunks, except that the blobs of its `PIXL`
   chunk are added to the work's pixel store (a blob under a key the
   store already has replaces it), and that the work keeps its `THMB`
   when the record has none.
3. The work is what the last whole record left.

A crash while a record was appended loses that record only; a crash
while the whole file was rewritten loses nothing, since the old file
stays until the new one replaces it.

## The preview

Every save of a project, and every whole write of kept work, puts a
preview into the file's `THMB` chunk (version 1): a PNG of the first
picture the project holds, flattened on its paper, at most 256 pixels on
its long edge. A project without a picture has none. The Hub's cards
read the preview by seeking to it, without reading the document. A
`THMB` holding JPEG XL bytes is read too.
