//! The input pipeline's promises, from the outside: smoothness, stroke
//! ends that never grow, and the same output however the samples come.
//!
//! Set `SUE_BRUSH_SHOTS` to a folder to have the strokes drawn there as
//! PNG pictures (the stroke, its centre line, and its width along the
//! arc), for looking at by eye.

use sue_brush::input::{Device, Kind, Pipeline, Sample, Settings, StrokePoint};
use sue_model::BrushPreset;
use sue_model::brush::{Binding, BrushRegistry, Taper, TaperMode, input, param};

// ---------------------------------------------------------------------
// Strokes to test with
// ---------------------------------------------------------------------

/// The device's packets a second.
const RATE: f64 = 200.0;

/// A number from -1 to 1 from `seed` and `i` (SplitMix64).
fn jitter(seed: u64, i: u64) -> f32 {
    let mut z = seed ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    ((z >> 40) as f32 / (1_u64 << 24) as f32) * 2.0 - 1.0
}

/// A pen with the size following pressure, from 15 % at no pressure
/// (Sue's default pen) to all of `size`.
fn pen(size: f32, level: u32) -> BrushPreset {
    let mut preset = BrushPreset::new("Test pen");
    preset.set(param::SIZE, size);
    preset
        .dynamics
        .push(Binding::new(param::SIZE, input::PRESSURE).range(0.15, 1.0));
    preset.input.stabilization = level;
    preset
}

fn with_end_taper(mut preset: BrushPreset, start: f32, end: f32) -> BrushPreset {
    preset.taper = Taper {
        start,
        end,
        mode: TaperMode::Length,
        ..Taper::default()
    };
    preset
}

/// A stroke along `path` (place at a share 0 to 1 of the stroke),
/// `seconds` long, with each packet's place moved by up to `wobble`
/// device units each way, pressing `pressure(share)`.
fn stroke(
    path: impl Fn(f64) -> [f32; 2],
    seconds: f64,
    wobble: f32,
    seed: u64,
    pressure: impl Fn(f64) -> Option<f32>,
) -> Vec<Sample> {
    let count = (seconds * RATE).round() as u64;
    let mut samples = Vec::new();
    for i in 0..=count {
        let share = i as f64 / count as f64;
        let [x, y] = path(share);
        let pos = [
            x + wobble * jitter(seed, 2 * i),
            y + wobble * jitter(seed, 2 * i + 1),
        ];
        let kind = if i == 0 { Kind::Down } else { Kind::Move };
        let mut sample = Sample::new(kind, pos, 10.0 + i as f64 / RATE);
        sample.pressure = pressure(share);
        samples.push(sample);
    }
    samples
}

/// The lift one packet after the last sample, at its place.
fn lift(samples: &mut Vec<Sample>) {
    let last = *samples.last().unwrap();
    samples.push(Sample::new(Kind::Up, last.pos, last.time + 1.0 / RATE));
}

const CENTRE: [f32; 2] = [320.0, 320.0];
const RADIUS: f32 = 200.0;

fn circle(share: f64) -> [f32; 2] {
    let angle = share * std::f64::consts::TAU;
    [
        CENTRE[0] + RADIUS * angle.cos() as f32,
        CENTRE[1] + RADIUS * angle.sin() as f32,
    ]
}

fn line(share: f64) -> [f32; 2] {
    [40.0 + 560.0 * share as f32, 160.0]
}

/// Every point the pipeline gives for `samples`, pushed `per_frame` at a
/// time, and the tail after each frame.
fn run(
    preset: &BrushPreset,
    settings: Settings,
    samples: &[Sample],
    per_frame: usize,
) -> (Vec<StrokePoint>, Vec<Vec<StrokePoint>>) {
    let mut pipeline = Pipeline::new(preset, &BrushRegistry::standard(), settings);
    let mut tails = Vec::new();
    for frame in samples.chunks(per_frame) {
        pipeline.push_all(frame);
        tails.push(pipeline.points());
    }
    assert!(pipeline.is_done(), "the stroke ends at the lift");
    assert!(pipeline.tail().is_empty());
    (pipeline.committed().to_vec(), tails)
}

