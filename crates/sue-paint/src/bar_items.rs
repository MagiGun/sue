//! The items of the command bar and of Quick Access's sets, and how they
//! are kept: `command_bar.txt` and `quick_access.txt` in Sue Paint's data
//! folder, and the bar lines of every registered workspace.
//!
//! An item is a command of the registry (a menu command, an option), a
//! tool, a sub tool, a drawing color, a palette that pops up, an Auto
//! Action, a line between groups, or a group of items under one button.
//! Each can show an icon of its own, picked by its stable name in Sue's
//! icon set (so an icon pack replaces it too).
//!
//! The files are plain text in English, whatever language Sue speaks:
//! people copy, share and edit them (docs/paint-command-bar-files.md).

use std::collections::HashSet;
use std::path::Path;

use sue_wm::AppletContext;
use sue_wm::dock::DockPanel;
use sue_wm::icons::Icon;

use crate::dock::Palette;
use crate::extras::{OLD_WORDS, read, write};
use crate::studio::SuePaint;
use crate::tools::Tool;

/// The file the command bar's items are kept in.
pub(crate) const COMMAND_BAR_FILE: &str = "command_bar.txt";

/// The file Quick Access's sets are kept in.
pub(crate) const QUICK_ACCESS_FILE: &str = "quick_access.txt";

/// The first line of a command bar file.
const BAR_HEADER: &str = "sue-command-bar 1";

/// The first line of a Quick Access file.
const QUICK_HEADER: &str = "sue-quick-access 1";

/// The line that says the workspace holds the command bar's items, and
/// the word before each of their lines (`bar command file.new`).
const BAR_WORD: &str = "bar";

/// The line that says the workspace holds Quick Access's sets, and the
/// word before each of their lines.
const QUICK_WORD: &str = "quick";

/// The extension of a command bar or a Quick Access file people share.
pub(crate) const SHARE_EXTENSION: &str = "txt";

/// The command that opens the command palette: the shell's, not one of
/// Sue Paint's registry, and a button of the command bar since 0.0.75.
pub(crate) const PALETTE_COMMAND: &str = "app.palette";

/// The command bar a new install starts with, in groups with a line
/// between each two: Clip Studio Paint's file, undo, edit and view
/// buttons, then Sue's own, then the timelapse recording button at the
/// end as Clip Studio Paint has it.
pub(crate) const DEFAULT_BAR: [&[&str]; 6] = [
    &["file.new", "file.open", "file.save"],
    &["edit.undo", "edit.redo"],
    &[
        "edit.clear",
        "edit.clear_outside",
        "edit.fill",
        "select.none",
        "edit.transform.scale",
    ],
    &[
        "view.flip_h",
        "view.rotate_left",
        "view.rotate_right",
        "view.fit",
    ],
    &[
        "view.snap_ruler",
        "view.snap_grid",
        "help.shortcuts",
        "help.about",
        PALETTE_COMMAND,
    ],
    &["file.timelapse.record"],
];

/// Quick Access's first set as Clip Studio Paint ships it, in groups:
/// undo; delete and the clipboard; the transforms; flips and the view;
/// quick mask; the brush size; then the button to its settings.
const DEFAULT_SET_1: [&[&str]; 7] = [
    &["edit.undo", "edit.redo"],
    &["edit.clear", "edit.cut", "edit.copy", "edit.paste"],
    &[
        "edit.transform.scale",
        "edit.transform.free",
        "edit.transform.mesh",
    ],
    &["view.flip_h", "view.flip_v", "view.reset_display"],
    &["select.quick_mask"],
    &["brush.bigger", "brush.smaller"],
    &["window.quick_access.settings"],
];

/// Quick Access's second set: the layer commands people reach for most.
const DEFAULT_SET_2: [&[&str]; 2] = [
    &[
        "layer.new",
        "layer.new.vector",
        "layer.new.folder",
        "layer.duplicate",
    ],
    &["layer.merge_down", "layer.delete"],
];

/// The palettes that can pop up from a button, as Clip Studio Paint's
/// "Pop-up palettes".
pub(crate) const POPUP_PALETTES: [Palette; 13] = [
    Palette::Color,
    Palette::ColorSlider,
    Palette::ColorSet,
    Palette::Intermediate,
    Palette::Approximate,
    Palette::ColorHistory,
    Palette::SubTool,
    Palette::ToolProperty,
    Palette::BrushGrid,
    Palette::Layer,
    Palette::LayerProperty,
    Palette::Navigator,
    Palette::History,
];

/// What a button does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Item {
    /// Runs a command of the registry (or opens the command palette).
    Command(String),
    /// Picks a tool.
    Tool(Tool),
    /// Picks a sub tool, by its tool and its name.
    SubTool(Tool, String),
    /// Makes this the drawing color.
    Color([u8; 3]),
    /// Pops a palette up under the button.
    Popup(Palette),
    /// Plays an Auto Action, by its name.
    AutoAction(String),
    /// A line between groups of buttons.
    Separator,
    /// Several items under one button: the one shown runs, the others are
    /// in its drop-down.
    Group(Vec<Slot>),
}

/// A button: what it does, and the icon people gave it (by its name in
/// Sue's icon set), if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pub item: Item,
    pub icon: Option<String>,
}

impl Slot {
    /// A button with its own icon.
    #[must_use]
    pub(crate) const fn of(item: Item) -> Self {
        Self { item, icon: None }
    }

    /// A button that runs `command`.
    #[must_use]
    pub(crate) fn command(command: &str) -> Self {
        Self::of(Item::Command(command.to_owned()))
    }

    /// The icon it shows: its own, else its item's.
    #[must_use]
    pub(crate) fn shown_icon(&self) -> Option<Icon> {
        self.icon
            .as_deref()
            .and_then(Icon::from_name)
            .or_else(|| self.item.icon())
    }
}

/// How a button is moved along its bar: by dragging it, or only while
/// Ctrl is held (Clip Studio Paint's "Change Order").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Reorder {
    /// A drag moves it.
    Drag,
    /// A drag with Ctrl held moves it; a plain drag does nothing.
    #[default]
    CtrlDrag,
}

impl Reorder {
    const fn word(self) -> &'static str {
        match self {
            Self::Drag => "drag",
            Self::CtrlDrag => "ctrl-drag",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        [Self::Drag, Self::CtrlDrag]
            .into_iter()
            .find(|reorder| reorder.word() == word)
    }
}

