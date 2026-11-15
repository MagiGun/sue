//! The Auto Action palette: commands recorded into named actions and
//! played back, in sets that can be shared as files.
//!
//! The palette (gaps CTV-3, FGV-16, A25V-26 of the 0.0.8 plan) has a set
//! picker at the top, a row per action with a check box (an action that
//! is off cannot be played) and an arrow that unfolds its steps, each step
//! with a check box of its own (a step that is off is skipped), and four
//! buttons at the bottom: Record (which turns into Stop while recording),
//! Play, New and Delete. Its palette menu makes, deletes, imports and
//! exports sets. Every action can be given a key (Settings, Keyboard
//! Shortcuts, Auto Action): the key runs `auto.run.<id>`.
//!
//! Recording keeps every command flagged `RECORDABLE` that runs while it
//! lasts, wherever it was run from: the menus, a key, the command bar,
//! Quick Access or a palette's button (gap CT-33).
//!
//! The sets live in `auto_actions.txt` in Sue Paint's data folder, in the
//! format `docs/paint-auto-action-files.md` describes; an exported set is
//! the same format in a `.sueactions` file. A file of Sue Paint 0.0.75
//! (sections of command names) still reads, as one set.

use egui::{Rect, Ui, pos2, vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, Tip, TreeRow};
use sue_wm::{AppletContext, AppletId, FilePick, ShellRequest, Theme};

use crate::studio::SuePaint;

/// The first line of an Auto Action file.
pub(crate) const HEADER: &str = "sue-auto-actions 1";

/// The extension of an exported set.
pub(crate) const EXTENSION: &str = "sueactions";

/// The family that plays an action by its id: `auto.run.<id>`.
pub(crate) const RUN_PREFIX: &str = "auto.run.";

/// The commands of the Auto Action palette's menu.
pub(crate) const MENU_COMMANDS: [&str; 8] = [
    "auto.new_set",
    "auto.rename_set",
    "auto.delete_set",
    "auto.import_set",
    "auto.export_set",
    "auto.rename",
    "auto.duplicate",
    "auto.restore_defaults",
];

/// A named list of commands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AutoAction {
    pub name: String,
    pub steps: Vec<String>,
    /// What its key is kept under (`auto.run.<id>`): made from its name
    /// once and kept when it is renamed.
    pub id: String,
    /// It is turned off: it cannot be played.
    pub off: bool,
    /// Its steps that are turned off (skipped when played), by place;
    /// shorter than `steps` when the last ones are on.
    pub steps_off: Vec<bool>,
}

impl AutoAction {
    /// An action named `name` with `steps`, all on.
    pub(crate) fn new(name: &str, steps: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            steps: steps.iter().map(|step| (*step).to_owned()).collect(),
            id: slug(name),
            ..Self::default()
        }
    }

    /// Whether step `index` is on.
    pub(crate) fn step_on(&self, index: usize) -> bool {
        !self.steps_off.get(index).copied().unwrap_or(false)
    }

    /// Turns step `index` on or off.
    pub(crate) fn set_step(&mut self, index: usize, on: bool) {
        if index >= self.steps.len() {
            return;
        }
        if self.steps_off.len() <= index {
            self.steps_off.resize(index + 1, false);
        }
        self.steps_off[index] = !on;
        self.tidy();
    }

    /// Takes step `index` out.
    pub(crate) fn remove_step(&mut self, index: usize) {
        if index < self.steps.len() {
            self.steps.remove(index);
            if index < self.steps_off.len() {
                self.steps_off.remove(index);
            }
            self.tidy();
        }
    }

    /// The steps that play, in order.
    pub(crate) fn playing(&self) -> Vec<String> {
        self.steps
            .iter()
            .enumerate()
            .filter(|(index, _)| self.step_on(*index))
            .map(|(_, step)| step.clone())
            .collect()
    }

    /// Drops the "off" marks past the last one that is set, so two
    /// actions that play alike compare alike.
    fn tidy(&mut self) {
        self.steps_off.truncate(self.steps.len());
        while self.steps_off.last() == Some(&false) {
            self.steps_off.pop();
        }
    }
}

/// A named set of actions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ActionSet {
    pub name: String,
    pub actions: Vec<AutoAction>,
}

