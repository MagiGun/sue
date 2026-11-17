//! Edit, Register Material, Image (full guide 00:45:18, FG-11, FGV-48):
//! the material's name, where it is kept, its search tags, whether it is
//! also a brush tip and a paper texture, and how it is pasted (its
//! scaling ratio, tiling and where its layer goes in the Layer palette).

use egui::Ui;
use sue_wm::Theme;
use sue_wm::ui::{dropdown, form_grid, text_field};

use super::fill::{Reads, check, number};
use super::{Outcome, bare, note, tip};
use crate::material_props::{Keep, MaterialProps, Order};

/// The Register Material dialog's form.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegisterForm {
    /// The material's name.
    pub name: String,
    /// Where it is kept.
    pub keep: Keep,
    /// The words it is found by, told apart by commas.
    pub tags: String,
    /// It is also a brush tip shape, in the brush tip picker.
    pub brush_tip: bool,
    /// How it is pasted, and whether brushes may lay it as their paper.
    pub props: MaterialProps,
}

impl RegisterForm {
    /// The form for a material named `name`, as Sue Paint suggests it.
    pub(crate) fn new(name: String) -> Self {
        Self {
            name,
            keep: Keep::Images,
            tags: String::new(),
            brush_tip: false,
            props: MaterialProps::default(),
        }
    }
}

/// The dialog's body.
pub(super) fn register_ui(ui: &mut Ui, form: &mut RegisterForm, theme: &Theme) -> Outcome {
    form_grid(ui, "sue-paint-register-material", theme, |grid| {
        let label = bare(tr!("material-name"));
        grid.row(&label, |ui| {
            let response = text_field("sue-paint-material-name", &mut form.name)
                .label(&label)
                .show(ui, theme);
            tip(response, &label, "", theme);
        });
        let label = bare(tr!("material-save-location"));
        grid.row(&label, |ui| {
            let names: Vec<String> = Keep::ALL.iter().map(|keep| keep.label()).collect();
            let mut at = Keep::ALL
                .iter()
                .position(|keep| *keep == form.keep)
                .unwrap_or(0);
            if dropdown("sue-paint-material-keep", &mut at, &names)
                .label(&label)
                .show(ui, theme)
                .changed()
            {
                form.keep = Keep::ALL[at];
            }
        });
        let label = bare(tr!("material-tags"));
        grid.row(&label, |ui| {
            let response = text_field("sue-paint-material-tags", &mut form.tags)
                .label(&label)
                .show(ui, theme);
            tip(response, &label, "", theme);
        });
        let label = bare(tr!("material-use"));
        grid.row(&label, |ui| {
            ui.vertical(|ui| {
                check(
                    ui,
                    "sue-paint-material-brush-tip",
                    &mut form.brush_tip,
                    &tr!("use-for-brush-tip-shape"),
                    &tr!("use-for-brush-tip-shape-hint"),
                    theme,
                );
                check(
                    ui,
                    "sue-paint-material-paper",
                    &mut form.props.paper,
                    &tr!("use-for-paper-texture"),
                    &tr!("use-for-paper-texture-hint"),
                    theme,
                );
            });
        });
        let label = bare(tr!("material-scaling-ratio"));
        grid.row(&label, |ui| {
            let mut scale = f64::from(form.props.scale * 100.0);
            let reads = Reads {
                range: 1.0..=1000.0,
                decimals: 0,
                suffix: " %",
            };
            if number(
                ui,
                "sue-paint-material-scale",
                &mut scale,
                &reads,
                &label,
                theme,
            ) {
                form.props.scale = (scale / 100.0) as f32;
            }
        });
        let label = bare(tr!("material-paste"));
        grid.row(&label, |ui| {
            check(
                ui,
                "sue-paint-material-tile",
                &mut form.props.tile,
                &tr!("material-tiling"),
                &tr!("material-tiling-hint"),
                theme,
            );
        });
        let label = bare(tr!("material-layer-order"));
        grid.row(&label, |ui| {
            let names: Vec<String> = Order::ALL.iter().map(|order| order.label()).collect();
            let mut at = Order::ALL
                .iter()
                .position(|order| *order == form.props.order)
                .unwrap_or(0);
            if dropdown("sue-paint-material-order", &mut at, &names)
                .label(&label)
                .show(ui, theme)
                .changed()
            {
                form.props.order = Order::ALL[at];
            }
        });
    });
    note(ui, &tr!("current-layer-goes-into-material-palette"), theme);
    Outcome::Open
}
