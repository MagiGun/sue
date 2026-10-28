//! Where Sue Paint's palettes sit, and how people move them: docked in
//! columns at the left and right of the canvas (drag a column's edge to
//! make it wider, the line between two palettes to share the height
//! differently), floating in their own windows, or collapsed to icons in a
//! strip at the far right, as in Clip Studio Paint. When the window is too
//! narrow for every column, the widest collapse to icons by themselves and
//! come back when there is room.

use egui::{Align2, Id, Order, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use crate::icons::{self, Icon};
use crate::panels::icon_button;
use crate::studio::{Panels, SuePaint};

/// A palette people can dock, float or collapse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Palette {
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
}

impl Palette {
    /// What its window and icon are called.
    pub fn title(self) -> &'static str {
        match self {
            Self::Tools => "Tool",
            Self::SubTool => "Sub Tool",
            Self::BrushSize => "Brush Size",
            Self::Color => "Color Wheel",
            Self::ColorSet => "Color Set",
            Self::Navigator => "Navigator",
            Self::Layer => "Layer",
        }
    }

    /// Its icon in the strip.
    pub fn icon(self) -> Icon {
        match self {
            Self::Tools => Icon::Pen,
            Self::SubTool => Icon::Brush,
            Self::BrushSize => Icon::BrushSize,
            Self::Color => Icon::Wheel,
            Self::ColorSet => Icon::Swatches,
            Self::Navigator => Icon::Navigator,
            Self::Layer => Icon::Layers,
        }
    }

    /// The width of a palette that is always as wide, like the tool strip.
    pub fn fixed_width(self) -> Option<f32> {
        match self {
            Self::Tools => Some(40.0),
            Self::BrushSize => Some(58.0),
            _ => None,
        }
    }

    /// The least height it is usable at.
    pub fn min_height(self) -> f32 {
        match self {
            Self::Tools | Self::BrushSize => 120.0,
            Self::Color => 150.0,
            _ => 90.0,
        }
    }

    /// `true` when the Window menu shows it.
    pub fn shown(self, panels: &Panels) -> bool {
        match self {
            Self::Tools => panels.tool,
            Self::SubTool => panels.sub_tool || panels.tool_property,
            Self::BrushSize => panels.brush_size,
            Self::Color => panels.color,
            Self::ColorSet => panels.color_set,
            Self::Navigator => panels.navigator,
            Self::Layer => panels.layer,
        }
    }

    /// The default size of its window when it floats.
    fn float_size(self) -> Vec2 {
        match self {
            Self::Tools => vec2(44.0, 420.0),
            Self::BrushSize => vec2(62.0, 360.0),
            Self::Layer => vec2(300.0, 420.0),
            _ => vec2(260.0, 320.0),
        }
    }
}

/// A side of the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Side {
    Left,
    Right,
}

/// A column of palettes sharing its height by weight.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Column {
    /// The palettes, top to bottom, with their share of the height.
    pub palettes: Vec<(Palette, f32)>,
    /// Its width in points (palettes with a fixed width decide it).
    pub width: f32,
}

/// What is being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// A column's inner edge.
    Edge {
        side: Side,
        column: usize,
        from: f32,
        width: f32,
    },
    /// The line between two palettes of a column.
    Split {
        side: Side,
        column: usize,
        upper: Palette,
        lower: Palette,
        from: f32,
        weights: (f32, f32),
        per_point: f32,
    },
    /// The top of the timeline.
    Timeline { from: f32, height: f32 },
}

/// Where every palette is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Dock {
    /// Columns from the window's left edge inward.
    pub left: Vec<Column>,
    /// Columns from the window's right edge inward.
    pub right: Vec<Column>,
    /// Floating palettes and their place in the studio.
    pub floating: Vec<(Palette, Rect)>,
    /// Palettes collapsed to icons on purpose.
    pub iconized: Vec<Palette>,
    /// The collapsed palette showing beside the strip.
    pub popped: Option<Palette>,
    /// The palette whose menu is open.
    pub menu: Option<Palette>,
    /// The timeline's height.
    pub timeline: f32,
    drag: Option<Drag>,
}

