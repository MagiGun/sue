//! Edit, Advanced Fill (its settings and its dialog), and the selection
//! dialogs: Expand, Shrink, Blur Border, Outline Selection and Select
//! Color Gamut.

use egui::{Color32, Rect, Response, Sense, Ui};
use sue_wm::Theme;

use sue_wm::ui::{
    Radius, Space, Tip, checkbox, dropdown, focus_ring, form_grid, number_field, segmented, slider,
    tooltip,
};

use super::{Outcome, bare, note, row_height, tip};
use crate::studio::SuePaint;

/// Edit, Advanced Fill's settings, kept from one use to the next as Clip
/// Studio Paint keeps them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AdvancedFill {
    /// 0 main color, 1 sub color, 2 transparent, 3 `rgb`.
    pub color: u8,
    pub rgb: [u8; 3],
    /// 0 to 100.
    pub opacity: f32,
    pub blend: sue_model::BlendMode,
    pub area: crate::fill::AdvancedArea,
    /// Refer to every shown layer (else the current one).
    pub refer_all: bool,
}

impl Default for AdvancedFill {
    fn default() -> Self {
        Self {
            color: 0,
            rgb: [0, 0, 0],
            opacity: 100.0,
            blend: sue_model::BlendMode::Normal,
            area: crate::fill::AdvancedArea {
                anti_alias: false,
                scaling: 10,
                gap: 2,
                ..crate::fill::AdvancedArea::default()
            },
            refer_all: false,
        }
    }
}

/// What people read for an Advanced Fill target.
pub(crate) fn target_label(target: crate::fill::Target) -> String {
    use crate::fill::Target;
    match target {
        Target::All => tr!("target-all-colors"),
        Target::Transparent => tr!("only-transparent"),
        Target::NotTransparent => tr!("other-than-transparent"),
        Target::SurroundedByClear => tr!("area-surrounded-by-transparent"),
        Target::Black => tr!("only-black"),
        Target::NotBlack => tr!("other-than-black"),
        Target::SurroundedByBlack => tr!("area-surrounded-by-black"),
        Target::WhiteOrClear => tr!("only-white-and-transparent"),
        Target::NotWhiteOrClear => tr!("other-than-white-and-transparent"),
        Target::SurroundedByWhite => tr!("area-surrounded-by-white-and-transparent"),
        Target::SemiClear => tr!("treat-semi-transparent-as-transparent"),
    }
}

/// One of Advanced Fill's three color chips: the main color, the sub
/// color, or see-through (a checker). Returns the chip's response.
fn color_chip(
    ui: &mut Ui,
    color: Option<[u8; 3]>,
    chosen: bool,
    name: &str,
    theme: &Theme,
) -> Response {
    let side = row_height(ui);
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(side), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, chosen, name));
    let corner = theme.radius(Radius::Sm);
    let fill = if chosen {
        theme.fill_selected()
    } else if response.hovered() {
        theme.fill_hover()
    } else {
        theme.surface(2)
    };
    ui.painter().rect_filled(rect, corner, fill);
    if chosen {
        ui.painter().rect_stroke(
            rect,
            corner,
            theme.selected_stroke(),
            egui::StrokeKind::Inside,
        );
    }
    let chip = rect.shrink(theme.space(Space::S3));
    let small = theme.radius(Radius::Xs);
    match color {
        // The drawing color itself: what will be laid down, not a
        // styling color.
        Some([r, g, b]) => {
            ui.painter()
                .rect_filled(chip, small, Color32::from_rgb(r, g, b));
        }
        None => {
            // A checker for see-through, in the theme's own two tones.
            let half = chip.size() / 2.0;
            for (column, row) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let light = column == row;
                let corner = chip.min + egui::Vec2::new(column * half.x, row * half.y);
                ui.painter().rect_filled(
                    Rect::from_min_size(corner, half),
                    egui::CornerRadius::ZERO,
                    if light {
                        theme.surface(0)
                    } else {
                        theme.border_strong()
                    },
                );
            }
        }
    }
    ui.painter()
        .rect_stroke(chip, small, theme.hairline(), egui::StrokeKind::Outside);
    focus_ring(ui, &response, corner, theme);
    tip(response, name, "", theme)
}

/// A dropdown of `options` with its tooltip. Returns whether the choice
/// changed.
fn choice(
    ui: &mut Ui,
    salt: &str,
    chosen: &mut usize,
    options: &[String],
    name: &str,
    theme: &Theme,
) -> bool {
    let response = dropdown(salt, chosen, options).label(name).show(ui, theme);
    tip(response, name, "", theme).changed()
}

/// How a number field reads: its range, its decimals and its unit.
pub(super) struct Reads<'a> {
    pub range: std::ops::RangeInclusive<f64>,
    pub decimals: usize,
    pub suffix: &'a str,
}

