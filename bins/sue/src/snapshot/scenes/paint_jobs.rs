//! Scenes of the exports that run in the background: the notice of one
//! running, with its progress and Cancel, the notice when it is done,
//! and Export Timelapse with its preview (CTV-7).

use sue_wm::{FilePick, ShellRequest};

use super::super::Tour;

/// 11j1. An animated GIF exporting in the background: its notice, with
/// the progress and Cancel. 11j2. Done: where it went, and Show in
/// Folder. 11j3. Export Timelapse: the preview playing the drawing in
/// the video's shape, here 16:9. Sue is left as it was found.
pub(super) fn exports(tour: &mut Tour<'_>) -> Result<(), String> {
    let folder = tour.home.join("exports");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let gif = folder.join("Walk.gif");
    // Held before it writes, so the picture shows it running.
    sue_paint::hold_exports_to(&folder, true);
    tour.request(ShellRequest::Picked {
        pick: FilePick::save(Some(tour.paint.clone()), "anim-gif", "Export", "x"),
        path: gif.clone(),
    });
    // How far it got: the desktop's timer reads it; here the tour does.
    let start = std::time::Instant::now();
    while tour
        .runtime
        .transfers
        .list()
        .first()
        .is_some_and(|transfer| !matches!(transfer.state, sue_wm::TransferState::Running(fraction) if fraction > 0.5))
        && start.elapsed() < std::time::Duration::from_secs(30)
    {
        tour.runtime.transfers.poll();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    tour.shoot("11j1-sue-paint-export-running")?;
    sue_paint::hold_exports_to(&folder, false);
    while tour.runtime.transfers.poll() && start.elapsed() < std::time::Duration::from_secs(60) {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    tour.shoot("11j2-sue-paint-export-done")?;
    tour.runtime.shell.clear_notices();
    let _ = std::fs::remove_dir_all(&folder);

    // A recorded timelapse, and its export dialog in a wide shape. The
    // frames are kept as recording keeps them, without the switch, so
    // History stays as it was.
    let picture = tour
        .runtime
        .applets
        .get(&tour.paint)
        .and_then(|applet| applet.focus())
        .ok_or("a canvas is open")?;
    let frames =
        sue_paint::timelapse::folder(&tour.runtime.engine.catalog.folders["paint"], picture);
    let flat = sue_paint::composite::flatten(
        &tour.runtime.engine.world.arena,
        picture,
        sue_paint::composite::RenderOptions::default(),
    )
    .ok_or("the canvas flattens")?;
    for index in 0..3 {
        sue_paint::timelapse::capture(
            &frames,
            index,
            flat.bytes().unwrap_or_default(),
            [flat.width(), flat.height()],
        )?;
    }
    tour.paint_command("file.timelapse.export");
    tour.paint_go(&["dialog/timelapse-wide"]);
    tour.shoot("11j3-sue-paint-export-timelapse")?;
    tour.paint_go(&["dialog/close"]);
    tour.settle();
    let _ = std::fs::remove_dir_all(&frames);
    Ok(())
}
