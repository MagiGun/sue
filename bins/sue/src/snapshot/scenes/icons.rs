//! Scenes of the icon set: its contact sheets. They are not part of
//! Sue's interface; they are the pictures the icons are reviewed on, in
//! the theme and at the scale of the run.

use sue_wm::icons::sheet::{self, Sheet};
use sue_wm::icons::{Icon, REFERENCE};

use super::super::Tour;

/// The size of a contact sheet, in points.
const SIZE: [u32; 2] = [1440, 900];

/// Scene 27: the twelve reference icons, a card each. Scenes 27a and
/// on: every icon of the set, small, on as many sheets as it takes.
pub(super) fn contact_sheets(tour: &mut Tour<'_>) -> Result<(), String> {
    let before = tour.camera.size;
    tour.camera.size = SIZE;
    tour.shoot_drawing("27-icons-reference", |painter, rect, theme| {
        sheet::paint(painter, rect, theme, Sheet::Cards, &REFERENCE);
    })?;
    let whole =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SIZE[0] as f32, SIZE[1] as f32));
    let pages = Icon::ALL.chunks(Sheet::Grid.capacity(whole));
    for (letter, icons) in ('a'..='z').zip(pages) {
        tour.shoot_drawing(&format!("27{letter}-icons-all"), |painter, rect, theme| {
            sheet::paint(painter, rect, theme, Sheet::Grid, icons);
        })?;
    }
    tour.camera.size = before;
    Ok(())
}
