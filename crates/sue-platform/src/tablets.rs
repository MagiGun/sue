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
    /// Its barrel buttons held, as pointer buttons.
    held: Vec<egui::PointerButton>,
}

impl PenTrack {
    /// Takes one frame, adding the pointer events it makes to `out`, each
    /// with the surface it is for: coming near, moving, the tip and
    /// buttons going down, then up, then going away.
    pub(crate) fn apply(&mut self, frame: &ToolFrame, out: &mut Vec<(usize, PenPointer)>) {
        if let Some(surface) = frame.enter {
            self.surface = Some(surface);
        }
        if let Some(pressure) = frame.pressure {
            self.pressure = (pressure as f32 / 65535.0).clamp(0.0, 1.0);
        }
        if let Some([x, y]) = frame.tilt {
            self.tilt = Some([
                (x / 90.0).clamp(-1.0, 1.0) as f32,
                (y / 90.0).clamp(-1.0, 1.0) as f32,
            ]);
        }
        let Some(surface) = self.surface else {
            return;
        };
        if let Some(at) = frame.position {
            self.position = at;
            out.push((surface, PenPointer::Move(at)));
        } else if frame.enter.is_some() {
            out.push((surface, PenPointer::Move(self.position)));
        }
        let at = self.position;
        let press = |button, pressed| PenPointer::Button {
            button,
            pressed,
            at,
        };
        if frame.down == Some(true) && !self.down {
            self.down = true;
            out.push((surface, press(egui::PointerButton::Primary, true)));
        }
        for &(code, pressed) in &frame.buttons {
            let Some(button) = barrel_button(code) else {
                continue;
            };
            let held = self.held.contains(&button);
            if pressed && !held {
                self.held.push(button);
                out.push((surface, press(button, true)));
            } else if !pressed && held {
                self.held.retain(|other| *other != button);
                out.push((surface, press(button, false)));
            }
        }
        // Going away lets go of everything still held.
        let lift = frame.down == Some(false) || frame.leave;
        if lift && self.down {
            self.down = false;
            out.push((surface, press(egui::PointerButton::Primary, false)));
        }
        if frame.leave {
            for button in std::mem::take(&mut self.held) {
                out.push((surface, press(button, false)));
            }
            out.push((surface, PenPointer::Gone));
            self.surface = None;
        }
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

/// `value` on a tilt axis from `min` to `max` as a lean from -1 to 1.
pub(crate) fn lean(value: f64, min: f64, max: f64) -> f32 {
    share(value, min, max) * 2.0 - 1.0
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
        assert_eq!(out, [(7, PenPointer::Move([10.0, 20.0]))]);
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
        assert_eq!(out, [(7, press(Primary, true, [10.0, 20.0]))]);
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
            out,
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
            out,
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
        assert_eq!(lean(-64.0, -64.0, 64.0), -1.0);
        assert_eq!(lean(0.0, -64.0, 64.0), 0.0);
        assert_eq!(lean(64.0, -64.0, 64.0), 1.0);
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