/// The auto actions and what is being recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AutoActions {
    /// The actions of the set in use.
    pub sets: Vec<AutoAction>,
    /// The action chosen in the palette.
    pub selected: usize,
    /// The action commands are being recorded into.
    pub recording: Option<usize>,
    pub loaded: bool,
    /// The name of the set in use.
    pub set_name: String,
    /// Every other set; the set in use stands at `in_use` among them.
    pub others: Vec<ActionSet>,
    /// The place of the set in use among every set.
    pub in_use: usize,
    /// The step chosen in the chosen action, if a step is.
    pub step: Option<usize>,
    /// The actions whose steps show, by id.
    pub open: Vec<String>,
    /// The action being renamed, and its name as typed.
    pub renaming: Option<(usize, String)>,
    /// The set being renamed, as typed.
    pub renaming_set: Option<String>,
}

/// An id made of `name`: lower case letters and digits, dashes between
/// words (`Flip Canvas and Fit` is `flip-canvas-and-fit`); `action` when
/// nothing of the name is left.
pub(crate) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "action".to_owned()
    } else {
        out
    }
}

/// Reads the lines of one action (`id`, `off`, `step`) into `action`.
fn read_line(action: &mut AutoAction, line: &str) {
    if let Some(id) = line.strip_prefix("id ") {
        let id = id.trim();
        if sue_wm::shortcuts::SparseKeys::holds(&format!("{RUN_PREFIX}{id}")) {
            id.clone_into(&mut action.id);
        }
    } else if line == "off" {
        action.off = true;
    } else if let Some(step) = line.strip_prefix("step ") {
        let (on, command) = match step.strip_prefix("off ") {
            Some(command) => (false, command),
            None => (true, step),
        };
        action.steps.push(command.trim().to_owned());
        let index = action.steps.len() - 1;
        action.set_step(index, on);
    }
}

impl AutoActions {
    /// The actions of an Auto Action file's first set, or of a file of
    /// Sue Paint 0.0.75 (sections of command names).
    #[cfg(test)]
    #[must_use]
    pub fn parse(text: &str) -> Vec<AutoAction> {
        Self::read_sets(text)
            .0
            .into_iter()
            .next()
            .map(|set| set.actions)
            .unwrap_or_default()
    }

