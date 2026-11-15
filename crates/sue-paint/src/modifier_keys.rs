//! Modifier keys as data: what a key held while pressing on the canvas,
//! the mouse wheel with each key, and the right and middle buttons do,
//! for every tool alike (General) and for one tool on its own, as Clip
//! Studio Paint's Modifier Key Settings has them.
//!
//! A row's action is one of [`Action`]: the tool's own use of the key
//! (Shift draws a straight line, Alt subtracts from a selection), a tool
//! borrowed while the key is held (Alt for the Eyedropper, Ctrl for the
//! Object tool, Space for the Hand), the brush size drag, or what the
//! wheel does to the view. A tool's row may say "as in General".
//!
//! Only the rows a person changed are kept, in the file
//! `modifier_keys.txt` of Sue Paint's data folder, one row a line; the
//! format is described in `docs/paint-modifier-keys.md`, so a file can
//! be shared and installed by hand. Three rows were settings of their
//! own before (`sue_paint.alt_tool`, `sue_paint.space_tool` and
//! `sue_paint.ctrl_shift`); they still decide those rows of General
//! until the file says otherwise, so an old setting keeps working.

use std::collections::BTreeMap;

use crate::tools::Tool;

/// The file the rows a person changed are kept in.
pub(crate) const FILE: &str = "modifier_keys.txt";

/// What the modifier keys go with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Input {
    /// A press (and its drag) on the canvas with the pen or the left
    /// button.
    Press,
    /// The mouse wheel.
    Wheel,
    /// A right-click.
    Right,
    /// A middle-click.
    Middle,
}

impl Input {
    /// Every input, in the dialog's order.
    pub(crate) const ALL: [Self; 4] = [Self::Press, Self::Wheel, Self::Right, Self::Middle];

    /// Its stable name in the file.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Wheel => "wheel",
            Self::Right => "right",
            Self::Middle => "middle",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|input| input.id() == id)
    }

    /// The heading of its rows in the dialog.
    pub(crate) fn label(self) -> String {
        match self {
            Self::Press => tr!("modifiers-press"),
            Self::Wheel => tr!("modifiers-wheel"),
            Self::Right => tr!("modifiers-right-click"),
            Self::Middle => tr!("modifiers-middle-click"),
        }
    }

    /// The key combinations it has rows for: a press needs at least one
    /// key (a plain press is the tool's), the wheel has one row more for
    /// no key, a click one row only.
    pub(crate) fn rows(self) -> Vec<Keys> {
        match self {
            Self::Press => Keys::COMBINATIONS.to_vec(),
            Self::Wheel => std::iter::once(Keys::NONE)
                .chain(Keys::COMBINATIONS)
                .collect(),
            Self::Right | Self::Middle => vec![Keys::NONE],
        }
    }

    /// Whether `action` can be given to its rows.
    pub(crate) fn allows(self, action: &Action) -> bool {
        match action {
            Action::Common => true,
            Action::Own => self == Self::Press,
            Action::Borrow { .. } => self != Self::Wheel,
            Action::BrushSize => matches!(self, Self::Press | Self::Wheel),
            Action::Zoom | Action::Rotate | Action::Scroll => self == Self::Wheel,
            Action::Nothing => self != Self::Press,
        }
    }
}

/// The keys held: Ctrl (Command on a Mac), Shift, Alt (Option) and Space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[allow(clippy::struct_excessive_bools, reason = "four keys, each held or not")]
pub(crate) struct Keys {
    pub(crate) ctrl: bool,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    pub(crate) space: bool,
}

impl Keys {
    /// No key held.
    pub(crate) const NONE: Self = Self::new(false, false, false, false);

    const fn new(ctrl: bool, shift: bool, alt: bool, space: bool) -> Self {
        Self {
            ctrl,
            shift,
            alt,
            space,
        }
    }

    /// Every combination of at least one key, the ones a person reaches
    /// for most first, as Clip Studio Paint lists them.
    pub(crate) const COMBINATIONS: [Self; 15] = [
        Self::new(true, false, false, false),
        Self::new(false, false, true, false),
        Self::new(true, false, true, false),
        Self::new(false, true, false, false),
        Self::new(true, true, false, false),
        Self::new(false, true, true, false),
        Self::new(true, true, true, false),
        Self::new(false, false, false, true),
        Self::new(true, false, false, true),
        Self::new(false, false, true, true),
        Self::new(true, false, true, true),
        Self::new(false, true, false, true),
        Self::new(true, true, false, true),
        Self::new(false, true, true, true),
        Self::new(true, true, true, true),
    ];

