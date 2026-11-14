//! Tabs: the strip across the top of what they switch, and the column of
//! them down the side of a settings page.
//!
//! - [`TabStrip`]: tabs side by side, each a name with an optional icon
//!   and close button. A line under the chosen tab glides to the next one
//!   chosen. When the strip runs out of room, tabs with an icon give up
//!   their name (the chosen one last), then the names that are left are
//!   cut short. A tab can be dragged to another place in the strip, and
//!   out of it to tear it off.
//! - [`SidebarTabs`]: tabs one under the other, each an icon and a name,
//!   the chosen one filled and marked by a bar that glides along the
//!   column.
//!
//! Both are walked with the arrow keys (Home and End go to the ends), and
//! a tab chosen by key is shown at once. A tab's identity comes from the
//! salt its caller gives it, never from its place, so a tab that moves
//! keeps its hover, its focus and its drag.

use egui::{
    Color32, Context, CursorIcon, EventFilter, Id, Key, Modifiers, PointerButton, Pos2, Rect,
    Response, Role, Sense, Stroke, Ui, Vec2, WidgetInfo, pos2, vec2,
};

use super::focus::{Roving, focus_ring};
use super::list::{blend, ground, one_line, paint_line, salted, scoped, state_fill, under_pointer};
use super::motion::{self, Motion, Pace};
use super::tokens::{Density, Elevation, Radius, Space, TextRole};
use crate::Theme;
use crate::icons::{self, Icon};

/// What a value on its way from one place to another remembers.
#[derive(Clone, Copy, Debug)]
struct Glide {
    from: f32,
    to: f32,
    /// When it set out, in the input's seconds.
    start: f64,
}

/// A value that follows `target`, eased over the time of `pace` from
/// wherever it was when the target changed. The first time an `id` is
/// seen it is at its target already; with transitions off it always is.
/// It asks for frames only while it moves.
pub(super) fn glide(ctx: &Context, id: Id, target: f32, pace: Pace) -> f32 {
    let seconds = f64::from(Motion::of(ctx).seconds(pace));
    let now = ctx.input(|input| input.time);
    let at = |glide: &Glide| {
        let t = if seconds > 0.0 {
            ((now - glide.start) / seconds).clamp(0.0, 1.0) as f32
        } else {
            1.0
        };
        (
            glide.from + (glide.to - glide.from) * motion::ease(t),
            t < 1.0,
        )
    };
    let (value, moving) = ctx.data_mut(|data| {
        let glide = data.get_temp_mut_or_insert_with(id, || Glide {
            from: target,
            to: target,
            start: now,
        });
        if glide.to != target {
            // It sets out again from where it is now.
            let (here, _) = at(glide);
            *glide = Glide {
                from: here,
                to: target,
                start: now,
            };
        }
        at(glide)
    });
    if moving {
        ctx.request_repaint();
    }
    value
}

/// Walks keyboard focus among `members`, in reading order, while one of
/// them holds it: the arrow keys of `axis` to the next and the previous
/// (wrapping), Home and End to the ends. Returns the place of the member
/// focus moved to this frame.
///
/// Unlike a [`FocusScope`](super::focus::FocusScope) it never takes focus
/// by itself: a strip that appears leaves focus where it was, as a page
/// with a tab strip on it must.
pub(super) fn rove(ctx: &Context, members: &[Id], axis: Roving) -> Option<usize> {
    let focused = ctx.memory(|memory| memory.focused())?;
    let index = members.iter().position(|member| *member == focused)?;
    let (across, along) = (
        matches!(axis, Roving::Horizontal | Roving::Both),
        matches!(axis, Roving::Vertical | Roving::Both),
    );
    // The arrows are the strip's: egui does not also move focus to
    // whatever lies in that direction.
    ctx.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            focused,
            EventFilter {
                horizontal_arrows: across,
                vertical_arrows: along,
                ..Default::default()
            },
        );
    });
    let pressed = |key: Key| ctx.input_mut(|input| input.consume_key(Modifiers::NONE, key));
    let count = members.len();
    let mut target = index;
    if (across && pressed(Key::ArrowRight)) || (along && pressed(Key::ArrowDown)) {
        target = (target + 1) % count;
    }
    if (across && pressed(Key::ArrowLeft)) || (along && pressed(Key::ArrowUp)) {
        target = (target + count - 1) % count;
    }
    if pressed(Key::Home) {
        target = 0;
    }
    if pressed(Key::End) {
        target = count - 1;
    }
    if target == index {
        return None;
    }
    ctx.memory_mut(|memory| memory.request_focus(members[target]));
    Some(target)
}

/// One tab of a [`TabStrip`] or of [`SidebarTabs`].
#[derive(Clone, Copy, Debug)]
#[must_use = "a tab is shown by the strip it is handed to"]
pub struct Tab<'a> {
    key: Id,
    label: &'a str,
    icon: Option<Icon>,
    closable: bool,
}

impl<'a> Tab<'a> {
    /// A tab named `label`. `salt` names it among the tabs of its strip
    /// and stays the same from frame to frame (an entity's id, a panel's
    /// key), wherever the tab stands.
    pub fn new(salt: impl std::hash::Hash + std::fmt::Debug, label: &'a str) -> Self {
        Self {
            key: Id::unique("sue-tab").with(salt),
            label,
            icon: None,
            closable: false,
        }
    }

