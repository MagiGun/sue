//! History: the global undo tree in the right drawer. Every change (typing,
//! a moved entity, a paint stroke, a Settings slider) is a node; branches
//! appear where you undid and then did something else. The newest change
//! is at the top. Click any node to go back or forward to it.

use egui::{RichText, Sense, Ui, vec2};
use sue_core::{NodeId, UndoStack};
use sue_wm::{AppletContext, AppletManifest, AppletRole, SueApplet, World};

/// The applet id.
pub const ID: &str = "history";

/// The undo tree view.
#[derive(Debug, Default)]
pub struct History;

/// Depth-first rows with their indent, oldest first: a branch indents one
/// step.
pub fn rows(history: &UndoStack<World>) -> Vec<(NodeId, usize)> {
    let mut indents: Vec<(NodeId, usize)> = Vec::new();
    for node in history.walk() {
        let indent = match history.parent(node) {
            None => 0,
            Some(parent) => {
                let parent_indent = indents
                    .iter()
                    .find(|(seen, _)| *seen == parent)
                    .map_or(0, |(_, indent)| *indent);
                let first = history.children(parent).first() == Some(&node);
                parent_indent + usize::from(!first)
            }
        };
        indents.push((node, indent));
    }
    indents
}

impl SueApplet for History {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "History", AppletRole::RightDrawer, "\u{1F552}");
        manifest.summary = "Every change as a tree; click to travel.".to_owned();
        manifest.order = 20;
        manifest
    }

    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        cx.typing.flush(cx.history);
        let cursor = cx.history.cursor();
        let mut checkout = None;
        // Newest at the top, the opened project at the bottom.
        for (node, indent) in rows(cx.history).into_iter().rev() {
            let label = cx.history.label(node).unwrap_or("").to_owned();
            let applied = cx.history.is_applied(node);
            let here = node == cursor;
            let (rect, response) = ui.allocate_exact_size(
                vec2(ui.available_width(), theme.ui_size * 1.8),
                Sense::click(),
            );
            if here || response.hovered() {
                ui.painter()
                    .rect_filled(rect, 4.0, if here { theme.active } else { theme.hover });
            }
            let x = rect.left() + 8.0 + indent as f32 * 14.0;
            ui.painter().circle_filled(
                egui::pos2(x, rect.center().y),
                3.0,
                if applied { theme.text } else { theme.muted },
            );
            ui.painter().text(
                egui::pos2(x + 10.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                &label,
                theme.font(theme.ui_size),
                if applied { theme.text } else { theme.muted },
            );
            if response.clicked() && !here {
                checkout = Some(node);
            }
        }
        if cx.history.len() <= 1 {
            ui.add_space(theme.padding);
            ui.label(RichText::new("Changes you make show up here.").color(theme.muted));
        }
        if let Some(node) = checkout
            && let Err(error) = cx.history.checkout(cx.world, node)
        {
            cx.fail(format!("Could not travel there: {error}"));
        }
    }
}
