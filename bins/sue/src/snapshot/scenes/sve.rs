//! Scenes of Sue VE in a computer's window: the start, the Edit page on
//! a sample edit (a movie Sue wrote itself, cut with a dissolve, a title
//! over it), a clip in the inspector, the pool as a list, the Media and
//! Deliver pages; then stage 2: the edit dressed with effects, a grade,
//! a text animator and an equalizer, on the Edit, Effects, Color and
//! Audio pages, at 1440 by 900 and at 1366 by 768, and the phone and
//! tablet layouts.

use std::time::Duration;

use sue_rice::RiceValue;
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
    go(tour, &sve, &["page/edit", "dialog/new-sequence"]);
    tour.settle();
    tour.shoot("50h-sve-new-sequence")?;
    go(tour, &sve, &["dialog/close"]);
    stage_two(tour, &sve, project)?;
    tour.request(ShellRequest::Hub);
    tour.settle();
    Ok(())
}

/// Lets processed frames, scopes and meters come in: they are drawn on
/// threads of their own.
fn wait_long(tour: &mut Tour<'_>) {
    for _ in 0..3 {
        wait(tour);
    }
}

/// 51a to 51m: the stage 2 pages on the dressed sample edit, at the
/// desktop size and at 1366 by 768, and the phone and tablet layouts.
fn stage_two(
    tour: &mut Tour<'_>,
    sve: &AppletId,
    project: sue_core::EntityId,
) -> Result<(), String> {
    let sequence = {
        let arena = &tour.runtime.engine.world.arena;
        sue_ve::model::sequences(arena, project)
            .first()
            .copied()
            .ok_or("the sample has a sequence")?
    };
    let dressed = tour
        .runtime
        .with_context(&mut tour.camera.painter, tour.camera.now, |cx| {
            sue_ve::sample::dress(cx, sequence)
        });
    dressed.ok_or("the sample edit was not dressed")?;
    // The first shot picked, the playhead in it.
    go(tour, sve, &["page/edit", "select/video/0", "seek/1.2"]);
    wait_long(tour);
    tour.shoot("51a-sve-edit-effects")?;
    go(tour, sve, &["workspace/effects"]);
    wait_long(tour);
    tour.shoot("51b-sve-effects-workspace")?;
    go(tour, sve, &["workspace/editing"]);
    tour.command(sve, "fx.mask_ellipse");
    go(tour, sve, &["page/effects"]);
    wait_long(tour);
    tour.shoot("51c-sve-effects-page")?;
    // The second shot, graded.
    go(tour, sve, &["page/color", "select/video/1", "seek/5.5"]);
    wait_long(tour);
    tour.shoot("51d-sve-color")?;
    tour.command(sve, "color.split");
    wait_long(tour);
    tour.shoot("51e-sve-color-split")?;
    tour.command(sve, "color.split");
    go(tour, sve, &["page/audio", "select/audio/0"]);
    wait_long(tour);
    tour.shoot("51f-sve-audio")?;
    go(tour, sve, &["page/deliver"]);
    wait_long(tour);
    tour.shoot("51g-sve-deliver")?;
    // The same pages at 1366 by 768: nothing runs off the side.
    let size = tour.camera.size;
    tour.camera.size = [1366, 768];
    for (page, name) in [
        ("page/edit", "51h-sve-1366-edit"),
        ("page/effects", "51i-sve-1366-effects"),
        ("page/color", "51j-sve-1366-color"),
        ("page/audio", "51k-sve-1366-audio"),
    ] {
        go(tour, sve, &[page, "select/video/0", "seek/1.2"]);
        wait_long(tour);
        tour.shoot(name)?;
    }
    // Phone and tablet: CapCut's single column.
    let interface = tour.runtime.engine.world.rice.get(sue_wm::INTERFACE_KEY);
    tour.set(sue_wm::INTERFACE_KEY, RiceValue::Text("phone".to_owned()));
    tour.camera.size = [430, 932];
    go(tour, sve, &["page/edit", "select/video/0", "seek/1.2"]);
    wait_long(tour);
    tour.shoot("51l-sve-phone")?;
    go(tour, sve, &["sheet/effects"]);
    wait(tour);
    tour.shoot("51m-sve-phone-effects-sheet")?;
    go(tour, sve, &["sheet/aspect"]);
    wait(tour);
    tour.shoot("51n-sve-phone-aspect-sheet")?;
    go(tour, sve, &["export-sheet"]);
    wait(tour);
    tour.shoot("51o-sve-phone-export")?;
    tour.set(sue_wm::INTERFACE_KEY, RiceValue::Text("tablet".to_owned()));
    tour.camera.size = [820, 1180];
    go(tour, sve, &["sheet/filters"]);
    wait_long(tour);
    tour.shoot("51p-sve-tablet")?;
    go(tour, sve, &["sheet/edit"]);
    if let Some(interface) = interface {
        tour.set(sue_wm::INTERFACE_KEY, interface);
    }
    tour.camera.size = size;
    go(tour, sve, &["page/edit"]);
    tour.settle();
    Ok(())
}
