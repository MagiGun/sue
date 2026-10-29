# Sue v1.0 "Faustian Enigma"

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

![Version](https://img.shields.io/badge/version-1.0%20Faustian%20Enigma-111111)
![Rust](https://img.shields.io/badge/rust-2024%20edition-333333?logo=rust)
![MSRV](https://img.shields.io/badge/MSRV-1.95-555555)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android%20%7C%20iOS-333333)
![Kernel](https://img.shields.io/badge/kernel-relational%20ECS%20%2F%20DAG-111111)
![Images](https://img.shields.io/badge/images-JPEG%20XL%20first-333333)
![AI](https://img.shields.io/badge/AI-100%25%20offline-444444)
![unsafe](https://img.shields.io/badge/unsafe-forbidden-111111)

**Sue, the Singular Unified Engine,** is one lightweight, endlessly expandable
creative environment. Day to day it is a fast file explorer, media player, notes
app and converter. One click turns it into a document processor, the **Sue
Paint** illustration studio, audio editor or code and hex viewer, and later into
a DAW, video editor, CAD/3D modeler, IDE or game engine.

Sue does this without a pile of separate apps or an N×N grid of file converters.
Every tool works on the same data in one shared, generation-indexed **Arena**.
Putting a document on the timeline, or a song next to a painting, attaches a new
*projection* to the data that is already there. Nothing is copied or converted.

## Where things stand

v1.0 "Faustian Enigma" contains the eleven crates below and the `sue`
executable. It opens a native GPU window drawn with egui in a pure monochrome
theme (black to white, nothing else), straight onto an **infinite canvas**:

- **The Hub:** Sue opens with a card for each place to work (the Canvas and
  every studio); `Esc` stays on the canvas.
- **The canvas:** a live dot grid you zoom from 10% to 3200% with the wheel and
  pan with the middle button or Space and drag, with the Welcome document on
  it to type and select text in.
- **The top pill:** Hub and the command palette, nothing else.
- **Sue Paint:** a full-window painting studio with the usual menu bar (File,
  Edit, Story, Animation, Layer, Select, View, Filter, Window, Help, every
  item working), a command bar, tools and sub tools (pens, pencils,
  brushes, airbrushes, erasers, fill, gradient, figures, selections,
  eyedropper, hand, zoom, move layer) with pen pressure and stabilization, a
  color wheel and sets, canvas tabs with a turnable view, a navigator, and
  layers with 26 blend modes, clipping and locks. Pictures live in the project
  and also sit on the canvas as cards. Going in, the canvas zooms into the
  card and the panels slide in; going out, they slide away and the canvas
  zooms back out.
- **The selection pill:** select an entity and a floating toolbar springs up
  above it with only its tools: headings, bold and text size for documents;
  brush, line, rectangle, eraser, size, value and JPEG XL export for paint;
  play, gain, split and normalize for audio; 1:1 zoom, pixel grid, re-encode and
  hex for pictures.
- **The marking menu:** right-click anywhere for a radial menu that grows on a
  spring. Its New File wedge opens a wheel of kinds of files (Document,
  Picture, Audio, Code) and each kind a wheel of exact kinds, from the
  applets' templates. **The command palette:** `Tab` or `/` for a fuzzy search
  over every action and template.
- **The timeline:** a bottom dock that springs open with Document, Paint/Visual
  and Audio tracks. Its blocks are the very entities on the canvas.
- **Drawers:** Files, Player, Convert and Notes on the left; Inspector and
  History (the clickable undo tree) on the right. Each slides on a spring.
- **Settings:** a floating window (gear or `Ctrl+,`) with eight categories, a
  filter box and a live control for every rice parameter.

Every text field selects with the mouse, double-clicks words, copies, cuts and
pastes, and `Ctrl+Z` and `Ctrl+Y` undo through the one global undo tree. Drop
any file on the window and Sue puts it on the canvas by its content. `Ctrl+S`
saves the whole canvas as a `.sue` project; opening a project brings it back.

Under the shell sits the **backbone** every future feature will share
(`HUMANZ.md` §1): stable entity ids, typed and ordered relations, open and
versioned component types that survive round trips through builds that do not
know them, one value, unit, color and time vocabulary, generic undoable edits,
the `.sue` container, one registry each for file formats and operators,
presets, a background job pool, and shared data models for rich text, tables,
animation curves, timelines, note events, node graphs, geometry and page
layout.

The large studios (DAW, video, 3D and CAD, IDE, the Noelle game engine) and
the AI model runtimes come in later releases. `HUMANZ.md` §7 lists, part by
part, what is implemented today and what is only specified, and §8 maps every
feature Sue may grow into onto the backbone.

## Crate map

| Crate | Role | Key types |
| --- | --- | --- |
| [`sue-core`](crates/sue-core) | The backbone: arena, stable ids, entities, open components, relations, values, units, color, time, undo tree, `.sue` file, format and operator registries, presets, jobs | `Arena`, `Entity`, `EntityId`, `ComponentRegistry`, `RelationStore`, `Value`, `Quantity`, `Color`, `Time`, `UndoStack`, `SueFile`, `FormatRegistry`, `OperatorRegistry`, `JobPool` |
| [`sue-model`](crates/sue-model) | Shared data models every feature can read | `TextMarks`, `Table`, `Animation`, `Sequence`, `Notes`, `NodeGraph`, `Geometry`, `Transform`, `PageLayout`, `picture`, `BlendMode`, `Encoded` |
| [`sue-platform`](crates/sue-platform) | The only place OS-specific code may live: hosts, the egui window and frame pacing, GPU choice, painting, audio output, clipboard, dialogs | `PlatformHost`, `DesktopHost`, `run_desktop`, `FramePacing`, `EguiPainter`, `TextureUpload`, `AudioOutput` |
| [`sue-rice`](crates/sue-rice) | `.suerice` parser, Settings descriptors, monochrome presets, migration, hardware profiles | `RiceSchema`, `RiceParameter`, `SetRiceValues`, `ThemePreset`, `migrate`, `HardwareProfile` |
| [`sue-io`](crates/sue-io) | Magic-byte sniffer, JPEG XL, PNG, JPEG and audio codecs, every format handler, ingest, tile cache, byte map | `sniff_stream`, `formats`, `ingest_bytes`, `NativeJxlPipeline`, `decode_audio`, `encode_jpeg`, `TileCache` |
| [`sue-pkg`](crates/sue-pkg) | Content-addressed packages, Ed25519 trust, resolver, atomic generations | `PackageManifest`, `ContentHash`, `TrustStore`, `PackageIndex`, `resolve`, `AtomicUpdater` |
| [`sue-ai`](crates/sue-ai) | Offline, zero-copy AI substrate | `LocalModelRegistry`, `ComputeBackend`, `ZeroCopyTensorTask` |
| [`sue-wm`](crates/sue-wm) | The shell: infinite canvas, spring camera, pills, marking menu, palette, timeline, drawers, studios, menu bars, theme | `SueApplet`, `Launchpad`, `Shell`, `MenuBar`, `Camera`, `Scene`, `SpringDamper`, `Typing`, `Transport` |
| [`sue-paint`](crates/sue-paint) | Sue Paint: the painting studio, its brush engine, compositing, selections, fills and filters | `SuePaint`, `PictureCard`, `Stroke`, `Compositor`, `Selection`, `Filter` |
| [`sue-type`](crates/sue-type) | Typography for every studio that sets text: paragraph styles, the rich text editor, page layout and typesetting | `Editor`, `StyleSheet`, `PageSetup` |
| [`sue-writer`](crates/sue-writer) | Sue Writer: the writing and publishing studio | `SueWriter`, `WriterCard` |
| [`bins/sue`](bins/sue) | The executable: runtime, built-in applets, self-check, snapshots | `Engine`, `Runtime` |

```text
bins/sue      ──► every crate below                   + egui
sue-paint     ──► sue-wm, sue-io, sue-model, sue-rice, sue-platform, sue-core   + egui
sue-wm        ──► sue-rice, sue-platform, sue-core   + egui
sue-ai        ──► sue-platform, sue-core
sue-rice      ──► sue-platform, sue-core
sue-io        ──► sue-model, sue-core                 + jxl-oxide, zune-jpegxl, zune-png, zune-jpeg, jpeg-encoder, symphonia, image, flacenc
sue-model     ──► sue-core
sue-pkg       ──► sha2, ed25519-dalek, semver         (no engine crates)
sue-core      ──► nothing                              (leaf)
sue-platform  ──► winit, wgpu, egui, egui-winit, egui-wgpu, arboard, pollster, cpal, audio_thread_priority, rfd, open   (no engine crates)
```

## Quick start

```bash
cargo run -p sue
```

```bash
cargo run -p sue -- --self-check
```

```bash
cargo run -p sue -- --snapshot target/snapshots
```

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

`cargo run -p sue` opens the window, titled `Sue`. It needs a graphics driver
with Vulkan, DirectX 12, Metal or OpenGL ES; playing audio needs an output
device. Your settings live in `default.suerice` in the `rice` folder of your
config directory (`%APPDATA%\Sue` on Windows, `~/Library/Application
Support/Sue` on macOS, `~/.config/sue` on Linux). The first launch creates it as
a copy of the shipped defaults. A file from an older release, or one with any
color that is not a gray, is migrated on the next start: Sue keeps the values
you changed (except tinted colors) and saves the old file beside it as
`default.suerice.v2.bak` (or `.v1.bak`, `.v3.bak`).

`--self-check` boots every subsystem headlessly and prints a diagnostics log:
the Arena, the rice introspection, the sniffer, the package manager, the AI
registry and the shell working together through real egui frames (a drawer that
glides on its spring, a mode switch that glides the camera, typing undone with
`Ctrl+Z`, a paint stroke undone tile by tile, the timeline, the marking menu and
the palette, then 0 FPS), and the backbone (the live canvas saved as a project
and opened again, pictures and tables through the format registry, an operator
run with a preset and undone, a background job). When a GPU is available it
also renders a frame and checks that not one pixel is tinted. The process exits non-zero if any check
fails. `--snapshot <dir>` renders ten scenes into PNG files in `<dir>`, from a
fresh temporary profile: the canvas, a selection pill, Document mode, the
timeline, the marking menu, the palette, two Settings pages, the drawers with a
picture, and Code / Hex.

## Community

- Website: <https://magigun.github.io/sue/>
- Discord: <https://discord.gg/h4Hp2bKrHd>, for questions, ideas and showing what you made
- Downloads: <https://github.com/MagiGun/sue/releases>

The website lives on the `gh-pages` branch, and GitHub Pages publishes it
again on every push there.

## Read next

| File | For whom |
| --- | --- |
| [`HUMANS.md`](HUMANS.md) | Everyday users and creators: what Sue is and how the screen works, in plain English |
| [`HUMANZ.md`](HUMANZ.md) | Systems engineers: the full architectural specification |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | New contributors: setting up, where a change goes, the checks to run, and how AI assistants find the rules |
| [`AGENTS.md`](AGENTS.md) | Contributors and AI agents: the backbone rules (they bind only the shared foundations) |
| [`CLAUDE.md`](CLAUDE.md) | The same rules, mirrored for Claude-based agents |
| [`SECURITY.md`](SECURITY.md) | Reporting a security problem privately |

Agents commit only when asked and never push unless asked (rule 8 in `AGENTS.md`).

## License

Sue is under the **Sayn License** ([`LICENSE`](LICENSE)). It is the MIT
License with one more condition, so please read it before you use Sue:

1. Keep the copyright notice and the license text in every copy or
   substantial portion of Sue, as with MIT.
2. **Any distribution, modification or project that uses Sue must include
   the exact string `Sayn was here` in at least one file.** A line in your
   README or your own license file is enough.

Contributions to this repository are made under the same license.
