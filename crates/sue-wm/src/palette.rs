//! The command palette (`Tab` or `/`): a centered floating search box that
//! filters every workspace action, mode and entity operation, fuzzily.

use egui::{Align2, Id, Key, Order, Rect, RichText, Sense, TextEdit, vec2};

use crate::{PaletteEntry, SpringDamper, SpringMode, SpringParams, Theme, widgets};

/// How well `query` matches `label`: every query character must appear in
/// order. Consecutive runs and word starts score higher. `None` if it does
/// not match.
#[must_use]
pub fn fuzzy_score(query: &str, label: &str) -> Option<i32> {
    let query: Vec<char> = query
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if query.is_empty() {
        return Some(0);
    }
    let label: Vec<char> = label.chars().flat_map(char::to_lowercase).collect();
    let mut score = 0;
    let mut at = 0;
    let mut previous: Option<usize> = None;
    for wanted in query {
        let found = (at..label.len()).find(|&index| label[index] == wanted)?;
        score += 1;
        if previous.is_some_and(|last| last + 1 == found) {
            score += 4;
        }
        if found == 0 || !label[found - 1].is_alphanumeric() {
            score += 3;
        }
        score -= i32::try_from(found - at).unwrap_or(i32::MAX).min(8) / 4;
        previous = Some(found);
        at = found + 1;
    }
    Some(score)
}

/// What the palette did this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteOutcome {
    /// Still open.
    Open,
    /// The entry at this index of the given entries was chosen.
    Run(usize),
    /// Dismissed.
    Dismissed,
}

/// An open command palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    query: String,
    selected: usize,
    appear: SpringDamper,
    closing: bool,
    focused: bool,
}

impl Palette {
    /// Opens empty.
    #[must_use]
    pub fn open(params: SpringParams, mode: SpringMode) -> Self {
        let mut appear = SpringDamper::new(params, mode, 0.0);
        appear.set_target(1.0);
        Self {
            query: String::new(),
            selected: 0,
            appear,
            closing: false,
            focused: false,
        }
    }

    /// Starts closing.
    pub fn close(&mut self) {
        self.closing = true;
        self.appear.set_target(0.0);
    }

    /// `true` once closed all the way.
    #[must_use]
    pub fn is_gone(&self) -> bool {
        self.closing && self.appear.is_settled()
    }

    /// Advances the spring. Returns `true` while it moves.
    pub fn step(&mut self, dt: f32) -> bool {
        self.appear.step(dt)
    }

    /// Types `text` into the search box (scripted runs and tests).
    pub fn set_query(&mut self, text: &str) {
        text.clone_into(&mut self.query);
        self.selected = 0;
    }

