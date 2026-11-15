//! The command bar's items and how they are kept: in `command_bar.txt`
//! in Sue Paint's data folder, and in every registered workspace.

use std::collections::HashSet;
use std::path::Path;

use crate::extras::{OLD_WORDS, read, write};
use crate::studio::SuePaint;

/// The file the command bar's own items are kept in.
pub(crate) const COMMAND_BAR_FILE: &str = "command_bar.txt";

/// The line that says the workspace holds the command bar's items, and
/// the word before each of them (`bar hide <command>`, `bar add
/// <command>`).
const BAR_WORD: &str = "bar";

/// The command bar's items as kept in `command_bar.txt`: one `hide
/// <command>` line per built-in button people hid (sorted), then one
/// `add <command>` line per command they added, in order.
#[must_use]
pub(crate) fn command_bar_text(hidden: &HashSet<String>, added: &[String]) -> String {
    let mut hidden: Vec<&String> = hidden.iter().collect();
    hidden.sort();
    let mut text = String::new();
    for command in hidden {
        text.push_str(&format!("hide {command}\n"));
    }
    for command in added {
        text.push_str(&format!("add {command}\n"));
    }
    text
}

/// The command bar's items from `command_bar.txt`, and whether the file
/// was in the old form (another language's words, or every line run
/// together, as 0.0.75 wrote it) and is to be written again.
#[must_use]
pub(crate) fn command_bar_from(text: &str) -> (HashSet<String>, Vec<String>, bool) {
    let registry = crate::commands::registry();
    let mut hidden = HashSet::new();
    let mut added = Vec::new();
    let mut tidy = true;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        if let Some(command) = line.strip_prefix("hide ")
            && registry.has(command)
        {
            hidden.insert(command.to_owned());
        } else if let Some(command) = line.strip_prefix("add ")
            && registry.has(command)
        {
            added.push(command.to_owned());
        } else {
            tidy = false;
        }
    }
    if tidy {
        return (hidden, added, false);
    }
    // The old form: look for every command in every language's words.
    hidden.clear();
    added.clear();
    let mut found: Vec<(usize, bool, String)> = Vec::new();
    let boundary_after = |rest: &str| {
        rest.is_empty()
            || rest.starts_with('\n')
            || OLD_WORDS.iter().any(|words| {
                [words.3, words.4]
                    .iter()
                    .any(|word| !word.starts_with(' ') && rest.starts_with(word))
            })
    };
    let boundary_before = |head: &str| {
        head.is_empty()
            || head.ends_with('\n')
            || OLD_WORDS.iter().any(|words| {
                [words.3, words.4]
                    .iter()
                    .any(|word| word.starts_with(' ') && head.ends_with(word))
            })
    };
    for command in registry
        .iter()
        .filter(|command| !command.is_family())
        .map(|command| command.name)
    {
        for words in &OLD_WORDS {
            for (word, hide) in [(words.3, true), (words.4, false)] {
                let needle = if word.starts_with(' ') {
                    format!("{command}{word}")
                } else {
                    format!("{word}{command}")
                };
                for (at, _) in text.match_indices(&needle) {
                    let fits = if word.starts_with(' ') {
                        boundary_before(&text[..at])
                    } else {
                        boundary_after(&text[at + needle.len()..])
                    };
                    if fits && !found.iter().any(|(_, _, known)| known == command) {
                        found.push((at, hide, command.to_owned()));
                    }
                }
            }
        }
    }
    found.sort_by_key(|(at, _, _)| *at);
    for (_, hide, command) in found {
        if hide {
            hidden.insert(command);
        } else {
            added.push(command);
        }
    }
    (hidden, added, true)
}

/// The command bar's lines of a workspace, gathered while its lines are
/// read.
#[derive(Default)]
pub(crate) struct WorkspaceBar {
    /// `None` until the line that says the workspace holds the bar.
    bar: Option<(HashSet<String>, Vec<String>)>,
}

