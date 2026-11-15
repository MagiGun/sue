//! The Auto Action palette: menu commands recorded and played back.

use egui::{Rect, Ui};
use sue_wm::{AppletContext, Theme};

use crate::extras::{sections, write_sections};
use crate::studio::SuePaint;

/// A named list of menu commands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AutoAction {
    pub name: String,
    pub steps: Vec<String>,
}

/// The auto actions and what is being recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AutoActions {
    pub sets: Vec<AutoAction>,
    pub selected: usize,
    /// The action commands are being recorded into.
    pub recording: Option<usize>,
    pub loaded: bool,
}

impl AutoActions {
    /// Reads them from `text`.
    #[must_use]
    pub fn parse(text: &str) -> Vec<AutoAction> {
        sections(text)
            .into_iter()
            .map(|(name, steps)| AutoAction { name, steps })
            .collect()
    }

    /// Them as text.
    #[must_use]
    pub fn write(sets: &[AutoAction]) -> String {
        write_sections(
            sets.iter()
                .map(|set| (set.name.as_str(), set.steps.clone())),
        )
    }

    /// The actions a new install starts with.
    #[must_use]
    pub fn builtin() -> Vec<AutoAction> {
        vec![
            AutoAction {
                name: tr!("flip-canvas-and-fit"),
                steps: vec!["canvas.flip_h".to_owned(), "view.fit".to_owned()],
            },
            AutoAction {
                name: tr!("rotate-canvas-right"),
                steps: vec!["canvas.rotate_right".to_owned(), "view.fit".to_owned()],
            },
        ]
    }
}

impl SuePaint {
    /// Plays auto action `index`: each of its commands in turn.
    pub(crate) fn play_auto(&mut self, index: usize, cx: &mut AppletContext<'_>) {
        let Some(steps) = self.auto.sets.get(index).map(|set| set.steps.clone()) else {
            return;
        };
        let recording = self.auto.recording.take();
        for step in steps {
            self.run(&step, cx);
        }
        self.auto.recording = recording;
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

    /// The Auto Actions tab.
    pub(crate) fn auto_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(6.0)));
        let mut changed = false;
        let mut play = None;
        inner.horizontal(|ui| {
            let recording = self.auto.recording.is_some();
            if ui
                .button(if recording {
                    tr!("stop")
                } else {
                    tr!("record")
                })
                .clicked()
            {
                self.auto.recording = if recording {
                    changed = true;
                    None
                } else {
                    self.auto
                        .sets
                        .get(self.auto.selected)
                        .map(|_| self.auto.selected)
                };
            }
            if ui.button(tr!("play")).clicked() {
                play = Some(self.auto.selected);
            }
            if ui.button(tr!("new-2")).clicked() {
                let name = tr!("auto-action", value = self.auto.sets.len() + 1);
                self.auto.sets.push(AutoAction {
                    name,
                    steps: Vec::new(),
                });
                self.auto.selected = self.auto.sets.len() - 1;
                self.auto.recording = Some(self.auto.selected);
                changed = true;
            }
            if ui.button(tr!("delete")).clicked() && self.auto.selected < self.auto.sets.len() {
                self.auto.sets.remove(self.auto.selected);
                self.auto.selected = self.auto.selected.saturating_sub(1);
                self.auto.recording = None;
                changed = true;
            }
        });
        if self.auto.recording.is_some() {
            inner.colored_label(theme.text, tr!("recording-run-commands-menus"));
        }
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-auto")
            .show(&mut inner, |ui| {
                for (index, set) in self.auto.sets.iter_mut().enumerate() {
                    let chosen = index == self.auto.selected;
                    let response = ui.selectable_label(
                        chosen,
                        tr!("steps", name = set.name, count = set.steps.len()),
                    );
                    if response.clicked() {
                        self.auto.selected = index;
                    }
                    if response.double_clicked() {
                        play = Some(index);
                    }
                    if chosen {
                        ui.indent("steps", |ui| {
                            if ui.text_edit_singleline(&mut set.name).lost_focus() {
                                changed = true;
                            }
                            let mut remove = None;
                            for (step, command) in set.steps.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let label = crate::menus::label_of(&self.menus, command)
                                        .unwrap_or(command);
                                    ui.label(label);
                                    if ui.small_button("x").clicked() {
                                        remove = Some(step);
                                    }
                                });
                            }
                            if let Some(step) = remove {
                                set.steps.remove(step);
                                changed = true;
                            }
                        });
                    }
                }
            });
        if let Some(index) = play {
            self.play_auto(index, cx);
        }
        if changed {
            self.save_extras(cx);
        }
    }
}