    /// Every set of an Auto Action file, and which one was in use; a file
    /// of 0.0.75 is one set of Sue Paint's name.
    #[must_use]
    pub fn read_sets(text: &str) -> (Vec<ActionSet>, usize) {
        let mut sets: Vec<ActionSet> = Vec::new();
        let mut in_use = 0;
        let new = text.lines().next().map(str::trim) == Some(HEADER);
        if !new {
            let actions = crate::extras::sections(text)
                .into_iter()
                .map(|(name, steps)| AutoAction {
                    id: slug(&name),
                    name,
                    steps,
                    ..AutoAction::default()
                })
                .collect();
            sets.push(ActionSet {
                name: tr!("auto-set-sue-paint"),
                actions,
            });
            return (Self::unique(sets), 0);
        }
        for line in text.lines().skip(1) {
            let line = line.trim_end();
            if let Some(name) = line.strip_prefix("set ") {
                sets.push(ActionSet {
                    name: name.to_owned(),
                    actions: Vec::new(),
                });
            } else if line == "in-use" {
                in_use = sets.len().saturating_sub(1);
            } else if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                if sets.is_empty() {
                    sets.push(ActionSet {
                        name: tr!("auto-set-sue-paint"),
                        actions: Vec::new(),
                    });
                }
                if let Some(set) = sets.last_mut() {
                    set.actions.push(AutoAction {
                        id: slug(name),
                        name: name.to_owned(),
                        ..AutoAction::default()
                    });
                }
            } else if let Some(action) = sets.last_mut().and_then(|set| set.actions.last_mut()) {
                read_line(action, line.trim_start());
            }
        }
        let in_use = in_use.min(sets.len().saturating_sub(1));
        (Self::unique(sets), in_use)
    }

    /// `sets` with every action's id told apart from the others' (a
    /// second `fit` becomes `fit-2`).
    fn unique(mut sets: Vec<ActionSet>) -> Vec<ActionSet> {
        let mut seen: Vec<String> = Vec::new();
        for action in sets.iter_mut().flat_map(|set| set.actions.iter_mut()) {
            let base = action.id.clone();
            let mut n = 2;
            while seen.contains(&action.id) {
                action.id = format!("{base}-{n}");
                n += 1;
            }
            seen.push(action.id.clone());
        }
        sets
    }

    /// One set as an Auto Action file.
    #[cfg(test)]
    #[must_use]
    pub fn write(actions: &[AutoAction]) -> String {
        Self::write_sets(
            &[ActionSet {
                name: tr!("auto-set-sue-paint"),
                actions: actions.to_vec(),
            }],
            0,
        )
    }

    /// `sets` as an Auto Action file, with the one at `in_use` marked.
    #[must_use]
    pub fn write_sets(sets: &[ActionSet], in_use: usize) -> String {
        let mut out = format!("{HEADER}\n");
        for (index, set) in sets.iter().enumerate() {
            out.push_str(&format!("set {}\n", one_line(&set.name)));
            if index == in_use && sets.len() > 1 {
                out.push_str("in-use\n");
            }
            for action in &set.actions {
                out.push_str(&format!("[{}]\n", one_line(&action.name)));
                out.push_str(&format!("id {}\n", action.id));
                if action.off {
                    out.push_str("off\n");
                }
                for (step, command) in action.steps.iter().enumerate() {
                    let off = if action.step_on(step) { "" } else { "off " };
                    out.push_str(&format!("step {off}{command}\n"));
                }
            }
        }
        out
    }

    /// The actions a new install starts with: nine of Sue Paint's own,
    /// each a short run of commands people repeat.
    #[must_use]
    pub fn builtin() -> Vec<AutoAction> {
        [
            (
                tr!("flip-canvas-and-fit"),
                &["canvas.flip_h", "view.fit"][..],
            ),
            (
                tr!("rotate-canvas-right"),
                &["canvas.rotate_right", "view.fit"][..],
            ),
            (
                tr!("auto-reset-the-view"),
                &["view.reset_turn", "view.fit"][..],
            ),
            (
                tr!("auto-new-clipped-layer"),
                &["layer.new", "layer.toggle.clip"][..],
            ),
            (
                tr!("auto-new-draft-layer"),
                &["layer.new", "layer.toggle.draft"][..],
            ),
            (
                tr!("auto-gray-copy"),
                &["layer.duplicate", "filter.grayscale"][..],
            ),
            (
                tr!("auto-fill-and-deselect"),
                &["edit.fill", "select.none"][..],
            ),
            (
                tr!("auto-clear-outside-and-deselect"),
                &["edit.clear_outside", "select.none"][..],
            ),
            (
                tr!("auto-merge-visible-to-new-layer"),
                &["layer.merge_visible"][..],
            ),
        ]
        .iter()
        .map(|(name, steps)| AutoAction::new(name, steps))
        .collect()
    }

    /// Every set in order, the set in use among them.
    #[must_use]
    pub fn all_sets(&self) -> Vec<ActionSet> {
        let mut sets = self.others.clone();
        sets.insert(
            self.in_use.min(sets.len()),
            ActionSet {
                name: self.set_name.clone(),
                actions: self.sets.clone(),
            },
        );
        sets
    }

    /// Takes `sets`, with the one at `in_use` in use.
    pub fn take_sets(&mut self, mut sets: Vec<ActionSet>, in_use: usize) {
        if sets.is_empty() {
            sets.push(ActionSet {
                name: tr!("auto-set-sue-paint"),
                actions: Self::builtin(),
            });
        }
        let in_use = in_use.min(sets.len() - 1);
        let chosen = sets.remove(in_use);
        self.set_name = chosen.name;
        self.sets = chosen.actions;
        self.others = sets;
        self.in_use = in_use;
        self.selected = 0;
        self.step = None;
        self.recording = None;
        self.renaming = None;
    }

    /// Reads them from the text of `auto_actions.txt`, or starts with Sue
    /// Paint's own set.
    pub fn load(&mut self, text: Option<&str>) {
        let (sets, in_use) = text.map(Self::read_sets).unwrap_or_default();
        self.take_sets(sets, in_use);
    }

    /// Them as the text of `auto_actions.txt`.
    #[must_use]
    pub fn to_text(&self) -> String {
        Self::write_sets(&self.all_sets(), self.in_use)
    }

    /// The names of every set, in order.
    #[must_use]
    pub fn set_names(&self) -> Vec<String> {
        self.all_sets().into_iter().map(|set| set.name).collect()
    }

    /// Uses set `index` from now on.
    pub fn use_set(&mut self, index: usize) {
        if index != self.in_use {
            let sets = self.all_sets();
            self.take_sets(sets, index);
        }
    }

    /// An id for an action named `name` that no action of any set has.
    #[must_use]
    pub fn fresh_id(&self, name: &str) -> String {
        let base = slug(name);
        let taken: Vec<String> = self
            .all_sets()
            .into_iter()
            .flat_map(|set| set.actions)
            .map(|action| action.id)
            .collect();
        let mut id = base.clone();
        let mut n = 2;
        while taken.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// The steps that play of the action `id`, in whichever set it is,
    /// and its name; none for an action that is off.
    #[must_use]
    pub fn steps_of(&self, id: &str) -> Option<(String, Vec<String>)> {
        self.all_sets()
            .into_iter()
            .flat_map(|set| set.actions)
            .find(|action| action.id == id && !action.off)
            .map(|action| (action.name.clone(), action.playing()))
    }
}

