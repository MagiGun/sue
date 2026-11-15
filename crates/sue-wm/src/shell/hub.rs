//! The Hub: Sue's workspace, a set of places. A rail at the left holds
//! them (a tab bar at the bottom when the room is narrow): Home, Files,
//! then the places applets add through [`crate::SueApplet::hub_places`]
//! (Sue Paint's materials). Home has a card per studio, New and Open, the
//! work to jump back into (on a first run, kinds of work that start with
//! one click and what is good to know) and the buttons of Your work,
//! which open Files already sorted; Files is the shared browser of [`crate::files`] over
//! the work of every studio, kept work included.
//!
//! Layout by room, sizes by touch: under [`NARROW`] points the rail turns
//! into a tab bar and the cards into rows, on any device; a touch layout
//! makes every target finger sized. Hover and press fade on the riced
//! motion and snap when transitions are instant, so an idle Hub asks for
//! no frame.

mod files;
mod home;
mod kit;

use std::path::PathBuf;

use egui::{Align, Layout, Rect, Ui, UiBuilder};
use sue_platform::FormFactor;
use sue_platform::time::Instant;

use super::{Shell, ShellIo, split};
use crate::files::{Browse, ProjectKey, ProjectsState, Thumbs};
use crate::icons::Icon;
use crate::ui::{Density, FocusScope, Roving, Space};
use crate::{AppletId, MenuBar, ShellRequest, Theme};

/// Narrower than this many points, the Hub lays out as on a phone: a tab
/// bar for the rail, rows for the cards.
pub const NARROW: f32 = 560.0;

/// The id of the Hub's Home place.
pub const HUB_HOME: &str = "home";

/// The id of the Hub's Files place.
pub const HUB_FILES: &str = "files";

/// What a card or a row of the Hub's Files place does when clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HubRow {
    /// Open a recent file or a saved project in its studio or tool.
    Open(PathBuf),
    /// Bring back work Sue kept and open it in its studio.
    Recover(String),
    /// Throw kept work away (a first click asks, a second one does it).
    Discard(String),
}

/// A shelf of Your work: the Files place, sorted and filtered for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkShelf {
    /// Everything, newest first.
    Recent,
    /// Sue projects only.
    Projects,
    /// The work Sue kept that nobody saved.
    Unsaved,
    /// What was marked a favorite.
    Favorites,
}

impl WorkShelf {
    /// Every shelf, in the order Home shows them.
    pub const ALL: [Self; 4] = [Self::Recent, Self::Projects, Self::Unsaved, Self::Favorites];

    /// Its name.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Recent => tr!("recent"),
            Self::Projects => tr!("hub-projects"),
            Self::Unsaved => tr!("hub-unsaved-work"),
            Self::Favorites => tr!("hub-favorites"),
        }
    }

    /// Its icon.
    #[must_use]
    pub const fn icon(self) -> Icon {
        match self {
            Self::Recent => Icon::Recent,
            Self::Projects => Icon::Projects,
            Self::Unsaved => Icon::Recover,
            Self::Favorites => Icon::Heart,
        }
    }
}

/// A button of the Hub other than a studio's card (those are in
/// [`Shell::hub_boxes`]) and a file's card (in [`Shell::hub_rows`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HubButton {
    /// A place in the rail or the tab bar, by its id: [`HUB_HOME`], [`HUB_FILES`]
    /// or an applet's.
    Place(String),
    /// New: a menu of every studio's kinds of work.
    New,
    /// Open: the system's open dialog.
    Open,
    /// A button of Your work: the Files place on that shelf.
    Work(WorkShelf),
    /// Folder: Sue's home folder in the system's file manager (only where
    /// there is one).
    Folder,
    /// All Files, beside Jump back in.
    AllFiles,
    /// A card of Jump back in, by its file.
    Jump(PathBuf),
    /// The search field of the top bar: the command palette.
    Search,
    /// The Transfers button of the top bar.
    Transfers,
    /// The Settings button of the top bar.
    Settings,
    /// The menu of the Files place: rebuild thumbnails, forget missing
    /// files, clear finished transfers.
    Upkeep,
    /// A kind of work under Start Something New, which Home shows while
    /// there is no work to jump back into: the studio that makes it and
    /// the template's key.
    Start(AppletId, String),
    /// Good to Know's Find Any Command: the command palette.
    FindCommands,
    /// Good to Know's Open Your Files: the system's open dialog.
    OpenFiles,
    /// Good to Know's Make Sue Yours: Settings.
    MakeItYours,
}

