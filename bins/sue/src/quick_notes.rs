//! Quick Notes: the reference applet. It keeps one note in the arena and edits
//! it only through transactions on the shared undo tree.

use sue_core::txn::{DeleteText, InsertText};
use sue_core::{AssetHandle, StringBuffer};
use sue_platform::{InputEvent, Key, NamedKey};
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, InputResponse, SueApplet,
    Zone,
};

/// The applet id.
pub const ID: &str = "quick-notes";

/// Name of the arena entity holding the note.
pub const NOTE_NAME: &str = "Quick Note";

/// A single scratch note in the Left Dock.
#[derive(Debug, Default)]
pub struct QuickNotes {
    note: Option<AssetHandle>,
}

impl SueApplet for QuickNotes {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Quick Notes id is valid"),
            name: "Quick Notes".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::LeftDock,
            uses_timeline: false,
            summary: "Jot something down without leaving what you're doing.".to_owned(),
        }
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        vec![RiceDeclaration::new(
            "quick_notes.font_size",
            RiceParameter::IntRange {
                value: 15,
                min: Some(8),
                max: Some(48),
            },
            "Note text size (pt)",
        )]
    }

    fn mount(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        if self.note.is_none_or(|note| !ctx.arena.contains(note)) {
            self.note = Some(ctx.arena.spawn(NOTE_NAME, StringBuffer::new()));
        }
        Ok(())
    }

    fn handle_input(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        let (Some(note), InputEvent::Keyboard(key)) = (self.note, event) else {
            return Ok(InputResponse::Ignored);
        };
        if !key.pressed || key.modifiers.is_command() {
            return Ok(InputResponse::Ignored);
        }
        let text = ctx
            .arena
            .get(note)?
            .string_buffer()
            .map_or("", StringBuffer::as_str);
        let end = text.len();
        match key.key {
            Key::Named(NamedKey::Backspace) => {
                let Some((last, _)) = text.char_indices().next_back() else {
                    return Ok(InputResponse::Consumed { redraw: false });
                };
                ctx.history
                    .apply(ctx.arena, DeleteText::new(note, last..end))?;
            }
            Key::Named(NamedKey::Enter) => {
                ctx.history
                    .apply(ctx.arena, InsertText::new(note, end, "\n"))?;
            }
            _ => match &key.text {
                Some(typed) if !typed.is_empty() && !typed.chars().any(char::is_control) => {
                    ctx.history
                        .apply(ctx.arena, InsertText::new(note, end, typed.as_str()))?;
                }
                _ => return Ok(InputResponse::Ignored),
            },
        }
        Ok(InputResponse::Consumed { redraw: true })
    }
}
