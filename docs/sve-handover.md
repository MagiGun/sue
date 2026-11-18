# Sue VE: handover for stage 3

Stage 1 (0.0.83) built Sue VE's core and its desktop editor; stage 2
(0.0.83) built effects and compositing, the Color and Audio pages, the
phone and tablet layout and the desktop polish. This page is for whoever
builds next. Read `CLAUDE.md` and `HUMANZ.md` §1 and §3.10 first;
`docs/ve-parity.md` lists what is done and what is missing.

## Where things are

| File | What |
| --- | --- |
| `crates/sue-model/src/sequence.rs` | the shared `Sequence` (version 2) and `SequenceEdit`, the undo step of a sequence change |
| `crates/sue-ve/src/model.rs` | classes, property keys, `SourceKind`, the project and pool helpers, `plan` (an edit on a copy, as one `SequenceEdit`) |
| `crates/sue-ve/src/edit.rs`, `keys.rs` | the edit algebra (from FilmCraft's); eases, key moves, time remapping and ramps, rate stretch, clip markers |
| `crates/sue-ve/src/fx/` | effect stacks (`mod.rs`: the model, the kinds' registry, parameters and their keys), their pixels (`pixels.rs`), masks (`mask.rs`), presets (`preset.rs`) |
| `crates/sue-ve/src/compose.rs` | the processor's frame: effects, grade, masks, matte, motion blur, blend, adjustment clips, nesting; `frame_key` for the render cache |
| `crates/sue-ve/src/render.rs` | `layers_at` (what shows, with the transitions' placement), `corners` (anchor point), `draw` (the drawing core), generated sources |
| `crates/sue-ve/src/layers.rs` | shapes and text animators |
| `crates/sue-ve/src/grade.rs`, `scopes.rs` | correctors, the baked table, `.cube` LUTs; the scopes and their worker |
| `crates/sue-ve/src/mix.rs`, `audio.rs` | the mixer with track effects and meters; the equalizer, gate, compressor, BS.1770 loudness |
| `crates/sue-ve/src/preview.rs` | the program monitor's worker and the render cache |
| `crates/sue-ve/src/media/` | import, decode workers (`frames.rs`), drawn frames off the interface's thread (`stills.rs`), sound, thumbnails |
| `crates/sue-ve/src/export.rs` | the export and the `Renderer` the preview shares |
| `crates/sue-ve/src/actions.rs`, `commands.rs`, `menus.rs` | what the commands do, the registry, the menus |
| `crates/sue-ve/src/ui/` | pages (`effects_page.rs`, `color_page.rs`, `audio_page.rs`, `pages.rs`), `mobile.rs`, palettes (`browser.rs`, `controls.rs`, `graph.rs`, `panels.rs`, `inspector.rs`, `pool.rs`, `timeline.rs`), `monitor.rs`, workspaces in `mod.rs` |
| `crates/sue-platform/src/audio.rs` | `AudioInput`, the microphone (cpal) |
| `bins/sue/src/runtime/tests/sve.rs` | runtime tests; `bins/sue/src/snapshot/scenes/sve.rs` the tour (`SUE_SNAPSHOT_ONLY=sve`) |

## How the pieces fit

- **One drawing for preview and export.** A frame whose layers are
  plain (`compose::needs_processing` false) is drawn by the program
  monitor as textured quads. Anything else is drawn by
  `export::Renderer::frame_with`, which calls `compose::frame`, on the
  preview worker at the monitor's size; the export runs the same at its
  size. Tests compare the two (`compose/tests.rs`,
  `the_preview_is_the_export_at_a_smaller_size`, and the runtime test
  `an_export_with_effects_matches_the_preview`).
- **Effects are data.** A new effect is a new `EffectKind` (registered
  with `fx::register`, or added to `builtin()`), a branch in
  `fx::pixels::apply`, its strings (`fx-<id>`, `fx-param-<key>`), and a
  test in `fx/tests.rs`. If it only maps colors, list it in
  `fx::pixels::is_pointwise` so runs of them bake into one table.
- **Keys.** Every number keys on the clip's `Animation`, keyed in source
  time: `fx.<effect>.<param>`, `mask.<index>.<param>`, the clip's own
  properties by name, `ve.remap` for time remapping, `text.range` for a
  text animator.
- **The render cache** keeps frames by `compose::frame_key`, which hashes
  only what shows at that moment, so edits elsewhere keep a stretch
  rendered.

## Extension points

- **GPU effects.** Effects run on the processor (in strips on every
  core). The painter draws textured quads only; a wgpu render pass per
  effect (in `sue-platform`'s painter, a callback or an offscreen
  texture) would move the heavy ones (blur, glow, grade lookup) to the
  card. Keep `compose::frame` as the reference the tests compare
  against.
- **Node graph compositing (Fusion).** The stack is the serial form of a
  chain; a `NodeGraph` per clip (or a `ve.comp` entity) with the same
  registered kinds would give branches, merges and a node editor.
- **Parallel and layer nodes on the Color page**, power windows there
  (reuse `fx::mask`), and the qualifier's matte view (the key is
  `grade::key`).
- **Mask tracking**: `Mask::tracked` is kept; a tracker would key the
  mask's points (path keys are not there yet).
- **Render files.** The render cache is in memory; writing rendered
  stretches to disk (JPEG XL, as Sue's caches do) would keep them across
  runs.
- **Buses and plugins** in the mixer; automation of the track fader
  (track keys would need a track `Animation`).
- **Captions** from speech (`sue-ai`), and CapCut's templates and
  stickers.
- **Expressions** in After Effects' manner.

## Known gaps

- Decoded sound is held in memory (about 23 MB a minute of stereo at
  48 kHz); stream it from disk for long projects.
- Hardware decoding through Direct3D 11 reads every frame back to the
  processor and measured slower than Windows' software decoder here;
  `sue_ve.hardware_decode` stays off by default.
- Movies decode only on Windows; elsewhere `sue_platform::video` reports
  `NotAvailable`.
- Mask shapes do not keyframe (their feather, expansion and opacity do).
- Text animators move letters set one by one where the title lays them
  out; kerning between them is lost.
- The voice over records the default microphone only, and needs a
  computer (phones need their permission flow).
- The STATUS_HEAP_CORRUPTION seen in stage 1's sue-platform tests came
  back once in stage 2, after the decode audit (its fixes are in
  `decode.rs` and `media.rs`), in a `cargo test` of several crates. The
  tests still running then were three in `paint` (the wgpu painter) and
  `packages::tests::a_dropped_download_carries_on_with_a_range_request`;
  those alone (15 runs each) and the whole test binary (460 runs) did
  not crash again. Running the binary under page heap (`gflags /p`)
  would find the writer; it was not run here, as it changes system
  settings.

## Performance (stage 2, this machine, release)

| What | Measured |
| --- | --- |
| 1080p25 shot, 3 effects (blur 6 px, vignette, hue and saturation) and a grade, drawn at full size | 28 ms a frame (36 fps), decoding included |
| The same at half size (playback of frames not rendered yet) | 10 ms a frame (99 fps) |
| 1080p25 export with those effects | 23 fps |
| 1080p25 export without effects | 54 fps |

## How to check your work

```bash
cargo test -p sue-ve
cargo test -p sue --lib -- sve
cargo run -p sue -- --self-check
SUE_SNAPSHOT_ONLY=sve cargo run -p sue -- --snapshot <folder>
cargo test -p sue-ve --release measure -- --ignored --nocapture
```
