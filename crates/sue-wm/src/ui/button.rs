//! Buttons: what a person presses to make something happen.
//!
//! - [`button`]: a labelled button in one of four kinds
//!   ([`ButtonKind`]): primary for the one thing a page or dialog is
//!   for, secondary for everything beside it, ghost where a frame would
//!   be noise (toolbars, rows), danger for what deletes or cannot be
//!   undone.
//! - [`toggle_button`]: a button that stays lit while something is on.
//! - [`icon_button`]: a square button that shows only an icon. Its name
//!   is read out and shown on hover.
//! - [`split_button`]: a button with a second, narrow half that opens a
//!   menu of related actions.
//! - [`link`]: a line of text that leads somewhere.
//!
//! Every one of them reads its sizes from the [`Density`] of the `Ui` it
//! is shown in, takes its colors from the [`Theme`], draws the focus
//! ring, tells assistive technology what it is, moves through
//! [`motion`] and is named by a salt its caller gives it: the salt, and
//! not the place it happens to be drawn at, is what keeps its focus and
//! its motion from frame to frame. Two controls in the same `Ui` need
//! different salts.
//!
//! A control does nothing by itself: `show` returns the
//! [`Response`], and `clicked()` on it is the press.
//!
//! [`sheet`] draws all of them, in every state, for review.

use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    AsIdSalt, Color32, CornerRadius, CursorIcon, FontId, Galley, Id, IdSalt, Painter, Pos2, Rect,
    Response, Role, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};

use super::{Density, Radius, Space, TextRole, contrast, focus, motion, nested_radius};
use crate::Theme;
use crate::icons::{self, Icon, IconStyle};

/// How far a solid fill moves toward its ink under the pointer.
const SOLID_HOVER: f32 = 0.13;

/// How far a solid fill moves toward its ink while pressed.
const SOLID_PRESS: f32 = 0.26;

/// How far the fill of what is chosen moves toward the text color under
/// the pointer.
const CHOSEN_HOVER: f32 = 0.1;

/// How far the fill of what is chosen moves toward the text color while
/// pressed.
const CHOSEN_PRESS: f32 = 0.2;

/// The gap between a split button's two halves and the ends of the line
/// that parts them, as a share of the height.
const DIVIDER_INSET: f32 = 0.25;

/// `from` moved the share `t` of the way to `to`.
pub(super) fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    from.lerp_to_gamma(to, t.clamp(0.0, 1.0))
}

/// The color of text and icons on a solid `fill`: the page color or the
/// text color, whichever reads better on it.
pub(super) fn ink_on(fill: Color32, theme: &Theme) -> Color32 {
    if contrast(theme.void, fill) >= contrast(theme.text, fill) {
        theme.void
    } else {
        theme.text
    }
}

/// The text and icons of a control that cannot be used now. A control
/// does not know which surface it lies on, so this is the disabled color
/// that reads best on the page, on panels and on cards at once.
pub(super) fn disabled_ink(theme: &Theme) -> Color32 {
    let worst = |ink: Color32| {
        (0..=2)
            .map(|level| contrast(ink, theme.surface(level)))
            .fold(f32::INFINITY, f32::min)
    };
    (0..=2)
        .map(|level| theme.text_disabled(theme.surface(level)))
        .max_by(|a, b| worst(*a).total_cmp(&worst(*b)))
        .unwrap_or(theme.muted)
}

/// The outline of a control at rest: between a hairline and the strong
/// border, so it frames the control without shouting.
pub(super) fn edge(theme: &Theme) -> Color32 {
    mix(theme.border, theme.border_strong(), 0.5)
}

/// What a control looks like at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Visual {
    /// It can be used.
    pub enabled: bool,
    /// How far it is on its way to its look under the pointer, 0 to 1.
    pub hover: f32,
    /// The pointer is held down on it.
    pub pressed: bool,
    /// The opacity of its focus ring, 0 to 1.
    pub focus: f32,
}

impl Visual {
    /// Nothing is happening to it.
    pub(super) const REST: Self = Self {
        enabled: true,
        hover: 0.0,
        pressed: false,
        focus: 0.0,
    };

    /// The states a review sheet shows, with their names.
    pub(super) const STATES: [(&'static str, Self); 5] = [
        ("Rest", Self::REST),
        (
            "Hover",
            Self {
                hover: 1.0,
                ..Self::REST
            },
        ),
        (
            "Pressed",
            Self {
                hover: 1.0,
                pressed: true,
                ..Self::REST
            },
        ),
        (
            "Focus",
            Self {
                focus: 1.0,
                ..Self::REST
            },
        ),
        (
            "Disabled",
            Self {
                enabled: false,
                ..Self::REST
            },
        ),
    ];

    /// The look of the control behind `response` now. Its hover fill and
    /// its focus ring fade at the fast pace.
    pub(super) fn of(ui: &Ui, response: &Response, enabled: bool) -> Self {
        let ctx = ui.ctx();
        let pressed = enabled && response.is_pointer_button_down_on();
        let hovered = enabled && (response.hovered() || pressed);
        let focused = enabled && focus::focus_visible(ctx) && response.has_focus();
        Self {
            enabled,
            hover: motion::hover_t(ctx, response.id.with("sue-hover"), hovered),
            pressed,
            focus: motion::hover_t(ctx, response.id.with("sue-focus-ring"), focused),
        }
    }
}

/// `rect`, grown about its middle until no side is shorter than `least`.
pub(super) fn hit_rect(rect: Rect, least: f32) -> Rect {
    let grow = ((Vec2::splat(least) - rect.size()) / 2.0).max(Vec2::ZERO);
    rect.expand2(grow)
}

/// The id of the control its caller named `salt`, in `ui`.
pub(super) fn control_id(ui: &Ui, salt: IdSalt) -> Id {
    ui.scope_id().with_salt(salt)
}

/// Senses clicks on the control `id` drawn at `rect`. What it can be
/// hit on is never smaller than the density's least hit area, also where
/// the drawing is; the response keeps `rect`. A control that is not
/// `enabled` (or lies in a disabled `Ui`) takes neither clicks nor focus.
pub(super) fn interact(ui: &Ui, rect: Rect, id: Id, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let least = Density::of(ui).min_hit();
    ui.interact(hit_rect(rect, least), id, sense)
        .with_new_rect(rect)
}

/// Takes `size` in the layout of `ui`.
pub(super) fn place(ui: &mut Ui, size: Vec2) -> Rect {
    ui.allocate_space(size).1
}

/// One line of `text` in `font`, cut short with an ellipsis where it is
/// wider than `width`. Its color is given when it is painted.
pub(super) fn line(painter: &Painter, text: &str, font: FontId, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, Color32::PLACEHOLDER);
    if width.is_finite() {
        job.wrap = TextWrapping::truncate_at_width(width.max(0.0));
    }
    painter.layout_job(job)
}

/// Paints `galley` with its left edge at `left`, centered on the height
/// `middle`.
pub(super) fn paint_line(
    painter: &Painter,
    left: f32,
    middle: f32,
    galley: Arc<Galley>,
    ink: Color32,
) {
    let top = middle - galley.size().y / 2.0;
    painter.galley(pos2(left, top), galley, ink);
}

/// Paints a chevron `size` wide about `center`, pointing down, or up
/// when `flip` is 1.
pub(super) fn paint_chevron(painter: &Painter, center: Pos2, size: f32, flip: f32, ink: Color32) {
    let half = size / 2.0;
    let rise = size * 0.25 * (1.0 - 2.0 * flip.clamp(0.0, 1.0));
    painter.line(
        vec![
            pos2(center.x - half, center.y - rise),
            pos2(center.x, center.y + rise),
            pos2(center.x + half, center.y - rise),
        ],
        Stroke::new(icons::weight(size * 2.0), ink),
    );
}

