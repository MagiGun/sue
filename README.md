# Sue v1.0 "Faustian Enigma"

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

![Version](https://img.shields.io/badge/version-1.0%20Faustian%20Enigma-3b82f6)
![Rust](https://img.shields.io/badge/rust-2024%20edition-b7410e?logo=rust)
![MSRV](https://img.shields.io/badge/MSRV-1.88-555555)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android%20%7C%20iOS-2b5797)
![Kernel](https://img.shields.io/badge/kernel-relational%20ECS%20%2F%20DAG-1f6f5c)
![Images](https://img.shields.io/badge/images-JPEG%20XL%20first-3b82f6)
![AI](https://img.shields.io/badge/AI-100%25%20offline-444444)
![unsafe](https://img.shields.io/badge/unsafe-forbidden-2e7d32)

**Sue, the Singular Unified Engine,** is one lightweight, endlessly expandable
creative environment. Day to day it is a fast file explorer, media player, notes
app, converter and web browser. One click turns it into a document processor,
vector/raster studio, DAW, video editor, CAD/3D modeler, IDE or game engine.

Sue does this without a pile of separate apps or an N×N grid of file converters.
Every studio works on the same data in one shared, generation-indexed **Arena**.
Switching from "Document" to "Video", or from "2D" to "3D", attaches a new
*projection* to the data that is already there. Nothing is copied or converted.

## Where things stand

v1.0 "Faustian Enigma" contains the seven kernel crates below and the `sue`
executable, which opens a native GPU window laid out like a studio workstation,
flush and edge to edge in a neutral graphite palette:

- **Header:** File, Edit, Workspace, Tools and Settings menus, the tabs (Home
  stays first), the Settings gear and the dock toggles.
- **Left rail and dock:** Files (browse folders, open anything), Player (MP3,
  FLAC, WAV, Ogg and M4A through your sound device), Convert (pictures to JPEG
  XL or PNG, songs to WAV) and Notes (saved automatically).
- **Center Stage:** Home, with the installed studios and inspectors and a "Pin
  as boot" switch on each, or the open editors: Text Studio, Image Viewer
  (JPEG XL, PNG, JPEG), Hex Inspector and Settings.
- **Right rail and dock:** Inspector (the properties of whatever is open),
  Browser (an address bar and bookmarks that open in your browser) and History
  (the undo tree; click any state to go back to it).
- **Status bar:** the active tab, entity count, undo depth, hardware profile
  and frame state (`Idle` at 0 FPS whenever nothing moves).

Settings is a tab with eight categories and a search field. Every option
applies at once, can be undone with `Ctrl+Z`, and is saved back to
`default.suerice`. Drop any file on the window and Sue opens it by its content.

The large studios (document, vector and paint, DAW, video, 3D and CAD, IDE,
game engine) and the AI model runtimes come in later releases. `HUMANZ.md` §7
lists, part by part, what is implemented today and what is only specified.

## Crate map

| Crate | Role | Key types |
| --- | --- | --- |
| [`sue-core`](crates/sue-core) | The substrate: arena, entities, projections, undo tree | `Arena`, `AssetHandle`, `ComponentData`, `ProjectionMask`, `ReversibleTransaction`, `UndoStack` |
| [`sue-platform`](crates/sue-platform) | The only place OS-specific code may live: hosts, window and frame pacing, GPU choice and device, audio output, dialogs | `PlatformHost`, `DesktopHost`, `run_desktop`, `FramePacing`, `GpuPreference`, `AudioOutput`, `InputEvent` |
| [`sue-rice`](crates/sue-rice) | `.suerice` parser, Settings descriptors, presets, migration, hardware profiles | `RiceSchema`, `RiceParameter`, `SetRiceValues`, `ThemePreset`, `migrate`, `HardwareProfile` |
| [`sue-io`](crates/sue-io) | Magic-byte sniffer, JPEG XL, PNG, JPEG and audio codecs, ingest, tile cache, byte map | `sniff_stream`, `ingest_bytes`, `NativeJxlPipeline`, `decode_audio`, `TileCache` |
| [`sue-pkg`](crates/sue-pkg) | Content-addressed packages, Ed25519 trust, resolver, atomic generations | `PackageManifest`, `ContentHash`, `TrustStore`, `PackageIndex`, `resolve`, `AtomicUpdater` |
| [`sue-ai`](crates/sue-ai) | Offline, zero-copy AI substrate | `LocalModelRegistry`, `ComputeBackend`, `ZeroCopyTensorTask` |
| [`sue-wm`](crates/sue-wm) | Window manager: workstation layout, tabs and docks, immediate-mode UI, GPU renderer | `SueApplet`, `Launchpad`, `Workspace`, `Catalog`, `Ui`, `UiRenderer`, `SpringDamper`, `FrameScheduler` |
| [`bins/sue`](bins/sue) | The executable: shell runtime, built-in applets, self-check, snapshots | `Engine`, `Runtime` |

```text
bins/sue      ──► every crate below
sue-wm        ──► sue-rice, sue-platform, sue-core   + wgpu, fontdue, epaint_default_fonts
sue-ai        ──► sue-platform, sue-core
sue-rice      ──► sue-platform, sue-core
sue-io        ──► sue-core                            + jxl-oxide, zune-jpegxl, zune-png, zune-jpeg, symphonia
sue-pkg       ──► sha2, ed25519-dalek, semver         (no engine crates)
sue-core      ──► nothing                              (leaf)
sue-platform  ──► winit, wgpu, pollster, cpal, audio_thread_priority, rfd, open   (no engine crates)
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
with Vulkan, DirectX 12, Metal or OpenGL ES; the Player needs an audio output
device. Your settings live in `default.suerice` in the `rice` folder of your
config directory (`%APPDATA%\Sue` on Windows, `~/Library/Application
Support/Sue` on macOS, `~/.config/sue` on Linux). The first launch creates it as
a copy of the shipped defaults; a file from an older release is migrated to the
current format on the next start, keeping a `.v1.bak` copy and any values you
changed yourself.

`--self-check` boots every subsystem headlessly and prints a diagnostics log:
the Arena, the rice introspection, the sniffer, the package manager, the AI
registry and the window manager working together, including a check that a
clicked dock animation runs to the end with no further input. When a GPU is
available it also renders the shell offscreen. The process exits non-zero if any
check fails. `--snapshot <dir>` renders ten scenes (Home, four Settings pages,
the Image Viewer, Text Studio, Hex Inspector, Player and Convert) into PNG files
in `<dir>`, from a fresh temporary profile.

## Read next

| File | For whom |
| --- | --- |
| [`HUMANS.md`](HUMANS.md) | Everyday users and creators: what Sue is and how the screen works, in plain English |
| [`HUMANZ.md`](HUMANZ.md) | Systems engineers: the full architectural specification |
| [`AGENTS.md`](AGENTS.md) | AI coding agents: the eight unbreakable laws |
| [`CLAUDE.md`](CLAUDE.md) | The same eight laws, mirrored for Claude-based agents |

Commits and pushes are made **by humans only** (see LAW 7 in `AGENTS.md`).
