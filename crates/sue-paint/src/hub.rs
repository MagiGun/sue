//! Sue Paint in the Hub: its Materials place, and the pictures of its
//! files on the Hub's cards.
//!
//! The Materials place is the material [`Library`] (whose kinds register
//! in a [`crate::materials::MaterialKinds`] registry) as a place of the
//! Hub's rail, beside Home and Files, drawn by `SueApplet::hub_ui`: the
//! kinds on the left (a row of chips on a narrow screen), search, the
//! origin chips, the toolbar and the cards, a popup with what a material
//! is, and the place's menu (find the materials again, draw their
//! pictures again, and where they are kept). A click on a material while
//! a canvas is open puts it on the canvas and goes there.
//!
//! It replaced the Manage Materials page of Sue Paint's own launcher,
//! which Sue 0.0.8 removed (owner note 11): Sue Paint opens on its canvas,
//! its files are in the Hub's Files place, and File, Back to the Hub goes
//! there. Everything here is on this device; nothing goes online.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::{Align, Layout, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use sue_core::{EntityId, Properties};
use sue_wm::files::browser::{COLUMN, preview, show, sort_buttons, toolbar_frame, view_buttons};
use sue_wm::files::thumbs::shrink;
use sue_wm::files::{Thumb, Thumbnailer, View, texture_key};
use sue_wm::icons::Icon;
use sue_wm::start::{
    self as page, CardClick, Category, CategoryClick, Chosen, Glyph, PopupOutcome, Row, Trailing,
};
use sue_wm::ui::{Density, Leading, ListRow, RowAction, Space, TextRole, Thumbnail, ThumbnailCard};
use sue_wm::{
    AppletContext, AppletId, FilePick, HubPlace, ItemState, MenuBar, MenuItem, ShellRequest, Theme,
    TransferKind, TransferState, ago, unix_now,
};

use crate::materials::Library;
use crate::studio::SuePaint;

/// The Materials place's id, as [`ShellRequest::HubPlace`] names it.
pub const PLACE: &str = "materials";

/// Under this width the place is laid out for a phone: the kinds become
/// a row of chips and popups fill the screen (the Hub's own breakpoint).
const NARROW: f32 = 560.0;

/// The place's menu: its commands.
const FIND: &str = "materials:find";
const REDRAW: &str = "materials:thumbnails";
const REVEAL: &str = "materials:reveal";
const LOCATION: &str = "materials:location";
const OWN: &str = "materials:own-folder";

/// The places Sue Paint adds to the Hub.
pub(crate) fn places() -> Vec<HubPlace> {
    vec![HubPlace::new(
        PLACE,
        tr!("materials-place"),
        Icon::Materials,
    )]
}

/// How Sue Paint makes a card's picture of one of its files in the
/// background, for the Hub.
pub(crate) fn thumbnailer() -> Thumbnailer {
    Thumbnailer::new(thumbnail)
}

