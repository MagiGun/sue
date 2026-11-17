//! Dialogs, sheets and forms: the one way Sue asks for something.
//!
//! - [`Dialog`] is the host of every dialog: a title, the caller's body
//!   and a footer whose buttons always stand in the same order. It dims
//!   what is behind it, keeps keyboard focus inside, answers Escape and
//!   Enter, and is drawn either inside the window ([`DialogHost::Embedded`])
//!   or as a window of its own with Sue's frame ([`DialogHost::Window`]).
//! - [`confirm`] is a dialog that asks one question.
//! - [`Sheet`] rises from the bottom edge and is dragged down to send it
//!   away: the dialog of phones and tablets.
//! - [`form_grid`] and [`form_row`] lay out labelled controls.
//!
//! The footer's order, from the leading edge: an extra action on its own
//! (Reset, Help), then, together at the trailing edge, the button that
//! accepts, an alternate answer (Do Not Save), and Cancel last. OK and
//! Cancel are read from the language database.
//!
//! Enter accepts while no control holds keyboard focus, and so does
//! Enter in a one-line text field, which gives focus up. A focused
//! button, check box or slider takes Enter itself. Escape cancels unless
//! a menu or a dropdown list is open, which Escape closes first.

use egui::{
    Align2, Area, Color32, Context, CornerRadius, Frame, Id, InnerResponse, Key, Margin, Modifiers,
    Rect, Response, Role, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder, UiKind,
    ViewportBuilder, ViewportCommand, ViewportId, WidgetInfo, pos2, vec2,
};

use super::focus::focus_ring;
use super::motion::{self, Motion, Pace};
use super::toast::{Tone, close_button};
use super::tokens::{Density, Elevation, Layer, Radius, Space, TextRole, contrast};
use crate::{SpringDamper, Theme};

/// How wide a dialog is when it does not say, in points.
const WIDTH: f32 = 440.0;

/// How wide a sheet gets on a wide screen, in points.
const SHEET_WIDTH: f32 = 560.0;

/// The share of its height a sheet has to be dragged down to leave.
const SHEET_LEAVES: f32 = 0.25;

/// A flick down at this speed (points a second) sends a sheet away too.
const SHEET_FLICK: f32 = 700.0;

/// Where a dialog is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DialogHost {
    /// Inside the window, over a scrim.
    #[default]
    Embedded,
    /// In a window of its own, which can be moved anywhere. Where the
    /// system gives Sue one window only (phones, the browser) it is
    /// drawn embedded.
    Window,
}

/// What a dialog was answered with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Answer {
    /// The button that accepts (OK, Save), or Enter.
    Accept,
    /// The other answer (Do Not Save).
    Alternate,
    /// Cancel, the close button, Escape, or closing the window.
    Cancel,
    /// The extra action at the leading edge (Reset).
    Extra,
}

impl Answer {
    /// The answers in the order their buttons stand, from the leading
    /// edge to the trailing one.
    pub const ORDER: [Self; 4] = [Self::Extra, Self::Accept, Self::Alternate, Self::Cancel];
}

/// What showing a dialog gave.
#[derive(Clone, Debug)]
pub struct DialogResponse<R> {
    /// What the body returned.
    pub inner: R,
    /// The answer, in the frame it is given.
    pub answer: Option<Answer>,
    /// Where the dialog was drawn, in the window it was drawn in.
    pub rect: Rect,
    /// Where its footer buttons were drawn.
    buttons: Vec<(Answer, Rect)>,
}

impl<R> DialogResponse<R> {
    /// Where the button of `answer` was drawn, when the dialog has it.
    #[must_use]
    pub fn button_rect(&self, answer: Answer) -> Option<Rect> {
        self.buttons
            .iter()
            .find(|(of, _)| *of == answer)
            .map(|(_, rect)| *rect)
    }
}

/// How a footer button is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The button that accepts: filled with the text color.
    Primary,
    /// The button that accepts in a dialog that destroys: filled with
    /// the danger color (the text color in the shipped themes).
    Danger,
    /// Every other button: a quiet fill and an outline.
    Secondary,
}

/// A dialog. Build it, then [`show`](Self::show) it every frame while it
/// is open.
#[derive(Clone, Debug)]
#[must_use = "a dialog does nothing until it is shown"]
pub struct Dialog<'a> {
    id: Id,
    title: &'a str,
    theme: &'a Theme,
    host: DialogHost,
    width: f32,
    height: Option<f32>,
    resizable: bool,
    accept: Option<String>,
    accept_enabled: bool,
    alternate: Option<String>,
    cancel: Option<String>,
    extra: Option<String>,
    danger: bool,
    close_button: bool,
    modal: bool,
    scrim_cancels: bool,
    enter_accepts: bool,
}

/// What the parts of a dialog gave in a frame.
struct Parts<R> {
    inner: R,
    answer: Option<Answer>,
    buttons: Vec<(Answer, Rect)>,
    /// How tall the body's content was.
    body_height: f32,
}

impl<'a> Dialog<'a> {
    /// A dialog named by `salt` (which has to differ from every other
    /// dialog's on screen) with `title`. It has a close button and no
    /// footer buttons yet.
    pub fn new(salt: impl egui::AsIdSalt, title: &'a str, theme: &'a Theme) -> Self {
        Self {
            id: Id::unique("sue-dialog").with(salt),
            title,
            theme,
            host: DialogHost::Embedded,
            width: WIDTH,
            height: None,
            resizable: false,
            accept: None,
            accept_enabled: true,
            alternate: None,
            cancel: None,
            extra: None,
            danger: false,
            close_button: true,
            modal: true,
            scrim_cancels: false,
            enter_accepts: true,
        }
    }

    /// Where it is drawn.
    pub fn host(mut self, host: DialogHost) -> Self {
        self.host = host;
        self
    }

