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
}

impl SpringDamper {
    /// Longest integration sub-step, in seconds.
    pub const MAX_SUBSTEP: f32 = 1.0 / 240.0;
    /// Longest frame time simulated at once; longer stalls are clamped.
    pub const MAX_FRAME_TIME: f32 = 0.25;
    /// Distance and speed below which the spring counts as settled.
    pub const SETTLE_EPSILON: f32 = 1e-3;

    /// A spring at rest at `position`.
    #[must_use]
    pub fn new(params: SpringParams, mode: SpringMode, position: f32) -> Self {
        Self {
            params,
            mode,
            position,
            velocity: 0.0,
            target: position,
        }
    }

    /// Sets a new target. In `InstantSnap` mode the spring is there at once.
    pub fn set_target(&mut self, target: f32) {
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
        while remaining > 0.0 {
            let step = remaining.min(Self::MAX_SUBSTEP);
            self.velocity += self.force() / mass * step;
            self.position += self.velocity * step;
            remaining -= step;
        }
        if self.is_settled() {
            self.snap();
            return false;
        }
        true
    }

    /// `true` if position and velocity are within
    /// [`SETTLE_EPSILON`](Self::SETTLE_EPSILON) of rest at the target.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        (self.position - self.target).abs() < Self::SETTLE_EPSILON
            && self.velocity.abs() < Self::SETTLE_EPSILON
    }

    fn snap(&mut self) {
        self.position = self.target;
        self.velocity = 0.0;
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
}
