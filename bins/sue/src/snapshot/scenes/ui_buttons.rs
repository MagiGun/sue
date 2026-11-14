//! Scenes of the buttons and choices of `sue_wm::ui`: their review
//! sheets. They are not part of Sue's interface; they are the pictures
//! the components are judged on, in the theme and at the scale of the run.

use sue_wm::ui::button::Sheet;
use sue_wm::ui::{button, choice};

use super::super::Tour;

/// What draws one page of a review sheet.
type SheetPainter = fn(&egui::Painter, egui::Rect, &sue_wm::Theme, Sheet);

/// The size of a review sheet, in points.
const SIZE: [u32; 2] = [1440, 900];

/// Scenes 28a (buttons) and 28b (choices): first every control in each
/// of its states, then all of them live in the three densities.
pub(super) fn sheets(tour: &mut Tour<'_>) -> Result<(), String> {
    let before = tour.camera.size;
    tour.camera.size = SIZE;
    let pages = [
        ("28a-ui-buttons", button::sheet as SheetPainter),
        ("28b-ui-choices", choice::sheet),
    ];
    for (name, draw) in pages {
        for page in Sheet::ALL {
            tour.shoot_drawing(
                &format!("{name}-{}", page.name()),
                |painter, rect, theme| {
                    draw(painter, rect, theme, page);
                },
            )?;
        }
    }
    tour.camera.size = before;
    Ok(())
}
