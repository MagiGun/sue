//! Sue Paint's start page, shown on the desktop while no canvas is open
//! (and from File, Start Page): a rail with Draw, Projects and Manage
//! Materials on the left, Data Transfers, Help and Settings at the top
//! right, and the projects or the materials in the main area. Everything
//! on it is on this device; nothing here goes online.
//!
//! The projects are the work Sue remembers, shared with the Hub and every
//! studio: the pictures and Sue Paint projects opened and saved lately and
//! the work kept while not saved, sorted with the shared labels. The
//! materials are the entities of the material [`Library`], whose kinds
//! register in a [`crate::materials::MaterialKinds`] registry.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use sue_core::{EntityId, Properties};
use sue_wm::files::browser::{
    COLUMN, lay_out, preview, show, sort_buttons, toolbar_frame, view_buttons,
};
use sue_wm::files::thumbs::shrink;
use sue_wm::files::{Studio, Thumb, Thumbnailer, transfers_popup};
use sue_wm::start::{
    self as page, Card, CardClick, Category, CategoryClick, Chosen, Glyph, PopupOutcome, RailItem,
    Row, Trailing,
};
use sue_wm::{
    AppletContext, AppletId, FilePick, ShellRequest, Theme, TransferKind, TransferState, ago,
    unix_now,
};

// The files browser moved to `sue_wm::files`; the start page draws it
// from there until the Hub's Files place replaces the start page.
pub(crate) use sue_wm::files::{
    Item, ProjectKey, ProjectsState, Shelf, Thumbs, Type, View, forget, shown, texture_key,
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

/// What the main area shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    /// The projects on this device.
    #[default]
    Projects,
    /// The materials.
    Materials,
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

/// Sue Paint as the files browser lists it: its pictures, its projects
/// and the work it kept.
fn studio() -> Studio<'static> {
    Studio {
        id: me(),
        name: tr!("applet-name"),
        opens: Some(&crate::studio::opens),
    }
}

/// The projects of Sue Paint in the work Sue remembers: pictures, Sue
/// Paint projects and its kept work.
pub(crate) fn items(work: &sue_wm::WorkList) -> Vec<Item> {
    sue_wm::files::items(work, &[studio()])
}

