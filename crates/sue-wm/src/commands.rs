//! One command registry per studio.
//!
//! A studio lists what it can do as [`Command`]s in `const` tables, one
//! table per module that owns the commands, and adds them to one
//! [`CommandRegistry`]. Menus, the command palette, the shortcut settings,
//! the command bar, Auto Actions and layout styles then all read the same
//! list instead of each keeping their own.
//!
//! The registry is the interface level one. `sue_core::OperatorRegistry`
//! stays the data level one: an operator plans one transaction from shared
//! references, which is the wrong shape for "open a dialog" or "hide all
//! palettes". A command wraps an operator where it is a pure data edit.
//!
//! **Command names are saved data.** Shortcut settings, Auto Actions and
//! custom touch keys store them, so a name is never changed without a
//! migration.
//!
//! A table looks like this:
//!
//! ```text
//! pub(crate) const COMMANDS: &[Command<SuePaint>] = &[
//!     Command::new("file.save", "save", "file", SuePaint::save)
//!         .keys("Ctrl+S")
//!         .with(Flags::PALETTE),
//!     Command::family("file.recent.open.", "open-recent", "file", SuePaint::open_recent),
//! ];
//! ```
//!
//! `run` takes the studio by `&mut` while the registry is borrowed, so a
//! studio keeps its registry in a `static OnceLock` (or behind an `Arc`),
//! not as a plain field.

use std::collections::HashMap;
use std::ops::BitOr;

use sue_core::{Arena, UndoStack};
use sue_rice::{Keybinds, RiceSchema};

use crate::shortcuts::{self, Shortcut};
use crate::{AppletContext, ItemState, PaletteEntry, World};

/// What a command is besides its name: bit flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(u8);

impl Flags {
    /// No flag.
    pub const NONE: Self = Self(0);
    /// The name is a prefix (`file.recent.open.`) and what follows it is
    /// the command's argument: one entry stands for a family of commands.
    pub const FAMILY: Self = Self(1);
    /// Offered in the command palette.
    pub const PALETTE: Self = Self(1 << 1);
    /// Auto Actions may record it.
    pub const RECORDABLE: Self = Self(1 << 2);
    /// It quits, prints or opens a system dialog, so a test that runs
    /// every command on a canvas skips it.
    pub const LEAVES_CANVAS: Self = Self(1 << 3);
    /// Deliberately in no menu, no shortcut list and not in the palette.
    pub const HIDDEN: Self = Self(1 << 4);

    /// Both sets of flags together.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// `true` if every flag of `other` is set.
    #[must_use]
    pub const fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for Flags {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        self.with(other)
    }
}

/// What a command's `state` may look at besides the studio.
#[derive(Clone, Copy)]
pub struct Facts<'a> {
    /// The data arena.
    pub arena: &'a Arena,
    /// The undo tree of the document in focus.
    pub history: &'a UndoStack<World>,
    /// The live settings.
    pub rice: &'a RiceSchema,
}

impl<'a> Facts<'a> {
    /// The facts an applet call has at hand.
    #[must_use]
    pub fn of(cx: &'a AppletContext<'_>) -> Self {
        Self {
            arena: &cx.world.arena,
            history: &*cx.history,
            rice: &cx.world.rice,
        }
    }
}

/// One invocation: the full name and, for a family, what follows the
/// prefix (`3` for `file.recent.open.3`; empty for a plain command).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Call<'a> {
    /// The name as it was asked for.
    pub name: &'a str,
    /// What follows the family's prefix.
    pub arg: &'a str,
}

