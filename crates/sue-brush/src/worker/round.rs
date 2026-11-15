//! The round tip swept along the stroke's points: the pens and pencils.
//!
//! The input pipeline hands a point every twentieth of the brush's size
//! (at most two pixels apart). Laying one capsule per pair of them would
//! make a big brush lay the same pixels hundreds of times: a 1,000 px
//! brush moved 100 px would work out fifty million pixels. So points
//! that lie on one capsule within a fifth of a pixel (place and radius)
//! and keep their strength within half a step of the layer's bytes are
//! laid as that one capsule. Whether a point joins a run depends only on
//! the points, never on how they were cut into updates: the run still
//! open is drawn with the tail, and only a closed run is committed.

use sue_model::BrushPreset;
use sue_model::brush::{AntiAlias, BrushRegistry, param};

use crate::input::StrokePoint;
use crate::raster::{Capsule, Disc, PixelRect, RoundTip, Shape};

/// The antialiasing width of a hard rim, in pixels (the legacy engine's
/// widths, so a pen keeps its edge).
#[must_use]
pub const fn edge(antialias: AntiAlias) -> f32 {
    match antialias {
        AntiAlias::None => 0.0,
        AntiAlias::Weak => 0.9,
        AntiAlias::Medium => 1.5,
        AntiAlias::Strong => 2.5,
    }
}

/// How far a point may lie off the capsule that stands for it, in pixels.
const PLACE: f32 = 0.2;
/// How far the strength may change along a run: under half a step of an
/// 8-bit layer.
const STRENGTH: f32 = 0.5 / 255.0;
/// How far its hardness may differ.
const HARDNESS: f32 = 1.0 / 256.0;
/// The most points one capsule stands for (so checking a run stays cheap).
const RUN: usize = 64;

/// A point as the round tip lays it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spot {
    disc: Disc,
    hardness: f32,
}

