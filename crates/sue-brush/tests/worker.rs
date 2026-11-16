//! The stroke worker from the outside: a stroke laid on the shared pool,
//! shown live, merged as one edit that undoes exactly, given up without a
//! trace, the same pixels on every thread count, and the time it takes.
//!
//! Set `SUE_BRUSH_SHOTS` to a folder to have the live and the committed
//! strokes drawn there as PNG pictures, for looking at by eye.

use std::sync::Arc;
use std::time::{Duration, Instant};

use sue_brush::buffer::TILE;
use sue_brush::input::{Device, Kind, Sample, Settings};
use sue_brush::worker::{Brush, SLICE_PIXELS, Stroke, StrokeWorker, shared_pool};
use sue_core::txn::RasterEdit;
use sue_core::{
    Arena, ComponentData, ComponentKind, EntityId, JobPool, PixelFormat, RasterTile2D, UndoStack,
};
use sue_model::BrushPreset;
use sue_model::brush::{Binding, Taper, TaperMode, input, param};

// ---------------------------------------------------------------------
// Brushes and strokes to test with
// ---------------------------------------------------------------------

/// The device's packets a second.
const RATE: f64 = 200.0;

/// A round pen of `size` whose size follows pressure from 15 %, with
/// `hardness` and a taper at both ends (10 and 30 px of the line).
fn pen(size: f32, hardness: f32) -> BrushPreset {
    let mut preset = BrushPreset::new("Test pen");
    preset.set(param::SIZE, size);
    preset.set(param::HARDNESS, hardness);
    preset
        .dynamics
        .push(Binding::new(param::SIZE, input::PRESSURE).range(0.15, 1.0));
    preset.input.stabilization = 10;
    preset.taper = Taper {
        start: 10.0,
        end: 30.0,
        mode: TaperMode::Length,
        ..Taper::default()
    };
    preset
}

/// A pencil: lighter, its opacity following pressure too.
fn pencil(size: f32) -> BrushPreset {
    let mut preset = pen(size, 0.8);
    preset.set(param::OPACITY, 0.9);
    preset.set(param::FLOW, 0.85);
    preset
        .dynamics
        .push(Binding::new(param::OPACITY, input::PRESSURE).range(0.3, 1.0));
    preset
}

/// A wavy stroke from `from` to `to`, `seconds` long, pressing in and out,
/// lifted at its end.
fn wave(from: [f32; 2], to: [f32; 2], seconds: f64, swing: f32) -> Vec<Sample> {
    let count = (seconds * RATE).round() as usize;
    let mut samples = Vec::new();
    for i in 0..=count {
        let s = i as f64 / count as f64;
        let x = from[0] + (to[0] - from[0]) * s as f32;
        let y = from[1] + (to[1] - from[1]) * s as f32 + swing * (s * 9.0).sin() as f32;
        let pressure = (0.25 + 0.7 * (s * std::f64::consts::PI).sin()) as f32;
        let kind = if i == 0 { Kind::Down } else { Kind::Move };
        samples.push(Sample::new(kind, [x, y], 1.0 + i as f64 / RATE).pressing(pressure));
    }
    let last = *samples.last().unwrap();
    samples.push(Sample::new(Kind::Up, last.pos, last.time + 1.0 / RATE));
    samples
}

/// A layer of `width` by `height` with a pattern in every byte.
fn patterned(width: u32, height: u32) -> RasterTile2D {
    let bytes = (0..width * height)
        .flat_map(|i| {
            let (x, y) = (i % width, i / width);
            [
                (x * 3) as u8,
                (y * 5) as u8,
                ((x ^ y) * 7) as u8,
                if (x / 16 + y / 16) % 3 == 0 { 0 } else { 255 },
            ]
        })
        .collect();
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, bytes).unwrap()
}

fn white(width: u32, height: u32) -> RasterTile2D {
    let bytes = vec![255; (width * height * 4) as usize];
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, bytes).unwrap()
}