/// A picture file or a Sue project (whose bytes are `bytes`) made at most
/// `side` pixels on its long edge: the project's first picture flattened
/// (a `.sue` or `.clip` file), a `.psd`'s composite, or the picture.
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
    } else if let Ok(tile) = sue_io::picture_ops::read_picture(path, bytes) {
        // The picture formats Sue reads by itself, as the Hub does without
        // a studio's help: no registry, no copy into an arena.
        tile
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

/// What the browser of materials shows.
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
    /// Delete was clicked once in the toolbar.
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

/// Everything the Materials place keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct MaterialsPlace {
    pub(crate) materials: MaterialsState,
    /// The material library, found when first shown, with the folder it
    /// was found in.
    library: Option<(Option<PathBuf>, Library)>,
    /// Material files being copied in, by transfer id.
    copies: Vec<u64>,
    /// The material whose popup shows.
    details: Option<EntityId>,
    /// Delete was clicked once in the popup.
    armed: bool,
    /// The place's menu.
    menu: MenuBar,
    /// A material clicked while a canvas is open: Sue Paint puts it on
    /// the canvas once it shows, with the canvas's own history.
    pub(crate) paste: Option<EntityId>,
    /// The cache was looked through for what older versions left in it.
    cache_checked: bool,
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

/// What the place asks the studio for, after drawing.
enum Act {
    /// Put the material on the canvas.
    Paste(EntityId),
    /// Ask for a picture to add to the materials.
    PickMaterial,
    /// Run an entry of the place's menu.
    Menu(String),
}

impl MaterialsPlace {
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

    /// Opens what scripts and snapshots name: `materials/menu` (the
    /// place's menu), `materials/close` (the menu and the popup closed).
    pub(crate) fn navigate(&mut self, target: &str) -> bool {
        match target {
            "materials/menu" => self.menu.open_dropdown(),
            "materials/close" => {
                self.menu.close();
                self.details = None;
                self.armed = false;
            }
            _ => return false,
        }
        true
    }

    /// The library in `folder`, found again when the folder changed or
    /// the files did.
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

/// The place's menu: finding and redrawing, then where the materials are.
fn menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action(tr!("materials-find-again"), FIND),
        MenuItem::action(tr!("materials-rebuild-thumbnails"), REDRAW),
        MenuItem::Separator,
        MenuItem::action(tr!("materials-show-folder"), REVEAL),
        MenuItem::action(tr!("materials-choose-location"), LOCATION),
        MenuItem::action(tr!("materials-use-own-folder"), OWN),
    ]
}

impl SuePaint {
    /// Draws the Hub place `place` in `ui`, which covers the Hub's pane.
    pub(crate) fn place_ui(&mut self, place: &str, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        if place == PLACE {
            self.materials_place_ui(ui, cx);
        }
    }

    /// Puts the material the Materials place asked for on the canvas, once
    /// Sue Paint shows.
    pub(crate) fn paste_from_hub(&mut self, cx: &mut AppletContext<'_>) {
        let Some(id) = self.hub.paste.take() else {
            return;
        };
        let folder = crate::materials::location(cx);
        let kinds = self.material_kinds.clone();
        let library = self.hub.library(&kinds, folder);
        let Some(material) = library.material(id).cloned() else {
            return;
        };
        let _ = library.used(id, unix_now());
        self.paste_material(cx, &material);
    }

    /// The Materials place: its title and menu, the browser under them,
    /// and the popup of a material over them.
    fn materials_place_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let pane = ui.max_rect();
        let narrow = pane.width() < NARROW;
        let full = narrow || cx.view.form_factor != sue_platform::FormFactor::Desktop;
        let margin = theme.space(if narrow { Space::S6 } else { Space::S9 });
        let mut hub = std::mem::take(&mut self.hub);
        let mut acts = Vec::new();
        // The copies into the materials finished: find them again.
        let before = hub.copies.len();
        hub.copies.retain(|id| {
            cx.transfers
                .get(*id)
                .is_some_and(|transfer| matches!(transfer.state, TransferState::Running(_)))
        });
        if hub.copies.len() != before {
            hub.materials_changed();
            self.material_thumbs.clear();
        }

