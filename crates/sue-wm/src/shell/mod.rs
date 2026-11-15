//! The shell: Sue's whole window as one immediate-mode egui frame.
//!
//! A top bar runs across the window. The Sue button sits once, in its top
//! left corner, and opens the Sue menu: the Hub, every studio and tool,
//! new files, projects, Send To, the command palette and Settings. The
//! studio or tool filling the page draws its own menus right after it
//! ([`SueApplet::bar_ui`](crate::SueApplet::bar_ui)). Under the bar is the
//! page: the Hub, a box for every studio and tool with a pin that makes Sue
//! open straight into it, or the studio or tool itself. A status bar runs
//! along the bottom; the command palette and the Settings window float over
//! everything. Nothing here names an applet: the Hub, the menus and the
//! palette are read from the registered manifests.
//!
//! Motion: a page comes in over the Hub and goes back out on a
//! [`SpringDamper`] stepped with the real, clamped frame time (studios
//! slide their palettes in with it). While any spring moves the shell calls
//! `request_repaint`, so frames come at the display rate with no input;
//! once all settle (plus a short grace period) the loop sleeps at 0 FPS.

use std::path::PathBuf;
use std::time::Duration;

use sue_platform::time::Instant;

use egui::{Align, Id, LayerId, Layout, Order, Rect, Sense, Ui, UiBuilder, vec2};
use sue_core::{ReversibleTransaction, UndoStack};
use sue_platform::{AudioSink, FormFactor, FramePacing, SafeAreaInsets, TextureUpload};
use sue_rice::{HardwareProfile, RiceSchema, RiceValue, SetRiceValue};

use crate::palette::Palette;
use crate::{
    AppletContext, AppletId, AppletRole, Applets, Catalog, FilePick, FrameScheduler, LayoutRequest,
    MenuBar, MenuItem, RiceEdit, ShellRequest, ShellView, SpringDamper, SpringMode, Theme,
    Transport, Typing, WmConfig, WorkList, World, solve_layout,
};

mod bar;
mod command_palette;
mod hub;
mod keys;
mod quit;
mod status;
mod sue_menu;
mod window;

pub use hub::{HUB_FILES, HUB_HOME, HubButton, HubRow, WorkShelf};

/// The rice key of the Hub's pin: the studio or tool Sue opens into at
/// launch, empty for the Hub.
pub const PIN_KEY: &str = "launchpad.pinned";

/// The rice key of the layout the whole app uses: `auto`, `desktop`,
/// `tablet` or `phone`.
pub const INTERFACE_KEY: &str = "display.interface";

/// The rice key that keeps the phone layout on a computer to a
/// phone-wide column.
pub const PHONE_FRAME_KEY: &str = "display.phone_frame";

/// How wide the phone-shaped column is, in points (a common phone's).
const PHONE_FRAME_WIDTH: f32 = 412.0;

/// A layout as the Sue menu names it.
fn layout_name(form_factor: FormFactor) -> String {
    match form_factor {
        FormFactor::Desktop => tr!("layout-desktop"),
        FormFactor::Tablet => tr!("layout-tablet"),
        FormFactor::Mobile => tr!("layout-phone"),
    }
}

/// A hardware profile as people read it, e.g. `Legacy Low Spec`, in the
/// language in use (as Settings names the choice).
#[must_use]
pub fn profile_label(profile: HardwareProfile) -> String {
    let english = profile
        .rice_name()
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ");
    let key = format!("profile-hardware.{}", profile.rice_name());
    sue_lang::text_or("rice", &key, &english)
}

/// The studio or tool pinned in `rice`, if it is installed.
#[must_use]
pub fn pinned(rice: &RiceSchema, applets: &Applets) -> Option<AppletId> {
    let id = AppletId::new(rice.get_str(PIN_KEY)?.trim()).ok()?;
    applets
        .manifest(&id)
        .is_some_and(|manifest| manifest.role.is_page())
        .then_some(id)
}

/// Everything outside the shell that a frame reads and changes.
pub struct ShellIo<'a> {
    /// The arena and the live rice.
    pub world: &'a mut World,
    /// The undo tree of the studio or tool on the page and its document (or
    /// Sue's own on the Hub).
    pub history: &'a mut UndoStack<World>,
    /// Sue's own undo tree, for settings, pins and the language, when it
    /// is not [`Self::history`].
    pub sue_history: Option<&'a mut UndoStack<World>>,
    /// Typing waiting to become an undo step.
    pub typing: &'a mut Typing,
    /// The live applets.
    pub applets: &'a mut Applets,
    /// Settings pages and tables.
    pub catalog: &'a Catalog,
    /// Image uploads.
    pub textures: &'a mut dyn TextureUpload,
    /// The audio output, once opened.
    pub audio: Option<&'a mut (dyn AudioSink + 'static)>,
    /// The audio transport.
    pub transport: &'a mut Transport,
    /// Requests for the engine.
    pub requests: &'a mut Vec<ShellRequest>,
    /// The files of late and the work Sue kept, for the Hub's lists.
    pub work: &'a WorkList,
    /// The files brought in, written out and copied in this run.
    pub transfers: &'a mut crate::Transfers,
    /// Frames per second while animating; `None` when idle.
    pub fps: Option<u32>,
}

