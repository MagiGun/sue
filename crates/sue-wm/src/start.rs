//! Start pages: the page a studio opens on before any work is open, where
//! people start something new, find their projects and browse their
//! materials. A narrow rail of large buttons on the left, a row of icon
//! buttons at the top right, and in the main area a category list, a
//! search box, filter chips, a toolbar and a grid or list of cards.
//! Popups come up in the middle over a dimmed page, with a title, an X and
//! grouped rows.
//!
//! These are plain widgets drawn from the [`Theme`], with its rounded
//! corners and Sue's icons ([`crate::icons`]), so every studio can have a start page that looks like the
//! others. Each studio keeps its own state and decides what the pieces
//! do.

use egui::{
    Align2, Color32, CornerRadius, Id, Order, Painter, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, TextureId, Ui, UiBuilder, Vec2, pos2, vec2,
};

use crate::Theme;
use crate::icons::{self, Icon};
use crate::ui::{Density, Radius, Space};

/// Draws `icon` in the largest square in the middle of `rect`, in
/// `color`.
pub fn square_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    icons::paint(
        painter,
        Rect::from_center_size(rect.center(), Vec2::splat(side)),
        icon,
        color,
    );
}

/// A square button with an icon, lit when `on`.
pub fn icon_button(
    ui: &mut Ui,
    rect: Rect,
    picture: Icon,
    hint: &str,
    on: bool,
    theme: &Theme,
) -> Response {
    icon_button_enabled(ui, rect, picture, hint, on, true, theme)
}

/// A square button with an icon, grayed and deaf when not `enabled`.
pub fn icon_button_enabled(
    ui: &mut Ui,
    rect: Rect,
    picture: Icon,
    hint: &str,
    on: bool,
    enabled: bool,
    theme: &Theme,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(
        rect,
        ui.scope_id()
            .with(("sue-start-icon", hint, rect.min.x as i32, rect.min.y as i32)),
        sense,
    );
    let painter = ui.painter();
    let hot = enabled && response.hovered();
    if on || hot {
        painter.rect_filled(
            rect,
            theme.radius(Radius::Md),
            if on { theme.active } else { theme.hover },
        );
    }
    if on {
        painter.rect_stroke(
            rect,
            theme.radius(Radius::Md),
            theme.hairline_active(),
            StrokeKind::Inside,
        );
    }
    let ink = if !enabled {
        theme.muted.gamma_multiply(0.5)
    } else if on || hot {
        theme.text
    } else {
        theme.muted
    };
    let side = (rect.width().min(rect.height()) * 0.6).max(8.0);
    square_icon(
        painter,
        Rect::from_center_size(rect.center(), Vec2::splat(side)),
        picture,
        ink,
    );
    if hint.is_empty() {
        response
    } else {
        response.on_hover_text(hint)
    }
}

/// A search box `width` wide with a magnifying glass. Returns the text
/// edit's response.
pub fn search_box(
    ui: &mut Ui,
    text: &mut String,
    hint: &str,
    width: f32,
    theme: &Theme,
) -> Response {
    // Finger sized on touch.
    let height = Density::of(ui).min_hit();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        theme.radius(Radius::Full),
        theme.surface(2),
        theme.hairline(),
        StrokeKind::Inside,
    );
    square_icon(
        painter,
        Rect::from_center_size(pos2(rect.left() + 15.0, rect.center().y), Vec2::splat(14.0)),
        Icon::Search,
        theme.muted,
    );
    let field = Rect::from_min_max(
        pos2(rect.left() + 30.0, rect.top() + 2.0),
        pos2(rect.right() - 28.0, rect.bottom() - 2.0),
    );
    let response = ui.put(
        field,
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .frame(egui::Frame::NONE)
            .text_color(theme.text)
            .font(theme.font(theme.ui_size))
            .align(Align2::LEFT_CENTER),
    );
    if !text.is_empty() {
        let clear = Rect::from_center_size(
            pos2(rect.right() - 14.0, rect.center().y),
            Vec2::splat(20.0),
        );
        if icon_button(ui, clear, Icon::Close, "", false, theme).clicked() {
            text.clear();
        }
    }
    response
}

