//! The window manager's riceables (LAW 3) and the typed config read from them.

use sue_rice::{Keybind, RiceDeclaration, RiceError, RiceParameter, RiceSchema};

use crate::{LayoutMetrics, SpringParams, WmError};

/// Every parameter the window manager and its renderer read, with defaults.
/// The shipped `default.suerice` must match these exactly (tested below).
#[must_use]
pub fn rice_declarations() -> Vec<RiceDeclaration> {
    let int = |value, min, max| RiceParameter::IntRange {
        value,
        min: Some(min),
        max: Some(max),
    };
    let float = |value, min, max| RiceParameter::FloatRange {
        value,
        min: Some(min),
        max: Some(max),
    };
    let chord = |text: &str| RiceParameter::Keybind {
        value: text
            .parse()
            .unwrap_or_else(|error| panic!("built-in keybind {text}: {error}")),
    };
    let color = |hex: &str| RiceParameter::ColorHex {
        value: hex
            .parse()
            .unwrap_or_else(|error| panic!("built-in color {hex}: {error}")),
    };
    let choice = |value: &str, options: &[&str]| RiceParameter::EnumChoice {
        value: value.to_owned(),
        options: options.iter().map(|&option| option.to_owned()).collect(),
    };
    let profiles = [
        "auto",
        "high-refresh",
        "balanced",
        "legacy-low-spec",
        "mobile-battery-saver",
    ];
    let declare = RiceDeclaration::new;
    vec![
        declare(
            "profile.hardware",
            choice("auto", &profiles),
            "Frame pacing and shader budget",
        ),
        declare(
            "launchpad.boot_into",
            RiceParameter::Text {
                value: "hub".to_owned(),
            },
            "Applet id to open at launch, or \"hub\" for the Launchpad",
        ),
        declare(
            "launchpad.card_width",
            int(240, 160, 480),
            "Width of Launchpad cards (px)",
        ),
        declare(
            "layout.gap",
            int(8, 0, 64),
            "Space between tiled panes (px)",
        ),
        declare(
            "layout.border_width",
            int(1, 0, 16),
            "Pane border thickness (px)",
        ),
        declare(
            "layout.corner_radius",
            float(8.0, 0.0, 24.0),
            "Pane corner rounding (px)",
        ),
        declare(
            "layout.top_bar_height",
            int(40, 28, 96),
            "Height of the top bar (px)",
        ),
        declare(
            "layout.left_dock_width",
            int(300, 160, 640),
            "Left dock width (px)",
        ),
        declare(
            "layout.right_dock_width",
            int(340, 160, 720),
            "Right dock width (px)",
        ),
        declare(
            "layout.timeline_height",
            int(220, 96, 600),
            "Bottom timeline height (px)",
        ),
        declare(
            "layout.min_center_width",
            int(480, 240, 1600),
            "Narrowest Center Stage before docks float (px)",
        ),
        declare(
            "layout.drawer_max_fraction",
            float(0.85, 0.5, 1.0),
            "Widest a phone drawer may be, as a share of the screen",
        ),
        declare(
            "layout.sheet_max_fraction",
            float(0.4, 0.2, 0.9),
            "Tallest the phone timeline sheet may peek, as a share of the screen",
        ),
        declare(
            "motion.instant_transitions",
            RiceParameter::Bool { value: false },
            "Snap panels in 0 ms instead of animating them",
        ),
        declare(
            "motion.spring_stiffness",
            float(170.0, 10.0, 1000.0),
            "Spring constant k",
        ),
        declare(
            "motion.spring_damping",
            float(26.0, 0.0, 120.0),
            "Damping coefficient c",
        ),
        declare(
            "colors.background",
            color("#14111a"),
            "Workspace background",
        ),
        declare("colors.surface", color("#1f1a26"), "Pane surface"),
        declare(
            "colors.raised",
            color("#2a2333"),
            "Cards, fields and buttons",
        ),
        declare("colors.hover", color("#382e44"), "Hovered controls"),
        declare(
            "colors.accent",
            color("#c9a227"),
            "Focus ring and highlights",
        ),
        declare(
            "colors.on_accent",
            color("#14111a"),
            "Text on accent surfaces",
        ),
        declare(
            "colors.alert",
            color("#a4243b"),
            "Errors and destructive actions",
        ),
        declare("colors.text", color("#e6dfd3"), "Primary text"),
        declare(
            "colors.muted",
            color("#8f8599"),
            "Secondary text and outlines",
        ),
        declare(
            "typography.ui_font",
            choice("Ubuntu Light", &["Ubuntu Light", "Hack"]),
            "UI font (bundled)",
        ),
        declare("typography.ui_size", int(14, 8, 32), "UI font size (px)"),
        declare(
            "typography.mono_size",
            int(13, 8, 32),
            "Code and hex font size (px)",
        ),
        declare(
            "input.wheel_line_px",
            int(48, 8, 200),
            "Pixels scrolled per mouse-wheel notch",
        ),
        declare(
            "keys.toggle_left_dock",
            chord("Super+B"),
            "Show or hide the left dock",
        ),
        declare(
            "keys.toggle_right_dock",
            chord("Super+N"),
            "Show or hide the right dock",
        ),
        declare(
            "keys.new_tab",
            chord("Ctrl+T"),
            "Open a new tab on the Launchpad",
        ),
        declare("keys.undo", chord("Ctrl+Z"), "Undo the last change"),
        declare("keys.redo", chord("Ctrl+Shift+Z"), "Redo"),
        declare("keys.redo_alt", chord("Ctrl+Y"), "Redo (alternative)"),
    ]
}