fn median(mut values: Vec<f32>) -> f32 {
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

// ---------------------------------------------------------------------
// Smoothness (D2): a circle and a line with one device unit of jitter
// ---------------------------------------------------------------------

/// How a stroke measures up against the path it was meant to be.
#[derive(Debug)]
struct Smoothness {
    /// The farthest a point lies from the ideal path, layer pixels.
    distance: f32,
    /// The largest turn between two steps of the line, degrees.
    turn: f32,
    /// The farthest the drawn end trailed the pen, layer pixels.
    lag: f32,
    /// How far from the pen's last place the finished line ends.
    reach: f32,
}

fn measure(
    level: u32,
    by_speed: bool,
    path: impl Fn(f64) -> [f32; 2] + Copy,
    off_path: impl Fn([f32; 2]) -> f32,
    seconds: f64,
    name: &str,
) -> Smoothness {
    let mut preset = pen(12.0, level);
    preset.input.by_speed = by_speed;
    let mut samples = stroke(path, seconds, 1.0, 42, |_| Some(0.6));
    lift(&mut samples);
    let mut pipeline = Pipeline::new(
        &preset,
        &BrushRegistry::standard(),
        Settings::new(Device::Pen),
    );
    let count = samples.len() - 1;
    let mut lag = 0.0_f32;
    for (i, sample) in samples.iter().enumerate() {
        pipeline.push(*sample);
        if i < count {
            let pen = path(i as f64 / (count - 1) as f64);
            let end = pipeline.points().last().unwrap().pos;
            lag = lag.max(distance(end, pen));
        }
    }
    let points = pipeline.committed();
    let distance_off = points
        .iter()
        .map(|p| off_path(p.pos))
        .fold(0.0_f32, f32::max);
    let turn = largest_turn(points);
    let speed = if by_speed { "-by-speed" } else { "" };
    draw(&format!("smooth-{name}-{level}{speed}"), points);
    Smoothness {
        distance: distance_off,
        turn,
        lag,
        reach: distance(points.last().unwrap().pos, path(1.0)),
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// The largest turn between consecutive steps of at least a tenth of a
/// pixel, in degrees.
fn largest_turn(points: &[StrokePoint]) -> f32 {
    let steps: Vec<[f32; 2]> = points
        .windows(2)
        .map(|w| [w[1].pos[0] - w[0].pos[0], w[1].pos[1] - w[0].pos[1]])
        .filter(|d| d[0].hypot(d[1]) >= 0.1)
        .collect();
    steps
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let cross = a[0] * b[1] - a[1] * b[0];
            let dot = a[0] * b[0] + a[1] * b[1];
            cross.atan2(dot).abs().to_degrees()
        })
        .fold(0.0, f32::max)
}

#[test]
fn a_jittered_circle_and_line_come_out_smooth() {
    let on_circle = |p: [f32; 2]| (distance(p, CENTRE) - RADIUS).abs();
    let on_line = |p: [f32; 2]| (p[1] - 160.0).abs();
    let mut report = String::new();
    // (level, by speed, circle distance, line distance, turn, lag,
    // reach): the bounds,
    // in layer pixels and degrees. At 0 the curve goes through the
    // device's own samples, so the jitter stays (one unit) and so do its
    // turns, about 35 degrees between samples 3 pixels apart; the line
    // reaches the pen within a packet. From 10 on the jitter is
    // averaged away: a step of the line turns by a few degrees at most.
    // A circle drawn at 630 pixels a second is pulled in by the
    // stabilizer's lag, as with every stabilizer that averages (about
    // 10 pixels on a radius of 200 at 50); the lag stays within
    // `10 + 1.5 x level` screen points. After the lift the line ends
    // where the pen lifted, within the jitter of its last packet.
    // "Adjust by speed" loosens the stabilizer on a quick stroke; at 50
    // and this speed the farthest lag binds first either way.
    let bounds = [
        (0, false, 1.6, 1.6, 60.0, 3.0, 1.5),
        (10, false, 1.6, 1.0, 8.0, 30.0, 1.5),
        (50, false, 12.0, 1.0, 4.0, 85.0, 1.5),
        (50, true, 12.0, 1.0, 4.0, 85.0, 1.5),
    ];
    for (level, by_speed, circle_bound, line_bound, turn_bound, lag_bound, reach_bound) in bounds {
        let round = measure(level, by_speed, circle, on_circle, 2.0, "circle");
        let straight = measure(level, by_speed, line, on_line, 1.0, "line");
        report += &format!(
            "level {level}, by speed {by_speed}: circle {round:?}, line {straight:?}
"
        );
        assert!(round.distance <= circle_bound, "{report}");
        assert!(straight.distance <= line_bound, "{report}");
        assert!(
            round.turn <= turn_bound && straight.turn <= turn_bound,
            "{report}"
        );
        assert!(
            round.lag <= lag_bound && straight.lag <= lag_bound,
            "{report}"
        );
        assert!(
            round.reach <= reach_bound && straight.reach <= reach_bound,
            "{report}"
        );
    }
    println!("{report}");
}

// ---------------------------------------------------------------------
// Stroke ends
// ---------------------------------------------------------------------

/// The ways a stroke ends that the pipeline must survive.
#[derive(Clone, Copy, Debug)]
enum Ending {
    /// A mouse: pressure is 1 all along.
    Mouse,
    /// A pen whose pressure ramps from 0.5 down to 0.05 over the last
    /// 40 ms while it moves.
    Ramp,
    /// A pen lifted while still pressing 0.5.
    AbruptLoss,
    /// A pen whose driver stops sending pressure for the last packets.
    NoLiftPressure,
}

const ENDINGS: [Ending; 4] = [
    Ending::Mouse,
    Ending::Ramp,
    Ending::AbruptLoss,
    Ending::NoLiftPressure,
];

/// A 0.6 s stroke ending as `ending` says, with a wave in it.
fn ending_stroke(ending: Ending) -> (Device, Vec<Sample>) {
    let path = |share: f64| {
        let x = 40.0 + 420.0 * share as f32;
        [x, 120.0 + 30.0 * (share * 5.0).sin() as f32]
    };
    let seconds = 0.6;
    let ramp_from = 1.0 - 0.04 / seconds;
    let mut samples = stroke(path, seconds, 0.3, 7, |share| match ending {
        Ending::Mouse => None,
        Ending::Ramp if share > ramp_from => {
            let t = ((share - ramp_from) / (1.0 - ramp_from)) as f32;
            Some(0.5 - 0.45 * t)
        }
        Ending::NoLiftPressure if share > 0.97 => None,
        _ => Some(if share < 0.05 {
            (0.1 + 8.0 * share) as f32
        } else {
            0.5
        }),
    });
    lift(&mut samples);
    let device = match ending {
        Ending::Mouse => Device::MOUSE,
        _ => Device::Pen,
    };
    (device, samples)
}

/// Checks the three promises about a stroke's end for `preset` and the
/// samples, and answers the points.
fn check_end(
    preset: &BrushPreset,
    device: Device,
    samples: &[Sample],
    what: &str,
) -> Vec<StrokePoint> {
    let mut pipeline = Pipeline::new(preset, &BrushRegistry::standard(), Settings::new(device));
    let (body, up) = samples.split_at(samples.len() - 1);
    pipeline.push_all(body);
    // The brush at the last contact, as the artist saw it.
    let before = pipeline.points();
    let at_contact = before.last().unwrap().clone();
    let committed_before = pipeline.committed().len();
    pipeline.push(up[0]);
    let points = pipeline.committed().to_vec();
    // 1. Nothing after the last contact presses harder than the stroke
    // did there.
    for point in &points[committed_before..] {
        if point.time > at_contact.time {
            assert!(
                point.pressure <= at_contact.pressure + 1e-6,
                "{what}: pressure {} at {} after the last contact's {}",
                point.pressure,
                point.arc,
                at_contact.pressure
            );
        }
    }
    let length = points.last().unwrap().arc;
    // 2. Over the final taper window the size never grows.
    if preset.taper.end > 0.0 {
        let from = length - preset.taper.end.min(0.4 * length);
        let mut biggest_before = f32::INFINITY;
        let mut place = f32::NEG_INFINITY;
        let mut biggest_here = 0.0_f32;
        for point in points.iter().filter(|p| p.arc >= from) {
            if point.arc > place + 1e-4 {
                biggest_before = biggest_before.min(biggest_here.max(point.size));
                biggest_here = 0.0;
                place = point.arc;
            }
            assert!(
                point.size <= biggest_before + 1e-4,
                "{what}: size {} at {} grows over the taper window",
                point.size,
                point.arc
            );
            biggest_here = biggest_here.max(point.size);
        }
    }
    // 3. The end is never wider than the body.
    let size = preset.value(param::SIZE).unwrap();
    let body = median(
        points
            .iter()
            .filter(|p| p.arc > 0.2 * length && p.arc < length - 3.0 * size)
            .map(|p| p.size)
            .collect(),
    );
    let end = points
        .iter()
        .filter(|p| p.arc >= length - 3.0 * size)
        .map(|p| p.size)
        .fold(0.0, f32::max);
    assert!(
        end <= body * 1.02,
        "{what}: the end is {end} wide, the body {body}"
    );
    points
}

#[test]
fn stroke_ends_never_grow_for_any_device() {
    for ending in ENDINGS {
        let (device, samples) = ending_stroke(ending);
        for level in [0, 20, 60] {
            for taper in [0.0, 40.0] {
                let preset = with_end_taper(pen(16.0, level), 0.0, taper);
                let what = format!("{ending:?} at {level}, ending {taper}");
                let points = check_end(&preset, device, &samples, &what);
                if taper > 0.0 {
                    let last = points.last().unwrap();
                    assert!(last.size < 1.5, "{what}: the tip is {} wide", last.size);
                }
                if level == 20 {
                    draw(&format!("end-{ending:?}-{taper}"), &points);
                }
            }
        }
    }
}

#[test]
fn a_stabilized_line_ends_where_and_as_thin_as_the_pen_did() {
    // A pen eases off from 0.5 to nothing over its last 40 ms and lifts,
    // slowly, at a medium pace and in a quick hatch (2,800 pixels a
    // second). However steady the line, it reaches the place the pen
    // lifted, and its end thins as the unsteadied line's does: the
    // stabilizer must not average the pen's easing away into a blunt
    // end, nor stop short of the pen.
    let end_of = |seconds: f64, level: u32| {
        let ramp_from = 1.0 - 0.04 / seconds;
        let mut samples = stroke(line, seconds, 0.3, 3, |share| {
            Some(if share > ramp_from {
                (0.5 * (1.0 - (share - ramp_from) / (1.0 - ramp_from))) as f32
            } else {
                0.5
            })
        });
        lift(&mut samples);
        let pen_end = samples.last().unwrap().pos;
        let (points, _) = run(&pen(16.0, level), Settings::new(Device::Pen), &samples, 1);
        let length = points.last().unwrap().arc;
        // The width two pixels before the end.
        let near_end = points
            .iter()
            .rev()
            .find(|p| p.arc <= length - 2.0)
            .unwrap()
            .size;
        let reach = distance(points.last().unwrap().pos, pen_end);
        (near_end, reach, points)
    };
    for seconds in [2.0, 0.8, 0.2] {
        let (plain, _, _) = end_of(seconds, 0);
        for level in [20, 60] {
            let (steady, reach, points) = end_of(seconds, level);
            let what = format!("{seconds} s at {level}");
            assert!(reach <= 1.5, "{what}: ends {reach} short of the pen");
            assert!(
                (steady - plain).abs() <= 0.5,
                "{what}: {steady} wide near the end, {plain} unsteadied"
            );
            if level == 20 && seconds < 0.5 {
                draw("end-quick-hatch-20", &points);
            }
        }
    }
}

#[test]
fn any_pressure_sequence_keeps_the_promises() {
    // Pressure that wanders, spikes and drops at random, with or without
    // packets that say none, lifted anywhere.
    for seed in 0..400_u64 {
        let spiky = move |share: f64| {
            let i = (share * 1000.0) as u64;
            let roll = jitter(seed ^ 0xABCD, i);
            if roll > 0.9 {
                None
            } else if roll < -0.95 {
                Some(1.0)
            } else {
                Some(0.4 + 0.35 * jitter(seed, i / 7))
            }
        };
        let path = move |share: f64| {
            [
                30.0 + 300.0 * share as f32,
                100.0 + 40.0 * (share * (2.0 + seed as f64 % 5.0)).sin() as f32,
            ]
        };
        let seconds = 0.15 + (seed % 7) as f64 * 0.1;
        let mut samples = stroke(path, seconds, 0.5, seed, spiky);
        lift(&mut samples);
        let level = [0, 5, 30, 100][seed as usize % 4];
        let preset = with_end_taper(pen(10.0, level), 5.0, 25.0);
        let mut pipeline = Pipeline::new(
            &preset,
            &BrushRegistry::standard(),
            Settings::new(Device::Pen),
        );
        let (body, up) = samples.split_at(samples.len() - 1);
        pipeline.push_all(body);
        let at_contact = pipeline.points().last().unwrap().clone();
        let before = pipeline.committed().len();
        pipeline.push(up[0]);
        let points = pipeline.committed();
        for point in &points[before..] {
            if point.time > at_contact.time {
                assert!(point.pressure <= at_contact.pressure + 1e-6, "seed {seed}");
            }
        }
        let length = points.last().unwrap().arc;
        let window: Vec<&StrokePoint> = points
            .iter()
            .filter(|p| p.arc >= length - 25.0_f32.min(0.4 * length))
            .collect();
        for pair in window.windows(2) {
            if pair[1].arc > pair[0].arc + 1e-4 {
                assert!(pair[1].size <= pair[0].size + 1e-4, "seed {seed}");
            }
        }
    }
}

#[test]
fn the_end_looks_the_same_before_and_after_the_lift() {
    // Without a stabilizer the pen and the brush are at one place, so
    // the end the artist sees while the pen is down is the end that
    // stays.
    let preset = with_end_taper(pen(16.0, 0), 20.0, 60.0);
    let (_, samples) = ending_stroke(Ending::AbruptLoss);
    let mut pipeline = Pipeline::new(
        &preset,
        &BrushRegistry::standard(),
        Settings::new(Device::Pen),
    );
    let (body, up) = samples.split_at(samples.len() - 1);
    pipeline.push_all(body);
    let live = pipeline.points();
    pipeline.push(up[0]);
    let done = pipeline.committed();
    let live_length = live.last().unwrap().arc;
    let done_length = done.last().unwrap().arc;
    assert!((live_length - done_length).abs() < 1.0);
    for point in live.iter().filter(|p| p.arc < live_length - 2.0) {
        let after = done
            .iter()
            .min_by(|a, b| {
                (a.arc - point.arc)
                    .abs()
                    .total_cmp(&(b.arc - point.arc).abs())
            })
            .unwrap();
        assert!(
            (after.size - point.size).abs() < 0.5,
            "at {}: {} live, {} after the lift",
            point.arc,
            point.size,
            after.size
        );
    }
    draw("live-before-lift", &live);
}

#[test]
fn a_tap_is_a_dot_as_big_as_the_press() {
    // An 80 ms tap: the press rises to 0.8, holds and lets go.
    let presses: Vec<f32> = (0..16)
        .map(|i| match i {
            0..4 => 0.2 * (i + 1) as f32,
            4..12 => 0.8,
            _ => 0.8 - 0.18 * (i - 11) as f32,
        })
        .collect();
    let mut samples: Vec<Sample> = presses
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let kind = if i == 0 { Kind::Down } else { Kind::Move };
            Sample::new(kind, [50.0, 50.0], i as f64 / RATE).pressing(*p)
        })
        .collect();
    lift(&mut samples);
    // (level, the least share of the full press the dot reaches). The
    // stabilizer averages pressure with the place over its window, so
    // at 10 (a 160 ms window) a short tap reads lighter.
    for (level, least) in [(0, 0.95), (10, 0.75)] {
        let preset = with_end_taper(pen(20.0, level), 10.0, 10.0);
        let (points, _) = run(&preset, Settings::new(Device::Pen), &samples, 1);
        let biggest = points.iter().map(|p| p.size).fold(0.0, f32::max);
        let full = 20.0 * (0.15 + 0.85 * 0.8);
        assert!(points.iter().all(|p| p.pos == [50.0, 50.0]));
        assert!(
            biggest >= least * full && biggest <= full + 1e-4,
            "{level}: {biggest}"
        );
    }
}

