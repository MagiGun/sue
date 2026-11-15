//! The Color Set palette, its settings menu, and Color History.
//!
//! A set is a grid of cells. A click on a color paints with it, a double
//! click opens Color Settings on it, a right click has Add, Replace,
//! Delete, Change Color Name and Edit Color; a click on an empty cell puts
//! the drawing color there. Dragging a swatch moves it in the set. The
//! wrench opens the set's menu: a new set (asked for its name), rename,
//! copy, delete, a standard set added again, Edit Color Sets, and the
//! swatch size.

use std::path::Path;

use egui::{Id, Rect, Sense, StrokeKind, Ui, UiBuilder, pos2, vec2};
use sue_wm::ui::{self, Density, MenuEvent, MenuRequest, Radius, Space, TextRole};
use sue_wm::{AppletContext, ShellRequest, Theme};
use sue_wm::{ItemState, MenuItem};

use crate::colorsets::{self, ClsError, Standard};
use crate::dialogs::ColorTarget;
use crate::icons::Icon;
use crate::studio::SuePaint;

/// Small swatches, in control heights.
const SMALL: f32 = 0.6;
/// Large swatches, in control heights.
const LARGE: f32 = 1.6;

/// What a frame of the palette asked for, done once it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    /// Paint with the color of a cell, and pick it.
    Pick(usize),
    /// Put the drawing color into a cell.
    Put(usize),
    /// Add the drawing color to the set.
    Add,
    /// Empty a cell.
    Delete(usize),
    /// Ask for a cell's name.
    Name(usize),
    /// Open Color Settings on a cell.
    Edit(usize),
    /// Move a cell to another place.
    Move(usize, usize),
    /// Open Color Settings on the main (`false`) or sub color.
    Settings(bool),
    /// Make the main (`false`) or sub color the drawing color.
    Choose(bool),
}

