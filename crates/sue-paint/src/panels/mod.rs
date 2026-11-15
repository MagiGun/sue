//! The palettes around the canvas: the command bar, the tool strip, Sub
//! Tool and Tool Property, the brush size sliders, the color wheel and
//! sliders, the color set and history, the navigator, the layers and the
//! timeline.
//!
//! This file holds what every palette shares: the child `Ui`, the icon
//! button, the tab header, the flow layout, the wedge and tall sliders,
//! and a single palette with its torn-off header.

mod brush_size;
mod cels;
mod color;
mod color_set;
mod command_bar;
mod history;
mod layer;
mod layer_property;
mod material;
mod navigator;
mod quick_access;
mod sub_tool;
mod timeline;
mod tool;
mod tool_property;

use egui::{
    Color32, Id, Mesh, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, UiBuilder, pos2, vec2,
};
use sue_wm::{AppletContext, Theme};

use crate::icons::{self, Icon};
use crate::studio::SuePaint;

pub(crate) use command_bar::icon_picker;
pub(crate) use material::material_thumb;
pub(crate) use quick_access::MENU_COMMANDS as QUICK_ACCESS_COMMANDS;
pub(crate) use tool::{slot_icon, tool_icon};

/// A child `Ui` confined to `rect`.
pub(crate) fn child(ui: &mut Ui, rect: Rect, salt: &str) -> Ui {
    let mut inner = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .id_salt(("sue-paint", salt))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    inner.set_clip_rect(rect.intersect(ui.clip_rect()));
    inner
}

/// A square icon button. Returns `true` when clicked while enabled.
pub(crate) fn icon_button(
    ui: &mut Ui,
    rect: Rect,
    icon: Icon,
    enabled: bool,
    active: bool,
    tip: &str,
    theme: &Theme,
) -> bool {
    let response = ui.interact(
        rect,
        Id::unique(("sue-paint-icon", tip, rect.min.x as i32, rect.min.y as i32)),
        Sense::click(),
    );
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, theme.control_radius, theme.active);
    } else if enabled && response.hovered() {
        painter.rect_filled(rect, theme.control_radius, theme.hover);
    }
    let color = if !enabled {
        theme.muted.gamma_multiply(0.45)
    } else if active {
        theme.text
    } else {
        theme.text.gamma_multiply(0.85)
    };
    icons::paint(painter, rect.shrink(rect.width() * 0.13), icon, color);
    let response = if enabled {
        response.on_hover_text(tip)
    } else {
        response.on_hover_text(tr!("not-available-now", tip = tip))
    };
    enabled && response.clicked()
}

/// A palette's tab strip, as Clip Studio Paint draws it: each tab an icon
/// and its name, the chosen one lit. When the names do not all fit, only
/// the chosen tab keeps its name and the others show their icon (with the
/// name on hover). Returns the body below it and a clicked tab.
fn header(
    ui: &mut Ui,
    rect: Rect,
    tabs: &[(String, Icon, bool)],
    selected: u8,
    theme: &Theme,
) -> (Rect, Option<u8>) {
    let (body, clicked, _) = header_tearing(ui, rect, tabs, &[], selected, theme);
    (body, clicked)
}

/// Where the dock keeps how much room a palette's header leaves at its
/// left for the palette's menu button.
pub(crate) const MENU_INSET: &str = "sue-paint-menu-inset";

/// The room a header leaves at its left for the palette's menu button:
/// none outside the dock (a phone's sheet).
fn menu_inset(ui: &Ui) -> f32 {
    ui.ctx()
        .data(|data| data.get_temp::<f32>(Id::unique(MENU_INSET)))
        .unwrap_or(0.0)
}

