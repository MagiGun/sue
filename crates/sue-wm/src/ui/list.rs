//! Lists: the row that everything listing things is made of, the tree
//! row, the empty state, the divider and the section heading.
//!
//! - [`ListRow`]: one or two lines of text, something leading (an icon or
//!   a small picture), a note and actions at its end. It can be chosen,
//!   takes keyboard focus and answers Enter and Space as a click.
//! - [`TreeRow`]: a list row at a depth, with a chevron that opens and
//!   closes what is under it (the arrow keys do too).
//! - [`EmptyState`]: what a list shows when it holds nothing: a picture, a
//!   line that says so, a line that says what to do, and room for the
//!   buttons that do it.
//! - [`divider`] and [`heading`]: the hairline between groups, and the
//!   small muted line over one.
//!
//! Every size comes from the [`Density`] of the `Ui`, every color from
//! the [`Theme`]. A row's identity comes from the salt its caller gives
//! it, so a row that moves (a list that is sorted, a tree that opens)
//! keeps its hover, its focus and its animations.

use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, EventFilter, FontId, Galley, Id, Key, Label, Layout, Modifiers, Painter, Pos2, Rect,
    Response, RichText, Role, Sense, Stroke, TextureId, Ui, UiBuilder, Vec2, WidgetInfo, pos2,
    vec2,
};

use super::focus::focus_ring;
use super::motion::{self, Pace};
use super::tokens::{Density, Radius, Space, TextRole};
use crate::Theme;
use crate::icons::{self, Icon, IconStyle};

/// How much of a picture shows on something that cannot be used: the
/// share the tokens give disabled text.
pub(super) const DISABLED_PICTURE: f32 = 0.38;

/// One line of `text` no wider than `width`: what does not fit is cut
/// and ends in an ellipsis.
pub(super) fn one_line(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = TextWrapping::truncate_at_width(width.max(0.0));
    painter.layout_job(job)
}

/// Paints `galley` with its left end at `left` and its middle on `y`.
pub(super) fn paint_line(painter: &Painter, left: f32, y: f32, galley: Arc<Galley>) {
    let top = y - galley.size().y / 2.0;
    painter.galley(pos2(left, top), galley, Color32::PLACEHOLDER);
}

/// The color `t` of the way from `from` to `to`.
pub(super) fn blend(from: Color32, to: Color32, t: f32) -> Color32 {
    from.lerp_to_gamma(to, t.clamp(0.0, 1.0))
}

/// The fill `ui` draws on, as far as the frames around it say; a panel's
/// when they say nothing.
pub(super) fn ground(ui: &Ui, theme: &Theme) -> Color32 {
    let known = ui.stack().bg_color();
    if known.is_opaque() {
        known
    } else {
        theme.surface(1)
    }
}

/// Whether the pointer is on `response`: over it with nothing covering
/// it, or holding it down (while a press lasts egui says only the
/// pressed widget is hovered, and something drawn over it no longer
/// counts as covering it). While something else is being dragged over
/// it (a tab, a splitter) it is not: what lies under a drag does not
/// light up.
pub(super) fn under_pointer(response: &Response) -> bool {
    let other_dragged = response
        .ctx
        .dragged_id()
        .is_some_and(|dragged| dragged != response.id);
    response.hovered() || (response.contains_pointer() && !other_dragged)
}

/// The fill of something that can be clicked: `rest` (or the chosen fill
/// while `selected`), on its way to the hover fill while the pointer is
/// over it, and the pressed fill while it is held down.
pub(super) fn state_fill(
    ui: &Ui,
    response: &Response,
    theme: &Theme,
    rest: Color32,
    selected: bool,
    enabled: bool,
) -> Color32 {
    let rest = if selected {
        theme.fill_selected()
    } else {
        rest
    };
    // The pointer over a button inside it still lights it.
    let over = enabled && !selected && under_pointer(response);
    let hover = motion::hover_t(ui.ctx(), response.id.with("sue-hover"), over);
    if enabled && response.is_pointer_button_down_on() {
        theme.fill_pressed()
    } else {
        blend(rest, theme.fill_hover(), hover)
    }
}

/// The key of a component of `kind` named by its caller's `salt`: what
/// [`scoped`] makes its id from where it is drawn.
pub(super) fn salted(kind: &'static str, salt: impl std::hash::Hash + std::fmt::Debug) -> Id {
    Id::unique(kind).with(salt)
}

/// The id of the component whose key is `key`, drawn in `ui`: the key
/// within the id scope of `ui`, like egui's own widgets, so the same
/// salt in two panels names two components. It never depends on the
/// component's place among its neighbours.
pub(super) fn scoped(ui: &Ui, key: Id) -> Id {
    ui.scope_id().with(key)
}

/// What a row starts with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Leading {
    /// Nothing: the text starts at the row's padding.
    #[default]
    None,
    /// An icon, in the color of the row's text.
    Icon(Icon),
    /// A small square picture, such as a thumbnail.
    Picture(TextureId),
}