/// The command bar's buttons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bar {
    pub items: Vec<Slot>,
    pub reorder: Reorder,
}

impl Default for Bar {
    fn default() -> Self {
        // Save keeps the ways to save and export in its list, as Clip
        // Studio Paint's Save button does.
        let items = grouped(&DEFAULT_BAR)
            .into_iter()
            .map(|slot| {
                if slot == Slot::command("file.save") {
                    Slot::of(Item::Group(
                        SAVE_GROUP
                            .iter()
                            .map(|command| Slot::command(command))
                            .collect(),
                    ))
                } else {
                    slot
                }
            })
            .collect();
        Self {
            items,
            reorder: Reorder::CtrlDrag,
        }
    }
}

/// The default bar's Save button and what its list holds.
const SAVE_GROUP: [&str; 5] = [
    "file.save",
    "file.save_as",
    "file.export.png",
    "file.export.jpg",
    "file.export.jxl",
];

/// A set of Quick Access: its name and buttons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QuickSet {
    pub name: String,
    pub items: Vec<Slot>,
}

/// Quick Access: its sets, the one showing and how buttons move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QuickAccess {
    pub sets: Vec<QuickSet>,
    pub shown: usize,
    pub reorder: Reorder,
}

impl Default for QuickAccess {
    fn default() -> Self {
        Self {
            sets: vec![
                QuickSet {
                    name: set_name(1),
                    items: grouped(&DEFAULT_SET_1),
                },
                QuickSet {
                    name: set_name(2),
                    items: grouped(&DEFAULT_SET_2),
                },
            ],
            shown: 0,
            reorder: Reorder::CtrlDrag,
        }
    }
}

impl QuickAccess {
    /// The set showing, to change.
    pub(crate) fn set_mut(&mut self) -> Option<&mut QuickSet> {
        self.sets.get_mut(self.shown)
    }

    /// A new empty set after the others, named after its number, shown.
    pub(crate) fn add_set(&mut self) {
        let mut number = self.sets.len() + 1;
        while self.sets.iter().any(|set| set.name == set_name(number)) {
            number += 1;
        }
        self.sets.push(QuickSet {
            name: set_name(number),
            items: Vec::new(),
        });
        self.shown = self.sets.len() - 1;
    }

    /// Takes the set showing away; the last set stays.
    pub(crate) fn delete_set(&mut self) -> bool {
        if self.sets.len() <= 1 || self.shown >= self.sets.len() {
            return false;
        }
        self.sets.remove(self.shown);
        self.shown = self.shown.min(self.sets.len() - 1);
        true
    }
}

/// "Set 1", "Set 2": a new set's name.
fn set_name(number: usize) -> String {
    tr!("quick-access-set-n", number = number)
}

/// Buttons from groups of command names, a line between each two groups.
fn grouped(groups: &[&[&str]]) -> Vec<Slot> {
    let mut items = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            items.push(Slot::of(Item::Separator));
        }
        items.extend(group.iter().map(|command| Slot::command(command)));
    }
    items
}

/// `true` if `command` can be a button: a command of the registry that is
/// not a family, or the command palette.
#[must_use]
pub(crate) fn known_command(command: &str) -> bool {
    command == PALETTE_COMMAND || crate::commands::registry().has(command)
}

/// The name of the shared icon for `tool`, as its palette shows it.
#[must_use]
pub(crate) const fn tool_icon_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Zoom => "tool-zoom",
        Tool::Hand => "tool-hand",
        Tool::Operation => "tool-operation",
        Tool::MoveLayer => "tool-move-layer",
        Tool::Select => "tool-select",
        Tool::AutoSelect => "tool-auto-select",
        Tool::Eyedropper => "tool-eyedropper",
        Tool::Pen => "tool-pen",
        Tool::Pencil => "tool-pencil",
        Tool::Brush => "tool-brush",
        Tool::Airbrush => "tool-airbrush",
        Tool::Decoration => "tool-decoration",
        Tool::Eraser => "tool-eraser",
        Tool::Blend => "tool-blend",
        Tool::Fill => "tool-fill",
        Tool::Gradient => "tool-gradient",
        Tool::Figure => "tool-figure",
        Tool::Frame => "tool-frame",
        Tool::Ruler => "tool-ruler",
        Tool::Text => "tool-text",
        Tool::Balloon => "tool-balloon",
        Tool::CorrectLine => "tool-correct-line",
    }
}

/// The name of the shared icon for `palette`, as its tab shows it.
#[must_use]
pub(crate) const fn palette_icon_name(palette: Palette) -> &'static str {
    match palette {
        Palette::CommandBar => "palette-command-bar",
        Palette::QuickAccess => "star",
        Palette::Tools => "palette-tools",
        Palette::SubTool => "palette-sub-tools",
        Palette::ToolProperty => "palette-tool-property",
        Palette::BrushSize => "palette-brush-size",
        Palette::BrushGrid => "palette-brush-grid",
        Palette::Color => "palette-color-wheel",
        Palette::ColorSlider => "palette-color-sliders",
        Palette::ColorSet => "palette-color-set",
        Palette::Intermediate => "palette-intermediate",
        Palette::Approximate => "palette-approximate",
        Palette::ColorHistory => "palette-color-history",
        Palette::Navigator => "palette-navigator",
        Palette::SubView => "palette-sub-view",
        Palette::Material => "palette-materials",
        Palette::History => "palette-history",
        Palette::AutoAction => "palette-auto-action",
        Palette::ItemBank => "palette-item-bank",
        Palette::Info => "palette-info",
        Palette::Timeline => "palette-timeline",
        Palette::Cels => "palette-cels",
        Palette::Layer => "layers",
        Palette::LayerProperty => "layer-property",
    }
}

impl Item {
    /// The icon the item shows of itself, if it has one (a color shows a
    /// swatch, a line is a line).
    #[must_use]
    pub(crate) fn icon(&self) -> Option<Icon> {
        match self {
            Self::Command(command) if command == PALETTE_COMMAND => Some(Icon::Search),
            Self::Command(command) => crate::commands::registry()
                .find(command)
                .and_then(|(found, _)| Icon::from_name(found.icon)),
            Self::Tool(tool) | Self::SubTool(tool, _) => Icon::from_name(tool_icon_name(*tool)),
            Self::Popup(palette) => Icon::from_name(palette_icon_name(*palette)),
            Self::AutoAction(_) => Some(Icon::PaletteAutoAction),
            Self::Group(items) => items.first().and_then(Slot::shown_icon),
            Self::Color(_) | Self::Separator => None,
        }
    }

