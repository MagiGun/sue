//! The sheets Sue Paint's touch layouts (Simple Mode and Studio Mode)
//! bring up from the bottom of the screen, one at a time: the tools, the
//! sub tools, the layers, the color, the timeline, the reference pictures,
//! the navigator, the materials and the menus. They are the same palettes
//! the desktop layout docks; only where they sit changes. Each is Sue's
//! own sheet (`sue_wm::ui::Sheet`): drag it down, tap beside it or press
//! Escape to send it away.

use egui::{Ui, UiBuilder, Vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density};
use sue_wm::{AppletContext, Theme};

use crate::studio::SuePaint;
use crate::studio_mode::tool_glyph;
use crate::tools::Tool;

/// A tile of the Tools or Pens sheet: its icon, name, whether it can be
/// used and is lit, and the tool palette entry (or tool) it picks.
type SheetTile = (Icon, String, bool, bool, Result<usize, Tool>);

/// What a sheet shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Sheet {
    /// Studio Mode's Menu: home, Simple Mode, Settings, then the menus.
    Menu,
    /// Every tool, as a grid.
    Tools,
    /// Sub tools and their properties.
    ToolSettings,
    /// Layers.
    Layers,
    /// The color wheel and the color set.
    Color,
    /// The animation timeline.
    Timeline,
    /// The navigator.
    Navigator,
    /// The current layer's settings.
    LayerProperty,
    /// The materials.
    Materials,
    /// The reference pictures (Sub View).
    Reference,
    /// Simple Mode's "..." menu.
    More,
    /// Simple Mode's pens and brushes.
    Pens,
}

impl Sheet {
    /// The sheet's title.
    fn title(self) -> String {
        match self {
            Self::Menu => tr!("menus-2"),
            Self::Tools => tr!("keys-group-tools"),
            Self::ToolSettings => tr!("tool-settings"),
            Self::Layers => tr!("layers-3"),
            Self::Color => tr!("color-2"),
            Self::Timeline => tr!("timeline"),
            Self::Navigator => tr!("navigator"),
            Self::LayerProperty => tr!("studio-layer-property"),
            Self::Materials => tr!("start-manage-materials"),
            Self::Reference => tr!("simple-reference"),
            Self::More => tr!("simple-more"),
            Self::Pens => tr!("simple-pens"),
        }
    }

    /// How much of `height` (the canvas's) a palette on the sheet takes;
    /// `None` for the lists and grids, which are as tall as what they
    /// hold.
    fn share(self, height: f32) -> Option<f32> {
        match self {
            Self::Layers | Self::ToolSettings => Some(height * 0.75),
            Self::Materials => Some(height * 0.7),
            Self::Reference => Some(height * 0.6),
            Self::Navigator | Self::LayerProperty | Self::Color | Self::Timeline => {
                Some((height * 0.55).min(420.0))
            }
            Self::Menu | Self::More | Self::Tools | Self::Pens => None,
        }
    }
}

impl SuePaint {
    pub(crate) fn toggle_sheet(&mut self, sheet: Sheet) {
        self.sheet = if self.sheet == Some(sheet) {
            None
        } else {
            Some(sheet)
        };
    }

    /// The sheet `sheet`, sized for a canvas `area` tall. A command one
    /// of its rows runs goes to `run`.
    pub(crate) fn phone_sheet(
        &mut self,
        ui: &mut Ui,
        area: egui::Rect,
        sheet: Sheet,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let title = sheet.title();
        let shown =
            ui::Sheet::new(("sue-paint-sheet", sheet), &title, theme).show(ui.ctx(), |ui| {
                match sheet.share(area.height()) {
                    Some(height) => {
                        let (_, body) = ui.allocate_space(Vec2::new(ui.available_width(), height));
                        let mut inner = ui.new_child(UiBuilder::new().max_rect(body));
                        inner.set_clip_rect(body.intersect(ui.clip_rect()));
                        self.sheet_palette(&mut inner, body, sheet, cx, theme);
                    }
                    None => self.sheet_list(ui, sheet, cx, theme, run),
                }
            });
        if shown.dismissed {
            self.sheet = None;
        }
        // A row that ran a command, or a tile that picked a tool, closes
        // the sheet it was on.
        if run.is_some() && matches!(sheet, Sheet::Menu | Sheet::More) {
            self.sheet = None;
        }
    }

    /// A palette on its sheet, in `body`.
    fn sheet_palette(
        &mut self,
        ui: &mut Ui,
        body: egui::Rect,
        sheet: Sheet,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        match sheet {
            Sheet::Layers => self.layer_panel(ui, body, cx, theme),
            Sheet::Color => self.color_panel(ui, body, theme),
            Sheet::ToolSettings => self.tool_panel(ui, body, cx, theme),
            Sheet::Timeline => self.timeline_panel(ui, body, cx, theme),
            Sheet::Reference => self.sub_view(ui, body, cx, theme),
            Sheet::Navigator => self.navigator(ui, body, cx, theme),
            Sheet::Materials => self.material_panel(ui, body, cx, theme),
            Sheet::LayerProperty => {
                let info = self.layer_info(&cx.world.arena);
                egui::ScrollArea::vertical()
                    .id_salt("sue-paint-touch-layer-property")
                    .show(ui, |ui| {
                        self.layer_property(ui, info.as_ref(), cx, theme);
                    });
            }
            Sheet::Menu | Sheet::More | Sheet::Tools | Sheet::Pens => {}
        }
    }