/// A small button at the end of a row or in the footer of a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowAction<'a> {
    /// Its picture.
    pub icon: Icon,
    /// Its name: what a hover and a screen reader say.
    pub label: &'a str,
    /// It is the lit half of an on and off pair (a favorite, a pin): the
    /// icon is drawn solid and in the text color.
    pub on: bool,
}

impl<'a> RowAction<'a> {
    /// An action that is off.
    #[must_use]
    pub const fn new(icon: Icon, label: &'a str) -> Self {
        Self {
            icon,
            label,
            on: false,
        }
    }

    /// Lit or not.
    #[must_use]
    pub const fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }
}

/// Draws `actions` leftward from `right`, each in a square `side` wide
/// with its middle on `y`, over the fill `under` (the chosen fill when
/// `under_selected`). Returns the one clicked and where the first of
/// them begins.
pub(super) fn actions_ui(
    ui: &Ui,
    theme: &Theme,
    owner: Id,
    actions: &[RowAction<'_>],
    (right, y, side): (f32, f32, f32),
    (under, under_selected): (Color32, bool),
    enabled: bool,
) -> (Option<usize>, f32) {
    let density = Density::of(ui);
    let mut clicked = None;
    let mut edge = right;
    for (index, action) in actions.iter().enumerate().rev() {
        let hit = Rect::from_center_size(pos2(edge - side / 2.0, y), Vec2::splat(side));
        edge -= side;
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(hit, owner.with(("sue-action", index)), sense);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, action.label));
        if !ui.is_rect_visible(hit) {
            continue;
        }
        let hot = enabled && response.hovered();
        let lit = motion::hover_t(ui.ctx(), response.id.with("sue-hover"), hot);
        // On a chosen row the pressed fill is the row's own.
        let fill = if under_selected {
            theme.fill_hover()
        } else {
            theme.fill_pressed()
        };
        let radius = theme.radius(Radius::Md);
        if lit > 0.0 {
            ui.painter()
                .rect_filled(hit, radius, blend(Color32::TRANSPARENT, fill, lit));
        }
        let ink = if !enabled {
            theme.text_disabled(under)
        } else if action.on {
            theme.text
        } else {
            let muted = if under_selected {
                theme.text_muted_on(theme.fill_selected())
            } else {
                theme.text_muted()
            };
            blend(muted, theme.text, lit)
        };
        let style = if action.on {
            IconStyle::Solid
        } else {
            IconStyle::Line
        };
        let picture = Rect::from_center_size(hit.center(), Vec2::splat(density.icon().min(side)));
        icons::paint_styled(ui.painter(), picture, action.icon, style, ink);
        focus_ring(ui, &response, radius, theme);
        if response.clicked() {
            clicked = Some(index);
        }
        if !action.label.is_empty() {
            response.on_hover_text(action.label);
        }
    }
    (clicked, edge)
}

/// What a row told its caller.
#[derive(Clone, Debug)]
pub struct RowOutput {
    /// The row itself: clicked, double clicked, hovered, focused, and what
    /// a context menu hangs on.
    pub response: Response,
    /// The action at its end that was clicked, by its place in the list
    /// the row was given.
    pub action: Option<usize>,
}

/// A row of a list.
#[derive(Clone, Copy, Debug)]
#[must_use = "a row draws nothing until it is shown"]
pub struct ListRow<'a> {
    id: Id,
    title: &'a str,
    detail: Option<&'a str>,
    note: Option<&'a str>,
    leading: Leading,
    actions: &'a [RowAction<'a>],
    selected: bool,
    enabled: bool,
    width: Option<f32>,
}

/// Where a tree row stands in its tree.
#[derive(Clone, Copy)]
struct Twig {
    depth: usize,
    expanded: Option<bool>,
}

impl<'a> ListRow<'a> {
    /// A row that says `title`. `salt` names it among what is drawn in
    /// the same `Ui` and stays the same from frame to frame (an entity's
    /// id, a path, a key), wherever the row stands among the others.
    pub fn new(salt: impl std::hash::Hash + std::fmt::Debug, title: &'a str) -> Self {
        Self {
            id: salted("sue-list-row", salt),
            title,
            detail: None,
            note: None,
            leading: Leading::None,
            actions: &[],
            selected: false,
            enabled: true,
            width: None,
        }
    }

