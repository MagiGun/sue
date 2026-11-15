//! Keys for sub tools: every sub tool can be given a key of its own, as
//! in Clip Studio Paint, which picks it (and its tool) from anywhere.
//!
//! A sub tool is named for this by one family of the registry,
//! `subtool.<tool id>.<name>`: the tool's stable id
//! ([`Tool::id`]) and the sub tool's name, written so that it holds no
//! space and no `=` ([`command_name`]). Its keys live where every command
//! without a default chord keeps them, the studio's sparse keys setting
//! (`sue_wm::shortcuts::SparseKeys`), so Settings binds them like any
//! other command, a sub tool someone made by hand included, and a sub tool
//! renamed by Sue Paint is still found by its old name.
//!
//! Picking a sub tool changes no document, so the family has no undo
//! pair. Auto Actions may record it. The family itself is hidden from
//! the menus and the command palette ([`Flags::HIDDEN`]): Settings lists
//! every sub tool on its own row instead (`prefs::keymap`).

use sue_rice::RiceSchema;
use sue_wm::shortcuts::SparseKeys;
use sue_wm::{AppletContext, Call, Command, Flags};

use crate::studio::{KEYS_SECTION, SuePaint};
use crate::tools::{SubTool, Tool, Toolbox};

/// The start of every sub tool's command name.
pub(crate) const PREFIX: &str = "subtool.";

/// The command that picks sub tool `name` of `tool`:
/// `subtool.pen.G-pen`, `subtool.pencil.Mechanical_pencil`. A space is
/// written `_`, and `_`, `%`, `=` and any other blank as `%`, their
/// hexadecimal code and `;`, so every name reads back as it was.
#[must_use]
pub(crate) fn command_name(tool: Tool, name: &str) -> String {
    let mut out = format!("{PREFIX}{}.", tool.id());
    for ch in name.chars() {
        match ch {
            ' ' => out.push('_'),
            ch if matches!(ch, '_' | '%' | '=') || ch.is_whitespace() => {
                out.push_str(&format!("%{:X};", u32::from(ch)));
            }
            ch => out.push(ch),
        }
    }
    out
}

/// The tool and sub tool name a command of [`command_name`] stands for,
/// from what follows [`PREFIX`].
#[must_use]
pub(crate) fn parse(arg: &str) -> Option<(Tool, String)> {
    let (tool, written) = arg.split_once('.')?;
    let tool = Tool::from_id(tool)?;
    let mut name = String::new();
    let mut chars = written.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '_' => name.push(' '),
            '%' => {
                let code: String = chars.by_ref().take_while(|digit| *digit != ';').collect();
                name.push(char::from_u32(u32::from_str_radix(&code, 16).ok()?)?);
            }
            ch => name.push(ch),
        }
    }
    (!name.is_empty()).then_some((tool, name))
}

/// Where sub tool `name` of `tool` is in `toolbox`: its tool palette
/// entry and its place there. A name Sue Paint has since changed is found
/// by its old one too.
#[must_use]
pub(crate) fn find(toolbox: &Toolbox, tool: Tool, name: &str) -> Option<(usize, usize)> {
    let renamed = SubTool::renamed(name);
    toolbox
        .slots()
        .iter()
        .enumerate()
        .find_map(|(slot, entry)| {
            entry
                .subs()
                .iter()
                .position(|sub| sub.tool == tool && (sub.name == name || sub.name == renamed))
                .map(|index| (slot, index))
        })
}

/// Picks sub tool `index` of tool palette entry `slot`, and the entry.
pub(crate) fn pick(toolbox: &mut Toolbox, slot: usize, index: usize) {
    toolbox.pick_slot(slot);
    // The functions that take a tool act on the current entry when it is
    // that tool.
    let kind = toolbox.current_entry().kind;
    toolbox.pick_sub(kind, index);
}

/// The sub tool commands whose keys were pressed this frame, taken out of
/// `input` so nothing else sees them.
pub(crate) fn take(input: &mut egui::InputState, rice: &RiceSchema) -> Vec<String> {
    let pressed = input
        .events
        .iter()
        .any(|event| matches!(event, egui::Event::Key { pressed: true, .. }));
    if !pressed {
        return Vec::new();
    }
    SparseKeys::read(rice, KEYS_SECTION)
        .iter()
        .filter(|(name, chords)| name.starts_with(PREFIX) && sue_wm::keys::consume(input, chords))
        .map(|(name, _)| name.to_owned())
        .collect()
}

