//! The dirty-frame scheduler: 0 FPS when nothing changes.

use sue_rice::HardwareProfile;

use crate::Rect;

/// Whether the host should render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderDemand {
    /// Nothing changed: block in the event loop, draw nothing.
    Sleep,
    /// Render at up to `target_hz`.
    ActiveFrame {
        /// Frame-rate target from the hardware profile.
        target_hz: u32,
    },
}

impl RenderDemand {
    /// Merges two demands: any active frame wins, at the higher rate.
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Sleep, demand) | (demand, Self::Sleep) => demand,
            (Self::ActiveFrame { target_hz: a }, Self::ActiveFrame { target_hz: b }) => {
                Self::ActiveFrame {
                    target_hz: a.max(b),
                }
            }
        }
    }

    /// `true` for [`RenderDemand::Sleep`].
    #[must_use]
    pub fn is_sleep(self) -> bool {
        self == Self::Sleep
    }
}

/// What must be redrawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Damage {
    /// Everything.
    Full,
    /// Only this region.
    Region(Rect),
}

/// Permission to render one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameTicket {
    /// Region to redraw; `None` while only an animation is running and the
    /// animated layers redraw themselves.
    pub damage: Option<Damage>,
    /// Frame-rate target.
    pub target_hz: u32,
}

/// Collects damage and animation state, and decides between sleeping and
/// rendering at the profile's frame rate.
#[derive(Clone, Debug)]
pub struct FrameScheduler {
    profile: HardwareProfile,
    display_hz: u32,
    damage: Option<Damage>,
    animating: bool,
    frames: u64,
    sleeps: u64,
}

impl FrameScheduler {
    /// A scheduler for `profile` on a display refreshing at `display_hz`.
    #[must_use]
    pub fn new(profile: HardwareProfile, display_hz: u32) -> Self {
        Self {
            profile,
            display_hz,
            damage: None,
            animating: false,
            frames: 0,
            sleeps: 0,
        }
    }

    /// Changes the hardware profile.
    pub fn set_profile(&mut self, profile: HardwareProfile) {
        self.profile = profile;
    }

    /// Marks `rect` as needing a redraw. Regions merge by union.
    pub fn invalidate(&mut self, rect: Rect) {
        self.damage = Some(match self.damage {
            None => Damage::Region(rect),
            Some(Damage::Region(existing)) => Damage::Region(existing.union(&rect)),
            Some(Damage::Full) => Damage::Full,
        });
    }

    /// Marks everything as needing a redraw.
    pub fn invalidate_all(&mut self) {
        self.damage = Some(Damage::Full);
    }

    /// Declares whether an animation is running.
    pub fn set_animating(&mut self, animating: bool) {
        self.animating = animating;
    }

    /// The current demand.
    #[must_use]
    pub fn demand(&self) -> RenderDemand {
        if self.damage.is_none() && !self.animating {
            RenderDemand::Sleep
        } else {
            RenderDemand::ActiveFrame {
                target_hz: self.profile.target_fps(self.display_hz),
            }
        }
    }

    /// Starts a frame if one is needed, consuming the damage. Returns `None`
    /// (and counts a sleep) when nothing needs drawing.
    pub fn begin_frame(&mut self) -> Option<FrameTicket> {
        match self.demand() {
            RenderDemand::Sleep => {
                self.sleeps += 1;
                None
            }
            RenderDemand::ActiveFrame { target_hz } => {
                self.frames += 1;
                Some(FrameTicket {
                    damage: self.damage.take(),
                    target_hz,
                })
            }
        }
    }

    /// `true` while an animation keeps frames coming.
    #[must_use]
    pub fn animating(&self) -> bool {
        self.animating
    }

    /// Frames rendered so far.
    #[must_use]
    pub fn frames_rendered(&self) -> u64 {
        self.frames
    }

    /// Frames skipped because nothing changed.
    #[must_use]
    pub fn sleeps(&self) -> u64 {
        self.sleeps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_scenes_sleep() {
        let mut scheduler = FrameScheduler::new(HardwareProfile::HighRefresh, 240);
        assert_eq!(scheduler.demand(), RenderDemand::Sleep);
        assert!(scheduler.begin_frame().is_none());
        assert_eq!((scheduler.frames_rendered(), scheduler.sleeps()), (0, 1));
    }

    #[test]
    fn damage_merges_and_is_consumed_by_one_frame() {
        let mut scheduler = FrameScheduler::new(HardwareProfile::Balanced, 144);
        scheduler.invalidate(Rect::new(0.0, 0.0, 10.0, 10.0));
        scheduler.invalidate(Rect::new(20.0, 0.0, 10.0, 10.0));
        assert_eq!(
            scheduler.demand(),
            RenderDemand::ActiveFrame { target_hz: 60 }
        );
        let ticket = scheduler.begin_frame().unwrap();
        assert_eq!(
            ticket.damage,
            Some(Damage::Region(Rect::new(0.0, 0.0, 30.0, 10.0)))
        );
        assert!(scheduler.begin_frame().is_none());
        scheduler.invalidate_all();
        scheduler.invalidate(Rect::new(1.0, 1.0, 1.0, 1.0));
        assert_eq!(scheduler.begin_frame().unwrap().damage, Some(Damage::Full));
    }

    #[test]
    fn animations_keep_frames_coming_at_the_profile_rate() {
        let mut scheduler = FrameScheduler::new(HardwareProfile::HighRefresh, 240);
        scheduler.set_animating(true);
        for _ in 0..3 {
            assert_eq!(scheduler.begin_frame().unwrap().target_hz, 240);
        }
        scheduler.set_profile(HardwareProfile::MobileBatterySaver);
        assert_eq!(
            scheduler.demand(),
            RenderDemand::ActiveFrame { target_hz: 30 }
        );
        scheduler.set_animating(false);
        assert!(scheduler.demand().is_sleep());
    }

    #[test]
    fn demands_combine() {
        let fast = RenderDemand::ActiveFrame { target_hz: 120 };
        let slow = RenderDemand::ActiveFrame { target_hz: 30 };
        assert_eq!(RenderDemand::Sleep.combine(slow), slow);
        assert_eq!(slow.combine(fast), fast);
        assert_eq!(
            RenderDemand::Sleep.combine(RenderDemand::Sleep),
            RenderDemand::Sleep
        );
    }
}
