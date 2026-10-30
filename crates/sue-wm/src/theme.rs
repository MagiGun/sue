//! The visual theme: every color, size and font comes from the rice
//! and is bound straight into `egui::Style` and `egui::Visuals`. The shipped
//! palette is pure monochrome, from `#000000` to `#ffffff`.

use std::time::Duration;

use egui::epaint::Shadow;
use egui::style::{Selection, TextCursorStyle, WidgetVisuals, Widgets};
use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};
use sue_rice::{Rgba, RiceSchema, ThemePreset};

/// Colors, geometry and type for the whole shell.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Page background.
    pub void: Color32,
    /// Bars and panels.
    pub panel: Color32,
    /// Menus, boxes and floating surfaces.
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
    /// Space inside bars, pills and controls.
    pub padding: f32,
    /// Hairline thickness.
    pub border_width: f32,
    /// Rounding of buttons and fields.
    pub control_radius: f32,
    /// Rounding of windows, menus and pill buttons.
    pub pill_radius: f32,
    /// Height of pill buttons.
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
    /// Monospace text.
    pub mono_size: f32,
}

fn color(rgba: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(rgba.r, rgba.g, rgba.b, rgba.a)
}

/// The color keys of a theme, as `colors.<key>` names them.
pub const COLOR_KEYS: [&str; 13] = [
    "void",
    "panel",
    "pill",
    "hover",
    "active",
    "border",
    "border_active",
    "text",
    "muted",
    "selection",
    "selection_text",
    "grid",
    "shadow",
];

/// What a studio's `<section>.theme` may say besides a preset's id.
pub const STUDIO_THEME_SUE: &str = "sue-theme";

/// A studio's own colors, in `<section>_colors`.
pub const STUDIO_THEME_CUSTOM: &str = "custom";

impl Theme {
    /// The color named by `key` (one of [`COLOR_KEYS`]).
    fn color_mut(&mut self, key: &str) -> Option<&mut Color32> {
        Some(match key {
            "void" => &mut self.void,
            "panel" => &mut self.panel,
            "pill" => &mut self.pill,
            "hover" => &mut self.hover,
            "active" => &mut self.active,
            "border" => &mut self.border,
            "border_active" => &mut self.border_active,
            "text" => &mut self.text,
            "muted" => &mut self.muted,
            "selection" => &mut self.selection,
            "selection_text" => &mut self.selection_text,
            "grid" => &mut self.grid,
            "shadow" => &mut self.shadow,
            _ => return None,
        })
    }

    /// This theme with a preset's colors (its `colors.*` values).
    #[must_use]
    pub fn with_preset(&self, preset: &ThemePreset) -> Self {
        let mut theme = self.clone();
        for (path, value) in &preset.values {
            if let (Some(key), sue_rice::RiceValue::Color(rgba)) =
                (path.strip_prefix("colors."), value)
                && let Some(slot) = theme.color_mut(key)
            {
                *slot = color(rgba.to_owned());
            }
        }
        theme
    }

    /// This theme with the colors kept in `section` of `rice` (keys as in
    /// [`COLOR_KEYS`]); colors the section lacks stay.
    #[must_use]
    pub fn with_colors_from(&self, rice: &RiceSchema, section: &str) -> Self {
        let mut theme = self.clone();
        for key in COLOR_KEYS {
            if let Some(rgba) = rice.get_color(&format!("{section}.{key}"))
                && let Some(slot) = theme.color_mut(key)
            {
                *slot = color(rgba);
            }
        }
        theme
    }