        // The title row: the place's name, and its menu at the right.
        let row = Rect::from_min_size(
            pane.min + vec2(margin, margin * if narrow { 1.0 } else { 0.6 }),
            vec2(pane.width() - margin * 2.0, Density::of(ui).icon_button()),
        );
        ui.painter().text(
            row.left_center(),
            egui::Align2::LEFT_CENTER,
            tr!("materials-place"),
            theme.text_font(TextRole::Display),
            theme.text,
        );
        let mut tools = ui.new_child(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let more = sue_wm::ui::icon_button(
            "sue-paint-materials-menu",
            Icon::More,
            &tr!("materials-menu"),
        )
        .show(&mut tools, &theme);
        if narrow
            && sue_wm::ui::icon_button(
                "sue-paint-materials-add",
                Icon::Plus,
                &tr!("start-add-material"),
            )
            .show(&mut tools, &theme)
            .clicked()
        {
            acts.push(Act::PickMaterial);
        }
        let custom = !cx
            .rice()
            .get_str(crate::materials::LOCATION_KEY)
            .unwrap_or("")
            .trim()
            .is_empty();
        let state = |command: &str| {
            if command == OWN && !custom {
                ItemState::off()
            } else {
                ItemState::on()
            }
        };
        if let Some(command) =
            hub.menu
                .dropdown(ui, more.rect, more.clicked(), &menu_items(), &state, &theme)
        {
            acts.push(Act::Menu(command));
        }

        // The browser under it.
        let inset = if narrow {
            margin
        } else {
            margin - theme.space(Space::S5)
        };
        let body = Rect::from_min_max(
            pos2(pane.left() + inset, row.bottom() + theme.space(Space::S4)),
            pos2(pane.right() - inset, pane.bottom()),
        );
        let mut body_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(body)
                .layout(Layout::top_down(Align::Min)),
        );
        body_ui.set_clip_rect(body.intersect(ui.clip_rect()));
        self.materials_ui(&mut body_ui, body, narrow, &mut hub, &mut acts, cx, &theme);
        if let Some(id) = hub.details {
            self.material_popup(ui.ctx(), full, &mut hub, &mut acts, id, cx, &theme);
        }
        self.hub = hub;
        for act in acts {
            self.place_act(act, cx);
        }
    }

    /// Does what the Materials place asked for.
    fn place_act(&mut self, act: Act, cx: &mut AppletContext<'_>) {
        match act {
            Act::Paste(id) => {
                // The canvas's own history takes the paste, so it waits
                // until Sue Paint shows.
                self.hub.paste = Some(id);
                cx.request(ShellRequest::Open(me()));
            }
            Act::PickMaterial => {
                let pick = FilePick::open(Some(me()), "material-add", &tr!("start-add-material"))
                    .extensions(&["png"]);
                cx.request(ShellRequest::PickFile(pick));
            }
            Act::Menu(command) => self.materials_menu(&command, cx),
        }
    }

    /// Runs an entry of the Materials place's menu.
    fn materials_menu(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            FIND => {
                self.hub.materials_changed();
                self.material_thumbs.clear();
                cx.notify(tr!("start-materials-found"));
            }
            REDRAW => {
                self.material_thumbs.clear();
                cx.notify(tr!("materials-thumbnails-redrawn"));
            }
            REVEAL => {
                if let Some(folder) = crate::materials::location(cx) {
                    let _ = sue_platform::fs::create_dir_all(&folder);
                    cx.request(ShellRequest::Command(format!(
                        "reveal:{}",
                        folder.display()
                    )));
                }
            }
            LOCATION => {
                let pick = FilePick::open(
                    Some(me()),
                    "materials-location",
                    &tr!("start-choose-location"),
                )
                .folder()
                .starting_in(crate::materials::location(cx));
                cx.request(ShellRequest::PickFile(pick));
            }
            OWN => {
                if let Err(error) = cx.edit_rice(sue_rice::SetRiceValue::new(
                    crate::materials::LOCATION_KEY,
                    sue_rice::RiceValue::Text(String::new()),
                )) {
                    cx.fail(error.to_string());
                }
                self.hub.materials_changed();
                self.material_thumbs.clear();
            }
            _ => {}
        }
    }

    /// What the system's dialog picked for the Materials place. Returns
    /// `true` if it was the place's.
    pub(crate) fn place_picked(
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
                let kind = match &self.hub.materials.node {
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
                self.hub.copies.push(id);
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
                self.hub.materials_changed();
                self.material_thumbs.clear();
                true
            }
            _ => false,
        }
    }

    /// The materials: the tree on the left (chips on a narrow screen);
    /// search, origin chips, the toolbar and the cards on the right.
    #[allow(clippy::too_many_arguments, reason = "the place and what it reads")]
    fn materials_ui(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        narrow: bool,
        hub: &mut MaterialsPlace,
        acts: &mut Vec<Act>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let folder = crate::materials::location(cx);
        let kinds = self.material_kinds.clone();
        let entries = hub.library(&kinds, folder.clone()).entries();
        let state = &mut hub.materials;
        let removable: Vec<EntityId> = entries
            .iter()
            .filter(|entry| state.chosen.contains(&entry.id) && entry.material.removable())
            .map(|entry| entry.id)
            .collect();

        // The tree, with Add, Delete and the folder over it, inset as the
        // Files place insets its labels so the two places line up.
        let inset = theme.space(Space::S5);
        let right = if narrow {
            body
        } else {
            let column = Rect::from_min_size(
                body.min + Vec2::splat(inset),
                vec2(COLUMN, body.height() - inset * 2.0),
            );
            let mut col = ui.new_child(
                UiBuilder::new()
                    .max_rect(column)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
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
                        self.material_thumbs.clear();
                        hub.library = None;
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
                {
                    acts.push(Act::Menu(REVEAL.to_owned()));
                }
            });
            col.add_space(6.0);
            col.spacing_mut().item_spacing.y = 3.0;
            let width = column.width();
            for (node, row) in tree(&kinds, state.open) {
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
            Rect::from_min_max(
                pos2(column.right() + inset, body.top() + inset),
                pos2(body.right() - inset, body.bottom()),
            )
        };

        // Search, chips and the toolbar.
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
        if narrow {
            // The kinds as a row of chips that scrolls sideways.
            let mut nodes = vec![(Node::All, tr!("start-all"))];
            nodes.extend(
                kinds
                    .iter()
                    .map(|kind| (Node::Kind(kind.id.clone()), crate::localize(&kind.name))),
            );
            nodes.push((Node::Favorite, tr!("start-favorite")));
            nodes.push((Node::Recent, tr!("start-recently-used")));
            egui::ScrollArea::horizontal()
                .id_salt("sue-paint-materials-kinds")
                .show(&mut area, |ui| {
                    ui.horizontal(|ui| {
                        for (node, name) in nodes {
                            let on = state.node == node;
                            if page::chip(ui, &name, on, theme).clicked() {
                                state.node = node;
                                state.chosen.clear();
                                state.armed = false;
                            }
                        }
                    });
                });
            area.add_space(8.0);
        }
        area.horizontal(|ui| {
            // The name as tall as a chip, centered on it.
            let galley = ui.painter().layout_no_wrap(
                tr!("start-type"),
                theme.font(theme.small_size + 1.0),
                theme.muted,
            );
            let height = page::chip_height(ui, theme).max(galley.size().y);
            let (rect, _) =
                ui.allocate_exact_size(vec2(galley.size().x + 6.0, height), Sense::hover());
            let at = pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(at, galley, theme.muted);
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
        let shown = shown(&entries, state);
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
            // A phone has no room for these words: Add is beside the
            // place's menu, and a material's popup makes it a favorite.
            if !narrow {
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
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                view_buttons(ui, &mut state.view, true, theme);
                ui.add_space(6.0);
                sort_buttons(
                    ui,
                    "sue-paint-material-sort",
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
            let chosen: Vec<EntityId> = hub.materials.chosen.iter().copied().collect();
            let library = hub.library(&kinds, folder.clone());
            for id in &chosen {
                if let Err(error) = library.set_favorite(*id, on) {
                    cx.fail(error);
                }
            }
        }
        let state = &mut hub.materials;
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
        let view = state.view;
        let hover = if canvas {
            tr!("start-click-to-put-on-canvas")
        } else {
            tr!("start-open-canvas-for-materials")
        };
        let pictures: Vec<_> = shown
            .iter()
            .map(|entry| self.material_texture(entry, cx))
            .collect();
        let chosen: Vec<bool> = shown
            .iter()
            .map(|entry| state.chosen.contains(&entry.id))
            .collect();
        let mut clicks = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-materials")
            .auto_shrink([false, false])
            .show(&mut area, |ui| {
                let drawn = material_cards(ui, &shown, &pictures, &chosen, view, &kinds, theme);
                for (index, click, response) in drawn {
                    if let Some(click) = click {
                        clicks.push((index, click));
                    }
                    let response = response.on_hover_text(&hover);
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
                }
            });
        for (index, click) in clicks {
            let Some(entry) = shown.get(index) else {
                continue;
            };
            match click {
                CardClick::Open if canvas => acts.push(Act::Paste(entry.id)),
                CardClick::Open | CardClick::Choose => {
                    let state = &mut hub.materials;
                    if !state.chosen.remove(&entry.id) {
                        state.chosen.insert(entry.id);
                    }
                    state.armed = false;
                }
                CardClick::Favorite => {
                    let library = hub.library(&kinds, folder.clone());
                    if let Err(error) = library.set_favorite(entry.id, !entry.favorite) {
                        cx.fail(error);
                    }
                }
                CardClick::Details => {
                    hub.armed = false;
                    hub.details = Some(entry.id);
                }
            }
        }
    }

    /// The texture of a material's picture, made once.
    fn material_texture(
        &mut self,
        entry: &crate::materials::Entry,
        cx: &mut AppletContext<'_>,
    ) -> Option<(egui::TextureId, [u32; 2])> {
        let side = 128_u32;
        let key = texture_key(&entry.material.key());
        let thumb = self
            .material_thumbs
            .entry(key)
            .or_insert_with(|| crate::panels::material_thumb(&entry.material, side));
        cx.textures
            .texture(key, 1, [side, side], 4, thumb, false)
            .map(|texture| (texture, [side, side]))
    }

    /// What a material is, with what can be done to it.
    #[allow(clippy::too_many_arguments, reason = "the popup and what it reads")]
    fn material_popup(
        &mut self,
        ctx: &egui::Context,
        full: bool,
        hub: &mut MaterialsPlace,
        acts: &mut Vec<Act>,
        id: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let folder = crate::materials::location(cx);
        let kinds = self.material_kinds.clone();
        let Some(entry) = hub
            .library(&kinds, folder.clone())
            .entries()
            .into_iter()
            .find(|entry| entry.id == id)
        else {
            hub.details = None;
            return;
        };
        let picture = self.material_texture(&entry, cx);
        let canvas = !self.docs.is_empty();
        let armed = hub.armed;
        let mut close = false;
        let mut favorite = None;
        let mut arm = false;
        let mut deleted = false;
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
                            deleted = true;
                            close = true;
                        } else {
                            arm = true;
                        }
                    }
                }
            },
        );
        if let Some(on) = favorite
            && let Err(error) = hub.library(&kinds, folder).set_favorite(id, on)
        {
            cx.fail(error);
        }
        if arm {
            hub.armed = true;
        }
        if deleted {
            self.material_thumbs.clear();
            hub.materials_changed();
        }
        if close || outcome == PopupOutcome::Closed {
            hub.details = None;
            hub.armed = false;
        }
    }
}

