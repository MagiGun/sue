//! Scenes of Sue Paint's filter dialogs and Edit's Colorize (36a to 36h).
//!
//! The dialogs preview on a clear layer made for them, so the sample
//! illustration stays as the other scenes show it; the layer goes again
//! after.

use super::super::Tour;

/// 36a to 36h: Gradient Map, Level Correction, Artistic, Radial Blur,
/// Perlin Noise's settings, Curved Surface, Smart Smoothing and Colorize.
pub(super) fn dialogs(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("layer.new");
    for (command, name) in [
        ("filter.gradient_map", "36a-sue-paint-gradient-map"),
        ("filter.levels", "36b-sue-paint-level-correction"),
        ("filter.artistic", "36c-sue-paint-artistic"),
        ("filter.radial_blur", "36d-sue-paint-radial-blur"),
        ("filter.curved_surface", "36f-sue-paint-curved-surface"),
        ("edit.smart_smoothing", "36g-sue-paint-smart-smoothing"),
        ("edit.colorize", "36h-sue-paint-colorize"),
    ] {
        tour.paint_command(command);
        tour.shoot(name)?;
        tour.paint_go(&["dialog/close"]);
    }
    // Perlin Noise fills its layer: shot last, on the clear layer, which
    // then goes.
    tour.paint_command("filter.perlin");
    tour.shoot("36e-sue-paint-perlin-noise")?;
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("edit.undo");
    Ok(())
}
