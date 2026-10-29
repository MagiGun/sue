//! Android: the activity Android starts Sue in. The launcher
//! (`crates/sue-platform/android`) hands it over with [`set_app`] before
//! Sue runs; the event loop, the sandbox paths and the window reach it
//! through [`app`].

use std::path::PathBuf;
use std::sync::OnceLock;

pub use winit::platform::android::activity::AndroidApp;

static APP: OnceLock<AndroidApp> = OnceLock::new();

/// Keeps the activity Android handed to Sue (the first one is kept).
pub fn set_app(app: AndroidApp) {
    let _ = APP.set(app);
}

/// The activity, once Android has started Sue.
#[must_use]
pub fn app() -> Option<AndroidApp> {
    APP.get().cloned()
}

/// The device's shared storage, where people's own files are (the
/// Download, Pictures and Documents folders).
#[must_use]
pub fn shared_storage() -> PathBuf {
    PathBuf::from("/storage/emulated/0")
}
