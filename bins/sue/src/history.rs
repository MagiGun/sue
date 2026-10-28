//! History: the branching undo tree, live. Every row is a transaction; a
//! click jumps the whole world (arena and rice) to the state after it.
//! Branches that were undone and replaced stay listed, indented.

use sue_core::NodeId;
use sue_platform::{CursorShape, InputEvent};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, InputResponse,
    Rect, ShellRequest, SueApplet, Ui, World, Zone, widget_id,
};

/// The applet id.
pub const ID: &str = "history";

/// The undo tree in the Right Dock.
#[derive(Debug, Default)]
pub struct History {
    scroll: f32,
}

/// Rows in depth-first order with their indent: a node that is not its
/// parent's first child starts a branch, one level deeper.
fn rows(history: &sue_core::UndoStack<World>) -> Vec<(NodeId, usize)> {
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
        AppletManifest {
            id: AppletId::new(ID).expect("the History id is valid"),
            name: "History".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::RightDock,
            uses_timeline: false,
            summary: "Every change, as a tree you can jump around in.".to_owned(),
            opens: Vec::new(),
            icon: "\u{1F558}".to_owned(),
            singleton: false,
        }
    }

    fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        Ok(())
    }

    fn handle_input(
        &mut self,
        _event: &InputEvent,
        _ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        Ok(InputResponse::Ignored)
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let pad = theme.padding;
        let row = ui.row_height();
        let half = (area.width - pad / 2.0) / 2.0;
        let undo = Rect::new(area.x, area.y, half, row);
        let redo = Rect::new(undo.right() + pad / 2.0, area.y, half, row);
        let mut jump = None;
        if ui.button(widget_id("history", "undo"), undo, "Undo") && ctx.history.can_undo() {
            jump = ctx.history.parent(ctx.history.cursor());
        }
        if ui.button(widget_id("history", "redo"), redo, "Redo") {
            jump = ctx.history.preferred_child(ctx.history.cursor());
        }
        let list = Rect::new(
            area.x,
            area.y + row + pad,
            area.width,
            (area.height - row - pad).max(0.0),
        );
        let rows = rows(ctx.history);
        let content = rows.len() as f32 * row;
        self.scroll =
            (self.scroll - ui.wheel_in(list)).clamp(0.0, (content - list.height).max(0.0));
        let cursor = ctx.history.cursor();
        ui.canvas.push_clip(list);
        let indent_step = pad * 1.5;
        for (slot, (node, indent)) in rows.iter().enumerate() {
            let y = list.y + slot as f32 * row - self.scroll;
            if y + row < list.y || y > list.bottom() {
                continue;
            }
            let rect = Rect::new(list.x, y, list.width, row);
            let response = ui.interact(widget_id("history/node", &node.index().to_string()), rect);
            let current = *node == cursor;
            if current {
                ui.canvas
                    .draw_box(rect, BoxStyle::filled(theme.surface, theme.radius));
                ui.fill(
                    Rect::new(rect.x, rect.y, theme.tab_accent.max(1.0), rect.height),
                    theme.accent,
                );
            } else if response.hovered {
                ui.canvas
                    .draw_box(rect, BoxStyle::filled(theme.hover, theme.radius));
            }
            if response.hovered {
                ui.set_cursor(CursorShape::Pointer);
            }
            let applied = ctx.history.is_applied(*node);
            let color = if current {
                theme.text
            } else if applied {
                theme.muted.mix(theme.text, 0.5)
            } else {
                theme.muted.fade(0.7)
            };
            let x = rect.x + pad + *indent as f32 * indent_step;
            let label = ctx.history.label(*node).unwrap_or("").to_owned();
            ui.label(
                Rect::new(x, rect.y, (rect.right() - x - pad).max(0.0), row),
                &label,
                color,
            );
            if response.clicked && !current {
                jump = Some(*node);
            }
        }
        ui.canvas.pop_clip();
        if let Some(node) = jump
            && let Err(error) = ctx.history.checkout(ctx.world, node)
        {
            ctx.request(ShellRequest::Notify(format!("History: {error}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::txn::InsertText;
    use sue_core::{StringBuffer, UndoStack};
    use sue_wm::ArenaEdit;

    #[test]
    fn branches_indent_under_the_change_they_replaced() {
        let mut world = World::default();
        let note = world.arena.spawn("note", StringBuffer::new());
        let mut history = UndoStack::new();
        let a = history
            .apply(&mut world, ArenaEdit(InsertText::new(note, 0, "a")))
            .unwrap();
        let b = history
            .apply(&mut world, ArenaEdit(InsertText::new(note, 1, "b")))
            .unwrap();
        history.undo(&mut world).unwrap();
        let c = history
            .apply(&mut world, ArenaEdit(InsertText::new(note, 1, "c")))
            .unwrap();
        assert_eq!(rows(&history), [(NodeId::ROOT, 0), (a, 0), (b, 0), (c, 1)]);
        history.checkout(&mut world, b).unwrap();
        assert_eq!(
            world
                .arena
                .get(note)
                .unwrap()
                .string_buffer()
                .unwrap()
                .as_str(),
            "ab"
        );
    }
}
