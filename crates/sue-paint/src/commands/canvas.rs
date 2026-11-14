//! Edit, Rotate/Flip Canvas: the whole canvas turned or mirrored.

use sue_wm::{AppletContext, Call, Command};

use crate::doc;
use crate::studio::SuePaint;

/// Does what a Canvas command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_canvas(call.name, cx);
}

impl SuePaint {
    /// Does what a Canvas command does.
    fn run_canvas(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "canvas.flip_h" => self.transform_canvas(cx, true, false, &tr!("flip-canvas")),
            "canvas.flip_v" => self.transform_canvas(cx, false, true, &tr!("flip-canvas")),
            "canvas.rotate180" => self.transform_canvas(cx, true, true, &tr!("rotate-canvas")),
            "canvas.rotate_left" | "canvas.rotate_right" => {
                if let Some(picture) = self.picture() {
                    match doc::rotate_canvas(
                        &cx.world.arena,
                        picture,
                        command == "canvas.rotate_right",
                    ) {
                        Ok(edit) => {
                            if Self::apply(cx, edit)
                                && let Some(doc) = self.doc_mut()
                            {
                                doc.set_selection(None);
                                doc.view.fitted = false;
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "canvas", run).state(super::doc_open)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("canvas.rotate_left", "rotate-90-degrees-left"),
    c("canvas.rotate_right", "rotate-90-degrees-right"),
    c("canvas.rotate180", "rotate-180-degrees"),
    c("canvas.flip_h", "flip-horizontal"),
    c("canvas.flip_v", "flip-vertical"),
];
