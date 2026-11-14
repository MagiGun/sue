//! Scenes of the two tools: the Image Viewer and the Converter.

use sue_io::{IngestOptions, NativeJxlPipeline, ingest_bytes, meta};
use sue_wm::{AppletId, ShellRequest};

use super::super::Tour;

/// Scene 06. The Image Viewer with a JPEG XL picture. Sue Paint is who
/// opens a picture file now, so the picture is read into the project
/// here and handed to the Image Viewer, as Send To does.
pub(super) fn image_viewer(tour: &mut Tour<'_>) -> Result<(), String> {
    let viewer = AppletId::new(crate::viewer::ID).map_err(|error| error.to_string())?;
    let picture = tour.home.join("test-card.jxl");
    let bytes = std::fs::read(&picture).map_err(|error| error.to_string())?;
    let arena = &mut tour.runtime.engine.world.arena;
    let ingested = ingest_bytes(
        arena,
        "test-card.jxl",
        bytes,
        &NativeJxlPipeline,
        IngestOptions {
            byte_map_width: 64,
            clock: sue_platform::time::since_start,
        },
    )
    .map_err(|error| error.to_string())?;
    let entity = arena
        .get_mut(ingested.handle)
        .map_err(|error| error.to_string())?;
    entity.set_meta(meta::SOURCE, picture.display().to_string());
    let entity = entity.id();
    tour.request(ShellRequest::OpenIn {
        entity,
        applet: viewer,
    });
    tour.shoot("06-image-viewer")
}

/// Scene 07. The Converter with a picture, a song and a text file.
pub(super) fn converter(tour: &mut Tour<'_>) -> Result<(), String> {
    for name in ["photo.png", "chord.wav", "README.md"] {
        tour.request(ShellRequest::SendFile {
            to: tour.convert.clone(),
            purpose: "add".to_owned(),
            path: tour.home.join(name),
        });
    }
    tour.request(ShellRequest::Open(tour.convert.clone()));
    tour.shoot("07-converter")
}
