//! The built-in applets, installed into the Launchpad at runtime.
//! Packages add more through the same `Launchpad::register` call; each
//! applet's manifest decides where it shows up (canvas modes, spawn buttons,
//! timeline lanes, rails) and in which order.

use sue_wm::{Launchpad, WmError};

use crate::audio_node::AudioNode;
use crate::convert::Convert;
use crate::document::Document;
use crate::files::Files;
use crate::hex::Hex;
use crate::history::History;
use crate::image::Image;
use crate::inspector::Inspector;
use crate::notes::Notes;
use crate::player::Player;
use crate::settings::Settings;

/// A launchpad with every built-in applet installed.
///
/// # Errors
/// If two applets share an id (a build error).
pub fn launchpad() -> Result<Launchpad, WmError> {
    let mut launchpad = Launchpad::new();
    // Entities on the canvas.
    launchpad.register(|| Box::new(Document))?;
    launchpad.register(|| Box::new(AudioNode))?;
    launchpad.register(|| Box::new(Image::default()))?;
    launchpad.register(|| Box::new(Hex::default()))?;
    launchpad.register(|| Box::new(sue_paint::PictureCard::default()))?;
    launchpad.register(|| Box::new(sue_writer::WriterCard))?;
    // Left drawer.
    launchpad.register(|| Box::new(Files::default()))?;
    launchpad.register(|| Box::new(Player::default()))?;
    launchpad.register(|| Box::new(Convert::default()))?;
    launchpad.register(|| Box::new(Notes::default()))?;
    // Right drawer.
    launchpad.register(|| Box::new(Inspector::default()))?;
    launchpad.register(|| Box::new(History))?;
    // Windows.
    launchpad.register(|| Box::new(Settings::default()))?;
    // Studios.
    launchpad.register(|| Box::new(sue_paint::SuePaint::default()))?;
    launchpad.register(|| Box::new(sue_writer::SueWriter::default()))?;
    Ok(launchpad)
}
