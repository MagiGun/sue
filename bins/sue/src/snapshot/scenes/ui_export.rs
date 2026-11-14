//! Gallery scenes of the export settings dialog and the batch list
//! (`sue_wm::export`, `sue_wm::job_list`). No studio opens them yet (Sue
//! Paint's File menu does in a later package), so each is drawn alone
//! over the plain backdrop of the dialog scenes.

use std::path::PathBuf;

use sue_core::{JobPool, PixelFormat, RasterTile2D};
use sue_io::SampleDepth;
use sue_wm::export::{
    CHANNELS, ExportDialog, PictureSettings, SAMPLE_RATES, SizeMode, SoundSettings,
};
use sue_wm::job_list::{JobList, Start};

use super::super::Tour;
use super::ui_dialogs::{backdrop, shoot_ui};

/// The size of the scenes, in points.
const DESKTOP: [u32; 2] = [1100, 860];

/// A small picture file for the batch list.
fn png(width: u32, height: u32) -> Result<Vec<u8>, String> {
    let pixels = (0..width * height)
        .flat_map(|index| [(index * 9 % 256) as u8, 120, 200, 255])
        .collect();
    let tile = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())?;
    sue_io::encode_png(&tile)
}

/// A folder of three scans and a broken file, made afresh for the scene.
fn scans() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("sue-snapshot-batch");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    for (name, size) in [
        ("Page 1.png", (60, 80)),
        ("Page 2.png", (60, 80)),
        ("Page 3.png", (64, 90)),
    ] {
        std::fs::write(dir.join(name), png(size.0, size.1)?).map_err(|error| error.to_string())?;
    }
    std::fs::write(dir.join("Page 4.png"), b"not a picture").map_err(|error| error.to_string())?;
    Ok(dir)
}

/// Scenes 28n to 28r: the picture page for a JPEG of a known size, the
/// note of a format with a largest side, the sound page, and the batch
/// dialog empty and after a run.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.settle();
    let before = tour.camera.size;
    tour.camera.size = DESKTOP;

    let mut jpeg = PictureSettings::default();
    jpeg.choose_format("jpg");
    jpeg.size = SizeMode::Fit;
    jpeg.width = 1920.0;
    jpeg.height = 1080.0;
    let mut dialog = ExportDialog::for_picture("Export Single Layer", jpeg, Some((3000, 2000)));
    shoot_ui(tour, "28n-ui-export-picture", false, |ui, theme| {
        backdrop(ui, theme);
        dialog.show(ui.ctx(), theme);
    })?;

    let mut icon = PictureSettings::default();
    icon.choose_format("ico");
    icon.options.grayscale = true;
    let mut dialog = ExportDialog::for_picture("Save Duplicate", icon, Some((1024, 1024)));
    shoot_ui(tour, "28o-ui-export-icon", false, |ui, theme| {
        backdrop(ui, theme);
        dialog.show(ui.ctx(), theme);
    })?;

    let sound = SoundSettings {
        format: 1,
        depth: SampleDepth::Int24,
        rate: SAMPLE_RATES
            .iter()
            .position(|rate| *rate == Some(48_000))
            .unwrap_or(0),
        channels: CHANNELS
            .iter()
            .position(|count| *count == Some(2))
            .unwrap_or(0),
        normalize: true,
    };
    let mut dialog = ExportDialog::for_sound("Export Animation Audio", sound);
    shoot_ui(tour, "28p-ui-export-sound", false, |ui, theme| {
        backdrop(ui, theme);
        dialog.show(ui.ctx(), theme);
    })?;

    let mut empty = JobList::default();
    shoot_ui(tour, "28q-ui-batch-empty", false, |ui, theme| {
        backdrop(ui, theme);
        empty.show_dialog(ui.ctx(), theme, "Batch Convert Pictures");
    })?;

    let dir = scans()?;
    let mut list = JobList::default().with_pool(JobPool::inline());
    list.add_path(&dir);
    list.picture.choose_format("jpg");
    list.picture.options.quarter_turns = 1;
    list.picture.options.brightness = 0.15;
    list.output.pattern = "{name} turned".to_owned();
    if list.start() != Start::Started(4) {
        return Err("the batch list did not start its four files".to_owned());
    }
    list.poll().ok_or("the batch run did not finish")?;
    // Tall enough to show every part of the dialog.
    tour.camera.size = [1100, 1400];
    shoot_ui(tour, "28r-ui-batch-run", false, |ui, theme| {
        backdrop(ui, theme);
        list.show_dialog(ui.ctx(), theme, "Batch Convert Pictures");
    })?;
    let _ = std::fs::remove_dir_all(&dir);

    tour.camera.size = before;
    Ok(())
}
