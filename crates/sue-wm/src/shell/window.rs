//! The window that floats over the page (Settings and the like), and the
//! applets' views in windows of their own.
//!
//! On a computer and a tablet the floating window is Sue's dialog host,
//! sized to the screen and how the tablet is held; on a phone it fills the
//! screen, clear of notches, and rises from below.
//!
//! A view ([`ShellRequest::OpenView`](crate::ShellRequest::OpenView)) is
//! a system window of its own where the system gives Sue many (the host
//! opens what [`Shell::os_windows`] lists and draws each with
//! [`Shell::view_window_ui`]), and a frame floating over the page where it
//! gives one (phones, tablets, the browser).

use egui::{
    Align, Id, Layout, Order, Pos2, Rect, RichText, Sense, Ui, UiBuilder, Vec2, pos2, vec2,
};
use sue_platform::time::Instant;
use sue_platform::{FormFactor, WindowSpec, WindowSupport};

use super::{Shell, ShellIo, split};
use crate::ui::{Answer, Density, Dialog, Elevation, Radius, Space, TextRole};
use crate::{AppletId, Theme, widgets};

impl Shell {
    #[allow(clippy::too_many_arguments, reason = "a window over the screen")]
    pub(super) fn window_ui(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        let Some(id) = self.window.clone() else {
            return;
        };
        // Sized by the screen's real size and shape: a phone's fills it,
        // clear of notches; a tablet's is shaped to how it is held.
        let form = self.form_factor();
        let insets = self.safe_area;
        let area = Rect::from_min_max(
            pos2(screen.left() + insets.left, screen.top() + insets.top),
            pos2(
                screen.right() - insets.right,
                screen.bottom() - insets.bottom,
            ),
        );
        let sheet = crate::sheet_rect(form, area);
        let name = io
            .applets
            .manifest(&id)
            .map(|manifest| manifest.title())
            .unwrap_or_default();
        if form == FormFactor::Mobile {
            self.phone_window_ui(ctx, screen, sheet, &id, &name, io, theme, now, dt);
        } else {
            self.dialog_window_ui(ctx, sheet, form, &id, &name, io, theme, now, dt);
        }
    }

    /// The window in the dialog host: its title and close button, the
    /// applet's body below; Escape, the close button and a click beside
    /// it close it. Once closing, it is gone at once.
    #[allow(clippy::too_many_arguments, reason = "a window over the screen")]
    fn dialog_window_ui(
        &mut self,
        ctx: &egui::Context,
        sheet: Rect,
        form: FormFactor,
        id: &AppletId,
        name: &str,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        if self.window_appear.target() <= 0.0 {
            return;
        }
        // The host draws a header (a title and the close button in a row
        // of its own), a margin under the body and its outline; the body
        // takes the rest of the sheet.
        let density = if form == FormFactor::Desktop {
            Density::Regular
        } else {
            Density::Touch
        };
        let chrome = theme.space(Space::S4) * 2.0
            + density.icon_button()
            + theme.dialog_margin()
            + theme.hairline().width * 2.0;
        let body_height = (sheet.height() - chrome).max(0.0);
        let view = self.view(now, dt);
        let mut dialog = Dialog::new(("sue-window", id.as_str()), name, theme)
            .width(sheet.width())
            .scrim_cancels()
            .enter_is_the_bodys();
        if form == FormFactor::Desktop {
            // Moved by its header, sized from its corner, as any window.
            dialog = dialog.resizable();
        }
        let body_height = dialog.sized_body_height(ctx).unwrap_or(body_height);
        let shown = dialog.height(body_height).show(ctx, |ui| {
            // The applet lays out its own body in the room it has, a
            // hair short of it so the host's scroll area never needs
            // its bar: only the applet's own lists scroll.
            let size = vec2(ui.available_width(), (body_height - 1.0).max(0.0));
            let (body, _) = ui.allocate_exact_size(size, Sense::hover());
            let mut body_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(body)
                    .id_salt(("sue-window-body", id.as_str()))
                    .layout(Layout::top_down(Align::Min)),
            );
            body_ui.set_clip_rect(body.intersect(ui.clip_rect()));
            let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, id));
            let (applets, mut cx) = split(io, theme, view);
            if let Some(applet) = applets.get_mut(id) {
                applet.window_ui(&mut body_ui, &mut cx);
            }
        });
        self.window_rect = shown.rect;
        if shown.answer == Some(Answer::Cancel) {
            self.close_window();
        }
    }

    /// The window on a phone: the whole screen, rising from below, with a
    /// finger-sized close button in its title bar.
    #[allow(clippy::too_many_arguments, reason = "a window over the screen")]
    fn phone_window_ui(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        sheet: Rect,
        id: &AppletId,
        name: &str,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        let appear = self.window_appear.position().clamp(0.0, 1.0);
        // A scrim that closes the window when clicked.
        let scrim = egui::Area::new(Id::unique("sue-window-scrim"))
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, theme.shadow.gamma_multiply(0.45 * appear));
                response.clicked()
            })
            .inner;
        if scrim {
            self.close_window();
        }
        let size = sheet.size();
        let rise = size.y * 0.2;
        let at = sheet.min + vec2(0.0, (1.0 - appear) * rise);
        self.window_rect = sheet;
        let bar_height = 52.0;
        let view = self.view(now, dt);
        let mut close = false;
        egui::Area::new(Id::unique("sue-window"))
            .order(Order::Foreground)
            .fixed_pos(at)
            .interactable(appear > 0.3)
            .show(ctx, |ui| {
                ui.set_opacity(appear);
                widgets::window_frame(theme).show(ui, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    let bar = Rect::from_min_size(ui.min_rect().min, vec2(size.x, bar_height));
                    let mut bar_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(bar.shrink2(vec2(theme.padding * 1.5, 0.0)))
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    bar_ui.label(RichText::new(name).size(theme.ui_size).color(theme.text));
                    bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if crate::ui::icon_button(
                            "sue-window-close",
                            crate::icons::Icon::Close,
                            &tr!("close-esc"),
                        )
                        .quiet()
                        .show(ui, theme)
                        .clicked()
                        {
                            close = true;
                        }
                    });
                    ui.painter()
                        .hline(bar.x_range(), bar.bottom(), theme.hairline());
                    let body =
                        Rect::from_min_max(pos2(bar.left(), bar.bottom()), bar.left_top() + size);
                    let mut body_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(body)
                            .id_salt(("sue-window-body", id.as_str()))
                            .layout(Layout::top_down(Align::Min)),
                    );
                    body_ui.set_clip_rect(body);
                    let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, id));
                    let (applets, mut cx) = split(io, theme, view);
                    if let Some(applet) = applets.get_mut(id) {
                        applet.window_ui(&mut body_ui, &mut cx);
                    }
                    ui.allocate_rect(Rect::from_min_size(bar.min, size), Sense::hover());
                });
            });
        if close {
            self.close_window();
        }
    }
}

