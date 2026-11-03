//! Pen tablets on Linux and the BSDs: the plain logic, kept apart from the
//! X11 calls in `crate::xinput_pen` so it runs and is tested everywhere.
//!
//! winit 0.30 does not speak Wayland's tablet protocol (`tablet-v2`), so
//! under Wayland a pen reaches Sue only when the compositor turns it into a
//! mouse, which not all do (under niri the pen did nothing). XWayland does
//! speak it, and hands the pen to X11 programs as an XInput2 device that
//! also moves the pointer. So when a pen tablet is connected and XWayland
//! is there, Sue opens its windows through it ([`display_server`]): the
//! pen moves and draws as a mouse does, and XInput2 adds its pressure,
//! tilt and eraser.

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
/// whether a pen tablet is connected, `choice` the value of
/// [`CHOICE_VARIABLE`]. `None` leaves it to winit, which prefers Wayland.
pub(crate) fn display_server(
    wayland: bool,
    x11: bool,
    pen: bool,
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
    (wayland && x11 && pen).then_some(DisplayServer::X11)
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
    fn a_pen_under_wayland_opens_through_xwayland_unless_chosen_otherwise() {
        use DisplayServer::{Wayland, X11};
        assert_eq!(display_server(true, true, true, None), Some(X11));
        // Without a pen, or without XWayland, winit decides (Wayland).
        assert_eq!(display_server(true, true, false, None), None);
        assert_eq!(display_server(true, false, true, None), None);
        // Plain X11 stays X11.
        assert_eq!(display_server(false, true, true, None), None);
        // A choice wins when it can be met.
        assert_eq!(
            display_server(true, true, true, Some("Wayland")),
            Some(Wayland)
        );
        assert_eq!(display_server(true, true, false, Some("x11")), Some(X11));
        assert_eq!(display_server(true, false, true, Some("x11")), None);
        assert_eq!(
            display_server(true, true, true, Some("whatever")),
            Some(X11)
        );
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
