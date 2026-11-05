//! A pen's position and pressure on Linux, from Wayland's tablet protocol
//! (`zwp_tablet_v2`, through the `octotablet` crate).
//!
//! `winit` does not speak that protocol, so a compositor hands a pen to Sue
//! as a plain mouse: it moves and clicks, with no pressure. With the protocol
//! bound, a tablet's pen reaches Sue through it alone, so this module also
//! turns the pen's moves, touches and lifts into the pointer events egui
//! expects, and publishes the pen's pressure to [`crate::stylus`], the way
//! the WinTab path does on Windows.
//!
//! The host calls [`Pen::poll`] whenever its loop wakes, and drains the
//! pointer events into the next frame with [`Pen::take_events`].

use std::sync::Arc;
use std::time::Duration;

use octotablet::Manager;
use octotablet::builder::Builder;
use octotablet::events::{Event, ToolEvent};
use winit::window::Window;

use crate::stylus::{self, PenSource};

/// One thing the pen did, in the order egui should see it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sample {
    /// The pen moved to `at` (logical pixels from the window's top left),
    /// pressing with `pressure` (0 to 1) when the tablet reports one.
    Pose { at: [f32; 2], pressure: Option<f32> },
    /// The nib touched the surface.
    Down,
    /// The nib lifted off the surface.
    Up,
    /// The pen left the tablet's range or the window.
    Out,
}

/// What the pen is doing and the egui events that tell it. No Wayland in
/// here, so it can be tested.
#[derive(Debug, Default)]
struct PenState {
    /// Where the pen is, in egui points, while it is in range.
    at: Option<egui::Pos2>,
    /// The latest pressure, 0 to 1.
    pressure: f32,
    /// Whether the nib touches.
    touching: bool,
    /// Pointer events for the next frame.
    events: Vec<egui::Event>,
    /// A pressure change for [`crate::stylus`]: `Some(None)` is a lift.
    published: Option<Option<f32>>,
}

impl PenState {
    fn apply(&mut self, sample: Sample, to_points: f32) {
        match sample {
            Sample::Pose { at, pressure } => {
                let at = egui::pos2(at[0] * to_points, at[1] * to_points);
                self.at = Some(at);
                self.pressure = pressure.unwrap_or(1.0).clamp(0.0, 1.0);
                self.events.push(egui::Event::PointerMoved(at));
                if self.touching {
                    self.published = Some(Some(self.pressure));
                }
            }
            Sample::Down => {
                if !self.touching {
                    self.touching = true;
                    if let Some(at) = self.at {
                        self.button(at, true);
                    }
                    self.published = Some(Some(self.pressure));
                }
            }
            Sample::Up => self.lift(),
            Sample::Out => {
                self.lift();
                self.at = None;
                self.events.push(egui::Event::PointerGone);
            }
        }
    }

    fn lift(&mut self) {
        if self.touching {
            self.touching = false;
            if let Some(at) = self.at {
                self.button(at, false);
            }
            self.published = Some(None);
        }
    }

    fn button(&mut self, pos: egui::Pos2, pressed: bool) {
        self.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
}

/// Moves the events of one tablet frame, in the order egui needs: where the
/// pen is first, then the touch or lift, then leaving.
fn flush(
    samples: &mut Vec<Sample>,
    pose: &mut Option<Sample>,
    touch: &mut Option<Sample>,
    out: &mut bool,
) {
    samples.extend(pose.take());
    samples.extend(touch.take());
    if std::mem::take(out) {
        samples.push(Sample::Out);
    }
}

/// The compositor's tablet protocol, bound on Sue's window.
pub(crate) struct Pen {
    manager: Manager,
    state: PenState,
}

impl Pen {
    /// The pen, when the compositor offers a tablet protocol (it does not on
    /// X11 or without a display).
    pub(crate) fn open(window: &Arc<Window>) -> Option<Self> {
        let manager = Builder::default().build_shared(window).ok()?;
        Some(Self {
            manager,
            state: PenState::default(),
        })
    }

