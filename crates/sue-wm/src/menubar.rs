//! Menu bars for studios: menus described as data, drawn by the shell.
//!
//! A studio lists its menus as [`Menu`]s of [`MenuItem`]s, each naming a
//! command. Every frame [`MenuBar::ui`] asks the studio for the state of
//! each command ([`ItemState`]): items whose command the studio cannot run
//! yet are drawn grayed out and do nothing, so a menu can show the whole
//! plan of a studio while only part of it works. Menus open on click, follow
//! the pointer across the bar while one is open, open submenus on hover,
//! and close on a pick, on Escape or on a click elsewhere. Scripted runs
//! open a menu with [`MenuBar::open`].

use egui::{Align2, Id, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::Theme;

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
        }
    }

    /// The same item showing `shortcut`.
    #[must_use]
    pub fn key(self, shortcut: impl Into<String>) -> Self {
        match self {
            Self::Action { label, command, .. } => Self::Action {
                label,
                command,
                shortcut: shortcut.into(),
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

    fn label(&self) -> &str {
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

/// Height of a menu row.
const ROW: f32 = 24.0;
/// Height of a separator.
const GAP: f32 = 9.0;

/// A menu bar's state between frames: which menu and submenus are open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuBar {
    open: Option<usize>,
    path: Vec<usize>,
}

impl MenuBar {
    /// A closed bar.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the menu titled `title` (scripted runs). Returns `false` if
    /// there is none.
    pub fn open(&mut self, menus: &[Menu], title: &str) -> bool {
        let found = menus
            .iter()
            .position(|menu| menu.title.eq_ignore_ascii_case(title));
        if found.is_some() {
            self.open = found;
            self.path.clear();
        }
        found.is_some()
    }

    /// Closes every menu.
    pub fn close(&mut self) {
        self.open = None;
        self.path.clear();
    }

    /// `true` while a menu is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.is_some()
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
        let painter = ui.painter().with_clip_rect(rect);
        let font = theme.font(theme.ui_size);
        let mut x = rect.left() + 4.0;
        let mut titles = Vec::with_capacity(menus.len());
        for (index, menu) in menus.iter().enumerate() {
            let galley = painter.layout_no_wrap(menu.title.clone(), font.clone(), theme.text);
            let title = Rect::from_min_size(
                pos2(x, rect.top() + 2.0),
                vec2(galley.size().x + 18.0, rect.height() - 4.0),
            );
            x = title.right();
            let response = ui.interact(title, Id::unique(("sue-menubar", index)), Sense::click());
            if response.clicked() {
                self.open = if self.open == Some(index) {
                    None
                } else {
                    Some(index)
                };
                self.path.clear();
            } else if response.hovered() && self.open.is_some() && self.open != Some(index) {
                self.open = Some(index);
                self.path.clear();
            }
            let shown = self.open == Some(index);
            if shown || response.hovered() {
                painter.rect_filled(
                    title,
                    theme.control_radius,
                    if shown { theme.active } else { theme.hover },
                );
            }
            painter.galley(title.center() - galley.size() / 2.0, galley, theme.text);
            titles.push(title);
        }
        let open = self.open?;
        let menu = menus.get(open)?;
        let ctx = ui.ctx().clone();
        let mut areas = vec![rect];
        let mut picked = None;
        let mut level_items = &menu.items;
        let mut anchor = titles[open].left_bottom() + vec2(0.0, 1.0);
        for level in 0..=self.path.len() {
            let (area, sub, chosen) = self.popup(&ctx, level, anchor, level_items, state, theme);
            areas.push(area);
            if chosen.is_some() {
                picked = chosen;
            }
            match sub {
                Some((index, row)) => {
                    if self.path.len() <= level {
                        break;
                    }
                    let _ = index;
                    let Some(MenuItem::Sub { items, .. }) = level_items.get(self.path[level])
                    else {
                        self.path.truncate(level);
                        break;
                    };
                    anchor = pos2(area.right() - 2.0, row.top() - 4.0);
                    level_items = items;
                }
                None => {
                    self.path.truncate(level);
                    break;
                }
            }
        }
        let (clicked, escape, pointer) = ctx.input(|input| {
            (
                input.pointer.any_click(),
                input.key_pressed(egui::Key::Escape),
                input.pointer.interact_pos(),
            )
        });
        let inside = pointer.is_some_and(|at| areas.iter().any(|area| area.contains(at)));
        if picked.is_some() || escape || (clicked && !inside) {
            self.close();
        }
        ctx.request_repaint();
        picked
    }

    /// One open menu at `level`: returns its rect, the open submenu row (if
    /// any) and the command picked.
    #[allow(clippy::too_many_arguments, reason = "one popup of a nested menu")]
    fn popup(
        &mut self,
        ctx: &egui::Context,
        level: usize,
        anchor: Pos2,
        items: &[MenuItem],
        state: &dyn Fn(&str) -> ItemState,
        theme: &Theme,
    ) -> (Rect, Option<(usize, Rect)>, Option<String>) {
        let font = theme.font(theme.ui_size);
        let width = ctx.fonts_mut(|fonts| {
            items
                .iter()
                .map(|item| {
                    let label = fonts
                        .layout_no_wrap(item.label().to_owned(), font.clone(), theme.text)
                        .size()
                        .x;
                    let right = match item {
                        MenuItem::Action { shortcut, .. } if !shortcut.is_empty() => {
                            fonts
                                .layout_no_wrap(shortcut.clone(), font.clone(), theme.text)
                                .size()
                                .x
                                + 32.0
                        }
                        MenuItem::Sub { .. } => 28.0,
                        _ => 0.0,
                    };
                    label + right + 44.0
                })
                .fold(180.0_f32, f32::max)
        });
        let height: f32 = items
            .iter()
            .map(|item| {
                if matches!(item, MenuItem::Separator) {
                    GAP
                } else {
                    ROW
                }
            })
            .sum::<f32>()
            + 8.0;
        let screen = ctx.content_rect();
        let mut origin = anchor;
        origin.x = origin
            .x
            .min(screen.right() - width - 2.0)
            .max(screen.left());
        origin.y = origin
            .y
            .min(screen.bottom() - height - 2.0)
            .max(screen.top());
        let area = Rect::from_min_size(origin, vec2(width, height));
        let mut sub = None;
        let mut picked = None;
        egui::Area::new(Id::unique(("sue-menu-popup", level)))
            .order(Order::Tooltip)
            .fixed_pos(origin)
            .show(ctx, |ui| {
                let painter = ui.painter().clone();
                painter.rect_filled(
                    area.translate(vec2(0.0, 3.0)).expand(2.0),
                    theme.control_radius + 2.0,
                    theme.shadow.gamma_multiply(0.35),
                );
                painter.rect_filled(area, theme.control_radius, theme.pill);
                painter.rect_stroke(
                    area,
                    theme.control_radius,
                    theme.hairline(),
                    StrokeKind::Inside,
                );
                ui.allocate_rect(area, Sense::hover());
                let mut y = area.top() + 4.0;
                for (index, item) in items.iter().enumerate() {
                    if matches!(item, MenuItem::Separator) {
                        let line = y + GAP / 2.0;
                        painter.hline(
                            (area.left() + 8.0)..=(area.right() - 8.0),
                            line,
                            theme.hairline(),
                        );
                        y += GAP;
                        continue;
                    }
                    let row =
                        Rect::from_min_size(pos2(area.left() + 4.0, y), vec2(width - 8.0, ROW));
                    y += ROW;
                    let item_state = match item {
                        MenuItem::Action { command, .. } => state(command),
                        MenuItem::Sub { .. } => {
                            if item.commands().iter().any(|command| state(command).enabled) {
                                ItemState::on()
                            } else {
                                ItemState::off()
                            }
                        }
                        MenuItem::Separator => ItemState::off(),
                    };
                    let response = ui.interact(
                        row,
                        Id::unique(("sue-menu-item", level, index)),
                        Sense::click(),
                    );
                    let open_sub = matches!(item, MenuItem::Sub { .. })
                        && self.path.get(level) == Some(&index);
                    if response.hovered() {
                        if matches!(item, MenuItem::Sub { .. }) {
                            self.path.truncate(level);
                            self.path.push(index);
                        } else if self.path.len() > level {
                            self.path.truncate(level);
                        }
                    }
                    if (response.hovered() || open_sub) && item_state.enabled {
                        painter.rect_filled(row, theme.control_radius, theme.hover);
                    }
                    let color = if item_state.enabled {
                        theme.text
                    } else {
                        theme.muted.gamma_multiply(0.75)
                    };
                    if item_state.checked == Some(true) {
                        let mark = row.left_center() + vec2(13.0, 0.0);
                        painter.line(
                            vec![
                                mark + vec2(-4.0, 0.0),
                                mark + vec2(-1.0, 3.5),
                                mark + vec2(5.0, -4.0),
                            ],
                            Stroke::new(1.5, color),
                        );
                    }
                    painter.text(
                        row.left_center() + vec2(26.0, 0.0),
                        Align2::LEFT_CENTER,
                        item.label(),
                        font.clone(),
                        color,
                    );
                    match item {
                        MenuItem::Action {
                            command, shortcut, ..
                        } => {
                            if !shortcut.is_empty() {
                                painter.text(
                                    row.right_center() - vec2(10.0, 0.0),
                                    Align2::RIGHT_CENTER,
                                    shortcut,
                                    font.clone(),
                                    theme.muted,
                                );
                            }
                            if response.clicked() && item_state.enabled {
                                picked = Some(command.clone());
                            }
                        }
                        MenuItem::Sub { .. } => {
                            let tip = row.right_center() - vec2(12.0, 0.0);
                            painter.line(
                                vec![tip + vec2(-3.0, -4.0), tip, tip + vec2(-3.0, 4.0)],
                                Stroke::new(1.2, color),
                            );
                            if self.path.get(level) == Some(&index) {
                                sub = Some((index, row));
                            }
                        }
                        MenuItem::Separator => {}
                    }
                }
            });
        (area, sub, picked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn items_list_their_commands_and_keys() {
        let menus = menus();
        let commands: Vec<&str> = menus[0].items.iter().flat_map(MenuItem::commands).collect();
        assert_eq!(commands, ["file.new", "file.export.png", "file.export.psd"]);
        assert_eq!(
            menus[0].items[0],
            MenuItem::Action {
                label: "New...".to_owned(),
                command: "file.new".to_owned(),
                shortcut: "Ctrl+N".to_owned(),
            }
        );
        // Separators and submenus take no shortcut.
        assert_eq!(MenuItem::Separator.key("Ctrl+X"), MenuItem::Separator);
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

    #[test]
    fn a_frame_draws_an_open_menu_and_grays_unknown_commands() {
        let menus = menus();
        let theme = Theme::from_rice(&sue_rice::RiceSchema::new());
        let mut bar = MenuBar::new();
        bar.open(&menus, "File");
        let ctx = egui::Context::default();
        let mut picked = Some("unset".to_owned());
        let state = |command: &str| {
            if command == "file.new" {
                ItemState::on()
            } else {
                ItemState::off()
            }
        };
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 24.0));
            picked = bar.ui(ui, rect, &menus, &state, &theme);
        });
        output.textures_delta.clear();
        assert_eq!(picked, None, "nothing is picked without a click");
        assert!(bar.is_open());
    }
}
