//! A pen's pressure where the window's touch events do not carry it: a
//! WinTab tablet on Windows (tablet drivers with Windows Ink turned off,
//! common among artists), a tablet on Linux's Wayland (winit has no tablet
//! protocol, so a compositor shows its pen to winit as a mouse) and a pen
//! in a browser, where winit treats pens as mice. With Windows Ink, on
//! macOS and phones the pressure arrives with egui's touch events instead.
//!
//! The host updates it every frame; studios read [`pressure`] when a
//! stroke's touch events bring none.

use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

/// Where [`pressure`] comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenSource {
    /// A WinTab tablet driver (Windows).
    WinTab,
    /// A pen's pointer events in a browser.
    Browser,
    /// A tablet on Wayland's tablet protocol (Linux).
    Wayland,
}

/// The pressure's bits, or `u32::MAX` while no pen touches.
static PRESSURE: AtomicU32 = AtomicU32::new(u32::MAX);
/// The last [`PenSource`] seen: 0 none, 1 WinTab, 2 a browser, 3 Wayland.
static SOURCE: AtomicU8 = AtomicU8::new(0);

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
        3 => Some(PenSource::Wayland),
        _ => None,
    }
}

/// Records what the pen does now: its pressure while it touches, else
/// `None`. Only Windows' tablet driver, Wayland's tablet protocol and the
/// browser report pressure this way.
#[cfg_attr(
    not(any(target_os = "windows", target_os = "linux", target_arch = "wasm32")),
    allow(dead_code)
)]
pub(crate) fn set(from: PenSource, pressure: Option<f32>) {
    SOURCE.store(
        match from {
            PenSource::WinTab => 1,
            PenSource::Browser => 2,
            PenSource::Wayland => 3,
        },
        Ordering::Relaxed,
    );
    let bits = pressure.map_or(u32::MAX, |value| value.clamp(0.0, 1.0).to_bits());
    PRESSURE.store(bits, Ordering::Relaxed);
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
    }
}
