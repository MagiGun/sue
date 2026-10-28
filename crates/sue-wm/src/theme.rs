//! The visual theme: every color, size and font comes from the rice
//! and is bound straight into `egui::Style` and `egui::Visuals`. The shipped
//! palette is pure monochrome, from `#000000` to `#ffffff`.

use std::time::Duration;

use egui::epaint::Shadow;
use egui::style::{Selection, TextCursorStyle, WidgetVisuals, Widgets};
use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};
use sue_rice::{Rgba, RiceSchema};

/// Colors, geometry and type for the whole shell.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Canvas background.
    pub void: Color32,
    /// Drawers and panels.
    pub panel: Color32,
    /// Floating pills, cards and entity frames.
    pub pill: Color32,
    /// Hovered controls.
    pub hover: Color32,
    /// Pressed and chosen controls.
    pub active: Color32,
    /// Hairline borders.
    pub border: Color32,
    /// Borders of selected entities and focused controls.
    pub border_active: Color32,
    /// Primary text and active icons.
    pub text: Color32,
    /// Secondary text.
    pub muted: Color32,
    /// Text selection highlight.
    pub selection: Color32,
    /// Text inside a selection.
    pub selection_text: Color32,
    /// Canvas grid.
    pub grid: Color32,
    /// Shadow under floating surfaces.
    pub shadow: Color32,
    /// Space inside drawers, pills and controls.
    pub padding: f32,
    /// Hairline thickness.
    pub border_width: f32,
    /// Rounding of buttons and fields.
    pub control_radius: f32,
    /// Rounding of pills and cards.
    pub pill_radius: f32,
    /// Height of floating pills.
    pub pill_height: f32,
    /// Width of the category list in Settings.
    pub settings_sidebar_width: f32,
    /// Shadow blur under floating surfaces.
    pub shadow_blur: f32,
    /// Interface font family name.
    pub ui_font: String,
    /// Interface text size.
    pub ui_size: f32,
    /// Captions and the status bar.
    pub small_size: f32,
    /// Headings.
    pub heading_size: f32,
    /// Code and hex.
    pub mono_size: f32,
    /// Default size of new documents.
    pub document_size: f32,
}

fn color(rgba: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(rgba.r, rgba.g, rgba.b, rgba.a)
}

impl Theme {
    /// Reads the theme from `rice`, falling back to the shipped defaults for
    /// anything missing.
    #[must_use]
    pub fn from_rice(rice: &RiceSchema) -> Self {
        let paint = |path: &str, fallback: u8| {
            rice.get_color(path)
                .map_or(Color32::from_gray(fallback), color)
        };
        let number =
            |path: &str, fallback: f32| rice.get_f64(path).map_or(fallback, |value| value as f32);
        Self {
            void: paint("colors.void", 0x0a),
            panel: paint("colors.panel", 0x11),
            pill: paint("colors.pill", 0x18),
            hover: paint("colors.hover", 0x26),
            active: paint("colors.active", 0x33),
            border: paint("colors.border", 0x26),
            border_active: paint("colors.border_active", 0xff),
            text: paint("colors.text", 0xff),
            muted: paint("colors.muted", 0x80),
            selection: paint("colors.selection", 0x40),
            selection_text: paint("colors.selection_text", 0xff),
            grid: paint("colors.grid", 0x26),
            shadow: paint("colors.shadow", 0x00),
            padding: number("layout.padding", 8.0),
            border_width: number("layout.border_width", 1.0),
            control_radius: number("layout.control_radius", 4.0),
            pill_radius: number("layout.pill_radius", 12.0),
            pill_height: number("layout.pill_height", 34.0),
            settings_sidebar_width: number("layout.settings_sidebar_width", 190.0),
            shadow_blur: number("layout.shadow_blur", 18.0),
            ui_font: rice
                .get_str("typography.ui_font")
                .unwrap_or("Ubuntu Light")
                .to_owned(),
            ui_size: number("typography.ui_size", 13.0),
            small_size: number("typography.small_size", 11.0),
            heading_size: number("typography.heading_size", 18.0),
            mono_size: number("typography.mono_size", 12.0),
            document_size: number("typography.document_size", 15.0),
        }
    }

    /// A hairline in the border color.
    #[must_use]
    pub fn hairline(&self) -> Stroke {
        Stroke::new(self.border_width, self.border)
    }

    /// A hairline in the active border color.
    #[must_use]
    pub fn hairline_active(&self) -> Stroke {
        Stroke::new(self.border_width, self.border_active)
    }