/// Paints the mark of what cannot be undone into the square `rect`: a
/// ring with a stem over a dot.
fn paint_alert(painter: &Painter, rect: Rect, ink: Color32) {
    let side = rect.width().min(rect.height());
    let pen = icons::weight(side);
    let center = rect.center();
    let radius = side * 0.44 - pen / 2.0;
    painter.circle_stroke(center, radius, Stroke::new(pen, ink));
    painter.line_segment(
        [
            pos2(center.x, center.y - radius * 0.52),
            pos2(center.x, center.y + radius * 0.1),
        ],
        Stroke::new(pen, ink),
    );
    painter.circle_filled(pos2(center.x, center.y + radius * 0.5), pen * 0.62, ink);
}

/// What a button is for, which decides how much it stands out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonKind {
    /// The one thing a page or a dialog is for: solid, in the text color.
    Primary,
    /// Everything beside it: a quiet fill with an outline.
    #[default]
    Secondary,
    /// No frame until the pointer is over it: toolbars and rows.
    Ghost,
    /// What deletes or cannot be undone: outlined in the danger color
    /// and marked, solid under the pointer. The shipped themes are
    /// monochrome, so it is the mark and the outline that tell it apart,
    /// never color alone.
    Danger,
}

impl ButtonKind {
    /// Every kind.
    pub const ALL: [Self; 4] = [Self::Primary, Self::Secondary, Self::Ghost, Self::Danger];
}

/// The colors of a button at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Skin {
    /// What fills it.
    pub fill: Color32,
    /// Its outline.
    pub stroke: Stroke,
    /// Its text and icon.
    pub ink: Color32,
}

/// The skin of a button of `kind`, chosen (`on`) or not, in the state
/// `visual`.
pub(super) fn skin(kind: ButtonKind, on: bool, theme: &Theme, visual: Visual) -> Skin {
    let hairline = theme.border_width;
    if !visual.enabled {
        let ink = disabled_ink(theme);
        return match (on, kind) {
            (true, _) => {
                let fill = mix(theme.surface(1), theme.fill_selected(), 0.5);
                Skin {
                    fill,
                    stroke: Stroke::new(hairline, ink),
                    ink: theme.text_disabled(fill),
                }
            }
            (false, ButtonKind::Primary) => {
                let fill = mix(theme.surface(1), theme.text, 0.14);
                Skin {
                    fill,
                    stroke: Stroke::NONE,
                    ink: theme.text_disabled(fill),
                }
            }
            (false, ButtonKind::Secondary) => Skin {
                fill: Color32::TRANSPARENT,
                stroke: Stroke::new(hairline, theme.border),
                ink,
            },
            (false, ButtonKind::Ghost) => Skin {
                fill: Color32::TRANSPARENT,
                stroke: Stroke::NONE,
                ink,
            },
            (false, ButtonKind::Danger) => Skin {
                fill: Color32::TRANSPARENT,
                stroke: Stroke::new(hairline, ink),
                ink,
            },
        };
    }
    let hover = visual.hover.clamp(0.0, 1.0);
    if on {
        return Skin {
            fill: chosen_fill(theme, visual),
            stroke: theme.hairline_active(),
            ink: theme.text,
        };
    }
    match kind {
        ButtonKind::Primary => {
            let solid = theme.text;
            let ink = ink_on(solid, theme);
            let dip = if visual.pressed {
                SOLID_PRESS
            } else {
                SOLID_HOVER * hover
            };
            Skin {
                fill: mix(solid, ink, dip),
                stroke: Stroke::NONE,
                ink,
            }
        }
        ButtonKind::Secondary => Skin {
            fill: if visual.pressed {
                theme.fill_pressed()
            } else {
                mix(theme.surface(2), theme.fill_hover(), hover)
            },
            stroke: Stroke::new(hairline, mix(edge(theme), theme.border_strong(), hover)),
            ink: theme.text,
        },
        ButtonKind::Ghost => Skin {
            fill: if visual.pressed {
                theme.fill_pressed()
            } else {
                theme.fill_hover().gamma_multiply(hover)
            },
            stroke: Stroke::NONE,
            ink: theme.text,
        },
        ButtonKind::Danger => {
            let danger = theme.danger();
            let inverse = ink_on(danger, theme);
            let fill = if visual.pressed {
                mix(danger, inverse, SOLID_PRESS)
            } else {
                danger.gamma_multiply(hover)
            };
            // The label takes whichever of the two inks reads better on
            // the fill as it is now. Blending the inks along with the
            // fill would make them meet halfway, where the label would
            // vanish for a moment on every hover.
            let seen = theme.surface(1).blend(fill);
            Skin {
                fill,
                stroke: Stroke::new(Theme::STRONG_STROKE * 0.75, danger),
                ink: if contrast(danger, seen) >= contrast(inverse, seen) {
                    danger
                } else {
                    inverse
                },
            }
        }
    }
}

/// The fill of something chosen (a toggle that is on, a chip that is
/// lit) in the state `visual`. Under the pointer and held down it moves
/// toward the text color, which brightens it in a dark theme; where that
/// would leave the text on it under 4.5 to 1 (the chosen fill of Gray is
/// already close to its text) it moves the other way, so the change is
/// always seen and the label always reads.
pub(super) fn chosen_fill(theme: &Theme, visual: Visual) -> Color32 {
    let chosen = theme.fill_selected();
    let lift = if visual.pressed {
        CHOSEN_PRESS
    } else {
        CHOSEN_HOVER * visual.hover.clamp(0.0, 1.0)
    };
    let toward = mix(chosen, theme.text, lift);
    if contrast(theme.text, toward) >= READS {
        toward
    } else {
        mix(chosen, ink_on(theme.text, theme), lift)
    }
}

/// The contrast text needs on what it lies on.
const READS: f32 = 4.5;

/// How far a button's body sinks in on every side while it is held down.
pub(super) const PRESS_SINK: f32 = 1.0;

/// Paints a button's body: its fill and its outline. Held down, the body
/// sinks in a little, so a press is felt as well as seen.
fn paint_body(painter: &Painter, rect: Rect, radius: f32, skin: &Skin, visual: Visual) {
    let (rect, radius) = if visual.pressed && visual.enabled {
        (rect.shrink(PRESS_SINK), (radius - PRESS_SINK).max(0.0))
    } else {
        (rect, radius)
    };
    painter.rect(rect, radius, skin.fill, skin.stroke, StrokeKind::Inside);
}

/// The space between a button's edge and what it holds.
fn side_padding(theme: &Theme, density: Density) -> f32 {
    theme.space(match density {
        Density::Compact => Space::S4,
        Density::Regular => Space::S5,
        Density::Touch => Space::S6,
    })
}

/// What stands before a button's label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lead {
    /// An icon of the set.
    Icon(Icon),
    /// The mark of what cannot be undone.
    Alert,
}

/// A labelled button. Made by [`button`] and [`toggle_button`].
#[derive(Clone, Debug)]
#[must_use = "a button is drawn by `show`"]
pub struct Button<'a> {
    salt: IdSalt,
    label: &'a str,
    kind: ButtonKind,
    icon: Option<Icon>,
    on: Option<bool>,
    enabled: bool,
    min_width: f32,
    full_width: bool,
    hint: &'a str,
}

/// A secondary button that says `label`. `salt` names it among the
/// controls of its `Ui`.
pub fn button(salt: impl AsIdSalt, label: &str) -> Button<'_> {
    Button {
        salt: IdSalt::new(salt),
        label,
        kind: ButtonKind::Secondary,
        icon: None,
        on: None,
        enabled: true,
        min_width: 0.0,
        full_width: false,
        hint: "",
    }
}

/// A button that stays lit while `on`. A click asks to flip it; the
/// caller does the flipping.
pub fn toggle_button(salt: impl AsIdSalt, label: &str, on: bool) -> Button<'_> {
    Button {
        on: Some(on),
        ..button(salt, label)
    }
}

