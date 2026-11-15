//! The History palette.

use egui::{Rect, Ui};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, ListRow};
use sue_wm::{AppletContext, Theme};

use crate::panels::child;
use crate::studio::SuePaint;

impl SuePaint {
    /// History, as Clip Studio Paint lists it: the start, then every step
    /// to the current one (chosen) and the steps undone after it (marked
    /// with the redo arrow and "Undone"); a click on any of them goes back
    /// or forward to it. A long history costs only the rows in view.
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
        let height = ListRow::height(theme, Density::of(&inner), false);
        let undone_note = tr!("history-undone");
        sue_wm::ui::ScrollView::vertical("sue-paint-history")
            .on(theme.panel)
            .stick_to_bottom(true)
            .show_rows(&mut inner, theme, height, rows.len(), |ui, range| {
                for (node, label, applied) in &rows[range] {
                    let mut row = ListRow::new(("history", node), label).selected(*node == cursor);
                    if !*applied {
                        row = row.icon(Icon::Redo).note(&undone_note);
                    }
                    if row.show(ui, theme).response.clicked() && *node != cursor {
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