/// A toolbar button: an icon and a label, grayed and deaf when not
/// `enabled`, lit while `armed` (a first click that asks again).
pub fn action(
    ui: &mut Ui,
    picture: Icon,
    label: &str,
    enabled: bool,
    armed: bool,
    theme: &Theme,
) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme.font(theme.ui_size), theme.text);
    let size = vec2(galley.size().x + 38.0, 26.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let painter = ui.painter();
    let hot = enabled && response.hovered();
    if armed || hot {
        painter.rect_filled(
            rect,
            theme.radius(Radius::Md),
            if armed { theme.active } else { theme.hover },
        );
    }
    if armed {
        painter.rect_stroke(
            rect,
            theme.radius(Radius::Md),
            theme.hairline_active(),
            StrokeKind::Inside,
        );
    }
    let ink = if enabled {
        theme.text
    } else {
        theme.muted.gamma_multiply(0.5)
    };
    square_icon(
        painter,
        Rect::from_center_size(pos2(rect.left() + 15.0, rect.center().y), Vec2::splat(15.0)),
        picture,
        ink,
    );
    painter.galley(
        pos2(rect.left() + 30.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    response
}

/// A thin bar showing `fraction` (0 to 1) done.
pub fn progress(ui: &mut Ui, width: f32, fraction: f32, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, theme.radius(Radius::Full), theme.void);
    let done = Rect::from_min_size(
        rect.min,
        vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
    );
    painter.rect_filled(done, theme.radius(Radius::Full), theme.text);
    painter.rect_stroke(
        rect,
        theme.radius(Radius::Full),
        theme.hairline(),
        StrokeKind::Inside,
    );
}

/// How tall a [`chip`] is: finger sized on touch.
#[must_use]
pub fn chip_height(ui: &Ui, theme: &Theme) -> f32 {
    let density = Density::of(ui);
    if density == Density::Touch {
        density.min_hit()
    } else {
        density.min_hit() - theme.space(Space::S2)
    }
}

/// A chip: a small button that stays lit while `on`.
pub fn chip(ui: &mut Ui, label: &str, on: bool, theme: &Theme) -> Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        theme.font(theme.small_size + 1.0),
        theme.text,
    );
    let height = chip_height(ui, theme);
    let size = vec2(galley.size().x + height, height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let fill = if on {
        theme.active
    } else if response.hovered() {
        theme.hover
    } else {
        theme.pill
    };
    painter.rect_filled(rect, theme.radius(Radius::Full), fill);
    painter.rect_stroke(
        rect,
        theme.radius(Radius::Full),
        if on {
            theme.hairline_active()
        } else {
            theme.hairline()
        },
        StrokeKind::Inside,
    );
    painter.galley(
        rect.center() - galley.size() / 2.0,
        galley,
        if on { theme.text } else { theme.muted },
    );
    response
}

/// How much of a set is chosen, for a Select All box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chosen {
    /// None.
    None,
    /// Some.
    Some,
    /// All.
    All,
}