/// [`header`], whose tabs flagged in `tearing` also drag: the tab dragged
/// and where the drag began, to tear it off into a palette of its own.
fn header_tearing(
    ui: &mut Ui,
    rect: Rect,
    tabs: &[(String, Icon, bool)],
    tearing: &[bool],
    selected: u8,
    theme: &Theme,
) -> (Rect, Option<u8>, Option<(u8, Pos2)>) {
    let strip = Rect::from_min_size(rect.min, vec2(rect.width(), 28.0));
    let painter = ui.painter().clone();
    painter.rect_filled(strip, 0.0, theme.pill);
    painter.hline(strip.x_range(), strip.bottom(), theme.hairline());
    let icon_side = 18.0;
    let lay_out = |size: f32| -> Vec<_> {
        tabs.iter()
            .map(|(title, _, _)| {
                painter.layout_no_wrap(title.clone(), theme.font(size), theme.text)
            })
            .collect()
    };
    let mut galleys = lay_out(theme.small_size + 0.5);
    // A lone tab in a narrow column (Tool Sliders) keeps its whole name in
    // a smaller font, clear of the palette's menu button.
    if let [galley] = galleys.as_slice()
        && icon_side + 6.0 + galley.size().x + 14.0 > strip.width() - 24.0
    {
        galleys = lay_out(theme.small_size - 1.5);
    }
    let labelled = |galley: &std::sync::Arc<egui::Galley>| icon_side + 6.0 + galley.size().x + 14.0;
    let bare = icon_side + 14.0;
    let all: f32 = galleys.iter().map(labelled).sum::<f32>() + 2.0 * tabs.len() as f32;
    // Room at the left for the palette's menu button, in the dock.
    let inset = menu_inset(ui);
    let names = all <= strip.width() - 8.0 - inset;
    let mut x = strip.left() + 4.0 + inset;
    let mut clicked = None;
    let mut dragged = None;
    let painter = painter.with_clip_rect(strip);
    for ((index, (title, icon, enabled)), galley) in tabs.iter().enumerate().zip(galleys) {
        let chosen = index as u8 == selected;
        let named = names || chosen;
        let width = if named { labelled(&galley) } else { bare };
        let tab = Rect::from_min_size(pos2(x, strip.top() + 2.0), vec2(width, 26.0));
        x = tab.right() + 2.0;
        let tears = tearing.get(index).copied().unwrap_or(false);
        let response = ui.interact(
            tab,
            Id::unique(("sue-paint-tab", title, rect.min.x as i32, rect.min.y as i32)),
            if tears {
                Sense::click_and_drag()
            } else {
                Sense::click()
            },
        );
        if tears
            && response.drag_started()
            && let Some(from) = ui.input(|input| input.pointer.press_origin())
        {
            dragged = Some((index as u8, from));
        }
        if chosen {
            painter.rect_filled(tab, theme.control_radius, theme.panel);
            painter.hline(
                tab.shrink2(vec2(6.0, 0.0)).x_range(),
                tab.bottom() - 1.0,
                Stroke::new(2.0, theme.border_active),
            );
        } else if *enabled && response.hovered() {
            painter.rect_filled(tab, theme.control_radius, theme.hover);
        }
        let color = if !enabled {
            theme.muted.gamma_multiply(0.5)
        } else if chosen {
            theme.text
        } else {
            theme.muted
        };
        let icon_rect = Rect::from_center_size(
            pos2(tab.left() + 7.0 + icon_side / 2.0, tab.center().y),
            vec2(icon_side, icon_side),
        );
        icons::paint(&painter, icon_rect, *icon, color);
        if named {
            painter.galley(
                pos2(
                    icon_rect.right() + 6.0,
                    tab.center().y - galley.size().y / 2.0,
                ),
                galley,
                color,
            );
        }
        let response = if named {
            response
        } else {
            response.on_hover_text(title)
        };
        if *enabled && response.clicked() {
            clicked = Some(index as u8);
        }
    }
    (
        Rect::from_min_max(pos2(rect.left(), strip.bottom()), rect.max),
        clicked,
        dragged,
    )
}

/// Buttons laid out left to right in `area`, wrapping to the next line
/// when one would pass the right edge and stopping when no line is left:
/// how the command bar and the tool palette fill whatever shape people
/// give them.
pub(crate) struct Flow {
    area: Rect,
    left: f32,
    right: f32,
    line: f32,
    x: f32,
    pub y: f32,
}

