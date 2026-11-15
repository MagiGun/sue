//! The Layer palette and the thumbnail of a layer.
//!
//! Its parts, top to bottom: the blend mode and opacity of the current
//! layer; two rows of buttons, each a command of the Layer menu (its icon,
//! its name and key in the tooltip, grayed with the reason when it cannot
//! run); the search bar while searching; and the list of layers, in one
//! pane or two. Each row holds, from the left: the palette color, the eye,
//! the pen on the layer being edited or a box that picks it with others,
//! the thumbnail, the mask and its link, the mode and opacity over the
//! name, the effect badges and the lock.

use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2};
use sue_core::{EntityId, Value};
use sue_model::BlendMode;
use sue_model::picture::{self, LayerKind};
use sue_wm::icons::Icon as WmIcon;
use sue_wm::ui::tokens::{Density, Radius, Space, TextRole};
use sue_wm::ui::tooltip::{Tip, tooltip};
use sue_wm::{AppletContext, Facts, Theme};

use super::search_layer::SearchLayer;
use crate::doc;
use crate::icons::Icon;
use crate::panels::child;
use crate::studio::{KEYS_SECTION, SuePaint, texture_key};

/// How strongly a palette color tints its cells of a row: a label that
/// must not drown the row's text.
const TINT: f32 = 0.45;

/// How see-through the copy of a dragged row is.
const GHOST: f32 = 0.7;

/// The size of the question asked before folders are deleted (until the
/// dialog host sizes dialogs by their content).
pub(crate) const DELETE_FOLDERS_SIZE: egui::Vec2 = egui::Vec2::new(380.0, 170.0);

