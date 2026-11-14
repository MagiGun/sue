//! Cards: the boxes things are put in, and the boxes that are things.
//!
//! - [`Card`]: a rounded box on the surface above the panel it lies on,
//!   around whatever its caller draws in it. [`group`] is the tight one
//!   that holds rows, [`section`] a titled stretch of a page.
//! - [`Tile`]: a box to click, with a picture over a name: a studio in the
//!   Hub, a template, a preset.
//! - [`ThumbnailCard`]: a file or a page as its picture over its name,
//!   with a tick box for group actions and small buttons in its footer.
//!   While its picture loads it shows a skeleton.
//!
//! What can be clicked is named by its caller's salt, lights under the
//! pointer, shows keyboard focus and answers Enter and Space.

use egui::epaint::CornerRadiusF32;
use egui::{
    Color32, Frame, Id, InnerResponse, Margin, Painter, Pos2, Rect, Response, Role, Sense, Stroke,
    StrokeKind, TextureId, Ui, Vec2, WidgetInfo, pos2, vec2,
};

use super::focus::focus_ring;
use super::list::{
    DISABLED_PICTURE, ListRow, RowAction, actions_ui, blend, heading, one_line, paint_line, salted,
    scoped, state_fill, under_pointer,
};
use super::motion;
use super::progress::paint_skeleton;
use super::tokens::{Density, Elevation, Radius, Space, TextRole, nested_radius};
use crate::Theme;
use crate::icons::{self, Icon};

/// A rounded box around what its caller draws.
#[derive(Clone, Copy, Debug)]
#[must_use = "a card draws nothing until it is shown"]
pub struct Card {
    surface: u8,
    padding: Option<Space>,
    elevation: Elevation,
    outline: bool,
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Card {
    /// A card on the surface above panels, with the card padding and a
    /// hairline around it.
    pub const fn new() -> Self {
        Self {
            surface: 2,
            padding: None,
            elevation: Elevation::Flat,
            outline: true,
        }
    }

    /// The surface it is filled with: 0 the page, 1 panels, 2 cards (the
    /// default), 3 popovers.
    pub const fn surface(mut self, level: u8) -> Self {
        self.surface = level;
        self
    }

    /// The space between its edge and what it holds, in place of the
    /// card padding.
    pub const fn padding(mut self, padding: Space) -> Self {
        self.padding = Some(padding);
        self
    }

    /// The shadow under it: none at rest, more for what floats.
    pub const fn elevation(mut self, elevation: Elevation) -> Self {
        self.elevation = elevation;
        self
    }

    /// Without the hairline around it.
    pub const fn plain(mut self) -> Self {
        self.outline = false;
        self
    }

    /// The frame it draws: for a caller that lays its box out itself.
    pub fn frame(self, theme: &Theme) -> Frame {
        let padding = self
            .padding
            .map_or_else(|| theme.card_padding(), |step| theme.space(step));
        let stroke = if self.outline {
            theme.hairline()
        } else {
            Stroke::NONE
        };
        Frame::new()
            .fill(theme.surface(self.surface))
            .stroke(stroke)
            .corner_radius(theme.corner(Radius::Lg))
            .inner_margin(Margin::same(padding.round().clamp(0.0, 127.0) as i8))
            .shadow(theme.elevation(self.elevation))
    }

    /// Draws the card around what `add_contents` draws.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        theme: &Theme,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        self.frame(theme).show(ui, add_contents)
    }
}

/// A card that holds rows: they lie edge to edge, a small step inside its
/// border, so their rounded fills follow its corners.
pub fn group<R>(
    ui: &mut Ui,
    theme: &Theme,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    Card::new().padding(Space::S2).show(ui, theme, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        add_contents(ui)
    })
}

/// A stretch of a page under a small muted `title`.
pub fn section<R>(
    ui: &mut Ui,
    theme: &Theme,
    title: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    ui.vertical(|ui| {
        heading(ui, title, theme);
        ui.add_space(theme.space(Space::S2));
        add_contents(ui)
    })
}

