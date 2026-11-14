//! Scenes of Sue Paint on a phone and on a tablet.

use sue_rice::RiceValue;

use super::super::{Tour, online};
use super::open_paint;

/// Sue Paint on a touch screen: its start page, Simple Mode and Studio
/// Mode. `prefix` starts every picture's name (`20-phone`, `22-tablet`).
pub(super) fn touch(tour: &mut Tour<'_>, prefix: &str) -> Result<(), String> {
    open_paint(tour);
    tour.shoot(&format!("{prefix}-sue-paint"))?;
    // The start page: Projects, the New canvas page (Illustration and
    // Comic), the materials, and Settings and Help as sheets.
    for (targets, name) in [
        (&["start/show"][..], "paint-start"),
        (&["start/new"], "paint-new-canvas"),
        (&["start/new/2"], "paint-new-comic"),
        (
            &["start/hide", "start/show", "start/materials"],
            "paint-materials",
        ),
        (&["start/projects", "start/settings"], "paint-settings"),
        (&["start/close", "start/help"], "paint-help"),
    ] {
        tour.paint_go(targets);
        tour.shoot(&format!("{prefix}-{name}"))?;
    }
    // Browse Assets, read from the site.
    if online() {
        tour.paint_go(&["start/close", "start/browse"]);
        tour.wait_for_the_network();
        tour.shoot(&format!("{prefix}-paint-browse"))?;
        tour.paint_go(&["start/projects"]);
    }
    tour.paint_go(&["start/hide"]);
    // Simple Mode: the canvas, then its "..." menu.
    tour.shoot(&format!("{prefix}-paint-simple"))?;
    tour.paint_go(&["sheet/more"]);
    tour.shoot(&format!("{prefix}-paint-simple-more"))?;
    tour.paint_go(&["sheet/close"]);
    // Studio Mode: the canvas, its modifier key bar and its Menu.
    tour.set(
        sue_paint::simple::MODE_KEY,
        RiceValue::Text("studio".to_owned()),
    );
    tour.shoot(&format!("{prefix}-paint-studio"))?;
    for (target, name) in [
        ("keys/open", "paint-studio-keys"),
        ("sheet/menu", "paint-studio-menu"),
    ] {
        tour.paint_go(&["sheet/close", target]);
        tour.shoot(&format!("{prefix}-{name}"))?;
    }
    tour.paint_go(&["sheet/close"]);
    tour.set(
        sue_paint::simple::MODE_KEY,
        RiceValue::Text("simple".to_owned()),
    );
    Ok(())
}
