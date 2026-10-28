# HUMANZ: Sue Systems Specification

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

This is the architectural specification of **Sue: Singular Unified Engine**, v1.0
"Faustian Enigma". It is written for systems engineers. The plain-English
companion is [`HUMANS.md`](HUMANS.md); the backbone rules every contributor,
human or AI, follows are in [`AGENTS.md`](AGENTS.md).

The words **MUST**, **MUST NOT**, **SHOULD** and **MAY** are used as in RFC 2119.
Type names in `code` refer to real items in the workspace. §7 maps every part of
this specification to its implementation status. Like every file in the
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
sue-wm        ──► sue-rice, sue-platform, sue-core   + egui
sue-ai        ──► sue-platform, sue-core
sue-rice      ──► sue-platform, sue-core
sue-io        ──► sue-core                            + jxl-oxide, zune-jpegxl, zune-png, zune-jpeg, jpeg-encoder, symphonia
sue-pkg       ──► sha2, ed25519-dalek, semver         (no engine crates)
sue-core      ──► nothing                              (leaf)
sue-platform  ──► winit, wgpu, egui, egui-winit, egui-wgpu, arboard, pollster, cpal, audio_thread_priority, rfd, open   (no engine crates)
```

The graph is acyclic by construction. `sue-core` and `sue-platform` are leaves
among the engine crates, so the data kernel never depends on an operating
system, and the OS layer never depends on the data model. `winit` (windows and
OS input), `cpal` (audio devices), `rfd` (native dialogs) and `open` (the
system browser and file manager), `arboard` (the clipboard), `egui-winit` (OS
input for egui) and `egui-wgpu` (egui's painter) are used only by
`sue-platform`. `egui` itself is a portable, immediate-mode widget library with
no OS bindings: `sue-wm` builds the whole shell with it, and the platform host
paints what it produces. `unsafe` code is forbidden workspace-wide
(`[workspace.lints] rust.unsafe_code = "forbid"`). The `wgpu` window surface is
created through its safe API, so `sue-platform` keeps the lint too. Native FFI
added to `sue-platform` later MUST relax it for that crate only.

---

## 1. Relational ECS / DAG kernel (`sue-core`)

### 1.1 Why there are no $O(N^2)$ converters

A conventional suite that supports $N$ file formats and wants any-to-any
conversion needs a converter for every ordered pair: $N(N-1) = O(N^2)$ code paths.
Each studio also keeps its own in-memory model, so moving work from the video
editor to the DAW is one more conversion.

Sue routes every format through **seven canonical typed buffer components**:

$$
\text{format}_i \xrightarrow{\ \text{decode}_i\ } \text{Component} \xrightarrow{\ \text{encode}_j\ } \text{format}_j
$$

Supporting $N$ formats takes at most $2N$ codecs, which is $O(N)$. A codec pack
adds one format to every studio at once. Studios never convert between
themselves. They read the same components through **projections** (§1.4).

### 1.2 The Arena and `AssetHandle`

All engine data lives in `sue_core::Arena`, a generational slot arena of
entities. An entity is addressed by an `AssetHandle`, a `#[repr(transparent)]`
`u64`:

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

**Concurrency model.** The target is a **lock-free shared arena**: slot pages are
allocated in fixed-size chunks that never move. Generations are atomics. Readers
validate a handle with one acquire-load and never block. Writers append to
per-thread command buffers that are merged at frame boundaries, and freed memory
is reclaimed epoch by epoch. v1.0 ships the single-writer form of the same
contract: mutation requires `&mut Arena`, and `Arena: Send + Sync` allows any
number of concurrent readers. Code written against the v1.0 API (handles, split
borrows, transactions) carries over unchanged.

### 1.3 Typed buffer components

Every entity has one **base component**, given at spawn, and at most one component
of each kind. Each kind is a dense, typed buffer:

