//! Scenes of how the interface fits the window sizes and scales people
//! use most: Sue Paint with its palettes, its dialogs, Settings, the Hub
//! and Sue Writer at 1280 by 720, 1366 by 768, 1920 by 1080 and 2560 by
//! 1440 pixels, at 100, 125 and 150 percent. `SUE_SNAPSHOT_ONLY=fit`
//! runs them alone; `fit-<size>` (such as `fit-1366x768@100`) one size.
//!
//! A size in pixels at a scale is a window of fewer points: 1366 by 768
//! at 150 percent is 911 by 512 points. The pictures are named by the
//! pixels and the scale.

use sue_wm::ShellRequest;

use super::super::Tour;

/// The window sizes in pixels and the scales, as people's displays have
/// them.
pub(super) const SIZES: &[([u32; 2], f32)] = &[
    ([1280, 720], 1.0),
    ([1366, 768], 1.0),
    ([1366, 768], 1.25),
    ([1366, 768], 1.5),
    ([1920, 1080], 1.0),
    ([1920, 1080], 1.25),
    ([1920, 1080], 1.5),
    ([2560, 1440], 1.25),
    ([2560, 1440], 1.5),
];

/// The name of a size: `1366x768@125`.
fn label(pixels: [u32; 2], scale: f32) -> String {
    format!("{}x{}@{}", pixels[0], pixels[1], (scale * 100.0).round())
}

/// Shoots `name` after the dialog or window it shows has come in, then
/// closes it.
fn shoot_dialog(tour: &mut Tour<'_>, name: &str) -> Result<(), String> {
    let path = tour.dir.join(format!("{name}.png"));
    tour.camera.shoot_after(&mut tour.runtime, &path, 40)?;
    tour.taken(path);
    tour.paint_go(&["dialog/close"]);
    tour.runtime.shell.close_window();
    tour.settle();
    Ok(())
}

/// Every size, or only the one `only` names (`fit-1366x768@100`).
pub(super) fn sizes(tour: &mut Tour<'_>, only: &str) -> Result<(), String> {
    let (size, scale) = (tour.camera.size, tour.camera.scale);
    for &(pixels, at) in SIZES {
        let name = label(pixels, at);
        if only != "fit" && only != format!("fit-{name}") {
            continue;
        }
        tour.camera.scale = at;
        tour.camera.size = pixels.map(|side| (side as f32 / at).round() as u32);
        one_size(tour, &name)?;
    }
    tour.camera.size = size;
    tour.camera.scale = scale;
    Ok(())
}

fn one_size(tour: &mut Tour<'_>, name: &str) -> Result<(), String> {
    super::open_paint(tour);
    tour.settle();
    super::paint_at_rest(tour);
    tour.shoot(&format!("fit-{name}-paint"))?;
    for (dialog, command) in [
        ("new", None),
        ("export", Some("file.export.png")),
        ("canvas-properties", Some("edit.canvas_properties")),
        ("timelapse", Some("file.timelapse.export")),
        ("preferences", Some("file.preferences")),
    ] {
        match command {
            Some(command) => tour.paint_command(command),
            None => tour.paint_go(&["dialog/new"]),
        }
        tour.settle();
        shoot_dialog(tour, &format!("fit-{name}-{dialog}"))?;
    }
    tour.request(ShellRequest::OpenSettings(None));
    tour.settle();
    shoot_dialog(tour, &format!("fit-{name}-settings"))?;
    super::to_the_hub(tour);
    tour.settle();
    tour.shoot(&format!("fit-{name}-hub"))?;
    let writer = tour.writer.clone();
    tour.request(ShellRequest::Open(writer));
    tour.settle();
    tour.shoot(&format!("fit-{name}-writer"))?;
    super::to_the_hub(tour);
    tour.settle();
    Ok(())
}
