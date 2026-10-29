//! The on-screen keyboard: where UIKit says it ends up each time it moves,
//! comes up or goes away, and how much of Sue's window that covers.

use std::sync::{Mutex, Once, PoisonError};

use objc2::MainThreadMarker;
use objc2_core_foundation::CGRect;
use objc2_ui_kit::UICoordinateSpace;

use crate::cover::{Frame, covered};

/// The keyboard's frame in the screen's points, as UIKit last told it;
/// `None` while it is hidden.
static KEYBOARD: Mutex<Option<CGRect>> = Mutex::new(None);

/// The observers are added once.
static WATCH: Once = Once::new();

/// The points at the bottom of Sue's window that the on-screen keyboard
/// covers: 0 while it is hidden, floating or split. The first call, on
/// the main thread, starts watching the keyboard; off the main thread
/// this is 0.
#[must_use]
pub fn keyboard_inset() -> f32 {
    let Some(mtm) = MainThreadMarker::new() else {
        return 0.0;
    };
    WATCH.call_once(audited::observe);
    let Some(keyboard) = *KEYBOARD.lock().unwrap_or_else(PoisonError::into_inner) else {
        return 0.0;
    };
    let Some(window) = crate::window::key_window(mtm) else {
        return 0.0;
    };
    // UIKit gives the frame in the screen's coordinates; the window may
    // not fill the screen (Slide Over, Stage Manager).
    let keyboard =
        window.convertRect_fromCoordinateSpace(keyboard, &window.screen().coordinateSpace());
    covered(frame(window.bounds()), frame(keyboard)) as f32
}

fn keep(keyboard: Option<CGRect>) {
    *KEYBOARD.lock().unwrap_or_else(PoisonError::into_inner) = keyboard;
}

fn frame(rect: CGRect) -> Frame {
    Frame {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

/// Every call into Objective-C that Rust counts as unsafe.
#[allow(
    unsafe_code,
    reason = "UIKit tells the keyboard's frame only through notifications, observed with blocks; approved on 2026-10-29"
)]
mod audited {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_core_foundation::CGRect;
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue, NSValue};
    use objc2_ui_kit::{
        UIKeyboardFrameEndUserInfoKey, UIKeyboardWillChangeFrameNotification,
        UIKeyboardWillHideNotification,
    };

    /// Keeps the keyboard's frame from now on: UIKit runs the blocks on
    /// the main queue each time the keyboard moves, comes up or goes away.
    pub(super) fn observe() {
        let center = NSNotificationCenter::defaultCenter();
        let queue = NSOperationQueue::mainQueue();
        let moved = RcBlock::new(|notification: NonNull<NSNotification>| {
            // SAFETY: UIKit hands the block a live notification for the
            // length of the call.
            super::keep(end_frame(unsafe { notification.as_ref() }));
        });
        let hidden = RcBlock::new(|_: NonNull<NSNotification>| super::keep(None));
        // SAFETY: UIKit's constant names; no object to match; the main
        // queue runs the blocks on the main thread; the blocks capture
        // nothing, so they may go to any thread. The center keeps both
        // observers, which are never removed, for as long as Sue runs.
        unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(UIKeyboardWillChangeFrameNotification),
                None,
                Some(&queue),
                &moved,
            );
            center.addObserverForName_object_queue_usingBlock(
                Some(UIKeyboardWillHideNotification),
                None,
                Some(&queue),
                &hidden,
            );
        }
    }

    /// Where the keyboard ends up, in the screen's points.
    fn end_frame(notification: &NSNotification) -> Option<CGRect> {
        // SAFETY: UIKit's constant key.
        let key = unsafe { UIKeyboardFrameEndUserInfoKey };
        let value = notification
            .userInfo()?
            .objectForKey(key)?
            .downcast::<NSValue>()
            .ok()?;
        // SAFETY: checked just before to hold a CGRect.
        value
            .contains_encoding::<CGRect>()
            .then(|| unsafe { value.get::<CGRect>() })
    }
}