    /// The theme a studio wears: `<section>.theme` names a preset, the Sue
    /// theme (this one, so it follows every change to Sue's colors), or its
    /// own colors in `<section>_colors`. A studio with no such setting
    /// wears this theme.
    #[must_use]
    pub fn for_studio(&self, rice: &RiceSchema, section: &str, presets: &[ThemePreset]) -> Self {
        match rice.get_str(&format!("{section}.theme")) {
            None | Some(STUDIO_THEME_SUE) => self.clone(),
            Some(STUDIO_THEME_CUSTOM) => self.with_colors_from(rice, &format!("{section}_colors")),
            Some(id) => presets
                .iter()
                .find(|preset| preset.id == id)
                .map_or_else(|| self.clone(), |preset| self.with_preset(preset)),
        }
    }

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
        }
    }

    /// `true` when the page is light (dark text on it), as in Paper.
    #[must_use]
    pub fn is_light(&self) -> bool {
        let [r, g, b, _] = self.void.to_array();
        0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b) > 127.5
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
        // A light theme (Paper) starts from egui's light visuals, so what
        // egui picks by itself (hints, scroll bars) reads on light pages.
        let mut visuals = if self.is_light() {
            Visuals::light()
        } else {
            Visuals::dark()
        };
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
        // Slider handles are drawn as rectangles of this unusual shape so
        // that [`reshape_handles`] can find them and give them Sue's look.
        visuals.handle_shape = egui::style::HandleShape::Rect {
            aspect_ratio: HANDLE_MARK,
        };
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
            // The bundled fonts, with the interface font first, and every
            // font a studio has installed so far.
            ctx.set_fonts(crate::fonts::definitions(&self.ui_font));
        }
    }
}

/// The width over height egui draws slider handles at, so that
/// [`reshape_handles`] knows them from every other rectangle.
pub const HANDLE_MARK: f32 = 0.314_159;

/// A slider handle's height, as a share of the circle egui would draw.
const HANDLE_HEIGHT: f32 = 0.72;
/// A slider handle's width, as a share of that circle.
const HANDLE_WIDTH: f32 = 0.3;

/// Gives every slider handle painted this frame Sue's look: a thin,
/// slightly tall rectangle `---[]---`, a little shorter than egui's circle
/// was. egui sizes a handle from the slider's height alone, so the handles
/// are drawn in an unusual shape ([`HANDLE_MARK`]) and resized here, every
/// slider in Sue at once. Call it once the frame's interface is drawn.
pub fn reshape_handles(ctx: &egui::Context) {
    let layers: Vec<egui::LayerId> = ctx.memory(|memory| memory.layer_ids().collect());
    ctx.graphics_mut(|graphics| {
        for layer in layers {
            let Some(list) = graphics.get_mut(layer) else {
                continue;
            };
            for index in 0..list.next_idx().0 {
                list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                    reshape(&mut clipped.shape);
                });
            }
        }
    });
}

