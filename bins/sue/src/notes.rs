//! Notes: a persistent scratchpad in the left drawer. The note is one text
//! entity in the arena, edited in place in a multiline `egui::TextEdit`
//! (select, copy, paste, undo through the global tree) and saved to the
//! notes folder by the runtime's autosave.
//!
//! A toolbar above it undoes and redoes, saves the note to a file, opens a
//! text file into it, copies it, clears it and changes its text size, and a
//! line under it counts words and characters. Dragging a selection past the
//! top or bottom of the note scrolls it, so the selection can grow.

use std::path::Path;
use std::time::Duration;

use egui::{FontId, RichText, ScrollArea, TextBuffer, Ui};
use sue_rice::{RiceDeclaration, RiceParameter, RiceValue, SetRiceValue};
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, ArenaText, FilePick, ShellRequest,
    SueApplet, widgets,
};

use crate::engine::NOTE_NAME;

/// The applet id.
pub const ID: &str = "notes";

/// How close to an edge, in points, a dragged selection starts scrolling.
const EDGE: f32 = 18.0;

/// The scratchpad.
#[derive(Debug, Default)]
pub struct Notes {
    /// Where to scroll the note next frame, while a selection is dragged
    /// past its edge.
    scroll_to: Option<f32>,
}

/// How far to scroll this frame for a pointer at `y` while dragging in a
/// view from `top` to `bottom`: faster the further past the edge.
#[must_use]
pub fn edge_scroll(y: f32, top: f32, bottom: f32) -> f32 {
    if y > bottom - EDGE {
        (y - (bottom - EDGE)).min(160.0) * 0.35
    } else if y < top + EDGE {
        (y - (top + EDGE)).max(-160.0) * 0.35
    } else {
        0.0
    }
}

/// Words and characters in `text`.
#[must_use]
pub fn counts(text: &str) -> (usize, usize) {
    (text.split_whitespace().count(), text.chars().count())
}

impl Notes {
    fn handle(cx: &AppletContext<'_>) -> Option<sue_core::AssetHandle> {
        cx.arena()
            .iter()
            .find(|(_, entity)| entity.name() == NOTE_NAME)
            .map(|(handle, _)| handle)
    }

    fn text(cx: &AppletContext<'_>) -> String {
        Self::handle(cx)
            .and_then(|handle| cx.arena().get(handle).ok())
            .and_then(|entity| {
                entity
                    .string_buffer()
                    .map(|buffer| buffer.as_str().to_owned())
            })
            .unwrap_or_default()
    }

    /// Replaces the whole note with `text`, as one undoable change.
    fn replace(cx: &mut AppletContext<'_>, text: &str) {
        let Some(handle) = Self::handle(cx) else {
            return;
        };
        let mut ops = Vec::new();
        if let Ok(entity) = cx.world.arena.get_mut(handle)
            && let Some(buffer) = entity.string_buffer_mut()
        {
            let mut edit = ArenaText::new(buffer, &mut ops);
            let length = edit.as_str().chars().count();
            edit.delete_char_range(egui::text::CharIndex(0)..egui::text::CharIndex(length));
            edit.insert_text(text, egui::text::CharIndex(0));
        }
        if !ops.is_empty() {
            cx.record_typing(handle, ops);
            cx.typing.flush(cx.history);
        }
    }

    fn me() -> Option<AppletId> {
        AppletId::new(ID).ok()
    }

    fn toolbar(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let size = cx.rice().get_i64("notes.font_size").unwrap_or(14);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(3.0, 4.0);
            if widgets::pill_button(ui, "Undo", false, &theme)
                .on_hover_text("Undo (Ctrl+Z)")
                .clicked()
            {
                match cx.undo(false) {
                    Ok(false) => cx.notify("Nothing to undo"),
                    Ok(true) => {}
                    Err(error) => cx.fail(format!("Undo failed: {error}")),
                }
            }
            if widgets::pill_button(ui, "Redo", false, &theme)
                .on_hover_text("Redo (Ctrl+Y)")
                .clicked()
            {
                match cx.undo(true) {
                    Ok(false) => cx.notify("Nothing to redo"),
                    Ok(true) => {}
                    Err(error) => cx.fail(format!("Redo failed: {error}")),
                }
            }
            if widgets::pill_button(ui, "Save As", false, &theme)
                .on_hover_text("Save the note to a text file")
                .clicked()
            {
                cx.request(ShellRequest::PickFile(
                    FilePick::save(Self::me(), "save", "Save the Note", "Note.txt")
                        .extensions(&["txt", "md"]),
                ));
            }
            if widgets::pill_button(ui, "Open", false, &theme)
                .on_hover_text("Put a text file into the note (Undo brings the old note back)")
                .clicked()
            {
                cx.request(ShellRequest::PickFile(
                    FilePick::open(Self::me(), "open", "Open a Text File into the Note")
                        .extensions(&["txt", "md", "csv", "json", "log", "ini", "toml"]),
                ));
            }
            if widgets::pill_button(ui, "Copy All", false, &theme).clicked() {
                ui.ctx().copy_text(Self::text(cx));
                cx.notify("Copied the note");
            }
            if widgets::pill_button(ui, "Clear", false, &theme)
                .on_hover_text("Empty the note (Undo brings it back)")
                .clicked()
            {
                Self::replace(cx, "");
            }
            for (label, step, tip) in [("A-", -1, "Smaller text"), ("A+", 1, "Larger text")] {
                if widgets::pill_button(ui, label, false, &theme)
                    .on_hover_text(tip)
                    .clicked()
                {
                    let next = (size + step).clamp(8, 48);
                    if next != size
                        && let Err(error) =
                            cx.edit_rice(SetRiceValue::new("notes.font_size", RiceValue::Int(next)))
                    {
                        cx.fail(error.to_string());
                    }
                }
            }
        });
    }
}