| `ComponentKind` | Payload | Layout | Zero-copy `f32` tensor view |
| --- | --- | --- | --- |
| `StringBuffer` | UTF-8 text | `String` | n/a |
| `PCMFloat1D` | Interleaved audio | `sample_rate: u32`, `channels: u16`, `Vec<f32>` | samples |
| `VectorSpline2D` | Cubic Bézier chain | `Vec<[f32; 2]>` with `3n + 1` knots, `closed` | knots, flattened |
| `RasterTile2D` | Pixel tile | `width`, `height`, `PixelFormat::{Gray8, Rgba8, RgbaF32}` | `RgbaF32` pixels |
| `MeshBuffer3D` | Indexed triangles | `Vec<[f32; 3]>` positions, `Vec<u32>` indices | positions, flattened |
| `TimeSpan` | Interval on the global clock | `start`, `duration` in flicks (§1.6) | n/a |
| `ASTNode` | Syntax / scene-graph node | `kind`, byte span, `Vec<AssetHandle>` children | n/a |

Constructors validate their invariants (for example, sample count divisible by
channel count, or triangle indices in bounds), so a component that exists is
well-formed. `ComponentData::f32_view{,_mut}` exposes the numeric payload as a
borrowed slice, and that slice is the tensor interface of §6.

### 1.4 Entities, projections and the DAG

A **projection** is a way of viewing an entity in a studio. There are seven, one
per native component kind:

| `Projection` | Native component | Typical studio |
| --- | --- | --- |
| `Document` | `StringBuffer` | Document processor, notes |
| `Audio` | `PCMFloat1D` | DAW, media player |
| `Vector2D` | `VectorSpline2D` | Vector studio, CAD sketches |
| `Raster2D` | `RasterTile2D` | Raster studio, file thumbnails |
| `Mesh3D` | `MeshBuffer3D` | CAD / 3D, game engine |
| `Timeline` | `TimeSpan` | Video editor, animation |
| `Code` | `ASTNode` | IDE |

Each entity carries a `ProjectionMask`, a 16-bit set of the projections it
currently supports. Switching a document into the video editor is
`Arena::attach_projection(handle, Projection::Timeline, TimeSpan)`. This adds a
`TimeSpan` component and sets one bit. It **MUST NOT** copy, move or mutate any
existing buffer. The heap allocation behind the base component keeps its exact
address, and a unit test asserts this pointer stability. A 2D raster becomes a 3D
asset the same way, by attaching a `MeshBuffer3D` (for example a textured quad)
next to the untouched `RasterTile2D`.

`detach_projection` returns the removed component, so the operation can be
reversed (§1.5). The base projection cannot be detached
(`CoreError::CannotDetachBase`), so a projection can never destroy the data it
views.

Entities reference each other only through handles. `ASTNode::children` and
timeline tracks form a **directed acyclic graph** over the arena: a scene graph,
a syntax tree and an edit decision list all use the same storage and the same
undo machinery.

### 1.5 Delta-tree undo/redo

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
- `undo` reverts the cursor's transaction and moves to the parent.
- `redo` re-applies the **preferred child**: the branch most recently created or
  visited. `select_redo_branch` changes it.
- `checkout(node)` walks to any node: it undoes up to the lowest common ancestor,
  then redoes down to the target. The cost is the path length.

`UndoStack<T>` is generic. The shell keeps **one global history**,
`UndoStack<sue_wm::World>`, where `World { arena, rice }` holds everything
undoable. `ArenaEdit(txn)` and `RiceEdit(txn)` lift arena and rice transactions
onto the world, so a single `Ctrl+Z` (or `Ctrl+Shift+Z` / `Ctrl+Y` to redo) steps
back through note edits and theme edits in the order they happened.

### 1.6 Time base

`TimeSpan` counts **flicks**: $1\ \text{s} = 705{,}600{,}000$ flicks
(`FLICKS_PER_SECOND`). The number divides evenly by every common frame rate (24,
25, 30, 48, 50, 60, 90, 100, 120 fps) and sample rate (8 kHz to 192 kHz,
including 44.1 kHz). Audio samples and video frames therefore land on exact
integer ticks, with no drift.

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
| 120 FPS File Explorer and canvas thumbnails | Progressive decoding. The DC pass (1:8 scale) is decodable from the first bytes, so a thumbnail never waits for the full file. |
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
and AAC / M4A decode through `symphonia` into `PCMFloat1D`; text becomes a
`StringBuffer`; anything else, including media Sue has no decoder for yet,
becomes a `Gray8` byte-map raster. Each entity carries metadata (`bytes`,
`route`, `format`, size, decode time, colour and depth, codec and tags,
entropy, and the source path). For the converter, `encode_png` writes
compressed PNG and `encode_wav` writes 16-bit PCM WAV.

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
| `paths` | `PlatformPaths`: config, data, cache and home folders, plus derived `rice_dir`, `packages_dir`, `models_dir`, `notes_dir` |
| `create_surface`, `resize_surface`, `destroy_surface`, `surface` | Window / swap-chain surfaces with logical size, scale factor and safe-area insets |
| `poll_events` | Drains `InputEvent`s |

