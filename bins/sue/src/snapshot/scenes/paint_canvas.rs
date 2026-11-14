//! Scenes of Sue Paint's canvas: the picture, the launchers under a
//! drawing and a 3D figure.

use super::super::Tour;

/// Scene 08. Sue Paint with a sample illustration.
pub(super) fn sample(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.runtime
        .with_context(&mut tour.camera.painter, tour.camera.now, |cx| {
            sue_paint::sample_picture(cx, 1600, 1200)
        });
    tour.shoot("08-sue-paint")
}

/// 08j and 08k. Clip Studio Paint's launchers: the Object launcher under
/// the layer's drawing with the Operation tool, and the Transform launcher
/// under Scale/Rotate's box.
pub(super) fn launchers(tour: &mut Tour<'_>) -> Result<(), String> {
    for (command, name) in [
        ("tool.operation", "08j-sue-paint-object-launcher"),
        ("edit.transform.scale", "08k-sue-paint-transform-launcher"),
    ] {
        tour.paint_command(command);
        tour.shoot(name)?;
    }
    for command in ["edit.transform.cancel", "tool.pen"] {
        tour.paint_command(command);
    }
    Ok(())
}

/// 08o. A 3D drawing figure on its own layer, its joints shown.
pub(super) fn figure(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("layer.new.figure");
    tour.paint_command("figure.preset.2");
    tour.shoot("08o-sue-paint-figure")?;
    tour.paint_command("edit.undo");
    tour.paint_command("edit.undo");
    Ok(())
}
