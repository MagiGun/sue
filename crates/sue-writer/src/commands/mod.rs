//! Sue Writer's commands: one registry that the menus, the shortcuts, the
//! command palette and the tests read.
//!
//! Each menu's commands are a `const` table in a file of their own,
//! beside what they do (`run`) and when they work (`state`). A command's
//! name is saved data (shortcut settings are keyed by it), so it is never
//! renamed without a migration.

use std::sync::OnceLock;

use sue_wm::CommandRegistry;

use crate::studio::{KEYS_SECTION, SueWriter};

mod edit;
mod file;
mod format;
mod help;
mod insert;
mod project;
mod review;
mod table;
mod view;

/// Commands the shell's own keys run (`keys.undo`, `keys.redo`,
/// `keys.open_file`): they get no row in the studio's shortcuts, so the
/// menus show the shell's keys beside them.
const SHELL_OWNED: [&str; 3] = ["edit.undo", "edit.redo", "file.open"];

/// Every command of Sue Writer.
pub(crate) fn registry() -> &'static CommandRegistry<SueWriter> {
    static REGISTRY: OnceLock<CommandRegistry<SueWriter>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CommandRegistry::new("writer", KEYS_SECTION);
        registry.add(file::COMMANDS);
        registry.add(edit::COMMANDS);
        registry.add(view::COMMANDS);
        registry.add(insert::COMMANDS);
        registry.add(format::COMMANDS);
        registry.add(table::COMMANDS);
        registry.add(review::COMMANDS);
        registry.add(project::COMMANDS);
        registry.add(help::COMMANDS);
        registry
    })
}

/// Settings that keep the English name 0.0.75 gave them, which differs
/// from the command's label (the shipped settings file and `rice.ftl`
/// name them so).
const SETTING_NAMES: [(&str, &str); 1] = [("compile.pdf", "PDF")];

/// Sue Writer's shortcuts: a row for every command that can be given a
/// key, with its default chords, but for the commands the shell's keys
/// run.
pub(crate) fn shortcut_table() -> &'static [sue_wm::shortcuts::Shortcut] {
    static TABLE: OnceLock<Vec<sue_wm::shortcuts::Shortcut>> = OnceLock::new();
    TABLE.get_or_init(|| {
        registry()
            .shortcuts()
            .into_iter()
            .filter(|shortcut| !SHELL_OWNED.contains(&shortcut.command))
            .map(|mut shortcut| {
                if let Some((_, name)) = SETTING_NAMES
                    .iter()
                    .find(|(command, _)| *command == shortcut.command)
                {
                    (*name).clone_into(&mut shortcut.label);
                }
                shortcut
            })
            .collect()
    })
}

/// The rows of the Keyboard Shortcuts list: each command as the menus
/// name it in the language in use, and its keys as set in `rice` now.
pub(crate) fn shortcut_rows(rice: &sue_rice::RiceSchema) -> Vec<(String, String)> {
    let table = shortcut_table();
    sue_wm::shortcuts::rows(rice, KEYS_SECTION, table)
        .into_iter()
        .zip(table)
        .map(|((label, keys), shortcut)| {
            let named = registry()
                .label(shortcut.command)
                .map_or(label, |name| name.trim_end_matches("...").to_owned());
            (named, keys)
        })
        .collect()
}

#[cfg(test)]
mod tests;