#[test]
fn a_pen_never_falls_back_to_a_full_press() {
    let preset = pen(16.0, 0);
    let mut samples = stroke(line, 0.3, 0.0, 1, |share| (share < 0.3).then_some(0.3));
    lift(&mut samples);
    let (points, _) = run(&preset, Settings::new(Device::Pen), &samples, 1);
    assert!(points.iter().all(|p| p.pressure <= 0.3 + 1e-6));
    // A pen whose first packets say no pressure waits for the first one.
    let mut late = stroke(line, 0.3, 0.0, 1, |share| (share > 0.1).then_some(0.4));
    lift(&mut late);
    let (points, _) = run(&preset, Settings::new(Device::Pen), &late, 1);
    assert!(points.iter().all(|p| p.pressure <= 0.4 + 1e-6));
    assert!(points[0].pressure > 0.1, "the start takes the first press");
}

#[test]
fn a_mouse_presses_the_same_all_along() {
    let preset = with_end_taper(pen(16.0, 10), 0.0, 0.0);
    let mut samples = stroke(line, 0.4, 0.0, 1, |_| Some(0.2));
    lift(&mut samples);
    let (points, _) = run(&preset, Settings::new(Device::MOUSE), &samples, 3);
    assert!(points.iter().all(|p| p.pressure == 1.0));
    assert!(points.iter().all(|p| (p.size - 16.0).abs() < 1e-4));
}

