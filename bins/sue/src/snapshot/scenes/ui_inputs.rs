//! Scenes of the inputs and sliders of `sue_wm::ui`: their review sheets.
//! They are not part of Sue's interface; they are the pictures the
//! components are reviewed on, in the theme and at the scale of the run.

use sue_wm::ui::{input, slider};

use super::super::Tour;

/// The size of a review sheet, in points.
const SIZE: [u32; 2] = [1440, 900];

/// The size of the sheet of open lists, whose Touch column needs the
/// height of three lists of 44 point rows.
const TALL: [u32; 2] = [1440, 1320];

/// Scenes 28c to 28g: the inputs in every state, their open lists, the
/// inputs live in the three densities, then the same for the sliders.
pub(super) fn sheets(tour: &mut Tour<'_>) -> Result<(), String> {
    let before = tour.camera.size;
    for (name, page, size) in [
        ("28c-ui-inputs-states", input::Sheet::States, SIZE),
        ("28d-ui-inputs-open", input::Sheet::Open, TALL),
        ("28e-ui-inputs-densities", input::Sheet::Densities, SIZE),
    ] {
        tour.camera.size = size;
        tour.shoot_drawing(name, |painter, rect, theme| {
            input::sheet(painter, rect, theme, page);
        })?;
    }
    tour.camera.size = SIZE;
    for (name, page) in [
        ("28f-ui-sliders-states", slider::Sheet::States),
        ("28g-ui-sliders-densities", slider::Sheet::Densities),
    ] {
        tour.shoot_drawing(name, |painter, rect, theme| {
            slider::sheet(painter, rect, theme, page);
        })?;
    }
    tour.camera.size = before;
    Ok(())
}
