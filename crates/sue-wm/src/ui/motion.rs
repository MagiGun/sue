//! Motion: the one way anything in the interface moves.
//!
//! Every transition asks here, so all of them honour the motion settings:
//! with `motion.instant_transitions` on (or a hardware profile that
//! forbids spring physics) each helper answers with its target at once,
//! and `motion.animation_speed` scales every duration. A helper asks for
//! another frame only while its value is still moving, so a still
//! interface draws nothing.
//!
//! [`Theme::apply`](crate::Theme::apply) hands the settings to the egui
//! context; the helpers read them back from there, so a component needs
//! nothing but its `Ui`.

use egui::{Context, Id};
use sue_rice::{HardwareProfile, RiceSchema};

use crate::{SpringDamper, SpringMode, SpringParams};

/// The three durations of timed transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pace {
    /// Hover and press fills, the focus ring (90 ms).
    Fast,
    /// Menus, tooltips, toggles, the line under a tab (160 ms).
    Base,
    /// Dialogs, sheets, page changes (240 ms).
    Slow,
}

impl Pace {
    /// The duration at `motion.animation_speed` 1, in seconds.
    #[must_use]
    pub const fn seconds(self) -> f32 {
        match self {
            Self::Fast => 0.09,
            Self::Base => 0.16,
            Self::Slow => 0.24,
        }
    }
}

/// The motion settings, as the helpers of this module need them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    /// Snap everything: `motion.instant_transitions`, or a hardware
    /// profile that forbids spring physics.
    pub instant: bool,
    /// `motion.animation_speed`: 2 halves every duration.
    pub speed: f32,
    /// The riced spring, for what is dragged or thrown.
    pub spring: SpringParams,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            instant: false,
            speed: 1.0,
            spring: SpringParams {
                stiffness: 260.0,
                damping: 32.0,
                mass: 1.0,
            },
        }
    }
}

/// Where [`Motion::store`] keeps the settings in a context.
fn motion_id() -> Id {
    Id::unique("sue-motion")
}

impl Motion {
    /// Reads the motion settings from `rice`, falling back to the shipped
    /// defaults for anything missing. A `profile.hardware` that names a
    /// profile without spring physics snaps too; what `auto` resolves to
    /// is only known to the shell, which says so with
    /// [`for_profile`](Self::for_profile).
    #[must_use]
    pub fn from_rice(rice: &RiceSchema) -> Self {
        let fallback = Self::default();
        let number =
            |path: &str, fallback: f32| rice.get_f64(path).map_or(fallback, |value| value as f32);
        let named: Option<HardwareProfile> = rice
            .get_str("profile.hardware")
            .and_then(|name| name.parse().ok());
        Self {
            instant: rice
                .get_bool("motion.instant_transitions")
                .unwrap_or(fallback.instant),
            speed: number("motion.animation_speed", fallback.speed),
            spring: SpringParams {
                stiffness: number("motion.spring_stiffness", fallback.spring.stiffness),
                damping: number("motion.spring_damping", fallback.spring.damping),
                mass: 1.0,
            },
        }
        .for_named(named)
    }

    /// [`for_profile`](Self::for_profile), when the rice names one.
    fn for_named(self, profile: Option<HardwareProfile>) -> Self {
        profile.map_or(self, |profile| self.for_profile(profile))
    }

    /// These settings on hardware of `profile`: a profile that forbids
    /// spring physics snaps everything.
    #[must_use]
    pub fn for_profile(mut self, profile: HardwareProfile) -> Self {
        self.instant |= SpringMode::for_profile(profile, self.instant) == SpringMode::InstantSnap;
        self
    }

    /// How springs behave under these settings.
    #[must_use]
    pub fn mode(&self) -> SpringMode {
        if self.instant {
            SpringMode::InstantSnap
        } else {
            SpringMode::Physics
        }
    }

    /// How long a transition of `pace` takes, in seconds: 0 when
    /// transitions are instant.
    #[must_use]
    pub fn seconds(&self, pace: Pace) -> f32 {
        if self.instant {
            0.0
        } else {
            pace.seconds() / self.speed.max(0.01)
        }
    }

