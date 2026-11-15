//! The Edit menu: undo, redo, find and replace.

use sue_wm::{AppletContext, Call, Command};

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "edit", run)
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    // The shell's Undo and Redo keys run these.
    c("edit.undo", "undo"),
    c("edit.redo", "redo"),
    // The text itself takes Ctrl+X, C, V and A as every text field does;
    // the menu items hand the same to it, so they have no keys of their
    // own to clash with it.
    c("edit.cut", "cut"),
    c("edit.copy", "copy-2"),
    c("edit.paste", "paste"),
    c("edit.select_all", "select-all"),
    c("edit.find", "find-ellipsis").keys("Ctrl+F"),
    c("edit.replace", "replace-ellipsis").keys("Ctrl+H"),
    c("edit.find_next", "find-next").keys("F3"),
    c("edit.find_previous", "find-previous").keys("Shift+F3"),
];

/// Does what an Edit menu command does.
fn run(writer: &mut SueWriter, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let command = call.name;
    match command {
        "edit.cut" => writer.hand_to_text(egui::Event::Cut),
        "edit.copy" => writer.hand_to_text(egui::Event::Copy),
        "edit.paste" => {
            if let Some(text) = sue_platform::paste_text() {
                writer.hand_to_text(egui::Event::Paste(text));
            }
        }
        "edit.select_all" => writer.hand_to_text(egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }),
        "edit.find" => writer.open_find(false),
        "edit.replace" => writer.open_find(true),
        "edit.find_next" => writer.find_next(cx, false),
        "edit.find_previous" => writer.find_next(cx, true),
        "edit.undo" => {
            writer.editor.commit(cx);
            let _ = cx.undo(false);
        }
        "edit.redo" => {
            writer.editor.commit(cx);
            let _ = cx.undo(true);
        }
        _ => {}
    }
}
