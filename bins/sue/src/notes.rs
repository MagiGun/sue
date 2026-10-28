//! Notes: a persistent scratchpad in the left drawer. The note is one text
//! entity in the arena, edited in place in a multiline `egui::TextEdit`
//! (select, copy, paste, undo through the global tree) and saved to the
//! notes folder by the runtime's autosave.

use egui::{FontId, RichText, Ui};
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_wm::{AppletContext, AppletManifest, AppletRole, ArenaText, SueApplet};

use crate::engine::NOTE_NAME;

/// The applet id.
pub const ID: &str = "notes";

/// The scratchpad.
#[derive(Debug, Default)]
pub struct Notes;

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
        let muted = cx.theme.muted;
        let Some(handle) = cx
            .arena()
            .iter()
            .find(|(_, entity)| entity.name() == NOTE_NAME)
            .map(|(handle, _)| handle)
        else {
            ui.label(RichText::new("The note could not be loaded.").color(muted));
            return;
        };
        let mut ops = Vec::new();
        if let Ok(entity) = cx.world.arena.get_mut(handle)
            && let Some(buffer) = entity.string_buffer_mut()
        {
            let mut text = ArenaText::new(buffer, &mut ops);
            let rows = ((ui.available_height() / (size * 1.35)).floor() as usize).max(4);
            egui::TextEdit::multiline(&mut text)
                .id(egui::Id::unique("sue-notes"))
                .hint_text("Type anything. Select, copy and paste work as usual.")
                .font(FontId::proportional(size))
                .desired_width(f32::INFINITY)
                .desired_rows(rows)
                .lock_focus(true)
                .show(ui);
        }
        if !ops.is_empty() {
            cx.record_typing(handle, ops);
        }
    }
}