/// What the Hub was asked to do this frame, done after drawing.
#[derive(Clone, Debug, PartialEq)]
enum HubAct {
    /// Go to a place.
    Place(String),
    /// Go to Files on a shelf of Your work.
    Work(WorkShelf),
    /// Open a studio or tool.
    Applet(AppletId),
    /// Pin a studio, or take its pin away.
    Pin(AppletId),
    /// Hand the engine a request.
    Request(ShellRequest),
    /// Open a file, or forget it with a word when it is gone.
    OpenFile(PathBuf),
}

/// The Hub's state between frames.
pub(super) struct HubState {
    /// The Files place's browser: shelf, search, chips, sort and view.
    files: ProjectsState,
    /// Its shape and where it drew its cards.
    browse: Browse,
    /// The cards' pictures, made in the background.
    thumbs: Thumbs,
    /// The studios' own thumbnail makers were handed to [`Self::thumbs`].
    makers: bool,
    /// New's menu.
    new_menu: MenuBar,
    /// The Files place's menu.
    upkeep: MenuBar,
    /// The item whose details show, and whether its Remove asks again.
    details: Option<(ProjectKey, bool)>,
    /// The Transfers popup shows.
    transfers: bool,
    /// Where the last frame drew each button.
    pub(super) buttons: Vec<(HubButton, Rect)>,
    /// This frame's controls in reading order, for the arrow keys, and
    /// the one focus starts on.
    walk: Vec<egui::Response>,
    first: Option<egui::Response>,
}

impl Default for HubState {
    fn default() -> Self {
        Self {
            files: ProjectsState::default(),
            browse: Browse::default(),
            thumbs: Thumbs::new(None),
            makers: false,
            new_menu: MenuBar::new(),
            upkeep: MenuBar::new(),
            details: None,
            transfers: false,
            buttons: Vec::new(),
            walk: Vec::new(),
            first: None,
        }
    }
}

/// A place of the rail: its id, its name, its icon, and the applet that
/// draws it (`None`: one of the Hub's own).
struct Place {
    id: String,
    label: String,
    icon: Icon,
    applet: Option<AppletId>,
}

