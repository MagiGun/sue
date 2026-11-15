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
use sue_wm::dock::{COLUMN_BAR, Column, DockPanel, Group, Holder, LaidGroup, LayoutStyle, Target};
use sue_wm::{AppletContext, Theme};

use crate::icons::Icon;
use crate::panels::icon_button;
use crate::studio::{Panels, SuePaint};

use sue_wm::dock::Row;
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
    /// Quick Access: sets of buttons with their names.
    QuickAccess,
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
    /// Sub Tool Detail: every setting of the picked sub tool.
    SubToolDetail,
}

/// The palettes the "Clip Studio" style keeps folded in the strip at the
/// right from the start: Clip Studio Paint's Material, then the ones it
/// does not show at first (Animation Cels, and Sue's own Tool Sliders).
pub(crate) const STRIP_PALETTES: [Palette; 4] = [
    Palette::QuickAccess,
    Palette::Material,
    Palette::Cels,
    Palette::BrushSize,
];

/// The six color palettes, tabs of one group in the "Clip Studio" style.
pub(crate) const COLOR_PALETTES: [Palette; 6] = [
    Palette::Color,
    Palette::ColorSlider,
    Palette::ColorSet,
    Palette::Intermediate,
    Palette::Approximate,
    Palette::ColorHistory,
];

/// A group of `tabs`, the first one showing.
fn tabs(tabs: &[Palette], weight: f32) -> Group<Palette> {
    Group {
        tabs: tabs.to_vec(),
        active: 0,
        weight,
    }
}

/// The "Clip Studio" layout style's arrangement, Clip Studio Paint's as
/// the tutorials show it: the command bar across the top; at the left the
/// Tool palette, then one column of Sub Tool, Tool Property, Brush Size
/// and the six color palettes as tabs; at the right Navigator with Sub
/// View, Item Bank and Information as tabs, Layer Property, and Layer
/// with History and Auto Action; the Timeline folded to an icon under
/// the canvas; Material and the rest as icons in the strip.
#[must_use]
pub(crate) fn classic_layout() -> Dock {
    let mut dock = Dock::empty();
    dock.top = vec![Group::one(Palette::CommandBar, 1.0)];
    dock.top_height = 38.0;
    dock.left = vec![
        Column::of(&[(Palette::Tools, 1.0)], 46.0),
        Column {
            groups: vec![
                Group::one(Palette::SubTool, 0.24),
                Group::one(Palette::ToolProperty, 0.3),
                Group::one(Palette::BrushGrid, 0.16),
                tabs(&COLOR_PALETTES, 0.3),
            ],
            // Wide enough for the six color tabs; the canvas starts where
            // it did before the styles.
            width: 322.0,
        },
    ];
    dock.right = vec![Column {
        groups: vec![
            tabs(
                &[
                    Palette::Navigator,
                    Palette::SubView,
                    Palette::ItemBank,
                    Palette::Info,
                ],
                0.3,
            ),
            Group::one(Palette::LayerProperty, 0.2),
            tabs(
                &[Palette::Layer, Palette::History, Palette::AutoAction],
                0.5,
            ),
        ],
        width: 300.0,
    }];
    dock.bottom = vec![Group::one(Palette::Timeline, 1.0)];
    dock.bottom_height = 160.0;
    dock.row_folded[sue_wm::dock::Row::Bottom.index()] = true;
    dock.iconized = STRIP_PALETTES.to_vec();
    dock
}

/// Every palette.
const PALETTES: [Palette; 25] = [
    Palette::CommandBar,
    Palette::QuickAccess,
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
    Palette::SubToolDetail,
];

impl Palette {
    /// What its window and icon are called.
    pub fn title(self) -> String {
        match self {
            Self::CommandBar => tr!("command-bar"),
            Self::QuickAccess => tr!("quick-access"),
            Self::Tools => tr!("tool"),
            Self::SubTool => tr!("sub-tool"),
            Self::BrushSize => tr!("tool-sliders"),
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
            Self::SubToolDetail => tr!("sub-tool-detail"),
        }
    }

    /// Its icon in the strip.
    pub fn icon(self) -> Icon {
        match self {
            Self::CommandBar => Icon::Menu,
            Self::QuickAccess => Icon::List,
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
            Self::SubToolDetail => Icon::Wrench,
        }
    }

