//! # sue-wm: Sue's shell
//!
//! Every studio, tool and window is a [`SueApplet`] registered at runtime
//! with the [`Launchpad`]. The [`Shell`] draws Sue's window with egui: a top
//! bar with the Sue button in its corner (the Sue menu, a
//! [`MenuBar::dropdown`]) followed by the open studio's or tool's menus,
//! the page under it (the Hub, a box for every studio and tool with a pin
//! that opens one at launch, or the studio or tool itself), a status bar,
//! a fuzzy command [`Palette`] and the Settings window.
//!
//! Every moving part is a [`SpringDamper`] (`F = -kx - cv`) stepped with the
//! real frame time; [`solve_layout`] places the bars and the page for every
//! form factor, and [`FrameScheduler`] keeps the frame cap of the hardware
//! profile. The [`Theme`] binds the rice palette into egui's style. Text
//! fields edit the arena in place through [`ArenaText`] and become undo
//! steps through [`Typing`] (backbone rule 2).
//!
//! Every string the shell shows comes from the language database
//! (`sue-lang`, namespace `wm`), in the language [`lang`] picks for Sue or
//! for the studio or tool being drawn.
//!
//! Specification: `HUMANZ.md` §3.2 to §3.6.

/// Text from the language database's `wm` namespace.
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("wm", $($arg)*) };
}

mod applet;
pub mod brand;
mod catalog;
mod color_pick;
mod config;
mod error;
pub mod fonts;
pub mod keys;
mod lang;
mod launchpad;
mod layout;
mod menubar;
mod palette;
mod scheduler;
mod shell;
pub mod shortcuts;
mod spring;
pub mod start;
mod text;
mod theme;
mod transfers;
mod transport;
pub mod widgets;
mod work;
mod world;

pub use applet::{
    AppletContext, AppletError, AppletId, AppletManifest, AppletRole, FilePick, FileTemplate,
    FileType, PaletteEntry, ShellRequest, ShellView, SueAction, SueApplet, Tick, sue_menu,
};
pub use catalog::{
    Catalog, Choice, CommandButton, InfoTable, Property, PropertyGroup, SettingsGroup,
    SettingsItem, SettingsPage,
};
pub use color_pick::{ColorPick, alpha_bar, color_button, color_wheel, hsv_of, rgb_of, swatch};
pub use config::{
    CARET_GLIDE_KEY, Keymap, SMOOTH_CARET_KEY, WmConfig, register_rice, rice_declarations,
};
pub use error::WmError;
pub use lang::{
    LANGUAGE_KEY, applet_language, applet_language_choices, language_declaration, language_key,
    rice_section, sue_language, sue_language_choices,
};
pub use launchpad::{AppletFactory, Applets, Launchpad};
pub use layout::{LayoutMetrics, LayoutRequest, LayoutSolution, bar_height, solve_layout};
pub use menubar::{ItemState, Menu, MenuBar, MenuItem, context_items};
pub use palette::{Palette, PaletteOutcome, fuzzy_score};
pub use scheduler::{Damage, FrameScheduler, FrameTicket, RenderDemand};
pub use shell::{
    HubRow, INTERFACE_KEY, PHONE_FRAME_KEY, PIN_KEY, Shell, ShellIo, pinned, profile_label,
};
pub use spring::{SpringDamper, SpringMode, SpringParams};
pub use text::{ArenaText, TYPING_PAUSE, Typing};
pub use theme::{COLOR_KEYS, STUDIO_THEME_CUSTOM, STUDIO_THEME_SUE, Theme};
pub use transfers::{Transfer, TransferKind, TransferState, Transfers};
pub use transport::{Clip, ClipSource, Transport, pcm_of};
pub use work::{Autosave, LabelEdit, RecentFile, WorkList, ago, unix_now};
pub use world::{ArenaEdit, Batch, RiceEdit, World};