impl<'a> Button<'a> {
    /// Makes it a button of `kind`.
    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }

    /// Makes it the primary button.
    pub fn primary(self) -> Self {
        self.kind(ButtonKind::Primary)
    }

    /// Makes it a ghost button.
    pub fn ghost(self) -> Self {
        self.kind(ButtonKind::Ghost)
    }

    /// Makes it a danger button. Without an [`icon`](Self::icon) of its
    /// own it shows the mark of what cannot be undone.
    pub fn danger(self) -> Self {
        self.kind(ButtonKind::Danger)
    }

    /// Puts `icon` before the label.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Whether it can be pressed. A button that cannot is grayed and
    /// takes neither clicks nor focus.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes it at least `width` wide, so that the buttons of a row
    /// match.
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Makes it as wide as the room it is shown in.
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    /// Shows `hint` when the pointer rests on it.
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    fn lead(&self) -> Option<Lead> {
        match (self.icon, self.kind) {
            (Some(icon), _) => Some(Lead::Icon(icon)),
            (None, ButtonKind::Danger) => Some(Lead::Alert),
            (None, _) => None,
        }
    }

    /// The width its lead takes, with the gap after it.
    fn lead_width(&self, theme: &Theme, density: Density) -> f32 {
        match self.lead() {
            Some(_) if self.label.is_empty() => density.icon(),
            Some(_) => density.icon() + theme.space(Space::S3),
            None => 0.0,
        }
    }

    /// The size it takes when nothing stretches it.
    #[must_use]
    pub fn size(&self, painter: &Painter, theme: &Theme, density: Density) -> Vec2 {
        let height = density.control_height();
        let text = line(
            painter,
            self.label,
            theme.text_font(TextRole::Body),
            f32::INFINITY,
        );
        let width =
            side_padding(theme, density) * 2.0 + self.lead_width(theme, density) + text.size().x;
        vec2(width.max(self.min_width).max(height).ceil(), height)
    }

    /// Shows it in the layout of `ui`.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let mut size = self.size(ui.painter(), theme, Density::of(ui));
        let room = ui.available_width();
        if self.full_width && room.is_finite() {
            size.x = size.x.max(room);
        }
        let rect = place(ui, size);
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &Ui, rect: Rect, theme: &Theme) -> Response {
        let enabled = self.enabled && ui.is_enabled();
        let response = interact(ui, rect, control_id(ui, self.salt), enabled);
        if ui.is_rect_visible(rect) {
            let visual = Visual::of(ui, &response, enabled);
            self.paint(ui.painter(), rect, theme, Density::of(ui), visual);
        }
        let (label, on) = (self.label, self.on);
        response.widget_info(|| match on {
            Some(on) => WidgetInfo::selected(Role::Button, enabled, on, label),
            None => WidgetInfo::labeled(Role::Button, enabled, label),
        });
        if self.hint.is_empty() {
            response
        } else {
            response.on_hover_text(self.hint)
        }
    }

    fn paint(
        &self,
        painter: &Painter,
        rect: Rect,
        theme: &Theme,
        density: Density,
        visual: Visual,
    ) {
        let skin = skin(self.kind, self.on == Some(true), theme, visual);
        let radius = theme.radius(Radius::Md).min(rect.height() / 2.0);
        paint_body(painter, rect, radius, &skin, visual);
        let lead_width = self.lead_width(theme, density);
        let room = rect.width() - side_padding(theme, density) * 2.0 - lead_width;
        let text = line(painter, self.label, theme.text_font(TextRole::Body), room);
        let mut left = rect.center().x - (lead_width + text.size().x) / 2.0;
        if let Some(lead) = self.lead() {
            let side = density.icon();
            let place =
                Rect::from_center_size(pos2(left + side / 2.0, rect.center().y), Vec2::splat(side));
            match lead {
                Lead::Icon(icon) => icons::paint(painter, place, icon, skin.ink),
                Lead::Alert => paint_alert(painter, place, skin.ink),
            }
            left += lead_width;
        }
        paint_line(painter, left, rect.center().y, text, skin.ink);
        focus::paint_focus_ring(painter, rect, radius, visual.focus, theme);
    }
}

/// A square button that shows only an icon. Made by [`icon_button`].
#[derive(Clone, Debug)]
#[must_use = "a button is drawn by `show`"]
pub struct IconButton<'a> {
    salt: IdSalt,
    icon: Icon,
    name: &'a str,
    kind: ButtonKind,
    on: Option<bool>,
    enabled: bool,
    quiet: bool,
    shortcut: &'a str,
}

/// A ghost button that shows `icon`. `name` says what it does: it is
/// read out by assistive technology and shown when the pointer rests on
/// the button, since an icon alone names nothing.
pub fn icon_button(salt: impl AsIdSalt, icon: Icon, name: &str) -> IconButton<'_> {
    IconButton {
        salt: IdSalt::new(salt),
        icon,
        name,
        kind: ButtonKind::Ghost,
        on: None,
        enabled: true,
        quiet: false,
        shortcut: "",
    }
}

impl<'a> IconButton<'a> {
    /// Makes it a button of `kind`.
    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }

    /// Makes it a toggle that is lit while `on`. An icon with a solid
    /// twin shows that while on.
    pub fn selected(mut self, on: bool) -> Self {
        self.on = Some(on);
        self
    }

    /// Whether it can be pressed.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws the icon in the secondary text color until the pointer is
    /// over it or it is on: for strips of many icons, where the one that
    /// is on should be the one that is seen.
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// Shows the keys that run what it does (`Ctrl+0`) in its tooltip,
    /// after its name; empty for none.
    pub fn shortcut(mut self, keys: &'a str) -> Self {
        self.shortcut = keys;
        self
    }

    /// The size it takes: a square.
    #[must_use]
    pub fn size(&self, density: Density) -> Vec2 {
        Vec2::splat(density.icon_button())
    }

    /// Shows it in the layout of `ui`.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let rect = place(ui, self.size(Density::of(ui)));
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &Ui, rect: Rect, theme: &Theme) -> Response {
        let enabled = self.enabled && ui.is_enabled();
        let response = interact(ui, rect, control_id(ui, self.salt), enabled);
        if ui.is_rect_visible(rect) {
            let visual = Visual::of(ui, &response, enabled);
            self.paint(ui.painter(), rect, theme, Density::of(ui), visual);
        }
        let (name, on) = (self.name, self.on);
        response.widget_info(|| match on {
            Some(on) => WidgetInfo::selected(Role::Button, enabled, on, name),
            None => WidgetInfo::labeled(Role::Button, enabled, name),
        });
        if name.is_empty() {
            response
        } else if self.shortcut.is_empty() {
            response.on_hover_text(name)
        } else {
            super::tooltip(
                response,
                &super::Tip::new(name).shortcut(self.shortcut),
                theme,
            )
        }
    }

    fn paint(
        &self,
        painter: &Painter,
        rect: Rect,
        theme: &Theme,
        density: Density,
        visual: Visual,
    ) {
        let on = self.on == Some(true);
        let skin = skin(self.kind, on, theme, visual);
        let radius = theme.radius(Radius::Md).min(rect.height() / 2.0);
        paint_body(painter, rect, radius, &skin, visual);
        let framed = matches!(self.kind, ButtonKind::Primary | ButtonKind::Danger);
        let ink = if self.quiet && visual.enabled && !on && !framed {
            mix(theme.text_muted(), skin.ink, visual.hover)
        } else {
            skin.ink
        };
        let side = density
            .icon()
            .min(rect.width().min(rect.height()) * 0.75)
            .round();
        let style = if on {
            IconStyle::Solid
        } else {
            IconStyle::Line
        };
        icons::paint_styled(
            painter,
            Rect::from_center_size(rect.center(), Vec2::splat(side)),
            self.icon,
            style,
            ink,
        );
        focus::paint_focus_ring(painter, rect, radius, visual.focus, theme);
    }
}

/// A button with a second half that opens a menu. Made by
/// [`split_button`].
#[derive(Clone, Debug)]
#[must_use = "a button is drawn by `show`"]
pub struct SplitButton<'a> {
    salt: IdSalt,
    label: &'a str,
    menu_name: &'a str,
    kind: ButtonKind,
    icon: Option<Icon>,
    enabled: bool,
    open: bool,
}