    /// `true` when the Window menu shows it.
    pub fn shown(self, panels: &Panels) -> bool {
        match self {
            Self::CommandBar => panels.command_bar,
            Self::Tools => panels.tool,
            Self::SubTool => panels.sub_tool,
            Self::ToolProperty => panels.tool_property,
            Self::BrushSize => panels.brush_size,
            Self::Color => panels.color,
            Self::ColorSet => panels.color_set,
            Self::Navigator => panels.navigator,
            Self::Layer => panels.layer,
            Self::Timeline => panels.timeline,
            Self::QuickAccess
            | Self::Material
            | Self::LayerProperty
            | Self::History
            | Self::Cels
            | Self::SubView
            | Self::AutoAction
            | Self::ItemBank
            | Self::Info
            | Self::BrushGrid
            | Self::ColorSlider
            | Self::Intermediate
            | Self::Approximate
            | Self::ColorHistory
            | Self::SubToolDetail => true,
        }
    }

    /// The palette a Window menu command shows, or hides when it shows:
    /// every palette is one of its own, opened as itself.
    pub fn of_window_command(command: &str) -> Option<Self> {
        Some(match command {
            "window.command_bar" => Self::CommandBar,
            "window.quick_access" => Self::QuickAccess,
            "window.tool" => Self::Tools,
            "window.sub_tool" => Self::SubTool,
            "window.tool_property" => Self::ToolProperty,
            "window.brush_size" => Self::BrushSize,
            "window.color_wheel" => Self::Color,
            "window.color_set" => Self::ColorSet,
            "window.navigator" => Self::Navigator,
            "window.layer" => Self::Layer,
            "window.timeline" => Self::Timeline,
            "window.brush_grid" => Self::BrushGrid,
            "window.color_slider" => Self::ColorSlider,
            "window.intermediate" => Self::Intermediate,
            "window.approximate" => Self::Approximate,
            "window.color_history" => Self::ColorHistory,
            "window.sub_tool_detail" => Self::SubToolDetail,
            "window.material" => Self::Material,
            "window.sub_view" => Self::SubView,
            "window.layer_property" => Self::LayerProperty,
            "window.history" => Self::History,
            "window.cels" => Self::Cels,
            "window.auto_actions" => Self::AutoAction,
            "window.item_bank" => Self::ItemBank,
            "window.info" => Self::Info,
            _ => return None,
        })
    }

    /// The palette's own commands, at the top of its menu (the menu at the
    /// top left of its header), before where it docks and "Hide X
    /// Palette". Each palette's owner fills in its own list.
    pub fn menu_commands(self) -> &'static [&'static str] {
        match self {
            Self::Layer => &[
                "layer.new",
                "layer.new.vector",
                "layer.new.folder",
                "layer.duplicate",
                "layer.delete",
                "layer.rename",
                "layer.properties",
            ],
            Self::CommandBar => &["file.command_bar"],
            Self::QuickAccess => &crate::panels::QUICK_ACCESS_COMMANDS,
            Self::Timeline => &[
                "anim.timeline.edit",
                "anim.onion",
                "anim.onion_settings",
                "anim.loop",
            ],
            Self::Navigator => &crate::panels::NAVIGATOR_COMMANDS,
            Self::SubView => &crate::panels::SUB_VIEW_COMMANDS,
            Self::Info => &crate::panels::INFO_COMMANDS,
            Self::ItemBank => &crate::panels::ITEM_BANK_COMMANDS,
            Self::AutoAction => &crate::panels::AUTO_ACTION_COMMANDS,
            Self::Material => &crate::panels::MATERIAL_COMMANDS,
            Self::Tools => crate::panels::TOOL_MENU,
            Self::SubTool => crate::panels::SUB_TOOL_MENU,
            Self::ToolProperty | Self::SubToolDetail => crate::panels::PROPERTY_MENU,
            _ => &[],
        }
    }

    /// The palette whose tab this one was in Sue Paint 0.0.75, before
    /// every palette was one of its own; a layout kept then that lacks it
    /// gets it back as a tab beside its host, where it showed.
    pub fn old_host(self) -> Option<Self> {
        match self {
            Self::ToolProperty | Self::BrushGrid => Some(Self::SubTool),
            Self::ColorSlider | Self::Intermediate | Self::Approximate => Some(Self::Color),
            Self::ColorHistory => Some(Self::ColorSet),
            _ => None,
        }
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
            Self::QuickAccess => "quick_access",
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
            Self::SubToolDetail => "sub_tool_detail",
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

    /// Sub Tool Detail is nowhere until it is opened (Tool Property's
    /// wrench, Window), and then floats beside the palettes.
    fn optional(self) -> bool {
        self == Self::SubToolDetail
    }

    fn min_width(self) -> f32 {
        match self {
            Self::Tools => 38.0,
            Self::CommandBar => 46.0,
            Self::BrushSize => 56.0,
            _ => 170.0,
        }
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
            Self::SubToolDetail => vec2(640.0, 540.0),
            _ => vec2(260.0, 320.0),
        }
    }

    fn anchored(self) -> bool {
        self == Self::Tools
    }

    /// Every layout style, the default ("Clip Studio") first.
    fn styles() -> &'static [LayoutStyle<Self>] {
        &crate::layout_style::STYLES
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

