//! Pen tablets on Linux and the BSDs: the plain logic, kept apart from the
//! Wayland and X11 calls in `crate::wayland_pen` and `crate::xinput_pen`
//! so it runs and is tested everywhere.
//!
//! Under Wayland Sue reads the pen itself through the tablet-v2 protocol
//! (winit 0.30 does not), and [`PenTrack`] turns its frames into pointer
//! events, so the pen draws even where the compositor does not turn it
//! into a mouse (under niri it did nothing). A compositor without
//! tablet-v2 leaves the pen to XWayland when it is there
//! ([`display_server`]): XWayland hands it to X11 programs as an XInput2
//! device that also moves the pointer, and XInput2 adds its pressure, tilt
//! and eraser.

/// The display server Sue's windows open on in a Linux or BSD session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayServer {
    Wayland,
    X11,
}

/// The environment variable that picks the display server by hand:
/// `wayland` or `x11`.
pub(crate) const CHOICE_VARIABLE: &str = "SUE_DISPLAY_SERVER";

/// Where Sue's windows open: `wayland` and `x11` say whether a Wayland
/// compositor and an X server (XWayland, under Wayland) are reachable, `pen`
/// whether a pen tablet is connected, `tablet_protocol` asks the compositor
/// whether it offers tablet-v2 (only when that decides it), `choice` is the
/// value of [`CHOICE_VARIABLE`]. `None` leaves it to winit, which prefers
/// Wayland.
pub(crate) fn display_server(
    wayland: bool,
    x11: bool,
    pen: bool,
    tablet_protocol: impl FnOnce() -> bool,
    choice: Option<&str>,
) -> Option<DisplayServer> {
    match choice
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("wayland") if wayland => return Some(DisplayServer::Wayland),
        Some("x11" | "xwayland") if x11 => return Some(DisplayServer::X11),
        _ => {}
    }
    // Wayland reads the pen itself where the compositor speaks tablet-v2;
    // elsewhere XWayland passes it on.
    (wayland && x11 && pen && !tablet_protocol()).then_some(DisplayServer::X11)
}

/// What one tablet-v2 frame said about a tool: the events between two
/// `frame` events, which happened together.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ToolFrame {
    /// It came near over this surface (a key the caller chooses).
    pub(crate) enter: Option<usize>,
    /// It went out of range.
    pub(crate) leave: bool,
    /// Its tip touched (`true`) or lifted.
    pub(crate) down: Option<bool>,
    /// Where it is, in the surface's logical pixels.
    pub(crate) position: Option<[f32; 2]>,
    /// How hard it presses, 0 to 65535.
    pub(crate) pressure: Option<u32>,
    /// How far it leans, in degrees, to the right and toward the person.
    pub(crate) tilt: Option<[f64; 2]>,
    /// Barrel buttons (Linux key codes) pressed or released.
    pub(crate) buttons: Vec<(u32, bool)>,
}

/// A pointer event of the pen's, for the surface it is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PenPointer {
    /// It moved to (or came near at) this point, in logical pixels.
    Move([f32; 2]),
    /// A button went down or up at this point: the tip is the primary.
    Button {
        button: egui::PointerButton,
        pressed: bool,
        at: [f32; 2],
    },
    /// It went away.
    Gone,
}

impl PenPointer {
    /// The egui event, `scale` being egui points per logical pixel.
    pub(crate) fn to_egui(self, scale: f32, modifiers: egui::Modifiers) -> egui::Event {
        let pos = |[x, y]: [f32; 2]| egui::pos2(x * scale, y * scale);
        match self {
            Self::Move(at) => egui::Event::PointerMoved(pos(at)),
            Self::Button {
                button,
                pressed,
                at,
            } => egui::Event::PointerButton {
                pos: pos(at),
                button,
                pressed,
                modifiers,
            },
            Self::Gone => egui::Event::PointerGone,
        }
    }
}

/// The pointer button for a pen's barrel button: the lower one
/// (`BTN_STYLUS`) is the middle button and the upper one (`BTN_STYLUS2`)
/// the secondary, as X11's tablet drivers have it.
pub(crate) fn barrel_button(code: u32) -> Option<egui::PointerButton> {
    match code {
        0x14b => Some(egui::PointerButton::Middle),
        0x14c => Some(egui::PointerButton::Secondary),
        0x149 => Some(egui::PointerButton::Extra1),
        _ => None,
    }
}