/// A mouse has no pressure to thin a stroke's ends, so by speed its
/// tapers keep at least their set lengths: a slow mouse stroke still
/// comes to a point at both ends, as a slow pen's does not (the pen
/// thins its ends by pressing lighter).
#[test]
fn a_slow_mouse_stroke_still_tapers_by_speed() {
    let mut preset = with_end_taper(pen(16.0, 0), 30.0, 60.0);
    preset.taper.by_speed = true;
    // 560 pixels in four seconds: slow.
    let mut samples = stroke(line, 4.0, 0.0, 1, |_| Some(1.0));
    lift(&mut samples);
    let (mouse, _) = run(&preset, Settings::new(Device::MOUSE), &samples, 3);
    let (pen, _) = run(&preset, Settings::new(Device::Pen), &samples, 3);
    let first = |points: &[StrokePoint]| points.first().unwrap().size;
    let last = |points: &[StrokePoint]| points.last().unwrap().size;
    let middle = |points: &[StrokePoint]| points[points.len() / 2].size;
    assert!((middle(&mouse) - 16.0).abs() < 1e-3, "the body is full");
    assert!(first(&mouse) < 2.0, "a pointed start: {}", first(&mouse));
    assert!(last(&mouse) < 2.0, "a pointed end: {}", last(&mouse));
    // Halfway along its ending the size is about half or more (the
    // taper eases out), never the full size.
    let end = mouse.last().unwrap().arc;
    let at = |arc: f32| {
        mouse
            .iter()
            .min_by(|a, b| (a.arc - arc).abs().total_cmp(&(b.arc - arc).abs()))
            .unwrap()
            .size
    };
    assert!(
        at(end - 30.0) < 15.0 && at(end - 30.0) > 6.0,
        "{}",
        at(end - 30.0)
    );
    // The slow pen's ends thin over a few pixels only (its pressure
    // thins them): ten pixels in, it is at nearly its full size, where
    // the mouse's is still well under it.
    let near = |points: &[StrokePoint], arc: f32| {
        points
            .iter()
            .min_by(|a, b| (a.arc - arc).abs().total_cmp(&(b.arc - arc).abs()))
            .unwrap()
            .size
    };
    let pen_end = pen.last().unwrap().arc;
    assert!(near(&pen, 10.0) > 14.0, "{}", near(&pen, 10.0));
    assert!(
        near(&pen, pen_end - 10.0) > 14.0,
        "{}",
        near(&pen, pen_end - 10.0)
    );
    assert!(near(&mouse, 10.0) < 12.0, "{}", near(&mouse, 10.0));
    assert!(
        near(&mouse, end - 10.0) < 8.0,
        "{}",
        near(&mouse, end - 10.0)
    );
}