/// A check box with a label. Returns its response.
pub fn check(ui: &mut Ui, label: &str, chosen: Chosen, theme: &Theme) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme.font(theme.ui_size), theme.text);
    let size = vec2(galley.size().x + 32.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let square =
        Rect::from_center_size(pos2(rect.left() + 10.0, rect.center().y), Vec2::splat(18.0));
    painter.rect(
        square,
        theme.radius(Radius::Xs),
        if response.hovered() {
            theme.hover
        } else {
            theme.void
        },
        if chosen == Chosen::None {
            theme.hairline()
        } else {
            theme.hairline_active()
        },
        StrokeKind::Inside,
    );
    match chosen {
        Chosen::All => square_icon(painter, square.shrink(3.0), Icon::Check, theme.text),
        Chosen::Some => {
            painter.line_segment(
                [
                    pos2(square.left() + 5.0, square.center().y),
                    pos2(square.right() - 5.0, square.center().y),
                ],
                Stroke::new(2.0, theme.text),
            );
        }
        Chosen::None => {}
    }
    painter.galley(
        pos2(
            square.right() + 8.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        theme.text,
    );
    response
}

/// A dropdown showing `options[*current]`, `width` wide. Returns `true`
/// when the choice changed.
pub fn dropdown(
    ui: &mut Ui,
    id: Id,
    current: &mut usize,
    options: &[String],
    width: f32,
    theme: &Theme,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let painter = ui.painter();
    painter.rect(
        rect,
        theme.radius(Radius::Md),
        if response.hovered() {
            theme.hover
        } else {
            theme.pill
        },
        theme.hairline(),
        StrokeKind::Inside,
    );
    let text = options.get(*current).cloned().unwrap_or_default();
    painter.text(
        pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        theme.font(theme.ui_size),
        theme.text,
    );
    square_icon(
        painter,
        Rect::from_center_size(
            pos2(rect.right() - 12.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        Icon::ChevronDown,
        theme.muted,
    );
    let mut changed = false;
    let popup = egui::Popup::menu(&response).id(id).width(width);
    popup.show(|ui| {
        for (index, option) in options.iter().enumerate() {
            if ui.selectable_label(index == *current, option).clicked() {
                changed |= index != *current;
                *current = index;
            }
        }
    });
    changed
}

/// A monochrome on and off switch. Returns `true` when clicked.
pub fn toggle(ui: &mut Ui, rect: Rect, on: bool, id: Id, theme: &Theme) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    let t = ui.ctx().animate_bool_responsive(response.id, on);
    let radius = rect.height() / 2.0;
    let painter = ui.painter();
    painter.rect_filled(rect, radius, if on { theme.text } else { theme.hover });
    painter.rect_stroke(rect, radius, theme.hairline(), StrokeKind::Inside);
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    painter.circle_filled(
        pos2(x, rect.center().y),
        radius * 0.72,
        if on { theme.void } else { theme.muted },
    );
    response.clicked()
}

/// A row of a category list or tree.
#[derive(Clone, Debug)]
pub struct Category {
    /// Its picture, if any.
    pub glyph: Option<Icon>,
    /// Its name.
    pub label: String,
    /// How deep in the tree, 0 at the top.
    pub depth: u8,
    /// `Some(open)` for a branch that opens and closes.
    pub open: Option<bool>,
}

/// What a click on a category row asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CategoryClick {
    /// Show it.
    Choose,
    /// Open or close the branch.
    Fold,
}

/// One category row `width` wide. Returns what was clicked, with the
/// row's response for a context menu.
pub fn category(
    ui: &mut Ui,
    width: f32,
    row: &Category,
    selected: bool,
    theme: &Theme,
) -> (Option<CategoryClick>, Response) {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let painter = ui.painter();
    let fill = if selected {
        theme.active
    } else if response.hovered() {
        theme.hover
    } else {
        theme.pill
    };
    painter.rect_filled(rect, theme.radius(Radius::Md), fill);
    if selected {
        painter.rect_stroke(
            rect,
            theme.radius(Radius::Md),
            theme.hairline_active(),
            StrokeKind::Inside,
        );
    }
    let mut x = rect.left() + 6.0 + f32::from(row.depth) * 14.0;
    let mut click = None;
    if let Some(open) = row.open {
        let arrow = Rect::from_min_size(pos2(x, rect.top() + 5.0), Vec2::splat(16.0));
        let fold = ui.interact(arrow, response.id.with("fold"), Sense::click());
        square_icon(
            painter,
            arrow.shrink(3.0),
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            theme.muted,
        );
        if fold.clicked() {
            click = Some(CategoryClick::Fold);
        }
        x += 18.0;
    }
    if let Some(picture) = row.glyph {
        square_icon(
            painter,
            Rect::from_min_size(pos2(x, rect.top() + 6.0), Vec2::splat(14.0)),
            picture,
            theme.text,
        );
        x += 20.0;
    }
    painter.with_clip_rect(rect.shrink(1.0)).text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        &row.label,
        theme.font(theme.ui_size),
        if selected {
            theme.text
        } else {
            theme.muted.lerp_to_gamma(theme.text, 0.5)
        },
    );
    if row.open.is_none() && row.depth == 0 && row.glyph.is_none() {
        square_icon(
            painter,
            Rect::from_center_size(
                pos2(rect.right() - 12.0, rect.center().y),
                Vec2::splat(12.0),
            ),
            Icon::ChevronRight,
            theme.muted,
        );
    }
    if click.is_none() && response.clicked() {
        click = Some(CategoryClick::Choose);
    }
    (click, response)
}

/// A card or a list row to draw.
#[derive(Clone, Debug)]
pub struct Card {
    /// Its name.
    pub title: String,
    /// A second line, e.g. when it was saved.
    pub detail: String,
    /// The picture and its size in pixels, once there is one.
    pub picture: Option<(TextureId, [u32; 2])>,
    /// What to draw while there is no picture.
    pub placeholder: Icon,
    /// `Some(on)` to show a heart.
    pub favorite: Option<bool>,
    /// Ticked for a group action.
    pub chosen: bool,
    /// What a hover says.
    pub hover: String,
}

/// What a click on a card asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardClick {
    /// Open it (a click on the picture or the name).
    Open,
    /// Tick it, or take its tick away.
    Choose,
    /// Make it a favorite, or not.
    Favorite,
    /// Show what it is (the `>` button).
    Details,
}