/// What the pen read at one of its pointer events: the reading a studio
/// pairs with that event (see `crate::stylus::Sample`), so each point of a
/// stroke has its own pressure and tilt, not just a frame's last ones.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Reading {
    /// Its pressure, 0 to 1, while the tip touches; on the tip's release,
    /// the pressure it had just before.
    pub(crate) pressure: Option<f32>,
    /// Its lean, -1 to 1 each way, once it has said.
    pub(crate) tilt: Option<[f32; 2]>,
    /// The eraser end (or an eraser tool) is the one in use.
    pub(crate) eraser: bool,
}

/// A pointer event of a pen's, with the surface it is for and what the
/// pen read then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PenEvent {
    pub(crate) surface: usize,
    pub(crate) pointer: PenPointer,
    pub(crate) reading: Reading,
}

/// A pen as its tablet-v2 frames left it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PenTrack {
    /// The surface it is over, while it is near.
    pub(crate) surface: Option<usize>,
    position: [f32; 2],
    /// Whether its tip touches.
    pub(crate) down: bool,
    /// Its pressure, 0 to 1.
    pub(crate) pressure: f32,
    /// Its lean, -1 to 1 each way, once it has said.
    pub(crate) tilt: Option<[f32; 2]>,
    /// It is an eraser (the pen's eraser end).
    pub(crate) eraser: bool,
    /// Its barrel buttons held, as pointer buttons.
    held: Vec<egui::PointerButton>,
}

impl PenTrack {
    /// Takes one frame (every event of one tool between two `frame`
    /// events, which happened together), adding the pointer events it
    /// makes to `out`, each with the surface it is for and the pen's
    /// reading at that event: coming near, moving, the tip and buttons
    /// going down, then up, then going away. A frame that only changes
    /// the pressure or the lean of a touching tip moves it where it is,
    /// so pressing harder in place still reaches the stroke.
    pub(crate) fn apply(&mut self, frame: &ToolFrame, out: &mut Vec<PenEvent>) {
        if let Some(surface) = frame.enter {
            self.surface = Some(surface);
        }
        // A frame that lifts the tip keeps the pressure it had for the
        // stroke's last point; what the frame says (often 0) comes after.
        let lifts = self.down && (frame.down == Some(false) || frame.leave);
        if !lifts {
            self.take_pressure(frame);
        }
        if let Some([x, y]) = frame.tilt {
            self.tilt = Some([
                (x / 90.0).clamp(-1.0, 1.0) as f32,
                (y / 90.0).clamp(-1.0, 1.0) as f32,
            ]);
        }
        let Some(surface) = self.surface else {
            self.down &= !lifts;
            self.take_pressure(frame);
            return;
        };
        if let Some(at) = frame.position {
            self.position = at;
            self.emit(out, surface, PenPointer::Move(at), self.touching());
        } else if frame.enter.is_some()
            || (self.down && !lifts && (frame.pressure.is_some() || frame.tilt.is_some()))
        {
            self.emit(
                out,
                surface,
                PenPointer::Move(self.position),
                self.touching(),
            );
        }
        let at = self.position;
        let press = |button, pressed| PenPointer::Button {
            button,
            pressed,
            at,
        };
        if frame.down == Some(true) && !self.down {
            self.down = true;
            let primary = press(egui::PointerButton::Primary, true);
            self.emit(out, surface, primary, self.touching());
        }
        for &(code, pressed) in &frame.buttons {
            let Some(button) = barrel_button(code) else {
                continue;
            };
            let held = self.held.contains(&button);
            if pressed && !held {
                self.held.push(button);
            } else if !pressed && held {
                self.held.retain(|other| *other != button);
            } else {
                continue;
            }
            self.emit(out, surface, press(button, pressed), self.touching());
        }
        // Going away lets go of everything still held.
        if lifts {
            self.down = false;
            let primary = press(egui::PointerButton::Primary, false);
            self.emit(out, surface, primary, Some(self.pressure));
            self.take_pressure(frame);
        }
        if frame.leave {
            for button in std::mem::take(&mut self.held) {
                self.emit(out, surface, press(button, false), None);
            }
            self.emit(out, surface, PenPointer::Gone, None);
            self.surface = None;
        }
    }

