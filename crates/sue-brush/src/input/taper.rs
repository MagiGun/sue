//! Tapers, and what is committed.
//!
//! The starting and ending are functions of arc length, eased the way
//! Sue's brushes always were (`1 - (1 - x)^2`): the stroke grows from a
//! point over the starting length and thins to one over the ending
//! length. A stroke too short for both shrinks them to fit in four
//! fifths of it; a stroke shorter than a pixel is a dot and has neither.
//! In percent mode both lengths are shares of the stroke; in fade mode
//! the stroke fades away by the ending length from where it began. "By
//! speed" stretches them by up to 1.7 times the share of full speed the
//! pen had: at the start the mean over its first 25 ms, at the end the
//! speed it has there (in fade mode the start's, so the fade is known
//! as it is drawn).
//!
//! The ending is applied while the pen is down, at the stroke's end as
//! it stands, so it does not change shape on the lift. Over the ending
//! window the size and the opacity never grow toward the end: a pen
//! that presses harder there cannot end in a blob.
//!
//! A point is committed once nothing can change it any more: it lies
//! before every ending window the stroke can still have, and its
//! starting is settled.

use sue_model::brush::{Taper, TaperMode};

use super::StrokePoint;

/// How much "by speed" stretches a taper at full speed.
const BY_SPEED: f32 = 1.7;

/// The device time the starting speed is measured over.
const START_SPEED_TIME: f64 = 0.025;

/// Layer pixels a second that read as full speed, as for the `speed`
/// input.
const FULL_SPEED: f32 = 2000.0;

/// The share of a stroke both tapers fit in.
const FIT: f32 = 0.8;

/// The ease of the tapers, 0 at 0 and 1 at 1.
fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    1.0 - (1.0 - x) * (1.0 - x)
}

/// The tapers and the committed points. See the module.
#[derive(Clone, Debug)]
pub(crate) struct Shaper {
    taper: Taper,
    min_width: f32,
    /// Points of final segments that are not committed yet, before the
    /// tapers.
    pending: Vec<StrokePoint>,
    committed: Vec<StrokePoint>,
    /// The first point's time.
    start: Option<f64>,
    /// The speeds seen over the starting time: their sum and count.
    speeds: (f32, u32),
    /// The mean speed share over the starting time, once it is over.
    start_speed: Option<f32>,
}

impl Shaper {
    pub fn new(taper: Taper, min_width: f32) -> Self {
        let clean = |length: f32| {
            if length.is_finite() {
                length.max(0.0)
            } else {
                0.0
            }
        };
        Self {
            taper: Taper {
                start: clean(taper.start),
                end: clean(taper.end),
                ..taper
            },
            min_width: if min_width.is_finite() {
                min_width.max(0.0)
            } else {
                0.0
            },
            pending: Vec::new(),
            committed: Vec::new(),
            start: None,
            speeds: (0.0, 0),
            start_speed: None,
        }
    }

    pub fn committed(&self) -> &[StrokePoint] {
        &self.committed
    }

    /// The newest point taken so far, pending or committed.
    pub fn last(&self) -> Option<&StrokePoint> {
        self.pending.last().or_else(|| self.committed.last())
    }

    /// Notes a point's speed while the starting time lasts.
    fn measure(&mut self, point: &StrokePoint) {
        if self.start_speed.is_some() {
            return;
        }
        let start = *self.start.get_or_insert(point.time);
        if point.time - start <= START_SPEED_TIME {
            self.speeds.0 += point.speed / FULL_SPEED;
            self.speeds.1 += 1;
        } else {
            self.start_speed = Some(self.measured());
        }
    }

    /// The mean speed share so far.
    fn measured(&self) -> f32 {
        match self.speeds {
            (_, 0) => 0.0,
            (sum, count) => (sum / count as f32).clamp(0.0, 1.0),
        }
    }

    /// Whether any taper is set.
    fn tapers(&self) -> bool {
        self.taper.start > 0.0 || self.taper.end > 0.0
    }

    /// Takes a point of a final segment, and commits what it can.
    pub fn push(&mut self, point: StrokePoint) {
        self.measure(&point);
        let known = point.arc;
        self.pending.push(point);
        let settled = self.pending.iter().take_while(|p| self.settled(p, known));
        let count = settled.count();
        if count > 0 {
            let start_speed = self.start_speed.unwrap_or(0.0);
            let mut done: Vec<StrokePoint> = self.pending.drain(..count).collect();
            for p in &mut done {
                // Settled points have no ending, and a settled starting
                // does not depend on the length.
                let factor = self.factor(p.arc, f32::INFINITY, start_speed, 0.0);
                self.apply(p, factor);
            }
            self.committed.extend(done);
        }
    }

    /// Whether the point can no longer change once the curve is final
    /// up to arc length `known`.
    fn settled(&self, point: &StrokePoint, known: f32) -> bool {
        if !self.tapers() {
            return point.arc < known;
        }
        let stretch = if self.taper.by_speed { BY_SPEED } else { 1.0 };
        let (start, end) = (self.taper.start * stretch, self.taper.end * stretch);
        let speed_known = !self.taper.by_speed || self.start_speed.is_some();
        match self.taper.mode {
            TaperMode::Percent => false,
            TaperMode::Fade => speed_known && point.arc < known,
            TaperMode::Length => {
                let start_settled = point.arc >= start
                    || (speed_known && known >= 1.0 && known * FIT >= start + end);
                start_settled && point.arc < known - end
            }
        }
    }