    /// The keys `modifiers` holds, with Space as `space` says.
    pub(crate) fn of(modifiers: egui::Modifiers, space: bool) -> Self {
        Self::new(
            modifiers.command || modifiers.ctrl,
            modifiers.shift,
            modifiers.alt,
            space,
        )
    }

    /// The names of the keys held, in a fixed order.
    fn names(self) -> Vec<&'static str> {
        [
            (self.ctrl, "ctrl"),
            (self.shift, "shift"),
            (self.alt, "alt"),
            (self.space, "space"),
        ]
        .into_iter()
        .filter_map(|(held, name)| held.then_some(name))
        .collect()
    }

    /// Its stable name in the file: `ctrl+shift`, or `none`.
    pub(crate) fn id(self) -> String {
        let names = self.names();
        if names.is_empty() {
            "none".to_owned()
        } else {
            names.join("+")
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        let mut keys = Self::NONE;
        if id == "none" {
            return Some(keys);
        }
        for name in id.split('+') {
            let held = match name.trim() {
                "ctrl" => &mut keys.ctrl,
                "shift" => &mut keys.shift,
                "alt" => &mut keys.alt,
                "space" => &mut keys.space,
                _ => return None,
            };
            if *held {
                return None;
            }
            *held = true;
        }
        Some(keys)
    }

    /// What people read: `Ctrl+Shift`, or the input's own name for no key.
    pub(crate) fn label(self) -> String {
        let names: Vec<String> = self
            .names()
            .into_iter()
            .map(|name| match name {
                "ctrl" => tr!("modifiers-key-ctrl"),
                "shift" => tr!("modifiers-key-shift"),
                "alt" => tr!("modifiers-key-alt"),
                _ => tr!("modifiers-key-space"),
            })
            .collect();
        if names.is_empty() {
            tr!("modifiers-no-key")
        } else {
            names.join("+")
        }
    }
}

/// What a row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Action {
    /// A tool's row: as General says.
    Common,
    /// The tool's own use of the key: Shift draws a straight line, Alt
    /// takes away from a selection, Alt zooms the other way.
    Own,
    /// `tool` while the key is held, with its sub tool of `mode` (or the
    /// one picked on it): Change tool temporarily.
    Borrow { tool: Tool, mode: Option<u8> },
    /// A sideways drag (or the wheel) sizes the brush.
    BrushSize,
    /// The wheel zooms the view.
    Zoom,
    /// The wheel turns the view.
    Rotate,
    /// The wheel scrolls the view.
    Scroll,
    /// Nothing happens.
    Nothing,
}

impl Action {
    /// The kinds of action, as the dialog's first drop-down lists them;
    /// a borrowed tool stands for every [`Self::Borrow`].
    pub(crate) const KINDS: [Self; 8] = [
        Self::Common,
        Self::Own,
        Self::Borrow {
            tool: Tool::Eyedropper,
            mode: None,
        },
        Self::BrushSize,
        Self::Zoom,
        Self::Rotate,
        Self::Scroll,
        Self::Nothing,
    ];

    /// The same kind of action, whatever tool it borrows.
    pub(crate) fn same_kind(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }

    /// The name of its kind, as the dialog's first drop-down shows it.
    pub(crate) fn kind_label(self) -> String {
        match self {
            Self::Common => tr!("modifiers-common"),
            Self::Own => tr!("modifiers-own"),
            Self::Borrow { .. } => tr!("modifiers-borrow"),
            Self::BrushSize => tr!("modifiers-brush-size"),
            Self::Zoom => tr!("modifiers-zoom"),
            Self::Rotate => tr!("modifiers-rotate"),
            Self::Scroll => tr!("modifiers-scroll"),
            Self::Nothing => tr!("modifiers-nothing"),
        }
    }