/// A card `size` big: its picture on top, its name under it, a tick box
/// at the top left, and a heart and a `>` at the bottom right. Returns the
/// click and the card's response for a context menu.
pub fn card(ui: &mut Ui, size: Vec2, card: &Card, theme: &Theme) -> (Option<CardClick>, Response) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let compact = size.x < 120.0;
    let footer = if compact { 22.0 } else { 44.0 };
    let picture_rect = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - footer));
    let hot = response.hovered();
    let painter = ui.painter().clone();
    painter.rect_filled(
        rect,
        theme.radius(Radius::Lg),
        if hot { theme.hover } else { theme.pill },
    );
    picture(&painter, picture_rect, card, theme);
    painter.rect_stroke(
        rect,
        theme.radius(Radius::Lg),
        if card.chosen {
            Stroke::new(2.0, theme.border_active)
        } else if hot {
            theme.hairline_active()
        } else {
            theme.hairline()
        },
        StrokeKind::Inside,
    );
    let name_rect = Rect::from_min_max(
        pos2(rect.left() + 6.0, picture_rect.bottom()),
        pos2(rect.right() - 6.0, rect.bottom()),
    );
    painter.with_clip_rect(name_rect).text(
        pos2(
            name_rect.left(),
            picture_rect.bottom() + if compact { 11.0 } else { 12.0 },
        ),
        Align2::LEFT_CENTER,
        &card.title,
        theme.font(if compact {
            theme.small_size
        } else {
            theme.ui_size
        }),
        theme.text,
    );
    let mut click = None;
    // The tick box, at the top left; it shows on hover or once ticked.
    let tick = Rect::from_min_size(rect.min + vec2(5.0, 5.0), Vec2::splat(18.0));
    let tick_response = ui.interact(tick, response.id.with("tick"), Sense::click());
    if card.chosen || hot || tick_response.hovered() {
        painter.rect(
            tick,
            theme.radius(Radius::Xs),
            theme.void,
            theme.hairline_active(),
            StrokeKind::Inside,
        );
        if card.chosen {
            square_icon(&painter, tick.shrink(3.0), Icon::Check, theme.text);
        }
    }
    if tick_response.clicked() {
        click = Some(CardClick::Choose);
    }
    if !compact {
        let mut right = rect.right() - 4.0;
        let row_y = rect.bottom() - 13.0;
        let details = Rect::from_center_size(pos2(right - 10.0, row_y), Vec2::splat(20.0));
        if icon_button(ui, details, Icon::Details, "", false, theme).clicked() {
            click = Some(CardClick::Details);
        }
        right -= 24.0;
        if let Some(on) = card.favorite {
            let heart = Rect::from_center_size(pos2(right - 10.0, row_y), Vec2::splat(20.0));
            let heart_response = ui.interact(heart, response.id.with("heart"), Sense::click());
            let ink = if on {
                theme.text
            } else if heart_response.hovered() {
                theme.muted
            } else {
                theme.muted.gamma_multiply(0.55)
            };
            square_icon(&painter, heart.shrink(3.0), Icon::Heart, ink);
            if heart_response.clicked() {
                click = Some(CardClick::Favorite);
            }
        }
        // The detail stops short of the heart and the `>`, never under them.
        let detail_rect = Rect::from_min_max(
            name_rect.min,
            pos2(
                right - if card.favorite.is_some() { 24.0 } else { 0.0 },
                name_rect.bottom(),
            ),
        );
        painter.with_clip_rect(detail_rect).text(
            pos2(name_rect.left(), row_y),
            Align2::LEFT_CENTER,
            &card.detail,
            theme.font(theme.small_size),
            theme.muted,
        );
    } else if card.favorite == Some(true) {
        let heart = Rect::from_center_size(
            pos2(rect.right() - 11.0, rect.bottom() - 11.0),
            Vec2::splat(14.0),
        );
        square_icon(&painter, heart, Icon::Heart, theme.text);
    }
    if click.is_none() && response.clicked() {
        click = Some(
            if response
                .ctx
                .input(|input| input.modifiers.command || input.modifiers.shift)
            {
                CardClick::Choose
            } else {
                CardClick::Open
            },
        );
    }
    let response = if card.hover.is_empty() {
        response
    } else {
        response.on_hover_text(&card.hover)
    };
    (click, response)
}

