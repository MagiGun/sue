//! Scenes of Sue Paint's color palettes and their dialogs. The tour calls
//! this file where Sue Paint has a picture open and its palettes are at
//! rest.

use super::super::Tour;

/// 45a to 45e. Color Settings on the drawing color, with the wheel, the
/// new and current color, hex and the four kinds of numbers; Edit Color
/// Sets, the list of sets and what can be done to them; New Color Set,
/// which asks for the name; the Color Slider palette with its three
/// modes; the Color Set palette. The Color Wheel comes to the front
/// again at the end, as the scenes after these expect it.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    for (command, name) in [
        ("color.settings", "45a-sue-paint-color-settings"),
        ("color.edit_sets", "45b-sue-paint-edit-color-sets"),
        ("color.new_set", "45c-sue-paint-new-color-set"),
    ] {
        tour.paint_command(command);
        tour.shoot(name)?;
        tour.paint_go(&["dialog/close"]);
    }
    // Each color palette is a tab of its own group: a palette comes to
    // the front when it is shown (a Window command on a palette that
    // shows hides it, hence the pairs).
    tour.paint_go(&["tab/color/1"]);
    tour.shoot("45d-sue-paint-color-slider")?;
    for _ in 0..2 {
        tour.paint_command("window.color_set");
    }
    tour.shoot("45e-sue-paint-color-set")?;
    for _ in 0..2 {
        tour.paint_command("window.color_wheel");
    }
    tour.settle();
    Ok(())
}