/// What a [`SplitButton`] answers with.
#[derive(Clone, Debug)]
pub struct SplitResponse {
    /// The half with the label: `clicked()` is the action itself.
    pub main: Response,
    /// The half with the chevron: `clicked()` asks for the menu.
    pub menu: Response,
    /// The whole button, to hang the menu under.
    pub rect: Rect,
}

/// A button that says `label`, with a half beside it that opens a menu
/// of related actions. `menu_name` says what that half does ("More ways
/// to save"): it is read out and shown on hover.
pub fn split_button<'a>(
    salt: impl AsIdSalt,
    label: &'a str,
    menu_name: &'a str,
) -> SplitButton<'a> {
    SplitButton {
        salt: IdSalt::new(salt),
        label,
        menu_name,
        kind: ButtonKind::Secondary,
        icon: None,
        enabled: true,
        open: false,
    }
}

impl SplitButton<'_> {
    /// Makes it a button of `kind`.
    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }

    /// Makes it the primary button.
    pub fn primary(self) -> Self {
        self.kind(ButtonKind::Primary)
    }

    /// Puts `icon` before the label.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Whether it can be pressed.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Says whether its menu is open now: the half that opened it stays
    /// pressed and its chevron points up.
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// The label half, as a button of its own.
    fn label_half(&self) -> Button<'_> {
        Button {
            salt: self.salt,
            kind: self.kind,
            icon: self.icon,
            ..button("sue-split-label", self.label)
        }
    }

    /// The width of the half with the chevron.
    fn menu_width(density: Density) -> f32 {
        density.control_height().max(density.min_hit())
    }

    /// The size it takes.
    #[must_use]
    pub fn size(&self, painter: &Painter, theme: &Theme, density: Density) -> Vec2 {
        let label = self.label_half().size(painter, theme, density);
        vec2(label.x + Self::menu_width(density), label.y)
    }

    /// Its two halves inside `rect`: the label, then the chevron.
    fn halves(rect: Rect, density: Density) -> [Rect; 2] {
        let split = (rect.right() - Self::menu_width(density)).max(rect.center().x);
        [
            Rect::from_min_max(rect.min, pos2(split, rect.bottom())),
            Rect::from_min_max(pos2(split, rect.top()), rect.max),
        ]
    }

    /// Shows it in the layout of `ui`.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> SplitResponse {
        let rect = place(ui, self.size(ui.painter(), theme, Density::of(ui)));
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &Ui, rect: Rect, theme: &Theme) -> SplitResponse {
        let density = Density::of(ui);
        let enabled = self.enabled && ui.is_enabled();
        let id = control_id(ui, self.salt);
        let [label_rect, menu_rect] = Self::halves(rect, density);
        let main = interact(ui, label_rect, id.with("sue-split-main"), enabled);
        let menu = interact(ui, menu_rect, id.with("sue-split-menu"), enabled);
        if ui.is_rect_visible(rect) {
            let turned = motion::timed(
                ui.ctx(),
                id.with("sue-split-open"),
                self.open,
                motion::Pace::Base,
            );
            let mut menu_visual = Visual::of(ui, &menu, enabled);
            menu_visual.pressed |= enabled && self.open;
            self.paint(
                ui.painter(),
                rect,
                theme,
                density,
                [Visual::of(ui, &main, enabled), menu_visual],
                turned,
            );
        }
        let (label, menu_name) = (self.label, self.menu_name);
        main.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
        menu.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, menu_name));
        let menu = if menu_name.is_empty() {
            menu
        } else {
            menu.on_hover_text(menu_name)
        };
        SplitResponse { main, menu, rect }
    }

    fn paint(
        &self,
        painter: &Painter,
        rect: Rect,
        theme: &Theme,
        density: Density,
        visuals: [Visual; 2],
        turned: f32,
    ) {
        let [label_rect, menu_rect] = Self::halves(rect, density);
        let radius = theme.radius(Radius::Md).min(rect.height() / 2.0);
        let round = radius.round().clamp(0.0, 255.0) as u8;
        let corners = [
            CornerRadius {
                nw: round,
                sw: round,
                ne: 0,
                se: 0,
            },
            CornerRadius {
                nw: 0,
                sw: 0,
                ne: round,
                se: round,
            },
        ];
        let skins = visuals.map(|visual| skin(self.kind, false, theme, visual));
        for ((half, corner), skin) in [label_rect, menu_rect].into_iter().zip(corners).zip(skins) {
            painter.rect_filled(half, corner, skin.fill);
        }
        // One outline around both halves, and a line between them.
        let rest = skin(
            self.kind,
            false,
            theme,
            Visual {
                enabled: visuals[0].enabled,
                ..Visual::REST
            },
        );
        painter.rect_stroke(rect, radius, rest.stroke, StrokeKind::Inside);
        let inset = rect.height() * DIVIDER_INSET;
        let seam = painter.round_to_pixel_center(menu_rect.left());
        let parted = if skins[0].fill == skins[1].fill {
            rest.ink.gamma_multiply(0.3)
        } else {
            Color32::TRANSPARENT
        };
        painter.line_segment(
            [
                pos2(seam, rect.top() + inset),
                pos2(seam, rect.bottom() - inset),
            ],
            Stroke::new(theme.border_width, parted),
        );
        // The label half is drawn as the button it is, without a body.
        let label = self.label_half();
        let lead_width = label.lead_width(theme, density);
        let room = label_rect.width() - side_padding(theme, density) * 2.0 - lead_width;
        let text = line(painter, self.label, theme.text_font(TextRole::Body), room);
        let mut left = label_rect.center().x - (lead_width + text.size().x) / 2.0;
        if let Some(lead) = label.lead() {
            let side = density.icon();
            let place =
                Rect::from_center_size(pos2(left + side / 2.0, rect.center().y), Vec2::splat(side));
            match lead {
                Lead::Icon(icon) => icons::paint(painter, place, icon, skins[0].ink),
                Lead::Alert => paint_alert(painter, place, skins[0].ink),
            }
            left += lead_width;
        }
        paint_line(painter, left, rect.center().y, text, skins[0].ink);
        paint_chevron(
            painter,
            menu_rect.center(),
            density.icon() * 0.5,
            turned,
            skins[1].ink,
        );
        for (((half, corner), visual), skin) in [label_rect, menu_rect]
            .into_iter()
            .zip(corners)
            .zip(visuals)
            .zip(skins)
        {
            paint_inner_ring(painter, half, corner, skin.ink, visual.focus, theme);
        }
    }
}

/// How far a focus ring drawn inside a part of a control lies from the
/// part's edge.
const INNER_RING_GAP: f32 = 2.0;

/// Paints the focus ring of one part of a control (a half of a split
/// button, a cell of a strip) inside `rect`, whose corners are `corner`,
/// in `ink` at `opacity`. A ring around such a part would cross the
/// frame it shares with its neighbours, so this one lies within it, in
/// the color of what is drawn on the part so it reads on any fill.
pub(super) fn paint_inner_ring(
    painter: &Painter,
    rect: Rect,
    corner: CornerRadius,
    ink: Color32,
    opacity: f32,
    theme: &Theme,
) {
    if opacity <= 0.0 {
        return;
    }
    let stroke = Stroke::new(
        theme.focus_stroke().width,
        ink.gamma_multiply(opacity.min(1.0)),
    );
    let gap = INNER_RING_GAP;
    // Where the part meets its neighbour it has no corner of its own,
    // but a ring with a sharp corner there reads as a mistake: it is
    // rounded as the small radius, like every other corner of the ring.
    let least = theme.radius(Radius::Sm);
    let inner = |radius: u8| {
        nested_radius(f32::from(radius), gap)
            .max(least)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    painter.rect_stroke(
        rect.shrink(gap),
        CornerRadius {
            nw: inner(corner.nw),
            ne: inner(corner.ne),
            sw: inner(corner.sw),
            se: inner(corner.se),
        },
        stroke,
        StrokeKind::Inside,
    );
}

/// A line of text that leads somewhere. Made by [`link`].
#[derive(Clone, Debug)]
#[must_use = "a link is drawn by `show`"]
pub struct Link<'a> {
    salt: IdSalt,
    text: &'a str,
    role: TextRole,
    enabled: bool,
}

