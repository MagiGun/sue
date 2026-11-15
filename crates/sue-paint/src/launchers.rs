//! Clip Studio Paint's launchers: small bars of buttons under what is
//! being worked on. The Selection launcher sits under the selection, the
//! Transform launcher under a transform's box, the Text launcher under the
//! current text layer's words, and the Object launcher under the current
//! layer's drawing while the Operation tool is out. View (and Select, for
//! the Selection launcher) turns each of them on and off.
//!
//! The Selection launcher's buttons are a list of command names people
//! arrange: its last button opens Selection Launcher Settings, where any
//! command of the menus can be added, moved and taken off. The list is
//! kept in `selection_launcher.txt` in Sue Paint's data folder, a file a
//! person can share (`docs/paint-launcher-format.md`).

use egui::{Id, Pos2, Rect, Ui, Vec2, pos2, vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Elevation, Radius, RowAction, Space, Tip};
use sue_wm::{AppletContext, Facts, MenuItem, Theme};

use crate::studio::SuePaint;
use crate::tools::Tool;

/// Where this frame's launchers are, for the canvas: a press there is the
/// launcher's, not the start of a stroke, a selection or a pose (which
/// would hide the launcher before its button saw the click).
fn zones() -> Id {
    Id::NULL.with("sue-paint-launcher-zones")
}

/// Where this frame's launcher buttons are, by what they run.
fn buttons() -> Id {
    Id::NULL.with("sue-paint-launcher-buttons")
}

/// Notes a launcher drawn at `rect` this frame.
pub(crate) fn note(ctx: &egui::Context, rect: Rect) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<Rect>>(zones())
            .push(rect);
    });
}

/// The launchers drawn last frame, taken (each frame notes its own).
pub(crate) fn take(ctx: &egui::Context) -> Vec<Rect> {
    ctx.data_mut(|data| {
        data.remove_temp::<Vec<(String, Rect)>>(buttons());
        data.remove_temp::<Vec<Rect>>(zones())
    })
    .unwrap_or_default()
}

/// The launcher buttons the last frame drew: what each runs (a command
/// name, or `launcher.<what>` for a button of its own) and where it is.
/// For tests that click them by where they are, never by a fixed place.
#[must_use]
pub fn shown_buttons(ctx: &egui::Context) -> Vec<(String, Rect)> {
    ctx.data(|data| data.get_temp::<Vec<(String, Rect)>>(buttons()))
        .unwrap_or_default()
}

/// Notes a mark drawn at `at` this frame that a drag can take (a handle
/// of the camera's frame, of the light table's box, a pin), by its id
/// (`mark.<what>`), for [`shown_buttons`].
pub(crate) fn note_mark(ctx: &egui::Context, id: &str, at: Pos2) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<(String, Rect)>>(buttons())
            .push((format!("mark.{id}"), Rect::from_center_size(at, Vec2::ZERO)));
    });
}

/// One button of a launcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Button {
    /// What it runs: a command name, or `launcher.<what>`.
    pub id: String,
    pub icon: Icon,
    /// What it is called, for its tooltip and screen readers.
    pub name: String,
    /// The keys that run it, as menus show them; empty for none.
    pub keys: String,
    /// Lit, as a switch that is on.
    pub on: bool,
    /// It can be pressed now.
    pub enabled: bool,
}

impl Button {
    /// A button of the launcher's own, which no key runs.
    pub(crate) fn own(id: &str, icon: Icon, name: String) -> Self {
        Self {
            id: format!("launcher.{id}"),
            icon,
            name,
            keys: String::new(),
            on: false,
            enabled: true,
        }
    }

    /// The same button lit when `on`.
    pub(crate) fn lit(mut self, on: bool) -> Self {
        self.on = on;
        self
    }
}

/// What goes on a launcher: a button, or a line between groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    Button(Button),
    Separator,
}

