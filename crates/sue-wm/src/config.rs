//! The window manager's riceables and the typed config read from them.

use std::time::Duration;

use sue_rice::{Keybinds, RiceDeclaration, RiceError, RiceParameter, RiceSchema};

use crate::{LayoutMetrics, SpringParams, WmError};

/// Every parameter the shell and its canvas read, with defaults. The shipped
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
            "launchpad.boot_mode",
            RiceParameter::Text {
                value: "hub".to_owned(),
            },
            "Where Sue opens: \"hub\" (the studios), \"canvas\", or a studio's id",
        ),
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
            "display.gpu_preference",
            choice("auto", &["auto", "integrated", "discrete"]),
            "GPU that draws the window; auto uses the display's GPU (applies after a restart)",
        ),
        declare(
            "motion.instant_transitions",
            toggle(false),
            "Snap drawers, pills and the camera in 0 ms instead of animating them",
        ),
        declare(
            "motion.animation_speed",
            float(1.0, 0.25, 4.0),
            "Speed of every spring animation",
        ),
        declare(
            "motion.spring_stiffness",
            float(260.0, 10.0, 1000.0),
            "Drawer and pill spring constant k",
        ),
        declare(
            "motion.spring_damping",
            float(32.0, 0.0, 120.0),
            "Drawer and pill damping coefficient c",
        ),
        declare(
            "motion.camera_stiffness",
            float(600.0, 10.0, 1000.0),
            "Camera spring constant k (frames an entity in about 250 ms)",
        ),
        declare(
            "motion.camera_damping",
            float(49.0, 0.0, 120.0),
            "Camera damping coefficient c",
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
        declare(
            "layout.padding",
            int(8, 2, 24),
            "Space inside drawers, pills and controls (px)",
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
            "Corner rounding of floating pills and cards (px)",
        ),
        declare(
            "layout.pill_height",
            int(34, 24, 64),
            "Height of the floating pills (px)",
        ),
        declare(
            "layout.pill_gap",
            int(12, 0, 64),
            "Space between a selected entity and its pill (px)",
        ),
        declare(
            "layout.rail_width",
            int(36, 28, 72),
            "Width of the activity rails (px)",
        ),
        declare(
            "layout.left_drawer_width",
            int(260, 160, 640),
            "Left drawer width (px)",
        ),
        declare(
            "layout.right_drawer_width",
            int(280, 160, 720),
            "Right drawer width (px)",
        ),
        declare(
            "layout.timeline_height",
            int(240, 120, 600),
            "Bottom timeline height (px)",
        ),
        declare(
            "layout.status_bar_height",
            int(22, 16, 48),
            "Height of the status bar (px)",
        ),
        declare(
            "layout.min_canvas_width",
            int(420, 240, 1600),
            "Narrowest canvas before desktop drawers shrink (px)",
        ),
        declare(
            "layout.drawer_max_fraction",
            float(0.85, 0.5, 1.0),
            "Widest a phone drawer may be, as a share of the screen",
        ),
        declare(
            "layout.sheet_max_fraction",
            float(0.5, 0.2, 0.9),
            "Tallest the phone timeline sheet may be, as a share of the screen",
        ),
        declare(
            "layout.settings_sidebar_width",
            int(190, 140, 360),
            "Width of the category list in Settings (px)",
        ),
        declare(
            "layout.radial_radius",
            int(104, 64, 200),
            "Radius of the right-click marking menu (px)",
        ),
        declare(
            "layout.shadow_blur",
            int(18, 0, 48),
            "Blur of the shadow under pills and windows (px)",
        ),
        declare(
            "canvas.zoom_min",
            float(0.1, 0.05, 1.0),
            "Farthest the canvas zooms out (0.1 is 10%)",
        ),
        declare(
            "canvas.zoom_max",
            float(32.0, 2.0, 64.0),
            "Closest the canvas zooms in (32 is 3200%)",
        ),
        declare(
            "canvas.zoom_step",
            float(0.18, 0.02, 1.0),
            "Zoom change per mouse-wheel notch (natural log)",
        ),
        declare(
            "canvas.grid_spacing",
            int(32, 8, 256),
            "Distance between grid dots at 100% (px)",
        ),
        declare(
            "canvas.grid_style",
            choice("dots", &["dots", "lines", "none"]),
            "Canvas background",
        ),
        declare(
            "canvas.frame_margin",
            int(48, 0, 400),
            "Space kept around an entity the camera frames (px)",
        ),
        declare("colors.void", color("#0a0a0a"), "Canvas background"),
        declare("colors.panel", color("#111111"), "Drawers and panels"),
        declare(
            "colors.pill",
            color("#181818"),
            "Floating pills, cards and entity frames",
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
            "Borders of selected entities and focused controls",
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
            "Canvas grid dots and lines",
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
            "Code and hex text size (px)",
        ),
        declare(
            "typography.document_size",
            int(15, 8, 48),
            "Default text size of new documents (px)",
        ),
        declare(
            "input.wheel_line_px",
            int(48, 8, 200),
            "Pixels scrolled per mouse-wheel notch in lists",
        ),
        declare(
            "keys.toggle_left_drawer",
            chord("Ctrl+B"),
            "Show or hide the left drawer",
        ),
        declare(
            "keys.toggle_right_drawer",
            chord("Ctrl+Alt+B"),
            "Show or hide the right drawer",
        ),
        declare(
            "keys.toggle_timeline",
            chord("Ctrl+J"),
            "Show or hide the timeline",
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
        declare(
            "keys.play_pause",
            chord("Space"),
            "Play or pause the timeline (tap; hold and drag pans)",
        ),
        declare(
            "keys.reset_camera",
            chord("Ctrl+0"),
            "Frame everything on the canvas",
        ),
        declare(
            "keys.delete",
            chord("Delete"),
            "Remove the selected entity from the canvas",
        ),
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
    /// Show or hide the left drawer.
    pub toggle_left_drawer: Keybinds,
    /// Show or hide the right drawer.
    pub toggle_right_drawer: Keybinds,
    /// Show or hide the timeline.
    pub toggle_timeline: Keybinds,
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
    /// Play or pause the timeline.
    pub play_pause: Keybinds,
    /// Frame everything.
    pub reset_camera: Keybinds,
    /// Remove the selected entity from the canvas.
    pub delete: Keybinds,
}

/// Canvas behavior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasConfig {
    /// Farthest zoom out.
    pub zoom_min: f32,
    /// Closest zoom in.
    pub zoom_max: f32,
    /// Log-zoom change per wheel notch.
    pub zoom_step: f32,
    /// Grid spacing at 100%.
    pub grid_spacing: f32,
    /// `dots`, `lines` or `none`.
    pub grid_style: GridStyle,
    /// Margin around a framed entity, in screen points.
    pub frame_margin: f32,
}