/// A link that says `text`. It is underlined, since the shipped themes
/// give links no color of their own. A click asks to follow it; the
/// caller does the following.
pub fn link(salt: impl AsIdSalt, text: &str) -> Link<'_> {
    Link {
        salt: IdSalt::new(salt),
        text,
        role: TextRole::Body,
        enabled: true,
    }
}

impl Link<'_> {
    /// Sets its text in `role`: a link in a caption is a caption.
    pub fn role(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    /// Whether it can be followed.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The size it takes: that of its text.
    #[must_use]
    pub fn size(&self, painter: &Painter, theme: &Theme) -> Vec2 {
        line(
            painter,
            self.text,
            theme.text_font(self.role),
            f32::INFINITY,
        )
        .size()
    }

    /// Shows it in the layout of `ui`.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let rect = place(ui, self.size(ui.painter(), theme));
        self.show_at(ui, rect, theme)
    }

    /// Shows it at `rect`, where the caller lays things out by hand.
    pub fn show_at(self, ui: &Ui, rect: Rect, theme: &Theme) -> Response {
        let enabled = self.enabled && ui.is_enabled();
        let response = interact(ui, rect, control_id(ui, self.salt), enabled);
        if ui.is_rect_visible(rect) {
            self.paint(
                ui.painter(),
                rect,
                theme,
                Visual::of(ui, &response, enabled),
            );
        }
        let text = self.text;
        response.widget_info(|| WidgetInfo::labeled(Role::Link, enabled, text));
        if enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        }
    }

    fn paint(&self, painter: &Painter, rect: Rect, theme: &Theme, visual: Visual) {
        let font = theme.text_font(self.role);
        let size = font.size;
        let text = line(painter, self.text, font, rect.width());
        let ink = if !visual.enabled {
            disabled_ink(theme)
        } else if visual.pressed {
            theme.text_muted()
        } else {
            theme.link()
        };
        let width = text.size().x;
        paint_line(painter, rect.left(), rect.center().y, text, ink);
        // The line under it is faint at rest and full under the pointer.
        let under = if visual.enabled {
            ink.gamma_multiply(0.45 + 0.55 * visual.hover)
        } else {
            ink.gamma_multiply(0.45)
        };
        let level = painter.round_to_pixel_center(rect.center().y + size * 0.56);
        painter.line_segment(
            [pos2(rect.left(), level), pos2(rect.left() + width, level)],
            Stroke::new(theme.border_width.max(1.0), under),
        );
        // The ring keeps clear of the text and of the line under it.
        focus::paint_focus_ring(
            painter,
            rect.expand2(vec2(theme.space(Space::S2), theme.space(Space::S1))),
            theme.radius(Radius::Sm),
            visual.focus,
            theme,
        );
    }
}

/// What a review sheet shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sheet {
    /// Every control in every state, painted as it would look.
    States,
    /// Every control at rest in the three densities, live.
    Densities,
}

impl Sheet {
    /// Both.
    pub const ALL: [Self; 2] = [Self::States, Self::Densities];

    /// Its name in file names: `states`, `densities`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::States => "states",
            Self::Densities => "densities",
        }
    }
}

/// The table a review sheet is laid out on: a title, a head per column
/// and a name per row. A sheet is a picture for reviewers and no part of
/// Sue's interface, so what it says is not in the language database.
pub(super) struct Table {
    /// The top left corner of the first cell.
    origin: Pos2,
    /// The size of a cell.
    cell: Vec2,
}

impl Table {
    /// The width of the column that names the rows.
    const NAMES: f32 = 168.0;

    /// The height of the title and the column heads.
    const HEAD: f32 = 84.0;

    /// Paints the ground, the `title` and the heads of `columns`, and
    /// makes room for `rows` rows.
    pub(super) fn new(
        painter: &Painter,
        rect: Rect,
        theme: &Theme,
        title: &str,
        columns: &[&str],
        rows: usize,
    ) -> Self {
        let card = rect.shrink(theme.space(Space::S6));
        painter.rect(
            card,
            theme.radius(Radius::Lg),
            theme.surface(1),
            theme.hairline(),
            StrokeKind::Inside,
        );
        let inner = card.shrink(theme.space(Space::S7));
        painter.text(
            inner.left_top(),
            egui::Align2::LEFT_TOP,
            title,
            theme.text_font(TextRole::Heading),
            theme.text,
        );
        let origin = pos2(inner.left() + Self::NAMES, inner.top() + Self::HEAD);
        let cell = vec2(
            (inner.width() - Self::NAMES) / columns.len().max(1) as f32,
            ((inner.height() - Self::HEAD) / rows.max(1) as f32).min(56.0),
        );
        for (column, head) in columns.iter().enumerate() {
            painter.text(
                pos2(
                    origin.x + cell.x * column as f32,
                    origin.y - theme.space(Space::S5),
                ),
                egui::Align2::LEFT_BOTTOM,
                head,
                theme.text_font(TextRole::Caption),
                theme.text_muted(),
            );
        }
        Self { origin, cell }
    }

    /// The cell at `row` and `column`.
    pub(super) fn cell(&self, row: usize, column: usize) -> Rect {
        Rect::from_min_size(
            self.origin + vec2(self.cell.x * column as f32, self.cell.y * row as f32),
            self.cell,
        )
    }

    /// The place of something `size` large in the cell at `row` and
    /// `column`: at its left, centered on its height.
    pub(super) fn place(&self, row: usize, column: usize, size: Vec2) -> Rect {
        let cell = self.cell(row, column);
        Rect::from_min_size(pos2(cell.left(), cell.center().y - size.y / 2.0), size)
    }

