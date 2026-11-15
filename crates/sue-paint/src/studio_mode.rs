//! Studio Mode, Sue Paint's fuller touch layout on phones and tablets,
//! closer to the desktop: a tool column at the left (the menu, swap with
//! the eraser, the pen, the lasso, the object tool, then size and opacity
//! on tall sliders and the color), a row of palettes and a command bar
//! along the bottom, a fullscreen toggle, and the edge key bar: Esc,
//! Ctrl, Shift, Alt, Space and seven keys of your own (T1 to T7, commands
//! kept in the rice).
//!
//! The edge key bar comes in from either side of the canvas. A swipe in
//! from an edge (or a tap on the tab there) brings it in see-through,
//! over the canvas; a second swipe pins it beside the canvas, which
//! makes room for it. A swipe back out, or its close button, puts it
//! away.
//!
//! What fingers do on the canvas is here too ([`Gestures`]): with Draw
//! with Fingers off, a finger moves the view and only the pen draws;
//! two fingers pan, pinch and turn the view as the settings allow.

use egui::{Align2, Color32, CornerRadius, Rect, Sense, TouchPhase, Ui, Vec2, pos2, vec2};
use sue_rice::{RiceDeclaration, RiceParameter, RiceSchema, RiceValue, SetRiceValue};
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Space, TextRole, Tip};
use sue_wm::{AppletContext, Facts, ShellRequest, Theme};

use crate::phone::Sheet;
use crate::studio::SuePaint;
use crate::tools::Tool;

/// How many keys of their own people have.
pub const CUSTOM_KEYS: usize = 7;
/// The commands T1 to T7 run at first.
pub const CUSTOM_DEFAULTS: [&str; CUSTOM_KEYS] = [
    "edit.undo",
    "edit.redo",
    "select.all",
    "select.none",
    "layer.new",
    "view.fit",
    "view.flip_h",
];

/// The rice key of Draw with Fingers: whether a finger on the canvas
/// draws with the tool (Clip Studio Paint's "Use tools with finger").
pub const FINGER_KEY: &str = "sue_paint.finger_draws";
/// The rice key of what a two-finger swipe does: `move` or `none`.
pub const SWIPE_KEY: &str = "sue_paint.two_finger_swipe";
/// The rice key of what a two-finger pinch does: `scale` or `none`.
pub const PINCH_KEY: &str = "sue_paint.two_finger_pinch";
/// The rice key of what turning two fingers does: `rotate` or `none`.
pub const ROTATE_KEY: &str = "sue_paint.two_finger_rotation";

/// The commands of Studio Mode's command bar, after its back button.
const COMMAND_BAR: [&str; 10] = [
    "file.save",
    "edit.undo",
    "edit.redo",
    "touch.finger_draws",
    "edit.transform.scale",
    "select.all",
    "edit.copy",
    "edit.cut",
    "edit.paste",
    "file.timelapse.record",
];

/// How far a swipe goes in from an edge before it counts.
const SWIPE: Space = Space::S8;
/// How far in from an edge a swipe may start.
const EDGE: Space = Space::S5;
/// How much of the see-through key bar shows: enough to read its keys,
/// little enough to see the picture under it.
const SEE_THROUGH: f32 = 0.8;

/// The rice key of custom key `index` (0 is T1).
#[must_use]
pub fn custom_key(index: usize) -> String {
    format!("sue_paint.custom_key_{}", index + 1)
}

/// What fingers do on the canvas, as the settings say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gestures {
    /// A finger draws with the tool; off, it moves the view.
    pub(crate) finger_draws: bool,
    /// Two fingers moving together move the view.
    pub(crate) swipe: bool,
    /// Two fingers pinching zoom the view.
    pub(crate) pinch: bool,
    /// Two fingers turning turn the view.
    pub(crate) rotate: bool,
}

impl Gestures {
    /// The gestures as `rice` has them (each on when it is not set).
    #[must_use]
    pub(crate) fn of(rice: &RiceSchema) -> Self {
        let on = |key: &str| rice.get_str(key) != Some("none");
        Self {
            finger_draws: rice.get_bool(FINGER_KEY).unwrap_or(true),
            swipe: on(SWIPE_KEY),
            pinch: on(PINCH_KEY),
            rotate: on(ROTATE_KEY),
        }
    }
}

