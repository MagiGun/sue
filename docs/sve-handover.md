# Sue VE: handover for stage 2

Stage 1 (0.0.83) built Sue VE's core and its desktop editor. This page is
for whoever builds stage 2 (effects and compositing in the manner of
After Effects, the Color page, the Audio page, the phone layout in the
manner of CapCut, polish). Read `CLAUDE.md` and `HUMANZ.md` §1 and §3.10
first; `docs/ve-parity.md` lists what is done and what is missing.

## Where things are

| File | What |
| --- | --- |
| `crates/sue-model/src/sequence.rs` | the shared `Sequence` (version 2: clip ids, enabled, links, keys; transitions; markers; properties) and `SequenceEdit`, the undo step of a sequence change |
| `crates/sue-ve/src/model.rs` | classes (`ve.project`, `ve.title`, ...), property keys (`media.*`, `ve.*`), `SourceKind`, the project and pool helpers, `plan` (an edit on a copy, as one `SequenceEdit`) |
| `crates/sue-ve/src/edit.rs` | the edit algebra (adapted from FilmCraft's), pure functions over `&mut Sequence`; `edit/tests.rs` undoes and redoes every one |
| `crates/sue-ve/src/media/` | import (`import.rs`), the decode workers and frame cache (`frames.rs`), decoded sound and waveforms (`sound.rs`), thumbnails and proxies (`thumbs.rs`), the hub the studio holds (`mod.rs`) |
| `crates/sue-ve/src/render.rs` | `layers_at` (what shows at a time), `corners` (placement), `composite` (the processor compositor), the generated sources (titles, solids, stills, Sue Paint pictures) |
| `crates/sue-ve/src/mix.rs` | the sound mix and the meters |
| `crates/sue-ve/src/playback.rs` | the clock (sound master), shuttle, loop |
| `crates/sue-ve/src/export.rs` | exports off the interface's thread on an arena snapshot; `Renderer` draws frames and mixes sound for any caller |
| `crates/sue-ve/src/studio.rs` | `SueVe`: state, the `SueApplet` implementation, import, placing, proxies, exports |
| `crates/sue-ve/src/commands.rs`, `menus.rs` | every command (menus, shortcuts, palette) and the menu bar |
| `crates/sue-ve/src/ui/` | pages, dock panels, monitors, timeline, inspector, pool |
| `crates/sue-platform/src/video.rs` | the neutral decoding API; `crates/sue-platform/windows/src/decode.rs` is the Media Foundation side |
| `bins/sue/src/runtime/tests/sve.rs` | runtime tests driving real frames; `bins/sue/src/snapshot/scenes/sve.rs` the snapshot scenes (`SUE_SNAPSHOT_ONLY=sve`) |

## Extension points

- **Effects (video).** Give a clip an effects chain as a `NodeGraph`
  (`sue_model::NodeGraph`, a new `ve.effects` extension component on the
  clip's properties or a child entity named by `EntityId` in
  `clip.properties`). Evaluate it per layer between decoding and drawing:
  in `render::composite` (processor, the oracle) and in
  `ui::monitor::draw_layers`, which draws textured quads today; for GPU
  effects, draw the layer through a wgpu render pass in `sue-platform`'s
  painter (a callback or an offscreen texture registered with
  `TextureUpload`), keeping the processor path as the reference the tests
  compare against.
- **Adjustment clips.** `SourceKind::Adjustment` is skipped by
  `layers_at`. Once effects exist, apply the clip's chain to the
  composite of the tracks below it at that time.
- **Transitions.** Kinds are names (`Transition::kind`); `layers_at` and
  `mix::mix` know `dissolve`, `dip-black` and `dip-white`. A new kind is a
  new branch there and a row in `ui/panels.rs`; wipes need a mask per
  layer in both drawing paths.
- **Keyframes.** Clips keep `Animation` curves keyed in source time. Only
  linear keys are made; the curves hold Bezier keys already. A graph
  editor edits `clip.animation` through `model::plan`.
- **Compositions (After Effects).** A sequence may be a clip source
  (`SourceKind::Sequence`); `layers_at` skips it today. Nesting renders
  the inner sequence's layers with the outer clip's transform applied
  (watch for cycles: walk with a seen set).
- **Color page.** Grades as a `NodeGraph` per clip (or per track);
  scopes read the frames from the frame cache (`media::frames`), which
  hands them out by `Arc` without copying.
- **Audio page.** `mix::mix` is the whole mixer: per track gain, mute
  and solo. Buses, pans, effects and automation go there; meters per
  track come from mixing tracks into separate blocks.
- **Phone layout.** `ui::SueVe::frame` draws the desktop layout for every
  form factor. Branch on `cx.view.form_factor == FormFactor::Mobile` as
  Sue Writer does (`crates/sue-writer/src/ui/phone.rs`): a preview on
  top, a single main track with the tool strip under it. The model, the
  edit algebra and the commands need no change.

## Known gaps

- Decoded sound is held in memory (about 23 MB a minute of stereo at
  48 kHz); stream it from disk for long projects.
- Hardware decoding through Direct3D 11 reads every frame back to the
  processor, which measured slower than Windows' software decoder on the
  test machine (see the measurements in the 0.0.83 report and the
  `measure_decoding_and_export` test); a zero copy path would share the
  decoder's texture with wgpu (D3D11 to D3D12 interop) in
  `sue-platform`. The setting `sue_ve.hardware_decode` stays off by
  default until then.
- Image sequences and Sue Paint animations decode on the interface's
  thread (cached by frame); large frames there can stutter playback.
- No relink of missing files, no bins command, no clip markers, no rate
  stretch tool, no nested sequences, no captions.
- Floating palettes: a palette dragged off the dock goes back to its
  place (stage 1 draws none floating).
- Movies decode only on Windows. `sue_platform::video` reports
  `NotAvailable` elsewhere; stills, titles, sound files (Symphonia) and
  Sue Paint pictures work everywhere.
- The program monitor previews at the size of the sequence's sources
  scaled by the quality; titles are drawn at the sequence's frame size
  and cached until they change.

## How to check your work

```bash
cargo test -p sue-ve
cargo test -p sue --lib -- sve
cargo run -p sue -- --self-check
SUE_SNAPSHOT_ONLY=sve cargo run -p sue -- --snapshot <folder>
cargo test -p sue-ve --release measure -- --ignored --nocapture
```
