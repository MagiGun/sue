//! Scenes of Sue VE in a computer's window: the start, the Edit page on
//! a sample edit (a movie Sue wrote itself, cut with a dissolve, a title
//! over it), a clip in the inspector, the pool as a list, the Media and
//! Deliver pages, and a page a later stage fills.

use std::time::Duration;

use sue_wm::{AppletId, ShellRequest};

use super::super::Tour;

/// Sends Sue VE to each of `targets` in turn.
fn go(tour: &mut Tour<'_>, sve: &AppletId, targets: &[&str]) {
    if let Some(applet) = tour.runtime.applets.get_mut(sve) {
        for target in targets {
            applet.navigate(target);
        }
    }
}

/// Lets the background work (thumbnails, sound, the frames the monitors
/// ask for) finish, in real time, then the shell come to rest.
fn wait(tour: &mut Tour<'_>) {
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(25));
        tour.settle();
    }
}

/// 50a to 50g.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    let sve = AppletId::new(sue_ve::ID).map_err(|error| error.to_string())?;
    tour.request(ShellRequest::Open(sve.clone()));
    tour.settle();
    tour.shoot("50a-sve-start")?;
    let dir = tour.home.join("sve-media");
    let made = tour
        .runtime
        .with_context(&mut tour.camera.painter, tour.camera.now, |cx| {
            sue_ve::sample::sample_project(cx, &dir)
        });
    let (project, _) = made.ok_or("the sample edit was not made")?;
    tour.request(ShellRequest::OpenIn {
        entity: project,
        applet: sve.clone(),
    });
    // The playhead where the title fades in over the first shot.
    tour.command(&sve, "marker.go_in");
    tour.command(&sve, "playback.next_frame");
    tour.command(&sve, "view.zoom_fit");
    wait(tour);
    tour.shoot("50b-sve-edit")?;
    tour.command(&sve, "edit.select_all");
    tour.command(&sve, "playback.next_edit");
    wait(tour);
    tour.shoot("50c-sve-inspector")?;
    tour.command(&sve, "edit.deselect");
    go(tour, &sve, &["pool/list"]);
    wait(tour);
    tour.shoot("50d-sve-pool-list")?;
    go(tour, &sve, &["pool/thumbs", "page/media"]);
    wait(tour);
    tour.shoot("50e-sve-media")?;
    go(tour, &sve, &["page/deliver"]);
    wait(tour);
    tour.shoot("50f-sve-deliver")?;
    go(tour, &sve, &["page/color"]);
    tour.settle();
    tour.shoot("50g-sve-color-later")?;
    go(tour, &sve, &["page/edit", "dialog/new-sequence"]);
    tour.settle();
    tour.shoot("50h-sve-new-sequence")?;
    go(tour, &sve, &["dialog/close"]);
    tour.request(ShellRequest::Hub);
    tour.settle();
    Ok(())
}