impl SueApplet for Notes {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Notes", AppletRole::LeftDrawer, "\u{1F4DD}");
        manifest.summary = "Jot something down; it saves itself.".to_owned();
        manifest.order = 40;
        manifest
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        vec![RiceDeclaration::new(
            "notes.font_size",
            RiceParameter::IntRange {
                value: 14,
                min: Some(8),
                max: Some(48),
            },
            "Note text size (px)",
        )]
    }

    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let size = cx.rice().get_i64("notes.font_size").unwrap_or(14) as f32;
        let theme = cx.theme.clone();
        let Some(handle) = Self::handle(cx) else {
            ui.label(RichText::new("The note could not be loaded.").color(theme.muted));
            return;
        };
        self.toolbar(ui, cx);
        ui.add_space(theme.padding * 0.5);
        let footer = theme.small_size * 2.0;
        let height = (ui.available_height() - footer).max(120.0);
        let mut ops = Vec::new();
        let mut area = ScrollArea::vertical()
            .id_salt("sue-notes-scroll")
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false]);
        if let Some(offset) = self.scroll_to.take() {
            area = area.vertical_scroll_offset(offset);
        }
        let output = area.show(ui, |ui| {
            let mut response = None;
            if let Ok(entity) = cx.world.arena.get_mut(handle)
                && let Some(buffer) = entity.string_buffer_mut()
            {
                let mut text = ArenaText::new(buffer, &mut ops);
                let rows = ((height / (size * 1.35)).floor() as usize).max(4);
                response = Some(
                    egui::TextEdit::multiline(&mut text)
                        .id(egui::Id::unique("sue-notes"))
                        .hint_text("Type anything. Select, copy and paste work as usual.")
                        .font(FontId::proportional(size))
                        .desired_width(f32::INFINITY)
                        .desired_rows(rows)
                        .lock_focus(true)
                        .show(ui)
                        .response,
                );
            }
            response
        });
        // A selection dragged past the top or bottom scrolls the note.
        if let Some(response) = output.inner.as_ref()
            && response.dragged()
            && let Some(pointer) = ui.ctx().pointer_interact_pos()
        {
            let view = output.inner_rect;
            let step = edge_scroll(pointer.y, view.top(), view.bottom());
            if step != 0.0 {
                let most = (output.content_size.y - view.height()).max(0.0);
                let next = (output.state.clamped_offset().y + step).clamp(0.0, most);
                if (next - output.state.clamped_offset().y).abs() > 0.01 {
                    self.scroll_to = Some(next);
                    ui.ctx().request_repaint_after(Duration::from_millis(16));
                }
            }
        }
        if !ops.is_empty() {
            cx.record_typing(handle, ops);
        }
        let (words, characters) = counts(&Self::text(cx));
        ui.label(
            RichText::new(format!("{words} words, {characters} characters"))
                .size(theme.small_size)
                .color(theme.muted),
        );
    }

    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        match pick.purpose.as_str() {
            "save" => {
                let text = Self::text(cx);
                match sue_platform::write_atomic(path, text.as_bytes()) {
                    Ok(()) => cx.saved(path, "note"),
                    Err(error) => cx.fail(format!("Cannot save the note: {error}")),
                }
            }
            "open" => match std::fs::read(path) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => Self::replace(cx, &text),
                    Err(_) => cx.fail("That file is not text"),
                },
                Err(error) => cx.fail(format!("Cannot read it: {error}")),
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_past_an_edge_scrolls_faster_further_out() {
        assert_eq!(edge_scroll(200.0, 0.0, 400.0), 0.0, "inside: still");
        let near = edge_scroll(395.0, 0.0, 400.0);
        let far = edge_scroll(480.0, 0.0, 400.0);
        assert!(near > 0.0 && far > near, "down, faster further out");
        assert!(edge_scroll(-30.0, 0.0, 400.0) < 0.0, "above: up");
        assert_eq!(counts("two words\n"), (2, 10));
    }
}