    /// How wide it is, in points. It is never wider than the window.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Gives the body exactly `height` points. Without it the dialog is
    /// as tall as its body, up to the window's height, and scrolls past
    /// that.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Lets a dialog in a window of its own be resized; the body takes
    /// the room. [`height`](Self::height) is then its first height.
    pub fn resizable(mut self) -> Self {
        self.resizable = true;
        self
    }

    /// The button that accepts, labelled `label`.
    pub fn accept(mut self, label: impl Into<String>) -> Self {
        self.accept = Some(label.into());
        self
    }

    /// Whether the button that accepts can be used now (it can unless
    /// this says otherwise).
    pub fn accept_enabled(mut self, enabled: bool) -> Self {
        self.accept_enabled = enabled;
        self
    }

    /// The button that accepts, as OK.
    pub fn ok(self) -> Self {
        self.accept(tr!("ok"))
    }

    /// The Cancel button.
    pub fn cancel(self) -> Self {
        self.cancel_as(tr!("cancel"))
    }

    /// The button that cancels, labelled `label` (Close, Not Now).
    pub fn cancel_as(mut self, label: impl Into<String>) -> Self {
        self.cancel = Some(label.into());
        self
    }

    /// OK and Cancel.
    pub fn ok_cancel(self) -> Self {
        self.ok().cancel()
    }

    /// The other answer, labelled `label`.
    pub fn alternate(mut self, label: impl Into<String>) -> Self {
        self.alternate = Some(label.into());
        self
    }

    /// An action on its own at the leading edge, labelled `label`.
    pub fn extra(mut self, label: impl Into<String>) -> Self {
        self.extra = Some(label.into());
        self
    }

    /// Says that accepting destroys something: focus starts on Cancel,
    /// and Enter accepts only once the accepting button holds focus.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// Leaves the close button out (the footer has to offer a way out).
    pub fn no_close_button(mut self) -> Self {
        self.close_button = false;
        self
    }

    /// Lets what is behind the dialog be used while it is open: no
    /// scrim, and focus is free to leave.
    pub fn modeless(mut self) -> Self {
        self.modal = false;
        self
    }

    /// Lets a click on the scrim cancel it. Right for a dialog that
    /// holds nothing a slip of the hand would lose.
    pub fn scrim_cancels(mut self) -> Self {
        self.scrim_cancels = true;
        self
    }

    /// Keeps Enter from accepting: for a body that needs Enter itself.
    pub fn enter_is_the_bodys(mut self) -> Self {
        self.enter_accepts = false;
        self
    }

    /// The viewport of this dialog when it is a window of its own.
    #[must_use]
    pub fn viewport(&self) -> ViewportId {
        ViewportId::from_hash_of(self.id)
    }

    /// Draws the dialog for this frame.
    pub fn show<R>(self, ctx: &Context, body: impl FnOnce(&mut Ui) -> R) -> DialogResponse<R> {
        if self.host == DialogHost::Window && !ctx.embed_viewports() {
            self.show_window(ctx, body)
        } else {
            self.show_embedded(ctx, body)
        }
    }

    /// The fill of a dialog: bars and panels.
    fn fill(&self) -> Color32 {
        self.theme.surface(1)
    }

    /// `true` when it has any footer button.
    fn has_footer(&self) -> bool {
        self.accept.is_some()
            || self.alternate.is_some()
            || self.cancel.is_some()
            || self.extra.is_some()
    }

    /// The label of the button of `answer`.
    fn label(&self, answer: Answer) -> Option<&str> {
        match answer {
            Answer::Accept => self.accept.as_deref(),
            Answer::Alternate => self.alternate.as_deref(),
            Answer::Cancel => self.cancel.as_deref(),
            Answer::Extra => self.extra.as_deref(),
        }
    }

    /// The id of the button of `answer`.
    fn button_id(&self, answer: Answer) -> Id {
        self.id.with(("sue-dialog-button", answer))
    }

    /// The id of the close button.
    fn close_id(&self) -> Id {
        self.id.with("sue-dialog-close")
    }

    /// The height of everything around the body: the header (when it
    /// has one, else the top margin), the gap above the footer, the
    /// footer and the bottom margin.
    fn chrome(&self, density: Density, header: bool) -> f32 {
        let theme = self.theme;
        let top = if header {
            theme.space(Space::S4) + density.icon_button() + theme.space(Space::S4)
        } else {
            theme.dialog_margin()
        };
        let footer = if self.has_footer() {
            theme.space(Space::S6) + density.control_height()
        } else {
            0.0
        };
        top + footer + theme.dialog_margin()
    }