    /// A second, smaller line under the title. The row grows to hold it.
    pub const fn detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail);
        self
    }

    /// A short muted note at the end of the row, before its actions (a
    /// size, a date, a shortcut).
    pub const fn note(mut self, note: &'a str) -> Self {
        self.note = Some(note);
        self
    }

    /// An icon before the text.
    pub const fn icon(mut self, icon: Icon) -> Self {
        self.leading = Leading::Icon(icon);
        self
    }

    /// What the row starts with.
    pub const fn leading(mut self, leading: Leading) -> Self {
        self.leading = leading;
        self
    }

    /// Small buttons at the end of the row.
    pub const fn actions(mut self, actions: &'a [RowAction<'a>]) -> Self {
        self.actions = actions;
        self
    }

    /// Whether it is the chosen one.
    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Whether it can be used. A row inside a disabled `Ui` cannot,
    /// whatever this says.
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Its width; without one it fills the width there is.
    pub const fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The height of a row at `density`: the density's row for one line,
    /// and room for the second line under it for two.
    #[must_use]
    pub fn height(theme: &Theme, density: Density, two_lines: bool) -> f32 {
        if two_lines {
            density.row() + theme.text_size(TextRole::Caption) + theme.space(Space::S2)
        } else {
            density.row()
        }
    }

    /// Draws the row.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> RowOutput {
        self.show_as(ui, theme, None).0
    }

    /// Draws the row, as a tree row when `twig` says where it stands.
    /// Returns what it told and whether its chevron was used.
    fn show_as(mut self, ui: &mut Ui, theme: &Theme, twig: Option<Twig>) -> (RowOutput, bool) {
        self.id = scoped(ui, self.id);
        let density = Density::of(ui);
        let enabled = self.enabled && ui.is_enabled();
        let height = Self::height(theme, density, self.detail.is_some());
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (_, rect) = ui.allocate_space(vec2(width, height));
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(rect, self.id, sense);
        let role = if twig.is_some() {
            Role::TreeItem
        } else {
            Role::ListBoxOption
        };
        let (title, selected) = (self.title, self.selected);
        response.widget_info(|| WidgetInfo::selected(role, enabled, selected, title));

        let pad = theme.space(Space::S4);
        let gap = theme.space(Space::S2);
        let side = density.icon();
        let middle = rect.center().y;
        let mut left = rect.left() + pad;
        let mut toggled = false;
        // The chevron of a tree row. It is made after the row, so a click
        // on it is the chevron's and not the row's.
        let mut chevron = None;
        if let Some(twig) = twig {
            left += twig.depth as f32 * theme.space(Space::S6);
            if let Some(open) = twig.expanded {
                let at = Rect::from_center_size(pos2(left + side / 2.0, middle), Vec2::splat(side));
                if enabled {
                    let hit = Rect::from_center_size(at.center(), vec2(side + gap * 2.0, height));
                    let twist = ui.interact(hit, self.id.with("sue-twist"), Sense::CLICK);
                    toggled |= twist.clicked();
                    toggled |= arrow_toggles(ui, &response, open);
                }
                chevron = Some((at, open));
            }
            left += side + gap;
        }
        let radius = theme.radius(Radius::Lg).min(height * 0.3);
        // What the row's text and icons lie on, for the disabled ink.
        let under = if self.selected {
            theme.fill_selected()
        } else {
            ground(ui, theme)
        };
        // The row's fill goes under its actions, which are drawn first so
        // that a click on one of them is theirs.
        let under_actions = ui.painter().add(egui::Shape::Noop);
        let (action, right) = actions_ui(
            ui,
            theme,
            self.id,
            self.actions,
            (rect.right() - gap, middle, density.min_hit().min(height)),
            (under, self.selected),
            enabled,
        );
        if ui.is_rect_visible(rect) {
            let painter = ui.painter().clone();
            let fill = state_fill(
                ui,
                &response,
                theme,
                Color32::TRANSPARENT,
                self.selected,
                enabled,
            );
            if fill != Color32::TRANSPARENT {
                painter.set(under_actions, egui::Shape::rect_filled(rect, radius, fill));
            }
            let lit = self.selected || (enabled && response.is_pointer_button_down_on());
            let (ink, soft) = if !enabled {
                let faint = theme.text_disabled(under);
                (faint, faint)
            } else if lit {
                (theme.text, theme.text_muted_on(theme.fill_selected()))
            } else {
                (theme.text, theme.text_muted())
            };
            if let Some((at, open)) = chevron {
                let turn = motion::timed(ui.ctx(), self.id.with("sue-turn"), open, Pace::Fast);
                paint_chevron(
                    &painter,
                    at,
                    turn,
                    Stroke::new(theme.icon_stroke(side), soft),
                );
            }
            match self.leading {
                Leading::None => {}
                Leading::Icon(icon) => {
                    let at =
                        Rect::from_center_size(pos2(left + side / 2.0, middle), Vec2::splat(side));
                    icons::paint(&painter, at, icon, ink);
                    left += side + pad;
                }
                Leading::Picture(texture) => {
                    let side = height - gap * 2.0;
                    let at =
                        Rect::from_center_size(pos2(left + side / 2.0, middle), Vec2::splat(side));
                    let whole = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                    let tint = if enabled {
                        Color32::WHITE
                    } else {
                        Color32::WHITE.gamma_multiply(DISABLED_PICTURE)
                    };
                    egui::Image::new((texture, at.size()))
                        .uv(whole)
                        .tint(tint)
                        .corner_radius(theme.corner(Radius::Xs))
                        .paint_at(ui, at);
                    left += side + pad;
                }
            }
            let mut right = right
                - if self.actions.is_empty() {
                    pad - gap
                } else {
                    gap
                };
            if let Some(note) = self.note {
                let font = theme.text_font(TextRole::Caption);
                let room = ((right - left) / 2.0).max(0.0);
                let galley = one_line(&painter, note, font, soft, room);
                right -= galley.size().x;
                paint_line(&painter, right, middle, galley);
                right -= pad;
            }
            let room = right - left;
            let title = one_line(
                &painter,
                self.title,
                theme.text_font(TextRole::Body),
                ink,
                room,
            );
            match self.detail {
                None => paint_line(&painter, left, middle, title),
                Some(detail) => {
                    let font = theme.text_font(TextRole::Caption);
                    let detail = one_line(&painter, detail, font, soft, room);
                    let between = theme.space(Space::S1);
                    let block = title.size().y + between + detail.size().y;
                    let top = middle - block / 2.0;
                    let second = top + title.size().y + between + detail.size().y / 2.0;
                    paint_line(&painter, left, top + title.size().y / 2.0, title);
                    paint_line(&painter, left, second, detail);
                }
            }
            focus_ring(ui, &response, radius, theme);
        }
        (RowOutput { response, action }, toggled)
    }
}