    /// What it is called, in the language in use.
    #[must_use]
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Command(command) if command == PALETTE_COMMAND => {
                tr!("search-every-command")
            }
            Self::Command(command) => crate::commands::registry()
                .label(command)
                .unwrap_or_else(|| command.clone())
                .trim_end_matches("...")
                .to_owned(),
            Self::Tool(tool) => tool.label(),
            Self::SubTool(_, name) => crate::localize(name),
            Self::Color(rgb) => tr!("drawing-color-hex", hex = crate::color::hex(*rgb)),
            Self::Popup(palette) => palette.title(),
            Self::AutoAction(name) => name.clone(),
            Self::Separator => tr!("separator"),
            Self::Group(items) => items.first().map_or_else(
                || tr!("group"),
                |first| tr!("group-of", first = first.item.label()),
            ),
        }
    }

    /// Its line in a file: `command file.new`, `color 1e90ff`; a group
    /// writes its own lines.
    fn line(&self) -> Option<String> {
        Some(match self {
            Self::Command(command) => format!("command {command}"),
            Self::Tool(tool) => format!("tool {}", tool.id()),
            Self::SubTool(tool, name) => format!("sub-tool {} {name}", tool.id()),
            Self::Color(rgb) => format!("color {}", crate::color::hex(*rgb)),
            Self::Popup(palette) => format!("popup {}", palette.key()),
            Self::AutoAction(name) => format!("auto-action {name}"),
            Self::Separator => "separator".to_owned(),
            Self::Group(_) => return None,
        })
    }

    /// The item of a line written by [`Self::line`], if Sue Paint knows
    /// it now.
    fn from_line(line: &str) -> Option<Self> {
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        Some(match word {
            "command" if known_command(rest) => Self::Command(rest.to_owned()),
            "tool" => Self::Tool(Tool::from_id(rest)?),
            "sub-tool" => {
                let (tool, name) = rest.split_once(' ')?;
                Self::SubTool(Tool::from_id(tool)?, name.to_owned())
            }
            "color" => Self::Color(crate::color::parse_hex(rest)?),
            "popup" => Self::Popup(
                Palette::named(rest).filter(|palette| POPUP_PALETTES.contains(palette))?,
            ),
            "auto-action" if !rest.is_empty() => Self::AutoAction(rest.to_owned()),
            "separator" => Self::Separator,
            _ => return None,
        })
    }
}

/// The lines of `items`, as a file holds them.
#[must_use]
pub(crate) fn items_text(items: &[Slot]) -> Vec<String> {
    let mut lines = Vec::new();
    for slot in items {
        match &slot.item {
            Item::Group(members) => {
                lines.push("group".to_owned());
                lines.extend(items_text(members));
                lines.push("end".to_owned());
            }
            item => lines.extend(item.line()),
        }
        if let Some(icon) = &slot.icon {
            lines.push(format!("icon {icon}"));
        }
    }
    lines
}

/// The items of `lines` written by [`items_text`]. A line Sue Paint does
/// not know (a command it no longer has, a word of a newer Sue) is
/// skipped; so is a group left with fewer than two items, whose one item
/// takes its place.
#[must_use]
pub(crate) fn items_from<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<Slot> {
    // A stack of open groups; the bar itself at the bottom.
    let mut stack: Vec<Vec<Slot>> = vec![Vec::new()];
    // The last line was an item Sue Paint knows, so an icon line is its.
    let mut last_known = false;
    for line in lines
        .into_iter()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let top = stack.len() - 1;
        if line == "group" {
            stack.push(Vec::new());
            last_known = false;
        } else if line == "end" {
            if top == 0 {
                last_known = false;
                continue;
            }
            let members = stack.pop().unwrap_or_default();
            let parent = &mut stack[top - 1];
            last_known = close_group(parent, members);
        } else if let Some(icon) = line.strip_prefix("icon ") {
            if last_known && let Some(slot) = stack[top].last_mut() {
                slot.icon = Some(icon.to_owned());
            }
        } else if let Some(item) = Item::from_line(line) {
            // A group holds buttons, never lines between them.
            let inside = top > 0;
            if inside && item == Item::Separator {
                last_known = false;
                continue;
            }
            stack[top].push(Slot::of(item));
            last_known = true;
        } else {
            last_known = false;
        }
    }
    // Groups the file never closed close at its end.
    while stack.len() > 1 {
        let members = stack.pop().unwrap_or_default();
        let top = stack.len() - 1;
        close_group(&mut stack[top], members);
    }
    stack.pop().unwrap_or_default()
}

/// Puts a group of `members` at the end of `parent`: a group of two or
/// more, the one item alone, or nothing. `true` when something was put.
fn close_group(parent: &mut Vec<Slot>, mut members: Vec<Slot>) -> bool {
    match members.len() {
        0 => false,
        1 => {
            parent.extend(members.pop());
            true
        }
        _ => {
            parent.push(Slot::of(Item::Group(members)));
            true
        }
    }
}

/// The command bar as `command_bar.txt` holds it.
#[must_use]
pub(crate) fn bar_text(bar: &Bar) -> String {
    let mut lines = vec![
        BAR_HEADER.to_owned(),
        format!("reorder {}", bar.reorder.word()),
    ];
    lines.extend(items_text(&bar.items));
    lines.join("\n") + "\n"
}

/// The command bar from the text of `command_bar.txt`, and whether it was
/// in an old form (0.0.75's `hide` and `add` lines) and is to be written
/// again.
#[must_use]
pub(crate) fn bar_from(text: &str) -> (Bar, bool) {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    if lines.next() != Some(BAR_HEADER) {
        let (hidden, added, _) = command_bar_from(text);
        return (bar_of_old(&hidden, &added), true);
    }
    let mut bar = Bar {
        items: Vec::new(),
        reorder: Reorder::CtrlDrag,
    };
    let rest: Vec<&str> = lines
        .filter(
            |line| match line.strip_prefix("reorder ").and_then(Reorder::from_word) {
                Some(reorder) => {
                    bar.reorder = reorder;
                    false
                }
                None => true,
            },
        )
        .collect();
    bar.items = items_from(rest);
    (bar, false)
}

