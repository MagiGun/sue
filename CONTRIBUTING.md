# Contributing to Sue

Thank you for helping. Sue is one creative environment that grows feature by
feature on a shared backbone, so the most useful thing to know before you
start is which part of the code you are touching. This file is the practical
guide; [`AGENTS.md`](AGENTS.md) holds the rules that bind every contributor,
human or AI. Read it once before your first change. It is short.

## Set up

1. Install Rust with [rustup](https://rustup.rs). The repository pins the
   toolchain the checks pass with (and `rustfmt` and `clippy`) in
   `rust-toolchain.toml`, so `cargo` fetches it by itself. Sue builds with
   Rust 1.95 or newer.
2. Install what your system needs to open a window and play sound:
   - **Windows**: nothing more (the MSVC build tools rustup asks for).
   - **macOS**: the Xcode command line tools (`xcode-select --install`).
   - **Linux** (Debian or Ubuntu names; other distributions have the same
     libraries):

     ```bash
     sudo apt install build-essential pkg-config libasound2-dev libudev-dev libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev libgl1-mesa-dev libgtk-3-dev
     ```

3. Build and run:

   ```bash
   cargo run -p sue
   ```

   A graphics driver with Vulkan, DirectX 12, Metal or OpenGL ES is needed for
   the window. Everything else (the tests, `--self-check`) runs without one.

## Where your change goes

| You are changing | Where | How strict |
| --- | --- | --- |
| The kernel: entities, components, relations, values, undo, the `.sue` file, registries, jobs | `crates/sue-core` | Backbone: every rule in `AGENTS.md` |
| Shared data models (text, tables, curves, sequences, node graphs, geometry, pictures) | `crates/sue-model` | Backbone |
| File formats (import and export) | `crates/sue-io` | Backbone |
| Anything that talks to the operating system | `crates/sue-platform` | Backbone; the only place for OS code and `unsafe` |
| The shell, studios, panels, tools, themes | `crates/sue-wm`, `crates/sue-paint`, `crates/sue-writer`, `crates/sue-type`, `bins/sue` | Feature code: the guidelines are advice |

[`HUMANZ.md`](HUMANZ.md) explains the architecture (section 1 is the
backbone; section 8 maps every feature area onto it).
[`HUMANS.md`](HUMANS.md) describes what people should experience.

## The ten things to remember

1. **One data model.** User data lives in the `Arena` as entities. Studios
   read and write it there and link by `EntityId`; they never keep their own
   copy of a document.
2. **Every change is undoable.** Edits are `ReversibleTransaction`s applied
   through the `UndoStack` (or recorded with their exact delta when they
   already happened, such as a brush stroke).
3. **Register, do not list.** New file formats, operators, node types and
   component types register at runtime. Nothing in the backbone names a
   feature.
4. **Files stay readable.** Every encoding is versioned and has a round trip
   test. A new version reads every older file.
5. **No hidden copies.** Large buffers move by handle or as `Shared`.
6. **OS code only in `sue-platform`.** No `cfg(target_os)`, `windows-sys`,
   `libc` or `objc` anywhere else. `unsafe` is forbidden everywhere.
7. **Studios share features.** Use the shared color picker, file picker,
   fonts, text editor, exporters and models; give a shared feature a new mode
   instead of copying it.
8. **Tests with every backbone change**, and runtime tests (in
   `bins/sue/src/runtime.rs`) for user-facing flows where you can drive them.
9. **House style.** Docs, interface text and comments use no em dashes, en
   dashes or middle dots (a test scans for them). The window title is exactly
   `Sue`. The default look is monochrome.
10. **Docs move with the code.** Update `HUMANZ.md` for how a feature is
    built and `HUMANS.md` for how people use it.

## Before you open a pull request

All four must pass with zero warnings:

```bash
cargo fmt --all --check
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

```bash
cargo run -p sue -- --self-check
```

The CI workflow runs the same four on Windows, macOS and Linux.

## Building packages

Each system has a script in `packaging/`, and the Release workflow runs
them all when a `v*` tag is pushed. Every package lands in `dist/`.

| System | Command | Makes |
| --- | --- | --- |
| Windows | `powershell -File packaging/windows/build.ps1` | a zip with `Sue.exe` |
| Linux | `packaging/linux/build.sh` | `.tar.gz`, `.deb`, `.rpm` and an AppImage |
| Linux from any system | `docker build -t sue-linux packaging/linux`, then `docker run --rm -v "$PWD:/src" -v sue-target:/target sue-linux` | the same, built in Debian 12 |
| macOS | `packaging/macos/build.sh` (on a Mac) | `Sue.app` for Apple silicon and Intel, zipped and as a `.dmg` |
| iPhone and iPad | `packaging/ios/build.sh` (on a Mac with Xcode) | an unsigned `.ipa` and a simulator build |
| Android phones and tablets | `python packaging/android/build_apk.py --release` | one APK for arm64, armv7 and x86_64 |

Android needs the SDK with the NDK, Java's `keytool`, `cargo install
cargo-ndk` and the Rust targets `aarch64-linux-android`,
`armv7-linux-androideabi` and `x86_64-linux-android`; `--abi x86_64` builds
just what an emulator runs. Sue starts there as a native activity from
`crates/sue-platform/android`, the one place that exports a symbol for the
system to call.

## Branches and commits

- Work on a branch, never directly on `main`.
- Commit messages read `<type>(<scope>): <summary>`, at most 72 characters,
  with a body wrapped at 72 columns that says what changed and why. Types:
  `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`.
  Scopes are crate or studio names: `core`, `model`, `io`, `platform`, `wm`,
  `paint`, `writer`, `type`, `rice`.
- Never force push a shared branch or rewrite history someone else made.
- In the pull request, say in plain words what changed, why, and how you
  checked it.

## Working with an AI assistant

Sue is built with AI assistants as well as by hand, and the same rules apply
to both. Every common assistant finds the rules on its own:

| Assistant | Reads |
| --- | --- |
| Codex, Jules, Amp and most agents | `AGENTS.md` |
| Claude Code | `CLAUDE.md` (an exact copy of `AGENTS.md`) |
| GitHub Copilot | `.github/copilot-instructions.md` |
| Cursor | `.cursor/rules/sue.mdc` |
| Gemini CLI | `GEMINI.md` |
| Windsurf | `.windsurfrules` |
| Aider | `.aider.conf.yml` (loads `AGENTS.md`) |

All of them point back to `AGENTS.md`, so there is one source of truth.
`AGENTS.md` and `CLAUDE.md` must stay identical: change both or neither.

You are responsible for what your assistant writes. Read the diff, run the
four checks, and make sure it did not touch the backbone without tests.

## Adding common things

- **A filter or adjustment in Sue Paint**: a `Filter` variant in
  `crates/sue-paint/src/filters.rs` (its label, its `apply`), its sliders in
  `dialogs.rs`, and the menu command in `studio.rs` and `menus.rs`. Add a
  test in `filters.rs`.
- **A file format**: a handler in `crates/sue-io` registered in the
  `FormatRegistry`, with a round trip test, and a decoder that refuses bad
  input with a reason instead of panicking.
- **A studio**: an applet implementing `SueApplet` in its own crate,
  registered with the Launchpad. It works on arena entities and offers its
  actions through `sue_actions`.
- **A setting**: a `RiceDeclaration` in the applet and the same key in
  `crates/sue-rice/rice/default.suerice` (a test checks they match).

## Reporting problems

Open an issue with what you did, what you expected and what happened, your
system, and the output of `cargo run -p sue -- --self-check`. Security
problems go privately to the maintainers, not in a public issue (see
[`SECURITY.md`](SECURITY.md)).