/// While a tree row holds focus, Right opens it and Left closes it.
/// Returns `true` when the key for the other state was pressed.
fn arrow_toggles(ui: &Ui, row: &Response, open: bool) -> bool {
    if !row.has_focus() {
        return false;
    }
    // The two arrows are the row's: egui does not also move focus
    // sideways.
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            row.id,
            EventFilter {
                horizontal_arrows: true,
                ..Default::default()
            },
        );
    });
    let key = if open {
        Key::ArrowLeft
    } else {
        Key::ArrowRight
    };
    ui.input_mut(|input| input.consume_key(Modifiers::NONE, key))
}

/// A chevron in `rect`: pointing right at `turn` 0, down at 1.
fn paint_chevron(painter: &Painter, rect: Rect, turn: f32, stroke: Stroke) {
    let reach = rect.width().min(rect.height()) * 0.22;
    let angle = turn.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    let at = |x: f32, y: f32| rect.center() + vec2(x * cos - y * sin, x * sin + y * cos) * reach;
    painter.line(vec![at(-0.5, -1.0), at(0.5, 0.0), at(-0.5, 1.0)], stroke);
}

/// What a tree row told its caller.
#[derive(Clone, Debug)]
pub struct TreeOutput {
    /// The row itself.
    pub response: Response,
    /// The action at its end that was clicked.
    pub action: Option<usize>,
    /// Its chevron was clicked, or the arrow key for the other state was
    /// pressed: the caller opens what is closed and closes what is open.
    pub toggled: bool,
}

/// A row of a tree: a [`ListRow`] set in by its depth, with a chevron
/// when something is under it.
#[derive(Clone, Copy, Debug)]
#[must_use = "a row draws nothing until it is shown"]
pub struct TreeRow<'a> {
    row: ListRow<'a>,
    depth: usize,
    expanded: Option<bool>,
}

impl<'a> TreeRow<'a> {
    /// A leaf at the root that says `title`; see [`ListRow::new`] for
    /// `salt`.
    pub fn new(salt: impl std::hash::Hash + std::fmt::Debug, title: &'a str) -> Self {
        Self {
            row: ListRow::new(salt, title),
            depth: 0,
            expanded: None,
        }
    }

    /// How many rows it hangs under: 0 at the root.
    pub const fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    /// It has rows under it, shown (`true`) or not. A row that was never
    /// told is a leaf and has no chevron.
    pub const fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    /// The row inside, to set its icon, note, actions and state.
    pub fn row(mut self, change: impl FnOnce(ListRow<'a>) -> ListRow<'a>) -> Self {
        self.row = change(self.row);
        self
    }

    /// An icon before the text.
    pub const fn icon(mut self, icon: Icon) -> Self {
        self.row = self.row.icon(icon);
        self
    }

    /// Whether it is the chosen one.
    pub const fn selected(mut self, selected: bool) -> Self {
        self.row = self.row.selected(selected);
        self
    }

    /// Whether it can be used.
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.row = self.row.enabled(enabled);
        self
    }

    /// Draws the row.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> TreeOutput {
        let twig = Twig {
            depth: self.depth,
            expanded: self.expanded,
        };
        let (output, toggled) = self.row.show_as(ui, theme, Some(twig));
        TreeOutput {
            response: output.response,
            action: output.action,
            toggled,
        }
    }
}

