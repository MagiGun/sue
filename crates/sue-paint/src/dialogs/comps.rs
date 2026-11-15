//! Layer Comps.

use egui::Ui;
use sue_wm::ui::{ListRow, Space, button, empty_state, list_row, text_field};
use sue_wm::{AppletContext, Theme};

use super::{Outcome, bare, tip};
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
        let Some(picture) = self.picture() else {
            return Outcome::Cancel;
        };
        let mut comps = crate::comps::read(&cx.world.arena, picture);
        dialog.chosen = dialog.chosen.min(comps.len().saturating_sub(1));
        let rows = ListRow::height(theme, sue_wm::ui::Density::of(ui), false);
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-comps-list")
            .max_height(rows * 6.0)
            .show(ui, |ui| {
                if comps.is_empty() {
                    empty_state("sue-paint-comps-empty", &tr!("no-layer-comps-yet"))
                        .show(ui, theme);
                }
                for (index, comp) in comps.iter().enumerate() {
                    let row = list_row(("sue-paint-comp", index), &comp.name)
                        .selected(dialog.chosen == index)
                        .show(ui, theme);
                    if row.response.clicked() {
                        dialog.chosen = index;
                        dialog.name.clone_from(&comp.name);
                    }
                    // A double-click shows it.
                    if row.response.double_clicked() {
                        let edit = crate::comps::apply(&cx.world.arena, picture, comp, &comp.name);
                        Self::apply(cx, edit);
                        self.refresh_comp(cx);
                    }
                }
            });
        let name_label = bare(tr!("name"));
        text_field("sue-paint-comp-name", &mut dialog.name)
            .label(&name_label)
            .hint(&name_label)
            .show(ui, theme);
        let name = if dialog.name.trim().is_empty() {
            tr!("layer-comp-number", number = comps.len() + 1)
        } else {
            dialog.name.trim().to_owned()
        };
        let chosen = comps.get(dialog.chosen).cloned();
        let mut store = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = theme.space(Space::S4);
            let label = tr!("new-layer-comp");
            let pressed = button("sue-paint-comp-new", &label).show(ui, theme);
            if tip(pressed, &label, "", theme).clicked() {
                comps.push(crate::comps::capture(&cx.world.arena, picture, &name));
                dialog.chosen = comps.len() - 1;
                store = Some(label);
            }
            let some = chosen.is_some();
            let label = tr!("show-layer-comp");
            let pressed = button("sue-paint-comp-show", &label)
                .enabled(some)
                .show(ui, theme);
            if tip(pressed, &label, "", theme).clicked()
                && let Some(comp) = &chosen
            {
                let edit = crate::comps::apply(&cx.world.arena, picture, comp, &comp.name);
                Self::apply(cx, edit);
                self.refresh_comp(cx);
            }
            let label = tr!("update-layer-comp");
            let pressed = button("sue-paint-comp-update", &label)
                .enabled(some)
                .show(ui, theme);
            if tip(pressed, &label, "", theme).clicked()
                && let Some(comp) = comps.get_mut(dialog.chosen)
            {
                *comp = crate::comps::capture(&cx.world.arena, picture, &comp.name);
                store = Some(label);
            }
            let label = tr!("rename");
            let pressed = button("sue-paint-comp-rename", &label)
                .enabled(some)
                .show(ui, theme);
            if tip(pressed, &label, "", theme).clicked()
                && let Some(comp) = comps.get_mut(dialog.chosen)
            {
                comp.name.clone_from(&name);
                store = Some(label);
            }
            let label = tr!("delete");
            let pressed = button("sue-paint-comp-delete", &label)
                .enabled(some)
                .show(ui, theme);
            if tip(pressed, &label, "", theme).clicked() && dialog.chosen < comps.len() {
                comps.remove(dialog.chosen);
                store = Some(tr!("delete-layer-comp"));
            }
        });
        if let Some(label) = store {
            Self::apply(cx, crate::comps::store(picture, &comps, &label));
        }
        let label = tr!("export-layer-comps-ellipsis");
        let pressed = button("sue-paint-comp-export", &label)
            .enabled(!comps.is_empty())
            .show(ui, theme);
        if tip(pressed, &label, "", theme).clicked() {
            self.run("file.export.layer_comps", cx);
        }
        Outcome::Open
    }

    /// Shows the layers as the comp just applied set them.
    fn refresh_comp(&mut self, cx: &AppletContext<'_>) {
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }
}