    /// The taper factor at arc length `s` of a stroke `length` long
    /// (infinite: not known, no ending), with the starting and ending
    /// speed shares.
    fn factor(&self, s: f32, length: f32, start_speed: f32, end_speed: f32) -> f32 {
        if !self.tapers() {
            return 1.0;
        }
        let (mut start, mut end) = match self.taper.mode {
            TaperMode::Percent if length.is_finite() => (
                self.taper.start / 100.0 * length,
                self.taper.end / 100.0 * length,
            ),
            _ => (self.taper.start, self.taper.end),
        };
        if self.taper.by_speed {
            start *= BY_SPEED * start_speed;
            end *= BY_SPEED
                * if self.taper.mode == TaperMode::Fade {
                    start_speed
                } else {
                    end_speed
                };
        }
        if self.taper.mode == TaperMode::Fade {
            let up = if start > 0.0 { ease(s / start) } else { 1.0 };
            let down = if end > 0.0 { ease(1.0 - s / end) } else { 1.0 };
            return up.min(down);
        }
        if length.is_finite() {
            if length < 1.0 {
                return 1.0;
            }
            let fit = (start + end) / (FIT * length);
            if fit > 1.0 {
                start /= fit;
                end /= fit;
            }
        }
        let up = if start > 0.0 { ease(s / start) } else { 1.0 };
        let down = if end > 0.0 && length.is_finite() {
            ease((length - s) / end)
        } else {
            1.0
        };
        up.min(down)
    }

    /// The length of the ending window of a stroke `length` long whose
    /// end moves at `end_speed`, after fitting.
    fn end_window(&self, length: f32, start_speed: f32, end_speed: f32) -> f32 {
        if self.taper.mode == TaperMode::Fade || self.taper.end <= 0.0 || length < 1.0 {
            return 0.0;
        }
        let (mut start, mut end) = match self.taper.mode {
            TaperMode::Percent => (
                self.taper.start / 100.0 * length,
                self.taper.end / 100.0 * length,
            ),
            _ => (self.taper.start, self.taper.end),
        };
        if self.taper.by_speed {
            start *= BY_SPEED * start_speed;
            end *= BY_SPEED * end_speed;
        }
        let fit = (start + end) / (FIT * length);
        if fit > 1.0 {
            end /= fit;
        }
        end
    }

    /// `factor` applied to the point, as the brush says, and the width
    /// kept at the least the brush allows.
    fn apply(&self, point: &mut StrokePoint, factor: f32) {
        point.taper = factor;
        if self.taper.size {
            point.size *= factor;
        }
        if self.taper.opacity {
            point.opacity *= factor;
        }
        point.size = point.size.max(self.min_width);
    }

    /// The pending points and then `provisional`, shaped for a stroke
    /// that ends at the last of them.
    fn shaped(&self, provisional: &[StrokePoint]) -> Vec<StrokePoint> {
        let mut points: Vec<StrokePoint> =
            self.pending.iter().chain(provisional).cloned().collect();
        let Some(last) = points.last() else {
            return points;
        };
        let length = last.arc;
        let end_speed = (last.speed / FULL_SPEED).clamp(0.0, 1.0);
        let start_speed = self.start_speed.unwrap_or_else(|| {
            // Still within the starting time: what was seen so far, the
            // provisional points included.
            let (mut sum, mut count) = self.speeds;
            if let Some(start) = self.start {
                for p in provisional {
                    if p.time - start <= START_SPEED_TIME {
                        sum += p.speed / FULL_SPEED;
                        count += 1;
                    }
                }
            }
            if count == 0 {
                0.0
            } else {
                (sum / count as f32).clamp(0.0, 1.0)
            }
        });
        for point in &mut points {
            let factor = self.factor(point.arc, length, start_speed, end_speed);
            self.apply(point, factor);
        }
        let window = self.end_window(length, start_speed, end_speed);
        if window > 0.0 {
            hold_down(&mut points, length - window);
        }
        points
    }

    /// The tail: what is not committed, with `provisional` after it.
    pub fn tail(&self, provisional: &[StrokePoint]) -> Vec<StrokePoint> {
        self.shaped(provisional)
    }

    /// The lift: the last points come, and everything is committed.
    pub fn finish(&mut self, last: &[StrokePoint]) {
        for point in last {
            self.measure(point);
        }
        if self.start_speed.is_none() {
            self.start_speed = Some(self.measured());
        }
        let shaped = self.shaped(last);
        self.pending.clear();
        self.committed.extend(shaped);
    }
}

/// Keeps size and opacity from growing along the points from arc length
/// `from` on. Points at the same arc length (the pen pressing in place)
/// are one place: together they may be as big as the place before them,
/// and the place after them may be as big as the biggest of them.
fn hold_down(points: &mut [StrokePoint], from: f32) {
    const SAME_PLACE: f32 = 1e-4;
    let Some(first) = points.iter().position(|p| p.arc >= from) else {
        return;
    };
    // What the place before allows, and the biggest of this place.
    let mut allowed = (points[first].size, points[first].opacity);
    let mut biggest = allowed;
    let mut place = points[first].arc;
    for point in &mut points[first..] {
        if point.arc > place + SAME_PLACE {
            allowed = biggest;
            biggest = (0.0, 0.0);
            place = point.arc;
        }
        point.size = point.size.min(allowed.0);
        point.opacity = point.opacity.min(allowed.1);
        biggest = (biggest.0.max(point.size), biggest.1.max(point.opacity));
    }
}
