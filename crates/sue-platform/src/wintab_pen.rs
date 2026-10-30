//! Pen pressure from a WinTab tablet driver on Windows: Huion, XP-Pen,
//! Wacom and others always offer WinTab, while Windows Ink (which reaches
//! Sue through winit's touch events) is often turned off in their settings.
//!
//! A system context on Sue's window is opened through the `wintab` crate,
//! which loads the driver's `wintab32.dll` from System32 and keeps its own
//! calls in it (this crate stays free of `unsafe`). Its packet queue is
//! drained every frame and the latest pressure goes to [`crate::stylus`];
//! the pen still moves the system pointer, so positions come with the
//! window's pointer events as before.

use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::stylus::{self, PenSource};

wintab::packet! {
    struct Sample { buttons, normal_pressure }
}

/// An open WinTab context on Sue's window.
pub(crate) struct Tablet {
    context: wintab::Context<Sample>,
    /// The tip's pressure range.
    pressure: Option<wintab::Axis>,
    packets: Vec<Sample>,
}

impl std::fmt::Debug for Tablet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tablet").finish_non_exhaustive()
    }
}

impl Tablet {
    /// The tablet, when a WinTab driver is installed and answers.
    pub(crate) fn open(window: &Window) -> Option<Self> {
        let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        let hwnd = windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut std::ffi::c_void);
        let wintab = wintab::Wintab::load().ok()?;
        let pressure = wintab.device(0).and_then(|device| device.pressure);
        let options = wintab::ContextOptions::new()
            .system(true)
            .queue_size(Some(128));
        let context = wintab.open::<Sample>(hwnd, &options).ok()?;
        Some(Self {
            context,
            pressure,
            packets: Vec::new(),
        })
    }

    /// Takes the packets since the last frame: the latest tells how hard
    /// the pen presses, or that it does not touch.
    pub(crate) fn poll(&mut self) {
        self.packets.clear();
        if self.context.drain(&mut self.packets) == 0 {
            return;
        }
        let Some(last) = self.packets.last().copied() else {
            return;
        };
        let raw = { last.normal_pressure };
        let buttons = { last.buttons };
        stylus::set(
            PenSource::WinTab,
            touching(raw, buttons.is_pressed(0), self.pressure),
        );
    }
}

/// The pressure, 0 to 1, of a packet with tip pressure `raw` (the tip
/// button held or not) on an axis `range`: `None` when the pen does not
/// touch.
fn touching(raw: u32, tip: bool, range: Option<wintab::Axis>) -> Option<f32> {
    if raw == 0 && !tip {
        return None;
    }
    let value = i32::try_from(raw).unwrap_or(i32::MAX);
    Some(range.map_or_else(
        // Without a range, the common 1024 or 8192 levels.
        || (raw as f32 / if raw > 1024 { 8191.0 } else { 1023.0 }).clamp(0.0, 1.0),
        |axis| axis.normalize(value),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lifted_pen_has_no_pressure_and_a_pressed_one_its_share() {
        assert_eq!(touching(0, false, None), None);
        assert_eq!(touching(0, true, None), Some(0.0), "touching, lightly");
        assert!((touching(512, true, None).unwrap() - 0.5).abs() < 0.01);
        assert!((touching(8191, true, None).unwrap() - 1.0).abs() < 1e-3);
    }
}