    /// Writes the name of `row`.
    pub(super) fn name(&self, painter: &Painter, theme: &Theme, row: usize, name: &str) {
        let cell = self.cell(row, 0);
        painter.text(
            pos2(cell.left() - Self::NAMES, cell.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            theme.text_font(TextRole::Caption),
            theme.text_muted(),
        );
    }
}

/// Lays `columns` live columns out over `rect`, one per density, with a
/// title: the ground of a [`Sheet::Densities`] page. `column` draws one.
pub(super) fn density_columns(
    painter: &Painter,
    rect: Rect,
    theme: &Theme,
    title: &str,
    mut column: impl FnMut(&mut Ui, Density),
) {
    let names = ["Compact", "Regular", "Touch"];
    let table = Table::new(painter, rect, theme, title, &names, 1);
    for (index, density) in Density::ALL.into_iter().enumerate() {
        let top = table.cell(0, index).left_top();
        let room = Rect::from_min_max(
            top,
            pos2(
                top.x + table.cell.x - theme.space(Space::S7),
                rect.bottom() - theme.space(Space::S9),
            ),
        );
        let mut ui = Ui::new(
            painter.ctx().clone(),
            Id::unique(("sue-ui-sheet", title, index)),
            UiBuilder::new().max_rect(room).layer_id(painter.layer_id()),
        );
        ui.spacing_mut().item_spacing = Vec2::splat(theme.space(Space::S5));
        density.scope(&mut ui, |ui| column(ui, density));
    }
}

/// Draws the review sheet `page` of the buttons over `rect`: what a
/// reviewer looks at to judge every button in every state. It is drawn
/// in `theme`, at the scale of the painter.
pub fn sheet(painter: &Painter, rect: Rect, theme: &Theme, page: Sheet) {
    match page {
        Sheet::States => states_sheet(painter, rect, theme),
        Sheet::Densities => {
            density_columns(painter, rect, theme, "Buttons", |ui, _| {
                live_column(ui, theme);
            });
        }
    }
}

/// One row of the states sheet: its name, and how to paint its control
/// at a place found from the control's size.
type StateRow<'a> = (&'a str, PaintCell<'a>);

/// Paints one control of the states sheet into the cell at a row and a
/// column of the table, in a state.
type PaintCell<'a> = Box<dyn Fn(&Table, usize, usize, Visual) + 'a>;

fn states_sheet(painter: &Painter, rect: Rect, theme: &Theme) {
    let density = Density::Regular;
    let labelled = |make: fn() -> Button<'static>| -> PaintCell<'_> {
        Box::new(move |table, row, column, visual| {
            let button = make();
            let place = table.place(row, column, button.size(painter, theme, density));
            button.paint(painter, place, theme, density, visual);
        })
    };
    let icon = |make: fn() -> IconButton<'static>| -> PaintCell<'_> {
        Box::new(move |table, row, column, visual| {
            let button = make();
            let place = table.place(row, column, button.size(density));
            button.paint(painter, place, theme, density, visual);
        })
    };
    let split = |make: fn() -> SplitButton<'static>, half: usize| -> PaintCell<'_> {
        Box::new(move |table, row, column, visual| {
            let button = make();
            let place = table.place(row, column, button.size(painter, theme, density));
            let rest = Visual {
                enabled: visual.enabled,
                ..Visual::REST
            };
            let mut visuals = [rest; 2];
            visuals[half] = visual;
            button.paint(painter, place, theme, density, visuals, 0.0);
        })
    };
    let rows: Vec<StateRow<'_>> = vec![
        ("Primary", labelled(|| button(0, "Save").primary())),
        ("Secondary", labelled(|| button(0, "Cancel"))),
        ("Ghost", labelled(|| button(0, "Skip").ghost())),
        ("Danger", labelled(|| button(0, "Delete").danger())),
        (
            "Primary with icon",
            labelled(|| button(0, "New picture").primary().icon(Icon::Picture)),
        ),
        (
            "Secondary with icon",
            labelled(|| button(0, "Open folder").icon(Icon::Folder)),
        ),
        (
            "Danger with icon",
            labelled(|| button(0, "Delete layer").danger().icon(Icon::Delete)),
        ),
        ("Toggle, off", labelled(|| toggle_button(0, "Snap", false))),
        ("Toggle, on", labelled(|| toggle_button(0, "Snap", true))),
        (
            "Icon button",
            icon(|| icon_button(0, Icon::Settings, "Settings")),
        ),
        (
            "Icon button, quiet",
            icon(|| icon_button(0, Icon::Search, "Search").quiet()),
        ),
        (
            "Icon toggle, on",
            icon(|| icon_button(0, Icon::Star, "Favorite").selected(true)),
        ),
        (
            "Icon button, framed",
            icon(|| icon_button(0, Icon::Folder, "Open").kind(ButtonKind::Secondary)),
        ),
        (
            "Split, label half",
            split(|| split_button(0, "Export", "More ways to export"), 0),
        ),
        (
            "Split primary, menu half",
            split(|| split_button(0, "Save", "More ways to save").primary(), 1),
        ),
        (
            "Link",
            Box::new(|table, row, column, visual| {
                let link = link(0, "Read the guide");
                let place = table.place(row, column, link.size(painter, theme));
                link.paint(painter, place, theme, visual);
            }),
        ),
    ];
    let heads = Visual::STATES.map(|(name, _)| name);
    let table = Table::new(painter, rect, theme, "Buttons", &heads, rows.len());
    for (row, (name, paint)) in rows.iter().enumerate() {
        table.name(painter, theme, row, name);
        for (column, (_, visual)) in Visual::STATES.into_iter().enumerate() {
            paint(&table, row, column, visual);
        }
    }
}

/// Every button, live and at rest, at the density of `ui`.
fn live_column(ui: &mut Ui, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        let _ = button("primary", "Save").primary().show(ui, theme);
        let _ = button("secondary", "Cancel").show(ui, theme);
        let _ = button("ghost", "Skip").ghost().show(ui, theme);
        let _ = button("danger", "Delete").danger().show(ui, theme);
    });
    ui.horizontal_wrapped(|ui| {
        let _ = button("new", "New picture")
            .primary()
            .icon(Icon::Picture)
            .show(ui, theme);
        let _ = button("open", "Open folder")
            .icon(Icon::Folder)
            .show(ui, theme);
    });
    ui.horizontal_wrapped(|ui| {
        let _ = toggle_button("snap", "Snap", true).show(ui, theme);
        let _ = toggle_button("grid", "Grid", false).show(ui, theme);
        let _ = button("off", "Not now").enabled(false).show(ui, theme);
    });
    ui.horizontal_wrapped(|ui| {
        let _ = icon_button("settings", Icon::Settings, "Settings").show(ui, theme);
        let _ = icon_button("search", Icon::Search, "Search")
            .quiet()
            .show(ui, theme);
        let _ = icon_button("star", Icon::Star, "Favorite")
            .selected(true)
            .show(ui, theme);
        let _ = icon_button("eye", Icon::Eye, "Show")
            .selected(false)
            .show(ui, theme);
        let _ = icon_button("folder", Icon::Folder, "Open")
            .kind(ButtonKind::Secondary)
            .show(ui, theme);
        let _ = icon_button("delete", Icon::Delete, "Delete")
            .enabled(false)
            .show(ui, theme);
    });
    ui.horizontal_wrapped(|ui| {
        let _ = split_button("export", "Export", "More ways to export").show(ui, theme);
        let _ = split_button("save", "Save", "More ways to save")
            .primary()
            .show(ui, theme);
    });
    ui.horizontal_wrapped(|ui| {
        let _ = link("guide", "Read the guide").show(ui, theme);
        let _ = link("notes", "Release notes")
            .role(TextRole::Caption)
            .show(ui, theme);
    });
    let _ = button("wide", "As wide as the column")
        .full_width()
        .show(ui, theme);
}

/// What the tests of the buttons and the choices drive them with.
#[cfg(test)]
pub(super) mod testkit {
    use egui::accesskit::{Node, Role as NodeRole, Toggled};
    use egui::{
        Context, Event, FullOutput, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Ui, pos2,
        vec2,
    };
    use sue_rice::RiceSchema;

    use crate::Theme;
    use crate::ui::{Density, Motion};

    /// What assistive technology is told about one control: its role, its
    /// name, whether it is on and whether it is disabled.
    pub type Told = (NodeRole, String, Option<Toggled>, bool);

    /// Sue's theme with the shipped defaults (Void).
    pub fn default_theme() -> Theme {
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        Theme::from_rice(&rice)
    }

    /// Sue's theme in each built-in preset.
    pub fn presets() -> Vec<(String, Theme)> {
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        crate::register_rice(&mut rice).unwrap();
        let sue = Theme::from_rice(&rice);
        sue_rice::builtin_presets()
            .iter()
            .map(|preset| (preset.id.clone(), sue.with_preset(preset)))
            .collect()
    }

