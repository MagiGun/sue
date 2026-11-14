//! Sue Paint's commands: one registry that the menus, the shortcuts, the
//! command palette and the tests read.
//!
//! Each group's commands are a `const` table in a file of their own. A
//! command's name is saved data (shortcut settings, Auto Actions and the
//! custom touch keys hold it), so it is never renamed without a
//! migration.

use std::sync::OnceLock;

use sue_wm::{AppletContext, Call, CommandRegistry, Facts, ItemState};

use crate::studio::{KEYS_SECTION, SuePaint};

mod anim;
mod canvas;
mod edit;
mod figure;
mod file;
mod filter;
mod help;
mod layer;
mod select;
mod story;
mod tool;
mod view;
mod window;

/// Commands the shell's own keys run (`keys.undo`, `keys.redo`,
/// `keys.open_file`, `keys.save`, `keys.quit`): they get no row in the
/// studio's shortcuts, so the menus show the shell's keys beside them.
pub(crate) const SHELL_OWNED: [&str; 5] = [
    "edit.undo",
    "edit.redo",
    "file.open",
    "file.save",
    "app.quit",
];

/// Every command of Sue Paint.
pub fn registry() -> &'static CommandRegistry<SuePaint> {
    static REGISTRY: OnceLock<CommandRegistry<SuePaint>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CommandRegistry::new("paint", KEYS_SECTION);
        registry.add(file::COMMANDS);
        registry.add(edit::COMMANDS);
        registry.add(story::COMMANDS);
        registry.add(anim::COMMANDS);
        registry.add(layer::COMMANDS);
        registry.add(select::COMMANDS);
        registry.add(view::COMMANDS);
        registry.add(filter::COMMANDS);
        registry.add(window::COMMANDS);
        registry.add(help::COMMANDS);
        registry.add(canvas::COMMANDS);
        registry.add(figure::COMMANDS);
        registry.add(tool::COMMANDS);
        registry
    })
}

/// Sue Paint's shortcuts: a row for every command that can be given a
/// key, with its default chords (Clip Studio Paint's), but for the
/// commands the shell's keys run. The labels are the settings' English
/// descriptions; Settings and the Keyboard Shortcuts list read them in
/// other languages from the language database.
pub(crate) fn shortcut_table() -> &'static [sue_wm::shortcuts::Shortcut] {
    static TABLE: OnceLock<Vec<sue_wm::shortcuts::Shortcut>> = OnceLock::new();
    TABLE.get_or_init(|| {
        registry()
            .shortcuts()
            .into_iter()
            .filter(|shortcut| !SHELL_OWNED.contains(&shortcut.command))
            .collect()
    })
}

/// Does what a command does, as the studio's one match always did.
fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_arms(call.name, cx);
}

/// How an item for a command looks, as the studio's one match always
/// said.
fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_arms(call.name, facts.arena, facts.history)
}

#[cfg(test)]
mod tests;
