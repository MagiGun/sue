//! Pen Pressure Settings: the curve every tool uses, fitted to a hand.

use egui::{RichText, Sense, Ui, vec2};
use sue_model::Response;
use sue_rice::{RiceValue, SetRiceValue};
use sue_wm::{AppletContext, Theme, widgets};

use super::Outcome;
use crate::studio::SuePaint;

/// The setting that holds the pen pressure curve every tool uses.
pub(crate) const PEN_PRESSURE: &str = "sue_paint.pen_pressure";

/// The pen pressure curve Sue Paint starts with: a little softer than
/// straight, as most hands press drawing tablets, so a usual press is
/// already more than half a line. File, Pen Pressure Settings fits it to a
/// hand.
pub(crate) const DEFAULT_PEN_PRESSURE: &str = "0,0 0.4,0.5 1,1";

/// Pen Pressure Settings (Clip Studio Paint's Adjust pen pressure): draw
/// on the pad as you normally would and the curve is fitted to how hard
/// you press, then made stronger or lighter by hand or on the graph.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PressureDialog {
    /// The curve being adjusted.
    pub curve: Response,
    /// The pen's own pressures drawn on the pad, as they came.
    pub raw: Vec<f32>,
    /// The pad's lines: points (from the pad's corner) and raw pressure.
    pub lines: Vec<Vec<(egui::Vec2, f32)>>,
    /// The fitted curve is being tried out.
    pub checked: bool,
    /// The graph shows.
    pub show_graph: bool,
    /// The pen's latest pressure, while it touches.
    pub force: Option<f32>,
}

impl PressureDialog {
    /// Adjusting from `curve`, the one in use.
    pub(crate) fn new(curve: Response) -> Self {
        Self {
            curve,
            raw: Vec::new(),
            lines: Vec::new(),
            checked: false,
            show_graph: true,
            force: None,
        }
    }

    /// The fewest pressures [`Self::fit`] works from.
    pub(crate) const ENOUGH: usize = 24;

    /// A curve for someone who drew `raw` (pressures as they normally
    /// press): the firm end of their pressing reaches a full line and
    /// their usual pressing half of one. `None` until there are enough.
    pub(crate) fn fit(raw: &[f32]) -> Option<Response> {
        let mut sorted: Vec<f32> = raw
            .iter()
            .copied()
            .filter(|pressure| pressure.is_finite() && *pressure > 0.0)
            .collect();
        if sorted.len() < Self::ENOUGH {
            return None;
        }
        sorted.sort_by(f32::total_cmp);
        let at = |share: f32| sorted[((sorted.len() - 1) as f32 * share).round() as usize];
        let firm = at(0.95).clamp(0.15, 1.0);
        let usual = at(0.5).clamp(0.02, firm * 0.9);
        let gamma = (0.5_f32.ln() / (usual / firm).ln()).clamp(0.35, 2.8);
        let out = |x: f32| (x / firm).min(1.0).powf(gamma);
        let mut points: Vec<[f32; 2]> = [0.0, 0.25, 0.5, 0.75, 1.0]
            .iter()
            .map(|share| [share * firm, out(share * firm)])
            .collect();
        points.push([1.0, 1.0]);
        Some(Response::new(&points))
    }
}