/// How Sue Paint makes a project card's picture in the background.
pub(crate) fn thumbnailer() -> Thumbnailer {
    Thumbnailer::new(thumbnail)
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

/// A picture file or a Sue project (whose bytes are `bytes`) made at most
/// `side` pixels on its long edge: the picture, or the project's first
/// picture flattened.
fn thumbnail(path: &Path, bytes: &[u8], side: u32, job: &sue_core::JobContext) -> Option<Thumb> {
    job.set_progress(0.3);
    let raster = if sue_core::SueFile::sniff(bytes) {
        let arena = sue_core::SueFile::from_bytes(bytes)
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
    } else if sue_io::clip::sniff(bytes) {
        let (arena, picture) = crate::clip_io::picture_arena(bytes)?;
        crate::composite::flatten(
            &arena,
            picture,
            crate::composite::RenderOptions {
                paper: true,
                drafts: false,
            },
        )?
    } else if bytes.starts_with(b"8BPS") {
        let psd = sue_io::psd::decode_psd(bytes).ok()?;
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
            .import(bytes, &file, &Properties::new())
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

/// Everything the start page keeps between frames.
#[derive(Debug)]
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
    /// The cache was looked through for what older versions left in it.
    cache_checked: bool,
}

impl Default for StartPage {
    fn default() -> Self {
        Self {
            forced: false,
            tab: Tab::default(),
            popup: None,
            projects: ProjectsState::default(),
            materials: MaterialsState::default(),
            library: None,
            thumbs: Thumbs::new(Some(thumbnailer())),
            copies: Vec::new(),
            armed: false,
            full: false,
            new_canvas: None,
            screen: [0, 0],
            cache_checked: false,
        }
    }
}

/// The folder in Sue's cache where versions before 0.0.8 kept pages and
/// pictures read from another company's site. Nothing reads it any more.
const OLD_CACHE: &str = "sue-paint-assets";

/// Deletes [`OLD_CACHE`] from the cache folder `cache`, with everything
/// in it. Returns `true` when there was one and it is gone. Nothing else
/// in the cache is touched.
fn forget_old_cache(cache: &Path) -> bool {
    let old = cache.join(OLD_CACHE);
    sue_platform::fs::is_dir(&old) && sue_platform::fs::remove_dir_all(&old).is_ok()
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

    /// Once in a run, deletes what versions before 0.0.8 kept in the
    /// cache folder from another company's site.
    pub(crate) fn forget_old_cache(&mut self, cache: Option<&Path>) {
        if std::mem::replace(&mut self.cache_checked, true) {
            return;
        }
        if let Some(cache) = cache {
            forget_old_cache(cache);
        }
    }

    /// The materials changed on disk; find them again.
    pub(crate) fn materials_changed(&mut self) {
        self.library = None;
    }

    /// Opens a page or popup by name (`start/projects`, `start/help` and
    /// so on), for scripts and snapshots.
    pub(crate) fn navigate(&mut self, target: &str) -> bool {
        match target {
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
        ];
        let selected = Some(match start.tab {
            Tab::Projects => 1,
            Tab::Materials => 2,
        });
        match page::rail(&mut child, rail_rect, &items, selected, theme) {
            Some(0) => acts.push(Act::Draw),
            Some(1) => start.tab = Tab::Projects,
            Some(2) => start.tab = Tab::Materials,
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
    /// and the cards on the right (the shared files browser).
    pub(crate) fn projects_ui(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        start: &mut StartPage,
        acts: &mut Vec<Act>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let studios = [studio()];
        let events = sue_wm::files::projects_ui(
            ui,
            body,
            &mut start.projects,
            &mut start.thumbs,
            &studios,
            cx,
            theme,
        );
        for event in events {
            match event {
                sue_wm::files::Event::Open(item) => acts.push(Act::Open(item)),
                sue_wm::files::Event::Details(key) => {
                    start.armed = false;
                    start.popup = Some(Popup::Project(key));
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
        let studios = [studio()];
        let (outcome, open) = sue_wm::files::project_popup(
            ctx,
            start.full,
            item,
            &mut start.thumbs,
            &mut start.armed,
            &studios,
            cx,
            theme,
        );
        if open {
            acts.push(Act::Open(item.clone()));
        }
        outcome
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

/// A group of rows that open web pages.
fn link_group(ui: &mut Ui, rows: &[(Row, &str)], cx: &mut AppletContext<'_>, theme: &Theme) {
    let shown: Vec<Row> = rows.iter().map(|(row, _)| row.clone()).collect();
    if let Some(index) = page::group(ui, &shown, theme)
        && let Some((_, link)) = rows.get(index)
    {
        cx.request(ShellRequest::Command(format!("open-url:{link}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::ReversibleTransaction;

    fn temp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sue-paint-start-{tag}-{}", std::process::id()));
        let _ = sue_platform::fs::remove_dir_all(&dir);
        sue_platform::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_start_page_lists_only_sue_paints_work() {
        let root = temp("items");
        let paint = me();
        let writer = AppletId::new("sue-writer").unwrap();
        let cat = root.join("cat.png");
        let comic = root.join("Comic.sue");
        let story = root.join("story.sue");
        let notes = root.join("notes.txt");
        for file in [&cat, &comic, &story, &notes] {
            sue_platform::fs::write(file, b"x").unwrap();
        }
        let mut work = sue_wm::WorkList::open(root.join("work"));
        // A picture is Sue Paint's wherever it was opened last.
        work.remember(&cat, false, None, 100).unwrap();
        work.remember(&comic, true, Some(paint.clone()), 300)
            .unwrap();
        work.remember(&story, true, Some(writer.clone()), 400)
            .unwrap();
        work.remember(&notes, false, Some(writer.clone()), 500)
            .unwrap();
        work.keep(b"SUE", None, "Sketch", Some(paint.clone()), 200)
            .unwrap();
        work.start_over();
        work.keep(b"SUE", None, "Draft", Some(writer), 250).unwrap();
        work.start_over();

        let all = items(&work);
        assert_eq!(work.unsaved().count(), 2);
        let mut titles: Vec<(String, Type)> = all
            .iter()
            .map(|item| (item.title.clone(), item.kind))
            .collect();
        titles.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            titles,
            [
                ("Comic".to_owned(), Type::Project),
                ("Sketch".to_owned(), Type::Unsaved),
                ("cat".to_owned(), Type::Picture),
            ]
        );
        assert!(all.iter().all(|item| item.studio.as_ref() == Some(&paint)));
        let _ = sue_platform::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_start_page_draws_a_project_without_a_preview_of_its_own() {
        // A Sue Paint project holds no THMB preview yet, so its card's
        // picture comes from Sue Paint's thumbnailer.
        let root = temp("thumb");
        let mut arena = sue_core::Arena::new();
        let (mut edit, picture) = crate::doc::create(&crate::doc::NewPicture {
            name: "Card".to_owned(),
            width: 40,
            height: 20,
            ..crate::doc::NewPicture::default()
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        assert!(arena.get_by_id(picture).is_ok());
        let mut file = sue_core::SueFile::new();
        file.set_arena(&arena);
        let project = root.join("Card.sue");
        sue_platform::fs::write(&project, file.to_bytes()).unwrap();

        let mut start = StartPage::default();
        assert!(start.thumbs.get(&project).is_none());
        let begun = std::time::Instant::now();
        while start.thumbs.poll() {
            assert!(begun.elapsed().as_secs() < 20);
            std::thread::yield_now();
        }
        let (_, thumb) = start.thumbs.get(&project).expect("Sue Paint's picture");
        assert_eq!(thumb.size, [40, 20]);
        assert_eq!(&thumb.pixels[..4], &[255, 255, 255, 255], "white paper");
        // Without Sue Paint's thumbnailer the shared browser has none.
        let mut plain = Thumbs::default();
        assert!(plain.get(&project).is_none());
        while plain.poll() {
            std::thread::yield_now();
        }
        assert!(plain.get(&project).is_none());
        let _ = sue_platform::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_old_cache_of_another_site_is_deleted_once_and_nothing_else() {
        let cache = temp("old-cache");
        let old = cache.join(OLD_CACHE);
        sue_platform::fs::create_dir_all(old.join("deeper")).unwrap();
        sue_platform::fs::write(old.join("page"), b"<html>").unwrap();
        sue_platform::fs::write(old.join("deeper").join("picture"), b"png").unwrap();
        // What else lives in the cache stays: other folders, and a file
        // whose name only starts the same way.
        let models = cache.join("models");
        sue_platform::fs::create_dir_all(&models).unwrap();
        sue_platform::fs::write(models.join("pose"), b"model").unwrap();
        let near = cache.join("sue-paint-assets.txt");
        sue_platform::fs::write(&near, b"mine").unwrap();

        let mut start = StartPage::default();
        // No cache folder known: nothing happens, and it is not tried again.
        start.forget_old_cache(None);
        assert!(sue_platform::fs::is_dir(&old));
        start.forget_old_cache(Some(&cache));
        assert!(sue_platform::fs::is_dir(&old), "one look in a run");

        let mut start = StartPage::default();
        start.forget_old_cache(Some(&cache));
        assert!(!sue_platform::fs::exists(&old));
        assert_eq!(
            sue_platform::fs::read(models.join("pose")).unwrap(),
            b"model"
        );
        assert_eq!(sue_platform::fs::read(&near).unwrap(), b"mine");
        // Gone already: asking again is no error and removes nothing.
        assert!(!forget_old_cache(&cache));
        assert!(sue_platform::fs::is_dir(&models));
        // A cache folder that does not exist is no error either.
        assert!(!forget_old_cache(&cache.join("missing")));
        let _ = sue_platform::fs::remove_dir_all(&cache);
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
        // The page that read another site is gone and must not come
        // back unnoticed.
        assert!(!start.navigate("start/browse"));
        assert!(!start.navigate("start/browse/1"));
        assert_eq!(start.tab, Tab::Materials);
    }
}