/// What a list, a folder or a search shows when it holds nothing.
#[derive(Clone, Copy, Debug)]
#[must_use = "an empty state draws nothing until it is shown"]
pub struct EmptyState<'a> {
    id: Id,
    icon: Option<Icon>,
    title: &'a str,
    detail: Option<&'a str>,
}

impl<'a> EmptyState<'a> {
    /// An empty state that says `title` (what is missing, in a few
    /// words). `salt` names it among what is drawn in the same `Ui`.
    pub fn new(salt: impl std::hash::Hash + std::fmt::Debug, title: &'a str) -> Self {
        Self {
            id: salted("sue-empty-state", salt),
            icon: None,
            title,
            detail: None,
        }
    }

    /// A large muted picture over the title.
    pub const fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A sentence under the title that says what to do about it.
    pub const fn detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Draws it in the middle of the room `ui` has left, with nothing to
    /// click.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        self.show_with(ui, theme, None::<fn(&mut Ui)>)
    }

    /// Draws it in the middle of the room `ui` has left, with what
    /// `actions` adds in a row under the text (the buttons that put
    /// something there: New, Open).
    pub fn show_actions(
        self,
        ui: &mut Ui,
        theme: &Theme,
        actions: impl FnOnce(&mut Ui),
    ) -> Response {
        self.show_with(ui, theme, Some(actions))
    }

    fn show_with(
        mut self,
        ui: &mut Ui,
        theme: &Theme,
        actions: Option<impl FnOnce(&mut Ui)>,
    ) -> Response {
        self.id = scoped(ui, self.id);
        let density = Density::of(ui);
        let room = ui.available_rect_before_wrap();
        // What it measured last frame: the block is centered with it.
        let known: Option<Vec2> = ui.data(|data| data.get_temp(self.id));
        let size = known.unwrap_or(Vec2::ZERO);
        let widest = (theme.space(Space::S9) * 9.0).min(room.width());
        let top = room.top() + ((room.height() - size.y) / 2.0).max(0.0);
        let column = Rect::from_min_max(
            pos2(room.center().x - widest / 2.0, top),
            pos2(room.center().x + widest / 2.0, room.bottom().max(top)),
        );
        let builder = UiBuilder::new()
            .id_salt(self.id)
            .max_rect(column)
            .layout(Layout::top_down(egui::Align::Center));
        let mut child = ui.new_child(builder);
        if known.is_none() {
            // Not measured yet: this frame only measures.
            child.set_invisible();
            ui.ctx().request_repaint();
        }
        let mut row_width = 0.0;
        {
            let ui = &mut child;
            ui.spacing_mut().item_spacing = vec2(theme.space(Space::S4), 0.0);
            if let Some(icon) = self.icon {
                let side = density.icon() * 2.5;
                let (_, at) = ui.allocate_space(Vec2::splat(side));
                icons::paint(ui.painter(), at, icon, theme.text_muted());
                ui.add_space(theme.space(Space::S5));
            }
            let title = RichText::new(self.title)
                .font(theme.text_font(TextRole::Title))
                .color(theme.text);
            let said = ui.add(Label::new(title).wrap());
            said.widget_info(|| WidgetInfo::labeled(Role::Heading, true, self.title));
            if let Some(detail) = self.detail {
                ui.add_space(theme.space(Space::S2));
                let detail = RichText::new(detail)
                    .font(theme.text_font(TextRole::Body))
                    .color(theme.text_muted());
                ui.add(Label::new(detail).wrap());
            }
            if let Some(actions) = actions {
                ui.add_space(theme.space(Space::S6));
                // A row is as wide as what it holds only once that is
                // known: it is centered with last frame's width.
                let width = size.x.min(column.width());
                let height = density.control_height();
                let left = column.center().x - width / 2.0;
                let at = Rect::from_min_size(pos2(left, ui.cursor().top()), vec2(width, height));
                let builder = UiBuilder::new()
                    .max_rect(at)
                    .layout(Layout::left_to_right(egui::Align::Center));
                let mut row = ui.new_child(builder);
                actions(&mut row);
                let used = row.min_rect();
                row_width = used.width();
                ui.allocate_space(vec2(used.width().min(column.width()), used.height()));
            }
        }
        let used = child.min_rect();
        let measured = vec2(row_width, used.height());
        if known != Some(measured) {
            ui.data_mut(|data| data.insert_temp(self.id, measured));
            ui.ctx().request_repaint();
        }
        ui.allocate_rect(room, Sense::hover())
    }
}

/// A hairline between two groups: across a column, or down a row.
pub fn divider(ui: &mut Ui, theme: &Theme) -> Response {
    let thick = theme.border_width.max(1.0);
    let size = if ui.layout().is_horizontal() {
        vec2(thick, ui.available_size_before_wrap().y)
    } else {
        vec2(ui.available_size_before_wrap().x, thick)
    };
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 0.0, theme.border);
    }
    response
}

