//! Scenes of the shell's polish (P4-19): the empty Hub in the presets
//! and at 2.0 scale is `00-hub` of each run; these add the notices.

use sue_rice::RiceValue;
use sue_wm::ShellRequest;

use super::super::Tour;

/// What the notices of these scenes say.
const NEWS: [&str; 5] = [
    "Sue uses the Tablet interface",
    "Sue uses the Desktop interface",
    "Copied two layers",
    "Saved Picture.png (PNG) in your Pictures folder",
    "no.such.command is not available",
];

/// 00n to 00q. Five notices on the empty Hub: three show and two fold
/// behind them; unfolded; waiting under the Settings dialog; and on a
/// phone, the newest alone over the tab bar. Leaves no notice behind.
pub(super) fn notices(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.keep_notices = true;
    for (index, text) in NEWS.iter().enumerate() {
        tour.runtime.shell.notify(*text, index == NEWS.len() - 1);
    }
    tour.shoot("00n-notices-folded")?;
    if let Some(more) = tour.runtime.shell.notices_fold_rect() {
        tour.camera.click(more.center());
    }
    tour.shoot("00o-notices-unfolded")?;
    if let Some(fewer) = tour.runtime.shell.notices_fold_rect() {
        tour.camera.click(fewer.center());
    }
    tour.settle();
    tour.request(ShellRequest::OpenSettings(None));
    tour.shoot("00p-notices-under-a-dialog")?;
    tour.runtime.shell.close_window();
    tour.settle();

    let size = tour.camera.size;
    let interface = tour.runtime.engine.world.rice.get(sue_wm::INTERFACE_KEY);
    tour.set(sue_wm::INTERFACE_KEY, RiceValue::Text("phone".to_owned()));
    tour.camera.size = [430, 932];
    tour.shoot("00q-phone-notice")?;
    if let Some(interface) = interface {
        tour.set(sue_wm::INTERFACE_KEY, interface);
    }
    tour.camera.size = size;
    tour.runtime.shell.clear_notices();
    tour.keep_notices = false;
    tour.settle();
    Ok(())
}