/// Draws `samples` with `brush` on `pool`, `per_frame` samples a poll,
/// and answers the finished stroke.
fn draw(pool: Arc<JobPool>, brush: &Brush, samples: &[Sample], per_frame: usize) -> Stroke {
    let mut worker = StrokeWorker::start(pool, brush, Settings::new(Device::Pen));
    for frame in samples.chunks(per_frame) {
        worker.push_all(frame);
        let _ = worker.poll();
    }
    worker.finish().expect("the stroke finished")
}

fn black(preset: BrushPreset) -> Brush {
    Brush::new(preset, [0.0, 0.0, 0.0])
}

// ---------------------------------------------------------------------
// Undo, and a stroke given up
// ---------------------------------------------------------------------

fn layer_bytes(arena: &Arena, layer: sue_core::AssetHandle) -> Vec<u8> {
    arena
        .get(layer)
        .unwrap()
        .component(ComponentKind::RasterTile2D)
        .unwrap()
        .as_raster()
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec()
}

/// Merges `stroke` into the layer's raster, at `origin`, as the canvas
/// does: in place, answering the edit.
fn commit(
    arena: &mut Arena,
    layer: sue_core::AssetHandle,
    stroke: &Stroke,
    origin: [i32; 2],
) -> RasterEdit {
    let id: EntityId = arena.get(layer).unwrap().id();
    let entity = arena.get_mut(layer).unwrap();
    let Some(ComponentData::RasterTile2D(raster)) =
        entity.component_mut(ComponentKind::RasterTile2D)
    else {
        panic!("no raster");
    };
    stroke.commit(raster, origin, id, "Stroke").unwrap()
}

#[test]
fn a_finished_stroke_reverts_exactly() {
    let mut arena = Arena::new();
    let layer = arena.spawn("layer", patterned(300, 200));
    let before = layer_bytes(&arena, layer);
    let samples = wave([20.0, 60.0], [330.0, 150.0], 0.5, 25.0);
    for (preset, threads) in [(pen(18.0, 1.0), 2), (pencil(40.0), 4)] {
        let stroke = draw(Arc::new(JobPool::new(threads)), &black(preset), &samples, 3);
        // The layer sits 10 pixels right of the picture's corner and 5 up:
        // the stroke is in layer pixels and reaches past its right edge.
        let edit = commit(&mut arena, layer, &stroke, [-10, 5]);
        assert!(!edit.is_empty());
        assert!(
            edit.patches()
                .iter()
                .all(|p| p.width <= TILE as u32 && p.height <= TILE as u32),
            "one patch per tile"
        );
        let painted = layer_bytes(&arena, layer);
        assert_ne!(painted, before);
        let mut history = UndoStack::new();
        history.record(edit);
        history.undo(&mut arena).unwrap();
        assert_eq!(
            layer_bytes(&arena, layer),
            before,
            "undo restores every byte"
        );
        history.redo(&mut arena).unwrap();
        assert_eq!(layer_bytes(&arena, layer), painted, "redo lays it again");
        history.undo(&mut arena).unwrap();
    }
}

#[test]
fn an_abandoned_stroke_leaves_nothing() {
    let pool = Arc::new(JobPool::new(2));
    let mut worker = StrokeWorker::start(pool, &black(pen(30.0, 1.0)), Settings::new(Device::Pen));
    let samples = wave([20.0, 60.0], [250.0, 100.0], 0.4, 10.0);
    let live = worker.live();
    let mut shown = false;
    for frame in samples[..samples.len() / 2].chunks(4) {
        worker.push_all(frame);
        let _ = worker.poll();
        while worker.is_busy() {
            std::thread::yield_now();
            let _ = worker.poll();
        }
        shown |= live.bounds().is_some();
    }
    assert!(shown, "the stroke was shown while it was drawn");
    assert!(live.coverage(20, 60) > 0);
    worker.abandon();
    assert!(live.is_ended());
    assert_eq!(live.bounds(), None, "nothing is shown any more");
    assert_eq!(live.coverage(20, 60), 0);
    assert_eq!(
        live.over(20, 60, [0.5, 0.25, 1.0, 1.0]),
        [0.5, 0.25, 1.0, 1.0]
    );
}

