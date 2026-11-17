//! Flick: a pen that leaves the tablet fast carries the line on into a
//! point, as a quick flick of the wrist does on paper.
//!
//! Whether the lift is a flick is read from the pen's own speed over its
//! last few hundredths of a second ([`WINDOW`]), in screen points a
//! second, so it feels the same at every zoom. The run out goes on along
//! the way the curve was going as it ended (its last point's direction,
//! not a chord back to an older sample, so it never bends off the line),
//! as far as the speed carries it up to a few brush widths, and narrows
//! from the line's own width to nothing, leaving the line without a
//! corner. Its points are stroke points like any other, and a stroke
//! painted again from its samples flicks the same. A brush with an
//! ending taper narrows the line and the run out in that one taper,
//! which then reaches at least over the run out: the line thins once,
//! to a clean point, never as a bulge with a needle on its end.

use super::StrokePoint;
use crate::math::cos_sin;

/// The device time over which the leaving speed is read, in seconds.
pub(crate) const WINDOW: f64 = 0.05;

/// How fast the pen must leave to flick, in screen points a second.
const FAST: f32 = 700.0;

/// The seconds of travel at the leaving speed the run out reaches.
const CARRY: f32 = 0.02;

/// The farthest a run out reaches, in layer pixels.
const FARTHEST: f32 = 120.0;

/// The pen's leaving speed, in layer pixels a second, when it is a
/// flick: the most it had at its contacts within [`WINDOW`] of `lift`
/// (`recent`: device time and speed), at `scale` screen points per layer
/// pixel. `None` for a lift that is not one.
pub(crate) fn speed(
    recent: &std::collections::VecDeque<(f64, f32)>,
    lift: f64,
    scale: f32,
) -> Option<f32> {
    let speed = recent
        .iter()
        .filter(|(time, _)| lift - time <= WINDOW)
        .map(|(_, speed)| *speed)
        .filter(|speed| speed.is_finite())
        .fold(0.0_f32, f32::max);
    (speed * scale >= FAST).then_some(speed)
}

/// How a run out comes to its point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Narrowing {
    /// It narrows by itself (the brush has no ending taper).
    Own,
    /// The brush's ending taper narrows the line and the run out
    /// together, in one taper, so the line does not thin twice (a
    /// bulging end with a needle on it): the run out keeps the line's
    /// width and reaches no farther than `most` pixels, the longest
    /// ending the brush can have.
    Taper { most: f32 },
}