    /// The picture before its name. A tab with one keeps it when the
    /// strip runs out of room for its name.
    pub const fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Whether it has a close button (a click with the middle button
    /// closes it too).
    pub const fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }
}

/// How a tab of a strip is drawn, after the strip has shared out its room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Fitted {
    /// Its width.
    pub width: f32,
    /// Its name shows (whole or cut short); without it only its icon does.
    pub named: bool,
}

/// The widths a tab of a strip can be drawn at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Widths {
    /// With its name whole.
    pub full: f32,
    /// With its icon alone, when it has one.
    pub icon: Option<f32>,
    /// The least it can be with its name cut short.
    pub least: f32,
}

/// Shares `room` out among tabs of `widths`, of which `chosen` is shown:
/// every tab whole when they fit; else the tabs with an icon give up
/// their names, the chosen one last; else the tabs that still have a name
/// share what is left evenly, never under their least. What then still
/// does not fit runs past the end of the strip.
pub(super) fn fit(widths: &[Widths], chosen: Option<usize>, room: f32) -> Vec<Fitted> {
    let whole = |index: usize, named: bool| Fitted {
        width: match (named, widths[index].icon) {
            (false, Some(icon)) => icon,
            _ => widths[index].full,
        },
        named: named || widths[index].icon.is_none(),
    };
    let total = |fitted: &[Fitted]| fitted.iter().map(|tab| tab.width).sum::<f32>();
    let mut fitted: Vec<Fitted> = (0..widths.len()).map(|index| whole(index, true)).collect();
    if total(&fitted) <= room {
        return fitted;
    }
    // The tabs with an icon that are not chosen, then the chosen one.
    let mut fold: Vec<usize> = (0..widths.len())
        .filter(|&index| Some(index) != chosen && widths[index].icon.is_some())
        .collect();
    fold.extend(chosen.filter(|&index| index < widths.len() && widths[index].icon.is_some()));
    for index in fold {
        fitted[index] = whole(index, false);
        if total(&fitted) <= room {
            return fitted;
        }
    }
    // Cut the names that are left to one even width: a tab narrower
    // than its share keeps its width and leaves the rest to the others.
    let named: Vec<usize> = (0..widths.len())
        .filter(|&index| widths[index].icon.is_none())
        .collect();
    let fixed: f32 = (0..widths.len())
        .filter(|index| !named.contains(index))
        .map(|index| fitted[index].width)
        .sum();
    let mut left = (room - fixed).max(0.0);
    let mut open: Vec<usize> = named;
    open.sort_by(|a, b| widths[*a].full.total_cmp(&widths[*b].full));
    let mut count = open.len();
    for index in open {
        let share = left / count.max(1) as f32;
        let width = widths[index].full.min(share).max(widths[index].least);
        fitted[index].width = width;
        left = (left - width).max(0.0);
        count -= 1;
    }
    fitted
}

/// What a tab strip told its caller.
#[derive(Clone, Debug)]
pub struct TabsOutput {
    /// The strip as a whole.
    pub response: Response,
    /// The tab to show now: clicked, or reached with the arrow keys. It is
    /// never the one that was shown already.
    pub chosen: Option<usize>,
    /// The tab whose close button (or middle button click) asked to close
    /// it.
    pub closed: Option<usize>,
    /// A dragged tab passed the middle of its neighbour: the caller moves
    /// the first to the place of the second. The tab goes on being dragged
    /// in its new place.
    pub moved: Option<(usize, usize)>,
    /// A dragged tab left the strip, at this point: the caller tears it
    /// off (into a window of its own, another strip). It is said once per
    /// drag.
    pub torn: Option<(usize, Pos2)>,
}

/// What a tab being dragged remembers.
#[derive(Clone, Copy, Debug)]
struct Grab {
    /// From the pointer to the tab's left edge.
    offset: f32,
    /// It was torn off already.
    torn: bool,
}

/// Tabs side by side over what they switch.
#[derive(Clone, Copy, Debug)]
#[must_use = "a tab strip draws nothing until it is shown"]
pub struct TabStrip<'a> {
    id: Id,
    tabs: &'a [Tab<'a>],
    chosen: Option<usize>,
    reorder: bool,
    tear: bool,
    label: &'a str,
}