    /// The dialog inside the window: a scrim and a panel in its middle.
    fn show_embedded<R>(self, ctx: &Context, body: impl FnOnce(&mut Ui) -> R) -> DialogResponse<R> {
        let theme = self.theme;
        let screen = ctx.content_rect();
        let (appeared, shown) = entered(ctx, self.id, Pace::Slow);
        let rise = (1.0 - shown) * theme.space(Space::S4);
        // Once it has come in it keeps its top where it stood: a body that
        // grows or shrinks (a list, a row shown or hidden) moves only its
        // bottom, never the controls under the pointer. Only when it would
        // run past the screen's bottom does it rise.
        let kept = self.id.with("sue-dialog-kept");
        if appeared {
            ctx.data_mut(|data| data.remove::<Rect>(kept));
        }
        let placed: Option<Rect> = ctx.data(|data| data.get_temp(kept));
        let area = Area::new(self.id)
            .kind(UiKind::Modal)
            .order(Layer::Dialog.order());
        let area = match placed.filter(|_| shown >= 1.0) {
            Some(last) => {
                let room = theme.space(Space::S6);
                let top = last
                    .top()
                    .min(screen.bottom() - room - last.height())
                    .max(screen.top() + room);
                area.fixed_pos(pos2(last.left(), top))
            }
            None => area.anchor(Align2::CENTER_CENTER, vec2(0.0, rise)),
        };
        let area = area
            .sense(Sense::hover())
            .interactable(true)
            .fade_in(false)
            .accessible_name(self.title);
        let top = if self.modal {
            ctx.memory_mut(|memory| {
                memory.set_modal_layer(area.layer());
                memory.top_modal_layer() == Some(area.layer())
            }) || appeared
        } else {
            true
        };
        let popup_open = egui::Popup::is_any_open(ctx);
        let margin = theme.space(Space::S6);
        let width = self.width.min(screen.width() - margin * 2.0).max(margin);
        let density = touch_or(ctx, Density::Regular);
        let room = screen.height() - margin * 2.0 - self.chrome(density, true);
        let frame = Frame::new()
            .fill(self.fill())
            .stroke(theme.hairline())
            .corner_radius(theme.corner(Radius::Xl))
            .shadow(theme.elevation(Elevation::Modal));
        let shown_area = area.show(ctx, |ui| {
            let mut scrim_clicked = false;
            if self.modal {
                let mut scrim = ui.new_child(
                    UiBuilder::new()
                        .sense(Sense::CLICK | Sense::DRAG)
                        .max_rect(screen),
                );
                scrim.set_min_size(screen.size());
                ui.painter()
                    .rect_filled(screen, 0.0, theme.scrim().gamma_multiply(shown));
                scrim_clicked = scrim.response().clicked();
            }
            // The panel takes the clicks on it, so they do not reach the
            // scrim.
            let panel =
                ui.scope_builder(UiBuilder::new().sense(Sense::CLICK | Sense::DRAG), |ui| {
                    ui.set_opacity(shown);
                    frame
                        .show(ui, |ui| {
                            // The outline is part of the width asked for.
                            ui.set_width(width - frame.stroke.width * 2.0);
                            let height = self.height.map_or(room, |height| height.min(room));
                            self.parts(ui, true, height, self.height.is_some(), body)
                        })
                        .inner
                });
            let mut parts = panel.inner;
            if scrim_clicked && self.scrim_cancels && parts.answer.is_none() {
                parts.answer = Some(Answer::Cancel);
            }
            self.keys(ui.ctx(), top && !popup_open, &mut parts.answer);
            if appeared {
                self.first_focus(ui);
            }
            (parts, panel.response.rect)
        });
        let (parts, rect) = shown_area.inner;
        if shown >= 1.0 {
            ctx.data_mut(|data| data.insert_temp(kept, rect));
        }
        DialogResponse {
            inner: parts.inner,
            answer: parts.answer,
            rect,
            buttons: parts.buttons,
        }
    }

