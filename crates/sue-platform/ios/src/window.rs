//! Sue's one window, as UIKit knows it.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_ui_kit::{UIApplication, UIWindow, UIWindowScene};

/// The key window of Sue's scene.
pub(crate) fn key_window(mtm: MainThreadMarker) -> Option<Retained<UIWindow>> {
    let app = UIApplication::sharedApplication(mtm);
    let window = app
        .connectedScenes()
        .allObjects()
        .iter()
        .find_map(|scene| scene.downcast::<UIWindowScene>().ok()?.keyWindow());
    #[allow(
        deprecated,
        reason = "the one key window, for a scene that has none yet"
    )]
    window.or_else(|| app.keyWindow())
}