| Service | What it does |
| --- | --- |
| `AudioOutput` / `AudioSink` | A `cpal` output stream fed from a lock-free ring (§3.5); `NullAudio` is a device-free clock for tests |
| `list_dir`, `hidden_by_os`, `write_atomic` | Sorted folder listings for the Files drawer, without dot files or, on Windows, the legacy junctions (`NetHood`, `PrintHood`, `Cookies`, `SendTo`, `Recent`, `Start Menu`, `Templates`, `Application Data`, `Local Settings`, `My Documents`); crash-safe writes (temporary file, sync, rename) |
| `copy_text` | The system clipboard (`arboard`) for buttons such as Copy All; text fields copy and paste through `egui-winit` |
| `EguiPainter`, `TextureUpload` | Paints egui frames with `egui-wgpu` into the window or an offscreen texture; uploads image pixels the caller borrows from the arena straight into GPU textures (rule 5) |
| `open_url`, `reveal_folder` | The system browser (only `http` and `https`, validated by `web_url`) and file manager |
| `pick_open_file`, `pick_save_file` | Native open and save dialogs (`rfd`) |

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
shell called `request_repaint`, the caret's blink time while a text field has
focus, never otherwise).

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

| `FormFactor` | `Topology` | Input model | Geometry |
| --- | --- | --- | --- |
| `Desktop` | `Tiled` | Mouse + keyboard, keybinds from rice | Rails at both edges; the drawers and the timeline push the canvas aside |
| `Tablet` | `Overlay` | Stylus pressure curves ($p' = p^{\gamma}$), the radial marking menu | Same chrome, but drawers slide over the canvas so it stays large |
| `Mobile` | `Overlay` | Touch, edge swipes | Drawers capped at `layout.drawer_max_fraction` of the screen; the timeline is a thumb-zone sheet capped at `layout.sheet_max_fraction` |

Classification follows Android's resource-qualifier convention: on a mobile OS, a
smallest screen side of **at least 600 dp** is a tablet (`FormFactor::classify`).
Desktop operating systems are `Desktop`.

### 3.3 Layout solver

`solve_layout(&LayoutRequest) -> LayoutSolution` is a pure function. It names
every region in screen points: `left_rail`, `right_rail`, `status`, the visible
part of each drawer (`left_drawer`, `right_drawer`) and its full-width rect slid
by its reveal (`left_drawer_full`, `right_drawer_full`), the same pair for the
timeline, and the `canvas`.

```text
┌──────┬────────────┬──────────────────────────────────┬────────────┬──────┐
│ rail │ left       │          infinite canvas         │ right      │ rail │
│ 36px │ drawer     │  (top pill and selection pill    │ drawer     │ 36px │
│      │ 260 px x   │   float over it)                 │ 280 px x   │      │
│      │ reveal     ├──────────────────────────────────┤ reveal     │      │
│      │            │ timeline, 240 px x reveal        │            │      │
├──────┴────────────┴──────────────────────────────────┴────────────┴──────┤
└──────────────────────────────── status bar (22 px) ──────────────────────┘
```

- **Inputs:** viewport, safe-area insets, form factor, the three reveal
  fractions (spring positions, §3.4) and `LayoutMetrics` (rail, drawer,
  timeline and status bar sizes, minimum canvas width, drawer and sheet
  fractions), all read from rice.
- **Tiled (desktop):** drawers take `width x reveal` and push the canvas; the
  timeline takes `height x reveal` from the bottom of the canvas column. If the
  canvas would fall below `layout.min_canvas_width`, both drawers shrink evenly.
  Drawer content is laid out at full width and slid, never squeezed, so a
  drawer glides open instead of reflowing.
- **Overlay (tablet, phone):** the canvas keeps the whole middle; drawers and the
  timeline slide over it within their caps.
- **Invariants (tested):** closed drawers leave the canvas between the rails,
  half-open drawers push it by exactly half their width, and narrow windows
  shrink drawers before the canvas.

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

1. A spring is moving (a drawer, the timeline, the camera, a pill, the marking
   menu, the palette), a node is being dragged, or the timeline plays:
   `Animate`. The shell also calls `request_repaint`, so egui asks for the next
   frame too.
2. Within `motion.settle_grace_ms` (300 ms) of the last change: `Animate`.
3. Timed work is pending: `WakeAt(earliest)` for the next audio top-up while
   something plays (every 40 ms), a burst of typing to close into one undo step
   (`TYPING_PAUSE`, 900 ms), a status message to clear, or a pending note
   autosave.
4. Otherwise `Sleep`: 0 FPS and no CPU. A focused text field still blinks its
   caret through egui's own repaint delay (`motion.caret_blink_ms`; 0 keeps it
   solid and the window fully asleep).

