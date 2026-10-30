//! Studio Mode, Sue Paint's fuller touch layout on phones and tablets,
//! closer to the desktop: a tool column at the left (the menu, swap with
//! the eraser, the pen, the lasso, the object tool, then size and opacity
//! on vertical sliders and the color), a row of palettes and a command bar
//! along the bottom, a fullscreen toggle, and a modifier key bar that
//! slides out of the right edge: Esc, Ctrl, Shift, Alt, Space and seven
//! keys of your own (T1 to T7, commands kept in the rice).

use egui::{Align2, Color32, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use sue_wm::start::{self as page, Glyph};
use sue_wm::{AppletContext, ShellRequest, Theme};

use crate::icons::Icon;
use crate::panels::{icon_button, tool_icon};
use crate::phone::Sheet;
use crate::studio::SuePaint;
use crate::tools::Tool;

/// A thumb-sized button.
const BUTTON: f32 = 44.0;
/// The tool column's width.
const COLUMN: f32 = 52.0;
/// How many keys of their own people have.
pub const CUSTOM_KEYS: usize = 7;
/// The commands T1 to T7 run at first.
pub const CUSTOM_DEFAULTS: [&str; CUSTOM_KEYS] = [
    "edit.undo",
    "edit.redo",
    "select.all",
    "select.none",
    "layer.new",
    "view.fit",
    "view.flip_h",
];

/// The rice key of custom key `index` (0 is T1).
#[must_use]
pub fn custom_key(index: usize) -> String {
    format!("sue_paint.custom_key_{}", index + 1)
}

/// Studio Mode's state between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Touch {
    /// Only the canvas shows.
    pub(crate) fullscreen: bool,
    /// The modifier key bar is out.
    pub(crate) keys: bool,
    /// The modifier keys latched on, as if held.
    pub(crate) ctrl: bool,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    pub(crate) space: bool,
}

impl Touch {
    /// The latched modifiers, to add to the keyboard's.
    #[must_use]
    pub(crate) fn modifiers(&self) -> egui::Modifiers {
        egui::Modifiers {
            alt: self.alt,
            ctrl: self.ctrl,
            shift: self.shift,
            mac_cmd: false,
            command: self.ctrl,
        }
    }

    /// Lets go of every latched key.
    pub(crate) fn release(&mut self) {
        self.ctrl = false;
        self.shift = false;
        self.alt = false;
        self.space = false;
    }
}

impl SuePaint {
    /// Studio Mode's row in Sue's top bar: the picture's name and the
    /// fullscreen toggle.
    pub(crate) fn studio_bar(
        &mut self,
        ui: &mut Ui,
        bar: Rect,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) {
        let side = BUTTON.min(bar.height());
        let at =
            |x: f32| Rect::from_min_size(pos2(x, bar.center().y - side / 2.0), Vec2::splat(side));
        let name = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map(|entity| entity.name().to_owned())
            .unwrap_or_default();
        ui.painter().with_clip_rect(bar).text(
            pos2(bar.left() + 12.0, bar.center().y),
            Align2::LEFT_CENTER,
            name,
            theme.font(theme.ui_size),
            theme.text,
        );
        let hint = if self.touch.fullscreen {
            tr!("studio-leave-fullscreen")
        } else {
            tr!("studio-fullscreen")
        };
        if page::icon_button(
            ui,
            at(bar.right() - side - 2.0),
            Glyph::External,
            &hint,
            self.touch.fullscreen,
            theme,
        )
        .clicked()
        {
            self.touch.fullscreen = !self.touch.fullscreen;
            self.sheet = None;
        }
    }