    /// The dialog as a window of its own, with Sue's title bar where Sue
    /// draws the frames.
    fn show_window<R>(self, ctx: &Context, body: impl FnOnce(&mut Ui) -> R) -> DialogResponse<R> {
        let theme = self.theme;
        let viewport = self.viewport();
        if self.modal {
            self.block_parent(ctx, viewport);
        }
        let own_frame = crate::widgets::own_frames(ctx);
        let bar = if own_frame {
            crate::widgets::TITLE_BAR
        } else {
            0.0
        };
        let density = touch_or(ctx, Density::Regular);
        let chrome = bar + self.chrome(density, false);
        // What is remembered of the window: where it was first put, and
        // how tall its body turned out.
        let memory = self.id.with("sue-dialog-window");
        let known: Option<Placed> = ctx.data(|data| data.get_temp(memory));
        // Never taller than the screen it opens on: the body scrolls past
        // that.
        let tallest = ctx
            .input(|input| input.viewport().monitor_size)
            .map_or(f32::INFINITY, |screen| {
                (screen.y * 0.85 - chrome).max(theme.space(Space::S9) * 2.0)
            });
        let body_height = self
            .height
            .or(known.map(|known| known.body_height))
            .unwrap_or(theme.space(Space::S9) * 3.0)
            .min(tallest);
        let size = vec2(self.width, chrome + body_height);
        let parent = ctx
            .input(|input| input.viewport().outer_rect)
            .unwrap_or_else(|| ctx.content_rect());
        let at = known.map_or_else(|| parent.center() - size / 2.0, |known| known.at);
        let mut builder = ViewportBuilder::default()
            .with_title(self.title)
            .with_position(at)
            .with_resizable(self.resizable)
            .with_decorations(!own_frame);
        // A window that can be resized is sized once and then left to
        // whoever resizes it.
        if !self.resizable || known.is_none() {
            builder = builder.with_inner_size(size);
        }
        if self.resizable {
            builder = builder.with_min_inner_size(vec2(
                theme.space(Space::S9) * 6.0,
                chrome + theme.space(Space::S9) * 2.0,
            ));
        }
        let appeared = known.is_none();
        let mut body = Some(body);
        let mut out = None;
        ctx.show_viewport_immediate(viewport, builder, |ui, _| {
            let Some(body) = body.take() else {
                return;
            };
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, self.fill());
            let mut closed = false;
            if own_frame {
                let top = Rect::from_min_size(rect.min, vec2(rect.width(), bar));
                closed = crate::widgets::title_bar(ui, top, self.title, false, theme);
                ui.add_space(bar);
            }
            let height = if self.resizable {
                (rect.height() - chrome).max(0.0)
            } else {
                body_height
            };
            let fixed = self.resizable || self.height.is_some();
            let popup_open = egui::Popup::is_any_open(ui.ctx());
            let mut parts = self.parts(ui, false, height, fixed, body);
            if own_frame && self.resizable {
                crate::widgets::resize_edges(ui, rect);
            }
            closed |= ui.input(|input| input.viewport().close_requested());
            if closed && parts.answer.is_none() {
                parts.answer = Some(Answer::Cancel);
            }
            self.keys(ui.ctx(), !popup_open, &mut parts.answer);
            if appeared {
                self.first_focus(ui);
            }
            out = Some((parts, rect));
        });
        let (parts, rect) = out.expect("the window's contents are drawn in the call");
        // The first frame did not know how tall the body is: the window
        // is put again around the parent's middle at the size it turned
        // out to have, so a tall dialog does not hang below it. From
        // then on it stays where it is, or where it was moved to.
        let at = if known.is_none() && self.height.is_none() {
            let size = vec2(self.width, chrome + parts.body_height.min(tallest));
            parent.center() - size / 2.0
        } else {
            at
        };
        let placed = Placed {
            at,
            body_height: parts.body_height,
        };
        if parts.answer.is_some() {
            // Answered: the next time it opens, it is placed afresh.
            ctx.data_mut(|data| data.remove::<Placed>(memory));
        } else {
            if known.is_some_and(|known| known.body_height != placed.body_height) {
                // Its body grew or shrank: the window follows.
                ctx.request_repaint();
            }
            ctx.data_mut(|data| data.insert_temp(memory, placed));
        }
        DialogResponse {
            inner: parts.inner,
            answer: parts.answer,
            rect,
            buttons: parts.buttons,
        }
    }

    /// Dims the window a dialog of its own window belongs to and takes
    /// its input; a click on it brings the dialog to the front.
    fn block_parent(&self, ctx: &Context, viewport: ViewportId) {
        let screen = ctx.content_rect();
        let area = Area::new(self.id.with("sue-dialog-parent"))
            .kind(UiKind::Modal)
            .order(Layer::Dialog.order())
            .fixed_pos(screen.min)
            .constrain(false)
            .sense(Sense::hover())
            .fade_in(false);
        ctx.memory_mut(|memory| memory.set_modal_layer(area.layer()));
        area.show(ctx, |ui| {
            let response = ui.allocate_rect(screen, Sense::CLICK | Sense::DRAG);
            ui.painter().rect_filled(screen, 0.0, self.theme.scrim());
            if response.clicked() || response.drag_started() {
                ctx.send_viewport_cmd_to(viewport, ViewportCommand::Focus);
            }
        });
    }

    /// The header (when asked for), the body and the footer, top down in
    /// `ui`. The body gets up to `height` points, or exactly that when
    /// `fixed`.
    fn parts<R>(
        &self,
        ui: &mut Ui,
        header: bool,
        height: f32,
        fixed: bool,
        body: impl FnOnce(&mut Ui) -> R,
    ) -> Parts<R> {
        let theme = self.theme;
        let regular = touch_or(ui.ctx(), Density::Regular);
        regular.scope(ui, |ui| {
            let density = Density::of(ui);
            let margin = theme.dialog_margin();
            let width = ui.available_width();
            let mut answer = None;
            ui.spacing_mut().item_spacing = vec2(theme.space(Space::S4), 0.0);
            if header {
                let pad = theme.space(Space::S4);
                let (_, row) = ui.allocate_space(vec2(width, pad + density.icon_button() + pad));
                let row = row.shrink2(vec2(0.0, pad));
                let close = Rect::from_min_size(
                    pos2(row.right() - pad - row.height(), row.top()),
                    vec2(row.height(), row.height()),
                );
                let room = if self.close_button {
                    close.left() - pad
                } else {
                    row.right() - margin
                } - (row.left() + margin);
                let title = one_line(ui, self.title, TextRole::Title, theme.text, room, theme);
                ui.painter().galley(
                    pos2(row.left() + margin, row.center().y - title.size().y / 2.0),
                    title,
                    theme.text,
                );
                if self.close_button
                    && close_button(ui, self.close_id(), close, &tr!("dialog-close"), theme)
                        .clicked()
                {
                    answer = Some(Answer::Cancel);
                }
            } else {
                ui.add_space(margin);
            }
            let mut body_height = 0.0;
            let inner = Frame::new()
                .inner_margin(Margin::symmetric(margin.round().clamp(0.0, 127.0) as i8, 0))
                .show(ui, |ui| {
                    let mut scroll = ScrollArea::vertical()
                        .id_salt(self.id.with("sue-dialog-body"))
                        .max_height(height.max(0.0));
                    scroll = if fixed {
                        scroll
                            .auto_shrink([false, false])
                            .min_scrolled_height(height.max(0.0))
                    } else {
                        scroll.auto_shrink([false, true])
                    };
                    let shown = scroll.show(ui, |ui| {
                        ui.spacing_mut().item_spacing =
                            vec2(theme.space(Space::S4), theme.form_row_gap());
                        body(ui)
                    });
                    body_height = shown.content_size.y;
                    shown.inner
                })
                .inner;
            let mut buttons = Vec::new();
            if self.has_footer() {
                ui.add_space(theme.space(Space::S6));
                let footer = Frame::new()
                    .inner_margin(Margin::symmetric(margin.round().clamp(0.0, 127.0) as i8, 0))
                    .show(ui, |ui| self.footer(ui, density))
                    .inner;
                answer = answer.or(footer.0);
                buttons = footer.1;
            }
            ui.add_space(margin);
            Parts {
                inner,
                answer,
                buttons,
                body_height,
            }
        })
    }

    /// The footer's buttons, in their one order. Returns the button
    /// pressed and where each was drawn.
    fn footer(&self, ui: &mut Ui, density: Density) -> (Option<Answer>, Vec<(Answer, Rect)>) {
        let theme = self.theme;
        let height = density.control_height();
        let gap = theme.space(Space::S4);
        let font = theme.text_font(TextRole::Body);
        let labels: Vec<(Answer, &str, f32)> = Answer::ORDER
            .into_iter()
            .filter_map(|answer| {
                let label = self.label(answer)?;
                let text = ui
                    .painter()
                    .layout_no_wrap(label.to_owned(), font.clone(), theme.text);
                let width = (text.size().x + theme.space(Space::S6) * 2.0)
                    .max(theme.space(Space::S9) * 2.0);
                Some((answer, label, width))
            })
            .collect();
        let room = ui.available_width();
        let needed: f32 = labels.iter().map(|(_, _, width)| *width).sum::<f32>()
            + gap * labels.len().saturating_sub(1) as f32;
        let mut places = Vec::with_capacity(labels.len());
        if needed <= room {
            // One row: the extra action at the leading edge, the others
            // together at the trailing one.
            let (_, row) = ui.allocate_space(vec2(room, height));
            let mut right = row.right();
            for (answer, _, width) in labels.iter().rev() {
                if *answer == Answer::Extra {
                    places.push(Rect::from_min_size(row.min, vec2(*width, height)));
                } else {
                    places.push(Rect::from_min_size(
                        pos2(right - width, row.top()),
                        vec2(*width, height),
                    ));
                    right -= width + gap;
                }
            }
            places.reverse();
        } else {
            // Too narrow for a row: one under the other, the accepting
            // button first and the extra action last.
            let count = labels.len() as f32;
            let (_, column) = ui.allocate_space(vec2(room, height * count + gap * (count - 1.0)));
            let mut top = column.top();
            let mut rows = vec![Rect::NOTHING; labels.len()];
            let extra_last = labels
                .iter()
                .enumerate()
                .filter(|(_, (answer, ..))| *answer != Answer::Extra)
                .chain(
                    labels
                        .iter()
                        .enumerate()
                        .filter(|(_, (answer, ..))| *answer == Answer::Extra),
                );
            for (index, _) in extra_last {
                rows[index] = Rect::from_min_size(pos2(column.left(), top), vec2(room, height));
                top += height + gap;
            }
            places = rows;
        }
        let mut pressed = None;
        let mut drawn = Vec::with_capacity(labels.len());
        for ((answer, label, _), place) in labels.iter().zip(places) {
            let (kind, enabled) = match answer {
                Answer::Accept if self.danger => (Kind::Danger, self.accept_enabled),
                Answer::Accept => (Kind::Primary, self.accept_enabled),
                _ => (Kind::Secondary, true),
            };
            let button = action_button(
                ui,
                self.button_id(*answer),
                place,
                label,
                kind,
                enabled,
                theme,
            );
            if button.clicked() && enabled {
                pressed = Some(*answer);
            }
            drawn.push((*answer, place));
        }
        (pressed, drawn)
    }

    /// Escape and Enter, when `listening` and nothing answered yet.
    fn keys(&self, ctx: &Context, listening: bool, answer: &mut Option<Answer>) {
        if answer.is_some() || !listening {
            return;
        }
        let pressed = |key: Key| ctx.input_mut(|input| input.consume_key(Modifiers::NONE, key));
        if pressed(Key::Escape) {
            *answer = Some(Answer::Cancel);
            return;
        }
        // A control that holds focus takes Enter itself; a one-line text
        // field has given focus up by now.
        let accepts = self.enter_accepts
            && !self.danger
            && self.accept.is_some()
            && self.accept_enabled
            && ctx.memory(|memory| memory.focused()).is_none();
        if accepts && pressed(Key::Enter) {
            *answer = Some(Answer::Accept);
        }
    }

    /// Gives keyboard focus to the dialog on the frame it appears, unless
    /// its body took it: to the button that accepts, or to Cancel when
    /// accepting destroys something.
    fn first_focus(&self, ui: &Ui) {
        let ctx = ui.ctx();
        let layer = ui.layer_id();
        let inside = ctx
            .memory(|memory| memory.focused())
            .and_then(|id| ctx.read_response(id))
            .is_some_and(|response| response.layer_id == layer);
        if inside {
            return;
        }
        let accept = (self.accept.is_some() && self.accept_enabled).then_some(Answer::Accept);
        let cancel = self.cancel.is_some().then_some(Answer::Cancel);
        let first = if self.danger {
            cancel.or(accept)
        } else {
            accept.or(cancel)
        };
        let id = match first {
            Some(answer) => self.button_id(answer),
            None if self.close_button => self.close_id(),
            None => return,
        };
        ctx.memory_mut(|memory| memory.request_focus(id));
    }
}