impl<'a> TabStrip<'a> {
    /// A strip of `tabs`, of which the one at `chosen` is shown. `salt`
    /// names it among what is drawn in the same `Ui` and stays the same
    /// from frame to frame.
    pub fn new(
        salt: impl std::hash::Hash + std::fmt::Debug,
        tabs: &'a [Tab<'a>],
        chosen: usize,
    ) -> Self {
        Self {
            id: salted("sue-tab-strip", salt),
            tabs,
            chosen: (chosen < tabs.len()).then_some(chosen),
            reorder: false,
            tear: false,
            label: "",
        }
    }

    /// Whether a tab can be dragged to another place in the strip.
    pub const fn reorder(mut self, reorder: bool) -> Self {
        self.reorder = reorder;
        self
    }

    /// Whether a tab can be dragged out of the strip to tear it off.
    pub const fn tear(mut self, tear: bool) -> Self {
        self.tear = tear;
        self
    }

    /// What the strip switches, for a screen reader.
    pub const fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// The space between a tab's edge and its content at `density`.
    fn padding(theme: &Theme, density: Density) -> f32 {
        theme.space(match density {
            Density::Compact => Space::S4,
            Density::Regular | Density::Touch => Space::S5,
        })
    }

    /// The side of a tab's close button at `density`.
    fn close_side(density: Density) -> f32 {
        density.icon()
    }

    /// How wide each tab can be.
    fn widths(&self, ui: &Ui, theme: &Theme, density: Density) -> Vec<Widths> {
        let pad = Self::padding(theme, density);
        let gap = theme.space(Space::S3);
        let side = density.icon();
        let font = theme.text_font(TextRole::Body);
        self.tabs
            .iter()
            .map(|tab| {
                let text = ui
                    .painter()
                    .layout_no_wrap(tab.label.to_owned(), font.clone(), Color32::PLACEHOLDER)
                    .size()
                    .x;
                let icon = tab.icon.map_or(0.0, |_| side + gap);
                let close = if tab.closable {
                    gap + Self::close_side(density)
                } else {
                    0.0
                };
                Widths {
                    full: pad + icon + text + close + pad,
                    icon: tab.icon.map(|_| pad + side + pad),
                    // An ellipsis and a letter or two.
                    least: pad + icon + font.size * 2.0 + close + pad,
                }
            })
            .collect()
    }

    /// Draws the strip across the width there is.
    pub fn show(mut self, ui: &mut Ui, theme: &Theme) -> TabsOutput {
        self.id = scoped(ui, self.id);
        let density = Density::of(ui);
        let height = density.tab_strip();
        let (strip, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let label = self.label;
        response.widget_info(|| WidgetInfo::labeled(Role::TabList, ui.is_enabled(), label));
        let mut output = TabsOutput {
            response,
            chosen: None,
            closed: None,
            moved: None,
            torn: None,
        };
        let widths = self.widths(ui, theme, density);
        let gaps = theme.space(Space::S1) * self.tabs.len().saturating_sub(1) as f32;
        let fitted = fit(&widths, self.chosen, strip.width() - gaps);
        let gap = theme.space(Space::S1);
        let enabled = ui.is_enabled();
        let painter = ui.painter().with_clip_rect(strip.intersect(ui.clip_rect()));
        // The hairline under the strip; the tabs and the line under the
        // chosen one lie over it.
        painter.hline(
            strip.x_range(),
            strip.bottom() - theme.border_width / 2.0,
            theme.hairline(),
        );
        let mut members = Vec::with_capacity(self.tabs.len());
        let mut left = strip.left();
        let mut chosen_rect = None;
        let mut lifted = None;
        let pointer = ui.input(|input| input.pointer.interact_pos());
        let mut places = Vec::with_capacity(self.tabs.len());
        for (index, (tab, fitted)) in self.tabs.iter().zip(&fitted).enumerate() {
            let rect = Rect::from_min_size(pos2(left, strip.top()), vec2(fitted.width, height));
            left += fitted.width + gap;
            places.push(rect);
            let id = self.id.with(tab.key);
            let sense = match (enabled, self.reorder || self.tear) {
                (false, _) => Sense::hover(),
                (true, true) => Sense::click_and_drag(),
                (true, false) => Sense::click(),
            };
            let tab_response = ui.interact(rect, id, sense);
            let selected = Some(index) == self.chosen;
            tab_response
                .widget_info(|| WidgetInfo::selected(Role::Tab, enabled, selected, tab.label));
            members.push(tab_response.id);
            if selected {
                chosen_rect = Some(rect);
            }
            // The close button comes after the tab, so a click on it is
            // its own.
            let close = (tab.closable && fitted.named).then(|| {
                let side = Self::close_side(density);
                let pad = Self::padding(theme, density);
                let at = Rect::from_center_size(
                    pos2(rect.right() - pad - side / 2.0, rect.center().y),
                    Vec2::splat(side),
                );
                let hit =
                    Rect::from_center_size(at.center(), Vec2::splat(density.min_hit().min(height)))
                        .intersect(rect);
                let sense = if enabled {
                    Sense::CLICK
                } else {
                    Sense::hover()
                };
                (at, ui.interact(hit, id.with("sue-close"), sense))
            });
            if let Some((_, button)) = &close
                && button.clicked()
            {
                output.closed = Some(index);
            }
            if tab.closable && enabled && tab_response.clicked_by(PointerButton::Middle) {
                output.closed = Some(index);
            }
            if tab_response.clicked() && !selected {
                output.chosen = Some(index);
            }
            if tab_response.drag_started()
                && let Some(at) = pointer
            {
                let grab = Grab {
                    offset: at.x - rect.left(),
                    torn: false,
                };
                ui.data_mut(|data| data.insert_temp(id.with("sue-grab"), grab));
            }
            if tab_response.dragged() {
                lifted = Some((index, id, rect, fitted.named));
                continue;
            }
            if !ui.is_rect_visible(rect) {
                continue;
            }
            Self::paint_tab(
                ui,
                theme,
                &tab_response,
                (tab, fitted.named, selected),
                rect,
            );
            if let Some((at, button)) = close {
                paint_close(
                    ui,
                    theme,
                    &button,
                    at,
                    (
                        selected
                            || under_pointer(&tab_response)
                            || tab_response.is_pointer_button_down_on(),
                        enabled,
                    ),
                );
            }
            if !fitted.named || fitted.width + 0.5 < widths[index].full {
                tab_response.on_hover_text(tab.label);
            }
        }
        // The line under the chosen tab, gliding to it.
        if let Some(rect) = chosen_rect {
            let inset = theme.radius(Radius::Sm);
            let from = glide(
                ui.ctx(),
                self.id.with("sue-line-left"),
                rect.left() + inset,
                Pace::Base,
            );
            let to = glide(
                ui.ctx(),
                self.id.with("sue-line-right"),
                rect.right() - inset,
                Pace::Base,
            );
            let thick = Theme::STRONG_STROKE;
            let line =
                Rect::from_min_max(pos2(from, strip.bottom() - thick), pos2(to, strip.bottom()));
            painter.rect_filled(
                line,
                theme.corner(Radius::Full),
                theme.selected_stroke().color,
            );
        }
        if let Some((index, id, rect, named)) = lifted {
            self.drag_ui(
                ui,
                theme,
                (index, id, rect, named),
                (strip, &places),
                pointer,
                &mut output,
            );
        }
        // Arrow keys: the tab they reach is shown.
        if let Some(index) = rove(ui.ctx(), &members, Roving::Horizontal)
            && Some(index) != self.chosen
        {
            output.chosen = Some(index);
        }
        output
    }

    /// Paints one tab at rest.
    fn paint_tab(
        ui: &Ui,
        theme: &Theme,
        response: &Response,
        (tab, named, selected): (&Tab<'_>, bool, bool),
        rect: Rect,
    ) {
        let density = Density::of(ui);
        let enabled = ui.is_enabled();
        let inset = theme.space(Space::S2);
        let body = rect.shrink2(vec2(0.0, inset));
        let radius = theme.radius(Radius::Sm);
        let fill = state_fill(ui, response, theme, Color32::TRANSPARENT, false, enabled);
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        if fill != Color32::TRANSPARENT {
            painter.rect_filled(body, radius, fill);
        }
        let hot = motion::hover_t(
            ui.ctx(),
            response.id.with("sue-ink"),
            selected || under_pointer(response),
        );
        let ink = if !enabled {
            theme.text_disabled(ground(ui, theme))
        } else {
            blend(theme.text_muted(), theme.text, hot)
        };
        paint_content(
            &painter,
            theme,
            density,
            (tab, named, ink),
            rect,
            Self::padding(theme, density),
            tab.closable && named,
        );
        focus_ring(ui, &response.clone().with_new_rect(body), radius, theme);
    }

    /// The tab being dragged: it follows the pointer along the strip,
    /// trades places with the neighbour whose middle it passes and tears
    /// off when it leaves the strip.
    fn drag_ui(
        &self,
        ui: &Ui,
        theme: &Theme,
        (index, id, rect, named): (usize, Id, Rect, bool),
        (strip, places): (Rect, &[Rect]),
        pointer: Option<Pos2>,
        output: &mut TabsOutput,
    ) {
        let Some(at) = pointer else { return };
        let key = id.with("sue-grab");
        let mut grab: Grab = ui.data(|data| data.get_temp(key)).unwrap_or(Grab {
            offset: rect.width() / 2.0,
            torn: false,
        });
        let reach = Density::of(ui).tab_strip();
        if self.tear && !grab.torn && !strip.expand(reach).contains(at) {
            grab.torn = true;
            output.torn = Some((index, at));
            ui.data_mut(|data| data.insert_temp(key, grab));
        }
        if grab.torn {
            // It is the caller's now; the strip keeps its place empty.
            return;
        }
        let left = if self.reorder {
            (at.x - grab.offset).clamp(
                strip.left(),
                (strip.right() - rect.width()).max(strip.left()),
            )
        } else {
            rect.left()
        };
        let shown = Rect::from_min_size(pos2(left, rect.top()), rect.size());
        if self.reorder {
            let middle = shown.center().x;
            if index + 1 < places.len() && middle > places[index + 1].center().x {
                output.moved = Some((index, index + 1));
            } else if index > 0 && middle < places[index - 1].center().x {
                output.moved = Some((index, index - 1));
            }
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }
        // Lifted over the strip.
        let density = Density::of(ui);
        let body = shown.shrink2(vec2(0.0, theme.space(Space::S2)));
        let painter = ui.painter().with_clip_rect(strip.intersect(ui.clip_rect()));
        let radius = theme.radius(Radius::Sm);
        painter.add(
            theme
                .elevation(Elevation::Raised)
                .as_shape(body, theme.corner(Radius::Sm)),
        );
        painter.rect_filled(body, radius, theme.fill_selected());
        let tab = &self.tabs[index];
        paint_content(
            &painter,
            theme,
            density,
            (tab, named, theme.text),
            shown,
            Self::padding(theme, density),
            false,
        );
    }
}

/// Paints a tab's icon and name in `rect`, leaving room for a close
/// button at its end when `close` says so.
fn paint_content(
    painter: &egui::Painter,
    theme: &Theme,
    density: Density,
    (tab, named, ink): (&Tab<'_>, bool, Color32),
    rect: Rect,
    pad: f32,
    close: bool,
) {
    let side = density.icon();
    let gap = theme.space(Space::S3);
    let middle = rect.center().y;
    let mut left = rect.left() + pad;
    if let Some(icon) = tab.icon {
        let centered = if named {
            left + side / 2.0
        } else {
            rect.center().x
        };
        let at = Rect::from_center_size(pos2(centered, middle), Vec2::splat(side));
        icons::paint(painter, at, icon, ink);
        left += side + gap;
    }
    if named {
        let mut right = rect.right() - pad;
        if close {
            right -= TabStrip::close_side(density) + gap;
        }
        let galley = one_line(
            painter,
            tab.label,
            theme.text_font(TextRole::Body),
            ink,
            right - left,
        );
        paint_line(painter, left, middle, galley);
    }
}

/// Paints a close button: a cross that shows on the chosen tab and under
/// the pointer, on a round fill while the pointer is on it.
fn paint_close(
    ui: &Ui,
    theme: &Theme,
    button: &Response,
    at: Rect,
    (shown, enabled): (bool, bool),
) {
    let lit = motion::hover_t(
        ui.ctx(),
        button.id.with("sue-hover"),
        enabled && button.hovered(),
    );
    let seen = motion::hover_t(
        ui.ctx(),
        button.id.with("sue-shown"),
        shown || button.has_focus(),
    );
    if seen <= 0.0 {
        return;
    }
    let painter = ui.painter();
    if lit > 0.0 {
        let fill = if button.is_pointer_button_down_on() {
            theme.fill_pressed()
        } else {
            theme.fill_hover()
        };
        painter.rect_filled(
            at,
            theme.radius(Radius::Xs),
            blend(Color32::TRANSPARENT, fill, lit),
        );
    }
    let ink = if enabled {
        blend(theme.text_muted(), theme.text, lit)
    } else {
        theme.text_disabled(ground(ui, theme))
    };
    paint_cross(
        painter,
        at,
        Stroke::new(theme.icon_stroke(at.width()), ink.gamma_multiply(seen)),
    );
    focus_ring(
        ui,
        &button.clone().with_new_rect(at),
        theme.radius(Radius::Xs),
        theme,
    );
}

/// A cross in `rect`.
pub(super) fn paint_cross(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    let reach = rect.width().min(rect.height()) * 0.22;
    let center = rect.center();
    for (x, y) in [(1.0, 1.0), (1.0, -1.0)] {
        let arm = vec2(x, y) * reach;
        painter.line_segment([center - arm, center + arm], stroke);
    }
}

/// What a column of side tabs told its caller.
#[derive(Clone, Debug)]
pub struct SideOutput {
    /// The column as a whole.
    pub response: Response,
    /// The tab to show now: clicked, or reached with the arrow keys. It is
    /// never the one that was shown already.
    pub chosen: Option<usize>,
}

/// Tabs one under the other, down the side of what they switch: the
/// pages of Settings, the sections of a long dialog.
#[derive(Clone, Copy, Debug)]
#[must_use = "side tabs draw nothing until they are shown"]
pub struct SidebarTabs<'a> {
    id: Id,
    tabs: &'a [Tab<'a>],
    chosen: Option<usize>,
    label: &'a str,
}

