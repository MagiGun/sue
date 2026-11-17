//! Scenes of Sue Paint's 3D drawing figures (0.0.81): the male and the
//! female body, Tool Property at the model chooser, at the body shape by
//! part and at the pose rows with the hand setup, a posed body, and the
//! hand close up with its veins and tendons at their least and most.

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
    wheel(tour, over, 400.0, 12);
    wheel(tour, over, -60.0, ticks);
    tour.shoot(name)?;
    wheel(tour, over, 400.0, 12);
    Ok(())
}

/// 81a to 81i. The drawing figures.
pub(super) fn figures(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("layer.new.figure");
    tour.paint_go(&["tab/tool/1"]);
    tour.shoot("81a-sue-paint-figure-male")?;
    scrolled(tour, "81b-sue-paint-figure-model-chooser", 20)?;
    scrolled(tour, "81c-sue-paint-figure-body-shape-by-part", 33)?;
    scrolled(tour, "81d-sue-paint-figure-pose-and-hand-setup", 47)?;
    tour.paint_command("figure.model.1");
    tour.paint_command("figure.preset.7");
    tour.shoot("81e-sue-paint-figure-female-posed")?;
    tour.paint_command("figure.preset.12");
    tour.shoot("81f-sue-paint-figure-crouch")?;
    // The hand, close up: smooth, then with its veins, tendons and
    // knuckles at their most.
    tour.paint_command("figure.model.2");
    tour.paint_command("figure.slider.veins=0");
    tour.paint_command("figure.slider.prominence=0");
    tour.shoot("81g-sue-paint-hand-veins-least")?;
    tour.paint_command("figure.slider.veins=1");
    tour.paint_command("figure.slider.prominence=1");
    tour.shoot("81h-sue-paint-hand-veins-most")?;
    tour.paint_command("figure.preset.6");
    tour.shoot("81i-sue-paint-hand-holding")?;
    // Back as the scenes after expect: every step of this scene undone.
    for _ in 0..12 {
        tour.paint_command("edit.undo");
    }
    tour.settle();
    Ok(())
}