impl Shell {
    /// The places of the rail: Home, Files, then every applet's.
    fn hub_places(io: &ShellIo<'_>) -> Vec<Place> {
        let mut places = vec![
            Place {
                id: HUB_HOME.to_owned(),
                label: tr!("hub-home"),
                icon: Icon::Home,
                applet: None,
            },
            Place {
                id: HUB_FILES.to_owned(),
                label: tr!("hub-files"),
                icon: Icon::Files,
                applet: None,
            },
        ];
        for (manifest, applet) in io.applets.iter() {
            let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &manifest.id));
            for place in applet.hub_places() {
                if places.iter().any(|known| known.id == place.id) {
                    continue;
                }
                places.push(Place {
                    id: place.id,
                    label: place.label,
                    icon: place.icon,
                    applet: Some(manifest.id.clone()),
                });
            }
        }
        places
    }

    /// The place the Hub shows: the one asked for, if it exists, else
    /// Home.
    fn hub_current(&self, places: &[Place]) -> String {
        self.hub_place
            .as_ref()
            .filter(|asked| places.iter().any(|place| &place.id == *asked))
            .cloned()
            .unwrap_or_else(|| HUB_HOME.to_owned())
    }

    /// The Hub: the rail (or the tab bar) and the place chosen.
    #[allow(clippy::too_many_arguments, reason = "one frame of the Hub")]
    pub(super) fn hub_ui(
        &mut self,
        root: &mut Ui,
        area: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        root.painter().rect_filled(area, 0.0, theme.surface(0));
        self.hub.buttons.clear();
        self.hub.walk.clear();
        self.hub.first = None;
        if !self.hub.makers {
            for (manifest, applet) in io.applets.iter() {
                if let Some(maker) = applet.thumbnail() {
                    self.hub.thumbs.set_maker(manifest.id.clone(), maker);
                }
            }
            self.hub.makers = true;
        }
        // A page on its way in takes no clicks on the Hub under it.
        let live = self.page.is_none();
        let touch = self.form_factor() != FormFactor::Desktop;
        let narrow = area.width() < NARROW;
        let places = Self::hub_places(io);
        let current = self.hub_current(&places);
        let mut ui = root.new_child(
            UiBuilder::new()
                .max_rect(area)
                .id_salt("sue-hub")
                .layout(Layout::top_down(Align::Min)),
        );
        ui.set_clip_rect(area);
        if !live {
            ui.disable();
        }
        let density = if touch {
            Density::Touch
        } else {
            Density::Regular
        };
        let mut acts = Vec::new();
        density.scope(&mut ui, |ui| {
            let items: Vec<(String, String, Icon)> = places
                .iter()
                .map(|place| (place.id.clone(), place.label.clone(), place.icon))
                .collect();
            let (nav, pane) = kit::split_area(area, narrow, density, theme);
            let (clicked, rects) = kit::places(ui, nav, &items, &current, narrow, theme);
            for (id, response) in rects {
                if id == current {
                    self.hub.first = Some(response.clone());
                }
                self.hub.buttons.push((HubButton::Place(id), response.rect));
                self.hub.walk.push(response);
            }
            if let Some(id) = clicked {
                acts.push(HubAct::Place(id));
            }
            let mut pane_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(pane)
                    .id_salt(("sue-hub-place", current.as_str()))
                    .layout(Layout::top_down(Align::Min)),
            );
            pane_ui.set_clip_rect(pane.intersect(area));
            match current.as_str() {
                HUB_HOME => self.home_ui(&mut pane_ui, pane, io, theme, narrow, &mut acts),
                HUB_FILES => {
                    let view = self.view(now, dt);
                    self.files_ui(
                        &mut pane_ui,
                        pane,
                        io,
                        theme,
                        view,
                        narrow || touch,
                        &mut acts,
                    );
                }
                place => {
                    let view = self.view(now, dt);
                    if let Some(id) = places
                        .iter()
                        .find(|known| known.id == place)
                        .and_then(|known| known.applet.clone())
                    {
                        let _language =
                            sue_lang::scope(&crate::applet_language(&io.world.rice, &id));
                        let (applets, mut cx) = split(io, theme, view);
                        if let Some(applet) = applets.get_mut(&id) {
                            applet.hub_ui(place, &mut pane_ui, &mut cx);
                        }
                    }
                }
            }
        });
        // The keyboard: Tab is the command palette's, so the arrow keys
        // walk the Hub's controls in reading order, starting on the place
        // shown; Enter or Space uses the one that has focus.
        if live {
            let mut scope = FocusScope::new("sue-hub").roving(Roving::Both);
            let first = self.hub.first.take();
            for response in self.hub.walk.drain(..) {
                if first.as_ref().is_some_and(|first| first.id == response.id) {
                    scope.add_initial(&response);
                } else {
                    scope.add(&response);
                }
            }
            scope.finish(root.ctx());
        }
        if self.hub.thumbs.poll() {
            root.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.hub.transfers {
            let view = self.view(now, dt);
            let (_, mut cx) = split(io, theme, view);
            let full = narrow || touch;
            if crate::files::transfers_popup(full, root.ctx(), &mut cx, theme)
                == crate::start::PopupOutcome::Closed
            {
                self.hub.transfers = false;
            }
        }
        if live {
            for act in acts {
                self.hub_act(act, io);
            }
        }
    }

    /// Does what the Hub was asked to.
    fn hub_act(&mut self, act: HubAct, io: &mut ShellIo<'_>) {
        match act {
            HubAct::Place(id) => {
                self.discard_armed = None;
                self.hub_place = (id != HUB_HOME).then_some(id);
            }
            HubAct::Work(shelf) => {
                self.hub.files = files::state_for(shelf);
                self.hub_place = Some(HUB_FILES.to_owned());
            }
            HubAct::Applet(id) => self.open(id, io.applets),
            HubAct::Pin(id) => {
                let again = super::pinned(&io.world.rice, io.applets).as_ref() == Some(&id);
                self.pin((!again).then_some(id), io);
            }
            HubAct::Request(request) => io.requests.push(request),
            HubAct::OpenFile(path) => {
                self.discard_armed = None;
                if sue_platform::fs::exists(&path) {
                    io.requests.push(ShellRequest::OpenFile(path));
                } else {
                    self.notify(tr!("hub-file-gone", path = path.display()), true);
                    io.requests.push(ShellRequest::Forget(path));
                }
            }
        }
        self.touch();
    }

    /// `true` while the Hub's thumbnails are still being made (it takes
    /// in those that are done).
    pub fn hub_working(&mut self) -> bool {
        self.hub.thumbs.poll()
    }

    /// `true` when the Files place holds a finished thumbnail of `path`
    /// (asking starts one in the background if there is none).
    pub fn hub_has_thumbnail(&mut self, path: &std::path::Path) -> bool {
        self.hub.thumbs.get(path).is_some()
    }

    /// Opens the Transfers popup over the Hub.
    pub(super) fn hub_transfers(&mut self) {
        self.hub.transfers = true;
        self.touch();
    }

    /// The margin around the Hub's places: roomy on a wide screen, tight
    /// on a narrow one.
    fn hub_margin(narrow: bool, theme: &Theme) -> f32 {
        theme.space(if narrow { Space::S6 } else { Space::S9 })
    }
}