impl ShellIo<'_> {
    /// Applies a change of Sue's own (a setting, a pin, the language) and
    /// records it in Sue's history, not a document's.
    ///
    /// # Errors
    /// The change's own error; nothing is recorded then.
    pub fn apply_sue(
        &mut self,
        txn: impl ReversibleTransaction<World> + 'static,
    ) -> Result<sue_core::NodeId, sue_core::TxnError> {
        let stack = match self.sue_history.as_deref_mut() {
            Some(sue) => sue,
            None => &mut *self.history,
        };
        stack.apply(self.world, txn)
    }
}

fn split<'b>(
    io: &'b mut ShellIo<'_>,
    theme: &'b Theme,
    view: ShellView,
) -> (&'b mut Applets, AppletContext<'b>) {
    let cx = AppletContext {
        world: &mut *io.world,
        history: &mut *io.history,
        sue_history: io.sue_history.as_deref_mut(),
        typing: &mut *io.typing,
        requests: &mut *io.requests,
        catalog: io.catalog,
        theme,
        textures: &mut *io.textures,
        audio: io.audio.as_deref_mut(),
        transport: &mut *io.transport,
        work: io.work,
        transfers: &mut *io.transfers,
        view,
    };
    (&mut *io.applets, cx)
}

/// The shell's state between frames.
pub struct Shell {
    config: WmConfig,
    theme: Theme,
    applied_theme: Option<Theme>,
    profile: HardwareProfile,
    form_factor: FormFactor,
    safe_area: SafeAreaInsets,
    scheduler: FrameScheduler,
    /// The studio or tool asked for; `None` is the Hub.
    page: Option<AppletId>,
    /// The studio or tool drawn, also while it leaves for the Hub.
    shown: Option<AppletId>,
    /// How far the shown page has come in over the Hub, 0 to 1.
    reveal: SpringDamper,
    palette: Option<Palette>,
    window: Option<AppletId>,
    window_appear: SpringDamper,
    /// The Sue button's menu: which submenus are open.
    sue_menu: MenuBar,
    /// The Sue menu, built for the context it opened in.
    sue_items: Vec<MenuItem>,
    /// What each of its items (`sue:<index>`) runs, and whether it shows a
    /// check mark.
    sue_runs: Vec<(ShellRequest, bool)>,
    presets: Vec<sue_rice::ThemePreset>,
    interactive: bool,
    message: Option<(String, bool, Instant)>,
    message_path: Option<PathBuf>,
    last_frame: Option<Instant>,
    last_change: Option<Instant>,
    animating: bool,
    page_rect: Rect,
    /// Where the last frame drew the Sue button.
    sue_button: Rect,
    /// Where the last frame drew the floating window (Settings).
    window_rect: Rect,
    /// Where the last frame drew the Hub's boxes and their pins.
    hub_boxes: Vec<(AppletId, Rect, Rect)>,
    /// The window's title as last sent.
    window_title: String,
    /// Sue draws the main window's frame itself (see
    /// `sue_platform::own_frames`).
    own_frame: bool,
    /// Quitting: the work not saved that was let go without saving. While
    /// it is `Some`, each other piece of work not saved is asked about.
    quitting: Option<Vec<(AppletId, String)>>,
    /// Where the last frame drew the rows (and buttons) of the Hub's lists.
    hub_rows: Vec<(HubRow, Rect)>,
    /// The kept work whose Discard was clicked once and asks again.
    discard_armed: Option<String>,
    booted: bool,
    /// The Hub's place [`ShellRequest::HubPlace`] asked for last (`None`:
    /// its Home).
    hub_place: Option<String>,
    /// The languages in use when the fonts last covered them, and the
    /// characters they (and every language's name) need.
    scripts: (Vec<String>, String),
    /// The Hub's places, browser, menus and buttons.
    hub: hub::HubState,
    /// The notices on screen (each [`notify`](Self::notify) puts one up).
    toasts: crate::ui::Toasts,
    /// The notices of saved files still showing, each with the file its
    /// button shows, the newest last.
    revealed: Vec<(crate::ui::ToastId, PathBuf)>,
}

impl std::fmt::Debug for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell")
            .field("page", &self.page)
            .field("shown", &self.shown)
            .field("window", &self.window)
            .finish_non_exhaustive()
    }
}

/// Every applet's file templates, in applet order (each in its applet's
/// language).
fn templates(applets: &Applets, rice: &RiceSchema) -> Vec<(AppletId, crate::FileTemplate)> {
    applets
        .iter()
        .flat_map(|(manifest, applet)| {
            let _language = sue_lang::scope(&crate::applet_language(rice, &manifest.id));
            applet
                .templates()
                .into_iter()
                .map(|template| (manifest.id.clone(), template))
        })
        .collect()
}

