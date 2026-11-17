//! Long, heavy strokes: what one costs while the pen is down and when it
//! lifts, and the promises that keep it cheap.
//!
//! The benchmark is ignored (it paints a minute of dense input with
//! every heavy brush on a big canvas); run it in a release build:
//!
//! ```text
//! cargo test --release -p sue-brush --test perf -- --ignored --nocapture
//! ```
//!
//! `SUE_PERF_SECONDS` sets how long each stroke is (60), `SUE_PERF_CASES`
//! keeps the cases whose name holds the text, `SUE_PERF_CANVAS` picks
//! `4000x6000` or `8000x8000` (both by default).
//!
//! The tests beside it are small enough to run every time: the cost of
//! a sample does not grow with the stroke, the stroke holds a bounded
//! number of points, the interface's calls never wait for the worker,
//! and every family of brush lays the same bytes as the same stroke
//! painted the slow way (one sample at a time, on one thread).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sue_brush::input::{Device, Kind, Sample, Settings};
use sue_brush::paint::{Behaviours, DualBrush, Resources};
use sue_brush::texture::Paper;
use sue_brush::worker::{Brush, Canvas, Stroke, StrokeWorker};
use sue_core::{EntityId, JobPool, PixelFormat, RasterTile2D};
use sue_model::BrushPreset;
use sue_model::brush::{EngineKind, param};

// ---------------------------------------------------------------------
// Brushes and strokes
// ---------------------------------------------------------------------

/// The pen's packets a second.
const RATE: f64 = 240.0;
/// The pen's packets a frame of the interface (60 frames a second).
const FRAME: usize = 4;

/// A heavy brush: one of Sue's own, `size` pixels wide, stabilized.
struct Heavy {
    name: &'static str,
    preset: BrushPreset,
}

impl Heavy {
    fn of(name: &'static str, id: &str, size: f32) -> Self {
        let mut preset = sue_brush::presets::find(id).expect("a built-in brush");
        preset.set(param::SIZE, size);
        preset.input.stabilization = preset.input.stabilization.max(20);
        Self { name, preset }
    }

    fn blender(name: &'static str, blender: &str, size: f32) -> Self {
        let mut heavy = Self::of(name, "sue.blend.blend", size);
        heavy.preset.engine = EngineKind::Blend(blender.to_owned());
        heavy
    }

    /// What the worker paints with: the brush and its paper.
    fn brush(&self) -> Brush {
        let behaviours = Behaviours::from_preset(&self.preset);
        let paper = behaviours
            .texture
            .as_ref()
            .and_then(|texture| match &texture.paper {
                sue_brush::paint::PaperChoice::Builtin(name) => Paper::builtin(name),
                sue_brush::paint::PaperChoice::Picture(_) => None,
            });
        let resources = Resources {
            paper: paper.map(Arc::new),
            ..Resources::default()
        };
        Brush::new(self.preset.clone(), [0.12, 0.2, 0.55]).with(resources, [0.9, 0.6, 0.1])
    }
}

/// The families the owner named, each at `scale` of its heavy size.
fn heavies(scale: f32) -> Vec<Heavy> {
    let s = |size: f32| (size * scale).max(4.0);
    let mut dual = Heavy::of("dual brush", "sue.pencil.pastel", s(300.0));
    let mut behaviours = Behaviours::from_preset(&dual.preset);
    behaviours.dual = Some(DualBrush::default());
    behaviours.write_to(&mut dual.preset);
    vec![
        Heavy::of("textured 500", "sue.pencil.pastel", s(500.0)),
        Heavy::of(
            "watercolor edge 300",
            "sue.brush.round_watercolor",
            s(300.0),
        ),
        Heavy::of("oil mixing 300", "sue.brush.oil_paint", s(300.0)),
        Heavy::of("gouache mixing 300", "sue.brush.gouache", s(300.0)),
        dual,
        Heavy::of("spray 400", "sue.airbrush.spray", s(400.0)),
        Heavy::of("soft airbrush 800", "sue.airbrush.soft", s(800.0)),
        Heavy::blender("blur 300", "blur", s(300.0)),
        Heavy::blender("finger tip 300", "finger-tip", s(300.0)),
        Heavy::blender("wet bleed 300", "wet-bleed", s(300.0)),
    ]
}

