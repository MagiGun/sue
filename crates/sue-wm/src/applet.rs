//! The applet contract every tool, utility and studio implements (LAW 1).

use std::fmt;
use std::path::PathBuf;
use std::time::Instant;

use sue_core::{Arena, AssetHandle, CoreError, NodeId, ReversibleTransaction, TxnError, UndoStack};
use sue_platform::{AudioSink, FormFactor, InputEvent};
use sue_rice::{HardwareProfile, RiceDeclaration, RiceSchema};

use crate::{
    ArenaEdit, Catalog, PropertyGroup, Rect, RenderDemand, RiceEdit, Ui, WmError, World, Zone,
};

/// An applet's unique id: `[a-z][a-z0-9-]*`, e.g. `quick-notes`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppletId(String);

impl AppletId {
    /// Validates and wraps an id.
    ///
    /// # Errors
    /// [`WmError::InvalidAppletId`].
    pub fn new(id: &str) -> Result<Self, WmError> {
        let starts_with_letter = id
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase());
        let body_ok = id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if starts_with_letter && body_ok {
            Ok(Self(id.to_owned()))
        } else {
            Err(WmError::InvalidAppletId(id.to_owned()))
        }
    }

    /// The id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppletId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What kind of applet this is, which decides where the shell lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppletCategory {
    /// A full creative environment, listed on Home.
    Studio,
    /// A viewer for one kind of asset, listed on Home.
    Inspector,
    /// A dock tool, listed on its activity rail.
    Utility,
    /// Sue's own preferences: opened from the gear, the menu and a key.
    System,
}

impl AppletCategory {
    /// Plural section title on Home, e.g. `Studios`.
    #[must_use]
    pub const fn plural(self) -> &'static str {
        match self {
            Self::Studio => "Studios",
            Self::Inspector => "Inspectors",
            Self::Utility => "Utilities",
            Self::System => "System",
        }
    }
}

/// Broad kinds of assets, used to pick an applet that can open one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaClass {
    /// A decoded raster image.
    Image,
    /// UTF-8 text.
    Text,
    /// Decoded audio.
    Audio,
    /// Raw bytes (byte map).
    Binary,
}

/// Static facts about an applet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppletManifest {
    /// Unique id.
    pub id: AppletId,
    /// Display name.
    pub name: String,
    /// Category: where the shell lists the applet.
    pub category: AppletCategory,
    /// Zone the applet lives in: dock applets are mounted into their dock at
    /// boot and picked on its rail; Center Stage applets open in tabs.
    pub home_zone: Zone,
    /// `true` if the applet works on time-based media, which shows the timeline.
    pub uses_timeline: bool,
    /// What it does, in a sentence or two (shown whole, never cut).
    pub summary: String,
    /// Asset kinds the applet can open (see [`SueApplet::open`]).
    pub opens: Vec<MediaClass>,
    /// A one-character icon from the bundled fonts.
    pub icon: String,
    /// `true` if at most one tab may host the applet.
    pub singleton: bool,
}

/// Something an applet (or the shell) asks for after the current call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellRequest {
    /// Open `handle` in the first applet that opens `class`.
    OpenAsset {
        /// The entity.
        handle: AssetHandle,
        /// What it is.
        class: MediaClass,
    },
    /// Open an applet, optionally at a place inside it (see
    /// [`SueApplet::navigate`]).
    OpenApplet {
        /// Which applet.
        id: AppletId,
        /// Where inside it, e.g. a Settings page id.
        target: Option<String>,
    },
    /// Read a file from disk and open it (handled by the engine).
    OpenFile(PathBuf),
    /// Open a web address in the system browser (handled by the engine).
    OpenUrl(String),
    /// An engine command, e.g. `rice.export` (handled by the engine).
    Command(String),
    /// Show a message in the status bar.
    Notify(String),
}

/// What an applet may touch while handling a call. Applets hold handles, not
/// data, and change the world only through transactions on `history`.
pub struct AppletContext<'a> {
    /// The arena and the live rice.
    pub world: &'a mut World,
    /// The global undo tree.
    pub history: &'a mut UndoStack<World>,
    /// Device form factor.
    pub form_factor: FormFactor,
    /// Active hardware profile.
    pub profile: HardwareProfile,
    /// Requests for the shell, processed after the call returns.
    pub requests: &'a mut Vec<ShellRequest>,
    /// `true` while the applet being called has keyboard focus.
    pub focused: bool,
    /// Settings pages, live choices and system tables.
    pub catalog: &'a Catalog,
    /// The audio output, when this host has one.
    pub audio: Option<&'a mut dyn AudioSink>,
    /// Properties of the active Center Stage applet, for the Inspector.
    pub inspection: Vec<PropertyGroup>,
}

impl AppletContext<'_> {
    /// The arena.
    #[must_use]
    pub fn arena(&self) -> &Arena {
        &self.world.arena
    }

    /// The live rice.
    #[must_use]
    pub fn rice(&self) -> &RiceSchema {
        &self.world.rice
    }

    /// Applies an arena transaction through the global undo tree.
    ///
    /// # Errors
    /// The transaction's error; nothing is recorded then.
    pub fn edit_arena<X>(&mut self, txn: X) -> Result<NodeId, TxnError>
    where
        X: ReversibleTransaction<Arena> + 'static,
    {
        self.history.apply(self.world, ArenaEdit(txn))
    }

    /// Applies a rice transaction through the global undo tree.
    ///
    /// # Errors
    /// The transaction's error; nothing is recorded then.
    pub fn edit_rice<X>(&mut self, txn: X) -> Result<NodeId, TxnError>
    where
        X: ReversibleTransaction<RiceSchema> + 'static,
    {
        self.history.apply(self.world, RiceEdit(txn))
    }

    /// Queues a request for the shell.
    pub fn request(&mut self, request: ShellRequest) {
        self.requests.push(request);
    }
}

