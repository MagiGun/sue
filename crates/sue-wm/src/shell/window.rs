//! The window that floats over the page (Settings and the like).
//!
//! On a computer and a tablet it is Sue's dialog host, sized to the screen
//! and how the tablet is held; on a phone it fills the screen, clear of
//! notches, and rises from below.

use egui::{Align, Id, Layout, Order, Rect, RichText, Sense, UiBuilder, Vec2, pos2, vec2};
use sue_platform::FormFactor;
use sue_platform::time::Instant;

use super::{Shell, ShellIo, split};
use crate::ui::{Answer, Density, Dialog, Space};
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
        let shown = Dialog::new(("sue-window", id.as_str()), name, theme)
            .width(sheet.width())
            .height(body_height)
            .scrim_cancels()
            .enter_is_the_bodys()
            .show(ctx, |ui| {
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
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("\u{00D7}")
                                        .size(theme.ui_size * 1.8)
                                        .color(theme.muted),
                                )
                                .frame(false)
                                .min_size(Vec2::splat(bar_height - 8.0)),
                            )
                            .on_hover_text(tr!("close-esc"))
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
