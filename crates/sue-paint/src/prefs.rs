//! File, Preferences: Sue Paint's own preferences, laid out as Clip Studio
//! Paint's are: its pages down the left, each page's options in framed
//! groups, OK and Cancel at the top right.
//!
//! An option Sue Paint honors is a setting in the rice (so it is in Sue's
//! Settings too), read when the dialog opens and kept on OK. An option it
//! does not honor yet shows Clip Studio's value, greyed out, so nobody
//! takes it for working.

use egui::{Align, Color32, Layout, RichText, Sense, Stroke, Ui, vec2};
use sue_rice::{RiceSchema, RiceValue};
use sue_wm::Theme;

mod table {
    include!("prefs_table.rs");
}

/// Where an option is kept.
#[derive(Clone, Copy, Debug)]
enum Wire {
    /// Not in Sue Paint yet: shown greyed out.
    None,
    /// A word setting.
    Text(&'static str),
    /// An on or off setting.
    Bool(&'static str),
    /// A number setting, shown divided by this (seconds as minutes).
    Int(&'static str, i64),
    /// A word setting that is the first word when on, the second when off.
    Flag(&'static str, &'static str, &'static str),
}

impl Wire {
    fn key(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Text(key) | Self::Bool(key) | Self::Int(key, _) | Self::Flag(key, ..) => {
                Some(key)
            }
        }
    }
}

/// One option of a group.
#[derive(Clone, Copy, Debug)]
enum Item {
    Check {
        label: &'static str,
        on: bool,
        wire: Wire,
    },
    Number {
        label: &'static str,
        value: f64,
        min: f64,
        max: f64,
        decimals: usize,
        unit: &'static str,
        wire: Wire,
    },
    /// A drop-down: each choice its value and label id.
    Choice {
        label: &'static str,
        choices: &'static [(&'static str, &'static str)],
        value: &'static str,
        wire: Wire,
    },
    /// Round buttons side by side.
    Radio {
        label: &'static str,
        choices: &'static [(&'static str, &'static str)],
        value: &'static str,
        wire: Wire,
    },
    Color {
        label: &'static str,
        rgb: [u8; 3],
    },
    /// An explanation with an i beside it.
    Note {
        text: &'static str,
    },
}

/// A framed group of options under its title.
#[derive(Clone, Copy, Debug)]
struct Group {
    title: &'static str,
    items: &'static [Item],
}

/// A page of the list.
#[derive(Clone, Copy, Debug)]
struct Page {
    title: &'static str,
    groups: &'static [Group],
}

/// The value an option has in the dialog.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Bool(bool),
    Number(f64),
    Text(String),
    None,
}

/// Every option of every page, in order.
fn items() -> impl Iterator<Item = &'static Item> {
    table::PAGES
        .iter()
        .flat_map(|page| page.groups.iter())
        .flat_map(|group| group.items.iter())
}

fn text(id: &str) -> String {
    sue_lang::text("paint", id)
}

/// The Preferences dialog: the page shown and each option's value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Preferences {
    pub page: usize,
    values: Vec<Value>,
}

impl Preferences {
    /// The preferences as the rice has them now.
    pub(crate) fn read(rice: &RiceSchema) -> Self {
        let values = items()
            .map(|item| match *item {
                Item::Check { on, wire, .. } => Value::Bool(match wire {
                    Wire::Bool(key) => rice.get_bool(key).unwrap_or(on),
                    Wire::Flag(key, yes, _) => rice.get_str(key).map_or(on, |now| now == yes),
                    _ => on,
                }),
                Item::Number {
                    value,
                    min,
                    max,
                    wire,
                    ..
                } => Value::Number(match wire {
                    Wire::Int(key, divisor) => rice
                        .get_i64(key)
                        .map_or(value, |now| now as f64 / divisor.max(1) as f64)
                        .clamp(min, max),
                    _ => value,
                }),
                Item::Choice {
                    choices,
                    value,
                    wire,
                    ..
                }
                | Item::Radio {
                    choices,
                    value,
                    wire,
                    ..
                } => Value::Text(
                    wire.key()
                        .and_then(|key| rice.get_str(key))
                        .filter(|now| choices.iter().any(|(choice, _)| choice == now))
                        .unwrap_or(value)
                        .to_owned(),
                ),
                Item::Color { .. } | Item::Note { .. } => Value::None,
            })
            .collect();
        Self { page: 0, values }
    }

