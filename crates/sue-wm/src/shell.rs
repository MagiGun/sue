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
use std::time::{Duration, Instant};

use egui::{
    Align, Align2, Id, LayerId, Layout, Order, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use sue_core::UndoStack;
use sue_platform::{AudioSink, FormFactor, FramePacing, SafeAreaInsets, TextureUpload};
use sue_rice::{HardwareProfile, RiceSchema, RiceValue, SetRiceValue};

use crate::palette::{Palette, PaletteOutcome};
use crate::{
    AppletContext, AppletId, AppletRole, Applets, Catalog, FilePick, FrameScheduler, ItemState,
    LayoutRequest, LayoutSolution, MenuBar, MenuItem, PaletteEntry, RiceEdit, ShellRequest,
    ShellView, SpringDamper, SpringMode, Theme, Transport, Typing, WmConfig, WorkList, World, keys,
    solve_layout, sue_menu, widgets,
};

/// The rice key of the Hub's pin: the studio or tool Sue opens into at
/// launch, empty for the Hub.
pub const PIN_KEY: &str = "launchpad.pinned";

/// A hardware profile as people read it, e.g. `Legacy Low Spec`.
#[must_use]
pub fn profile_label(profile: HardwareProfile) -> String {
    profile
        .rice_name()
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    /// The global undo tree.
    pub history: &'a mut UndoStack<World>,
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
    /// Frames per second while animating; `None` when idle.
    pub fps: Option<u32>,
}

fn split<'b>(
    io: &'b mut ShellIo<'_>,
    theme: &'b Theme,
    view: ShellView,
) -> (&'b mut Applets, AppletContext<'b>) {
    let cx = AppletContext {
        world: &mut *io.world,
        history: &mut *io.history,
        typing: &mut *io.typing,
        requests: &mut *io.requests,
        catalog: io.catalog,
        theme,
        textures: &mut *io.textures,
        audio: io.audio.as_deref_mut(),
        transport: &mut *io.transport,
        work: io.work,
        view,
    };
    (&mut *io.applets, cx)
}

/// What a click on a box of the Hub asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HubClick {
    /// Open the studio or tool.
    Open,
    /// Pin it, or take its pin away.
    Pin,
}

/// What a row of the Hub's lists does when clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HubRow {
    /// Open a recent file or a saved project in its studio or tool.
    Open(PathBuf),
    /// Bring back work Sue kept and open it in its studio.
    Recover(String),
    /// Throw kept work away (a first click asks, a second one does it).
    Discard(String),
}

/// A row of one of the Hub's lists, ready to draw.
struct ListRow {
    title: String,
    /// What it is, at the right of the title: `Project`, `PNG`.
    kind: String,
    detail: String,
    /// The full path, on hover.
    hover: String,
    open: HubRow,
    /// Kept work can be recovered or discarded with its own buttons.
    kept: Option<String>,
}

