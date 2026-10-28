//! The applet contract every tool, utility and studio implements.
//!
//! Nothing in the shell names an applet. Canvas entity kinds (documents,
//! paint layers, audio, images, hex views), drawer utilities, the Settings
//! window and full-window studios (Sue Paint) all register at runtime and
//! describe themselves through an [`AppletManifest`]: the modes in the top
//! pill, the spawn buttons, the wedges of the marking menu, the timeline
//! lanes and the rail icons are all read from manifests.

use std::fmt;
use std::path::PathBuf;
use std::time::Instant;

use egui::{Ui, Vec2, vec2};
use sue_core::{Arena, AssetHandle, CoreError, Entity, ReversibleTransaction, TxnError, UndoStack};
use sue_platform::{AudioSink, FormFactor, TextureUpload};
use sue_rice::{HardwareProfile, RiceDeclaration, RiceSchema, SetRiceValue};

use crate::{
    Catalog, NewEntity, NodeId, RiceEdit, SceneNode, Theme, Transport, Typing, WmError, World,
};

/// An applet's unique id: `[a-z][a-z0-9-]*`, e.g. `paint`.
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

/// Broad kinds of assets, used to pick the applet that shows an opened file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaClass {
    /// A decoded raster image.
    Image,
    /// UTF-8 text.
    Text,
    /// Decoded audio.
    Audio,
    /// Raw bytes.
    Binary,
}

/// Where an applet lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppletRole {
    /// Draws one kind of entity on the canvas, with a floating pill.
    Node,
    /// A utility in the left drawer, picked on the left rail.
    LeftDrawer,
    /// A utility in the right drawer, picked on the right rail.
    RightDrawer,
    /// A floating window over the canvas (Settings).
    Window,
    /// A workspace that fills the window with its own menus and panels
    /// (Sue Paint), opened from its mode and left for the canvas again.
    Studio,
}

/// A mode in the top pill: switching to it frames an entity of the applet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeSpec {
    /// Button label, e.g. `Document`.
    pub label: String,
    /// `false` keeps it out of the top pill (it stays in the palette).
    pub in_pill: bool,
}

/// A way to create a new entity of the applet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnSpec {
    /// Top pill button, e.g. `+ Text`.
    pub button: String,
    /// Marking menu wedge and palette entry, e.g. `New Document`.
    pub label: String,
}

/// Static facts about an applet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppletManifest {
    /// Unique id.
    pub id: AppletId,
    /// Display name.
    pub name: String,
    /// What it does, in a sentence.
    pub summary: String,
    /// Where it lives.
    pub role: AppletRole,
    /// A one-character icon from the bundled fonts.
    pub icon: String,
    /// Sort key among applets of the same role (lower first).
    pub order: i32,
    /// Asset kinds the applet shows.
    pub opens: Vec<MediaClass>,
    /// Its mode, for node applets.
    pub mode: Option<ModeSpec>,
    /// How to create new entities, for node applets.
    pub spawn: Option<SpawnSpec>,
    /// Its timeline lane, e.g. `Audio Track`.
    pub lane: Option<String>,
}

