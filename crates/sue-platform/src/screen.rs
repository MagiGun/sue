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
    /// waits and shown again after. `wake` is called, from another
    /// thread, whenever [`ScreenPick::state`] changes, so the interface
    /// draws again.
    ///
    /// # Errors
    /// [`ScreenError::Unsupported`] where there is no route.
    pub fn start(hide: bool, wake: Box<dyn Fn() + Send + Sync>) -> Result<Self, ScreenError> {
        #[cfg(target_os = "windows")]
        {
            Ok(Self {
                pick: sue_windows::screen::Pick::start(hide, wake),
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (hide, wake);
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