/// Where a bar of `count` buttons and `lines` separators goes: centered
/// under `corners` (screen points), kept inside `viewport`.
fn bar_rect(
    viewport: Rect,
    corners: &[Pos2],
    count: usize,
    lines: usize,
    ui: &Ui,
    theme: &Theme,
) -> (Rect, f32, f32) {
    let side = Density::of(ui).icon_button();
    let gap = theme.space(Space::S1);
    let pad = theme.space(Space::S1);
    let line = theme.space(Space::S3);
    let bottom = corners
        .iter()
        .map(|corner| corner.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let middle = corners.iter().map(|corner| corner.x).sum::<f32>() / corners.len().max(1) as f32;
    let width = count as f32 * side
        + count.saturating_sub(1) as f32 * gap
        + lines as f32 * line
        + 2.0 * pad;
    let height = side + 2.0 * pad;
    let bar = Rect::from_min_size(
        pos2(middle - width / 2.0, bottom + theme.space(Space::S4)),
        vec2(width, height),
    );
    let inset = theme.space(Space::S2);
    let bar = bar.translate(vec2(
        (viewport.left() + inset - bar.left()).max(0.0)
            + (viewport.right() - inset - bar.right()).min(0.0),
        (viewport.bottom() - inset - bar.bottom()).min(0.0),
    ));
    (bar, side, line)
}

/// Draws a launcher of `slots` centered under `corners` (screen points),
/// kept inside `viewport`: a floating bar of icon buttons, each with its
/// name and key in its tooltip and the focus ring when the keyboard is
/// on it. Returns the clicked button's id and the bar's rect.
pub(crate) fn bar(
    ui: &mut Ui,
    viewport: Rect,
    corners: &[Pos2],
    slots: &[Slot],
    theme: &Theme,
) -> (Option<String>, Rect) {
    let count = slots
        .iter()
        .filter(|slot| matches!(slot, Slot::Button(_)))
        .count();
    let lines = slots.len() - count;
    let (bar, side, line) = bar_rect(viewport, corners, count, lines, ui, theme);
    note(ui.ctx(), bar);
    let painter = ui.painter();
    let radius = theme.radius(Radius::Lg);
    painter.add(theme.elevation(Elevation::Raised).as_shape(bar, radius));
    painter.rect_filled(bar, radius, theme.surface(2));
    painter.rect_stroke(bar, radius, theme.hairline(), egui::StrokeKind::Inside);
    let gap = theme.space(Space::S1);
    let pad = theme.space(Space::S1);
    let mut x = bar.left() + pad;
    let mut clicked = None;
    let mut placed = Vec::new();
    for (index, slot) in slots.iter().enumerate() {
        match slot {
            Slot::Separator => {
                let middle = x + line / 2.0 - gap / 2.0;
                ui.painter().vline(
                    middle,
                    (bar.top() + pad * 2.0)..=(bar.bottom() - pad * 2.0),
                    theme.hairline(),
                );
                x += line;
            }
            Slot::Button(button) => {
                let at = Rect::from_min_size(pos2(x, bar.top() + pad), Vec2::splat(side));
                x += side + gap;
                let response = sue_wm::ui::icon_button(
                    ("sue-paint-launcher", index, &button.id),
                    button.icon,
                    "",
                )
                .selected(button.on)
                .enabled(button.enabled)
                .show_at(ui, at, theme);
                let (enabled, on, name) = (button.enabled, button.on, button.name.clone());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::Button, enabled, on, &name)
                });
                let tip = Tip::new(&button.name).shortcut(&button.keys);
                let response = sue_wm::ui::tooltip(response, &tip, theme);
                if button.enabled && response.clicked() {
                    clicked = Some(button.id.clone());
                }
                placed.push((button.id.clone(), at));
            }
        }
    }
    ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<(String, Rect)>>(buttons())
            .extend(placed);
    });
    (clicked, bar)
}

// ----- The Selection launcher's set ------------------------------------------

/// The file the Selection launcher's buttons are kept in.
pub(crate) const SELECTION_FILE: &str = "selection_launcher.txt";

/// The first line of the file, which names what it is.
const SELECTION_HEADER: &str = "# Sue Paint selection launcher";

/// A line between groups of buttons, in the list and in the file.
pub(crate) const SEPARATOR: &str = "-";

/// The Selection launcher's buttons as Sue Paint ships them: Clip Studio
/// Paint's set and order (Deselect, Crop, Invert, Expand, Shrink, Delete,
/// Delete outside, Cut and paste, Copy and paste, Scale/Rotate, Fill, New
/// Tone). Settings is always last and is not in the list.
pub(crate) const SELECTION_DEFAULT: [&str; 12] = [
    "select.none",
    "edit.crop",
    "select.invert",
    "select.expand",
    "select.shrink",
    "edit.clear",
    "edit.clear_outside",
    "edit.cut_paste",
    "edit.copy_paste",
    "edit.transform.scale",
    "edit.fill",
    "layer.new.tone",
];

