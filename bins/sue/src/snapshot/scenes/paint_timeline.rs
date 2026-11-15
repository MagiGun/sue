//! Scenes of Sue Paint's timeline and animation cels.

use super::super::Tour;

/// 11d. The Timeline of a page: two cels, onion skin on. 11d1. The
/// frames held by the keyboard. 11d2. The Animation Cels palette in a
/// column beside the Layer palette, a traced layer's entry picked. 11t1 to 11t7. The
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
        "anim.onion",
    ] {
        tour.paint_command(command);
    }
    tour.shoot("11d-sue-paint-timeline")?;
    // 11d1. The frames hold the keyboard: a click on frame 4 of the
    // folder's row (where the timeline of 11d draws it), then Right:
    // frame 5 shows, with the focus ring. Left goes back.
    let key = |key| {
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]
    };
    // The Layer palette's tab in front (History's list would take the
    // keyboard as it comes into sight).
    tour.paint_go(&["dock/menu/layer", "dock/menu"]);
    tour.settle();
    tour.camera.click(egui::pos2(657.0, 868.0));
    tour.settle();
    tour.camera.input.push(key(egui::Key::ArrowRight));
    tour.shoot("11d1-sue-paint-timeline-keyboard")?;
    tour.camera.input.push(key(egui::Key::ArrowLeft));
    tour.settle();
    // Cel 1 on the light table of cel 2 (cel 2 locked as the one drawn
    // while frame 1 picks cel 1), so the Animation Cels palette shows an
    // entry, picked, its settings at the foot.
    for command in [
        "anim.cel.lock",
        "anim.first",
        "anim.light_table.register",
        "anim.cel.lock",
        "anim.next",
        "anim.next",
        "anim.next",
    ] {
        tour.paint_command(command);
    }
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