    /// Each setting Sue Paint honors and the value chosen for it.
    pub(crate) fn changes(&self) -> Vec<(&'static str, RiceValue)> {
        items()
            .zip(&self.values)
            .filter_map(|(item, value)| {
                let wire = match *item {
                    Item::Check { wire, .. }
                    | Item::Number { wire, .. }
                    | Item::Choice { wire, .. }
                    | Item::Radio { wire, .. } => wire,
                    Item::Color { .. } | Item::Note { .. } => Wire::None,
                };
                Some(match (wire, value) {
                    (Wire::Bool(key), Value::Bool(on)) => (key, RiceValue::Bool(*on)),
                    (Wire::Flag(key, yes, no), Value::Bool(on)) => {
                        (key, RiceValue::Text(if *on { yes } else { no }.to_owned()))
                    }
                    (Wire::Int(key, divisor), Value::Number(number)) => (
                        key,
                        RiceValue::Int((number * divisor as f64).round() as i64),
                    ),
                    (Wire::Text(key), Value::Text(word)) => (key, RiceValue::Text(word.clone())),
                    _ => return None,
                })
            })
            .collect()
    }

    /// The dialog's body. Returns `Some(true)` for OK and `Some(false)` for
    /// Cancel.
    pub(crate) fn ui(&mut self, ui: &mut Ui, theme: &Theme) -> Option<bool> {
        let mut answer = None;
        // Three columns at set sizes, as Clip Studio's dialog has them: the
        // pages, the page, and OK and Cancel.
        let (area, _) = ui.allocate_exact_size(vec2(748.0, 450.0), Sense::hover());
        let list = egui::Rect::from_min_size(area.min, vec2(160.0, area.height()));
        let page = egui::Rect::from_min_max(
            egui::pos2(list.right() + 10.0, area.top()),
            egui::pos2(area.right() - 112.0, area.bottom()),
        );
        let buttons =
            egui::Rect::from_min_max(egui::pos2(page.right() + 12.0, area.top()), area.max);
        let column = |ui: &mut Ui, rect: egui::Rect, salt: &str| {
            let mut inner = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .id_salt(("sue-paint-prefs", salt))
                    .layout(Layout::top_down(Align::Min)),
            );
            inner.set_clip_rect(rect);
            inner
        };
        // The pages, in a sunken list.
        ui.painter().rect_filled(list, 0.0, theme.void);
        ui.painter()
            .rect_stroke(list, 0.0, theme.hairline(), egui::StrokeKind::Inside);
        let mut pages = column(ui, list.shrink(3.0), "pages");
        pages.spacing_mut().item_spacing.y = 0.0;
        for (index, item) in table::PAGES.iter().enumerate() {
            let (row, response) =
                pages.allocate_exact_size(vec2(list.width() - 6.0, 20.0), Sense::click());
            if self.page == index {
                pages.painter().rect_filled(row, 0.0, theme.active);
            } else if response.hovered() {
                pages.painter().rect_filled(row, 0.0, theme.hover);
            }
            pages.painter().text(
                row.left_center() + vec2(5.0, 0.0),
                egui::Align2::LEFT_CENTER,
                text(item.title),
                theme.font(theme.small_size + 0.5),
                theme.text,
            );
            if response.clicked() {
                self.page = index;
            }
        }
        // The page, scrolling when long.
        ui.painter().rect_filled(page, 0.0, theme.panel);
        ui.painter()
            .rect_stroke(page, 0.0, theme.hairline(), egui::StrokeKind::Inside);
        let mut body = column(ui, page.shrink(6.0), "page");
        egui::ScrollArea::vertical()
            .id_salt(("sue-paint-prefs-page", self.page))
            .auto_shrink([false, false])
            .show(&mut body, |ui| self.page_ui(ui, page.width() - 26.0, theme));
        // OK and Cancel, at the top right.
        let mut side = column(ui, buttons, "buttons");
        if side
            .add_sized(
                vec2(buttons.width(), 26.0),
                egui::Button::new("OK").fill(theme.active),
            )
            .clicked()
        {
            answer = Some(true);
        }
        side.add_space(4.0);
        if side
            .add_sized(
                vec2(buttons.width(), 26.0),
                egui::Button::new(text("cancel")),
            )
            .clicked()
        {
            answer = Some(false);
        }
        answer
    }

    /// The groups of the page shown.
    fn page_ui(&mut self, ui: &mut Ui, width: f32, theme: &Theme) {
        let first: usize = table::PAGES[..self.page]
            .iter()
            .flat_map(|page| page.groups.iter())
            .map(|group| group.items.len())
            .sum();
        let mut index = first;
        let later = tr!("not-in-sue-paint-yet");
        for group in table::PAGES[self.page].groups {
            ui.add_space(6.0);
            // A frame with its title on the top border, as Clip Studio draws
            // its groups.
            ui.label(
                RichText::new(text(group.title))
                    .size(theme.small_size + 0.5)
                    .color(theme.text),
            );
            egui::Frame::new()
                .stroke(Stroke::new(1.0, theme.border_active.gamma_multiply(0.25)))
                .corner_radius(2.0)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(width - 22.0);
                    for item in group.items {
                        let value = &mut self.values[index];
                        index += 1;
                        item_ui(ui, item, value, theme, &later);
                    }
                });
        }
    }
}

