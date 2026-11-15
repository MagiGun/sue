//! Scenes of Sue Paint's files: the export settings dialog over the
//! canvas, Batch Convert Pictures with a folder in it, and Export
//! Animation, Audio.

use sue_rice::RiceValue;
use sue_wm::{FilePick, ShellRequest};

use super::super::Tour;

/// Shoots `name` once the dialog in front has faded in, then closes it.
fn shoot_dialog(tour: &mut Tour<'_>, name: &str) -> Result<(), String> {
    let path = tour.dir.join(format!("{name}.png"));
    tour.camera.shoot_after(&mut tour.runtime, &path, 40)?;
    tour.taken(path);
    tour.paint_go(&["dialog/close"]);
    tour.settle();
    Ok(())
}

/// 36a. File, Export (Single Layer), .jpg: the export settings dialog
/// over a canvas. 36b. Save Duplicate, .ico: the note on the icon's
/// largest side. 36c. File, Batch Convert Pictures with a folder of
/// three pictures waiting. 36d. File, Export Animation, Audio on a
/// timeline with a sound.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    let size = tour.camera.size;
    tour.camera.size = [1440, 900];
    tour.set("display.interface", RiceValue::Text("auto".to_owned()));
    super::open_paint(tour);
    let dir = tour.home.join("Documents").join("Scans");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let page = |shade: u8| -> Result<Vec<u8>, String> {
        let tile = sue_core::RasterTile2D::from_bytes(
            120,
            160,
            sue_core::PixelFormat::Rgba8,
            [shade, shade, shade.saturating_sub(20), 255].repeat(120 * 160),
        )
        .map_err(|error| error.to_string())?;
        sue_io::encode_png(&tile)
    };
    for (name, shade) in [
        ("Page 1.png", 230),
        ("Page 2.png", 215),
        ("Page 3.png", 200),
    ] {
        std::fs::write(dir.join(name), page(shade)?).map_err(|error| error.to_string())?;
    }
    let paint = tour.paint.clone();
    tour.request(ShellRequest::Picked {
        pick: FilePick::open(Some(paint.clone()), "open", "Open"),
        path: dir.join("Page 1.png"),
    });
    tour.settle();

    tour.paint_command("file.export.jpg");
    tour.settle();
    shoot_dialog(tour, "36a-sue-paint-export-settings")?;
    tour.paint_command("file.duplicate.ico");
    tour.settle();
    shoot_dialog(tour, "36b-sue-paint-save-duplicate-icon")?;

    tour.paint_command("file.batch_convert");
    tour.request(ShellRequest::Picked {
        pick: FilePick::open(Some(paint.clone()), "batch-add", "Batch Convert Pictures"),
        path: dir.clone(),
    });
    tour.settle();
    shoot_dialog(tour, "36c-sue-paint-batch-convert")?;

    tour.paint_command("anim.timeline.new");
    tour.settle();
    let samples: Vec<f32> = (0..24_000)
        .map(|at| (at as f32 * 0.06).sin() * 0.4)
        .collect();
    let sound = sue_core::PCMFloat1D::new(24_000, 1, samples).map_err(|error| error.to_string())?;
    let wav = dir.join("Voice.wav");
    std::fs::write(&wav, sue_io::encode_wav(&sound)).map_err(|error| error.to_string())?;
    tour.request(ShellRequest::OpenFile(wav));
    tour.settle();
    tour.paint_command("file.export.audio");
    tour.settle();
    shoot_dialog(tour, "36d-sue-paint-export-audio")?;
    tour.camera.size = size;
    Ok(())
}