/// The Selection launcher's buttons and its settings popover.
#[derive(Clone, Debug, Default)]
pub(crate) struct LauncherSet {
    /// Command names and [`SEPARATOR`]s, left to right; `None` until the
    /// file has been read.
    pub items: Option<Vec<String>>,
    /// Selection Launcher Settings shows.
    pub settings: bool,
    /// What the settings' search holds.
    pub search: String,
}

/// The Selection launcher's buttons as the file `text` lists them: a
/// command name or [`SEPARATOR`] a line, `#` lines and blank lines left
/// out. Names this Sue does not know are kept (a newer Sue's file loses
/// nothing) and simply not drawn.
#[must_use]
pub(crate) fn selection_from(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// The file's text for `items`.
#[must_use]
pub(crate) fn selection_text(items: &[String]) -> String {
    let mut text = format!("{SELECTION_HEADER}\n");
    for item in items {
        text.push_str(item);
        text.push('\n');
    }
    text
}

/// The shipped set, as names.
fn default_items() -> Vec<String> {
    SELECTION_DEFAULT
        .iter()
        .map(|&name| name.to_owned())
        .collect()
}

/// `true` if `name` can be a launcher button: a command of the registry
/// that names an icon, not one of a family (a recent file, a workspace)
/// and not hidden.
fn can_be_button(name: &str) -> bool {
    crate::commands::registry()
        .find(name)
        .is_some_and(|(command, call)| {
            Icon::from_name(command.icon).is_some()
                && call.arg.is_empty()
                && !command.flags.has(sue_wm::Flags::HIDDEN)
        })
}

/// The keys that run `command` now, as menus show them (`Ctrl+Z`); empty
/// for none.
pub(crate) fn keys_of(menus: &[sue_wm::Menu], command: &str, cx: &AppletContext<'_>) -> String {
    fn find<'m>(items: &'m [MenuItem], command: &str) -> Option<&'m str> {
        items.iter().find_map(|item| match item {
            MenuItem::Action {
                command: found,
                shortcut,
                ..
            } if found == command && !shortcut.is_empty() => Some(shortcut.as_str()),
            MenuItem::Sub { items, .. } => find(items, command),
            _ => None,
        })
    }
    if let Some(shown) = menus.iter().find_map(|menu| find(&menu.items, command)) {
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

/// Every command of `menus` that can be a launcher button, once each, in
/// menu order, with the menu it is in.
fn menu_commands(menus: &[sue_wm::Menu]) -> Vec<(String, String)> {
    fn walk(items: &[MenuItem], menu: &str, out: &mut Vec<(String, String)>) {
        for item in items {
            match item {
                MenuItem::Action { command, .. } => {
                    if can_be_button(command) && !out.iter().any(|(found, _)| found == command) {
                        out.push((command.clone(), menu.to_owned()));
                    }
                }
                MenuItem::Sub { items, .. } => walk(items, menu, out),
                MenuItem::Separator => {}
            }
        }
    }
    let mut out = Vec::new();
    for menu in menus {
        walk(&menu.items, &menu.title, &mut out);
    }
    // What a selection is for comes first: the Select menu's commands,
    // then Edit's, Layer's and Filter's, then the rest, each in menu order.
    out.sort_by_key(|(command, _)| {
        ["select.", "edit.", "layer.", "filter."]
            .iter()
            .position(|prefix| command.starts_with(prefix))
            .unwrap_or(4)
    });
    out
}

/// What the menus or the registry call `command`, without a trailing
/// ellipsis (a launcher button opens nothing new by its name).
fn name_of(menus: &[sue_wm::Menu], command: &str) -> String {
    crate::menus::label_of(menus, command)
        .map(str::to_owned)
        .or_else(|| crate::commands::registry().label(command))
        .unwrap_or_else(|| command.to_owned())
        .trim_end_matches("...")
        .to_owned()
}

impl SuePaint {
    /// The Selection launcher's buttons, read from its file the first
    /// time they are needed.
    pub(crate) fn launcher_items(&mut self, cx: &AppletContext<'_>) -> Vec<String> {
        if self.launcher_set.items.is_none() {
            let read = crate::colorsets::folder(cx)
                .and_then(|folder| {
                    sue_platform::fs::read_to_string(folder.join(SELECTION_FILE)).ok()
                })
                .map(|text| selection_from(&text));
            self.launcher_set.items = Some(read.unwrap_or_else(default_items));
        }
        self.launcher_set.items.clone().unwrap_or_default()
    }

    /// Changes the Selection launcher's buttons and keeps them in its
    /// file.
    pub(crate) fn set_launcher_items(&mut self, items: Vec<String>, cx: &mut AppletContext<'_>) {
        let text = selection_text(&items);
        self.launcher_set.items = Some(items);
        let Some(folder) = crate::colorsets::folder(cx) else {
            return;
        };
        let written = sue_platform::fs::create_dir_all(&folder)
            .and_then(|()| sue_platform::fs::write(folder.join(SELECTION_FILE), text));
        if let Err(error) = written {
            cx.fail(tr!(
                "cannot-keep-the-selection-launcher",
                error = error.to_string()
            ));
        }
    }

    /// The button for `command` as it stands now.
    fn launcher_button(
        &self,
        command: &str,
        facts: &Facts<'_>,
        cx: &AppletContext<'_>,
    ) -> Option<Button> {
        let (found, _) = crate::commands::registry().find(command)?;
        let icon = Icon::from_name(found.icon)?;
        let state = self.state(command, facts);
        Some(Button {
            id: command.to_owned(),
            icon,
            name: name_of(&self.menus, command),
            keys: keys_of(&self.menus, command, cx),
            on: state.checked == Some(true),
            enabled: state.enabled,
        })
    }

    /// Clip Studio Paint's Selection Launcher: the commands for a
    /// selection, in a bar under it, and Selection Launcher Settings.
    pub(crate) fn selection_launcher(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.launcher || self.gesture.is_some() || self.transform.is_some() {
            self.launcher_set.settings = false;
            return;
        }
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let Some([x0, y0, x1, y1]) = doc
            .selection
            .as_ref()
            .and_then(crate::select::Selection::bounds)
        else {
            self.launcher_set.settings = false;
            return;
        };
        let view = doc.view;
        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            .map(|[x, y]| view.to_screen(viewport, size, pos2(x as f32, y as f32)));
        let items = self.launcher_items(cx);
        let facts = Facts::of(cx);
        let mut slots: Vec<Slot> = items
            .iter()
            .filter_map(|item| {
                if item == SEPARATOR {
                    Some(Slot::Separator)
                } else {
                    self.launcher_button(item, &facts, cx).map(Slot::Button)
                }
            })
            .collect();
        slots.push(Slot::Button(
            Button::own(
                "settings",
                Icon::Settings,
                tr!("selection-launcher-settings"),
            )
            .lit(self.launcher_set.settings),
        ));
        let (clicked, rect) = bar(ui, viewport, &corners, &slots, theme);
        match clicked.as_deref() {
            Some("launcher.settings") => self.launcher_set.settings = !self.launcher_set.settings,
            Some(command) => {
                let command = command.to_owned();
                self.launcher_set.settings = false;
                self.run(&command, cx);
            }
            None => {}
        }
        if self.launcher_set.settings {
            self.launcher_settings(ui, rect, cx, theme);
        }
    }

    /// Selection Launcher Settings, over the launcher: its buttons in a
    /// list (move them up and down, take them off), every menu command
    /// to add with a search, a separator, and the shipped set back.
    /// Every change shows on the launcher at once and is kept.
    fn launcher_settings(
        &mut self,
        ui: &mut Ui,
        anchor: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut items = self.launcher_items(cx);
        let mut open = true;
        let mut search = std::mem::take(&mut self.launcher_set.search);
        let mut changed = false;
        let menus = self.menus.clone();
        let all = menu_commands(&menus);
        let width = theme.space(Space::S9) * 12.0;
        let row = Density::of(ui).row();
        sue_wm::ui::popover(
            ui.ctx(),
            Id::unique("sue-paint-selection-launcher-settings"),
            anchor,
            &mut open,
            theme,
            |ui| {
                ui.set_width(width);
                sue_wm::ui::heading(ui, &tr!("selection-launcher-settings"), theme);
                ui.add_space(theme.space(Space::S2));
                let up = tr!("move-up");
                let down = tr!("move-down");
                let off = tr!("remove-from-launcher");
                let actions = [
                    RowAction::new(Icon::ArrowUp, &up),
                    RowAction::new(Icon::ArrowDown, &down),
                    RowAction::new(Icon::Close, &off),
                ];
                let mut act = None;
                crate::transform_ui::caption(ui, &tr!("launcher-on-it"), theme);
                egui::ScrollArea::vertical()
                    .id_salt("sue-paint-launcher-buttons")
                    .max_height(row * 7.0)
                    .show(ui, |ui| {
                        for (index, item) in items.iter().enumerate() {
                            let (title, icon) = if item == SEPARATOR {
                                (tr!("launcher-separator"), Icon::Minus)
                            } else {
                                let icon = crate::commands::registry()
                                    .find(item)
                                    .and_then(|(command, _)| Icon::from_name(command.icon))
                                    .unwrap_or(Icon::Question);
                                (name_of(&menus, item), icon)
                            };
                            let keys = if item == SEPARATOR {
                                String::new()
                            } else {
                                keys_of(&menus, item, cx)
                            };
                            let shown =
                                sue_wm::ui::list_row(("launcher-item", index, item), &title)
                                    .icon(icon)
                                    .note(&keys)
                                    .actions(&actions)
                                    .show(ui, theme);
                            if let Some(action) = shown.action {
                                act = Some((index, action));
                            }
                        }
                    });
                if let Some((index, action)) = act {
                    match action {
                        0 if index > 0 => items.swap(index, index - 1),
                        1 if index + 1 < items.len() => items.swap(index, index + 1),
                        2 => {
                            items.remove(index);
                        }
                        _ => {}
                    }
                    changed = true;
                }
                sue_wm::ui::divider(ui, theme);
                crate::transform_ui::caption(ui, &tr!("launcher-to-add"), theme);
                let hint = tr!("add-a-command");
                let field = sue_wm::ui::search_field("sue-paint-launcher-search", &mut search)
                    .hint(&hint)
                    .show(ui, theme);
                sue_wm::ui::tooltip(field, &Tip::new(&hint), theme);
                let needle = search.to_lowercase();
                let plus = tr!("add-to-launcher");
                let add = [RowAction::new(Icon::Plus, &plus)];
                egui::ScrollArea::vertical()
                    .id_salt("sue-paint-launcher-commands")
                    .max_height(row * 6.0)
                    .show(ui, |ui| {
                        let offered: Vec<&(String, String)> = all
                            .iter()
                            .filter(|(command, _)| !items.contains(command))
                            .collect();
                        for (command, menu) in offered {
                            let title = name_of(&menus, command);
                            if !needle.is_empty()
                                && !title.to_lowercase().contains(&needle)
                                && !menu.to_lowercase().contains(&needle)
                            {
                                continue;
                            }
                            let icon = crate::commands::registry()
                                .find(command)
                                .and_then(|(found, _)| Icon::from_name(found.icon))
                                .unwrap_or(Icon::Question);
                            let shown = sue_wm::ui::list_row(("launcher-add", command), &title)
                                .icon(icon)
                                .note(menu)
                                .actions(&add)
                                .show(ui, theme);
                            if shown.action.is_some() || shown.response.clicked() {
                                items.push(command.clone());
                                changed = true;
                            }
                        }
                    });
                sue_wm::ui::divider(ui, theme);
                ui.horizontal(|ui| {
                    let name = tr!("launcher-add-separator");
                    let pressed = sue_wm::ui::button("sue-paint-launcher-separator", &name)
                        .icon(Icon::Minus)
                        .show(ui, theme);
                    if pressed.clicked() {
                        items.push(SEPARATOR.to_owned());
                        changed = true;
                    }
                    sue_wm::ui::tooltip(pressed, &Tip::new(&name), theme);
                    let name = tr!("launcher-restore-default");
                    let pressed = sue_wm::ui::button("sue-paint-launcher-default", &name)
                        .icon(Icon::Reset)
                        .show(ui, theme);
                    if pressed.clicked() {
                        items = default_items();
                        changed = true;
                    }
                    sue_wm::ui::tooltip(pressed, &Tip::new(&name), theme);
                });
            },
        );
        self.launcher_set.search = search;
        if !open {
            self.launcher_set.settings = false;
        }
        if changed {
            self.set_launcher_items(items, cx);
        }
    }

    /// Cut and Paste: the selected part of the layer leaves it for a new
    /// layer in the same place, as one undo step.
    /// What was copied before stays copied: the
    /// launcher's pastes do not go through the clipboard.
    pub(crate) fn cut_and_paste(&mut self, cx: &mut AppletContext<'_>) {
        let label = tr!("cut-and-paste");
        let kept = self.clipboard.take();
        one_step(cx, label.clone(), |cx| {
            if self.copy(cx) {
                self.run_clear(cx, false, &label);
                self.paste(cx, &label);
            }
        });
        self.clipboard = kept;
    }

    /// Copy and Paste: the selected part of the layer is copied onto a
    /// new layer in the same place. What was copied before stays copied.
    pub(crate) fn copy_and_paste(&mut self, cx: &mut AppletContext<'_>) {
        let kept = self.clipboard.take();
        if self.copy(cx) {
            self.paste(cx, &tr!("copy-and-paste"));
        }
        self.clipboard = kept;
    }

    /// While the Light table tool is out: the box around what the light
    /// table's layers show, with its corner and edge handles, the turning
    /// handle on its stalk and its middle, and a launcher under it (flip
    /// them left to right, put them back, put the tool away).
    pub(crate) fn light_table_launcher(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::canvas::marks;
        use crate::transform_ui::FrameGrip;
        self.keyed_marks(ui, viewport, size, cx, theme);
        if !self.light_tool {
            return;
        }
        let Some((layers, quad)) = self.light_table_box(&cx.world.arena) else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let grip = match &self.gesture {
            Some(crate::studio::Gesture::LightTable { grip, .. }) => Some(*grip),
            _ => None,
        };
        let shown = quad.map(|at| view.to_screen(viewport, size, pos2(at[0], at[1])));
        let painter = ui.painter().with_clip_rect(viewport);
        marks::guide(&painter, shown.to_vec(), true, theme);
        for (corner, at) in shown.iter().enumerate() {
            marks::handle(
                &painter,
                *at,
                grip == Some(FrameGrip::Corner(corner)),
                theme,
            );
        }
        for edge in 0..4 {
            let (a, b) = (shown[edge], shown[(edge + 1) % 4]);
            marks::handle(
                &painter,
                pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
                grip == Some(FrameGrip::Edge(edge)),
                theme,
            );
        }
        let (from, stalk) = crate::transform_ui::stalk_of(&shown, marks::STALK);
        marks::stalk(&painter, from, stalk, grip == Some(FrameGrip::Turn), theme);
        for (corner, at) in shown.iter().enumerate() {
            note_mark(ui.ctx(), &format!("light.corner.{corner}"), *at);
        }
        note_mark(ui.ctx(), "light.stalk", stalk);
        let middle = crate::transform::middle(&quad);
        // Its centre of rotation, which a drag moves (the box's middle at
        // first, and it goes along when the box moves); while the box turns
        // it stays where it was.
        let from = match &self.gesture {
            Some(crate::studio::Gesture::LightTable {
                grip: FrameGrip::Turn,
                quad,
                ..
            }) => crate::transform::middle(quad),
            _ => middle,
        };
        let centre = view.to_screen(
            viewport,
            size,
            pos2(
                from[0] + self.light_centre[0],
                from[1] + self.light_centre[1],
            ),
        );
        marks::pivot(&painter, centre, grip == Some(FrameGrip::Centre), theme);
        note_mark(ui.ctx(), "light.centre", centre);
        if self.gesture.is_some() {
            return;
        }
        let facts = Facts::of(cx);
        let mut slots = vec![Slot::Button(Button::own(
            "light.flip",
            Icon::FlipHorizontal,
            tr!("light-table-flip"),
        ))];
        slots.extend(
            ["anim.light_table.reset", "anim.light_table.tool"]
                .iter()
                .filter_map(|command| self.launcher_button(command, &facts, cx))
                .map(Slot::Button),
        );
        match bar(ui, viewport, &shown, &slots, theme).0.as_deref() {
            Some("launcher.light.flip") => {
                let Some(size) = self
                    .picture()
                    .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                    .and_then(sue_model::picture::size)
                else {
                    return;
                };
                let layers = Self::with_kept(&cx.world.arena, layers);
                for (layer, was, _) in &layers {
                    if let Ok(entity) = cx.world.arena.get_by_id_mut(*layer) {
                        let flipped = was.then(middle, 0.0, 1.0, true, size);
                        entity
                            .properties_mut()
                            .set(crate::anim::PLACE, flipped.text());
                    }
                }
                Self::light_table_step(cx, &layers);
                if let Some(doc) = self.docs.get_mut(self.active) {
                    doc.compositor
                        .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
                }
            }
            Some(command) => {
                let command = command.to_owned();
                self.run(&command, cx);
            }
            None => {}
        }
    }

    /// With the Object tool on a layer whose keyframes are on: the box
    /// round its drawing where it shows at this frame, with its corner and
    /// edge handles, the turning handle on its stalk and its centre.
    fn keyed_marks(
        &self,
        ui: &Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::canvas::marks;
        use crate::transform_ui::FrameGrip;
        let Some((_, pose, quad, _)) = self.keyed_box(&cx.world.arena) else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let grip = match &self.gesture {
            Some(crate::studio::Gesture::Keyed { grip, .. }) => Some(*grip),
            _ => None,
        };
        let shown = quad.map(|at| view.to_screen(viewport, size, pos2(at[0], at[1])));
        let painter = ui.painter().with_clip_rect(viewport);
        marks::guide(&painter, shown.to_vec(), true, theme);
        for (corner, at) in shown.iter().enumerate() {
            marks::handle(
                &painter,
                *at,
                grip == Some(FrameGrip::Corner(corner)),
                theme,
            );
            note_mark(ui.ctx(), &format!("keyed.corner.{corner}"), shown[corner]);
        }
        for edge in 0..4 {
            let (a, b) = (shown[edge], shown[(edge + 1) % 4]);
            marks::handle(
                &painter,
                pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
                grip == Some(FrameGrip::Edge(edge)),
                theme,
            );
        }
        let (from, stalk) = crate::transform_ui::stalk_of(&shown, marks::STALK);
        marks::stalk(&painter, from, stalk, grip == Some(FrameGrip::Turn), theme);
        note_mark(ui.ctx(), "keyed.stalk", stalk);
        let middle = crate::transform::middle(&quad);
        let _ = pose;
        marks::pivot(
            &painter,
            view.to_screen(viewport, size, pos2(middle[0], middle[1])),
            false,
            theme,
        );
        note_mark(
            ui.ctx(),
            "keyed.middle",
            view.to_screen(viewport, size, pos2(middle[0], middle[1])),
        );
    }

    /// The Text launcher, under the current text layer's words: edit
    /// them, make them smaller or larger, set them upright or centered.
    pub(crate) fn text_launcher(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.text_launcher
            || self.gesture.is_some()
            || self.dialog.is_some()
            || self.files.dialog_open()
        {
            return;
        }
        let tool = self.toolbox.tool();
        if !matches!(tool, Tool::Text | Tool::Operation) {
            return;
        }
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let Some(layer) = doc.layer else {
            return;
        };
        let Some(entity) = cx.world.arena.get_by_id(layer).ok() else {
            return;
        };
        let (Some(lettering), Some([x0, y0, x1, y1])) = (
            crate::lettering::Lettering::of(entity),
            crate::lettering::area_of(entity),
        ) else {
            return;
        };
        let view = doc.view;
        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            .map(|[x, y]| view.to_screen(viewport, size, pos2(x, y)));
        let slots = [
            Button::own("text.edit", Icon::Edit, tr!("edit-text")),
            Button::own("text.smaller", Icon::TextSmaller, tr!("smaller-text")),
            Button::own("text.larger", Icon::TextLarger, tr!("larger-text")),
            Button::own("text.vertical", Icon::TextVertical, tr!("vertical-text"))
                .lit(lettering.vertical),
            Button::own("text.center", Icon::TextCenter, tr!("center-text")).lit(lettering.center),
        ]
        .map(Slot::Button);
        let (Some(clicked), _) = bar(ui, viewport, &corners, &slots, theme) else {
            return;
        };
        let mut next = lettering.clone();
        match clicked.as_str() {
            "launcher.text.edit" => {
                self.dialog = Some(crate::dialogs::Dialog::Text(Some(layer), lettering));
                return;
            }
            "launcher.text.smaller" => next.size = (next.size / 1.2).max(4.0),
            "launcher.text.larger" => next.size = (next.size * 1.2).min(2000.0),
            "launcher.text.vertical" => next.vertical = !next.vertical,
            _ => next.center = !next.center,
        }
        self.put_text(cx, Some(layer), &next);
    }

    /// The Object launcher, under the current layer's drawing while the
    /// Operation tool is out: scale and turn it, flip it, its properties,
    /// or delete it.
    pub(crate) fn object_launcher(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.object_launcher
            || self.gesture.is_some()
            || self.transform.is_some()
            || self.light_tool
            || self.toolbox.tool() != Tool::Operation
        {
            return;
        }
        let Some(info) = self.layer_info(&cx.world.arena) else {
            return;
        };
        if cx
            .world
            .arena
            .get_by_id(info.id)
            .ok()
            .and_then(crate::lettering::Lettering::of)
            .is_some()
        {
            // Text has its own launcher.
            return;
        }
        // Where its drawing is, worked out again only when it changes.
        if self
            .object_bounds
            .is_none_or(|(layer, revision, _)| layer != info.id || revision != info.revision)
        {
            let bounds = crate::doc::raster(&cx.world.arena, info.id).and_then(|raster| {
                let bytes = raster.bytes()?;
                let width = raster.width() as usize;
                let mut found: Option<[usize; 4]> = None;
                for (index, pixel) in bytes.as_chunks::<4>().0.iter().enumerate() {
                    if pixel[3] > 0 {
                        let (x, y) = (index % width, index / width);
                        let b = found.get_or_insert([x, y, x, y]);
                        b[0] = b[0].min(x);
                        b[1] = b[1].min(y);
                        b[2] = b[2].max(x);
                        b[3] = b[3].max(y);
                    }
                }
                found
            });
            self.object_bounds = Some((
                info.id,
                info.revision,
                bounds.map(|[x0, y0, x1, y1]| {
                    let [ox, oy] = info.style.offset.map(|value| value as f32);
                    [
                        x0 as f32 + ox,
                        y0 as f32 + oy,
                        x1 as f32 + 1.0 + ox,
                        y1 as f32 + 1.0 + oy,
                    ]
                }),
            ));
        }
        let Some((_, _, Some([x0, y0, x1, y1]))) = self.object_bounds else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            .map(|[x, y]| view.to_screen(viewport, size, pos2(x, y)));
        let facts = Facts::of(cx);
        let slots: Vec<Slot> = [
            "edit.transform.scale",
            "edit.flip_h",
            "edit.flip_v",
            "window.layer_property",
            "layer.delete",
        ]
        .iter()
        .filter_map(|command| self.launcher_button(command, &facts, cx))
        .map(Slot::Button)
        .collect();
        if let (Some(command), _) = bar(ui, viewport, &corners, &slots, theme) {
            self.run(&command, cx);
        }
    }
}

/// Runs `work` as one undo step called `label`, however many edits it
/// applies.
pub(crate) fn one_step(
    cx: &mut AppletContext<'_>,
    label: String,
    work: impl FnOnce(&mut AppletContext<'_>),
) {
    cx.typing.flush(cx.history);
    let mark = cx.history.cursor();
    work(cx);
    let mut steps = cx.history.take_since(mark);
    if steps.len() == 1 {
        cx.history.record_boxed(steps.remove(0));
    } else if !steps.is_empty() {
        let mut batch = sue_core::txn::Batch::new(label);
        for step in steps {
            batch.push(step);
        }
        cx.history.record(batch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_selection_launcher_file_round_trips_and_keeps_unknown_names() {
        let items = vec![
            "select.none".to_owned(),
            SEPARATOR.to_owned(),
            "edit.crop".to_owned(),
            "future.command".to_owned(),
        ];
        let text = selection_text(&items);
        assert!(text.starts_with(SELECTION_HEADER));
        assert_eq!(selection_from(&text), items);
        assert_eq!(selection_from("\n# a note\n  edit.fill  \n"), ["edit.fill"]);
    }

    #[test]
    fn every_shipped_launcher_button_is_a_command_with_an_icon() {
        for name in SELECTION_DEFAULT {
            assert!(can_be_button(name), "{name}");
        }
        assert!(!can_be_button("future.command"));
        let menus = crate::menus::menus();
        let offered = menu_commands(&menus);
        assert!(offered.len() > 50, "the menus offer their commands");
        for name in SELECTION_DEFAULT {
            assert!(
                offered.iter().any(|(command, _)| command == name),
                "{name} is in a menu, so Settings can add it back"
            );
        }
        // The Select menu's commands are offered first, not File's.
        assert!(offered[0].0.starts_with("select."), "{:?}", offered[0]);
        let first_file = offered
            .iter()
            .position(|(command, _)| command.starts_with("file."));
        let last_select = offered
            .iter()
            .rposition(|(command, _)| command.starts_with("select."));
        assert!(first_file > last_select, "{first_file:?} {last_select:?}");
    }
}
