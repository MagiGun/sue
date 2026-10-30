//! Sue Paint's start page, shown on the desktop while no canvas is open
//! (and from File, Start Page): a rail with Draw, Projects, Manage
//! Materials and Browse Assets on the left, Data Transfers, Help and
//! Settings at the top right, and the projects, the materials or CLIP
//! STUDIO ASSETS in the main area. Only Browse Assets goes online (see
//! [`crate::assets`]); everything else is on this device.
//!
//! The projects are the work Sue remembers, shared with the Hub and every
//! studio: the pictures and Sue Paint projects opened and saved lately and
//! the work kept while not saved, sorted with the shared labels. The
//! materials are the entities of the material [`Library`], whose kinds
//! register in a [`crate::materials::MaterialKinds`] registry.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{Id, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use sue_core::{EntityId, JobHandle, JobPool, Properties};
use sue_wm::start::{
    self as page, Card, CardClick, Category, CategoryClick, Chosen, Glyph, PopupOutcome, RailItem,
    Row, Trailing,
};
use sue_wm::{
    AppletContext, AppletId, FilePick, LabelEdit, ShellRequest, Theme, TransferKind, TransferState,
    ago, unix_now,
};

use crate::materials::Library;
use crate::studio::SuePaint;

/// The Sue site and where people ask and tell.
const SITE: &str = "https://magigun.github.io/sue/";
const GUIDE: &str =
    "https://github.com/MagiGun/sue/blob/main/HUMANS.md#sue-paint-painting-and-illustration";
const FAQ: &str = "https://github.com/MagiGun/sue/issues?q=label%3Aquestion";
const CONTACT: &str = "https://github.com/MagiGun/sue/issues/new/choose";
const FEEDBACK: &str = "https://github.com/MagiGun/sue/issues/new?template=feature.yml";

/// How wide the rail and the category column are.
const RAIL: f32 = 84.0;
const COLUMN: f32 = 210.0;

/// What the main area shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    /// The projects on this device.
    #[default]
    Projects,
    /// The materials.
    Materials,
    /// Browse Assets: CLIP STUDIO ASSETS, read from its site.
    Browse,
}

/// A popup over the start page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Popup {
    /// Data Transfers.
    Transfers,
    /// Help.
    Help,
    /// Settings.
    Settings,
    /// Settings, Maintenance Menu.
    Maintenance,
    /// What a project is.
    Project(ProjectKey),
    /// What a material is.
    Material(EntityId),
}

/// How the cards lie.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum View {
    /// Large cards.
    #[default]
    Large,
    /// Small cards.
    Small,
    /// Rows.
    List,
}

/// Which projects show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Shelf {
    /// Every one.
    #[default]
    All,
    /// The favorites.
    Favorite,
    /// The ones wearing a label.
    Label(String),
}

/// What a project card stands for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ProjectKey {
    /// A picture or project file.
    File(PathBuf),
    /// Work Sue kept while it was not saved, by its id.
    Kept(String),
}

/// What kind of project a card is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Type {
    /// A picture file.
    Picture,
    /// A Sue project.
    Project,
    /// Work kept while it was not saved.
    Unsaved,
}

impl Type {
    const ALL: [Self; 3] = [Self::Picture, Self::Project, Self::Unsaved];

    fn label(self) -> String {
        match self {
            Self::Picture => tr!("start-type-picture"),
            Self::Project => tr!("start-type-project"),
            Self::Unsaved => tr!("start-type-unsaved"),
        }
    }

    fn glyph(self) -> Glyph {
        match self {
            Self::Picture => Glyph::Picture,
            Self::Project => Glyph::Projects,
            Self::Unsaved => Glyph::Clock,
        }
    }
}

/// One project on the start page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub(crate) key: ProjectKey,
    /// The file (the kept project for unsaved work).
    pub(crate) path: PathBuf,
    pub(crate) title: String,
    /// When it was last opened, saved or kept.
    pub(crate) when: u64,
    pub(crate) kind: Type,
    pub(crate) favorite: bool,
}

/// The projects of Sue Paint in the work Sue remembers: pictures, Sue
/// Paint projects and its kept work.
pub(crate) fn items(work: &sue_wm::WorkList) -> Vec<Item> {
    let paint = AppletId::new(crate::ID).ok();
    let mut items: Vec<Item> = work
        .recent()
        .filter_map(|file| {
            let kind = if file.is_project() {
                (file.page == paint).then_some(Type::Project)?
            } else if crate::studio::opens(&file.path) {
                Type::Picture
            } else {
                return None;
            };
            Some(Item {
                key: ProjectKey::File(file.path.clone()),
                path: file.path.clone(),
                title: file.title(),
                when: file.last(),
                kind,
                favorite: work.is_favorite(&file.path),
            })
        })
        .collect();
    items.extend(
        work.unsaved()
            .filter(|kept| kept.page == paint)
            .map(|kept| Item {
                key: ProjectKey::Kept(kept.id.clone()),
                path: kept.file.clone(),
                title: kept.title(),
                when: kept.at,
                kind: Type::Unsaved,
                favorite: work.is_favorite(&kept.file),
            }),
    );
    items
}

/// The projects `state` shows, in its order.
pub(crate) fn shown(items: &[Item], state: &ProjectsState, work: &sue_wm::WorkList) -> Vec<Item> {
    let needle = state.search.trim().to_lowercase();
    let mut shown: Vec<Item> = items
        .iter()
        .filter(|item| match &state.shelf {
            Shelf::All => true,
            Shelf::Favorite => item.favorite,
            Shelf::Label(label) => work.has_label(&item.path, label),
        })
        .filter(|item| state.types.is_empty() || state.types.contains(&(item.kind as u8)))
        .filter(|item| needle.is_empty() || item.title.to_lowercase().contains(&needle))
        .cloned()
        .collect();
    match state.sort {
        1 => shown.sort_by_cached_key(|item| item.title.to_lowercase()),
        _ => shown.sort_by_key(|item| std::cmp::Reverse(item.when)),
    }
    if state.reversed {
        shown.reverse();
    }
    shown
}

impl ProjectsState {
    /// Every project on `shelf`, newest first.
    pub(crate) fn on_shelf(shelf: Shelf) -> Self {
        Self {
            shelf,
            ..Self::default()
        }
    }
}

/// The Projects page's state.
#[derive(Debug, Default)]
pub(crate) struct ProjectsState {
    pub(crate) shelf: Shelf,
    pub(crate) search: String,
    /// 0 by date modified (newest first), 1 by name.
    pub(crate) sort: usize,
    pub(crate) reversed: bool,
    pub(crate) view: View,
    /// The type chips that are on (none: every type).
    pub(crate) types: HashSet<u8>,
    pub(crate) chosen: HashSet<ProjectKey>,
    /// A label being renamed, and its new name so far.
    renaming: Option<(String, String)>,
    /// A label just added, to rename once it shows.
    adding: Option<String>,
    /// Remove from List was clicked once and asks again.
    armed: bool,
}

/// A branch of the materials tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Node {
    /// Every material.
    #[default]
    All,
    /// One kind, by id.
    Kind(String),
    /// The favorites.
    Favorite,
    /// The ones used, newest first.
    Recent,
}

/// The Manage Materials page's state.
#[derive(Debug)]
pub(crate) struct MaterialsState {
    pub(crate) node: Node,
    /// All materials shows its kinds under it.
    open: bool,
    pub(crate) search: String,
    /// 0 by date added, 1 by name, 2 by last used.
    pub(crate) sort: usize,
    pub(crate) reversed: bool,
    pub(crate) view: View,
    /// The chips that are on: 0 Sue Paint's own, 1 added by you.
    pub(crate) origins: HashSet<u8>,
    pub(crate) chosen: HashSet<EntityId>,
    armed: bool,
}

impl Default for MaterialsState {
    fn default() -> Self {
        Self {
            node: Node::All,
            open: true,
            search: String::new(),
            sort: 0,
            reversed: false,
            view: View::Large,
            origins: HashSet::new(),
            chosen: HashSet::new(),
            armed: false,
        }
    }
}

/// A picture made small for a card.
#[derive(Clone, Debug)]
pub(crate) struct Thumb {
    pub(crate) size: [u32; 2],
    pub(crate) pixels: Vec<u8>,
}

/// Project thumbnails, made in the background.
#[derive(Debug, Default)]
pub(crate) struct Thumbs {
    pool: Option<JobPool>,
    jobs: HashMap<PathBuf, (u64, JobHandle<Thumb>)>,
    done: HashMap<PathBuf, (u64, Option<Thumb>)>,
}

/// When `path` last changed, in seconds since 1970 (0: unknown).
fn stamp(path: &Path) -> u64 {
    sue_platform::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified())
        .and_then(|time| time.duration_since(sue_platform::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs())
}

impl Thumbs {
    /// The thumbnail of `path`, starting it in the background if it is
    /// new or changed.
    pub(crate) fn get(&mut self, path: &Path) -> Option<(u64, &Thumb)> {
        let now = stamp(path);
        let fresh = self.done.get(path).is_some_and(|(at, _)| *at == now);
        if !fresh && !self.jobs.contains_key(path) {
            let pool = self.pool.get_or_insert_with(|| JobPool::new(1));
            let file = path.to_path_buf();
            let job = pool.spawn(tr!("start-thumbnail"), move |job| {
                thumbnail(&file, 256, job).ok_or_else(String::new)
            });
            self.jobs.insert(path.to_path_buf(), (now, job));
        }
        self.done
            .get(path)
            .and_then(|(at, thumb)| thumb.as_ref().map(|thumb| (*at, thumb)))
    }

    /// Takes in what the background finished. Returns `true` while any
    /// still runs.
    pub(crate) fn poll(&mut self) -> bool {
        let mut finished = Vec::new();
        for (path, (at, job)) in &mut self.jobs {
            if let Some(result) = job.try_take() {
                finished.push((path.clone(), *at, result.ok()));
            }
        }
        for (path, at, thumb) in finished {
            self.jobs.remove(&path);
            self.done.insert(path, (at, thumb));
        }
        !self.jobs.is_empty()
    }

    fn clear(&mut self) {
        self.done.clear();
    }
}