/// The settings of the touch gestures, for Settings and the rice file.
#[must_use]
pub(crate) fn declarations() -> Vec<RiceDeclaration> {
    let choice = |value: &str| RiceParameter::EnumChoice {
        value: value.to_owned(),
        options: vec![value.to_owned(), "none".to_owned()],
    };
    vec![
        RiceDeclaration::new(
            FINGER_KEY,
            RiceParameter::Bool { value: true },
            "A finger on the canvas draws with the tool; off, fingers only move, zoom and turn the view and the pen draws (Preferences, Touch gestures)",
        )
        .labeled("Draw with fingers"),
        RiceDeclaration::new(
            SWIPE_KEY,
            choice("move"),
            "What two fingers moving together on the canvas do (Preferences, Touch gestures)",
        )
        .labeled("Two-finger swipe"),
        RiceDeclaration::new(
            PINCH_KEY,
            choice("scale"),
            "What pinching two fingers on the canvas does (Preferences, Touch gestures)",
        )
        .labeled("Two-finger pinch"),
        RiceDeclaration::new(
            ROTATE_KEY,
            choice("rotate"),
            "What turning two fingers on the canvas does (Preferences, Touch gestures)",
        )
        .labeled("Two-finger rotation"),
    ]
}

/// Sets the switch `key` in the rice, so Settings shows it too.
pub(crate) fn set_bool(cx: &mut AppletContext<'_>, key: &str, on: bool) {
    if let Err(error) = cx.edit_rice(SetRiceValue::new(key, RiceValue::Bool(on))) {
        cx.fail(error.to_string());
    }
}

/// `true` when the window's touch `id` is a finger: as the system said,
/// else (where it does not say) a touch that reports no force, which a
/// pen always does.
#[must_use]
pub(crate) fn is_finger(id: u64, force: Option<f32>) -> bool {
    sue_platform::stylus::touch_is_pen(id).map_or(force.is_none(), |pen| !pen)
}

/// Where the edge key bar is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum KeyBar {
    /// Away: only its tab shows at the edge.
    #[default]
    Hidden,
    /// In, see-through, over the canvas.
    Peek,
    /// Pinned beside the canvas, which makes room for it.
    Pinned,
}

/// The side of the canvas the edge key bar comes in from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Edge {
    Left,
    #[default]
    Right,
}

/// Studio Mode's state between frames, and what fingers are doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Touch {
    /// Only the canvas shows.
    pub(crate) fullscreen: bool,
    /// The edge key bar.
    pub(crate) bar: KeyBar,
    /// The side it is on.
    pub(crate) edge: Edge,
    /// The modifier keys latched on, as if held.
    pub(crate) ctrl: bool,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    pub(crate) space: bool,
    /// The touch the pointer follows is a finger, until a press on the
    /// canvas reads it.
    pub(crate) finger: bool,
    /// A swipe in from an edge under way: the edge and where it began.
    swipe: Option<(Edge, f32)>,
}

impl Touch {
    /// The latched modifiers, to add to the keyboard's.
    #[must_use]
    pub(crate) fn modifiers(&self) -> egui::Modifiers {
        egui::Modifiers {
            alt: self.alt,
            ctrl: self.ctrl,
            shift: self.shift,
            mac_cmd: false,
            command: self.ctrl,
        }
    }

    /// Lets go of every latched key.
    pub(crate) fn release(&mut self) {
        self.ctrl = false;
        self.shift = false;
        self.alt = false;
        self.space = false;
    }

    /// A swipe in from `edge`: the bar comes in see-through, a second
    /// swipe from the same side pins it, one from the other side moves
    /// it there.
    pub(crate) fn swiped_in(&mut self, edge: Edge) {
        self.bar = match self.bar {
            KeyBar::Peek if self.edge == edge => KeyBar::Pinned,
            KeyBar::Pinned => KeyBar::Pinned,
            _ => KeyBar::Peek,
        };
        self.edge = edge;
    }
}

/// The icon of `tool`, as the tool commands give it.
#[must_use]
pub(crate) fn tool_glyph(tool: Tool) -> Icon {
    let registry = crate::commands::registry();
    // The Frame Border tool's command is `tool.pick.frame`.
    let short = tool.id().split('_').next().unwrap_or_default();
    [
        format!("tool.pick.{}", tool.id()),
        format!("tool.pick.{short}"),
        format!("tool.{}", tool.id()),
    ]
    .iter()
    .filter_map(|name| registry.find(name))
    .find_map(|(command, _)| Icon::from_name(command.icon))
    .unwrap_or(Icon::ToolPen)
}

impl SuePaint {
    /// A touch on the canvas, after the canvas has counted it: the first
    /// finger down tells whether the pointer it moves is a finger.
    pub(crate) fn touch_seen(&mut self, id: u64, phase: TouchPhase, force: Option<f32>) {
        if phase == TouchPhase::Start && self.touches.len() == 1 {
            self.touch.finger = is_finger(id, force);
        }
    }