impl WorkspaceBar {
    /// Takes `line` if it is one of the command bar's.
    pub(crate) fn read(&mut self, line: &str) {
        if line == BAR_WORD {
            self.bar = Some((HashSet::new(), Vec::new()));
        } else if let Some(item) = line.strip_prefix(&format!("{BAR_WORD} "))
            && let Some((hidden, added)) = self.bar.as_mut()
        {
            let registry = crate::commands::registry();
            if let Some(command) = item.strip_prefix("hide ")
                && registry.has(command)
            {
                hidden.insert(command.to_owned());
            } else if let Some(command) = item.strip_prefix("add ")
                && registry.has(command)
            {
                added.push(command.to_owned());
            }
        }
    }

    /// Gives `paint` the bar the workspace held.
    pub(crate) fn apply(self, paint: &mut SuePaint) {
        // A workspace kept before it held the command bar leaves it as is.
        if let Some((hidden, added)) = self.bar {
            paint.bar_hidden = hidden;
            paint.bar_extra = added;
        }
    }
}

impl SuePaint {
    /// Reads the command bar's items from `command_bar.txt` in `folder`.
    pub(crate) fn load_command_bar(&mut self, folder: Option<&Path>) {
        if let Some(text) = read(folder, COMMAND_BAR_FILE) {
            let (hidden, added, old) = command_bar_from(&text);
            self.bar_hidden = hidden;
            self.bar_extra = added;
            if old {
                // Read once in the old form, kept in the new one.
                let _ = write(
                    folder,
                    COMMAND_BAR_FILE,
                    &command_bar_text(&self.bar_hidden, &self.bar_extra),
                );
            }
        }
    }

    /// Keeps the command bar's items in `command_bar.txt` in `folder`.
    pub(crate) fn save_command_bar(&self, folder: Option<&Path>) -> Result<(), String> {
        let bar = command_bar_text(&self.bar_hidden, &self.bar_extra);
        write(folder, COMMAND_BAR_FILE, &bar)
    }

    /// The command bar's lines of a workspace.
    pub(crate) fn bar_workspace_lines(&self) -> Vec<String> {
        let mut lines = vec![BAR_WORD.to_owned()];
        let bar = command_bar_text(&self.bar_hidden, &self.bar_extra);
        lines.extend(bar.lines().map(|line| format!("{BAR_WORD} {line}")));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_bar_file_keeps_one_line_per_item() {
        let hidden: HashSet<String> = ["view.fit", "edit.undo"].map(String::from).into();
        let added = vec!["layer.new".to_owned(), "select.all".to_owned()];
        let text = command_bar_text(&hidden, &added);
        assert_eq!(
            text,
            "hide edit.undo\nhide view.fit\nadd layer.new\nadd select.all\n"
        );
        assert_eq!(
            command_bar_from(&text),
            (hidden.clone(), added.clone(), false)
        );
        // As 0.0.75 wrote it: every line run together, in English or in
        // German (whose words come after the command).
        for old in [
            "hide edit.undohide view.fitadd layer.newadd select.all",
            "edit.undo ausblendenview.fit ausblendenlayer.new hinzufügenselect.all hinzufügen",
            "ocultar edit.undoocultar view.fitañadir layer.newañadir select.all",
        ] {
            assert_eq!(
                command_bar_from(old),
                (hidden.clone(), added.clone(), true),
                "{old}"
            );
        }
        assert_eq!(command_bar_from(""), (HashSet::new(), Vec::new(), false));
    }

    #[test]
    fn a_workspace_kept_before_the_command_bar_was_in_it_leaves_the_bar() {
        let mut paint = SuePaint {
            bar_extra: vec!["view.fit".to_owned()],
            ..SuePaint::default()
        };
        let old: Vec<String> = ["panels 1111111111", "menus 1"].map(String::from).to_vec();
        paint.apply_layout(&old);
        assert_eq!(paint.bar_extra, ["view.fit"]);
        // One kept now holds the bar, an empty one too.
        paint.bar_extra.clear();
        let now = paint.workspace_now("").lines;
        paint.bar_extra = vec!["view.fit".to_owned()];
        paint.apply_layout(&now);
        assert!(paint.bar_extra.is_empty());
    }
}