    /// Studio Mode's page.
    pub(crate) fn studio_frame(
        &mut self,
        ui: &mut Ui,
        full: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut run: Option<String> = None;
        if self.touch.fullscreen {
            self.canvas_area(ui, full, cx, theme);
            self.key_bar(ui, full, theme, &mut run);
            if let Some(command) = run {
                self.run(&command, cx);
            }
            return;
        }
        let rows = BUTTON * 2.0 + 8.0;
        let column = Rect::from_min_max(full.min, pos2(full.left() + COLUMN, full.bottom() - rows));
        let canvas = Rect::from_min_max(
            pos2(column.right(), full.top()),
            pos2(full.right(), column.bottom()),
        );
        self.canvas_area(ui, canvas, cx, theme);
        self.tool_column(ui, column, theme);

        // The palettes, then the command bar.
        let palettes = Rect::from_min_max(
            pos2(full.left(), column.bottom()),
            pos2(full.right(), column.bottom() + BUTTON + 4.0),
        );
        let commands = Rect::from_min_max(pos2(full.left(), palettes.bottom()), full.max);
        for band in [palettes, commands] {
            ui.painter().rect_filled(band, 0.0, theme.panel);
            ui.painter()
                .hline(band.x_range(), band.top(), theme.hairline());
        }
        let slot = |band: Rect, index: usize| {
            Rect::from_min_size(
                pos2(
                    band.left() + 4.0 + index as f32 * (BUTTON + 2.0),
                    band.center().y - BUTTON / 2.0,
                ),
                Vec2::splat(BUTTON),
            )
        };
        let sheets = [
            (Icon::Navigator, Sheet::Navigator, tr!("navigator")),
            (Icon::Layers, Sheet::Layers, tr!("layers-3")),
            (
                Icon::Edit,
                Sheet::LayerProperty,
                tr!("studio-layer-property"),
            ),
            (Icon::Wrench, Sheet::ToolSettings, tr!("tool-settings")),
            (Icon::Wheel, Sheet::Color, tr!("color-2")),
            (Icon::Paper, Sheet::Materials, tr!("start-manage-materials")),
            (Icon::Play, Sheet::Timeline, tr!("timeline")),
            (Icon::Reference, Sheet::Reference, tr!("simple-reference")),
        ];
        let fits = ((palettes.width() - 8.0) / (BUTTON + 2.0)).floor().max(1.0) as usize;
        for (index, (icon, sheet, tip)) in sheets.iter().enumerate().take(fits) {
            if icon_button(
                ui,
                slot(palettes, index),
                *icon,
                true,
                self.sheet == Some(*sheet),
                tip,
                theme,
            ) {
                self.toggle_sheet(*sheet);
            }
        }
        let back = slot(commands, 0);
        if page::icon_button(ui, back, Glyph::Back, &tr!("start-back"), false, theme).clicked() {
            self.start.forced = true;
            self.sheet = None;
        }
        let history = (cx.history.can_undo(), cx.history.can_redo());
        let picture = self.picture().is_some();
        let buttons = [
            (Icon::Save, "file.save", tr!("save-project"), true),
            (
                Icon::Undo,
                "edit.undo",
                tr!("undo-two-finger-tap"),
                history.0,
            ),
            (
                Icon::Redo,
                "edit.redo",
                tr!("redo-three-finger-tap"),
                history.1,
            ),
            (
                Icon::Operation,
                "edit.transform.scale",
                tr!("scale-rotate-ellipsis"),
                picture,
            ),
            (Icon::Select, "select.all", tr!("all"), picture),
            (Icon::Duplicate, "edit.copy", tr!("copy-2"), picture),
            (Icon::Clear, "edit.cut", tr!("cut"), picture),
            (Icon::Import, "edit.paste", tr!("paste"), picture),
        ];
        let fits = ((commands.width() - 8.0) / (BUTTON + 2.0)).floor().max(2.0) as usize - 1;
        for (index, (icon, command, tip, enabled)) in buttons.iter().enumerate().take(fits) {
            if icon_button(
                ui,
                slot(commands, index + 1),
                *icon,
                *enabled,
                false,
                tip,
                theme,
            ) {
                run = Some((*command).to_owned());
            }
        }
        self.key_bar(ui, canvas, theme, &mut run);
        if let Some(sheet) = self.sheet {
            self.phone_sheet(ui, canvas, sheet, cx, theme, &mut run);
        }
        if let Some(command) = run {
            self.run(&command, cx);
        }
    }