    /// A press on the canvas: `true` when it is a finger's and fingers do
    /// not draw, so the press moves the view instead of using the tool.
    pub(crate) fn finger_moves_view(&mut self, cx: &AppletContext<'_>) -> bool {
        std::mem::take(&mut self.touch.finger) && !Gestures::of(cx.rice()).finger_draws
    }

    /// The keys that run `command`, as the menus show them; empty for
    /// none.
    pub(crate) fn keys_of(&self, command: &str) -> String {
        fn find(items: &[sue_wm::MenuItem], command: &str) -> Option<String> {
            items.iter().find_map(|item| match item {
                sue_wm::MenuItem::Action {
                    command: own,
                    shortcut,
                    ..
                } if own == command => Some(shortcut.clone()),
                sue_wm::MenuItem::Sub { items, .. } => find(items, command),
                _ => None,
            })
        }
        self.menus
            .iter()
            .find_map(|menu| find(&menu.items, command))
            .unwrap_or_default()
    }

    /// A button at `rect` that runs `command`: its icon and name from the
    /// registry, grayed or lit as the command is now, with its keys in
    /// the tooltip. Returns `true` when it was pressed.
    pub(crate) fn command_button(
        &self,
        ui: &Ui,
        rect: Rect,
        command: &str,
        facts: &Facts<'_>,
        theme: &Theme,
    ) -> bool {
        let registry = crate::commands::registry();
        let icon = registry
            .find(command)
            .and_then(|(found, _)| Icon::from_name(found.icon))
            .unwrap_or(Icon::More);
        let name = registry.label(command).unwrap_or_default();
        let keys = self.keys_of(command);
        let state = self.state(command, facts);
        let mut button = ui::icon_button(("sue-paint-touch", command), icon, &name)
            .enabled(state.enabled)
            .shortcut(&keys);
        if let Some(on) = state.checked {
            button = button.selected(on);
        }
        button.show_at(ui, rect, theme).clicked()
    }

