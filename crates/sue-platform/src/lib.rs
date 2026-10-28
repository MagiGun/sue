//! # sue-platform: desktop and mobile OS abstraction
//!
//! The **only** crate allowed to contain OS-specific code (LAW 5). Everything
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
//! stream fed by a lock-free ring), [`list_dir`] and [`write_atomic`], and
//! hand-offs to the system browser, file manager and native dialogs.
//!
//! Specification: `HUMANZ.md` §3.1, §3.2 and §3.5.

mod audio;
mod desktop;
mod device;
mod fs;
mod gpu;
mod host;
mod input;
mod os;
mod paint;
mod paths;

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
pub use gpu::{GpuContext, GpuPreference, WindowSurface, describe_adapter};
pub use host::{
    HeadlessHost, PlatformError, PlatformHost, SafeAreaInsets, SurfaceDescriptor, SurfaceId,
    SurfaceInfo,
};
pub use input::{
    FileDrop, InputEvent, Key, KeyEvent, Modifiers, MouseButton, MouseEvent, NamedKey, Point,
    ScreenEdge, StylusSample, SwipeDirection, TouchGesture, WindowEvent,
};
pub use os::{
    FileFilter, copy_text, open_url, pick_open_file, pick_save_file, reveal_folder, web_url,
};
pub use paint::{EguiPainter, NoTextures, TextureUpload};
pub use paths::PlatformPaths;
/// The `wgpu` version Sue renders with, re-exported so renderers match it.
pub use wgpu;