/// The small muted line over a group of rows or controls.
pub fn heading(ui: &mut Ui, text: &str, theme: &Theme) -> Response {
    let line = RichText::new(text)
        .font(theme.text_font(TextRole::Caption))
        .color(theme.text_muted());
    let response = ui.add(Label::new(line).truncate());
    response.widget_info(|| WidgetInfo::labeled(Role::Heading, true, text));
    response
}

/// A [`ListRow`] that says `title`.
pub fn list_row(salt: impl std::hash::Hash + std::fmt::Debug, title: &str) -> ListRow<'_> {
    ListRow::new(salt, title)
}

/// A [`TreeRow`] that says `title`.
pub fn tree_row(salt: impl std::hash::Hash + std::fmt::Debug, title: &str) -> TreeRow<'_> {
    TreeRow::new(salt, title)
}

/// An [`EmptyState`] that says `title`.
pub fn empty_state(salt: impl std::hash::Hash + std::fmt::Debug, title: &str) -> EmptyState<'_> {
    EmptyState::new(salt, title)
}

#[cfg(test)]
mod tests {
    use egui::{Modifiers, Shape};

    use super::harness::{Run, at};
    use super::*;

    const ACTIONS: [RowAction<'static>; 2] = [
        RowAction::new(Icon::Star, "Favorite"),
        RowAction::new(Icon::Delete, "Delete"),
    ];

    fn row(ui: &mut Ui, theme: &Theme, enabled: bool) -> RowOutput {
        ListRow::new("row", "Illustration")
            .icon(Icon::Picture)
            .note("2 MB")
            .actions(&ACTIONS)
            .enabled(enabled)
            .show(ui, theme)
    }

    #[test]
    fn rows_are_as_tall_as_their_density_and_grow_for_a_second_line() {
        let theme = Run::new().theme;
        for density in Density::ALL {
            assert_eq!(ListRow::height(&theme, density, false), density.row());
            assert!(ListRow::height(&theme, density, true) > density.row());
        }
        let mut run = Run::new();
        let heights = run
            .frame(Vec::new(), |ui, theme| {
                Density::ALL.map(|density| {
                    density.scope(ui, |ui| {
                        ListRow::new(density, "Row")
                            .detail("Second line")
                            .show(ui, theme)
                            .response
                            .rect
                            .height()
                    })
                })
            })
            .0;
        for (height, density) in heights.into_iter().zip(Density::ALL) {
            assert_eq!(height, ListRow::height(&theme, density, true));
        }
    }

    #[test]
    fn a_click_on_an_action_is_the_action_and_not_the_row() {
        let mut run = Run::new();
        let rect = run
            .frame(Vec::new(), |ui, theme| row(ui, theme, true).response.rect)
            .0;
        let seen = run.click(at(rect, 0.2, 0.5), |ui, theme| row(ui, theme, true));
        assert!(seen.iter().any(|output| output.response.clicked()));
        assert!(seen.iter().all(|output| output.action.is_none()));
        // The last action sits at the end, a small gap in.
        let end = rect.right_center() - vec2(theme_gap(&run.theme) + 4.0, 0.0);
        let seen = run.click(end, |ui, theme| row(ui, theme, true));
        let actions: Vec<_> = seen.iter().filter_map(|output| output.action).collect();
        assert_eq!(actions, [1]);
        assert!(seen.iter().all(|output| !output.response.clicked()));
    }

    fn theme_gap(theme: &Theme) -> f32 {
        theme.space(Space::S2)
    }

    #[test]
    fn a_disabled_row_and_its_actions_take_no_click() {
        let mut run = Run::new();
        let rect = run
            .frame(Vec::new(), |ui, theme| row(ui, theme, false).response.rect)
            .0;
        for x in [0.2, 0.99] {
            let seen = run.click(at(rect, x, 0.5), |ui, theme| row(ui, theme, false));
            assert!(
                seen.iter()
                    .all(|output| !output.response.clicked() && output.action.is_none())
            );
        }
    }

