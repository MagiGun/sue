//! The window manager's riceables and the typed config read from them.

use std::time::Duration;

use sue_rice::{Keybinds, RiceDeclaration, RiceError, RiceParameter, RiceSchema};

use crate::{LayoutMetrics, SpringParams, WmError};

/// The rice key that lets the text caret glide (`true`) or jump.
pub const SMOOTH_CARET_KEY: &str = "motion.smooth_caret";
/// The rice key of how long a caret glide takes (ms).
pub const CARET_GLIDE_KEY: &str = "motion.caret_glide_ms";

/// Every parameter the shell reads, with defaults. The shipped
/// `default.suerice` must match these exactly (tested below).
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
            "launchpad.pinned",
            RiceParameter::Text {
                value: String::new(),
            },
            "The studio or tool Sue opens straight into (the pin on its box in the Hub; empty opens the Hub)",
        )
        .labeled("Open At Launch"),
        declare(
            "launchpad.recent_count",
            int(8, 0, 50),
            "How many recent files and projects the Hub lists (0 hides the list)",
        )
        .labeled("Recent Files on the Hub"),
        declare(
            crate::LANGUAGE_KEY,
            RiceParameter::Text {
                value: sue_lang::SYSTEM.to_owned(),
            },
            "Sue's language, which every studio and tool follows unless set apart (system follows the system's language)",
        )
        .labeled("Language"),
        declare(
            "profile.hardware",
            choice("auto", &profiles),
            "Frame pacing and effect budget",
        )
        .labeled("Performance Preference"),
        declare(
            "display.frame_rate_cap",
            int(0, 0, 360),
            "Highest frame rate while animating (0 follows the performance preference)",
        ),
        declare(
            "display.ui_scale",
            float(1.0, 0.75, 2.0),
            "Interface scale on top of the system scale",
        ),
        declare(
            "display.interface",
            choice("auto", &["auto", "desktop", "tablet", "phone"]),
            "Layout: auto follows the device (touch layouts on tablets and phones); pick one to switch",
        )
        .labeled("Interface"),
        declare(
            "display.phone_frame",
            toggle(false),
            "On a computer, draw the phone layout in a phone-wide column in the middle of the window",
        )
        .labeled("Phone Shaped"),
        declare(
            "display.gpu_preference",
            choice("auto", &["auto", "integrated", "discrete"]),
            "GPU that draws the window; auto uses the display's GPU (applies after a restart)",
        ),
        declare(
            "motion.instant_transitions",
            toggle(false),
            "Snap pages, menus and windows in 0 ms instead of animating them",
        ),
        declare(
            "motion.animation_speed",
            float(1.0, 0.25, 4.0),
            "Speed of every spring animation",
        ),
        declare(
            "motion.spring_stiffness",
            float(260.0, 10.0, 1000.0),
            "Spring constant k of every animation",
        ),
        declare(
            "motion.spring_damping",
            float(32.0, 0.0, 120.0),
            "Damping coefficient c of every animation",
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
            SMOOTH_CARET_KEY,
            toggle(true),
            "Let the text caret glide to its new place instead of jumping",
        )
        .labeled("Smooth Caret"),
        declare(
            CARET_GLIDE_KEY,
            int(100, 20, 400),
            "How long the caret takes to glide to its new place (ms; less is faster)",
        )
        .labeled("Caret Glide Time"),
        declare(
            "motion.message_seconds",
            int(4, 1, 30),
            "How long messages stay in the status bar (s)",
        ),
        declare(
            "layout.padding",
            int(8, 2, 24),
            "Space inside bars, pills and controls (px)",
        ),
        declare(
            "layout.border_width",
            int(1, 0, 4),
            "Hairline border thickness (px)",
        ),
        declare(
            "layout.control_radius",
            float(4.0, 0.0, 12.0),
            "Corner rounding of buttons and fields (px)",
        ),
        declare(
            "layout.pill_radius",
            float(12.0, 0.0, 24.0),
            "Corner rounding of windows, menus and pill buttons (px)",
        ),
        declare(
            "layout.pill_height",
            int(34, 24, 64),
            "Height of pill buttons (px)",
        ),
        declare(
            "layout.bar_height",
            int(28, 22, 64),
            "Height of the top bar with the Sue button and the menus (px; touch layouts make it taller)",
        ),
        declare(
            "layout.status_bar",
            toggle(false),
            "Show the thin status bar along the bottom of the window (messages show as a notice without it)",
        )
        .labeled("Status bar"),
        declare(
            "layout.status_bar_height",
            int(22, 16, 48),
            "Height of the status bar (px)",
        ),
        declare(
            "layout.settings_sidebar_width",
            int(190, 140, 360),
            "Width of the category list in Settings (px)",
        ),
        declare(
            "layout.shadow_blur",
            int(18, 0, 48),
            "Blur of the shadow under menus and windows (px)",
        ),
        declare("colors.void", color("#0a0a0a"), "Page background"),
        declare("colors.panel", color("#111111"), "Bars and panels"),
        declare(
            "colors.pill",
            color("#181818"),
            "Menus, boxes and floating surfaces",
        ),
        declare("colors.hover", color("#262626"), "Hovered controls"),
        declare(
            "colors.active",
            color("#333333"),
            "Pressed and chosen controls",
        ),
        declare("colors.border", color("#262626"), "Hairline borders"),
        declare(
            "colors.border_active",
            color("#ffffff"),
            "Borders of hovered boxes and focused controls",
        ),
        declare(
            "colors.text",
            color("#ffffff"),
            "Primary text and active icons",
        ),
        declare("colors.muted", color("#808080"), "Secondary text"),
        declare(
            "colors.selection",
            color("#404040"),
            "Text selection highlight",
        ),
        declare(
            "colors.selection_text",
            color("#ffffff"),
            "Text inside a selection",
        ),
        declare(
            "colors.grid",
            color("#262626"),
            "Grid lines in curve editors",
        ),
        declare(
            "colors.shadow",
            color("#000000"),
            "Shadow under floating surfaces",
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
            int(18, 12, 48),
            "Heading size (px)",
        ),
        declare(
            "typography.mono_size",
            int(12, 8, 32),
            "Monospace text size (px)",
        ),
        declare(
            "input.wheel_line_px",
            int(48, 8, 200),
            "Pixels scrolled per mouse-wheel notch in lists",
        ),
        declare(
            "keys.command_palette",
            chord("Tab"),
            "Open the command palette",
        ),
        declare(
            "keys.command_palette_alt",
            chord("/"),
            "Open the command palette (alternative)",
        ),
        declare("keys.open_settings", chord("Ctrl+,"), "Open Settings"),
        declare("keys.open_file", chord("Ctrl+O"), "Open a file"),
        declare("keys.save", chord("Ctrl+S"), "Save the project"),
        declare("keys.quit", chord("Ctrl+Q"), "Quit Sue"),
        declare("keys.undo", chord("Ctrl+Z"), "Undo the last change"),
        declare("keys.redo", chord("Ctrl+Shift+Z"), "Redo"),
        declare("keys.redo_alt", chord("Ctrl+Y"), "Redo (alternative)"),
    ]
}

