//! The live theme: every color and size the shell draws with, read from rice
//! (LAW 3). Missing keys fall back to their declared defaults.

use sue_rice::{Rgba, RiceParameter, RiceSchema};

use crate::{Color, rice_declarations};

/// Colors and metrics for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Window and canvas base.
    pub base: Color,
    /// Dock and panel background.
    pub panel: Color,
    /// Activity rails, header and status bar.
    pub chrome: Color,
    /// Cards, controls and inputs.
    pub surface: Color,
    /// Hovered controls.
    pub hover: Color,
    /// Hairline borders and dividers.
    pub border: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text.
    pub muted: Color,
    /// Focus, selection and the active tab.
    pub accent: Color,
    /// Text on accent surfaces.
    pub on_accent: Color,
    /// Errors and destructive actions.
    pub alert: Color,
    /// Healthy and finished states.
    pub success: Color,
    /// Space between panels.
    pub gap: f32,
    /// Space inside panels and controls.
    pub padding: f32,
    /// Hairline divider thickness.
    pub border_width: f32,
    /// Corner rounding of structural panels.
    pub panel_radius: f32,
    /// Corner rounding of buttons, fields and cards.
    pub radius: f32,
    /// Header bar height.
    pub header_height: f32,
    /// Status bar height.
    pub status_bar_height: f32,
    /// Activity rail width.
    pub rail_width: f32,
    /// Accent line above the active tab.
    pub tab_accent: f32,
    /// Slider track thickness.
    pub slider_track: f32,
    /// Width of the Settings category list.
    pub settings_sidebar_width: f32,
    /// Narrowest Home card.
    pub card_width: f32,
    /// Interface text size.
    pub ui_size: f32,
    /// Caption and status text size.
    pub small_size: f32,
    /// Page title size.
    pub heading_size: f32,
    /// Code and hex text size.
    pub mono_size: f32,
    /// Interface font name.
    pub ui_font: String,
}

impl Theme {
    /// Reads the theme from `rice`.
    #[must_use]
    pub fn from_rice(rice: &RiceSchema) -> Self {
        let declarations = rice_declarations();
        let declared = |path: &str| {
            declarations
                .iter()
                .find(|declaration| declaration.path == path)
                .map(|declaration| &declaration.default)
        };
        let number = |path: &str| {
            let fallback = match declared(path) {
                Some(RiceParameter::IntRange { value, .. }) => *value as f64,
                Some(RiceParameter::FloatRange { value, .. }) => *value,
                _ => 0.0,
            };
            rice.get_f64(path).unwrap_or(fallback) as f32
        };
        let color = |path: &str| {
            let fallback = match declared(path) {
                Some(RiceParameter::ColorHex { value }) => *value,
                _ => Rgba::new(255, 0, 255, 255),
            };
            Color::from_rgba(rice.get_color(path).unwrap_or(fallback))
        };
        Self {
            base: color("colors.base"),
            panel: color("colors.panel"),
            chrome: color("colors.chrome"),
            surface: color("colors.surface"),
            hover: color("colors.hover"),
            border: color("colors.border"),
            text: color("colors.text"),
            muted: color("colors.muted"),
            accent: color("colors.accent"),
            on_accent: color("colors.on_accent"),
            alert: color("colors.alert"),
            success: color("colors.success"),
            gap: number("layout.gap"),
            padding: number("layout.padding"),
            border_width: number("layout.border_width"),
            panel_radius: number("layout.panel_radius"),
            radius: number("layout.control_radius"),
            header_height: number("layout.header_height"),
            status_bar_height: number("layout.status_bar_height"),
            rail_width: number("layout.rail_width"),
            tab_accent: number("layout.tab_accent"),
            slider_track: number("layout.slider_track"),
            settings_sidebar_width: number("layout.settings_sidebar_width"),
            card_width: number("launchpad.card_width"),
            ui_size: number("typography.ui_size"),
            small_size: number("typography.small_size"),
            heading_size: number("typography.heading_size"),
            mono_size: number("typography.mono_size"),
            ui_font: rice
                .get_str("typography.ui_font")
                .unwrap_or("Ubuntu Light")
                .to_owned(),
        }
    }

    /// A hairline: the border width, but never thinner than one pixel when
    /// borders are on.
    #[must_use]
    pub fn hairline(&self) -> f32 {
        if self.border_width <= 0.0 {
            0.0
        } else {
            self.border_width.max(1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_rice::{DEFAULT_RICE, RiceValue};

    #[test]
    fn theme_follows_the_rice() {
        let mut rice = RiceSchema::parse(DEFAULT_RICE).unwrap();
        let theme = Theme::from_rice(&rice);
        assert_eq!(theme.gap, 0.0);
        assert_eq!(theme.rail_width, 38.0);
        assert_eq!(theme.ui_font, "Ubuntu Light");
        assert_eq!(theme.accent, Color::from_rgba("#3b82f6".parse().unwrap()));
        assert_eq!(theme.hairline(), 1.0);
        rice.set("layout.gap", RiceValue::Int(20)).unwrap();
        assert_eq!(Theme::from_rice(&rice).gap, 20.0);
    }

    #[test]
    fn missing_keys_use_declared_defaults() {
        let theme = Theme::from_rice(&RiceSchema::new());
        assert_eq!(theme.header_height, 30.0);
        assert_eq!(theme.base, Color::from_rgba("#121212".parse().unwrap()));
    }
}
