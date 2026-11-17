//! Tool Property while a transform is open: what the bar over the canvas
//! holds, as rows a person reads from the top (Reset, the flips, Cancel
//! and OK; the mode and the rotation centre; Keep original image; the
//! scale, the angle and the position as numbers; how the pixels are
//! sampled).

use egui::Ui;
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::Space;

use super::{Category, Rows, name_cell, name_width};
use crate::studio::SuePaint;
use crate::transform::{Interpolation, Mode};
use crate::transform_ui::Pivot;

/// The names of the interpolation choices, in [`Interpolation::ALL`]'s
/// order.
fn interpolation_names() -> Vec<String> {
    Interpolation::ALL
        .iter()
        .map(|choice| crate::transform_ui::interpolation_label(*choice))
        .collect()
}

/// A row of a name and a number field. Returns `true` when it was typed
/// in or dragged.
#[allow(clippy::too_many_arguments, reason = "a number field and its row")]
fn number_row(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    id: &str,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    decimals: usize,
) -> bool {
    rows.row(ui, id, Category::Operation, |ui, theme, enabled| {
        ui.horizontal(|ui| {
            name_cell(ui, label, name_width(theme), theme);
            let width = ui.available_width();
            let field = sue_wm::ui::number_field(("sue-paint-transform-row", id), value)
                .range(range)
                .decimals(decimals)
                .suffix(suffix)
                .width(width)
                .label(label)
                .show(ui, theme);
            let field = field.on_hover_text(label);
            enabled && field.changed()
        })
        .inner
    })
    .unwrap_or(false)
}

impl SuePaint {
    /// Whether Tool Property holds an open transform's settings: one is
    /// open, and it is not Move layer's drag of selected pixels.
    pub(crate) fn transforming(&self) -> bool {
        self.transform.is_some() && !self.move_lift
    }

    /// What Tool Property's title says while a transform is open: the
    /// transform's name, as its undo step names it.
    pub(crate) fn transform_title(&self) -> Option<String> {
        let session = self.transform.as_ref().filter(|_| !self.move_lift)?;
        Some(if session.mesh.is_some() {
            tr!("mesh-transformation")
        } else {
            self.transform_label(session.free)
        })
    }