    /// Its text in the file.
    fn id(self) -> String {
        match self {
            Self::Common => "common".to_owned(),
            Self::Own => "own".to_owned(),
            Self::Borrow { tool, mode: None } => format!("borrow {}", tool.id()),
            Self::Borrow {
                tool,
                mode: Some(mode),
            } => format!("borrow {} {mode}", tool.id()),
            Self::BrushSize => "brush_size".to_owned(),
            Self::Zoom => "zoom".to_owned(),
            Self::Rotate => "rotate".to_owned(),
            Self::Scroll => "scroll".to_owned(),
            Self::Nothing => "nothing".to_owned(),
        }
    }

    fn from_id(text: &str) -> Option<Self> {
        let mut words = text.split_whitespace();
        let action = match words.next()? {
            "common" => Self::Common,
            "own" => Self::Own,
            "borrow" => {
                let tool = Tool::from_id(words.next()?)?;
                let mode = match words.next() {
                    Some(mode) => Some(mode.parse().ok()?),
                    None => None,
                };
                Self::Borrow { tool, mode }
            }
            "brush_size" => Self::BrushSize,
            "zoom" => Self::Zoom,
            "rotate" => Self::Rotate,
            "scroll" => Self::Scroll,
            "nothing" => Self::Nothing,
            _ => return None,
        };
        words.next().is_none().then_some(action)
    }
}

/// The sub tool a borrowed `tool` of `mode` is, by name, for the dialog
/// and the tooltips: the built-in one of that mode.
pub(crate) fn borrowed_name(tool: Tool, mode: Option<u8>) -> String {
    mode.and_then(|mode| tool.sub_tools().into_iter().find(|sub| sub.mode == mode))
        .map_or_else(
            || tool.label(),
            |sub| format!("{}: {}", tool.label(), crate::localize(&sub.name)),
        )
}

/// The tools a held Alt turns into the Eyedropper unless a tool says
/// otherwise: the ones that lay color (Clip Studio Paint's, with Fill and
/// Gradient as its own captions show). The others have a use of their own
/// for Alt (a selection takes away, Zoom goes the other way) or none.
const ALT_PICKS: [Tool; 7] = [
    Tool::Pen,
    Tool::Pencil,
    Tool::Brush,
    Tool::Airbrush,
    Tool::Decoration,
    Tool::Fill,
    Tool::Gradient,
];

/// What the three settings that came before this file say (rule 4: an
/// old setting still counts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Legacy {
    /// `sue_paint.alt_tool`: the tool a held Alt borrows (`None`: Alt is
    /// the tool's own).
    pub(crate) alt: Option<Tool>,
    /// `sue_paint.space_tool` is `hand`.
    pub(crate) space: bool,
    /// `sue_paint.ctrl_shift` is `pick_layer`.
    pub(crate) ctrl_shift: bool,
}

impl Default for Legacy {
    fn default() -> Self {
        Self {
            alt: Some(Tool::Eyedropper),
            space: true,
            ctrl_shift: true,
        }
    }
}

impl Legacy {
    /// The three settings as the rice holds them.
    pub(crate) fn of(rice: &sue_rice::RiceSchema) -> Self {
        let text = |key: &str, default: &'static str| rice.get_str(key).unwrap_or(default);
        Self {
            alt: match text("sue_paint.alt_tool", "eyedropper") {
                "eraser" => Some(Tool::Eraser),
                "none" => None,
                _ => Some(Tool::Eyedropper),
            },
            space: text("sue_paint.space_tool", "hand") != "none",
            ctrl_shift: text("sue_paint.ctrl_shift", "pick_layer") != "none",
        }
    }

    /// The three settings' values in the rice, in the order of
    /// [`LEGACY_KEYS`].
    pub(crate) fn values(self) -> [&'static str; 3] {
        [
            match self.alt {
                Some(Tool::Eraser) => "eraser",
                Some(_) => "eyedropper",
                None => "none",
            },
            if self.space { "hand" } else { "none" },
            if self.ctrl_shift {
                "pick_layer"
            } else {
                "none"
            },
        ]
    }
}

/// The rice keys of [`Legacy`].
pub(crate) const LEGACY_KEYS: [&str; 3] = [
    "sue_paint.alt_tool",
    "sue_paint.space_tool",
    "sue_paint.ctrl_shift",
];

/// A row: General's (`None`) or a tool's, its input and its keys.
pub(crate) type Row = (Option<Tool>, Input, Keys);

/// The modifier keys: the rows a person changed, over Sue's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ModifierKeys {
    rows: BTreeMap<Row, Action>,
    legacy: Legacy,
}