/// A number field with its tooltip. Returns whether it changed.
pub(super) fn number(
    ui: &mut Ui,
    salt: &str,
    value: &mut f64,
    reads: &Reads<'_>,
    name: &str,
    theme: &Theme,
) -> bool {
    let response = number_field(salt, value)
        .range(reads.range.clone())
        .decimals(reads.decimals)
        .suffix(reads.suffix)
        .label(name)
        .show(ui, theme);
    tip(response, name, "", theme).changed()
}

/// A checkbox that flips `on` when clicked, with its tooltip (`help`
/// under its name when not empty). Returns whether it flipped.
pub(super) fn check(
    ui: &mut Ui,
    salt: &str,
    on: &mut bool,
    name: &str,
    help: &str,
    theme: &Theme,
) -> bool {
    let response = checkbox(salt, name, *on).show(ui, theme);
    let response = tooltip(response, &Tip::new(name).help(help), theme);
    if response.clicked() {
        *on = !*on;
        return true;
    }
    false
}

/// Advanced Fill's dialog, laid out as Clip Studio Paint's: the color and
/// how it lays down, then what it fills, then what it refers to.
#[expect(clippy::too_many_lines, reason = "the whole Advanced Fill dialog")]
pub(super) fn advanced_fill_ui(
    ui: &mut Ui,
    fill: &mut AdvancedFill,
    main: [u8; 3],
    sub: [u8; 3],
    theme: &Theme,
) -> Outcome {
    use crate::fill::Target;
    let px = tr!("px");
    let percent = |decimals| Reads {
        range: 0.0..=100.0,
        decimals,
        suffix: " %",
    };
    form_grid(ui, "sue-paint-advanced-fill", theme, |form| {
        form.row(&tr!("drawing-color"), |ui| {
            for (index, (color, name)) in [
                (Some(main), tr!("main-color")),
                (Some(sub), tr!("sub-color")),
                (None, tr!("transparent-color")),
            ]
            .into_iter()
            .enumerate()
            {
                let chosen = usize::from(fill.color) == index;
                if color_chip(ui, color, chosen, &name, theme).clicked() {
                    fill.color = index as u8;
                }
            }
            let mut rgba = [fill.rgb[0], fill.rgb[1], fill.rgb[2], 255];
            let pick =
                sue_wm::color_button(ui, "sue-paint-advanced-fill-color", &mut rgba, false, theme);
            if pick.changed {
                fill.rgb = [rgba[0], rgba[1], rgba[2]];
                fill.color = 3;
            }
        });
        let name = tr!("opacity");
        form.row(&name, |ui| {
            let mut opacity = f64::from(fill.opacity);
            let salt = "sue-paint-advanced-fill-opacity";
            if number(ui, salt, &mut opacity, &percent(0), &name, theme) {
                fill.opacity = opacity as f32;
            }
        });
        let name = tr!("blending-mode");
        form.row(&name, |ui| {
            let modes: Vec<String> = sue_model::BlendMode::ALL
                .iter()
                .map(|mode| sue_lang::translate("model", mode.label()))
                .collect();
            let mut chosen = sue_model::BlendMode::ALL
                .iter()
                .position(|mode| *mode == fill.blend)
                .unwrap_or(0);
            let salt = "sue-paint-advanced-fill-blend";
            if choice(ui, salt, &mut chosen, &modes, &name, theme) {
                fill.blend = sue_model::BlendMode::ALL[chosen];
            }
        });
        form.wide(|ui| {
            let name = tr!("anti-aliasing");
            let salt = "sue-paint-advanced-fill-aa";
            check(ui, salt, &mut fill.area.anti_alias, &name, "", theme);
        });
        form.section(&tr!("advanced-fill-area"));
        let name = tr!("target-color");
        form.row(&name, |ui| {
            let targets: Vec<Target> = Target::ALL
                .into_iter()
                .filter(|target| target.offered(fill.area.closed))
                .collect();
            let labels: Vec<String> = targets.iter().map(|target| target_label(*target)).collect();
            let mut chosen = targets
                .iter()
                .position(|target| *target == fill.area.target)
                .unwrap_or(0);
            let salt = "sue-paint-advanced-fill-target";
            if choice(ui, salt, &mut chosen, &labels, &name, theme) {
                fill.area.target = targets[chosen];
            }
        });
        let name = tr!("tolerance");
        form.row(&name, |ui| {
            let mut tolerance = f64::from(fill.area.tolerance) * 100.0;
            let salt = "sue-paint-advanced-fill-tolerance";
            if number(ui, salt, &mut tolerance, &percent(2), &name, theme) {
                fill.area.tolerance = (tolerance / 100.0) as f32;
            }
        });
        let name = tr!("area-scaling");
        form.row(&name, |ui| {
            let mut scaling = f64::from(fill.area.scaling);
            let reads = Reads {
                range: -50.0..=50.0,
                decimals: 0,
                suffix: &px,
            };
            let salt = "sue-paint-advanced-fill-scaling";
            if number(ui, salt, &mut scaling, &reads, &name, theme) {
                fill.area.scaling = scaling.round() as i32;
            }
        });
        let name = tr!("scaling-mode");
        form.row(&name, |ui| {
            let modes = [tr!("rectangle"), tr!("round")];
            let mut chosen = usize::from(fill.area.round);
            let salt = "sue-paint-advanced-fill-scaling-mode";
            if choice(ui, salt, &mut chosen, &modes, &name, theme) {
                fill.area.round = chosen == 1;
            }
        });
        form.wide(|ui| {
            let name = tr!("fill-closed-area");
            let salt = "sue-paint-advanced-fill-closed";
            if check(ui, salt, &mut fill.area.closed, &name, "", theme)
                && !fill.area.target.offered(fill.area.closed)
            {
                fill.area.target = Target::All;
            }
        });
        let name = tr!("close-gap");
        form.row(&name, |ui| {
            ui.add_enabled_ui(fill.area.closed, |ui| {
                let mut gap = f64::from(fill.area.gap);
                let reads = Reads {
                    range: 0.0..=50.0,
                    decimals: 0,
                    suffix: &px,
                };
                if number(
                    ui,
                    "sue-paint-advanced-fill-gap",
                    &mut gap,
                    &reads,
                    &name,
                    theme,
                ) {
                    fill.area.gap = gap.round() as u32;
                }
            });
        });
        form.section(&tr!("advanced-fill-refer"));
        form.wide(|ui| {
            let (name, help) = (tr!("refer-multiple"), tr!("refer-multiple-hint"));
            let salt = "sue-paint-advanced-fill-refer";
            check(ui, salt, &mut fill.refer_all, &name, &help, theme);
        });
        form.wide(|ui| {
            let name = tr!("refer-to-image-border");
            let salt = "sue-paint-advanced-fill-border";
            check(ui, salt, &mut fill.area.border, &name, "", theme);
        });
    });
    Outcome::Open
}