/// Registers [`rice_declarations`].
///
/// # Errors
/// If a user value cannot be reconciled with its declaration.
pub fn register_rice(schema: &mut RiceSchema) -> Result<(), RiceError> {
    schema.register_all(&rice_declarations())
}

/// The global shortcuts the shell handles before applets see input. Each
/// action may have any number of chords.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    /// Open the command palette.
    pub command_palette: Keybinds,
    /// Open the command palette (alternative).
    pub command_palette_alt: Keybinds,
    /// Open Settings.
    pub open_settings: Keybinds,
    /// Open a file.
    pub open_file: Keybinds,
    /// Save the project.
    pub save: Keybinds,
    /// Quit.
    pub quit: Keybinds,
    /// Undo.
    pub undo: Keybinds,
    /// Redo.
    pub redo: Keybinds,
    /// Redo (alternative).
    pub redo_alt: Keybinds,
}

/// The typed window-manager config, read from the rice.
#[derive(Clone, Debug, PartialEq)]
pub struct WmConfig {
    /// Layout sizes.
    pub metrics: LayoutMetrics,
    /// Page, menu and window springs.
    pub spring: SpringParams,
    /// Snap everything instead of animating.
    pub instant_transitions: bool,
    /// Multiplier on spring time.
    pub animation_speed: f32,
    /// How long to keep drawing after the last change.
    pub settle_grace: Duration,
    /// Caret blink half period, `None` for a solid caret.
    pub caret_blink: Option<Duration>,
    /// How long status messages stay.
    pub message_duration: Duration,
    /// Frame cap while animating (0 follows the profile).
    pub frame_rate_cap: u32,
    /// Interface scale.
    pub ui_scale: f32,
    /// `display.interface`: `auto`, `desktop`, `tablet` or `phone`.
    pub interface: String,
    /// `display.phone_frame`: on a computer, the phone layout keeps to a
    /// phone-wide column.
    pub phone_frame: bool,
    /// Global shortcuts.
    pub keymap: Keymap,
    /// `profile.hardware`.
    pub hardware_profile: String,
    /// `launchpad.pinned`: the studio or tool Sue opens into, empty for
    /// the Hub.
    pub pinned: String,
    /// `launchpad.recent_count`: how many recent files the Hub lists.
    pub recent_count: usize,
    /// Points per wheel line in lists.
    pub wheel_line_px: f32,
    /// `general.language`: a language tag, or `system`.
    pub language: String,
}