/// Strokes given up while their job runs, each followed at once by the
/// next stroke, on a pool of two threads: a stroke job waits for its
/// bands on the same pool, so two such jobs at once could take both
/// threads and wait for ever. Every stroke here still finishes.
#[test]
fn strokes_given_up_mid_job_never_stall_the_pool() {
    let pool = Arc::new(JobPool::new(2));
    let brush = black(pen(400.0, 0.0));
    let samples = wave([0.0, 300.0], [900.0, 340.0], 0.3, 60.0);
    for round in 0..40 {
        let mut worker = StrokeWorker::start(Arc::clone(&pool), &brush, Settings::new(Device::Pen));
        worker.push_all(&samples[..samples.len() / 2]);
        let _ = worker.poll();
        if round % 2 == 0 {
            worker.abandon();
        } else {
            drop(worker);
        }
    }
    let stroke = draw(pool, &brush, &samples, 4);
    assert!(stroke.is_done());
}

// ---------------------------------------------------------------------
// The same pixels, however it is laid
// ---------------------------------------------------------------------

#[test]
fn pixels_are_the_same_for_one_two_and_eight_threads() {
    let samples = wave([30.0, 200.0], [600.0, 260.0], 0.6, 60.0);
    for (name, preset) in [
        ("hard pen", pen(12.0, 1.0)),
        ("soft brush", pen(300.0, 0.0)),
        ("pencil", pencil(60.0)),
    ] {
        let brush = black(preset);
        let lay = |pool: Arc<JobPool>, per_frame: usize| {
            let stroke = draw(pool, &brush, &samples, per_frame);
            let mut raster = white(700, 500);
            let edit = stroke
                .commit(&mut raster, [0, 0], EntityId::new(), "Stroke")
                .unwrap();
            assert!(!edit.is_empty(), "{name}");
            raster.bytes().unwrap().to_vec()
        };
        let one = lay(Arc::new(JobPool::new(1)), 1);
        for (threads, per_frame) in [(2, 1), (8, 1), (8, 5), (2, 13)] {
            assert!(
                one == lay(Arc::new(JobPool::new(threads)), per_frame),
                "{name}: {threads} threads, {per_frame} samples a frame"
            );
        }
        assert!(one == lay(Arc::new(JobPool::inline()), 3), "{name}: inline");
    }
}

#[test]
fn the_live_stroke_shows_what_the_merge_lays() {
    let samples = wave([20.0, 80.0], [400.0, 120.0], 0.4, 30.0);
    let mut brush = black(pencil(36.0));
    brush.color = [0.1, 0.3, 0.8];
    let stroke = draw(Arc::new(JobPool::new(4)), &brush, &samples, 2);
    let live = stroke.live();
    let mut raster = white(440, 200);
    stroke
        .commit(&mut raster, [0, 0], EntityId::new(), "Stroke")
        .unwrap();
    let bytes = raster.bytes().unwrap();
    let mut covered = 0;
    for y in 0..200 {
        for x in 0..440 {
            let shown = live.over(i64::from(x), i64::from(y), [1.0; 4]);
            let i = ((y * 440 + x) * 4) as usize;
            for c in 0..4 {
                let merged = f32::from(bytes[i + c]) / 255.0;
                assert!(
                    (shown[c] - merged).abs() <= 0.6 / 255.0,
                    "({x}, {y}) channel {c}: {} shown, {merged} merged",
                    shown[c]
                );
            }
            covered += usize::from(live.coverage(i64::from(x), i64::from(y)) > 0);
        }
    }
    assert!(covered > 5000, "{covered}");
    shoot("live-and-merged", &raster, &live_picture(&stroke, 440, 200));
}