impl SuePaint {
    pub(crate) fn color_set_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::dock::Palette;
        // Material is a palette of its own, never a tab here.
        let tabs = [
            (tr!("color-set"), Icon::Swatches, true, None),
            (
                tr!("color-history"),
                Icon::History,
                true,
                Some(Palette::ColorHistory),
            ),
        ];
        let (body, tab) = self.torn_header(ui, rect, &tabs, self.set_tab.min(1), theme);
        self.set_tab = tab;
        self.color_set_body(ui, body, cx, theme);
    }

    /// The Color Set palette's tab showing (`set_tab`: 0 the set, 1 Color
    /// History), in `body`.
    pub(crate) fn color_set_body(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let body = body.shrink(theme.space(Space::S3));
        let editing = self.set_tab != 1;
        let density = Density::of(ui);
        let line = density.control_height();
        let gap = theme.space(Space::S2);
        let mut grid = body;
        if editing {
            self.color_sets.load(colorsets::folder(cx).as_deref());
            let top = Rect::from_min_size(body.min, vec2(body.width(), line));
            self.color_set_bar(ui, top, cx, theme);
            let bottom = Rect::from_min_max(pos2(body.left(), body.bottom() - line), body.max);
            grid = Rect::from_min_max(
                pos2(body.left(), top.bottom() + gap),
                pos2(body.right(), bottom.top() - gap),
            );
            let act = self.color_set_footer(ui, bottom, theme);
            if let Some(act) = act {
                self.color_set_act(act, cx);
            }
        }
        let act = self.color_set_grid(ui, grid, editing, theme);
        if let Some(act) = act {
            self.color_set_act(act, cx);
        }
    }

    /// The row over the set: which set, its menu, import and export.
    fn color_set_bar(&mut self, ui: &mut Ui, row: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        let density = Density::of(ui);
        let button = density.icon_button().min(row.height());
        let gap = theme.space(Space::S1);
        let icons_at = |index: f32| {
            Rect::from_min_size(
                pos2(
                    row.right() - (3.0 - index) * (button + gap) + gap,
                    row.center().y - button / 2.0,
                ),
                vec2(button, button),
            )
        };
        let names: Vec<String> = self
            .color_sets
            .sets
            .iter()
            .map(|set| set.name.clone())
            .collect();
        let mut current = self.color_sets.current.min(names.len().saturating_sub(1));
        let pick = Rect::from_min_max(row.min, pos2(icons_at(0.0).left() - gap, row.bottom()));
        let mut inner = ui.new_child(UiBuilder::new().max_rect(pick));
        let response = ui::dropdown("sue-paint-color-set", &mut current, &names)
            .label(&tr!("color-set"))
            .width(pick.width())
            .show(&mut inner, theme);
        let response = ui::tooltip(response, &ui::Tip::new(&tr!("color-set")), theme);
        if response.changed() {
            self.color_sets.current = current;
            self.color_sets.selected = None;
        }
        let menu_button = ui::icon_button(
            "sue-paint-color-set-menu",
            sue_wm::icons::Icon::Wrench,
            &tr!("color-set-settings"),
        )
        .selected(self.set_menu)
        .show_at(ui, icons_at(0.0), theme);
        if menu_button.clicked() {
            self.set_menu = !self.set_menu;
            if self.set_menu {
                self.color_ui.set_menu.open();
            } else {
                self.color_ui.set_menu.close();
            }
        }
        let me = || sue_wm::AppletId::new(crate::studio::ID).expect("a valid id");
        if ui::icon_button(
            "sue-paint-color-set-import",
            sue_wm::icons::Icon::Import,
            &tr!("import-color-set-files"),
        )
        .show_at(ui, icons_at(1.0), theme)
        .clicked()
        {
            cx.request(ShellRequest::PickFile(
                sue_wm::FilePick::open(Some(me()), "colorset-import", &tr!("import-a-color-set"))
                    .extensions(&["gpl", "cls", "txt", "hex"]),
            ));
        }
        if ui::icon_button(
            "sue-paint-color-set-export",
            sue_wm::icons::Icon::Export,
            &tr!("export-color-set-gimp-gpl"),
        )
        .show_at(ui, icons_at(2.0), theme)
        .clicked()
        {
            let name = names
                .get(current)
                .cloned()
                .unwrap_or_else(|| tr!("my-colors"));
            cx.request(ShellRequest::PickFile(
                sue_wm::FilePick::save(
                    Some(me()),
                    "colorset-export",
                    &tr!("export-the-color-set"),
                    &format!("{name}.gpl"),
                )
                .extensions(&["gpl"]),
            ));
        }
        if self.set_menu {
            self.color_set_menu(ui, menu_button.rect, cx, theme);
        }
    }

    /// The set's settings menu, under the wrench.
    fn color_set_menu(
        &mut self,
        ui: &mut Ui,
        button: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let standards = Standard::ALL
            .iter()
            .enumerate()
            .map(|(index, standard)| MenuItem::action(standard.name(), format!("standard.{index}")))
            .collect();
        let items = [
            MenuItem::action(tr!("new-color-set-ellipsis"), "new"),
            MenuItem::action(tr!("rename-color-set-ellipsis"), "rename"),
            MenuItem::action(tr!("duplicate-this-set"), "duplicate"),
            MenuItem::action(tr!("delete-this-set"), "delete").note(tr!("the-last-set-stays")),
            MenuItem::Separator,
            MenuItem::sub(tr!("add-standard-set"), standards),
            MenuItem::action(tr!("edit-color-sets-ellipsis"), "edit"),
            MenuItem::Separator,
            MenuItem::action(tr!("small-swatches"), "small"),
            MenuItem::action(tr!("medium-swatches"), "medium"),
            MenuItem::action(tr!("large-swatches"), "large"),
        ];
        let tile = self.color_sets.tile;
        let many = self.color_sets.sets.len() > 1;
        let state = |command: &str| match command {
            "delete" if !many => ItemState::off(),
            "small" => ItemState::check(tile > 0.0 && tile < 1.0),
            "medium" => ItemState::check(tile <= 0.0),
            "large" => ItemState::check(tile > 1.0),
            _ => ItemState::on(),
        };
        let event = self.color_ui.set_menu.show(
            ui.ctx(),
            &MenuRequest {
                id: Id::unique("sue-paint-color-set-settings"),
                anchor: button.left_bottom(),
                owner: button,
                items: &items,
                state: &state,
                density: Density::of(ui),
            },
            theme,
        );
        let picked = match event {
            MenuEvent::Open | MenuEvent::Previous | MenuEvent::Next => return,
            MenuEvent::Closed => None,
            MenuEvent::Picked(command) => Some(command),
        };
        self.set_menu = false;
        let Some(command) = picked else {
            return;
        };
        let sets = &mut self.color_sets;
        let current = sets.current;
        // The swatch size is kept as a share of a control's height, so it
        // follows the interface's scale and density.
        match command.as_str() {
            "new" => self.open_set_name(None),
            "rename" => self.open_set_name(Some(current)),
            "edit" => self.open_edit_sets(),
            "duplicate" => {
                sets.duplicate(current);
            }
            "delete" => {
                sets.delete(current);
            }
            "small" => sets.tile = SMALL,
            "medium" => sets.tile = 0.0,
            "large" => sets.tile = LARGE,
            other => {
                if let Some(standard) = other
                    .strip_prefix("standard.")
                    .and_then(|index| index.parse::<usize>().ok())
                    .and_then(|index| Standard::ALL.get(index))
                {
                    sets.add_standard(*standard);
                }
            }
        }
        self.save_color_sets(cx);
    }

    /// The side of a swatch in a grid `width` wide.
    fn swatch_side(&self, ui: &Ui, width: f32) -> f32 {
        let line = Density::of(ui).control_height();
        let tile = self.color_sets.tile;
        let side = if tile <= 0.0 {
            width / 8.0
        } else if tile <= LARGE {
            line * tile
        } else {
            // A size kept in points (Sue 0.0.75's swatch sizes).
            tile
        };
        side.max(line * SMALL).floor()
    }

    /// The cells (or Color History's colors) in `area`, scrolled when they
    /// are more than it holds.
    fn color_set_grid(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        editing: bool,
        theme: &Theme,
    ) -> Option<Act> {
        let cells: Vec<colorsets::Cell> = if editing {
            self.color_sets.cells().to_vec()
        } else {
            self.recent
                .colors()
                .iter()
                .map(|rgb| Some(colorsets::Swatch::of(*rgb)))
                .collect()
        };
        if cells.is_empty() {
            let mut inner = ui.new_child(UiBuilder::new().max_rect(area));
            let text = if editing {
                tr!("empty-set-add-drawing-color")
            } else {
                tr!("colors-you-paint-show-here")
            };
            inner.label(
                egui::RichText::new(text)
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
            );
            return None;
        }
        let side = self.swatch_side(ui, area.width());
        let columns = ((area.width() / side).floor() as usize).max(1);
        let cell = (area.width() / columns as f32).floor();
        let rows = cells.len().div_ceil(columns);
        let gap = theme.hairline().width;
        let picked = if editing {
            self.color_sets.picked()
        } else {
            None
        };
        let dragging = self.color_ui.dragging.filter(|_| editing);
        let set = self.color_sets.current;
        let mut act = None;
        let mut drop_on = None;
        let mut inner = ui.new_child(UiBuilder::new().max_rect(area));
        let salt = if editing { "set" } else { "history" };
        ui::ScrollView::vertical(("sue-paint-swatches", salt)).show(&mut inner, theme, |ui| {
            let (grid, _) =
                ui.allocate_exact_size(vec2(area.width(), rows as f32 * cell), Sense::hover());
            for (index, swatch) in cells.iter().enumerate() {
                let (column, row) = (index % columns, index / columns);
                let rect = Rect::from_min_size(
                    pos2(
                        grid.left() + column as f32 * cell,
                        grid.top() + row as f32 * cell,
                    ),
                    vec2(cell - gap, cell - gap),
                );
                let sense = if editing {
                    Sense::click_and_drag()
                } else {
                    Sense::click()
                };
                let response = ui.interact(
                    rect,
                    Id::unique(("sue-paint-swatch", salt, set, index)),
                    sense,
                );
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let radius = theme.radius(Radius::Xs);
                match swatch {
                    Some(swatch) => {
                        let [r, g, b] = swatch.rgb;
                        sue_wm::swatch(ui, rect, [r, g, b, u8::MAX], theme);
                    }
                    None => {
                        sue_wm::checker_fill(ui.painter(), rect, radius, rect.width() / 4.0);
                        ui.painter().rect_stroke(
                            rect,
                            radius,
                            theme.hairline(),
                            StrokeKind::Inside,
                        );
                    }
                }
                let chosen = picked == Some(index);
                if chosen || response.hovered() || drop_on == Some(index) {
                    let stroke = if chosen {
                        theme.selected_stroke()
                    } else {
                        egui::Stroke {
                            color: theme.border_strong(),
                            ..theme.hairline()
                        }
                    };
                    ui.painter()
                        .rect_stroke(rect, radius, stroke, StrokeKind::Inside);
                }
                ui::focus_ring(ui, &response, radius, theme);
                if dragging.is_some() && response.contains_pointer() {
                    drop_on = Some(index);
                }
                let tip_name = match swatch {
                    Some(swatch) => swatch.label(),
                    None => tr!("empty-cell"),
                };
                let help = match (swatch, editing) {
                    (Some(_), true) => tr!("swatch-help"),
                    (None, _) => tr!("empty-cell-help"),
                    (Some(_), false) => String::new(),
                };
                let response = ui::tooltip(response, &ui::Tip::new(&tip_name).help(&help), theme);
                if response.drag_started() && swatch.is_some() {
                    self.color_ui.dragging = Some(index);
                }
                if response.double_clicked() && swatch.is_some() && editing {
                    act = Some(Act::Edit(index));
                } else if response.clicked() {
                    act = Some(match swatch {
                        Some(_) => Act::Pick(index),
                        None => Act::Put(index),
                    });
                }
                if editing {
                    let filled = swatch.is_some();
                    let items = [
                        MenuItem::action(tr!("add-color"), "add"),
                        MenuItem::action(tr!("replace-color"), "replace"),
                        MenuItem::action(tr!("delete-color"), "delete"),
                        MenuItem::Separator,
                        MenuItem::action(tr!("change-color-name-ellipsis"), "name"),
                        MenuItem::action(tr!("edit-color-ellipsis"), "edit"),
                    ];
                    let state = |command: &str| match command {
                        "delete" | "name" | "edit" if !filled => ItemState::off(),
                        _ => ItemState::on(),
                    };
                    if let Some(command) = ui::context_menu(ui, &response, &items, &state, theme) {
                        act = match command.as_str() {
                            "add" => Some(Act::Add),
                            "replace" => Some(Act::Put(index)),
                            "delete" => Some(Act::Delete(index)),
                            "name" => Some(Act::Name(index)),
                            "edit" => Some(Act::Edit(index)),
                            _ => None,
                        };
                    }
                }
            }
        });
        if let Some(from) = dragging
            && ui.input(|input| input.pointer.any_released())
        {
            self.color_ui.dragging = None;
            if let Some(to) = drop_on.filter(|to| *to != from) {
                act = Some(Act::Move(from, to));
            }
        }
        act
    }

    /// Under the set: the main and sub color, and adding, replacing and
    /// deleting with the drawing color.
    fn color_set_footer(&mut self, ui: &mut Ui, row: Rect, theme: &Theme) -> Option<Act> {
        let density = Density::of(ui);
        let side = density.icon().min(row.height());
        let gap = theme.space(Space::S2);
        let mut act = None;
        let radius = theme.radius(Radius::Xs);
        for (index, (rgb, sub)) in [(self.main, false), (self.sub, true)]
            .into_iter()
            .enumerate()
        {
            let rect = Rect::from_min_size(
                pos2(
                    row.left() + index as f32 * (side + gap),
                    row.center().y - side / 2.0,
                ),
                vec2(side, side),
            );
            let [r, g, b] = rgb;
            sue_wm::swatch(ui, rect, [r, g, b, u8::MAX], theme);
            if sub == self.sub_active && !self.transparent {
                ui.painter().rect_stroke(
                    rect,
                    radius,
                    theme.selected_stroke(),
                    StrokeKind::Outside,
                );
            }
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-set-drawing-color", sub)),
                Sense::click(),
            );
            ui::focus_ring(ui, &response, radius, theme);
            let name = if sub {
                tr!("sub-color")
            } else {
                tr!("main-color")
            };
            let response = ui::tooltip(
                response,
                &ui::Tip::new(&name).help(&tr!("double-click-for-color-settings")),
                theme,
            );
            if response.double_clicked() {
                act = Some(Act::Settings(sub));
            } else if response.clicked() {
                act = Some(Act::Choose(sub));
            }
        }
        let button = density.icon_button().min(row.height());
        let at = |index: f32| {
            Rect::from_min_size(
                pos2(
                    row.right() - (3.0 - index) * (button + gap) + gap,
                    row.center().y - button / 2.0,
                ),
                vec2(button, button),
            )
        };
        let has = self.color_sets.picked().filter(|cell| {
            self.color_sets
                .cells()
                .get(*cell)
                .is_some_and(Option::is_some)
        });
        if ui::icon_button(
            "sue-paint-set-add",
            sue_wm::icons::Icon::Plus,
            &tr!("add-the-drawing-color"),
        )
        .show_at(ui, at(0.0), theme)
        .clicked()
        {
            act = Some(Act::Add);
        }
        let picked = self.color_sets.picked();
        if ui::icon_button(
            "sue-paint-set-replace",
            sue_wm::icons::Icon::Replace,
            &tr!("replace-picked-swatch-drawing-color"),
        )
        .enabled(picked.is_some())
        .show_at(ui, at(1.0), theme)
        .clicked()
            && let Some(cell) = picked
        {
            act = Some(Act::Put(cell));
        }
        if ui::icon_button(
            "sue-paint-set-delete",
            sue_wm::icons::Icon::Delete,
            &tr!("delete-the-picked-swatch"),
        )
        .enabled(has.is_some())
        .show_at(ui, at(2.0), theme)
        .clicked()
            && let Some(cell) = has
        {
            act = Some(Act::Delete(cell));
        }
        act
    }

    /// Does what the palette asked for, and keeps the sets when they
    /// changed.
    fn color_set_act(&mut self, act: Act, cx: &mut AppletContext<'_>) {
        let drawing = self.drawing_color();
        let sets = &mut self.color_sets;
        let changed = match act {
            Act::Pick(cell) => {
                let rgb = if self.set_tab == 1 {
                    self.recent.colors().get(cell).copied()
                } else {
                    sets.selected = Some(cell);
                    sets.cells()
                        .get(cell)
                        .and_then(Option::as_ref)
                        .map(|swatch| swatch.rgb)
                };
                if let Some(rgb) = rgb {
                    self.set_drawing_color(rgb);
                    self.transparent = false;
                }
                false
            }
            Act::Put(cell) => sets.put(cell, drawing),
            Act::Add => sets.add_color(drawing).is_some(),
            Act::Delete(cell) => sets.delete_color(cell),
            Act::Move(from, to) => sets.move_color(from, to),
            Act::Name(cell) => {
                self.open_color_name(cell);
                false
            }
            Act::Edit(cell) => {
                let set = sets.current;
                self.open_color_settings(ColorTarget::Cell(set, cell));
                false
            }
            Act::Settings(sub) => {
                self.open_color_settings(if sub {
                    ColorTarget::Sub
                } else {
                    ColorTarget::Main
                });
                false
            }
            Act::Choose(sub) => {
                self.sub_active = sub;
                self.transparent = false;
                self.hex = crate::color::hex(self.drawing_color());
                false
            }
        };
        if changed {
            self.save_color_sets(cx);
        }
    }

    /// File, a color set brought in: a GIMP palette, a list of hex colors
    /// or a Clip Studio Paint color set becomes a new set, named as the
    /// file names it (else after the file).
    pub(crate) fn import_color_set(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let bytes = match sue_platform::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(tr!("cannot-read-it", error = error));
                return;
            }
        };
        let clip = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("cls"));
        let imported = if clip || bytes.starts_with(b"SLCC") {
            match colorsets::import_cls(&bytes) {
                Ok(imported) => imported,
                Err(ClsError::NotAColorSet | ClsError::Cut) => {
                    cx.fail(tr!("not-a-color-set-file"));
                    return;
                }
            }
        } else {
            colorsets::import(&String::from_utf8_lossy(&bytes))
        };
        if imported.cells.iter().all(Option::is_none) {
            cx.fail(tr!("no-colors-in-that-file"));
            return;
        }
        let name = imported.name.unwrap_or_else(|| {
            path.file_stem().map_or_else(
                || tr!("imported"),
                |stem| stem.to_string_lossy().into_owned(),
            )
        });
        self.color_sets.add_set(&name, imported.cells);
        self.save_color_sets(cx);
    }

    /// The set showing, written out as a GIMP palette.
    pub(crate) fn export_color_set(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(set) = self.color_sets.set() else {
            return;
        };
        match sue_platform::write_atomic(path, colorsets::export_gpl(set).as_bytes()) {
            Ok(()) => cx.saved(path, tr!("gimp-palette")),
            Err(error) => cx.fail(tr!("cannot-write-it", error = error)),
        }
    }
}