impl<'a> SidebarTabs<'a> {
    /// A column of `tabs`, of which the one at `chosen` is shown. `salt`
    /// names it among what is drawn in the same `Ui`.
    pub fn new(
        salt: impl std::hash::Hash + std::fmt::Debug,
        tabs: &'a [Tab<'a>],
        chosen: usize,
    ) -> Self {
        Self {
            id: salted("sue-side-tabs", salt),
            tabs,
            chosen: (chosen < tabs.len()).then_some(chosen),
            label: "",
        }
    }

    /// What the column switches, for a screen reader.
    pub const fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Draws the column, as wide as there is room.
    pub fn show(mut self, ui: &mut Ui, theme: &Theme) -> SideOutput {
        self.id = scoped(ui, self.id);
        let density = Density::of(ui);
        let enabled = ui.is_enabled();
        let row = density.row();
        let gap = theme.space(Space::S1);
        let width = ui.available_width();
        let count = self.tabs.len() as f32;
        let height = (row * count + gap * (count - 1.0)).max(0.0);
        let (column, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let label = self.label;
        response.widget_info(|| WidgetInfo::labeled(Role::TabList, enabled, label));
        let mut output = SideOutput {
            response,
            chosen: None,
        };
        let pad = theme.space(Space::S4);
        let side = density.icon();
        let radius = theme.radius(Radius::Md);
        let mut members = Vec::with_capacity(self.tabs.len());
        let mut chosen_rect = None;
        for (index, tab) in self.tabs.iter().enumerate() {
            let top = column.top() + index as f32 * (row + gap);
            let rect = Rect::from_min_size(pos2(column.left(), top), vec2(width, row));
            let id = self.id.with(tab.key);
            let sense = if enabled {
                Sense::click()
            } else {
                Sense::hover()
            };
            let tab_response = ui.interact(rect, id, sense);
            let selected = Some(index) == self.chosen;
            tab_response
                .widget_info(|| WidgetInfo::selected(Role::Tab, enabled, selected, tab.label));
            members.push(tab_response.id);
            if tab_response.clicked() && !selected {
                output.chosen = Some(index);
            }
            if selected {
                chosen_rect = Some(rect);
            }
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let fill = state_fill(
                ui,
                &tab_response,
                theme,
                Color32::TRANSPARENT,
                selected,
                enabled,
            );
            let painter = ui.painter();
            if fill != Color32::TRANSPARENT {
                painter.rect_filled(rect, radius, fill);
            }
            let ink = if !enabled {
                theme.text_disabled(ground(ui, theme))
            } else if selected {
                theme.text
            } else {
                let hot =
                    motion::hover_t(ui.ctx(), id.with("sue-ink"), under_pointer(&tab_response));
                blend(theme.text_muted(), theme.text, hot)
            };
            let mut left = rect.left() + pad;
            if let Some(icon) = tab.icon {
                let at = Rect::from_center_size(
                    pos2(left + side / 2.0, rect.center().y),
                    Vec2::splat(side),
                );
                icons::paint(painter, at, icon, ink);
                left += side + theme.space(Space::S4);
            }
            let galley = one_line(
                painter,
                tab.label,
                theme.text_font(TextRole::Body),
                ink,
                rect.right() - pad - left,
            );
            let cut = galley.size().x + 0.5
                < painter
                    .layout_no_wrap(tab.label.to_owned(), theme.text_font(TextRole::Body), ink)
                    .size()
                    .x;
            paint_line(painter, left, rect.center().y, galley);
            focus_ring(ui, &tab_response, radius, theme);
            if cut {
                tab_response.on_hover_text(tab.label);
            }
        }
        // The bar at the chosen tab's start, gliding to it.
        if let Some(rect) = chosen_rect {
            let reach = rect.height() * 0.25;
            let top = glide(
                ui.ctx(),
                self.id.with("sue-bar-top"),
                rect.top() + reach,
                Pace::Base,
            );
            let bottom = glide(
                ui.ctx(),
                self.id.with("sue-bar-bottom"),
                rect.bottom() - reach,
                Pace::Base,
            );
            let thick = Theme::STRONG_STROKE;
            let left = rect.left() + theme.space(Space::S1);
            let bar = Rect::from_min_max(pos2(left, top), pos2(left + thick, bottom));
            ui.painter().rect_filled(
                bar,
                theme.corner(Radius::Full),
                theme.selected_stroke().color,
            );
        }
        if let Some(index) = rove(ui.ctx(), &members, Roving::Vertical)
            && Some(index) != self.chosen
        {
            output.chosen = Some(index);
        }
        output
    }
}

