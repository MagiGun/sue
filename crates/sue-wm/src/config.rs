//! The window manager's riceables (LAW 3) and the typed config read from them.

use std::time::Duration;

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
    let toggle = |value| RiceParameter::Bool { value };
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
            "launchpad.boot_into",
            RiceParameter::Text {
                value: "hub".to_owned(),
            },
            "What Sue opens at launch: \"hub\" for Home, or an applet id",
        ),
        declare(
            "launchpad.card_width",
            int(300, 280, 560),
            "Narrowest a Home card may get (px)",
        ),
        declare(
            "profile.hardware",
            choice("auto", &profiles),
            "Frame pacing and effect budget",
        ),
        declare(
            "display.frame_rate_cap",
            int(0, 0, 360),
            "Highest frame rate while animating (0 follows the hardware profile)",
        ),
        declare(
            "display.ui_scale",
            float(1.0, 0.75, 2.0),
            "Interface scale on top of the system scale",
        ),
        declare(
            "display.gpu_preference",
            choice("auto", &["auto", "integrated", "discrete"]),
            "GPU that draws the window; auto uses the display's GPU (applies after a restart)",
        ),
        declare(
            "motion.instant_transitions",
            toggle(false),
            "Snap panels in 0 ms instead of animating them",
        ),
        declare(
            "motion.animation_speed",
            float(1.0, 0.25, 4.0),
            "Speed of panel animations",
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
            "motion.settle_grace_ms",
            int(300, 0, 2000),
            "Keep drawing this long after the last change (ms)",
        ),
        declare(
            "motion.caret_blink_ms",
            int(530, 0, 2000),
            "Caret blink interval (0 keeps the caret solid)",
        ),
        declare(
            "motion.message_seconds",
            int(4, 1, 30),
            "How long messages stay in the status bar (s)",
        ),
        declare("layout.gap", int(0, 0, 32), "Space between panels (px)"),
        declare(
            "layout.padding",
            int(8, 2, 24),
            "Space inside panels and controls (px)",
        ),
        declare(
            "layout.border_width",
            int(1, 0, 4),
            "Hairline divider thickness (px)",
        ),
        declare(
            "layout.panel_radius",
            float(0.0, 0.0, 16.0),
            "Corner rounding of structural panels (px)",
        ),
        declare(
            "layout.control_radius",
            float(3.0, 0.0, 12.0),
            "Corner rounding of buttons, fields and cards (px)",
        ),
        declare(
            "layout.header_height",
            int(30, 24, 64),
            "Height of the header bar (px)",
        ),
        declare(
            "layout.status_bar_height",
            int(22, 16, 48),
            "Height of the status bar (px)",
        ),
        declare(
            "layout.rail_width",
            int(38, 28, 72),
            "Width of the activity rails (px)",
        ),
        declare(
            "layout.left_dock_width",
            int(260, 160, 640),
            "Left dock width (px)",
        ),
        declare(
            "layout.right_dock_width",
            int(280, 160, 720),
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
            "layout.tab_accent",
            int(2, 0, 6),
            "Accent line above the active tab (px)",
        ),
        declare(
            "layout.slider_track",
            int(4, 2, 12),
            "Slider track thickness (px)",
        ),
        declare(
            "layout.settings_sidebar_width",
            int(190, 140, 360),
            "Width of the category list in Settings (px)",
        ),
        declare("colors.base", color("#121212"), "Window and canvas base"),
        declare(
            "colors.panel",
            color("#181818"),
            "Dock and panel background",
        ),
        declare(
            "colors.chrome",
            color("#141414"),
            "Activity rails, header and status bar",
        ),
        declare(
            "colors.surface",
            color("#222222"),
            "Cards, controls and inputs",
        ),
        declare("colors.hover", color("#2a2a2a"), "Hovered controls"),
        declare(
            "colors.border",
            color("#2b2b2b"),
            "Hairline borders and dividers",
        ),
        declare("colors.text", color("#e4e4e4"), "Primary text"),
        declare("colors.muted", color("#888888"), "Secondary text"),
        declare(
            "colors.accent",
            color("#3b82f6"),
            "Focus, selection and the active tab",
        ),
        declare(
            "colors.on_accent",
            color("#ffffff"),
            "Text on accent surfaces",
        ),
        declare(
            "colors.alert",
            color("#e5484d"),
            "Errors and destructive actions",
        ),
        declare(
            "colors.success",
            color("#3fb950"),
            "Healthy and finished states",
        ),
        declare(
            "typography.ui_font",
            choice("Ubuntu Light", &["Ubuntu Light", "Hack"]),
            "Interface font (bundled)",
        ),
        declare(
            "typography.ui_size",
            int(13, 8, 32),
            "Interface text size (px)",
        ),
        declare(
            "typography.small_size",
            int(11, 8, 24),
            "Caption and status bar text size (px)",
        ),
        declare(
            "typography.heading_size",
            int(20, 12, 48),
            "Page title size (px)",
        ),
        declare(
            "typography.mono_size",
            int(12, 8, 32),
            "Code and hex text size (px)",
        ),
        declare(
            "input.wheel_line_px",
            int(48, 8, 200),
            "Pixels scrolled per mouse-wheel notch",
        ),
        declare(
            "keys.toggle_left_dock",
            chord("Ctrl+B"),
            "Show or hide the left dock",
        ),
        declare(
            "keys.toggle_right_dock",
            chord("Ctrl+Alt+B"),
            "Show or hide the right dock",
        ),
        declare("keys.new_tab", chord("Ctrl+T"), "Open a new tab"),
        declare("keys.close_tab", chord("Ctrl+W"), "Close the active tab"),
        declare("keys.next_tab", chord("Ctrl+Tab"), "Switch to the next tab"),
        declare(
            "keys.previous_tab",
            chord("Ctrl+Shift+Tab"),
            "Switch to the previous tab",
        ),
        declare("keys.home", chord("Ctrl+Shift+H"), "Go to the Home tab"),
        declare("keys.open_settings", chord("Ctrl+,"), "Open Settings"),
        declare("keys.open_file", chord("Ctrl+O"), "Open a file"),
        declare("keys.save", chord("Ctrl+S"), "Save the active document"),
        declare("keys.quit", chord("Ctrl+Q"), "Quit Sue"),
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
    /// Open a new tab.
    pub new_tab: Keybind,
    /// Close the active tab.
    pub close_tab: Keybind,
    /// Next tab.
    pub next_tab: Keybind,
    /// Previous tab.
    pub previous_tab: Keybind,
    /// The Home tab.
    pub home: Keybind,
    /// Open Settings.
    pub open_settings: Keybind,
    /// Open a file.
    pub open_file: Keybind,
    /// Save the active document.
    pub save: Keybind,
    /// Quit.
    pub quit: Keybind,
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
    /// Multiplies the time springs see each frame.
    pub animation_speed: f32,
    /// How long frames keep coming after the last change.
    pub settle_grace: Duration,
    /// Caret blink half-period; `None` keeps carets solid.
    pub caret_blink: Option<Duration>,
    /// How long status messages stay.
    pub message_duration: Duration,
    /// Highest animation frame rate; `0` follows the hardware profile.
    pub frame_rate_cap: u32,
    /// Interface scale on top of the system scale.
    pub ui_scale: f32,
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
        let millis = |path: &'static str| {
            schema
                .get_i64(path)
                .map(|value| Duration::from_millis(u64::try_from(value).unwrap_or(0)))
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
        let blink = millis("motion.caret_blink_ms")?;
        Ok(Self {
            metrics: LayoutMetrics {
                gap: number("layout.gap")?,
                header_height: number("layout.header_height")?,
                status_bar_height: number("layout.status_bar_height")?,
                rail_width: number("layout.rail_width")?,
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
            animation_speed: number("motion.animation_speed")?,
            settle_grace: millis("motion.settle_grace_ms")?,
            caret_blink: (!blink.is_zero()).then_some(blink),
            message_duration: Duration::from_secs(
                u64::try_from(
                    schema
                        .get_i64("motion.message_seconds")
                        .ok_or(WmError::MissingRice("motion.message_seconds"))?,
                )
                .unwrap_or(1),
            ),
            frame_rate_cap: schema
                .get_i64("display.frame_rate_cap")
                .and_then(|cap| u32::try_from(cap).ok())
                .ok_or(WmError::MissingRice("display.frame_rate_cap"))?,
            ui_scale: number("display.ui_scale")?,
            keymap: Keymap {
                toggle_left_dock: keybind("keys.toggle_left_dock")?,
                toggle_right_dock: keybind("keys.toggle_right_dock")?,
                new_tab: keybind("keys.new_tab")?,
                close_tab: keybind("keys.close_tab")?,
                next_tab: keybind("keys.next_tab")?,
                previous_tab: keybind("keys.previous_tab")?,
                home: keybind("keys.home")?,
                open_settings: keybind("keys.open_settings")?,
                open_file: keybind("keys.open_file")?,
                save: keybind("keys.save")?,
                quit: keybind("keys.quit")?,
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
                schema
                    .entry(&declaration.path)
                    .and_then(|entry| entry.description()),
                Some(declaration.description.as_str()),
                "{} has a different description",
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
        assert_eq!(config.metrics.gap, 0.0);
        assert_eq!(config.metrics.rail_width, 38.0);
        assert_eq!(config.spring.stiffness, 170.0);
        assert_eq!(config.keymap.redo.to_string(), "Ctrl+Shift+Z");
        assert_eq!(config.keymap.open_settings.to_string(), "Ctrl+,");
        assert_eq!(config.settle_grace, Duration::from_millis(300));
        assert_eq!(config.caret_blink, Some(Duration::from_millis(530)));
        assert_eq!(config.boot_into, "hub");
        assert!(!config.instant_transitions);
        assert!(matches!(
            WmConfig::from_rice(&RiceSchema::new()),
            Err(WmError::MissingRice(_))
        ));
    }
}
