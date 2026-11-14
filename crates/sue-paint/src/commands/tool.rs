//! The tool and brush keys: Clip Studio Paint's letters for each tool,
//! the sub tool steps, the colors and the brush size.

use sue_wm::{AppletContext, Call, Command};

use crate::color;
use crate::studio::SuePaint;

/// Does what a tool or brush key does when Auto Actions or Studio Mode's
/// keys run its command, as the key itself does.
fn run(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    paint.tool_key(call.name);
}

impl SuePaint {
    /// Runs a tool or brush key's command; `false` if `command` is not one.
    pub(crate) fn tool_key(&mut self, command: &str) -> bool {
        let letter = match command {
            "tool.zoom" => '/',
            "tool.hand" => 'H',
            "tool.operation" => 'O',
            "tool.move_layer" => 'K',
            "tool.selection" => 'M',
            "tool.auto_select" => 'W',
            "tool.eyedropper" => 'I',
            "tool.pen" => 'P',
            "tool.brush" => 'B',
            "tool.eraser" => 'E',
            "tool.blend" => 'J',
            "tool.fill" => 'G',
            "tool.figure" => 'U',
            "tool.text" => 'T',
            "tool.correct_line" => 'Y',
            _ => '\0',
        };
        if letter != '\0' {
            self.toolbox.cycle(letter);
            return true;
        }
        match command {
            // The hand's Rotate sub tool.
            "tool.rotate" => {
                self.toolbox.pick(crate::tools::Tool::Hand);
                self.toolbox.pick_sub(crate::tools::Tool::Hand, 1);
            }
            "tool.previous_sub" => self.toolbox.step_sub(false),
            "tool.next_sub" => self.toolbox.step_sub(true),
            "color.swap" => {
                std::mem::swap(&mut self.main, &mut self.sub);
                self.transparent = false;
                self.hex = color::hex(self.drawing_color());
            }
            // Between the drawing color and the transparent color.
            "color.transparent" => self.transparent = !self.transparent,
            "brush.smaller" | "brush.bigger" => {
                let tip = &mut self.toolbox.current_mut().tip;
                tip.size = if command == "brush.bigger" {
                    (tip.size * 1.2).max(tip.size + 1.0)
                } else {
                    (tip.size / 1.2).min(tip.size - 1.0)
                }
                .clamp(0.5, 2000.0);
            }
            _ => return false,
        }
        true
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "tool", run).state(super::doc_open)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("tool.zoom", "zoom-tool").keys("/"),
    c("tool.hand", "hand-tool").keys("H"),
    c("tool.rotate", "rotate-tool").keys("R"),
    c("tool.operation", "operation-tool").keys("O"),
    c("tool.move_layer", "move-layer-tool").keys("K"),
    c("tool.selection", "selection-tool").keys("M"),
    c("tool.auto_select", "auto-select-tool").keys("W"),
    c("tool.eyedropper", "eyedropper-tool").keys("I"),
    c("tool.pen", "pen-and-pencil-tools").keys("P"),
    c("tool.brush", "brush-airbrush-and-decoration-tools").keys("B"),
    c("tool.eraser", "eraser-tool").keys("E"),
    c("tool.blend", "blend-tool").keys("J"),
    c("tool.fill", "fill-and-gradient-tools").keys("G"),
    c("tool.figure", "figure-frame-border-ruler-tools").keys("U"),
    c("tool.text", "text-and-balloon-tools").keys("T"),
    c("tool.correct_line", "correct-line-tool").keys("Y"),
    c("tool.previous_sub", "previous-sub-tool").keys(","),
    c("tool.next_sub", "next-sub-tool").keys("."),
    c("color.swap", "swap-main-and-sub-color").keys("X"),
    c("color.transparent", "drawing-or-transparent-color").keys("C"),
    c("brush.smaller", "smaller-brush").keys("["),
    c("brush.bigger", "bigger-brush").keys("]"),
];