/// The command bar a 0.0.75 file described: Sue Paint's buttons but the
/// `hidden` ones, then the `added` commands.
#[must_use]
pub(crate) fn bar_of_old(hidden: &HashSet<String>, added: &[String]) -> Bar {
    let mut items: Vec<Slot> = Bar::default()
        .items
        .into_iter()
        .filter(|slot| {
            let command = match &slot.item {
                Item::Command(command) => Some(command),
                Item::Group(members) => members.first().and_then(|first| match &first.item {
                    Item::Command(command) => Some(command),
                    _ => None,
                }),
                _ => None,
            };
            command.is_none_or(|command| !hidden.contains(command))
        })
        .collect();
    tidy_lines(&mut items);
    if !added.is_empty() {
        items.push(Slot::of(Item::Separator));
        items.extend(added.iter().map(|command| Slot::command(command)));
    }
    Bar {
        items,
        reorder: Reorder::CtrlDrag,
    }
}

/// Takes away lines at either end and lines next to each other.
pub(crate) fn tidy_lines(items: &mut Vec<Slot>) {
    let mut kept: Vec<Slot> = Vec::with_capacity(items.len());
    for slot in items.drain(..) {
        let line = slot.item == Item::Separator;
        if line && kept.last().is_none_or(|last| last.item == Item::Separator) {
            continue;
        }
        kept.push(slot);
    }
    if kept.last().is_some_and(|last| last.item == Item::Separator) {
        kept.pop();
    }
    *items = kept;
}

/// Quick Access as `quick_access.txt` holds it.
#[must_use]
pub(crate) fn quick_text(quick: &QuickAccess) -> String {
    let mut lines = vec![
        QUICK_HEADER.to_owned(),
        format!("reorder {}", quick.reorder.word()),
        format!("shown {}", quick.shown),
    ];
    for set in &quick.sets {
        lines.push(format!("[{}]", set.name));
        lines.extend(items_text(&set.items));
    }
    lines.join("\n") + "\n"
}

/// Quick Access from the text of `quick_access.txt`; `None` when it is
/// not such a file.
#[must_use]
pub(crate) fn quick_from(text: &str) -> Option<QuickAccess> {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    if lines.next() != Some(QUICK_HEADER) {
        return None;
    }
    let mut quick = QuickAccess {
        sets: Vec::new(),
        shown: 0,
        reorder: Reorder::CtrlDrag,
    };
    let mut current: Option<(String, Vec<&str>)> = None;
    let finish = |set: Option<(String, Vec<&str>)>, sets: &mut Vec<QuickSet>| {
        if let Some((name, lines)) = set {
            sets.push(QuickSet {
                name,
                items: items_from(lines),
            });
        }
    };
    for line in lines {
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            finish(current.take(), &mut quick.sets);
            current = Some((name.to_owned(), Vec::new()));
        } else if let Some((_, lines)) = current.as_mut() {
            lines.push(line);
        } else if let Some(reorder) = line.strip_prefix("reorder ").and_then(Reorder::from_word) {
            quick.reorder = reorder;
        } else if let Some(shown) = line
            .strip_prefix("shown ")
            .and_then(|shown| shown.parse().ok())
        {
            quick.shown = shown;
        }
    }
    finish(current.take(), &mut quick.sets);
    if quick.sets.is_empty() {
        quick.sets.push(QuickSet {
            name: set_name(1),
            items: Vec::new(),
        });
    }
    quick.shown = quick.shown.min(quick.sets.len() - 1);
    Some(quick)
}

/// The command bar's items as kept in `command_bar.txt` by 0.0.75 to
/// 0.0.8's first builds: one `hide <command>` line per built-in button
/// people hid (sorted), then one `add <command>` line per command they
/// added, in order. Written now only by the tests that make old files.
#[cfg(test)]
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

/// The command bar's items from an old `command_bar.txt`, and whether the
/// file was in the oldest form (another language's words, or every line
/// run together, as 0.0.75 wrote it).
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

/// The key the menus show beside `command`, when a menu has it and shows
/// one.
fn shortcut_in<'m>(menus: &'m [sue_wm::Menu], command: &str) -> Option<&'m str> {
    fn find<'m>(items: &'m [sue_wm::MenuItem], command: &str) -> Option<&'m str> {
        items.iter().find_map(|item| match item {
            sue_wm::MenuItem::Action {
                command: found,
                shortcut,
                ..
            } if found == command && !shortcut.is_empty() => Some(shortcut.as_str()),
            sue_wm::MenuItem::Sub { items, .. } => find(items, command),
            _ => None,
        })
    }
    menus.iter().find_map(|menu| find(&menu.items, command))
}

/// The command bar's and Quick Access's lines of a workspace, gathered
/// while its lines are read.
#[derive(Default)]
pub(crate) struct WorkspaceBar {
    /// The bar's lines; `None` until the line that says the workspace
    /// holds the bar.
    bar: Option<Vec<String>>,
    /// Quick Access's lines, the same way.
    quick: Option<Vec<String>>,
}

impl WorkspaceBar {
    /// Takes `line` if it is one of the command bar's or Quick Access's.
    pub(crate) fn read(&mut self, line: &str) {
        if line == BAR_WORD {
            self.bar = Some(Vec::new());
        } else if line == QUICK_WORD {
            self.quick = Some(Vec::new());
        } else if let Some(item) = line.strip_prefix(&format!("{BAR_WORD} "))
            && let Some(lines) = self.bar.as_mut()
        {
            lines.push(item.to_owned());
        } else if let Some(item) = line.strip_prefix(&format!("{QUICK_WORD} "))
            && let Some(lines) = self.quick.as_mut()
        {
            lines.push(item.to_owned());
        }
    }

    /// Gives `paint` the bar and Quick Access the workspace held. A
    /// workspace kept before it held them leaves them as they are.
    pub(crate) fn apply(self, paint: &mut SuePaint) {
        if let Some(lines) = self.bar {
            let old = lines
                .iter()
                .any(|line| line.starts_with("hide ") || line.starts_with("add "));
            paint.bar = if old {
                let (hidden, added, _) = command_bar_from(&(lines.join("\n") + "\n"));
                bar_of_old(&hidden, &added)
            } else {
                let mut text = vec![BAR_HEADER.to_owned()];
                text.extend(lines);
                bar_from(&text.join("\n")).0
            };
        }
        if let Some(lines) = self.quick {
            let mut text = vec![QUICK_HEADER.to_owned()];
            text.extend(lines);
            if let Some(quick) = quick_from(&text.join("\n")) {
                paint.quick_access = quick;
            }
        }
    }
}