impl Default for Dock {
    fn default() -> Self {
        let column = |palettes: &[(Palette, f32)], width: f32| Column {
            palettes: palettes.to_vec(),
            width,
        };
        Self {
            left: vec![
                column(&[(Palette::Tools, 1.0)], 40.0),
                column(
                    &[
                        (Palette::SubTool, 0.44),
                        (Palette::Color, 0.3),
                        (Palette::ColorSet, 0.26),
                    ],
                    236.0,
                ),
                column(&[(Palette::BrushSize, 1.0)], 58.0),
            ],
            right: vec![column(
                &[(Palette::Navigator, 0.42), (Palette::Layer, 0.58)],
                300.0,
            )],
            floating: Vec::new(),
            iconized: Vec::new(),
            popped: None,
            menu: None,
            timeline: 160.0,
            drag: None,
        }
    }
}

/// The width of the icon strip.
pub(crate) const STRIP: f32 = 34.0;

/// The canvas never gets narrower than this while columns can collapse.
const MIN_CANVAS: f32 = 300.0;

/// Where everything goes this frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Laid {
    /// Docked palettes and their rects.
    pub docked: Vec<(Palette, Rect)>,
    /// Columns: side, index, rect.
    pub columns: Vec<(Side, usize, Rect)>,
    /// Lines between palettes: side, column, upper, lower, rect, the
    /// column's height.
    pub splits: Vec<(Side, usize, Palette, Palette, Rect, f32)>,
    /// Columns' inner edges people can drag: side, column, rect.
    pub edges: Vec<(Side, usize, Rect)>,
    /// The icon strip, when it shows.
    pub strip: Option<Rect>,
    /// Palettes in the strip: the collapsed ones, then those the window
    /// had no room for.
    pub icons: Vec<Palette>,
    /// What is left for the canvas and the timeline.
    pub center: Rect,
}

impl Dock {
    /// Takes `palette` out of wherever it is.
    pub fn remove(&mut self, palette: Palette) {
        for column in self.left.iter_mut().chain(self.right.iter_mut()) {
            column.palettes.retain(|(found, _)| *found != palette);
        }
        self.left.retain(|column| !column.palettes.is_empty());
        self.right.retain(|column| !column.palettes.is_empty());
        self.floating.retain(|(found, _)| *found != palette);
        self.iconized.retain(|found| *found != palette);
        if self.popped == Some(palette) {
            self.popped = None;
        }
    }

