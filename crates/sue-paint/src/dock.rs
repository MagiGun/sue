//! Where Sue Paint's palettes sit, and how people move them, as in Clip
//! Studio Paint: docked in columns at the left and right of the canvas, in
//! the row across the top (the command bar's) or the row under the canvas
//! (the timeline's), stacked as tabs, floating over the canvas, popped out
//! into windows of their own, or collapsed to icons in a strip at the far
//! right. People drag a palette by its title bar, grip, tab or icon to
//! move it, tear a tab off a palette into a palette of its own, drag a
//! column's edge to make it wider (the tool palette grows more columns)
//! and the line between two palettes to share the room differently. When
//! the window is too narrow for every column, the widest collapse to icons
//! by themselves and come back when there is room.
//!
//! The layout itself (columns, groups, tabs, splitters, drop targets and
//! the lines it is kept as) is Sue's shared dock, [`sue_wm::dock`]; this
//! module says what Sue Paint's palettes are and draws them.

use egui::{Id, Order, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use sue_wm::dock::{COLUMN_BAR, Column, DockPanel, Group, Holder, LaidGroup, Target};
use sue_wm::{AppletContext, Theme};

use crate::icons::Icon;
use crate::panels::icon_button;
use crate::studio::{Panels, SuePaint};

pub(crate) use sue_wm::dock::Side;

/// Where every palette is.
pub(crate) type Dock = sue_wm::dock::DockLayout<Palette>;

/// Where everything goes this frame.
pub(crate) type Laid = sue_wm::dock::Laid<Palette>;

/// A palette people can dock, tab, float or collapse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Palette {
    /// The command bar.
    CommandBar,
    /// The tool strip.
    Tools,
    /// Sub Tool and Tool Property.
    SubTool,
    /// The brush size presets.
    BrushSize,
    /// The color wheel and sliders.
    Color,
    /// The color set and history.
    ColorSet,
    /// The navigator and sub view.
    Navigator,
    /// The layers, their properties, history and animation cels.
    Layer,
    /// The materials.
    Material,
    /// The current layer's properties and effects.
    LayerProperty,
    /// The undo history.
    History,
    /// The light table and cel buttons.
    Cels,
    /// A picture to look at or pick colors from.
    SubView,
    /// Recorded steps to play again.
    AutoAction,
    /// Things kept to use again.
    ItemBank,
    /// The pointer's place and the canvas's size.
    Info,
    /// The animation timeline.
    Timeline,
    /// Tool Property, torn off Sub Tool's tabs.
    ToolProperty,
    /// The brush size presets, torn off Sub Tool's tabs.
    BrushGrid,
    /// Color Slider, torn off the color wheel's tabs.
    ColorSlider,
    /// Intermediate Color, torn off the color wheel's tabs.
    Intermediate,
    /// Approximate Color, torn off the color wheel's tabs.
    Approximate,
    /// Color History, torn off the color set's tabs.
    ColorHistory,
}

/// The palettes Clip Studio Paint keeps folded in the strip at the right
/// from the start.
pub(crate) const STRIP_PALETTES: [Palette; 8] = [
    Palette::Material,
    Palette::LayerProperty,
    Palette::Cels,
    Palette::History,
    Palette::SubView,
    Palette::AutoAction,
    Palette::ItemBank,
    Palette::Info,
];

/// Every palette.
const PALETTES: [Palette; 23] = [
    Palette::CommandBar,
    Palette::Tools,
    Palette::SubTool,
    Palette::BrushSize,
    Palette::Color,
    Palette::ColorSet,
    Palette::Navigator,
    Palette::Layer,
    Palette::Material,
    Palette::LayerProperty,
    Palette::History,
    Palette::Cels,
    Palette::SubView,
    Palette::AutoAction,
    Palette::ItemBank,
    Palette::Info,
    Palette::Timeline,
    Palette::ToolProperty,
    Palette::BrushGrid,
    Palette::ColorSlider,
    Palette::Intermediate,
    Palette::Approximate,
    Palette::ColorHistory,
];

impl Palette {
    /// What its window and icon are called.
    pub fn title(self) -> String {
        match self {
            Self::CommandBar => tr!("command-bar"),
            Self::Tools => tr!("tool"),
            Self::SubTool => tr!("sub-tool"),
            Self::BrushSize => tr!("brush-size"),
            Self::Color => tr!("color-wheel"),
            Self::ColorSet => tr!("color-set"),
            Self::Navigator => tr!("navigator"),
            Self::Layer => tr!("layer-2"),
            Self::Material => tr!("material"),
            Self::LayerProperty => tr!("layer-property"),
            Self::History => tr!("history"),
            Self::Cels => tr!("animation-cels"),
            Self::SubView => tr!("sub-view"),
            Self::AutoAction => tr!("auto-actions"),
            Self::ItemBank => tr!("item-bank"),
            Self::Info => tr!("info"),
            Self::Timeline => tr!("timeline"),
            Self::ToolProperty => tr!("tool-settings"),
            Self::BrushGrid => tr!("brush-size"),
            Self::ColorSlider => tr!("color-slider"),
            Self::Intermediate => tr!("intermediate-color"),
            Self::Approximate => tr!("approximate-color"),
            Self::ColorHistory => tr!("color-history"),
        }
    }