/// A card's picture fitted inside `rect` on a light ground, or its
/// placeholder icon.
fn picture(painter: &Painter, rect: Rect, card: &Card, theme: &Theme) {
    let inner = rect.shrink(theme.border_width.max(1.0));
    // Round at the top, as the card around it is; square where the
    // footer meets it.
    let top = theme.corner(Radius::Lg).nw.saturating_sub(1);
    let ground = CornerRadius {
        nw: top,
        ne: top,
        sw: 0,
        se: 0,
    };
    match card.picture {
        Some((texture, [width, height])) if width > 0 && height > 0 => {
            painter.rect_filled(inner, ground, theme.void);
            let scale = (inner.width() / width as f32).min(inner.height() / height as f32);
            let shown = vec2(width as f32 * scale, height as f32 * scale);
            painter.image(
                texture,
                Rect::from_center_size(inner.center(), shown),
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        _ => {
            painter.rect_filled(inner, ground, theme.void);
            let side = (inner.width().min(inner.height()) * 0.4).max(10.0);
            square_icon(
                painter,
                Rect::from_center_size(inner.center(), Vec2::splat(side)),
                card.placeholder,
                theme.muted,
            );
        }
    }
}

/// A card as a row `width` wide: a small picture, the name and detail,
/// the heart and the `>` at the right.
pub fn list_row(
    ui: &mut Ui,
    width: f32,
    card: &Card,
    theme: &Theme,
) -> (Option<CardClick>, Response) {
    let height = 44.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let hot = response.hovered();
    let painter = ui.painter().clone();
    painter.rect(
        rect,
        theme.radius(Radius::Md),
        if hot { theme.hover } else { theme.pill },
        if card.chosen {
            Stroke::new(2.0, theme.border_active)
        } else {
            theme.hairline()
        },
        StrokeKind::Inside,
    );
    let mut click = None;
    let tick = Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), Vec2::splat(18.0));
    let tick_response = ui.interact(tick, response.id.with("tick"), Sense::click());
    painter.rect(
        tick,
        theme.radius(Radius::Xs),
        theme.void,
        if card.chosen {
            theme.hairline_active()
        } else {
            theme.hairline()
        },
        StrokeKind::Inside,
    );
    if card.chosen {
        square_icon(&painter, tick.shrink(3.0), Icon::Check, theme.text);
    }
    if tick_response.clicked() {
        click = Some(CardClick::Choose);
    }
    let thumb = Rect::from_min_size(pos2(rect.left() + 32.0, rect.top() + 4.0), vec2(48.0, 36.0));
    picture(&painter, thumb, card, theme);
    let text_left = thumb.right() + 10.0;
    let text_rect = Rect::from_min_max(
        pos2(text_left, rect.top()),
        pos2(rect.right() - 60.0, rect.bottom()),
    );
    painter.with_clip_rect(text_rect).text(
        pos2(text_left, rect.top() + 14.0),
        Align2::LEFT_CENTER,
        &card.title,
        theme.font(theme.ui_size),
        theme.text,
    );
    painter.with_clip_rect(text_rect).text(
        pos2(text_left, rect.top() + 31.0),
        Align2::LEFT_CENTER,
        &card.detail,
        theme.font(theme.small_size),
        theme.muted,
    );
    let details = Rect::from_center_size(
        pos2(rect.right() - 16.0, rect.center().y),
        Vec2::splat(22.0),
    );
    if icon_button(ui, details, Icon::Details, "", false, theme).clicked() {
        click = Some(CardClick::Details);
    }
    if let Some(on) = card.favorite {
        let heart = Rect::from_center_size(
            pos2(rect.right() - 42.0, rect.center().y),
            Vec2::splat(20.0),
        );
        let heart_response = ui.interact(heart, response.id.with("heart"), Sense::click());
        square_icon(
            &painter,
            heart.shrink(3.0),
            Icon::Heart,
            if on {
                theme.text
            } else {
                theme.muted.gamma_multiply(0.55)
            },
        );
        if heart_response.clicked() {
            click = Some(CardClick::Favorite);
        }
    }
    if click.is_none() && response.clicked() {
        click = Some(
            if response
                .ctx
                .input(|input| input.modifiers.command || input.modifiers.shift)
            {
                CardClick::Choose
            } else {
                CardClick::Open
            },
        );
    }
    let response = if card.hover.is_empty() {
        response
    } else {
        response.on_hover_text(&card.hover)
    };
    (click, response)
}

/// What sits at the right end of a popup row.
#[derive(Clone, Debug, PartialEq)]
pub enum Trailing {
    /// Nothing.
    None,
    /// A chevron: it opens more.
    More,
    /// A box with an arrow: it opens elsewhere (a web page, a folder).
    External,
    /// A switch, on or off.
    Switch(bool),
    /// Short text, e.g. a version.
    Text(String),
}

/// One row of a group in a popup.
#[derive(Clone, Debug)]
pub struct Row {
    /// Its picture, if any.
    pub glyph: Option<Icon>,
    /// What it says.
    pub label: String,
    /// A second line in small text, if any.
    pub detail: String,
    /// What sits at its right end.
    pub trailing: Trailing,
    /// `false` grays it out.
    pub enabled: bool,
}

impl Row {
    /// A row saying `label`, with the icon `glyph` and `trailing`.
    #[must_use]
    pub fn new(glyph: Option<Icon>, label: impl Into<String>, trailing: Trailing) -> Self {
        Self {
            glyph,
            label: label.into(),
            detail: String::new(),
            trailing,
            enabled: true,
        }
    }

    /// The same row with a second line.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }
}

