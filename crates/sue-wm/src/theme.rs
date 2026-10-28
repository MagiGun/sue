//! The live theme: every color and size the shell draws with, read from rice
//! (LAW 3). Missing keys fall back to their declared defaults.

use sue_rice::{Rgba, RiceParameter, RiceSchema};

use crate::{Color, rice_declarations};

/// Colors and metrics for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Workspace background.
    pub background: Color,
    /// Pane surface.
    pub surface: Color,
    /// Cards, fields and buttons.
    pub raised: Color,
    /// Hovered controls.
    pub hover: Color,
    /// Focus ring and highlights.
    pub accent: Color,
    /// Text on accent surfaces.
    pub on_accent: Color,
    /// Errors.
    pub alert: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text and outlines.
    pub muted: Color,
    /// Gap between panes and padding inside them.
    pub gap: f32,
    /// Border thickness.
    pub border_width: f32,
    /// Corner radius.
    pub radius: f32,
    /// Top bar height.
    pub top_bar_height: f32,
    /// UI font size.
    pub ui_size: f32,
    /// Monospaced font size.
    pub mono_size: f32,
    /// Launchpad card width.
    pub card_width: f32,
    /// UI font name.
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
            background: color("colors.background"),
            surface: color("colors.surface"),
            raised: color("colors.raised"),
            hover: color("colors.hover"),
            accent: color("colors.accent"),
            on_accent: color("colors.on_accent"),
            alert: color("colors.alert"),
            text: color("colors.text"),
            muted: color("colors.muted"),
            gap: number("layout.gap"),
            border_width: number("layout.border_width"),
            radius: number("layout.corner_radius"),
            top_bar_height: number("layout.top_bar_height"),
            ui_size: number("typography.ui_size"),
            mono_size: number("typography.mono_size"),
            card_width: number("launchpad.card_width"),
            ui_font: rice
                .get_str("typography.ui_font")
                .unwrap_or("Ubuntu Light")
                .to_owned(),
        }
    }

    /// Padding inside panes: the gap, but never less than a hairline.
    #[must_use]
    pub fn padding(&self) -> f32 {
        self.gap.max(self.border_width.max(1.0) * 2.0)
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
        assert_eq!(theme.gap, 8.0);
        assert_eq!(theme.ui_font, "Ubuntu Light");
        assert_eq!(theme.accent, Color::from_rgba("#c9a227".parse().unwrap()));
        rice.set("layout.gap", RiceValue::Int(20)).unwrap();
        assert_eq!(Theme::from_rice(&rice).gap, 20.0);
    }

    #[test]
    fn missing_keys_use_declared_defaults() {
        let theme = Theme::from_rice(&RiceSchema::new());
        assert_eq!(theme.top_bar_height, 40.0);
        assert_eq!(
            theme.background,
            Color::from_rgba("#14111a".parse().unwrap())
        );
    }
}