/// Where a dialog's own window was put and how tall its body was.
#[derive(Clone, Copy, PartialEq)]
struct Placed {
    at: egui::Pos2,
    body_height: f32,
}

/// The touch density on a touch layout, else `other`.
fn touch_or(ctx: &Context, other: Density) -> Density {
    if crate::widgets::is_touch(ctx) {
        Density::Touch
    } else {
        other
    }
}

/// `text` on one line of `role`, cut short with an ellipsis to fit
/// `room`.
fn one_line(
    ui: &Ui,
    text: &str,
    role: TextRole,
    color: Color32,
    room: f32,
    theme: &Theme,
) -> std::sync::Arc<egui::Galley> {
    let mut job =
        egui::text::LayoutJob::simple_singleline(text.to_owned(), theme.text_font(role), color);
    job.wrap.max_width = room.max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.painter().layout_job(job)
}

/// What [`entered`] remembers of something on screen.
#[derive(Clone, Copy)]
struct Shown {
    /// The frame it was last drawn in.
    frame: u64,
    /// When it appeared.
    since: f64,
}

/// Follows something that is drawn every frame while it is open. Returns
/// whether it appeared in this frame, and how far it has come in at
/// `pace`: 0 on the frame it appears, 1 once it is in place (at once
/// when transitions are instant). Asks for frames while it comes in.
fn entered(ctx: &Context, id: Id, pace: Pace) -> (bool, f32) {
    let id = id.with("sue-entered");
    let frame = ctx.cumulative_frame_nr();
    let now = ctx.input(|input| input.time);
    let known: Option<Shown> = ctx.data(|data| data.get_temp(id));
    let appeared = known.is_none_or(|known| frame > known.frame + 1);
    let since = match known {
        Some(known) if !appeared => known.since,
        _ => now,
    };
    ctx.data_mut(|data| data.insert_temp(id, Shown { frame, since }));
    let seconds = Motion::of(ctx).seconds(pace);
    let along = if seconds <= 0.0 {
        1.0
    } else {
        ((now - since) as f32 / seconds).clamp(0.0, 1.0)
    };
    if along < 1.0 {
        ctx.request_repaint();
    }
    (appeared, motion::ease(along))
}