/// A pencil whose opacity follows a pressure that falls evenly lays an
/// even gradient along its middle: no step where one capsule of the
/// stroke meets the next.
#[test]
fn an_opacity_that_follows_pressure_fades_without_steps() {
    let mut preset = pencil(40.0);
    preset.taper = Taper::default();
    let count = 200;
    let mut samples: Vec<Sample> = (0..=count)
        .map(|i| {
            let s = i as f32 / count as f32;
            let kind = if i == 0 { Kind::Down } else { Kind::Move };
            Sample::new(kind, [40.0 + 600.0 * s, 60.0], 1.0 + f64::from(i) / RATE)
                .pressing(1.0 - 0.8 * s)
        })
        .collect();
    let last = *samples.last().unwrap();
    samples.push(Sample::new(Kind::Up, last.pos, last.time + 1.0 / RATE));
    let stroke = draw(Arc::new(JobPool::new(2)), &black(preset), &samples, 4);
    let mut raster = white(700, 120);
    stroke
        .commit(&mut raster, [0, 0], EntityId::new(), "Stroke")
        .unwrap();
    let bytes = raster.bytes().unwrap();
    let row: Vec<i32> = (100..600)
        .map(|x| i32::from(bytes[(60 * 700 + x) * 4]))
        .collect();
    for pair in row.windows(2) {
        assert!(
            (pair[1] - pair[0]).abs() <= 1,
            "a step along the middle: {:?}",
            row
        );
    }
    assert!(row[row.len() - 1] - row[0] > 40, "it fades: {row:?}");
}

/// How the pen leaves the tablet at the end of a straight line.
#[derive(Clone, Copy, Debug)]
enum Lift {
    /// A mouse, pressing fully all along.
    Mouse,
    /// The pressure falls from 0.5 to 0.05 over the last 40 ms.
    Ramp,
    /// Lifted while still pressing 0.5.
    Abrupt,
    /// The driver stops sending pressure for the last packets.
    Silent,
}

