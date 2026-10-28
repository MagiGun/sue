//! Quick Notes: one scratch note in the arena, edited only through
//! transactions on the global undo tree.

use sue_core::txn::{DeleteText, InsertText};
use sue_core::{AssetHandle, StringBuffer};
use sue_platform::{InputEvent, Key, NamedKey, Point};
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, FontFace,
    InputResponse, Rect, SueApplet, Ui, Zone,
};

/// The applet id.
pub const ID: &str = "quick-notes";

/// Name of the arena entity holding the note; every instance shares it.
pub const NOTE_NAME: &str = "Quick Note";

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
            name: "Quick Notes".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::LeftDock,
            uses_timeline: false,
            summary: "Jot something down without leaving what you're doing.".to_owned(),
            opens: Vec::new(),
            accepts_drops: false,
        }
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        vec![RiceDeclaration::new(
            FONT_SIZE,
            RiceParameter::IntRange {
                value: 15,
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
        let size = ctx.rice().get_f64(FONT_SIZE).unwrap_or(15.0) as f32;
        let text = self
            .note
            .and_then(|note| ctx.arena().get(note).ok())
            .and_then(|entity| entity.string_buffer())
            .map_or("", StringBuffer::as_str);
        let border = if ctx.focused {
            theme.accent
        } else {
            theme.raised
        };
        ui.canvas.draw_box(
            area,
            BoxStyle {
                fill: theme.background,
                border,
                border_width: theme.border_width.max(1.0),
                radius: theme.radius / 2.0,
            },
        );
        let pad = theme.padding();
        let inner = area.shrink(pad);
        let line = ui.fonts.line_height(FontFace::Ui, size);
        let lines = ui.fonts.wrap(FontFace::Ui, size, text, inner.width);
        let content = lines.len() as f32 * line;
        self.scroll =
            (self.scroll - ui.wheel_in(area)).clamp(0.0, (content - inner.height).max(0.0));
        ui.canvas.push_clip(inner);
        if text.is_empty() {
            ui.text(
                Point::new(inner.x, inner.y),
                "Type a note…",
                FontFace::Ui,
                size,
                theme.muted,
            );
        }
        for (row, range) in lines.iter().enumerate() {
            let y = inner.y + row as f32 * line - self.scroll;
            ui.text(
                Point::new(inner.x, y),
                &text[range.clone()],
                FontFace::Ui,
                size,
                theme.text,
            );
        }
        if ctx.focused {
            let last = lines.last().cloned().unwrap_or(0..0);
            let x = inner.x + ui.fonts.measure(FontFace::Ui, size, &text[last]);
            let y = inner.y + (lines.len().max(1) - 1) as f32 * line - self.scroll;
            ui.canvas.draw_box(
                Rect::new(x + 1.0, y, 1.5, line),
                BoxStyle::filled(theme.accent, 0.0),
            );
        }
        ui.canvas.pop_clip();
    }
}
