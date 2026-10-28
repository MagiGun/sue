//! The built-in applets, installed into the Launchpad at runtime (LAW 1).
//! Packages add more through the same `Launchpad::register` call; the order
//! here is the order of the activity rails.

use sue_wm::{Launchpad, WmError};

use crate::browser::Browser;
use crate::convert::Convert;
use crate::files::Files;
use crate::hex_inspector::HexInspector;
use crate::history::History;
use crate::image_viewer::ImageViewer;
use crate::inspector::Inspector;
use crate::player::Player;
use crate::quick_notes::QuickNotes;
use crate::settings::Settings;
use crate::text_studio::TextStudio;

/// A launchpad with every built-in applet installed.
///
/// # Errors
/// If two applets share an id (a build error).
pub fn launchpad() -> Result<Launchpad, WmError> {
    let mut launchpad = Launchpad::new();
    // Left dock.
    launchpad.register(|| Box::new(Files::default()))?;
    launchpad.register(|| Box::new(Player::default()))?;
    launchpad.register(|| Box::new(Convert::default()))?;
    launchpad.register(|| Box::new(QuickNotes::default()))?;
    // Right dock.
    launchpad.register(|| Box::new(Inspector))?;
    launchpad.register(|| Box::new(Browser::default()))?;
    launchpad.register(|| Box::new(History::default()))?;
    // Center Stage.
    launchpad.register(|| Box::new(TextStudio::default()))?;
    launchpad.register(|| Box::new(ImageViewer::default()))?;
    launchpad.register(|| Box::new(HexInspector::default()))?;
    launchpad.register(|| Box::new(Settings::default()))?;
    Ok(launchpad)
}