/// `text` on one line, for a file's `set` and `[name]` lines.
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r', '[', ']'], " ").trim().to_owned()
}

impl SuePaint {
    /// Keeps the Auto Actions in Sue Paint's data folder.
    pub(crate) fn save_auto(&mut self, cx: &mut AppletContext<'_>) {
        let folder = crate::colorsets::folder(cx);
        if let Err(error) = crate::extras::write(
            folder.as_deref(),
            crate::extras::AUTO_FILE,
            &self.auto.to_text(),
        ) {
            cx.fail(tr!("cannot-keep-the-palettes-settings", error = error));
        }
    }

    /// Plays auto action `index`: each of its steps that is on, in turn.
    pub(crate) fn play_auto(&mut self, index: usize, cx: &mut AppletContext<'_>) {
        let Some(steps) = self
            .auto
            .sets
            .get(index)
            .filter(|action| !action.off)
            .map(AutoAction::playing)
        else {
            return;
        };
        self.play_steps(&steps, cx);
    }

    /// Runs `steps` in turn, recording none of them.
    pub(crate) fn play_steps(&mut self, steps: &[String], cx: &mut AppletContext<'_>) {
        let recording = self.auto.recording.take();
        for step in steps {
            self.run(step, cx);
        }
        self.auto.recording = recording;
    }

    /// Plays the action whose id is `id` (its key, `auto.run.<id>`).
    pub(crate) fn play_auto_id(&mut self, id: &str, cx: &mut AppletContext<'_>) {
        if let Some((_, steps)) = self.auto.steps_of(id) {
            self.play_steps(&steps, cx);
        }
    }

    /// Keeps `command` in the auto action being recorded.
    pub(crate) fn record_auto(&mut self, command: &str) {
        if let Some(index) = self.auto.recording
            && crate::commands::registry()
                .flags(command)
                .has(sue_wm::Flags::RECORDABLE)
            && let Some(set) = self.auto.sets.get_mut(index)
        {
            set.steps.push(command.to_owned());
        }
    }

    /// Record, or Stop while recording: recording goes into the chosen
    /// action, or a new one when there is none.
    pub(crate) fn auto_record(&mut self, cx: &mut AppletContext<'_>) {
        if self.auto.recording.take().is_some() {
            self.save_auto(cx);
            return;
        }
        if self.auto.sets.get(self.auto.selected).is_none() {
            self.auto_new(cx);
        }
        self.auto.recording = Some(self.auto.selected);
        if let Some(action) = self.auto.sets.get(self.auto.selected) {
            let id = action.id.clone();
            if !self.auto.open.contains(&id) {
                self.auto.open.push(id);
            }
        }
    }

    /// A new action, chosen and recording.
    pub(crate) fn auto_new(&mut self, cx: &mut AppletContext<'_>) {
        let name = tr!("auto-action", value = self.auto.sets.len() + 1);
        let id = self.auto.fresh_id(&name);
        self.auto.sets.push(AutoAction {
            name,
            id: id.clone(),
            ..AutoAction::default()
        });
        self.auto.selected = self.auto.sets.len() - 1;
        self.auto.step = None;
        self.auto.recording = Some(self.auto.selected);
        self.auto.open.push(id);
        self.save_auto(cx);
    }