/// A picture file or a Sue project made at most `side` pixels on its long
/// edge: the picture, or the project's first picture flattened.
fn thumbnail(path: &Path, side: u32, job: &sue_core::JobContext) -> Option<Thumb> {
    let bytes = sue_platform::fs::read(path).ok()?;
    job.set_progress(0.3);
    let raster = if sue_core::SueFile::sniff(&bytes) {
        let arena = sue_core::SueFile::from_bytes(&bytes)
            .ok()?
            .arena(&sue_model::registry())
            .ok()?;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())?;
        crate::composite::flatten(
            &arena,
            picture,
            crate::composite::RenderOptions {
                paper: true,
                drafts: false,
            },
        )?
    } else if sue_io::clip::sniff(&bytes) {
        let (arena, picture) = crate::clip_io::picture_arena(&bytes)?;
        crate::composite::flatten(
            &arena,
            picture,
            crate::composite::RenderOptions {
                paper: true,
                drafts: false,
            },
        )?
    } else if bytes.starts_with(b"8BPS") {
        let psd = sue_io::psd::decode_psd(&bytes).ok()?;
        sue_core::RasterTile2D::from_bytes(
            psd.width,
            psd.height,
            sue_core::PixelFormat::Rgba8,
            psd.merged,
        )
        .ok()?
    } else {
        let file = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let bundle = sue_io::formats(sue_model::registry())
            .import(&bytes, &file, &Properties::new())
            .ok()?;
        bundle
            .entities
            .iter()
            .find_map(|entity| {
                entity
                    .component(sue_core::ComponentKind::RasterTile2D)?
                    .as_raster()
            })?
            .clone()
    };
    job.set_progress(0.7);
    let rgba = crate::studio::to_rgba(&raster);
    Some(shrink(&rgba, side))
}

/// `tile` (RGBA8) box-filtered down to at most `side` on its long edge.
fn shrink(tile: &sue_core::RasterTile2D, side: u32) -> Thumb {
    let (width, height) = (tile.width().max(1), tile.height().max(1));
    let scale = (side as f32 / width.max(height) as f32).min(1.0);
    let (w, h) = (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    );
    let bytes = tile.bytes().unwrap_or(&[]);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let (y0, y1) = (
            y * height / h,
            ((y + 1) * height / h).max(y * height / h + 1),
        );
        for x in 0..w {
            let (x0, x1) = (x * width / w, ((x + 1) * width / w).max(x * width / w + 1));
            let mut sum = [0_u64; 4];
            let mut count = 0_u64;
            // At most a 4 by 4 sample of the box, so big pictures stay quick.
            let step_y = ((y1 - y0) / 4).max(1);
            let step_x = ((x1 - x0) / 4).max(1);
            for sy in (y0..y1.min(height)).step_by(step_y as usize) {
                for sx in (x0..x1.min(width)).step_by(step_x as usize) {
                    let at = ((sy * width + sx) * 4) as usize;
                    if let Some(pixel) = bytes.get(at..at + 4) {
                        // Over white, as on paper.
                        let alpha = u64::from(pixel[3]);
                        for channel in 0..3 {
                            sum[channel] +=
                                (u64::from(pixel[channel]) * alpha + 255 * (255 - alpha)) / 255;
                        }
                        sum[3] += 255;
                        count += 1;
                    }
                }
            }
            let count = count.max(1);
            pixels.extend(sum.map(|value| (value / count) as u8));
        }
    }
    Thumb {
        size: [w, h],
        pixels,
    }
}

/// A texture key for `text`, apart from the canvases' keys.
pub(crate) fn texture_key(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("sue-paint-start", text).hash(&mut hasher);
    hasher.finish()
}

/// Everything the start page keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct StartPage {
    /// File, Start Page asked for it over open canvases.
    pub(crate) forced: bool,
    pub(crate) tab: Tab,
    pub(crate) popup: Option<Popup>,
    pub(crate) projects: ProjectsState,
    pub(crate) materials: MaterialsState,
    /// The material library, found when first shown.
    library: Option<(Option<PathBuf>, Library)>,
    pub(crate) thumbs: Thumbs,
    /// Material files being copied in, by transfer id.
    copies: Vec<u64>,
    /// Remove (a material or a kept project) was clicked once in a popup.
    armed: bool,
    /// Popups fill the screen (phones and tablets).
    pub(crate) full: bool,
    /// The New canvas page (phones and tablets) shows, on this category.
    pub(crate) new_canvas: Option<usize>,
    /// The device's screen in pixels, for the Screen size template.
    pub(crate) screen: [u32; 2],
    /// Home's browser of CLIP STUDIO ASSETS: where it is and was.
    pub(crate) browser: crate::assets::Browser,
}

/// What the start page asks the studio for, after drawing.
pub(crate) enum Act {
    Draw,
    Open(Item),
    Paste(EntityId),
    PickMaterial,
    PickLocation,
    /// Show the materials.
    Materials,
    /// Make a canvas from the template with this id.
    Template(String),
    /// The full New dialog, for a category.
    Advanced(usize),
    /// A picture from the device becomes a new canvas.
    ImportImage,
    /// Open any file Sue opens.
    OpenFile,
}

impl StartPage {
    /// `true` while it shows over the canvases: there are none, or File,
    /// Start Page asked for it.
    pub(crate) fn shows(&self, no_canvas: bool) -> bool {
        no_canvas || self.forced
    }

    /// The materials changed on disk; find them again.
    pub(crate) fn materials_changed(&mut self) {
        self.library = None;
    }

    /// Opens a page or popup by name (`start/projects`, `start/help` and
    /// so on), for scripts and snapshots.
    pub(crate) fn navigate(&mut self, target: &str) -> bool {
        match target {
            "start/browse" => {
                self.tab = Tab::Browse;
                self.browser.go(None);
            }
            "start/projects" => self.tab = Tab::Projects,
            "start/materials" => self.tab = Tab::Materials,
            "start/transfers" => self.popup = Some(Popup::Transfers),
            "start/help" => self.popup = Some(Popup::Help),
            "start/settings" => self.popup = Some(Popup::Settings),
            "start/maintenance" => self.popup = Some(Popup::Maintenance),
            "start/close" => self.popup = None,
            "start/show" => self.forced = true,
            "start/hide" => {
                self.forced = false;
                self.popup = None;
                self.new_canvas = None;
                self.tab = Tab::Projects;
            }
            "start/new" => self.new_canvas = Some(0),
            other if other.starts_with("start/browse/") => {
                // A material's page, by id.
                self.tab = Tab::Browse;
                self.browser.go(Some(crate::assets::Place::Material(
                    other["start/browse/".len()..].to_owned(),
                )));
            }
            other => match other
                .strip_prefix("start/new/")
                .and_then(|n| n.parse().ok())
            {
                Some(category) => self.new_canvas = Some(category),
                None => return false,
            },
        }
        true
    }

    pub(crate) fn library(
        &mut self,
        kinds: &crate::materials::MaterialKinds,
        folder: Option<PathBuf>,
    ) -> &mut Library {
        if self
            .library
            .as_ref()
            .is_none_or(|(scanned, _)| *scanned != folder)
        {
            let library = Library::scan(kinds, folder.as_deref());
            self.library = Some((folder, library));
        }
        let (_, library) = self.library.as_mut().expect("found above");
        library
    }
}

