//! # sue-platform: desktop and mobile OS abstraction
//!
//! The **only** crate allowed to contain OS-specific code (backbone rule 6). Everything
//! above it talks to the [`PlatformHost`] trait and the portable types here:
//! [`OsFamily`], [`FormFactor`], [`HardwareCapabilities`], [`PlatformPaths`]
//! and the unified [`InputEvent`].
//!
//! Two hosts implement the trait. [`HeadlessHost`] has virtual surfaces and an
//! injectable input queue, for CI, the self-check and remote scripting.
//! [`DesktopHost`] opens a native `winit` window with a `wgpu` surface and
//! egui on top (`egui-winit` input and clipboard, [`EguiPainter`] drawing),
//! driven by [`run_desktop`], which sleeps at 0 FPS whenever neither the
//! application nor egui wants a frame. The `wgpu` surface is created through
//! its safe API, so this crate keeps `unsafe` forbidden like every other one.
//!
//! The rest of the OS surface lives here too: [`AudioOutput`] (a `cpal`
//! stream fed by a lock-free ring), [`list_dir`] and [`write_atomic`], the
//! system's languages ([`system_languages`]), and
//! hand-offs to the system browser, file manager and the system's own file
//! dialogs (on phones and tablets they answer later, see [`poll_picks`]).
//!
//! Specification: `HUMANZ.md` §3.1, §3.2 and §3.5.

#[cfg(target_os = "android")]
pub mod android;
mod audio;
mod desktop;
mod device;
pub mod fs;
mod gpu;
mod host;
mod input;
#[cfg(target_os = "ios")]
mod ios;
mod locale;
mod os;
mod paint;
mod paths;
#[cfg_attr(
    not(any(target_os = "android", target_os = "ios")),
    allow(
        dead_code,
        reason = "only phones and tablets stage picked files; the tests run it everywhere"
    )
)]
mod picks;
pub mod time;
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "only the browser keeps its files in memory; the tests run it everywhere"
    )
)]
mod vfs;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub use audio::{
    AudioConfig, AudioOutput, AudioSink, AudioStreamInfo, NullAudio, RealtimeStatus,
    audio_output_devices,
};
pub use desktop::{
    DesktopApp, DesktopHost, DesktopOptions, FramePacing, MAIN_SURFACE, run_desktop,
};
pub use device::{
    FormFactor, GpuTier, GpuVendor, GraphicsApi, HardwareCapabilities, OsFamily, PowerSource,
};
pub use fs::{DirEntryInfo, hidden_by_os, list_dir, write_atomic};
pub use gpu::{GpuContext, GpuPreference, WindowSurface, describe_adapter, device_kind};
pub use host::{
    HeadlessHost, PlatformError, PlatformHost, SafeAreaInsets, SurfaceDescriptor, SurfaceId,
    SurfaceInfo,
};
pub use input::{
    FileDrop, InputEvent, Key, KeyEvent, Modifiers, MouseButton, MouseEvent, NamedKey, Point,
    ScreenEdge, StylusSample, SwipeDirection, TouchGesture, WindowEvent,
};
pub use locale::{normalize_language, system_languages};
pub use os::{
    FileFilter, PickNews, Picking, ShownPlace, copy_text, deliver_picked_files, file_manager,
    open_document, open_url, pick_folder, pick_open_file, pick_save_file, picks_busy, poll_picks,
    reveal_file, reveal_folder, shown_path, shown_place, stays_in_cache, web_url,
};
pub use paint::{EguiPainter, NoTextures, TextureUpload};
pub use paths::{PlatformPaths, font_dirs, system_font_dirs};
/// Random bits for seeding entity ids (`sue_core::seed_entity_ids`): the
/// page's own randomness in the browser, where std has none; elsewhere
/// std's hasher keys, which the system fills.
#[must_use]
pub fn random_seed() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        web::random_seed()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::hash::RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        hasher.finish()
    }
}

/// `false` where Sue cannot start threads of its own (the browser, as a
/// static page without wasm threads): work meant for a worker runs where
/// it is asked for.
pub const THREADS: bool = !cfg!(target_arch = "wasm32");
/// The `wgpu` version Sue renders with, re-exported so renderers match it.
pub use wgpu;