// ---------------------------------------------------------------------
// The same samples, however they come
// ---------------------------------------------------------------------

#[test]
fn one_two_or_eight_samples_a_frame_give_the_same_stroke() {
    let mut speedy = pen(24.0, 30);
    speedy.input.by_speed = true;
    speedy.taper = Taper {
        start: 15.0,
        end: 40.0,
        by_speed: true,
        ..Taper::default()
    };
    let mut percent = pen(10.0, 5);
    percent.taper = Taper {
        start: 10.0,
        end: 20.0,
        mode: TaperMode::Percent,
        ..Taper::default()
    };
    let mut fade = pen(10.0, 0);
    fade.taper = Taper {
        end: 300.0,
        mode: TaperMode::Fade,
        opacity: true,
        ..Taper::default()
    };
    fade.dynamics
        .push(Binding::new(param::ANGLE, input::DIRECTION).range(0.0, 360.0));
    fade.dynamics
        .push(Binding::new(param::OPACITY, input::RANDOM).range(0.5, 1.0));
    for (name, preset) in [
        ("plain", pen(16.0, 0)),
        ("stabilized", pen(16.0, 50)),
        ("speedy", speedy),
        ("percent", percent),
        ("fade", fade),
    ] {
        for ending in ENDINGS {
            let (device, samples) = ending_stroke(ending);
            let settings = Settings::new(device);
            let (one, one_tails) = run(&preset, settings, &samples, 1);
            for per_frame in [2, 8] {
                let (many, many_tails) = run(&preset, settings, &samples, per_frame);
                assert_eq!(one, many, "{name} {ending:?}: {per_frame} a frame");
                // The whole stroke as it stood after the same samples is
                // the same too.
                for (i, tail) in many_tails.iter().enumerate() {
                    let pushed = ((i + 1) * per_frame).min(samples.len());
                    assert_eq!(
                        &one_tails[pushed - 1],
                        tail,
                        "{name} {ending:?}: after {pushed} samples"
                    );
                }
            }
        }
    }
}

