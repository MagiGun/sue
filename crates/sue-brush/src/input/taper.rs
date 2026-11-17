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
//! as it is drawn). A device with no pressure (a mouse) has nothing
//! else to thin its ends, so by speed its tapers keep at least their set
//! lengths and stretch from there: a slow mouse stroke still comes to a
//! point, a quick one to a longer one, as a pen user's would.
//!
//! An ending comes to a clean point or not at all. A tip that thins
//! slower than it advances is pointed; one that thins faster ends in
//! the round end of its widest disc. An ending between a quarter of
//! [`ENDING_PER_WIDTH`] widths of the line near its end and that many
//! widths would be both, a blunt wedge with a rounded nose that reads
//! as a cut, so it is made that many widths long (as the stroke's
//! length allows). Shorter than that quarter it is the round end it
//! already looks like (a slow pen, whose pressure thins its end). A
//! flick's run out lies inside the ending, which then reaches at least
//! over it.
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

/// The shortest pointed ending, in widths of the line near its end.
pub(crate) const ENDING_PER_WIDTH: f32 = 2.0;

/// How much longer an ending is made at most to come to a clean point,
/// as an area: its length times the brush's size, in square pixels.
const EXTRA_WORK: f32 = 6000.0;

/// The share of the shortest pointed ending under which an ending is a
/// round end and is left as it is.
const ROUND_BELOW: f32 = 0.25;

/// What an ending must reach over at least: a pointed ending's length
/// for the line's width near its end, and a flick's run out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Floor {
    pointed: f32,
    run_out: f32,
}

impl Floor {
    /// The ending `end` long made clean: a round end is left, a blunt
    /// wedge made pointed, and it reaches over the run out.
    fn widen(self, end: f32) -> f32 {
        let pointed = if end >= self.pointed * ROUND_BELOW {
            end.max(self.pointed)
        } else {
            end
        };
        pointed.max(self.run_out)
    }
}

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
    /// The newest committed point, once the committed points were
    /// taken away ([`Self::take_committed`]).
    taken_last: Option<StrokePoint>,
    /// The first point's time.
    start: Option<f64>,
    /// The speeds seen over the starting time: their sum and count.
    speeds: (f32, u32),
    /// The mean speed share over the starting time, once it is over.
    start_speed: Option<f32>,
    /// The device has no pressure (a mouse): by speed, the tapers keep
    /// at least their set lengths.
    pressureless: bool,
    /// The brush's size before its dynamics: the widest a line can be,
    /// so the longest an ending can be made.
    base_size: f32,
}