impl ModifierKeys {
    /// Sue's own rows, with the three old settings as `legacy` says.
    pub(crate) fn new(legacy: Legacy) -> Self {
        Self {
            rows: BTreeMap::new(),
            legacy,
        }
    }

    /// What the three old settings say.
    pub(crate) fn legacy(&self) -> Legacy {
        self.legacy
    }

    /// Follows the three old settings as they are now (Settings may have
    /// changed them).
    pub(crate) fn set_legacy(&mut self, legacy: Legacy) {
        self.legacy = legacy;
    }

    /// What `tool` does with `keys` held on `input`: its own row, else
    /// General's.
    pub(crate) fn action(&self, tool: Tool, input: Input, keys: Keys) -> Action {
        match self.row(Some(tool), input, keys) {
            Action::Common => self.row(None, input, keys),
            action => action,
        }
    }

    /// What a row says now: as a person set it, else Sue's own.
    pub(crate) fn row(&self, scope: Option<Tool>, input: Input, keys: Keys) -> Action {
        self.rows
            .get(&(scope, input, keys))
            .copied()
            .unwrap_or_else(|| self.builtin(scope, input, keys))
    }

    /// Sets a row; a row set back to Sue's own is no longer kept.
    pub(crate) fn set(&mut self, scope: Option<Tool>, input: Input, keys: Keys, action: Action) {
        let row = (scope, input, keys);
        // General has no "as in General".
        if (scope.is_none() && action == Action::Common) || !input.allows(&action) {
            return;
        }
        if action == self.builtin(scope, input, keys) {
            self.rows.remove(&row);
        } else {
            self.rows.insert(row, action);
        }
    }

    /// Every row of `scope` back to Sue's own (for General, also the
    /// three old settings).
    pub(crate) fn reset(&mut self, scope: Option<Tool>) {
        self.rows.retain(|(at, _, _), _| *at != scope);
        if scope.is_none() {
            self.legacy = Legacy::default();
        }
    }

    /// `true` when a row of `scope` differs from Sue's own.
    pub(crate) fn changed(&self, scope: Option<Tool>) -> bool {
        self.rows.keys().any(|(at, _, _)| *at == scope)
    }

    /// Moves General's rows that the three old settings can say into
    /// them, so Settings shows what the canvas does; the rest stay rows.
    pub(crate) fn fold_legacy(&mut self) {
        let alt = Keys {
            alt: true,
            ..Keys::NONE
        };
        let space = Keys {
            space: true,
            ..Keys::NONE
        };
        let ctrl_shift = Keys {
            ctrl: true,
            shift: true,
            ..Keys::NONE
        };
        let take = |rows: &mut BTreeMap<Row, Action>, keys: Keys| {
            rows.get(&(None, Input::Press, keys))
                .copied()
                .map(|action| (keys, action))
        };
        if let Some((keys, action)) = take(&mut self.rows, alt) {
            let said = match action {
                Action::Borrow {
                    tool: tool @ (Tool::Eyedropper | Tool::Eraser),
                    mode: None,
                } => Some(Some(tool)),
                Action::Own => Some(None),
                _ => None,
            };
            if let Some(tool) = said {
                self.legacy.alt = tool;
                self.rows.remove(&(None, Input::Press, keys));
            }
        }
        for (keys, hand_like, field) in [
            (
                space,
                Action::Borrow {
                    tool: Tool::Hand,
                    mode: Some(0),
                },
                0,
            ),
            (
                ctrl_shift,
                Action::Borrow {
                    tool: Tool::Operation,
                    mode: Some(1),
                },
                1,
            ),
        ] {
            let Some((_, action)) = take(&mut self.rows, keys) else {
                continue;
            };
            let on = if action == hand_like {
                true
            } else if action == Action::Own {
                false
            } else {
                continue;
            };
            if field == 0 {
                self.legacy.space = on;
            } else {
                self.legacy.ctrl_shift = on;
            }
            self.rows.remove(&(None, Input::Press, keys));
        }
    }