/// [`reshape_handles`] for one shape and the shapes inside it.
fn reshape(shape: &mut egui::Shape) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter_mut().for_each(reshape),
        egui::Shape::Rect(rect) => {
            let size = rect.rect.size();
            if size.x <= 0.0 || size.y <= 0.0 {
                return;
            }
            // Across the rail and along it: a vertical slider's handle
            // lies on its side.
            let (across, along, upright) = if size.x <= size.y {
                (size.y, size.x, true)
            } else {
                (size.x, size.y, false)
            };
            // egui makes a handle exactly that shape (to float rounding);
            // a looser test caught other boxes that came near it, such as a
            // 140 by 44 list on a phone, and shrank them.
            if ((along / across) - HANDLE_MARK).abs() > 2e-5 {
                return;
            }
            let height = across * HANDLE_HEIGHT;
            let width = (across * HANDLE_WIDTH).max(3.0);
            let size = if upright {
                egui::vec2(width, height)
            } else {
                egui::vec2(height, width)
            };
            rect.rect = egui::Rect::from_center_size(rect.rect.center(), size);
            // Square, not a pill: `[]`.
            rect.corner_radius = CornerRadius::same(1);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_handles_become_thin_and_a_little_shorter() {
        let ctx = egui::Context::default();
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        crate::register_rice(&mut rice).unwrap();
        Theme::from_rice(&rice).apply(&ctx, None, None);
        let mut value = 0.5_f32;
        let mut tall = 0.5_f32;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(egui::Slider::new(&mut value, 0.0..=1.0));
            ui.add(egui::Slider::new(&mut tall, 0.0..=1.0).vertical());
            ui.add(egui::Button::new("A button"));
            reshape_handles(ui.ctx());
        });
        output.textures_delta.clear();
        let mut handles = Vec::new();
        let mut collect = |shape: &egui::Shape| {
            if let egui::Shape::Rect(rect) = shape {
                handles.push(rect.rect);
            }
        };
        for clipped in &output.shapes {
            match &clipped.shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut collect),
                shape => collect(shape),
            }
        }
        let thin: Vec<_> = handles
            .iter()
            .filter(|rect| {
                let (long, short) = (
                    rect.width().max(rect.height()),
                    rect.width().min(rect.height()),
                );
                (short / long - HANDLE_WIDTH / HANDLE_HEIGHT).abs() < 0.01
            })
            .collect();
        assert_eq!(thin.len(), 2, "both handles: {handles:?}");
        assert!(
            thin[0].height() > thin[0].width(),
            "upright on a level slider"
        );
        assert!(
            thin[1].width() > thin[1].height(),
            "on its side on a vertical one"
        );
        // A box that only comes near the mark keeps its size.
        let mut near = egui::Shape::rect_filled(
            egui::Rect::from_min_size(egui::pos2(4.0, 838.0), egui::vec2(140.0, 44.0)),
            4.0,
            egui::Color32::GRAY,
        );
        reshape(&mut near);
        let egui::Shape::Rect(near) = near else {
            panic!("still a rectangle");
        };
        assert_eq!(near.rect.width(), 140.0);
        // Nothing keeps egui's own shape.
        assert!(
            handles
                .iter()
                .all(|rect| ((rect.width() / rect.height()) - HANDLE_MARK).abs() > 1e-3)
        );
    }

    #[test]
    fn studios_wear_a_preset_sues_theme_or_their_own_colors() {
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        crate::register_rice(&mut rice).unwrap();
        let presets = sue_rice::builtin_presets();
        let sue = Theme::from_rice(&rice);
        // Sue Paint ships in Gray.
        let paint = sue.for_studio(&rice, "sue_paint", &presets);
        assert_eq!(paint.panel, Color32::from_gray(0x3a));
        assert_eq!(paint.padding, sue.padding, "only the colors change");
        // A studio with no theme setting wears Sue's.
        assert_eq!(sue.for_studio(&rice, "no_such_studio", &presets), sue);
        // Sue theme follows Sue's colors, even after they change.
        rice.set(
            "sue_paint.theme",
            sue_rice::RiceValue::Text(STUDIO_THEME_SUE.to_owned()),
        )
        .unwrap();
        rice.set(
            "colors.panel",
            sue_rice::RiceValue::Color("#202020".parse().unwrap()),
        )
        .unwrap();
        let synced = Theme::from_rice(&rice);
        assert_eq!(
            synced.for_studio(&rice, "sue_paint", &presets).panel,
            Color32::from_gray(0x20)
        );
        // Custom reads the studio's own section.
        rice.set(
            "sue_paint.theme",
            sue_rice::RiceValue::Text(STUDIO_THEME_CUSTOM.to_owned()),
        )
        .unwrap();
        rice.set(
            "sue_paint_colors.panel",
            sue_rice::RiceValue::Color("#123456".parse().unwrap()),
        )
        .unwrap();
        let custom = synced.for_studio(&rice, "sue_paint", &presets);
        assert_eq!(custom.panel, Color32::from_rgb(0x12, 0x34, 0x56));
        assert_eq!(
            custom.text,
            Color32::from_gray(0xe6),
            "the rest of the custom colors"
        );
    }

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

    #[test]
    fn light_presets_make_light_visuals() {
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        let sue = Theme::from_rice(&rice);
        assert!(!sue.is_light() && sue.visuals(None).dark_mode);
        let presets = sue_rice::builtin_presets();
        let paper = presets.iter().find(|preset| preset.id == "paper").unwrap();
        let paper = sue.with_preset(paper);
        let visuals = paper.visuals(None);
        assert!(paper.is_light() && !visuals.dark_mode);
        // Its own colors still win over egui's light ones.
        assert_eq!(visuals.widgets.inactive.fg_stroke.color, paper.text);
        assert_eq!(visuals.window_fill, paper.panel);
    }
}