    /// A sheet that is a list or a grid.
    fn sheet_list(
        &mut self,
        ui: &mut Ui,
        sheet: Sheet,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        match sheet {
            Sheet::Menu => self.studio_menu(ui, cx, theme, run),
            Sheet::More => {
                if let Some(command) = self.simple_more(ui, cx, theme) {
                    *run = Some(command);
                }
            }
            Sheet::Tools | Sheet::Pens => self.tool_tiles(ui, sheet, theme),
            _ => {}
        }
    }

    /// The Tools sheet (the tool palette itself, Liquify and any sub tool
    /// made into a tool among them) or the Pens sheet, as tiles.
    fn tool_tiles(&mut self, ui: &mut Ui, sheet: Sheet, theme: &Theme) {
        let current = self.toolbox.current_slot();
        let kind = self.toolbox.current_entry().kind;
        let tiles: Vec<SheetTile> = if sheet == Sheet::Pens {
            crate::simple::PENS
                .iter()
                .map(|tool| {
                    let lit = *tool == kind;
                    (
                        tool_glyph(*tool),
                        tool.label(),
                        tool.available(),
                        lit,
                        Err(*tool),
                    )
                })
                .collect()
        } else {
            self.toolbox
                .slots()
                .iter()
                .enumerate()
                .map(|(at, slot)| {
                    let icon = if crate::panels::slot_is_liquify(slot) {
                        Icon::ToolLiquify
                    } else {
                        tool_glyph(slot.kind)
                    };
                    let name = crate::localize(&slot.name);
                    (icon, name, slot.kind.available(), at == current, Ok(at))
                })
                .collect()
        };
        let size = ui::Tile::default_size(Density::Compact);
        let mut picked = None;
        ui.horizontal_wrapped(|ui| {
            for (index, (icon, name, available, lit, picks)) in tiles.iter().enumerate() {
                if ui::tile(("sue-paint-sheet-tool", index), name)
                    .icon(*icon)
                    .enabled(*available)
                    .selected(*lit)
                    .size(size)
                    .show(ui, theme)
                    .clicked()
                {
                    picked = Some(*picks);
                }
            }
        });
        if let Some(picks) = picked {
            match picks {
                Ok(slot) => self.toolbox.pick_slot(slot),
                Err(tool) => self.toolbox.pick(tool),
            }
            self.sheet = None;
        }
    }

    /// Sue Paint's menus as a tree, each menu folded under its title.
    pub(crate) fn menu_list(
        &mut self,
        ui: &mut Ui,
        cx: &AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let facts = sue_wm::Facts::of(cx);
        let menus = self.menus.clone();
        for menu in &menus {
            let open = folded_row(ui, &menu.title, 0, theme);
            if open {
                menu_items(
                    ui,
                    &menu.items,
                    1,
                    &|command| self.state(command, &facts),
                    theme,
                    run,
                );
            }
        }
    }
}

/// A row that folds what is under it: `title` at `depth`, open or shut
/// as it was left. Returns whether it is open.
fn folded_row(ui: &mut Ui, title: &str, depth: usize, theme: &Theme) -> bool {
    let id = ui.scope_id().with(("sue-paint-touch-fold", title, depth));
    let open = ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);
    let row = ui::tree_row(("sue-paint-touch-fold", title, depth), title)
        .depth(depth)
        .expanded(open)
        .show(ui, theme);
    if row.toggled || row.response.clicked() {
        ui.data_mut(|data| data.insert_temp(id, !open));
    }
    open
}

/// `items` as rows at `depth`, submenus folded.
fn menu_items(
    ui: &mut Ui,
    items: &[sue_wm::MenuItem],
    depth: usize,
    state: &dyn Fn(&str) -> sue_wm::ItemState,
    theme: &Theme,
    run: &mut Option<String>,
) {
    for item in items {
        match item {
            sue_wm::MenuItem::Action {
                label,
                command,
                shortcut,
                ..
            } => {
                let now = state(command);
                let mut row = ui::tree_row(("sue-paint-touch-item", command), label)
                    .depth(depth)
                    .enabled(now.enabled)
                    .row(|row| row.note(shortcut));
                if now.checked == Some(true) {
                    row = row.icon(Icon::Check);
                }
                if row.show(ui, theme).response.clicked() {
                    *run = Some(command.clone());
                }
            }
            sue_wm::MenuItem::Sub { label, items } => {
                if folded_row(ui, label, depth, theme) {
                    menu_items(ui, items, depth + 1, state, theme, run);
                }
            }
            sue_wm::MenuItem::Separator => {
                ui::divider(ui, theme);
            }
        }
    }
}
