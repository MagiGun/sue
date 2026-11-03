//! A pen's pressure where the window's touch events do not carry it: a
//! WinTab tablet on Windows (tablet drivers with Windows Ink turned off,
//! common among artists), a pen tablet on Linux and the BSDs (Wayland's
//! tablet-v2, or XInput2 under X11 and XWayland), and a pen in a browser,
//! where winit treats pens as mice. With Windows Ink, on macOS and phones
//! the pressure arrives with egui's touch events instead.
//!
//! The host updates it every frame; studios read [`pressure`] when a
//! stroke's touch events bring none, and [`tilt`] and [`eraser`] where a
//! source reports them.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

/// Where [`pressure`] comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenSource {
    /// A WinTab tablet driver (Windows).
    WinTab,
    /// A pen's pointer events in a browser.
    Browser,
    /// A pen tablet's XInput2 device (X11 on Linux and the BSDs, or
    /// XWayland under a Wayland compositor).
    XInput,
    /// A pen through Wayland's tablet-v2 protocol (Linux and the BSDs).
    WaylandTablet,
}

/// The pressure's bits, or `u32::MAX` while no pen touches.
static PRESSURE: AtomicU32 = AtomicU32::new(u32::MAX);
/// The last [`PenSource`] seen: 0 none, 1 WinTab, 2 a browser, 3 XInput2,
/// 4 Wayland's tablet-v2.
static SOURCE: AtomicU8 = AtomicU8::new(0);
/// The pen's lean to the right and toward the person, each a bit pattern
/// of -1 to 1, or `u32::MAX` when the source does not say.
static TILT_X: AtomicU32 = AtomicU32::new(u32::MAX);
static TILT_Y: AtomicU32 = AtomicU32::new(u32::MAX);
/// `true` while the pen's eraser end is the one in use.
static ERASER: AtomicBool = AtomicBool::new(false);

/// The pen's pressure, 0 to 1, while it touches; `None` when no such pen
/// touches.
#[must_use]
pub fn pressure() -> Option<f32> {
    let bits = PRESSURE.load(Ordering::Relaxed);
    (bits != u32::MAX).then(|| f32::from_bits(bits))
}

/// Where pen pressure last came from, once a pen has been seen.
#[must_use]
pub fn source() -> Option<PenSource> {
    match SOURCE.load(Ordering::Relaxed) {
        1 => Some(PenSource::WinTab),
        2 => Some(PenSource::Browser),
        3 => Some(PenSource::XInput),
        4 => Some(PenSource::WaylandTablet),
        _ => None,
    }
}

/// How far the pen leans, `[x, y]`, each from -1 to 1: x to the right, y
/// toward the person, 0 upright and 1 the farthest the source reports (90
/// degrees under Wayland, the axis's end under XInput2).
/// `None` when the pen's source does not report tilt.
#[must_use]
pub fn tilt() -> Option<[f32; 2]> {
    let x = TILT_X.load(Ordering::Relaxed);
    let y = TILT_Y.load(Ordering::Relaxed);
    (x != u32::MAX && y != u32::MAX).then(|| [f32::from_bits(x), f32::from_bits(y)])
}

/// `true` while the pen is turned over to its eraser end (or an eraser
/// tool is in use), where the source tells the ends apart.
#[must_use]
pub fn eraser() -> bool {
    ERASER.load(Ordering::Relaxed)
}

/// Records what the pen does now: its pressure while it touches, else
/// `None`. Only Windows' tablet driver, XInput2 and the browser report
/// pressure this way.
#[cfg_attr(
    not(any(
        target_os = "windows",
        target_arch = "wasm32",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )),
    allow(dead_code)
)]
pub(crate) fn set(from: PenSource, pressure: Option<f32>) {
    SOURCE.store(
        match from {
            PenSource::WinTab => 1,
            PenSource::Browser => 2,
            PenSource::XInput => 3,
            PenSource::WaylandTablet => 4,
        },
        Ordering::Relaxed,
    );
    let bits = pressure.map_or(u32::MAX, |value| value.clamp(0.0, 1.0).to_bits());
    PRESSURE.store(bits, Ordering::Relaxed);
}

/// Records which end of the pen is in use and how far it leans (`None`
/// when the source does not say). Only XInput2 and Wayland's tablet-v2
/// report these so far.
#[cfg_attr(
    not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )),
    allow(dead_code)
)]
pub(crate) fn set_tool(tilt: Option<[f32; 2]>, eraser: bool) {
    let [x, y] = tilt.map_or([u32::MAX; 2], |[x, y]| {
        [x.clamp(-1.0, 1.0).to_bits(), y.clamp(-1.0, 1.0).to_bits()]
    });
    TILT_X.store(x, Ordering::Relaxed);
    TILT_Y.store(y, Ordering::Relaxed);
    ERASER.store(eraser, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pens_pressure_reads_back_until_it_lifts() {
        set(PenSource::WinTab, Some(0.42));
        assert_eq!(pressure(), Some(0.42));
        assert_eq!(source(), Some(PenSource::WinTab));
        set(PenSource::Browser, Some(3.0));
        assert_eq!(pressure(), Some(1.0), "clamped");
        set(PenSource::Browser, None);
        assert_eq!(pressure(), None);
        assert_eq!(source(), Some(PenSource::Browser));
        set(PenSource::XInput, Some(0.5));
        assert_eq!(source(), Some(PenSource::XInput));
        set(PenSource::XInput, None);
    }

    #[test]
    fn tilt_and_the_eraser_read_back() {
        set_tool(Some([0.25, -2.0]), true);
        assert_eq!(tilt(), Some([0.25, -1.0]), "clamped");
        assert!(eraser());
        set_tool(None, false);
        assert_eq!(tilt(), None);
        assert!(!eraser());
    }
}