impl AppletManifest {
    /// A manifest with the required facts; the rest is empty.
    ///
    /// # Panics
    /// If `id` is not a valid applet id (a programming error).
    #[must_use]
    pub fn new(id: &str, name: &str, role: AppletRole, icon: &str) -> Self {
        Self {
            id: AppletId::new(id).expect("built-in applet ids are valid"),
            name: name.to_owned(),
            summary: String::new(),
            role,
            icon: icon.to_owned(),
            order: 0,
            opens: Vec::new(),
            mode: None,
            spawn: None,
            lane: None,
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

/// Something an applet asks the shell or the engine for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellRequest {
    /// A message for the status bar.
    Notify(String),
    /// An error for the status bar.
    Fail(String),
    /// Read a file and put it on the canvas.
    OpenFile(PathBuf),
    /// An engine command such as `audio.start`, `file.open` or `rice.reload`.
    Command(String),
    /// Select a node (or nothing).
    Select(Option<NodeId>),
    /// Switch to the node's mode and frame it.
    Focus(NodeId),
    /// Frame a node, optionally at a given zoom.
    Frame {
        /// The node.
        node: NodeId,
        /// Zoom in percent instead of fitting (100 shows pixels 1:1).
        zoom_percent: Option<u32>,
    },
    /// Show or hide the timeline.
    Timeline(bool),
    /// Open Settings, optionally at a page.
    OpenSettings(Option<String>),
    /// Switch to a mode (`canvas` or a node applet id).
    Mode(String),
    /// Create a new entity of a node applet.
    Spawn(AppletId),
    /// Open the command palette.
    Palette,
    /// Frame everything.
    ResetCamera,
    /// Show a drawer applet.
    ShowDrawer(AppletId),
    /// Put an arena entity on the canvas, drawn by the applet that accepts it.
    Place {
        /// The entity.
        entity: AssetHandle,
        /// Switch to its mode and frame it.
        focus: bool,
    },
    /// Fill the window with a studio applet.
    OpenStudio(AppletId),
    /// Leave the studio for the canvas.
    CloseStudio,
}

/// What an applet's tick asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// Redraw once.
    pub repaint: bool,
    /// Keep drawing every frame (something moves).
    pub animating: bool,
    /// Run the tick again at this instant.
    pub wake: Option<Instant>,
}

impl Tick {
    /// Combines two ticks.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            repaint: self.repaint || other.repaint,
            animating: self.animating || other.animating,
            wake: match (self.wake, other.wake) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
        }
    }
}

/// What the shell shows right now, for applets to read.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellView {
    /// The selected node.
    pub selected: Option<NodeId>,
    /// The canvas zoom (1.0 is 100%).
    pub zoom: f32,
    /// The active mode: `canvas` or a node applet id.
    pub mode: String,
    /// `true` while the timeline is open.
    pub timeline_open: bool,
    /// The timeline playhead, in seconds.
    pub playhead: f64,
    /// `true` while the timeline plays.
    pub timeline_playing: bool,
    /// The device class.
    pub form_factor: FormFactor,
    /// The active hardware profile.
    pub profile: HardwareProfile,
    /// This frame's time.
    pub now: Instant,
    /// Seconds since the last frame (clamped).
    pub dt: f32,
    /// The studio filling the window, if any.
    pub studio: Option<AppletId>,
    /// `true` in the desktop window, where native dialogs can open;
    /// `false` in headless runs.
    pub interactive: bool,
}

/// One entry of the command palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteEntry {
    /// What the user reads, e.g. `Paint: Export .jxl`.
    pub label: String,
    /// The command an applet's [`SueApplet::command`] runs.
    pub command: String,
    /// The shortcut, if any.
    pub hint: String,
}

/// Everything an applet may touch during a call.
pub struct AppletContext<'a> {
    /// The arena, the live rice and the canvas scene.
    pub world: &'a mut World,
    /// The global undo tree.
    pub history: &'a mut UndoStack<World>,
    /// Typing waiting to become one undo step.
    pub typing: &'a mut Typing,
    /// Requests for the shell and the engine.
    pub requests: &'a mut Vec<ShellRequest>,
    /// Settings pages, live choices and system tables.
    pub catalog: &'a Catalog,
    /// The theme.
    pub theme: &'a Theme,
    /// Texture uploads for images.
    pub textures: &'a mut dyn TextureUpload,
    /// The audio output, once opened.
    pub audio: Option<&'a mut (dyn AudioSink + 'static)>,
    /// The shared audio transport.
    pub transport: &'a mut Transport,
    /// What the shell shows.
    pub view: ShellView,
}

