//! The colour of the screen under the pointer, for Edit, Obtain Screen
//! Color: a pick that waits for a click anywhere on any screen (also
//! outside Sue's window), and the same with Sue's windows hidden while
//! it waits.
//!
//! - Windows: the audited `sue-windows` crate, module `screen`.
//! - Elsewhere there is no route yet ([`available`] is `false`, and the
//!   commands stay grayed with a note): X11 is P8-1's of the v0.0.8 plan;
//!   Wayland, phones and the browser let no program read the screen.

/// Why a screen colour could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenError {
    /// This system gives programs no way to read the screen.
    Unsupported,
    /// The point lies outside every screen.
    OffScreen,
    /// The system would not give the pixel.
    Capture,
    /// The picker could not start.
    Failed(String),
}

/// Where a [`ScreenPick`] has got to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickState {
    /// Waiting for the click; the colour under the pointer once known.
    Waiting(Option<[u8; 3]>),
    /// Clicked: the colour there, sRGB.
    Picked([u8; 3]),
    /// Cancelled (right click, Esc, or [`ScreenPick::cancel`]).
    Cancelled,
    /// Something failed; nothing was picked.
    Failed(ScreenError),
}

/// Whether this system can pick a screen colour.
#[must_use]
pub fn available() -> bool {
    cfg!(target_os = "windows")
}

/// The sRGB colour of the screen pixel at `x`, `y`, in the system's
/// screen pixels.
///
/// # Errors
/// [`ScreenError::Unsupported`] where there is no route, else
/// [`ScreenError::OffScreen`] or [`ScreenError::Capture`]. Never panics.
pub fn pixel_at(x: i32, y: i32) -> Result<[u8; 3], ScreenError> {
    #[cfg(target_os = "windows")]
    {
        sue_windows::screen::pixel_at(x, y).map_err(from_windows)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (x, y);
        Err(ScreenError::Unsupported)
    }
}

#[cfg(target_os = "windows")]
fn from_windows(error: sue_windows::screen::ScreenError) -> ScreenError {
    use sue_windows::screen::ScreenError as Win;
    match error {
        Win::OffScreen => ScreenError::OffScreen,
        Win::Capture => ScreenError::Capture,
        Win::Window(text) => ScreenError::Failed(text),
    }
}

/// A screen colour pick under way; dropping it cancels it.
pub struct ScreenPick {
    #[cfg(target_os = "windows")]
    pick: sue_windows::screen::Pick,
}

impl ScreenPick {
    /// Starts a pick. With `hide`, Sue's own windows are hidden while it
    /// waits and shown again after. A label by the pointer reads `title`
    /// and the colour under it. `wake` is called, from another
    /// thread, whenever [`ScreenPick::state`] changes, so the interface
    /// draws again.
    ///
    /// # Errors
    /// [`ScreenError::Unsupported`] where there is no route.
    pub fn start(
        hide: bool,
        title: String,
        wake: Box<dyn Fn() + Send + Sync>,
    ) -> Result<Self, ScreenError> {
        #[cfg(target_os = "windows")]
        {
            Ok(Self {
                pick: sue_windows::screen::Pick::start(hide, title, wake),
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (hide, title, wake);
            Err(ScreenError::Unsupported)
        }
    }

    /// Where the pick has got to.
    #[must_use]
    pub fn state(&self) -> PickState {
        #[cfg(target_os = "windows")]
        {
            use sue_windows::screen::PickState as Win;
            match self.pick.state() {
                Win::Waiting(hover) => PickState::Waiting(hover.map(|(_, rgb)| rgb)),
                Win::Picked(rgb) => PickState::Picked(rgb),
                Win::Cancelled => PickState::Cancelled,
                Win::Failed(error) => PickState::Failed(from_windows(error)),
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            PickState::Cancelled
        }
    }

    /// Ends the pick without a colour.
    pub fn cancel(&self) {
        #[cfg(target_os = "windows")]
        self.pick.cancel();
    }

    /// Acts as a click at screen point `x`, `y`, for tests.
    #[doc(hidden)]
    pub fn click_at(&self, x: i32, y: i32) {
        #[cfg(target_os = "windows")]
        self.pick.click_at(x, y);
        #[cfg(not(target_os = "windows"))]
        let _ = (x, y);
    }

    /// Whether the picker is up and waiting for the click, for tests.
    #[doc(hidden)]
    #[must_use]
    pub fn ready(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.pick.window_up()
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }
}

/// Acts as a click at screen point `x`, `y` on the latest pick still
/// waiting (one Sue's own command started), for tests; `false` when there
/// is none ready.
#[doc(hidden)]
#[must_use]
pub fn click_latest(x: i32, y: i32) -> bool {
    #[cfg(target_os = "windows")]
    {
        sue_windows::screen::click_latest(x, y)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (x, y);
        false
    }
}

/// Whether the latest pick (one Sue's own command started) still waits
/// for its click, for tests; `false` once it ended or where there is none.
#[doc(hidden)]
#[must_use]
pub fn latest_waiting() -> bool {
    #[cfg(target_os = "windows")]
    {
        sue_windows::screen::latest_waiting()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// A solid square of one known colour that Sue shows in a window of its
/// own, in the middle of the main screen, for tests that read the screen
/// (so what they read is what Sue drew, on any desktop). Dropping it takes
/// the window away. Where no route exists it cannot be shown.
#[doc(hidden)]
pub struct ScreenPatch {
    #[cfg(target_os = "windows")]
    patch: sue_windows::screen::Patch,
}

impl ScreenPatch {
    /// Shows a square of `color`, `side` screen pixels wide.
    ///
    /// # Errors
    /// [`ScreenError::Unsupported`] where there is no route, else
    /// [`ScreenError::Failed`] when the window cannot be made.
    pub fn show(color: [u8; 3], side: i32) -> Result<Self, ScreenError> {
        #[cfg(target_os = "windows")]
        {
            sue_windows::screen::Patch::show(color, side)
                .map(|patch| Self { patch })
                .map_err(from_windows)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (color, side);
            Err(ScreenError::Unsupported)
        }
    }

    /// A point inside the square where the screen shows its colour,
    /// waiting up to `wait` for it to be drawn; `None` when none does.
    #[must_use]
    pub fn spot(&self, wait: std::time::Duration) -> Option<[i32; 2]> {
        #[cfg(target_os = "windows")]
        {
            self.patch.spot(wait)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = wait;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_screen_points_answer_an_error() {
        let result = pixel_at(i32::MIN, i32::MIN);
        if available() {
            assert_eq!(result, Err(ScreenError::OffScreen));
        } else {
            assert_eq!(result, Err(ScreenError::Unsupported));
        }
    }
}