    /// Docks `palette` on `side`: in a column of its own for a palette of
    /// fixed width, else at the bottom of the innermost wide column.
    pub fn dock(&mut self, palette: Palette, side: Side) {
        self.remove(palette);
        let columns = match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        };
        match palette.fixed_width() {
            Some(width) => columns.push(Column {
                palettes: vec![(palette, 1.0)],
                width,
            }),
            None => {
                let wide = columns.iter_mut().rev().find(|column| {
                    column
                        .palettes
                        .iter()
                        .all(|(found, _)| found.fixed_width().is_none())
                });
                match wide {
                    Some(column) => {
                        let share = 1.0 / (column.palettes.len() as f32 + 1.0);
                        column.palettes.push((palette, share.max(0.2)));
                    }
                    None => columns.push(Column {
                        palettes: vec![(palette, 1.0)],
                        width: 260.0,
                    }),
                }
            }
        }
    }

    /// Floats `palette` in a window at `at`.
    pub fn float(&mut self, palette: Palette, at: Pos2) {
        self.remove(palette);
        self.floating
            .push((palette, Rect::from_min_size(at, palette.float_size())));
    }

    /// Collapses `palette` to an icon in the strip.
    pub fn iconize(&mut self, palette: Palette) {
        self.remove(palette);
        self.iconized.push(palette);
    }

    /// The side `palette` is docked on, if it is docked.
    pub fn side_of(&self, palette: Palette) -> Option<Side> {
        let has = |columns: &[Column]| {
            columns
                .iter()
                .any(|column| column.palettes.iter().any(|(found, _)| *found == palette))
        };
        if has(&self.left) {
            Some(Side::Left)
        } else if has(&self.right) {
            Some(Side::Right)
        } else {
            None
        }
    }

    /// Lays everything out in `body`; `shown` says which palettes the
    /// Window menu shows and `hide` (0 to 1) slides the columns out to
    /// their edges.
    pub fn layout(&self, body: Rect, shown: &dyn Fn(Palette) -> bool, hide: f32) -> Laid {
        let visible = |column: &Column| -> Vec<(Palette, f32)> {
            column
                .palettes
                .iter()
                .copied()
                .filter(|(palette, _)| shown(*palette))
                .collect()
        };
        let width_of = |column: &Column| -> f32 {
            let palettes = visible(column);
            palettes
                .iter()
                .filter_map(|(palette, _)| palette.fixed_width())
                .fold(None, |most: Option<f32>, width| {
                    Some(most.map_or(width, |most| most.max(width)))
                })
                .unwrap_or(column.width)
        };
        // Columns that show anything, and their widths.
        let mut sides: Vec<(Side, usize, f32, bool)> = Vec::new();
        for (side, columns) in [(Side::Left, &self.left), (Side::Right, &self.right)] {
            for (index, column) in columns.iter().enumerate() {
                if !visible(column).is_empty() {
                    sides.push((side, index, width_of(column), true));
                }
            }
        }
        let mut icons: Vec<Palette> = self
            .iconized
            .iter()
            .copied()
            .filter(|palette| shown(*palette))
            .collect();
        // Too narrow: the widest columns collapse to icons (never the tools).
        loop {
            let used: f32 = sides
                .iter()
                .filter(|(_, _, _, open)| *open)
                .map(|(_, _, width, _)| width)
                .sum();
            let strip = if icons.is_empty() { 0.0 } else { STRIP };
            if used + strip + MIN_CANVAS <= body.width() {
                break;
            }
            let widest = sides
                .iter_mut()
                .filter(|(side, index, _, open)| {
                    let column = match side {
                        Side::Left => &self.left[*index],
                        Side::Right => &self.right[*index],
                    };
                    *open
                        && !visible(column)
                            .iter()
                            .any(|(palette, _)| *palette == Palette::Tools)
                })
                .max_by(|a, b| a.2.total_cmp(&b.2));
            let Some(widest) = widest else {
                break;
            };
            widest.3 = false;
            let column = match widest.0 {
                Side::Left => &self.left[widest.1],
                Side::Right => &self.right[widest.1],
            };
            icons.extend(visible(column).iter().map(|(palette, _)| *palette));
        }
        let open = |side: Side| -> Vec<(usize, f32)> {
            sides
                .iter()
                .filter(|(found, _, _, open)| *found == side && *open)
                .map(|(_, index, width, _)| (*index, *width))
                .collect()
        };
        let (left, right) = (open(Side::Left), open(Side::Right));
        let left_total: f32 = left.iter().map(|(_, width)| width).sum();
        let strip_width = if icons.is_empty() { 0.0 } else { STRIP };
        let right_total: f32 = right.iter().map(|(_, width)| width).sum::<f32>() + strip_width;
        let mut laid = Laid {
            docked: Vec::new(),
            columns: Vec::new(),
            splits: Vec::new(),
            edges: Vec::new(),
            strip: None,
            icons,
            center: body,
        };
        let place_column = |laid: &mut Laid, side: Side, index: usize, rect: Rect| {
            let column = match side {
                Side::Left => &self.left[index],
                Side::Right => &self.right[index],
            };
            laid.columns.push((side, index, rect));
            let palettes = visible(column);
            let total: f32 = palettes.iter().map(|(_, weight)| weight.max(0.01)).sum();
            let mut heights: Vec<f32> = palettes
                .iter()
                .map(|(palette, weight)| {
                    (rect.height() * weight.max(0.01) / total)
                        .max(palette.min_height().min(rect.height()))
                })
                .collect();
            // Keep the column's height: take any excess from the tallest.
            let excess: f32 = heights.iter().sum::<f32>() - rect.height();
            if excess > 0.0
                && let Some(tallest) = heights.iter_mut().max_by(|a, b| a.total_cmp(b))
            {
                *tallest = (*tallest - excess).max(24.0);
            }
            let mut top = rect.top();
            for (at, ((palette, _), height)) in palettes.iter().zip(&heights).enumerate() {
                let piece =
                    Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), *height));
                laid.docked.push((*palette, piece));
                top = piece.bottom();
                if let Some((lower, _)) = palettes.get(at + 1) {
                    laid.splits.push((
                        side,
                        index,
                        *palette,
                        *lower,
                        Rect::from_center_size(pos2(rect.center().x, top), vec2(rect.width(), 6.0)),
                        rect.height(),
                    ));
                }
            }
            let resizable = palettes
                .iter()
                .all(|(palette, _)| palette.fixed_width().is_none());
            if resizable {
                let x = match side {
                    Side::Left => rect.right(),
                    Side::Right => rect.left(),
                };
                laid.edges.push((
                    side,
                    index,
                    Rect::from_center_size(pos2(x, rect.center().y), vec2(6.0, rect.height())),
                ));
            }
        };
        let mut x = body.left() - hide * left_total;
        for (index, width) in &left {
            let rect = Rect::from_min_max(pos2(x, body.top()), pos2(x + width, body.bottom()));
            place_column(&mut laid, Side::Left, *index, rect);
            x = rect.right();
        }
        let center_left = x;
        let mut x = body.right() + hide * right_total;
        if strip_width > 0.0 {
            laid.strip = Some(Rect::from_min_max(
                pos2(x - STRIP, body.top()),
                pos2(x, body.bottom()),
            ));
            x -= STRIP;
        }
        for (index, width) in &right {
            let rect = Rect::from_min_max(pos2(x - width, body.top()), pos2(x, body.bottom()));
            place_column(&mut laid, Side::Right, *index, rect);
            x = rect.left();
        }
        laid.center = Rect::from_min_max(
            pos2(center_left, body.top()),
            pos2(x.max(center_left), body.bottom()),
        );
        laid
    }

    fn column_mut(&mut self, side: Side, index: usize) -> Option<&mut Column> {
        match side {
            Side::Left => self.left.get_mut(index),
            Side::Right => self.right.get_mut(index),
        }
    }
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
            Palette::Tools => self.tool_strip(ui, rect, cx, theme),
            Palette::SubTool => self.tool_panel(ui, rect, cx, theme),
            Palette::BrushSize => self.size_column(ui, rect, theme),
            Palette::Color => self.color_panel(ui, rect, theme),
            Palette::ColorSet => self.color_set_panel(ui, rect, theme),
            Palette::Navigator => self.navigator(ui, rect, cx, theme),
            Palette::Layer => self.layer_panel(ui, rect, cx, theme),
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
        let button = if palette.fixed_width().is_some() {
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
            &format!("{}: dock, float or collapse", palette.title()),
            theme,
        ) {
            self.dock.menu = if open { None } else { Some(palette) };
        }
        if self.dock.menu != Some(palette) {
            return;
        }
        let side = self.dock.side_of(palette);
        let mut chosen: Option<&str> = None;
        let shown = egui::Area::new(Id::unique(("sue-paint-dock-menu", palette.title())))
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
                            item(ui, "Dock on the Left", "left");
                        }
                        if side != Some(Side::Right) {
                            item(ui, "Dock on the Right", "right");
                        }
                        if !floating {
                            item(ui, "Float in a Window", "float");
                        }
                        if !self.dock.iconized.contains(&palette) {
                            item(ui, "Collapse to an Icon", "icon");
                        }
                        item(ui, "Close", "close");
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
        match chosen {
            Some("left") => self.dock.dock(palette, Side::Left),
            Some("right") => self.dock.dock(palette, Side::Right),
            Some("float") => {
                let at = rect.min + vec2(-40.0, 30.0);
                self.dock.float(palette, at.max(pos2(8.0, 40.0)));
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
    fn close_palette(&mut self, palette: Palette) {
        let panels = &mut self.panels;
        match palette {
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
        }
        if self.dock.popped == Some(palette) {
            self.dock.popped = None;
        }
    }

    /// The palettes, docked, collapsed and floating, around the canvas.
    /// Returns what is left for the canvas and the timeline.
    pub(crate) fn dock_ui(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Rect {
        let panels = self.panels;
        let shown = move |palette: Palette| palette.shown(&panels);
        let laid = self.dock.layout(body, &shown, self.hide);
        for (_, _, rect) in &laid.columns {
            ui.painter().rect_filled(*rect, 0.0, theme.panel);
        }
        for (palette, rect) in &laid.docked {
            self.draw_palette(ui, *palette, *rect, cx, theme);
            ui.painter()
                .hline(rect.x_range(), rect.bottom(), theme.hairline());
        }
        for (side, _, rect) in &laid.columns {
            let x = match side {
                Side::Left => rect.right(),
                Side::Right => rect.left(),
            };
            ui.painter().vline(x, rect.y_range(), theme.hairline());
        }
        if self.ready {
            for (palette, rect) in &laid.docked {
                self.palette_menu(ui, *palette, *rect, false, theme);
            }
            self.splitters(ui, &laid, theme);
        }
        if let Some(strip) = laid.strip {
            self.strip_ui(ui, strip, &laid.icons, cx, theme);
        }
        if self.ready {
            self.floating_ui(ui, body, cx, theme);
        }
        laid.center
    }

    /// Dragging columns' edges and the lines between palettes.
    fn splitters(&mut self, ui: &mut Ui, laid: &Laid, theme: &Theme) {
        let pointer = ui.input(|input| input.pointer.interact_pos());
        for (side, column, rect) in &laid.edges {
            let response = ui.interact(
                *rect,
                Id::unique(("sue-paint-edge", *side == Side::Left, *column)),
                Sense::drag(),
            );
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                ui.painter().vline(
                    rect.center().x,
                    rect.y_range(),
                    Stroke::new(2.0, theme.border_active),
                );
            }
            if response.drag_started()
                && let (Some(point), Some(found)) = (pointer, self.dock.column_mut(*side, *column))
            {
                self.dock.drag = Some(Drag::Edge {
                    side: *side,
                    column: *column,
                    from: point.x,
                    width: found.width,
                });
            }
        }
        for (side, column, upper, lower, rect, height) in &laid.splits {
            let response = ui.interact(
                *rect,
                Id::unique((
                    "sue-paint-split",
                    *side == Side::Left,
                    *column,
                    upper.title(),
                )),
                Sense::drag(),
            );
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                ui.painter().hline(
                    rect.x_range(),
                    rect.center().y,
                    Stroke::new(2.0, theme.border_active),
                );
            }
            if response.drag_started()
                && let (Some(point), Some(found)) = (pointer, self.dock.column_mut(*side, *column))
            {
                let weight = |palette: Palette| {
                    found
                        .palettes
                        .iter()
                        .find(|(p, _)| *p == palette)
                        .map_or(0.5, |(_, w)| *w)
                };
                let total: f32 = found.palettes.iter().map(|(_, w)| w.max(0.01)).sum();
                self.dock.drag = Some(Drag::Split {
                    side: *side,
                    column: *column,
                    upper: *upper,
                    lower: *lower,
                    from: point.y,
                    weights: (weight(*upper), weight(*lower)),
                    per_point: total / height.max(1.0),
                });
            }
        }
        let down = ui.input(|input| input.pointer.primary_down());
        match (self.dock.drag, pointer) {
            (
                Some(Drag::Edge {
                    side,
                    column,
                    from,
                    width,
                }),
                Some(point),
            ) if down => {
                let delta = match side {
                    Side::Left => point.x - from,
                    Side::Right => from - point.x,
                };
                if let Some(found) = self.dock.column_mut(side, column) {
                    found.width = (width + delta).clamp(170.0, 560.0);
                }
            }
            (
                Some(Drag::Split {
                    side,
                    column,
                    upper,
                    lower,
                    from,
                    weights,
                    per_point,
                }),
                Some(point),
            ) if down => {
                let moved = (point.y - from) * per_point;
                let least = 0.08;
                let up = (weights.0 + moved).max(least);
                let low = (weights.0 + weights.1 - up).max(least);
                if let Some(found) = self.dock.column_mut(side, column) {
                    for (palette, weight) in &mut found.palettes {
                        if *palette == upper {
                            *weight = weights.0 + weights.1 - low;
                        } else if *palette == lower {
                            *weight = low;
                        }
                    }
                }
            }
            (Some(Drag::Timeline { from, height }), Some(point)) if down => {
                self.dock.timeline = (height + from - point.y).clamp(90.0, 520.0);
            }
            (Some(_), _) if !down => self.dock.drag = None,
            _ => {}
        }
    }

    /// The timeline's top edge, which people drag to resize it.
    pub(crate) fn timeline_splitter(
        &mut self,
        ui: &mut Ui,
        top: f32,
        x: egui::Rangef,
        theme: &Theme,
    ) {
        let rect = Rect::from_x_y_ranges(x, egui::Rangef::new(top - 3.0, top + 3.0));
        let response = ui.interact(rect, Id::unique("sue-paint-timeline-edge"), Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            ui.painter()
                .hline(x, top, Stroke::new(2.0, theme.border_active));
        }
        if response.drag_started()
            && let Some(point) = ui.input(|input| input.pointer.interact_pos())
        {
            self.dock.drag = Some(Drag::Timeline {
                from: point.y,
                height: self.dock.timeline,
            });
        }
    }

    /// The icon strip at the far right, and the collapsed palette it shows.
    fn strip_ui(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        icons_shown: &[Palette],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(strip, 0.0, theme.panel);
        ui.painter()
            .vline(strip.left(), strip.y_range(), theme.hairline());
        for (index, palette) in icons_shown.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(strip.left() + 4.0, strip.top() + 6.0 + index as f32 * 32.0),
                vec2(26.0, 26.0),
            );
            let open = self.dock.popped == Some(*palette);
            if icon_button(ui, rect, palette.icon(), true, open, palette.title(), theme) {
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
        let at = pos2(strip.left() - size.x - 4.0, strip.top() + 4.0);
        let area = egui::Area::new(Id::unique(("sue-paint-popped", palette.title())))
            .order(Order::Foreground)
            .fade_in(false)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter()
                    .rect_filled(rect.expand(1.0), 0.0, theme.border);
                self.draw_palette(ui, palette, rect, cx, theme);
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
        if clicked_outside && self.dock.menu.is_none() {
            self.dock.popped = None;
        }
    }

    /// Floating palettes, each in a window people move and resize.
    fn floating_ui(&mut self, ui: &mut Ui, body: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        let floating = self.dock.floating.clone();
        let panels = self.panels;
        for (palette, rect) in floating {
            if !palette.shown(&panels) {
                continue;
            }
            let mut open = true;
            let window = egui::Window::new(palette.title())
                .id(Id::unique(("sue-paint-float", palette.title())))
                .default_rect(rect)
                .constrain_to(body)
                .resizable(true)
                .collapsible(false)
                .title_bar(true)
                .min_size(vec2(palette.fixed_width().unwrap_or(180.0), 120.0))
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    let area = ui.available_rect_before_wrap();
                    let area = Rect::from_min_size(area.min, area.size().max(vec2(40.0, 80.0)));
                    ui.allocate_rect(area, Sense::hover());
                    self.draw_palette(ui, palette, area, cx, theme);
                    // Back into a column, from inside its window.
                    let back = Rect::from_min_size(
                        pos2(area.right() - 22.0, area.top() + 3.0),
                        vec2(18.0, 18.0),
                    );
                    if icon_button(ui, back, Icon::Up, true, false, "Dock it again", theme) {
                        Some(true)
                    } else {
                        None
                    }
                });
            if let Some(inner) = &window
                && let Some(entry) = self
                    .dock
                    .floating
                    .iter_mut()
                    .find(|(found, _)| *found == palette)
            {
                entry.1 = inner.response.rect;
            }
            let redock = window
                .as_ref()
                .and_then(|inner| inner.inner.flatten())
                .is_some();
            if !open || redock {
                let side = if rect.center().x < body.center().x {
                    Side::Left
                } else {
                    Side::Right
                };
                self.dock.dock(palette, side);
            }
        }
        let _ = Align2::LEFT_TOP;
        let _ = icons::paint;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(_: Palette) -> bool {
        true
    }

    #[test]
    fn the_default_layout_is_like_a_painting_apps() {
        let dock = Dock::default();
        let body = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0));
        let laid = dock.layout(body, &all, 0.0);
        assert_eq!(laid.docked.len(), 7);
        assert!(laid.strip.is_none(), "room for every column");
        assert_eq!(laid.center.left(), 40.0 + 236.0 + 58.0);
        assert_eq!(laid.center.right(), 1600.0 - 300.0);
        // Heights share the column without gaps.
        let sub = laid
            .docked
            .iter()
            .find(|(p, _)| *p == Palette::SubTool)
            .unwrap()
            .1;
        let set = laid
            .docked
            .iter()
            .find(|(p, _)| *p == Palette::ColorSet)
            .unwrap()
            .1;
        assert_eq!(sub.top(), 0.0);
        assert!((set.bottom() - 900.0).abs() < 0.5);
        assert_eq!(laid.splits.len(), 3);
        assert_eq!(laid.edges.len(), 2, "the two wide columns resize");
    }

    #[test]
    fn a_narrow_window_collapses_the_widest_columns_to_icons() {
        let dock = Dock::default();
        let body = Rect::from_min_size(Pos2::ZERO, vec2(760.0, 700.0));
        let laid = dock.layout(body, &all, 0.0);
        assert!(laid.strip.is_some());
        assert!(laid.icons.contains(&Palette::Navigator) && laid.icons.contains(&Palette::Layer));
        assert!(
            laid.center.width() >= MIN_CANVAS - 0.5,
            "{}",
            laid.center.width()
        );
        assert!(
            laid.docked.iter().any(|(p, _)| *p == Palette::Tools),
            "the tools stay"
        );
    }

    #[test]
    fn palettes_move_float_collapse_and_come_back() {
        let mut dock = Dock::default();
        dock.dock(Palette::Color, Side::Right);
        assert_eq!(dock.side_of(Palette::Color), Some(Side::Right));
        assert_eq!(dock.right[0].palettes.len(), 3);
        dock.float(Palette::Layer, pos2(100.0, 100.0));
        assert_eq!(dock.side_of(Palette::Layer), None);
        assert_eq!(dock.floating.len(), 1);
        dock.iconize(Palette::Navigator);
        let body = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0));
        let laid = dock.layout(body, &all, 0.0);
        assert_eq!(laid.icons, [Palette::Navigator]);
        dock.dock(Palette::Navigator, Side::Left);
        assert!(dock.iconized.is_empty());
        dock.dock(Palette::BrushSize, Side::Right);
        assert!(
            dock.right
                .iter()
                .any(|column| column.palettes == [(Palette::BrushSize, 1.0)])
        );
        assert!(
            dock.left.iter().all(|column| !column.palettes.is_empty()),
            "no empty columns"
        );
    }
}
