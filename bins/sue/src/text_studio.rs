//! Text Studio: a code and prose editor over an arena `StringBuffer`. Every
//! keystroke is an undoable transaction.

use sue_core::txn::{DeleteText, InsertText};
use sue_core::{AssetHandle, StringBuffer};
use sue_platform::{CursorShape, InputEvent, Key, NamedKey, Point};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, FontFace,
    InputResponse, MediaClass, Rect, SueApplet, Ui, World, Zone, widget_id,
};

/// The applet id.
pub const ID: &str = "text-studio";

/// A caret-based editor.
#[derive(Debug, Default)]
pub struct TextStudio {
    doc: Option<AssetHandle>,
    caret: usize,
    scroll: f32,
}

/// Byte offsets where each line starts.
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(index, _)| index + 1))
        .collect()
}

/// `(line, byte column)` of byte `index`.
fn locate(starts: &[usize], index: usize) -> (usize, usize) {
    let line = starts
        .partition_point(|&start| start <= index)
        .saturating_sub(1);
    (line, index - starts[line])
}

/// The nearest character boundary at or before `index`.
fn floor_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

impl TextStudio {
    fn text<'w>(&self, world: &'w World) -> &'w str {
        self.doc
            .and_then(|doc| world.arena.get(doc).ok())
            .and_then(|entity| entity.string_buffer())
            .map_or("", StringBuffer::as_str)
    }

    /// Moves the caret for a navigation key; `None` if the key is not one.
    fn navigate(text: &str, caret: usize, key: Key) -> Option<usize> {
        let starts = line_starts(text);
        let (line, column) = locate(&starts, caret);
        let line_end = |line: usize| starts.get(line + 1).map_or(text.len(), |next| next - 1);
        Some(match key {
            Key::Named(NamedKey::ArrowLeft) => text[..caret]
                .char_indices()
                .next_back()
                .map_or(0, |(index, _)| index),
            Key::Named(NamedKey::ArrowRight) => text[caret..]
                .chars()
                .next()
                .map_or(caret, |ch| caret + ch.len_utf8()),
            Key::Named(NamedKey::Home) => starts[line],
            Key::Named(NamedKey::End) => line_end(line),
            Key::Named(NamedKey::ArrowUp) if line > 0 => {
                floor_boundary(text, (starts[line - 1] + column).min(line_end(line - 1)))
            }
            Key::Named(NamedKey::ArrowDown) if line + 1 < starts.len() => {
                floor_boundary(text, (starts[line + 1] + column).min(line_end(line + 1)))
            }
            Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown) => caret,
            _ => return None,
        })
    }
}