    /// Its pressure while the tip touches.
    fn touching(&self) -> Option<f32> {
        self.down.then_some(self.pressure)
    }

    fn take_pressure(&mut self, frame: &ToolFrame) {
        if let Some(pressure) = frame.pressure {
            self.pressure = (f64::from(pressure) / 65535.0).clamp(0.0, 1.0) as f32;
        }
    }

    fn emit(
        &self,
        out: &mut Vec<PenEvent>,
        surface: usize,
        pointer: PenPointer,
        pressure: Option<f32>,
    ) {
        out.push(PenEvent {
            surface,
            pointer,
            reading: Reading {
                pressure,
                tilt: self.tilt,
                eraser: self.eraser,
            },
        });
    }
}

/// What wakes Sue's loop for a pen while it sleeps.
///
/// Under Wayland winit reads the socket a pen's events come on, but a
/// wake that brought winit nothing of its own (only tablet-v2 events, on
/// Sue's queue) never reaches the loop, so a pen that only hovers or draws
/// could wait for the next mouse or key event. A watcher thread waits on
/// the socket while the loop sleeps (with `poll` alone, nothing of
/// libwayland's) and wakes the loop once something arrives. This is its
/// state, shared by the loop and the thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Watch {
    /// The loop went to sleep: the thread should watch the socket.
    armed: bool,
    /// The thread is watching it now.
    watching: bool,
    /// The thread should end.
    stopped: bool,
}

/// What the socket did while the watcher waited on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Socket {
    /// Nothing (the wait was interrupted).
    Quiet,
    /// Something arrived to read.
    Readable,
    /// It closed or failed.
    Closed,
}

/// What the watcher does after its wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Watched {
    /// Wakes the loop, then waits until it sleeps again.
    Wake,
    /// Goes on watching.
    Again,
    /// Ends.
    Stop,
}

impl Watch {
    /// The loop is about to sleep. `true` when the thread must be told to
    /// watch: not while it already watches (from an earlier sleep) or was
    /// told already, and never once it was stopped.
    pub(crate) fn arm(&mut self) -> bool {
        if self.stopped || self.watching || self.armed {
            return false;
        }
        self.armed = true;
        true
    }

    /// The thread, before it waits on the socket: `Some(true)` to watch
    /// it now, `Some(false)` to end, `None` to wait until it is armed.
    pub(crate) fn begin(&mut self) -> Option<bool> {
        if self.stopped {
            return Some(false);
        }
        if !self.armed {
            return None;
        }
        self.armed = false;
        self.watching = true;
        Some(true)
    }

    /// The thread's wait ended: what the socket did, and whether the stop
    /// signal came.
    pub(crate) fn end(&mut self, socket: Socket, stop_signal: bool) -> Watched {
        self.watching = false;
        if stop_signal || self.stopped {
            self.stopped = true;
            return Watched::Stop;
        }
        match socket {
            Socket::Readable => Watched::Wake,
            // The loop still sleeps: keep watching for it.
            Socket::Quiet => {
                self.armed = true;
                Watched::Again
            }
            // Once, for the loop to find the failure, then never again (a
            // closed socket would wake it for ever).
            Socket::Closed => {
                self.stopped = true;
                Watched::Wake
            }
        }
    }

    /// Ends the thread.
    pub(crate) fn stop(&mut self) {
        self.stopped = true;
    }
}

/// The kernel's key code for a pen's tip in range (`BTN_TOOL_PEN`).
const BTN_TOOL_PEN: u32 = 0x140;
/// The kernel's axis code for pressure (`ABS_PRESSURE`).
const ABS_PRESSURE: u32 = 0x18;

/// `true` when the kernel's device list (`/proc/bus/input/devices`, which
/// every user may read) has a pen tablet: a device with a pen tool among
/// its keys and pressure among its axes. `word_bits` is the size of the
/// kernel's bitmap words (its `long`).
pub(crate) fn lists_pen(devices: &str, word_bits: u32) -> bool {
    devices.split("\n\n").any(|device| {
        let bitmap = |name: &str| {
            device.lines().find_map(|line| {
                line.strip_prefix("B: ")
                    .and_then(|rest| rest.strip_prefix(name))
                    .and_then(|rest| rest.strip_prefix('='))
            })
        };
        bitmap("KEY").is_some_and(|keys| has_bit(keys, BTN_TOOL_PEN, word_bits))
            && bitmap("ABS").is_some_and(|axes| has_bit(axes, ABS_PRESSURE, word_bits))
    })
}