/// The owner's first complaint: a line that ends in a blob. A straight
/// line through the worker, for every way of lifting, stabilized or not,
/// with and without an end taper: no column near the end, live before
/// the lift or merged after it, is wider than the line's body.
#[test]
fn no_stroke_ends_wider_than_its_body() {
    let (width, height) = (560_u32, 120_u32);
    for lift in [Lift::Mouse, Lift::Ramp, Lift::Abrupt, Lift::Silent] {
        for stabilization in [0, 20, 60] {
            for end_taper in [0.0, 40.0] {
                let mut preset = pen(16.0, 1.0);
                preset.input.stabilization = stabilization;
                preset.taper = Taper {
                    end: end_taper,
                    mode: TaperMode::Length,
                    ..Taper::default()
                };
                let count = 120;
                let mut samples: Vec<Sample> = (0..=count)
                    .map(|i| {
                        let s = f64::from(i) / f64::from(count);
                        let kind = if i == 0 { Kind::Down } else { Kind::Move };
                        let sample = Sample::new(
                            kind,
                            [40.0 + 460.0 * s as f32, 60.0],
                            1.0 + f64::from(i) / RATE,
                        );
                        let last = s > 1.0 - 8.0 / f64::from(count);
                        match lift {
                            Lift::Mouse => sample.pressing(1.0),
                            Lift::Ramp if last => {
                                let t =
                                    (s - (1.0 - 8.0 / f64::from(count))) * f64::from(count) / 8.0;
                                sample.pressing((0.5 - 0.45 * t) as f32)
                            }
                            Lift::Silent if last => sample,
                            _ => sample.pressing(if s < 0.05 { 0.1 + 8.0 * s as f32 } else { 0.5 }),
                        }
                    })
                    .collect();
                let last = *samples.last().unwrap();
                samples.push(Sample::new(Kind::Up, last.pos, last.time + 1.0 / RATE));
                let device = if matches!(lift, Lift::Mouse) {
                    Device::MOUSE
                } else {
                    Device::Pen
                };
                let brush = black(preset);
                let mut worker =
                    StrokeWorker::start(Arc::new(JobPool::new(2)), &brush, Settings::new(device));
                let (body, up) = samples.split_at(samples.len() - 1);
                for frame in body.chunks(3) {
                    worker.push_all(frame);
                    let _ = worker.poll();
                }
                while worker.is_busy() || worker.waiting() > 0 {
                    std::thread::yield_now();
                    let _ = worker.poll();
                }
                let live = worker.live();
                let columns = |at: &dyn Fn(i32, i32) -> u16| -> Vec<u32> {
                    (0..width as i32)
                        .map(|x| (0..height as i32).filter(|&y| at(x, y) >= 32_768).count() as u32)
                        .collect()
                };
                let shown = columns(&|x, y| live.coverage(i64::from(x), i64::from(y)));
                worker.push_all(up);
                let stroke = worker.finish().unwrap();
                let merged = columns(&|x, y| stroke.coverage(x, y));
                let what =
                    format!("{lift:?}, stabilization {stabilization}, end taper {end_taper}");
                for (name, widths) in [("live", &shown), ("merged", &merged)] {
                    let mut body: Vec<u32> = widths[150..350].to_vec();
                    body.sort_unstable();
                    let body = body[body.len() / 2];
                    assert!(body >= 6, "{what}, {name}: a body {body} wide");
                    let end = widths[350..].iter().copied().max().unwrap_or(0);
                    assert!(
                        end <= body + 1,
                        "{what}, {name}: the end is {end} px wide, the body {body}"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------

/// The lay time of one frame's samples, for each frame of `samples` cut
/// `per_frame` at a time, waited for at once.
fn frame_times(
    pool: &Arc<JobPool>,
    brush: &Brush,
    samples: &[Sample],
    per_frame: usize,
) -> Vec<Duration> {
    let mut worker = StrokeWorker::start(Arc::clone(pool), brush, Settings::new(Device::Pen));
    let mut times = Vec::new();
    for frame in samples.chunks(per_frame) {
        let start = Instant::now();
        worker.push_all(frame);
        let _ = worker.poll();
        while worker.is_busy() || worker.waiting() > 0 {
            std::thread::yield_now();
            let _ = worker.poll();
        }
        times.push(start.elapsed());
    }
    if let Some(stroke) = worker.finish() {
        let frames = times.len().max(1) as u64;
        PIXELS.store(
            stroke.counters().pixels / frames,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    times
}

/// The pixels the last [`frame_times`] laid a frame.
static PIXELS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort_unstable();
    times[times.len() / 2]
}

/// A 1,000 px soft brush drawn at a steady 1,500 px a second, a frame of
/// 60 a second at a time (about 3 packets): the stroke must keep up with
/// the screen. The hard bound is loose (other work shares the machine
/// while the tests run); `brush_latency_table` prints the numbers.
#[test]
fn a_1000_px_soft_brush_keeps_up_with_the_frames() {
    let pool = shared_pool();
    let brush = black(pen(1_000.0, 0.0));
    let samples = wave([0.0, 600.0], [1_500.0, 700.0], 1.0, 80.0);
    let times = frame_times(&pool, &brush, &samples, 3);
    let typical = median(times[10..].to_vec());
    assert!(
        typical < Duration::from_millis(40),
        "a 1,000 px soft brush takes {typical:?} a frame on {} threads",
        pool.threads()
    );
}

/// On a pool without threads (the browser) each poll lays one slice:
/// about [`SLICE_PIXELS`] of work, then the frame goes on. The budget is
/// held as work (the pixels a slice works out), which does not depend on
/// how busy the machine is; the time is printed, and held only loosely
/// on the typical slice.
#[test]
fn one_slice_on_the_interface_thread_stays_in_its_budget() {
    let pool = Arc::new(JobPool::inline());
    let brush = black(pen(400.0, 0.5));
    let samples = wave([0.0, 300.0], [1_200.0, 340.0], 0.8, 150.0);
    let mut worker = StrokeWorker::start(pool, &brush, Settings::new(Device::Pen));
    // A whole stroke's samples arrive at once, as after a stall.
    worker.push_all(&samples);
    let mut times = Vec::new();
    let mut most = 0;
    let mut laid = 0;
    while worker.waiting() > 0 {
        let start = Instant::now();
        let _ = worker.poll();
        times.push(start.elapsed());
        let now = worker
            .counters()
            .expect("an inline pool keeps the stroke")
            .pixels;
        most = most.max(now - laid);
        laid = now;
        assert!(times.len() < 1_000, "the slices make progress");
    }
    let slices = times.len();
    assert!(
        slices > 1,
        "the work was cut into slices of about {SLICE_PIXELS} pixels"
    );
    // A slice stops once its shapes reach the budget: it may finish the
    // sample under way, never much more (the provisional tail is drawn
    // only by the slice that empties the queue).
    assert!(
        most <= SLICE_PIXELS + SLICE_PIXELS / 2,
        "a slice worked out {most} pixels, the budget is {SLICE_PIXELS}"
    );
    let slowest = times.iter().max().copied().unwrap_or_default();
    let typical = median(times);
    println!("{slices} slices, at most {most} pixels; typical {typical:?}, slowest {slowest:?}");
    // A quarter of a megapixel is a few milliseconds optimized: held
    // loosely, as other builds share the machine while tests run.
    assert!(typical < Duration::from_millis(30), "{typical:?}");
    let stroke = worker.finish().unwrap();
    assert!(stroke.is_done());
}

/// The table of how long a frame of a stroke takes, by brush size and
/// threads. `cargo test -p sue-brush --release --test worker -- --ignored
/// --nocapture brush_latency_table`.
#[test]
#[ignore = "a benchmark: prints a table"]
fn brush_latency_table() {
    let samples = wave([0.0, 600.0], [1_500.0, 700.0], 1.0, 80.0);
    println!("size  hardness  threads  median ms  slowest ms  pixels a frame");
    for size in [10.0, 100.0, 300.0, 1_000.0] {
        for hardness in [1.0, 0.0] {
            for pool in [Arc::new(JobPool::new(1)), shared_pool()] {
                let times = frame_times(&pool, &black(pen(size, hardness)), &samples, 3);
                let slowest = times.iter().max().copied().unwrap_or_default();
                println!(
                    "{size:>5} {hardness:>9} {:>8} {:>10.2} {:>11.2} {:>15}",
                    pool.threads(),
                    median(times).as_secs_f64() * 1000.0,
                    slowest.as_secs_f64() * 1000.0,
                    PIXELS.load(std::sync::atomic::Ordering::Relaxed)
                );
            }
        }
    }
}

// ---------------------------------------------------------------------
// Pictures for looking at
// ---------------------------------------------------------------------

/// The stroke as the compositor shows it before the lift: its live view
/// over white.
fn live_picture(stroke: &Stroke, width: u32, height: u32) -> RasterTile2D {
    let live = stroke.live();
    let mut bytes = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let p = live.over(i64::from(x), i64::from(y), [1.0; 4]);
            bytes.extend(p.map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
        }
    }
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, bytes).unwrap()
}

/// Writes `merged` over `live` (each over white) into
/// `$SUE_BRUSH_SHOTS/<name>.png`.
fn shoot(name: &str, merged: &RasterTile2D, live: &RasterTile2D) {
    let Some(folder) = std::env::var_os("SUE_BRUSH_SHOTS") else {
        return;
    };
    let (width, height) = (merged.width() as usize, merged.height() as usize);
    let mut pixels = Vec::with_capacity(width * height * 2);
    for raster in [live, merged] {
        for p in raster.bytes().unwrap().chunks(4) {
            let a = f32::from(p[3]) / 255.0;
            let over = |c: u8| (f32::from(c) * a + 255.0 * (1.0 - a)).round() as u8;
            pixels.push([over(p[0]), over(p[1]), over(p[2])]);
        }
    }
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        std::path::Path::new(&folder).join(format!("{name}.png")),
        png(width, height * 2, &pixels),
    )
    .unwrap();
}

/// Strokes of every kind side by side, live and committed, for the eye.
#[test]
fn a_sheet_of_strokes_live_and_committed() {
    if std::env::var_os("SUE_BRUSH_SHOTS").is_none() {
        return;
    }
    let (width, height) = (900_u32, 420_u32);
    let mut merged = white(width, height);
    let mut live = white(width, height);
    let rows: [(BrushPreset, f32); 5] = [
        (pen(4.0, 1.0), 40.0),
        (pen(14.0, 1.0), 110.0),
        (pencil(10.0), 180.0),
        (pencil(40.0), 260.0),
        (pen(90.0, 0.0), 350.0),
    ];
    for (preset, y) in rows {
        let samples = wave([40.0, y], [860.0, y + 10.0], 0.7, 18.0);
        let brush = black(preset);
        let mut worker = StrokeWorker::start(shared_pool(), &brush, Settings::new(Device::Pen));
        // The live picture: everything but the lift, as it stands.
        let (body, up) = samples.split_at(samples.len() - 1);
        for frame in body.chunks(3) {
            worker.push_all(frame);
            let _ = worker.poll();
        }
        while worker.is_busy() {
            std::thread::yield_now();
            let _ = worker.poll();
        }
        let view = worker.live();
        let bytes = live.bytes_mut().unwrap();
        for py in 0..height as i64 {
            for px in 0..width as i64 {
                let i = ((py * i64::from(width) + px) * 4) as usize;
                let under = [0, 1, 2, 3].map(|c| f32::from(bytes[i + c]) / 255.0);
                let p = view.over(px, py, under);
                for c in 0..4 {
                    bytes[i + c] = (p[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        worker.push_all(up);
        let stroke = worker.finish().unwrap();
        stroke
            .commit(&mut merged, [0, 0], EntityId::new(), "Stroke")
            .unwrap();
    }
    shoot("sheet", &merged, &live);
}

/// An RGB picture as a PNG with stored (uncompressed) deflate blocks.
fn png(width: usize, height: usize, pixels: &[[u8; 3]]) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = !0_u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }
    let mut raw = Vec::with_capacity(height * (width * 3 + 1));
    for row in pixels.chunks(width) {
        raw.push(0);
        for pixel in row {
            raw.extend(pixel);
        }
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        zlib.extend(len.to_le_bytes());
        zlib.extend((!len).to_le_bytes());
        zlib.extend(*block);
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend(((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = Vec::new();
    header.extend((width as u32).to_be_bytes());
    header.extend((height as u32).to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

// ---------------------------------------------------------------------
// Reading what is shown a tile at a time
// ---------------------------------------------------------------------

/// A tile read whole from the live view holds what the view shows pixel
/// by pixel, and a tile the stroke never reached reads as nothing.
#[test]
fn a_tile_of_the_live_view_reads_as_its_pixels() {
    let brush = black(pen(18.0, 1.0));
    let stroke = draw(
        Arc::new(JobPool::inline()),
        &brush,
        &wave([20.0, 40.0], [180.0, 60.0], 0.3, 6.0),
        8,
    );
    let live = stroke.live();
    let key = sue_brush::buffer::TileKey::of_pixel(64, 48);
    let mut tile = vec![0_u16; sue_brush::buffer::AREA];
    assert!(live.tile_coverage(key, &mut tile), "the stroke crosses it");
    let [ox, oy] = key.origin();
    for y in 0..TILE {
        for x in 0..TILE {
            assert_eq!(
                tile[(y * TILE + x) as usize],
                live.coverage(i64::from(ox + x), i64::from(oy + y))
            );
        }
    }
    let far = sue_brush::buffer::TileKey::of_pixel(4000, 4000);
    assert!(!live.tile_coverage(far, &mut tile), "nothing there");
}

/// A flat tip (thinner than it is wide) is not a swept round tip: the
/// worker lays it by dabs, which turn it, so a flat marker and a
/// calligraphy nib paint flat.
#[test]
fn a_flat_tip_is_laid_by_dabs_not_swept_round() {
    let round = sue_brush::presets::find("sue.pen.g_pen").expect("the G-pen");
    assert!(Brush::new(round.clone(), [0.0; 3]).is_plain());
    let mut flat = round;
    flat.set(sue_model::brush::param::ASPECT, 0.3);
    assert!(!Brush::new(flat, [0.0; 3]).is_plain());
    for id in ["sue.pen.calligraphy", "sue.pen.flat_marker"] {
        let preset = sue_brush::presets::find(id).expect(id);
        assert!(!Brush::new(preset, [0.0; 3]).is_plain(), "{id}");
    }
}

/// A brush with paint behaviours (an airbrush that builds up, here with
/// a soft round tip and its size and flow following pressure) goes
/// through the worker too: what the live view shows is, pixel for
/// pixel, what the merge lays on the layer, its copies lay once, and the
/// stroke painted again from what a recorder kept gives the same bytes.
#[test]
fn a_brush_with_paint_behaviours_shows_what_it_lays_and_replays() {
    use sue_brush::replay::{self, ENGINE, Recorder};
    use sue_brush::worker::Canvas;
    let preset = sue_brush::presets::find("sue.airbrush.soft").expect("the soft airbrush");
    let brush = Brush::new(preset.clone(), [0.2, 0.3, 0.8]);
    assert!(!brush.is_plain(), "an airbrush builds up");
    let layer = patterned(256, 160);
    let settings = Settings::new(Device::Pen);
    let mut recorder = Recorder::new(
        ENGINE,
        [7; 32],
        &settings,
        sue_brush::paint::Colors {
            main: brush.color,
            sub: brush.sub_color,
        },
        Vec::new(),
    );
    let canvas = Canvas {
        snapshot: Some(layer.clone()),
        ..Canvas::default()
    };
    let pool = Arc::new(JobPool::new(2));
    let mut worker = StrokeWorker::start_on(pool, &brush, recorder.settings(), canvas);
    let samples = wave([30.0, 80.0], [220.0, 70.0], 0.4, 12.0);
    for frame in samples.chunks(3) {
        for sample in frame {
            if let Some(kept) = recorder.take(*sample) {
                worker.push(kept);
            }
        }
        let _ = worker.poll();
    }
    let live = worker.live();
    let mut stroke = worker.finish().expect("finished");
    assert!(stroke.paints_behaviours());
    stroke.let_go_of_layer();
    let mut merged = layer.clone();
    let edit = stroke
        .commit(&mut merged, [0, 0], EntityId::NIL, "Airbrush")
        .expect("laid");
    assert!(!edit.is_empty(), "it painted");
    let (width, height) = (merged.width() as i64, merged.height() as i64);
    let bytes = merged.bytes().unwrap();
    let under = layer.bytes().unwrap();
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let before =
                [under[i], under[i + 1], under[i + 2], under[i + 3]].map(|v| f32::from(v) / 255.0);
            let shown = live.over(x, y, before).map(|v| (v * 255.0).round() as u8);
            assert_eq!(
                &shown[..],
                &bytes[i..i + 4],
                "pixel {x}, {y}: shown is laid"
            );
        }
    }
    // Painted again from what was kept.
    let kept = recorder.finish();
    let mut again = layer.clone();
    replay::paint(
        &kept,
        &preset,
        sue_brush::paint::Resources::default(),
        &mut again,
        [0, 0],
        false,
    )
    .expect("replayed");
    assert_eq!(
        again.bytes(),
        merged.bytes(),
        "the replay lays the same bytes"
    );
}