/// `lead` and the folder `path` is in, by its name: for a file a phone's
/// picker chose, the folder people chose it in, where the system says it.
fn in_folder(lead: String, path: &std::path::Path) -> String {
    let folder = match sue_platform::shown_place(path) {
        Some(shown) => shown.folder,
        None => path.parent().map(|parent| {
            parent.file_name().map_or_else(
                || parent.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
        }),
    };
    match folder {
        Some(folder) => format!("{lead} in {folder}"),
        None => lead,
    }
}

/// What a palette entry does.
#[derive(Clone, Debug, PartialEq)]
enum PaletteAction {
    Request(ShellRequest),
    Undo(bool),
    Applet(AppletId, String),
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
    /// Where the last frame drew the Hub's boxes and their pins.
    hub_boxes: Vec<(AppletId, Rect, Rect)>,
    /// Where the last frame drew the rows (and buttons) of the Hub's lists.
    hub_rows: Vec<(HubRow, Rect)>,
    /// The kept work whose Discard was clicked once and asks again.
    discard_armed: Option<String>,
    booted: bool,
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

/// An item of the Sue menu that runs `request`.
fn sue_item(
    runs: &mut Vec<(ShellRequest, bool)>,
    label: impl Into<String>,
    request: ShellRequest,
    checked: bool,
) -> MenuItem {
    runs.push((request, checked));
    MenuItem::action(label, format!("sue:{}", runs.len() - 1))
}

/// Every applet's file templates, in applet order.
fn templates(applets: &Applets) -> Vec<(AppletId, crate::FileTemplate)> {
    applets
        .iter()
        .flat_map(|(manifest, applet)| {
            applet
                .templates()
                .into_iter()
                .map(|template| (manifest.id.clone(), template))
        })
        .collect()
}

/// A pushpin in `rect`: filled when `pinned`.
fn pin_glyph(painter: &egui::Painter, rect: Rect, pinned: bool, color: egui::Color32) {
    let size = rect.width().min(rect.height()) * 0.3;
    let center = rect.center() - vec2(0.0, size * 0.25);
    let head = center - vec2(0.0, size * 0.55);
    let stroke = Stroke::new(1.5, color);
    if pinned {
        painter.circle_filled(head, size * 0.55, color);
    } else {
        painter.circle_stroke(head, size * 0.55, stroke);
    }
    painter.line_segment(
        [
            center + vec2(-size * 0.85, size * 0.1),
            center + vec2(size * 0.85, size * 0.1),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + vec2(0.0, size * 0.1),
            center + vec2(0.0, size * 1.45),
        ],
        stroke,
    );
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
            hub_boxes: Vec::new(),
            hub_rows: Vec::new(),
            discard_armed: None,
            booted: false,
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
        self.message = Some((
            text.into(),
            error,
            Instant::now() + self.config.message_duration,
        ));
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
        let folder = folder.map_or_else(String::new, |folder| format!(" in {folder}"));
        let reveal = sue_platform::file_manager().is_some();
        let click = if reveal { ". Click to show it" } else { "" };
        self.message = Some((
            format!("Saved {name}{note}{folder}{click}"),
            false,
            Instant::now() + self.config.message_duration.max(Duration::from_secs(10)),
        ));
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
            form_factor: self.form_factor.chosen(&self.config.interface),
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
                self.notify(format!("There is no studio or tool called {id}"), true);
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
            .map(|manifest| manifest.name.clone());
        let txn = SetRiceValue::new(PIN_KEY, RiceValue::Text(value));
        io.typing.flush(io.history);
        match io.history.apply(io.world, RiceEdit(txn)) {
            Ok(_) => self.notify(
                name.map_or_else(
                    || "Sue opens on the Hub at launch".to_owned(),
                    |name| format!("Sue opens in {name} at launch"),
                ),
                false,
            ),
            Err(error) => self.notify(format!("Could not pin it: {error}"), true),
        }
    }

    // ----- The Sue menu -----------------------------------------------------

    /// Builds the Sue menu for what is open: the Hub, the studios and
    /// tools, new files, files, projects, Send To for what the page works
    /// on, every submenu applets add to, the pin, the palette, Settings.
    fn build_sue_menu(&mut self, io: &ShellIo<'_>) {
        let mut runs = Vec::new();
        let run = &mut runs;
        let keymap = &self.config.keymap;
        let mut items = vec![sue_item(run, "Hub", ShellRequest::Hub, self.page.is_none())];
        for (title, role) in [("Studios", AppletRole::Studio), ("Tools", AppletRole::Tool)] {
            let pages: Vec<MenuItem> = io
                .applets
                .with_role(role)
                .map(|manifest| {
                    sue_item(
                        run,
                        manifest.name.clone(),
                        ShellRequest::Open(manifest.id.clone()),
                        self.page.as_ref() == Some(&manifest.id),
                    )
                })
                .collect();
            if !pages.is_empty() {
                items.push(MenuItem::sub(title, pages));
            }
        }
        items.push(MenuItem::Separator);
        // New: a submenu per kind of file, each with its exact kinds.
        let mut kinds: Vec<(String, Vec<MenuItem>)> = Vec::new();
        for (applet, template) in templates(io.applets) {
            let item = sue_item(
                run,
                template.label,
                ShellRequest::Create {
                    applet,
                    key: template.key,
                },
                false,
            );
            match kinds
                .iter_mut()
                .find(|(kind, _)| *kind == template.category)
            {
                Some((_, list)) => list.push(item),
                None => kinds.push((template.category, vec![item])),
            }
        }
        if !kinds.is_empty() {
            items.push(MenuItem::sub(
                "New",
                kinds
                    .into_iter()
                    .map(|(kind, list)| MenuItem::sub(kind, list))
                    .collect(),
            ));
        }
        // The named submenus, Sue's own first.
        let mut named: Vec<(String, Vec<MenuItem>)> = vec![
            (sue_menu::FILE.to_owned(), {
                let mut file = vec![
                    sue_item(
                        run,
                        "Open File...",
                        ShellRequest::Command("file.open".to_owned()),
                        false,
                    )
                    .key(keymap.open_file.to_string()),
                ];
                let recent: Vec<MenuItem> = io
                    .work
                    .recent()
                    .take(10)
                    .map(|recent| {
                        let name = recent.path.file_name().map_or_else(
                            || recent.path.display().to_string(),
                            |name| name.to_string_lossy().into_owned(),
                        );
                        sue_item(
                            run,
                            name,
                            ShellRequest::OpenFile(recent.path.clone()),
                            false,
                        )
                    })
                    .collect();
                if !recent.is_empty() {
                    file.push(MenuItem::sub("Open Recent", recent));
                }
                file
            }),
            (
                sue_menu::PROJECT.to_owned(),
                vec![
                    sue_item(
                        run,
                        "Open Project...",
                        ShellRequest::PickFile(
                            FilePick::open(None, "open", "Open Project").extensions(&["sue"]),
                        ),
                        false,
                    ),
                    sue_item(
                        run,
                        "Save Project",
                        ShellRequest::Command("file.save".to_owned()),
                        false,
                    )
                    .key(keymap.save.to_string()),
                    sue_item(
                        run,
                        "Save Project As...",
                        ShellRequest::Command("file.save_as".to_owned()),
                        false,
                    ),
                ],
            ),
            (sue_menu::SEND_TO.to_owned(), Vec::new()),
        ];
        let add = |named: &mut Vec<(String, Vec<MenuItem>)>, menu: &str, item: MenuItem| match named
            .iter_mut()
            .find(|(name, _)| name == menu)
        {
            Some((_, list)) => list.push(item),
            None => named.push((menu.to_owned(), vec![item])),
        };
        // What the page works on, into every other studio and tool that
        // takes it, then whatever each applet offers.
        let focus_id = self
            .page
            .as_ref()
            .and_then(|page| io.applets.get(page))
            .and_then(|applet| applet.focus());
        let focus = focus_id.and_then(|id| io.world.arena.get_by_id(id).ok());
        if let (Some(id), Some(entity)) = (focus_id, focus) {
            for (manifest, applet) in io.applets.iter() {
                let current = self.page.as_ref() == Some(&manifest.id);
                if manifest.role.is_page() && !current && applet.accepts(entity) {
                    let item = sue_item(
                        run,
                        manifest.name.clone(),
                        ShellRequest::OpenIn {
                            entity: id,
                            applet: manifest.id.clone(),
                        },
                        false,
                    );
                    add(&mut named, sue_menu::SEND_TO, item);
                }
            }
        }
        for (manifest, applet) in io.applets.iter() {
            let current = self.page.as_ref() == Some(&manifest.id);
            for action in applet.sue_actions(focus, current) {
                let item = sue_item(run, action.label, action.request, false);
                add(&mut named, &action.menu, item);
            }
        }
        items.extend(
            named
                .into_iter()
                .filter(|(_, list)| !list.is_empty())
                .map(|(name, list)| MenuItem::sub(name, list)),
        );
        items.push(MenuItem::Separator);
        if let Some(page) = self.page.clone() {
            let pinned = pinned(&io.world.rice, io.applets).as_ref() == Some(&page);
            let name = io
                .applets
                .manifest(&page)
                .map_or_else(String::new, |manifest| manifest.name.clone());
            items.push(sue_item(
                run,
                format!("Open {name} at Launch"),
                ShellRequest::Pin((!pinned).then_some(page)),
                pinned,
            ));
        }
        let palette = format!("Command Palette ({})", keymap.command_palette);
        items.push(sue_item(run, palette, ShellRequest::Palette, false));
        items.push(
            sue_item(run, "Settings", ShellRequest::OpenSettings(None), false)
                .key(keymap.open_settings.to_string()),
        );
        items.push(
            sue_item(
                run,
                "Quit Sue",
                ShellRequest::Command("app.quit".to_owned()),
                false,
            )
            .key(keymap.quit.to_string()),
        );
        self.sue_items = items;
        self.sue_runs = runs;
    }

    /// Opens the Sue menu under the Sue button, built for what is open.
    pub fn open_sue_menu(&mut self, io: &ShellIo<'_>) {
        if let Some(palette) = self.palette.as_mut() {
            palette.close();
        }
        self.build_sue_menu(io);
        self.sue_menu.open_dropdown();
        self.touch();
    }

    /// The items of the Sue menu (with `path` naming the submenus to go
    /// through), as labels (scripted runs).
    #[must_use]
    pub fn sue_menu_labels(&self, path: &[&str]) -> Vec<String> {
        let mut items = &self.sue_items;
        for step in path {
            let found = items.iter().find_map(|item| match item {
                MenuItem::Sub { label, items } if label == step => Some(items),
                _ => None,
            });
            match found {
                Some(next) => items = next,
                None => return Vec::new(),
            }
        }
        items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { label, .. } | MenuItem::Sub { label, .. } => Some(label.clone()),
                MenuItem::Separator => None,
            })
            .collect()
    }

    /// Picks the Sue menu's item at `path` (its submenus, then its label),
    /// as a click would (scripted runs). Returns `false` if there is none.
    pub fn sue_menu_pick(&mut self, path: &[&str], io: &mut ShellIo<'_>, now: Instant) -> bool {
        let Some((last, submenus)) = path.split_last() else {
            return false;
        };
        let mut items = &self.sue_items;
        for step in submenus {
            let found = items.iter().find_map(|item| match item {
                MenuItem::Sub { label, items } if label == step => Some(items),
                _ => None,
            });
            match found {
                Some(next) => items = next,
                None => return false,
            }
        }
        let command = items.iter().find_map(|item| match item {
            MenuItem::Action { label, command, .. } if label == last => Some(command.clone()),
            _ => None,
        });
        match command {
            Some(command) => {
                self.run_sue_item(&command, io, now);
                true
            }
            None => false,
        }
    }

    fn run_sue_item(&mut self, command: &str, io: &mut ShellIo<'_>, now: Instant) {
        self.sue_menu.close();
        let request = command
            .strip_prefix("sue:")
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| self.sue_runs.get(index))
            .map(|(request, _)| request.clone());
        if let Some(request) = request
            && let Some(leftover) = self.handle(request, io, now)
        {
            io.requests.push(leftover);
        }
        self.touch();
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
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(target) = applets.get_mut(&applet) {
                    target.create(&key, &mut cx);
                }
            }
            ShellRequest::Palette => self.open_palette(),
            ShellRequest::Open(id) => self.open(id, io.applets),
            ShellRequest::Hub => self.hub(),
            ShellRequest::Pin(id) => self.pin(id, io),
            ShellRequest::SueMenu => self.open_sue_menu(io),
            ShellRequest::OpenIn { entity, applet } => {
                if io.world.arena.get_by_id(entity).is_err() {
                    self.notify("That is gone", true);
                    return None;
                }
                if !io
                    .applets
                    .manifest(&applet)
                    .is_some_and(|manifest| manifest.role.is_page())
                {
                    self.notify(format!("There is no studio or tool called {applet}"), true);
                    return None;
                }
                {
                    let view = self.view(now, 0.0);
                    let theme = self.theme.clone();
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
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(applet) = applets.get_mut(&to) {
                    applet.take_entity(&purpose, entity, &mut cx);
                }
            }
            ShellRequest::Saved { path, note } => self.notify_saved(&path, &note),
            ShellRequest::CloseWindow => self.close_window(),
            ShellRequest::OpenWindow(id) => self.open_window(id),
            ShellRequest::AppletCommand { applet, command } => {
                let view = self.view(now, 0.0);
                let theme = self.theme.clone();
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(target) = applets.get_mut(&applet)
                    && !target.command(&command, &mut cx)
                {
                    cx.fail(format!("{command} is not available"));
                }
            }
            ShellRequest::SendFile { to, purpose, path } => {
                let pick = FilePick::open(Some(to.clone()), &purpose, "");
                let view = self.view(now, 0.0);
                let theme = self.theme.clone();
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
                    "Nothing to redo"
                } else {
                    "Nothing to undo"
                },
                false,
            ),
            Err(error) => self.notify(format!("Undo failed: {error}"), true),
        }
        self.touch();
    }

    // ----- The frame --------------------------------------------------------

    /// Builds one frame into the root `ui`.
    pub fn ui(&mut self, root: &mut Ui, io: &mut ShellIo<'_>, now: Instant) {
        let ctx = root.ctx().clone();
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

        let layout = solve_layout(&LayoutRequest {
            viewport: root.max_rect(),
            safe_area: self.safe_area,
            form_factor: self.form_factor.chosen(&self.config.interface),
            metrics: self.config.metrics,
        });
        self.page_rect = layout.page;
        self.sue_button = layout.sue_button;
        self.hub_boxes.clear();
        self.hub_rows.clear();
        let settled = self.shown.is_some() && self.reveal.position() >= 0.999;
        // The Hub, unless a page covers it whole.
        if !settled {
            self.hub_ui(root, layout.page, io, &theme);
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
        self.status_ui(root, layout.status, io, &theme);
        self.palette_ui(&ctx, io, &theme, now);
        self.window_ui(&ctx, root.max_rect(), io, &theme, now, dt);

        self.animating = moving;
        self.scheduler.set_animating(self.animating);
        if self.animating {
            ctx.request_repaint();
        }
        if io.typing.flush_if_paused(io.history, now) {
            self.touch();
        }
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

    /// The top bar: the Sue button and its menu, then the page's menus (or
    /// the word Hub).
    #[allow(clippy::too_many_arguments, reason = "the bar and what is in it")]
    fn bar_ui(
        &mut self,
        root: &mut Ui,
        layout: &LayoutSolution,
        page: Option<&AppletId>,
        moving: bool,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        let painter = root.painter().clone();
        painter.rect_filled(layout.bar, 0.0, theme.panel);
        painter.hline(layout.bar.x_range(), layout.bar.bottom(), theme.hairline());

        // The Sue button: Sue's silhouette ("Sue" without a GPU).
        let button = layout.sue_button;
        let response = root
            .interact(button, Id::unique("sue-button"), Sense::click())
            .on_hover_text("Sue: the Hub, every studio and tool, files, projects and Settings");
        let open = self.sue_menu.is_open();
        let face = button.shrink2(vec2(4.0, 3.0));
        if open || response.hovered() {
            painter.rect_filled(
                face,
                theme.control_radius,
                if open { theme.active } else { theme.hover },
            );
        }
        let mark = Rect::from_center_size(face.center(), Vec2::splat(face.height() - 4.0));
        if !crate::brand::mark(&painter, &mut *io.textures, mark, theme.text) {
            painter.text(
                face.center(),
                Align2::CENTER_CENTER,
                "Sue",
                theme.font(theme.small_size + 0.5),
                theme.text,
            );
        }
        if response.clicked() && !open {
            self.build_sue_menu(io);
            if let Some(palette) = self.palette.as_mut() {
                palette.close();
            }
        }
        let picked = {
            let runs = &self.sue_runs;
            let state = |command: &str| {
                command
                    .strip_prefix("sue:")
                    .and_then(|index| index.parse::<usize>().ok())
                    .and_then(|index| runs.get(index))
                    .map_or(ItemState::off(), |(_, checked)| {
                        if *checked {
                            ItemState::check(true)
                        } else {
                            ItemState::on()
                        }
                    })
            };
            self.sue_menu.dropdown(
                root,
                button,
                response.clicked(),
                &self.sue_items,
                &state,
                theme,
            )
        };
        if let Some(command) = picked {
            self.run_sue_item(&command, io, now);
        }
        painter.vline(
            button.right(),
            layout.bar.shrink2(vec2(0.0, 6.0)).y_range(),
            theme.hairline(),
        );

        // The page's own menus, or where people are.
        match page {
            Some(id) => {
                let view = self.view(now, dt);
                let mut bar = root.new_child(
                    UiBuilder::new()
                        .max_rect(layout.menus)
                        .id_salt(("sue-bar", id.as_str()))
                        .style(root.ctx().global_style())
                        .layout(Layout::left_to_right(Align::Center)),
                );
                bar.set_clip_rect(layout.menus);
                if moving {
                    bar.set_opacity(self.reveal());
                    bar.disable();
                }
                let (applets, mut cx) = split(io, theme, view);
                if let Some(applet) = applets.get_mut(id) {
                    applet.bar_ui(&mut bar, &mut cx);
                }
            }
            None => {
                painter.text(
                    layout.menus.left_center() + vec2(theme.padding * 1.5, 0.0),
                    Align2::LEFT_CENTER,
                    "Hub",
                    theme.font(theme.ui_size),
                    theme.muted,
                );
            }
        }
    }

    /// The Hub: a box for every studio, then for every tool, each with a
    /// pin at its bottom right.
    fn hub_ui(&mut self, root: &mut Ui, area: Rect, io: &mut ShellIo<'_>, theme: &Theme) {
        root.painter().rect_filled(area, 0.0, theme.void);
        // A page on its way in takes no clicks on the Hub under it.
        let live = self.page.is_none();
        let touch = self.form_factor.chosen(&self.config.interface) != FormFactor::Desktop;
        let narrow = area.width() < 560.0;
        let margin = if narrow { 16.0 } else { 40.0 };
        let gap = if narrow { 12.0 } else { 16.0 };
        let size = if narrow {
            vec2(
                ((area.width() - margin * 2.0 - gap) / 2.0).max(132.0),
                184.0,
            )
        } else {
            vec2(256.0, 176.0)
        };
        let columns = ((area.width() - margin * 2.0 + gap) / (size.x + gap))
            .floor()
            .max(1.0) as usize;
        let pinned = pinned(&io.world.rice, io.applets);
        let mut clicked = None;
        let mut row_click = None;
        let mut ui = root.new_child(
            UiBuilder::new()
                .max_rect(area)
                .id_salt("sue-hub")
                .layout(Layout::top_down(Align::Min)),
        );
        ui.set_clip_rect(area);
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(&mut ui, |ui| {
                ui.add_space(margin * 0.75);
                for (title, role) in [("Studios", AppletRole::Studio), ("Tools", AppletRole::Tool)]
                {
                    let boxes: Vec<(AppletId, String, String, String)> = io
                        .applets
                        .with_role(role)
                        .map(|manifest| {
                            (
                                manifest.id.clone(),
                                manifest.name.clone(),
                                manifest.icon.clone(),
                                manifest.summary.clone(),
                            )
                        })
                        .collect();
                    if boxes.is_empty() {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(margin);
                        ui.label(
                            RichText::new(title)
                                .size(theme.heading_size)
                                .color(theme.text),
                        );
                    });
                    ui.add_space(gap * 0.75);
                    for row in boxes.chunks(columns) {
                        ui.horizontal(|ui| {
                            ui.add_space(margin);
                            ui.spacing_mut().item_spacing.x = gap;
                            for (id, name, icon, summary) in row {
                                let on = pinned.as_ref() == Some(id);
                                let what = (name.as_str(), icon.as_str(), summary.as_str());
                                let (click, rect, pin) =
                                    Self::hub_box(ui, size, what, on, live, touch, theme);
                                self.hub_boxes.push((id.clone(), rect, pin));
                                if let Some(click) = click {
                                    clicked = Some((id.clone(), click));
                                }
                            }
                        });
                        ui.add_space(gap);
                    }
                    ui.add_space(margin * 0.5);
                }
                row_click = self.hub_lists(ui, io, theme, (margin, gap), live, touch);
                ui.add_space(margin);
            });
        match row_click {
            Some(HubRow::Open(path)) => {
                self.discard_armed = None;
                io.requests.push(ShellRequest::OpenFile(path));
            }
            Some(HubRow::Recover(id)) => {
                self.discard_armed = None;
                io.requests.push(ShellRequest::Recover(id));
            }
            Some(HubRow::Discard(id)) => {
                if self.discard_armed.as_ref() == Some(&id) {
                    self.discard_armed = None;
                    io.requests.push(ShellRequest::Discard(id));
                } else {
                    self.discard_armed = Some(id);
                }
                self.touch();
            }
            None => {}
        }
        match clicked {
            Some((id, HubClick::Open)) => self.open(id, io.applets),
            Some((id, HubClick::Pin)) => {
                let again = pinned.as_ref() == Some(&id);
                self.pin((!again).then_some(id), io);
            }
            None => {}
        }
    }

    /// The Hub's lists under the boxes: the files and projects of late,
    /// the saved projects, and the work Sue kept that nobody saved. Three
    /// columns side by side when there is room, one under the other when
    /// not. Returns the row or button clicked.
    fn hub_lists(
        &mut self,
        ui: &mut Ui,
        io: &ShellIo<'_>,
        theme: &Theme,
        (margin, gap): (f32, f32),
        live: bool,
        touch: bool,
    ) -> Option<HubRow> {
        let now = crate::unix_now();
        let mut lists = Vec::new();
        if self.config.recent_count > 0 {
            lists.push((
                "Recent",
                "Files and projects you open or save show up here.",
                Self::recent_rows(io, now, self.config.recent_count),
            ));
        }
        lists.push((
            "Saved Projects",
            "Projects you save (Sue menu, Project, Save Project) show up here.",
            Self::project_rows(io, now),
        ));
        lists.push((
            "Unsaved Work",
            "Work Sue keeps for you before it is saved shows up here, to recover or discard.",
            Self::unsaved_rows(io, now),
        ));
        let room = (ui.available_width() - margin * 2.0).max(120.0);
        let side_by_side = room >= 3.0 * 300.0 + 2.0 * gap;
        let width = if side_by_side {
            ((room - 2.0 * gap) / 3.0).min(440.0)
        } else {
            room.min(560.0)
        };
        let mut clicked = None;
        let mut draw = |shell: &mut Self, ui: &mut Ui, list: (&str, &str, Vec<ListRow>)| {
            let (title, empty, rows) = list;
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.label(
                    RichText::new(title)
                        .size(theme.heading_size)
                        .color(theme.text),
                );
                ui.add_space(gap * 0.75);
                if rows.is_empty() {
                    ui.label(
                        RichText::new(empty)
                            .size(theme.small_size)
                            .color(theme.muted),
                    );
                }
                ui.spacing_mut().item_spacing.y = 6.0;
                for row in rows {
                    if let Some(click) = shell.hub_list_row(ui, width, &row, live, touch, theme) {
                        clicked = Some(click);
                    }
                }
            });
        };
        if side_by_side {
            ui.horizontal_top(|ui| {
                ui.add_space(margin);
                ui.spacing_mut().item_spacing.x = gap;
                for list in lists {
                    draw(self, ui, list);
                }
            });
        } else {
            for list in lists {
                ui.horizontal_top(|ui| {
                    ui.add_space(margin);
                    draw(self, ui, list);
                });
                ui.add_space(gap * 1.5);
            }
        }
        clicked
    }

    /// The files and projects of late, newest first.
    fn recent_rows(io: &ShellIo<'_>, now: u64, count: usize) -> Vec<ListRow> {
        io.work
            .recent()
            .take(count)
            .map(|file| {
                let verb = if file.saved >= file.opened {
                    "Saved"
                } else {
                    "Opened"
                };
                ListRow {
                    title: file.title(),
                    kind: if file.is_project() {
                        "Project".to_owned()
                    } else {
                        file.path
                            .extension()
                            .map(|extension| extension.to_string_lossy().to_uppercase())
                            .unwrap_or_default()
                    },
                    detail: in_folder(
                        format!("{verb} {}", crate::ago(file.last(), now)),
                        &file.path,
                    ),
                    hover: sue_platform::shown_path(&file.path),
                    open: HubRow::Open(file.path.clone()),
                    kept: None,
                }
            })
            .collect()
    }

    /// The Sue projects saved or opened, by name.
    fn project_rows(io: &ShellIo<'_>, now: u64) -> Vec<ListRow> {
        io.work
            .projects()
            .into_iter()
            .take(50)
            .map(|file| {
                let (verb, when) = if file.saved > 0 {
                    ("Saved", file.saved)
                } else {
                    ("Opened", file.opened)
                };
                ListRow {
                    title: file.title(),
                    kind: Self::studio_name(io, file.page.as_ref()),
                    detail: in_folder(format!("{verb} {}", crate::ago(when, now)), &file.path),
                    hover: sue_platform::shown_path(&file.path),
                    open: HubRow::Open(file.path.clone()),
                    kept: None,
                }
            })
            .collect()
    }

    /// The work Sue kept that nobody saved, newest first.
    fn unsaved_rows(io: &ShellIo<'_>, now: u64) -> Vec<ListRow> {
        io.work
            .unsaved()
            .map(|kept| {
                let detail = match &kept.project {
                    Some(project) => format!(
                        "Kept {}, newer than {}",
                        crate::ago(kept.at, now),
                        project.file_name().map_or_else(
                            || project.display().to_string(),
                            |name| name.to_string_lossy().into_owned(),
                        )
                    ),
                    None => format!("Kept {}, never saved", crate::ago(kept.at, now)),
                };
                ListRow {
                    title: kept.title(),
                    kind: Self::studio_name(io, kept.page.as_ref()),
                    detail,
                    hover: kept
                        .project
                        .as_ref()
                        .map(|project| project.display().to_string())
                        .unwrap_or_default(),
                    open: HubRow::Recover(kept.id.clone()),
                    kept: Some(kept.id.clone()),
                }
            })
            .collect()
    }

    /// The name of the studio or tool `page`, if it is installed.
    fn studio_name(io: &ShellIo<'_>, page: Option<&AppletId>) -> String {
        page.and_then(|page| io.applets.manifest(page))
            .map(|manifest| manifest.name.clone())
            .unwrap_or_default()
    }

    /// One row of a list: a sharp box with the title, what it is and a line
    /// of detail; kept work has Recover and Discard buttons at its right.
    fn hub_list_row(
        &mut self,
        ui: &mut Ui,
        width: f32,
        row: &ListRow,
        live: bool,
        touch: bool,
        theme: &Theme,
    ) -> Option<HubRow> {
        let height = if touch { 56.0 } else { 46.0 };
        let sense = if live { Sense::click() } else { Sense::hover() };
        let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), sense);
        if !row.hover.is_empty() {
            response = response.on_hover_text(&row.hover);
        }
        self.hub_rows.push((row.open.clone(), rect));
        // Kept work's buttons, from the right; checked after the row, so
        // they win its clicks.
        let mut buttons = Vec::new();
        if let Some(id) = &row.kept {
            let armed = self.discard_armed.as_ref() == Some(id);
            let mut right = rect.right() - 6.0;
            let discard = if armed { "Really Discard" } else { "Discard" };
            for (label, action) in [
                (discard, HubRow::Discard(id.clone())),
                ("Recover", HubRow::Recover(id.clone())),
            ] {
                let text = ui.painter().layout_no_wrap(
                    label.to_owned(),
                    theme.font(theme.small_size + 1.0),
                    theme.text,
                );
                let button = Rect::from_min_max(
                    pos2(right - text.size().x - 20.0, rect.top() + 6.0),
                    pos2(right, rect.bottom() - 6.0),
                );
                right = button.left() - 6.0;
                let hit = ui.interact(
                    button,
                    ui.scope_id().with(("sue-hub-kept", label, id.as_str())),
                    sense,
                );
                self.hub_rows.push((action.clone(), button));
                let lit = armed && matches!(action, HubRow::Discard(_));
                buttons.push((button, text, hit, action, lit));
            }
        }
        let over_button = buttons.iter().any(|(_, _, hit, _, _)| hit.hovered());
        let hovered = live && (response.hovered() || over_button);
        let painter = ui.painter();
        painter.rect(
            rect,
            0.0,
            if hovered { theme.hover } else { theme.pill },
            Stroke::new(
                theme.border_width.max(1.0),
                if hovered {
                    theme.border_active
                } else {
                    theme.border
                },
            ),
            StrokeKind::Inside,
        );
        let text_right = buttons
            .iter()
            .map(|(button, ..)| button.left())
            .fold(rect.right(), f32::min)
            - 10.0;
        let clip = Rect::from_min_max(rect.min, pos2(text_right, rect.bottom())).shrink(1.0);
        let painter = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
        let line = (height - theme.ui_size - theme.small_size - 4.0) / 2.0;
        let kind =
            painter.layout_no_wrap(row.kind.clone(), theme.font(theme.small_size), theme.muted);
        let kind_width = kind.size().x;
        painter.galley(
            pos2(text_right - kind_width, rect.top() + line + 1.0),
            kind,
            theme.muted,
        );
        let title =
            painter.layout_no_wrap(row.title.clone(), theme.font(theme.ui_size), theme.text);
        painter
            .with_clip_rect(Rect::from_min_max(
                clip.min,
                pos2(text_right - kind_width - 8.0, clip.bottom()),
            ))
            .galley(
                pos2(rect.left() + 12.0, rect.top() + line),
                title,
                theme.text,
            );
        painter.text(
            pos2(rect.left() + 12.0, rect.top() + line + theme.ui_size + 4.0),
            Align2::LEFT_TOP,
            &row.detail,
            theme.font(theme.small_size),
            theme.muted,
        );
        let mut clicked = (live && response.clicked()).then(|| row.open.clone());
        for (button, text, hit, action, lit) in buttons {
            let hot = live && hit.hovered();
            let painter = ui.painter();
            // A fill of its own, so it shows on a hovered row too.
            let fill = if lit {
                theme.active
            } else if hot {
                theme.hover
            } else {
                theme.pill
            };
            painter.rect_filled(button, 0.0, fill);
            painter.rect_stroke(
                button,
                0.0,
                if lit || hot {
                    theme.hairline_active()
                } else {
                    theme.hairline()
                },
                StrokeKind::Inside,
            );
            painter.galley(button.center() - text.size() / 2.0, text, theme.text);
            if live && hit.clicked() {
                clicked = Some(action);
            }
        }
        clicked
    }

    /// One box of the Hub, with sharp corners and a pin at its bottom
    /// right. Returns what was clicked, the box and the pin.
    fn hub_box(
        ui: &mut Ui,
        size: Vec2,
        (name, icon, summary): (&str, &str, &str),
        pinned: bool,
        live: bool,
        touch: bool,
        theme: &Theme,
    ) -> (Option<HubClick>, Rect, Rect) {
        let sense = if live { Sense::click() } else { Sense::hover() };
        let (rect, response) = ui.allocate_exact_size(size, sense);
        let side = if touch { 44.0 } else { 30.0 };
        let pin = Rect::from_min_size(
            rect.right_bottom() - vec2(side + 8.0, side + 8.0),
            Vec2::splat(side),
        );
        // The pin is checked after the box, so it wins the click over it.
        let pin_response = ui
            .interact(pin, ui.scope_id().with(("sue-hub-pin", name)), sense)
            .on_hover_text(if pinned {
                "Pinned: Sue opens here at launch. Click to take the pin away"
            } else {
                "Pin: Sue opens here at launch"
            });
        let hovered = live && (response.hovered() || pin_response.hovered());
        let painter = ui.painter();
        painter.rect(
            rect,
            0.0,
            if hovered { theme.hover } else { theme.pill },
            Stroke::new(
                theme.border_width.max(1.0),
                if hovered {
                    theme.border_active
                } else {
                    theme.border
                },
            ),
            StrokeKind::Inside,
        );
        painter.text(
            rect.left_top() + vec2(18.0, 16.0),
            Align2::LEFT_TOP,
            icon,
            theme.font(theme.heading_size * 1.8),
            theme.text,
        );
        painter.text(
            rect.left_top() + vec2(18.0, 76.0),
            Align2::LEFT_TOP,
            name,
            theme.font(theme.ui_size * 1.2),
            theme.text,
        );
        let galley = painter.layout(
            summary.to_owned(),
            theme.font(theme.small_size),
            theme.muted,
            rect.width() - 36.0 - side,
        );
        painter.with_clip_rect(rect.shrink(1.0)).galley(
            rect.left_top() + vec2(18.0, 102.0),
            galley,
            theme.muted,
        );
        // The pin: a square button, sharp like its box.
        if pin_response.hovered() && live {
            painter.rect_filled(pin, 0.0, theme.active);
        }
        if pinned {
            painter.rect_stroke(pin, 0.0, theme.hairline_active(), StrokeKind::Inside);
        }
        pin_glyph(
            painter,
            pin,
            pinned,
            if pinned || pin_response.hovered() {
                theme.text
            } else {
                theme.muted
            },
        );
        let click = if !live {
            None
        } else if pin_response.clicked() {
            Some(HubClick::Pin)
        } else if response.clicked() {
            Some(HubClick::Open)
        } else {
            None
        };
        (click, rect, pin)
    }

    fn global_keys(&mut self, ctx: &egui::Context, io: &mut ShellIo<'_>, now: Instant) {
        if io.applets.captures_keys() {
            return;
        }
        let keymap = self.config.keymap.clone();
        let typing = ctx.text_edit_focused();
        // Chords the open studio uses itself stay with it.
        let reserved: Vec<sue_rice::Keybind> = self
            .page
            .as_ref()
            .and_then(|page| io.applets.get(page))
            .map(|applet| applet.reserved_keys())
            .unwrap_or_default();
        let take = |input: &mut egui::InputState, bind: &sue_rice::Keybinds| {
            keys::consume_except(input, bind, &reserved)
        };
        let (undo, redo, settings, open, save, quit) = ctx.input_mut(|input| {
            let redo = take(input, &keymap.redo) || take(input, &keymap.redo_alt);
            (
                take(input, &keymap.undo),
                redo,
                take(input, &keymap.open_settings),
                take(input, &keymap.open_file),
                take(input, &keymap.save),
                take(input, &keymap.quit),
            )
        });
        if undo {
            self.undo(io, false);
        }
        if redo {
            self.undo(io, true);
        }
        if settings {
            self.handle(ShellRequest::OpenSettings(None), io, now);
        }
        if open {
            // The page opens files its own way first.
            let handled = match self.page.clone() {
                Some(page) => {
                    let view = self.view(now, 0.0);
                    let theme = self.theme.clone();
                    let (applets, mut cx) = split(io, &theme, view);
                    applets
                        .get_mut(&page)
                        .is_some_and(|applet| applet.command("file.open", &mut cx))
                }
                None => false,
            };
            if !handled {
                io.requests
                    .push(ShellRequest::Command("file.open".to_owned()));
            }
        }
        if save {
            io.requests
                .push(ShellRequest::Command("file.save".to_owned()));
        }
        if quit {
            io.requests
                .push(ShellRequest::Command("app.quit".to_owned()));
        }
        if typing || self.palette.is_some() {
            return;
        }
        let (palette, escape) = ctx.input_mut(|input| {
            (
                take(input, &keymap.command_palette) || take(input, &keymap.command_palette_alt),
                self.window.is_some()
                    && self.window_appear.target() > 0.0
                    && input.key_pressed(egui::Key::Escape),
            )
        });
        if palette {
            self.open_palette();
        }
        if escape {
            self.close_window();
        }
    }

    fn status_ui(&mut self, root: &mut Ui, rect: Rect, io: &mut ShellIo<'_>, theme: &Theme) {
        let painter = root.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, theme.panel);
        painter.hline(rect.x_range(), rect.top(), theme.hairline());
        let font = theme.font(theme.small_size);
        let place = self
            .page
            .as_ref()
            .and_then(|page| io.applets.manifest(page))
            .map_or_else(|| "Hub".to_owned(), |manifest| manifest.name.clone());
        let placed = painter.text(
            rect.left_center() + vec2(theme.padding, 0.0),
            Align2::LEFT_CENTER,
            place,
            font.clone(),
            theme.muted,
        );
        let items = [
            format!("{} entities", io.world.arena.len()),
            format!("Undo {}", io.history.depth()),
            profile_label(self.profile),
            match io.fps {
                Some(fps) if self.animating => format!("{fps} FPS"),
                _ => "Idle".to_owned(),
            },
        ];
        let mut x = rect.right() - theme.padding;
        // From the right; on a narrow screen the ones that would run into
        // the page's name are left out.
        for (index, item) in items.iter().enumerate().rev() {
            let width = painter
                .layout_no_wrap(item.clone(), font.clone(), theme.muted)
                .size()
                .x;
            if x - width < placed.right() + theme.padding * 2.0 {
                break;
            }
            let shown = painter.text(
                pos2(x, rect.center().y),
                Align2::RIGHT_CENTER,
                item,
                font.clone(),
                theme.muted,
            );
            x = shown.left() - theme.padding;
            if index > 0 {
                painter.vline(x, rect.shrink2(vec2(0.0, 5.0)).y_range(), theme.hairline());
                x -= theme.padding;
            }
        }
        if let Some((text, error)) = self.message() {
            let text = if error {
                format!("! {text}")
            } else {
                text.to_owned()
            };
            let at = pos2(placed.right() + theme.padding * 3.0, rect.center().y);
            let galley = painter.layout_no_wrap(text, font.clone(), theme.muted);
            // The message keeps to the room the counters left.
            let painter = painter.with_clip_rect(Rect::from_min_max(
                pos2(at.x, rect.top()),
                pos2(x.max(at.x), rect.bottom()),
            ));
            let area =
                Rect::from_min_size(pos2(at.x, rect.top()), vec2(galley.size().x, rect.height()));
            let mut color = if error { theme.text } else { theme.muted };
            if let Some(path) = self.message_path.clone() {
                // A saved file's message shows the file when clicked.
                let response = root
                    .interact(area, Id::unique("sue-status-message"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(path.display().to_string());
                if response.hovered() {
                    color = theme.text;
                    painter.hline(
                        area.x_range(),
                        rect.center().y + galley.size().y / 2.0,
                        Stroke::new(1.0, theme.muted),
                    );
                }
                if response.clicked() {
                    io.requests.push(ShellRequest::Command(format!(
                        "reveal-file:{}",
                        path.display()
                    )));
                }
            }
            painter.galley(
                pos2(at.x, rect.center().y - galley.size().y / 2.0),
                galley,
                color,
            );
        }
    }

    // ----- Palette and window -------------------------------------------------

    /// Every palette entry: the Hub, every studio and tool, the pin, Sue's
    /// own actions, new files and the applets' own commands.
    fn palette_entries(&self, io: &ShellIo<'_>) -> Vec<(PaletteEntry, PaletteAction)> {
        let keymap = &self.config.keymap;
        let request = PaletteAction::Request;
        let command = |name: &str| request(ShellRequest::Command(name.to_owned()));
        let mut entries = vec![(
            entry("Go to the Hub", String::new()),
            request(ShellRequest::Hub),
        )];
        for manifest in io.applets.manifests() {
            if manifest.role.is_page() {
                entries.push((
                    entry(&format!("Open {}", manifest.name), String::new()),
                    request(ShellRequest::Open(manifest.id.clone())),
                ));
            }
        }
        let pinned = pinned(&io.world.rice, io.applets);
        if let Some(page) = self.page.clone()
            && pinned.as_ref() != Some(&page)
        {
            let name = io
                .applets
                .manifest(&page)
                .map_or_else(String::new, |manifest| manifest.name.clone());
            entries.push((
                entry(&format!("Pin {name}: Open It at Launch"), String::new()),
                request(ShellRequest::Pin(Some(page))),
            ));
        }
        if pinned.is_some() {
            entries.push((
                entry("Unpin: Open the Hub at Launch", String::new()),
                request(ShellRequest::Pin(None)),
            ));
        }
        entries.extend([
            (
                entry("Open Settings", keymap.open_settings.to_string()),
                request(ShellRequest::OpenSettings(None)),
            ),
            (
                entry("Undo", keymap.undo.to_string()),
                PaletteAction::Undo(false),
            ),
            (
                entry("Redo", keymap.redo.to_string()),
                PaletteAction::Undo(true),
            ),
            (
                entry("Open File", keymap.open_file.to_string()),
                command("file.open"),
            ),
            (
                entry("Open Project", String::new()),
                request(ShellRequest::PickFile(
                    FilePick::open(None, "open", "Open Project").extensions(&["sue"]),
                )),
            ),
            (
                entry("Save Project", keymap.save.to_string()),
                command("file.save"),
            ),
            (
                entry("Save Project As", String::new()),
                command("file.save_as"),
            ),
            (
                entry("Quit Sue", keymap.quit.to_string()),
                command("app.quit"),
            ),
        ]);
        for (applet, template) in templates(io.applets) {
            entries.push((
                entry(
                    &format!("New {}: {}", template.category, template.label),
                    String::new(),
                ),
                request(ShellRequest::Create {
                    applet,
                    key: template.key,
                }),
            ));
        }
        for (manifest, applet) in io.applets.iter() {
            for command in applet.commands() {
                entries.push((
                    PaletteEntry {
                        label: format!("{}: {}", manifest.name, command.label),
                        command: command.command.clone(),
                        hint: command.hint.clone(),
                    },
                    PaletteAction::Applet(manifest.id.clone(), command.command),
                ));
            }
        }
        entries
    }

    fn palette_ui(
        &mut self,
        ctx: &egui::Context,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
    ) {
        if self.palette.is_none() {
            return;
        }
        let entries = self.palette_entries(io);
        let shown: Vec<PaletteEntry> = entries.iter().map(|(entry, _)| entry.clone()).collect();
        let area = self.page_rect;
        let outcome = self
            .palette
            .as_mut()
            .map_or(PaletteOutcome::Open, |palette| {
                palette.ui(ctx, area, &shown, theme)
            });
        self.touch();
        if let PaletteOutcome::Run(index) = outcome
            && let Some((_, action)) = entries.into_iter().nth(index)
        {
            match action {
                PaletteAction::Request(request) => {
                    if let Some(request) = self.handle(request, io, now) {
                        io.requests.push(request);
                    }
                }
                PaletteAction::Undo(redo) => self.undo(io, redo),
                PaletteAction::Applet(id, command) => {
                    let view = self.view(now, 0.0);
                    let (applets, mut cx) = split(io, theme, view);
                    if let Some(applet) = applets.get_mut(&id)
                        && !applet.command(&command, &mut cx)
                    {
                        cx.fail(format!("{command} is not available"));
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments, reason = "a window over the screen")]
    fn window_ui(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        let Some(id) = self.window.clone() else {
            return;
        };
        let appear = self.window_appear.position().clamp(0.0, 1.0);
        // A scrim that closes the window when clicked.
        let scrim = egui::Area::new(Id::unique("sue-window-scrim"))
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, theme.shadow.gamma_multiply(0.45 * appear));
                response.clicked()
            })
            .inner;
        if scrim {
            self.close_window();
        }
        let size = vec2(
            (screen.width() - 48.0).clamp(320.0, 980.0),
            (screen.height() - 64.0).clamp(320.0, 700.0),
        );
        let at = screen.center() - size / 2.0 + vec2(0.0, (1.0 - appear) * 18.0);
        let name = io
            .applets
            .manifest(&id)
            .map(|manifest| manifest.name.clone())
            .unwrap_or_default();
        let view = self.view(now, dt);
        let mut close = false;
        egui::Area::new(Id::unique("sue-window"))
            .order(Order::Foreground)
            .fixed_pos(at)
            .interactable(appear > 0.3)
            .show(ctx, |ui| {
                ui.set_opacity(appear);
                widgets::window_frame(theme).show(ui, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    let bar = Rect::from_min_size(ui.min_rect().min, vec2(size.x, 36.0));
                    let mut bar_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(bar.shrink2(vec2(theme.padding * 1.5, 0.0)))
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    bar_ui.label(RichText::new(&name).size(theme.ui_size).color(theme.text));
                    bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("\u{00D7}")
                                        .size(theme.ui_size * 1.3)
                                        .color(theme.muted),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Close (Esc)")
                            .clicked()
                        {
                            close = true;
                        }
                    });
                    ui.painter()
                        .hline(bar.x_range(), bar.bottom(), theme.hairline());
                    let body =
                        Rect::from_min_max(pos2(bar.left(), bar.bottom()), bar.left_top() + size);
                    let mut body_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(body)
                            .id_salt(("sue-window-body", id.as_str()))
                            .layout(Layout::top_down(Align::Min)),
                    );
                    body_ui.set_clip_rect(body);
                    let (applets, mut cx) = split(io, theme, view);
                    if let Some(applet) = applets.get_mut(&id) {
                        applet.window_ui(&mut body_ui, &mut cx);
                    }
                    ui.allocate_rect(Rect::from_min_size(bar.min, size), Sense::hover());
                });
            });
        if close {
            self.close_window();
        }
    }

    /// Runs every applet's tick. Returns what they asked for.
    pub fn tick(&mut self, io: &mut ShellIo<'_>, now: Instant) -> crate::Tick {
        let view = self.view(now, 0.0);
        let theme = self.theme.clone();
        let (applets, mut cx) = split(io, &theme, view);
        let mut tick = crate::Tick::default();
        for (_, applet) in applets.iter_mut() {
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

fn entry(label: &str, hint: String) -> PaletteEntry {
    PaletteEntry {
        label: label.to_owned(),
        command: label.to_owned(),
        hint,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use sue_core::Arena;
    use sue_platform::NoTextures;
    use sue_rice::{RiceDeclaration, RiceParameter};

    use super::*;
    use crate::{AppletManifest, Launchpad, SueApplet};

    /// A studio that wears Paper and notes the widget fill its page and its
    /// menus are drawn with.
    struct Probe(Arc<Mutex<Vec<egui::Color32>>>);

    impl SueApplet for Probe {
        fn manifest(&self) -> AppletManifest {
            AppletManifest::new("probe", "Probe", AppletRole::Studio, "*")
        }

        fn rice_declarations(&self) -> Vec<RiceDeclaration> {
            vec![RiceDeclaration::new(
                "probe.theme",
                RiceParameter::Text {
                    value: "paper".to_owned(),
                },
                "The probe's colors",
            )]
        }

        fn page_ui(&mut self, ui: &mut Ui, _cx: &mut AppletContext<'_>) {
            self.0
                .lock()
                .unwrap()
                .push(ui.visuals().widgets.inactive.bg_fill);
        }

        fn bar_ui(&mut self, ui: &mut Ui, _cx: &mut AppletContext<'_>) {
            self.0
                .lock()
                .unwrap()
                .push(ui.visuals().widgets.inactive.bg_fill);
        }
    }

    #[test]
    fn a_page_and_its_menus_wear_their_own_theme() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut launchpad = Launchpad::new();
        let probe = Arc::clone(&seen);
        launchpad
            .register(move || Box::new(Probe(Arc::clone(&probe))))
            .unwrap();
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        rice.register_all(&launchpad.rice_declarations()).unwrap();
        let mut world = World {
            arena: Arena::new(),
            rice,
        };
        let mut history = UndoStack::new();
        let mut typing = Typing::default();
        let mut applets = launchpad.instantiate();
        let catalog = Catalog::default();
        let mut transport = Transport::default();
        let mut requests = Vec::new();
        let work = WorkList::default();
        let theme = Theme::from_rice(&world.rice);
        let mut shell = Shell::new(
            WmConfig::from_rice(&world.rice).unwrap(),
            theme.clone(),
            HardwareProfile::Balanced,
            60,
            FormFactor::Desktop,
        );
        shell.open(AppletId::new("probe").unwrap(), &applets);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let mut io = ShellIo {
                world: &mut world,
                history: &mut history,
                typing: &mut typing,
                applets: &mut applets,
                catalog: &catalog,
                textures: &mut NoTextures,
                audio: None,
                transport: &mut transport,
                requests: &mut requests,
                work: &work,
                fps: None,
            };
            ctx.run_ui(egui::RawInput::default(), |ui| {
                shell.ui(ui, &mut io, Instant::now());
            })
            .drop_without_applying_deltas();
        }
        // Sue wears its dark default; the page and its menus wear Paper,
        // whose fields are white.
        let paper = sue_rice::builtin_presets()
            .into_iter()
            .find(|preset| preset.id == "paper")
            .map(|preset| theme.with_preset(&preset))
            .unwrap();
        assert_ne!(paper.pill, theme.pill);
        let seen = seen.lock().unwrap();
        assert!(seen.len() >= 4, "{seen:?}");
        assert!(seen.iter().all(|fill| *fill == paper.pill), "{seen:?}");
        // Sue's own style is back for the Hub, the status bar and windows.
        assert_eq!(
            ctx.global_style().visuals.widgets.inactive.bg_fill,
            theme.pill
        );
    }
}
