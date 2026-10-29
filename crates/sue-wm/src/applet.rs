//! The applet contract every studio, tool and window implements.
//!
//! Nothing in the shell names an applet. Studios (Sue Paint, Sue Writer),
//! tools (the Image Viewer, the Converter) and the Settings window all
//! register at runtime and describe themselves through an
//! [`AppletManifest`] and their [`FileTemplate`]s: the Hub's boxes, the Sue
//! menu's Studios, Tools and New submenus and the command palette are all
//! read from them.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::Ui;
use sue_core::{Arena, AssetHandle, CoreError, Entity, ReversibleTransaction, TxnError, UndoStack};
use sue_platform::{AudioSink, FormFactor, TextureUpload};
use sue_rice::{HardwareProfile, RiceDeclaration, RiceSchema, SetRiceValue};

use crate::{Catalog, RiceEdit, Theme, Transport, Typing, WmError, WorkList, World};

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

/// Where an applet lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppletRole {
    /// A whole workspace for making something (Sue Paint, Sue Writer). It
    /// fills the page under the top bar and is listed under Studios in the
    /// Hub.
    Studio,
    /// A focused job on a file or an entity (the Image Viewer, the
    /// Converter). It opens like a studio, in a page of its own, and is
    /// listed under Tools in the Hub.
    Tool,
    /// A floating window over the page (Settings).
    Window,
}

impl AppletRole {
    /// `true` for the roles that fill the page: studios and tools.
    #[must_use]
    pub const fn is_page(self) -> bool {
        matches!(self, Self::Studio | Self::Tool)
    }
}

/// A kind of new file an applet makes. The Sue menu's New submenu shows
/// the categories, and each category its exact kinds.
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
    /// What it does, in a sentence (the Hub shows it on the applet's box).
    pub summary: String,
    /// Where it lives.
    pub role: AppletRole,
    /// A one-character icon from the bundled fonts.
    pub icon: String,
    /// Sort key among applets of the same role (lower first).
    pub order: i32,
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
    /// Read a file and open it in the studio or tool that takes it.
    OpenFile(PathBuf),
    /// An engine command such as `audio.start`, `file.open` or `rice.reload`.
    Command(String),
    /// Open Settings, optionally at a page.
    OpenSettings(Option<String>),
    /// Make a new file from an applet's template.
    Create {
        /// The applet.
        applet: AppletId,
        /// The template's key.
        key: String,
    },
    /// Open the command palette.
    Palette,
    /// Fill the page with a studio or tool.
    Open(AppletId),
    /// Go back to the Hub.
    Hub,
    /// Pin a studio or tool, so Sue opens straight into it at launch, or
    /// take the pin away (`None`: Sue opens on the Hub).
    Pin(Option<AppletId>),
    /// Ask for a file through the system's open or save dialog.
    PickFile(FilePick),
    /// The file a [`FilePick`] asked for.
    Picked {
        /// The request it answers.
        pick: FilePick,
        /// The chosen file (for a save, where to write it).
        path: PathBuf,
    },
    /// Show an applet's window (its `window_ui`) floating over the page.
    OpenWindow(AppletId),
    /// Run one of an applet's commands, as the palette does.
    AppletCommand {
        /// The applet.
        applet: AppletId,
        /// The command, e.g. `file.export.png`.
        command: String,
    },
    /// Open an entity in a studio or tool (it gets the entity through
    /// [`SueApplet::take_entity`] with the purpose `open`, then fills the
    /// page). How work moves between studios.
    OpenIn {
        /// The entity.
        entity: sue_core::EntityId,
        /// The studio or tool.
        applet: AppletId,
    },
    /// Hand an entity to an applet (`SueApplet::take_entity`), e.g. a
    /// picture to the Converter.
    SendEntity {
        /// Who gets it.
        to: AppletId,
        /// What it is for.
        purpose: String,
        /// The entity.
        entity: sue_core::EntityId,
    },
    /// Open the Sue menu.
    SueMenu,
    /// Hand a file to an applet, as if it had picked it.
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
    /// Put a file first in the work Sue remembers (the Hub's lists and
    /// every Open Recent): it was opened, or `saved`.
    Remember {
        /// The file.
        path: PathBuf,
        /// `true` when it was saved, `false` when opened.
        saved: bool,
    },
    /// Forget a file in the work Sue remembers.
    Forget(PathBuf),
    /// Bring back work Sue kept by itself (an autosave, by its id) and open
    /// it in its studio.
    Recover(String),
    /// Throw away work Sue kept by itself (an autosave, by its id).
    Discard(String),
}