/// One or two small arrows in `rect` pointing down (`down`) or up, as
/// Clip Studio Paint's arrows of a row. `true` when clicked.
fn vertical_arrows(
    ui: &mut Ui,
    rect: Rect,
    count: usize,
    down: bool,
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
    let point = if down { 1.0 } else { -1.0 };
    let c = rect.center();
    let offsets: &[f32] = if count == 2 { &[-2.0, 2.0] } else { &[0.0] };
    for offset in offsets {
        let y = c.y + offset;
        ui.painter().add(egui::Shape::line(
            vec![
                pos2(c.x - 3.0, y - 1.5 * point),
                pos2(c.x, y + 1.5 * point),
                pos2(c.x + 3.0, y - 1.5 * point),
            ],
            Stroke::new(1.0, color),
        ));
    }
    response.clicked()
}

/// Where the strip's right-click menu is open (the clicked icon's corner).
const STRIP_MENU: &str = "sue-paint-strip-menu";

/// Where people grab a palette drawn in `body` to drag it: its title bar,
/// clear of its menu button at the left. The command bar, and the tool palette along a
/// row, have a grip at their left end instead; a tool palette in a column
/// is grabbed by the bar above it (`above`, its column's), or its top.
fn title_bar(palette: Palette, body: Rect, above: Option<Rect>) -> Rect {
    if palette.sideways(body) {
        Rect::from_min_size(body.min, vec2(GRIP, body.height()))
    } else if palette.fixed_width().is_some() {
        above.unwrap_or_else(|| Rect::from_min_size(body.min, vec2(body.width(), 8.0)))
    } else {
        Rect::from_min_size(
            body.min + vec2(26.0, 0.0),
            vec2((body.width() - 26.0).max(0.0), 28.0),
        )
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
        // Its header leaves room at the left for its menu button.
        let inset = Id::unique(crate::panels::MENU_INSET);
        ui.ctx().data_mut(|data| data.insert_temp(inset, 26.0_f32));
        match palette {
            Palette::CommandBar => self.command_bar(ui, rect, cx, theme),
            Palette::QuickAccess => self.quick_access_panel(ui, rect, cx, theme),
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
        ui.ctx().data_mut(|data| data.remove::<f32>(inset));
    }

    /// A palette's menu button, at the top left of its header as in Clip
    /// Studio Paint, and its menu: the palette's own commands
    /// ([`Palette::menu_commands`]), where it docks and floats, and
    /// "Hide X Palette" last.
    fn palette_menu(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        rect: Rect,
        cx: &mut AppletContext<'_>,
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
            Rect::from_min_size(pos2(rect.left() + 4.0, rect.top() + 5.0), vec2(18.0, 18.0))
        };
        let open = self.dock.menu == Some(palette);
        if icon_button(
            ui,
            button,
            Icon::Menu,
            true,
            open,
            &tr!("palette-menu-of", title = palette.title()),
            theme,
        ) {
            self.dock.menu = if open { None } else { Some(palette) };
        }
        if self.dock.menu != Some(palette) {
            return;
        }
        let place = self.dock.find(palette).map(|(holder, _)| holder);
        let side = self.dock.side_of(palette);
        let floating = self
            .dock
            .floating
            .iter()
            .any(|(found, _)| *found == palette);
        // A window of its own needs Sue's own windows (not on a phone).
        let can_pop_out = !ui.ctx().embed_viewports();
        let popped_out = self.dock.windows.iter().any(|(found, _)| *found == palette);
        let registry = crate::commands::registry();
        let facts = sue_wm::Facts::of(cx);
        let own: Vec<(String, &'static str, sue_wm::ItemState)> = palette
            .menu_commands()
            .iter()
            .map(|command| {
                let label = registry
                    .label(command)
                    .unwrap_or_else(|| (*command).to_owned());
                (label, *command, self.state(command, &facts))
            })
            .collect();
        let mut chosen: Option<&str> = None;
        let mut run: Option<&'static str> = None;
        let at = if button.left() < rect.center().x {
            pos2(button.left(), button.bottom() + 2.0)
        } else {
            pos2(button.right() - 190.0, button.bottom() + 2.0)
        };
        let shown = egui::Area::new(Id::unique(("sue-paint-dock-menu", palette.key())))
            .order(Order::Tooltip)
            .fade_in(false)
            .fixed_pos(at.max(pos2(4.0, 4.0)))
            .show(ui.ctx(), |ui| {
                sue_wm::widgets::window_frame(theme)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_min_width(178.0);
                        for (label, command, state) in &own {
                            let entry = egui::Button::new(label.as_str())
                                .selected(state.checked == Some(true))
                                .frame(false);
                            if ui.add_enabled(state.enabled, entry).clicked() {
                                run = Some(command);
                            }
                        }
                        if !own.is_empty() {
                            ui.separator();
                        }
                        ui.label(
                            egui::RichText::new(tr!("palette-docking"))
                                .size(theme.small_size)
                                .color(theme.muted),
                        );
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
                        ui.separator();
                        item(ui, &tr!("hide-palette", title = palette.title()), "close");
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
        if chosen.is_some() || run.is_some() || outside {
            self.dock.menu = None;
        }
        if let Some(command) = run {
            self.run(command, cx);
        }
    }

    /// The arrangement of the layout style in use: where a palette goes
    /// back to when it is shown again, and what Reset to Default makes.
    pub(crate) fn style_layout(&self) -> Dock {
        Palette::style_layout(self.layout_style)
    }

    /// Hides `palette` for good, as its Close does in Clip Studio Paint,
    /// floating, docked or collapsed: only the Window menu shows it again.
    /// A palette with a switch of its own in [`Panels`] keeps its place;
    /// any other leaves the layout and comes back where the style has it.
    pub(crate) fn close_palette(&mut self, palette: Palette) {
        let panels = &mut self.panels;
        match palette {
            Palette::CommandBar => panels.command_bar = false,
            Palette::Tools => panels.tool = false,
            Palette::SubTool => panels.sub_tool = false,
            Palette::ToolProperty => panels.tool_property = false,
            Palette::BrushSize => panels.brush_size = false,
            Palette::Color => panels.color = false,
            Palette::ColorSet => panels.color_set = false,
            Palette::Navigator => panels.navigator = false,
            Palette::Layer => panels.layer = false,
            Palette::Timeline => panels.timeline = false,
            other => self.dock.close(other),
        }
        if self.dock.popped == Some(palette) {
            self.dock.popped = None;
        }
        if self.dock.menu == Some(palette) {
            self.dock.menu = None;
        }
    }

    /// `true` while `palette` shows: its switch is on and it is open
    /// (docked in a row or column that is not folded, floating, in a
    /// window) or opened beside the strip. Only an icon, it is not.
    pub(crate) fn palette_showing(&self, palette: Palette) -> bool {
        let folded_row = match self.dock.find(palette) {
            Some((Holder::Top, _)) => self.dock.row_folded[Row::Top.index()],
            Some((Holder::Bottom, _)) => self.dock.row_folded[Row::Bottom.index()],
            _ => false,
        };
        palette.shown(&self.panels)
            && ((self.dock.is_open(palette) && !folded_row) || self.dock.popped == Some(palette))
    }

    /// Shows `palette` as the Window menu does: its switch on, at the
    /// front of its tabs, open beside the strip if it is an icon there;
    /// a palette the layout lacks (closed for good) goes back where the
    /// style in use has it.
    pub(crate) fn show_palette(&mut self, palette: Palette) {
        // A palette shown while all are hidden: the rest come back too.
        if self.hidden_all.is_some() {
            self.toggle_hide_all();
        }
        self.set_switch(palette, true);
        if !self.dock.has(palette) {
            self.place_as_style(palette);
        }
        self.dock.choose(palette);
        if self.dock.iconized.contains(&palette) {
            self.dock.popped = Some(palette);
        }
        // In a row folded into icons: the row opens.
        match self.dock.find(palette) {
            Some((Holder::Top, _)) => self.dock.row_folded[Row::Top.index()] = false,
            Some((Holder::Bottom, _)) => self.dock.row_folded[Row::Bottom.index()] = false,
            _ => {}
        }
    }

    /// A layout from its kept lines, palettes the lines lack placed as
    /// `model` (the style in use) has them; a palette that was a tab of
    /// another in Sue Paint 0.0.75 goes back beside that palette instead
    /// ([`Self::rejoin_old_tabs`]), so a layout kept then looks as it did.
    pub(crate) fn read_layout(lines: &[String], model: &Dock) -> Option<Dock> {
        let mut bare = model.clone();
        for palette in PALETTES {
            if palette.old_host().is_some() {
                bare.close(palette);
            }
        }
        let mut dock = Dock::from_lines_with(lines, &bare)?;
        Self::rejoin_old_tabs(&mut dock);
        dock.fill_missing_from(model);
        Some(dock)
    }

    /// Puts every palette `dock` lacks (and did not close for good) that
    /// was a tab of another palette in Sue Paint 0.0.75 back as a tab of
    /// that palette (or into its icon's group).
    fn rejoin_old_tabs(dock: &mut Dock) {
        for palette in PALETTES {
            let Some(host) = palette.old_host() else {
                continue;
            };
            if dock.has(palette) || dock.is_closed(palette) {
                continue;
            }
            if dock.find(host).is_some() {
                dock.put(
                    palette,
                    Target::Join {
                        anchor: host,
                        before: false,
                    },
                );
                dock.choose(host);
            } else if dock.iconized.contains(&host) {
                dock.join_icon(palette, host);
            }
        }
    }

    /// Hide All Palettes, and Show All Palettes while they are hidden:
    /// hiding keeps the palettes and the dock exactly as they are and
    /// leaves only the command bar; showing puts that back.
    pub(crate) fn toggle_hide_all(&mut self) {
        if let Some((panels, dock)) = self.hidden_all.take() {
            self.panels = panels;
            self.dock = dock;
            // Shown again: nothing counts as newly shown.
            self.panels_seen = panels;
            return;
        }
        self.hidden_all = Some((self.panels, self.dock.clone()));
        let command_bar = self.panels.command_bar;
        self.panels = Panels {
            command_bar,
            tool: false,
            sub_tool: false,
            tool_property: false,
            brush_size: false,
            color: false,
            color_set: false,
            navigator: false,
            layer: false,
            timeline: false,
        };
        for palette in PALETTES {
            if palette != Palette::CommandBar {
                self.dock.remove(palette);
            }
        }
        self.dock.popped = None;
        self.dock.menu = None;
    }

    /// Shows `palette`, or hides it for good when it shows.
    pub(crate) fn toggle_palette(&mut self, palette: Palette) {
        if self.palette_showing(palette) {
            self.close_palette(palette);
        } else {
            self.show_palette(palette);
        }
    }

    /// Turns `palette`'s switch in [`Panels`] on or off, if it has one.
    fn set_switch(&mut self, palette: Palette, on: bool) {
        let panels = &mut self.panels;
        match palette {
            Palette::CommandBar => panels.command_bar = on,
            Palette::Tools => panels.tool = on,
            Palette::SubTool => panels.sub_tool = on,
            Palette::ToolProperty => panels.tool_property = on,
            Palette::BrushSize => panels.brush_size = on,
            Palette::Color => panels.color = on,
            Palette::ColorSet => panels.color_set = on,
            Palette::Navigator => panels.navigator = on,
            Palette::Layer => panels.layer = on,
            Palette::Timeline => panels.timeline = on,
            _ => {}
        }
    }

    /// Puts `palette`, which the layout lacks, where the style in use has
    /// it: as a tab of a palette its group there shares, else in a column
    /// on the same side, the same row, floating where it floats, or as an
    /// icon; a palette the style does not place floats over the canvas.
    fn place_as_style(&mut self, palette: Palette) {
        let model = self.style_layout();
        self.dock.remove(palette);
        let mates: Vec<Palette> = match model.find(palette) {
            Some((holder, index)) => {
                let groups = match holder {
                    Holder::Column(side, column) => {
                        let columns = match side {
                            Side::Left => &model.left,
                            Side::Right => &model.right,
                        };
                        columns.get(column).map(|column| &column.groups)
                    }
                    Holder::Top => Some(&model.top),
                    Holder::Bottom => Some(&model.bottom),
                };
                groups
                    .and_then(|groups| groups.get(index))
                    .map(|group| group.tabs.clone())
                    .unwrap_or_default()
            }
            None => Vec::new(),
        };
        if let Some(anchor) = mates
            .iter()
            .copied()
            .find(|mate| *mate != palette && self.dock.find(*mate).is_some())
        {
            self.dock.put(
                palette,
                Target::Join {
                    anchor,
                    before: false,
                },
            );
            return;
        }
        match model.find(palette) {
            Some((Holder::Column(side, _), _)) => self.dock.dock(palette, side),
            Some((Holder::Top, _)) => self.dock.put(palette, Target::Top),
            Some((Holder::Bottom, _)) => self.dock.put(palette, Target::Bottom),
            None if model.iconized.contains(&palette) => self.dock.iconize(palette),
            None => {
                let at = model
                    .floating
                    .iter()
                    .find(|(found, _)| *found == palette)
                    .map_or(pos2(360.0, 160.0), |(_, rect)| rect.min);
                self.dock.float(palette, at);
            }
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
                self.palette_menu(ui, group.active, group.body, cx, theme);
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
        self.row_strips(ui, &laid, theme);
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
            // Right-click: Display palette as tab, or as pop-up.
            let right_clicked = ui.input(|input| {
                input.pointer.secondary_clicked()
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|point| rect.contains(point))
            });
            if right_clicked {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(Id::unique(STRIP_MENU), rect.left_bottom());
                });
            }
        }
        self.strip_mode_menu(ui, theme);
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
                self.palette_menu(ui, palette, rect, cx, theme);
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
        // Displayed as a tab, it stays until its icon is clicked again.
        if clicked_outside
            && !self.strip_as_tab
            && self.dock.menu.is_none()
            && !self.dock.dragging()
        {
            self.dock.popped = None;
        }
    }

    /// The menu a right-click on a strip icon opens: Display palette as
    /// tab (it stays open beside the strip) or as pop-up (a click
    /// elsewhere closes it), for every icon of the strip.
    fn strip_mode_menu(&mut self, ui: &mut Ui, theme: &Theme) {
        let id = Id::unique(STRIP_MENU);
        let Some(at) = ui.ctx().data(|data| data.get_temp::<Pos2>(id)) else {
            return;
        };
        let mut chosen = None;
        let shown = egui::Area::new(Id::unique("sue-paint-strip-menu"))
            .order(Order::Tooltip)
            .fade_in(false)
            .fixed_pos(pos2(at.x - 200.0, at.y).max(pos2(4.0, 4.0)))
            .show(ui.ctx(), |ui| {
                sue_wm::widgets::window_frame(theme)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_min_width(190.0);
                        for (label, as_tab) in [
                            (tr!("display-palette-as-tab"), true),
                            (tr!("display-palette-as-pop-up"), false),
                        ] {
                            let entry = egui::Button::new(label)
                                .selected(self.strip_as_tab == as_tab)
                                .frame(false);
                            if ui.add(entry).clicked() {
                                chosen = Some(as_tab);
                            }
                        }
                    });
            });
        let inside = shown.response.rect;
        let outside = ui.input(|input| {
            input.pointer.primary_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|point| !inside.contains(point))
        });
        if let Some(as_tab) = chosen {
            self.strip_as_tab = as_tab;
        }
        if chosen.is_some() || outside {
            ui.ctx().data_mut(|data| data.remove::<Pos2>(id));
        }
    }

    /// The top and bottom rows folded into icons (the Timeline under the
    /// canvas, as Clip Studio Paint starts), closed rows' thin bars, and
    /// the arrows of an open bottom row: one folds it into icons, two
    /// close it.
    fn row_strips(&mut self, ui: &mut Ui, laid: &Laid, theme: &Theme) {
        for (row, strip) in [
            (Row::Top, laid.top_strip.as_ref()),
            (Row::Bottom, laid.bottom_strip.as_ref()),
        ] {
            let Some(strip) = strip else {
                continue;
            };
            let rect = strip.rect;
            ui.painter().rect_filled(rect, 0.0, theme.panel);
            let edge = if row == Row::Bottom {
                rect.top()
            } else {
                rect.bottom()
            };
            ui.painter().hline(rect.x_range(), edge, theme.hairline());
            let mut x = rect.left() + 6.0;
            for group in &strip.groups {
                for palette in group {
                    let icon =
                        Rect::from_min_size(pos2(x, rect.center().y - 13.0), vec2(26.0, 26.0));
                    x += 30.0;
                    if self.ready {
                        self.dock.grip(ui, icon, *palette, "row-icon");
                    }
                    if icon_button(
                        ui,
                        icon,
                        palette.icon(),
                        true,
                        false,
                        &palette.title(),
                        theme,
                    ) {
                        // The row opens on that palette.
                        self.dock.row_folded[row.index()] = false;
                        self.dock.choose(*palette);
                    }
                }
                x += 6.0;
            }
        }
        for (row, bar) in &laid.row_handles {
            ui.painter().rect_filled(*bar, 0.0, theme.panel);
            let at = Rect::from_center_size(bar.center(), vec2(24.0, bar.height()));
            let id = ("sue-paint-open-row", row.index());
            let down = *row == Row::Top;
            if vertical_arrows(ui, at, 2, down, id, &tr!("show-palettes"), theme) {
                self.dock.row_hidden[row.index()] = false;
            }
        }
        // An open bottom row folds into icons, or closes, from the right end
        // of its header.
        if let Some(row) = laid.bottom {
            let fold =
                Rect::from_min_size(pos2(row.right() - 40.0, row.top() + 6.0), vec2(16.0, 16.0));
            let close = fold.translate(vec2(18.0, 0.0));
            let tip = tr!("fold-column-into-icons");
            if vertical_arrows(ui, fold, 1, true, "sue-paint-fold-bottom", &tip, theme) {
                self.dock.row_folded[Row::Bottom.index()] = true;
            }
            let tip = tr!("hide-palettes");
            if vertical_arrows(ui, close, 2, true, "sue-paint-close-bottom", &tip, theme) {
                self.dock.row_hidden[Row::Bottom.index()] = true;
            }
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
                // Rolled up by a double-click on its title, or its arrow
                // (Clip Studio Paint's Minimize).
                .collapsible(true)
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
                    // A floating palette has its menu too.
                    self.palette_menu(ui, palette, area, cx, theme);
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
            // Kept while unrolled: a rolled-up window keeps its size.
            if let Some(inner) = &window
                && inner.inner.is_some()
            {
                self.keep_window(palette, inner.response.rect);
            }
            let redock = window
                .as_ref()
                .and_then(|inner| inner.inner.flatten())
                .is_some();
            // Its X hides it for good, as Clip Studio Paint's does; the
            // arrow docks it again.
            if !open {
                self.close_palette(palette);
            } else if redock {
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
        if closed {
            self.close_palette(palette);
        } else if redock {
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
    fn the_clip_studio_style_is_arranged_as_clip_studio_paint_is() {
        let dock = Dock::default();
        assert_eq!(dock, classic_layout(), "the default style's");
        let laid = dock.layout(body(), &all, 0.0);
        // The command bar, the tools, Sub Tool, Tool Property, Brush Size,
        // the color tabs, then Navigator, Layer Property and Layer.
        assert_eq!(laid.groups.len(), 9);
        let bar = laid.top.unwrap();
        assert_eq!((bar.width(), bar.height()), (1600.0, 38.0));
        assert_eq!(laid.center.top(), 38.0);
        assert_eq!(laid.center.left(), 46.0 + 322.0);
        assert_eq!(laid.center.right(), 1600.0 - 300.0 - STRIP);
        // Sub Tool, Tool Property and Brush Size are three palettes
        // stacked in one column, over the six color palettes as tabs.
        let left: Vec<Vec<Palette>> = dock.left[1]
            .groups
            .iter()
            .map(|group| group.tabs.clone())
            .collect();
        assert_eq!(
            left,
            [
                vec![Palette::SubTool],
                vec![Palette::ToolProperty],
                vec![Palette::BrushGrid],
                COLOR_PALETTES.to_vec(),
            ]
        );
        let place = |palette| {
            laid.docked()
                .find(|(found, _)| *found == palette)
                .unwrap()
                .1
        };
        assert_eq!(place(Palette::SubTool).top(), 38.0 + COLUMN_BAR);
        assert!(place(Palette::ToolProperty).top() > place(Palette::SubTool).top());
        assert!(place(Palette::BrushGrid).top() > place(Palette::ToolProperty).top());
        assert!((place(Palette::Color).bottom() - 900.0).abs() < 0.5);
        // The Timeline starts folded to an icon under the canvas.
        assert!(laid.bottom.is_none());
        let strip = laid.bottom_strip.unwrap();
        assert_eq!(strip.groups, [vec![Palette::Timeline]]);
        assert_eq!(strip.rect.x_range(), laid.center.x_range());
        // Material and the rest in the strip at the right.
        assert_eq!(laid.icons, STRIP_PALETTES);
        // Every palette has a place: none is left out, and none shows in
        // two places.
        // Sub Tool Detail waits until it is opened.
        for palette in PALETTES.into_iter().filter(|palette| !palette.optional()) {
            let places = usize::from(dock.find(palette).is_some())
                + usize::from(dock.iconized.contains(&palette));
            assert_eq!(places, 1, "{palette:?}");
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
        // Along the bottom row (opened) it lies on its side, gripped at
        // its left.
        dock.row_folded = [false; 2];
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
    fn tabs_tear_off_into_palettes_of_their_own_and_close_for_good() {
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
        // Closed, it is nowhere, and stays so when the layout is read again.
        dock.close(Palette::ColorSlider);
        dock.fill_missing();
        assert!(!dock.has(Palette::ColorSlider));
        let back = Dock::from_lines(&dock.to_lines()).unwrap();
        assert!(!back.has(Palette::ColorSlider) && back.is_closed(Palette::ColorSlider));
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
        // Lines as Sue Paint wrote them before, palettes named by title,
        // with Tool Property, Brush Size and four color palettes as inner
        // tabs of Sub Tool, Color Wheel and Color Set.
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
            [
                Palette::Material,
                Palette::LayerProperty,
                Palette::Cels,
                Palette::History,
                Palette::SubView,
                Palette::AutoAction,
                Palette::ItemBank,
                Palette::Info,
            ]
            .iter()
            .map(|palette| format!("icon {}", palette.title())),
        );
        let read = SuePaint::read_layout(&old, &classic_layout()).unwrap();
        let tabs = |side: &[Column<Palette>], column: usize| -> Vec<Vec<Palette>> {
            side[column]
                .groups
                .iter()
                .map(|group| group.tabs.clone())
                .collect()
        };
        assert_eq!(
            tabs(&read.left, 1),
            [
                vec![Palette::SubTool, Palette::ToolProperty, Palette::BrushGrid],
                vec![
                    Palette::Color,
                    Palette::ColorSlider,
                    Palette::Intermediate,
                    Palette::Approximate
                ],
                vec![Palette::ColorSet, Palette::ColorHistory],
            ],
            "the old inner tabs come back as tabs, where they showed"
        );
        assert!(read.left[1].groups.iter().all(|group| group.active == 0));
        assert_eq!(tabs(&read.left, 2), [vec![Palette::BrushSize]]);
        assert_eq!(
            tabs(&read.right, 0),
            [vec![Palette::Navigator], vec![Palette::Layer]]
        );
        assert_eq!(read.bottom_height, 210.0);
        assert!(!read.row_folded[1], "the Timeline stays open as it was");
        for palette in PALETTES.into_iter().filter(|palette| !palette.optional()) {
            let places = usize::from(read.find(palette).is_some())
                + usize::from(read.iconized.contains(&palette));
            assert_eq!(places, 1, "{palette:?}");
        }
    }
}
