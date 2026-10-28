# HUMANZ — Sue Systems Specification

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

This is the architectural specification of **Sue: Singular Unified Engine**, v1.0
"Faustian Enigma". It is written for systems engineers. The plain-English
companion is [`HUMANS.md`](HUMANS.md); the rules every AI coding agent must follow
are in [`AGENTS.md`](AGENTS.md).

The words **MUST**, **MUST NOT**, **SHOULD** and **MAY** are used as in RFC 2119.
Type names in `code` refer to real items in the workspace. §7 maps every part of
this specification to its implementation status.

---

## 0. Design tenets

| # | Tenet | Enforced by |
| --- | --- | --- |
| T1 | **One data model.** Every asset is an entity in one arena, made of typed buffer components. | `sue-core` |
| T2 | **Zero-copy.** Subsystems exchange 64-bit `AssetHandle`s, never buffers. | `sue-core`, LAW 2 |
| T3 | **Everything is reversible.** Every state mutation is a `ReversibleTransaction` storing a delta. | `sue-core`, LAW 6 |
| T4 | **Everything is riceable.** Every visual constant, keybind and curve is a `.suerice` parameter. | `sue-rice`, LAW 3 |
| T5 | **Platform isolation.** OS-specific code exists only in `sue-platform`. | LAW 5 |
| T6 | **Hardware empathy.** Static scenes cost 0 frames. Budgets follow `HardwareProfile`. | `sue-wm`, `sue-rice`, LAW 4 |
| T7 | **Offline first.** AI, codecs and updates work with no network. | `sue-ai`, `sue-pkg` |
| T8 | **Infinite modularity.** Tools are `SueApplet`s registered at runtime; nothing lists apps statically. | `sue-wm`, LAW 1 |

### 0.1 Crate graph

```text
bins/sue      ──► every crate below
sue-wm        ──► sue-rice, sue-platform, sue-core
sue-ai        ──► sue-platform, sue-core
sue-rice      ──► sue-platform, sue-core
sue-io        ──► sue-core
sue-pkg       ──► sha2, ed25519-dalek, semver   (no engine crates)
sue-core      ──► nothing                        (leaf)
sue-platform  ──► nothing                        (leaf)
```

The graph is acyclic by construction. `sue-core` and `sue-platform` are leaves, so
the data kernel never depends on an operating system, and the OS layer never
depends on the data model. `unsafe` code is forbidden workspace-wide
(`[workspace.lints] rust.unsafe_code = "forbid"`). Native FFI added to
`sue-platform` later MUST relax that lint for that crate only.

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
| `StringBuffer` | UTF-8 text | `String` | — |
| `PCMFloat1D` | Interleaved audio | `sample_rate: u32`, `channels: u16`, `Vec<f32>` | samples |
| `VectorSpline2D` | Cubic Bézier chain | `Vec<[f32; 2]>` with `3n + 1` knots, `closed` | knots, flattened |
| `RasterTile2D` | Pixel tile | `width`, `height`, `PixelFormat::{Gray8, Rgba8, RgbaF32}` | `RgbaF32` pixels |
| `MeshBuffer3D` | Indexed triangles | `Vec<[f32; 3]>` positions, `Vec<u32>` indices | positions, flattened |
| `TimeSpan` | Interval on the global clock | `start`, `duration` in flicks (§1.6) | — |
| `ASTNode` | Syntax / scene-graph node | `kind`, byte span, `Vec<AssetHandle>` children | — |

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

`UndoStack<T>` is generic. The same engine drives arena edits
(`UndoStack<Arena>`) and Settings edits (`UndoStack<RiceSchema>` with
`SetRiceValue`).

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

JPEG XL is Sue's **default internal image format**. LAW 6 requires every internal
image or texture cache to default to `.jxl`.

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
| `decode_progressive(bytes, ProgressiveTarget)` | Decode to `DcPreview`, a given pass, or `Full`, into a `RasterTile2D` |
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

The `ContainerOnlyPipeline` shipped in v1.0 implements `probe` completely. Its
pixel operations return `JxlError::CodecUnavailable` until a codec package
(`sue.codec.jxl`) is installed through `sue-pkg`. `reconstruct_jpeg` first checks
for a `jbrd` box and reports `NoJpegReconstruction` when there is none.

