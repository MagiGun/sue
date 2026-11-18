//! Second-order spring physics, `F = -kx - cv`, with an instant-snap bypass.

use sue_rice::HardwareProfile;

/// Spring constants, read from rice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringParams {
    /// Stiffness `k`.
    pub stiffness: f32,
    /// Damping coefficient `c`.
    pub damping: f32,
    /// Mass `m` (must be positive).
    pub mass: f32,
}

impl SpringParams {
    /// The damping ratio `ζ = c / (2√(km))`: below 1 overshoots, 1 is
    /// critically damped, above 1 is sluggish.
    #[must_use]
    pub fn damping_ratio(&self) -> f32 {
        self.damping / (2.0 * (self.stiffness * self.mass).sqrt())
    }

    /// The least and most stiffness an animation runs with: softer
    /// crawls for seconds, stiffer than this moves faster than a frame
    /// shows.
    pub const STIFFNESS: (f32, f32) = (120.0, 1000.0);

    /// The least and most damping ratio an animation runs with. At the
    /// least (0.7) a spring overshoots its target by under 5 percent and
    /// comes to rest within a few swings; at the most (1.2) it still
    /// arrives without crawling.
    pub const DAMPING_RATIO: (f32, f32) = (0.7, 1.2);

    /// These constants made safe for the interface: any `motion.spring_*`
    /// values (from the ends of their sliders, or a hand-edited file)
    /// give a spring that comes to rest soon, without bouncing on and on
    /// (the interface flickered and never stopped drawing) or crawling.
    #[must_use]
    pub fn stable(self) -> Self {
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        let mass = finite(self.mass, 1.0).max(0.01);
        let (least, most) = Self::STIFFNESS;
        let stiffness = finite(self.stiffness, 260.0).clamp(least * mass, most * mass);
        let critical = 2.0 * (stiffness * mass).sqrt();
        let (low, high) = Self::DAMPING_RATIO;
        let damping = finite(self.damping, critical).clamp(low * critical, high * critical);
        Self {
            stiffness,
            damping,
            mass,
        }
    }
}

/// Whether a spring integrates or jumps straight to its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpringMode {
    /// Animate with spring physics.
    Physics,
    /// Jump to the target: 0 ms transitions.
    InstantSnap,
}

impl SpringMode {
    /// `InstantSnap` when the profile forbids spring physics (`LegacyLowSpec`)
    /// or the user enabled `motion.instant_transitions`; `Physics` otherwise.
    #[must_use]
    pub fn for_profile(profile: HardwareProfile, instant_transitions: bool) -> Self {
        if !instant_transitions && profile.budget().spring_physics {
            Self::Physics
        } else {
            Self::InstantSnap
        }
    }
}

/// A damped harmonic oscillator pulling `position` towards `target`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringDamper {
    params: SpringParams,
    mode: SpringMode,
    position: f32,
    velocity: f32,
    target: f32,
    /// It has come to its target since the target last changed (crossed
    /// it, or rests there).
    reached: bool,
}

impl SpringDamper {
    /// Longest integration sub-step, in seconds.
    pub const MAX_SUBSTEP: f32 = 1.0 / 240.0;
    /// Longest frame time simulated at once; longer stalls are clamped.
    pub const MAX_FRAME_TIME: f32 = 0.25;
    /// Distance and speed below which the spring counts as settled.
    pub const SETTLE_EPSILON: f32 = 1e-3;

    /// A spring at rest at `position`. Its constants are made
    /// [`stable`](SpringParams::stable).
    #[must_use]
    pub fn new(params: SpringParams, mode: SpringMode, position: f32) -> Self {
        Self {
            params: params.stable(),
            mode,
            position,
            velocity: 0.0,
            target: position,
            reached: true,
        }
    }

    /// Sets a new target. In `InstantSnap` mode the spring is there at once.
    pub fn set_target(&mut self, target: f32) {
        if target != self.target {
            self.reached = false;
        }
        self.target = target;
        if self.mode == SpringMode::InstantSnap {
            self.snap();
        }
    }

    /// Switches mode, e.g. when the hardware profile changes.
    pub fn set_mode(&mut self, mode: SpringMode) {
        self.mode = mode;
        if mode == SpringMode::InstantSnap {
            self.snap();
        }
    }