/// A strip of `tabs` with the one at `chosen` shown; see [`TabStrip`].
pub fn tab_strip<'a>(
    salt: impl std::hash::Hash + std::fmt::Debug,
    tabs: &'a [Tab<'a>],
    chosen: usize,
) -> TabStrip<'a> {
    TabStrip::new(salt, tabs, chosen)
}

/// A column of `tabs` with the one at `chosen` shown; see
/// [`SidebarTabs`].
pub fn sidebar_tabs<'a>(
    salt: impl std::hash::Hash + std::fmt::Debug,
    tabs: &'a [Tab<'a>],
    chosen: usize,
) -> SidebarTabs<'a> {
    SidebarTabs::new(salt, tabs, chosen)
}

#[cfg(test)]
mod tests {
    use egui::{Key, Modifiers, PointerButton};

    use super::super::list::harness::{Run, at};
    use super::*;

    fn widths(full: f32, icon: Option<f32>) -> Widths {
        Widths {
            full,
            icon,
            least: 40.0,
        }
    }

    fn sizes(fitted: &[Fitted]) -> Vec<(f32, bool)> {
        fitted.iter().map(|tab| (tab.width, tab.named)).collect()
    }

    #[test]
    fn tabs_that_fit_keep_their_names_whole() {
        let tabs = [widths(100.0, Some(40.0)), widths(80.0, None)];
        let fitted = fit(&tabs, Some(0), 180.0);
        assert_eq!(sizes(&fitted), [(100.0, true), (80.0, true)]);
    }

