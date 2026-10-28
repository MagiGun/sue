//! The applet contract every tool, utility and studio implements.
//!
//! Nothing in the shell names an applet. Canvas entity kinds (documents,
//! paint layers, audio, images, hex views), drawer utilities, the Settings
//! window and full-window studios (Sue Paint) all register at runtime and
//! describe themselves through an [`AppletManifest`] and their
//! [`FileTemplate`]s: the modes in the top pill, the New File wheels of the
//! marking menu, the timeline lanes and the rail icons are all read from
//! them.

use std::fmt;
use std::path::{Path, PathBuf};
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

/// A kind of new file an applet makes. The marking menu's New File wheel
/// shows the categories; each category's wheel shows its exact kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTemplate {
    /// The kind of file, e.g. `Picture`.
    pub category: String,
    /// The exact kind, e.g. `Square Post`.
    pub label: String,
    /// What the applet's [`SueApplet::create`] gets back.
    pub key: String,
}

impl FileTemplate {
    /// A template.
    pub fn new(
        category: impl Into<String>,
        label: impl Into<String>,
        key: impl Into<String>,
    ) -> Self {
        Self {
            category: category.into(),
            label: label.into(),
            key: key.into(),
        }
    }
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
    /// Its mode, for node applets and studios.
    pub mode: Option<ModeSpec>,
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
    /// Make a new file from an applet's template.
    Create {
        /// The applet.
        applet: AppletId,
        /// The template's key.
        key: String,
    },
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
    /// Show or hide the Hub, the list of studios.
    Hub(bool),
    /// Ask for a file through the File Manager.
    PickFile(FilePick),
    /// The file a [`FilePick`] asked for.
    Picked {
        /// The request it answers.
        pick: FilePick,
        /// The chosen file (for a save, where to write it).
        path: PathBuf,
    },
    /// Show an applet's window (its `window_ui`) floating over the screen.
    OpenWindow(AppletId),
    /// Run one of an applet's commands, as the palette does.
    AppletCommand {
        /// The applet.
        applet: AppletId,
        /// The command, e.g. `player.next`.
        command: String,
    },
    /// Hand a file to an applet, as if it had picked it (`Add to Player`).
    SendFile {
        /// Who gets it.
        to: AppletId,
        /// What it is for, e.g. `import`.
        purpose: String,
        /// The file or folder.
        path: PathBuf,
    },
    /// Close the floating window.
    CloseWindow,
    /// A file was written: say where, and show it in its folder on a click.
    Saved {
        /// The file.
        path: PathBuf,
        /// What it is, e.g. `JPEG XL, lossless, 1.2 MiB`.
        note: String,
    },
}

/// A request for a file, answered by the applet that picks files (the File
/// Manager) in a window: one to open, or where to save one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilePick {
    /// Who gets the file: an applet, or `None` for the shell and engine
    /// (opening on the canvas, saving the project).
    pub requester: Option<AppletId>,
    /// What the file is for, handed back with it, e.g. `open` or `layer`.
    pub purpose: String,
    /// The window's title, e.g. `Open Picture`.
    pub title: String,
    /// Extensions it takes, without the dot; empty takes any file.
    pub extensions: Vec<String>,
    /// `Some(name)` to choose where to save, starting with that file name.
    pub save_as: Option<String>,
    /// The folder to start in.
    pub start: Option<PathBuf>,
    /// `true` when a whole folder may be chosen too.
    pub folders: bool,
}

impl FilePick {
    /// A request for a file to open.
    #[must_use]
    pub fn open(requester: Option<AppletId>, purpose: &str, title: &str) -> Self {
        Self {
            requester,
            purpose: purpose.to_owned(),
            title: title.to_owned(),
            ..Self::default()
        }
    }

    /// A request for where to save `name`.
    #[must_use]
    pub fn save(requester: Option<AppletId>, purpose: &str, title: &str, name: &str) -> Self {
        Self {
            save_as: Some(name.to_owned()),
            ..Self::open(requester, purpose, title)
        }
    }

    /// The same request, taking only these extensions.
    #[must_use]
    pub fn extensions(mut self, extensions: &[&str]) -> Self {
        self.extensions = extensions.iter().map(|&ext| ext.to_owned()).collect();
        self
    }

    /// The same request, also taking a whole folder.
    #[must_use]
    pub fn or_folder(mut self) -> Self {
        self.folders = true;
        self
    }

    /// The same request, starting in `folder`.
    #[must_use]
    pub fn starting_in(mut self, folder: Option<PathBuf>) -> Self {
        self.start = folder;
        self
    }

    /// `true` when `path` has one of the extensions (or any are taken).
    #[must_use]
    pub fn takes(&self, path: &Path) -> bool {
        self.extensions.is_empty()
            || path
                .extension()
                .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
                .is_some_and(|ext| self.extensions.contains(&ext))
    }
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
    /// The studio filling the window, if any (`None` while one leaves).
    pub studio: Option<AppletId>,
    /// How far the studio has come in, 0 (the canvas) to 1 (all panels in
    /// place). Between the two it is entering or leaving.
    pub studio_reveal: f32,
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

    /// Undoes (or, with `redo`, redoes) the last change anywhere, as
    /// `Ctrl+Z` does. Returns `false` when there was nothing to undo.
    ///
    /// # Errors
    /// If the change could not be undone.
    pub fn undo(&mut self, redo: bool) -> Result<bool, TxnError> {
        self.typing.flush(self.history);
        if redo {
            self.history.redo(self.world)
        } else {
            self.history.undo(self.world)
        }
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

    /// Tells where a file was written (`note` says what it is), and offers
    /// to show it in its folder.
    pub fn saved(&mut self, path: impl Into<PathBuf>, note: impl Into<String>) {
        self.requests.push(ShellRequest::Saved {
            path: path.into(),
            note: note.into(),
        });
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

    /// `true` if this node applet shows `entity` (for opened files), or this
    /// studio works on it (the shell zooms into its card when the studio
    /// opens).
    fn accepts(&self, entity: &Entity) -> bool {
        let _ = entity;
        false
    }

    /// The canvas size of a node showing `entity`.
    fn node_size(&self, entity: &Entity) -> Vec2 {
        let _ = entity;
        vec2(480.0, 320.0)
    }

    /// A new entity, when a mode needs one and none exists.
    fn new_entity(&mut self, cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        let _ = cx;
        None
    }

    /// The kinds of new files it makes.
    fn templates(&self) -> Vec<FileTemplate> {
        Vec::new()
    }

    /// Makes a new file from template `key`. A node applet returns the
    /// entity, which the shell places where the menu opened. An applet that
    /// makes something larger (a picture with layers) makes it itself,
    /// places it with [`ShellRequest::Place`] and returns `None`.
    fn create(&mut self, key: &str, cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        let _ = (key, cx);
        None
    }

    /// The entity a studio is working on, so the shell can zoom out of its
    /// card when the studio closes.
    fn studio_focus(&self) -> Option<sue_core::EntityId> {
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

    /// Takes on a [`FilePick`] in its window. Returns `false` if the applet
    /// does not pick files.
    fn begin_pick(&mut self, pick: &FilePick) -> bool {
        let _ = pick;
        false
    }

    /// Receives the file a [`FilePick`] this applet sent asked for.
    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        let _ = (pick, path, cx);
    }

    /// The floating window's title, when not the applet's name.
    fn window_title(&self) -> Option<String> {
        None
    }
}