    #[test]
    fn the_rows_fill_lies_under_its_actions() {
        let mut run = Run::new();
        let draw = |ui: &mut Ui, theme: &Theme| {
            ListRow::new("chosen", "Chosen")
                .actions(&ACTIONS)
                .selected(true)
                .show(ui, theme)
        };
        let _ = run.frame(Vec::new(), draw);
        let input = egui::RawInput::default();
        let output = run.ctx.run_ui(input, |ui| {
            let theme = Run::new().theme;
            draw(ui, &theme);
        });
        let fill = Run::new().theme.fill_selected();
        let shapes: Vec<&Shape> = output.shapes.iter().map(|clipped| &clipped.shape).collect();
        let first_fill = shapes
            .iter()
            .position(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == fill))
            .expect("the chosen row is filled");
        assert!(
            shapes[..first_fill]
                .iter()
                .all(|shape| matches!(shape, Shape::Noop)),
            "nothing of the row is drawn before its fill"
        );
    }

    #[test]
    fn a_tree_row_opens_by_its_chevron_and_by_the_arrows() {
        let mut run = Run::new();
        let draw = |ui: &mut Ui, theme: &Theme, open: bool| {
            TreeRow::new("folder", "Folder")
                .depth(1)
                .expanded(open)
                .icon(Icon::Folder)
                .show(ui, theme)
        };
        let output = run.frame(Vec::new(), |ui, theme| draw(ui, theme, false)).0;
        let rect = output.response.rect;
        let theme = run.theme.clone();
        let chevron = rect.left() + theme.space(Space::S4) + theme.space(Space::S6);
        let side = Density::Regular.icon();
        let seen = run.click(pos2(chevron + side / 2.0, rect.center().y), |ui, theme| {
            draw(ui, theme, false)
        });
        assert!(seen.iter().any(|output| output.toggled));
        assert!(seen.iter().all(|output| !output.response.clicked()));
        // A click on the name is the row's.
        let seen = run.click(at(rect, 0.6, 0.5), |ui, theme| draw(ui, theme, false));
        assert!(seen.iter().all(|output| !output.toggled));
        assert!(seen.iter().any(|output| output.response.clicked()));
        // Right opens a closed row, Left closes an open one, and the other
        // way round they say nothing.
        run.focus(output.response.id);
        let opened = run.key(egui::Key::ArrowRight, Modifiers::NONE, |ui, theme| {
            draw(ui, theme, false)
        });
        assert!(opened.toggled);
        let nothing = run.key(egui::Key::ArrowRight, Modifiers::NONE, |ui, theme| {
            draw(ui, theme, true)
        });
        assert!(!nothing.toggled);
        let closed = run.key(egui::Key::ArrowLeft, Modifiers::NONE, |ui, theme| {
            draw(ui, theme, true)
        });
        assert!(closed.toggled);
        assert_eq!(run.focused(), Some(output.response.id));
    }

    #[test]
    fn a_leaf_has_no_chevron_to_click() {
        let mut run = Run::new();
        let draw = |ui: &mut Ui, theme: &Theme| TreeRow::new("leaf", "Leaf").show(ui, theme);
        let rect = run.frame(Vec::new(), draw).0.response.rect;
        let theme = run.theme.clone();
        let x = rect.left() + theme.space(Space::S4) + Density::Regular.icon() / 2.0;
        let seen = run.click(pos2(x, rect.center().y), draw);
        assert!(seen.iter().all(|output| !output.toggled));
        assert!(seen.iter().any(|output| output.response.clicked()));
    }

    #[test]
    fn an_empty_state_measures_once_then_sits_in_the_middle_and_rests() {
        let mut run = Run::new();
        let draw = |ui: &mut Ui, theme: &Theme| {
            EmptyState::new("empty", "Nothing here")
                .icon(Icon::Folder)
                .detail("Make something.")
                .show_actions(ui, theme, |ui| {
                    let _ = ui.button("New");
                })
        };
        let (_, again) = run.frame(Vec::new(), draw);
        assert!(again, "the first frame only measures");
        let room = run.settle(|ui, theme| {
            let response = draw(ui, theme);
            response.rect
        });
        assert_eq!(room, Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)));
        let (_, again) = run.frame(Vec::new(), draw);
        assert!(!again, "measured, it asks for nothing");
    }

    #[test]
    fn what_a_drag_passes_over_does_not_light_up() {
        let mut run = Run::new();
        let left = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 30.0));
        let right = Rect::from_min_size(pos2(100.0, 0.0), vec2(100.0, 30.0));
        let draw = |ui: &mut Ui, _: &Theme| {
            let dragged = ui.interact(left, Id::unique("dragged"), Sense::click_and_drag());
            let under = ui.interact(right, Id::unique("under"), Sense::click());
            (dragged.dragged(), under_pointer(&under))
        };
        let seen = run.drag(left.center(), right.center(), 6, draw);
        let (dragging, lit) = *seen.last().expect("frames ran");
        assert!(dragging, "the drag goes on over the other");
        assert!(!lit, "the one under the drag stays unlit");
        // Without a drag the pointer lights it.
        let mut run = Run::new();
        let _ = run.frame(vec![egui::Event::PointerMoved(right.center())], draw);
        let (_, lit) = run.frame(Vec::new(), draw).0;
        assert!(lit);
    }

    #[test]
    fn a_divider_runs_across_a_column_and_down_a_row() {
        let mut run = Run::new();
        let (across, down) = run
            .frame(Vec::new(), |ui, theme| {
                let across = divider(ui, theme).rect;
                let down = ui
                    .horizontal(|ui| {
                        ui.set_height(40.0);
                        divider(ui, theme).rect
                    })
                    .inner;
                (across, down)
            })
            .0;
        assert_eq!(across.height(), 1.0);
        assert!(across.width() > 700.0);
        assert_eq!(down.width(), 1.0);
        assert!(down.height() >= 39.0);
    }
}