/// A group of rows sharing one box, the width of `ui`. Returns the row
/// clicked (a click on a row's switch counts).
pub fn group(ui: &mut Ui, rows: &[Row], theme: &Theme) -> Option<usize> {
    let width = ui.available_width();
    let mut clicked = None;
    let top = ui.cursor().min.y;
    for (index, row) in rows.iter().enumerate() {
        let height = if row.detail.is_empty() { 42.0 } else { 52.0 };
        let sense = if row.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            theme.radius(Radius::Md),
            if row.enabled && response.hovered() {
                theme.hover
            } else {
                theme.pill
            },
        );
        if index > 0 {
            painter.line_segment(
                [
                    rect.left_top() + vec2(12.0, 0.0),
                    rect.right_top() - vec2(12.0, 0.0),
                ],
                theme.hairline(),
            );
        }
        let ink = if row.enabled {
            theme.text
        } else {
            theme.muted.gamma_multiply(0.6)
        };
        let mut x = rect.left() + 14.0;
        if let Some(picture) = row.glyph {
            square_icon(
                painter,
                Rect::from_center_size(pos2(x + 9.0, rect.center().y), Vec2::splat(18.0)),
                picture,
                ink,
            );
            x += 28.0;
        }
        if row.detail.is_empty() {
            painter.text(
                pos2(x, rect.center().y),
                Align2::LEFT_CENTER,
                &row.label,
                theme.font(theme.ui_size + 1.0),
                ink,
            );
        } else {
            painter.text(
                pos2(x, rect.top() + 16.0),
                Align2::LEFT_CENTER,
                &row.label,
                theme.font(theme.ui_size + 1.0),
                ink,
            );
            let detail_rect = Rect::from_min_max(
                pos2(x, rect.top()),
                pos2(rect.right() - 60.0, rect.bottom()),
            );
            painter.with_clip_rect(detail_rect).text(
                pos2(x, rect.top() + 36.0),
                Align2::LEFT_CENTER,
                &row.detail,
                theme.font(theme.small_size),
                theme.muted,
            );
        }
        let end = pos2(rect.right() - 14.0, rect.center().y);
        let mut hit = row.enabled && response.clicked();
        match &row.trailing {
            Trailing::None => {}
            Trailing::More => square_icon(
                painter,
                Rect::from_center_size(end - vec2(6.0, 0.0), Vec2::splat(14.0)),
                Icon::ChevronRight,
                theme.muted,
            ),
            Trailing::External => square_icon(
                painter,
                Rect::from_center_size(end - vec2(7.0, 0.0), Vec2::splat(15.0)),
                Icon::External,
                theme.muted,
            ),
            Trailing::Text(text) => {
                painter.text(
                    end,
                    Align2::RIGHT_CENTER,
                    text,
                    theme.font(theme.small_size),
                    theme.muted,
                );
            }
            Trailing::Switch(on) => {
                let switch = Rect::from_min_max(end - vec2(46.0, 12.0), end + vec2(0.0, 12.0));
                hit |= toggle(ui, switch, *on, response.id.with("switch"), theme);
            }
        }
        if hit {
            clicked = Some(index);
        }
    }
    let bottom = ui.cursor().min.y - ui.spacing().item_spacing.y;
    ui.painter().rect_stroke(
        Rect::from_min_max(
            pos2(ui.min_rect().left(), top),
            pos2(ui.min_rect().left() + width, bottom),
        ),
        theme.radius(Radius::Lg),
        theme.hairline(),
        StrokeKind::Inside,
    );
    clicked
}

/// A popup's frame, title and X: what it answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupOutcome {
    /// Still open.
    Open,
    /// Closed (the X, Escape or a click on the dimmed page).
    Closed,
}

/// Shows a popup `width` wide (and at most `height` tall) in the middle of
/// the screen over the dimmed page, with `title` and an X; `body` draws its
/// rows. Escape or a click beside it closes it.
pub fn popup(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: Vec2,
    theme: &Theme,
    body: impl FnOnce(&mut Ui),
) -> PopupOutcome {
    let screen = ctx.content_rect();
    let mut outcome = PopupOutcome::Open;
    egui::Area::new(Id::unique(("sue-start-scrim", id)))
        .fade_in(false)
        .order(Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let response = ui.allocate_rect(screen, Sense::click());
            ui.painter().rect_filled(
                screen,
                CornerRadius::ZERO,
                theme.shadow.gamma_multiply(0.55),
            );
            if response.clicked() {
                outcome = PopupOutcome::Closed;
            }
        });
    let size = size.min(screen.size() - vec2(16.0, 16.0));
    let rect = Rect::from_center_size(screen.center(), size);
    egui::Area::new(Id::unique(("sue-start-popup", id)))
        .fade_in(false)
        .order(Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.set_min_size(size);
            ui.set_max_size(size);
            let painter = ui.painter();
            painter.rect_filled(
                rect.translate(vec2(0.0, 6.0)).expand(2.0),
                theme.radius(Radius::Xl),
                theme.shadow.gamma_multiply(0.5),
            );
            painter.rect(
                rect,
                theme.corner(Radius::Xl),
                theme.panel,
                theme.hairline(),
                StrokeKind::Inside,
            );
            painter.text(
                pos2(rect.center().x, rect.top() + 28.0),
                Align2::CENTER_CENTER,
                title,
                theme.font(theme.heading_size),
                theme.text,
            );
            let close = Rect::from_center_size(
                pos2(rect.right() - 28.0, rect.top() + 28.0),
                Vec2::splat(26.0),
            );
            if icon_button(ui, close, Icon::Close, "", false, theme).clicked() {
                outcome = PopupOutcome::Closed;
            }
            let inner = Rect::from_min_max(
                pos2(rect.left() + 20.0, rect.top() + 56.0),
                pos2(rect.right() - 20.0, rect.bottom() - 20.0),
            );
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(inner);
            egui::ScrollArea::vertical()
                .id_salt(("sue-start-popup-body", id))
                .auto_shrink([false, true])
                .show(&mut child, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.set_width(inner.width());
                    body(ui);
                });
        });
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        outcome = PopupOutcome::Closed;
    }
    outcome
}