    /// A key going down.
    pub fn key(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    /// A context that runs frames of controls, a sixtieth of a second
    /// apart, with assistive technology listening.
    pub struct Bench {
        pub ctx: Context,
        pub theme: Theme,
        time: f64,
    }

    impl Bench {
        /// A bench in Sue's default theme. With `instant`, every
        /// transition lands at once, so one frame shows a state whole.
        pub fn new(instant: bool) -> Self {
            let ctx = Context::default();
            let theme = default_theme();
            theme.apply(&ctx, None, None);
            Motion {
                instant,
                ..Motion::default()
            }
            .store(&ctx);
            ctx.enable_accesskit();
            Self {
                ctx,
                theme,
                time: 0.0,
            }
        }

        /// One frame after `events`, with `add` showing controls in a
        /// `Ui` of `density`. Returns what `add` returned and the frame.
        pub fn frame<R>(
            &mut self,
            density: Density,
            events: Vec<Event>,
            add: impl FnOnce(&mut Ui, &Theme) -> R,
        ) -> (R, FullOutput) {
            self.time += 1.0 / 60.0;
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                time: Some(self.time),
                predicted_dt: 1.0 / 60.0,
                events,
                ..Default::default()
            };
            let theme = self.theme.clone();
            let mut add = Some(add);
            let mut result = None;
            let mut output = self.ctx.run_ui(input, |ui| {
                if let Some(add) = add.take() {
                    result = Some(density.scope(ui, |ui| add(ui, &theme)));
                }
            });
            output.textures_delta.clear();
            (result.expect("the frame ran"), output)
        }

        /// The pointer moved to `at`, pressed and let go over two frames,
        /// with `add` showing the same controls in both. Returns what the
        /// second frame's `add` returned.
        pub fn click<R>(
            &mut self,
            at: Pos2,
            density: Density,
            mut add: impl FnMut(&mut Ui, &Theme) -> R,
        ) -> R {
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let _ = self.frame(
                density,
                vec![Event::PointerMoved(at), button(true)],
                &mut add,
            );
            self.frame(density, vec![button(false)], &mut add).0
        }

        /// The pointer moved far away from every control.
        pub fn away(&mut self) {
            let _ = self.frame(
                Density::Regular,
                vec![Event::PointerMoved(pos2(790.0, 590.0))],
                |_, _| (),
            );
        }
    }

    /// The controls a frame told assistive technology about, with a name.
    pub fn nodes(output: &FullOutput) -> Vec<Told> {
        output
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|update| update.nodes.iter())
            .filter_map(|(_, node): &(_, Node)| {
                node.label().map(|label| {
                    (
                        node.role(),
                        label.to_owned(),
                        node.toggled(),
                        node.is_disabled(),
                    )
                })
            })
            .collect()
    }

    /// What was told about the control named `label`.
    pub fn node(output: &FullOutput, label: &str) -> Told {
        nodes(output)
            .into_iter()
            .find(|node| node.1 == label)
            .unwrap_or_else(|| panic!("nothing is named {label:?}"))
    }

    /// The color `fill` makes over `surface`.
    pub fn over(surface: egui::Color32, fill: egui::Color32) -> egui::Color32 {
        surface.blend(fill)
    }

    /// Every rectangle a frame drew, with its fill and its outline.
    pub fn rects(output: &FullOutput) -> Vec<(Rect, egui::Color32, egui::Stroke)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(Rect, egui::Color32, egui::Stroke)>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, out)),
                egui::Shape::Rect(rect) => out.push((rect.rect, rect.fill, rect.stroke)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    /// Whether a frame drew a focus ring (a rectangle with no fill and the
    /// focus stroke's width) around all of `around`.
    pub fn ring_around(output: &FullOutput, around: Rect) -> bool {
        rects(output).into_iter().any(|(drawn, fill, stroke)| {
            fill == egui::Color32::TRANSPARENT
                && stroke.width == Theme::STRONG_STROKE
                && stroke.color.a() > 0
                && drawn.expand(0.5).contains_rect(around)
        })
    }

    /// Whether a frame asks for another one soon: something is moving.
    pub fn moving(output: &FullOutput) -> bool {
        output
            .viewport_output
            .values()
            .any(|viewport| viewport.repaint_delay < std::time::Duration::from_secs(1))
    }
}

#[cfg(test)]
mod tests {
    use egui::accesskit::{Role as NodeRole, Toggled};
    use egui::{FullOutput, Key, Ui, pos2};

    use super::testkit::{Bench, key, moving, node, over, presets, rects, ring_around};
    use super::*;

    /// The colors of a button whose fill lies over `surface`, settled in
    /// `visual`: (what is behind its text, its text).
    fn seen(
        kind: ButtonKind,
        on: bool,
        theme: &Theme,
        visual: Visual,
        surface: Color32,
    ) -> (Color32, Color32) {
        let skin = skin(kind, on, theme, visual);
        (over(surface, skin.fill), skin.ink)
    }

    #[test]
    fn every_button_reads_in_every_preset_and_state() {
        for (id, theme) in presets() {
            for kind in ButtonKind::ALL {
                for on in [false, true] {
                    for (state, visual) in Visual::STATES {
                        for level in 0..=2 {
                            let surface = theme.surface(level);
                            let (under, ink) = seen(kind, on, &theme, visual, surface);
                            let ratio = contrast(ink, under);
                            let least = if visual.enabled { 4.5 } else { 2.5 };
                            assert!(
                                ratio >= least,
                                "{id}: {kind:?} on={on} {state} on surface {level}: \
                                 {ratio:.2} to 1"
                            );
                        }
                    }
                }
            }
            // The ring around what holds focus stands out from every
            // surface it can lie on.
            for level in 0..=2 {
                assert!(contrast(theme.focus(), theme.surface(level)) >= 3.0, "{id}");
            }
        }
    }

    #[test]
    fn hover_and_press_each_look_different() {
        for (id, theme) in presets() {
            for kind in ButtonKind::ALL {
                for on in [false, true] {
                    let looks: Vec<(Color32, Color32)> = Visual::STATES[..3]
                        .iter()
                        .map(|(_, visual)| seen(kind, on, &theme, *visual, theme.surface(1)))
                        .collect();
                    assert_ne!(looks[0], looks[1], "{id}: {kind:?} on={on}: hover");
                    assert_ne!(looks[1], looks[2], "{id}: {kind:?} on={on}: press");
                }
                // A toggle that is on is told from one that is off at rest.
                assert_ne!(
                    seen(kind, true, &theme, Visual::REST, theme.surface(1)),
                    seen(kind, false, &theme, Visual::REST, theme.surface(1)),
                    "{id}: {kind:?}"
                );
            }
        }
    }