    #[test]
    fn short_of_room_tabs_with_icons_give_up_their_names_the_chosen_last() {
        let tabs = [
            widths(100.0, Some(40.0)),
            widths(100.0, Some(40.0)),
            widths(100.0, Some(40.0)),
        ];
        // One folded is enough.
        let fitted = fit(&tabs, Some(0), 240.0);
        assert_eq!(
            sizes(&fitted),
            [(100.0, true), (40.0, false), (100.0, true)]
        );
        // Every one but the chosen.
        let fitted = fit(&tabs, Some(1), 180.0);
        assert_eq!(
            sizes(&fitted),
            [(40.0, false), (100.0, true), (40.0, false)]
        );
        // The chosen one too.
        let fitted = fit(&tabs, Some(1), 130.0);
        assert_eq!(sizes(&fitted), [(40.0, false); 3]);
    }

    #[test]
    fn names_without_icons_share_what_is_left_and_narrow_ones_keep_theirs() {
        let tabs = [
            widths(60.0, None),
            widths(200.0, None),
            widths(300.0, None),
            widths(100.0, Some(40.0)),
        ];
        // 40 for the icon, 60 for the narrow name, 100 each for the two
        // wide ones.
        let fitted = fit(&tabs, Some(0), 300.0);
        assert_eq!(
            sizes(&fitted),
            [(60.0, true), (100.0, true), (100.0, true), (40.0, false)]
        );
        // Never under the least: the strip runs over instead.
        let fitted = fit(&tabs, None, 50.0);
        assert!(fitted[..3].iter().all(|tab| tab.width == 40.0 && tab.named));
    }

