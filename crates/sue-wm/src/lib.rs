//! # sue-wm: the adaptive tiling and touch window manager
//!
//! Every tool is a [`SueApplet`] registered at runtime with the
//! [`Launchpad`], which also resolves the pinned boot applet. [`solve_layout`]
//! maps a flush workstation (header, activity rails, docks, Center Stage,
//! status bar) onto desktop tiles or mobile drawers and bottom sheets.
//! [`SpringDamper`] animates panels with `F = -kx - cv` (or snaps instantly on
//! `LegacyLowSpec`), and [`FrameScheduler`] sleeps at 0 FPS whenever nothing
//! changed. [`Workspace`] ties them together with tabs, one applet per dock,
//! focus and the riceable keymap, over one undoable [`World`].
//!
//! The visual layer is renderer-agnostic down to the [`Canvas`] draw list:
//! [`Ui`] is an immediate-mode widget toolkit with an overlay layer for menus
//! and pop-ups, [`draw_shell`] composes the shell, and [`UiRenderer`] draws the
//! canvas with one instanced `wgpu` pipeline and `fontdue` glyphs.
//!
//! Specification: `HUMANZ.md` §3.2 to §3.6.

mod applet;
mod canvas;
mod catalog;
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
    MediaClass, ShellRequest, SueApplet, Tick,
};
pub use canvas::{BoxStyle, Canvas, Color, DrawCommand, FontFace, Layer, intersect};
pub use catalog::{
    Catalog, Choice, CommandButton, InfoTable, Property, PropertyGroup, SettingsGroup,
    SettingsItem, SettingsPage,
};
pub use config::{Keymap, WmConfig, register_rice, rice_declarations};
pub use error::WmError;
pub use gpu::{FrameTarget, UiRenderer, render_offscreen};
pub use launchpad::{AppletFactory, BootTarget, HubView, Launchpad};
pub use layout::{
    LayoutMetrics, LayoutRequest, LayoutSolution, Placement, Rect, SheetDetent, Topology, Zone,
    solve_layout,
};
pub use scheduler::{Damage, FrameScheduler, FrameTicket, RenderDemand};
pub use shell::{Dialog, ShellAction, ShellFrame, apply_actions, draw_shell, profile_label};
pub use spring::{SpringDamper, SpringMode, SpringParams};
pub use text::{FontId, Fonts, PlacedGlyph};
pub use theme::Theme;
pub use ui::{
    EditOutcome, LineEdit, MenuEntry, Response, SliderEvent, Ui, UiChoice, UiInput, UiState,
    WidgetId, widget_id,
};
pub use workspace::{Focus, Tab, Workspace};
pub use world::{ArenaEdit, RiceEdit, World};
