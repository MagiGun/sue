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

v1.0 "Faustian Enigma" is the **foundation release**. It contains the seven
kernel crates below and a headless executable that boots every subsystem and runs
a self-check. The windowed shell, the studios, and the codec and AI model backends
are built on top of this foundation in later releases. `HUMANZ.md` §7 lists, part
by part, what is implemented today and what is only specified.

## Crate map

| Crate | Role | Key types |
| --- | --- | --- |
| [`sue-core`](crates/sue-core) | The substrate: arena, entities, projections, undo tree | `Arena`, `AssetHandle`, `ComponentData`, `ProjectionMask`, `ReversibleTransaction`, `UndoStack` |
| [`sue-platform`](crates/sue-platform) | The only place OS-specific code may live | `PlatformHost`, `HeadlessHost`, `FormFactor`, `InputEvent`, `HardwareCapabilities` |
| [`sue-rice`](crates/sue-rice) | `.suerice` parser, live Settings GUI descriptors, hardware profiles | `RiceSchema`, `RiceParameter`, `SettingsTree`, `HardwareProfile` |
| [`sue-io`](crates/sue-io) | Magic-byte sniffer, JPEG XL pipeline, tile cache, byte-map fallback | `sniff_stream`, `IngestRoute`, `JxlPipeline`, `TileCache` |
| [`sue-pkg`](crates/sue-pkg) | Content-addressed packages, Ed25519 trust, resolver, atomic generations | `PackageManifest`, `ContentHash`, `TrustStore`, `PackageIndex`, `resolve`, `AtomicUpdater` |
| [`sue-ai`](crates/sue-ai) | Offline, zero-copy AI substrate | `LocalModelRegistry`, `ComputeBackend`, `ZeroCopyTensorTask` |
| [`sue-wm`](crates/sue-wm) | Adaptive tiling/touch window manager | `SueApplet`, `Launchpad`, `Workspace`, `solve_layout`, `SpringDamper`, `FrameScheduler` |
| [`bins/sue`](bins/sue) | The executable: boot + self-check diagnostics | — |

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

## Quick start

```bash
cargo run -p sue
```

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

`cargo run -p sue` boots Sue v1.0 "Faustian Enigma" in headless mode and prints
a diagnostics log. The log shows the Arena, the rice introspection, the sniffer,
the package manager, the AI registry and the window manager working together.
The process exits non-zero if any check fails.

## Read next

| File | For whom |
| --- | --- |
| [`HUMANS.md`](HUMANS.md) | Everyday users and creators: what Sue is and how the screen works, in plain English |
| [`HUMANZ.md`](HUMANZ.md) | Systems engineers: the full architectural specification |
| [`AGENTS.md`](AGENTS.md) | AI coding agents: the seven unbreakable laws |
| [`CLAUDE.md`](CLAUDE.md) | The same seven laws, mirrored for Claude-based agents |

Commits and pushes are made **by humans only** (see LAW 7 in `AGENTS.md`).