impl SuePaint {
    /// Reads the command bar from `command_bar.txt` and Quick Access from
    /// `quick_access.txt` in `folder`. An old command bar file is written
    /// again in the form of now.
    pub(crate) fn load_command_bar(&mut self, folder: Option<&Path>) {
        if let Some(text) = read(folder, COMMAND_BAR_FILE) {
            let (bar, old) = bar_from(&text);
            self.bar = bar;
            if old {
                // Read once in the old form, kept in the new one.
                let _ = write(folder, COMMAND_BAR_FILE, &bar_text(&self.bar));
            }
        }
        if let Some(quick) = read(folder, QUICK_ACCESS_FILE).and_then(|text| quick_from(&text)) {
            self.quick_access = quick;
        }
    }

    /// Keeps the command bar and Quick Access in their files in `folder`.
    pub(crate) fn save_command_bar(&self, folder: Option<&Path>) -> Result<(), String> {
        write(folder, COMMAND_BAR_FILE, &bar_text(&self.bar))
            .and_then(|()| write(folder, QUICK_ACCESS_FILE, &quick_text(&self.quick_access)))
    }

    /// Keeps the command bar and Quick Access now, after a change made on
    /// them.
    pub(crate) fn save_bars(&self, cx: &mut AppletContext<'_>) {
        let folder = crate::colorsets::folder(cx);
        if let Err(error) = self.save_command_bar(folder.as_deref()) {
            cx.fail(tr!("cannot-keep-the-palettes-settings", error = error));
        }
    }

    /// The command bar's and Quick Access's lines of a workspace.
    pub(crate) fn bar_workspace_lines(&self) -> Vec<String> {
        let mut lines = vec![BAR_WORD.to_owned()];
        let bar = bar_text(&self.bar);
        lines.extend(bar.lines().skip(1).map(|line| format!("{BAR_WORD} {line}")));
        lines.push(QUICK_WORD.to_owned());
        let quick = quick_text(&self.quick_access);
        lines.extend(
            quick
                .lines()
                .skip(1)
                .map(|line| format!("{QUICK_WORD} {line}")),
        );
        lines
    }

    /// Does what a button does.
    pub(crate) fn run_item(&mut self, item: &Item, cx: &mut AppletContext<'_>) {
        match item {
            Item::Command(command) if command == PALETTE_COMMAND => {
                cx.request(sue_wm::ShellRequest::Palette);
            }
            Item::Command(command) => self.run(command, cx),
            Item::Tool(tool) => self.toolbox.pick(*tool),
            Item::SubTool(tool, name) => {
                if let Some(index) = self.sub_tool_index(*tool, name) {
                    self.toolbox.pick(*tool);
                    self.toolbox.pick_sub(*tool, index);
                }
            }
            Item::Color(rgb) => self.set_drawing_color(*rgb),
            Item::AutoAction(name) => {
                if let Some(index) = self.auto.sets.iter().position(|set| &set.name == name) {
                    self.play_auto(index, cx);
                }
            }
            // A popup opens where its button is drawn; a group runs its
            // first item; a line does nothing.
            Item::Group(members) => {
                if let Some(first) = members.first() {
                    let first = first.item.clone();
                    self.run_item(&first, cx);
                }
            }
            Item::Popup(_) | Item::Separator => {}
        }
    }

    /// Where the sub tool `name` of `tool` is among its sub tools now.
    fn sub_tool_index(&self, tool: Tool, name: &str) -> Option<usize> {
        self.toolbox
            .subs(tool)
            .iter()
            .position(|sub| sub.name == name)
    }

    /// Whether a button can be pressed now, and whether it is on.
    pub(crate) fn item_state(&self, item: &Item, cx: &AppletContext<'_>) -> (bool, bool) {
        match item {
            Item::Command(command) if command == PALETTE_COMMAND => (true, false),
            Item::Command(command) => {
                let state = self.state(command, &sue_wm::Facts::of(cx));
                (state.enabled, state.checked == Some(true))
            }
            Item::Tool(tool) => (true, self.toolbox.tool() == *tool),
            Item::SubTool(tool, name) => match self.sub_tool_index(*tool, name) {
                Some(index) => (
                    true,
                    self.toolbox.tool() == *tool && self.toolbox.picked(*tool) == index,
                ),
                None => (false, false),
            },
            Item::Color(rgb) => (true, self.drawing_color() == *rgb),
            Item::AutoAction(name) => (self.auto.sets.iter().any(|set| &set.name == name), false),
            Item::Group(members) => members
                .first()
                .map_or((false, false), |first| self.item_state(&first.item, cx)),
            Item::Popup(_) | Item::Separator => (true, false),
        }
    }

    /// The key that runs `item` now, as menus show it (`Ctrl+Z`); empty
    /// for none.
    pub(crate) fn item_keys(&self, item: &Item, cx: &AppletContext<'_>) -> String {
        let Item::Command(command) = item else {
            return String::new();
        };
        if let Some(shown) = shortcut_in(&self.menus, command) {
            return shown.to_owned();
        }
        let registry = crate::commands::registry();
        let Some((found, _)) = registry.find(command) else {
            return String::new();
        };
        let section = registry.keys_section();
        sue_wm::shortcuts::chords_among(
            cx.rice(),
            section,
            command,
            &found.default_chords(),
            &sue_wm::shortcuts::SparseKeys::read(cx.rice(), section),
        )
        .first()
        .map(|chord| chord.to_string())
        .unwrap_or_default()
    }

    /// Takes a shared command bar or Quick Access file (Import in their
    /// settings): `true` when `purpose` was one of theirs.
    pub(crate) fn bar_picked(
        &mut self,
        purpose: &str,
        path: &Path,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        match purpose {
            "bar-import" | "quick-import" => {
                match sue_platform::fs::read_to_string(path) {
                    Ok(text) => {
                        let read = if purpose == "bar-import" {
                            text.trim_start()
                                .starts_with(BAR_HEADER)
                                .then(|| self.bar = bar_from(&text).0)
                        } else {
                            quick_from(&text).map(|quick| self.quick_access = quick)
                        };
                        if read.is_some() {
                            self.save_bars(cx);
                        } else {
                            cx.fail(tr!("not-a-bar-file"));
                        }
                    }
                    Err(error) => cx.fail(tr!("cannot-read-it", error = error)),
                }
                true
            }
            "bar-export" | "quick-export" => {
                let text = if purpose == "bar-export" {
                    bar_text(&self.bar)
                } else {
                    quick_text(&self.quick_access)
                };
                match sue_platform::fs::write(path, text) {
                    Ok(()) => cx.saved(path, tr!("command-bar-file")),
                    Err(error) => cx.fail(tr!("cannot-write-it", error = error.to_string())),
                }
                true
            }
            _ => false,
        }
    }
}