    /// Its icon in the strip.
    pub fn icon(self) -> Icon {
        match self {
            Self::CommandBar => Icon::Menu,
            Self::Tools => Icon::Pen,
            Self::SubTool => Icon::Brush,
            Self::BrushSize => Icon::BrushSize,
            Self::Color => Icon::Wheel,
            Self::ColorSet => Icon::Swatches,
            Self::Navigator => Icon::Navigator,
            Self::Layer => Icon::Layers,
            Self::Material => Icon::Material,
            Self::LayerProperty => Icon::LayerProperty,
            Self::History => Icon::History,
            Self::Cels => Icon::Cel,
            Self::SubView => Icon::SubView,
            Self::AutoAction => Icon::AutoAction,
            Self::ItemBank => Icon::ItemBank,
            Self::Info => Icon::Info,
            Self::Timeline => Icon::Timeline,
            Self::ToolProperty => Icon::Sliders,
            Self::BrushGrid => Icon::BrushSize,
            Self::ColorSlider => Icon::ColorSlider,
            Self::Intermediate => Icon::Intermediate,
            Self::Approximate => Icon::Approximate,
            Self::ColorHistory => Icon::History,
        }
    }

    /// `true` when the Window menu shows it.
    pub fn shown(self, panels: &Panels) -> bool {
        match self {
            Self::CommandBar => panels.command_bar,
            Self::Tools => panels.tool,
            Self::SubTool => panels.sub_tool || panels.tool_property,
            Self::BrushSize => panels.brush_size,
            Self::Color => panels.color,
            Self::ColorSet => panels.color_set,
            Self::Navigator => panels.navigator,
            Self::Layer => panels.layer,
            Self::Timeline => panels.timeline,
            Self::Material
            | Self::LayerProperty
            | Self::History
            | Self::Cels
            | Self::SubView
            | Self::AutoAction
            | Self::ItemBank
            | Self::Info
            | Self::ToolProperty
            | Self::BrushGrid
            | Self::ColorSlider
            | Self::Intermediate
            | Self::Approximate
            | Self::ColorHistory => true,
        }
    }

    /// The palette a Window menu command shows as another palette's tab,
    /// which people can tear off into a palette of its own.
    pub fn torn_by(command: &str) -> Option<Self> {
        Some(match command {
            "window.tool_property" => Self::ToolProperty,
            "window.brush_grid" => Self::BrushGrid,
            "window.color_slider" => Self::ColorSlider,
            "window.intermediate" => Self::Intermediate,
            "window.approximate" => Self::Approximate,
            "window.color_history" => Self::ColorHistory,
            "window.material" => Self::Material,
            "window.sub_view" => Self::SubView,
            "window.layer_property" => Self::LayerProperty,
            "window.history" => Self::History,
            "window.cels" => Self::Cels,
            _ => return None,
        })
    }

    /// A palette laid out along a row rather than down a column: the
    /// command bar, or the tool palette in the top or bottom row.
    fn sideways(self, body: Rect) -> bool {
        self == Self::CommandBar
            || (self.fixed_width().is_some() && body.width() > body.height() * 1.3)
    }
}

impl DockPanel for Palette {
    fn all() -> &'static [Self] {
        &PALETTES
    }

    fn key(self) -> &'static str {
        match self {
            Self::CommandBar => "command_bar",
            Self::Tools => "tool",
            Self::SubTool => "sub_tool",
            Self::BrushSize => "brush_size",
            Self::Color => "color",
            Self::ColorSet => "color_set",
            Self::Navigator => "navigator",
            Self::Layer => "layer",
            Self::Material => "material",
            Self::LayerProperty => "layer_property",
            Self::History => "history",
            Self::Cels => "animation_cels",
            Self::SubView => "sub_view",
            Self::AutoAction => "auto_action",
            Self::ItemBank => "item_bank",
            Self::Info => "info",
            Self::Timeline => "timeline",
            Self::ToolProperty => "tool_property",
            Self::BrushGrid => "brush_grid",
            Self::ColorSlider => "color_slider",
            Self::Intermediate => "intermediate_color",
            Self::Approximate => "approximate_color",
            Self::ColorHistory => "color_history",
        }
    }

    /// Its key, or (in layouts saved before keys) its title.
    fn named(text: &str) -> Option<Self> {
        PALETTES
            .into_iter()
            .find(|palette| palette.key() == text)
            .or_else(|| PALETTES.into_iter().find(|palette| palette.title() == text))
    }

    fn fixed_width(self) -> Option<f32> {
        match self {
            Self::Tools => Some(46.0),
            Self::BrushSize => Some(86.0),
            _ => None,
        }
    }

    fn min_width(self) -> f32 {
        match self {
            Self::Tools => 38.0,
            Self::CommandBar => 46.0,
            Self::BrushSize => 56.0,
            _ => 170.0,
        }
    }

    /// Torn off another palette's tab, and nowhere until then.
    fn optional(self) -> bool {
        matches!(
            self,
            Self::ToolProperty
                | Self::BrushGrid
                | Self::ColorSlider
                | Self::Intermediate
                | Self::Approximate
                | Self::ColorHistory
        )
    }

    fn min_height(self) -> f32 {
        match self {
            Self::CommandBar => 34.0,
            Self::Tools | Self::BrushSize => 120.0,
            Self::Color => 150.0,
            _ => 90.0,
        }
    }

    fn float_size(self) -> Vec2 {
        match self {
            Self::CommandBar => vec2(620.0, 44.0),
            Self::Tools => vec2(44.0, 420.0),
            Self::BrushSize => vec2(62.0, 360.0),
            Self::Layer => vec2(300.0, 420.0),
            Self::Timeline => vec2(640.0, 220.0),
            _ => vec2(260.0, 320.0),
        }
    }

    fn anchored(self) -> bool {
        self == Self::Tools
    }

    /// Clip Studio Paint's: the command bar across the top; the tools, Sub
    /// Tool over the color wheel and set, and the brush sizes at the left;
    /// the navigator over the layers at the right; the timeline under the
    /// canvas; the rest as icons.
    fn default_layout() -> Dock {
        let mut dock = Dock::empty();
        dock.top = vec![Group::one(Palette::CommandBar, 1.0)];
        dock.top_height = 38.0;
        dock.left = vec![
            Column::of(&[(Palette::Tools, 1.0)], 46.0),
            Column::of(
                &[
                    (Palette::SubTool, 0.44),
                    (Palette::Color, 0.3),
                    (Palette::ColorSet, 0.26),
                ],
                236.0,
            ),
            Column::of(&[(Palette::BrushSize, 1.0)], 86.0),
        ];
        dock.right = vec![Column::of(
            &[(Palette::Navigator, 0.42), (Palette::Layer, 0.58)],
            300.0,
        )];
        dock.bottom = vec![Group::one(Palette::Timeline, 1.0)];
        dock.bottom_height = 160.0;
        dock.iconized = STRIP_PALETTES.to_vec();
        dock
    }
}

