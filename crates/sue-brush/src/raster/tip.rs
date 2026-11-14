//! The round tip's profile: how much paint a disc lays at a distance from
//! its center.
//!
//! Both profiles are box-filtered across the line, which is what keeps a
//! thin line even. A line's cross-section summed over the pixels of one
//! column is the same wherever the line falls between pixel centers
//! exactly when the cross-section is something convolved with a box one
//! pixel wide (the box's spectrum is zero at every whole frequency).
//! Without it a thin line ropes: it brightens where it runs over a row of
//! pixel centers and fades between two.

/// The profile of a round tip. A hard tip (hardness 1) is a disc whose
/// rim is smoothed over `edge` pixels, the antialiasing width; a softer
/// one fades from `hardness` of its radius out to the rim with a
/// smoothstep, and is then smoothed over one pixel. The hard rim and the
/// soft fade are the legacy engine's (`Tip::coverage` in
/// `paint/brush.rs`), so a brush keeps its look at every size where the
/// legacy engine drew it evenly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundTip {
    hardness: f32,
    edge: f32,
}

impl RoundTip {
    /// A hardness from this up draws a hard tip with an antialiased rim.
    pub const HARD: f32 = 0.99;

    /// The box a soft tip's profile is smoothed with, in pixels.
    const PIXEL: f32 = 1.0;

    /// A tip of `hardness` (0 to 1) whose hard rim is smoothed over
    /// `edge` pixels (0 for no antialiasing; the presets use 0.9, 1.5
    /// and 2.5).
    #[must_use]
    pub fn new(hardness: f32, edge: f32) -> Self {
        let clean = |v: f32, max: f32| {
            if v.is_finite() {
                v.clamp(0.0, max)
            } else {
                0.0
            }
        };
        Self {
            hardness: clean(hardness, 1.0),
            edge: clean(edge, 64.0),
        }
    }

    /// A hard tip with an antialiased rim of `edge` pixels.
    #[must_use]
    pub fn hard(edge: f32) -> Self {
        Self::new(1.0, edge)
    }

    /// The tip's hardness, 0 to 1.
    #[must_use]
    pub const fn hardness(&self) -> f32 {
        self.hardness
    }

    /// The width of a hard tip's smoothed rim, in pixels.
    #[must_use]
    pub const fn edge(&self) -> f32 {
        self.edge
    }

    /// `true` for a hard tip, whose coverage follows the distance to its
    /// rim in pixels rather than a share of its radius.
    #[must_use]
    pub fn is_hard(&self) -> bool {
        self.hardness >= Self::HARD
    }

    /// How far beyond its radius the tip still lays paint, in pixels.
    #[must_use]
    pub fn reach(&self) -> f32 {
        if self.is_hard() {
            // An edge narrower than a pixel is drawn a pixel wide.
            self.edge.max(Self::PIXEL) * 0.5
        } else {
            Self::PIXEL * 0.5
        }
    }

    /// A hard tip's coverage at `distance` pixels outside its rim
    /// (negative inside), where the tip's radius there is `radius`.
    ///
    /// The rim is a filter `edge` pixels wide run over the line's
    /// cross-section, a band `2 * radius` wide: the near rim's ramp less
    /// what of the filter falls past the far rim. The filter is a box of
    /// one pixel run through a box of the rest of the edge, so the ramp
    /// is as wide as the edge says and a line of any width sums to the
    /// same down every column. An edge narrower than a pixel is drawn a
    /// pixel wide: a narrower one cannot be sampled evenly. For a line
    /// wider than its edge the far rim never comes into it; a thinner
    /// one is lighter in the middle, as much as it is narrower than the
    /// filter, instead of keeping a full peak.
    #[must_use]
    pub fn hard_at(&self, distance: f32, radius: f32) -> f32 {
        if self.edge <= 0.0 {
            return if distance <= 0.0 { 1.0 } else { 0.0 };
        }
        // The far rim is across the band; and never nearer than the near
        // one, which matters for an ellipse read along its long axis.
        let far = (distance + 2.0 * radius.max(0.0)).max(-distance);
        (self.ramp(distance) - self.ramp(far)).max(0.0)
    }

    /// How much of the rim filter lies beyond `s` pixels: 1 well inside
    /// the rim, 0 well outside, 0.5 on it.
    fn ramp(&self, s: f32) -> f32 {
        let edge = self.edge.max(Self::PIXEL);
        let rest = edge - Self::PIXEL;
        // Half widths of the two boxes, the narrower first.
        let (a, b) = (rest.min(Self::PIXEL) * 0.5, rest.max(Self::PIXEL) * 0.5);
        if s <= -(a + b) {
            return 1.0;
        }
        if s >= a + b {
            return 0.0;
        }
        if a < 1.0e-4 {
            return 0.5 - s / (2.0 * b);
        }
        // The share of the sum of two uniform variables, on -a..a and
        // -b..b, that lies below `s`.
        let r = |u: f32| if u > 0.0 { 0.5 * u * u } else { 0.0 };
        let below = (r(s + a + b) - r(s + a - b) - r(s - a + b) + r(s - a - b)) / (4.0 * a * b);
        (1.0 - below).clamp(0.0, 1.0)
    }