impl Shell {
    /// A shell on the Hub.
    #[must_use]
    pub fn new(
        config: WmConfig,
        theme: Theme,
        profile: HardwareProfile,
        display_hz: u32,
        form_factor: FormFactor,
    ) -> Self {
        let mode = SpringMode::for_profile(profile, config.instant_transitions);
        let spring = |position| SpringDamper::new(config.spring, mode, position);
        let mut scheduler = FrameScheduler::new(profile, display_hz);
        scheduler.set_frame_cap(config.frame_rate_cap);
        Self {
            page: None,
            shown: None,
            reveal: spring(0.0),
            palette: None,
            window: None,
            window_appear: spring(0.0),
            sue_menu: MenuBar::new(),
            sue_items: Vec::new(),
            sue_runs: Vec::new(),
            presets: sue_rice::builtin_presets(),
            interactive: false,
            message: None,
            message_path: None,
            last_frame: None,
            last_change: None,
            animating: true,
            page_rect: Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 720.0)),
            sue_button: Rect::NOTHING,
            window_rect: Rect::NOTHING,
            hub_boxes: Vec::new(),
            window_title: "Sue".to_owned(),
            own_frame: false,
            quitting: None,
            hub_rows: Vec::new(),
            discard_armed: None,
            booted: false,
            hub_place: None,
            scripts: (Vec::new(), String::new()),
            hub: hub::HubState::default(),
            toasts: crate::ui::Toasts::new("sue-notices"),
            revealed: Vec::new(),
            config,
            theme,
            applied_theme: None,
            profile,
            form_factor,
            safe_area: SafeAreaInsets::default(),
            scheduler,
        }
    }

    /// The config.
    #[must_use]
    pub fn config(&self) -> &WmConfig {
        &self.config
    }

    /// The theme.
    #[must_use]
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// The active hardware profile.
    #[must_use]
    pub fn profile(&self) -> HardwareProfile {
        self.profile
    }

    /// Frames per second while animating.
    #[must_use]
    pub fn target_hz(&self) -> u32 {
        self.scheduler.target_hz()
    }

    /// The page area of the last frame, in screen points.
    #[must_use]
    pub fn page_rect(&self) -> Rect {
        self.page_rect
    }

    /// The layout every studio and tool draws: the device's own, or the
    /// one chosen in the Sue menu's Layout.
    #[must_use]
    pub fn form_factor(&self) -> FormFactor {
        self.form_factor.chosen(&self.config.interface)
    }

    /// The device's own form factor, as the system detected it.
    #[must_use]
    pub fn device_form_factor(&self) -> FormFactor {
        self.form_factor
    }

    /// Starts quitting: each studio's work not saved is asked about first
    /// (Save, Do Not Save, Cancel), then `app.quit.checked` is requested.
    pub fn begin_quit(&mut self) {
        if self.quitting.is_none() {
            self.quitting = Some(Vec::new());
        }
        self.touch();
    }

    /// `true` while quitting asks about work not saved.
    #[must_use]
    pub fn quitting(&self) -> bool {
        self.quitting.is_some()
    }

    /// Draws the main window's frame (and those of Sue's windows of their
    /// own) itself, the system's frame being off.
    pub fn set_own_frame(&mut self, own: bool) {
        self.own_frame = own;
    }

    /// Where the app draws: the whole window, or a phone-wide column in
    /// its middle when a computer shows the phone layout phone shaped.
    #[must_use]
    pub fn frame(&self, window: Rect) -> Rect {
        let width = PHONE_FRAME_WIDTH.min(window.width());
        if self.config.phone_frame
            && !self.form_factor.is_touch()
            && self.form_factor() == FormFactor::Mobile
        {
            Rect::from_center_size(window.center(), vec2(width, window.height()))
        } else {
            window
        }
    }

    /// Where the last frame drew the floating window (Settings), if one is
    /// open.
    #[must_use]
    pub fn window_rect(&self) -> Option<Rect> {
        self.window.as_ref().map(|_| self.window_rect)
    }

    /// Where the last frame drew the Sue button.
    #[must_use]
    pub fn sue_button(&self) -> Rect {
        self.sue_button
    }

    /// Where the last frame drew each box of the Hub, and its pin (empty
    /// when a page covered the Hub).
    #[must_use]
    pub fn hub_boxes(&self) -> &[(AppletId, Rect, Rect)] {
        &self.hub_boxes
    }

    /// Where the last frame drew the rows of the Hub's lists and the
    /// buttons of kept work (empty when a page covered the Hub).
    #[must_use]
    pub fn hub_rows(&self) -> &[(HubRow, Rect)] {
        &self.hub_rows
    }

    /// Where the last frame drew the Hub's other buttons: its places, New,
    /// Open, the buttons of Your work, the cards of Jump back in, and the
    /// top bar's search, Transfers and Settings (empty when a page covered
    /// the Hub).
    #[must_use]
    pub fn hub_buttons(&self) -> &[(HubButton, Rect)] {
        &self.hub.buttons
    }

    /// The Hub's place [`ShellRequest::HubPlace`] asked for last, by its
    /// id (`None`: the Hub's Home, as [`ShellRequest::Hub`] leaves it).
    #[must_use]
    pub fn hub_place(&self) -> Option<&str> {
        self.hub_place.as_deref()
    }

    /// The studio or tool filling the page (`None` on the Hub, and as soon
    /// as one starts leaving for it).
    #[must_use]
    pub fn page(&self) -> Option<&AppletId> {
        self.page.as_ref()
    }

    /// `true` while the Hub is the page.
    #[must_use]
    pub fn hub_open(&self) -> bool {
        self.page.is_none()
    }

    /// How far the page has come in over the Hub, 0 to 1.
    #[must_use]
    pub fn reveal(&self) -> f32 {
        if self.shown.is_some() {
            self.reveal.position().clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Marks the shell as running in the desktop window, where applets may
    /// open native dialogs.
    pub fn set_interactive(&mut self, interactive: bool) {
        self.interactive = interactive;
    }

    /// The open window applet, if any.
    #[must_use]
    pub fn window(&self) -> Option<&AppletId> {
        self.window.as_ref()
    }

    /// `true` while the palette is open.
    #[must_use]
    pub fn palette_open(&self) -> bool {
        self.palette.is_some()
    }

    /// `true` while the Sue menu is open.
    #[must_use]
    pub fn sue_menu_open(&self) -> bool {
        self.sue_menu.is_open()
    }

    /// `true` while any spring moves.
    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// Insets for notches and system bars.
    pub fn set_safe_area(&mut self, safe_area: SafeAreaInsets) {
        self.safe_area = safe_area;
    }

    fn spring_mode(&self) -> SpringMode {
        SpringMode::for_profile(self.profile, self.config.instant_transitions)
    }

    /// Follows a rice change.
    pub fn apply_config(&mut self, config: WmConfig, theme: Theme, profile: HardwareProfile) {
        self.profile = profile;
        self.scheduler.set_profile(profile);
        self.scheduler.set_frame_cap(config.frame_rate_cap);
        self.config = config;
        self.theme = theme;
        let mode = self.spring_mode();
        let params = self.config.spring;
        for spring in [&mut self.reveal, &mut self.window_appear] {
            spring.set_params(params);
            spring.set_mode(mode);
        }
        self.touch();
    }

    /// Marks a change: keep drawing through the grace period.
    pub fn touch(&mut self) {
        self.last_change = Some(Instant::now());
    }

    /// Shows a status message.
    pub fn notify(&mut self, text: impl Into<String>, error: bool) {
        let text = text.into();
        let stay = self.config.message_duration;
        self.push_notice(&text, error, stay, None);
        self.message = Some((text, error, Instant::now() + stay));
        self.message_path = None;
        self.touch();
    }

    /// Says where a file was written, for long enough to read the folder;
    /// a click on the message shows the file in its folder, where the
    /// system has a file manager. A file a phone's picker chose reads as
    /// what was chosen (`Picture.png in Download`), not Sue's copy.
    pub fn notify_saved(&mut self, path: &std::path::Path, note: &str) {
        let (name, folder) = match sue_platform::shown_place(path) {
            Some(shown) => (shown.name, shown.folder),
            None => (
                path.file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                Some(
                    path.parent()
                        .map_or_else(String::new, |folder| folder.display().to_string()),
                ),
            ),
        };
        let note = if note.is_empty() {
            String::new()
        } else {
            format!(" ({note})")
        };
        let folder = folder.map_or_else(String::new, |folder| {
            tr!("saved-in-folder", folder = folder)
        });
        // Where the system has a file manager, the notice's button shows
        // the file in it.
        let reveal = sue_platform::file_manager().is_some();
        let text = tr!("saved-file", name = name, note = note, folder = folder);
        let stay = self.config.message_duration.max(Duration::from_secs(10));
        self.push_notice(&text, false, stay, reveal.then(|| path.to_path_buf()));
        self.message = Some((text, false, Instant::now() + stay));
        self.message_path = reveal.then(|| path.to_path_buf());
        self.touch();
    }

    /// The file the status message is about, if it is about one.
    #[must_use]
    pub fn message_path(&self) -> Option<&std::path::Path> {
        self.message_path.as_deref()
    }

    /// The status message, if any.
    #[must_use]
    pub fn message(&self) -> Option<(&str, bool)> {
        self.message
            .as_ref()
            .map(|(text, error, _)| (text.as_str(), *error))
    }

    /// What the loop should do next.
    #[must_use]
    pub fn pacing(&self, now: Instant) -> FramePacing {
        let target_hz = self.scheduler.target_hz();
        if self.animating {
            return FramePacing::Animate { target_hz };
        }
        if self
            .last_change
            .is_some_and(|change| now < change + self.config.settle_grace)
        {
            return FramePacing::Animate { target_hz };
        }
        self.message
            .as_ref()
            .map(|(_, _, until)| *until)
            .map_or(FramePacing::Sleep, FramePacing::WakeAt)
    }

    /// Clears an expired message. Returns `true` if the window must redraw.
    pub fn expire(&mut self, now: Instant) -> bool {
        if self
            .message
            .as_ref()
            .is_some_and(|(_, _, until)| *until <= now)
        {
            self.message = None;
            self.message_path = None;
            return true;
        }
        false
    }

    fn view(&self, now: Instant, dt: f32) -> ShellView {
        ShellView {
            form_factor: self.form_factor(),
            profile: self.profile,
            now,
            dt,
            page: self.page.clone(),
            reveal: self.reveal(),
            interactive: self.interactive,
        }
    }

    // ----- Windows and overlays ---------------------------------------------

    /// Opens the window applet `id` (Settings).
    pub fn open_window(&mut self, id: AppletId) {
        self.dismiss_overlays();
        self.window = Some(id);
        self.window_appear.set_target(1.0);
        self.touch();
    }

    /// Closes the window.
    pub fn close_window(&mut self) {
        self.window_appear.set_target(0.0);
        self.touch();
    }

    /// Opens the command palette.
    pub fn open_palette(&mut self) {
        self.sue_menu.close();
        if self.palette.is_none() {
            self.palette = Some(Palette::open(self.config.spring, self.spring_mode()));
            self.touch();
        }
    }

    /// Types into the open palette (scripted runs).
    pub fn palette_query(&mut self, text: &str) {
        if let Some(palette) = self.palette.as_mut() {
            palette.set_query(text);
        }
    }

    /// Closes the palette and the Sue menu.
    pub fn dismiss_overlays(&mut self) {
        if let Some(palette) = self.palette.as_mut() {
            palette.close();
        }
        self.sue_menu.close();
        self.touch();
    }

    // ----- Pages ------------------------------------------------------------

    /// Fills the page with studio or tool `id`, which comes in over the Hub
    /// (or takes the place of the one open).
    pub fn open(&mut self, id: AppletId, applets: &Applets) {
        match applets.manifest(&id).map(|manifest| manifest.role) {
            Some(role) if role.is_page() => {}
            Some(AppletRole::Window) => {
                self.open_window(id);
                return;
            }
            _ => {
                self.notify(tr!("there-no-studio-tool-called", id = id), true);
                return;
            }
        }
        self.dismiss_overlays();
        if self.shown.is_none() {
            self.reveal.snap_to(0.0);
        }
        self.page = Some(id.clone());
        self.shown = Some(id);
        self.reveal.set_target(1.0);
        self.touch();
    }

    /// Goes back to the Hub: the page slides away over it.
    pub fn hub(&mut self) {
        self.dismiss_overlays();
        self.discard_armed = None;
        self.page = None;
        self.reveal.set_target(0.0);
        self.touch();
    }

    /// Lets a page that left for the Hub go once it is out of sight.
    fn advance(&mut self) {
        if self.page.is_none()
            && self.shown.is_some()
            && self.reveal.target() <= 0.0
            && self.reveal.position() <= 0.002
        {
            self.shown = None;
            self.reveal.snap_to(0.0);
        }
    }

    /// Pins `id` (or takes the pin away) as one undoable rice edit, so Sue
    /// opens straight into it at launch. One pin at a time: pinning one
    /// moves the pin from any other.
    pub fn pin(&mut self, id: Option<AppletId>, io: &mut ShellIo<'_>) {
        let value = id
            .as_ref()
            .map_or_else(String::new, |id| id.as_str().to_owned());
        if io.world.rice.get_str(PIN_KEY).map(str::trim) == Some(value.as_str()) {
            return;
        }
        let name = id
            .as_ref()
            .and_then(|id| io.applets.manifest(id))
            .map(|manifest| manifest.title());
        let txn = SetRiceValue::new(PIN_KEY, RiceValue::Text(value));
        io.typing.flush(io.history);
        match io.apply_sue(RiceEdit(txn)) {
            Ok(_) => self.notify(
                name.map_or_else(
                    || tr!("sue-opens-hub-launch"),
                    |name| tr!("sue-opens-in-at-launch", name = name),
                ),
                false,
            ),
            Err(error) => self.notify(tr!("could-not-pin-it", error = error), true),
        }
    }

    /// Makes sure the fonts draw the languages in use (Sue's and each
    /// studio's and tool's) and every language's own name.
    fn cover_scripts(&mut self, ctx: &egui::Context, io: &ShellIo<'_>) {
        let mut tags = vec![crate::sue_language(&io.world.rice)];
        for manifest in io.applets.manifests() {
            let tag = crate::applet_language(&io.world.rice, &manifest.id);
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        if tags != self.scripts.0 {
            let mut text: String = sue_lang::languages()
                .into_iter()
                .map(|language| language.name)
                .collect();
            for tag in &tags {
                text.push_str(&sue_lang::characters(tag));
            }
            self.scripts = (tags, text);
        }
        crate::fonts::cover(ctx, &self.scripts.1);
    }

    // ----- Requests ---------------------------------------------------------

    /// Handles a request meant for the shell; returns the ones for the
    /// engine (messages, files, commands, file dialogs).
    pub fn handle(
        &mut self,
        request: ShellRequest,
        io: &mut ShellIo<'_>,
        now: Instant,
    ) -> Option<ShellRequest> {
        let _language = sue_lang::scope(&crate::sue_language(&io.world.rice));
        match request {
            ShellRequest::OpenSettings(page) => {
                let id = io
                    .applets
                    .with_role(AppletRole::Window)
                    .next()
                    .map(|manifest| manifest.id.clone());
                if let Some(id) = id {
                    if let (Some(page), Some(applet)) = (page, io.applets.get_mut(&id)) {
                        applet.navigate(&page);
                    }
                    self.open_window(id);
                }
            }
            ShellRequest::Create { applet, key } => {
                let view = self.view(now, 0.0);
                let theme = self.theme.clone();
                let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &applet));
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(target) = applets.get_mut(&applet) {
                    target.create(&key, &mut cx);
                }
            }
            ShellRequest::Palette => self.open_palette(),
            ShellRequest::Open(id) => self.open(id, io.applets),
            ShellRequest::Hub => {
                self.hub_place = None;
                self.hub();
            }
            ShellRequest::HubPlace(place) => {
                self.hub_place = Some(place);
                self.hub();
            }
            ShellRequest::Pin(id) => self.pin(id, io),
            ShellRequest::Language(setting) => self.set_language(&setting, io),
            ShellRequest::Interface(setting) => self.set_interface(&setting, io),
            ShellRequest::PhoneFrame(on) => self.set_phone_frame(on, io),
            ShellRequest::SueMenu => self.open_sue_menu(io),
            ShellRequest::OpenIn { entity, applet } => {
                if io.world.arena.get_by_id(entity).is_err() {
                    self.notify(tr!("that-is-gone"), true);
                    return None;
                }
                if !io
                    .applets
                    .manifest(&applet)
                    .is_some_and(|manifest| manifest.role.is_page())
                {
                    self.notify(tr!("there-no-studio-tool-called", id = applet), true);
                    return None;
                }
                {
                    let view = self.view(now, 0.0);
                    let theme = self.theme.clone();
                    let _language =
                        sue_lang::scope(&crate::applet_language(&io.world.rice, &applet));
                    let (applets, mut cx) = split(io, &theme, view);
                    if let Some(target) = applets.get_mut(&applet) {
                        target.take_entity("open", entity, &mut cx);
                    }
                }
                self.open(applet, io.applets);
            }
            ShellRequest::SendEntity {
                to,
                purpose,
                entity,
            } => {
                let view = self.view(now, 0.0);
                let theme = self.theme.clone();
                let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &to));
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(applet) = applets.get_mut(&to) {
                    applet.take_entity(&purpose, entity, &mut cx);
                }
            }
            ShellRequest::Saved { path, note } => {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                io.transfers
                    .done(crate::TransferKind::Export, &name, Some(path.clone()));
                self.notify_saved(&path, &note);
            }
            ShellRequest::CloseWindow => self.close_window(),
            ShellRequest::OpenWindow(id) => self.open_window(id),
            ShellRequest::AppletCommand { applet, command } => {
                let view = self.view(now, 0.0);
                let theme = self.theme.clone();
                let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &applet));
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(target) = applets.get_mut(&applet)
                    && !target.command(&command, &mut cx)
                {
                    cx.fail(tr!("is-not-available", command = command));
                }
            }
            ShellRequest::SendFile { to, purpose, path } => {
                let pick = FilePick::open(Some(to.clone()), &purpose, "");
                let view = self.view(now, 0.0);
                let theme = self.theme.clone();
                let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &to));
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(applet) = applets.get_mut(&to) {
                    applet.picked(&pick, &path, &mut cx);
                }
            }
            ShellRequest::Picked { pick, path } => {
                let Some(requester) = pick.requester.clone() else {
                    return Some(ShellRequest::Picked { pick, path });
                };
                let view = self.view(now, 0.0);
                let theme = self.theme.clone();
                let _language =
                    sue_lang::scope(&crate::applet_language(&io.world.rice, &requester));
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(applet) = applets.get_mut(&requester) {
                    applet.picked(&pick, &path, &mut cx);
                }
            }
            other => return Some(other),
        }
        self.touch();
        None
    }

    /// Boots: opens the pinned studio or tool, already in place, or stays
    /// on the Hub.
    pub fn boot(&mut self, io: &mut ShellIo<'_>) {
        if self.booted {
            return;
        }
        self.booted = true;
        if self.page.is_none()
            && let Some(id) = pinned(&io.world.rice, io.applets)
        {
            self.open(id, io.applets);
            self.reveal.snap_to(1.0);
        }
    }

    fn undo(&mut self, io: &mut ShellIo<'_>, redo: bool) {
        io.typing.flush(io.history);
        let result = if redo {
            io.history.redo(io.world)
        } else {
            io.history.undo(io.world)
        };
        match result {
            Ok(true) => {}
            Ok(false) => self.notify(
                if redo {
                    tr!("nothing-to-redo")
                } else {
                    tr!("nothing-to-undo")
                },
                false,
            ),
            Err(error) => self.notify(tr!("undo-failed", error = error), true),
        }
        self.touch();
    }

    // ----- The frame --------------------------------------------------------

    /// Builds one frame into the root `ui`.
    pub fn ui(&mut self, root: &mut Ui, io: &mut ShellIo<'_>, now: Instant) {
        let _language = sue_lang::scope(&crate::sue_language(&io.world.rice));
        let ctx = root.ctx().clone();
        crate::widgets::set_touch(&ctx, self.form_factor() != FormFactor::Desktop);
        crate::widgets::set_own_frames(&ctx, self.own_frame);
        // The window's title: `Sue`, or what the open studio has open.
        let title = self
            .page
            .as_ref()
            .and_then(|page| io.applets.get(page))
            .and_then(|applet| applet.window_title(&io.world.arena))
            .unwrap_or_else(|| "Sue".to_owned());
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        crate::cover::begin_frame(&ctx);
        self.cover_scripts(&ctx, io);
        // Real wall-clock time, clamped; after a sleep, one nominal frame.
        let nominal = 1.0 / self.scheduler.target_hz().max(1) as f32;
        let raw_dt = self
            .last_frame
            .map_or(nominal, |last| now.duration_since(last).as_secs_f32());
        let dt = if raw_dt > 0.1 {
            nominal
        } else {
            raw_dt.clamp(0.0001, 0.05)
        };
        self.last_frame = Some(now);
        let motion_dt = dt * self.config.animation_speed.max(0.01);

        if self.applied_theme.as_ref() != Some(&self.theme) {
            self.theme
                .apply(&ctx, self.config.caret_blink, self.applied_theme.as_ref());
            ctx.set_zoom_factor(self.config.ui_scale.clamp(0.5, 3.0));
            ctx.options_mut(|options| {
                options.input_options.line_scroll_speed = self.config.wheel_line_px;
            });
            self.applied_theme = Some(self.theme.clone());
        }
        let theme = self.theme.clone();

        // Files dropped anywhere open in the studio or tool that takes them.
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .filter(|path| !path.as_os_str().is_empty())
                .collect()
        });
        for path in dropped {
            io.requests.push(ShellRequest::OpenFile(path));
        }
        self.global_keys(&ctx, io, now);

        // Springs.
        let mut moving = false;
        moving |= self.reveal.step(motion_dt);
        moving |= self.window_appear.step(motion_dt);
        if let Some(palette) = self.palette.as_mut() {
            moving |= palette.step(motion_dt);
        }
        if self.window_appear.target() == 0.0 && self.window_appear.is_settled() {
            self.window = None;
        }
        if self.palette.as_ref().is_some_and(Palette::is_gone) {
            self.palette = None;
        }
        self.advance();

        let mut window = root.max_rect();
        if self.own_frame {
            // Sue's own title bar, as Clip Studio Paint's: the title in the
            // middle, the window's buttons at the right.
            let bar =
                Rect::from_min_size(window.min, vec2(window.width(), crate::widgets::TITLE_BAR));
            let title = self.window_title.clone();
            if crate::widgets::title_bar(root, bar, &title, true, &theme) {
                io.requests
                    .push(ShellRequest::Command("app.quit".to_owned()));
            }
            window.min.y = bar.bottom();
        }
        let frame = self.frame(window);
        if frame != window {
            // Phone shaped: the window around the column stays empty.
            let painter = root.painter();
            painter.rect_filled(window, 0.0, theme.void);
            painter.rect_filled(window, 0.0, theme.shadow.gamma_multiply(0.5));
            painter.rect_stroke(
                frame.expand(0.5),
                0.0,
                theme.hairline(),
                egui::StrokeKind::Outside,
            );
        }
        let layout = solve_layout(&LayoutRequest {
            viewport: frame,
            safe_area: self.safe_area,
            form_factor: self.form_factor(),
            metrics: self.config.metrics,
        });
        self.page_rect = layout.page;
        self.sue_button = layout.sue_button;
        self.hub_boxes.clear();
        self.hub_rows.clear();
        self.hub.buttons.clear();
        let settled = self.shown.is_some() && self.reveal.position() >= 0.999;
        // The Hub, unless a page covers it whole.
        if !settled {
            self.hub_ui(root, layout.page, io, &theme, now, dt);
        }
        // The page, wearing its own theme if it has one, with its menus in
        // the bar after the Sue button.
        let page = self
            .shown
            .clone()
            .map(|id| (id.clone(), self.page_theme(&id, &io.world.rice, &theme)));
        if let Some((id, page_theme)) = &page {
            self.with_theme(&ctx, page_theme, |shell| {
                shell.page_layer(root, id, layout.page, !settled, io, page_theme, now, dt);
            });
        }
        let bar_theme = page.as_ref().map_or(&theme, |(_, theme)| theme).clone();
        self.with_theme(&ctx, &bar_theme, |shell| {
            shell.bar_ui(
                root,
                &layout,
                page.as_ref().map(|(id, _)| id),
                !settled,
                io,
                &bar_theme,
                now,
                dt,
            );
        });
        if layout.status.height() >= 1.0 {
            self.status_ui(root, layout.status, io, &theme);
        }
        self.notice_ui(&ctx, layout.page, io, &theme);
        self.palette_ui(&ctx, io, &theme, now);
        self.window_ui(&ctx, frame, io, &theme, now, dt);
        self.quit_ui(&ctx, io, &theme, now);
        if self.own_frame {
            crate::widgets::resize_edges(root, root.max_rect());
        }

        self.animating = moving;
        self.scheduler.set_animating(self.animating);
        if self.animating {
            ctx.request_repaint();
        }
        if io.typing.flush_if_paused(io.history, now) {
            self.touch();
        }
        crate::theme::reshape_handles(&ctx);
    }

    /// The theme a page wears: its own (its rice section's `theme`), or
    /// Sue's.
    fn page_theme(&self, id: &AppletId, rice: &RiceSchema, theme: &Theme) -> Theme {
        let section = id.as_str().replace('-', "_");
        theme.for_studio(rice, &section, &self.presets)
    }

    /// Runs `draw` with `theme` as egui's style, so menus and popups wear it
    /// too, and Sue's again afterwards.
    fn with_theme(&mut self, ctx: &egui::Context, theme: &Theme, draw: impl FnOnce(&mut Self)) {
        let own = *theme != self.theme;
        if own {
            let visuals = theme.visuals(self.config.caret_blink);
            ctx.all_styles_mut(|style| style.visuals = visuals.clone());
        }
        draw(self);
        if own {
            let visuals = self.theme.visuals(self.config.caret_blink);
            ctx.all_styles_mut(|style| style.visuals = visuals.clone());
        }
    }

    /// The studio or tool itself. While it comes in or goes out (`lifted`)
    /// it paints on a layer above the Hub and a shield takes the clicks;
    /// its widgets keep the same ids either way.
    #[allow(clippy::too_many_arguments, reason = "one layer of a page")]
    fn page_layer(
        &mut self,
        root: &mut Ui,
        id: &AppletId,
        area: Rect,
        lifted: bool,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        let view = self.view(now, dt);
        // A child takes its parent's style, not the context's: the page
        // wears its own theme only when handed the style `with_theme` set.
        let mut builder = UiBuilder::new()
            .max_rect(area)
            .id_salt(("sue-page", id.as_str()))
            .style(root.ctx().global_style())
            .layout(Layout::top_down(Align::Min));
        if lifted {
            builder = builder.layer_id(LayerId::new(Order::Middle, Id::unique("sue-page-lifted")));
        }
        let mut page_ui = root.new_child(builder);
        page_ui.set_clip_rect(area);
        if lifted {
            // A gentle fade over the Hub; studios slide their palettes too.
            let t = self.reveal();
            page_ui.set_opacity(t * t * (3.0 - 2.0 * t));
        }
        page_ui.painter().rect_filled(area, 0.0, theme.void);
        let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, id));
        let (applets, mut cx) = split(io, theme, view);
        match applets.get_mut(id) {
            Some(applet) => applet.page_ui(&mut page_ui, &mut cx),
            None => {
                self.page = None;
                self.shown = None;
            }
        }
        if lifted {
            egui::Area::new(Id::unique("sue-page-shield"))
                .order(Order::Middle)
                .fixed_pos(area.min)
                .show(root.ctx(), |ui| {
                    ui.allocate_rect(area, Sense::click_and_drag());
                });
        }
    }

    /// Runs every applet's tick. Returns what they asked for.
    pub fn tick(&mut self, io: &mut ShellIo<'_>, now: Instant) -> crate::Tick {
        let view = self.view(now, 0.0);
        let theme = self.theme.clone();
        let languages: Vec<String> = io
            .applets
            .manifests()
            .map(|manifest| crate::applet_language(&io.world.rice, &manifest.id))
            .collect();
        let (applets, mut cx) = split(io, &theme, view);
        let mut tick = crate::Tick::default();
        for ((_, applet), language) in applets.iter_mut().zip(&languages) {
            let _language = sue_lang::scope(language);
            tick = tick.merge(applet.tick(&mut cx));
        }
        tick
    }

    /// Runs `work` with an applet context over `io`, as an applet would see
    /// it this frame (scripted runs, snapshots and tests).
    pub fn with_context<R>(
        &self,
        io: &mut ShellIo<'_>,
        now: Instant,
        work: impl FnOnce(&mut AppletContext<'_>) -> R,
    ) -> R {
        let view = self.view(now, 0.0);
        let (_, mut cx) = split(io, &self.theme, view);
        work(&mut cx)
    }

    /// Steps the shell's springs without drawing (headless scripted runs).
    pub fn settle(&mut self, dt: f32) -> bool {
        let mut moving = false;
        for spring in [&mut self.reveal, &mut self.window_appear] {
            moving |= spring.step(dt);
        }
        self.advance();
        moving
    }

    /// Duration of the grace period.
    #[must_use]
    pub fn settle_grace(&self) -> Duration {
        self.config.settle_grace
    }
}

#[cfg(test)]
mod tests;