/// A footer button at `rect`: the accepting one filled with the text
/// color, the others quiet with an outline.
fn action_button(
    ui: &Ui,
    id: Id,
    rect: Rect,
    label: &str,
    kind: Kind,
    enabled: bool,
    theme: &Theme,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, id, sense);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
    let radius = theme.radius(Radius::Md);
    let surface = theme.surface(1);
    let lit = motion::hover_t(
        ui.ctx(),
        id.with("sue-hover"),
        enabled && response.hovered(),
    );
    let down = enabled && response.is_pointer_button_down_on();
    let (fill, edge, text) = match kind {
        _ if !enabled => (
            theme.surface(2),
            Stroke::NONE,
            theme.text_disabled(theme.surface(2)),
        ),
        Kind::Primary | Kind::Danger => {
            // The text color (or the danger color) as the fill, and as
            // the text whichever of the page and text colors reads best
            // on it: the strongest thing in a monochrome dialog. Hover
            // and press take it a step toward the panel.
            let base = if kind == Kind::Danger {
                theme.danger()
            } else {
                theme.text
            };
            let step = if down { 0.28 } else { 0.14 * lit };
            let fill = base.lerp_to_gamma(surface, step);
            let text = if contrast(theme.void, base) >= contrast(theme.text, base) {
                theme.void
            } else {
                theme.text
            };
            (fill, Stroke::NONE, text)
        }
        Kind::Secondary => {
            let fill = if down {
                theme.fill_pressed()
            } else {
                theme.surface(2).lerp_to_gamma(theme.fill_hover(), lit)
            };
            (
                fill,
                Stroke::new(theme.border_width, theme.border_strong_on(surface)),
                theme.text,
            )
        }
    };
    let painter = ui.painter();
    painter.rect(rect, radius, fill, edge, StrokeKind::Inside);
    let galley = one_line(
        ui,
        label,
        TextRole::Body,
        text,
        rect.width() - theme.space(Space::S4) * 2.0,
        theme,
    );
    painter.galley(rect.center() - galley.size() / 2.0, galley, text);
    focus_ring(ui, &response, radius, theme);
    response
}

/// Asks one question: `dialog` (its title and buttons; OK and Cancel
/// when it names none) around the glyph of `tone` and `message`. Returns
/// the answer in the frame it is given.
pub fn confirm(ctx: &Context, dialog: Dialog<'_>, tone: Tone, message: &str) -> Option<Answer> {
    let theme = dialog.theme;
    let dialog = if dialog.has_footer() {
        dialog
    } else {
        dialog.ok_cancel()
    };
    dialog
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                let side = theme.space(Space::S8);
                let (mark, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                tone.paint(ui.painter(), mark, theme);
                ui.add_space(theme.space(Space::S4));
                ui.vertical(|ui| {
                    // The first line sits level with the middle of the
                    // glyph.
                    let line = theme.text_size(TextRole::Body);
                    ui.add_space(((side - line) / 2.0 - 2.0).max(0.0));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(message)
                                .font(theme.text_font(TextRole::Body))
                                .color(theme.text),
                        )
                        .wrap(),
                    );
                });
            });
        })
        .answer
}

/// A sheet: a panel that rises from the bottom edge over a scrim. Drag
/// its top down, click beside it or press Escape to send it away.
#[derive(Clone, Debug)]
#[must_use = "a sheet does nothing until it is shown"]
pub struct Sheet<'a> {
    id: Id,
    title: &'a str,
    theme: &'a Theme,
    max_width: f32,
}

/// What showing a sheet gave.
#[derive(Clone, Debug)]
pub struct SheetResponse<R> {
    /// What the body returned.
    pub inner: R,
    /// The sheet was sent away and has slid out: stop showing it.
    pub dismissed: bool,
    /// Where the sheet was drawn.
    pub rect: Rect,
    /// Where its top can be grabbed to drag it.
    pub grip: Rect,
}

/// What a sheet remembers between frames.
#[derive(Clone, Copy)]
struct SheetState {
    /// How far down it is dragged.
    offset: f32,
    /// How tall it was drawn last.
    height: f32,
    /// Let go too early: the spring that takes it back up.
    back: Option<SpringDamper>,
    /// Sent away: when, and from which offset it slides out.
    leaving: Option<(f64, f32)>,
}

impl<'a> Sheet<'a> {
    /// A sheet named by `salt` with `title` (which may be empty).
    pub fn new(salt: impl egui::AsIdSalt, title: &'a str, theme: &'a Theme) -> Self {
        Self {
            id: Id::unique("sue-sheet").with(salt),
            title,
            theme,
            max_width: SHEET_WIDTH,
        }
    }