/// A request for a file, answered by the system's own open or save dialog:
/// one to open, or where to save one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilePick {
    /// Who gets the file: an applet, or `None` for the shell and engine
    /// (opening a file, saving the project).
    pub requester: Option<AppletId>,
    /// What the file is for, handed back with it, e.g. `open` or `layer`.
    pub purpose: String,
    /// The dialog's title, e.g. `Open Picture`.
    pub title: String,
    /// Extensions it takes, without the dot; empty takes any file.
    pub extensions: Vec<String>,
    /// `Some(name)` to choose where to save, starting with that file name.
    pub save_as: Option<String>,
    /// The folder to start in.
    pub start: Option<PathBuf>,
    /// `true` to choose a folder instead of a file.
    pub folders: bool,
    /// The kinds of file to choose between ("save as type"), first the
    /// default. The chosen file's extension says which was picked.
    pub types: Vec<FileType>,
}

/// One kind of file a pick offers, e.g. `PNG picture` with `png`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileType {
    /// What people read.
    pub name: String,
    /// Its extensions, without the dot, the usual one first.
    pub extensions: Vec<String>,
}

impl FileType {
    /// A kind of file.
    #[must_use]
    pub fn new(name: &str, extensions: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            extensions: extensions.iter().map(|&ext| ext.to_owned()).collect(),
        }
    }

    /// `true` if `path` has one of its extensions.
    #[must_use]
    pub fn takes(&self, path: &Path) -> bool {
        path.extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .is_some_and(|ext| self.extensions.contains(&ext))
    }
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

    /// The same request, for a folder instead of a file.
    #[must_use]
    pub fn folder(mut self) -> Self {
        self.folders = true;
        self
    }

    /// The same request, starting in `folder`.
    #[must_use]
    pub fn starting_in(mut self, folder: Option<PathBuf>) -> Self {
        self.start = folder;
        self
    }

    /// The same request, offering these kinds of file (the first is the
    /// default). Their extensions are what it takes.
    #[must_use]
    pub fn types(mut self, types: Vec<FileType>) -> Self {
        for kind in &types {
            for ext in &kind.extensions {
                if !self.extensions.contains(ext) {
                    self.extensions.push(ext.clone());
                }
            }
        }
        self.types = types;
        self
    }

    /// The kind of file `path` is among the offered ones.
    #[must_use]
    pub fn type_of(&self, path: &Path) -> Option<&FileType> {
        self.types.iter().find(|kind| kind.takes(path))
    }

    /// The filters the system dialog offers: the kinds of file, else one
    /// filter of every extension, else none (any file).
    #[must_use]
    pub fn filters(&self) -> Vec<sue_platform::FileFilter> {
        if !self.types.is_empty() {
            return self
                .types
                .iter()
                .map(|kind| sue_platform::FileFilter {
                    name: kind.name.clone(),
                    extensions: kind.extensions.clone(),
                })
                .collect();
        }
        if self.extensions.is_empty() {
            Vec::new()
        } else {
            vec![sue_platform::FileFilter {
                name: "Files".to_owned(),
                extensions: self.extensions.clone(),
            }]
        }
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

/// The Sue menu's submenus every applet can add to.
pub mod sue_menu {
    /// Projects: open, save, save as.
    pub const PROJECT: &str = "Project";
    /// Files: open and convert.
    pub const FILE: &str = "File";
    /// The thing in focus, into another studio or tool.
    pub const SEND_TO: &str = "Send To";
}

/// One thing the Sue menu offers, for the thing in focus or for Sue as a
/// whole. Applets add them (`SueApplet::sue_actions`), so the menu grows
/// with every studio and tool, and none has to be named by the shell.
#[derive(Clone, Debug, PartialEq)]
pub struct SueAction {
    /// The submenu it sits in: one of [`sue_menu`], or a name of its own.
    pub menu: String,
    /// What people read.
    pub label: String,
    /// What it asks for.
    pub request: ShellRequest,
}

impl SueAction {
    /// An action in submenu `menu`.
    pub fn new(menu: impl Into<String>, label: impl Into<String>, request: ShellRequest) -> Self {
        Self {
            menu: menu.into(),
            label: label.into(),
            request,
        }
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
    /// The device class.
    pub form_factor: FormFactor,
    /// The active hardware profile.
    pub profile: HardwareProfile,
    /// This frame's time.
    pub now: Instant,
    /// Seconds since the last frame (clamped).
    pub dt: f32,
    /// The studio or tool filling the page, if any (`None` on the Hub and
    /// while one leaves).
    pub page: Option<AppletId>,
    /// How far the page has come in, 0 (the Hub) to 1 (all in place).
    /// Between the two it is entering or leaving.
    pub reveal: f32,
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
    /// The arena and the live rice.
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
    /// The files of late and the work Sue kept; change it with
    /// [`ShellRequest::Remember`] and [`ShellRequest::Forget`].
    pub work: &'a WorkList,
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

    /// Records rich text typed into `handle` (entity `id`) whose formatting
    /// component moved with it: `before` is that component as it was before
    /// `ops`, so one undo step puts back both.
    pub fn record_rich_typing(
        &mut self,
        handle: AssetHandle,
        id: sue_core::EntityId,
        ops: Vec<sue_core::txn::TextOp>,
        before: Box<dyn sue_core::Component>,
    ) {
        let now = self.view.now;
        self.typing
            .record_with(self.history, handle, id, ops, before, now);
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

/// The trait every studio, tool and window implements.
pub trait SueApplet: Send {
    /// Static facts about the applet.
    fn manifest(&self) -> AppletManifest;

    /// Riceable parameters the applet declares.
    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        Vec::new()
    }

    /// `true` if this studio or tool works on `entity`: a file opened
    /// anywhere goes to the first that takes it, and the Sue menu's Send To
    /// offers every other one.
    fn accepts(&self, entity: &Entity) -> bool {
        let _ = entity;
        false
    }

    /// The kinds of new files it makes.
    fn templates(&self) -> Vec<FileTemplate> {
        Vec::new()
    }

    /// Makes a new file from template `key` and opens it (asking for its
    /// page with [`ShellRequest::Open`]).
    fn create(&mut self, key: &str, cx: &mut AppletContext<'_>) {
        let _ = (key, cx);
    }

    /// The entity this studio or tool works on, for the Sue menu's Send To.
    fn focus(&self) -> Option<sue_core::EntityId> {
        None
    }

    /// Draws the applet's part of the top bar, right after the Sue button:
    /// its menus (with a [`crate::MenuBar`]), or on a phone a row of large
    /// buttons. `ui` covers that part of the bar.
    fn bar_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let _ = (ui, cx);
    }

    /// Draws a studio's or tool's page: `ui` covers everything between the
    /// top bar and the status bar.
    fn page_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let _ = (ui, cx);
    }

    /// Draws a window's content.
    fn window_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
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

    /// Receives the file a [`FilePick`] this applet sent asked for.
    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        let _ = (pick, path, cx);
    }

    /// What the Sue menu offers from this applet for `focus` (the entity
    /// the open studio or tool works on). `current` is `true` when this
    /// applet fills the page.
    fn sue_actions(&self, focus: Option<&Entity>, current: bool) -> Vec<SueAction> {
        let _ = (focus, current);
        Vec::new()
    }

    /// Receives an entity another applet sent with
    /// [`ShellRequest::SendEntity`], or one to open
    /// ([`ShellRequest::OpenIn`], purpose `open`).
    fn take_entity(
        &mut self,
        purpose: &str,
        entity: sue_core::EntityId,
        cx: &mut AppletContext<'_>,
    ) {
        let _ = (purpose, entity, cx);
    }

    /// `true` if this applet takes files like `path` itself when one is
    /// opened anywhere in Sue (a word processor's documents go to it as
    /// they are, not read into the arena first). It gets them through
    /// [`SueApplet::picked`] with the purpose `import`.
    fn opens_file(&self, path: &Path) -> bool {
        let _ = path;
        false
    }

    /// Chords a studio uses itself while it is open, which the shell then
    /// leaves to it (Sue Paint's `Tab` hides its palettes, as painting apps
    /// do, instead of opening the command palette).
    fn reserved_keys(&self) -> Vec<sue_rice::Keybind> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_hand_the_system_dialog_their_kinds_of_file() {
        let any = FilePick::open(None, "open", "Open File");
        assert!(any.filters().is_empty(), "any file");
        assert!(any.takes(Path::new("song.flac")));
        let project = FilePick::open(None, "open", "Open Project").extensions(&["sue"]);
        let filters = project.filters();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].extensions, ["sue"]);
        assert!(!project.takes(Path::new("a.png")));
        let save = FilePick::save(None, "save-as", "Save Picture As", "a.png").types(vec![
            FileType::new("PNG picture", &["png"]),
            FileType::new("JPEG picture", &["jpg", "jpeg"]),
        ]);
        let names: Vec<String> = save
            .filters()
            .into_iter()
            .map(|filter| filter.name)
            .collect();
        assert_eq!(names, ["PNG picture", "JPEG picture"]);
        assert_eq!(
            save.type_of(Path::new("b.JPEG"))
                .map(|kind| kind.name.as_str()),
            Some("JPEG picture")
        );
        assert!(save.takes(Path::new("c.jpg")));
        let folder = FilePick::open(None, "folder", "Choose a Folder").folder();
        assert!(folder.folders);
    }
}