/// One command of a studio `S`.
pub struct Command<S: 'static> {
    /// Stable name, e.g. `layer.merge_down`; for a family the prefix,
    /// e.g. `file.recent.open.`. Saved data: never renamed without a
    /// migration.
    pub name: &'static str,
    /// Its message in the studio's language namespace.
    pub label: &'static str,
    /// Its group for the palette and for who owns it: `file`, `layer`.
    pub group: &'static str,
    /// Default chords as Settings writes them (`Ctrl+E`,
    /// `Ctrl+Shift+S or Ctrl+Alt+S`), or empty.
    pub chords: &'static str,
    /// Its icon's name in the shared icon set, or empty.
    pub icon: &'static str,
    /// What it is besides its name.
    pub flags: Flags,
    /// Enabled and checked right now.
    pub state: fn(&S, &Facts<'_>, Call<'_>) -> ItemState,
    /// Does it.
    pub run: fn(&mut S, &mut AppletContext<'_>, Call<'_>),
}

impl<S: 'static> Clone for Command<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: 'static> Copy for Command<S> {}

impl<S: 'static> std::fmt::Debug for Command<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("group", &self.group)
            .field("chords", &self.chords)
            .field("icon", &self.icon)
            .field("flags", &self.flags)
            .finish_non_exhaustive()
    }
}

/// The state of a command that always works.
fn always_on<S>(_: &S, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    ItemState::on()
}

impl<S: 'static> Command<S> {
    /// A command that always works, with no key, no icon and no flag.
    #[must_use]
    pub const fn new(
        name: &'static str,
        label: &'static str,
        group: &'static str,
        run: fn(&mut S, &mut AppletContext<'_>, Call<'_>),
    ) -> Self {
        Self {
            name,
            label,
            group,
            chords: "",
            icon: "",
            flags: Flags::NONE,
            state: always_on::<S>,
            run,
        }
    }

    /// A family: every name that starts with `prefix` and goes on.
    #[must_use]
    pub const fn family(
        prefix: &'static str,
        label: &'static str,
        group: &'static str,
        run: fn(&mut S, &mut AppletContext<'_>, Call<'_>),
    ) -> Self {
        Self::new(prefix, label, group, run).with(Flags::FAMILY)
    }

    /// The same command with these default chords.
    #[must_use]
    pub const fn keys(mut self, chords: &'static str) -> Self {
        self.chords = chords;
        self
    }

    /// The same command with this icon.
    #[must_use]
    pub const fn icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }

    /// The same command with these flags added.
    #[must_use]
    pub const fn with(mut self, flags: Flags) -> Self {
        self.flags = self.flags.with(flags);
        self
    }

    /// The same command, enabled and checked as `state` says.
    #[must_use]
    pub const fn state(mut self, state: fn(&S, &Facts<'_>, Call<'_>) -> ItemState) -> Self {
        self.state = state;
        self
    }

    /// `true` for a family.
    #[must_use]
    pub const fn is_family(&self) -> bool {
        self.flags.has(Flags::FAMILY)
    }

    /// Its default chords; none when they do not parse
    /// ([`CommandRegistry::problems`] reports that).
    #[must_use]
    pub fn default_chords(&self) -> Keybinds {
        self.chords.parse().unwrap_or_default()
    }
}

/// Every command of one studio.
pub struct CommandRegistry<S: 'static> {
    /// In the order they were added.
    commands: Vec<Command<S>>,
    /// Plain names to their place in `commands` (the first of a name).
    by_name: HashMap<&'static str, usize>,
    /// The families' places in `commands`, longest prefix first.
    families: Vec<usize>,
    /// Where the labels are in the language database.
    namespace: &'static str,
    /// The settings section the shortcuts live in.
    keys_section: &'static str,
}

impl<S: 'static> std::fmt::Debug for CommandRegistry<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandRegistry")
            .field("namespace", &self.namespace)
            .field("keys_section", &self.keys_section)
            .field("commands", &self.commands.len())
            .finish_non_exhaustive()
    }
}