/// The box of a tile or a thumbnail card for the state it is in: its
/// shadow and its fill. Returns the fill and how far the box is lifted,
/// for [`paint_outline`], which goes over whatever is drawn inside it.
fn paint_box(ui: &Ui, response: &Response, theme: &Theme, enabled: bool) -> (Color32, f32) {
    let rect = response.rect;
    let painter = ui.painter();
    let over = enabled && under_pointer(response);
    let lift = motion::hover_t(ui.ctx(), response.id.with("sue-lift"), over);
    if lift > 0.0 {
        let mut shadow = theme.elevation(Elevation::Raised);
        shadow.color = shadow.color.gamma_multiply(lift);
        painter.add(shadow.as_shape(rect, theme.corner(Radius::Lg)));
    }
    let fill = state_fill(ui, response, theme, theme.surface(2), false, enabled);
    painter.rect_filled(rect, theme.radius(Radius::Lg), fill);
    (fill, lift)
}

/// The outline of a tile or a thumbnail card, drawn over its fill and
/// its picture so that the chosen one's strong line is as thick all
/// round.
fn paint_outline(ui: &Ui, rect: Rect, theme: &Theme, (fill, lift): (Color32, f32), selected: bool) {
    let stroke = if selected {
        theme.selected_stroke()
    } else {
        Stroke::new(
            theme.border_width,
            blend(theme.border, theme.border_strong_on(fill), lift),
        )
    };
    ui.painter()
        .rect_stroke(rect, theme.radius(Radius::Lg), stroke, StrokeKind::Inside);
}

/// A box to click: a picture over a name.
#[derive(Clone, Copy, Debug)]
#[must_use = "a tile draws nothing until it is shown"]
pub struct Tile<'a> {
    id: Id,
    label: &'a str,
    detail: Option<&'a str>,
    icon: Option<Icon>,
    selected: bool,
    enabled: bool,
    size: Option<Vec2>,
}

impl<'a> Tile<'a> {
    /// A tile named `label`. `salt` names it among what is drawn in the
    /// same `Ui` and stays the same from frame to frame.
    pub fn new(salt: impl std::hash::Hash + std::fmt::Debug, label: &'a str) -> Self {
        Self {
            id: salted("sue-tile", salt),
            label,
            detail: None,
            icon: None,
            selected: false,
            enabled: true,
            size: None,
        }
    }

    /// The picture over the name.
    pub const fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A smaller muted line under the name.
    pub const fn detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Whether it is the chosen one.
    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Whether it can be used.
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Its size, in place of the density's.
    pub const fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// The size of a tile at `density`: four rows wide and three high.
    #[must_use]
    pub const fn default_size(density: Density) -> Vec2 {
        vec2(density.row() * 4.0, density.row() * 3.0)
    }

    /// Draws the tile.
    pub fn show(mut self, ui: &mut Ui, theme: &Theme) -> Response {
        self.id = scoped(ui, self.id);
        let density = Density::of(ui);
        let enabled = self.enabled && ui.is_enabled();
        let size = self.size.unwrap_or(Tile::default_size(density));
        let (_, rect) = ui.allocate_space(size);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(rect, self.id, sense);
        let (label, selected) = (self.label, self.selected);
        response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, label));
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let lit = paint_box(ui, &response, theme, enabled);
        paint_outline(ui, rect, theme, lit, self.selected);
        let fill = lit.0;
        let painter = ui.painter();
        let (ink, soft) = if enabled {
            (theme.text, theme.text_muted_on(fill))
        } else {
            let faint = theme.text_disabled(fill);
            (faint, faint)
        };
        let pad = theme.space(Space::S4);
        let room = rect.width() - pad * 2.0;
        let name = one_line(
            painter,
            self.label,
            theme.text_font(TextRole::Body),
            ink,
            room,
        );
        let detail = self.detail.map(|text| {
            one_line(
                painter,
                text,
                theme.text_font(TextRole::Caption),
                soft,
                room,
            )
        });
        let between = theme.space(Space::S2);
        let text_height = name.size().y
            + detail
                .as_ref()
                .map_or(0.0, |line| between / 2.0 + line.size().y);
        // The picture takes what the text leaves, up to twice an icon.
        let side = self.icon.map_or(0.0, |_| {
            (rect.height() - pad * 2.0 - between - text_height).clamp(0.0, density.icon() * 2.0)
        });
        let gap = if side > 0.0 { between } else { 0.0 };
        let mut y = rect.center().y - (side + gap + text_height) / 2.0;
        if let Some(icon) = self.icon.filter(|_| side > 0.0) {
            let at =
                Rect::from_center_size(pos2(rect.center().x, y + side / 2.0), Vec2::splat(side));
            icons::paint(painter, at, icon, ink);
            y += side + gap;
        }
        let centered = |width: f32| rect.center().x - width / 2.0;
        let line = name.size();
        paint_line(painter, centered(line.x), y + line.y / 2.0, name);
        y += line.y + between / 2.0;
        if let Some(detail) = detail {
            let line = detail.size();
            paint_line(painter, centered(line.x), y + line.y / 2.0, detail);
        }
        focus_ring(ui, &response, theme.radius(Radius::Lg), theme);
        response
    }
}

