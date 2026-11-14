//! Menu bars for studios and tools: menus described as data, drawn in the
//! shell's top bar after the Sue button, whose own menu is a
//! [`MenuBar::dropdown`].
//!
//! A studio lists its menus as [`Menu`]s of [`MenuItem`]s, each naming a
//! command. Every frame [`MenuBar::ui`] asks the studio for the state of
//! each command ([`ItemState`]): items whose command the studio cannot run
//! yet are drawn grayed out and do nothing, so a menu can show the whole
//! plan of a studio while only part of it works. An item may carry a note
//! that says why it is grayed ("Needs a selection"), shown when the pointer
//! or the keyboard rests on it.
//!
//! The menus themselves are drawn by [`crate::ui::menu`], the one menu
//! implementation: they open on click, follow the pointer across the bar
//! while one is open, open submenus on hover, answer the arrow keys,
//! Enter, Escape and typed letters, scroll when taller than the window,
//! and close on a pick, on Escape or on a click elsewhere. Left and Right
//! on a top level menu go to the menu beside it. Scripted runs open a menu
//! with [`MenuBar::open`].

use egui::{Id, Rect, Sense, Ui, pos2, vec2};

use crate::Theme;
use crate::ui::menu::{MenuEvent, MenuRequest, MenuState};
use crate::ui::{Density, Radius, TextRole};

/// One entry of a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem {
    /// Runs a command.
    Action {
        /// What people read.
        label: String,
        /// The command the studio runs.
        command: String,
        /// The shortcut shown at the right, e.g. `Ctrl+N`.
        shortcut: String,
        /// One line shown when the pointer or the keyboard rests on the
        /// item, mostly to say why it is grayed out ("Needs a
        /// selection"); empty for none.
        note: String,
    },
    /// A nested menu.
    Sub {
        /// What people read.
        label: String,
        /// The nested items.
        items: Vec<MenuItem>,
    },
    /// A line between groups.
    Separator,
}

impl MenuItem {
    /// An item that runs `command`.
    pub fn action(label: impl Into<String>, command: impl Into<String>) -> Self {
        Self::Action {
            label: label.into(),
            command: command.into(),
            shortcut: String::new(),
            note: String::new(),
        }
    }

    /// The same item showing `shortcut`.
    #[must_use]
    pub fn key(self, shortcut: impl Into<String>) -> Self {
        match self {
            Self::Action {
                label,
                command,
                note,
                ..
            } => Self::Action {
                label,
                command,
                shortcut: shortcut.into(),
                note,
            },
            other => other,
        }
    }

    /// The same item with a note, shown when the pointer or the keyboard
    /// rests on it: why it is grayed out, or what it will do.
    #[must_use]
    pub fn note(self, note: impl Into<String>) -> Self {
        match self {
            Self::Action {
                label,
                command,
                shortcut,
                ..
            } => Self::Action {
                label,
                command,
                shortcut,
                note: note.into(),
            },
            other => other,
        }
    }

    /// A nested menu.
    pub fn sub(label: impl Into<String>, items: Vec<Self>) -> Self {
        Self::Sub {
            label: label.into(),
            items,
        }
    }

    /// What people read (empty for a separator).
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Action { label, .. } | Self::Sub { label, .. } => label,
            Self::Separator => "",
        }
    }

    /// Every command in this item and below it.
    pub fn commands(&self) -> Vec<&str> {
        match self {
            Self::Action { command, .. } => vec![command.as_str()],
            Self::Sub { items, .. } => items.iter().flat_map(Self::commands).collect(),
            Self::Separator => Vec::new(),
        }
    }
}

/// A menu of the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    /// The title in the bar, e.g. `File`.
    pub title: String,
    /// Its items.
    pub items: Vec<MenuItem>,
}

impl Menu {
    /// A menu.
    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

/// How a command's item looks right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemState {
    /// `false` grays the item out.
    pub enabled: bool,
    /// `Some(true)` draws a check mark.
    pub checked: Option<bool>,
}

impl ItemState {
    /// An item that works.
    #[must_use]
    pub const fn on() -> Self {
        Self {
            enabled: true,
            checked: None,
        }
    }

    /// An item that does nothing yet.
    #[must_use]
    pub const fn off() -> Self {
        Self {
            enabled: false,
            checked: None,
        }
    }

    /// A working item with a check mark when `checked`.
    #[must_use]
    pub const fn check(checked: bool) -> Self {
        Self {
            enabled: true,
            checked: Some(checked),
        }
    }
}

