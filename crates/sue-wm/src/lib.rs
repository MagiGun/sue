//! # sue-wm: the adaptive tiling and touch window manager
//!
//! Every tool is a [`SueApplet`] registered at runtime with the
//! [`Launchpad`], which also resolves the pinned boot applet. [`solve_layout`]
//! maps the Left Dock, Center Stage, Right Dock and Bottom Timeline onto
//! desktop tiles or mobile drawers and bottom sheets. [`SpringDamper`] animates
//! panels with `F = -kx - cv` (or snaps instantly on `LegacyLowSpec`), and
//! [`FrameScheduler`] sleeps at 0 FPS whenever nothing changed. [`Workspace`]
//! ties them together with tabs and the riceable global keymap.
//!
//! Specification: `HUMANZ.md` §3.2–3.6.

mod applet;
mod config;
mod error;
mod launchpad;
mod layout;
mod scheduler;
mod spring;
mod workspace;

pub use applet::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, InputResponse, SueApplet,
};
pub use config::{Keymap, WmConfig, register_rice, rice_declarations};
pub use error::WmError;
pub use launchpad::{AppletFactory, BootTarget, HubView, Launchpad};
pub use layout::{
    LayoutMetrics, LayoutRequest, LayoutSolution, Placement, Rect, SheetDetent, Topology, Zone,
    solve_layout,
};
pub use scheduler::{Damage, FrameScheduler, FrameTicket, RenderDemand};
pub use spring::{SpringDamper, SpringMode, SpringParams};
pub use workspace::{Tab, Workspace};
