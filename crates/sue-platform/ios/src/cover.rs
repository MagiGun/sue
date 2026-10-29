//! How much of a window the on-screen keyboard covers, in plain numbers so
//! that it is tested on every system.

/// A rectangle in points: its top left corner and its size, with y going
/// down, as in UIKit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// The points at the bottom of `window` that `keyboard` covers, both in
/// the window's coordinates. A keyboard docked at the bottom covers from
/// its top edge down. One that floats or is split on an iPad (it stops
/// above the window's bottom edge), lies beside the window or has no size
/// covers nothing, as UIKit's keyboard layout guide counts it.
pub(crate) fn covered(window: Frame, keyboard: Frame) -> f64 {
    let bottom = window.y + window.height;
    let docked = keyboard.width > 0.0
        && keyboard.height > 0.0
        && keyboard.y + keyboard.height >= bottom - 1.0
        && keyboard.x < window.x + window.width
        && keyboard.x + keyboard.width > window.x;
    if docked {
        (bottom - keyboard.y).min(window.height).max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: f64, y: f64, width: f64, height: f64) -> Frame {
        Frame {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_docked_keyboard_covers_from_its_top_edge_down() {
        let phone = frame(0.0, 0.0, 390.0, 844.0);
        assert_eq!(covered(phone, frame(0.0, 508.0, 390.0, 336.0)), 336.0);
        // A hardware keyboard leaves only the shortcut bar.
        assert_eq!(covered(phone, frame(0.0, 789.0, 390.0, 55.0)), 55.0);
    }

    #[test]
    fn a_keyboard_that_went_away_covers_nothing() {
        let phone = frame(0.0, 0.0, 390.0, 844.0);
        // Hiding ends below the screen.
        assert_eq!(covered(phone, frame(0.0, 844.0, 390.0, 336.0)), 0.0);
        assert_eq!(covered(phone, frame(0.0, 0.0, 0.0, 0.0)), 0.0);
    }

    #[test]
    fn a_floating_or_split_keyboard_covers_nothing() {
        let tablet = frame(0.0, 0.0, 1024.0, 1366.0);
        assert_eq!(covered(tablet, frame(300.0, 700.0, 320.0, 260.0)), 0.0);
        assert_eq!(covered(tablet, frame(0.0, 600.0, 1024.0, 300.0)), 0.0);
    }

    #[test]
    fn a_window_beside_or_above_the_keyboard_is_free() {
        // Stage Manager: a window placed above the keyboard.
        let high = frame(100.0, 50.0, 600.0, 400.0);
        assert_eq!(covered(high, frame(-100.0, 650.0, 1366.0, 374.0)), 0.0);
        // A keyboard wholly to the side of the window.
        let left = frame(0.0, 0.0, 400.0, 1024.0);
        assert_eq!(covered(left, frame(500.0, 700.0, 500.0, 324.0)), 0.0);
    }

    #[test]
    fn a_window_partly_over_the_keyboard_loses_only_that_part() {
        // A window from 100 to 900 points down, the keyboard from 624 to the
        // screen's bottom at 1024, both in the window's coordinates.
        let window = frame(0.0, 0.0, 800.0, 800.0);
        assert_eq!(covered(window, frame(-200.0, 524.0, 1366.0, 400.0)), 276.0);
        // Never more than the whole window.
        assert_eq!(covered(window, frame(0.0, -50.0, 800.0, 900.0)), 800.0);
    }
}