/// Draws the materials `shown` as the Files place draws its files: as
/// thumbnail cards that fill the rows evenly (`view` Large or Small), or
/// as list rows, each with Favorite and Details at its end and a tick
/// box for the toolbar. `pictures` and `chosen` go with `shown` one to
/// one. Returns every card's click, if any, and its response.
fn material_cards(
    ui: &mut Ui,
    shown: &[crate::materials::Entry],
    pictures: &[Option<(egui::TextureId, [u32; 2])>],
    chosen: &[bool],
    view: View,
    kinds: &crate::materials::MaterialKinds,
    theme: &Theme,
) -> Vec<(usize, Option<CardClick>, egui::Response)> {
    let density = Density::of(ui);
    let width = ui.available_width();
    let gap = theme.space(Space::S5);
    let (favorite, details, select) = (
        tr!("start-favorite"),
        tr!("start-details"),
        tr!("start-select-all"),
    );
    // As many columns as cards of the view's smallest width fit, each
    // stretched so that the rows end flush.
    let size = (view != View::List).then(|| {
        let least = theme.space(Space::S9) * if view == View::Large { 4.5 } else { 3.0 };
        let columns = ((width + gap) / (least + gap)).floor().max(1.0) as usize;
        let card = ((width - gap * (columns - 1) as f32) / columns as f32).floor();
        (
            columns,
            vec2(card, card.round() + ListRow::height(theme, density, true)),
        )
    });
    let mut drawn = Vec::new();
    let mut one = |ui: &mut Ui, index: usize| {
        let entry = &shown[index];
        let title = crate::localize(&entry.material.name);
        let kind = kinds
            .get(&entry.material.kind)
            .map(|kind| crate::localize(&kind.name))
            .unwrap_or_default();
        let actions = [
            RowAction::new(Icon::Heart, &favorite).on(entry.favorite),
            RowAction::new(Icon::Details, &details),
        ];
        let picture = pictures.get(index).copied().flatten();
        let ticked_now = chosen.get(index).copied().unwrap_or(false);
        let (response, ticked, action) = match size {
            None => {
                let leading = picture.map_or(Leading::Icon(Icon::Materials), |(texture, _)| {
                    Leading::Picture(texture)
                });
                let row = ListRow::new(("sue-paint-material-row", entry.id), &title)
                    .detail(&kind)
                    .leading(leading)
                    .actions(&actions)
                    .selected(ticked_now)
                    .width(width)
                    .show(ui, theme);
                (row.response, false, row.action)
            }
            Some((_, size)) => {
                let thumbnail = picture.map_or(
                    Thumbnail::Placeholder(Icon::Materials),
                    |(texture, size)| Thumbnail::Picture(texture, size),
                );
                let out = ThumbnailCard::new(("sue-paint-material-card", entry.id), &title)
                    .detail(&kind)
                    .thumbnail(thumbnail)
                    .actions(&actions)
                    .selected(ticked_now)
                    .tick(ticked_now, &select)
                    .size(size)
                    .show(ui, theme);
                (out.response, out.ticked, out.action)
            }
        };
        let click = if ticked {
            Some(CardClick::Choose)
        } else if let Some(action) = action {
            Some(if action == 0 {
                CardClick::Favorite
            } else {
                CardClick::Details
            })
        } else if response.clicked() {
            let chooses = response
                .ctx
                .input(|input| input.modifiers.command || input.modifiers.shift);
            Some(if chooses {
                CardClick::Choose
            } else {
                CardClick::Open
            })
        } else {
            None
        };
        drawn.push((index, click, response));
    };
    match size {
        None => {
            ui.spacing_mut().item_spacing.y = theme.space(Space::S1);
            for index in 0..shown.len() {
                one(ui, index);
            }
        }
        Some((columns, _)) => {
            let mut index = 0;
            while index < shown.len() {
                let end = (index + columns).min(shown.len());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for at in index..end {
                        one(ui, at);
                    }
                });
                ui.add_space(gap);
                index = end;
            }
        }
    }
    drawn
}