/// Which bar a button is on: the command bar, or a set of Quick Access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Holder {
    CommandBar,
    QuickAccess(usize),
}

/// Something dragged onto a command bar or Quick Access from elsewhere (a
/// palette, a menu, the other bar): the button it becomes. A palette that
/// lets its things be dragged out sets this as egui's drag payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BarPayload(pub Slot);

/// What the bars show between frames: the popover open (a palette that
/// pops up, a group's list, the icon picker), and the button being
/// dragged.
#[derive(Clone, Debug, Default)]
pub(crate) struct BarUi {
    /// A popover under a button: what it is for, the button, and where
    /// the button was drawn.
    pub open: Option<(Popover, Holder, usize, egui::Rect)>,
    /// The words typed into the icon picker's search.
    pub icon_query: String,
    /// The button being moved by a drag: its bar and place.
    pub dragging: Option<(Holder, usize)>,
    /// The button a right click was on (none: the bar itself), for the
    /// menu it opened.
    pub menu_target: Option<(Holder, Option<usize>)>,
}

/// The kind of popover a button opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Popover {
    /// The palette a popup button shows.
    Palette,
    /// A group's buttons.
    Group,
    /// Icon Settings: the icons to give the button.
    Icon,
}

impl SuePaint {
    /// The buttons of `holder`.
    #[must_use]
    pub(crate) fn holder_items(&self, holder: Holder) -> &[Slot] {
        match holder {
            Holder::CommandBar => &self.bar.items,
            Holder::QuickAccess(set) => self
                .quick_access
                .sets
                .get(set)
                .map_or(&[], |set| set.items.as_slice()),
        }
    }

    /// The buttons of `holder`, to change.
    pub(crate) fn holder_items_mut(&mut self, holder: Holder) -> Option<&mut Vec<Slot>> {
        match holder {
            Holder::CommandBar => Some(&mut self.bar.items),
            Holder::QuickAccess(set) => self
                .quick_access
                .sets
                .get_mut(set)
                .map(|set| &mut set.items),
        }
    }

    /// How buttons of `holder` move.
    #[must_use]
    pub(crate) fn holder_reorder(&self, holder: Holder) -> Reorder {
        match holder {
            Holder::CommandBar => self.bar.reorder,
            Holder::QuickAccess(_) => self.quick_access.reorder,
        }
    }

    /// Sets how buttons of `holder` move.
    pub(crate) fn set_holder_reorder(&mut self, holder: Holder, reorder: Reorder) {
        match holder {
            Holder::CommandBar => self.bar.reorder = reorder,
            Holder::QuickAccess(_) => self.quick_access.reorder = reorder,
        }
    }

    /// `true` while the dock draws `palette` itself: showing, and the tab
    /// in front of its group (or floating, in a window, or opened beside
    /// the strip). A palette behind another tab or out of the layout is
    /// not drawn, so a button may pop it up.
    #[must_use]
    pub(crate) fn palette_drawn(&self, palette: Palette) -> bool {
        if !self.palette_showing(palette) {
            return false;
        }
        let dock = &self.dock;
        let groups = dock
            .left
            .iter()
            .chain(&dock.right)
            .flat_map(|column| &column.groups)
            .chain(&dock.top)
            .chain(&dock.bottom);
        let mut docked = false;
        for group in groups {
            if let Some(place) = group.tabs.iter().position(|tab| *tab == palette) {
                docked = true;
                let shown = |tab: &&Palette| tab.shown(&self.panels);
                let front = group
                    .tabs
                    .get(group.active)
                    .filter(shown)
                    .or_else(|| group.tabs.iter().find(shown));
                if front.is_some_and(|tab| *tab == group.tabs[place]) {
                    return true;
                }
            }
        }
        // Not in a column or row: floating, in a window or popped out.
        !docked
    }
}

/// Edits of a bar's buttons, each a plain change of a list, shared by the
/// bars' right-click menus and their settings dialog.
pub(crate) mod edit {
    use super::{Item, Slot, tidy_lines};

    /// Puts `slot` at `at` (or at the end). Returns where it went.
    pub(crate) fn insert(items: &mut Vec<Slot>, at: Option<usize>, slot: Slot) -> usize {
        let at = at.map_or(items.len(), |at| at.min(items.len()));
        items.insert(at, slot);
        at
    }

    /// Takes the button at `at` away; lines left together become one.
    pub(crate) fn delete(items: &mut Vec<Slot>, at: usize) -> Option<Slot> {
        (at < items.len()).then(|| {
            let gone = items.remove(at);
            tidy_lines(items);
            gone
        })
    }

    /// Moves the button at `from` so it stands before the one that was at
    /// `to` (`to` may be the length: the end). Returns where it is now.
    pub(crate) fn move_to(items: &mut Vec<Slot>, from: usize, to: usize) -> usize {
        if from >= items.len() {
            return from;
        }
        let slot = items.remove(from);
        let to = if to > from { to - 1 } else { to }.min(items.len());
        items.insert(to, slot);
        to
    }

    /// Puts `slot` into the group at `at`: a group takes it at its end, a
    /// button becomes a group of itself and `slot`. A line takes nothing.
    pub(crate) fn into_group(items: &mut [Slot], at: usize, slot: Slot) -> bool {
        let Some(target) = items.get_mut(at) else {
            return false;
        };
        match &mut target.item {
            Item::Separator => false,
            Item::Group(members) => {
                members.push(slot);
                true
            }
            _ => {
                let alone = std::mem::replace(target, Slot::of(Item::Separator));
                *target = Slot::of(Item::Group(vec![alone, slot]));
                true
            }
        }
    }

    /// Brings the member `member` of the group at `at` to the front, so the
    /// group shows it and runs it.
    pub(crate) fn choose_in_group(items: &mut [Slot], at: usize, member: usize) {
        if let Some(Slot {
            item: Item::Group(members),
            ..
        }) = items.get_mut(at)
            && member < members.len()
        {
            let chosen = members.remove(member);
            members.insert(0, chosen);
        }
    }
}

/// Window, Quick Access, New Set (its palette menu): a new empty set,
/// shown.
pub(crate) fn new_quick_set(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: sue_wm::Call<'_>) {
    paint.quick_access.add_set();
    paint.save_bars(cx);
}

