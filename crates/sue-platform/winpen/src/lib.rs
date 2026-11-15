//! Sue on Windows: a Windows Ink pen's packet history.
//!
//! Windows hands a pen's packets to a window in pointer messages, and
//! when the window has not read its queue for a while (it was drawing a
//! frame) one message stands for several packets. winit 0.30 makes one
//! touch event of each packet, at the packet's place, but with the
//! pressure the pen has now (`GetPointerPenInfo`) for all of them, and
//! Sue stamps them all with the moment they arrived: a frame's packets
//! share one pressure and one time. [`pen_history`] reads the history of
//! the message being handled (`GetPointerPenInfoHistory`), so each packet
//! keeps its own pressure, lean, twist and time.
//!
//! [`pen_history`] holds this crate's one `unsafe` block, under the
//! owner's standing approval of 2026-11-13 for unsafe code that brings a
//! real benefit: the operating system offers this reading through no
//! safe call. On every other system the same functions find nothing.

/// One packet of a Windows Ink pen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PenPacket {
    /// The pointer frame the packet belongs to.
    pub frame: u32,
    /// Where the pen was, in screen pixels.
    pub screen: [i32; 2],
    /// How hard it pressed, 0 to 1, when the pen says.
    pub pressure: Option<f32>,
    /// How far it leaned, `[x, y]` from -1 to 1 (x to the right, y toward
    /// the person, 1 for 90 degrees), when the pen says.
    pub tilt: Option<[f32; 2]>,
    /// Its turn about its own axis, degrees clockwise, when the pen says.
    pub twist: Option<f32>,
    /// The eraser end (or the eraser button) is in use.
    pub eraser: bool,
    /// When, in milliseconds on the system's tick count.
    pub time: u32,
}

/// The most packets of one message that are read.
pub const MOST: usize = 64;

/// Pressure as Windows Ink reports it: 0 to 1024.
const FULL_PRESSURE: f32 = 1024.0;

/// A packet as Windows reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Raw {
    /// The pointer frame.
    pub frame: u32,
    /// The place in screen pixels.
    pub screen: [i32; 2],
    /// `PEN_MASK_*` bits: which readings the pen reports.
    pub mask: u32,
    /// `PEN_FLAG_*` bits.
    pub flags: u32,
    /// 0 to 1024.
    pub pressure: u32,
    /// Degrees, 0 to 359.
    pub rotation: u32,
    /// Degrees, -90 to 90.
    pub tilt: [i32; 2],
    /// Milliseconds on the tick count.
    pub time: u32,
}

impl Raw {
    /// The packet in Sue's terms.
    #[must_use]
    pub fn packet(&self) -> PenPacket {
        const PRESSURE: u32 = 1;
        const ROTATION: u32 = 2;
        const TILT_X: u32 = 4;
        const TILT_Y: u32 = 8;
        const INVERTED: u32 = 2;
        const ERASER: u32 = 4;
        let has = |bit: u32| self.mask & bit != 0;
        PenPacket {
            frame: self.frame,
            screen: self.screen,
            pressure: has(PRESSURE).then(|| (self.pressure as f32 / FULL_PRESSURE).clamp(0.0, 1.0)),
            tilt: (has(TILT_X) || has(TILT_Y)).then(|| {
                let x = if has(TILT_X) { self.tilt[0] } else { 0 };
                let y = if has(TILT_Y) { self.tilt[1] } else { 0 };
                [x, y].map(|degrees| (degrees as f32 / 90.0).clamp(-1.0, 1.0))
            }),
            twist: has(ROTATION).then_some((self.rotation % 360) as f32),
            eraser: self.flags & (INVERTED | ERASER) != 0,
            time: self.time,
        }
    }
}

/// The packets the pointer message being handled stands for, of pen
/// pointer `pointer`, oldest first; empty when `pointer` is no pen, the
/// history cannot be read, or on another system.
#[must_use]
pub fn pen_history(pointer: u32) -> Vec<PenPacket> {
    raw_history(pointer).iter().rev().map(Raw::packet).collect()
}

/// The raw history, newest first, as Windows hands it over.
#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "GetPointerPenInfoHistory has no safe binding; the owner's standing approval of 2026-11-13 covers operating system calls without one (backbone rule 6)"
)]
fn raw_history(pointer: u32) -> Vec<Raw> {
    use windows::Win32::UI::Input::Pointer::{GetPointerPenInfoHistory, POINTER_PEN_INFO};
    use windows::Win32::UI::WindowsAndMessaging::PT_PEN;

    let mut buffer = [POINTER_PEN_INFO::default(); MOST];
    let mut count = MOST as u32;
    // SAFETY: `buffer` is an array of `MOST` initialized POINTER_PEN_INFO
    // records that lives across the call, and `count` says it holds
    // `MOST`: Windows writes at most `count` records into it, sets `count`
    // to how many it wrote, and keeps neither pointer after the call
    // returns. A pointer id that is no pen, or no longer exists, makes
    // the call fail without writing, which is the error handled below.
    let read =
        unsafe { GetPointerPenInfoHistory(pointer, &raw mut count, Some(buffer.as_mut_ptr())) };
    if read.is_err() {
        return Vec::new();
    }
    buffer[..(count as usize).min(MOST)]
        .iter()
        .filter(|info| info.pointerInfo.pointerType == PT_PEN)
        .map(|info| Raw {
            frame: info.pointerInfo.frameId,
            screen: [
                info.pointerInfo.ptPixelLocation.x,
                info.pointerInfo.ptPixelLocation.y,
            ],
            mask: info.penMask,
            flags: info.penFlags,
            pressure: info.pressure,
            rotation: info.rotation,
            tilt: [info.tiltX, info.tiltY],
            time: info.pointerInfo.dwTime,
        })
        .collect()
}

/// No history on other systems.
#[cfg(not(target_os = "windows"))]
fn raw_history(_pointer: u32) -> Vec<Raw> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raw_packet_says_only_what_the_pen_reports() {
        let raw = Raw {
            frame: 7,
            screen: [10, 20],
            mask: 1 | 4,
            flags: 0,
            pressure: 256,
            rotation: 400,
            tilt: [45, -30],
            time: 99,
        };
        let packet = raw.packet();
        assert_eq!(packet.pressure, Some(0.25));
        assert_eq!(packet.tilt, Some([0.5, 0.0]), "only tilt x is reported");
        assert_eq!(packet.twist, None);
        assert!(!packet.eraser);
        let all = Raw {
            mask: 1 | 2 | 4 | 8,
            flags: 4,
            pressure: 4096,
            ..raw
        }
        .packet();
        assert_eq!(all.pressure, Some(1.0), "held to full");
        assert_eq!(all.twist, Some(40.0));
        assert_eq!(all.tilt, Some([0.5, -30.0 / 90.0]));
        assert!(all.eraser);
    }

    #[test]
    fn a_pointer_that_is_no_pen_has_no_history() {
        // No pointer has this id: the call fails and nothing is read.
        assert!(pen_history(u32::MAX - 7).is_empty());
    }
}