impl SueApplet for TextStudio {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Text Studio id is valid"),
            name: "Text Studio".to_owned(),
            category: AppletCategory::Studio,
            home_zone: Zone::CenterStage,
            uses_timeline: false,
            summary: "Edit text and code with full undo: the seed of the IDE.".to_owned(),
            opens: vec![MediaClass::Text],
            accepts_drops: false,
        }
    }

    fn mount(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        if self.doc.is_none() {
            self.doc = Some(ctx.world.arena.spawn("Untitled.txt", StringBuffer::new()));
        }
        Ok(())
    }

    fn open(&mut self, asset: AssetHandle, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        ctx.arena()
            .get(asset)?
            .string_buffer()
            .ok_or_else(|| AppletError::new("the asset is not text"))?;
        self.doc = Some(asset);
        self.caret = 0;
        self.scroll = 0.0;
        Ok(())
    }

    fn title(&self, world: &World) -> Option<String> {
        let entity = world.arena.get(self.doc?).ok()?;
        Some(format!("Text Studio — {}", entity.name()))
    }

    fn handle_input(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        let (Some(doc), InputEvent::Keyboard(key)) = (self.doc, event) else {
            return Ok(InputResponse::Ignored);
        };
        if !key.pressed || key.modifiers.is_command() {
            return Ok(InputResponse::Ignored);
        }
        let text = self.text(ctx.world);
        let caret = floor_boundary(text, self.caret);
        if let Some(next) = Self::navigate(text, caret, key.key) {
            self.caret = next;
            return Ok(InputResponse::Consumed { redraw: true });
        }
        match key.key {
            Key::Named(NamedKey::Backspace) => {
                if let Some((previous, _)) = text[..caret].char_indices().next_back() {
                    ctx.edit_arena(DeleteText::new(doc, previous..caret))?;
                    self.caret = previous;
                }
            }
            Key::Named(NamedKey::Delete) => {
                if let Some(ch) = text[caret..].chars().next() {
                    ctx.edit_arena(DeleteText::new(doc, caret..caret + ch.len_utf8()))?;
                }
            }
            Key::Named(NamedKey::Enter) => {
                ctx.edit_arena(InsertText::new(doc, caret, "\n"))?;
                self.caret = caret + 1;
            }
            _ => match &key.text {
                Some(typed) if !typed.is_empty() && !typed.chars().any(char::is_control) => {
                    let length = typed.len();
                    ctx.edit_arena(InsertText::new(doc, caret, typed.as_str()))?;
                    self.caret = caret + length;
                }
                _ => return Ok(InputResponse::Ignored),
            },
        }
        Ok(InputResponse::Consumed { redraw: true })
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let text = self.text(ctx.world);
        self.caret = floor_boundary(text, self.caret);
        let size = theme.mono_size;
        let line = ui.fonts.line_height(FontFace::Mono, size);
        let starts = line_starts(text);
        let digits = starts.len().to_string().len().max(3);
        let gutter =
            ui.fonts.measure(FontFace::Mono, size, &"0".repeat(digits)) + theme.padding() * 2.0;
        let content_height = starts.len() as f32 * line;
        self.scroll = (self.scroll - ui.wheel_in(area))
            .clamp(0.0, (content_height - area.height + line).max(0.0));

        let response = ui.interact(widget_id("text-studio", "page"), area);
        if response.hovered {
            ui.set_cursor(CursorShape::Text);
        }
        if response.pressed
            && let Some(pointer) = ui.input.pointer
        {
            let row = (((pointer.y - area.y + self.scroll) / line).floor().max(0.0) as usize)
                .min(starts.len() - 1);
            let end = starts.get(row + 1).map_or(text.len(), |next| next - 1);
            let line_text = &text[starts[row]..end];
            let column =
                ui.fonts
                    .hit_index(FontFace::Mono, size, line_text, pointer.x - area.x - gutter);
            self.caret = starts[row] + column;
        }

        ui.canvas.draw_box(
            Rect::new(area.x, area.y, gutter - theme.padding(), area.height),
            BoxStyle::filled(theme.background.fade(0.6), theme.radius / 2.0),
        );
        let (caret_line, caret_column) = locate(&starts, self.caret);
        let first = (self.scroll / line).floor() as usize;
        let visible = (area.height / line).ceil() as usize + 1;
        for row in first..(first + visible).min(starts.len()) {
            let y = area.y + row as f32 * line - self.scroll;
            let end = starts.get(row + 1).map_or(text.len(), |next| next - 1);
            let line_text = &text[starts[row]..end];
            if row == caret_line && ctx.focused {
                ui.canvas.draw_box(
                    Rect::new(
                        area.x + gutter - theme.padding() / 2.0,
                        y,
                        area.width - gutter,
                        line,
                    ),
                    BoxStyle::filled(theme.raised.fade(0.6), 0.0),
                );
            }
            let number = format!("{:>digits$}", row + 1);
            ui.text(
                Point::new(area.x + theme.padding() / 2.0, y),
                &number,
                FontFace::Mono,
                size,
                theme.muted,
            );
            ui.canvas.push_clip(Rect::new(
                area.x + gutter,
                area.y,
                area.width - gutter,
                area.height,
            ));
            ui.text(
                Point::new(area.x + gutter, y),
                line_text,
                FontFace::Mono,
                size,
                theme.text,
            );
            if row == caret_line && ctx.focused {
                let x = area.x
                    + gutter
                    + ui.fonts
                        .caret_x(FontFace::Mono, size, line_text, caret_column);
                ui.canvas.draw_box(
                    Rect::new(x, y, 1.5, line),
                    BoxStyle::filled(theme.accent, 0.0),
                );
            }
            ui.canvas.pop_clip();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_columns() {
        let starts = line_starts("ab\ncde\n");
        assert_eq!(starts, [0, 3, 7]);
        assert_eq!(locate(&starts, 4), (1, 1));
        assert_eq!(locate(&starts, 7), (2, 0));
    }

    #[test]
    fn navigation_respects_lines_and_utf8() {
        let text = "héllo\nab";
        let named = |key| Key::Named(key);
        assert_eq!(
            TextStudio::navigate(text, 3, named(NamedKey::ArrowLeft)),
            Some(1)
        );
        assert_eq!(
            TextStudio::navigate(text, 1, named(NamedKey::ArrowRight)),
            Some(3)
        );
        assert_eq!(
            TextStudio::navigate(text, 4, named(NamedKey::ArrowDown)),
            Some(9)
        );
        assert_eq!(
            TextStudio::navigate(text, 8, named(NamedKey::ArrowUp)),
            Some(1)
        );
        assert_eq!(
            TextStudio::navigate(text, 8, named(NamedKey::Home)),
            Some(7)
        );
        assert_eq!(TextStudio::navigate(text, 0, named(NamedKey::End)), Some(6));
        assert_eq!(TextStudio::navigate(text, 0, Key::Character('x')), None);
    }
}