impl<S: 'static> CommandRegistry<S> {
    /// An empty registry whose labels are messages of `namespace` and
    /// whose shortcuts are settings of `keys_section`.
    #[must_use]
    pub fn new(namespace: &'static str, keys_section: &'static str) -> Self {
        Self {
            commands: Vec::new(),
            by_name: HashMap::new(),
            families: Vec::new(),
            namespace,
            keys_section,
        }
    }

    /// Adds one module's table.
    pub fn add(&mut self, commands: &'static [Command<S>]) {
        for command in commands {
            let index = self.commands.len();
            self.commands.push(*command);
            if command.is_family() {
                self.families.push(index);
            } else {
                self.by_name.entry(command.name).or_insert(index);
            }
        }
        let commands = &self.commands;
        // Stable, so of two families with one prefix the first added wins.
        self.families
            .sort_by_key(|&index| std::cmp::Reverse(commands[index].name.len()));
    }

    /// The language namespace of the labels.
    #[must_use]
    pub const fn namespace(&self) -> &'static str {
        self.namespace
    }

    /// The settings section of the shortcuts.
    #[must_use]
    pub const fn keys_section(&self) -> &'static str {
        self.keys_section
    }

    /// How many entries it holds (a family counts once).
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// `true` with no command.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Every entry, in the order added.
    pub fn iter(&self) -> impl Iterator<Item = &Command<S>> {
        self.commands.iter()
    }

    /// The command `name` runs and the call it makes: a plain command of
    /// exactly that name, else the family with the longest prefix that
    /// `name` starts with and goes beyond.
    #[must_use]
    pub fn find<'a>(&self, name: &'a str) -> Option<(&Command<S>, Call<'a>)> {
        if let Some(&index) = self.by_name.get(name) {
            return Some((&self.commands[index], Call { name, arg: "" }));
        }
        self.families.iter().find_map(|&index| {
            let family = &self.commands[index];
            name.strip_prefix(family.name)
                .filter(|arg| !arg.is_empty())
                .map(|arg| (family, Call { name, arg }))
        })
    }

    /// `true` if `name` is a command here.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// The flags of `name` (none for a name that is not here).
    #[must_use]
    pub fn flags(&self, name: &str) -> Flags {
        self.find(name)
            .map_or(Flags::NONE, |(command, _)| command.flags)
    }

    /// How an item for `name` looks right now; grayed when the name is not
    /// a command here.
    #[must_use]
    pub fn state(&self, studio: &S, facts: &Facts<'_>, name: &str) -> ItemState {
        self.find(name)
            .map_or_else(ItemState::off, |(command, call)| {
                (command.state)(studio, facts, call)
            })
    }

    /// Runs `name`. Returns `false` if it is not a command here.
    pub fn run(&self, studio: &mut S, cx: &mut AppletContext<'_>, name: &str) -> bool {
        let Some((command, call)) = self.find(name) else {
            return false;
        };
        (command.run)(studio, cx, call);
        true
    }

    /// What `name` is called in the language in use (a family's members
    /// share the family's label).
    #[must_use]
    pub fn label(&self, name: &str) -> Option<String> {
        self.find(name)
            .map(|(command, _)| sue_lang::text(self.namespace, command.label))
    }

    /// A row of the shortcut table for every command that can be given a
    /// key: all but the families and the hidden ones, with or without a
    /// default chord, labeled in English (as settings are declared).
    #[must_use]
    pub fn shortcuts(&self) -> Vec<Shortcut> {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        self.listed()
            .map(|command| Shortcut {
                command: command.name,
                label: sue_lang::text(self.namespace, command.label)
                    .trim_end_matches("...")
                    .to_owned(),
                chords: command.default_chords(),
            })
            .collect()
    }

    /// The command palette's entries: every command flagged
    /// [`Flags::PALETTE`], with the first chord it has in `rice` now.
    #[must_use]
    pub fn palette(&self, rice: &RiceSchema) -> Vec<PaletteEntry> {
        let sparse = shortcuts::SparseKeys::read(rice, self.keys_section);
        self.listed()
            .filter(|command| command.flags.has(Flags::PALETTE))
            .map(|command| PaletteEntry {
                label: sue_lang::text(self.namespace, command.label),
                command: command.name.to_owned(),
                hint: shortcuts::chords_among(
                    rice,
                    self.keys_section,
                    command.name,
                    &command.default_chords(),
                    &sparse,
                )
                .first()
                .map(|chord| chord.to_string())
                .unwrap_or_default(),
            })
            .collect()
    }

    /// What is wrong with the tables, one line each (none in a good
    /// build): a name used twice, a name the shortcut settings cannot
    /// hold, chords that do not parse, a label with no English message, a
    /// family whose prefix shadows a plain name, and one default chord
    /// held by two commands.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for command in &self.commands {
            let name = command.name;
            *seen.entry(name).or_default() += 1;
            if seen[name] == 2 {
                problems.push(format!("{name}: the name is used twice"));
            }
            if name.is_empty()
                || name == "or"
                || name.contains(|ch: char| ch.is_whitespace() || ch == '=')
            {
                problems.push(format!(
                    "{name:?}: a name is not empty, not \"or\", and has no space and no ="
                ));
            }
            if let Err(error) = command.chords.parse::<Keybinds>() {
                problems.push(format!(
                    "{name}: the chords {:?} do not parse ({error})",
                    command.chords
                ));
            }
            if !sue_lang::has(self.namespace, command.label) {
                problems.push(format!(
                    "{name}: the label {:?} has no English message in {}.ftl",
                    command.label, self.namespace
                ));
            }
        }
        for &index in &self.families {
            let prefix = self.commands[index].name;
            for command in self.commands.iter().filter(|command| !command.is_family()) {
                if command.name.len() > prefix.len() && command.name.starts_with(prefix) {
                    problems.push(format!(
                        "{prefix}: the family's prefix shadows the name {}",
                        command.name
                    ));
                }
            }
        }
        let defaults: Vec<(&str, Keybinds)> = self
            .listed()
            .map(|command| (command.name, command.default_chords()))
            .collect();
        for conflict in shortcuts::conflicts_among(&defaults) {
            problems.push(format!(
                "{}: one chord held by {}",
                conflict.chord,
                conflict.commands.join(" and ")
            ));
        }
        problems
    }

    /// The commands that can be given a key, each name once.
    fn listed(&self) -> impl Iterator<Item = &Command<S>> {
        self.commands
            .iter()
            .enumerate()
            .filter(|(index, command)| {
                !command.is_family()
                    && !command.flags.has(Flags::HIDDEN)
                    && self.by_name.get(command.name) == Some(index)
            })
            .map(|(_, command)| command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_rice::RiceValue;

    /// A toy studio: a counter, a grid switch and the last file opened.
    #[derive(Default)]
    struct Toy {
        count: i32,
        grid: bool,
        opened: String,
    }

    fn save(toy: &mut Toy, _: &mut AppletContext<'_>, _: Call<'_>) {
        toy.count += 1;
    }

    fn grid(toy: &mut Toy, _: &mut AppletContext<'_>, _: Call<'_>) {
        toy.grid = !toy.grid;
    }

    fn grid_state(toy: &Toy, _: &Facts<'_>, _: Call<'_>) -> ItemState {
        ItemState::check(toy.grid)
    }

    fn undo_state(_: &Toy, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
        if facts.history.can_undo() {
            ItemState::on()
        } else {
            ItemState::off()
        }
    }

    fn open(toy: &mut Toy, _: &mut AppletContext<'_>, call: Call<'_>) {
        toy.opened = format!("{} ({})", call.arg, call.name);
    }

    fn recent_state(toy: &Toy, _: &Facts<'_>, call: Call<'_>) -> ItemState {
        ItemState::check(toy.opened.starts_with(call.arg))
    }

    fn nothing(_: &mut Toy, _: &mut AppletContext<'_>, _: Call<'_>) {}

    // The labels are messages of the shell's own namespace, `wm`.
    const FILE: &[Command<Toy>] = &[
        Command::new("file.save", "save", "file", save)
            .keys("Ctrl+S or F12")
            .icon("save")
            .with(Flags::PALETTE.with(Flags::RECORDABLE)),
        Command::new("app.quit", "cancel", "file", nothing).with(Flags::LEAVES_CANVAS),
        Command::family("file.recent.open.", "none", "file", open).state(recent_state),
        Command::family("file.recent.open.pinned.", "none", "file", nothing),
    ];
    const VIEW: &[Command<Toy>] = &[
        Command::new("view.grid", "none", "view", grid)
            .state(grid_state)
            .with(Flags::PALETTE),
        Command::new("edit.undo", "cancel", "edit", nothing).state(undo_state),
        Command::new("debug.dump", "none", "view", nothing)
            .keys("Ctrl+Alt+D")
            .with(Flags::HIDDEN.with(Flags::PALETTE)),
    ];

    fn registry() -> CommandRegistry<Toy> {
        let mut registry = CommandRegistry::new("wm", "toy_keys");
        registry.add(FILE);
        registry.add(VIEW);
        registry
    }

    /// Runs `work` with an applet context over an empty world.
    fn with_context<R>(work: impl FnOnce(&mut AppletContext<'_>) -> R) -> R {
        let rice = RiceSchema::new();
        let theme = crate::Theme::from_rice(&rice);
        let mut world = World {
            arena: Arena::new(),
            rice,
        };
        let mut history = UndoStack::new();
        let mut typing = crate::Typing::default();
        let mut requests = Vec::new();
        let catalog = crate::Catalog::default();
        let mut textures = sue_platform::NoTextures;
        let mut transport = crate::Transport::default();
        let work_list = crate::WorkList::default();
        let mut transfers = crate::Transfers::default();
        let mut cx = AppletContext {
            world: &mut world,
            history: &mut history,
            sue_history: None,
            typing: &mut typing,
            requests: &mut requests,
            catalog: &catalog,
            theme: &theme,
            textures: &mut textures,
            audio: None,
            transport: &mut transport,
            work: &work_list,
            transfers: &mut transfers,
            view: crate::ShellView {
                form_factor: sue_platform::FormFactor::Desktop,
                profile: sue_rice::HardwareProfile::Balanced,
                now: std::time::Instant::now(),
                dt: 0.0,
                page: None,
                reveal: 1.0,
                interactive: false,
            },
        };
        work(&mut cx)
    }

    #[test]
    fn a_good_table_has_no_problems() {
        assert_eq!(registry().problems(), Vec::<String>::new());
    }

    #[test]
    fn a_name_finds_its_command_and_runs_it() {
        let registry = registry();
        assert_eq!(registry.len(), 7);
        assert!(!registry.is_empty());
        assert_eq!(registry.namespace(), "wm");
        assert_eq!(registry.keys_section(), "toy_keys");
        let (command, call) = registry.find("file.save").unwrap();
        assert_eq!(command.name, "file.save");
        assert_eq!(command.group, "file");
        assert_eq!(command.icon, "save");
        assert_eq!(
            call,
            Call {
                name: "file.save",
                arg: ""
            }
        );
        assert!(registry.find("file.sav").is_none());
        assert!(registry.find("file.save.more").is_none());
        assert!(!registry.has("layer.new"));

        let mut toy = Toy::default();
        with_context(|cx| {
            assert!(registry.run(&mut toy, cx, "file.save"));
            assert!(registry.run(&mut toy, cx, "file.save"));
            assert!(registry.run(&mut toy, cx, "view.grid"));
            assert!(!registry.run(&mut toy, cx, "layer.new"), "not a command");
        });
        assert_eq!(toy.count, 2);
        assert!(toy.grid);
    }

    #[test]
    fn a_family_takes_what_follows_its_prefix() {
        let registry = registry();
        let (family, call) = registry.find("file.recent.open.3").unwrap();
        assert_eq!(family.name, "file.recent.open.");
        assert!(family.is_family());
        assert_eq!(
            call,
            Call {
                name: "file.recent.open.3",
                arg: "3"
            }
        );
        // The longest prefix wins, whatever the order of the tables.
        let (family, call) = registry.find("file.recent.open.pinned.2").unwrap();
        assert_eq!(family.name, "file.recent.open.pinned.");
        assert_eq!(call.arg, "2");
        // The bare prefix is no command.
        assert!(registry.find("file.recent.open.").is_none());
        assert!(registry.find("file.recent.open").is_none());
        assert!(registry.flags("file.recent.open.9").has(Flags::FAMILY));

        let mut toy = Toy::default();
        with_context(|cx| {
            assert!(registry.run(&mut toy, cx, "file.recent.open.3"));
            assert!(!registry.run(&mut toy, cx, "file.recent.open."));
            // A family's state sees the argument too.
            let facts = Facts::of(cx);
            assert_eq!(
                registry.state(&toy, &facts, "file.recent.open.3"),
                ItemState::check(true)
            );
            assert_eq!(
                registry.state(&toy, &facts, "file.recent.open.4"),
                ItemState::check(false)
            );
        });
        assert_eq!(toy.opened, "3 (file.recent.open.3)");
    }

    #[test]
    fn state_reads_the_studio_and_the_facts() {
        let registry = registry();
        let mut toy = Toy::default();
        with_context(|cx| {
            let facts = Facts::of(cx);
            assert_eq!(registry.state(&toy, &facts, "file.save"), ItemState::on());
            assert_eq!(
                registry.state(&toy, &facts, "view.grid"),
                ItemState::check(false)
            );
            assert_eq!(
                registry.state(&toy, &facts, "edit.undo"),
                ItemState::off(),
                "nothing to undo in a new history"
            );
            assert_eq!(
                registry.state(&toy, &facts, "layer.new"),
                ItemState::off(),
                "an unknown name is grayed"
            );
            assert!(registry.run(&mut toy, cx, "view.grid"));
            let facts = Facts::of(cx);
            assert_eq!(
                registry.state(&toy, &facts, "view.grid"),
                ItemState::check(true)
            );
        });
    }

    #[test]
    fn flags_say_what_a_command_is() {
        let registry = registry();
        let save = registry.flags("file.save");
        assert!(save.has(Flags::PALETTE));
        assert!(save.has(Flags::RECORDABLE));
        assert!(save.has(Flags::PALETTE | Flags::RECORDABLE));
        assert!(!save.has(Flags::LEAVES_CANVAS));
        assert!(!save.has(Flags::PALETTE | Flags::HIDDEN));
        assert!(registry.flags("app.quit").has(Flags::LEAVES_CANVAS));
        assert!(!registry.flags("app.quit").has(Flags::RECORDABLE));
        assert!(registry.flags("debug.dump").has(Flags::HIDDEN));
        assert_eq!(registry.flags("layer.new"), Flags::NONE);
        assert!(save.has(Flags::NONE));
        // The test that runs every command skips those that leave.
        let stay: Vec<&str> = registry
            .iter()
            .filter(|command| !command.flags.has(Flags::LEAVES_CANVAS))
            .map(|command| command.name)
            .collect();
        assert!(!stay.contains(&"app.quit"));
        assert_eq!(stay.len(), 6);
        // What Auto Actions may record.
        let recordable: Vec<&str> = registry
            .iter()
            .filter(|command| command.flags.has(Flags::RECORDABLE))
            .map(|command| command.name)
            .collect();
        assert_eq!(recordable, ["file.save"]);
    }

    #[test]
    fn labels_come_from_the_language_database() {
        let registry = registry();
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        assert_eq!(registry.label("file.save").as_deref(), Some("Save"));
        assert_eq!(
            registry.label("file.recent.open.2").as_deref(),
            Some("None")
        );
        assert_eq!(registry.label("layer.new"), None);
    }

    #[test]
    fn every_command_that_can_have_a_key_gets_a_row() {
        let rows = registry().shortcuts();
        let names: Vec<&str> = rows.iter().map(|row| row.command).collect();
        // No family and no hidden command; a command without a default
        // chord has a row all the same.
        assert_eq!(names, ["file.save", "app.quit", "view.grid", "edit.undo"]);
        assert_eq!(rows[0].label, "Save");
        assert_eq!(rows[0].chords.to_string(), "Ctrl+S or F12");
        assert!(rows[1].chords.is_empty());
        assert!(rows[2].chords.is_empty());
    }

    #[test]
    fn the_palette_lists_flagged_commands_with_the_key_set_now() {
        let registry = registry();
        let table = registry.shortcuts();
        let mut rice = RiceSchema::new();
        rice.register_all(&shortcuts::declarations("toy_keys", &table))
            .unwrap();
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let entry = |label: &str, command: &str, hint: &str| PaletteEntry {
            label: label.to_owned(),
            command: command.to_owned(),
            hint: hint.to_owned(),
        };
        // The hidden command is flagged for the palette and still left out.
        assert_eq!(
            registry.palette(&rice),
            [
                entry("Save", "file.save", "Ctrl+S"),
                entry("None", "view.grid", "")
            ]
        );
        // A key someone set shows, for a command with a default and for
        // one without.
        rice.set(
            "toy_keys.file_save",
            RiceValue::Keybind("F9".parse().unwrap()),
        )
        .unwrap();
        let (path, value) =
            shortcuts::bind(&rice, "toy_keys", "view.grid", &"Ctrl+G".parse().unwrap());
        rice.set(&path, value).unwrap();
        assert_eq!(
            registry.palette(&rice),
            [
                entry("Save", "file.save", "F9"),
                entry("None", "view.grid", "Ctrl+G")
            ]
        );
    }

    fn broken(_: &mut Toy, _: &mut AppletContext<'_>, _: Call<'_>) {}

    const BROKEN: &[Command<Toy>] = &[
        Command::new("file.save", "save", "file", broken).keys("Ctrl+S"),
        Command::new("file.save", "save", "file", broken),
        Command::new("file.print", "save", "file", broken).keys("Ctrl+Nonsense"),
        Command::new("file.export", "no-such-message-anywhere", "file", broken),
        Command::family("layer.", "none", "layer", broken),
        Command::new("layer.new", "none", "layer", broken).keys("Ctrl+Shift+N"),
        Command::new("layer.copy", "none", "layer", broken).keys("F5 or Ctrl+Shift+N"),
        Command::new("view.fit", "none", "view", broken).keys("Super+S"),
        Command::new("view grid", "none", "view", broken),
    ];

    #[test]
    fn problems_name_what_is_wrong_with_a_table() {
        let mut registry = CommandRegistry::new("wm", "toy_keys");
        registry.add(BROKEN);
        let problems = registry.problems();
        let has = |part: &str| problems.iter().any(|line| line.contains(part));
        assert!(has("file.save: the name is used twice"), "{problems:?}");
        assert!(
            has("file.print: the chords \"Ctrl+Nonsense\" do not parse"),
            "{problems:?}"
        );
        assert!(
            has("file.export: the label \"no-such-message-anywhere\" has no English message"),
            "{problems:?}"
        );
        assert!(
            has("layer.: the family's prefix shadows the name layer.new"),
            "{problems:?}"
        );
        assert!(
            has("layer.: the family's prefix shadows the name layer.copy"),
            "{problems:?}"
        );
        // One chord, two commands: today the first row eats the key.
        assert!(
            has("Ctrl+Shift+N: one chord held by layer.new and layer.copy"),
            "{problems:?}"
        );
        // Ctrl and Super are one key to the shell (the command key).
        assert!(
            has("one chord held by file.save and view.fit"),
            "{problems:?}"
        );
        assert!(has("\"view grid\": a name is not empty"), "{problems:?}");
        assert_eq!(problems.len(), 8, "{problems:?}");
        // The first of a name is the one that runs, and a plain name wins
        // over a family.
        assert_eq!(registry.find("file.save").unwrap().0.chords, "Ctrl+S");
        assert!(!registry.find("layer.new").unwrap().0.is_family());
        assert!(registry.find("layer.other").unwrap().0.is_family());
        // The table has one row per name, and bad chords are no chords.
        let rows = registry.shortcuts();
        assert_eq!(
            rows.iter().filter(|row| row.command == "file.save").count(),
            1
        );
        let print = rows.iter().find(|row| row.command == "file.print").unwrap();
        assert!(print.chords.is_empty());
    }
}