/// Whether bit `bit` is set in a kernel bitmap written as hexadecimal
/// words, the most significant first.
fn has_bit(words: &str, bit: u32, word_bits: u32) -> bool {
    let words: Vec<&str> = words.split_whitespace().collect();
    let index = (bit / word_bits) as usize;
    let Some(word) = words
        .len()
        .checked_sub(index + 1)
        .and_then(|at| words.get(at))
    else {
        return false;
    };
    u64::from_str_radix(word, 16).is_ok_and(|value| (value >> (bit % word_bits)) & 1 == 1)
}

/// An XInput2 fixed point number (32 bits whole, 32 bits fraction).
pub(crate) fn fixed(integral: i32, frac: u32) -> f64 {
    f64::from(integral) + f64::from(frac) / 4_294_967_296.0
}

/// The value of valuator `number` in an XInput2 event, whose `mask` says
/// which valuators it carries and `values` holds those, in order.
pub(crate) fn valuator(mask: &[u32], values: &[f64], number: u16) -> Option<f64> {
    let number = usize::from(number);
    let word = *mask.get(number / 32)?;
    if (word >> (number % 32)) & 1 == 0 {
        return None;
    }
    // The values of the set bits below this one come first.
    let before: u32 = mask
        .iter()
        .take(number / 32)
        .map(|word| word.count_ones())
        .sum::<u32>()
        + (word & ((1u32 << (number % 32)) - 1)).count_ones();
    values.get(before as usize).copied()
}