Each frame measures real elapsed time, `dt = now - last_frame` clamped to
$[0.0001, 0.05]$ s; the first frame after sleeping advances by one nominal
frame. Springs see `dt x motion.animation_speed`. The self-check drives the
whole shell through real egui frames and asserts that the drawer glides over
many frames (and snaps in one on `LegacyLowSpec`), that a mode switch glides
the camera, and that afterwards neither the runtime nor egui asks for a frame.

**Spring physics.** Everything that moves uses a damped harmonic oscillator,
$F = -kx - cv$, so $m\ddot{x} = -k(x - x_\text{target}) - c\dot{x}$. The
damping ratio is $\zeta = c / (2\sqrt{km})$. Drawers, pills and the marking
menu use $k = 260$, $c = 32$ ($\zeta \approx 0.99$). The camera uses $k = 600$,
$c = 49$ ($\zeta \approx 1.0$): critically damped, it covers 98% of the way to
a framed entity in about 0.24 s (tested). `SpringDamper` integrates with
semi-implicit (symplectic) Euler in sub-steps of at most 1/240 s. A spring is
**settled** when $|x - x_\text{target}|$ and $|v|$ are both below $10^{-3}$
(the camera's pan settles within a twentieth of a screen pixel); it then snaps
exactly to the target. Under `LegacyLowSpec`, or with
`motion.instant_transitions`, `SpringMode::InstantSnap` skips integration:
transitions take 0 ms.

**The camera.** The canvas `Camera` is three springs: the canvas point at the
center of the view, and the natural log of the zoom (so zooming feels even at
10% and at 3200%). The wheel zooms by `canvas.zoom_step` per notch around the
pointer, which keeps showing the same canvas point for the whole glide. Dragging
snaps the pan springs, so the canvas sticks to the pointer. Framing an entity
sets all three targets; fitting never magnifies past 200% (1:1 and the wheel go
further).

**Painting.** egui tessellates every frame and `EguiPainter` draws it with
`egui-wgpu`. The canvas is an egui layer with a camera transform
(`set_transform_layer`), so entity nodes are ordinary egui widgets, text fields
included, at any zoom; hit testing and pointer positions follow the transform.
Images (pictures, paint layers) are uploaded from the arena's own bytes into
native textures by key and version: a paint layer's version is its `revision`
metadata, bumped by every stroke and by undo and redo. Textures no frame used
are freed.

**Telemetry.** The status bar shows the mode and the selected entity (or the
latest message) on the left; the arena entity count, the undo depth, the
hardware profile, the zoom and the frame state (`Idle` while asleep, `N FPS`
measured over the last second while animating) on the right.

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
reopens when its settings change. The shared `Transport` (used by the Player,
audio entities and the timeline) keeps decoded tracks in the arena and tops the
ring up every 40 ms from the next slice of the clip, resampled linearly to the
device rate; the ring is the device boundary, like a texture upload. A DAW graph
will run on the same ring discipline.

### 3.6 Applets, the Launchpad, the canvas and its chrome

Every tool, utility and studio implements `SueApplet`:

```rust
pub trait SueApplet: Send {
    fn manifest(&self) -> AppletManifest;   // id, name, role, icon, order, mode, spawn, lane
    fn rice_declarations(&self) -> Vec<RiceDeclaration> { Vec::new() }
    fn accepts(&self, entity: &Entity) -> bool { false }            // for opened files
    fn node_size(&self, entity: &Entity) -> Vec2;
    fn new_entity(&mut self, cx: &mut AppletContext<'_>) -> Option<NewEntity> { None }
    fn node_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {}
    fn pill_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {}
    fn duration(&self, node: &SceneNode, world: &World) -> f64 { 4.0 }
    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {}
    fn window_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {}
    fn tick(&mut self, cx: &mut AppletContext<'_>) -> Tick { Tick::default() }
    fn commands(&self) -> Vec<PaletteEntry> { Vec::new() }
    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool { false }
    fn navigate(&mut self, target: &str) -> bool { false }        // e.g. a Settings page
    fn captures_keys(&self) -> bool { false }                      // e.g. recording a shortcut
}
```

The **role** decides where an applet lives: `Node` applets draw one kind of
entity on the canvas and its selection pill, `LeftDrawer` and `RightDrawer`
applets sit on a rail, and the `Window` applet (Settings) floats over the
canvas. Nothing in the shell names an applet: the modes of the top pill come
from manifests' `mode`, the spawn buttons and the marking menu's New wedges from
`spawn`, the timeline's lanes from `lane`, the rail icons from `icon` and
`order`, and opened files go to the first node applet that `accepts` the
ingested entity. `Launchpad` is the runtime registry of factories; `Applets` is
the set of live instances, sorted by `order`.

Applets hold `AssetHandle`s and `NodeId`s, never media. They change the world
only through the global `UndoStack<World>` (`cx.apply`, `cx.edit_rice`, or
`cx.record` for an edit that already happened in place), and ask for things
through `cx.request(ShellRequest::…)`: select, focus or frame a node, switch
mode, spawn, open the timeline or Settings, open a file, run an engine command,
show a message.

**The scene.** `World` holds the arena, the live rice and the `Scene`: one
`SceneNode` per card on the canvas, holding an `AssetHandle`, the applet that
draws it, its rect in canvas units, its stacking order and a few numeric
properties (a document's `font_size`; an audio clip's `clip_start`, `clip_end`
and `gain`; `t0` and `dur` on the timeline). Several nodes may show one entity:
splitting audio places a second node over the same samples. `SpawnNode` (create
an entity and its node; redo restores the same entity under the same handle),
`PlaceNode`, `RemoveNode` (the entity stays in the arena), `SetNodeRect`,
`SetNodeProp` and `Batch` (several steps as one, atomic) are the scene's
transactions.

**The shell** (`Shell::ui`) draws, in order: the canvas (void fill, an adaptive
dot or line grid that keeps 14 to 112 points between marks, and the scene layer
with every visible node as a card: title bar with name and kind, a dot when it
is on the timeline, the applet's content, a resize grip), the rails, the
drawers, the timeline, the status bar, then the floating surfaces:

| Surface | Behavior |
| --- | --- |
| Top pill | Centered over the canvas. Modes (Canvas, the node applets' modes, and Timeline between the visual and code modes), spawn buttons, and `Command (Tab)`. Fades and slides in at boot. |
| Selection pill | Above the selected node's top edge, clamped into view, drawn by its applet's `pill_ui`. Springs in and out; when the selection moves to another node it glides from the old place. |
| Marking menu | Right-click: wedges whose radius grows on a spring (`layout.radial_radius`). Click a wedge, or press, drag and release. |
| Command palette | `Tab` or `/` when no text field has focus: fuzzy search (in-order letters, bonuses for runs and word starts) over shell actions, every mode, spawn and drawer, and the applets' `commands`. |
| Settings window | Over a scrim; 190 px category sidebar, filter field, one page at a time (§4.3). |
| Quit dialog | When `general.confirm_on_exit` is on. |

Interaction: the wheel zooms around the pointer, the middle button or Space
plus drag pans (a tap on Space plays or pauses the timeline instead), a click on
a node selects and raises it, a double-click on its title focuses it (its mode,
framed), dragging the title moves it and the grip resizes it (one undo step each
on release), a click on empty canvas deselects, `Delete` removes the selected
node from the canvas, and a file dropped anywhere is ingested and placed.

**Text and undo.** Every text surface is an `egui::TextEdit`. Documents, code
and the note edit the arena's `StringBuffer` in place through `ArenaText`, an
egui `TextBuffer` that records each insert and delete as a byte-offset delta.
`Typing` groups a burst (same entity, no pause of 900 ms) into one
`TextEdits` transaction and records it, already applied, in the global undo
tree. The shell consumes the riced undo and redo chords (`Ctrl+Z`,
`Ctrl+Shift+Z`, `Ctrl+Y`) before any widget sees them, closes the open burst,
and walks the global tree, so text fields never run egui's private undo.

**The timeline** is a bottom drawer on a spring. Its lanes are the distinct
`lane` names of the node applets; its blocks are the nodes whose entity carries
the `Timeline` projection, at `t0` for `dur` seconds (audio: the clip's length).
Project to Timeline is one undoable `Batch`: it attaches the projection (a
`TimeSpan` component next to the entity's own data, nothing copied), places the
block after the last one in its lane, and opens the timeline. Blocks drag along
their lane (one `SetNodeProp` on release); the ruler and the playhead seek;
play advances the playhead with real time and plays each audio block through the
shared `Transport` as the playhead crosses it.

The built-in applets are registered by the binary, like any package would
register its own:

| Applet | Role | Mode, spawn, lane | What it does |
| --- | --- | --- | --- |
| Document | Node | Document; `+ Text`, New Document; Document Track | Markdown-like text (`#`, `##`, `**`, backticks styled, never hidden) in a `TextEdit`; pill: H1, H2, Bold, Monospace, text size, Project to Timeline |
| Paint | Node | Paint; `+ Paint`, New Paint Layer; Paint/Visual Track | An RGBA raster in the arena; Brush, Line, Rect, Eraser, size, gray value; strokes keep 64 px undo tiles; Export .jxl (lossless) |
| Audio | Node | Audio (palette only); `+ Audio`, New Audio Node; Audio Track | Waveform of a clip of a `PCMFloat1D`; Play or Pause, Gain, Split (zero-copy), Normalize, Open in Timeline |
| Image | Node | Image (palette only); Paint/Visual Track | Decoded JPEG XL, PNG or JPEG; 1:1 Zoom, Pixel Grid, Re-encode .jxl (bit-exact for JPEG when a codec allows), Inspect Hex |
| Code / Hex | Node | Code / Hex | Code in a monospace `TextEdit`, or a selectable hex dump of any file's bytes; Copy All |
| Files | Left drawer | n/a | Folders with quick jumps (Project, Desktop, Documents, Downloads, Pictures, Music); files open on the canvas |
| Player | Left drawer | n/a | Every decoded track as a playlist; waveform scrubber, play, stop, volume |
| Convert | Left drawer | n/a | Pictures to lossless JPEG XL, PNG or JPEG (`io.jpeg_quality`), songs to 16-bit WAV |
| Notes | Left drawer | n/a | One shared note in a `TextEdit`, autosaved every `general.autosave_seconds` and on exit |
| Inspector | Right drawer | n/a | The selected node's place, size and properties, its entity's projections, memory and metadata |
| History | Right drawer | n/a | The undo tree, branches indented; a click checks out any state |
| Settings | Window | n/a | Preferences by category (§4.3) |

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

**Settings.** Preferences open as a floating window over the canvas (the
gear at the foot of the left rail, or `keys.open_settings`, `Ctrl+,`). A
category sidebar (`layout.settings_sidebar_width`, 190 px) lists the pages; the
right pane shows only the chosen page, under a "Filter settings..." field.
While a filter is active, matching parameters from every page are listed.

The pages are data the engine registers in its `Catalog`, like applets, so
packages can add their own:

| Page | Holds |
| --- | --- |
| General | The boot mode (Canvas or any node applet's mode), the Files folder, hidden files, autosave interval, quit confirmation, the note file and its text size |
| Appearance & Rice | Theme presets, every color, geometry (padding, hairlines, radii, pill and rail sizes, drawer and timeline sizes, the marking menu, shadows), the canvas (zoom limits, wheel step, grid), typography, the rice file with Reload, Export and Open Folder |
| Display & Graphics | The hardware profile, the active profile and GPU, the GPU preference, the frame-rate cap, the settle grace, UI scale, list wheel speed, motion (instant snap, animation speed, drawer and camera spring constants, caret blink, message time) |
| Audio & DSP | Output device, sample rate, buffer size, real-time priority, volume, and the live stream and real-time status |
| Files & Codecs | JPEG XL effort, bit-exact JPEG recompression, the conversion folder and JPEG quality, the open-file size limit, byte-map width, new paint layers' size and brush, the built-in codecs |
| Local AI | The compute backend (backends this machine lacks are greyed out), the VRAM budget, which model and backend a task would use now, the model folder and the model files found in it |
| Packages & Updates | The signature policy, the installed applets, installed packages, the active generation and rollback state |
| Keymaps | Every shortcut as an action and a key cap |

Any rice section no page claims is listed on a final "Other" page, so every
parameter stays reachable (tested).

| Parameter | Control |
| --- | --- |
| `Bool` | A black and white switch |
| `IntRange` / `FloatRange` | Slider with a 4 px track, a filled part and a round thumb, its value on the right |
| `ColorHex` | Swatch plus hex field |
| `EnumChoice` | A segmented control for up to five short options, else a compact drop-down |
| A text parameter with live choices (output device, boot mode) | A drop-down |
| `Keybind` | A key cap; click it and press the new chord (`Esc` cancels; global shortcuts are held back while recording) |
| `ShaderPath` / `Text` | Text field |

Text fields commit when they lose focus. A slider drag previews
live: it sets the schema directly, so the whole UI follows the value as you
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
(`# sue-rice format 3`). Each released default is frozen in `rice/history/`
(`v1`, `v2`). At boot, `migrate` brings an older file up to date: a value still
equal to the old default takes the new default (so the purple and gold palette
of format 1 and the blue `#3b82f6` accent of format 2 become the monochrome
palette), a value the user changed is carried over when its key still exists
and the value passes the new checks, and the rest is reported. Any color that
is not a gray is reset to its default even in a current file (`purged`), so a
tinted color never survives a start. The old file is kept as
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

## 7. Implementation status (v1.0 "Faustian Enigma")

| Spec | Status in v1.0 | Where |
| --- | --- | --- |
| §1.2 Arena, handles, split borrows | **Implemented** (single-writer; lock-free arena planned) | `sue-core/src/arena.rs`, `handle.rs` |
| §1.3 Seven typed components | **Implemented** | `sue-core/src/component.rs` |
| §1.4 Projections, mask, pointer stability | **Implemented** | `sue-core/src/projection.rs`, `arena.rs` |
| §1.5 Delta-tree undo, `checkout` | **Implemented** | `sue-core/src/undo.rs`, `txn.rs` |
| §1.5 One global history for arena and rice edits | **Implemented** (`UndoStack<World>`) | `sue-wm/src/world.rs` |
| §2.1 Three-tier sniffer | **Implemented** | `sue-io/src/sniff.rs` |
| §2.2 JXL probe (boxes + `SizeHeader`) | **Implemented** | `sue-io/src/jxl.rs` |
| §2.2 JXL pixel decode and JPEG reconstruction | **Implemented** with `jxl-oxide` (no CMYK yet; the DC preview and pass targets come from a full decode until a streaming decoder lands) | `sue-io/src/jxl.rs` |
| §2.2 Lossless JXL encode | **Implemented** with `zune-jpegxl` (one `Gray8`, `Rgba8` or `RgbaF32` layer; multi-layer planned) | `sue-io/src/jxl.rs` |
| §2.2 Lossless JPEG → JXL recompression | Planned; returns `CodecUnavailable` | `sue-io/src/jxl.rs` |
| §2.2 PNG and JPEG decode, PNG encode | **Implemented** with `zune-png` and `zune-jpeg` | `sue-io/src/raster.rs` |
| §2.2 Audio decode (MP3, FLAC, WAV, Ogg Vorbis, AAC) and WAV encode | **Implemented** with `symphonia`; WAV is written as 16-bit PCM | `sue-io/src/audio.rs` |
| §2.2 Ingest of opened and dropped files (sniff, then raster, audio, text or bytes entity) | **Implemented** (decoding runs on the UI thread; spawning the entity is not yet an undoable transaction) | `sue-io/src/ingest.rs`, `bins/sue/src/runtime.rs` |
| §2.3 Tile cache, byte map | **Implemented** | `sue-io/src/tile_cache.rs`, `bytemap.rs` |
| §3.1 `PlatformHost`, paths, `HeadlessHost` | **Implemented** | `sue-platform` |
| §3.1 Native desktop host (`winit` window, `wgpu` surface, egui input, IME, clipboard, file drops) | **Implemented** (one window; tested on Windows) | `sue-platform/src/desktop.rs`, `gpu.rs`, `paint.rs` |
| §3.1 GPU choice (`display.gpu_preference`: auto, integrated, discrete) | **Implemented**; `auto` draws on the display GPU, which a device test checks on Windows | `sue-platform/src/gpu.rs`, `bins/sue/src/desktop.rs` |
| §3.1 Frame pacing: application and egui repaint requests combined in `about_to_wait`, timed wakes, close confirmation | **Implemented**; the self-check asserts that neither the runtime nor egui asks for frames once idle | `sue-platform/src/desktop.rs`, `bins/sue/src/runtime.rs` |
| §3.1 Native dialogs, system browser, file manager | **Implemented** with `rfd` and `open` (web addresses limited to `http` and `https`) | `sue-platform/src/os.rs` |
| §3.1 Native mobile hosts (Android, iOS) | Planned (the mobile topologies already run on `HeadlessHost`) | n/a |
| §3.2 to 3.4 Topologies, layout, springs, spring camera, scheduler | **Implemented** | `sue-wm` |
| §3.4 Painting with egui (`egui-wgpu`), arena images as native textures | **Implemented**; the self-check renders a frame on the GPU and counts tinted pixels (zero) | `sue-platform/src/paint.rs` |
| §3.4 Multi-pass shader effects | Planned (the `ShaderPath` parameter type exists; no shipped parameter uses it yet) | n/a |
| §3.5 Audio output (ring, atomic gain, real-time priority) and the shared transport | **Implemented** with `cpal`; the transport is tested headlessly with `NullAudio`, and a device test streams to the real output | `sue-platform/src/audio.rs`, `sue-wm/src/transport.rs` |
| §3.5 DAW audio graph | Specified; planned with the DAW studio | n/a |
| §3.6 Applets, Launchpad, infinite canvas, top and selection pills, marking menu, palette, drawers, status bar | **Implemented** (twelve built-in applets, see §3.6) | `sue-wm`, `bins/sue/src` |
| §3.6 Text editing over the arena with one global undo (`ArenaText`, `Typing`) | **Implemented**; the self-check types, undoes with `Ctrl+Z` and redoes with `Ctrl+Y` through egui events | `sue-wm/src/text.rs`, `sue-core/src/txn.rs` |
| §3.6 Timeline: lanes, projected blocks, playhead, playback of audio blocks | **Implemented** (blocks are moved, not trimmed; video lanes arrive with the video studio) | `sue-wm/src/timeline.rs`, `shell.rs` |
| §3.6 Studios beyond the built-in entity kinds (DAW, video, 3D and CAD, IDE, game engine) | Planned; they arrive as packages that register node applets | n/a |
| §3.6 Embedded web view | Planned (the Browser drawer of earlier builds was retired with the canvas) | n/a |
| §3.6 A bold typeface for documents | Planned; the bundled fonts have no bold weight, so bold spans show in full white with wider spacing | `bins/sue/src/document.rs` |
| §4 Rice parser, GUI descriptors, write-back, profiles | **Implemented** | `sue-rice` |
| §4.3 Settings window: categories, filter, segmented, slider, switch, color and keycap controls, presets, undoable edits, atomic persistence | **Implemented** (the file is read at boot and by Reload from Disk; watching it for outside edits is planned) | `bins/sue/src/settings.rs`, `runtime.rs`, `engine.rs` |
| §4.4 Rice format marker, migration from formats 1 and 2, tinted colors reset | **Implemented**, with a backup of the old file | `sue-rice/src/migrate.rs`, `bins/sue/src/engine.rs` |
| §5 Hashes, Ed25519, resolver, generations, atomic pointer | **Implemented** | `sue-pkg` |
| §5 Package download, binary deltas, WASM plugin loading | Planned (the signature policy is riced and shown; nothing installs packages yet) | n/a |
| §6 Formats, backends, registry, zero-copy tasks | **Implemented** (reference kernel on `CpuSimd`) | `sue-ai` |
| §6 GGUF/ONNX inference runtimes | Planned (the Local AI page lists model files and the backend a task would use) | n/a |
| House style: no em dashes, en dashes or middle dots | **Checked** by a test that scans every text file | `bins/sue/src/style.rs` |