/// How tall the rows of a menu are: compact on a desktop, finger sized on
/// a touch screen.
fn menu_density(ctx: &egui::Context) -> Density {
    if crate::widgets::is_touch(ctx) {
        Density::Touch
    } else {
        Density::Compact
    }
}

/// A menu bar's state between frames: which menu is open, and the state of
/// that menu (its open submenus, the row the keyboard is on).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuBar {
    open: Option<usize>,
    menu: MenuState,
}

impl MenuBar {
    /// A closed bar.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the menu titled `title` (scripted runs), at once and without
    /// a fade. Returns `false` if there is none.
    pub fn open(&mut self, menus: &[Menu], title: &str) -> bool {
        let found = menus
            .iter()
            .position(|menu| menu.title.eq_ignore_ascii_case(title));
        if found.is_some() {
            self.open = found;
            self.menu.open_at_once();
        }
        found.is_some()
    }

    /// Opens a [`Self::dropdown`] menu (scripted runs, a shortcut), at once
    /// and without a fade.
    pub fn open_dropdown(&mut self) {
        self.open = Some(0);
        self.menu.open_at_once();
    }

    /// Closes every menu.
    pub fn close(&mut self) {
        self.open = None;
        self.menu.close();
    }

    /// `true` while a menu is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The open menu's state: its open submenus and the row the pointer or
    /// the keyboard is on (tests, scripted runs).
    #[must_use]
    pub fn state(&self) -> &MenuState {
        &self.menu
    }

    /// Draws the bar into `rect` and any open menu over everything. Returns
    /// the command of the item picked this frame.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        menus: &[Menu],
        state: &dyn Fn(&str) -> ItemState,
        theme: &Theme,
    ) -> Option<String> {
        let base = ui.scope_id().with("sue-menubar");
        if self.open.is_some_and(|open| open >= menus.len()) {
            self.close();
        }
        let painter = ui.painter().with_clip_rect(rect);
        let font = theme.text_font(TextRole::Body);
        let radius = theme.radius(Radius::Md);
        let mut x = rect.left() + 4.0;
        let mut titles = Vec::with_capacity(menus.len());
        // Only a pointer that moves takes the bar from menu to menu: one
        // resting on a title leaves the keyboard free to go elsewhere.
        let moved = ui.input(|input| input.pointer.delta() != egui::Vec2::ZERO);
        for (index, menu) in menus.iter().enumerate() {
            let galley = painter.layout_no_wrap(menu.title.clone(), font.clone(), theme.text);
            let title = Rect::from_min_size(
                pos2(x, rect.top() + 2.0),
                vec2(galley.size().x + 18.0, rect.height() - 4.0),
            );
            x = title.right();
            let response = ui.interact(title, base.with(("title", index)), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::MenuItem, true, menu.title.as_str())
            });
            if response.clicked() {
                if self.open == Some(index) {
                    self.close();
                } else {
                    self.open = Some(index);
                    self.menu.open();
                }
            } else if moved && response.hovered() && self.open.is_some() && self.open != Some(index)
            {
                // While a menu is open, the pointer moving along the bar
                // shows the menu under it.
                self.open = Some(index);
                self.menu.switch(&menu.items, false);
            }
            let shown = self.open == Some(index);
            if shown || response.hovered() {
                painter.rect_filled(
                    title,
                    radius,
                    if shown {
                        theme.fill_selected()
                    } else {
                        theme.fill_hover()
                    },
                );
            }
            painter.galley(title.center() - galley.size() / 2.0, galley, theme.text);
            titles.push(title);
        }
        let Some(open) = self.open else {
            self.menu.idle(ui.ctx());
            return None;
        };
        let event = self.menu.show(
            ui.ctx(),
            &MenuRequest {
                id: base.with(("menu", open)),
                anchor: titles[open].left_bottom() + vec2(0.0, 1.0),
                owner: rect,
                items: &menus[open].items,
                state,
                density: menu_density(ui.ctx()),
            },
            theme,
        );
        self.answer(event, menus)
    }

    /// What the bar does about what its open menu did.
    fn answer(&mut self, event: MenuEvent, menus: &[Menu]) -> Option<String> {
        let open = self.open?;
        let count = menus.len();
        match event {
            MenuEvent::Open => None,
            MenuEvent::Picked(command) => {
                self.close();
                Some(command)
            }
            MenuEvent::Closed => {
                self.close();
                None
            }
            MenuEvent::Previous | MenuEvent::Next => {
                let next = if event == MenuEvent::Next {
                    (open + 1) % count
                } else {
                    (open + count - 1) % count
                };
                self.open = Some(next);
                self.menu.switch(&menus[next].items, true);
                // The new menu is drawn from the next frame on.
                None
            }
        }
    }

    /// A single menu under a button the caller drew (the Sue button):
    /// `toggle` is the button's click, which opens or closes it. Returns
    /// the command of the item picked this frame.
    pub fn dropdown(
        &mut self,
        ui: &mut Ui,
        button: Rect,
        toggle: bool,
        items: &[MenuItem],
        state: &dyn Fn(&str) -> ItemState,
        theme: &Theme,
    ) -> Option<String> {
        if toggle {
            if self.open.is_some() {
                self.close();
            } else {
                self.open = Some(0);
                self.menu.open();
            }
        }
        if self.open.is_none() {
            self.menu.idle(ui.ctx());
            return None;
        }
        let base: Id = ui.scope_id().with("sue-dropdown");
        let event = self.menu.show(
            ui.ctx(),
            &MenuRequest {
                id: base,
                anchor: button.left_bottom() + vec2(0.0, 1.0),
                owner: button,
                items,
                state,
                density: menu_density(ui.ctx()),
            },
            theme,
        );
        match event {
            MenuEvent::Picked(command) => {
                self.close();
                Some(command)
            }
            MenuEvent::Closed => {
                self.close();
                None
            }
            // One menu has no menu beside it.
            MenuEvent::Open | MenuEvent::Previous | MenuEvent::Next => None,
        }
    }
}