/// What the Layer palette keeps between frames.
#[derive(Clone, Debug, Default)]
pub(crate) struct LayerPalette {
    /// The list shows in two panes, one over the other, each scrolled on
    /// its own (Clip Studio Paint's Split Layer List).
    pub two_panes: bool,
    /// Searching, and for what (Search Layer).
    pub search: Option<SearchLayer>,
    /// The palette color menu is open.
    pub color_menu: bool,
    /// The ruler menu is open.
    pub ruler_menu: bool,
    /// Where the buttons that take a dragged layer were last drawn: New
    /// Raster Layer (a copy) and New Layer Folder (wrapped in one).
    pub(crate) drop_targets: Vec<(Rect, &'static str)>,
    /// Where every button of the palette was last drawn, with its command.
    pub(crate) buttons: Vec<(Rect, &'static str)>,
}

/// The switches over the list, as Clip Studio Paint has them: each a
/// command, or one of the palette's own (`palette.*`).
const SWITCHES: [&str; 12] = [
    "layer.toggle.clip",
    "layer.toggle.reference",
    "layer.toggle.draft",
    "layer.toggle.lock",
    "layer.toggle.lock_alpha",
    "layer.mask.toggle",
    "palette.mask_link",
    "palette.color",
    "layer.toggle.layer_color",
    "palette.ruler",
    "palette.two_panes",
    "palette.search",
];

/// The actions over the list.
const ACTIONS: [&str; 9] = [
    "layer.new",
    "layer.new.vector",
    "layer.new.folder",
    "layer.duplicate",
    "layer.transfer_down",
    "layer.merge_down",
    "layer.mask.outside",
    "layer.mask.apply",
    "layer.delete",
];

/// What happened in the list this frame, done once it is drawn.
#[derive(Default)]
struct RowEvents {
    /// A plain click: the layer becomes the only one picked.
    pick: Option<EntityId>,
    /// Ctrl+click or the box: the layer joins the picked ones or leaves.
    also: Option<EntityId>,
    /// Shift+click: every layer from the current one to it.
    range: Option<EntityId>,
    /// Ctrl+click on the thumbnail: a selection of what it holds.
    select_from: Option<EntityId>,
    /// A cel of an animation folder was clicked: the playhead goes where it
    /// starts.
    cel: Option<(usize, u32)>,
    toggle: Option<(EntityId, bool)>,
    rename: Option<(EntityId, String, bool)>,
    fold: Option<(EntityId, bool)>,
    mask: Option<EntityId>,
    link: Option<(EntityId, bool)>,
    reopen: Option<(EntityId, crate::filters::Filter, String)>,
    dragging: Option<EntityId>,
    dropped: Option<EntityId>,
    onto: Option<EntityId>,
    menu: Option<String>,
    /// Where the pointer is.
    pointer: Option<Pos2>,
}

/// The sizes of the palette, from the theme's tokens.
struct Metrics {
    gap: f32,
    pad: f32,
    /// Twice the pad: around the dragged row's copy.
    wide: f32,
    /// A folder's picture in its row: two icons high.
    folder: f32,
    button: f32,
    icon: f32,
    row: f32,
    indent: f32,
    thumb: egui::Vec2,
}

impl Metrics {
    fn of(theme: &Theme) -> Self {
        let density = Density::Compact;
        let row = density.row() * 2.0;
        let pad = theme.space(Space::S2);
        let high = row - pad * 2.0;
        Self {
            gap: theme.space(Space::S1),
            pad,
            wide: theme.space(Space::S4),
            folder: density.icon() * 2.0,
            button: density.icon_button(),
            icon: density.icon(),
            row,
            indent: theme.space(Space::S5),
            thumb: egui::Vec2::new(high * 4.0 / 3.0, high),
        }
    }
}

/// The icon of the shared set named `name`, or a plain one.
fn wm_icon(name: &str) -> WmIcon {
    WmIcon::from_name(name).unwrap_or(WmIcon::Layers)
}

/// The keys that run `command` now, as Settings shows them; empty for
/// none.
pub(super) fn keys_of(cx: &AppletContext<'_>, command: &str) -> String {
    crate::commands::shortcut_table()
        .iter()
        .find(|shortcut| shortcut.command == command)
        .map(|shortcut| sue_wm::shortcuts::chords(cx.rice(), KEYS_SECTION, shortcut).to_string())
        .unwrap_or_default()
}

/// A square button of the palette at `rect`: the icon, the name and keys
/// in its tooltip with `why` under them, read out by name. Returns its
/// response.
#[allow(clippy::too_many_arguments, reason = "one button and all it says")]
fn palette_button(
    ui: &Ui,
    rect: Rect,
    icon: WmIcon,
    name: &str,
    keys: &str,
    why: &str,
    on: Option<bool>,
    enabled: bool,
    theme: &Theme,
) -> egui::Response {
    let mut button = sue_wm::ui::icon_button(("sue-paint-layer-button", name), icon, "")
        .enabled(enabled)
        .quiet();
    if let Some(on) = on {
        button = button.selected(on);
    }
    let response = button.show_at(ui, rect, theme);
    response.widget_info(|| match on {
        Some(on) => egui::WidgetInfo::selected(egui::Role::Button, enabled, on, name),
        None => egui::WidgetInfo::labeled(egui::Role::Button, enabled, name),
    });
    tooltip(response, &Tip::new(name).shortcut(keys).help(why), theme)
}

impl SuePaint {
    /// Why `command` cannot run now, in the words of its tooltip; empty
    /// when it can.
    fn why_not(&self, command: &str, arena: &sue_core::Arena) -> String {
        let info = self.layer_info(arena);
        let folder = info
            .as_ref()
            .is_some_and(|info| doc::is_folder(arena, info.id));
        let paper = info
            .as_ref()
            .is_some_and(|info| info.kind == LayerKind::Paper);
        match command {
            _ if info.is_none() => tr!("why-pick-a-layer"),
            "layer.mask.toggle" | "layer.mask.apply" | "palette.mask_link" => {
                tr!("why-needs-a-mask")
            }
            "layer.mask.outside" if paper => tr!("why-not-on-paper"),
            "layer.mask.outside" => tr!("why-has-a-mask"),
            "layer.merge_down" | "layer.transfer_down" if paper => tr!("why-not-on-paper"),
            "layer.merge_down" | "layer.transfer_down" => tr!("why-needs-raster-below"),
            "layer.merge_selected" => tr!("why-pick-two-layers"),
            "layer.rasterize" => tr!("why-nothing-to-rasterize"),
            "layer.convert" if folder => tr!("why-not-on-a-folder"),
            "layer.convert" => tr!("why-needs-raster-or-vector"),
            "layer.delete" => tr!("why-last-layer"),
            _ if folder => tr!("why-not-on-a-folder"),
            _ if paper => tr!("why-not-on-paper"),
            _ => tr!("why-needs-a-raster-layer"),
        }
    }

    /// How a palette button stands now: its name, icon, keys, whether it
    /// is on and whether it works, and why not.
    fn button_state(
        &self,
        command: &str,
        cx: &AppletContext<'_>,
    ) -> (String, WmIcon, String, Option<bool>, bool) {
        let arena = &cx.world.arena;
        let current = self.layer();
        let mask = current.and_then(|layer| doc::mask_of(arena, layer));
        match command {
            "palette.mask_link" => (
                if current.is_some_and(|layer| doc::mask_linked(arena, layer)) {
                    tr!("unlink-mask-from-layer")
                } else {
                    tr!("link-mask-to-layer")
                },
                WmIcon::Link,
                String::new(),
                Some(current.is_some_and(|layer| mask.is_some() && doc::mask_linked(arena, layer))),
                mask.is_some(),
            ),
            "palette.color" => (
                tr!("change-palette-color"),
                WmIcon::PaletteColorSet,
                String::new(),
                Some(self.layer_palette.color_menu),
                current.is_some(),
            ),
            "palette.ruler" => (
                tr!("set-showing-area-of-ruler"),
                WmIcon::ToolRuler,
                keys_of(cx, "layer.ruler.show"),
                Some(
                    self.layer_palette.ruler_menu || self.doc().is_some_and(|doc| doc.show_rulers),
                ),
                true,
            ),
            "palette.two_panes" => (
                tr!("split-layer-list"),
                WmIcon::ViewList,
                String::new(),
                Some(self.layer_palette.two_panes),
                true,
            ),
            "palette.search" => (
                tr!("search-layer"),
                WmIcon::Search,
                String::new(),
                Some(self.layer_palette.search.is_some()),
                true,
            ),
            _ => {
                let registry = crate::commands::registry();
                let state = self.state(command, &Facts::of(cx));
                let icon = registry
                    .find(command)
                    .map_or(WmIcon::Layers, |(found, _)| wm_icon(found.icon));
                (
                    registry
                        .label(command)
                        .unwrap_or_else(|| command.to_owned()),
                    icon,
                    keys_of(cx, command),
                    state.checked,
                    state.enabled,
                )
            }
        }
    }

    /// Lays out and draws the palette's buttons from `top`, wrapping to a
    /// new row when one is full. Returns the command of a clicked button
    /// and where the buttons end.
    fn palette_buttons(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        top: f32,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) -> (Option<&'static str>, f32) {
        let metrics = Metrics::of(theme);
        let step = metrics.button + metrics.gap;
        let mut clicked = None;
        let (mut x, mut y) = (body.left() + metrics.pad, top);
        let mut buttons = Vec::new();
        let mut targets = Vec::new();
        for (row, commands) in [&SWITCHES[..], &ACTIONS[..]].into_iter().enumerate() {
            if row > 0 {
                x = body.left() + metrics.pad;
                y += step;
            }
            for command in commands {
                if x + metrics.button > body.right() - metrics.pad && x > body.left() + metrics.pad
                {
                    x = body.left() + metrics.pad;
                    y += step;
                }
                let rect = Rect::from_min_size(pos2(x, y), egui::Vec2::splat(metrics.button));
                x += step;
                let (name, icon, keys, on, enabled) = self.button_state(command, cx);
                let why = if enabled {
                    String::new()
                } else {
                    self.why_not(command, &cx.world.arena)
                };
                let response =
                    palette_button(ui, rect, icon, &name, &keys, &why, on, enabled, theme);
                if response.clicked() && enabled {
                    clicked = Some(*command);
                }
                if matches!(*command, "layer.new" | "layer.new.folder") {
                    targets.push((rect, *command));
                }
                buttons.push((rect, *command));
            }
        }
        self.layer_palette.drop_targets = targets;
        self.layer_palette.buttons = buttons;
        (clicked, y + step)
    }

    /// The blend mode and opacity of the current layer, in `area`.
    fn blend_and_opacity(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        current: Option<&doc::LayerInfo>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let arena = &cx.world.arena;
        let folder = current.is_some_and(|info| doc::is_folder(arena, info.id));
        let modes: &[BlendMode] = if folder {
            &BlendMode::FOLDER
        } else {
            &BlendMode::ALL
        };
        let names: Vec<String> = modes
            .iter()
            .map(|mode| sue_lang::translate("model", mode.label()))
            .collect();
        let blend = current.map_or(BlendMode::Normal, |info| info.style.blend);
        let mut chosen = modes.iter().position(|mode| *mode == blend).unwrap_or(0);
        let can_blend = current.is_some_and(|info| info.kind != LayerKind::Paper);
        let gap = theme.space(Space::S2);
        let field = theme.space(Space::S9) + theme.space(Space::S6);
        let split = area.left() + (area.width() * 0.45).max(field);
        let blend_rect = Rect::from_min_max(area.min, pos2(split - gap, area.bottom()));
        let wedge_rect = Rect::from_min_max(
            pos2(split, area.top()),
            pos2(area.right() - field - gap, area.bottom()),
        );
        let number_rect = Rect::from_min_max(pos2(area.right() - field, area.top()), area.max);
        let mut opacity = current.map_or(100.0, |info| f64::from(info.style.opacity) * 100.0);
        let (changed, wedge, typed) = Density::Compact.scope(ui, |ui| {
            let label = tr!("blending-mode");
            let changed = sue_wm::ui::dropdown("sue-paint-blend", &mut chosen, &names)
                .label(&label)
                .enabled(can_blend)
                .show_at(ui, blend_rect, theme)
                .changed();
            let opacity_name = tr!("opacity");
            let wedge = sue_wm::ui::wedge("sue-paint-layer-opacity", &mut opacity, 0.0..=100.0)
                .label(&opacity_name)
                .enabled(current.is_some())
                .show_at(ui, wedge_rect, theme);
            let typed = sue_wm::ui::number_field("sue-paint-layer-opacity-number", &mut opacity)
                .range(0.0..=100.0)
                .decimals(0)
                .suffix("%")
                .label(&opacity_name)
                .enabled(current.is_some())
                .show_at(ui, number_rect, theme);
            (changed, wedge, typed)
        });
        let Some(info) = current else {
            return;
        };
        // The wheel over the list steps it, and each step shows on the
        // canvas at once (the dropdown changes as it is stepped).
        if changed && let Some(mode) = modes.get(chosen) {
            Self::apply(cx, doc::set(info.id, picture::BLEND, mode.name()));
        }
        let response = if typed.changed() || typed.lost_focus() {
            typed
        } else {
            wedge
        };
        self.live_property(
            cx,
            &response,
            info.id,
            picture::OPACITY,
            Value::Float(opacity / 100.0),
        );
    }

    #[allow(clippy::too_many_lines, reason = "the layer palette's tabs")]
    pub(crate) fn layer_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::dock::Palette;
        let tabs = [
            (tr!("layer-2"), Icon::Layers, true, None),
            (
                tr!("layer-property"),
                Icon::LayerProperty,
                true,
                Some(Palette::LayerProperty),
            ),
            (tr!("history"), Icon::History, true, Some(Palette::History)),
            (tr!("animation-cels"), Icon::Cel, true, Some(Palette::Cels)),
        ];
        let (body, tab) = self.torn_header(ui, rect, &tabs, self.layer_tab, theme);
        self.layer_tab = tab;
        let Some(picture_id) = self.picture() else {
            ui.painter().text(
                body.center(),
                Align2::CENTER_CENTER,
                tr!("no-canvas"),
                theme.text_font(TextRole::Caption),
                theme.text_muted(),
            );
            return;
        };
        let current = self.layer_info(&cx.world.arena);
        match self.layer_tab {
            1 => {
                let pad = theme.space(Space::S4);
                let mut inner = child(ui, body.shrink(pad), "layer-property");
                self.layer_property(&mut inner, current.as_ref(), cx, theme);
                return;
            }
            2 => {
                self.history_tab(ui, body, cx, theme);
                return;
            }
            3 => {
                self.cels_tab(ui, body, cx, theme);
                return;
            }
            _ => {}
        }
        let metrics = Metrics::of(theme);
        let control = Density::Compact.control_height();
        let top = Rect::from_min_size(
            body.min + egui::Vec2::splat(metrics.pad),
            egui::Vec2::new(body.width() - metrics.pad * 2.0, control),
        );
        self.blend_and_opacity(ui, top, current.as_ref(), cx, theme);
        let (clicked, below) =
            self.palette_buttons(ui, body, top.bottom() + metrics.pad, cx, theme);
        let anchor = self
            .layer_palette
            .buttons
            .iter()
            .find(|(_, command)| *command == "palette.color")
            .map(|(rect, _)| *rect);
        if let Some(command) = clicked {
            self.palette_command(command, cx);
            return;
        }
        if let Some(anchor) = anchor {
            self.palette_color_menu(ui, anchor, cx, theme);
        }
        let ruler = self
            .layer_palette
            .buttons
            .iter()
            .find(|(_, command)| *command == "palette.ruler")
            .map(|(rect, _)| *rect);
        if let Some(anchor) = ruler {
            self.ruler_menu(ui, anchor, cx, theme);
        }
        // The search bar, then the list, top layer first.
        let mut list_top = below;
        ui.painter()
            .hline(body.x_range(), list_top, theme.hairline());
        let searched = Rect::from_min_max(pos2(body.left(), list_top), body.max);
        list_top += self.search_layer_bar(ui, searched, theme);
        let list = Rect::from_min_max(pos2(body.left(), list_top), body.max);
        let stack = doc::layers(&cx.world.arena, picture_id);
        let mut events = RowEvents {
            pointer: ui.ctx().pointer_latest_pos(),
            ..RowEvents::default()
        };
        if self.layer_palette.two_panes && list.height() > metrics.row * 2.0 {
            let middle = list.center().y;
            let upper = Rect::from_min_max(list.min, pos2(list.right(), middle));
            let lower = Rect::from_min_max(pos2(list.left(), middle), list.max);
            self.layer_rows(ui, upper, "upper", &stack, cx, theme, &mut events);
            ui.painter().hline(list.x_range(), middle, theme.hairline());
            self.layer_rows(ui, lower, "lower", &stack, cx, theme, &mut events);
        } else {
            self.layer_rows(ui, list, "one", &stack, cx, theme, &mut events);
        }
        self.drag_feedback(ui, &stack, theme);
        self.row_events(events, &stack, cx);
    }

    /// Runs a button of the palette: a command, or a switch of its own.
    fn palette_command(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "palette.mask_link" => {
                if let Some(layer) = self.layer() {
                    let linked = doc::mask_linked(&cx.world.arena, layer);
                    if let Some(edit) = doc::set_mask_linked(&cx.world.arena, layer, !linked) {
                        Self::apply(cx, edit);
                    }
                }
            }
            "palette.color" => self.layer_palette.color_menu = !self.layer_palette.color_menu,
            "palette.ruler" => self.layer_palette.ruler_menu = !self.layer_palette.ruler_menu,
            "palette.two_panes" => self.layer_palette.two_panes = !self.layer_palette.two_panes,
            "palette.search" => {
                self.layer_palette.search = match self.layer_palette.search.take() {
                    Some(_) => None,
                    None => Some(SearchLayer::default()),
                };
            }
            _ => self.run(command, cx),
        }
    }