    /// Sue's own row: a tool's says "as in General" but where the tool
    /// has a use of its own for the key.
    pub(crate) fn builtin(&self, scope: Option<Tool>, input: Input, keys: Keys) -> Action {
        let Some(tool) = scope else {
            return self.general(input, keys);
        };
        if input != Input::Press {
            return Action::Common;
        }
        // Alt (or Shift and Alt) without Ctrl or Space: the Eyedropper
        // for the tools that lay color; a selection takes away (or
        // intersects), Zoom goes the other way, Blend's Copy stamp sets
        // its source.
        if keys.alt && !keys.ctrl && !keys.space && !ALT_PICKS.contains(&tool) {
            return Action::Own;
        }
        // Ctrl alone on the Operation tool is its own (a 3D layer's
        // camera moves sideways).
        if keys
            == (Keys {
                ctrl: true,
                ..Keys::NONE
            })
            && tool == Tool::Operation
        {
            return Action::Own;
        }
        Action::Common
    }

    /// General's own rows, Clip Studio Paint's.
    fn general(&self, input: Input, keys: Keys) -> Action {
        let borrow = |tool, mode| Action::Borrow { tool, mode };
        match input {
            Input::Press if keys.space => {
                if keys.shift {
                    borrow(Tool::Hand, Some(1))
                } else if keys.alt {
                    borrow(Tool::Zoom, Some(1))
                } else if keys.ctrl {
                    borrow(Tool::Zoom, Some(0))
                } else if self.legacy.space {
                    borrow(Tool::Hand, Some(0))
                } else {
                    Action::Own
                }
            }
            Input::Press if keys.ctrl && keys.shift => {
                if self.legacy.ctrl_shift {
                    borrow(Tool::Operation, Some(1))
                } else {
                    Action::Own
                }
            }
            Input::Press if keys.ctrl && keys.alt => Action::BrushSize,
            Input::Press if keys.alt => self
                .legacy
                .alt
                .map_or(Action::Own, |tool| borrow(tool, None)),
            Input::Press if keys.ctrl => borrow(Tool::Operation, Some(0)),
            Input::Press => Action::Own,
            Input::Wheel if keys.shift => Action::Rotate,
            Input::Wheel => Action::Zoom,
            Input::Right => borrow(Tool::Eyedropper, Some(0)),
            Input::Middle => borrow(Tool::Hand, Some(0)),
        }
    }

    /// The rows a person changed, as the file keeps them.
    pub(crate) fn to_text(&self) -> String {
        let mut text = String::from("# Sue Paint modifier keys (docs/paint-modifier-keys.md)\n");
        for ((scope, input, keys), action) in &self.rows {
            text.push_str(&format!(
                "{} {} {} = {}\n",
                scope.map_or("general", Tool::id),
                input.id(),
                keys.id(),
                action.id()
            ));
        }
        text
    }

    /// The rows of a file over Sue's own (with `legacy` for the three old
    /// settings), and how many lines could not be read.
    pub(crate) fn parse(text: &str, legacy: Legacy) -> (Self, usize) {
        let mut keys = Self::new(legacy);
        let mut unread = 0;
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match parse_row(line) {
                Some(((scope, input, held), action))
                    if input.allows(&action) && !(scope.is_none() && action == Action::Common) =>
                {
                    keys.set(scope, input, held, action);
                }
                _ => unread += 1,
            }
        }
        (keys, unread)
    }
}

/// One line of the file: `<general or tool> <input> <keys> = <action>`.
fn parse_row(line: &str) -> Option<(Row, Action)> {
    let (row, action) = line.split_once('=')?;
    let mut words = row.split_whitespace();
    let scope = match words.next()? {
        "general" => None,
        tool => Some(Tool::from_id(tool)?),
    };
    let input = Input::from_id(words.next()?)?;
    let keys = Keys::from_id(words.next()?)?;
    if words.next().is_some() {
        return None;
    }
    // A click has no keys.
    if matches!(input, Input::Right | Input::Middle) && keys != Keys::NONE {
        return None;
    }
    if input == Input::Press && keys == Keys::NONE {
        return None;
    }
    Some(((scope, input, keys), Action::from_id(action.trim())?))
}

/// A tool picked by holding its key: the tool and sub tool to go back to
/// when the key comes up (Clip Studio Paint's temporary tool on a held
/// tool key).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HeldTool {
    /// The command that picked it.
    pub(crate) command: String,
    /// The keys of that command down when it was picked (none yet: read
    /// on the next frame).
    pub(crate) keys: Vec<egui::Key>,
    /// The tool palette entry and the sub tool picked on it before.
    pub(crate) before: (usize, usize),
    /// When it was picked, in the input's seconds.
    pub(crate) since: f64,
    /// It was used on the canvas while the key was down.
    pub(crate) used: bool,
}