#[test]
fn committed_points_never_change() {
    let preset = with_end_taper(pen(16.0, 25), 10.0, 30.0);
    let (device, samples) = ending_stroke(Ending::Ramp);
    let mut pipeline = Pipeline::new(&preset, &BrushRegistry::standard(), Settings::new(device));
    let mut seen: Vec<StrokePoint> = Vec::new();
    let mut longest_tail = 0.0_f32;
    for sample in samples {
        pipeline.push(sample);
        let now = pipeline.committed();
        assert_eq!(&now[..seen.len()], &seen[..], "a committed point changed");
        seen = now.to_vec();
        let tail = pipeline.tail();
        // Past the starting (whose taper is settled once the stroke is
        // long enough for both), the tail is the ending window and the
        // curve's last segments, not the stroke.
        if let (Some(first), Some(last)) = (tail.first(), tail.last())
            && first.arc > 100.0
        {
            longest_tail = longest_tail.max(last.arc - first.arc);
        }
    }
    assert!(
        longest_tail > 20.0 && longest_tail < 30.0 + 5.0,
        "{longest_tail}"
    );
    assert!(seen.last().unwrap().arc > 300.0);
}

#[test]
fn a_size_on_the_screen_and_a_zoom_change_the_layer_size_only() {
    let mut preset = pen(16.0, 0);
    preset.size_on_screen = true;
    let mut samples = stroke(line, 0.2, 0.0, 1, |_| Some(1.0));
    lift(&mut samples);
    let mut settings = Settings::new(Device::Pen);
    settings.scale = 2.0;
    let (points, _) = run(&preset, settings, &samples, 1);
    let body = median(points.iter().map(|p| p.size).collect());
    assert!((body - 8.0).abs() < 0.5, "{body}");
}