/// A scribble that never lifts: `seconds` of the pen going round and
/// round over the same `extent` pixels about `center`, pressing in and
/// out. Without the lift.
fn scribble(seconds: f64, center: [f32; 2], extent: f32) -> Vec<Sample> {
    let count = (seconds * RATE).round() as usize;
    (0..=count)
        .map(|i| {
            let t = i as f64 / RATE;
            // Two rates that never line up: the pen crosses its own
            // line again and again. About three extents a second.
            let x = f64::from(center[0]) + f64::from(extent) * (t * 5.1).sin();
            let y = f64::from(center[1]) + f64::from(extent) * (t * 3.7 + 0.6).sin();
            let pressure = 0.55 + 0.4 * (t * 1.3).sin();
            let kind = if i == 0 { Kind::Down } else { Kind::Move };
            Sample::new(kind, [x as f32, y as f32], 1.0 + t).pressing(pressure as f32)
        })
        .collect()
}

/// The lift after `samples`.
fn lift(samples: &[Sample]) -> Sample {
    let last = samples[samples.len() - 1];
    Sample::new(Kind::Up, last.pos, last.time + 1.0 / RATE)
}

/// A layer with bands of color, so a mixing brush has paint to pick up.
fn layer(width: u32, height: u32) -> RasterTile2D {
    let mut row = Vec::with_capacity(width as usize * 4);
    for x in 0..width {
        let rgb = [[200, 60, 40], [40, 160, 90], [230, 210, 60]][(x / 96 % 3) as usize];
        row.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    let mut bytes = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        if y / 128 % 4 == 3 {
            // A clear band now and then.
            bytes.extend(std::iter::repeat_n(0_u8, row.len()));
        } else {
            bytes.extend_from_slice(&row);
        }
    }
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, bytes).expect("a layer")
}

fn settings() -> Settings {
    let mut settings = Settings::new(Device::Pen);
    settings.seed = 0x5EED_0081;
    settings
}

fn canvas_of(layer: &RasterTile2D) -> Canvas {
    Canvas {
        snapshot: Some(layer.clone()),
        ..Canvas::default()
    }
}

// ---------------------------------------------------------------------
// Measuring
// ---------------------------------------------------------------------

/// What one stroke cost.
struct Cost {
    samples: usize,
    /// Each frame's update, in seconds.
    frames: Vec<f64>,
    /// The update that takes the lift.
    close: f64,
    /// The merge into the layer and its undo record.
    commit: f64,
    patches: usize,
    patch_bytes: usize,
    /// The process's memory before the stroke and at its end, in MB.
    memory: [f64; 2],
}

impl Cost {
    fn total(&self) -> f64 {
        self.frames.iter().sum()
    }

    /// The mean cost of a sample over frames `from..to` of ten parts.
    fn tenth(&self, part: usize) -> f64 {
        let n = self.frames.len();
        let (from, to) = (n * part / 10, n * (part + 1) / 10);
        self.frames[from..to].iter().sum::<f64>() / ((to - from) * FRAME) as f64
    }

    fn worst(&self) -> f64 {
        self.frames.iter().copied().fold(0.0, f64::max)
    }
}

/// The memory this process holds, in MB (0 where it cannot be asked).
fn memory_mb() -> f64 {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        return status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<f64>().ok())
            .map_or(0.0, |kb| kb / 1024.0);
    }
    let filter = format!("PID eq {}", std::process::id());
    std::process::Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .output()
        .ok()
        .and_then(|out| {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let field = text.trim().rsplit('"').nth(1)?.to_owned();
            let digits: String = field.chars().filter(char::is_ascii_digit).collect();
            digits.parse::<f64>().ok()
        })
        .map_or(0.0, |kb| kb / 1024.0)
}

/// Draws `samples` and the lift with `brush` on `raster` the way the
/// worker's job does, a frame's samples an update, and times it.
fn measure(
    brush: &Brush,
    samples: &[Sample],
    raster: &mut RasterTile2D,
    pool: &Arc<JobPool>,
) -> Cost {
    let before = memory_mb();
    let bands = pool.threads().clamp(1, 8);
    let mut stroke = Stroke::on(brush, settings(), bands, canvas_of(raster));
    stroke.share(Arc::clone(pool));
    let mut queue = VecDeque::new();
    let mut frames = Vec::with_capacity(samples.len() / FRAME + 1);
    for chunk in samples.chunks(FRAME) {
        queue.extend(chunk.iter().copied());
        let start = Instant::now();
        stroke.update(&mut queue, None, Some(pool)).expect("laid");
        let _ = stroke.take_dirty();
        frames.push(start.elapsed().as_secs_f64());
    }
    let held = memory_mb();
    queue.push_back(lift(samples));
    let start = Instant::now();
    stroke.update(&mut queue, None, Some(pool)).expect("laid");
    let close = start.elapsed().as_secs_f64();
    assert!(stroke.is_done());
    stroke.let_go_of_layer();
    let start = Instant::now();
    let edit = stroke
        .commit(raster, [0, 0], EntityId::from_u128(7), "Stroke")
        .expect("merged");
    let commit = start.elapsed().as_secs_f64();
    Cost {
        samples: samples.len(),
        frames,
        close,
        commit,
        patches: edit.patches().len(),
        patch_bytes: edit
            .patches()
            .iter()
            .map(|patch| patch.before.len() + patch.after.len())
            .sum(),
        memory: [before, held],
    }
}

