//! The tool and brush keys: Clip Studio Paint's letters for each tool,
//! the sub tool steps, the colors and the brush size.

use sue_wm::Command;

use crate::studio::SuePaint;

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "tool", super::nothing).state(super::doc_open)
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