/// `true` when every spot of `inner` lies on the capsule from `a` to `b`
/// within the tolerances.
fn fits(a: Spot, b: Spot, inner: &[Spot]) -> bool {
    // The strength stays as it is along a run. A capsule lays its
    // strength at each pixel's nearest point, where the discs of a run of
    // points would lay the most of the discs over it; with an even
    // strength the two are the same, and a changing one (a pencil whose
    // opacity follows pressure) would show steps at every joint.
    if (b.disc.strength - a.disc.strength).abs() > STRENGTH {
        return false;
    }
    let (dx, dy) = (b.disc.at[0] - a.disc.at[0], b.disc.at[1] - a.disc.at[1]);
    let length2 = dx * dx + dy * dy;
    inner.iter().all(|q| {
        let (px, py) = (q.disc.at[0] - a.disc.at[0], q.disc.at[1] - a.disc.at[1]);
        let t = if length2 > 0.0 {
            ((px * dx + py * dy) / length2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (ex, ey) = (px - t * dx, py - t * dy);
        let radius = a.disc.radius + (b.disc.radius - a.disc.radius) * t;
        ex * ex + ey * ey <= PLACE * PLACE
            && (q.disc.radius - radius).abs() <= PLACE
            && (q.disc.strength - a.disc.strength).abs() <= STRENGTH
            && (q.hardness - b.hardness).abs() <= HARDNESS
            && (q.hardness - a.hardness).abs() <= HARDNESS
    })
}

/// Runs of spots, each closed run one capsule.
#[derive(Clone, Debug, Default)]
struct Runs {
    /// Where the open run starts: the end of the last closed one.
    anchor: Option<Spot>,
    /// The spots after the anchor, not laid as a closed capsule yet.
    run: Vec<Spot>,
}

impl Runs {
    /// Takes the next spot; a run it closes goes to `out` as a capsule.
    fn push(&mut self, spot: Spot, edge: f32, out: &mut Vec<Swept>) {
        let Some(anchor) = self.anchor else {
            // The stroke's first spot: a dot, so a tap shows.
            out.push(Swept::whole(
                spot.disc,
                spot.disc,
                RoundTip::new(spot.hardness, edge),
            ));
            self.anchor = Some(spot);
            return;
        };
        if self.run.len() < RUN && fits(anchor, spot, &self.run) {
            self.run.push(spot);
            return;
        }
        // The spot does not fit: the run so far is closed.
        if let Some(&end) = self.run.last() {
            out.push(capsule(anchor, end, edge));
            self.anchor = Some(end);
            self.run.clear();
            self.run.push(spot);
        } else {
            self.run.push(spot);
        }
    }

    /// The open run as a capsule, if there is one.
    fn open(&self, edge: f32) -> Option<Swept> {
        Some(capsule(self.anchor?, *self.run.last()?, edge))
    }
}

/// The capsule from `a` to `b`, with `b`'s hardness, laid only ahead
/// of `a` (see [`Swept`]). When the hardness changes between the two (a
/// hardness that follows the pen), the disc laid at `a` had another rim
/// than this capsule has there, so the capsule is laid whole.
fn capsule(a: Spot, b: Spot, edge: f32) -> Swept {
    let tip = RoundTip::new(b.hardness, edge);
    if (a.hardness - b.hardness).abs() > HARDNESS {
        Swept::whole(a.disc, b.disc, tip)
    } else {
        Swept::after(a.disc, b.disc, tip)
    }
}

/// A capsule that goes on from a disc already laid at its start, laid
/// only where it can add to that disc.
///
/// Every capsule of a stroke but the first dot starts at the disc the
/// one before it ended at, which is laid already in the same part (or in
/// the committed part under the tail). Where the capsule's nearest disc
/// is its first one, it lays no more than that disc did, and the most of
/// the two (the pens' and pencils' combine) is what is there already. So
/// on each row only the pixels ahead of a line across the start, a pixel
/// behind the farthest the nearest disc can sit at the start, are worked
/// out: a big brush moved a little works out the new half of its disc
/// and the band it swept, not the whole disc again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swept {
    capsule: Capsule,
    /// The way along the segment (unit vector), and the least distance
    /// along it from the start a worked out pixel lies at; `None` lays
    /// the whole capsule.
    ahead: Option<([f32; 2], f32)>,
}

impl Swept {
    /// The whole capsule from `a` to `b`.
    #[must_use]
    pub fn whole(a: Disc, b: Disc, tip: RoundTip) -> Self {
        Self {
            capsule: Capsule::new(a, b, tip),
            ahead: None,
        }
    }

    /// The capsule from `a` (laid already) to `b`, laid ahead of `a`.
    #[must_use]
    pub fn after(a: Disc, b: Disc, tip: RoundTip) -> Self {
        let mut swept = Self::whole(a, b, tip);
        let (a, b) = (a.drawn(false), b.drawn(false));
        let (dx, dy) = (b.at[0] - a.at[0], b.at[1] - a.at[1]);
        let length = (dx * dx + dy * dy).sqrt();
        // Small discs keep the whole capsule: their rim profile is not
        // the same function of distance at every radius.
        if !(length.is_finite() && length >= 0.5 && a.radius >= 2.0 && b.radius >= 2.0) {
            return swept;
        }
        let slope = (b.radius - a.radius) / length;
        let reach = a.radius.max(b.radius) + tip.reach() + 1.0;
        let behind = if tip.is_hard() {
            // The nearest point of the outline is at the start while the
            // distance along the axis is at most -slope * y / sqrt(1 -
            // slope^2), y the distance off it (at most `reach`).
            if slope >= 0.9 {
                return swept;
            }
            slope.max(0.0) * reach / (1.0 - slope * slope).sqrt()
        } else {
            // The smallest share of a radius is at the start while
            // r_a * along * length + grow * |q|^2 <= 0.
            slope.max(0.0) * reach * reach / a.radius
        };
        swept.ahead = Some(([dx / length, dy / length], -behind - 1.0));
        swept
    }

    /// The same capsule laid whole: for a combine that adds coverage up,
    /// where laying the start again is not free.
    #[must_use]
    pub const fn laid_whole(mut self) -> Self {
        self.ahead = None;
        self
    }

    /// The capsule.
    #[must_use]
    pub const fn capsule(&self) -> &Capsule {
        &self.capsule
    }
}

impl Shape for Swept {
    fn bounds(&self) -> PixelRect {
        self.capsule.bounds()
    }

    fn span(&self, y: f32) -> Option<(f32, f32)> {
        let (lo, hi) = self.capsule.span(y)?;
        let Some(([ux, uy], least)) = self.ahead else {
            return Some((lo, hi));
        };
        // Along the axis from the start: (x - ax) ux + (y - ay) uy, which
        // must be over `least`.
        let a = self.capsule.start().at;
        let rest = least - (y - a[1]) * uy;
        if ux.abs() < 1.0e-6 {
            return (rest < 0.0).then_some((lo, hi));
        }
        let edge = a[0] + rest / ux;
        let (lo, hi) = if ux > 0.0 {
            (lo.max(edge), hi)
        } else {
            (lo, hi.min(edge))
        };
        (lo <= hi).then_some((lo, hi))
    }

    fn coverage(&self, p: [f32; 2]) -> f32 {
        self.capsule.coverage(p)
    }
}

/// Turns stroke points into the capsules of a round tip.
#[derive(Clone, Debug)]
pub struct Round {
    edge: f32,
    at_least_one: bool,
    /// The stroke's opacity, laid once at the merge; a point carries its
    /// own opacity as a share of it.
    opacity: f32,
    /// Coverage adds up (build-up): every capsule is laid whole.
    adds_up: bool,
    runs: Runs,
    /// How many committed points are taken already.
    taken: usize,
}

impl Round {
    /// The round tip of `preset`, whose parameters `registry` knows.
    #[must_use]
    pub fn new(preset: &BrushPreset, registry: &BrushRegistry) -> Self {
        let opacity = registry
            .value(preset, param::OPACITY)
            .filter(|value| value.is_finite())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        Self {
            edge: edge(preset.antialias),
            at_least_one: preset.min_width >= 1.0,
            opacity,
            adds_up: preset.build_up,
            runs: Runs::default(),
            taken: 0,
        }
    }

    /// The stroke's opacity.
    #[must_use]
    pub const fn opacity(&self) -> f32 {
        self.opacity
    }

    /// The spot a point lays.
    fn spot(&self, point: &StrokePoint) -> Spot {
        let unit = |value: f32| {
            if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let share = if self.opacity > 0.0 {
            unit(point.opacity / self.opacity)
        } else {
            0.0
        };
        let strength = share * unit(point.flow) * unit(point.density);
        Spot {
            disc: Disc::new(point.pos, point.size * 0.5, strength).drawn(self.at_least_one),
            hardness: unit(point.hardness),
        }
    }

    /// The capsules of closed runs among the committed points after the
    /// ones taken already (`committed` is every committed point so far).
    pub fn committed(&mut self, committed: &[StrokePoint], out: &mut Vec<Swept>) {
        let from = out.len();
        for point in committed.iter().skip(self.taken) {
            let spot = self.spot(point);
            self.runs.push(spot, self.edge, out);
        }
        self.taken = self.taken.max(committed.len());
        self.whole(&mut out[from..]);
    }

    /// The capsules of the open run and of `tail`, which goes on from the
    /// committed points.
    #[must_use]
    pub fn tail(&self, tail: &[StrokePoint]) -> Vec<Swept> {
        let mut runs = self.runs.clone();
        let mut out = Vec::new();
        for point in tail {
            runs.push(self.spot(point), self.edge, &mut out);
        }
        out.extend(runs.open(self.edge));
        self.whole(&mut out);
        out
    }

    /// Lays `shapes` whole when coverage adds up.
    fn whole(&self, shapes: &mut [Swept]) {
        if self.adds_up {
            for shape in shapes {
                *shape = shape.laid_whole();
            }
        }
    }

    /// Closes the open run (the stroke has ended): its capsule, if any.
    pub fn close(&mut self, out: &mut Vec<Swept>) {
        let from = out.len();
        out.extend(self.runs.open(self.edge));
        self.whole(&mut out[from..]);
        if let Some(&end) = self.runs.run.last() {
            self.runs.anchor = Some(end);
            self.runs.run.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(x: f32, radius: f32) -> Spot {
        Spot {
            disc: Disc::new([x, 0.0], radius, 1.0),
            hardness: 1.0,
        }
    }

    #[test]
    fn a_straight_even_run_is_one_capsule_and_a_bend_closes_it() {
        let mut runs = Runs::default();
        let mut out = Vec::new();
        for i in 0..20 {
            runs.push(spot(i as f32 * 2.0, 50.0), 0.9, &mut out);
        }
        assert_eq!(out.len(), 1, "the first dot only");
        let open = runs.open(0.9).expect("an open run");
        assert_eq!(open.capsule().start().at, [0.0, 0.0]);
        assert_eq!(open.capsule().end().at, [38.0, 0.0]);
        // A point off the line closes the run at the point before it.
        runs.push(
            Spot {
                disc: Disc::new([40.0, 3.0], 50.0, 1.0),
                hardness: 1.0,
            },
            0.9,
            &mut out,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].capsule().end().at, [38.0, 0.0]);
        // So does a radius that does not change evenly.
        let mut runs = Runs::default();
        let mut out = Vec::new();
        for (i, radius) in [5.0, 5.0, 5.0, 9.0].into_iter().enumerate() {
            runs.push(spot(i as f32, radius), 0.9, &mut out);
        }
        assert_eq!(out.len(), 2);
    }

    /// Laying only the part of a capsule ahead of its start disc, over
    /// that disc, gives every pixel the coverage the whole capsule over
    /// the disc gives (within one step of the 16-bit coverage, which no
    /// layer byte can show), for hard and soft
    /// tips, growing and shrinking radii, short and long segments.
    #[test]
    fn a_capsule_laid_ahead_of_its_start_lays_what_the_whole_one_does() {
        use crate::buffer::{Combine, StrokeBuffer};

        // A small fixed generator, so the cases are the same every run.
        let mut state = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 40) as f32 / (1_u64 << 24) as f32
        };
        let mut cases = 0;
        let mut most = 0;
        for hardness in [1.0, 0.85, 0.5, 0.0] {
            for edge in [0.0, 0.9, 2.5] {
                for _ in 0..24 {
                    let ra = 2.0 + 90.0 * next() * next();
                    let rb = (ra * (0.5 + next())).max(2.0);
                    let length = 0.5 + 40.0 * next() * next();
                    let turn = std::f32::consts::TAU * next();
                    let at = [300.5 + 7.0 * next(), 300.25 + 7.0 * next()];
                    let a = Disc::new(at, ra, 1.0).drawn(true);
                    let b = Disc::new(
                        [at[0] + length * turn.cos(), at[1] + length * turn.sin()],
                        rb,
                        1.0,
                    )
                    .drawn(true);
                    let tip = RoundTip::new(hardness, edge);
                    let lay = |second: Swept| {
                        let mut buffer = StrokeBuffer::new(Combine::Max);
                        {
                            let mut painter = buffer.committed();
                            painter.fill(&Swept::whole(a, a, tip));
                            painter.fill(&second);
                        }
                        buffer
                    };
                    let ahead = lay(Swept::after(a, b, tip));
                    let whole = lay(Swept::whole(a, b, tip));
                    let r = Swept::whole(a, b, tip).bounds();
                    for y in r.y0..r.y1 {
                        for x in r.x0..r.x1 {
                            let (shortcut, full) = (ahead.coverage(x, y), whole.coverage(x, y));
                            // One step of the 16-bit coverage is float
                            // rounding where the nearest disc sits a hair
                            // ahead of the start: far under a byte's step.
                            assert!(
                                shortcut.abs_diff(full) <= 1,
                                "({x}, {y}): {shortcut} against {full}, {a:?} to {b:?},                                  hardness {hardness}, edge {edge}"
                            );
                            most = most.max(shortcut.abs_diff(full));
                        }
                    }
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 4 * 3 * 24);
        assert!(most <= 1, "{most}");
    }
}
