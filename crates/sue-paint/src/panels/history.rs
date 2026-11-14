//! The History tab.

use egui::{Align2, Rect, Sense, Ui, vec2};
use sue_wm::{AppletContext, Theme};

use crate::panels::child;
use crate::studio::SuePaint;

impl SuePaint {
    /// History, as Clip Studio Paint lists it: the start, then every step
    /// to the current one (lit) and the steps undone after it (dimmed); a
    /// click on any of them goes back or forward to it.
    pub(super) fn history_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use sue_core::NodeId;
        let history = &*cx.history;
        let cursor = history.cursor();
        let mut done = Vec::new();
        let mut node = cursor;
        while node != NodeId::ROOT {
            done.push(node);
            node = history.parent(node).unwrap_or(NodeId::ROOT);
        }
        done.reverse();
        let mut undone = Vec::new();
        let mut next = history.preferred_child(cursor);
        while let Some(node) = next {
            undone.push(node);
            next = history.preferred_child(node);
        }
        let start = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map_or_else(|| tr!("picture"), |entity| entity.name().to_owned());
        let rows: Vec<(NodeId, String, bool)> = std::iter::once((NodeId::ROOT, start, true))
            .chain(done.iter().map(|node| {
                (
                    *node,
                    history.label(*node).unwrap_or_default().to_owned(),
                    true,
                )
            }))
            .chain(undone.iter().map(|node| {
                (
                    *node,
                    history.label(*node).unwrap_or_default().to_owned(),
                    false,
                )
            }))
            .collect();
        let mut go = None;
        let mut inner = child(ui, body, "layer-history");
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-history")
            .stick_to_bottom(true)
            .show(&mut inner, |ui| {
                for (node, label, applied) in &rows {
                    let (row, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                    let painter = ui.painter();
                    if *node == cursor {
                        painter.rect_filled(row, 0.0, theme.active);
                    } else if response.hovered() {
                        painter.rect_filled(row, 0.0, theme.hover);
                    }
                    painter.hline(row.x_range(), row.bottom(), theme.hairline());
                    painter.text(
                        row.left_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        label,
                        theme.font(theme.small_size + 0.5),
                        if *applied {
                            theme.text
                        } else {
                            theme.muted.gamma_multiply(0.6)
                        },
                    );
                    if response.clicked() && *node != cursor {
                        go = Some(*node);
                    }
                }
            });
        if let Some(node) = go {
            cx.typing.flush(cx.history);
            if let Err(error) = cx.history.checkout(cx.world, node) {
                cx.fail(tr!("undo-failed", error = error));
            }
        }
    }
}