/// Delete Set: the set showing goes; the last one stays.
pub(crate) fn delete_quick_set(
    paint: &mut SuePaint,
    cx: &mut AppletContext<'_>,
    _: sue_wm::Call<'_>,
) {
    if paint.quick_access.delete_set() {
        paint.save_bars(cx);
    }
}

/// Delete Set works while there is more than one set.
pub(crate) fn delete_quick_set_state(
    paint: &SuePaint,
    _: &sue_wm::Facts<'_>,
    _: sue_wm::Call<'_>,
) -> sue_wm::ItemState {
    if paint.quick_access.sets.len() > 1 {
        sue_wm::ItemState::on()
    } else {
        sue_wm::ItemState::off()
    }
}

/// Quick Access Settings: the settings dialog, for the set showing.
pub(crate) fn quick_access_settings(
    paint: &mut SuePaint,
    _: &mut AppletContext<'_>,
    _: sue_wm::Call<'_>,
) {
    paint.dialog = Some(crate::dialogs::Dialog::CommandBar(
        crate::dialogs::BarDialog::new(crate::dialogs::BarTarget::QuickAccess),
    ));
}

/// Where the command bar's button `index` was drawn in the last frame of
/// `ctx`: for the snapshots, which click it.
#[must_use]
pub fn bar_button_rect(ctx: &egui::Context, index: usize) -> Option<egui::Rect> {
    let id = egui::Id::unique(("sue-paint-bar-button", Holder::CommandBar, index));
    ctx.read_response(id).map(|response| response.rect)
}