// ---------------------------------------------------------------------
// Pictures for looking at
// ---------------------------------------------------------------------

/// Draws `points` as the stroke a round pen would make, its centre line
/// and its width along the arc, into `$SUE_BRUSH_SHOTS/<name>.png`.
fn draw(name: &str, points: &[StrokePoint]) {
    let Some(folder) = std::env::var_os("SUE_BRUSH_SHOTS") else {
        return;
    };
    let (width, top) = (640_usize, 640_usize);
    let chart = 160_usize;
    let height = top + chart;
    let mut image = vec![[255_u8; 3]; width * height];
    // The stroke: the distance to the uneven capsule between each pair.
    let mut coverage = vec![0.0_f32; width * top];
    for pair in points.windows(2) {
        capsule(&mut coverage, width, top, &pair[0], &pair[1]);
    }
    if points.len() == 1 {
        capsule(&mut coverage, width, top, &points[0], &points[0]);
    }
    for (pixel, c) in image.iter_mut().zip(&coverage) {
        let shade = (255.0 * (1.0 - 0.75 * c)) as u8;
        *pixel = [shade; 3];
    }
    // The centre line in red.
    for p in points {
        let (x, y) = (p.pos[0].round() as isize, p.pos[1].round() as isize);
        if (0..width as isize).contains(&x) && (0..top as isize).contains(&y) {
            image[y as usize * width + x as usize] = [220, 30, 30];
        }
    }
    // The width along the arc, in blue, under a line at the brush size.
    let length = points.last().map_or(1.0, |p| p.arc).max(1.0);
    let most = points.iter().map(|p| p.size).fold(1.0, f32::max);
    for x in 0..width {
        image[(top + 4) * width + x] = [160; 3];
    }
    for p in points {
        let x = ((p.arc / length) * (width - 1) as f32) as usize;
        let h = ((p.size / most) * (chart - 12) as f32) as usize;
        for y in 0..h {
            image[(height - 1 - y) * width + x] = [40, 70, 200];
        }
    }
    let path = std::path::Path::new(&folder).join(format!("{name}.png"));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(path, png(width, height, &image)).unwrap();
}

