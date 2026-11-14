//! The effect rows of Tool Property.

use egui::{RichText, Ui};
use sue_wm::Theme;

use crate::studio::SuePaint;

impl SuePaint {
    /// The Object tool's settings for a 3D layer: its camera, light and
    /// look. A change shows at once and keeps as one step when let go.
    /// Tool Property for an effect line layer under the Object tool: how
    /// many lines, how wide, and a new scatter; kept when let go.
    pub(super) fn effect_properties(
        &mut self,
        ui: &mut Ui,
        layer: sue_core::EntityId,
        lines: crate::manga::EffectLines,
        theme: &Theme,
    ) {
        let mut next = lines;
        let mut count = next.count as f32;
        let mut changed = false;
        let mut row = |ui: &mut Ui,
                       name: &str,
                       value: &mut f32,
                       range: std::ops::RangeInclusive<f32>,
                       suffix: &str| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(theme.muted));
                let response = ui.add(egui::Slider::new(value, range).suffix(suffix));
                changed |= response.drag_stopped() || (response.changed() && !response.dragged());
            });
        };
        row(ui, &tr!("number-of-lines"), &mut count, 4.0..=400.0, "");
        row(
            ui,
            &tr!("line-width-2"),
            &mut next.line,
            0.5..=40.0,
            &tr!("px"),
        );
        next.count = count.round() as u32;
        if ui.button(tr!("scatter-again")).clicked() {
            next.seed = next
                .seed
                .wrapping_mul(747_796_405)
                .wrapping_add(2_891_336_453)
                | 1;
            changed = true;
        }
        if ui.button(tr!("use-drawing-color")).clicked() {
            next.rgb = self.drawing_color();
            changed = true;
        }
        ui.label(
            RichText::new(tr!("drag-effect-lines-ends"))
                .size(theme.small_size)
                .color(theme.muted),
        );
        if changed && next != lines {
            self.pending_effect = Some((layer, next));
        }
    }
}