    /// A soft tip's coverage at `distance` pixels from the center of a
    /// disc of `radius`: the smoothstep fade from `hardness * radius` to
    /// `radius`, averaged over one pixel across the line.
    #[must_use]
    pub fn soft_at(&self, distance: f32, radius: f32) -> f32 {
        let radius = radius.max(1.0e-3);
        let inner = self.hardness.min(0.98) * radius;
        let fade = radius - inner;
        // The fade's integral from the center out to `s` (odd in `s`).
        let integral = |s: f32| {
            let u = s.abs();
            let value = if u <= inner {
                u
            } else if u >= radius {
                inner + fade * 0.5
            } else {
                let v = (u - inner) / fade;
                // The integral of 1 - (3 v^2 - 2 v^3) from 0 to v.
                inner + fade * (v - v * v * v + 0.5 * v * v * v * v)
            };
            if s < 0.0 { -value } else { value }
        };
        let half = Self::PIXEL * 0.5;
        ((integral(distance + half) - integral(distance - half)) / Self::PIXEL).clamp(0.0, 1.0)
    }

    /// A disc's coverage at `distance` from its center, for `radius`.
    #[must_use]
    pub fn disc_at(&self, distance: f32, radius: f32) -> f32 {
        if self.is_hard() {
            self.hard_at(distance - radius, radius)
        } else {
            self.soft_at(distance, radius)
        }
    }
}

impl Default for RoundTip {
    /// A hard tip with the weak antialiasing most pens use.
    fn default() -> Self {
        Self::hard(0.9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hard_rim_is_half_covered_on_the_circle() {
        for edge in [0.5_f32, 0.9, 1.0, 1.5, 2.0, 2.5, 7.0] {
            let tip = RoundTip::hard(edge);
            let half = edge.max(1.0) * 0.5;
            assert!((tip.disc_at(10.0, 10.0) - 0.5).abs() < 1.0e-5, "{edge}");
            assert_eq!(tip.disc_at(10.0 - half, 10.0), 1.0, "{edge}");
            assert_eq!(tip.disc_at(10.0 + half, 10.0), 0.0, "{edge}");
            let mut last = 1.0;
            for step in 0..=100 {
                let c = tip.disc_at(6.0 + step as f32 * 0.08, 10.0);
                assert!(c <= last + 1.0e-6, "{edge}: the rim never rises outward");
                last = c;
            }
        }
        let aliased = RoundTip::hard(0.0);
        assert_eq!(aliased.disc_at(10.0, 10.0), 1.0);
        assert_eq!(aliased.disc_at(10.01, 10.0), 0.0);
    }

    #[test]
    fn a_line_thinner_than_the_rim_is_lighter_not_peaked() {
        // A band 0.6 wide under a box 1 wide: at most 0.6 of the box.
        let tip = RoundTip::hard(1.0);
        assert!((tip.disc_at(0.0, 0.3) - 0.6).abs() < 1.0e-6);
        // Its cross-section sums to its width wherever it falls, for
        // every edge and every width.
        for edge in [0.9_f32, 1.5, 2.5] {
            let tip = RoundTip::hard(edge);
            for radius in [0.3_f32, 0.7, 1.1, 2.6] {
                for k in 0..10 {
                    let shift = k as f32 * 0.1;
                    let sum: f32 = (-9..=9)
                        .map(|i| tip.disc_at((i as f32 + shift).abs(), radius))
                        .sum();
                    assert!(
                        (sum - 2.0 * radius).abs() < 1.0e-4,
                        "{edge} {radius} {shift}: {sum}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_soft_tip_falls_smoothly_and_sums_evenly() {
        let tip = RoundTip::new(0.5, 1.0);
        assert!(!tip.is_hard());
        assert_eq!(tip.disc_at(0.0, 10.0), 1.0);
        assert_eq!(tip.disc_at(4.0, 10.0), 1.0);
        assert!((tip.disc_at(7.5, 10.0) - 0.5).abs() < 0.01);
        assert_eq!(tip.disc_at(10.5, 10.0), 0.0);
        let mut last = 1.0;
        for step in 0..=110 {
            let c = tip.disc_at(step as f32 * 0.1, 10.0);
            assert!(c <= last + 1.0e-6, "the profile never rises outward");
            last = c;
        }
        // However thin, a soft line's cross-section sums to the same
        // wherever it falls: the fade's own width (here 0.7).
        let thin = RoundTip::new(0.0, 1.0);
        for k in 0..10 {
            let shift = k as f32 * 0.1;
            let sum: f32 = (-4..=4)
                .map(|i| thin.disc_at((i as f32 + shift).abs(), 0.7))
                .sum();
            assert!((sum - 0.7).abs() < 1.0e-5, "{shift}: {sum}");
        }
    }

    #[test]
    fn values_out_of_range_are_held() {
        let tip = RoundTip::new(f32::NAN, -3.0);
        assert_eq!(tip.hardness(), 0.0);
        assert_eq!(tip.edge(), 0.0);
        assert!(RoundTip::new(7.0, 1.0).is_hard());
    }
}