    /// Hands these settings to `ctx`, for [`Motion::of`].
    pub fn store(&self, ctx: &Context) {
        ctx.data_mut(|data| data.insert_temp(motion_id(), *self));
    }

    /// The settings `ctx` was handed, or the shipped defaults.
    #[must_use]
    pub fn of(ctx: &Context) -> Self {
        ctx.data(|data| data.get_temp(motion_id()))
            .unwrap_or_default()
    }
}

/// The easing of every timed transition: fast at first and slowing into
/// place on the way in (ease-out cubic), slow at first on the way out
/// (ease-in), which is one curve read in both directions.
#[must_use]
pub fn ease(t: f32) -> f32 {
    let rest = 1.0 - t.clamp(0.0, 1.0);
    1.0 - rest * rest * rest
}

/// A value that follows `on` from 0 to 1 and back in the time of `pace`,
/// eased. The first time an `id` is seen it is at its target already.
pub fn timed(ctx: &Context, id: Id, on: bool, pace: Pace) -> f32 {
    // egui moves the value at a steady rate and asks for another frame
    // while it is between its ends; a time of 0 lands at once.
    let seconds = Motion::of(ctx).seconds(pace);
    ease(ctx.animate_bool_with_time(id, on, seconds))
}

/// The opacity of something that fades in while `shown` and out after.
pub fn fade(ctx: &Context, id: Id, shown: bool) -> f32 {
    timed(ctx, id, shown, Pace::Base)
}

/// How far from its place something still is that slides in over
/// `distance` while `shown`, and back out after: `distance` when hidden,
/// 0 in place.
pub fn slide(ctx: &Context, id: Id, shown: bool, distance: f32) -> f32 {
    (1.0 - timed(ctx, id, shown, Pace::Slow)) * distance
}

/// How far a control's fill is on its way to the hovered (or pressed, or
/// focused) one.
pub fn hover_t(ctx: &Context, id: Id, hovered: bool) -> f32 {
    timed(ctx, id, hovered, Pace::Fast)
}

/// A spring between frames: where it is, and the frame that last moved
/// it.
#[derive(Clone, Copy)]
struct Sprung {
    spring: SpringDamper,
    frame: u64,
}