    /// The restoring force `F = -k (x - target) - c v`.
    #[must_use]
    pub fn force(&self) -> f32 {
        -self.params.stiffness * (self.position - self.target) - self.params.damping * self.velocity
    }

    /// Advances by `dt` seconds with semi-implicit Euler in sub-steps of at
    /// most [`MAX_SUBSTEP`](Self::MAX_SUBSTEP). Returns `true` while moving.
    pub fn step(&mut self, dt: f32) -> bool {
        if self.mode == SpringMode::InstantSnap || self.is_settled() {
            self.snap();
            return false;
        }
        let mass = self.params.mass.max(f32::EPSILON);
        let mut remaining = dt.clamp(0.0, Self::MAX_FRAME_TIME);
        let side = (self.position - self.target).signum();
        while remaining > 0.0 {
            let step = remaining.min(Self::MAX_SUBSTEP);
            self.velocity += self.force() / mass * step;
            self.position += self.velocity * step;
            remaining -= step;
        }
        if (self.position - self.target).signum() != side {
            self.reached = true;
        }
        if self.is_settled() {
            self.snap();
            return false;
        }
        true
    }

    /// `true` if position and velocity are within
    /// [`SETTLE_EPSILON`](Self::SETTLE_EPSILON) of rest at the target.
    /// Jumps to `value` and rests there: direct manipulation such as a drag
    /// follows the pointer exactly.
    pub fn snap_to(&mut self, value: f32) {
        self.target = value;
        self.snap();
    }

    /// Changes the spring constants (after a rice edit), made
    /// [`stable`](SpringParams::stable).
    pub fn set_params(&mut self, params: SpringParams) {
        self.params = params.stable();
    }

    /// `true` once it has come to its target since the target last
    /// changed: it crossed it (and swings back by a hair at most) or rests
    /// there. What switches on arriving (a page covering the Hub) asks
    /// this, not the position, so a swing around the target does not
    /// switch it back and forth.
    #[must_use]
    pub fn reached(&self) -> bool {
        self.reached || self.is_settled()
    }

    #[must_use]
    pub fn is_settled(&self) -> bool {
        (self.position - self.target).abs() < Self::SETTLE_EPSILON
            && self.velocity.abs() < Self::SETTLE_EPSILON
    }

    fn snap(&mut self) {
        self.position = self.target;
        self.velocity = 0.0;
        self.reached = true;
    }

    /// Current position.
    #[must_use]
    pub fn position(&self) -> f32 {
        self.position
    }

    /// Current velocity.
    #[must_use]
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    /// Current target.
    #[must_use]
    pub fn target(&self) -> f32 {
        self.target
    }

