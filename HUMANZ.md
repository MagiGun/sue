# HUMANZ: Sue Systems Specification

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

This is the architectural specification of **Sue: Singular Unified Engine**.
It is written for systems engineers. The plain-English
companion is [`HUMANS.md`](HUMANS.md); the backbone rules every contributor,
human or AI, follows are in [`AGENTS.md`](AGENTS.md).

The words **MUST**, **MUST NOT**, **SHOULD** and **MAY** are used as in RFC 2119.
Type names in `code` refer to real items in the workspace. §1 is the backbone
every feature shares. §7 maps every part of this specification to its
implementation status, and §8 maps the features Sue may grow into onto the
backbone. Like every file in the
repository, this one contains no em dashes, en dashes or middle dots (house
style).

---

## 0. Design tenets

| # | Tenet | Enforced by |
| --- | --- | --- |
| T1 | **One data model.** Every asset is an entity in one arena, with a stable id, typed components, properties and relations. | `sue-core`, rule 1 |
| T2 | **Zero-copy.** Subsystems exchange handles, ids and shared buffers, never copies. | `sue-core`, rule 5 |
| T3 | **Everything is reversible.** Every data change is a `ReversibleTransaction` storing a delta. | `sue-core`, rule 2 |
| T4 | **Everything is riceable.** Tunable look and feel lives in `.suerice` parameters. | `sue-rice` (guideline) |
| T5 | **Platform isolation.** OS-specific code exists only in `sue-platform`. | rule 6 |
| T6 | **Hardware empathy.** Static scenes cost 0 frames. Budgets follow `HardwareProfile`. | `sue-wm`, `sue-rice` (guideline) |
| T7 | **Offline first.** AI, codecs and updates work with no network. | `sue-ai`, `sue-pkg` |
| T8 | **Open by registration.** Component types, formats, operators, node types and applets register at runtime; unknown data round-trips untouched. | `sue-core`, `sue-wm`, rule 3 |

### 0.1 Crate graph

```text
bins/sue      ──► every crate below                   + egui
sue-writer    ──► sue-type, sue-paint, sue-wm, sue-io, sue-model, sue-rice, sue-core   + egui
sue-paint     ──► sue-wm, sue-io, sue-model, sue-rice, sue-platform, sue-core   + egui
sue-type      ──► sue-wm, sue-io, sue-model, sue-core   + egui, ttf-parser
sue-wm        ──► sue-rice, sue-platform, sue-core   + egui, ttf-parser
sue-ai        ──► sue-platform, sue-core
sue-rice      ──► sue-platform, sue-core
sue-io        ──► sue-model, sue-core                 + jxl-oxide, zune-jpegxl, zune-png, zune-jpeg, jpeg-encoder, symphonia, image, flacenc, miniz_oxide, ttf-parser
sue-model     ──► sue-core
sue-pkg       ──► sha2, ed25519-dalek, semver         (no engine crates)
sue-core      ──► nothing                              (leaf)
sue-platform  ──► winit, wgpu, egui, egui-winit, egui-wgpu, arboard, pollster, cpal, audio_thread_priority, rfd, open   (no engine crates)
```

