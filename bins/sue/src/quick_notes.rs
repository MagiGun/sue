//! Quick Notes: one scratch note in the arena, edited only through
//! transactions on the global undo tree, and autosaved by the runtime.

use sue_core::txn::{DeleteText, InsertText};
use sue_core::{AssetHandle, StringBuffer};
use sue_platform::{CursorShape, InputEvent, Key, NamedKey, Point};
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, FontFace, InputResponse,
    Rect, SueApplet, Ui, Zone, widget_id,
};

use crate::engine::NOTE_NAME;

/// The applet id.
pub const ID: &str = "quick-notes";

const FONT_SIZE: &str = "quick_notes.font_size";

/// A scratch note in the Left Dock.
#[derive(Debug, Default)]
pub struct QuickNotes {
    note: Option<AssetHandle>,
    scroll: f32,
}

impl SueApplet for QuickNotes {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Quick Notes id is valid"),
            name: "Notes".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::LeftDock,
            uses_timeline: false,
            summary: "Jot something down without leaving what you're doing. Saved automatically."
                .to_owned(),
            opens: Vec::new(),
            icon: "\u{1F4DD}".to_owned(),
            singleton: false,
        }
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        vec![RiceDeclaration::new(
            FONT_SIZE,
            RiceParameter::IntRange {
                value: 14,
                min: Some(8),
                max: Some(48),
            },
            "Note text size (px)",
        )]
    }

    fn mount(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        let existing = ctx
            .arena()
            .iter()
            .find(|(_, entity)| entity.name() == NOTE_NAME && entity.string_buffer().is_some())
            .map(|(handle, _)| handle);
        self.note =
            Some(existing.unwrap_or_else(|| ctx.world.arena.spawn(NOTE_NAME, StringBuffer::new())));
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
            .arena()
            .get(note)?
            .string_buffer()
            .map_or("", StringBuffer::as_str);
        let end = text.len();
        match key.key {
            Key::Named(NamedKey::Backspace) => {
                let Some((last, _)) = text.char_indices().next_back() else {
                    return Ok(InputResponse::Consumed { redraw: false });
                };
                ctx.edit_arena(DeleteText::new(note, last..end))?;
            }
            Key::Named(NamedKey::Enter) => {
                ctx.edit_arena(InsertText::new(note, end, "\n"))?;
            }
            _ => match &key.text {
                Some(typed) if !typed.is_empty() && !typed.chars().any(char::is_control) => {
                    ctx.edit_arena(InsertText::new(note, end, typed.as_str()))?;
                }
                _ => return Ok(InputResponse::Ignored),
            },
        }
        Ok(InputResponse::Consumed { redraw: true })
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let size = ctx.rice().get_f64(FONT_SIZE).unwrap_or(14.0) as f32;
        let text = self
            .note
            .and_then(|note| ctx.arena().get(note).ok())
            .and_then(|entity| entity.string_buffer())
            .map_or("", StringBuffer::as_str);
        let response = ui.interact(widget_id("quick-notes", "page"), area);
        if response.hovered {
            ui.set_cursor(CursorShape::Text);
        }
        let line = ui.fonts.line_height(FontFace::Ui, size);
        let lines = ui.fonts.wrap(FontFace::Ui, size, text, area.width);
        let content = lines.len() as f32 * line;
        self.scroll =
            (self.scroll - ui.wheel_in(area)).clamp(0.0, (content - area.height).max(0.0));
        if text.is_empty() {
            ui.text(
                Point::new(area.x, area.y),
                "Type a note. It saves itself.",
                FontFace::Ui,
                size,
                theme.muted,
            );
        }
        for (row, range) in lines.iter().enumerate() {
            let y = area.y + row as f32 * line - self.scroll;
            ui.text(
                Point::new(area.x, y),
                &text[range.clone()],
                FontFace::Ui,
                size,
                theme.text,
            );
        }
        if ctx.focused {
            let last = lines.last().cloned().unwrap_or(0..0);
            let x = area.x + ui.fonts.measure(FontFace::Ui, size, &text[last]);
            let y = area.y + (lines.len().max(1) - 1) as f32 * line - self.scroll;
            ui.caret(Rect::new(x + 1.0, y, 1.5, line));
        }
    }
}
