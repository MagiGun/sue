//! Leaving Sue: the question about work that is not saved.

use egui::{Align2, Id, Rect, RichText, Sense, vec2};
use sue_platform::time::Instant;

use super::{Shell, ShellIo, split};
use crate::{ShellRequest, Theme};

impl Shell {
    /// Quitting, as Clip Studio Paint asks: "Illustration has been changed.
    /// Do you want to save it?" for each piece of work not saved, with
    /// Save, Do Not Save and Cancel. When none is left, asks the runtime to
    /// quit with `app.quit.checked`.
    pub(super) fn quit_ui(
        &mut self,
        ctx: &egui::Context,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
    ) {
        let Some(skipped) = self.quitting.clone() else {
            return;
        };
        // A save that asks where uses the system's dialog, over this one;
        // once it is saved it leaves the list, and if not it is asked again.
        let next = io.applets.iter().find_map(|(manifest, applet)| {
            applet
                .unsaved(&io.world.arena)
                .into_iter()
                .find(|(key, _)| !skipped.contains(&(manifest.id.clone(), key.clone())))
                .map(|(key, name)| (manifest.id.clone(), manifest.title(), key, name))
        });
        let Some((id, studio, key, name)) = next else {
            self.quitting = None;
            io.requests
                .push(ShellRequest::Command("app.quit.checked".to_owned()));
            return;
        };
        let screen = ctx.content_rect();
        egui::Area::new(Id::unique("sue-unsaved-scrim"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, theme.shadow.gamma_multiply(0.5));
            });
        let mut answer = None;
        egui::Area::new(Id::unique("sue-unsaved"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(screen.center())
            .show(ctx, |ui| {
                // A sharp box, as Clip Studio Paint's message boxes.
                egui::Frame::new()
                    .fill(theme.panel)
                    .stroke(theme.hairline())
                    .shadow(theme.shadow())
                    .show(ui, |ui| {
                        ui.set_width(420.0);
                        let bar = Rect::from_min_size(
                            ui.max_rect().min,
                            vec2(420.0, crate::widgets::TITLE_BAR),
                        );
                        ui.painter().rect_filled(bar, 0.0, theme.void);
                        ui.painter().text(
                            bar.center(),
                            Align2::CENTER_CENTER,
                            &studio,
                            theme.font(theme.small_size + 0.5),
                            theme.muted,
                        );
                        // Its close button cancels, as Clip Studio's does.
                        let close =
                            Rect::from_min_max(egui::pos2(bar.right() - 40.0, bar.top()), bar.max);
                        let hit = ui
                            .interact(close, Id::unique("sue-unsaved-close"), Sense::click())
                            .on_hover_text(tr!("cancel"));
                        if hit.hovered() {
                            ui.painter().rect_filled(close, 0.0, theme.hover);
                        }
                        let stroke = egui::Stroke::new(
                            1.0,
                            if hit.hovered() {
                                theme.text
                            } else {
                                theme.muted
                            },
                        );
                        let c = close.center();
                        ui.painter()
                            .line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], stroke);
                        ui.painter()
                            .line_segment([c + vec2(5.0, -5.0), c + vec2(-5.0, 5.0)], stroke);
                        if hit.clicked() {
                            answer = Some(0);
                        }
                        ui.add_space(crate::widgets::TITLE_BAR);
                        egui::Frame::new()
                            .inner_margin(egui::Margin::same(16))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Clip Studio's question mark in a disc.
                                    let (mark, _) =
                                        ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                                    ui.painter()
                                        .circle_filled(mark.center(), 15.0, theme.active);
                                    ui.painter().text(
                                        mark.center(),
                                        Align2::CENTER_CENTER,
                                        "?",
                                        theme.font(theme.heading_size),
                                        theme.text,
                                    );
                                    ui.add_space(8.0);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(tr!(
                                                "unsaved-save-it",
                                                name = name.as_str()
                                            ))
                                            .color(theme.text),
                                        )
                                        .wrap(),
                                    );
                                });
                                ui.add_space(theme.padding);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if crate::widgets::pill_button(
                                            ui,
                                            &tr!("cancel"),
                                            false,
                                            theme,
                                        )
                                        .clicked()
                                        {
                                            answer = Some(0);
                                        }
                                        if crate::widgets::pill_button(
                                            ui,
                                            &tr!("do-not-save"),
                                            false,
                                            theme,
                                        )
                                        .clicked()
                                        {
                                            answer = Some(1);
                                        }
                                        if crate::widgets::pill_button(
                                            ui,
                                            &tr!("save"),
                                            true,
                                            theme,
                                        )
                                        .clicked()
                                        {
                                            answer = Some(2);
                                        }
                                    },
                                );
                            });
                    });
            });
        let (enter, escape) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Escape),
            )
        });
        if escape {
            answer = Some(0);
        } else if enter {
            answer = Some(2);
        }
        match answer {
            Some(0) => self.quitting = None,
            Some(1) => {
                if let Some(skipped) = self.quitting.as_mut() {
                    skipped.push((id, key));
                }
            }
            Some(_) => {
                let view = self.view(now, 0.0);
                let theme = self.theme.clone();
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(applet) = applets.get_mut(&id) {
                    applet.save_unsaved(&key, &mut cx);
                }
            }
            None => {}
        }
        if answer.is_some() {
            self.touch();
        }
    }
}