impl Shaper {
    pub fn new(taper: Taper, min_width: f32, pressureless: bool, base_size: f32) -> Self {
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
            taken_last: None,
            start: None,
            speeds: (0.0, 0),
            start_speed: None,
            pressureless,
            base_size: if base_size.is_finite() {
                base_size.max(0.0)
            } else {
                0.0
            },
        }
    }

    /// `true` when the brush has an ending taper of a length (not a
    /// fade).
    pub fn ends_tapered(&self) -> bool {
        self.taper.end > 0.0 && self.taper.mode != TaperMode::Fade
    }

    /// The longest ending window a stroke of this brush can have in
    /// length mode, at any speed and width: its set length (stretched
    /// at full speed), made longer for a clean point by at most
    /// [`EXTRA_WORK`] square pixels of the line. The ending is drawn
    /// again on every frame while the pen is down, so a very wide brush
    /// with a short ending is not given a long one (its frames would
    /// slow down), and at that width the short ending reads as a round
    /// end anyway.
    pub fn longest_ending(&self) -> f32 {
        let stretch = if self.taper.by_speed { BY_SPEED } else { 1.0 };
        let set = self.taper.end * stretch;
        let wanted = (ENDING_PER_WIDTH * self.base_size - set).max(0.0);
        set + wanted.min(EXTRA_WORK / self.base_size.max(1.0))
    }

    /// How much "by speed" stretches a taper at speed share `share`: up
    /// to [`BY_SPEED`] times at full speed, from nothing for a pen and
    /// from its set length for a device with no pressure.
    fn stretch(&self, share: f32) -> f32 {
        if self.pressureless {
            1.0 + (BY_SPEED - 1.0) * share
        } else {
            BY_SPEED * share
        }
    }

    pub fn committed(&self) -> &[StrokePoint] {
        &self.committed
    }

    /// The newest point taken so far, pending or committed.
    pub fn last(&self) -> Option<&StrokePoint> {
        self.pending
            .last()
            .or_else(|| self.committed.last())
            .or(self.taken_last.as_ref())
    }

    /// How many points are held: committed and not taken, and pending.
    pub fn held(&self) -> usize {
        self.committed.len() + self.pending.len()
    }

    /// Takes the committed points away, so a long stroke does not keep
    /// every point it ever made: whoever lays them has them now.
    pub fn take_committed(&mut self) -> Vec<StrokePoint> {
        if let Some(last) = self.committed.last() {
            self.taken_last = Some(last.clone());
        }
        std::mem::take(&mut self.committed)
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
                let factor = self.factor(p.arc, f32::INFINITY, start_speed, 0.0, Floor::default());
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
        let start = self.taper.start * stretch;
        let end = if self.ends_tapered() {
            self.longest_ending()
        } else {
            0.0
        };
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
    /// speed shares and the shortest the ending may be (`floor`).
    fn factor(&self, s: f32, length: f32, start_speed: f32, end_speed: f32, floor: Floor) -> f32 {
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
            start *= self.stretch(start_speed);
            end *= self.stretch(if self.taper.mode == TaperMode::Fade {
                start_speed
            } else {
                end_speed
            });
        }
        if self.ends_tapered() {
            end = floor.widen(end);
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
    /// end moves at `end_speed`, at least `floor`, after fitting.
    fn end_window(&self, length: f32, start_speed: f32, end_speed: f32, floor: Floor) -> f32 {
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
            start *= self.stretch(start_speed);
            end *= self.stretch(end_speed);
        }
        end = floor.widen(end);
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

    /// What the ending of `points` (before the tapers), the last
    /// `run_out` pixels of which are a flick's run out, reaches over at
    /// least: the run out, and if pointed, [`ENDING_PER_WIDTH`] times the
    /// widest the line is near its end, that never more than
    /// [`Self::longest_ending`].
    fn ending_floor(&self, points: &[StrokePoint], length: f32, run_out: f32) -> Floor {
        if !self.ends_tapered() {
            return Floor::default();
        }
        let near = length - self.longest_ending();
        let widest = points
            .iter()
            .rev()
            .take_while(|p| p.arc >= near)
            .map(|p| p.size)
            .fold(0.0_f32, f32::max);
        let most = self.longest_ending();
        Floor {
            pointed: (ENDING_PER_WIDTH * widest).min(most),
            // The run out lies past every point committed while the pen
            // was down, so an ending over it alone changes none of them.
            run_out,
        }
    }

    /// The pending points and then `provisional`, shaped for a stroke
    /// that ends at the last of them, the last `run_out` pixels a
    /// flick's run out.
    fn shaped(&self, provisional: &[StrokePoint], run_out: f32) -> Vec<StrokePoint> {
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
        let floor = self.ending_floor(&points, length, run_out);
        for point in &mut points {
            let factor = self.factor(point.arc, length, start_speed, end_speed, floor);
            self.apply(point, factor);
        }
        let window = self.end_window(length, start_speed, end_speed, floor);
        if window > 0.0 {
            hold_down(&mut points, length - window);
        }
        points
    }

    /// The tail: what is not committed, with `provisional` after it.
    pub fn tail(&self, provisional: &[StrokePoint]) -> Vec<StrokePoint> {
        self.shaped(provisional, 0.0)
    }

    /// The lift: the last points come (the last `run_out` pixels of
    /// them a flick's run out), and everything is committed.
    pub fn finish(&mut self, last: &[StrokePoint], run_out: f32) {
        for point in last {
            self.measure(point);
        }
        if self.start_speed.is_none() {
            self.start_speed = Some(self.measured());
        }
        let shaped = self.shaped(last, run_out);
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
