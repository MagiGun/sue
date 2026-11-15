//! The Item Bank palette: sub tools kept to add to their tool again, and
//! pictures kept to drag onto the canvas (gap FG-31 of the 0.0.8 plan).
//!
//! The sub tools are lines of `item_bank.txt` in Sue Paint's data folder
//! (as 0.0.75 wrote them); the pictures are PNG files in its `item_bank`
//! folder, so people can put pictures there themselves or share the
//! folder. A picture dragged out of the palette and let go over the
//! canvas lands on a new layer with its middle where it was let go; a
//! double click puts it in the middle. Either is one undo step.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Id, Pos2, Rect, Sense, StrokeKind, Ui, pos2, vec2};
use sue_core::{PixelFormat, RasterTile2D};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, ListRow, RowAction, Space, TextRole, Tip};
use sue_wm::{AppletContext, AppletId, FilePick, ShellRequest, Theme};

use crate::brush::Tip as BrushTip;
use crate::studio::SuePaint;
use crate::tools::{SubTool, Tool};

/// The folder of the Item Bank's pictures, in Sue Paint's data folder.
pub(crate) const PICTURES: &str = "item_bank";

/// The commands of the Item Bank's palette menu.
pub(crate) const MENU_COMMANDS: [&str; 3] = [
    "item_bank.keep_sub_tool",
    "item_bank.keep_layer",
    "item_bank.add_picture",
];

/// What is being dragged from a palette onto the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CanvasDrop {
    /// An Item Bank picture, by its file.
    Picture(PathBuf),
    /// A material, by its library key.
    Material(String),
}

/// The Item Bank's pictures in `folder`, by name.
pub(crate) fn bank_pictures(folder: Option<&Path>) -> Vec<PathBuf> {
    let Some(entries) =
        folder.and_then(|folder| sue_platform::list_dir(&folder.join(PICTURES), true).ok())
    else {
        return Vec::new();
    };
    let mut pictures: Vec<PathBuf> = entries
        .into_iter()
        .map(|entry| entry.path)
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        })
        .collect();
    pictures.sort();
    pictures
}

/// The picture file `path` as an RGBA8 tile.
pub(crate) fn picture_tile(path: &Path) -> Option<RasterTile2D> {
    let (size, pixels) = crate::subview::decode(path).ok()?;
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, pixels).ok()
}

/// The name a picture file goes by: its file name without `.png`.
fn picture_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A `side` square thumbnail of `tile` over clear.
fn thumbnail(tile: &RasterTile2D, side: u32) -> Vec<u8> {
    crate::doc::fitted(tile, [side, side])
        .and_then(|fitted| fitted.bytes().map(<[u8]>::to_vec))
        .unwrap_or_else(|| vec![0; (side * side * 4) as usize])
}

/// Thumbnails of the Item Bank's pictures, by file and side.
pub(crate) type Thumbs = HashMap<(PathBuf, u32), Vec<u8>>;

/// A sub tool kept in the Item Bank.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub tool: Tool,
    pub name: String,
    pub tip: BrushTip,
    pub stabilization: u32,
}

impl Item {
    /// The item for `sub`.
    #[must_use]
    pub fn of(sub: &SubTool) -> Self {
        Self {
            tool: sub.tool,
            name: sub.name.clone(),
            tip: sub.tip,
            stabilization: sub.stabilization,
        }
    }

    /// One line: tool, name, stabilization and brush, tab separated.
    #[must_use]
    pub fn to_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.tool.label(),
            self.name.replace('\t', " "),
            self.stabilization,
            self.tip.to_text()
        )
    }

    /// An item from [`Self::to_line`].
    #[must_use]
    pub fn from_line(line: &str) -> Option<Self> {
        let mut parts = line.splitn(4, '\t');
        let label = parts.next()?;
        let tool = Tool::ALL.into_iter().find(|tool| tool.label() == label)?;
        Some(Self {
            tool,
            name: parts.next()?.to_owned(),
            stabilization: parts.next()?.parse().ok()?,
            tip: BrushTip::from_text(parts.next()?),
        })
    }
}

impl SuePaint {
    /// Keeps the current sub tool in the Item Bank.
    pub(crate) fn bank_keep_sub_tool(&mut self, cx: &mut AppletContext<'_>) {
        self.item_bank.push(Item::of(self.toolbox.current()));
        self.save_extras(cx);
    }

    /// Asks for a picture to keep in the Item Bank.
    pub(crate) fn bank_pick_picture(&mut self, cx: &mut AppletContext<'_>) {
        cx.request(ShellRequest::PickFile(
            FilePick::open(
                AppletId::new(crate::studio::ID).ok(),
                "bank-picture",
                &tr!("item-bank-add-picture"),
            )
            .extensions(&[
                "png", "jpg", "jpeg", "jxl", "webp", "bmp", "gif", "tif", "tiff", "qoi",
            ]),
        ));
    }

