//! # sue-wm: the infinite canvas shell
//!
//! Every tool is a [`SueApplet`] registered at runtime with the
//! [`Launchpad`] (LAW 1): canvas entity kinds, drawer utilities and the
//! Settings window. The [`Shell`] draws Sue's window with egui: an infinite
//! spatial canvas whose [`Camera`] zooms from 10% to 3200%, entity nodes
//! drawn by their applets inside a camera-transformed layer, a floating top
//! pill of modes and spawn buttons, a floating selection pill above the
//! selected entity, a spring-animated radial marking menu, a fuzzy command
//! [`Palette`], spring-driven drawers and a bottom timeline whose blocks are
//! the same arena entities.
//!
//! Every moving part is a [`SpringDamper`] (`F = -kx - cv`) stepped with the
//! real frame time; [`solve_layout`] maps the shell onto desktop tiles or
//! touch overlays, and [`FrameScheduler`] keeps the frame cap of the
//! hardware profile. The [`Theme`] binds the rice palette into egui's style.
//! Text fields edit the arena in place through [`ArenaText`] and become
//! undo steps through [`Typing`] (LAW 6).
//!
//! Specification: `HUMANZ.md` §3.2 to §3.6.

mod applet;
mod camera;
mod catalog;
mod config;
mod error;
pub mod keys;
mod launchpad;
mod layout;
mod palette;
mod radial;
mod scene;
mod scheduler;
mod shell;
mod spring;
mod text;
mod theme;
mod timeline;
mod transport;
pub mod widgets;
mod world;

pub use applet::{
    AppletContext, AppletError, AppletId, AppletManifest, AppletRole, MediaClass, ModeSpec,
    PaletteEntry, ShellRequest, ShellView, SpawnSpec, SueApplet, Tick,
};
pub use camera::Camera;
pub use catalog::{
    Catalog, Choice, CommandButton, InfoTable, Property, PropertyGroup, SettingsGroup,
    SettingsItem, SettingsPage,
};
pub use config::{CanvasConfig, GridStyle, Keymap, WmConfig, register_rice, rice_declarations};
pub use error::WmError;
pub use launchpad::{AppletFactory, Applets, Launchpad};
pub use layout::{LayoutMetrics, LayoutRequest, LayoutSolution, Topology, solve_layout};
pub use palette::{Palette, PaletteOutcome, fuzzy_score};
pub use radial::{Radial, RadialOutcome, wedge_at, wedge_point};
pub use scene::{NodeId, Scene, SceneNode};
pub use scheduler::{Damage, FrameScheduler, FrameTicket, RenderDemand};
pub use shell::{
    CANVAS_MODE, NODE_TITLE, Shell, ShellIo, TIMELINE_MODE, profile_label, project_to_timeline,
};
pub use spring::{SpringDamper, SpringMode, SpringParams};
pub use text::{ArenaText, TYPING_PAUSE, Typing};
pub use theme::Theme;
pub use timeline::{Block, TimelineEvent, TimelineState, timecode};
pub use transport::{Clip, Transport, pcm_of};
pub use world::{
    ArenaEdit, Batch, NewEntity, PlaceNode, RemoveNode, RiceEdit, SetNodeProp, SetNodeRect,
    SpawnNode, World,
};