    /// How wide it gets on a wide screen. On a narrow one it spans the
    /// screen.
    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = width;
        self
    }

    /// Draws the sheet for this frame.
    pub fn show<R>(self, ctx: &Context, body: impl FnOnce(&mut Ui) -> R) -> SheetResponse<R> {
        let theme = self.theme;
        let screen = ctx.content_rect();
        let now = ctx.input(|input| input.time);
        let motion = Motion::of(ctx);
        let (appeared, shown) = entered(ctx, self.id, Pace::Slow);
        let memory = self.id.with("sue-sheet-state");
        let mut state = ctx
            .data(|data| data.get_temp::<SheetState>(memory))
            .filter(|_| !appeared)
            .unwrap_or(SheetState {
                offset: 0.0,
                height: 0.0,
                back: None,
                leaving: None,
            });

        // Where it is this frame.
        let mut dismissed = false;
        if let Some((since, from)) = state.leaving {
            let seconds = motion.seconds(Pace::Slow);
            let along = if seconds <= 0.0 {
                1.0
            } else {
                ((now - since) as f32 / seconds).clamp(0.0, 1.0)
            };
            state.offset = from + (state.height - from) * motion::ease(along);
            dismissed = along >= 1.0;
            ctx.request_repaint();
        } else if let Some(spring) = &mut state.back {
            spring.set_params(motion.spring);
            spring.set_mode(motion.mode());
            let dt = ctx.input(|input| input.stable_dt) * motion.speed.max(0.01);
            let moving = spring.step(dt);
            state.offset = spring.position().max(0.0);
            if moving {
                ctx.request_repaint();
            } else {
                state.offset = 0.0;
                state.back = None;
            }
        }
        let rising = (1.0 - shown) * state.height;
        let down = state.offset + rising;
        let there = if state.height > 0.0 {
            (1.0 - down / state.height).clamp(0.0, 1.0)
        } else {
            shown
        };

        let area = Area::new(self.id)
            .kind(UiKind::Modal)
            .order(Layer::Dialog.order())
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, down))
            .constrain(false)
            .sense(Sense::hover())
            .interactable(true)
            .fade_in(false)
            .accessible_name(self.title);
        let top = ctx.memory_mut(|memory| {
            memory.set_modal_layer(area.layer());
            memory.top_modal_layer() == Some(area.layer())
        }) || appeared;
        let popup_open = egui::Popup::is_any_open(ctx);
        let density = touch_or(ctx, Density::Regular);
        let width = self.max_width.min(screen.width());
        let round = theme.corner(Radius::Xl);
        let frame = Frame::new()
            .fill(theme.surface(1))
            .stroke(theme.hairline())
            .corner_radius(CornerRadius {
                nw: round.nw,
                ne: round.ne,
                sw: 0,
                se: 0,
            })
            .shadow(theme.elevation(Elevation::Modal));
        let margin = theme.dialog_margin();
        let grip_height = theme.space(Space::S7);
        let title_height = if self.title.is_empty() {
            0.0
        } else {
            density.control_height()
        };
        let room = screen.height() * 0.85 - grip_height - title_height - margin;
        let mut leave = false;
        let shown_area = area.show(ctx, |ui| {
            let mut scrim = ui.new_child(
                UiBuilder::new()
                    .sense(Sense::CLICK | Sense::DRAG)
                    .max_rect(screen),
            );
            scrim.set_min_size(screen.size());
            ui.painter()
                .rect_filled(screen, 0.0, theme.scrim().gamma_multiply(there));
            leave |= scrim.response().clicked();
            let panel =
                ui.scope_builder(UiBuilder::new().sense(Sense::CLICK | Sense::DRAG), |ui| {
                    // Unseen on the frame it appears: its height is not
                    // known yet, so it cannot start below the edge.
                    if appeared && shown < 1.0 {
                        ui.set_opacity(0.0);
                    }
                    frame
                        .show(ui, |ui| {
                            // The outline is part of the width: a sheet
                            // as wide as a phone ends at its edges.
                            let width = width - frame.stroke.width * 2.0;
                            ui.set_width(width);
                            ui.spacing_mut().item_spacing = vec2(theme.space(Space::S4), 0.0);
                            let (_, top) =
                                ui.allocate_space(vec2(width, grip_height + title_height));
                            let grip =
                                ui.interact(top, self.id.with("sue-sheet-grip"), Sense::DRAG);
                            let bar = Rect::from_center_size(
                                pos2(top.center().x, top.top() + grip_height / 2.0),
                                vec2(theme.space(Space::S9), theme.space(Space::S2)),
                            );
                            ui.painter().rect_filled(
                                bar,
                                theme.radius(Radius::Full),
                                theme.border_strong_on(theme.surface(1)),
                            );
                            if !self.title.is_empty() {
                                let title = one_line(
                                    ui,
                                    self.title,
                                    TextRole::Title,
                                    theme.text,
                                    width - margin * 2.0,
                                    theme,
                                );
                                let row = Rect::from_min_max(
                                    pos2(top.left(), top.top() + grip_height),
                                    top.max,
                                );
                                ui.painter().galley(
                                    pos2(
                                        row.left() + margin,
                                        row.center().y - title.size().y / 2.0,
                                    ),
                                    title,
                                    theme.text,
                                );
                            }
                            let inner = Frame::new()
                                .inner_margin(Margin::symmetric(
                                    margin.round().clamp(0.0, 127.0) as i8,
                                    0,
                                ))
                                .show(ui, |ui| {
                                    ScrollArea::vertical()
                                        .id_salt(self.id.with("sue-sheet-body"))
                                        .max_height(room.max(0.0))
                                        .auto_shrink([false, true])
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing =
                                                vec2(theme.space(Space::S4), theme.form_row_gap());
                                            body(ui)
                                        })
                                        .inner
                                })
                                .inner;
                            ui.add_space(margin);
                            (inner, grip)
                        })
                        .inner
                });
            (panel.inner, panel.response.rect)
        });
        let ((inner, grip), rect) = shown_area.inner;
        state.height = rect.height();

        // The drag, Escape and the scrim.
        if state.leaving.is_none() {
            if grip.dragged() {
                state.offset = (state.offset + grip.drag_delta().y).max(0.0);
                state.back = None;
            }
            if grip.drag_stopped() {
                let speed = ctx.input(|input| input.pointer.velocity().y);
                if state.offset > state.height * SHEET_LEAVES || speed > SHEET_FLICK {
                    leave = true;
                } else if state.offset > 0.0 {
                    let mut spring = SpringDamper::new(motion.spring, motion.mode(), state.offset);
                    spring.set_target(0.0);
                    state.back = Some(spring);
                    ctx.request_repaint();
                }
            }
            if top
                && !popup_open
                && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
            {
                leave = true;
            }
            if leave {
                state.leaving = Some((now, state.offset));
                state.back = None;
                ctx.request_repaint();
            }
        }
        if dismissed {
            ctx.data_mut(|data| data.remove::<SheetState>(memory));
        } else {
            ctx.data_mut(|data| data.insert_temp(memory, state));
        }
        SheetResponse {
            inner,
            dismissed,
            rect,
            grip: grip.rect,
        }
    }
}