    /// Keeps `tile` as an Item Bank picture named `name` (cut to what it
    /// covers), under a name not taken.
    fn bank_keep(&mut self, cx: &mut AppletContext<'_>, name: &str, tile: &RasterTile2D) {
        let Some(folder) = crate::colorsets::folder(cx) else {
            cx.fail(tr!("sue-paint-has-no-data-folder"));
            return;
        };
        let written = crate::materials::cut_png(tile).and_then(|png| {
            let dir = folder.join(PICTURES);
            sue_platform::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            let base: String = name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == ' ' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let base = if base.trim().is_empty() {
                tr!("item-bank-picture")
            } else {
                base.trim().to_owned()
            };
            let mut path = dir.join(format!("{base}.png"));
            let mut n = 2;
            while sue_platform::fs::exists(&path) {
                path = dir.join(format!("{base} {n}.png"));
                n += 1;
            }
            sue_platform::write_atomic(&path, &png).map_err(|error| error.to_string())?;
            Ok(path)
        });
        match written {
            Ok(path) => {
                cx.notify(tr!("item-bank-kept", name = picture_name(&path)));
                self.extras.pictures = bank_pictures(Some(&folder));
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Keeps the current raster layer as an Item Bank picture.
    pub(crate) fn bank_keep_layer(&mut self, cx: &mut AppletContext<'_>) {
        let arena = &cx.world.arena;
        let Some(info) = self.layer_info(arena) else {
            return;
        };
        let Some(raster) = crate::doc::raster(arena, info.id).cloned() else {
            cx.fail(tr!("only-raster-layers-can-copied"));
            return;
        };
        self.bank_keep(cx, &info.name, &raster);
    }

    /// Takes a picture chosen for the Item Bank (`bank-picture`): `true`
    /// when `purpose` was that.
    pub(crate) fn bank_picked(
        &mut self,
        purpose: &str,
        path: &Path,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        if purpose != "bank-picture" {
            return false;
        }
        match picture_tile(path) {
            Some(tile) => self.bank_keep(cx, &picture_name(path), &tile),
            None => cx.fail(tr!("cannot-read-the-material", name = picture_name(path))),
        }
        true
    }

    /// Puts what was dragged from the Item Bank or the Material palette
    /// on the canvas, when it was let go over the canvas this frame.
    pub(crate) fn canvas_drops(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        let released = ctx.input(|input| input.pointer.any_released());
        let at = ctx.input(|input| input.pointer.interact_pos().or(input.pointer.latest_pos()));
        let (Some(at), true) = (at, released) else {
            return;
        };
        if !self.viewport.contains(at) {
            return;
        }
        let Some(dropped) = egui::DragAndDrop::take_payload::<CanvasDrop>(ctx) else {
            return;
        };
        self.drop_on_canvas(&dropped, at, cx);
    }

    /// Puts `dropped` on the canvas with its middle at the screen point
    /// `at`.
    pub(crate) fn drop_on_canvas(
        &mut self,
        dropped: &CanvasDrop,
        at: Pos2,
        cx: &mut AppletContext<'_>,
    ) {
        let viewport = self.viewport;
        let Some(doc) = self.doc() else {
            return;
        };
        let size = cx
            .world
            .arena
            .get_by_id(doc.picture)
            .ok()
            .and_then(sue_model::picture::size)
            .unwrap_or([0, 0]);
        let point = doc.view.to_picture(viewport, size, at);
        let point = Some([point.x, point.y]);
        match dropped {
            CanvasDrop::Picture(path) => match picture_tile(path) {
                Some(tile) => {
                    self.paste_picture_at(cx, &picture_name(path), &tile, point);
                }
                None => cx.fail(tr!("cannot-read-the-material", name = picture_name(path))),
            },
            CanvasDrop::Material(key) => self.drop_material(key, point, cx),
        }
    }

    /// The Item Bank palette.
    pub(crate) fn item_bank_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        // Shown again after a while away: the folder is read again, so
        // pictures people put there themselves show.
        let pass = ui.ctx().cumulative_pass_nr();
        if self.extras.bank_drawn.is_none_or(|last| last + 1 < pass) {
            self.extras.pictures = bank_pictures(crate::colorsets::folder(cx).as_deref());
        }
        self.extras.bank_drawn = Some(pass);
        let density = Density::of(ui);
        let pad = theme.space(Space::S3);
        let gap = theme.space(Space::S2);
        let button = density.icon_button();
        let body = body.shrink(pad);
        let list = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - button - gap));
        let mut inner = crate::panels::child(ui, list, "item-bank");
        let mut add = None;
        let mut remove = None;
        let mut remove_picture = None;
        let mut double = None;
        let side = (button * 2.0).round();
        let names: Vec<String> = self
            .item_bank
            .iter()
            .map(|item| item.name.clone())
            .collect();
        let tools: Vec<String> = self
            .item_bank
            .iter()
            .map(|item| item.tool.label())
            .collect();
        let pictures = self.extras.pictures.clone();
        let add_name = tr!("item-bank-add-to-tool");
        let remove_name = tr!("item-bank-remove");
        let help = tr!("item-bank-picture-help");
        let actions = [
            RowAction::new(Icon::Plus, &add_name),
            RowAction::new(Icon::Close, &remove_name),
        ];
        let thumbs = &mut self.extras.bank_thumbs;
        let caption = |ui: &mut Ui, text: String| {
            ui.label(
                egui::RichText::new(text)
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
            );
        };
        sue_wm::ui::ScrollView::vertical("sue-paint-bank")
            .on(theme.panel)
            .show(&mut inner, theme, |ui| {
                sue_wm::ui::heading(ui, &tr!("item-bank-sub-tools"), theme);
                if names.is_empty() {
                    caption(ui, tr!("kept-sub-tools-show-here-add"));
                }
                for (index, (name, tool)) in names.iter().zip(&tools).enumerate() {
                    let output = ListRow::new(("bank-item", index, name), name)
                        .note(tool)
                        .actions(&actions)
                        .show(ui, theme);
                    match output.action {
                        Some(0) => add = Some(index),
                        Some(_) => remove = Some(index),
                        None => {}
                    }
                    if output.response.double_clicked() {
                        add = Some(index);
                    }
                }
                sue_wm::ui::heading(ui, &tr!("item-bank-pictures"), theme);
                if pictures.is_empty() {
                    caption(ui, tr!("item-bank-pictures-empty"));
                }
                let columns = ((ui.available_width() + gap) / (side + gap))
                    .floor()
                    .max(1.0) as usize;
                for row in pictures.chunks(columns) {
                    ui.horizontal(|ui| {
                        for path in row {
                            let name = picture_name(path);
                            let id = Id::unique(("sue-paint-bank-picture", path));
                            let thumb =
                                thumbs
                                    .entry((path.clone(), side as u32))
                                    .or_insert_with(|| {
                                        picture_tile(path).map_or_else(
                                            || vec![0; (side as usize).pow(2) * 4],
                                            |tile| thumbnail(&tile, side as u32),
                                        )
                                    });
                            let key = {
                                use std::hash::{Hash, Hasher};
                                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                                path.hash(&mut hasher);
                                hasher.finish()
                            };
                            let texture = cx.textures.texture(
                                key ^ 0x4241_4e4b,
                                1,
                                [side as u32; 2],
                                4,
                                thumb,
                                false,
                            );
                            let response = ui
                                .dnd_drag_source(id, CanvasDrop::Picture(path.clone()), |ui| {
                                    let (rect, _) =
                                        ui.allocate_exact_size(vec2(side, side), Sense::hover());
                                    let corner = theme.corner(sue_wm::ui::Radius::Sm);
                                    ui.painter().rect_filled(rect, corner, theme.void);
                                    if let Some(texture) = texture {
                                        ui.painter().image(
                                            texture,
                                            rect,
                                            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                            crate::panels::run_button::UNTINTED,
                                        );
                                    }
                                    ui.painter().rect_stroke(
                                        rect,
                                        corner,
                                        theme.hairline(),
                                        StrokeKind::Inside,
                                    );
                                })
                                .response;
                            let response = response.interact(Sense::click());
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Button, true, &name)
                            });
                            sue_wm::ui::focus_ring(
                                ui,
                                &response,
                                theme.radius(sue_wm::ui::Radius::Sm),
                                theme,
                            );
                            let response =
                                sue_wm::ui::tooltip(response, &Tip::new(&name).help(&help), theme);
                            if response.double_clicked() {
                                double = Some(path.clone());
                            }
                            response.context_menu(|ui| {
                                if ui.button(&remove_name).clicked() {
                                    remove_picture = Some(path.clone());
                                    ui.close();
                                }
                            });
                        }
                    });
                }
            });
        // The buttons under the list.
        let row = Rect::from_min_size(
            pos2(body.left(), body.bottom() - button),
            vec2(body.width(), button),
        );
        let buttons = crate::panels::child(ui, row, "item-bank-buttons");
        let mut x = row.left();
        for (command, icon) in [
            ("item_bank.keep_sub_tool", Icon::Plus),
            ("item_bank.keep_layer", Icon::Picture),
            ("item_bank.add_picture", Icon::Import),
        ] {
            let rect = Rect::from_min_size(pos2(x, row.top()), vec2(button, button));
            self.command_button(&buttons, rect, command, icon, cx, theme);
            x += button + gap;
        }
        if let Some(index) = add
            && let Some(item) = self.item_bank.get(index).cloned()
        {
            let mut sub =
                SubTool::new(item.tool, &item.name, 0, item.tip).stabilization(item.stabilization);
            sub.name = item.name;
            self.toolbox.add_sub(sub);
        }
        if let Some(index) = remove {
            self.item_bank.remove(index);
            self.save_extras(cx);
        }
        if let Some(path) = remove_picture {
            match sue_platform::fs::remove_file(&path) {
                Ok(()) => self.extras.pictures.retain(|known| *known != path),
                Err(error) => cx.fail(error.to_string()),
            }
        }
        if let Some(path) = double
            && let Some(tile) = picture_tile(&path)
        {
            self.paste_picture_at(cx, &picture_name(&path), &tile, None);
        }
        self.canvas_drops(ui.ctx(), cx);
    }
}