impl fmt::Debug for AppletContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppletContext")
            .field("entities", &self.world.arena.len())
            .field("history", &self.history)
            .field("form_factor", &self.form_factor)
            .field("profile", &self.profile)
            .field("audio", &self.audio.is_some())
            .finish_non_exhaustive()
    }
}

/// How an applet reacted to an input event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputResponse {
    /// Not handled; the event may go elsewhere.
    Ignored,
    /// Handled.
    Consumed {
        /// `true` if something visible changed.
        redraw: bool,
    },
}

/// What a timer tick did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// `true` if something visible changed.
    pub redraw: bool,
    /// When the applet wants the next tick, if it wants one.
    pub wake: Option<Instant>,
}

impl Tick {
    /// Merges two ticks: redraw if either asks, wake at the earlier time.
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        Self {
            redraw: self.redraw || other.redraw,
            wake: match (self.wake, other.wake) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
        }
    }
}

/// An error raised by an applet.
#[derive(Debug)]
pub struct AppletError {
    message: String,
}

impl AppletError {
    /// An error with a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AppletError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppletError {}

impl From<CoreError> for AppletError {
    fn from(error: CoreError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<TxnError> for AppletError {
    fn from(error: TxnError) -> Self {
        Self::new(error.to_string())
    }
}

/// The contract every tool, utility and studio implements. The window manager
/// knows applets only through this trait.
pub trait SueApplet: Send {
    /// Static facts about the applet.
    fn manifest(&self) -> AppletManifest;

    /// Riceable parameters the applet declares (LAW 3).
    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        Vec::new()
    }

    /// Called when the applet is mounted in a tab or dock.
    ///
    /// # Errors
    /// Any reason the applet cannot start.
    fn mount(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), AppletError>;

    /// Opens an asset of a [`MediaClass`] the manifest lists in `opens`.
    ///
    /// # Errors
    /// If the applet cannot show this asset.
    fn open(&mut self, asset: AssetHandle, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        let _ = (asset, ctx);
        Err(AppletError::new("this applet does not open assets"))
    }

    /// Moves to a place inside the applet, e.g. a Settings page. Returns
    /// `false` if the target is unknown.
    fn navigate(&mut self, target: &str) -> bool {
        let _ = target;
        false
    }

    /// Runs a named command such as `save`. Returns `false` if the applet
    /// does not have it.
    ///
    /// # Errors
    /// If the command fails.
    fn command(&mut self, name: &str, ctx: &mut AppletContext<'_>) -> Result<bool, AppletError> {
        let _ = (name, ctx);
        Ok(false)
    }

    /// Handles keyboard and other input the window manager did not consume,
    /// while the applet has focus.
    ///
    /// # Errors
    /// Any failure while handling the event.
    fn handle_input(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError>;

    /// `true` while the applet needs every key first, even global shortcuts
    /// (for example while recording a new shortcut).
    fn captures_keys(&self) -> bool {
        false
    }

    /// Draws the applet into `area` and handles pointer interaction there.
    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let _ = (ui, area, ctx);
    }

    /// A title for its tab, e.g. `photo.jxl`.
    fn title(&self, world: &World) -> Option<String> {
        let _ = world;
        None
    }

    /// Properties of what the applet shows, for the Inspector.
    fn inspect(&self, world: &World) -> Vec<PropertyGroup> {
        let _ = world;
        Vec::new()
    }

    /// Advances animations by `dt` seconds. Static applets keep the default,
    /// which never asks for frames.
    fn update(&mut self, _dt: f32, _ctx: &mut AppletContext<'_>) -> RenderDemand {
        RenderDemand::Sleep
    }

    /// Does timed work (feeding audio, a clock) without a frame being drawn.
    /// Called on every frame and whenever a requested wake time passes.
    fn tick(&mut self, now: Instant, ctx: &mut AppletContext<'_>) -> Tick {
        let _ = (now, ctx);
        Tick::default()
    }

    /// Called when the applet's tab closes or switches to another applet.
    fn unmount(&mut self, _ctx: &mut AppletContext<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn validates_applet_ids() {
        assert_eq!(
            AppletId::new("quick-notes").unwrap().as_str(),
            "quick-notes"
        );
        assert!(AppletId::new("cad2").is_ok());
        for bad in ["", "Quick", "1st", "with space", "under_score"] {
            assert!(AppletId::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ticks_combine() {
        let now = Instant::now();
        let soon = Tick {
            redraw: false,
            wake: Some(now),
        };
        let later = Tick {
            redraw: true,
            wake: Some(now + Duration::from_secs(1)),
        };
        assert_eq!(
            soon.combine(later),
            Tick {
                redraw: true,
                wake: Some(now)
            }
        );
        assert_eq!(Tick::default().combine(later).wake, later.wake);
    }
}