/// Registers every window-manager parameter with `schema`, keeping the
/// user's compatible values.
///
/// # Errors
/// Only if a declared path is invalid, which would be a bug in this crate.
pub fn register_rice(schema: &mut RiceSchema) -> Result<(), RiceError> {
    schema.register_all(&rice_declarations())
}

/// The global shortcuts the window manager handles before applets see input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keymap {
    /// Show or hide the left dock.
    pub toggle_left_dock: Keybind,
    /// Show or hide the right dock.
    pub toggle_right_dock: Keybind,
    /// Open a new tab on the Hub.
    pub new_tab: Keybind,
    /// Undo.
    pub undo: Keybind,
    /// Redo.
    pub redo: Keybind,
    /// Redo, second binding.
    pub redo_alt: Keybind,
}

/// Window manager settings, read from rice.
#[derive(Clone, Debug, PartialEq)]
pub struct WmConfig {
    /// Layout sizes.
    pub metrics: LayoutMetrics,
    /// Spring constants for panel motion.
    pub spring: SpringParams,
    /// `true` turns every spring into an instant snap.
    pub instant_transitions: bool,
    /// Global shortcuts.
    pub keymap: Keymap,
    /// `profile.hardware`: `"auto"` or a profile name.
    pub hardware_profile: String,
    /// `launchpad.boot_into`: an applet id, or `"hub"`.
    pub boot_into: String,
    /// Pixels scrolled per wheel notch.
    pub wheel_line_px: f32,
}

impl WmConfig {
    /// Reads the config from a schema on which [`register_rice`] has run.
    ///
    /// # Errors
    /// [`WmError::MissingRice`] for a missing or mistyped parameter.
    pub fn from_rice(schema: &RiceSchema) -> Result<Self, WmError> {
        let number = |path: &'static str| {
            schema
                .get_f64(path)
                .map(|value| value as f32)
                .ok_or(WmError::MissingRice(path))
        };
        let keybind =
            |path: &'static str| schema.get_keybind(path).ok_or(WmError::MissingRice(path));
        let text = |path: &'static str| {
            schema
                .get_str(path)
                .map(str::to_owned)
                .ok_or(WmError::MissingRice(path))
        };
        Ok(Self {
            metrics: LayoutMetrics {
                gap: number("layout.gap")?,
                left_dock_width: number("layout.left_dock_width")?,
                right_dock_width: number("layout.right_dock_width")?,
                timeline_height: number("layout.timeline_height")?,
                min_center_width: number("layout.min_center_width")?,
                drawer_max_fraction: number("layout.drawer_max_fraction")?,
                sheet_max_fraction: number("layout.sheet_max_fraction")?,
            },
            spring: SpringParams {
                stiffness: number("motion.spring_stiffness")?,
                damping: number("motion.spring_damping")?,
                mass: 1.0,
            },
            instant_transitions: schema
                .get_bool("motion.instant_transitions")
                .ok_or(WmError::MissingRice("motion.instant_transitions"))?,
            keymap: Keymap {
                toggle_left_dock: keybind("keys.toggle_left_dock")?,
                toggle_right_dock: keybind("keys.toggle_right_dock")?,
                new_tab: keybind("keys.new_tab")?,
                undo: keybind("keys.undo")?,
                redo: keybind("keys.redo")?,
                redo_alt: keybind("keys.redo_alt")?,
            },
            hardware_profile: text("profile.hardware")?,
            boot_into: text("launchpad.boot_into")?,
            wheel_line_px: number("input.wheel_line_px")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_rice::{DEFAULT_RICE, RegisterOutcome};

    #[test]
    fn shipped_rice_matches_the_declarations_exactly() {
        let mut schema = RiceSchema::parse(DEFAULT_RICE).unwrap();
        let revision = schema.revision();
        for declaration in rice_declarations() {
            assert_eq!(
                schema.parameter(&declaration.path),
                Some(&declaration.default),
                "{} differs from its declaration",
                declaration.path
            );
            assert_eq!(
                schema.register(&declaration),
                Ok(RegisterOutcome::KeptUserValue)
            );
        }
        assert_eq!(schema.revision(), revision);
        assert!(schema.diagnostics().is_empty());
    }

    #[test]
    fn config_reads_from_registered_rice() {
        let mut schema = RiceSchema::new();
        register_rice(&mut schema).unwrap();
        let config = WmConfig::from_rice(&schema).unwrap();
        assert_eq!(config.metrics.gap, 8.0);
        assert_eq!(config.spring.stiffness, 170.0);
        assert_eq!(config.keymap.redo.to_string(), "Ctrl+Shift+Z");
        assert_eq!(config.keymap.redo_alt.to_string(), "Ctrl+Y");
        assert_eq!(config.boot_into, "hub");
        assert!(!config.instant_transitions);
        assert!(matches!(
            WmConfig::from_rice(&RiceSchema::new()),
            Err(WmError::MissingRice(_))
        ));
    }
}