/// How long a tool key is held before letting go of it goes back to the
/// tool before even when it was not used, in seconds: a tap picks the
/// tool for good.
pub(crate) const HOLD: f64 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    const fn keys(ctrl: bool, shift: bool, alt: bool, space: bool) -> Keys {
        Keys::new(ctrl, shift, alt, space)
    }

    #[test]
    fn sue_s_own_rows_are_clip_studio_paint_s() {
        let map = ModifierKeys::default();
        let borrow = |tool, mode| Action::Borrow { tool, mode };
        let press = |tool, held| map.action(tool, Input::Press, held);
        // Ctrl alone borrows the Object tool; Alt the Eyedropper for the
        // tools that lay color, Fill and Gradient too.
        assert_eq!(
            press(Tool::Pen, keys(true, false, false, false)),
            borrow(Tool::Operation, Some(0))
        );
        for tool in [Tool::Pen, Tool::Brush, Tool::Fill, Tool::Gradient] {
            assert_eq!(
                press(tool, keys(false, false, true, false)),
                borrow(Tool::Eyedropper, None),
                "{tool:?}"
            );
        }
        // A selection keeps Alt to take away; the Eraser and Figure keep it.
        for tool in [Tool::Select, Tool::AutoSelect, Tool::Eraser, Tool::Figure] {
            assert_eq!(press(tool, keys(false, false, true, false)), Action::Own);
        }
        assert_eq!(
            press(Tool::Pen, keys(false, false, false, true)),
            borrow(Tool::Hand, Some(0))
        );
        assert_eq!(
            press(Tool::Pen, keys(false, true, false, true)),
            borrow(Tool::Hand, Some(1))
        );
        assert_eq!(
            press(Tool::Pen, keys(true, false, false, true)),
            borrow(Tool::Zoom, Some(0))
        );
        assert_eq!(
            press(Tool::Pen, keys(false, false, true, true)),
            borrow(Tool::Zoom, Some(1))
        );
        assert_eq!(
            press(Tool::Pen, keys(true, true, false, false)),
            borrow(Tool::Operation, Some(1))
        );
        assert_eq!(
            press(Tool::Blend, keys(true, false, true, false)),
            Action::BrushSize
        );
        assert_eq!(
            press(Tool::Pen, keys(false, true, false, false)),
            Action::Own
        );
        assert_eq!(
            press(Tool::Operation, keys(true, false, false, false)),
            Action::Own
        );
        assert_eq!(
            map.action(Tool::Pen, Input::Wheel, Keys::NONE),
            Action::Zoom
        );
        assert_eq!(
            map.action(Tool::Pen, Input::Wheel, keys(false, true, false, false)),
            Action::Rotate
        );
        assert_eq!(
            map.action(Tool::Pen, Input::Right, Keys::NONE),
            borrow(Tool::Eyedropper, Some(0))
        );
        assert_eq!(
            map.action(Tool::Pen, Input::Middle, Keys::NONE),
            borrow(Tool::Hand, Some(0))
        );
    }

    #[test]
    fn a_tool_s_row_wins_over_general_and_common_falls_through() {
        let mut map = ModifierKeys::default();
        let ctrl = keys(true, false, false, false);
        let rotate = Action::Borrow {
            tool: Tool::Hand,
            mode: Some(1),
        };
        // Ctrl alone turns the view, for every tool (the tutorial's step).
        map.set(None, Input::Press, ctrl, rotate);
        assert_eq!(map.action(Tool::Brush, Input::Press, ctrl), rotate);
        // The Pen keeps Ctrl for the Object tool.
        let object = Action::Borrow {
            tool: Tool::Operation,
            mode: Some(0),
        };
        map.set(Some(Tool::Pen), Input::Press, ctrl, object);
        assert_eq!(map.action(Tool::Pen, Input::Press, ctrl), object);
        assert_eq!(map.action(Tool::Brush, Input::Press, ctrl), rotate);
        // Back to "as in General".
        map.set(Some(Tool::Pen), Input::Press, ctrl, Action::Common);
        assert_eq!(map.action(Tool::Pen, Input::Press, ctrl), rotate);
        assert!(!map.changed(Some(Tool::Pen)));
        // A row that cannot be given is refused.
        map.set(None, Input::Wheel, Keys::NONE, Action::Own);
        map.set(None, Input::Press, ctrl, Action::Common);
        assert_eq!(map.action(Tool::Brush, Input::Press, ctrl), rotate);
        map.reset(None);
        assert_eq!(map, ModifierKeys::default());
    }

    #[test]
    fn the_file_keeps_only_changed_rows_and_reads_back_the_same() {
        let mut map = ModifierKeys::default();
        assert_eq!(
            map.to_text(),
            "# Sue Paint modifier keys (docs/paint-modifier-keys.md)\n"
        );
        map.set(
            None,
            Input::Press,
            keys(true, false, false, false),
            Action::Borrow {
                tool: Tool::Hand,
                mode: Some(1),
            },
        );
        map.set(
            Some(Tool::Fill),
            Input::Press,
            keys(false, false, true, false),
            Action::Own,
        );
        map.set(None, Input::Wheel, Keys::NONE, Action::Scroll);
        map.set(None, Input::Right, Keys::NONE, Action::Nothing);
        let text = map.to_text();
        assert!(
            text.contains("general press ctrl = borrow hand 1\n"),
            "{text}"
        );
        assert!(text.contains("fill press alt = own\n"), "{text}");
        assert!(text.contains("general wheel none = scroll\n"), "{text}");
        assert!(text.contains("general right none = nothing\n"), "{text}");
        let (back, unread) = ModifierKeys::parse(&text, Legacy::default());
        assert_eq!(unread, 0);
        assert_eq!(back, map);
    }

    #[test]
    fn lines_that_cannot_be_read_are_counted_and_left_out() {
        let text = "general press ctrl = borrow nothing_such\n\
                    general press none = own\n\
                    general wheel ctrl = own\n\
                    general right ctrl = nothing\n\
                    general press ctrl+ctrl = own\n\
                    pen press alt = common\n\
                    general press alt = common\n\
                    nonsense\n\
                    # a comment\n\
                    \n\
                    pen press shift = borrow eraser\n";
        let (map, unread) = ModifierKeys::parse(text, Legacy::default());
        assert_eq!(unread, 7, "{map:?}");
        assert_eq!(
            map.action(Tool::Pen, Input::Press, keys(false, true, false, false)),
            Action::Borrow {
                tool: Tool::Eraser,
                mode: None
            }
        );
    }

    #[test]
    fn the_three_old_settings_still_decide_their_rows() {
        let legacy = Legacy {
            alt: Some(Tool::Eraser),
            space: false,
            ctrl_shift: false,
        };
        let map = ModifierKeys::new(legacy);
        assert_eq!(
            map.action(Tool::Pen, Input::Press, keys(false, false, true, false)),
            Action::Borrow {
                tool: Tool::Eraser,
                mode: None
            }
        );
        assert_eq!(
            map.action(Tool::Pen, Input::Press, keys(false, false, false, true)),
            Action::Own
        );
        assert_eq!(
            map.action(Tool::Pen, Input::Press, keys(true, true, false, false)),
            Action::Own
        );
        // The rows they can say go back into them.
        let mut map = ModifierKeys::default();
        map.set(
            None,
            Input::Press,
            keys(false, false, true, false),
            Action::Own,
        );
        map.set(
            None,
            Input::Press,
            keys(false, false, false, true),
            Action::Own,
        );
        map.set(
            None,
            Input::Press,
            keys(true, false, false, false),
            Action::BrushSize,
        );
        map.fold_legacy();
        assert_eq!(
            map.legacy().values(),
            ["none", "none", "pick_layer"],
            "{map:?}"
        );
        assert!(map.changed(None), "Ctrl's row stays a row");
        assert_eq!(
            map.to_text().lines().count(),
            2,
            "only Ctrl's row is in the file: {}",
            map.to_text()
        );
    }

    #[test]
    fn every_key_combination_has_one_stable_name() {
        let mut seen = std::collections::BTreeSet::new();
        for held in Input::Wheel.rows() {
            assert!(seen.insert(held.id()));
            assert_eq!(Keys::from_id(&held.id()), Some(held));
        }
        assert_eq!(seen.len(), 16);
    }
}