/// Expand, Shrink or Blur the border of the selection, or Outline
/// Selection: by how many pixels and, for Expand, with round or square
/// corners.
pub(super) fn select_by_ui(ui: &mut Ui, kind: &mut u8, amount: &mut f32, theme: &Theme) -> Outcome {
    let px = tr!("px");
    form_grid(ui, "sue-paint-select-by", theme, |form| {
        let name = if *kind == 2 {
            bare(tr!("blur-width"))
        } else {
            bare(tr!("width"))
        };
        form.row(&name, |ui| {
            let mut width = f64::from(*amount);
            let response = slider("sue-paint-select-by-width", &mut width, 1.0..=100.0)
                .name(&name)
                .suffix(&px)
                .show(ui, theme);
            if response.changed() {
                *amount = width as f32;
            }
            tip(response, &name, "", theme);
        });
        if matches!(*kind, 0 | 4) {
            let name = tr!("expansion-type");
            form.row(&name, |ui| {
                let types = [tr!("expansion-round"), tr!("expansion-square")];
                let chosen = usize::from(*kind == 4);
                let strip =
                    segmented("sue-paint-select-by-type", &types, Some(chosen)).show(ui, theme);
                tip(strip.response, &name, "", theme);
                match strip.picked {
                    Some(0) => *kind = 0,
                    Some(1) => *kind = 4,
                    _ => {}
                }
            });
        }
    });
    Outcome::Open
}

/// Select Color Gamut: the tolerance and whether it looks at every layer.
pub(super) fn gamut_ui(ui: &mut Ui, tolerance: &mut f32, all: &mut bool, theme: &Theme) -> Outcome {
    note(ui, &tr!("selects-colors-near-drawing-color"), theme);
    form_grid(ui, "sue-paint-gamut", theme, |form| {
        let name = bare(tr!("color-margin"));
        form.row(&name, |ui| {
            let mut percent = f64::from(*tolerance) * 100.0;
            let response = slider("sue-paint-gamut-margin", &mut percent, 0.0..=100.0)
                .name(&name)
                .suffix(" %")
                .show(ui, theme);
            if response.changed() {
                *tolerance = (percent / 100.0) as f32;
            }
            tip(response, &name, "", theme);
        });
        form.wide(|ui| {
            let name = tr!("refer-to-all-layers");
            check(ui, "sue-paint-gamut-all", all, &name, "", theme);
        });
    });
    Outcome::Open
}

impl SuePaint {
    /// Expands, shrinks or blurs the border of the selection by `amount`.
    pub(super) fn select_by_ok(&mut self, kind: u8, amount: f32) {
        if let Some(doc) = self.doc_mut()
            && let Some(selection) = doc.selection.clone()
        {
            let changed = match kind {
                0 => selection.expanded(amount),
                4 => selection.expanded_square(amount),
                1 => selection.shrunk(amount),
                _ => selection.blurred(amount),
            };
            doc.set_selection(Some(changed));
        }
    }

    /// Edit, Advanced Fill with the studio's colors and its kept settings.
    pub(super) fn advanced_fill_dialog_ui(&mut self, ui: &mut Ui, theme: &Theme) -> Outcome {
        let (main, sub) = (self.main, self.sub);
        advanced_fill_ui(ui, &mut self.advanced_fill, main, sub, theme)
    }
}