impl Flow {
    /// Lines `line` tall in `area`, starting `inset` in from its left and
    /// keeping `reserve` clear at its right (a menu button).
    pub(crate) fn new(area: Rect, inset: f32, reserve: f32, line: f32) -> Self {
        let left = area.left() + inset;
        Self {
            area,
            left,
            right: (area.right() - reserve).max(left + line),
            line,
            x: left,
            y: area.top(),
        }
    }

    /// The rect of the next button of `size`, `advance` wide in the line,
    /// or `None` when it has no room.
    pub(crate) fn next(&mut self, size: egui::Vec2, advance: f32) -> Option<Rect> {
        if self.x + size.x > self.right && self.x > self.left {
            self.x = self.left;
            self.y += self.line;
        }
        if self.y + size.y > self.area.bottom() + 0.5 {
            return None;
        }
        let rect = Rect::from_min_size(pos2(self.x, self.y), size);
        self.x += advance;
        Some(rect)
    }

    /// A gap of `width` between groups, unless a line just began; where
    /// it starts.
    pub(crate) fn gap(&mut self, width: f32) -> Option<f32> {
        if self.x <= self.left || self.x + width > self.right {
            return None;
        }
        let at = self.x;
        self.x += width;
        Some(at)
    }
}

/// Clip Studio Paint's wedge slider: a dark triangle that grows to the
/// right, lit up to the value, with a thin bar at it. A widget, so its
/// response says when it changed and when a drag ended.
pub(crate) struct Wedge<'a> {
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    logarithmic: bool,
    width: f32,
    theme: &'a Theme,
}

impl<'a> Wedge<'a> {
    pub(crate) fn new(
        value: &'a mut f32,
        range: std::ops::RangeInclusive<f32>,
        width: f32,
        theme: &'a Theme,
    ) -> Self {
        Self {
            value,
            range,
            logarithmic: false,
            width,
            theme,
        }
    }

    pub(crate) fn logarithmic(mut self) -> Self {
        self.logarithmic = true;
        self
    }

    fn to_unit(&self, value: f32) -> f32 {
        let (low, high) = (*self.range.start(), *self.range.end());
        let t = if self.logarithmic {
            (value.max(low).ln() - low.max(1e-3).ln()) / (high.ln() - low.max(1e-3).ln())
        } else {
            (value - low) / (high - low)
        };
        t.clamp(0.0, 1.0)
    }

    fn value_at(&self, t: f32) -> f32 {
        let (low, high) = (*self.range.start(), *self.range.end());
        if self.logarithmic {
            (low.max(1e-3).ln() + t * (high.ln() - low.max(1e-3).ln())).exp()
        } else {
            low + t * (high - low)
        }
    }
}

impl egui::Widget for Wedge<'_> {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(self.width.max(24.0), 18.0), Sense::click_and_drag());
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked())
        {
            let t = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let value = self.value_at(t);
            if (value - *self.value).abs() > f32::EPSILON {
                *self.value = value;
                response.mark_changed();
            }
        }
        let theme = self.theme;
        let wedge = rect.shrink2(vec2(1.0, 2.0));
        let t = self.to_unit(*self.value);
        let x = wedge.left() + t * wedge.width();
        // The wedge's top edge at `x`: a point at the left, full height at
        // the right.
        let top_at = |x: f32| {
            let u = (x - wedge.left()) / wedge.width();
            wedge.bottom() - wedge.height() * (0.08 + 0.92 * u)
        };
        let painter = ui.painter();
        let lit = if response.hovered() || response.dragged() {
            theme.text
        } else {
            theme.text.gamma_multiply(0.78)
        };
        let dark = theme.void;
        let polygon = |from: f32, to: f32, color: Color32| {
            if to - from < 0.5 {
                return;
            }
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(from, wedge.bottom()),
                    pos2(from, top_at(from)),
                    pos2(to, top_at(to)),
                    pos2(to, wedge.bottom()),
                ],
                color,
                Stroke::NONE,
            ));
        };
        polygon(wedge.left(), x, lit);
        polygon(x, wedge.right(), dark);
        painter.line_segment(
            [pos2(x, wedge.top()), pos2(x, wedge.bottom())],
            Stroke::new(1.5, theme.text),
        );
        response
    }
}