/// One option, its label to the left of its control as Clip Studio puts
/// them (a check box's label after it).
fn item_ui(ui: &mut Ui, item: &Item, value: &mut Value, theme: &Theme, later: &str) {
    let label_width = 200.0;
    let muted = |ui: &mut Ui, label: &str| {
        ui.add_sized(
            vec2(label_width, 20.0),
            egui::Label::new(RichText::new(text(label)).color(theme.text)).halign(Align::Max),
        );
    };
    let live = |wire: Wire| !matches!(wire, Wire::None);
    match (*item, value) {
        (Item::Check { label, wire, .. }, Value::Bool(on)) => {
            let response = ui.add_enabled(live(wire), egui::Checkbox::new(on, text(label)));
            if !live(wire) {
                response.on_disabled_hover_text(later);
            }
        }
        (
            Item::Number {
                label,
                min,
                max,
                decimals,
                unit,
                wire,
                ..
            },
            Value::Number(number),
        ) => {
            ui.horizontal(|ui| {
                muted(ui, label);
                let response = ui.add_enabled(
                    live(wire),
                    egui::DragValue::new(number)
                        .range(min..=max)
                        .max_decimals(decimals)
                        .min_decimals(decimals),
                );
                if !live(wire) {
                    response.on_disabled_hover_text(later);
                }
                if !unit.is_empty() {
                    ui.label(RichText::new(unit).color(theme.muted));
                }
            });
        }
        (
            Item::Choice {
                label,
                choices,
                wire,
                ..
            },
            Value::Text(now),
        ) => {
            ui.horizontal(|ui| {
                muted(ui, label);
                let name = |value: &str| {
                    choices
                        .iter()
                        .find(|(choice, _)| *choice == value)
                        .map_or_else(String::new, |(_, id)| text(id))
                };
                let response = ui.add_enabled_ui(live(wire), |ui| {
                    egui::ComboBox::from_id_salt(("sue-paint-pref", label))
                        .selected_text(name(now))
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for (choice, id) in choices {
                                if ui.selectable_label(now == choice, text(id)).clicked() {
                                    *now = (*choice).to_owned();
                                }
                            }
                        });
                });
                if !live(wire) {
                    response.response.on_disabled_hover_text(later);
                }
            });
        }
        (
            Item::Radio {
                label,
                choices,
                wire,
                ..
            },
            Value::Text(now),
        ) => {
            if !label.is_empty() {
                ui.label(RichText::new(text(label)).color(theme.muted));
            }
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(live(wire), |ui| {
                    for (choice, id) in choices {
                        if ui.radio(now == choice, text(id)).clicked() {
                            *now = (*choice).to_owned();
                        }
                        ui.add_space(12.0);
                    }
                })
                .response
                .on_disabled_hover_text(later);
            });
        }
        (Item::Color { label, rgb }, _) => {
            ui.horizontal(|ui| {
                muted(ui, label);
                let (swatch, response) = ui.allocate_exact_size(vec2(34.0, 20.0), Sense::hover());
                ui.painter().rect_filled(
                    swatch,
                    2.0,
                    Color32::from_rgb(rgb[0], rgb[1], rgb[2]).gamma_multiply(0.6),
                );
                ui.painter()
                    .rect_stroke(swatch, 2.0, theme.hairline(), egui::StrokeKind::Inside);
                response.on_hover_text(later);
            });
        }
        (Item::Note { text: id }, _) => {
            ui.horizontal(|ui| {
                let (mark, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                ui.painter()
                    .rect_filled(mark, 2.0, theme.text.gamma_multiply(0.85));
                ui.painter().text(
                    mark.center(),
                    egui::Align2::CENTER_CENTER,
                    "i",
                    theme.font(theme.ui_size + 2.0),
                    theme.panel,
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(text(id))
                            .size(theme.small_size + 0.5)
                            .color(theme.text),
                    )
                    .wrap(),
                );
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_option_sue_paint_honors_is_a_setting_it_ships() {
        let rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        let prefs = Preferences::read(&rice);
        let changes = prefs.changes();
        assert!(changes.len() >= 9, "the honored options");
        for (key, value) in &changes {
            assert_eq!(
                rice.get(key).as_ref(),
                Some(value),
                "{key} reads back as it is"
            );
        }
        // Every page and its texts are in the language database.
        assert_eq!(table::PAGES.len(), 17);
        for page in table::PAGES {
            assert_ne!(text(page.title), page.title, "{} has its words", page.title);
        }
    }
}
