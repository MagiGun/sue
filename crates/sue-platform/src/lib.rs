//! # sue-platform: desktop and mobile OS abstraction
//!
//! The **only** crate allowed to contain OS-specific code (LAW 5). Everything
//! above it talks to the [`PlatformHost`] trait and the portable types here:
//! [`OsFamily`], [`FormFactor`], [`HardwareCapabilities`], [`PlatformPaths`]
//! and the unified [`InputEvent`].
//!
//! Two hosts implement the trait. [`HeadlessHost`] has virtual surfaces and an
//! injectable input queue, for CI, the self-check and remote scripting.
//! [`DesktopHost`] opens a native `winit` window with a `wgpu` surface and is
//! driven by [`run_desktop`], which sleeps at 0 FPS whenever the application
//! asks for [`FramePacing::Sleep`]. The `wgpu` surface is created through its
//! safe API, so this crate keeps `unsafe` forbidden like every other one.
//!
//! Specification: `HUMANZ.md` §3.1–3.2.

mod desktop;
mod device;
mod fs;
mod gpu;
mod host;
mod input;
mod paths;

pub use desktop::{
    CursorShape, DesktopApp, DesktopHost, DesktopOptions, FramePacing, MAIN_SURFACE, run_desktop,
};
pub use device::{
    FormFactor, GpuTier, GpuVendor, GraphicsApi, HardwareCapabilities, OsFamily, PowerSource,
};
pub use fs::write_atomic;
pub use gpu::{GpuContext, WindowSurface, describe_adapter};
pub use host::{
    HeadlessHost, PlatformError, PlatformHost, SafeAreaInsets, SurfaceDescriptor, SurfaceId,
    SurfaceInfo,
};
pub use input::{
    FileDrop, InputEvent, Key, KeyEvent, Modifiers, MouseButton, MouseEvent, NamedKey, Point,
    ScreenEdge, StylusSample, SwipeDirection, TouchGesture, WindowEvent,
};
pub use paths::PlatformPaths;
/// The `wgpu` version Sue renders with, re-exported so renderers match it.
pub use wgpu;