/// The picture of a thumbnail card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thumbnail {
    /// The picture and its size in pixels. It is fitted into the card
    /// whole, never cropped or stretched.
    Picture(TextureId, [u32; 2]),
    /// No picture will come: an icon stands for the thing.
    Placeholder(Icon),
    /// The picture is being made: a skeleton holds its place.
    Loading,
}

/// What a thumbnail card told its caller.
#[derive(Clone, Debug)]
pub struct ThumbnailOutput {
    /// The card itself: clicked (open it), hovered, focused, and what a
    /// context menu hangs on.
    pub response: Response,
    /// Its tick box was clicked: the caller ticks or unticks it.
    pub ticked: bool,
    /// The action in its footer that was clicked.
    pub action: Option<usize>,
}

/// A file, a page or a preset as its picture over its name.
#[derive(Clone, Copy, Debug)]
#[must_use = "a card draws nothing until it is shown"]
pub struct ThumbnailCard<'a> {
    id: Id,
    title: &'a str,
    detail: Option<&'a str>,
    thumbnail: Thumbnail,
    tick: Option<(bool, &'a str)>,
    actions: &'a [RowAction<'a>],
    selected: bool,
    enabled: bool,
    size: Option<Vec2>,
}

impl<'a> ThumbnailCard<'a> {
    /// A card named `title` whose picture is still loading. `salt` names
    /// it among what is drawn in the same `Ui` and stays the same from
    /// frame to frame (an entity's id, a path).
    pub fn new(salt: impl std::hash::Hash + std::fmt::Debug, title: &'a str) -> Self {
        Self {
            id: salted("sue-thumbnail-card", salt),
            title,
            detail: None,
            thumbnail: Thumbnail::Loading,
            tick: None,
            actions: &[],
            selected: false,
            enabled: true,
            size: None,
        }
    }

    /// A smaller muted line under the name (when it was saved, its size).
    pub const fn detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Its picture.
    pub const fn thumbnail(mut self, thumbnail: Thumbnail) -> Self {
        self.thumbnail = thumbnail;
        self
    }

    /// A tick box at its top corner, for group actions: ticked or not,
    /// and its name for a screen reader ("Select"). The box shows while
    /// the pointer is over the card, while it holds focus, and for as
    /// long as it is ticked.
    pub const fn tick(mut self, ticked: bool, label: &'a str) -> Self {
        self.tick = Some((ticked, label));
        self
    }

