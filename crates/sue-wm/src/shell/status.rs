//! The status bar along the bottom, and the notice in it.

use egui::{Align2, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::{Shell, ShellIo, profile_label};
use crate::{ShellRequest, Theme};

impl Shell {
    /// Without the status bar, its message shows as a small notice at the
    /// bottom of the window while it lasts; a saved file's shows the file
    /// when clicked.
    pub(super) fn notice_ui(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
    ) {
        let Some((text, error)) = self.message() else {
            return;
        };
        let text = if error {
            format!("! {text}")
        } else {
            text.to_owned()
        };
        let path = self.message_path.clone();
        let font = theme.font(theme.small_size);
        let galley = ctx.fonts_mut(|fonts| {
            fonts.layout(text, font, theme.text, (screen.width() - 48.0).max(80.0))
        });
        let size = galley.size() + vec2(theme.padding * 2.0, theme.padding);
        let rect = Rect::from_center_size(
            pos2(screen.center().x, screen.bottom() - 16.0 - size.y / 2.0),
            size,
        );
        egui::Area::new(Id::unique("sue-notice"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .interactable(path.is_some())
            .show(ctx, |ui| {
                let (shown, response) = ui.allocate_exact_size(size, Sense::click());
                let painter = ui.painter();
                painter.rect(
                    shown,
                    0.0,
                    theme.panel,
                    theme.hairline(),
                    StrokeKind::Inside,
                );
                painter.galley(
                    shown.min + vec2(theme.padding, theme.padding / 2.0),
                    galley,
                    theme.text,
                );
                if let Some(path) = &path {
                    let response = response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(path.display().to_string());
                    if response.clicked() {
                        io.requests.push(ShellRequest::Command(format!(
                            "reveal-file:{}",
                            path.display()
                        )));
                    }
                }
            });
    }

    pub(super) fn status_ui(
        &mut self,
        root: &mut Ui,
        rect: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
    ) {
        let painter = root.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, theme.panel);
        painter.hline(rect.x_range(), rect.top(), theme.hairline());
        let font = theme.font(theme.small_size);
        let place = self
            .page
            .as_ref()
            .and_then(|page| io.applets.manifest(page))
            .map_or_else(|| tr!("hub"), |manifest| manifest.title());
        let placed = painter.text(
            rect.left_center() + vec2(theme.padding, 0.0),
            Align2::LEFT_CENTER,
            place,
            font.clone(),
            theme.muted,
        );
        let items = [
            tr!("entities", count = io.world.arena.len()),
            tr!("undo-depth", depth = io.history.depth()),
            profile_label(self.profile),
            match io.fps {
                Some(fps) if self.animating => tr!("fps", fps = fps),
                _ => tr!("idle"),
            },
        ];
        let mut x = rect.right() - theme.padding;
        // From the right; on a narrow screen the ones that would run into
        // the page's name are left out.
        for (index, item) in items.iter().enumerate().rev() {
            let width = painter
                .layout_no_wrap(item.clone(), font.clone(), theme.muted)
                .size()
                .x;
            if x - width < placed.right() + theme.padding * 2.0 {
                break;
            }
            let shown = painter.text(
                pos2(x, rect.center().y),
                Align2::RIGHT_CENTER,
                item,
                font.clone(),
                theme.muted,
            );
            x = shown.left() - theme.padding;
            if index > 0 {
                painter.vline(x, rect.shrink2(vec2(0.0, 5.0)).y_range(), theme.hairline());
                x -= theme.padding;
            }
        }
        if let Some((text, error)) = self.message() {
            let text = if error {
                format!("! {text}")
            } else {
                text.to_owned()
            };
            let at = pos2(placed.right() + theme.padding * 3.0, rect.center().y);
            let galley = painter.layout_no_wrap(text, font.clone(), theme.muted);
            // The message keeps to the room the counters left.
            let painter = painter.with_clip_rect(Rect::from_min_max(
                pos2(at.x, rect.top()),
                pos2(x.max(at.x), rect.bottom()),
            ));
            let area =
                Rect::from_min_size(pos2(at.x, rect.top()), vec2(galley.size().x, rect.height()));
            let mut color = if error { theme.text } else { theme.muted };
            if let Some(path) = self.message_path.clone() {
                // A saved file's message shows the file when clicked.
                let response = root
                    .interact(area, Id::unique("sue-status-message"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(path.display().to_string());
                if response.hovered() {
                    color = theme.text;
                    painter.hline(
                        area.x_range(),
                        rect.center().y + galley.size().y / 2.0,
                        Stroke::new(1.0, theme.muted),
                    );
                }
                if response.clicked() {
                    io.requests.push(ShellRequest::Command(format!(
                        "reveal-file:{}",
                        path.display()
                    )));
                }
            }
            painter.galley(
                pos2(at.x, rect.center().y - galley.size().y / 2.0),
                galley,
                color,
            );
        }
    }
}