    /// Deletes the chosen step, or the chosen action when no step is.
    pub(crate) fn auto_delete(&mut self, cx: &mut AppletContext<'_>) {
        let selected = self.auto.selected;
        match self.auto.step.take() {
            Some(step) => {
                if let Some(action) = self.auto.sets.get_mut(selected) {
                    action.remove_step(step);
                }
            }
            None if selected < self.auto.sets.len() => {
                self.auto.sets.remove(selected);
                self.auto.selected = selected.saturating_sub(1);
                self.auto.recording = None;
            }
            None => return,
        }
        self.save_auto(cx);
    }

    /// A copy of the chosen action under it.
    pub(crate) fn auto_duplicate(&mut self, cx: &mut AppletContext<'_>) {
        let Some(mut copy) = self.auto.sets.get(self.auto.selected).cloned() else {
            return;
        };
        copy.name = tr!("auto-copy-of", name = copy.name);
        copy.id = self.auto.fresh_id(&copy.name);
        self.auto.sets.insert(self.auto.selected + 1, copy);
        self.auto.selected += 1;
        self.auto.step = None;
        self.save_auto(cx);
    }

    /// A new, empty set, in use.
    pub(crate) fn auto_new_set(&mut self, cx: &mut AppletContext<'_>) {
        let mut sets = self.auto.all_sets();
        let name = tr!("auto-set-n", value = sets.len() + 1);
        sets.push(ActionSet {
            name,
            actions: Vec::new(),
        });
        let last = sets.len() - 1;
        self.auto.take_sets(sets, last);
        self.save_auto(cx);
    }

    /// Deletes the set in use, when it is not the only one.
    pub(crate) fn auto_delete_set(&mut self, cx: &mut AppletContext<'_>) {
        let mut sets = self.auto.all_sets();
        if sets.len() < 2 {
            return;
        }
        let at = self.auto.in_use.min(sets.len() - 1);
        sets.remove(at);
        self.auto.take_sets(sets, at.saturating_sub(1));
        self.save_auto(cx);
    }

    /// Puts Sue Paint's own actions back as a set of their own, in use.
    pub(crate) fn auto_restore(&mut self, cx: &mut AppletContext<'_>) {
        let mut sets = self.auto.all_sets();
        let name = tr!("auto-set-sue-paint");
        sets.retain(|set| set.name != name);
        sets.insert(
            0,
            ActionSet {
                name,
                actions: AutoActions::builtin(),
            },
        );
        self.auto.take_sets(AutoActions::unique(sets), 0);
        self.save_auto(cx);
    }

    /// Asks where to read a shared set from, or where to write the set in
    /// use.
    pub(crate) fn auto_file(&mut self, export: bool, cx: &mut AppletContext<'_>) {
        let me = AppletId::new(crate::studio::ID).ok();
        let pick = if export {
            FilePick::save(
                me,
                "auto-export",
                &tr!("auto-export-set"),
                &format!("{}.{EXTENSION}", self.auto.set_name),
            )
        } else {
            FilePick::open(me, "auto-import", &tr!("auto-import-set"))
        };
        cx.request(ShellRequest::PickFile(pick.extensions(&[EXTENSION, "txt"])));
    }

    /// Reads a shared set (`auto-import`), adding its sets after the
    /// others and using the first; or writes the set in use
    /// (`auto-export`). `true` when `purpose` was one of these.
    pub(crate) fn auto_picked(
        &mut self,
        purpose: &str,
        path: &std::path::Path,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        match purpose {
            "auto-export" => {
                let set = ActionSet {
                    name: self.auto.set_name.clone(),
                    actions: self.auto.sets.clone(),
                };
                let text = AutoActions::write_sets(std::slice::from_ref(&set), 0);
                match sue_platform::fs::write(path, text) {
                    Ok(()) => cx.notify(tr!("auto-set-exported", name = set.name)),
                    Err(error) => cx.fail(tr!("auto-cannot-write", error = error.to_string())),
                }
                true
            }
            "auto-import" => {
                match sue_platform::fs::read_to_string(path) {
                    Ok(text) if text.lines().next().map(str::trim) == Some(HEADER) => {
                        let (read, _) = AutoActions::read_sets(&text);
                        let mut sets = self.auto.all_sets();
                        let first = sets.len();
                        let names: Vec<String> = read.iter().map(|set| set.name.clone()).collect();
                        sets.extend(read);
                        if names.is_empty() {
                            cx.fail(tr!("auto-not-a-set-file"));
                        } else {
                            self.auto.take_sets(AutoActions::unique(sets), first);
                            self.save_auto(cx);
                            cx.notify(tr!("auto-set-imported", name = names.join(", ")));
                        }
                    }
                    Ok(_) => cx.fail(tr!("auto-not-a-set-file")),
                    Err(error) => cx.fail(tr!("auto-cannot-read", error = error.to_string())),
                }
                true
            }
            _ => false,
        }
    }

