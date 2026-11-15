//! Scenes of Sue Paint's vector lines: picked lines in their box with
//! Tool Property's rows for them, and the Figure tool's Vector magnet.

use sue_rice::RiceValue;

use super::super::Tour;

/// 36a and 36b. The sample illustration with a vector layer of a few
/// lines: the Figure tool's rows with Vector magnet (36a); then every
/// line picked by Select Vectors Within Area, their box with its eight
/// handles and Tool Property's Editing vector rows (36b). It runs last:
/// its steps stay in Sue Paint's history.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    let size = tour.camera.size;
    tour.camera.size = [1440, 900];
    tour.set("display.interface", RiceValue::Text("auto".to_owned()));
    super::open_paint(tour);
    tour.runtime
        .with_context(&mut tour.camera.painter, tour.camera.now, |cx| {
            sue_paint::sample_picture(cx, 1600, 1200)
        });
    tour.settle();
    tour.paint_command("layer.new.vector");
    tour.settle();
    let Some(picture) = tour
        .runtime
        .applets
        .get(&tour.paint)
        .and_then(|applet| applet.focus())
    else {
        return Err("Sue Paint has no picture open".to_owned());
    };
    let arena = &mut tour.runtime.engine.world.arena;
    let Some(layer) = sue_paint::doc::layers(arena, picture)
        .last()
        .map(|info| info.id)
    else {
        return Err("the vector layer is missing".to_owned());
    };
    let Some([width, height]) = arena
        .get_by_id(picture)
        .ok()
        .and_then(sue_model::picture::size)
    else {
        return Err("the picture has no size".to_owned());
    };
    let (w, h) = (width as f32, height as f32);
    let tip = sue_paint::brush::Tip {
        size: 9.0,
        ..sue_paint::brush::Tip::default()
    };
    let ink = [[0.12, 0.12, 0.14], [1.0, 1.0, 1.0]];
    let round: Vec<[f32; 2]> = (0..=96)
        .map(|step| {
            let t = step as f32 / 96.0 * std::f32::consts::TAU;
            [w * 0.42 + w * 0.12 * t.cos(), h * 0.45 + h * 0.14 * t.sin()]
        })
        .collect();
    let wave: Vec<[f32; 2]> = (0..=60)
        .map(|step| {
            let t = step as f32 / 60.0;
            [
                w * 0.3 + w * 0.36 * t,
                h * 0.7 + h * 0.04 * (t * std::f32::consts::TAU * 2.0).sin(),
            ]
        })
        .collect();
    let lines = vec![
        sue_paint::vector::figure_line(&round, tip, ink, false),
        sue_paint::vector::figure_line(&wave, tip, ink, false),
        sue_paint::vector::figure_line(
            &[
                [w * 0.58, h * 0.28],
                [w * 0.7, h * 0.28],
                [w * 0.7, h * 0.4],
            ],
            tip,
            ink,
            true,
        ),
    ];
    let old = sue_paint::vector::geometry(arena, layer);
    if let Some(raster) = sue_paint::doc::raster_mut(arena, layer)
        && let Some(painted) = sue_paint::vector::render(&lines, raster.width(), raster.height())
        && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
    {
        into.copy_from_slice(from);
    }
    let _ = sue_paint::vector::keep(arena, layer, old, &lines, None, "scene");
    tour.paint_go(&["tab/tool/1"]);
    tour.paint_command("tool.pick.figure");
    tour.shoot("36a-sue-paint-figure-vector-magnet")?;
    for command in ["select.all", "select.vectors_within", "select.none"] {
        tour.paint_command(command);
    }
    tour.shoot("36b-sue-paint-vector-lines")?;
    tour.paint_command("tool.pen");
    tour.settle();
    tour.camera.size = size;
    Ok(())
}
