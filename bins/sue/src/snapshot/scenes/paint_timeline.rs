//! Scenes of Sue Paint's timeline and animation cels.

use super::super::Tour;

/// 11d. The Timeline of a page: two cels, onion skin on. 11d2. The
/// Animation cels tab, as Clip Studio Paint's palette. The layer palette
/// is left on that tab for the scene after.
pub(super) fn timeline(tour: &mut Tour<'_>) -> Result<(), String> {
    for command in [
        // The Timeline starts folded under the canvas: Window, Timeline
        // opens it.
        "window.timeline",
        "anim.cel.new",
        "anim.next",
        "anim.next",
        "anim.next",
        "anim.cel.new",
        "anim.layer.folder",
        "anim.onion",
    ] {
        tour.paint_command(command);
    }
    tour.shoot("11d-sue-paint-timeline")?;
    tour.paint_go(&["tab/layer/3"]);
    tour.shoot("11d2-sue-paint-animation-cels")
}