/// Carries `points` (the stroke's last points; `before` the point taken
/// before them, when they are too few to say) on into a point: a run out
/// at `speed` layer pixels a second, for a brush `size` wide, a point
/// every `step` pixels, narrowing as `narrowing` says. Answers how far
/// it reaches (0 for none).
pub(crate) fn run_out(
    points: &mut Vec<StrokePoint>,
    before: Option<&StrokePoint>,
    speed: f32,
    size: f32,
    step: f32,
    narrowing: Narrowing,
) -> f32 {
    let Some(end) = points.last().or(before).cloned() else {
        return 0.0;
    };
    if !(speed.is_finite() && speed > 0.0 && step > 0.0) {
        return 0.0;
    }
    let mut reach = (speed * CARRY).min(size * 4.0 + 16.0).min(FARTHEST);
    let own = match narrowing {
        Narrowing::Own => true,
        Narrowing::Taper { most } => {
            reach = reach.min(most);
            false
        }
    };
    if reach < step {
        return 0.0;
    }
    let (cos, sin) = cos_sin(end.direction);
    let count = (reach / step).ceil() as usize;
    for k in 1..=count {
        let d = reach * k as f32 / count as f32;
        // Narrowing as `1 - t^2`: no corner where the run out leaves the
        // line, and a sharp point at its end.
        let t = d / reach;
        let left = if own { 1.0 - t * t } else { 1.0 };
        let mut point = end.clone();
        point.pos = [end.pos[0] + cos * d, end.pos[1] + sin * d];
        point.arc = end.arc + d;
        point.time = end.time + f64::from(d / speed);
        point.pressure = end.pressure * left;
        point.size = end.size * left;
        point.taper = end.taper * left;
        points.push(point);
    }
    reach
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn point(pos: [f32; 2], arc: f32, direction: f32) -> StrokePoint {
        StrokePoint {
            pos,
            arc,
            time: 1.0,
            pressure: 0.6,
            speed: 1500.0,
            tilt: [0.0; 2],
            twist: 0.0,
            direction,
            taper: 1.0,
            size: 10.0,
            opacity: 1.0,
            flow: 1.0,
            density: 1.0,
            hardness: 1.0,
            aspect: 1.0,
            angle: 0.0,
            spacing: 0.1,
            scatter: 0.0,
            texture_strength: 0.0,
            hue: 0.0,
            saturation: 0.0,
            luminosity: 0.0,
            extra: Vec::new(),
        }
    }

    #[test]
    fn only_a_fast_leave_is_a_flick_at_any_zoom() {
        let recent: VecDeque<(f64, f32)> = [(0.90, 2000.0), (0.97, 400.0), (0.99, 500.0)].into();
        // The fast contact is too long ago.
        assert_eq!(speed(&recent, 1.0, 1.0), None);
        // At 200 % the same pen moves twice as fast on screen.
        assert_eq!(speed(&recent, 1.0, 2.0), Some(500.0));
        let fast: VecDeque<(f64, f32)> = [(0.98, 900.0)].into();
        assert_eq!(speed(&fast, 1.0, 1.0), Some(900.0));
    }

    #[test]
    fn the_run_out_goes_on_along_the_line_and_narrows_to_a_point() {
        // A line going down and to the right, at 30 degrees.
        let mut points = vec![point([0.0, 0.0], 0.0, 30.0), point([8.66, 5.0], 10.0, 30.0)];
        run_out(&mut points, None, 1500.0, 10.0, 1.0, Narrowing::Own);
        let end = &points[1];
        let runs: Vec<&StrokePoint> = points[2..].iter().collect();
        assert!(runs.len() >= 20, "it reaches on: {}", runs.len());
        let mut size = end.size;
        for p in &runs {
            let (dx, dy) = (p.pos[0] - end.pos[0], p.pos[1] - end.pos[1]);
            // Along the line's own way, never bending off it.
            assert!(
                (dy - dx * (30.0_f32).to_radians().tan()).abs() < 1e-3,
                "{p:?}"
            );
            assert!(p.size <= size, "it never widens");
            size = p.size;
            assert!(p.arc > end.arc && p.time > end.time);
        }
        assert!(runs.last().unwrap().size.abs() < 1e-6, "a point");
        let reach = runs.last().unwrap().arc - end.arc;
        assert!(
            (reach - 30.0).abs() < 1e-3,
            "speed carries it 30 px: {reach}"
        );
        // Never farther than a few brush widths.
        let mut fast = vec![point([0.0, 0.0], 0.0, 0.0)];
        run_out(&mut fast, None, 100_000.0, 10.0, 2.0, Narrowing::Own);
        let reach = fast.last().unwrap().arc;
        assert!((reach - 56.0).abs() < 1e-3, "{reach}");
        // Under the brush's ending taper it keeps the line's width (the
        // taper narrows it) and reaches no farther than the taper can.
        let mut tapered = vec![point([0.0, 0.0], 0.0, 0.0)];
        let reach = run_out(
            &mut tapered,
            None,
            100_000.0,
            10.0,
            2.0,
            Narrowing::Taper { most: 20.0 },
        );
        assert!((reach - 20.0).abs() < 1e-3, "{reach}");
        assert!(tapered.iter().all(|p| (p.size - 10.0).abs() < 1e-6));
    }
}