    /// The tool column: the menu, swap, pen, lasso, object, then size and
    /// opacity on vertical sliders, and the color at the bottom.
    fn tool_column(&mut self, ui: &mut Ui, column: Rect, theme: &Theme) {
        ui.painter().rect_filled(column, 0.0, theme.panel);
        ui.painter()
            .vline(column.right(), column.y_range(), theme.hairline());
        let x = column.center().x;
        let mut y = column.top() + 4.0;
        let mut next = |height: f32| {
            let rect = Rect::from_min_size(pos2(x - BUTTON / 2.0, y), vec2(BUTTON, height));
            y += height + 2.0;
            rect
        };
        let tool = self.toolbox.tool();
        let pen = if tool.paints() {
            tool
        } else {
            self.drawing_tool
        };
        if icon_button(
            ui,
            next(BUTTON),
            Icon::Menu,
            true,
            self.sheet == Some(Sheet::Menu),
            &tr!("menus-2"),
            theme,
        ) {
            self.toggle_sheet(Sheet::Menu);
        }
        if icon_button(
            ui,
            next(BUTTON),
            Icon::Swap,
            true,
            false,
            &tr!("simple-swap"),
            theme,
        ) {
            if tool == Tool::Eraser {
                self.toolbox.pick(self.drawing_tool);
            } else {
                if tool.paints() {
                    self.drawing_tool = tool;
                }
                self.toolbox.pick(Tool::Eraser);
            }
        }
        if icon_button(
            ui,
            next(BUTTON),
            tool_icon(pen),
            true,
            tool == pen,
            &tr!("simple-pens"),
            theme,
        ) {
            if tool == pen {
                // Every tool, as on the desktop.
                self.toggle_sheet(Sheet::Tools);
            } else {
                self.toolbox.pick(pen);
            }
        }
        for (picked, tip) in [
            (Tool::Select, tr!("studio-lasso")),
            (Tool::Operation, tr!("studio-object")),
        ] {
            if icon_button(
                ui,
                next(BUTTON),
                tool_icon(picked),
                picked.available(),
                tool == picked,
                &tip,
                theme,
            ) {
                self.toolbox.pick(picked);
            }
        }
        // Size and opacity: a badge with the number over a vertical slider,
        // sharing what is left above the color.
        let color_side = BUTTON;
        // Five buttons above.
        let used = 4.0 + 5.0 * (BUTTON + 2.0);
        let left = (column.height() - used - color_side - 8.0).max(120.0);
        let each = left / 2.0;
        let tip = &mut self.toolbox.current_mut().tip;
        let mut opacity = tip.opacity * 100.0;
        for (index, (text, value, range, log)) in [
            (
                format!("{:.1}\n{}", tip.size, tr!("px")),
                &mut tip.size,
                0.5..=1000.0,
                true,
            ),
            (format!("{opacity:.0}\n%"), &mut opacity, 0.0..=100.0, false),
        ]
        .into_iter()
        .enumerate()
        {
            let badge = next(36.0);
            ui.painter().circle_filled(badge.center(), 18.0, theme.void);
            ui.painter()
                .circle_stroke(badge.center(), 18.0, theme.hairline());
            ui.painter().text(
                badge.center(),
                Align2::CENTER_CENTER,
                text,
                theme.font(theme.small_size - 1.0),
                theme.text,
            );
            let track = next((each - 38.0).max(60.0));
            let mut slider = ui.new_child(
                UiBuilder::new()
                    .max_rect(track)
                    .id_salt(("sue-paint-studio-slider", index)),
            );
            slider.spacing_mut().slider_width = track.height() - 8.0;
            slider.add(
                egui::Slider::new(value, range)
                    .vertical()
                    .logarithmic(log)
                    .show_value(false),
            );
        }
        tip.opacity = opacity / 100.0;
        let color = Rect::from_center_size(
            pos2(x, column.bottom() - color_side / 2.0 - 4.0),
            Vec2::splat(color_side),
        );
        let [r, g, b] = self.main;
        ui.painter()
            .circle_filled(color.center(), 17.0, Color32::from_rgb(r, g, b));
        ui.painter()
            .circle_stroke(color.center(), 18.0, theme.hairline_active());
        if ui
            .interact(
                color,
                ui.scope_id().with("sue-paint-studio-color"),
                Sense::click(),
            )
            .on_hover_text(tr!("color-2"))
            .clicked()
        {
            self.toggle_sheet(Sheet::Color);
        }
    }