/// A sheet over the whole screen (phones and tablets), with `title` in the
/// middle of its top, an X at the right, and `body`'s rows under them.
/// Escape closes it too.
pub fn sheet(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    theme: &Theme,
    body: impl FnOnce(&mut Ui),
) -> PopupOutcome {
    let rect = ctx.content_rect();
    let mut outcome = PopupOutcome::Open;
    egui::Area::new(Id::unique(("sue-start-sheet", id)))
        .fade_in(false)
        .order(Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.set_min_size(rect.size());
            ui.set_max_size(rect.size());
            // The sheet takes every touch, so nothing under it reacts.
            let _ = ui.interact(rect, ui.scope_id().with("cover"), Sense::click_and_drag());
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, theme.panel);
            ui.painter().text(
                pos2(rect.center().x, rect.top() + 34.0),
                Align2::CENTER_CENTER,
                title,
                theme.font(theme.heading_size),
                theme.text,
            );
            let close = Rect::from_center_size(
                pos2(rect.right() - 32.0, rect.top() + 34.0),
                Vec2::splat(44.0),
            );
            if icon_button(ui, close, Icon::Close, "", false, theme).clicked() {
                outcome = PopupOutcome::Closed;
            }
            let side = ((rect.width() - 560.0) / 2.0).max(16.0);
            let inner = Rect::from_min_max(
                pos2(rect.left() + side, rect.top() + 68.0),
                pos2(rect.right() - side, rect.bottom() - 12.0),
            );
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(inner);
            egui::ScrollArea::vertical()
                .id_salt(("sue-start-sheet-body", id))
                .auto_shrink([false, false])
                .show(&mut child, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.set_width(inner.width());
                    body(ui);
                });
        });
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        outcome = PopupOutcome::Closed;
    }
    outcome
}

/// A row of tabs `width` wide, the chosen one underlined. Returns the tab
/// tapped.
pub fn tabs(
    ui: &mut Ui,
    width: f32,
    labels: &[String],
    chosen: usize,
    theme: &Theme,
) -> Option<usize> {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 40.0), Sense::hover());
    let painter = ui.painter().clone();
    painter.line_segment([rect.left_bottom(), rect.right_bottom()], theme.hairline());
    let each = rect.width() / labels.len().max(1) as f32;
    let mut tapped = None;
    for (index, label) in labels.iter().enumerate() {
        let cell = Rect::from_min_size(
            pos2(rect.left() + index as f32 * each, rect.top()),
            vec2(each, rect.height()),
        );
        let response = ui.interact(
            cell,
            ui.scope_id().with(("sue-tabs", index)),
            Sense::click(),
        );
        let on = index == chosen;
        painter.text(
            cell.center(),
            Align2::CENTER_CENTER,
            label,
            theme.font(theme.ui_size),
            if on { theme.text } else { theme.muted },
        );
        if on {
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(cell.left() + 6.0, cell.bottom() - 2.0),
                    pos2(cell.right() - 6.0, cell.bottom()),
                ),
                theme.radius(Radius::Full),
                theme.border_active,
            );
        }
        if response.clicked() {
            tapped = Some(index);
        }
    }
    tapped
}

/// What a tile shows above its name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Preview {
    /// An icon.
    Icon(Icon),
    /// A sheet of paper of this width to height.
    Paper(f32),
}

/// A template tile `side` wide: its preview in a square, then its name and
/// a small line under it.
pub fn tile(
    ui: &mut Ui,
    side: f32,
    preview: Preview,
    name: &str,
    detail: &str,
    theme: &Theme,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(side, side + 44.0), Sense::click());
    let square = Rect::from_min_size(rect.min, Vec2::splat(side));
    let painter = ui.painter();
    painter.rect(
        square,
        theme.radius(Radius::Lg),
        if response.hovered() {
            theme.hover
        } else {
            theme.pill
        },
        if response.hovered() {
            theme.hairline_active()
        } else {
            theme.hairline()
        },
        StrokeKind::Inside,
    );
    match preview {
        Preview::Icon(picture) => square_icon(
            painter,
            Rect::from_center_size(square.center(), Vec2::splat(side * 0.4)),
            picture,
            theme.text,
        ),
        Preview::Paper(aspect) => {
            let room = side * 0.72;
            let aspect = aspect.clamp(0.02, 50.0);
            let size = if aspect >= 1.0 {
                vec2(room, room / aspect)
            } else {
                vec2((room * aspect).max(6.0), room)
            };
            let paper = Rect::from_center_size(square.center(), size);
            painter.rect_filled(
                paper.translate(vec2(3.0, 3.0)),
                theme.radius(Radius::Xs),
                theme.shadow.gamma_multiply(0.5),
            );
            painter.rect_filled(paper, theme.radius(Radius::Xs), Color32::WHITE);
        }
    }
    painter.text(
        pos2(rect.center().x, square.bottom() + 12.0),
        Align2::CENTER_CENTER,
        name,
        theme.font(theme.ui_size),
        theme.text,
    );
    painter.text(
        pos2(rect.center().x, square.bottom() + 30.0),
        Align2::CENTER_CENTER,
        detail,
        theme.font(theme.small_size),
        theme.muted,
    );
    response
}

