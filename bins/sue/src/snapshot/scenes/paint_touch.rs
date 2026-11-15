//! Scenes of Sue Paint on a phone and on a tablet.

use sue_rice::RiceValue;
use sue_wm::ShellRequest;

use super::super::Tour;
use super::open_paint;

/// Sue Paint on a touch screen: with no canvas, Simple Mode and Studio
/// Mode. `prefix` starts every picture's name (`20-phone`, `22-tablet`).
pub(super) fn touch(tour: &mut Tour<'_>, prefix: &str) -> Result<(), String> {
    open_paint(tour);
    tour.shoot(&format!("{prefix}-sue-paint"))?;
    // The New canvas page (Illustration and Comic), then the Hub's
    // Materials place.
    for (targets, name) in [
        (&["new-canvas"][..], "paint-new-canvas"),
        (&["new-canvas/2"], "paint-new-comic"),
    ] {
        tour.paint_go(targets);
        tour.shoot(&format!("{prefix}-{name}"))?;
    }
    tour.paint_go(&["new-canvas/close"]);
    tour.request(ShellRequest::HubPlace("materials".to_owned()));
    tour.shoot(&format!("{prefix}-hub-materials"))?;
    open_paint(tour);
    // Simple Mode: the canvas, then its "..." menu.
    tour.shoot(&format!("{prefix}-paint-simple"))?;
    tour.paint_go(&["sheet/more"]);
    tour.shoot(&format!("{prefix}-paint-simple-more"))?;
    tour.paint_go(&["sheet/close", "sheet/pens"]);
    tour.shoot(&format!("{prefix}-paint-simple-pens"))?;
    tour.paint_go(&["sheet/close"]);
    // Studio Mode: the canvas, its modifier key bar and its Menu.
    tour.set(
        sue_paint::simple::MODE_KEY,
        RiceValue::Text("studio".to_owned()),
    );
    tour.shoot(&format!("{prefix}-paint-studio"))?;
    // The edge key bar pinned, see-through, then pinned at the left;
    // the Menu, Tools and Layers sheets.
    for (targets, name) in [
        (&["keys/right", "keys/open"][..], "paint-studio-keys"),
        (&["keys/peek"], "paint-studio-keys-peek"),
        (&["keys/left", "keys/open"], "paint-studio-keys-left"),
        (&["keys/right", "sheet/menu"], "paint-studio-menu"),
        (&["sheet/tools"], "paint-studio-tools"),
        (&["sheet/layers"], "paint-studio-layers"),
    ] {
        tour.paint_go(&["sheet/close"]);
        tour.paint_go(targets);
        tour.shoot(&format!("{prefix}-{name}"))?;
    }
    tour.paint_go(&["sheet/close"]);
    tour.set(
        sue_paint::simple::MODE_KEY,
        RiceValue::Text("simple".to_owned()),
    );
    Ok(())
}