    /// Current mode.
    #[must_use]
    pub fn mode(&self) -> SpringMode {
        self.mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAMS: SpringParams = SpringParams {
        stiffness: 170.0,
        damping: 26.0,
        mass: 1.0,
    };

    fn frames_to_settle(spring: &mut SpringDamper, dt: f32) -> usize {
        (1..=10_000)
            .find(|_| !spring.step(dt))
            .expect("spring settles")
    }

    #[test]
    fn default_constants_are_near_critical_damping() {
        let ratio = PARAMS.damping_ratio();
        assert!((0.95..=1.05).contains(&ratio), "ζ = {ratio}");
    }

    #[test]
    fn settles_on_target_without_overshoot() {
        let mut spring = SpringDamper::new(PARAMS, SpringMode::Physics, 0.0);
        spring.set_target(1.0);
        let mut peak = 0.0_f32;
        let mut frames = 0;
        while spring.step(1.0 / 60.0) {
            peak = peak.max(spring.position());
            frames += 1;
        }
        assert_eq!(spring.position(), 1.0);
        assert_eq!(spring.velocity(), 0.0);
        assert!(peak < 1.0 + 1e-3, "overshoot to {peak}");
        assert!((10..120).contains(&frames), "settled after {frames} frames");
    }

    #[test]
    fn frame_rate_does_not_change_the_motion() {
        let mut at_60 = SpringDamper::new(PARAMS, SpringMode::Physics, 0.0);
        let mut at_240 = at_60;
        at_60.set_target(100.0);
        at_240.set_target(100.0);
        at_60.step(0.1);
        for _ in 0..24 {
            at_240.step(1.0 / 240.0);
        }
        assert!((at_60.position() - at_240.position()).abs() < 0.5);
    }

    #[test]
    fn instant_snap_bypasses_physics() {
        assert_eq!(
            SpringMode::for_profile(HardwareProfile::LegacyLowSpec, false),
            SpringMode::InstantSnap
        );
        assert_eq!(
            SpringMode::for_profile(HardwareProfile::Balanced, true),
            SpringMode::InstantSnap,
            "instant_transitions snaps on any profile"
        );
        assert_eq!(
            SpringMode::for_profile(HardwareProfile::HighRefresh, false),
            SpringMode::Physics
        );
        let mut spring = SpringDamper::new(PARAMS, SpringMode::InstantSnap, 0.0);
        spring.set_target(280.0);
        assert_eq!(spring.position(), 280.0);
        assert!(!spring.step(1.0 / 60.0));
    }

    #[test]
    fn long_stalls_are_clamped_and_force_follows_hookes_law() {
        let mut spring = SpringDamper::new(PARAMS, SpringMode::Physics, 0.0);
        spring.set_target(1.0);
        assert!((spring.force() - 170.0).abs() < 1e-4);
        spring.step(30.0);
        assert!(spring.position().is_finite());
        let mut switched = spring;
        switched.set_mode(SpringMode::InstantSnap);
        assert_eq!(switched.position(), switched.target());
        assert!(frames_to_settle(&mut spring, 1.0 / 120.0) < 1_000);
    }

    /// Every pair of `motion.spring_stiffness` and `motion.spring_damping`
    /// at the ends of their sliders (10 to 1000, 0 to 120), and the
    /// shipped pair: the spring overshoots by under 5 percent, comes to
    /// rest within 100 frames at 60 a second (the slowest, the softest
    /// spring damped most, takes about 93), and asks for no frame after.
    #[test]
    fn every_setting_at_the_ends_of_its_sliders_comes_to_rest_soon() {
        for stiffness in [10.0, 260.0, 1000.0] {
            for damping in [0.0, 32.0, 120.0] {
                let params = SpringParams {
                    stiffness,
                    damping,
                    mass: 1.0,
                };
                let mut spring = SpringDamper::new(params, SpringMode::Physics, 0.0);
                spring.set_target(1.0);
                let mut peak = 0.0_f32;
                let mut frames = 0;
                while spring.step(1.0 / 60.0) {
                    peak = peak.max(spring.position());
                    frames += 1;
                    assert!(frames < 100, "k {stiffness} c {damping}: still moving");
                }
                assert!(
                    peak < 1.05,
                    "k {stiffness} c {damping}: overshoot to {peak}"
                );
                assert_eq!(spring.position(), 1.0);
                assert!(spring.reached());
                assert!(!spring.step(1.0 / 60.0), "settled: no more frames");
            }
        }
        // What cannot be a spring at all still is one.
        let broken = SpringParams {
            stiffness: f32::NAN,
            damping: f32::INFINITY,
            mass: 0.0,
        }
        .stable();
        assert!(broken.stiffness.is_finite() && broken.damping.is_finite());
        assert!(broken.mass > 0.0);
    }

    /// A spring that swings past its target has reached it, and stays so
    /// while it swings back: what switches on arriving does not flicker.
    #[test]
    fn a_swing_back_past_the_target_still_counts_as_arrived() {
        let bouncy = SpringParams {
            stiffness: 1000.0,
            damping: 0.0,
            mass: 1.0,
        };
        let mut spring = SpringDamper::new(bouncy, SpringMode::Physics, 0.0);
        spring.set_target(1.0);
        assert!(!spring.reached());
        let mut arrived_at = None;
        for frame in 0..90 {
            spring.step(1.0 / 60.0);
            if spring.reached() {
                arrived_at.get_or_insert(frame);
            } else {
                assert!(arrived_at.is_none(), "it left again in frame {frame}");
            }
        }
        assert!(arrived_at.is_some());
        spring.set_target(0.0);
        assert!(!spring.reached(), "a new target is a new trip");
    }
}