// ----- Views in windows of their own ----------------------------------------

/// How big a view's window opens, in points, when nothing smaller fits.
const VIEW_SIZE: Vec2 = vec2(960.0, 640.0);

/// One of an applet's views in a window of its own
/// ([`ShellRequest::OpenView`](crate::ShellRequest::OpenView)), or, where
/// the system gives Sue one window, in a frame floating over the page.
#[derive(Clone, Debug)]
pub(super) struct View {
    applet: AppletId,
    view: String,
    title: String,
    /// Where the floating frame's top left corner is (`None`: not placed
    /// yet).
    at: Option<Pos2>,
    /// Where the last frame drew the floating frame, and its close button.
    rect: Rect,
    close: Rect,
}

impl View {
    fn key(&self) -> String {
        view_key(&self.applet, &self.view)
    }
}

/// The key of view `view` of `applet`: names its window while it is open.
#[must_use]
pub fn view_key(applet: &AppletId, view: &str) -> String {
    format!("{}/{view}", applet.as_str())
}

impl Shell {
    /// Says how many windows the system gives Sue: with
    /// [`WindowSupport::Many`] each view opens in a window of its own
    /// ([`Self::os_windows`]), with [`WindowSupport::One`] it floats over
    /// the page.
    pub fn set_window_support(&mut self, support: WindowSupport) {
        self.window_support = support;
    }

    /// How many windows the system gives Sue.
    #[must_use]
    pub fn window_support(&self) -> WindowSupport {
        self.window_support
    }

    /// Opens view `view` of `applet` in a window titled `title`, or brings
    /// its window to the front when it is open (taking the new title).
    pub fn open_view(&mut self, applet: AppletId, view: &str, title: &str) {
        let key = view_key(&applet, view);
        match self.views.iter().position(|open| open.key() == key) {
            Some(index) => {
                let mut open = self.views.remove(index);
                title.clone_into(&mut open.title);
                // The last is the front one.
                self.views.push(open);
                self.raise_view = Some(key);
            }
            None => self.views.push(View {
                applet,
                view: view.to_owned(),
                title: title.to_owned(),
                at: None,
                rect: Rect::NOTHING,
                close: Rect::NOTHING,
            }),
        }
        self.touch();
    }