### 2.3 Tile cache

`TileCache` keeps encoded tiles in memory under a byte budget, keyed by
`TileKey { layer, level, x, y }`, where `level` is a mip or progressive level.
Eviction is least-recently-used, ordered by a monotonic tick in a `BTreeMap`, so
`get`, `insert` and eviction cost $O(\log n)$. A tile larger than the whole
budget is refused rather than flushing the cache. `invalidate_layer` drops every
tile of an edited layer. Hits, misses and evictions are counted for the
diagnostics overlay. The cache stores opaque bytes, and by LAW 6 those bytes are
`.jxl`-encoded tiles.

---

## 3. Adaptive cross-platform window manager (`sue-wm` & `sue-platform`)

### 3.1 Platform isolation

`sue-platform` is the **only** crate allowed to touch OS-specific APIs
(`windows-sys`, `libc`, `jni`, `objc`, …). Everything above it talks to the
`PlatformHost` trait:

| `PlatformHost` method | Abstracts |
| --- | --- |
| `os`, `form_factor` | `OsFamily::{Windows, Linux, MacOs, Android, Ios, Other}`, `FormFactor` |
| `capabilities` | `HardwareCapabilities`: logical cores, refresh rate, graphics API, GPU tier and vendor, memory, power source, touch, stylus |
| `paths` | `PlatformPaths`: config, data and cache roots, plus derived `rice_dir`, `packages_dir`, `models_dir` |
| `create_surface`, `resize_surface`, `destroy_surface`, `surface` | Window / swap-chain surfaces with logical size, scale factor and safe-area insets |
| `poll_events` | Drains `InputEvent`s |

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

`HeadlessHost` is a complete, real `PlatformHost` with virtual surfaces and an
injectable input queue. CI, the self-check and remote scripting use it, and native
windowed hosts implement the same trait.

### 3.2 Form-factor topologies