    /// The palette color menu under `anchor`: No Palette Color and each
    /// offered color, for the current and every picked layer.
    fn palette_color_menu(
        &mut self,
        ui: &Ui,
        anchor: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.layer_palette.color_menu {
            return;
        }
        let mut open = true;
        let mut chosen = None;
        let current = self
            .layer()
            .and_then(|layer| doc::palette_color(&cx.world.arena, layer));
        sue_wm::ui::popover(
            ui.ctx(),
            Id::unique("sue-paint-palette-color"),
            anchor,
            &mut open,
            theme,
            |ui| {
                let density = Density::Compact;
                let width = theme.space(Space::S9) * 4.0;
                let options = std::iter::once(("none", None)).chain(
                    doc::PALETTE_COLORS
                        .iter()
                        .map(|(name, rgb)| (*name, Some(*rgb))),
                );
                for (name, rgb) in options {
                    let label = if rgb.is_some() {
                        sue_lang::text("paint", &format!("palette-color-{name}"))
                    } else {
                        tr!("no-palette-color")
                    };
                    let (rect, response) = ui
                        .allocate_exact_size(egui::Vec2::new(width, density.row()), Sense::click());
                    let on = rgb.map(|[r, g, b]| sue_core::Color::srgb8(r, g, b)) == current;
                    if response.hovered() || on {
                        ui.painter().rect_filled(
                            rect,
                            theme.radius(Radius::Sm),
                            if on {
                                theme.fill_selected()
                            } else {
                                theme.fill_hover()
                            },
                        );
                    }
                    let swatch = Rect::from_center_size(
                        pos2(rect.left() + density.row() / 2.0, rect.center().y),
                        egui::Vec2::splat(density.icon()),
                    );
                    let _ = match rgb {
                        Some([r, g, b]) => ui.painter().rect_filled(
                            swatch,
                            theme.radius(Radius::Xs),
                            Color32::from_rgb(r, g, b),
                        ),
                        None => ui.painter().rect_stroke(
                            swatch,
                            theme.radius(Radius::Xs),
                            theme.hairline(),
                            StrokeKind::Inside,
                        ),
                    };
                    ui.painter().text(
                        pos2(rect.left() + density.row(), rect.center().y),
                        Align2::LEFT_CENTER,
                        &label,
                        theme.text_font(TextRole::Body),
                        theme.text,
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Button, true, on, &label)
                    });
                    sue_wm::ui::focus_ring(ui, &response, theme.radius(Radius::Sm), theme);
                    if response.clicked() {
                        chosen = Some(name);
                    }
                }
            },
        );
        if let Some(name) = chosen {
            self.set_palette_color_named(cx, name);
            open = false;
        }
        self.layer_palette.color_menu = open;
    }

    /// The ruler menu under `anchor` (Clip Studio Paint's Set showing area
    /// of ruler): Show Ruler, then where the rulers show; a choice other
    /// than all layers links them to the current layer.
    fn ruler_menu(&mut self, ui: &Ui, anchor: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        if !self.layer_palette.ruler_menu {
            return;
        }
        let Some(picture) = self.picture() else {
            return;
        };
        let mut open = true;
        let mut chosen = None;
        let mut show = false;
        let now = crate::rulers::ShowRange::of(&cx.world.arena, picture);
        let showing = self.doc().is_some_and(|doc| doc.show_rulers);
        sue_wm::ui::popover(
            ui.ctx(),
            Id::unique("sue-paint-ruler-menu"),
            anchor,
            &mut open,
            theme,
            |ui| {
                let label = crate::commands::registry()
                    .label("layer.ruler.show")
                    .unwrap_or_default();
                if sue_wm::ui::checkbox("sue-paint-ruler-show", &label, showing)
                    .show(ui, theme)
                    .clicked()
                {
                    show = true;
                }
                sue_wm::ui::divider(ui, theme);
                for range in crate::rulers::ShowRange::ALL {
                    if sue_wm::ui::radio(
                        ("sue-paint-ruler-range", range.id()),
                        &range.label(),
                        range == now,
                    )
                    .show(ui, theme)
                    .clicked()
                    {
                        chosen = Some(range);
                    }
                }
            },
        );
        if show {
            self.run("layer.ruler.show", cx);
        }
        if let Some(range) = chosen {
            let layer = (range != crate::rulers::ShowRange::AllLayers)
                .then(|| self.layer())
                .flatten();
            Self::apply(cx, range.edit(picture, layer, &range.label()));
        }
        self.layer_palette.ruler_menu = open && chosen.is_none() && !show;
    }

    /// The rows of the list in `area`, scrolled on their own (`salt`).
    #[allow(clippy::too_many_arguments, reason = "one pane of the list")]
    #[allow(clippy::too_many_lines, reason = "one row and all it shows")]
    fn layer_rows(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        salt: &str,
        stack: &[doc::LayerInfo],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        events: &mut RowEvents,
    ) {
        let metrics = Metrics::of(theme);
        let mut list_ui = child(ui, area, &format!("layer-list-{salt}"));
        let menu = self.layer_row_menu();
        let search = self
            .layer_palette
            .search
            .clone()
            .filter(SearchLayer::is_active);
        let frame = self.doc().map_or(0, |doc| doc.frame);
        let sequence = self
            .picture()
            .and_then(|picture| crate::anim::timeline(&cx.world.arena, picture))
            .cloned();
        sue_wm::ui::ScrollView::vertical(("sue-paint-layers", salt)).show(
            &mut list_ui,
            theme,
            |ui| {
                for info in stack.iter().rev() {
                    let arena = &cx.world.arena;
                    let around = doc::folders_around(arena, info.id);
                    // Searching shows every match, wherever it is; else
                    // layers in a closed folder stay out of sight.
                    if let Some(search) = &search {
                        if !search.matches(arena, info.id) {
                            continue;
                        }
                    } else if around.iter().any(|folder| !doc::is_open(arena, *folder)) {
                        continue;
                    }
                    let depth = if search.is_some() { 0 } else { around.len() };
                    let is_folder = doc::is_folder(arena, info.id);
                    // On touch screens a drag scrolls the list instead.
                    let (full, response) = ui.allocate_exact_size(
                        egui::Vec2::new(ui.available_width(), metrics.row),
                        if cx.view.form_factor == sue_platform::FormFactor::Desktop {
                            Sense::click_and_drag()
                        } else {
                            Sense::click()
                        },
                    );
                    if response.drag_started() {
                        events.dragging = Some(info.id);
                    }
                    if response.drag_stopped() {
                        events.dropped = Some(info.id);
                    }
                    let drop_here = self.layer_drag.is_some_and(|dragged| dragged != info.id)
                        && ui
                            .ctx()
                            .pointer_hover_pos()
                            .is_some_and(|pos| full.contains(pos));
                    if drop_here {
                        events.onto = Some(info.id);
                    }
                    let chosen = Some(info.id) == self.layer();
                    let also = self.doc().is_some_and(|doc| doc.picked.contains(&info.id));
                    let painter = ui.painter().clone();
                    if chosen {
                        painter.rect_filled(full, CornerRadius::ZERO, theme.fill_selected());
                    } else if also {
                        painter.rect_filled(full, CornerRadius::ZERO, theme.fill_pressed());
                    } else if response.hovered() {
                        painter.rect_filled(full, CornerRadius::ZERO, theme.fill_hover());
                    }
                    painter.hline(full.x_range(), full.bottom(), theme.hairline());
                    // Where a dragged layer would go: into a folder (the
                    // whole row), or right above a layer (a line on top).
                    if drop_here && is_folder {
                        painter.rect_stroke(
                            full,
                            theme.radius(Radius::Xs),
                            theme.selected_stroke(),
                            StrokeKind::Inside,
                        );
                    } else if drop_here {
                        painter.hline(full.x_range(), full.top(), theme.selected_stroke());
                    }
                    let mid = full.center().y;
                    let cell = metrics.icon + metrics.pad;
                    // The palette color: the eye's and the box's cells
                    // tinted (only a label, the picture never shows it).
                    let tint = doc::palette_color(arena, info.id).map(|color| {
                        let [r, g, b] =
                            crate::color::bytes(color.to_space(sue_core::ColorSpace::Srgb).rgb);
                        Color32::from_rgb(r, g, b)
                    });
                    let lead = Rect::from_min_size(
                        pos2(full.left() + metrics.gap, full.top()),
                        egui::Vec2::new(cell * 2.0, metrics.row),
                    );
                    if let Some(tint) = tint {
                        painter.rect_filled(lead, CornerRadius::ZERO, tint.linear_multiply(TINT));
                    }
                    // A draft layer: a dashed bar at the row's left edge.
                    if info.style.draft {
                        let edge = full.left() + metrics.gap;
                        painter.add(egui::Shape::dashed_line(
                            &[pos2(edge, full.top()), pos2(edge, full.bottom())],
                            Stroke::new(Theme::STRONG_STROKE, theme.text_muted()),
                            metrics.pad,
                            metrics.gap,
                        ));
                    }
                    // Visibility: in an animation folder only the cel now
                    // shown has its eye open.
                    let folder = doc::folder_of(arena, info.id)
                        .filter(|folder| crate::anim::is_animation_folder(arena, *folder));
                    let shown_cel = folder.zip(sequence.as_ref()).map(|(folder, sequence)| {
                        crate::anim::cels::shows_at(sequence, folder, info.id, frame)
                    });
                    let visible = shown_cel.unwrap_or(info.style.visible);
                    let eye = Rect::from_center_size(
                        pos2(lead.left() + cell / 2.0, mid),
                        egui::Vec2::splat(cell),
                    );
                    let eye_name = if visible {
                        tr!("hide-layer")
                    } else {
                        tr!("show-layer")
                    };
                    let eye_response = sue_wm::ui::icon_button(
                        ("sue-paint-eye", info.id.to_u128()),
                        if visible { WmIcon::Eye } else { WmIcon::EyeOff },
                        &eye_name,
                    )
                    .quiet()
                    .show_at(ui, eye, theme);
                    if eye_response.clicked() && shown_cel.is_none() {
                        events.toggle = Some((info.id, !info.style.visible));
                    }
                    // The pen on the layer being edited, a box on the rest
                    // to pick several (a cel locked against keyframes shows
                    // a lock beside the pen).
                    let pick_cell = Rect::from_center_size(
                        pos2(lead.left() + cell * 1.5, mid),
                        egui::Vec2::splat(cell),
                    );
                    if chosen {
                        sue_wm::icons::paint(
                            &painter,
                            Rect::from_center_size(
                                pick_cell.center(),
                                egui::Vec2::splat(metrics.icon),
                            ),
                            WmIcon::ToolPen,
                            theme.text,
                        );
                    } else {
                        let pick_name = tr!("pick-this-layer", name = info.name.as_str());
                        let check =
                            sue_wm::ui::checkbox(("sue-paint-pick", info.id.to_u128()), "", also)
                                .bare()
                                .show_at(ui, pick_cell, theme);
                        check.widget_info(|| {
                            egui::WidgetInfo::selected(egui::Role::CheckBox, true, also, &pick_name)
                        });
                        if tooltip(
                            check,
                            &Tip::new(&pick_name).shortcut(&tr!("ctrl-click")),
                            theme,
                        )
                        .clicked()
                        {
                            events.also = Some(info.id);
                        }
                    }
                    let mut left = lead.right() + metrics.gap + depth as f32 * metrics.indent;
                    // A folder's open and close arrow and its picture, or
                    // the thumbnail.
                    let thumb =
                        Rect::from_min_size(pos2(left, full.top() + metrics.pad), metrics.thumb);
                    if is_folder {
                        let open = doc::is_open(arena, info.id);
                        let arrow = Rect::from_min_size(
                            pos2(left, mid - cell / 2.0),
                            egui::Vec2::splat(cell),
                        );
                        let arrow_name = if open {
                            tr!("close-the-folder")
                        } else {
                            tr!("open-the-folder")
                        };
                        if sue_wm::ui::icon_button(
                            ("sue-paint-fold", info.id.to_u128()),
                            if open {
                                WmIcon::ChevronDown
                            } else {
                                WmIcon::ChevronRight
                            },
                            &arrow_name,
                        )
                        .quiet()
                        .show_at(ui, arrow, theme)
                        .clicked()
                        {
                            events.fold = Some((info.id, !open));
                        }
                        let animation = crate::anim::is_animation_folder(arena, info.id);
                        let picture_rect = Rect::from_min_size(
                            pos2(arrow.right(), mid - metrics.folder / 2.0),
                            egui::Vec2::splat(metrics.folder),
                        );
                        sue_wm::icons::paint(
                            &painter,
                            picture_rect,
                            if animation {
                                WmIcon::AnimationFolder
                            } else {
                                WmIcon::Folder
                            },
                            theme.text,
                        );
                        left = picture_rect.right() + metrics.pad;
                    } else {
                        self.layer_thumbnail(ui, thumb, info.id, info.revision, cx, theme);
                        left = thumb.right() + metrics.pad;
                        // Ctrl+click on the thumbnail: Selection from Layer. It
                        // takes a click only with Ctrl held, so a plain click
                        // picks the row as anywhere else on it.
                        let command = ui.input(|input| input.modifiers.command);
                        let thumb_response = ui.interact(
                            thumb,
                            Id::unique(("sue-paint-thumb", info.id.to_u128())),
                            if command {
                                Sense::click()
                            } else {
                                Sense::hover()
                            },
                        );
                        let select_name = crate::commands::registry()
                            .label("select.from_layer")
                            .unwrap_or_default();
                        if tooltip(
                            thumb_response,
                            &Tip::new(&select_name).shortcut(&tr!("ctrl-click")),
                            theme,
                        )
                        .clicked()
                            && command
                        {
                            events.select_from = Some(info.id);
                        }
                    }
                    let arena = &cx.world.arena;
                    // The mask beside the layer and the link between them.
                    if let Some(mask) = doc::mask_of(arena, info.id) {
                        let linked = doc::mask_linked(arena, info.id);
                        let link = Rect::from_center_size(
                            pos2(left + metrics.icon / 2.0, mid),
                            egui::Vec2::new(metrics.icon, cell),
                        );
                        let link_name = if linked {
                            tr!("unlink-mask-from-layer")
                        } else {
                            tr!("link-mask-to-layer")
                        };
                        if sue_wm::ui::icon_button(
                            ("sue-paint-mask-link", info.id.to_u128()),
                            WmIcon::Link,
                            &link_name,
                        )
                        .selected(linked)
                        .quiet()
                        .show_at(ui, link, theme)
                        .clicked()
                        {
                            events.link = Some((info.id, !linked));
                        }
                        let mask_rect = Rect::from_min_size(
                            pos2(link.right() + metrics.gap, thumb.top()),
                            egui::Vec2::new(metrics.thumb.y * 0.75, metrics.thumb.y),
                        );
                        let revision = arena.get_by_id(mask).map_or(0, sue_core::Entity::revision);
                        self.layer_thumbnail(ui, mask_rect, mask, revision, cx, theme);
                        let on = doc::mask_on(&cx.world.arena, info.id);
                        if !on {
                            ui.painter()
                                .rect_filled(mask_rect, CornerRadius::ZERO, theme.scrim());
                        }
                        let editing = chosen && self.doc().is_some_and(|doc| doc.mask_edit);
                        if editing {
                            ui.painter().rect_stroke(
                                mask_rect,
                                theme.radius(Radius::Xs),
                                theme.selected_stroke(),
                                StrokeKind::Outside,
                            );
                        }
                        let mask_name = tr!("draw-mask-pen-shows-eraser-hides");
                        let mask_response = ui.interact(
                            mask_rect,
                            Id::unique(("sue-paint-mask", info.id.to_u128())),
                            Sense::click(),
                        );
                        if tooltip(mask_response, &Tip::new(&mask_name), theme).clicked() {
                            events.mask = Some(info.id);
                        }
                        left = mask_rect.right() + metrics.pad;
                    }
                    let arena = &cx.world.arena;
                    // The marks at the right end: the lock, the badges of
                    // effects that are on, keyframes. Each tells its name
                    // when the pointer rests on it.
                    let mut right = full.right() - metrics.pad;
                    let mut marks: Vec<(Rect, String)> = Vec::new();
                    let mut mark = |right: &mut f32, icon: WmIcon, color: Color32, name: String| {
                        let at = Rect::from_min_size(
                            pos2(*right - metrics.icon, mid - metrics.icon / 2.0),
                            egui::Vec2::splat(metrics.icon),
                        );
                        sue_wm::icons::paint(&painter, at, icon, color);
                        *right = at.left() - metrics.gap;
                        marks.push((at, name));
                        at
                    };
                    if info.locked {
                        mark(
                            &mut right,
                            WmIcon::Lock,
                            theme.text_muted(),
                            tr!("layer-mark-locked"),
                        );
                    }
                    if let Ok(entity) = arena.get_by_id(info.id) {
                        let effects = crate::effects::Effects::of(entity, 72.0);
                        if crate::anim::motion::keyframes_on(entity) {
                            let pen = mark(
                                &mut right,
                                WmIcon::ToolPen,
                                theme.text_muted(),
                                tr!("layer-mark-keyframes"),
                            );
                            let small = Rect::from_min_size(pen.center(), pen.size() / 2.0);
                            sue_wm::icons::paint(&painter, small, WmIcon::Lock, theme.text);
                        }
                        if effects.tone.is_some() {
                            mark(
                                &mut right,
                                WmIcon::Tone,
                                theme.text_muted(),
                                tr!("layer-mark-tone"),
                            );
                        }
                        if effects.border.is_some() {
                            mark(
                                &mut right,
                                WmIcon::BorderEffect,
                                theme.text_muted(),
                                tr!("layer-mark-border"),
                            );
                        }
                        if let Some(color) = effects.color {
                            // The layer color, and its sub color beside it.
                            let chips = [
                                (Some(color), tr!("layer-mark-color")),
                                (effects.sub_color, tr!("layer-mark-sub-color")),
                            ];
                            for (rgb, name) in chips.into_iter().rev() {
                                let Some(rgb) = rgb else {
                                    continue;
                                };
                                let [r, g, b] = crate::color::bytes(rgb);
                                let chip = Rect::from_min_size(
                                    pos2(right - metrics.icon / 2.0, mid - metrics.icon / 2.0),
                                    egui::Vec2::new(metrics.icon / 2.0, metrics.icon),
                                );
                                painter.rect_filled(
                                    chip,
                                    theme.radius(Radius::Xs),
                                    Color32::from_rgb(r, g, b),
                                );
                                painter.rect_stroke(
                                    chip,
                                    theme.radius(Radius::Xs),
                                    theme.hairline(),
                                    StrokeKind::Inside,
                                );
                                right = chip.left() - metrics.gap;
                                marks.push((chip, name));
                            }
                        }
                    }
                    if info.style.draft {
                        let edge = full.left() + metrics.gap;
                        marks.push((
                            Rect::from_min_max(
                                pos2(full.left(), full.top()),
                                pos2(edge + metrics.gap, full.bottom()),
                            ),
                            tr!("layer-mark-draft"),
                        ));
                    }
                    for (at, name) in marks {
                        let told = ui.interact(
                            at,
                            Id::unique(("sue-paint-layer-mark", info.id.to_u128(), name.as_str())),
                            Sense::hover(),
                        );
                        told.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Image, true, &name)
                        });
                        tooltip(told, &Tip::new(&name), theme);
                    }
                    // Text: the mode and opacity over the name.
                    let top_line = match info.kind {
                        LayerKind::Paper => tr!("paper-2"),
                        _ => format!(
                            "{} % {}",
                            sue_lang::number(f64::from(info.style.opacity) * 100.0, 0),
                            sue_lang::translate("model", info.style.blend.label())
                        ),
                    };
                    let width = (right - left).max(0.0);
                    let caption = theme.text_font(TextRole::Caption);
                    painter
                        .with_clip_rect(Rect::from_min_max(
                            pos2(left, full.top()),
                            pos2(right, full.bottom()),
                        ))
                        .text(
                            pos2(left, mid - metrics.gap),
                            Align2::LEFT_BOTTOM,
                            top_line,
                            caption,
                            theme.text_muted(),
                        );
                    let renaming = self
                        .renaming
                        .as_ref()
                        .filter(|(id, _)| *id == info.id)
                        .map(|(_, name)| name.clone());
                    if let Some(mut name) = renaming {
                        let field = Rect::from_min_size(
                            pos2(left, mid),
                            egui::Vec2::new(width, Density::Compact.control_height()),
                        );
                        let edit = sue_wm::ui::text_field(
                            ("sue-paint-rename", info.id.to_u128()),
                            &mut name,
                        )
                        .label(&tr!("change-layer-name-ellipsis"))
                        .show_at(ui, field, theme);
                        edit.request_focus();
                        if edit.lost_focus() {
                            events.rename = Some((
                                info.id,
                                name.clone(),
                                ui.input(|input| input.key_pressed(egui::Key::Escape)),
                            ));
                        }
                        self.renaming = Some((info.id, name));
                    } else {
                        let arena = &cx.world.arena;
                        // An animation folder names the cel it shows now.
                        let shows = (is_folder && crate::anim::is_animation_folder(arena, info.id))
                            .then_some(sequence.as_ref())
                            .flatten()
                            .and_then(|sequence| {
                                let track = crate::anim::track_of(sequence, info.id)?;
                                crate::anim::cel_at(&sequence.tracks[track], sequence.rate, frame)
                            })
                            .and_then(|cel| arena.get_by_id(cel).ok())
                            .map(|cel| cel.name().to_owned());
                        let name = match (shows, doc::is_reference(arena, info.id)) {
                            (Some(cel), _) => {
                                tr!("folder-shows-cel", name = info.name.as_str(), cel = cel)
                            }
                            (None, true) => tr!("reference", name = info.name.as_str()),
                            _ if crate::three::is_3d(arena, info.id) => {
                                tr!("layer-3d-name", name = info.name.as_str())
                            }
                            _ => info.name.clone(),
                        };
                        painter
                            .with_clip_rect(Rect::from_min_max(
                                pos2(left, full.top()),
                                pos2(right, full.bottom()),
                            ))
                            .text(
                                pos2(left, mid + metrics.gap),
                                Align2::LEFT_TOP,
                                name,
                                theme.text_font(TextRole::Body),
                                if visible {
                                    theme.text
                                } else {
                                    theme.text_muted()
                                },
                            );
                    }
                    if info.style.clip {
                        // Clipped to the layer below: a line down the left
                        // of its thumbnail.
                        let x = thumb.left() - metrics.gap;
                        painter.vline(
                            x,
                            full.shrink(metrics.pad).y_range(),
                            theme.selected_stroke(),
                        );
                    }
                    let arena = &cx.world.arena;
                    let correction = arena
                        .get_by_id(info.id)
                        .ok()
                        .filter(|entity| entity.class() == doc::CORRECTION)
                        .and_then(|entity| entity.property(doc::CORRECTION_FILTER))
                        .and_then(Value::as_str)
                        .and_then(|text| {
                            crate::filters::Filter::from_text(text)
                                .map(|filter| (filter, text.to_owned()))
                        });
                    let modifiers = ui.input(|input| input.modifiers);
                    if response.double_clicked()
                        && let Some((filter, text)) = correction
                    {
                        // A correction layer's settings, as Clip Studio Paint
                        // opens them on a double-click.
                        events.reopen = Some((info.id, filter, text));
                    } else if response.double_clicked() {
                        self.renaming = Some((info.id, info.name.clone()));
                    } else if response.clicked() && modifiers.shift {
                        events.range = Some(info.id);
                    } else if response.clicked() && modifiers.command {
                        // Ctrl+click picks more layers.
                        events.also = Some(info.id);
                    } else if response.clicked() || response.secondary_clicked() {
                        events.pick = Some(info.id);
                        if let (Some(folder), Some(sequence)) = (folder, sequence.as_ref())
                            && let Some(track) = crate::anim::track_of(sequence, folder)
                            && let Some(start) =
                                crate::anim::cels::first_frame_of(sequence, track, info.id)
                        {
                            events.cel = Some((track, start));
                        }
                    }
                    let state = |command: &str| self.state(command, &Facts::of(cx));
                    response.context_menu(|ui| {
                        if let Some(command) = sue_wm::context_items(ui, &menu, &state) {
                            events.menu = Some(command);
                        }
                    });
                }
            },
        );
    }

    /// The short list a right click on a layer opens, in Clip Studio
    /// Paint's order: making layers, then masks, rulers, settings and
    /// selections, then turning and merging.
    fn layer_row_menu(&self) -> Vec<sue_wm::MenuItem> {
        use sue_wm::MenuItem;
        let menus = crate::menus::menus();
        let all: Vec<&MenuItem> = menus.iter().flat_map(|menu| menu.items.iter()).collect();
        // The submenu of the menus that holds `command`.
        fn holding<'m>(items: &[&'m MenuItem], command: &str) -> Option<&'m MenuItem> {
            fn has(items: &[MenuItem], command: &str) -> bool {
                items.iter().any(|item| match item {
                    MenuItem::Action { command: found, .. } => found == command,
                    MenuItem::Sub { items, .. } => has(items, command),
                    MenuItem::Separator => false,
                })
            }
            items.iter().copied().find_map(|item| match item {
                MenuItem::Sub { items: inner, .. } if has(inner, command) => {
                    let children: Vec<&MenuItem> = inner.iter().collect();
                    holding(&children, command).or(Some(item))
                }
                _ => None,
            })
        }
        let action = |command: &str| {
            let label = crate::commands::registry()
                .label(command)
                .unwrap_or_else(|| command.to_owned());
            MenuItem::action(label, command)
        };
        let sub_of = |command: &str| holding(&all, command).cloned();
        let palette_colors = MenuItem::sub(
            tr!("change-palette-color"),
            std::iter::once(MenuItem::action(
                tr!("no-palette-color"),
                "layer.palette_color.none",
            ))
            .chain(doc::PALETTE_COLORS.iter().map(|(name, _)| {
                MenuItem::action(
                    sue_lang::text("paint", &format!("palette-color-{name}")),
                    format!("layer.palette_color.{name}"),
                )
            }))
            .collect(),
        );
        let mut items = vec![action("layer.new.raster_top")];
        items.extend(sub_of("layer.new.vector"));
        items.extend(sub_of("layer.correction.levels"));
        items.extend(
            [
                "layer.new.folder",
                "layer.folder_insert",
                "layer.ungroup",
                "layer.duplicate",
                "layer.delete",
            ]
            .map(action),
        );
        items.push(MenuItem::Separator);
        items.extend(sub_of("layer.mask.apply"));
        items.extend(sub_of("layer.ruler.show"));
        items.extend(sub_of("layer.toggle.draft"));
        items.push(palette_colors);
        items.extend(sub_of("select.add_layer"));
        items.extend(["layer.rasterize", "layer.convert"].map(action));
        items.push(MenuItem::Separator);
        items.extend(
            [
                "layer.transfer_down",
                "layer.merge_down",
                "layer.merge_selected",
                "layer.merge_visible_here",
                "layer.merge_visible",
                "layer.flatten",
            ]
            .map(action),
        );
        items
    }

    /// While a layer is dragged: a see-through copy of its name follows
    /// the pointer, and over a button that takes no drop a no-entry sign
    /// shows (both in Sue's own look).
    fn drag_feedback(&self, ui: &Ui, stack: &[doc::LayerInfo], theme: &Theme) {
        let (Some(dragged), Some(at)) = (self.layer_drag, ui.ctx().pointer_hover_pos()) else {
            return;
        };
        let Some(info) = stack.iter().find(|info| info.id == dragged) else {
            return;
        };
        let metrics = Metrics::of(theme);
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            Id::unique("sue-paint-layer-ghost"),
        ));
        let takes = self
            .layer_palette
            .drop_targets
            .iter()
            .any(|(rect, _)| rect.contains(at));
        let over_button = self
            .layer_palette
            .buttons
            .iter()
            .any(|(rect, _)| rect.contains(at));
        let font = theme.text_font(TextRole::Body);
        let galley = painter.layout_no_wrap(info.name.clone(), font, theme.text);
        let ghost = Rect::from_min_size(
            at + egui::Vec2::splat(metrics.wide),
            galley.size() + egui::Vec2::splat(metrics.wide),
        );
        painter.rect_filled(
            ghost,
            theme.radius(Radius::Sm),
            theme.surface(3).linear_multiply(GHOST),
        );
        painter.galley(
            ghost.min + egui::Vec2::splat(metrics.pad),
            galley,
            theme.text,
        );
        if over_button && !takes {
            let sign = Rect::from_center_size(at, egui::Vec2::splat(metrics.icon));
            sue_wm::icons::paint(&painter, sign, WmIcon::Cancel, theme.danger());
        }
    }

    /// Does what the rows asked for this frame.
    fn row_events(
        &mut self,
        events: RowEvents,
        stack: &[doc::LayerInfo],
        cx: &mut AppletContext<'_>,
    ) {
        if let Some(id) = events.pick
            && let Some(doc) = self.doc_mut()
        {
            if doc.layer != Some(id) {
                doc.mask_edit = false;
                doc.picked.clear();
            }
            doc.layer = Some(id);
        }
        if let Some((track, frame)) = events.cel {
            if let Some(doc) = self.doc_mut() {
                doc.track = track;
            }
            self.go_to_frame(&cx.world.arena, i64::from(frame));
        }
        if let Some(id) = events.also
            && let Some(doc) = self.doc_mut()
            && doc.layer != Some(id)
        {
            if let Some(at) = doc.picked.iter().position(|known| *known == id) {
                doc.picked.remove(at);
            } else {
                doc.picked.push(id);
            }
        }
        if let Some(id) = events.range {
            self.pick_range(id, stack, &cx.world.arena);
        }
        if let Some(id) = events.select_from {
            if let Some(doc) = self.doc_mut() {
                doc.layer = Some(id);
                doc.picked.clear();
            }
            self.run("select.from_layer", cx);
        }
        if let Some((id, visible)) = events.toggle {
            // The eye of a picked layer shows or hides every picked one.
            let picked = self.picked_layers(&cx.world.arena);
            let ids = if picked.contains(&id) {
                picked
            } else {
                vec![id]
            };
            if let Some(edit) = doc::set_layers(&cx.world.arena, &ids, picture::VISIBLE, visible) {
                Self::apply(cx, edit);
            }
        }
        if let Some(command) = events.menu {
            self.run(&command, cx);
        }
        if let Some((id, open)) = events.fold {
            Self::apply(cx, doc::set(id, doc::OPEN, open));
        }
        if let Some((id, linked)) = events.link
            && let Some(edit) = doc::set_mask_linked(&cx.world.arena, id, linked)
        {
            Self::apply(cx, edit);
        }
        if let Some(id) = events.dragging {
            self.layer_drag = Some(id);
        }
        if let Some(dragged) = events.dropped {
            self.layer_drag = None;
            self.drop_layer(dragged, events.onto, events.pointer, cx);
        }
        if let Some((id, filter, text)) = events.reopen {
            self.dialog = Some(crate::dialogs::Dialog::Correction(id, filter, text, false));
        }
        if let Some(id) = events.mask
            && let Some(doc) = self.doc_mut()
        {
            // The mask of the current layer toggles; another layer's mask
            // picks that layer with its mask.
            doc.mask_edit = doc.layer != Some(id) || !doc.mask_edit;
            doc.layer = Some(id);
        }
        if let Some((id, name, cancelled)) = events.rename {
            self.renaming = None;
            let name = name.trim().to_owned();
            let old = cx
                .world
                .arena
                .get_by_id(id)
                .map(|entity| entity.name().to_owned())
                .unwrap_or_default();
            if !cancelled && !name.is_empty() && name != old {
                Self::apply(cx, sue_core::txn::Rename::new(id, name));
            }
        }
    }

    /// The body of the question asked before picked folders are deleted.
    pub(crate) fn delete_folders_ui(
        ui: &mut Ui,
        quiet: &mut bool,
        theme: &Theme,
    ) -> crate::dialogs::Outcome {
        ui.label(
            egui::RichText::new(tr!("delete-layer-folders-question"))
                .font(theme.text_font(TextRole::Body))
                .color(theme.text),
        );
        ui.add_space(theme.space(Space::S3));
        if sue_wm::ui::checkbox("sue-paint-delete-quiet", &tr!("do-not-show-again"), *quiet)
            .show(ui, theme)
            .clicked()
        {
            *quiet = !*quiet;
        }
        ui.add_space(theme.space(Space::S4));
        let mut outcome = crate::dialogs::Outcome::Open;
        ui.horizontal(|ui| {
            if sue_wm::ui::button("sue-paint-delete-folders", &tr!("delete"))
                .danger()
                .show(ui, theme)
                .clicked()
            {
                outcome = crate::dialogs::Outcome::Ok;
            }
            if sue_wm::ui::button("sue-paint-delete-keep", &tr!("cancel"))
                .show(ui, theme)
                .clicked()
            {
                outcome = crate::dialogs::Outcome::Cancel;
            }
        });
        outcome
    }

    /// Delete in that question: the folders go, and with "Do not show
    /// again" ticked Sue stops asking.
    pub(crate) fn delete_folders_ok(&mut self, cx: &mut AppletContext<'_>, quiet: bool) {
        if quiet
            && let Err(error) = cx.edit_rice(sue_rice::SetRiceValue::new(
                crate::studio::ASK_DELETE_FOLDERS,
                sue_rice::RiceValue::Bool(false),
            ))
        {
            cx.fail(error.to_string());
        }
        self.delete_layers(cx, true);
    }

    /// Shift+click: every shown layer from the current one to `to` is
    /// picked.
    fn pick_range(&mut self, to: EntityId, stack: &[doc::LayerInfo], arena: &sue_core::Arena) {
        let search = self
            .layer_palette
            .search
            .clone()
            .filter(SearchLayer::is_active);
        let shown: Vec<EntityId> = stack
            .iter()
            .filter(|info| match &search {
                Some(search) => search.matches(arena, info.id),
                None => doc::folders_around(arena, info.id)
                    .iter()
                    .all(|folder| doc::is_open(arena, *folder)),
            })
            .map(|info| info.id)
            .collect();
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let from = doc
            .layer
            .and_then(|layer| shown.iter().position(|id| *id == layer));
        let Some(end) = shown.iter().position(|id| *id == to) else {
            return;
        };
        let Some(start) = from else {
            doc.layer = Some(to);
            return;
        };
        let (low, high) = (start.min(end), start.max(end));
        doc.picked = shown[low..=high]
            .iter()
            .copied()
            .filter(|id| doc.layer != Some(*id))
            .collect();
    }

    /// A dragged layer let go: onto New Raster Layer it is copied, onto
    /// New Layer Folder it goes into a new folder, onto a folder in it,
    /// onto a layer right above it.
    fn drop_layer(
        &mut self,
        dragged: EntityId,
        onto: Option<EntityId>,
        at: Option<Pos2>,
        cx: &mut AppletContext<'_>,
    ) {
        let target = at.and_then(|at| {
            self.layer_palette
                .drop_targets
                .iter()
                .find(|(rect, _)| rect.contains(at))
                .map(|(_, command)| *command)
        });
        // A picked layer brings every picked layer with it.
        let picked = self.picked_layers(&cx.world.arena);
        let several = picked.len() > 1 && picked.contains(&dragged);
        match target {
            Some(command) => {
                if !several && let Some(doc) = self.doc_mut() {
                    doc.layer = Some(dragged);
                    doc.picked.clear();
                }
                self.run(
                    if command == "layer.new" {
                        "layer.duplicate"
                    } else {
                        "layer.folder_insert"
                    },
                    cx,
                );
            }
            None => {
                let (Some(picture), Some(onto)) = (self.picture(), onto) else {
                    return;
                };
                let moving = if several {
                    Self::outermost(&cx.world.arena, &picked)
                } else {
                    vec![dragged]
                };
                if let Some(edit) = Self::drop_several(&cx.world.arena, picture, &moving, onto) {
                    Self::apply(cx, edit);
                }
            }
        }
    }

    /// The layers of `ids` (bottom first) that are not inside another of
    /// them: a picked folder goes with what it holds.
    fn outermost(arena: &sue_core::Arena, ids: &[EntityId]) -> Vec<EntityId> {
        ids.iter()
            .copied()
            .filter(|id| {
                !doc::folders_around(arena, *id)
                    .iter()
                    .any(|folder| ids.contains(folder))
            })
            .collect()
    }

    /// The one step that drops `moving` (bottom first) onto `onto`: right
    /// above a layer, or at the top inside a folder, in their order. Each
    /// layer's move is worked out on a copy of the arena (its pixels are
    /// shared, not copied) after the moves before it.
    fn drop_several(
        arena: &sue_core::Arena,
        picture: EntityId,
        moving: &[EntityId],
        onto: EntityId,
    ) -> Option<doc::Edit> {
        use sue_core::ReversibleTransaction;
        if let [only] = moving {
            return crate::anim::drop_layer(arena, picture, *only, onto);
        }
        // Above a layer the top one goes first, so each next one lands
        // under it; into a folder the bottom one goes first.
        let mut order: Vec<EntityId> = moving.iter().copied().filter(|id| *id != onto).collect();
        if !doc::is_folder(arena, onto) {
            order.reverse();
        }
        let mut scratch = arena.clone();
        let mut edit = doc::Edit::new(tr!("move-layer"));
        let mut any = false;
        for layer in order {
            let (Some(mut tried), Some(step)) = (
                crate::anim::drop_layer(&scratch, picture, layer, onto),
                crate::anim::drop_layer(&scratch, picture, layer, onto),
            ) else {
                continue;
            };
            tried.apply(&mut scratch).ok()?;
            edit = edit.then(step);
            any = true;
        }
        any.then_some(edit)
    }

    /// The thumbnail of the layer or mask `id` at `revision`, fitted into
    /// `rect` over the checkerboard of see-through pixels.
    fn layer_thumbnail(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        id: EntityId,
        revision: u64,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let key = texture_key(id, 0x7448_554d_4253);
        if self
            .thumbs
            .get(&id)
            .is_none_or(|(known, _, _)| *known != revision)
            && let Some((pixels, size)) = crate::composite::thumbnail(&cx.world.arena, id, [96, 72])
        {
            self.thumbs.insert(id, (revision, pixels, size));
        }
        let texture = self.thumbs.get(&id).and_then(|(revision, pixels, size)| {
            cx.textures.texture(key, *revision, *size, 4, pixels, false)
        });
        ui.painter().rect_filled(rect, 0.0, theme.void);
        if let Some(texture) = texture {
            let aspect = self
                .thumbs
                .get(&id)
                .map_or(4.0 / 3.0, |(_, _, [w, h])| *w as f32 / (*h).max(1) as f32);
            let size = if aspect > rect.width() / rect.height() {
                egui::vec2(rect.width(), rect.width() / aspect)
            } else {
                egui::vec2(rect.height() * aspect, rect.height())
            };
            let image = Rect::from_center_size(rect.center(), size);
            ui.painter().image(
                texture,
                image,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            ui.painter()
                .rect_stroke(image, 0.0, theme.hairline(), StrokeKind::Outside);
        }
    }
}