/// A value that follows `target` on the riced spring, for what is dragged
/// or thrown. The first time an `id` is seen it rests at its target.
pub fn spring(ctx: &Context, id: Id, target: f32) -> f32 {
    let motion = Motion::of(ctx);
    let frame = ctx.cumulative_frame_nr();
    let dt = ctx.input(|input| input.stable_dt) * motion.speed.max(0.01);
    let (position, moving) = ctx.data_mut(|data| {
        let state = data.get_temp_mut_or_insert_with(id, || Sprung {
            spring: SpringDamper::new(motion.spring, motion.mode(), target),
            frame,
        });
        state.spring.set_params(motion.spring);
        state.spring.set_mode(motion.mode());
        state.spring.set_target(target);
        // Asked twice in one frame, it moves once.
        let moving = if state.frame == frame {
            !state.spring.is_settled()
        } else {
            state.frame = frame;
            state.spring.step(dt)
        };
        (state.spring.position(), moving)
    });
    if moving {
        ctx.request_repaint();
    }
    position
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame of `ctx`, `dt` seconds after the last, running `work`.
    fn frame<R>(
        ctx: &Context,
        time: &mut f64,
        dt: f32,
        work: impl FnOnce(&Context) -> R,
    ) -> (R, bool) {
        *time += f64::from(dt);
        let input = egui::RawInput {
            time: Some(*time),
            predicted_dt: dt,
            ..Default::default()
        };
        let mut result = None;
        let mut work = Some(work);
        let mut output = ctx.run_ui(input, |ui| {
            if let Some(work) = work.take() {
                result = Some(work(ui.ctx()));
            }
        });
        output.textures_delta.clear();
        let again = output
            .viewport_output
            .values()
            .any(|viewport| viewport.repaint_delay < std::time::Duration::from_secs(1));
        (result.expect("the frame ran"), again)
    }

    /// A context past the frames egui asks for by itself when it starts.
    fn settled(time: &mut f64) -> Context {
        let ctx = Context::default();
        let quiet = (0..8).any(|_| !frame(&ctx, time, 0.016, |_| ()).1);
        assert!(quiet, "an idle context stops asking for frames");
        ctx
    }

    #[test]
    fn the_settings_come_from_the_rice() {
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        let motion = Motion::from_rice(&rice);
        assert_eq!(motion, Motion::default());
        assert_eq!(motion.seconds(Pace::Fast), 0.09);
        assert_eq!(motion.seconds(Pace::Base), 0.16);
        assert_eq!(motion.seconds(Pace::Slow), 0.24);
        assert_eq!(motion.mode(), SpringMode::Physics);
        rice.set("motion.animation_speed", sue_rice::RiceValue::Float(2.0))
            .unwrap();
        let fast = Motion::from_rice(&rice);
        assert_eq!(fast.seconds(Pace::Base), 0.08);
        rice.set(
            "motion.instant_transitions",
            sue_rice::RiceValue::Bool(true),
        )
        .unwrap();
        let instant = Motion::from_rice(&rice);
        assert!(instant.instant);
        assert_eq!(instant.seconds(Pace::Slow), 0.0);
        assert_eq!(instant.mode(), SpringMode::InstantSnap);
        // A profile without spring physics snaps as well, named in the
        // rice or resolved by the shell.
        rice.set(
            "motion.instant_transitions",
            sue_rice::RiceValue::Bool(false),
        )
        .unwrap();
        assert!(!Motion::from_rice(&rice).instant);
        rice.set(
            "profile.hardware",
            sue_rice::RiceValue::Text("legacy-low-spec".to_owned()),
        )
        .unwrap();
        assert!(Motion::from_rice(&rice).instant);
        assert!(
            Motion::default()
                .for_profile(HardwareProfile::LegacyLowSpec)
                .instant
        );
        assert!(
            !Motion::default()
                .for_profile(HardwareProfile::Balanced)
                .instant
        );
        // An empty rice reads as the shipped defaults.
        assert_eq!(Motion::from_rice(&RiceSchema::new()), Motion::default());
    }

    #[test]
    fn a_context_keeps_the_settings_it_was_handed() {
        let ctx = Context::default();
        assert_eq!(Motion::of(&ctx), Motion::default());
        let instant = Motion {
            instant: true,
            ..Motion::default()
        };
        instant.store(&ctx);
        assert_eq!(Motion::of(&ctx), instant);
    }

    #[test]
    fn easing_slows_into_place() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert_eq!(ease(-3.0), 0.0);
        assert_eq!(ease(7.0), 1.0);
        // Fast at first: half the time is most of the way.
        assert!((ease(0.5) - 0.875).abs() < 1e-6);
        let steps: Vec<f32> = (0..=10).map(|step| ease(step as f32 / 10.0)).collect();
        assert!(steps.is_sorted());
    }

    #[test]
    fn a_timed_value_moves_then_rests_and_asks_for_frames_only_while_moving() {
        let mut time = 0.0;
        let ctx = settled(&mut time);
        Motion::default().store(&ctx);
        let id = Id::unique("fade");
        // First seen, it is where it belongs and nothing moves.
        let (start, again) = frame(&ctx, &mut time, 0.016, |ctx| fade(ctx, id, false));
        assert_eq!(start, 0.0);
        assert!(!again, "nothing moves yet");
        let mut values = Vec::new();
        let mut frames_asked = 0;
        for _ in 0..30 {
            let (value, again) = frame(&ctx, &mut time, 0.016, |ctx| fade(ctx, id, true));
            values.push(value);
            if value < 1.0 {
                assert!(again, "still moving at {value}");
                frames_asked += 1;
            }
        }
        assert!(values[0] > 0.0 && values[0] < 1.0, "{values:?}");
        assert!(values.is_sorted(), "{values:?}");
        assert_eq!(values.last(), Some(&1.0));
        // 160 ms at 16 ms a frame: ten frames, give or take one.
        assert!((9..=11).contains(&frames_asked), "{frames_asked}");
        // Arrived: no more frames.
        let (_, again) = frame(&ctx, &mut time, 0.016, |ctx| fade(ctx, id, true));
        assert!(!again, "at rest");
        // And back out, slowly at first.
        let (leaving, again) = frame(&ctx, &mut time, 0.016, |ctx| fade(ctx, id, false));
        assert!(leaving > 0.9 && leaving < 1.0, "{leaving}");
        assert!(again);
    }

    #[test]
    fn instant_transitions_land_at_once() {
        let mut time = 0.0;
        let ctx = settled(&mut time);
        Motion {
            instant: true,
            ..Motion::default()
        }
        .store(&ctx);
        let ids = [
            Id::unique("a"),
            Id::unique("b"),
            Id::unique("c"),
            Id::unique("d"),
        ];
        let all = |ctx: &Context, on: bool| {
            [
                fade(ctx, ids[0], on),
                hover_t(ctx, ids[1], on),
                slide(ctx, ids[2], on, 40.0),
                spring(ctx, ids[3], if on { 100.0 } else { 0.0 }),
            ]
        };
        let (off, _) = frame(&ctx, &mut time, 0.016, |ctx| all(ctx, false));
        assert_eq!(off, [0.0, 0.0, 40.0, 0.0]);
        let (on, again) = frame(&ctx, &mut time, 0.016, |ctx| all(ctx, true));
        assert_eq!(on, [1.0, 1.0, 0.0, 100.0]);
        assert!(!again, "nothing is on its way");
        let (off, again) = frame(&ctx, &mut time, 0.016, |ctx| all(ctx, false));
        assert_eq!(off, [0.0, 0.0, 40.0, 0.0]);
        assert!(!again);
    }

    #[test]
    fn the_animation_speed_scales_every_duration() {
        let frames_to_arrive = |speed: f32| {
            let ctx = Context::default();
            Motion {
                speed,
                ..Motion::default()
            }
            .store(&ctx);
            let id = Id::unique("slide");
            let mut time = 0.0;
            let _ = frame(&ctx, &mut time, 0.008, |ctx| slide(ctx, id, false, 10.0));
            (1..200)
                .find(|_| frame(&ctx, &mut time, 0.008, |ctx| slide(ctx, id, true, 10.0)).0 == 0.0)
                .expect("it arrives")
        };
        let (normal, double) = (frames_to_arrive(1.0), frames_to_arrive(2.0));
        // 240 ms at 8 ms a frame is 30 frames; twice the speed, 15.
        assert!((29..=31).contains(&normal), "{normal}");
        assert!((14..=16).contains(&double), "{double}");
    }

    #[test]
    fn a_spring_follows_its_target_and_comes_to_rest() {
        let mut time = 0.0;
        let ctx = settled(&mut time);
        Motion::default().store(&ctx);
        let id = Id::unique("sheet");
        let (rest, again) = frame(&ctx, &mut time, 0.008, |ctx| spring(ctx, id, 0.0));
        assert_eq!(rest, 0.0);
        assert!(!again);
        let (first, again) = frame(&ctx, &mut time, 0.008, |ctx| {
            // Asked twice in a frame, it moves once.
            let once = spring(ctx, id, 100.0);
            assert_eq!(spring(ctx, id, 100.0), once);
            once
        });
        assert!(first > 0.0 && first < 100.0, "{first}");
        assert!(again, "on its way");
        let mut last = first;
        let mut arrived = None;
        for step in 0..400 {
            let (value, again) = frame(&ctx, &mut time, 0.008, |ctx| spring(ctx, id, 100.0));
            assert!(value >= last - 0.5, "no way back: {last} then {value}");
            last = value;
            if !again {
                arrived = Some(step);
                break;
            }
        }
        assert!(arrived.is_some(), "it comes to rest");
        assert_eq!(last, 100.0);
    }
}