    /// The tab at the right edge of `area` and, when out, the modifier key
    /// bar: Esc, Ctrl, Shift, Alt, Space, then T1 to T7.
    fn key_bar(&mut self, ui: &mut Ui, area: Rect, theme: &Theme, run: &mut Option<String>) {
        let width = 64.0;
        let open = self.touch.keys;
        let edge = if open {
            area.right() - width
        } else {
            area.right()
        };
        let tab = Rect::from_min_size(pos2(edge - 22.0, area.center().y - 28.0), vec2(22.0, 56.0));
        ui.painter().rect_filled(tab, 0.0, theme.panel);
        ui.painter()
            .rect_stroke(tab, 0.0, theme.hairline(), egui::StrokeKind::Inside);
        page::glyph(
            ui.painter(),
            tab.shrink(4.0),
            if open { Glyph::Chevron } else { Glyph::Back },
            theme.text,
        );
        if ui
            .interact(tab, ui.scope_id().with("sue-paint-key-tab"), Sense::click())
            .on_hover_text(tr!("studio-modifier-keys"))
            .clicked()
        {
            self.touch.keys = !open;
        }
        if !open {
            return;
        }
        let bar = Rect::from_min_max(pos2(edge, area.top()), area.max);
        ui.painter().rect_filled(bar, 0.0, theme.panel);
        ui.painter()
            .vline(bar.left(), bar.y_range(), theme.hairline());
        let mut keys: Vec<(String, u8)> = vec![
            (tr!("studio-key-esc"), 0),
            (tr!("studio-key-ctrl"), 1),
            (tr!("studio-key-shift"), 2),
            (tr!("studio-key-alt"), 3),
            (tr!("studio-key-space"), 4),
        ];
        keys.extend((0..CUSTOM_KEYS).map(|index| (format!("T{}", index + 1), 5 + index as u8)));
        let height = ((bar.height() - 4.0) / keys.len() as f32).clamp(28.0, 52.0);
        for (index, (label, key)) in keys.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(bar.left() + 6.0, bar.top() + 2.0 + index as f32 * height),
                vec2(width - 12.0, height - 4.0),
            );
            if rect.bottom() > bar.bottom() {
                break;
            }
            let on = match key {
                1 => self.touch.ctrl,
                2 => self.touch.shift,
                3 => self.touch.alt,
                4 => self.touch.space,
                _ => false,
            };
            let response = ui.interact(
                rect,
                ui.scope_id().with(("sue-paint-key", *key)),
                Sense::click(),
            );
            let fill = if on {
                theme.active
            } else if response.hovered() {
                theme.hover
            } else {
                theme.pill
            };
            ui.painter().rect(
                rect,
                0.0,
                fill,
                if on {
                    theme.hairline_active()
                } else {
                    theme.hairline()
                },
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                theme.font(theme.small_size + 1.0),
                theme.text,
            );
            if response.clicked() {
                match key {
                    0 => {
                        self.touch.release();
                        self.sheet = None;
                        *run = Some("select.none".to_owned());
                    }
                    1 => self.touch.ctrl = !self.touch.ctrl,
                    2 => self.touch.shift = !self.touch.shift,
                    3 => self.touch.alt = !self.touch.alt,
                    4 => self.touch.space = !self.touch.space,
                    custom => *run = Some(format!("touch.custom.{}", custom - 5)),
                }
            }
        }
    }

    /// The command custom key `index` runs, as the rice has it.
    pub(crate) fn custom_command(&self, index: usize, cx: &AppletContext<'_>) -> String {
        cx.rice()
            .get_str(&custom_key(index))
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .unwrap_or(CUSTOM_DEFAULTS.get(index).copied().unwrap_or(""))
            .to_owned()
    }

    /// Studio Mode's Menu sheet: home, Simple Mode and Settings, then Sue
    /// Paint's menus.
    pub(crate) fn studio_menu(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        use sue_wm::start::{Row, Trailing};
        let rows = [
            Row::new(Some(Glyph::Back), tr!("studio-return-home"), Trailing::None),
            Row::new(
                Some(Glyph::Transfers),
                tr!("studio-to-simple"),
                Trailing::None,
            ),
            Row::new(
                Some(Glyph::Settings),
                tr!("studio-app-settings"),
                Trailing::More,
            ),
        ];
        match page::group(ui, &rows, theme) {
            Some(0) => {
                self.start.forced = true;
                self.sheet = None;
            }
            Some(1) => {
                crate::simple::set_mode(cx, false);
                self.sheet = None;
            }
            Some(2) => {
                cx.request(ShellRequest::OpenSettings(Some("studios".to_owned())));
                self.sheet = None;
            }
            _ => {}
        }
        ui.add_space(12.0);
        self.menu_list(ui, cx, theme, run);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latched_keys_count_as_held_until_let_go() {
        let mut touch = Touch {
            ctrl: true,
            shift: true,
            space: true,
            ..Touch::default()
        };
        let held = touch.modifiers();
        assert!(held.ctrl && held.command && held.shift && !held.alt);
        touch.release();
        assert_eq!(touch.modifiers(), egui::Modifiers::NONE);
        assert!(!touch.space);
        assert_eq!(custom_key(0), "sue_paint.custom_key_1");
        assert_eq!(CUSTOM_DEFAULTS.len(), CUSTOM_KEYS);
    }
}
