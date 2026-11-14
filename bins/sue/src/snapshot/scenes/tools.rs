//! Scenes of the two tools: the Image Viewer and the Converter.

use sue_wm::{AppletId, ShellRequest};

use super::super::Tour;

/// Scene 06. The Image Viewer: Open Picture asks the system's dialog, answered
/// here with a JPEG XL picture.
pub(super) fn image_viewer(tour: &mut Tour<'_>) -> Result<(), String> {
    let viewer = AppletId::new(crate::viewer::ID).map_err(|error| error.to_string())?;
    tour.command(&viewer, "viewer.open");
    let picture = tour.home.join("test-card.jxl");
    if !tour
        .runtime
        .answer_pick(&picture, &mut tour.camera.painter, tour.camera.now)
    {
        return Err("Open Picture did not ask for a file".to_owned());
    }
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
