//! Smooth arcs from sparse input, and clean stroke ends from fast pen
//! strokes (Phase 6, "Brush engine: ends, smooth curves").
//!
//! A mouse, or a pen moved fast, reports places far apart: 40 pixels and
//! more between two packets. The line must still run through them as a
//! smooth arc, as a hand meant it, never as straight pieces with a turn
//! at each packet. And however fast the pen leaves, the stroke's end is
//! a clean taper (or a clean round end where the brush has none): never
//! cut off, never hooked, never wider than the body.

use sue_brush::input::{Device, Kind, Pipeline, Sample, Settings, StrokePoint};
use sue_model::BrushPreset;
use sue_model::brush::{Binding, BrushRegistry, Taper, TaperMode, input, param};

const CENTRE: [f32; 2] = [400.0, 400.0];
const RADIUS: f32 = 160.0;

fn pen(size: f32, level: u32) -> BrushPreset {
    let mut preset = BrushPreset::new("Test pen");
    preset.set(param::SIZE, size);
    preset
        .dynamics
        .push(Binding::new(param::SIZE, input::PRESSURE).range(0.15, 1.0));
    preset.input.stabilization = level;
    preset
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The largest turn between consecutive steps of at least a tenth of a
/// pixel, in degrees, over `points`.
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

/// The largest turn over any stretch of `span` pixels of arc: a corner
/// shows as a large turn over a short stretch even when it is made of
/// many small steps.
fn sharpest_bend(points: &[StrokePoint], span: f32) -> f32 {
    let mut worst = 0.0_f32;
    for (i, p) in points.iter().enumerate() {
        let Some(j) = points[i..].iter().position(|q| q.arc - p.arc >= span) else {
            break;
        };
        let j = i + j;
        let Some(k) = points[j..]
            .iter()
            .position(|q| q.arc - points[j].arc >= span)
        else {
            break;
        };
        let k = j + k;
        let a = [points[j].pos[0] - p.pos[0], points[j].pos[1] - p.pos[1]];
        let b = [
            points[k].pos[0] - points[j].pos[0],
            points[k].pos[1] - points[j].pos[1],
        ];
        let cross = a[0] * b[1] - a[1] * b[0];
        let dot = a[0] * b[0] + a[1] * b[1];
        worst = worst.max(cross.atan2(dot).abs().to_degrees());
    }
    worst
}

/// A mouse drawing a circle at 60 packets a second, `apart` pixels
/// between packets, lifted at its last place.
fn sparse_circle(apart: f32) -> Vec<Sample> {
    let step = apart / RADIUS;
    let count = (std::f32::consts::TAU / step).floor() as usize;
    let mut samples = Vec::new();
    for i in 0..=count {
        let angle = i as f32 * step;
        let pos = [
            CENTRE[0] + RADIUS * angle.cos(),
            CENTRE[1] + RADIUS * angle.sin(),
        ];
        let kind = if i == 0 { Kind::Down } else { Kind::Move };
        samples.push(Sample::new(kind, pos, 1.0 + i as f64 / 60.0));
    }
    let last = *samples.last().unwrap();
    samples.push(Sample::new(Kind::Up, last.pos, last.time + 1.0 / 60.0));
    samples
}

fn draw(preset: &BrushPreset, settings: Settings, samples: &[Sample]) -> Vec<StrokePoint> {
    let mut pipeline = Pipeline::new(preset, &BrushRegistry::standard(), settings);
    pipeline.push_all(samples);
    assert!(pipeline.is_done());
    pipeline.committed().to_vec()
}

#[test]
fn sparse_mouse_input_draws_smooth_arcs() {
    let on_circle = |p: [f32; 2]| (distance(p, CENTRE) - RADIUS).abs();
    let mut report = String::new();
    for apart in [40.0, 60.0] {
        for level in [0, 2, 5, 10, 30] {
            let points = draw(
                &pen(8.0, level),
                Settings::new(Device::MOUSE),
                &sparse_circle(apart),
            );
            // Leave the ends out: the start and the lift are the pen's.
            let body = &points[points.len() / 10..points.len() * 9 / 10];
            let off = body.iter().map(|p| on_circle(p.pos)).fold(0.0, f32::max);
            let turn = largest_turn(body);
            // A circle of this radius turns 7.2 degrees over 20 pixels.
            let bend = sharpest_bend(body, 10.0);
            report += &format!(
                "{apart} px apart, level {level}: off {off:.2}, turn {turn:.2}, bend {bend:.2}\n"
            );
            assert!(turn <= 3.0, "a visible turn at a packet:\n{report}");
            assert!(bend <= 6.0, "a corner at a packet:\n{report}");
            // On the arc at level 0 (a chord between packets 40 pixels
            // apart would lie 1.25 pixels inside it); pulled in a little
            // by the stabilizer's lag on the higher levels, as every
            // stabilizer that averages is on a mouse this quick (2,400
            // pixels a second and more).
            let bound = (0.5 + 0.35 * level as f32) * apart / 40.0;
            assert!(off <= bound, "off the arc:\n{report}");
        }
    }
    println!("{report}");
}

/// A fast pen stroke: `count` packets at 200 a second along a gentle
/// curve, `speed` pixels a second, pressure easing off as the pen leaves
/// over its last `out` packets (a hand lifting quickly).
fn fast_pen(speed: f32, count: usize, out: usize, bend: f32) -> Vec<Sample> {
    let mut samples = Vec::new();
    let mut pos = [100.0_f32, 300.0_f32];
    let mut way = 0.0_f32;
    for i in 0..count {
        let kind = if i == 0 { Kind::Down } else { Kind::Move };
        let mut sample = Sample::new(kind, pos, 2.0 + i as f64 / 200.0);
        let left = count - i;
        sample.pressure = Some(if left <= out {
            0.7 * left as f32 / (out + 1) as f32
        } else {
            0.7
        });
        samples.push(sample);
        way += bend;
        let step = speed / 200.0;
        pos = [pos[0] + step * way.cos(), pos[1] + step * way.sin()];
    }
    let last = *samples.last().unwrap();
    samples.push(Sample::new(Kind::Up, last.pos, last.time + 1.0 / 200.0));
    samples
}

/// The widths along the last stretch of the stroke: never wider than the
/// body, getting no wider toward the end, and the line's way does not
/// hook back.
fn check_end(points: &[StrokePoint], body: f32, name: &str, tapered: bool) {
    let end = points.last().unwrap().arc;
    let window: Vec<&StrokePoint> = points
        .iter()
        .filter(|p| p.arc >= end - body * 6.0)
        .collect();
    let mut widest = 0.0_f32;
    for p in &window {
        assert!(
            p.size <= body * 1.02 + 0.05,
            "{name}: {} wide at {} of {end}, body {body}",
            p.size,
            p.arc
        );
        widest = widest.max(p.size);
    }
    // No growth over the final taper: each point at most as wide as the
    // one before (a hair of rounding allowed).
    for w in window.windows(2) {
        assert!(
            w[1].size <= w[0].size + 0.02,
            "{name}: grows from {} to {} at {}",
            w[0].size,
            w[1].size,
            w[1].arc
        );
    }
    if tapered {
        let last = points.last().unwrap();
        assert!(
            last.size <= body * 0.35,
            "{name}: the end is cut, {} of {body}",
            last.size
        );
    }
    // No hook: over the last few sizes of arc the line does not turn by
    // more than a gentle curve would.
    let tail: Vec<StrokePoint> = window.iter().map(|p| (*p).clone()).collect();
    let hook = sharpest_bend(&tail, (body * 0.75).max(2.0));
    assert!(hook <= 25.0, "{name}: a hook of {hook} degrees at the end");
}

#[test]
fn fast_pen_strokes_end_in_a_clean_taper() {
    for level in [0, 2, 5, 10, 30] {
        for speed in [1500.0, 3000.0, 6000.0] {
            for bend in [0.0, 0.01, -0.02] {
                let samples = fast_pen(speed, 40, 4, bend);
                let mut preset = pen(16.0, level);
                preset.taper = Taper {
                    start: 0.0,
                    end: 40.0,
                    mode: TaperMode::Length,
                    ..Taper::default()
                };
                let points = draw(&preset, Settings::new(Device::Pen), &samples);
                let body = points[points.len() / 2].size;
                let name = format!("level {level}, {speed} px/s, bend {bend}");
                check_end(&points, body, &name, true);
            }
        }
    }
}

#[test]
fn fast_pen_strokes_without_a_taper_end_round_and_never_wider() {
    for level in [0, 2, 5, 10, 30] {
        for speed in [1500.0, 3000.0, 6000.0] {
            let samples = fast_pen(speed, 40, 4, 0.01);
            let points = draw(&pen(16.0, level), Settings::new(Device::Pen), &samples);
            let body = points[points.len() / 2].size;
            let name = format!("level {level}, {speed} px/s");
            check_end(&points, body, &name, false);
        }
    }
}

#[test]
fn a_fast_mouse_stroke_ends_without_a_hook() {
    for level in [0, 2, 5, 10, 30] {
        let samples = sparse_circle(45.0);
        let mut preset = pen(12.0, level);
        preset.taper = Taper {
            start: 20.0,
            end: 40.0,
            mode: TaperMode::Length,
            ..Taper::default()
        };
        let points = draw(&preset, Settings::new(Device::MOUSE), &samples);
        let body = points[points.len() / 2].size;
        check_end(&points, body, &format!("mouse, level {level}"), true);
    }
}
