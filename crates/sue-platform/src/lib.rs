//! # sue-platform: desktop and mobile OS abstraction
//!
//! The **only** crate allowed to contain OS-specific code (LAW 5). Everything
//! above it talks to the [`PlatformHost`] trait and the portable types here:
//! [`OsFamily`], [`FormFactor`], [`HardwareCapabilities`], [`PlatformPaths`]
//! and the unified [`InputEvent`].
//!
//! [`HeadlessHost`] is a complete host with virtual surfaces and an injectable
//! input queue, used by CI, the self-check and remote scripting.
//!
//! Specification: `HUMANZ.md` §3.1–3.2.

mod device;
mod host;
mod input;
mod paths;

pub use device::{
    FormFactor, GpuTier, GpuVendor, GraphicsApi, HardwareCapabilities, OsFamily, PowerSource,
};
pub use host::{
    HeadlessHost, PlatformError, PlatformHost, SafeAreaInsets, SurfaceDescriptor, SurfaceId,
    SurfaceInfo,
};
pub use input::{
    InputEvent, Key, KeyEvent, Modifiers, MouseButton, MouseEvent, NamedKey, Point, ScreenEdge,
    StylusSample, SwipeDirection, TouchGesture,
};
pub use paths::PlatformPaths;