/// The keys people gave commands without a default key, sub tools'
/// among them, as Settings keeps them now.
#[must_use]
pub(crate) fn keys(rice: &RiceSchema) -> SparseKeys {
    SparseKeys::read(rice, KEYS_SECTION)
}

/// The keys of `command` in `keys`, as people read them (`Shift+F9`),
/// when it has any.
#[must_use]
pub(crate) fn key_text(keys: &SparseKeys, command: &str) -> Option<String> {
    let chords = keys.get(command)?;
    let text: Vec<String> = chords.chords().iter().map(ToString::to_string).collect();
    (!text.is_empty()).then(|| text.join(", "))
}

/// Picks the sub tool a command names, telling the person when it is
/// gone.
fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let found = parse(call.arg).and_then(|(tool, name)| find(&paint.toolbox, tool, &name));
    match found {
        Some((slot, index)) => pick(&mut paint.toolbox, slot, index),
        None => cx.notify(tr!("sub-tool-not-found")),
    }
}

pub(super) const COMMANDS: &[Command<SuePaint>] =
    &[Command::family(PREFIX, "pick-sub-tool", "subtool", run)
        .with(Flags::RECORDABLE.with(Flags::HIDDEN))];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_sub_tool_has_a_name_that_reads_back() {
        let toolbox = Toolbox::default();
        for entry in toolbox.slots() {
            for sub in entry.subs() {
                let name = command_name(sub.tool, &sub.name);
                assert!(SparseKeys::holds(&name), "{name}");
                let arg = name.strip_prefix(PREFIX).expect("the prefix");
                assert_eq!(parse(arg), Some((sub.tool, sub.name.clone())), "{name}");
                assert!(find(&toolbox, sub.tool, &sub.name).is_some());
            }
        }
    }

    #[test]
    fn odd_names_read_back_too() {
        for name in [
            "a_b",
            "50% grey",
            "x=y",
            "tab\there",
            "two  spaces",
            "%41",
            "Pinsel ä",
        ] {
            let command = command_name(Tool::Pen, name);
            assert!(SparseKeys::holds(&command), "{command}");
            let arg = command.strip_prefix(PREFIX).unwrap();
            assert_eq!(parse(arg), Some((Tool::Pen, name.to_owned())), "{command}");
        }
        assert_eq!(
            command_name(Tool::Pencil, "Mechanical pencil"),
            "subtool.pencil.Mechanical_pencil"
        );
        assert_eq!(parse("nothing.G-pen"), None);
        assert_eq!(parse("pen."), None);
    }

    #[test]
    fn a_sub_tool_of_another_tool_in_an_entry_is_picked_with_its_entry() {
        // Fill's entry holds the Figure tool's Lasso fill.
        let mut toolbox = Toolbox::default();
        let lasso = toolbox
            .slots()
            .iter()
            .flat_map(|slot| slot.subs())
            .find(|sub| sub.tool == Tool::Figure && sub.name == "Lasso fill")
            .cloned();
        let Some(lasso) = lasso else {
            return;
        };
        let (slot, index) = find(&toolbox, lasso.tool, &lasso.name).unwrap();
        toolbox.pick(Tool::Pen);
        pick(&mut toolbox, slot, index);
        assert_eq!(toolbox.current_slot(), slot);
        assert_eq!(toolbox.current().name, "Lasso fill");
        assert_eq!(toolbox.current_entry().kind, Tool::Fill);
    }

    #[test]
    fn a_key_given_in_the_sparse_setting_is_taken_once() {
        let mut rice = sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        sue_wm::register_rice(&mut rice).unwrap();
        let mut paint = SuePaint::default();
        let _ = rice.register_all(&sue_wm::SueApplet::rice_declarations(&paint));
        let name = command_name(Tool::Pencil, "Mechanical pencil");
        let mut sparse = SparseKeys::default();
        sparse.set(&name, "Shift+F9".parse().unwrap());
        let setting = sue_wm::shortcuts::sparse_setting(KEYS_SECTION);
        rice.set(&setting, sue_rice::RiceValue::Text(sparse.to_string()))
            .unwrap();
        let event = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        };
        let mut input = egui::InputState::default();
        input.events = vec![event(egui::Key::F9), event(egui::Key::F8)];
        let taken = take(&mut input, &rice);
        assert_eq!(input.events.len(), 1, "only the bound chord is taken");
        assert_eq!(taken, vec![name.clone()]);
        let (slot, index) = find(&paint.toolbox, Tool::Pencil, "Mechanical pencil").unwrap();
        pick(&mut paint.toolbox, slot, index);
        assert_eq!(paint.toolbox.current().name, "Mechanical pencil");
    }
}