/// Frames of a headless context for the tests of the components: input
/// goes in, what the drawing returned comes out.
#[cfg(test)]
pub(super) mod harness {
    use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Ui, pos2, vec2};
    use sue_rice::RiceSchema;

    use crate::Theme;

    /// A context, its clock and the theme the components are drawn in.
    pub struct Run {
        pub ctx: Context,
        pub theme: Theme,
        time: f64,
    }

    impl Run {
        /// A context of 800 by 600 points with the shipped motion.
        pub fn new() -> Self {
            let theme = Theme::from_rice(&RiceSchema::default());
            let ctx = Context::default();
            theme.motion.store(&ctx);
            Self {
                ctx,
                theme,
                time: 0.0,
            }
        }

        /// One frame, a tenth of a second after the last, with `events`.
        /// Returns what `draw` returned, the last time it ran, and
        /// whether the frame asked for another at once.
        pub fn frame<R>(
            &mut self,
            events: Vec<Event>,
            mut draw: impl FnMut(&mut Ui, &Theme) -> R,
        ) -> (R, bool) {
            self.time += 0.1;
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                time: Some(self.time),
                predicted_dt: 0.1,
                events,
                ..Default::default()
            };
            let mut result = None;
            let theme = self.theme.clone();
            let mut output = self.ctx.run_ui(input, |ui| result = Some(draw(ui, &theme)));
            output.textures_delta.clear();
            let again = output
                .viewport_output
                .values()
                .any(|viewport| viewport.repaint_delay.is_zero());
            (result.expect("the frame ran"), again)
        }

        /// Frames with nothing happening, until nothing moves (at most
        /// twenty); returns what the last one drew.
        pub fn settle<R>(&mut self, mut draw: impl FnMut(&mut Ui, &Theme) -> R) -> R {
            let mut last = None;
            for _ in 0..20 {
                let (result, again) = self.frame(Vec::new(), &mut draw);
                last = Some(result);
                if !again {
                    break;
                }
            }
            last.expect("a frame ran")
        }

        /// The three frames of a click of `button` at `at`: the pointer
        /// comes, goes down, comes up. Returns what each drew.
        pub fn click_with<R>(
            &mut self,
            at: Pos2,
            button: PointerButton,
            mut draw: impl FnMut(&mut Ui, &Theme) -> R,
        ) -> Vec<R> {
            let press = |pressed| Event::PointerButton {
                pos: at,
                button,
                pressed,
                modifiers: Modifiers::NONE,
            };
            [
                vec![Event::PointerMoved(at)],
                vec![press(true)],
                vec![press(false)],
            ]
            .into_iter()
            .map(|events| self.frame(events, &mut draw).0)
            .collect()
        }

        /// A click of the primary button at `at`.
        pub fn click<R>(&mut self, at: Pos2, draw: impl FnMut(&mut Ui, &Theme) -> R) -> Vec<R> {
            self.click_with(at, PointerButton::Primary, draw)
        }

        /// A drag of the primary button from `from` to `to` in `steps`
        /// moves, without letting go. Returns what each frame drew.
        pub fn drag<R>(
            &mut self,
            from: Pos2,
            to: Pos2,
            steps: usize,
            mut draw: impl FnMut(&mut Ui, &Theme) -> R,
        ) -> Vec<R> {
            let mut seen = vec![self.frame(vec![Event::PointerMoved(from)], &mut draw).0];
            seen.push(
                self.frame(
                    vec![Event::PointerButton {
                        pos: from,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    }],
                    &mut draw,
                )
                .0,
            );
            for step in 1..=steps {
                let at = from + (to - from) * (step as f32 / steps as f32);
                seen.push(self.frame(vec![Event::PointerMoved(at)], &mut draw).0);
            }
            seen
        }

        /// Lets go of the primary button at `at`.
        pub fn release<R>(&mut self, at: Pos2, draw: impl FnMut(&mut Ui, &Theme) -> R) -> R {
            let event = Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![event], draw).0
        }

        /// A frame with `key` pressed (with `modifiers`).
        pub fn key<R>(
            &mut self,
            key: egui::Key,
            modifiers: Modifiers,
            draw: impl FnMut(&mut Ui, &Theme) -> R,
        ) -> R {
            let events = vec![
                Event::ModifiersChanged(modifiers),
                Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ];
            self.frame(events, draw).0
        }

        /// Gives keyboard focus to `id`.
        pub fn focus(&self, id: egui::Id) {
            self.ctx.memory_mut(|memory| memory.request_focus(id));
        }

        /// The widget that holds keyboard focus.
        pub fn focused(&self) -> Option<egui::Id> {
            self.ctx.memory(egui::Memory::focused)
        }
    }

    /// The point `x` and `y` of the way across `rect`.
    pub fn at(rect: Rect, x: f32, y: f32) -> Pos2 {
        pos2(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        )
    }
}
