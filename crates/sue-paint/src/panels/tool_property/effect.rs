//! The effect rows of Tool Property.

use egui::{RichText, Ui};
use sue_wm::Theme;
use sue_wm::ui::TextRole;

use crate::studio::SuePaint;

impl SuePaint {
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
        let mut count = f64::from(next.count);
        let mut line = f64::from(next.line);
        let mut changed = false;
        let mut row = |ui: &mut Ui,
                       id: &str,
                       name: &str,
                       value: &mut f64,
                       range: std::ops::RangeInclusive<f64>,
                       suffix: &str| {
            let response = sue_wm::ui::slider(("sue-paint-effect", id), value, range)
                .label(name)
                .suffix(suffix)
                .decimals(usize::from(!suffix.is_empty()))
                .show(ui, theme)
                .on_hover_text(name);
            changed |= response.drag_stopped() || (response.changed() && !response.dragged());
        };
        row(
            ui,
            "count",
            &tr!("number-of-lines"),
            &mut count,
            4.0..=400.0,
            "",
        );
        row(
            ui,
            "line",
            &tr!("line-width-2"),
            &mut line,
            0.5..=40.0,
            &tr!("px"),
        );
        next.count = count.round() as u32;
        next.line = line as f32;
        let scatter = tr!("scatter-again");
        if sue_wm::ui::button("sue-paint-effect-scatter", &scatter)
            .show(ui, theme)
            .on_hover_text(&scatter)
            .clicked()
        {
            next.seed = next
                .seed
                .wrapping_mul(747_796_405)
                .wrapping_add(2_891_336_453)
                | 1;
            changed = true;
        }
        let color = tr!("use-drawing-color");
        if sue_wm::ui::button("sue-paint-effect-color", &color)
            .show(ui, theme)
            .on_hover_text(&color)
            .clicked()
        {
            next.rgb = self.drawing_color();
            changed = true;
        }
        ui.label(
            RichText::new(tr!("drag-effect-lines-ends"))
                .size(theme.text_size(TextRole::Caption))
                .color(theme.text_muted()),
        );
        if changed && next != lines {
            self.pending_effect = Some((layer, next));
        }
    }
}