impl SuePaint {
    /// Draws the start page over `rect`.
    pub(crate) fn start_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut start = std::mem::take(&mut self.start);
        let mut acts = Vec::new();
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .id_salt("sue-paint-start")
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect);
        child.multiply_opacity(self.cover.max(0.05));
        let live = self.ready;
        if !live {
            child.disable();
        }
        child.painter().rect_filled(rect, 0.0, theme.void);
        // The copies into the materials finished: find them again.
        let before = start.copies.len();
        start.copies.retain(|id| {
            cx.transfers
                .get(*id)
                .is_some_and(|transfer| matches!(transfer.state, TransferState::Running(_)))
        });
        if start.copies.len() != before {
            start.materials_changed();
            self.material_thumbs.clear();
        }

        // The rail.
        let rail_rect = Rect::from_min_size(rect.min, vec2(RAIL, rect.height()));
        let items = [
            RailItem {
                glyph: Glyph::Draw,
                label: tr!("start-draw"),
                round: true,
            },
            RailItem {
                glyph: Glyph::Projects,
                label: tr!("start-projects"),
                round: false,
            },
            RailItem {
                glyph: Glyph::Materials,
                label: tr!("start-manage-materials"),
                round: false,
            },
            RailItem {
                glyph: Glyph::Home,
                label: tr!("start-browse-assets"),
                round: false,
            },
        ];
        let selected = Some(match start.tab {
            Tab::Projects => 1,
            Tab::Materials => 2,
            Tab::Browse => 3,
        });
        match page::rail(&mut child, rail_rect, &items, selected, theme) {
            Some(0) => acts.push(Act::Draw),
            Some(1) => start.tab = Tab::Projects,
            Some(2) => start.tab = Tab::Materials,
            Some(3) => {
                // Browse Assets again goes back to its top.
                if start.tab == Tab::Browse {
                    start.browser.go(None);
                }
                start.tab = Tab::Browse;
            }
            _ => {}
        }

        // The top bar: the page's name, and the three buttons at the right.
        let main = Rect::from_min_max(pos2(rect.left() + RAIL, rect.top()), rect.max);
        let top = Rect::from_min_size(main.min, vec2(main.width(), 52.0));
        child.painter().rect_filled(top, 0.0, theme.panel);
        child
            .painter()
            .line_segment([top.left_bottom(), top.right_bottom()], theme.hairline());
        child.painter().text(
            pos2(top.left() + 20.0, top.center().y),
            egui::Align2::LEFT_CENTER,
            match start.tab {
                Tab::Browse => tr!("start-browse-assets"),
                Tab::Projects => tr!("start-projects"),
                Tab::Materials => tr!("start-manage-materials"),
            },
            theme.font(theme.heading_size),
            theme.text,
        );
        let running = cx.transfers.running();
        let buttons = [
            (
                Glyph::Transfers,
                tr!("start-data-transfers"),
                Popup::Transfers,
            ),
            (Glyph::Help, tr!("start-help"), Popup::Help),
            (Glyph::Settings, tr!("start-settings"), Popup::Settings),
        ];
        for (index, (glyph, hint, popup)) in buttons.into_iter().enumerate() {
            let right = top.right() - 14.0 - (2 - index) as f32 * 44.0;
            let button =
                Rect::from_min_size(pos2(right - 38.0, top.top() + 7.0), Vec2::splat(38.0));
            let on = start.popup.as_ref() == Some(&popup);
            if page::icon_button(&mut child, button, glyph, &hint, on, theme).clicked() {
                start.armed = false;
                start.popup = Some(popup.clone());
            }
            if popup == Popup::Transfers && running > 0 {
                child.painter().circle_filled(
                    button.right_top() + vec2(-7.0, 7.0),
                    4.0,
                    theme.text,
                );
            }
        }

        let body = Rect::from_min_max(pos2(main.left(), top.bottom()), main.max);
        match start.tab {
            Tab::Browse => self.browse_ui(&mut child, body, &mut start, cx, theme),
            Tab::Projects => self.projects_ui(&mut child, body, &mut start, &mut acts, cx, theme),
            Tab::Materials => self.materials_ui(&mut child, body, &mut start, &mut acts, cx, theme),
        }
        if start.thumbs.poll() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if live {
            self.popup_ui(ui.ctx(), &mut start, &mut acts, cx, theme);
        }
        self.start = start;
        for act in acts {
            self.start_act(act, cx);
        }
    }

    /// Browse Assets: CLIP STUDIO ASSETS' rows of today's top materials,
    /// laid out as Clip Studio's start page has them; See more and a
    /// material open in the same place, with back, forward and reload
    /// above them.
    pub(crate) fn browse_ui(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        start: &mut StartPage,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::assets::{self, Go, Place};
        self.assets.use_folder(
            cx.catalog
                .folders
                .get("cache")
                .map(|folder| folder.join("sue-paint-assets")),
        );
        let mut top = body.top();
        // The browser's bar, away from Home's top.
        if start.browser.place.is_some() || !start.browser.forward.is_empty() {
            let bar = Rect::from_min_size(body.min, vec2(body.width(), 34.0));
            ui.painter().rect_filled(bar, 0.0, theme.panel);
            let buttons = [
                (
                    Glyph::Back,
                    tr!("start-back"),
                    !start.browser.back.is_empty(),
                ),
                (
                    Glyph::Chevron,
                    tr!("start-forward"),
                    !start.browser.forward.is_empty(),
                ),
                (Glyph::Reload, tr!("start-reload"), true),
            ];
            for (index, (glyph, hint, enabled)) in buttons.into_iter().enumerate() {
                let rect = Rect::from_min_size(
                    pos2(bar.left() + 10.0 + index as f32 * 34.0, bar.top() + 3.0),
                    Vec2::splat(28.0),
                );
                if page::icon_button_enabled(ui, rect, glyph, &hint, false, enabled, theme)
                    .clicked()
                {
                    match index {
                        0 => start.browser.back(),
                        1 => start.browser.forward(),
                        _ => {
                            let url = match &start.browser.place {
                                None => assets::HOME.to_owned(),
                                Some(Place::List(url)) => url.clone(),
                                Some(Place::Material(id)) => assets::detail_url(id),
                            };
                            self.assets.reload(&url);
                        }
                    }
                }
            }
            top = bar.bottom();
        }
        let area = Rect::from_min_max(pos2(body.left(), top), body.max).shrink2(vec2(20.0, 8.0));
        let mut inner = ui.new_child(
            UiBuilder::new()
                .max_rect(area)
                .id_salt("sue-paint-start-home")
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        inner.set_clip_rect(area);
        let mut go = None;
        egui::ScrollArea::vertical()
            .id_salt((
                "sue-paint-start-home-scroll",
                format!("{:?}", start.browser.place),
            ))
            .auto_shrink([false, false])
            .show(&mut inner, |ui| match start.browser.place.clone() {
                None => {
                    go = assets::home_rows(ui, &mut self.assets, cx, theme);
                }
                Some(Place::List(url)) => {
                    go = assets::list_page(ui, &url, &mut self.assets, cx, theme);
                }
                Some(Place::Material(id)) => {
                    go = assets::material_page(ui, &id, &mut self.assets, cx, theme);
                }
            });
        match go {
            Some(Go::Place(place)) => start.browser.go(Some(place)),
            Some(Go::Site(link)) => assets::open_site(&link, cx),
            None => {}
        }
        if self.assets.poll() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub(crate) fn start_act(&mut self, act: Act, cx: &mut AppletContext<'_>) {
        match act {
            Act::Draw => {
                if !self.docs.is_empty() {
                    self.start.forced = false;
                    return;
                }
                // The last Paint project opened, else a new one.
                let last = items(cx.work)
                    .into_iter()
                    .filter(|item| {
                        item.kind != Type::Unsaved && sue_platform::fs::exists(&item.path)
                    })
                    .max_by_key(|item| item.when);
                match last {
                    Some(item) => self.start_act(Act::Open(item), cx),
                    None if self.start.full => self.start.new_canvas = Some(0),
                    None => {
                        self.dialog = Some(crate::dialogs::Dialog::New(Self::new_dialog(cx)));
                    }
                }
            }
            Act::Open(item) => {
                self.start.forced = false;
                match item.key {
                    ProjectKey::Kept(id) => cx.request(ShellRequest::Recover(id)),
                    ProjectKey::File(path) if !sue_platform::fs::exists(&path) => {
                        cx.fail(tr!("is-gone", path = path.display()));
                        cx.request(ShellRequest::Forget(path));
                    }
                    ProjectKey::File(path) if item.kind == Type::Picture => {
                        self.open_picked(cx, &path, false);
                    }
                    ProjectKey::File(path) => cx.request(ShellRequest::OpenFile(path)),
                }
            }
            Act::Paste(id) => {
                let folder = crate::materials::location(cx);
                let kinds = self.material_kinds.clone();
                let library = self.start.library(&kinds, folder);
                let Some(material) = library.material(id).cloned() else {
                    return;
                };
                let _ = library.used(id, unix_now());
                self.paste_material(cx, &material);
                self.start.forced = false;
            }
            Act::PickMaterial => {
                let pick = FilePick::open(Some(me()), "material-add", &tr!("start-add-material"))
                    .extensions(&["png"]);
                cx.request(ShellRequest::PickFile(pick));
            }
            Act::Materials => {
                self.start.tab = Tab::Materials;
                self.start.new_canvas = None;
            }
            Act::Template(id) => self.make_from_template(&id, cx),
            Act::Advanced(category) => self.advanced_new(category, cx),
            Act::ImportImage => self.pick_image(cx, false),
            Act::OpenFile => cx.request(ShellRequest::Command("file.open".to_owned())),
            Act::PickLocation => {
                let pick = FilePick::open(
                    Some(me()),
                    "materials-location",
                    &tr!("start-choose-location"),
                )
                .folder()
                .starting_in(crate::materials::location(cx));
                cx.request(ShellRequest::PickFile(pick));
            }
        }
    }

    /// What the system's dialog picked for the start page. Returns `true`
    /// if it was the start page's.
    pub(crate) fn start_picked(
        &mut self,
        purpose: &str,
        path: &Path,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        match purpose {
            "material-add" => {
                let Some(folder) = crate::materials::location(cx) else {
                    return true;
                };
                let kind = match &self.start.materials.node {
                    Node::Kind(kind) if kind == crate::materials::TIP => crate::materials::TIP,
                    _ => crate::materials::IMAGE,
                };
                let from = path.to_path_buf();
                let name = path
                    .file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
                let id = cx.transfers.spawn(
                    TransferKind::Copy,
                    &name,
                    Some(folder.clone()),
                    move |job| {
                        job.set_progress(0.2);
                        crate::materials::add_file(&folder, kind, &from).map(|_| ())
                    },
                );
                self.start.copies.push(id);
                true
            }
            "new-from-movie" => {
                self.new_from_movie(path, cx);
                true
            }
            "materials-location" => {
                let text = path.display().to_string();
                if let Err(error) = cx.edit_rice(sue_rice::SetRiceValue::new(
                    crate::materials::LOCATION_KEY,
                    sue_rice::RiceValue::Text(text),
                )) {
                    cx.fail(error.to_string());
                }
                self.start.materials_changed();
                self.material_thumbs.clear();
                true
            }
            _ => false,
        }
    }

    /// The projects: labels on the left; search, type chips, the toolbar
    /// and the cards on the right.
    pub(crate) fn projects_ui(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        start: &mut StartPage,
        acts: &mut Vec<Act>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let all = items(cx.work);
        let state = &mut start.projects;
        // A label just added is renamed once it shows.
        if let Some(name) = state.adding.take() {
            if cx.work.labels().contains(&name) {
                state.shelf = Shelf::Label(name.clone());
                state.renaming = Some((name.clone(), name));
            } else {
                state.adding = Some(name);
            }
        }
        if let Shelf::Label(label) = &state.shelf
            && !cx.work.labels().contains(label)
            && state.adding.is_none()
        {
            state.shelf = Shelf::All;
        }

        // The label column.
        let column = Rect::from_min_size(
            body.min + vec2(10.0, 10.0),
            vec2(COLUMN, body.height() - 20.0),
        );
        let mut col = ui.new_child(
            UiBuilder::new()
                .max_rect(column)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let label = match &state.shelf {
            Shelf::Label(label) => Some(label.clone()),
            _ => None,
        };
        col.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let size = Vec2::splat(28.0);
            let (add, _) = ui.allocate_exact_size(size, Sense::hover());
            if page::icon_button(ui, add, Glyph::Add, &tr!("start-add-label"), false, theme)
                .clicked()
            {
                let mut name = tr!("start-new-label");
                let mut n = 2;
                while cx.work.labels().contains(&name) {
                    name = format!("{} {n}", tr!("start-new-label"));
                    n += 1;
                }
                cx.request(ShellRequest::Labels(LabelEdit::Add(name.clone())));
                state.adding = Some(name);
            }
            let (delete, _) = ui.allocate_exact_size(size, Sense::hover());
            if page::icon_button_enabled(
                ui,
                delete,
                Glyph::Delete,
                &tr!("start-delete-label"),
                false,
                label.is_some(),
                theme,
            )
            .clicked()
                && let Some(label) = &label
            {
                cx.request(ShellRequest::Labels(LabelEdit::Remove(label.clone())));
                state.shelf = Shelf::All;
            }
            let (rename, _) = ui.allocate_exact_size(size, Sense::hover());
            if page::icon_button_enabled(
                ui,
                rename,
                Glyph::Rename,
                &tr!("start-rename-label"),
                false,
                label.is_some(),
                theme,
            )
            .clicked()
                && let Some(label) = &label
            {
                state.renaming = Some((label.clone(), label.clone()));
            }
        });
        col.add_space(6.0);
        col.spacing_mut().item_spacing.y = 3.0;
        let mut shelves = vec![
            (Shelf::All, Glyph::All, tr!("start-all")),
            (Shelf::Favorite, Glyph::Heart, tr!("start-favorite")),
        ];
        shelves.extend(
            cx.work
                .labels()
                .iter()
                .map(|label| (Shelf::Label(label.clone()), Glyph::Tag, label.clone())),
        );
        let width = column.width();
        for (shelf, glyph, name) in shelves {
            if let (Shelf::Label(label), Some((from, text))) = (&shelf, state.renaming.as_mut())
                && *label == *from
            {
                let response = col.add_sized(
                    vec2(width, 26.0),
                    egui::TextEdit::singleline(text).font(theme.font(theme.ui_size)),
                );
                response.request_focus();
                let enter = col.input(|input| input.key_pressed(egui::Key::Enter));
                let escape = col.input(|input| input.key_pressed(egui::Key::Escape));
                if enter || (response.lost_focus() && !escape) {
                    let to = text.trim().to_owned();
                    if !to.is_empty() && to != *from {
                        cx.request(ShellRequest::Labels(LabelEdit::Rename {
                            from: from.clone(),
                            to: to.clone(),
                        }));
                        state.shelf = Shelf::Label(to);
                    }
                    state.renaming = None;
                } else if escape {
                    state.renaming = None;
                }
                continue;
            }
            let row = Category {
                glyph: Some(glyph),
                label: name,
                depth: 0,
                open: None,
            };
            let (click, response) =
                page::category(&mut col, width, &row, state.shelf == shelf, theme);
            if click.is_some() {
                state.shelf = shelf.clone();
                state.chosen.clear();
                state.armed = false;
            }
            if let Shelf::Label(label) = &shelf {
                // Chosen projects drop onto a label with a right click.
                response.context_menu(|ui| {
                    if ui
                        .add_enabled(
                            !state.chosen.is_empty(),
                            egui::Button::new(tr!("start-move-chosen-here")),
                        )
                        .clicked()
                    {
                        for key in &state.chosen {
                            if let Some(item) = all.iter().find(|item| &item.key == key) {
                                cx.request(ShellRequest::Labels(LabelEdit::Mark {
                                    path: item.path.clone(),
                                    label: label.clone(),
                                    on: true,
                                }));
                            }
                        }
                        ui.close();
                    }
                    if ui.button(tr!("start-rename-label")).clicked() {
                        state.renaming = Some((label.clone(), label.clone()));
                        ui.close();
                    }
                    if ui.button(tr!("start-delete-label")).clicked() {
                        cx.request(ShellRequest::Labels(LabelEdit::Remove(label.clone())));
                        ui.close();
                    }
                });
            }
        }

        // Search, chips and the toolbar.
        let right = Rect::from_min_max(
            pos2(column.right() + 12.0, body.top() + 10.0),
            pos2(body.right() - 12.0, body.bottom()),
        );
        let mut area = ui.new_child(
            UiBuilder::new()
                .max_rect(right)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        area.set_clip_rect(right.intersect(ui.clip_rect()));
        page::search_box(
            &mut area,
            &mut state.search,
            &tr!("start-search-projects"),
            right.width(),
            theme,
        );
        area.add_space(8.0);
        area.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("start-type"))
                    .color(theme.muted)
                    .size(theme.small_size + 1.0),
            );
            ui.add_space(6.0);
            for kind in Type::ALL {
                let on = state.types.contains(&(kind as u8));
                if page::chip(ui, &kind.label(), on, theme).clicked() {
                    if on {
                        state.types.remove(&(kind as u8));
                    } else {
                        state.types.insert(kind as u8);
                    }
                }
            }
        });
        area.add_space(8.0);
        let shown = shown(&all, state, cx.work);
        state
            .chosen
            .retain(|key| all.iter().any(|item| &item.key == key));
        let chosen: Vec<Item> = shown
            .iter()
            .filter(|item| state.chosen.contains(&item.key))
            .cloned()
            .collect();
        toolbar_frame(&mut area, right.width(), theme, |ui| {
            let mark = match chosen.len() {
                0 => Chosen::None,
                n if n == shown.len() => Chosen::All,
                _ => Chosen::Some,
            };
            if page::check(ui, &tr!("start-select-all"), mark, theme).clicked() {
                if mark == Chosen::All {
                    state.chosen.clear();
                } else {
                    state
                        .chosen
                        .extend(shown.iter().map(|item| item.key.clone()));
                }
            }
            ui.add_space(8.0);
            let some = !chosen.is_empty();
            let move_to = page::action(ui, Glyph::Move, &tr!("start-move-to"), some, false, theme);
            egui::Popup::menu(&move_to).show(|ui| {
                for label in cx.work.labels() {
                    if ui.button(label).clicked() {
                        for item in &chosen {
                            cx.request(ShellRequest::Labels(LabelEdit::Mark {
                                path: item.path.clone(),
                                label: label.clone(),
                                on: true,
                            }));
                        }
                    }
                }
                if ui.button(tr!("start-favorite")).clicked() {
                    for item in &chosen {
                        cx.request(ShellRequest::Labels(LabelEdit::Favorite {
                            path: item.path.clone(),
                            on: true,
                        }));
                    }
                }
            });
            if let Shelf::Label(label) = &state.shelf
                && page::action(
                    ui,
                    Glyph::Tag,
                    &tr!("start-take-off-label"),
                    some,
                    false,
                    theme,
                )
                .clicked()
            {
                for item in &chosen {
                    cx.request(ShellRequest::Labels(LabelEdit::Mark {
                        path: item.path.clone(),
                        label: label.clone(),
                        on: false,
                    }));
                }
            }
            let text = if state.armed {
                tr!("start-really-remove")
            } else {
                tr!("start-remove-from-list")
            };
            let remove = page::action(ui, Glyph::Delete, &text, some, state.armed, theme)
                .on_hover_text(tr!("start-remove-hint"));
            if remove.clicked() {
                if state.armed {
                    for item in &chosen {
                        forget(item, cx);
                    }
                    state.chosen.clear();
                    state.armed = false;
                } else {
                    state.armed = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                view_buttons(ui, &mut state.view, false, theme);
                ui.add_space(6.0);
                sort_buttons(
                    ui,
                    "sue-paint-start-project-sort",
                    &mut state.sort,
                    &mut state.reversed,
                    &[tr!("start-date-modified"), tr!("start-name")],
                    theme,
                );
            });
        });
        area.add_space(10.0);

        // The cards.
        let now = unix_now();
        if shown.is_empty() {
            let message = if all.is_empty() {
                tr!("start-no-projects")
            } else {
                tr!("start-none-match")
            };
            area.add_space(40.0);
            area.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(message)
                        .color(theme.muted)
                        .size(theme.ui_size),
                );
            });
            return;
        }
        let mut clicks = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-start-projects")
            .auto_shrink([false, false])
            .show(&mut area, |ui| {
                let width = ui.available_width();
                let cards: Vec<Card> = shown
                    .iter()
                    .map(|item| {
                        let picture = start.thumbs.get(&item.path).and_then(|(at, thumb)| {
                            let key = texture_key(&item.path.display().to_string());
                            cx.textures
                                .texture(key, at.max(1), thumb.size, 4, &thumb.pixels, false)
                                .map(|texture| (texture, thumb.size))
                        });
                        Card {
                            title: item.title.clone(),
                            detail: format!("{}  {}", item.kind.label(), ago(item.when, now)),
                            picture,
                            placeholder: item.kind.glyph(),
                            favorite: Some(item.favorite),
                            chosen: start.projects.chosen.contains(&item.key),
                            hover: item.path.display().to_string(),
                        }
                    })
                    .collect();
                lay_out(
                    ui,
                    width,
                    start.projects.view,
                    &cards,
                    theme,
                    |index, click, response| {
                        if let Some(click) = click {
                            clicks.push((index, click));
                        }
                        let item = &shown[index];
                        response.context_menu(|ui| {
                            if ui.button(tr!("start-open")).clicked() {
                                clicks.push((index, CardClick::Open));
                                ui.close();
                            }
                            if ui.button(tr!("start-show-in-folder")).clicked() {
                                cx.request(ShellRequest::Command(format!(
                                    "reveal-file:{}",
                                    item.path.display()
                                )));
                                ui.close();
                            }
                            if ui.button(tr!("start-details")).clicked() {
                                clicks.push((index, CardClick::Details));
                                ui.close();
                            }
                        });
                    },
                );
            });
        let state = &mut start.projects;
        for (index, click) in clicks {
            let Some(item) = shown.get(index) else {
                continue;
            };
            match click {
                CardClick::Open => acts.push(Act::Open(item.clone())),
                CardClick::Choose => {
                    if !state.chosen.remove(&item.key) {
                        state.chosen.insert(item.key.clone());
                    }
                    state.armed = false;
                }
                CardClick::Favorite => cx.request(ShellRequest::Labels(LabelEdit::Favorite {
                    path: item.path.clone(),
                    on: !item.favorite,
                })),
                CardClick::Details => {
                    start.armed = false;
                    start.popup = Some(Popup::Project(item.key.clone()));
                }
            }
        }
    }

    /// The materials: the tree on the left; search, origin chips, the
    /// toolbar and the cards on the right.
    pub(crate) fn materials_ui(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        start: &mut StartPage,
        acts: &mut Vec<Act>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let folder = crate::materials::location(cx);
        let kinds = self.material_kinds.clone();
        let entries = start.library(&kinds, folder.clone()).entries();
        let state = &mut start.materials;

        // The tree.
        let column = Rect::from_min_size(
            body.min + vec2(10.0, 10.0),
            vec2(COLUMN, body.height() - 20.0),
        );
        let mut col = ui.new_child(
            UiBuilder::new()
                .max_rect(column)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let removable: Vec<EntityId> = entries
            .iter()
            .filter(|entry| state.chosen.contains(&entry.id) && entry.material.removable())
            .map(|entry| entry.id)
            .collect();
        col.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let size = Vec2::splat(28.0);
            let (add, _) = ui.allocate_exact_size(size, Sense::hover());
            if page::icon_button(
                ui,
                add,
                Glyph::Add,
                &tr!("start-add-material"),
                false,
                theme,
            )
            .clicked()
            {
                acts.push(Act::PickMaterial);
            }
            let (delete, _) = ui.allocate_exact_size(size, Sense::hover());
            let hint = if state.armed {
                tr!("start-really-delete-materials")
            } else {
                tr!("start-delete-materials")
            };
            if page::icon_button_enabled(
                ui,
                delete,
                Glyph::Delete,
                &hint,
                state.armed,
                !removable.is_empty(),
                theme,
            )
            .clicked()
            {
                if state.armed {
                    delete_materials(&entries, &removable, cx);
                    state.chosen.clear();
                    state.armed = false;
                    start_rescan(&mut self.material_thumbs);
                } else {
                    state.armed = true;
                }
            }
            let (reveal, _) = ui.allocate_exact_size(size, Sense::hover());
            if page::icon_button(
                ui,
                reveal,
                Glyph::Folder,
                &tr!("start-show-materials-folder"),
                false,
                theme,
            )
            .clicked()
                && let Some(folder) = &folder
            {
                cx.request(ShellRequest::Command(format!(
                    "reveal:{}",
                    folder.display()
                )));
            }
        });
        col.add_space(6.0);
        col.spacing_mut().item_spacing.y = 3.0;
        let width = column.width();
        let mut nodes = vec![(
            Node::All,
            Category {
                glyph: Some(Glyph::All),
                label: tr!("start-all-materials"),
                depth: 0,
                open: Some(state.open),
            },
        )];
        if state.open {
            nodes.extend(kinds.iter().map(|kind| {
                (
                    Node::Kind(kind.id.clone()),
                    Category {
                        glyph: Some(kind.glyph),
                        label: crate::localize(&kind.name),
                        depth: 1,
                        open: None,
                    },
                )
            }));
        }
        nodes.push((
            Node::Favorite,
            Category {
                glyph: Some(Glyph::Heart),
                label: tr!("start-favorite"),
                depth: 0,
                open: None,
            },
        ));
        nodes.push((
            Node::Recent,
            Category {
                glyph: Some(Glyph::Clock),
                label: tr!("start-recently-used"),
                depth: 0,
                open: None,
            },
        ));
        for (node, row) in nodes {
            let (click, _) = page::category(&mut col, width, &row, state.node == node, theme);
            match click {
                Some(CategoryClick::Fold) => state.open = !state.open,
                Some(CategoryClick::Choose) => {
                    state.node = node;
                    state.chosen.clear();
                    state.armed = false;
                }
                None => {}
            }
        }

        // Search, chips and the toolbar.
        let right = Rect::from_min_max(
            pos2(column.right() + 12.0, body.top() + 10.0),
            pos2(body.right() - 12.0, body.bottom()),
        );
        let mut area = ui.new_child(
            UiBuilder::new()
                .max_rect(right)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        area.set_clip_rect(right.intersect(ui.clip_rect()));
        page::search_box(
            &mut area,
            &mut state.search,
            &tr!("start-search-materials"),
            right.width(),
            theme,
        );
        area.add_space(8.0);
        area.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("start-type"))
                    .color(theme.muted)
                    .size(theme.small_size + 1.0),
            );
            ui.add_space(6.0);
            for (index, label) in [tr!("start-sue-paints-own"), tr!("start-added-by-you")]
                .iter()
                .enumerate()
            {
                let index = index as u8;
                let on = state.origins.contains(&index);
                if page::chip(ui, label, on, theme).clicked() {
                    if on {
                        state.origins.remove(&index);
                    } else {
                        state.origins.insert(index);
                    }
                }
            }
        });
        area.add_space(8.0);
        let needle = state.search.trim().to_lowercase();
        let mut shown: Vec<crate::materials::Entry> = entries
            .iter()
            .filter(|entry| match &state.node {
                Node::All => true,
                Node::Kind(kind) => entry.material.kind == *kind,
                Node::Favorite => entry.favorite,
                Node::Recent => entry.used > 0,
            })
            .filter(|entry| {
                state.origins.is_empty()
                    || state
                        .origins
                        .contains(&u8::from(entry.material.removable()))
            })
            .filter(|entry| {
                needle.is_empty()
                    || crate::localize(&entry.material.name)
                        .to_lowercase()
                        .contains(&needle)
            })
            .cloned()
            .collect();
        let sort = if state.node == Node::Recent {
            2
        } else {
            state.sort
        };
        match sort {
            1 => shown
                .sort_by_cached_key(|entry| crate::localize(&entry.material.name).to_lowercase()),
            2 => shown.sort_by_key(|entry| std::cmp::Reverse(entry.used)),
            _ => shown.sort_by_key(|entry| std::cmp::Reverse(entry.added)),
        }
        if state.reversed {
            shown.reverse();
        }
        state
            .chosen
            .retain(|id| entries.iter().any(|entry| entry.id == *id));
        let picked = shown
            .iter()
            .filter(|entry| state.chosen.contains(&entry.id))
            .count();
        let canvas = !self.docs.is_empty();
        let mut favorite_all = None;
        toolbar_frame(&mut area, right.width(), theme, |ui| {
            let mark = match picked {
                0 => Chosen::None,
                n if n == shown.len() => Chosen::All,
                _ => Chosen::Some,
            };
            if page::check(ui, &tr!("start-select-all"), mark, theme).clicked() {
                if mark == Chosen::All {
                    state.chosen.clear();
                } else {
                    state.chosen.extend(shown.iter().map(|entry| entry.id));
                }
            }
            ui.add_space(8.0);
            if page::action(
                ui,
                Glyph::Heart,
                &tr!("start-favorite"),
                picked > 0,
                false,
                theme,
            )
            .clicked()
            {
                favorite_all = Some(true);
            }
            if page::action(
                ui,
                Glyph::Add,
                &tr!("start-add-material"),
                true,
                false,
                theme,
            )
            .clicked()
            {
                acts.push(Act::PickMaterial);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                view_buttons(ui, &mut state.view, true, theme);
                ui.add_space(6.0);
                sort_buttons(
                    ui,
                    "sue-paint-start-material-sort",
                    &mut state.sort,
                    &mut state.reversed,
                    &[
                        tr!("start-date-added"),
                        tr!("start-name"),
                        tr!("start-last-used"),
                    ],
                    theme,
                );
            });
        });
        if let Some(on) = favorite_all {
            let chosen: Vec<EntityId> = start.materials.chosen.iter().copied().collect();
            let library = start.library(&kinds, folder.clone());
            for id in &chosen {
                if let Err(error) = library.set_favorite(*id, on) {
                    cx.fail(error);
                }
            }
        }
        let state = &mut start.materials;
        area.add_space(10.0);
        if shown.is_empty() {
            area.add_space(40.0);
            area.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(tr!("start-no-materials"))
                        .color(theme.muted)
                        .size(theme.ui_size),
                );
            });
            return;
        }
        let mut clicks = Vec::new();
        let view = state.view;
        let cards: Vec<Card> = shown
            .iter()
            .map(|entry| {
                let side = 128_u32;
                let key = texture_key(&entry.material.key());
                let thumb = self
                    .material_thumbs
                    .entry(key)
                    .or_insert_with(|| crate::panels::material_thumb(&entry.material, side));
                let picture = cx
                    .textures
                    .texture(key, 1, [side, side], 4, thumb, false)
                    .map(|texture| (texture, [side, side]));
                let kind = kinds
                    .get(&entry.material.kind)
                    .map(|kind| crate::localize(&kind.name))
                    .unwrap_or_default();
                Card {
                    title: crate::localize(&entry.material.name),
                    detail: kind,
                    picture,
                    placeholder: Glyph::Materials,
                    favorite: Some(entry.favorite),
                    chosen: state.chosen.contains(&entry.id),
                    hover: if canvas {
                        tr!("start-click-to-put-on-canvas")
                    } else {
                        tr!("start-open-canvas-for-materials")
                    },
                }
            })
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-start-materials")
            .auto_shrink([false, false])
            .show(&mut area, |ui| {
                let width = ui.available_width();
                lay_out(ui, width, view, &cards, theme, |index, click, response| {
                    if let Some(click) = click {
                        clicks.push((index, click));
                    }
                    response.context_menu(|ui| {
                        if ui
                            .add_enabled(canvas, egui::Button::new(tr!("paste-to-canvas")))
                            .clicked()
                        {
                            clicks.push((index, CardClick::Open));
                            ui.close();
                        }
                        if ui.button(tr!("start-details")).clicked() {
                            clicks.push((index, CardClick::Details));
                            ui.close();
                        }
                    });
                });
            });
        for (index, click) in clicks {
            let Some(entry) = shown.get(index) else {
                continue;
            };
            match click {
                CardClick::Open if canvas => acts.push(Act::Paste(entry.id)),
                CardClick::Open | CardClick::Choose => {
                    let state = &mut start.materials;
                    if !state.chosen.remove(&entry.id) {
                        state.chosen.insert(entry.id);
                    }
                    state.armed = false;
                }
                CardClick::Favorite => {
                    let library = start.library(&kinds, folder.clone());
                    if let Err(error) = library.set_favorite(entry.id, !entry.favorite) {
                        cx.fail(error);
                    }
                }
                CardClick::Details => {
                    start.armed = false;
                    start.popup = Some(Popup::Material(entry.id));
                }
            }
        }
    }

    /// The popup over the start page, if one is open.
    pub(crate) fn popup_ui(
        &mut self,
        ctx: &egui::Context,
        start: &mut StartPage,
        acts: &mut Vec<Act>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(popup) = start.popup.clone() else {
            return;
        };
        let full = start.full;
        let outcome = match &popup {
            Popup::Transfers => transfers_popup(full, ctx, cx, theme),
            Popup::Help => show(
                full,
                ctx,
                "help",
                &tr!("start-help"),
                vec2(460.0, 330.0),
                theme,
                |ui| {
                    let mut rows = vec![(
                        Row::new(
                            Some(Glyph::Book),
                            tr!("start-basic-tips"),
                            Trailing::External,
                        ),
                        SITE,
                    )];
                    // Phones keep Help short.
                    if !full {
                        rows.push((
                            Row::new(
                                Some(Glyph::Projects),
                                tr!("start-using-sue-paint"),
                                Trailing::External,
                            ),
                            GUIDE,
                        ));
                    }
                    link_group(ui, &rows, cx, theme);
                    page::gap(ui);
                    let rows = [
                        (
                            Row::new(Some(Glyph::Help), tr!("start-faq"), Trailing::External),
                            FAQ,
                        ),
                        (
                            Row::new(Some(Glyph::Mail), tr!("start-contact"), Trailing::External),
                            CONTACT,
                        ),
                        (
                            Row::new(
                                Some(Glyph::Bubble),
                                tr!("start-feedback"),
                                Trailing::External,
                            ),
                            FEEDBACK,
                        ),
                    ];
                    link_group(ui, &rows, cx, theme);
                },
            ),
            Popup::Settings => {
                let dark = cx.theme.void.r() < 128;
                let location = crate::materials::location(cx);
                let custom = !cx
                    .rice()
                    .get_str(crate::materials::LOCATION_KEY)
                    .unwrap_or("")
                    .trim()
                    .is_empty();
                let mut next = None;
                let outcome = show(
                    full,
                    ctx,
                    "settings",
                    &tr!("start-settings"),
                    vec2(460.0, 470.0),
                    theme,
                    |ui| {
                        let rows = [Row::new(
                            Some(Glyph::Moon),
                            tr!("start-dark-mode"),
                            Trailing::Switch(dark),
                        )];
                        if page::group(ui, &rows, theme).is_some() {
                            let preset = if dark { "paper" } else { "gray" };
                            if let Err(error) = cx.edit_rice(sue_rice::SetRiceValue::new(
                                "sue_paint.theme",
                                sue_rice::RiceValue::Text(preset.to_owned()),
                            )) {
                                cx.fail(error.to_string());
                            }
                        }
                        page::gap(ui);
                        if full {
                            // On phones Settings also leads to the transfers
                            // and the materials.
                            let rows = [Row::new(
                                Some(Glyph::Transfers),
                                tr!("start-data-transfers"),
                                Trailing::More,
                            )];
                            if page::group(ui, &rows, theme).is_some() {
                                next = Some(Popup::Transfers);
                            }
                            page::gap(ui);
                            let rows = [Row::new(
                                Some(Glyph::Materials),
                                tr!("start-manage-materials"),
                                Trailing::More,
                            )];
                            if page::group(ui, &rows, theme).is_some() {
                                acts.push(Act::Materials);
                                next = Some(Popup::Settings);
                            }
                            page::gap(ui);
                        }
                        let shown = location.as_ref().map_or_else(
                            || tr!("start-nowhere"),
                            |folder| folder.display().to_string(),
                        );
                        let mut rows = vec![
                            Row::new(
                                Some(Glyph::Folder),
                                tr!("start-location-of-materials"),
                                Trailing::External,
                            )
                            .detail(shown),
                            Row::new(None, tr!("start-choose-location"), Trailing::More),
                        ];
                        let mut back =
                            Row::new(None, tr!("start-use-sue-paints-folder"), Trailing::None);
                        back.enabled = custom;
                        rows.push(back);
                        match page::group(ui, &rows, theme) {
                            Some(0) => {
                                if let Some(folder) = &location {
                                    let _ = sue_platform::fs::create_dir_all(folder);
                                    cx.request(ShellRequest::Command(format!(
                                        "reveal:{}",
                                        folder.display()
                                    )));
                                }
                            }
                            Some(1) => acts.push(Act::PickLocation),
                            Some(2) => {
                                if let Err(error) = cx.edit_rice(sue_rice::SetRiceValue::new(
                                    crate::materials::LOCATION_KEY,
                                    sue_rice::RiceValue::Text(String::new()),
                                )) {
                                    cx.fail(error.to_string());
                                }
                                start.library = None;
                                self.material_thumbs.clear();
                            }
                            _ => {}
                        }
                        page::gap(ui);
                        let rows = [
                            Row::new(Some(Glyph::Draw), tr!("start-pen-pressure"), Trailing::More),
                            Row::new(
                                Some(Glyph::Wrench),
                                tr!("start-maintenance-menu"),
                                Trailing::More,
                            ),
                            Row::new(
                                Some(Glyph::Settings),
                                tr!("start-all-settings"),
                                Trailing::More,
                            ),
                        ];
                        match page::group(ui, &rows, theme) {
                            Some(0) => {
                                // Fitting the pen's pressure to the hand.
                                cx.request(ShellRequest::AppletCommand {
                                    applet: sue_wm::AppletId::new(crate::studio::ID)
                                        .expect("a valid id"),
                                    command: "file.pressure".to_owned(),
                                });
                                next = None;
                            }
                            Some(1) => next = Some(Popup::Maintenance),
                            Some(2) => {
                                cx.request(ShellRequest::OpenSettings(Some("studios".to_owned())));
                                next = Some(Popup::Settings);
                            }
                            _ => {}
                        }
                        page::gap(ui);
                        let rows = [Row::new(
                            Some(Glyph::Draw),
                            tr!("applet-name"),
                            Trailing::Text(tr!(
                                "start-version",
                                version = env!("CARGO_PKG_VERSION")
                            )),
                        )];
                        page::group(ui, &rows, theme);
                    },
                );
                match next {
                    // Settings itself closes it (Sue's Settings or the
                    // materials come up instead).
                    Some(Popup::Settings) => PopupOutcome::Closed,
                    // The Maintenance Menu or the transfers replace it.
                    Some(other) => {
                        start.popup = Some(other);
                        return;
                    }
                    None => outcome,
                }
            }
            Popup::Maintenance => {
                let mut done = None;
                let outcome = show(
                    full,
                    ctx,
                    "maintenance",
                    &tr!("start-maintenance-menu"),
                    vec2(460.0, 330.0),
                    theme,
                    |ui| {
                        let rows = [
                            Row::new(None, tr!("start-rebuild-thumbnails"), Trailing::None)
                                .detail(tr!("start-rebuild-thumbnails-hint")),
                            Row::new(None, tr!("start-find-materials"), Trailing::None)
                                .detail(tr!("start-find-materials-hint")),
                            Row::new(None, tr!("start-forget-missing"), Trailing::None)
                                .detail(tr!("start-forget-missing-hint")),
                            Row::new(None, tr!("start-clear-completed"), Trailing::None),
                        ];
                        done = page::group(ui, &rows, theme);
                    },
                );
                match done {
                    Some(0) => {
                        start.thumbs.clear();
                        self.material_thumbs.clear();
                        cx.notify(tr!("start-thumbnails-rebuilt"));
                    }
                    Some(1) => {
                        start.library = None;
                        self.material_thumbs.clear();
                        cx.notify(tr!("start-materials-found"));
                    }
                    Some(2) => {
                        let gone: Vec<Item> = items(cx.work)
                            .into_iter()
                            .filter(|item| {
                                item.kind != Type::Unsaved && !sue_platform::fs::exists(&item.path)
                            })
                            .collect();
                        for item in &gone {
                            forget(item, cx);
                        }
                        cx.notify(tr!("start-forgot", count = gone.len()));
                    }
                    Some(3) => cx.transfers.clear_completed(),
                    _ => {}
                }
                if outcome == PopupOutcome::Closed {
                    // Back to Settings, where the menu was.
                    start.popup = Some(Popup::Settings);
                    return;
                }
                outcome
            }
            Popup::Project(key) => {
                let all = items(cx.work);
                let Some(item) = all.into_iter().find(|item| &item.key == key) else {
                    start.popup = None;
                    return;
                };
                self.project_popup(ctx, start, acts, &item, cx, theme)
            }
            Popup::Material(id) => self.material_popup(ctx, start, acts, *id, cx, theme),
        };
        if outcome == PopupOutcome::Closed && start.popup.as_ref() == Some(&popup) {
            start.popup = None;
            start.armed = false;
        }
    }

    /// What a project is, with what can be done to it.
    fn project_popup(
        &mut self,
        ctx: &egui::Context,
        start: &mut StartPage,
        acts: &mut Vec<Act>,
        item: &Item,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> PopupOutcome {
        let now = unix_now();
        let full = start.full;
        let mut close = false;
        let picture = start.thumbs.get(&item.path).and_then(|(at, thumb)| {
            cx.textures
                .texture(
                    texture_key(&item.path.display().to_string()),
                    at.max(1),
                    thumb.size,
                    4,
                    &thumb.pixels,
                    false,
                )
                .map(|texture| (texture, thumb.size))
        });
        let armed = start.armed;
        let mut arm = None;
        let outcome = show(
            full,
            ctx,
            "project",
            &item.title,
            vec2(480.0, 620.0),
            theme,
            |ui| {
                preview(ui, picture, item.kind.glyph(), theme);
                page::gap(ui);
                let rows = [
                    Row::new(Some(Glyph::Draw), tr!("start-open"), Trailing::More),
                    Row::new(
                        Some(Glyph::Folder),
                        tr!("start-show-in-folder"),
                        Trailing::External,
                    ),
                ];
                match page::group(ui, &rows, theme) {
                    Some(0) => {
                        acts.push(Act::Open(item.clone()));
                        close = true;
                    }
                    Some(1) => cx.request(ShellRequest::Command(format!(
                        "reveal-file:{}",
                        item.path.display()
                    ))),
                    _ => {}
                }
                page::gap(ui);
                let mut rows = vec![Row::new(
                    Some(Glyph::Heart),
                    tr!("start-favorite"),
                    Trailing::Switch(item.favorite),
                )];
                let labels = cx.work.labels().to_vec();
                rows.extend(labels.iter().map(|label| {
                    Row::new(
                        Some(Glyph::Tag),
                        label.clone(),
                        Trailing::Switch(cx.work.has_label(&item.path, label)),
                    )
                }));
                match page::group(ui, &rows, theme) {
                    Some(0) => cx.request(ShellRequest::Labels(LabelEdit::Favorite {
                        path: item.path.clone(),
                        on: !item.favorite,
                    })),
                    Some(index) => {
                        if let Some(label) = labels.get(index - 1) {
                            cx.request(ShellRequest::Labels(LabelEdit::Mark {
                                path: item.path.clone(),
                                label: label.clone(),
                                on: !cx.work.has_label(&item.path, label),
                            }));
                        }
                    }
                    None => {}
                }
                page::gap(ui);
                let rows = [
                    Row::new(None, tr!("start-kind"), Trailing::Text(item.kind.label())),
                    Row::new(
                        None,
                        tr!("start-last-worked-on"),
                        Trailing::Text(ago(item.when, now)),
                    ),
                    Row::new(None, tr!("start-where"), Trailing::None)
                        .detail(item.path.display().to_string()),
                ];
                page::group(ui, &rows, theme);
                page::gap(ui);
                let text = match (item.kind, armed) {
                    (Type::Unsaved, false) => tr!("start-discard"),
                    (Type::Unsaved, true) => tr!("start-really-discard"),
                    (_, false) => tr!("start-remove-from-list"),
                    (_, true) => tr!("start-really-remove"),
                };
                let rows = [Row::new(Some(Glyph::Delete), text, Trailing::None)
                    .detail(tr!("start-remove-hint"))];
                if page::group(ui, &rows, theme).is_some() {
                    if armed {
                        forget(item, cx);
                        close = true;
                    } else {
                        arm = Some(true);
                    }
                }
            },
        );
        if let Some(on) = arm {
            start.armed = on;
        }
        if close { PopupOutcome::Closed } else { outcome }
    }

    /// What a material is, with what can be done to it.
    fn material_popup(
        &mut self,
        ctx: &egui::Context,
        start: &mut StartPage,
        acts: &mut Vec<Act>,
        id: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> PopupOutcome {
        let folder = crate::materials::location(cx);
        let kinds = self.material_kinds.clone();
        let Some(entry) = start
            .library(&kinds, folder.clone())
            .entries()
            .into_iter()
            .find(|entry| entry.id == id)
        else {
            start.popup = None;
            return PopupOutcome::Closed;
        };
        let side = 128_u32;
        let key = texture_key(&entry.material.key());
        let thumb = self
            .material_thumbs
            .entry(key)
            .or_insert_with(|| crate::panels::material_thumb(&entry.material, side));
        let picture = cx
            .textures
            .texture(key, 1, [side, side], 4, thumb, false)
            .map(|texture| (texture, [side, side]));
        let canvas = !self.docs.is_empty();
        let full = start.full;
        let armed = start.armed;
        let mut close = false;
        let mut favorite = None;
        let mut arm = false;
        let kind = kinds
            .get(&entry.material.kind)
            .map(|kind| crate::localize(&kind.name))
            .unwrap_or_default();
        let title = crate::localize(&entry.material.name);
        let outcome = show(
            full,
            ctx,
            "material",
            &title,
            vec2(460.0, 600.0),
            theme,
            |ui| {
                preview(ui, picture, Glyph::Materials, theme);
                page::gap(ui);
                let mut put = Row::new(Some(Glyph::Draw), tr!("paste-to-canvas"), Trailing::More);
                if !canvas {
                    put = put.detail(tr!("start-open-canvas-for-materials"));
                    put.enabled = false;
                }
                let rows = [
                    put,
                    Row::new(
                        Some(Glyph::Heart),
                        tr!("start-favorite"),
                        Trailing::Switch(entry.favorite),
                    ),
                ];
                match page::group(ui, &rows, theme) {
                    Some(0) => {
                        acts.push(Act::Paste(id));
                        close = true;
                    }
                    Some(1) => favorite = Some(!entry.favorite),
                    _ => {}
                }
                page::gap(ui);
                let used = if entry.used > 0 {
                    ago(u64::try_from(entry.used).unwrap_or(0), unix_now())
                } else {
                    tr!("start-never")
                };
                let mut rows = vec![
                    Row::new(None, tr!("start-kind"), Trailing::Text(kind.clone())),
                    Row::new(None, tr!("start-last-used"), Trailing::Text(used)),
                ];
                if let crate::materials::Source::File(path) = &entry.material.source {
                    rows.push(
                        Row::new(
                            Some(Glyph::Folder),
                            tr!("start-show-in-folder"),
                            Trailing::External,
                        )
                        .detail(path.display().to_string()),
                    );
                } else {
                    rows.push(Row::new(
                        None,
                        tr!("start-where"),
                        Trailing::Text(tr!("start-sue-paints-own")),
                    ));
                }
                if page::group(ui, &rows, theme) == Some(2)
                    && let crate::materials::Source::File(path) = &entry.material.source
                {
                    cx.request(ShellRequest::Command(format!(
                        "reveal-file:{}",
                        path.display()
                    )));
                }
                if entry.material.removable() {
                    page::gap(ui);
                    let text = if armed {
                        tr!("start-really-delete-materials")
                    } else {
                        tr!("start-delete-materials")
                    };
                    if page::group(
                        ui,
                        &[Row::new(Some(Glyph::Delete), text, Trailing::None)],
                        theme,
                    )
                    .is_some()
                    {
                        if armed {
                            delete_materials(std::slice::from_ref(&entry), &[id], cx);
                            start_rescan(&mut self.material_thumbs);
                            close = true;
                        } else {
                            arm = true;
                        }
                    }
                }
            },
        );
        if let Some(on) = favorite
            && let Err(error) = start.library(&kinds, folder).set_favorite(id, on)
        {
            cx.fail(error);
        }
        if arm {
            start.armed = true;
        }
        if close {
            start.library = None;
            PopupOutcome::Closed
        } else {
            outcome
        }
    }
}

/// Sue Paint's applet id.
fn me() -> AppletId {
    AppletId::new(crate::ID).expect("a valid id")
}

/// Forgets a project in the work Sue remembers (files stay on disk), or
/// throws away kept work.
pub(crate) fn forget(item: &Item, cx: &mut AppletContext<'_>) {
    match &item.key {
        ProjectKey::File(path) => cx.request(ShellRequest::Forget(path.clone())),
        ProjectKey::Kept(id) => cx.request(ShellRequest::Discard(id.clone())),
    }
}

/// Deletes the material files of `ids` (only ones people added).
fn delete_materials(
    entries: &[crate::materials::Entry],
    ids: &[EntityId],
    cx: &mut AppletContext<'_>,
) {
    for entry in entries.iter().filter(|entry| ids.contains(&entry.id)) {
        if let crate::materials::Source::File(path) = &entry.material.source
            && let Err(error) = sue_platform::fs::remove_file(path)
        {
            cx.fail(tr!("cannot-delete-the-material", error = error));
        }
    }
}

/// Forgets the material thumbnails after files changed.
fn start_rescan(thumbs: &mut HashMap<u64, Vec<u8>>) {
    thumbs.clear();
}

/// A toolbar's band, `width` wide.
fn toolbar_frame(ui: &mut Ui, width: f32, theme: &Theme, add: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 36.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme.panel);
    let mut band = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(8.0, 5.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    band.spacing_mut().item_spacing.x = 4.0;
    add(&mut band);
}

/// The view toggles (`small` adds small cards), right to left.
fn view_buttons(ui: &mut Ui, view: &mut View, small: bool, theme: &Theme) {
    let mut views = vec![(View::List, Glyph::List, tr!("start-view-list"))];
    if small {
        views.push((View::Small, Glyph::GridSmall, tr!("start-view-small")));
    }
    views.push((View::Large, Glyph::Grid, tr!("start-view-large")));
    for (which, glyph, hint) in views {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
        if page::icon_button(ui, rect, glyph, &hint, *view == which, theme).clicked() {
            *view = which;
        }
    }
}

/// The sort dropdown and its direction, right to left.
fn sort_buttons(
    ui: &mut Ui,
    id: &str,
    sort: &mut usize,
    reversed: &mut bool,
    options: &[String],
    theme: &Theme,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
    let glyph = if *reversed {
        Glyph::SortUp
    } else {
        Glyph::SortDown
    };
    if page::icon_button(ui, rect, glyph, &tr!("start-sort-direction"), false, theme).clicked() {
        *reversed = !*reversed;
    }
    page::dropdown(ui, Id::unique(id), sort, options, 150.0, theme);
}

/// Lays `cards` out as `view` says in `width`, calling `each` with every
/// card's click, if any, and its response (for a context menu).
fn lay_out(
    ui: &mut Ui,
    width: f32,
    view: View,
    cards: &[Card],
    theme: &Theme,
    mut each: impl FnMut(usize, Option<CardClick>, &egui::Response),
) {
    let mut report = |index: usize, (click, response): (Option<CardClick>, egui::Response)| {
        each(index, click, &response);
    };
    match view {
        View::List => {
            ui.spacing_mut().item_spacing.y = 4.0;
            for (index, card) in cards.iter().enumerate() {
                let shown = page::list_row(ui, width - 4.0, card, theme);
                report(index, shown);
            }
        }
        View::Large | View::Small => {
            let size = if view == View::Large {
                vec2(150.0, 196.0)
            } else {
                vec2(100.0, 122.0)
            };
            let gap = 10.0;
            let columns = ((width + gap) / (size.x + gap)).floor().max(1.0) as usize;
            for (row, chunk) in cards.chunks(columns).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (column, card) in chunk.iter().enumerate() {
                        let shown = page::card(ui, size, card, theme);
                        report(row * columns + column, shown);
                    }
                });
                ui.add_space(gap);
            }
        }
    }
}