/// Paints the round-tipped capsule from `a` to `b` (radii from their
/// sizes) into `coverage`, keeping the most at each pixel.
fn capsule(coverage: &mut [f32], width: usize, height: usize, a: &StrokePoint, b: &StrokePoint) {
    let (ra, rb) = (a.size / 2.0, b.size / 2.0);
    let reach = ra.max(rb) + 1.0;
    let x0 = (a.pos[0].min(b.pos[0]) - reach).floor().max(0.0) as usize;
    let x1 = ((a.pos[0].max(b.pos[0]) + reach).ceil() as usize).min(width - 1);
    let y0 = (a.pos[1].min(b.pos[1]) - reach).floor().max(0.0) as usize;
    let y1 = ((a.pos[1].max(b.pos[1]) + reach).ceil() as usize).min(height - 1);
    let (dx, dy) = (b.pos[0] - a.pos[0], b.pos[1] - a.pos[1]);
    let len2 = dx * dx + dy * dy;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = (x as f32 + 0.5 - a.pos[0], y as f32 + 0.5 - a.pos[1]);
            let t = if len2 > 0.0 {
                ((px * dx + py * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = (px - t * dx).hypot(py - t * dy);
            let r = ra + (rb - ra) * t;
            let c = (r - d + 0.5).clamp(0.0, 1.0);
            let slot = &mut coverage[y * width + x];
            *slot = slot.max(c);
        }
    }
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
