//! Scenes of File, Save in Sue Paint on a canvas that came from another
//! format: the one question it asks.

use sue_rice::RiceValue;
use sue_wm::{FilePick, ShellRequest};

use super::super::Tour;

/// 35a. Save on a canvas opened from a `.clip`: export back, or keep in
/// a Sue project. 35b. Save on a canvas from a PNG that has two layers by
/// then: the layers warning.
pub(super) fn questions(tour: &mut Tour<'_>) -> Result<(), String> {
    let size = tour.camera.size;
    tour.camera.size = [1440, 900];
    tour.set("display.interface", RiceValue::Text("auto".to_owned()));
    super::open_paint(tour);
    let dir = tour.home.join("Documents");
    let clip = dir.join("Comic page.clip");
    let mut doc = sue_io::clip::ClipDocument::new(800, 600, 350.0);
    doc.root.children.push(sue_io::clip::ClipLayer::paper(
        "Paper",
        [255, 255, 255],
        800,
        600,
    ));
    let ink = [30_u8, 30, 40, 255].repeat(400 * 300);
    doc.root.children.push(sue_io::clip::ClipLayer::raster(
        "Ink",
        200,
        150,
        sue_io::clip::ClipBitmap::color(400, 300, &ink),
    ));
    let bytes = sue_io::clip::write(&doc)?;
    std::fs::write(&clip, bytes).map_err(|error| error.to_string())?;
    let png = dir.join("Sketch.png");
    let tile = sue_core::RasterTile2D::from_bytes(
        800,
        600,
        sue_core::PixelFormat::Rgba8,
        [240_u8, 230, 210, 255].repeat(800 * 600),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(&png, sue_io::encode_png(&tile)?).map_err(|error| error.to_string())?;
    let paint = tour.paint.clone();
    for (path, name) in [
        (&clip, "35a-sue-paint-save-clip"),
        (&png, "35b-sue-paint-save-merge"),
    ] {
        tour.request(ShellRequest::Picked {
            pick: FilePick::open(Some(paint.clone()), "open", "Open"),
            path: path.clone(),
        });
        tour.settle();
        if name.ends_with("merge") {
            tour.paint_command("layer.new");
            tour.settle();
        }
        tour.paint_command("file.save");
        tour.settle();
        // The question fades in on egui's clock, which the shell's
        // pacing does not wait for: let it finish.
        let path = tour.dir.join(format!("{name}.png"));
        tour.camera.shoot_after(&mut tour.runtime, &path, 40)?;
        tour.written.push(path);
        tour.paint_go(&["dialog/close"]);
        tour.settle();
    }
    tour.camera.size = size;
    Ok(())
}