/// One row of a form: `label` in a column `label_width` wide, then the
/// control `add` draws, on one line and level with each other. The
/// response is the label's, for
/// [`labelled_by`](egui::Response::labelled_by).
pub fn form_row<R>(
    ui: &mut Ui,
    label: &str,
    label_width: f32,
    theme: &Theme,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let height = Density::of(ui).control_height();
    let font = theme.text_font(TextRole::Body);
    let color = theme.text_muted();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space(Space::S4);
        let text = ui
            .painter()
            .layout(label.to_owned(), font, color, label_width);
        let (rect, response) =
            ui.allocate_exact_size(vec2(label_width, height.max(text.size().y)), Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(Role::Label, true, label));
        ui.painter().galley(
            pos2(rect.left(), rect.center().y - text.size().y / 2.0),
            text,
            color,
        );
        ui.add_space(theme.label_gap() - theme.space(Space::S4));
        ui.set_min_height(height);
        InnerResponse::new(add(ui), response)
    })
    .inner
}

/// The rows of a [`form_grid`].
pub struct FormGrid<'u, 't> {
    ui: &'u mut Ui,
    theme: &'t Theme,
    /// The width of the label column this frame.
    label_width: f32,
    /// The widest label met this frame.
    widest: f32,
    /// Labels stand over their controls: the form is too narrow for two
    /// columns.
    stacked: bool,
}

impl FormGrid<'_, '_> {
    /// A row: `label`, then the control `add` draws. The response is the
    /// label's.
    pub fn row<R>(&mut self, label: &str, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let theme = self.theme;
        let font = theme.text_font(TextRole::Body);
        let natural = self
            .ui
            .painter()
            .layout_no_wrap(label.to_owned(), font.clone(), theme.text_muted())
            .size()
            .x;
        self.widest = self.widest.max(natural.ceil());
        if !self.stacked {
            return form_row(self.ui, label, self.label_width, theme, add);
        }
        let height = Density::of(self.ui).control_height();
        self.ui
            .vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.space(Space::S2);
                let response = ui.add(
                    egui::Label::new(
                        egui::RichText::new(label)
                            .font(font)
                            .color(theme.text_muted()),
                    )
                    .wrap(),
                );
                let inner = ui
                    .horizontal(|ui| {
                        ui.set_min_height(height);
                        add(ui)
                    })
                    .inner;
                InnerResponse::new(inner, response)
            })
            .inner
    }

    /// A row without a label, its control in the control column: a check
    /// box, a note.
    pub fn wide<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> R {
        if self.stacked {
            return self.ui.horizontal(add).inner;
        }
        form_row(self.ui, "", self.label_width, self.theme, add).inner
    }

    /// A heading over the rows that follow, with a hairline to its
    /// right.
    pub fn section(&mut self, title: &str) {
        let theme = self.theme;
        let ui = &mut *self.ui;
        if ui.min_rect().height() > 0.0 {
            ui.add_space(theme.space(Space::S4));
        }
        let text = ui.painter().layout_no_wrap(
            title.to_owned(),
            theme.text_font(TextRole::Body),
            theme.text,
        );
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), text.size().y), Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(Role::Label, true, title));
        let line = rect.left() + text.size().x + theme.space(Space::S4);
        ui.painter().galley(rect.min, text, theme.text);
        if line < rect.right() {
            ui.painter()
                .hline(line..=rect.right(), rect.center().y, theme.hairline());
        }
    }

    /// The `Ui` the rows are drawn in, for anything that spans both
    /// columns.
    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }
}

/// A form: rows of a label and a control, the labels in one column as
/// wide as the widest of them. Where the room is too narrow for two
/// columns, each label stands over its control. `salt` names the form
/// among those of the same `ui`.
pub fn form_grid<R>(
    ui: &mut Ui,
    salt: impl egui::AsIdSalt,
    theme: &Theme,
    add: impl FnOnce(&mut FormGrid<'_, '_>) -> R,
) -> R {
    let memory = ui.scope_id().with(("sue-form", Id::unique(salt)));
    let known: Option<f32> = ui.data(|data| data.get_temp(memory));
    let room = ui.available_width();
    // A label column is at most 45 percent of the form: longer labels
    // wrap.
    let label_width = known.unwrap_or(0.0).min(room * 0.45);
    let control_room = theme.space(Space::S9) * 4.0;
    let stacked = known.is_some_and(|_| room - label_width - theme.label_gap() < control_room);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = theme.form_row_gap();
        let mut form = FormGrid {
            ui,
            theme,
            label_width,
            widest: 0.0,
            stacked,
        };
        let inner = add(&mut form);
        let widest = form.widest;
        if known != Some(widest) {
            // The labels changed (or this is the first frame): lay the
            // form out again with the column they need.
            ui.data_mut(|data| data.insert_temp(memory, widest));
            ui.ctx().request_discard("sue form labels");
        }
        inner
    })
    .inner
}

#[cfg(test)]
pub(super) mod tests;