/// A tall slider down `lane` below its `label` space, as Clip Studio
/// Paint's Tool Sliders draw them, the top being 1. The size slider is a
/// thin dark line, lit from the bottom up to its knob; `fading` (opacity)
/// is a wider line of solid black at the top fading to clear at the bottom,
/// over a checkerboard. The new place while it is dragged.
fn tall_slider(
    ui: &mut Ui,
    lane: Rect,
    label: f32,
    at: f32,
    fading: bool,
    salt: &str,
    theme: &Theme,
) -> Option<f32> {
    // On whole screen pixels, so the checkerboard stays crisp.
    let pixel = 1.0 / ui.ctx().pixels_per_point();
    let snap = |value: f32| (value / pixel).round() * pixel;
    let cell = snap(3.0).max(pixel);
    let width = if fading { cell * 2.0 } else { snap(4.0) };
    let height = snap((lane.height() - label - 18.0).max(20.0));
    let track = Rect::from_min_size(
        pos2(
            snap(lane.center().x - width / 2.0),
            snap(lane.top() + label + 8.0),
        ),
        vec2(width, height),
    );
    let response = ui.interact(
        track.expand2(vec2(10.0, 8.0)),
        Id::unique(salt),
        Sense::click_and_drag(),
    );
    let painter = ui.painter();
    let y = track.bottom() - at.clamp(0.0, 1.0) * track.height();
    // Clip Studio's own grays, the same in light and dark looks.
    let lit = Color32::from_gray(176);
    if fading {
        // Clear shows as Sue's checkerboard.
        sue_wm::checker_fill(painter, track, 0.0, cell);
        // Solid black at the top, clear at the bottom, faded as Clip
        // Studio fades it: evenly to the eye, so it stays dark to about
        // halfway. The screen blends in linear light, so each step's
        // alpha is the one that darkens as much as the opacity does on
        // screen.
        let mut mesh = Mesh::default();
        let steps = 48;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let opacity = 1.0 - t;
            let alpha = 1.0
                - egui::ecolor::linear_f32_from_gamma_u8(((1.0 - opacity) * 255.0).round() as u8);
            let color = Color32::from_black_alpha((alpha * 255.0).round() as u8);
            let y = track.top() + t * track.height();
            mesh.colored_vertex(pos2(track.left(), y), color);
            mesh.colored_vertex(pos2(track.right(), y), color);
            if step > 0 {
                let at = step * 2;
                mesh.add_triangle(at - 2, at - 1, at + 1);
                mesh.add_triangle(at - 2, at + 1, at);
            }
        }
        painter.add(Shape::mesh(mesh));
    } else {
        // Dark above the knob, lit from the bottom up to it.
        painter.rect_filled(track, 2.0, theme.void);
        painter.rect_filled(
            Rect::from_min_max(pos2(track.left(), y), track.right_bottom()),
            2.0,
            lit,
        );
    }
    let knob = pos2(track.center().x, y);
    painter.circle_filled(knob, 7.5, lit);
    let pointer = response
        .interact_pointer_pos()
        .filter(|_| response.dragged() || response.clicked())?;
    Some(((track.bottom() - pointer.y) / track.height()).clamp(0.0, 1.0))
}

/// The name at the start of a Tool Property row, as wide as every other
/// row's so their controls line up.
fn row_label(ui: &mut Ui, label: &str, theme: &Theme) {
    ui.add_sized(
        vec2(78.0, 18.0),
        egui::Label::new(
            RichText::new(label)
                .size(theme.small_size + 0.5)
                .color(theme.muted),
        ),
    );
}

/// A labelled horizontal slider row.
fn slider_row(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    log: bool,
    theme: &Theme,
) -> bool {
    ui.horizontal(|ui| {
        row_label(ui, label, theme);
        ui.spacing_mut().slider_width = (ui.available_width() - 62.0).max(36.0);
        ui.add(
            egui::Slider::new(value, range)
                .logarithmic(log)
                .max_decimals(if log { 1 } else { 0 })
                .suffix(suffix),
        )
        .changed()
    })
    .inner
}