    /// The interface font at `size`.
    #[must_use]
    pub fn font(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Proportional)
    }

    /// The monospace font at `size`.
    #[must_use]
    pub fn mono(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Monospace)
    }

    /// The soft shadow under pills, menus and windows.
    #[must_use]
    pub fn shadow(&self) -> Shadow {
        let blur = self.shadow_blur.clamp(0.0, 48.0) as u8;
        Shadow {
            offset: [0, (blur / 3).min(16) as i8],
            blur,
            spread: 0,
            color: self.shadow.gamma_multiply(0.6),
        }
    }

    fn radius(value: f32) -> CornerRadius {
        CornerRadius::same(value.clamp(0.0, 255.0) as u8)
    }

    /// egui visuals for this theme.
    #[must_use]
    pub fn visuals(&self, caret_blink: Option<Duration>) -> Visuals {
        let control = Self::radius(self.control_radius);
        let widget = |fill: Color32, stroke: Stroke, text: Color32| WidgetVisuals {
            bg_fill: fill,
            weak_bg_fill: fill,
            bg_stroke: stroke,
            corner_radius: control,
            fg_stroke: Stroke::new(1.0, text),
            expansion: 0.0,
        };
        let mut visuals = Visuals::dark();
        visuals.dark_mode = true;
        visuals.override_text_color = None;
        visuals.weak_text_color = Some(self.muted);
        visuals.widgets = Widgets {
            noninteractive: widget(self.panel, self.hairline(), self.text),
            inactive: widget(self.pill, self.hairline(), self.text),
            hovered: widget(
                self.hover,
                Stroke::new(self.border_width, self.active),
                self.text,
            ),
            active: widget(self.active, self.hairline_active(), self.text),
            open: widget(self.active, self.hairline(), self.text),
        };
        visuals.selection = Selection {
            bg_fill: self.selection,
            stroke: Stroke::new(1.0, self.selection_text),
        };
        visuals.text_cursor = TextCursorStyle {
            stroke: Stroke::new(2.0, self.text),
            preview: false,
            blink: caret_blink.is_some(),
            on_duration: caret_blink.map_or(0.5, |half| half.as_secs_f32()),
            off_duration: caret_blink.map_or(0.5, |half| half.as_secs_f32()),
        };
        visuals.hyperlink_color = self.text;
        visuals.faint_bg_color = self.panel;
        visuals.extreme_bg_color = self.void;
        visuals.text_edit_bg_color = Some(self.void);
        visuals.code_bg_color = self.pill;
        visuals.warn_fg_color = self.text;
        visuals.error_fg_color = self.text;
        visuals.window_corner_radius = Self::radius(self.pill_radius);
        visuals.window_shadow = self.shadow();
        visuals.window_fill = self.panel;
        visuals.window_stroke = self.hairline();
        visuals.window_highlight_topmost = false;
        visuals.menu_corner_radius = Self::radius(self.pill_radius);
        visuals.panel_fill = self.panel;
        visuals.popup_shadow = self.shadow();
        visuals.button_frame = true;
        visuals.striped = false;
        visuals.slider_trailing_fill = true;
        visuals.handle_shape = egui::style::HandleShape::Circle;
        visuals.indent_has_left_vline = false;
        visuals
    }

    /// Applies the theme to `ctx`: visuals, spacing and text styles. Fonts
    /// change only when the interface font does.
    pub fn apply(
        &self,
        ctx: &egui::Context,
        caret_blink: Option<Duration>,
        previous: Option<&Self>,
    ) {
        let visuals = self.visuals(caret_blink);
        let theme = self.clone();
        ctx.all_styles_mut(move |style| {
            style.visuals = visuals.clone();
            style.text_styles = [
                (TextStyle::Small, theme.font(theme.small_size)),
                (TextStyle::Body, theme.font(theme.ui_size)),
                (TextStyle::Button, theme.font(theme.ui_size)),
                (TextStyle::Heading, theme.font(theme.heading_size)),
                (TextStyle::Monospace, theme.mono(theme.mono_size)),
            ]
            .into();
            let spacing = &mut style.spacing;
            spacing.item_spacing = egui::vec2(theme.padding * 0.75, theme.padding * 0.5);
            spacing.button_padding = egui::vec2(theme.padding, theme.padding * 0.5);
            spacing.window_margin = egui::Margin::same(theme.padding.clamp(0.0, 127.0) as i8);
            spacing.menu_margin =
                egui::Margin::same((theme.padding * 0.75).clamp(0.0, 127.0) as i8);
            spacing.interact_size.y = (theme.ui_size * 1.9).max(20.0);
            spacing.slider_rail_height = 4.0;
            spacing.slider_width = 160.0;
        });
        if previous.is_none_or(|old| old.ui_font != self.ui_font) {
            ctx.set_fonts(fonts(&self.ui_font));
        }
    }
}

/// The bundled fonts, with `ui_font` first for proportional text.
fn fonts(ui_font: &str) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    if ui_font == "Hack"
        && let Some(proportional) = fonts.families.get_mut(&FontFamily::Proportional)
    {
        proportional.retain(|name| name != "Hack");
        proportional.insert(0, "Hack".to_owned());
    }
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_theme_is_pure_monochrome() {
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        let theme = Theme::from_rice(&rice);
        assert_eq!(theme.void, Color32::from_gray(0x0a));
        assert_eq!(theme.selection, Color32::from_gray(0x40));
        let visuals = theme.visuals(None);
        for color in [
            visuals.widgets.inactive.bg_fill,
            visuals.widgets.hovered.bg_fill,
            visuals.widgets.active.bg_fill,
            visuals.selection.bg_fill,
            visuals.hyperlink_color,
            visuals.window_fill,
            visuals.error_fg_color,
        ] {
            assert!(
                color.r() == color.g() && color.g() == color.b(),
                "{color:?}"
            );
        }
        assert!(!visuals.text_cursor.blink);
    }
}