| `FormFactor` | `Topology` | Input model | Geometry |
| --- | --- | --- | --- |
| `Desktop` | `DesktopTiling` | Mouse + keyboard, keybinds from rice | Docks tile beside Center Stage; timeline tiles beneath it |
| `Tablet` | `TabletStylus` | Stylus pressure curves ($p' = p^{\gamma}$), radial marking menus | Docks become overlay drawers so the canvas is as large as possible; timeline stays tiled |
| `Mobile` | `MobilePocket` | Touch gestures, edge swipes | Single-pane stack inside the safe area; docks are edge drawers; timeline is a thumb-zone bottom sheet |

Classification follows Android's resource-qualifier convention: on a mobile OS, a
smallest screen side of **≥ 600 dp** is a tablet (`FormFactor::classify`).
Desktop operating systems are `Desktop`.

### 3.3 Layout solver

`solve_layout(&LayoutRequest) -> LayoutSolution` is a pure function. It maps the
four zones (`LeftDock`, `CenterStage`, `RightDock`, `BottomTimeline`) to
`Placement::{Hidden, Tiled(Rect), Drawer { edge, rect, reveal }, BottomSheet { rect, detent }}`.

- **Inputs:** viewport, safe-area insets, form factor, dock reveal fractions
  (driven by springs, §3.4), whether a time-based asset is active, and
  `LayoutMetrics` (gap, dock widths, timeline height, minimum center width).
  The metrics are read from rice.
- **Desktop:** the content rect is the viewport inset by the safe area and outer
  gap. Docks take `width × reveal` from each side. Center Stage fills the rest,
  and the timeline takes the bottom of the center column only when
  `timeline_active`. If Center Stage would fall below `min_center_width`, the
  right dock (then the left dock) degrades to an overlay drawer instead of
  crushing the workspace.
- **Mobile:** Center Stage is exactly the safe-area rect. A drawer's width is
  `min(dock_width, drawer_max_fraction × safe_width)`, and it slides in from its
  edge by its reveal fraction. The timeline becomes a `BottomSheet` at the `Peek`
  detent, capped at `sheet_max_fraction` of the safe height (the thumb zone).
  Both fractions are riceable (`layout.drawer_max_fraction`, default 0.85;
  `layout.sheet_max_fraction`, default 0.4).
- **Invariants (tested):** tiled rects never overlap, tiled widths plus gaps sum
  to the content width, and nothing is placed outside the safe area.

### 3.4 Hybrid render scheduler

Sue redraws **only when something changed**:

```rust
pub enum RenderDemand { Sleep, ActiveFrame { target_hz: u32 } }
```

`FrameScheduler` collects damage rectangles (merged by union) and an "animating"
flag. With no damage and no running animation it reports `Sleep`. The host then
blocks in its event wait: 0 FPS and roughly 0% CPU/GPU. Otherwise
`begin_frame` hands out the merged damage rect and a frame-rate target from the
active `HardwareProfile`:

| `HardwareProfile` | Frame cap | Shader passes | Spring physics | Blur | Graphics path |
| --- | --- | --- | --- | --- | --- |
| `HighRefresh` | display rate (240 Hz+) | 8 | yes | yes | Vulkan / DX12 / Metal, multi-pass |
| `Balanced` | 60 | 4 | yes | yes | Vulkan / DX12 / Metal |
| `LegacyLowSpec` | 30 | 0 | **no: instant 0 ms snap** | no | OpenGL / GLES |
| `MobileBatterySaver` | 30 | 1 | yes | no | GLES / Vulkan / Metal |

`HardwareProfile::recommend` picks a profile from `HardwareCapabilities`: low-power
mode → `MobileBatterySaver`; software/legacy GPU or ≤ 2 cores → `LegacyLowSpec`;
≥ 120 Hz with a high-end GPU → `HighRefresh`; otherwise `Balanced`. The user can
override it with `profile.hardware` in rice.

**Spring physics.** Panel motion uses a damped harmonic oscillator,
$F = -kx - cv$, so $m\ddot{x} = -k(x - x_\text{target}) - c\dot{x}$. The
damping ratio is $\zeta = c / (2\sqrt{km})$. The shipped defaults ($k = 170$,
$c = 26$, $m = 1$) give $\zeta \approx 1.0$: critically damped, fast and with no
overshoot. `SpringDamper` integrates with semi-implicit (symplectic) Euler in
sub-steps of at most 1/240 s. That is stable for every riceable stiffness
($\omega\,\Delta t \ll 2$), and frame time is clamped to 0.25 s so a long stall
does not cause a burst. A spring is **settled** when $|x - x_\text{target}|$ and
$|v|$ are both below $10^{-3}$. It then snaps exactly to the target and
reports `Sleep`. Under `LegacyLowSpec`, or when the user turns animations off,
`SpringMode::InstantSnap` skips integration entirely: transitions take 0 ms.

### 3.5 Real-time audio thread isolation

The DAW audio graph runs on a dedicated real-time thread that **MUST NOT**
allocate, lock, log or make syscalls inside the render callback. It talks to the
rest of the engine only through wait-free single-producer/single-consumer ring
buffers:

```text
UI / arena ──(SPSC: parameter & transport commands)──► audio thread
audio thread ──(SPSC: meters, playhead position)──► UI
```

All buffers the graph needs are preallocated when the graph is compiled. Graph
changes are prepared off-thread and swapped in at a buffer boundary. GPU frame
timing therefore never causes an audio dropout, and an audio overload never
stalls the UI.

### 3.6 Applets, the Launchpad and pin-to-boot

Every tool, utility and studio implements `SueApplet` (LAW 1):

```rust
pub trait SueApplet: Send {
    fn manifest(&self) -> AppletManifest;
    fn rice_declarations(&self) -> Vec<RiceDeclaration> { Vec::new() }
    fn mount(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), AppletError>;
    fn handle_input(&mut self, event: &InputEvent, ctx: &mut AppletContext<'_>)
        -> Result<InputResponse, AppletError>;
    fn update(&mut self, dt: f32, ctx: &mut AppletContext<'_>) -> RenderDemand { RenderDemand::Sleep }
    fn unmount(&mut self, ctx: &mut AppletContext<'_>) {}
}
```

Applets hold `AssetHandle`s, not data, and change the arena only through
transactions on the shared `UndoStack<Arena>` in `AppletContext`. The
`Launchpad` is a runtime registry of applet factories. It knows nothing about
specific applets. `hub()` groups the installed applets into Studios and
Utilities, `pin_boot` stores the `pinned_boot_applet`, and `boot_target()`
resolves it at startup. A pinned applet that has been uninstalled falls back to
the Hub instead of failing. `Workspace` owns the tabs (each one the Hub or a
mounted applet), the dock springs, the global keymap (dock toggles, undo, redo,
new tab, all read from rice) and the `FrameScheduler`.

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
| `true`/`false` | — | `Bool` | `Toggle` |
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

- **File → GUI:** `RiceSchema::parse` builds the schema, and
  `SettingsTree::build(&schema)` turns it into sections and widgets. Labels come
  from keys (`border_width` → "Border width"), and tooltips come from the
  descriptions. Each widget's `binding` is its dotted path.
- **GUI → file:** a widget edit becomes `SetRiceValue { path, value }`, a
  `ReversibleTransaction<RiceSchema>`, so Settings changes are undoable.
  `RiceSchema::set` type-checks and range-checks the value (an int is accepted
  for a float), then bumps `revision()`. `SettingsTree::is_stale` compares
  revisions.
- **Format-preserving write-back:** `to_suerice()` reproduces every unchanged line
  byte for byte, comments and spacing included. For a changed key it splices
  only the new value into the original value span. Defaults registered at runtime
  are inserted at the end of their section, or in a new section.

### 4.4 Registration (LAW 3)

Subsystems and applets declare their riceables as `RiceDeclaration { path,
default, description }`. `RiceSchema::register` inserts a missing key, or keeps the
user's value when its type is compatible (re-clamping it to the declared range).
An incompatible user value is replaced and reported. `sue_wm::register_rice`
declares every window-manager constant, and a test checks that the shipped
`default.suerice` matches those declarations exactly.

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
| `Gguf` | magic `GGUF` | version (1–3), tensor count, metadata key/value count (32-bit counts in v1, 64-bit after) |
| `Safetensors` | 8-byte LE header length (≤ 100 MB) followed by `{` | header length |
| `Onnx` | `.onnx` extension (protobuf has no magic) | — |
| `Magic` | `.magic` extension: Sue's native serialized compute graph | — |
| `Builtin` | compiled into the engine | — |

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
| §2.1 Three-tier sniffer | **Implemented** | `sue-io/src/sniff.rs` |
| §2.2 JXL probe (boxes + `SizeHeader`) | **Implemented** | `sue-io/src/jxl.rs` |
| §2.2 JXL pixel decode / encode / JPEG transcode | Trait defined; returns `CodecUnavailable` until a codec package exists | `sue-io/src/jxl.rs` |
| §2.3 Tile cache, byte map | **Implemented** | `sue-io/src/tile_cache.rs`, `bytemap.rs` |
| §3.1 `PlatformHost`, paths, `HeadlessHost` | **Implemented** (native windowed hosts planned) | `sue-platform` |
| §3.2–3.4 Topologies, layout, springs, scheduler | **Implemented** | `sue-wm` |
| §3.4 GPU renderer backends | Planned | — |
| §3.5 Real-time audio thread | Specified; planned with the DAW | — |
| §3.6 Applets, Launchpad, pin-to-boot, workspace | **Implemented** (reference applet: Quick Notes) | `sue-wm`, `bins/sue/src/quick_notes.rs` |
| §4 Rice parser, GUI descriptors, write-back, profiles | **Implemented** | `sue-rice` |
| §5 Hashes, Ed25519, resolver, generations, atomic pointer | **Implemented** | `sue-pkg` |
| §5 Package download, binary deltas, WASM plugin loading | Planned | — |
| §6 Formats, backends, registry, zero-copy tasks | **Implemented** (reference kernel on `CpuSimd`) | `sue-ai` |
| §6 GGUF/ONNX inference runtimes | Planned | — |