#[test]
#[ignore = "a benchmark: minutes of painting on a big canvas"]
fn long_heavy_strokes_benchmark() {
    let seconds: f64 = std::env::var("SUE_PERF_SECONDS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(60.0);
    let only = std::env::var("SUE_PERF_CASES").unwrap_or_default();
    let canvas = std::env::var("SUE_PERF_CANVAS").unwrap_or_default();
    let pool = Arc::new(JobPool::for_this_machine());
    println!(
        "{seconds} s strokes at {RATE} Hz, {FRAME} samples a frame, {} threads",
        pool.threads()
    );
    println!(
        "{:<22} {:>9} {:>8} {:>9} {:>9} {:>9} {:>6} {:>9} {:>9} {:>9} {:>8} {:>9} {:>9}",
        "brush",
        "canvas",
        "samples",
        "total s",
        "us/sample",
        "first 10%",
        "last/1st",
        "worst ms",
        "behind s",
        "close ms",
        "commit ms",
        "undo MB",
        "mem +MB"
    );
    for (width, height) in [(4000_u32, 6000_u32), (8000, 8000)] {
        let name = format!("{width}x{height}");
        if !canvas.is_empty() && canvas != name {
            continue;
        }
        let base = layer(width, height);
        let center = [width as f32 / 2.0, height as f32 / 2.0];
        let samples = scribble(seconds, center, 900.0);
        for heavy in heavies(1.0) {
            if !only.is_empty() && !heavy.name.contains(&only) {
                continue;
            }
            let mut raster = base.clone();
            let cost = measure(&heavy.brush(), &samples, &mut raster, &pool);
            let total = cost.total();
            println!(
                "{:<22} {:>9} {:>8} {:>9.2} {:>9.1} {:>9.1} {:>6.2} {:>9.1} {:>9.1} {:>9.1} {:>8.1} {:>9.1} {:>9.1}",
                heavy.name,
                name,
                cost.samples,
                total,
                total / cost.samples as f64 * 1e6,
                cost.tenth(0) * 1e6,
                cost.tenth(9) / cost.tenth(0).max(1e-12),
                cost.worst() * 1e3,
                // How far behind the pen the worker is at the lift: what
                // a lift that waits for it waits.
                (total - seconds).max(0.0),
                cost.close * 1e3,
                cost.commit * 1e3,
                cost.patch_bytes as f64 / 1_048_576.0,
                cost.memory[1] - cost.memory[0],
            );
            let _ = cost.patches;
        }
    }
}

/// Where a stroke's time goes, stage by stage: the stroke driven as the
/// worker's painter drives it, each stage timed.
#[test]
#[ignore = "a profile: prints where a heavy stroke's time goes"]
fn where_the_time_goes() {
    use sue_brush::buffer::{TILE, TileKey};
    use sue_brush::input::Pipeline;
    use sue_brush::paint::lay::lay_field;
    use sue_brush::paint::wet::{Backdrop, RasterBackdrop};
    use sue_brush::paint::{Colors, PaintStroke};
    use sue_brush::raster::PixelRect;
    let seconds: f64 = std::env::var("SUE_PERF_SECONDS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(3.0);
    let only = std::env::var("SUE_PERF_CASES").unwrap_or_default();
    let base = layer(4000, 6000);
    let samples = scribble(seconds, [2000.0, 3000.0], 900.0);
    println!(
        "{:<22} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>7} {:>7}",
        "brush", "push", "p.tail", "commit", "hold", "s.tail", "dirty", "laid", "points", "tiles"
    );
    for heavy in heavies(1.0) {
        if !only.is_empty() && !heavy.name.contains(&only) {
            continue;
        }
        let brush = heavy.brush();
        let preset = &brush.preset;
        let mut pipeline = Pipeline::new(preset, &sue_brush::paint::registry(), settings());
        let mut stroke = PaintStroke::new(
            preset,
            &Behaviours::from_preset(preset),
            brush.resources.clone(),
            Colors {
                main: brush.color,
                sub: brush.sub_color,
            },
            settings().seed,
            pipeline.extra_keys(),
        );
        let wet = stroke.is_wet();
        let backdrop = RasterBackdrop {
            raster: &base,
            origin: [0, 0],
        };
        let backdrop: Option<&dyn Backdrop> = wet.then_some(&backdrop as &dyn Backdrop);
        let mut t = [0.0_f64; 7];
        let mut done = 0;
        let mut tiles = 0;
        let timed = |slot: &mut f64, start: Instant| *slot += start.elapsed().as_secs_f64();
        for chunk in samples.chunks(FRAME) {
            for sample in chunk {
                let start = Instant::now();
                pipeline.push(*sample);
                timed(&mut t[0], start);
                let committed = pipeline.committed();
                if committed.len() > done {
                    let start = Instant::now();
                    stroke.commit(&committed[done..], backdrop);
                    timed(&mut t[2], start);
                    done = committed.len();
                }
                let start = Instant::now();
                let tail = pipeline.tail();
                timed(&mut t[1], start);
                let start = Instant::now();
                stroke.hold(sample.time, tail.last(), backdrop);
                timed(&mut t[3], start);
            }
            let start = Instant::now();
            let tail = pipeline.tail();
            timed(&mut t[1], start);
            let start = Instant::now();
            stroke.tail(&tail);
            timed(&mut t[4], start);
            let start = Instant::now();
            let dirty: Vec<TileKey> = stroke.take_dirty();
            timed(&mut t[5], start);
            tiles += dirty.len();
            let start = Instant::now();
            for key in dirty {
                let [ox, oy] = key.origin();
                if wet {
                    let back = backdrop.expect("a backdrop");
                    for y in 0..TILE {
                        for x in 0..TILE {
                            if stroke.wet_touched(ox + x, oy + y) {
                                std::hint::black_box(stroke.wet_pixel(back, ox + x, oy + y, false));
                            }
                        }
                    }
                } else {
                    let rect = PixelRect {
                        x0: ox,
                        y0: oy,
                        x1: ox + TILE,
                        y1: oy + TILE,
                    };
                    let field = stroke.field(rect, true).expect("a field");
                    let mut tile = RasterTile2D::new(TILE as u32, TILE as u32, PixelFormat::Rgba8)
                        .expect("a tile");
                    lay_field(&field, &mut tile, [ox, oy], stroke.lay(), false).expect("laid");
                }
            }
            timed(&mut t[6], start);
        }
        println!(
            "{:<22} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>7} {:>7}",
            heavy.name, t[0], t[1], t[2], t[3], t[4], t[5], t[6], done, tiles
        );
    }
}

// ---------------------------------------------------------------------
// The same bytes as ever
// ---------------------------------------------------------------------

/// FNV-1a over `bytes`.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The slow way: every sample an update of its own, on this thread
/// alone, the tail drawn after each.
fn slow(brush: &Brush, samples: &[Sample], raster: &mut RasterTile2D) {
    let mut stroke = Stroke::on(brush, settings(), 1, canvas_of(raster));
    let mut queue = VecDeque::new();
    for sample in samples.iter().copied().chain([lift(samples)]) {
        queue.push_back(sample);
        stroke.update(&mut queue, None, None).expect("laid");
    }
    stroke.let_go_of_layer();
    stroke
        .commit(raster, [0, 0], EntityId::from_u128(7), "Stroke")
        .expect("merged");
}

/// The quick way: the worker on a pool of `threads`, the samples
/// pushed a frame's worth at a time as fast as they come.
fn quick(brush: &Brush, samples: &[Sample], raster: &mut RasterTile2D, threads: usize) {
    let pool = Arc::new(JobPool::new(threads));
    let mut worker = StrokeWorker::start_on(pool, brush, settings(), canvas_of(raster));
    for chunk in samples.chunks(FRAME) {
        worker.push_all(chunk);
        let _ = worker.poll();
    }
    worker.push(lift(samples));
    let mut stroke = worker.finish().expect("a stroke");
    stroke.let_go_of_layer();
    stroke
        .commit(raster, [0, 0], EntityId::from_u128(7), "Stroke")
        .expect("merged");
}

/// A small canvas, the heavy brushes at `scale` of their size and a
/// scribble of `seconds` over it.
fn small(scale: f32, seconds: f64) -> (RasterTile2D, Vec<Heavy>, Vec<Sample>) {
    let side = (3000.0 * scale) as u32;
    let half = side as f32 / 2.0;
    (
        layer(side, side),
        heavies(scale),
        scribble(seconds, [half, half], 900.0 * scale),
    )
}

/// What every heavy brush laid before the engine was made quicker
/// (recorded on the 0.0.81 candidate, the slow way): `SUE_PERF_HASHES=1`
/// prints the table instead of holding it.
const AS_EVER: &[(&str, u64)] = &[
    ("textured 500", 0x95f6_4637_a36b_0ae3),
    ("watercolor edge 300", 0xe13b_2edd_04b7_cba2),
    ("oil mixing 300", 0x0af8_20ce_4762_aa72),
    ("gouache mixing 300", 0x852a_1eb6_9436_8f31),
    ("dual brush", 0x4a9c_5276_3799_1745),
    ("spray 400", 0xd064_b0f8_a900_43db),
    ("soft airbrush 800", 0xfb14_fdcd_24f2_8731),
    ("blur 300", 0x1db3_5a51_bad3_d038),
    ("finger tip 300", 0x8bf5_8edb_6731_6ba1),
    ("wet bleed 300", 0x5fca_2a2e_845b_3c11),
];

#[test]
fn heavy_brushes_lay_the_bytes_they_always_did() {
    let (base, brushes, samples) = small(0.2, 1.5);
    let print = std::env::var("SUE_PERF_HASHES").is_ok();
    for heavy in brushes {
        let brush = heavy.brush();
        let mut reference = base.clone();
        slow(&brush, &samples, &mut reference);
        let hash = fnv(reference.bytes().expect("bytes"));
        assert_ne!(hash, fnv(base.bytes().expect("bytes")), "{}", heavy.name);
        if print {
            println!("    (\"{}\", 0x{hash:016x}),", heavy.name);
            continue;
        }
        let expected = AS_EVER.iter().find(|(name, _)| *name == heavy.name);
        assert_eq!(
            expected.map(|(_, hash)| *hash),
            Some(hash),
            "{}: the bytes changed",
            heavy.name
        );
        for threads in [1, 4] {
            let mut raster = base.clone();
            quick(&brush, &samples, &mut raster, threads);
            assert!(
                raster.bytes() == reference.bytes(),
                "{}: the worker on {threads} threads lays other bytes than the slow way",
                heavy.name
            );
        }
    }
}

/// [`AS_EVER`] at the brushes' full size, one second of scribble.
const FULL_AS_EVER: &[(&str, u64)] = &[
    ("textured 500", 0x6579_9283_408e_e4eb),
    ("watercolor edge 300", 0xe2ff_3b4a_07a1_ce44),
    ("oil mixing 300", 0x6fe6_e396_814b_4ce9),
    ("gouache mixing 300", 0xb29a_2e16_7c23_5391),
    ("dual brush", 0x3e0e_270d_f584_0127),
    ("spray 400", 0x47ca_3121_49e1_1afe),
    ("soft airbrush 800", 0x2ba2_d30f_9bf6_7d16),
    ("blur 300", 0x4071_f4aa_062b_1463),
    ("finger tip 300", 0xbabd_4ff7_08f6_8c72),
    ("wet bleed 300", 0x8414_dcec_560c_8327),
];

/// The same check at the brushes' full size, for running by hand.
#[test]
#[ignore = "slow: full size brushes painted twice"]
fn full_size_brushes_lay_the_same_bytes_the_slow_way() {
    let seconds: f64 = std::env::var("SUE_PERF_SECONDS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(1.0);
    let (base, brushes, samples) = small(1.0, seconds);
    for heavy in brushes {
        let brush = heavy.brush();
        let mut reference = base.clone();
        slow(&brush, &samples, &mut reference);
        let mut raster = base.clone();
        quick(&brush, &samples, &mut raster, 6);
        let hash = fnv(reference.bytes().expect("bytes"));
        println!("    (\"{}\", 0x{hash:016x}),", heavy.name);
        assert!(raster.bytes() == reference.bytes(), "{}", heavy.name);
        if (seconds - 1.0).abs() < 1.0e-9 {
            let expected = FULL_AS_EVER.iter().find(|(name, _)| *name == heavy.name);
            assert_eq!(
                expected.map(|(_, hash)| *hash),
                Some(hash),
                "{}",
                heavy.name
            );
        }
    }
}

// Kept for the tests below.
#[allow(dead_code)]
fn unused(_: Duration) {}
