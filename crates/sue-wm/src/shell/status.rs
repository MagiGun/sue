//! The status bar along the bottom, and the notices (the toast stack)
//! above it.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Align2, Rect, Ui, pos2, vec2};

use super::{Shell, ShellIo, profile_label};
use crate::ui::{Toast, ToastEvent};
use crate::{ShellRequest, Theme};

impl Shell {
    /// Puts a notice on the stack for `stay`, worn as an error, a saved
    /// file (which offers to show the file, `reveal`) or plain news.
    pub(super) fn push_notice(
        &mut self,
        text: &str,
        error: bool,
        stay: Duration,
        reveal: Option<PathBuf>,
    ) {
        let toast = if error {
            Toast::error(text)
        } else if reveal.is_some() {
            Toast::success(text).action(tr!("show-in-folder"))
        } else {
            Toast::info(text)
        };
        let id = self.toasts.push(toast.seconds(stay.as_secs_f32()));
        // Each saved file's notice keeps its own file, whatever is said
        // after it; the same words again are the same notice.
        if let Some(path) = reveal {
            self.revealed.retain(|(shown, _)| *shown != id);
            self.revealed.push((id, path));
        }
    }

    /// The notices, at the bottom of `room` (the page, so they never
    /// cover the bar or the status bar). A saved file's shows the file
    /// in its folder when its button is pressed.
    pub(super) fn notice_ui(
        &mut self,
        ctx: &egui::Context,
        room: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
    ) {
        for event in self.toasts.show(ctx, room, theme) {
            let (ToastEvent::Action(id) | ToastEvent::Dismissed(id) | ToastEvent::Expired(id)) =
                event;
            let Some(at) = self.revealed.iter().position(|(shown, _)| *shown == id) else {
                continue;
            };
            let (_, path) = self.revealed.remove(at);
            if matches!(event, ToastEvent::Action(_)) {
                io.requests.push(ShellRequest::Command(format!(
                    "reveal-file:{}",
                    path.display()
                )));
            }
        }
        // Notices taken away without a word (all at once) leave no file.
        let toasts = &self.toasts;
        self.revealed.retain(|(id, _)| toasts.get(*id).is_some());
    }

    /// How many notices the stack holds, those leaving included.
    #[must_use]
    pub fn notices(&self) -> usize {
        self.toasts.len()
    }

    /// Takes every notice away at once, without a word (scripted runs:
    /// a tour that shows notices puts the window back as it found it).
    pub fn clear_notices(&mut self) {
        self.toasts.clear();
        self.revealed.clear();
        self.touch();
    }

    /// How many notices were folded behind the newest in the last frame
    /// (the stack shows a few and says how many more there are).
    #[must_use]
    pub fn notices_folded(&self) -> usize {
        self.toasts.folded()
    }

    /// Where the button that unfolds the notices ("2 more") or folds them
    /// again was drawn last, while there is one.
    #[must_use]
    pub fn notices_fold_rect(&self) -> Option<Rect> {
        self.toasts.fold_rect()
    }

    /// The band the notices took in the last frame: from the top of the
    /// highest to the bottom of their room, across the room. `None` when
    /// none showed.
    #[must_use]
    pub fn notices_area(&self) -> Option<Rect> {
        self.toasts.area()
    }

    /// The layer the notices were drawn on in the last frame: under the
    /// dialogs' while a modal dialog is open, over everything else
    /// otherwise.
    #[must_use]
    pub fn notices_layer(&self) -> Option<egui::LayerId> {
        self.toasts.layer()
    }

    /// Where the notice of the last saved file and its buttons were drawn
    /// last, while it shows (scripted runs and tests).
    #[must_use]
    pub fn saved_notice_rects(&self) -> Option<crate::ui::ToastRects> {
        self.revealed
            .last()
            .and_then(|(id, _)| self.toasts.rects(*id))
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
    }
}