    /// Small buttons at the end of its footer.
    pub const fn actions(mut self, actions: &'a [RowAction<'a>]) -> Self {
        self.actions = actions;
        self
    }

    /// Whether it is the chosen one.
    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Whether it can be used.
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Its size, in place of the density's.
    pub const fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// The size of a card at `density`: six rows wide and six high.
    #[must_use]
    pub const fn default_size(density: Density) -> Vec2 {
        vec2(density.row() * 6.0, density.row() * 6.0)
    }

    /// Draws the card.
    pub fn show(mut self, ui: &mut Ui, theme: &Theme) -> ThumbnailOutput {
        self.id = scoped(ui, self.id);
        let density = Density::of(ui);
        let enabled = self.enabled && ui.is_enabled();
        let size = self.size.unwrap_or(Self::default_size(density));
        let (_, rect) = ui.allocate_space(size);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(rect, self.id, sense);
        let (title, selected) = (self.title, self.selected);
        response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, title));

        let pad = theme.space(Space::S4);
        let gap = theme.space(Space::S2);
        let footer = ListRow::height(theme, density, self.detail.is_some()).min(rect.height());
        let footer_rect = Rect::from_min_max(pos2(rect.left(), rect.bottom() - footer), rect.max);
        let picture = Rect::from_min_max(rect.min, pos2(rect.right(), footer_rect.top()));
        let visible = ui.is_rect_visible(rect);
        let mut lit = (theme.surface(2), 0.0);
        if visible {
            lit = paint_box(ui, &response, theme, enabled);
            self.paint_picture(ui, picture, theme, enabled);
            paint_outline(ui, rect, theme, lit, self.selected);
        }
        let fill = lit.0;
        // The tick box and the actions come after the card, so a click
        // on one of them is theirs.
        let ticked = self.tick_ui(ui, &response, theme, picture, enabled);
        let (action, right) = actions_ui(
            ui,
            theme,
            self.id,
            self.actions,
            (
                rect.right() - gap,
                footer_rect.center().y,
                density.min_hit().min(footer),
            ),
            (fill, false),
            enabled,
        );
        if visible {
            let painter = ui.painter();
            let (ink, soft) = if enabled {
                (theme.text, theme.text_muted_on(fill))
            } else {
                let faint = theme.text_disabled(fill);
                (faint, faint)
            };
            let left = rect.left() + pad;
            let room = right
                - if self.actions.is_empty() {
                    pad - gap
                } else {
                    gap
                }
                - left;
            let middle = footer_rect.center().y;
            let name = one_line(
                painter,
                self.title,
                theme.text_font(TextRole::Body),
                ink,
                room,
            );
            match self.detail {
                None => paint_line(painter, left, middle, name),
                Some(detail) => {
                    let font = theme.text_font(TextRole::Caption);
                    let detail = one_line(painter, detail, font, soft, room);
                    let between = theme.space(Space::S1);
                    let block = name.size().y + between + detail.size().y;
                    let top = middle - block / 2.0;
                    let second = top + name.size().y + between + detail.size().y / 2.0;
                    paint_line(painter, left, top + name.size().y / 2.0, name);
                    paint_line(painter, left, second, detail);
                }
            }
            focus_ring(ui, &response, theme.radius(Radius::Lg), theme);
        }
        ThumbnailOutput {
            response,
            ticked,
            action,
        }
    }

    /// The picture's ground and what stands on it.
    fn paint_picture(&self, ui: &Ui, picture: Rect, theme: &Theme, enabled: bool) {
        let edge = theme.border_width.max(1.0);
        let inner = Rect::from_min_max(
            picture.min + Vec2::splat(edge),
            pos2(picture.right() - edge, picture.bottom()),
        );
        let top = nested_radius(theme.radius(Radius::Lg), edge);
        let corners = CornerRadiusF32 {
            nw: top,
            ne: top,
            sw: 0.0,
            se: 0.0,
        };
        let painter = ui.painter();
        if matches!(self.thumbnail, Thumbnail::Loading) {
            // The skeleton is the ground.
            paint_skeleton(ui, inner, corners, theme);
            return;
        }
        painter.rect_filled(inner, corners, theme.surface(0));
        let room = inner.shrink(theme.space(Space::S2));
        match self.thumbnail {
            Thumbnail::Picture(texture, [width, height]) if width > 0 && height > 0 => {
                let (width, height) = (width as f32, height as f32);
                let scale = (room.width() / width).min(room.height() / height);
                let shown = Rect::from_center_size(room.center(), vec2(width, height) * scale);
                let whole = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                let tint = if enabled {
                    Color32::WHITE
                } else {
                    Color32::WHITE.gamma_multiply(DISABLED_PICTURE)
                };
                painter.image(texture, shown, whole, tint);
            }
            Thumbnail::Placeholder(icon) => {
                let side = (room.width().min(room.height()) * 0.4)
                    .clamp(0.0, Density::of(ui).icon() * 3.0);
                let at = Rect::from_center_size(room.center(), Vec2::splat(side));
                let ink = if enabled {
                    theme.text_muted()
                } else {
                    theme.text_disabled(theme.surface(0))
                };
                icons::paint(painter, at, icon, ink);
            }
            Thumbnail::Picture(..) | Thumbnail::Loading => {}
        }
    }

    /// The tick box: drawn and asked. Returns `true` when it was clicked.
    fn tick_ui(
        &self,
        ui: &Ui,
        card: &Response,
        theme: &Theme,
        picture: Rect,
        enabled: bool,
    ) -> bool {
        let Some((ticked, label)) = self.tick else {
            return false;
        };
        let density = Density::of(ui);
        let gap = theme.space(Space::S3);
        let side = density.icon();
        let reach = density.min_hit().max(side);
        let mark = Rect::from_min_size(picture.min + Vec2::splat(gap), Vec2::splat(side));
        let hit = Rect::from_center_size(mark.center(), Vec2::splat(reach)).intersect(card.rect);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(hit, self.id.with("sue-tick"), sense);
        response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, ticked, label));
        let wanted = ticked || response.has_focus() || (enabled && under_pointer(card));
        let shown = motion::hover_t(ui.ctx(), self.id.with("sue-tick-shown"), wanted);
        if shown > 0.0 && ui.is_rect_visible(mark) {
            let mut painter = ui.painter().clone();
            painter.multiply_opacity(shown);
            let ground = theme.surface(0);
            let radius = theme.radius(Radius::Xs);
            if ticked {
                painter.rect_filled(mark, radius, theme.text);
                paint_check(&painter, mark, Stroke::new(theme.icon_stroke(side), ground));
            } else {
                let lit = enabled && response.hovered();
                let fill = if lit { theme.fill_hover() } else { ground };
                let edge = Stroke::new(theme.border_width.max(1.0), theme.border_strong_on(ground));
                painter.rect(mark, radius, fill, edge, StrokeKind::Inside);
            }
        }
        focus_ring(
            ui,
            &response.clone().with_new_rect(mark),
            theme.radius(Radius::Xs),
            theme,
        );
        let clicked = response.clicked();
        if !label.is_empty() {
            response.on_hover_text(label);
        }
        clicked
    }
}

