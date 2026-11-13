//! Scenes of Sue Writer on a phone and on a tablet.

use super::super::Tour;

/// Sue Writer on a touch screen: the novel, the start page, and the page
/// on its side; on a phone (`typing`) also the page with the on-screen
/// keyboard up. `prefix` starts every picture's name.
pub(super) fn touch(tour: &mut Tour<'_>, prefix: &str, typing: bool) -> Result<(), String> {
    tour.open_novel()?;
    tour.shoot(&format!("{prefix}-sue-writer"))?;
    tour.writer_go(&["start"]);
    tour.shoot(&format!("{prefix}-sue-writer-start"))?;
    tour.open_novel()?;
    let upright = tour.camera.size;
    tour.camera.size = [upright[1], upright[0]];
    tour.shoot(&format!("{prefix}-sue-writer-turned"))?;
    tour.camera.size = upright;
    if typing {
        // Writing with the on-screen keyboard up: the window above it
        // keeps the caret's line and the formatting in sight.
        tour.camera.size = [430, 520];
        tour.writer_go(&["text/end"]);
        tour.shoot(&format!("{prefix}-sue-writer-typing"))?;
    }
    Ok(())
}
