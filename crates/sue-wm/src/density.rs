//! The density of a studio's interface color: Clip Studio Paint's
//! Preferences, Interface, "Adjust density", as Sue does it.
//!
//! A studio's `<section>.theme_density` setting (-100 to 100, 0 leaves
//! the theme as it is) makes the surfaces of its theme (the page, bars,
//! menus, controls, borders) darker (above 0) or lighter (below 0). The
//! text keeps its colors, and the change stops where body or secondary
//! text on the page, a bar or a menu would read worse than 4.5 to 1, so no
//! density makes the interface hard to read.

use egui::Color32;

use crate::Theme;
use crate::ui::tokens::contrast;

/// How far the strongest density moves a surface toward black or white.
const REACH: f32 = 0.6;

/// The least contrast body text keeps against the bars (WCAG AA).
const LEAST: f32 = 4.5;

/// `color` moved `share` of the way to `to`, its alpha kept.
fn toward(color: Color32, to: u8, share: f32) -> Color32 {
    let mix = |value: u8| {
        (f32::from(value) + (f32::from(to) - f32::from(value)) * share)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgba_unmultiplied(mix(color.r()), mix(color.g()), mix(color.b()), color.a())
}

impl Theme {
    /// This theme with its surfaces `amount` denser (above 0: darker,
    /// below 0: lighter; -1 to 1, 0 leaves it). The text keeps its colors;
    /// the change goes no further than keeps body and secondary text on
    /// the page, the bars and the menus at 4.5 to 1 or better (where the
    /// theme reads worse than that already, it never makes it worse).
    #[must_use]
    pub fn with_density(&self, amount: f32) -> Self {
        let amount = amount.clamp(-1.0, 1.0);
        if amount == 0.0 || !amount.is_finite() {
            return self.clone();
        }
        let shaded = |amount: f32| {
            let mut theme = self.clone();
            let (to, share) = if amount > 0.0 {
                (0, amount * REACH)
            } else {
                (255, -amount * REACH)
            };
            for slot in [
                &mut theme.void,
                &mut theme.panel,
                &mut theme.pill,
                &mut theme.hover,
                &mut theme.active,
                &mut theme.border,
                &mut theme.grid,
            ] {
                *slot = toward(*slot, to, share);
            }
            theme
        };
        // Body and secondary text, on the page, the bars and the menus.
        let pairs = |theme: &Self| {
            [theme.void, theme.panel, theme.pill]
                .into_iter()
                .flat_map(|surface| {
                    [
                        contrast(theme.text, surface),
                        contrast(theme.muted, surface),
                    ]
                })
                .collect::<Vec<f32>>()
        };
        let floors: Vec<f32> = pairs(self).into_iter().map(|now| now.min(LEAST)).collect();
        let reads = |theme: &Self| {
            pairs(theme)
                .iter()
                .zip(&floors)
                .all(|(now, floor)| *now >= floor - 1e-3)
        };
        let full = shaded(amount);
        if reads(&full) {
            return full;
        }
        // The most of `amount` that still reads well.
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for _ in 0..20 {
            let middle = (low + high) / 2.0;
            if reads(&shaded(amount * middle)) {
                low = middle;
            } else {
                high = middle;
            }
        }
        shaded(amount * low)
    }

    /// This theme as a studio's `<section>.theme_density` setting (-100
    /// to 100) shades it.
    #[must_use]
    pub fn with_density_from(&self, rice: &sue_rice::RiceSchema, section: &str) -> Self {
        match rice.get_f64(&format!("{section}.theme_density")) {
            Some(percent) if percent != 0.0 => self.with_density((percent / 100.0) as f32),
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color32) -> u32 {
        u32::from(color.r()) + u32::from(color.g()) + u32::from(color.b())
    }

    fn dark() -> Theme {
        Theme::from_rice(&sue_rice::RiceSchema::default())
    }

    #[test]
    fn density_darkens_or_lightens_the_surfaces_and_keeps_the_text() {
        let theme = dark();
        let denser = theme.with_density(0.5);
        let lighter = theme.with_density(-0.5);
        assert!(luminance(denser.panel) < luminance(theme.panel));
        assert!(luminance(lighter.panel) > luminance(theme.panel));
        assert!(luminance(lighter.void) > luminance(theme.void));
        assert_eq!(denser.text, theme.text);
        assert_eq!(lighter.muted, theme.muted);
        assert_eq!(theme.with_density(0.0), theme);
    }

    #[test]
    fn no_density_makes_the_text_hard_to_read() {
        // A light page with dark text, made as dense as it goes.
        let mut paper = dark();
        paper.void = Color32::from_gray(0xf4);
        paper.panel = Color32::from_gray(0xec);
        paper.pill = Color32::from_gray(0xff);
        paper.text = Color32::from_gray(0x1a);
        paper.muted = Color32::from_gray(0x55);
        for amount in [-1.0, -0.5, 0.5, 1.0] {
            for theme in [dark(), paper.clone()] {
                let shaded = theme.with_density(amount);
                let before = [theme.void, theme.panel, theme.pill];
                let after = [shaded.void, shaded.panel, shaded.pill];
                for (was, now) in before.into_iter().zip(after) {
                    for text in [theme.text, theme.muted] {
                        let floor = contrast(text, was).min(4.5);
                        assert!(
                            contrast(text, now) >= floor - 1e-3,
                            "{amount}: {} below {floor}",
                            contrast(text, now)
                        );
                    }
                }
            }
        }
        // It still changes as far as it may.
        assert_ne!(paper.with_density(1.0).panel, paper.panel);
    }

    #[test]
    fn the_setting_is_a_percentage_in_the_studios_section() {
        let mut rice = sue_rice::RiceSchema::default();
        rice.register(&sue_rice::RiceDeclaration::new(
            "studio.theme_density",
            sue_rice::RiceParameter::IntRange {
                value: 40,
                min: Some(-100),
                max: Some(100),
            },
            "density",
        ))
        .unwrap();
        let theme = dark();
        assert_eq!(
            theme.with_density_from(&rice, "studio"),
            theme.with_density(0.4)
        );
        assert_eq!(theme.with_density_from(&rice, "other"), theme);
    }
}