impl SuePaint {
    /// The routes of the snapshots: `bar/sample` puts a button of every
    /// kind on the command bar; `bar/icons/<n>`, `bar/group/<n>` and
    /// `bar/popup/<n>` open the popover of its button `n`; `bar/close`
    /// closes it, and `bar/default` puts Sue Paint's own buttons back.
    pub(crate) fn bar_navigate(&mut self, route: &str) -> bool {
        let rest = route.strip_prefix("bar/").unwrap_or(route);
        let open = |kind| {
            rest.rsplit('/')
                .next()
                .and_then(|index| index.parse::<usize>().ok())
                .map(|index| (kind, Holder::CommandBar, index, egui::Rect::NOTHING))
        };
        match rest {
            "sample" => {
                let mut items = Bar::default().items;
                items.extend([
                    Slot::of(Item::Separator),
                    Slot::of(Item::Tool(Tool::Pen)),
                    Slot::of(Item::Tool(Tool::Eraser)),
                    Slot::of(Item::SubTool(Tool::Pen, "G-pen".to_owned())),
                    Slot::of(Item::Popup(Palette::ColorSlider)),
                    Slot::of(Item::Separator),
                    Slot::of(Item::Color([0x1d, 0x1d, 0x1f])),
                    Slot::of(Item::Group(vec![
                        Slot::of(Item::Color([0xd6, 0x45, 0x45])),
                        Slot::of(Item::Color([0x3a, 0x7b, 0xd5])),
                    ])),
                ]);
                if let Some(first) = self.auto.sets.first() {
                    items.push(Slot::of(Item::AutoAction(first.name.clone())));
                }
                self.bar.items = items;
                true
            }
            "default" => {
                self.bar = Bar::default();
                self.bar_ui.open = None;
                true
            }
            "close" => {
                self.bar_ui.open = None;
                true
            }
            icons if icons.starts_with("icons/") => {
                self.bar_ui.open = open(Popover::Icon);
                true
            }
            group if group.starts_with("group/") => {
                self.bar_ui.open = open(Popover::Group);
                true
            }
            popup if popup.starts_with("popup/") => {
                self.bar_ui.open = open(Popover::Palette);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_keep_lines_tidy_and_groups_whole() {
        let mut items = grouped(&[&["edit.undo"], &["edit.redo"], &["view.fit"]]);
        // Deleting the middle button leaves one line between the others.
        edit::delete(&mut items, 2);
        assert_eq!(commands(&items), ["edit.undo", "|", "view.fit"]);
        // Moving the last to the front.
        assert_eq!(edit::move_to(&mut items, 2, 0), 0);
        assert_eq!(commands(&items), ["view.fit", "edit.undo", "|"]);
        // A move to the end.
        assert_eq!(edit::move_to(&mut items, 0, 3), 2);
        assert_eq!(commands(&items), ["edit.undo", "|", "view.fit"]);
        // A color put into a button makes a group; a second goes in it.
        assert!(edit::into_group(
            &mut items,
            0,
            Slot::of(Item::Color([1, 1, 1]))
        ));
        assert!(edit::into_group(
            &mut items,
            0,
            Slot::of(Item::Color([2, 2, 2]))
        ));
        assert!(!edit::into_group(
            &mut items,
            1,
            Slot::of(Item::Color([3, 3, 3]))
        ));
        let Item::Group(members) = &items[0].item else {
            panic!("a group: {items:?}");
        };
        assert_eq!(members.len(), 3);
        edit::choose_in_group(&mut items, 0, 2);
        let Item::Group(members) = &items[0].item else {
            panic!("a group");
        };
        assert_eq!(members[0].item, Item::Color([2, 2, 2]));
    }

    fn commands(items: &[Slot]) -> Vec<String> {
        items
            .iter()
            .map(|slot| match &slot.item {
                Item::Command(command) => command.clone(),
                Item::Separator => "|".to_owned(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn every_default_button_is_a_command_with_an_icon() {
        for slot in Bar::default().items.iter().chain(
            QuickAccess::default()
                .sets
                .iter()
                .flat_map(|set| &set.items),
        ) {
            // A group's buttons count too (the default bar's Save).
            let members = match &slot.item {
                Item::Group(members) => members.clone(),
                _ => vec![slot.clone()],
            };
            for slot in &members {
                let Item::Command(command) = &slot.item else {
                    continue;
                };
                assert!(known_command(command), "{command}");
                assert!(slot.shown_icon().is_some(), "{command} has no icon");
            }
        }
        for tool in Tool::ALL {
            assert!(Icon::from_name(tool_icon_name(tool)).is_some(), "{tool:?}");
        }
        for palette in <Palette as DockPanel>::all() {
            assert!(
                Icon::from_name(palette_icon_name(*palette)).is_some(),
                "{palette:?}"
            );
        }
    }

    #[test]
    fn every_kind_of_item_round_trips_through_its_file() {
        let bar = Bar {
            items: vec![
                Slot::command("file.new"),
                Slot::of(Item::Separator),
                Slot {
                    item: Item::Tool(Tool::Pen),
                    icon: Some("star".to_owned()),
                },
                Slot::of(Item::SubTool(Tool::Pen, "G-Pen".to_owned())),
                Slot::of(Item::Color([0x1e, 0x90, 0xff])),
                Slot::of(Item::Popup(Palette::Color)),
                Slot::of(Item::AutoAction("Flip and fit".to_owned())),
                Slot::of(Item::Group(vec![
                    Slot::of(Item::Color([255, 0, 0])),
                    Slot {
                        item: Item::Color([0, 255, 0]),
                        icon: Some("heart".to_owned()),
                    },
                ])),
                Slot::command(PALETTE_COMMAND),
            ],
            reorder: Reorder::Drag,
        };
        let text = bar_text(&bar);
        assert!(text.starts_with("sue-command-bar 1\nreorder drag\ncommand file.new\n"));
        assert!(text.contains("sub-tool pen G-Pen\n"));
        assert!(text.contains("color #1e90ff\n"), "{text}");
        assert_eq!(bar_from(&text), (bar, false));
        // Written in any language, the words stay English.
        let _german = sue_lang::scope("de");
        assert_eq!(bar_text(&bar_from(&text).0), text);
    }

    #[test]
    fn unknown_lines_and_lone_groups_are_skipped() {
        let items = items_from([
            "command no.such.command",
            "icon star",
            "command edit.undo",
            "icon heart",
            "wobble 3",
            "group",
            "separator",
            "command edit.redo",
            "end",
            "group",
            "end",
            "end",
            "group",
            "color 000000",
            "color FFFFFF",
        ]);
        assert_eq!(
            items,
            [
                Slot {
                    item: Item::Command("edit.undo".to_owned()),
                    icon: Some("heart".to_owned()),
                },
                Slot::command("edit.redo"),
                Slot::of(Item::Group(vec![
                    Slot::of(Item::Color([0, 0, 0])),
                    Slot::of(Item::Color([255, 255, 255])),
                ])),
            ]
        );
    }

    #[test]
    fn quick_access_keeps_its_sets_in_their_order() {
        let mut quick = QuickAccess::default();
        quick.add_set();
        quick.sets[2].items.push(Slot::command("view.fit"));
        quick.sets[2].name = "Inking".to_owned();
        let text = quick_text(&quick);
        assert_eq!(quick_from(&text), Some(quick.clone()));
        assert!(quick_from("not quick access").is_none());
        // The last set stays.
        let mut one = QuickAccess {
            sets: vec![quick.sets[0].clone()],
            ..quick
        };
        assert!(!one.delete_set());
        assert_eq!(one.sets.len(), 1);
    }

    #[test]
    fn the_first_set_is_the_one_clip_studio_ships() {
        let quick = QuickAccess::default();
        let names = commands(&quick.sets[0].items);
        assert_eq!(names.first().map(String::as_str), Some("edit.undo"));
        assert_eq!(
            names.last().map(String::as_str),
            Some("window.quick_access.settings")
        );
        assert!(names.contains(&"select.quick_mask".to_owned()));
        assert_eq!(quick.sets[0].name, "Set 1");
        assert_eq!(quick.sets[1].name, "Set 2");
    }

    #[test]
    fn the_command_bar_file_of_0_0_75_becomes_the_bar_of_now() {
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
        // Read as a bar: the default buttons but the hidden ones, then the
        // added ones after a line.
        let (bar, old) = bar_from(&text);
        assert!(old);
        let names = commands(&bar.items);
        assert!(!names.contains(&"edit.undo".to_owned()));
        assert!(!names.contains(&"view.fit".to_owned()));
        assert!(names.contains(&"edit.redo".to_owned()));
        assert_eq!(names[names.len() - 3..], ["|", "layer.new", "select.all"]);
        // Hiding a whole group leaves no two lines together.
        let all: HashSet<String> = ["edit.undo", "edit.redo"].map(String::from).into();
        let names = commands(&bar_of_old(&all, &[]).items);
        assert!(
            !names.windows(2).any(|pair| pair == ["|", "|"]),
            "{names:?}"
        );
    }

    #[test]
    fn a_workspace_kept_before_the_command_bar_was_in_it_leaves_the_bar() {
        let mut paint = SuePaint::default();
        paint.bar.items = vec![Slot::command("view.fit")];
        let old: Vec<String> = ["panels 1111111111", "menus 1"].map(String::from).to_vec();
        paint.apply_layout(&old);
        assert_eq!(paint.bar.items, [Slot::command("view.fit")]);
        // One kept now holds the bar, an empty one too.
        paint.bar.items.clear();
        let now = paint.workspace_now("").lines;
        paint.bar.items = vec![Slot::command("view.fit")];
        paint.apply_layout(&now);
        assert!(paint.bar.items.is_empty());
        // A workspace of 0.0.8's first builds: hide and add lines.
        let first: Vec<String> = ["bar", "bar hide edit.undo", "bar add layer.new"]
            .map(String::from)
            .to_vec();
        paint.apply_layout(&first);
        let names = commands(&paint.bar.items);
        assert!(!names.contains(&"edit.undo".to_owned()));
        assert_eq!(names.last().map(String::as_str), Some("layer.new"));
    }

    #[test]
    fn a_workspace_keeps_the_bar_and_quick_access_in_every_language() {
        let mut paint = SuePaint::default();
        paint.bar.items.push(Slot::of(Item::Color([1, 2, 3])));
        paint.quick_access.add_set();
        paint.quick_access.sets[2]
            .items
            .push(Slot::of(Item::Tool(Tool::Eraser)));
        let english = {
            let _english = sue_lang::scope(sue_lang::ENGLISH);
            paint.workspace_now("Inking").lines
        };
        for language in ["de", "ja"] {
            let _scope = sue_lang::scope(language);
            let lines = paint.workspace_now("Inking").lines;
            assert_eq!(lines, english, "{language}");
            let mut back = SuePaint::default();
            back.apply_layout(&lines);
            assert_eq!(back.bar, paint.bar, "{language}");
            assert_eq!(
                back.quick_access.sets, paint.quick_access.sets,
                "{language}"
            );
        }
        assert!(english.contains(&"bar command file.new".to_owned()));
        assert!(english.contains(&"quick tool eraser".to_owned()));
    }
}
