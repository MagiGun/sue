//! The built-in applets, installed into the Launchpad at runtime (LAW 1).
//! Packages add more through the same `Launchpad::register` call.

use sue_wm::{Launchpad, WmError};

use crate::drop_zone::DropZone;
use crate::hex_inspector::HexInspector;
use crate::image_viewer::ImageViewer;
use crate::quick_notes::QuickNotes;
use crate::settings::Settings;
use crate::text_studio::TextStudio;

/// A launchpad with every built-in applet installed.
///
/// # Errors
/// If two applets share an id (a build error).
pub fn launchpad() -> Result<Launchpad, WmError> {
    let mut launchpad = Launchpad::new();
    launchpad.register(|| Box::new(QuickNotes::default()))?;
    launchpad.register(|| Box::new(DropZone::default()))?;
    launchpad.register(|| Box::new(Settings::default()))?;
    launchpad.register(|| Box::new(TextStudio::default()))?;
    launchpad.register(|| Box::new(ImageViewer::default()))?;
    launchpad.register(|| Box::new(HexInspector::default()))?;
    Ok(launchpad)
}
