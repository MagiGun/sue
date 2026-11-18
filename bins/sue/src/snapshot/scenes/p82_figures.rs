//! Scenes of Sue Paint's 3D drawing figures (0.0.82): bent joints that
//! keep their flesh, a fist with its thumb wrapped, the face with an
//! expression and its section of Tool Property, the head in the drawn
//! style looking aside, and the hand with every vein.
//! `SUE_SNAPSHOT_ONLY=figures` runs these and the 0.0.81 scenes alone.

use egui::pos2;

use super::super::Tour;
use super::paint_3d::{drawn_at, wheel};

/// Tool Property scrolled `ticks` turns of the wheel down from its top,
/// shot as `name`, then scrolled back.
fn scrolled(tour: &mut Tour<'_>, name: &str, ticks: usize) -> Result<(), String> {
    tour.settle();
    let output = tour.camera.frame(&mut tour.runtime);
    let tab = drawn_at(&output, "Tool Property");
    tour.camera.painter.apply_textures(output.textures_delta);
    let Some(tab) = tab else {
        return tour.shoot(name);
    };
    let over = pos2(tab.center().x, tab.bottom() + 160.0);
    wheel(tour, over, 400.0, 40);
    wheel(tour, over, -60.0, ticks);
    tour.shoot(name)?;
    wheel(tour, over, 400.0, 40);
    Ok(())
}

/// 82a to 82h. The drawing figures of 0.0.82.
pub(super) fn figures(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("layer.new.figure");
    tour.paint_go(&["tab/tool/1"]);
    // Joints that keep their volume: arms up, arms crossed, a crouch,
    // a punch with its fist.
    let mut steps = 1;
    for (preset, name) in [
        (5, "82a-sue-paint-figure-arms-up"),
        (9, "82b-sue-paint-figure-arms-crossed"),
        (12, "82c-sue-paint-figure-crouch"),
        (15, "82d-sue-paint-figure-punch"),
    ] {
        tour.paint_command(&format!("figure.preset.{preset}"));
        tour.shoot(name)?;
        steps += 1;
    }
    // The face: a smile, larger eyes, and its section of Tool Property.
    tour.paint_command("figure.preset.0");
    tour.paint_command("figure.slider.smile=1");
    tour.paint_command("figure.slider.eye_size=0.6");
    steps += 3;
    scrolled(tour, "82e-sue-paint-figure-face-rows", 63)?;
    // The head figure in the drawn style.
    tour.paint_command("figure.model.4");
    tour.paint_command("figure.slider.anime=1");
    tour.paint_command("figure.slider.smile=0.4");
    tour.shoot("82f-sue-paint-head-drawn-style")?;
    // The hand: a fist with its thumb wrapped, then every vein.
    tour.paint_command("figure.model.2");
    tour.paint_command("figure.preset.2");
    tour.shoot("82g-sue-paint-hand-fist")?;
    tour.paint_command("figure.preset.0");
    tour.paint_command("figure.slider.veins=1");
    tour.paint_command("figure.slider.vein_pattern=0.5");
    tour.shoot("82h-sue-paint-hand-veins-pattern")?;
    steps += 8;
    // Back as the scenes after expect: every step of this scene undone.
    for _ in 0..steps {
        tour.paint_command("edit.undo");
    }
    tour.settle();
    Ok(())
}