/// An outlined button with a label (and an icon before it, if any),
/// sized to its text.
pub fn outline_button(ui: &mut Ui, picture: Option<Icon>, label: &str, theme: &Theme) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme.font(theme.ui_size), theme.text);
    let lead = if picture.is_some() { 28.0 } else { 0.0 };
    let size = vec2(galley.size().x + 28.0 + lead, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, theme.radius(Radius::Md), theme.hover);
    }
    painter.rect_stroke(
        rect,
        theme.radius(Radius::Md),
        Stroke::new(1.5, theme.text),
        StrokeKind::Inside,
    );
    if let Some(picture) = picture {
        square_icon(
            painter,
            Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), Vec2::splat(16.0)),
            picture,
            theme.text,
        );
    }
    painter.galley(
        pos2(
            rect.left() + 14.0 + lead,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        theme.text,
    );
    response
}

/// A gap between groups in a popup.
pub fn gap(ui: &mut Ui) {
    ui.add_space(18.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY: [Icon; 40] = [
        Icon::More,
        Icon::ArrowLeft,
        Icon::Film,
        Icon::Draw,
        Icon::Projects,
        Icon::Materials,
        Icon::Transfers,
        Icon::Help,
        Icon::Settings,
        Icon::Plus,
        Icon::Delete,
        Icon::Rename,
        Icon::Heart,
        Icon::Details,
        Icon::ChevronRight,
        Icon::ChevronDown,
        Icon::Search,
        Icon::ViewGrid,
        Icon::ViewGridSmall,
        Icon::ViewList,
        Icon::SortUp,
        Icon::SortDown,
        Icon::Close,
        Icon::External,
        Icon::Moon,
        Icon::Folder,
        Icon::Wrench,
        Icon::Check,
        Icon::Picture,
        Icon::ToolBrush,
        Icon::MaterialPattern,
        Icon::MaterialTone,
        Icon::Cube,
        Icon::Recent,
        Icon::Tag,
        Icon::MoveTo,
        Icon::Book,
        Icon::Feedback,
        Icon::Mail,
        Icon::All,
    ];

    #[test]
    fn every_widget_draws_and_a_popup_closes_on_escape() {
        let mut rice = sue_rice::RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        let theme = Theme::from_rice(&rice);
        let ctx = egui::Context::default();
        let mut closed = false;
        for frame in 0..2 {
            let mut input = egui::RawInput::default();
            if frame == 1 {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let output = ctx.run_ui(input, |ui| {
                for (index, picture) in EVERY.iter().enumerate() {
                    let rect =
                        Rect::from_min_size(pos2(index as f32 * 20.0, 0.0), Vec2::splat(18.0));
                    square_icon(ui.painter(), rect, *picture, theme.text);
                }
                let mut text = "cat".to_owned();
                search_box(ui, &mut text, "Search", 200.0, &theme);
                chip(ui, "Tones", true, &theme);
                check(ui, "Select All", Chosen::Some, &theme);
                let mut current = 0;
                dropdown(
                    ui,
                    Id::unique("sort"),
                    &mut current,
                    &["Name".to_owned()],
                    120.0,
                    &theme,
                );
                let row = Category {
                    glyph: Some(Icon::All),
                    label: "All".into(),
                    depth: 0,
                    open: Some(true),
                };
                category(ui, 180.0, &row, true, &theme);
                let shown = Card {
                    title: "Cat".into(),
                    detail: "just now".into(),
                    picture: None,
                    placeholder: Icon::Picture,
                    favorite: Some(true),
                    chosen: true,
                    hover: String::new(),
                };
                card(ui, vec2(144.0, 190.0), &shown, &theme);
                card(ui, vec2(96.0, 118.0), &shown, &theme);
                list_row(ui, 400.0, &shown, &theme);
                let outcome = popup(ui.ctx(), "test", "Help", vec2(420.0, 300.0), &theme, |ui| {
                    group(
                        ui,
                        &[
                            Row::new(Some(Icon::Moon), "Dark mode", Trailing::Switch(true)),
                            Row::new(None, "Version", Trailing::Text("1.0".into())).detail("Sue"),
                        ],
                        &theme,
                    );
                });
                closed |= outcome == PopupOutcome::Closed;
            });
            assert!(!output.shapes.is_empty());
            output.drop_without_applying_deltas();
        }
        assert!(closed, "Escape closes a popup");
    }
}