impl WmConfig {
    /// Reads the config from `schema`.
    ///
    /// # Errors
    /// [`WmError::MissingRice`] if a parameter was never registered.
    pub fn from_rice(schema: &RiceSchema) -> Result<Self, WmError> {
        let number = |path: &'static str| {
            schema
                .get_f64(path)
                .map(|value| value as f32)
                .ok_or(WmError::MissingRice(path))
        };
        let key = |path: &'static str| schema.get_keybind(path).ok_or(WmError::MissingRice(path));
        let millis = |path: &'static str| {
            schema
                .get_i64(path)
                .map(|value| Duration::from_millis(u64::try_from(value).unwrap_or(0)))
                .ok_or(WmError::MissingRice(path))
        };
        let caret = millis("motion.caret_blink_ms")?;
        Ok(Self {
            metrics: LayoutMetrics {
                bar_height: number("layout.bar_height")?,
                // Off unless asked for: the layout then gives it no room.
                status_bar_height: if schema.get_bool("layout.status_bar").unwrap_or(false) {
                    number("layout.status_bar_height")?
                } else {
                    0.0
                },
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
            caret_blink: (!caret.is_zero()).then_some(caret),
            message_duration: Duration::from_secs(
                schema
                    .get_i64("motion.message_seconds")
                    .and_then(|seconds| u64::try_from(seconds).ok())
                    .ok_or(WmError::MissingRice("motion.message_seconds"))?,
            ),
            frame_rate_cap: schema
                .get_i64("display.frame_rate_cap")
                .and_then(|cap| u32::try_from(cap).ok())
                .ok_or(WmError::MissingRice("display.frame_rate_cap"))?,
            ui_scale: number("display.ui_scale")?,
            interface: schema
                .get_str("display.interface")
                .unwrap_or("auto")
                .to_owned(),
            phone_frame: schema.get_bool("display.phone_frame").unwrap_or(false),
            keymap: Keymap {
                command_palette: key("keys.command_palette")?,
                command_palette_alt: key("keys.command_palette_alt")?,
                open_settings: key("keys.open_settings")?,
                open_file: key("keys.open_file")?,
                save: key("keys.save")?,
                quit: key("keys.quit")?,
                undo: key("keys.undo")?,
                redo: key("keys.redo")?,
                redo_alt: key("keys.redo_alt")?,
            },
            hardware_profile: schema
                .get_str("profile.hardware")
                .ok_or(WmError::MissingRice("profile.hardware"))?
                .to_owned(),
            pinned: schema
                .get_str("launchpad.pinned")
                .ok_or(WmError::MissingRice("launchpad.pinned"))?
                .trim()
                .to_owned(),
            recent_count: schema
                .get_i64("launchpad.recent_count")
                .and_then(|count| usize::try_from(count).ok())
                .ok_or(WmError::MissingRice("launchpad.recent_count"))?,
            wheel_line_px: number("input.wheel_line_px")?,
            language: schema
                .get_str(crate::LANGUAGE_KEY)
                .unwrap_or(sue_lang::SYSTEM)
                .trim()
                .to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> RiceSchema {
        let mut schema = RiceSchema::new();
        register_rice(&mut schema).unwrap();
        schema
    }

    #[test]
    fn the_config_reads_every_declaration() {
        let config = WmConfig::from_rice(&schema()).unwrap();
        assert_eq!(config.metrics.bar_height, 28.0);
        assert_eq!(
            config.metrics.status_bar_height, 0.0,
            "off unless asked for"
        );
        assert_eq!(config.pinned, "", "nothing pinned: Sue opens on the Hub");
        assert_eq!(config.recent_count, 8);
        assert_eq!(config.keymap.command_palette.to_string(), "Tab");
    }

    #[test]
    fn the_shipped_palette_is_achromatic() {
        for declaration in rice_declarations() {
            if let RiceParameter::ColorHex { value } = &declaration.default {
                assert!(
                    value.r == value.g && value.g == value.b,
                    "{} is not a gray",
                    declaration.path
                );
            }
        }
    }

    #[test]
    fn the_shipped_rice_describes_every_parameter_like_the_code() {
        let shipped = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        for declaration in rice_declarations() {
            let entry = shipped
                .entry(&declaration.path)
                .unwrap_or_else(|| panic!("{} is missing", declaration.path));
            assert_eq!(
                entry.description(),
                Some(declaration.description.as_str()),
                "{}",
                declaration.path
            );
            assert_eq!(
                entry.parameter().value(),
                declaration.default.value(),
                "{}",
                declaration.path
            );
        }
    }
}