/// `value` on an axis from `min` to `max` as a share from 0 to 1.
pub(crate) fn share(value: f64, min: f64, max: f64) -> f32 {
    if max <= min {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
}

/// `value` on an XInput2 tilt axis from `min` to `max` as a lean from -1
/// to 1, 1 being 90 degrees as under Wayland. X11's tablet drivers report
/// tilt in degrees on a signed axis (libinput's -64 to 63, the Wacom
/// driver's), some in radians times the axis's `resolution` (units per
/// radian) on a wider signed one; any other axis is taken to span 60
/// degrees each way, as far as most pens lean.
pub(crate) fn lean(value: f64, min: f64, max: f64, resolution: u32) -> f32 {
    if max <= min {
        return 0.0;
    }
    let degrees = if min < 0.0 && max.abs() <= 90.5 && min.abs() <= 90.5 {
        value
    } else if min < 0.0 && resolution > 0 {
        (value / f64::from(resolution)).to_degrees()
    } else {
        let middle = (min + max) / 2.0;
        (value - middle) / ((max - min) / 2.0) * 60.0
    };
    (degrees / 90.0).clamp(-1.0, 1.0) as f32
}

/// The pressure to report for a pen whose last pressure is `pressure`:
/// `tip` says whether its tip is down, when its tip's button has been seen
/// (`None` before that, when only the pressure tells). `None` when the
/// pen does not touch.
pub(crate) fn contact(pressure: f32, tip: Option<bool>) -> Option<f32> {
    tip.unwrap_or(pressure > 0.0).then_some(pressure)
}

/// `true` for the device name of a pen's eraser end, as X11's tablet
/// drivers and XWayland name them ("Wacom Intuos Pro M Pen eraser",
/// "xwayland-tablet eraser:21").
pub(crate) fn is_eraser(name: &str) -> bool {
    name.to_ascii_lowercase().contains("eraser")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pen_stays_on_wayland_unless_the_compositor_lacks_tablet_v2() {
        use DisplayServer::{Wayland, X11};
        let yes = || true;
        let no = || false;
        let unasked = || -> bool { panic!("the compositor need not be asked") };
        // The compositor reads pens: Wayland, as winit picks.
        assert_eq!(display_server(true, true, true, yes, None), None);
        // It does not: XWayland passes the pen on.
        assert_eq!(display_server(true, true, true, no, None), Some(X11));
        // Without a pen, or without XWayland, nothing to ask.
        assert_eq!(display_server(true, true, false, unasked, None), None);
        assert_eq!(display_server(true, false, true, unasked, None), None);
        // Plain X11 stays X11.
        assert_eq!(display_server(false, true, true, unasked, None), None);
        // A choice wins when it can be met.
        assert_eq!(
            display_server(true, true, true, unasked, Some("Wayland")),
            Some(Wayland)
        );
        assert_eq!(
            display_server(true, true, false, unasked, Some("x11")),
            Some(X11)
        );
        assert_eq!(display_server(true, false, true, no, Some("x11")), None);
        assert_eq!(
            display_server(true, true, true, no, Some("whatever")),
            Some(X11)
        );
    }

    fn frame() -> ToolFrame {
        ToolFrame::default()
    }

    #[test]
    fn a_pen_stroke_becomes_pointer_events_on_its_surface() {
        use egui::PointerButton::{Middle, Primary};
        let mut pen = PenTrack::default();
        let mut out = Vec::new();
        // Coming near over surface 7.
        pen.apply(
            &ToolFrame {
                enter: Some(7),
                position: Some([10.0, 20.0]),
                tilt: Some([45.0, -90.0]),
                ..frame()
            },
            &mut out,
        );
        assert_eq!(pointers(&out), [(7, PenPointer::Move([10.0, 20.0]))]);
        assert_eq!(pen.tilt, Some([0.5, -1.0]));
        out.clear();
        // Touching, then drawing with pressure.
        pen.apply(
            &ToolFrame {
                down: Some(true),
                pressure: Some(65535),
                ..frame()
            },
            &mut out,
        );
        let press = |button, pressed, at| PenPointer::Button {
            button,
            pressed,
            at,
        };
        assert_eq!(pointers(&out), [(7, press(Primary, true, [10.0, 20.0]))]);
        assert!(pen.down);
        assert_eq!(pen.pressure, 1.0);
        out.clear();
        pen.apply(
            &ToolFrame {
                position: Some([11.0, 21.0]),
                buttons: vec![(0x14b, true), (0x14b, true), (0x110, true)],
                ..frame()
            },
            &mut out,
        );
        assert_eq!(
            pointers(&out),
            [
                (7, PenPointer::Move([11.0, 21.0])),
                (7, press(Middle, true, [11.0, 21.0])),
            ],
            "a button already held and an unknown one add nothing"
        );
        out.clear();
        // Going away while touching and holding lets go of everything.
        pen.apply(
            &ToolFrame {
                leave: true,
                ..frame()
            },
            &mut out,
        );
        assert_eq!(
            pointers(&out),
            [
                (7, press(Primary, false, [11.0, 21.0])),
                (7, press(Middle, false, [11.0, 21.0])),
                (7, PenPointer::Gone),
            ]
        );
        assert_eq!(pen.surface, None);
        assert!(!pen.down);
        out.clear();
        // Out of range, nothing is pointed at.
        pen.apply(
            &ToolFrame {
                position: Some([1.0, 1.0]),
                ..frame()
            },
            &mut out,
        );
        assert!(out.is_empty());
    }

    /// The pointer events alone, with their surfaces.
    fn pointers(out: &[PenEvent]) -> Vec<(usize, PenPointer)> {
        out.iter()
            .map(|event| (event.surface, event.pointer))
            .collect()
    }

    /// The pressures read at each event.
    fn pressures(out: &[PenEvent]) -> Vec<Option<f32>> {
        out.iter().map(|event| event.reading.pressure).collect()
    }

    #[test]
    fn each_pen_event_carries_its_own_reading() {
        let mut pen = PenTrack {
            eraser: true,
            ..PenTrack::default()
        };
        let mut out = Vec::new();
        let half = 32768;
        // Hovering: no pressure, whatever the tablet says.
        pen.apply(
            &ToolFrame {
                enter: Some(3),
                position: Some([0.0, 0.0]),
                pressure: Some(0),
                tilt: Some([30.0, 0.0]),
                ..frame()
            },
            &mut out,
        );
        // Landing with a moving tip in one frame: the move, then the touch.
        pen.apply(
            &ToolFrame {
                down: Some(true),
                position: Some([1.0, 0.0]),
                pressure: Some(half),
                ..frame()
            },
            &mut out,
        );
        // Pressing harder in place still moves the stroke, at the new
        // pressure; a frame of nothing new adds nothing.
        pen.apply(
            &ToolFrame {
                pressure: Some(65535),
                ..frame()
            },
            &mut out,
        );
        pen.apply(&frame(), &mut out);
        // Lifting with a last motion and a pressure of 0: the last point
        // and the release keep the pressure the tip had.
        pen.apply(
            &ToolFrame {
                down: Some(false),
                position: Some([2.0, 0.0]),
                pressure: Some(0),
                ..frame()
            },
            &mut out,
        );
        let half = (f64::from(half) / 65535.0) as f32;
        assert_eq!(
            pressures(&out),
            [None, None, Some(half), Some(1.0), Some(1.0), Some(1.0)]
        );
        assert_eq!(out[1].pointer, PenPointer::Move([1.0, 0.0]));
        assert!(matches!(
            out[2].pointer,
            PenPointer::Button { pressed: true, .. }
        ));
        assert_eq!(out[3].pointer, PenPointer::Move([1.0, 0.0]));
        assert_eq!(out[4].pointer, PenPointer::Move([2.0, 0.0]));
        assert!(matches!(
            out[5].pointer,
            PenPointer::Button { pressed: false, .. }
        ));
        assert!(out.iter().all(|event| {
            event.reading.eraser
                && event
                    .reading
                    .tilt
                    .is_some_and(|[x, y]| (x - 1.0 / 3.0).abs() < 1e-6 && y == 0.0)
        }));
        assert_eq!(pen.pressure, 0.0, "the lift's own pressure comes after");
        out.clear();
        // Hovering again: tilt alone moves nothing.
        pen.apply(
            &ToolFrame {
                tilt: Some([0.0, 0.0]),
                ..frame()
            },
            &mut out,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn the_watcher_wakes_the_loop_once_per_sleep() {
        let mut watch = Watch::default();
        // Unarmed, the thread waits.
        assert_eq!(watch.begin(), None);
        // The loop sleeps: the thread is told once, and watches.
        assert!(watch.arm());
        assert!(!watch.arm(), "told already");
        assert_eq!(watch.begin(), Some(true));
        assert!(!watch.arm(), "watching already, from an earlier sleep");
        // Something arrived: wake the loop, then wait to be armed again.
        assert_eq!(watch.end(Socket::Readable, false), Watched::Wake);
        assert_eq!(watch.begin(), None);
        // An interrupted wait goes on watching by itself.
        assert!(watch.arm());
        assert_eq!(watch.begin(), Some(true));
        assert_eq!(watch.end(Socket::Quiet, false), Watched::Again);
        assert_eq!(watch.begin(), Some(true));
        // A closed socket wakes the loop once, then the thread ends.
        assert_eq!(watch.end(Socket::Closed, false), Watched::Wake);
        assert_eq!(watch.begin(), Some(false));
        assert!(!watch.arm());
        // The stop signal ends it, whatever the socket did.
        let mut watch = Watch::default();
        assert!(watch.arm());
        assert_eq!(watch.begin(), Some(true));
        assert_eq!(watch.end(Socket::Readable, true), Watched::Stop);
        assert_eq!(watch.begin(), Some(false));
        // Stopped while waiting to be armed.
        let mut watch = Watch::default();
        watch.stop();
        assert_eq!(watch.begin(), Some(false));
        assert!(!watch.arm());
    }

    #[test]
    fn pen_pointer_events_scale_to_egui_points() {
        let modifiers = egui::Modifiers::default();
        assert_eq!(
            PenPointer::Move([10.0, 4.0]).to_egui(1.5, modifiers),
            egui::Event::PointerMoved(egui::pos2(15.0, 6.0))
        );
        assert_eq!(
            PenPointer::Gone.to_egui(2.0, modifiers),
            egui::Event::PointerGone
        );
        assert_eq!(barrel_button(0x14c), Some(egui::PointerButton::Secondary));
        assert_eq!(barrel_button(0x110), None);
    }

    /// A trimmed `/proc/bus/input/devices` with a mouse, an XP-Pen pen and
    /// a keyboard, as a 64-bit kernel writes it.
    const DEVICES: &str = "\
I: Bus=0003 Vendor=046d Product=c077 Version=0111
N: Name=\"Logitech USB Optical Mouse\"
H: Handlers=mouse0 event3
B: EV=17
B: KEY=1f0000 0 0 0 0
B: REL=903

I: Bus=0003 Vendor=28bd Product=0928 Version=0100
N: Name=\"UGTABLET 10 inch PenTablet Pen\"
H: Handlers=mouse2 event5
B: PROP=0
B: EV=1b
B: KEY=1c03 0 0 0 0 0
B: ABS=d000003
B: MSC=10

I: Bus=0011 Vendor=0001 Product=0001 Version=ab41
N: Name=\"AT Translated Set 2 keyboard\"
B: EV=120013
B: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe
B: MSC=10
";

    #[test]
    fn the_kernels_device_list_shows_a_pen_tablet() {
        assert!(lists_pen(DEVICES, 64));
        let without_pen: String = DEVICES
            .split("\n\n")
            .filter(|device| !device.contains("PenTablet"))
            .collect::<Vec<_>>()
            .join("\n\n");
        assert!(!lists_pen(&without_pen, 64));
        assert!(!lists_pen("", 64));
        // BTN_TOOL_PEN is bit 0 of the sixth 64-bit word, and of the
        // eleventh 32-bit one.
        assert!(has_bit("1 0 0 0 0 0", 0x140, 64));
        assert!(has_bit("1 0 0 0 0 0 0 0 0 0 0", 0x140, 32));
        assert!(!has_bit("1 0 0 0 0", 0x140, 64));
        assert!(!has_bit("zz 0 0 0 0 0", 0x140, 64));
    }

    #[test]
    fn valuators_are_found_by_their_mask() {
        // Valuators 0, 1, 2 and 33 are in the event.
        let mask = [0b111, 0b10];
        let values = [10.0, 20.0, 30.0, 40.0];
        assert_eq!(valuator(&mask, &values, 0), Some(10.0));
        assert_eq!(valuator(&mask, &values, 2), Some(30.0));
        assert_eq!(valuator(&mask, &values, 3), None);
        assert_eq!(valuator(&mask, &values, 33), Some(40.0));
        assert_eq!(valuator(&mask, &values, 64), None);
        assert_eq!(valuator(&[0b100], &[], 2), None, "a short event");
        assert!((fixed(3, 1 << 31) - 3.5).abs() < 1e-9);
        assert!((fixed(-1, 0) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn axes_map_to_shares_and_leans() {
        assert!((share(32767.5, 0.0, 65535.0) - 0.5).abs() < 1e-6);
        assert_eq!(share(70000.0, 0.0, 65535.0), 1.0);
        assert_eq!(share(5.0, 1.0, 1.0), 0.0, "an empty range");
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // A signed axis in degrees (libinput's -64 to 63): 45 degrees is
        // half of 90, as under Wayland, not 45 of 64.
        assert!(near(lean(45.0, -64.0, 63.0, 57), 0.5));
        assert!(near(lean(-90.0, -90.0, 90.0, 0), -1.0));
        assert_eq!(lean(0.0, -64.0, 63.0, 0), 0.0);
        // Radians times the resolution on a wider axis.
        let quarter_turn = std::f64::consts::FRAC_PI_4 * 1000.0;
        assert!(near(lean(quarter_turn, -1571.0, 1571.0, 1000), 0.5));
        // An unsigned axis spans 60 degrees each way from its middle.
        assert!(near(lean(127.0, 0.0, 127.0, 0), 60.0 / 90.0));
        assert!(near(lean(63.5, 0.0, 127.0, 0), 0.0));
        assert_eq!(lean(5.0, 1.0, 1.0, 0), 0.0, "an empty range");
    }

    #[test]
    fn a_pen_touches_while_its_tip_is_down_or_it_presses() {
        assert_eq!(contact(0.0, None), None);
        assert_eq!(contact(0.3, None), Some(0.3));
        assert_eq!(contact(0.0, Some(true)), Some(0.0), "touching, lightly");
        assert_eq!(
            contact(0.3, Some(false)),
            None,
            "lifted before its last pressure"
        );
        assert!(is_eraser("xwayland-tablet eraser:21"));
        assert!(is_eraser("Wacom Intuos Pro M Pen Eraser"));
        assert!(!is_eraser("xwayland-tablet stylus:20"));
    }
}
