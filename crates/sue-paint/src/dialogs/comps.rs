//! Layer Comps.

use egui::{RichText, Ui};
use sue_wm::{AppletContext, Theme};

use super::{Outcome, label};
use crate::studio::SuePaint;

/// The Layer Comps dialog: the comp chosen and the name being typed.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CompsDialog {
    pub chosen: usize,
    pub name: String,
}

impl SuePaint {
    /// Layer Comps: the picture's comps to choose, show, update, rename,
    /// delete or export, and a new one from how the layers show now. Every
    /// change is one undo step.
    pub(super) fn comps_ui(
        &mut self,
        ui: &mut Ui,
        dialog: &mut CompsDialog,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Outcome {
        let mut outcome = Outcome::Open;
        let Some(picture) = self.picture() else {
            return Outcome::Cancel;
        };
        let mut comps = crate::comps::read(&cx.world.arena, picture);
        dialog.chosen = dialog.chosen.min(comps.len().saturating_sub(1));
        egui::ScrollArea::vertical()
            .max_height(170.0)
            .show(ui, |ui| {
                if comps.is_empty() {
                    ui.label(
                        RichText::new(tr!("no-layer-comps-yet"))
                            .size(theme.small_size)
                            .color(theme.muted),
                    );
                }
                for (index, comp) in comps.iter().enumerate() {
                    let response = ui.selectable_label(dialog.chosen == index, &comp.name);
                    if response.clicked() {
                        dialog.chosen = index;
                        dialog.name.clone_from(&comp.name);
                    }
                    // A double-click shows it.
                    if response.double_clicked() {
                        let edit = crate::comps::apply(&cx.world.arena, picture, comp, &comp.name);
                        Self::apply(cx, edit);
                        self.refresh_comp(cx);
                    }
                }
            });
        ui.separator();
        ui.horizontal(|ui| {
            label(ui, &tr!("name"), theme);
            ui.text_edit_singleline(&mut dialog.name);
        });
        let name = if dialog.name.trim().is_empty() {
            tr!("layer-comp-number", number = comps.len() + 1)
        } else {
            dialog.name.trim().to_owned()
        };
        let chosen = comps.get(dialog.chosen).cloned();
        let mut store = None;
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr!("new-layer-comp")).clicked() {
                comps.push(crate::comps::capture(&cx.world.arena, picture, &name));
                dialog.chosen = comps.len() - 1;
                store = Some(tr!("new-layer-comp"));
            }
            ui.add_enabled_ui(chosen.is_some(), |ui| {
                if ui.button(tr!("show-layer-comp")).clicked()
                    && let Some(comp) = &chosen
                {
                    let edit = crate::comps::apply(&cx.world.arena, picture, comp, &comp.name);
                    Self::apply(cx, edit);
                    self.refresh_comp(cx);
                }
                if ui.button(tr!("update-layer-comp")).clicked()
                    && let Some(comp) = comps.get_mut(dialog.chosen)
                {
                    *comp = crate::comps::capture(&cx.world.arena, picture, &comp.name);
                    store = Some(tr!("update-layer-comp"));
                }
                if ui.button(tr!("rename")).clicked()
                    && let Some(comp) = comps.get_mut(dialog.chosen)
                {
                    comp.name.clone_from(&name);
                    store = Some(tr!("rename"));
                }
                if ui.button(tr!("delete")).clicked() && dialog.chosen < comps.len() {
                    comps.remove(dialog.chosen);
                    store = Some(tr!("delete-layer-comp"));
                }
            });
        });
        if let Some(label) = store {
            Self::apply(cx, crate::comps::store(picture, &comps, &label));
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !comps.is_empty(),
                    egui::Button::new(tr!("export-layer-comps-ellipsis")),
                )
                .clicked()
            {
                self.run("file.export.layer_comps", cx);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr!("close")).clicked() {
                    outcome = Outcome::Cancel;
                }
            });
        });
        outcome
    }

    /// Shows the layers as the comp just applied set them.
    fn refresh_comp(&mut self, cx: &AppletContext<'_>) {
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }
}