    /// Closes the window of the view named `key` and tells its applet.
    pub fn close_view(&mut self, key: &str, io: &mut ShellIo<'_>, now: Instant) {
        let Some(index) = self.views.iter().position(|open| open.key() == key) else {
            return;
        };
        let gone = self.views.remove(index);
        let theme = self.theme.clone();
        let view = self.view(now, 0.0);
        let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &gone.applet));
        let (applets, mut cx) = split(io, &theme, view);
        if let Some(applet) = applets.get_mut(&gone.applet) {
            applet.view_closed(&gone.view, &mut cx);
        }
        self.touch();
    }

    /// The keys of the views open in windows of their own or floating, the
    /// front one last.
    #[must_use]
    pub fn views(&self) -> Vec<String> {
        self.views.iter().map(View::key).collect()
    }

    /// Where the last frame drew the floating frame of the view named
    /// `key`, and its close button (only where it floats).
    #[must_use]
    pub fn view_rects(&self, key: &str) -> Option<(Rect, Rect)> {
        self.views
            .iter()
            .find(|open| open.key() == key)
            .filter(|open| open.rect.is_positive())
            .map(|open| (open.rect, open.close))
    }

    /// The windows of their own the views want, for the host to open
    /// ([`WindowSupport::Many`] only; none where they float).
    #[must_use]
    pub fn os_windows(&self) -> Vec<WindowSpec> {
        if !self.window_support.many() {
            return Vec::new();
        }
        self.views
            .iter()
            .map(|open| WindowSpec::new(open.key(), open.title.clone(), [VIEW_SIZE.x, VIEW_SIZE.y]))
            .collect()
    }

    /// Builds one frame of the window of the view named `key` into `root`,
    /// which covers that window: Sue's title bar where Sue draws the
    /// frames, then the applet's view.
    pub fn view_window_ui(&mut self, key: &str, root: &mut Ui, io: &mut ShellIo<'_>, now: Instant) {
        let _language = sue_lang::scope(&crate::sue_language(&io.world.rice));
        let Some(open) = self.views.iter().find(|open| open.key() == key).cloned() else {
            return;
        };
        let ctx = root.ctx().clone();
        let theme = self.page_theme(&open.applet, &io.world.rice, &self.theme);
        let nominal = 1.0 / self.scheduler.target_hz().max(1) as f32;
        let window = root.max_rect();
        let mut close = false;
        let mut body = window;
        if self.own_frame {
            // Sue's title bar, as the main window's: in Sue's own theme.
            let bar =
                Rect::from_min_size(window.min, vec2(window.width(), crate::widgets::TITLE_BAR));
            let sue_theme = self.theme.clone();
            close = crate::widgets::title_bar(root, bar, &open.title, true, &sue_theme);
            body.min.y = bar.bottom();
        }
        self.with_theme(&ctx, &theme, |shell| {
            // The view wears its applet's theme, as its page does.
            root.painter().rect_filled(body, 0.0, theme.void);
            let mut body_ui = root.new_child(
                UiBuilder::new()
                    .max_rect(body)
                    .id_salt(("sue-view", key))
                    .layout(Layout::top_down(Align::Min)),
            );
            body_ui.set_clip_rect(body);
            let view = shell.view(now, nominal);
            let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &open.applet));
            let (applets, mut cx) = split(io, &theme, view);
            if let Some(applet) = applets.get_mut(&open.applet) {
                applet.view_ui(&open.view, &mut body_ui, &mut cx);
            }
        });
        if self.own_frame {
            crate::widgets::resize_edges(root, window);
        }
        if close {
            self.close_view(key, io, now);
        }
    }

    /// Draws the views that float over the page where the system gives
    /// Sue one window: each in a frame with its title and a close button,
    /// moved by its title strip, kept on the page. On a phone a view fills
    /// the page. Where views have windows of their own, it only brings a
    /// window asked for again to the front.
    #[allow(clippy::too_many_arguments, reason = "frames over the page")]
    pub(super) fn views_ui(
        &mut self,
        ctx: &egui::Context,
        page: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        if self.window_support.many() {
            if let Some(key) = self.raise_view.take() {
                ctx.send_viewport_cmd_to(
                    sue_platform::windows::viewport_of(&key),
                    egui::ViewportCommand::Focus,
                );
            }
            return;
        }
        let raise = self.raise_view.take();
        let caret_blink = self.config.caret_blink;
        let form = self.form_factor();
        let phone = form == FormFactor::Mobile;
        // The systems with one window are the touch ones: there the title
        // strip and its close button are finger sized.
        let density = if form == FormFactor::Desktop && !crate::widgets::is_touch(ctx) {
            Density::Compact
        } else {
            Density::Touch
        };
        let strip = density.tab_strip();
        let step = theme.space(Space::S6);
        let mut closed = Vec::new();
        for index in 0..self.views.len() {
            let open = self.views[index].clone();
            let key = open.key();
            // The frame is Sue's; the view inside wears its applet's theme.
            let page_theme = self.page_theme(&open.applet, &io.world.rice, theme);
            let size = if phone {
                page.size()
            } else {
                VIEW_SIZE.min(page.size() * 0.8)
            };
            // A new frame is cascaded from the page's middle; a frame keeps
            // its title strip on the page.
            let cascade = page.center() - size / 2.0 + Vec2::splat(step * index as f32);
            let wanted = if phone {
                page.min
            } else {
                open.at.unwrap_or(cascade)
            };
            let at = pos2(
                wanted
                    .x
                    .clamp(page.left(), (page.right() - size.x).max(page.left())),
                wanted
                    .y
                    .clamp(page.top(), (page.bottom() - strip).max(page.top())),
            );
            let frame = Rect::from_min_size(at, size);
            let area = egui::Area::new(Id::unique(("sue-view-frame", key.as_str())))
                .order(Order::Middle)
                .fixed_pos(at)
                .constrain(false);
            let shown = area.show(ctx, |ui| {
                let painter = ui.painter().clone();
                let radius = theme.radius(Radius::Md);
                // Lifted off the page as a popover is, so its edge reads
                // where the view and the page share a color.
                painter.add(theme.elevation(Elevation::Floating).as_shape(frame, radius));
                painter.rect_filled(frame, radius, page_theme.void);
                // The title strip: dragging it moves the frame.
                let header = Rect::from_min_size(frame.min, vec2(size.x, strip));
                let top = radius.round().clamp(0.0, f32::from(u8::MAX)) as u8;
                let corners = egui::CornerRadius {
                    nw: top,
                    ne: top,
                    sw: 0,
                    se: 0,
                };
                painter.rect_filled(header, corners, theme.surface(0));
                painter.hline(header.x_range(), header.bottom(), theme.hairline());
                painter.rect_stroke(frame, radius, theme.hairline(), egui::StrokeKind::Outside);
                let grip = ui.interact(
                    header,
                    Id::unique(("sue-view-grip", key.as_str())),
                    Sense::click_and_drag(),
                );
                let button = density.icon_button().min(strip);
                let close = Rect::from_min_size(
                    pos2(
                        header.right() - button - theme.space(Space::S1),
                        header.center().y - button / 2.0,
                    ),
                    Vec2::splat(button),
                );
                let inset = theme.space(Space::S4);
                let room = (close.left() - header.left() - 2.0 * inset).max(0.0);
                let galley = painter.layout(
                    open.title.clone(),
                    theme.text_font(if density == Density::Touch {
                        TextRole::Body
                    } else {
                        TextRole::Caption
                    }),
                    theme.text_muted(),
                    room,
                );
                painter.with_clip_rect(header).galley(
                    pos2(
                        header.left() + inset,
                        header.center().y - galley.size().y / 2.0,
                    ),
                    galley,
                    theme.text_muted(),
                );
                let closing = crate::ui::icon_button(
                    ("sue-view-close", key.as_str()),
                    crate::icons::Icon::Close,
                    &tr!("window-close"),
                )
                .show_at(ui, close, theme)
                .clicked();
                // The view itself, below the strip.
                let body = Rect::from_min_max(pos2(frame.left(), header.bottom()), frame.max);
                let mut body_ui = ui.new_child(
                    UiBuilder::new()
                        .max_rect(body)
                        .id_salt(("sue-view", key.as_str()))
                        .layout(Layout::top_down(Align::Min)),
                );
                body_ui.set_clip_rect(body);
                body_ui.style_mut().visuals = page_theme.visuals(caret_blink);
                let view = self.view(now, dt);
                let _language =
                    sue_lang::scope(&crate::applet_language(&io.world.rice, &open.applet));
                let (applets, mut cx) = split(io, &page_theme, view);
                if let Some(applet) = applets.get_mut(&open.applet) {
                    applet.view_ui(&open.view, &mut body_ui, &mut cx);
                }
                // The frame takes the clicks it covers.
                ui.allocate_rect(frame, Sense::hover());
                (grip.drag_delta(), close, closing)
            });
            if raise.as_deref() == Some(key.as_str()) {
                ctx.move_to_top(shown.response.layer_id);
            }
            let (moved, close, closing) = shown.inner;
            let entry = &mut self.views[index];
            entry.at = Some(at + moved);
            entry.rect = frame;
            entry.close = close;
            if moved != Vec2::ZERO {
                self.touch();
            }
            if closing {
                closed.push(key);
            }
        }
        for key in closed {
            self.close_view(&key, io, now);
        }
    }
}

#[cfg(test)]
mod view_tests;