    /// Reads what the tablet sent since the last call. `to_points` turns the
    /// protocol's logical pixels into egui points. True while pointer events
    /// wait for a frame.
    pub(crate) fn poll(&mut self, to_points: f32) -> bool {
        let mut samples = Vec::new();
        {
            let Ok(events) = self.manager.pump() else {
                return !self.state.events.is_empty();
            };
            let mut pose = None;
            let mut touch = None;
            let mut out = false;
            for event in events {
                let Event::Tool { event, .. } = event else {
                    continue;
                };
                match event {
                    ToolEvent::Pose(p) => {
                        pose = Some(Sample::Pose {
                            at: p.position,
                            pressure: p.pressure.get(),
                        });
                    }
                    ToolEvent::Down => touch = Some(Sample::Down),
                    ToolEvent::Up => touch = Some(Sample::Up),
                    ToolEvent::Out | ToolEvent::Removed => out = true,
                    ToolEvent::Frame(_) => flush(&mut samples, &mut pose, &mut touch, &mut out),
                    _ => {}
                }
            }
            flush(&mut samples, &mut pose, &mut touch, &mut out);
        }
        for sample in samples {
            self.state.apply(sample, to_points);
        }
        if let Some(pressure) = self.state.published.take() {
            stylus::set(PenSource::Wayland, pressure);
        }
        !self.state.events.is_empty()
    }

    /// Moves the pen's pointer events into the frame's input, with the
    /// modifier keys held now.
    pub(crate) fn take_events(&mut self, modifiers: egui::Modifiers, into: &mut Vec<egui::Event>) {
        for mut event in self.state.events.drain(..) {
            if let egui::Event::PointerButton {
                modifiers: held, ..
            } = &mut event
            {
                *held = modifiers;
            }
            into.push(event);
        }
    }

    /// How long the loop may sleep before it looks at the tablet again. The
    /// compositor's messages wake the loop by themselves; this is the
    /// backstop, short while a pen is in range.
    pub(crate) fn wake_after(&self) -> Duration {
        if self.state.at.is_some() {
            Duration::from_millis(4)
        } else {
            Duration::from_millis(250)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(x: f32, y: f32, pressure: Option<f32>) -> Sample {
        Sample::Pose {
            at: [x, y],
            pressure,
        }
    }

    #[test]
    fn a_stroke_moves_presses_and_lifts_with_the_pens_pressure() {
        let mut pen = PenState::default();
        pen.apply(pose(10.0, 20.0, Some(0.0)), 2.0);
        assert_eq!(pen.published, None, "hovering publishes no pressure");
        pen.apply(pose(10.0, 20.0, Some(0.5)), 2.0);
        pen.apply(Sample::Down, 2.0);
        assert_eq!(pen.published.take(), Some(Some(0.5)));
        pen.apply(pose(12.0, 20.0, Some(0.8)), 2.0);
        assert_eq!(pen.published.take(), Some(Some(0.8)));
        pen.apply(Sample::Up, 2.0);
        assert_eq!(pen.published.take(), Some(None));
        assert_eq!(
            pen.events[0],
            egui::Event::PointerMoved(egui::pos2(20.0, 40.0)),
            "points, not logical pixels"
        );
        let pressed: Vec<bool> = pen
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::PointerButton { pressed, .. } => Some(*pressed),
                _ => None,
            })
            .collect();
        assert_eq!(pressed, [true, false]);
    }

    #[test]
    fn leaving_range_mid_stroke_releases_the_pen() {
        let mut pen = PenState::default();
        pen.apply(pose(1.0, 1.0, Some(0.3)), 1.0);
        pen.apply(Sample::Down, 1.0);
        pen.apply(Sample::Out, 1.0);
        assert_eq!(pen.published.take(), Some(None));
        assert!(matches!(pen.events.last(), Some(egui::Event::PointerGone)));
        assert!(pen.at.is_none());
    }

    #[test]
    fn a_tablet_without_pressure_presses_fully() {
        let mut pen = PenState::default();
        pen.apply(pose(5.0, 5.0, None), 1.0);
        pen.apply(Sample::Down, 1.0);
        assert_eq!(pen.published.take(), Some(Some(1.0)));
    }

    #[test]
    fn a_frame_orders_where_then_touch_then_leave() {
        let mut samples = Vec::new();
        let mut at = Some(pose(1.0, 2.0, Some(0.4)));
        let mut touch = Some(Sample::Down);
        let mut out = true;
        flush(&mut samples, &mut at, &mut touch, &mut out);
        assert_eq!(
            samples,
            [pose(1.0, 2.0, Some(0.4)), Sample::Down, Sample::Out]
        );
        assert!(at.is_none() && touch.is_none() && !out);
    }
}
