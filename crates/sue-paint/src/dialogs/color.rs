//! The color dialogs: Color Settings, which edits one color by eye and by
//! number (hue, saturation and value; red, green and blue; cyan, magenta,
//! yellow and black; L*a*b*; hex), opened by a double click on a color;
//! Edit Color Sets, the list of sets to reorder, add, copy, rename and
//! delete; and the two small dialogs that name a set and a color.
//!
//! The conversions the Color Slider palette shares live here too:
//! [`cmyk_of`], [`rgb_of_cmyk`], [`lab_of`] and [`rgb_of_lab`].

use egui::{Align, Id, Key, Layout, Modifiers, Rect, Sense, Ui, UiBuilder, vec2};
use sue_wm::ui::{self, Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::{Dialog, Outcome};
use crate::color;
use crate::colorsets::Standard;
use crate::studio::SuePaint;

/// The Color Settings dialog's size before the screen clamps it.
const SETTINGS_SIZE: egui::Vec2 = egui::Vec2::new(680.0, 490.0);
/// Edit Color Sets' size.
const SETS_SIZE: egui::Vec2 = egui::Vec2::new(600.0, 420.0);
/// The size of the dialogs that ask for a name.
const NAME_SIZE: egui::Vec2 = egui::Vec2::new(380.0, 180.0);
/// How many control heights the Color Settings wheel is across.
const WHEEL_ROWS: f32 = 8.0;
/// How many control heights the buttons of Edit Color Sets are wide.
const BUTTON_ROWS: f32 = 7.5;
/// How many control heights a number field of Color Settings is wide.
const FIELD_ROWS: f32 = 2.6;

// ----- Conversions -----------------------------------------------------------

/// Cyan, magenta, yellow and black (0 to 1) of sRGB bytes, the simple
/// device conversion with all the gray in black (no color profile).
#[must_use]
pub(crate) fn cmyk_of(rgb: [u8; 3]) -> [f32; 4] {
    let [r, g, b] = color::unit(rgb);
    let k = 1.0 - r.max(g).max(b);
    if k >= 1.0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let ink = |channel: f32| (1.0 - channel - k) / (1.0 - k);
    [ink(r), ink(g), ink(b), k]
}

/// sRGB bytes of cyan, magenta, yellow and black (0 to 1).
#[must_use]
pub(crate) fn rgb_of_cmyk([c, m, y, k]: [f32; 4]) -> [u8; 3] {
    let channel = |ink: f32| (1.0 - ink.clamp(0.0, 1.0)) * (1.0 - k.clamp(0.0, 1.0));
    color::bytes([channel(c), channel(m), channel(y)])
}

/// The D65 white point, in XYZ.
const WHITE: [f32; 3] = [0.950_47, 1.0, 1.088_83];

/// sRGB's transfer curve, undone.
fn linear(channel: f32) -> f32 {
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB's transfer curve.
fn gamma(channel: f32) -> f32 {
    if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

/// CIE L* (0 to 100), a* and b* (about -128 to 127) of sRGB bytes, under
/// the D65 white of sRGB.
#[must_use]
pub(crate) fn lab_of(rgb: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = color::unit(rgb).map(linear);
    let xyz = [
        0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b,
        0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b,
        0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b,
    ];
    let f = |t: f32| {
        if t > 216.0 / 24_389.0 {
            t.cbrt()
        } else {
            (24_389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let [fx, fy, fz] = [0, 1, 2].map(|axis| f(xyz[axis] / WHITE[axis]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// sRGB bytes of CIE L*a*b* under D65; colors outside sRGB are clipped.
#[must_use]
pub(crate) fn rgb_of_lab([l, a, b]: [f32; 3]) -> [u8; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let back = |t: f32| {
        if t.powi(3) > 216.0 / 24_389.0 {
            t.powi(3)
        } else {
            (116.0 * t - 16.0) * 27.0 / 24_389.0
        }
    };
    let [x, y, z] = [back(fx), back(fy), back(fz)]
        .into_iter()
        .zip(WHITE)
        .map(|(t, white)| t * white)
        .collect::<Vec<_>>()[..]
    else {
        return [0, 0, 0];
    };
    let r = 3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z;
    let g = -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z;
    let b = 0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z;
    color::bytes([r, g, b].map(|channel| gamma(channel.max(0.0))))
}

// ----- The dialogs -----------------------------------------------------------

/// Which color the Color Settings dialog edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColorTarget {
    /// The main drawing color.
    Main,
    /// The sub drawing color.
    Sub,
    /// A cell of a color set: the set and the cell.
    Cell(usize, usize),
}

/// The Color Settings dialog: what it edits, the color as it stands, the
/// color it had when it opened, the hue kept while the color is gray, and
/// the hex code being typed.
#[derive(Clone, Debug)]
pub(crate) struct ColorSettings {
    pub target: ColorTarget,
    pub rgb: [u8; 3],
    pub before: [u8; 3],
    pub hue: f32,
    pub hex: String,
}

impl ColorSettings {
    /// The dialog for `target`, whose color is `rgb`.
    #[must_use]
    pub fn new(target: ColorTarget, rgb: [u8; 3], hue: f32) -> Self {
        Self {
            target,
            rgb,
            before: rgb,
            hue,
            hex: color::hex(rgb),
        }
    }

    /// Takes `rgb` as the color, keeping the hue while it is gray.
    fn set(&mut self, rgb: [u8; 3]) {
        let [h, s, _] = sue_wm::hsv_of(rgb);
        if s > 0.0 {
            self.hue = h;
        }
        self.rgb = rgb;
        self.hex = color::hex(rgb);
    }
}

/// One of the color dialogs.
#[derive(Clone, Debug)]
pub(crate) enum ColorDialog {
    /// Color Settings.
    Settings(ColorSettings),
    /// The name of a new set (`None`) or of the set at an index.
    SetName(Option<usize>, String),
    /// The name of a color: the set, the cell and the name typed.
    ColorName(usize, usize, String),
    /// Edit Color Sets: the set picked in its list, its name as typed,
    /// and whether Delete This Set was pressed (it closes to ask first).
    EditSets(usize, String, bool),
    /// Whether to delete the set at an index, and whether Edit Color Sets
    /// opens again after.
    DeleteSet(usize, bool),
}

impl ColorDialog {
    /// The dialog's title and its size before the screen clamps it.
    pub(super) fn title_and_size(&self) -> (String, egui::Vec2) {
        match self {
            Self::Settings(_) => (tr!("color-settings"), SETTINGS_SIZE),
            Self::SetName(None, _) => (tr!("new-color-set"), NAME_SIZE),
            Self::SetName(Some(_), _) => (tr!("rename-color-set"), NAME_SIZE),
            Self::ColorName(..) => (tr!("change-color-name"), NAME_SIZE),
            Self::EditSets(..) => (tr!("edit-color-sets"), SETS_SIZE),
            Self::DeleteSet(..) => (tr!("delete-color-set"), NAME_SIZE),
        }
    }
}

/// OK (or the label `accept`) and Cancel at the bottom right; what was
/// pressed.
fn accept_cancel(ui: &mut Ui, accept: &str, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.add_space(theme.space(Space::S4));
    crate::dialogs::button_row(ui, |ui| {
        if ui::button("sue-paint-color-accept", accept)
            .primary()
            .show(ui, theme)
            .clicked()
        {
            outcome = Outcome::Ok;
        }
        if ui::button("sue-paint-color-cancel", &tr!("cancel"))
            .show(ui, theme)
            .clicked()
        {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// A muted caption.
fn caption(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// One number of Color Settings: `name`, the field, and its unit. `true`
/// when it was changed.
fn number(
    ui: &mut Ui,
    salt: &str,
    name: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f64>,
    unit: &str,
    theme: &Theme,
) -> bool {
    let width = Density::of(ui).control_height() * FIELD_ROWS;
    let mut wide = f64::from(*value);
    let before = wide;
    ui::form_row(ui, name, theme.space(Space::S5), theme, |ui| {
        let field = ui::number_field(salt, &mut wide)
            .range(range)
            .decimals(0)
            .suffix(unit)
            .label(name)
            .width(width)
            .show(ui, theme);
        ui::tooltip(field, &ui::Tip::new(name), theme);
    });
    if (wide - before).abs() > f64::EPSILON {
        *value = wide as f32;
        true
    } else {
        false
    }
}

impl SuePaint {
    /// Opens Color Settings on `target`: a double click on a drawing
    /// color, on a swatch of a set, or the command `color.settings`.
    pub(crate) fn open_color_settings(&mut self, target: ColorTarget) {
        let rgb = match target {
            ColorTarget::Main => self.main,
            ColorTarget::Sub => self.sub,
            ColorTarget::Cell(set, cell) => {
                match self
                    .color_sets
                    .sets
                    .get(set)
                    .and_then(|set| set.cells.get(cell))
                {
                    Some(Some(swatch)) => swatch.rgb,
                    Some(None) => self.drawing_color(),
                    None => return,
                }
            }
        };
        self.dialog = Some(Dialog::Color(ColorDialog::Settings(ColorSettings::new(
            target, rgb, self.hue,
        ))));
    }

    /// Opens Color Settings on the drawing color.
    pub(crate) fn open_drawing_color_settings(&mut self) {
        self.open_color_settings(if self.sub_active {
            ColorTarget::Sub
        } else {
            ColorTarget::Main
        });
    }

    /// Asks for the name of a new set, or of the set at `index`.
    pub(crate) fn open_set_name(&mut self, index: Option<usize>) {
        let name = index
            .and_then(|index| self.color_sets.sets.get(index))
            .map_or_else(
                || self.color_sets.fresh_name(&tr!("my-colors")),
                |set| set.name.clone(),
            );
        self.dialog = Some(Dialog::Color(ColorDialog::SetName(index, name)));
    }

    /// Asks for the name of the color in the cell `cell` of the set showing.
    pub(crate) fn open_color_name(&mut self, cell: usize) {
        if let Some(Some(swatch)) = self.color_sets.cells().get(cell) {
            self.dialog = Some(Dialog::Color(ColorDialog::ColorName(
                self.color_sets.current,
                cell,
                swatch.name.clone(),
            )));
        }
    }

    /// Opens Edit Color Sets on the set showing.
    pub(crate) fn open_edit_sets(&mut self) {
        let picked = self.color_sets.current;
        let name = self
            .color_sets
            .set()
            .map(|set| set.name.clone())
            .unwrap_or_default();
        self.dialog = Some(Dialog::Color(ColorDialog::EditSets(picked, name, false)));
    }

    /// Deletes the set at `index`, asking first when it holds a color
    /// (a set cannot be brought back); `back` opens Edit Color Sets again
    /// after. The last set stays.
    pub(crate) fn ask_delete_set(&mut self, cx: &mut AppletContext<'_>, index: usize, back: bool) {
        let Some(set) = self.color_sets.sets.get(index) else {
            return;
        };
        if self.color_sets.sets.len() <= 1 {
            return;
        }
        if set.filled() > 0 {
            self.dialog = Some(Dialog::Color(ColorDialog::DeleteSet(index, back)));
            return;
        }
        if self.color_sets.delete(index) {
            self.save_color_sets(cx);
        }
        if back {
            self.open_edit_sets();
        }
    }

    /// The body of a color dialog.
    pub(super) fn color_dialog_ui(
        &mut self,
        ui: &mut Ui,
        dialog: &mut ColorDialog,
        theme: &Theme,
    ) -> Outcome {
        match dialog {
            ColorDialog::Settings(settings) => settings_ui(ui, settings, theme),
            ColorDialog::SetName(_, name) => name_ui(ui, "sue-paint-set-name", name, theme),
            ColorDialog::ColorName(_, _, name) => name_ui(ui, "sue-paint-color-name", name, theme),
            ColorDialog::EditSets(picked, name, deleting) => {
                self.edit_sets_ui(ui, picked, name, deleting, theme)
            }
            ColorDialog::DeleteSet(index, _) => {
                let (name, count) = self
                    .color_sets
                    .sets
                    .get(*index)
                    .map(|set| (set.name.clone(), set.filled()))
                    .unwrap_or_default();
                delete_ui(ui, &name, count, theme)
            }
        }
    }

    /// What OK does in a color dialog.
    pub(super) fn color_dialog_ok(&mut self, cx: &mut AppletContext<'_>, dialog: ColorDialog) {
        match dialog {
            ColorDialog::Settings(settings) => self.color_settings_ok(cx, &settings),
            ColorDialog::SetName(None, name) => {
                let name = if name.trim().is_empty() {
                    tr!("my-colors")
                } else {
                    name
                };
                self.color_sets.new_set(&name);
                self.save_color_sets(cx);
            }
            ColorDialog::SetName(Some(index), name) => {
                if self.color_sets.rename(index, &name) {
                    self.save_color_sets(cx);
                }
            }
            ColorDialog::ColorName(set, cell, name) => {
                let showing = self.color_sets.current;
                self.color_sets.current = set;
                let named = self.color_sets.name_color(cell, &name);
                self.color_sets.current = showing;
                if named {
                    self.save_color_sets(cx);
                }
            }
            ColorDialog::EditSets(picked, name, deleting) => {
                self.color_sets.rename(picked, &name);
                self.save_color_sets(cx);
                if deleting {
                    self.ask_delete_set(cx, picked, true);
                }
            }
            ColorDialog::DeleteSet(index, back) => {
                if self.color_sets.delete(index) {
                    self.save_color_sets(cx);
                }
                if back {
                    self.open_edit_sets();
                }
            }
        }
    }

    /// What Cancel does in a color dialog: Edit Color Sets changes the
    /// sets as it goes, so it keeps them whichever way it closes.
    pub(super) fn color_dialog_cancel(&mut self, cx: &mut AppletContext<'_>, dialog: ColorDialog) {
        match dialog {
            ColorDialog::EditSets(..) => self.save_color_sets(cx),
            // Keeping the set goes back to where Delete was pressed.
            ColorDialog::DeleteSet(index, true) => {
                self.color_sets.current = index.min(self.color_sets.sets.len().saturating_sub(1));
                self.open_edit_sets();
            }
            _ => {}
        }
    }

    /// Color Settings' OK: the color goes where the dialog was opened from.
    fn color_settings_ok(&mut self, cx: &mut AppletContext<'_>, settings: &ColorSettings) {
        let rgb = settings.rgb;
        match settings.target {
            ColorTarget::Main | ColorTarget::Sub => {
                let sub = settings.target == ColorTarget::Sub;
                if sub == self.sub_active {
                    self.set_drawing_color(rgb);
                    self.transparent = false;
                } else if sub {
                    self.sub = rgb;
                } else {
                    self.main = rgb;
                }
            }
            ColorTarget::Cell(set, cell) => {
                let showing = self.color_sets.current;
                self.color_sets.current = set;
                let put = self.color_sets.put(cell, rgb);
                if showing != set {
                    self.color_sets.current = showing;
                    self.color_sets.selected = None;
                }
                if put {
                    self.save_color_sets(cx);
                }
            }
        }
    }

    /// Edit Color Sets: the list (drag a row by its grip, or Alt and the
    /// arrow keys, to move it) beside what can be done to the set picked.
    fn edit_sets_ui(
        &mut self,
        ui: &mut Ui,
        picked: &mut usize,
        name: &mut String,
        deleting: &mut bool,
        theme: &Theme,
    ) -> Outcome {
        let sets = &mut self.color_sets;
        let last = sets.sets.len().saturating_sub(1);
        *picked = (*picked).min(last);
        let density = Density::of(ui);
        let gap = theme.space(Space::S5);
        // The room the dialog shows: its body scrolls, so what is left of
        // the view (not of the endless scrolled content) is what fits.
        let view = ui.clip_rect();
        let room = vec2(
            view.right() - ui.cursor().left(),
            view.bottom() - ui.cursor().top(),
        );
        let buttons = density.control_height() * BUTTON_ROWS + gap;
        ui.set_max_width(room.x);
        let list_width = (room.x - buttons - gap).max(buttons);
        let footer = density.control_height() + theme.space(Space::S6);
        let list_height = (room.y - footer).max(density.row());
        let mut chosen = None;
        let mut moved = None;
        ui.horizontal_top(|ui| {
            // The list.
            let (area, _) = ui.allocate_exact_size(vec2(list_width, list_height), Sense::hover());
            ui.painter()
                .rect_filled(area, theme.radius(ui::Radius::Md), theme.surface(1));
            ui.painter().rect_stroke(
                area,
                theme.radius(ui::Radius::Md),
                theme.hairline(),
                egui::StrokeKind::Inside,
            );
            let mut list = ui.new_child(
                UiBuilder::new()
                    .max_rect(area)
                    .layout(Layout::top_down(Align::Min)),
            );
            ui::ScrollView::vertical("sue-paint-edit-sets").show(&mut list, theme, |ui| {
                let mut rows: Vec<Rect> = Vec::with_capacity(sets.sets.len());
                let drag_id = Id::unique("sue-paint-edit-sets-drag");
                let dragging: Option<usize> = ui.data(|data| data.get_temp(drag_id));
                for (index, set) in sets.sets.iter().enumerate() {
                    let count = set.filled().to_string();
                    let row = ui::list_row(("sue-paint-set-row", index), &set.name)
                        .icon(sue_wm::icons::Icon::Grip)
                        .note(&count)
                        .selected(index == *picked)
                        .show(ui, theme);
                    if row.response.clicked() {
                        chosen = Some(index);
                    }
                    let rect = row.response.rect;
                    rows.push(rect);
                    // The grip, at the row's start, drags the row.
                    let grip =
                        Rect::from_min_size(rect.min, vec2(density.icon_button(), rect.height()));
                    let held = ui.interact(
                        grip,
                        Id::unique(("sue-paint-set-grip", index)),
                        Sense::drag(),
                    );
                    let held =
                        ui::tooltip(held, &ui::Tip::new(&tr!("drag-to-reorder-sets")), theme);
                    if held.drag_started() {
                        ui.data_mut(|data| data.insert_temp(drag_id, index));
                        chosen = Some(index);
                    }
                }
                if let Some(from) = dragging {
                    let pointer = ui.input(|input| input.pointer.interact_pos());
                    let to = pointer.map(|at| {
                        rows.iter()
                            .position(|rect| at.y < rect.center().y)
                            .unwrap_or(rows.len())
                    });
                    if let Some(to) = to {
                        // The line where it would go.
                        let y = rows
                            .get(to)
                            .map_or_else(|| rows.last().map_or(0.0, |r| r.bottom()), Rect::top);
                        if let Some(first) = rows.first() {
                            ui.painter().hline(first.x_range(), y, theme.focus_stroke());
                        }
                    }
                    if ui.input(|input| input.pointer.any_released()) {
                        ui.data_mut(|data| data.remove::<usize>(drag_id));
                        if let Some(to) = to {
                            let to = if to > from { to - 1 } else { to };
                            moved = Some((from, to.min(last)));
                        }
                    }
                }
            });
            ui.add_space(gap);
            // What can be done to the set picked.
            ui.vertical(|ui| {
                ui.set_width(buttons - gap);
                caption(ui, &tr!("color-set-name"), theme);
                let field = ui::text_field("sue-paint-edit-set-name", name)
                    .label(&tr!("color-set-name"))
                    .width(buttons - gap)
                    .show(ui, theme);
                let field = ui::tooltip(field, &ui::Tip::new(&tr!("color-set-name")), theme);
                if field.lost_focus() && sets.rename(*picked, name) {
                    sets.sets[*picked].name.clone_into(name);
                }
                ui.add_space(theme.space(Space::S4));
                if ui::button("sue-paint-set-new", &tr!("new-color-set"))
                    .icon(sue_wm::icons::Icon::Plus)
                    .full_width()
                    .show(ui, theme)
                    .clicked()
                {
                    let index = sets.new_set(&tr!("my-colors"));
                    chosen = Some(index);
                }
                let mut adding = ui.data(|data| {
                    data.get_temp::<bool>(Id::unique("sue-paint-add-standard"))
                        .unwrap_or(false)
                });
                let add = ui::button("sue-paint-set-standard", &tr!("add-standard-set"))
                    .icon(sue_wm::icons::Icon::PaletteColorSet)
                    .full_width()
                    .show(ui, theme);
                if add.clicked() {
                    adding = !adding;
                }
                let standard = ui::popover(
                    ui.ctx(),
                    Id::unique("sue-paint-add-standard-menu"),
                    add.rect,
                    &mut adding,
                    theme,
                    |ui| {
                        let mut picked = None;
                        for standard in Standard::ALL {
                            if ui::button(("sue-paint-standard", standard.name()), &standard.name())
                                .ghost()
                                .full_width()
                                .show(ui, theme)
                                .clicked()
                            {
                                picked = Some(standard);
                            }
                        }
                        picked
                    },
                )
                .flatten();
                if let Some(standard) = standard {
                    chosen = Some(sets.add_standard(standard));
                    adding = false;
                }
                ui.data_mut(|data| data.insert_temp(Id::unique("sue-paint-add-standard"), adding));
                if ui::button("sue-paint-set-duplicate", &tr!("duplicate-this-set"))
                    .icon(sue_wm::icons::Icon::Duplicate)
                    .full_width()
                    .show(ui, theme)
                    .clicked()
                {
                    chosen = sets.duplicate(*picked);
                }
                if ui::button("sue-paint-set-delete", &tr!("delete-this-set"))
                    .icon(sue_wm::icons::Icon::Delete)
                    .danger()
                    .enabled(sets.sets.len() > 1)
                    .full_width()
                    .show(ui, theme)
                    .clicked()
                {
                    // It closes, asks, and opens again (see `color_dialog_ok`).
                    *deleting = true;
                }
                ui.add_space(theme.space(Space::S4));
                caption(ui, &tr!("edit-color-sets-hint"), theme);
            });
        });
        // Alt and Up or Down move the set picked.
        let (up, down) = ui.input_mut(|input| {
            (
                input.consume_key(Modifiers::ALT, Key::ArrowUp),
                input.consume_key(Modifiers::ALT, Key::ArrowDown),
            )
        });
        if up && *picked > 0 {
            moved = Some((*picked, *picked - 1));
        } else if down && *picked < last {
            moved = Some((*picked, *picked + 1));
        }
        if let Some((from, to)) = moved {
            // The name typed so far goes to its set before the sets move,
            // and the set picked stays picked wherever it lands.
            sets.rename(*picked, name);
            if sets.move_set(from, to) {
                *picked = moved_index(*picked, from, to);
                chosen = Some(to);
            }
        }
        if let Some(index) = chosen {
            // The name typed so far goes to the set it was typed for.
            if index != *picked {
                sets.rename(*picked, name);
            }
            *picked = index.min(sets.sets.len().saturating_sub(1));
            sets.current = *picked;
            sets.selected = None;
            if let Some(set) = sets.sets.get(*picked) {
                set.name.clone_into(name);
            }
        }
        let mut outcome = if *deleting {
            Outcome::Ok
        } else {
            Outcome::Open
        };
        crate::dialogs::button_row(ui, |ui| {
            if ui::button("sue-paint-edit-sets-close", &tr!("close"))
                .primary()
                .show(ui, theme)
                .clicked()
            {
                outcome = Outcome::Ok;
            }
        });
        outcome
    }
}

/// Where the item at `index` is after the item at `from` moved to `to`.
fn moved_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < index && to >= index {
        index - 1
    } else if from > index && to <= index {
        index + 1
    } else {
        index
    }
}

/// Asks whether to delete the set `name` of `count` colors: Delete, or
/// Cancel to keep it.
fn delete_ui(ui: &mut Ui, name: &str, count: usize, theme: &Theme) -> Outcome {
    ui.label(
        egui::RichText::new(tr!("delete-color-set-ask", name = name, count = count))
            .font(theme.text_font(TextRole::Body))
            .color(theme.text),
    );
    let mut outcome = Outcome::Open;
    ui.add_space(theme.space(Space::S4));
    crate::dialogs::button_row(ui, |ui| {
        if ui::button("sue-paint-delete-set-accept", &tr!("delete"))
            .danger()
            .show(ui, theme)
            .clicked()
        {
            outcome = Outcome::Ok;
        }
        if ui::button("sue-paint-delete-set-cancel", &tr!("cancel"))
            .show(ui, theme)
            .clicked()
        {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// A dialog that asks for a name.
fn name_ui(ui: &mut Ui, salt: &str, name: &mut String, theme: &Theme) -> Outcome {
    let field = ui::text_field(salt, name)
        .label(&tr!("color-set-name"))
        .show(ui, theme);
    let field = ui::tooltip(field, &ui::Tip::new(&tr!("color-set-name")), theme);
    // The field has the keyboard from the start.
    if !field.has_focus() && ui.memory(|memory| memory.focused().is_none()) {
        field.request_focus();
    }
    // Enter in the field is OK (the field gives up the keyboard as it
    // takes the key, so either may be seen this frame).
    let entered = (field.lost_focus() || field.has_focus())
        && ui.input(|input| input.key_pressed(Key::Enter));
    let outcome = accept_cancel(ui, &tr!("ok"), theme);
    if entered { Outcome::Ok } else { outcome }
}

/// Color Settings: the wheel, the color before and now, the hex code, and
/// the color as numbers in four models.
fn settings_ui(ui: &mut Ui, settings: &mut ColorSettings, theme: &Theme) -> Outcome {
    let density = Density::of(ui);
    let side = density.control_height() * WHEEL_ROWS;
    let gap = theme.space(Space::S6);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        // The wheel, the two swatches and the hex code.
        ui.vertical(|ui| {
            ui.set_width(side);
            let (area, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
            let mut wheel = ui.new_child(UiBuilder::new().max_rect(area));
            let [r, g, b] = settings.rgb;
            let mut rgba = [r, g, b, u8::MAX];
            let pick = sue_wm::color_wheel(
                &mut wheel,
                Id::unique("sue-paint-color-settings-wheel"),
                &mut rgba,
                side,
            );
            if pick.changed {
                settings.set([rgba[0], rgba[1], rgba[2]]);
            }
            ui.add_space(theme.space(Space::S4));
            // Now beside before; a click on before takes it back.
            let half = (side - theme.space(Space::S2)) / 2.0;
            let tall = density.control_height();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.space(Space::S2);
                for (salt, rgb, name) in [
                    ("now", settings.rgb, tr!("color-new")),
                    ("before", settings.before, tr!("color-current")),
                ] {
                    ui.vertical(|ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(half, tall), Sense::click());
                        let [r, g, b] = rgb;
                        sue_wm::swatch(ui, rect, [r, g, b, u8::MAX], theme);
                        ui::focus_ring(ui, &response, theme.radius(ui::Radius::Xs), theme);
                        let help = if salt == "before" {
                            tr!("color-current-help")
                        } else {
                            color::hex(rgb)
                        };
                        let response =
                            ui::tooltip(response, &ui::Tip::new(&name).help(&help), theme);
                        if salt == "before" && response.clicked() {
                            settings.set(settings.before);
                        }
                        caption(ui, &name, theme);
                    });
                }
            });
            ui.add_space(theme.space(Space::S4));
            ui::form_row(ui, &tr!("hex"), theme.space(Space::S7), theme, |ui| {
                let field = ui::text_field("sue-paint-color-settings-hex", &mut settings.hex)
                    .label(&tr!("hex"))
                    .width(side - theme.space(Space::S7) - theme.label_gap())
                    .show(ui, theme);
                let field = ui::tooltip(field, &ui::Tip::new(&tr!("hex")), theme);
                if field.changed()
                    && let Some(rgb) = color::parse_hex(&settings.hex)
                {
                    let hex = settings.hex.clone();
                    settings.set(rgb);
                    settings.hex = hex;
                }
                if field.lost_focus() {
                    settings.hex = color::hex(settings.rgb);
                }
            });
        });
        // The numbers.
        let rgb = settings.rgb;
        let [_, s, v] = sue_wm::hsv_of(rgb);
        let mut hsv = [settings.hue, s * 100.0, v * 100.0];
        let mut channels = rgb.map(f32::from);
        let mut cmyk = cmyk_of(rgb).map(|ink| ink * 100.0);
        let mut lab = lab_of(rgb);
        let mut next = None;
        ui.vertical(|ui| {
            caption(ui, &tr!("color-mode-hsv"), theme);
            let names = [tr!("color-h"), tr!("color-s"), tr!("color-v")];
            let mut changed = false;
            for (index, name) in names.iter().enumerate() {
                let (range, unit) = if index == 0 {
                    (0.0..=359.0, "\u{b0}")
                } else {
                    (0.0..=100.0, "%")
                };
                changed |= number(
                    ui,
                    &format!("sue-paint-hsv-{index}"),
                    name,
                    &mut hsv[index],
                    range,
                    unit,
                    theme,
                );
            }
            if changed {
                settings.hue = hsv[0];
                next = Some(sue_wm::rgb_of([hsv[0], hsv[1] / 100.0, hsv[2] / 100.0]));
            }
            ui.add_space(theme.space(Space::S4));
            caption(ui, &tr!("color-mode-rgb"), theme);
            let names = [tr!("color-r"), tr!("color-g"), tr!("color-b")];
            let mut changed = false;
            for (index, name) in names.iter().enumerate() {
                changed |= number(
                    ui,
                    &format!("sue-paint-rgb-{index}"),
                    name,
                    &mut channels[index],
                    0.0..=255.0,
                    "",
                    theme,
                );
            }
            if changed {
                next = Some(channels.map(|value| value.round().clamp(0.0, 255.0) as u8));
            }
        });
        ui.vertical(|ui| {
            caption(ui, &tr!("color-mode-cmyk"), theme);
            let names = [
                tr!("color-c"),
                tr!("color-m"),
                tr!("color-y"),
                tr!("color-k"),
            ];
            let mut changed = false;
            for (index, name) in names.iter().enumerate() {
                changed |= number(
                    ui,
                    &format!("sue-paint-cmyk-{index}"),
                    name,
                    &mut cmyk[index],
                    0.0..=100.0,
                    "%",
                    theme,
                );
            }
            if changed {
                next = Some(rgb_of_cmyk(cmyk.map(|ink| ink / 100.0)));
            }
            ui.add_space(theme.space(Space::S4));
            caption(ui, &tr!("color-mode-lab"), theme);
            let names = [tr!("color-lab-l"), tr!("color-lab-a"), tr!("color-lab-b")];
            let mut changed = false;
            for (index, name) in names.iter().enumerate() {
                let range = if index == 0 {
                    0.0..=100.0
                } else {
                    -128.0..=127.0
                };
                changed |= number(
                    ui,
                    &format!("sue-paint-lab-{index}"),
                    name,
                    &mut lab[index],
                    range,
                    "",
                    theme,
                );
            }
            if changed {
                next = Some(rgb_of_lab(lab));
            }
        });
        if let Some(rgb) = next {
            let hue = settings.hue;
            settings.set(rgb);
            // A hue typed in stays, even where the color it gives is gray.
            if sue_wm::hsv_of(rgb)[1] <= 0.0 {
                settings.hue = hue;
            }
        }
    });
    accept_cancel(ui, &tr!("ok"), theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmyk_round_trips_and_keeps_gray_in_black() {
        assert_eq!(cmyk_of([0, 0, 0]), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(cmyk_of([255, 255, 255]), [0.0, 0.0, 0.0, 0.0]);
        let [c, m, y, k] = cmyk_of([255, 0, 0]);
        assert_eq!([c, m, y, k], [0.0, 1.0, 1.0, 0.0]);
        let [c, m, y, _] = cmyk_of([128, 128, 128]);
        assert_eq!([c, m, y], [0.0, 0.0, 0.0], "a gray is black ink only");
        for rgb in [[12, 200, 99], [255, 128, 0], [1, 2, 3], [250, 250, 251]] {
            assert_eq!(rgb_of_cmyk(cmyk_of(rgb)), rgb);
        }
    }

    #[test]
    fn lab_matches_known_values_and_round_trips() {
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.05);
        assert!(close(lab_of([255, 255, 255]), [100.0, 0.0, 0.0]));
        assert!(close(lab_of([0, 0, 0]), [0.0, 0.0, 0.0]));
        // sRGB red under D65.
        assert!(
            close(lab_of([255, 0, 0]), [53.24, 80.09, 67.20]),
            "{:?}",
            lab_of([255, 0, 0])
        );
        for rgb in [[12, 200, 99], [255, 128, 0], [1, 2, 3], [90, 30, 200]] {
            assert_eq!(rgb_of_lab(lab_of(rgb)), rgb);
        }
        // Out of sRGB, clipped rather than wrapped.
        assert_eq!(rgb_of_lab([50.0, 127.0, -128.0])[1], 0);
    }

    #[test]
    fn a_picked_set_is_followed_through_a_move() {
        let mut order = vec![0, 1, 2, 3, 4];
        for (from, to) in [(4, 0), (0, 3), (2, 1), (1, 1)] {
            for index in 0..order.len() {
                let item = order[index];
                let mut moved = order.clone();
                let one = moved.remove(from);
                moved.insert(to, one);
                assert_eq!(moved[moved_index(index, from, to)], item);
            }
            let one = order.remove(from);
            order.insert(to, one);
        }
    }
}