    /// Four tabs; the strip as drawn, where each tab is and its id.
    fn strip(
        ui: &mut Ui,
        theme: &Theme,
        order: &[u32],
        chosen: usize,
        drag: bool,
    ) -> (TabsOutput, Vec<Rect>, Vec<Id>) {
        let names = ["Layer", "Color", "Brush", "History"];
        let tabs: Vec<Tab<'_>> = order
            .iter()
            .map(|&key| Tab::new(key, names[key as usize]).closable(true))
            .collect();
        let strip = TabStrip::new("test", &tabs, chosen)
            .reorder(drag)
            .tear(drag);
        let ids: Vec<Id> = tabs
            .iter()
            .map(|tab| scoped(ui, strip.id).with(tab.key))
            .collect();
        let widths = strip.widths(ui, theme, Density::of(ui));
        let output = strip.show(ui, theme);
        let gap = theme.space(Space::S1);
        let mut left = output.response.rect.left();
        let rects = widths
            .iter()
            .map(|width| {
                let rect = Rect::from_min_size(
                    pos2(left, output.response.rect.top()),
                    vec2(width.full, output.response.rect.height()),
                );
                left += width.full + gap;
                rect
            })
            .collect();
        (output, rects, ids)
    }

    fn chosen(seen: &[TabsOutput]) -> Vec<usize> {
        seen.iter().filter_map(|output| output.chosen).collect()
    }

    fn closed(seen: &[TabsOutput]) -> Vec<usize> {
        seen.iter().filter_map(|output| output.closed).collect()
    }

    #[test]
    fn a_click_chooses_a_tab_and_a_click_on_the_chosen_one_says_nothing() {
        let mut run = Run::new();
        let order = [0, 1, 2, 3];
        let draw = |ui: &mut Ui, theme: &Theme| strip(ui, theme, &order, 0, false);
        let (_, rects, _) = run.frame(Vec::new(), draw).0;
        let seen = run.click(at(rects[2], 0.2, 0.5), |ui, theme| draw(ui, theme).0);
        assert_eq!(chosen(&seen), [2]);
        let seen = run.click(at(rects[0], 0.2, 0.5), |ui, theme| draw(ui, theme).0);
        assert!(chosen(&seen).is_empty());
    }

    #[test]
    fn the_close_button_and_the_middle_button_close_and_do_not_choose() {
        let mut run = Run::new();
        let order = [0, 1, 2, 3];
        let draw = |ui: &mut Ui, theme: &Theme| strip(ui, theme, &order, 0, false);
        let (_, rects, _) = run.frame(Vec::new(), draw).0;
        let pad = TabStrip::padding(&run.theme, Density::Regular);
        let side = TabStrip::close_side(Density::Regular);
        let cross = pos2(rects[1].right() - pad - side / 2.0, rects[1].center().y);
        let seen = run.click(cross, |ui, theme| draw(ui, theme).0);
        assert_eq!(closed(&seen), [1]);
        assert!(chosen(&seen).is_empty());
        let seen = run.click_with(
            at(rects[3], 0.2, 0.5),
            PointerButton::Middle,
            |ui, theme| draw(ui, theme).0,
        );
        assert_eq!(closed(&seen), [3]);
        assert!(chosen(&seen).is_empty());
    }

    #[test]
    fn the_arrow_keys_walk_the_tabs_and_show_the_one_they_reach() {
        let mut run = Run::new();
        let order = [0, 1, 2, 3];
        let (_, _, ids) = run
            .frame(Vec::new(), |ui, theme| strip(ui, theme, &order, 1, false))
            .0;
        run.focus(ids[1]);
        let _ = run.frame(Vec::new(), |ui, theme| strip(ui, theme, &order, 1, false));
        let output = run.key(Key::ArrowRight, Modifiers::NONE, |ui, theme| {
            strip(ui, theme, &order, 1, false).0
        });
        assert_eq!(output.chosen, Some(2));
        assert_eq!(run.focused(), Some(ids[2]));
        let output = run.key(Key::Home, Modifiers::NONE, |ui, theme| {
            strip(ui, theme, &order, 2, false).0
        });
        assert_eq!(output.chosen, Some(0));
    }

    #[test]
    fn tabs_that_appear_leave_keyboard_focus_where_it_was() {
        let mut run = Run::new();
        let order = [0, 1, 2, 3];
        let tabs = [Tab::new("a", "General"), Tab::new("b", "Files")];
        for _ in 0..3 {
            let _ = run.frame(Vec::new(), |ui, theme| {
                let _ = strip(ui, theme, &order, 2, false);
                SidebarTabs::new("side", &tabs, 1).show(ui, theme)
            });
        }
        assert_eq!(run.focused(), None, "a page with tabs on it takes no focus");
    }

    #[test]
    fn a_tab_is_known_by_its_salt_wherever_it_stands() {
        let mut run = Run::new();
        let (_, _, ids) = run
            .frame(Vec::new(), |ui, theme| {
                strip(ui, theme, &[0, 1, 2, 3], 0, false)
            })
            .0;
        run.focus(ids[1]);
        // The same tabs in another order: the focused one is now third.
        let order = [2, 0, 1, 3];
        let (_, _, moved) = run
            .frame(Vec::new(), |ui, theme| strip(ui, theme, &order, 0, false))
            .0;
        assert_eq!(moved[2], ids[1]);
        assert_eq!(run.focused(), Some(ids[1]));
        let output = run.key(Key::ArrowRight, Modifiers::NONE, |ui, theme| {
            strip(ui, theme, &order, 0, false).0
        });
        assert_eq!(output.chosen, Some(3), "the one after it in the new order");
    }