    /// The keys people gave Auto Actions (`auto.run.<id>` in the
    /// studio's keys without a default): the actions whose key was
    /// pressed this frame, played.
    pub(crate) fn auto_keys(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        let sparse = sue_wm::shortcuts::SparseKeys::read(cx.rice(), crate::studio::KEYS_SECTION);
        let ids: Vec<String> = self
            .auto
            .all_sets()
            .into_iter()
            .flat_map(|set| set.actions)
            .map(|action| action.id)
            .collect();
        let pressed: Vec<String> = ctx.input_mut(|input| {
            ids.into_iter()
                .filter(|id| {
                    sparse
                        .get(&format!("{RUN_PREFIX}{id}"))
                        .is_some_and(|chords| sue_wm::keys::consume(input, chords))
                })
                .collect()
        });
        for id in pressed {
            self.play_auto_id(&id, cx);
        }
    }

    /// The Keyboard Shortcuts group of the Auto Actions, so each can be
    /// given a key in Settings.
    pub(crate) fn auto_key_group(&self) -> sue_wm::KeyGroup {
        sue_wm::KeyGroup {
            title: tr!("auto-actions"),
            commands: self
                .auto
                .all_sets()
                .into_iter()
                .flat_map(|set| {
                    let set_name = set.name;
                    set.actions
                        .into_iter()
                        .map(move |action| sue_wm::KeyCommand {
                            command: format!("{RUN_PREFIX}{}", action.id),
                            label: tr!(
                                "auto-key-label",
                                set = set_name.clone(),
                                name = action.name
                            ),
                            defaults: sue_rice::Keybinds::default(),
                        })
                })
                .collect(),
        }
    }