/// The rows of the materials tree: All (with the kinds under it while
/// open), then Favorite and Recently used.
fn tree(kinds: &crate::materials::MaterialKinds, open: bool) -> Vec<(Node, Category)> {
    let mut nodes = vec![(
        Node::All,
        Category {
            glyph: Some(Glyph::All),
            label: tr!("start-all-materials"),
            depth: 0,
            open: Some(open),
        },
    )];
    if open {
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
    nodes
}

/// The materials `state` lets through, sorted as it asks.
fn shown(
    entries: &[crate::materials::Entry],
    state: &MaterialsState,
) -> Vec<crate::materials::Entry> {
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
        1 => shown.sort_by_cached_key(|entry| crate::localize(&entry.material.name).to_lowercase()),
        2 => shown.sort_by_key(|entry| std::cmp::Reverse(entry.used)),
        _ => shown.sort_by_key(|entry| std::cmp::Reverse(entry.added)),
    }
    if state.reversed {
        shown.reverse();
    }
    shown
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

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::ReversibleTransaction;
    use sue_wm::files::Thumbs;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sue-paint-hub-{tag}-{}", std::process::id()));
        let _ = sue_platform::fs::remove_dir_all(&dir);
        sue_platform::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sue_paint_adds_one_place_to_the_hub_its_materials() {
        let places = places();
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].id, PLACE);
        assert_eq!(places[0].label, "Materials");
        assert_eq!(places[0].icon, Icon::Materials);
    }

    #[test]
    fn the_hub_draws_a_project_without_a_preview_of_its_own() {
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

        let mut thumbs = Thumbs::new(Some(thumbnailer()));
        assert!(thumbs.get(&project).is_none());
        let begun = std::time::Instant::now();
        while thumbs.poll() {
            assert!(begun.elapsed().as_secs() < 20);
            std::thread::yield_now();
        }
        let (_, thumb) = thumbs.get(&project).expect("Sue Paint's picture");
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

        let mut place = MaterialsPlace::default();
        // No cache folder known: nothing happens, and it is not tried again.
        place.forget_old_cache(None);
        assert!(sue_platform::fs::is_dir(&old));
        place.forget_old_cache(Some(&cache));
        assert!(sue_platform::fs::is_dir(&old), "one look in a run");

        let mut place = MaterialsPlace::default();
        place.forget_old_cache(Some(&cache));
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
    fn the_place_opens_its_menu_by_name_and_the_start_routes_are_gone() {
        let mut place = MaterialsPlace::default();
        assert!(place.navigate("materials/menu"));
        assert!(place.menu.is_open());
        place.details = Some(EntityId::from_u128(7));
        place.armed = true;
        assert!(place.navigate("materials/close"));
        assert!(!place.menu.is_open());
        assert!(place.details.is_none() && !place.armed);
        assert!(!place.navigate("materials/nowhere"));
        // Nothing of the launcher answers any more.
        for gone in [
            "start/projects",
            "start/materials",
            "start/help",
            "start/show",
        ] {
            assert!(!place.navigate(gone), "{gone}");
        }
    }

    #[test]
    fn the_materials_shown_follow_the_branch_the_search_and_the_origin() {
        let root = temp("shown");
        let kinds = crate::materials::MaterialKinds::builtin();
        let mut library = Library::scan(&kinds, Some(&root));
        let all = library.entries();
        assert!(!all.is_empty(), "Sue Paint has materials of its own");
        let mut state = MaterialsState::default();
        assert_eq!(shown(&all, &state).len(), all.len());
        // A kind shows only its own.
        let kind = all[0].material.kind.clone();
        state.node = Node::Kind(kind.clone());
        let of_kind = shown(&all, &state);
        assert!(!of_kind.is_empty());
        assert!(of_kind.iter().all(|entry| entry.material.kind == kind));
        // Favorites: none until one is made a favorite.
        state.node = Node::Favorite;
        assert!(shown(&all, &state).is_empty());
        library.set_favorite(all[0].id, true).unwrap();
        let all = library.entries();
        let favorites = shown(&all, &state);
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].id, all[0].id);
        // The search matches the name, in any case.
        state.node = Node::All;
        let name = crate::localize(&all[0].material.name);
        state.search = name.to_uppercase();
        assert!(
            shown(&all, &state)
                .iter()
                .any(|entry| entry.id == all[0].id)
        );
        state.search = "no material is called this".to_owned();
        assert!(shown(&all, &state).is_empty());
        // Added by you: nothing yet; Sue Paint's own: all of them.
        state.search.clear();
        state.origins.insert(1);
        assert!(shown(&all, &state).is_empty());
        state.origins = HashSet::from([0]);
        assert_eq!(shown(&all, &state).len(), all.len());
        // By name, then the other way round.
        state.origins.clear();
        state.sort = 1;
        let names = |state: &MaterialsState| -> Vec<String> {
            shown(&all, state)
                .iter()
                .map(|entry| crate::localize(&entry.material.name).to_lowercase())
                .collect()
        };
        let mut sorted = names(&state);
        sorted.sort();
        assert_eq!(names(&state), sorted);
        state.reversed = true;
        sorted.reverse();
        assert_eq!(names(&state), sorted);
        let _ = sue_platform::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_new_canvas_page_opens_by_its_own_names() {
        let mut page = crate::mobile::NewCanvas::default();
        assert!(page.navigate("new-canvas"));
        assert_eq!(page.category, Some(0));
        assert!(page.navigate("new-canvas/2"));
        assert_eq!(page.category, Some(2));
        assert!(page.navigate("new-canvas/close"));
        assert_eq!(page.category, None);
        assert!(!page.navigate("new-canvas/x"));
        assert!(!page.navigate("start/new"));
    }
}