The graph is acyclic by construction. `sue-core` and `sue-platform` are leaves
among the engine crates, so the data kernel never depends on an operating
system, and the OS layer never depends on the data model. `winit` (windows and
OS input), `cpal` (audio devices), `rfd` (native dialogs on desktops), `jni`
(Android's file pickers, in Java), `objc2-ui-kit` (iOS's, in `sue-ios`) and
`open` (the system browser and file manager), `arboard` (the clipboard),
`egui-winit` (OS input for egui) and `egui-wgpu` (egui's painter) are used only
by `sue-platform`. `egui` itself is a portable, immediate-mode widget library with
no OS bindings: `sue-wm` builds the whole shell with it, and the platform host
paints what it produces. `unsafe` code is forbidden workspace-wide
(`[workspace.lints] rust.unsafe_code = "forbid"`). The `wgpu` window surface is
created through its safe API, so `sue-platform` keeps the lint too. Native FFI
is relaxed only in its two small entry crates, each item approved by the
maintainer: `crates/sue-platform/android` (the native activity's entry and the
two raw JNI pointers it wraps) and `crates/sue-platform/ios` (UIKit's
document picker and keyboard notifications); `sue-platform` calls Java
through `jni`'s safe API.

---

## 1. The backbone (`sue-core`, `sue-model`)

Sue is meant to grow into one app that does the work of many: documents,
spreadsheets, design, photo, video, audio, 3D and CAD, engineering drawings, a
game engine and more (§8). Those features will be built one by one, many years
apart, by different people. The backbone is the part they all share, and it is
what makes them **one** app: a feature added later reads, links, saves, undoes
and exports the data of every feature added before it, without knowing it
exists. The rules that protect the backbone are in [`AGENTS.md`](AGENTS.md).

The backbone is these pieces, each described below:

| Piece | Type names | Section |
| --- | --- | --- |
| One arena of entities with stable ids | `Arena`, `Entity`, `EntityId`, `AssetHandle` | §1.2, §1.3 |
| Seven frozen built-in buffers and projections | `ComponentData`, `ComponentKind`, `Projection` | §1.4 |
| Open, versioned extension components | `ComponentType`, `ComponentRegistry`, `OpaqueComponent` | §1.5 |
| Typed, ordered relations | `Relation`, `RelationStore`, `OrderKey`, `CHILD`, `REF` | §1.6 |
| A shared vocabulary | `Value`, `Properties`, `PropertySchema`, `Quantity`, `Color`, `Time`, `Rate`, `Timecode` | §1.7 |
| Delta-tree undo and generic edits | `ReversibleTransaction`, `UndoStack`, `txn::*`, `Batch` | §1.8 |
| The `.sue` project file | `SueFile`, `Chunk` | §1.9 |
| One format registry and bundles | `FormatHandler`, `FormatRegistry`, `Bundle`, `InsertBundle` | §1.10 |
| One operator registry and presets | `Operator`, `OperatorRegistry`, `Preset`, `PresetLibrary` | §1.11 |
| Background work and shared buffers | `JobPool`, `Shared` | §1.12 |
| Shared data models | `sue_model::*` | §1.13 |

### 1.1 Why there are no $O(N^2)$ converters

A conventional suite that supports $N$ file formats and wants any-to-any
conversion needs a converter for every ordered pair: $N(N-1) = O(N^2)$ code paths.
Each program also keeps its own in-memory model, so moving work from the video
editor to the DAW is one more conversion, and usually a lossy one.

Sue routes every format through **one data model**: entities made of shared
components (§1.3 to §1.5).

$$
\text{format}_i \xrightarrow{\ \text{import}_i\ } \text{entities} \xrightarrow{\ \text{export}_j\ } \text{format}_j
$$

Supporting $N$ formats takes at most $2N$ handlers, which is $O(N)$. A format
handler adds one format to every feature at once. Features never convert
between themselves: a table made by the CSV importer is the same `sue.table`
component a spreadsheet, a BIM schedule and a chart read, and a clip on a video
timeline points at the very entity the audio editor changes.

### 1.2 The Arena, `AssetHandle` and `EntityId`

All engine data lives in `sue_core::Arena`, a generational slot arena of
entities. At runtime an entity is addressed by an `AssetHandle`, a
`#[repr(transparent)]` `u64`:

```text
 63                              32 31                               0
┌──────────────────────────────────┬──────────────────────────────────┐
│        generation (u32)          │          slot index (u32)        │
└──────────────────────────────────┴──────────────────────────────────┘
```

Invariants:

1. A slot's first occupant has generation **1**. Generation 0 is never live, so
   `AssetHandle::NULL` (all bits zero) never resolves.
2. Despawning increments the slot's generation, so every outstanding handle to
   the old occupant goes stale. This prevents ABA reuse bugs. Lookups of stale
   handles return `CoreError::StaleHandle` instead of the new occupant.
3. A slot whose generation reaches `u32::MAX` is **retired** and never reused.
4. Freed slots are recycled LIFO through a free list. `spawn` and `despawn` run
   in $O(1)$.
5. `Arena::split_pair(read, write)` hands out a shared borrow of one entity and an
   exclusive borrow of another at the same time, with no copying and no `unsafe`
   (it uses `split_at_mut`). This is the primitive behind every zero-copy
   pipeline (§6.4). Passing the same handle twice returns
   `CoreError::AliasedHandles`.

Handles are fast but temporary: they change when a document is reopened. Every
entity therefore also has an **`EntityId`**, a random version 4 UUID that is its
name for good. It survives saving, loading, undo and redo, copying to another
document and sending to another device. The arena keeps an id to handle map
(`handle_of`, `get_by_id`, `get_by_id_mut`), and `insert_keep_id` refuses a
duplicate id (`CoreError::DuplicateId`). **Everything that links data across
features, files or time links by `EntityId`**: relations, clips, frames on a
page, links in a note, AST children, node graph parameters.

**Revisions.** The arena counts changes: every spawn, despawn, relation edit and
every mutable borrow (`get_mut`, `get_by_id_mut`, the writer of `split_pair`)
raises `Arena::revision()` and stamps the entity's `Entity::revision()`. "Is the
document modified?" and "is this thumbnail stale?" are one comparison. A
mutable borrow that ends up changing nothing still counts; the counter errs on
the side of rebuilding.

**Concurrency model.** The target is a **lock-free shared arena**: slot pages are
allocated in fixed-size chunks that never move. Generations are atomics. Readers
validate a handle with one acquire-load and never block. Writers append to
per-thread command buffers that are merged at frame boundaries, and freed memory
is reclaimed epoch by epoch. Sue ships the single-writer form of the same
contract: mutation requires `&mut Arena`, and `Arena: Send + Sync` allows any
number of concurrent readers. Long work runs on the job pool (§1.12) over
`Shared` buffers and comes back as a transaction. Code written against today's
API (handles, ids, split borrows, transactions) carries over unchanged.

### 1.3 Entities

An `Entity` has:

| Part | Type | Meaning |
| --- | --- | --- |
| `id()` | `EntityId` | Stable identity (§1.2) |
| `name()` | `String` | Display name, e.g. a file name |
| `class()` | `String` | **What it is**: `sue.image`, `sue.sheet`, `noelle.scene`, `bim.wall`. Features decide whether they open an entity by its class first. `sue_model::class` lists the well-known ones. |
| `base_kind()` | `Option<ComponentKind>` | The built-in buffer it was made from, if any. Folders, tables, scenes and graphs have none. |
| `components()` | `ComponentData` | Built-in buffers, at most one per kind (§1.4) |
| `exts()` | `dyn Component` | Extension components, at most one per type name (§1.5) |
| `properties()` | `Properties` | Edit state any feature can read and write (§1.7) |
| `metadata()` | `String` to `String` | Facts about where it came from (source path, codec, decode time) |
| `revision()` | `u64` | Arena revision of its last change |

`Entity::new(name, base)` makes an entity from a buffer (its class defaults to
the kind's, e.g. `sue.text`); `Entity::empty(name, class)` makes one without a
buffer. Builders (`with_class`, `with_property`, `with_ext`, `with_component`,
`with_meta`, `with_id`) assemble an entity before it enters an arena.
`remap_entities` rewrites every link inside an entity (properties, AST
children, extension components), which is how copies get fresh ids.

Class names are namespaced like every other registry key: `sue.*` for the core,
`noelle.*` for the game engine, a feature's own prefix for its own.

### 1.4 The built-in buffers and projections

The seven built-in components are dense, typed buffers for the data every
feature touches. Their encodings are **frozen**: new data types are extension
components (§1.5), never new built-in kinds.

| `ComponentKind` | Payload | Layout | Zero-copy `f32` tensor view |
| --- | --- | --- | --- |
| `StringBuffer` | UTF-8 text | `String` | n/a |
| `PCMFloat1D` | Interleaved audio | `sample_rate: u32`, `channels: u16`, `Vec<f32>` | samples |
| `VectorSpline2D` | Cubic Bézier chain | `Vec<[f32; 2]>` with `3n + 1` knots, `closed` | knots, flattened |
| `RasterTile2D` | Pixel tile | `width`, `height`, `PixelFormat::{Gray8, Rgba8, RgbaF32}` | `RgbaF32` pixels |
| `MeshBuffer3D` | Indexed triangles | `Vec<[f32; 3]>` positions, `Vec<u32>` indices | positions, flattened |
| `TimeSpan` | Interval on the global clock | `start`, `duration` in flicks (§1.7) | n/a |
| `ASTNode` | Syntax / scene-graph node | `kind`, byte span, `Vec<EntityId>` children | n/a |

Constructors validate their invariants (for example, sample count divisible by
channel count, or triangle indices in bounds), and so does
`ComponentData::decode`, so a component that exists is well-formed.
`ComponentData::f32_view{,_mut}` exposes the numeric payload as a borrowed
slice, and that slice is the tensor interface of §6.

A **projection** is a way of viewing an entity, one per built-in kind:

| `Projection` | Native component | Typical feature |
| --- | --- | --- |
| `Document` | `StringBuffer` | Documents, notes, code |
| `Audio` | `PCMFloat1D` | Audio editing, playback |
| `Vector2D` | `VectorSpline2D` | Vector art, sketches |
| `Raster2D` | `RasterTile2D` | Painting, photos, thumbnails |
| `Mesh3D` | `MeshBuffer3D` | 3D, CAD previews, game levels |
| `Timeline` | `TimeSpan` | Video, animation, slideshows |
| `Code` | `ASTNode` | IDE, scene trees |

Each entity carries a `ProjectionMask`. Putting a document on the timeline is
`Arena::attach_projection(handle, Projection::Timeline, TimeSpan)`. This adds a
`TimeSpan` component and sets one bit. It **MUST NOT** copy, move or mutate any
existing buffer; the heap allocation behind every other component keeps its
exact address, and a unit test asserts this pointer stability.
`detach_projection` returns the removed component, so the operation can be
reversed, and the base projection cannot be detached
(`CoreError::CannotDetachBase`). An entity without a base takes the first
attached buffer as its base.

### 1.5 Open components

Most future data does not fit seven buffers: a spreadsheet, a node graph, a
rig, a BIM wall, a Noelle physics body. A feature defines such a type once:

```rust
pub trait ComponentType: Clone + Send + Sync + fmt::Debug + 'static {
    const NAME: &'static str;   // namespaced, never changed: "sue.table"
    const VERSION: u32;         // bumped whenever the encoding changes
    fn encode(&self, out: &mut Writer);
    fn decode(version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError>;
    fn heap_bytes(&self) -> usize { 0 }
    fn remap_entities(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId) {}
}
```

and registers it: `ComponentRegistry::register::<T>()`. A blanket impl gives
every `ComponentType` the object-safe `Component` trait the arena stores, so
`entity.ext::<Table>()`, `entity.ext_mut::<Table>()` and `entity.ext_named("sue.table")`
work for any type. `components_equal` compares two components by encoding.

Compatibility follows from three rules:

1. **Decode every old version.** `decode` receives the stored version and MUST
   read every version up to `VERSION` (a test per old version).
2. **Never lose the unknown.** A type that is not registered, or a version newer
   than this build knows, decodes as an `OpaqueComponent` holding the bytes, and
   encodes back byte for byte. A document made by a Sue with more features
   passes through an older Sue unharmed. `ComponentRegistry::upgrade` turns an
   opaque component into its real type once the feature is installed.
3. **Links are ids.** `remap_entities` rewrites them when entities are copied.

The binary encoding uses `sue_core::codec` (`Writer`, `Reader`): little-endian
numbers, LEB128 varints, length-prefixed strings and nested runs, with bounds
checks so hostile input returns `CodecError` instead of panicking or
allocating without limit. It has no third-party dependency, so the file format
never depends on a serializer's stability.

### 1.6 Relations

A `Relation` is a typed, ordered edge between two `EntityId`s with optional data
(`Properties`). `Arena::relate` and `Arena::unrelate` keep them in the
`RelationStore`, indexed both ways (`targets`, `sources`, `involving`).

- `CHILD` (`sue.child`) is a **tree**: one parent per child, no cycles
  (`RelationError::SecondParent`, `RelationError::Cycle`). Folders, layer
  stacks, book chapters, scene trees, assemblies and building storeys are
  `CHILD` trees. Other kinds can be registered as trees with `add_tree_kind`.
- `REF` (`sue.ref`) and any other kind form a **graph**: a link in a note, a
  material used by a mesh, a sidechain from one track to another.
- Siblings are ordered by an `OrderKey`, a base-36 fractional key.
  `OrderKey::between(a, b)` always finds a key strictly between two others, so
  moving one item rewrites only that item (which also merges well in future
  collaboration).
- Despawning an entity drops its relations; `despawn_with_relations` returns
  them so `DespawnEntity` can put them back on undo.

### 1.7 The shared vocabulary

Features agree on values, units, color and time, so their data can meet.

**`Value` and `Properties`.** `Value` is the one dynamic type: `Bool`, `Int`,
`Float`, `Text`, `Vec2`/`Vec3`/`Vec4`, `Color`, `Quantity`, `Time`, `Entity`
(a link), `List`, `Map`, `Bytes`, and `Opaque` (a value type from a newer Sue,
kept byte for byte). Entity properties, node parameters, cell contents, preset
fields and relation data are all `Value`s. Keys are namespaced by the feature
that owns them (`text.size`, `audio.gain`, `noelle.gravity`). A
`PropertySchema` holds `PropertyDef`s (key, label, `ValueType`, default, range,
help, group), so a generic Inspector can edit any property of any feature and
`validate` it; unknown keys are allowed. `sue_model::register_properties` adds
the shared ones (opacity, blend mode, fill, stroke, font, size, gain, pan,
material, layer).

**`Quantity` and units.** A number with a `Unit` from one table that covers
length (mm to miles, typographic points and picas, CSS pixels), area, volume,
angle, time, frequency, mass, force, pressure (Pa to psi and inches of water),
energy (J to kWh, BTU), power (W to hp, BTU/h, tons of refrigeration),
temperature (K, °C, °F, affine), velocity, flow (L/s, cfm, gpm), voltage,
current, resistance, capacitance, inductance, luminous intensity, data and
level. Conversions are exact within a dimension, and mixing dimensions is an
error (`DimensionMismatch`), never a silent bug. `"12.5 mm".parse::<Quantity>()`
reads user input.

**`Color`.** A color carries its `ColorSpace`: sRGB, linear sRGB, Rec. 709,
Display P3, linear Rec. 2020, Rec. 2100 PQ and HLG, ACEScg, ACES2065-1 and XYZ.
Conversions go through CIE XYZ with matrices derived from the published
primaries, Bradford adaptation between the D65 and ACES whites, and exact
transfer functions (sRGB, BT.1886, PQ, HLG). Out-of-gamut values are kept, not
clipped, so grading never loses data.

**Time.** `Time` counts **flicks**: $1\ \text{s} = 705{,}600{,}000$ flicks
(`FLICKS_PER_SECOND`). The number divides evenly by every common frame rate (24,
25, 30, 48, 50, 60, 90, 100, 120 fps) and sample rate (8 kHz to 192 kHz,
including 44.1 kHz). `Rate` is an exact rational rate (23.976 is 24000/1001), and
frame and sample positions convert with no drift (a test checks NTSC rates over
a full day). `Timecode` reads and writes SMPTE timecode, drop-frame included.
`TimeSpan` is the built-in interval on the same clock.

### 1.8 Delta-tree undo and generic edits

Every mutation is a `ReversibleTransaction<T>`:

```rust
pub trait ReversibleTransaction<T>: Send {
    fn label(&self) -> String;
    fn apply(&mut self, target: &mut T) -> Result<(), TxnError>;
    fn revert(&mut self, target: &mut T) -> Result<(), TxnError>;
}
```

Contract:

- `apply` is **atomic**: on `Err`, the target is unchanged.
- `revert` is the **exact inverse** of the most recent successful `apply`.
- A transaction stores a **delta**, never a snapshot. `InsertText` stores the
  inserted text. `DeleteText` captures only the removed range during `apply`.
  `AttachProjection` holds the component and takes it back on `revert`. Memory is
  $O(\sum_i |\delta_i|)$, not $O(k \cdot |\text{state}|)$ for $k$ snapshots.

`UndoStack<T>` is a **tree**, not a stack. Undoing and then making a new edit
does not throw the old future away. It becomes a sibling branch.

```text
root ── A ── B ── C          undo ×2, then apply D:
             └── D  ◄ cursor  (C is still reachable)
```

- `apply` appends a child of the cursor and moves the cursor to it.
- `record` adds an edit that already happened in place (typing in a live
  editor) without applying it again.
- `undo` reverts the cursor's transaction and moves to the parent.
- `redo` re-applies the **preferred child**: the branch most recently created or
  visited. `select_redo_branch` changes it.
- `checkout(node)` walks to any node: it undoes up to the lowest common ancestor,
  then redoes down to the target. The cost is the path length.

The kernel ships generic edits that name entities by `EntityId`, so they stay
valid however often an entity is removed and brought back: `SpawnEntity`
(optionally with relations), `DespawnEntity` (restores relations), `SetProperty`,
`SetMeta`, `Rename`, `Relate`, `Unrelate`, `SetExtension` (swap a whole extension
component; fine-grained edits like `sue_model::SetCell` and `GraphEdit` store
smaller deltas), `ReplaceComponent` (swap the component of a kind, a raster
for an edited one, under the same id), `RasterEdit` (the rectangles of a raster an in-place pixel edit
changed, before and after: a brush stroke stores only the 64-pixel tiles it
touched), `InsertBundle` (§1.10) and `Batch<T>`, which applies several
transactions as one step and takes back the done ones if a later one fails.

`UndoStack<T>` is generic. The shell keeps **one global history**,
`UndoStack<sue_wm::World>`, where `World { arena, rice }` holds everything
undoable. `ArenaEdit(txn)` and `RiceEdit(txn)` lift arena and rice transactions
onto the world, so a single `Ctrl+Z` (or `Ctrl+Shift+Z` / `Ctrl+Y` to redo) steps
back through typing, brush strokes, the Hub's pin and theme edits in the order
they happened.

### 1.9 The `.sue` project file

A project is one file:

```text
magic    8 bytes   "SUE\0FILE"
version  u32       container version (1)
extra    bytes     length-prefixed header extension (empty in version 1)
chunks   repeated  tag [u8; 4], version u32, length u64, payload, crc32 u32
end      chunk     tag "END\0", length 0
```

The chunk framing never changes. Each chunk has its own version, so one part
can evolve without touching the others, and a CRC-32 per chunk reports damage
by name. A reader keeps every chunk it does not understand and writes it back
unchanged (`SueFile` holds them; the app keeps the open file so they survive the
next save). Chunks today:

| Tag | Version | Holds | Owner |
| --- | --- | --- | --- |
| `ARNA` | 1 | Every document entity (ids, classes, buffers, extension components, properties, metadata) and every relation | `sue-core` |
| `META` | 1 | Document properties (title, author) | `sue-core` |
| `PRST` | 1 | A preset library (§1.11) | `sue-core` |
| `THMB` | 1 | A preview picture (reserved) | `sue-core` |
| `SCNE` | 1 | The layout of the infinite canvas of builds before the Hub: nodes naming entities by id, rectangles, node properties. This build does not read it and keeps it untouched, like any chunk it does not know | older `sue-wm` |

Opening a project replaces the document. Loading never reuses a
handle that was live before, so caches keyed by handle cannot show stale data.
Saving a loaded file is byte for byte stable (a test checks it), and a document
whose extension types are unknown to this build still round-trips byte for
byte. Big buffers are stored raw today; compressed chunks (JPEG XL for rasters)
are planned as a new `ARNA` version.

### 1.10 Formats and bundles

Every importer and exporter is a `FormatHandler` in one `FormatRegistry`:

```rust
pub trait FormatHandler: Send + Sync {
    fn info(&self) -> FormatInfo;                  // id, name, extensions, MIME, category
    fn sniff(&self, head: &[u8]) -> bool;          // content first
    fn can_import(&self) -> bool;
    fn import(&self, bytes: &[u8], name: &str, options: &Properties) -> Result<Bundle, FormatError>;
    fn can_export(&self, entity: &Entity) -> bool;
    fn export(&self, arena: &Arena, ids: &[EntityId], options: &Properties) -> Result<Vec<u8>, FormatError>;
    fn options(&self) -> Vec<PropertyDef>;         // quality, sample rate, page size
}
```

`FormatRegistry::importer_for` asks every handler's `sniff` first and falls back
to the extension, so a misnamed file still opens. `exporters_for(entity)` lists
what an entity can become. Export options are `PropertyDef`s, so export dialogs
and export presets (`export.<format id>`) work for every format alike.

A `Bundle` is entities plus the relations between them, outside any arena. It
is what importers return, what the clipboard carries (`Bundle::encode`), and
what moves between documents: `Bundle::copy_from(arena, ids, descendants)`
lifts entities out, `renew_ids` gives the copies fresh ids and rewrites every
link between them (properties, components, relations), and `InsertBundle` puts
a bundle into an arena as one undo step, optionally `under` a parent.

`sue_io::formats(registry)` registers every format this build knows (§2): PNG,
JPEG, JPEG XL, WAV, MP3, FLAC, Ogg Vorbis, AAC, plain text, Markdown, CSV, TSV,
Wavefront OBJ and `.sue`. Formats Sue recognizes but cannot decode yet (MP4,
QuickTime, Matroska, AVI, PDF, MIDI, GIF, WebP, TIFF, BMP, PSD, AVIF, HEIF, glTF
binary, fonts) open as entities of the right class carrying
`sue_model::Encoded`, the original bytes, which export unchanged and wait for a
decoder.

### 1.11 Operators and presets

Every action a feature offers is an `Operator<T>`: an id (`audio.normalize`), a
label, a category, a parameter schema (`Vec<PropertyDef>`), an availability test
on the target and the selection (a slice of `EntityId`), and `plan`, which
returns the change as a transaction without making it. `FnOperator` builds one
from closures. The `OperatorRegistry<T>` resolves parameters (defaults, then
the given values, each validated) and `run` applies the plan through the undo
history. The command palette, menus, shortcuts, macros, scripting, batch
processing and AI tools are meant to drive features only through operators, so
a feature is reachable everywhere the moment it registers.

A `Preset` is a named set of values for one operator or format id. A
`PresetLibrary` adds, replaces, lists by target, merges and encodes presets; it
saves inside a project (`PRST`) or on its own, so presets travel between
projects and people.

### 1.12 Background jobs and shared buffers

`JobPool` runs long work (render, export, transcode, simulate, index, infer)
on worker threads that sleep when idle (`for_this_machine` leaves one core for
the interface). A job gets a `JobContext` to report progress and notice
cancellation; a panic inside a job becomes `JobError::Panicked` instead of
taking Sue down. Jobs read data through `Shared<T>` handles (reference counted,
copy-on-write: a write copies only while someone else still holds the buffer),
and hand back a result that the interface applies as a transaction, so undo
covers background work too.

### 1.13 Shared data models (`sue-model`)

`sue-model` defines the data shapes many features share, as registered
extension components (`sue_model::register`):

| Model | Name | Holds | Shared by |
| --- | --- | --- | --- |
| `TextMarks` | `sue.text.marks` | Formatting ranges over the entity's text (bold, link, heading, list, font, size, color, inline math, comment, footnote, embedded entity); paragraph marks ride on each paragraph's newline and follow edits through `insert_text` and `delete_text` (§3.8) | word processing, notes, Markdown, novels, LaTeX, layout |
| `Table` | `sue.table` | Sparse cells (`Value` plus optional formula text), typed columns, A1 references, row insert and remove; `SetCell` edits | spreadsheets, databases, CSV, schedules, bills of materials, cue lists |
| `Animation`, `Curve` | `sue.animation` | Keyframes (constant, linear, Bezier) per property key | motion graphics, animation, rigging, automation, grading keyframes |
| `Response` | (a plain value, kept in settings and tool presets) | A curve from 0 to 1 in to 0 to 1 out: up to eight control points, the first at input 0 and the last at input 1, evaluated as a clamped B-spline (degree up to 3) that passes through the ends and bends toward the rest without overshoot; written as text, `0,0 0.5,0.35 1,1` | pen pressure, brush dynamics, tone and level curves, audio transfer curves |
| `Sequence`, `Track`, `Clip` | `sue.sequence` | Tracks of clips pointing at any entity, with trims, speed and split | video editing, DAWs, slideshows, podcasts, subtitles |
| `Notes`, `Note` | `sue.notes` | Note events with pitch, velocity, channel, lyric and expression | MIDI, piano roll, notation, singing synthesis |
| `NodeGraph`, `NodeRegistry` | `sue.graph` | Nodes with typed ports and parameters, links, evaluation order; `GraphEdit` edits | compositing, shaders, audio patches, procedural geometry, game logic |
| `Geometry`, `Transform` | `sue.geometry`, `sue.transform` | Double-precision points, polygons, polylines and Bezier prims with attributes per domain; placement | 3D, CAD, BIM, vector art, rigging, game levels |
| `PageLayout` | `sue.layout` | Pages (size, margins, bleed, masters) and frames showing other entities | publishing, books, slides, PDF pages, drawing sheets |
| `Encoded` | `sue.encoded` | The original bytes of a file Sue cannot decode yet | every format before its decoder exists |

Three shared shapes need no component of their own:

| Model | Stored as | Holds | Shared by |
| --- | --- | --- | --- |
| Layered pictures (`sue_model::picture`) | a `sue.image` entity without a base, ordered `CHILD` layers, shared properties | Size (`image.width`, `image.height`) and resolution (`image.dpi`); raster layers (any child with a `RasterTile2D` base, usually class `sue.layer`) and paper layers (`sue.layer.paper`, one `layer.color`); `LayerStyle` reads visibility, opacity, blend, clipping, draft and offset from `style.*` and `layer.*` properties | painting, photo editing, compositing, textures, thumbnails |
| Manuscripts (`sue_model::manuscript`) | a `sue.manuscript` entity, ordered `CHILD` folders and documents | A binder with draft, research and trash folders (`manuscript.role`); synopsis, notes, status, label, include, word target and keywords as `doc.*` properties (§3.8) | novels, scripts, theses, manuals, e-books, outlines |
| `BlendMode` | its name in `style.blend` | 26 blend modes (normal; darken, multiply, burns; lighten, screen, dodges, add; overlay and the lights; difference, exclusion, divide; hue, saturation, color, luminosity) and W3C compositing with straight alpha; names from a newer Sue fall back to normal | every feature that stacks layers |

A layer can be any raster entity, so a photo open in the Image Viewer can also
be a layer of a picture: the same entity, never a copy.

A feature that needs something new adds its own `ComponentType` the same way,
under its own prefix, and registers it.

---

## 2. First-class JPEG XL pipeline (`sue-io`; future `sue-jxl`)

### 2.1 Universal magic-byte sniffer

`sue_io::sniff_stream(&[u8]) -> IngestRoute` reads only content, never the file
extension, and sends each stream to one of three tiers:

| Tier | `IngestRoute` | Handles | Lands in |
| --- | --- | --- | --- |
| 1 | `Native(NativeFormat)` | Media with a native decoder: JPEG XL (codestream and container), JPEG, PNG, GIF, WebP, BMP, TIFF, PSD, ICO, AVIF, HEIF, MP3, FLAC, WAV, Ogg, MIDI, MP4/M4A, QuickTime, Matroska/WebM, AVI, glTF binary, PDF, fonts | `RasterTile2D`, `PCMFloat1D`, `TimeSpan`, `MeshBuffer3D`, `VectorSpline2D`, `StringBuffer` |
| 2 | `Structured(StructuredFormat)` | UTF-8 text with structure: JSON, XML, SVG, HTML, TOML / `.suerice`, Markdown, scripts, plain text | `ASTNode`, `StringBuffer`, `VectorSpline2D` (SVG) |
| 3 | `ByteMap(ByteMapHint)` | Anything else: an interactive hex view plus a **byte map** (bytes rendered as a `Gray8` raster) | `RasterTile2D` |

Detection order:

1. Fixed signatures at offset 0, longest first (for example the 12-byte JXL
   container signature before the 2-byte codestream marker `FF 0A`).
2. Structured containers: `RIFF` + form type (`WAVE`, `WEBP`, `AVI `), and ISO-BMFF
   `ftyp` + major brand (`avif`, `heic`/`mif1`, `qt  `, `M4A `, others → MP4).
3. Weak signatures with plausibility checks: an MPEG audio frame sync must have
   valid version and layer bits, BMP must have zero reserved bytes, and PE must
   point at `PE\0\0`.
4. Text: the first 4 KiB must be valid UTF-8 (a code point cut at the window edge
   is accepted) and free of control bytes. A BOM is skipped, then the leading
   syntax is classified. TOML is recognised by `[table]` or `key = value` lines,
   so `.suerice` files are not mistaken for Markdown despite their `#` comments.
5. Otherwise Tier 3. `ByteMapHint` carries the Shannon entropy in bits per byte
   (above 7.5 suggests compressed or encrypted data) and, when known, a binary
   signature label (ZIP, gzip, zstd, 7z, xz, ELF, PE, Mach-O, WASM, SQLite).

An empty stream is an empty plain-text document.

### 2.2 JPEG XL everywhere

JPEG XL is Sue's **default internal image format**: internal image and texture
caches default to `.jxl`.

| Use | Feature relied on |
| --- | --- |
| 120 FPS thumbnails of folders full of pictures | Progressive decoding. The DC pass (1:8 scale) is decodable from the first bytes, so a thumbnail never waits for the full file. |
| Multi-layer raster canvases | Native layers / extra channels; one `.jxl` per document |
| 32-bit float HDR 3D textures | Float samples and wide-gamut / HDR colour encodings |
| Importing existing JPEGs | **Lossless JPEG recompression**: about 20% smaller, with a `jbrd` box that reconstructs the original JPEG **bit for bit** |
| Undo history of raster edits | Tile cache of encoded tiles (§2.3) |
| `.suerice` assets | Wallpapers and icons shipped inside themes |

`JxlPipeline` is the codec-facing trait:

| Method | Purpose |
| --- | --- |
| `probe` | Parse the container or codestream header without decoding pixels |
| `decode(bytes, ProgressiveTarget)` | Decode to `DcPreview` (1:8), a given pass, or `Full`, into an `Rgba8` `RasterTile2D` plus `JxlImageInfo` |
| `transcode_jpeg_lossless(jpeg)` | JPEG → JXL with reconstruction data |
| `reconstruct_jpeg(jxl)` | JXL with `jbrd` → the original JPEG bytes |
| `encode_layers(layers, effort)` | Encode a layered canvas |

`probe` is implemented natively. It validates the container signature
`00 00 00 0C 'JXL ' 0D 0A 87 0A`, walks ISO-BMFF boxes (32-bit, 64-bit and to-EOF
sizes), recognises `ftyp`, `jxll` (level), `jxlc` (codestream), `jxlp` (partial
codestream with a 4-byte sequence index), `jbrd` (JPEG reconstruction), `Exif`,
`xml ` and `brob` (Brotli-wrapped boxes). It then decodes the codestream
`SizeHeader` after the `FF 0A` marker. That header is bit-packed least
significant bit first:

```text
small: Bool
if small:  ysize = (u(5) + 1) * 8
else:      ysize = U32(1 + u(9), 1 + u(13), 1 + u(18), 1 + u(30))
ratio: u(3)
if ratio == 0:  xsize = (small ? (u(5) + 1) * 8 : U32(…))
else:           xsize = ysize * {1/1, 12/10, 4/3, 3/2, 16/9, 5/4, 2/1}[ratio - 1]
```

`NativeJxlPipeline` is the pipeline Sue ships:

- **Decoding** uses the pure-Rust `jxl-oxide` decoder. The first frame is
  rendered to 8-bit samples in the image's encoding (sRGB for typical files), and
  gray, gray + alpha and RGB are widened to RGBA. `DcPreview` box-filters the
  full decode by 8; until the streaming decoder lands, `Pass(n)` decodes fully.
  CMYK images are reported as unsupported.
- **JPEG reconstruction** (`reconstruct_jpeg`) rebuilds the original JPEG bit for
  bit from the `jbrd` box through `jxl-oxide`, after checking
  `jpeg_reconstruction_status`.
- **Encoding** (`encode_layers`) is lossless modular encoding with the pure-Rust
  `zune-jpegxl` encoder, for one `Gray8`, `Rgba8` or `RgbaF32` layer of at least
  2×2 pixels. A test encodes and decodes a test card and checks every pixel.
- **Lossless JPEG recompression** (`transcode_jpeg_lossless`) still returns
  `JxlError::CodecUnavailable`: no pure-Rust JPEG→JXL transcoder exists yet.

`sue_io::ingest_bytes` turns any opened or dropped file into one entity, routed
by content: JPEG XL decodes through the pipeline, PNG and JPEG through
`zune-png` / `zune-jpeg`, all into `Rgba8` rasters; MP3, FLAC, WAV, Ogg Vorbis
and AAC / M4A decode through `symphonia` into `PCMFloat1D` (`probe_audio`
reads a file's tags and length without decoding it); text becomes a
`StringBuffer`; anything else, including media Sue has no decoder for yet,
becomes a `Gray8` byte-map raster. Each entity carries metadata (`bytes`,
`route`, `format`, size, decode time, colour and depth, codec and tags,
entropy, and the source path). For the converter, `encode_png` writes
compressed PNG; `encode_wav_as`, `encode_aiff` and `encode_flac` (with
`flacenc`) write WAV (16, 24 or 32-bit float), AIFF and FLAC; and
`decode_picture` and `encode_picture` read and write BMP, GIF, ICO, PNM, QOI,
TGA, TIFF and WebP through the `image` crate (`PictureFormat`).
`PICTURE_SAVE_TYPES` lists every picture format Sue writes (PNG, JPEG XL,
JPEG, WebP, TIFF, BMP, TGA, GIF, QOI) for a Save As type list, and
`encode_as` writes a raster as the one an extension names (JPEG over white,
JPEG XL losslessly).

### 2.3 Tile cache

`TileCache` keeps encoded tiles in memory under a byte budget, keyed by
`TileKey { layer, level, x, y }`, where `level` is a mip or progressive level.
Eviction is least-recently-used, ordered by a monotonic tick in a `BTreeMap`, so
`get`, `insert` and eviction cost $O(\log n)$. A tile larger than the whole
budget is refused rather than flushing the cache. `invalidate_layer` drops every
tile of an edited layer. Hits, misses and evictions are counted for the
diagnostics overlay. The cache stores opaque bytes; by default those bytes are
`.jxl`-encoded tiles.

---

## 3. Adaptive cross-platform window manager (`sue-wm` & `sue-platform`)

### 3.1 Platform isolation

`sue-platform` is the **only** crate allowed to touch OS-specific APIs
(`windows-sys`, `libc`, `jni`, `objc`, and so on). Everything above it talks to
the `PlatformHost` trait and the portable services next to it:

| `PlatformHost` method | Abstracts |
| --- | --- |
| `os`, `form_factor` | `OsFamily::{Windows, Linux, MacOs, Android, Ios, Other}`, `FormFactor` |
| `capabilities` | `HardwareCapabilities`: logical cores, refresh rate, graphics API, GPU tier and vendor, memory, power source, touch, stylus |
| `paths` | `PlatformPaths`: config, data, cache and home folders, plus derived `rice_dir`, `packages_dir`, `models_dir` |
| `create_surface`, `resize_surface`, `destroy_surface`, `surface` | Window / swap-chain surfaces with logical size, scale factor and safe-area insets |
| `poll_events` | Drains `InputEvent`s |

| Service | What it does |
| --- | --- |
| `AudioOutput` / `AudioSink` | A `cpal` output stream fed from a lock-free ring (§3.5); `NullAudio` is a device-free clock for tests |
| `list_dir`, `hidden_by_os`, `write_atomic` | Sorted folder listings (the Converter adds a whole folder), without dot files or, on Windows, the legacy junctions (`NetHood`, `PrintHood`, `Cookies`, `SendTo`, `Recent`, `Start Menu`, `Templates`, `Application Data`, `Local Settings`, `My Documents`); crash-safe writes (temporary file, sync, rename) |
| `copy_text` | The system clipboard (`arboard`) for buttons such as Copy All; text fields copy and paste through `egui-winit` |
| `EguiPainter`, `TextureUpload` | Paints egui frames with `egui-wgpu` into the window or an offscreen texture; uploads image pixels the caller borrows from the arena straight into GPU textures (rule 5) |
| `open_url`, `reveal_folder` | The system browser (only `http` and `https`, validated by `web_url`) and file manager |
| `pick_open_file`, `pick_save_file`, `pick_folder` | The system's own open, save and folder dialogs: every file Sue asks for goes through them. Each answers a `Picking`: `Chosen(path)`, `Cancelled`, `Failed(why)`, or `Waiting` on phones and tablets (below) |
| `poll_picks`, `deliver_picked_files`, `picks_busy` | The answers of pickers that run beside Sue (`PickNews`), the hand-back of files written to the paths they chose, and whether to keep polling; no-ops on desktops |

Path resolution is a pure function of the OS and an injected environment lookup,
so it can be tested without touching the real environment:

| OS | Config | Data | Cache |
| --- | --- | --- | --- |
| Windows | `%APPDATA%\Sue` | `%LOCALAPPDATA%\Sue` | `%LOCALAPPDATA%\Sue\cache` |
| macOS | `~/Library/Application Support/Sue` | same | `~/Library/Caches/Sue` |
| Linux / other | `$XDG_CONFIG_HOME/sue` or `~/.config/sue` | `$XDG_DATA_HOME/sue` or `~/.local/share/sue` | `$XDG_CACHE_HOME/sue` or `~/.cache/sue` |
| Android / iOS | `<sandbox>/config` | `<sandbox>/data` | `<sandbox>/cache` |

Relative `XDG_*` values are ignored, as the XDG Base Directory specification
requires. On mobile, the sandbox root comes from the native runtime (for example
`Context.getFilesDir()` or `NSFileManager`).

Two hosts implement the trait:

- `HeadlessHost` has virtual surfaces and an injectable input queue. CI, the
  self-check, offscreen snapshots and remote scripting use it.
- `DesktopHost` opens one native window titled `Sue` (`winit` 0.30) with a
  `wgpu` 30 surface, sRGB where available and presented with vsync, and runs
  egui 0.37 on it: `egui-winit` turns OS events into egui input (mouse, wheel,
  keyboard, text and IME, the clipboard, file drops) and `EguiPainter` draws
  each frame. It fills `HardwareCapabilities` from the adapter and the
  monitor's refresh rate. The application builds every frame into egui's root
  `Ui` through `DesktopApp::ui`.

**GPU choice.** The window draws on the GPU picked by `display.gpu_preference`
(`GpuPreference`). `auto`, the default, uses the GPU that drives the display: on
Windows the adapter DXGI lists first (which also follows the per-app choice in
the Windows graphics settings), matched by vendor and device id through the
first backend `wgpu` itself would choose; elsewhere the integrated GPU, which
drives the screen on most laptops. `integrated` and `discrete` map to `wgpu`'s
`LowPower` and `HighPerformance`. The rule decides the idle cost: when one GPU
draws and the monitor hangs off another, Windows copies every presented frame
between adapters, and that path kept a time-critical driver thread spinning at
a full core while Sue slept at 0 FPS (measured with an AMD integrated GPU
drawing for a monitor on an NVIDIA card; on the display GPU the same idle
window used 0.3% of one core). The value is read from the rice file before the
window opens, so a change applies after a restart. Offscreen rendering
(snapshots, tests) uses the integrated GPU, since nothing is presented.

**Frame pacing (the event loop).** `run_desktop(app, options)` drives a
`DesktopApp`. Every time the loop is about to wait (`about_to_wait`) it combines
two questions: the application's `FramePacing`, and when egui wants its next
frame (`repaint_delay` of the last frame: zero while a widget animates or the
shell called `request_repaint` or the editor's caret glides, the caret's blink
time while a text field has focus, never otherwise).

| Answer | Loop step |
| --- | --- |
| `Animate { target_hz }`, or egui's repaint is due | Request the next redraw now when vsync paces it; with a lower cap, wait for the next frame slot after the last frame |
| `WakeAt(t)` or a later egui repaint | `ControlFlow::WaitUntil` the earlier one; a due `WakeAt` runs `DesktopApp::timer`, which redraws only if it asks to |
| `Sleep`, and egui wants nothing | `ControlFlow::Wait`: no frames and no CPU until input arrives |

The loop never requests a redraw from inside the redraw handler (some platforms
drop such requests); it does so from `about_to_wait`, so an animation started by
a click keeps its frames coming with no further input. Input events ask for one
redraw each; `egui-winit` also reports the redraw event itself as needing a
repaint, and the loop ignores that one, or it would draw forever. Closing the
window asks the application first (`close_requested`), which can show a
confirmation, and `exit` lets it save before the loop ends.

### 3.2 Form-factor topologies

| `FormFactor` | Input model | Geometry |
| --- | --- | --- |
| `Desktop` | Mouse + keyboard, keybinds from rice | The top bar is `layout.bar_height` (28 points) tall |
| `Tablet` | Stylus pressure curves ($p' = p^{\gamma}$), larger targets | The top bar is at least 36 points tall; Sue Paint docks its tool strip and layers and iconizes the rest |
| `Mobile` | Touch | The top bar is at least 52 points tall, for 44 point buttons; the Hub shows two columns of boxes; Sue Paint moves its palettes into bottom sheets |

Classification follows Android's resource-qualifier convention: on a mobile OS, a
smallest screen side of **at least 600 dp** is a tablet (`FormFactor::classify`).
Desktop operating systems are `Desktop`.

People can override it: `display.interface` (`auto`, `desktop`, `tablet`,
`phone`) goes through `FormFactor::chosen` before the shell lays out or hands
`ShellView::form_factor` to applets, so a phone can run the desktop layout and
a desktop the touch ones. Every form factor has the same structure: the top
bar with the Sue button, the page (the Hub, a studio or a tool) across the
whole width, and the status bar. Sue Paint draws its own phone layout
(`phone.rs`: its row of 44 point buttons in the top bar after the Sue button, a
bottom bar of them, palettes and menus as bottom sheets) and, on tablets, docks
the tool strip and layers and iconizes the rest. Two-finger pan, pinch and turn come from egui's
`multi_touch`; a second finger takes back the stroke the first began (the
stroke is finished and undone, so no half stroke stays), and short two- and
three-finger taps undo and redo.

**Phones and tablets.** `sue-platform` builds for Android and iOS with the
desktop-only crates (`arboard`, `rfd`, `open`, `audio_thread_priority`) left
out by target. On Android, `crates/sue-platform/android` is a `cdylib` whose
items are the exported `android_main` and `java` (the `unsafe_code` lint is
relaxed there from `forbid` to `deny`, with an audited `allow` on each,
approved by the maintainer on 2026-10-28 for the entry and on 2026-10-29 for
`java`, which wraps the raw JNI pointers of Java's VM and the activity once);
it hands the `AndroidApp`, the VM and a global reference to the activity to
`sue_platform::android` and runs `sue::run_window`, the same
app the desktop executable runs, then ends the process, because a window's
event loop can be made once per process. The host:

- lays egui out in the content rectangle Android reports (`raw.screen_rect`),
  so nothing hides under the system bars (the app theme opts out of forced edge
  to edge);
- tries Vulkan alone and then OpenGL ES alone, since an Android window takes
  only one graphics API;
- drops the surface on `suspended` and makes a new one on `resumed`, skipping
  resizes in between;
- keeps config and data in the app sandbox (`internal_data_path`).

iOS uses the ordinary executable in an app bundle; its sandbox is `HOME`.
egui lays out over the whole window there (`egui-winit` takes winit's outer
size), and the host shrinks `raw.screen_rect` from the bottom by
`sue_ios::keyboard_inset()`, the points the on-screen keyboard covers. That
crate observes `UIKeyboardWillChangeFrameNotification` and
`UIKeyboardWillHideNotification` on the main queue with blocks (approved by
the maintainer on 2026-10-29), keeps the keyboard's end frame in screen
coordinates, and converts it into the key window's coordinates when asked,
so a window that does not fill the screen (Slide Over, Stage Manager) loses
only the part the keyboard really covers. A keyboard that floats or is split
on an iPad, or does not reach the window's bottom edge, covers nothing, as
UIKit's keyboard layout guide counts it (`cover.rs`, tested on every
system). The keyboard moves with no winit event, so `about_to_wait` compares
the inset with the last frame's (`SurfaceInfo::safe_area.bottom`) and
redraws when it changed.

**File pickers on phones and tablets.** Desktops show `rfd`'s dialogs, which
block until they close. Android and iOS show the system's own picker beside
Sue: it answers later, and the place chosen is not a path Sue can write.
`sue-platform/src/picks.rs` bridges both with one engine, `Picker<S: System>`,
over a small `System` trait (show, answer, name, copy in, copy out, make a
document in a folder, export), so the whole flow runs headless in tests with a
stand-in:

- a pick answers `Picking::Waiting`; the runtime keeps the `FilePick`, wakes
  every 200 ms while `picks_busy`, and turns the answer from `poll_picks` into
  `ShellRequest::Picked`, the same request a desktop dialog makes;
- a chosen file is *staged* under `<cache>/picked/<n>/<name>`, keeping the
  name the system shows (it says the format): an opened document is copied
  there on a worker thread and that copy's path is the answer;
- `deliver_picked_files` (after every `serve_requests`) compares each staged
  file's size and time of change with the last hand-back and copies a written
  file back on a worker thread; hidden files (`write_atomic`'s temporaries)
  are skipped, and a failure comes back as `PickNews::Failed`;
- the staging folder is emptied when Sue starts, since what it held stood for
  places of the last run.

| | Android (`android::Documents`) | iOS (`ios::Files`) |
| --- | --- | --- |
| Reached through | `SueActivity` (`packaging/android/java`), a `NativeActivity` subclass with no native methods, called with `jni` 0.21's safe API on a `GlobalRef` the launcher makes: `showPicker`, `takePicked`, `displayName`, `copyIn`, `copyOut`, `createIn` and `listIn`, strings in and out (approved by the maintainer on 2026-10-29) | `sue-ios` (`crates/sue-platform/ios`): a `UIDocumentPickerDelegate` class and the calls that show the picker over the key window's topmost view controller |
| Open | `ACTION_OPEN_DOCUMENT` (MIME types from the extensions, any file when one is unknown); the copy's writes go back to the document | `initForOpeningContentTypes:asCopy:` with `UTType`s from the extensions; the copy is Sue's own |
| Save | `ACTION_CREATE_DOCUMENT` asks first; every write to the staged path (`Ctrl+S` too) goes to the document (`"wt"`, else `"w"`) | The staged path is the answer at once; once written, `initForExportingURLs:asCopy:` asks where it goes, each time |
| Folder | `ACTION_OPEN_DOCUMENT_TREE`; the tree's top-level files are copied in first on a worker thread (`listIn`, at most 500, hidden ones left out) and each written back to its own document; each new file written in the staged folder becomes a document in the tree (`createDocument`), then is updated in place | The folder's own path, with security-scoped access started |

**Packages.** `packaging/` has a script per system: for Windows a portable
zip, an `.msi` (WiX, with Sue's icon on its pages and its Start menu
shortcut) and a setup `.exe` that wraps the `.msi` and carries the icon
itself; a Linux tarball, `.deb`, `.rpm` and AppImage (also buildable in
Docker from any system); a universal macOS app and disk image; an unsigned
iOS `.ipa` and a simulator build; and an Android APK for arm64, armv7 and
x86_64 linked, aligned and signed with the SDK's own tools (no Gradle), with
an adaptive launcher icon. Every Linux package names its desktop entry and
icon after the app id `io.github.MagiGun.Sue` (`sue_wm::brand::APP_ID`),
which the window also reports (Wayland's app id, X11's `WM_CLASS`), so the
desktop shows the icon on the window. On Windows `sue-platform` sets the
window's small and big icons from the one built into `Sue.exe` and shows
the window only after its first frame: the taskbar asks a newly shown
window for its icon once, and a window still starting up answers too late. The CI workflow runs
the four checks on Windows, macOS and Linux and builds Android and iOS; the
Release workflow builds every package on a `v*` tag and publishes them with
checksums.

### 3.3 Layout solver

`solve_layout(&LayoutRequest) -> LayoutSolution` is a pure function. It names
every region in screen points: the top `bar`, the `sue_button` square at its
left and the `menus` after it, the `page` and the `status` bar.

```text
┌────────┬──────────────────────────────────────────────────────────────────┐
│  Sue   │ File  Edit  ...  (the open studio's or tool's menus)             │ bar
├────────┴──────────────────────────────────────────────────────────────────┤
│                                                                           │
│              the page: the Hub, a studio or a tool                        │
│                                                                           │
├───────────────────────────────────────────────────────────────────────────┤
└──────────────────────────── status bar (22 px) ───────────────────────────┘
```

- **Inputs:** viewport, safe-area insets, form factor and `LayoutMetrics` (the
  bar and status bar heights), read from rice.
- **The bar** is `layout.bar_height` tall on a desktop, at least 36 points on
  a tablet and 52 on a phone (`bar_height`); the Sue button is as wide as the
  bar is tall (12 points wider on a desktop).
- **Invariants (tested):** the bar, the page and the status bar stack without
  gaps, the page has the whole width, the Sue button sits in the top left
  corner, safe areas keep the bars clear of notches, and a tiny window still
  lays out in order.

### 3.4 Hybrid render scheduler

Sue redraws **only when something changed**:

```rust
pub enum RenderDemand { Sleep, ActiveFrame { target_hz: u32 } }
```

`FrameScheduler` collects damage rectangles (merged by union) and an "animating"
flag. With no damage and no running animation it reports `Sleep`. The frame-rate
target comes from the active `HardwareProfile`, lowered by
`display.frame_rate_cap` when the user sets one:

| `HardwareProfile` | Frame cap | Shader passes | Spring physics | Blur | Graphics path |
| --- | --- | --- | --- | --- | --- |
| `HighRefresh` | display rate (240 Hz+) | 8 | yes | yes | Vulkan / DX12 / Metal, multi-pass |
| `Balanced` | 60 | 4 | yes | yes | Vulkan / DX12 / Metal |
| `LegacyLowSpec` | 30 | 0 | **no: instant 0 ms snap** | no | OpenGL / GLES |
| `MobileBatterySaver` | 30 | 1 | yes | no | GLES / Vulkan / Metal |

`HardwareProfile::recommend` picks a profile from `HardwareCapabilities`:
low-power mode becomes `MobileBatterySaver`; a software or legacy GPU, or at most
2 cores, becomes `LegacyLowSpec`; 120 Hz or more with a high-end GPU becomes
`HighRefresh`; otherwise `Balanced`. The user can override it with
`profile.hardware`.

**Pacing in the runtime.** The runtime answers the host's pacing question from
these facts, in order:

1. A spring is moving (a page coming in or going out, a window, the palette):
   `Animate`. The shell also calls `request_repaint`, so egui asks for the next
   frame too.
2. Within `motion.settle_grace_ms` (300 ms) of the last change: `Animate`.
3. Timed work is pending: `WakeAt(earliest)` for the next audio top-up while
   something plays (every 40 ms), a burst of typing to close into one undo step
   (`TYPING_PAUSE`, 900 ms), or a status message to clear.
4. Otherwise `Sleep`: 0 FPS and no CPU. A focused text field still blinks its
   caret through egui's own repaint delay (`motion.caret_blink_ms`; 0 keeps it
   solid and the window fully asleep). The rich text editor's caret asks egui
   for every frame only while it glides, then for one at each turn of its
   blink, and for none once it has rested `sue_type::caret::BLINK_FOR` (10 s).

Each frame measures real elapsed time, `dt = now - last_frame` clamped to
$[0.0001, 0.05]$ s; the first frame after sleeping advances by one nominal
frame. Springs see `dt x motion.animation_speed`. The self-check drives the
whole shell through real egui frames and asserts that a page clicked open in
the Hub glides in over many frames (and snaps in one on `LegacyLowSpec`), and
that afterwards neither the runtime nor egui asks for a frame.

**Spring physics.** Everything that moves uses a damped harmonic oscillator,
$F = -kx - cv$, so $m\ddot{x} = -k(x - x_\text{target}) - c\dot{x}$. The
damping ratio is $\zeta = c / (2\sqrt{km})$. Pages, windows and the palette
use $k = 260$, $c = 32$ ($\zeta \approx 0.99$). `SpringDamper` integrates with
semi-implicit (symplectic) Euler in sub-steps of at most 1/240 s. A spring is
**settled** when $|x - x_\text{target}|$ and $|v|$ are both below $10^{-3}$;
it then snaps exactly to the target. Under `LegacyLowSpec`, or with
`motion.instant_transitions`, `SpringMode::InstantSnap` skips integration:
transitions take 0 ms.

**Painting.** egui tessellates every frame and `EguiPainter` draws it with
`egui-wgpu`. Images (pictures, Sue Paint's layers) are uploaded from the arena's own bytes into
native textures by key and version: a layer's version is its `revision`
metadata, bumped by every stroke and by undo and redo. Textures no frame used
are freed.

**Telemetry.** The status bar shows where you are (the Hub, or the studio or
tool) and the latest message on the left; the arena entity count, the undo
depth, the hardware profile and the frame state (`Idle` while asleep, `N FPS`
measured over the last second while animating) on the right.
A file written anywhere (`cx.saved(path, note)`, `ShellRequest::Saved`) is
reported with its name, a note and its folder for at least ten seconds; a
click on that message runs `reveal-file:<path>`, which shows the file
selected in Explorer or Finder (`sue_platform::reveal_file`), or its folder
elsewhere.

### 3.5 Real-time audio thread isolation

The device callback runs on the audio backend's real-time thread and
**MUST NOT** allocate, lock, log or make syscalls. `AudioOutput` meets that
with a single-producer, single-consumer ring of `f32` samples stored as atomic
bit patterns (safe Rust, no `unsafe`): the UI thread pushes, the callback pops,
and a flush index lets the producer drop queued audio after a seek without a
lock. Volume is an atomic gain applied in the callback, so it takes effect at
once. A paused stream is stopped at the device and costs no CPU.

```text
UI / arena ──(SPSC ring: interleaved f32 samples)──► audio callback
audio callback ──(atomics: frames played, underruns)──► UI
```

`AudioConfig` comes from rice: `audio.output_device`, `audio.sample_rate`
(44.1, 48 or 96 kHz, falling back to the device's rate), `audio.buffer_frames`
(used when the device accepts a fixed size) and `audio.realtime_priority`, which
registers the callback thread with the OS real-time scheduler
(`audio_thread_priority`: MMCSS on Windows) on its first call and reports
whether the OS granted it. The stream opens when something first plays and
reopens when its settings change. The shared `Transport` (Sue Paint's
animation sound uses it today) plays a `Clip` whose `ClipSource` is an arena
entity or a `Shared<PCMFloat1D>` held outside the arena (a mix made on the
fly, which is not project data); it tops the ring up every 40 ms from the
next slice of the clip, resampled linearly to the
device rate; the ring is the device boundary, like a texture upload. A DAW graph
will run on the same ring discipline.

### 3.6 Applets, the Launchpad, the Hub and the top bar

Every studio, tool and window implements `SueApplet`:

```rust
pub trait SueApplet: Send {
    fn manifest(&self) -> AppletManifest;   // id, name, summary, role, icon, order
    fn rice_declarations(&self) -> Vec<RiceDeclaration> { Vec::new() }
    fn accepts(&self, entity: &Entity) -> bool { false }            // what it works on
    fn templates(&self) -> Vec<FileTemplate> { Vec::new() }         // the Sue menu's New
    fn create(&mut self, key: &str, cx: &mut AppletContext<'_>) {}
    fn focus(&self) -> Option<EntityId> { None }                    // what the page works on
    fn bar_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {}  // its menus, after the Sue button
    fn page_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {} // its page
    fn window_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {}
    fn tick(&mut self, cx: &mut AppletContext<'_>) -> Tick { Tick::default() }
    fn commands(&self) -> Vec<PaletteEntry> { Vec::new() }
    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool { false }
    fn navigate(&mut self, target: &str) -> bool { false }        // e.g. a Settings page
    fn captures_keys(&self) -> bool { false }                      // e.g. recording a shortcut
    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {}
    fn sue_actions(&self, focus: Option<&Entity>, current: bool) -> Vec<SueAction> { Vec::new() }
    fn take_entity(&mut self, purpose: &str, entity: EntityId, cx: &mut AppletContext<'_>) {}
    fn opens_file(&self, path: &Path) -> bool { false }
    fn reserved_keys(&self) -> Vec<Keybind> { Vec::new() }
}
```

The **role** decides where an applet lives. A `Studio` is a whole workspace
for making something (Sue Paint, Sue Writer); a `Tool` does a focused job on a
file or an entity (the Image Viewer, the Converter). Both fill the **page**
under the top bar (`page_ui`) and put their menus in the bar after the Sue
button (`bar_ui`), so a tool opens exactly like a studio. A `Window` (Settings)
floats over the page. There are no side panels.

**The Hub** is the page Sue opens on, a full page and not an overlay: a
heading **Studios** with a box for every `Studio` manifest, then **Tools** with
a box for every `Tool`, in manifest order. Each box has sharp corners and
shows the applet's icon, name and summary; a click opens it
(`ShellRequest::Open`). At each box's bottom right is a **pin**. Pinning makes
that studio or tool the one Sue opens straight into at launch, already in
place; there is one pin at a time, so pinning another moves it, and a click
on the pinned one takes it away (`ShellRequest::Pin`). The pin is the rice
key `launchpad.pinned` (empty for the Hub), set as one undoable `RiceEdit`,
so `Ctrl+Z` takes it back, and it shows in Settings, General, Startup as a
drop-down of the Hub and every studio and tool. A narrow window or a phone
gets two columns of boxes and 44 point pins.

Under the boxes the Hub lists the **work Sue remembers** (`WorkList`, below):
**Recent** (the files and projects opened or saved lately, newest first, as
many as `launchpad.recent_count`), **Saved Projects** (every `.sue` project
the list knows, by name, with the studio it was saved from) and **Unsaved
Work** (autosaves of other runs). Three columns side by side when the page
is wide enough, one under the other when not, each row a sharp box with the
title, what it is, when and the folder, the full path on hover, 56 points
tall on touch. A recent file or project opens with `ShellRequest::OpenFile`;
kept work has **Recover** (also a click on the row, `ShellRequest::Recover`)
and **Discard** (`ShellRequest::Discard`), which arms on a first click
(Really Discard) and acts on a second. `Shell::hub_rows` reports where each
row and button was drawn, for scripted runs and tests.

**The top bar** runs across the window (§3.3). The **Sue button** sits once,
in its top left corner, with Sue's silhouette; no studio or tool draws one of
its own. Right after it come the open page's menus: a studio or tool draws
them in `bar_ui` with a `MenuBar` (menus are data, `Menu` and `MenuItem` with
a command and a shortcut; each frame the applet says per command whether it
works and whether it is checked, `ItemState`, so items whose feature is not
built yet are grayed out). On the Hub the bar reads Hub. On a phone Sue Paint
puts its row of large buttons there instead. While a page comes in or goes
out, its part of the bar fades with it and takes no clicks.

**The Sue menu** (`ShellRequest::SueMenu`, or a click on the Sue button) drops
down under the button, built for what is open (`MenuBar::dropdown`):

| Item | What it does |
| --- | --- |
| Hub | Back to the Hub (checked there) |
| Studios, Tools | Every studio and every tool (the open one checked) |
| New | A submenu per kind of file from the applets' `templates` (a `FileTemplate` names a category such as Picture, the exact kind such as Square Post, and a key the applet's `create` gets back; the applet makes it and opens itself) |
| File | Open File (`Ctrl+O`), Open Recent (the `WorkList`'s ten newest), and what applets add there |
| Project | Open Project, Save Project (`Ctrl+S`), Save Project As |
| Send To | What the page works on (`focus`), into every other studio or tool whose `accepts` takes it (`ShellRequest::OpenIn`), and what applets add (`SueAction`s, such as Converter, or Sue Writer for a picture) |
| Any other submenu | `SueAction`s on a menu of an applet's own |
| Open ... at Launch | The open page's pin (checked when it is pinned) |
| Command Palette, Settings, Quit Sue | With their shortcuts |

Applets add to it with `sue_actions(focus, current)`: `SueAction`s on a named
submenu (`sue_menu::FILE`, `sue_menu::PROJECT`, `sue_menu::SEND_TO`, or one of
their own) with the request each runs; an item whose label is already on its
submenu is not added twice, and a submenu with nothing on it does not show.
Studios offer nothing for themselves while open, since their menus sit right
beside the Sue button.

**Shared features.** Sue owns its features and every studio uses them: one
color wheel (`color_button`), one way to pick and save files (`FilePick`,
answered by the system's dialogs), one Converter, one theme system, and the
shared models. When a studio needs more, the shared feature gains a mode or
parameter. Work moves between studios by `EntityId`: `ShellRequest::OpenIn {
entity, applet }` hands an entity to a studio or tool (`take_entity` with the
purpose `open`) and opens its page, and `ShellRequest::SendEntity` hands one
to an applet for a purpose of its own (`figure`, `add`, `adopt`).

**Opening files.** A file opened anywhere (Open File, a drop on the window)
goes first to an applet whose `opens_file` takes it as it is (Sue Writer:
Word, Markdown, HTML, Fountain and plain text). Anything else is read into
the arena through the ingest path (§2.2), a `.sue` project replacing the
document, and opens in the first studio, then tool, whose `accepts` takes the
entity (`Applets::page_for`): a picture in the Image Viewer. A file nothing
opens is not kept in the project, and the status bar says so.

**The work Sue remembers.** `sue_wm::WorkList` is one list for every studio
and tool, kept in Sue's data folder (`<data>/work`), read by the shell for the
Hub and the Sue menu and by applets through `AppletContext::work` (Sue
Paint's File, Open Recent filters it to the pictures it opens). Only the
runtime changes it; applets ask with `ShellRequest::Remember { path, saved }`
and `ShellRequest::Forget`. `recent.txt` holds one line per file (when it
was opened, when saved, the page it was worked on in, the path), newest
first, at most 100, under a format line. The runtime remembers every file
`open_path` opens and every project `save_project_to` writes; files that are
gone drop off when the Hub opens (`WorkList::refresh`). On first use the list
takes over Sue Paint's old `paint/recent.txt`.

**Autosave.** Work nobody saved is kept by itself: `general.autosave_seconds`
(60; 0 turns it off) after the arena's revision changes, and at quit
(`Runtime::shutdown`), the runtime writes the project as it would be saved
into `<data>/work/autosave/<id>.sue` with a note (`<id>.txt`: when, the
project file it belongs to, what it holds, its page). A digest of the bytes
skips work that equals the saved file or the last copy kept, and saving the
project lets the copy go. The id is the run's own (seconds, process and a
count), so runs never share one, and a run never lists its own kept work.
Opening another project (or recovering kept work) first keeps the work it
replaces, then starts a new id. Recovering loads the copy as the project,
with its project file, opens it in its page (`open_first_page`: the page it
names if it takes something, else the first studio, then tool, that does)
and adopts its id, so it stays kept until saved. Kept work its project file
has since caught up with is let go when the Hub opens. Only a runtime that
remembers (`Runtime::remembering`, on in the desktop and mobile window)
touches the data folder; tests choose it, and the self-check never does.

**Asking for a file.** No applet opens a dialog itself. It sends
`ShellRequest::PickFile(FilePick)` (who asks, what for, a title, the
extensions or the kinds of file it takes, a file name when saving, a folder
to start in, or a folder instead of a file); the runtime shows the system's
own open, save or folder dialog (`sue_platform::pick_open_file`,
`pick_save_file`, `pick_folder`, the kinds as its filters,
`FilePick::filters`). The chosen path comes back as `ShellRequest::Picked`,
which the shell delivers to the asking applet's `picked`; a pick with no
applet behind it (opening a file or a project, saving the project, exporting
the rice) goes to the engine. A headless run keeps the pick for a script to
answer (`Runtime::answer_pick`), which is how the tests and snapshots choose
files.

**Studio themes.** A studio or tool whose rice section (its id with `-` as
`_`) has a `theme` wears `Theme::for_studio`: a preset's colors, the Sue theme
(`sue-theme`, the global colors, so it follows every change to them), or its
own colors in `<section>_colors` (`custom`). Only the colors change. The
shell sets egui's visuals to the page's theme while the page and its part of
the top bar draw, so its menus and popups wear it too, and back to Sue's for
the status bar, the palette and Settings. The page's and the bar's own `Ui`s
are handed that style (`UiBuilder::style`): an egui child takes its parent's
style, not the context's, so without it a studio in Gray inside a Paper Sue
got Paper's white fields. A light theme (`Theme::is_light`, a light page)
starts from egui's light visuals, so what egui picks by itself reads on it.

**Shortcuts.** Every applet assigns keys the same way, through
`sue_wm::shortcuts`: it lists its commands with their default chords
(`Shortcut::new("file.save_as", "Save As", "Ctrl+Shift+S or Ctrl+Alt+S")`),
`declarations` turns each into a key setting in the applet's own section
(`<section>.<command>`, dots as underscores, such as `sue_paint_keys`,
`sue_writer_keys` and `image_viewer_keys`), and `take` consumes this frame's
presses of the chords as they are set now. A `Keybinds` value holds any
number of chords, written `A or B`; `keys::consume` matches exact modifiers,
so `Ctrl+S` never fires for `Ctrl+Shift+S`. `shortcuts::Bound` keeps an
applet's menus showing its current first chords and gives the shell the
chords the open page keeps for itself (`SueApplet::reserved_keys`). The
shell's own keys (`[keys]`: the palette, Settings, open, save, quit, undo
and redo) work the same way; `Ctrl+O` asks the open page first.

**Sue's icon.** `assets/icons` holds the app icon and the bare silhouette,
each as SVG (the source), PNG and JPEG XL at 1024 pixels, and a Windows
`.ico` with six sizes, plus inverted and black versions. `sue_wm::brand`
reads the `.ico` sizes: the app icon is the window's and the taskbar's
(`DesktopOptions::icon`), and `brand::mark` draws the silhouette, white on
clear, tinted to the theme's text color, on the Sue button. On Windows,
`bins/sue/build.rs` copies the icon's sizes into a compiled resource file the
MSVC linker takes, so `sue.exe` itself carries the icon (plain Rust reading
Cargo's target variables, no resource compiler).

**Going in and out of a page.** Opening a studio or tool from the Hub, its
reveal spring runs from 0 to 1: the page, drawn on a layer above the Hub,
fades in and a studio slides its palettes in from the window's edges
(`ShellView::reveal`); a shield takes the clicks until it is in place, and
from then on the Hub is not drawn under it. Going back to the Hub
(`ShellRequest::Hub`, the Sue menu's Hub, or a studio's Back to the Hub) runs
the reveal back to 0 over the Hub, which takes clicks at once. Opening one
page from another switches straight to it. A pinned page opens at launch
already in place. Every spring follows `motion.animation_speed` and
`motion.instant_transitions`.

Nothing in the shell names an applet: the Hub's boxes and the Sue menu's
Studios and Tools come from manifests' `role` and `order`, New from the
applets' `templates`, Send To from `accepts` and `sue_actions`, the palette
from `commands`, and opened files go to the first applet that `opens_file`
or `accepts` them. `Launchpad` is the runtime registry of factories;
`Applets` is the set of live instances, sorted by `order`.

Applets hold `AssetHandle`s and `EntityId`s, never media. They change the
world only through the global `UndoStack<World>` (`cx.apply`, `cx.edit_rice`,
or `cx.record` for an edit that already happened in place), and ask for
things through `cx.request(ShellRequest::…)`: open a studio or tool or the
Hub, pin one, make a new file from a template, open an entity in an applet,
send one, ask for a file, run an engine command, open Settings or the
palette, show a message.

**The world.** `World` holds the arena and the live rice: every project is
the arena's entities and relations (§1.9), with no layout of its own.

**The shell** (`Shell::ui`) draws, in order: the Hub (unless a page covers it
whole), the page, the top bar with the Sue button and the page's menus, the
status bar, then the floating surfaces:

| Surface | Behavior |
| --- | --- |
| Sue menu | A drop-down under the Sue button, built for what is open; submenus open on hover; `Esc` or a click outside closes it. |
| Command palette | `Tab` or `/` when no text field has focus (unless the page keeps the key): fuzzy search (in-order letters, bonuses for runs and word starts) over the Hub, every studio and tool, the pin, Sue's own actions, every template (`New Picture: Square Post`) and the applets' `commands`. |
| Settings window | Over a scrim; 190 px category sidebar, filter field, one page at a time (§4.3). |
| Quit dialog | When `general.confirm_on_exit` is on. |

A file dropped anywhere opens as above.

**Text and undo.** Every text surface is an `egui::TextEdit` or `sue-type`'s
editor. Text in the arena is edited in place through `ArenaText`, an egui
`TextBuffer` that records each insert and delete as a byte-offset delta.
`Typing` groups a burst (same entity, no pause of 900 ms) into one
`TextEdits` transaction and records it, already applied, in the global undo
tree. The shell consumes the riced undo and redo chords (`Ctrl+Z`,
`Ctrl+Shift+Z`, `Ctrl+Y`) before any widget sees them, closes the open burst,
and walks the global tree, so text fields never run egui's private undo.

The built-in applets are registered by the binary, like any package would
register its own:

| Applet | Role | Templates | What it does |
| --- | --- | --- | --- |
| Sue Paint | Studio | Picture: Illustration, Print A4, Full HD, Square Post, Sprite, Custom... | The painting studio (§3.7) |
| Sue Writer | Studio | Writing: Blank Document, Novel, Short Story, Screenplay and the other project templates (§3.8) | The writing and publishing studio (§3.8) |
| Image Viewer | Tool | n/a | One picture filling its page: fit, 100% and wheel zoom, panning, the project's next and previous pictures; simple edits (turn, mirror, crop, resize, brightness and contrast, saturation and hue, grayscale, invert), each one undo step on the same entity (`ReplaceComponent`); menus File (Open Picture, Save As in any picture format, Save Over, a lossless JPEG XL re-encode, Show in Folder, Back to the Hub), Image, View and Send (the Converter, Sue Paint and Sue Writer by `EntityId`), and a toolbar under them |
| Converter | Tool | n/a | Any file in (Add Files, Add a Folder, the project's pictures and sounds, or Send To from any studio or tool); a target per kind (pictures: PNG, JPEG, JPEG XL, WebP, TIFF, BMP, GIF, TGA, QOI, ICO, PPM; sounds: FLAC, WAV, AIFF; text: tidied, Markdown to HTML, CSV, TSV, JSON; anything: Base64, hex dump, byte map, copy); picture resizing with Lanczos, bicubic, bilinear or nearest in premultiplied alpha, turns, mirrors, brightness, contrast, saturation, grays, flattening; sound trimming, channels, Lanczos resampling, speed, reverse, gain or normalizing, fades, depth; text tidying; output folder, name pattern and keep or replace (never the source); files convert on a background thread; menus File and Convert (`Ctrl+Enter`) |
| Settings | Window | n/a | Preferences by category (§4.3) |

### 3.7 Sue Paint (`sue-paint`)

Sue Paint is the painting and illustration studio, a clean-room studio laid
out the way painting apps usually are. It fills the window with:

| Part | What it holds |
| --- | --- |
| Menus | File, Edit, Story, Animation, Layer, Select, View, Filter, Window, Help, in Sue's top bar after the Sue button (`SueApplet::bar_ui`); about 230 items, the unbuilt ones grayed out; File, Back to the Hub; hidden with `Shift+Tab` |
| Command bar | New, Open, Save (with a menu for Save As and the exports); Undo, Redo; Clear, Clear Outside, Fill, Deselect, Transform; view flip, rotate and fit; snapping; shortcuts, about, and search (the command palette) |
| Tool strip | Zoom, Hand, Operation, Move Layer, Selection, Auto Select, Eyedropper, Pen, Pencil, Brush, Airbrush, Decoration, Eraser, Blend, Fill, Gradient, Figure, Frame Border, Ruler, Text, Balloon, Correct Line; main and sub color chips |
| Sub Tool and Tool Property | The sub tools of the tool (G-Pen, Mapping Pen, Turnip Pen, Marker; Pencil, Mechanical, Rough; Watercolor, Opaque Watercolor, Gouache, Ink Brush; Soft, Strong, Fine airbrushes; Hard, Soft, Rough erasers; fill, gradient, figure, selection modes) and their settings, with a live stroke preview |
| Size | Vertical brush size and opacity sliders |
| Color Wheel, Color Slider | A hue ring around a saturation and value square; RGB and HSV sliders and hex |
| Color Set, Color History | Color sets (Default, Grays, Skin Tones, Pastels, and the user's) in a drop-down, kept in the data folder (`color_sets.txt`), with a settings menu (new, duplicate, delete, swatch size, restore), GIMP palette and hex import, GIMP palette export, and add, replace and delete; the colors painted with, newest first |
| Canvas | Tabs of open pictures; the view zooms (1% to 6400%), scrolls, turns and mirrors; a status strip with zoom, rotation, flip, size, resolution and the pointer's pixel |
| Navigator, Sub View | The whole picture with the visible area outlined (drag to scroll), zoom and rotation sliders, turns, flips; reference pictures (import through the system's open dialog, previous, next, list, eyedropper, zoom, turn, fit, delete) |
| Layer, Layer Property, History | Blend mode (26), opacity, clip, draft, lock, lock transparent pixels; new, duplicate, merge down, move, delete; the stack with visibility, thumbnails, rename by double-click; the current layer's settings; the undo list |
| Timeline, Animation Cels | The timeline picker, new timeline, animation folder and cel, specify and delete cels, keyframes, onion skin, light table and playback, grayed out until animation arrives; the timeline's top edge resizes it |

**Palettes and docking** (`dock.rs`). The palettes (Tool, Sub Tool, Brush
Size, Color Wheel, Color Set, Navigator, Layer) sit in columns at the left
and right, each column's height shared by weight. A column's inner edge
drags its width (170 to 560 points; the tool strip and brush size keep
theirs), the line between two palettes their shares. A palette's menu docks
it on either side, floats it in an `egui::Window`, collapses it to an icon
in the strip at the far right (shown beside the strip on a click), or
closes it. When the columns, the strip and 300 points of canvas do not fit,
the widest columns (never the tools) fold into the strip for that frame.
Tabs show a dot when a picture's composite changed since it was opened or
saved.

**Shortcuts.** Clip Studio Paint's defaults: the File, Edit, Layer,
Selection, View, Window and Help chords; the tool letters; comma and period
for sub tools; `C` for the transparent color; and on the canvas Shift+Space
(turn), Ctrl+Space and Alt+Space (click-zoom), Ctrl+Shift+click (pick a
layer), Ctrl+Alt+drag (brush size), Shift+drag (straight line), right-click
(eyedropper) and Shift+wheel (turn). Chords for features not built yet are
bound and report that. The studio reserves `Tab`, `Shift+Tab`, `/` and
`Ctrl+0` from the shell (`SueApplet::reserved_keys`).

**Data.** A picture is the shared layered picture of §1.13, made by the New
dialog (presets as backbone `Preset`s, paper sizes, px, cm, mm and inches,
resolution, paper color) or by opening a PNG, JPEG or JPEG XL through the
format registry. Every open tab is a picture in the arena: tabs appear for
pictures made, opened, loaded with a project or brought back by redo, and
closing a tab leaves the picture in the project. The studio keeps only views,
screen images, the current layer and selections. Other studios and tools reach
the same entity by `EntityId`: Send To Sue Paint opens a layered picture as it
is, and any other picture (from the Image Viewer, say) becomes a layered
picture whose layer is that very entity.

**Painting.** The brush engine (`sue_paint::brush`) is built so lines come out
clean without the brush feeling slow. A stroke passes through these stages:

| Stage | What it does |
| --- | --- |
| Input | egui's raw event stream, in order, so a fast stroke keeps every sample between frames; each frame's samples are spread over its time, so speeds are real. Pen pressure comes from egui touch events with force, through the pen pressure curve every tool shares (`sue_paint.pen_pressure`, a `Response`). |
| Dynamics | Brush Size and Opacity each have a `Dynamics`, as in Clip Studio Paint: pen pressure, velocity (the pen's speed, still to 2,000 points per second) and random, each on or off with a minimum value and, for pressure and velocity, its own curve; the inputs multiply. Random is steady along the path, so the stroke painted again from its corrected path keeps its sizes. *Specify by size on screen* keeps the brush the same size at any zoom; *At least 1 pixel* keeps a light line from thinning below a pixel. |
| Stabilization | A time-based filter (lag of about 4 ms per level, the same at any sample rate) that the brush follows. *Adjust by speed* loosens it as the pen speeds up (from 150 to 1500 points per second the lag drops to a tenth), so slow lines lose their tremor while quick hatching stays snappy. On lift the brush catches up with the pen. |
| Painting live | Pen-like tips paint a continuous tapered capsule between consecutive samples: each pixel's coverage comes from its exact distance to the segment (four anti-aliasing widths, hardness falloff), so there are no gaps, ridges or beads at any speed, and lines thinner than a pixel stay unbroken as lighter ink. Airbrushes and mixing brushes stamp dabs at a tight spacing instead, because their dabs add up. The start taper grows the line from a point. |
| Finishing on lift | The hooks a pen leaves when it touches down and lifts are trimmed; the samples become a centripetal Catmull-Rom spline (no loops or overshoot), so even a fast mouse that reported a few points ends as a curve; *post correction* (0 to 10) resamples the path evenly, finds deliberate corners (a turn of more than about 65 degrees over arms a brush wide, on a lightly smoothed copy) and smooths everything between them with a gaussian along the path that grows with the level and the brush; a quick *flick* (over 700 points per second) carries the line on along the pen's momentum. The stroke is then painted again from the finished path with its end tapered, the flick thinning to a point. |
| Color mixing | Mixing brushes pick up the color under them and are reloaded with their own paint, both per distance travelled, so a stroke settles at the mix amount of the canvas color whatever the spacing. |
| Starting and ending | Measured as Clip Studio Paint's How to specify offers: in pixels, as percentages of the whole stroke (exact once the pen lifts and the stroke is painted again), or Fade (gone by a length however long the stroke); they thin the brush, fade the paint or both, and with *by speed* quicker strokes get longer ones. |
| Color jitter | Per dab for brushes that stamp (hue, saturation, luminosity, toward the sub color) and per stroke for any brush, from steady chances along the path, so a repainted stroke keeps its colors. |

A stroke never stacks on itself: each touched 64-pixel tile keeps its pixels
from before and a coverage mask, and every pixel is recomputed from the two, so
a 50% pen stays at 50% where it crosses itself, and painting the finished path
again starts from untouched pixels. On release the tiles become one
`RasterEdit`. Each pen and brush sub tool ships with its own stabilization,
correction, tapers, flick and mixing, all editable in Tool Property; an ignored
test (`stroke_sheet`) paints a sheet of simulated strokes for tuning by eye.

**Transform.** Scale/Rotate (Ctrl+T) and Free Transform (Ctrl+Shift+T)
lift the selected part of the current layer, or all of it
(`sue_paint::transform::lift`), and draw it onto a quad with one projective
mapping (a homography from the piece's rectangle, worked backwards with
premultiplied bilinear sampling). Scale/Rotate keeps the quad a turned,
scaled rectangle (center, scale, angle; corners keep the aspect unless
Shift, edges stretch, inside moves, outside turns); Free Transform moves
each corner. OK records one `RasterEdit` and moves the selection along.

**Layer folders.** A folder is a `sue.folder` entity in the picture's stack,
right above its layers, and each layer in it is linked by a
`sue.paint.member` relation, so layers stay where every edit already finds
them. The compositor passes folders through, folding in their visibility
and opacity (Clip Studio Paint's Through); the palette indents members and
hides those of closed folders; moving a layer past its folder's top or
bottom takes it out, onto a folder puts it in, and a folder moves with its
layers.

**Layer masks.** A mask is an RGBA raster entity (`sue.paint.layer_mask`)
linked from its layer by `sue.paint.mask`; its alpha multiplies the layer's
in the compositor, and the picture's signature includes it, so its edits
recomposite. Strokes go to the mask while it is being edited: the pen
shows, the eraser hides, one `RasterEdit` each.

**Canvas.** Change Canvas Size moves every layer by the anchor and cuts or
widens it to the new canvas, Crop is the same to the selection's bounds,
and Change Image Resolution resamples every layer (averaged when shrinking,
bilinear when growing, premultiplied); frame panels and balloons move and
scale with them. Each is one `Batch` of `ReplaceComponent`s and properties.

**Symmetry.** The symmetrical ruler (the Ruler tool) has a center, an angle,
a number of lines (counting the mirrored ones, as Clip Studio Paint does)
and Line symmetry; a stroke paints once through each of its copies
(`brush::symmetry`, affine maps) into the one stroke's coverage, so copies
never stack and the stroke stays one undo step.

**Vector layers.** A vector layer is a raster layer of class
`sue.paint.vector` (its pixels composite like any layer's) whose `Geometry`
keeps its lines (`sue_paint::vector`): a polyline a line over the pen's
samples as drawn, with pressure and speed per point, and per primitive the
brush (`Tip::to_text`, `key=value` pairs that skip unknown keys), the start
and sub colors, the finish (post correction and flick) and the symmetry
copies. A stroke on one is painted live as usual and kept as a line when the
pen lifts, both in one undo step: the `RasterEdit` and a `SetExtension`
holding the geometry from before, recorded after the new geometry is put in
place. Painting a line again from its samples gives the same pixels, since
strokes take their chances from their own path. The eraser on a vector
layer takes lines away as its Vector setting says (Erase touched area cuts
the line where the eraser meets it, Erase up to intersection cuts it at the
nearest crossings with other lines either side, Whole line takes it all),
paints the layer again from the lines left, and records one step on
release. Raster-only tools (fill, gradient, blend, filters, transform) say
so on a vector layer. The Operation tool's Object picks vector lines (Shift
adds or takes away), drags them and deletes them, each one step; Select
layer picks the layer under the pointer.

**Correct Line.** Every brush sample carries a width and an opacity share
(`Sample::width`, `Sample::alpha`, 1 as drawn) that scale its dab, so a
vector line can be made wider, narrower or fainter point by point, and keeps
them in its geometry with flags marking control points and corners
(`sue_paint::correct`). A line without flags finds its control points by
simplifying its path (Douglas and Peucker). Control point moves one (the
path to the controls either side follows smoothly), adds, deletes (the
neighbors are joined by a Catmull-Rom curve), switches corners (straight or
curved to the neighbors), widens and fades at one, and splits a line; Pinch
vector line pulls a stretch of a line within its effect range, sharper by
level, with its ends fixed or free; Simplify vector line smooths under the
brush and can delete short lines; Connect vector lines joins the nearest
ends; Adjust line width thickens, narrows, scales or fixes the width under
the brush or for the whole line; Redraw vector line replaces the traced
stretch with the trace; Redraw vector line width gives it the pen's
pressure. Each drag paints the layer again from its lines and records one
step. Remove dust clears specks no larger than the dust size from a raster
layer, in a dragged box or all of it.

**Selections.** Beside rectangles, ellipses and lassos, the Selection tool
has Polyline (clicked corners, closed on the first or with Enter), Selection
pen and Erase selection (soft discs painted into or out of the mask along
the drag). Expand and Shrink Selected Area grow the mask (or what is not
selected, counting past the canvas edge as unselected) by an exact distance
transform; Blur Border runs three box blurs. Select Color Gamut selects the
pixels near the drawing color, of the layer or of the picture as shown.
Convert to Selection Layer keeps the selection as a green draft layer, and
Add Selection and Remove Selection take a layer's alpha back into it. Quick
Mask turns the selection into a red draft layer to paint on, and that layer
back into the selection. The Selection Launcher, under any selection, holds
Deselect, Invert, Expand, Shrink, Clear, Clear Outside, Fill, Scale/Rotate
and Crop.

**Timeline audio.** File, Import, Audio decodes a sound file into an audio
entity (`sue.audio`) and puts it on a track of kind `sue.paint.audio` in the
picture's shared `Sequence`, a clip from the frame shown. The timeline
draws each audio track as a bar to drag (one step on release) with Mute and
Delete Audio Track in its menu. When the animation plays, every unmuted
audio clip is mixed into one stereo `Shared` buffer at 48 kHz (mixed again
only when the clips change) and the shared Transport plays it from the
frame shown, again at each loop, and stops with playback.

**3D layers.** A 3D layer (`sue.paint.layer3d`, Layer, New Layer, 3D Layer
with a cube, sphere, cylinder, cone or plane, or File, Import, 3D Data for an
OBJ model through `sue_io::ObjFormat`) links by `sue.ref` to a model entity
whose shape is the shared `Geometry`, and keeps its camera (angle, height,
distance, field of view, pan), light, look (shaded, line art or both),
line width and fold angle as properties (`sue_paint::three`). Its pixels are
the model fitted to a box 2 wide and drawn by a small software renderer:
triangles projected with perspective into a depth buffer (depth
interpolated as its inverse), flat lighting, and lines where the outline
is, where the depth jumps, or where neighboring normals differ by more than
the fold angle, widened by a distance transform. Every view change redraws
the layer and is one step (properties with `ReplaceComponent`); the Object
tool turns the camera by dragging (Shift pans, Ctrl zooms) and Tool
Property sets each value, shown live and kept when let go. Convert to Lines
and Tones on a 3D layer takes the lines from the model and its shading as
tone (Clip Studio Paint's LT conversion of 3D). Pens refuse 3D layers.

**Webtoons and timelines.** New has a Webtoon canvas (800 by 12800
pixels); Export Webtoon scales the flattened picture to an output width and
cuts it every split height into numbered pictures, top to bottom. Edit
Timeline changes the frame rate and length, keeping every key on its frame
(`anim::retimed`) and dropping keys past the end, as one step; Enable
Timeline turns the frame view off and on; First and Last Frame jump; Export
Animation Cels writes each cel as a picture named for it.

**Line extraction and books.** Filter, Line Extraction turns a photo or scan
into black line art on clear: where the picture is darker than its
surroundings (a fine gaussian against a coarser one), stronger the higher
the sensitivity, with areas darker than the black level filled. Convert to
Lines and Tones puts that on a Lines layer over a Tones layer (the shading
as ink by darkness, with the Tone effect on) and hides the layer, as one
step. Export as PDF and Export as EPUB write every page of a story, or the
picture alone, through `sue_io::publish::PictureBook`: a PDF page a
picture at its size on paper with a bookmark each, or a fixed-layout EPUB 3
(`rendition:layout` pre-paginated, a JPEG and an XHTML page each, the spine
right to left for manga).

**Colors and layers.** The color palette has Clip Studio Paint's
Intermediate Color (a grid blending four corner colors, each set to the
drawing color by a click, in 2 to 24 steps) and Approximate Color (a 9 by 9
grid round the drawing color, changing hue, saturation or brightness
across and down, over a range). Set as Reference Layer marks a layer
(`layer.reference`) that the fill and auto select can refer to alone (the
Refer row's Reference layer, rendered with every other layer hidden).
Rasterize swaps a vector layer for a raster layer in its place with its
pixels, name, properties and folder, as one step.

**Layer effects and right-click menus.** Layer Property has Clip Studio
Paint's effects, each a layer property the compositor applies as it draws,
leaving the pixels alone (`sue_paint::effects`): Border effect (a rim of
its thickness and color round where the layer is at least half opaque,
from an exact Euclidean distance transform over the rectangle being drawn,
widened by the rim so a part sees the layer outside it), Tone (halftone dots
on a turned grid, as large as the gray is dark, at the screen frequency over
the picture's resolution), Layer color, and Expression color (gray, or
monochrome at a threshold). Right-clicking a layer picks it and opens the
Layer menu (drawn by the shared `sue_wm::context_items`, grayed and
checked as the menu bar is); sub tools offer Duplicate, Delete and Reset to
Initial Settings; canvas tabs Save, Save As and Close; materials Paste and
Delete; swatches Add, Replace and Delete. A right-click on the canvas picks
a color, as in Clip Studio Paint.

**Materials.** The Material palette (a tab beside Color Set and Color
History, or Window, Material) lists Sue Paint's own screentones (dots on a
45 degree grid of 8 pixel cells sized to their share, lines, sand) and
patterns (crosshatch, grid, checker, bricks, stripes, speckles), drawn at
any size from a function a pixel, and the pictures registered with Edit,
Register Material, Image: the current layer cut to what it covers, kept as
a PNG in Sue Paint's folder (`materials/`) so every project has it
(`sue_paint::materials`). Double-clicking one, or Paste to Canvas, puts it on
a new layer above the current one: tones and patterns cover the canvas in
the drawing color, pictures sit in the middle, shrunk to fit.

**Camera folders.** A camera folder (Animation, New Animation Layer, Camera
Folder) is a layer folder flagged `folder.camera` whose camera keyframes are
the shared `Animation` on it, a curve each for `camera.x`, `camera.y` (the
middle of its view), `camera.scale` and `camera.angle`, keyed linearly
(`sue_paint::camera`). At each frame the frame view carries every camera's
affine map from the canvas to what it sees, and the compositor samples the
folder's layers through it (bilinear, premultiplied; masks to the nearest
pixel), on screen and in every animation export. With the Object tool on a
camera folder, the layers show as they are with the camera's frame over
them: dragging inside moves it, a corner zooms, outside turns it, and Tool
Property sets position, scale and rotation and adds, updates or deletes the
keyframe at the frame shown.

**Timelapse.** A picture that records (`timelapse.record`, set from New or
File, Timelapse) keeps a frame each time its composite settles after a
change, between strokes: a JPEG at most 1280 pixels on its longest side in
Sue Paint's folder (`timelapse/<picture id>/`), so recording adds nothing to
the undo history or the project (`sue_paint::timelapse`). Export Timelapse
lays the frames over white in one size (the last frame's shape at the long
side chosen), every frame at 30 a second or as many a second as fit the
length (skipping frames past 60 a second), holds the last frame a second,
and writes a Motion JPEG AVI (`sue_io::MjpegAvi`, every frame a JPEG and a
key frame, with an index) or an animated GIF. Export Animation, Movie writes
the timeline the same way.

**Rulers.** A picture keeps its rulers as text properties
(`sue_paint::rulers`): `ruler.linear` (two ends), `ruler.special` (parallel
lines at an angle, radial lines or concentric circles about a center),
`ruler.perspective` (one to three vanishing points, the first two on the eye
level), `ruler.symmetry` and `ruler.paths` (curve, figure and pen rulers: open
or closed paths sampled every 2 pixels), so each is undone, saved and opened
with the picture. The Ruler tool has Clip Studio Paint's sub tools: Linear
Ruler, Curve Ruler (clicked points joined by a Catmull-Rom curve, laid with
Enter or a click on the last point), Figure Ruler (a rectangle or ellipse),
Ruler Pen (drawn freehand), Special Ruler, Perspective Ruler (Add vanishing point: two dragged lines
cross at the new point; Delete vanishing point; Fix eye level) and
Symmetrical Ruler; dragging a vanishing point, the eye level or a linear
ruler's end moves it, shown live and recorded as one `SetProperty` on
release. Create Perspective Ruler lays one, two or three points as Clip
Studio Paint does (the middle, the left and right edges at mid height, and
one far below). A stroke follows them through `rulers::Snap`: one near the
linear ruler or a path ruler goes onto the nearest at once (Snap to Ruler); otherwise the guides
through its first point (toward each vanishing point, level and upright
for one point, upright for two, the special ruler's line or circle) wait
until the pen has moved 4 screen pixels, the one running most like the pen
is chosen, and every sample, stabilized point and catch-up point after is
projected onto it, with post correction and flicks off so the line stays
exact (Snap to Special Ruler).

**Text.** A text layer (`sue.paint.text`) keeps its words, font, size, ink,
placement, centering and vertical setting and draws them again after every
edit with `sue_type::raster::draw_text`, which lays lines across or columns
down and right to left and fills glyph outlines (read with ttf-parser from
the shared font library) with an accumulation rasterizer after font-rs.
Clicked inside a balloon, the Text tool centers the words in it.

**Tonal corrections.** Tone Curve (a `Response` for RGB and each channel, run
through a table per channel), Level Correction, Color Balance (shadows,
midtones and highlights weighted by brightness) and Gradient Map join the
filters, each previewed in place and recorded as one step.

**Correction layers.** A correction layer (`sue.paint.correction`) keeps a
correction as text (`Filter::to_text`, in `correction.filter`) and changes
everything composited under it without touching those pixels: the
compositor runs corrections that treat channels apart through a table of
256 values a channel, others pixel by pixel, and mixes the result in by the
layer's opacity. Its dialog writes the setting in place while it is open;
OK records it as one `SetProperty`, Cancel puts it back (and undoes a layer
just made).

**Blend.** The Blend tool reworks the pixels already on the layer
(`sue_paint::blend`): Blur averages around each pixel, Finger Tip drags color
along the stroke, and Liquify moves the picture (Push along the stroke,
Expand, Pinch and Twirl about the brush's middle). Dabs land every fifth of
the brush, each reading a copy of its patch from before it, weighted by a
smooth falloff and the strength; the drag is one `RasterEdit`.

**Puppet warp.** Edit, Transform, Puppet Warp puts pins on the current
layer: dragging one bends the layer around the others by rigid moving least
squares (`sue_paint::warp`, after Schaefer, McPhail and Warren, 2006), so
shapes turn and move without smearing; the preview is worked out on an 8
pixel grid and filled in between, OK bends it on a 2 pixel grid as one
`RasterEdit`, Cancel puts the layer back.

**Stories.** A comic or manga is an entity of class `sue.paint.story`
(`sue_paint::story`) whose pages are ordinary pictures, its ordered
`sue.child` relations, with its binding (`story.binding`, right for manga).
Pages are added, inserted, duplicated with their layers, deleted with their
layers and moved, each one undo step; the Page Manager lays them out as
spreads facing each other by binding, and Export All Pages writes them
numbered in any picture type.

**Manga.** A frame border layer (class `sue.paint.frame`) keeps its panels,
convex polygons, in `frame.panels` with its border and gutter, and draws
them clear inside, inked at the edge and paper-colored in the gutters, so the
layers under it show only inside the panels; layouts make the panels, the
Frame Border tool adds rectangles and divides a panel along a dragged line
by clipping it with two half-planes a gutter apart. A balloon layer
(`sue.paint.balloon`) keeps ellipses and tails in `balloon.shapes` and draws
their union white with one outline. Each edit is one step that holds the
shapes and the drawing together (`SetProperty` with `ReplaceComponent`).
Focus lines, speed lines and screentone are filters; the lines are inked with
the brush engine itself as pressure-tapered pen strokes.

**Animation.** A picture's timeline is the shared `Sequence` on the picture
(`sue_paint::anim`): its rate is the frame rate, each track of kind
`sue.paint.cels` is an animation folder, and each clip is a key that shows
one cel, a layer of the picture by id, until the next key (a blank key shows
nothing); `anim.length` holds the length in frames. At a frame the
compositor takes a `FrameView` that hides the cels no folder shows and draws
the cels before and after tinted blue and green at a third of their opacity
(onion skin). Onion Skin Settings (`anim::Onion`, kept on the picture as
`anim.onion`) choose how many cels show before and after, Clip Studio
Paint's color modes (Color, Half color, Monochrome: how far each cel's color
gives way to its skin color) and an opacity that starts at a level and drops
a step per cel. The light table is a `sue.paint.light_table` relation from a
cel to each layer registered to it; while that cel is drawn the frame view
shows them in their own colors at the light table's opacity. Playback
advances by the frame rate, and moving to a frame makes its cel the current
layer. Animations export as an animated GIF
(`sue_io::encode_gif_animation`) or a numbered picture sequence.

**Other edits.** Fill floods the current layer or all layers within a
tolerance and grows one pixel under anti-aliased lines; Close gap floods as
if the lines were a few pixels thicker, so small breaks hold the fill in,
then grows back out to the lines (`fill::flood_closing`), and Area scaling
spreads it further under them; gradients run linear or
radial, drawing color to transparent or main to sub; figures draw lines,
outlines and filled shapes; selections are coverage masks (rectangle, ellipse,
lasso, auto select; new, add, subtract, intersect) that every edit stays inside,
drawn as an outline. Edit and Filter commands (clear, fill, flips, recolor,
brightness to opacity, reverse gradient, brightness and contrast, hue,
saturation and luminosity, posterization, binarization, blurs, sharpen, mosaic,
noise) rewrite a copy of the layer and record the smallest changed rectangle;
dialogs preview in place and undo nothing until OK. Layer changes use the
generic edits, named the way people say them ("Hide Layer", "Move Layer").

**Compositing.** Layers composite bottom to top with `BlendMode::composite`,
clipping to the nearest unclipped layer below, over a checkerboard where the
picture is clear. The screen image recomposites only the painted rectangle
while a stroke is drawn and everything when the arena changes behind it (undo,
a property, a new layer), detected by the layers' revisions; large areas are
split into bands of rows on every core. Exports (PNG, JPEG over white, JPEG XL)
flatten the paper and leave draft layers out.

**Every menu item works** (a test fails if one is left grayed). The last ones
built on shared pieces: `sue-io` gained a layered Photoshop codec (`psd`: PSD
and PSB, PackBits, folders as section dividers, blend keys, opacity,
visibility, clipping, Unicode names), an SVG shape reader (`svg`: every path
command, the basic shapes, transforms and the view box) and `decode_frames`
(animated GIF, PNG and WebP, Motion JPEG AVI), each with round trip or read
back tests. Sue Paint maps them onto layers (`psd_io`), vector lines
(`vector::from_svg`) and animation cels (`anim::import_frames`). Convert Layer
traces raster pixels to vector center lines (`vector::trace`: Zhang and Suen
thinning, chains walked between ends and joins, widths from a chamfer distance,
Ramer, Douglas and Peucker simplification). Brush tips can be pictures
(`brush::TipShape`, registered by name so a vector line keeps its tip).
Rotating the canvas a quarter turn moves rasters, masks and vector lines
together in one edit; Merge Selected Layers composites the picked layers with
their blend modes into the lowest. Story gained Combine and Separate Pages
(re-parenting layers between page pictures), page settings, story
information, the Story Editor and batch text import; View gained page frames,
a ruler bar, snap to grid, the tone area and a CMYK soft proof (`proof.rs`:
separation with an ink limit, dot gain and inks laid over the paper's color).
Print lays pages on paper as a PDF (`print.rs`) handed to the system viewer
through `sue_platform::open_document`. Auto Actions, the Item Bank and
registered workspaces are plain text files in Sue Paint's data folder.

---

### 3.8 Sue Writer (`sue-writer`) and typography (`sue-type`)

Sue Writer is the writing and publishing studio: a word processor's pages
and a long-form writing studio's binder in one. It owns little itself; it is
assembled from features Sue owns, so the next studio that sets text (notes,
slides, page layout, a font studio) uses the same ones.

**Rich text (`sue-model::text`).** A document is a `StringBuffer` with
`TextMarks`. Paragraphs end with a newline, and that newline carries the
paragraph's own marks (`style`, `heading`, `list`, `level`, `align`, indents,
spacing, `page_break`), as a word processor's paragraph mark does: a
paragraph mark covers its whole paragraph, newline included, so an empty
paragraph keeps its style, and a rich text ends with a newline.
`TextMarks::insert_text` and `delete_text` keep them right through typing,
pasting (pasted text brings its own marks, `source`) and joins (the joined
paragraph keeps the first's marks, unless the deletion took that one from its
very start). Character runs are read once for every view and format
(`runs`, `CharFormat`, `marks_by_paragraph`). New kinds: `align`, `indent`,
`indent_right`, `first_indent`, `line_spacing`, `space_before`, `space_after`,
`page_break`, `level`, `highlight`, `script`, `small_caps`, `footnote` (the
note's text, on the words it follows) and `embed` (an entity shown on an
object replacement character).

**Tables in the text (`sue-model::table`, `sue-type::tables`).** A word
processor's table is an entity of class `sue.text_table` holding the shared
`Table`, shown where an `embed` mark names it, on an object character in a
paragraph of its own (as a picture is). `Table::grid` makes every cell an
empty text so a new table's size is its shape; `sue.table.header_rows`
(`Int`) and `sue.table.widths` (shares of the line) are properties of the
entity. `Reshape` inserts and removes rows and columns as one undo step
with its exact inverse (removed cells and column definitions come back).
`sue-type::tables` lays a table out as one block whose rows are the block's
rows, so page view breaks it between rows, and repeats the heading rows at
the top of each later page; the editor paints it and hit-tests cells, and a
caret in a cell (`CellCaret`) types there in place, recorded with
`Typing::hold` so a burst in one cell is one undo step that the shell's
`Ctrl+Z` sees like any other typing. `Tab` moves on and adds a row in the
last cell. A copied document, a snapshot and a restored snapshot get copies
of their tables, so they never share one. The typesetter sets tables for
print with ruled cells and repeated heading rows; Word (`w:tbl`, a repeated
`w:tblHeader`), OpenDocument (`table:table` with header rows and column
widths), HTML and EPUB (`thead` and `tbody`), RTF (`\trowd` rows, `\trhdr`),
Markdown (pipe tables) and plain text (tab-separated rows) write them.

**Tables of contents (`sue-model::contents`, `sue-type::contents`).** A
table of contents is an entity of class `sue.text_contents` shown by an
`embed` mark; it keeps only its settings (`sue.contents.title`,
`sue.contents.levels`), and its entries are the headings of the text it is
in, made at every layout, so it never goes stale. The editor lays it out as
a read-only `TableBlock` with a `Listing` (each entry set in by level, its
page at the right, a dotted leader): once to paginate, then again with each
heading's page; a press on an entry moves the caret to its heading. The
typesetter sets the book twice when it holds contents (`Embeds`), the second
time with the pages the first found. Word gets a `TOC` field over the
headings with `w:updateFields`, so Word fills in the pages on opening;
OpenDocument a `text:table-of-content` over the outline levels; HTML and
EPUB a `nav` list; RTF, Markdown and plain text the headings, set in.

**Running headers and footers (`sue-model::layout`).** `Running` holds a
header's or footer's left, middle and right parts, stored as one text parted
by tabs; each part is text with fields in braces (`FIELDS`: `{page}`,
`{pages}`, `{title}`, `{author}`, `{date}`). `fill` sets them for one page
(`PageFields`), and `running_pieces` hands formats the fields to write as
their own live ones. `sue-type`'s `PageSetup` carries a header, a footer
(the page number in the middle by default), `different_first` and the
document fields; `PageSetup::furniture` gives each page what its margins
show, which the editor paints and the typesetter sets. A `Book` carries the
same: Word writes a header and a footer part with `PAGE` and `NUMPAGES`
fields at a centered and a right tab stop (`w:titlePg` for a title page),
OpenDocument a master page's `style:header` and `style:footer` with
`text:page-number` and `text:page-count` (and `style:header-first`), RTF
`\header` and `\footer` groups with `PAGE` and `NUMPAGES` fields
(`\titlepg`). Sue Writer keeps them on the project
(`sue_writer.header`, `sue_writer.footer`, `sue_writer.different_first`).

**Manuscripts (`sue-model::manuscript`).** A long work is an entity of class
`sue.manuscript` whose ordered `sue.child` relations make a binder of folders
and documents. Its top folders have a `manuscript.role`: `draft` (what is
published), `research` (any entity: pictures, sounds, web pages) and
`trash`. Documents carry `doc.synopsis`, `doc.notes`, `doc.status`,
`doc.label`, `doc.include`, `doc.target_words` and `doc.keywords` as shared
properties. `walk`, `publishable` and `total_words` read it.

**Fonts (`sue-wm::fonts`).** A background scan of the folders the OS keeps
fonts in (`sue_platform::system_font_dirs`) reads only each file's name, OS/2
and post tables and groups the faces into families with their weights and
italics. `fonts::resolve` installs a face into egui the first time it is
drawn (answering with the bundled family until egui has it) and says when
bold or italic must be faked. `fonts::face_bytes` hands exporters the file.
Sue's bundled fonts are always there as `Sue Sans` and `Sue Mono`.

**Styles (`sue-type::style`).** A `StyleSheet` holds named paragraph styles
(font, size, weight, alignment, indents, spacing, line spacing, page break,
keep with next, outline level, the style Enter goes on with, and the one Tab
turns it into). Three ship: Standard (body text, a title, four headings,
quotes, code, scene breaks, captions), Manuscript (double spaced, indented
paragraphs, chapters on a new page) and Screenplay (scene headings, action,
characters, parentheticals, dialogue and transitions at their standard
indents). `paragraph_look` resolves a paragraph's style and its own marks;
every view and exporter reads that.

**Layout and the editor (`sue-type::layout`, `editor`).** Each paragraph is
one egui galley, shaped with the fonts it asks for, with lists numbered
across their run. In page view the galleys are broken across pages between
their rows, headings kept with the line after them and no lone first line at
a page's foot; in draft view they stack in one column. The `Editor` shows one
document or several in a row and draws its own caret, selection, lists and
pictures; it takes keys (words, rows, pages, `Home`, `End`), the mouse (one,
two and three clicks, dragging past the edge scrolls), input methods and the
clipboard, keeping formatting between Sue's own documents. Edits go through
`sue-type::edit`: typing edits the `StringBuffer` in place and moves the
marks, and `Typing::record_with` makes each burst one undo step for both;
formatting is a `SetExtension` transaction. The editor registers as a text
field, so the shell's keys (the command palette) leave typing alone.

**The caret (`sue-type::caret`).** The editor's caret glides to its new place
instead of jumping, the approach of WordCraft (MIT or Apache 2.0), which
measured it from Word for Windows. Each frame the editor hands `CaretGlide` the
caret's bar in content space (points from the scrolled content's top left, so
scrolling does not bend the path), a fingerprint of what put it there (the
caret and its document's revision) and one of the view (documents, zoom, page,
width). When the first changes, the bar's top and bottom ease from where the
caret was heading to the new place in `motion.caret_glide_ms` (100 ms) along
Word's curve, `cubic-bezier(0, 0, 0, 1)`, whose closed form is
`3s^(2/3) - 2s`: the same time for a character, a line or the next page.
Another move before it lands sets off from the end of the glide under way, so
fast typing and held keys keep the caret at most one move behind the text and
never send it back. Typing, Backspace and Delete, the arrows, Home and End, a
click, text wrapping to a new line or page and a heading's taller caret all
glide; a view change, a hidden caret, a pointer dragging a selection, a start
off screen or a jump further than the view is tall (Ctrl+End) put it straight
there. A gliding caret stays lit; at rest it blinks at egui's text cursor
timing (`motion.caret_blink_ms`), lit first after each move, and after 10 s
without one it stays lit. `motion.smooth_caret` off, or a `HardwareProfile`
without spring physics (`LegacyLowSpec`), makes it jump. The motion is pure
and tested apart from egui (curve, landing exactly, retargeting, the blink, no
frames at rest); `bins/sue` types in Sue Writer and checks egui's repaint
delay: zero while it glides, the blink at rest, nothing after.

**Find and case (`sue-type::find`, `case`).** `Finder` searches plain
text (case folded unless asked, whole words) or a regular expression (the
`regex` crate; case and whole words apply to it too, no empty matches), and
fills a regular expression's replacement with what its groups caught (`$1`,
`${name}`). `case::change` turns text into upper, lower, sentence or title
case or swaps it; `Editor::change_case` does it to the selection or the
word at the caret as one undo step, keeping the marks.

**Typesetting and PDF (`sue-type::typeset`, `sue-io::pdf`).** For print, the
same styles and marks are set from the font files themselves (`ttf-parser`
widths): lines broken at spaces, justified, paginated with the same rules,
lists, pictures, links, footnotes gathered as notes at the end, page numbers
and a running head. `sue-io::pdf` writes it: fonts embedded whole as
CID-keyed fonts (a collection's face pulled out on its own), text that copies
out as Unicode, pictures, links and bookmarks, every stream deflated.

**Publishing formats (`sue-io::publish`).** A `Book` (sections of rich text
with the paragraph styles they use, pictures as PNG, the page) is written as
Word (Word's own style names, so its navigation and contents work; lists,
footnotes, links, pictures, page numbers), OpenDocument, EPUB 3 (a file per
section that starts a page, a navigation document and an NCX), HTML, RTF,
Markdown and plain text; Word, HTML and Markdown read back into rich text.
The registry offers Word (both ways), OpenDocument, EPUB and RTF for any
text entity (`PublishFormat`). `zip` writes and reads the archives.

**The studio.**

| Part | What it holds |
| --- | --- |
| Menus | File, Edit, View, Insert, Format, Table, Project, Help, in Sue's top bar after the Sue button; Table as LibreOffice Writer's (Insert Table, rows and columns, delete, heading row, convert text and tables); File, Back to the Hub. Each keeps to a dozen entries at most (a test checks): Format groups like LibreOffice Writer's (Text, Text Color, Highlight, Align Text, Line Spacing, Lists, Clear Direct Formatting), View puts the zooms in a submenu |
| Toolbar | what Word's mini toolbar and LibreOffice Writer's single toolbar (26.8) keep at hand: undo, redo, paragraph style, font (every installed family, searchable), size, bold, italic, underline, text color, highlight, alignment, lists, indents; Find at the right; it wraps onto a second row in a narrow window, and a tablet's buttons are 32 points |
| Binder | the project's folders and documents: click to open (a folder shows its documents one after another), double-click to rename, drag to move (above, below or into a folder), a right-click menu (new, rename, duplicate, include in compile, snapshot, merge, label, status, trash, Open in Sue Paint for pictures) |
| Text | the editor on pages that fit the window (until you zoom) or one column |
| Corkboard | index cards of a folder's items with their synopsis, label color, status and words; drag to reorder |
| Outliner | a table of everything under a folder: title, synopsis, label, status, words, included |
| Inspector | hidden until View, Inspector: synopsis, label, status, include, word target with progress, keywords, statistics, notes, snapshots (take, restore) and the outline of the headings |
| Status bar | words in the document (or the selection), the draft against its target, today's words against the daily goal, the page; Draft and Page and the zoom at the right, where Word and LibreOffice keep them |

**On a phone** (`FormFactor::Mobile`) the same studio lays out for a thumb
and an on-screen keyboard:

| Part | On a phone |
| --- | --- |
| Top bar | the Sue button, a menu button (every menu but Help as a submenu, `MenuBar::dropdown`), undo, redo; Find and the binder at the right; 44 point buttons |
| Text | reflowed to the screen's width at 100% with 14 point margins (`EditorView::gap`), as Word's mobile view does; View, Page View shows pages, fitted down to 30% |
| Formatting | the toolbar's controls as 44 point buttons in one row at the bottom (alignment as one button that steps through them), scrolling sideways; it sits above the status bar and so right above the keyboard |
| Binder, inspector | each covers the page when asked for; picking a document closes the binder |
| Status bar | the words and the page |

Two rules keep the line being written in sight. The host lays the window
out above the keyboard and redraws when the keyboard comes up, goes away or
changes height (winit passes no event for it): on Android when the
activity's content rectangle moves, on iPhone and iPad when UIKit's keyboard
notifications move the keyboard's frame (§3.1). And the editor scrolls the
caret back into view whenever its view changes size while it has the
keyboard. A press on the Writer's own bars
calls `Editor::keep_focus`, so the editor keeps egui's focus (and the IME
its keyboard) through the tap, and a formatting command from a menu gives
the keyboard back to the text (`run_typing`).

Composition mode (`F11`) leaves only the text, set large on dark paper with
typewriter scrolling. Find and replace work in what is shown or across the
whole project. Compile (`Ctrl+Shift+E`) puts the draft together (folder
titles as chapter headings, scene breaks or new pages between documents, a
title page) and writes any of the formats above. Projects come from
templates: blank document, novel, short story, screenplay, non-fiction book,
essay or paper, poetry collection, letter and report.

**Woven in.** A project is an entity of the arena; its documents are the
shared rich text, its pictures the shared pictures. The Sue menu offers Send
To Sue Writer for any text (opened in place) and any picture (put into the
text at the caret, drawn with Sue Paint's compositing). Opening a Word,
Markdown, HTML, Fountain or plain text file anywhere in Sue
(`SueApplet::opens_file`) brings it into Sue Writer. Writing sessions are
counted and kept on the project by day (`sue_writer.history`).

---

## 4. Introspective rice engine (`sue-rice`)

### 4.1 File format

A `.suerice` file is a TOML subset: `[table]` headers (dotted names allowed),
`key = value` lines (bare and dotted keys), and comments. Values are booleans,
integers (decimal with `_` separators, `0x`, `0o`, `0b`), finite floats, and basic
(`"…"`, with escapes) or literal (`'…'`) strings. Arrays, inline tables,
multi-line strings, dates and arrays of tables are rejected with a line-numbered
`RiceError::Unsupported`. They are never silently misread.

**Metadata** lives in the trailing comment of each key. Tokens come first, in any
order, and whatever text follows them is the tooltip:

```text
metadata := { range | choices | tag } [ description ]
range    := "[" [number] ".." [number] "]"           e.g. [0..64]  [0.0..1.0]  [1..]
choices  := "{" ident { "|" ident } "}"              e.g. {auto|balanced|legacy-low-spec}
tag      := "@color" | "@keybind" | "@shader" | "@text"
```

### 4.2 Type inference → `RiceParameter` → widget

| Literal | Metadata | `RiceParameter` | `Control` |
| --- | --- | --- | --- |
| `true`/`false` | n/a | `Bool` | `Toggle` |
| integer | `[a..b]` (integer bounds) | `IntRange` | `Slider` (step 1), or `NumberField` if a bound is missing |
| integer or float | float bounds, or a float literal | `FloatRange` | `Slider` (step = span / 100), or `NumberField` |
| string | `{a\|b\|…}` | `EnumChoice` | `Dropdown` |
| string | `@color`, or matches `#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa` | `ColorHex` | `ColorPicker` |
| string | `@keybind`, or `Mod+…+Key` with at least one modifier | `Keybind` | `KeybindRecorder` |
| string | `@shader`, or ends in `.wgsl` `.glsl` `.hlsl` `.spv` `.metal` `.frag` `.vert` `.comp` | `ShaderPath` | `ShaderPicker` |
| string | `@text`, or nothing else matched | `Text` | `TextField` |

Metadata that does not fit the literal, such as a range on a boolean or an
unknown tag, is a hard error, because it is a mistake in the schema. An
out-of-range value or an unlisted choice is a **user** mistake. The engine clamps
it, or falls back to the first choice, and records a `RiceDiagnostic`, so a typo
never stops Sue from booting.

### 4.3 Two-way binding

- **File to GUI:** `RiceSchema::parse` builds the schema, and
  `SettingsTree::build(&schema)` turns it into sections and widgets. Labels come
  from keys with known acronyms and units spelled out (`border_width` reads
  "Border width", `ui_scale` reads "UI scale", `settle_grace_ms` reads "Settle
  grace (ms)"), and descriptions come from the trailing comments. Each widget's
  `binding` is its dotted path.
- **GUI to file:** a widget edit becomes `SetRiceValue { path, value }`, a
  `ReversibleTransaction<RiceSchema>`, so Settings changes are undoable.
  `SetRiceValues` sets several values as one step (a theme preset, a reload from
  disk), all or none. `RiceSchema::set` type-checks and range-checks the value
  (an int is accepted for a float), then bumps `revision()`.
- **Format-preserving write-back:** `to_suerice()` reproduces every unchanged line
  byte for byte, comments and spacing included. For a changed key it splices
  only the new value into the original value span. Defaults registered at runtime
  are inserted at the end of their section, or in a new section.

**Settings.** Preferences open as a floating window over the page (the Sue
menu's Settings, or `keys.open_settings`, `Ctrl+,`). A
category sidebar (`layout.settings_sidebar_width`, 190 px) lists the pages; the
right pane shows only the chosen page, under a "Filter settings..." field.
While a filter is active, matching parameters from every page are listed.

The pages are data the engine registers in its `Catalog`, like applets, so
packages can add their own:

| Page | Holds |
| --- | --- |
| General | Where Sue opens (`launchpad.pinned`, labeled Open At Launch: the Hub, or the studio or tool pinned there), how many recent files the Hub lists (`launchpad.recent_count`), how often unsaved work is kept (`general.autosave_seconds`, labeled Autosave Every) and quit confirmation |
| Appearance & Rice | Theme presets (Void, Graphite, Paper, Gray), every color, geometry (padding, hairlines, radii, pill buttons, the top bar and the status bar, shadows), typography, the rice file with Reload, Export and Open Folder |
| Display & Graphics | The performance preference (`profile.hardware`, labeled Performance Preference) and the active profile; the GPU preference (picked by adapter type: integrated, discrete, or the display's GPU), the GPU drawing, every GPU found and Restart Sue (the GPU is chosen at startup); the frame-rate cap, the settle grace, UI scale, list wheel speed, motion (instant snap, animation speed, spring constants, caret blink, smooth caret and its glide time, message time) |
| Audio & DSP | Output device, sample rate, buffer size, real-time priority, volume, and the live stream and real-time status |
| Files & Codecs | JPEG XL effort, bit-exact JPEG recompression, the conversion folder and JPEG quality, the open-file size limit, byte-map width, the built-in codecs |
| Studios | Each studio's own settings and theme: for Sue Paint the new picture size and resolution, `sue_paint.theme` (Gray by default, the Sue theme, a preset, or Custom) and the Custom colors (`sue_paint_colors`) |
| Packages & Updates | The signature policy, the installed applets, installed packages, the active generation and rollback state |
| Keymaps | Every shortcut as an action with all its chords: Sue's own, then Sue Paint's (Clip Studio Paint's defaults, tools and brush keys too), Sue Writer's and the Image Viewer's |
| MCP | "Not yet available." (a `SettingsItem::Note`) |

Any rice section no page claims is listed on a final "Other" page, so every
parameter stays reachable (tested).

| Parameter | Control |
| --- | --- |
| `Bool` | A black and white switch |
| `IntRange` / `FloatRange` | Slider with a 4 px track, a filled part and a round thumb, its value on the right |
| `ColorHex` | A swatch that opens the shared color wheel (`sue_wm::color_button`: a hue ring around a saturation and value square, an opacity bar, HSV numbers and a row of grays) in a layer over every window; dragging previews, release commits |
| `EnumChoice` | A segmented control for up to five short options, else a compact drop-down |
| A text parameter with live choices (output device, the pin) | A drop-down |
| `Keybind` | One key cap per chord: click one and press its new chord, x takes one away, + adds another, as many as wanted (`Esc` cancels; global shortcuts are held back while recording) |
| `ShaderPath` / `Text` | Text field |

Text fields commit when they lose focus. A slider or color wheel drag
previews live: it sets the schema directly, so the whole UI follows the value as you
drag. On release the preview is reverted and one `RiceEdit(SetRiceValue)` is
recorded, so a whole drag is one undo step. After any change (an edit, a
preview, an undo or a redo) the runtime re-reads `WmConfig`, the
`HardwareProfile`, the `Theme`, the UI font, the audio stream settings and,
when they changed, the catalog. Once no drag is in progress, it writes the rice
back with `write_atomic` to `<config>/rice/default.suerice`. A test edits a
value, persists it, checks that exactly one line changed, undoes the edit, and
checks that the file is identical to the original again.

**Theme presets.** Void (the default), Graphite and Paper ship as `.suerice`
files with a `[colors]` section. All three are pure grays, from `#000000` to
`#ffffff`; a test checks that every shipped color, preset or default, is
achromatic. Applying one is a single
`SetRiceValues` step; the switcher marks the preset whose colors are all in
effect, or none for custom colors.

The file is read at boot (and by Reload from Disk), and is not watched yet. An
edit made in a text editor while Sue runs is overwritten by the next change made
in Settings; Reload from Disk applies the file's differences as one undoable
step first.

### 4.4 Registration and migration

Subsystems and applets declare their riceables as `RiceDeclaration { path,
default, description }`. `RiceSchema::register` inserts a missing key, or keeps the
user's value when its type is compatible (re-clamping it to the declared range).
An incompatible user value is replaced and reported. `sue_wm::register_rice`
declares the window manager's constants and `shell_declarations` the shell's.
Tests check that the shipped `default.suerice` matches the declarations key for
key, description included, and that it holds no undeclared key.

**Format and migration.** The shipped file declares its format in its header
(`# sue-rice format 5`). Each released default is frozen in `rice/history/`
(`v1` to `v4`). At boot, `migrate` brings an older file up to date: a value
still equal to the old default takes the new default (so the purple and gold
palette of format 1 and the blue `#3b82f6` accent of format 2 become the
monochrome palette), a value the user changed is carried over when its key
still exists and the value passes the new checks, and the rest is reported. A
key that moved is carried to its new name: format 5 turned the boot mode into
the Hub's pin, so `launchpad.boot_mode` set to a studio becomes
`launchpad.pinned`, while `hub` and `canvas` mean no pin; the keys of the
canvas, the drawers, the File Manager, Notes and the Player are gone. Any color
that is not a gray is reset to its default even in a current file (`purged`),
so a tinted color never survives a start. The old file is kept as
`default.suerice.v<format>.bak`. A file that does not parse is moved to
`default.suerice.broken` and the defaults are used, so a typo never stops Sue
from starting.

---

## 5. Atomic self-updater & package manager (`sue-pkg`)

### 5.1 Packages and content addressing

`PackageManifest { id, version, kind, summary, payload, payload_size,
dependencies, delta_base }` describes one package. `kind` is one of `Applet`
(including hot-loadable WASM plugins), `Codec`, `AiModel`, `RiceTheme` or
`CoreUpdate`. Payloads are **content-addressed** by SHA-256 (`ContentHash`,
displayed as `sha256:<hex>`). `ContentStore` deduplicates identical payloads
across packages and generations, and `verify_payload` checks both the hash and the
size before anything is installed.

### 5.2 Trust: Ed25519

`SignedManifest` pairs a manifest with an Ed25519 signature (`ed25519-dalek`,
`verify_strict`, which rejects malleable and small-order signatures). The signed
message is `canonical_bytes()`: a deterministic, domain-separated
(`sue-manifest-v1`) serialization, with dependencies sorted and the summary
escaped. `TrustStore` maps publisher key ids to verifying keys.
`PackageIndex::add_verified` is the only way a remote manifest enters the
resolver, so **an unverified manifest can never be resolved or installed.**
Locally built packages go through the explicitly named `add_local`.

### 5.3 Dependency resolution

Versions and requirements use Cargo's SemVer semantics (`semver` crate: `^`, `~`,
`=`, comparison operators, comma-separated sets). `resolve(&index, &requests)` is a
complete backtracking search. It tries candidate versions newest first, so the
first solution it finds is the most up to date one. Every requirement placed on a
package is checked against every candidate version for it, and a conflict
backtracks to the most recent choice point. The result is ordered topologically
(dependencies before dependents, deterministic ties). A cycle is reported as
`DependencyCycle`, naming the packages caught in it.

### 5.4 Generations and atomic rollback

The installed set is a sequence of immutable **generations**. Updates never edit
the live generation:

```text
        stage(plan)          verify()            activate()           confirm()
 Idle ─────────────► Staged ─────────► Verified ──────────► Activating ─────────► Idle (committed)
   ▲                   │                  │                      │
   └──── abort ────────┴──────────────────┘                      └─ rollback() ─► Idle (previous generation restored)
```

- `stage` builds the candidate generation from the resolved plan and checks that
  every payload is present in the store.
- `verify` re-hashes every payload in the candidate. Any mismatch aborts back to
  `Idle`, and the live generation is never touched.
- `activate` switches the active-generation pointer in one assignment. On disk,
  `persist_active_pointer` writes the pointer through write-temp, `fsync`,
  `rename`, so a crash leaves either the old pointer or the new one, never a torn
  one.
- `apply_with_health_check` runs the whole pipeline. If the post-activation health
  check fails, it **rolls back automatically** and reports
  `UpdateOutcome::RolledBack`.
- Invalid transitions (for example `confirm` while `Idle`) are typed errors, not
  panics.

`CoreUpdate` packages MAY name a `delta_base`. The payload is then a binary delta
against that generation's binary, signed like any other payload.

---

## 6. Zero-copy offline AI engine (`sue-ai`)

### 6.1 Model formats

| `ModelFormat` | Identified by | Header facts parsed |
| --- | --- | --- |
| `Gguf` | magic `GGUF` | version (1 to 3), tensor count, metadata key/value count (32-bit counts in v1, 64-bit after) |
| `Safetensors` | 8-byte LE header length (≤ 100 MB) followed by `{` | header length |
| `Onnx` | `.onnx` extension (protobuf has no magic) | n/a |
| `Magic` | `.magic` extension: Sue's native serialized compute graph | n/a |
| `Builtin` | compiled into the engine | n/a |

### 6.2 Compute backends

| `ComputeBackend` | Available when | Rank |
| --- | --- | --- |
| `Cuda` | Windows/Linux with an NVIDIA GPU | 4 |
| `Metal` | macOS / iOS | 4 |
| `VulkanWgpu` | Vulkan or DX12 graphics API present | 3 |
| `AndroidNnapi` | Android | 2 |
| `CpuSimd` | always (auto-vectorized Rust) | 1 |

`ComputeBackend::detect_available(os, &caps)` returns the usable backends, best
first.

### 6.3 Local model registry

`LocalModelRegistry` holds `ModelDescriptor { id, task, format, size_bytes,
quantization, min_memory_bytes, backends }`. `select(task, &available,
memory_budget)` picks the best model and backend pair: the highest-ranked backend
the model supports, then the largest model (as a proxy for quality) that fits
the memory budget. Nothing in `sue-ai` opens a network connection. Models arrive
as `AiModel` packages through `sue-pkg`.

### 6.4 `ZeroCopyTensorTask`

```rust
pub trait ZeroCopyTensorTask: Send + Sync {
    fn name(&self) -> &str;
    fn task(&self) -> AiTask;                       // fixes input/output component kinds
    fn supported_backends(&self) -> &[ComputeBackend];
    fn compute(&self, input: &ComponentData, output: &mut ComponentData,
               backend: ComputeBackend) -> Result<TaskReport, AiError>;
    fn run(&self, arena: &mut Arena, input: AssetHandle, output: AssetHandle,
           available: &[ComputeBackend]) -> Result<TaskReport, AiError> { /* provided */ }
}
```

`run` takes the input and output `AssetHandle`s, borrows both entities at once
with `Arena::split_pair` (one shared, one exclusive), and passes the components
to `compute` as borrowed tensor views. **No buffer is copied and nothing touches
the disk.** Stem separation, OCR, inpainting, shader generation and AST
refactoring all read from and write to the same memory the studios display.
`TaskReport::bytes_copied` is part of the contract and is `0` for conforming
kernels.

The reference kernel is `MidSideStemSplit`
(`AiTask::StemSeparation`, `CpuSimd`). It splits stereo into a center stem
$M = (L + R)/2$ (lead vocal, bass, kick) and a side stem $S = (L - R)/2$
(ambience, wide instruments). The split is exactly invertible ($L = M + S$,
$R = M - S$). It needs no model, it is the classic baseline that learned
separators are measured against, and it exercises the full zero-copy path.

---

## 7. Implementation status

| Spec | Status | Where |
| --- | --- | --- |
| §1.2 Arena, handles, split borrows | **Implemented** (single-writer; lock-free arena planned) | `sue-core/src/arena.rs`, `handle.rs` |
| §1.2 Stable `EntityId`s, id lookup, revisions | **Implemented** | `sue-core/src/id.rs`, `arena.rs` |
| §1.3 Entities: class, optional base, properties, extensions, metadata | **Implemented** | `sue-core/src/arena.rs` |
| §1.4 Seven frozen built-in components with validated encodings | **Implemented** | `sue-core/src/component.rs` |
| §1.4 Projections, mask, pointer stability | **Implemented** | `sue-core/src/projection.rs`, `arena.rs` |
| §1.5 Open components, registry, opaque round trip, binary codec | **Implemented** | `sue-core/src/registry.rs`, `codec.rs` |
| §1.6 Relations, trees, fractional order keys | **Implemented** | `sue-core/src/relation.rs` |
| §1.7 `Value`, `Properties`, `PropertySchema` | **Implemented** | `sue-core/src/value.rs` |
| §1.7 Units and quantities | **Implemented** (one table; derived units beyond the table are planned) | `sue-core/src/units.rs` |
| §1.7 Managed color | **Implemented** (matrix and transfer conversions; ICC profiles and gamut mapping planned) | `sue-core/src/color.rs` |
| §1.7 Time, rates, drop-frame timecode | **Implemented** | `sue-core/src/time.rs` |
| §1.8 Delta-tree undo, `checkout`, `record` | **Implemented** | `sue-core/src/undo.rs` |
| §1.8 Generic edits by id and `Batch<T>` | **Implemented** | `sue-core/src/txn.rs`, `format.rs` |
| §1.8 One global history for arena and rice edits | **Implemented** (`UndoStack<World>`) | `sue-wm/src/world.rs` |
| §1.9 `.sue` container, `ARNA`, `META`, `PRST`; an old `SCNE` kept untouched | **Implemented**; save with `Ctrl+S`, open like any file; compressed chunks planned | `sue-core/src/file.rs`, `sue-wm/src/world.rs`, `bins/sue/src/runtime.rs` |
| §1.10 Format registry, bundles, copy with fresh ids | **Implemented**; opened and dropped files still go through the ingest path (§2.2) and the `.sue` handler | `sue-core/src/format.rs`, `sue-io/src/formats.rs` |
| §1.11 Operators and presets | **Implemented** in the kernel; the palette and applets do not route through operators yet | `sue-core/src/operator.rs` |
| §1.12 Job pool and shared buffers | **Implemented**; no feature uses the pool yet | `sue-core/src/jobs.rs`, `shared.rs` |
| §1.8 `RasterEdit` (pixel rectangles before and after) | **Implemented**; Sue Paint records every stroke, fill and filter with it | `sue-core/src/txn.rs` |
| §1.13 Shared models (text marks, table, curves, sequence, notes, graph, geometry, layout, encoded) | **Implemented** as data with tests; no applet edits them yet | `sue-model` |
| §1.13 Layered pictures and blend modes | **Implemented**; Sue Paint makes and edits them | `sue-model/src/picture.rs`, `blend.rs` |
| §3.8 Rich text with paragraph marks, manuscripts | **Implemented**; Sue Writer edits them | `sue-model/src/text.rs`, `manuscript.rs` |
| §3.8 Installed fonts | **Implemented** (a background scan; faces installed into egui on first use) | `sue-wm/src/fonts.rs`, `sue-platform/src/paths.rs` |
| §3.8 Rich text editor, styles, page layout, typesetting | **Implemented**; the caret glides and blinks at rest (`sue-type/src/caret.rs`) | `sue-type` |
| §3.8 Publishing formats and PDF | **Implemented** (Word both ways; OpenDocument, EPUB, HTML, RTF, Markdown, text out; HTML and Markdown in; fonts embedded whole, not subset) | `sue-io/src/publish`, `pdf.rs`, `zip.rs` |
| §3.8 Sue Writer | **Implemented**; on a phone laid out for a thumb and an on-screen keyboard (text reflowed to the screen, formatting above the keyboard, the caret kept in sight); snapshots show the phone with and without the keyboard | `sue-writer`, `sue-type/src/editor.rs`, `sue-platform/src/desktop.rs`, `crates/sue-platform/ios/src/keyboard.rs` |
| §2.1 Three-tier sniffer | **Implemented** | `sue-io/src/sniff.rs` |
| §2.2 JXL probe (boxes + `SizeHeader`) | **Implemented** | `sue-io/src/jxl.rs` |
| §2.2 JXL pixel decode and JPEG reconstruction | **Implemented** with `jxl-oxide` (no CMYK yet; the DC preview and pass targets come from a full decode until a streaming decoder lands) | `sue-io/src/jxl.rs` |
| §2.2 Lossless JXL encode | **Implemented** with `zune-jpegxl` (one `Gray8`, `Rgba8` or `RgbaF32` layer; multi-layer planned) | `sue-io/src/jxl.rs` |
| §2.2 Lossless JPEG → JXL recompression | Planned; returns `CodecUnavailable` | `sue-io/src/jxl.rs` |
| §2.2 PNG and JPEG decode, PNG encode | **Implemented** with `zune-png` and `zune-jpeg` | `sue-io/src/raster.rs` |
| §2.2 Audio decode (MP3, FLAC, WAV, AIFF, Ogg Vorbis, AAC) and WAV, AIFF and FLAC encode | **Implemented** with `symphonia` and `flacenc`; WAV in 16, 24 or 32-bit float, AIFF and FLAC in 16 or 24-bit | `sue-io/src/audio.rs` |
| §2.2 Ingest of opened and dropped files (sniff, then raster, audio, text or bytes entity) | **Implemented** (decoding runs on the UI thread; spawning the entity is not yet an undoable transaction) | `sue-io/src/ingest.rs`, `bins/sue/src/runtime.rs` |
| §2.3 Tile cache, byte map | **Implemented** | `sue-io/src/tile_cache.rs`, `bytemap.rs` |
| §3.1 `PlatformHost`, paths, `HeadlessHost` | **Implemented** | `sue-platform` |
| §3.1 Native desktop host (`winit` window, `wgpu` surface, egui input, IME, clipboard, file drops) | **Implemented** (one window; tested on Windows) | `sue-platform/src/desktop.rs`, `gpu.rs`, `paint.rs` |
| §3.1 GPU choice (`display.gpu_preference`: auto, integrated, discrete) | **Implemented**; `auto` draws on the display GPU, which a device test checks on Windows | `sue-platform/src/gpu.rs`, `bins/sue/src/desktop.rs` |
| §3.1 Frame pacing: application and egui repaint requests combined in `about_to_wait`, timed wakes, close confirmation | **Implemented**; the self-check asserts that neither the runtime nor egui asks for frames once idle | `sue-platform/src/desktop.rs`, `bins/sue/src/runtime.rs` |
| §3.1 Native open, save and folder dialogs, system browser, file manager | **Implemented** with `rfd` and `open` (web addresses limited to `http` and `https`) on desktops | `sue-platform/src/os.rs` |
| §3.1 The system's own file pickers on Android and iOS (staged copies handed back) | **Implemented**; the engine is tested headless with a stand-in system; Android's picker is tried on an emulator; the iOS picker is type-checked but not yet run on a device | `sue-platform/src/picks.rs`, `android.rs`, `ios.rs`, `crates/sue-platform/ios`, `packaging/android/java` |
| §3.1 Layout above the on-screen keyboard on Android and iOS | **Implemented**; Android reads the activity's content rectangle; iOS watches UIKit's keyboard notifications, type-checked but not yet run on a device (the geometry is tested on every system) | `sue-platform/src/desktop.rs`, `crates/sue-platform/ios/src/keyboard.rs`, `cover.rs` |
| §3.1 Native mobile hosts (Android, iOS) | Planned (the mobile topologies already run on `HeadlessHost`) | n/a |
| §3.2 to 3.4 Form factors, layout, springs, scheduler | **Implemented** | `sue-wm` |
| §3.4 Painting with egui (`egui-wgpu`), arena images as native textures | **Implemented**; the self-check renders a frame on the GPU and counts tinted pixels (zero) | `sue-platform/src/paint.rs` |
| §3.4 Multi-pass shader effects | Planned (the `ShaderPath` parameter type exists; no shipped parameter uses it yet) | n/a |
| §3.5 Audio output (ring, atomic gain, real-time priority) and the shared transport | **Implemented** with `cpal`; the transport is tested headlessly with `NullAudio`, and a device test streams to the real output | `sue-platform/src/audio.rs`, `sue-wm/src/transport.rs` |
| §3.5 DAW audio graph | Specified; planned with the DAW studio | n/a |
| §3.6 Applets, Launchpad, the Hub with its pins, the top bar with the Sue button and the page's menus, palette, status bar | **Implemented** (five built-in applets: two studios, two tools and Settings, see §3.6) | `sue-wm`, `bins/sue/src`, `sue-paint`, `sue-writer` |
| §3.6 Text editing over the arena with one global undo (`ArenaText`, `Typing`) | **Implemented**; the self-check types, undoes with `Ctrl+Z` and redoes with `Ctrl+Y` through egui events | `sue-wm/src/text.rs`, `sue-core/src/txn.rs` |
| §3.6 Timeline: lanes, projected blocks, playhead, playback of audio blocks | **Implemented** (blocks are moved, not trimmed; video lanes arrive with the video studio) | `sue-wm/src/timeline.rs`, `shell.rs` |
| §3.6 Studio role, data-driven menu bars with grayed items | **Implemented** | `sue-wm/src/applet.rs`, `shell.rs`, `menubar.rs` |
| §3.6 Studios beyond Sue Paint (DAW, video, 3D and CAD, IDE, game engine) | Planned; they arrive as studio applets, the way Sue Paint did | n/a |
| §3.7 Sue Paint: layout, every menu, tools and sub tools, brush engine with pressure and stabilization, fills, gradients, figures, selections, layers, 26 blend modes, edits and filters, open and export | **Implemented**; an end-to-end test drives it with egui events (New, a pressure stroke, a layer, undo, save and reopen), the self-check paints, undoes, saves and exports, and three snapshots show it | `sue-paint`, `bins/sue/src/runtime.rs`, `selfcheck.rs`, `snapshot.rs` |
| §3.7 Sue Paint: pen pressure curves and dynamics, starting and ending modes, color jitter, puppet warp, stories and the Page Manager, frame borders, balloons, focus and speed lines, screentone, animation timelines with onion skin and GIF export | **Implemented**; end-to-end tests drive puppet warp, stories, manga and animation with egui events, and snapshots show the pressure dialog, the Page Manager and the Timeline | `sue-paint`, `bins/sue/src/runtime.rs`, `snapshot.rs` |
| §3.7 Sue Paint: scale and free transform, layer folders, layer masks, canvas size, crop and resolution, the symmetrical ruler, text layers and balloon text, tone curve, levels, color balance and gradient maps | **Implemented**; end-to-end tests drive each with egui events (Ctrl+T, Ctrl+G and the folder moves, crop, a mirrored stroke, a mask erased, words written and edited) | `sue-paint`, `sue-type/src/raster.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: Close gap and Area scaling, the Blend tool (Blur, Finger Tip, Liquify) and correction layers | **Implemented**; unit tests for a fill held in a broken ring, every dab kind and every correction read back from its text; end-to-end runs that liquify a stroke and add, undo and cancel correction layers | `sue-paint/src/fill.rs`, `blend.rs`, `composite.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: rulers (linear, parallel and radial lines, concentric circles, 1, 2 and 3-point perspective, symmetry), saved on the picture | **Implemented**; unit tests for their text, the guide a stroke chooses, and vanishing points made and moved; an end-to-end run that creates a 2-point ruler, holds a drifting stroke upright (and not with snapping off), adds a third point from two lines, and deletes and undoes | `sue-paint/src/rulers.rs`, `canvas.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: Onion Skin Settings and the light table | **Implemented**; a unit test for as many cels as asked fading a step each, color modes and light table layers, and the animation end-to-end run keeps the settings and registers, refuses and undoes a light table layer | `sue-paint/src/anim.rs`, `anim_ui.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: vector layers with the Vector eraser (touched area, up to intersection, whole line) | **Implemented**; unit tests for a brush and lines read back, a line painted again to the same pixels, and each eraser mode; an end-to-end run that draws two crossing lines, undoes and redoes one, erases up to the crossing and undoes it | `sue-paint/src/vector.rs`, `brush.rs`, `canvas.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: Correct Line (control points, pinch, simplify, connect, adjust line width, redraw, redraw width) and Remove dust | **Implemented**; unit tests for every operation and dust removal, and the vector end-to-end run pinches a line and undoes it | `sue-paint/src/correct.rs`, `vector.rs`, `brush.rs`, `canvas.rs` |
| §3.7 Sue Paint: the Operation tool (Object for vector lines, Select layer), curve, figure and pen rulers | **Implemented**; a unit test for path rulers read back and holding strokes on a curve and round a rectangle's corner; the vector end-to-end run moves a line with Object, deletes and undoes it | `sue-paint/src/rulers.rs`, `canvas.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: timelapse recording and export, and movies of animations (Motion JPEG AVI) | **Implemented**; unit tests for the AVI's headers and index and for frames kept, fitted and timed; an end-to-end run records a frame a stroke, exports a video and stops | `sue-io/src/video.rs`, `sue-paint/src/timelapse.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: camera folders with keyframed position, zoom and rotation | **Implemented**; unit tests for the camera's map and frame, keyframes in between, deleted and undone, and a dot rendered four times larger through the camera | `sue-paint/src/camera.rs`, `composite.rs`, `anim_ui.rs` |
| §3.7 Sue Paint: the Material palette, built-in tones and patterns, Register Material, Image | **Implemented**; a unit test for a tone's coverage and color and a picture registered, cut, read back and placed; an end-to-end run registers a drawing and pastes it as a layer | `sue-paint/src/materials.rs`, `panels.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: Layer Property effects (border, tone, layer color, expression color) and right-click menus | **Implemented**; unit tests for the border's reach, rim and roundness, each effect's color, a tone's coverage, a border rendered round a dot with the pixels kept, and sub tools duplicated, reset and deleted | `sue-paint/src/effects.rs`, `composite.rs`, `panels.rs`, `tools.rs`, `sue-wm/src/menubar.rs` |
| §3.7 Sue Paint: Polyline, Selection pen and Erase selection; expand, shrink and blur selections; Select Color Gamut; selection layers; Quick Mask; the Selection Launcher | **Implemented**; a unit test for growing, shrinking, blurring, painting and picking colors, and an end-to-end run that shrinks a full selection, keeps it as a layer and turns Quick Mask on and off | `sue-paint/src/select.rs`, `canvas.rs`, `studio.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: 3D layers (shapes and OBJ models), their camera and look, and lines and tones from them | **Implemented**; unit tests for the renderer's coverage, outlines and folds (a sphere smooth inside), each look, and a layer turned and undone; an end-to-end run makes a cube, turns it with the Object tool, undoes, converts it to lines and tones and imports an OBJ model | `sue-paint/src/three.rs`, `canvas.rs`, `panels.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: audio tracks on the animation timeline, played in step | **Implemented**; a unit test for a sound imported, mixed from its frame, moved, removed and undone; the animation end-to-end run imports a WAV and plays and stops it with the animation | `sue-paint/src/anim.rs`, `anim_ui.rs`, `bins/sue/src/runtime.rs` |
| §3.7 Sue Paint: brush tip materials | Planned; the menu item is present and grayed out | n/a |
| §3.6 Embedded web view | Planned (a Browser drawer of earlier builds was retired) | n/a |
| §3.6 A bold typeface for documents | Planned; the bundled fonts have no bold weight, so bold spans show in full white with wider spacing | `bins/sue/src/document.rs` |
| §4 Rice parser, GUI descriptors, write-back, profiles | **Implemented** | `sue-rice` |
| §4.3 Settings window: categories, filter, segmented, slider, switch, color and keycap controls, presets, undoable edits, atomic persistence | **Implemented** (the file is read at boot and by Reload from Disk; watching it for outside edits is planned) | `bins/sue/src/settings.rs`, `runtime.rs`, `engine.rs` |
| §4.4 Rice format marker, migration from formats 1 and 2, tinted colors reset | **Implemented**, with a backup of the old file | `sue-rice/src/migrate.rs`, `bins/sue/src/engine.rs` |
| §5 Hashes, Ed25519, resolver, generations, atomic pointer | **Implemented** | `sue-pkg` |
| §5 Package download, binary deltas, WASM plugin loading | Planned (the signature policy is riced and shown; nothing installs packages yet) | n/a |
| §6 Formats, backends, registry, zero-copy tasks | **Implemented** (reference kernel on `CpuSimd`) | `sue-ai` |
| §6 GGUF/ONNX inference runtimes | Planned (`sue-ai` selects backends and models; its Settings page returns with the runtimes) | n/a |
| House style: no em dashes, en dashes or middle dots | **Checked** by a test that scans every text file | `bins/sue/src/style.rs` |
| §8 Features beyond today's applets | Ideas mapped onto the backbone; none is promised | n/a |

---

## 8. Feature map: how future features sit on the backbone

Sue aims to cover, over time, the work people do today in many separate
programs. Sue does not clone those programs: it has its own interface and its
own ways of working, and many ideas below may never ship. This section exists
so that whichever features do arrive, they arrive as parts of **one** app. For
each area it names the entity classes, the backbone pieces (§1) it builds on,
the formats involved, and what the feature itself must still add. Programs are
named only to say which kind of work is meant.

### 8.1 How a feature plugs in

A feature (built in, or a package later, §5) only ever does these things:

1. **Classes.** Picks namespaced entity classes for what it makes
   (`noelle.scene`, `cad.part`), reusing the well-known ones in
   `sue_model::class` when the thing is the same thing.
2. **Data.** Stores data in built-in buffers, shared models (§1.13) or its own
   `ComponentType`s, registered in the `ComponentRegistry`. Links are
   `EntityId`s and `Relation`s, never handles or paths.
3. **Edits.** Changes data only through `ReversibleTransaction`s (generic ones,
   or its own delta transactions), so one undo history covers everything.
4. **Actions.** Registers `Operator`s with parameter schemas, so the palette,
   menus, shortcuts, presets, scripts and batch runs reach it at once.
5. **Formats.** Registers `FormatHandler`s that import to bundles and export
   entities, with options as `PropertyDef`s for presets.
6. **Nodes.** Registers `NodeType`s if it has a node graph, so its nodes can
   mix with other features' nodes in one `NodeGraph`.
7. **Interface.** Registers applets with the `Launchpad` (§3.6) that draw and
   edit entities of its classes, and riced parameters (§4) for its look.
8. **Heavy work.** Runs it on the `JobPool` over `Shared` buffers and applies
   the result as a transaction.

Because every step goes through a registry, a feature that is missing (not
installed, or not written yet) costs nothing and breaks nothing: its entities
still open, save and copy, with their components kept as `OpaqueComponent`s and
their files as `Encoded` bytes.

### 8.2 Documents, writing and knowledge

| Area (for example) | Entities and data | Backbone pieces | Still to build |
| --- | --- | --- | --- |
| Word processing (Word, Pages) | `sue.document`: `StringBuffer` plus `TextMarks` (paragraph marks on the newline); pictures by `embed` marks | rich text edits and their undo, style sheets, pagination, footnotes, Word both ways, OpenDocument out (Sue Writer, §3.8) | tables, comments, tracked changes (as transactions), spelling, footnotes at the foot of each page, OpenDocument and RTF in |
| Spreadsheets (Excel, Sheets) | `sue.sheet`: `Table`; several sheets as `CHILD` entities of a workbook folder | `Table`, `SetCell`, `Value`, `Quantity`, CSV and TSV handlers (done) | a formula engine with dependency tracking, charts (entities that `REF` a table), XLSX and ODS handlers |
| Slideshows (PowerPoint, Keynote) | `sue.slides`: `PageLayout` with slide-sized pages; frames show text, pictures, video | `PageLayout`, `Animation` (builds), `Sequence` (timed shows) | transitions, presenter view, PPTX handler |
| Long-form writing and manuscripts | a `sue.manuscript`: a binder of folders and documents (`CHILD`, ordered), research as any entity under its research folder | `sue-model::manuscript`, the binder, corkboard, outliner, snapshots, targets and compile to every format (Sue Writer, §3.8) | keywords search, linked research (`REF`), revision marks, collections |
| Book publishing and ebook libraries (Calibre) | books as `sue.publication`; library metadata in properties; a catalogue `Table` | `PageLayout`, `Bundle` export, properties | EPUB and MOBI handlers, library index |
| PDF editing and signing | `sue.publication` holding `Encoded` PDF bytes today; later pages as `PageLayout` with frames | `Encoded` (done), `PageLayout`, a signature `ComponentType` | a PDF parser and writer, annotation and form models, signing through `sue-platform` key stores |
| Notes and knowledge bases (Obsidian, Notion) | notes as documents, links as `REF` relations (backlinks are `sources`), databases as `Table` with typed columns | relations, `Table`, `TextMarks` (`link` marks), Markdown handler (done) | graph view, templates, queries over properties |
| Markdown | `sue.document` with Markdown text | Markdown handler (done) | live preview that maps syntax to `TextMarks` and back |
| LaTeX and math | documents with `math` marks (LaTeX source); math solver results as values | `TextMarks`, `Value`, `Quantity` (units in answers) | a typesetter, a computer algebra operator set |

### 8.3 Code and tools

| Area (for example) | Entities and data | Backbone pieces | Still to build |
| --- | --- | --- | --- |
| Code IDE | `sue.code` documents; syntax trees as `ASTNode` entities with `EntityId` children; projects as folders | text edits, `ASTNode`, relations, `JobPool` (indexing, builds) | parsers, language servers (through `sue-platform` processes), debugger |
| Terminal emulator | a terminal session entity; scrollback as text | `StringBuffer`, `JobPool` | PTY access in `sue-platform` (OS-specific, rule 6), escape sequence model |
| Web browser and rendering engine | page entities; a rendered page as a raster or a layout | `PageLayout`, `RasterTile2D`, `Encoded` (HTML) | an embedded engine behind `sue-platform` (system web view) or a native one |
| Local file transfer (LocalSend) | transfers as entities with progress properties; files arrive as bundles | `Bundle` (the unit that travels), `JobPool`, `.sue` and every `FormatHandler` | discovery and encrypted transport in `sue-platform` |
| Remote desktop and collaboration (RustDesk, Tailscale, AnyDesk, TeamViewer) | sessions as entities | `EntityId` (stable across devices), transactions as the unit of change, `OrderKey` (merge-friendly ordering) | screen capture and input injection in `sue-platform`, a sync protocol that ships transactions, conflict rules |

### 8.4 Design, photo and video

| Area (for example) | Entities and data | Backbone pieces | Still to build |
| --- | --- | --- | --- |
| Raster painting and photo editing (Photoshop, Krita, Clip Studio) | a picture is a `sue.image` entity with ordered layer entities (`CHILD`), each a `RasterTile2D` with blend and opacity properties (§1.13); **Sue Paint (§3.7) works on exactly this** | `sue_model::picture`, `BlendMode`, `RasterEdit`, `CHILD` order, `style.*` properties, `Color` (managed), JPEG XL pipeline | brushes as operators, masks, adjustment layers as `NodeGraph`s, vector and text layers, PSD and KRA handlers |
| Vector design and UI design (Illustrator, Figma, CorelDraw) | `sue.vector` entities: `Geometry` with Bezier prims, fill and stroke properties, `Transform`; artboards as `PageLayout` pages | `Geometry`, `Transform`, `PropertySchema`, `REF` (components and instances) | boolean path ops, text on path, constraints and auto layout, SVG, AI and PDF handlers |
| Photo and RAW development (Lightroom, Topaz) | photos as `sue.image` with `Encoded` RAW bytes; develop settings as properties; a catalogue `Table` | `Encoded` (lossless original), `Color` (ACES and wide gamut), `Preset`s for looks, `JobPool` | RAW decoders, a develop pipeline as `NodeGraph`, AI enhance through `sue-ai` |
| Video editing (Premiere, Resolve, Final Cut) | `sue.sequence` with `Sequence` tracks; clips point at `sue.video` entities (`Encoded` today) | `Sequence`, `Clip` trims and splits, `Rate`, `Timecode`, `Time` | video decoding and encoding (behind a codec package), proxy media on the `JobPool`, export handlers |
| Motion graphics (After Effects) | compositions as sequences of layer entities with `Transform` and `Animation` | `Animation` on any property key, `Transform`, `NodeGraph` (effects) | expression language, rendering |
| Color grading and finishing | grade settings as `NodeGraph`s per clip; LUTs as entities | `Color` (ACEScg, Rec. 2100 PQ and HLG), `Animation` (keyframed grades), `Preset`s | scopes, a GPU grade evaluator, LUT and CDL handlers |
| Screen recording, streaming and transcoding (HandBrake, ffmpeg) | recordings arrive as `sue.video`; transcodes are export jobs | `FormatRegistry` (one export path for every format), `JobPool`, `Preset`s per format | capture in `sue-platform`, encoder packages |

### 8.5 Audio and music

| Area (for example) | Entities and data | Backbone pieces | Still to build |
| --- | --- | --- | --- |
| Playback (VLC) | any audio or video entity | `PCMFloat1D`, the shared transport (§3.5), every audio `FormatHandler` (done for MP3, FLAC, WAV, Ogg, AAC) | video playback |
| DAWs (Ableton, Reaper) | a song is a `sue.sequence`: audio clips point at `sue.audio` entities, MIDI clips at `sue.notes` entities; mixer settings as track properties; automation as `Animation` | `Sequence`, `Notes`, `Animation`, `REF` relations for sends and sidechains, `Time` sample-exact | the real-time audio graph (§3.5), plugin hosting through `sue-platform` |
| Mastering, restoration and analysis (WaveLab) | audio entities; processing chains as `NodeGraph`s | `NodeGraph`, `Preset`s, `JobPool` for offline renders, `ZeroCopyTensorTask` for AI denoise (§6) | DSP node types, loudness meters |
| Sound design and patching (Pure Data, Reaktor) | patches as `sue.graph` entities | `NodeGraph`, `NodeRegistry` (typed ports: `audio`, `control`, `float`), `GraphEdit` | a real-time graph evaluator, DSP node library |
| Voice synthesis and singing (Vocaloid, OpenUtau, Synthesizer V, CeVIO) | a vocal track is `Notes` with `lyric` per note and phonemes, vibrato and expression in note properties; voicebanks as entities | `Notes`, `Sequence`, `Animation` (pitch and dynamics curves), `sue-ai` models | phonemizers, voicebank formats, a synthesis engine (offline AI through `sue-ai`) |

### 8.6 3D, animation and the Noelle engine

| Area (for example) | Entities and data | Backbone pieces | Still to build |
| --- | --- | --- | --- |
| 3D modeling (Blender) | `sue.model` entities: `Geometry` (attributes for normals, UVs, materials), `Transform`; scenes are `sue.scene` `CHILD` trees | `Geometry`, `Transform`, relations, OBJ handler (done), `MeshBuffer3D` for display | editing operators, glTF and FBX handlers, a viewport renderer |
| Sculpting and texturing (ZBrush, Substance) | dense meshes as `Geometry`; texture sets as `CHILD` image entities; material graphs as `NodeGraph` | `Geometry` attributes, `RasterTile2D`, JPEG XL pipeline, `NodeGraph` | multires and dynamic topology data, brush operators, bakers on the `JobPool` |
| 2D animation and rigging (Spine, DragonBones, Toon Boom, Moho, Synfig) | a character is a `sue.scene` tree of parts with `Transform`s; bones as entities; skins as `REF`s; poses and timing as `Animation` | `CHILD` trees, `Transform`, `Animation`, `Geometry` (meshes with weight attributes) | rig model (`ComponentType`), inverse kinematics, onion skin views |
| Compositing and VFX (Nuke, Houdini) | comps and procedural networks as `sue.graph` entities; outputs written to image or geometry entities | `NodeGraph` with typed ports (`image`, `geometry`), `Geometry` attribute domains, `Color`, `JobPool` | node libraries, a cooking engine with caches keyed by `Entity::revision` |
| **Noelle engine** (game engine inside Sue, based on Godot's ideas) | a game is a `sue.folder` of `noelle.scene` entities; a scene is a `CHILD` tree of node entities (class `noelle.node.*`) with `Transform`s and properties; scripts are `sue.code` entities `REF`ed by nodes; assets are ordinary Sue entities (images, audio, models, animations) referenced by id | `CHILD` order (the scene tree), `REF` (scripts, resources, signals as relation kinds), `Transform`, `Animation`, `NodeGraph` (visual scripting, shaders), `Properties` with a `PropertySchema` per node type (the inspector), `Time` and `JobPool` | the runtime (game loop, physics, input, rendering), its node types and operators, export of a playable build; nothing in the backbone needs to change for it |

### 8.7 Engineering, architecture and manufacturing

| Area (for example) | Entities and data | Backbone pieces | Still to build |
| --- | --- | --- | --- |
| CAD drafting (AutoCAD, MicroStation) | drawings as `sue.vector` entities in double precision; layers as a `cad.layer` property and folder trees; dimensions and blocks as entities (`REF` for block instances) | `Geometry` (f64), `Quantity` (mm, in, ft, angles), `PageLayout` for sheets and title blocks, `Table` for schedules | constraint solver, DWG and DXF handlers, snapping |
| Mechanical CAD (Fusion 360, SolidWorks) | parts as `cad.part` with a feature history as an ordered `CHILD` tree of feature entities; assemblies as trees with mates as relations | `CHILD` order (feature history), relations with properties (mates), `Quantity`, `Geometry`, operators with parameter schemas | a B-rep kernel as a `ComponentType`, STEP and STL handlers |
| Architecture, civil and BIM (ArchiCAD, Vectorworks) | buildings as `CHILD` trees (site, building, storey, space, element); elements carry typed properties (fire rating, U-value) and `Geometry` | relations (`bim.contains` registered as a tree kind), `PropertySchema`, `Quantity` (area, volume), `Table` (schedules derived from properties) | IFC handler, parametric wall and slab types, plan and section views on `PageLayout` |
| HVAC, MEP and engineering drafting | systems as graphs: equipment, ducts and pipes as entities, connections as relations with flow data | relations with `Properties`, `Quantity` (cfm, L/s, Pa, inH2O, BTU/h, tons of refrigeration, kW), `Table` (equipment schedules) | sizing calculators as operators, routing, standard symbol libraries |
| Electrical, plumbing and piping | circuits and pipe runs as relation graphs; panels and fixtures as entities | relations, `Quantity` (V, A, ohm, W, gpm, psi), `Table` (panel schedules) | load and pressure calculations as operators, schematic and isometric views |
| Industrial design and manufacturing | products as assemblies; manufacturing data (material, tolerance, process) as properties; BOMs as `Table`s derived from the tree | `CHILD` trees, `PropertySchema` (`cad.material`), `Table`, `Quantity`, `Geometry` | CAM toolpaths as `Geometry` polylines, G-code export handler, render and simulation jobs |

### 8.8 What the backbone deliberately leaves to features

The backbone does not draw, decode video, evaluate formulas, solve
constraints, cook node graphs or run game loops. It gives every feature the
same entities, ids, relations, values, units, color, time, undo, files, formats,
operators, presets, jobs and models, and it guarantees that data made by one
feature stays intact, linkable and undoable in all the others. When a feature
needs something the backbone lacks, the change goes into the backbone (with
the rules in [`AGENTS.md`](AGENTS.md)), not around it.
