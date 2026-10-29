//! The built-in applets, installed into the Launchpad at runtime.
//! Packages add more through the same `Launchpad::register` call; each
//! applet's manifest decides where it shows up (Studios or Tools in the Hub
//! and the Sue menu, or a window) and in which order.

use sue_wm::{Launchpad, WmError};

use crate::convert::Convert;
use crate::settings::Settings;
use crate::viewer::Viewer;

/// A launchpad with every built-in applet installed.
///
/// # Errors
/// If two applets share an id (a build error).
pub fn launchpad() -> Result<Launchpad, WmError> {
    let mut launchpad = Launchpad::new();
    // Studios.
    launchpad.register(|| Box::new(sue_paint::SuePaint::default()))?;
    launchpad.register(|| Box::new(sue_writer::SueWriter::default()))?;
    // Tools.
    launchpad.register(|| Box::new(Viewer::default()))?;
    launchpad.register(|| Box::new(Convert::default()))?;
    // Windows.
    launchpad.register(|| Box::new(Settings::default()))?;
    Ok(launchpad)
}