    /// The main color as a round swatch at `rect`, which opens the Color
    /// sheet.
    pub(crate) fn color_swatch(&mut self, ui: &Ui, rect: Rect, theme: &Theme) {
        let response = ui.interact(
            rect,
            ui.scope_id().with("sue-paint-touch-color"),
            Sense::click(),
        );
        let side = rect.width().min(rect.height());
        let ring = side / 2.0 - theme.space(Space::S1);
        let [r, g, b] = self.main;
        let painter = ui.painter();
        painter.circle_filled(
            rect.center(),
            ring - theme.space(Space::S1),
            Color32::from_rgb(r, g, b),
        );
        painter.circle_stroke(rect.center(), ring, theme.hairline_active());
        ui::focus_ring(ui, &response, side / 2.0, theme);
        let name = tr!("color-2");
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &name));
        if ui::tooltip(response, &Tip::new(&name), theme).clicked() {
            self.toggle_sheet(Sheet::Color);
        }
    }

    /// Studio Mode's row in Sue's top bar: the picture's name and the
    /// fullscreen toggle.
    pub(crate) fn studio_bar(
        &mut self,
        ui: &mut Ui,
        bar: Rect,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) {
        let side = Density::Touch.icon_button().min(bar.height());
        let name = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map(|entity| entity.name().to_owned())
            .unwrap_or_default();
        ui.painter().with_clip_rect(bar).text(
            pos2(bar.left() + theme.space(Space::S5), bar.center().y),
            Align2::LEFT_CENTER,
            name,
            theme.text_font(TextRole::Body),
            theme.text,
        );
        let hint = if self.touch.fullscreen {
            tr!("studio-leave-fullscreen")
        } else {
            tr!("studio-fullscreen")
        };
        let at = Rect::from_min_size(
            pos2(
                bar.right() - side - theme.space(Space::S1),
                bar.center().y - side / 2.0,
            ),
            Vec2::splat(side),
        );
        if ui::icon_button("sue-paint-fullscreen", Icon::Fullscreen, &hint)
            .selected(self.touch.fullscreen)
            .show_at(ui, at, theme)
            .clicked()
        {
            self.touch.fullscreen = !self.touch.fullscreen;
            self.sheet = None;
        }
    }

    /// Studio Mode's page.
    pub(crate) fn studio_frame(
        &mut self,
        ui: &mut Ui,
        full: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut run: Option<String> = None;
        let button = Density::Touch.icon_button();
        let pad = theme.space(Space::S2);
        // The canvas and, when pinned, the key bar beside it share what
        // the tool column and the bottom bands leave.
        let (area, column, bands) = if self.touch.fullscreen {
            (full, None, None)
        } else {
            let rows = button * 2.0 + pad * 2.0;
            let column = Rect::from_min_max(
                full.min,
                pos2(full.left() + button + pad * 2.0, full.bottom() - rows),
            );
            let area = Rect::from_min_max(
                pos2(column.right(), full.top()),
                pos2(full.right(), column.bottom()),
            );
            (area, Some(column), Some((full, column.bottom())))
        };
        let width = self.key_bar_width(ui, theme);
        let canvas = match (self.touch.bar, self.touch.edge) {
            (KeyBar::Pinned, Edge::Left) => {
                Rect::from_min_max(pos2(area.left() + width, area.top()), area.max)
            }
            (KeyBar::Pinned, Edge::Right) => {
                Rect::from_min_max(area.min, pos2(area.right() - width, area.bottom()))
            }
            _ => area,
        };
        self.canvas_area(ui, canvas, cx, theme);
        if let Some(column) = column {
            self.tool_column(ui, column, theme);
        }
        if let Some((full, top)) = bands {
            self.studio_bands(ui, full, top, cx, theme, &mut run);
        }
        self.key_bar(ui, area, canvas, cx, theme, &mut run);
        if let Some(sheet) = self.sheet {
            self.phone_sheet(ui, canvas, sheet, cx, theme, &mut run);
        }
        if let Some(command) = run {
            self.run(&command, cx);
        }
    }

    /// The row of palettes and the command bar under the canvas, from
    /// `top` to the bottom of `full`.
    fn studio_bands(
        &mut self,
        ui: &Ui,
        full: Rect,
        top: f32,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let button = Density::Touch.icon_button();
        let (gap, pad) = (theme.space(Space::S1), theme.space(Space::S2));
        let palettes = Rect::from_min_max(
            pos2(full.left(), top),
            pos2(full.right(), top + button + pad),
        );
        let commands = Rect::from_min_max(pos2(full.left(), palettes.bottom()), full.max);
        for band in [palettes, commands] {
            ui.painter()
                .rect_filled(band, CornerRadius::ZERO, theme.panel);
            sue_wm::cover::cover(ui.ctx(), ui.layer_id(), band);
            ui.painter()
                .hline(band.x_range(), band.top(), theme.hairline());
        }
        let slot = |band: Rect, index: usize| {
            Rect::from_min_size(
                pos2(
                    band.left() + pad + index as f32 * (button + gap),
                    band.center().y - button / 2.0,
                ),
                Vec2::splat(button),
            )
        };
        let fits = |band: Rect| {
            ((band.width() - pad * 2.0) / (button + gap))
                .floor()
                .max(1.0) as usize
        };
        let sheets = [
            (Icon::PaletteNavigator, Sheet::Navigator, tr!("navigator")),
            (Icon::Layers, Sheet::Layers, tr!("layers-3")),
            (
                Icon::LayerProperty,
                Sheet::LayerProperty,
                tr!("studio-layer-property"),
            ),
            (
                Icon::PaletteToolProperty,
                Sheet::ToolSettings,
                tr!("tool-settings"),
            ),
            (Icon::PaletteColorWheel, Sheet::Color, tr!("color-2")),
            (
                Icon::PaletteMaterials,
                Sheet::Materials,
                tr!("start-manage-materials"),
            ),
            (Icon::PaletteTimeline, Sheet::Timeline, tr!("timeline")),
            (
                Icon::PaletteSubView,
                Sheet::Reference,
                tr!("simple-reference"),
            ),
        ];
        for (index, (icon, sheet, name)) in sheets.iter().enumerate().take(fits(palettes)) {
            if ui::icon_button(("sue-paint-studio-sheet", index), *icon, name)
                .selected(self.sheet == Some(*sheet))
                .show_at(ui, slot(palettes, index), theme)
                .clicked()
            {
                self.toggle_sheet(*sheet);
            }
        }
        if ui::icon_button("sue-paint-studio-back", Icon::ArrowLeft, &tr!("start-back"))
            .show_at(ui, slot(commands, 0), theme)
            .clicked()
        {
            cx.request(ShellRequest::Hub);
            self.sheet = None;
        }
        let facts = Facts::of(cx);
        let room = fits(commands).saturating_sub(1);
        for (index, command) in COMMAND_BAR.iter().enumerate().take(room) {
            if self.command_button(ui, slot(commands, index + 1), command, &facts, theme) {
                *run = Some((*command).to_owned());
            }
        }
    }

    /// The tool column: the menu, swap, pen, lasso, object, then size and
    /// opacity on tall sliders, and the color at the bottom.
    fn tool_column(&mut self, ui: &mut Ui, column: Rect, theme: &Theme) {
        ui.painter()
            .rect_filled(column, CornerRadius::ZERO, theme.panel);
        sue_wm::cover::cover(ui.ctx(), ui.layer_id(), column);
        ui.painter()
            .vline(column.right(), column.y_range(), theme.hairline());
        let button = Density::Touch.icon_button();
        let (gap, pad) = (theme.space(Space::S1), theme.space(Space::S2));
        let x = column.center().x;
        let mut y = column.top() + pad;
        let mut next = |height: f32| {
            let rect = Rect::from_min_size(pos2(x - button / 2.0, y), vec2(button, height));
            y += height + gap;
            rect
        };
        let tool = self.toolbox.tool();
        let pen = if tool.paints() {
            tool
        } else {
            self.drawing_tool
        };
        if ui::icon_button("sue-paint-studio-menu", Icon::Menu, &tr!("menus-2"))
            .selected(self.sheet == Some(Sheet::Menu))
            .show_at(ui, next(button), theme)
            .clicked()
        {
            self.toggle_sheet(Sheet::Menu);
        }
        if ui::icon_button("sue-paint-studio-swap", Icon::Swap, &tr!("simple-swap"))
            .show_at(ui, next(button), theme)
            .clicked()
        {
            self.swap_eraser();
        }
        if ui::icon_button("sue-paint-studio-pen", tool_glyph(pen), &tr!("simple-pens"))
            .selected(tool == pen)
            .show_at(ui, next(button), theme)
            .clicked()
        {
            if tool == pen {
                // Every tool, as on the desktop.
                self.toggle_sheet(Sheet::Tools);
            } else {
                self.toolbox.pick(pen);
            }
        }
        for (picked, icon, name) in [
            (Tool::Select, Icon::ToolSelectLasso, tr!("studio-lasso")),
            (Tool::Operation, Icon::ToolOperation, tr!("studio-object")),
        ] {
            if ui::icon_button(("sue-paint-studio-tool", picked.id()), icon, &name)
                .selected(tool == picked)
                .enabled(picked.available())
                .show_at(ui, next(button), theme)
                .clicked()
            {
                self.toolbox.pick(picked);
            }
        }
        // Size and opacity: the number over a tall slider, sharing what
        // is left above the color.
        // Five buttons above.
        let used = pad + (button + gap) * 5.0;
        let caption = theme.text_size(TextRole::Caption) * 2.0 + gap;
        let left = (column.height() - used - button - pad * 2.0).max(button * 3.0);
        let lane = (left / 2.0 - caption - gap).max(button);
        let tip = &mut self.toolbox.current_mut().tip;
        let mut size = f64::from(tip.size);
        let mut opacity = f64::from(tip.opacity) * 100.0;
        for (index, value) in [&mut size, &mut opacity].into_iter().enumerate() {
            let (name, text) = if index == 0 {
                (tr!("size-2"), format!("{:.1}\n{}", *value, tr!("px")))
            } else {
                (tr!("opacity"), format!("{:.0}\n%", *value))
            };
            let label = next(caption);
            ui.painter().text(
                label.center(),
                Align2::CENTER_CENTER,
                text,
                theme.text_font(TextRole::Caption),
                theme.text,
            );
            let slider = if index == 0 {
                ui::tall_slider(("sue-paint-studio-slider", index), value, 0.5..=1000.0)
                    .logarithmic()
                    .decimals(1)
            } else {
                ui::tall_slider(("sue-paint-studio-slider", index), value, 0.0..=100.0).fading()
            };
            let response = slider.label(&name).show_at(ui, next(lane), theme);
            let _ = ui::tooltip(response, &Tip::new(&name), theme);
        }
        tip.size = size as f32;
        tip.opacity = (opacity / 100.0) as f32;
        let color = Rect::from_center_size(
            pos2(x, column.bottom() - button / 2.0 - pad),
            Vec2::splat(button),
        );
        self.color_swatch(ui, color, theme);
    }

    /// Swaps the drawing tool for the eraser, and back.
    pub(crate) fn swap_eraser(&mut self) {
        let tool = self.toolbox.tool();
        if tool == Tool::Eraser {
            self.toolbox.pick(self.drawing_tool);
        } else {
            if tool.paints() {
                self.drawing_tool = tool;
            }
            self.toolbox.pick(Tool::Eraser);
        }
    }

    /// The edge key bar's keys: what each says, as a key of the bar.
    fn key_labels() -> Vec<String> {
        let mut keys = vec![
            tr!("studio-key-esc"),
            tr!("studio-key-ctrl"),
            tr!("studio-key-shift"),
            tr!("studio-key-alt"),
            tr!("studio-key-space"),
        ];
        keys.extend((0..CUSTOM_KEYS).map(|index| format!("T{}", index + 1)));
        keys
    }

    /// How wide the edge key bar is: its widest key, and its margins.
    fn key_bar_width(&self, ui: &Ui, theme: &Theme) -> f32 {
        let density = Density::Touch;
        let widest = Self::key_labels()
            .iter()
            .map(|label| {
                ui::button("sue-paint-key-measure", label)
                    .size(ui.painter(), theme, density)
                    .x
            })
            .fold(density.icon_button(), f32::max);
        widest + theme.space(Space::S2) * 2.0
    }

    /// The edge key bar over `area` (the canvas with the room a pinned
    /// bar takes), the swipes in from its edges, and the tab that shows
    /// where the bar is while it is away. `canvas` is what the canvas
    /// has of `area`.
    fn key_bar(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        canvas: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        self.key_bar_body(ui, area, canvas, cx, theme, run);
        // Over the bar: a swipe in from its edge pins a see-through bar.
        self.edge_swipes(ui, canvas, theme);
    }

    /// The bar itself, or its tab while it is away.
    fn key_bar_body(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        canvas: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let width = self.key_bar_width(ui, theme);
        let button = Density::Touch.icon_button();
        if self.touch.bar == KeyBar::Hidden {
            // The tab at the edge: a tap brings the bar in.
            let (x, icon) = match self.touch.edge {
                Edge::Left => (canvas.left(), Icon::ChevronRight),
                Edge::Right => (canvas.right() - button, Icon::ChevronLeft),
            };
            let tab = Rect::from_min_size(
                pos2(x, canvas.center().y - button / 2.0),
                Vec2::splat(button),
            );
            sue_wm::cover::cover(ui.ctx(), ui.layer_id(), tab);
            if ui::icon_button("sue-paint-key-tab", icon, &tr!("edge-key-bar"))
                .kind(ui::ButtonKind::Secondary)
                .shortcut(&self.keys_of("touch.key_bar"))
                .show_at(ui, tab, theme)
                .clicked()
            {
                self.touch.bar = KeyBar::Peek;
            }
            return;
        }
        let bar = match self.touch.edge {
            Edge::Left => Rect::from_min_max(area.min, pos2(area.left() + width, area.bottom())),
            Edge::Right => Rect::from_min_max(pos2(area.right() - width, area.top()), area.max),
        };
        let peek = self.touch.bar == KeyBar::Peek;
        let mut keys = ui.new_child(egui::UiBuilder::new().max_rect(bar));
        if peek {
            keys.multiply_opacity(SEE_THROUGH);
        }
        keys.painter()
            .rect_filled(bar, CornerRadius::ZERO, theme.panel);
        sue_wm::cover::cover(ui.ctx(), ui.layer_id(), bar);
        let inner = match self.touch.edge {
            Edge::Left => bar.right(),
            Edge::Right => bar.left(),
        };
        keys.painter().vline(inner, bar.y_range(), theme.hairline());
        // A swipe back out over the bar puts it away.
        let away = keys.interact(
            bar,
            keys.scope_id().with("sue-paint-key-bar"),
            Sense::drag(),
        );
        if away.drag_stopped() {
            let moved = away.drag_delta();
            let out = match self.touch.edge {
                Edge::Left => -moved.x,
                Edge::Right => moved.x,
            };
            if out > 0.0 {
                self.touch.bar = KeyBar::Hidden;
            }
        }
        if let (Some(from), Some(at)) = (
            ui.input(|input| input.pointer.press_origin()),
            away.interact_pointer_pos(),
        ) && away.dragged()
        {
            let out = match self.touch.edge {
                Edge::Left => from.x - at.x,
                Edge::Right => at.x - from.x,
            };
            if out >= theme.space(SWIPE) {
                self.touch.bar = KeyBar::Hidden;
                return;
            }
        }
        let (gap, pad) = (theme.space(Space::S1), theme.space(Space::S2));
        let labels = Self::key_labels();
        // The pin and the close button, the keys, then the button that
        // chooses what T1 to T7 run.
        let count = labels.len() + 3;
        let height = ((bar.height() - pad * 2.0) / count as f32 - gap).clamp(
            Density::Compact.control_height(),
            Density::Touch.control_height(),
        );
        let mut y = bar.top() + pad;
        let mut next = || {
            let rect = Rect::from_min_max(
                pos2(bar.left() + pad, y),
                pos2(bar.right() - pad, y + height),
            );
            y += height + gap;
            (rect.bottom() <= bar.bottom() - pad).then_some(rect)
        };
        let close_icon = match self.touch.edge {
            Edge::Left => Icon::ChevronLeft,
            Edge::Right => Icon::ChevronRight,
        };
        if let Some(rect) = next()
            && ui::icon_button("sue-paint-key-pin", Icon::Pin, &tr!("edge-key-bar-pin"))
                .selected(!peek)
                .show_at(&keys, rect, theme)
                .clicked()
        {
            self.touch.bar = if peek { KeyBar::Pinned } else { KeyBar::Peek };
        }
        if let Some(rect) = next()
            && ui::icon_button("sue-paint-key-close", close_icon, &tr!("edge-key-bar-hide"))
                .show_at(&keys, rect, theme)
                .clicked()
        {
            self.touch.bar = KeyBar::Hidden;
        }
        for (index, label) in labels.iter().enumerate() {
            let Some(rect) = next() else { break };
            let latched = match index {
                1 => Some(self.touch.ctrl),
                2 => Some(self.touch.shift),
                3 => Some(self.touch.alt),
                4 => Some(self.touch.space),
                _ => None,
            };
            let key = match latched {
                Some(on) => ui::toggle_button(("sue-paint-key", index), label, on),
                None => ui::button(("sue-paint-key", index), label),
            };
            let custom = index.checked_sub(5);
            let hint = custom
                .map(|custom| {
                    let command = self.custom_command(custom, cx);
                    crate::commands::registry()
                        .label(&command)
                        .unwrap_or(command)
                })
                .unwrap_or_default();
            let response = key.show_at(&keys, rect, theme);
            let response = ui::tooltip(response, &Tip::new(label).help(&hint), theme);
            if response.clicked() {
                match index {
                    0 => {
                        self.touch.release();
                        self.sheet = None;
                        *run = Some("select.none".to_owned());
                    }
                    1 => self.touch.ctrl = !self.touch.ctrl,
                    2 => self.touch.shift = !self.touch.shift,
                    3 => self.touch.alt = !self.touch.alt,
                    4 => self.touch.space = !self.touch.space,
                    _ => *run = custom.map(|custom| format!("touch.custom.{custom}")),
                }
            }
        }
        if let Some(rect) = next()
            && ui::icon_button(
                "sue-paint-key-settings",
                Icon::Settings,
                &tr!("edge-key-bar-choose"),
            )
            .show_at(&keys, rect, theme)
            .clicked()
        {
            self.open_touch_settings(cx);
        }
    }

    /// The swipes in from the left and right edges of `canvas`.
    fn edge_swipes(&mut self, ui: &Ui, canvas: Rect, theme: &Theme) {
        let reach = theme.space(EDGE);
        for edge in [Edge::Left, Edge::Right] {
            // A pinned bar's own side has no canvas edge to swipe from.
            if self.touch.bar == KeyBar::Pinned && self.touch.edge == edge {
                continue;
            }
            let strip = match edge {
                Edge::Left => {
                    Rect::from_min_max(canvas.min, pos2(canvas.left() + reach, canvas.bottom()))
                }
                Edge::Right => {
                    Rect::from_min_max(pos2(canvas.right() - reach, canvas.top()), canvas.max)
                }
            };
            sue_wm::cover::cover(ui.ctx(), ui.layer_id(), strip);
            let response = ui.interact(
                strip,
                ui.scope_id().with(("sue-paint-edge", edge)),
                Sense::drag(),
            );
            if response.drag_started()
                && let Some(from) = ui.input(|input| input.pointer.press_origin())
            {
                self.touch.swipe = Some((edge, from.x));
            }
            if response.dragged()
                && let (Some((from_edge, from)), Some(at)) =
                    (self.touch.swipe, response.interact_pointer_pos())
                && from_edge == edge
            {
                let inward = match edge {
                    Edge::Left => at.x - from,
                    Edge::Right => from - at.x,
                };
                if inward >= theme.space(SWIPE) {
                    self.touch.swipe = None;
                    self.touch.swiped_in(edge);
                }
            }
            if response.drag_stopped() {
                self.touch.swipe = None;
            }
        }
    }

    /// Opens Settings on Sue Paint's page, where the touch settings and
    /// the commands of T1 to T7 are.
    pub(crate) fn open_touch_settings(&mut self, cx: &mut AppletContext<'_>) {
        cx.request(ShellRequest::OpenSettings(Some(
            crate::studio::ID.to_owned(),
        )));
        self.sheet = None;
    }

    /// The command custom key `index` runs, as the rice has it.
    pub(crate) fn custom_command(&self, index: usize, cx: &AppletContext<'_>) -> String {
        cx.rice()
            .get_str(&custom_key(index))
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .unwrap_or(CUSTOM_DEFAULTS.get(index).copied().unwrap_or(""))
            .to_owned()
    }

    /// Studio Mode's Menu sheet: home, Simple Mode and Settings, then Sue
    /// Paint's menus.
    pub(crate) fn studio_menu(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let rows = [
            (Icon::Home, tr!("studio-return-home")),
            (Icon::LayoutStyle, tr!("studio-to-simple")),
            (Icon::Settings, tr!("studio-app-settings")),
        ];
        for (index, (icon, title)) in rows.iter().enumerate() {
            if ui::list_row(("sue-paint-studio-menu", index), title)
                .icon(*icon)
                .show(ui, theme)
                .response
                .clicked()
            {
                match index {
                    0 => {
                        cx.request(ShellRequest::Hub);
                        self.sheet = None;
                    }
                    1 => {
                        crate::simple::set_mode(cx, false);
                        self.sheet = None;
                    }
                    _ => self.open_touch_settings(cx),
                }
            }
        }
        ui::divider(ui, theme);
        self.menu_list(ui, cx, theme, run);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latched_keys_count_as_held_until_let_go() {
        let mut touch = Touch {
            ctrl: true,
            shift: true,
            space: true,
            ..Touch::default()
        };
        let held = touch.modifiers();
        assert!(held.ctrl && held.command && held.shift && !held.alt);
        touch.release();
        assert_eq!(touch.modifiers(), egui::Modifiers::NONE);
        assert!(!touch.space);
        assert_eq!(custom_key(0), "sue_paint.custom_key_1");
        assert_eq!(CUSTOM_DEFAULTS.len(), CUSTOM_KEYS);
    }

    #[test]
    fn a_swipe_brings_the_bar_in_see_through_and_a_second_pins_it() {
        let mut touch = Touch::default();
        assert_eq!(touch.bar, KeyBar::Hidden);
        touch.swiped_in(Edge::Left);
        assert_eq!((touch.bar, touch.edge), (KeyBar::Peek, Edge::Left));
        // From the other side: it moves there, still see-through.
        touch.swiped_in(Edge::Right);
        assert_eq!((touch.bar, touch.edge), (KeyBar::Peek, Edge::Right));
        touch.swiped_in(Edge::Right);
        assert_eq!((touch.bar, touch.edge), (KeyBar::Pinned, Edge::Right));
        touch.swiped_in(Edge::Right);
        assert_eq!(touch.bar, KeyBar::Pinned, "it stays pinned");
    }

    #[test]
    fn gestures_follow_the_settings_and_default_on() {
        let mut rice = RiceSchema::new();
        rice.register_all(&declarations()).unwrap();
        let all = Gestures::of(&rice);
        assert!(all.finger_draws && all.swipe && all.pinch && all.rotate);
        rice.set(FINGER_KEY, RiceValue::Bool(false)).unwrap();
        rice.set(PINCH_KEY, RiceValue::Text("none".to_owned()))
            .unwrap();
        let some = Gestures::of(&rice);
        assert!(!some.finger_draws && some.swipe && !some.pinch && some.rotate);
    }

    #[test]
    fn a_touch_with_force_is_the_pen_where_the_system_does_not_say() {
        assert!(is_finger(9_001, None));
        assert!(!is_finger(9_002, Some(0.4)));
        // The system's word wins over the force.
        sue_platform::stylus::mark_touch_for_test(9_003, false);
        assert!(is_finger(9_003, Some(0.4)));
        sue_platform::stylus::mark_touch_for_test(9_004, true);
        assert!(!is_finger(9_004, None));
        sue_platform::stylus::forget_for_test();
    }

    #[test]
    fn every_tool_has_an_icon_of_its_own_command() {
        for tool in Tool::ALL {
            let icon = tool_glyph(tool);
            assert!(
                icon.name().starts_with("tool-"),
                "{}: {}",
                tool.id(),
                icon.name()
            );
        }
        assert_eq!(tool_glyph(Tool::Pencil), Icon::ToolPencil);
        assert_eq!(tool_glyph(Tool::Frame), Icon::ToolFrame);
    }
}
