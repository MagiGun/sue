# Sue — v1.0 "Faustian Enigma"

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

![Version](https://img.shields.io/badge/version-1.0%20Faustian%20Enigma-8b1e3f)
![Rust](https://img.shields.io/badge/rust-2024%20edition-b7410e?logo=rust)
![MSRV](https://img.shields.io/badge/MSRV-1.88-555555)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android%20%7C%20iOS-2b5797)
![Kernel](https://img.shields.io/badge/kernel-relational%20ECS%20%2F%20DAG-1f6f5c)
![Images](https://img.shields.io/badge/images-JPEG%20XL%20first-c9a227)
![AI](https://img.shields.io/badge/AI-100%25%20offline-6a4c93)
![unsafe](https://img.shields.io/badge/unsafe-forbidden-2e7d32)

**Sue — the Singular Unified Engine** — is one lightweight, endlessly expandable
creative environment. Day to day it is a fast file explorer, media player, notes
app, converter and web browser. One click turns it into a document processor,
vector/raster studio, DAW, video editor, CAD/3D modeler, IDE or game engine.

Sue does this without a pile of separate apps or an N×N grid of file converters.
Every studio works on the same data in one shared, generation-indexed **Arena**.
Switching from "Document" to "Video", or from "2D" to "3D", attaches a new
*projection* to the data that is already there. Nothing is copied or converted.

## Where things stand

v1.0 "Faustian Enigma" contains the seven kernel crates below and the `sue`
executable, which opens a **native GPU window**:

- a top bar with the active mode and a live render meter
  (`Render: SLEEP (0 FPS)` while nothing changes),
- a Left Dock with Quick Notes and a Drop Zone,
- a Launchpad of Studio and Utility cards in the center, with pin-to-boot,
- the live Settings for every `.suerice` parameter in the Right Dock. Every
  edit applies at once, can be undone with `Ctrl+Z`, and is saved back to the
  file.

Drop a file on the window and Sue routes it by its content. A JPEG XL image is
decoded and opens in the Image Viewer, text opens in the Text Studio, and
anything else opens in the Hex Inspector. The large studios (DAW, video, CAD,
IDE, game engine) and the AI model runtimes come in later releases. `HUMANZ.md`
§7 lists, part by part, what is implemented today and what is only specified.

## Crate map

| Crate | Role | Key types |
| --- | --- | --- |
| [`sue-core`](crates/sue-core) | The substrate: arena, entities, projections, undo tree | `Arena`, `AssetHandle`, `ComponentData`, `ProjectionMask`, `ReversibleTransaction`, `UndoStack` |
| [`sue-platform`](crates/sue-platform) | The only place OS-specific code may live: hosts, native window, GPU device | `PlatformHost`, `HeadlessHost`, `DesktopHost`, `run_desktop`, `GpuContext`, `InputEvent` |
| [`sue-rice`](crates/sue-rice) | `.suerice` parser, live Settings GUI descriptors, hardware profiles | `RiceSchema`, `RiceParameter`, `SettingsTree`, `HardwareProfile` |
| [`sue-io`](crates/sue-io) | Magic-byte sniffer, JPEG XL codec, ingest, tile cache, byte-map fallback | `sniff_stream`, `ingest_bytes`, `NativeJxlPipeline`, `TileCache` |
| [`sue-pkg`](crates/sue-pkg) | Content-addressed packages, Ed25519 trust, resolver, atomic generations | `PackageManifest`, `ContentHash`, `TrustStore`, `PackageIndex`, `resolve`, `AtomicUpdater` |
| [`sue-ai`](crates/sue-ai) | Offline, zero-copy AI substrate | `LocalModelRegistry`, `ComputeBackend`, `ZeroCopyTensorTask` |
| [`sue-wm`](crates/sue-wm) | Adaptive tiling/touch window manager, immediate-mode UI, GPU renderer | `SueApplet`, `Launchpad`, `Workspace`, `World`, `Ui`, `UiRenderer`, `SpringDamper`, `FrameScheduler` |
| [`bins/sue`](bins/sue) | The executable: desktop shell, built-in applets, self-check, snapshots | — |

```text
bins/sue      ──► every crate below
sue-wm        ──► sue-rice, sue-platform, sue-core   + wgpu, fontdue, epaint_default_fonts
sue-ai        ──► sue-platform, sue-core
sue-rice      ──► sue-platform, sue-core
sue-io        ──► sue-core                            + jxl-oxide, zune-jpegxl
sue-pkg       ──► sha2, ed25519-dalek, semver         (no engine crates)
sue-core      ──► nothing                              (leaf)
sue-platform  ──► winit, wgpu, pollster                (no engine crates)
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

`cargo run -p sue` opens the Sue v1.0 "Faustian Enigma" window. It needs a
graphics driver with Vulkan, DirectX 12, Metal or OpenGL ES. Your settings live
in `default.suerice` in the `rice` folder of your config directory
(`%APPDATA%\Sue` on Windows, `~/Library/Application Support/Sue` on macOS,
`~/.config/sue` on Linux). The first launch creates it as a copy of the shipped
defaults.

`--self-check` boots every subsystem headlessly and prints a diagnostics log.
The log shows the Arena, the rice introspection, the sniffer, the package
manager, the AI registry and the window manager working together. When a GPU is
available it also renders the shell offscreen. The process exits non-zero if any
check fails. `--snapshot <dir>` renders four views of the shell (Launchpad,
Image Viewer, Text Studio, Hex Inspector) into PNG files in `<dir>`.

## Read next

| File | For whom |
| --- | --- |
| [`HUMANS.md`](HUMANS.md) | Everyday users and creators: what Sue is and how the screen works, in plain English |
| [`HUMANZ.md`](HUMANZ.md) | Systems engineers: the full architectural specification |
| [`AGENTS.md`](AGENTS.md) | AI coding agents: the seven unbreakable laws |
| [`CLAUDE.md`](CLAUDE.md) | The same seven laws, mirrored for Claude-based agents |

Commits and pushes are made **by humans only** (see LAW 7 in `AGENTS.md`).
