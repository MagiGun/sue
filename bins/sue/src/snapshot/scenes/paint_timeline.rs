//! Scenes of Sue Paint's timeline and animation cels.

use super::super::Tour;

/// 11d. The Timeline of a page: two cels, onion skin on. 11d2. The
/// Animation cels tab, as Clip Studio Paint's palette. 11t1 to 11t7. The
/// timeline's dialogs: Change Settings, Insert Frame, the movie, image
/// sequence, animated GIF and cels exports, Onion Skin Settings. The
/// layer palette is left on the cels tab for the scene after.
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
    tour.shoot("11d2-sue-paint-animation-cels")?;
    for (command, name) in [
        ("anim.timeline.settings", "11t1-sue-paint-timeline-settings"),
        ("anim.frame.insert", "11t2-sue-paint-insert-frame"),
        ("file.export.movie", "11t3-sue-paint-movie-export"),
        ("file.export.sequence", "11t4-sue-paint-sequence-export"),
        ("file.export.gif", "11t5-sue-paint-gif-export"),
        ("file.export.cels", "11t6-sue-paint-cels-export"),
        ("anim.onion_settings", "11t7-sue-paint-onion-settings"),
    ] {
        tour.paint_command(command);
        tour.shoot(name)?;
        tour.paint_go(&["dialog/close"]);
    }
    Ok(())
}
