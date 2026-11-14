//! The window that floats over the page (Settings and the like).

use egui::{Align, Id, Layout, Order, Rect, RichText, Sense, UiBuilder, Vec2, pos2, vec2};
use sue_platform::FormFactor;
use sue_platform::time::Instant;

use super::{Shell, ShellIo, split};
use crate::{Theme, widgets};

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
        let size = sheet.size();
        let rise = if form == FormFactor::Mobile {
            size.y * 0.2
        } else {
            18.0
        };
        let at = sheet.min + vec2(0.0, (1.0 - appear) * rise);
        self.window_rect = sheet;
        // Its title bar holds a finger-sized close button on touch layouts.
        let bar_height = match form {
            FormFactor::Desktop => 36.0,
            FormFactor::Tablet => 44.0,
            FormFactor::Mobile => 52.0,
        };
        let name = io
            .applets
            .manifest(&id)
            .map(|manifest| manifest.title())
            .unwrap_or_default();
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
                    bar_ui.label(RichText::new(&name).size(theme.ui_size).color(theme.text));
                    bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (glyph, target) = if form == FormFactor::Desktop {
                            (1.3, Vec2::ZERO)
                        } else {
                            (1.8, Vec2::splat(bar_height - 8.0))
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("\u{00D7}")
                                        .size(theme.ui_size * glyph)
                                        .color(theme.muted),
                                )
                                .frame(false)
                                .min_size(target),
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
                    let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &id));
                    let (applets, mut cx) = split(io, theme, view);
                    if let Some(applet) = applets.get_mut(&id) {
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