    #[test]
    fn a_label_never_fades_out_while_a_hover_comes_and_goes() {
        for (id, theme) in presets() {
            for kind in ButtonKind::ALL {
                for on in [false, true] {
                    for step in 0..=20 {
                        let visual = Visual {
                            hover: step as f32 / 20.0,
                            ..Visual::REST
                        };
                        for level in 0..=2 {
                            let (under, ink) = seen(kind, on, &theme, visual, theme.surface(level));
                            let ratio = contrast(ink, under);
                            assert!(
                                ratio >= 3.0,
                                "{id}: {kind:?} on={on} hover {:.2} on surface {level}:                                  {ratio:.2} to 1",
                                visual.hover
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn buttons_take_the_sizes_of_their_density() {
        let mut bench = Bench::new(true);
        for density in Density::ALL {
            let (shown, _) = bench.frame(density, Vec::new(), |ui, theme| {
                let room = ui.available_width();
                let shown = [
                    button("save", "Save").primary().show(ui, theme),
                    icon_button("settings", Icon::Settings, "Settings").show(ui, theme),
                    split_button("export", "Export", "More ways to export")
                        .show(ui, theme)
                        .main,
                    button("wide", "As wide as the room")
                        .full_width()
                        .show(ui, theme),
                ]
                .map(|response| (response.rect, response.interact_rect));
                (shown, room)
            });
            let ([plain, icon, split, wide], room) = shown;
            assert_eq!(plain.0.height(), density.control_height(), "{density:?}");
            assert_eq!(
                icon.0.size(),
                Vec2::splat(density.icon_button()),
                "{density:?}"
            );
            assert_eq!(split.0.height(), density.control_height(), "{density:?}");
            assert_eq!(wide.0.width(), room, "{density:?}: the room");
            // Nothing is harder to hit than the density allows.
            for (drawn, hit) in [plain, icon, split, wide] {
                assert!(hit.contains_rect(drawn), "{density:?}");
                assert!(
                    hit.width() >= density.min_hit() && hit.height() >= density.min_hit(),
                    "{density:?}: {hit:?}"
                );
            }
        }
    }

    #[test]
    fn a_long_label_is_cut_short_and_never_spills() {
        let mut bench = Bench::new(true);
        let label = "A label far longer than the room the caller left for it";
        let (_, output) = bench.frame(Density::Regular, Vec::new(), |ui, theme| {
            let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(120.0, 30.0));
            button("long", label).show_at(ui, rect, theme)
        });
        let mut widest = 0.0_f32;
        for clipped in &output.shapes {
            if let egui::Shape::Text(text) = &clipped.shape {
                widest = widest.max(text.pos.x + text.galley.size().x);
            }
        }
        assert!(widest > 0.0 && widest <= 130.0, "the text ends at {widest}");
        // The whole label is still what is read out.
        assert_eq!(node(&output, label).0, NodeRole::Button);
    }

    /// The parts the click test clicks, top to bottom.
    fn column(ui: &mut Ui, theme: &Theme) -> Vec<Response> {
        let mut shown = vec![
            button("save", "Save").primary().show(ui, theme),
            button("off", "Not now").enabled(false).show(ui, theme),
        ];
        let split = split_button("export", "Export", "More ways to export").show(ui, theme);
        shown.push(split.main);
        shown.push(split.menu);
        shown.push(icon_button("star", Icon::Star, "Favorite").show(ui, theme));
        shown.push(link("guide", "Read the guide").show(ui, theme));
        shown
    }

    #[test]
    fn a_click_lands_on_the_part_under_the_pointer() {
        let mut bench = Bench::new(true);
        let (shown, _) = bench.frame(Density::Regular, Vec::new(), column);
        let places: Vec<Rect> = shown.iter().map(|response| response.rect).collect();
        for (index, place) in places.iter().enumerate() {
            let clicked = bench.click(place.center(), Density::Regular, |ui, theme| {
                column(ui, theme)
                    .iter()
                    .map(Response::clicked)
                    .collect::<Vec<_>>()
            });
            // The disabled button takes no click; every other part takes
            // its own and no other's.
            let expected: Vec<bool> = (0..places.len())
                .map(|at| at == index && index != 1)
                .collect();
            assert_eq!(clicked, expected, "a click on part {index}");
        }
    }

    #[test]
    fn a_control_is_named_by_its_salt_not_by_its_place() {
        let mut bench = Bench::new(true);
        let ids = |bench: &mut Bench, before: usize| {
            bench
                .frame(Density::Regular, Vec::new(), |ui, theme| {
                    for line in 0..before {
                        ui.label(format!("line {line}"));
                    }
                    [
                        button("a", "Same").show(ui, theme).id,
                        button("b", "Same").show(ui, theme).id,
                        icon_button("c", Icon::Star, "Star").show(ui, theme).id,
                    ]
                })
                .0
        };
        let first = ids(&mut bench, 0);
        let moved = ids(&mut bench, 3);
        assert_eq!(first, moved, "moving down keeps every id");
        assert_ne!(first[0], first[1], "two salts, two controls");
    }

    #[test]
    fn buttons_tell_assistive_technology_what_they_are() {
        let mut bench = Bench::new(true);
        let (_, output) = bench.frame(Density::Regular, Vec::new(), |ui, theme| {
            let _ = button("save", "Save").primary().show(ui, theme);
            let _ = toggle_button("snap", "Snap", true).show(ui, theme);
            let _ = toggle_button("grid", "Grid", false).show(ui, theme);
            let _ = button("off", "Not now").enabled(false).show(ui, theme);
            let _ = icon_button("star", Icon::Star, "Favorite")
                .selected(true)
                .show(ui, theme);
            let _ = split_button("export", "Export", "More ways to export").show(ui, theme);
            let _ = link("guide", "Read the guide").show(ui, theme);
        });
        assert_eq!(
            node(&output, "Save"),
            (NodeRole::Button, "Save".to_owned(), None, false)
        );
        assert_eq!(node(&output, "Snap").2, Some(Toggled::True));
        assert_eq!(node(&output, "Grid").2, Some(Toggled::False));
        assert!(node(&output, "Not now").3, "it says it is disabled");
        assert_eq!(node(&output, "Favorite").2, Some(Toggled::True));
        assert_eq!(node(&output, "Export").0, NodeRole::Button);
        assert_eq!(node(&output, "More ways to export").0, NodeRole::Button);
        assert_eq!(node(&output, "Read the guide").0, NodeRole::Link);
    }

    #[test]
    fn the_focus_ring_follows_the_keyboard_and_goes_with_a_click() {
        let mut bench = Bench::new(true);
        let show = |ui: &mut Ui, theme: &Theme| button("save", "Save").show(ui, theme);
        let (response, _) = bench.frame(Density::Regular, Vec::new(), show);
        let place = response.rect;
        // Tab brings focus, and the ring with it.
        let _ = bench.frame(Density::Regular, vec![key(Key::Tab)], show);
        let (response, output) = bench.frame(Density::Regular, Vec::new(), show);
        assert!(response.has_focus(), "Tab gives it focus");
        assert!(ring_around(&output, place), "a ring after Tab");
        // Enter presses the focused button.
        let (pressed, _) = bench.frame(Density::Regular, vec![key(Key::Enter)], show);
        assert!(pressed.clicked(), "Enter presses it");
        // A click elsewhere: the ring goes.
        bench.click(pos2(700.0, 500.0), Density::Regular, |ui, theme| {
            show(ui, theme)
        });
        let (_, output) = bench.frame(Density::Regular, Vec::new(), show);
        assert!(!ring_around(&output, place), "no ring after a click");
        // A disabled button never takes focus.
        let off = |ui: &mut Ui, theme: &Theme| button("off", "Off").enabled(false).show(ui, theme);
        for _ in 0..3 {
            let (response, _) = bench.frame(Density::Regular, vec![key(Key::Tab)], off);
            assert!(!response.has_focus());
        }
    }

    #[test]
    fn buttons_at_rest_ask_for_no_frames() {
        let mut bench = Bench::new(false);
        let all = |ui: &mut Ui, theme: &Theme| {
            [
                button("save", "Save").primary().show(ui, theme).rect,
                split_button("export", "Export", "More")
                    .show(ui, theme)
                    .rect,
                icon_button("star", Icon::Star, "Star").show(ui, theme).rect,
                link("guide", "Guide").show(ui, theme).rect,
            ]
        };
        let settle = |bench: &mut Bench| {
            (0..60).any(|_| !moving(&bench.frame(Density::Regular, Vec::new(), all).1))
        };
        assert!(settle(&mut bench), "the buttons come to rest");
        // Under the pointer a fill fades in, then rests again.
        let (places, _) = bench.frame(Density::Regular, Vec::new(), all);
        for place in places {
            let (_, output) = bench.frame(
                Density::Regular,
                vec![egui::Event::PointerMoved(place.center())],
                all,
            );
            assert!(moving(&output), "a hover fades in at {place:?}");
            assert!(settle(&mut bench), "and then rests");
        }
        bench.away();
        assert!(settle(&mut bench), "and fades out to rest");
    }

    #[test]
    fn every_kind_changes_when_pressed_and_an_open_menu_half_stays_down() {
        let theme = super::testkit::default_theme();
        let pressed = Visual {
            hover: 1.0,
            pressed: true,
            ..Visual::REST
        };
        for kind in ButtonKind::ALL {
            let up = skin(kind, false, &theme, Visual::REST);
            let down = skin(kind, false, &theme, pressed);
            assert_ne!(up, down, "{kind:?}");
        }
        // The open menu half is drawn held down with the pointer elsewhere.
        let mut bench = Bench::new(true);
        let (halves, output): ((Rect, Rect), FullOutput) =
            bench.frame(Density::Regular, Vec::new(), |ui, theme| {
                let split = split_button("export", "Export", "More")
                    .open(true)
                    .show(ui, theme);
                (split.main.rect, split.menu.rect)
            });
        let drawn = rects(&output);
        let fill_of = |place: Rect| {
            drawn
                .iter()
                .find(|(rect, fill, _)| *rect == place && *fill != Color32::TRANSPARENT)
                .map(|(_, fill, _)| *fill)
        };
        assert_eq!(fill_of(halves.1), Some(bench.theme.fill_pressed()));
        assert_ne!(fill_of(halves.0), fill_of(halves.1));
    }
}