/// One or two small arrows in `rect` pointing right (`point` 1) or left
/// (-1), as Clip Studio Paint's palette arrows. `true` when clicked.
fn arrows(
    ui: &mut Ui,
    rect: Rect,
    count: usize,
    point: f32,
    id: impl std::hash::Hash + std::fmt::Debug,
    tip: &str,
    theme: &Theme,
) -> bool {
    let response = ui
        .interact(rect, Id::unique(id), Sense::click())
        .on_hover_text(tip);
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme.hover);
    }
    let color = if response.hovered() {
        theme.text
    } else {
        theme.muted
    };
    let c = rect.center();
    let offsets: &[f32] = if count == 2 { &[-2.0, 2.0] } else { &[0.0] };
    for offset in offsets {
        let x = c.x + offset;
        ui.painter().add(egui::Shape::line(
            vec![
                pos2(x - 1.5 * point, c.y - 3.0),
                pos2(x + 1.5 * point, c.y),
                pos2(x - 1.5 * point, c.y + 3.0),
            ],
            Stroke::new(1.0, color),
        ));
    }
    response.clicked()
}

/// Where people grab a palette drawn in `body` to drag it: its title bar,
/// clear of its menu button. The command bar, and the tool palette along a
/// row, have a grip at their left end instead; a tool palette in a column
/// is grabbed by the bar above it (`above`, its column's), or its top.
fn title_bar(palette: Palette, body: Rect, above: Option<Rect>) -> Rect {
    if palette.sideways(body) {
        Rect::from_min_size(body.min, vec2(GRIP, body.height()))
    } else if palette.fixed_width().is_some() {
        above.unwrap_or_else(|| Rect::from_min_size(body.min, vec2(body.width(), 8.0)))
    } else {
        Rect::from_min_size(body.min, vec2((body.width() - 26.0).max(0.0), 28.0))
    }
}

/// The width of the grip at the left of a palette laid along a row.
const GRIP: f32 = 14.0;

/// The grip's dots, for a palette whose grip is at its left end.
fn grip_dots(ui: &Ui, palette: Palette, body: Rect, theme: &Theme) {
    if !palette.sideways(body) {
        return;
    }
    let center = pos2(
        body.left() + GRIP / 2.0,
        body.top() + body.height().min(38.0) / 2.0,
    );
    for step in [-6.0, -2.0, 2.0, 6.0] {
        ui.painter()
            .circle_filled(center + vec2(0.0, step), 1.2, theme.muted);
    }
}

/// The column bar right above `group`, when it is the first in a column.
fn bar_above(laid: &Laid, group: &LaidGroup<Palette>) -> Option<Rect> {
    let Holder::Column(side, column) = group.holder else {
        return None;
    };
    laid.columns
        .iter()
        .find(|(found, index, rect)| {
            *found == side
                && *index == column
                && (rect.top() + COLUMN_BAR - group.rect.top()).abs() < 0.5
        })
        .map(|(_, _, rect)| Rect::from_min_size(rect.min, vec2(rect.width(), COLUMN_BAR)))
}

impl SuePaint {
    /// Draws one palette into `rect`.
    pub(crate) fn draw_palette(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        match palette {
            Palette::CommandBar => self.command_bar(ui, rect, cx, theme),
            Palette::Tools => self.tool_strip(ui, rect, cx, theme),
            Palette::SubTool => self.tool_panel(ui, rect, cx, theme),
            Palette::BrushSize => self.size_column(ui, rect, theme),
            Palette::Color => self.color_panel(ui, rect, theme),
            Palette::ColorSet => self.color_set_panel(ui, rect, cx, theme),
            Palette::Navigator => self.navigator(ui, rect, cx, theme),
            Palette::Layer => self.layer_panel(ui, rect, cx, theme),
            Palette::Timeline => self.timeline_panel(ui, rect, cx, theme),
            other => self.single_palette(ui, other, rect, cx, theme),
        }
    }