/// A check mark in `rect`.
fn paint_check(painter: &Painter, rect: Rect, stroke: Stroke) {
    let at = |x: f32, y: f32| rect.min + vec2(x * rect.width(), y * rect.height());
    painter.line(vec![at(0.24, 0.54), at(0.43, 0.72), at(0.77, 0.3)], stroke);
}

/// Draws a [`Card`] around what `add_contents` draws.
pub fn card<R>(
    ui: &mut Ui,
    theme: &Theme,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    Card::new().show(ui, theme, add_contents)
}

/// A [`Tile`] named `label`.
pub fn tile(salt: impl std::hash::Hash + std::fmt::Debug, label: &str) -> Tile<'_> {
    Tile::new(salt, label)
}

/// A [`ThumbnailCard`] named `title`.
pub fn thumbnail_card(
    salt: impl std::hash::Hash + std::fmt::Debug,
    title: &str,
) -> ThumbnailCard<'_> {
    ThumbnailCard::new(salt, title)
}

#[cfg(test)]
mod tests {
    use super::super::list::harness::{Run, at};
    use super::*;

    const ACTIONS: [RowAction<'static>; 1] = [RowAction::new(Icon::Star, "Favorite")];

    fn card_ui(ui: &mut Ui, theme: &Theme, ticked: bool, enabled: bool) -> ThumbnailOutput {
        ThumbnailCard::new("card", "Sketch")
            .detail("Today")
            .thumbnail(Thumbnail::Placeholder(Icon::Picture))
            .tick(ticked, "Select")
            .actions(&ACTIONS)
            .enabled(enabled)
            .show(ui, theme)
    }

    #[test]
    fn a_thumbnail_card_tells_its_tick_its_actions_and_itself_apart() {
        let mut run = Run::new();
        let draw = |ui: &mut Ui, theme: &Theme| card_ui(ui, theme, false, true);
        let rect = run.frame(Vec::new(), draw).0.response.rect;
        // The picture: the card.
        let seen = run.click(at(rect, 0.5, 0.4), draw);
        assert!(seen.iter().any(|output| output.response.clicked()));
        assert!(
            seen.iter()
                .all(|output| !output.ticked && output.action.is_none())
        );
        // The tick box in the top corner.
        let theme = run.theme.clone();
        let corner = rect.min + Vec2::splat(theme.space(Space::S3) + Density::Regular.icon() / 2.0);
        let seen = run.click(corner, draw);
        assert!(seen.iter().any(|output| output.ticked));
        assert!(seen.iter().all(|output| !output.response.clicked()));
        // The action at the end of the footer.
        let footer = rect.right_bottom() - vec2(theme.space(Space::S2) + 6.0, 12.0);
        let seen = run.click(footer, draw);
        assert_eq!(
            seen.iter()
                .filter_map(|output| output.action)
                .collect::<Vec<_>>(),
            [0]
        );
        assert!(seen.iter().all(|output| !output.response.clicked()));
    }

    #[test]
    fn a_disabled_card_and_tile_take_no_click() {
        let mut run = Run::new();
        let draw = |ui: &mut Ui, theme: &Theme| card_ui(ui, theme, true, false);
        let rect = run.frame(Vec::new(), draw).0.response.rect;
        for point in [at(rect, 0.5, 0.4), at(rect, 0.1, 0.1), at(rect, 0.95, 0.95)] {
            let seen = run.click(point, draw);
            assert!(seen.iter().all(|output| {
                !output.response.clicked() && !output.ticked && output.action.is_none()
            }));
        }
        let tile =
            |ui: &mut Ui, theme: &Theme| Tile::new("t", "Tile").enabled(false).show(ui, theme);
        let rect = run.frame(Vec::new(), tile).0.rect;
        let seen = run.click(rect.center(), tile);
        assert!(seen.iter().all(|response| !response.clicked()));
    }

    #[test]
    fn a_tile_is_clicked_and_is_as_large_as_its_density_asks() {
        let mut run = Run::new();
        let draw = |ui: &mut Ui, theme: &Theme| {
            Tile::new("t", "Sue Paint")
                .icon(Icon::ToolBrush)
                .detail("Draw and paint")
                .show(ui, theme)
        };
        let rect = run.frame(Vec::new(), draw).0.rect;
        assert_eq!(rect.size(), Tile::default_size(Density::Regular));
        let seen = run.click(rect.center(), draw);
        assert!(seen.iter().any(Response::clicked));
        let sizes = run
            .frame(Vec::new(), |ui, theme| {
                Density::ALL.map(|density| {
                    density.scope(ui, |ui| {
                        ThumbnailCard::new(density, "Card")
                            .show(ui, theme)
                            .response
                            .rect
                            .size()
                    })
                })
            })
            .0;
        assert_eq!(sizes, Density::ALL.map(ThumbnailCard::default_size));
    }

    #[test]
    fn a_loading_card_breathes_and_a_still_one_rests() {
        let mut run = Run::new();
        let loading =
            |ui: &mut Ui, theme: &Theme| ThumbnailCard::new("l", "Loading").show(ui, theme);
        let _ = run.frame(Vec::new(), loading);
        let (_, again) = run.frame(Vec::new(), loading);
        assert!(again, "the skeleton moves");
        let mut run = Run::new();
        let still = |ui: &mut Ui, theme: &Theme| {
            ThumbnailCard::new("s", "Still")
                .thumbnail(Thumbnail::Placeholder(Icon::Picture))
                .show(ui, theme)
        };
        let _ = run.settle(still);
        let (_, again) = run.frame(Vec::new(), still);
        assert!(!again, "a card at rest asks for nothing");
    }

    #[test]
    fn a_card_frame_reads_the_tokens() {
        let theme = Run::new().theme;
        let frame = Card::new().frame(&theme);
        assert_eq!(frame.fill, theme.surface(2));
        assert_eq!(frame.corner_radius, theme.corner(Radius::Lg));
        assert_eq!(frame.stroke, theme.hairline());
        let plain = Card::new()
            .plain()
            .surface(3)
            .padding(Space::S6)
            .frame(&theme);
        assert_eq!(plain.stroke, Stroke::NONE);
        assert_eq!(plain.fill, theme.surface(3));
        assert_eq!(f32::from(plain.inner_margin.left), theme.space(Space::S6));
    }

    #[test]
    fn the_chosen_cards_outline_lies_over_its_picture() {
        let mut run = Run::new();
        let theme = run.theme.clone();
        let draw = |ui: &mut Ui, theme: &Theme| {
            ThumbnailCard::new("chosen", "Chosen")
                .thumbnail(Thumbnail::Placeholder(Icon::Picture))
                .selected(true)
                .show(ui, theme)
        };
        let _ = run.settle(draw);
        let output = run.ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = draw(ui, &theme);
        });
        let shapes: Vec<&egui::Shape> =
            output.shapes.iter().map(|clipped| &clipped.shape).collect();
        let place = |wanted: &dyn Fn(&egui::epaint::RectShape) -> bool| {
            shapes
                .iter()
                .position(|shape| matches!(shape, egui::Shape::Rect(rect) if wanted(rect)))
        };
        let picture = place(&|rect| rect.fill == theme.surface(0)).expect("the picture's ground");
        let outline = place(&|rect| rect.stroke == theme.selected_stroke()).expect("the outline");
        assert!(
            outline > picture,
            "the picture would hide half of the outline's strong line"
        );
    }
}