    /// The Auto Action palette.
    pub(crate) fn auto_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let density = Density::of(ui);
        let pad = theme.space(Space::S3);
        let gap = theme.space(Space::S2);
        let body = body.shrink(pad);
        let control = density.control_height();
        let button = density.icon_button();
        let mut inner = crate::panels::child(ui, body, "auto-action");
        let mut changed = false;
        // The set picker, or the set's name being typed.
        let picker = Rect::from_min_size(body.min, vec2(body.width(), control));
        if let Some(mut typed) = self.auto.renaming_set.take() {
            let name = tr!("auto-set-name");
            let response = sue_wm::ui::text_field("sue-paint-auto-set-name", &mut typed)
                .label(&name)
                .show_at(&mut inner, picker, theme);
            response.request_focus();
            if response.lost_focus() {
                let typed = typed.trim().to_owned();
                if !typed.is_empty() {
                    self.auto.set_name = typed;
                    changed = true;
                }
            } else {
                self.auto.renaming_set = Some(typed);
            }
        } else {
            let names = self.auto.set_names();
            let mut chosen = self.auto.in_use;
            let label = tr!("auto-set");
            let response = sue_wm::ui::dropdown("sue-paint-auto-set", &mut chosen, &names)
                .label(&label)
                .show_at(&mut inner, picker, theme);
            sue_wm::ui::tooltip(response, &Tip::new(&label), theme);
            if chosen != self.auto.in_use {
                self.auto.use_set(chosen);
                changed = true;
            }
        }
        // The actions, each with its steps when unfolded.
        let list = Rect::from_min_max(
            pos2(body.left(), picker.bottom() + gap),
            pos2(body.right(), body.bottom() - button - gap),
        );
        let mut list_ui = crate::panels::child(ui, list, "auto-action-list");
        let mut play = None;
        let recording = self.auto.recording;
        let recording_note = tr!("auto-recording");
        let off_note = tr!("auto-off");
        if self.auto.sets.is_empty() {
            sue_wm::ui::empty_state("sue-paint-auto-empty", &tr!("auto-empty"))
                .show(&mut list_ui, theme);
        }
        let menus = self.menus.clone();
        sue_wm::ui::ScrollView::vertical("sue-paint-auto")
            .on(theme.panel)
            .show(&mut list_ui, theme, |ui| {
                let auto = &mut self.auto;
                for index in 0..auto.sets.len() {
                    let action = &auto.sets[index];
                    let open = auto.open.contains(&action.id);
                    let chosen = index == auto.selected && auto.step.is_none();
                    let mut action_on = !action.off;
                    let mut toggled = false;
                    let mut clicked = false;
                    let mut double = false;
                    let renaming = auto
                        .renaming
                        .as_ref()
                        .filter(|(at, _)| *at == index)
                        .is_some();
                    ui.horizontal(|ui| {
                        let check = sue_wm::ui::checkbox(
                            ("sue-paint-auto-on", &action.id),
                            &action.name,
                            action_on,
                        )
                        .bare()
                        .show(ui, theme);
                        let check = sue_wm::ui::tooltip(
                            check,
                            &Tip::new(&tr!("auto-turn-on-off", name = action.name.clone())),
                            theme,
                        );
                        if check.clicked() {
                            action_on = !action_on;
                        }
                        if renaming {
                            if let Some((_, typed)) = auto.renaming.as_mut() {
                                let response =
                                    sue_wm::ui::text_field("sue-paint-auto-rename", typed)
                                        .show(ui, theme);
                                response.request_focus();
                                if response.lost_focus() {
                                    clicked = true;
                                }
                            }
                            return;
                        }
                        let note = if recording == Some(index) {
                            Some(recording_note.as_str())
                        } else if action.off {
                            Some(off_note.as_str())
                        } else {
                            None
                        };
                        let output = TreeRow::new(("sue-paint-auto-row", &action.id), &action.name)
                            .expanded(open)
                            .selected(chosen)
                            .row(|row| match note {
                                Some(note) => row.note(note),
                                None => row,
                            })
                            .show(ui, theme);
                        toggled = output.toggled;
                        clicked = output.response.clicked();
                        double = output.response.double_clicked();
                    });
                    if renaming && clicked {
                        if let Some((at, typed)) = auto.renaming.take() {
                            let typed = typed.trim().to_owned();
                            if let Some(action) = auto.sets.get_mut(at)
                                && !typed.is_empty()
                            {
                                action.name = typed;
                                changed = true;
                            }
                        }
                        continue;
                    }
                    let id = auto.sets[index].id.clone();
                    if action_on == auto.sets[index].off {
                        auto.sets[index].off = !action_on;
                        changed = true;
                    }
                    if toggled {
                        if open {
                            auto.open.retain(|known| *known != id);
                        } else {
                            auto.open.push(id.clone());
                        }
                    }
                    if clicked {
                        auto.selected = index;
                        auto.step = None;
                    }
                    if double {
                        play = Some(index);
                    }
                    if open == toggled {
                        continue;
                    }
                    let steps = auto.sets[index].steps.clone();
                    for (step, command) in steps.iter().enumerate() {
                        let label = crate::menus::label_of(&menus, command)
                            .map(str::to_owned)
                            .or_else(|| crate::commands::registry().label(command))
                            .unwrap_or_else(|| command.clone());
                        let on = auto.sets[index].step_on(step);
                        ui.horizontal(|ui| {
                            let check = sue_wm::ui::checkbox(
                                ("sue-paint-auto-step-on", &id, step),
                                &label,
                                on,
                            )
                            .bare()
                            .show(ui, theme);
                            let check = sue_wm::ui::tooltip(
                                check,
                                &Tip::new(&tr!("auto-turn-step-on-off", name = label.clone())),
                                theme,
                            );
                            if check.clicked() {
                                auto.sets[index].set_step(step, !on);
                                changed = true;
                            }
                            let chosen = auto.selected == index && auto.step == Some(step);
                            let row = TreeRow::new(("sue-paint-auto-step", &id, step), &label)
                                .depth(1)
                                .selected(chosen)
                                .row(|row| if on { row } else { row.note(&off_note) })
                                .show(ui, theme);
                            if row.response.clicked() {
                                auto.selected = index;
                                auto.step = Some(step);
                            }
                        });
                    }
                }
            });
        // The four buttons: Record (Stop while recording), Play, New,
        // Delete.
        let row = Rect::from_min_size(
            pos2(body.left(), body.bottom() - button),
            vec2(body.width(), button),
        );
        let buttons = crate::panels::child(ui, row, "auto-action-buttons");
        let mut x = row.left();
        let place = |x: f32| Rect::from_min_size(pos2(x, row.top()), vec2(button, button));
        {
            let look = self.command_look("auto.record", Icon::RadioOn, cx);
            let (icon, name) = if self.auto.recording.is_some() {
                (Icon::Stop, tr!("stop"))
            } else {
                (Icon::RadioOn, look.name.clone())
            };
            let response = sue_wm::ui::icon_button("sue-paint-auto-record", icon, &name)
                .selected(self.auto.recording.is_some())
                .enabled(look.enabled)
                .shortcut(&look.keys)
                .show_at(&buttons, place(x), theme);
            if response.clicked() {
                self.run("auto.record", cx);
            }
            x += button + gap;
        }
        for (command, icon) in [
            ("auto.play", Icon::Play),
            ("auto.new", Icon::Plus),
            ("auto.delete", Icon::Delete),
        ] {
            self.command_button(&buttons, place(x), command, icon, cx, theme);
            x += button + gap;
        }
        if let Some(index) = play {
            self.play_auto(index, cx);
        }
        if changed {
            self.save_auto(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_round_trip_with_their_ids_and_switches() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let mut off = AutoAction::new("Ink Prep", &["layer.new", "view.fit", "select.none"]);
        off.set_step(1, false);
        off.off = true;
        let sets = vec![
            ActionSet {
                name: "Sue Paint Actions".to_owned(),
                actions: AutoActions::builtin(),
            },
            ActionSet {
                name: "Mine".to_owned(),
                actions: vec![off.clone()],
            },
        ];
        let text = AutoActions::write_sets(&sets, 1);
        assert!(text.starts_with(HEADER));
        assert!(text.contains("step off view.fit"));
        let (back, in_use) = AutoActions::read_sets(&text);
        assert_eq!(back, sets);
        assert_eq!(in_use, 1);
        assert_eq!(back[1].actions[0].playing(), ["layer.new", "select.none"]);
        // Taken in, the set in use is Mine; every set comes back in order.
        let mut auto = AutoActions::default();
        auto.load(Some(&text));
        assert_eq!(auto.set_name, "Mine");
        assert_eq!(auto.sets, [off]);
        assert_eq!(auto.all_sets(), sets);
        assert_eq!(AutoActions::read_sets(&auto.to_text()), (sets, 1));
    }

    #[test]
    fn a_file_of_0_0_75_reads_as_one_set() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let text =
            "[Flip Canvas and Fit]\ncanvas.flip_h\nview.fit\n[Fit]\nview.fit\n[Fit]\nview.fit\n";
        let (sets, in_use) = AutoActions::read_sets(text);
        assert_eq!((sets.len(), in_use), (1, 0));
        let ids: Vec<&str> = sets[0].actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["flip-canvas-and-fit", "fit", "fit-2"]);
        assert_eq!(sets[0].actions[0].steps, ["canvas.flip_h", "view.fit"]);
    }

    #[test]
    fn the_nine_actions_of_sue_paint_play_recordable_commands() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let actions = AutoActions::builtin();
        assert_eq!(actions.len(), 9);
        let registry = crate::commands::registry();
        for action in &actions {
            assert!(!action.steps.is_empty(), "{}", action.name);
            for step in &action.steps {
                assert!(registry.has(step), "{step} of {}", action.name);
                assert!(
                    registry.flags(step).has(sue_wm::Flags::RECORDABLE),
                    "{step} of {} is recordable",
                    action.name
                );
            }
            assert!(
                sue_wm::shortcuts::SparseKeys::holds(&format!("{RUN_PREFIX}{}", action.id)),
                "{} can be given a key",
                action.id
            );
        }
    }

    #[test]
    fn steps_turned_off_are_skipped_and_ids_stay_apart() {
        let mut action = AutoAction::new("A", &["a.one", "a.two", "a.three"]);
        action.set_step(2, false);
        action.set_step(2, true);
        assert!(action.steps_off.is_empty(), "all on again: no marks left");
        action.set_step(0, false);
        action.remove_step(0);
        assert_eq!(action.playing(), ["a.two", "a.three"]);
        assert_eq!(slug("Flip Canvas & Fit!"), "flip-canvas-fit");
        assert_eq!(slug("線画"), "action");
        let mut auto = AutoActions::default();
        auto.take_sets(Vec::new(), 0);
        let id = auto.fresh_id(&tr!("flip-canvas-and-fit"));
        assert_ne!(id, auto.sets[0].id);
    }
}