/// Shows `items` as a right-click menu in `ui` (inside egui's context menu),
/// nested menus opening to the side, each item grayed or checked as
/// `state` says, its note shown on hover. Returns the command picked; the
/// menu closes then.
///
/// New code opens its right-click menus with [`crate::ui::context_menu`],
/// which draws them like every other menu and answers the keyboard.
pub fn context_items(
    ui: &mut Ui,
    items: &[MenuItem],
    state: &dyn Fn(&str) -> ItemState,
) -> Option<String> {
    let mut picked = None;
    for item in items {
        match item {
            MenuItem::Action {
                label,
                command,
                shortcut,
                note,
            } => {
                let now = state(command);
                let mut button =
                    egui::Button::new(label.as_str()).selected(now.checked == Some(true));
                if !shortcut.is_empty() {
                    button = button.shortcut_text(shortcut.as_str());
                }
                let mut response = ui.add_enabled(now.enabled, button);
                if !note.is_empty() {
                    response = response
                        .on_hover_text(note.as_str())
                        .on_disabled_hover_text(note.as_str());
                }
                if response.clicked() {
                    picked = Some(command.clone());
                    ui.close();
                }
            }
            MenuItem::Sub { label, items } => {
                ui.menu_button(label.as_str(), |ui| {
                    if let Some(command) = context_items(ui, items, state) {
                        picked = Some(command);
                    }
                });
            }
            MenuItem::Separator => {
                ui.separator();
            }
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers, Pos2, RawInput};

    fn menus() -> Vec<Menu> {
        vec![
            Menu::new(
                "File",
                vec![
                    MenuItem::action("New...", "file.new").key("Ctrl+N"),
                    MenuItem::Separator,
                    MenuItem::sub(
                        "Export",
                        vec![
                            MenuItem::action(".png", "file.export.png"),
                            MenuItem::action(".psd", "file.export.psd"),
                        ],
                    ),
                ],
            ),
            Menu::new("Layer", vec![MenuItem::action("Delete", "layer.delete")]),
        ]
    }

    #[test]
    fn items_list_their_commands_keys_and_notes() {
        let menus = menus();
        let commands: Vec<&str> = menus[0].items.iter().flat_map(MenuItem::commands).collect();
        assert_eq!(commands, ["file.new", "file.export.png", "file.export.psd"]);
        assert_eq!(
            menus[0].items[0],
            MenuItem::Action {
                label: "New...".to_owned(),
                command: "file.new".to_owned(),
                shortcut: "Ctrl+N".to_owned(),
                note: String::new(),
            }
        );
        // A note and a key do not undo each other.
        let item = MenuItem::action("Crop", "edit.crop")
            .note("Needs a selection")
            .key("Ctrl+K");
        assert_eq!(
            item,
            MenuItem::Action {
                label: "Crop".to_owned(),
                command: "edit.crop".to_owned(),
                shortcut: "Ctrl+K".to_owned(),
                note: "Needs a selection".to_owned(),
            }
        );
        assert_eq!(item.label(), "Crop");
        // Separators and submenus take no shortcut and no note.
        assert_eq!(MenuItem::Separator.key("Ctrl+X"), MenuItem::Separator);
        assert_eq!(MenuItem::Separator.note("Why"), MenuItem::Separator);
        assert_eq!(MenuItem::Separator.label(), "");
    }

    #[test]
    fn scripted_runs_open_menus_by_title() {
        let menus = menus();
        let mut bar = MenuBar::new();
        assert!(!bar.is_open());
        assert!(bar.open(&menus, "layer"));
        assert!(bar.is_open());
        assert!(!bar.open(&menus, "Story"));
        assert!(bar.is_open(), "a missing title leaves the bar as it was");
        bar.close();
        assert!(!bar.is_open());
    }

    /// One frame of a bar 800 points wide with `events`.
    fn frame(
        ctx: &egui::Context,
        bar: &mut MenuBar,
        menus: &[Menu],
        events: Vec<Event>,
    ) -> Option<String> {
        let theme = Theme::from_rice(&sue_rice::RiceSchema::new());
        let state = |command: &str| {
            if command == "layer.delete" || command.starts_with("file.") {
                ItemState::on()
            } else {
                ItemState::off()
            }
        };
        let mut picked = None;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 24.0));
            picked = bar.ui(ui, rect, menus, &state, &theme);
        });
        output.textures_delta.clear();
        picked
    }

    fn key(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn a_frame_draws_an_open_menu_and_picks_nothing_without_input() {
        let menus = menus();
        let ctx = egui::Context::default();
        let mut bar = MenuBar::new();
        bar.open(&menus, "File");
        assert_eq!(frame(&ctx, &mut bar, &menus, Vec::new()), None);
        assert!(bar.is_open());
    }

    #[test]
    fn the_keyboard_walks_the_bar_and_picks() {
        let menus = menus();
        let ctx = egui::Context::default();
        let mut bar = MenuBar::new();
        bar.open(&menus, "File");
        frame(&ctx, &mut bar, &menus, Vec::new());
        // Down lands on the first row, Down again skips the line.
        frame(&ctx, &mut bar, &menus, vec![key(Key::ArrowDown)]);
        assert_eq!(bar.state().cursor(), Some((0, 0)));
        frame(&ctx, &mut bar, &menus, vec![key(Key::ArrowDown)]);
        assert_eq!(bar.state().cursor(), Some((0, 2)));
        // Right opens Export on its first row; Left comes back.
        frame(&ctx, &mut bar, &menus, vec![key(Key::ArrowRight)]);
        assert_eq!(bar.state().path(), [2]);
        assert_eq!(bar.state().cursor(), Some((1, 0)));
        frame(&ctx, &mut bar, &menus, vec![key(Key::ArrowLeft)]);
        assert_eq!(bar.state().path(), [] as [usize; 0]);
        assert_eq!(bar.state().cursor(), Some((0, 2)));
        // Left on the top level goes to the menu before, wrapping to Layer.
        frame(&ctx, &mut bar, &menus, vec![key(Key::ArrowLeft)]);
        assert_eq!(bar.open, Some(1));
        assert_eq!(bar.state().cursor(), Some((0, 0)));
        // Enter picks Delete and closes the bar.
        let picked = frame(&ctx, &mut bar, &menus, vec![key(Key::Enter)]);
        assert_eq!(picked.as_deref(), Some("layer.delete"));
        assert!(!bar.is_open());
    }

    #[test]
    fn a_pointer_resting_on_a_title_leaves_the_keyboard_free() {
        let menus = menus();
        let ctx = egui::Context::default();
        let mut bar = MenuBar::new();
        bar.open(&menus, "File");
        // The pointer comes to rest on File's title.
        frame(
            &ctx,
            &mut bar,
            &menus,
            vec![Event::PointerMoved(pos2(10.0, 12.0))],
        );
        frame(&ctx, &mut bar, &menus, Vec::new());
        assert_eq!(bar.open, Some(0));
        frame(&ctx, &mut bar, &menus, vec![key(Key::ArrowRight)]);
        frame(&ctx, &mut bar, &menus, Vec::new());
        frame(&ctx, &mut bar, &menus, Vec::new());
        assert_eq!(bar.open, Some(1), "Right went to Layer, and it stays");
        // A pointer that moves over File's title shows File again.
        frame(
            &ctx,
            &mut bar,
            &menus,
            vec![Event::PointerMoved(pos2(12.0, 12.0))],
        );
        assert_eq!(bar.open, Some(0));
    }

    #[test]
    fn escape_closes_the_bar() {
        let menus = menus();
        let ctx = egui::Context::default();
        let mut bar = MenuBar::new();
        bar.open(&menus, "Layer");
        frame(&ctx, &mut bar, &menus, Vec::new());
        assert_eq!(frame(&ctx, &mut bar, &menus, vec![key(Key::Escape)]), None);
        assert!(!bar.is_open());
    }
}