impl AppletContext<'_> {
    /// The live rice.
    #[must_use]
    pub fn rice(&self) -> &RiceSchema {
        &self.world.rice
    }

    /// The arena.
    #[must_use]
    pub fn arena(&self) -> &Arena {
        &self.world.arena
    }

    /// A node of the scene.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&SceneNode> {
        self.world.scene.get(id)
    }

    /// The entity a node shows.
    #[must_use]
    pub fn entity(&self, id: NodeId) -> Option<&Entity> {
        let node = self.world.scene.get(id)?;
        self.world.arena.get(node.entity).ok()
    }

    /// Applies `txn` and records it in the global history, closing any open
    /// typing group first.
    ///
    /// # Errors
    /// The transaction's own error; nothing is recorded then.
    pub fn apply<X>(&mut self, txn: X) -> Result<sue_core::NodeId, TxnError>
    where
        X: ReversibleTransaction<World> + 'static,
    {
        self.typing.flush(self.history);
        self.history.apply(self.world, txn)
    }

    /// Records `txn`, whose effect already happened in place (a finished
    /// paint stroke), closing any open typing group first.
    pub fn record<X>(&mut self, txn: X) -> sue_core::NodeId
    where
        X: ReversibleTransaction<World> + 'static,
    {
        self.typing.flush(self.history);
        self.history.record(txn)
    }

    /// Applies a rice edit (see [`Self::apply`]).
    ///
    /// # Errors
    /// The transaction's own error.
    pub fn edit_rice(&mut self, txn: SetRiceValue) -> Result<sue_core::NodeId, TxnError> {
        self.apply(RiceEdit(txn))
    }

    /// Records text a text field already typed into `handle`.
    pub fn record_typing(&mut self, handle: AssetHandle, ops: Vec<sue_core::txn::TextOp>) {
        let now = self.view.now;
        self.typing.record(self.history, handle, ops, now);
    }

    /// A message for the status bar.
    pub fn notify(&mut self, text: impl Into<String>) {
        self.requests.push(ShellRequest::Notify(text.into()));
    }

    /// An error for the status bar.
    pub fn fail(&mut self, text: impl Into<String>) {
        self.requests.push(ShellRequest::Fail(text.into()));
    }

    /// Asks the shell or engine for something.
    pub fn request(&mut self, request: ShellRequest) {
        self.requests.push(request);
    }

    /// The audio output, if open.
    pub fn sink(&mut self) -> Option<&mut (dyn AudioSink + 'static)> {
        self.audio.as_deref_mut()
    }
}

/// The trait every tool, utility and studio implements.
pub trait SueApplet: Send {
    /// Static facts about the applet.
    fn manifest(&self) -> AppletManifest;

    /// Riceable parameters the applet declares.
    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        Vec::new()
    }

    /// `true` if this node applet shows `entity` (for opened files).
    fn accepts(&self, entity: &Entity) -> bool {
        let _ = entity;
        false
    }

    /// The canvas size of a node showing `entity`.
    fn node_size(&self, entity: &Entity) -> Vec2 {
        let _ = entity;
        vec2(480.0, 320.0)
    }

    /// A new entity for the spawn buttons.
    fn new_entity(&mut self, cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        let _ = cx;
        None
    }

    /// Draws node `node`: `ui` covers its content area, in canvas units.
    fn node_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let _ = (ui, node, cx);
    }

    /// Draws the tools of the floating selection pill for `node`.
    fn pill_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let _ = (ui, node, cx);
    }

    /// How long a node lasts on the timeline, in seconds.
    fn duration(&self, node: &SceneNode, world: &World) -> f64 {
        let _ = (node, world);
        4.0
    }

    /// Draws the drawer.
    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let _ = (ui, cx);
    }

    /// Draws the window content.
    fn window_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let _ = (ui, cx);
    }

    /// Draws a studio: `ui` covers the whole window above the status bar.
    fn studio_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let _ = (ui, cx);
    }

    /// Timed work between frames.
    fn tick(&mut self, cx: &mut AppletContext<'_>) -> Tick {
        let _ = cx;
        Tick::default()
    }

    /// Entries for the command palette.
    fn commands(&self) -> Vec<PaletteEntry> {
        Vec::new()
    }

    /// Runs a command. Returns `false` if the applet does not have it.
    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        let _ = (name, cx);
        false
    }

    /// Moves to a place inside the applet, e.g. a Settings page.
    fn navigate(&mut self, target: &str) -> bool {
        let _ = target;
        false
    }

    /// `true` while the applet needs every key first (recording a shortcut).
    fn captures_keys(&self) -> bool {
        false
    }
}