    /// A palette's menu button, at its header's right end, and its menu.
    fn palette_menu(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        rect: Rect,
        floating: bool,
        theme: &Theme,
    ) {
        let button = if palette.sideways(rect) {
            Rect::from_min_size(
                pos2(
                    rect.right() - 22.0,
                    rect.top() + (rect.height().min(38.0) - 18.0) / 2.0,
                ),
                vec2(18.0, 18.0),
            )
        } else if palette.fixed_width().is_some() {
            Rect::from_min_size(
                pos2(rect.center().x - 9.0, rect.bottom() - 22.0),
                vec2(18.0, 18.0),
            )
        } else {
            Rect::from_min_size(
                pos2(rect.right() - 22.0, rect.top() + 3.0),
                vec2(18.0, 18.0),
            )
        };
        let open = self.dock.menu == Some(palette);
        if icon_button(
            ui,
            button,
            Icon::Menu,
            true,
            open,
            &tr!("dock-float-or-collapse", title = palette.title()),
            theme,
        ) {
            self.dock.menu = if open { None } else { Some(palette) };
        }
        if self.dock.menu != Some(palette) {
            return;
        }
        let place = self.dock.find(palette).map(|(holder, _)| holder);
        let side = self.dock.side_of(palette);
        // A window of its own needs Sue's own windows (not on a phone).
        let can_pop_out = !ui.ctx().embed_viewports();
        let popped_out = self.dock.windows.iter().any(|(found, _)| *found == palette);
        let mut chosen: Option<&str> = None;
        let shown = egui::Area::new(Id::unique(("sue-paint-dock-menu", palette.key())))
            .order(Order::Tooltip)
            .fade_in(false)
            .fixed_pos(pos2(button.right() - 170.0, button.bottom() + 2.0).max(pos2(4.0, 4.0)))
            .show(ui.ctx(), |ui| {
                sue_wm::widgets::window_frame(theme)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_min_width(158.0);
                        let mut item = |ui: &mut Ui, label: &str, key: &'static str| {
                            if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                chosen = Some(key);
                            }
                        };
                        if side != Some(Side::Left) {
                            item(ui, &tr!("dock-on-the-left"), "left");
                        }
                        item(ui, &tr!("new-column-on-the-left"), "new_left");
                        if side != Some(Side::Right) {
                            item(ui, &tr!("dock-on-the-right"), "right");
                        }
                        item(ui, &tr!("new-column-on-the-right"), "new_right");
                        if place != Some(Holder::Top) {
                            item(ui, &tr!("dock-at-the-top"), "top");
                        }
                        if place != Some(Holder::Bottom) {
                            item(ui, &tr!("dock-at-the-bottom"), "bottom");
                        }
                        if !floating {
                            item(ui, &tr!("float-in-a-window"), "float");
                        }
                        if can_pop_out && !popped_out {
                            item(ui, &tr!("pop-out-into-a-window"), "window");
                        }
                        if !self.dock.iconized.contains(&palette) {
                            item(ui, &tr!("collapse-to-an-icon"), "icon");
                        }
                        item(ui, &tr!("close"), "close");
                    });
            });
        let inside = shown.response.rect;
        let outside = ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|point| !inside.contains(point) && !button.contains(point))
        });
        let column = |side| Target::Column { side, beside: None };
        match chosen {
            Some("left") => self.dock.dock(palette, Side::Left),
            Some("right") => self.dock.dock(palette, Side::Right),
            Some("new_left") => self.dock.put(palette, column(Side::Left)),
            Some("new_right") => self.dock.put(palette, column(Side::Right)),
            Some("top") => self.dock.put(palette, Target::Top),
            Some("bottom") => self.dock.put(palette, Target::Bottom),
            Some("float") => {
                let at = rect.min + vec2(-40.0, 30.0);
                self.dock.float(palette, at.max(pos2(8.0, 40.0)));
            }
            Some("window") => {
                let at = (rect.min + vec2(32.0, 32.0)).max(pos2(24.0, 48.0));
                self.dock
                    .pop_out(palette, Rect::from_min_size(at, palette.float_size()));
            }
            Some("icon") => self.dock.iconize(palette),
            Some("close") => self.close_palette(palette),
            _ => {}
        }
        if chosen.is_some() || outside {
            self.dock.menu = None;
        }
    }

    /// Hides `palette` from the Window menu's point of view.
    pub(crate) fn close_palette(&mut self, palette: Palette) {
        let panels = &mut self.panels;
        match palette {
            Palette::CommandBar => panels.command_bar = false,
            Palette::Tools => panels.tool = false,
            Palette::SubTool => {
                panels.sub_tool = false;
                panels.tool_property = false;
            }
            Palette::BrushSize => panels.brush_size = false,
            Palette::Color => panels.color = false,
            Palette::ColorSet => panels.color_set = false,
            Palette::Navigator => panels.navigator = false,
            Palette::Layer => panels.layer = false,
            Palette::Timeline => panels.timeline = false,
            // A torn-off tab goes back to its palette's tabs.
            other if other.optional() => self.dock.remove(other),
            // The others fold back into the strip, as in Clip Studio Paint.
            other => self.dock.iconize(other),
        }
        if self.dock.popped == Some(palette) {
            self.dock.popped = None;
        }
    }

    /// The palettes, docked, collapsed and floating, around the canvas and
    /// under it. Returns what is left for the canvas.
    pub(crate) fn dock_ui(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Rect {
        let panels = self.panels;
        // A palette the Window menu shows again comes to the front of its
        // tabs.
        for palette in PALETTES {
            if palette.shown(&panels) && !palette.shown(&self.panels_seen) {
                self.dock.choose(palette);
            }
        }
        self.panels_seen = panels;
        // The command bar hides with the menus (Window, Hide Menus).
        let menus_hidden = self.menus_hidden;
        let shown = move |palette: Palette| {
            palette.shown(&panels) && !(palette == Palette::CommandBar && menus_hidden)
        };
        let laid = self.dock.layout(body, &shown, self.hide);
        for (_, _, rect) in &laid.columns {
            ui.painter().rect_filled(*rect, 0.0, theme.panel);
            let bar = Rect::from_min_size(rect.min, vec2(rect.width(), COLUMN_BAR));
            ui.painter().rect_filled(bar, 0.0, theme.void);
        }
        for group in &laid.groups {
            // The grip first, so what the palette draws over it (tabs,
            // buttons) stays on top and keeps its clicks.
            if self.ready {
                let grab = title_bar(group.active, group.body, bar_above(&laid, group));
                self.dock.grip(ui, grab, group.active, "docked");
            }
            self.draw_palette(ui, group.active, group.body, cx, theme);
            grip_dots(ui, group.active, group.body, theme);
            if group.holder.is_row() {
                ui.painter()
                    .vline(group.rect.right(), group.rect.y_range(), theme.hairline());
            }
            if group.holder != Holder::Bottom {
                ui.painter()
                    .hline(group.rect.x_range(), group.rect.bottom(), theme.hairline());
            }
            self.dock
                .tabs_ui(ui, group, &|palette: Palette| palette.title(), theme);
        }
        for (side, _, rect) in &laid.columns {
            let x = match side {
                Side::Left => rect.right(),
                Side::Right => rect.left(),
            };
            ui.painter().vline(x, rect.y_range(), theme.hairline());
        }
        if self.ready {
            for group in &laid.groups {
                self.palette_menu(ui, group.active, group.body, false, theme);
            }
            self.dock.splitters(ui, &laid, theme);
        }
        if let Some(strip) = laid.strip {
            self.strip_ui(ui, strip, Side::Right, &laid.icons, cx, theme);
        }
        if let Some(strip) = laid.left_strip {
            self.strip_ui(ui, strip, Side::Left, &laid.left_icons, cx, theme);
        }
        self.side_arrows(ui, &laid, theme);
        if self.ready {
            self.floating_ui(ui, body, &shown, cx, theme);
            self.dock
                .drop_ui(ui, &laid, &|palette: Palette| palette.title(), theme);
        }
        laid.center
    }

    /// Clip Studio Paint's arrows at the top of each side: one folds the
    /// side's palette columns into icons and opens them again, two close
    /// the whole side; a closed side leaves a thin bar whose arrows open it.
    fn side_arrows(&mut self, ui: &mut Ui, laid: &Laid, theme: &Theme) {
        for (side, handle) in &laid.handles {
            ui.painter().rect_filled(*handle, 0.0, theme.panel);
            let edge = match side {
                Side::Left => handle.right(),
                Side::Right => handle.left(),
            };
            ui.painter().vline(edge, handle.y_range(), theme.hairline());
            let top = Rect::from_min_size(handle.min, vec2(handle.width(), COLUMN_BAR + 4.0));
            // Pointing in: the side comes back.
            let point = if *side == Side::Left { 1.0 } else { -1.0 };
            let id = ("sue-paint-open-side", *side == Side::Left);
            if arrows(ui, top, 2, point, id, &tr!("show-palettes"), theme) {
                self.dock.hidden[side.index()] = false;
            }
        }
        for side in [Side::Left, Side::Right] {
            if self.dock.hidden[side.index()] {
                continue;
            }
            // The arrows sit at the outer end of the outermost bar: the
            // strip's, or the outermost column's.
            let strip = match side {
                Side::Left => laid.left_strip,
                Side::Right => laid.strip,
            };
            let outermost = laid
                .columns
                .iter()
                .filter(|(found, _, _)| *found == side)
                .map(|(_, _, rect)| *rect)
                .reduce(|a, b| match side {
                    Side::Left if b.left() < a.left() => b,
                    Side::Right if b.right() > a.right() => b,
                    _ => a,
                });
            let Some(host) = strip.or(outermost) else {
                continue;
            };
            let bar = Rect::from_min_size(host.min, vec2(host.width(), COLUMN_BAR));
            let out = if side == Side::Left { -1.0 } else { 1.0 };
            let (close_at, fold_at) = match side {
                Side::Left => (bar.left(), bar.left() + 16.0),
                Side::Right => (bar.right() - 16.0, bar.right() - 32.0),
            };
            let close = Rect::from_min_size(pos2(close_at, bar.top()), vec2(16.0, COLUMN_BAR));
            let close_id = ("sue-paint-close-side", side == Side::Left);
            if arrows(ui, close, 2, out, close_id, &tr!("hide-palettes"), theme) {
                self.dock.hidden[side.index()] = true;
                self.dock.popped = None;
            }
            let columns = match side {
                Side::Left => &self.dock.left,
                Side::Right => &self.dock.right,
            };
            let foldable = columns.iter().any(|column| {
                !column
                    .groups
                    .iter()
                    .any(|group| group.tabs.iter().any(|palette| palette.anchored()))
            });
            if !foldable {
                continue;
            }
            let folded = self.dock.folded[side.index()];
            let fold = Rect::from_min_size(pos2(fold_at, bar.top()), vec2(16.0, COLUMN_BAR));
            let (point, tip) = if folded {
                (-out, tr!("open-palettes"))
            } else {
                (out, tr!("fold-column-into-icons"))
            };
            let fold_id = ("sue-paint-fold-side", side == Side::Left);
            if arrows(ui, fold, 1, point, fold_id, &tip, theme) {
                self.dock.folded[side.index()] = !folded;
                self.dock.popped = None;
            }
        }
    }

    /// An icon strip (the right one, or the left one while the left side
    /// is folded), and the collapsed palette it shows. Icons drag out to
    /// dock or float their palettes.
    fn strip_ui(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        side: Side,
        icons_shown: &[Palette],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(strip, 0.0, theme.panel);
        ui.painter().rect_filled(
            Rect::from_min_size(strip.min, vec2(strip.width(), COLUMN_BAR)),
            0.0,
            theme.void,
        );
        let edge = match side {
            Side::Left => strip.right(),
            Side::Right => strip.left(),
        };
        ui.painter().vline(edge, strip.y_range(), theme.hairline());
        for (index, palette) in icons_shown.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(
                    strip.left() + 4.0,
                    strip.top() + COLUMN_BAR + 6.0 + index as f32 * 32.0,
                ),
                vec2(26.0, 26.0),
            );
            let open = self.dock.popped == Some(*palette);
            // The grip first, so the icon's own click stays on top of it:
            // egui drops a click when a drag-only widget is above the one
            // that senses it (`hit_test_on_close`).
            if self.ready {
                self.dock.grip(ui, rect, *palette, "icon");
            }
            if icon_button(
                ui,
                rect,
                palette.icon(),
                true,
                open,
                &palette.title(),
                theme,
            ) {
                self.dock.popped = if open { None } else { Some(*palette) };
            }
        }
        let Some(palette) = self
            .dock
            .popped
            .filter(|palette| icons_shown.contains(palette))
        else {
            self.dock.popped = None;
            return;
        };
        // The collapsed palette, beside the strip, until a click elsewhere.
        let size = match palette.fixed_width() {
            Some(width) => vec2(width + 8.0, (strip.height() * 0.7).max(240.0)),
            None => vec2(280.0, (strip.height() * 0.7).clamp(220.0, 560.0)),
        };
        let at = match side {
            Side::Left => pos2(strip.right() + 4.0, strip.top() + 4.0),
            Side::Right => pos2(strip.left() - size.x - 4.0, strip.top() + 4.0),
        };
        let area = egui::Area::new(Id::unique(("sue-paint-popped", palette.key())))
            .order(Order::Foreground)
            .fade_in(false)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter()
                    .rect_filled(rect.expand(1.0), 0.0, theme.border);
                self.dock
                    .grip(ui, title_bar(palette, rect, None), palette, "popped");
                self.draw_palette(ui, palette, rect, cx, theme);
                grip_dots(ui, palette, rect, theme);
                self.palette_menu(ui, palette, rect, false, theme);
                rect
            });
        let inside = area.inner;
        let clicked_outside = ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|point| !inside.contains(point) && !strip.contains(point))
        });
        if clicked_outside && self.dock.menu.is_none() && !self.dock.dragging() {
            self.dock.popped = None;
        }
    }

    /// Floating palettes, each in a window over the canvas people move and
    /// resize, and those popped out into windows of their own (which can
    /// go outside Sue's window; on a phone they float too). A floating
    /// palette drags out of its window by its title bar, onto a column, a
    /// row or the strip.
    fn floating_ui(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        shown: &dyn Fn(Palette) -> bool,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let floating = self
            .dock
            .floating
            .iter()
            .map(|(palette, rect)| (*palette, *rect, false));
        let windows = self
            .dock
            .windows
            .iter()
            .map(|(palette, rect)| (*palette, *rect, true));
        let all: Vec<(Palette, Rect, bool)> = floating.chain(windows).collect();
        for (palette, rect, own) in all {
            if !shown(palette) {
                continue;
            }
            if own && !ui.ctx().embed_viewports() {
                self.palette_window(ui, palette, rect, body, cx, theme);
                continue;
            }
            let mut open = true;
            let window = egui::Window::new(palette.title())
                .id(Id::unique(("sue-paint-float", palette.key())))
                .default_rect(rect)
                .constrain_to(body)
                .resizable(true)
                .collapsible(false)
                .title_bar(true)
                .min_size(vec2(
                    palette.min_width().min(180.0),
                    palette.min_height().min(120.0),
                ))
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    let area = ui.available_rect_before_wrap();
                    let area = Rect::from_min_size(area.min, area.size().max(vec2(40.0, 34.0)));
                    ui.allocate_rect(area, Sense::hover());
                    self.dock
                        .grip(ui, title_bar(palette, area, None), palette, "floating");
                    self.draw_palette(ui, palette, area, cx, theme);
                    grip_dots(ui, palette, area, theme);
                    // Back into a column, from inside its window.
                    let back = Rect::from_min_size(
                        pos2(area.right() - 22.0, area.top() + 3.0),
                        vec2(18.0, 18.0),
                    );
                    if icon_button(
                        ui,
                        back,
                        Icon::Up,
                        true,
                        false,
                        &tr!("dock-it-again"),
                        theme,
                    ) {
                        Some(true)
                    } else {
                        None
                    }
                });
            if let Some(inner) = &window {
                self.keep_window(palette, inner.response.rect);
            }
            let redock = window
                .as_ref()
                .and_then(|inner| inner.inner.flatten())
                .is_some();
            if !open || redock {
                self.redock(palette, rect, body);
            }
        }
    }

    /// Keeps where a floating or popped-out `palette`'s window is now.
    fn keep_window(&mut self, palette: Palette, rect: Rect) {
        if let Some(entry) = self
            .dock
            .floating
            .iter_mut()
            .chain(self.dock.windows.iter_mut())
            .find(|(found, _)| *found == palette)
        {
            entry.1 = rect;
        }
    }

    /// Docks a floating `palette` again on the side its window is nearer
    /// (the command bar at the top, the timeline at the bottom).
    fn redock(&mut self, palette: Palette, rect: Rect, body: Rect) {
        let side = if rect.center().x < body.center().x {
            Side::Left
        } else {
            Side::Right
        };
        match palette {
            Palette::CommandBar => self.dock.put(palette, Target::Top),
            Palette::Timeline => self.dock.put(palette, Target::Bottom),
            _ => self.dock.dock(palette, side),
        }
    }

    /// A floating palette in a window of its own, as Clip Studio Paint's:
    /// it can go anywhere, outside Sue's window too, and be resized. Its
    /// place is kept relative to Sue's window.
    fn palette_window(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        rect: Rect,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let ctx = ui.ctx().clone();
        let main = ctx
            .input(|input| input.viewport().inner_rect)
            .map_or(Pos2::ZERO, |inner| inner.min);
        let own_frame = sue_wm::widgets::own_frames(&ctx);
        let bar = if own_frame {
            sue_wm::widgets::TITLE_BAR
        } else {
            0.0
        };
        let builder = egui::ViewportBuilder::default()
            .with_title(palette.title())
            .with_position(main + rect.min.to_vec2())
            .with_inner_size(rect.size() + vec2(0.0, bar))
            .with_min_inner_size(vec2(palette.fixed_width().unwrap_or(160.0), 100.0 + bar))
            .with_resizable(true)
            .with_decorations(!own_frame);
        let (closed, redock, now) = ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of(("sue-paint-palette", palette.key())),
            builder,
            |ui, _| {
                let whole = ui.max_rect();
                let mut area = whole;
                let mut shut = false;
                if own_frame {
                    // Sue's own title bar, as Clip Studio Paint's.
                    let top = Rect::from_min_size(whole.min, vec2(whole.width(), bar));
                    shut = sue_wm::widgets::title_bar(ui, top, &palette.title(), false, theme);
                    area.min.y = top.bottom();
                }
                self.draw_palette(ui, palette, area, cx, theme);
                // Back into a column, from inside its window.
                let back = Rect::from_min_size(
                    pos2(area.right() - 22.0, area.bottom() - 22.0),
                    vec2(18.0, 18.0),
                );
                let redock = icon_button(
                    ui,
                    back,
                    Icon::Up,
                    true,
                    false,
                    &tr!("dock-it-again"),
                    theme,
                );
                if own_frame {
                    sue_wm::widgets::resize_edges(ui, whole);
                }
                let (closed, now) = ui.input(|input| {
                    let viewport = input.viewport();
                    (viewport.close_requested(), viewport.inner_rect)
                });
                (closed || shut, redock, now)
            },
        );
        if let Some(now) = now {
            self.keep_window(palette, now.translate(-main.to_vec2()));
        }
        if closed || redock {
            self.redock(palette, rect, body);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_wm::dock::{STRIP, Target};

    fn all(_: Palette) -> bool {
        true
    }

    fn body() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))
    }

    #[test]
    fn the_default_layout_is_like_a_painting_apps() {
        let dock = Dock::default();
        let laid = dock.layout(body(), &all, 0.0);
        assert_eq!(
            laid.groups.len(),
            9,
            "the command bar, seven palettes and the timeline"
        );
        // The command bar across the top, over the columns.
        let bar = laid.top.unwrap();
        assert_eq!((bar.width(), bar.height()), (1600.0, 38.0));
        assert_eq!(laid.center.top(), 38.0);
        assert!(
            laid.strip.is_some(),
            "the folded palettes, as Clip Studio Paint"
        );
        assert_eq!(laid.icons.len(), STRIP_PALETTES.len());
        assert_eq!(laid.center.left(), 46.0 + 236.0 + 86.0);
        assert_eq!(laid.center.right(), 1600.0 - 300.0 - STRIP);
        // The timeline under the canvas, between the columns.
        let timeline = laid.bottom.unwrap();
        assert_eq!(timeline.x_range(), laid.center.x_range());
        assert_eq!(timeline.height(), 160.0);
        // Heights share the column without gaps.
        let place = |palette| {
            laid.docked()
                .find(|(found, _)| *found == palette)
                .unwrap()
                .1
        };
        assert_eq!(place(Palette::SubTool).top(), 38.0 + COLUMN_BAR);
        assert_eq!(place(Palette::CommandBar), bar);
        assert!((place(Palette::ColorSet).bottom() - 900.0).abs() < 0.5);
        assert_eq!(laid.splits.len(), 3);
        assert_eq!(
            laid.edges.len(),
            4,
            "every column resizes, the tools' and the sliders' too"
        );
        // The torn-off palettes are nowhere until people tear them off.
        for palette in PALETTES.into_iter().filter(|palette| palette.optional()) {
            assert!(!dock.has(palette), "{palette:?}");
        }
    }

    #[test]
    fn the_command_bar_and_tools_move_and_the_tools_widen() {
        let mut dock = Dock::default();
        // The command bar into a column, and back to the top.
        dock.dock(Palette::CommandBar, Side::Right);
        assert_eq!(dock.side_of(Palette::CommandBar), Some(Side::Right));
        let laid = dock.layout(body(), &all, 0.0);
        assert!(laid.top.is_none(), "an empty top row leaves the room");
        dock.put(Palette::CommandBar, Target::Top);
        assert_eq!(dock.find(Palette::CommandBar), Some((Holder::Top, 0)));
        assert_eq!(dock.top_height, 38.0);
        // The tool palette's column widens into several columns of tools.
        dock.set_column_width(Side::Left, 0, 120.0);
        let laid = dock.layout(body(), &all, 0.0);
        let tools = laid
            .docked()
            .find(|(palette, _)| *palette == Palette::Tools)
            .unwrap()
            .1;
        assert_eq!(tools.width(), 120.0);
        // Along the bottom row it lies on its side, gripped at its left.
        dock.put(Palette::Tools, Target::Bottom);
        let laid = dock.layout(body(), &all, 0.0);
        let tools = laid
            .docked()
            .find(|(palette, _)| *palette == Palette::Tools)
            .unwrap()
            .1;
        assert!(Palette::Tools.sideways(tools));
        assert_eq!(title_bar(Palette::Tools, tools, None).width(), GRIP);
        let back = Dock::from_lines(&dock.to_lines()).unwrap();
        assert_eq!(back, dock);
    }

    #[test]
    fn tabs_tear_off_into_palettes_of_their_own_and_go_back() {
        let mut dock = Dock::default();
        dock.put(
            Palette::ColorSlider,
            Target::Beside {
                anchor: Palette::Layer,
                after: true,
            },
        );
        assert!(dock.is_open(Palette::ColorSlider));
        assert_eq!(dock.side_of(Palette::ColorSlider), Some(Side::Right));
        let back = Dock::from_lines(&dock.to_lines()).unwrap();
        assert_eq!(back, dock, "kept by key in the layout");
        // Popped out into a window of its own, kept too.
        dock.pop_out(
            Palette::Intermediate,
            Rect::from_min_size(pos2(-100.0, 80.0), vec2(260.0, 300.0)),
        );
        let back = Dock::from_lines(&dock.to_lines()).unwrap();
        assert_eq!(back.windows, dock.windows);
        // Closed, it is nowhere again (back among its palette's tabs).
        dock.remove(Palette::ColorSlider);
        dock.fill_missing();
        assert!(!dock.has(Palette::ColorSlider));
    }

    #[test]
    fn layouts_kept_before_the_command_bar_moved_still_open() {
        // Lines as Sue Paint kept them before the top row (layout.txt and
        // workspaces of the docking before this one).
        let old: Vec<String> = [
            "column left 46 1:tool",
            "column left 236 0.44:sub_tool 0.3:color 0.26:color_set",
            "column left 86 1:brush_size",
            "column right 320 0.42:navigator 0.58:layer+history@1",
            "bottom 200 1:timeline",
            "icon material",
            "fold right",
        ]
        .map(String::from)
        .to_vec();
        let read = Dock::from_lines(&old).unwrap();
        assert_eq!(read.find(Palette::CommandBar), Some((Holder::Top, 0)));
        assert_eq!(read.top_height, 38.0);
        assert_eq!(read.right[0].width, 320.0);
        assert_eq!(
            read.right[0].groups[1].tabs,
            [Palette::Layer, Palette::History]
        );
        assert_eq!(read.bottom_height, 200.0);
        assert!(read.folded[1]);
        assert!(read.windows.is_empty());
    }

    #[test]
    fn sides_fold_into_icons_and_close_and_come_back() {
        let mut dock = Dock::default();
        let open = dock.layout(body(), &all, 0.0);
        dock.folded[1] = true;
        let laid = dock.layout(body(), &all, 0.0);
        assert!(laid.icons.contains(&Palette::Navigator));
        assert!(!laid.docked().any(|(p, _)| p == Palette::Layer));
        dock.folded[0] = true;
        let laid = dock.layout(body(), &all, 0.0);
        assert!(
            laid.docked().any(|(p, _)| p == Palette::Tools),
            "the tools stay"
        );
        assert!(laid.left_icons.contains(&Palette::SubTool));
        dock.hidden = [true; 2];
        let back = Dock::from_lines(&dock.to_lines()).unwrap();
        assert_eq!((back.folded, back.hidden), ([true; 2], [true; 2]));
        dock.hidden = [false; 2];
        dock.folded = [false; 2];
        assert_eq!(dock.layout(body(), &all, 0.0), open);
    }

    #[test]
    fn palettes_tab_float_collapse_dock_and_come_back() {
        let mut dock = Dock::default();
        dock.put(
            Palette::Color,
            Target::Join {
                anchor: Palette::Layer,
                before: false,
            },
        );
        assert_eq!(dock.side_of(Palette::Color), Some(Side::Right));
        dock.put(Palette::Navigator, Target::Bottom);
        assert_eq!(dock.find(Palette::Navigator), Some((Holder::Bottom, 1)));
        dock.float(Palette::Layer, pos2(100.0, 100.0));
        assert_eq!(dock.side_of(Palette::Layer), None);
        dock.iconize(Palette::ColorSet);
        let laid = dock.layout(body(), &all, 0.0);
        assert_eq!(laid.icons.last(), Some(&Palette::ColorSet));
        dock.dock(Palette::BrushSize, Side::Right);
        assert!(
            dock.right
                .iter()
                .any(|column| column.groups == [Group::one(Palette::BrushSize, 1.0)])
        );
        // Kept as lines, by key, and read back the same.
        let back = Dock::from_lines(&dock.to_lines()).unwrap();
        assert_eq!(back, dock);
        // Reset.
        dock = Dock::default();
        assert_eq!(dock, Palette::default_layout());
    }

    #[test]
    fn workspaces_saved_before_tabs_still_open() {
        // Lines as Sue Paint wrote them before, palettes named by title.
        let old = [
            format!("left 46 {}=1", Palette::Tools.title()),
            format!(
                "left 236 {}=0.44,{}=0.3,{}=0.26",
                Palette::SubTool.title(),
                Palette::Color.title(),
                Palette::ColorSet.title()
            ),
            format!("left 86 {}=1", Palette::BrushSize.title()),
            format!(
                "right 300 {}=0.42,{}=0.58",
                Palette::Navigator.title(),
                Palette::Layer.title()
            ),
            "timeline 210".to_owned(),
        ];
        let mut old = old.to_vec();
        old.extend(
            STRIP_PALETTES
                .iter()
                .map(|palette| format!("icon {}", palette.title())),
        );
        let read = Dock::from_lines(&old).unwrap();
        let mut expected = Dock::default();
        expected.bottom_height = 210.0;
        assert_eq!(read, expected);
    }
}
