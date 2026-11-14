//! Scenes of what Sue Paint's View menu shows over the picture.

use super::super::Tour;

/// A few curves and a box, as an SVG drawing, for a vector layer.
const LINES: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1200">
<path d="M300 300 C 500 120, 700 480, 900 300" fill="none" stroke="#202020" stroke-width="8"/>
<path d="M300 520 C 520 700, 760 380, 1050 620" fill="none" stroke="#202020" stroke-width="8"/>
<path d="M420 780 L 1180 780 L 1180 960 L 420 960 Z" fill="none" stroke="#202020" stroke-width="6"/>
</svg>"##;

/// 31a. View, Safety Margin on a page, and Show Vector Paths with every
/// control point on a vector layer of lines. 31c. The View menu with
/// them on. 31b. Show Playback FPS while the timeline plays. Everything
/// is turned back off and the layer deleted after, so the scenes after
/// look as they did.
pub(super) fn marks(tour: &mut Tour<'_>) -> Result<(), String> {
    let picture = tour
        .runtime
        .applets
        .get(&tour.paint)
        .and_then(|applet| applet.focus())
        .ok_or("Sue Paint has a picture")?;
    let made = tour
        .runtime
        .with_context(&mut tour.camera.painter, tour.camera.now, |cx| {
            let arena = &cx.world.arena;
            let size = arena
                .get_by_id(picture)
                .ok()
                .and_then(sue_model::picture::size)
                .unwrap_or([1, 1]);
            let drawing = sue_io::svg::parse_svg(LINES)?;
            let lines = sue_paint::vector::from_svg(&drawing, size);
            let (spawn, layer) =
                sue_paint::doc::add_vector_lines(arena, picture, None, "Lines".to_owned(), &lines)?;
            cx.apply(sue_wm::ArenaEdit(
                sue_paint::doc::Edit::new("Lines").then(spawn),
            ))
            .map(|_| layer)
            .map_err(|error| error.to_string())
        });
    let layer = made?;
    let marks = [
        "view.safety_margin",
        "view.vector_paths.all",
        "view.vector_paths.points",
    ];
    for command in marks {
        tour.paint_command(command);
    }
    tour.shoot("31a-sue-paint-view-marks")?;
    tour.paint_go(&["menu/View"]);
    tour.shoot("31c-sue-paint-view-menu")?;
    tour.paint_go(&["dialog/close"]);
    for command in marks {
        tour.paint_command(command);
    }
    tour.paint_command("view.playback_fps");
    tour.paint_command("anim.play");
    let path = tour.dir.join("31b-sue-paint-playback-fps.png");
    tour.camera.shoot_after(&mut tour.runtime, &path, 90)?;
    tour.written.push(path);
    tour.paint_command("anim.play");
    tour.paint_command("view.playback_fps");
    // Back on the fourth frame, where the timeline scene left it.
    for command in ["anim.first", "anim.next", "anim.next", "anim.next"] {
        tour.paint_command(command);
    }
    // Deleted rather than undone, so the scenes after have nothing to
    // redo, as before.
    tour.runtime
        .with_context(&mut tour.camera.painter, tour.camera.now, |cx| {
            cx.apply(sue_wm::ArenaEdit(sue_paint::doc::delete(layer)))
                .map(|_| ())
                .map_err(|error| error.to_string())
        })?;
    tour.settle();
    Ok(())
}
