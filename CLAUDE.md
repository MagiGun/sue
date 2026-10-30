# Sue Backbone Rules

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

Sue is meant to do everything: writing, spreadsheets, design, photo and video,
audio and music, 3D, CAD and BIM, animation, games, engineering, the web and
more. Those features arrive one by one, so the parts they all stand on have to
hold. These rules protect **only that backbone**. They bind every contributor,
human or AI (Claude, Codex, Copilot, Cursor, Gemini, Aider and any other).
Feature and interface code is free: the guidelines at the end are advice, not
law. `AGENTS.md` and `CLAUDE.md` are identical copies. Change both or neither.

Architecture: [`HUMANZ.md`](HUMANZ.md) (the backbone in §1, every feature area
mapped onto it in §8). What people should experience:
[`HUMANS.md`](HUMANS.md).

## What the backbone is

| Part | Where |
| --- | --- |
| The kernel: arena, entity ids, components (built-in and registered), relations, values and properties, units, color, time, transactions and undo, the codec and the `.sue` container, registries, shared buffers, background jobs | `crates/sue-core` |
| The shared data models features build on: text marks, tables, curves and animation, sequences and note events, node graphs, geometry and transforms, page layout, layered pictures and blend modes, manuscripts, kept encoded files | `crates/sue-model` |
| Format handlers (import, export) and the `.sue` project format | `crates/sue-io`, `crates/sue-core/src/file.rs` |
| Operating-system isolation | `crates/sue-platform` |
| How changes reach the repository | Git, below |

Everything else (the shell, applets, panels, tools, studios, themes) is feature
code.

---

## Rule 1: One data model

- All user data lives in the `Arena` as entities: a stable `EntityId`, a
  class, an optional base buffer, extra components, properties and relations.
  Features read and write that data. They never keep a private copy of a
  document.
- Features link to each other's data by `EntityId` and relations (`sue.child`,
  `sue.ref` and registered kinds), never by pointers, file paths or private
  ids. A song on a timeline, a picture in a layout and a mesh in a game scene
  are the very same entities.
- A new kind of data is a new `ComponentType` registered in the
  `ComponentRegistry`, built on a shared model from `sue-model` whenever one
  fits: a spreadsheet is a `Table`, a shader or compositing network is a
  `NodeGraph`, a CAD part is `Geometry` with properties in real units
  (`Quantity`), a painting is a layered `picture` whose layers blend with the
  shared `BlendMode`s.

## Rule 2: Every change is a transaction

- Every change to backbone data goes through a `ReversibleTransaction` on an
  `UndoStack`. A transaction stores its inverse delta, never a snapshot of a
  whole document. `apply` is atomic and `revert` restores the exact previous
  state, entity ids and handles included.
- An edit that already happened in place (typing, a brush stroke) is recorded
  with `UndoStack::record` together with its exact delta.

## Rule 3: Open and forward compatible

- The backbone never hardcodes a list of features, file formats, operators or
  node types. They register at runtime: component types in the
  `ComponentRegistry`, formats in the `FormatRegistry`, operators in an
  `OperatorRegistry`, node types in a `NodeRegistry`, and classes and property
  keys under the feature's own namespace.
- Data a build does not understand survives untouched: unknown component types
  and unknown file chunks round-trip as opaque bytes.

## Rule 4: Stable files

- The `.sue` container and every component encoding carry a version. A new
  version reads every older file. A change that would break that needs a
  migration and a test that loads an old file.
- Saving and loading again reproduces the same entities, ids, components,
  properties and relations. Every encoding has a round-trip test.

## Rule 5: Shared buffers, no hidden copies

- Large data (pixels, samples, meshes, tensors) moves by handle, by `EntityId`
  or as a `Shared` buffer, never cloned to hand it to another module or thread.
- Background jobs read shared buffers and write through copy-on-write, so the
  interface never waits for them.

## Rule 6: Platform isolation

- OS-specific code (`windows-sys`, `winapi`, `libc`, `nix`, `jni`, `ndk`,
  `objc`, `cocoa`, `#[cfg(target_os = ...)]`) lives only in
  `crates/sue-platform`.
- `unsafe` is forbidden workspace-wide. Only `sue-platform` may relax this, for
  audited FFI, and only with a human's approval.

## Rule 7: A tested backbone

