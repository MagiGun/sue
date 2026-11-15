//! Scenes of the palettes that keep work at hand: Navigator, Sub View,
//! History, Auto Action, Information, Item Bank and Material. The tour
//! calls this file where Sue Paint has a picture open and its palettes are
//! at rest; each palette is shown where the layout has it, and shot.

use super::super::Tour;

/// 46a to 46g.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    for (key, name) in [
        ("navigator", "46a-sue-paint-navigator"),
        ("sub_view", "46b-sue-paint-sub-view"),
        ("history", "46c-sue-paint-history"),
        ("auto_action", "46d-sue-paint-auto-action"),
        ("info", "46e-sue-paint-information"),
        ("item_bank", "46f-sue-paint-item-bank"),
        ("material", "46g-sue-paint-material"),
    ] {
        tour.paint_go(&[format!("extras/show/{key}").as_str()]);
        tour.shoot(name)?;
    }
    Ok(())
}