    #[test]
    fn a_dragged_tab_trades_places_and_tears_off_once_out_of_the_strip() {
        let mut run = Run::new();
        let order = [0, 1, 2, 3];
        let draw = |ui: &mut Ui, theme: &Theme| strip(ui, theme, &order, 0, true);
        let (_, rects, _) = run.frame(Vec::new(), draw).0;
        let from = at(rects[0], 0.3, 0.5);
        let past = pos2(rects[1].center().x + rects[0].width() * 0.5, from.y);
        let seen = run.drag(from, past, 6, |ui, theme| draw(ui, theme).0);
        assert!(seen.iter().any(|output| output.moved == Some((0, 1))));
        assert!(seen.iter().all(|output| output.torn.is_none()));
        // Down and out of the strip.
        let out = pos2(past.x, rects[0].bottom() + 200.0);
        let seen: Vec<TabsOutput> = (1..=4)
            .map(|step| {
                let at = past + (out - past) * (step as f32 / 4.0);
                run.frame(vec![egui::Event::PointerMoved(at)], |ui, theme| {
                    draw(ui, theme).0
                })
                .0
            })
            .collect();
        let torn: Vec<_> = seen.iter().filter_map(|output| output.torn).collect();
        assert_eq!(torn.len(), 1, "said once");
        assert_eq!(torn[0].0, 0);
        let after: Vec<_> = seen
            .iter()
            .skip_while(|output| output.torn.is_none())
            .collect();
        assert!(
            after.iter().all(|output| output.moved.is_none()),
            "a torn tab moves no more"
        );
    }

    #[test]
    fn a_strip_that_does_not_drag_takes_no_drag() {
        let mut run = Run::new();
        let order = [0, 1, 2, 3];
        let draw = |ui: &mut Ui, theme: &Theme| strip(ui, theme, &order, 0, false);
        let (_, rects, _) = run.frame(Vec::new(), draw).0;
        let seen = run.drag(
            at(rects[0], 0.3, 0.5),
            pos2(400.0, 400.0),
            6,
            |ui, theme| draw(ui, theme).0,
        );
        assert!(
            seen.iter()
                .all(|output| output.moved.is_none() && output.torn.is_none())
        );
    }

    #[test]
    fn side_tabs_choose_by_click_and_by_arrow() {
        let mut run = Run::new();
        let tabs = [
            Tab::new("a", "General").icon(Icon::Settings),
            Tab::new("b", "Files").icon(Icon::Folder),
            Tab::new("c", "Keys"),
        ];
        let show = |ui: &mut Ui, theme: &Theme, chosen: usize| {
            let side = SidebarTabs::new("side", &tabs, chosen);
            let ids: Vec<Id> = tabs
                .iter()
                .map(|tab| scoped(ui, side.id).with(tab.key))
                .collect();
            (side.show(ui, theme), ids)
        };
        let (output, ids) = run.frame(Vec::new(), |ui, theme| show(ui, theme, 0)).0;
        let rect = output.response.rect;
        let row = Density::Regular.row();
        let seen = run.click(
            pos2(rect.center().x, rect.top() + row * 2.5),
            |ui, theme| show(ui, theme, 0).0,
        );
        let picked: Vec<_> = seen.iter().filter_map(|output| output.chosen).collect();
        assert_eq!(picked, [2]);
        run.focus(ids[0]);
        let _ = run.frame(Vec::new(), |ui, theme| show(ui, theme, 0));
        let output = run.key(Key::ArrowDown, Modifiers::NONE, |ui, theme| {
            show(ui, theme, 0).0
        });
        assert_eq!(output.chosen, Some(1));
    }

    #[test]
    fn a_disabled_strip_answers_nothing() {
        let mut run = Run::new();
        let order = [0, 1, 2, 3];
        let draw = |ui: &mut Ui, theme: &Theme| {
            ui.add_enabled_ui(false, |ui| strip(ui, theme, &order, 0, true))
                .inner
        };
        let (_, rects, _) = run.frame(Vec::new(), draw).0;
        let seen = run.click(at(rects[2], 0.2, 0.5), |ui, theme| draw(ui, theme).0);
        assert!(chosen(&seen).is_empty() && closed(&seen).is_empty());
    }

    #[test]
    fn the_line_glides_to_its_place_and_then_stands_still() {
        let mut run = Run::new();
        let id = Id::unique("glide");
        let (first, _) = run.frame(Vec::new(), |ui, _| glide(ui.ctx(), id, 10.0, Pace::Base));
        assert_eq!(first, 10.0, "it starts in place");
        let (start, again) = run.frame(Vec::new(), |ui, _| glide(ui.ctx(), id, 110.0, Pace::Base));
        assert_eq!(start, 10.0, "the clock starts with the change");
        assert!(again);
        // 0.1 s into 0.16, eased: past the middle.
        let (moving, again) = run.frame(Vec::new(), |ui, _| glide(ui.ctx(), id, 110.0, Pace::Base));
        assert!(moving > 60.0 && moving < 110.0, "{moving}");
        assert!(again);
        let landed = run.settle(|ui, _| glide(ui.ctx(), id, 110.0, Pace::Base));
        assert_eq!(landed, 110.0);
        let (_, again) = run.frame(Vec::new(), |ui, _| glide(ui.ctx(), id, 110.0, Pace::Base));
        assert!(!again, "a line in place asks for nothing");
        // With transitions off it jumps.
        Motion {
            instant: true,
            ..run.theme.motion
        }
        .store(&run.ctx);
        let (jumped, _) = run.frame(Vec::new(), |ui, _| glide(ui.ctx(), id, -40.0, Pace::Base));
        assert_eq!(jumped, -40.0);
    }
}
