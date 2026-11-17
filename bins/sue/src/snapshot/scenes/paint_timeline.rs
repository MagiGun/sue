//! Scenes of Sue Paint's timeline and animation cels.

use super::super::Tour;

/// 11d. The Timeline of a page: two cels, onion skin on. 11d1. The
/// frames held by the keyboard. 11d2. The Animation Cels palette in a
/// column beside the Layer palette, a traced layer's entry picked. 11t1 to 11t7. The
/// timeline's dialogs: Change Settings, Insert Frame, the movie, image
/// sequence, animated GIF and cels exports, Onion Skin Settings. The
/// dialogs show with Sue Paint's palettes at rest and the Timeline open,
/// as the scenes after need it.
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
    // Frame 4 of the folder's row, found from where the row was laid
    // (128 points under its top edge), whatever height it opens at.
    let row = sue_wm::dock::bottom_row(&tour.camera.ctx).ok_or("the Timeline is open")?;
    tour.camera.click(egui::pos2(657.0, row.top() + 128.0));
    tour.settle();
    tour.camera.input.push(key(egui::Key::ArrowRight));
    tour.shoot("11d1-sue-paint-timeline-keyboard")?;
    tour.camera.input.push(key(egui::Key::ArrowLeft));
    tour.settle();
    tour.paint_command("anim.track_note");
    tour.settle();
    tour.camera
        .input
        .push(vec![egui::Event::Text("Walk cycle".to_owned())]);
    tour.shoot("11d1a-sue-paint-track-label-dialog")?;
    tour.camera.input.push(key(egui::Key::Enter));
    tour.settle();
    tour.shoot("11d1b-sue-paint-track-label")?;
    tour.paint_command("edit.undo");
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
    // The cels palette 11d2 brought out goes, so the dialogs after show
    // over the timeline alone; the Timeline is opened again.
    super::paint_at_rest(tour);
    tour.paint_command("window.timeline");
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