/// Pen Pressure Settings: the words, the buttons, the pad and the graph.
pub(super) fn pressure_ui(ui: &mut Ui, pressure: &mut PressureDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let words = if pressure.checked {
        tr!("draw-again-try-adjusted-pressure-stronger")
    } else {
        tr!("draw-freely-pad-pen-while-drawing")
    };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(330.0);
            ui.label(RichText::new(words).color(theme.muted));
        });
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            let wide = vec2(ui.available_width(), 22.0);
            if pressure.checked {
                ui.horizontal(|ui| {
                    let half = vec2(wide.x / 2.0 - 4.0, wide.y);
                    if ui
                        .add_sized(half, egui::Button::new(tr!("stronger")))
                        .clicked()
                    {
                        pressure.curve = pressure.curve.map_outputs(|y| y.powf(0.8));
                    }
                    if ui
                        .add_sized(half, egui::Button::new(tr!("lighter")))
                        .clicked()
                    {
                        pressure.curve = pressure.curve.map_outputs(|y| y.powf(1.25));
                    }
                });
                if ui.add_sized(wide, egui::Button::new(tr!("done"))).clicked() {
                    outcome = Outcome::Ok;
                }
            } else {
                let enough = pressure.raw.len() >= PressureDialog::ENOUGH;
                let check = ui
                    .add_enabled_ui(enough, |ui| {
                        ui.add_sized(wide, egui::Button::new(tr!("check-adjusted-settings")))
                    })
                    .inner
                    .on_disabled_hover_text(tr!("draw-few-lines-pen-first"));
                if check.clicked()
                    && let Some(curve) = PressureDialog::fit(&pressure.raw)
                {
                    pressure.curve = curve;
                    pressure.checked = true;
                    pressure.lines.clear();
                }
            }
            if ui
                .add_sized(wide, egui::Button::new(tr!("cancel")))
                .clicked()
            {
                outcome = Outcome::Cancel;
            }
            if ui
                .add_sized(wide, egui::Button::new(tr!("reset-to-default")))
                .clicked()
            {
                pressure.curve = DEFAULT_PEN_PRESSURE.parse().unwrap_or_default();
                pressure.raw.clear();
                pressure.lines.clear();
                pressure.checked = false;
            }
        });
    });
    ui.add_space(6.0);

    // The pad: lines as wide as the adjusted pressure makes them.
    let (pad, response) = ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::drag());
    let touch = ui.input(|input| {
        input.events.iter().rev().find_map(|event| match event {
            egui::Event::Touch { phase, force, .. } => Some(match phase {
                egui::TouchPhase::Start | egui::TouchPhase::Move => *force,
                egui::TouchPhase::End | egui::TouchPhase::Cancel => None,
            }),
            _ => None,
        })
    });
    if let Some(force) = touch {
        pressure.force = force.map(|force| force.clamp(0.0, 1.0));
    } else if sue_platform::stylus::source().is_some() {
        // A WinTab tablet or a pen in a browser.
        pressure.force = sue_platform::stylus::pressure();
    }
    if response.drag_started() {
        pressure.lines.push(Vec::new());
        if pressure.lines.len() > 40 {
            pressure.lines.remove(0);
        }
    }
    if response.dragged()
        && let (Some(pos), Some(line)) =
            (response.interact_pointer_pos(), pressure.lines.last_mut())
    {
        let raw = pressure.force.unwrap_or(1.0);
        line.push((pos - pad.min, raw));
        if let Some(force) = pressure.force
            && pressure.raw.len() < 4000
        {
            pressure.raw.push(force);
        }
    }
    let painter = ui.painter_at(pad);
    painter.rect_filled(pad, 4.0, egui::Color32::from_gray(245));
    for line in &pressure.lines {
        for pair in line.windows(2) {
            let width = 0.6 + 9.0 * pressure.curve.eval(pair[1].1);
            painter.line_segment(
                [pad.min + pair[0].0, pad.min + pair[1].0],
                egui::Stroke::new(width, egui::Color32::from_gray(20)),
            );
        }
    }
    if pressure.lines.iter().any(|line| !line.is_empty()) && pressure.raw.is_empty() {
        painter.text(
            pad.center_bottom() - vec2(0.0, 8.0),
            egui::Align2::CENTER_BOTTOM,
            "This pointer reports no pen pressure",
            egui::FontId::proportional(theme.small_size),
            egui::Color32::from_gray(110),
        );
    }
    ui.add_space(6.0);
    ui.checkbox(
        &mut pressure.show_graph,
        RichText::new(tr!("show-pen-pressure-graph")).color(theme.text),
    );
    if pressure.show_graph {
        widgets::response_curve(
            ui,
            "pen-pressure",
            &mut pressure.curve,
            150.0,
            widgets::CurveLabels::pressure(),
            theme,
        );
    }
    outcome
}

impl SuePaint {
    /// Keeps the fitted curve for every tool.
    pub(super) fn pressure_ok(cx: &mut AppletContext<'_>, pressure: &PressureDialog) {
        let text = pressure.curve.to_string();
        match cx.edit_rice(SetRiceValue::new(PEN_PRESSURE, RiceValue::Text(text))) {
            Ok(_) => cx.notify(tr!("pen-pressure-adjusted-every-tool")),
            Err(error) => cx.fail(tr!("cannot-keep-the-pen-pressure", error = error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_pen_curve_is_a_little_soft() {
        let curve: Response = DEFAULT_PEN_PRESSURE.parse().unwrap();
        assert!((curve.eval(0.0)).abs() < 1e-3);
        assert!((curve.eval(1.0) - 1.0).abs() < 1e-3);
        let usual = curve.eval(0.4);
        assert!(usual > 0.42 && usual < 0.6, "softer than straight: {usual}");
        // Rising all the way.
        let mut last = 0.0;
        for step in 1..=20 {
            let value = curve.eval(step as f32 / 20.0);
            assert!(value >= last);
            last = value;
        }
    }

    #[test]
    fn pen_pressure_fits_to_how_hard_someone_presses() {
        assert_eq!(PressureDialog::fit(&[0.5; 3]), None, "not enough drawn");
        // A light hand: presses between 5% and 60%.
        let light: Vec<f32> = (0..200)
            .map(|step| 0.05 + 0.55 * step as f32 / 199.0)
            .collect();
        let curve = PressureDialog::fit(&light).unwrap();
        assert!(
            curve.eval(0.6) > 0.85,
            "a firm press is a full line: {}",
            curve.eval(0.6)
        );
        let usual = curve.eval(0.33);
        assert!((usual - 0.5).abs() < 0.15, "a usual press is half: {usual}");
        assert!((curve.eval(1.0) - 1.0).abs() < 1e-3);
        // Stronger and Lighter move it up and down.
        let stronger = curve.map_outputs(|y| y.powf(0.8));
        assert!(stronger.eval(0.33) > usual);
        // It reads back from its setting.
        assert_eq!(
            curve
                .to_string()
                .parse::<Response>()
                .unwrap()
                .points()
                .len(),
            curve.points().len()
        );
    }
}