- A backbone change ships with tests for what it promises: codec round trips,
  undo and redo, and the invariants it relies on.
- The definition of done below passes with zero warnings.

## Rule 8: Git

- Agents commit only when the human has asked for commits (a standing "commit
  as you go" counts) and push only when asked. Never force push and never
  rewrite history that was pushed or that a human made.
- Work on a branch rather than directly on `main`, unless the human says
  otherwise.
- Commit messages read `<type>(<scope>): <summary>` (at most 72 characters)
  with a body wrapped at 72 columns. They carry no `Co-Authored-By` trailer or
  other AI attribution unless the human asks for one.
- At hand-off, explain in plain language what changed and why, and what was
  verified.

## Rule 9: Working alongside other agents

- Several agents may work at once, each in its own worktree and branch.
  They coordinate on a board in the shared Git directory
  (`$(git rev-parse --git-common-dir)/sue-agents/`): before changing
  anything, read the others' claims and your messages, then write your own
  claim of the paths you will change. [`docs/agents.md`](docs/agents.md)
  says how.
- Never edit a path another agent claims; ask it on the board instead.
  Shared files (the changelog, the guides, language and settings files,
  `Cargo.lock`) take additions only, never rewrites of others' lines.
- Never switch, commit, merge, reset or stash in a checkout that is not
  yours, and move a branch only with `git update-ref` after
  `git worktree list` shows nobody has it out.
- When work clashes: the older claim keeps its paths; on merge conflicts
  the later merger keeps both sides and re-runs the definition of done;
  when both changes cannot stand, stop and ask the human.

## Do and do not, in short

Do:

- Read `CONTRIBUTING.md` for where a change goes, and `HUMANZ.md` before
  touching the backbone.
- Keep each change small and on a branch, with tests for what it promises.
- Use the shared features (`sue-wm` widgets, `sue_wm::fonts`, `sue-type`,
  `sue-io` formats, `FilePick` for the system's file dialogs, the shared
  models) instead of writing new ones; give a shared feature a new mode
  when a studio needs more.
- Update `HUMANZ.md` and `HUMANS.md` when a feature changes, and add a line
  for people who use Sue under Unreleased in `CHANGELOG.md`.
- Say plainly what you verified and what you did not.

Do not:

- Keep a private copy of a document, link data by pointer or path, or
  snapshot a whole document for undo.
- Hardcode a list of features, formats, operators or node types in the
  backbone.
- Put OS-specific code outside `crates/sue-platform`, or write `unsafe`.
- Add a dependency that duplicates one the workspace already has.
- Commit build output (`target/`), editor folders, secrets or files you were
  not asked to change.
- Push, force push or rewrite history unless the human asked.

---

## Definition of done

All four pass with zero warnings:

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

## Guidelines for feature code (advice, not law)

- Put values people may want to tune in the rice (`sue-rice`), so they appear
  in Settings by themselves.
- Put every string people read in the language database (`sue-lang`, read
  with `tr!`), English first; CONTRIBUTING.md says how languages are added.
- Register applets, operators and presets instead of keeping lists by hand.
- Sue owns every feature; studios use them. A studio never builds its own
  color picker, file dialog, converter, brush engine, font list, text
  editor, 3D view or exporter when Sue has one (the shared widgets in
  `sue-wm`, the installed fonts in `sue_wm::fonts`, the rich text editor and
  typesetting in `sue-type`, the publishing formats in `sue-io`, the
  system's own open, save and folder dialogs through `FilePick`, the
  Converter, the shared models). When a studio needs more than a feature
  offers, give that feature a new mode or parameter so every studio gains
  it, instead of copying it.
- Studios are woven together. Work moves between them by `EntityId`
  (`ShellRequest::OpenIn`, `ShellRequest::SendEntity`, the Sue menu's Send
  To), never by exporting a file and importing it again. Offer what a studio
  or tool can do for the thing in focus through `SueApplet::sue_actions`.
- Draw nothing when nothing moves, and respect `HardwareProfile` and
  `FormFactor`.
- The default look is monochrome. Internal image caches default to JPEG XL.

## House style

- Docs, interface text and comments use no em dashes, en dashes or middle dots.
  A test (`bins/sue/src/style.rs`) scans the repository. The window title is
  exactly `Sue`.
