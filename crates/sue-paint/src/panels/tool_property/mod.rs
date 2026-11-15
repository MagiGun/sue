//! The Tool Property palette.

mod brush;
mod comic;
mod correct;
mod effect;
mod fill;
mod lines;
mod paint;
mod ruler;
mod select;
mod text;
mod three;

use egui::{RichText, Ui};
use sue_wm::Theme;

use crate::studio::SuePaint;
use crate::tools::Tool;

/// Tool Property's small, muted labels and hints.
fn small_text(theme: &Theme) -> impl Fn(&str) -> RichText + '_ {
    move |text: &str| {
        RichText::new(text)
            .size(theme.small_size + 0.5)
            .color(theme.muted)
    }
}

/// A Blend sub tool mode that liquifies (`blend::LiquifyMode`), which the
/// Liquify button shows.
pub(super) fn is_liquify_mode(mode: u8) -> bool {
    crate::blend::is_liquify(mode)
}

impl SuePaint {
    /// The settings of the current sub tool, and a command they ask for.
    pub(super) fn tool_properties(
        &mut self,
        ui: &mut Ui,
        tool: Tool,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) -> Option<&'static str> {
        if tool == Tool::Operation
            && let Some(folder) = self.camera_folder(arena)
        {
            return self.camera_properties(ui, folder, arena, theme);
        }
        if tool == Tool::Operation
            && let Some(layer) = self
                .layer()
                .filter(|layer| crate::three::is_3d(arena, *layer))
        {
            self.view3d_properties(ui, layer, arena, theme);
            return None;
        }
        if tool == Tool::Operation
            && let Some(layer) = self.layer()
            && let Some(lines) = Self::effect_of(arena, layer)
        {
            self.effect_properties(ui, layer, lines, theme);
            return None;
        }
        // The Object tool on a vector layer: the picked lines' settings.
        if tool == Tool::Operation && self.line_properties(ui, arena, theme) {
            return None;
        }
        ui.spacing_mut().item_spacing.y = 6.0;
        let small = small_text(theme);
        let mut command = None;
        let sub = self.toolbox.current_mut();
        match tool {
            tool if tool.paints() => {
                if tool == Tool::Figure {
                    self.magnet_row(ui, theme);
                }
                let sub = self.toolbox.current_mut();
                paint::paint_properties(ui, tool, sub, theme);
            }
            Tool::Fill | Tool::AutoSelect => {
                fill::fill_properties(ui, tool, sub, &mut self.select_op, theme);
            }
            Tool::Gradient => fill::gradient_properties(ui, sub, theme),
            Tool::Select => select::select_properties(ui, sub, &mut self.select_op, theme),
            Tool::Eyedropper => {
                ui.selectable_value(&mut sub.mode, 0, tr!("obtain-display-color"));
                ui.selectable_value(&mut sub.mode, 1, tr!("obtain-layer-color"));
            }
            Tool::Zoom => {
                ui.selectable_value(&mut sub.mode, 0, tr!("zoom-in-on-click"));
                ui.selectable_value(&mut sub.mode, 1, tr!("zoom-out-on-click"));
                ui.label(small(&tr!("alt-click-does-the-opposite")));
            }
            Tool::Hand => {
                ui.selectable_value(&mut sub.mode, 0, tr!("scroll"));
                ui.selectable_value(&mut sub.mode, 1, tr!("rotate"));
            }
            Tool::MoveLayer => {
                ui.label(small(&tr!("drag-move-current-layer")));
            }
            Tool::Ruler => command = ruler::ruler_properties(ui, sub, theme),
            Tool::CorrectLine => correct::correct_line_properties(ui, sub, theme),
            Tool::Blend => paint::blend_properties(ui, sub, theme),
            Tool::Frame => comic::frame_properties(ui, sub, theme),
            Tool::Balloon => comic::balloon_properties(ui, sub, theme),
            Tool::Text => text::text_properties(ui, sub, self.typing.as_mut(), theme),
            // The Object tool: what it does on the layer in hand.
            Tool::Operation => {
                ui.label(small(&tr!("object-tool-hint")));
            }
            _ => {
                ui.label(small(&tr!("tool-arrives-later-sue-paint")));
            }
        }
        command
    }
}