    /// Tool Property's body while a transform is open.
    pub(super) fn transform_properties(&mut self, ui: &mut Ui, theme: &Theme) {
        let Self {
            transform,
            transform_prefs: prefs,
            tool_palettes,
            ..
        } = self;
        let Some(session) = transform.as_mut() else {
            return;
        };
        ui.spacing_mut().item_spacing.y = theme.space(Space::S3);
        // The buttons: Reset, the two flips, Cancel and OK.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.space(Space::S1);
            let mut buttons = vec![(Icon::Reset, tr!("reset"), 0)];
            if session.mesh.is_none() {
                buttons.push((Icon::FlipHorizontal, tr!("flip-horizontal"), 1));
                buttons.push((Icon::FlipVertical, tr!("flip-vertical"), 2));
            }
            buttons.push((Icon::Cancel, tr!("cancel"), 3));
            buttons.push((Icon::Confirm, tr!("ok"), 4));
            for (icon, name, what) in buttons {
                let pressed =
                    sue_wm::ui::icon_button(("sue-paint-transform-tool", what), icon, &name)
                        .show(ui, theme);
                let pressed = pressed.clicked();
                if pressed {
                    match what {
                        0 => session.reset(),
                        1 => session.flip(true),
                        2 => session.flip(false),
                        3 => session.asked = Some(false),
                        _ => session.asked = Some(true),
                    }
                }
            }
        });
        let mut rows = Rows::property(&[], tool_palettes.headings, true, theme);
        if session.mesh.is_none() {
            let names: Vec<String> = Mode::ALL.iter().map(|mode| mode.label()).collect();
            let mut chosen = Mode::ALL
                .iter()
                .position(|mode| *mode == session.mode)
                .unwrap_or(0);
            if rows.choice(
                ui,
                "transform-mode",
                Category::Operation,
                &tr!("mode"),
                &mut chosen,
                &names,
            ) {
                let quad = session.quad();
                session.mode = Mode::ALL[chosen];
                if session.mode.frees_corners() && !session.free {
                    session.free = true;
                    session.corners = quad;
                }
            }
            if !session.mode.frees_corners() {
                let pivots: Vec<String> = Pivot::LIST.iter().map(|pivot| pivot.label()).collect();
                let mut at = Pivot::LIST
                    .iter()
                    .position(|pivot| {
                        *pivot == session.pivot
                            || (session.pivot == Pivot::Point(1, 1) && *pivot == Pivot::Middle)
                    })
                    .unwrap_or(4);
                if rows.choice(
                    ui,
                    "transform-pivot",
                    Category::Operation,
                    &tr!("tp-rotation-center"),
                    &mut at,
                    &pivots,
                ) {
                    let point = session.pivot_point();
                    session.pivot = Pivot::LIST[at];
                    if session.pivot == Pivot::Free {
                        session.pivot_at = point;
                    } else {
                        prefs.pivot = session.pivot;
                    }
                }
            }
        }
        rows.check(
            ui,
            "transform-keep-original",
            Category::Operation,
            &tr!("keep-original-image"),
            None,
            &mut prefs.keep_original,
        );
        if session.mesh.is_none() && !session.free {
            let mut width = f64::from(session.scale[0] * 100.0);
            let mut height = f64::from(session.scale[1] * 100.0);
            let was = [width, height];
            let mut typed = number_row(
                ui,
                &mut rows,
                "transform-scale-width",
                &tr!("tp-scale-width"),
                &mut width,
                -2000.0..=2000.0,
                " %",
                1,
            );
            typed |= number_row(
                ui,
                &mut rows,
                "transform-scale-height",
                &tr!("tp-scale-height"),
                &mut height,
                -2000.0..=2000.0,
                " %",
                1,
            );
            if typed {
                if prefs.keep_aspect {
                    // The other side follows, keeping the shape.
                    if (width - was[0]).abs() > f64::EPSILON && was[0].abs() > f64::EPSILON {
                        height = was[1] * width / was[0];
                    } else if (height - was[1]).abs() > f64::EPSILON && was[1].abs() > f64::EPSILON
                    {
                        width = was[0] * height / was[1];
                    }
                }
                session.scale = [(width / 100.0) as f32, (height / 100.0) as f32];
            }
            rows.check(
                ui,
                "transform-keep-aspect",
                Category::Operation,
                &tr!("keep-aspect-ratio"),
                Some(&tr!("tp-keep-aspect-hint")),
                &mut prefs.keep_aspect,
            );
            let mut degrees = f64::from(session.angle.to_degrees());
            if number_row(
                ui,
                &mut rows,
                "transform-angle",
                &tr!("tp-rotation-angle"),
                &mut degrees,
                -360.0..=360.0,
                &tr!("deg"),
                1,
            ) {
                session.angle = (degrees as f32).to_radians();
            }
            let mut x = f64::from(session.center[0]);
            let mut y = f64::from(session.center[1]);
            let mut moved = number_row(
                ui,
                &mut rows,
                "transform-x",
                &tr!("position-x"),
                &mut x,
                -100_000.0..=100_000.0,
                " px",
                0,
            );
            moved |= number_row(
                ui,
                &mut rows,
                "transform-y",
                &tr!("position-y"),
                &mut y,
                -100_000.0..=100_000.0,
                " px",
                0,
            );
            if moved {
                session.center = [x as f32, y as f32];
            }
        }
        if session.mesh.is_none() {
            let names = interpolation_names();
            let mut chosen = Interpolation::ALL
                .iter()
                .position(|choice| *choice == prefs.interpolation)
                .unwrap_or(1);
            if rows.choice(
                ui,
                "transform-interpolation",
                Category::Operation,
                &tr!("tp-interpolation"),
                &mut chosen,
                &names,
            ) {
                prefs.interpolation = Interpolation::ALL[chosen];
            }
        }
    }
}
