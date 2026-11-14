//! Scenes of Sue Paint's menus, dialogs and galleries.

use super::super::Tour;

/// 08l and 08m. Quick Filters and Quick Effects: their galleries.
/// 08n. Advanced Fill.
pub(super) fn galleries(tour: &mut Tour<'_>) -> Result<(), String> {
    for (command, name) in [
        ("filter.quick.layer", "08l-sue-paint-quick-filters"),
        ("filter.effects.layer", "08m-sue-paint-quick-effects"),
        ("edit.advanced_fill", "08n-sue-paint-advanced-fill"),
    ] {
        tour.paint_command(command);
        tour.shoot(name)?;
        tour.paint_go(&["dialog/close"]);
    }
    Ok(())
}

/// Scene 10. The Layer menu; 11. New, 11b. Pen Pressure, 11e. Preferences
/// and 11f. Manage Fonts.
pub(super) fn menu_and_dialogs(tour: &mut Tour<'_>) -> Result<(), String> {
    for (target, name) in [
        ("menu/Layer", "10-sue-paint-menu"),
        ("dialog/new", "11-sue-paint-new"),
        ("dialog/pressure", "11b-sue-paint-pressure"),
        ("dialog/preferences", "11e-sue-paint-preferences"),
    ] {
        tour.paint_go(&["dialog/close", target]);
        tour.shoot(name)?;
    }
    // 11f. Manage Fonts: Sue's shared font list.
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("file.fonts");
    tour.shoot("11f-sue-paint-manage-fonts")?;
    Ok(())
}

/// 11c. The Page Manager of a manga of five pages.
pub(super) fn pages(tour: &mut Tour<'_>) -> Result<(), String> {
    for command in [
        "story.add",
        "story.add",
        "story.add",
        "story.add",
        "story.first",
    ] {
        tour.paint_command(command);
    }
    tour.paint_go(&["dialog/close", "dialog/pages"]);
    tour.shoot("11c-sue-paint-pages")?;
    tour.paint_go(&["dialog/close"]);
    Ok(())
}