impl SuePaint {
    /// A palette of its own for what is also a tab elsewhere (Clip Studio
    /// Paint's Material, Layer Property, History, Animation Cels, Sub
    /// View, Auto Action, Item Bank and Info palettes).
    pub(crate) fn single_palette(
        &mut self,
        ui: &mut Ui,
        palette: crate::dock::Palette,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::dock::Palette;
        let (body, _) = header(
            ui,
            rect,
            &[(palette.title(), palette.icon(), true)],
            0,
            theme,
        );
        match palette {
            Palette::Material => self.material_panel(ui, body, cx, theme),
            Palette::LayerProperty => {
                let current = self.layer_info(&cx.world.arena);
                let mut inner = child(ui, body.shrink(8.0), "layer-property-palette");
                self.layer_property(&mut inner, current.as_ref(), cx, theme);
            }
            Palette::History => self.history_tab(ui, body, cx, theme),
            Palette::Cels => self.cels_tab(ui, body, cx, theme),
            Palette::SubView => self.sub_view(ui, body.shrink(6.0), cx, theme),
            Palette::AutoAction => self.auto_tab(ui, body, cx, theme),
            Palette::ItemBank => self.item_bank_tab(ui, body, cx, theme),
            Palette::Info => self.info_tab(ui, body, cx, theme),
            // Torn off their palettes' tabs.
            Palette::ToolProperty => {
                let was = self.tool_tab;
                self.tool_tab = 1;
                self.tool_panel_body(ui, body, cx, theme);
                self.tool_tab = was;
            }
            Palette::BrushGrid => self.brush_size_grid(ui, body.shrink2(vec2(6.0, 6.0)), theme),
            Palette::ColorSlider => {
                let mut inner = child(ui, body.shrink(8.0), "color-slider-palette");
                self.color_sliders(&mut inner, theme);
            }
            Palette::Intermediate => self.intermediate_colors(ui, body.shrink(8.0), theme),
            Palette::Approximate => self.approximate_colors(ui, body.shrink(8.0), theme),
            Palette::ColorHistory => {
                let was = self.set_tab;
                self.set_tab = 1;
                self.color_set_body(ui, body, cx, theme);
                self.set_tab = was;
            }
            _ => {}
        }
    }

    /// A palette's tabs where some are palettes of their own (Clip Studio
    /// Paint's palettes stacked as tabs): a tab whose palette is open
    /// elsewhere (docked, floating or in a window) does not show here, and
    /// dragging one of the others tears it off into a palette of its own.
    /// `tabs` are (name, icon, enabled, its own palette); returns the body
    /// and the tab showing (an index into `tabs`).
    fn torn_header(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        tabs: &[(String, Icon, bool, Option<crate::dock::Palette>)],
        selected: u8,
        theme: &Theme,
    ) -> (Rect, u8) {
        // Every palette is one of its own in the dock, so a palette there
        // shows only its own tab; a phone's sheet, which shows one palette
        // at a time, keeps the others as its tabs.
        let in_sheet = self.sheet.is_some();
        let shown: Vec<usize> = (0..tabs.len())
            .filter(|at| in_sheet || tabs[*at].3.is_none())
            .collect();
        let mut chosen = usize::from(selected);
        if !shown.contains(&chosen) {
            chosen = shown.first().copied().unwrap_or(0);
        }
        let visible: Vec<(String, Icon, bool)> = shown
            .iter()
            .map(|at| (tabs[*at].0.clone(), tabs[*at].1, tabs[*at].2))
            .collect();
        let tearing: Vec<bool> = shown.iter().map(|at| tabs[*at].3.is_some()).collect();
        let place = shown.iter().position(|at| *at == chosen).unwrap_or(0);
        let (body, clicked, dragged) =
            header_tearing(ui, rect, &visible, &tearing, place as u8, theme);
        if let Some(tab) = clicked.and_then(|tab| shown.get(usize::from(tab))) {
            chosen = *tab;
        }
        if let Some((tab, from)) = dragged
            && let Some(palette) = shown.get(usize::from(tab)).and_then(|at| tabs[*at].3)
        {
            self.dock.begin_drag(palette, from);
        }
        (body, chosen as u8)
    }
}