/// The canvas background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridStyle {
    /// A dot at every grid point.
    Dots,
    /// Thin grid lines.
    Lines,
    /// Plain background.
    None,
}

/// The typed window-manager config, read from the rice.
#[derive(Clone, Debug, PartialEq)]
pub struct WmConfig {
    /// Layout sizes.
    pub metrics: LayoutMetrics,
    /// Drawer, pill and menu springs.
    pub spring: SpringParams,
    /// Camera springs.
    pub camera_spring: SpringParams,
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
    /// Global shortcuts.
    pub keymap: Keymap,
    /// `profile.hardware`.
    pub hardware_profile: String,
    /// `launchpad.boot_mode`.
    pub boot_mode: String,
    /// Points per wheel line in lists.
    pub wheel_line_px: f32,
    /// Canvas behavior.
    pub canvas: CanvasConfig,
    /// Pill height.
    pub pill_height: f32,
    /// Gap between an entity and its pill.
    pub pill_gap: f32,
    /// Radius of the marking menu.
    pub radial_radius: f32,
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
        let grid_style = match schema.get_str("canvas.grid_style") {
            Some("lines") => GridStyle::Lines,
            Some("none") => GridStyle::None,
            _ => GridStyle::Dots,
        };
        Ok(Self {
            metrics: LayoutMetrics {
                rail_width: number("layout.rail_width")?,
                left_drawer_width: number("layout.left_drawer_width")?,
                right_drawer_width: number("layout.right_drawer_width")?,
                timeline_height: number("layout.timeline_height")?,
                status_bar_height: number("layout.status_bar_height")?,
                min_canvas_width: number("layout.min_canvas_width")?,
                drawer_max_fraction: number("layout.drawer_max_fraction")?,
                sheet_max_fraction: number("layout.sheet_max_fraction")?,
            },
            spring: SpringParams {
                stiffness: number("motion.spring_stiffness")?,
                damping: number("motion.spring_damping")?,
                mass: 1.0,
            },
            camera_spring: SpringParams {
                stiffness: number("motion.camera_stiffness")?,
                damping: number("motion.camera_damping")?,
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
            keymap: Keymap {
                toggle_left_drawer: key("keys.toggle_left_drawer")?,
                toggle_right_drawer: key("keys.toggle_right_drawer")?,
                toggle_timeline: key("keys.toggle_timeline")?,
                command_palette: key("keys.command_palette")?,
                command_palette_alt: key("keys.command_palette_alt")?,
                open_settings: key("keys.open_settings")?,
                open_file: key("keys.open_file")?,
                save: key("keys.save")?,
                quit: key("keys.quit")?,
                undo: key("keys.undo")?,
                redo: key("keys.redo")?,
                redo_alt: key("keys.redo_alt")?,
                play_pause: key("keys.play_pause")?,
                reset_camera: key("keys.reset_camera")?,
                delete: key("keys.delete")?,
            },
            hardware_profile: schema
                .get_str("profile.hardware")
                .ok_or(WmError::MissingRice("profile.hardware"))?
                .to_owned(),
            boot_mode: schema
                .get_str("launchpad.boot_mode")
                .ok_or(WmError::MissingRice("launchpad.boot_mode"))?
                .trim()
                .to_owned(),
            wheel_line_px: number("input.wheel_line_px")?,
            canvas: CanvasConfig {
                zoom_min: number("canvas.zoom_min")?,
                zoom_max: number("canvas.zoom_max")?,
                zoom_step: number("canvas.zoom_step")?,
                grid_spacing: number("canvas.grid_spacing")?,
                grid_style,
                frame_margin: number("canvas.frame_margin")?,
            },
            pill_height: number("layout.pill_height")?,
            pill_gap: number("layout.pill_gap")?,
            radial_radius: number("layout.radial_radius")?,
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
        assert_eq!(config.metrics.rail_width, 36.0);
        assert_eq!(config.canvas.zoom_min, 0.1);
        assert_eq!(config.canvas.zoom_max, 32.0);
        assert_eq!(config.boot_mode, "hub");
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
