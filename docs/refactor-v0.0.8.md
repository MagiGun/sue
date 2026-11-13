# Groundwork for 0.0.8

0.0.8 is a large release rather than a round of small fixes, and it may
start with a refactor. This page is where that work begins: what the code
looks like after 0.0.75, where it has grown past what one file or one
function should hold, and the rules a refactor follows so that nothing a
person relies on breaks along the way. Update it as the work moves; delete
a section once it is done.

## Where things stand (after 0.0.75 and pull requests 6 to 9)

| Measure | Value |
| --- | --- |
| Rust lines in `crates` and `bins` | about 184,000 |
| Tests (`#[test]`) | 795 |
| `TODO`, `FIXME` or `HACK` markers | 0 |
| Crates in `Cargo.lock` | 557, 33 of them in two versions |
| `#[allow]` and `#[expect]` (besides the audited `unsafe`) | 81 |
| Sue Paint parity with Clip Studio Paint | 84% (267 done, 61 partial, 28 missing) |

Lines per crate: `sue-paint` 71,600; `bins/sue` 19,500; `sue-io` 17,800;
`sue-wm` 17,100; `sue-platform` 13,500; `sue-core` 10,100; `sue-type`
10,000; `sue-writer` 9,700; `sue-model` 6,500; `sue-rice` 3,500; `sue-lang`
1,700; `sue-pkg` 1,600; `sue-ai` 1,100.

## Where the code has outgrown its files

| File | Lines | What is wrong | Proposed shape |
| --- | --- | --- | --- |
| `bins/sue/src/runtime.rs` | 8,700 | 7,200 of the lines are its tests | the runtime alone; its tests under `bins/sue/src/runtime/tests/`, one file per studio or area, sharing one driver (the `PaintDriver` already written for the timeline tests) |
| `crates/sue-paint/src/studio.rs` | 7,700 | `run()` is about 3,000 lines matching 408 command names by hand, and `commands()` keeps another list beside it | commands registered by the module that owns them (an `OperatorRegistry` per studio: name, label, shortcut, when it can run, the function), so the menus, shortcuts, command palette and the "every command is in a menu" test all read one table |
| `crates/sue-paint/src/panels.rs` | 5,900 | every palette's interface in one file | one module per palette under `panels/`, named as `dock::Palette` names them |
| `crates/sue-paint/src/canvas.rs` | 5,100 | pointer and pen input, gestures and drawing the view together | `canvas/input.rs` (events to gestures), `canvas/gestures.rs`, `canvas/view.rs` (drawing) |
| `crates/sue-paint/src/brush.rs` | 4,700 | the brush model (tips, dynamics, saving them) and the engine that lays dabs in one file | `brush/tip.rs` and `brush/dynamics.rs` (the model and its encoding), `brush/engine.rs` (strokes and dabs) |
| `crates/sue-writer/src/ui.rs` | 4,200 | Sue Writer's whole interface | one module per pane, as for Sue Paint |
| `crates/sue-paint/src/dialogs.rs` | 3,900 | every dialog | one module per dialog family |
| `crates/sue-wm/src/shell.rs` | 3,400 | the shell and everything drawn around the studios | the frame, the Hub and the top bar apart |

Nothing here is wrong in what it does; every one of these passes its
tests. They are hard to change safely because a change to one part means
reading thousands of lines around it, and two agents cannot work in the
same file at once.

## Where the architecture bends its own rules

- **A studio depends on another studio.** `sue-writer` depends on
  `sue-paint` for two functions, `composite::signature` and
  `composite::preview`. Sue owns its features and studios use them
  (`AGENTS.md`, Guidelines): these belong in a shared crate (`sue-model`'s
  `picture` or a small rendering module in `sue-wm`), after which
  `sue-writer` no longer builds Sue Paint.
- **Commands are listed by hand** in `studio.rs` (see above), where
  `AGENTS.md` asks for registered operators.
- **33 crates in two versions** (`cargo tree -d`). Each one is build time
  and binary size; most come from winit, wgpu and the codecs moving at
  different speeds. Line them up where the newer version works for every
  user of it, and say in `deny.toml` why the rest stay.
- **81 lint exceptions.** Each should say why (`reason = "..."`) or go.

## Rules for the refactor

1. **A refactor changes no behaviour.** A pull request that moves code
   does not also change what it does; a fix found on the way is its own
   pull request. Reviewers can then read a move as a move.
2. **One file or one seam per pull request,** so two agents never touch
   the same file. Claim it on the agent board (`docs/agents.md`).
3. **Tests first.** Before splitting a file, make sure what people do with
   it is covered by a runtime test that drives real frames (the
   `PaintDriver` style in `bins/sue/src/runtime.rs`), so the split is
   checked by the same test before and after.
4. **The definition of done passes after every step** (`AGENTS.md`), with
   zero warnings, and the self-check passes 10 of 10.
5. **Files stay readable:** aim for modules under about 1,500 lines and
   functions under about 150. These are guidelines, not a test.
6. **The backbone rules still bind:** a refactor that touches `sue-core`,
   `sue-model`, `sue-io` or `sue-platform` keeps every encoding's version,
   round trip and test (`AGENTS.md`, Rules 1 to 7).

A suggested order, each step small enough to merge on its own:

1. Move `runtime.rs`'s tests into files of their own (no code changes).
2. Move `composite::signature` and `composite::preview` out of Sue Paint;
   drop `sue-writer`'s dependency on `sue-paint`.
3. Split `panels.rs` by palette, then `dialogs.rs`.
4. Give Sue Paint a command registry and move `run()`'s arms into the
   modules that own them, a group at a time (File, Edit, Layer, and so on),
   with the menus and shortcuts reading the registry.
5. Split `canvas.rs` and `brush.rs`.
6. Do the same for Sue Writer's `ui.rs` and the shell.
7. Line up duplicated dependencies and give each lint exception its reason.

## Carried into 0.0.8

- **Linux pens are untested on hardware.** The Wayland and X11 pen work
  (#2) passes its tests and the Linux build, but nobody has drawn with it
  on a real tablet yet; the reporters of #2 and #4 were asked to try it.
- **Pop Out windows** (a palette in a window of its own) were not seen on
  screen; the tests run with embedded windows.
- **Translations** cover what existed on 2026-10-30; everything newer shows
  in English in other languages (`sue-cli lang check <tag>` lists it).
- **Sue Paint parity gaps** (`docs/paint-parity.md`): shading assist, vivid
  watercolor, painting straight on 3D, FBX and VRM import, 3D head shapes,
  text that wraps inside a shape, batch processing, multi-page printing,
  CMYK PDF, OpenToonz export, a font manager, user tags, and 3D and texture
  materials, plus about 50 rows still to verify.

## Working on this machine and others

- Keep build output out of the repository and off small system drives:
  set `CARGO_TARGET_DIR` per checkout to a folder on a fast data drive. A
  debug build of everything is about 3 GB with line tables only for Sue's
  crates and no debug info for dependencies.
- Set `SUE_HOME` to a scratch folder when Sue runs for tests or for an
  assistant: it keeps every file of Sue's own there, so a test run never
  touches the settings of the person who uses Sue, and an assistant whose
  sandbox cannot write AppData still passes `--self-check`.