/// A popup in the middle of the page, or a sheet over the whole screen
/// (`full`, on phones and tablets).
fn show(
    full: bool,
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: Vec2,
    theme: &Theme,
    body: impl FnOnce(&mut Ui),
) -> PopupOutcome {
    if full {
        page::sheet(ctx, id, title, theme, body)
    } else {
        page::popup(ctx, id, title, size, theme, body)
    }
}

/// A large picture at the top of a popup.
fn preview(
    ui: &mut Ui,
    picture: Option<(egui::TextureId, [u32; 2])>,
    placeholder: Glyph,
    theme: &Theme,
) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 180.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme.void);
    match picture {
        Some((texture, [w, h])) if w > 0 && h > 0 => {
            let inner = rect.shrink(8.0);
            let scale = (inner.width() / w as f32).min(inner.height() / h as f32);
            painter.image(
                texture,
                Rect::from_center_size(inner.center(), vec2(w as f32 * scale, h as f32 * scale)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        _ => page::glyph(
            painter,
            Rect::from_center_size(rect.center(), Vec2::splat(56.0)),
            placeholder,
            theme.muted,
        ),
    }
    painter.rect_stroke(rect, 0.0, theme.hairline(), egui::StrokeKind::Inside);
}

/// A group of rows that open web pages.
fn link_group(ui: &mut Ui, rows: &[(Row, &str)], cx: &mut AppletContext<'_>, theme: &Theme) {
    let shown: Vec<Row> = rows.iter().map(|(row, _)| row.clone()).collect();
    if let Some(index) = page::group(ui, &shown, theme)
        && let Some((_, link)) = rows.get(index)
    {
        cx.request(ShellRequest::Command(format!("open-url:{link}")));
    }
}

/// The Data Transfers popup: what Sue brought in, wrote out and copied in
/// this run, still going or done.
fn transfers_popup(
    full: bool,
    ctx: &egui::Context,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> PopupOutcome {
    let now = unix_now();
    show(
        full,
        ctx,
        "transfers",
        &tr!("start-data-transfers"),
        vec2(760.0, 560.0),
        theme,
        |ui| {
            let finished = cx
                .transfers
                .list()
                .iter()
                .filter(|transfer| transfer.finished())
                .count();
            ui.horizontal(|ui| {
                if page::action(
                    ui,
                    Glyph::Delete,
                    &tr!("start-clear-completed"),
                    finished > 0,
                    false,
                    theme,
                )
                .clicked()
                {
                    cx.transfers.clear_completed();
                }
            });
            ui.add_space(12.0);
            if cx.transfers.list().is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("start-no-transfers"))
                            .color(theme.muted)
                            .size(theme.ui_size),
                    );
                });
                return;
            }
            let width = ui.available_width();
            let mut cancel = None;
            for transfer in cx.transfers.list() {
                let (rect, _) = ui.allocate_exact_size(vec2(width, 46.0), Sense::hover());
                let painter = ui.painter();
                painter.rect(
                    rect,
                    0.0,
                    theme.pill,
                    theme.hairline(),
                    egui::StrokeKind::Inside,
                );
                let glyph = match transfer.kind {
                    TransferKind::Import => Glyph::Folder,
                    TransferKind::Export => Glyph::External,
                    TransferKind::Copy => Glyph::Transfers,
                };
                page::glyph(
                    painter,
                    Rect::from_center_size(
                        pos2(rect.left() + 20.0, rect.center().y),
                        Vec2::splat(18.0),
                    ),
                    glyph,
                    theme.text,
                );
                painter.with_clip_rect(rect.shrink(1.0)).text(
                    pos2(rect.left() + 40.0, rect.top() + 15.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{}  {}", transfer.kind.label(), transfer.label),
                    theme.font(theme.ui_size),
                    theme.text,
                );
                let place = transfer
                    .path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                painter
                    .with_clip_rect(Rect::from_min_max(
                        rect.min,
                        pos2(rect.right() - 200.0, rect.bottom()),
                    ))
                    .text(
                        pos2(rect.left() + 40.0, rect.top() + 32.0),
                        egui::Align2::LEFT_CENTER,
                        place,
                        theme.font(theme.small_size),
                        theme.muted,
                    );
                let state = match &transfer.state {
                    TransferState::Running(_) => tr!("start-transfer-running"),
                    TransferState::Done => {
                        tr!("start-transfer-done", when = ago(transfer.started, now))
                    }
                    TransferState::Failed(why) => tr!("start-transfer-failed", why = why.clone()),
                    TransferState::Cancelled => tr!("start-transfer-cancelled"),
                };
                painter.text(
                    pos2(rect.right() - 14.0, rect.top() + 15.0),
                    egui::Align2::RIGHT_CENTER,
                    state,
                    theme.font(theme.small_size),
                    theme.muted,
                );
                if let TransferState::Running(fraction) = transfer.state {
                    let mut bar = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                        pos2(rect.right() - 190.0, rect.top() + 29.0),
                        vec2(140.0, 8.0),
                    )));
                    page::progress(&mut bar, 140.0, fraction, theme);
                    let stop = Rect::from_center_size(
                        pos2(rect.right() - 24.0, rect.top() + 32.0),
                        Vec2::splat(18.0),
                    );
                    if page::icon_button(ui, stop, Glyph::Close, &tr!("start-cancel"), false, theme)
                        .clicked()
                    {
                        cancel = Some(transfer.id);
                    }
                }
                ui.add_space(4.0);
            }
            if let Some(id) = cancel {
                cx.transfers.cancel(id);
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sue-paint-start-{tag}-{}", std::process::id()));
        let _ = sue_platform::fs::remove_dir_all(&dir);
        sue_platform::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn projects_come_from_the_shared_work_and_sort_and_filter() {
        let root = temp("items");
        let paint = AppletId::new(crate::ID).unwrap();
        let writer = AppletId::new("sue-writer").unwrap();
        let cat = root.join("cat.png");
        let comic = root.join("Comic.sue");
        let story = root.join("story.sue");
        let notes = root.join("notes.txt");
        for file in [&cat, &comic, &story, &notes] {
            sue_platform::fs::write(file, b"x").unwrap();
        }
        let mut work = sue_wm::WorkList::open(root.join("work"));
        work.remember(&cat, false, Some(paint.clone()), 100)
            .unwrap();
        work.remember(&comic, true, Some(paint.clone()), 300)
            .unwrap();
        work.remember(&story, true, Some(writer), 400).unwrap();
        work.remember(&notes, false, None, 500).unwrap();
        work.keep(b"SUE", None, "Sketch", Some(paint), 200).unwrap();
        work.start_over();
        work.edit_labels(&LabelEdit::Favorite {
            path: cat.clone(),
            on: true,
        })
        .unwrap();
        let label = work.labels()[0].clone();
        work.edit_labels(&LabelEdit::Mark {
            path: comic.clone(),
            label: label.clone(),
            on: true,
        })
        .unwrap();

        // A Writer project and a text file are not Sue Paint's.
        let all = items(&work);
        let titles = |shown: &[Item]| {
            shown
                .iter()
                .map(|item| item.title.clone())
                .collect::<Vec<_>>()
        };
        let mut state = ProjectsState::default();
        assert_eq!(
            titles(&shown(&all, &state, &work)),
            ["Comic", "Sketch", "cat"]
        );
        state.sort = 1;
        assert_eq!(
            titles(&shown(&all, &state, &work)),
            ["cat", "Comic", "Sketch"]
        );
        state.reversed = true;
        assert_eq!(
            titles(&shown(&all, &state, &work)),
            ["Sketch", "Comic", "cat"]
        );
        state.reversed = false;
        state.shelf = Shelf::Favorite;
        assert_eq!(titles(&shown(&all, &state, &work)), ["cat"]);
        state.shelf = Shelf::Label(label);
        assert_eq!(titles(&shown(&all, &state, &work)), ["Comic"]);
        state.shelf = Shelf::All;
        state.types.insert(Type::Unsaved as u8);
        assert_eq!(titles(&shown(&all, &state, &work)), ["Sketch"]);
        state.types.clear();
        state.search = "CO".to_owned();
        assert_eq!(titles(&shown(&all, &state, &work)), ["Comic"]);
        let _ = sue_platform::fs::remove_dir_all(&root);
    }

    #[test]
    fn thumbnails_shrink_to_fit_over_white() {
        let mut pixels = Vec::new();
        for _ in 0..(400 * 200) {
            pixels.extend([255, 0, 0, 0]);
        }
        let tile =
            sue_core::RasterTile2D::from_bytes(400, 200, sue_core::PixelFormat::Rgba8, pixels)
                .unwrap();
        let thumb = shrink(&tile, 100);
        assert_eq!(thumb.size, [100, 50]);
        // Clear pixels show the paper.
        assert_eq!(&thumb.pixels[..4], &[255, 255, 255, 255]);
        // A real picture file comes back small in the background.
        let root = temp("thumb");
        let png = sue_io::encode_png_rgba(64, 32, &vec![0; 64 * 32 * 4]).unwrap();
        let path = root.join("small.png");
        sue_platform::fs::write(&path, png).unwrap();
        let mut thumbs = Thumbs::default();
        assert!(thumbs.get(&path).is_none());
        let start = std::time::Instant::now();
        while thumbs.poll() {
            assert!(start.elapsed().as_secs() < 20);
            std::thread::yield_now();
        }
        assert_eq!(thumbs.get(&path).unwrap().1.size, [64, 32]);
        let _ = sue_platform::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_start_page_opens_its_pages_and_popups_by_name() {
        let mut start = StartPage::default();
        assert!(start.shows(true) && !start.shows(false));
        start.forced = true;
        assert!(start.shows(false));
        assert!(start.navigate("start/materials"));
        assert_eq!(start.tab, Tab::Materials);
        for (target, popup) in [
            ("start/transfers", Popup::Transfers),
            ("start/help", Popup::Help),
            ("start/settings", Popup::Settings),
        ] {
            assert!(start.navigate(target));
            assert_eq!(start.popup, Some(popup));
        }
        assert!(start.navigate("start/close"));
        assert_eq!(start.popup, None);
        assert!(!start.navigate("start/nowhere"));
    }
}