    /// The entries matching the query, best first, as indices into `entries`.
    #[must_use]
    pub fn matches(&self, entries: &[PaletteEntry]) -> Vec<usize> {
        let mut scored: Vec<(i32, usize)> = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((fuzzy_score(&self.query, &entry.label)?, index)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, index)| index).collect()
    }

    /// Draws the palette centered near the top of `area`.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        area: Rect,
        entries: &[PaletteEntry],
        theme: &Theme,
    ) -> PaletteOutcome {
        let appear = self.appear.position().clamp(0.0, 1.0);
        let width = (area.width() - 32.0).clamp(240.0, 560.0);
        let top = area.top() + area.height() * 0.14 - (1.0 - appear) * 12.0;
        let matches = self.matches(entries);
        self.selected = self.selected.min(matches.len().saturating_sub(1));
        let mut outcome = PaletteOutcome::Open;
        if !self.closing {
            let (down, up, enter, escape) = ctx.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                    input.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                    input.consume_key(egui::Modifiers::NONE, Key::Enter),
                    input.consume_key(egui::Modifiers::NONE, Key::Escape),
                )
            });
            if down && !matches.is_empty() {
                self.selected = (self.selected + 1) % matches.len();
            }
            if up && !matches.is_empty() {
                self.selected = (self.selected + matches.len() - 1) % matches.len();
            }
            if enter && let Some(index) = matches.get(self.selected) {
                outcome = PaletteOutcome::Run(*index);
            }
            if escape {
                outcome = PaletteOutcome::Dismissed;
            }
        }
        egui::Area::new(Id::unique("sue-palette"))
            .order(Order::Foreground)
            .fixed_pos(egui::pos2(area.center().x - width / 2.0, top))
            .interactable(!self.closing)
            .show(ctx, |ui| {
                ui.set_opacity(appear);
                widgets::window_frame(theme).show(ui, |ui| {
                    ui.set_width(width);
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same((theme.padding).clamp(0.0, 127.0) as i8))
                        .show(ui, |ui| {
                            let edit = TextEdit::singleline(&mut self.query)
                                .hint_text("Type a command, mode or entity action")
                                .desired_width(f32::INFINITY)
                                .frame(egui::Frame::NONE)
                                .font(theme.font(theme.ui_size * 1.15));
                            let response = ui.add(edit);
                            if !self.focused {
                                response.request_focus();
                                self.focused = true;
                            }
                            if response.changed() {
                                self.selected = 0;
                            }
                        });
                    ui.add_space(1.0);
                    let (line, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
                    ui.painter().rect_filled(line, 0.0, theme.border);
                    let row_height = theme.ui_size * 2.2;
                    for (row, index) in matches.iter().take(9).enumerate() {
                        let entry = &entries[*index];
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(width, row_height), Sense::click());
                        let chosen = row == self.selected;
                        if response.hovered() {
                            self.selected = row;
                        }
                        if chosen {
                            ui.painter().rect_filled(rect.shrink(3.0), 4.0, theme.hover);
                        }
                        ui.painter().text(
                            rect.left_center() + vec2(theme.padding * 1.5, 0.0),
                            Align2::LEFT_CENTER,
                            &entry.label,
                            theme.font(theme.ui_size),
                            if chosen { theme.text } else { theme.muted },
                        );
                        if !entry.hint.is_empty() {
                            ui.painter().text(
                                rect.right_center() - vec2(theme.padding * 1.5, 0.0),
                                Align2::RIGHT_CENTER,
                                &entry.hint,
                                theme.mono(theme.small_size),
                                theme.muted,
                            );
                        }
                        if response.clicked() {
                            outcome = PaletteOutcome::Run(*index);
                        }
                    }
                    if matches.is_empty() {
                        ui.add_space(theme.padding);
                        ui.horizontal(|ui| {
                            ui.add_space(theme.padding * 1.5);
                            ui.label(RichText::new("Nothing matches").color(theme.muted));
                        });
                    }
                    ui.add_space(theme.padding * 0.5);
                });
            });
        if !matches!(outcome, PaletteOutcome::Open) {
            self.close();
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str) -> PaletteEntry {
        PaletteEntry {
            label: label.to_owned(),
            command: label.to_owned(),
            hint: String::new(),
        }
    }

    #[test]
    fn fuzzy_matching_prefers_word_starts_and_runs() {
        assert!(fuzzy_score("tl", "Toggle Timeline").is_some());
        assert!(fuzzy_score("xyz", "Toggle Timeline").is_none());
        assert!(
            fuzzy_score("time", "Toggle Timeline").unwrap()
                > fuzzy_score("time", "Mode: Image Tools").unwrap_or(-1)
        );
        let mut palette = Palette::open(
            SpringParams {
                stiffness: 260.0,
                damping: 32.0,
                mass: 1.0,
            },
            SpringMode::InstantSnap,
        );
        let entries = [
            entry("New Document"),
            entry("New Paint Layer"),
            entry("Reset Camera"),
        ];
        palette.set_query("paint");
        assert_eq!(palette.matches(&entries), [1]);
        palette.set_query("");
        assert_eq!(palette.matches(&entries).len(), 3);
    }
}
