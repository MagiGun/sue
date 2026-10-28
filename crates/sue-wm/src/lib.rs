//! # sue-wm: the adaptive tiling and touch window manager
//!
//! Every tool is a [`SueApplet`] registered at runtime with the
//! [`Launchpad`], which also resolves the pinned boot applet. [`solve_layout`]
//! maps the Left Dock, Center Stage, Right Dock and Bottom Timeline onto
//! desktop tiles or mobile drawers and bottom sheets. [`SpringDamper`] animates
//! panels with `F = -kx - cv` (or snaps instantly on `LegacyLowSpec`), and
//! [`FrameScheduler`] sleeps at 0 FPS whenever nothing changed. [`Workspace`]
//! ties them together with tabs, docked applets, focus and the riceable
//! keymap, over one undoable [`World`].
//!
//! The visual layer is renderer-agnostic down to the [`Canvas`] draw list:
//! [`Ui`] is an immediate-mode widget toolkit, [`draw_shell`] composes the
//! top bar, docks, Launchpad and drop overlay, and [`UiRenderer`] draws the
//! canvas with one instanced `wgpu` pipeline and `fontdue` glyphs.
//!
//! Specification: `HUMANZ.md` §3.2–3.6.

mod applet;
mod canvas;
mod config;
mod error;
mod gpu;
mod launchpad;
mod layout;
mod scheduler;
mod shell;
mod spring;
mod text;
mod theme;
mod ui;
mod workspace;
mod world;

pub use applet::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, InputResponse,
    MediaClass, ShellRequest, SueApplet,
};
pub use canvas::{BoxStyle, Canvas, Color, DrawCommand, FontFace, intersect};
pub use config::{Keymap, WmConfig, register_rice, rice_declarations};
pub use error::WmError;
pub use gpu::{FrameTarget, UiRenderer, render_offscreen};
pub use launchpad::{AppletFactory, BootTarget, HubView, Launchpad};
pub use layout::{
    LayoutMetrics, LayoutRequest, LayoutSolution, Placement, Rect, SheetDetent, Topology, Zone,
    solve_layout,
};
pub use scheduler::{Damage, FrameScheduler, FrameTicket, RenderDemand};
pub use shell::{ShellAction, ShellFrame, apply_actions, draw_shell};
pub use spring::{SpringDamper, SpringMode, SpringParams};
pub use text::{FontId, Fonts, PlacedGlyph};
pub use theme::Theme;
pub use ui::{Response, SliderEvent, Ui, UiInput, UiState, WidgetId, widget_id};
pub use workspace::{Focus, Tab, Workspace};
pub use world::{ArenaEdit, RiceEdit, World};
